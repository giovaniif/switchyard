//! Agents: the registry, the one conversation that carries an agent's
//! preamble, agents that belong to the host they run on (listed, reached,
//! edited and moved from any hub; handed over from a hub that kept them),
//! and the retired host agent (Scout) leaving with its conversation.
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
    assert_eq!(agent["handle"], "Reviewer");
    for (bad, why) in [(json!({ "name": "reviewer", "engine": "codex" }), "already called"), (json!({ "name": " \n ", "engine": "codex" }), "Give it a name")] {
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

/// Slack calls go to a port nobody answers: tests never reach slack.com.
const NO_SLACK: &[(&str, &str)] = &[("YARD_SLACK_API", "http://127.0.0.1:9/api/")];

#[tokio::test]
async fn an_agent_belongs_to_the_host_it_runs_on_and_the_hub_lists_it() {
    let remote_sb = Sandbox::new("agents-far-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-far-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;

    // The folder and the account are the remote's to check: it makes the agent.
    let folder = remote_sb.folder("TEST-far-project");
    assert!(save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": host, "cwd": "/TEST-no/such" })).await.unwrap_err().contains("not a folder"));
    assert!(save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": host, "account": "TEST-nobody" })).await.is_err());
    let agent = save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "effort": EFFORT, "host": host, "instructions": "TEST-be far", "memory": "TEST-far memory" })).await.unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    let (prefix, own_id) = id.split_once('~').expect("a host's agent is named like its sessions");
    assert_eq!(prefix, host);
    assert_eq!(agent["host"], host.as_str());
    // The record is the remote's own; nothing of it is kept on the hub.
    assert!(!saved_agents(&hub_sb).iter().any(|a| a["id"] == own_id));
    let kept = saved_agents(&remote_sb).into_iter().find(|a| a["id"] == own_id).expect("the remote keeps it");
    assert!(kept["hosted"].is_null() && kept["host"] == "local", "{kept}");
    // The remote lists it as its own, and the hub lists it with the prefix.
    let direct = remote.client().await;
    assert!(direct.log[0]["agents"].as_array().unwrap().iter().any(|a| a["id"] == own_id));
    drop(direct);
    c.wait_for("the hub lists it", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str() && a["host"] == host.as_str() && a["memory"] == "TEST-far memory")).await;

    // Its first message makes its conversation on the remote, in its own
    // folder there, with the preamble.
    let sid = ask(&mut c, &id, "TEST-hello far", json!({})).await.unwrap();
    assert!(sid.starts_with(&format!("{host}~")), "{sid}");
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    let first = texts(&remote_sb)[0].clone();
    assert!(first.starts_with("You are Far,") && first.contains("TEST-be far") && first.contains("TEST-far memory"), "{first}");
    assert!(first.ends_with("The user's message:\nTEST-hello far"), "{first}");
    assert!(texts(&hub_sb).is_empty());
    let meta = c.wait_for("the conversation's metadata", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str()).await["session"].clone();
    let home = remote_sb.data().join("agents").join(own_id);
    assert_eq!(meta["cwd"], home.to_str().unwrap());
    assert!(home.is_dir());
    c.wait_for("the hub's list names its conversation", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str() && a["chat"] == sid.as_str())).await;

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

    // New notes reach the thread with the next message, once.
    save(&mut c, json!({ "id": id, "memory": "TEST-far more", "host": host })).await.unwrap();
    assert_eq!(saved_agents(&remote_sb).into_iter().find(|a| a["id"] == own_id).unwrap()["memory"], "TEST-far more");
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

    // Files uploaded to the hub are not on the remote: said, not lost.
    let att = c.upload("TEST-note.txt", "text/plain", b"hi", json!({})).await.unwrap();
    let err = ask(&mut c, &id, "TEST-with a file", json!({ "attachments": [att["id"]] })).await.unwrap_err();
    assert!(err.contains("not on TEST-box"), "{err}");

    // Its host goes offline: it stays listed, and sending, editing and
    // deleting say so and change nothing.
    remote.stop();
    c.wait_for("the remote offline", |m| m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["id"] == host.as_str() && h["status"] != "connected"))).await;
    let err = ask(&mut c, &id, "TEST-anyone?", json!({})).await.unwrap_err();
    assert!(err.contains("Far runs on TEST-box, which is offline") && err.contains("not sent"), "{err}");
    assert!(save(&mut c, json!({ "id": id, "memory": "TEST-while away" })).await.unwrap_err().contains("offline"));
    let from = c.mark();
    c.send(json!({ "op": "agent.delete", "id": id })).await;
    assert!(c.wait_error(from, "agent.delete").await.contains("offline"));
    let from = c.mark();
    c.send(json!({ "op": "agent.list", "ref": "list" })).await;
    let listed = c.wait_after(from, "the list", |m| m["ref"] == "list").await;
    assert!(listed["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str() && a["chat"] == sid.as_str()), "{listed}");
    assert_eq!(saved_agents(&remote_sb).into_iter().find(|a| a["id"] == own_id).unwrap()["memory"], "TEST-far more");
}

#[tokio::test]
async fn a_second_hub_sees_the_hosts_agents_and_can_ask_edit_and_delete_them() {
    let remote_sb = Sandbox::new("agents-two-remote");
    let remote = remote_sb.start();
    let first_sb = Sandbox::new("agents-two-first");
    let first = first_sb.start();
    let mut a = first.client().await;
    ready(&mut a).await;
    let host_a = add_remote(&mut a, &remote, "TEST-box").await;
    let made = save(&mut a, json!({ "name": "On-call", "engine": "codex", "model": MODEL, "effort": EFFORT, "host": host_a, "memory": "TEST-pager" })).await.unwrap();
    let own_id = made["id"].as_str().unwrap().split_once('~').unwrap().1.to_string();
    let chat = ask(&mut a, made["id"].as_str().unwrap(), "TEST-from the first Mac", json!({})).await.unwrap();
    a.wait_turn_done(&chat, 0).await;

    // Another Mac adds the same server: the agent is listed there too, with
    // its conversation, under that Mac's own id for the host.
    let second_sb = Sandbox::new("agents-two-second");
    let second = second_sb.start();
    let mut b = second.client().await;
    ready(&mut b).await;
    let host_b = add_remote(&mut b, &remote, "TEST-server").await;
    let id = format!("{host_b}~{own_id}");
    let listed = b.wait_for("the second Mac lists it", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|x| x["id"] == id.as_str() && x["chat"].is_string())).await;
    let seen = listed["agents"].as_array().unwrap().iter().find(|x| x["id"] == id.as_str()).unwrap().clone();
    let remote_chat = chat.split_once('~').unwrap().1;
    assert_eq!(seen["chat"], format!("{host_b}~{remote_chat}"));
    assert_eq!((seen["name"].as_str(), seen["memory"].as_str(), seen["host"].as_str()), (Some("On-call"), Some("TEST-pager"), Some(host_b.as_str())));
    assert!(saved_agents(&second_sb).is_empty());

    // It asks: the same one conversation, on the server.
    let sid = ask(&mut b, &id, "TEST-from the second Mac", json!({})).await.unwrap();
    assert_eq!(sid, format!("{host_b}~{remote_chat}"));
    b.wait_turn_done(&sid, b.next_seq(&sid).saturating_sub(1)).await;
    assert_eq!(texts(&remote_sb).last().unwrap(), "TEST-from the second Mac");

    // It edits: the server keeps the change, and the first Mac hears it.
    let edited = save(&mut b, json!({ "id": id, "memory": "TEST-pager, edited", "host": host_b })).await.unwrap();
    assert_eq!(edited["id"], id.as_str());
    assert_eq!(saved_agents(&remote_sb).into_iter().find(|x| x["id"] == own_id.as_str()).unwrap()["memory"], "TEST-pager, edited");
    let first_id = made["id"].as_str().unwrap().to_string();
    a.wait_for("the first Mac sees the edit", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|x| x["id"] == first_id.as_str() && x["memory"] == "TEST-pager, edited")).await;

    // It deletes: gone on the server, and from both lists.
    let from = b.mark();
    b.send(json!({ "op": "agent.delete", "id": id, "ref": "gone" })).await;
    let done = b.wait_after(from, "the delete answered", |m| m["ref"] == "gone").await;
    assert_eq!((done["op"].as_str(), done["id"].as_str()), (Some("agent.deleted"), Some(id.as_str())), "{done}");
    assert!(!saved_agents(&remote_sb).iter().any(|x| x["id"] == own_id.as_str()));
    a.wait_for("the first Mac's list without it", |m| m["op"] == "agents" && !m["agents"].as_array().unwrap().iter().any(|x| x["id"] == first_id.as_str())).await;
    b.wait_for("the second Mac's list without it", |m| m["op"] == "agents" && !m["agents"].as_array().unwrap().iter().any(|x| x["id"] == id.as_str())).await;
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
    let agent = save(&mut c, json!({ "name": "Mover", "engine": "codex", "model": MODEL, "effort": EFFORT, "memory": "TEST-mover memory", "instructions": "TEST-move well" })).await.unwrap();
    let id = agent["id"].as_str().unwrap().to_string();
    let here = ask(&mut c, &id, "TEST-said here", json!({})).await.unwrap();
    c.wait_turn_done(&here, 0).await;
    // The shared bot's default agent is this one (no Slack app of its own).
    let from = c.mark();
    c.send(json!({ "op": "slack.route", "agent": id, "ref": "route" })).await;
    c.wait_after(from, "the default agent set", |m| m["ref"] == "route").await;

    // To the remote: the record goes there, the conversation here stays as
    // an ordinary session, and the next one starts there with the preamble
    // and the latest exchanges.
    let moved = save(&mut c, json!({ "id": id, "host": host })).await.unwrap();
    let far_id = format!("{host}~{id}");
    assert_eq!(moved["id"], far_id.as_str());
    assert_eq!(moved["host"], host.as_str());
    assert!(moved["chat"].is_null());
    assert!(moved.get("seed").is_none());
    assert!(!saved_agents(&hub_sb).iter().any(|a| a["id"] == id.as_str()));
    let there_record = saved_agents(&remote_sb).into_iter().find(|a| a["id"] == id.as_str()).expect("the remote owns it");
    assert_eq!((there_record["memory"].as_str(), there_record["instructions"].as_str()), (Some("TEST-mover memory"), Some("TEST-move well")));
    assert!(there_record["hosted"].is_null());
    c.wait_for("the old conversation is an ordinary session", |m| m["op"] == "session" && m["session"]["id"] == here.as_str() && m["session"]["agent"].is_null()).await;
    // Backed up before the first change, even with no Slack app of its own.
    assert_eq!(oldest_backup(&hub_sb, "slack.json")["defaultAgent"], id.as_str());
    assert_eq!(oldest_backup(&hub_sb, "agents.json").as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap()["host"], "local");
    let slack_now: Value = serde_json::from_str(&std::fs::read_to_string(hub_sb.data().join("slack.json")).unwrap()).unwrap();
    assert_eq!(slack_now["defaultAgent"], far_id.as_str());
    assert!(!c.history(&here).await.is_empty(), "the old conversation stays readable");
    let there = ask(&mut c, &far_id, "TEST-said there", json!({})).await.unwrap();
    assert!(there.starts_with(&format!("{host}~")));
    c.wait_turn_done(&there, 0).await;
    let input = texts(&remote_sb)[0].clone();
    assert!(input.starts_with("You are Mover,") && input.contains("TEST-mover memory"), "{input}");
    assert!(input.contains("<earlier_in_this_conversation>") && input.contains("User: TEST-said here") && input.contains("You: Echo: TEST-said here"), "{input}");
    assert!(input.ends_with("The user's message:\nTEST-said there"), "{input}");
    let note = c.events(&there).into_iter().find(|e| e["type"] == "note").expect("a note");
    assert!(note["text"].as_str().unwrap().starts_with("Started a fresh context: it moved to TEST-box."), "{note}");

    // Not while a turn runs there.
    let seq = c.next_seq(&there);
    ask(&mut c, &far_id, "[slow] TEST-busy", json!({})).await.unwrap();
    c.wait_event(&there, "assistant_delta", seq).await;
    let err = save(&mut c, json!({ "id": far_id, "host": "local" })).await.unwrap_err();
    assert!(err.contains("middle of a turn"), "{err}");
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == id.as_str()));
    c.send(json!({ "op": "session.interrupt", "sessionId": there })).await;
    c.wait_turn_done(&there, seq).await;
    c.wait_status(&there, "idle").await;

    // And back: the remote lets go of it but keeps the conversation.
    let before_back = backups(&hub_sb, "slack.json");
    let back = save(&mut c, json!({ "id": far_id, "host": "local" })).await.unwrap();
    assert_eq!(back["id"], id.as_str());
    // Back here: this side backed up before it took the agent or moved the default.
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(hub_sb.data()).unwrap().map(|e| e.unwrap().path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with("slack.json.bak-")).collect();
    found.sort();
    assert!(found.len() > before_back);
    let taken: Value = serde_json::from_str(&std::fs::read_to_string(&found[before_back]).unwrap()).unwrap();
    assert_eq!(taken["defaultAgent"], far_id.as_str(), "{taken}");
    assert_eq!(back["host"], "local");
    assert!(!saved_agents(&remote_sb).iter().any(|a| a["id"] == id.as_str()));
    assert!(saved_agents(&hub_sb).iter().any(|a| a["id"] == id.as_str() && a["memory"] == "TEST-mover memory"));
    assert!(!c.history(&there).await.is_empty());
    let home = ask(&mut c, &id, "TEST-home again", json!({})).await.unwrap();
    assert!(!home.contains('~') && home != here);
    c.wait_turn_done(&home, 0).await;
    let input = texts(&hub_sb).last().unwrap().clone();
    assert!(input.contains("User: TEST-said there") && input.ends_with("The user's message:\nTEST-home again"), "{input}");

    // Refused while the other end is offline; nothing changes.
    remote.stop();
    c.wait_for("the remote offline", |m| m["op"] == "hosts" && m["hosts"].as_array().is_some_and(|h| h.iter().any(|h| h["id"] == host.as_str() && h["status"] != "connected"))).await;
    let err = save(&mut c, json!({ "id": id, "host": host })).await.unwrap_err();
    assert!(err.contains("offline"), "{err}");
    assert!(saved_agents(&hub_sb).iter().any(|a| a["id"] == id.as_str()));

    // Deleting it deletes its conversation; the ones it left are ordinary sessions now.
    let from = c.mark();
    c.send(json!({ "op": "agent.delete", "id": id, "ref": "gone" })).await;
    c.wait_after(from, "the delete answered", |m| m["ref"] == "gone" && m["op"] == "agent.deleted").await;
    c.wait_after(from, "its conversation deleted", |m| m["op"] == "session.deleted" && m["sessionId"] == home.as_str()).await;
    assert!(!saved_agents(&hub_sb).iter().any(|a| a["id"] == id.as_str()));
    assert!(!c.history(&here).await.is_empty(), "the conversation it left here stays");
}

/// What a hub sends a host that runs its agent the older way, sent by hand.
async fn save_hosted(c: &mut Client, agent: Value) {
    let from = c.mark();
    c.send(json!({ "op": "agent.hosted.save", "agent": agent, "ref": "s" })).await;
    let r = c.wait_after(from, "hosted save", |m| m["ref"] == "s").await;
    assert_eq!(r["op"], "agent.hosted.saved", "{r}");
}

async fn ask_hosted(c: &mut Client, id: &str, text: &str) -> String {
    let from = c.mark();
    c.send(json!({ "op": "agent.hosted.ask", "agentId": id, "text": text, "ref": "a" })).await;
    let r = c.wait_after(from, "hosted ask", |m| m["ref"] == "a").await;
    assert_eq!(r["op"], "created", "{r}");
    r["sessionId"].as_str().unwrap().to_string()
}

/// A hub from before agents lived on their host: its record of an agent on
/// `host` (whose hidden copy there runs conversation `chat`), the host's
/// entry and its own Slack app, written as that hub left them.
fn seed_old_hub(hub_sb: &Sandbox, remote: &Daemon, host: &str, chat: &str) {
    let hosts = json!([{ "id": host, "name": "TEST-box", "kind": "direct", "url": remote.url() }]);
    std::fs::write(hub_sb.data().join("hosts.json"), hosts.to_string()).unwrap();
    std::fs::write(hub_sb.data().join("hub-id"), "0123456789abcdef0123456789abcdef").unwrap();
    let record = json!([{
        "id": "g-oncall", "name": "On-call", "handle": "On-call", "color": "teal", "face": "glasses", "about": "TEST-watches the pager",
        "engine": "codex", "model": MODEL, "effort": EFFORT, "account": "default", "cwd": "", "permission": "ask",
        "instructions": "TEST-be calm", "memory": "TEST-the pager is loud", "chat": format!("{host}~{chat}"), "host": host,
        "previous": ["s-TEST-left-here"], "request": { "slack": true }, "createdAt": 1, "updatedAt": 1,
    }]);
    std::fs::write(hub_sb.data().join("agents.json"), record.to_string()).unwrap();
    // Its own Slack app, with a field this build does not know: it must travel as it is.
    let slack = json!({
        "allowed": [{ "id": "UOWNER", "name": "TEST-Owner" }, { "id": "UOTHER", "name": "TEST-Someone else" }],
        "workspaces": [{ "teamId": "T1", "teamName": "TEST-team", "userId": "UOWNER", "refreshToken": "xoxe-TEST-refresh", "accessToken": "xoxe.xoxp-TEST", "expiresAt": 1 }],
        "apps": [{ "agent": "g-oncall", "teamId": "T1", "teamName": "TEST-team", "appId": "A-TEST", "botToken": "xoxb-TEST-oncall", "botUser": "UBOT",
                   "owner": "UOWNER", "ownerName": "TEST-Owner", "handle": "on-call", "mentions": [{ "id": "UPAGER", "name": "TEST-pager" }], "watchedChannels": ["C-TEST"], "manifest": "{}" }],
        "defaultAgent": "g-oncall",
        "threads": [{ "key": "T1/C1/1.0", "agent": "g-oncall", "at": 1, "app": true, "place": "#ops" }, { "key": "T1/C2/2.0", "agent": "g-oncall", "at": 1, "app": false, "place": "#dev" }],
    });
    std::fs::write(hub_sb.data().join("slack.json"), slack.to_string()).unwrap();
}

/// Wait until `cond` holds, for a change on disk no message announces.
async fn eventually(what: &str, cond: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + WAIT;
    while !cond() {
        assert!(std::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// The oldest `<name>.bak-<time>` in the data folder, read.
fn oldest_backup(sb: &Sandbox, name: &str) -> Value {
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(sb.data()).unwrap().map(|e| e.unwrap().path()).filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(&format!("{name}.bak-"))).collect();
    found.sort();
    let first = found.first().unwrap_or_else(|| panic!("no backup of {name}"));
    serde_json::from_str(&std::fs::read_to_string(first).unwrap()).unwrap()
}

fn backups(sb: &Sandbox, name: &str) -> usize {
    std::fs::read_dir(sb.data()).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with(&format!("{name}.bak-"))).count()
}

fn saved_slack(sb: &Sandbox) -> Value {
    serde_json::from_str(&std::fs::read_to_string(sb.data().join("slack.json")).unwrap_or_else(|_| "{}".into())).unwrap()
}

#[tokio::test]
async fn a_hub_hands_the_agents_it_kept_to_their_host_once_and_safely() {
    let remote_sb = Sandbox::new("agents-migrate-remote");
    let remote = remote_sb.start();
    let mut r = remote.client().await;
    ready(&mut r).await;
    // The host runs the agent as an older hub left it: a hidden copy with its conversation.
    let hub_id = "0123456789abcdef0123456789abcdef";
    save_hosted(&mut r, json!({ "id": "g-oncall", "name": "On-call", "handle": "On-call", "engine": "codex", "model": MODEL, "effort": EFFORT, "memory": "TEST-the pager is loud", "createdAt": 1, "updatedAt": 1, "hub": hub_id })).await;
    let chat = ask_hosted(&mut r, "g-oncall", "TEST-before the update").await;
    r.wait_turn_done(&chat, 0).await;
    drop(r);

    // Older backups beside the file: only the newest ten are kept.
    for n in 0..12 {
        std::fs::write(remote_sb.data().join(format!("agents.json.bak-20200101-0000{n:02}.000")), "[]").unwrap();
    }
    let host = "hTEST1";
    let hub_sb = Sandbox::new("agents-migrate-hub");
    seed_old_hub(&hub_sb, &remote, host, &chat);
    let hub = hub_sb.start_with(NO_SLACK);
    let mut c = hub.client().await;
    let id = format!("{host}~g-oncall");
    let listed = c.wait_for("the agent listed from its host", |m| (m["op"] == "agents" || m["op"] == "snapshot") && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str())).await;
    c.wait_for("the hub's own record gone", |m| (m["op"] == "agents" || m["op"] == "snapshot") && !m["agents"].as_array().unwrap().iter().any(|a| a["id"] == "g-oncall")).await;
    let a = listed["agents"].as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap().clone();
    // Nothing was lost: notes, look, request, and its conversation.
    for (k, v) in [("instructions", "TEST-be calm"), ("memory", "TEST-the pager is loud"), ("about", "TEST-watches the pager"), ("face", "glasses"), ("color", "teal")] {
        assert_eq!(a[k], v, "{k}: {a}");
    }
    assert_eq!(a["chat"], format!("{host}~{chat}"));
    assert_eq!(a["request"]["slack"], true);
    let own = saved_agents(&remote_sb).into_iter().find(|x| x["id"] == "g-oncall").unwrap();
    assert!(own["hosted"].is_null() && own["hub"].is_null(), "the copy is the host's own now: {own}");
    assert_eq!(own["chat"], chat.as_str());
    assert!(own.get("previous").is_none(), "a conversation not on this host is dropped from the list: {own}");
    assert_eq!(backups(&remote_sb, "agents.json"), 10);
    assert!(!remote_sb.data().join("agents.json.bak-20200101-000000.000").exists(), "the oldest went first");
    // The hub lets go once the host confirmed (its list hides the record before).
    eventually("the hub's record removed", || saved_agents(&hub_sb).is_empty()).await;
    assert!(backups(&hub_sb, "agents.json") >= 1);

    // Its Slack app moved whole: every field (one this build does not know
    // too), its workspace's token and the allowed people. The hub keeps
    // none of it, and its shared bot's default agent follows it.
    let there = saved_slack(&remote_sb);
    let app = &there["apps"][0];
    assert_eq!((app["agent"].as_str(), app["appId"].as_str(), app["botToken"].as_str()), (Some("g-oncall"), Some("A-TEST"), Some("xoxb-TEST-oncall")));
    assert_eq!(app["watchedChannels"], json!(["C-TEST"]));
    assert_eq!(app["mentions"][0]["id"], "UPAGER");
    assert_eq!(there["workspaces"][0]["refreshToken"], "xoxe-TEST-refresh");
    // Only whoever installed the app may reach the server through it; the
    // Mac's other allowed people are the Mac's (they reach its shared bot).
    assert_eq!(there["allowed"], json!([{ "id": "UOWNER", "name": "TEST-Owner" }]));
    let left = saved_slack(&hub_sb);
    assert!(left["apps"].as_array().unwrap().is_empty(), "{left}");
    assert_eq!(left["defaultAgent"], id.as_str());
    // The threads its own app heard went with it (its slack_reply posts there); the shared bot's stay, following it.
    assert_eq!(left["threads"], json!([{ "key": "T1/C2/2.0", "agent": id, "at": 1, "app": false, "place": "#dev" }]));
    assert_eq!(there["threads"], json!([{ "key": "T1/C1/1.0", "agent": "g-oncall", "at": 1, "app": true, "place": "#ops" }]));
    assert!(backups(&hub_sb, "slack.json") >= 1);
    // The Mac backed up before its first change: its oldest backup is the file as it was.
    let first = oldest_backup(&hub_sb, "slack.json");
    assert_eq!(first["defaultAgent"], "g-oncall", "{first}");
    assert_eq!(first["apps"][0]["agent"], "g-oncall");
    assert_eq!(oldest_backup(&hub_sb, "agents.json")[0]["id"], "g-oncall");
    c.wait_for("its app listed under its new id", |m| m["op"] == "slack" && m["slack"]["agentApps"][&id].is_object()).await;

    // It goes on in the same conversation, now as the host's own.
    let seq = c.next_seq(&id_chat(&a));
    let sid = ask(&mut c, &id, "TEST-after the update", json!({})).await.unwrap();
    assert_eq!(sid, format!("{host}~{chat}"));
    c.wait_turn_done(&sid, seq).await;
    assert_eq!(texts(&remote_sb).last().unwrap(), "TEST-after the update");

    // Twice is safe: a hub that lost the host's answer (its old file back)
    // hands it over again, and the host, owning it already, changes nothing.
    drop(c);
    hub.stop();
    let old = std::fs::read_dir(hub_sb.data()).unwrap().map(|e| e.unwrap().path()).find(|p| p.file_name().unwrap().to_string_lossy().starts_with("agents.json.bak-")).unwrap();
    std::fs::copy(&old, hub_sb.data().join("agents.json")).unwrap();
    let memory_before = saved_agents(&remote_sb).into_iter().find(|x| x["id"] == "g-oncall").unwrap()["memory"].clone();
    let hub = hub_sb.start_with(NO_SLACK);
    let mut c = hub.client().await;
    c.wait_for("the kept record gone again", |m| (m["op"] == "agents" || m["op"] == "snapshot") && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == id.as_str()) && !m["agents"].as_array().unwrap().iter().any(|a| a["id"] == "g-oncall")).await;
    eventually("the hub's record removed again", || saved_agents(&hub_sb).is_empty()).await;
    let after = saved_agents(&remote_sb);
    assert_eq!(after.iter().filter(|x| x["id"] == "g-oncall").count(), 1);
    assert_eq!(after.iter().find(|x| x["id"] == "g-oncall").unwrap()["memory"], memory_before);
    assert_eq!(saved_slack(&remote_sb)["apps"].as_array().unwrap().len(), 1);
    drop(hub);
}

fn id_chat(a: &Value) -> String {
    a["chat"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn a_host_too_old_to_own_agents_keeps_running_the_hubs_copy() {
    let remote_sb = Sandbox::new("agents-old-remote");
    let remote = remote_sb.start_with(&[("YARD_TEST_HOSTED_ONLY", "1")]);
    let mut r = remote.client().await;
    ready(&mut r).await;
    save_hosted(&mut r, json!({ "id": "g-oncall", "name": "On-call", "engine": "codex", "model": MODEL, "effort": EFFORT, "createdAt": 1, "updatedAt": 1, "hub": "0123456789abcdef0123456789abcdef" })).await;
    let chat = ask_hosted(&mut r, "g-oncall", "TEST-old host").await;
    r.wait_turn_done(&chat, 0).await;
    drop(r);

    let host = "hTEST2";
    let hub_sb = Sandbox::new("agents-old-hub");
    seed_old_hub(&hub_sb, &remote, host, &chat);
    let hub = hub_sb.start_with(NO_SLACK);
    let mut c = hub.client().await;
    // The host says it cannot own agents; the record stays here and runs there as before.
    c.wait_for("the host says it is older", |m| (m["op"] == "hosts" || m["op"] == "snapshot") && m["hosts"][0]["ownAgents"] == false).await;
    c.drain_for(std::time::Duration::from_millis(500)).await;
    assert!(saved_agents(&hub_sb).iter().any(|a| a["id"] == "g-oncall" && a["host"] == host));
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == "g-oncall" && a["hosted"] == true));
    assert_eq!(saved_slack(&hub_sb)["apps"].as_array().unwrap().len(), 1);
    let seq = c.next_seq(&format!("{host}~{chat}"));
    let sid = ask(&mut c, "g-oncall", "TEST-still works", json!({})).await.unwrap();
    assert_eq!(sid, format!("{host}~{chat}"));
    c.wait_turn_done(&sid, seq).await;
    // A new agent there, or a move there, is refused with what to do.
    let err = save(&mut c, json!({ "name": "Fresh", "engine": "codex", "model": MODEL, "host": host })).await.unwrap_err();
    assert!(err.contains("Update Switchyard"), "{err}");
    let mine = save(&mut c, json!({ "name": "Mine", "engine": "codex", "model": MODEL })).await.unwrap();
    let err = save(&mut c, json!({ "id": mine["id"], "host": host })).await.unwrap_err();
    assert!(err.contains("Update Switchyard"), "{err}");
}

#[tokio::test]
async fn a_display_name_gets_a_mention_handle_that_stays_while_it_fits() {
    let sb = Sandbox::new("agents-handles");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let base = json!({ "engine": "codex", "model": MODEL, "effort": EFFORT });
    let with = |extra: Value| {
        let mut v = base.clone();
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };
    // Spaces and punctuation are kept in the name; the handle is one word made from it.
    let a = save(&mut c, with(json!({ "name": "  Custom   Domains Triage ", "handle": "ignored" }))).await.unwrap();
    let id = a["id"].as_str().unwrap().to_string();
    assert_eq!((a["name"].as_str(), a["handle"].as_str()), (Some("Custom Domains Triage"), Some("Custom-Domains-Triage")));
    // Unique names ignoring case; a name whose handle is taken gets a number.
    assert!(save(&mut c, with(json!({ "name": "custom domains triage" }))).await.unwrap_err().contains("Another agent is already called"));
    let b = save(&mut c, with(json!({ "name": "Custom-Domains: Triage!" }))).await.unwrap();
    assert_eq!(b["handle"], "Custom-Domains-Triage-2");
    // Too long, or with something invisible in it, is refused with the reason.
    assert!(save(&mut c, with(json!({ "name": "x".repeat(41) }))).await.unwrap_err().contains("at most 40 characters"));
    assert!(save(&mut c, with(json!({ "name": "Look\u{202E}alike" }))).await.unwrap_err().contains("invisible"));
    // A rename to the same words keeps mentions working; new words, a new handle.
    let b2 = save(&mut c, json!({ "id": b["id"], "name": "Custom domains: triage" })).await.unwrap();
    assert_eq!(b2["handle"], "Custom-Domains-Triage-2");
    let a2 = save(&mut c, json!({ "id": id, "name": "Domain Watcher", "handle": "Nope" })).await.unwrap();
    assert_eq!(a2["handle"], "Domain-Watcher");
    let a3 = save(&mut c, json!({ "id": id, "about": "Watches domains" })).await.unwrap();
    assert_eq!(a3["handle"], "Domain-Watcher");

    // The preamble names it as it reads and by its handle.
    let sid = ask(&mut c, &id, "TEST-hello", json!({})).await.unwrap();
    c.wait_turn_done(&sid, 0).await;
    let first = sb.turn_inputs()[0][0]["text"].as_str().unwrap().to_string();
    assert!(first.starts_with("You are Domain Watcher (@Domain-Watcher),"), "{first}");

    // Agents saved before handles get their one-word name as handle.
    drop(c);
    daemon.stop();
    let file = sb.data().join("agents.json");
    let mut list: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    list.push(json!({ "id": "gold", "name": "Old-timer", "engine": "codex", "createdAt": 1, "updatedAt": 1 }));
    std::fs::write(&file, serde_json::to_string(&list).unwrap()).unwrap();
    let daemon = sb.start();
    let c = daemon.client().await;
    let old = c.log[0]["agents"].as_array().unwrap().iter().find(|a| a["id"] == "gold").unwrap().clone();
    assert_eq!(old["handle"], "Old-timer");
    let saved: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    assert!(saved.iter().all(|a| a["handle"].as_str().is_some_and(|h| !h.is_empty())), "{saved:?}");
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

/// A client of `d` that a browser would be: it sends the page's Origin.
async fn window(d: &Daemon) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue};
    let mut req = d.url().into_client_request().unwrap();
    req.headers_mut().insert("Origin", HeaderValue::from_str(&format!("http://127.0.0.1:{}", d.port)).unwrap());
    tokio_tungstenite::connect_async(req).await.unwrap().0
}

/// Send one op on a raw socket and read the answer that carries its `ref`.
async fn on_socket(ws: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, msg: Value) -> Value {
    use futures_util::{SinkExt, StreamExt};
    let r = msg["ref"].clone();
    ws.send(tokio_tungstenite::tungstenite::Message::Text(msg.to_string())).await.unwrap();
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let next = tokio::time::timeout_at(deadline, ws.next()).await.expect("an answer").unwrap().unwrap();
        if let tokio_tungstenite::tungstenite::Message::Text(t) = next {
            let v: Value = serde_json::from_str(&t).unwrap();
            if v["ref"] == r {
                return v;
            }
        }
    }
}

#[tokio::test]
async fn a_window_cannot_send_a_hand_over_to_a_host_through_the_hub() {
    let remote_sb = Sandbox::new("agents-window-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-window-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;
    let far = save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": host })).await.unwrap();
    let own = far["id"].as_str().unwrap().split_once('~').unwrap().1.to_string();
    let mut w = window(&hub).await;
    // Any id that names the host would route the op there; none gets through.
    let sid = format!("{host}~s-TEST");
    let record = json!({ "id": "g-TEST-forged", "name": "Forged", "engine": "codex", "createdAt": 1, "updatedAt": 1 });
    for (n, msg) in [
        json!({ "op": "agent.export", "id": own, "sessionId": sid }),
        json!({ "op": "agent.release", "id": own, "sessionId": sid }),
        json!({ "op": "agent.settled", "id": own, "sessionId": sid }),
        json!({ "op": "agent.adopt", "agent": record, "sessionId": sid }),
        json!({ "op": "agent.deliver", "agentId": own, "text": "TEST-forged", "origin": { "kind": "agent", "agentId": "gx", "name": "Forged" }, "sessionId": sid }),
        json!({ "op": "agent.hosted.leave", "id": own, "sessionId": sid }),
        json!({ "op": "agent.export", "id": own, "agentId": far["id"] }),
    ]
    .into_iter()
    .enumerate()
    {
        let mut msg = msg;
        msg["ref"] = json!(format!("w{n}"));
        let answer = on_socket(&mut w, msg.clone()).await;
        assert_eq!(answer["op"], "error", "{msg} got {answer}");
        assert!(answer["message"].as_str().unwrap().contains("only another Switchyard"), "{answer}");
    }
    // The host still owns it, and nothing ran.
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == own.as_str()));
    assert!(!saved_agents(&remote_sb).iter().any(|a| a["id"] == "g-TEST-forged"));
    assert!(texts(&remote_sb).is_empty());
}

#[tokio::test]
async fn a_hand_over_waits_for_the_turn_running_on_its_host_and_leaves_no_trace_meanwhile() {
    let remote_sb = Sandbox::new("agents-busy-remote");
    let remote = remote_sb.start();
    let mut r = remote.client().await;
    ready(&mut r).await;
    let hub_id = "0123456789abcdef0123456789abcdef";
    save_hosted(&mut r, json!({ "id": "g-oncall", "name": "On-call", "handle": "On-call", "engine": "codex", "model": MODEL, "effort": EFFORT, "createdAt": 1, "updatedAt": 1, "hub": hub_id })).await;
    let chat = ask_hosted(&mut r, "g-oncall", "[slow] TEST-a long answer").await;
    r.wait_event(&chat, "assistant_delta", 0).await;

    let host = "hTEST4";
    let hub_sb = Sandbox::new("agents-busy-hub");
    seed_old_hub(&hub_sb, &remote, host, &chat);
    let hub = hub_sb.start_with(&[("YARD_SLACK_API", "http://127.0.0.1:9/api/"), ("YARD_SETTLE_MS", "200")]);
    let mut c = hub.client().await;
    c.wait_for("the host can own agents", |m| (m["op"] == "hosts" || m["op"] == "snapshot") && m["hosts"][0]["ownAgents"] == true).await;
    c.drain_for(std::time::Duration::from_millis(2000)).await;
    // Mid-turn nothing moves: its answer goes back to the Mac that asked.
    assert!(saved_agents(&hub_sb).iter().any(|a| a["id"] == "g-oncall"));
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == "g-oncall" && a["hosted"] == true));
    // Refused tries change nothing, so they leave no backup on either side.
    for (sb, name) in [(&hub_sb, "agents.json"), (&hub_sb, "slack.json"), (&remote_sb, "agents.json"), (&remote_sb, "slack.json")] {
        assert_eq!(backups(sb, name), 0, "{name}");
    }
    // Once the turn ends, it moves.
    let seq = r.next_seq(&chat);
    r.send(json!({ "op": "session.interrupt", "sessionId": chat })).await;
    r.wait_turn_done(&chat, seq.saturating_sub(1)).await;
    eventually("the host owns it", || saved_agents(&remote_sb).iter().any(|a| a["id"] == "g-oncall" && a["hosted"].is_null())).await;
    eventually("the Mac let go", || saved_agents(&hub_sb).is_empty()).await;
}

#[tokio::test]
async fn a_hidden_copy_another_mac_runs_is_left_alone_and_its_page_says_why() {
    let remote_sb = Sandbox::new("agents-theirs-remote");
    let remote = remote_sb.start();
    let mut r = remote.client().await;
    ready(&mut r).await;
    // Another Mac put a copy of this id there.
    save_hosted(&mut r, json!({ "id": "g-oncall", "name": "On-call", "engine": "codex", "model": MODEL, "createdAt": 1, "updatedAt": 1, "hub": "fedcba9876543210fedcba9876543210" })).await;
    let chat = ask_hosted(&mut r, "g-oncall", "TEST-the other Mac's").await;
    r.wait_turn_done(&chat, 0).await;
    let host = "hTEST6";
    let hub_sb = Sandbox::new("agents-theirs-hub");
    seed_old_hub(&hub_sb, &remote, host, &chat);
    let hub = hub_sb.start_with(&[("YARD_SLACK_API", "http://127.0.0.1:9/api/"), ("YARD_SETTLE_MS", "200")]);
    let mut c = hub.client().await;
    let why = c.wait_for("the refusal on its page", |m| (m["op"] == "agents" || m["op"] == "snapshot") && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == "g-oncall" && a["handOverError"].is_string())).await;
    let shown = why["agents"].as_array().unwrap().iter().find(|a| a["id"] == "g-oncall").unwrap().clone();
    assert!(shown["handOverError"].as_str().unwrap().contains("for another Mac"), "{shown}");
    c.drain_for(std::time::Duration::from_millis(1500)).await;
    // Nothing changed or was copied on either side, and it is not tried again and again.
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == "g-oncall" && a["hosted"] == true));
    assert!(saved_agents(&hub_sb).iter().any(|a| a["id"] == "g-oncall"));
    for (sb, name) in [(&hub_sb, "agents.json"), (&hub_sb, "slack.json"), (&remote_sb, "agents.json")] {
        assert_eq!(backups(sb, name), 0, "{name}");
    }
    let tries = remote.log_text().matches("client connected").count();
    c.drain_for(std::time::Duration::from_millis(1000)).await;
    assert_eq!(remote.log_text().matches("client connected").count(), tries, "tried again while refused for good");
}

#[tokio::test]
async fn sending_an_agent_again_is_the_same_hand_over_or_refused() {
    let sb = Sandbox::new("agents-adopt-again");
    let d = sb.start();
    let mut c = d.client().await;
    ready(&mut c).await;
    let mac = "0123456789abcdef0123456789abcdef";
    let other = "fedcba9876543210fedcba9876543210";
    let record = |memory: &str, cwd: &str, at: u64| json!({ "id": "g-far", "name": "Far", "engine": "codex", "model": MODEL, "memory": memory, "cwd": cwd, "permission": "ask", "createdAt": 1, "updatedAt": at });
    let adopt = |agent: Value, hub: &str, transfer: &str, r: &str| json!({ "op": "agent.adopt", "agent": agent, "slack": null, "hub": hub, "transfer": transfer, "ref": r });
    let first = c.mark();
    c.send(adopt(record("TEST-first", "", 10), mac, "t1", "a1")).await;
    assert_eq!(c.wait_after(first, "adopted", |m| m["ref"] == "a1").await["op"], "agent.adopted");
    let written = backups(&sb, "agents.json");
    // The same hand-over again (its answer was lost): nothing changes.
    let from = c.mark();
    c.send(adopt(record("TEST-other words", "", 20), mac, "t1", "a2")).await;
    let again = c.wait_after(from, "adopted again", |m| m["ref"] == "a2").await;
    assert_eq!((again["op"].as_str(), again["agent"]["memory"].as_str()), (Some("agent.adopted"), Some("TEST-first")), "{again}");
    // Another hand-over of the same id, even saying the same and newer, is refused.
    let folder = sb.folder("TEST-elsewhere");
    for (n, (hub, transfer)) in [(mac, "t2"), (other, "t3")].into_iter().enumerate() {
        let from = c.mark();
        let r = format!("b{n}");
        c.send(adopt(record("TEST-first", &folder, 30), hub, transfer, &r)).await;
        let refused = c.wait_after(from, "refused", |m| m["ref"] == r.as_str()).await;
        assert_eq!(refused["op"], "error", "{refused}");
        assert!(refused["message"].as_str().unwrap().contains("already has an agent of its own"), "{refused}");
    }
    let kept = saved_agents(&sb).into_iter().find(|a| a["id"] == "g-far").unwrap();
    assert_eq!((kept["memory"].as_str(), kept["cwd"].as_str()), (Some("TEST-first"), Some("")));
    // A hidden copy another hub put here is that hub's.
    save_hosted(&mut c, json!({ "id": "g-kept", "name": "Kept", "engine": "codex", "model": MODEL, "createdAt": 1, "updatedAt": 1, "hub": mac })).await;
    let from = c.mark();
    c.send(adopt(json!({ "id": "g-kept", "name": "Kept", "engine": "codex", "createdAt": 1, "updatedAt": 2 }), other, "t4", "a5")).await;
    assert_eq!(c.wait_after(from, "refused", |m| m["ref"] == "a5").await["op"], "error");
    assert!(saved_agents(&sb).iter().any(|a| a["id"] == "g-kept" && a["hosted"] == true));
    // Nothing that was sent again or refused wrote, or left a backup.
    assert_eq!(backups(&sb, "agents.json"), written);
}

#[tokio::test]
async fn a_hand_over_that_cannot_be_saved_says_so_and_keeps_nothing() {
    let sb = Sandbox::new("agents-adopt-unsaved");
    let d = sb.start();
    let mut c = d.client().await;
    ready(&mut c).await;
    // agents.json cannot be written (its temporary file's place is taken).
    std::fs::create_dir_all(sb.data().join("agents.json.tmp")).unwrap();
    let from = c.mark();
    c.send(json!({ "op": "agent.adopt", "agent": { "id": "g-far", "name": "Far", "engine": "codex", "createdAt": 1, "updatedAt": 1 }, "hub": "0123456789abcdef0123456789abcdef", "transfer": "t1", "ref": "a" })).await;
    let answer = c.wait_after(from, "the answer", |m| m["ref"] == "a").await;
    assert_eq!(answer["op"], "error", "{answer}");
    let from = c.mark();
    c.send(json!({ "op": "agent.list", "ref": "l" })).await;
    let listed = c.wait_after(from, "the list", |m| m["ref"] == "l").await;
    assert!(!listed["agents"].as_array().unwrap().iter().any(|a| a["id"] == "g-far"), "{listed}");
}

#[tokio::test]
async fn an_owed_release_from_another_hand_over_never_drops_an_agent() {
    let remote_sb = Sandbox::new("agents-release-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-release-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;
    let far = save(&mut c, json!({ "name": "Far", "engine": "codex", "model": MODEL, "host": host })).await.unwrap();
    let own = far["id"].as_str().unwrap().split_once('~').unwrap().1.to_string();
    drop(c);
    hub.stop();
    // A release owed by a hand-over that is not the one that put it there.
    std::fs::write(hub_sb.data().join("agents-pending.json"), json!([{ "host": host, "op": "agent.release", "id": own, "transfer": "TEST-some-other-move" }]).to_string()).unwrap();
    let hub = hub_sb.start_with(&[("YARD_SETTLE_MS", "200")]);
    let mut c = hub.client().await;
    eventually("the step done", || std::fs::read_to_string(hub_sb.data().join("agents-pending.json")).is_ok_and(|t| t.trim() == "[]")).await;
    c.drain_for(std::time::Duration::from_millis(300)).await;
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == own.as_str()), "the host kept its agent");
}

#[tokio::test]
async fn an_agent_moved_back_before_its_old_host_let_go_is_never_lost() {
    let remote_sb = Sandbox::new("agents-bounce-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-bounce-hub");
    let hub = hub_sb.start_with(&[("YARD_SETTLE_MS", "600000")]);
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;
    let made = save(&mut c, json!({ "name": "Bouncer", "engine": "codex", "model": MODEL, "memory": "TEST-bounce" })).await.unwrap();
    let id = made["id"].as_str().unwrap().to_string();
    // Here to the box.
    save(&mut c, json!({ "id": id, "host": host })).await.unwrap();
    assert!(saved_agents(&remote_sb).iter().any(|a| a["id"] == id.as_str()));
    // Back to the hub, as a move whose release the box never answered leaves
    // it: the box's copy marked as leaving, the hub's own, the release owed.
    let mut r = remote.client().await;
    let from = r.mark();
    r.send(json!({ "op": "agent.export", "id": id, "transfer": "TEST-back", "ref": "e" })).await;
    let out = r.wait_after(from, "exported", |m| m["ref"] == "e").await;
    assert_eq!(out["op"], "agent.exported", "{out}");
    drop(r);
    drop(c);
    hub.stop();
    let mut back = saved_agents(&remote_sb).into_iter().find(|a| a["id"] == id.as_str()).unwrap();
    back["transfer"] = json!("TEST-back");
    back["leaving"] = json!("");
    back["host"] = json!("local");
    std::fs::write(hub_sb.data().join("agents.json"), json!([back]).to_string()).unwrap();
    std::fs::write(hub_sb.data().join("agents-pending.json"), json!([{ "host": host, "op": "agent.release", "id": id, "transfer": "TEST-back" }]).to_string()).unwrap();
    let hub = hub_sb.start_with(&[("YARD_SETTLE_MS", "600000")]);
    let mut c = hub.client().await;
    ready(&mut c).await;
    c.wait_for("the box connected", |m| (m["op"] == "hosts" || m["op"] == "snapshot") && m["hosts"][0]["ownAgents"] == true).await;
    // Straight back to the box, before the owed release ran.
    let _ = save(&mut c, json!({ "id": id, "host": host })).await;
    drop(c);
    hub.stop();
    // The owed release and the move both finish, in whatever order: one copy, on the box.
    let hub = hub_sb.start_with(&[("YARD_SETTLE_MS", "200")]);
    let _c = hub.client().await;
    eventually("the box owns it", || saved_agents(&remote_sb).iter().any(|a| a["id"] == id.as_str() && a["leaving"].is_null()) && saved_agents(&hub_sb).is_empty()).await;
    eventually("nothing owed", || std::fs::read_to_string(hub_sb.data().join("agents-pending.json")).is_ok_and(|t| t.trim() == "[]")).await;
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    let there = saved_agents(&remote_sb).into_iter().find(|a| a["id"] == id.as_str()).expect("the agent is still on the box");
    assert_eq!(there["memory"], "TEST-bounce");
}

#[tokio::test]
async fn a_host_does_not_let_go_mid_turn() {
    let sb = Sandbox::new("agents-release-busy");
    let d = sb.start();
    let mut c = d.client().await;
    ready(&mut c).await;
    let op = |m: Value| m;
    let from = c.mark();
    c.send(op(json!({ "op": "agent.adopt", "agent": { "id": "g-busy", "name": "Busy", "engine": "codex", "model": MODEL, "effort": EFFORT, "createdAt": 1, "updatedAt": 1 }, "hub": "0123456789abcdef0123456789abcdef", "transfer": "t1", "ref": "a" }))).await;
    assert_eq!(c.wait_after(from, "adopted", |m| m["ref"] == "a").await["op"], "agent.adopted");
    let from = c.mark();
    c.send(json!({ "op": "agent.export", "id": "g-busy", "transfer": "t2", "ref": "e" })).await;
    assert_eq!(c.wait_after(from, "exported", |m| m["ref"] == "e").await["op"], "agent.exported");
    // A Slack message, say, starts a turn before the release arrives.
    let sid = ask(&mut c, "g-busy", "[slow] TEST-still answering", json!({})).await.unwrap();
    c.wait_event(&sid, "assistant_delta", 0).await;
    let from = c.mark();
    c.send(json!({ "op": "agent.release", "id": "g-busy", "transfer": "t2", "ref": "r1" })).await;
    let refused = c.wait_after(from, "refused", |m| m["ref"] == "r1").await;
    assert_eq!(refused["op"], "error", "{refused}");
    assert!(saved_agents(&sb).iter().any(|a| a["id"] == "g-busy"));
    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.wait_turn_done(&sid, seq.saturating_sub(1)).await;
    c.wait_status(&sid, "idle").await;
    let from = c.mark();
    c.send(json!({ "op": "agent.release", "id": "g-busy", "transfer": "t2", "ref": "r2" })).await;
    assert_eq!(c.wait_after(from, "released", |m| m["ref"] == "r2").await["op"], "agent.released");
    assert!(!saved_agents(&sb).iter().any(|a| a["id"] == "g-busy"));
}

#[tokio::test]
async fn a_message_to_a_kept_record_its_host_took_reaches_the_host_s_own() {
    let remote_sb = Sandbox::new("agents-window-owner-remote");
    let remote = remote_sb.start();
    let mut r = remote.client().await;
    ready(&mut r).await;
    // The host took the agent by this hub's hand-over; the hub has yet to let go.
    let hub_id = "0123456789abcdef0123456789abcdef";
    let from = r.mark();
    r.send(json!({ "op": "agent.adopt", "agent": { "id": "g-mid", "name": "Mid", "engine": "codex", "model": MODEL, "effort": EFFORT, "createdAt": 1, "updatedAt": 5 }, "hub": hub_id, "transfer": format!("{hub_id}-g-mid-5"), "ref": "a" })).await;
    assert_eq!(r.wait_after(from, "adopted", |m| m["ref"] == "a").await["op"], "agent.adopted");
    drop(r);
    let host = "hTEST7";
    let hub_sb = Sandbox::new("agents-window-owner-hub");
    std::fs::write(hub_sb.data().join("hosts.json"), json!([{ "id": host, "name": "TEST-box", "kind": "direct", "url": remote.url() }]).to_string()).unwrap();
    std::fs::write(hub_sb.data().join("hub-id"), hub_id).unwrap();
    std::fs::write(hub_sb.data().join("agents.json"), json!([{ "id": "g-mid", "name": "Mid", "engine": "codex", "model": MODEL, "effort": EFFORT, "host": host, "createdAt": 1, "updatedAt": 5 }]).to_string()).unwrap();
    // The hub cannot let go yet (its file cannot be written).
    std::fs::create_dir_all(hub_sb.data().join("agents.json.tmp")).unwrap();
    let hub = hub_sb.start_with(&[("YARD_SETTLE_MS", "600000")]);
    let mut c = hub.client().await;
    ready(&mut c).await;
    c.wait_for("the host's agent listed", |m| (m["op"] == "agents" || m["op"] == "snapshot") && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == format!("{host}~g-mid").as_str())).await;
    let sid = ask(&mut c, "g-mid", "TEST-who answers?", json!({})).await.expect("the host's own agent answers");
    assert!(sid.starts_with(&format!("{host}~")), "{sid}");
    c.wait_turn_done(&sid, 0).await;
    assert_eq!(texts(&remote_sb).last().unwrap().contains("TEST-who answers?"), true);
}

#[tokio::test]
async fn fields_this_build_does_not_know_survive_saves_and_hand_overs() {
    let remote_sb = Sandbox::new("agents-extra-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("agents-extra-hub");
    // An agent a newer yardd wrote: fields this build does not know.
    let record = json!([{ "id": "g-tooled", "name": "Tooled", "handle": "Tooled", "engine": "codex", "model": MODEL, "effort": EFFORT, "createdAt": 1, "updatedAt": 1, "skills": ["TEST-github"], "mcp": ["TEST-metabase"], "future": { "x": 1 } }]);
    std::fs::write(hub_sb.data().join("agents.json"), record.to_string()).unwrap();
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;
    let kept = |sb: &Sandbox| saved_agents(sb).into_iter().find(|a| a["id"] == "g-tooled").expect("the agent");

    // An edit of other fields keeps them, and the list shows them as they are.
    save(&mut c, json!({ "id": "g-tooled", "memory": "TEST-edited" })).await.unwrap();
    let here = kept(&hub_sb);
    assert_eq!((here["skills"].clone(), here["future"].clone(), here["memory"].clone()), (json!(["TEST-github"]), json!({ "x": 1 }), json!("TEST-edited")), "{here}");
    let from = c.mark();
    c.send(json!({ "op": "agent.list", "ref": "l" })).await;
    let listed = c.wait_after(from, "the list", |m| m["ref"] == "l").await["agents"].as_array().unwrap().iter().find(|a| a["id"] == "g-tooled").unwrap().clone();
    assert_eq!(listed["skills"], json!(["TEST-github"]));
    for stripped in ["seed", "hosted", "hub"] {
        assert!(listed.get(stripped).is_none(), "{stripped} in {listed}");
    }

    // Moved to the host and back, they travel with it.
    let far_id = format!("{host}~g-tooled");
    assert_eq!(save(&mut c, json!({ "id": "g-tooled", "host": host })).await.unwrap()["id"], far_id.as_str());
    let there = kept(&remote_sb);
    assert_eq!((there["skills"].clone(), there["future"].clone()), (json!(["TEST-github"]), json!({ "x": 1 })), "{there}");
    // Its connectors (a field this build knows) travel too.
    assert_eq!(there["mcp"], json!(["TEST-metabase"]), "{there}");
    let shown = c.wait_for("the host's agent listed", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == far_id.as_str())).await;
    let far = shown["agents"].as_array().unwrap().iter().find(|a| a["id"] == far_id.as_str()).unwrap().clone();
    assert_eq!(far["skills"], json!(["TEST-github"]));
    for stripped in ["seed", "hosted", "hub"] {
        assert!(far.get(stripped).is_none(), "{stripped} in {far}");
    }
    // Edited there through the hub, then back here.
    save(&mut c, json!({ "id": far_id, "about": "TEST-there" })).await.unwrap();
    assert_eq!(kept(&remote_sb)["future"], json!({ "x": 1 }));
    save(&mut c, json!({ "id": far_id, "host": "local" })).await.unwrap();
    let back = kept(&hub_sb);
    assert_eq!((back["skills"].clone(), back["future"].clone(), back["about"].clone()), (json!(["TEST-github"]), json!({ "x": 1 }), json!("TEST-there")), "{back}");
}
