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
///
/// `origin`: where the first message came from when the owner did not type
/// it (see `origins`).
pub async fn begin(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value], origin: Option<&Value>) {
    let items = [item(text, attachments, origin)];
    if let Some(reason) = app.guard.admit() {
        record(app, sid, &items, false);
        let mut queued = json!({ "type": "queued", "reason": reason, "text": text });
        if !attachments.is_empty() {
            queued["attachments"] = json!(attachments);
        }
        if crate::origins::is_tagged(origin) {
            queued["origin"] = origin.cloned().unwrap_or_default();
        }
        app.emit(sid, queued);
        app.guard.enqueue(sid, &reason);
        return;
    }
    // A restricted session gets a fresh engine with each turn (agents::prepare).
    if !crate::private::restricted(app, sid) && !app.meta(sid).is_some_and(|m| m.slack_place.is_some()) {
        if let Err(e) = start_engine(app, sid).await {
            record(app, sid, &items, false);
            fail_turn(app, sid, &e);
            return;
        }
    }
    send_turn(app, sid, &items).await;
}

/// A message for a session: queued while a turn runs (see Meta::pending),
/// else the next turn, with anything still queued going first.
pub async fn send_message(app: &Arc<App>, sid: &str, text: &str, files: &[Value], origin: Option<&Value>) -> anyhow::Result<()> {
    let meta = app.meta(sid).ok_or_else(|| anyhow::anyhow!("no such session"))?;
    if meta.slack_place.is_some() {
        app.queue_message(sid, text, files, origin.filter(|o| crate::origins::is_tagged(Some(o))));
        Box::pin(deliver(app, sid, Delivery::NextTurn)).await;
        return Ok(());
    }
    if meta.status == "running" || meta.status == "needs_approval" {
        app.queue_message(sid, text, files, origin.filter(|o| crate::origins::is_tagged(Some(o))));
        return Ok(());
    }
    if meta.status == "queued" {
        anyhow::bail!("this session is queued; start it first");
    }
    let mut items = app.take_all_queued(sid, |_| true);
    items.push(item(text, files, origin));
    send_turn(app, sid, &items).await;
    Ok(())
}

/// One message: its text, attachments and, unless the owner typed it, origin.
pub fn item(text: &str, attachments: &[Value], origin: Option<&Value>) -> Value {
    let mut item = json!({ "text": text, "attachments": attachments });
    if crate::origins::is_tagged(origin) {
        item["origin"] = origin.cloned().unwrap_or_default();
    }
    item
}

/// The `user_message` event. Attachments ride along (metadata only) so the
/// thread can show them after a reload and on other clients; so does the
/// origin of a message the owner did not type.
fn user_message(text: &str, attachments: &[Value], origin: Option<&Value>) -> Value {
    let mut ev = json!({ "type": "user_message", "text": text });
    if !attachments.is_empty() {
        ev["attachments"] = json!(attachments);
    }
    if crate::origins::is_tagged(origin) {
        ev["origin"] = origin.cloned().unwrap_or_default();
    }
    ev
}

/// Log messages that go to the engine together, and note where they came
/// from: the turn's answer is owed to those origins. The owner's messages
/// read as one, as before; once any came from elsewhere, each is its own
/// message, so each keeps its badge. `steered`: they joined the running turn
/// (whose `answering` the caller already grew).
fn record(app: &App, sid: &str, items: &[Value], steered: bool) {
    let tagged = origins_of(items);
    let mut events = Vec::new();
    if tagged.is_empty() {
        let (text, files) = combine(items);
        events.push(user_message(&text, &files, None));
    } else {
        for i in items {
            let files = i["attachments"].as_array().cloned().unwrap_or_default();
            events.push(user_message(i["text"].as_str().unwrap_or(""), &files, Some(&i["origin"])));
        }
    }
    // Before the events: a turn that ends at once must still owe its answer.
    if !steered && (!tagged.is_empty() || app.meta(sid).is_some_and(|m| !m.answering.is_empty())) {
        app.update_meta(sid, |m| m.answering = tagged);
    }
    for mut ev in events {
        if steered {
            ev["steered"] = json!(true);
            // When the agent got it, so the thread can time the work after it.
            ev["at"] = json!(crate::state::now_ms());
        }
        app.emit(sid, ev);
    }
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
    if !crate::private::restricted(app, sid) && !app.meta(sid).is_some_and(|m| m.slack_place.is_some()) {
        if let Err(e) = start_engine(app, sid).await {
            fail_turn(app, sid, &e);
            return;
        }
    }
    let origin = queued.as_ref().map(|e| e["origin"].clone()).filter(Value::is_object);
    let attachments = queued.and_then(|e| e["attachments"].as_array().cloned()).unwrap_or_default();
    if let Some(origin) = origin.as_ref().filter(|o| crate::origins::slack_guest(o)) {
        crate::slack::guest_turn(app, sid, &text, origin);
        return;
    }
    start_turn(app, sid, &crate::origins::engine_text(&text, origin.as_ref()), &attachments).await;
}

pub async fn start_engine(app: &Arc<App>, sid: &str) -> anyhow::Result<()> {
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
            let items = if app.meta(sid).is_some_and(|m| m.slack_place.is_some()) {
                app.take_place_turn(sid).into_iter().collect()
            } else {
                app.take_all_queued(sid, |s| s == "idle")
            };
            if items.is_empty() {
                return;
            }
            send_turn(app, sid, &items).await;
        }
        Delivery::Steer | Delivery::SteerOne(_) => {
            if app.meta(sid).is_some_and(|m| m.slack_place.is_some()) {
                return;
            }
            let engine = app.meta(sid).map(|m| m.engine).unwrap_or_default();
            // The engine cannot take input mid-turn: it waits for the next one.
            if !crate::engines::can_steer(&engine) {
                return;
            }
            let items: Vec<Value> = match when {
                Delivery::SteerOne(id) => {
                    // A watched channel's notice never joins a turn that may act:
                    // it stays queued for a read-only turn of its own.
                    let one: Vec<Value> = app.take_queued(sid, &id, true).into_iter().collect();
                    if one.iter().any(|i| crate::origins::read_only(&i["origin"])) {
                        app.requeue(sid, one);
                        return;
                    }
                    one
                }
                _ => app.take_steerable(sid),
            };
            if items.is_empty() {
                return;
            }
            let (text, attachments) = engine_input(&items);
            // Owed before the engine reads them, so a turn that ends right
            // after still answers them.
            let tagged = origins_of(&items);
            if !tagged.is_empty() {
                app.update_meta(sid, |m| m.answering.extend(tagged.iter().cloned()));
            }
            let sent = match engine.as_str() {
                "codex" => codex::steer(app, sid, &text, &attachments).await,
                _ => claude::push(app, sid, &text, &attachments),
            };
            match sent {
                Ok(()) => record(app, sid, &items, true),
                Err(e) => {
                    // The turn may have ended under us: back in the queue,
                    // and on to the next turn if it already completed.
                    eprintln!("yardd: could not steer {sid}: {e:#}");
                    if !tagged.is_empty() {
                        app.update_meta(sid, |m| m.answering.retain(|o| !tagged.contains(o)));
                    }
                    app.requeue(sid, items);
                    if app.meta(sid).is_some_and(|m| m.status == "idle") {
                        Box::pin(deliver(app, sid, Delivery::NextTurn)).await;
                    }
                }
            }
        }
    }
}

/// Record the messages and hand them to the session's engine as one turn.
pub async fn send_turn(app: &Arc<App>, sid: &str, items: &[Value]) {
    record(app, sid, items, false);
    if let [item] = items {
        if crate::origins::slack_guest(&item["origin"]) {
            crate::slack::guest_turn(app, sid, item["text"].as_str().unwrap_or(""), &item["origin"]);
            return;
        }
    }
    let (text, attachments) = engine_input(items);
    start_turn(app, sid, &text, &attachments).await;
}

/// What the engine reads for messages that go together: each framed with
/// where it came from (see `origins::engine_text`), joined like `combine`.
fn engine_input(items: &[Value]) -> (String, Vec<Value>) {
    let framed: Vec<Value> = items
        .iter()
        .map(|i| {
            let origin = Some(&i["origin"]).filter(|o| o.is_object());
            json!({ "text": crate::origins::engine_text(i["text"].as_str().unwrap_or(""), origin), "attachments": i["attachments"] })
        })
        .collect();
    combine(&framed)
}

fn origins_of(items: &[Value]) -> Vec<Value> {
    items.iter().filter(|i| crate::origins::is_tagged(Some(&i["origin"]))).map(|i| i["origin"].clone()).collect()
}

async fn start_turn(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) {
    app.emit(sid, json!({ "type": "turn_started" }));
    run_engine_turn(app, sid, text, attachments).await;
}

pub async fn resume_slack_access(app: &Arc<App>, sid: &str, text: &str, origin: Value) {
    let input = crate::origins::engine_text(text, Some(&origin));
    app.update_meta(sid, |m| m.answering = vec![origin]);
    run_engine_turn(app, sid, &input, &[]).await;
}

async fn run_engine_turn(app: &Arc<App>, sid: &str, text: &str, attachments: &[Value]) {
    // The engines run it in Plan (origins::turn_permission); the owner sees why.
    if app.meta(sid).is_some_and(|m| m.permission != "plan" && m.answering.iter().any(crate::origins::read_only)) {
        let text = "This turn is read-only (Plan): it works on a notice from a watched Slack channel. Anything it wants to change waits for your approval.";
        app.emit(sid, json!({ "type": "note", "text": text }));
    }
    // The sweep removes a merged PR's worktree under an idle session. An
    // engine started there fails its own way: Claude Code's SDK reports
    // ENOENT as a libc mismatch, which sends the user after the wrong thing.
    if let Some(meta) = app.meta(sid) {
        if crate::git::ensure_folder(&meta.cwd).is_err() {
            let e = anyhow::anyhow!("This session's folder no longer exists: {}. Start a new session to keep working.", meta.cwd);
            fail_turn(app, sid, &e);
            return;
        }
    }
    let original = text;
    // An agent's session gives its engine more than the user typed.
    let text = match crate::agents::prepare(app, sid, text).await {
        Ok(input) => input,
        Err(e) => {
            fail_turn(app, sid, &e);
            return;
        }
    };
    let engine = app.meta(sid).map(|m| m.engine).unwrap_or_default();
    let needs_engine = app.sessions.lock().unwrap().get(sid).is_some_and(|s| {
        s.meta.slack_place.is_some() && s.engine_ref.is_none() && matches!(s.backend, Backend::Pending)
    });
    if needs_engine {
        if let Err(e) = start_engine(app, sid).await {
            fail_turn(app, sid, &e);
            return;
        }
    }
    let mut sent = send_to(app, sid, &engine, &text, attachments).await;
    // An agent's conversation outlives its engine thread: one that cannot
    // be resumed is replaced, with the conversation so far, and tried again.
    if let Err(e) = &sent {
        if crate::agents::may_refresh(app, sid) {
            eprintln!("yardd: {sid}: {e:#}; starting a fresh context");
            app.update_meta(sid, |m| m.fresh_context = Some("the engine could not continue the conversation".into()));
            sent = match crate::agents::prepare(app, sid, original).await {
                Ok(input) => send_to(app, sid, &engine, &input, attachments).await,
                Err(e) => Err(e),
            };
        }
    }
    if let Err(e) = sent {
        fail_turn(app, sid, &e);
    }
}

async fn send_to(app: &Arc<App>, sid: &str, engine: &str, text: &str, attachments: &[Value]) -> anyhow::Result<()> {
    match engine {
        "codex" => codex::send(app, sid, text, attachments).await,
        "opencode" | "grok" | "gemini" => acp::send(app, sid, text, attachments).await,
        _ => claude::send(app, sid, text, attachments),
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
    if meta.slack_place.is_some() && crate::slack::stop_guest_turn(app, sid)? {
        return Ok(());
    }
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
