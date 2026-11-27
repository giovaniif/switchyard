//! Pressure episodes: each stretch of time the host was overloaded, kept so
//! that afterwards someone can say what happened. An episode opens on the
//! first sample under pressure (1-minute load above the cores, free memory
//! under the guard's floor, or swap nearly full), keeps its peaks and who
//! used the most at its worst sample (sessions, and processes that belong to
//! no session, which are often the culprit), and closes once the host has
//! been calm for a minute. Episodes are saved in `pressure.json` in the data
//! dir: a week of them, at most `KEEP` (the open one too, so a box that hard
//! resets mid-episode still has it).
//!
//! Nothing here stops anything. The windows are told (the `host` message
//! carries the open episode and the recent ones, plus the running turns that
//! went quiet meanwhile) and decide with the owner.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::System;

use crate::state::{now_ms, App};

const KEEP_MS: u64 = 7 * 24 * 3600 * 1000;
const KEEP: usize = 100;
/// Closed episodes every `host` message carries; `host.pressure` pages the rest.
const RECENT: usize = 5;
/// Contributors kept per ranking: the top by CPU and the top by memory.
const TOP: usize = 3;
/// Swap at least this full is pressure: the host is paging hard.
const SWAP_FULL: f64 = 0.9;
/// Below both, a session or process is no contributor at all: a calm
/// helper would only crowd the list.
const MIN_CORES: f64 = 0.05;
const MIN_MEM: u64 = 64 * 1024 * 1024;
/// How often an open episode's new peaks are written out.
const SAVE_EVERY_MS: u64 = 30_000;

/// What the sampler read this round, after any test readings.
pub struct Readings {
    pub load: f64,
    pub cores: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub mem_free: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    /// The readings file set the load, or the swap: counted even where the
    /// test machine's own are ignored (`YARD_TEST_IGNORE_LOAD`).
    pub load_injected: bool,
    pub swap_injected: bool,
}

/// One process's or session's use in the readings file.
#[derive(Clone, Copy, Deserialize)]
pub struct Use {
    /// Percent of one core, as sysinfo counts.
    #[serde(default)]
    pub cpu: f32,
    #[serde(default)]
    pub mem: u64,
}

/// For tests only: `YARD_TEST_READINGS` names a JSON file read every sample.
/// Each field it sets replaces what the host read; `sessions` and `procs`
/// (by session id and pid) replace what those used. No file reads nothing.
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Injected {
    pub load: Option<f64>,
    pub mem_free: Option<u64>,
    pub swap_used: Option<u64>,
    pub swap_total: Option<u64>,
    #[serde(default)]
    pub sessions: HashMap<String, Use>,
    #[serde(default)]
    pub procs: HashMap<String, Use>,
}

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Peak {
    pub load: f64,
    pub cores: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SessionTop {
    pub id: String,
    pub title: String,
    pub engine: String,
    /// The automation's name, for one of its runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub automation: Option<String>,
    pub cpu_cores: f64,
    pub mem_bytes: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ProcTop {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
    pub cpu_cores: f64,
    pub mem_bytes: u64,
}

/// Who used the most at the episode's worst sample.
#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Top {
    pub at: u64,
    pub load: f64,
    pub mem_used: u64,
    pub swap_used: u64,
    pub sessions: Vec<SessionTop>,
    pub procs: Vec<ProcTop>,
    /// How bad that sample was (`score`); a worse one replaces it.
    #[serde(default)]
    pub score: f64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Episode {
    pub id: String,
    pub start: u64,
    /// None while it lasts.
    pub end: Option<u64>,
    /// The last sample under pressure: the end, should yardd stop mid-episode.
    pub last_seen: u64,
    /// Every kind of pressure seen during it: `load`, `memory`, `swap`, `forced`.
    pub reasons: Vec<String>,
    pub peak: Peak,
    pub top: Top,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub notified: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    episodes: Vec<Episode>,
}

#[derive(Default)]
struct Log {
    /// Oldest first.
    closed: Vec<Episode>,
    open: Option<Episode>,
    /// The first calm sample since the last one under pressure.
    calm_since: Option<u64>,
    saved_at: u64,
}

pub struct Pressure {
    file: PathBuf,
    readings: Option<PathBuf>,
    calm_ms: u64,
    stall_ms: u64,
    notify_ms: u64,
    log: Mutex<Log>,
}

fn secs_env(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(default) * 1000
}

/// Older than a week, or beyond the cap, oldest first.
fn prune(closed: &mut Vec<Episode>, now: u64) {
    closed.retain(|e| now.saturating_sub(e.end.unwrap_or(e.last_seen)) <= KEEP_MS);
    if closed.len() > KEEP {
        let extra = closed.len() - KEEP;
        closed.drain(..extra);
    }
}

impl Pressure {
    pub fn load(dir: &Path) -> Pressure {
        let file = dir.join("pressure.json");
        let saved: Saved = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut closed = saved.episodes;
        // One that was open when yardd stopped ended with its last sample.
        for e in &mut closed {
            if e.end.is_none() {
                e.end = Some(e.last_seen);
            }
        }
        closed.sort_by_key(|e| e.start);
        prune(&mut closed, now_ms());
        Pressure {
            file,
            readings: std::env::var("YARD_TEST_READINGS").ok().filter(|p| !p.is_empty()).map(PathBuf::from),
            calm_ms: secs_env("YARD_TEST_PRESSURE_CALM_SECS", 60),
            stall_ms: secs_env("YARD_TEST_STALL_SECS", 300),
            notify_ms: secs_env("YARD_TEST_PRESSURE_NOTIFY_SECS", 120),
            log: Mutex::new(Log { closed, ..Log::default() }),
        }
    }

    /// The test readings, or nothing outside tests.
    pub fn injected(&self) -> Injected {
        let Some(path) = &self.readings else { return Injected::default() };
        std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    fn save(&self, log: &mut Log) {
        let mut episodes = log.closed.clone();
        episodes.extend(log.open.clone());
        let tmp = self.file.with_extension("json.tmp");
        if std::fs::write(&tmp, json!({ "episodes": episodes }).to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &self.file);
        }
        log.saved_at = now_ms();
    }

    /// Closed episodes, newest first, that started before `before`, and
    /// whether older ones remain.
    pub fn page(&self, before: Option<u64>, limit: usize) -> (Vec<Episode>, bool) {
        let log = self.log.lock().unwrap();
        let older: Vec<&Episode> = log.closed.iter().rev().filter(|e| before.is_none_or(|b| e.start < b)).collect();
        let more = older.len() > limit;
        (older.into_iter().take(limit).cloned().collect(), more)
    }
}

/// How bad a sample is: load per core plus memory and swap used per GB of RAM.
fn score(r: &Readings) -> f64 {
    r.load / r.cores.max(1) as f64 + (r.mem_used + r.swap_used) as f64 / r.mem_total.max(1) as f64
}

/// The top `TOP` by CPU and the top `TOP` by memory, once each, largest
/// memory first; nothing that uses next to nothing.
fn pick<T: Clone>(rows: &[T], cpu: impl Fn(&T) -> f64, mem: impl Fn(&T) -> u64, same: impl Fn(&T, &T) -> bool) -> Vec<T> {
    let mut by_cpu: Vec<&T> = rows.iter().filter(|r| cpu(r) >= MIN_CORES).collect();
    by_cpu.sort_by(|a, b| cpu(b).total_cmp(&cpu(a)));
    let mut by_mem: Vec<&T> = rows.iter().filter(|r| mem(r) >= MIN_MEM).collect();
    by_mem.sort_by_key(|r| std::cmp::Reverse(mem(r)));
    let mut out: Vec<T> = Vec::new();
    for r in by_cpu.into_iter().take(TOP).chain(by_mem.into_iter().take(TOP)) {
        if !out.iter().any(|o| same(o, r)) {
            out.push(r.clone());
        }
    }
    out.sort_by_key(|r| std::cmp::Reverse(mem(r)));
    out
}

/// Who uses the most now: sessions (from the guard's attribution) and the
/// processes that belong to none.
fn contributors(app: &Arc<App>, sys: &System, usage: &HashMap<String, (f32, u64)>, listed: &HashSet<u32>, injected: &Injected) -> (Vec<SessionTop>, Vec<ProcTop>) {
    let mut used: HashMap<&str, (f32, u64)> = usage.iter().map(|(sid, u)| (sid.as_str(), *u)).collect();
    for (sid, u) in &injected.sessions {
        used.insert(sid.as_str(), (u.cpu, u.mem));
    }
    let sessions: Vec<SessionTop> = {
        let map = app.sessions.lock().unwrap();
        used.iter()
            .filter(|(sid, _)| !crate::private::internal(sid))
            .filter_map(|(sid, (cpu, mem))| {
                let meta = &map.get(*sid)?.meta;
                Some(SessionTop {
                    id: sid.to_string(),
                    title: meta.title.clone(),
                    engine: meta.engine.clone(),
                    automation: meta.automation.clone(),
                    cpu_cores: *cpu as f64 / 100.0,
                    mem_bytes: *mem,
                })
            })
            .collect()
    };
    let mut sessions = pick(&sessions, |s| s.cpu_cores, |s| s.mem_bytes, |a, b| a.id == b.id);
    // The automation's id becomes its name; a deleted one says nothing.
    for s in &mut sessions {
        s.automation = s.automation.take().and_then(|id| app.automations.get(&id)).map(|a| a.name);
    }

    let procs: Vec<ProcTop> = sys
        .processes()
        .iter()
        .filter(|(pid, p)| p.thread_kind().is_none() && !listed.contains(&pid.as_u32()))
        .filter_map(|(pid, p)| {
            let (cpu, mem) = injected.procs.get(&pid.as_u32().to_string()).map_or((p.cpu_usage(), p.memory()), |u| (u.cpu, u.mem));
            if cpu <= 0.0 && mem == 0 {
                return None;
            }
            let cmd = p.cmd().join(" ");
            Some(ProcTop {
                pid: pid.as_u32(),
                name: p.name().to_string(),
                cmd: if cmd.is_empty() { p.name().to_string() } else { cmd.chars().take(200).collect() },
                cpu_cores: cpu as f64 / 100.0,
                mem_bytes: mem,
            })
        })
        .collect();
    (sessions, pick(&procs, |p| p.cpu_cores, |p| p.mem_bytes, |a, b| a.pid == b.pid))
}

fn size(bytes: u64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    if (bytes as f64) < GB {
        format!("{} MB", bytes / (1024 * 1024))
    } else {
        format!("{:.1} GB", bytes as f64 / GB)
    }
}

/// What the OS notification says, and the session it opens when the worst
/// contributor is one. By CPU when load is the trouble, by memory otherwise.
fn notice(name: &str, ep: &Episode) -> Value {
    let by_cpu = ep.reasons.iter().any(|r| r == "load");
    let session = ep.top.sessions.iter().max_by(|a, b| if by_cpu { a.cpu_cores.total_cmp(&b.cpu_cores) } else { a.mem_bytes.cmp(&b.mem_bytes) });
    let proc_ = ep.top.procs.iter().max_by(|a, b| if by_cpu { a.cpu_cores.total_cmp(&b.cpu_cores) } else { a.mem_bytes.cmp(&b.mem_bytes) });
    let key = |cpu: f64, mem: u64| if by_cpu { cpu } else { mem as f64 };
    let session_wins = match (session, proc_) {
        (Some(s), Some(p)) => key(s.cpu_cores, s.mem_bytes) >= key(p.cpu_cores, p.mem_bytes),
        (Some(_), None) => true,
        _ => false,
    };
    let mut body = format!("Load {:.0} on {} cores", ep.peak.load, ep.peak.cores);
    let mut note = json!({ "op": "notify", "title": format!("{name} is overloaded") });
    if session_wins {
        let s = session.unwrap();
        body.push_str(&format!(" · mostly {} ({})", s.title, size(s.mem_bytes)));
        note["sessionId"] = json!(s.id);
    } else if let Some(p) = proc_ {
        body.push_str(&format!(" · mostly {} (pid {}, {})", p.name, p.pid, size(p.mem_bytes)));
    }
    note["body"] = json!(body);
    note
}

/// Called by the guard on every host sample with this round's readings, the
/// kinds of pressure it found, what each session used and every pid a
/// session lists. Opens, grows or closes the episode and returns the fields
/// it adds to the `host` message.
pub fn observe(app: &Arc<App>, sys: &System, r: &Readings, reasons: Vec<&'static str>, usage: &HashMap<String, (f32, u64)>, listed: &HashSet<u32>, injected: &Injected) -> Value {
    let p = &app.pressure;
    let now = now_ms();
    let under = !reasons.is_empty();
    let top = under.then(|| {
        let (sessions, procs) = contributors(app, sys, usage, listed, injected);
        Top { at: now, load: r.load, mem_used: r.mem_used, swap_used: r.swap_used, sessions, procs, score: score(r) }
    });

    let mut announce = None;
    let (open, recent) = {
        let mut log = p.log.lock().unwrap();
        if let Some(top) = top {
            log.calm_since = None;
            let peak = Peak { load: r.load, cores: r.cores, mem_used: r.mem_used, mem_total: r.mem_total, swap_used: r.swap_used, swap_total: r.swap_total };
            let opened = log.open.is_none();
            let ep = log.open.get_or_insert_with(|| Episode {
                id: now.to_string(),
                start: now,
                end: None,
                last_seen: now,
                reasons: vec![],
                peak: peak.clone(),
                top: top.clone(),
                notified: false,
            });
            ep.last_seen = now;
            for reason in &reasons {
                if !ep.reasons.iter().any(|r| r == reason) {
                    ep.reasons.push(reason.to_string());
                }
            }
            let mut changed = opened;
            if top.score > ep.top.score {
                ep.top = top;
                changed = true;
            }
            let peaks = &mut ep.peak;
            for (slot, now_value) in [(&mut peaks.mem_used, peak.mem_used), (&mut peaks.swap_used, peak.swap_used)] {
                if now_value > *slot {
                    *slot = now_value;
                    changed = true;
                }
            }
            if peak.load > peaks.load {
                peaks.load = peak.load;
                changed = true;
            }
            peaks.mem_total = peak.mem_total;
            peaks.swap_total = peaks.swap_total.max(peak.swap_total);
            if !ep.notified && now.saturating_sub(ep.start) >= p.notify_ms {
                ep.notified = true;
                announce = Some(notice(crate::hosts::local_name(), ep));
                changed = true;
            }
            if opened || announce.is_some() || (changed && now.saturating_sub(log.saved_at) >= SAVE_EVERY_MS) {
                p.save(&mut log);
            }
        } else if log.open.is_some() {
            let since = *log.calm_since.get_or_insert(now);
            if now.saturating_sub(since) >= p.calm_ms {
                let mut ep = log.open.take().unwrap();
                ep.end = Some(ep.last_seen);
                log.closed.push(ep);
                log.calm_since = None;
                prune(&mut log.closed, now);
                p.save(&mut log);
            }
        }
        let recent: Vec<Episode> = log.closed.iter().rev().take(RECENT).cloned().collect();
        (log.open.clone(), recent)
    };
    if let Some(note) = announce {
        app.broadcast(note);
    }

    // A turn that is running but has said nothing for a while, while the
    // host is overloaded: the windows say why it is slow. Not failed.
    let mut stalled: Vec<Value> = Vec::new();
    if under {
        let map = app.sessions.lock().unwrap();
        for (sid, s) in map.iter() {
            let quiet = now.saturating_sub(s.event_at);
            if s.meta.status == "running" && quiet >= p.stall_ms && !crate::private::internal(sid) {
                stalled.push(json!({ "id": sid, "quietSecs": quiet / 1000 }));
            }
        }
    }
    json!({
        "swapUsed": r.swap_used,
        "swapTotal": r.swap_total,
        "episode": open,
        "episodes": recent,
        "stalled": stalled,
    })
}

/// Which kinds of pressure this sample shows. The test machine's own load
/// and swap count for nothing under `YARD_TEST_IGNORE_LOAD`; test readings do.
pub fn reasons(r: &Readings, forced: bool, ignore_real: bool, min_free: u64) -> Vec<&'static str> {
    let mut out = Vec::new();
    if forced {
        out.push("forced");
    }
    if r.load > r.cores as f64 && (!ignore_real || r.load_injected) {
        out.push("load");
    }
    if r.mem_free < min_free {
        out.push("memory");
    }
    if r.swap_total > 0 && r.swap_used as f64 >= SWAP_FULL * r.swap_total as f64 && (!ignore_real || r.swap_injected) {
        out.push("swap");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn episode(start: u64, end: Option<u64>) -> Episode {
        Episode { id: start.to_string(), start, end, last_seen: start + 5, reasons: vec!["load".into()], peak: Peak::default(), top: Top::default(), notified: false }
    }

    #[test]
    fn a_week_and_a_hundred_at_most_and_an_open_one_ends_at_its_last_sample() {
        let dir = std::env::temp_dir().join(format!("yard-pressure-{}-{}", std::process::id(), now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let now = now_ms();
        let mut episodes = vec![episode(now - KEEP_MS - 10_000, Some(now - KEEP_MS - 5_000))];
        episodes.extend((0..120).map(|i| episode(now - 600_000 + i * 1000, Some(now - 600_000 + i * 1000 + 500))));
        episodes.push(episode(now - 1000, None));
        std::fs::write(dir.join("pressure.json"), json!({ "episodes": episodes }).to_string()).unwrap();
        let p = Pressure::load(&dir);
        let (page, more) = p.page(None, 1000);
        assert!(!more);
        assert_eq!(page.len(), KEEP, "the oldest go first");
        assert_eq!(page[0].start, now - 1000, "newest first");
        assert_eq!(page[0].end, Some(now - 1000 + 5), "closed at its last sample");
        assert!(page.iter().all(|e| e.start >= now - 600_000));
        let (first, more) = p.page(Some(page[0].start), 2);
        assert!(more);
        assert_eq!(first.len(), 2);
        assert!(first.iter().all(|e| e.start < page[0].start));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_top_by_cpu_and_by_memory_once_each() {
        const MB: u64 = 1024 * 1024;
        let rows: Vec<(u32, f64, u64)> = vec![(1, 9.0, 70 * MB), (2, 0.1, 900 * MB), (3, 5.0, 5), (4, 4.0, 4), (5, 0.0, 800 * MB), (6, 3.0, 3), (7, 0.0, 700 * MB), (8, 0.01, 10 * MB)];
        let got: Vec<u32> = pick(&rows, |r| r.1, |r| r.2, |a, b| a.0 == b.0).iter().map(|r| r.0).collect();
        assert_eq!(got, vec![2, 5, 7, 1, 3, 4], "8 uses next to nothing");
    }

    #[test]
    fn what_counts_as_pressure() {
        let r = |load: f64, free: u64, swap: (u64, u64)| Readings {
            load,
            cores: 4,
            mem_used: 0,
            mem_total: 16,
            mem_free: free,
            swap_used: swap.0,
            swap_total: swap.1,
            load_injected: false,
            swap_injected: false,
        };
        assert!(reasons(&r(3.0, 10, (1, 10)), false, false, 2).is_empty());
        assert_eq!(reasons(&r(5.0, 10, (0, 0)), false, false, 2), ["load"]);
        assert_eq!(reasons(&r(1.0, 1, (0, 0)), false, false, 2), ["memory"]);
        // Linux leaves stale pages in swap long after the pressure is over:
        // full swap on a host with memory to spare is no pressure.
        assert!(reasons(&r(1.0, 10, (10, 10)), false, false, 2).is_empty(), "swap full, host calm");
        assert_eq!(reasons(&r(5.0, 10, (10, 10)), false, false, 2), ["load"], "full swap adds nothing to load");
        assert_eq!(reasons(&r(1.0, 1, (9, 10)), false, false, 2), ["memory", "swap"], "nowhere left to go");
        assert_eq!(reasons(&r(1.0, 1, (8, 10)), false, false, 2), ["memory"]);
        assert!(reasons(&r(5.0, 10, (9, 10)), false, true, 2).is_empty(), "a test machine's own");
        assert_eq!(reasons(&r(5.0, 1, (9, 10)), false, true, 2), ["memory"], "a test machine's own swap");
        assert_eq!(reasons(&Readings { load_injected: true, swap_injected: true, ..r(5.0, 10, (9, 10)) }, false, true, 2), ["load"]);
        assert_eq!(reasons(&Readings { load_injected: true, swap_injected: true, ..r(5.0, 1, (9, 10)) }, false, true, 2), ["load", "memory", "swap"]);
        assert_eq!(reasons(&r(0.0, 10, (0, 0)), true, false, 2), ["forced"]);
    }
}
