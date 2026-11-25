//! Slack: a fake Slack (verify/fakes/fake-slack: the Web API and Socket
//! Mode) drives yardd's connection. Messages from allowed users reach the
//! agent they name, the default agent, or the agent their thread went to;
//! everyone else and every bot is ignored; the answer goes back to the
//! thread with reactions. No real Slack, no real model.
mod common;
use common::*;
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const APP_TOKEN: &str = "xapp-1-A0TEST-TESTsecretAPPtoken";
const BOT_TOKEN: &str = "xoxb-TEST-secretBOTtoken";

/// The fake, on a free port, killed on drop.
struct FakeSlack {
    child: Child,
    port: u16,
}

impl FakeSlack {
    fn start() -> FakeSlack {
        let mut child = Command::new("bun")
            .arg(repo_root().join("verify/fakes/fake-slack"))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("bun runs verify/fakes/fake-slack");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        let port = serde_json::from_str::<Value>(&line).expect("the fake prints its port")["port"].as_u64().unwrap() as u16;
        assert!(port != 47717 && !(4510..=4559).contains(&port));
        FakeSlack { child, port }
    }

    fn api(&self) -> String {
        format!("http://127.0.0.1:{}/api/", self.port)
    }

    fn request(&self, method: &str, path: &str, body: &Value) -> Value {
        let mut s = std::net::TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let body = body.to_string();
        write!(s, "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).unwrap();
        let body = out.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or("");
        serde_json::from_str(body).unwrap_or_else(|_| panic!("fake answered {out}"))
    }

    /// Slack's events for a message from `user`; returns its ts.
    fn say(&self, msg: Value) -> String {
        self.request("POST", "/_fake/message", &msg)["ts"].as_str().unwrap().to_string()
    }

    fn state(&self) -> Value {
        self.request("GET", "/_fake/state", &json!({}))
    }

    async fn wait(&self, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + WAIT;
        loop {
            let s = self.state();
            if pred(&s) {
                return s;
            }
            if Instant::now() > deadline {
                panic!("timed out waiting for the fake Slack: {what}\n{s:#}");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// The bot's posts in one thread.
    fn posts(state: &Value, thread: &str) -> Vec<String> {
        state["posts"].as_array().unwrap().iter().filter(|p| p["thread_ts"] == thread).map(|p| p["text"].as_str().unwrap().to_string()).collect()
    }
}

impl Drop for FakeSlack {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn ready(c: &mut Client) {
    c.wait_for("models ready", |m| (m["op"] == "models" || m["op"] == "snapshot") && m["models"]["codex"].as_array().is_some_and(|a| !a.is_empty())).await;
}

async fn op(c: &mut Client, msg: Value) -> Result<(), String> {
    let from = c.mark();
    let op = msg["op"].as_str().unwrap().to_string();
    c.send(msg).await;
    let reply = c.wait_after(from, &format!("{op} applied"), |m| m["op"] == "slack" || (m["op"] == "error" && m["for"] == op.as_str())).await;
    match reply["op"].as_str() {
        Some("error") => Err(reply["message"].as_str().unwrap_or("").to_string()),
        _ => Ok(()),
    }
}

async fn slack_state(c: &mut Client, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    c.wait_for(what, |m| (m["op"] == "slack" && pred(&m["slack"])) || (m["op"] == "snapshot" && pred(&m["slack"]))).await["slack"].clone()
}

async fn connect(c: &mut Client) {
    op(c, json!({ "op": "slack.tokens", "appToken": APP_TOKEN, "botToken": BOT_TOKEN })).await.unwrap();
    let s = slack_state(c, "Slack connected", |s| s["state"] == "connected").await;
    assert_eq!(s["team"], "TEST-workspace");
    assert_eq!(s["bot"], "switchyard");
}

async fn reviewer(c: &mut Client) -> String {
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "name": "Reviewer", "engine": "codex", "model": MODEL, "effort": EFFORT }, "ref": "save" })).await;
    c.wait_after(from, "agent saved", |m| m["op"] == "agent.saved").await["agent"]["id"].as_str().unwrap().to_string()
}

/// The agent's conversation, once its first message made it.
async fn chat_of(c: &mut Client, id: &str) -> String {
    let m = c.wait_for("the agent's conversation", |m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == id && a["chat"].is_string())).await;
    m["agents"].as_array().unwrap().iter().find(|a| a["id"] == id).unwrap()["chat"].as_str().unwrap().to_string()
}

fn user_messages(c: &Client, sid: &str) -> Vec<Value> {
    c.events(sid).into_iter().filter(|e| e["type"] == "user_message").collect()
}

#[tokio::test]
async fn messages_from_slack_reach_the_named_or_default_agent_and_answers_go_back_to_the_thread() {
    let fake = FakeSlack::start();
    let sb = Sandbox::new("slack-route");
    let daemon = sb.start_with(&[("YARD_SLACK_API", &fake.api())]);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    assert_eq!(c.log[0]["slack"]["state"], "off");
    assert_eq!(c.log[0]["slack"]["configured"], false);
    let id = reviewer(&mut c).await;

    // Tokens of the wrong kind are refused before anything is saved.
    assert!(op(&mut c, json!({ "op": "slack.tokens", "appToken": BOT_TOKEN, "botToken": BOT_TOKEN })).await.unwrap_err().contains("xapp-"));
    assert!(op(&mut c, json!({ "op": "slack.tokens", "appToken": APP_TOKEN })).await.unwrap_err().contains("both tokens"));
    connect(&mut c).await;

    // Not on the allowlist: ignored, told once, and offered for approval.
    let first = fake.say(json!({ "user": "UOWNER", "text": "Reviewer TEST-hello" }));
    let s = slack_state(&mut c, "a request to allow the owner", |s| s["requests"][0]["id"] == "UOWNER").await;
    assert_eq!(s["requests"][0]["name"], "TEST-Owner");
    assert_eq!(s["requests"][0]["place"], "Direct message");
    fake.say(json!({ "user": "UOWNER", "text": "Reviewer TEST-hello again" }));
    fake.wait("both messages seen", |s| s["acks"].as_array().unwrap().len() >= 2).await;
    c.drain_for(Duration::from_millis(500)).await;
    let st = fake.state();
    assert_eq!(st["ephemerals"].as_array().unwrap().len(), 1, "told once: {st:#}");
    assert!(st["ephemerals"][0]["text"].as_str().unwrap().contains("UOWNER"));
    assert!(st["posts"].as_array().unwrap().is_empty());
    assert!(st["reactions"].get(&first).is_none());
    assert!(!c.log.iter().any(|m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["chat"].is_string())));
    op(&mut c, json!({ "op": "slack.allow", "user": "UOWNER" })).await.unwrap();
    let s = slack_state(&mut c, "the owner allowed", |s| s["allowed"][0]["id"] == "UOWNER").await;
    assert_eq!(s["allowed"][0]["name"], "TEST-Owner");
    assert!(s["requests"].as_array().unwrap().is_empty());

    // A DM naming the agent goes into its one conversation, with its origin.
    let ts = fake.say(json!({ "user": "UOWNER", "text": "Reviewer: TEST-look at the build" }));
    let sid = chat_of(&mut c, &id).await;
    c.wait_turn_done(&sid, 0).await;
    let msgs = user_messages(&c, &sid);
    assert_eq!(msgs[0]["text"], "TEST-look at the build");
    let origin = &msgs[0]["origin"];
    assert_eq!(origin["kind"], "slack");
    assert_eq!(origin["team"], "T0TEST");
    assert_eq!(origin["channel"], "D0OWNER");
    assert_eq!(origin["channelName"], "Direct message");
    assert_eq!(origin["threadTs"], ts.as_str());
    assert_eq!(origin["ts"], ts.as_str());
    assert_eq!(origin["user"], "UOWNER");
    assert_eq!(origin["userName"], "TEST-Owner");
    // The engine reads the text framed with who sent it and where (origins::engine_text).
    let input = sb.turn_inputs()[0][0]["text"].as_str().unwrap().to_string();
    assert!(input.contains("<slack from=\"TEST-Owner\" in=\"a direct message\">\nTEST-look at the build\n</slack>"), "{input}");
    assert!(input.contains("posted back to that Slack thread"), "{input}");
    let st = fake.wait("the answer in the thread", |s| !FakeSlack::posts(s, &ts).is_empty() && s["reactions"][&ts] == json!(["white_check_mark"])).await;
    let answer = FakeSlack::posts(&st, &ts);
    assert_eq!(answer.len(), 1);
    assert!(answer[0].starts_with("Echo: ") && answer[0].contains("TEST-look at the build"), "{answer:?}");
    assert!(st["calls"].as_array().unwrap().iter().any(|c| c["method"] == "reactions.add" && c["params"]["name"] == "eyes" && c["params"]["timestamp"] == ts.as_str()));

    // A follow-up in that Slack thread stays with Reviewer without its name.
    let seq = c.next_seq(&sid);
    fake.say(json!({ "user": "UOWNER", "text": "TEST-and the tests?", "thread_ts": ts }));
    c.wait_turn_done(&sid, seq).await;
    let st = fake.wait("the follow-up's answer", |s| FakeSlack::posts(s, &ts).len() == 2).await;
    assert!(FakeSlack::posts(&st, &ts)[1].contains("TEST-and the tests?"));

    // No name and no default agent: the bot lists the agents.
    let lost = fake.say(json!({ "user": "UOWNER", "text": "TEST-anyone there?" }));
    let st = fake.wait("the list of agents", |s| !FakeSlack::posts(s, &lost).is_empty()).await;
    let list = &FakeSlack::posts(&st, &lost)[0];
    assert!(list.starts_with("Which agent?") && list.contains("Reviewer"), "{list}");

    // With a default agent it goes there. A bot's message never does.
    op(&mut c, json!({ "op": "slack.route", "agent": id })).await.unwrap();
    slack_state(&mut c, "the default agent", |s| s["defaultAgent"] == id.as_str()).await;
    fake.say(json!({ "user": "UOWNER", "bot_id": "BOTHER", "text": "TEST-from a bot" }));
    fake.say(json!({ "user": "UBOT", "text": "TEST-from myself" }));
    let seq = c.next_seq(&sid);
    let plain = fake.say(json!({ "user": "UOWNER", "text": "TEST-plain question" }));
    c.wait_turn_done(&sid, seq).await;
    fake.wait("the default agent's answer", |s| !FakeSlack::posts(s, &plain).is_empty()).await;
    let texts: Vec<String> = user_messages(&c, &sid).iter().map(|m| m["text"].as_str().unwrap().to_string()).collect();
    assert!(texts.contains(&"TEST-plain question".to_string()));
    assert!(!texts.iter().any(|t| t.contains("from a bot") || t.contains("from myself")), "{texts:?}");

    // An @mention in a channel comes twice (app_mention and message): one turn.
    let seq = c.next_seq(&sid);
    let mention = fake.say(json!({ "user": "UOWNER", "channel": "C0GENERAL", "text": "<@UBOT> Reviewer TEST-check ops" }));
    c.wait_turn_done(&sid, seq).await;
    let st = fake.wait("the answer in the channel thread", |s| !FakeSlack::posts(s, &mention).is_empty()).await;
    assert!(st["posts"].as_array().unwrap().iter().any(|p| p["channel"] == "C0GENERAL" && p["thread_ts"] == mention.as_str()));
    let msgs = user_messages(&c, &sid);
    let from_channel: Vec<&Value> = msgs.iter().filter(|m| m["text"] == "TEST-check ops").collect();
    assert_eq!(from_channel.len(), 1);
    assert_eq!(from_channel[0]["origin"]["channelName"], "#general");
    let s = slack_state(&mut c, "the channel listed", |s| s["channels"][0]["id"] == "C0GENERAL").await;
    assert_eq!(s["channels"][0]["name"], "#general");
    // In a channel, a message without a mention counts only inside a routed thread.
    fake.say(json!({ "user": "UOWNER", "channel": "C0GENERAL", "text": "TEST-chatter" }));
    let seq = c.next_seq(&sid);
    fake.say(json!({ "user": "UOWNER", "channel": "C0GENERAL", "text": "TEST-in the thread", "thread_ts": mention }));
    c.wait_turn_done(&sid, seq).await;
    let texts: Vec<String> = user_messages(&c, &sid).iter().map(|m| m["text"].as_str().unwrap().to_string()).collect();
    assert!(texts.contains(&"TEST-in the thread".to_string()) && !texts.contains(&"TEST-chatter".to_string()), "{texts:?}");

    // A failed turn says so in the thread and marks the message with :x:.
    let seq = c.next_seq(&sid);
    let failing = fake.say(json!({ "user": "UOWNER", "text": "Reviewer [fail] TEST-break" }));
    c.wait_turn_done(&sid, seq).await;
    let st = fake.wait("the failure post", |s| !FakeSlack::posts(s, &failing).is_empty() && s["reactions"][&failing] == json!(["x"])).await;
    assert!(FakeSlack::posts(&st, &failing)[0].starts_with("Reviewer could not finish"));

    // A turn waiting on an approval says so once; Stop then reports it.
    let seq = c.next_seq(&sid);
    let asking = fake.say(json!({ "user": "UOWNER", "text": "Reviewer [cmd] TEST-run it" }));
    c.wait_event(&sid, "approval_request", seq).await;
    fake.wait("the approval note", |s| FakeSlack::posts(s, &asking).len() == 1).await;
    // A message that arrives meanwhile waits in the queue; Stop hands it back
    // to its sender unread (the owner cannot edit someone else's message).
    let waiting = fake.say(json!({ "user": "UOWNER", "text": "Reviewer TEST-after that" }));
    c.wait_for("the Slack message queued", |m| m["op"] == "session" && m["session"]["id"] == sid.as_str() && m["session"]["pending"].as_array().is_some_and(|p| p.iter().any(|q| q["origin"]["ts"] == waiting.as_str()))).await;
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.wait_turn_done(&sid, seq).await;
    let st = fake.wait("the stop note", |s| FakeSlack::posts(s, &asking).len() == 2 && s["reactions"][&asking] == json!(["x"])).await;
    let notes = FakeSlack::posts(&st, &asking);
    assert!(notes[0].starts_with("Reviewer needs your approval in Switchyard"), "{notes:?}");
    assert_eq!(notes[1], "Reviewer was stopped in Switchyard.");
    let st = fake.wait("the unread note", |s| !FakeSlack::posts(s, &waiting).is_empty() && s["reactions"][&waiting] == json!(["x"])).await;
    assert_eq!(FakeSlack::posts(&st, &waiting), ["Reviewer was stopped in Switchyard before it read your message."]);
    assert!(!user_messages(&c, &sid).iter().any(|m| m["text"] == "TEST-after that"));

    // Every envelope was acknowledged.
    let st = fake.state();
    let envelopes = st["acks"].as_array().unwrap().len();
    assert!(envelopes >= 10, "{envelopes} acks");

    // The tokens never reach a client or a log, and their file is the owner's alone.
    let fresh = daemon.client().await;
    let snapshot = fresh.log[0].to_string();
    assert_eq!(fresh.log[0]["slack"]["botToken"], "xoxb-…oken");
    assert_eq!(fresh.log[0]["slack"]["appToken"], "xapp-…oken");
    let everything = format!("{snapshot}{}", c.log.iter().map(Value::to_string).collect::<String>());
    for secret in [APP_TOKEN, BOT_TOKEN, "secretAPP", "secretBOT", "TEST-ticket"] {
        assert!(!everything.contains(secret), "{secret} reached a client");
        assert!(!daemon.log_text().contains(secret), "{secret} reached the log");
    }
    let file = sb.data().join("slack.json");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    assert!(std::fs::read_to_string(&file).unwrap().contains(BOT_TOKEN));
}

#[tokio::test]
async fn an_agent_still_setting_up_is_not_offered_or_reached_by_name_from_slack() {
    let fake = FakeSlack::start();
    let sb = Sandbox::new("slack-setup");
    let daemon = sb.start_with(&[("YARD_SLACK_API", &fake.api())]);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    reviewer(&mut c).await;
    connect(&mut c).await;
    op(&mut c, json!({ "op": "slack.allow", "user": "UOWNER" })).await.unwrap();
    // New agent: a placeholder that sets itself up from the owner's first words.
    let from = c.mark();
    c.send(json!({ "op": "agent.new", "engine": "codex", "model": MODEL, "effort": EFFORT })).await;
    let blank = c.wait_after(from, "the new agent", |m| m["op"] == "agent.saved").await["agent"].clone();
    let (blank_id, blank_name) = (blank["id"].as_str().unwrap().to_string(), blank["name"].as_str().unwrap().to_string());

    // No name: the bot lists only agents that are set up.
    let lost = fake.say(json!({ "user": "UOWNER", "text": "TEST-anyone there?" }));
    let st = fake.wait("the list of agents", |s| !FakeSlack::posts(s, &lost).is_empty()).await;
    let note = &FakeSlack::posts(&st, &lost)[0];
    assert!(note.contains("Reviewer"), "{note}");
    assert!(!note.contains(&blank_name), "a placeholder is offered: {note}");

    // Its placeholder name reaches nobody: a teammate's words would set it up.
    let named = fake.say(json!({ "user": "UOWNER", "text": format!("{blank_name} TEST-set yourself up") }));
    let st = fake.wait("an answer to the placeholder's name", |s| !FakeSlack::posts(s, &named).is_empty()).await;
    assert!(FakeSlack::posts(&st, &named)[0].starts_with("Which agent?"), "{st:#}");
    c.drain_for(Duration::from_millis(500)).await;
    assert!(!c.log.iter().any(|m| m["op"] == "agents" && m["agents"].as_array().unwrap().iter().any(|a| a["id"] == blank_id.as_str() && a["chat"].is_string())), "the placeholder got a conversation");
}

#[tokio::test]
async fn the_connection_shows_refused_tokens_and_comes_back_after_a_disconnect_or_a_drop() {
    let fake = FakeSlack::start();
    let sb = Sandbox::new("slack-connection");
    let daemon = sb.start_with(&[("YARD_SLACK_API", &fake.api())]);
    let mut c = daemon.client().await;
    ready(&mut c).await;

    op(&mut c, json!({ "op": "slack.tokens", "appToken": APP_TOKEN, "botToken": "xoxb-bad-TESTtoken" })).await.unwrap();
    let s = slack_state(&mut c, "a refused bot token", |s| s["state"] == "error").await;
    assert_eq!(s["error"], "The bot token was refused (auth.test: invalid_auth).");
    assert_eq!(s["configured"], true);

    // Replacing one token keeps the other.
    op(&mut c, json!({ "op": "slack.tokens", "botToken": BOT_TOKEN })).await.unwrap();
    slack_state(&mut c, "connected", |s| s["state"] == "connected").await;
    fake.wait("one connection", |s| s["connections"] == 1 && s["opens"] == 1).await;

    // Slack's `disconnect`: a new connection right away.
    let from = c.mark();
    fake.request("POST", "/_fake/disconnect", &json!({}));
    fake.wait("a second connection", |s| s["connections"] == 1 && s["opens"] == 2).await;
    c.wait_after(from, "connected again", |m| m["op"] == "slack" && m["slack"]["state"] == "connected").await;

    // A dropped socket: dialled again after a short wait.
    fake.request("POST", "/_fake/drop", &json!({}));
    fake.wait("a third connection", |s| s["connections"] == 1 && s["opens"] == 3).await;

    // Messages still arrive on the new connection.
    op(&mut c, json!({ "op": "slack.allow", "user": "UOWNER" })).await.unwrap();
    reviewer(&mut c).await;
    let ts = fake.say(json!({ "user": "UOWNER", "text": "Reviewer TEST-are you there?" }));
    fake.wait("Reviewer's answer", |s| !FakeSlack::posts(s, &ts).is_empty()).await;

    // Disconnect forgets the tokens and closes the socket.
    op(&mut c, json!({ "op": "slack.disconnect" })).await.unwrap();
    let s = slack_state(&mut c, "off", |s| s["state"] == "off").await;
    assert_eq!(s["configured"], false);
    assert_eq!(s["botToken"], "");
    fake.wait("no connection", |s| s["connections"] == 0).await;
    assert!(!std::fs::read_to_string(sb.data().join("slack.json")).unwrap().contains("secret"));
}

#[tokio::test]
async fn slack_reaches_an_agent_on_a_remote_host_and_its_answer_comes_back_to_the_thread() {
    let fake = FakeSlack::start();
    let remote_sb = Sandbox::new("slack-far-remote");
    let remote = remote_sb.start();
    let sb = Sandbox::new("slack-far-hub");
    let daemon = sb.start_with(&[("YARD_SLACK_API", &fake.api())]);
    let mut c = daemon.client().await;
    ready(&mut c).await;
    c.send(json!({ "op": "host.add", "name": "TEST-box", "kind": "direct", "url": remote.url() })).await;
    let hosts = c.wait_for("the remote connected", |m| m["op"] == "hosts" && m["hosts"][0]["status"] == "connected").await;
    let host = hosts["hosts"][0]["id"].as_str().unwrap().to_string();
    c.wait_for("the remote's models", |m| (m["op"] == "host.snapshot" || m["op"] == "models") && m["hostId"] == host.as_str() && m["models"]["codex"].as_array().is_some_and(|a| !a.is_empty())).await;
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "name": "Far", "engine": "codex", "model": MODEL, "effort": EFFORT, "host": host }, "ref": "far" })).await;
    let far = c.wait_after(from, "Far saved", |m| m["ref"] == "far").await["agent"]["id"].as_str().unwrap().to_string();
    connect(&mut c).await;
    fake.say(json!({ "user": "UOWNER", "text": "hi" }));
    slack_state(&mut c, "a request", |s| s["requests"][0]["id"] == "UOWNER").await;
    op(&mut c, json!({ "op": "slack.allow", "user": "UOWNER" })).await.unwrap();

    // Its conversation on the remote gets the message with its Slack origin...
    let ts = fake.say(json!({ "user": "UOWNER", "text": "Far: TEST-how is the box?" }));
    let sid = chat_of(&mut c, &far).await;
    assert!(sid.starts_with(&format!("{host}~")), "{sid}");
    c.wait_turn_done(&sid, 0).await;
    let msgs = user_messages(&c, &sid);
    assert_eq!(msgs[0]["text"], "TEST-how is the box?");
    assert_eq!(msgs[0]["origin"]["kind"], "slack");
    assert_eq!(msgs[0]["origin"]["threadTs"], ts.as_str());
    assert_eq!(msgs[0]["origin"]["userName"], "TEST-Owner");
    let input = remote_sb.turn_inputs()[0][0]["text"].as_str().unwrap().to_string();
    assert!(input.contains("<slack from=\"TEST-Owner\" in=\"a direct message\">\nTEST-how is the box?\n</slack>"), "{input}");
    // ...and the answer the remote's turn made is posted from the hub, in that thread.
    let st = fake.wait("the answer in the thread", |s| !FakeSlack::posts(s, &ts).is_empty() && s["reactions"][&ts] == json!(["white_check_mark"])).await;
    let answer = FakeSlack::posts(&st, &ts);
    assert_eq!(answer.len(), 1);
    assert!(answer[0].starts_with("Echo: ") && answer[0].contains("TEST-how is the box?"), "{answer:?}");
}

/// A Slack-app step with a `ref`: the new Slack state, or why not.
async fn step(c: &mut Client, msg: Value) -> Result<Value, String> {
    let from = c.mark();
    let r = format!("step-{from}");
    let mut msg = msg;
    msg["ref"] = json!(r);
    c.send(msg).await;
    let reply = c.wait_after(from, "the step's answer", |m| m["ref"] == r.as_str() && (m["op"] == "slack" || m["op"] == "error")).await;
    match reply["op"].as_str() {
        Some("error") => Err(reply["message"].as_str().unwrap_or("").to_string()),
        _ => Ok(reply["slack"].clone()),
    }
}

#[tokio::test]
async fn an_agent_gets_a_slack_app_of_its_own_from_a_configuration_token() {
    let fake = FakeSlack::start();
    let sb = Sandbox::new("slack-agent-app");
    let api = fake.api();
    let env = [("YARD_SLACK_API", api.as_str()), ("YARD_ALLOW_ORIGIN", "https://yard.test")];
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    ready(&mut c).await;
    let id = reviewer(&mut c).await;

    // The configuration token: an Access Token is told apart, a wrong one refused.
    let wrong = step(&mut c, json!({ "op": "slack.grant", "token": "xoxe.xoxp-1-TEST" })).await.unwrap_err();
    assert!(wrong.contains("That is the Access Token"), "{wrong}");
    assert!(step(&mut c, json!({ "op": "slack.grant", "token": "xoxb-1-TEST" })).await.unwrap_err().contains("starts with xoxe-"));
    assert!(step(&mut c, json!({ "op": "slack.grant", "token": "xoxe-1-bad-TEST" })).await.unwrap_err().contains("refused"));
    // The refresh token, with the agent: its app is made in that workspace at once.
    let s = step(&mut c, json!({ "op": "slack.grant", "token": "xoxe-1-TEST-refresh", "agentId": id, "origin": "https://yard.test" })).await.unwrap();
    assert_eq!(s["workspaces"][0]["teamId"], "T0TEST");
    let app = &s["agentApps"][&id];
    assert_eq!(app["step"], "install");
    assert_eq!(app["handle"], "reviewer");
    let manifest = &fake.state()["manifests"][0];
    assert_eq!(manifest["display_information"]["name"], "Reviewer");
    assert_eq!(manifest["features"]["bot_user"]["display_name"], "reviewer");
    assert_eq!(manifest["settings"]["socket_mode_enabled"], true);
    assert_eq!(manifest["oauth_config"]["redirect_urls"][0], "https://yard.test/slack/oauth");
    let url = app["authorizeUrl"].as_str().unwrap().to_string();
    assert!(url.contains("client_id=C0AGENT") && url.contains("redirect_uri=https%3A%2F%2Fyard.test%2Fslack%2Foauth"), "{url}");
    // No token or secret reaches a window.
    let shown = s.to_string();
    assert!(!shown.contains("xoxe") && !shown.contains("TEST-secret"), "{shown}");
    // A second app for the same agent is refused.
    assert!(step(&mut c, json!({ "op": "slack.app.create", "agentId": id, "teamId": "T0TEST", "origin": "" })).await.unwrap_err().contains("already has a Slack app"));

    // Slack sends the browser back; a made-up state is refused.
    let port = d.port;
    let forged = tokio::task::spawn_blocking(move || http_get(port, "/slack/oauth?code=TEST-code&state=forged").unwrap()).await.unwrap();
    assert!(forged.contains("This link has expired"));
    let state = url.split("&state=").nth(1).unwrap().to_string();
    let path = format!("/slack/oauth?code=TEST-code&state={state}");
    let page = tokio::task::spawn_blocking(move || http_get(port, &path).unwrap()).await.unwrap();
    assert!(page.contains("Reviewer is installed in Slack"), "{page}");
    // Once only.
    let path = format!("/slack/oauth?code=TEST-code&state={state}");
    assert!(tokio::task::spawn_blocking(move || http_get(port, &path).unwrap()).await.unwrap().contains("expired"));
    let s = slack_state(&mut c, "the app-level token step", |s| s["agentApps"][&id]["step"] == "appToken").await;
    assert_eq!(s["agentApps"][&id]["ownerName"], "TEST-Owner");
    // Whoever installed it may ask.
    assert!(s["allowed"].as_array().unwrap().iter().any(|p| p["id"] == "UOWNER"));

    // Its app-level token opens its own Socket Mode connection.
    assert!(step(&mut c, json!({ "op": "slack.app.appToken", "agentId": id, "token": "xoxb-agent-TEST" })).await.unwrap_err().contains("starts with xapp-"));
    step(&mut c, json!({ "op": "slack.app.appToken", "agentId": id, "token": "xapp-1-agent-TEST" })).await.unwrap();
    slack_state(&mut c, "its app connected", |s| s["agentApps"][&id]["state"] == "connected").await;

    // A mention of its own bot reaches it without naming it, and the answer
    // goes back through its app.
    let ts = fake.say(json!({ "app": "agent", "user": "UOWNER", "channel": "C0OPS", "text": "<@UAGENT> TEST-check the deploy" }));
    let st = fake.wait("its answer", |s| !FakeSlack::posts(s, &ts).is_empty() && s["reactions"][&ts] == json!(["white_check_mark"])).await;
    assert!(FakeSlack::posts(&st, &ts)[0].contains("TEST-check the deploy"));
    let sid = chat_of(&mut c, &id).await;
    let history = c.history(&sid).await;
    let m = history.iter().find(|e| e["type"] == "user_message").expect("the message in its conversation");
    assert_eq!(m["text"], "TEST-check the deploy");
    assert_eq!(m["origin"]["kind"], "slack");
    assert_eq!(m["origin"]["app"], id.as_str());
    assert!(st["calls"].as_array().unwrap().iter().any(|call| call["method"] == "chat.postMessage" && call["token"] == "bot"));

    // After a restart its app connects again on its own.
    drop(c);
    d.stop();
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    slack_state(&mut c, "its app connected again", |s| s["agentApps"][&id]["state"] == "connected").await;

    // Leaving Slack deletes its app; the agent stays.
    step(&mut c, json!({ "op": "slack.app.remove", "agentId": id })).await.unwrap();
    assert_eq!(fake.state()["deleted"], json!(["A0AGENT"]));
    let saved = std::fs::read_to_string(sb.data().join("slack.json")).unwrap();
    assert!(!saved.contains("xoxb-agent-TEST"));
}

/// A bare HTTP/1.1 POST to yardd: its status and body.
fn http_post(port: u16, path: &str, headers: &[(&str, &str)], body: &[u8]) -> (u16, String) {
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut head = format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\nConnection: close\r\n", body.len());
    for (k, v) in headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes()).unwrap();
    let _ = s.write_all(body);
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    let out = String::from_utf8_lossy(&out).to_string();
    let status = out.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    (status, out.split_once("\r\n\r\n").map(|(_, b)| b.to_string()).unwrap_or_default())
}

/// The head of a PNG `w`×`h` pixels: enough for yardd's check and the fake's.
fn png(w: u32, h: u32) -> String {
    use base64::Engine;
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    b.extend_from_slice(&w.to_be_bytes());
    b.extend_from_slice(&h.to_be_bytes());
    b.extend_from_slice(&[8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89]);
    base64::engine::general_purpose::STANDARD.encode(b)
}

#[tokio::test]
async fn with_a_public_address_an_agents_app_needs_only_the_configuration_token() {
    let fake = FakeSlack::start();
    let sb = Sandbox::new("slack-public");
    let api = fake.api();
    let env = [("YARD_SLACK_API", api.as_str()), ("YARD_ALLOW_ORIGIN", "https://yard.test"), ("YARD_SLACK_PUBLIC_URL", "https://slack.yard.test/")];
    let d = sb.start_with(&env);
    let port = d.port;
    let events = format!("http://127.0.0.1:{port}/slack/events");
    let mut c = d.client().await;
    ready(&mut c).await;
    assert_eq!(c.log[0]["slack"]["publicUrl"], "https://slack.yard.test");
    assert_eq!(c.log[0]["slack"]["publicUrlFixed"], true);
    assert_eq!(c.log[0]["slack"]["canRedirect"], true);
    // Set where Switchyard runs: the page cannot change it.
    assert!(step(&mut c, json!({ "op": "slack.publicUrl", "url": "https://other.test" })).await.unwrap_err().contains("YARD_SLACK_PUBLIC_URL"));
    let id = reviewer(&mut c).await;

    // The window is the desktop app's http page; the install still comes
    // back on its own, to the https address yardd serves its page on.
    let window = format!("http://127.0.0.1:{port}");
    let s = step(&mut c, json!({ "op": "slack.grant", "token": "xoxe-1-TEST-refresh", "agentId": id, "origin": window })).await.unwrap();
    let app = &s["agentApps"][&id];
    assert_eq!(app["step"], "install");
    assert_eq!(app["canRedirect"], true);
    assert_eq!(app["eventsUrl"], "https://slack.yard.test/slack/events");
    let manifest = &fake.state()["manifests"][0];
    assert_eq!(manifest["settings"]["socket_mode_enabled"], false);
    assert_eq!(manifest["settings"]["event_subscriptions"]["request_url"], "https://slack.yard.test/slack/events");
    assert_eq!(manifest["oauth_config"]["redirect_urls"], json!(["https://yard.test/slack/oauth", "https://slack.yard.test/slack/oauth"]));
    let url = app["authorizeUrl"].as_str().unwrap().to_string();
    assert!(url.contains("redirect_uri=https%3A%2F%2Fyard.test%2Fslack%2Foauth"), "{url}");
    // A window on the public address gets its own.
    let s = step(&mut c, json!({ "op": "slack.app.authorize", "agentId": id, "origin": "https://slack.yard.test" })).await.unwrap();
    assert!(s["agentApps"][&id]["authorizeUrl"].as_str().unwrap().contains("redirect_uri=https%3A%2F%2Fslack.yard.test%2Fslack%2Foauth"));
    let s = step(&mut c, json!({ "op": "slack.app.authorize", "agentId": id, "origin": window })).await.unwrap();
    let url = s["agentApps"][&id]["authorizeUrl"].as_str().unwrap().to_string();
    assert!(url.contains("redirect_uri=https%3A%2F%2Fyard.test%2Fslack%2Foauth"), "{url}");

    // Slack checks the events address: answered only when signed by the app.
    let challenge = |sign: &str| fake.request("POST", "/_fake/challenge", &json!({ "url": events, "sign": sign }));
    let good = challenge("good");
    assert_eq!(good["status"], 200, "{good}");
    assert!(good["body"].as_str().unwrap().contains("TEST-challenge-"));
    for sign in ["bad", "stale", "none"] {
        let r = challenge(sign);
        assert_eq!(r["status"], 401, "{sign}: {r}");
        assert!(!r["body"].as_str().unwrap().contains("TEST-challenge-"));
    }
    // Too large to be Slack's.
    let (status, _) = tokio::task::spawn_blocking(move || http_post(port, "/slack/events", &[("Content-Type", "application/json")], &vec![b' '; 2 << 20])).await.unwrap();
    assert_eq!(status, 413);
    // The probe the public address is checked with.
    let probe = tokio::task::spawn_blocking(move || http_get(port, "/slack/events").unwrap()).await.unwrap();
    assert!(probe.contains("Switchyard receives Slack's events here."));

    // The install: then it is ready, with no app-level token.
    let state = url.split("&state=").nth(1).unwrap().to_string();
    let path = format!("/slack/oauth?code=TEST-code&state={state}");
    let page = tokio::task::spawn_blocking(move || http_get(port, &path).unwrap()).await.unwrap();
    assert!(page.contains("Reviewer is installed in Slack") && page.contains("It is ready"), "{page}");
    let exchange = fake.state()["calls"].as_array().unwrap().iter().find(|c| c["method"] == "oauth.v2.access").cloned().unwrap();
    assert_eq!(exchange["params"]["redirect_uri"], "https://yard.test/slack/oauth");
    let s = slack_state(&mut c, "its app ready", |s| s["agentApps"][&id]["step"] == "ready").await;
    assert_eq!(s["agentApps"][&id]["state"], "events");
    assert_eq!(fake.state()["opens"], 0, "no Socket Mode connection");

    // A mention over HTTP reaches it; the answer goes back through its app.
    let say = |extra: Value| {
        let mut msg = json!({ "url": events, "user": "UOWNER", "channel": "C0OPS", "text": "<@UAGENT> TEST-over http" });
        for (k, v) in extra.as_object().unwrap() {
            msg[k] = v.clone();
        }
        fake.request("POST", "/_fake/event", &msg)
    };
    let r = say(json!({}));
    assert!(r["results"].as_array().unwrap().iter().all(|x| x["status"] == 200), "{r}");
    let ts = r["ts"].as_str().unwrap().to_string();
    let st = fake.wait("its answer", |s| !FakeSlack::posts(s, &ts).is_empty() && s["reactions"][&ts] == json!(["white_check_mark"])).await;
    assert_eq!(FakeSlack::posts(&st, &ts).len(), 1);
    assert!(FakeSlack::posts(&st, &ts)[0].contains("TEST-over http"));
    let sid = chat_of(&mut c, &id).await;
    let history = c.history(&sid).await;
    let m = history.iter().find(|e| e["type"] == "user_message").expect("the message in its conversation");
    assert_eq!(m["text"], "TEST-over http");
    assert_eq!(m["origin"]["app"], id.as_str());
    slack_state(&mut c, "its app connected", |s| s["agentApps"][&id]["state"] == "connected").await;

    // Unsigned, mis-signed and replayed requests reach nobody.
    for sign in ["bad", "stale", "none"] {
        let r = say(json!({ "sign": sign, "text": format!("<@UAGENT> TEST-forged {sign}") }));
        assert!(r["results"].as_array().unwrap().iter().all(|x| x["status"] == 401), "{sign}: {r}");
    }
    // A retry of an event already handled is acknowledged, not handled again.
    let r = say(json!({ "channel": "D0OWNER", "text": "TEST-once only", "event_id": "EvOnce" }));
    let once = r["ts"].as_str().unwrap().to_string();
    fake.wait("the answer to once", |s| s["reactions"][&once] == json!(["white_check_mark"])).await;
    let again = say(json!({ "channel": "D0OWNER", "text": "TEST-once only", "event_id": "EvOnce", "retry": 1, "ts": once }));
    assert_eq!(again["results"][0]["status"], 200, "{again}");
    c.drain_for(Duration::from_millis(800)).await;
    let history = c.history(&sid).await;
    let texts: Vec<&str> = history.iter().filter(|e| e["type"] == "user_message").filter_map(|e| e["text"].as_str()).collect();
    assert_eq!(texts.iter().filter(|t| t.contains("TEST-once only")).count(), 1, "{texts:?}");
    assert!(!texts.iter().any(|t| t.contains("TEST-forged")), "{texts:?}");

    // After a restart it still needs no connection.
    drop(c);
    d.stop();
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    let s = slack_state(&mut c, "its app ready again", |s| s["agentApps"][&id]["step"] == "ready").await;
    assert_eq!(s["agentApps"][&id]["state"], "events");
    assert_eq!(fake.state()["opens"], 0);
    // Nothing secret reaches a window or the log.
    let shown = s.to_string();
    assert!(!shown.contains("TEST-signing-secret") && !shown.contains("TEST-secret"), "{shown}");
    assert!(!d.log_text().contains("TEST-signing-secret"));
}

#[tokio::test]
async fn an_agents_slack_app_follows_its_name_and_icon_and_says_when_it_cannot() {
    let fake = FakeSlack::start();
    let sb = Sandbox::new("slack-rename");
    let api = fake.api();
    let env = [("YARD_SLACK_API", api.as_str())];
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    ready(&mut c).await;
    // No https page and no public address: the bot token is pasted.
    assert_eq!(c.log[0]["slack"]["canRedirect"], false);
    let id = reviewer(&mut c).await;
    let s = step(&mut c, json!({ "op": "slack.grant", "token": "xoxe-1-TEST-refresh", "agentId": id, "origin": "http://127.0.0.1" })).await.unwrap();
    assert_eq!(s["agentApps"][&id]["canRedirect"], false);
    assert!(s["agentApps"][&id]["authorizeUrl"].is_null());
    assert!(fake.state()["manifests"][0]["oauth_config"]["redirect_urls"].is_null());
    step(&mut c, json!({ "op": "slack.app.botToken", "agentId": id, "token": "xoxb-agent-TEST" })).await.unwrap();
    assert!(fake.state()["updates"].as_array().unwrap().is_empty());

    // A new name and purpose: Slack's app follows, the handle made from the name.
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "id": id, "name": "Release-Auditor", "about": "TEST-audits releases" }, "ref": "r1" })).await;
    c.wait_after(from, "renamed", |m| m["op"] == "agent.saved").await;
    let st = fake.wait("the manifest update", |s| !s["updates"].as_array().unwrap().is_empty()).await;
    let update = &st["updates"][0];
    assert_eq!(update["app_id"], "A0AGENT");
    assert_eq!(update["manifest"]["display_information"]["name"], "Release-Auditor");
    assert_eq!(update["manifest"]["display_information"]["description"], "TEST-audits releases");
    assert_eq!(update["manifest"]["features"]["bot_user"]["display_name"], "release-auditor");
    assert_eq!(update["manifest"]["settings"]["socket_mode_enabled"], true);
    slack_state(&mut c, "the new handle", |s| s["agentApps"][&id]["handle"] == "release-auditor").await;
    // Its other settings alone change nothing in Slack.
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "id": id, "instructions": "TEST-be brief" }, "ref": "r2" })).await;
    c.wait_after(from, "saved", |m| m["op"] == "agent.saved").await;

    // Its icon, as the page drew it.
    assert!(step(&mut c, json!({ "op": "slack.app.icon", "agentId": id, "key": "k1", "png": png(256, 256) })).await.unwrap_err().contains("512 to 2000"));
    assert!(step(&mut c, json!({ "op": "slack.app.icon", "agentId": id, "key": "k1", "png": "PHN2Zz4=" })).await.unwrap_err().contains("not a PNG"));
    let s = step(&mut c, json!({ "op": "slack.app.icon", "agentId": id, "key": "k1", "png": png(1024, 1024) })).await.unwrap();
    assert_eq!(s["agentApps"][&id]["iconKey"], "k1");
    let icons = fake.state()["icons"].clone();
    assert_eq!(icons.as_array().unwrap().len(), 1, "{icons}");
    assert_eq!(icons[0]["app_id"], "A0AGENT");
    assert_eq!(icons[0]["width"], 1024);
    assert!(icons[0]["png"] == true && icons[0]["type"] == "image/png");
    // The same picture again is not sent again.
    step(&mut c, json!({ "op": "slack.app.icon", "agentId": id, "key": "k1", "png": png(1024, 1024) })).await.unwrap();
    assert_eq!(fake.state()["icons"].as_array().unwrap().len(), 1);
    assert_eq!(fake.state()["updates"].as_array().unwrap().len(), 1);

    // The configuration token lapses: the rename is saved all the same, and
    // its dialog says Slack was not told.
    fake.request("POST", "/_fake/lapse", &json!({}));
    let from = c.mark();
    c.send(json!({ "op": "agent.save", "agent": { "id": id, "name": "Inspector" }, "ref": "r3" })).await;
    let saved = c.wait_after(from, "renamed again", |m| m["op"] == "agent.saved" || (m["op"] == "error" && m["for"] == "agent.save")).await;
    assert_eq!(saved["op"], "agent.saved");
    let s = slack_state(&mut c, "the update failed", |s| s["agentApps"][&id]["syncError"].is_string()).await;
    let error = s["agentApps"][&id]["syncError"].as_str().unwrap().to_string();
    assert!(error.contains("configuration token"), "{error}");
    let s = step(&mut c, json!({ "op": "slack.app.icon", "agentId": id, "key": "k2", "png": png(1024, 1024) })).await.unwrap();
    assert!(s["agentApps"][&id]["syncError"].is_string());
    assert_eq!(fake.state()["updates"].as_array().unwrap().len(), 1);

    // A new configuration token sends what waited: the name and the icon.
    let s = step(&mut c, json!({ "op": "slack.grant", "token": "xoxe-1-TEST-again", "agentId": id, "origin": "" })).await.unwrap();
    assert!(s["agentApps"][&id]["syncError"].is_null(), "{s}");
    assert_eq!(s["agentApps"][&id]["handle"], "inspector");
    let st = fake.state();
    assert_eq!(st["manifests"].as_array().unwrap().len(), 1, "no second app");
    assert_eq!(st["updates"].as_array().unwrap().last().unwrap()["manifest"]["display_information"]["name"], "Inspector");
    assert_eq!(st["icons"].as_array().unwrap().len(), 2);

    // The public address, set from the page: checked through itself.
    assert!(step(&mut c, json!({ "op": "slack.publicUrl", "url": "http://pub.test" })).await.unwrap_err().contains("https://"));
    let s = step(&mut c, json!({ "op": "slack.publicUrl", "url": "https://pub.test/" })).await.unwrap();
    assert_eq!(s["publicUrl"], "https://pub.test");
    assert_eq!(s["publicUrlFixed"], false);
    assert_eq!(s["canRedirect"], true);
    assert_eq!(fake.state()["updates"].as_array().unwrap().last().unwrap()["manifest"]["oauth_config"]["redirect_urls"], json!(["https://pub.test/slack/oauth"]));
    let s = slack_state(&mut c, "the address checked", |s| s["publicCheck"].is_string()).await;
    assert!(s["publicCheck"].as_str().unwrap().contains("https://pub.test/slack/events"), "{s}");

    // An app made before names followed (no manifest kept) is updated when yardd starts.
    drop(c);
    d.stop();
    let file = sb.data().join("slack.json");
    let mut saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    saved["apps"][0].as_object_mut().unwrap().remove("manifest");
    std::fs::write(&file, saved.to_string()).unwrap();
    let before = fake.state()["updates"].as_array().unwrap().len();
    let d = sb.start_with(&env);
    let _c = d.client().await;
    let st = fake.wait("the update at start", |s| s["updates"].as_array().unwrap().len() == before + 1).await;
    let update = &st["updates"][before]["manifest"];
    assert_eq!(update["features"]["bot_user"]["display_name"], "inspector");
    // Its install may now come back to the public address; its events stay on
    // Socket Mode, whose token it has.
    assert_eq!(update["oauth_config"]["redirect_urls"], json!(["https://pub.test/slack/oauth"]));
    assert_eq!(update["settings"]["socket_mode_enabled"], true);
}
