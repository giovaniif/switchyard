//! Resource guard: what each session's processes use, and a queue that holds
//! new sessions while the host is busy. Nothing running is ever stopped for
//! its size: a per-session memory cap was tried and removed (2026-11-15).
//! The only kill here is one the user asks for (`kill`), checked against the
//! session's process tree read at that moment.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, Signal, System, UpdateKind};

use crate::{pty, sessions, state::App};

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

struct Limits {
    min_free: u64,
    /// `YARD_FORCE_PRESSURE=1`, for testing the queue.
    forced: bool,
    /// For tests only (`YARD_TEST_IGNORE_LOAD=1`): the load of a busy test
    /// machine queues no session; the queue tests force pressure instead.
    ignore_load: bool,
}

fn gb_env(key: &str, default: f64) -> u64 {
    let gb = std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(default);
    (gb * GB) as u64
}

/// `8` or `0.2`: whole numbers without a decimal.
fn gb_text(bytes: u64) -> String {
    let gb = bytes as f64 / GB;
    if (gb - gb.round()).abs() < 0.05 {
        format!("{:.0}", gb.round())
    } else {
        format!("{gb:.1}")
    }
}

/// A Codex command between `item/started` and `item/completed`.
#[derive(Clone)]
struct Command {
    cwd: PathBuf,
    /// Seconds since the epoch, comparable with a process start time.
    since: u64,
}

/// One process of a session, as of the last sample.
#[derive(Clone)]
struct ProcRow {
    pid: u32,
    ppid: u32,
    name: String,
    cmd: String,
    /// Percent of one core.
    cpu: f32,
    mem: u64,
    /// Seconds since the epoch.
    start: u64,
    /// `agent`: what the engine runs. `terminal`: the session's shells.
    origin: &'static str,
    /// The engine's own process (the Claude sidecar or its CLI), which
    /// `kill` refuses: the session's Stop and Delete are the ways to end it.
    engine: bool,
}

#[derive(Default)]
struct Inner {
    /// Session -> its processes at the last sample, for `session.procs`.
    procs: HashMap<String, Vec<ProcRow>>,
    /// Claude session -> its sidecar.
    sidecars: HashMap<String, u32>,
    /// Each account's `codex app-server`, and each ACP agent (OpenCode, Grok)
    /// keyed `<engine>/<account>`. It cannot be split by session, but
    /// the commands it runs are its children and can.
    codex_servers: HashMap<String, u32>,
    /// Codex session -> running command item.
    commands: HashMap<String, HashMap<String, Command>>,
    /// Session id and why it waits, oldest first.
    queue: VecDeque<(String, String)>,
    /// From the last sample; None when the host is fine.
    pressure: Option<String>,
    /// When `procs` was last sampled, in ms.
    sampled_at: u64,
}

pub struct Guard {
    limits: Limits,
    inner: Mutex<Inner>,
}

impl Default for Guard {
    fn default() -> Self {
        Guard {
            limits: Limits {
                min_free: gb_env("YARD_MIN_FREE_GB", 2.0),
                forced: std::env::var("YARD_FORCE_PRESSURE").is_ok_and(|v| v == "1"),
                ignore_load: std::env::var("YARD_TEST_IGNORE_LOAD").is_ok_and(|v| v == "1"),
            },
            inner: Mutex::default(),
        }
    }
}

impl Guard {
    pub fn set_sidecar(&self, sid: &str, pid: u32) {
        self.inner.lock().unwrap().sidecars.insert(sid.to_string(), pid);
    }

    /// Only if it is still this sidecar: a resumed session may have a newer one.
    pub fn clear_sidecar(&self, sid: &str, pid: u32) {
        let mut inner = self.inner.lock().unwrap();
        if inner.sidecars.get(sid) == Some(&pid) {
            inner.sidecars.remove(sid);
        }
    }

    pub fn set_codex_server(&self, account: &str, pid: u32) {
        self.inner.lock().unwrap().codex_servers.insert(account.to_string(), pid);
    }

    pub fn clear_codex_server(&self, account: &str) {
        self.inner.lock().unwrap().codex_servers.remove(account);
    }

    pub fn command_started(&self, sid: &str, item: &str, cwd: &str) {
        let command = Command {
            cwd: std::fs::canonicalize(cwd).unwrap_or_else(|_| PathBuf::from(cwd)),
            since: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        let mut inner = self.inner.lock().unwrap();
        inner.commands.entry(sid.to_string()).or_default().insert(item.to_string(), command);
    }

    pub fn command_done(&self, sid: &str, item: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(items) = inner.commands.get_mut(sid) {
            items.remove(item);
            if items.is_empty() {
                inner.commands.remove(sid);
            }
        }
    }

    /// A finished turn has no commands left, whatever events were missed.
    pub fn turn_done(&self, sid: &str) {
        self.inner.lock().unwrap().commands.remove(sid);
    }

    /// The session is being deleted.
    pub fn forget(&self, sid: &str) {
        let mut inner = self.inner.lock().unwrap();
        inner.sidecars.remove(sid);
        inner.commands.remove(sid);
        inner.procs.remove(sid);
        inner.queue.retain(|(id, _)| id != sid);
    }

    /// The session's process tree at the last sample (at most 3 s old), for
    /// `session.procs`. The ages are worked out now.
    pub fn procs(&self, sid: &str) -> Vec<Value> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let inner = self.inner.lock().unwrap();
        inner
            .procs
            .get(sid)
            .into_iter()
            .flatten()
            .map(|p| {
                json!({
                    "pid": p.pid,
                    "ppid": p.ppid,
                    "name": p.name,
                    "cmd": p.cmd,
                    "cpu": p.cpu,
                    "memBytes": p.mem,
                    "ageSecs": now.saturating_sub(p.start),
                    "start": p.start,
                    "origin": p.origin,
                    "engine": p.engine,
                })
            })
            .collect()
    }

    /// When the processes were last sampled (ms), 0 before the first sample.
    pub fn sampled_at(&self) -> u64 {
        self.inner.lock().unwrap().sampled_at
    }

    /// A process a Claude session's agent left running (a background shell,
    /// a `Monitor`, a watcher a subagent started), as of the last sample:
    /// stopping the sidecar would kill it. Not the engine (sidecar and CLI),
    /// not its MCP servers, not the terminal's shells. A process the CLI runs
    /// is the agent's when it is a shell given a command (`sh -c …`, how
    /// Claude Code runs Bash), or below one; any other child of the CLI is an
    /// MCP server. Anything the sidecar runs itself is the agent's.
    pub fn agent_work(&self, sid: &str) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        let rows = inner.procs.get(sid)?;
        let sidecar = *inner.sidecars.get(sid)?;
        let row = |pid: u32| rows.iter().find(|r| r.pid == pid);
        rows.iter()
            .filter(|r| r.origin == "agent" && !r.engine)
            .find(|r| {
                // Walk up to the process right below the engine.
                let mut top = *r;
                while let Some(parent) = row(top.ppid).filter(|p| !p.engine && p.pid != top.pid) {
                    top = parent;
                }
                top.ppid == sidecar || is_shell_command(&top.cmd)
            })
            .map(|r| format!("{} (pid {})", r.cmd.chars().take(120).collect::<String>(), r.pid))
    }

    fn pressure(&self, inner: &Inner) -> Option<String> {
        if self.limits.forced {
            return Some("pressure is forced on (YARD_FORCE_PRESSURE=1)".to_string());
        }
        inner.pressure.clone()
    }

    /// Why a new session has to wait, or None to start it now. Sessions that
    /// are already waiting go first.
    pub fn admit(&self) -> Option<String> {
        let inner = self.inner.lock().unwrap();
        if let Some(reason) = self.pressure(&inner) {
            return Some(reason);
        }
        match inner.queue.len() {
            0 => None,
            1 => Some("1 queued session starts first".to_string()),
            n => Some(format!("{n} queued sessions start first")),
        }
    }

    pub fn enqueue(&self, sid: &str, reason: &str) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.queue.iter().any(|(id, _)| id == sid) {
            inner.queue.push_back((sid.to_string(), reason.to_string()));
        }
    }

    /// Remove the session from the queue. True if it was there, so exactly
    /// one caller gets to start it.
    pub fn take(&self, sid: &str) -> bool {
        let mut inner = self.inner.lock().unwrap();
        let before = inner.queue.len();
        inner.queue.retain(|(id, _)| id != sid);
        inner.queue.len() != before
    }
}

/// Every process below `root`, and `root` itself, children before parents.
fn tree(children: &HashMap<Pid, Vec<Pid>>, root: Pid, out: &mut Vec<Pid>, seen: &mut HashSet<Pid>) {
    if !seen.insert(root) {
        return;
    }
    if let Some(kids) = children.get(&root) {
        for &kid in kids {
            tree(children, kid, out, seen);
        }
    }
    out.push(root);
}

#[derive(Default)]
struct SessionUse {
    cpu: f32,
    mem: u64,
}

/// Called by the host sampler after it refreshed `sys` (memory and processes).
/// Measures each session, starts one queued session if the host has room,
/// records pressure episodes (`pressure`), and returns the fields it adds to
/// the `host` message.
pub fn sample(app: &Arc<App>, sys: &System, readings: &crate::pressure::Readings, injected: &crate::pressure::Injected) -> Value {
    let guard = &app.guard;
    let limits = &guard.limits;
    let (load, cores, mem_free) = (readings.load, readings.cores, readings.mem_free);

    let mut reasons = Vec::new();
    if load > cores as f64 && !limits.ignore_load {
        reasons.push(format!("load {load:.1} is above {cores} cores"));
    }
    if mem_free < limits.min_free {
        reasons.push(format!(
            "{:.2} GB of memory free, needs {} GB",
            mem_free as f64 / GB,
            gb_text(limits.min_free)
        ));
    }

    guard.inner.lock().unwrap().pressure = (!reasons.is_empty()).then(|| reasons.join("; "));

    let (listed, usage, _) = attribute(app, sys);
    // The same load and memory rules as the queue's above, so an episode is
    // open exactly while the host reads "under pressure"; swap is only noted.
    let kinds = crate::pressure::reasons(readings, limits.forced, limits.ignore_load, limits.min_free);
    let listed_pids: HashSet<u32> = listed.values().flatten().map(|r| r.pid).collect();
    let used: HashMap<String, (f32, u64)> = usage.iter().map(|(sid, u)| (sid.clone(), (u.cpu, u.mem))).collect();
    let episodes = crate::pressure::observe(app, sys, readings, kinds, &used, &listed_pids, injected);
    {
        let mut inner = guard.inner.lock().unwrap();
        inner.procs = listed;
        inner.sampled_at = crate::state::now_ms();
    }

    // One per sample, so a burst of starts cannot push the host straight
    // back under pressure.
    let next = {
        let mut inner = guard.inner.lock().unwrap();
        if guard.pressure(&inner).is_none() {
            inner.queue.pop_front().map(|(sid, _)| sid)
        } else {
            None
        }
    };
    if let Some(sid) = next {
        let app = app.clone();
        tokio::spawn(async move { sessions::start_queued(&app, &sid).await });
    }

    let mut rows: Vec<(String, SessionUse)> = usage.into_iter().collect();
    rows.sort_by(|a, b| b.1.mem.cmp(&a.1.mem));
    let rows: Vec<Value> = rows
        .iter()
        .map(|(sid, u)| json!({ "id": sid, "cpuCores": u.cpu / 100.0, "memBytes": u.mem }))
        .collect();
    let inner = guard.inner.lock().unwrap();
    let queued: Vec<Value> = inner
        .queue
        .iter()
        .map(|(sid, reason)| json!({ "id": sid, "reason": reason }))
        .collect();
    let mut out = json!({
        "sessions": rows,
        "queued": queued,
        "pressure": guard.pressure(&inner),
        "limits": {
            "minFreeBytes": limits.min_free,
            "forced": limits.forced,
        },
    });
    if let (Some(out), Some(extra)) = (out.as_object_mut(), episodes.as_object()) {
        out.extend(extra.clone());
    }
    out
}

/// Each session's processes in `sys`, what they use, and the engines' own
/// pids: Claude sidecars and their CLIs, and the shared Codex app-servers and
/// ACP agents.
fn attribute(app: &Arc<App>, sys: &System) -> (HashMap<String, Vec<ProcRow>>, HashMap<String, SessionUse>, HashSet<u32>) {
    let (sidecars, codex_servers, commands) = {
        let inner = app.guard.inner.lock().unwrap();
        (inner.sidecars.clone(), inner.codex_servers.values().copied().collect::<Vec<u32>>(), inner.commands.clone())
    };

    let procs = sys.processes();
    let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
    for (pid, p) in procs {
        // On Linux sysinfo also lists every thread, with its process as the
        // parent and the process's whole memory as its own. Counted, a CLI
        // with 30 threads looked 30 times its size; killed, a thread id takes
        // the whole process down, which is how the cap killed Claude Code.
        if p.thread_kind().is_some() {
            continue;
        }
        if let Some(parent) = p.parent() {
            children.entry(parent).or_default().push(*pid);
        }
    }

    // A sidecar's own child is the Claude CLI the SDK starts; the agent's
    // commands run below that. The CLI is told apart by name, so a process
    // the sidecar runs directly (the scripted sidecar's `sleep`) stays the
    // session's to stop.
    let mut engines: HashSet<u32> = codex_servers.iter().copied().collect();
    for pid in sidecars.values() {
        engines.insert(*pid);
        for kid in children.get(&Pid::from_u32(*pid)).into_iter().flatten() {
            let Some(p) = procs.get(kid) else { continue };
            if p.name().contains("claude") || p.cmd().iter().any(|arg| arg.contains("claude")) {
                engines.insert(kid.as_u32());
            }
        }
    }

    let mut usage: HashMap<String, SessionUse> = HashMap::new();
    let mut listed: HashMap<String, Vec<ProcRow>> = HashMap::new();
    let mut add = |sid: &str, root: u32, origin: &'static str| {
        let u = usage.entry(sid.to_string()).or_default();
        let rows = listed.entry(sid.to_string()).or_default();
        let mut pids = Vec::new();
        tree(&children, Pid::from_u32(root), &mut pids, &mut HashSet::new());
        // Children come first in `pids`; the list reads better parent first,
        // and the UI orders it anyway.
        for pid in pids {
            let Some(p) = procs.get(&pid) else { continue };
            u.cpu += p.cpu_usage();
            u.mem += p.memory();
            if rows.iter().any(|r| r.pid == pid.as_u32()) {
                continue;
            }
            let cmd = p.cmd().join(" ");
            rows.push(ProcRow {
                pid: pid.as_u32(),
                ppid: p.parent().map_or(0, |pp| pp.as_u32()),
                name: p.name().to_string(),
                cmd: if cmd.is_empty() { p.name().to_string() } else { cmd.chars().take(400).collect() },
                cpu: p.cpu_usage(),
                mem: p.memory(),
                start: p.start_time(),
                origin,
                engine: engines.contains(&pid.as_u32()),
            });
        }
    };
    // The whole sidecar tree: the sidecar, the CLI and what it runs.
    for (sid, pid) in &sidecars {
        add(sid, *pid, "agent");
    }
    // Codex reports an id of its own as `processId`, not an OS pid. So each
    // child of the app-server goes to the session with a running command in
    // that folder (or above it, for a shell that ran `cd`) that started no
    // later than the process did, the latest when several match. Helpers the
    // server keeps for itself match no command. Two sessions running
    // commands in one folder at the same moment can be mixed up.
    for server in codex_servers {
        for child in children.get(&Pid::from_u32(server)).into_iter().flatten() {
            let Some(p) = procs.get(child) else { continue };
            let Some(cwd) = p.cwd() else { continue };
            let cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
            let owner = commands
                .iter()
                .flat_map(|(sid, items)| items.values().map(move |c| (sid, c)))
                .filter(|(_, c)| cwd.starts_with(&c.cwd) && c.since <= p.start_time() + 2)
                .max_by_key(|(_, c)| c.since)
                .map(|(sid, _)| sid.clone());
            if let Some(sid) = owner {
                add(&sid, child.as_u32(), "agent");
            }
        }
    }
    for (sid, pid) in pty::shell_pids(app) {
        add(&sid, pid, "terminal");
    }
    // The closure holds `usage` and `listed` until here.
    (listed, usage, engines)
}

/// `bash -c '…'`, `/bin/zsh -c -l …`: a shell running a command line, as
/// Claude Code runs every Bash call (an MCP server that is a shell script
/// runs as `bash script.sh`, without `-c`).
fn is_shell_command(cmd: &str) -> bool {
    let mut words = cmd.split_whitespace();
    let shell = words.next().and_then(|w| w.rsplit('/').next()).unwrap_or("");
    ["sh", "bash", "zsh", "dash", "fish"].contains(&shell) && words.any(|w| w.starts_with('-') && !w.starts_with("--") && w.contains('c'))
}

/// The signal `session.kill` sends.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sig {
    Term,
    Kill,
}

impl Sig {
    pub fn parse(name: &str) -> Option<Sig> {
        match name {
            "term" => Some(Sig::Term),
            "kill" => Some(Sig::Kill),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Sig::Term => "term",
            Sig::Kill => "kill",
        }
    }

    /// How long the process gets to end before the reply says it is still
    /// running. SIGTERM gives it time to clean up; SIGKILL cannot be caught.
    fn grace(self) -> Duration {
        match self {
            Sig::Term => Duration::from_secs(3),
            Sig::Kill => Duration::from_secs(2),
        }
    }
}

/// What `session.kill` may signal: `pid`, and with `tree` every process below
/// it in the session's list, children before parents, each with its start
/// time. `Err` says why the request is refused.
///
/// Only a process the session's tree lists right now qualifies, and never
/// pid 0 or 1, yardd itself, an engine's own process (a Claude sidecar or its
/// CLI, a Codex app-server or ACP agent, which other sessions may share), or
/// a process another session lists too. A `start` that differs from the
/// listed one means the pid was reused since the UI saw it.
fn targets(
    listed: &HashMap<String, Vec<ProcRow>>,
    engines: &HashSet<u32>,
    own: u32,
    sid: &str,
    pid: u32,
    start: Option<u64>,
    tree: bool,
) -> Result<Vec<(u32, u64)>, String> {
    if pid <= 1 {
        return Err(format!("pid {pid} belongs to the system, not to this session"));
    }
    if pid == own {
        return Err(format!("pid {pid} is Switchyard's own daemon"));
    }
    let engine = || "it is the session's agent, which may serve other sessions too. Use Stop, or delete the session".to_string();
    if engines.contains(&pid) {
        return Err(engine());
    }
    let rows: &[ProcRow] = listed.get(sid).map(Vec::as_slice).unwrap_or(&[]);
    let Some(row) = rows.iter().find(|r| r.pid == pid) else {
        return Err(format!("pid {pid} is not one of this session's processes (it may have ended)"));
    };
    if row.engine {
        return Err(engine());
    }
    if start.is_some_and(|s| s != row.start) {
        return Err(format!("pid {pid} is now a different process"));
    }

    // Children before parents, so a parent that waits on its children sees
    // them go rather than restarting them; all are signalled in one pass.
    fn below(rows: &[ProcRow], row: &ProcRow, out: &mut Vec<(u32, u64)>, seen: &mut HashSet<u32>) {
        if !seen.insert(row.pid) {
            return;
        }
        for kid in rows.iter().filter(|r| r.ppid == row.pid && r.pid != row.pid) {
            below(rows, kid, out, seen);
        }
        out.push((row.pid, row.start));
    }
    let mut picked = Vec::new();
    if tree {
        below(rows, row, &mut picked, &mut HashSet::new());
    } else {
        picked.push((row.pid, row.start));
    }
    for (p, _) in &picked {
        if engines.contains(p) || rows.iter().any(|r| r.pid == *p && r.engine) {
            return Err(engine());
        }
        if listed.iter().any(|(other, rows)| other != sid && rows.iter().any(|r| r.pid == *p)) {
            return Err(format!("pid {p} is shared with another session"));
        }
    }
    Ok(picked)
}

/// Still there as the same process, and not a zombie waiting to be reaped.
fn alive(sys: &mut System, pid: u32, start: u64) -> bool {
    let pid = Pid::from_u32(pid);
    sys.refresh_process_specifics(pid, ProcessRefreshKind::new())
        && sys
            .process(pid)
            .is_some_and(|p| p.start_time() == start && !matches!(p.status(), ProcessStatus::Zombie | ProcessStatus::Dead))
}

/// `session.kill`: read the process table now, check the request against the
/// session's tree in it (`targets`), signal from that same read, then wait a
/// moment to tell whether the processes ended. Answers `session.killed` with
/// `result` `killed`, `running` (still there: SIGKILL is the next step) or
/// `refused` with a `reason`.
pub async fn kill(app: &Arc<App>, sid: &str, pid: u32, start: Option<u64>, sig: Sig, tree: bool) -> Value {
    let reply = |result: &str, reason: Option<String>, running: &[u32]| {
        let mut out = json!({
            "op": "session.killed",
            "sessionId": sid,
            "pid": pid,
            "signal": sig.name(),
            "tree": tree,
            "result": result,
            "running": running,
        });
        if let Some(reason) = reason {
            out["reason"] = json!(reason);
        }
        out
    };

    let checked = {
        let app = app.clone();
        let sid = sid.to_string();
        tokio::task::spawn_blocking(move || {
            let mut sys = System::new();
            sys.refresh_processes_specifics(
                ProcessRefreshKind::new().with_cmd(UpdateKind::Always).with_cwd(UpdateKind::Always),
            );
            let (listed, _, engines) = attribute(&app, &sys);
            let picked = targets(&listed, &engines, std::process::id(), &sid, pid, start, tree)?;
            let signal = match sig {
                Sig::Term => Signal::Term,
                Sig::Kill => Signal::Kill,
            };
            let mut sent = 0;
            for (p, st) in &picked {
                let Some(proc_) = sys.process(Pid::from_u32(*p)) else { continue };
                if proc_.start_time() != *st {
                    continue;
                }
                if proc_.kill_with(signal).unwrap_or(false) {
                    sent += 1;
                }
            }
            Ok::<_, String>((sys, picked, sent))
        })
        .await
    };
    let (mut sys, picked, sent) = match checked {
        Ok(Ok(found)) => found,
        Ok(Err(reason)) => return reply("refused", Some(reason), &[]),
        Err(e) => return reply("refused", Some(format!("the process list could not be read: {e}")), &[]),
    };
    let pids: Vec<u32> = picked.iter().map(|(p, _)| *p).collect();
    eprintln!("yardd: session {sid}: SIG{} to {pids:?} ({sent} sent)", sig.name().to_uppercase());

    let deadline = Instant::now() + sig.grace();
    let running = loop {
        let running: Vec<u32> = picked.iter().filter(|(p, st)| alive(&mut sys, *p, *st)).map(|(p, _)| *p).collect();
        if running.is_empty() || Instant::now() >= deadline {
            break running;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // The list the UI refreshes from is the last sample, up to 3 s old: drop
    // what ended so the row goes at once.
    if let Some(rows) = app.guard.inner.lock().unwrap().procs.get_mut(sid) {
        rows.retain(|r| !pids.contains(&r.pid) || running.contains(&r.pid));
    }
    if running.is_empty() {
        reply("killed", None, &running)
    } else if sent == 0 {
        reply("refused", Some("the system refused the signal (the process may belong to another user)".to_string()), &running)
    } else {
        reply("running", None, &running)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pid: u32, ppid: u32, start: u64, engine: bool) -> ProcRow {
        ProcRow { pid, ppid, name: "p".into(), cmd: "p".into(), cpu: 0.0, mem: 0, start, origin: "agent", engine }
    }

    /// Sidecar 10 (engine) -> CLI 11 (engine) -> shell 12 -> 13, 14; terminal 20.
    fn fixture() -> (HashMap<String, Vec<ProcRow>>, HashSet<u32>) {
        let mut listed = HashMap::new();
        listed.insert(
            "s1".to_string(),
            vec![row(10, 1, 100, true), row(11, 10, 101, true), row(12, 11, 102, false), row(13, 12, 103, false), row(14, 12, 104, false), row(20, 5, 200, false)],
        );
        listed.insert("s2".to_string(), vec![row(30, 1, 300, false)]);
        (listed, [10, 11, 99].into_iter().collect())
    }

    #[test]
    fn an_agent_shell_is_work_and_an_mcp_server_is_not() {
        let guard = Guard::default();
        let with_cmd = |pid, ppid, cmd: &str, engine| ProcRow { cmd: cmd.into(), ..row(pid, ppid, 100, engine) };
        let sidecar = with_cmd(10, 1, "bun run main.ts", true);
        let cli = with_cmd(11, 10, "claude --output-format stream-json", true);
        let mcp = with_cmd(12, 11, "/bin/bash /tmp/probe-mcp.sh", false);
        let mcp_kid = with_cmd(13, 12, "bun mcp-chart", false);
        let set = |rows: Vec<ProcRow>| {
            let mut inner = guard.inner.lock().unwrap();
            inner.sidecars.insert("s1".into(), 10);
            inner.procs.insert("s1".into(), rows);
        };
        set(vec![sidecar.clone(), cli.clone(), mcp.clone(), mcp_kid.clone()]);
        assert_eq!(guard.agent_work("s1"), None, "only the engine and an MCP server");
        // A background Bash call: Claude Code's shell, and what it runs.
        set(vec![sidecar.clone(), cli.clone(), mcp.clone(), with_cmd(14, 11, "/bin/bash -c -l source snapshot.sh && eval 'gh pr checks --watch'", false), with_cmd(15, 14, "gh pr checks --watch", false)]);
        assert!(guard.agent_work("s1").is_some_and(|w| w.contains("pid 14") || w.contains("pid 15")));
        // Run by the sidecar itself.
        set(vec![sidecar.clone(), cli.clone(), with_cmd(16, 10, "sleep 30", false)]);
        assert!(guard.agent_work("s1").is_some());
        // The terminal's shells are not the engine's to keep.
        set(vec![sidecar, cli, ProcRow { origin: "terminal", ..with_cmd(17, 1, "bash -c x", false) }]);
        assert_eq!(guard.agent_work("s1"), None);
        assert!(is_shell_command("zsh -lc 'echo'"));
        assert!(!is_shell_command("bash script.sh --color"));
    }

    #[test]
    fn a_listed_process_is_signalled_alone_unless_its_tree_is_asked_for() {
        let (listed, engines) = fixture();
        assert_eq!(targets(&listed, &engines, 7, "s1", 12, Some(102), false), Ok(vec![(12, 102)]));
        assert_eq!(targets(&listed, &engines, 7, "s1", 12, None, true), Ok(vec![(13, 103), (14, 104), (12, 102)]));
        assert_eq!(targets(&listed, &engines, 7, "s1", 20, None, true), Ok(vec![(20, 200)]));
    }

    #[test]
    fn refuses_what_is_not_the_sessions_to_kill() {
        let (mut listed, engines) = fixture();
        let refused = |listed: &HashMap<String, Vec<ProcRow>>, pid: u32, start: Option<u64>| {
            targets(listed, &engines, 7, "s1", pid, start, false).expect_err("refused")
        };
        assert!(refused(&listed, 1, None).contains("system"));
        assert!(refused(&listed, 0, None).contains("system"));
        assert!(refused(&listed, 7, None).contains("own daemon"));
        assert!(refused(&listed, 10, None).contains("agent"));
        assert!(refused(&listed, 11, None).contains("agent"));
        // A shared app-server is never in a session's list, and still refused by name.
        assert!(refused(&listed, 99, None).contains("agent"));
        // Another session's process.
        assert!(refused(&listed, 30, None).contains("not one of this session's"));
        // The pid now names a process that started later.
        assert!(refused(&listed, 12, Some(999)).contains("different process"));
        // Listed by two sessions.
        listed.get_mut("s2").unwrap().push(row(13, 12, 103, false));
        assert!(refused(&listed, 13, None).contains("shared"));
        assert!(targets(&listed, &engines, 7, "s1", 12, None, true).unwrap_err().contains("shared"));
    }
}
