//! Connectors: the MCP servers each engine account has on this host (claude.ai
//! connectors, plugins, config files, ChatGPT apps), whether each one works,
//! and signing them in from the UI.
//!
//! Claude: a `yard-claude connectors` process per account, started on demand
//! and stopped after 10 idle minutes. It runs Claude Code with no prompt, so
//! listing and signing in spend no usage. Codex: the account's app-server
//! (`mcpServerStatus/list`, `mcpServer/oauth/login`, `app/list`).
//!
//! Ops (each may carry `host`, routed like `engines.status`):
//! - `mcp.status { engine, account? }` -> broadcast `mcp { engine, account, servers | error }`
//! - `mcp.auth { engine, account?, server }` -> broadcast `mcp.auth { engine, account, server, state, url?, paste?, error? }`
//!   with `state` `open` (a sign-in page to visit), then `done` or `failed`
//! - `mcp.callback { engine, account?, server, url }`: the address the browser
//!   ended on after signing in, for a host the browser cannot reach
//! - `mcp.reconnect { engine, account?, server }`
//! - `mcp.add { engine, account?, name, url }`: a remote (HTTP) server added to
//!   the account's own config (Claude: `claude mcp add --scope user`; Codex:
//!   the app-server's `config/value/write`, since `codex mcp add` goes on to
//!   wait for a browser sign-in), then signed in like any other; `mcp.remove
//!   { engine, account?, server }` takes one out. Both broadcast `mcp.change
//!   { engine, account, name, state: "done" | "failed", error? }` and a fresh list.
//!
//! For Claude and Codex yardd never sees a token: the CLIs run their own
//! OAuth flows and keep the tokens where they always do. The ACP engines
//! have no sign-in to drive, so their connectors are held by Switchyard
//! itself (`held.rs`): the same ops with `engine: "switchyard"`, no account.

use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::{mpsc, oneshot},
};

use crate::state::{App, Backend};

/// A connectors process with nothing to do is stopped after this.
const IDLE: Duration = Duration::from_secs(10 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(90);

type Reply = oneshot::Sender<Result<Value, String>>;

#[derive(Default)]
pub struct Connectors {
    /// Claude connectors processes by account.
    probes: Mutex<HashMap<String, Arc<Probe>>>,
    /// Answers a session's sidecar owes yardd (a widget's tool call), by ref.
    replies: Mutex<HashMap<String, Reply>>,
    /// One start at a time: two callers at once (a listing and a sign-in
    /// right after an add) must share one process, or the sign-in's callback
    /// page dies with the one that is dropped.
    starting: tokio::sync::Mutex<()>,
}

struct Probe {
    tx: mpsc::UnboundedSender<String>,
    pending: Mutex<HashMap<u64, Reply>>,
    next: AtomicU64,
    last_used: Mutex<Instant>,
    /// Sign-ins the process is still watching; it is not stopped meanwhile.
    watching: Mutex<std::collections::HashSet<String>>,
    _stop: oneshot::Sender<()>,
}

impl Probe {
    async fn request(&self, kind: &str, mut msg: Value) -> Result<Value> {
        *self.last_used.lock().unwrap() = Instant::now();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().unwrap().insert(id, tx);
        msg["id"] = json!(id);
        msg["type"] = json!(kind);
        self.tx.send(msg.to_string()).map_err(|_| anyhow!("the connectors process is not running"))?;
        let answer = tokio::time::timeout(REQUEST_TIMEOUT, rx).await;
        self.pending.lock().unwrap().remove(&id);
        match answer {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(anyhow!(e)),
            Ok(Err(_)) => Err(anyhow!("the connectors process exited")),
            Err(_) => Err(anyhow!("Claude Code did not answer in time")),
        }
    }
}

impl Connectors {
    /// Hand a sidecar's `{ type: "reply", ref, result | error }` to whoever waits on it.
    pub fn reply(&self, msg: &Value) {
        let Some(r) = msg["ref"].as_str() else { return };
        if let Some(tx) = self.replies.lock().unwrap().remove(r) {
            let _ = tx.send(match msg["error"].as_str() {
                Some(e) => Err(e.to_string()),
                None => Ok(msg["result"].clone()),
            });
        }
    }

    /// Wait for the reply to a request sent with `r`.
    pub async fn wait(&self, r: &str, timeout: Duration) -> Result<Value> {
        let (tx, rx) = oneshot::channel();
        self.replies.lock().unwrap().insert(r.to_string(), tx);
        let answer = tokio::time::timeout(timeout, rx).await;
        self.replies.lock().unwrap().remove(r);
        match answer {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(anyhow!(e)),
            Ok(Err(_)) => Err(anyhow!("the Claude sidecar exited")),
            Err(_) => Err(anyhow!("the tool did not answer in time")),
        }
    }
}

fn account_of(msg: &Value) -> &str {
    msg["account"].as_str().filter(|a| !a.is_empty()).unwrap_or(crate::accounts::DEFAULT)
}

fn str_field<'a>(msg: &'a Value, key: &str) -> Result<&'a str> {
    msg[key].as_str().filter(|s| !s.is_empty()).with_context(|| format!("missing `{key}`"))
}

pub async fn handle(app: &Arc<App>, msg: &Value) -> Result<()> {
    let op = msg["op"].as_str().unwrap_or("");
    let engine = str_field(msg, "engine")?.to_string();
    // The connectors Switchyard holds itself are one set per host: no account.
    if engine == crate::held::ENGINE {
        return crate::held::handle(app, msg).await;
    }
    if engine != "claude" && engine != "codex" {
        bail!("Unknown engine for connectors: {engine}");
    }
    let account = account_of(msg).to_string();
    app.accounts.check(&engine, &account)?;
    match op {
        "mcp.status" => {
            let app = app.clone();
            tokio::spawn(async move { broadcast_status(&app, &engine, &account).await });
        }
        "mcp.auth" => {
            let server = str_field(msg, "server")?.to_string();
            let app = app.clone();
            tokio::spawn(async move {
                if let Err(e) = auth(&app, &engine, &account, &server).await {
                    auth_state(&app, &engine, &account, &server, json!({ "state": "failed", "error": format!("{e:#}") }));
                }
            });
        }
        "mcp.callback" => {
            let server = str_field(msg, "server")?.to_string();
            let url = str_field(msg, "url")?.trim().to_string();
            let app = app.clone();
            tokio::spawn(async move {
                if let Err(e) = callback(&app, &engine, &account, &server, &url).await {
                    auth_state(&app, &engine, &account, &server, json!({ "state": "failed", "error": format!("{e:#}") }));
                }
            });
        }
        "mcp.reconnect" => {
            let server = str_field(msg, "server")?.to_string();
            let app = app.clone();
            tokio::spawn(async move {
                if engine == "claude" {
                    if let Ok(p) = probe(&app, &account).await {
                        let _ = p.request("reconnect", json!({ "server": server })).await;
                    }
                } else if let Ok(c) = crate::codex::client(&app, &account).await {
                    let _ = c.request("config/mcpServer/reload", json!({})).await;
                }
                broadcast_status(&app, &engine, &account).await;
            });
        }
        "mcp.add" | "mcp.remove" => {
            let adding = op == "mcp.add";
            let name = if adding { str_field(msg, "name")? } else { str_field(msg, "server")? }.trim().to_string();
            let url = if adding { Some(str_field(msg, "url")?.trim().to_string()) } else { None };
            if adding {
                check_new_server(&name, url.as_deref().unwrap_or(""))?;
            }
            let app = app.clone();
            tokio::spawn(async move {
                let result = change(&app, &engine, &account, &name, url.as_deref()).await;
                let mut note = json!({ "op": "mcp.change", "engine": engine, "account": account, "name": name, "state": "done" });
                if let Err(e) = result {
                    note["state"] = json!("failed");
                    note["error"] = json!(clip(&format!("{e:#}"), 500));
                }
                app.broadcast(note);
                broadcast_status(&app, &engine, &account).await;
            });
        }
        other => bail!("unknown op `{other}`"),
    }
    Ok(())
}

/// A name the CLIs take as a config key and a plain https URL: neither can
/// become a flag or carry anything but an address.
pub fn check_new_server(name: &str, url: &str) -> Result<()> {
    let name_ok = !name.is_empty()
        && name.len() <= 40
        && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !name_ok {
        bail!("Name it with letters, digits, - or _ (up to 40), starting with a letter or digit");
    }
    let rest = url.strip_prefix("https://").context("The address must start with https://")?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_ok = !host.is_empty() && host.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    if !host_ok || url.len() > 300 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("That is not a valid https address");
    }
    Ok(())
}

/// Add (`url` given) or remove a server in the account's own config.
async fn change(app: &Arc<App>, engine: &str, account: &str, name: &str, url: Option<&str>) -> Result<()> {
    if engine == "codex" {
        // `codex mcp add` starts the server's sign-in and waits for a browser
        // (codex-cli 0.164), so the config is written through the app-server.
        let c = crate::codex::client(app, account).await?;
        let config = c.request("config/read", json!({})).await?;
        let exists = !config["config"]["mcp_servers"][name].is_null();
        match url {
            Some(_) if exists => bail!("This account already has a server named {name}"),
            None if !exists => bail!("No server named {name} in this account's config.toml"),
            _ => {}
        }
        let value = url.map_or(Value::Null, |u| json!({ "url": u }));
        c.request("config/value/write", json!({ "keyPath": format!("mcp_servers.{name}"), "value": value, "mergeStrategy": "replace" }))
            .await?;
        let _ = c.request("config/mcpServer/reload", json!({})).await;
        return Ok(());
    }
    let mut cmd = tokio::process::Command::new("claude");
    match url {
        Some(url) => cmd.args(["mcp", "add", "--transport", "http", "--scope", "user", name, url]),
        None => cmd.args(["mcp", "remove", "--scope", "user", name]),
    };
    crate::login::strip_claude_env_tokio(&mut cmd);
    if let Some(dir) = app.accounts.config_dir(engine, account)? {
        cmd.env(crate::accounts::env_var(engine), dir);
    }
    cmd.stdin(Stdio::null()).kill_on_drop(true);
    let out = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .with_context(|| format!("`{engine} mcp` did not answer"))?
        .with_context(|| format!("could not run `{engine}`"))?;
    if !out.status.success() {
        let text = String::from_utf8_lossy(&out.stderr);
        let text = if text.trim().is_empty() { String::from_utf8_lossy(&out.stdout) } else { text };
        bail!("{}", clip(text.trim(), 400));
    }
    // The connectors process read the old config; the next listing starts a
    // new one. Idle sessions get a fresh sidecar (resuming) with their next
    // message, so they see the change; a running turn keeps its set.
    forget(app, account);
    let mut map = app.sessions.lock().unwrap();
    for s in map.values_mut() {
        if s.meta.engine == "claude" && s.meta.account == account && s.meta.status == "idle" && matches!(s.backend, Backend::Claude { .. }) {
            s.backend = Backend::Pending;
        }
    }
    Ok(())
}

async fn broadcast_status(app: &Arc<App>, engine: &str, account: &str) {
    let mut reply = json!({ "op": "mcp", "engine": engine, "account": account, "at": crate::state::now_ms() });
    match status(app, engine, account).await {
        Ok(servers) => reply["servers"] = json!(servers),
        Err(e) => reply["error"] = json!(format!("{e:#}")),
    }
    app.broadcast(reply);
}

async fn status(app: &Arc<App>, engine: &str, account: &str) -> Result<Vec<Value>> {
    if engine == "claude" {
        let rows = probe(app, account).await?.request("status", json!({})).await?;
        return Ok(rows.as_array().cloned().unwrap_or_default());
    }
    let c = crate::codex::client(app, account).await?;
    let mut rows = Vec::new();
    let mut cursor = Value::Null;
    // Pages, with a bound in case a server keeps handing out cursors.
    for _ in 0..20 {
        let page = c
            .request("mcpServerStatus/list", json!({ "detail": "toolsAndAuthOnly", "cursor": cursor }))
            .await?;
        rows.extend(page["data"].as_array().into_iter().flatten().map(codex_row));
        cursor = page["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    // ChatGPT apps (connectors) are a separate list, and older app-servers
    // or plans without them refuse it: that is not an error here.
    if let Ok(apps) = c.request("app/list", json!({})).await {
        rows.extend(apps["data"].as_array().into_iter().flatten().map(codex_app_row));
    }
    Ok(rows)
}

pub(crate) fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    format!("{}…", s.chars().take(max - 1).collect::<String>())
}

/// A widget is a `ui://` resource under the current key or the flat one.
pub fn ui_resource(meta: &Value) -> Option<&str> {
    meta["ui"]["resourceUri"]
        .as_str()
        .or_else(|| meta["ui/resourceUri"].as_str())
        .filter(|u| u.starts_with("ui://"))
}

/// One `mcpServerStatus/list` entry in the shape the sidecar reports.
fn codex_row(s: &Value) -> Value {
    let tools: Vec<&Value> = s["tools"].as_object().map(|t| t.values().collect()).unwrap_or_default();
    let app_tools = tools.iter().filter(|t| ui_resource(&t["_meta"]).is_some()).count();
    let status = match s["runtimeStatus"].as_str() {
        Some("connected") => "connected",
        Some("authenticationRequired") => "needs-auth",
        Some("failed" | "cancelled") => "failed",
        Some("disabled") => "disabled",
        Some("starting" | "notStarted") => "pending",
        _ if s["authStatus"] == "notLoggedIn" => "needs-auth",
        _ if !s["toolsError"].is_null() => "failed",
        _ => "connected",
    };
    let mut row = json!({
        "name": clip(s["name"].as_str().unwrap_or(""), 200),
        "status": status,
        "source": if s["pluginId"].is_string() { "plugin" } else { "config" },
        "tools": tools.len(),
        "appTools": app_tools,
    });
    if status == "failed" {
        if let Some(e) = s["toolsError"].as_str() {
            row["error"] = json!(clip(e, 500));
        }
    }
    if let Some(o) = s["httpOrigin"].as_str() {
        row["origin"] = json!(o);
    }
    row
}

/// A ChatGPT app: connected on chatgpt.com, so "Connect" just opens its page.
fn codex_app_row(a: &Value) -> Value {
    let accessible = a["isAccessible"].as_bool().unwrap_or(false);
    let mut row = json!({
        "name": clip(a["name"].as_str().unwrap_or(""), 200),
        "status": if !a["isEnabled"].as_bool().unwrap_or(true) { "disabled" } else if accessible { "connected" } else { "needs-auth" },
        "source": "chatgpt",
        "tools": 0,
        "appTools": 0,
        "app": true,
    });
    if let Some(u) = a["installUrl"].as_str().filter(|u| u.starts_with("https://")) {
        row["installUrl"] = json!(u);
    }
    row
}

pub(crate) fn auth_state(app: &App, engine: &str, account: &str, server: &str, mut state: Value) {
    state["op"] = json!("mcp.auth");
    state["engine"] = json!(engine);
    state["account"] = json!(account);
    state["server"] = json!(server);
    app.broadcast(state);
}

async fn auth(app: &Arc<App>, engine: &str, account: &str, server: &str) -> Result<()> {
    if engine == "claude" {
        let p = probe(app, account).await?;
        let res = p.request("auth", json!({ "server": server })).await?;
        if res["done"] == true {
            return signed_in(app, engine, account, server).await;
        }
        let url = res["url"].as_str().filter(|u| u.starts_with("https://") || u.starts_with("http://")).context("Claude Code gave no sign-in link")?;
        p.watching.lock().unwrap().insert(server.to_string());
        auth_state(app, engine, account, server, json!({ "state": "open", "url": url, "paste": res["paste"] == true }));
        return Ok(());
    }
    let c = crate::codex::client(app, account).await?;
    let res = c.request("mcpServer/oauth/login", json!({ "name": server })).await?;
    let url = res["authorizationUrl"].as_str().filter(|u| u.starts_with("https://") || u.starts_with("http://")).context("Codex gave no sign-in link")?;
    // Codex redirects to a loopback page on this host; from another machine
    // the browser cannot reach it, so the address is pasted back.
    auth_state(app, engine, account, server, json!({ "state": "open", "url": url, "paste": true }));
    Ok(())
}

async fn callback(app: &Arc<App>, engine: &str, account: &str, server: &str, url: &str) -> Result<()> {
    if engine == "claude" {
        probe(app, account).await?.request("callback", json!({ "server": server, "url": url })).await?;
        return signed_in(app, engine, account, server).await;
    }
    // Codex has no call for this: deliver the redirect to its loopback page
    // ourselves, from this host. Its completion notification says how it went.
    loopback_get(url).await
}

/// After a sign-in: tell every window, list the servers again, and have the
/// account's running Claude sessions dial the server again, so its tools work
/// without a restart.
async fn signed_in(app: &Arc<App>, engine: &str, account: &str, server: &str) -> Result<()> {
    auth_state(app, engine, account, server, json!({ "state": "done" }));
    if engine == "claude" {
        if let Some(p) = app.mcp.probes.lock().unwrap().get(account) {
            p.watching.lock().unwrap().remove(server);
        }
        let line = json!({ "type": "mcp_reconnect", "server": server }).to_string();
        for s in app.sessions.lock().unwrap().values() {
            if s.meta.engine == "claude" && s.meta.account == account {
                if let Backend::Claude { tx, .. } = &s.backend {
                    let _ = tx.send(line.clone());
                }
            }
        }
    }
    broadcast_status(app, engine, account).await;
    Ok(())
}

/// `mcpServer/oauthLogin/completed` from an account's app-server.
pub fn codex_login_completed(app: &Arc<App>, account: &str, p: &Value) {
    let Some(server) = p["name"].as_str().map(str::to_string) else { return };
    let (app, account) = (app.clone(), account.to_string());
    let ok = p["success"].as_bool().unwrap_or(false);
    let error = p["error"].as_str().unwrap_or("The sign-in failed.").to_string();
    tokio::spawn(async move {
        if ok {
            let _ = signed_in(&app, "codex", &account, &server).await;
        } else {
            auth_state(&app, "codex", &account, &server, json!({ "state": "failed", "error": clip(&error, 500) }));
        }
    });
}

/// The account's connectors process, started on first use. Boxed: its reader
/// signs servers in, which lists them again through here.
fn probe<'a>(app: &'a Arc<App>, account: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Arc<Probe>>> + Send + 'a>> {
    Box::pin(start_probe(app, account))
}

async fn start_probe(app: &Arc<App>, account: &str) -> Result<Arc<Probe>> {
    let _one = app.mcp.starting.lock().await;
    if let Some(p) = app.mcp.probes.lock().unwrap().get(account) {
        if !p.tx.is_closed() {
            return Ok(p.clone());
        }
    }
    let mut cmd = crate::claude::sidecar_command(app)?;
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    cmd.arg("connectors")
        .arg(&home)
        .current_dir(&home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if let Some(dir) = app.accounts.config_dir("claude", account)? {
        cmd.env(crate::accounts::env_var("claude"), dir);
    }
    let mut child = cmd.spawn().context("could not start the Claude sidecar (is `bun` on PATH?)")?;
    let mut stdin = child.stdin.take().context("sidecar stdin")?;
    let stdout = child.stdout.take().context("sidecar stdout")?;
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if stdin.write_all(line.as_bytes()).await.is_err() || stdin.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = stdin.flush().await;
        }
    });
    let (stop, mut stopped) = oneshot::channel::<()>();
    let p = Arc::new(Probe {
        tx,
        pending: Mutex::new(HashMap::new()),
        next: AtomicU64::new(1),
        last_used: Mutex::new(Instant::now()),
        watching: Mutex::new(Default::default()),
        _stop: stop,
    });
    app.mcp.probes.lock().unwrap().insert(account.to_string(), p.clone());

    let (app2, account2, weak) = (app.clone(), account.to_string(), Arc::downgrade(&p));
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            let line = tokio::select! {
                line = lines.next_line() => line,
                _ = &mut stopped => break,
            };
            let Ok(Some(line)) = line else { break };
            let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
            let Some(p) = weak.upgrade() else { break };
            if let Some(id) = msg["id"].as_u64() {
                if let Some(tx) = p.pending.lock().unwrap().remove(&id) {
                    let _ = tx.send(if msg["ok"] == true { Ok(msg["result"].clone()) } else { Err(msg["error"].as_str().unwrap_or("failed").to_string()) });
                }
            } else if msg["type"] == "auth_done" {
                let server = msg["server"].as_str().unwrap_or("").to_string();
                p.watching.lock().unwrap().remove(&server);
                if msg["ok"] == true {
                    // Its own task: listing again asks this process, whose
                    // answer only this loop can read.
                    let (app, account) = (app2.clone(), account2.clone());
                    tokio::spawn(async move { signed_in(&app, "claude", &account, &server).await });
                } else {
                    let error = msg["error"].as_str().unwrap_or("The sign-in failed.");
                    auth_state(&app2, "claude", &account2, &server, json!({ "state": "failed", "error": clip(error, 500) }));
                }
            }
        }
        let _ = child.kill().await;
        let mut probes = app2.mcp.probes.lock().unwrap();
        if probes.get(&account2).is_some_and(|p| p.tx.is_closed() || weak.ptr_eq(&Arc::downgrade(p))) {
            probes.remove(&account2);
        }
    });

    // Stop it once it has been idle a while with no sign-in to watch.
    let (app3, account3, weak) = (app.clone(), account.to_string(), Arc::downgrade(&p));
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let Some(p) = weak.upgrade() else { return };
            let idle = p.last_used.lock().unwrap().elapsed() > IDLE && p.watching.lock().unwrap().is_empty();
            if idle {
                drop(p);
                let mut probes = app3.mcp.probes.lock().unwrap();
                if probes.get(&account3).is_some_and(|p| weak.ptr_eq(&Arc::downgrade(p))) {
                    // Dropping the last handle fires `_stop`, and the reader kills it.
                    probes.remove(&account3);
                }
                return;
            }
        }
    });
    Ok(p)
}

/// Stop an account's connectors process, for an account that is removed or logged in again.
pub fn forget(app: &App, account: &str) {
    app.mcp.probes.lock().unwrap().remove(account);
}

/// Only a loopback `http://` address with a port, as an OAuth redirect to a
/// local CLI is. Returns the port and the path with its query.
pub fn loopback_target(url: &str) -> Result<(u16, String)> {
    let rest = url.strip_prefix("http://").context("Paste the whole address the browser ended on, starting with http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = authority.rsplit_once(':').context("the address has no port")?;
    if !matches!(host, "127.0.0.1" | "localhost" | "[::1]") {
        bail!("That is not the sign-in's own page (it should start with http://127.0.0.1: or http://localhost:)");
    }
    let port: u16 = port.parse().context("the address has no valid port")?;
    let path = path.split('#').next().unwrap_or("/");
    if path.chars().any(|c| c.is_control() || c == ' ') {
        bail!("the address has spaces or control characters");
    }
    Ok((port, path.to_string()))
}

async fn loopback_get(url: &str) -> Result<()> {
    let (port, path) = loopback_target(url)?;
    let mut stream = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(("127.0.0.1", port)))
        .await
        .context("the sign-in page did not answer")?
        .context("Nothing is waiting for that sign-in any more. Start it again.")?;
    let req = format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await?;
    let mut head = vec![0u8; 512];
    let n = tokio::time::timeout(Duration::from_secs(30), stream.read(&mut head)).await.context("the sign-in page did not answer")??;
    let status = String::from_utf8_lossy(&head[..n]);
    let code = status.split_whitespace().nth(1).unwrap_or("");
    if !code.starts_with('2') && !code.starts_with('3') {
        bail!("the sign-in page answered {}", clip(code, 20));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_redirects_are_replayed() {
        assert_eq!(loopback_target("http://127.0.0.1:44399/callback?code=a&state=b").unwrap(), (44399, "/callback?code=a&state=b".to_string()));
        assert_eq!(loopback_target("http://localhost:3118/callback?code=x#frag").unwrap(), (3118, "/callback?code=x".to_string()));
        assert!(loopback_target("https://127.0.0.1:1/x").is_err());
        assert!(loopback_target("http://example.com:80/callback").is_err());
        assert!(loopback_target("http://127.0.0.1.evil.com:80/x").is_err());
        assert!(loopback_target("http://127.0.0.1/x").is_err());
        assert!(loopback_target("http://127.0.0.1:80/a b").is_err());
        assert!(loopback_target("http://127.0.0.1:80/a\r\nHost: x").is_err());
    }

    #[test]
    fn added_servers_are_a_plain_name_and_https_address() {
        assert!(check_new_server("linear", "https://mcp.linear.app/mcp").is_ok());
        assert!(check_new_server("metabase-prod", "https://metabase.example.com:8443/api/mcp").is_ok());
        assert!(check_new_server("-x", "https://a.example/mcp").is_err());
        assert!(check_new_server("a b", "https://a.example/mcp").is_err());
        assert!(check_new_server("ok", "http://a.example/mcp").is_err());
        assert!(check_new_server("ok", "https://a.example/m cp").is_err());
        assert!(check_new_server("ok", "https://user@a.example/mcp").is_err());
        assert!(check_new_server("ok", "https:///mcp").is_err());
    }

    #[test]
    fn codex_statuses_map_to_the_shared_shape() {
        let row = codex_row(&json!({
            "name": "linear", "runtimeStatus": null, "pluginId": null, "httpOrigin": "https://mcp.linear.app",
            "tools": {}, "toolsError": "Auth required", "authStatus": "notLoggedIn"
        }));
        assert_eq!(row["status"], "needs-auth");
        assert_eq!(row["origin"], "https://mcp.linear.app");
        assert!(row["error"].is_null());
        let row = codex_row(&json!({
            "name": "chart", "runtimeStatus": null, "authStatus": "unsupported", "toolsError": null,
            "tools": { "a": { "_meta": { "ui": { "resourceUri": "ui://c/v.html" } } }, "b": {} }
        }));
        assert_eq!((row["status"].as_str(), row["tools"].as_u64(), row["appTools"].as_u64()), (Some("connected"), Some(2), Some(1)));
        let row = codex_row(&json!({ "name": "x", "runtimeStatus": "failed", "toolsError": "boom", "tools": {} }));
        assert_eq!((row["status"].as_str(), row["error"].as_str()), (Some("failed"), Some("boom")));
        let app = codex_app_row(&json!({ "name": "GitHub", "isAccessible": false, "isEnabled": true, "installUrl": "https://chatgpt.com/apps/github" }));
        assert_eq!((app["status"].as_str(), app["installUrl"].as_str()), (Some("needs-auth"), Some("https://chatgpt.com/apps/github")));
        let app = codex_app_row(&json!({ "name": "Evil", "isAccessible": false, "installUrl": "javascript:alert(1)" }));
        assert!(app["installUrl"].is_null());
    }
}
