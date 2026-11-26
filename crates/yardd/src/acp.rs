//! Engines that speak ACP (the Agent Client Protocol: JSON-RPC over stdio):
//! OpenCode (`opencode acp`), Grok (`grok agent stdio`) and Gemini
//! (`gemini --acp`). One agent process per engine and account, started on first use like Codex's app-server, holds
//! all of that account's sessions. Its `session/update` notifications are
//! normalized into session events here; `session/request_permission` becomes
//! an approval, answered by the user or by the session's permission mode.
//!
//! The ACP session id is the session's engine ref, so a session restored after
//! a restart, or one whose agent process was replaced, resumes with
//! `session/resume` (or `session/load`, whose replayed history is ignored).

use std::{
    collections::{HashMap, HashSet},
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine as _;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{mpsc, oneshot},
};

use crate::{
    attachments,
    state::{App, Backend},
};

/// Tells a session attached to an older agent process from one on the
/// current process, so it resumes before its next prompt.
static EPOCH: AtomicU64 = AtomicU64::new(1);

/// Tool output kept per call; a `cat` of a large file should not fill the log.
const MAX_OUTPUT: usize = 50_000;

/// What OpenCode asks before, on top of the user's own config. Without it
/// OpenCode runs commands and edits unasked, and Ask first would mean nothing;
/// the session's mode then decides what yardd answers itself (see `policy`).
pub const OPENCODE_PERMISSION: &str = r#"{"edit":"ask","bash":"ask","webfetch":"ask"}"#;

/// The command that runs an engine's ACP agent.
pub(crate) fn agent_command(engine: &str) -> Result<Command> {
    let mut cmd = match engine {
        "opencode" => {
            let mut cmd = Command::new("opencode");
            cmd.arg("acp").env("OPENCODE_PERMISSION", OPENCODE_PERMISSION);
            cmd
        }
        "grok" => {
            let mut cmd = Command::new("grok");
            cmd.args(["agent", "stdio"]);
            cmd
        }
        "gemini" => {
            let mut cmd = Command::new("gemini");
            // Override a saved yolo mode; yardd applies each session's choice.
            cmd.args(["--acp", "--approval-mode", "default"]).env("NO_BROWSER", "true");
            cmd
        }
        other => bail!("{other} does not speak ACP"),
    };
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    Ok(cmd)
}

struct Approval {
    /// The JSON-RPC id the agent waits on.
    id: Value,
    sid: String,
    options: Vec<Value>,
}

/// One session's open stream: the text and thought items being built, and
/// which tool calls were already announced.
#[derive(Default)]
struct Stream {
    text: Option<(String, String)>,
    thought: Option<(String, String)>,
    n: u64,
    started: HashSet<String>,
    done: HashSet<String>,
    diffs: HashSet<String>,
    /// Set while `session/load` replays history we already have.
    loading: bool,
}

pub struct Client {
    pub engine: String,
    pub account: String,
    pub epoch: u64,
    stop: Mutex<Option<oneshot::Sender<()>>>,
    tx: mpsc::UnboundedSender<String>,
    pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
    next: AtomicU64,
    /// ACP session id -> our session id
    sessions: Mutex<HashMap<String, String>>,
    /// our request id -> the agent's waiting permission request
    approvals: Mutex<HashMap<String, Approval>>,
    streams: Mutex<HashMap<String, Stream>>,
    /// ACP session id -> its latest `configOptions` and `modes`
    options: Mutex<HashMap<String, Value>>,
    /// What `initialize` answered: capabilities, auth methods, Grok's models.
    pub info: Mutex<Value>,
}

/// The key of an engine's account in `App::acp`.
pub fn key(engine: &str, account: &str) -> String {
    format!("{engine}/{account}")
}

impl Client {
    async fn start(app: Arc<App>, engine: String, account: String) -> Result<Arc<Client>> {
        let mut cmd = agent_command(&engine)?;
        if let Some(dir) = app.accounts.config_dir(&engine, &account)? {
            cmd.env(crate::accounts::env_var(&engine), dir);
        }
        let label = crate::engines::name(&engine);
        let mut child = cmd.spawn().with_context(|| format!("could not start {label}: is it installed here?"))?;
        if let Some(pid) = child.id() {
            app.guard.set_codex_server(&key(&engine, &account), pid);
        }
        let mut stdin = child.stdin.take().context("agent stdin")?;
        let stdout = child.stdout.take().context("agent stdout")?;
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.write_all(b"\n").await.is_err() {
                    break;
                }
                let _ = stdin.flush().await;
            }
        });
        let (stop, stopped) = oneshot::channel::<()>();
        let client = Arc::new(Client {
            engine: engine.clone(),
            account: account.clone(),
            epoch: EPOCH.fetch_add(1, Ordering::Relaxed),
            stop: Mutex::new(Some(stop)),
            tx,
            pending: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            options: Mutex::new(HashMap::new()),
            info: Mutex::new(Value::Null),
        });
        let reader = client.clone();
        let reader_app = app.clone();
        tokio::spawn(async move {
            let mut child = child;
            let mut stopped = stopped;
            let mut lines = BufReader::new(stdout).lines();
            loop {
                tokio::select! {
                    line = lines.next_line() => match line {
                        Ok(Some(line)) => reader.on_line(&reader_app, &line),
                        _ => break,
                    },
                    _ = &mut stopped => {
                        let _ = child.kill().await;
                        break;
                    }
                }
            }
            reader.gone(&reader_app);
        });
        let info = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": 1,
                    // No file system or terminal of ours: the agent uses its own.
                    "clientCapabilities": { "fs": { "readTextFile": false, "writeTextFile": false }, "terminal": false },
                    "clientInfo": { "name": "switchyard", "title": "Switchyard", "version": env!("CARGO_PKG_VERSION") },
                }),
            )
            .await?;
        *client.info.lock().unwrap() = info;
        Ok(client)
    }

    /// The process ended (or was stopped): requests waiting on it fail, so a
    /// running turn ends with an error, and the slot is freed for a new one.
    fn gone(self: &Arc<Self>, app: &Arc<App>) {
        self.pending.lock().unwrap().clear();
        let key = key(&self.engine, &self.account);
        let mut map = app.acp.lock().unwrap();
        if map.get(&key).and_then(|c| c.get()).is_some_and(|c| Arc::ptr_eq(c, self)) {
            map.remove(&key);
            app.guard.clear_codex_server(&key);
        }
        eprintln!("yardd: {} agent ({}) exited", self.engine, self.account);
    }

    pub fn shutdown(&self) {
        if let Some(stop) = self.stop.lock().unwrap().take() {
            let _ = stop.send(());
        }
    }

    fn send(&self, msg: Value) {
        let _ = self.tx.send(msg.to_string());
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let label = crate::engines::name(&self.engine);
        match rx.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(anyhow!("{label} {method}: {e}")),
            Err(_) => Err(anyhow!("{label} stopped before answering {method}")),
        }
    }

    fn reply(&self, id: &Value, result: Value) {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn session_of(&self, acp_sid: &str) -> Option<String> {
        self.sessions.lock().unwrap().get(acp_sid).cloned()
    }

    fn on_line(&self, app: &Arc<App>, line: &str) {
        let Ok(msg) = serde_json::from_str::<Value>(line) else { return };
        let has_id = !msg["id"].is_null();
        match (has_id, msg["method"].as_str()) {
            (true, None) => {
                let Some(id) = msg["id"].as_u64() else { return };
                if let Some(tx) = self.pending.lock().unwrap().remove(&id) {
                    let result = if msg["error"].is_null() { Ok(msg["result"].clone()) } else { Err(error_text(&msg["error"])) };
                    let _ = tx.send(result);
                }
            }
            (true, Some("session/request_permission")) => self.on_permission(app, &msg["id"], &msg["params"]),
            // File system and terminal requests: we offered neither.
            (true, Some(_)) => self.send(json!({ "jsonrpc": "2.0", "id": msg["id"], "error": { "code": -32601, "message": "not supported by Switchyard" } })),
            (false, Some("session/update")) => {
                let p = &msg["params"];
                let Some(sid) = p["sessionId"].as_str().and_then(|s| self.session_of(s)) else { return };
                self.on_update(app, &sid, p["sessionId"].as_str().unwrap_or(""), &p["update"]);
            }
            _ => {}
        }
    }

    fn on_update(&self, app: &Arc<App>, sid: &str, acp_sid: &str, u: &Value) {
        if self.streams.lock().unwrap().get(sid).is_some_and(|s| s.loading) {
            return;
        }
        let kind = u["sessionUpdate"].as_str().unwrap_or("");
        // Output with no turn is a prompt that Stop ended here and the agent
        // kept running: shown, it would open a message no turn owns and the
        // next answer would be appended to it. Updates carry no prompt id, so
        // what such a prompt says during the next turn cannot be told apart.
        if matches!(kind, "agent_message_chunk" | "agent_thought_chunk" | "tool_call" | "tool_call_update") && !in_turn(app, sid) {
            return;
        }
        match kind {
            "agent_message_chunk" => {
                let Some(text) = u["content"]["text"].as_str() else { return };
                let id = {
                    let mut streams = self.streams.lock().unwrap();
                    let s = streams.entry(sid.to_string()).or_default();
                    let thought = s.thought.take();
                    if let Some((id, text)) = thought {
                        app.emit(sid, json!({ "type": "reasoning", "itemId": id, "text": text }));
                    }
                    if s.text.is_none() {
                        s.n += 1;
                        s.text = Some((format!("{acp_sid}-t{}", s.n), String::new()));
                    }
                    let (id, acc) = s.text.as_mut().unwrap();
                    acc.push_str(text);
                    id.clone()
                };
                app.emit(sid, json!({ "type": "assistant_delta", "itemId": id, "text": text }));
            }
            "agent_thought_chunk" => {
                let Some(text) = u["content"]["text"].as_str() else { return };
                let id = {
                    let mut streams = self.streams.lock().unwrap();
                    let s = streams.entry(sid.to_string()).or_default();
                    if let Some((id, text)) = s.text.take() {
                        app.emit(sid, json!({ "type": "assistant_message", "itemId": id, "text": text }));
                    }
                    if s.thought.is_none() {
                        s.n += 1;
                        s.thought = Some((format!("{acp_sid}-r{}", s.n), String::new()));
                    }
                    let (id, acc) = s.thought.as_mut().unwrap();
                    acc.push_str(text);
                    id.clone()
                };
                app.emit(sid, json!({ "type": "reasoning_delta", "itemId": id, "text": text }));
            }
            "tool_call" | "tool_call_update" => self.on_tool(app, sid, u),
            "config_option_update" => {
                self.options.lock().unwrap().entry(acp_sid.to_string()).or_insert_with(|| json!({}))["configOptions"] =
                    u["configOptions"].clone();
            }
            "current_mode_update" => {
                if let Some(o) = self.options.lock().unwrap().get_mut(acp_sid) {
                    o["modes"]["currentModeId"] = u["currentModeId"].clone();
                }
            }
            _ => {}
        }
    }

    /// Text or thinking in progress ends where a tool call starts, so the
    /// thread shows them in the order they happened.
    fn close_text(&self, app: &Arc<App>, sid: &str) {
        let (text, thought) = {
            let mut streams = self.streams.lock().unwrap();
            let s = streams.entry(sid.to_string()).or_default();
            (s.text.take(), s.thought.take())
        };
        if let Some((id, text)) = thought {
            app.emit(sid, json!({ "type": "reasoning", "itemId": id, "text": text }));
        }
        if let Some((id, text)) = text {
            app.emit(sid, json!({ "type": "assistant_message", "itemId": id, "text": text }));
        }
    }

    fn on_tool(&self, app: &Arc<App>, sid: &str, u: &Value) {
        let Some(id) = u["toolCallId"].as_str() else { return };
        let cwd = app.meta(sid).map(|m| m.cwd).unwrap_or_default();
        let first = self.streams.lock().unwrap().entry(sid.to_string()).or_default().started.insert(id.to_string());
        if first {
            self.close_text(app, sid);
            let (kind, title, detail) = describe(u, &cwd);
            if kind == "command" {
                app.guard.command_started(sid, id, &cwd);
            }
            // Switchyard's own tools read as what they did ("Messaged @Reviewer").
            let (title, detail) = match own_tool(u).and_then(|t| crate::agent_tools::label(t, &u["rawInput"])) {
                Some(own) => own,
                None => (title.to_string(), detail),
            };
            app.emit(sid, json!({ "type": "tool_started", "itemId": id, "kind": kind, "title": title, "detail": detail }));
        }
        self.diffs(app, sid, id, &u["content"], &cwd);
        let status = u["status"].as_str().unwrap_or("");
        if matches!(status, "completed" | "failed")
            && self.streams.lock().unwrap().entry(sid.to_string()).or_default().done.insert(id.to_string())
        {
            app.guard.command_done(sid, id);
            app.emit(
                sid,
                json!({ "type": "tool_completed", "itemId": id, "ok": status == "completed", "output": output_text(u) }),
            );
        }
    }

    /// A tool call's diffs, once, as soon as they are known: before the
    /// approval that asks about them, like Codex's.
    fn diffs(&self, app: &Arc<App>, sid: &str, id: &str, content: &Value, cwd: &str) {
        let files: Vec<Value> = content
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter(|c| c["type"] == "diff")
                    .map(|c| {
                        let path = relative(c["path"].as_str().unwrap_or(""), cwd);
                        let old = c["oldText"].as_str();
                        let new = c["newText"].as_str().unwrap_or("");
                        let diff = match old {
                            Some(old) => replace_patch(old, new),
                            None => new.lines().take(200).map(|l| format!("+{l}")).collect::<Vec<_>>().join("\n"),
                        };
                        json!({ "path": path, "diff": diff, "kind": if old.is_none() { "add" } else { "update" } })
                    })
                    .collect()
            })
            .unwrap_or_default();
        if files.is_empty() || !self.streams.lock().unwrap().entry(sid.to_string()).or_default().diffs.insert(id.to_string()) {
            return;
        }
        app.emit(sid, json!({ "type": "file_change", "itemId": format!("{id}-diff"), "files": files }));
    }

    fn on_permission(&self, app: &Arc<App>, id: &Value, p: &Value) {
        let options: Vec<Value> = p["options"].as_array().cloned().unwrap_or_default();
        let Some(sid) = p["sessionId"].as_str().and_then(|s| self.session_of(s)) else {
            self.reply(id, json!({ "outcome": answer(&options, "deny") }));
            return;
        };
        let call = &p["toolCall"];
        let cwd = app.meta(&sid).map(|m| m.cwd).unwrap_or_default();
        if let Some(tool) = call["toolCallId"].as_str() {
            self.diffs(app, &sid, tool, &call["content"], &cwd);
        }
        let (kind, title, detail) = describe(call, &cwd);
        let approval_kind = match kind {
            "command" => "command",
            "edit" => "edit",
            _ => "other",
        };
        // Switchyard's own tools act only on the agent's own conversation and
        // are capped, so they run unasked.
        if own_tool(call).is_some() && crate::agent_tools::offered(app, &sid) {
            self.reply(id, json!({ "outcome": answer(&options, "allow") }));
            return;
        }
        let permission = app.meta(&sid).map(|m| crate::origins::turn_permission(&m.permission, &m.answering)).unwrap_or_default();
        if let Some(decision) = policy(&permission, approval_kind) {
            self.reply(id, json!({ "outcome": answer(&options, decision) }));
            return;
        }
        let request_id = format!("acp-{}", self.next.fetch_add(1, Ordering::Relaxed));
        self.approvals.lock().unwrap().insert(request_id.clone(), Approval { id: id.clone(), sid: sid.clone(), options });
        let (title, detail) = match approval_kind {
            "command" => (detail, String::new()),
            _ => (call["title"].as_str().map(str::to_string).unwrap_or(title.to_string()), detail),
        };
        app.emit(
            &sid,
            json!({ "type": "approval_request", "requestId": request_id, "kind": approval_kind, "title": title, "detail": detail, "itemId": call["toolCallId"] }),
        );
    }

    /// `resolved` is logged before the agent hears the answer, so nothing it
    /// does next can come first in the log.
    pub fn approve(&self, request_id: &str, decision: &str, resolved: impl FnOnce()) -> Result<()> {
        let a = self.approvals.lock().unwrap().remove(request_id).context("approval is no longer pending")?;
        resolved();
        self.reply(&a.id, json!({ "outcome": answer(&a.options, decision) }));
        Ok(())
    }

    /// Stop asking: a cancelled turn's open questions are answered `cancelled`, as ACP wants.
    fn cancel_approvals(&self, sid: &str) {
        let open: Vec<Approval> = {
            let mut map = self.approvals.lock().unwrap();
            let ids: Vec<String> = map.iter().filter(|(_, a)| a.sid == sid).map(|(k, _)| k.clone()).collect();
            ids.into_iter().filter_map(|k| map.remove(&k)).collect()
        };
        for a in open {
            self.reply(&a.id, json!({ "outcome": { "outcome": "cancelled" } }));
        }
    }

    fn attach(&self, app: &Arc<App>, sid: &str, acp_sid: &str, answer: &Value) {
        self.sessions.lock().unwrap().insert(acp_sid.to_string(), sid.to_string());
        self.options.lock().unwrap().insert(acp_sid.to_string(), json!({ "configOptions": answer["configOptions"], "modes": answer["modes"] }));
        if self.engine == "gemini" && self.account == crate::accounts::DEFAULT {
            crate::gemini::publish_models(app, answer);
        }
        app.set_engine_ref(sid, acp_sid);
        if let Some(s) = app.sessions.lock().unwrap().get_mut(sid) {
            s.backend = Backend::Acp { session_id: acp_sid.to_string(), epoch: self.epoch, turn: None, applied: None };
        }
    }

    /// Set an option the agent offers (`configOptions`): `value` as ACP has it,
    /// then in the `{ value }` shape Grok's docs show, if the first is refused.
    async fn set_option(&self, acp_sid: &str, config_id: &str, value: &str) -> Result<()> {
        let first = self
            .request("session/set_config_option", json!({ "sessionId": acp_sid, "configId": config_id, "value": value }))
            .await;
        let res = match first {
            Ok(v) => v,
            Err(_) => {
                self.request(
                    "session/set_config_option",
                    json!({ "sessionId": acp_sid, "configId": config_id, "value": { "value": value } }),
                )
                .await?
            }
        };
        if res["configOptions"].is_array() {
            self.options.lock().unwrap().entry(acp_sid.to_string()).or_insert_with(|| json!({}))["configOptions"] =
                res["configOptions"].clone();
        }
        Ok(())
    }

    /// Bring the agent's session in line with ours: model, effort and the
    /// permission mode's agent mode (plan or not), where the agent offers them.
    async fn apply(&self, acp_sid: &str, model: &str, effort: &str, permission: &str) -> Result<()> {
        let opts = self.options.lock().unwrap().get(acp_sid).cloned().unwrap_or(Value::Null);
        let config = |category: &str| -> Option<Value> {
            opts["configOptions"].as_array()?.iter().find(|o| o["category"] == category || o["id"] == category).cloned()
        };
        let values = |o: &Value| -> Vec<String> {
            o["options"]
                .as_array()
                .map(|l| l.iter().filter_map(|x| x["value"].as_str().or(x["id"].as_str()).map(str::to_string)).collect())
                .unwrap_or_default()
        };
        if !model.is_empty() {
            match config("model") {
                Some(o) if o["currentValue"].as_str() != Some(model) => {
                    self.set_option(acp_sid, o["id"].as_str().unwrap_or("model"), model).await.context("could not switch the model")?
                }
                Some(_) => {}
                None => {
                    self.request("session/set_model", json!({ "sessionId": acp_sid, "modelId": model }))
                        .await
                        .context("could not switch the model")?;
                }
            }
        }
        // Read again: a new model brings its own effort list.
        let opts = self.options.lock().unwrap().get(acp_sid).cloned().unwrap_or(Value::Null);
        let effort_option = opts["configOptions"].as_array().and_then(|l| l.iter().find(|o| o["category"] == "thought_level").cloned());
        if let Some(o) = effort_option {
            if !effort.is_empty() && o["currentValue"].as_str() != Some(effort) && values(&o).iter().any(|v| v == effort) {
                // An effort the model does not take is not worth failing the turn for.
                if let Err(e) = self.set_option(acp_sid, o["id"].as_str().unwrap_or("effort"), effort).await {
                    eprintln!("yardd: {} kept its effort: {e:#}", self.engine);
                }
            }
        }
        let wanted: &[&str] = match (self.engine.as_str(), permission) {
            // Plan is optional in Gemini. Fall back to default (even after
            // yolo), with edit refusals enforced by yardd's policy.
            ("gemini", "plan") => &["plan", "default"],
            ("gemini", "full") => &["yolo"],
            ("gemini", "acceptEdits") => &["auto_edit"],
            ("gemini", _) => &["default"],
            (_, "plan") => &["plan"],
            (_, "full") => &["bypassPermissions", "build", "default"],
            (_, "acceptEdits") => &["acceptEdits", "build", "default"],
            _ => &["build", "default"],
        };
        if let Some(o) = config("mode") {
            let offered = values(&o);
            if let Some(mode) = wanted.iter().find(|m| offered.iter().any(|v| v == *m)) {
                if o["currentValue"].as_str() != Some(mode) {
                    self.set_option(acp_sid, o["id"].as_str().unwrap_or("mode"), mode).await.context("could not set the agent's mode")?;
                }
            }
        } else if let Some(modes) = opts["modes"]["availableModes"].as_array() {
            let offered: Vec<&str> = modes.iter().filter_map(|m| m["id"].as_str()).collect();
            if let Some(mode) = wanted.iter().find(|m| offered.contains(m)) {
                if opts["modes"]["currentModeId"].as_str() != Some(mode) {
                    self.request("session/set_mode", json!({ "sessionId": acp_sid, "modeId": mode }))
                        .await
                        .context("could not set the agent's mode")?;
                    if let Some(o) = self.options.lock().unwrap().get_mut(acp_sid) {
                        o["modes"]["currentModeId"] = json!(mode);
                    }
                }
            }
        }
        Ok(())
    }

    /// The text and thinking still open when a turn ends, as final events.
    fn flush(&self, app: &Arc<App>, sid: &str) {
        self.close_text(app, sid);
        if let Some(s) = self.streams.lock().unwrap().get_mut(sid) {
            s.started.clear();
            s.done.clear();
            s.diffs.clear();
        }
    }
}

fn error_text(e: &Value) -> String {
    let message = e["message"].as_str().map(str::to_string).unwrap_or_else(|| e.to_string());
    match e["data"].as_str().filter(|d| !d.is_empty()) {
        Some(data) => format!("{message} ({data})"),
        None => message,
    }
}

/// What yardd answers itself, by the session's permission mode: full access
/// allows everything, Accept edits allows edits, Plan only refuses them.
/// Anything else is the user's to answer.
pub fn policy(permission: &str, kind: &str) -> Option<&'static str> {
    match (permission, kind) {
        ("full", _) => Some("allow"),
        ("acceptEdits", "edit") => Some("allow"),
        ("plan", "edit") => Some("deny"),
        _ => None,
    }
}

/// The outcome for a decision, from the options the agent offered.
pub fn answer(options: &[Value], decision: &str) -> Value {
    let kinds: &[&str] = match decision {
        "allow" => &["allow_once", "allow_always"],
        "allow_always" => &["allow_always", "allow_once"],
        _ => &["reject_once", "reject_always"],
    };
    for kind in kinds {
        if let Some(id) = options.iter().find(|o| o["kind"] == *kind).and_then(|o| o["optionId"].as_str()) {
            return json!({ "outcome": "selected", "optionId": id });
        }
    }
    json!({ "outcome": "cancelled" })
}

/// A tool call as the thread shows it: kind, title, detail.
fn describe(call: &Value, cwd: &str) -> (&'static str, &'static str, String) {
    let title = call["title"].as_str().unwrap_or("").to_string();
    let path = call["locations"][0]["path"].as_str().map(|p| relative(p, cwd));
    let input = &call["rawInput"];
    match call["kind"].as_str().unwrap_or("other") {
        "execute" => {
            let command = match &input["command"] {
                Value::String(s) => s.clone(),
                Value::Array(parts) => parts.iter().filter_map(|p| p.as_str()).collect::<Vec<_>>().join(" "),
                _ => title,
            };
            ("command", "Ran", command)
        }
        "read" => ("read", "Read", path.unwrap_or(title)),
        "edit" | "delete" | "move" => ("edit", "Edited", path.unwrap_or(title)),
        "search" => ("search", "Searched", title),
        "fetch" => ("other", "Fetched", title),
        _ => ("other", "Called", title),
    }
}

fn output_text(u: &Value) -> Value {
    let mut parts: Vec<String> = u["content"]
        .as_array()
        .map(|l| l.iter().filter(|c| c["type"] == "content").filter_map(|c| c["content"]["text"].as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    if parts.is_empty() {
        match &u["rawOutput"] {
            Value::String(s) => parts.push(s.clone()),
            o if o["output"].is_string() => parts.push(o["output"].as_str().unwrap_or("").to_string()),
            _ => {}
        }
    }
    if parts.is_empty() {
        return Value::Null;
    }
    let text = parts.join("\n");
    if text.len() <= MAX_OUTPUT {
        return json!(text);
    }
    let mut cut = MAX_OUTPUT;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    json!(format!("{}\n[... cut ...]", &text[..cut]))
}

fn relative(path: &str, cwd: &str) -> String {
    let prefix = format!("{}/", cwd.trim_end_matches('/'));
    path.strip_prefix(&prefix).unwrap_or(path).to_string()
}

/// Old text -> new text, with the unchanged lines at both ends kept as two
/// lines of context: the patch the Claude sidecar builds for its edits.
pub fn replace_patch(old: &str, new: &str) -> String {
    let a: Vec<&str> = old.split('\n').collect();
    let b: Vec<&str> = new.split('\n').collect();
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let (mut end_a, mut end_b) = (a.len(), b.len());
    while end_a > start && end_b > start && a[end_a - 1] == b[end_b - 1] {
        end_a -= 1;
        end_b -= 1;
    }
    let mut out: Vec<String> = Vec::new();
    out.extend(a[start.saturating_sub(2)..start].iter().map(|l| format!(" {l}")));
    out.extend(a[start..end_a].iter().take(200).map(|l| format!("-{l}")));
    out.extend(b[start..end_b].iter().take(200).map(|l| format!("+{l}")));
    out.extend(a[end_a..(end_a + 2).min(a.len())].iter().map(|l| format!(" {l}")));
    out.join("\n")
}

/// The agent for an engine's account, started on first use.
pub async fn client(app: &Arc<App>, engine: &str, account: &str) -> Result<Arc<Client>> {
    let cell = app.acp.lock().unwrap().entry(key(engine, account)).or_default().clone();
    let (app2, engine2, account2) = (app.clone(), engine.to_string(), account.to_string());
    cell.get_or_try_init(|| async move { Client::start(app2, engine2, account2).await }).await.cloned()
}

/// The agent if it is running, without starting one.
pub fn running(app: &App, engine: &str, account: &str) -> Option<Arc<Client>> {
    app.acp.lock().unwrap().get(&key(engine, account)).and_then(|c| c.get().cloned())
}

/// Stop an account's agent when none of its sessions is mid-turn, so the next
/// message starts a fresh one (after a login, which it may have cached).
/// Its sessions resume there.
pub fn restart_when_idle(app: &App, engine: &str, account: &str) {
    let busy = app.sessions.lock().unwrap().values().any(|s| {
        s.meta.engine == engine && s.meta.account == account && matches!(s.meta.status.as_str(), "running" | "needs_approval")
    });
    if busy {
        return;
    }
    let gone = app.acp.lock().unwrap().remove(&key(engine, account));
    if let Some(c) = gone.and_then(|cell| cell.get().cloned()) {
        c.shutdown();
    }
}

/// The agent for a session's account, with the session attached to it:
/// resumed when it was made by an earlier process (a yardd restart, or an
/// agent that exited), or created when it never got as far as the agent.
async fn attached(app: &Arc<App>, sid: &str) -> Result<(Arc<Client>, String)> {
    let meta = app.meta(sid).context("no such session")?;
    let c = client(app, &meta.engine, &meta.account).await?;
    let (current, saved) = {
        let map = app.sessions.lock().unwrap();
        let s = map.get(sid).context("no such session")?;
        let current = match &s.backend {
            Backend::Acp { session_id, epoch, .. } if *epoch == c.epoch => Some(session_id.clone()),
            _ => None,
        };
        (current, s.engine_ref.clone())
    };
    if let Some(acp_sid) = current {
        return Ok((c, acp_sid));
    }
    let Some(acp_sid) = saved else {
        create(app, sid).await?;
        let acp_sid = app.sessions.lock().unwrap().get(sid).and_then(|s| s.engine_ref.clone()).context("no session")?;
        return Ok((c, acp_sid));
    };
    let caps = c.info.lock().unwrap()["agentCapabilities"].clone();
    let params = json!({ "sessionId": acp_sid, "cwd": meta.cwd, "mcpServers": mcp_servers(app, sid) });
    let answer = if !caps["sessionCapabilities"]["resume"].is_null() {
        c.request("session/resume", params).await?
    } else if caps["loadSession"].as_bool() == Some(true) {
        // Map the session first, so its replayed history can be told apart and dropped.
        c.sessions.lock().unwrap().insert(acp_sid.clone(), sid.to_string());
        c.streams.lock().unwrap().entry(sid.to_string()).or_default().loading = true;
        let answer = c.request("session/load", params).await;
        if let Some(s) = c.streams.lock().unwrap().get_mut(sid) {
            s.loading = false;
        }
        answer?
    } else {
        bail!("{} cannot reopen an earlier session", crate::engines::name(&meta.engine));
    };
    c.attach(app, sid, &acp_sid, &answer);
    Ok((c, acp_sid))
}

/// The MCP servers a session opens with: Switchyard's tools for an agent's
/// conversation, none otherwise (the agent loads the user's own config).
fn mcp_servers(app: &App, sid: &str) -> Value {
    json!(crate::agent_tools::launch(app, sid).map(|l| vec![l.acp()]).unwrap_or_default())
}

/// One of Switchyard's own tools, from an ACP tool call.
fn own_tool(call: &Value) -> Option<&'static str> {
    [&call["title"], &call["_meta"]["toolName"], &call["_meta"]["name"]].into_iter().filter_map(Value::as_str).find_map(crate::agent_tools::own_tool)
}

pub async fn create(app: &Arc<App>, sid: &str) -> Result<()> {
    let meta = app.meta(sid).context("no such session")?;
    let c = client(app, &meta.engine, &meta.account).await?;
    let answer = c.request("session/new", json!({ "cwd": meta.cwd, "mcpServers": mcp_servers(app, sid) })).await?;
    let acp_sid = answer["sessionId"].as_str().context("session/new returned no session id")?.to_string();
    c.attach(app, sid, &acp_sid, &answer);
    Ok(())
}

/// The prompt's content blocks: the text with attachments named by path, and
/// images inline when the agent takes them.
fn prompt(c: &Client, text: &str, files: &[Value]) -> Vec<Value> {
    let mut blocks = vec![json!({ "type": "text", "text": attachments::prompt_text(text, files) })];
    let images = c.info.lock().unwrap()["agentCapabilities"]["promptCapabilities"]["image"].as_bool() == Some(true);
    if images {
        for a in files.iter().filter(|a| attachments::is_inline_image(a)) {
            if let Ok(bytes) = std::fs::read(a["path"].as_str().unwrap_or("")) {
                blocks.push(json!({ "type": "image", "mimeType": a["mime"], "data": base64::engine::general_purpose::STANDARD.encode(bytes) }));
            }
        }
    }
    blocks
}

/// Start a turn. `session/prompt` answers when the turn is over, so the
/// answer is awaited in the background and ends the turn.
pub async fn send(app: &Arc<App>, sid: &str, text: &str, files: &[Value]) -> Result<()> {
    let (c, acp_sid) = attached(app, sid).await?;
    let meta = app.meta(sid).context("no such session")?;
    // A turn on a watched channel's notice runs in Plan.
    let permission = crate::origins::turn_permission(&meta.permission, &meta.answering);
    let wanted = (meta.model.clone(), meta.effort.clone(), permission.clone());
    let applied = match app.sessions.lock().unwrap().get(sid).map(|s| &s.backend) {
        Some(Backend::Acp { applied, .. }) => applied.clone(),
        _ => None,
    };
    if applied.as_ref() != Some(&wanted) {
        c.apply(&acp_sid, &meta.model, &meta.effort, &permission).await?;
    }
    let token = EPOCH.fetch_add(1, Ordering::Relaxed);
    if let Some(s) = app.sessions.lock().unwrap().get_mut(sid) {
        if let Backend::Acp { turn, applied, .. } = &mut s.backend {
            *turn = Some(token);
            *applied = Some(wanted);
        }
    }
    let blocks = prompt(&c, text, files);
    let (app, sid) = (app.clone(), sid.to_string());
    tokio::spawn(async move {
        let res = c.request("session/prompt", json!({ "sessionId": acp_sid, "prompt": blocks })).await;
        // Ended here already (Stop that the agent ignored): nothing more to say.
        let ours = match app.sessions.lock().unwrap().get_mut(&sid).map(|s| &mut s.backend) {
            Some(Backend::Acp { turn, .. }) if *turn == Some(token) => {
                *turn = None;
                true
            }
            _ => false,
        };
        c.flush(&app, &sid);
        // A turn spends Grok's credits: read what is left, on the agent at hand.
        if c.engine == "grok" {
            let (c, app) = (c.clone(), app.clone());
            tokio::spawn(async move {
                if let Ok(answer) = c.request(crate::grok::BILLING, json!({})).await {
                    let _ = crate::grok::store_usage(&app, &c.account, &answer);
                }
            });
        }
        if !ours {
            return;
        }
        app.guard.turn_done(&sid);
        match res {
            Ok(r) => {
                let status = match r["stopReason"].as_str().unwrap_or("end_turn") {
                    "cancelled" => "interrupted",
                    "refusal" => {
                        app.emit(&sid, json!({ "type": "error", "message": "The model refused to continue." }));
                        "failed"
                    }
                    "max_tokens" | "max_turn_requests" => {
                        app.emit(&sid, json!({ "type": "note", "text": "The turn stopped at the engine's length limit." }));
                        "completed"
                    }
                    _ => "completed",
                };
                app.emit(&sid, json!({ "type": "turn_completed", "status": status }));
            }
            Err(e) => {
                app.emit(&sid, json!({ "type": "error", "message": format!("{e:#}") }));
                app.emit(&sid, json!({ "type": "turn_completed", "status": "failed" }));
            }
        }
    });
    Ok(())
}

fn current(app: &App, sid: &str) -> Result<(Arc<Client>, String)> {
    let meta = app.meta(sid).context("no such session")?;
    let c = running(app, &meta.engine, &meta.account).context("the agent is not running")?;
    let map = app.sessions.lock().unwrap();
    match map.get(sid).map(|s| &s.backend) {
        Some(Backend::Acp { session_id, epoch, .. }) if *epoch == c.epoch => Ok((c.clone(), session_id.clone())),
        _ => bail!("the session is not attached to the agent"),
    }
}

/// `session/cancel` is a notification: the agent ends the turn with
/// `cancelled`, or Stop's grace period ends it here.
pub fn interrupt(app: &Arc<App>, sid: &str) -> Result<()> {
    let (c, acp_sid) = current(app, sid)?;
    c.cancel_approvals(sid);
    c.send(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": acp_sid } }));
    Ok(())
}

fn in_turn(app: &App, sid: &str) -> bool {
    matches!(app.sessions.lock().unwrap().get(sid).map(|s| &s.backend), Some(Backend::Acp { turn: Some(_), .. }))
}

/// The turn did not end after Stop: forget it, so a late answer says nothing.
/// Its open text is closed here, or the next turn's first words would be
/// appended to it.
pub fn end_turn_here(app: &Arc<App>, sid: &str) {
    if let Ok((c, _)) = current(app, sid) {
        c.flush(app, sid);
    }
    if let Some(s) = app.sessions.lock().unwrap().get_mut(sid) {
        if let Backend::Acp { turn, .. } = &mut s.backend {
            *turn = None;
        }
    }
}

pub async fn approve(app: &Arc<App>, sid: &str, request_id: &str, decision: &str) -> Result<()> {
    let (c, _) = current(app, sid)?;
    c.approve(request_id, decision, || crate::sessions::approval_resolved(app, sid, request_id, decision))
}

/// A session being deleted: stop its turn and drop its mapping.
pub fn forget(app: &Arc<App>, sid: &str) {
    if let Ok((c, acp_sid)) = current(app, sid) {
        c.cancel_approvals(sid);
        c.send(json!({ "jsonrpc": "2.0", "method": "session/cancel", "params": { "sessionId": acp_sid } }));
        c.sessions.lock().unwrap().remove(&acp_sid);
        c.streams.lock().unwrap().remove(sid);
        c.options.lock().unwrap().remove(&acp_sid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_pick_the_offered_option_or_cancel() {
        let options = vec![
            json!({ "optionId": "once", "kind": "allow_once" }),
            json!({ "optionId": "always", "kind": "allow_always" }),
            json!({ "optionId": "reject", "kind": "reject_once" }),
        ];
        assert_eq!(answer(&options, "allow"), json!({ "outcome": "selected", "optionId": "once" }));
        assert_eq!(answer(&options, "allow_always"), json!({ "outcome": "selected", "optionId": "always" }));
        assert_eq!(answer(&options, "deny"), json!({ "outcome": "selected", "optionId": "reject" }));
        // Nothing to reject with: cancelling is the only way to say no.
        assert_eq!(answer(&options[..2], "deny"), json!({ "outcome": "cancelled" }));
    }

    #[test]
    fn the_permission_mode_answers_what_it_covers() {
        assert_eq!(policy("full", "command"), Some("allow"));
        assert_eq!(policy("acceptEdits", "edit"), Some("allow"));
        assert_eq!(policy("acceptEdits", "command"), None);
        assert_eq!(policy("plan", "edit"), Some("deny"));
        assert_eq!(policy("plan", "command"), None);
        assert_eq!(policy("ask", "edit"), None);
    }

    #[test]
    fn tool_calls_read_like_the_other_engines() {
        let cwd = "/w/p";
        let run = json!({ "kind": "execute", "title": "bash", "rawInput": { "command": "ls -la" } });
        assert_eq!(describe(&run, cwd), ("command", "Ran", "ls -la".to_string()));
        let edit = json!({ "kind": "edit", "title": "Edit", "locations": [{ "path": "/w/p/src/a.rs" }] });
        assert_eq!(describe(&edit, cwd), ("edit", "Edited", "src/a.rs".to_string()));
        assert_eq!(describe(&json!({ "kind": "think", "title": "todo" }), cwd), ("other", "Called", "todo".to_string()));
    }

    #[test]
    fn a_patch_keeps_two_lines_of_context() {
        let old = "a\nb\nc\nd\ne\nf";
        let new = "a\nb\nc\nD\ne\nf";
        assert_eq!(replace_patch(old, new), " b\n c\n-d\n+D\n e\n f");
    }
}
