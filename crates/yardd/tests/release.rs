//! Idle engines let go of their sessions (see yardd's release.rs), an
//! app-server that dies is replaced, and agents' and automations' sessions get
//! only their connectors (scope.rs). All on the scripted engines.
mod common;
use std::{fs, time::Duration};

use common::*;
use serde_json::{json, Value};

/// Wait until `cond` holds, or fail saying `what`.
async fn until(what: &str, secs: u64, mut cond: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    while !cond() {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

fn lines(sb: &Sandbox, file: &str) -> Vec<Value> {
    fs::read_to_string(sb.fake_state().join(file))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The `loaded=<n>` the fake Codex logged last.
fn loaded(sb: &Sandbox) -> Option<usize> {
    sb.codex_log().lines().rev().find_map(|l| l.strip_prefix("loaded=")).and_then(|n| n.parse().ok())
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid]).status().is_ok_and(|s| s.success())
}

fn sidecar_starts(sb: &Sandbox) -> Vec<String> {
    fs::read_to_string(sb.fake_state().join("sidecar.log"))
        .unwrap_or_default()
        .lines()
        .filter(|l| l.starts_with("start pid="))
        .map(str::to_string)
        .collect()
}

/// The scripted Claude sidecar, which logs each start to sidecar.log.
fn sidecar() -> String {
    fake_bin().join("yard-claude").to_str().unwrap().to_string()
}

fn pid_of(start: &str) -> String {
    start["start pid=".len()..].split(' ').next().unwrap().to_string()
}

#[tokio::test]
async fn an_idle_codex_thread_is_released_and_resumed_by_the_next_turn() {
    let sb = Sandbox::new("release-codex");
    let d = sb.start_with(&[("YARD_RELEASE_IDLE_MS", "400")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "TEST-hi", json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    let thread = sb.codex_log().lines().find_map(|l| l.strip_prefix("req turn/start ").map(|r| r.split(' ').next().unwrap().to_string())).unwrap();
    assert_eq!(loaded(&sb), Some(1));

    until("the idle thread to be unsubscribed", 10, || sb.codex_log().contains(&format!("req thread/unsubscribe {thread}"))).await;
    until("the loaded list to be read", 5, || sb.codex_log().contains("req thread/loaded/list")).await;
    assert_eq!(loaded(&sb), Some(0), "{}", sb.codex_log());
    // Letting go says nothing in the conversation.
    c.drain_for(Duration::from_millis(300)).await;
    let before = c.event_types(&sid);
    assert_eq!(before.last().map(String::as_str), Some("turn_completed"), "{before:?}");

    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-again" })).await;
    let done = c.wait_turn_done(&sid, seq).await;
    assert_eq!(done["status"], "completed", "{:?}", c.events(&sid));
    let log = sb.codex_log();
    let unsub = log.find(&format!("req thread/unsubscribe {thread}")).unwrap();
    let resume = log.find(&format!("req thread/resume {thread}")).expect("the next turn resumes the thread");
    assert!(unsub < resume, "{log}");
    assert!(c.events(&sid).iter().any(|e| e["type"] == "assistant_message" && e["text"] == "Echo: TEST-again"));
    assert!(!c.event_types(&sid).contains(&"error".to_string()), "{:?}", c.events(&sid));
}

#[tokio::test]
async fn a_session_waiting_on_an_approval_keeps_its_thread() {
    let sb = Sandbox::new("release-busy");
    let d = sb.start_with(&[("YARD_RELEASE_IDLE_MS", "300")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[cmd] TEST-run it", json!({})).await;
    let req = c.wait_event(&sid, "approval_request", 0).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(!sb.codex_log().contains("req thread/unsubscribe"), "released while waiting for approval:\n{}", sb.codex_log());

    c.send(json!({ "op": "session.approve", "sessionId": sid, "requestId": req["requestId"], "decision": "allow" })).await;
    let done = c.wait_turn_done(&sid, 0).await;
    assert_eq!(done["status"], "completed");
    until("the thread to be released once the turn ended", 10, || sb.codex_log().contains("req thread/unsubscribe")).await;
}

#[tokio::test]
async fn a_dead_app_server_is_replaced_and_its_sessions_resume() {
    let sb = Sandbox::new("codex-crash");
    let d = sb.start();
    let mut c = d.client().await;
    let calm = c.create(&sb.folder("calm"), "TEST-hi", json!({})).await;
    c.wait_turn_done(&calm, 0).await;
    let calm_thread = sb.codex_log().lines().find_map(|l| l.strip_prefix("req turn/start ").map(|r| r.split(' ').next().unwrap().to_string())).unwrap();

    let crash = c.create(&sb.folder("crash"), "[crash] TEST-boom", json!({})).await;
    let done = c.wait_turn_done(&crash, 0).await;
    assert_eq!(done["status"], "failed");
    let errors: Vec<Value> = c.events(&crash).into_iter().filter(|e| e["type"] == "error").collect();
    assert!(errors.iter().any(|e| e["message"].as_str().unwrap().contains("app-server exited")), "{errors:?}");

    // Both sessions go on, on a new app-server that resumes their threads.
    for (sid, thread) in [(&calm, Some(calm_thread)), (&crash, None)] {
        let seq = c.next_seq(sid);
        c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-after" })).await;
        let done = c.wait_turn_done(sid, seq).await;
        assert_eq!(done["status"], "completed", "{:?}", c.events(sid));
        if let Some(thread) = thread {
            assert!(sb.codex_log().contains(&format!("req thread/resume {thread}")), "{}", sb.codex_log());
        }
    }
    let starts = sb.codex_log().lines().filter(|l| l.starts_with("start pid=")).count();
    assert_eq!(starts, 2, "one new app-server, not one per request:\n{}", sb.codex_log());
}

/// The servers a Codex thread was started or resumed with: name -> enabled.
fn mcp_overrides(params: &Value) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = params["config"]
        .as_object()
        .map(|c| {
            c.iter()
                .filter_map(|(k, v)| k.strip_prefix("mcp_servers.").filter(|n| *n != "switchyard").map(|n| (n.to_string(), v["enabled"].as_bool().unwrap())))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

const CONFIG: &str = r#"{"mcp_servers":{
  "metabase":{"url":"https://metabase.example/mcp","tool_timeout_sec":null},
  "handoff":{"command":"handoff-mcp","args":["--stdio"]},
  "off":{"command":"off-mcp","enabled":false}
}}"#;

fn on(list: &[(&str, bool)]) -> Vec<(String, bool)> {
    list.iter().map(|(n, e)| (n.to_string(), *e)).collect()
}

async fn save_automation(c: &mut Client, a: Value) -> Value {
    let from = c.mark();
    c.send(json!({ "op": "automation.save", "automation": a, "ref": "save" })).await;
    let reply = c.wait_after(from, "automation.saved", |m| m["ref"] == "save" && (m["op"] == "automation.saved" || m["op"] == "error")).await;
    assert_eq!(reply["op"], "automation.saved", "{reply}");
    reply["automation"].clone()
}

async fn run_automation(c: &mut Client, id: &str) -> String {
    let from = c.mark();
    c.send(json!({ "op": "automation.run", "id": id, "force": true, "ref": "run" })).await;
    let ran = c.wait_after(from, "automation.ran", |m| m["ref"] == "run" && (m["op"] == "automation.ran" || m["op"] == "error")).await;
    ran["sessionId"].as_str().unwrap_or_else(|| panic!("{ran}")).to_string()
}

fn automation(cwd: &str, name: &str) -> Value {
    json!({
        "name": name, "prompt": "TEST-patrol", "cwd": cwd, "engine": "codex", "model": MODEL, "effort": EFFORT,
        "permission": "ask", "schedule": { "kind": "manual" },
    })
}

#[tokio::test]
async fn agents_and_automations_get_only_their_connectors_on_codex() {
    let sb = Sandbox::new("scope-codex");
    fs::write(sb.fake_state().join("config-read.json"), CONFIG).unwrap();
    let cwd = sb.folder("proj");
    // An automation saved before connectors could be picked: no `mcp`.
    let mut old = automation(&cwd, "Old");
    old["id"] = json!("a-old");
    old["createdAt"] = json!(1);
    fs::write(sb.data().join("automations.json"), json!([old]).to_string()).unwrap();
    let d = sb.start_with(&[("YARD_RELEASE_IDLE_MS", "300")]);
    let mut c = d.client().await;

    // Not an agent's or an automation's: the account's servers, untouched.
    let plain = c.create(&cwd, "TEST-plain", json!({})).await;
    c.wait_turn_done(&plain, 0).await;
    assert_eq!(mcp_overrides(lines(&sb, "thread-starts.jsonl").last().unwrap()), on(&[]));

    // The old one: everything but Metabase; a server the owner turned off stays off.
    let run = run_automation(&mut c, "a-old").await;
    c.wait_turn_done(&run, 0).await;
    assert_eq!(mcp_overrides(lines(&sb, "thread-starts.jsonl").last().unwrap()), on(&[("handoff", true), ("metabase", false), ("off", false)]));
    // The override keeps the server's transport, without nulls Codex would reject.
    let starts = lines(&sb, "thread-starts.jsonl");
    let metabase = &starts.last().unwrap()["config"]["mcp_servers.metabase"];
    assert_eq!(metabase, &json!({ "url": "https://metabase.example/mcp", "enabled": false }));

    // A new automation has none until the owner picks some.
    let fresh = save_automation(&mut c, automation(&cwd, "Fresh")).await;
    assert_eq!(fresh["mcp"], json!([]));
    let run = run_automation(&mut c, fresh["id"].as_str().unwrap()).await;
    c.wait_turn_done(&run, 0).await;
    assert_eq!(mcp_overrides(lines(&sb, "thread-starts.jsonl").last().unwrap()), on(&[("handoff", false), ("metabase", false), ("off", false)]));
    // An automation's run lets go of its thread as soon as it ends.
    until("the run's thread to be released", 10, || sb.codex_log().matches("req thread/unsubscribe").count() >= 2).await;

    // An agent with Metabase picked gets it, and only it, after a release too.
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "ref": "ag", "agent": { "name": "TEST-Fire", "engine": "codex", "model": MODEL, "effort": EFFORT, "mcp": ["metabase", " metabase "] } })).await;
    let saved = c.wait_after(from, "agent.saved", |m| m["ref"] == "ag" && (m["op"] == "agent.saved" || m["op"] == "error")).await;
    assert_eq!(saved["agent"]["mcp"], json!(["metabase"]), "{saved}");
    let agent = saved["agent"]["id"].as_str().unwrap().to_string();
    let from = c.mark();
    c.send(json!({ "op": "agent.ask", "agentId": agent, "text": "TEST-look", "ref": "ask1" })).await;
    let created = c.wait_after(from, "created", |m| m["ref"] == "ask1" && m["op"] == "created").await;
    let chat = created["sessionId"].as_str().unwrap().to_string();
    c.wait_turn_done(&chat, 0).await;
    assert_eq!(mcp_overrides(lines(&sb, "thread-starts.jsonl").last().unwrap()), on(&[("handoff", false), ("metabase", true), ("off", false)]));
    let thread = sb.codex_log().lines().rev().find_map(|l| l.strip_prefix("req turn/start ").map(|r| r.split(' ').next().unwrap().to_string())).unwrap();
    until("the agent's thread to be released", 10, || sb.codex_log().contains(&format!("req thread/unsubscribe {thread}"))).await;
    let seq = c.next_seq(&chat);
    c.send(json!({ "op": "agent.ask", "agentId": agent, "text": "TEST-again", "ref": "ask2" })).await;
    c.wait_turn_done(&chat, seq).await;
    assert_eq!(mcp_overrides(lines(&sb, "thread-resumes.jsonl").last().unwrap()), on(&[("handoff", false), ("metabase", true), ("off", false)]));
}

#[tokio::test]
async fn an_idle_claude_sidecar_is_stopped_and_the_next_message_resumes() {
    let sb = Sandbox::new("release-claude");
    let d = sb.start_with(&[("YARD_RELEASE_IDLE_MS", "400"), ("YARD_SIDECAR", &sidecar())]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "TEST-hi", json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" })).await;
    c.wait_turn_done(&sid, 0).await;
    let first = sidecar_starts(&sb);
    assert_eq!(first.len(), 1, "{first:?}");
    let pid = pid_of(&first[0]);
    until("the idle sidecar to exit", 10, || !alive(&pid)).await;
    // Stopping an idle sidecar says nothing in the conversation.
    c.drain_for(Duration::from_millis(500)).await;
    assert_eq!(c.event_types(&sid).last().map(String::as_str), Some("turn_completed"), "{:?}", c.events(&sid));
    assert!(!c.event_types(&sid).contains(&"error".to_string()), "{:?}", c.events(&sid));
    assert_eq!(c.statuses(&sid).last().map(String::as_str), Some("idle"));

    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-again" })).await;
    let done = c.wait_turn_done(&sid, seq).await;
    assert_eq!(done["status"], "completed", "{:?}", c.events(&sid));
    let starts = sidecar_starts(&sb);
    assert_eq!(starts.len(), 2, "{starts:?}");
    assert!(starts[1].contains(&format!("resume=fake-claude-{pid}")), "the new sidecar resumes the conversation: {starts:?}");
}

#[tokio::test]
async fn a_claude_session_with_subagents_at_work_keeps_its_sidecar() {
    let sb = Sandbox::new("release-claude-bg");
    let d = sb.start_with(&[("YARD_RELEASE_IDLE_MS", "300"), ("YARD_SIDECAR", &sidecar())]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[background-agents] TEST-survey", json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" })).await;
    c.wait_turn_done(&sid, 0).await;
    let pid = pid_of(&sidecar_starts(&sb)[0]);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(alive(&pid), "the sidecar was stopped while its subagents worked");
}

#[tokio::test]
async fn claude_sessions_carry_their_connector_scope() {
    let sb = Sandbox::new("scope-claude");
    let cwd = sb.folder("proj");
    let mut old = automation(&cwd, "Old");
    old["id"] = json!("a-old");
    old["engine"] = json!("claude");
    old["model"] = json!("claude-haiku-5-5");
    old["createdAt"] = json!(1);
    fs::write(sb.data().join("automations.json"), json!([old]).to_string()).unwrap();
    let d = sb.start_with(&[("YARD_SIDECAR", &sidecar())]);
    let mut c = d.client().await;

    let plain = c.create(&cwd, "TEST-plain", json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" })).await;
    c.wait_turn_done(&plain, 0).await;
    let run = run_automation(&mut c, "a-old").await;
    c.wait_turn_done(&run, 0).await;
    let mut picked = automation(&cwd, "Picked");
    picked["engine"] = json!("claude");
    picked["model"] = json!("claude-haiku-5-5");
    picked["mcp"] = json!(["handoff", "claude.ai Linear"]);
    let picked = save_automation(&mut c, picked).await;
    let run = run_automation(&mut c, picked["id"].as_str().unwrap()).await;
    c.wait_turn_done(&run, 0).await;

    let starts = sidecar_starts(&sb);
    assert_eq!(starts.len(), 3, "{starts:?}");
    assert!(!starts[0].contains("mcp="), "a plain session is not scoped: {}", starts[0]);
    assert!(starts[1].ends_with(" mcp=except:metabase,claude.ai Metabase"), "{}", starts[1]);
    assert!(starts[2].ends_with(" mcp=only:handoff,claude.ai Linear"), "{}", starts[2]);
}
