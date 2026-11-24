//! Agents: the registry, the one conversation that carries an agent's
//! preamble, agents on remote hosts, and the retired host agent (Scout)
//! leaving with its conversation.
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
async fn an_agents_first_turn_carries_its_preamble_and_every_message_goes_to_its_one_conversation() {
    let sb = Sandbox::new("agents-chat");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let folder = sb.folder("TEST-agent-home");
    let agent = save(
        &mut c,
        json!({ "name": "Reviewer", "engine": "codex", "model": MODEL, "effort": EFFORT, "color": "rose", "face": "wink", "about": "Reviews  pull\nrequests",
            "instructions": "Be terse.", "memory": "Main is trunk.", "cwd": folder, "permission": "acceptEdits" }),
    )
    .await
    .unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    assert_eq!(agent["about"], "Reviews pull requests");
    for (bad, why) in [(json!({ "name": "two words", "engine": "codex" }), "one word"), (json!({ "name": "reviewer", "engine": "codex" }), "already called")] {
        assert!(save(&mut c, bad).await.unwrap_err().contains(why));
    }
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

    // An empty message and an unknown agent are refused.
    assert!(ask(&mut c, &id, "  ", json!({})).await.unwrap_err().contains("empty"));
    assert!(ask(&mut c, "nobody", "hi", json!({})).await.unwrap_err().contains("no such agent"));

    // Deleting an agent deletes its conversation.
    c.wait_status(&sid, "idle").await;
    c.send(json!({ "op": "agent.delete", "id": id })).await;
    c.wait_for("the conversation deleted", |m| m["op"] == "session.deleted" && m["sessionId"] == sid.as_str()).await;
    c.wait_for("agents without it", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().is_empty()).await;
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(sb.data().join("agents.json")).unwrap()).unwrap();
    assert!(saved.as_array().unwrap().is_empty());
}

#[tokio::test]
async fn pressure_queues_a_new_chat_until_start_anyway() {
    let sb = Sandbox::new("agents-pressure");
    let daemon = sb.start_with(&[("YARD_FORCE_PRESSURE", "1")]);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let id = save(&mut c, json!({ "name": "Waiter", "engine": "codex", "model": MODEL, "effort": EFFORT })).await.unwrap()["id"].as_str().unwrap().to_string();
    let sid = ask(&mut c, &id, "TEST-queued question", json!({})).await.unwrap();
    c.wait_event(&sid, "queued", 0).await;
    assert!(sb.turn_inputs().is_empty());
    // A second message to a queued chat says so instead of piling up.
    assert!(ask(&mut c, &id, "another", json!({})).await.unwrap_err().contains("waiting for room"));
    c.send(json!({ "op": "session.startNow", "sessionId": sid })).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert_eq!(sb.turn_inputs().len(), 1);
    assert!(sb.turn_inputs()[0][0]["text"].as_str().unwrap().ends_with("The user's message:\nTEST-queued question"));
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

/// `account.add`, waiting for the `accounts` list that holds the new one.
async fn add_account(c: &mut Client, engine: &str, name: &str) -> String {
    let from = c.mark();
    c.send(json!({ "op": "account.add", "engine": engine, "name": name })).await;
    let list = c
        .wait_after(from, "the accounts list with the new account", |m| m["op"] == "accounts" && m["accounts"].as_array().is_some_and(|a| a.iter().any(|x| x["name"] == name)))
        .await;
    list["accounts"].as_array().unwrap().iter().find(|a| a["name"] == name).unwrap()["id"].as_str().unwrap().to_string()
}

/// The `home` (CODEX_HOME) each turn ran under, in order: empty for Default.
fn turn_homes(sb: &Sandbox) -> Vec<String> {
    std::fs::read_to_string(sb.fake_state().join("turn-inputs.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap()["home"].as_str().unwrap_or("").to_string())
        .collect()
}

#[tokio::test]
async fn a_new_account_in_edit_continues_the_conversation_in_a_fresh_context() {
    let sb = Sandbox::new("agents-account");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let work = add_account(&mut c, "codex", "TEST-Work").await;
    // An account the engine does not have is refused.
    assert!(save(&mut c, json!({ "name": "Payer", "engine": "codex", "model": MODEL, "account": "TEST-nobody" })).await.is_err());
    let agent = save(&mut c, json!({ "name": "Payer", "engine": "codex", "model": MODEL, "effort": EFFORT })).await.unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    assert_eq!(agent["account"], "default");
    let sid = ask(&mut c, &id, "TEST-on default", json!({})).await.unwrap();
    c.wait_turn_done(&sid, 0).await;

    let saved = save(&mut c, json!({ "id": id, "account": work })).await.unwrap();
    assert_eq!(saved["account"], work.as_str());
    c.wait_for("the conversation on the new account", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["account"] == work.as_str()).await;
    let seq = c.next_seq(&sid);
    assert_eq!(ask(&mut c, &id, "TEST-on work", json!({})).await.unwrap(), sid);
    assert_eq!(c.wait_turn_done(&sid, seq).await["status"], "completed");
    // The same conversation, a fresh engine thread under the other login,
    // seeded with the exchange so far.
    let note = c.events(&sid).into_iter().filter(|e| e["type"] == "note").last().expect("a note");
    assert!(note["text"].as_str().unwrap().starts_with("Started a fresh context: its account changed."), "{note}");
    let homes = turn_homes(&sb);
    assert_eq!(homes[0], "");
    assert!(homes.last().unwrap().ends_with(&work), "{homes:?}");
    let input = texts(&sb).last().unwrap().clone();
    assert!(input.starts_with("You are Payer,") && input.contains("User: TEST-on default"), "{input}");
    assert_eq!(thread_starts(&sb).len(), 2);
}

/// Add a second yardd as a `direct` host and wait until its models are known.
async fn add_remote(c: &mut Client, remote: &Daemon, name: &str) -> String {
    c.send(json!({ "op": "host.add", "name": name, "kind": "direct", "url": remote.url() })).await;
    let hosts = c
        .wait_for("the remote connected", |m| m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["name"] == name && h["status"] == "connected")))
        .await;
    let host = hosts["hosts"].as_array().unwrap().iter().find(|h| h["name"] == name).unwrap()["id"].as_str().unwrap().to_string();
    c.wait_for("the remote's models", |m| (m["op"] == "host.snapshot" || m["op"] == "models") && m["hostId"] == host.as_str() && m["models"]["codex"].as_array().is_some_and(|a| !a.is_empty()))
        .await;
    host
}

/// The agents a yardd saved, its hosted copies included.
fn saved_agents(sb: &Sandbox) -> Vec<Value> {
    let text = std::fs::read_to_string(sb.data().join("agents.json")).unwrap_or_else(|_| "[]".into());
    serde_json::from_str::<Value>(&text).unwrap().as_array().unwrap().clone()
}

#[tokio::test]
async fn an_agent_on_a_remote_host_runs_its_one_conversation_there() {
    let remote_sb = Sandbox::new("agents-far-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-far-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;

    // The folder and the account are the remote's to check.
    let folder = remote_sb.folder("TEST-far-project");
    assert!(save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": host, "cwd": "/TEST-no/such" })).await.unwrap_err().contains("not a folder"));
    assert!(save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": host, "account": "TEST-nobody" })).await.is_err());
    assert!(save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": "h-TEST-gone" })).await.unwrap_err().contains("no such host"));
    let agent = save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "effort": EFFORT, "host": host, "instructions": "TEST-be far", "memory": "TEST-far memory" })).await.unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    assert_eq!(agent["host"], host.as_str());
    // The record stays on the hub; the remote keeps a copy it never lists.
    assert!(saved_agents(&hub_sb).iter().any(|a| a["id"] == id.as_str() && a["host"] == host.as_str()));
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == id.as_str() && a["hosted"] == true));
    let direct = remote.client().await;
    assert!(!direct.log[0]["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str()));
    drop(direct);

    // Its first message makes its conversation on the remote, in its own
    // folder there, with the preamble.
    let sid = ask(&mut c, &id, "TEST-hello far", json!({})).await.unwrap();
    assert!(sid.starts_with(&format!("{host}~")), "{sid}");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let first = texts(&remote_sb)[0].clone();
    assert!(first.starts_with("You are Far,") && first.contains("TEST-be far") && first.contains("TEST-far memory"), "{first}");
    assert!(first.ends_with("The user's message:\nTEST-hello far"), "{first}");
    assert!(texts(&hub_sb).is_empty());
    assert_eq!(c.events(&sid)[0]["text"], "TEST-hello far");
    let meta = c.wait_for("the conversation's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str()).await["session"].clone();
    let home = remote_sb.data().join("agents").join(&id);
    assert_eq!(meta["cwd"], home.to_str().unwrap());
    assert_eq!(meta["host"], host.as_str());
    assert!(home.is_dir());
    c.wait_for("the hub's record names it", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str() && a["chat"] == sid.as_str())).await;

    // A reply from its conversation goes straight to the session; a mention
    // (agent.ask) lands in the same one. Neither repeats the preamble.
    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "TEST-straight" })).await;
    c.wait_turn_done(&sid, seq).await;
    assert_eq!(texts(&remote_sb).last().unwrap(), "TEST-straight");
    let seq = c.next_seq(&sid);
    assert_eq!(ask(&mut c, &id, "TEST-mentioned", json!({})).await.unwrap(), sid);
    c.wait_turn_done(&sid, seq).await;
    assert_eq!(texts(&remote_sb).last().unwrap(), "TEST-mentioned");

    // New notes reach the thread on its host with the next message, once.
    save(&mut c, json!({ "id": id, "memory": "TEST-far more" })).await.unwrap();
    for (n, text) in ["TEST-noted?", "TEST-and again"].iter().enumerate() {
        let seq = c.next_seq(&sid);
        ask(&mut c, &id, text, json!({})).await.unwrap();
        c.wait_turn_done(&sid, seq).await;
        let input = texts(&remote_sb).last().unwrap().clone();
        assert_eq!(input.contains("<notes_changed>") && input.contains("TEST-far more"), n == 0, "{input}");
    }

    // A new model and folder there: the same conversation in a fresh context.
    save(&mut c, json!({ "id": id, "model": "fake-deep", "cwd": folder })).await.unwrap();
    c.wait_for("the conversation on the new model", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["model"] == "fake-deep" && m["session"]["cwd"] == folder.as_str())
        .await;
    let seq = c.next_seq(&sid);
    assert_eq!(ask(&mut c, &id, "TEST-deeper", json!({})).await.unwrap(), sid);
    c.wait_turn_done(&sid, seq).await;
    assert!(c.events(&sid).iter().any(|e| e["type"] == "note" && e["text"].as_str().unwrap().contains("its model changed")));
    let input = texts(&remote_sb).last().unwrap().clone();
    assert!(input.starts_with("You are Far,") && input.contains("User: TEST-hello far"), "{input}");

    // Files uploaded to the hub are not on the remote: said, not lost.
    let att = c.upload("TEST-note.txt", "text/plain", b"hi", json!({})).await.unwrap();
    let err = ask(&mut c, &id, "TEST-with a file", json!({ "attachments": [att["id"]] })).await.unwrap_err();
    assert!(err.contains("not on TEST-box"), "{err}");

    // Its host goes offline: sending, editing and deleting say so and change nothing.
    remote.stop();
    c.wait_for("the remote offline", |m| m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["id"] == host.as_str() && h["status"] != "connected"))).await;
    let err = ask(&mut c, &id, "TEST-anyone?", json!({})).await.unwrap_err();
    assert!(err.contains("TEST-box, which is offline") && err.contains("not sent"), "{err}");
    assert!(save(&mut c, json!({ "id": id, "memory": "TEST-while away" })).await.unwrap_err().contains("offline"));
    let from = c.mark();
    c.send(json!({ "op": "agent.delete", "id": id })).await;
    assert!(c.wait_error(from, "agent.delete").await.contains("offline"));
    let kept = saved_agents(&hub_sb).into_iter().find(|a| a["id"] == id.as_str()).unwrap();
    assert_eq!(kept["memory"], "TEST-far more");
    assert_eq!(kept["chat"], sid.as_str());
}

#[tokio::test]
async fn an_agent_moves_between_hosts_and_its_next_conversation_starts_where_the_last_ended() {
    let remote_sb = Sandbox::new("agents-move-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-move-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;
    let agent = save(&mut c, json!({ "name": "Mover", "engine": "codex", "model": MODEL, "effort": EFFORT, "memory": "TEST-mover memory" })).await.unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    let here = ask(&mut c, &id, "TEST-said here", json!({})).await.unwrap();
    c.wait_turn_done(&here, 0).await;

    // To the remote: the conversation here stays, the next one starts there
    // with the preamble and the latest exchanges.
    let moved = save(&mut c, json!({ "id": id, "host": host })).await.unwrap();
    assert!(moved["chat"].is_null());
    assert_eq!(moved["previous"], json!([here]));
    assert!(moved.get("seed").is_none());
    let there = ask(&mut c, &id, "TEST-said there", json!({})).await.unwrap();
    assert!(there.starts_with(&format!("{host}~")));
    c.wait_turn_done(&there, 0).await;
    let input = texts(&remote_sb)[0].clone();
    assert!(input.starts_with("You are Mover,") && input.contains("TEST-mover memory"), "{input}");
    assert!(input.contains("<earlier_in_this_conversation>") && input.contains("User: TEST-said here") && input.contains("You: Echo: TEST-said here"), "{input}");
    assert!(input.ends_with("The user's message:\nTEST-said there"), "{input}");
    let note = c.events(&there).into_iter().find(|e| e["type"] == "note").expect("a note");
    assert!(note["text"].as_str().unwrap().starts_with("Started a fresh context: it moved to TEST-box."), "{note}");
    assert!(!c.history(&here).await.is_empty(), "the old conversation stays readable");

    // Not while a turn runs there.
    let seq = c.next_seq(&there);
    ask(&mut c, &id, "[slow] TEST-busy", json!({})).await.unwrap();
    c.wait_event(&there, "assistant_delta", seq).await;
    let err = save(&mut c, json!({ "id": id, "host": "local" })).await.unwrap_err();
    assert!(err.contains("middle of a turn"), "{err}");
    c.send(json!({ "op": "session.interrupt", "sessionId": there })).await;
    c.wait_turn_done(&there, seq).await;
    c.wait_status(&there, "idle").await;

    // And back: the remote lets go of its copy but keeps the conversation.
    let back = save(&mut c, json!({ "id": id, "host": "local" })).await.unwrap();
    assert_eq!(back["previous"], json!([here, there]));
    assert!(!saved_agents(&remote_sb).iter().any(|a| a["id"] == id.as_str()));
    assert!(!c.history(&there).await.is_empty());
    let home = ask(&mut c, &id, "TEST-home again", json!({})).await.unwrap();
    assert!(!home.contains('~') && home != here);
    c.wait_turn_done(&home, 0).await;
    let input = texts(&hub_sb).last().unwrap().clone();
    assert!(input.contains("User: TEST-said there") && input.ends_with("The user's message:\nTEST-home again"), "{input}");

    // Deleting it deletes every conversation it had, on both hosts.
    let from = c.mark();
    c.send(json!({ "op": "agent.delete", "id": id, "ref": "gone" })).await;
    c.wait_after(from, "the delete answered", |m| m["ref"] == "gone" && m["op"] == "agent.deleted").await;
    for sid in [&here, &there, &home] {
        c.wait_after(from, "a conversation deleted", |m| m["op"] == "session.deleted" && m["sessionId"] == sid.as_str()).await;
    }
    assert!(!saved_agents(&hub_sb).iter().any(|a| a["id"] == id.as_str()));
}

#[tokio::test]
async fn the_retired_host_agent_leaves_with_its_conversation() {
    let sb = Sandbox::new("agents-retire");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let id = save(&mut c, json!({ "name": "Keeper", "engine": "codex", "model": MODEL, "effort": EFFORT })).await.unwrap()["id"].as_str().unwrap().to_string();
    let kept = ask(&mut c, &id, "TEST-stays", json!({})).await.unwrap();
    c.wait_turn_done(&kept, 0).await;
    let plain = c.create(&sb.folder("TEST-proj"), "TEST-scout's chat", json!({})).await;
    c.wait_turn_done(&plain, 0).await;
    c.wait_status(&plain, "idle").await;
    drop(c);
    daemon.stop();
    // What an older yardd left: Scout in agents.json and a session tagged with it.
    let file = sb.data().join("agents.json");
    let mut list: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    list.insert(0, json!({ "id": "scout", "name": "Scout", "engine": "codex", "builtin": true, "scope": "all", "chat": plain, "createdAt": 1, "updatedAt": 1 }));
    std::fs::write(&file, serde_json::to_string(&list).unwrap()).unwrap();
    let mut meta = saved_meta(&sb, &plain).unwrap();
    meta["agent"] = json!("scout");
    let conn = rusqlite::Connection::open(sb.data().join("yard.db")).unwrap();
    conn.execute("UPDATE sessions SET meta = ?2 WHERE id = ?1", [plain.as_str(), meta.to_string().as_str()]).unwrap();
    drop(conn);

    let daemon = sb.start();
    let c = daemon.client().await;
    let agents = c.log[0]["agents"].as_array().unwrap();
    assert_eq!(agents.iter().map(|a| a["id"].as_str().unwrap()).collect::<Vec<_>>(), [id.as_str()]);
    let sessions: Vec<&str> = c.log[0]["sessions"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(sessions, [kept.as_str()]);
    let saved: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(saved.iter().all(|a| a["id"] != "scout"));
}
