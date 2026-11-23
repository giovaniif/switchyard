//! Starting sessions and turns. Shared by everything that can begin work:
//! the new-session page today, hand-off and the queue next.

use std::{sync::Arc, time::Duration};

use serde_json::{json, Value};

use crate::{acp, claude, codex, state::{App, Backend, Delivery}};

/// How long Stop waits for the engine to end the turn before ending it itself.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Start the engine for a session that was just registered with
/// `App::new_session`, then run its first turn. Failures become events in the
/// session rather than errors, so the session shows what went wrong.
///
/// While the host is under pressure the session is queued instead: it keeps
/// its prompt, and the host sampler starts it once there is room.
pub async fn begin(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) {
    if let Some(reason) = app.guard.admit() {
        app.emit(sid, user_message(text, attachments));
        let mut queued = json!({ "type": "queued", "reason": reason, "text": text });
        if !attachments.is_empty() {
            queued["attachments"] = json!(attachments);
        }
        app.emit(sid, queued);
        app.guard.enqueue(sid, &reason);
        return;
    }
    // A restricted session gets a fresh engine with each turn (agents::prepare).
    if !crate::private::restricted(app, sid) {
        if let Err(e) = start_engine(app, sid).await {
            app.emit(sid, user_message(text, attachments));
            fail_turn(app, sid, &e);
            return;
        }
    }
    send_turn(app, sid, text, attachments).await;
}

/// A message for a session: queued while a turn runs (see Meta::pending),
/// else the next turn, with anything still queued going first.
pub async fn send_message(app: &Arc<App>, sid: &str, text: &str, files: &[Value]) -> anyhow::Result<()> {
    let meta = app.meta(sid).ok_or_else(|| anyhow::anyhow!("no such session"))?;
    if meta.status == "running" || meta.status == "needs_approval" {
        app.queue_message(sid, text, files);
        return Ok(());
    }
    if meta.status == "queued" {
        anyhow::bail!("this session is queued; start it first");
    }
    let mut items = app.take_all_queued(sid, |_| true);
    items.push(json!({ "text": text, "attachments": files }));
    let (text, files) = combine(&items);
    send_turn(app, sid, &text, &files).await;
    Ok(())
}

/// The `user_message` event. Attachments ride along (metadata only) so the
/// thread can show them after a reload and on other clients.
fn user_message(text: &str, attachments: &[Value]) -> Value {
    let mut ev = json!({ "type": "user_message", "text": text });
    if !attachments.is_empty() {
        ev["attachments"] = json!(attachments);
    }
    ev
}

/// Start a session taken off the queue, with the prompt it was queued with.
/// Its `user_message` is already in the log.
pub async fn start_queued(app: &Arc<App>, sid: &str) {
    let queued = {
        let map = app.sessions.lock().unwrap();
        map.get(sid).and_then(|s| crate::state::queued_event(&s.events)).cloned()
    };
    let Some(text) = queued.as_ref().and_then(|e| e["text"].as_str()).map(str::to_string) else {
        fail_turn(app, sid, &anyhow::anyhow!("the queued prompt was lost"));
        return;
    };
    if !crate::private::restricted(app, sid) {
        if let Err(e) = start_engine(app, sid).await {
            fail_turn(app, sid, &e);
            return;
        }
    }
    let attachments = queued.and_then(|e| e["attachments"].as_array().cloned()).unwrap_or_default();
    start_turn(app, sid, &text, &attachments).await;
}

async fn start_engine(app: &Arc<App>, sid: &str) -> anyhow::Result<()> {
    let engine = app.meta(sid).map(|m| m.engine).unwrap_or_default();
    match engine.as_str() {
        "codex" => codex::create(app, sid).await,
        "opencode" | "grok" | "gemini" => acp::create(app, sid).await,
        _ => claude::create(app, sid).await,
    }
}

/// Several queued messages go to the agent as one, as in Claude Code.
pub fn combine(items: &[Value]) -> (String, Vec<Value>) {
    let text = items.iter().filter_map(|i| i["text"].as_str()).filter(|t| !t.is_empty()).collect::<Vec<_>>().join("\n\n");
    let attachments = items.iter().flat_map(|i| i["attachments"].as_array().cloned().unwrap_or_default()).collect();
    (text, attachments)
}

/// Deliver queued messages: into the running turn at a step, or as the next
/// turn once it completed.
pub async fn deliver(app: &Arc<App>, sid: &str, when: Delivery) {
    match when {
        Delivery::NextTurn => {
            let items = app.take_all_queued(sid, |s| s == "idle");
            if items.is_empty() {
                return;
            }
            let (text, attachments) = combine(&items);
            send_turn(app, sid, &text, &attachments).await;
        }
        Delivery::Steer | Delivery::SteerOne(_) => {
            let engine = app.meta(sid).map(|m| m.engine).unwrap_or_default();
            // The engine cannot take input mid-turn: it waits for the next one.
            if !crate::engines::can_steer(&engine) {
                return;
            }
            let items = match when {
                Delivery::SteerOne(id) => app.take_queued(sid, &id, true).into_iter().collect(),
                _ => app.take_all_queued(sid, |s| s == "running"),
            };
            if items.is_empty() {
                return;
            }
            let (text, attachments) = combine(&items);
            let sent = match engine.as_str() {
                "codex" => codex::steer(app, sid, &text, &attachments).await,
                _ => claude::push(app, sid, &text, &attachments),
            };
            match sent {
                Ok(()) => {
                    let mut ev = user_message(&text, &attachments);
                    ev["steered"] = json!(true);
                    // When the agent got it, so the thread can time the work after it.
                    ev["at"] = json!(crate::state::now_ms());
                    app.emit(sid, ev);
                }
                Err(e) => {
                    // The turn may have ended under us: back in the queue,
                    // and on to the next turn if it already completed.
                    eprintln!("yardd: could not steer {sid}: {e:#}");
                    app.requeue(sid, items);
                    if app.meta(sid).is_some_and(|m| m.status == "idle") {
                        Box::pin(deliver(app, sid, Delivery::NextTurn)).await;
                    }
                }
            }
        }
    }
}

/// Record the user's message and hand it to the session's engine.
pub async fn send_turn(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) {
    app.emit(sid, user_message(text, attachments));
    start_turn(app, sid, text, attachments).await;
}

async fn start_turn(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) {
    app.emit(sid, json!({ "type": "turn_started" }));
    // An agent's session gives its engine more than the user typed.
    let text = match crate::agents::prepare(app, sid, text).await {
        Ok(crate::agents::Prepared::Send(input)) => input,
        Ok(crate::agents::Prepared::Stopped) => return,
        Err(e) => {
            fail_turn(app, sid, &e);
            return;
        }
    };
    let text = text.as_str();
    let engine = app.meta(sid).map(|m| m.engine).unwrap_or_default();
    let sent = match engine.as_str() {
        "codex" => codex::send(app, sid, text, attachments).await,
        "opencode" | "grok" | "gemini" => acp::send(app, sid, text, attachments).await,
        _ => claude::send(app, sid, text, attachments),
    };
    if let Err(e) = sent {
        fail_turn(app, sid, &e);
    }
}

/// The user answered an approval. Engines call this before passing the answer on.
pub fn approval_resolved(app: &App, sid: &str, request_id: &str, decision: &str) {
    app.emit(sid, json!({ "type": "approval_resolved", "requestId": request_id, "decision": decision }));
}

pub fn fail_turn(app: &Arc<App>, sid: &str, error: &anyhow::Error) {
    app.emit(sid, json!({ "type": "error", "message": format!("{error:#}") }));
    app.emit(sid, json!({ "type": "turn_completed", "status": "failed" }));
}

/// Where the session's latest turn starts in its log, if it is still running.
fn running_turn(app: &App, sid: &str) -> Option<usize> {
    let map = app.sessions.lock().unwrap();
    let s = map.get(sid)?;
    if s.meta.status != "running" && s.meta.status != "needs_approval" {
        return None;
    }
    s.events.iter().rposition(|e| e["type"] == "turn_started")
}

/// Stop the running turn. Asks the engine first; if it cannot be asked, or
/// the turn is still open after `STOP_GRACE`, ends the turn here so Stop
/// always works: a Claude sidecar is killed (its next message resumes the
/// conversation), a Codex turn is closed on our side.
pub async fn interrupt(app: &Arc<App>, sid: &str) -> anyhow::Result<()> {
    let meta = app.meta(sid).ok_or_else(|| anyhow::anyhow!("no such session"))?;
    let Some(turn) = running_turn(app, sid) else {
        return Ok(());
    };
    let asked = match meta.engine.as_str() {
        // A wedged app-server may never answer.
        "codex" => match tokio::time::timeout(STOP_GRACE, codex::interrupt(app, sid)).await {
            Ok(r) => r,
            Err(_) => Err(anyhow::anyhow!("Codex did not answer turn/interrupt")),
        },
        "opencode" | "grok" | "gemini" => acp::interrupt(app, sid),
        _ => claude::interrupt(app, sid),
    };
    if let Err(e) = &asked {
        eprintln!("yardd: could not interrupt {sid}, ending the turn: {e:#}");
        force_stop(app, sid, &meta.engine);
        return Ok(());
    }
    let app = app.clone();
    let sid = sid.to_string();
    tokio::spawn(async move {
        tokio::time::sleep(STOP_GRACE).await;
        if running_turn(&app, &sid) == Some(turn) {
            force_stop(&app, &sid, &meta.engine);
        }
    });
    Ok(())
}

fn force_stop(app: &Arc<App>, sid: &str, engine: &str) {
    if engine == "codex" {
        if let Some(s) = app.sessions.lock().unwrap().get_mut(sid) {
            if let Backend::Codex { turn_id, .. } = &mut s.backend {
                *turn_id = None;
            }
        }
    }
    if engine != "codex" && engine != "claude" {
        acp::end_turn_here(app, sid);
    }
    app.guard.turn_done(sid);
    let text = if engine == "claude" {
        "Claude Code did not stop, so its process was ended. The next message resumes the conversation.".to_string()
    } else {
        format!("{} did not stop the turn, so it was ended here.", crate::engines::name(engine))
    };
    app.emit(sid, json!({ "type": "note", "text": text }));
    // Ended before the kill, so the sidecar's exit does not report a failure.
    app.emit(sid, json!({ "type": "turn_completed", "status": "interrupted" }));
    if engine == "claude" {
        claude::kill(app, sid);
    }
}
