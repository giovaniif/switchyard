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

/// Sessions whose engine is due to be released, with their engine.
fn due(app: &App, window_ms: u64) -> Vec<(String, String)> {
    let now = now_ms();
    let map = app.sessions.lock().unwrap();
    map.values()
        .filter(|s| !crate::private::internal(&s.meta.id) && quiet(&s.meta))
        .filter(|s| matches!(s.backend, Backend::Codex { turn_id: None, .. } | Backend::Claude { .. }))
        .filter(|s| {
            let ended_run = s.meta.automation.is_some() && s.meta.finished_at.is_some();
            ended_run || now.saturating_sub(s.active_at) >= window_ms
        })
        .map(|s| (s.meta.id.clone(), s.meta.engine.clone()))
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
