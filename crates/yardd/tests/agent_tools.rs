//! Agents' own tools, end to end: the built daemon hands an agent's
//! conversation its tools server (`yardd agent-tools`), and the scripted
//! engines in `tests/fake-codex` call it for real over stdio when a prompt
//! carries a `[[tool NAME {json}]]` marker (see agent-tools.mjs). Covers
//! reaching the owner, messaging another agent and getting its answer back,
//! the hop limit, refusals, failure replies, memory edits, plain sessions
//! without tools, and the loopback endpoint's key check.
mod common;

use std::{
    io::{Read, Write},
    time::Duration,
};

use common::*;
use serde_json::{json, Value};

/// A daemon whose Claude sessions run the scripted sidecar.
fn start(sb: &Sandbox) -> Daemon {
    sb.start_with(&[("YARD_SIDECAR", fake_bin().join("yard-claude").to_str().unwrap())])
}

async fn ready(c: &mut Client) {
    c.wait_for("models ready", |m| {
        (m["op"] == "models" || m["op"] == "snapshot") && m["models"]["codex"].as_array().is_some_and(|a| !a.is_empty())
    })
    .await;
}

async fn ask(c: &mut Client, agent: &str, text: &str) -> String {
    let from = c.mark();
    let r = format!("ask-{from}");
    c.send(json!({ "op": "agent.ask", "agentId": agent, "text": text, "ref": r })).await;
    let reply = c.wait_after(from, "agent.ask reply", |m| m["ref"] == r.as_str() && (m["op"] == "created" || m["op"] == "error")).await;
    assert_eq!(reply["op"], "created", "{reply}");
    reply["sessionId"].as_str().unwrap().to_string()
}

/// A user-made agent; answers its id.
async fn agent(c: &mut Client, name: &str, engine: &str, model: &str) -> String {
    let from = c.mark();
    let r = format!("save-{from}");
    c.send(json!({ "op": "agent.save", "agent": { "name": name, "engine": engine, "model": model }, "ref": r })).await;
    let reply = c.wait_after(from, "agent.save reply", |m| m["ref"] == r.as_str()).await;
    assert_eq!(reply["op"], "agent.saved", "{reply}");
    reply["agent"]["id"].as_str().unwrap().to_string()
}

async fn codex_agent(c: &mut Client, name: &str) -> String {
    agent(c, name, "codex", MODEL).await
}

/// The agent's conversation, once yardd has made it.
async fn chat_of(c: &mut Client, id: &str) -> String {
    let id = id.to_string();
    let m = c
        .wait_for("the agent's conversation", |m| {
            (m["op"] == "agents" || m["op"] == "snapshot") && m["agents"].as_array().is_some_and(|l| l.iter().any(|a| a["id"] == id.as_str() && a["chat"].is_string()))
        })
        .await;
    m["agents"].as_array().unwrap().iter().find(|a| a["id"] == id.as_str()).unwrap()["chat"].as_str().unwrap().to_string()
}

fn marker(tool: &str, args: Value) -> String {
    format!("[[tool {tool} {args}]]")
}

/// Every call the fake engines made through the tools server, with its result.
fn calls(sb: &Sandbox) -> Vec<Value> {
    std::fs::read_to_string(sb.fake_state().join("agent-tools.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn results(sb: &Sandbox, tool: &str) -> Vec<Value> {
    calls(sb).into_iter().filter(|r| r["tool"] == tool).collect()
}

fn thread_starts(sb: &Sandbox) -> Vec<Value> {
    std::fs::read_to_string(sb.fake_state().join("thread-starts.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// The first `user_message` of a conversation that came from `kind`.
async fn from_kind(c: &mut Client, sid: &str, kind: &str) -> Value {
    let what = format!("a `{kind}` message in {sid}");
    let m = c.wait_for(&what, |m| is_event(m, sid) && m["event"]["type"] == "user_message" && m["event"]["origin"]["kind"] == kind).await;
    let mut ev = m["event"].clone();
    ev["seq"] = m["seq"].clone();
    ev
}

#[tokio::test]
async fn an_agent_reaches_the_owner_a_few_times_an_hour_and_is_marked_until_seen() {
    let sb = Sandbox::new("tools-notify");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = codex_agent(&mut c, "Ada").await;
    let note = |i: usize| marker("notify_owner", json!({ "message": format!("Need the deploy key {i}") }));
    let sid = ask(&mut c, &ada, &(1..=4).map(note).collect::<Vec<_>>().join(" ")).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");

    // Three reach every window, opening the conversation; the fourth is refused.
    let notes: Vec<Value> = c.log.iter().filter(|m| m["op"] == "notify").cloned().collect();
    assert_eq!(notes.len(), 3, "{notes:?}");
    assert_eq!(notes[0]["title"], "Ada needs you");
    assert_eq!(notes[0]["body"], "Need the deploy key 1");
    assert_eq!(notes[0]["sessionId"], sid.as_str());
    let r = results(&sb, "notify_owner");
    assert_eq!(r.iter().map(|r| r["isError"] == true).collect::<Vec<_>>(), [false, false, false, true]);
    assert!(r[3]["text"].as_str().unwrap().contains("3 times in the last hour"), "{}", r[3]);

    // The thread shows the calls as steps; the card keeps the call for the owner.
    let events = c.events(&sid);
    let steps: Vec<&Value> = events.iter().filter(|e| e["type"] == "tool_started").collect();
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[0]["title"], "Asked for you");
    assert_eq!(steps[0]["detail"], "Need the deploy key 1");
    // The latest call is the one shown.
    c.wait_for("needs you", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["needsYou"]["message"] == "Need the deploy key 3").await;

    // Opening the conversation clears it, for every window.
    let from = c.mark();
    c.send(json!({ "op": "agent.seen", "agentId": ada })).await;
    c.wait_after(from, "needs you cleared", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["needsYou"].is_null()).await;
}

#[tokio::test]
async fn a_message_to_another_agent_comes_back_as_its_answer() {
    let sb = Sandbox::new("tools-message");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = codex_agent(&mut c, "Ada").await;
    let bea = codex_agent(&mut c, "Bea").await;
    let ada_chat = ask(&mut c, &ada, &marker("message_agent", json!({ "to": "@bea", "text": "what is 2+2?" }))).await;
    c.wait_turn_done(&ada_chat, 0).await;
    let sent = &results(&sb, "message_agent")[0];
    assert_eq!(sent["isError"], false, "{sent}");
    assert!(sent["text"].as_str().unwrap().starts_with("Sent to @Bea (hop 1 of 3)."), "{sent}");
    // The tool's description names the agents there are.
    let listed = calls(&sb).into_iter().find(|r| r["listed"].is_array()).unwrap();
    assert_eq!(listed["listed"], json!(["notify_owner", "message_agent", "list_agents", "remember", "set_memory", "update_setup", "slack_reply", "request_connector"]));
    assert!(listed["messageAgent"].as_str().unwrap().contains("Agents: @Bea."), "{listed}");
    assert_eq!(c.events(&ada_chat).iter().find(|e| e["type"] == "tool_started").unwrap()["title"], "Messaged @bea");

    // Bea's one conversation gets it, tagged as Ada's; her engine reads who it is from.
    let bea_chat = chat_of(&mut c, &bea).await;
    let got = from_kind(&mut c, &bea_chat, "agent").await;
    assert_eq!(got["text"], "what is 2+2?");
    assert_eq!(got["origin"], json!({ "kind": "agent", "agentId": ada, "name": "Ada", "handle": "Ada", "hops": 1 }));
    c.wait_turn_done(&bea_chat, 0).await;
    let inputs: Vec<String> = sb.turn_inputs().iter().map(|i| i[0]["text"].as_str().unwrap_or("").to_string()).collect();
    assert!(inputs.iter().any(|t| t.contains("<message from=\"@Ada\">\nwhat is 2+2?\n</message>")), "{inputs:?}");

    // Her answer comes back into Ada's conversation as a new message, which starts Ada's turn.
    let back = from_kind(&mut c, &ada_chat, "agentReply").await;
    assert_eq!(back["origin"], json!({ "kind": "agentReply", "agentId": bea, "name": "Bea", "handle": "Bea", "hops": 1 }));
    assert!(back["text"].as_str().unwrap().starts_with("Echo: <message from=\"@Ada\">"), "{back}");
    c.wait_turn_done(&ada_chat, back["seq"].as_u64().unwrap()).await;
    let inputs: Vec<String> = sb.turn_inputs().iter().map(|i| i[0]["text"].as_str().unwrap_or("").to_string()).collect();
    assert!(inputs.iter().any(|t| t.starts_with("<reply from=\"@Bea\">")), "{inputs:?}");
    // A reply is not answered back: Bea had one turn.
    c.drain_for(Duration::from_millis(800)).await;
    assert_eq!(c.events(&bea_chat).iter().filter(|e| e["type"] == "turn_started").count(), 1);

    // The origins are kept with the messages.
    drop(c);
    daemon.stop();
    let daemon = sb.start();
    let mut c = daemon.client().await;
    let history = c.history(&bea_chat).await;
    assert_eq!(history[0]["origin"]["kind"], "agent");
    let history = c.history(&ada_chat).await;
    assert!(history.iter().any(|e| e["origin"]["kind"] == "agentReply" && e["origin"]["name"] == "Bea"));
}

#[tokio::test]
async fn an_agent_with_a_spaced_name_is_messaged_by_its_handle_or_its_name() {
    let sb = Sandbox::new("tools-handles");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = codex_agent(&mut c, "Ada").await;
    let triage = codex_agent(&mut c, "Custom Domains Triage").await;
    let text = [
        marker("list_agents", json!({})),
        marker("message_agent", json!({ "to": "@custom-domains-triage", "text": "TEST-by handle" })),
        marker("message_agent", json!({ "to": "Custom Domains Triage", "text": "TEST-by name" })),
        marker("message_agent", json!({ "to": "Custom", "text": "TEST-nobody" })),
    ]
    .join(" ");
    let ada_chat = ask(&mut c, &ada, &text).await;
    c.wait_turn_done(&ada_chat, 0).await;
    let listed = &results(&sb, "list_agents")[0];
    assert_eq!(listed["text"], "@Custom-Domains-Triage (Custom Domains Triage)", "{listed}");
    let sent = results(&sb, "message_agent");
    assert!(sent[0]["text"].as_str().unwrap().starts_with("Sent to @Custom-Domains-Triage (hop 1 of 3)."), "{}", sent[0]);
    assert!(sent[1]["text"].as_str().unwrap().starts_with("Sent to @Custom-Domains-Triage"), "{}", sent[1]);
    assert_eq!(sent[2]["isError"], true);
    assert!(sent[2]["text"].as_str().unwrap().contains("Agents: @Custom-Domains-Triage (Custom Domains Triage)."), "{}", sent[2]);
    let described = calls(&sb).into_iter().find(|r| r["listed"].is_array()).unwrap();
    assert!(described["messageAgent"].as_str().unwrap().contains("Agents: @Custom-Domains-Triage (Custom Domains Triage)."), "{described}");

    // Both land in its one conversation; its engine reads the sender by handle and its own name as it reads.
    let chat = chat_of(&mut c, &triage).await;
    let got = from_kind(&mut c, &chat, "agent").await;
    assert_eq!(got["origin"]["handle"], "Ada");
    c.wait_turn_done(&chat, 0).await;
    let inputs: Vec<String> = sb.turn_inputs().iter().map(|i| i[0]["text"].as_str().unwrap_or("").to_string()).collect();
    assert!(inputs.iter().any(|t| t.starts_with("You are Custom Domains Triage (@Custom-Domains-Triage),")), "{inputs:?}");
    // Its answer goes back to Ada badged with its handle and name.
    let back = from_kind(&mut c, &ada_chat, "agentReply").await;
    assert_eq!((back["origin"]["name"].as_str(), back["origin"]["handle"].as_str()), (Some("Custom Domains Triage"), Some("Custom-Domains-Triage")));
}

#[tokio::test]
async fn chains_stop_at_three_hops_and_an_agent_cannot_message_itself() {
    let sb = Sandbox::new("tools-hops");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = codex_agent(&mut c, "Ada").await;
    for name in ["Bea", "Cy", "Dot"] {
        codex_agent(&mut c, name).await;
    }
    // Ada -> Bea -> Cy -> Dot, and Dot's message back to Ada would be the fourth hop.
    let to = |who: &str, text: String| marker("message_agent", json!({ "to": who, "text": text }));
    let chain = to("Bea", to("Cy", to("Dot", to("Ada", "loop".into()))));
    let self_note = to("ada", "hi me".into());
    let ada_chat = ask(&mut c, &ada, &format!("{chain} {self_note}")).await;

    let deadline = std::time::Instant::now() + WAIT;
    let refused = loop {
        if let Some(r) = results(&sb, "message_agent").into_iter().find(|r| r["args"]["to"] == "Ada") {
            break r;
        }
        assert!(std::time::Instant::now() < deadline, "Dot never tried: {:?}", calls(&sb));
        c.drain_for(Duration::from_millis(200)).await;
    };
    assert_eq!(refused["isError"], true);
    assert!(refused["text"].as_str().unwrap().contains("hop 4 of a chain"), "{refused}");
    let to_self = results(&sb, "message_agent").into_iter().find(|r| r["args"]["to"] == "ada").unwrap();
    assert_eq!(to_self["isError"], true);
    assert!(to_self["text"].as_str().unwrap().contains("that is you"), "{to_self}");
    let hops: Vec<u64> = results(&sb, "message_agent").iter().filter(|r| r["isError"] == false).map(|r| r["text"].as_str().unwrap()[r["text"].as_str().unwrap().find("(hop ").unwrap() + 5..].chars().next().unwrap().to_digit(10).unwrap() as u64).collect();
    assert_eq!(hops, [1, 2, 3]);
    // Ada's own conversation never got a message from Dot.
    c.drain_for(Duration::from_millis(800)).await;
    assert!(!c.events(&ada_chat).iter().any(|e| e["origin"]["kind"] == "agent"));
}

#[tokio::test]
async fn a_failed_or_stopped_answer_still_reaches_the_sender() {
    let sb = Sandbox::new("tools-fail");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = codex_agent(&mut c, "Ada").await;
    let bea = codex_agent(&mut c, "Bea").await;
    let ada_chat = ask(&mut c, &ada, &marker("message_agent", json!({ "to": "Bea", "text": "[fail] please" }))).await;
    let back = from_kind(&mut c, &ada_chat, "agentReply").await;
    assert!(back["text"].as_str().unwrap().starts_with("Bea's turn failed before it answered: fake failure"), "{back}");

    // A message still queued when Bea is stopped is not the owner's to edit:
    // Ada hears that Bea stopped before reading it.
    let bea_chat = chat_of(&mut c, &bea).await;
    let seq = c.next_seq(&bea_chat);
    ask(&mut c, &bea, "[slow] take your time").await;
    c.wait_event(&bea_chat, "turn_started", seq).await;
    let seq = c.next_seq(&ada_chat);
    ask(&mut c, &ada, &marker("message_agent", json!({ "to": "Bea", "text": "are you there?" }))).await;
    c.wait_for("Ada's message queued for Bea", |m| {
        m["op"] == "session" && m["session"]["id"] == bea_chat.as_str() && m["session"]["pending"].as_array().is_some_and(|p| p.iter().any(|i| i["origin"]["name"] == "Ada"))
    })
    .await;
    let from = c.mark();
    c.send(json!({ "op": "session.interrupt", "sessionId": bea_chat })).await;
    let unread = c
        .wait_after(from, "Ada told Bea stopped", |m| {
            is_event(m, &ada_chat) && m["seq"].as_u64() >= Some(seq) && m["event"]["origin"]["kind"] == "agentReply" && m["event"]["text"].as_str().is_some_and(|t| t.contains("before it read"))
        })
        .await;
    assert_eq!(unread["event"]["text"], "Bea was stopped before it read your message.");
    // The owner's reply box gets nothing back of it.
    assert!(!c.log[from..].iter().any(|m| m["op"] == "queue.returned"));
}

#[tokio::test]
async fn a_claude_agent_edits_its_own_memory() {
    let sb = Sandbox::new("tools-memory");
    let daemon = start(&sb);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = agent(&mut c, "Ada", "claude", "claude-haiku-5-5").await;
    agent(&mut c, "Bea", "claude", "claude-haiku-5-5").await;
    let text = format!(
        "{} {} {}",
        marker("remember", json!({ "text": "The main branch is trunk." })),
        marker("remember", json!({ "text": "Deploys go out on Fridays." })),
        marker("list_agents", json!({})),
    );
    let sid = ask(&mut c, &ada, &text).await;
    c.wait_turn_done(&sid, 0).await;
    let saved = c.wait_for("memory", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == ada.as_str() && a["memory"].as_str().is_some_and(|t| t.contains("Fridays")))).await;
    let me = saved["agents"].as_array().unwrap().iter().find(|a| a["id"] == ada.as_str()).unwrap().clone();
    assert_eq!(me["memory"], "The main branch is trunk.\nDeploys go out on Fridays.");
    assert!(std::fs::read_to_string(sb.data().join("agents.json")).unwrap().contains("Deploys go out on Fridays."));
    let steps: Vec<Value> = c.events(&sid).into_iter().filter(|e| e["type"] == "tool_started").collect();
    assert_eq!(steps[0]["title"], "Remembered");
    assert_eq!(steps[0]["detail"], "The main branch is trunk.");
    assert_eq!(steps[2]["title"], "Listed agents");
    let listed = results(&sb, "list_agents")[0]["text"].as_str().unwrap().to_string();
    assert!(listed.contains("@Bea") && !listed.contains("@Ada"), "{listed}");
    assert!(std::fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap().contains("tools=switchyard"));

    // set_memory replaces it; past the cap it is refused and nothing changes.
    let seq = c.next_seq(&sid);
    let big = "x".repeat(20_001);
    ask(&mut c, &ada, &format!("{} {}", marker("set_memory", json!({ "text": "Only this." })), marker("set_memory", json!({ "text": big })))).await;
    c.wait_turn_done(&sid, seq).await;
    let r = results(&sb, "set_memory");
    assert_eq!(r[0]["isError"], false);
    assert_eq!(r[1]["isError"], true);
    assert!(r[1]["text"].as_str().unwrap().contains("at most 20,000 characters"));
    c.wait_for("memory replaced", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == ada.as_str() && a["memory"] == "Only this.")).await;
}

#[tokio::test]
async fn an_opencode_agent_calls_its_tools_without_asking() {
    let sb = Sandbox::new("tools-acp");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    c.wait_for("opencode models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["opencode"].as_array().is_some_and(|l| !l.is_empty())).await;
    let ada = agent(&mut c, "Ada", "opencode", "opencode/fake-free").await;
    let sid = ask(&mut c, &ada, &marker("remember", json!({ "text": "Use pnpm? No, bun." }))).await;
    assert_eq!(c.wait_turn_done(&sid, 0).await["status"], "completed");
    assert_eq!(results(&sb, "remember")[0]["isError"], false);
    let events = c.events(&sid);
    assert!(!events.iter().any(|e| e["type"] == "approval_request"), "our tools run unasked");
    assert_eq!(events.iter().find(|e| e["type"] == "tool_started").unwrap()["title"], "Remembered");
    let log = std::fs::read_to_string(sb.fake_state().join("acp.log")).unwrap();
    assert!(log.contains("opencode mcpServers switchyard"), "{log}");
}

#[tokio::test]
async fn the_owners_own_sessions_get_only_the_connector_tool() {
    let sb = Sandbox::new("tools-none");
    let daemon = start(&sb);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let note = marker("notify_owner", json!({ "message": "hello" }));
    // A plain Codex session has the server, with one tool: an agent's tool is not there to call.
    let plain = c.create(&sb.folder("proj"), &note, json!({})).await;
    c.wait_turn_done(&plain, 0).await;
    assert!(thread_starts(&sb)[0]["config"]["mcp_servers.switchyard"].is_object(), "{:?}", thread_starts(&sb));
    let listed = calls(&sb).into_iter().find(|r| r["listed"].is_array()).unwrap();
    assert_eq!(listed["listed"], json!(["request_connector"]));
    let refused = &results(&sb, "notify_owner")[0];
    assert!(refused["missing"] == true || refused["isError"] == true, "{refused}");
    assert!(!c.log.iter().any(|m| m["op"] == "notify"));
    // Plain Claude and OpenCode sessions get the server too.
    let claude = c.create(&sb.folder("proj2"), "hi", json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" })).await;
    c.wait_turn_done(&claude, 0).await;
    assert!(std::fs::read_to_string(sb.fake_state().join("sidecar.log")).unwrap().contains("tools="));
    c.wait_for("opencode models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["opencode"].as_array().is_some_and(|l| !l.is_empty())).await;
    let oc = c.create(&sb.folder("proj3"), "hi", json!({ "engine": "opencode", "model": "opencode/fake-free", "effort": "default" })).await;
    c.wait_turn_done(&oc, 0).await;
    assert!(std::fs::read_to_string(sb.fake_state().join("acp.log")).unwrap().contains("opencode mcpServers switchyard\n"));
}

/// The fake engines' `[[tool request_connector …]]`: the card's events, the
/// mark on the session, and what the owner's answer does.
#[tokio::test]
async fn a_session_asks_for_a_connector_and_hears_the_owners_answer() {
    let sb = Sandbox::new("tools-connector");
    let daemon = start(&sb);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ask = marker("request_connector", json!({ "service": "  Linear\n", "reason": "To read ENG-412\nand its comments." }));
    let sid = c.create(&sb.folder("proj"), &ask, json!({})).await;
    c.wait_turn_done(&sid, 0).await;
    let asked = c.events(&sid).into_iter().find(|e| e["type"] == "connector_request").expect("a connector_request event");
    // One line each, whatever the agent wrote.
    assert_eq!(asked["service"], "Linear");
    assert_eq!(asked["reason"], "To read ENG-412 and its comments.");
    let id = asked["requestId"].as_str().unwrap().to_string();
    let told = &results(&sb, "request_connector")[0];
    assert_eq!(told["isError"], false, "{told}");
    assert!(told["text"].as_str().unwrap().contains("End your turn now"), "{told}");
    let meta = c.wait_for("the mark", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["needsConnector"].is_object()).await;
    assert_eq!(meta["session"]["needsConnector"]["requestId"], id.as_str());
    assert_eq!(meta["session"]["needsConnector"]["service"], "Linear");

    // Asking again for the same service makes no second card.
    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": marker("request_connector", json!({ "service": "linear", "reason": "again" })) })).await;
    c.wait_turn_done(&sid, seq).await;
    assert_eq!(c.events(&sid).iter().filter(|e| e["type"] == "connector_request").count(), 1);
    assert!(results(&sb, "request_connector")[1]["text"].as_str().unwrap().contains("already has a card"));

    // A bad answer changes nothing.
    c.send(json!({ "op": "connector.resolve", "sessionId": sid, "requestId": id, "service": "Linear", "outcome": "maybe" })).await;
    c.wait_for("the refusal", |m| m["op"] == "error" && m["for"] == "connector.resolve").await;
    assert!(!c.events(&sid).iter().any(|e| e["type"] == "connector_resolved"));

    // Connected: the thread records it, the mark goes, and the agent's next turn starts with yardd's word.
    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "connector.resolve", "sessionId": sid, "requestId": id, "service": "Linear", "outcome": "connected" })).await;
    let word = from_kind(&mut c, &sid, "connector").await;
    assert!(word["text"].as_str().unwrap().starts_with("Linear is connected now."), "{word}");
    c.wait_turn_done(&sid, seq).await;
    let events = c.events(&sid);
    let resolved = events.iter().find(|e| e["type"] == "connector_resolved").unwrap();
    assert_eq!(resolved["requestId"], id.as_str());
    assert_eq!(resolved["outcome"], "connected");
    c.wait_for("the mark gone", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["needsConnector"].is_null()).await;

    // Skipped reads differently, and a Claude session asks the same way.
    let claude = c.create(&sb.folder("proj2"), &marker("request_connector", json!({ "service": "Sentry", "reason": "The stack trace." })), json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" })).await;
    c.wait_turn_done(&claude, 0).await;
    let asked = c.events(&claude).into_iter().find(|e| e["type"] == "connector_request").expect("Claude's connector_request");
    c.send(json!({ "op": "connector.resolve", "sessionId": claude, "requestId": asked["requestId"], "service": "Sentry", "outcome": "skipped" })).await;
    let word = from_kind(&mut c, &claude, "connector").await;
    assert!(word["text"].as_str().unwrap().starts_with("The user chose not to connect Sentry"), "{word}");
}

/// A raw POST to the tools endpoint: the status line and body.
fn post(port: u16, headers: &str, body: &str) -> String {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "POST /agent-tools HTTP/1.1\r\nHost: 127.0.0.1\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).unwrap();
    out
}

#[tokio::test]
async fn the_tools_endpoint_answers_only_a_key_it_made() {
    let sb = Sandbox::new("tools-key");
    let daemon = sb.start();
    let body = r#"{"method":"tools/list"}"#;
    assert!(post(daemon.port, "", body).starts_with("HTTP/1.1 401"));
    assert!(post(daemon.port, "Authorization: Bearer 00ff\r\n", body).starts_with("HTTP/1.1 401"));
    // A browser page can never call it, key or not.
    assert!(post(daemon.port, "Origin: http://evil.example\r\nAuthorization: Bearer 00ff\r\n", body).starts_with("HTTP/1.1 403"));

    // A real key, as an agent's engine got it, works from this machine.
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let ada = codex_agent(&mut c, "Ada").await;
    let sid = ask(&mut c, &ada, "hello").await;
    c.wait_turn_done(&sid, 0).await;
    let server = thread_starts(&sb)[0]["config"]["mcp_servers.switchyard"].clone();
    assert_eq!(server["args"], json!(["agent-tools"]));
    let key = server["env"]["YARD_TOOLS_KEY"].as_str().unwrap().to_string();
    assert_eq!(server["env"]["YARD_TOOLS_URL"], format!("http://127.0.0.1:{}/agent-tools", daemon.port));
    let ok = post(daemon.port, &format!("Authorization: Bearer {key}\r\n"), body);
    assert!(ok.starts_with("HTTP/1.1 200") && ok.contains("message_agent"), "{ok}");
    // Deleting the agent (and its conversation) retires the key.
    c.send(json!({ "op": "agent.delete", "id": ada })).await;
    c.wait_for("the conversation gone", |m| m["op"] == "session.deleted" && m["sessionId"] == sid.as_str()).await;
    assert!(post(daemon.port, &format!("Authorization: Bearer {key}\r\n"), body).starts_with("HTTP/1.1 401"));
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

fn saved_agent(sb: &Sandbox, id: &str) -> Value {
    let text = std::fs::read_to_string(sb.data().join("agents.json")).unwrap();
    serde_json::from_str::<Vec<Value>>(&text).unwrap().into_iter().find(|a| a["id"] == id).unwrap_or(Value::Null)
}

#[tokio::test]
async fn an_agent_on_a_host_uses_its_tools_there_and_agents_reach_each_other_across_hosts() {
    let remote_sb = Sandbox::new("tools-far-remote");
    let remote = remote_sb.start();
    let hub_sb = Sandbox::new("tools-far-hub");
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    let host = add_remote(&mut c, &remote, "TEST-box").await;
    let bea = codex_agent(&mut c, "Bea").await;
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "name": "Far", "engine": "codex", "model": MODEL, "host": host, "memory": "TEST-old line" }, "ref": "far" })).await;
    let far = c.wait_after(from, "Far saved", |m| m["ref"] == "far").await["agent"]["id"].as_str().unwrap().to_string();
    let far_own = far.split_once('~').unwrap().1.to_string();
    // A neighbour on the same host.
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "name": "Near", "engine": "codex", "model": MODEL, "host": host }, "ref": "near" })).await;
    let near = c.wait_after(from, "Near saved", |m| m["ref"] == "near").await["agent"]["id"].as_str().unwrap().to_string();

    // Far's tools act on its own host: its memory there, the agents there.
    let text = format!(
        "{} {} {}",
        marker("remember", json!({ "text": "TEST-deploys on Fridays" })),
        marker("message_agent", json!({ "to": "Near", "text": "TEST-what is 2+2?" })),
        marker("notify_owner", json!({ "message": "TEST-need the key" })),
    );
    let far_chat = ask(&mut c, &far, &text).await;
    assert!(far_chat.starts_with(&format!("{host}~")), "{far_chat}");
    c.wait_turn_done(&far_chat, 0).await;
    assert!(results(&hub_sb, "remember").is_empty(), "the calls ran on the remote");
    let r = &results(&remote_sb, "remember")[0];
    assert_eq!(r["text"], "Saved. Your memory now:\nTEST-old line\nTEST-deploys on Fridays");
    assert_eq!(saved_agent(&remote_sb, &far_own)["memory"], "TEST-old line\nTEST-deploys on Fridays");
    c.wait_for("Far's memory listed on the hub", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == far.as_str() && a["memory"].as_str().is_some_and(|t| t.ends_with("TEST-deploys on Fridays")))).await;
    let sent = &results(&remote_sb, "message_agent")[0];
    assert!(sent["text"].as_str().unwrap().starts_with("Sent to @Near (hop 1 of 3)."), "{sent}");
    let near_chat = chat_of(&mut c, &near).await;
    assert!(near_chat.starts_with(&format!("{host}~")));
    let back = from_kind(&mut c, &far_chat, "agentReply").await;
    assert_eq!(back["origin"]["name"], "Near");
    // notify_owner reaches the hub's windows, naming the conversation as they know it.
    let note = c.wait_for("a notification on the hub", |m| m["op"] == "notify").await;
    assert_eq!(note["title"], "Far needs you");
    assert_eq!(note["sessionId"], far_chat.as_str());
    let from = c.mark();
    c.send(json!({ "op": "agent.seen", "agentId": far })).await;
    c.wait_after(from, "needs you cleared", |m| m["op"] == "session" && m["session"]["id"] == far_chat.as_str() && m["session"]["needsYou"].is_null()).await;

    // An agent on the hub messages Far by its handle; Far's answer, made on
    // the remote, comes back to it here.
    let bea_chat = ask(&mut c, &bea, &marker("message_agent", json!({ "to": "Far", "text": "TEST-ping far" }))).await;
    c.wait_turn_done(&bea_chat, 0).await;
    let sent = &results(&hub_sb, "message_agent")[0];
    assert!(sent["text"].as_str().unwrap().starts_with("Sent to @Far (hop 1 of 3)."), "{sent}");
    let got = c.wait_for("Bea's message at Far", |m| is_event(m, &far_chat) && m["event"]["origin"]["kind"] == "agent").await;
    assert_eq!(got["event"]["origin"]["agentId"], bea.as_str());
    assert_eq!(got["event"]["origin"]["name"], "Bea");
    let answer = c
        .wait_for("Far's answer at Bea", |m| is_event(m, &bea_chat) && m["event"]["origin"]["kind"] == "agentReply" && m["event"]["origin"]["agentId"] == far.as_str())
        .await;
    assert!(answer["event"]["text"].as_str().unwrap().contains("TEST-ping far"), "{answer}");
    // list_agents on the hub names the host's agents too.
    let listed = calls(&hub_sb).into_iter().find(|r| r["listed"].is_array()).unwrap();
    assert!(listed["messageAgent"].as_str().unwrap().contains("@Far") && listed["messageAgent"].as_str().unwrap().contains("@Near"), "{listed}");
}

#[tokio::test]
async fn a_kept_agents_tools_act_on_the_hub_while_its_host_is_older() {
    // An older host: it runs the hub's copy, and passes its tool calls back.
    let remote_sb = Sandbox::new("tools-kept-remote");
    let remote = remote_sb.start_with(&[("YARD_TEST_HOSTED_ONLY", "1")]);
    let hub_sb = Sandbox::new("tools-kept-hub");
    let host = "hTEST3";
    std::fs::write(hub_sb.data().join("hosts.json"), json!([{ "id": host, "name": "TEST-box", "kind": "direct", "url": remote.url() }]).to_string()).unwrap();
    std::fs::write(
        hub_sb.data().join("agents.json"),
        json!([{ "id": "g-far", "name": "Far", "handle": "Far", "engine": "codex", "model": MODEL, "memory": "TEST-old line", "host": host, "createdAt": 1, "updatedAt": 1 }]).to_string(),
    )
    .unwrap();
    let hub = hub_sb.start();
    let mut c = hub.client().await;
    ready(&mut c).await;
    c.wait_for("the older host connected", |m| (m["op"] == "hosts" || m["op"] == "snapshot") && m["hosts"][0]["ownAgents"] == false).await;
    let bea = codex_agent(&mut c, "Bea").await;
    // Its first message hands the host the copy (`agent.hosted.save`).
    let text = format!("{} {}", marker("remember", json!({ "text": "TEST-deploys on Fridays" })), marker("message_agent", json!({ "to": "Bea", "text": "TEST-what is 2+2?" })));
    let far_chat = ask(&mut c, "g-far", &text).await;
    assert!(far_chat.starts_with(&format!("{host}~")), "{far_chat}");
    c.wait_turn_done(&far_chat, 0).await;
    let r = &results(&remote_sb, "remember")[0];
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(saved_agent(&hub_sb, "g-far")["memory"], "TEST-old line\nTEST-deploys on Fridays");
    let copy = saved_agent(&remote_sb, "g-far");
    assert_eq!(copy["hosted"], true);
    assert_eq!(copy["memory"], "TEST-old line\nTEST-deploys on Fridays");
    let bea_chat = chat_of(&mut c, &bea).await;
    let got = from_kind(&mut c, &bea_chat, "agent").await;
    assert_eq!(got["origin"], json!({ "kind": "agent", "agentId": "g-far", "name": "Far", "handle": "Far", "hops": 1 }));
    let back = from_kind(&mut c, &far_chat, "agentReply").await;
    assert_eq!(back["origin"]["agentId"], bea.as_str());
}

/// What a hub sends a host that runs its agent, sent by hand.
async fn save_hosted(c: &mut Client, agent: Value) {
    let from = c.mark();
    c.send(json!({ "op": "agent.hosted.save", "agent": agent, "ref": "s" })).await;
    assert_eq!(c.wait_after(from, "hosted save", |m| m["ref"] == "s").await["op"], "agent.hosted.saved");
}

async fn ask_hosted(c: &mut Client, id: &str, text: &str) -> String {
    let from = c.mark();
    c.send(json!({ "op": "agent.hosted.ask", "agentId": id, "text": text, "ref": "a" })).await;
    let r = c.wait_after(from, "hosted ask", |m| m["ref"] == "a").await;
    assert_eq!(r["op"], "created", "{r}");
    r["sessionId"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn a_hosted_agent_gets_tools_only_from_a_hub_that_answers_them() {
    let sb = Sandbox::new("tools-hosted");
    let daemon = sb.start();
    let mut c = daemon.client().await;
    ready(&mut c).await;
    let hosted = |id: &str, hub: &str| json!({ "id": id, "name": id, "engine": "codex", "model": MODEL, "createdAt": 1, "updatedAt": 1, "hub": hub });

    // Pushed by a hub that does not answer tools: none offered, none promised.
    save_hosted(&mut c, hosted("g-old", "")).await;
    let sid = ask_hosted(&mut c, "g-old", "hello").await;
    c.wait_turn_done(&sid, 0).await;
    // The server every session has is there (`request_connector`), but none of the agents' tools is promised.
    assert!(thread_starts(&sb)[0]["config"]["mcp_servers.switchyard"].is_object(), "{:?}", thread_starts(&sb));
    assert!(!sb.turn_inputs()[0][0]["text"].as_str().unwrap().contains("message_agent"));

    // A hub that answers them, while it is not connected: the call says so.
    save_hosted(&mut c, hosted("g-far", "abc123")).await;
    let sid = ask_hosted(&mut c, "g-far", &marker("remember", json!({ "text": "TEST-x" }))).await;
    c.wait_turn_done(&sid, 0).await;
    let r = &results(&sb, "remember")[0];
    assert_eq!(r["isError"], true);
    assert!(r["text"].as_str().unwrap().contains("not connected to this host"), "{r}");
    assert_eq!(saved_agent(&sb, "g-far")["memory"], "");

    // Once it is: the call goes to it, on its connection only, and its answer is used.
    let mut hub = daemon.client().await;
    hub.send(json!({ "op": "agent.hosted.hub", "hub": "abc123" })).await;
    hub.drain_for(Duration::from_millis(300)).await;
    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": marker("remember", json!({ "text": "TEST-y" })) })).await;
    // The server lists the tools first, then calls one.
    let list = hub.wait_for("the list at the hub", |m| m["op"] == "agent.hub.call" && m["method"] == "tools/list").await;
    hub.send(json!({ "op": "agent.hosted.answer", "ref": list["ref"], "result": { "tools": [{ "name": "remember", "description": "TEST-from the hub", "inputSchema": { "type": "object" } }] } })).await;
    let call = hub.wait_for("the call at the hub", |m| m["op"] == "agent.hub.call" && m["method"] == "tools/call").await;
    assert_eq!(call["agentId"], "g-far");
    assert_eq!(call["sessionId"], sid.as_str());
    assert_eq!(call["params"]["name"], "remember");
    // Another client's answer is not the hub's.
    c.send(json!({ "op": "agent.hosted.answer", "ref": call["ref"], "result": { "content": [{ "type": "text", "text": "forged" }], "isError": false }, "memory": "forged" })).await;
    hub.send(json!({ "op": "agent.hosted.answer", "ref": call["ref"], "result": { "content": [{ "type": "text", "text": "TEST-done" }], "isError": false }, "memory": "TEST-y" })).await;
    c.wait_turn_done(&sid, seq).await;
    let r = results(&sb, "remember").pop().unwrap();
    assert_eq!(r["isError"], false, "{r}");
    assert_eq!(r["text"], "TEST-done");
    assert_eq!(saved_agent(&sb, "g-far")["memory"], "TEST-y");
}

/// `agent.new`, answered with the agent.
async fn new_agent(c: &mut Client) -> Value {
    let from = c.mark();
    let r = format!("new-{from}");
    c.send(json!({ "op": "agent.new", "engine": "codex", "model": MODEL, "effort": EFFORT, "ref": r })).await;
    let reply = c.wait_after(from, "agent.new reply", |m| m["ref"] == r.as_str()).await;
    assert_eq!(reply["op"], "agent.saved", "{reply}");
    reply["agent"].clone()
}

fn agent_in(m: &Value, id: &str) -> Value {
    m["agents"].as_array().and_then(|l| l.iter().find(|a| a["id"] == id).cloned()).unwrap_or(Value::Null)
}

#[tokio::test]
async fn a_new_agent_sets_itself_up_with_its_tool() {
    let sb = Sandbox::new("tools-setup");
    let d = start(&sb);
    let mut c = d.client().await;
    ready(&mut c).await;
    let folder = sb.folder("TEST-project");
    let made = new_agent(&mut c).await;
    let id = made["id"].as_str().unwrap().to_string();
    assert_eq!((made["name"].as_str(), made["setup"].as_bool()), (Some("New-agent"), Some(true)));
    // Asked again before anyone wrote to it: the same one.
    assert_eq!(new_agent(&mut c).await["id"], id.as_str());

    // Its first turn has the first-run rules; the scripted engine calls the
    // tool as if the model chose to.
    let setup = json!({ "name": "Deployer", "about": "Watches deploys", "instructions": "Be brief.", "face": "bot", "color": "rose",
        "model": "fake-deep", "permission": "full", "cwd": folder, "connect": "slack", "mood": "x" });
    let sid = ask(&mut c, &id, &format!("TEST-watch my deploys {}", marker("update_setup", setup))).await;
    c.wait_turn_done(&sid, 0).await;
    let first = sb.turn_inputs()[0][0]["text"].as_str().unwrap().to_string();
    assert!(first.contains("<first_run>") && first.contains("New-agent is a placeholder name") && first.contains("update_setup"), "{first}");
    assert!(first.contains("Your setup now: name New-agent;"), "{first}");
    let result = &results(&sb, "update_setup")[0];
    assert_eq!(result["isError"], false, "{result}");
    let text = result["text"].as_str().unwrap();
    assert!(text.starts_with("Saved. New-agent is set up as @Deployer: name, what it is for, instructions, face, colour."), "{text}");
    assert!(text.contains("It switches its model when this turn ends."), "{text}");
    assert!(text.contains(&format!("It asks for the folder {folder}, full access (runs anything without asking) and a place in Slack")), "{text}");
    assert!(text.contains("Ignored: mood."), "{text}");
    let m = c.wait_for("the agent set up", |m| m["op"] == "agents" && agent_in(m, &id)["request"].is_object() && agent_in(m, &id)["model"] == "fake-deep").await;
    let a = agent_in(&m, &id);
    assert_eq!((a["name"].as_str(), a["setup"].as_bool()), (Some("Deployer"), None));
    assert_eq!((a["about"].as_str(), a["instructions"].as_str(), a["face"].as_str(), a["color"].as_str()), (Some("Watches deploys"), Some("Be brief."), Some("bot"), Some("rose")));
    // A folder of the user's, more access and Slack wait for a click.
    assert_eq!((a["permission"].as_str(), a["cwd"].as_str()), (Some("ask"), Some("")));
    assert_eq!(a["request"], json!({ "cwd": folder, "permission": "full", "slack": true }));
    // The thread says what changed, and that it now runs on the new model.
    let notes: Vec<String> = c.history(&sid).await.into_iter().filter(|e| e["type"] == "note").map(|e| e["text"].as_str().unwrap().to_string()).collect();
    assert!(notes.iter().any(|n| n.starts_with("New-agent is set up as @Deployer")), "{notes:?}");
    assert!(notes.iter().any(|n| n == "Deployer now runs on fake-deep."), "{notes:?}");

    // The owner answers: more access yes, the folder no, Slack not now.
    let from = c.mark();
    for (part, allow) in [("permission", true), ("cwd", false), ("slack", false)] {
        c.send(json!({ "op": "agent.request", "id": id, "part": part, "allow": allow })).await;
    }
    let m = c.wait_after(from, "the requests answered", |m| m["op"] == "agents" && agent_in(m, &id)["request"].is_null() && agent_in(m, &id)["permission"] == "full").await;
    assert_eq!(agent_in(&m, &id)["cwd"], "");
    let notes: Vec<String> = c.history(&sid).await.into_iter().filter(|e| e["type"] == "note").map(|e| e["text"].as_str().unwrap().to_string()).collect();
    assert!(notes.iter().any(|n| n.starts_with("Gave Deployer full access")), "{notes:?}");
    assert!(notes.iter().any(|n| n.starts_with("Did not allow Deployer to work in")), "{notes:?}");
    assert!(notes.iter().any(|n| n == "Not now: Deployer stays out of Slack."), "{notes:?}");

    // Its next turn runs on the new model in a fresh context, with its setup
    // as it is now and no first-run rules.
    let seq = c.next_seq(&sid);
    ask(&mut c, &id, "TEST-thanks").await;
    c.wait_turn_done(&sid, seq).await;
    let next = sb.turn_inputs().last().unwrap()[0]["text"].as_str().unwrap().to_string();
    assert!(next.starts_with("You are Deployer,") && next.contains("model fake-deep;") && next.contains("permission full,") && !next.contains("<first_run>"), "{next}");
    assert_eq!(thread_starts(&sb).last().unwrap()["model"], "fake-deep");
    let notes: Vec<String> = c.history(&sid).await.into_iter().filter(|e| e["type"] == "note").map(|e| e["text"].as_str().unwrap().to_string()).collect();
    assert!(notes.iter().any(|n| n.starts_with("Started a fresh context: its model changed.")), "{notes:?}");
    // A name of a few words is a name like any other; its handle is made from it.
    let seq = c.next_seq(&sid);
    ask(&mut c, &id, &format!("TEST-rename {}", marker("update_setup", json!({ "name": "Deploy  Watcher" })))).await;
    c.wait_turn_done(&sid, seq).await;
    let renamed = results(&sb, "update_setup").pop().unwrap();
    assert_eq!(renamed["isError"], false, "{renamed}");
    assert!(renamed["text"].as_str().unwrap().starts_with("Saved. Deploy Watcher changed its name."), "{renamed}");
    let m = c.wait_for("the new name", |m| m["op"] == "agents" && agent_in(m, &id)["name"] == "Deploy Watcher").await;
    assert_eq!(agent_in(&m, &id)["handle"], "Deploy-Watcher");
    // A bad name is refused with the reason, in the tool's answer.
    let seq = c.next_seq(&sid);
    ask(&mut c, &id, &format!("TEST-rename {}", marker("update_setup", json!({ "name": "x".repeat(41) })))).await;
    c.wait_turn_done(&sid, seq).await;
    let refused = results(&sb, "update_setup").pop().unwrap();
    assert_eq!(refused["isError"], true);
    assert!(refused["text"].as_str().unwrap().contains("A name is at most 40 characters"), "{refused}");
    // A new agent after this one is a new one again.
    let other = new_agent(&mut c).await;
    assert_ne!(other["id"], id.as_str());
    assert_eq!(other["name"], "New-agent");
}
