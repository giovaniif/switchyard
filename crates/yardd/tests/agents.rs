//! Agents: the registry, chats that carry an agent's preamble, and the
//! built-in host agent, which answers from bounded read-only host evidence on
//! a fresh tool-restricted engine thread per question.
mod common;
use common::*;
use serde_json::{json, Value};

async fn ready(c: &mut Client) {
    c.wait_for("models ready", |m| {
        (m["op"] == "models" || m["op"] == "snapshot") && m["models"]["codex"].as_array().is_some_and(|a| !a.is_empty())
    })
    .await;
}

/// `agent.ask`, answered with `created` (the session to open) or an error.
async fn ask(c: &mut Client, agent: &str, text: &str, extra: Value) -> Result<String, String> {
    let from = c.mark();
    let mut msg = json!({ "op": "agent.ask", "agentId": agent, "text": text, "ref": format!("ask-{from}") });
    if let (Some(m), Some(e)) = (msg.as_object_mut(), extra.as_object()) {
        m.extend(e.clone());
    }
    let r = msg["ref"].clone();
    c.send(msg).await;
    let reply = c.wait_after(from, "agent.ask reply", |m| m["ref"] == r && (m["op"] == "created" || m["op"] == "error")).await;
    match reply["op"].as_str() {
        Some("created") => Ok(reply["sessionId"].as_str().unwrap().to_string()),
        _ => Err(reply["message"].as_str().unwrap_or("").to_string()),
    }
}

async fn save(c: &mut Client, agent: Value) -> Result<Value, String> {
    let from = c.mark();
    let r = format!("save-{from}");
    c.send(json!({ "op": "agent.save", "agent": agent, "ref": r })).await;
    let reply = c.wait_after(from, "agent.save reply", |m| m["ref"] == r.as_str()).await;
    match reply["op"].as_str() {
        Some("agent.saved") => Ok(reply["agent"].clone()),
        _ => Err(reply["message"].as_str().unwrap_or("").to_string()),
    }
}

fn thread_starts(sb: &Sandbox) -> Vec<Value> {
    std::fs::read_to_string(sb.fake_state().join("thread-starts.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn agent_of(c: &Client, id: &str) -> Value {
    c.log
        .iter()
        .rev()
        .find_map(|m| (m["op"] == "agents" || m["op"] == "snapshot").then(|| m["agents"].as_array().and_then(|l| l.iter().find(|a| a["id"] == id).cloned())).flatten())
        .unwrap_or(Value::Null)
}

#[tokio::test]
async fn the_host_agent_answers_in_a_chat_from_evidence_with_a_fresh_restricted_thread() {
    let sb = Sandbox::new("agents-host");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    assert_eq!(c.log[0]["agents"][0]["id"], "scout");
    assert_eq!(c.log[0]["agents"][0]["builtin"], true);
    save(&mut c, json!({ "id": "scout", "model": MODEL, "scope": "local" })).await.unwrap();

    let sid = ask(&mut c, "scout", "What is running?", json!({})).await.unwrap();
    let done = c.wait_turn_done(&sid, 0).await;
    assert_eq!(done["status"], "completed");
    let events = c.events(&sid);
    let context = events.iter().find(|e| e["type"] == "tool_completed").expect("evidence step");
    assert!(context["output"].as_str().unwrap().contains("[local:1]"));
    assert!(events.iter().any(|e| e["type"] == "tool_started" && e["kind"] == "context"));
    assert!(events.iter().any(|e| e["type"] == "assistant_message" && e["text"].as_str().unwrap().contains("scripted host agent answer")));
    // The thread shows what the user asked, not the engine's input.
    assert_eq!(events[0]["text"], "What is running?");
    let session = c.wait_for("the chat's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["permission"] == "plan").await;
    assert_eq!(session["session"]["agent"], "scout");
    assert_eq!(session["session"]["permission"], "plan");
    c.wait_for("Scout's ongoing chat", |m| m["op"] == "agents" && m["agents"][0]["chat"] == sid.as_str()).await;

    // A follow-up goes to the same chat, on a new thread that sees the first exchange.
    let seq = c.next_seq(&sid);
    assert_eq!(ask(&mut c, "scout", "And what changed?", json!({})).await.unwrap(), sid);
    c.wait_turn_done(&sid, seq).await;
    let starts = thread_starts(&sb);
    assert_eq!(starts.len(), 2);
    for start in &starts {
        assert_eq!(start["sandbox"], "read-only");
        assert_eq!(start["approvalPolicy"], "never");
        assert_eq!(start["ephemeral"], true);
        assert_eq!(start["config"]["features.shell_tool"], false);
        assert_eq!(start["config"]["features.apps"], false);
    }
    let inputs = sb.turn_inputs();
    let first = inputs[0][0]["text"].as_str().unwrap();
    let second = inputs[1][0]["text"].as_str().unwrap();
    assert!(first.starts_with("You are Scout,") && first.contains("\nEvidence: ") && first.len() <= 32_000);
    assert!(second.contains("What is running?") && second.contains("scripted host agent answer"));
    assert!(sb.codex_log().contains("approval=never sandboxPolicy=readOnly"));

    // Its chats only read.
    let from = c.mark();
    c.send(json!({ "op": "session.setPermission", "sessionId": sid, "permission": "full" })).await;
    assert!(c.wait_error(from, "session.setPermission").await.contains("only reads"));

    // After a restart the chat is still Scout's and goes on.
    drop(c);
    daemon.stop();
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let restored = c.log[0]["sessions"].as_array().unwrap().iter().find(|s| s["id"] == sid.as_str()).cloned().unwrap();
    assert_eq!(restored["agent"], "scout");
    assert_eq!(c.log[0]["agents"][0]["chat"], sid.as_str());
    let seq = c.history(&sid).await.len() as u64;
    assert_eq!(ask(&mut c, "scout", "Still there?", json!({})).await.unwrap(), sid);
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "completed");
}

#[tokio::test]
async fn an_agents_first_turn_carries_its_preamble_and_every_message_goes_to_its_one_conversation() {
    let sb = Sandbox::new("agents-chat");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let folder = sb.folder("TEST-agent-home");
    for (bad, why) in [(json!({ "name": "two words", "engine": "codex" }), "one word"), (json!({ "name": "Scout", "engine": "codex" }), "already called")] {
        assert!(save(&mut c, bad).await.unwrap_err().contains(why));
    }
    let agent = save(
        &mut c,
        json!({ "name": "Reviewer", "engine": "codex", "model": MODEL, "effort": EFFORT, "color": "rose", "face": "wink", "about": "Reviews  pull\nrequests",
            "instructions": "Be terse.", "memory": "Main is trunk.", "cwd": folder, "permission": "acceptEdits" }),
    )
    .await
    .unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    assert_eq!(agent["about"], "Reviews pull requests");
    assert_eq!(agent["builtin"], false);
    assert_eq!(agent["face"], "wink");
    // A face yardd does not know falls back to the one the UI picks from the id.
    let unknown = save(&mut c, json!({ "id": id, "face": "moustache" })).await.unwrap();
    assert_eq!(unknown["face"], "");
    assert_eq!(unknown["color"], "rose");

    let sid = ask(&mut c, &id, "TEST-look at the diff", json!({})).await.unwrap();
    c.wait_turn_done(&sid, 0).await;
    let seq = c.next_seq(&sid);
    assert_eq!(ask(&mut c, &id, "TEST-and the tests", json!({})).await.unwrap(), sid);
    c.wait_turn_done(&sid, seq).await;
    let inputs = sb.turn_inputs();
    let first = inputs[0][0]["text"].as_str().unwrap();
    assert!(first.starts_with("You are Reviewer,"), "{first}");
    assert!(first.contains("<instructions>\nBe terse.") && first.contains("Main is trunk."));
    assert!(first.ends_with("The user's message:\nTEST-look at the diff"));
    assert_eq!(inputs[1][0]["text"], "TEST-and the tests");
    // The thread shows only what was typed; the fake echoes only that too.
    assert!(c.events(&sid).iter().any(|e| e["type"] == "assistant_message" && e["text"] == "Echo: TEST-look at the diff"));
    let meta = c.wait_for("the chat's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["permission"] == "acceptEdits").await["session"].clone();
    assert_eq!(meta["agent"], id.as_str());
    assert_eq!(meta["cwd"], folder.as_str());
    assert_eq!(meta["permission"], "acceptEdits");

    // One conversation: asking for a new one changes nothing.
    assert_eq!(ask(&mut c, &id, "TEST-still here?", json!({ "fresh": true })).await.unwrap(), sid);
    assert_eq!(agent_of(&c, &id)["chat"], sid.as_str());

    // An empty message, an unknown agent and deleting the built-in one are refused.
    assert!(ask(&mut c, &id, "  ", json!({})).await.unwrap_err().contains("empty"));
    assert!(ask(&mut c, "nobody", "hi", json!({})).await.unwrap_err().contains("no such agent"));
    let from = c.mark();
    c.send(json!({ "op": "agent.delete", "id": "scout" })).await;
    assert!(c.wait_error(from, "agent.delete").await.contains("built in"));

    // Deleting an agent deletes its conversation.
    c.wait_status(&sid, "idle").await;
    c.send(json!({ "op": "agent.delete", "id": id })).await;
    c.wait_for("the conversation deleted", |m| m["op"] == "session.deleted" && m["sessionId"] == sid.as_str()).await;
    let list = c.wait_for("agents without it", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().len() == 1).await;
    assert_eq!(list["agents"][0]["id"], "scout");
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(sb.data().join("agents.json")).unwrap()).unwrap();
    assert_eq!(saved.as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn switchys_old_conversation_is_left_alone_and_scout_starts_empty() {
    let sb = Sandbox::new("agents-no-migration");
    let old = json!({ "status": "idle", "engine": "codex", "model": MODEL, "scope": "local", "revision": 1,
        "turns": [{ "id": "switchy-1", "question": "TEST-what runs?", "answer": "nginx", "engine": "codex", "model": MODEL, "at": 1, "status": "completed", "sources": [], "coverage": [], "error": null }] });
    std::fs::write(sb.data().join("switchy.json"), old.to_string()).unwrap();
    let daemon = sb.start();
    let c = daemon.client().await;
    assert!(c.log[0]["sessions"].as_array().unwrap().is_empty());
    assert!(c.log[0]["agents"][0]["chat"].is_null());
    // Not read, renamed or deleted.
    assert_eq!(std::fs::read_to_string(sb.data().join("switchy.json")).unwrap(), old.to_string());
}

#[tokio::test]
async fn host_context_is_bounded_carries_provenance_and_answers_the_old_name() {
    let sb = Sandbox::new("agents-context");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    for op in ["context.collect", "switchy.context"] {
        c.send(json!({ "op": op, "query": "services", "ref": op })).await;
        let reply = c.wait_for("host context", |m| m["ref"] == op).await;
        assert_eq!(reply["op"], op);
        assert!(reply.to_string().len() < 24_000);
        assert!(reply["sources"].as_array().unwrap().iter().all(|s| s["at"].is_u64() && s["title"].is_string()));
        assert!(reply["hostName"].is_string());
    }
}

#[tokio::test]
async fn remote_evidence_is_scoped_and_offline_hosts_are_reported() {
    let remote_sb = Sandbox::new("agents-remote");
    std::fs::write(remote_sb.home().join(".bash_history"), "echo UNIQUE_REMOTE_WORK\nTOKEN=must-not-appear\n").unwrap();
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    c.send(json!({"op":"host.add", "name":"TEST-box", "kind":"direct", "url":remote.url()})).await;
    let reply = c.wait_for("remote connected", |m| m["op"] == "hosts" && m["hosts"][0]["status"] == "connected").await;
    let host = reply["hosts"][0]["id"].as_str().unwrap().to_string();
    save(&mut c, json!({ "id": "scout", "model": MODEL, "scope": host })).await.unwrap();
    let sid = ask(&mut c, "scout", "What did I do?", json!({})).await.unwrap();
    c.wait_turn_done(&sid, 0).await;
    let output = c.events(&sid).iter().find(|e| e["type"] == "tool_completed").unwrap()["output"].as_str().unwrap().to_string();
    assert!(output.contains(&format!("[{host}:")), "{output}");
    assert!(!output.contains("[local:"));
    assert!(output.contains("UNIQUE_REMOTE_WORK"));
    assert!(!output.contains("must-not-appear"));
    assert!(!sb_text(&hub_sb).contains("must-not-appear"));
    // A remote's agents are not the hub's.
    assert!(!c.log.iter().any(|m| m["op"] == "agents" && m["hostId"].is_string()));
    remote.stop();
    c.wait_for("remote disconnected", |m| m["op"] == "hosts" && m["hosts"][0]["status"] != "connected").await;
    let seq = c.next_seq(&sid);
    ask(&mut c, "scout", "Is it still running?", json!({})).await.unwrap();
    c.wait_turn_done(&sid, seq).await;
    let last = c.events(&sid).into_iter().filter(|e| e["type"] == "tool_completed").last().unwrap();
    assert!(last["output"].as_str().unwrap().contains("offline"));
}

/// What the hub sent the fake engine, to check no secret line got through.
fn sb_text(sb: &Sandbox) -> String {
    std::fs::read_to_string(sb.fake_state().join("turn-inputs.jsonl")).unwrap_or_default()
}

#[tokio::test]
async fn claude_runs_the_host_agent_in_a_restricted_sidecar_and_reports_its_failure() {
    let sb = Sandbox::new("agents-claude");
    let daemon = sb.start_with(&[("YARD_SIDECAR", fake_bin().join("yard-claude").to_str().unwrap())]);
    let mut c = daemon.client().await;
    save(&mut c, json!({ "id": "scout", "engine": "claude", "model": "claude-haiku-5-5", "scope": "local" })).await.unwrap();
    let sid = ask(&mut c, "scout", "What is running?", json!({})).await.unwrap();
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert!(c.events(&sid).iter().any(|e| e["type"] == "assistant_message" && e["text"].as_str().unwrap().contains("Scripted Claude host agent answer")));
    // A second question replaces the sidecar without failing the new turn.
    let seq = c.next_seq(&sid);
    ask(&mut c, "scout", "And now?", json!({})).await.unwrap();
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "completed");
    let log = std::fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap();
    assert_eq!(log.matches("restricted=true").count(), 2, "{log}");
    let seq = c.next_seq(&sid);
    ask(&mut c, "scout", "[die]", json!({})).await.unwrap();
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "failed");
}

#[tokio::test]
async fn codex_nullable_config_does_not_prevent_host_answers() {
    let sb = Sandbox::new("agents-null-config");
    let config = json!({"mcp_servers": {"linear": {
        "url": "https://example.invalid/mcp", "enabled": true,
        "tool_timeout_sec": null, "startup_timeout_sec": null,
        "bearer_token_env_var": null, "http_headers": null
    }}});
    std::fs::write(sb.fake_state().join("config-read.json"), config.to_string()).unwrap();
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    save(&mut c, json!({ "id": "scout", "model": MODEL, "scope": "local" })).await.unwrap();
    let sid = ask(&mut c, "scout", "What is happening on my server?", json!({})).await.unwrap();
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let start = &thread_starts(&sb)[0];
    let server = &start["config"]["mcp_servers"]["linear"];
    assert_eq!(server["enabled"], false);
    assert_eq!(server["url"], config["mcp_servers"]["linear"]["url"]);
    assert!(!server.as_object().unwrap().contains_key("tool_timeout_sec"));
}

#[tokio::test]
async fn stop_while_the_host_agent_answers_ends_the_turn() {
    let sb = Sandbox::new("agents-stop");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    save(&mut c, json!({ "id": "scout", "model": MODEL, "scope": "local" })).await.unwrap();
    let sid = ask(&mut c, "scout", "[slow]", json!({})).await.unwrap();
    c.wait_event(&sid, "assistant_delta", 0).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "interrupted");
    c.wait_status(&sid, "idle").await;
}

#[tokio::test]
async fn pressure_queues_a_new_chat_until_start_anyway() {
    let sb = Sandbox::new("agents-pressure");
    let daemon = sb.start_with(&[("YARD_FORCE_PRESSURE", "1")]);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    save(&mut c, json!({ "id": "scout", "model": MODEL, "scope": "local" })).await.unwrap();
    let sid = ask(&mut c, "scout", "TEST-queued question", json!({})).await.unwrap();
    c.wait_event(&sid, "queued", 0).await;
    assert!(sb.turn_inputs().is_empty());
    // A second message to a queued chat says so instead of piling up.
    assert!(ask(&mut c, "scout", "another", json!({})).await.unwrap_err().contains("waiting for room"));
    c.send(json!({ "op": "session.startNow", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert_eq!(sb.turn_inputs().len(), 1);
    assert!(sb.turn_inputs()[0][0]["text"].as_str().unwrap().contains("Question: TEST-queued question"));
}

#[tokio::test]
async fn discovers_work_outside_sessions_and_indexes_prior_observations() {
    let sb = Sandbox::new("agents-external");
    let repo = make_repo(&sb, "TEST-external-api");
    std::fs::write(
        std::path::Path::new(&repo).join("README.md"),
        "The nebula service listens on port 8081.\n",
    )
    .unwrap();
    git(&repo, &["add", "README.md"], &sb.home());
    git(
        &repo,
        &["commit", "-qm", "Document nebula deployment"],
        &sb.home(),
    );
    std::fs::write(
        sb.home().join(".bash_history"),
        "echo FIRST_NEBULA_DEPLOY\n",
    )
    .unwrap();
    let daemon = sb.start_with(&[("YARD_CONTEXT_ROOTS", &repo)]);
    let mut c = daemon.client().await;
    c.send(json!({"op":"context.collect", "query":"nebula", "ref":"first"}))
        .await;
    let first = c
        .wait_for("external repository context", |m| {
            m["op"] == "context.collect" && m["ref"] == "first"
        })
        .await;
    assert!(first.to_string().contains("8081"));
    assert!(first.to_string().contains("FIRST_NEBULA_DEPLOY"));
    assert!(c.log[0]["sessions"].as_array().unwrap().is_empty());
    std::fs::write(
        sb.home().join(".bash_history"),
        "echo SECOND_NEBULA_DEPLOY\n",
    )
    .unwrap();
    c.send(json!({"op":"context.collect", "query":"nebula", "ref":"second"}))
        .await;
    let second = c
        .wait_for("prior observations", |m| {
            m["op"] == "context.collect" && m["ref"] == "second"
        })
        .await;
    assert!(second["sources"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["title"] == "Earlier host observations"
            && s["text"].as_str().unwrap().contains("FIRST_NEBULA_DEPLOY")));
}

/// The turn inputs' texts, in order.
fn texts(sb: &Sandbox) -> Vec<String> {
    sb.turn_inputs().iter().map(|i| i[0]["text"].as_str().unwrap_or("").to_string()).collect()
}

#[tokio::test]
async fn a_fresh_context_continues_the_same_conversation() {
    let sb = Sandbox::new("agents-fresh");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let agent = save(&mut c, json!({ "name": "Keeper", "engine": "codex", "model": MODEL, "effort": EFFORT, "memory": "TEST-remember" })).await.unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    let sid = ask(&mut c, &id, "TEST-first", json!({})).await.unwrap();
    c.wait_turn_done(&sid, 0).await;

    // Its context runs out: the next message gets a new thread with the
    // preamble and the latest exchanges, and the thread says so.
    let seq = c.next_seq(&sid);
    ask(&mut c, &id, "[context] TEST-too much", json!({})).await.unwrap();
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "failed");
    let seq = c.next_seq(&sid);
    assert_eq!(ask(&mut c, &id, "TEST-after the limit", json!({})).await.unwrap(), sid);
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "completed");
    let note = c.events(&sid).into_iter().filter(|e| e["type"] == "note").last().expect("a note");
    assert!(note["text"].as_str().unwrap().starts_with("Started a fresh context: the engine's context was full."), "{note}");
    let input = texts(&sb).last().unwrap().clone();
    assert!(input.starts_with("You are Keeper,") && input.contains("TEST-remember"), "{input}");
    assert!(input.contains("<earlier_in_this_conversation>") && input.contains("User: TEST-first") && input.contains("You: Echo: TEST-first"), "{input}");
    assert!(input.ends_with("The user's message:\nTEST-after the limit"), "{input}");
    assert_eq!(std::fs::read_to_string(sb.fake_state().join("thread-starts.jsonl")).unwrap().lines().count(), 2);
    // The turn after that goes on in the new thread, as is.
    let seq = c.next_seq(&sid);
    ask(&mut c, &id, "TEST-plain", json!({})).await.unwrap();
    c.wait_turn_done(&sid, seq).await;
    assert_eq!(texts(&sb).last().unwrap(), "TEST-plain");

    // New notes reach the same thread with the next message, once.
    save(&mut c, json!({ "id": id, "memory": "TEST-remember more" })).await.unwrap();
    for (n, text) in ["TEST-noted?", "TEST-and again"].iter().enumerate() {
        let seq = c.next_seq(&sid);
        ask(&mut c, &id, text, json!({})).await.unwrap();
        c.wait_turn_done(&sid, seq).await;
        let input = texts(&sb).last().unwrap().clone();
        assert_eq!(input.contains("<notes_changed>") && input.contains("TEST-remember more"), n == 0, "{input}");
    }
    assert_eq!(std::fs::read_to_string(sb.fake_state().join("thread-starts.jsonl")).unwrap().lines().count(), 2);

    // A new model in Edit: the conversation follows it, in a fresh context.
    save(&mut c, json!({ "id": id, "model": "fake-deep" })).await.unwrap();
    c.wait_for("the conversation on the new model", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["model"] == "fake-deep").await;
    let seq = c.next_seq(&sid);
    ask(&mut c, &id, "TEST-on the new model", json!({})).await.unwrap();
    c.wait_turn_done(&sid, seq).await;
    let starts: Vec<Value> = std::fs::read_to_string(sb.fake_state().join("thread-starts.jsonl")).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(starts.last().unwrap()["model"], "fake-deep");
    assert!(c.events(&sid).iter().any(|e| e["type"] == "note" && e["text"].as_str().unwrap().contains("its model changed")));
    assert!(texts(&sb).last().unwrap().starts_with("You are Keeper,"));

    // The engine lost the thread (here: its state is gone after a restart):
    // the message still gets an answer, in a fresh context.
    drop(c);
    daemon.stop();
    std::fs::remove_file(sb.fake_state().join("threads.json")).unwrap();
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let seq = c.history(&sid).await.len() as u64;
    assert_eq!(ask(&mut c, &id, "TEST-after a restart", json!({})).await.unwrap(), sid);
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "completed");
    assert!(c.events(&sid).iter().any(|e| e["type"] == "note" && e["text"].as_str().unwrap().contains("could not continue")));
    assert!(texts(&sb).last().unwrap().contains("User: TEST-on the new model"));
}
