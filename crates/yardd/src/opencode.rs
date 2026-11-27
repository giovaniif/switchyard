//! OpenCode beyond its sessions (which `acp` drives): its model list, whether
//! it can run, and logging in to its providers from the app.
//!
//! OpenCode runs on any of many providers, each with its own credentials, and
//! works with none (its free models). "Logging in" adds a provider. There is
//! no CLI flow to drive without a terminal UI, so a short-lived
//! `opencode serve` on loopback (with a password of ours) does it through its
//! own endpoints: `/provider/auth` lists the ways in, `oauth/authorize` and
//! `oauth/callback` run a sign-in, `PUT /auth/<provider>` saves an API key.
//! OpenCode keeps the credentials in its own `auth.json`; yardd only passes a
//! pasted key through, never stores or logs it.

use std::{collections::HashMap, process::Stdio, sync::Arc, time::Duration};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use jsonc_parser::cst::{CstInputValue, CstRootNode};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

use crate::{accounts::DEFAULT, login::report, state::App};

/// Providers most people mean, listed first in the login dialog.
const POPULAR: [&str; 8] = ["anthropic", "openai", "github-copilot", "xai", "google", "openrouter", "opencode", "deepseek"];

/// `opencode` with the account's data folder (its logins and sessions).
fn command(app: &App, account: &str) -> Result<Command> {
    let mut cmd = Command::new("opencode");
    if let Some(dir) = app.accounts.config_dir("opencode", account)? {
        cmd.env(crate::accounts::env_var("opencode"), dir);
    }
    cmd.stdin(Stdio::null()).kill_on_drop(true);
    Ok(cmd)
}

/// Both CLIs print one `provider/model` id per line. V1's verbose output
/// adds the name and variants as indented JSON after each id.
pub fn parse_models(text: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let mut id: Option<String> = None;
    let mut body = String::new();
    let mut flush = |id: &mut Option<String>, body: &mut String| {
        if let Some(full) = id.take() {
            let m: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let provider = full.split('/').next().unwrap_or("");
            let name = m["name"].as_str().map_or_else(|| full.clone(), |name| format!("{name} ({provider})"));
            let mut efforts: Vec<String> = m["variants"].as_object().map(|v| v.keys().cloned().collect()).unwrap_or_default();
            // Lowest to highest; the JSON's own order is lost in parsing.
            const ORDER: [&str; 7] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
            efforts.sort_by_key(|e| (ORDER.iter().position(|o| o == e).unwrap_or(ORDER.len()), e.clone()));
            // A model with variants runs as itself unless one is picked.
            if !efforts.is_empty() {
                efforts.insert(0, "default".into());
            }
            let mut model = json!({ "id": full, "name": name, "efforts": efforts });
            if !efforts.is_empty() {
                model["defaultEffort"] = json!("default");
            }
            out.push(model);
        }
        body.clear();
    };
    for line in text.lines() {
        let top = !line.starts_with([' ', '\t', '{', '}', '[', ']', '"']) && !line.trim().is_empty();
        if top && line.contains('/') && !line.contains(' ') {
            flush(&mut id, &mut body);
            id = Some(line.trim().to_string());
        } else if id.is_some() {
            body.push_str(line);
            body.push('\n');
        }
    }
    flush(&mut id, &mut body);
    out
}

async fn run(app: &App, account: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut cmd = command(app, account).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::null());
    match tokio::time::timeout(Duration::from_secs(60), cmd.output()).await {
        Ok(out) => out,
        Err(_) => Err(std::io::Error::new(std::io::ErrorKind::TimedOut, format!("`opencode {}` did not answer", args.join(" ")))),
    }
}

/// Both CLIs ask before these tools so that yardd can apply the session's
/// permission mode. OpenCode 2 reads ordered rules in its inline config.
pub(crate) async fn configure_agent(app: &App, account: &str, cmd: &mut Command) -> Result<()> {
    let out = run(app, account, &["--version"]).await.context("could not check the OpenCode version")?;
    if !out.status.success() {
        bail!("`opencode --version` failed");
    }
    let version = String::from_utf8_lossy(&out.stdout);
    let major = version
        .split_whitespace()
        .last()
        .and_then(|v| v.trim_start_matches('v').split('.').next())
        .and_then(|v| v.parse::<u32>().ok());
    if major.is_some_and(|v| v >= 2) {
        let content = std::env::var_os("OPENCODE_CONFIG_CONTENT").unwrap_or_default();
        let content = content.to_str().context("OpenCode's inline config is not UTF-8")?;
        cmd.env("OPENCODE_CONFIG_CONTENT", v2_permission_config(content)?);
    } else {
        cmd.env("OPENCODE_PERMISSION", r#"{"edit":"ask","bash":"ask","webfetch":"ask"}"#);
    }
    Ok(())
}

fn v2_permission_config(content: &str) -> Result<String> {
    let root = CstRootNode::parse(content, &Default::default())
        .map_err(|_| anyhow::anyhow!("OpenCode's inline config is invalid JSONC"))?;
    let config = root.object_value_or_create().context("OpenCode's inline config must be an object")?;
    let permissions = config.array_value_or_create("permissions").context("OpenCode's inline permissions must be an array")?;
    for action in ["edit", "shell", "webfetch"] {
        permissions.append(CstInputValue::Object(vec![
            ("action".into(), action.into()),
            ("resource".into(), "*".into()),
            ("effect".into(), "ask".into()),
        ]));
    }
    Ok(root.to_string())
}

/// The models of Default's providers. Added accounts share the list, like Codex's.
pub async fn load_models(app: &Arc<App>) -> Result<()> {
    let mut out = run(app, DEFAULT, &["models", "--verbose"]).await?;
    if !out.status.success() {
        // OpenCode 2 removed --verbose, but its plain catalog still has the
        // exact model IDs the ACP agent takes.
        out = run(app, DEFAULT, &["models"]).await?;
    }
    if !out.status.success() {
        bail!("`opencode models` failed");
    }
    let models = parse_models(&String::from_utf8_lossy(&out.stdout));
    let all = {
        let mut current = app.models.lock().unwrap();
        current["opencode"] = json!(models);
        current.clone()
    };
    app.broadcast(json!({ "op": "models", "models": all }));
    Ok(())
}

/// Installed, and which providers it can use. It runs with none of its own
/// (OpenCode's free models), so "logged in" means it has any model at all.
pub async fn status(app: &Arc<App>, account: &str) -> Value {
    match run(app, account, &["models"]).await {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({ "installed": false, "loggedIn": false }),
        Err(e) => json!({ "installed": true, "loggedIn": null, "error": e.to_string() }),
        Ok(out) if !out.status.success() => json!({ "installed": true, "loggedIn": null, "error": "`opencode models` failed" }),
        Ok(out) => {
            let mut providers: Vec<String> = Vec::new();
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some((p, _)) = line.trim().split_once('/') {
                    if !providers.iter().any(|x| x == p) {
                        providers.push(p.to_string());
                    }
                }
            }
            json!({ "installed": true, "loggedIn": !providers.is_empty(), "providers": providers })
        }
    }
}

/// What the user picked and the server waits on.
enum Step {
    Choosing,
    Key { provider: String },
    Code { provider: String, method: u64 },
    Waiting,
}

/// A login in progress: its `opencode serve` (stopped when this is dropped),
/// the ways in it offered, and where the user is.
pub struct Login {
    port: u16,
    auth: String,
    _child: tokio::process::Child,
    /// Closing it stops the server.
    _stdin: tokio::process::ChildStdin,
    methods: Value,
    step: Step,
    waiter: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for Login {
    fn drop(&mut self) {
        if let Some(w) = self.waiter.take() {
            w.abort();
        }
    }
}

/// A request to the login server that gives up after a minute.
async fn http(port: u16, auth: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    http_within(Duration::from_secs(60), port, auth, method, path, body).await
}

/// The server accepts connections before it answers them, so every request
/// has a time limit: short while waiting for it to start, long for a sign-in.
async fn http_within(limit: Duration, port: u16, auth: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    match tokio::time::timeout(limit, exchange(port, auth, method, path, body)).await {
        Ok(res) => res,
        Err(_) => bail!("OpenCode's server did not answer {method} {path}"),
    }
}

/// A minimal HTTP/1.1 exchange with the loopback server: JSON in, JSON out.
async fn exchange(port: u16, auth: &str, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.context("OpenCode's server is not answering")?;
    let body = body.map(|b| b.to_string()).unwrap_or_default();
    let head = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Basic {auth}\r\nContent-Type: application/json\r\nAccept: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    // The server keeps the connection open whatever we ask, so the body ends
    // where its length says, not at EOF.
    let mut raw = Vec::new();
    let mut buf = [0u8; 16384];
    let (headers, mut payload) = loop {
        if let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
            let mut body = raw[split + 4..].to_vec();
            let length = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length:"))
                .and_then(|v| v.trim().parse::<usize>().ok());
            let chunked = headers.contains("transfer-encoding: chunked");
            loop {
                let complete = match length {
                    Some(n) => body.len() >= n,
                    None if chunked => body.ends_with(b"0\r\n\r\n"),
                    None => false,
                };
                if complete {
                    break;
                }
                let n = stream.read(&mut buf).await?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&buf[..n]);
            }
            if chunked {
                body = dechunk(&body);
            }
            break (headers, body);
        }
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            bail!("a malformed answer from OpenCode");
        }
        raw.extend_from_slice(&buf[..n]);
    };
    if let Some(n) = headers.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()) {
        payload.truncate(n);
    }
    let status: u16 = headers.split_whitespace().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let text = String::from_utf8_lossy(&payload).to_string();
    if !(200..300).contains(&status) {
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let why = v["data"]["message"].as_str().or(v["message"].as_str()).or(v["error"].as_str()).map(str::to_string);
        bail!("{}", why.unwrap_or_else(|| format!("OpenCode answered {status}")));
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

fn dechunk(mut data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(eol) = data.windows(2).position(|w| w == b"\r\n") {
        let size = usize::from_str_radix(String::from_utf8_lossy(&data[..eol]).trim(), 16).unwrap_or(0);
        if size == 0 || data.len() < eol + 2 + size {
            break;
        }
        out.extend_from_slice(&data[eol + 2..eol + 2 + size]);
        data = &data[(eol + 4 + size).min(data.len())..];
    }
    out
}

/// The inputs a method's prompts get without asking: each select's first
/// option. None when a text prompt applies, which needs the terminal.
fn default_inputs(method: &Value) -> Option<serde_json::Map<String, Value>> {
    let mut inputs = serde_json::Map::new();
    for p in method["prompts"].as_array().into_iter().flatten() {
        let when = &p["when"];
        if when.is_object() {
            let have = inputs.get(when["key"].as_str().unwrap_or("")).and_then(|v| v.as_str()).unwrap_or("");
            let is = have == when["value"].as_str().unwrap_or("");
            if (when["op"] == "eq") != is {
                continue;
            }
        }
        match p["type"].as_str() {
            Some("select") => {
                inputs.insert(p["key"].as_str()?.to_string(), p["options"][0]["value"].clone());
            }
            _ => return None,
        }
    }
    Some(inputs)
}

/// The ways into each provider that work from the app: an API key, or a
/// sign-in that needs no typed-in setting.
fn ways_in(providers: &Value, methods: &Value) -> Vec<Value> {
    let connected: Vec<&str> = providers["connected"].as_array().map(|l| l.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let mut list: Vec<Value> = providers["all"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let id = p["id"].as_str()?;
            let ways: Vec<Value> = match methods[id].as_array() {
                Some(list) => list
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| match m["type"].as_str() {
                        Some("api") => m["prompts"].as_array().map_or(true, |p| p.is_empty()),
                        _ => default_inputs(m).is_some(),
                    })
                    .map(|(i, m)| json!({ "index": i, "type": m["type"], "label": m["label"] }))
                    .collect(),
                None => vec![json!({ "index": 0, "type": "api", "label": "API key" })],
            };
            (!ways.is_empty()).then(|| {
                json!({ "id": id, "name": p["name"].as_str().unwrap_or(id), "connected": connected.contains(&id), "methods": ways })
            })
        })
        .collect();
    let rank = |p: &Value| {
        let id = p["id"].as_str().unwrap_or("");
        (!p["connected"].as_bool().unwrap_or(false), POPULAR.iter().position(|x| *x == id).unwrap_or(POPULAR.len()), p["name"].as_str().unwrap_or("").to_lowercase())
    };
    list.sort_by_key(rank);
    list
}

/// The login server's password. No randomness means no server: a guessable
/// password would let any local process add providers.
fn secret() -> Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 24];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .context("could not read /dev/urandom for the login server's password")?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

/// Start `opencode serve` for the account and offer its providers.
pub async fn start(app: &Arc<App>, account: &str) -> Result<()> {
    cancel(app, account);
    report(app, "opencode", account, "starting", json!({}));
    let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
    let password = secret()?;
    // `opencode serve` never reads its stdin, so it would outlive a yardd that
    // was killed. A shell in front of it stops it once its stdin closes: when
    // the login is dropped, or yardd is gone.
    let mut cmd = Command::new("sh");
    cmd.args(["-c", r#""$@" & pid=$!; read _; kill $pid 2>/dev/null; wait $pid"#, "sh", "opencode", "serve", "--hostname", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .env("OPENCODE_SERVER_USERNAME", "switchyard")
        .env("OPENCODE_SERVER_PASSWORD", &password)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(dir) = app.accounts.config_dir("opencode", account)? {
        cmd.env(crate::accounts::env_var("opencode"), dir);
    }
    let mut child = cmd.spawn().context("could not start `opencode serve`")?;
    let stdin = child.stdin.take().context("serve stdin")?;
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("switchyard:{password}"));
    let mut methods = None;
    let started = std::time::Instant::now();
    while started.elapsed() < Duration::from_secs(60) {
        if let Ok(m) = http_within(Duration::from_secs(5), port, &auth, "GET", "/provider/auth", None).await {
            methods = Some(m);
            break;
        }
        // Gone already: not installed, or it failed to start.
        if child.try_wait().ok().flatten().is_some() {
            bail!("`opencode serve` exited: is OpenCode installed here?");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let methods = methods.context("OpenCode's server did not start")?;
    let providers = http(port, &auth, "GET", "/provider", None).await?;
    let list = ways_in(&providers, &methods);
    app.logins.opencode.lock().unwrap().insert(
        account.to_string(),
        Login { port, auth, _child: child, _stdin: stdin, methods, step: Step::Choosing, waiter: None },
    );
    report(app, "opencode", account, "providers", json!({ "providers": list }));
    Ok(())
}

/// The way in the user picked: a key to paste, or a sign-in to open.
pub async fn choose(app: &Arc<App>, account: &str, provider: &str, index: u64) -> Result<()> {
    let (port, auth, method) = {
        let logins = app.logins.opencode.lock().unwrap();
        let l = logins.get(account).context("no OpenCode login is running; start again")?;
        let method = match l.methods[provider].as_array() {
            Some(list) => list.get(index as usize).cloned().context("no such way in")?,
            None => json!({ "type": "api", "label": "API key" }),
        };
        (l.port, l.auth.clone(), method)
    };
    if method["type"] == "api" {
        set_step(app, account, Step::Key { provider: provider.to_string() });
        report(app, "opencode", account, "key", json!({ "provider": provider, "label": method["label"] }));
        return Ok(());
    }
    let inputs = default_inputs(&method).context("this sign-in needs settings only the terminal asks for")?;
    let path = format!("/provider/{provider}/oauth/authorize");
    let res = http(port, &auth, "POST", &path, Some(&json!({ "method": index, "inputs": inputs }))).await?;
    let url = res["url"].as_str().unwrap_or("").to_string();
    if !url.starts_with("https://") && !url.starts_with("http://") {
        bail!("OpenCode gave no sign-in link");
    }
    let instructions = res["instructions"].as_str().unwrap_or("").to_string();
    if res["method"] == "code" {
        set_step(app, account, Step::Code { provider: provider.to_string(), method: index });
        report(app, "opencode", account, "url", json!({ "url": url, "instructions": instructions, "provider": provider }));
        return Ok(());
    }
    // `auto`: the server finishes by itself (a device code, or a callback it
    // receives); the callback request returns once it has.
    set_step(app, account, Step::Waiting);
    report(
        app,
        "opencode",
        account,
        "code",
        json!({ "url": url, "code": device_code(&instructions), "instructions": instructions, "provider": provider }),
    );
    let (app2, account2, provider2) = (app.clone(), account.to_string(), provider.to_string());
    let waiter = tokio::spawn(async move {
        let path = format!("/provider/{provider2}/oauth/callback");
        let res = http_within(Duration::from_secs(15 * 60), port, &auth, "POST", &path, Some(&json!({ "method": index }))).await;
        finish(&app2, &account2, res.map(|_| ()));
    });
    if let Some(l) = app.logins.opencode.lock().unwrap().get_mut(account) {
        l.waiter = Some(waiter);
    }
    Ok(())
}

fn set_step(app: &App, account: &str, step: Step) {
    if let Some(l) = app.logins.opencode.lock().unwrap().get_mut(account) {
        l.step = step;
    }
}

/// A device code such as `ABCD-1234` in a provider's instructions.
pub fn device_code(text: &str) -> Option<String> {
    text.split(|c: char| c.is_whitespace() || c == ':' || c == '"' || c == '\'')
        .find(|w| {
            let parts: Vec<&str> = w.split('-').collect();
            parts.len() == 2 && parts.iter().all(|p| (4..=5).contains(&p.len()) && p.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
        })
        .map(str::to_string)
}

/// A pasted API key, or the code a sign-in page showed.
pub async fn code(app: &Arc<App>, account: &str, code: &str) -> Result<()> {
    let (port, auth, step) = {
        let logins = app.logins.opencode.lock().unwrap();
        let l = logins.get(account).context("no OpenCode login is running; start again")?;
        let step = match &l.step {
            Step::Key { provider } => (provider.clone(), None),
            Step::Code { provider, method } => (provider.clone(), Some(*method)),
            _ => bail!("pick a provider first"),
        };
        (l.port, l.auth.clone(), step)
    };
    report(app, "opencode", account, "checking", json!({}));
    let (provider, method) = step;
    let res = match method {
        None => http(port, &auth, "PUT", &format!("/auth/{provider}"), Some(&json!({ "type": "api", "key": code }))).await,
        Some(m) => http(port, &auth, "POST", &format!("/provider/{provider}/oauth/callback"), Some(&json!({ "method": m, "code": code }))).await,
    };
    finish(app, account, res.map(|_| ()));
    Ok(())
}

fn finish(app: &Arc<App>, account: &str, res: Result<()>) {
    // Dropping the login stops its server.
    if app.logins.opencode.lock().unwrap().remove(account).is_none() {
        return; // cancelled meanwhile
    }
    match res {
        Ok(()) => {
            report(app, "opencode", account, "done", json!({}));
            // The agent may have read its providers at start.
            crate::acp::restart_when_idle(app, "opencode", account);
            let (app, account) = (app.clone(), account.to_string());
            tokio::spawn(async move {
                if account == DEFAULT {
                    let _ = load_models(&app).await;
                }
                app.broadcast(crate::login::status(&app).await);
            });
        }
        Err(e) => report(app, "opencode", account, "failed", json!({ "error": format!("{e:#}") })),
    }
}

pub fn cancel(app: &App, account: &str) {
    app.logins.opencode.lock().unwrap().remove(account);
}

/// Logins in progress, by account.
pub type Logins = std::sync::Mutex<HashMap<String, Login>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbose_models_become_picker_models() {
        let text = "opencode/big-pickle\n{\n  \"id\": \"big-pickle\",\n  \"name\": \"Big Pickle\",\n  \"variants\": {}\n}\nanthropic/claude-x\n{\n  \"name\": \"Claude X\",\n  \"variants\": {\n    \"high\": { \"thinking\": 1 },\n    \"max\": {}\n  }\n}\n";
        let models = parse_models(text);
        assert_eq!(
            models,
            vec![
                json!({ "id": "opencode/big-pickle", "name": "Big Pickle (opencode)", "efforts": [] }),
                json!({ "id": "anthropic/claude-x", "name": "Claude X (anthropic)", "efforts": ["default", "high", "max"], "defaultEffort": "default" }),
            ]
        );
    }

    #[test]
    fn v2_approval_rules_append_to_inline_jsonc() {
        let content = r#"{
            // Keep the user's inline settings and existing rule order.
            "model": "opencode-go/fake-go",
            "permissions": [{"action":"read","resource":"*","effect":"allow"}],
        }"#;
        let config = v2_permission_config(content).unwrap();
        assert!(config.contains("Keep the user's inline settings"));
        let rendered = config.find(r#""edit""#).unwrap();
        assert!(config.find(r#""read""#).unwrap() < rendered);
        let empty: Value = serde_json::from_str(&v2_permission_config("").unwrap()).unwrap();
        assert_eq!(empty["permissions"], json!([
            {"action":"edit","resource":"*","effect":"ask"},
            {"action":"shell","resource":"*","effect":"ask"},
            {"action":"webfetch","resource":"*","effect":"ask"}
        ]));
    }

    #[test]
    fn a_sign_in_works_from_the_app_when_its_prompts_have_defaults() {
        let copilot = json!({ "type": "oauth", "prompts": [
            { "type": "select", "key": "deploymentType", "options": [{ "value": "github.com" }, { "value": "enterprise" }] },
            { "type": "text", "key": "enterpriseUrl", "when": { "key": "deploymentType", "op": "eq", "value": "enterprise" } },
        ]});
        assert_eq!(default_inputs(&copilot).unwrap()["deploymentType"], "github.com");
        let gitlab = json!({ "type": "oauth", "prompts": [{ "type": "text", "key": "instanceUrl" }] });
        assert!(default_inputs(&gitlab).is_none());
    }

    #[test]
    fn providers_are_offered_connected_and_popular_first() {
        let providers = json!({
            "all": [{ "id": "zeta", "name": "Zeta" }, { "id": "anthropic", "name": "Anthropic" }, { "id": "gitlab", "name": "GitLab" }, { "id": "mine", "name": "Mine" }],
            "connected": ["mine"],
        });
        let methods = json!({ "gitlab": [{ "type": "oauth", "label": "GitLab OAuth", "prompts": [{ "type": "text", "key": "instanceUrl" }] }] });
        let list = ways_in(&providers, &methods);
        let ids: Vec<&str> = list.iter().map(|p| p["id"].as_str().unwrap()).collect();
        // GitLab's only way in needs a typed URL, so it is left to the terminal.
        assert_eq!(ids, ["mine", "anthropic", "zeta"]);
        assert_eq!(list[1]["methods"], json!([{ "index": 0, "type": "api", "label": "API key" }]));
    }

    #[test]
    fn device_codes_are_found_in_instructions() {
        assert_eq!(device_code("Enter code: ABCD-1234").as_deref(), Some("ABCD-1234"));
        assert_eq!(device_code("Open the page and sign in"), None);
    }

    #[test]
    fn chunked_bodies_are_joined() {
        assert_eq!(dechunk(b"4\r\ntrue\r\n0\r\n\r\n"), b"true");
    }
}
