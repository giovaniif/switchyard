//! yardd end to end: the built daemon, driven over its WebSocket, with the
//! scripted fake Codex in `tests/fake-codex` instead of the real CLI.

mod common;

use std::{
    fs,
    path::Path,
    time::{Duration, Instant},
};

use base64::Engine as _;
use common::*;
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue};

#[tokio::test]
async fn snapshot_has_its_shape_and_the_fake_models() {
    let sb = Sandbox::new("snapshot");
    let d = sb.start();
    let mut c = d.client().await;

    let snap = c.log[0].clone();
    assert_eq!(snap["op"], "snapshot");
    assert_eq!(snap["sessions"], json!([]));
    assert_eq!(snap["hosts"], json!([]));
    assert_eq!(snap["home"], sb.home().to_str().unwrap());
    assert!(snap["usage"].is_object());
    let claude: Vec<&str> = snap["models"]["claude"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert!(claude.contains(&"claude-opus-5-5"), "{claude:?}");

    // The model list comes from the fake, either in the snapshot or right after it.
    let codex = c
        .wait_for("codex models from the fake", |m| {
            let models = if m["op"] == "snapshot" || m["op"] == "models" { &m["models"]["codex"] } else { &Value::Null };
            models.as_array().is_some_and(|l| !l.is_empty())
        })
        .await["models"]["codex"]
        .clone();
    assert_eq!(
        codex,
        json!([
            { "id": "fake-fast", "name": "Fake Fast", "efforts": ["low", "high"], "defaultEffort": "low", "isDefault": true,
                "serviceTiers": [
                    { "id": "priority", "name": "Fast", "description": "Higher speed, increased usage" },
                    { "id": "ultrafast", "name": "Ultrafast", "description": "Fastest available responses" },
                ] },
            { "id": "fake-deep", "name": "Fake Deep", "efforts": ["medium", "xhigh"], "defaultEffort": "medium", "isDefault": false },
        ])
    );
    let usage = c
        .wait_for("codex usage from the fake", |m| {
            (m["op"] == "usage" || m["op"] == "snapshot") && !m["usage"]["codex"].is_null()
        })
        .await;
    assert_eq!(usage["usage"]["codex"]["primary"]["usedPercent"], 12);
}

/// The Claude models a snapshot or `models` message lists.
fn claude_ids(m: &Value) -> Vec<String> {
    let models = if m["op"] == "snapshot" || m["op"] == "models" { &m["models"]["claude"] } else { &Value::Null };
    models.as_array().map(|l| l.iter().filter_map(|x| x["id"].as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Claude's models are what Claude Code reports for each account (the fake
/// sidecar's `models`, read without a turn), not a list built into yardd: the
/// owner's plan has Fable, and the picker never offered it. A model only some
/// accounts report names them. The lists are saved: a restart whose reads
/// fail still lists them.
#[tokio::test]
async fn claude_models_are_what_each_account_reports() {
    let sb = Sandbox::new("claude-models");
    let sidecar = fake_bin().join("yard-claude");
    let d = sb.start_with(&[("YARD_SIDECAR", sidecar.to_str().unwrap()), ("YARD_CLAUDE_MODELS_EVERY_MS", "600000")]);
    let mut c = d.client().await;
    let efforts = json!(["low", "medium", "high", "xhigh", "max"]);
    // The built-in list marks no default; Claude Code's does.
    let got = c.wait_for("the models Claude reports", |m| !claude_ids(m).is_empty() && m["models"]["claude"][0]["isDefault"] == true).await;
    assert_eq!(claude_ids(&got), ["claude-opus-5-5", "claude-fable-5-2", "claude-sonnet-5-5", "claude-haiku-5-5"]);
    assert_eq!(got["models"]["claude"][0], json!({ "id": "claude-opus-5-5", "name": "Opus 5.5", "efforts": efforts, "defaultEffort": "high", "isDefault": true }));
    assert_eq!(got["models"]["claude"][1], json!({ "id": "claude-fable-5-2", "name": "Fable 5.2", "efforts": efforts, "defaultEffort": "high" }));
    assert!(c.log.iter().all(|m| m["op"] != "event"), "no turn ran");
    let log = fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap();
    assert!(log.lines().any(|l| l == "models config="), "{log}");

    // An account whose plan has no Fable: Fable stays, as Default's only.
    let from = c.mark();
    c.send(json!({ "op": "account.add", "engine": "claude", "name": "Work" })).await;
    let accounts = c.wait_after(from, "the accounts list", |m| m["op"] == "accounts" && m["accounts"].as_array().is_some_and(|l| !l.is_empty())).await;
    let id = accounts["accounts"][0]["id"].as_str().unwrap().to_string();
    let fable_only_default = |m: &Value| m["op"] == "models" && m["models"]["claude"][1]["accounts"] == json!(["default"]);
    let got = c.wait_after(from, "Fable marked as Default's", fable_only_default).await;
    assert_eq!(claude_ids(&got), ["claude-opus-5-5", "claude-fable-5-2", "claude-sonnet-5-5", "claude-haiku-5-5"]);
    assert!(got["models"]["claude"][0].get("accounts").is_none(), "a model every account has names none: {got}");
    let log = fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap();
    assert!(log.contains(&format!("models config={}", sb.data().join("accounts").join(&id).display())), "{log}");
    d.stop();

    // The sidecar is now the plain fake `claude`, which answers nothing.
    let d = sb.start_with(&[("YARD_CLAUDE_MODELS_EVERY_MS", "600000")]);
    let mut c = d.client().await;
    assert!(fable_only_default(&json!({ "op": "models", "models": c.log[0]["models"] })), "the saved lists: {}", c.log[0]["models"]);
    wait_for_models_read(&sb).await;
    c.drain_for(Duration::from_millis(300)).await;
    let last = c.log.iter().rev().find(|m| !claude_ids(m).is_empty()).unwrap().clone();
    assert_eq!(last["models"]["claude"][1]["id"], "claude-fable-5-2", "a failed read keeps them: {last}");
}

/// Until Claude Code answers (or when it cannot), the built-in list stands,
/// and it has Fable.
#[tokio::test]
async fn claude_models_fall_back_to_a_built_in_list_with_fable() {
    let sb = Sandbox::new("claude-models-fallback");
    let d = sb.start_with(&[("YARD_CLAUDE_MODELS_EVERY_MS", "600000")]);
    let mut c = d.client().await;
    let built_in = ["claude-opus-5-5", "claude-fable-5-2", "claude-sonnet-5-5", "claude-haiku-5-5"];
    assert_eq!(claude_ids(&c.log[0]), built_in);
    wait_for_models_read(&sb).await;
    c.drain_for(Duration::from_millis(300)).await;
    let last = c.log.iter().rev().find(|m| !claude_ids(m).is_empty()).unwrap().clone();
    assert_eq!(claude_ids(&last), built_in);
    assert_eq!(last["models"]["claude"][1]["name"], "Fable 5.2");
}

/// The fake `claude` records the failed `models` read.
async fn wait_for_models_read(sb: &Sandbox) {
    for _ in 0..100 {
        if fs::read_to_string(sb.fake_state().join("claude.log")).is_ok_and(|l| l.contains("args=models")) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("yardd never asked the sidecar for its models");
}

#[tokio::test]
async fn the_snapshot_names_this_machine_before_its_first_stats_sample() {
    // Stats come every few seconds; until then the snapshot's `host` is null,
    // and the UI fell back to "This Mac" even on a Linux box.
    let sb = Sandbox::new("snapshot-host-name");
    let d = sb.start_with(&[("YARD_HOST_NAME", "TEST-box")]);
    let c = d.client().await;
    let snap = &c.log[0];
    assert_eq!(snap["op"], "snapshot");
    assert_eq!(snap["hostName"], "TEST-box", "{snap}");
}

#[tokio::test]
async fn a_new_client_gets_its_snapshot_before_any_broadcast() {
    let sb = Sandbox::new("snapshot-first");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;
    // Keep broadcasts flowing while clients connect. The UI replaces its
    // state with a snapshot, so a broadcast that overtook the snapshot it
    // came after was lost.
    for i in 0..3000 {
        c.send(json!({ "op": "session.update", "sessionId": sid, "pinned": i % 2 == 0 })).await;
    }
    for n in 0..40 {
        let other = d.client().await;
        assert_eq!(other.log[0]["op"], "snapshot", "client {n} got {} first", other.log[0]["op"]);
    }
}

#[tokio::test]
async fn a_plain_turn_streams_then_completes() {
    let sb = Sandbox::new("plain");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");

    let sid = c.create(&cwd, "say hello", json!({})).await;
    let done = c.wait_turn_done(&sid, 0).await;
    assert_eq!(done["status"], "completed");
    c.wait_status(&sid, "idle").await;

    let session = c.wait_for("the session's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid).await;
    assert_eq!(session["session"]["engine"], "codex");
    assert_eq!(session["session"]["model"], MODEL);
    assert_eq!(session["session"]["cwd"], cwd);
    assert_eq!(session["session"]["title"], "say hello");

    let events = c.events(&sid);
    assert_eq!(
        types(&events),
        [
            "user_message",
            "turn_started",
            "assistant_delta",
            "assistant_delta",
            "assistant_delta",
            "assistant_message",
            "turn_completed"
        ]
    );
    assert_eq!(events[0]["text"], "say hello");
    let streamed: String = events.iter().filter(|e| e["type"] == "assistant_delta").map(|e| e["text"].as_str().unwrap()).collect();
    assert_eq!(streamed, "Echo: say hello");
    assert_eq!(events[5]["text"], "Echo: say hello");
    assert_eq!(events[2]["itemId"], events[5]["itemId"], "deltas and the final message share an item");
    assert_eq!(c.statuses(&sid), ["running", "idle"]);

    // A follow-up in the same thread.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "again" })).await;
    c.wait_turn_done(&sid, next).await;
    let last = c.events(&sid);
    assert_eq!(last[last.len() - 2]["text"], "Echo: again");
    assert_eq!(sb.codex_log().matches("req thread/start").count(), 1);
    assert!(sb.codex_log().contains("req thread/start sandbox=workspace-write"), "{}", sb.codex_log());
}

#[tokio::test]
async fn the_codex_sandbox_can_be_set() {
    let sb = Sandbox::new("sandbox");
    let d = sb.start_with(&[("YARD_CODEX_SANDBOX", "danger-full-access")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    assert!(sb.codex_log().contains("req thread/start sandbox=danger-full-access"), "{}", sb.codex_log());
}

/// A long first line becomes a title cut at a word, marked with "…", not
/// one that stops mid-word and reads like a typo.
#[tokio::test]
async fn a_long_first_line_is_titled_at_a_word_with_an_ellipsis() {
    let sb = Sandbox::new("long-title");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");
    let long = "a very long first prompt that becomes the title and keeps going on and on";
    let sid = c.create(&cwd, &format!("{long}\nsecond line"), json!({})).await;
    let session = c.wait_for("the session's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid).await;
    assert_eq!(session["session"]["title"], "a very long first prompt that becomes the title and keeps…");

    // One that fits stays whole, with no mark.
    let fits = "x".repeat(60);
    let sid = c.create(&sb.folder("proj2"), &fits, json!({})).await;
    let session = c.wait_for("the session's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid).await;
    assert_eq!(session["session"]["title"], fits.as_str());

    // A single word longer than the limit is cut, still marked.
    let word = "y".repeat(80);
    let sid = c.create(&sb.folder("proj3"), &word, json!({})).await;
    let session = c.wait_for("the session's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid).await;
    assert_eq!(session["session"]["title"], format!("{}…", "y".repeat(59)));

    // A long word after a short one (a pasted URL) is cut through, not
    // dropped: backing up to the word would leave just "fix…".
    let url = "fix https://github.com/example-org/example-repo/issues/12345#issuecomment-987654321";
    let sid = c.create(&sb.folder("proj4"), url, json!({})).await;
    let session = c.wait_for("the session's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid).await;
    assert_eq!(session["session"]["title"], format!("{}…", url.chars().take(59).collect::<String>()));
}

#[tokio::test]
async fn approval_allow_runs_the_command() {
    let sb = Sandbox::new("allow");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] run it", json!({})).await;

    let req = c.wait_event(&sid, "approval_request", 0).await;
    assert_eq!(req["kind"], "command");
    assert_eq!(req["title"], "echo approved", "the shell wrapper is stripped");
    assert_eq!(req["detail"], "needs to run a command");
    // The UI leaves the wait out of "Worked for".
    assert!(req["at"].is_u64(), "an approval carries yardd's clock: {req}");
    c.wait_status(&sid, "needs_approval").await;

    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" }))
        .await;
    let resolved = c.wait_event(&sid, "approval_resolved", 0).await;
    assert_eq!(resolved["decision"], "allow");
    assert!(resolved["at"].is_u64(), "its answer too: {resolved}");
    let tool = c.wait_event(&sid, "tool_completed", 0).await;
    assert_eq!(tool["ok"], true);
    assert_eq!(tool["output"], "approved\n");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    c.wait_status(&sid, "idle").await;
    assert_eq!(c.statuses(&sid), ["running", "needs_approval", "running", "idle"]);
    let started = c.wait_event(&sid, "tool_started", 0).await;
    assert_eq!(started["detail"], "echo approved");
}

/// An engine that runs the command and ends the turn right after the answer
/// must not overtake `approval_resolved`: a late one put the session back to
/// `running` after its turn had completed, and the next message then waited
/// in the queue forever. The fake answers instantly; many at once make the
/// race show up.
#[tokio::test]
async fn an_answered_approval_is_logged_before_the_engine_moves_on() {
    let sb = Sandbox::new("allow-race");
    let d = sb.start();
    let mut c = d.client().await;
    let mut asks = Vec::new();
    for i in 0..16 {
        let sid = c.create(&sb.folder(&format!("proj{i}")), "[cmd] run it", json!({})).await;
        let req = c.wait_event(&sid, "approval_request", 0).await;
        asks.push((sid, req["requestId"].clone()));
    }
    for (sid, request_id) in &asks {
        c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": request_id, "decision": "allow" }))
            .await;
    }
    for (sid, _) in &asks {
        c.wait_turn_done(sid, 0).await;
        c.wait_event(sid, "approval_resolved", 0).await;
        c.wait_status(sid, "idle").await;
    }
    // One more turn end to end, so a status broadcast late from the ones
    // above has arrived by the time it is done.
    let flush = c.create(&sb.folder("flush"), "flush", json!({})).await;
    c.wait_turn_done(&flush, 0).await;
    let fresh = d.client().await;
    for (sid, _) in &asks {
        let events = c.events(sid);
        let order: Vec<&str> = types(&events)
            .into_iter()
            .filter(|t| ["approval_resolved", "tool_completed", "turn_completed"].contains(t))
            .collect();
        assert_eq!(order, ["approval_resolved", "tool_completed", "turn_completed"], "{sid}");
        assert_eq!(c.statuses(sid).last().map(String::as_str), Some("idle"), "what the client heard for {sid}");
        assert_eq!(meta_of(&fresh, sid)["status"], "idle", "the daemon's own status for {sid}");
    }
}

#[tokio::test]
async fn approval_deny_declines_the_command() {
    let sb = Sandbox::new("deny");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] run it", json!({})).await;

    let req = c.wait_event(&sid, "approval_request", 0).await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "deny" }))
        .await;
    let tool = c.wait_event(&sid, "tool_completed", 0).await;
    assert_eq!(tool["ok"], false);
    assert_eq!(tool["note"], "declined");
    assert_eq!(c.wait_event(&sid, "assistant_message", 0).await["text"], "Skipped it.");
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;

    // The request is answered; a second answer is refused.
    let from = c.mark();
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" }))
        .await;
    assert!(c.wait_error(from, "session.approve").await.contains("no longer pending"));
}

#[tokio::test]
async fn a_file_change_carries_its_diff_with_relative_paths() {
    let sb = Sandbox::new("edit");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[edit] add a function", json!({})).await;

    let change = c.wait_event(&sid, "file_change", 0).await;
    let files = change["files"].as_array().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["path"], "src/lib.rs");
    assert_eq!(files[0]["kind"], "update");
    assert!(files[0]["diff"].as_str().unwrap().contains("+fn added() {}"));
    c.wait_turn_done(&sid, 0).await;
}

/// The session's newest metadata as this client last heard it.
fn meta_of(c: &Client, sid: &str) -> Value {
    c.log
        .iter()
        .rev()
        .find_map(|m| {
            if m["op"] == "session" && m["session"]["id"] == sid {
                return Some(m["session"].clone());
            }
            if m["op"] == "snapshot" {
                return m["sessions"].as_array()?.iter().find(|s| s["id"] == sid).cloned();
            }
            None
        })
        .unwrap_or(Value::Null)
}

fn pending_texts(c: &Client, sid: &str) -> Vec<String> {
    meta_of(c, sid)["pending"].as_array().map(|a| a.iter().map(|p| p["text"].as_str().unwrap_or("").to_string()).collect()).unwrap_or_default()
}

fn user_texts(c: &Client, sid: &str) -> Vec<String> {
    c.events(sid).iter().filter(|e| e["type"] == "user_message").map(|e| e["text"].as_str().unwrap_or("").to_string()).collect()
}

fn count(c: &Client, sid: &str, ty: &str) -> usize {
    c.event_types(sid).iter().filter(|t| *t == ty).count()
}

// Messages sent during a turn work like Claude Code's queue: they reach the
// agent at its next step, in the same turn; Stop hands them back.

#[tokio::test]
async fn stop_hands_queued_messages_back() {
    let sb = Sandbox::new("interrupt");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[slow] think hard", json!({})).await;
    c.wait_event(&sid, "assistant_delta", 0).await;

    // No step ends during a [slow] turn, so the message waits in the queue.
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "and another" })).await;
    c.wait_for("the queued message", |m| m["op"] == "session" && m["session"]["pending"].as_array().is_some_and(|p| p.len() == 1)).await;
    assert_eq!(pending_texts(&c, &sid), ["and another"]);

    // Stop ends the turn and gives the queue back to the window that stopped it.
    let from = c.mark();
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    let back = c.wait_after(from, "the returned queue", |m| m["op"] == "queue.returned" && m["sessionId"] == sid).await;
    assert_eq!(back["text"], "and another");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "interrupted");
    c.wait_status(&sid, "idle").await;
    c.drain_for(Duration::from_millis(600)).await;
    assert_eq!(user_texts(&c, &sid), ["[slow] think hard"], "nothing queued was sent");
    assert!(pending_texts(&c, &sid).is_empty());
}

#[tokio::test]
async fn a_queued_message_can_be_taken_back_to_edit() {
    let sb = Sandbox::new("unqueue");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[slow] think hard", json!({})).await;
    c.wait_event(&sid, "assistant_delta", 0).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "fix the typo" })).await;
    c.wait_for("the queued message", |m| m["op"] == "session" && m["session"]["pending"].as_array().is_some_and(|p| p.len() == 1)).await;
    let id = meta_of(&c, &sid)["pending"][0]["id"].as_str().unwrap().to_string();
    let from = c.mark();
    c.send(json!({ "op": "session.unqueue", "sessionId": sid, "id": id })).await;
    let back = c.wait_after(from, "the taken message", |m| m["op"] == "queue.returned" && m["sessionId"] == sid).await;
    assert_eq!(back["text"], "fix the typo");
    assert_eq!(back["id"], id);
    c.wait_after(from, "the empty queue", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["pending"].as_array().map_or(true, |p| p.is_empty())).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
}

#[tokio::test]
async fn codex_speed_is_used_for_new_turns_and_survives_restarts() {
    let sb = Sandbox::new("codex-speed");
    let d = sb.start();
    let mut c = d.client().await;
    c.wait_for("Codex models", |m| m["models"]["codex"].as_array().is_some_and(|models| !models.is_empty())).await;
    let sid = c.create(&sb.folder("proj"), "first", json!({ "serviceTier": "ultrafast" })).await;
    c.wait_turn_done(&sid, 0).await;
    let tier = || {
        fs::read_to_string(sb.fake_state().join("turn-inputs.jsonl")).unwrap()
            .lines().last().map(|line| serde_json::from_str::<Value>(line).unwrap()["serviceTierForTurn"].clone()).unwrap()
    };
    assert_eq!(tier(), "ultrafast");
    c.send(json!({ "op": "session.setServiceTier", "sessionId": sid, "serviceTier": "default" })).await;
    c.wait_for("standard speed", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["serviceTier"] == "default").await;
    let deadline = Instant::now() + WAIT;
    while saved_meta(&sb, &sid).map(|m| m["serviceTier"] == "default") != Some(true) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "second" })).await;
    c.wait_for("second reply", |m| is_event(m, &sid) && m["event"]["text"] == "Echo: second").await;
    assert_eq!(tier(), "default");
}

#[tokio::test]
async fn codex_subagent_activity_keeps_one_agent_and_its_report() {
    let sb = Sandbox::new("codex-subagent");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("repo"), "[subagent] TEST-codex delegation", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    let events = c.events(&sid);
    let started: Vec<_> = events.iter().filter(|ev| ev["type"] == "tool_started" && ev["agent"] == "Codex").collect();
    assert_eq!(started.len(), 1, "{events:?}");
    assert_eq!(started[0]["detail"], "TEST-check the migration");
    assert!(events.iter().any(|ev| ev["type"] == "tool_completed"
        && ev["itemId"] == started[0]["itemId"]
        && ev["ok"] == true
        && ev["output"] == "TEST-Codex subagent report"), "{events:?}");
}

#[tokio::test]
async fn a_message_sent_mid_turn_is_steered_into_the_codex_turn() {
    let sb = Sandbox::new("steer");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[steps] TEST-go", json!({})).await;
    c.wait_event(&sid, "tool_started", 0).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-also this" })).await;

    let done = c.wait_turn_done(&sid, 0).await;
    assert_eq!(done["status"], "completed");
    let answer = c.events(&sid).into_iter().rev().find(|e| e["type"] == "assistant_message").unwrap();
    assert_eq!(answer["text"], "Echo: [steps] TEST-go | steered: TEST-also this");
    c.wait_status(&sid, "idle").await;
    c.drain_for(Duration::from_millis(600)).await;
    assert_eq!(user_texts(&c, &sid), ["[steps] TEST-go", "TEST-also this"]);
    let steered = c.events(&sid).into_iter().find(|e| e["type"] == "user_message" && e["text"] == "TEST-also this").unwrap();
    assert_eq!(steered["steered"], true, "marked as sent while the agent worked");
    assert_eq!(count(&c, &sid, "turn_started"), 1, "one turn, not two");
    assert_eq!(count(&c, &sid, "turn_completed"), 1);
    assert_eq!(sb.turn_inputs().len(), 1);
    assert!(pending_texts(&c, &sid).is_empty());
}

#[tokio::test]
async fn a_message_sent_mid_turn_is_folded_into_the_claude_turn() {
    let sb = Sandbox::new("fold");
    let sidecar = fake_bin().join("yard-claude");
    let d = sb.start_with(&[("YARD_SIDECAR", sidecar.to_str().unwrap())]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[steps] TEST-go", json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "medium" })).await;
    c.wait_event(&sid, "tool_started", 0).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-and this" })).await;

    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let answer = c.events(&sid).into_iter().rev().find(|e| e["type"] == "assistant_message").unwrap();
    assert_eq!(answer["text"], "Echo: [steps] TEST-go | also: TEST-and this");
    c.wait_status(&sid, "idle").await;
    c.drain_for(Duration::from_millis(600)).await;
    assert_eq!(user_texts(&c, &sid), ["[steps] TEST-go", "TEST-and this"]);
    assert_eq!(count(&c, &sid, "turn_completed"), 1, "folded into the same turn");
}

#[tokio::test]
async fn messages_after_the_last_step_go_together_as_the_next_turn() {
    let sb = Sandbox::new("tail");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[tail] TEST-go", json!({})).await;
    c.wait_event(&sid, "tool_completed", 0).await;
    // The last step is done: these wait for the turn to end, then go as one message.
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-one" })).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-two" })).await;

    c.wait_for("the combined reply", |m| is_event(m, &sid) && m["event"]["type"] == "assistant_message" && m["event"]["text"] == "Echo: TEST-one\n\nTEST-two").await;
    let after = c.next_seq(&sid);
    c.wait_for("the second turn's end", |m| is_event(m, &sid) && m["event"]["type"] == "turn_completed" && m["seq"].as_u64() >= Some(after)).await;
    assert_eq!(user_texts(&c, &sid), ["[tail] TEST-go", "TEST-one\n\nTEST-two"]);
    assert_eq!(count(&c, &sid, "turn_completed"), 2);
    let inputs: Vec<String> = sb.turn_inputs().iter().map(|i| i[0]["text"].as_str().unwrap_or("").to_string()).collect();
    assert_eq!(inputs, ["[tail] TEST-go", "TEST-one\n\nTEST-two"]);
}

#[tokio::test]
async fn messages_queued_during_an_approval_reach_the_agent_after_it() {
    let sb = Sandbox::new("queue");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] run it", json!({})).await;
    let req = c.wait_event(&sid, "approval_request", 0).await;
    c.wait_status(&sid, "needs_approval").await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "first queued" })).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "second queued" })).await;
    c.wait_for("two queued messages", |m| m["op"] == "session" && m["session"]["pending"].as_array().is_some_and(|p| p.len() == 2)).await;
    assert_eq!(user_texts(&c, &sid), ["[cmd] run it"], "queued messages are not in the thread yet");

    // The approved command is a step: both go to the agent as one message,
    // steered into this turn (the fake keeps working briefly after it, as
    // an agent does, and names what was steered in).
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    c.wait_status(&sid, "idle").await;
    c.drain_for(Duration::from_millis(600)).await;
    assert_eq!(user_texts(&c, &sid), ["[cmd] run it", "first queued\n\nsecond queued"]);
    assert_eq!(count(&c, &sid, "turn_completed"), 1, "one turn");
    let answer = c.events(&sid).into_iter().rev().find(|e| e["type"] == "assistant_message").unwrap();
    assert_eq!(answer["text"], "Ran it. | steered: first queued\n\nsecond queued");
    assert!(pending_texts(&c, &sid).is_empty());
}

// The owner's messages reach the agent in the order he sent them, however
// fast they came. One worker thread makes the daemon run ops it read
// together in a fixed order, so one that hands each op to its own task
// swaps them every time instead of now and then under load.
#[tokio::test]
async fn messages_sent_in_a_row_reach_the_agent_in_the_order_sent() {
    let sb = Sandbox::new("send-order");
    let d = sb.start_with(&[("TOKIO_WORKER_THREADS", "1")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "TEST-hi", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let send = |text: &str| json!({ "op": "session.send", "sessionId": sid, "text": text });

    // To an idle session: the first starts the turn, the second waits for it.
    let seq = c.next_seq(&sid);
    c.send_burst(&[send("TEST-one"), send("TEST-two")]).await;
    let done = c.wait_turn_done(&sid, seq).await;
    c.wait_turn_done(&sid, done["seq"].as_u64().unwrap() + 1).await;
    c.wait_status(&sid, "idle").await;
    assert_eq!(user_texts(&c, &sid), ["TEST-hi", "TEST-one", "TEST-two"]);

    // Behind an approval: queued as sent.
    let seq = c.next_seq(&sid);
    c.send(send("[cmd] TEST-run it")).await;
    c.wait_event(&sid, "approval_request", seq).await;
    c.wait_status(&sid, "needs_approval").await;
    let queued = ["TEST-a", "TEST-b", "TEST-c", "TEST-d"];
    c.send_burst(&queued.map(send)).await;
    c.wait_for("four queued messages", |m| m["op"] == "session" && m["session"]["pending"].as_array().is_some_and(|p| p.len() == 4)).await;
    assert_eq!(pending_texts(&c, &sid), queued);
}

// A session's settings too: a group set and then cleared stays cleared
// (seen as sessions_are_grouped_across_restarts failing under load).
#[tokio::test]
async fn session_settings_sent_in_a_row_apply_in_the_order_sent() {
    let sb = Sandbox::new("update-order");
    let d = sb.start_with(&[("TOKIO_WORKER_THREADS", "1")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "TEST-hi", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    let update = |change: Value| {
        let mut op = json!({ "op": "session.update", "sessionId": sid });
        op.as_object_mut().unwrap().extend(change.as_object().unwrap().clone());
        op
    };
    c.send_burst(&[update(json!({ "group": "TEST-release" })), update(json!({ "group": null }))]).await;
    c.wait_for("the group cleared", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["group"].is_null()).await;
    // Sent after, so its answer shows where both left the session.
    c.send(update(json!({ "pinned": true }))).await;
    let pinned = c.wait_for("pinned", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["pinned"] == true).await;
    assert!(pinned["session"]["group"].is_null(), "{pinned}");
}

#[tokio::test]
async fn a_session_says_when_and_how_its_last_turn_ended() {
    let sb = Sandbox::new("turn-end");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "say hello", json!({})).await;
    assert!(meta_of(&c, &sid)["turnEndedAt"].is_null(), "no turn has ended yet");
    let done = c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let meta = meta_of(&c, &sid);
    assert_eq!(meta["lastTurn"], "completed");
    let first = meta["turnEndedAt"].as_u64().expect("turnEndedAt is set");

    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[fail] again" })).await;
    c.wait_turn_done(&sid, done["seq"].as_u64().unwrap() + 1).await;
    c.wait_for("the failed turn's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["lastTurn"] == "failed").await;
    assert!(meta_of(&c, &sid)["turnEndedAt"].as_u64().unwrap() >= first);
}

#[tokio::test]
async fn an_error_turn_reports_and_fails() {
    let sb = Sandbox::new("error");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[fail] please", json!({})).await;

    let err = c.wait_event(&sid, "error", 0).await;
    assert_eq!(err["message"], "fake failure: the model is unavailable");
    let done = c.wait_turn_done(&sid, 0).await;
    assert_eq!(done["status"], "failed");
    assert!(err["seq"].as_u64() < done["seq"].as_u64());
    c.wait_status(&sid, "idle").await;
}

#[tokio::test]
async fn sessions_survive_a_restart_and_resume_their_thread() {
    let sb = Sandbox::new("persist");
    let cwd = sb.folder("proj");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&cwd, "remember me", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    d.stop();

    let d = sb.start();
    let mut c = d.client().await;
    let snap = c.log[0].clone();
    let restored = snap["sessions"].as_array().unwrap().iter().find(|s| s["id"] == sid).expect("session is back");
    assert_eq!(restored["status"], "idle");
    assert_eq!(restored["cwd"], cwd);
    let history = c.history(&sid).await;
    assert_eq!(types(&history), ["user_message", "turn_started", "assistant_message", "turn_completed"], "no deltas saved");
    assert_eq!(history[2]["text"], "Echo: remember me");

    // A follow-up reopens the saved thread on the (new) fake.
    let thread = sb.codex_log().lines().find_map(|l| l.strip_prefix("req turn/start ")).and_then(|r| r.split_whitespace().next()).unwrap().to_string();
    let next = history.len() as u64;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "still there?" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    assert!(sb.codex_log().contains(&format!("req thread/resume {thread}")), "{}", sb.codex_log());
    assert_eq!(sb.codex_log().matches("req thread/start").count(), 1);

    // Killed in the middle of a turn: back as idle with the restart error.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[slow] long job" })).await;
    c.wait_event(&sid, "assistant_delta", next).await;
    let deadline = Instant::now() + WAIT;
    while saved_status(&sb, &sid).as_deref() != Some("running") {
        assert!(Instant::now() < deadline, "the running status never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();

    let d = sb.start();
    let mut c = d.client().await;
    let back = c.log[0]["sessions"].as_array().unwrap().iter().find(|s| s["id"] == sid).unwrap().clone();
    assert_eq!(back["status"], "idle");
    let history = c.history(&sid).await;
    let n = history.len();
    assert_eq!(history[n - 2], json!({ "type": "error", "message": "yardd restarted during this turn." }));
    assert_eq!(history[n - 1]["type"], "turn_completed");
    assert_eq!(history[n - 1]["status"], "failed");
    assert!(history[n - 1]["at"].is_u64(), "turn ends carry yardd's clock: {}", history[n - 1]);
}

#[tokio::test]
async fn resuming_a_thread_the_engine_lost_fails_the_turn() {
    let sb = Sandbox::new("lost");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    d.stop();
    fs::remove_file(sb.fake_state().join("threads.json")).unwrap();

    let d = sb.start();
    let mut c = d.client().await;
    let next = c.history(&sid).await.len() as u64;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "hello?" })).await;
    let err = c.wait_event(&sid, "error", next).await;
    assert!(err["message"].as_str().unwrap().contains("no such thread"), "{err}");
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "failed");
}

#[tokio::test]
async fn delete_removes_a_session_across_a_restart() {
    let sb = Sandbox::new("delete");
    let d = sb.start();
    let mut c = d.client().await;
    let keep = c.create(&sb.folder("a"), "keep me", json!({})).await;
    let gone = c.create(&sb.folder("b"), "delete me", json!({})).await;
    c.wait_turn_done(&keep, 0).await;
    c.wait_turn_done(&gone, 0).await;
    c.send(json!({ "op": "session.delete", "sessionId": gone })).await;
    c.wait_for("session.deleted", |m| m["op"] == "session.deleted" && m["sessionId"] == gone).await;
    d.stop();

    let d = sb.start();
    let mut c = d.client().await;
    let ids: Vec<&str> = c.log[0]["sessions"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [keep.as_str()]);
    assert_eq!(c.history(&gone).await, Vec::<Value>::new());
}

#[tokio::test]
async fn forced_pressure_queues_until_start_now() {
    let sb = Sandbox::new("queue");
    let d = sb.start_with(&[("YARD_FORCE_PRESSURE", "1")]);
    let mut c = d.client().await;
    // The attachment waits in the queue with the prompt.
    let shot = c.upload("shot.png", "image/png", &png(40), json!({})).await.unwrap();
    let sid = c.try_create(&sb.folder("proj"), "wait your turn", json!({ "attachments": [shot["id"]] })).await.unwrap();

    let queued = c.wait_event(&sid, "queued", 0).await;
    assert_eq!(queued["text"], "wait your turn");
    assert_eq!(queued["attachments"][0]["id"], shot["id"]);
    assert!(queued["reason"].as_str().unwrap().contains("YARD_FORCE_PRESSURE"));
    c.wait_status(&sid, "queued").await;
    assert_eq!(c.event_types(&sid), ["user_message", "queued"]);

    // Queued sessions take no messages until started.
    let from = c.mark();
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "now?" })).await;
    assert!(c.wait_error(from, "session.send").await.contains("queued"));
    assert!(!sb.codex_log().contains("req turn/start"), "no turn reached the engine");

    c.send(json!({ "op": "session.startNow", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let answer = c.wait_event(&sid, "assistant_message", 0).await;
    assert!(answer["text"].as_str().unwrap().starts_with("Echo: wait your turn\n\nAttached files:"), "{answer}");
    assert_eq!(split_input(&sb.turn_inputs()[0]).1, [shot["path"].as_str().unwrap()]);
    c.wait_status(&sid, "idle").await;

    let from = c.mark();
    c.send(json!({ "op": "session.startNow", "sessionId": sid })).await;
    assert_eq!(c.wait_error(from, "session.startNow").await, "this session is not queued");
}

#[tokio::test]
async fn one_writer_per_folder() {
    let sb = Sandbox::new("writer");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");
    let first = c.create(&cwd, "[slow] busy", json!({})).await;
    c.wait_event(&first, "assistant_delta", 0).await;

    let refused = c.try_create(&cwd, "me too", json!({})).await.unwrap_err();
    // It names the session in the way, and offers nothing the user cannot do:
    // there is no "New worktree" box, and this folder could not have one.
    assert!(refused.starts_with("\"[slow] busy\" is working in this folder."), "{refused}");
    assert!(!refused.contains("worktree"), "{refused}");
    let asked = c.try_create(&cwd, "me too", json!({ "worktree": true })).await.unwrap_err();
    assert_eq!(asked, refused);
    // The same folder spelled with a trailing slash is still the same folder.
    let slash = c.try_create(&format!("{cwd}/"), "me too", json!({})).await.unwrap_err();
    assert_eq!(slash, refused);

    c.send(json!({ "op": "session.interrupt", "sessionId": first })).await;
    c.wait_turn_done(&first, 0).await;
    c.wait_status(&first, "idle").await;
    let second = c.create(&cwd, "my turn now", json!({})).await;
    c.wait_turn_done(&second, 0).await;
}

#[tokio::test]
async fn worktree_branches_and_a_session_from_a_base() {
    let sb = Sandbox::new("worktree");
    let repo = make_repo(&sb, "repo");
    let d = sb.start();
    let mut c = d.client().await;

    c.send(json!({ "op": "git.branches", "cwd": repo })).await;
    let branches = c.wait_for("git.branches", |m| m["op"] == "git.branches").await;
    assert_eq!(branches["current"], "main");
    let mut names: Vec<&str> = branches["branches"].as_array().unwrap().iter().map(|b| b.as_str().unwrap()).collect();
    names.sort();
    assert_eq!(names, ["feature", "main"]);

    let sid = c.create(&repo, "work on the feature", json!({ "worktree": true, "base": "feature" })).await;
    let meta = c.wait_for("the worktree session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    let branch = meta["branch"].as_str().unwrap();
    assert!(branch.starts_with("yard/work-on-the-feature-"), "{branch}");
    assert_eq!(meta["repo"], repo);
    let wt = meta["cwd"].as_str().unwrap();
    assert!(Path::new(wt).starts_with(sb.data().join("wt")), "{wt}");
    let home = sb.home();
    assert_eq!(git(wt, &["rev-parse", "HEAD"], &home), git(&repo, &["rev-parse", "feature"], &home));
    assert!(Path::new(wt).join("feature.txt").is_file());
    c.wait_turn_done(&sid, 0).await;
    assert_eq!(git(&repo, &["branch", "--show-current"], &home), "main", "the original checkout stays put");

    let refused = c.try_create(&repo, "bad base", json!({ "worktree": true, "base": "nope" })).await.unwrap_err();
    assert!(refused.contains("is not a branch or commit"), "{refused}");
}

/// New session asks for the branches of the folder it remembers. A folder
/// deleted since must read as gone, not as a plain folder that Send would
/// then refuse.
#[tokio::test]
async fn branches_of_a_missing_folder_say_it_is_missing() {
    let sb = Sandbox::new("branches-missing");
    let plain = sb.folder("plain");
    let gone = format!("{plain}-gone");
    let d = sb.start();
    let mut c = d.client().await;

    c.send(json!({ "op": "git.branches", "cwd": plain })).await;
    let reply = c.wait_for("plain folder branches", |m| m["op"] == "git.branches" && m["cwd"] == plain).await;
    assert!(reply["error"].is_string(), "{reply}");
    assert_eq!(reply.get("missing"), None, "an existing folder is not missing: {reply}");

    c.send(json!({ "op": "git.branches", "cwd": gone })).await;
    let reply = c.wait_for("missing folder branches", |m| m["op"] == "git.branches" && m["cwd"] == gone).await;
    assert_eq!(reply["missing"], true, "{reply}");
    // An answer, not a failure: the page reads `missing`.
    assert_eq!(reply.get("error"), None, "{reply}");
}

#[tokio::test]
async fn a_folder_outside_a_repository_runs_in_place() {
    let sb = Sandbox::new("inplace");
    let plain = sb.folder("plain");
    let d = sb.start();
    let mut c = d.client().await;

    // The UI always asks for a worktree; a folder that cannot have one is
    // not an error, it just runs there.
    let sid = c.create(&plain, "no repo here", json!({ "worktree": true })).await;
    let meta = c.wait_for("the session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    assert_eq!(meta["cwd"], plain);
    assert!(meta["branch"].is_null(), "{meta}");
    let note = c.wait_event(&sid, "note", 0).await;
    assert!(note["text"].as_str().unwrap().contains("in the folder itself"), "{note}");
    c.wait_turn_done(&sid, 0).await;
}

#[tokio::test]
async fn folders_are_listed_for_the_picker() {
    let sb = Sandbox::new("dirs");
    let repo = make_repo(&sb, "repo");
    let parent = Path::new(&repo).parent().unwrap().to_str().unwrap().to_string();
    fs::create_dir_all(Path::new(&parent).join(".hidden")).unwrap();
    fs::write(Path::new(&parent).join("a-file.txt"), "not a folder").unwrap();
    let d = sb.start();
    let mut c = d.client().await;

    c.send(json!({ "op": "fs.dirs", "path": parent })).await;
    let reply = c.wait_for("fs.dirs", |m| m["op"] == "fs.dirs").await;
    let dirs = reply["dirs"].as_array().unwrap();
    let repo_entry = dirs.iter().find(|d| d["name"] == "repo").expect("the repo is listed");
    assert_eq!(repo_entry["git"], true);
    assert!(dirs.iter().any(|d| d["name"] == ".hidden"), "hidden folders are sent; the UI filters them");
    assert!(!dirs.iter().any(|d| d["name"] == "a-file.txt"), "files are not folders");

    c.send(json!({ "op": "fs.dirs", "path": "relative/path" })).await;
    let refused = c.wait_for("fs.dirs refusal", |m| m["op"] == "fs.dirs" && m["path"] == "relative/path").await;
    assert!(refused["error"].as_str().unwrap().contains("absolute"), "{refused}");

    // A typo higher up the path names the first folder that is not there,
    // not the io error of the folder the picker asked for.
    let gone = Path::new(&parent).join("TEST-gone");
    let deep = gone.join("deeper").to_str().unwrap().to_string();
    c.send(json!({ "op": "fs.dirs", "path": deep })).await;
    let missing = c.wait_for("fs.dirs missing", |m| m["op"] == "fs.dirs" && m["path"] == deep.as_str()).await;
    assert_eq!(missing["missing"], gone.to_str().unwrap(), "{missing}");
    assert!(missing["error"].is_null(), "{missing}");

    // A path through a file names that file, typed with or without the
    // trailing slash, not "Not a directory (os error 20)".
    let file = Path::new(&parent).join("a-file.txt");
    for through in [format!("{}/", file.to_str().unwrap()), file.join("deeper").to_str().unwrap().to_string()] {
        c.send(json!({ "op": "fs.dirs", "path": through })).await;
        let reply = c.wait_for("fs.dirs file", |m| m["op"] == "fs.dirs" && m["path"] == through.as_str()).await;
        assert_eq!(reply["file"], file.to_str().unwrap(), "{reply}");
        assert!(reply["error"].is_null(), "{reply}");
    }
}

#[tokio::test]
async fn a_failing_worktree_setup_makes_no_session() {
    let sb = Sandbox::new("setup");
    let repo = make_repo(&sb, "repo");
    let home = sb.home();
    fs::create_dir_all(Path::new(&repo).join(".yard")).unwrap();
    fs::write(Path::new(&repo).join(".yard/worktree-setup.sh"), "echo setting up\necho broken >&2\nexit 3\n").unwrap();
    git(&repo, &["add", "."], &home);
    git(&repo, &["commit", "-q", "-m", "setup script"], &home);
    let head = git(&repo, &["rev-parse", "HEAD"], &home);
    let d = sb.start();
    let mut c = d.client().await;

    let refused = c.try_create(&repo, "set me up", json!({ "worktree": true })).await.unwrap_err();
    assert!(refused.contains("worktree setup"), "{refused}");
    assert!(refused.contains("broken"), "the script's output is shown: {refused}");
    assert!(!c.log.iter().any(|m| m["op"] == "session"), "no session was made");

    assert_eq!(git(&repo, &["status", "--porcelain"], &home), "");
    assert_eq!(git(&repo, &["rev-parse", "HEAD"], &home), head);
    assert_eq!(git(&repo, &["branch", "--show-current"], &home), "main");
    assert_eq!(git(&repo, &["branch", "--format=%(refname:short)"], &home), "feature\nmain");
    assert_eq!(git(&repo, &["worktree", "list"], &home).lines().count(), 1);
}

#[tokio::test]
async fn removing_a_dirty_worktree_is_refused() {
    let sb = Sandbox::new("rmwt");
    let repo = make_repo(&sb, "repo");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&repo, "scratch", json!({ "worktree": true })).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let session = c.wait_for("the worktree session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    let wt = session["cwd"].as_str().unwrap().to_string();

    fs::write(Path::new(&wt).join("notes.txt"), "unsaved\n").unwrap();
    let from = c.mark();
    c.send(json!({ "op": "session.delete", "sessionId": sid, "removeWorktree": true })).await;
    let refused = c.wait_error(from, "session.delete").await;
    assert!(refused.contains("uncommitted or untracked"), "{refused}");
    // Deleting from the sidebar shows the refusal over whatever session is
    // open, so it names the one that was refused.
    let title = session["title"].as_str().unwrap();
    assert!(refused.starts_with(&format!("\"{title}\" was not deleted: ")), "{refused}");
    assert!(Path::new(&wt).join("notes.txt").is_file(), "nothing was deleted");

    // Clean again, it goes, with its branch.
    fs::remove_file(Path::new(&wt).join("notes.txt")).unwrap();
    c.send(json!({ "op": "session.delete", "sessionId": sid, "removeWorktree": true })).await;
    let removed = c.wait_for("worktree.removed", |m| m["op"] == "worktree.removed").await;
    assert_eq!(removed["branchDeleted"], true);
    assert!(!Path::new(&wt).exists());
}

#[tokio::test]
async fn handoff_to_the_same_target_or_mid_turn_is_refused() {
    let sb = Sandbox::new("handoff");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[slow] start", json!({})).await;
    c.wait_event(&sid, "assistant_delta", 0).await;

    // To Claude while a turn runs: refused before any Claude session exists.
    let from = c.mark();
    c.send(json!({ "op": "session.handoff", "sessionId": sid, "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" }))
        .await;
    assert_eq!(c.wait_error(from, "session.handoff").await, "stop the running turn before handing off");

    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;

    let from = c.mark();
    c.send(json!({ "op": "session.handoff", "sessionId": sid, "engine": "codex", "model": "fake-fast", "effort": "low" }))
        .await;
    assert!(c.wait_error(from, "session.handoff").await.contains("already runs on Codex"));
    assert!(!c.log.iter().any(|m| m["op"] == "created" && m["sessionId"] != sid));
}

#[tokio::test]
async fn handoff_can_change_the_account_or_model_without_changing_engines() {
    let sb = Sandbox::new("handoff-target");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "original work", json!({})).await;
    c.wait_status(&sid, "idle").await;
    let account = add_account(&mut c, "codex", "TEST-Other").await;
    for (model, target_account) in [("fake-fast", account.as_str()), ("fake-deep", "default")] {
        let from = c.mark();
        c.send(json!({ "op": "session.handoff", "sessionId": sid, "engine": "codex", "model": model, "effort": "low", "account": target_account })).await;
        let created = c.wait_after(from, "handover created", |m| m["op"] == "created" || m["op"] == "error").await;
        assert_eq!(created["op"], "created", "{created}");
        let id = created["sessionId"].as_str().unwrap();
        let child = c.wait_for("handover parent and target", |m| m["op"] == "session" && m["session"]["id"] == id && m["session"]["parent"] == sid).await;
        assert_eq!(child["session"]["model"], model);
        assert_eq!(child["session"]["account"], target_account);
        c.wait_turn_done(id, 0).await;
        c.wait_status(id, "idle").await;
    }
}

#[tokio::test]
async fn handoff_keeps_the_sidebar_group() {
    let sb = Sandbox::new("handoff-group");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "TEST-hi", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    c.send(json!({ "op": "session.update", "sessionId": sid, "group": "TEST-grp" })).await;
    c.wait_for("the group saved", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["group"] == "TEST-grp")
        .await;

    // The work goes on in the new session, so it belongs where the user filed the old one.
    let from = c.mark();
    c.send(json!({ "op": "session.handoff", "sessionId": sid, "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" }))
        .await;
    let created = c.wait_after(from, "created", |m| m["op"] == "created").await;
    let new_sid = created["sessionId"].as_str().unwrap().to_string();
    let meta = c
        .wait_for("the new session's meta", |m| m["op"] == "session" && m["session"]["id"] == new_sid && m["session"]["parent"] == sid)
        .await;
    assert_eq!(meta["session"]["group"], "TEST-grp");

    // Both sessions keep one title, so each hand-off note names the other one.
    let note = |events: Vec<Value>| events.into_iter().find(|e| e["type"] == "note").expect("a hand-off note");
    assert_eq!(note(c.history(&sid).await)["session"], new_sid.as_str());
    assert_eq!(note(c.history(&new_sid).await)["session"], sid.as_str());
}

#[tokio::test]
async fn terminal_runs_commands_and_replays_its_scrollback() {
    let sb = Sandbox::new("term");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;

    let from = c.mark();
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 80, "rows": 24 })).await;
    let opened = c.wait_after(from, "term.opened", |m| m["op"] == "term.opened").await;
    let term = opened["termId"].as_str().unwrap().to_string();
    assert_eq!(opened["sessionId"], sid);
    assert_eq!(opened["offset"], 0);

    c.send(json!({ "op": "term.input", "termId": term, "data": "echo hi\n" })).await;
    // The typed line echoes back too, so `hi` on a line of its own shows the command ran.
    let output = |c: &Client| -> (u64, Vec<u8>) {
        let mut chunks: Vec<(u64, Vec<u8>)> = c
            .log
            .iter()
            .filter(|m| m["op"] == "term.data" && m["termId"] == term)
            .map(|m| (m["offset"].as_u64().unwrap(), b64(m["data"].as_str().unwrap())))
            .collect();
        chunks.sort_by_key(|(o, _)| *o);
        let mut all = b64(opened["data"].as_str().unwrap());
        for (offset, bytes) in chunks {
            assert_eq!(offset, all.len() as u64, "chunks are contiguous");
            all.extend(bytes);
        }
        (all.len() as u64, all)
    };
    loop {
        let (_, all) = output(&c);
        // bash wraps each command in bracketed-paste toggles (`\e[?2004l\r`),
        // so the last carriage-return segment of a line is what shows.
        let text = String::from_utf8_lossy(&all).into_owned();
        if text.lines().any(|l| l.trim_end_matches('\r').rsplit('\r').next() == Some("hi")) {
            break;
        }
        let from = c.mark();
        c.wait_after(from, "more terminal output with `hi`", |m| m["op"] == "term.data").await;
    }

    // Reattach: the same terminal, its scrollback from offset 0, ending where the stream is.
    let (total, all) = output(&c);
    let from = c.mark();
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 100, "rows": 30 })).await;
    let again = c.wait_after(from, "term.opened again", |m| m["op"] == "term.opened").await;
    assert_eq!(again["termId"], term);
    let scroll = b64(again["data"].as_str().unwrap());
    let offset = again["offset"].as_u64().unwrap();
    assert_eq!(offset, 0);
    // The shell may print more (a prompt) between the two reads; never less.
    assert!(offset + scroll.len() as u64 >= total);
    assert_eq!(&scroll[..all.len()], &all[..]);

    // A second client attaching sees the same scrollback.
    let mut other = d.client().await;
    other.send(json!({ "op": "term.open", "sessionId": sid })).await;
    let theirs = other.wait_for("term.opened on a second client", |m| m["op"] == "term.opened").await;
    assert_eq!(theirs["termId"], term);
    assert!(b64(theirs["data"].as_str().unwrap()).starts_with(&all));
}

#[tokio::test]
async fn terminal_keystrokes_reach_the_shell_in_order() {
    let sb = Sandbox::new("term-order");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;

    let from = c.mark();
    // Wide enough that the typed line never wraps.
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 400, "rows": 24 })).await;
    let opened = c.wait_after(from, "term.opened", |m| m["op"] == "term.opened").await;
    let term = opened["termId"].as_str().unwrap().to_string();

    // One frame per key, as xterm sends them while someone types.
    let word: String = (0..6).map(|_| "abcdefghijklmnopqrstuvwxyz0123456789").collect();
    for ch in format!("echo {word}\n").chars() {
        c.send(json!({ "op": "term.input", "termId": term, "data": ch.to_string() })).await;
    }
    let mut all = b64(opened["data"].as_str().unwrap());
    let mut seen = 0;
    loop {
        for m in &c.log[seen..] {
            if m["op"] == "term.data" && m["termId"] == term {
                all.extend(b64(m["data"].as_str().unwrap()));
            }
        }
        seen = c.log.len();
        let text = String::from_utf8_lossy(&all).into_owned();
        // `echo` prints the word on a line of its own only if every key arrived in order.
        if text.lines().any(|l| l.trim_end_matches('\r').rsplit('\r').next() == Some(word.as_str())) {
            break;
        }
        let from = c.mark();
        c.wait_after(from, "the typed word echoed back unscrambled", |m| m["op"] == "term.data").await;
    }
}

#[tokio::test]
async fn resizing_an_exited_terminal_is_not_an_error() {
    let sb = Sandbox::new("term-exited");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;

    let from = c.mark();
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 80, "rows": 24 })).await;
    let opened = c.wait_after(from, "term.opened", |m| m["op"] == "term.opened").await;
    let term = opened["termId"].as_str().unwrap().to_string();
    c.send(json!({ "op": "term.input", "termId": term, "data": "exit\n" })).await;
    c.wait_after(from, "the shell to exit", |m| m["op"] == "term.exit" && m["termId"] == term).await;

    // The UI fits a terminal, and so resizes it, before the attach reply
    // tells it the shell exited; after a reload that refusal showed as an
    // error banner. A tab that is really gone is still refused.
    let from = c.mark();
    c.send(json!({ "op": "term.resize", "termId": term, "cols": 100, "rows": 30, "ref": "exited" })).await;
    c.send(json!({ "op": "term.resize", "termId": "no-such-term", "cols": 100, "rows": 30, "ref": "gone" })).await;
    // Resizes run in order, so once the second is refused the first was handled.
    c.wait_after(from, "the gone terminal's resize refused", |m| m["op"] == "error" && m["ref"] == "gone").await;
    let refused: Vec<&Value> = c.log[from..].iter().filter(|m| m["op"] == "error" && m["ref"] == "exited").collect();
    assert!(refused.is_empty(), "resizing an exited terminal was refused: {refused:?}");

    // Its tab and scrollback stay until it is closed.
    let from = c.mark();
    c.send(json!({ "op": "term.attach", "termId": term, "cols": 100, "rows": 30 })).await;
    let again = c.wait_after(from, "term.opened", |m| m["op"] == "term.opened").await;
    assert_eq!(again["exited"], true);
}

#[tokio::test]
async fn deleting_a_session_does_not_ask_its_terminal_pane_to_reload() {
    let sb = Sandbox::new("term-delete");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({})).await;

    let from = c.mark();
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 80, "rows": 24 })).await;
    c.wait_after(from, "term.opened", |m| m["op"] == "term.opened").await;
    // Opening it changed the list too; that broadcast may trail the reply.
    c.wait_after(from, "term.listChanged for the open", |m| m["op"] == "term.listChanged" && m["sessionId"] == sid).await;

    // An open terminal pane answers `term.listChanged` with `term.list`. Sent
    // for a session being deleted, that ask reached yardd after the session
    // was gone, and the refusal ("no such session") showed as an error banner
    // over the page the user landed on. `session.deleted` already closes the pane.
    let from = c.mark();
    c.send(json!({ "op": "session.delete", "sessionId": sid })).await;
    c.wait_after(from, "session.deleted", |m| m["op"] == "session.deleted" && m["sessionId"] == sid).await;
    let reloads: Vec<&Value> = c.log[from..].iter().filter(|m| m["op"] == "term.listChanged" && m["sessionId"] == sid).collect();
    assert!(reloads.is_empty(), "a deleted session's terminal pane was told to reload its list: {reloads:?}");
}

fn b64(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}

#[tokio::test]
async fn foreign_origins_are_refused() {
    let sb = Sandbox::new("origin");
    let d = sb.start();
    let connect = |origin: &'static str| {
        let mut req = d.url().into_client_request().unwrap();
        req.headers_mut().insert("Origin", HeaderValue::from_str(&origin.replace("PORT", &d.port.to_string())).unwrap());
        tokio_tungstenite::connect_async(req)
    };

    match connect("http://evil.example").await {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => assert_eq!(resp.status(), 403),
        Err(e) => panic!("expected a 403, got {e}"),
        Ok(_) => panic!("a foreign origin was let in"),
    }
    match connect("http://127.0.0.1:PORT.evil.example").await {
        Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => assert_eq!(resp.status(), 403),
        other => panic!("a look-alike origin was not refused: {:?}", other.err()),
    }
    let (_ws, resp) = connect("http://127.0.0.1:PORT").await.expect("own origin is accepted");
    assert_eq!(resp.status(), 101);
    let (_ws, resp) = connect("http://localhost:PORT/").await.expect("localhost origin is accepted");
    assert_eq!(resp.status(), 101);
}

#[tokio::test]
async fn a_window_cannot_ask_for_an_agents_hand_over() {
    use futures_util::{SinkExt, StreamExt};
    let sb = Sandbox::new("handover-origin");
    let d = sb.start();
    let mut req = d.url().into_client_request().unwrap();
    req.headers_mut().insert("Origin", HeaderValue::from_str(&format!("http://127.0.0.1:{}", d.port)).unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    // A hand-over carries an agent's Slack tokens: a window never gets one.
    for op in ["agent.export", "agent.adopt", "agent.release", "agent.deliver"] {
        ws.send(tokio_tungstenite::tungstenite::Message::Text(json!({ "op": op, "id": "g1", "ref": op }).to_string())).await.unwrap();
        let reply = loop {
            let tokio_tungstenite::tungstenite::Message::Text(t) = ws.next().await.unwrap().unwrap() else { continue };
            let v: Value = serde_json::from_str(&t).unwrap();
            if v["ref"] == op {
                break v;
            }
        };
        assert_eq!(reply["op"], "error", "{reply}");
        assert!(reply["message"].as_str().unwrap().contains("only another Switchyard"), "{reply}");
    }
}

#[tokio::test]
async fn allow_origin_takes_a_list() {
    let sb = Sandbox::new("origins");
    let d = sb.start_with(&[("YARD_ALLOW_ORIGIN", " https://yard.example.com/ , http://100.64.0.9:47717")]);
    let connect = |origin: &'static str| {
        let mut req = d.url().into_client_request().unwrap();
        req.headers_mut().insert("Origin", HeaderValue::from_static(origin));
        tokio_tungstenite::connect_async(req)
    };
    for origin in ["https://yard.example.com", "http://100.64.0.9:47717", "http://100.64.0.9:47717/"] {
        let (_ws, resp) = connect(origin).await.unwrap_or_else(|e| panic!("{origin} was refused: {e}"));
        assert_eq!(resp.status(), 101);
    }
    for origin in ["https://yard.example.com.evil.example", "http://yard.example.com", "http://evil.example"] {
        match connect(origin).await {
            Err(tokio_tungstenite::tungstenite::Error::Http(resp)) => assert_eq!(resp.status(), 403, "{origin}"),
            other => panic!("{origin} was not refused: {:?}", other.err()),
        }
    }
}

#[tokio::test]
async fn the_local_host_carries_its_configured_name() {
    let sb = Sandbox::new("hostname");
    let d = sb.start_with(&[("YARD_HOST_NAME", "yard-box")]);
    let mut c = d.client().await;
    let host = c
        .wait_for("the local host stats", |m| {
            (m["op"] == "host" || m["op"] == "snapshot") && m["host"]["id"] == "local"
        })
        .await;
    assert_eq!(host["host"]["name"], "yard-box");

    // A remote may not take the local host's name.
    c.send(json!({ "op": "host.add", "name": "Yard-Box", "kind": "direct", "url": "ws://127.0.0.1:1/ws" })).await;
    let err = c.wait_for("the duplicate name refused", |m| m["op"] == "error").await;
    assert!(err["message"].as_str().unwrap_or("").contains("already exists"), "{err}");
}

#[tokio::test]
async fn a_hub_runs_sessions_on_a_direct_host() {
    let remote_sb = Sandbox::new("remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;

    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    assert_eq!(hosts["hosts"][0]["name"], "box");
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;

    let cwd = remote_sb.folder("proj");
    let sid = c.create(&cwd, "hello remote", json!({ "host": host })).await;
    let (prefix, remote_id) = sid.split_once('~').expect("a prefixed id");
    assert_eq!(prefix, host);
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert_eq!(c.wait_event(&sid, "assistant_message", 0).await["text"], "Echo: hello remote");
    let meta = c.wait_for("the remote session", |m| m["op"] == "session" && m["session"]["id"] == sid).await;
    assert_eq!(meta["session"]["host"], host);

    // It ran on the remote, under its own id, and the hub has no session of its own.
    let direct = remote.client().await;
    let ids: Vec<&str> = direct.log[0]["sessions"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [remote_id]);
    assert!(remote_sb.codex_log().contains("req turn/start"));
    assert!(!hub_sb.codex_log().contains("req turn/start"));
    let hub_snapshot = hub.client().await.log[0].clone();
    assert_eq!(hub_snapshot["sessions"], json!([]));

    // Ops with the prefixed id reach the remote.
    let history = c.history(&sid).await;
    let kept: Vec<&str> = types(&history).into_iter().filter(|t| *t != "assistant_delta").collect();
    assert_eq!(kept, ["user_message", "turn_started", "assistant_message", "turn_completed"]);
}

#[tokio::test]
async fn a_page_s_first_op_reaches_a_connected_host_while_its_own_connection_opens() {
    // Every page gets its own connection from the hub to each host, opened
    // when the page connects. A page that asks right away (a reload, a phone
    // coming back, with a remote conversation open) must not be told the
    // host is not connected while the hub shows it connected.
    use futures_util::{SinkExt, StreamExt};
    let remote_sb = Sandbox::new("remote-first-op");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("hub-first-op");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;
    let sid = c.create(&remote_sb.folder("proj"), "hello remote", json!({ "host": host })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");

    for n in 0..3 {
        // The page's first op, sent before it has read anything.
        let (mut ws, _) = tokio_tungstenite::connect_async(hub.url()).await.unwrap();
        ws.send(tokio_tungstenite::tungstenite::Message::Text(json!({ "op": "session.open", "sessionId": sid }).to_string())).await.unwrap();
        let deadline = tokio::time::Instant::now() + WAIT;
        let answer = loop {
            let next = tokio::time::timeout_at(deadline, ws.next()).await.expect("an answer").unwrap().unwrap();
            if let tokio_tungstenite::tungstenite::Message::Text(t) = next {
                let v: Value = serde_json::from_str(&t).unwrap();
                if (v["op"] == "history" && v["sessionId"] == sid.as_str()) || v["op"] == "error" {
                    break v;
                }
            }
        };
        assert_eq!(answer["op"], "history", "page {n}: {answer}");
        assert!(answer["events"].as_array().is_some_and(|e| !e.is_empty()), "page {n}: {answer}");
    }

    // A host that is down still says so at once.
    remote.stop();
    c.wait_for("the host to go offline", |m| {
        m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] != "connected"))
    })
    .await;
    let mut late = hub.client().await;
    let from = late.mark();
    late.send(json!({ "op": "session.open", "sessionId": sid })).await;
    assert_eq!(late.wait_error(from, "session.open").await, "this host is not connected");
}

#[tokio::test]
async fn a_page_opened_while_a_host_is_down_still_lists_its_sessions() {
    let remote_sb = Sandbox::new("remote-down");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("hub-down");
    let hub = hub_sb.start();
    let mut c = hub.client().await;

    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;
    let sid = c.create(&remote_sb.folder("proj"), "hello remote", json!({ "host": host })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");

    remote.stop();
    c.wait_for("the host to go offline", |m| {
        m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] != "connected"))
    })
    .await;

    // A page opened now (a reload, another window) still lists the host's
    // sessions, marked offline by the host.down that follows.
    let mut late = hub.client().await;
    let snap = late.wait_for("the offline host's last snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;
    let ids: Vec<&str> = snap["sessions"].as_array().unwrap().iter().filter_map(|s| s["id"].as_str()).collect();
    assert_eq!(ids, [sid.as_str()]);
    late.wait_for("the host marked down", |m| m["op"] == "host.down" && m["hostId"] == host).await;
}

#[tokio::test]
async fn install_sets_up_yardd_on_an_ssh_host() {
    let sb = Sandbox::new("install");
    // A release folder in the shape CI publishes, holding this very yardd.
    let release = sb.root.join("release");
    let pkg = sb.root.join("pkg/switchyard");
    fs::create_dir_all(pkg.join("bin")).unwrap();
    fs::create_dir_all(pkg.join("ui/dist")).unwrap();
    fs::copy(env!("CARGO_BIN_EXE_yardd"), pkg.join("bin/yardd")).unwrap();
    fs::write(pkg.join("bin/yard-claude"), "#!/bin/sh\nexit 1\n").unwrap();
    fs::write(pkg.join("VERSION"), "test-build\n").unwrap();
    fs::create_dir_all(&release).unwrap();
    let asset = if cfg!(target_arch = "aarch64") { "switchyard-linux-arm64.tar.gz" } else { "switchyard-linux-x64.tar.gz" };
    let tar = std::process::Command::new("tar")
        .args(["-czf", release.join(asset).to_str().unwrap(), "-C", sb.root.join("pkg").to_str().unwrap(), "switchyard"])
        .status()
        .unwrap();
    assert!(tar.success());

    // ssh that runs the remote command here, as the "remote" machine. Its
    // config sets a RemoteCommand (a tmux attach, say), so like real ssh it
    // refuses a command line unless that is turned off.
    let fake_ssh = sb.root.join("fake-ssh");
    fs::write(
        &fake_ssh,
        "#!/bin/bash\n[[ \" $* \" == *' RemoteCommand=none '* ]] || { echo 'Cannot execute command-line and remote command.' >&2; exit 255; }\nwhile [ \"$1\" != -- ]; do shift; done\nshift 2\n[ $# -gt 0 ] || { echo 'fake ssh: no tunnels' >&2; exit 255; }\nexec bash -c \"$*\"\n",
    )
    .unwrap();
    std::process::Command::new("chmod").args(["+x", fake_ssh.to_str().unwrap()]).status().unwrap();

    let remote_port = free_port();
    let release_url = format!("file://{}", release.display());
    let d = sb.start_with(&[
        ("YARD_SSH", fake_ssh.to_str().unwrap()),
        ("YARD_RELEASE_URL", &release_url),
        ("SWITCHYARD_NO_SYSTEMD", "1"),
    ]);
    let mut c = d.client().await;
    c.send(json!({ "op": "host.add", "name": "box", "kind": "ssh", "destination": "me@box", "remotePort": remote_port })).await;
    let hosts = c.wait_for("the host", |m| m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| !h.is_empty())).await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();

    c.send(json!({ "op": "host.install", "hostId": host })).await;
    let done = c.wait_for("the install to finish", |m| m["op"] == "host.install" && m["done"] == true).await;
    let lines: Vec<String> = c
        .log
        .iter()
        .filter(|m| m["op"] == "host.install" && m["hostId"] == host.as_str())
        .filter_map(|m| m["line"].as_str().map(str::to_string))
        .collect();
    let installed = sb.home().join(".local/share/switchyard");
    let pid = fs::read_to_string(installed.join("yardd.pid")).unwrap_or_default();
    let stop = || {
        if !pid.trim().is_empty() {
            let _ = std::process::Command::new("kill").arg(pid.trim()).status();
        }
    };
    if done["ok"] != true {
        stop();
        panic!("install failed: {lines:#?}");
    }
    assert!(lines.iter().any(|l| l.starts_with("@@ok yardd ")), "{lines:#?}");
    assert!(lines.iter().any(|l| l.starts_with("@@engine codex ")), "{lines:#?}");
    assert!(lines.iter().any(|l| l == "Installed test-build"), "{lines:#?}");
    let health = http_get(remote_port, "/health");
    stop();
    assert!(health.unwrap_or_default().contains("yardd "), "the installed yardd answers");
}

/// Bytes that pass for a PNG: yardd only checks the signature.
fn png(len: usize) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.resize(len.max(8), 7);
    bytes
}

/// The text and the `localImage` paths of one `turn/start` input.
fn split_input(input: &Value) -> (String, Vec<String>) {
    let items = input.as_array().expect("an input list");
    let text = items.iter().filter(|i| i["type"] == "text").map(|i| i["text"].as_str().unwrap()).collect::<Vec<_>>().join("\n");
    let images = items.iter().filter(|i| i["type"] == "localImage").map(|i| i["path"].as_str().unwrap().to_string()).collect();
    (text, images)
}

#[tokio::test]
async fn attachments_reach_codex_as_a_local_image_and_a_path() {
    let sb = Sandbox::new("attach");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");

    let shot = c.upload("shot.png", "image/png", &png(300), json!({})).await.unwrap();
    let notes = c.upload("notes.txt", "text/plain", b"remember the milk\n", json!({})).await.unwrap();
    assert_eq!(shot["name"], "shot.png");
    assert_eq!(shot["mime"], "image/png");
    assert_eq!(shot["size"], 300);
    assert!(shot["hostId"].is_null());
    let shot_path = shot["path"].as_str().unwrap();
    let notes_path = notes["path"].as_str().unwrap();
    assert_eq!(Path::new(shot_path), sb.data().join("attachments").join(shot["id"].as_str().unwrap()).join("shot.png"));
    assert_eq!(fs::read(notes_path).unwrap(), b"remember the milk\n");

    let sid = c.create(&cwd, "look at these", json!({ "attachments": [shot["id"], notes["id"]] })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");

    let inputs = sb.turn_inputs();
    assert_eq!(inputs.len(), 1);
    let (text, images) = split_input(&inputs[0]);
    assert!(text.starts_with("look at these\n\nAttached files:\n"), "{text}");
    assert!(text.contains(&format!("- {notes_path} (text/plain, 18 B)")), "{text}");
    assert!(text.contains(shot_path), "every attachment is named by path: {text}");
    assert_eq!(images, [shot_path]);

    // The event keeps the user's own text and the attachments' details, for the thread.
    let history = c.history(&sid).await;
    let user = &history[0];
    assert_eq!(user["type"], "user_message");
    assert_eq!(user["text"], "look at these");
    let names: Vec<&str> = user["attachments"].as_array().unwrap().iter().map(|a| a["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["shot.png", "notes.txt"]);
    assert_eq!(user["attachments"][0]["path"], shot_path);
    assert_eq!(user["attachments"][1]["size"], 18);

    // A file the agent links is read from the session's folder, whatever
    // dialect the link is in; a missing one says so.
    fs::write(Path::new(&cwd).join("report.csv"), "org,domain\n").unwrap();
    assert_eq!(c.file(&sid, "report.csv:2").await.unwrap(), ("report.csv".to_string(), b"org,domain\n".to_vec()));
    assert_eq!(c.file(&sid, "sandbox:/mnt/data/report.csv").await.unwrap().1, b"org,domain\n");
    let missing = c.file(&sid, "gone.csv").await.unwrap_err();
    assert!(missing.contains("gone.csv was not found"), "{missing}");

    // Thumbnails: the bytes come back for this session's attachments only.
    assert_eq!(c.fetch(&sid, shot["id"].as_str().unwrap()).await.unwrap(), png(300));
    let stray = c.upload("other.png", "image/png", &png(20), json!({})).await.unwrap();
    let refused = c.fetch(&sid, stray["id"].as_str().unwrap()).await.unwrap_err();
    assert!(refused.contains("no attachment"), "{refused}");

    // A reply can be an attachment alone. The image claimed a wrong type,
    // and goes by what its bytes are.
    let next = c.next_seq(&sid);
    let pic = c.upload("pic", "application/octet-stream", &png(64), json!({ "sessionId": sid })).await.unwrap();
    assert_eq!(pic["mime"], "image/png");
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "", "attachments": [pic["id"]] })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let (text, images) = split_input(&sb.turn_inputs()[1]);
    assert_eq!(text, format!("Attached files:\n- {} (image/png, 64 B)", pic["path"].as_str().unwrap()));
    assert_eq!(images, [pic["path"].as_str().unwrap()]);

    // A file that claims to be an image but is not goes by path only.
    let fake = c.upload("fake.png", "image/png", b"not an image", json!({})).await.unwrap();
    assert_eq!(fake["mime"], "application/octet-stream");

    // An unknown or malformed id refuses the message and starts nothing.
    let from = c.mark();
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "x", "attachments": ["anope"] })).await;
    let err = c.wait_error(from, "session.send").await;
    assert!(err.contains("no attachment anope"), "{err}");
    let from = c.mark();
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "x", "attachments": ["../../etc"] })).await;
    let err = c.wait_error(from, "session.send").await;
    assert!(err.contains("bad attachment id"), "{err}");
    assert_eq!(sb.turn_inputs().len(), 2);
}

#[tokio::test]
async fn a_too_large_upload_is_refused_and_the_socket_survives() {
    let sb = Sandbox::new("attach-big");
    let d = sb.start();
    let mut c = d.client().await;

    let err = c.upload("big.bin", "application/octet-stream", &vec![0u8; 10 * 1024 * 1024 + 1], json!({})).await.unwrap_err();
    assert!(err.contains("big.bin is too large to attach (10.0 MB). The limit is 10.0 MB."), "{err}");
    let stored = |sb: &Sandbox| fs::read_dir(sb.data().join("attachments")).map(|d| d.count()).unwrap_or(0);
    assert_eq!(stored(&sb), 0);

    // Exactly at the cap is fine, and the same socket still works.
    let full = c.upload("full.bin", "", &vec![1u8; 10 * 1024 * 1024], json!({})).await.unwrap();
    assert_eq!(full["size"], 10 * 1024 * 1024);
    assert_eq!(full["mime"], "application/octet-stream");
    assert_eq!(stored(&sb), 1);

    let from = c.mark();
    c.send(json!({ "op": "attach.put", "ref": "bad64", "name": "x", "mime": "text/plain", "data": "@@not base64@@" })).await;
    let reply = c.wait_after(from, "the bad upload's reply", |m| m["ref"] == "bad64").await;
    assert!(reply["error"].as_str().unwrap().contains("not valid base64"), "{reply}");
}

#[tokio::test]
async fn upload_names_are_sanitized() {
    let sb = Sandbox::new("attach-names");
    let d = sb.start();
    let mut c = d.client().await;
    let root = sb.data().join("attachments");

    let a = c.upload("../../escape.txt", "text/plain", b"x", json!({})).await.unwrap();
    assert_eq!(a["name"], "escape.txt");
    assert_eq!(Path::new(a["path"].as_str().unwrap()), root.join(a["id"].as_str().unwrap()).join("escape.txt"));
    assert!(!sb.data().join("escape.txt").exists() && !sb.root.join("escape.txt").exists());

    let dots = c.upload("..", "text/plain", b"x", json!({})).await.unwrap();
    assert_eq!(dots["name"], "file");
    let hidden = c.upload(".env", "text/plain", b"x", json!({})).await.unwrap();
    assert_eq!(hidden["name"], "env");
    let dash = c.upload("-rf", "text/plain", b"x", json!({})).await.unwrap();
    assert_eq!(dash["name"], "rf");
    let windows = c.upload("C:\\Users\\me\\report.pdf", "application/pdf", b"%PDF", json!({})).await.unwrap();
    assert_eq!(windows["name"], "report.pdf");
    // Every file stays one level down, in its own folder.
    for att in [&a, &dots, &hidden, &dash, &windows] {
        let p = Path::new(att["path"].as_str().unwrap());
        assert_eq!(p.parent().unwrap().parent().unwrap(), root);
        assert!(p.is_file());
    }
}

#[tokio::test]
async fn attachments_land_on_the_remote_host_through_the_hub() {
    let remote_sb = Sandbox::new("attach-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("attach-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;

    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;

    // A full-size image crosses both hops (UI -> hub -> remote) in one message.
    let big = png(10 * 1024 * 1024);
    let shot = c.upload("shot.png", "image/png", &big, json!({ "host": host })).await.unwrap();
    assert_eq!(shot["hostId"], host.as_str());
    let shot_path = shot["path"].as_str().unwrap().to_string();
    assert!(Path::new(&shot_path).starts_with(remote_sb.data().join("attachments")), "{shot_path}");
    assert_eq!(fs::read(&shot_path).unwrap().len(), big.len());
    assert!(!hub_sb.data().join("attachments").exists(), "nothing is stored on the hub");

    let cwd = remote_sb.folder("proj");
    let sid = c.create(&cwd, "remote look", json!({ "host": host, "attachments": [shot["id"]] })).await;
    assert!(sid.starts_with(&format!("{host}~")));
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let (text, images) = split_input(&remote_sb.turn_inputs()[0]);
    assert!(text.contains(&shot_path), "{text}");
    assert_eq!(images, [shot_path.clone()]);
    assert!(hub_sb.turn_inputs().is_empty());

    // A reply on the remote session uploads by session id.
    let next = c.next_seq(&sid);
    let log = c.upload("err.log", "text/plain", b"boom\n", json!({ "sessionId": sid })).await.unwrap();
    assert!(Path::new(log["path"].as_str().unwrap()).starts_with(remote_sb.data().join("attachments")));
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "and this", "attachments": [log["id"]] })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let (text, images) = split_input(&remote_sb.turn_inputs()[1]);
    assert!(text.contains(log["path"].as_str().unwrap()), "{text}");
    assert!(images.is_empty());

    // History and thumbnails come back through the hub under the prefixed id.
    let history = c.history(&sid).await;
    assert_eq!(history[0]["attachments"][0]["path"], shot_path.as_str());
    assert_eq!(c.fetch(&sid, shot["id"].as_str().unwrap()).await.unwrap(), big);
    // A linked file is read on the remote host, at the size cap, through the hub.
    let csv = vec![b'x'; 10 * 1024 * 1024];
    fs::write(Path::new(&cwd).join("big.csv"), &csv).unwrap();
    assert_eq!(c.file(&sid, &format!("{cwd}/big.csv")).await.unwrap(), ("big.csv".to_string(), csv));
    let over = c.file(&sid, "../proj").await.unwrap_err();
    assert!(over.contains("is a folder"), "{over}");

    // Over the cap through the hub: refused by the remote, the link stays up.
    let err = c.upload("huge.bin", "", &vec![0u8; 10 * 1024 * 1024 + 1], json!({ "host": host })).await.unwrap_err();
    assert!(err.contains("too large"), "{err}");
    let ok = c.upload("after.txt", "text/plain", b"ok", json!({ "host": host })).await.unwrap();
    assert_eq!(ok["hostId"], host.as_str());
}

#[tokio::test]
async fn permission_modes_reach_codex_and_switch_mid_session() {
    let sb = Sandbox::new("perm");
    let d = sb.start();
    let mut c = d.client().await;

    let sid = c.create(&sb.folder("proj"), "look around", json!({ "permission": "plan" })).await;
    let meta = c.wait_for("the session", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["permission"] == "plan").await;
    assert_eq!(meta["session"]["permission"], "plan");
    c.wait_turn_done(&sid, 0).await;
    assert!(sb.codex_log().contains("approval=on-request sandboxPolicy=readOnly"), "{}", sb.codex_log());

    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "full" })).await;
    c.wait_for("the new mode", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["permission"] == "full").await;
    let from = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "go ahead" })).await;
    c.wait_turn_done(&sid, from).await;
    assert!(sb.codex_log().contains("approval=never sandboxPolicy=dangerFullAccess"), "{}", sb.codex_log());

    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "yolo" })).await;
    let refused = c.wait_for("the refusal", |m| m["op"] == "error").await;
    assert!(refused["message"].as_str().unwrap().contains("unknown permission mode"), "{refused}");
    let err = c.try_create(&sb.folder("other"), "x", json!({ "permission": "yolo" })).await.unwrap_err();
    assert!(err.contains("unknown permission mode"), "{err}");
}

#[tokio::test]
async fn engines_log_in_from_the_app() {
    let sb = Sandbox::new("login");
    let d = sb.start();
    let mut c = d.client().await;

    c.send(json!({ "op": "engines.status" })).await;
    let before = c.wait_for("engine status", |m| m["op"] == "engines").await;
    assert_eq!(before["engines"]["claude"]["loggedIn"], false, "{before}");
    assert_eq!(before["engines"]["codex"]["loggedIn"], false, "{before}");

    // Codex: a link and a code, then the app-server says it is done.
    let mark = c.log.len();
    c.send(json!({ "op": "login.start", "engine": "codex" })).await;
    let code = c.wait_after(mark, "codex device code", |m| m["op"] == "login" && m["engine"] == "codex" && m["state"] == "code").await;
    assert_eq!(code["code"], "FAKE-1234");
    assert_eq!(code["url"], "https://auth.openai.com/codex/device");
    c.wait_after(mark, "codex login done", |m| m["op"] == "login" && m["engine"] == "codex" && m["state"] == "done").await;
    c.wait_after(mark, "codex logged in", |m| m["op"] == "engines" && m["engines"]["codex"]["loggedIn"] == true).await;

    // Claude: the sign-in link is pulled out of the CLI's output, a wrong
    // code is refused, the right one logs in.
    let mark = c.log.len();
    c.send(json!({ "op": "login.start", "engine": "claude" })).await;
    let url = c.wait_after(mark, "claude link", |m| m["op"] == "login" && m["engine"] == "claude" && m["state"] == "url").await;
    assert_eq!(url["url"], "https://claude.com/cai/oauth/authorize?code=true&state=fake");
    c.send(json!({ "op": "login.code", "engine": "claude", "code": "wrong" })).await;
    let failed = c.wait_after(mark, "wrong code", |m| m["op"] == "login" && m["engine"] == "claude" && m["state"] == "failed").await;
    assert!(failed["error"].as_str().unwrap().contains("not accepted"), "{failed}");

    let mark = c.log.len();
    c.send(json!({ "op": "login.start", "engine": "claude" })).await;
    c.wait_after(mark, "claude link again", |m| m["op"] == "login" && m["engine"] == "claude" && m["state"] == "url").await;
    c.send(json!({ "op": "login.code", "engine": "claude", "code": "good-code" })).await;
    c.wait_after(mark, "claude login done", |m| m["op"] == "login" && m["engine"] == "claude" && m["state"] == "done").await;
    let after = c.wait_after(mark, "claude logged in", |m| m["op"] == "engines" && m["engines"]["claude"]["loggedIn"] == true).await;
    assert_eq!(after["engines"]["claude"]["account"], "me@example.com");
}

// A closed tab never sends `login.cancel`, so a Claude login nobody finishes
// would keep `claude auth login` running under yardd until it stops.
#[tokio::test]
async fn an_abandoned_claude_login_is_stopped() {
    let sb = Sandbox::new("login-abandoned");
    let d = sb.start_with(&[("YARD_CLAUDE_LOGIN_TIMEOUT_MS", "1500")]);
    let mut c = d.client().await;

    let mark = c.log.len();
    c.send(json!({ "op": "login.start", "engine": "claude" })).await;
    c.wait_after(mark, "claude link", |m| m["op"] == "login" && m["engine"] == "claude" && m["state"] == "url").await;
    let pid = claude_log(&sb)
        .lines()
        .find(|l| l.contains("args=auth login"))
        .and_then(|l| l.split_whitespace().find_map(|w| w.strip_prefix("pid=")).map(str::to_string))
        .expect("the login was logged");

    // No code comes: the login gives up on its own and says why.
    let failed = c.wait_after(mark, "the login timing out", |m| m["op"] == "login" && m["engine"] == "claude" && m["state"] == "failed").await;
    assert!(failed["error"].as_str().unwrap().contains("Start again"), "{failed}");
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists(), "`claude auth login` (pid {pid}) is still running");

    // A code after that finds no login, rather than a stale prompt.
    c.send(json!({ "op": "login.code", "engine": "claude", "code": "good-code" })).await;
    let refused = c.wait_after(mark, "the late code refused", |m| m["op"] == "error").await;
    assert!(refused["message"].as_str().unwrap().contains("no Claude login is running"), "{refused}");
}

#[tokio::test]
async fn codex_reasoning_streams_as_thinking() {
    let sb = Sandbox::new("think");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");

    let sid = c.create(&cwd, "[think] why", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    let events = c.events(&sid);
    let deltas: String = events
        .iter()
        .filter(|e| e["type"] == "reasoning_delta")
        .map(|e| e["text"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(deltas, "Reading the tests\n\nThen the fix", "summary parts become paragraphs");
    let done = events.iter().find(|e| e["type"] == "reasoning").expect("the finished reasoning");
    assert_eq!(done["text"], "Reading the tests\n\nThen the fix");
    let think = events.iter().position(|e| e["type"] == "reasoning").unwrap();
    let reply = events.iter().position(|e| e["type"] == "assistant_message").unwrap();
    assert!(think < reply, "thinking comes before the answer");
}

#[tokio::test]
async fn renaming_a_session_updates_other_clients_and_survives_restart() {
    let sb = Sandbox::new("rename-session");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = make_repo(&sb, "proj");
    let sid = c.create(&cwd, "original title", json!({ "worktree": true })).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let mut other = d.client().await;
    let previous = other.log[0]["sessions"][0].clone();

    let title = "Revisão do lançamento ⚽";
    c.send(json!({ "op": "session.rename", "sessionId": sid, "title": format!("  {title}  "), "ref": "rename" })).await;
    let ack = c.wait_for("rename reply", |m| m["ref"] == "rename").await;
    assert_eq!(ack["op"], "session.renamed", "{ack}");
    let renamed = other.wait_for("renamed session", |m| m["op"] == "session" && m["session"]["title"] == title).await;
    for key in ["id", "cwd", "branch", "status"] {
        assert_eq!(renamed["session"][key], previous[key], "{key}");
    }

    for (i, invalid) in [String::from(" "), String::from("two\nlines"), "🔧".repeat(61)].into_iter().enumerate() {
        let reference = format!("invalid-{i}");
        c.send(json!({ "op": "session.rename", "sessionId": sid, "title": invalid, "ref": reference })).await;
        let reply = c.wait_for("rename rejected", |m| m["ref"] == reference && m["op"] == "error").await;
        assert_eq!(reply["for"], "session.rename");
    }

    let deadline = Instant::now() + WAIT;
    while saved_meta(&sb, &sid).map(|m| m["title"] == title) != Some(true) {
        assert!(Instant::now() < deadline, "the new title never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();

    let d = sb.start();
    let mut c = d.client().await;
    assert_eq!(c.log[0]["sessions"][0]["title"], title);
    c.send(json!({ "op": "session.open", "sessionId": sid })).await;
    let history = c.wait_for("same history", |m| m["op"] == "history" && m["sessionId"] == sid).await;
    assert_eq!(history["events"][0]["text"], "original title");
}

#[tokio::test]
async fn sessions_are_pinned_and_archived_across_restarts() {
    let sb = Sandbox::new("pin");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");
    let a = c.create(&cwd, "first", json!({})).await;
    // One writer per folder: the second session may start only once the first is done.
    c.wait_turn_done(&a, 0).await;
    c.wait_status(&a, "idle").await;
    let b = c.create(&cwd, "second", json!({})).await;
    c.wait_turn_done(&b, 0).await;

    c.send(json!({ "op": "session.update", "sessionId": a, "pinned": true })).await;
    c.wait_for("a pinned", |m| m["op"] == "session" && m["session"]["id"] == a && m["session"]["pinned"] == true).await;
    // Archiving a pinned session unpins it.
    c.send(json!({ "op": "session.update", "sessionId": b, "pinned": true })).await;
    // Ops run in their own tasks: a pin applied after the archive would
    // unarchive it again, so the archive goes once the pin has landed.
    c.wait_for("b pinned", |m| m["op"] == "session" && m["session"]["id"] == b && m["session"]["pinned"] == true).await;
    c.send(json!({ "op": "session.update", "sessionId": b, "archived": true })).await;
    let archived = c
        .wait_for("b archived", |m| m["op"] == "session" && m["session"]["id"] == b && m["session"]["archived"] == true)
        .await;
    assert_eq!(archived["session"]["pinned"], false);

    let deadline = Instant::now() + WAIT;
    while saved_meta(&sb, &b).map(|m| m["archived"] == true) != Some(true) {
        assert!(Instant::now() < deadline, "the archive never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();

    let d = sb.start();
    let c = d.client().await;
    let sessions = c.log[0]["sessions"].as_array().unwrap();
    let find = |id: &str| sessions.iter().find(|s| s["id"] == id).unwrap().clone();
    assert_eq!(find(&a)["pinned"], true);
    assert_eq!(find(&b)["archived"], true);
    assert_eq!(find(&b)["pinned"], false);
}

#[tokio::test]
async fn sessions_are_grouped_across_restarts() {
    let sb = Sandbox::new("group");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");
    let a = c.create(&cwd, "first", json!({})).await;
    c.wait_turn_done(&a, 0).await;
    c.wait_status(&a, "idle").await;
    let b = c.create(&cwd, "second", json!({})).await;
    c.wait_turn_done(&b, 0).await;

    c.send(json!({ "op": "session.update", "sessionId": a, "group": "  Release  " })).await;
    let set = c.wait_for("a grouped", |m| m["op"] == "session" && m["session"]["id"] == a && m["session"]["group"] == "Release").await;
    assert_eq!(set["session"]["group"], "Release", "the name is trimmed");
    c.send(json!({ "op": "session.update", "sessionId": b, "group": "Release" })).await;
    c.send(json!({ "op": "session.update", "sessionId": b, "group": null })).await;
    let cleared = c
        .wait_for("b ungrouped", |m| m["op"] == "session" && m["session"]["id"] == b && m["session"]["group"].is_null() && m["session"]["pinned"] == false)
        .await;
    assert!(cleared["session"]["group"].is_null());
    // An empty name is no group either, and an update without `group` leaves it.
    c.send(json!({ "op": "session.update", "sessionId": a, "pinned": true })).await;
    let pinned = c.wait_for("a pinned", |m| m["op"] == "session" && m["session"]["id"] == a && m["session"]["pinned"] == true).await;
    assert_eq!(pinned["session"]["group"], "Release");

    let deadline = Instant::now() + WAIT;
    while saved_meta(&sb, &a).map(|m| m["group"] == "Release") != Some(true) {
        assert!(Instant::now() < deadline, "the group never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();

    let d = sb.start();
    let mut c = d.client().await;
    let sessions = c.log[0]["sessions"].as_array().unwrap();
    let find = |id: &str| sessions.iter().find(|s| s["id"] == id).unwrap().clone();
    assert_eq!(find(&a)["group"], "Release");
    assert!(find(&b)["group"].is_null());
    c.send(json!({ "op": "session.update", "sessionId": a, "group": "" })).await;
    c.wait_for("a ungrouped", |m| m["op"] == "session" && m["session"]["id"] == a && m["session"]["group"].is_null()).await;
}

#[tokio::test]
async fn stop_ends_a_codex_turn_that_ignores_the_interrupt() {
    let sb = Sandbox::new("stuck-codex");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[stuck] spin", json!({})).await;
    c.wait_event(&sid, "assistant_delta", 0).await;

    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "interrupted");
    c.wait_status(&sid, "idle").await;
    assert!(c.event_types(&sid).contains(&"note".to_string()), "{:?}", c.event_types(&sid));

    // The thread still takes a new turn.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "hello again" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
}

fn claude_sidecar() -> String {
    fake_bin().join("yard-claude").to_str().unwrap().to_string()
}

#[tokio::test]
async fn a_claude_session_whose_sidecar_died_takes_the_next_message() {
    let sb = Sandbox::new("claude-died");
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let extra = json!({ "engine": "claude", "model": "claude-haiku-5-5" });
    let sid = c.create(&sb.folder("proj"), "first", extra.clone()).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");

    // Before the fix, the dead sidecar stayed in place: this message went
    // nowhere and its turn never ended.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[die] now" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "failed");

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "still there?" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let answer = c.wait_event(&sid, "assistant_message", next).await;
    assert_eq!(answer["text"], "Echo: still there?");

    // The new sidecar resumed the first one's Claude Code session.
    let log = fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap();
    let starts: Vec<&str> = log.lines().filter(|l| l.starts_with("start")).collect();
    assert_eq!(starts.len(), 2, "{log}");
    assert!(starts[0].contains("resume= "), "{log}");
    assert!(starts[1].contains("resume=fake-claude-"), "{log}");
}

#[tokio::test]
async fn stop_kills_a_claude_sidecar_that_ignores_the_interrupt() {
    let sb = Sandbox::new("claude-stuck");
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let extra = json!({ "engine": "claude", "model": "claude-haiku-5-5" });
    let sid = c.create(&sb.folder("proj"), "[stuck] spin", extra).await;
    c.wait_event(&sid, "assistant_delta", 0).await;

    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "interrupted");
    c.wait_status(&sid, "idle").await;
    // Ended once: the killed sidecar's exit adds no failure.
    c.drain_for(Duration::from_millis(500)).await;
    let types = c.event_types(&sid);
    assert_eq!(types.iter().filter(|t| *t == "turn_completed").count(), 1, "{types:?}");
    assert!(!types.contains(&"error".to_string()), "{types:?}");

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "hello" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
}

#[tokio::test]
async fn notify_reaches_every_connected_window() {
    let sb = Sandbox::new("notify");
    let d = sb.start();
    let mut a = d.client().await;
    let mut b = d.client().await;
    let mut agent = d.client().await;
    let (ma, mb) = (a.mark(), b.mark());

    agent
        .send(json!({ "op": "notify", "title": "Loop failed", "body": "patrol did not start", "sessionId": "s1", "url": "https://github.com/o/r/pull/7" }))
        .await;
    let want = |m: &Value| m["op"] == "notify";
    let got_a = a.wait_after(ma, "the notification in window a", want).await;
    let got_b = b.wait_after(mb, "the notification in window b", want).await;
    for got in [&got_a, &got_b] {
        assert_eq!(got["title"], "Loop failed");
        assert_eq!(got["body"], "patrol did not start");
        assert_eq!(got["sessionId"], "s1");
        assert_eq!(got["url"], "https://github.com/o/r/pull/7");
    }
    // The sender learns how many windows got it: three clients are connected,
    // and the sender is one of them, so a caller can tell nobody was there.
    let sent = agent.wait_for("the sender's receipt", |m| m["op"] == "notify.sent").await;
    assert_eq!(sent["windows"], 3, "{sent}");
}

#[tokio::test]
async fn notify_needs_a_title_and_only_takes_web_links() {
    let sb = Sandbox::new("notify-bad");
    let d = sb.start();
    let mut c = d.client().await;
    let from = c.mark();
    c.send(json!({ "op": "notify", "body": "no title" })).await;
    let err = c.wait_error(from, "notify").await;
    assert!(err.contains("title"), "{err}");

    let from = c.mark();
    c.send(json!({ "op": "notify", "title": "x", "url": "file:///etc/passwd" })).await;
    let err = c.wait_error(from, "notify").await;
    assert!(err.contains("url"), "{err}");
}

#[tokio::test]
async fn claude_code_sessions_are_listed_and_imported_with_their_history() {
    let sb = Sandbox::new("claude-import");
    let sidecar = claude_sidecar();
    let work = sb.folder("claude-home");
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar), ("FAKE_CLAUDE_CWD", &work)]);
    let mut c = d.client().await;
    let found = "11111111-1111-4111-8111-111111111111";
    let gone = "22222222-2222-4222-8222-222222222222";

    // The list comes from the sidecar (the SDK's listSessions), newest first.
    c.send(json!({ "op": "claude.sessions" })).await;
    let list = c.wait_for("the session list", |m| m["op"] == "claude.sessions").await;
    let sessions = list["sessions"].as_array().unwrap_or_else(|| panic!("{list}"));
    assert_eq!(sessions.len(), 3, "{list}");
    assert_eq!(sessions[0]["sessionId"], found);
    assert_eq!(sessions[0]["title"], "TEST-fix the login redirect");
    assert_eq!(sessions[0]["cwd"], work.as_str());
    assert!(sessions[0]["importedAs"].is_null(), "{list}");

    // Importing makes a Claude session in that folder, in place, with the history.
    let from = c.mark();
    c.send(json!({ "op": "session.import", "claudeSessionId": found })).await;
    let created = c.wait_after(from, "the import", |m| m["op"] == "created" || m["op"] == "error").await;
    assert_eq!(created["op"], "created", "{created}");
    let sid = created["sessionId"].as_str().unwrap().to_string();
    let history = c.history(&sid).await;
    let texts: Vec<&str> = history.iter().filter_map(|e| e["text"].as_str()).collect();
    assert!(texts.contains(&"TEST-fix the login redirect"), "{history:?}");
    assert!(texts.contains(&"TEST-added login.test.ts."), "{history:?}");
    assert!(history.iter().any(|e| e["type"] == "note" && e["text"].as_str().unwrap_or("").contains("Imported from Claude Code")), "{history:?}");
    // Writes reach the database through a batching writer thread, so the row
    // can land after the `created` reply.
    let deadline = Instant::now() + WAIT;
    let meta = loop {
        if let Some(meta) = saved_meta(&sb, &sid) {
            break meta;
        }
        assert!(Instant::now() < deadline, "the imported session never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(meta["engine"], "claude");
    assert_eq!(meta["cwd"], work.as_str());
    assert!(meta["branch"].is_null(), "imports run in place: {meta}");

    // Listed as imported; a second import answers with the same session.
    let from = c.mark();
    c.send(json!({ "op": "claude.sessions" })).await;
    let list = c.wait_after(from, "the list again", |m| m["op"] == "claude.sessions" && m["sessions"][0]["importedAs"].is_string()).await;
    assert_eq!(list["sessions"][0]["importedAs"], sid.as_str());
    let from = c.mark();
    c.send(json!({ "op": "session.import", "claudeSessionId": found })).await;
    let again = c.wait_after(from, "the second import", |m| m["op"] == "created" || m["op"] == "error").await;
    assert_eq!(again["sessionId"], sid.as_str(), "{again}");

    // A session whose folder is gone is refused.
    let from = c.mark();
    c.send(json!({ "op": "session.import", "claudeSessionId": gone })).await;
    let err = c.wait_error(from, "session.import").await;
    assert!(err.contains("no longer exists"), "{err}");

    // The next message resumes the same Claude Code conversation.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "carry on" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let log = fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap();
    assert!(log.lines().any(|l| l.starts_with("start") && l.contains(&format!("resume={found} "))), "{log}");
}

#[tokio::test]
async fn two_imports_of_one_claude_code_session_at_once_make_one_session() {
    let sb = Sandbox::new("claude-import-race");
    let sidecar = claude_sidecar();
    let work = sb.folder("claude-home");
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar), ("FAKE_CLAUDE_CWD", &work)]);
    let id = "11111111-1111-4111-8111-111111111111";
    // Two clients (the desktop app and a phone), each importing at the same moment.
    let mut a = d.client().await;
    let mut b = d.client().await;
    let (fa, fb) = (a.mark(), b.mark());
    a.send(json!({ "op": "session.import", "claudeSessionId": id })).await;
    b.send(json!({ "op": "session.import", "claudeSessionId": id })).await;
    let ra = a.wait_after(fa, "a's import", |m| m["op"] == "created" || m["op"] == "error").await;
    let rb = b.wait_after(fb, "b's import", |m| m["op"] == "created" || m["op"] == "error").await;
    assert_eq!(ra["op"], "created", "{ra}");
    assert_eq!(rb["op"], "created", "{rb}");
    assert_eq!(ra["sessionId"], rb["sessionId"], "two sessions for one Claude Code conversation: {ra} {rb}");
    let claude_sessions = a
        .log
        .iter()
        .filter(|m| m["op"] == "session" && m["session"]["engine"] == "claude")
        .map(|m| m["session"]["id"].as_str().unwrap_or("").to_string())
        .collect::<std::collections::HashSet<_>>();
    assert!(claude_sessions.len() <= 1, "{claude_sessions:?}");
}

// ---- Automations ----

fn automation_of(cwd: &str, extra: Value) -> Value {
    let mut a = json!({
        "name": "Patrol",
        "prompt": "say hello",
        "cwd": cwd,
        "engine": "codex",
        "model": MODEL,
        "effort": EFFORT,
        "permission": "ask",
        "schedule": { "kind": "interval", "every": 60, "unit": "minutes" },
    });
    if let (Some(m), Some(e)) = (a.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    a
}

/// Save an automation and return it as yardd stored it, or yardd's error.
async fn save_automation(c: &mut Client, a: Value) -> Result<Value, String> {
    let from = c.mark();
    c.send(json!({ "op": "automation.save", "automation": a, "ref": "save" })).await;
    let reply = c
        .wait_after(from, "automation.saved or an error", |m| {
            m["ref"] == "save" && (m["op"] == "automation.saved" || m["op"] == "error")
        })
        .await;
    match reply["op"].as_str() {
        Some("automation.saved") => Ok(reply["automation"].clone()),
        _ => Err(reply["message"].as_str().unwrap_or("").to_string()),
    }
}

/// The newest `automations` list this client heard (snapshot or broadcast).
fn automations_of(c: &Client) -> Vec<Value> {
    c.log
        .iter()
        .rev()
        .find_map(|m| match m["op"].as_str() {
            Some("automations") | Some("snapshot") => m["automations"].as_array().cloned(),
            _ => None,
        })
        .unwrap_or_default()
}

/// Run now (or a scheduled-style run without `force`); returns the reply.
async fn run_automation(c: &mut Client, id: &str, force: bool) -> Value {
    let from = c.mark();
    c.send(json!({ "op": "automation.run", "id": id, "force": force, "ref": "run" })).await;
    c.wait_after(from, "automation.ran", |m| m["ref"] == "run" && (m["op"] == "automation.ran" || m["op"] == "error"))
        .await
}

#[tokio::test]
async fn automations_are_saved_listed_edited_and_deleted() {
    let sb = Sandbox::new("auto-crud");
    let cwd = sb.folder("proj");
    let d = sb.start();
    let mut c = d.client().await;
    assert_eq!(c.log[0]["automations"], json!([]), "the snapshot carries the list");

    let saved = save_automation(&mut c, automation_of(&cwd, json!({}))).await.unwrap();
    let id = saved["id"].as_str().unwrap().to_string();
    assert_eq!(saved["name"], "Patrol");
    assert_eq!(saved["enabled"], true, "on by default");
    assert_eq!(saved["skipWhileRunning"], true, "skipping a run that is still going is on by default");
    let next = saved["nextRun"].as_u64().unwrap();
    let in_an_hour = next as i64 - (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64);
    assert!((3_500_000..=3_600_500).contains(&in_an_hour), "next run is an hour away, not {in_an_hour} ms");
    c.wait_for("the broadcast", |m| m["op"] == "automations" && m["automations"].as_array().is_some_and(|l| l.len() == 1)).await;

    // A second window sees it in its snapshot.
    let other = d.client().await;
    assert_eq!(other.log[0]["automations"][0]["id"], id);

    // Edit by id; a partial change keeps the rest. Disabling clears the next run.
    let off = save_automation(&mut c, json!({ "id": id, "enabled": false })).await.unwrap();
    assert_eq!(off["enabled"], false);
    assert!(off["nextRun"].is_null(), "{off}");
    assert_eq!(off["prompt"], "say hello");
    let on = save_automation(&mut c, json!({ "id": id, "enabled": true, "schedule": { "kind": "daily", "time": "09:30" } })).await.unwrap();
    assert!(on["nextRun"].as_u64().unwrap() > 0);

    // Refused: no name, a bad time, a folder that is not there.
    assert!(save_automation(&mut c, automation_of(&cwd, json!({ "name": " " }))).await.unwrap_err().contains("name"));
    let bad = json!({ "schedule": { "kind": "daily", "time": "25:99" } });
    assert!(save_automation(&mut c, automation_of(&cwd, bad)).await.unwrap_err().contains("time"));
    assert!(save_automation(&mut c, automation_of("/no/such/folder", json!({}))).await.unwrap_err().contains("not a folder"));

    c.send(json!({ "op": "automation.list" })).await;
    let from = c.mark();
    c.send(json!({ "op": "automation.list" })).await;
    let listed = c.wait_after(from, "the list", |m| m["op"] == "automations").await;
    assert_eq!(listed["automations"].as_array().unwrap().len(), 1);

    c.send(json!({ "op": "automation.delete", "id": id })).await;
    c.wait_for("an empty list", |m| m["op"] == "automations" && m["automations"].as_array().is_some_and(|l| l.is_empty())).await;
    let from = c.mark();
    c.send(json!({ "op": "automation.delete", "id": id })).await;
    assert!(c.wait_error(from, "automation.delete").await.contains("no such automation"));
}

#[tokio::test]
async fn automation_window_survives_partial_edits_and_null_clears_it() {
    let sb = Sandbox::new("auto-window");
    let cwd = sb.folder("proj");
    let d = sb.start_with(&[("TZ", "UTC")]);
    let mut c = d.client().await;
    let window = json!({ "start": "21:00", "end": "09:00", "days": [1, 5] });
    let a = save_automation(&mut c, automation_of(&cwd, json!({ "window": window }))).await.unwrap();
    assert_eq!(a["window"], window);
    let id = a["id"].as_str().unwrap();
    let off = save_automation(&mut c, json!({ "id": id, "enabled": false })).await.unwrap();
    assert_eq!(off["window"], window);
    let on = save_automation(&mut c, json!({ "id": id, "enabled": true })).await.unwrap();
    assert_eq!(on["window"], window);
    assert_eq!(d.client().await.log[0]["automations"][0]["window"], window);
    for schedule in [json!({ "kind": "daily", "time": "09:00" }), json!({ "kind": "weekly", "day": 1, "time": "09:00" }), json!({ "kind": "manual" })] {
        assert!(save_automation(&mut c, json!({ "id": id, "schedule": schedule })).await.unwrap_err().contains("interval"));
    }
    let before = chrono::Utc::now().timestamp_millis() as u64;
    let clear = save_automation(&mut c, json!({ "id": id, "window": null })).await.unwrap();
    let after = chrono::Utc::now().timestamp_millis() as u64;
    assert!(clear["window"].is_null());
    let next = clear["nextRun"].as_u64().unwrap();
    assert!((before + 3_600_000..=after + 3_600_000).contains(&next), "clearing the window recomputes the ordinary interval");
}

#[tokio::test]
async fn run_now_starts_a_worktree_session_that_is_archived_when_it_ends() {
    let sb = Sandbox::new("auto-run");
    let repo = make_repo(&sb, "repo");
    let d = sb.start();
    let mut c = d.client().await;
    let a = save_automation(&mut c, automation_of(&repo, json!({ "name": "Map upkeep" }))).await.unwrap();
    let id = a["id"].as_str().unwrap().to_string();

    let ran = run_automation(&mut c, &id, true).await;
    assert_eq!(ran["op"], "automation.ran", "{ran}");
    assert_eq!(ran["outcome"], "started");
    let sid = ran["sessionId"].as_str().unwrap().to_string();

    let meta = c.wait_for("the run's session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    assert_eq!(meta["automation"], id);
    let title = meta["title"].as_str().unwrap();
    assert!(title.starts_with("Map upkeep · "), "{title}");
    assert!(meta["branch"].as_str().unwrap().starts_with("yard/"), "a worktree: {meta}");
    assert!(Path::new(meta["cwd"].as_str().unwrap()).starts_with(sb.data().join("wt")), "{meta}");
    assert_eq!(meta["permission"], "ask");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert_eq!(c.wait_event(&sid, "user_message", 0).await["text"], "say hello");

    // A finished run leaves the main list by itself.
    c.wait_for("the run archived", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["archived"] == true).await;

    // The automation remembers its last run.
    let last = c
        .wait_for("lastRun", |m| m["op"] == "automations" && m["automations"][0]["lastRun"]["sessionId"] == sid)
        .await["automations"][0]["lastRun"]
        .clone();
    assert_eq!(last["outcome"], "started");

    // Pinning a run brings it back to the main list.
    c.send(json!({ "op": "session.update", "sessionId": sid, "pinned": true })).await;
    let back = c.wait_for("the run pinned", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["pinned"] == true).await;
    assert_eq!(back["session"]["archived"], false);

    // Deleting the automation keeps its runs, which become ordinary sessions.
    c.send(json!({ "op": "automation.delete", "id": id })).await;
    let freed = c.wait_for("the run unlinked", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["automation"].is_null()).await;
    assert_eq!(freed["session"]["pinned"], true);
}

#[tokio::test]
async fn a_runs_finish_time_stays_put_when_it_is_pinned_later() {
    // The Automations page shows how long a run took. `updatedAt` moves on any
    // later change (a pin, a rename, a reply), so the run's end is kept apart.
    let sb = Sandbox::new("auto-finished");
    let repo = make_repo(&sb, "repo");
    let d = sb.start();
    let mut c = d.client().await;
    let a = save_automation(&mut c, automation_of(&repo, json!({}))).await.unwrap();
    let ran = run_automation(&mut c, a["id"].as_str().unwrap(), true).await;
    let sid = ran["sessionId"].as_str().unwrap().to_string();

    let done = c
        .wait_for("the run archived", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["archived"] == true)
        .await["session"]
        .clone();
    let finished = done["finishedAt"].as_u64().unwrap_or_else(|| panic!("a finished run says when it ended: {done}"));
    assert!(finished >= done["createdAt"].as_u64().unwrap(), "{done}");

    tokio::time::sleep(Duration::from_millis(1100)).await;
    c.send(json!({ "op": "session.update", "sessionId": sid, "pinned": true })).await;
    let pinned = c
        .wait_for("the run pinned", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["pinned"] == true)
        .await["session"]
        .clone();
    assert!(pinned["updatedAt"].as_u64().unwrap() >= finished + 1000, "the pin moved updatedAt: {pinned}");
    assert_eq!(pinned["finishedAt"].as_u64(), Some(finished), "a pin does not move the run's end: {pinned}");
}

#[tokio::test]
async fn a_run_is_skipped_while_the_previous_one_is_still_going() {
    let sb = Sandbox::new("auto-skip");
    let cwd = make_repo(&sb, "repo");
    let d = sb.start();
    let mut c = d.client().await;
    let a = save_automation(&mut c, automation_of(&cwd, json!({ "prompt": "[slow] think hard" }))).await.unwrap();
    let id = a["id"].as_str().unwrap().to_string();

    let first = run_automation(&mut c, &id, true).await;
    let sid = first["sessionId"].as_str().unwrap().to_string();
    c.wait_event(&sid, "assistant_delta", 0).await;

    // A scheduled run (no force) sees the first one and leaves it alone.
    let second = run_automation(&mut c, &id, false).await;
    assert_eq!(second["outcome"], "skipped", "{second}");
    assert!(second["reason"].as_str().unwrap().contains("still going"), "{second}");
    // The broadcast and the reply travel separately: wait for the list to say so too.
    c.wait_for("lastRun skipped", |m| m["op"] == "automations" && m["automations"][0]["lastRun"]["outcome"] == "skipped").await;
    let sessions = c.log.iter().filter(|m| m["op"] == "session").map(|m| m["session"]["id"].clone()).collect::<std::collections::HashSet<_>>();
    assert_eq!(sessions.len(), 1, "no second session");

    // With the switch off the next run starts alongside.
    save_automation(&mut c, json!({ "id": id, "skipWhileRunning": false })).await.unwrap();
    let third = run_automation(&mut c, &id, false).await;
    assert_eq!(third["outcome"], "started", "{third}");
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": third["sessionId"] })).await;
}

/// Run with a per-run prompt and title suffix; returns the reply (or the error).
async fn run_automation_with(c: &mut Client, id: &str, prompt: &str, title: &str) -> Value {
    let from = c.mark();
    c.send(json!({ "op": "automation.run", "id": id, "force": true, "prompt": prompt, "title": title, "ref": "run" })).await;
    c.wait_after(from, "automation.ran", |m| m["ref"] == "run" && (m["op"] == "automation.ran" || m["op"] == "error"))
        .await
}

#[tokio::test]
async fn a_run_can_carry_its_own_prompt_and_title() {
    // The per-PR loops (a verifier for #161) are one automation whose every run
    // has its own prompt; the saved prompt is only for Run now.
    let sb = Sandbox::new("auto-prompt");
    let repo = make_repo(&sb, "repo");
    let d = sb.start();
    let mut c = d.client().await;
    let a = save_automation(&mut c, automation_of(&repo, json!({ "name": "Loop verify", "prompt": "the saved prompt", "permission": "full" })))
        .await
        .unwrap();
    let id = a["id"].as_str().unwrap().to_string();

    let ran = run_automation_with(&mut c, &id, "  verify pull request 161  ", "#161").await;
    assert_eq!(ran["outcome"], "started", "{ran}");
    let sid = ran["sessionId"].as_str().unwrap().to_string();
    let meta = c.wait_for("the run's session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    let title = meta["title"].as_str().unwrap();
    assert!(title.starts_with("Loop verify #161 · "), "{title}");
    assert_eq!(meta["automation"], id, "a run, so the sidebar leaves it out");
    c.wait_for("the run's permission", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["permission"] == "full").await;
    assert!(meta["branch"].as_str().unwrap().starts_with("yard/"), "a worktree: {meta}");
    assert_eq!(c.wait_event(&sid, "user_message", 0).await["text"], "verify pull request 161");
    c.wait_for("the run archived", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["archived"] == true).await;

    // The automation keeps its saved prompt for the next Run now.
    let plain = run_automation(&mut c, &id, true).await;
    let plain_sid = plain["sessionId"].as_str().unwrap().to_string();
    assert_eq!(c.wait_event(&plain_sid, "user_message", 0).await["text"], "the saved prompt");
    let plain_meta = c.wait_for("the plain run", |m| m["op"] == "session" && m["session"]["id"] == plain_sid).await["session"].clone();
    assert!(plain_meta["title"].as_str().unwrap().starts_with("Loop verify · "), "{plain_meta}");

    // Refused, and no session started: an empty or oversized prompt, a title that is not one short line.
    let before = c.log.iter().filter(|m| m["op"] == "created").count();
    for (prompt, title, why) in [
        ("   ", "#1", "empty"),
        (&"x".repeat(200_001)[..], "#1", "too long"),
        ("ok", "#1\nsecond line", "one line"),
        ("ok", &"t".repeat(81)[..], "too long"),
    ] {
        let r = run_automation_with(&mut c, &id, prompt, title).await;
        assert_eq!(r["op"], "error", "{why}: {r}");
        assert!(r["message"].as_str().unwrap().contains(why), "{why}: {r}");
    }
    c.drain_for(Duration::from_millis(300)).await;
    let runs = c.log.iter().filter(|m| m["op"] == "session" && m["session"]["automation"] == id).map(|m| m["session"]["id"].clone()).collect::<std::collections::HashSet<_>>();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert_eq!(c.log.iter().filter(|m| m["op"] == "created").count(), before);
}

#[tokio::test]
async fn a_manual_automation_never_fires_on_its_own() {
    let sb = Sandbox::new("auto-manual");
    let repo = make_repo(&sb, "repo");
    let d = sb.start_with(&[("YARD_AUTOMATION_TICK_MS", "100")]);
    let mut c = d.client().await;
    let a = save_automation(&mut c, automation_of(&repo, json!({ "name": "Loop babysit", "schedule": { "kind": "manual" } }))).await.unwrap();
    let id = a["id"].as_str().unwrap().to_string();
    assert_eq!(a["schedule"], json!({ "kind": "manual" }));
    assert_eq!(a["enabled"], true);
    assert!(a["nextRun"].is_null(), "nothing is due: {a}");

    // Many ticks: no run.
    c.drain_for(Duration::from_millis(800)).await;
    assert!(!c.log.iter().any(|m| m["op"] == "session" && m["session"]["automation"] == id));

    // Started by hand (or by `yv loop run`), it runs like any other.
    let ran = run_automation_with(&mut c, &id, "babysit pull request 7", "#7").await;
    assert_eq!(ran["outcome"], "started", "{ran}");
    let sid = ran["sessionId"].as_str().unwrap().to_string();
    c.wait_turn_done(&sid, 0).await;
    let list = automations_of(&c);
    let saved = list.iter().find(|x| x["id"] == id).unwrap();
    assert!(saved["nextRun"].is_null(), "still nothing due after a run: {saved}");

    // Switching it off and on keeps it manual, with no next run.
    let off = save_automation(&mut c, json!({ "id": id, "enabled": false })).await.unwrap();
    assert!(off["nextRun"].is_null());
    let on = save_automation(&mut c, json!({ "id": id, "enabled": true })).await.unwrap();
    assert!(on["nextRun"].is_null(), "{on}");
}

#[tokio::test]
async fn a_run_is_skipped_when_engine_usage_is_above_the_limit() {
    let sb = Sandbox::new("auto-usage");
    let cwd = sb.folder("proj");
    let d = sb.start();
    let mut c = d.client().await;
    // The fake Codex reports 12% for the 5-hour window and 34% for the week.
    c.wait_for("codex usage", |m| (m["op"] == "usage" || m["op"] == "snapshot") && !m["usage"]["codex"].is_null()).await;
    let a = save_automation(&mut c, automation_of(&cwd, json!({ "maxFiveHour": 10 }))).await.unwrap();
    let id = a["id"].as_str().unwrap().to_string();
    let skipped = run_automation(&mut c, &id, false).await;
    assert_eq!(skipped["outcome"], "skipped", "{skipped}");
    assert!(skipped["reason"].as_str().unwrap().contains("5-hour usage 12%"), "{skipped}");

    save_automation(&mut c, json!({ "id": id, "maxFiveHour": 50, "maxWeek": 30 })).await.unwrap();
    let skipped = run_automation(&mut c, &id, false).await;
    assert!(skipped["reason"].as_str().unwrap().contains("weekly usage 34%"), "{skipped}");

    save_automation(&mut c, json!({ "id": id, "maxWeek": 90 })).await.unwrap();
    assert_eq!(run_automation(&mut c, &id, false).await["outcome"], "started");
}

#[tokio::test]
async fn the_scheduler_fires_a_missed_run_once_and_automations_survive_a_restart() {
    let sb = Sandbox::new("auto-sched");
    let cwd = sb.folder("proj");
    // yardd was down for hours: ten hourly slots are overdue.
    let overdue = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 - 10 * 3_600_000;
    let file = json!([{
        "id": "a1", "name": "Hourly", "prompt": "say hello", "cwd": cwd, "engine": "codex", "model": MODEL, "effort": EFFORT,
        "permission": "ask", "schedule": { "kind": "interval", "every": 60, "unit": "minutes" },
        "skipWhileRunning": true, "enabled": true, "createdAt": overdue - 1000, "nextRun": overdue,
    }, {
        "id": "a2", "name": "Off", "prompt": "never", "cwd": cwd, "engine": "codex", "model": MODEL, "effort": EFFORT,
        "permission": "ask", "schedule": { "kind": "interval", "every": 1, "unit": "minutes" },
        "skipWhileRunning": true, "enabled": false, "createdAt": overdue - 1000, "nextRun": null,
    }]);
    fs::write(sb.data().join("automations.json"), file.to_string()).unwrap();

    let d = sb.start_with(&[("YARD_AUTOMATION_TICK_MS", "100")]);
    let mut c = d.client().await;
    let fired = c.wait_for("the missed run", |m| m["op"] == "session" && m["session"]["automation"] == "a1").await;
    let sid = fired["session"]["id"].as_str().unwrap().to_string();
    c.wait_turn_done(&sid, 0).await;
    // Plenty of ticks later there is still exactly one run, and the next is an hour out.
    c.drain_for(Duration::from_millis(1200)).await;
    let runs = c.log.iter().filter(|m| m["op"] == "session" && m["session"]["automation"].is_string()).map(|m| m["session"]["id"].clone()).collect::<std::collections::HashSet<_>>();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let list = automations_of(&c);
    let hourly = list.iter().find(|a| a["id"] == "a1").unwrap();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
    assert!(hourly["nextRun"].as_u64().unwrap() > now + 3_000_000, "{hourly}");
    assert_eq!(hourly["lastRun"]["sessionId"], sid);
    d.stop();

    // After a restart the automations and the run are still there, and nothing fires again.
    let d = sb.start_with(&[("YARD_AUTOMATION_TICK_MS", "100")]);
    let mut c = d.client().await;
    assert_eq!(c.log[0]["automations"].as_array().unwrap().len(), 2);
    assert_eq!(c.log[0]["automations"][0]["lastRun"]["sessionId"], sid);
    let kept = c.log[0]["sessions"].as_array().unwrap().iter().filter(|s| s["automation"] == "a1").count();
    assert_eq!(kept, 1);
    c.drain_for(Duration::from_millis(800)).await;
    assert!(!c.log.iter().any(|m| m["op"] == "session" && m["session"]["automation"] == "a1" && m["session"]["id"] != sid));
}

#[tokio::test]
async fn a_missed_run_outside_the_window_creates_no_session_but_run_now_does() {
    use chrono::{Duration, Utc};
    let sb = Sandbox::new("auto-window-missed");
    let cwd = sb.folder("proj");
    let now = Utc::now();
    let window = json!({ "start": (now + Duration::hours(2)).format("%H:%M").to_string(), "end": (now + Duration::hours(3)).format("%H:%M").to_string() });
    let mut a = automation_of(&cwd, json!({ "id": "a1", "window": window, "nextRun": 1, "createdAt": 1 }));
    a["enabled"] = json!(true);
    fs::write(sb.data().join("automations.json"), json!([a]).to_string()).unwrap();
    let d = sb.start_with(&[("TZ", "UTC"), ("YARD_AUTOMATION_TICK_MS", "30")]);
    let mut c = d.client().await;
    // The scheduler may move the slot before the client connects; its first
    // snapshot is just as conclusive as a later rescheduling broadcast.
    let moved = c.wait_for("rescheduled instead of fired", |m| (m["op"] == "automations" || m["op"] == "snapshot") && m["automations"][0]["nextRun"].as_u64().is_some_and(|n| n > now.timestamp_millis() as u64)).await;
    let next = moved["automations"][0]["nextRun"].as_i64().unwrap();
    assert_eq!(chrono::DateTime::from_timestamp_millis(next).unwrap().format("%H:%M").to_string(), window["start"].as_str().unwrap());
    assert!(d.client().await.log[0]["sessions"].as_array().unwrap().is_empty());
    assert!(moved["automations"][0]["lastRun"].is_null(), "not even a skipped run");
    assert_eq!(run_automation(&mut c, "a1", true).await["outcome"], "started", "Run now ignores active hours");
}

#[tokio::test]
async fn a_hub_routes_automations_to_the_host_that_runs_them() {
    let remote_sb = Sandbox::new("auto-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("auto-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;

    let cwd = remote_sb.folder("proj");
    let window = json!({ "start": "21:00", "end": "09:00", "days": [5] });
    let from = c.mark();
    c.send(json!({ "op": "automation.save", "host": host, "automation": automation_of(&cwd, json!({ "window": window })), "ref": "save" })).await;
    let saved = c.wait_after(from, "saved on the remote", |m| m["op"] == "automation.saved" && m["ref"] == "save").await;
    let id = saved["automation"]["id"].as_str().unwrap().to_string();
    let list = c.wait_after(from, "the remote's list", |m| m["op"] == "automations" && m["hostId"] == host).await;
    assert_eq!(list["automations"][0]["id"], id);
    assert_eq!(list["automations"][0]["window"], window);
    assert_eq!(remote.client().await.log[0]["automations"][0]["window"], window);

    // It lives on the remote; the hub keeps none of its own.
    assert_eq!(remote.client().await.log[0]["automations"][0]["id"], id);
    assert_eq!(hub.client().await.log[0]["automations"], json!([]));

    // Run now goes to the remote too; the session comes back with the host's prefix.
    let from = c.mark();
    c.send(json!({ "op": "automation.run", "host": host, "id": id, "force": true, "ref": "run" })).await;
    let ran = c.wait_after(from, "the run", |m| m["op"] == "automation.ran" && m["ref"] == "run").await;
    assert_eq!(ran["outcome"], "started");
    let meta = c.wait_after(from, "the run's session", |m| m["op"] == "session" && m["session"]["automation"] == id).await;
    assert_eq!(meta["session"]["host"], host);
    assert!(meta["session"]["id"].as_str().unwrap().starts_with(&format!("{host}~")));
}

// Accounts: more than one login per engine, each with its own config folder
// (CLAUDE_CONFIG_DIR for Claude Code, CODEX_HOME for Codex) under the data dir.

/// `account.add`, waiting for the `accounts` list that holds the new one.
async fn add_account(c: &mut Client, engine: &str, name: &str) -> String {
    let from = c.mark();
    c.send(json!({ "op": "account.add", "engine": engine, "name": name })).await;
    let list = c
        .wait_after(from, "the accounts list with the new account", |m| {
            m["op"] == "accounts" && m["accounts"].as_array().is_some_and(|a| a.iter().any(|x| x["name"] == name))
        })
        .await;
    let found = list["accounts"].as_array().unwrap().iter().find(|a| a["name"] == name).unwrap().clone();
    assert_eq!(found["engine"], engine, "{list}");
    found["id"].as_str().unwrap().to_string()
}

fn claude_log(sb: &Sandbox) -> String {
    fs::read_to_string(sb.fake_state().join("claude.log")).unwrap_or_default()
}

fn sidecar_starts(sb: &Sandbox) -> Vec<String> {
    let log = fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap_or_default();
    log.lines().filter(|l| l.starts_with("start")).map(str::to_string).collect()
}

async fn log_in_claude_account(c: &mut Client, id: &str) {
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "claude", "account": id })).await;
    c.wait_after(mark, "the account's sign-in link", |m| m["op"] == "login" && m["account"] == id && m["state"] == "url").await;
    c.send(json!({ "op": "login.code", "engine": "claude", "account": id, "code": "good-code" })).await;
    c.wait_after(mark, "the account's login done", |m| m["op"] == "login" && m["account"] == id && m["state"] == "done").await;
}

#[tokio::test]
async fn an_account_is_added_logged_in_and_removed_without_touching_default() {
    let sb = Sandbox::new("accounts");
    let d = sb.start();
    let mut c = d.client().await;
    assert_eq!(c.log[0]["accounts"], json!([]), "the snapshot lists the added accounts");

    let id = add_account(&mut c, "claude", "Work").await;
    let dir = sb.data().join("accounts").join(&id);
    assert!(dir.is_dir(), "the account's config folder exists: {}", dir.display());

    // Names are unique per engine, and an engine must be known.
    let from = c.mark();
    c.send(json!({ "op": "account.add", "engine": "claude", "name": "work" })).await;
    let dup = c.wait_error(from, "account.add").await;
    assert!(dup.contains("already"), "{dup}");
    let from = c.mark();
    c.send(json!({ "op": "account.add", "engine": "not-an-engine", "name": "X" })).await;
    c.wait_error(from, "account.add").await;

    // Status is per account: Default and Work are both logged out.
    let from = c.mark();
    c.send(json!({ "op": "engines.status" })).await;
    let before = c.wait_after(from, "engine status", |m| m["op"] == "engines").await;
    assert_eq!(before["engines"]["claude"]["loggedIn"], false, "{before}");
    assert_eq!(before["accounts"][&id]["loggedIn"], false, "{before}");

    log_in_claude_account(&mut c, &id).await;
    let from = c.mark();
    c.send(json!({ "op": "engines.status" })).await;
    // Adding the account and logging in also broadcast a status; on a loaded
    // machine one computed before the login can land after `from`.
    let after = c.wait_after(from, "engine status after the login", |m| m["op"] == "engines" && m["accounts"][&id]["loggedIn"] == true).await;
    assert_eq!(after["accounts"][&id]["account"], format!("{id}@example.com"), "{after}");
    assert_eq!(after["engines"]["claude"]["loggedIn"], false, "the Default login is untouched: {after}");
    assert!(dir.join("claude-logged-in").exists(), "the login landed in the account's folder");
    assert!(!sb.fake_state().join("claude-logged-in").exists(), "and not in the Default one");
    assert!(claude_log(&sb).contains(&format!("args=auth login config={}", dir.display())), "{}", claude_log(&sb));

    // It survives a restart, login included.
    d.kill();
    let d = sb.start();
    let mut c = d.client().await;
    assert_eq!(c.log[0]["accounts"], json!([{ "id": id, "engine": "claude", "name": "Work" }]));
    let from = c.mark();
    c.send(json!({ "op": "engines.status" })).await;
    let again = c.wait_after(from, "engine status", |m| m["op"] == "engines").await;
    assert_eq!(again["accounts"][&id]["loggedIn"], true, "{again}");

    // Removing it deletes its folder (and so its login) and leaves Default alone.
    c.send(json!({ "op": "account.remove", "id": id })).await;
    c.wait_for("the empty accounts list", |m| m["op"] == "accounts" && m["accounts"] == json!([])).await;
    assert!(!dir.exists(), "the folder is gone");
    let from = c.mark();
    c.send(json!({ "op": "engines.status" })).await;
    let last = c.wait_after(from, "engine status", |m| m["op"] == "engines").await;
    assert!(last["accounts"].get(&id).is_none(), "{last}");
    d.kill();
    let d = sb.start();
    assert_eq!(d.client().await.log[0]["accounts"], json!([]));
}

#[tokio::test]
async fn a_claude_session_runs_under_its_accounts_config_dir() {
    let sb = Sandbox::new("accounts-claude");
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let id = add_account(&mut c, "claude", "Work").await;
    let dir = sb.data().join("accounts").join(&id);
    let claude = |extra: Value| {
        let mut v = json!({ "engine": "claude", "model": "claude-haiku-5-5" });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };

    let work = c.create(&sb.folder("work"), "on the company plan", claude(json!({ "account": id }))).await;
    assert_eq!(c.wait_turn_done(&work, 0).await["status"], "completed");
    let plain = c.create(&sb.folder("own"), "on my own plan", claude(json!({}))).await;
    assert_eq!(c.wait_turn_done(&plain, 0).await["status"], "completed");

    let starts = sidecar_starts(&sb);
    assert_eq!(starts.len(), 2, "{starts:?}");
    assert!(starts[0].contains(&format!("config={} ", dir.display())), "{starts:?}");
    assert!(starts[1].contains("config= "), "Default runs with no config dir set: {starts:?}");

    // The account is saved on the session, and each account has its own usage.
    let meta = c.wait_for("the work session", |m| m["op"] == "session" && m["session"]["id"] == work).await;
    assert_eq!(meta["session"]["account"], id);
    let meta = c.wait_for("the plain session", |m| m["op"] == "session" && m["session"]["id"] == plain).await;
    assert_eq!(meta["session"]["account"], "default");
    let usage = c.wait_for("usage for both accounts", |m| {
        m["op"] == "usage"
            && m["usage"]["accounts"][&id]["windows"]["five_hour"]["utilization"] == 0.91
            && m["usage"]["claude"]["windows"]["five_hour"]["utilization"] == 0.47
    }).await;
    let _ = usage;

    // Unknown accounts and another engine's account are refused.
    let err = c.try_create(&sb.folder("x"), "x", claude(json!({ "account": "nope" }))).await.unwrap_err();
    assert!(err.contains("account"), "{err}");
    let err = c.try_create(&sb.folder("y"), "y", json!({ "account": id })).await.unwrap_err();
    assert!(err.contains("account"), "{err}");

    // An account that sessions use cannot be removed.
    let from = c.mark();
    c.send(json!({ "op": "account.remove", "id": id })).await;
    let refused = c.wait_error(from, "account.remove").await;
    assert!(refused.contains("session"), "{refused}");
    assert!(dir.is_dir());

    // After a restart the session resumes under the same account.
    c.wait_status(&work, "idle").await;
    let deadline = Instant::now() + WAIT;
    while saved_meta(&sb, &work).map(|m| m["account"] == json!(id)) != Some(true) {
        assert!(Instant::now() < deadline, "the account never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let kept = c.log[0]["sessions"].as_array().unwrap().iter().find(|s| s["id"] == work).unwrap().clone();
    assert_eq!(kept["account"], id);
    c.send(json!({ "op": "session.open", "sessionId": work })).await;
    c.send(json!({ "op": "session.send", "sessionId": work, "text": "back again" })).await;
    c.wait_event(&work, "turn_completed", 0).await;
    let starts = sidecar_starts(&sb);
    let last = starts.last().unwrap();
    assert!(last.contains(&format!("config={} ", dir.display())) && last.contains("resume=fake-claude-"), "{starts:?}");
}

#[tokio::test]
async fn a_codex_session_runs_on_its_accounts_own_app_server() {
    let sb = Sandbox::new("accounts-codex");
    let d = sb.start();
    let mut c = d.client().await;
    let id = add_account(&mut c, "codex", "Work").await;
    let dir = sb.data().join("accounts").join(&id);

    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "codex", "account": id })).await;
    c.wait_after(mark, "the account's device code", |m| m["op"] == "login" && m["account"] == id && m["state"] == "code").await;
    c.wait_after(mark, "the account's login done", |m| m["op"] == "login" && m["account"] == id && m["state"] == "done").await;
    let status = c
        .wait_after(mark, "the account logged in", |m| m["op"] == "engines" && m["accounts"][&id]["loggedIn"] == true)
        .await;
    assert_eq!(status["accounts"][&id]["account"], format!("{id}@example.com"));
    assert_eq!(status["engines"]["codex"]["loggedIn"], false, "Default stays logged out: {status}");
    assert!(dir.join("codex-logged-in").exists());

    let work = c.create(&sb.folder("work"), "company task", json!({ "account": id })).await;
    assert_eq!(c.wait_turn_done(&work, 0).await["status"], "completed");
    let plain = c.create(&sb.folder("own"), "own task", json!({})).await;
    assert_eq!(c.wait_turn_done(&plain, 0).await["status"], "completed");

    // One app-server per account, each started with its own CODEX_HOME.
    let log = sb.codex_log();
    let starts: Vec<&str> = log.lines().filter(|l| l.starts_with("start")).collect();
    assert!(starts.iter().any(|l| l.ends_with(&format!("home={}", dir.display()))), "{log}");
    assert!(starts.iter().any(|l| l.ends_with("home=")), "{log}");
    let inputs: Vec<Value> = fs::read_to_string(sb.fake_state().join("turn-inputs.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(inputs[0]["home"], dir.to_str().unwrap(), "{inputs:?}");
    assert_eq!(inputs[1]["home"], "", "{inputs:?}");

    // Limits are per account.
    let usage = c.wait_for("usage for the account", |m| {
        m["op"] == "usage" && m["usage"]["accounts"][&id]["primary"]["usedPercent"] == 77
    }).await;
    assert_eq!(usage["usage"]["codex"]["primary"]["usedPercent"], 12, "{usage}");

    // A restart resumes the thread on the account's server: the other one never saw it.
    c.wait_status(&work, "idle").await;
    let deadline = Instant::now() + WAIT;
    while saved_meta(&sb, &work).map(|m| m["account"] == json!(id)) != Some(true) {
        assert!(Instant::now() < deadline, "the account never reached the database");
        std::thread::sleep(Duration::from_millis(10));
    }
    d.kill();
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "session.open", "sessionId": work })).await;
    c.send(json!({ "op": "session.send", "sessionId": work, "text": "again" })).await;
    assert_eq!(c.wait_turn_done(&work, 0).await["status"], "completed");
    let answer = c.wait_event(&work, "assistant_message", 0).await;
    assert_eq!(answer["text"], "Echo: again");
}

#[tokio::test]
async fn accounts_are_per_host_through_the_hub() {
    let remote_sb = Sandbox::new("accounts-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("accounts-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;

    let from = c.mark();
    c.send(json!({ "op": "account.add", "engine": "claude", "name": "Work", "host": host })).await;
    let list = c.wait_after(from, "the remote's accounts", |m| m["op"] == "accounts" && m["hostId"] == host).await;
    let id = list["accounts"][0]["id"].as_str().unwrap().to_string();
    assert!(remote_sb.data().join("accounts").join(&id).is_dir(), "the folder is on the remote");
    assert!(!hub_sb.data().join("accounts").exists(), "and not on the hub");

    // Its status comes back tagged with the host.
    let from = c.mark();
    c.send(json!({ "op": "engines.status", "host": host })).await;
    let status = c.wait_after(from, "the remote's engine status", |m| m["op"] == "engines" && m["hostId"] == host).await;
    assert_eq!(status["accounts"][&id]["loggedIn"], false, "{status}");

    c.send(json!({ "op": "account.remove", "id": id, "host": host })).await;
    c.wait_for("the remote's empty list", |m| m["op"] == "accounts" && m["hostId"] == host && m["accounts"] == json!([])).await;
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid]).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

#[tokio::test]
async fn removing_a_codex_account_stops_its_app_server() {
    let sb = Sandbox::new("accounts-rm-codex");
    let d = sb.start();
    let mut c = d.client().await;
    let id = add_account(&mut c, "codex", "Work").await;
    let dir = sb.data().join("accounts").join(&id);
    // A session on the account starts its app-server.
    let sid = c.create(&sb.folder("work"), "task", json!({ "account": id })).await;
    c.wait_turn_done(&sid, 0).await;
    let pid = sb
        .codex_log()
        .lines()
        .find(|l| l.starts_with("start") && l.ends_with(&format!("home={}", dir.display())))
        .and_then(|l| l.split_whitespace().find_map(|w| w.strip_prefix("pid=")))
        .expect("the account's app-server started")
        .to_string();
    assert!(alive(&pid));

    c.send(json!({ "op": "session.delete", "sessionId": sid })).await;
    c.wait_for("the session gone", |m| m["op"] == "session.deleted").await;
    c.send(json!({ "op": "account.remove", "id": id })).await;
    c.wait_for("the empty accounts list", |m| m["op"] == "accounts" && m["accounts"] == json!([])).await;
    let deadline = Instant::now() + WAIT;
    while alive(&pid) {
        assert!(Instant::now() < deadline, "the removed account's app-server (pid {pid}) is still running");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Ask `session.procs` until the list has a process whose command line
/// contains `needle`: the list comes from the host sampler, a few seconds
/// behind.
async fn procs_with(c: &mut Client, sid: &str, needle: &str) -> Vec<Value> {
    for _ in 0..15 {
        let from = c.mark();
        c.send(json!({ "op": "session.procs", "sessionId": sid })).await;
        let reply = c.wait_after(from, "a session.procs reply", |m| m["op"] == "session.procs" && m["sessionId"] == sid).await;
        let procs = reply["procs"].as_array().expect("procs is a list").clone();
        if procs.iter().any(|p| p["cmd"].as_str().is_some_and(|cmd| cmd.contains(needle))) {
            return procs;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    panic!("no process with `{needle}` in the list of {sid}");
}

/// A subagent Claude runs in the background outlives its turn: the session
/// says it is still at work (`background`) until the subagent reports.
#[tokio::test]
async fn a_background_subagent_keeps_its_session_marked_working() {
    let sb = Sandbox::new("background-agent");
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let extra = json!({ "engine": "claude", "model": "claude-haiku-5-5" });
    let sid = c.create(&sb.folder("proj"), "[background-agent] TEST-go", extra).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let meta = c.wait_for("idle with a subagent at work", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["status"] == "idle" && m["session"]["background"].as_array().is_some_and(|b| b.len() == 1)).await;
    let item = meta["session"]["background"][0].as_str().unwrap().to_string();
    // It reports after the turn: the mark goes.
    let from = c.mark();
    c.wait_after(from, "the subagent done", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"].get("background").is_none()).await;
    assert!(c.history(&sid).await.iter().any(|e| e["type"] == "tool_completed" && e["itemId"] == item.as_str()));
}

#[tokio::test]
async fn session_procs_lists_what_a_session_is_running() {
    let sb = Sandbox::new("procs");
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let extra = json!({ "engine": "claude", "model": "claude-haiku-5-5" });
    // The scripted sidecar leaves a `sleep 30` running under itself.
    let sid = c.create(&sb.folder("proj"), "[subagent] look around", extra).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");

    let procs = procs_with(&mut c, &sid, "sleep 30").await;
    let sleep = procs.iter().find(|p| p["cmd"].as_str().is_some_and(|c| c.contains("sleep 30"))).unwrap();
    assert_eq!(sleep["name"], "sleep");
    assert!(sleep["pid"].as_u64().unwrap() > 0);
    assert!(sleep["cpu"].is_number() && sleep["memBytes"].as_u64().is_some() && sleep["ageSecs"].as_u64().is_some());
    assert_eq!(sleep["origin"], "agent");
    // A tree: the sleep's parent is in the list, and so is the sidecar's own process.
    let parent = procs.iter().find(|p| p["pid"] == sleep["ppid"]).expect("the sleep's parent is listed");
    assert!(parent["cmd"].as_str().unwrap().contains("yard-claude"), "{parent}");

    // A terminal's shell is listed too, as the terminal's.
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 80, "rows": 24 })).await;
    c.wait_for("the terminal", |m| m["op"] == "term.opened").await;
    for _ in 0..15 {
        let from = c.mark();
        c.send(json!({ "op": "session.procs", "sessionId": sid })).await;
        let reply = c.wait_after(from, "a session.procs reply", |m| m["op"] == "session.procs").await;
        if reply["procs"].as_array().unwrap().iter().any(|p| p["origin"] == "terminal") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    panic!("the terminal's shell never showed in the list");
}

#[tokio::test]
async fn session_procs_for_an_unknown_session_is_an_error() {
    let sb = Sandbox::new("procs-unknown");
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "session.procs", "sessionId": "nope" })).await;
    let err = c.wait_for("an error", |m| m["op"] == "error" && m["for"] == "session.procs").await;
    assert!(err["message"].as_str().unwrap().contains("no such session"), "{err}");
}

#[tokio::test]
async fn session_procs_goes_through_the_hub() {
    let sidecar = claude_sidecar();
    let remote_sb = Sandbox::new("procs-remote");
    let remote = remote_sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let hub_sb = Sandbox::new("procs-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    c.send(json!({ "op": "host.add", "name": "box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the direct host to connect", |m| {
            m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["status"] == "connected"))
        })
        .await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the host's snapshot", |m| m["op"] == "host.snapshot" && m["hostId"] == host).await;

    let extra = json!({ "host": host, "engine": "claude", "model": "claude-haiku-5-5" });
    let sid = c.create(&remote_sb.folder("proj"), "[subagent] look around", extra).await;
    assert!(sid.starts_with(&format!("{host}~")));
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    // The reply carries the hub's id for the session, like every session op.
    procs_with(&mut c, &sid, "sleep 30").await;
}

#[tokio::test]
async fn a_session_whose_folder_is_gone_says_so() {
    let sb = Sandbox::new("gone");
    let d = sb.start();
    let mut c = d.client().await;
    let folder = sb.folder("proj");
    let sid = c.create(&folder, "hi", json!({})).await;
    c.wait_event(&sid, "turn_completed", 0).await;
    fs::remove_dir_all(&folder).unwrap();

    // Not "not a git repository": that sends the user looking for a git problem.
    c.send(json!({ "op": "changes.get", "sessionId": sid })).await;
    let changes = c.wait_for("changes", |m| m["op"] == "changes").await;
    let error = changes["changes"]["error"].as_str().unwrap_or_default();
    assert!(error.contains("no longer exists") && error.contains(&folder), "{changes}");

    // A shell there would silently start in the home folder instead.
    let from = c.mark();
    c.send(json!({ "op": "term.open", "sessionId": sid, "cols": 80, "rows": 24 })).await;
    let refused = c.wait_after(from, "term.open refused", |m| m["op"] == "error" || m["op"] == "term.opened").await;
    assert_eq!(refused["op"], "error", "{refused}");
    assert!(refused["message"].as_str().unwrap_or_default().contains("no longer exists"), "{refused}");
}

#[tokio::test]
async fn a_message_to_a_session_whose_folder_is_gone_says_so() {
    let sb = Sandbox::new("gonesend");
    let d = sb.start();
    let mut c = d.client().await;
    for engine in ["codex", "claude"] {
        let folder = sb.folder(&format!("proj-{engine}"));
        // As the merged-worktree sweep leaves it: Claude's sidecar stopped, folder gone.
        let first = if engine == "claude" { "[die] hi" } else { "hi" };
        let sid = c.create(&folder, first, json!({ "engine": engine })).await;
        c.wait_turn_done(&sid, 0).await;
        c.wait_status(&sid, "idle").await;
        fs::remove_dir_all(&folder).unwrap();

        // The engine would fail its own way: Claude Code's SDK blames libc.
        let seq = c.next_seq(&sid);
        c.send(json!({ "op": "session.send", "sessionId": sid, "text": "keep going" })).await;
        let error = c.wait_event(&sid, "error", seq).await;
        let message = error["message"].as_str().unwrap_or_default();
        assert!(message.contains("no longer exists") && message.contains(&folder), "{engine}: {error}");
        assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "failed", "{engine}");
    }
}

#[tokio::test]
async fn claude_usage_survives_a_restart() {
    // Claude reports its limits only during a turn, so without saving them a
    // restart blanks the meters and lets a usage-capped automation run.
    let sb = Sandbox::new("usage-restart");
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({ "engine": "claude", "model": "claude-haiku-5-5" })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    c.wait_for("claude usage", |m| m["op"] == "usage" && m["usage"]["claude"]["windows"]["five_hour"]["utilization"] == 0.47).await;
    d.stop();

    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let snapshot = d.client().await.log[0].clone();
    assert_eq!(snapshot["usage"]["claude"]["windows"]["five_hour"]["utilization"], 0.47, "{}", snapshot["usage"]);
}

#[tokio::test]
async fn the_page_is_sent_so_a_web_view_rechecks_it() {
    // Without it, the Mac app kept showing an old cached page after an update.
    let sb = Sandbox::new("cache");
    let d = sb.start();
    let page = http_get(d.port, "/").unwrap().to_lowercase();
    assert!(page.contains("cache-control: no-cache\r\n"), "{}", page.lines().take(12).collect::<Vec<_>>().join("\n"));
}

#[tokio::test]
async fn pr_get_names_a_local_base_for_a_review_in_a_repo_without_a_remote() {
    // The pane offers a review here (no PR to read), and its prompt diffs
    // against this base: a repo with no origin has no origin/HEAD to name.
    let sb = Sandbox::new("pr-no-remote");
    let d = sb.start();
    let mut c = d.client().await;
    let home = sb.home();
    let repo = make_repo(&sb, "proj");
    git(&repo, &["checkout", "-q", "feature"], &home);
    let sid = c.create(&repo, "hello", json!({ "worktree": false })).await;
    c.wait_turn_done(&sid, 0).await;
    let from = c.mark();
    c.send(json!({ "op": "pr.get", "sessionId": sid })).await;
    let reply = c.wait_after(from, "a pr reply", |m| m["op"] == "pr" && m["sessionId"] == sid).await;
    assert_eq!(reply["pr"]["repo"], true, "{reply}");
    assert_eq!(reply["pr"]["defaultBranch"], "main", "{reply}");

    // Nothing to fall back on: no base is named rather than a guessed one.
    git(&repo, &["branch", "-q", "-m", "main", "trunk"], &home);
    let from = c.mark();
    c.send(json!({ "op": "pr.get", "sessionId": sid })).await;
    let reply = c.wait_after(from, "a pr reply", |m| m["op"] == "pr" && m["sessionId"] == sid).await;
    assert_eq!(reply["pr"]["repo"], true, "{reply}");
    assert!(reply["pr"]["defaultBranch"].is_null(), "{reply}");
}

#[tokio::test]
async fn pr_get_says_when_the_folder_is_a_repository_it_cannot_read_a_pr_for() {
    let sb = Sandbox::new("pr-detached");
    let d = sb.start();
    let mut c = d.client().await;
    let home = sb.home();
    let repo = make_repo(&sb, "proj");
    let sid = c.create(&repo, "hello", json!({ "worktree": false })).await;
    c.wait_turn_done(&sid, 0).await;
    // No branch, so there is no PR to look up, but there is still a diff to review.
    git(&repo, &["checkout", "-q", "--detach"], &home);
    let from = c.mark();
    c.send(json!({ "op": "pr.get", "sessionId": sid })).await;
    let reply = c.wait_after(from, "a pr reply", |m| m["op"] == "pr" && m["sessionId"] == sid).await;
    assert_eq!(reply["pr"]["error"], "no branch is checked out", "{reply}");
    assert_eq!(reply["pr"]["repo"], true, "{reply}");

    // A plain folder has nothing to review and no PR, and that is an answer,
    // not a failure: the session header asks every minute (#309).
    let plain = sb.folder("plain");
    let sid = c.create(&plain, "hello", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    let from = c.mark();
    c.send(json!({ "op": "pr.get", "sessionId": sid })).await;
    let reply = c.wait_after(from, "a pr reply", |m| m["op"] == "pr" && m["sessionId"] == sid).await;
    assert_eq!(reply["pr"]["repo"], false, "{reply}");
    assert!(reply["pr"]["error"].is_null(), "{reply}");
}

#[tokio::test]
async fn removing_an_account_in_use_names_its_sessions() {
    let sb = Sandbox::new("accounts-rm-in-use");
    let d = sb.start();
    let mut c = d.client().await;
    let id = add_account(&mut c, "codex", "Work").await;
    async fn refusal(c: &mut Client, id: &str) -> String {
        let from = c.mark();
        c.send(json!({ "op": "account.remove", "id": id })).await;
        c.wait_error(from, "account.remove").await
    }

    // An archived session is not in the sidebar, so the refusal has to say
    // which one it is and where it went.
    let old = c.create(&sb.folder("old"), "fix the login page", json!({ "account": id })).await;
    c.wait_turn_done(&old, 0).await;
    c.send(json!({ "op": "session.update", "sessionId": old, "archived": true })).await;
    c.wait_for("the archived session", |m| m["op"] == "session" && m["session"]["id"] == old && m["session"]["archived"] == true).await;
    let one = refusal(&mut c, &id).await;
    assert_eq!(one, "The session \"fix the login page\" (archived) still uses this account. Delete it first.");

    let new = c.create(&sb.folder("new"), "write the docs", json!({ "account": id })).await;
    c.wait_turn_done(&new, 0).await;
    let two = refusal(&mut c, &id).await;
    assert_eq!(two, "2 sessions still use this account: \"fix the login page\" (archived) and \"write the docs\". Delete them first.");
}

/// Claude's AskUserQuestion reached the owner as "Allow this?" with the raw
/// JSON, and allowing it told Claude "The user did not answer the questions."
/// The questions come as a request of kind `question`; the owner's answers go
/// to Claude with the decision and stay in the log for the card.
#[tokio::test]
async fn a_claude_question_takes_the_owners_answers() {
    let sb = Sandbox::new("question");
    let sidecar = fake_bin().join("yard-claude");
    let d = sb.start_with(&[("YARD_SIDECAR", sidecar.to_str().unwrap())]);
    let mut c = d.client().await;
    let claude = json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "medium" });
    let sid = c.create(&sb.folder("proj"), "[question] TEST-pick", claude).await;
    let req = c.wait_event(&sid, "approval_request", 0).await;
    assert_eq!(req["kind"], "question");
    assert_eq!(req["questions"].as_array().map(Vec::len), Some(2));
    c.wait_status(&sid, "needs_approval").await;

    let answers = json!({
        "Which database should TEST-app use?": "SQLite",
        "Which checks should run before merging?": "Unit tests, Lint",
    });
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow", "answers": answers })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let answer = c.events(&sid).into_iter().rev().find(|e| e["type"] == "assistant_message").unwrap();
    assert_eq!(
        answer["text"],
        "Answered: Which database should TEST-app use? → SQLite | Which checks should run before merging? → Unit tests, Lint"
    );
    // The card shows what was answered, after a reload too.
    let resolved = c.events(&sid).into_iter().find(|e| e["type"] == "approval_resolved").unwrap();
    assert_eq!(resolved["answers"], answers);

    // Skip: Claude hears that the owner chose not to answer.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[question] TEST-again" })).await;
    let req = c.wait_event(&sid, "approval_request", next).await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "deny" })).await;
    let done = c.wait_event(&sid, "tool_completed", next).await;
    assert_eq!(done["ok"], false);
    c.wait_turn_done(&sid, next).await;

    // Answers are text: anything else is refused before Claude hears it.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[question] TEST-bad" })).await;
    let req = c.wait_event(&sid, "approval_request", next).await;
    let mark = c.mark();
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow", "answers": { "Which database should TEST-app use?": 3 } })).await;
    let refused = c.wait_after(mark, "the refusal", |m| m["op"] == "error" && m["for"] == "session.approve").await;
    assert!(refused["message"].as_str().unwrap_or("").contains("answers"), "{refused}");
}

/// Codex's request_user_input got "not supported", and yardd started Codex
/// without the feature that offers it outside Plan mode, so Codex never asked.
/// It is the same question card as Claude's: answers by question text go to
/// Codex by question id, a secret answer never reaches the log, Skip sends none.
#[tokio::test]
async fn a_codex_question_takes_the_owners_answers() {
    let sb = Sandbox::new("codex-question");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[question] TEST-pick", json!({})).await;
    let req = c.wait_event(&sid, "approval_request", 0).await;
    assert_eq!(req["kind"], "question");
    assert_eq!(req["title"], "Asked Which color should TEST-app use?");
    assert_eq!(req["questions"][0]["options"][1]["label"], "Blue");
    assert_eq!(req["questions"][1]["secret"], true);
    assert_eq!(req["questions"][1]["options"], json!([]));
    c.wait_status(&sid, "needs_approval").await;
    assert!(sb.codex_log().contains("features.default_mode_request_user_input=true"), "{}", sb.codex_log());

    let answers = json!({
        "Which color should TEST-app use?": "Blue",
        "Paste the TEST-app deploy token.": "s3cret-TEST",
    });
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow", "answers": answers })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let answer = c.events(&sid).into_iter().rev().find(|e| e["type"] == "assistant_message").unwrap();
    assert_eq!(answer["text"], "Answered: color=Blue | token=s3cret-TEST");
    let resolved = c.events(&sid).into_iter().find(|e| e["type"] == "approval_resolved").unwrap();
    assert_eq!(resolved["answers"]["Which color should TEST-app use?"], "Blue");
    assert!(!resolved.to_string().contains("s3cret"), "a secret answer is not logged: {resolved}");

    // Skip: Codex hears no answers.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[question] TEST-again" })).await;
    let req = c.wait_event(&sid, "approval_request", next).await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "deny" })).await;
    c.wait_turn_done(&sid, next).await;
    let answer = c.events(&sid).into_iter().rev().find(|e| e["type"] == "assistant_message").unwrap();
    assert_eq!(answer["text"], "No answers.");
}

#[tokio::test]
async fn codex_usage_keeps_the_full_resets_and_every_limit() {
    // Codex answers its free "Full reset" credits and a limit per id beside
    // the main windows; yardd kept only the main windows.
    let sb = Sandbox::new("codex-resets");
    let d = sb.start();
    let mut c = d.client().await;
    let usage = c
        .wait_for("codex usage", |m| (m["op"] == "usage" || m["op"] == "snapshot") && !m["usage"]["codex"].is_null())
        .await;
    let codex = &usage["usage"]["codex"];
    assert_eq!(codex["primary"]["usedPercent"], 12, "{codex}");
    assert_eq!(codex["resetCredits"]["availableCount"], 2, "{codex}");
    assert_eq!(codex["resetCredits"]["credits"][0]["title"], "Full reset", "{codex}");
    assert_eq!(codex["byLimitId"]["codex_spark"]["primary"]["usedPercent"], 5, "{codex}");
}

#[tokio::test]
async fn a_claude_turn_keeps_the_windows_it_did_not_report() {
    // A turn's rate_limit_events name only the windows that turn touched;
    // they replaced everything, so the per-model week the usage report had
    // read (Fable's) vanished from the meters until the next report.
    let sb = Sandbox::new("usage-merge");
    let reset = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3 * 86400;
    std::fs::create_dir_all(sb.data()).unwrap();
    std::fs::write(
        sb.data().join("usage.json"),
        json!({ "claude": { "windows": {
            "five_hour": { "utilization": 0.1, "resetsAt": reset },
            "seven_day_fable": { "utilization": 0.04, "resetsAt": reset },
        } } })
        .to_string(),
    )
    .unwrap();
    let sidecar = claude_sidecar();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar)]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", json!({ "engine": "claude", "model": "claude-haiku-5-5" })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let usage = c.wait_for("claude usage", |m| m["op"] == "usage" && m["usage"]["claude"]["windows"]["five_hour"]["utilization"] == 0.47).await;
    let windows = &usage["usage"]["claude"]["windows"];
    assert_eq!(windows["seven_day_sonnet"]["utilization"], 0.08, "{windows}");
    assert_eq!(windows["seven_day_fable"]["utilization"], 0.04, "{windows}");
}
