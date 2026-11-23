//! Agents' own tools: reaching the owner (`notify_owner`), messaging another
//! agent (`message_agent`, `list_agents`) and editing their own memory
//! (`remember`, `set_memory`).
//!
//! One implementation for every engine: yardd is also a small stdio MCP
//! server (`yardd agent-tools`). A user-made agent's conversation hands its
//! engine that server (Claude: the SDK's `mcpServers`; Codex: a thread-local
//! `mcp_servers.switchyard` override; ACP: `mcpServers` on `session/new`,
//! `session/resume` and `session/load`), with a key that names that one
//! conversation. The server answers `initialize` itself and passes
//! `tools/list` and `tools/call` to the running yardd over loopback
//! (`POST /agent-tools`, the key as a bearer token), so the tools act on the
//! daemon's state and the key decides who is calling. Scout, private threads
//! and the owner's own sessions are never given the server, and a key is
//! only ever made for an agent's conversation.

use std::{
    collections::{HashMap, VecDeque},
    net::SocketAddr,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use crate::{
    host_context::clip,
    origins,
    state::{now_ms, App},
};

/// The MCP server's name, as engines show it (`mcp__switchyard__remember`).
pub const SERVER: &str = "switchyard";
/// How often one agent may reach the owner, and message other agents, per hour.
pub const NOTIFY_PER_HOUR: usize = 3;
pub const MESSAGES_PER_HOUR: usize = 30;
const HOUR_MS: u64 = 3_600_000;
const MESSAGE_BYTES: usize = 20_000;

#[derive(Default)]
pub struct Tools {
    /// Where the stdio server reaches yardd: set by main when yardd listens
    /// on loopback. Unset, no engine is offered the tools.
    url: Mutex<Option<String>>,
    /// Key -> the agent conversation it was made for. One per conversation
    /// for this yardd's life; a restart makes new ones.
    keys: Mutex<HashMap<String, String>>,
    /// `<agent id>:<tool>` -> when it was used in the last hour.
    used: Mutex<HashMap<String, VecDeque<u64>>>,
}

impl Tools {
    pub fn set_url(&self, url: Option<String>) {
        *self.url.lock().unwrap() = url;
    }

    pub fn forget(&self, sid: &str) {
        self.keys.lock().unwrap().retain(|_, s| s != sid);
    }

    fn key_for(&self, sid: &str) -> String {
        let mut keys = self.keys.lock().unwrap();
        if let Some((key, _)) = keys.iter().find(|(_, s)| s.as_str() == sid) {
            return key.clone();
        }
        let key = random_key();
        keys.insert(key.clone(), sid.to_string());
        key
    }

    fn session(&self, key: &str) -> Option<String> {
        self.keys.lock().unwrap().get(key).cloned()
    }

    /// Count a use against the hourly cap; refused with when the oldest use
    /// leaves the window.
    fn take(&self, agent: &str, tool: &str, max: usize, now: u64) -> std::result::Result<(), u64> {
        let mut used = self.used.lock().unwrap();
        let times = used.entry(format!("{agent}:{tool}")).or_default();
        while times.front().is_some_and(|t| now.saturating_sub(*t) >= HOUR_MS) {
            times.pop_front();
        }
        if times.len() >= max {
            return Err(times.front().copied().unwrap_or(now) + HOUR_MS);
        }
        times.push_back(now);
        Ok(())
    }
}

/// 32 random bytes as hex, from the kernel.
fn random_key() -> String {
    let mut bytes = [0u8; 32];
    let read = std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes));
    if read.is_err() {
        // No /dev/urandom: hash what differs per process and call (never seen on macOS or Linux).
        use std::hash::{BuildHasher, Hasher};
        for (i, chunk) in bytes.chunks_mut(8).enumerate() {
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(now_ms() ^ (i as u64) ^ u64::from(std::process::id()));
            chunk.copy_from_slice(&h.finish().to_le_bytes());
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The agent a conversation belongs to, if it is a user-made agent's.
fn agent_of(app: &App, sid: &str) -> Option<crate::agents::Agent> {
    if crate::private::internal(sid) {
        return None;
    }
    let id = app.meta(sid)?.agent?;
    app.agents.get(&id).filter(|a| !a.builtin)
}

/// Whether the session's engine gets the tools: a user-made agent's
/// conversation, while yardd listens on loopback.
pub fn offered(app: &App, sid: &str) -> bool {
    app.tools.url.lock().unwrap().is_some() && agent_of(app, sid).is_some()
}

/// How an engine starts the tools' MCP server for one conversation.
pub struct Launch {
    pub command: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Launch {
    /// The Agent SDK's `mcpServers` entry.
    pub fn claude(&self) -> Value {
        json!({ "type": "stdio", "command": self.command, "args": self.args, "env": self.env_map() })
    }

    /// Codex's `mcp_servers.<name>` table, as a thread-local config override.
    pub fn codex(&self) -> Value {
        json!({ "command": self.command, "args": self.args, "env": self.env_map() })
    }

    /// ACP's `McpServerStdio`.
    pub fn acp(&self) -> Value {
        let env: Vec<Value> = self.env.iter().map(|(k, v)| json!({ "name": k, "value": v })).collect();
        json!({ "name": SERVER, "command": self.command, "args": self.args, "env": env })
    }

    fn env_map(&self) -> Value {
        json!(self.env.iter().cloned().collect::<HashMap<_, _>>())
    }
}

/// The MCP server a session's engine launches, or none (see `offered`).
pub fn launch(app: &App, sid: &str) -> Option<Launch> {
    let url = app.tools.url.lock().unwrap().clone()?;
    agent_of(app, sid)?;
    let command = std::env::current_exe().ok()?.to_string_lossy().into_owned();
    Some(Launch {
        command,
        args: vec!["agent-tools".into()],
        env: vec![("YARD_TOOLS_URL".into(), url), ("YARD_TOOLS_KEY".into(), app.tools.key_for(sid))],
    })
}

pub const TOOLS: [&str; 5] = ["notify_owner", "message_agent", "list_agents", "remember", "set_memory"];

/// One of our tools, from the name an engine shows for an MCP call: Claude's
/// `mcp__switchyard__remember`, OpenCode's `switchyard_remember`, or any other
/// spelling that names the server and ends with the tool.
pub fn own_tool(shown: &str) -> Option<&'static str> {
    let shown = shown.trim().to_ascii_lowercase();
    if !shown.contains(SERVER) {
        return None;
    }
    TOOLS.into_iter().find(|t| shown.ends_with(t) && !shown[..shown.len() - t.len()].ends_with(|c: char| c.is_ascii_alphanumeric()))
}

/// How a call reads in the thread: a title and one line of detail.
pub fn label(tool: &str, args: &Value) -> Option<(String, String)> {
    let s = |k: &str| args[k].as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
    Some(match tool {
        "notify_owner" => ("Asked for you".into(), s("message")),
        "message_agent" => (format!("Messaged @{}", s("to").trim_start_matches('@')), s("text")),
        "list_agents" => ("Listed agents".into(), String::new()),
        "remember" => ("Remembered".into(), s("text")),
        "set_memory" => ("Updated memory".into(), s("text")),
        _ => return None,
    })
}

/// The tools, described for the calling agent: the agents it can message
/// are named in `message_agent`'s description as they are now.
fn definitions(app: &App, me: &crate::agents::Agent) -> Value {
    let others = others(app, me);
    let names = if others.is_empty() {
        "There are no other agents yet.".to_string()
    } else {
        format!("Agents: {}.", others.iter().map(|a| format!("@{}", a.name)).collect::<Vec<_>>().join(", "))
    };
    let text = |d: &str| json!({ "type": "string", "description": d });
    json!([
        {
            "name": "notify_owner",
            "description": format!("Reach the user who set you up, when you need them: a decision, something only they can do, or a result they asked to hear about. Shows a notification in their open Switchyard windows and marks you as needing them until they open this conversation. At most {NOTIFY_PER_HOUR} per hour, so use it only when it matters; otherwise just answer."),
            "inputSchema": { "type": "object", "properties": { "message": text("What you need from them, in one or two sentences.") }, "required": ["message"] },
            "annotations": { "title": "Needs you", "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "message_agent",
            "description": format!("Send a message to another agent in Switchyard, by name. It goes into that agent's conversation; when the agent's turn that answers it ends, its final answer comes back to you as a new message. Do not wait or poll for it. Chains of agents messaging agents stop after {} hops. {names}", origins::MAX_HOPS),
            "inputSchema": { "type": "object", "properties": { "to": text("The agent's name, such as Reviewer."), "text": text("The message.") }, "required": ["to", "text"] },
            "annotations": { "title": "Message an agent", "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "list_agents",
            "description": "List the other agents you can message, with what each one is for.",
            "inputSchema": { "type": "object", "properties": {} },
            "annotations": { "title": "List agents", "readOnlyHint": true, "openWorldHint": false },
        },
        {
            "name": "remember",
            "description": "Add one line to your memory: what to keep in mind from now on. The user sees your memory in your settings and can edit it. Answers your whole memory.",
            "inputSchema": { "type": "object", "properties": { "text": text("The line to add.") }, "required": ["text"] },
            "annotations": { "title": "Remember", "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false },
        },
        {
            "name": "set_memory",
            "description": "Replace your whole memory with this text, to tidy it or drop what no longer holds. At most 20,000 characters. Empty text clears it.",
            "inputSchema": { "type": "object", "properties": { "text": text("Your memory, all of it.") }, "required": ["text"] },
            "annotations": { "title": "Set memory", "readOnlyHint": false, "destructiveHint": true, "openWorldHint": false },
        },
    ])
}

fn others(app: &App, me: &crate::agents::Agent) -> Vec<crate::agents::Agent> {
    serde_json::from_value::<Vec<crate::agents::Agent>>(app.agents.list()).unwrap_or_default().into_iter().filter(|a| a.id != me.id).collect()
}

fn at_clock(ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(ms as i64).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default()
}

/// Run one tool for the agent whose conversation `sid` is. Answers the text
/// the model reads and whether it is an error.
pub async fn call(app: &Arc<App>, sid: &str, tool: &str, args: &Value) -> (String, bool) {
    match run(app, sid, tool, args).await {
        Ok(text) => (text, false),
        Err(e) => (format!("{e:#}"), true),
    }
}

async fn run(app: &Arc<App>, sid: &str, tool: &str, args: &Value) -> Result<String> {
    let me = agent_of(app, sid).context("This conversation no longer belongs to an agent.")?;
    let arg = |k: &str| args[k].as_str().unwrap_or("").trim().to_string();
    match tool {
        "notify_owner" => {
            let message = arg("message");
            if message.is_empty() {
                bail!("Not sent: the message is empty.");
            }
            if let Err(until) = app.tools.take(&me.id, tool, NOTIFY_PER_HOUR, now_ms()) {
                bail!("Not sent: you already reached the user {NOTIFY_PER_HOUR} times in the last hour. Try again after {}, or put it in your answer.", at_clock(until));
            }
            let message = clip(&message, 400);
            app.update_meta(sid, |m| m.needs_you = Some(json!({ "message": message, "at": now_ms() })));
            let windows = crate::ws::notify(app, &json!({ "title": format!("{} needs you", me.name), "body": message, "sessionId": sid }))?;
            Ok(if windows == 0 {
                "No Switchyard window is open right now. Your card shows that you need the user until they open this conversation.".into()
            } else {
                format!("Sent to the user's open Switchyard windows ({windows}). Your card shows that you need them until they open this conversation.")
            })
        }
        "message_agent" => {
            let to = arg("to").trim_start_matches('@').to_string();
            let text = arg("text");
            if text.is_empty() {
                bail!("Not sent: the message is empty.");
            }
            let others = others(app, &me);
            let Some(target) = others.iter().find(|a| a.name.eq_ignore_ascii_case(&to)).cloned() else {
                if me.name.eq_ignore_ascii_case(&to) {
                    bail!("Not sent: that is you. message_agent sends to another agent.");
                }
                let names = others.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ");
                bail!("Not sent: no agent is called {to}. {}", if names.is_empty() { "There are no other agents.".to_string() } else { format!("Agents: {names}.") });
            };
            let hops = origins::next_hops(&app.meta(sid).map(|m| m.answering).unwrap_or_default());
            if hops > origins::MAX_HOPS {
                bail!(
                    "Not sent: this would be hop {hops} of a chain of agents messaging agents, and at most {} are allowed. Answer with what you have instead.",
                    origins::MAX_HOPS
                );
            }
            if let Err(until) = app.tools.take(&me.id, tool, MESSAGES_PER_HOUR, now_ms()) {
                bail!("Not sent: you already sent {MESSAGES_PER_HOUR} messages to agents in the last hour. Try again after {}.", at_clock(until));
            }
            let origin = json!({ "kind": "agent", "agentId": me.id, "name": me.name, "hops": hops });
            let text = clip(&text, MESSAGE_BYTES);
            // Delivery may start the target's engine: the tool answers now,
            // and a message that cannot be delivered comes back as a reply.
            let (app2, target2) = (app.clone(), target.clone());
            tokio::spawn(async move {
                if let Err(e) = crate::agents::deliver(&app2, &target2.id, &text, &origin).await {
                    let back = json!({ "kind": "agentReply", "agentId": target2.id, "name": target2.name, "hops": hops });
                    let note = origins::answer_text(&target2.name, "undelivered", &format!("{e:#}"));
                    if let Err(e) = crate::agents::deliver(&app2, &me.id, &note, &back).await {
                        eprintln!("yardd: could not tell {} its message failed: {e:#}", me.name);
                    }
                }
            });
            Ok(format!(
                "Sent to @{} (hop {hops} of {}). Its answer will arrive in this conversation as a new message when it is done; you do not need to wait for it.",
                target.name,
                origins::MAX_HOPS
            ))
        }
        "list_agents" => {
            let list = others(app, &me);
            if list.is_empty() {
                return Ok("There are no other agents yet.".into());
            }
            Ok(list
                .iter()
                .map(|a| {
                    let built = if a.builtin { " (built in)" } else { "" };
                    format!("@{}{built}{}", a.name, if a.about.is_empty() { String::new() } else { format!(": {}", a.about) })
                })
                .collect::<Vec<_>>()
                .join("\n"))
        }
        "remember" | "set_memory" => {
            let memory = crate::agents::edit_memory(app, &me.id, &arg("text"), tool == "remember")?;
            Ok(if memory.is_empty() { "Your memory is now empty.".into() } else { format!("Saved. Your memory now:\n{memory}") })
        }
        other => bail!("Switchyard has no tool {other}."),
    }
}

/// `POST /agent-tools`: the stdio server's calls. Only from this machine, and
/// only with a key yardd made for an agent's conversation.
pub async fn http(State(app): State<Arc<App>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, body: Bytes) -> Response {
    let refuse = |code: StatusCode, why: &str| (code, Json(json!({ "error": why }))).into_response();
    if !peer.ip().is_loopback() || headers.contains_key("origin") {
        return refuse(StatusCode::FORBIDDEN, "agent tools answer only local processes");
    }
    let key = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    let Some(sid) = (!key.is_empty()).then(|| app.tools.session(key)).flatten() else {
        return refuse(StatusCode::UNAUTHORIZED, "unknown key");
    };
    let Ok(req) = serde_json::from_slice::<Value>(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "expected JSON");
    };
    let Some(me) = agent_of(&app, &sid) else {
        return refuse(StatusCode::UNAUTHORIZED, "this conversation no longer belongs to an agent");
    };
    match req["method"].as_str() {
        Some("tools/list") => Json(json!({ "tools": definitions(&app, &me) })).into_response(),
        Some("tools/call") => {
            let p = &req["params"];
            let tool = p["name"].as_str().unwrap_or("");
            let (text, error) = call(&app, &sid, tool, &p["arguments"]).await;
            Json(json!({ "content": [{ "type": "text", "text": text }], "isError": error })).into_response()
        }
        _ => refuse(StatusCode::BAD_REQUEST, "unknown method"),
    }
}

/// `yardd agent-tools`: the stdio MCP server an agent's engine launches.
/// Answers `initialize` and `ping` itself and passes tool calls to yardd
/// (`YARD_TOOLS_URL`, `YARD_TOOLS_KEY`).
pub async fn serve_stdio() -> Result<()> {
    let url = std::env::var("YARD_TOOLS_URL").context("YARD_TOOLS_URL is not set")?;
    let key = std::env::var("YARD_TOOLS_KEY").context("YARD_TOOLS_KEY is not set")?;
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut out = tokio::io::stdout();
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        // Notifications (`notifications/initialized`) want no answer.
        let Some(id) = msg.get("id").cloned() else { continue };
        let method = msg["method"].as_str().unwrap_or("");
        let answer = match method {
            "initialize" => json!({ "result": {
                "protocolVersion": msg["params"]["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER, "version": env!("CARGO_PKG_VERSION") },
            } }),
            "ping" => json!({ "result": {} }),
            "tools/list" | "tools/call" => match post(&url, &key, &json!({ "method": method, "params": msg["params"] })).await {
                Ok(result) => json!({ "result": result }),
                // A model reads a failed call; it cannot read a protocol error.
                Err(e) if method == "tools/call" => {
                    json!({ "result": { "content": [{ "type": "text", "text": format!("Switchyard did not answer: {e:#}") }], "isError": true } })
                }
                Err(e) => json!({ "error": { "code": -32603, "message": format!("Switchyard did not answer: {e:#}") } }),
            },
            _ => json!({ "error": { "code": -32601, "message": format!("unknown method {method}") } }),
        };
        let mut reply = json!({ "jsonrpc": "2.0", "id": id });
        if let (Some(r), Some(a)) = (reply.as_object_mut(), answer.as_object()) {
            r.extend(a.clone());
        }
        out.write_all(format!("{reply}\n").as_bytes()).await?;
        out.flush().await?;
    }
    Ok(())
}

/// One JSON POST to yardd over loopback: plain HTTP/1.1, so the server mode
/// needs no HTTP client.
async fn post(url: &str, key: &str, body: &Value) -> Result<Value> {
    let rest = url.strip_prefix("http://").context("YARD_TOOLS_URL must be an http:// address")?;
    let (host, path) = match rest.split_once('/') {
        Some((h, p)) => (h, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let body = body.to_string();
    let exchange = async {
        let mut stream = tokio::net::TcpStream::connect(host).await?;
        let request = format!(
            "POST {path} HTTP/1.1\r\nHost: {host}\r\nAuthorization: Bearer {key}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await?;
        anyhow::Ok(raw)
    };
    let raw = tokio::time::timeout(Duration::from_secs(60), exchange).await.map_err(|_| anyhow::anyhow!("no answer within 60 s"))??;
    let text = String::from_utf8_lossy(&raw);
    let (head, rest) = text.split_once("\r\n\r\n").context("a malformed answer")?;
    let status = head.split_whitespace().nth(1).unwrap_or("");
    let payload = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { dechunk(rest) } else { rest.to_string() };
    let v: Value = serde_json::from_str(payload.trim()).with_context(|| format!("HTTP {status}"))?;
    if status != "200" {
        bail!("{}", v["error"].as_str().unwrap_or(status));
    }
    Ok(v)
}

fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(size.trim(), 16).unwrap_or(0);
        if n == 0 || after.len() < n {
            break;
        }
        out.push_str(&after[..n]);
        rest = after[n..].trim_start_matches("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hourly_cap_refuses_until_the_oldest_use_leaves() {
        let t = Tools::default();
        for i in 0..3 {
            assert!(t.take("g1", "notify_owner", 3, 1000 + i).is_ok());
        }
        assert_eq!(t.take("g1", "notify_owner", 3, 2000), Err(1000 + HOUR_MS));
        // Another agent, or another tool, has its own count.
        assert!(t.take("g2", "notify_owner", 3, 2000).is_ok());
        assert!(t.take("g1", "message_agent", 3, 2000).is_ok());
        assert!(t.take("g1", "notify_owner", 3, 1000 + HOUR_MS).is_ok());
    }

    #[test]
    fn one_key_per_conversation_and_none_after_it_goes() {
        let t = Tools::default();
        let k = t.key_for("s1");
        assert_eq!(k.len(), 64);
        assert_eq!(t.key_for("s1"), k);
        assert_ne!(t.key_for("s2"), k);
        assert_eq!(t.session(&k).as_deref(), Some("s1"));
        t.forget("s1");
        assert_eq!(t.session(&k), None);
    }

    #[test]
    fn calls_read_as_steps() {
        assert_eq!(label("message_agent", &json!({ "to": "@Reviewer", "text": "look at\n#12" })), Some(("Messaged @Reviewer".into(), "look at #12".into())));
        assert_eq!(label("remember", &json!({ "text": "trunk is main" })).unwrap().0, "Remembered");
        assert_eq!(label("other", &json!({})), None);
        assert_eq!(own_tool("mcp__switchyard__remember"), Some("remember"));
        assert_eq!(own_tool("switchyard_message_agent"), Some("message_agent"));
        assert_eq!(own_tool("mcp__other__remember"), None);
        assert_eq!(own_tool("switchyard_unremember"), None);
    }

    #[test]
    fn a_chunked_body_is_joined() {
        assert_eq!(dechunk("4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n"), "{\"a\":1}");
    }
}
