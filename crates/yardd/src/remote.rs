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
    collections::HashMap,
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
    tasks.push(tokio::spawn(monitor(app.clone(), conn.clone())));
    Entry { config, conn, tasks, ssh_pid, last: Default::default() }
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

/// The hub's own connection to a host. It only decides the status the UI
/// shows; clients get their own connections in `pipe`.
async fn monitor(app: Arc<App>, conn: Arc<watch::Sender<Conn>>) {
    let mut rx = conn.subscribe();
    let mut backoff = Backoff::new(Duration::from_secs(10));
    loop {
        let url = match rx.wait_for(|c| c.url.is_some()).await {
            Ok(c) => c.url.clone().unwrap_or_default(),
            Err(_) => return,
        };
        match connect(&url).await {
            Ok(mut ws) => {
                set(&app, &conn, |c| {
                    c.status = "connected";
                    c.error = None;
                    c.no_daemon = false;
                });
                backoff.reset();
                while let Some(Ok(_)) = ws.next().await {}
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
    task: JoinHandle<()>,
}

impl Drop for Pipe {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Pipe {
    /// Forward an op to the host, or say why it cannot go.
    pub fn send(&self, msg: Value) -> Result<()> {
        if !self.up.load(Ordering::Relaxed) {
            bail!("this host is not connected");
        }
        self.tx.send(outgoing(msg)).ok().context("this host is not connected")
    }
}

/// Open (and keep reopening) a client's connection to a host. Everything the
/// host sends goes to `direct`, with ids rewritten.
pub fn pipe(app: &Arc<App>, host_id: &str, direct: mpsc::UnboundedSender<String>) -> Option<Pipe> {
    let (mut conn, name, last) = app.remotes.lookup(host_id)?;
    let (tx, mut rx) = mpsc::unbounded_channel::<Value>();
    let up = Arc::new(AtomicBool::new(false));
    let host = host_id.to_string();
    let flag = up.clone();
    if conn.borrow().status != "connected" {
        if let Some(mut snapshot) = last.lock().unwrap().clone() {
            // Its stats are stale; the host's row says it is offline instead.
            snapshot["host"] = Value::Null;
            let _ = direct.send(snapshot.to_string());
            let _ = direct.send(json!({ "op": "host.down", "hostId": host }).to_string());
        }
    }
    let task = tokio::spawn(async move {
        let mut backoff = Backoff::new(Duration::from_secs(10));
        loop {
            let url = match conn.wait_for(|c| c.url.is_some()).await {
                Ok(c) => c.url.clone().unwrap_or_default(),
                Err(_) => return,
            };
            if let Ok(ws) = connect(&url).await {
                backoff.reset();
                let (mut sink, mut stream) = ws.split();
                flag.store(true, Ordering::Relaxed);
                loop {
                    tokio::select! {
                        incoming = stream.next() => match incoming {
                            Some(Ok(Message::Text(text))) => {
                                if let Some(out) = rewrite(&host, &name, &text) {
                                    remember(&last, &out);
                                    if direct.send(out.to_string()).is_err() { return; }
                                }
                            }
                            Some(Ok(_)) => {}
                            _ => break,
                        },
                        op = rx.recv() => match op {
                            Some(op) => {
                                if sink.send(Message::Text(op.to_string())).await.is_err() { break; }
                            }
                            None => return,
                        },
                    }
                }
                flag.store(false, Ordering::Relaxed);
                let _ = direct.send(json!({ "op": "host.down", "hostId": host }).to_string());
            }
            backoff.wait().await;
        }
    });
    Some(Pipe { tx, up, task })
}

/// Which host an op is for, when it is not for this yardd.
pub fn route(msg: &Value) -> Option<String> {
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
    ["sessionId", "termId"]
        .iter()
        .find_map(|key| msg[key].as_str().and_then(|s| s.split_once(SEP)).map(|(h, _)| h.to_string()))
}

/// Strip the host prefix from ids on an op headed for the host.
fn outgoing(mut msg: Value) -> Value {
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
    rewrite(host, name, text).map(|msg| msg.to_string())
}

fn rewrite(host: &str, name: &str, text: &str) -> Option<Value> {
    let mut msg: Value = serde_json::from_str(text).ok()?;
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
        // Agents live on the hub: a remote's own are not offered here.
        "hosts" | "host.snapshot" | "host.down" | "switchy.state" | "agents" => return None,
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
    fn destinations_that_look_like_options_are_refused() {
        assert!(check_destination("-oProxyCommand=evil").is_err());
        assert!(check_destination("devbox -v").is_err());
        assert!(check_destination("user@100.64.0.1").is_ok());
    }
}

/// A daemon-owned read request, independent of any browser connection: the
/// host agent keeps collecting if the user closes its tab. Never accepts arbitrary URLs.
pub async fn request(app: &Arc<App>, host: &str, mut msg: Value) -> Result<Value> {
    let (conn, _, _) = app.remotes.lookup(host).context("unknown host")?;
    let connection = conn.borrow().clone();
    anyhow::ensure!(connection.status == "connected", "host is offline");
    let url = connection.url.context("host has no connection")?;
    let reference = app.id("context-read-");
    msg["ref"] = json!(reference);
    tokio::time::timeout(Duration::from_secs(20), async {
        let (mut socket, _) = tokio_tungstenite::connect_async(url).await?;
        socket.send(Message::Text(msg.to_string())).await?;
        while let Some(frame) = socket.next().await {
            if let Message::Text(text) = frame? {
                let reply: Value = serde_json::from_str(&text)?;
                if reply["ref"] == reference {
                    if reply["op"] == "error" { bail!("{}", reply["message"].as_str().unwrap_or("host request failed")); }
                    return Ok(reply);
                }
            }
        }
        bail!("host disconnected during context collection")
    }).await.context("host context request timed out")?
}
