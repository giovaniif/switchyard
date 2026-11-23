//! Session names from the conversation (`naming.rs`): the fake Codex stands in
//! for `gpt-5.6-luna` (as `fake-fast`) and the fake sidecar for Haiku. No model
//! is reached.

mod common;

use std::{
    fs,
    time::{Duration, Instant},
};

use common::*;
use serde_json::{json, Value};

const ON: [(&str, &str); 2] = [("YARD_SESSION_NAMES", "on"), ("YARD_NAMING_CODEX_MODEL", "fake-fast")];

/// What each naming turn on the fake Codex was given.
fn naming_inputs(sb: &Sandbox) -> Vec<String> {
    sb.turn_inputs()
        .iter()
        .map(|i| i[0]["text"].as_str().unwrap_or("").to_string())
        .filter(|t| t.starts_with("Name this coding session"))
        .collect()
}

async fn titled(c: &mut Client, sid: &str, title: &str) -> Value {
    let what = format!("{sid} to be titled {title:?}");
    c.wait_for(&what, |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["title"] == title)
        .await["session"]
        .clone()
}

async fn turn(c: &mut Client, sid: &str, text: &str) {
    let from = c.next_seq(sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": text })).await;
    let done = c.wait_turn_done(sid, from).await;
    assert_eq!(done["status"], "completed");
}

async fn until(what: &str, ok: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test]
async fn a_session_is_named_from_its_conversation_after_its_first_and_third_turns() {
    let sb = Sandbox::new("naming");
    let d = sb.start_with(&ON);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[name] hey can you look at the sidebar\nit overflows", json!({})).await;
    c.wait_turn_done(&sid, 0).await;

    let session = titled(&mut c, &sid, "Fake named session").await;
    assert_eq!(session["titleSource"], "auto");
    // The store writes behind the broadcast.
    until("the title on disk", || saved_meta(&sb, &sid).is_some_and(|m| m["title"] == "Fake named session" && m["titleSource"] == "auto")).await;

    let input = &naming_inputs(&sb)[0];
    assert!(input.contains("\n\nUser: [name] hey can you look at the sidebar it overflows\nAssistant: Echo:"), "{input}");
    // A private, read-only, ephemeral thread that never asks for approval.
    let starts = fs::read_to_string(sb.fake_state().join("thread-starts.jsonl")).unwrap();
    let naming: Value = starts.lines().map(|l| serde_json::from_str::<Value>(l).unwrap()).find(|s| s["ephemeral"] == true).expect("a naming thread");
    assert_eq!((naming["sandbox"].as_str(), naming["approvalPolicy"].as_str()), (Some("read-only"), Some("never")));
    assert!(!c.log.iter().any(|m| m["op"] == "session" && m["session"]["id"].as_str().is_some_and(|id| crate_private(id))));

    // Not after the second turn; again after the third, with more context.
    turn(&mut c, &sid, "and the group header too").await;
    c.drain_for(Duration::from_millis(500)).await;
    assert_eq!(naming_inputs(&sb).len(), 1);
    turn(&mut c, &sid, "thanks").await;
    until("a second naming turn", || naming_inputs(&sb).len() == 2).await;
    let input = &naming_inputs(&sb)[1];
    assert!(input.contains("User: and the group header too") && input.contains("User: thanks"), "{input}");
}

#[tokio::test]
async fn a_title_typed_by_the_user_is_never_replaced() {
    let sb = Sandbox::new("naming-user");
    let d = sb.start_with(&ON);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[name] first task", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    titled(&mut c, &sid, "Fake named session").await;

    c.send(json!({ "op": "session.rename", "sessionId": sid, "title": " My own name " })).await;
    let session = titled(&mut c, &sid, "My own name").await;
    assert_eq!(session["titleSource"], "user");

    turn(&mut c, &sid, "two").await;
    turn(&mut c, &sid, "three").await;
    c.drain_for(Duration::from_millis(800)).await;
    assert_eq!(naming_inputs(&sb).len(), 1, "no naming after a rename");
    assert_eq!(saved_meta(&sb, &sid).unwrap()["title"], "My own name");
}

#[tokio::test]
async fn haiku_names_the_session_when_codex_fails() {
    let sb = Sandbox::new("naming-claude");
    let sidecar = fake_bin().join("yard-claude");
    let d = sb.start_with(&[ON[0], ON[1], ("YARD_SIDECAR", sidecar.to_str().unwrap())]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[name-fail] tidy the docs", json!({})).await;
    c.wait_turn_done(&sid, 0).await;

    let session = titled(&mut c, &sid, "Claude named session").await;
    assert_eq!(session["titleSource"], "auto");
    assert_eq!(naming_inputs(&sb).len(), 1, "Codex was asked first");
    // Haiku, in a tool-less private sidecar.
    let log = fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap_or_default();
    assert!(log.lines().any(|l| l.contains("start pid=") && l.contains("restricted=true")), "{log}");
    assert!(!c.log.iter().any(|m| m["op"] == "session" && m["session"]["id"].as_str().is_some_and(|id| crate_private(id))));
}

#[tokio::test]
async fn with_no_naming_model_the_title_stays() {
    let sb = Sandbox::new("naming-none");
    // The fake lists neither model.
    let d = sb.start_with(&[("YARD_SESSION_NAMES", "on"), ("YARD_NAMING_CLAUDE_MODEL", "claude-none")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[name] fix the login", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    until("the namer to give up", || d.log_text().contains("could not name session")).await;
    assert!(d.log_text().contains("neither the Codex nor the Claude naming model is available"), "{}", d.log_text());
    let saved = saved_meta(&sb, &sid).unwrap();
    assert_eq!(saved["title"], "[name] fix the login");
    assert!(saved["titleSource"].is_null());
    assert!(naming_inputs(&sb).is_empty());
}

#[tokio::test]
async fn naming_can_be_turned_off() {
    let sb = Sandbox::new("naming-off");
    let d = sb.start_with(&[("YARD_SESSION_NAMES", "off"), ("YARD_NAMING_CODEX_MODEL", "fake-fast")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[name] anything", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    c.drain_for(Duration::from_millis(800)).await;
    assert!(naming_inputs(&sb).is_empty());
    assert_eq!(saved_meta(&sb, &sid).unwrap()["title"], "[name] anything");
}

/// A private engine thread's id (crates/yardd/src/private.rs).
fn crate_private(id: &str) -> bool {
    id.starts_with("private-") || id.starts_with("switchy-")
}
