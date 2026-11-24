//! WebSocket API for the UI.

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{bail, Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc};

use crate::{
    attachments, claude, codex, git, guard, handoff, pr, pty, remote, sessions,
    state::{App, Worktree, PERMISSIONS},
};

pub async fn handler(ws: WebSocketUpgrade, headers: HeaderMap, State(app): State<Arc<App>>) -> Response {
    let origin = headers.get("origin").and_then(|v| v.to_str().ok()).unwrap_or("none");
    if !origin_ok(&headers, app.port) {
        eprintln!("yardd: refused client, origin {origin}");
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let agent = headers.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or("none");
    eprintln!("yardd: client connected, origin {origin}, agent {agent}");
    ws.max_message_size(attachments::WS_MAX_MESSAGE)
        .max_frame_size(attachments::WS_MAX_MESSAGE)
        .on_upgrade(move |socket| client(socket, app))
}

/// This socket can run commands, so a web page in the user's browser must not
/// be able to open it. Browsers always send Origin; allow only our own page,
/// the Tauri window and the origins listed in YARD_ALLOW_ORIGIN. Non-browser
/// local clients send none. Matching Origin against the Host header is
/// deliberately not done: a rebound DNS name would pass it.
fn origin_ok(headers: &HeaderMap, port: u16) -> bool {
    let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
        return true;
    };
    let origin = origin.trim_end_matches('/');
    if origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}") {
        return true;
    }
    if origin == "tauri://localhost" || origin == "http://tauri.localhost" {
        return true;
    }
    allowed_origins().iter().any(|o| o == origin)
}

/// YARD_ALLOW_ORIGIN: extra page origins, comma separated, such as the URL a
/// container is served on behind a reverse proxy.
pub fn allowed_origins() -> Vec<String> {
    parse_origins(&std::env::var("YARD_ALLOW_ORIGIN").unwrap_or_default())
}

fn parse_origins(list: &str) -> Vec<String> {
    list.split(',')
        .map(|o| o.trim().trim_end_matches('/'))
        .filter(|o| !o.is_empty())
        .map(str::to_string)
        .collect()
}

async fn client(socket: WebSocket, app: Arc<App>) {
    let (mut sink, mut stream) = socket.split();
    let mut rx = app.tx.subscribe();
    // Replies meant for this client only.
    let (direct, mut direct_rx) = mpsc::unbounded_channel::<String>();
    // The snapshot goes out before the loop: queued, `select!` could send a
    // broadcast first, and the UI replaces its state with a snapshot, so a
    // change that overtook it was lost.
    if sink.send(Message::Text(app.snapshot().to_string())).await.is_err() {
        return;
    }
    // This client's own connection to each remote host.
    let mut pipes: HashMap<String, remote::Pipe> = HashMap::new();
    let mut host_ids = app.remotes.ids();
    let ids = host_ids.borrow_and_update().clone();
    remote::sync_pipes(&app, &mut pipes, &ids, &direct);
    // Keystrokes and resizes run one after another: each in its own task,
    // two keys typed in a row could reach the shell swapped.
    let (keys, mut keys_rx) = mpsc::unbounded_channel::<Value>();
    {
        let app = app.clone();
        let direct = direct.clone();
        tokio::spawn(async move {
            while let Some(msg) = keys_rx.recv().await {
                if let Err(e) = handle(&app, &direct, &msg).await {
                    let _ = direct.send(error_reply(&msg, &e));
                }
            }
        });
    }

    loop {
        tokio::select! {
            msg = rx.recv() => match msg {
                Ok(text) => {
                    if sink.send(Message::Text(text)).await.is_err() { break; }
                }
                // Fell behind: resync from a fresh snapshot.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    if sink.send(Message::Text(app.snapshot().to_string())).await.is_err() { break; }
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            Some(text) = direct_rx.recv() => {
                if sink.send(Message::Text(text)).await.is_err() { break; }
            }
            Ok(()) = host_ids.changed() => {
                let ids = host_ids.borrow_and_update().clone();
                remote::sync_pipes(&app, &mut pipes, &ids, &direct);
            }
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    let msg: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                    // An op for a session on another host goes to that host.
                    if let Some(host) = remote::route(&msg) {
                        let sent = match pipes.get(&host) {
                            Some(pipe) => pipe.send(msg.clone()),
                            None => Err(anyhow::anyhow!("no such host")),
                        };
                        if let Err(e) = sent {
                            let _ = direct.send(error_reply(&msg, &e));
                        }
                        continue;
                    }
                    if matches!(msg["op"].as_str(), Some("term.input" | "term.resize")) {
                        let _ = keys.send(msg);
                        continue;
                    }
                    let app = app.clone();
                    let direct = direct.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle(&app, &direct, &msg).await {
                            let _ = direct.send(error_reply(&msg, &e));
                        }
                    });
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                _ => {}
            }
        }
    }
}

/// An op's failure. `ref` comes back when the op had one, so a client
/// waiting on that one request (an upload) can tell it failed.
fn error_reply(msg: &Value, e: &anyhow::Error) -> String {
    let mut reply = json!({ "op": "error", "message": format!("{e:#}"), "for": msg["op"] });
    if matches!(msg["op"].as_str(), Some("term.open" | "term.new")) {
        reply["sessionId"] = msg["sessionId"].clone();
    }
    if !msg["ref"].is_null() {
        reply["ref"] = msg["ref"].clone();
    }
    reply.to_string()
}

fn str_field<'a>(msg: &'a Value, key: &str) -> Result<&'a str> {
    msg[key].as_str().with_context(|| format!("missing `{key}`"))
}

fn check_service_tier(app: &App, engine: &str, model: &str, tier: &str) -> Result<()> {
    if engine != "codex" || !codex::supports_service_tier(app, model, tier) {
        bail!("This model does not offer that speed.");
    }
    Ok(())
}

/// A notification is a line or two of text: cut what an agent sends too long.
fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max - 1).collect();
    format!("{cut}…")
}

/// The account a login op is for; Default when it names none.
fn account_field(msg: &Value) -> &str {
    msg["account"].as_str().filter(|a| !a.is_empty()).unwrap_or(crate::accounts::DEFAULT)
}

fn dim(msg: &Value, key: &str, fallback: u16) -> u16 {
    msg[key].as_u64().and_then(|n| u16::try_from(n).ok()).unwrap_or(fallback)
}

/// Subfolders of `path` for the folder picker, which browses whichever host
/// a session will run on. Symlinked folders count; hidden ones are included
/// and the UI decides whether to show them.
fn list_dirs(path: &str) -> Result<Vec<Value>> {
    let dir = Path::new(path);
    if !dir.is_absolute() {
        bail!("{path} is not an absolute path");
    }
    let mut dirs: Vec<(String, bool)> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {path}"))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let full = entry.path();
            if !full.is_dir() {
                return None;
            }
            let name = entry.file_name().into_string().ok()?;
            Some((name, full.join(".git").exists()))
        })
        .collect();
    dirs.sort_by_key(|(name, _)| name.to_lowercase());
    // A home folder with thousands of entries should not flood the socket.
    dirs.truncate(2000);
    Ok(dirs.into_iter().map(|(name, git)| json!({ "name": name, "git": git })).collect())
}

/// Start a session from a `session.create` message. `direct` gets `created`
/// as soon as the session exists (the UI opens it), with the message's `ref`;
/// automations pass `None` and a title of their own, and tag the session with
/// their id; an agent's chat is tagged with the agent's. `origin`: where the
/// first message came from when the owner did not type it (set by yardd
/// only, never read from a client's message).
pub async fn create_session(
    app: &Arc<App>,
    direct: Option<&mpsc::UnboundedSender<String>>,
    msg: &Value,
    title: Option<&str>,
    automation: Option<&str>,
    agent: Option<&str>,
    origin: Option<&Value>,
) -> Result<String> {
    let engine = str_field(msg, "engine")?;
    let model = str_field(msg, "model")?;
    let effort = str_field(msg, "effort")?;
    let cwd = str_field(msg, "cwd")?;
    let service_tier = msg["serviceTier"].as_str();
    if let Some(tier) = service_tier {
        check_service_tier(app, engine, model, tier)?;
    }
    let text = msg["text"].as_str().unwrap_or("").trim();
    let files = attachments::resolve(&msg["attachments"])?;
    if text.is_empty() && files.is_empty() {
        bail!("the task is empty");
    }
    if !Path::new(cwd).is_dir() {
        bail!("{cwd} is not a folder");
    }
    if !crate::engines::is_engine(engine) {
        bail!("unknown engine {engine}");
    }
    let permission = msg["permission"].as_str().unwrap_or("ask");
    if !PERMISSIONS.contains(&permission) {
        bail!("unknown permission mode {permission}");
    }
    let account = account_field(msg);
    app.accounts.check(engine, account)?;
    let first = text.lines().next().filter(|l| !l.trim().is_empty());
    let first = first.or_else(|| files.first().and_then(|a| a["name"].as_str())).unwrap_or("");
    let title: String = match title {
        Some(t) => t.to_string(),
        None => title_from(first),
    };

    // The worktree (and its setup) comes first: if it cannot be made,
    // no session exists. Without one, the folder must be free.
    // `worktree` asks for one where possible: a folder outside a
    // repository, or in one with no commits, runs in place.
    let asked = msg["worktree"].as_bool().unwrap_or(false);
    let in_place = asked && !git::can_branch(cwd).await?;
    let (sid, setup) = if asked && !in_place {
        let unique = app.id("");
        let suffix = &unique[unique.len().saturating_sub(4)..];
        let wt = git::add_worktree(cwd, msg["branch"].as_str(), msg["base"].as_str(), &title, suffix).await?;
        let worktree = Worktree { branch: wt.branch, repo: wt.repo };
        (app.insert_session(engine, model, effort, &wt.cwd, &title, Some(worktree), false, account, automation, agent)?, wt.setup)
    } else {
        // An agent's own folder holds no code: a new chat may start while another runs.
        let exclusive = !agent.is_some_and(|a| Path::new(cwd).starts_with(crate::agents::home(a)));
        (app.insert_session(engine, model, effort, cwd, &title, None, exclusive, account, automation, agent)?, None)
    };
    if let Some(agent) = agent {
        crate::agents::set_chat(app, agent, &sid);
    }
    if let Some(script) = setup {
        app.emit(&sid, json!({ "type": "note", "text": format!("Worktree setup ran: {script}") }));
    }
    if in_place {
        let text = "Not a git repository with commits, so this session works in the folder itself.";
        app.emit(&sid, json!({ "type": "note", "text": text }));
    }
    app.update_meta(&sid, |m| {
        m.permission = permission.to_string();
        m.service_tier = service_tier.map(str::to_string);
    });
    if let Some(direct) = direct {
        let mut created = json!({ "op": "created", "sessionId": sid });
        if !msg["ref"].is_null() {
            created["ref"] = msg["ref"].clone();
        }
        let _ = direct.send(created.to_string());
    }
    sessions::begin(app, &sid, text, &files, origin).await;
    // A title from the task now, rather than its first line until a turn ends.
    if app.meta(&sid).is_some_and(|m| crate::naming::due_at_start(&m)) {
        app.name_later(&sid);
    }
    Ok(sid)
}

async fn handle(app: &Arc<App>, direct: &mpsc::UnboundedSender<String>, msg: &Value) -> Result<()> {
    if msg["sessionId"].as_str().is_some_and(crate::private::internal) {
        bail!("that engine thread is private to yardd");
    }
    if msg["op"].as_str().is_some_and(|op| op.starts_with("slack.")) {
        return crate::slack::handle(app, msg).await;
    }
    if msg["op"].as_str().is_some_and(|op| op.starts_with("agent.")) {
        return crate::agents::handle(app, direct, msg).await;
    }
    // A hub asks for this host's evidence for its host agent; a hub from
    // before agents asks with the old name.
    if matches!(msg["op"].as_str(), Some("context.collect" | "switchy.context")) {
        let query = msg["query"].as_str().unwrap_or("");
        anyhow::ensure!(query.len() <= 8000, "question is too long");
        let mut reply = crate::host_context::collect(app, query).await;
        reply["op"] = msg["op"].clone();
        reply["ref"] = msg["ref"].clone();
        let _ = direct.send(reply.to_string());
        return Ok(());
    }
    if msg["op"].as_str().is_some_and(|op| op.starts_with("pr.")) {
        return pr::handle(app, direct, msg).await;
    }
    if msg["op"].as_str().is_some_and(|op| op.starts_with("session.handoff")) {
        return handoff::handle(app, direct, msg).await;
    }
    if msg["op"].as_str().is_some_and(|op| op.starts_with("mcp.")) {
        return crate::mcp::handle(app, msg).await;
    }
    if msg["op"].as_str().is_some_and(|op| op.starts_with("widget.")) {
        return crate::widgets::handle(app, direct, msg).await;
    }
    if matches!(msg["op"].as_str(), Some("claude.sessions" | "session.import")) {
        return crate::import::handle(app, direct, msg).await;
    }
    match msg["op"].as_str().unwrap_or("") {
        "mobile.connected" => {
            app.mobile.connected()?;
            app.broadcast(json!({ "op": "mobile.status", "mobile": app.mobile.status() }));
        }
        "session.create" => {
            create_session(app, Some(direct), msg, None, None, None, None).await?;
        }
        "git.branches" => {
            let cwd = str_field(msg, "cwd")?;
            // New session asks this for the folder it remembers, which may be
            // gone since. Say so, so the page can tell before the user writes
            // a task that `session.create` would then refuse.
            let reply = if !Path::new(cwd).is_dir() {
                json!({ "op": "git.branches", "cwd": cwd, "missing": true })
            } else {
                match git::branches(cwd).await {
                    Ok(mut v) => {
                        v["op"] = json!("git.branches");
                        v["cwd"] = json!(cwd);
                        v
                    }
                    Err(e) => json!({ "op": "git.branches", "cwd": cwd, "error": format!("{e:#}") }),
                }
            };
            let _ = direct.send(reply.to_string());
        }
        "usage.refresh" => {
            let account = account_field(msg);
            match str_field(msg, "engine")? {
                "claude" => claude::refresh_usage(app, account).await?,
                "codex" => crate::codex::refresh_usage(app, account).await?,
                _ => bail!("this engine does not report subscription limits"),
            }
            let _ = direct.send(json!({ "op": "usage.refreshed", "ref": msg["ref"] }).to_string());
        }
        "engines.status" => {
            let app = app.clone();
            tokio::spawn(async move { app.broadcast(crate::login::status(&app).await) });
        }
        "login.start" => {
            let engine = str_field(msg, "engine")?;
            let account = account_field(msg);
            if let Err(e) = crate::login::start(app, engine, account).await {
                app.broadcast(json!({ "op": "login", "engine": engine, "account": account, "state": "failed", "error": format!("{e:#}") }));
            }
        }
        "login.code" => crate::login::code(app, str_field(msg, "engine")?, account_field(msg), str_field(msg, "code")?).await?,
        // OpenCode: the provider and way in picked in the login dialog.
        "login.choose" => {
            let account = account_field(msg);
            app.accounts.check("opencode", account)?;
            let index = msg["method"].as_u64().context("missing `method`")?;
            if let Err(e) = crate::opencode::choose(app, account, str_field(msg, "provider")?, index).await {
                app.broadcast(json!({ "op": "login", "engine": "opencode", "account": account, "state": "failed", "error": format!("{e:#}") }));
            }
        }
        "login.cancel" => crate::login::cancel(app, str_field(msg, "engine")?, account_field(msg)).await?,
        "account.add" => {
            let engine = str_field(msg, "engine")?;
            let added = app.accounts.add(app.id("a"), engine, str_field(msg, "name")?)?;
            if engine == "claude" {
                claude::refresh_usage_soon(app, &added.id);
            }
            app.broadcast_accounts();
            // The new account's login state, for the Hosts page.
            let app = app.clone();
            tokio::spawn(async move { app.broadcast(crate::login::status(&app).await) });
        }
        "account.remove" => {
            let id = str_field(msg, "id")?;
            let used = app.sessions.lock().unwrap().values().filter(|s| s.meta.account == id).count();
            if used > 0 {
                bail!("{used} session{} still use this account. Delete {} first.", if used == 1 { "" } else { "s" }, if used == 1 { "it" } else { "them" });
            }
            let found = app.accounts.all().into_iter().find(|a| a.id == id).context("no such account")?;
            // A login still waiting on the account would write into a deleted folder.
            crate::login::cancel(app, &found.engine, id).await?;
            let removed = app.accounts.remove(id)?;
            crate::mcp::forget(app, &removed.id);
            // Its app-server (and the login it holds open) goes with it.
            let gone = app.codex.lock().unwrap().remove(&removed.id);
            if let Some(client) = gone.and_then(|cell| cell.get().cloned()) {
                client.shutdown();
            }
            app.guard.clear_codex_server(&removed.id);
            let agent = app.acp.lock().unwrap().remove(&crate::acp::key(&removed.engine, &removed.id));
            if let Some(client) = agent.and_then(|cell| cell.get().cloned()) {
                client.shutdown();
            }
            app.broadcast_accounts();
            let app = app.clone();
            tokio::spawn(async move { app.broadcast(crate::login::status(&app).await) });
        }
        "fs.dirs" => {
            let path = str_field(msg, "path")?;
            let reply = match list_dirs(path) {
                Ok(dirs) => json!({ "op": "fs.dirs", "path": path, "dirs": dirs }),
                Err(e) => json!({ "op": "fs.dirs", "path": path, "error": format!("{e:#}") }),
            };
            let _ = direct.send(reply.to_string());
        }
        "attach.put" => {
            // Answered with the client's `ref`, so concurrent uploads match up.
            let mut reply = match attachments::put(app, msg) {
                Ok(attachment) => json!({ "op": "attach.stored", "attachment": attachment }),
                Err(e) => json!({ "op": "attach.stored", "error": format!("{e:#}") }),
            };
            reply["ref"] = msg["ref"].clone();
            let _ = direct.send(reply.to_string());
        }
        "attach.get" => {
            let sid = str_field(msg, "sessionId")?;
            let id = str_field(msg, "id")?;
            let mut reply = attachments::get(app, sid, id)
                .unwrap_or_else(|e| json!({ "op": "attach.data", "sessionId": sid, "id": id, "error": format!("{e:#}") }));
            reply["ref"] = msg["ref"].clone();
            let _ = direct.send(reply.to_string());
        }
        "session.open" => {
            let sid = str_field(msg, "sessionId")?;
            let _ = direct.send(app.history(sid).to_string());
        }
        "session.send" => {
            let sid = str_field(msg, "sessionId")?;
            let text = msg["text"].as_str().unwrap_or("").trim();
            let files = attachments::resolve(&msg["attachments"])?;
            if text.is_empty() && files.is_empty() {
                bail!("the message is empty");
            }
            sessions::send_message(app, sid, text, &files, None).await?;
        }
        // Taken back to edit (or dropped): the asking window gets it, if it
        // had not reached the agent yet.
        "session.unqueue" => {
            let sid = str_field(msg, "sessionId")?;
            let item = app.take_queued(sid, str_field(msg, "id")?, false);
            if let Some(item) = item.filter(|_| msg["drop"] != true) {
                let _ = direct.send(
                    json!({ "op": "queue.returned", "sessionId": sid, "id": item["id"], "text": item["text"], "attachments": item["attachments"] }).to_string(),
                );
            }
        }
        "session.steer" => {
            let sid = str_field(msg, "sessionId")?;
            let id = str_field(msg, "id")?;
            app.meta(sid).context("no such session")?;
            app.steer_queued(sid, id);
        }
        "session.setModel" => {
            let sid = str_field(msg, "sessionId")?;
            let model = str_field(msg, "model")?;
            let effort = str_field(msg, "effort")?;
            let meta = app.meta(sid).context("no such session")?;
            let clear_tier = meta.service_tier.as_deref().is_some_and(|tier| !codex::supports_service_tier(app, model, tier));
            // A queued Claude session has no sidecar yet; it starts on the new model.
            if meta.engine == "claude" && meta.model != model && meta.status != "queued" {
                claude::set_model(app, sid, model)?;
            }
            app.update_meta(sid, |m| {
                m.model = model.to_string();
                m.effort = effort.to_string();
                if clear_tier {
                    m.service_tier = None;
                }
            });
        }
        "session.setServiceTier" => {
            let sid = str_field(msg, "sessionId")?;
            let tier = str_field(msg, "serviceTier")?;
            let meta = app.meta(sid).context("no such session")?;
            check_service_tier(app, &meta.engine, &meta.model, tier)?;
            app.update_meta(sid, |m| {
                if m.model == meta.model {
                    m.service_tier = Some(tier.to_string());
                }
            });
        }
        "session.setPermission" => {
            let sid = str_field(msg, "sessionId")?;
            let permission = str_field(msg, "permission")?;
            if !PERMISSIONS.contains(&permission) {
                bail!("unknown permission mode {permission}");
            }
            let meta = app.meta(sid).context("no such session")?;
            if permission != "plan" && crate::private::restricted(app, sid) {
                bail!("The host agent only reads: its chats stay in Plan only.");
            }
            // Codex and the ACP engines take the mode with their next turn (yardd
            // answers an ACP engine's later questions by the new mode at once); a
            // running Claude sidecar switches now, a queued one starts in it.
            if meta.engine == "claude" && meta.status != "queued" {
                claude::set_permission(app, sid, permission)?;
            }
            app.update_meta(sid, |m| m.permission = permission.to_string());
        }
        "session.rename" => {
            let sid = str_field(msg, "sessionId")?;
            let title = str_field(msg, "title")?.trim();
            if title.is_empty()
                || title.chars().count() > SESSION_TITLE_LIMIT
                || title.chars().any(|c| c.is_control() || matches!(c, '\u{2028}' | '\u{2029}'))
            {
                bail!("Enter a session name on one line with 1 to {SESSION_TITLE_LIMIT} characters.");
            }
            // A typed title is the user's: never replaced by a generated one (see `naming`).
            if !app.update_meta(sid, |m| {
                m.title = title.to_string();
                m.title_source = Some("user".into());
            }) {
                bail!("no such session");
            }
            if !msg["ref"].is_null() {
                let _ = direct.send(json!({ "op": "session.renamed", "ref": msg["ref"] }).to_string());
            }
        }
        // Sidebar housekeeping.
        "session.update" => {
            let sid = str_field(msg, "sessionId")?;
            app.meta(sid).context("no such session")?;
            let pinned = msg["pinned"].as_bool();
            let archived = msg["archived"].as_bool();
            // A name sets the group; null or a blank name clears it; absent leaves it.
            let group = match &msg["group"] {
                Value::String(g) => Some(Some(g.trim().chars().take(60).collect::<String>()).filter(|g| !g.is_empty())),
                Value::Null if msg.get("group").is_some() => Some(None),
                _ => None,
            };
            app.update_meta(sid, |m| {
                if let Some(g) = group {
                    m.group = g;
                }
                if let Some(p) = pinned {
                    m.pinned = p;
                    // Pinning an archived run brings it back to the list.
                    if p {
                        m.archived = false;
                    }
                }
                if let Some(a) = archived {
                    m.archived = a;
                    // An archived session leaves the pinned list too.
                    if a {
                        m.pinned = false;
                    }
                }
            });
        }
        "session.interrupt" => {
            let sid = str_field(msg, "sessionId")?;
            // Like Esc in Claude Code: what was queued comes back to the
            // reply box of the window that stopped the turn, unsent.
            let items = app.take_all_queued(sid, |_| true);
            // Messages from elsewhere are not the owner's to edit: their
            // senders hear that the turn stopped before reading them.
            let (theirs, items): (Vec<Value>, Vec<Value>) = items.into_iter().partition(|i| crate::origins::is_tagged(Some(&i["origin"])));
            if !theirs.is_empty() {
                let origins = theirs.iter().map(|i| i["origin"].clone()).collect();
                app.owe(crate::origins::Owed { sid: sid.to_string(), origins, status: "unread".into(), text: String::new() });
            }
            if !items.is_empty() {
                let (text, files) = sessions::combine(&items);
                let _ = direct.send(json!({ "op": "queue.returned", "sessionId": sid, "text": text, "attachments": files }).to_string());
            }
            sessions::interrupt(app, sid).await?;
        }
        "session.approve" => {
            let sid = str_field(msg, "sessionId")?;
            let request_id = str_field(msg, "requestId")?;
            let decision = str_field(msg, "decision")?;
            let meta = app.meta(sid).context("no such session")?;
            // Each engine logs `approval_resolved` itself, before it passes the answer on.
            match meta.engine.as_str() {
                "codex" => codex::approve(app, sid, request_id, decision).await?,
                "opencode" | "grok" | "gemini" => crate::acp::approve(app, sid, request_id, decision).await?,
                _ => claude::approve(app, sid, request_id, decision)?,
            }
        }
        "session.startNow" => {
            let sid = str_field(msg, "sessionId")?;
            app.meta(sid).context("no such session")?;
            if !app.guard.take(sid) {
                bail!("this session is not queued");
            }
            sessions::start_queued(app, sid).await;
        }
        "session.delete" => {
            let sid = str_field(msg, "sessionId")?;
            let meta = app.meta(sid).context("no such session")?;
            if !msg["removeWorktree"].as_bool().unwrap_or(false) {
                delete_session(app, sid, &meta).await;
                return Ok(());
            }
            // Everything that could refuse runs before anything is deleted.
            let (Some(branch), Some(repo)) = (meta.branch.clone(), meta.repo.clone()) else {
                bail!("this session has no worktree that yardd made");
            };
            let top = git::removable_worktree(&meta.cwd, &repo).await?;
            // git prints the real path; a session may hold a symlinked spelling.
            let real = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
            let top_real = real(&top);
            let others = app
                .sessions
                .lock()
                .unwrap()
                .values()
                .filter(|s| s.meta.id != sid && real(&s.meta.cwd).starts_with(&top_real))
                .count();
            if others > 0 {
                bail!("Another session still uses this worktree. Delete it first, or keep the worktree.");
            }
            delete_session(app, sid, &meta).await;
            let removed = git::remove_worktree(&repo, &top, &branch).await;
            let _ = direct.send(
                json!({
                    "op": "worktree.removed",
                    "sessionId": sid,
                    "branchDeleted": removed.branch_deleted,
                    "message": removed.message,
                })
                .to_string(),
            );
        }
        "changes.get" => {
            let sid = str_field(msg, "sessionId")?;
            let meta = app.meta(sid).context("no such session")?;
            let changes = match git::changes(&meta.cwd).await {
                Ok(c) => c,
                Err(e) => json!({ "error": format!("{e:#}") }),
            };
            let _ = direct.send(json!({ "op": "changes", "sessionId": sid, "changes": changes }).to_string());
        }
        // The session's processes as of the last host sample. Asked for, not
        // broadcast: only the pane that shows them wants them.
        "session.procs" => {
            let sid = str_field(msg, "sessionId")?;
            app.meta(sid).context("no such session")?;
            let _ = direct.send(json!({ "op": "session.procs", "sessionId": sid, "procs": app.guard.procs(sid) }).to_string());
        }
        // Stop one of the session's processes from the Processes tab. The
        // pid is checked against the session's tree read now, not trusted
        // from the client (see `guard::kill`).
        "session.kill" => {
            let sid = str_field(msg, "sessionId")?;
            app.meta(sid).context("no such session")?;
            let pid = msg["pid"].as_u64().and_then(|p| u32::try_from(p).ok()).context("missing `pid`")?;
            let signal = str_field(msg, "signal")?;
            let sig = guard::Sig::parse(signal).with_context(|| format!("unknown signal `{signal}`: term or kill"))?;
            let tree = msg["tree"].as_bool().unwrap_or(false);
            let mut reply = guard::kill(app, sid, pid, msg["start"].as_u64(), sig, tree).await;
            // The tab waits on this one request.
            if !msg["ref"].is_null() {
                reply["ref"] = msg["ref"].clone();
            }
            let _ = direct.send(reply.to_string());
        }
        "term.open" => {
            let sid = str_field(msg, "sessionId")?;
            let opened = pty::open(app, sid, dim(msg, "cols", 80), dim(msg, "rows", 24))?;
            let _ = direct.send(opened.to_string());
        }
        "term.new" => {
            let sid = str_field(msg, "sessionId")?;
            let opened = pty::new(app, sid, dim(msg, "cols", 80), dim(msg, "rows", 24))?;
            let _ = direct.send(opened.to_string());
        }
        "term.attach" => {
            let opened = pty::attach(app, str_field(msg, "termId")?, dim(msg, "cols", 80), dim(msg, "rows", 24))?;
            let _ = direct.send(opened.to_string());
        }
        "term.list" => {
            let sid = str_field(msg, "sessionId")?;
            app.meta(sid).context("no such session")?;
            let _ = direct.send(pty::list(app, sid).to_string());
        }
        "term.input" => pty::input(app, str_field(msg, "termId")?, str_field(msg, "data")?)?,
        "term.resize" => pty::resize(app, str_field(msg, "termId")?, dim(msg, "cols", 80), dim(msg, "rows", 24))?,
        "term.close" => pty::close(app, str_field(msg, "termId")?)?,
        op if op.starts_with("automation.") => crate::automations::handle(app, direct, msg).await?,
        "host.add" => remote::add(app, msg)?,
        "host.remove" => remote::remove(app, str_field(msg, "hostId")?)?,
        "host.install" => remote::install(app, str_field(msg, "hostId")?)?,
        "notify" => {
            let windows = notify(app, msg)?;
            let _ = direct.send(json!({ "op": "notify.sent", "windows": windows }).to_string());
        }
        other => bail!("unknown op `{other}`"),
    }
    Ok(())
}

/// Show `{ title, body?, sessionId?, url? }` as an OS notification in every
/// open window. Answers how many windows were connected (counting a caller
/// on the socket), so a caller can tell nobody was there.
pub fn notify(app: &App, msg: &Value) -> Result<usize> {
    let title = msg["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).context("missing `title`")?;
    let mut note = json!({ "op": "notify", "title": clip(title, 120) });
    if let Some(body) = msg["body"].as_str().map(str::trim).filter(|b| !b.is_empty()) {
        note["body"] = json!(clip(body, 400));
    }
    if let Some(sid) = msg["sessionId"].as_str().filter(|s| !s.is_empty()) {
        note["sessionId"] = json!(sid);
    }
    // The window opens this on a click, so only web pages are allowed.
    if let Some(url) = msg["url"].as_str().filter(|u| !u.is_empty()) {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            bail!("`url` must be an http(s) link");
        }
        note["url"] = json!(url);
    }
    let windows = app.tx.receiver_count();
    app.broadcast(note);
    Ok(windows)
}

/// Stop the session's engine and terminal and forget it.
pub async fn delete_session(app: &Arc<App>, sid: &str, meta: &crate::state::Meta) {
    // A queued session has no engine yet: this just drops it from the queue.
    app.guard.forget(sid);
    match meta.engine.as_str() {
        "codex" => codex::forget(app, sid).await,
        "opencode" | "grok" | "gemini" => crate::acp::forget(app, sid),
        _ => {}
    }
    pty::close_session(app, sid);
    app.tools.forget(sid);
    app.remove_session(sid);
}

const SESSION_TITLE_LIMIT: usize = 60;

/// A session's title from its first line: at most 60 characters. A longer
/// line is cut at a word and marked "…", so it does not read like a typo
/// (the whole prompt is still the first message).
fn title_from(first: &str) -> String {
    let first = first.trim();
    if first.chars().count() <= SESSION_TITLE_LIMIT {
        return first.to_string();
    }
    let head: String = first.chars().take(SESSION_TITLE_LIMIT - 1).collect();
    let next = first.chars().nth(SESSION_TITLE_LIMIT - 1);
    // A word that runs past the cut is dropped, unless that would leave less
    // than half the title: then a long word (a pasted URL) is cut through.
    let at_word = head.rsplit_once(char::is_whitespace).map(|(h, _)| h).filter(|h| h.trim_end().chars().count() >= SESSION_TITLE_LIMIT / 2);
    let cut = if next.is_some_and(char::is_whitespace) { head.as_str() } else { at_word.unwrap_or(head.as_str()) };
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::parse_origins;

    #[test]
    fn allow_origin_is_a_trimmed_comma_list() {
        assert_eq!(
            parse_origins(" https://yard.example.com/ ,http://100.64.0.1:47717,, https://b.example//"),
            ["https://yard.example.com", "http://100.64.0.1:47717", "https://b.example"]
        );
        assert!(parse_origins("").is_empty());
        assert!(parse_origins(" , ").is_empty());
    }
}
