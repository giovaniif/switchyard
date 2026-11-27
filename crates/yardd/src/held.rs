//! Connectors Switchyard holds itself, for the engines that speak ACP
//! (OpenCode, Grok, Gemini). Claude and Codex run their own OAuth (`mcp.rs`);
//! these engines have no sign-in yardd can drive, so yardd does it: it signs
//! in to a remote MCP server (the MCP authorization spec: OAuth 2.1 with
//! PKCE, discovery and dynamic client registration), keeps the tokens, and
//! gives each ACP session a stdio bridge to every connected server instead of
//! the token.
//!
//! One set per host, shared by every ACP engine and account, addressed with
//! the `mcp.*` ops under the engine name `switchyard` (see `mcp.rs` for the
//! shapes). Kept in `<data dir>/held-connectors.json`, mode 0600.
//!
//! The bridge is `yardd mcp-proxy`: the session's agent starts it like any
//! stdio MCP server, and it passes each JSON-RPC message to the running yardd
//! over loopback (`POST /connector-proxy`, with a key that names one session
//! and one connector). yardd sends it on as MCP Streamable HTTP with the
//! token, refreshing it when due, so the token never leaves this process.
//!
//! Tokens and client secrets are never broadcast, logged or put in an error.

use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use base64::Engine as _;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::{
    mcp::{auth_state, clip},
    state::{now_ms, App},
};

/// The engine name the `mcp.*` ops use for these connectors.
pub const ENGINE: &str = "switchyard";
const ACCOUNT: &str = crate::accounts::DEFAULT;
const FILE: &str = "held-connectors.json";
const MAX_CONNECTORS: usize = 100;

/// A sign-in's `state` is good for this long.
const SIGN_IN: Duration = Duration::from_secs(15 * 60);
/// Refresh an access token that expires within this.
const REFRESH_AHEAD_MS: u64 = 60_000;
/// Metadata, registration and token calls.
const OAUTH_WAIT: Duration = Duration::from_secs(20);
/// One request of a status check, and the whole check of one connector.
const CHECK_STEP: Duration = Duration::from_secs(15);
const CHECK: Duration = Duration::from_secs(45);
/// A tool call can be slow.
const CALL_WAIT: Duration = Duration::from_secs(10 * 60);
const NOTE_WAIT: Duration = Duration::from_secs(30);
/// An answer larger than this is cut off rather than kept in memory.
const MAX_ANSWER: usize = 32 * 1024 * 1024;
const PROTOCOL: &str = "2025-06-18";

#[derive(Clone, Serialize, Deserialize)]
struct Connector {
    name: String,
    url: String,
    /// The authorization server found for it at the last sign-in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auth: Option<AuthServer>,
    /// Switchyard's registration with that server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client: Option<Registration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tokens: Option<Tokens>,
    /// The server answered without a sign-in when last asked.
    #[serde(default)]
    open: bool,
    /// How many tools it listed when last connected.
    #[serde(default)]
    tools: u64,
}

impl Connector {
    /// A session can be given it: signed in, or it needs no sign-in.
    fn usable(&self) -> bool {
        self.tokens.is_some() || self.open
    }
}

#[derive(Clone, Serialize, Deserialize)]
struct AuthServer {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    registration_endpoint: Option<String>,
    #[serde(default)]
    scope: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Registration {
    client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    client_secret: Option<String>,
    /// How the server said this client authenticates at the token endpoint.
    #[serde(default)]
    auth_method: String,
    /// What it was registered for: another server or redirect needs a new one.
    issuer: String,
    redirect_uri: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Tokens {
    access: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh: Option<String>,
    /// In ms; none when the server did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
}

/// A sign-in waiting for its redirect.
struct Pending {
    name: String,
    verifier: String,
    redirect: String,
    at: u64,
}

/// One session's bridge to one connector, and where its MCP session with the
/// server stands.
struct Bridge {
    sid: String,
    connector: String,
    upstream: Mutex<Upstream>,
}

#[derive(Clone, Default)]
struct Upstream {
    /// The `Mcp-Session-Id` the server handed out.
    session: Option<String>,
    /// The protocol version `initialize` settled on.
    version: Option<String>,
    /// The agent's `initialize`, replayed if the server forgets the session.
    init: Option<Value>,
}

/// The connector has no usable sign-in.
#[derive(Debug)]
struct NeedsAuth;

impl std::fmt::Display for NeedsAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("it needs a sign-in")
    }
}

impl std::error::Error for NeedsAuth {}

/// The authorization server said no to a token request (as opposed to not
/// answering): the grant is gone.
#[derive(Debug)]
struct Refused(String);

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Refused {}

pub struct Held {
    file: PathBuf,
    list: Mutex<Vec<Connector>>,
    /// Sign-ins in progress, by their `state`.
    pending: Mutex<HashMap<String, Pending>>,
    /// Bridge key -> the session and connector it was made for.
    keys: Mutex<HashMap<String, Arc<Bridge>>>,
    /// One refresh at a time per connector: two sessions must never spend
    /// the same refresh token.
    refreshing: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// yardd on loopback (`http://127.0.0.1:<port>`), set by main. Unset, no
    /// session is given a bridge.
    base: Mutex<Option<String>>,
    http: OnceLock<reqwest::Client>,
}

impl Held {
    pub fn load(dir: PathBuf) -> Held {
        let file = dir.join(FILE);
        let list = std::fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .and_then(|v| serde_json::from_value::<Vec<Connector>>(v["connectors"].clone()).ok())
            .unwrap_or_default();
        Held {
            file,
            list: Mutex::new(list),
            pending: Mutex::default(),
            keys: Mutex::default(),
            refreshing: Mutex::default(),
            base: Mutex::default(),
            http: OnceLock::new(),
        }
    }

    pub fn set_base(&self, base: Option<String>) {
        *self.base.lock().unwrap() = base;
    }

    /// A deleted session's bridges stop working.
    pub fn forget(&self, sid: &str) {
        self.keys.lock().unwrap().retain(|_, b| b.sid != sid);
    }

    fn key_for(&self, sid: &str, connector: &str) -> String {
        let mut keys = self.keys.lock().unwrap();
        if let Some((key, _)) = keys.iter().find(|(_, b)| b.sid == sid && b.connector == connector) {
            return key.clone();
        }
        let key = crate::agent_tools::random_key();
        keys.insert(key.clone(), Arc::new(Bridge { sid: sid.to_string(), connector: connector.to_string(), upstream: Mutex::default() }));
        key
    }

    fn all(&self) -> Vec<Connector> {
        self.list.lock().unwrap().clone()
    }

    fn get(&self, name: &str) -> Option<Connector> {
        self.list.lock().unwrap().iter().find(|c| c.name == name).cloned()
    }

    /// Change the list and save it.
    fn edit<R>(&self, f: impl FnOnce(&mut Vec<Connector>) -> R) -> R {
        let mut list = self.list.lock().unwrap();
        let out = f(&mut list);
        if let Err(e) = self.save(&list) {
            eprintln!("yardd: cannot save {}: {e}", self.file.display());
        }
        out
    }

    /// Change one connector, if it is still there.
    fn update(&self, name: &str, f: impl FnOnce(&mut Connector)) -> bool {
        self.edit(|list| list.iter_mut().find(|c| c.name == name).map(f).is_some())
    }

    /// Written aside and renamed, so a crash leaves the old file; the new
    /// one is private before anything is in it.
    fn save(&self, list: &[Connector]) -> std::io::Result<()> {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let tmp = self.file.with_extension("json.tmp");
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
        f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        f.write_all(json!({ "connectors": list }).to_string().as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, &self.file)
    }

    /// No redirect is followed anywhere: a token or a registration must not
    /// travel to wherever a server points, and neither should the rest.
    fn http(&self) -> &reqwest::Client {
        self.http.get_or_init(|| {
            crate::slack::crypto();
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .user_agent(concat!("Switchyard/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("an HTTP client")
        })
    }

    fn take_pending(&self, state: &str) -> Result<Pending> {
        let mut pending = self.pending.lock().unwrap();
        let now = now_ms();
        pending.retain(|_, p| now.saturating_sub(p.at) < SIGN_IN.as_millis() as u64);
        pending.remove(state).context("That sign-in is not in progress: it was already finished, or it expired. Start it again.")
    }
}

/// A network error without the address it was for.
fn net(e: reqwest::Error) -> anyhow::Error {
    let what = if e.is_timeout() {
        "it did not answer in time"
    } else if e.is_connect() {
        "could not connect to it"
    } else {
        "the request failed"
    };
    anyhow!("{what}")
}

/// An address yardd may call or send a browser to: `https`, or plain `http`
/// on this machine (local servers and the tests).
fn endpoint(url: &str, what: &str) -> Result<Url> {
    let refuse = || anyhow!("{what} is not an https address");
    let u = Url::parse(url).map_err(|_| refuse())?;
    let host = u.host_str().unwrap_or("");
    let local = host == "localhost" || host.trim_matches(['[', ']']).parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback());
    let ok = match u.scheme() {
        "https" => !host.is_empty(),
        "http" => local,
        _ => false,
    };
    if !ok || !u.username().is_empty() || u.password().is_some() {
        return Err(refuse());
    }
    Ok(u)
}

/// What `mcp.add` takes: the name rule every connector has, and a plain
/// `https` address or `http` on this machine with a port.
fn check_new(name: &str, url: &str) -> Result<()> {
    if url.starts_with("http://") {
        crate::mcp::check_new_server(name, "https://name.check")?;
        if crate::mcp::loopback_target(url).is_err() || url.len() > 300 {
            bail!("The address must start with https:// (http:// only for a server on this machine, with its port)");
        }
    } else {
        crate::mcp::check_new_server(name, url)?;
    }
    // The agents' own tools already use this name in every session.
    if name.eq_ignore_ascii_case(crate::agent_tools::SERVER) {
        bail!("That name is taken by Switchyard's own tools");
    }
    Ok(())
}

fn random(bytes: usize) -> Result<String> {
    use ring::rand::SecureRandom;
    let mut buf = vec![0u8; bytes];
    ring::rand::SystemRandom::new().fill(&mut buf).map_err(|_| anyhow!("the system gave no random bytes"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf))
}

/// PKCE's S256 challenge for a verifier.
fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()))
}

/// A parameter of a `WWW-Authenticate` challenge, quoted or not.
fn challenge_param(header: &str, name: &str) -> Option<String> {
    let mut rest = header;
    while let Some(i) = rest.find(name) {
        let before_ok = rest[..i].chars().next_back().is_none_or(|c| c == ' ' || c == ',');
        let after = rest[i + name.len()..].trim_start();
        if before_ok && after.starts_with('=') {
            let value = after[1..].trim_start();
            return Some(match value.strip_prefix('"') {
                Some(quoted) => quoted.split('"').next().unwrap_or("").to_string(),
                None => value.split([',', ' ']).next().unwrap_or("").to_string(),
            });
        }
        rest = &rest[i + name.len()..];
    }
    None
}

/// Where a well-known document may be for `of`: with its path inserted after
/// the well-known part (RFC 8414, RFC 9728), then appended to it where
/// OpenID Connect does that, then at the root.
fn well_known(of: &Url, name: &str, appended: bool) -> Vec<Url> {
    let path = of.path().trim_end_matches('/').to_string();
    let mut root = of.clone();
    root.set_query(None);
    root.set_fragment(None);
    let at = |p: String| {
        let mut u = root.clone();
        u.set_path(&p);
        u
    };
    let mut out = Vec::new();
    if !path.is_empty() {
        out.push(at(format!("/.well-known/{name}{path}")));
        if appended {
            out.push(at(format!("{path}/.well-known/{name}")));
        }
    }
    out.push(at(format!("/.well-known/{name}")));
    out
}

/// Scope names as one space-separated parameter; anything that is not a
/// plain token is left out.
fn scopes(v: &Value) -> String {
    let ok = |s: &&str| !s.is_empty() && s.len() <= 200 && s.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\');
    match v {
        Value::Array(l) => l.iter().filter_map(Value::as_str).filter(ok).take(50).collect::<Vec<_>>().join(" "),
        Value::String(s) => s.split_whitespace().filter(ok).take(50).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

/// The JSON-RPC messages of the complete events in `buf` (a `text/event-
/// stream` body so far); what is left in `buf` is an event still arriving.
fn sse_messages(buf: &mut Vec<u8>) -> Vec<Value> {
    let mut out = Vec::new();
    loop {
        let end = [&b"\r\n\r\n"[..], b"\n\n", b"\r\r"]
            .iter()
            .filter_map(|sep| buf.windows(sep.len()).position(|w| w == *sep).map(|i| (i, sep.len())))
            .min();
        let Some((at, sep)) = end else { break };
        let block: Vec<u8> = buf.drain(..at + sep).collect();
        let text = String::from_utf8_lossy(&block[..at]);
        let data: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|d| d.strip_prefix(' ').unwrap_or(d))
            .collect();
        if let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) {
            match v {
                Value::Array(l) => out.extend(l),
                v => out.push(v),
            }
        }
    }
    out
}

/// What an MCP server answered to one POST.
struct Reply {
    status: StatusCode,
    messages: Vec<Value>,
    session: Option<String>,
}

impl Reply {
    /// The answer to the request with this id.
    fn answer(&self, id: &Value) -> Option<&Value> {
        self.messages.iter().find(|m| &m["id"] == id && m.get("method").is_none())
    }
}

fn is_request(msg: &Value) -> bool {
    msg["method"].is_string() && msg.get("id").is_some_and(|id| !id.is_null())
}

impl Held {
    /// One JSON-RPC message to an MCP server over Streamable HTTP. The
    /// answer is a JSON body or an event stream; a stream is read until the
    /// request's own answer arrives.
    async fn post(&self, url: &str, token: Option<&str>, up: &Upstream, body: &Value, wait: Duration) -> Result<Reply> {
        let mut req = self
            .http()
            .post(endpoint(url, "the connector's address")?)
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .body(body.to_string())
            .timeout(wait);
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        if let Some(session) = &up.session {
            req = req.header("mcp-session-id", session);
        }
        if let Some(version) = &up.version {
            req = req.header("mcp-protocol-version", version);
        }
        let mut res = req.send().await.map_err(net)?;
        let status = res.status();
        let header = |name: &str| res.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
        let session = header("mcp-session-id").filter(|s| !s.is_empty() && s.len() <= 500);
        let stream = header("content-type").is_some_and(|c| c.to_ascii_lowercase().starts_with("text/event-stream"));
        let mut messages = Vec::new();
        if status.is_success() && status != StatusCode::ACCEPTED {
            let want = is_request(body).then(|| body["id"].clone());
            let mut buf = Vec::new();
            let mut read = 0usize;
            while let Some(chunk) = res.chunk().await.map_err(net)? {
                read += chunk.len();
                if read > MAX_ANSWER {
                    bail!("its answer is too large");
                }
                buf.extend_from_slice(&chunk);
                if stream {
                    messages.extend(sse_messages(&mut buf));
                    // The server may hold the stream open after the answer.
                    if want.as_ref().is_some_and(|id| messages.iter().any(|m| &m["id"] == id && m.get("method").is_none())) {
                        break;
                    }
                }
            }
            if !stream && !buf.iter().all(u8::is_ascii_whitespace) {
                match serde_json::from_slice::<Value>(&buf).map_err(|_| anyhow!("its answer is not JSON"))? {
                    Value::Array(l) => messages.extend(l),
                    v => messages.push(v),
                }
            }
        }
        Ok(Reply { status, messages, session })
    }

    async fn get_json(&self, url: &Url) -> Option<Value> {
        let res = self.http().get(url.clone()).header("accept", "application/json").timeout(OAUTH_WAIT).send().await.ok()?;
        if !res.status().is_success() {
            return None;
        }
        serde_json::from_slice::<Value>(&res.bytes().await.ok()?).ok().filter(Value::is_object)
    }

    /// Find the connector's authorization server. `None`: the server answers
    /// without a sign-in.
    async fn discover(&self, c: &Connector) -> Result<Option<AuthServer>> {
        let url = endpoint(&c.url, "the connector's address")?;
        let probe = self
            .http()
            .post(url.clone())
            .header("accept", "application/json, text/event-stream")
            .header("content-type", "application/json")
            .body(initialize(json!(1)).to_string())
            .timeout(OAUTH_WAIT)
            .send()
            .await
            .map_err(net)?;
        if probe.status().is_success() {
            return Ok(None);
        }
        let asked = probe.headers().get("www-authenticate").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        drop(probe);

        // The protected resource's own metadata: where the server said, then
        // the well-known places.
        let mut places: Vec<Url> = challenge_param(&asked, "resource_metadata").and_then(|u| endpoint(&u, "the metadata address").ok()).into_iter().collect();
        places.extend(well_known(&url, "oauth-protected-resource", false));
        let mut resource = None;
        for place in &places {
            if let Some(v) = self.get_json(place).await {
                resource = Some(v);
                break;
            }
        }
        let issuer = match resource.as_ref().and_then(|r| r["authorization_servers"][0].as_str()) {
            Some(issuer) => endpoint(issuer, "the authorization server's address")?,
            // An older server is its own authorization server.
            None => endpoint(&url.origin().ascii_serialization(), "the connector's address")?,
        };
        let mut found = None;
        for place in well_known(&issuer, "oauth-authorization-server", false).into_iter().chain(well_known(&issuer, "openid-configuration", true)) {
            if let Some(v) = self.get_json(&place).await {
                found = Some(v);
                break;
            }
        }
        let meta = found.with_context(|| format!("{} did not say how to sign in (no OAuth metadata)", c.name))?;
        let field = |key: &str, what: &str| -> Result<String> {
            let value = meta[key].as_str().with_context(|| format!("{}'s sign-in metadata has no {what}", c.name))?;
            Ok(endpoint(value, what)?.to_string())
        };
        if meta["code_challenge_methods_supported"].as_array().is_some_and(|l| !l.iter().any(|m| m == "S256")) {
            bail!("{}'s sign-in does not support PKCE (S256)", c.name);
        }
        // The server's challenge names the scopes it wants now; its metadata, all it has.
        let scope = match challenge_param(&asked, "scope").map(|s| scopes(&json!(s))).filter(|s| !s.is_empty()) {
            Some(s) => s,
            None => resource.as_ref().map(|r| scopes(&r["scopes_supported"])).unwrap_or_default(),
        };
        Ok(Some(AuthServer {
            issuer: issuer.to_string(),
            authorization_endpoint: field("authorization_endpoint", "sign-in page")?,
            token_endpoint: field("token_endpoint", "token address")?,
            registration_endpoint: match meta["registration_endpoint"].as_str() {
                Some(_) => Some(field("registration_endpoint", "registration address")?),
                None => None,
            },
            scope,
        }))
    }

    /// Switchyard's client at the authorization server: the saved one while
    /// it still fits, or a new dynamic registration (RFC 7591).
    async fn registration(&self, c: &Connector, auth: &AuthServer, redirect: &str) -> Result<Registration> {
        if let Some(r) = c.client.as_ref().filter(|r| r.redirect_uri == redirect && r.issuer == auth.issuer) {
            return Ok(r.clone());
        }
        let Some(register) = &auth.registration_endpoint else {
            bail!("{} needs a client registered with it in advance, which Switchyard does not support yet", c.name);
        };
        let body = json!({
            "client_name": "Switchyard",
            "redirect_uris": [redirect],
            "grant_types": ["authorization_code", "refresh_token"],
            "response_types": ["code"],
            "token_endpoint_auth_method": "none",
        });
        let res = self
            .http()
            .post(endpoint(register, "registration address")?)
            .header("accept", "application/json")
            .header("content-type", "application/json")
            .body(body.to_string())
            .timeout(OAUTH_WAIT)
            .send()
            .await
            .map_err(net)
            .with_context(|| format!("registering with {}", c.name))?;
        let status = res.status();
        let v: Value = serde_json::from_slice(&res.bytes().await.map_err(net)?).unwrap_or(Value::Null);
        if !status.is_success() {
            bail!("{} refused to register Switchyard: {}", c.name, oauth_error(&v, status));
        }
        let client_id = v["client_id"].as_str().filter(|s| !s.is_empty()).with_context(|| format!("{} registered Switchyard without a client id", c.name))?;
        Ok(Registration {
            client_id: client_id.to_string(),
            client_secret: v["client_secret"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
            auth_method: v["token_endpoint_auth_method"].as_str().unwrap_or("none").to_string(),
            issuer: auth.issuer.clone(),
            redirect_uri: redirect.to_string(),
        })
    }

    /// One call to the token endpoint. A refusal (`Refused`) is told apart
    /// from a server that did not answer.
    async fn token_call(&self, auth: &AuthServer, client: &Registration, mut form: Vec<(&'static str, String)>) -> Result<Tokens> {
        form.push(("client_id", client.client_id.clone()));
        let mut req = self.http().post(endpoint(&auth.token_endpoint, "token address")?).header("accept", "application/json").timeout(OAUTH_WAIT);
        match (client.client_secret.as_deref(), client.auth_method.as_str()) {
            (Some(secret), "client_secret_basic") => req = req.basic_auth(&client.client_id, Some(secret)),
            (Some(secret), _) => form.push(("client_secret", secret.to_string())),
            _ => {}
        }
        let res = req.form(&form).send().await.map_err(net)?;
        let status = res.status();
        let v: Value = serde_json::from_slice(&res.bytes().await.map_err(net)?).unwrap_or(Value::Null);
        if status.is_client_error() {
            return Err(Refused(oauth_error(&v, status)).into());
        }
        if !status.is_success() {
            bail!("the sign-in server answered {}", status.as_u16());
        }
        let access = v["access_token"].as_str().filter(|s| !s.is_empty()).context("the sign-in server sent no access token")?;
        let expires_in = v["expires_in"].as_u64().or_else(|| v["expires_in"].as_str().and_then(|s| s.parse().ok()));
        Ok(Tokens {
            access: access.to_string(),
            refresh: v["refresh_token"].as_str().filter(|s| !s.is_empty()).map(str::to_string),
            expires_at: expires_in.map(|s| now_ms() + s.saturating_mul(1000)),
        })
    }
}

/// An OAuth error body as a short line: the code and what the server said
/// about it, which never hold a credential of ours.
fn oauth_error(v: &Value, status: StatusCode) -> String {
    let plain = |s: &str| clip(&s.chars().filter(|c| !c.is_control()).collect::<String>(), 200);
    match (v["error"].as_str(), v["error_description"].as_str()) {
        (Some(code), Some(text)) => format!("{} ({})", plain(text), plain(code)),
        (Some(code), None) => plain(code),
        _ => format!("it answered {}", status.as_u16()),
    }
}

fn initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0", "id": id, "method": "initialize",
        "params": {
            "protocolVersion": PROTOCOL,
            "capabilities": {},
            "clientInfo": { "name": "switchyard", "title": "Switchyard", "version": env!("CARGO_PKG_VERSION") },
        },
    })
}

fn redirect_uri(app: &App) -> String {
    format!("http://127.0.0.1:{}/connector/callback", app.port)
}

/// Forget a connector's sign-in and say so to every window: it reads
/// `needs-auth` and sessions stop getting it.
fn drop_tokens(app: &Arc<App>, name: &str) {
    let mut dropped = false;
    app.held.update(name, |c| {
        dropped = c.tokens.take().is_some();
    });
    if dropped {
        let app = app.clone();
        tokio::spawn(async move { broadcast_status(&app).await });
    }
}

/// A new access token for the connector, unless another caller got one
/// while this one waited (`stale` is the token the caller had).
async fn refresh(app: &Arc<App>, name: &str, stale: &str) -> Result<String> {
    let h = &app.held;
    let lock = h.refreshing.lock().unwrap().entry(name.to_string()).or_default().clone();
    let _one = lock.lock().await;
    let c = h.get(name).ok_or(NeedsAuth)?;
    let tokens = c.tokens.ok_or(NeedsAuth)?;
    if tokens.access != stale {
        return Ok(tokens.access);
    }
    let (Some(refresh), Some(auth), Some(client)) = (tokens.refresh, c.auth, c.client) else {
        drop_tokens(app, name);
        return Err(NeedsAuth.into());
    };
    let form = vec![("grant_type", "refresh_token".to_string()), ("refresh_token", refresh.clone()), ("resource", c.url.clone())];
    match h.token_call(&auth, &client, form).await {
        Ok(mut new) => {
            // A server that does not rotate refresh tokens sends none back.
            new.refresh.get_or_insert(refresh);
            let access = new.access.clone();
            h.update(name, |c| c.tokens = Some(new));
            Ok(access)
        }
        Err(e) if e.is::<Refused>() => {
            drop_tokens(app, name);
            Err(NeedsAuth.into())
        }
        Err(e) => Err(e.context("refreshing the sign-in")),
    }
}

/// One message to a connector with its sign-in: the token is refreshed
/// first when it is about to expire, and once more if the server refuses it.
async fn call(app: &Arc<App>, name: &str, up: &Upstream, body: &Value, wait: Duration) -> Result<Reply> {
    let h = &app.held;
    let c = h.get(name).ok_or(NeedsAuth)?;
    let token = match &c.tokens {
        Some(t) if t.refresh.is_some() && t.expires_at.is_some_and(|at| at <= now_ms() + REFRESH_AHEAD_MS) => Some(refresh(app, name, &t.access).await?),
        Some(t) => Some(t.access.clone()),
        None => None,
    };
    let reply = h.post(&c.url, token.as_deref(), up, body, wait).await?;
    if reply.status != StatusCode::UNAUTHORIZED {
        return Ok(reply);
    }
    let Some(stale) = token else {
        if c.open {
            h.update(name, |c| c.open = false);
        }
        return Err(NeedsAuth.into());
    };
    let fresh = refresh(app, name, &stale).await?;
    let reply = h.post(&c.url, Some(&fresh), up, body, wait).await?;
    if reply.status == StatusCode::UNAUTHORIZED {
        drop_tokens(app, name);
        return Err(NeedsAuth.into());
    }
    Ok(reply)
}

/// End an MCP session the server handed out, if it takes that.
async fn end_session(app: &Arc<App>, name: &str, up: &Upstream) {
    let (Some(session), Some(c)) = (&up.session, app.held.get(name)) else { return };
    let Ok(url) = endpoint(&c.url, "the connector's address") else { return };
    let mut req = app.held.http().delete(url).header("mcp-session-id", session).timeout(Duration::from_secs(5));
    if let Some(t) = &c.tokens {
        req = req.bearer_auth(&t.access);
    }
    let _ = req.send().await;
}

/// Connect to a connector as a session would and count its tools.
async fn count_tools(app: &Arc<App>, name: &str) -> Result<u64> {
    let mut up = Upstream::default();
    let answer_of = |reply: &Reply, id: u64| -> Result<Value> {
        if !reply.status.is_success() {
            bail!("the server answered {}", reply.status.as_u16());
        }
        let m = reply.answer(&json!(id)).context("the server did not answer")?;
        if let Some(e) = m.get("error") {
            bail!("{}", clip(e["message"].as_str().unwrap_or("the server refused"), 300));
        }
        Ok(m["result"].clone())
    };
    let reply = call(app, name, &up, &initialize(json!(1)), CHECK_STEP).await?;
    let result = answer_of(&reply, 1)?;
    up.session = reply.session;
    up.version = result["protocolVersion"].as_str().map(str::to_string);
    let _ = call(app, name, &up, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), CHECK_STEP).await;
    let mut tools = 0u64;
    let mut cursor = Value::Null;
    let mut listed = Ok(());
    // Pages, with a bound in case a server keeps handing out cursors.
    for page in 0..20u64 {
        let mut params = json!({});
        if !cursor.is_null() {
            params["cursor"] = cursor;
        }
        let body = json!({ "jsonrpc": "2.0", "id": 2 + page, "method": "tools/list", "params": params });
        let result = match call(app, name, &up, &body, CHECK_STEP).await.and_then(|r| answer_of(&r, 2 + page)) {
            Ok(r) => r,
            Err(e) => {
                listed = Err(e);
                break;
            }
        };
        tools += result["tools"].as_array().map_or(0, |l| l.len() as u64);
        cursor = result["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    end_session(app, name, &up).await;
    listed?;
    app.held.update(name, |c| {
        c.tools = tools;
        // It answered with no token: sessions can use it as it is.
        c.open = c.tokens.is_none();
    });
    Ok(tools)
}

/// One connector as the Connectors page lists it.
async fn row(app: &Arc<App>, c: Connector) -> Value {
    let mut row = json!({ "name": c.name, "status": "needs-auth", "source": ENGINE, "tools": c.tools, "appTools": 0 });
    if let Ok(u) = Url::parse(&c.url) {
        row["origin"] = json!(u.origin().ascii_serialization());
    }
    match tokio::time::timeout(CHECK, count_tools(app, &c.name)).await {
        Ok(Ok(tools)) => {
            row["status"] = json!("connected");
            row["tools"] = json!(tools);
        }
        Ok(Err(e)) if e.is::<NeedsAuth>() => {}
        Ok(Err(e)) => {
            row["status"] = json!("failed");
            row["error"] = json!(clip(&format!("{e:#}"), 500));
        }
        Err(_) => {
            row["status"] = json!("failed");
            row["error"] = json!("It did not answer in time.");
        }
    }
    row
}

async fn broadcast_status(app: &Arc<App>) {
    let rows = futures_util::future::join_all(app.held.all().into_iter().map(|c| row(app, c))).await;
    app.broadcast(json!({ "op": "mcp", "engine": ENGINE, "account": ACCOUNT, "at": now_ms(), "servers": rows }));
}

fn auth_note(app: &App, server: &str, state: Value) {
    auth_state(app, ENGINE, ACCOUNT, server, state);
}

fn auth_failed(app: &App, server: &str, e: &anyhow::Error) {
    auth_note(app, server, json!({ "state": "failed", "error": clip(&format!("{e:#}"), 500) }));
}

/// After a sign-in: tell every window and list again. Sessions pick the
/// connector up with their next turn (`acp::attached`).
fn signed_in(app: &Arc<App>, name: &str) {
    auth_note(app, name, json!({ "state": "done" }));
    let app = app.clone();
    tokio::spawn(async move { broadcast_status(&app).await });
}

/// The `mcp.*` ops for `engine: "switchyard"`.
pub async fn handle(app: &Arc<App>, msg: &Value) -> Result<()> {
    let field = |key: &str| msg[key].as_str().map(str::trim).filter(|s| !s.is_empty()).with_context(|| format!("missing `{key}`")).map(str::to_string);
    let app = app.clone();
    match msg["op"].as_str().unwrap_or("") {
        "mcp.status" | "mcp.reconnect" => {
            tokio::spawn(async move { broadcast_status(&app).await });
        }
        "mcp.add" => {
            let (name, url) = (field("name")?, field("url")?);
            check_new(&name, &url)?;
            tokio::spawn(async move {
                let added = app.held.edit(|list| -> Result<()> {
                    if list.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
                        bail!("Switchyard already holds a connector named {name}");
                    }
                    if list.len() >= MAX_CONNECTORS {
                        bail!("At most {MAX_CONNECTORS} connectors");
                    }
                    list.push(Connector { name: name.clone(), url, auth: None, client: None, tokens: None, open: false, tools: 0 });
                    Ok(())
                });
                changed(&app, &name, added).await;
            });
        }
        "mcp.remove" => {
            let name = field("server")?;
            tokio::spawn(async move {
                let removed = app.held.edit(|list| -> Result<()> {
                    let before = list.len();
                    list.retain(|c| c.name != name);
                    if list.len() == before {
                        bail!("Switchyard holds no connector named {name}");
                    }
                    Ok(())
                });
                app.held.keys.lock().unwrap().retain(|_, b| b.connector != name);
                app.held.pending.lock().unwrap().retain(|_, p| p.name != name);
                app.held.refreshing.lock().unwrap().remove(&name);
                changed(&app, &name, removed).await;
            });
        }
        "mcp.auth" => {
            let server = field("server")?;
            tokio::spawn(async move {
                if let Err(e) = auth(&app, &server).await {
                    auth_failed(&app, &server, &e);
                }
            });
        }
        "mcp.callback" => {
            let (server, url) = (field("server")?, field("url")?);
            tokio::spawn(async move {
                match pasted(&app, &server, &url).await {
                    Ok(()) => signed_in(&app, &server),
                    Err(e) => auth_failed(&app, &server, &e),
                }
            });
        }
        other => bail!("unknown op `{other}`"),
    }
    Ok(())
}

async fn changed(app: &Arc<App>, name: &str, result: Result<()>) {
    let mut note = json!({ "op": "mcp.change", "engine": ENGINE, "account": ACCOUNT, "name": name, "state": "done" });
    if let Err(e) = result {
        note["state"] = json!("failed");
        note["error"] = json!(clip(&format!("{e:#}"), 500));
    }
    app.broadcast(note);
    broadcast_status(app).await;
}

/// Start a sign-in: find the authorization server, register with it, and
/// hand the windows the page to open.
async fn auth(app: &Arc<App>, name: &str) -> Result<()> {
    let h = &app.held;
    let c = h.get(name).with_context(|| format!("Switchyard holds no connector named {name}"))?;
    let Some(auth) = h.discover(&c).await? else {
        h.update(name, |c| c.open = true);
        signed_in(app, name);
        return Ok(());
    };
    let redirect = redirect_uri(app);
    let client = h.registration(&c, &auth, &redirect).await?;
    h.update(name, |c| {
        c.auth = Some(auth.clone());
        c.client = Some(client.clone());
    });
    let (verifier, state) = (random(32)?, random(24)?);
    let mut url = endpoint(&auth.authorization_endpoint, "sign-in page")?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &client.client_id)
            .append_pair("redirect_uri", &redirect)
            .append_pair("code_challenge", &challenge(&verifier))
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state)
            .append_pair("resource", &c.url);
        if !auth.scope.is_empty() {
            q.append_pair("scope", &auth.scope);
        }
    }
    {
        let mut pending = h.pending.lock().unwrap();
        // A second Connect replaces the first: only the newest link works.
        pending.retain(|_, p| p.name != name);
        pending.insert(state, Pending { name: name.to_string(), verifier, redirect, at: now_ms() });
    }
    // The redirect lands on this host's loopback; from a browser on another
    // machine it cannot load, so the address is pasted back.
    auth_note(app, name, json!({ "state": "open", "url": url.as_str(), "paste": true }));
    Ok(())
}

/// Trade the authorization code for tokens and keep them.
async fn exchange(app: &Arc<App>, p: &Pending, code: &str) -> Result<()> {
    let h = &app.held;
    let c = h.get(&p.name).with_context(|| format!("{} was removed", p.name))?;
    let (Some(auth), Some(client)) = (c.auth, c.client) else { bail!("{} has no sign-in in progress. Start it again.", p.name) };
    let form = vec![
        ("grant_type", "authorization_code".to_string()),
        ("code", code.to_string()),
        ("redirect_uri", p.redirect.clone()),
        ("code_verifier", p.verifier.clone()),
        ("resource", c.url.clone()),
    ];
    let tokens = h.token_call(&auth, &client, form).await.context("The sign-in could not be finished")?;
    h.update(&p.name, |c| {
        c.tokens = Some(tokens);
        c.open = false;
    });
    Ok(())
}

/// What the browser was sent back with: finish the sign-in it names.
async fn finish(app: &Arc<App>, p: &Pending, q: &HashMap<String, String>) -> Result<()> {
    if let Some(error) = q.get("error") {
        let text = q.get("error_description").unwrap_or(error);
        bail!("The sign-in was refused: {}", clip(&text.chars().filter(|c| !c.is_control()).collect::<String>(), 200));
    }
    let code = q.get("code").filter(|c| !c.is_empty()).context("That address has no authorization code")?;
    exchange(app, p, code).await
}

/// `mcp.callback`: the address the browser ended on, pasted by the user.
async fn pasted(app: &Arc<App>, server: &str, url: &str) -> Result<()> {
    let u = Url::parse(url).ok().context("Paste the whole address the browser ended on")?;
    let q: HashMap<String, String> = u.query_pairs().into_owned().collect();
    let state = q.get("state").filter(|s| !s.is_empty()).context("That address has no `state`: paste the whole address the browser ended on")?;
    let p = app.held.take_pending(state)?;
    if p.name != server {
        bail!("That address belongs to another connector's sign-in. Start this one again.");
    }
    finish(app, &p, &q).await
}

fn page(code: StatusCode, title: &str, text: &str) -> Response {
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let html = format!(
        "<!doctype html><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Switchyard</title>\
         <body style=\"font:16px system-ui,sans-serif;margin:15vh auto;max-width:28em;padding:0 1em\"><h1 style=\"font-size:20px\">{}</h1><p>{}</p>",
        escape(title),
        escape(text)
    );
    (code, Html(html)).into_response()
}

/// `GET /connector/callback`: where a sign-in's redirect lands when the
/// browser runs on this machine. The `state` decides which sign-in it is.
pub async fn callback(State(app): State<Arc<App>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, Query(q): Query<HashMap<String, String>>) -> Response {
    if !peer.ip().is_loopback() {
        return page(StatusCode::FORBIDDEN, "Not from here", "A sign-in finishes on the machine that runs Switchyard.");
    }
    let pending = q.get("state").filter(|s| !s.is_empty()).context("That address has no `state`.").and_then(|s| app.held.take_pending(s));
    let p = match pending {
        Ok(p) => p,
        Err(e) => return page(StatusCode::BAD_REQUEST, "Not connected", &format!("{e:#}")),
    };
    match finish(&app, &p, &q).await {
        Ok(()) => {
            signed_in(&app, &p.name);
            page(StatusCode::OK, "Connected", &format!("{} is connected. You can close this tab.", p.name))
        }
        Err(e) => {
            auth_failed(&app, &p.name, &e);
            page(StatusCode::BAD_REQUEST, "Not connected", &clip(&format!("{e:#}"), 500))
        }
    }
}

/// The bridges a session's ACP agent is given: one stdio MCP server per
/// connector that is signed in and that the session's scope allows.
pub fn launches(app: &App, sid: &str) -> Vec<Value> {
    // yardd's own engine threads (naming and the like) work without tools.
    if crate::private::internal(sid) {
        return Vec::new();
    }
    let Some(base) = app.held.base.lock().unwrap().clone() else { return Vec::new() };
    let Ok(exe) = std::env::current_exe() else { return Vec::new() };
    let scope = crate::scope::of_session(app, sid);
    let mut names: Vec<String> = app
        .held
        .list
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c.usable() && scope.as_ref().is_none_or(|s| s.allows(&c.name)))
        .map(|c| c.name.clone())
        .collect();
    // The same list twice must read the same: a change reopens the session.
    names.sort();
    names
        .into_iter()
        .map(|name| {
            crate::agent_tools::Launch {
                command: exe.to_string_lossy().into_owned(),
                args: vec!["mcp-proxy".into()],
                env: vec![("YARD_CONNECTOR_URL".into(), format!("{base}/connector-proxy")), ("YARD_CONNECTOR_KEY".into(), app.held.key_for(sid, &name))],
            }
            .acp_named(&name)
        })
        .collect()
}

/// What the bridge's client reads when a message could not be passed on:
/// a failed tool call for the model, a protocol error otherwise.
fn refusal(msg: &Value, why: &str) -> Vec<Value> {
    if !is_request(msg) {
        return Vec::new();
    }
    let mut answer = json!({ "jsonrpc": "2.0", "id": msg["id"] });
    if msg["method"] == "tools/call" {
        answer["result"] = json!({ "content": [{ "type": "text", "text": why }], "isError": true });
    } else {
        answer["error"] = json!({ "code": -32603, "message": why });
    }
    vec![answer]
}

/// Pass one of the agent's messages to the connector and collect what
/// comes back for it.
async fn forward(app: &Arc<App>, bridge: &Bridge, msg: &Value) -> Result<Vec<Value>> {
    let name = &bridge.connector;
    let request = is_request(msg);
    let wait = if request { CALL_WAIT } else { NOTE_WAIT };
    let starting = msg["method"] == "initialize";
    let mut up = if starting {
        let fresh = Upstream { init: Some(msg.clone()), ..Default::default() };
        *bridge.upstream.lock().unwrap() = fresh.clone();
        fresh
    } else {
        bridge.upstream.lock().unwrap().clone()
    };
    let mut reply = call(app, name, &up, msg, wait).await?;
    // The server forgot the session (it restarted, or let it expire): open a
    // new one as the agent did, then ask again.
    if reply.status == StatusCode::NOT_FOUND && up.session.is_some() && !starting {
        if let Some(mut init) = up.init.clone() {
            init["id"] = json!("switchyard-reopen");
            up.session = None;
            up.version = None;
            let opened = call(app, name, &up, &init, NOTE_WAIT).await?;
            let version = opened.answer(&init["id"]).filter(|_| opened.status.is_success()).map(|m| m["result"]["protocolVersion"].as_str().map(str::to_string));
            if let Some(version) = version {
                up.session = opened.session;
                up.version = version;
                *bridge.upstream.lock().unwrap() = up.clone();
                let _ = call(app, name, &up, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }), NOTE_WAIT).await;
                reply = call(app, name, &up, msg, wait).await?;
            }
        }
    }
    if !reply.status.is_success() {
        bail!("{name} answered {}", reply.status.as_u16());
    }
    if starting {
        let mut up = bridge.upstream.lock().unwrap();
        up.session = reply.session.clone();
        up.version = reply.answer(&msg["id"]).and_then(|m| m["result"]["protocolVersion"].as_str()).map(str::to_string);
    }
    if request && reply.answer(&msg["id"]).is_none() {
        bail!("{name} ended its answer early");
    }
    Ok(reply.messages)
}

/// `POST /connector-proxy`: a bridge's messages. Only from this machine,
/// and only with a key yardd made for one session and one connector.
pub async fn proxy(State(app): State<Arc<App>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, body: Bytes) -> Response {
    let refuse = |code: StatusCode, why: &str| (code, Json(json!({ "error": why }))).into_response();
    if !peer.ip().is_loopback() || headers.contains_key("origin") {
        return refuse(StatusCode::FORBIDDEN, "connectors answer only local processes");
    }
    let key = headers.get("authorization").and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    let Some(bridge) = (!key.is_empty()).then(|| app.held.keys.lock().unwrap().get(key).cloned()).flatten() else {
        return refuse(StatusCode::UNAUTHORIZED, "unknown key");
    };
    let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
        return refuse(StatusCode::BAD_REQUEST, "expected JSON");
    };
    let answer = |messages: Vec<Value>| Json(json!({ "messages": messages })).into_response();
    // The bridge is going away: let the server drop its side too.
    if msg["switchyard"] == "close" {
        let up = std::mem::take(&mut *bridge.upstream.lock().unwrap());
        end_session(&app, &bridge.connector, &Upstream { init: None, ..up }).await;
        return answer(Vec::new());
    }
    let name = &bridge.connector;
    // The scope is read now, not when the key was made: a list the owner
    // shortened applies to a session that is still open.
    let allowed = app.meta(&bridge.sid).is_some() && crate::scope::of_session(&app, &bridge.sid).is_none_or(|s| s.allows(name));
    if !allowed {
        return answer(refusal(&msg, &format!("{name} is not available to this conversation.")));
    }
    match forward(&app, &bridge, &msg).await {
        Ok(messages) => answer(messages),
        Err(e) if e.is::<NeedsAuth>() => answer(refusal(&msg, &format!("{name} needs a sign-in: open Connectors in Switchyard and connect it again."))),
        Err(e) => answer(refusal(&msg, &format!("{name} did not answer: {}", clip(&format!("{e:#}"), 300)))),
    }
}

/// `yardd mcp-proxy`: the stdio MCP server an ACP agent starts for one
/// connector. Every line goes to yardd (`YARD_CONNECTOR_URL`,
/// `YARD_CONNECTOR_KEY`), which talks to the real server.
pub async fn serve_stdio() -> Result<()> {
    let url = std::env::var("YARD_CONNECTOR_URL").context("YARD_CONNECTOR_URL is not set")?;
    let key = std::env::var("YARD_CONNECTOR_KEY").context("YARD_CONNECTOR_KEY is not set")?;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        let mut out = tokio::io::stdout();
        while let Some(msg) = rx.recv().await {
            if out.write_all(format!("{msg}\n").as_bytes()).await.is_err() || out.flush().await.is_err() {
                break;
            }
        }
    });
    async fn pass(url: &str, key: &str, msg: &Value, tx: &tokio::sync::mpsc::UnboundedSender<Value>) {
        // Past yardd's own wait for the server, so its answer wins.
        match crate::agent_tools::post(url, key, msg, CALL_WAIT + Duration::from_secs(30)).await {
            Ok(v) => {
                for m in v["messages"].as_array().into_iter().flatten() {
                    let _ = tx.send(m.clone());
                }
            }
            Err(e) => {
                for m in refusal(msg, &format!("Switchyard did not answer: {e:#}")) {
                    let _ = tx.send(m);
                }
            }
        }
    }
    let mut calls = tokio::task::JoinSet::new();
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&line) else { continue };
        // Requests run side by side, so a slow tool call holds up nothing.
        // `initialize`, notifications and answers keep their order.
        if is_request(&msg) && msg["method"] != "initialize" {
            let (url, key, tx) = (url.clone(), key.clone(), tx.clone());
            calls.spawn(async move { pass(&url, &key, &msg, &tx).await });
        } else {
            pass(&url, &key, &msg, &tx).await;
        }
    }
    while calls.join_next().await.is_some() {}
    drop(tx);
    let _ = writer.await;
    let _ = crate::agent_tools::post(&url, &key, &json!({ "switchyard": "close" }), Duration::from_secs(5)).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_or_this_machine() {
        assert!(endpoint("https://auth.example.com/token", "x").is_ok());
        assert!(endpoint("http://127.0.0.1:9000/token", "x").is_ok());
        assert!(endpoint("http://localhost:9000/token", "x").is_ok());
        assert!(endpoint("http://[::1]:9000/token", "x").is_ok());
        assert!(endpoint("http://auth.example.com/token", "x").is_err());
        assert!(endpoint("http://127.0.0.1.evil.com/token", "x").is_err());
        assert!(endpoint("https://user:pw@auth.example.com/token", "x").is_err());
        assert!(endpoint("file:///etc/passwd", "x").is_err());
        assert!(endpoint("javascript:alert(1)", "x").is_err());
    }

    #[test]
    fn added_connectors_are_https_or_local_with_a_port() {
        assert!(check_new("linear", "https://mcp.linear.app/mcp").is_ok());
        assert!(check_new("local", "http://127.0.0.1:8080/mcp").is_ok());
        assert!(check_new("local", "http://127.0.0.1/mcp").is_err());
        assert!(check_new("far", "http://mcp.example.com:80/mcp").is_err());
        assert!(check_new("bad name", "https://mcp.linear.app/mcp").is_err());
        assert!(check_new("bad name", "http://127.0.0.1:8080/mcp").is_err());
        assert!(check_new("Switchyard", "https://mcp.linear.app/mcp").is_err());
    }

    #[test]
    fn well_known_places_are_path_aware_first() {
        let list = |u: &str, name: &str, appended: bool| well_known(&Url::parse(u).unwrap(), name, appended).iter().map(Url::to_string).collect::<Vec<_>>();
        assert_eq!(
            list("https://mcp.example.com/v1/mcp?x=1", "oauth-protected-resource", false),
            ["https://mcp.example.com/.well-known/oauth-protected-resource/v1/mcp", "https://mcp.example.com/.well-known/oauth-protected-resource"]
        );
        assert_eq!(list("https://auth.example.com", "oauth-authorization-server", false), ["https://auth.example.com/.well-known/oauth-authorization-server"]);
        assert_eq!(
            list("https://auth.example.com/tenant/", "openid-configuration", true),
            [
                "https://auth.example.com/.well-known/openid-configuration/tenant",
                "https://auth.example.com/tenant/.well-known/openid-configuration",
                "https://auth.example.com/.well-known/openid-configuration"
            ]
        );
    }

    #[test]
    fn a_challenge_names_its_metadata() {
        let h = r#"Bearer realm="mcp", error="invalid_token", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource", scope="read write""#;
        assert_eq!(challenge_param(h, "resource_metadata").as_deref(), Some("https://mcp.example.com/.well-known/oauth-protected-resource"));
        assert_eq!(challenge_param(h, "scope").as_deref(), Some("read write"));
        assert_eq!(challenge_param("Bearer resource_metadata=https://a.example/x, realm=y", "resource_metadata").as_deref(), Some("https://a.example/x"));
        assert_eq!(challenge_param("Bearer realm=\"x\"", "resource_metadata"), None);
        assert_eq!(challenge_param("Bearer xscope=\"a\"", "scope"), None);
    }

    #[test]
    fn pkce_is_s256() {
        // RFC 7636, appendix B.
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        assert_ne!(random(32).unwrap(), random(32).unwrap());
        assert_eq!(random(32).unwrap().len(), 43);
    }

    #[test]
    fn an_event_stream_gives_its_messages_as_they_complete() {
        let mut buf = b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\nid: 7\r\ndata: {\"id\":1,\r\ndata: \"result\":{}}\r\n\r\n: ping\n\ndata: {\"id\":2".to_vec();
        let got = sse_messages(&mut buf);
        assert_eq!(got, [json!({ "jsonrpc": "2.0", "method": "notifications/progress" }), json!({ "id": 1, "result": {} })]);
        assert_eq!(buf, b"data: {\"id\":2");
        buf.extend_from_slice(b",\"result\":1}\n\n");
        assert_eq!(sse_messages(&mut buf), [json!({ "id": 2, "result": 1 })]);
        assert!(buf.is_empty());
    }

    #[test]
    fn scopes_are_plain_tokens() {
        assert_eq!(scopes(&json!(["read", "write", "bad scope", ""])), "read write");
        assert_eq!(scopes(&json!("a b")), "a b");
        assert_eq!(scopes(&Value::Null), "");
    }

    #[test]
    fn the_file_is_private_and_a_state_works_once() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("yardd-held-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let h = Held::load(dir.clone());
        h.edit(|l| l.push(Connector { name: "a".into(), url: "https://a.example/mcp".into(), auth: None, client: None, tokens: Some(Tokens { access: "s3cret".into(), refresh: None, expires_at: None }), open: false, tools: 2 }));
        let mode = std::fs::metadata(dir.join(FILE)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let again = Held::load(dir.clone());
        assert_eq!(again.get("a").unwrap().tokens.unwrap().access, "s3cret");

        h.pending.lock().unwrap().insert("s1".into(), Pending { name: "a".into(), verifier: "v".into(), redirect: "r".into(), at: now_ms() });
        h.pending.lock().unwrap().insert("old".into(), Pending { name: "a".into(), verifier: "v".into(), redirect: "r".into(), at: now_ms() - SIGN_IN.as_millis() as u64 - 1 });
        assert!(h.take_pending("old").is_err(), "expired");
        assert!(h.take_pending("nope").is_err());
        assert!(h.take_pending("s1").is_ok());
        assert!(h.take_pending("s1").is_err(), "single use");
        let _ = std::fs::remove_dir_all(dir);
    }
}
