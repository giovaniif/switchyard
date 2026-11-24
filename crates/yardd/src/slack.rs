//! Slack: the owner talks to his agents from Slack.
//!
//! A Slack thread is not a session of its own. A message to the bot (a DM, or
//! an @mention in a channel) goes into one agent's one conversation, tagged
//! with where it came from (`origin`, kind `slack`), and that agent's answer
//! is posted back to the Slack thread it came from (the Slack arm of the
//! reply hook in origins.rs calls `answer`). While the agent works the
//! message gets :eyes:, then :white_check_mark: or :x:.
//!
//! Transport is Socket Mode: the app-level token (`xapp-`) opens a WebSocket
//! with `apps.connections.open`, so the server needs no public address; the
//! bot token (`xoxb-`) makes the Web API calls. Both are kept in `slack.json`
//! in the data dir (mode 0600) and never leave yardd: the snapshot carries
//! only their last four characters, and no log line names them.
//!
//! Only Slack users on the allowlist reach an agent (agents can have shell
//! access). Anyone else gets one ephemeral "not allowed" note, and shows up
//! on the Connectors page as a request to approve: that is also how the owner
//! adds himself, with his first DM. Bots, the app itself included, are
//! ignored.
//!
//! Routing: a follow-up in a Slack thread goes to the agent the thread's
//! first message went to; else a message starting with an agent's name
//! (`Reviewer …`, `@Reviewer …`, `Reviewer: …`) goes to it; else the
//! channel's default agent, then the default agent; else the bot answers with
//! the agents' names. In a channel, a message without an @mention is read
//! only as a follow-up in a thread already routed.
//!
//! `YARD_SLACK_API` points the Web API (and so Socket Mode) elsewhere: tests
//! and `verify/yv` use the fake in `verify/fakes/fake-slack`.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::{
    origins::Owed,
    state::{now_ms, App},
};

/// A Slack message stays readable below this; longer answers are split.
const PART: usize = 3500;
const MAX_REQUESTS: usize = 10;
const MAX_CHANNELS: usize = 50;
const MAX_THREADS: usize = 1000;
/// With no frame from Slack for this long, the connection is redialled.
const SILENT: Duration = Duration::from_secs(90);

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Config {
    app_token: String,
    bot_token: String,
    /// Slack users who may talk to agents.
    allowed: Vec<Person>,
    /// Users who tried and are not allowed, newest first, for the owner to approve.
    requests: Vec<Request>,
    /// Users already told they are not allowed (told once).
    told: Vec<String>,
    default_agent: Option<String>,
    /// Channels the bot has been mentioned in, each with an optional default agent.
    channels: Vec<Channel>,
    /// Which agent each routed Slack thread went to (`team/channel/thread_ts`).
    threads: Vec<Link>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Person {
    id: String,
    name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    id: String,
    name: String,
    /// Where they wrote: `Direct message` or `#channel`.
    place: String,
    at: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Channel {
    id: String,
    name: String,
    #[serde(default)]
    agent: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Link {
    key: String,
    agent: String,
    at: u64,
}

/// The connection as the Connectors page shows it.
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    /// `off` (no tokens), `connecting`, `connected` or `error`.
    state: String,
    team: String,
    bot: String,
    error: Option<String>,
}

/// Who the bot is, from `auth.test`.
#[derive(Clone)]
struct Me {
    team_id: String,
    bot_user: String,
}

pub struct Slack {
    file: PathBuf,
    cfg: Mutex<Config>,
    status: Mutex<Status>,
    /// Bumped when the tokens change or Retry is clicked: the connection redials.
    changed: watch::Sender<u64>,
    /// Answers are posted one at a time, so a thread reads them in order.
    posting: tokio::sync::Mutex<()>,
    /// User and channel names by id.
    names: Mutex<HashMap<String, String>>,
    /// Recent `channel:ts`, so a message that comes as an app_mention and a
    /// channel message (or is redelivered) is handled once.
    seen: Mutex<VecDeque<String>>,
    http: OnceLock<reqwest::Client>,
}

impl Slack {
    pub fn load(dir: PathBuf) -> Slack {
        let file = dir.join("slack.json");
        let cfg = match std::fs::read_to_string(&file) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("yardd: {} is unreadable ({e}); Slack starts unset", file.display());
                let _ = std::fs::rename(&file, file.with_extension("json.bad"));
                Config::default()
            }),
            Err(_) => Config::default(),
        };
        let state = if cfg.app_token.is_empty() || cfg.bot_token.is_empty() { "off" } else { "connecting" };
        Slack {
            file,
            cfg: Mutex::new(cfg),
            status: Mutex::new(Status { state: state.into(), ..Default::default() }),
            changed: watch::channel(0).0,
            posting: tokio::sync::Mutex::new(()),
            names: Mutex::new(HashMap::new()),
            seen: Mutex::new(VecDeque::new()),
            http: OnceLock::new(),
        }
    }

    /// What the UI sees: never a token, only its kind and last four characters.
    pub fn view(&self) -> Value {
        let cfg = self.cfg.lock().unwrap();
        let status = self.status.lock().unwrap().clone();
        json!({
            "configured": !cfg.app_token.is_empty() && !cfg.bot_token.is_empty(),
            "appToken": hint(&cfg.app_token),
            "botToken": hint(&cfg.bot_token),
            "state": status.state,
            "team": status.team,
            "bot": status.bot,
            "error": status.error,
            "allowed": cfg.allowed,
            "requests": cfg.requests,
            "defaultAgent": cfg.default_agent,
            "channels": cfg.channels,
        })
    }

    fn tokens(&self) -> (String, String) {
        let cfg = self.cfg.lock().unwrap();
        (cfg.app_token.clone(), cfg.bot_token.clone())
    }

    fn bot_token(&self) -> String {
        self.cfg.lock().unwrap().bot_token.clone()
    }

    fn change(&self, f: impl FnOnce(&mut Config)) {
        let mut cfg = self.cfg.lock().unwrap();
        f(&mut cfg);
        if let Err(e) = save(&self.file, &cfg) {
            eprintln!("yardd: cannot save {}: {e}", self.file.display());
        }
    }

    fn set_status(&self, app: &App, f: impl FnOnce(&mut Status)) {
        {
            let mut status = self.status.lock().unwrap();
            f(&mut status);
        }
        broadcast(app);
    }

    fn http(&self) -> &reqwest::Client {
        self.http.get_or_init(|| {
            crypto();
            reqwest::Client::builder().timeout(Duration::from_secs(20)).build().expect("an HTTP client")
        })
    }

    /// One Web API call, form-encoded, with the token as a bearer header (so
    /// it is in no URL or log). A rate limit is waited out twice.
    async fn api(&self, token: &str, method: &str, form: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}{method}", api_base());
        for attempt in 0.. {
            let res = self.http().post(&url).bearer_auth(token).form(form).send().await.map_err(|e| anyhow!("{method}: {}", e.without_url()))?;
            let status = res.status();
            if status.as_u16() == 429 && attempt < 2 {
                let wait = res.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok()).unwrap_or(1u64);
                tokio::time::sleep(Duration::from_secs(wait.min(30))).await;
                continue;
            }
            let bytes = res.bytes().await.map_err(|e| anyhow!("{method}: {}", e.without_url()))?;
            let body: Value = serde_json::from_slice(&bytes).map_err(|_| anyhow!("{method}: Slack answered HTTP {status} without JSON"))?;
            if body["ok"] == true {
                return Ok(body);
            }
            bail!("{method}: {}", body["error"].as_str().unwrap_or("failed"));
        }
        unreachable!()
    }

    async fn post(&self, token: &str, channel: &str, thread: &str, text: &str) -> Result<()> {
        self.api(token, "chat.postMessage", &[("channel", channel), ("thread_ts", thread), ("text", text), ("unfurl_links", "false")]).await.map(|_| ())
    }

    async fn react(&self, token: &str, channel: &str, ts: &str, name: &str, add: bool) {
        let method = if add { "reactions.add" } else { "reactions.remove" };
        // already_reacted, no_reaction: the reaction is as wanted.
        let _ = self.api(token, method, &[("channel", channel), ("timestamp", ts), ("name", name)]).await;
    }

    async fn user_name(&self, token: &str, user: &str) -> String {
        if let Some(name) = self.names.lock().unwrap().get(user) {
            return name.clone();
        }
        let name = match self.api(token, "users.info", &[("user", user)]).await {
            Ok(v) => {
                let u = &v["user"];
                [&u["profile"]["display_name"], &u["real_name"], &u["profile"]["real_name"], &u["name"]]
                    .iter()
                    .find_map(|n| n.as_str().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string))
                    .unwrap_or_else(|| user.to_string())
            }
            Err(_) => return user.to_string(),
        };
        self.names.lock().unwrap().insert(user.to_string(), name.clone());
        name
    }

    async fn channel_name(&self, token: &str, channel: &str, im: bool) -> String {
        if im {
            return "Direct message".into();
        }
        if let Some(name) = self.names.lock().unwrap().get(channel) {
            return name.clone();
        }
        let name = match self.api(token, "conversations.info", &[("channel", channel)]).await {
            Ok(v) if v["channel"]["is_im"] == true => "Direct message".to_string(),
            Ok(v) => v["channel"]["name"].as_str().map(|n| format!("#{n}")).unwrap_or_else(|| channel.to_string()),
            Err(_) => return channel.to_string(),
        };
        self.names.lock().unwrap().insert(channel.to_string(), name.clone());
        name
    }

    fn first_time(&self, key: String) -> bool {
        let mut seen = self.seen.lock().unwrap();
        if seen.contains(&key) {
            return false;
        }
        seen.push_back(key);
        if seen.len() > 500 {
            seen.pop_front();
        }
        true
    }
}

/// `xoxb-…abcd`: enough to tell which token is set.
fn hint(token: &str) -> String {
    if token.is_empty() {
        return String::new();
    }
    let kind = token.split('-').next().unwrap_or("");
    let tail: String = token.chars().rev().take(4).collect::<Vec<_>>().into_iter().rev().collect();
    format!("{kind}-…{tail}")
}

fn api_base() -> String {
    let base = std::env::var("YARD_SLACK_API").ok().filter(|b| !b.trim().is_empty()).unwrap_or_else(|| "https://slack.com/api/".into());
    if base.ends_with('/') {
        base
    } else {
        format!("{base}/")
    }
}

/// rustls is built with ring only; say so once, before any TLS.
fn crypto() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Written aside with mode 0600, then renamed: the tokens are never readable
/// by others, not even for a moment.
fn save(file: &std::path::Path, cfg: &Config) -> std::io::Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = file.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    f.write_all(&serde_json::to_vec_pretty(cfg)?)?;
    f.sync_all()?;
    std::fs::rename(&tmp, file)
}

fn broadcast(app: &App) {
    app.broadcast(json!({ "op": "slack", "slack": app.slack.view() }));
}

/// `U…` or `W…` (an Enterprise Grid user).
fn user_id_ok(id: &str) -> bool {
    id.len() >= 3 && id.len() <= 20 && id.starts_with(['U', 'W']) && id.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// The Connectors page's ops. Every change broadcasts `slack`.
pub async fn handle(app: &Arc<App>, msg: &Value) -> Result<()> {
    let slack = &app.slack;
    match msg["op"].as_str().unwrap_or("") {
        "slack.tokens" => {
            let app_token = msg["appToken"].as_str().unwrap_or("").trim().to_string();
            let bot_token = msg["botToken"].as_str().unwrap_or("").trim().to_string();
            if !app_token.is_empty() && (!app_token.starts_with("xapp-") || app_token.len() < 12) {
                bail!("The app-level token starts with xapp-. It is under Basic Information, App-Level Tokens.");
            }
            if !bot_token.is_empty() && (!bot_token.starts_with("xoxb-") || bot_token.len() < 12) {
                bail!("The bot token starts with xoxb-. It is under OAuth & Permissions, after installing the app.");
            }
            if app_token.chars().chain(bot_token.chars()).any(char::is_whitespace) {
                bail!("A token is one word with no spaces.");
            }
            let (old_app, old_bot) = slack.tokens();
            if (app_token.is_empty() && old_app.is_empty()) || (bot_token.is_empty() && old_bot.is_empty()) {
                bail!("Paste both tokens: the app-level token (xapp-) and the bot token (xoxb-).");
            }
            slack.change(|c| {
                if !app_token.is_empty() {
                    c.app_token = app_token;
                }
                if !bot_token.is_empty() {
                    c.bot_token = bot_token;
                }
            });
            slack.set_status(app, |s| *s = Status { state: "connecting".into(), ..Default::default() });
            slack.changed.send_modify(|n| *n += 1);
        }
        "slack.disconnect" => {
            slack.change(|c| {
                c.app_token.clear();
                c.bot_token.clear();
            });
            slack.set_status(app, |s| *s = Status { state: "off".into(), ..Default::default() });
            slack.changed.send_modify(|n| *n += 1);
        }
        "slack.retry" => {
            slack.changed.send_modify(|n| *n += 1);
        }
        "slack.allow" => {
            let user = msg["user"].as_str().unwrap_or("").trim().to_uppercase();
            if !user_id_ok(&user) {
                bail!("A Slack member ID looks like U0123ABCD. Find it in Slack under the person's profile, More, Copy member ID.");
            }
            slack.change(|c| {
                let name = c.requests.iter().find(|r| r.id == user).map(|r| r.name.clone());
                let name = name.or_else(|| msg["name"].as_str().map(str::to_string)).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| user.clone());
                c.requests.retain(|r| r.id != user);
                c.told.retain(|t| *t != user);
                if !c.allowed.iter().any(|p| p.id == user) {
                    c.allowed.push(Person { id: user.clone(), name });
                }
            });
        }
        "slack.disallow" => {
            let user = msg["user"].as_str().context("missing `user`")?;
            slack.change(|c| c.allowed.retain(|p| p.id != user));
        }
        "slack.dismiss" => {
            let user = msg["user"].as_str().context("missing `user`")?;
            slack.change(|c| c.requests.retain(|r| r.id != user));
        }
        "slack.route" => {
            let agent = msg["agent"].as_str().filter(|a| !a.is_empty()).map(str::to_string);
            if let Some(id) = &agent {
                app.agents.get(id).context("no such agent")?;
            }
            match msg["channel"].as_str() {
                Some(channel) => {
                    let mut found = false;
                    slack.change(|c| {
                        if let Some(ch) = c.channels.iter_mut().find(|ch| ch.id == channel) {
                            ch.agent = agent;
                            found = true;
                        }
                    });
                    if !found {
                        bail!("no such channel");
                    }
                }
                None => slack.change(|c| c.default_agent = agent),
            }
        }
        other => bail!("unknown op `{other}`"),
    }
    broadcast(app);
    Ok(())
}

/// Why a connection ended without an error.
enum Ended {
    /// Slack asked for a new connection (`disconnect`): dial again now.
    Refresh,
    /// The socket closed; `up` if it had said hello.
    Closed { up: bool },
}

/// The Socket Mode connection, for yardd's whole life: dialled whenever
/// both tokens are set, again after Slack's `disconnect` or a drop (backing
/// off to a minute), and at once when the tokens change.
pub async fn run(app: Arc<App>) {
    crypto();
    let (events, events_rx) = mpsc::unbounded_channel::<(Me, Value)>();
    // Messages are handled one at a time, in the order Slack sent them, so
    // two quick messages in a thread reach the agent in order.
    tokio::spawn(handle_events(app.clone(), events_rx));
    let mut changed = app.slack.changed.subscribe();
    let mut backoff = Duration::from_secs(1);
    loop {
        changed.borrow_and_update();
        let (app_token, bot_token) = app.slack.tokens();
        if app_token.is_empty() || bot_token.is_empty() {
            app.slack.set_status(&app, |s| *s = Status { state: "off".into(), ..Default::default() });
            if changed.changed().await.is_err() {
                return;
            }
            continue;
        }
        app.slack.set_status(&app, |s| {
            s.state = "connecting".into();
        });
        let ended = tokio::select! {
            r = connect(&app, &app_token, &bot_token, &events) => r,
            _ = changed.changed() => {
                backoff = Duration::from_secs(1);
                continue;
            }
        };
        let wait = match ended {
            Ok(Ended::Refresh) => {
                backoff = Duration::from_secs(1);
                Duration::ZERO
            }
            Ok(Ended::Closed { up }) => {
                if up {
                    backoff = Duration::from_secs(1);
                }
                backoff
            }
            Err(e) => {
                let error = format!("{e:#}");
                eprintln!("yardd: slack: {error}");
                app.slack.set_status(&app, |s| {
                    s.state = "error".into();
                    s.error = Some(error);
                });
                backoff
            }
        };
        tokio::select! {
            _ = tokio::time::sleep(wait) => backoff = (backoff * 2).min(Duration::from_secs(60)),
            _ = changed.changed() => backoff = Duration::from_secs(1),
        }
    }
}

async fn connect(app: &Arc<App>, app_token: &str, bot_token: &str, events: &mpsc::UnboundedSender<(Me, Value)>) -> Result<Ended> {
    let slack = &app.slack;
    let who = slack.api(bot_token, "auth.test", &[]).await.map_err(|e| refused("The bot token was refused", "", e))?;
    let me = Me { team_id: who["team_id"].as_str().unwrap_or("").into(), bot_user: who["user_id"].as_str().unwrap_or("").into() };
    let team = who["team"].as_str().unwrap_or("").to_string();
    let bot = who["user"].as_str().unwrap_or("").to_string();
    let open = slack.api(app_token, "apps.connections.open", &[]).await.map_err(|e| refused("The app-level token was refused", " It needs the connections:write scope.", e))?;
    let url = open["url"].as_str().context("Slack gave no Socket Mode address")?;
    // The address carries a ticket: it stays out of every message.
    let (ws, _) = tokio::time::timeout(Duration::from_secs(20), tokio_tungstenite::connect_async(url))
        .await
        .map_err(|_| anyhow!("Slack's Socket Mode did not answer"))?
        .map_err(|e| anyhow!("Could not open Slack's Socket Mode connection ({})", ws_error(&e)))?;
    let (mut sink, mut stream) = ws.split();
    let mut up = false;
    let mut last = Instant::now();
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    tick.tick().await;
    loop {
        tokio::select! {
            frame = stream.next() => {
                last = Instant::now();
                let text = match frame {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None => return Ok(Ended::Closed { up }),
                    Some(Ok(_)) => continue,
                    Some(Err(e)) => bail!("The Slack connection dropped ({})", ws_error(&e)),
                };
                let envelope: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                // Every envelope is acknowledged, or Slack sends it again.
                if let Some(id) = envelope["envelope_id"].as_str() {
                    sink.send(Message::Text(json!({ "envelope_id": id }).to_string())).await.map_err(|e| anyhow!("The Slack connection dropped ({})", ws_error(&e)))?;
                }
                match envelope["type"].as_str().unwrap_or("") {
                    "hello" => {
                        up = true;
                        let (team, bot) = (team.clone(), bot.clone());
                        slack.set_status(app, |s| *s = Status { state: "connected".into(), team, bot, error: None });
                    }
                    "disconnect" => {
                        if envelope["reason"] == "link_disabled" {
                            bail!("Socket Mode is turned off for this Slack app. Turn it on under Settings, Socket Mode.");
                        }
                        return Ok(Ended::Refresh);
                    }
                    "events_api" => {
                        let _ = events.send((me.clone(), envelope["payload"].clone()));
                    }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                if last.elapsed() > SILENT {
                    bail!("Slack stopped answering");
                }
                sink.send(Message::Ping(Vec::new())).await.map_err(|e| anyhow!("The Slack connection dropped ({})", ws_error(&e)))?;
            }
        }
    }
}

/// A token Slack turned down reads as such; anything else (no network, a
/// wrong `YARD_SLACK_API`) as Slack not being reached.
fn refused(what: &str, hint: &str, e: anyhow::Error) -> anyhow::Error {
    let text = format!("{e:#}");
    let auth = ["invalid_auth", "not_authed", "account_inactive", "token_revoked", "token_expired", "not_allowed_token_type", "missing_scope"];
    if auth.iter().any(|code| text.ends_with(code)) {
        anyhow!("{what} ({text}).{hint}")
    } else {
        anyhow!("Could not reach Slack ({text}).")
    }
}

/// A WebSocket error without the address it was dialling.
fn ws_error(e: &tokio_tungstenite::tungstenite::Error) -> String {
    use tokio_tungstenite::tungstenite::Error as E;
    match e {
        E::Http(res) => format!("HTTP {}", res.status()),
        E::Url(_) => "a bad address".into(),
        E::Io(io) => io.kind().to_string(),
        E::Tls(_) => "a TLS failure".into(),
        E::ConnectionClosed | E::AlreadyClosed => "closed".into(),
        _ => "protocol error".into(),
    }
}

async fn handle_events(app: Arc<App>, mut rx: mpsc::UnboundedReceiver<(Me, Value)>) {
    while let Some((me, payload)) = rx.recv().await {
        if let Err(e) = on_event(&app, &me, &payload).await {
            eprintln!("yardd: slack: {e:#}");
        }
    }
}

/// One Slack event: a DM, an @mention, or a channel message that may be a
/// follow-up in a routed thread.
async fn on_event(app: &Arc<App>, me: &Me, payload: &Value) -> Result<()> {
    let slack = &app.slack;
    let ev = &payload["event"];
    let kind = ev["type"].as_str().unwrap_or("");
    if kind != "app_mention" && kind != "message" {
        return Ok(());
    }
    // Bots (this app's own answers too) and edits, deletions and joins.
    let subtype = ev["subtype"].as_str();
    if !ev["bot_id"].is_null() || !matches!(subtype, None | Some("file_share") | Some("thread_broadcast")) {
        return Ok(());
    }
    let (Some(user), Some(channel), Some(ts)) = (ev["user"].as_str(), ev["channel"].as_str(), ev["ts"].as_str()) else { return Ok(()) };
    if user == me.bot_user {
        return Ok(());
    }
    let team = payload["team_id"].as_str().or(ev["team"].as_str()).unwrap_or(&me.team_id).to_string();
    let im = ev["channel_type"] == "im" || (ev["channel_type"].is_null() && channel.starts_with('D'));
    let thread = ev["thread_ts"].as_str().unwrap_or(ts).to_string();
    let key = format!("{team}/{channel}/{thread}");
    let raw = ev["text"].as_str().unwrap_or("");
    let mention = format!("<@{}", me.bot_user);
    let known = linked(slack, &key);
    if kind == "message" && !im {
        // A mention comes as app_mention too; a plain channel message counts
        // only as a follow-up in a thread an agent already has.
        if raw.contains(&mention) || known.is_none() || ev["thread_ts"].is_null() {
            return Ok(());
        }
    }
    if !slack.first_time(format!("{channel}:{ts}")) {
        return Ok(());
    }
    let token = slack.bot_token();
    if token.is_empty() {
        return Ok(());
    }
    let place = slack.channel_name(&token, channel, im).await;
    if !im {
        slack.change(|c| match c.channels.iter_mut().find(|ch| ch.id == channel) {
            Some(ch) => ch.name = place.clone(),
            None => {
                c.channels.push(Channel { id: channel.to_string(), name: place.clone(), agent: None });
                if c.channels.len() > MAX_CHANNELS {
                    c.channels.remove(0);
                }
            }
        });
    }
    let allowed = slack.cfg.lock().unwrap().allowed.iter().any(|p| p.id == user);
    if !allowed {
        let name = slack.user_name(&token, user).await;
        let mut tell = false;
        slack.change(|c| {
            c.requests.retain(|r| r.id != user);
            c.requests.insert(0, Request { id: user.to_string(), name, place: place.clone(), at: now_ms() });
            c.requests.truncate(MAX_REQUESTS);
            if !c.told.iter().any(|t| t == user) {
                c.told.push(user.to_string());
                tell = true;
            }
        });
        broadcast(app);
        if tell {
            let note = format!(
                "Sorry, you can't talk to these agents yet. The owner can allow you in Switchyard, under Connectors, Slack (your member ID is {user})."
            );
            let _ = slack.api(&token, "chat.postEphemeral", &[("channel", channel), ("user", user), ("text", &note)]).await;
        }
        return Ok(());
    }
    broadcast(app);
    let text = from_slack(&strip_mention(raw, &me.bot_user));
    let agents: Vec<(String, String)> = app.agents.all().into_iter().map(|a| (a.id, a.name)).collect();
    let name_of = |id: &str| agents.iter().find(|(a, _)| a == id).map(|(_, n)| n.clone());
    // A thread keeps its agent; a deleted one lets the message route anew.
    let (agent, text) = match known.filter(|id| name_of(id).is_some()) {
        Some(id) => (Some(id), text),
        None => match route(&text, &agents) {
            Some((id, rest)) => (Some(id), rest),
            None => {
                let (channel_default, default) = {
                    let cfg = slack.cfg.lock().unwrap();
                    (cfg.channels.iter().find(|ch| ch.id == channel).and_then(|ch| ch.agent.clone()), cfg.default_agent.clone())
                };
                (channel_default.filter(|id| name_of(id).is_some()).or(default.filter(|id| name_of(id).is_some())), text)
            }
        },
    };
    let Some(agent) = agent else {
        let names = agents.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ");
        let note = format!("Which agent? Start your message with its name: {names}. Or pick a default agent in Switchyard, under Connectors, Slack.");
        slack.post(&token, channel, &thread, &note).await?;
        return Ok(());
    };
    let name = name_of(&agent).unwrap_or_default();
    if text.trim().is_empty() {
        let note = if ev["files"].is_array() { format!("Files are not passed on to {name} yet. Write the message as text.") } else { format!("What should {name} do? Write it after the name.") };
        slack.post(&token, channel, &thread, &note).await?;
        return Ok(());
    }
    slack.change(|c| {
        c.threads.retain(|l| l.key != key);
        c.threads.push(Link { key: key.clone(), agent: agent.clone(), at: now_ms() });
        if c.threads.len() > MAX_THREADS {
            c.threads.remove(0);
        }
    });
    let user_name = slack.user_name(&token, user).await;
    let origin = json!({
        "kind": "slack",
        "team": team,
        "channel": channel,
        "channelName": place,
        "threadTs": thread,
        "ts": ts,
        "user": user,
        "userName": user_name,
    });
    slack.react(&token, channel, ts, "eyes", true).await;
    if let Err(e) = crate::agents::deliver(app, &agent, text.trim(), &origin).await {
        slack.react(&token, channel, ts, "eyes", false).await;
        slack.react(&token, channel, ts, "x", true).await;
        slack.post(&token, channel, &thread, &format!("Could not reach {name}: {e:#}")).await?;
    }
    Ok(())
}

fn linked(slack: &Slack, key: &str) -> Option<String> {
    slack.cfg.lock().unwrap().threads.iter().find(|l| l.key == key).map(|l| l.agent.clone())
}

/// The Slack arm of the reply hook (`origins::reply`): what a turn owes the
/// Slack messages it answered, posted once in each of their threads, and the
/// :eyes: on each message swapped for how the turn ended. `approval` is the
/// note that the turn waits on the owner; the turn goes on, so :eyes: stays.
pub async fn answer(app: &Arc<App>, owed: &Owed, agent: &str, origins: &[Value]) {
    crypto();
    let slack = &app.slack;
    let _order = slack.posting.lock().await;
    let token = slack.bot_token();
    let note = |text: String| app.emit(&owed.sid, json!({ "type": "note", "text": text }));
    if token.is_empty() {
        if owed.status != "approval" {
            note("Slack is not connected, so this answer was not posted there.".into());
        }
        return;
    }
    let mut threads: Vec<(String, String, String)> = Vec::new();
    for o in origins {
        let t = (o["channel"].as_str().unwrap_or("").to_string(), o["threadTs"].as_str().unwrap_or("").to_string(), o["channelName"].as_str().unwrap_or("Slack").to_string());
        if !t.0.is_empty() && !t.1.is_empty() && !threads.iter().any(|x| x.0 == t.0 && x.1 == t.1) {
            threads.push(t);
        }
    }
    let why = || Some(clip_line(&owed.text, 300)).filter(|e| !e.is_empty()).map(|e| to_mrkdwn(&e));
    let (parts, mark) = match owed.status.as_str() {
        "approval" => {
            let title = clip_line(&owed.text, 200);
            let text = if title.is_empty() { format!("{agent} needs your approval in Switchyard.") } else { format!("{agent} needs your approval in Switchyard: {}", to_mrkdwn(&title)) };
            (vec![text], None)
        }
        "completed" => {
            let parts = match owed.text.trim() {
                "" => vec![format!("{agent} finished without a written answer.")],
                t => split(&to_mrkdwn(t), PART),
            };
            (parts, Some("white_check_mark"))
        }
        "interrupted" => (vec![format!("{agent} was stopped in Switchyard.")], Some("x")),
        "unread" => (vec![format!("{agent} was stopped in Switchyard before it read your message.")], Some("x")),
        "failed" => (vec![why().map_or_else(|| format!("{agent} could not finish."), |w| format!("{agent} could not finish: {w}"))], Some("x")),
        _ => (vec![why().map_or_else(|| format!("Your message did not reach {agent}."), |w| format!("Your message did not reach {agent}: {w}"))], Some("x")),
    };
    for (channel, thread, place) in &threads {
        for part in &parts {
            if let Err(e) = slack.post(&token, channel, thread, part).await {
                note(format!("Could not post to Slack ({place}): {e:#}"));
                break;
            }
        }
    }
    if let Some(mark) = mark {
        for o in origins {
            let (Some(channel), Some(ts)) = (o["channel"].as_str(), o["ts"].as_str()) else { continue };
            slack.react(&token, channel, ts, "eyes", false).await;
            slack.react(&token, channel, ts, mark, true).await;
        }
    }
}

fn clip_line(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    format!("{}…", line.chars().take(max - 1).collect::<String>())
}

/// The text without the bot's leading @mention(s).
pub fn strip_mention(text: &str, bot_user: &str) -> String {
    let mut rest = text.trim_start();
    loop {
        let Some(after) = rest.strip_prefix("<@").and_then(|r| r.strip_prefix(bot_user)) else { break };
        let Some(end) = after.find('>') else { break };
        if !(end == 0 || after.starts_with('|')) {
            break;
        }
        rest = after[end + 1..].trim_start();
    }
    rest.to_string()
}

/// Slack's message markup as plain text: `<url|label>` reads `label (url)`,
/// `<@U1>` `@U1`, `<#C1|name>` `#name`, `<!here>` `@here`; entities decoded.
pub fn from_slack(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let inner = &after[..end];
        let (target, label) = match inner.split_once('|') {
            Some((t, l)) => (t, Some(l)),
            None => (inner, None),
        };
        let piece = if let Some(id) = target.strip_prefix('@') {
            format!("@{}", label.unwrap_or(id))
        } else if let Some(id) = target.strip_prefix('#') {
            format!("#{}", label.unwrap_or(id))
        } else if let Some(special) = target.strip_prefix('!') {
            format!("@{}", label.map(|l| l.trim_start_matches('@')).unwrap_or(special.split('^').next().unwrap_or(special)))
        } else {
            let url = target.strip_prefix("mailto:").unwrap_or(target);
            match label {
                Some(l) if l != url && l != target => format!("{l} ({url})"),
                _ => url.to_string(),
            }
        };
        out.push_str(&piece);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The agent a message names first, and the message without the name:
/// `Reviewer look`, `@Reviewer look`, `Reviewer: look`, `reviewer, look`.
pub fn route(text: &str, agents: &[(String, String)]) -> Option<(String, String)> {
    let t = text.trim_start();
    let t = t.strip_prefix('@').unwrap_or(t);
    let end = t.find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_')).unwrap_or(t.len());
    let (word, rest) = t.split_at(end);
    if word.is_empty() || !(rest.is_empty() || rest.starts_with([':', ',']) || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    let (id, _) = agents.iter().find(|(_, name)| name.eq_ignore_ascii_case(word) || name.to_lowercase() == word.to_lowercase())?;
    let rest = rest.trim_start_matches([':', ',']).trim();
    Some((id.clone(), rest.to_string()))
}

/// Markdown as Slack mrkdwn: bold, italics, strikes, links, headings and
/// bullets converted; `&`, `<` and `>` escaped everywhere, so an answer can
/// never ping `<!channel>` or a user; code kept as code.
pub fn to_mrkdwn(md: &str) -> String {
    let mut out = Vec::new();
    let mut fence = false;
    for line in md.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            // Slack shows no language and knows only backtick fences.
            out.push("```".to_string());
            fence = !fence;
            continue;
        }
        let line = escape(line);
        if fence {
            out.push(line);
            continue;
        }
        let indent = &line[..line.len() - line.trim_start().len()];
        let body = line.trim_start();
        let converted = if let Some(h) = heading(body) {
            format!("*{}*", inline(h).replace('*', ""))
        } else if let Some(item) = ["- ", "* ", "+ "].iter().find_map(|b| body.strip_prefix(b)) {
            format!("{indent}• {}", inline(item))
        } else {
            format!("{indent}{}", inline(body))
        };
        out.push(converted);
    }
    out.join("\n")
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn heading(line: &str) -> Option<&str> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) {
        line[hashes..].strip_prefix(' ').map(str::trim)
    } else {
        None
    }
}

/// Inline Markdown outside code spans.
fn inline(line: &str) -> String {
    let mut out = String::new();
    for (i, part) in line.split('`').enumerate() {
        if i > 0 {
            out.push('`');
        }
        if i % 2 == 1 {
            out.push_str(part);
            continue;
        }
        let s = pairs(part, "**", "\u{1}");
        let s = pairs(&s, "__", "\u{1}");
        let s = pairs(&s, "~~", "~");
        let s = pairs(&s, "*", "_");
        out.push_str(&links(&s.replace('\u{1}', "*")));
    }
    out
}

/// `delim text delim` as `with text with`, when the text hugs both ends.
fn pairs(s: &str, delim: &str, with: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find(delim) {
        let after = &rest[open + delim.len()..];
        match after.find(delim) {
            Some(close) if close > 0 && !after[..close].starts_with(char::is_whitespace) && !after[..close].ends_with(char::is_whitespace) => {
                out.push_str(&rest[..open]);
                out.push_str(with);
                out.push_str(&after[..close]);
                out.push_str(with);
                rest = &after[close + delim.len()..];
            }
            _ => {
                out.push_str(&rest[..open + delim.len()]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// `[label](url)` as `<url|label>`.
fn links(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let found = after.find("](").and_then(|mid| after[mid + 2..].find(')').map(|end| (mid, mid + 2 + end)));
        match found {
            Some((mid, end)) if mid > 0 && !after[mid + 2..end].contains(char::is_whitespace) && after[mid + 2..end].contains(':') => {
                let label = after[..mid].replace('|', "/");
                out.push_str(&rest[..open]);
                out.push_str(&format!("<{}|{}>", &after[mid + 2..end], label));
                rest = &after[end + 1..];
            }
            _ => {
                out.push_str(&rest[..open + 1]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Text in parts of at most `max` bytes, cut between lines; a code block cut
/// in two is closed and reopened.
pub fn split(text: &str, max: usize) -> Vec<String> {
    if text.len() <= max {
        return vec![text.to_string()];
    }
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut fence = false;
    let mut lines: VecDeque<String> = text.lines().map(str::to_string).collect();
    while let Some(line) = lines.pop_front() {
        // A line longer than a part is cut at a character boundary.
        if line.len() + 8 > max {
            let mut cut = max - 8;
            while !line.is_char_boundary(cut) {
                cut -= 1;
            }
            lines.push_front(line[cut..].to_string());
            lines.push_front(line[..cut].to_string());
            continue;
        }
        if !cur.is_empty() && cur.len() + line.len() + 1 + 4 > max {
            if fence {
                cur.push_str("\n```");
            }
            parts.push(std::mem::take(&mut cur));
            if fence {
                cur.push_str("```");
            }
        }
        if !cur.is_empty() {
            cur.push('\n');
        }
        cur.push_str(&line);
        if line.trim_start().starts_with("```") {
            fence = !fence;
        }
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents() -> Vec<(String, String)> {
        vec![("scout".into(), "Scout".into()), ("g1".into(), "Reviewer".into()), ("g2".into(), "pr-bot".into())]
    }

    #[test]
    fn a_message_goes_to_the_agent_it_names_first() {
        let a = agents();
        for text in ["Reviewer look at #12", "@Reviewer look at #12", "Reviewer: look at #12", "reviewer, look at #12", "  REVIEWER   look at #12"] {
            assert_eq!(route(text, &a), Some(("g1".into(), "look at #12".into())), "{text:?}");
        }
        assert_eq!(route("pr-bot: go", &a), Some(("g2".into(), "go".into())));
        assert_eq!(route("Scout", &a), Some(("scout".into(), "".into())));
        for text in ["Reviewers look", "look Reviewer", "Reviewer's turn", "", "@ Reviewer x", "Nobody: hi"] {
            assert_eq!(route(text, &a), None, "{text:?}");
        }
    }

    #[test]
    fn the_bots_mention_is_dropped_and_slack_markup_read_as_text() {
        assert_eq!(strip_mention("<@UBOT> Reviewer hi", "UBOT"), "Reviewer hi");
        assert_eq!(strip_mention("<@UBOT|switchyard>   <@UBOT> hi", "UBOT"), "hi");
        assert_eq!(strip_mention("hi <@UBOT>", "UBOT"), "hi <@UBOT>");
        assert_eq!(strip_mention("<@UBOTX> hi", "UBOT"), "<@UBOTX> hi");
        assert_eq!(
            from_slack("see <https://x.dev/a?b=1&amp;c=2|the docs> and <https://y.dev>, ask <@U1> in <#C1|ops> &lt;now&gt; <!here>"),
            "see the docs (https://x.dev/a?b=1&c=2) and https://y.dev, ask @U1 in #ops <now> @here"
        );
    }

    #[test]
    fn markdown_becomes_slack_mrkdwn_safely() {
        assert_eq!(to_mrkdwn("**Done** with _care_ and *style*, ~~old~~"), "*Done* with _care_ and _style_, ~old~");
        assert_eq!(to_mrkdwn("## Summary\n- one\n  * two"), "*Summary*\n• one\n  • two");
        assert_eq!(to_mrkdwn("see [the PR](https://github.com/o/r/pull/1)"), "see <https://github.com/o/r/pull/1|the PR>");
        // Nothing an agent writes can mention everyone or forge a link.
        assert_eq!(to_mrkdwn("<!channel> <@U1> <https://evil|ok> a & b"), "&lt;!channel&gt; &lt;@U1&gt; &lt;https://evil|ok&gt; a &amp; b");
        // Code stays code: no conversion inside, the language dropped.
        assert_eq!(to_mrkdwn("run `a **b** c` then\n```rust\nlet x = **y** < 2;\n```"), "run `a **b** c` then\n```\nlet x = **y** &lt; 2;\n```");
        assert_eq!(to_mrkdwn("2 * 3 and a*b"), "2 * 3 and a*b");
    }

    #[test]
    fn long_answers_are_split_between_lines_and_code_blocks_reopened() {
        let text = (0..100).map(|i| format!("line {i} {}", "x".repeat(40))).collect::<Vec<_>>().join("\n");
        let parts = split(&text, 1000);
        assert!(parts.len() > 4 && parts.iter().all(|p| p.len() <= 1000));
        assert_eq!(parts.join("\n"), text);
        let code = format!("intro\n```\n{}\n```", (0..60).map(|i| format!("code {i} {}", "y".repeat(30))).collect::<Vec<_>>().join("\n"));
        let parts = split(&code, 800);
        assert!(parts.len() > 2);
        for p in &parts {
            assert!(p.len() <= 800);
            assert_eq!(p.matches("```").count() % 2, 0, "every part closes its code block: {p}");
        }
        let long = "é".repeat(3000);
        assert!(split(&long, 1000).iter().all(|p| p.len() <= 1000));
    }

    #[test]
    fn tokens_show_only_their_kind_and_end() {
        assert_eq!(hint("xoxb-1234-5678-abcdWXYZ"), "xoxb-…WXYZ");
        assert_eq!(hint(""), "");
        assert!(user_id_ok("U0123ABCD") && user_id_ok("W12AB"));
        assert!(!user_id_ok("u0123") && !user_id_ok("C0123ABCD") && !user_id_ok("U01 23"));
    }
}
