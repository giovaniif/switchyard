//! Harness for yardd's integration tests: a sandbox per test, the built
//! daemon started on a free port with the fake Codex first on PATH, and a
//! WebSocket client that waits for messages with timeouts.
//!
//! Nothing here touches port 47717, `~/.yard` or a real engine: every daemon
//! gets a temp data dir and a temp HOME, `codex` and `claude` resolve to the
//! scripts in `tests/fake-codex`, and the Claude sidecar is the fake `claude`.

#![allow(dead_code)]

use std::{
    fs,
    io::{Read, Write},
    net::TcpStream as StdTcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

/// How long any single wait may take before the test fails.
pub const WAIT: Duration = Duration::from_secs(20);

pub const MODEL: &str = "fake-fast";
pub const EFFORT: &str = "low";

static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

pub fn fake_bin() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-codex")
}

/// A free loopback port, away from the user's yardd and LocalStack.
pub fn free_port() -> u16 {
    loop {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if port != 47717 && !(4510..=4559).contains(&port) {
            return port;
        }
    }
}

/// A temp folder for one test: the daemon's data dir, HOME, the fake's state
/// and work folders. Removed on drop unless the test failed, so it can be read.
pub struct Sandbox {
    pub root: PathBuf,
}

impl Sandbox {
    pub fn new(tag: &str) -> Sandbox {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("yardd-test-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for sub in ["data", "home", "fake", "work"] {
            fs::create_dir_all(root.join(sub)).unwrap();
        }
        // One spelling of the path (macOS temp is behind /var -> /private/var),
        // so paths yardd reports compare equal to ours.
        let root = root.canonicalize().unwrap();
        Sandbox { root }
    }

    pub fn data(&self) -> PathBuf {
        self.root.join("data")
    }

    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// The fake engine's log and thread list.
    pub fn fake_state(&self) -> PathBuf {
        self.root.join("fake")
    }

    pub fn codex_log(&self) -> String {
        fs::read_to_string(self.fake_state().join("codex.log")).unwrap_or_default()
    }

    /// The `input` of every `turn/start` the fake Codex received, in order.
    pub fn turn_inputs(&self) -> Vec<Value> {
        fs::read_to_string(self.fake_state().join("turn-inputs.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str::<Value>(l).unwrap()["input"].clone())
            .collect()
    }

    /// A new folder for sessions to run in.
    pub fn folder(&self, name: &str) -> String {
        let dir = self.root.join("work").join(name);
        fs::create_dir_all(&dir).unwrap();
        dir.to_str().unwrap().to_string()
    }

    pub fn start(&self) -> Daemon {
        self.start_with(&[])
    }

    pub fn start_with(&self, env: &[(&str, &str)]) -> Daemon {
        Daemon::start(self, env)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("kept {} for inspection", self.root.display());
        } else {
            // A file appearing mid-removal (a late log line) fails one pass.
            for _ in 0..3 {
                if fs::remove_dir_all(&self.root).is_ok() || !self.root.exists() {
                    break;
                }
            }
        }
    }
}

/// A running yardd. Killed on drop, so a failing test leaves nothing behind.
pub struct Daemon {
    child: Option<Child>,
    pub port: u16,
    pub log: PathBuf,
}

impl Daemon {
    fn start(sandbox: &Sandbox, env: &[(&str, &str)]) -> Daemon {
        // free_port() lets the port go before yardd binds it, so a test running
        // in parallel can take it first. This yardd then exits, and /health is
        // answered by the other test's daemon: try a fresh port instead.
        for _ in 0..5 {
            if let Some(daemon) = Daemon::try_start(sandbox, env) {
                return daemon;
            }
        }
        panic!("yardd lost its port to another process five times");
    }

    fn try_start(sandbox: &Sandbox, env: &[(&str, &str)]) -> Option<Daemon> {
        let port = free_port();
        let log = sandbox.data().join("yardd.log");
        let stderr = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .unwrap();
        let path = format!(
            "{}:{}",
            fake_bin().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_yardd"));
        cmd.current_dir(&sandbox.root)
            .env("YARD_PORT", port.to_string())
            .env("YARD_DATA_DIR", sandbox.data())
            .env("YARD_ROOT", repo_root())
            .env("YARD_WORKSPACE", sandbox.root.join("work"))
            .env("PATH", path)
            .env("HOME", sandbox.home())
            .env("SHELL", "/bin/bash")
            .env("BASH_SILENCE_DEPRECATION_WARNING", "1")
            .env("FAKE_CODEX_DIR", sandbox.fake_state())
            .env("YARD_SIDECAR", fake_bin().join("claude"))
            // A busy test machine must not queue sessions on its own; the
            // queue test forces pressure instead. Its load did: on CI's 4
            // cores a load of 6.7 held a watched channel's turn until the
            // test timed out.
            .env("YARD_MIN_FREE_GB", "0")
            .env("YARD_TEST_IGNORE_LOAD", "1")
            // Naming would add engine turns other tests count; tests/naming.rs turns it on.
            .env("YARD_SESSION_NAMES", "off")
            // A test run from inside a Switchyard session inherits its account's
            // folders; the fakes would then read and write the real ones under ~/.yard.
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env_remove("GROK_HOME")
            // No background Claude usage reads, except where a test asks for them.
            .env("YARD_CLAUDE_USAGE_EVERY_MS", "0")
            // Nor model list reads: Claude keeps its built-in list.
            .env("YARD_CLAUDE_MODELS_EVERY_MS", "0")
            .env_remove("YARD_FORCE_PRESSURE")
            .env_remove("YARD_ALLOW_ORIGIN")
            .env_remove("YARD_HOST_NAME")
            .env_remove("YARD_BIND")
            .env_remove("YARD_CODEX_SANDBOX")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let from = fs::metadata(&log).map(|m| m.len() as usize).unwrap_or(0);
        let child = cmd.spawn().expect("could not start yardd");
        let mut daemon = Daemon {
            child: Some(child),
            port,
            log,
        };
        daemon.wait_ready(from).then_some(daemon)
    }

    pub fn pid(&self) -> u32 {
        self.child.as_ref().unwrap().id()
    }

    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}/ws", self.port)
    }

    pub fn log_text(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// True once this yardd answers; false when it lost its port. `from` is
    /// where this start's lines begin in the log (a restart appends to it).
    fn wait_ready(&mut self, from: usize) -> bool {
        let deadline = Instant::now() + WAIT;
        let port = format!(":{}", self.port);
        let listening = |text: &str| {
            text.lines()
                .any(|l| l.starts_with("yardd: http://") && l.ends_with(&port))
        };
        let ours = |d: &Daemon| d.log_text().get(from..).unwrap_or_default().to_string();
        loop {
            if let Some(status) = self.child.as_mut().unwrap().try_wait().unwrap() {
                let text = ours(self);
                if text.contains("Address already in use") {
                    self.child = None;
                    return false;
                }
                panic!("yardd exited during startup ({status}):\n{text}");
            }
            // Only our own yardd's log says it is listening on this port.
            if listening(&ours(self)) {
                if let Ok(body) = http_get(self.port, "/health") {
                    if body.contains("yardd ") {
                        return true;
                    }
                }
            }
            if Instant::now() > deadline {
                panic!(
                    "yardd on port {} did not answer /health within {WAIT:?}",
                    self.port
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub async fn client(&self) -> Client {
        Client::connect(&self.url()).await
    }

    /// A normal stop (SIGTERM): yardd flushes its store, then exits.
    pub fn stop(mut self) {
        let mut child = self.child.take().unwrap();
        let kids = children(child.id());
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(child.id().to_string())
            .status();
        let deadline = Instant::now() + WAIT;
        while child.try_wait().unwrap().is_none() {
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                reap(&kids);
                panic!("yardd did not stop on SIGTERM within {WAIT:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        reap(&kids);
    }

    /// SIGKILL: no flush, no cleanup, as after a crash.
    pub fn kill(mut self) {
        let mut child = self.child.take().unwrap();
        let kids = children(child.id());
        let _ = child.kill();
        let _ = child.wait();
        reap(&kids);
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let kids = children(child.id());
            let _ = child.kill();
            let _ = child.wait();
            reap(&kids);
        }
    }
}

/// The direct children of a process: the fake Codex, terminal shells, git.
fn children(pid: u32) -> Vec<String> {
    let out = Command::new("pgrep")
        .arg("-P")
        .arg(pid.to_string())
        .output();
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .split_whitespace()
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default()
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Wait for a stopped yardd's children to go: they exit on their own when
/// yardd's end of their pipes or PTY closes. Whatever is still there after a
/// few seconds is killed, so no test leaves a process (or a write into its
/// sandbox) behind.
fn reap(kids: &[String]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut left: Vec<&String> = kids.iter().collect();
    while !left.is_empty() && Instant::now() < deadline {
        left.retain(|pid| alive(pid));
        if !left.is_empty() {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    for pid in left {
        let _ = Command::new("kill").args(["-KILL", pid.as_str()]).status();
    }
}

/// A bare HTTP/1.1 GET, enough for `/health`.
pub fn http_get(port: u16, path: &str) -> std::io::Result<String> {
    let mut s = StdTcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(2)))?;
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n"
    )?;
    let mut out = String::new();
    s.read_to_string(&mut out)?;
    Ok(out)
}

/// A WebSocket client that keeps every message it received, so a wait can
/// find a message that arrived while the test waited for another.
pub struct Client {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
    pub log: Vec<Value>,
}

impl Client {
    pub async fn connect(url: &str) -> Client {
        let (ws, _) = tokio::time::timeout(WAIT, connect_async(url))
            .await
            .unwrap_or_else(|_| panic!("timed out connecting to {url}"))
            .unwrap_or_else(|e| panic!("could not connect to {url}: {e}"));
        let mut client = Client {
            ws,
            log: Vec::new(),
        };
        client
            .wait_for("the first snapshot", |m| m["op"] == "snapshot")
            .await;
        client
    }

    pub async fn send(&mut self, msg: Value) {
        self.ws
            .send(Message::Text(msg.to_string()))
            .await
            .expect("send failed");
    }

    /// Several messages in one write, so the daemon reads them together, as
    /// it does from a client that sends faster than it is scheduled.
    pub async fn send_burst(&mut self, msgs: &[Value]) {
        for msg in msgs {
            self.ws.feed(Message::Text(msg.to_string())).await.expect("send failed");
        }
        self.ws.flush().await.expect("send failed");
    }

    /// Where the log ends now; pass it to `wait_after` to ignore older messages.
    pub fn mark(&self) -> usize {
        self.log.len()
    }

    pub async fn wait_for(&mut self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        self.wait_after(0, what, pred).await
    }

    /// The first message at or after `from` that matches, waiting for more as needed.
    pub async fn wait_after(
        &mut self,
        from: usize,
        what: &str,
        pred: impl Fn(&Value) -> bool,
    ) -> Value {
        let deadline = tokio::time::Instant::now() + WAIT;
        let mut checked = from;
        loop {
            while checked < self.log.len() {
                if pred(&self.log[checked]) {
                    return self.log[checked].clone();
                }
                checked += 1;
            }
            let next = tokio::time::timeout_at(deadline, self.ws.next()).await;
            match next {
                Err(_) => panic!(
                    "timed out after {WAIT:?} waiting for {what}; last messages:\n{}",
                    self.tail(12)
                ),
                Ok(None) => panic!("socket closed while waiting for {what}"),
                Ok(Some(Err(e))) => panic!("socket error while waiting for {what}: {e}"),
                Ok(Some(Ok(Message::Text(text)))) => self
                    .log
                    .push(serde_json::from_str(&text).expect("yardd sent invalid JSON")),
                Ok(Some(Ok(_))) => {}
            }
        }
    }

    /// Read whatever arrives within `d` without waiting for anything specific.
    /// Only used to show that something did *not* happen.
    pub async fn drain_for(&mut self, d: Duration) {
        let deadline = tokio::time::Instant::now() + d;
        while let Ok(Some(Ok(msg))) = tokio::time::timeout_at(deadline, self.ws.next()).await {
            if let Message::Text(text) = msg {
                self.log.push(serde_json::from_str(&text).unwrap());
            }
        }
    }

    fn tail(&self, n: usize) -> String {
        self.log
            .iter()
            .filter(|m| m["op"] != "host")
            .rev()
            .take(n)
            .map(|m| {
                let s = m.to_string();
                s.chars().take(300).collect::<String>()
            })
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Send a `session.create` (codex, fake model) and return the new id, or
    /// yardd's error message.
    pub async fn try_create(
        &mut self,
        cwd: &str,
        text: &str,
        extra: Value,
    ) -> Result<String, String> {
        let mut msg = json!({
            "op": "session.create",
            "engine": "codex",
            "model": MODEL,
            "effort": EFFORT,
            "cwd": cwd,
            "text": text,
        });
        if let (Some(m), Some(e)) = (msg.as_object_mut(), extra.as_object()) {
            m.extend(e.clone());
        }
        let from = self.mark();
        self.send(msg).await;
        let reply = self
            .wait_after(from, "`created` or an error for session.create", |m| {
                m["op"] == "created" || (m["op"] == "error" && m["for"] == "session.create")
            })
            .await;
        match reply["op"].as_str() {
            Some("created") => Ok(reply["sessionId"].as_str().unwrap().to_string()),
            _ => Err(reply["message"].as_str().unwrap_or("").to_string()),
        }
    }

    /// Create a session and wait until its first turn has started. Should the
    /// test machine itself be under load, the session is started by hand
    /// rather than left in the queue.
    pub async fn create(&mut self, cwd: &str, text: &str, extra: Value) -> String {
        let sid = self
            .try_create(cwd, text, extra)
            .await
            .unwrap_or_else(|e| panic!("session.create failed: {e}"));
        let first = self
            .wait_for("the first turn to start or queue", |m| {
                is_event(m, &sid)
                    && (m["event"]["type"] == "turn_started" || m["event"]["type"] == "queued")
            })
            .await;
        if first["event"]["type"] == "queued" {
            self.send(json!({ "op": "session.startNow", "sessionId": sid }))
                .await;
            self.wait_event(&sid, "turn_started", 0).await;
        }
        sid
    }

    /// The first event of this type with `seq >= from_seq`.
    pub async fn wait_event(&mut self, sid: &str, ty: &str, from_seq: u64) -> Value {
        let what = format!("event `{ty}` (seq >= {from_seq}) in {sid}");
        let msg = self
            .wait_for(&what, |m| {
                is_event(m, sid) && m["event"]["type"] == ty && m["seq"].as_u64() >= Some(from_seq)
            })
            .await;
        let mut ev = msg["event"].clone();
        ev["seq"] = msg["seq"].clone();
        ev
    }

    /// Wait for the turn that is running now (or the next one) to end.
    pub async fn wait_turn_done(&mut self, sid: &str, from_seq: u64) -> Value {
        self.wait_event(sid, "turn_completed", from_seq).await
    }

    /// The next free sequence number in this session, as far as seen.
    pub fn next_seq(&self, sid: &str) -> u64 {
        self.log
            .iter()
            .filter(|m| is_event(m, sid))
            .filter_map(|m| m["seq"].as_u64())
            .max()
            .map_or(0, |s| s + 1)
    }

    pub async fn wait_status(&mut self, sid: &str, status: &str) -> Value {
        let what = format!("{sid} to have status `{status}`");
        self.wait_for(&what, |m| {
            m["op"] == "session" && m["session"]["id"] == sid && m["session"]["status"] == status
        })
        .await["session"]
            .clone()
    }

    /// Wait for yardd to refuse an op, and return its message.
    pub async fn wait_error(&mut self, from: usize, op: &str) -> String {
        let what = format!("an error for `{op}`");
        self.wait_after(from, &what, |m| m["op"] == "error" && m["for"] == op)
            .await["message"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    /// Every event of a session seen so far, in order, deltas included.
    pub fn events(&self, sid: &str) -> Vec<Value> {
        let mut evs: Vec<(u64, Value)> = self
            .log
            .iter()
            .filter(|m| is_event(m, sid))
            .map(|m| (m["seq"].as_u64().unwrap(), m["event"].clone()))
            .collect();
        evs.sort_by_key(|(s, _)| *s);
        evs.dedup_by_key(|(s, _)| *s);
        evs.into_iter().map(|(_, e)| e).collect()
    }

    pub fn event_types(&self, sid: &str) -> Vec<String> {
        self.events(sid)
            .iter()
            .map(|e| e["type"].as_str().unwrap_or("").to_string())
            .collect()
    }

    /// The statuses this session went through, repeats folded.
    pub fn statuses(&self, sid: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in &self.log {
            if m["op"] == "session" && m["session"]["id"] == sid {
                let s = m["session"]["status"].as_str().unwrap().to_string();
                if out.last() != Some(&s) {
                    out.push(s);
                }
            }
        }
        out
    }

    /// `attach.put` one file. `target` is `{ "host": .. }`, `{ "sessionId": .. }`
    /// or `{}` for this yardd. The stored attachment (with `hostId` when it
    /// came through the hub), or yardd's error.
    pub async fn upload(
        &mut self,
        name: &str,
        mime: &str,
        bytes: &[u8],
        target: Value,
    ) -> Result<Value, String> {
        let r = format!("r{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let mut msg = json!({
            "op": "attach.put",
            "ref": r,
            "name": name,
            "mime": mime,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        });
        if let (Some(m), Some(t)) = (msg.as_object_mut(), target.as_object()) {
            m.extend(t.clone());
        }
        let from = self.mark();
        self.send(msg).await;
        let reply = self
            .wait_after(from, "the reply to attach.put", |m| {
                (m["op"] == "attach.stored" || m["op"] == "error") && m["ref"] == r.as_str()
            })
            .await;
        if let Some(e) = reply["error"].as_str().or(reply["message"].as_str()) {
            return Err(e.to_string());
        }
        let mut a = reply["attachment"].clone();
        if !reply["hostId"].is_null() {
            a["hostId"] = reply["hostId"].clone();
        }
        Ok(a)
    }

    /// `attach.get`: the bytes of an attachment of a session, or yardd's error.
    pub async fn fetch(&mut self, sid: &str, id: &str) -> Result<Vec<u8>, String> {
        let from = self.mark();
        self.send(json!({ "op": "attach.get", "sessionId": sid, "id": id }))
            .await;
        let reply = self
            .wait_after(from, "the reply to attach.get", |m| {
                (m["op"] == "attach.data" && m["sessionId"] == sid && m["id"] == id)
                    || (m["op"] == "error" && m["for"] == "attach.get")
            })
            .await;
        if let Some(e) = reply["error"].as_str().or(reply["message"].as_str()) {
            return Err(e.to_string());
        }
        Ok(base64::engine::general_purpose::STANDARD
            .decode(reply["data"].as_str().unwrap())
            .unwrap())
    }

    /// `file.get`: the name and bytes of the file a link in the session names.
    pub async fn file(&mut self, sid: &str, path: &str) -> Result<(String, Vec<u8>), String> {
        let from = self.mark();
        self.send(json!({ "op": "file.get", "sessionId": sid, "path": path }))
            .await;
        let reply = self
            .wait_after(from, "the reply to file.get", |m| {
                (m["op"] == "file.data" && m["sessionId"] == sid && m["path"].is_string())
                    || (m["op"] == "error" && m["for"] == "file.get")
            })
            .await;
        if let Some(e) = reply["error"].as_str().or(reply["message"].as_str()) {
            return Err(e.to_string());
        }
        let data = base64::engine::general_purpose::STANDARD
            .decode(reply["data"].as_str().unwrap())
            .unwrap();
        Ok((reply["name"].as_str().unwrap().to_string(), data))
    }

    /// `session.open` and the history that comes back.
    pub async fn history(&mut self, sid: &str) -> Vec<Value> {
        let from = self.mark();
        self.send(json!({ "op": "session.open", "sessionId": sid }))
            .await;
        let what = format!("history of {sid}");
        self.wait_after(from, &what, |m| {
            m["op"] == "history" && m["sessionId"] == sid
        })
        .await["events"]
            .as_array()
            .unwrap()
            .clone()
    }
}

pub fn is_event(m: &Value, sid: &str) -> bool {
    m["op"] == "event" && m["sessionId"] == sid
}

pub fn types(events: &[Value]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e["type"].as_str().unwrap_or(""))
        .collect()
}

/// `git` with a fixed identity and no user or system config, so the user's
/// hooks and settings stay out of the test repos.
pub fn git(dir: &str, args: &[&str], home: &Path) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=Yard Test",
            "-c",
            "user.email=yard@test.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("could not run git");
    assert!(
        out.status.success(),
        "git {args:?} failed in {dir}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repo with one commit on `main` and a `feature` branch one commit ahead.
pub fn make_repo(sandbox: &Sandbox, name: &str) -> String {
    let dir = sandbox.folder(name);
    let home = sandbox.home();
    git(&dir, &["init", "-q"], &home);
    fs::write(Path::new(&dir).join("README.md"), "hello\n").unwrap();
    git(&dir, &["add", "."], &home);
    git(&dir, &["commit", "-q", "-m", "first"], &home);
    git(&dir, &["checkout", "-q", "-b", "feature"], &home);
    fs::write(Path::new(&dir).join("feature.txt"), "feature\n").unwrap();
    git(&dir, &["add", "."], &home);
    git(&dir, &["commit", "-q", "-m", "feature work"], &home);
    git(&dir, &["checkout", "-q", "main"], &home);
    dir
}

/// A session's saved status, read straight from yardd's database.
pub fn saved_status(sandbox: &Sandbox, sid: &str) -> Option<String> {
    saved_meta(sandbox, sid)?["status"]
        .as_str()
        .map(str::to_string)
}

/// A session's metadata as yardd last wrote it to disk.
pub fn saved_meta(sandbox: &Sandbox, sid: &str) -> Option<Value> {
    let conn = rusqlite::Connection::open_with_flags(
        sandbox.data().join("yard.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .ok()?;
    let meta: String = conn
        .query_row("SELECT meta FROM sessions WHERE id = ?1", [sid], |r| {
            r.get(0)
        })
        .ok()?;
    serde_json::from_str(&meta).ok()
}
