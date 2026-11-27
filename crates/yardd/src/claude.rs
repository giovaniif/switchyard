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

/// How often each Claude and Grok account's plan limits are read without a
/// turn. `YARD_CLAUDE_USAGE_EVERY_MS=0` turns it off (the tests do, but one).
pub fn usage_every() -> Option<std::time::Duration> {
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

/// A one-shot of the sidecar (`usage`, `models`) on an account: its one JSON line.
async fn one_shot(app: &Arc<App>, account: &str, mode: &str) -> Result<Value> {
    let mut cmd = sidecar_command(app)?;
    cmd.arg(mode).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    if let Some(dir) = app.accounts.config_dir("claude", account)? {
        cmd.env(crate::accounts::env_var("claude"), dir);
    }
    let out = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| anyhow::anyhow!("no answer within 60 s"))?
        .context("could not start the Claude sidecar")?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('{')).unwrap_or("");
    let v: Value = serde_json::from_str(line).with_context(|| format!("no {mode} line"))?;
    if let Some(e) = v["error"].as_str() {
        bail!("{e}");
    }
    Ok(v)
}

pub async fn refresh_usage(app: &Arc<App>, account: &str) -> Result<()> {
    let v = one_shot(app, account, "usage").await?;
    // An account with no plan limits (an API key) has none to show.
    if v["usage"]["windows"].is_object() {
        app.set_usage("claude", account, v["usage"].clone());
    }
    Ok(())
}

/// How often each Claude account's model list is read again. A read runs no
/// turn: the list is Claude Code's answer when it starts.
/// `YARD_CLAUDE_MODELS_EVERY_MS=0` turns the reads off (the tests do, but
/// two), leaving the saved or built-in list.
pub fn models_every() -> Option<std::time::Duration> {
    match std::env::var("YARD_CLAUDE_MODELS_EVERY_MS").ok().and_then(|v| v.trim().parse::<u64>().ok()) {
        Some(0) => None,
        Some(ms) => Some(std::time::Duration::from_millis(ms)),
        None => Some(std::time::Duration::from_secs(24 * 3600)),
    }
}

/// What Claude lists before Claude Code has answered, or when it cannot.
pub fn fallback_models() -> Value {
    let efforts = json!(["low", "medium", "high", "xhigh", "max"]);
    json!([
        { "id": "claude-opus-5-5", "name": "Opus 5.5", "efforts": efforts, "defaultEffort": "high" },
        { "id": "claude-fable-5-2", "name": "Fable 5.2", "efforts": efforts, "defaultEffort": "high" },
        { "id": "claude-sonnet-5-5", "name": "Sonnet 5.5", "efforts": efforts, "defaultEffort": "medium" },
        { "id": "claude-haiku-5-5", "name": "Haiku 5.5", "efforts": efforts, "defaultEffort": "medium" },
    ])
}

fn models_file() -> std::path::PathBuf {
    crate::store::data_dir().join("claude-models.json")
}

/// Each account's last list, `{ account: { at, models } }`, kept across
/// restarts so the picker does not fall back to the built-in list meanwhile.
pub fn load_model_lists() -> Value {
    std::fs::read(models_file())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn save_model_lists(lists: &Value) {
    // Written aside, then renamed, so a crash never leaves half a file.
    let file = models_file();
    let tmp = file.with_extension("json.tmp");
    if std::fs::write(&tmp, lists.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, file);
    }
}

/// Default, then every added Claude account.
pub fn claude_accounts(accounts: &crate::accounts::Accounts) -> Vec<String> {
    let mut all = vec![crate::accounts::DEFAULT.to_string()];
    all.extend(accounts.all().into_iter().filter(|a| a.engine == "claude").map(|a| a.id));
    all
}

/// The picker's Claude list from each account's: Default's order, then what
/// only other accounts have. Plans differ, so a model that not every account
/// reports names the ones that do (`accounts`). None read yet: the built-in list.
pub fn merged_models(lists: &Value, accounts: &[String]) -> Value {
    let read: Vec<(&str, &Vec<Value>)> = accounts
        .iter()
        .filter_map(|a| lists[a.as_str()]["models"].as_array().filter(|l| !l.is_empty()).map(|l| (a.as_str(), l)))
        .collect();
    if read.is_empty() {
        return fallback_models();
    }
    let mut out: Vec<Value> = Vec::new();
    for (i, (_, list)) in read.iter().enumerate() {
        for m in list.iter() {
            if out.iter().any(|o| o["id"] == m["id"]) {
                continue;
            }
            let mut m = m.clone();
            if let Some(o) = m.as_object_mut() {
                o.remove("accounts");
                // The plan's default is the first account's.
                if i > 0 {
                    o.remove("isDefault");
                }
            }
            out.push(m);
        }
    }
    if read.len() > 1 {
        for m in &mut out {
            let has: Vec<&str> = read.iter().filter(|(_, l)| l.iter().any(|x| x["id"] == m["id"])).map(|(a, _)| *a).collect();
            if has.len() < read.len() {
                m["accounts"] = json!(has);
            }
        }
    }
    json!(out)
}

/// Put the merged list in the picker, telling every window when it changed.
pub fn publish_models(app: &App) {
    let list = merged_models(&app.claude_models.lock().unwrap(), &claude_accounts(&app.accounts));
    let all = {
        let mut current = app.models.lock().unwrap();
        if current["claude"] == list {
            return;
        }
        current["claude"] = list;
        current.clone()
    };
    app.broadcast(json!({ "op": "models", "models": all }));
}

/// Read one account's models from Claude Code (`yard-claude models`). A failed
/// read keeps what the account had.
pub async fn refresh_models(app: &Arc<App>, account: &str) -> Result<()> {
    let v = one_shot(app, account, "models").await?;
    let models: Vec<Value> = v["models"]
        .as_array()
        .map(|l| l.iter().filter(|m| m["id"].as_str().is_some_and(|id| !id.is_empty()) && m["name"].is_string()).cloned().collect())
        .unwrap_or_default();
    if models.is_empty() {
        bail!("Claude Code reported no models");
    }
    let lists = {
        let known = claude_accounts(&app.accounts);
        let mut lists = app.claude_models.lock().unwrap();
        lists[account] = json!({ "at": crate::state::now_ms(), "models": models });
        // A removed account's list goes with it.
        if let Some(map) = lists.as_object_mut() {
            map.retain(|a, _| known.contains(a));
        }
        lists.clone()
    };
    save_model_lists(&lists);
    publish_models(app);
    Ok(())
}

/// Read every Claude account's models at start, then now and then: a plan
/// gains models (Fable) without Switchyard changing.
pub async fn run_models_refresh(app: Arc<App>) {
    let Some(every) = models_every() else { return };
    loop {
        for account in claude_accounts(&app.accounts) {
            if let Err(e) = refresh_models(&app, &account).await {
                eprintln!("yardd: claude models for {account}: {e:#}");
            }
        }
        tokio::time::sleep(every).await;
    }
}

/// Read one account's models now (a new account, a fresh login), unless the
/// reads are turned off.
pub fn refresh_models_soon(app: &Arc<App>, account: &str) {
    if models_every().is_none() {
        return;
    }
    let (app, account) = (app.clone(), account.to_string());
    tokio::spawn(async move {
        if let Err(e) = refresh_models(&app, &account).await {
            eprintln!("yardd: claude models for {account}: {e:#}");
        }
    });
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
                        // A turn names only the windows it touched.
                        reader_app.update_usage("claude", &account, |kept| crate::usage::merge_claude_turn(kept, &msg["usage"]));
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
        if current {
            reader_app.fail_open_turn(&reader_sid, "The Claude sidecar exited. The next message starts it again.");
        }
    });

    let restricted = crate::private::restricted(app, sid);
    // A session gets Switchyard's tools as an MCP server: an agent's, or `request_connector` alone.
    let tools = crate::agent_tools::launch(app, sid).filter(|_| !restricted).map(|l| l.claude());
    // An agent's or automation's session gets only its connectors (see `scope`).
    let mcp = crate::scope::of_session(app, sid).filter(|_| !restricted).map(|s| s.to_json());
    let _ = tx.send(
        json!({
            "type": "start",
            "restricted": restricted,
            "slackGuest": sid.starts_with("private-slack-"),
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
    // No sidecar (let go while idle, or restored after a restart): the next
    // one starts on the session's model, saved by the caller.
    let pending = matches!(app.sessions.lock().unwrap().get(sid).map(|s| &s.backend), Some(Backend::Pending));
    if pending {
        return Ok(());
    }
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

    #[test]
    fn models_merge_every_accounts_list_and_name_the_accounts_of_one_not_all_have() {
        let m = |id: &str, default: bool| json!({ "id": id, "name": id, "efforts": [], "isDefault": default });
        let accounts = ["default".to_string(), "work".to_string()];
        assert_eq!(merged_models(&json!({}), &accounts), fallback_models());
        let lists = json!({
            "default": { "models": [m("opus", true), m("fable", false)] },
            "work": { "models": [m("sonnet", true), m("opus", false)] },
            "gone": { "models": [m("removed", false)] },
        });
        assert_eq!(
            merged_models(&lists, &accounts),
            json!([
                m("opus", true),
                { "id": "fable", "name": "fable", "efforts": [], "isDefault": false, "accounts": ["default"] },
                { "id": "sonnet", "name": "sonnet", "efforts": [], "accounts": ["work"] },
            ])
        );
        // One account read: nothing to name.
        assert_eq!(merged_models(&lists, &accounts[..1]), json!([m("opus", true), m("fable", false)]));
    }

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
