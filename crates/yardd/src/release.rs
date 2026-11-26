//! Idle engines let go of their session, so a finished session holds no
//! connections. Every loaded Codex thread keeps its own MCP clients, and every
//! Claude sidecar is a Claude Code process with its own; left loaded, one per
//! session piled up (one per automation run) into dozens of open connections
//! to one server.
//!
//! A session whose turn is over, with no subagent still at work, no approval
//! waiting and no queued message, is released after `window()` without
//! activity: its Codex thread is unsubscribed (`thread/unsubscribe`, which
//! unloads it) or its Claude sidecar stopped. Its backend goes back to
//! `Pending` with the engine id kept, so the next turn resumes it first, as
//! after a yardd restart. An automation's run is released as soon as it ends.

use std::{sync::Arc, time::Duration};

use crate::state::{now_ms, App, Backend, Meta};

const DEFAULT_WINDOW: Duration = Duration::from_secs(5 * 60);

/// How long a session stays idle before its engine is released.
/// `YARD_RELEASE_IDLE_MS` overrides it (the tests use a fraction of a second).
pub fn window() -> Duration {
    std::env::var("YARD_RELEASE_IDLE_MS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(Duration::from_millis)
        .unwrap_or(DEFAULT_WINDOW)
}

/// Nothing is going on or waiting in the session.
pub fn quiet(m: &Meta) -> bool {
    m.status == "idle" && m.background.is_empty() && m.pending.is_empty()
}

/// Release the session's engine at the next sweep once it is quiet, without
/// waiting out the window (its setup changed).
pub fn soon(app: &App, sid: &str) {
    if let Some(s) = app.sessions.lock().unwrap().get_mut(sid) {
        s.active_at = 0;
    }
    app.release_now.notify_one();
}

/// Work the engine itself keeps for later, by the tools the agent called
/// (the sidecar flags them `keepsAlive`, see sidecar/yard-claude/src/tools.ts):
/// a wakeup (`ScheduleWakeup`), a `Monitor` or a background command started
/// in the latest turn holds the engine until a later turn ends without one.
/// A cron job (`CronCreate`) lives in the process until `CronDelete`
/// (`endsCron`), whatever turns come after.
pub fn keeps_alive(events: &[serde_json::Value]) -> Option<&str> {
    let start = events.iter().rposition(|e| e["type"] == "turn_started").unwrap_or(0);
    let latest = events[start..].iter().filter(|e| e["type"] == "tool_started").find_map(|e| e["keepsAlive"].as_str().filter(|k| *k != "cron"));
    if latest.is_some() {
        return latest;
    }
    let cron = events.iter().rev().find(|e| e["type"] == "tool_started" && (e["keepsAlive"] == "cron" || e["endsCron"] == true));
    cron.and_then(|e| e["keepsAlive"].as_str())
}

/// Sessions whose engine is due to be released, with their engine.
fn due(app: &App, window_ms: u64) -> Vec<(String, String)> {
    let now = now_ms();
    let candidates: Vec<(String, String, u64)> = {
        let map = app.sessions.lock().unwrap();
        map.values()
            .filter(|s| !crate::private::internal(&s.meta.id) && quiet(&s.meta))
            .filter(|s| matches!(s.backend, Backend::Codex { turn_id: None, .. } | Backend::Claude { .. }))
            .filter(|s| {
                let ended_run = s.meta.automation.is_some() && s.meta.finished_at.is_some();
                ended_run || now.saturating_sub(s.active_at) >= window_ms
            })
            .filter(|s| keeps_alive(&s.events).is_none())
            .map(|s| (s.meta.id.clone(), s.meta.engine.clone(), s.active_at))
            .collect()
    };
    // Stopping a Claude sidecar ends every process under it: not while the
    // agent left one running, judged on a sample taken after the session's
    // last activity. Codex's shared app-server cannot tell whose a process
    // is once its command item has ended, so Codex sessions are not checked.
    let sampled = app.guard.sampled_at();
    candidates
        .into_iter()
        .filter(|(sid, engine, active)| engine != "claude" || (sampled > *active && app.guard.agent_work(sid).is_none()))
        .map(|(sid, engine, _)| (sid, engine))
        .collect()
}

/// Release one session's engine if it is (still) quiet. True if it was.
pub async fn release(app: &Arc<App>, sid: &str, engine: &str) -> bool {
    match engine {
        "codex" => match crate::codex::release(app, sid).await {
            Ok(done) => done,
            Err(e) => {
                eprintln!("yardd: releasing {sid}'s Codex thread: {e:#}");
                false
            }
        },
        "claude" => crate::claude::release(app, sid),
        _ => false,
    }
}

pub async fn run(app: Arc<App>) {
    let window = window();
    let tick = (window / 2).clamp(Duration::from_millis(100), Duration::from_secs(30));
    let window_ms = window.as_millis() as u64;
    loop {
        tokio::select! {
            _ = tokio::time::sleep(tick) => {}
            _ = app.release_now.notified() => {}
        }
        for (sid, engine) in due(&app, window_ms) {
            if release(&app, &sid, &engine).await {
                eprintln!("yardd: released the idle {engine} engine of {sid}");
            }
        }
    }
}
