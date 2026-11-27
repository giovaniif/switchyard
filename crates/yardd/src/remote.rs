//! Remote hosts: other yardds whose sessions show up here as if they were local.
//!
//! This yardd is the hub. For each UI client it opens one connection to every
//! remote yardd and pipes messages both ways, so replies meant for one client
//! (`history`, `changes`, `created`, ...) reach the client that asked without
//! any routing table. Every session and terminal id that crosses the hub
//! carries the host id: `<hostId>~<remoteId>`.
//!
//! An `ssh` host is reached through `ssh -N -L`, one tunnel per host shared by
//! every client. A `direct` host is a plain `ws://` URL, for testing only. The
//! remote's origin check lets us in because a non-browser client sends no
//! `Origin`.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_tungstenite::tungstenite::Message;

use crate::state::App;

/// Between the host id and the remote's own id. Local ids never contain it.
pub const SEP: char = '~';
const DEFAULT_REMOTE_PORT: u16 = 47717;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConfig {
    pub id: String,
    pub name: String,
    /// `ssh` or `direct`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, PartialEq)]
pub struct Conn {
    /// The remote's WebSocket; `None` while an ssh tunnel is down.
    pub url: Option<String>,
    /// `connecting`, `connected` or `error`.
    pub status: &'static str,
    pub error: Option<String>,
    /// The way there works (an ssh tunnel is up) but no yardd answered at the
    /// end of it: the host needs Switchyard installed or started.
    pub no_daemon: bool,
}

struct Entry {
    config: HostConfig,
    conn: Arc<watch::Sender<Conn>>,
    tasks: Vec<JoinHandle<()>>,
    /// The running `ssh` process, so a stop can kill it: `process::exit`
    /// skips the drops that would.
    ssh_pid: Arc<Mutex<Option<u32>>>,
    /// The host's last snapshot, kept current by its session updates, so a
    /// page opened while the host is down still lists its sessions (offline).
    last: Arc<Mutex<Option<Value>>>,
    /// The host's own agents and Slack apps, as its broadcasts last said,
    /// kept while it is down so they stay listed (offline).
    agents: Arc<Mutex<HostAgents>>,
}

/// What a host says about the agents it owns, as the hub's own connection
/// to it hears (`monitor`). Ids are the host's own, without the prefix.
#[derive(Clone, Default)]
pub struct HostAgents {
    /// Whether the host owns agents itself (its snapshot says `ownAgents`):
    /// `None` until it has said hello once.
    pub own: Option<bool>,
    /// Its `agents` list (its own agents only).
    pub list: Vec<Value>,
    /// Its `slack` view (for its agents' own apps and its workspaces).
    pub slack: Value,
}

/// A host's agents as the hub knows them, for listing and routing.
pub struct HostAgentsView {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub agents: HostAgents,
}

impl Drop for Entry {
    fn drop(&mut self) {
        // Aborting the tunnel task drops its `ssh` child, which kills it.
        for task in &self.tasks {
            task.abort();
        }
    }
}

pub struct Remotes {
    file: PathBuf,
    entries: Mutex<Vec<Entry>>,
    /// Host ids, so each client can open and close its pipes as hosts come and go.
    ids: watch::Sender<Vec<String>>,
    /// Hosts with a `host.install` running, so a second click does not start another.
    installing: Mutex<std::collections::HashSet<String>>,
}

impl Remotes {
    pub fn new(dir: PathBuf) -> Self {
        Remotes {
            file: dir.join("hosts.json"),
            entries: Mutex::new(Vec::new()),
            ids: watch::Sender::new(Vec::new()),
            installing: Mutex::new(Default::default()),
        }
    }

    pub fn ids(&self) -> watch::Receiver<Vec<String>> {
        self.ids.subscribe()
    }

    /// Every remote host with its connection state, for the UI.
    pub fn list(&self) -> Value {
        let entries = self.entries.lock().unwrap();
        let hosts: Vec<Value> = entries
            .iter()
            .map(|e| {
                let conn = e.conn.borrow().clone();
                let mut v = serde_json::to_value(&e.config).unwrap_or_else(|_| json!({}));
                v["status"] = json!(conn.status);
                if let Some(err) = conn.error {
                    v["error"] = json!(err);
                }
                if conn.no_daemon {
                    v["noDaemon"] = json!(true);
                }
                if let Some(own) = e.agents.lock().unwrap().own {
                    // Whether its agents live there, or it needs Update Switchyard first.
                    v["ownAgents"] = json!(own);
                }
                v
            })
            .collect();
        Value::Array(hosts)
    }

    fn lookup(&self, id: &str) -> Option<(watch::Receiver<Conn>, String, Arc<Mutex<Option<Value>>>)> {
        let entries = self.entries.lock().unwrap();
        let e = entries.iter().find(|e| e.config.id == id)?;
        Some((e.conn.subscribe(), e.config.name.clone(), e.last.clone()))
    }

    /// Every host's agents, in the order the hosts were added.
    pub fn host_agents(&self) -> Vec<HostAgentsView> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .map(|e| HostAgentsView {
                id: e.config.id.clone(),
                name: e.config.name.clone(),
                connected: e.conn.borrow().status == "connected",
                agents: e.agents.lock().unwrap().clone(),
            })
            .collect()
    }

    /// One host's agents, if it is a host.
    pub fn agents_of(&self, host: &str) -> Option<HostAgentsView> {
        self.host_agents().into_iter().find(|h| h.id == host)
    }

    fn publish_ids(&self, entries: &[Entry]) {
        self.ids.send_replace(entries.iter().map(|e| e.config.id.clone()).collect());
    }

    fn save(&self, entries: &[Entry]) -> Result<()> {
        let configs: Vec<&HostConfig> = entries.iter().map(|e| &e.config).collect();
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&configs)?)
            .with_context(|| format!("could not write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.file).with_context(|| format!("could not write {}", self.file.display()))?;
        Ok(())
    }

    /// Kill every ssh tunnel. For the signal handler, which exits without drops.
    pub fn shutdown(&self) {
        let entries = self.entries.lock().unwrap();
        for e in entries.iter() {
            if let Some(pid) = *e.ssh_pid.lock().unwrap() {
                let _ = std::process::Command::new("kill").arg(pid.to_string()).status();
            }
        }
    }
}

fn broadcast_hosts(app: &App) {
    app.broadcast(json!({ "op": "hosts", "hosts": app.remotes.list() }));
}

/// Change a host's connection state and tell every client when it moved.
fn set(app: &App, conn: &watch::Sender<Conn>, f: impl FnOnce(&mut Conn)) {
    let changed = conn.send_if_modified(|c| {
        let before = c.clone();
        f(c);
        *c != before
    });
    if changed {
        broadcast_hosts(app);
    }
}

/// Read the saved hosts and start connecting to each.
pub fn start(app: &Arc<App>) {
    let configs: Vec<HostConfig> = match std::fs::read_to_string(&app.remotes.file) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("yardd: ignoring unreadable {}: {e}", app.remotes.file.display());
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    };
    let mut entries = app.remotes.entries.lock().unwrap();
    for config in configs {
        entries.push(spawn_host(app, config));
    }
    app.remotes.publish_ids(&entries);
}

fn spawn_host(app: &Arc<App>, config: HostConfig) -> Entry {
    let conn = Arc::new(watch::Sender::new(Conn {
        url: config.url.clone().filter(|_| config.kind == "direct"),
        status: "connecting",
        error: None,
        no_daemon: false,
    }));
    let ssh_pid = Arc::new(Mutex::new(None));
    let mut tasks = Vec::new();
    if config.kind == "ssh" {
        let destination = config.destination.clone().unwrap_or_default();
        let port = config.remote_port.unwrap_or(DEFAULT_REMOTE_PORT);
        tasks.push(tokio::spawn(tunnel(app.clone(), conn.clone(), destination, port, ssh_pid.clone())));
    }
    let agents: Arc<Mutex<HostAgents>> = Default::default();
    tasks.push(tokio::spawn(monitor(app.clone(), conn.clone(), config.id.clone(), agents.clone())));
    Entry { config, conn, tasks, ssh_pid, last: Default::default(), agents }
}

/// `host.add`: check the fields, save, and start connecting.
pub fn add(app: &Arc<App>, msg: &Value) -> Result<()> {
    let name = msg["name"].as_str().unwrap_or("").trim();
    if name.is_empty() {
        bail!("the host needs a name");
    }
    if name.chars().count() > 40 || name.chars().any(char::is_control) {
        bail!("the host name must be at most 40 plain characters");
    }
    let id = app.id("h");
    let config = match msg["kind"].as_str().unwrap_or("") {
        "ssh" => {
            let destination = msg["destination"].as_str().unwrap_or("").trim();
            check_destination(destination)?;
            let port = match &msg["remotePort"] {
                Value::Null => DEFAULT_REMOTE_PORT,
                Value::String(s) if s.trim().is_empty() => DEFAULT_REMOTE_PORT,
                v => v
                    .as_u64()
                    .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                    .and_then(|n| u16::try_from(n).ok())
                    .filter(|p| *p > 0)
                    .context("the remote port must be a number from 1 to 65535")?,
            };
            HostConfig {
                id,
                name: name.to_string(),
                kind: "ssh".into(),
                destination: Some(destination.to_string()),
                remote_port: Some(port),
                url: None,
            }
        }
        "direct" => {
            let url = msg["url"].as_str().unwrap_or("").trim();
            if !url.starts_with("ws://") || url.len() <= 5 || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
                bail!("a direct host needs a ws:// URL");
            }
            HostConfig {
                id,
                name: name.to_string(),
                kind: "direct".into(),
                destination: None,
                remote_port: None,
                url: Some(url.to_string()),
            }
        }
        other => bail!("unknown host kind `{other}`"),
    };
    {
        let mut entries = app.remotes.entries.lock().unwrap();
        if name.eq_ignore_ascii_case(crate::hosts::local_name()) || entries.iter().any(|e| e.config.name.eq_ignore_ascii_case(name)) {
            bail!("a host named {name} already exists");
        }
        entries.push(spawn_host(app, config));
        app.remotes.publish_ids(&entries);
        if let Err(e) = app.remotes.save(&entries) {
            eprintln!("yardd: {e:#}");
        }
    }
    broadcast_hosts(app);
    Ok(())
}

/// `host.remove`: stop its tunnel and forget it. Sessions on it are untouched.
pub fn remove(app: &Arc<App>, host_id: &str) -> Result<()> {
    let removed = {
        let mut entries = app.remotes.entries.lock().unwrap();
        let i = entries.iter().position(|e| e.config.id == host_id).context("no such host")?;
        let removed = entries.remove(i);
        app.remotes.publish_ids(&entries);
        if let Err(e) = app.remotes.save(&entries) {
            eprintln!("yardd: {e:#}");
        }
        removed
    };
    drop(removed);
    broadcast_hosts(app);
    // Its agents are no longer listed here.
    crate::agents::broadcast(app);
    crate::slack::broadcast(app);
    Ok(())
}

/// The installer the app runs on a host that has SSH but no yardd yet.
const REMOTE_INSTALL: &str = include_str!("../../../scripts/remote-install.sh");

/// A `RemoteCommand` (a tmux attach, say) or `RequestTTY` in the user's ssh
/// config is for their own logins. ssh refuses a command line next to the
/// first ("Cannot execute command-line and remote command"), and a forced tty
/// would mangle the script piped in, so yardd's own connections turn both off.
const OVERRIDE_SESSION: [&str; 4] = ["-o", "RemoteCommand=none", "-o", "RequestTTY=no"];

/// `ssh`, or `YARD_SSH` so the tests can stand in a fake one.
fn ssh_program() -> String {
    std::env::var("YARD_SSH").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "ssh".to_string())
}

/// `host.install`: install or update yardd on an SSH host from the release,
/// over its own ssh connection. Every output line goes to all clients as
/// `host.install { hostId, line }`, then one `{ hostId, done, ok }`.
pub fn install(app: &Arc<App>, host_id: &str) -> Result<()> {
    let (destination, port) = {
        let entries = app.remotes.entries.lock().unwrap();
        let e = entries.iter().find(|e| e.config.id == host_id).context("no such host")?;
        if e.config.kind != "ssh" {
            bail!("only an SSH host can be set up from here");
        }
        let dest = e.config.destination.clone().unwrap_or_default();
        (dest, e.config.remote_port.unwrap_or(DEFAULT_REMOTE_PORT))
    };
    check_destination(&destination)?;
    if !app.remotes.installing.lock().unwrap().insert(host_id.to_string()) {
        bail!("Switchyard is already being installed on that host");
    }
    // The remote command is one string for the remote shell: keep it to
    // characters that need no quoting.
    let mut remote = format!("bash -s -- {port}");
    if let Ok(url) = std::env::var("YARD_RELEASE_URL") {
        if !url.chars().all(|c| c.is_ascii_alphanumeric() || "/:._-~".contains(c)) {
            app.remotes.installing.lock().unwrap().remove(host_id);
            bail!("YARD_RELEASE_URL has characters that would need quoting");
        }
        remote = format!("SWITCHYARD_RELEASE_URL={url} {remote}");
    }
    let spawned = Command::new(ssh_program())
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"])
        .args(OVERRIDE_SESSION)
        .args(["--", &destination, &remote])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => {
            app.remotes.installing.lock().unwrap().remove(host_id);
            bail!("could not run ssh: {e}");
        }
    };
    let app = app.clone();
    let host = host_id.to_string();
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(REMOTE_INSTALL.as_bytes()).await;
            // Closing stdin ends the script for `bash -s`.
        }
        let lines = |stream: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>| {
            let app = app.clone();
            let host = host.clone();
            tokio::spawn(async move {
                let Some(stream) = stream else { return };
                let mut lines = BufReader::new(stream).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let line: String = line.chars().take(500).collect();
                    app.broadcast(json!({ "op": "host.install", "hostId": host, "line": line }));
                }
            })
        };
        let out = lines(child.stdout.take().map(|s| Box::new(s) as _));
        let err = lines(child.stderr.take().map(|s| Box::new(s) as _));
        // Downloading and starting take seconds; a hung ssh should not
        // leave the card spinning forever.
        let status = tokio::time::timeout(Duration::from_secs(300), child.wait()).await;
        let _ = tokio::join!(out, err);
        let ok = matches!(status, Ok(Ok(s)) if s.success());
        if status.is_err() {
            app.broadcast(json!({ "op": "host.install", "hostId": host, "line": "Gave up after 5 minutes." }));
        }
        app.remotes.installing.lock().unwrap().remove(&host);
        app.broadcast(json!({ "op": "host.install", "hostId": host, "done": true, "ok": ok }));
    });
    Ok(())
}

/// The destination goes to ssh as one argument after `--`, and a leading `-`
/// is refused too, so it can never be read as an option such as `-oProxyCommand`.
fn check_destination(dest: &str) -> Result<()> {
    if dest.is_empty() {
        bail!("an SSH host needs a destination, such as devbox or user@100.x.y.z");
    }
    if dest.starts_with('-') {
        bail!("the SSH destination cannot start with `-`");
    }
    if dest.len() > 255 || dest.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("the SSH destination cannot contain spaces");
    }
    Ok(())
}

struct Backoff {
    next: Duration,
    max: Duration,
}

impl Backoff {
    fn new(max: Duration) -> Self {
        Backoff { next: Duration::from_secs(1), max }
    }
    fn reset(&mut self) {
        self.next = Duration::from_secs(1);
    }
    async fn wait(&mut self) {
        tokio::time::sleep(self.next).await;
        self.next = (self.next * 2).min(self.max);
    }
}

async fn connect(
    url: &str,
) -> Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>> {
    // The same limits as yardd's own server, so an upload that reached the
    // hub also fits on the way to the host, and its reply on the way back.
    let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
    config.max_message_size = Some(crate::attachments::WS_MAX_MESSAGE);
    config.max_frame_size = Some(crate::attachments::WS_MAX_MESSAGE);
    let connecting = tokio_tungstenite::connect_async_with_config(url, Some(config), false);
    let (ws, _) = tokio::time::timeout(CONNECT_TIMEOUT, connecting).await.context("timed out")??;
    Ok(ws)
}

/// The hub's own connection to a host. It decides the status the UI shows
/// (clients get their own connections in `pipe`), and it is the way back
/// for this hub's agents that run there: it says which hub it is, and the
/// host passes their tool calls and owed answers to it (`agent_tools`).
async fn monitor(app: Arc<App>, conn: Arc<watch::Sender<Conn>>, host: String, agents: Arc<Mutex<HostAgents>>) {
    let mut rx = conn.subscribe();
    let mut backoff = Backoff::new(Duration::from_secs(10));
    loop {
        let url = match rx.wait_for(|c| c.url.is_some()).await {
            Ok(c) => c.url.clone().unwrap_or_default(),
            Err(_) => return,
        };
        match connect(&url).await {
            Ok(ws) => {
                // Shown connected once its snapshot is read (`serve_host`).
                backoff.reset();
                serve_host(&app, ws, &host, &conn, &agents).await;
                set(&app, &conn, |c| {
                    c.status = "error";
                    c.error = Some("Connection lost. Retrying.".into());
                });
            }
            // The top error only: tungstenite repeats its source in its own text.
            Err(e) => set(&app, &conn, |c| {
                c.status = "error";
                c.error = Some(format!("{e}"));
                c.no_daemon = true;
            }),
        }
        backoff.wait().await;
    }
}

/// Until the connection drops: say which hub this is, keep the host's own
/// agents and Slack apps as it broadcasts them (the hub lists them with its
/// own), hand the host the agents this hub still keeps for it (`agents::
/// migrate`), and answer what the host passes back for this hub's agents. A
/// host from before remote agents answers the hello with an error, which is
/// ignored like everything else.
async fn serve_host(
    app: &Arc<App>,
    ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    host: &str,
    conn: &watch::Sender<Conn>,
    agents: &Arc<Mutex<HostAgents>>,
) {
    let (mut sink, mut stream) = ws.split();
    let hello = json!({ "op": "agent.hosted.hub", "hub": app.tools.hub_id() });
    if sink.send(Message::Text(hello.to_string())).await.is_err() {
        return;
    }
    let (answers, mut answers_rx) = mpsc::unbounded_channel::<Value>();
    loop {
        tokio::select! {
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if text.contains("\"op\":\"snapshot\"") || text.contains("\"op\":\"agents\"") || text.contains("\"op\":\"slack\"") {
                        if let Ok(msg) = serde_json::from_str::<Value>(&text) {
                            if msg["op"] == "snapshot" {
                                // Connected from here on: the hub now knows
                                // what the host is (whether it owns agents),
                                // so nothing acting on a connected host meets
                                // one it has not heard yet. `hear_agents` tells
                                // the windows (and starts the hand-overs).
                                conn.send_if_modified(|c| {
                                    let before = c.clone();
                                    c.status = "connected";
                                    c.error = None;
                                    c.no_daemon = false;
                                    *c != before
                                });
                            }
                            hear_agents(app, host, agents, &msg);
                        }
                        continue;
                    }
                    // Everything the host broadcasts comes here too: parse only what is ours.
                    if !text.contains("\"agent.hub.") {
                        continue;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(&text) else { continue };
                    if !msg["op"].as_str().is_some_and(|op| op.starts_with("agent.hub.")) {
                        continue;
                    }
                    let (app, host, answers) = (app.clone(), host.to_string(), answers.clone());
                    tokio::spawn(async move {
                        if let Some(answer) = crate::agent_tools::from_host(&app, &host, &msg).await {
                            let _ = answers.send(answer);
                        }
                    });
                }
                Some(Ok(_)) => {}
                _ => return,
            },
            Some(answer) = answers_rx.recv() => {
                if sink.send(Message::Text(answer.to_string())).await.is_err() {
                    return;
                }
            }
        }
    }
}

/// What the hub's own connection hears about the host's agents: its
/// snapshot (with whether it owns agents at all), then each `agents` and
/// `slack` broadcast. Entries that are the host's own remotes' (an id with
/// the separator) are not the host's to show. The hub's windows get the
/// merged lists again; a host that owns agents gets the ones this hub still
/// keeps for it (`agents::migrate`).
fn hear_agents(app: &Arc<App>, host: &str, agents: &Mutex<HostAgents>, msg: &Value) {
    let own_list = |v: &Value| -> Vec<Value> { v.as_array().map(|l| l.iter().filter(|a| a["id"].as_str().is_some_and(|id| !id.contains(SEP))).cloned().collect()).unwrap_or_default() };
    let mut migrate = false;
    let (agents_changed, slack_changed) = {
        let mut cache = agents.lock().unwrap();
        match msg["op"].as_str() {
            Some("snapshot") => {
                let own = msg["ownAgents"] == true;
                migrate = own;
                cache.own = Some(own);
                cache.list = own_list(&msg["agents"]);
                cache.slack = msg["slack"].clone();
                (true, true)
            }
            Some("agents") if msg["ref"].is_null() => {
                let list = own_list(&msg["agents"]);
                let changed = list != cache.list;
                cache.list = list;
                (changed, false)
            }
            Some("slack") if msg["ref"].is_null() => {
                let changed = msg["slack"] != cache.slack;
                cache.slack = msg["slack"].clone();
                (false, changed)
            }
            _ => (false, false),
        }
    };
    if agents_changed {
        crate::agents::broadcast(app);
        // An agent it took, or deleted, may finish a hand-over kept here.
        if msg["op"] == "agents" {
            let (app, host) = (app.clone(), host.to_string());
            tokio::spawn(async move { crate::agents::migrate(&app, &host, false).await });
        }
    }
    if slack_changed {
        crate::slack::broadcast(app);
    }
    if msg["op"] == "snapshot" {
        broadcast_hosts(app);
    }
    if migrate {
        let (app, host) = (app.clone(), host.to_string());
        tokio::spawn(async move { crate::agents::migrate(&app, &host, true).await });
    }
}

/// Keep one `ssh -N -L` running for a host, restarting it with backoff.
async fn tunnel(app: Arc<App>, conn: Arc<watch::Sender<Conn>>, destination: String, remote_port: u16, pid: Arc<Mutex<Option<u32>>>) {
    let mut backoff = Backoff::new(Duration::from_secs(30));
    loop {
        set(&app, &conn, |c| {
            c.url = None;
            c.status = "connecting";
        });
        let started = Instant::now();
        let error = run_ssh(&app, &conn, &destination, remote_port, &pid).await;
        *pid.lock().unwrap() = None;
        set(&app, &conn, |c| {
            c.url = None;
            c.status = "error";
            c.error = Some(error);
            c.no_daemon = false;
        });
        if started.elapsed() > Duration::from_secs(60) {
            backoff.reset();
        }
        backoff.wait().await;
    }
}

/// A free loopback port, outside LocalStack's range and yardd's default.
fn free_port() -> Result<u16> {
    for _ in 0..20 {
        let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        if !(4510..=4559).contains(&port) && port != DEFAULT_REMOTE_PORT {
            return Ok(port);
        }
    }
    bail!("no free local port")
}

/// Run one ssh tunnel until it dies. Returns what to show as the error.
async fn run_ssh(app: &App, conn: &watch::Sender<Conn>, destination: &str, remote_port: u16, pid: &Mutex<Option<u32>>) -> String {
    let port = match free_port() {
        Ok(p) => p,
        Err(e) => return format!("{e:#}"),
    };
    let forward = format!("127.0.0.1:{port}:127.0.0.1:{remote_port}");
    let spawned = Command::new(ssh_program())
        .args([
            "-N",
            "-o",
            "BatchMode=yes",
            "-o",
            "ExitOnForwardFailure=yes",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ConnectTimeout=10",
            "-L",
            &forward,
        ])
        .args(OVERRIDE_SESSION)
        .args(["--", destination])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(c) => c,
        Err(e) => return format!("could not run ssh: {e}"),
    };
    *pid.lock().unwrap() = child.id();

    // Keep the last thing ssh said; that is the error to show.
    let last = Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let last = last.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line = line.trim();
                if !line.is_empty() {
                    *last.lock().unwrap() = line.chars().take(300).collect();
                }
            }
        });
    }
    let exit_message = |status: std::io::Result<std::process::ExitStatus>, last: &Mutex<String>| {
        let line = last.lock().unwrap().clone();
        if !line.is_empty() {
            return line;
        }
        match status {
            Ok(s) => format!("ssh exited ({s})"),
            Err(e) => format!("ssh failed: {e}"),
        }
    };

    // The forward is up once its local port accepts a connection.
    loop {
        tokio::select! {
            status = child.wait() => {
                // Let the stderr reader catch the final line.
                tokio::time::sleep(Duration::from_millis(100)).await;
                return exit_message(status, &last);
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                if tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
                    break;
                }
            }
        }
    }
    set(app, conn, |c| c.url = Some(format!("ws://127.0.0.1:{port}/ws")));
    let status = child.wait().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    exit_message(status, &last)
}

/// One client's connection to one host.
pub struct Pipe {
    tx: mpsc::UnboundedSender<Value>,
    up: Arc<AtomicBool>,
    /// The hub's own connection to the host (`monitor`), which the host's
    /// row shows.
    conn: watch::Receiver<Conn>,
    task: JoinHandle<()>,
}

impl Drop for Pipe {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Pipe {
    /// Forward an op to the host, or say why it cannot go. While the host
    /// shows connected, an op waits for this client's own connection to
    /// open: a page asks right after it connects (a reload, a phone coming
    /// back), and after the host comes back this connection may still be
    /// waiting to try again. If it cannot open, the op fails then (`pipe`).
    pub fn send(&self, msg: Value) -> Result<()> {
        if !self.up.load(Ordering::Relaxed) && self.conn.borrow().status != "connected" {
            bail!("{NOT_CONNECTED}");
        }
        self.tx.send(msg).ok().context(NOT_CONNECTED)
    }
}

const NOT_CONNECTED: &str = "this host is not connected";

/// Open (and keep reopening) a client's connection to a host. Everything the
/// host sends goes to `direct`, with ids rewritten.
pub fn pipe(app: &Arc<App>, host_id: &str, direct: mpsc::UnboundedSender<String>) -> Option<Pipe> {
    let (conn, name, last) = app.remotes.lookup(host_id)?;
    if conn.borrow().status != "connected" {
        if let Some(mut snapshot) = last.lock().unwrap().clone() {
            // Its stats are stale; the host's row says it is offline instead.
            snapshot["host"] = Value::Null;
            let _ = direct.send(snapshot.to_string());
            let _ = direct.send(json!({ "op": "host.down", "hostId": host_id }).to_string());
        }
    }
    Some(open(conn, host_id.to_string(), name, last, direct))
}

/// The connection itself, given the hub's own connection to the host
/// (`conn`), which the tests stand in for.
fn open(mut conn: watch::Receiver<Conn>, host: String, name: String, last: Arc<Mutex<Option<Value>>>, direct: mpsc::UnboundedSender<String>) -> Pipe {
    let (tx, rx) = mpsc::unbounded_channel::<Value>();
    let up = Arc::new(AtomicBool::new(false));
    let flag = up.clone();
    let shown = conn.clone();
    // Built outside the task, so a pipe dropped before its task first runs
    // (or while it writes) still answers what it was sent.
    let mut pending = Pending { held: VecDeque::new(), rx, direct: direct.clone(), host: host.clone(), name: name.clone(), sent: HashMap::new(), writing: None, refs: 0, greeted: false };
    let task = tokio::spawn(async move {
        let mut backoff = Backoff::new(Duration::from_secs(10));
        loop {
            let Some(url) = url_for(&mut conn, &mut pending).await else { return };
            if let Ok(ws) = connect(&url).await {
                backoff.reset();
                let (mut sink, mut stream) = ws.split();
                flag.store(true, Ordering::Relaxed);
                pending.greeted = false;
                // The op whose write failed, answered once the page knows
                // the host is down (below).
                let mut unwritten = None;
                loop {
                    let op = match pending.held.pop_front() {
                        Some(op) => op,
                        None => tokio::select! {
                            incoming = stream.next() => match incoming {
                                Some(Ok(Message::Text(text))) => {
                                    let Ok(mut msg) = serde_json::from_str::<Value>(&text) else { continue };
                                    pending.heard(&mut msg);
                                    if let Some(out) = rewrite(&host, &name, msg) {
                                        remember(&last, &out);
                                        if direct.send(out.to_string()).is_err() { return; }
                                    }
                                    continue;
                                }
                                Some(Ok(_)) => continue,
                                _ => break,
                            },
                            op = pending.rx.recv() => match op {
                                Some(op) => op,
                                None => return,
                            },
                        },
                    };
                    // Made now: the op goes out with the host's own ids, and
                    // its error must carry the page's.
                    let refused = refusal(&host, &op, NOT_CONNECTED);
                    let awaited = pending.awaited(&op);
                    let mut out = outgoing(op);
                    if let Some((r, _)) = &awaited {
                        out["ref"] = r.clone();
                    }
                    // Held where a pipe dropped mid-write still answers it.
                    pending.writing = awaited;
                    if sink.send(Message::Text(out.to_string())).await.is_err() {
                        pending.writing = None;
                        unwritten = refused;
                        break;
                    }
                    pending.wrote();
                }
                flag.store(false, Ordering::Relaxed);
                // `host.down` first: a page shows an error where the host's
                // state says it can be seen (a connector's card is hidden
                // while its host is down).
                let _ = direct.send(json!({ "op": "host.down", "hostId": host }).to_string());
                if let Some(refused) = unwritten {
                    let _ = direct.send(refused);
                }
                pending.lose_sent();
            }
            // Ops that waited for this try and still could not go: say so.
            pending.fail(NOT_CONNECTED);
            // An op sent meanwhile means the host shows connected (`Pipe::
            // send`): try again now rather than after the backoff.
            tokio::select! {
                _ = backoff.wait() => {}
                op = pending.rx.recv() => match op {
                    Some(op) => pending.held.push_back(op),
                    None => return,
                },
            }
        }
    });
    Pipe { tx, up, conn: shown, task }
}

/// Ops a pipe took and the page has no answer for yet: not written to the
/// host, or written and waiting on its answer. Each one gets an error if
/// it cannot go or its answer is lost, so a page never waits on a `ref`
/// with no answer.
struct Pending {
    /// Ops sent while it waited to try again: they go first once it opens.
    held: VecDeque<Value>,
    rx: mpsc::UnboundedReceiver<Value>,
    direct: mpsc::UnboundedSender<String>,
    /// The host's id, which every error made here carries (`error_for`).
    host: String,
    /// The host's name, for `lost`.
    name: String,
    /// Ops written on this connection that wait on the host's answer, by the
    /// `ref` it will carry (as JSON text, so any `ref` a page sends matches).
    sent: HashMap<String, Awaited>,
    /// The op being written, if the page waits on its answer.
    writing: Option<(Value, Awaited)>,
    /// Awaited ops so far: it orders them (`Awaited::seq`) and numbers the
    /// pipe's own refs (`awaited`).
    refs: u64,
    /// The host sent this connection's first snapshot: another one is a
    /// resync (`heard`).
    greeted: bool,
}

/// An op the host took whose answer the page waits on.
struct Awaited {
    /// The op's name and the page's own ids, which its error carries
    /// (`ws::error_reply`). Not the op itself: an upload is megabytes.
    stub: Value,
    /// Its place among the ops this pipe wrote, so an answer that names no
    /// op settles the oldest it could be for (`Pending::heard`).
    seq: u64,
    /// Host news that answers it, for an op whose success has no `ref`.
    news: Option<News>,
}

#[derive(PartialEq)]
enum News {
    /// The request's `approval_resolved` event (`session.approve`).
    Approval { session: String, request: String },
    /// `mcp.change` for the server (`mcp.add`, `mcp.remove`).
    Connector { engine: String, name: String },
}

/// The `ref` prefix of the pipe's own refs (`Pending::awaited`).
const OWN_REF: &str = "yardd-pipe~";

/// Plenty for every op a page waits on at once. Past it an op is not
/// tracked, and says so: some op is answered without its `ref`.
const MAX_AWAITED: usize = 4096;

/// What a page gets for an op the host took when the connection ends before
/// its answer. The host may well have done it, so it claims neither way.
fn lost(name: &str) -> String {
    format!("Lost the connection to {name} before it answered. This may have gone through; check once it's back.")
}

impl Pending {
    /// Whether the page waits on the host's answer to `op`, and the `ref`
    /// that answer will carry.
    /// - An op with the page's own `ref`: a page sends one only to wait on
    ///   it, and the host answers with it, done or refused (every op the UI
    ///   and the iOS app `ask` for).
    /// - `session.create`, `session.approve`, `mcp.add` and `mcp.remove`,
    ///   which a page sends without one and whose button waits for the
    ///   host's answer or any error: the op goes out with a `ref` of the
    ///   pipe's own, which the host's `created` or error carries. An
    ///   approval's and a connector's success is news to every window
    ///   instead (`News`).
    ///
    /// Anything else is fire-and-forget, with nothing to answer.
    fn awaited(&mut self, op: &Value) -> Option<(Value, Awaited)> {
        let kind = op["op"].as_str()?;
        // Silent when refused, too (`refusal`).
        if kind == "agent.seen" {
            return None;
        }
        let text = |key: &str| op[key].as_str().unwrap_or_default().trim().to_string();
        let news = match kind {
            "session.approve" => Some(News::Approval { session: own_id(&text("sessionId")).to_string(), request: text("requestId") }),
            "mcp.add" => Some(News::Connector { engine: text("engine"), name: text("name") }),
            "mcp.remove" => Some(News::Connector { engine: text("engine"), name: text("server") }),
            _ => None,
        };
        if op["ref"].is_null() && news.is_none() && kind != "session.create" {
            return None;
        }
        self.refs += 1;
        let r = match &op["ref"] {
            Value::Null => json!(format!("{OWN_REF}{}", self.refs)),
            r => r.clone(),
        };
        let stub = json!({ "op": kind, "ref": op["ref"], "sessionId": op["sessionId"] });
        Some((r, Awaited { stub, seq: self.refs, news }))
    }

    /// The op being written went out whole: it waits for its answer now.
    fn wrote(&mut self) {
        let Some((r, awaited)) = self.writing.take() else { return };
        if self.sent.len() >= MAX_AWAITED {
            eprintln!("yardd: {} ops to {} wait on an answer; not tracking {}", self.sent.len(), self.name, awaited.stub["op"]);
            return;
        }
        self.sent.insert(r.to_string(), awaited);
    }

    /// A message from the host, before it is rewritten (it may be dropped):
    /// whatever it answers no longer waits. A `ref` of the pipe's own is
    /// taken off, so the page sees the answer as it would without the pipe.
    fn heard(&mut self, msg: &mut Value) {
        if let Some(r) = msg.get("ref").filter(|r| !r.is_null()) {
            self.sent.remove(&r.to_string());
            if r.as_str().is_some_and(|r| r.starts_with(OWN_REF)) {
                if let Some(o) = msg.as_object_mut() {
                    o.remove("ref");
                }
            }
        } else if msg["op"] == "created" {
            // A host older than v0.18.0 answers `session.create` with
            // `created` and no `ref`, whatever the op carried: the session
            // was made, so the oldest create still waiting is answered. (A
            // current host says `created` without one only for an import or
            // a hand-over.)
            let oldest = self.sent.iter().filter(|(_, a)| a.stub["op"] == "session.create").min_by_key(|(_, a)| a.seq).map(|(r, _)| r.clone());
            if let Some(r) = oldest {
                self.sent.remove(&r);
            }
        }
        if self.sent.is_empty() && msg["op"] != "snapshot" {
            return;
        }
        let news = match msg["op"].as_str() {
            Some("event") if msg["event"]["type"] == "approval_resolved" => {
                let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
                News::Approval { session: text(&msg["sessionId"]), request: text(&msg["event"]["requestId"]) }
            }
            Some("mcp.change") => News::Connector { engine: msg["engine"].as_str().unwrap_or_default().to_string(), name: msg["name"].as_str().unwrap_or_default().to_string() },
            // A host whose broadcasts fell behind sends a snapshot instead of
            // the news it skipped, and the page reloads from it: that news
            // will not come, and the page needs no answer from this pipe.
            Some("snapshot") => {
                if self.greeted {
                    self.sent.retain(|_, a| a.news.is_none());
                }
                self.greeted = true;
                return;
            }
            _ => return,
        };
        self.sent.retain(|_, a| a.news.as_ref() != Some(&news));
    }

    /// The connection ended: every op the host took and has not answered
    /// gets `lost`, once.
    fn lose_sent(&mut self) {
        let why = anyhow::Error::msg(lost(&self.name));
        for awaited in self.writing.take().map(|(_, a)| a).into_iter().chain(self.sent.drain().map(|(_, a)| a)) {
            let _ = self.direct.send(error_for(&self.host, &awaited.stub, &why));
        }
    }

    fn fail(&mut self, why: &str) {
        while let Ok(op) = self.rx.try_recv() {
            self.held.push_back(op);
        }
        for op in self.held.drain(..) {
            if let Some(refused) = refusal(&self.host, &op, why) {
                let _ = self.direct.send(refused);
            }
        }
    }
}

impl Drop for Pending {
    /// The pipe ended: its host was removed (the page drops the pipe, or the
    /// hub's connection closes), or the page is gone.
    fn drop(&mut self) {
        self.lose_sent();
        self.fail("no such host");
    }
}

/// The error a page gets for an op that did not reach the host. A window
/// opening an agent's conversation has nothing to clear: `agent.seen` stays
/// silent, as when `Pipe::send` refuses it (ws.rs).
fn refusal(host: &str, op: &Value, why: &str) -> Option<String> {
    (op["op"] != "agent.seen").then(|| error_for(host, op, &anyhow::Error::msg(why.to_string())))
}

/// An error the hub itself makes for an op meant for `host`. It names the
/// host as that host's own errors do (`rewrite`), so a page files it under
/// the host it asked: a connector's card, say, is released by its host's
/// error only.
pub(crate) fn error_for(host: &str, op: &Value, why: &anyhow::Error) -> String {
    let mut reply = crate::ws::error_value(op, why);
    reply["hostId"] = json!(host);
    reply.to_string()
}

/// The host's url to connect to. Ops cannot wait while there is none (an
/// ssh host's tunnel is down): they went in while the host showed connected
/// (`Pipe::send`), and the tunnel may take any time to come back.
async fn url_for(conn: &mut watch::Receiver<Conn>, pending: &mut Pending) -> Option<String> {
    loop {
        let url = conn.borrow_and_update().url.clone();
        if url.is_some() {
            return url;
        }
        pending.fail(NOT_CONNECTED);
        tokio::select! {
            changed = conn.changed() => changed.ok()?,
            op = pending.rx.recv() => pending.held.push_back(op?),
        }
    }
}

/// Which host an op is for, when it is not for this yardd.
pub fn route(msg: &Value) -> Option<String> {
    // A Slack approval goes where the agent's app keeps it
    // (`slack::approval_host`).
    if crate::slack::is_access_approval(msg) {
        return None;
    }
    // Ops that name no session say which host they are for. An upload names
    // either: a host from the new-session page, a session from a reply box.
    if ["session.create", "git.branches", "fs.dirs", "attach.put", "engines.status", "account.add", "account.remove", "login.start", "login.code", "login.choose", "login.cancel", "claude.sessions", "session.import", "automation.list", "automation.save", "automation.delete", "automation.run", "mcp.status", "mcp.auth", "mcp.callback", "mcp.reconnect", "mcp.add", "mcp.remove"]
        .iter()
        .any(|op| msg["op"] == *op)
    {
        let host = msg["host"].as_str().filter(|h| !h.is_empty() && *h != "local").map(str::to_string);
        if host.is_some() || msg["op"] != "attach.put" {
            return host;
        }
    }
    // An agent another host owns: its ops go there (see `agents`).
    if let Some(id) = agent_ref(msg) {
        return id.split_once(SEP).map(|(h, _)| h.to_string());
    }
    if msg["op"] == "agent.new" || (msg["op"] == "agent.save" && msg["agent"]["id"].is_null()) {
        let host = if msg["op"] == "agent.new" { &msg["host"] } else { &msg["agent"]["host"] };
        return host.as_str().filter(|h| !h.is_empty() && *h != "local").map(str::to_string);
    }
    ["sessionId", "termId"]
        .iter()
        .find_map(|key| msg[key].as_str().and_then(|s| s.split_once(SEP)).map(|(h, _)| h.to_string()))
}

/// The agent an agent op, or an agent's own Slack app op, names: `agentId`,
/// or `id` (`agent.delete`, `agent.request`), or the saved agent's `id`.
pub fn agent_ref(msg: &Value) -> Option<&str> {
    let op = msg["op"].as_str()?;
    let slot = match op {
        "agent.ask" | "agent.seen" => &msg["agentId"],
        "agent.delete" | "agent.request" => &msg["id"],
        "agent.save" => &msg["agent"]["id"],
        _ if op.starts_with("slack.app.") || op == "slack.grant" => &msg["agentId"],
        _ => return None,
    };
    slot.as_str()
}

/// A host's own id for an id the hub shows (`<hostId>~<id>`).
fn own_id(id: &str) -> &str {
    id.split_once(SEP).map_or(id, |(_, rest)| rest)
}

fn strip(slot: &mut Value) {
    if let Some((_, rest)) = slot.as_str().and_then(|s| s.split_once(SEP)) {
        *slot = Value::String(rest.to_string());
    }
}

/// Strip the host prefix from ids on an op headed for the host.
fn outgoing(mut msg: Value) -> Value {
    // An agent op: the agent's id is the host's own there, and the host is
    // that host itself (`local`).
    match msg["op"].as_str() {
        Some("agent.delete" | "agent.request") => strip(&mut msg["id"]),
        Some("agent.save") => {
            if msg["agent"]["id"].is_string() {
                strip(&mut msg["agent"]["id"]);
            }
            if msg["agent"]["host"].is_string() {
                msg["agent"]["host"] = json!("local");
            }
        }
        _ if agent_ref(&msg).is_some() => strip(&mut msg["agentId"]),
        _ => {}
    }
    if let Some(obj) = msg.as_object_mut() {
        obj.remove("host");
        for key in ["sessionId", "termId"] {
            if let Some(slot) = obj.get_mut(key) {
                if let Some((_, rest)) = slot.as_str().and_then(|s| s.split_once(SEP)) {
                    *slot = Value::String(rest.to_string());
                }
            }
        }
    }
    msg
}

fn prefix(host: &str, obj: &mut Value, key: &str) {
    if let Some(slot) = obj.get_mut(key) {
        if let Some(id) = slot.as_str() {
            *slot = Value::String(format!("{host}{SEP}{id}"));
        }
    }
}

/// A host's agent as the hub shows it: its id and conversations carry the
/// host, and its host is that host. Nothing of the hosted-copy plumbing.
pub fn prefixed_agent(host: &str, agent: &Value) -> Value {
    let mut a = agent.clone();
    prefix(host, &mut a, "id");
    prefix(host, &mut a, "chat");
    if let Some(list) = a.get_mut("previous").and_then(Value::as_array_mut) {
        for p in list {
            if let Some(id) = p.as_str() {
                *p = Value::String(format!("{host}{SEP}{id}"));
            }
        }
    }
    if let Some(o) = a.as_object_mut() {
        o.insert("host".into(), json!(host));
        o.remove("hub");
        o.remove("hosted");
        o.remove("seed");
        o.remove("transfer");
        o.remove("leaving");
    }
    a
}

fn rewrite_session(host: &str, s: &mut Value) {
    prefix(host, s, "id");
    prefix(host, s, "parent");
    if let Some(obj) = s.as_object_mut() {
        obj.insert("host".into(), json!(host));
    }
}

fn rewrite_event(host: &str, ev: &mut Value) {
    if ev["type"] == "note" {
        prefix(host, ev, "session");
    }
}

fn rewrite_stats(host: &str, name: &str, stats: &mut Value) {
    let Some(obj) = stats.as_object_mut() else { return };
    obj.insert("id".into(), json!(host));
    obj.insert("name".into(), json!(name));
    for list in ["sessions", "queued"] {
        if let Some(items) = obj.get_mut(list).and_then(Value::as_array_mut) {
            for item in items {
                prefix(host, item, "id");
            }
        }
    }
}

/// Keep a host's last snapshot in step with the session updates after it.
fn remember(last: &Mutex<Option<Value>>, msg: &Value) {
    let mut last = last.lock().unwrap();
    if msg["op"] == "host.snapshot" {
        *last = Some(msg.clone());
        return;
    }
    let Some(list) = last.as_mut().and_then(|s| s["sessions"].as_array_mut()) else { return };
    match msg["op"].as_str() {
        Some("session") => match list.iter_mut().find(|s| s["id"] == msg["session"]["id"]) {
            Some(slot) => *slot = msg["session"].clone(),
            None => list.push(msg["session"].clone()),
        },
        Some("session.deleted") => list.retain(|s| s["id"] != msg["sessionId"]),
        _ => {}
    }
}

/// Turn a message from a remote yardd into what the UI expects from a host.
/// `None` drops it.
#[cfg(test)]
fn rewrite_incoming(host: &str, name: &str, text: &str) -> Option<String> {
    rewrite(host, name, serde_json::from_str(text).ok()?).map(|msg| msg.to_string())
}

fn rewrite(host: &str, name: &str, mut msg: Value) -> Option<Value> {
    let op = msg["op"].as_str().unwrap_or("").to_string();
    match op.as_str() {
        "snapshot" => {
            let mut sessions = msg["sessions"].take();
            if let Some(list) = sessions.as_array_mut() {
                for s in list {
                    rewrite_session(host, s);
                }
            }
            let mut stats = msg["host"].take();
            rewrite_stats(host, name, &mut stats);
            msg = json!({
                "op": "host.snapshot",
                "hostId": host,
                "sessions": sessions,
                "models": msg["models"].take(),
                "usage": msg["usage"].take(),
                "accounts": msg["accounts"].take(),
                "host": stats,
                "home": msg["home"].take(),
                "workspace": msg["workspace"].take(),
                "automations": msg["automations"].take(),
                // A phone connects to the host it can reach: the hub shows each one's.
                "mobile": msg["mobile"].take(),
            });
        }
        // A remote's own remotes are not ours to show.
        // Its agents and Slack apps reach the windows merged with the hub's
        // (`agents::shown`, `slack::view`), from the hub's own connection.
        "hosts" | "host.snapshot" | "host.down" | "switchy.state" | "agents" => return None,
        // The owner's own Slack is the hub's: a remote's is not shown.
        "slack.inbox" | "slack.inbox.message" | "slack.inbox.messages" => return None,
        // The answer to a Slack op this window sent for one of its agents.
        "slack" if !msg["ref"].is_null() => {
            msg["hostId"] = json!(host);
            msg["slack"] = crate::slack::prefixed_view(host, &msg["slack"]);
        }
        "slack" => return None,
        "agent.saved" => {
            msg["hostId"] = json!(host);
            msg["agent"] = prefixed_agent(host, &msg["agent"]);
        }
        "agent.deleted" => {
            msg["hostId"] = json!(host);
            prefix(host, &mut msg, "id");
        }
        "session" => rewrite_session(host, &mut msg["session"]),
        "host" => {
            rewrite_stats(host, name, &mut msg["host"]);
            msg["hostId"] = json!(host);
        }
        "models" | "usage" | "accounts" | "mobile.status" | "error" | "git.branches" | "fs.dirs" | "attach.stored" | "engines" | "login" | "claude.sessions" | "automations" | "mcp" | "mcp.auth" | "mcp.change" => {
            msg["hostId"] = json!(host);
            if op == "error" {
                prefix(host, &mut msg, "sessionId");
            }
        }
        "term.list" => {
            prefix(host, &mut msg, "sessionId");
            if let Some(terms) = msg["terms"].as_array_mut() {
                for t in terms {
                    prefix(host, t, "termId");
                }
            }
        }
        _ => {
            prefix(host, &mut msg, "sessionId");
            prefix(host, &mut msg, "termId");
            // An agent's conversation was opened (`created`).
            prefix(host, &mut msg, "agentId");
            // A hand-off note names a session on the same host.
            rewrite_event(host, &mut msg["event"]);
            if let Some(events) = msg["events"].as_array_mut() {
                for ev in events {
                    rewrite_event(host, ev);
                }
            }
        }
    }
    Some(msg)
}

/// Open pipes for hosts a client does not have yet and drop the ones removed.
pub fn sync_pipes(app: &Arc<App>, pipes: &mut HashMap<String, Pipe>, ids: &[String], direct: &mpsc::UnboundedSender<String>) {
    pipes.retain(|id, _| ids.contains(id));
    for id in ids {
        if !pipes.contains_key(id) {
            if let Some(p) = pipe(app, id, direct.clone()) {
                pipes.insert(id.clone(), p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_get_the_host_prefix_and_lose_it_on_the_way_back() {
        let out = rewrite_incoming("h1", "box", r#"{"op":"event","sessionId":"s9","seq":0,"event":{}}"#).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["sessionId"], "h1~s9");
        let note = r#"{"op":"history","sessionId":"s9","events":[{"type":"note","text":"Handed off to Claude.","session":"s8"}]}"#;
        let v: Value = serde_json::from_str(&rewrite_incoming("h1", "box", note).unwrap()).unwrap();
        assert_eq!(v["events"][0]["session"], "h1~s8");
        let note = r#"{"op":"event","sessionId":"s8","seq":0,"event":{"type":"note","text":"Handed off from Codex.","session":"s9"}}"#;
        let v: Value = serde_json::from_str(&rewrite_incoming("h1", "box", note).unwrap()).unwrap();
        assert_eq!(v["event"]["session"], "h1~s9");
        let back = outgoing(json!({ "op": "term.input", "termId": "h1~t3", "data": "x" }));
        assert_eq!(back["termId"], "t3");
        assert_eq!(route(&json!({ "op": "session.send", "sessionId": "h1~s9" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "session.send", "sessionId": "s9" })), None);
        assert_eq!(route(&json!({ "op": "session.create", "host": "local" })), None);
        assert_eq!(route(&json!({ "op": "claude.sessions", "host": "h1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "session.import", "host": "h1", "claudeSessionId": "x" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "git.branches", "cwd": "/x", "host": "h1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "git.branches", "cwd": "/x" })), None);
        assert_eq!(route(&json!({ "op": "fs.dirs", "path": "/x", "host": "h1" })).as_deref(), Some("h1"));
        let reply = rewrite_incoming("h1", "box", r#"{"op":"git.branches","cwd":"/x","branches":[]}"#).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&reply).unwrap()["hostId"], "h1");
        // Uploads go by host on the new-session page and by session in a reply.
        assert_eq!(route(&json!({ "op": "attach.put", "host": "h1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "attach.put", "sessionId": "h1~s9" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "attach.put", "sessionId": "s9" })), None);
        assert_eq!(route(&json!({ "op": "attach.put", "host": "local" })), None);
        assert_eq!(route(&json!({ "op": "attach.get", "sessionId": "h1~s9", "id": "a1" })).as_deref(), Some("h1"));
        let stored = rewrite_incoming("h1", "box", r#"{"op":"attach.stored","ref":"r1","attachment":{"id":"a1"}}"#).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap()["hostId"], "h1");
        let data = rewrite_incoming("h1", "box", r#"{"op":"attach.data","sessionId":"s9","id":"a1","data":""}"#).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&data).unwrap()["sessionId"], "h1~s9");
        // A file link in a remote session reads the file on that host.
        let get = json!({ "op": "file.get", "sessionId": "h1~s9", "path": "report.csv", "ref": "r2" });
        assert_eq!(route(&get).as_deref(), Some("h1"));
        assert_eq!(outgoing(get)["sessionId"], "s9");
        let file = rewrite_incoming("h1", "box", r#"{"op":"file.data","ref":"r2","sessionId":"s9","name":"report.csv","data":""}"#).unwrap();
        let file: Value = serde_json::from_str(&file).unwrap();
        assert_eq!((file["sessionId"].as_str(), file["ref"].as_str()), (Some("h1~s9"), Some("r2")));
    }

    #[test]
    fn terminal_ids_are_prefixed_inside_a_term_list() {
        let text = r#"{"op":"term.list","sessionId":"s1","terms":[{"termId":"t1","title":"1"},{"termId":"t4","title":"2"}]}"#;
        let v: Value = serde_json::from_str(&rewrite_incoming("h1", "box", text).unwrap()).unwrap();
        assert_eq!(v["sessionId"], "h1~s1");
        assert_eq!(v["terms"][0]["termId"], "h1~t1");
        assert_eq!(v["terms"][1]["termId"], "h1~t4");
        assert_eq!(v["terms"][1]["title"], "2");
        let changed = rewrite_incoming("h1", "box", r#"{"op":"term.listChanged","sessionId":"s1"}"#).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&changed).unwrap()["sessionId"], "h1~s1");
        assert_eq!(route(&json!({ "op": "term.new", "sessionId": "h1~s1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "term.attach", "termId": "h1~t1" })).as_deref(), Some("h1"));
        assert_eq!(outgoing(json!({ "op": "term.list", "sessionId": "h1~s1" }))["sessionId"], "s1");
    }

    #[test]
    fn a_snapshot_becomes_a_host_snapshot() {
        let text = r#"{"op":"snapshot","sessions":[{"id":"s1","parent":"s0","host":"local"}],"models":{},"usage":{},"host":{"id":"local","name":"This Mac","sessions":[{"id":"s1"}]},"home":"/home/me","mobile":{"configured":true,"origins":["https://box.ts.net"]}}"#;
        let v: Value = serde_json::from_str(&rewrite_incoming("h1", "box", text).unwrap()).unwrap();
        assert_eq!(v["op"], "host.snapshot");
        // The phone connects to the box; the hub shows where to point it and whether one has.
        assert_eq!(v["mobile"]["origins"][0], "https://box.ts.net");
        let status: Value = serde_json::from_str(&rewrite_incoming("h1", "box", r#"{"op":"mobile.status","mobile":{"configured":true}}"#).unwrap()).unwrap();
        assert_eq!(status["hostId"], "h1");
        assert_eq!(v["sessions"][0]["id"], "h1~s1");
        assert_eq!(v["sessions"][0]["parent"], "h1~s0");
        assert_eq!(v["sessions"][0]["host"], "h1");
        assert_eq!(v["host"]["name"], "box");
        assert_eq!(v["host"]["sessions"][0]["id"], "h1~s1");
    }

    #[test]
    fn a_hosts_agents_ops_go_there_with_its_own_ids_and_come_back_prefixed() {
        assert_eq!(route(&json!({ "op": "agent.ask", "agentId": "h1~g1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "agent.ask", "agentId": "g1" })), None);
        assert_eq!(route(&json!({ "op": "agent.delete", "id": "h1~g1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "agent.save", "agent": { "id": "h1~g1" } })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "agent.save", "agent": { "name": "x", "host": "h1" } })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "agent.save", "agent": { "id": "g1", "host": "h1" } })), None, "a move is the hub's");
        assert_eq!(route(&json!({ "op": "agent.new", "host": "h1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "slack.app.icon", "agentId": "h1~g1" })).as_deref(), Some("h1"));
        assert_eq!(route(&json!({ "op": "slack.me.retry" })), None, "the owner's own Slack is the hub's");
        let out = outgoing(json!({ "op": "agent.save", "agent": { "id": "h1~g1", "host": "h1", "memory": "m" } }));
        assert_eq!(out["agent"], json!({ "id": "g1", "host": "local", "memory": "m" }));
        assert!(outgoing(json!({ "op": "agent.save", "agent": { "name": "x", "host": "h1" } }))["agent"].get("id").is_none());
        assert_eq!(outgoing(json!({ "op": "agent.request", "id": "h1~g1" }))["id"], "g1");
        assert_eq!(outgoing(json!({ "op": "slack.app.appToken", "agentId": "h1~g1" }))["agentId"], "g1");
        assert_eq!(outgoing(json!({ "op": "automation.delete", "id": "a1", "host": "h1" }))["id"], "a1");
        let saved: Value = serde_json::from_str(&rewrite_incoming("h1", "box", r#"{"op":"agent.saved","ref":"r","agent":{"id":"g1","chat":"s1","previous":["s0"],"host":"local","hosted":false}}"#).unwrap()).unwrap();
        assert_eq!(saved["agent"]["id"], "h1~g1");
        assert_eq!(saved["agent"]["chat"], "h1~s1");
        assert_eq!(saved["agent"]["previous"], json!(["h1~s0"]));
        assert_eq!(saved["agent"]["host"], "h1");
        assert!(saved["agent"].get("hosted").is_none());
        let created: Value = serde_json::from_str(&rewrite_incoming("h1", "box", r#"{"op":"created","sessionId":"s1","agentId":"g1","ref":"r"}"#).unwrap()).unwrap();
        assert_eq!((created["sessionId"].as_str(), created["agentId"].as_str()), (Some("h1~s1"), Some("h1~g1")));
        // Its broadcasts reach the windows merged by the hub, not piped.
        assert!(rewrite_incoming("h1", "box", r#"{"op":"agents","agents":[]}"#).is_none());
        assert!(rewrite_incoming("h1", "box", r#"{"op":"slack","slack":{}}"#).is_none());
        let answer: Value = serde_json::from_str(&rewrite_incoming("h1", "box", r#"{"op":"slack","ref":"r","slack":{"agentApps":{"g1":{}}}}"#).unwrap()).unwrap();
        assert!(answer["slack"]["agentApps"]["h1~g1"].is_object());
        // No `previous` stays none.
        assert!(prefixed_agent("h1", &json!({ "id": "g1" })).get("previous").is_none());
    }

    #[test]
    fn destinations_that_look_like_options_are_refused() {
        assert!(check_destination("-oProxyCommand=evil").is_err());
        assert!(check_destination("devbox -v").is_err());
        assert!(check_destination("user@100.64.0.1").is_ok());
    }

    // A page's pipe answers every op it takes: the host's reply, or an error
    // with the op's `ref`. These drive one pipe with a stand-in for the hub's
    // own connection (`conn`) and a hand-made host.

    fn test_pipe(conn: Conn) -> (watch::Sender<Conn>, Pipe, mpsc::UnboundedReceiver<String>) {
        let (shown, rx) = watch::channel(conn);
        let (direct, replies) = mpsc::unbounded_channel();
        let pipe = open(rx, "h".into(), "box".into(), Arc::new(Mutex::new(None)), direct);
        (shown, pipe, replies)
    }

    fn connected(url: Option<String>) -> Conn {
        Conn { url, status: "connected", error: None, no_daemon: false }
    }

    fn op(r: &str) -> Value {
        json!({ "op": "session.open", "sessionId": "h~s1", "ref": r })
    }

    /// The error replies (`ref` → message) that come back by `deadline`,
    /// stopping once `want` of them are in.
    async fn errors(replies: &mut mpsc::UnboundedReceiver<String>, want: usize, within: Duration) -> HashMap<String, String> {
        let deadline = tokio::time::Instant::now() + within;
        let mut got = HashMap::new();
        while got.len() < want {
            let Ok(Some(text)) = tokio::time::timeout_at(deadline, replies.recv()).await else { break };
            let reply: Value = serde_json::from_str(&text).unwrap();
            if reply["op"] == "error" {
                got.insert(reply["ref"].as_str().unwrap_or_default().to_string(), reply["message"].as_str().unwrap_or_default().to_string());
            }
        }
        got
    }

    /// A host that accepts connections and never answers the handshake.
    async fn silent_host() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });
        url
    }

    #[tokio::test]
    async fn an_op_taken_as_an_ssh_tunnel_drops_is_answered_without_waiting_for_it() {
        // The host showed connected when the page's op went in, and its
        // tunnel dropped before the pipe took the op: no url to connect to,
        // maybe for a long time.
        let (_shown, pipe, mut replies) = test_pipe(connected(None));
        pipe.send(op("a")).unwrap();
        let got = errors(&mut replies, 1, CONNECT_TIMEOUT + Duration::from_secs(2)).await;
        assert_eq!(got.get("a").map(String::as_str), Some(NOT_CONNECTED), "answered within the connect bound: {got:?}");
    }

    #[tokio::test]
    async fn an_op_a_pipe_still_holds_when_its_host_is_removed_gets_an_error() {
        // The host is removed while the pipe is still opening: the page's
        // socket loop drops the pipe (`sync_pipes`).
        let url = silent_host().await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url.clone())));
        pipe.send(op("a")).unwrap();
        drop(pipe);
        let got = errors(&mut replies, 1, Duration::from_secs(2)).await;
        assert_eq!(got.get("a").map(String::as_str), Some("no such host"), "{got:?}");
    }

    #[tokio::test]
    async fn every_op_a_failed_write_leaves_unsent_gets_an_error() {
        // The host completes the handshake, never reads, and lets go while
        // the pipe is still writing the first op (too big for the socket
        // buffers): that write fails, and the two ops behind it never go.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let ws = tokio_tungstenite::accept_async(socket).await.unwrap();
            tokio::time::sleep(Duration::from_millis(300)).await;
            // Closed with data unread, so the pipe's write is reset.
            drop(ws);
        });
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        let big = json!({ "op": "attach.put", "host": "h", "ref": "a", "data": "x".repeat(24 << 20) });
        pipe.send(big).unwrap();
        pipe.send(op("b")).unwrap();
        pipe.send(op("c")).unwrap();
        let got = errors(&mut replies, 3, Duration::from_secs(5)).await;
        for r in ["a", "b", "c"] {
            assert_eq!(got.get(r).map(String::as_str), Some(NOT_CONNECTED), "{r} answered: {got:?}");
        }
    }

    // An op the host took, whose answer the connection lost: one error with
    // its `ref` (or its `for`, when the page waits without one), worded so
    // it claims nothing, since the host may have done it.

    const LOST: &str = "Lost the connection to box before it answered. This may have gone through; check once it's back.";

    type HostWs = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

    /// A host that completes one pipe's handshake and runs `script` on it.
    /// The pipe's next tries are refused.
    async fn host_with<F, Fut>(script: F) -> String
    where
        F: FnOnce(HostWs) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send,
    {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/ws", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            drop(listener);
            let mut config = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default();
            config.max_message_size = None;
            config.max_frame_size = None;
            let ws = tokio_tungstenite::accept_async_with_config(socket, Some(config)).await.unwrap();
            script(ws).await;
        });
        url
    }

    /// The next op the pipe wrote.
    async fn read_op(ws: &mut HostWs) -> Value {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => return serde_json::from_str(&text).unwrap(),
                Some(Ok(_)) => continue,
                other => panic!("the pipe went away: {:?}", other.map(|r| r.map(|_| ()))),
            }
        }
    }

    async fn say(ws: &mut HostWs, msg: Value) {
        ws.send(Message::Text(msg.to_string())).await.unwrap();
    }

    /// Everything the page gets until `stop` matches one, then for `quiet`
    /// more (so a second answer would show).
    async fn page_gets(replies: &mut mpsc::UnboundedReceiver<String>, stop: impl Fn(&Value) -> bool, quiet: Duration) -> Vec<Value> {
        let mut got = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while let Ok(Some(text)) = tokio::time::timeout_at(deadline, replies.recv()).await {
            let reply: Value = serde_json::from_str(&text).unwrap();
            let done = stop(&reply);
            got.push(reply);
            if done {
                break;
            }
        }
        let deadline = tokio::time::Instant::now() + quiet;
        while let Ok(Some(text)) = tokio::time::timeout_at(deadline, replies.recv()).await {
            got.push(serde_json::from_str(&text).unwrap());
        }
        got
    }

    fn errors_in(got: &[Value]) -> Vec<&Value> {
        got.iter().filter(|r| r["op"] == "error").collect()
    }

    #[tokio::test]
    async fn an_op_the_host_took_gets_one_answer_when_the_connection_drops_before_its_reply() {
        // `a` is answered, `b` is taken and its answer never comes, `c`
        // waits on nothing (no `ref`).
        let url = host_with(|mut ws| async move {
            let a = read_op(&mut ws).await;
            say(&mut ws, json!({ "op": "session.renamed", "ref": a["ref"] })).await;
            read_op(&mut ws).await;
            read_op(&mut ws).await;
            // Gone without a close: a dropped tunnel, a host that died.
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        pipe.send(json!({ "op": "session.rename", "sessionId": "h~s1", "title": "x", "ref": "a" })).unwrap();
        pipe.send(op("b")).unwrap();
        pipe.send(json!({ "op": "session.send", "sessionId": "h~s1", "text": "hi" })).unwrap();
        let got = page_gets(&mut replies, |r| r["op"] == "host.down", Duration::from_millis(1500)).await;
        let answers = |r: &str| got.iter().filter(|m| m["ref"] == r).count();
        assert_eq!((answers("a"), answers("b")), (1, 1), "one answer each: {got:?}");
        let errors = errors_in(&got);
        assert_eq!(errors.len(), 1, "only `b` gets an error: {got:?}");
        assert_eq!((errors[0]["ref"].as_str(), errors[0]["message"].as_str(), errors[0]["for"].as_str()), (Some("b"), Some(LOST), Some("session.open")));
    }

    #[tokio::test]
    async fn an_op_the_host_took_gets_one_answer_when_its_pipe_is_dropped() {
        // The host is removed (or the page leaves) after the host read the
        // op and before it answered.
        let (took, taken) = tokio::sync::oneshot::channel();
        let url = host_with(|mut ws| async move {
            read_op(&mut ws).await;
            let _ = took.send(());
            std::future::pending::<()>().await;
            drop(ws);
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        pipe.send(op("a")).unwrap();
        taken.await.unwrap();
        drop(pipe);
        let got = page_gets(&mut replies, |r| r["op"] == "error", Duration::from_millis(500)).await;
        let errors = errors_in(&got);
        assert_eq!(errors.len(), 1, "{got:?}");
        assert_eq!((errors[0]["ref"].as_str(), errors[0]["message"].as_str()), (Some("a"), Some(LOST)));
    }

    #[tokio::test]
    async fn an_op_being_written_when_its_pipe_is_dropped_gets_one_answer() {
        // The write of a big upload is under way (the host does not read, so
        // it cannot finish) when the host is removed: the task is aborted
        // mid-write.
        let (opened, open) = tokio::sync::oneshot::channel();
        let url = host_with(|ws| async move {
            let _ = opened.send(());
            std::future::pending::<()>().await;
            drop(ws);
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        pipe.send(json!({ "op": "attach.put", "host": "h", "ref": "a", "data": "x".repeat(24 << 20) })).unwrap();
        open.await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        drop(pipe);
        let got = page_gets(&mut replies, |r| r["op"] == "error", Duration::from_millis(500)).await;
        let errors = errors_in(&got);
        assert_eq!(errors.len(), 1, "{got:?}");
        assert_eq!((errors[0]["ref"].as_str(), errors[0]["message"].as_str()), (Some("a"), Some(LOST)));
    }

    #[tokio::test]
    async fn ops_a_page_waits_on_without_a_ref_get_one_answer_too() {
        // A new session's Send, an approval's buttons and a connector being
        // added wait for an error or for the host's news: `created`, the
        // approval's event, `mcp.change`. The host answers the first of each
        // and takes the second, then the connection drops.
        let url = host_with(|mut ws| async move {
            say(&mut ws, json!({ "op": "snapshot", "sessions": [] })).await;
            let create = read_op(&mut ws).await;
            // The real host answers with the op's `ref` when it has one.
            let mut created = json!({ "op": "created", "sessionId": "s9" });
            if !create["ref"].is_null() {
                created["ref"] = create["ref"].clone();
            }
            say(&mut ws, created).await;
            let approve = read_op(&mut ws).await;
            say(&mut ws, json!({ "op": "event", "sessionId": approve["sessionId"], "seq": 3, "event": { "type": "approval_resolved", "requestId": approve["requestId"], "decision": "accept" } })).await;
            let add = read_op(&mut ws).await;
            say(&mut ws, json!({ "op": "mcp.change", "engine": add["engine"], "account": "default", "name": add["name"], "state": "done" })).await;
            for _ in 0..3 {
                read_op(&mut ws).await;
            }
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        let create = json!({ "op": "session.create", "host": "h", "engine": "codex", "model": "m", "effort": "low", "cwd": "/x", "text": "go" });
        let approve = |r: &str| json!({ "op": "session.approve", "sessionId": "h~s1", "requestId": r, "decision": "accept" });
        let add = |n: &str| json!({ "op": "mcp.add", "host": "h", "engine": "claude", "name": n, "url": "https://x.example/mcp" });
        for msg in [create.clone(), approve("r1"), add("one"), create, approve("r2"), add("two")] {
            pipe.send(msg).unwrap();
        }
        let got = page_gets(&mut replies, |r| r["op"] == "host.down", Duration::from_millis(1500)).await;
        let created: Vec<&Value> = got.iter().filter(|r| r["op"] == "created").collect();
        assert_eq!(created.len(), 1, "{got:?}");
        // As the host gave it: a `ref` the page did not send is not its own.
        assert_eq!((created[0]["sessionId"].as_str(), created[0].get("ref")), (Some("h~s9"), None), "{got:?}");
        let errors = errors_in(&got);
        let mut failed: Vec<&str> = errors.iter().map(|e| e["for"].as_str().unwrap_or_default()).collect();
        failed.sort();
        assert_eq!(failed, ["mcp.add", "session.approve", "session.create"], "one error for each op left unanswered: {got:?}");
        for e in errors {
            assert_eq!((e["message"].as_str(), e.get("ref")), (Some(LOST), None), "{e}");
        }
    }

    #[tokio::test]
    async fn a_host_that_resyncs_its_page_settles_what_its_news_would_have() {
        // A host whose broadcasts fell behind sends a fresh snapshot instead
        // of the events it skipped (the page reloads from it): an approval
        // taken before it is not reported lost, one after it is.
        let url = host_with(|mut ws| async move {
            say(&mut ws, json!({ "op": "snapshot", "sessions": [] })).await;
            read_op(&mut ws).await;
            say(&mut ws, json!({ "op": "snapshot", "sessions": [] })).await;
            read_op(&mut ws).await;
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        let approve = |r: &str| json!({ "op": "session.approve", "sessionId": "h~s1", "requestId": r, "decision": "accept" });
        pipe.send(approve("r1")).unwrap();
        let mut seen = 0;
        while seen < 2 {
            let text = tokio::time::timeout(Duration::from_secs(5), replies.recv()).await.unwrap().unwrap();
            seen += usize::from(serde_json::from_str::<Value>(&text).unwrap()["op"] == "host.snapshot");
        }
        pipe.send(approve("r2")).unwrap();
        let got = page_gets(&mut replies, |r| r["op"] == "host.down", Duration::from_millis(1000)).await;
        assert_eq!(errors_in(&got).len(), 1, "{got:?}");
    }

    #[tokio::test]
    async fn the_pipe_s_own_errors_name_their_host_and_come_after_it_went_down() {
        // A page files an error under the host it names (`hostId`): a
        // connector's row, for one. Without it the error was the hub's own.
        let url = host_with(|mut ws| async move {
            read_op(&mut ws).await;
            read_op(&mut ws).await;
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        pipe.send(op("a")).unwrap();
        pipe.send(json!({ "op": "mcp.remove", "host": "h", "engine": "claude", "server": "linear" })).unwrap();
        let got = page_gets(&mut replies, |r| r["op"] == "error" && r["for"] == "mcp.remove", Duration::from_millis(500)).await;
        let down = got.iter().position(|r| r["op"] == "host.down").expect("host.down");
        let errors: Vec<(usize, &Value)> = got.iter().enumerate().filter(|(_, r)| r["op"] == "error").collect();
        assert_eq!(errors.len(), 2, "{got:?}");
        for (i, e) in errors {
            assert_eq!(e["hostId"], "h", "{e}");
            // The page knows the host is down when the error comes: its
            // connectors are hidden then, so it says it where it shows.
            assert!(i > down, "after host.down: {got:?}");
        }
        // Refused before it went: no url, and still held when the pipe goes.
        let (_shown, pipe, mut replies) = test_pipe(connected(None));
        pipe.send(op("b")).unwrap();
        let got = errors_in(&page_gets(&mut replies, |r| r["op"] == "error", Duration::from_millis(100)).await).into_iter().cloned().collect::<Vec<_>>();
        assert_eq!((got.len(), got[0]["hostId"].as_str()), (1, Some("h")), "{got:?}");
        let url = silent_host().await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        pipe.send(op("c")).unwrap();
        drop(pipe);
        let got = errors_in(&page_gets(&mut replies, |r| r["op"] == "error", Duration::from_millis(100)).await).into_iter().cloned().collect::<Vec<_>>();
        assert_eq!((got.len(), got[0]["hostId"].as_str()), (1, Some("h")), "{got:?}");
    }

    #[tokio::test]
    async fn a_host_from_before_v0_18_is_not_reported_lost_for_the_sessions_it_made() {
        // Before v0.18.0 a host answered `session.create` with `created`
        // and no `ref` (and its snapshot had no `agents`). The sessions were
        // made and announced: a later disconnect has nothing to report.
        let url = host_with(|mut ws| async move {
            say(&mut ws, json!({ "op": "snapshot", "sessions": [] })).await;
            for sid in ["s8", "s9"] {
                read_op(&mut ws).await;
                say(&mut ws, json!({ "op": "created", "sessionId": sid })).await;
            }
            // Updated and restarted, say, a while later.
            tokio::time::sleep(Duration::from_millis(300)).await;
            let _ = ws.close(None).await;
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        let create = json!({ "op": "session.create", "host": "h", "engine": "codex", "model": "m", "effort": "low", "cwd": "/x", "text": "go" });
        pipe.send(create.clone()).unwrap();
        // The iOS app's, with a `ref` of its own.
        let mut ios = create;
        ios["ref"] = json!("m1");
        pipe.send(ios).unwrap();
        let got = page_gets(&mut replies, |r| r["op"] == "host.down", Duration::from_millis(1000)).await;
        assert_eq!(got.iter().filter(|r| r["op"] == "created").count(), 2, "{got:?}");
        assert!(errors_in(&got).is_empty(), "no error: {got:?}");
    }

    #[tokio::test]
    async fn a_healthy_host_answers_every_op_once_and_in_order() {
        // The healthy path: a big upload and many ops, each answered by the
        // host, then a clean end. No error, nothing twice, the same order.
        let (n, big) = (300, 16usize << 20);
        let url = host_with(move |mut ws| async move {
            for _ in 0..n + 2 {
                let op = read_op(&mut ws).await;
                if op["ref"].is_null() {
                    continue;
                }
                let reply = if op["op"] == "attach.put" {
                    json!({ "op": "attach.stored", "ref": op["ref"], "attachment": { "id": "a1", "size": op["data"].as_str().map_or(0, str::len) } })
                } else {
                    json!({ "op": "session.renamed", "ref": op["ref"] })
                };
                say(&mut ws, reply).await;
            }
            let _ = ws.close(None).await;
        })
        .await;
        let (_shown, pipe, mut replies) = test_pipe(connected(Some(url)));
        pipe.send(json!({ "op": "attach.put", "host": "h", "ref": "big", "data": "x".repeat(big) })).unwrap();
        for i in 0..n {
            pipe.send(json!({ "op": "session.rename", "sessionId": "h~s1", "title": "t", "ref": format!("r{i}") })).unwrap();
        }
        pipe.send(json!({ "op": "session.send", "sessionId": "h~s1", "text": "no answer wanted" })).unwrap();
        let got = page_gets(&mut replies, |r| r["op"] == "host.down", Duration::from_millis(1500)).await;
        assert!(errors_in(&got).is_empty(), "no error: {:?}", errors_in(&got));
        let refs: Vec<&str> = got.iter().filter_map(|r| r["ref"].as_str()).collect();
        let want: Vec<String> = std::iter::once("big".to_string()).chain((0..n).map(|i| format!("r{i}"))).collect();
        assert_eq!(refs, want, "every op answered once, in the order sent");
        assert_eq!(got[0]["attachment"]["size"], big, "the upload arrived whole");
    }
}

/// How long a daemon-owned request waits for its answer
/// (`YARD_REQUEST_TIMEOUT_MS` for tests of a lost answer).
fn request_timeout() -> Duration {
    Duration::from_millis(std::env::var("YARD_REQUEST_TIMEOUT_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(20_000))
}

/// A daemon-owned request, independent of any browser connection: the host
/// agent keeps collecting if the user closes its tab, and a remote agent's
/// message reaches its host whoever sent it. Answered by the first reply with
/// its `ref`. Never accepts arbitrary URLs.
pub async fn request(app: &Arc<App>, host: &str, mut msg: Value) -> Result<Value> {
    let (conn, _, _) = app.remotes.lookup(host).context("unknown host")?;
    let connection = conn.borrow().clone();
    anyhow::ensure!(connection.status == "connected", "host is offline");
    let url = connection.url.context("host has no connection")?;
    let reference = app.id("context-read-");
    msg["ref"] = json!(reference);
    tokio::time::timeout(request_timeout(), async {
        let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
        socket.send(Message::Text(msg.to_string())).await?;
        while let Some(frame) = socket.next().await {
            if let Message::Text(text) = frame? {
                let reply: Value = serde_json::from_str(&text)?;
                if reply["ref"] == reference {
                    if reply["op"] == "error" {
                        let message = reply["message"].as_str().unwrap_or("host request failed").to_string();
                        // A hand-over refusal keeps its code (`agents::Refusal`).
                        if let Some(code) = reply["code"].as_str() {
                            return Err(crate::agents::Refusal { code: code.to_string(), message }.into());
                        }
                        bail!("{message}");
                    }
                    return Ok(reply);
                }
            }
        }
        bail!("the host disconnected before it answered")
    }).await.context("the host did not answer in time")?
}
