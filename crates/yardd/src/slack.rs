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
//! first message went to; else a message starting with an agent's handle or
//! name (`Reviewer …`, `@Reviewer …`, `Reviewer: …`, `@Custom-Domains …`,
//! `Custom Domains: …`; the longest that fits) goes to it; else the
//! channel's default agent, then the default agent; else the bot answers with
//! the agents' names. In a channel, a message without an @mention is read
//! only as a follow-up in a thread already routed.
//!
//! An agent can also have a Slack app of its own, as Orca's bots do, so people
//! @mention it by its name. The owner pastes a Slack App Configuration Refresh
//! Token once per workspace (`slack.grant`); yardd keeps it rotated and makes
//! the agent's app from a manifest (`apps.manifest.create`), and keeps its
//! name, description and icon in step with the agent (`apps.manifest.update`,
//! `apps.icon.set`). The install is approved in Slack, which sends the browser
//! back to `/slack/oauth` on an https address yardd serves its page on,
//! whichever window asked (only the owner's browser has to reach it; with
//! none, the owner pastes the bot token). Its events come one of two ways:
//! over Socket Mode, which takes an app-level token Slack only makes by hand,
//! or, when the owner gave yardd a public https address for Slack
//! (`YARD_SLACK_PUBLIC_URL`), as signed HTTP requests to `/slack/events`, so
//! the configuration token is the only thing pasted. Its messages always go
//! to its agent, through the same allowlist, origins and answers as the
//! shared bot's, and whoever installed it is allowed.
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
    /// Configuration tokens, one per workspace, for making agents' apps.
    workspaces: Vec<Workspace>,
    /// Agents' own Slack apps.
    apps: Vec<AgentApp>,
    /// The public https address Slack sends agents' apps' events to (set in
    /// the UI; `YARD_SLACK_PUBLIC_URL` wins). Empty: Socket Mode.
    public_url: String,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Workspace {
    team_id: String,
    team_name: String,
    /// Who made the configuration token: the owner, in Slack.
    user_id: String,
    refresh_token: String,
    access_token: String,
    expires_at: u64,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AgentApp {
    agent: String,
    team_id: String,
    team_name: String,
    app_id: String,
    client_id: String,
    client_secret: String,
    /// Where Slack sends the browser after the install this time (one of
    /// `redirects`); empty: the owner pastes the bot token instead.
    redirect: String,
    /// Every install redirect its manifest lists.
    redirects: Vec<String>,
    authorize_url: String,
    bot_token: String,
    bot_user: String,
    /// Its bot's handle, as the manifest named it.
    handle: String,
    /// Who installed it, added to the allowlist.
    owner: String,
    owner_name: String,
    app_token: String,
    /// Where Slack sends its events over HTTP (`<public>/slack/events`);
    /// empty: Socket Mode, with `app_token`.
    events_url: String,
    /// Signs those requests (from `apps.manifest.create`).
    signing_secret: String,
    /// The manifest Slack has: a different one is due when the agent's name
    /// or what it is for changed (empty for apps made before it was kept).
    manifest: String,
    /// The UI's key of the icon kept for it (`slack-icons/<agent>.png`), and
    /// whether Slack has that icon.
    icon_key: String,
    icon_set: bool,
    /// Why the last update did not reach Slack.
    sync_error: String,
    /// The only people its answers may @mention, chosen by the owner: an
    /// agent's text is untrusted, so it never widens this list itself.
    mentions: Vec<Person>,
}

impl AgentApp {
    fn step(&self) -> &'static str {
        if self.bot_token.is_empty() {
            "install"
        } else if self.events_url.is_empty() && self.app_token.is_empty() {
            "appToken"
        } else {
            "ready"
        }
    }
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

/// Who the bot is, from `auth.test`, and for an agent's own app, whose.
#[derive(Clone)]
struct Me {
    team_id: String,
    bot_user: String,
    /// The bot token its answers and notes are posted with.
    token: String,
    /// The agent whose own app this is; none for the shared bot.
    agent: Option<String>,
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
    /// Each agent app's connection, as its dialog shows it.
    app_status: Mutex<HashMap<String, Status>>,
    /// Each ready agent app's Socket Mode connection.
    app_tasks: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    /// Install links in flight: OAuth `state` → (agent, redirect, until).
    installs: Mutex<HashMap<String, (String, String, u64)>>,
    /// The connections' event queue, made when `run` starts.
    events: OnceLock<mpsc::UnboundedSender<(Me, Value)>>,
    /// Apps being made: Slack may check their events address before
    /// `apps.manifest.create` has answered with the secret that signs it.
    creating: std::sync::atomic::AtomicUsize,
    /// Recent `app/event_id` of events over HTTP, so a retry is handled once.
    event_ids: Mutex<VecDeque<String>>,
    /// Answers `GET /slack/events`, so a probe through the public address
    /// can tell it reached this yardd.
    probe: String,
    /// Whether the public address reached this yardd: none untried, "" yes,
    /// else why not.
    public_check: Mutex<Option<String>>,
    /// Updates to agents' apps go to Slack one at a time.
    syncing: tokio::sync::Mutex<()>,
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
        let mut cfg = cfg;
        // Apps made before redirects were listed had one at most.
        for a in &mut cfg.apps {
            if a.redirects.is_empty() && !a.redirect.is_empty() {
                a.redirects = vec![a.redirect.clone()];
            }
        }
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
            app_status: Mutex::default(),
            app_tasks: Mutex::default(),
            installs: Mutex::default(),
            events: OnceLock::new(),
            creating: Default::default(),
            event_ids: Mutex::default(),
            probe: random_hex(),
            public_check: Mutex::default(),
            syncing: tokio::sync::Mutex::new(()),
        }
    }

    /// The public https address for Slack's events, normalized; empty: none.
    pub fn public_url(&self) -> String {
        let env = std::env::var("YARD_SLACK_PUBLIC_URL").unwrap_or_default();
        let raw = if env.trim().is_empty() { self.cfg.lock().unwrap().public_url.clone() } else { env };
        public_address(&raw).unwrap_or_default()
    }

    /// What the UI sees: never a token, only its kind and last four characters.
    pub fn view(&self) -> Value {
        let cfg = self.cfg.lock().unwrap();
        let status = self.status.lock().unwrap().clone();
        let fixed = std::env::var("YARD_SLACK_PUBLIC_URL").ok().filter(|v| !v.trim().is_empty());
        let public = public_address(fixed.as_deref().unwrap_or(&cfg.public_url)).unwrap_or_default();
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
            "workspaces": cfg.workspaces.iter().map(|w| json!({ "teamId": w.team_id, "teamName": if w.team_name.is_empty() { &w.team_id } else { &w.team_name } })).collect::<Vec<_>>(),
            "agentApps": self.apps_view(&cfg),
            // Slack's events to agents' new apps come here, over HTTP.
            "publicUrl": public,
            "publicUrlFixed": fixed.is_some(),
            "publicCheck": *self.public_check.lock().unwrap(),
            // A new app's install can come back to yardd on its own.
            "canRedirect": !redirect_urls_with(&public).is_empty(),
        })
    }

    /// Each agent's own app: its step and connection, never a token.
    fn apps_view(&self, cfg: &Config) -> Value {
        let status = self.app_status.lock().unwrap();
        let map: serde_json::Map<String, Value> = cfg
            .apps
            .iter()
            .map(|a| {
                let mut v = json!({
                    "step": a.step(),
                    "appId": a.app_id,
                    "teamId": a.team_id,
                    "teamName": a.team_name,
                    "handle": a.handle,
                    "ownerName": a.owner_name,
                    "canRedirect": !a.redirects.is_empty(),
                    "eventsUrl": a.events_url,
                    "syncError": Some(&a.sync_error).filter(|e| !e.is_empty()),
                    "iconKey": a.icon_key,
                    "mentions": a.mentions,
                });
                if a.step() == "install" && !a.redirect.is_empty() {
                    v["authorizeUrl"] = json!(a.authorize_url);
                }
                if a.step() == "ready" {
                    // Over HTTP there is no connection to show: Slack calls in.
                    let idle = if a.events_url.is_empty() { "connecting" } else { "events" };
                    let st = status.get(&a.agent).cloned().unwrap_or_else(|| Status { state: idle.into(), ..Default::default() });
                    v["state"] = json!(st.state);
                    v["error"] = json!(st.error);
                }
                (a.agent.clone(), v)
            })
            .collect();
        Value::Object(map)
    }

    fn app_of(&self, agent: &str) -> Option<AgentApp> {
        self.cfg.lock().unwrap().apps.iter().find(|a| a.agent == agent).cloned()
    }

    /// Changes some of an app's fields in place, so a change made meanwhile
    /// (its bot token, say) is kept.
    fn update_app(&self, agent: &str, f: impl FnOnce(&mut AgentApp)) {
        self.change(|c| {
            if let Some(a) = c.apps.iter_mut().find(|a| a.agent == agent) {
                f(a);
            }
        });
    }

    fn put_app(&self, rec: AgentApp) {
        self.change(|c| match c.apps.iter_mut().find(|a| a.agent == rec.agent) {
            Some(slot) => *slot = rec,
            None => c.apps.push(rec),
        });
    }

    /// A Web API call that takes no bearer token (token rotation, the OAuth
    /// code exchange): the secret is in the form.
    async fn api_open(&self, method: &str, form: &[(&str, &str)]) -> Result<Value> {
        let url = format!("{}{method}", api_base());
        let res = self.http().post(&url).form(form).send().await.map_err(|e| anyhow!("{method}: {}", e.without_url()))?;
        let status = res.status();
        let bytes = res.bytes().await.map_err(|e| anyhow!("{method}: {}", e.without_url()))?;
        let body: Value = serde_json::from_slice(&bytes).map_err(|_| anyhow!("{method}: Slack answered HTTP {status} without JSON"))?;
        if body["ok"] == true {
            return Ok(body);
        }
        bail!("{method}: {}", body["error"].as_str().unwrap_or("failed"))
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

    /// The agent has a Slack app of its own (made, maybe not installed yet).
    pub fn has_app(&self, agent: &str) -> bool {
        self.cfg.lock().unwrap().apps.iter().any(|a| a.agent == agent)
    }

    /// Who the agent's answers may @mention, as it should write them.
    pub fn mentions_note(&self, agent: &str) -> Option<String> {
        let people = self.app_of(agent)?.mentions;
        (!people.is_empty()).then(|| people.iter().map(|p| format!("{} (<@{}>)", p.name, p.id)).collect::<Vec<_>>().join(", "))
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

/// The Connectors page's ops, and an agent's own app's steps (`SlackSetup`
/// in the UI). Every change broadcasts `slack`; the asking window also gets
/// it as the answer to its `ref`.
pub async fn handle(app: &Arc<App>, direct: &mpsc::UnboundedSender<String>, msg: &Value) -> Result<()> {
    let slack = &app.slack;
    let agent = || -> Result<crate::agents::Agent> { app.agents.get(msg["agentId"].as_str().context("missing `agentId`")?).filter(|a| !a.hosted).context("no such agent") };
    let token = || msg["token"].as_str().unwrap_or("").trim().to_string();
    match msg["op"].as_str().unwrap_or("") {
        "slack.grant" => {
            let t = token();
            if t.starts_with("xoxe.xoxp-") {
                bail!("That is the Access Token. Copy the Refresh Token, the one next to it.");
            }
            if !t.starts_with("xoxe-") || t.len() < 12 || t.chars().any(char::is_whitespace) {
                bail!("That is not a configuration refresh token. It starts with xoxe-.");
            }
            let v = slack.api_open("tooling.tokens.rotate", &[("refresh_token", &t)]).await.map_err(|e| refused("Slack refused the configuration token", " Generate a new one and paste it.", e))?;
            let team = keep_grant(slack, &v);
            // With an agent: its app is made in that workspace at once.
            if msg["agentId"].is_string() && !slack.has_app(&agent()?.id) {
                create_app(app, &agent()?, &team, msg["origin"].as_str().unwrap_or("")).await?;
            }
            // Updates that waited for a token that works go out now.
            let waiting: Vec<String> = slack.cfg.lock().unwrap().apps.iter().filter(|a| a.team_id == team).map(|a| a.agent.clone()).collect();
            for agent in waiting {
                let _ = sync_app(app, &agent).await;
            }
        }
        "slack.app.create" => {
            let team = msg["teamId"].as_str().context("missing `teamId`")?.to_string();
            create_app(app, &agent()?, &team, msg["origin"].as_str().unwrap_or("")).await?;
        }
        "slack.app.authorize" => authorize(app, &agent()?.id, msg["origin"].as_str().unwrap_or("")),
        "slack.app.icon" => {
            let a = agent()?;
            let key = msg["key"].as_str().unwrap_or("").trim().to_string();
            if key.is_empty() || key.len() > 64 {
                bail!("missing `key`");
            }
            use base64::Engine;
            let png = base64::engine::general_purpose::STANDARD.decode(msg["png"].as_str().unwrap_or("")).map_err(|_| anyhow!("The icon is not base64."))?;
            icon_ok(&png)?;
            let rec = slack.app_of(&a.id).context("Make its Slack app first.")?;
            if rec.icon_key != key || !rec.icon_set {
                let path = icon_path(slack, &a.id);
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&path, &png)?;
                slack.update_app(&a.id, |r| {
                    r.icon_key = key;
                    r.icon_set = false;
                });
                // A lapsed configuration token is shown in its dialog, not here.
                let _ = sync_app(app, &a.id).await;
            }
        }
        "slack.publicUrl" => {
            if std::env::var("YARD_SLACK_PUBLIC_URL").is_ok_and(|v| !v.trim().is_empty()) {
                bail!("This address is set by YARD_SLACK_PUBLIC_URL where Switchyard runs.");
            }
            let url = public_address(msg["url"].as_str().unwrap_or(""))?;
            slack.change(|c| c.public_url = url);
            *slack.public_check.lock().unwrap() = None;
            tokio::spawn(check_public(app.clone()));
            // Apps list it as an install address; those on HTTP get their events there.
            let all: Vec<String> = slack.cfg.lock().unwrap().apps.iter().map(|a| a.agent.clone()).collect();
            for agent in all {
                let _ = sync_app(app, &agent).await;
            }
        }
        "slack.app.botToken" => {
            let a = agent()?;
            let t = token();
            if !t.starts_with("xoxb-") || t.chars().any(char::is_whitespace) {
                bail!("That is not a bot token. It starts with xoxb-: Bot User OAuth Token, on the app's OAuth & Permissions page.");
            }
            let mut rec = slack.app_of(&a.id).context("Make its Slack app first.")?;
            let who = slack.api(&t, "auth.test", &[]).await.map_err(|e| refused("Slack refused that bot token", "", e))?;
            rec.bot_token = t;
            rec.bot_user = who["user_id"].as_str().unwrap_or_default().to_string();
            rec.team_name = who["team"].as_str().unwrap_or_default().to_string();
            // Pasted by hand: the owner is whoever made the configuration token.
            rec.owner = slack.cfg.lock().unwrap().workspaces.iter().find(|w| w.team_id == rec.team_id).map(|w| w.user_id.clone()).unwrap_or_default();
            installed(app, rec).await;
        }
        "slack.app.appToken" => {
            let a = agent()?;
            let t = token();
            if !t.starts_with("xapp-") || t.chars().any(char::is_whitespace) {
                bail!("That is not an app-level token. It starts with xapp-.");
            }
            let mut rec = slack.app_of(&a.id).context("Make its Slack app first.")?;
            slack.api(&t, "apps.connections.open", &[]).await.map_err(|e| refused("Slack refused that token", " It needs the connections:write scope.", e))?;
            rec.app_token = t;
            slack.put_app(rec);
            start_app(app, &a.id);
        }
        "slack.app.remove" => remove_app(app, &agent()?.id).await,
        "slack.app.mention" => {
            let a = agent()?;
            let user = msg["user"].as_str().unwrap_or("").trim().to_uppercase();
            if !user_id_ok(&user) {
                bail!("A Slack member ID looks like U0123ABCD. Find it in Slack under the person's profile, More, Copy member ID.");
            }
            let name = msg["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&user).to_string();
            slack.app_of(&a.id).context("Make its Slack app first.")?;
            slack.update_app(&a.id, |r| {
                r.mentions.retain(|p| p.id != user);
                r.mentions.push(Person { id: user, name });
            });
        }
        "slack.app.unmention" => {
            let a = agent()?;
            let user = msg["user"].as_str().context("missing `user`")?.to_string();
            slack.update_app(&a.id, |r| r.mentions.retain(|p| p.id != user));
        }
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
    let _ = direct.send(json!({ "op": "slack", "slack": slack.view(), "ref": msg["ref"] }).to_string());
    Ok(())
}

/// A rotated configuration token, kept for its workspace. Answers the team.
fn keep_grant(slack: &Slack, v: &Value) -> String {
    let team = v["team_id"].as_str().unwrap_or_default().to_string();
    let w = Workspace {
        team_id: team.clone(),
        team_name: String::new(),
        user_id: v["user_id"].as_str().unwrap_or_default().to_string(),
        refresh_token: v["refresh_token"].as_str().unwrap_or_default().to_string(),
        access_token: v["token"].as_str().unwrap_or_default().to_string(),
        expires_at: v["exp"].as_u64().map(|s| s * 1000).unwrap_or_else(|| now_ms() + 11 * 3600 * 1000),
    };
    slack.change(|c| {
        let name = c.workspaces.iter().find(|x| x.team_id == team).map(|x| x.team_name.clone()).unwrap_or_default();
        c.workspaces.retain(|x| x.team_id != team);
        c.workspaces.push(Workspace { team_name: name, ..w });
    });
    team
}

/// The workspace's configuration access token, rotated when it is about to
/// run out (they last 12 hours; each refresh token works once).
async fn config_token(slack: &Slack, team: &str) -> Result<String> {
    let w = slack.cfg.lock().unwrap().workspaces.iter().find(|w| w.team_id == team).cloned().context("No configuration token for that workspace. Paste one.")?;
    if w.expires_at > now_ms() + 5 * 60 * 1000 {
        return Ok(w.access_token);
    }
    let v = slack.api_open("tooling.tokens.rotate", &[("refresh_token", &w.refresh_token)]).await.map_err(|e| anyhow!("Slack no longer accepts this workspace's configuration token. Generate a new one and paste it. ({e:#})"))?;
    keep_grant(slack, &v);
    Ok(v["token"].as_str().unwrap_or_default().to_string())
}

/// The manifest of an agent's own app: a bot named after it, its events over
/// Socket Mode or (with `events_url`) HTTP, and the shared bot's scopes and
/// events, so it reads the same messages.
fn agent_manifest(name: &str, about: &str, redirects: &[String], events_url: &str) -> Value {
    let mut oauth = json!({ "scopes": { "bot": ["app_mentions:read", "chat:write", "im:history", "im:read", "im:write", "channels:history", "channels:read", "groups:history", "groups:read", "users:read", "reactions:write"] } });
    if !redirects.is_empty() {
        oauth["redirect_urls"] = json!(redirects);
    }
    let title = app_name(name);
    let about: String = if about.trim().is_empty() { format!("{title}, an agent in Switchyard.") } else { about.to_string() };
    let mut events = json!({ "bot_events": ["app_mention", "message.im", "message.channels", "message.groups"] });
    if !events_url.is_empty() {
        events["request_url"] = json!(events_url);
    }
    json!({
        "display_information": { "name": title, "description": about.chars().take(139).collect::<String>() },
        "features": {
            "bot_user": { "display_name": bot_handle(name), "always_online": true },
            "app_home": { "home_tab_enabled": false, "messages_tab_enabled": true, "messages_tab_read_only_enabled": false },
        },
        "oauth_config": oauth,
        "settings": {
            "event_subscriptions": events,
            "interactivity": { "is_enabled": false },
            "org_deploy_enabled": false,
            "socket_mode_enabled": events_url.is_empty(),
            "token_rotation_enabled": false,
        },
    })
}

/// The app's name in Slack: the agent's, as written, cut to Slack's 35
/// characters.
pub fn app_name(name: &str) -> String {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = name.chars().take(35).collect();
    match cut.trim() {
        "" => "Agent".into(),
        t => t.to_string(),
    }
}

/// The bot's display name, which is also how people @mention it. Slack makes
/// the bot's handle from it by keeping a-z, 0-9, `-`, `_` and `.` and cutting
/// it to 21 characters, so it is made that way here and the two agree: lower
/// case, accents dropped from their letters, spaces and every other separator
/// a dash, runs of dashes one, no separator at either end.
pub fn bot_handle(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let piece: &str = match c {
            'a'..='z' | '0'..='9' | '_' | '.' => {
                out.push(c);
                continue;
            }
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'æ' => "ae",
            'ç' | 'ć' | 'č' => "c",
            'ď' | 'đ' | 'ð' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
            'ğ' => "g",
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
            'ł' | 'ľ' => "l",
            'ñ' | 'ń' | 'ň' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
            'œ' => "oe",
            'ř' => "r",
            'ś' | 'š' | 'ş' => "s",
            'ß' => "ss",
            'ť' | 'ţ' => "t",
            'þ' => "th",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
            'ý' | 'ÿ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            // Other scripts and emoji: nothing Slack keeps.
            c if !c.is_ascii() && !c.is_whitespace() && !is_dash(c) => "",
            // Spaces, dashes, slashes, punctuation: a word break.
            _ => "-",
        };
        if piece == "-" {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        } else {
            out.push_str(piece);
        }
    }
    let cut: String = out.chars().take(21).collect();
    match cut.trim_matches(['-', '_', '.']) {
        "" => "agent".into(),
        t => t.to_string(),
    }
}

/// Dashes and spaces from beyond ASCII (en and em dashes, no-break space…).
fn is_dash(c: char) -> bool {
    matches!(c, '\u{2010}'..='\u{2015}' | '\u{2212}' | '\u{00a0}' | '\u{2000}'..='\u{200a}' | '\u{3000}' | '·' | '•' | '…')
}

/// An https address as the public one for Slack: no end slash, no query.
fn public_address(raw: &str) -> Result<String> {
    let url = raw.trim().trim_end_matches('/');
    if url.is_empty() {
        return Ok(String::new());
    }
    let host = url.strip_prefix("https://").context("The public address starts with https://.")?;
    if host.is_empty() || host.starts_with('/') || url.len() > 200 || url.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '?' | '#' | '@' | '\\' | '"' | '<' | '>')) {
        bail!("That is not a web address, such as https://slack.example.com.");
    }
    Ok(url.to_string())
}

/// Every address the install may send the browser back to: the https ones
/// yardd serves its page on (only the owner's browser has to reach them),
/// then the public one.
fn redirect_urls_with(public: &str) -> Vec<String> {
    let mut list: Vec<String> = crate::ws::allowed_origins().into_iter().filter(|o| o.starts_with("https://")).map(|o| format!("{o}/slack/oauth")).collect();
    if !public.is_empty() {
        list.push(format!("{public}/slack/oauth"));
    }
    let mut seen = Vec::new();
    list.retain(|u| {
        let new = !seen.contains(u);
        seen.push(u.clone());
        new
    });
    list
}

/// The install's redirect for a window at `origin`: its own address when the
/// app lists it, else the first one the app lists. A window cannot name
/// another site: only listed addresses are used.
fn redirect_for(origin: &str, listed: &[String]) -> String {
    let own = format!("{}/slack/oauth", origin.trim().trim_end_matches('/'));
    if listed.contains(&own) {
        own
    } else {
        listed.first().cloned().unwrap_or_default()
    }
}

/// The manifest the agent's app should have now (with its install addresses
/// and events address): its name and what it is for, the addresses yardd
/// has, and its events where they go.
fn wanted_manifest(slack: &Slack, rec: &AgentApp, a: &crate::agents::Agent) -> (String, Vec<String>, String) {
    let public = slack.public_url();
    // An app on HTTP follows the public address; one on Socket Mode stays
    // there (its connection token is pasted already).
    let events_url = if rec.events_url.is_empty() || public.is_empty() { rec.events_url.clone() } else { format!("{public}/slack/events") };
    let redirects = redirect_urls_with(&public);
    (agent_manifest(&a.name, &a.about, &redirects, &events_url).to_string(), redirects, events_url)
}

/// Counts an app being made while it lives (see `Slack::creating`).
struct Creating<'a>(&'a Slack);

impl<'a> Creating<'a> {
    fn new(slack: &'a Slack) -> Self {
        slack.creating.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Creating(slack)
    }
}

impl Drop for Creating<'_> {
    fn drop(&mut self) {
        self.0.creating.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }
}

async fn create_app(app: &Arc<App>, a: &crate::agents::Agent, team: &str, origin: &str) -> Result<()> {
    let slack = &app.slack;
    if let Some(old) = slack.app_of(&a.id) {
        bail!("{} already has a Slack app ({}). Remove it first.", a.name, old.app_id);
    }
    let config = config_token(slack, team).await?;
    let public = slack.public_url();
    let events_url = if public.is_empty() { String::new() } else { format!("{public}/slack/events") };
    let redirects = redirect_urls_with(&public);
    let manifest = agent_manifest(&a.name, &a.about, &redirects, &events_url).to_string();
    let made = {
        let _making = Creating::new(slack);
        slack.api(&config, "apps.manifest.create", &[("manifest", &manifest)]).await.map_err(|e| refused("Slack did not make the app", "", e))?
    };
    let team_name = slack.cfg.lock().unwrap().workspaces.iter().find(|w| w.team_id == team).map(|w| w.team_name.clone()).unwrap_or_default();
    slack.put_app(AgentApp {
        agent: a.id.clone(),
        team_id: team.to_string(),
        team_name,
        app_id: made["app_id"].as_str().unwrap_or_default().to_string(),
        client_id: made["credentials"]["client_id"].as_str().unwrap_or_default().to_string(),
        client_secret: made["credentials"]["client_secret"].as_str().unwrap_or_default().to_string(),
        signing_secret: made["credentials"]["signing_secret"].as_str().unwrap_or_default().to_string(),
        redirects,
        events_url,
        manifest,
        authorize_url: made["oauth_authorize_url"].as_str().unwrap_or_default().to_string(),
        handle: bot_handle(&a.name),
        ..Default::default()
    });
    authorize(app, &a.id, origin);
    Ok(())
}

/// An install link good for 30 minutes: Slack's own, with a redirect for the
/// window that asked and a one-time state.
fn authorize(app: &App, agent: &str, origin: &str) {
    let Some(mut rec) = app.slack.app_of(agent) else { return };
    let redirect = redirect_for(origin, &rec.redirects);
    if redirect.is_empty() || rec.authorize_url.is_empty() {
        return;
    }
    let state = format!("{}{}", app.id("slack-"), random_hex());
    app.slack.installs.lock().unwrap().insert(state.clone(), (agent.to_string(), redirect.clone(), now_ms() + 30 * 60 * 1000));
    let base = rec.authorize_url.split("&redirect_uri=").next().unwrap_or_default().split("&state=").next().unwrap_or_default().to_string();
    rec.authorize_url = format!("{base}&redirect_uri={}&state={state}", encode(&redirect));
    rec.redirect = redirect;
    app.slack.put_app(rec);
}

fn random_hex() -> String {
    let mut bytes = [0u8; 16];
    if std::fs::File::open("/dev/urandom").and_then(|mut f| std::io::Read::read_exact(&mut f, &mut bytes)).is_err() {
        bytes = now_ms().to_le_bytes().repeat(2).try_into().unwrap_or_default();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn encode(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

/// The app is installed: its installer may ask, and it connects once its
/// app-level token is there.
async fn installed(app: &Arc<App>, mut rec: AgentApp) {
    let slack = &app.slack;
    if !rec.owner.is_empty() {
        rec.owner_name = slack.user_name(&rec.bot_token, &rec.owner).await;
        let (id, name) = (rec.owner.clone(), rec.owner_name.clone());
        slack.change(|c| {
            c.requests.retain(|r| r.id != id);
            if !c.allowed.iter().any(|p| p.id == id) {
                c.allowed.push(Person { id, name });
            }
        });
    }
    if !rec.team_name.is_empty() {
        let (team, name) = (rec.team_id.clone(), rec.team_name.clone());
        slack.change(|c| {
            if let Some(w) = c.workspaces.iter_mut().find(|w| w.team_id == team) {
                w.team_name = name;
            }
        });
    }
    let ready = rec.step() == "ready";
    let agent = rec.agent.clone();
    slack.put_app(rec);
    if ready {
        start_app(app, &agent);
    }
    broadcast(app);
}

/// The agent leaves Slack: its connection closes and its app is deleted
/// (best effort: the configuration token may have lapsed).
pub async fn remove_app(app: &Arc<App>, agent: &str) {
    stop_app(app, agent);
    let Some(rec) = app.slack.app_of(agent) else { return };
    if let Ok(token) = config_token(&app.slack, &rec.team_id).await {
        if let Err(e) = app.slack.api(&token, "apps.manifest.delete", &[("app_id", &rec.app_id)]).await {
            eprintln!("yardd: could not delete Slack app {}: {e:#}", rec.app_id);
        }
    }
    app.slack.change(|c| c.apps.retain(|a| a.agent != agent));
    let _ = std::fs::remove_file(icon_path(&app.slack, agent));
    broadcast(app);
}

/// The agent's name or what it is for changed: its app follows, in the
/// background. When Slack cannot be told (a lapsed configuration token, say)
/// the save stands, and its dialog and its conversation say so.
pub fn agent_changed(app: &Arc<App>, agent: &str) {
    if !app.slack.has_app(agent) {
        return;
    }
    let (app, agent) = (app.clone(), agent.to_string());
    tokio::spawn(async move {
        if let Err(e) = sync_app(&app, &agent).await {
            let Some(a) = app.agents.get(&agent) else { return };
            if let Some(sid) = a.chat.as_deref().filter(|sid| app.meta(sid).is_some()) {
                app.emit(sid, json!({ "type": "note", "text": format!("{}'s Slack app still shows its old name: {e:#}", a.name) }));
            }
        }
    });
}

/// Brings an agent's app in Slack up to date: its name, handle, description
/// and install addresses (`apps.manifest.update`, which takes the whole
/// manifest) and its icon (`apps.icon.set`). Nothing goes to Slack when it
/// has it all already. A failure is kept for its dialog (`syncError`) and
/// answered; the next change, a restart or a new configuration token tries
/// again.
pub async fn sync_app(app: &Arc<App>, agent: &str) -> Result<()> {
    let slack = &app.slack;
    let _one = slack.syncing.lock().await;
    let (Some(rec), Some(a)) = (slack.app_of(agent), app.agents.get(agent)) else { return Ok(()) };
    let (manifest, redirects, events_url) = wanted_manifest(slack, &rec, &a);
    let icon = icon_path(slack, agent);
    let icon_due = !rec.icon_key.is_empty() && !rec.icon_set && icon.exists();
    let manifest_due = manifest != rec.manifest;
    let result = if !manifest_due && !icon_due {
        Ok(())
    } else {
        async {
            let token = config_token(slack, &rec.team_id).await?;
            if manifest_due {
                slack.api(&token, "apps.manifest.update", &[("app_id", &rec.app_id), ("manifest", &manifest)]).await.map_err(|e| refused("Slack did not update the app", "", e))?;
                let handle = bot_handle(&a.name);
                slack.update_app(agent, |r| {
                    r.manifest = manifest;
                    r.redirects = redirects;
                    r.events_url = events_url;
                    r.handle = handle;
                });
            }
            if icon_due {
                let png = std::fs::read(&icon)?;
                slack.set_icon(&token, &rec.app_id, &png).await.map_err(|e| refused("Slack did not take the icon", "", e))?;
                let key = rec.icon_key.clone();
                slack.update_app(agent, |r| r.icon_set = r.icon_key == key);
            }
            anyhow::Ok(())
        }
        .await
    };
    let error = result.as_ref().err().map(|e| format!("{e:#}")).unwrap_or_default();
    if error != rec.sync_error {
        slack.update_app(agent, |r| r.sync_error = error);
    }
    broadcast(app);
    result
}

/// Where an agent app's icon is kept, as the UI drew it.
fn icon_path(slack: &Slack, agent: &str) -> PathBuf {
    let safe: String = agent.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
    slack.file.with_file_name("slack-icons").join(format!("{safe}.png"))
}

/// A PNG Slack takes as an app icon: 512 to 2000 pixels a side.
fn icon_ok(png: &[u8]) -> Result<()> {
    if png.len() > 4 << 20 {
        bail!("The icon is larger than 4 MB.");
    }
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        bail!("The icon is not a PNG.");
    }
    let side = |at: usize| u32::from_be_bytes([png[at], png[at + 1], png[at + 2], png[at + 3]]);
    let (w, h) = (side(16), side(20));
    if !(512..=2000).contains(&w) || !(512..=2000).contains(&h) {
        bail!("Slack takes icons of 512 to 2000 pixels a side; this one is {w}×{h}.");
    }
    Ok(())
}

/// Tries the public address: it works when `GET <public>/slack/events`
/// answers with this yardd's probe.
async fn check_public(app: Arc<App>) {
    let public = app.slack.public_url();
    if public.is_empty() {
        return;
    }
    crypto();
    let url = format!("{public}/slack/events");
    let res = app.slack.http().get(&url).timeout(Duration::from_secs(10)).send().await;
    let check = match res {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(body) if body.contains(&app.slack.probe) => String::new(),
            _ => format!("{url} answers, but not from this Switchyard."),
        },
        Ok(r) => format!("{url} answers HTTP {}.", r.status().as_u16()),
        Err(e) => format!("{url} cannot be reached from here ({}).", e.without_url()),
    };
    if public == app.slack.public_url() {
        *app.slack.public_check.lock().unwrap() = Some(check);
        broadcast(&app);
    }
}

/// Requests larger than this are not Slack's events.
const EVENT_MAX: usize = 1 << 20;
/// Slack's timestamps older (or newer) than this are refused: a replay.
const EVENT_AGE: i64 = 5 * 60;

/// `GET /slack/events`: the probe's answer, so the owner can check the
/// public address reaches this yardd.
pub async fn events_probe(axum::extract::State(app): axum::extract::State<Arc<App>>) -> String {
    format!("Switchyard receives Slack's events here. {}\n", app.slack.probe)
}

/// `POST /slack/events`: Slack's Events API, for agents' apps made with a
/// public address. Nothing is read before its signature checks out with one
/// of those apps' signing secrets (`v0`, HMAC-SHA256 over the timestamp and
/// the body, within five minutes). Slack's address check is answered; an
/// event is handed to the same handling as Socket Mode's and acknowledged at
/// once (Slack wants an answer within 3 s and retries otherwise); a retry of
/// an event already handled is acknowledged only.
pub async fn events(axum::extract::State(app): axum::extract::State<Arc<App>>, headers: axum::http::HeaderMap, body: axum::body::Body) -> axum::response::Response {
    use axum::{http::StatusCode, response::IntoResponse};
    let slack = &app.slack;
    let Ok(body) = axum::body::to_bytes(body, EVENT_MAX).await else {
        return (StatusCode::PAYLOAD_TOO_LARGE, "too large").into_response();
    };
    let header = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let (ts, signature) = (header("x-slack-request-timestamp"), header("x-slack-signature"));
    if !fresh(&ts, (now_ms() / 1000) as i64) {
        return (StatusCode::UNAUTHORIZED, "stale or missing timestamp").into_response();
    }
    let mut rec = slack.signed_by(&ts, &body, &signature);
    // An app being made may be checked before Slack's answer gave its secret.
    let until = Instant::now() + Duration::from_millis(2500);
    while rec.is_none() && slack.creating.load(std::sync::atomic::Ordering::SeqCst) > 0 && Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(100)).await;
        rec = slack.signed_by(&ts, &body, &signature);
    }
    let Some(rec) = rec else {
        eprintln!("yardd: slack: refused a request to /slack/events that no agent's app signed");
        return (StatusCode::UNAUTHORIZED, "bad signature").into_response();
    };
    let payload: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match payload["type"].as_str().unwrap_or("") {
        "url_verification" => axum::Json(json!({ "challenge": payload["challenge"] })).into_response(),
        "event_callback" => {
            let id = format!("{}/{}", rec.app_id, payload["event_id"].as_str().unwrap_or(""));
            let first = {
                let mut seen = slack.event_ids.lock().unwrap();
                let first = !seen.contains(&id);
                if first {
                    seen.push_back(id);
                    if seen.len() > 1000 {
                        seen.pop_front();
                    }
                }
                first
            };
            if first && !rec.bot_token.is_empty() {
                if app.slack.app_status.lock().unwrap().get(&rec.agent).map_or(true, |s| s.state != "connected") {
                    app_status(&app, &rec.agent, Status { state: "connected".into(), team: rec.team_name.clone(), bot: rec.handle.clone(), error: None });
                }
                if let Some(tx) = slack.events.get() {
                    let me = Me { team_id: rec.team_id.clone(), bot_user: rec.bot_user.clone(), token: rec.bot_token.clone(), agent: Some(rec.agent.clone()) };
                    let _ = tx.send((me, payload));
                }
            }
            StatusCode::OK.into_response()
        }
        _ => StatusCode::OK.into_response(),
    }
}

/// Slack's timestamp is within five minutes of now.
fn fresh(ts: &str, now: i64) -> bool {
    ts.parse::<i64>().is_ok_and(|t| (now - t).abs() <= EVENT_AGE)
}

/// Slack's `v0` signature of a request: HMAC-SHA256 of `v0:<ts>:<body>`
/// with the app's signing secret, compared in constant time.
fn signed(secret: &str, ts: &str, body: &[u8], signature: &str) -> bool {
    let Some(tag) = signature.strip_prefix("v0=").and_then(unhex) else { return false };
    if secret.is_empty() {
        return false;
    }
    let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
    let mut msg = Vec::with_capacity(body.len() + ts.len() + 4);
    msg.extend_from_slice(b"v0:");
    msg.extend_from_slice(ts.as_bytes());
    msg.push(b':');
    msg.extend_from_slice(body);
    ring::hmac::verify(&key, &msg, &tag).is_ok()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || s.len() > 128 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

impl Slack {
    /// The agent's app (on HTTP) whose signing secret signed this request.
    fn signed_by(&self, ts: &str, body: &[u8], signature: &str) -> Option<AgentApp> {
        self.cfg.lock().unwrap().apps.iter().find(|a| !a.events_url.is_empty() && signed(&a.signing_secret, ts, body, signature)).cloned()
    }

    /// `apps.icon.set`, its image as `multipart/form-data` (built here:
    /// one file part is all it takes).
    async fn set_icon(&self, token: &str, app_id: &str, png: &[u8]) -> Result<()> {
        let method = "apps.icon.set";
        let boundary = format!("yard-{}", random_hex());
        let mut body = Vec::with_capacity(png.len() + 400);
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"app_id\"\r\n\r\n{app_id}\r\n").as_bytes());
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"icon.png\"\r\nContent-Type: image/png\r\n\r\n").as_bytes());
        body.extend_from_slice(png);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let res = self
            .http()
            .post(format!("{}{method}", api_base()))
            .bearer_auth(token)
            .header("content-type", format!("multipart/form-data; boundary={boundary}"))
            .body(body)
            .send()
            .await
            .map_err(|e| anyhow!("{method}: {}", e.without_url()))?;
        let status = res.status();
        let bytes = res.bytes().await.map_err(|e| anyhow!("{method}: {}", e.without_url()))?;
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| anyhow!("{method}: Slack answered HTTP {status} without JSON"))?;
        if v["ok"] == true {
            return Ok(());
        }
        bail!("{method}: {}", v["error"].as_str().unwrap_or("failed"))
    }
}

#[derive(Deserialize)]
pub struct Callback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// Where Slack sends the owner's browser after an agent's app is installed.
pub async fn oauth(axum::extract::State(app): axum::extract::State<Arc<App>>, axum::extract::Query(q): axum::extract::Query<Callback>) -> axum::response::Html<String> {
    let escape = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let page = |title: &str, body: &str| {
        axum::response::Html(format!(
            "<!doctype html><meta charset=utf-8><meta name=viewport content=\"width=device-width\"><title>{title}</title>\
<body style=\"font:15px system-ui;max-width:32em;margin:15vh auto;padding:0 20px;color:#222\"><h1 style=\"font-size:20px\">{title}</h1><p>{body}</p>\
<p><a href=\"/#/agents\">Back to Switchyard</a></p></body>"
        ))
    };
    let taken = q.state.as_deref().and_then(|s| app.slack.installs.lock().unwrap().remove(s)).filter(|(_, _, until)| *until > now_ms());
    let Some((agent, redirect, _)) = taken else {
        return page("This link has expired", "Open the agent in Switchyard and choose Approve in Slack again.");
    };
    if let Some(e) = q.error {
        return page("Not installed", &format!("Slack said: {}.", escape(&e)));
    }
    let Some(rec) = app.slack.app_of(&agent) else { return page("Not installed", "The agent left Slack meanwhile.") };
    crypto();
    let code = q.code.unwrap_or_default();
    match app.slack.api_open("oauth.v2.access", &[("client_id", &rec.client_id), ("client_secret", &rec.client_secret), ("code", &code), ("redirect_uri", &redirect)]).await {
        Ok(v) => {
            let mut rec = rec;
            rec.bot_token = v["access_token"].as_str().unwrap_or_default().to_string();
            rec.bot_user = v["bot_user_id"].as_str().unwrap_or_default().to_string();
            rec.team_name = v["team"]["name"].as_str().unwrap_or_default().to_string();
            rec.owner = v["authed_user"]["id"].as_str().unwrap_or_default().to_string();
            installed(&app, rec).await;
            let name = app.agents.get(&agent).map(|a| a.name).unwrap_or_default();
            let ready = app.slack.app_of(&agent).is_some_and(|a| a.step() == "ready");
            let next = if ready { "It is ready: mention it in Slack. You can close this tab." } else { "One step is left in Switchyard: its connection token. You can close this tab." };
            page(&format!("{} is installed in Slack", escape(&name)), next)
        }
        Err(e) => page("Not installed", &escape(&format!("{e:#}"))),
    }
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
    let _ = app.slack.events.set(events.clone());
    // Agents' own apps each keep a connection of their own.
    let ready: Vec<String> = app.slack.cfg.lock().unwrap().apps.iter().filter(|a| a.step() == "ready").map(|a| a.agent.clone()).collect();
    for agent in ready {
        start_app(&app, &agent);
    }
    // Apps whose agent changed while yardd was down, or made before their
    // name followed the agent's, are brought up to date.
    let all: Vec<String> = app.slack.cfg.lock().unwrap().apps.iter().map(|a| a.agent.clone()).collect();
    let syncing = app.clone();
    tokio::spawn(async move {
        for agent in all {
            let _ = sync_app(&syncing, &agent).await;
        }
    });
    tokio::spawn(check_public(app.clone()));
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
            r = connect(&app, &app_token, &bot_token, &events, None) => r,
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

/// One Socket Mode connection: the shared bot's (`agent` none) or an agent's own app's.
async fn connect(app: &Arc<App>, app_token: &str, bot_token: &str, events: &mpsc::UnboundedSender<(Me, Value)>, agent: Option<&str>) -> Result<Ended> {
    let slack = &app.slack;
    let who = slack.api(bot_token, "auth.test", &[]).await.map_err(|e| refused("The bot token was refused", "", e))?;
    let me = Me { team_id: who["team_id"].as_str().unwrap_or("").into(), bot_user: who["user_id"].as_str().unwrap_or("").into(), token: bot_token.to_string(), agent: agent.map(str::to_string) };
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
                        let status = Status { state: "connected".into(), team: team.clone(), bot: bot.clone(), error: None };
                        match agent {
                            Some(agent) => app_status(app, agent, status),
                            None => slack.set_status(app, |s| *s = status),
                        }
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

fn app_status(app: &App, agent: &str, status: Status) {
    app.slack.app_status.lock().unwrap().insert(agent.to_string(), status);
    broadcast(app);
}

/// An agent's app is ready (or its tokens changed): (re)dial its connection.
fn start_app(app: &Arc<App>, agent: &str) {
    let task = tokio::spawn(run_app(app.clone(), agent.to_string()));
    if let Some(old) = app.slack.app_tasks.lock().unwrap().insert(agent.to_string(), task.abort_handle()) {
        old.abort();
    }
}

fn stop_app(app: &App, agent: &str) {
    if let Some(task) = app.slack.app_tasks.lock().unwrap().remove(agent) {
        task.abort();
    }
    app.slack.app_status.lock().unwrap().remove(agent);
}

/// An agent's own app's connection, redialled like the shared bot's.
async fn run_app(app: Arc<App>, agent: String) {
    crypto();
    let mut backoff = Duration::from_secs(1);
    loop {
        // An app on HTTP has no connection: Slack calls `/slack/events`.
        let Some(rec) = app.slack.app_of(&agent).filter(|a| a.step() == "ready" && a.events_url.is_empty()) else { return };
        let Some(events) = app.slack.events.get().cloned() else { return };
        app_status(&app, &agent, Status { state: "connecting".into(), ..Default::default() });
        let wait = match connect(&app, &rec.app_token, &rec.bot_token, &events, Some(&agent)).await {
            Ok(Ended::Refresh) => Duration::ZERO,
            Ok(Ended::Closed { up }) => {
                if up {
                    backoff = Duration::from_secs(1);
                }
                backoff
            }
            Err(e) => {
                let error = format!("{e:#}");
                eprintln!("yardd: slack ({agent}): {error}");
                app_status(&app, &agent, Status { state: "error".into(), error: Some(error), ..Default::default() });
                backoff
            }
        };
        tokio::time::sleep(wait).await;
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

/// A token Slack turned down reads as such; anything else (no network, a
/// wrong `YARD_SLACK_API`) as Slack not being reached.
fn refused(what: &str, hint: &str, e: anyhow::Error) -> anyhow::Error {
    let text = format!("{e:#}");
    let auth = ["invalid_auth", "not_authed", "account_inactive", "token_revoked", "token_expired", "not_allowed_token_type", "missing_scope", "invalid_refresh_token", "invalid_code", "invalid_manifest", "invalid_app_id"];
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
    let token = me.token.clone();
    if token.is_empty() {
        return Ok(());
    }
    let place = slack.channel_name(&token, channel, im).await;
    // Channels are listed for the shared bot's default agents.
    if !im && me.agent.is_none() {
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
    // A hub's agents kept here to run are the hub's to reach.
    let all: Vec<crate::agents::Agent> = app.agents.all().into_iter().filter(|a| !a.hosted).collect();
    let agents: Vec<(String, String)> = all.iter().map(|a| (a.id.clone(), a.name.clone())).collect();
    // One still setting up has only a placeholder name and sets itself up
    // from its first message: that is the owner's to write, not a teammate's.
    let named: Vec<(String, String)> = all.iter().filter(|a| !a.setup).map(|a| (a.id.clone(), a.name.clone())).collect();
    let labels: Vec<(String, String)> = all.iter().filter(|a| !a.setup).flat_map(|a| [(a.id.clone(), crate::agents::mention(a).to_string()), (a.id.clone(), a.name.clone())]).collect();
    let name_of = |id: &str| agents.iter().find(|(a, _)| a == id).map(|(_, n)| n.clone());
    // An agent's own app is only ever that agent. Else a thread keeps its
    // agent; a deleted one lets the message route anew.
    let (agent, text) = match me.agent.clone().or(known).filter(|id| name_of(id).is_some()) {
        Some(id) => (Some(id), text),
        None => match route(&text, &labels) {
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
        let names = named.iter().map(|(_, n)| n.as_str()).collect::<Vec<_>>().join(", ");
        let note = if named.is_empty() {
            "No agent is set up yet. Make one in Switchyard, under Agents, then start your message with its name.".to_string()
        } else {
            format!("Which agent? Start your message with its name: {names}. Or pick a default agent in Switchyard, under Connectors, Slack.")
        };
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
    let mut origin = json!({
        "kind": "slack",
        "team": team,
        "channel": channel,
        "channelName": place,
        "threadTs": thread,
        "ts": ts,
        "user": user,
        "userName": user_name,
    });
    if let Some(agent) = &me.agent {
        // Answered through the same app.
        origin["app"] = json!(agent);
    }
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
    // A message is answered through the app it came in on: an agent's own,
    // or the shared bot.
    let token_for = |o: &Value| match o["app"].as_str().filter(|a| !a.is_empty()) {
        Some(agent) => slack.app_of(agent).map(|a| a.bot_token).unwrap_or_default(),
        None => slack.bot_token(),
    };
    // A remote agent's conversation is on its host: the note goes to the log.
    let note = |text: String| {
        if app.meta(&owed.sid).is_some() {
            app.emit(&owed.sid, json!({ "type": "note", "text": text }));
        } else {
            eprintln!("yardd: {agent}: {text}");
        }
    };
    let mentions_for = |o: &Value| match o["app"].as_str().filter(|a| !a.is_empty()) {
        Some(agent) => slack.app_of(agent).map(|a| a.mentions.into_iter().map(|p| p.id).collect()).unwrap_or_default(),
        None => Vec::new(),
    };
    let mut threads: Vec<(String, String, String, String, Vec<String>)> = Vec::new();
    for o in origins {
        let token = token_for(o);
        if token.is_empty() {
            if owed.status != "approval" {
                note("Slack is not connected, so this answer was not posted there.".into());
            }
            continue;
        }
        let t = (o["channel"].as_str().unwrap_or("").to_string(), o["threadTs"].as_str().unwrap_or("").to_string(), o["channelName"].as_str().unwrap_or("Slack").to_string(), token, mentions_for(o));
        if !t.0.is_empty() && !t.1.is_empty() && !threads.iter().any(|x| x.0 == t.0 && x.1 == t.1 && x.3 == t.3) {
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
                t => vec![t.to_string()],
            };
            (parts, Some("white_check_mark"))
        }
        "interrupted" => (vec![format!("{agent} was stopped in Switchyard.")], Some("x")),
        "unread" => (vec![format!("{agent} was stopped in Switchyard before it read your message.")], Some("x")),
        "failed" => (vec![why().map_or_else(|| format!("{agent} could not finish."), |w| format!("{agent} could not finish: {w}"))], Some("x")),
        _ => (vec![why().map_or_else(|| format!("Your message did not reach {agent}."), |w| format!("Your message did not reach {agent}: {w}"))], Some("x")),
    };
    let answered = owed.status == "completed" && !owed.text.trim().is_empty();
    for (channel, thread, place, token, mentions) in &threads {
        // Each thread's app has its own people to mention.
        let parts = if answered { split(&to_mrkdwn_with(&parts[0], mentions), PART) } else { parts.clone() };
        for part in &parts {
            if let Err(e) = slack.post(token, channel, thread, part).await {
                note(format!("Could not post to Slack ({place}): {e:#}"));
                break;
            }
        }
    }
    if let Some(mark) = mark {
        for o in origins {
            let (Some(channel), Some(ts)) = (o["channel"].as_str(), o["ts"].as_str()) else { continue };
            let token = token_for(o);
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
/// `Reviewer look`, `@Reviewer look`, `Reviewer: look`, `reviewer, look`,
/// `Custom Domains: look`. `agents` pairs an id with each label it answers
/// to (its handle and its name); the longest label that fits wins, so
/// `Deploy Bot …` is not `Deploy`'s.
pub fn route(text: &str, agents: &[(String, String)]) -> Option<(String, String)> {
    let t = text.trim_start();
    let t = t.strip_prefix('@').unwrap_or(t);
    let (id, rest) = agents
        .iter()
        .filter_map(|(id, label)| {
            let rest = after_label(t, label)?;
            (rest.is_empty() || rest.starts_with([':', ',']) || rest.starts_with(char::is_whitespace)).then(|| (label.chars().count(), id, rest))
        })
        .max_by_key(|(len, _, _)| *len)
        .map(|(_, id, rest)| (id, rest))?;
    let rest = rest.trim_start_matches([':', ',']).trim();
    Some((id.clone(), rest.to_string()))
}

/// What follows `label` at the start of `text`, compared ignoring case.
fn after_label<'a>(text: &'a str, label: &str) -> Option<&'a str> {
    if label.is_empty() {
        return None;
    }
    let mut chars = text.char_indices();
    for l in label.chars() {
        let (_, c) = chars.next()?;
        if !c.to_lowercase().eq(l.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or("", |(i, _)| &text[i..]))
}

/// Markdown as Slack mrkdwn: bold, italics, strikes, links, headings and
/// bullets converted; `&`, `<` and `>` escaped everywhere, so an answer can
/// never ping `<!channel>` or a user; code kept as code.
pub fn to_mrkdwn(md: &str) -> String {
    to_mrkdwn_with(md, &[])
}

/// `to_mrkdwn`, with `<@U…>` kept as a real mention for these users only.
pub fn to_mrkdwn_with(md: &str, mentions: &[String]) -> String {
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
            format!("*{}*", inline(h, mentions).replace('*', ""))
        } else if let Some(item) = ["- ", "* ", "+ "].iter().find_map(|b| body.strip_prefix(b)) {
            format!("{indent}• {}", inline(item, mentions))
        } else {
            format!("{indent}{}", inline(body, mentions))
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
fn inline(line: &str, mentions: &[String]) -> String {
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
        let mut s = links(&s.replace('\u{1}', "*"));
        for id in mentions {
            s = s.replace(&format!("&lt;@{id}&gt;"), &format!("<@{id}>"));
        }
        out.push_str(&s);
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
        vec![("g0".into(), "Scribe".into()), ("g1".into(), "Reviewer".into()), ("g2".into(), "pr-bot".into())]
    }

    #[test]
    fn a_message_goes_to_the_agent_it_names_first() {
        let a = agents();
        for text in ["Reviewer look at #12", "@Reviewer look at #12", "Reviewer: look at #12", "reviewer, look at #12", "  REVIEWER   look at #12"] {
            assert_eq!(route(text, &a), Some(("g1".into(), "look at #12".into())), "{text:?}");
        }
        assert_eq!(route("pr-bot: go", &a), Some(("g2".into(), "go".into())));
        assert_eq!(route("Scribe", &a), Some(("g0".into(), "".into())));
        // A name of a few words, by its handle or as it reads; the longest that fits.
        let a = [a, vec![("g3".into(), "Deploy".into()), ("g4".into(), "Deploy-Bot".into()), ("g4".into(), "Deploy Bot".into())]].concat();
        assert_eq!(route("@Deploy-Bot ship it", &a), Some(("g4".into(), "ship it".into())));
        assert_eq!(route("deploy bot: ship it", &a), Some(("g4".into(), "ship it".into())));
        assert_eq!(route("Deploy ship it", &a), Some(("g3".into(), "ship it".into())));
        assert_eq!(route("Deploy Bots ship", &a), Some(("g3".into(), "Bots ship".into())));
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
    fn only_the_owners_chosen_people_can_be_mentioned() {
        let ok = vec!["U1".to_string()];
        assert_eq!(to_mrkdwn_with("cc <@U1> and <@U2>", &ok), "cc <@U1> and &lt;@U2&gt;");
        assert_eq!(to_mrkdwn_with("<!channel> <!here> <@U1|x> <@U12>", &ok), "&lt;!channel&gt; &lt;!here&gt; &lt;@U1|x&gt; &lt;@U12&gt;");
        assert_eq!(to_mrkdwn_with("- **ask** <@U1>, see [PR](https://x.dev/1)", &ok), "• *ask* <@U1>, see <https://x.dev/1|PR>");
        assert_eq!(to_mrkdwn_with("code `<@U1>` stays", &ok), "code `&lt;@U1&gt;` stays");
        assert_eq!(to_mrkdwn("<@U1>"), "&lt;@U1&gt;");
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

    #[test]
    fn a_bot_handle_is_the_name_as_slack_would_make_it() {
        assert_eq!(bot_handle("Reviewer"), "reviewer");
        assert_eq!(bot_handle("Code Reviewer"), "code-reviewer");
        assert_eq!(bot_handle("  PR / Deploy — Watcher!! "), "pr-deploy-watcher");
        assert_eq!(bot_handle("José's helper"), "jose-s-helper");
        assert_eq!(bot_handle("Straße Ærø"), "strasse-aero");
        assert_eq!(bot_handle("ops.bot_2"), "ops.bot_2");
        assert_eq!(bot_handle("--Release---Manager--"), "release-manager");
        assert_eq!(bot_handle("Rocket 🚀 Launcher"), "rocket-launcher");
        // Cut to Slack's 21, with no dash left at the end.
        assert_eq!(bot_handle("Pull request reviewer"), "pull-request-reviewer");
        assert_eq!(bot_handle("Pull request reviewer bot"), "pull-request-reviewer");
        assert_eq!(bot_handle("The quick brown fox jumps"), "the-quick-brown-fox-j");
        assert_eq!(bot_handle("abcdefghijklmnopqrst uv"), "abcdefghijklmnopqrst");
        // Nothing Slack keeps: a handle all the same.
        assert_eq!(bot_handle("日本語"), "agent");
        assert_eq!(bot_handle("!!!"), "agent");
        let long = "x".repeat(40);
        for name in ["Code Reviewer", "José", "a b c", "日本 bot", long.as_str()] {
            let h = bot_handle(name);
            assert!(h.len() <= 21 && h.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "-_.".contains(c)), "{h}");
        }
    }

    #[test]
    fn the_app_name_keeps_the_readable_name_within_35_characters() {
        assert_eq!(app_name("Code Reviewer"), "Code Reviewer");
        assert_eq!(app_name("  José   the   helper "), "José the helper");
        assert_eq!(app_name(&"é".repeat(50)).chars().count(), 35);
        assert_eq!(app_name(""), "Agent");
        let m = agent_manifest("Code Reviewer", "", &[], "");
        assert_eq!(m["display_information"]["name"], "Code Reviewer");
        assert_eq!(m["display_information"]["description"], "Code Reviewer, an agent in Switchyard.");
        assert_eq!(m["features"]["bot_user"]["display_name"], "code-reviewer");
        assert_eq!(m["settings"]["socket_mode_enabled"], true);
        assert!(m["settings"]["event_subscriptions"]["request_url"].is_null() && m["oauth_config"]["redirect_urls"].is_null());
        let m = agent_manifest("Reviewer", &"x".repeat(200), &["https://yard.test/slack/oauth".into()], "https://pub.test/slack/events");
        assert_eq!(m["settings"]["socket_mode_enabled"], false);
        assert_eq!(m["settings"]["event_subscriptions"]["request_url"], "https://pub.test/slack/events");
        assert_eq!(m["oauth_config"]["redirect_urls"], json!(["https://yard.test/slack/oauth"]));
        assert_eq!(m["display_information"]["description"].as_str().unwrap().len(), 139);
    }

    #[test]
    fn slack_signatures_are_checked_with_the_secret_within_five_minutes() {
        // Slack's documented example: secret, timestamp, body and signature.
        let body = b"token=xyzz0WbapA4vBCDEFasx0q6G&team_id=T1DC2JH3J&team_domain=testteamnow&channel_id=G8PSS9T3V&channel_name=foobar&user_id=U2CERLKJA&user_name=roadrunner&command=%2Fwebhook-collect&text=&response_url=https%3A%2F%2Fhooks.slack.com%2Fcommands%2FT1DC2JH3J%2F397700885554%2F96rGlfmibIGlgcZRskXaIFfN&trigger_id=398738663015.47445629121.803a0bc887a14d10d2c447fce8b6703c";
        let sig = "v0=a2114d57b48eac39b9ad189dd8316235a7b4a8d21a10bd27519666489c69b503";
        assert!(signed("8f742231b10e8888abcd99yyyzzz85a5", "1531420618", body, sig));
        assert!(!signed("8f742231b10e8888abcd99yyyzzz85a5", "1531420619", body, sig));
        assert!(!signed("another-secret", "1531420618", body, sig));
        assert!(!signed("8f742231b10e8888abcd99yyyzzz85a5", "1531420618", &body[1..], sig));
        assert!(!signed("", "1", body, "v0=00"));
        assert!(!signed("s", "1", body, "a2114d57"));
        assert!(!signed("s", "1", body, "v0=zz"));
        assert!(fresh("1000", 1000 + 300) && fresh("1000", 1000 - 300));
        assert!(!fresh("1000", 1000 + 301) && !fresh("", 1000) && !fresh("12a", 1000));
    }

    #[test]
    fn the_install_comes_back_to_a_listed_https_address_only() {
        let listed = vec!["https://yard.test/slack/oauth".to_string(), "https://pub.test/slack/oauth".to_string()];
        // The asking window's own address when listed; else the first.
        assert_eq!(redirect_for("https://pub.test", &listed), "https://pub.test/slack/oauth");
        assert_eq!(redirect_for("http://127.0.0.1:47717", &listed), "https://yard.test/slack/oauth");
        assert_eq!(redirect_for("https://evil.test", &listed), "https://yard.test/slack/oauth");
        assert_eq!(redirect_for("https://yard.test", &[]), "");
        assert_eq!(public_address(" https://slack.example.com/ ").unwrap(), "https://slack.example.com");
        assert_eq!(public_address("https://example.com/yard").unwrap(), "https://example.com/yard");
        assert_eq!(public_address("").unwrap(), "");
        for bad in ["http://example.com", "https://", "https://a b", "https://x.test/?q=1", "https://u@x.test", "example.com"] {
            assert!(public_address(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn an_icon_is_a_png_slack_takes() {
        let png = |w: u32, h: u32| {
            let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
            b.extend_from_slice(&w.to_be_bytes());
            b.extend_from_slice(&h.to_be_bytes());
            b.extend_from_slice(&[8, 6, 0, 0, 0]);
            b
        };
        assert!(icon_ok(&png(1024, 1024)).is_ok());
        assert!(icon_ok(&png(512, 2000)).is_ok());
        assert!(icon_ok(&png(256, 256)).unwrap_err().to_string().contains("512 to 2000"));
        assert!(icon_ok(&png(2048, 2048)).is_err());
        assert!(icon_ok(b"<svg></svg> and more bytes to pass the length").unwrap_err().to_string().contains("not a PNG"));
    }
}
