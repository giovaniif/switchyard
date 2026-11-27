//! Engine logins from the app, so a server or the web version never needs a
//! terminal for them. Codex logs in through its own app-server (device code:
//! a link and a code to enter). Claude Code has no such API, so
//! `claude auth login` runs in a PTY: its sign-in link is shown in the app,
//! and the code the page gives back is typed into it.

use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde_json::{json, Value};
use tokio::process::Command;

use crate::{
    accounts::DEFAULT,
    state::{App, Backend},
};

struct ClaudeLogin {
    id: u64,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn ChildKiller + Send + Sync>,
    timed_out: bool,
}

/// How long a Claude login waits for its code. A closed tab never sends
/// `login.cancel`, and ending a login when its window's socket drops would
/// break a phone that leaves the app for the sign-in page, so it ends on time.
fn claude_login_timeout() -> Duration {
    Duration::from_millis(std::env::var("YARD_CLAUDE_LOGIN_TIMEOUT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(15 * 60 * 1000))
}

/// Logins in progress are per account: Default and an added one can run at once.
#[derive(Default)]
pub struct Logins {
    claude: Mutex<HashMap<String, ClaudeLogin>>,
    /// account -> Codex login id
    codex: Mutex<HashMap<String, String>>,
    pub opencode: crate::opencode::Logins,
    pub grok: crate::grok::Logins,
    pub next: std::sync::atomic::AtomicU64,
}

pub fn report(app: &App, engine: &str, account: &str, state: &str, extra: Value) {
    let mut msg = json!({ "op": "login", "engine": engine, "account": account, "state": state });
    if let (Some(m), Some(e)) = (msg.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    app.broadcast(msg);
}

/// Whether each engine is installed and logged in, for the app to offer a login.
/// `engines` is each engine's Default login; `accounts` has the added ones by id.
pub async fn status(app: &Arc<App>) -> Value {
    let added = app.accounts.all();
    let one = |engine: String, account: String| async move { (engine.clone(), account.clone(), engine_status(app, &engine, &account).await) };
    let (defaults, rest) = tokio::join!(
        futures_util::future::join_all(crate::engines::ALL.iter().map(|e| one(e.to_string(), DEFAULT.into()))),
        futures_util::future::join_all(added.iter().map(|a| one(a.engine.clone(), a.id.clone()))),
    );
    let engines: serde_json::Map<String, Value> = defaults.into_iter().map(|(engine, _, state)| (engine, state)).collect();
    let accounts: serde_json::Map<String, Value> = rest.into_iter().map(|(_, account, state)| (account, state)).collect();
    json!({ "op": "engines", "engines": engines, "accounts": accounts })
}

async fn engine_status(app: &Arc<App>, engine: &str, account: &str) -> Value {
    match engine {
        "codex" => codex_status(app, account).await,
        "opencode" => crate::opencode::status(app, account).await,
        "grok" => crate::grok::status(app, account).await,
        "gemini" => crate::gemini::status(app, account).await,
        _ => claude_status(app, account).await,
    }
}

async fn claude_status(app: &Arc<App>, account: &str) -> Value {
    let mut cmd = Command::new("claude");
    cmd.args(["auth", "status"]).kill_on_drop(true);
    strip_claude_env_tokio(&mut cmd);
    match app.accounts.config_dir("claude", account) {
        Ok(Some(dir)) => {
            cmd.env(crate::accounts::env_var("claude"), dir);
        }
        Ok(None) => {}
        Err(e) => return json!({ "installed": true, "loggedIn": null, "error": e.to_string() }),
    }
    match tokio::time::timeout(Duration::from_secs(20), cmd.output()).await {
        Err(_) => json!({ "installed": true, "loggedIn": null, "error": "`claude auth status` did not answer" }),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => json!({ "installed": false, "loggedIn": false }),
        Ok(Err(e)) => json!({ "installed": true, "loggedIn": null, "error": e.to_string() }),
        Ok(Ok(out)) => {
            // Logged out exits 1 but still prints the JSON.
            let v: Value = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
            match v["loggedIn"].as_bool() {
                Some(logged_in) => json!({
                    "installed": true,
                    "loggedIn": logged_in,
                    "account": v["email"].as_str().or_else(|| v["account"]["email"].as_str()),
                }),
                None => json!({ "installed": true, "loggedIn": null, "error": "could not read `claude auth status`" }),
            }
        }
    }
}

async fn codex_status(app: &Arc<App>, account: &str) -> Value {
    let client = match crate::codex::client(app, account).await {
        Ok(c) => c,
        Err(_) => return json!({ "installed": false, "loggedIn": false }),
    };
    match client.request("account/read", json!({})).await {
        Ok(res) => {
            let account = &res["account"];
            json!({
                "installed": true,
                "loggedIn": !account.is_null(),
                "account": account["email"].as_str(),
            })
        }
        Err(e) => json!({ "installed": true, "loggedIn": null, "error": format!("{e:#}") }),
    }
}

pub async fn start(app: &Arc<App>, engine: &str, account: &str) -> Result<()> {
    app.accounts.check(engine, account)?;
    match engine {
        "codex" => start_codex(app, account).await,
        "claude" => start_claude(app, account),
        "opencode" => crate::opencode::start(app, account).await,
        "grok" => crate::grok::start(app, account),
        "gemini" => crate::gemini::start(app, account).await,
        _ => bail!("unknown engine {engine}"),
    }
}

async fn start_codex(app: &Arc<App>, account: &str) -> Result<()> {
    let client = crate::codex::client(app, account).await.context("Codex is not installed here")?;
    let old = app.logins.codex.lock().unwrap().remove(account);
    if let Some(old) = old {
        let _ = client.request("account/login/cancel", json!({ "loginId": old })).await;
    }
    let res = client
        .request("account/login/start", json!({ "type": "chatgptDeviceCode" }))
        .await?;
    let login_id = res["loginId"].as_str().context("Codex sent no login id")?.to_string();
    app.logins.codex.lock().unwrap().insert(account.to_string(), login_id);
    report(app, "codex", account, "code", json!({ "url": res["verificationUrl"], "code": res["userCode"] }));
    Ok(())
}

/// `account/login/completed` from the Codex app-server.
pub fn codex_completed(app: &Arc<App>, account: &str, p: &Value) {
    let ours = {
        let mut current = app.logins.codex.lock().unwrap();
        let ours = current.get(account).is_some_and(|id| p["loginId"].as_str().map_or(true, |got| got == id));
        if ours {
            current.remove(account);
        }
        ours
    };
    if !ours {
        return;
    }
    if p["success"].as_bool() == Some(true) {
        report(app, "codex", account, "done", json!({}));
        let app = app.clone();
        let account = account.to_string();
        tokio::spawn(async move {
            // Models and limits depend on the account.
            let _ = crate::codex::load_models(&app, &account).await;
            app.broadcast(status(&app).await);
        });
    } else {
        let error = p["error"].as_str().unwrap_or("The login did not finish.");
        report(app, "codex", account, "failed", json!({ "error": error }));
    }
}

fn start_claude(app: &Arc<App>, account: &str) -> Result<()> {
    cancel_claude(app, account);
    // Wide, so the link is never wrapped.
    let pair = native_pty_system().openpty(PtySize { cols: 2000, rows: 40, pixel_width: 0, pixel_height: 0 })?;
    let mut cmd = CommandBuilder::new("claude");
    cmd.args(["auth", "login"]);
    if let Some(home) = std::env::var_os("HOME") {
        cmd.cwd(home);
    }
    cmd.env("TERM", "xterm-256color");
    if let Some(dir) = app.accounts.config_dir("claude", account)? {
        cmd.env(crate::accounts::env_var("claude"), dir);
    }
    for (key, _) in std::env::vars() {
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") || key.starts_with("CLAUDE_AGENT_SDK") {
            cmd.env_remove(&key);
        }
    }
    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("could not start `claude auth login`: is Claude Code installed here?")?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;
    let id = app.logins.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    app.logins.claude.lock().unwrap().insert(account.to_string(), ClaudeLogin { id, writer, killer: child.clone_killer(), timed_out: false });
    report(app, "claude", account, "starting", json!({}));
    {
        let app = app.clone();
        let account = account.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(claude_login_timeout()).await;
            if let Some(login) = app.logins.claude.lock().unwrap().get_mut(&account).filter(|l| l.id == id) {
                login.timed_out = true;
                let _ = login.killer.kill();
            }
        });
    }

    let app = app.clone();
    let account = account.to_string();
    let runtime = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let _master = pair.master; // the PTY stays open while the login runs
        let mut text = String::new();
        let mut url_sent = false;
        let mut chunk = [0u8; 8192];
        loop {
            let n = match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            text.push_str(&strip_ansi(&String::from_utf8_lossy(&chunk[..n])));
            if !url_sent {
                if let Some(url) = sign_in_url(&text) {
                    url_sent = true;
                    report(&app, "claude", &account, "url", json!({ "url": url }));
                }
            }
            // A wrong code leaves it prompting again; say so and stop.
            if text.contains("Invalid code") {
                let _ = app.logins.claude.lock().unwrap().get_mut(&account).map(|l| l.killer.kill());
                break;
            }
        }
        let ok = child.wait().map(|s| s.success()).unwrap_or(false);
        let timed_out = {
            let mut slot = app.logins.claude.lock().unwrap();
            if slot.get(&account).map(|l| l.id) == Some(id) {
                slot.remove(&account).map(|l| l.timed_out)
            } else {
                None
            }
        };
        let Some(timed_out) = timed_out else {
            return; // replaced or cancelled
        };
        let logged_in = ok && !text.contains("Invalid code");
        if logged_in {
            report(&app, "claude", &account, "done", json!({}));
            restart_idle_claude(&app, &account);
        } else {
            let error = if timed_out {
                "The sign-in waited too long for its code. Start again.".to_string()
            } else if text.contains("Invalid code") {
                "That code was not accepted. Start again and paste the whole code.".to_string()
            } else {
                last_line(&text).unwrap_or_else(|| "The login did not finish.".into())
            };
            report(&app, "claude", &account, "failed", json!({ "error": error }));
        }
        // This is a plain thread (it reads a terminal): anything async goes
        // through the runtime. The new login's limits come without a turn.
        runtime.spawn(async move {
            if logged_in {
                crate::claude::refresh_usage_soon(&app, &account);
                // Another login may be another plan, with other models.
                crate::claude::refresh_models_soon(&app, &account);
            }
            app.broadcast(status(&app).await)
        });
    });
    Ok(())
}

/// The code from the sign-in page, typed into `claude auth login`; for
/// OpenCode, a provider's API key or sign-in code.
pub async fn code(app: &Arc<App>, engine: &str, account: &str, code: &str) -> Result<()> {
    let code = code.trim();
    if code.is_empty() || code.contains(['\r', '\n']) {
        bail!("paste the code from the sign-in page");
    }
    if engine == "opencode" {
        return crate::opencode::code(app, account, code).await;
    }
    if engine != "claude" {
        bail!("only Claude Code and OpenCode take a code");
    }
    let mut slot = app.logins.claude.lock().unwrap();
    let login = slot.get_mut(account).context("no Claude login is running; start again")?;
    login.writer.write_all(code.as_bytes())?;
    login.writer.write_all(b"\r")?;
    login.writer.flush()?;
    drop(slot);
    report(app, "claude", account, "checking", json!({}));
    Ok(())
}

pub async fn cancel(app: &Arc<App>, engine: &str, account: &str) -> Result<()> {
    match engine {
        "claude" => cancel_claude(app, account),
        "opencode" => crate::opencode::cancel(app, account),
        "grok" => crate::grok::cancel(app, account),
        "gemini" => {}, // The CLI login runs in the user's terminal.
        "codex" => {
            let old = app.logins.codex.lock().unwrap().remove(account);
            if let (Some(id), Some(client)) = (old, app.codex_running(account)) {
                let _ = client.request("account/login/cancel", json!({ "loginId": id })).await;
            }
        }
        _ => bail!("unknown engine {engine}"),
    }
    report(app, engine, account, "cancelled", json!({}));
    Ok(())
}

fn cancel_claude(app: &Arc<App>, account: &str) {
    if let Some(mut old) = app.logins.claude.lock().unwrap().remove(account) {
        let _ = old.killer.kill();
    }
}

/// A sidecar started while logged out keeps failing, so idle Claude sessions
/// get a fresh one with their next message.
fn restart_idle_claude(app: &Arc<App>, account: &str) {
    // The connectors process too: it holds the old login's connector list.
    crate::mcp::forget(app, account);
    let mut map = app.sessions.lock().unwrap();
    for s in map.values_mut() {
        if matches!(s.backend, Backend::Claude { .. }) && s.meta.status == "idle" && s.meta.account == account {
            s.backend = Backend::Pending;
        }
    }
}

pub fn strip_claude_env_tokio(cmd: &mut Command) {
    for (key, _) in std::env::vars() {
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") || key.starts_with("CLAUDE_AGENT_SDK") {
            cmd.env_remove(key);
        }
    }
}

/// Drop terminal escapes: CSI (`ESC [ … final`) and OSC (`ESC ] … BEL` or
/// `ESC ] … ESC \`), which is how the CLI wraps its link.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn sign_in_url(text: &str) -> Option<String> {
    let start = text.find("https://")?;
    let url: String = text[start..].chars().take_while(|c| !c.is_whitespace()).collect();
    // The whole link has arrived once something follows it.
    (text.len() > start + url.len() && url.contains("oauth")).then_some(url)
}

pub fn last_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("Paste code"))
        .last()
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_sign_in_link_in_the_cli_output() {
        let raw = "Opening browser to sign in…\r\nIf the browser didn't open, visit: \x1b]8;;https://claude.com/cai/oauth/authorize?code=true&state=abc\x07https://claude.com/cai/oauth/authorize?code=true&state=abc\x1b]8;;\x07\r\nPaste code here if prompted > ";
        let text = strip_ansi(raw);
        assert!(text.contains("visit: https://claude.com/cai/oauth/authorize?code=true&state=abc\r\n"));
        assert_eq!(sign_in_url(&text).as_deref(), Some("https://claude.com/cai/oauth/authorize?code=true&state=abc"));
        // Half a link is not a link yet.
        assert_eq!(sign_in_url("visit: https://claude.com/cai/oau"), None);
    }
}
