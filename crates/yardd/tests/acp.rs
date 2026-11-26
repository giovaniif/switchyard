//! The ACP engines (OpenCode, Grok, Gemini) end to end: the built daemon with the
//! scripted `opencode` and `grok` in `tests/fake-codex` first on PATH. No
//! turn reaches a model and no real login is used.

mod common;

use std::{fs, time::Duration};

use common::*;
use serde_json::{json, Value};

fn acp_log(sb: &Sandbox) -> String {
    fs::read_to_string(sb.fake_state().join("acp.log")).unwrap_or_default()
}

/// Every prompt the fake agents received, with the session's model, effort and mode then.
fn prompts(sb: &Sandbox) -> Vec<Value> {
    fs::read_to_string(sb.fake_state().join("acp-prompts.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn on(engine: &str, model: &str, effort: &str) -> Value {
    json!({ "engine": engine, "model": model, "effort": effort })
}

fn opencode() -> Value {
    on("opencode", "opencode/fake-free", "default")
}

async fn login_state(c: &mut Client, from: usize, engine: &str, state: &str) -> Value {
    c.wait_after(from, &format!("{engine} login {state}"), |m| m["op"] == "login" && m["engine"] == engine && m["state"] == state)
        .await
}

#[tokio::test]
async fn opencode_lists_its_models_and_reports_its_providers() {
    let sb = Sandbox::new("oc-models");
    let d = sb.start();
    let mut c = d.client().await;
    let models = c
        .wait_for("opencode models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["opencode"].as_array().is_some_and(|l| !l.is_empty()))
        .await["models"]["opencode"]
        .clone();
    assert_eq!(models, json!([{ "id": "opencode/fake-free", "name": "Fake Free (opencode)", "efforts": [] }]));

    c.send(json!({ "op": "engines.status" })).await;
    let st = c.wait_for("engine status", |m| m["op"] == "engines").await;
    assert_eq!(st["engines"]["opencode"], json!({ "installed": true, "loggedIn": true, "providers": ["opencode"] }), "{st}");
    // The others are still there.
    assert!(st["engines"]["codex"].is_object() && st["engines"]["claude"].is_object(), "{st}");
}

#[tokio::test]
async fn an_opencode_turn_streams_then_completes_and_follows_up() {
    let sb = Sandbox::new("oc-plain");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "say hello", opencode()).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    c.wait_status(&sid, "idle").await;
    let events = c.events(&sid);
    assert_eq!(
        types(&events),
        ["user_message", "turn_started", "assistant_delta", "assistant_delta", "assistant_delta", "assistant_message", "turn_completed"]
    );
    assert_eq!(events[5]["text"], "Echo: say hello");
    assert_eq!(events[2]["itemId"], events[5]["itemId"]);

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "again" })).await;
    c.wait_turn_done(&sid, next).await;
    let log = acp_log(&sb);
    assert_eq!(log.matches("opencode req session/new").count(), 1, "{log}");
    assert_eq!(log.matches("opencode start").count(), 1, "one agent for both turns: {log}");
    let p = prompts(&sb);
    assert_eq!(p.len(), 2);
    assert_eq!(p[0]["sessionId"], p[1]["sessionId"]);
    assert_eq!(p[1]["prompt"][0]["text"], "again");
}

#[tokio::test]
async fn opencode_thinking_streams_as_reasoning() {
    let sb = Sandbox::new("oc-think");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[think] hmm", opencode()).await;
    c.wait_turn_done(&sid, 0).await;
    let events = c.events(&sid);
    let reasoning = events.iter().find(|e| e["type"] == "reasoning").expect("a final thought");
    assert_eq!(reasoning["text"], "Weighing it. Decided.");
    assert_eq!(events.iter().filter(|e| e["type"] == "reasoning_delta").count(), 2);
    let pos = |ty: &str| events.iter().position(|e| e["type"] == ty).unwrap();
    assert!(pos("reasoning") < pos("assistant_message"), "the thought ends before the answer");
}

#[tokio::test]
async fn opencode_asks_before_a_command_and_takes_the_answer() {
    let sb = Sandbox::new("oc-approve");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] run it", opencode()).await;
    let started = c.wait_event(&sid, "tool_started", 0).await;
    assert_eq!((started["kind"].as_str(), started["title"].as_str(), started["detail"].as_str()), (Some("command"), Some("Ran"), Some("echo hi")));
    let req = c.wait_event(&sid, "approval_request", 0).await;
    assert_eq!(req["kind"], "command");
    assert_eq!(req["title"], "echo hi");
    // Names the call it asks about, so the thread can say that call is waiting.
    assert_eq!(req["itemId"], started["itemId"]);
    c.wait_status(&sid, "needs_approval").await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" })).await;
    let done = c.wait_event(&sid, "tool_completed", 0).await;
    assert_eq!((done["ok"].as_bool(), done["output"].as_str()), (Some(true), Some("hi")));
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert!(acp_log(&sb).contains(r#"answer {"outcome":{"optionId":"once","outcome":"selected"}}"#), "{}", acp_log(&sb));

    // Denied: the agent hears "reject".
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[cmd] again" })).await;
    let req = c.wait_event(&sid, "approval_request", next).await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "deny" })).await;
    let done = c.wait_event(&sid, "tool_completed", next).await;
    assert_eq!(done["ok"], false);
    c.wait_turn_done(&sid, next).await;
    assert!(acp_log(&sb).contains(r#""optionId":"reject""#), "{}", acp_log(&sb));
}

#[tokio::test]
async fn opencode_edits_show_their_diff_and_follow_the_permission_mode() {
    let sb = Sandbox::new("oc-modes");
    let d = sb.start();
    let mut c = d.client().await;
    let cwd = sb.folder("proj");
    fs::write(format!("{cwd}/notes.txt"), "old line\n").unwrap();

    // Accept edits: allowed without asking, and the diff is shown.
    let sid = c.create(&cwd, "[edit] change it", json!({ "engine": "opencode", "model": "opencode/fake-free", "effort": "default", "permission": "acceptEdits" })).await;
    let change = c.wait_event(&sid, "file_change", 0).await;
    assert_eq!(change["files"][0]["path"], "notes.txt");
    assert_eq!(change["files"][0]["diff"], "-old line\n+edited by the fake\n ");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert!(!c.event_types(&sid).contains(&"approval_request".to_string()), "{:?}", c.event_types(&sid));
    assert_eq!(c.events(&sid).iter().find(|e| e["type"] == "tool_completed").unwrap()["ok"], true);

    // Plan only: OpenCode's plan mode, and an edit is refused unasked.
    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "plan" })).await;
    // yardd handles each message in its own task: wait for the mode to land
    // before sending, or the send can overtake it.
    c.wait_for("the new permission mode", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["permission"] == "plan").await;
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[edit] change it again" })).await;
    c.wait_turn_done(&sid, next).await;
    let last = c.events(&sid);
    assert_eq!(last.iter().rev().find(|e| e["type"] == "tool_completed").unwrap()["ok"], false);
    assert_eq!(prompts(&sb)[1]["mode"], "plan");

    // Full access: commands run unasked.
    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "full" })).await;
    // yardd handles each message in its own task: wait for the mode to land
    // before sending, or the send can overtake it.
    c.wait_for("the new permission mode", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["permission"] == "full").await;
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[cmd] go" })).await;
    c.wait_turn_done(&sid, next).await;
    let types_now = c.event_types(&sid);
    assert!(!types_now.contains(&"approval_request".to_string()), "{types_now:?}");
    assert_eq!(prompts(&sb)[2]["mode"], "build", "back out of plan mode");
}

#[tokio::test]
async fn opencode_takes_the_model_and_effort_from_the_session() {
    let sb = Sandbox::new("oc-effort");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "hi", on("opencode", "opencode/fake-free", "high")).await;
    c.wait_turn_done(&sid, 0).await;
    assert_eq!(prompts(&sb)[0]["effort"], "high");
    // A model the agent does not know fails the turn with its reason.
    c.send(json!({ "op": "session.setModel", "sessionId": sid, "model": "nope/none", "effort": "high" })).await;
    c.wait_for("the unknown model", |m| m["op"] == "session" && m["session"]["id"] == sid && m["session"]["model"] == "nope/none").await;
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "hi again" })).await;
    let err = c.wait_event(&sid, "error", next).await;
    assert!(err["message"].as_str().unwrap().contains("unknown model nope/none"), "{err}");
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "failed");
}

#[tokio::test]
async fn messages_sent_during_an_opencode_turn_are_the_next_turn() {
    let sb = Sandbox::new("oc-queue");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] first", opencode()).await;
    let req = c.wait_event(&sid, "approval_request", 0).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "after that" })).await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" })).await;
    c.wait_turn_done(&sid, 0).await;
    // OpenCode cannot take input mid-turn: the message waits for the turn to end.
    let next = c.wait_for("the queued message as a turn", |m| is_event(m, &sid) && m["event"]["type"] == "user_message" && m["event"]["text"] == "after that").await;
    assert!(next["event"]["steered"].is_null(), "{next}");
    c.wait_turn_done(&sid, next["seq"].as_u64().unwrap()).await;
    assert_eq!(prompts(&sb)[1]["prompt"][0]["text"], "after that");
}

#[tokio::test]
async fn stop_ends_an_opencode_turn_even_when_it_does_not_listen() {
    let sb = Sandbox::new("oc-stop");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[slow] long", opencode()).await;
    c.wait_event(&sid, "assistant_delta", 0).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "interrupted");
    assert!(acp_log(&sb).contains("opencode req session/cancel"));

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[stuck] forever" })).await;
    c.wait_event(&sid, "assistant_delta", next).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    let note = c.wait_event(&sid, "note", next).await;
    assert_eq!(note["text"], "OpenCode did not stop the turn, so it was ended here.");
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "interrupted");
    c.drain_for(Duration::from_millis(300)).await;
    // A later answer from the agent cannot end the turn twice.
    assert_eq!(c.events(&sid).iter().filter(|e| e["type"] == "turn_completed").count(), 2);
}

#[tokio::test]
async fn text_of_a_turn_ended_after_stop_does_not_run_into_the_next_answer() {
    let sb = Sandbox::new("oc-stuck-text");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[stuck] forever", opencode()).await;
    c.wait_event(&sid, "assistant_delta", 0).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.wait_event(&sid, "note", 0).await;
    c.wait_turn_done(&sid, 0).await;
    // The ended turn's text is closed as its own message.
    let first = c.events(&sid).into_iter().find(|e| e["type"] == "assistant_message").expect("the stuck turn's text, closed");
    assert_eq!(first["text"], "Working");

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "fresh" })).await;
    c.wait_turn_done(&sid, next).await;
    let replies: Vec<_> = c.events(&sid).into_iter().filter(|e| e["type"] == "assistant_message").collect();
    let last = replies.last().unwrap();
    assert_eq!(last["text"], "Echo: fresh", "{replies:?}");
    assert_ne!(last["itemId"], first["itemId"]);
}

#[tokio::test]
async fn output_of_a_turn_ended_after_stop_that_arrives_later_is_dropped() {
    let sb = Sandbox::new("oc-late");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[late] forever", opencode()).await;
    c.wait_event(&sid, "assistant_delta", 0).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.wait_turn_done(&sid, 0).await;
    // The fake streams a thought and "Late words" 6 s after the cancel, 1 s after the turn was ended here.
    c.drain_for(Duration::from_secs(3)).await;
    assert!(acp_log(&sb).contains("opencode req session/cancel"));
    let late: Vec<_> = c.events(&sid).into_iter().filter(|e| e.to_string().contains("Late")).collect();
    assert!(late.is_empty(), "no turn owns these: {late:?}");

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "fresh" })).await;
    c.wait_turn_done(&sid, next).await;
    let replies: Vec<_> = c.events(&sid).into_iter().filter(|e| e["type"] == "assistant_message").collect();
    assert_eq!(replies.last().unwrap()["text"], "Echo: fresh", "{replies:?}");
}

#[tokio::test]
async fn an_opencode_agent_that_dies_fails_the_turn_and_the_next_one_resumes() {
    let sb = Sandbox::new("oc-exit");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[exit] crash", opencode()).await;
    let err = c.wait_event(&sid, "error", 0).await;
    assert!(err["message"].as_str().unwrap().contains("OpenCode stopped before answering session/prompt"), "{err}");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "failed");

    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "back?" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let log = acp_log(&sb);
    assert_eq!(log.matches("opencode start").count(), 2, "{log}");
    assert!(log.contains("opencode req session/resume"), "{log}");
    let p = prompts(&sb);
    assert_eq!(p[0]["sessionId"], p[1]["sessionId"], "the same conversation");
}

#[tokio::test]
async fn an_opencode_session_resumes_after_a_restart() {
    let sb = Sandbox::new("oc-restart");
    let cwd = sb.folder("proj");
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&cwd, "remember me", opencode()).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    d.stop();

    let d = sb.start();
    let mut c = d.client().await;
    let next = c.history(&sid).await.len() as u64;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "still there?" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let p = prompts(&sb);
    assert_eq!(p[0]["sessionId"], p[1]["sessionId"]);
    assert!(acp_log(&sb).contains("opencode req session/resume"), "{}", acp_log(&sb));
}

#[tokio::test]
async fn opencode_providers_log_in_from_the_app() {
    let sb = Sandbox::new("oc-login");
    let d = sb.start();
    let mut c = d.client().await;

    // An API key: pick the provider, paste the key, its models appear.
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "opencode" })).await;
    let offered = login_state(&mut c, mark, "opencode", "providers").await;
    let ids: Vec<&str> = offered["providers"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["opencode", "fakeprov", "fakeauth"], "connected first, then by name");
    c.send(json!({ "op": "login.choose", "engine": "opencode", "provider": "fakeprov", "method": 0 })).await;
    let key = login_state(&mut c, mark, "opencode", "key").await;
    assert_eq!(key["provider"], "fakeprov");
    c.send(json!({ "op": "login.code", "engine": "opencode", "code": "sk-fake-key" })).await;
    login_state(&mut c, mark, "opencode", "done").await;
    c.wait_after(mark, "fakeprov models", |m| m["op"] == "models" && m["models"]["opencode"].as_array().is_some_and(|l| l.len() == 2)).await;
    let st = c.wait_after(mark, "fakeprov connected", |m| m["op"] == "engines" && m["engines"]["opencode"]["providers"].as_array().is_some_and(|p| p.len() == 2)).await;
    assert_eq!(st["engines"]["opencode"]["providers"], json!(["opencode", "fakeprov"]));
    // yardd never keeps the key.
    assert!(!fs::read_dir(sb.data()).unwrap().flatten().any(|f| fs::read(f.path()).is_ok_and(|b| String::from_utf8_lossy(&b).contains("sk-fake-key"))));
    assert!(!d.log_text().contains("sk-fake-key"));

    // A sign-in that gives back a code: a wrong one fails, the right one logs in.
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "opencode" })).await;
    login_state(&mut c, mark, "opencode", "providers").await;
    c.send(json!({ "op": "login.choose", "engine": "opencode", "provider": "fakeauth", "method": 0 })).await;
    let url = login_state(&mut c, mark, "opencode", "url").await;
    assert_eq!(url["url"], "https://fake.example/oauth?state=x");
    c.send(json!({ "op": "login.code", "engine": "opencode", "code": "wrong" })).await;
    let failed = login_state(&mut c, mark, "opencode", "failed").await;
    assert!(failed["error"].as_str().unwrap().contains("not accepted"), "{failed}");

    // A device code: shown, and the server finishes by itself.
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "opencode" })).await;
    login_state(&mut c, mark, "opencode", "providers").await;
    c.send(json!({ "op": "login.choose", "engine": "opencode", "provider": "fakeauth", "method": 1 })).await;
    let code = login_state(&mut c, mark, "opencode", "code").await;
    assert_eq!((code["url"].as_str(), code["code"].as_str()), (Some("https://fake.example/device"), Some("FAKE-4321")));
    login_state(&mut c, mark, "opencode", "done").await;
}

#[tokio::test]
async fn an_added_opencode_account_has_its_own_logins_and_sessions() {
    let sb = Sandbox::new("oc-account");
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "account.add", "engine": "opencode", "name": "Work" })).await;
    let accounts = c.wait_for("the new account", |m| m["op"] == "accounts" && m["accounts"].as_array().is_some_and(|l| !l.is_empty())).await;
    let id = accounts["accounts"][0]["id"].as_str().unwrap().to_string();

    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "opencode", "account": id })).await;
    login_state(&mut c, mark, "opencode", "providers").await;
    c.send(json!({ "op": "login.choose", "engine": "opencode", "account": id, "provider": "fakeprov", "method": 0 })).await;
    login_state(&mut c, mark, "opencode", "key").await;
    c.send(json!({ "op": "login.code", "engine": "opencode", "account": id, "code": "sk-work" })).await;
    login_state(&mut c, mark, "opencode", "done").await;
    let st = c.wait_after(mark, "the account's providers", |m| m["op"] == "engines" && m["accounts"][&id]["providers"].as_array().is_some_and(|p| p.len() == 2)).await;
    // Default did not get the Work login.
    assert_eq!(st["engines"]["opencode"]["providers"], json!(["opencode"]));
    assert!(sb.data().join("accounts").join(&id).join("opencode/connected.json").exists());

    let sid = c.create(&sb.folder("proj"), "hi", json!({ "engine": "opencode", "model": "opencode/fake-free", "effort": "default", "account": id })).await;
    c.wait_turn_done(&sid, 0).await;
    assert_eq!(prompts(&sb)[0]["account"], json!(id));
}

fn grok() -> Value {
    on("grok", "fake-small", "high")
}

async fn grok_login(c: &mut Client, account: Option<&str>) {
    let mark = c.mark();
    let mut msg = json!({ "op": "login.start", "engine": "grok" });
    if let Some(a) = account {
        msg["account"] = json!(a);
    }
    c.send(msg).await;
    let code = login_state(c, mark, "grok", "code").await;
    assert_eq!(code["code"], "GROK-1234");
    assert_eq!(code["url"], "https://accounts.x.ai/oauth2/device?user_code=GROK-1234");
    login_state(c, mark, "grok", "done").await;
}

#[tokio::test]
async fn grok_lists_models_logs_in_with_a_device_code_and_runs_a_turn() {
    let sb = Sandbox::new("grok-login");
    let d = sb.start();
    let mut c = d.client().await;
    let models = c
        .wait_for("grok models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["grok"].as_array().is_some_and(|l| !l.is_empty()))
        .await["models"]["grok"]
        .clone();
    assert_eq!(
        models,
        json!([
            { "id": "fake-small", "name": "Fake Small", "efforts": ["low", "high"], "defaultEffort": "high", "isDefault": true },
            { "id": "fake-large", "name": "Fake Large", "efforts": [], "isDefault": false },
        ])
    );
    c.send(json!({ "op": "engines.status" })).await;
    let st = c.wait_for("engine status", |m| m["op"] == "engines").await;
    assert_eq!(st["engines"]["grok"], json!({ "installed": true, "loggedIn": false }), "{st}");

    // Logged out: the session says why.
    let cwd = sb.folder("proj");
    // The agent refuses the session, so no turn starts.
    let sid = c.try_create(&cwd, "hello", grok()).await.unwrap();
    let err = c.wait_event(&sid, "error", 0).await;
    assert!(err["message"].as_str().unwrap().contains("Authentication required"), "{err}");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "failed");

    let mark = c.mark();
    grok_login(&mut c, None).await;
    let st = c.wait_after(mark, "grok logged in", |m| m["op"] == "engines" && m["engines"]["grok"]["loggedIn"] == true).await;
    assert_eq!(st["engines"]["grok"]["account"], "default@grok.example");

    // The same session now works: it never reached the agent, so it starts there now.
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "hello again" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    let p = prompts(&sb);
    assert_eq!((p[0]["engine"].as_str(), p[0]["effort"].as_str(), p[0]["mode"].as_str()), (Some("grok"), Some("high"), Some("default")));
    assert!(acp_log(&sb).contains(r#"grok req session/set_config_option reasoning_effort="high""#), "{}", acp_log(&sb));
}

#[tokio::test]
async fn a_failed_grok_login_says_why() {
    let sb = Sandbox::new("grok-login-fail");
    let d = sb.start_with(&[("FAKE_GROK_LOGIN_FAIL", "1")]);
    let mut c = d.client().await;
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "grok" })).await;
    login_state(&mut c, mark, "grok", "code").await;
    let failed = login_state(&mut c, mark, "grok", "failed").await;
    assert_eq!(failed["error"], "Error: the code expired");
}

#[tokio::test]
async fn grok_follows_the_permission_mode_with_its_own_modes() {
    let sb = Sandbox::new("grok-modes");
    let d = sb.start();
    let mut c = d.client().await;
    grok_login(&mut c, None).await;
    let cwd = sb.folder("proj");
    let sid = c.create(&cwd, "[edit] change it", json!({ "engine": "grok", "model": "fake-small", "effort": "high", "permission": "acceptEdits" })).await;
    c.wait_turn_done(&sid, 0).await;
    assert!(!c.event_types(&sid).contains(&"approval_request".to_string()));
    assert_eq!(prompts(&sb)[0]["mode"], "acceptEdits");

    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "plan" })).await;
    c.send(json!({ "op": "session.setModel", "sessionId": sid, "model": "fake-large", "effort": "high" })).await;
    // Each message is handled in its own task; let both changes land first.
    c.wait_for("plan mode on the larger model", |m| {
        m["op"] == "session" && m["session"]["id"] == sid && m["session"]["permission"] == "plan" && m["session"]["model"] == "fake-large"
    })
    .await;
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[cmd] look" })).await;
    let req = c.wait_event(&sid, "approval_request", next).await;
    assert_eq!(req["title"], "echo hi", "Plan only still asks before commands");
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" })).await;
    c.wait_turn_done(&sid, next).await;
    let p = prompts(&sb);
    assert_eq!((p[1]["mode"].as_str(), p[1]["model"].as_str()), (Some("plan"), Some("fake-large")));
    assert!(acp_log(&sb).contains("grok req session/set_mode plan"), "{}", acp_log(&sb));
}

#[tokio::test]
async fn an_added_grok_account_logs_in_on_its_own() {
    let sb = Sandbox::new("grok-account");
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "account.add", "engine": "grok", "name": "Work" })).await;
    let accounts = c.wait_for("the new account", |m| m["op"] == "accounts" && m["accounts"].as_array().is_some_and(|l| !l.is_empty())).await;
    let id = accounts["accounts"][0]["id"].as_str().unwrap().to_string();
    let mark = c.mark();
    grok_login(&mut c, Some(&id)).await;
    let st = c.wait_after(mark, "the account logged in", |m| m["op"] == "engines" && m["accounts"][&id]["loggedIn"] == true).await;
    assert_eq!(st["engines"]["grok"]["loggedIn"], false, "Default is still logged out");
    assert_eq!(st["accounts"][&id]["account"], json!(format!("{id}@grok.example")));

    let sid = c.create(&sb.folder("proj"), "hi", json!({ "engine": "grok", "model": "fake-small", "effort": "low", "account": id })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert_eq!(prompts(&sb)[0]["account"], json!(id));
}


#[tokio::test]
async fn gemini_discovers_models_streams_and_switches_models_without_config_options() {
    let sb = Sandbox::new("gemini-models");
    fs::write(sb.fake_state().join("gemini-logged-in"), "").unwrap();
    let d = sb.start();
    let mut c = d.client().await;
    let models = c.wait_for("Gemini models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["gemini"].as_array().is_some_and(|l| l.len() == 2)).await;
    assert_eq!(models["models"]["gemini"][0]["id"], "fake-small");
    assert_eq!(models["models"]["gemini"][0]["efforts"], json!([]));
    let sid = c.create(&sb.folder("proj"), "hello Gemini", on("gemini", "fake-large", "")).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert!(c.events(&sid).iter().any(|e| e["type"] == "assistant_message" && e["text"] == "Echo: hello Gemini"));
    assert_eq!(prompts(&sb)[0]["model"], "fake-large");
    assert!(acp_log(&sb).contains("gemini req session/set_model"));
    assert!(!acp_log(&sb).contains("gemini req session/set_config_option"));
}

#[tokio::test]
async fn gemini_permissions_stop_and_reload_use_its_native_protocol() {
    let sb = Sandbox::new("gemini-permissions");
    fs::write(sb.fake_state().join("gemini-logged-in"), "").unwrap();
    let d = sb.start();
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] ask", on("gemini", "fake-small", "")).await;
    let req = c.wait_event(&sid, "approval_request", 0).await;
    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" })).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "full" })).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[edit] full" })).await;
    c.wait_turn_done(&sid, next).await;
    assert_eq!(prompts(&sb)[1]["mode"], "yolo");
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "plan" })).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[edit] plan" })).await;
    c.wait_turn_done(&sid, next).await;
    assert_eq!(prompts(&sb)[2]["mode"], "default", "without native plan support, leave yolo and enforce refusals in yardd");
    assert!(c.events(&sid).iter().any(|e| e["type"] == "assistant_message" && e["text"] == "Left it alone."));
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[slow] wait" })).await;
    c.wait_event(&sid, "assistant_delta", next).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "interrupted");
    c.wait_status(&sid, "idle").await;
    drop(c);
    d.stop();
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "after restart" })).await;
    c.wait_turn_done(&sid, 0).await;
    assert!(acp_log(&sb).contains("gemini req session/load"));
    assert!(!c.events(&sid).iter().any(|e| e["text"] == "old answer"));
}

#[tokio::test]
async fn gemini_logged_out_offers_terminal_login_and_added_accounts_stay_separate() {
    let sb = Sandbox::new("gemini-accounts");
    let d = sb.start();
    let mut c = d.client().await;
    c.send(json!({ "op": "engines.status" })).await;
    let status = c.wait_for("Gemini logged out", |m| m["op"] == "engines" && m["engines"]["gemini"]["installed"] == true).await;
    assert_eq!(status["engines"]["gemini"]["loggedIn"], false);
    c.send(json!({ "op": "login.start", "engine": "gemini" })).await;
    let login = login_state(&mut c, 0, "gemini", "terminal").await;
    assert_eq!(login["command"], "gemini");
    assert!(login["error"].is_null(), "terminal login guidance is an expected state, not a failed API call");
    assert_eq!(login["instructions"], "Authentication required");
    c.send(json!({ "op": "account.add", "engine": "gemini", "name": "Work" })).await;
    let accounts = c.wait_for("Gemini account", |m| m["op"] == "accounts").await;
    let id = accounts["accounts"][0]["id"].as_str().unwrap().to_string();
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "gemini", "account": id })).await;
    let login = login_state(&mut c, mark, "gemini", "terminal").await;
    assert!(login["command"].as_str().unwrap().contains("GEMINI_CLI_HOME="));
    let state = sb.data().join("accounts").join(&id).join(".gemini");
    fs::create_dir_all(&state).unwrap();
    fs::write(state.join("logged-in"), "").unwrap();
    let mark = c.mark();
    c.send(json!({ "op": "login.start", "engine": "gemini", "account": id })).await;
    login_state(&mut c, mark, "gemini", "done").await;
    let sid = c.create(&sb.folder("proj"), "work account", json!({ "engine": "gemini", "model": "fake-small", "effort": "", "account": id })).await;
    c.wait_turn_done(&sid, 0).await;
    assert_eq!(prompts(&sb)[0]["account"], id);
    let mark = c.mark();
    c.send(json!({ "op": "engines.status" })).await;
    let status = c.wait_after(mark, "Default still logged out", |m| m["op"] == "engines").await;
    assert_eq!(status["engines"]["gemini"]["loggedIn"], false);
}


#[tokio::test]
async fn gemini_accepts_edits_and_hands_off_with_the_conversation() {
    let sb = Sandbox::new("gemini-handoff");
    fs::write(sb.fake_state().join("gemini-logged-in"), "").unwrap();
    let d = sb.start();
    let mut c = d.client().await;
    let source = c.create(&sb.folder("proj"), "original task", opencode()).await;
    c.wait_turn_done(&source, 0).await;
    c.wait_status(&source, "idle").await;
    let mark = c.mark();
    c.send(json!({ "op": "session.handoff", "sessionId": source, "engine": "gemini", "model": "fake-small", "effort": "" })).await;
    let created = c.wait_after(mark, "handed off", |m| m["op"] == "created").await;
    let sid = created["sessionId"].as_str().unwrap();
    c.wait_turn_done(sid, 0).await;
    assert_eq!(prompts(&sb)[1]["engine"], "gemini");
    assert!(prompts(&sb)[1]["prompt"][0]["text"].as_str().unwrap().contains("original task"));
    c.wait_status(sid, "idle").await;
    let next = c.next_seq(sid);
    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "acceptEdits" })).await;
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "[edit] allowed" })).await;
    c.wait_turn_done(sid, next).await;
    assert_eq!(prompts(&sb)[2]["mode"], "auto_edit");
    assert!(!c.events(sid).iter().any(|e| e["type"] == "approval_request"));
}

#[tokio::test]
async fn grok_reports_its_credit_allowance_and_when_it_resets() {
    // Grok's agent answers `_x.ai/billing` (what its /usage shows) with no
    // turn; yardd keeps it as Grok's usage, one window with its reset time.
    let sb = Sandbox::new("grok-usage");
    let d = sb.start();
    let mut c = d.client().await;
    let from = c.mark();
    c.send(json!({ "op": "usage.refresh", "engine": "grok", "ref": "out" })).await;
    let reply = c.wait_after(from, "refresh while logged out", |m| m["ref"] == "out").await;
    assert_eq!(reply["op"], "error", "{reply}");
    assert!(reply["message"].as_str().unwrap_or("").contains("sign in"), "{reply}");

    grok_login(&mut c, None).await;
    c.wait_for("grok logged in", |m| m["op"] == "engines" && m["engines"]["grok"]["loggedIn"] == true).await;
    let from = c.mark();
    c.send(json!({ "op": "usage.refresh", "engine": "grok", "ref": "in" })).await;
    let reply = c.wait_after(from, "refresh", |m| m["ref"] == "in").await;
    assert_eq!(reply["op"], "usage.refreshed", "{reply}");
    let usage = c.wait_after(from, "grok usage", |m| m["op"] == "usage" && m["usage"]["grok"].is_object()).await;
    let week = &usage["usage"]["grok"]["windows"]["week"];
    assert_eq!(week["utilization"], 0.23, "{usage}");
    assert_eq!(week["label"], "Week", "{usage}");
    let left = week["resetsAt"].as_f64().unwrap() - std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
    assert!((5.0 * 86400.0..5.2 * 86400.0).contains(&left), "{usage}");
}
