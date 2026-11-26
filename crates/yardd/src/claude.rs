//! Claude adapter: one `yard-claude` sidecar per session, JSON lines over stdio.
//! The sidecar already speaks normalized events, so this only relays.

use std::{process::Stdio, sync::Arc};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::{mpsc, oneshot},
};

use crate::state::{App, Backend};

/// The sidecar as a command, for a session or a one-shot (`sessions`,
/// `messages <id>`): the bundled binary, else the script from the repo.
pub fn sidecar_command(app: &App) -> Result<Command> {
    // The desktop app bundles a compiled sidecar and points YARD_SIDECAR at
    // it, so it needs no Bun. Without it, run the script from the repo.
    let mut cmd = match std::env::var_os("YARD_SIDECAR").filter(|s| !s.is_empty()) {
        Some(binary) => Command::new(binary),
        None => {
            let script = app.root.join("sidecar/yard-claude/src/main.ts");
            if !script.exists() {
                bail!("sidecar not found at {}", script.display());
            }
            let mut cmd = Command::new("bun");
            cmd.arg("run").arg(&script);
            cmd
        }
    };
    // yardd may itself be started from inside a Claude Code session.
    for (key, _) in std::env::vars() {
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") || key.starts_with("CLAUDE_AGENT_SDK") {
            cmd.env_remove(key);
        }
    }
    Ok(cmd)
}

/// How often each Claude account's plan limits are read without a turn.
/// `YARD_CLAUDE_USAGE_EVERY_MS=0` turns it off (the tests do, but one).
fn usage_every() -> Option<std::time::Duration> {
    match std::env::var("YARD_CLAUDE_USAGE_EVERY_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(0) => None,
        Some(ms) => Some(std::time::Duration::from_millis(ms)),
        None => Some(std::time::Duration::from_secs(10 * 60)),
    }
}

/// Claude reports its limits only during a turn, so an account that has not
/// run one showed none. This reads every Claude account's limits now and then
/// with `yard-claude usage` (Claude Code's usage report, no prompt and no
/// turn) and stores them like a turn's own report.
pub async fn run_usage_refresh(app: Arc<App>) {
    let Some(every) = usage_every() else { return };
    loop {
        let mut accounts = vec![crate::accounts::DEFAULT.to_string()];
        accounts.extend(app.accounts.all().into_iter().filter(|a| a.engine == "claude").map(|a| a.id));
        for account in accounts {
            if let Err(e) = refresh_usage(&app, &account).await {
                eprintln!("yardd: claude usage for {account}: {e:#}");
            }
        }
        tokio::time::sleep(every).await;
    }
}

/// Read one account's limits now (a new account, a fresh login), unless the
/// refresh is turned off.
pub fn refresh_usage_soon(app: &Arc<App>, account: &str) {
    if usage_every().is_none() {
        return;
    }
    let (app, account) = (app.clone(), account.to_string());
    tokio::spawn(async move {
        if let Err(e) = refresh_usage(&app, &account).await {
            eprintln!("yardd: claude usage for {account}: {e:#}");
        }
    });
}

pub async fn refresh_usage(app: &Arc<App>, account: &str) -> Result<()> {
    let mut cmd = sidecar_command(app)?;
    cmd.arg("usage").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    if let Some(dir) = app.accounts.config_dir("claude", account)? {
        cmd.env(crate::accounts::env_var("claude"), dir);
    }
    let out = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("no answer within 60 s"))?
        .context("could not start the Claude sidecar")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('{')).unwrap_or("");
    let v: Value = serde_json::from_str(line).context("no usage line")?;
    if let Some(e) = v["error"].as_str() {
        bail!("{e}");
    }
    // An account with no plan limits (an API key) has none to show.
    if v["usage"]["windows"].is_object() {
        app.set_usage("claude", account, v["usage"].clone());
    }
    Ok(())
}

pub async fn create(app: &Arc<App>, sid: &str) -> Result<()> {
    spawn(app, sid, None)
}

/// A sidecar for a session that has none (restored after a restart, or
/// dropped after a login), resuming its conversation. It runs no turn until
/// a message comes; a widget's tool call needs only the process.
pub fn ensure(app: &Arc<App>, sid: &str) -> Result<()> {
    let pending = {
        let map = app.sessions.lock().unwrap();
        map.get(sid)
            .filter(|s| matches!(s.backend, Backend::Pending))
            .map(|s| s.engine_ref.clone())
    };
    match pending {
        Some(resume) => spawn(app, sid, resume),
        None => Ok(()),
    }
}

/// Start the sidecar. `resume` is a Claude Code session id to continue.
fn spawn(app: &Arc<App>, sid: &str, resume: Option<String>) -> Result<()> {
    let meta = app.meta(sid).context("no such session")?;
    let mut cmd = sidecar_command(app)?;
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    // An added account is its own config folder; Default keeps the environment.
    if let Some(dir) = app.accounts.config_dir("claude", &meta.account)? {
        cmd.env(crate::accounts::env_var("claude"), dir);
    }
    let mut child = cmd
        .spawn()
        .context("could not start the Claude sidecar (YARD_SIDECAR unset: is `bun` on PATH?)")?;
    // The sidecar's tree (sidecar, CLI, tool commands) is this session's usage.
    let pid = child.id();
    if let Some(pid) = pid {
        app.guard.set_sidecar(sid, pid);
    }
    let mut stdin = child.stdin.take().context("sidecar stdin")?;
    let stdout = child.stdout.take().context("sidecar stdout")?;

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if stdin.write_all(line.as_bytes()).await.is_err() {
                break;
            }
            if stdin.write_all(b"\n").await.is_err() {
                break;
            }
            let _ = stdin.flush().await;
        }
    });

    let (stop, mut stopped) = oneshot::channel::<()>();
    // Before the reader starts, so a sidecar that dies at once is still the
    // one the reader finds and clears.
    let own = tx.clone();
    if let Some(s) = app.sessions.lock().unwrap().get_mut(sid) {
        s.backend = Backend::Claude { tx: tx.clone(), _stop: stop };
        s.active_at = crate::state::now_ms();
    }
    let reader_app = app.clone();
    let reader_sid = sid.to_string();
    tokio::spawn(async move {
        let mut child = child;
        let mut lines = BufReader::new(stdout).lines();
        loop {
            let line = tokio::select! {
                line = lines.next_line() => line,
                // The session was deleted.
                _ = &mut stopped => {
                    let _ = child.kill().await;
                    break;
                }
            };
            let Ok(Some(line)) = line else { break };
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match msg["type"].as_str() {
                Some("event") => reader_app.emit(&reader_sid, msg["event"].clone()),
                Some("usage") => {
                    if let Some(account) = reader_app.meta(&reader_sid).map(|m| m.account) {
                        reader_app.set_usage("claude", &account, msg["usage"].clone());
                    }
                }
                Some("reply") => reader_app.mcp.reply(&msg),
                Some("session") => {
                    if let Some(id) = msg["claudeSessionId"].as_str() {
                        reader_app.set_engine_ref(&reader_sid, id);
                    }
                }
                _ => {}
            }
        }
        let _ = child.wait().await;
        if let Some(pid) = pid {
            reader_app.guard.clear_sidecar(&reader_sid, pid);
        }
        // A dead sidecar left in place swallows the next message, and that
        // turn never ends. Pending makes the next message start a new one
        // that resumes the conversation.
        let mut current = false;
        if let Some(s) = reader_app.sessions.lock().unwrap().get_mut(&reader_sid) {
            if matches!(&s.backend, Backend::Claude { tx, .. } if tx.same_channel(&own)) {
                s.backend = Backend::Pending;
                current = true;
            }
        }
        drop(own);
        // The sidecar is gone: close out whatever turn was open. One already
        // replaced (the host agent starts a fresh one per question) had none.
        let running = reader_app
            .meta(&reader_sid)
            .map(|m| m.status != "idle")
            .unwrap_or(false);
        if running && current {
            reader_app.emit(
                &reader_sid,
                json!({ "type": "error", "message": "The Claude sidecar exited. The next message starts it again." }),
            );
            reader_app.emit(&reader_sid, json!({ "type": "turn_completed", "status": "failed" }));
        }
    });

    let restricted = crate::private::restricted(app, sid);
    // An agent's conversation gets Switchyard's tools as an MCP server.
    let tools = crate::agent_tools::launch(app, sid).filter(|_| !restricted).map(|l| l.claude());
    // An agent's or automation's session gets only its connectors (see `scope`).
    let mcp = crate::scope::of_session(app, sid).filter(|_| !restricted).map(|s| s.to_json());
    let _ = tx.send(
        json!({
            "type": "start",
            "restricted": restricted,
            // What a sidecar from before agents reads; it restricts only in Plan,
            // which a restricted session always is.
            "switchy": restricted,
            "cwd": meta.cwd,
            "model": meta.model,
            "effort": meta.effort,
            "permission": crate::origins::turn_permission(&meta.permission, &meta.answering),
            "resume": resume,
            "tools": tools,
            "mcp": mcp,
        })
        .to_string(),
    );
    Ok(())
}

fn line(app: &Arc<App>, sid: &str, msg: Value) -> Result<()> {
    let map = app.sessions.lock().unwrap();
    match map.get(sid).map(|s| &s.backend) {
        Some(Backend::Claude { tx, .. }) => tx
            .send(msg.to_string())
            .map_err(|_| anyhow::anyhow!("the Claude sidecar is not running")),
        _ => bail!("session has no Claude sidecar"),
    }
}

/// The sidecar reads image attachments from disk and builds the content
/// blocks itself; yardd only passes the records.
pub fn send(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) -> Result<()> {
    // A session restored after a restart gets a fresh sidecar that resumes
    // the saved Claude Code session.
    // So does a session whose sidecar was dropped after a login, which may
    // have no Claude Code session to resume yet.
    let pending = {
        let map = app.sessions.lock().unwrap();
        map.get(sid)
            .filter(|s| matches!(s.backend, Backend::Pending))
            .map(|s| s.engine_ref.clone())
    };
    if let Some(resume) = pending {
        spawn(app, sid, resume)?;
    }
    // Each turn names its mode, so a turn on a watched channel's notice runs
    // in Plan and the next one goes back to the session's (the sidecar
    // switches before the message reaches Claude Code).
    let permission = app.meta(sid).map(|m| crate::origins::turn_permission(&m.permission, &m.answering)).unwrap_or_default();
    line(app, sid, json!({ "type": "user", "text": text, "attachments": attachments, "permission": permission }))
}

/// A message for the running turn: Claude Code folds input that arrives
/// mid-turn into it at its next step (checked against a real turn,
/// 2026-11-15), so there is no separate turn to start.
pub fn push(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) -> Result<()> {
    if !app.meta(sid).is_some_and(|m| m.status == "running") {
        anyhow::bail!("the turn already ended");
    }
    line(app, sid, json!({ "type": "user", "text": text, "attachments": attachments }))
}

pub fn set_model(app: &Arc<App>, sid: &str, model: &str) -> Result<()> {
    line(app, sid, json!({ "type": "set_model", "model": model }))
}

pub fn set_permission(app: &Arc<App>, sid: &str, permission: &str) -> Result<()> {
    line(app, sid, json!({ "type": "set_permission", "permission": permission }))
}

pub fn interrupt(app: &Arc<App>, sid: &str) -> Result<()> {
    line(app, sid, json!({ "type": "interrupt" }))
}

/// Kill the session's sidecar, for a turn that ignored an interrupt. The
/// conversation is kept: the next message starts a sidecar that resumes it.
pub fn kill(app: &Arc<App>, sid: &str) {
    let old = {
        let mut map = app.sessions.lock().unwrap();
        match map.get_mut(sid) {
            Some(s) if matches!(s.backend, Backend::Claude { .. }) => {
                Some(std::mem::replace(&mut s.backend, Backend::Pending))
            }
            _ => None,
        }
    };
    // Dropping it fires `_stop`, and the reader kills the process.
    drop(old);
}

/// Stop an idle session's sidecar, so its Claude Code process and the MCP
/// connections it holds go away (see `release`). Like `kill`, the next message
/// starts one that resumes the conversation; the session was idle, so the
/// sidecar's exit logs nothing. False if the session is not quiet.
pub fn release(app: &Arc<App>, sid: &str) -> bool {
    let old = {
        let mut map = app.sessions.lock().unwrap();
        match map.get_mut(sid) {
            Some(s) if matches!(s.backend, Backend::Claude { .. }) && crate::release::quiet(&s.meta) => {
                Some(std::mem::replace(&mut s.backend, Backend::Pending))
            }
            _ => None,
        }
    };
    let released = old.is_some();
    drop(old);
    released
}

/// `answers`: a question's answers (AskUserQuestion), logged with the decision
/// so the card shows them after a reload.
pub fn approve(app: &Arc<App>, sid: &str, request_id: &str, decision: &str, answers: Option<&Value>) -> Result<()> {
    // Logged before the sidecar hears the answer: a tool that runs and a turn
    // that ends right after it must come later in the log, not before it.
    let tx = {
        let map = app.sessions.lock().unwrap();
        match map.get(sid).map(|s| &s.backend) {
            Some(Backend::Claude { tx, .. }) if !tx.is_closed() => tx.clone(),
            Some(Backend::Claude { .. }) => bail!("the Claude sidecar is not running"),
            _ => bail!("session has no Claude sidecar"),
        }
    };
    let mut resolved = json!({ "type": "approval_resolved", "requestId": request_id, "decision": decision });
    let mut line = json!({ "type": "permission", "requestId": request_id, "decision": decision });
    if let Some(answers) = answers {
        resolved["answers"] = answers.clone();
        line["answers"] = answers.clone();
    }
    app.emit(sid, resolved);
    tx.send(line.to_string())
        .map_err(|_| anyhow::anyhow!("the Claude sidecar is not running"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sidecar may run the tool and end the turn the moment it reads the
    /// answer, so `approval_resolved` must already be in the log by then. Logged
    /// after, it set the finished session back to `running` and the next
    /// message waited in the queue for a turn that never ended.
    #[test]
    fn an_answer_is_logged_before_the_sidecar_reads_it() {
        let dir = std::env::temp_dir().join(format!("yardd-unit-{}-approve", std::process::id()));
        let (store, _) = crate::store::open(&dir).unwrap();
        let app = Arc::new(App::new(dir.clone(), 0, store));
        let sid = app.new_session("claude", "claude-opus-5-5", "high", "/tmp", "t", None, crate::accounts::DEFAULT);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let (stop, _keep) = oneshot::channel();
        app.sessions.lock().unwrap().get_mut(&sid).unwrap().backend = Backend::Claude { tx, _stop: stop };
        app.emit(&sid, json!({ "type": "approval_request", "requestId": "r1" }));

        approve(&app, &sid, "r1", "allow", None).unwrap();
        // What the sidecar reads, and what the log holds at that moment.
        let line: Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(line["type"], "permission");
        let logged: Vec<Value> = app.sessions.lock().unwrap()[&sid].events.iter().map(|e| e["type"].clone()).collect();
        assert_eq!(logged, [json!("approval_request"), json!("approval_resolved")]);
        assert_eq!(app.meta(&sid).unwrap().status, "running");
        let _ = std::fs::remove_dir_all(dir);
    }
}
