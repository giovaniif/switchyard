//! Where a message into a conversation came from, and how its answer gets back.
//!
//! A message the owner typed has no origin. Anything else carries one, kept on
//! its `user_message` event (and on the queued message before that) and shown
//! in the thread as a badge above the message:
//!
//! - `{ kind: "agent", agentId, name, hops }`: another agent's `message_agent`.
//! - `{ kind: "agentReply", agentId, name, hops }`: the answer to one, delivered
//!   back into the sender's conversation.
//! - Slack will add `{ kind: "slack", ... }`.
//!
//! A session's `Meta::answering` keeps the origins of the messages its running
//! turn answers (the message that started it and any steered in). When the
//! turn ends, `App::emit` hands them, with the turn's final answer or how it
//! failed, to `reply`: one function with an arm per kind that wants an answer.

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{host_context::clip, state::App};

/// The longest chain of agents messaging agents: a message that would be the
/// fourth hop is refused.
pub const MAX_HOPS: u64 = 3;
/// An answer handed back is clipped to this many bytes.
const REPLY_BYTES: usize = 12_000;

/// An answer owed to messages' origins.
pub struct Owed {
    /// The conversation that answered.
    pub sid: String,
    pub origins: Vec<Value>,
    /// How the turn ended (`completed`, `failed`, `interrupted`), or why there
    /// was none: `unread` (Stop took the message back from the queue) or
    /// `undelivered` (it could not be put into the conversation).
    pub status: String,
    /// The final answer, or the error.
    pub text: String,
}

impl Owed {
    /// What a turn that just ended owes: its last answer, or its last error.
    pub fn of_turn(sid: &str, origins: Vec<Value>, status: &str, events: &[Value]) -> Owed {
        let start = events.iter().rposition(|e| e["type"] == "turn_started").unwrap_or(0);
        let wanted = if status == "completed" { "assistant_message" } else { "error" };
        let field = if status == "completed" { "text" } else { "message" };
        let text = events[start..].iter().rev().find(|e| e["type"] == wanted).and_then(|e| e[field].as_str()).unwrap_or("").trim().to_string();
        Owed { sid: sid.to_string(), origins, status: status.to_string(), text }
    }
}

pub fn kind(origin: &Value) -> &str {
    origin["kind"].as_str().unwrap_or("user")
}

pub fn hops(origin: &Value) -> u64 {
    origin["hops"].as_u64().unwrap_or(0)
}

/// An origin that is not the owner typing: kept on the message and in `answering`.
pub fn is_tagged(origin: Option<&Value>) -> bool {
    origin.is_some_and(|o| o.is_object() && kind(o) != "user")
}

/// Origins whose sender waits for the answer. A reply is not answered back.
pub fn reply_able(origin: &Value) -> bool {
    matches!(kind(origin), "agent")
}

/// The hop count of a message a conversation sends now: one more than the
/// longest chain among the messages its turn answers.
pub fn next_hops(answering: &[Value]) -> u64 {
    answering.iter().map(hops).max().unwrap_or(0) + 1
}

/// What the engine reads for a message: the owner's words as typed; anything
/// else framed with where it came from, so the agent knows who is talking and
/// that its answer goes back. The thread shows only the text, with a badge.
pub fn engine_text(text: &str, origin: Option<&Value>) -> String {
    let Some(origin) = origin.filter(|o| is_tagged(Some(o))) else { return text.to_string() };
    let name = origin["name"].as_str().unwrap_or("another agent");
    match kind(origin) {
        "agent" => format!(
            "<message from=\"@{name}\">\n{text}\n</message>\n(From {name}, another agent in Switchyard, not the user. Your final answer to this message is sent back to {name}.)"
        ),
        "agentReply" => format!("<reply from=\"@{name}\">\n{text}\n</reply>\n({name}'s answer to the message you sent it with message_agent.)"),
        _ => text.to_string(),
    }
}

/// Hand each origin that waits for an answer what it is owed. One arm per kind.
pub async fn reply(app: &Arc<App>, owed: Owed) {
    let meta = app.meta(&owed.sid);
    let answerer = meta.as_ref().and_then(|m| m.agent.clone()).and_then(|id| app.agents.get(&id));
    for origin in owed.origins.iter().filter(|o| reply_able(o)) {
        match kind(origin) {
            "agent" => {
                let Some(to) = origin["agentId"].as_str() else { continue };
                let (from_id, from_name) = match &answerer {
                    Some(a) => (a.id.clone(), a.name.clone()),
                    None => (String::new(), origin["to"].as_str().unwrap_or("The agent").to_string()),
                };
                let text = answer_text(&from_name, &owed.status, &owed.text);
                let back = json!({ "kind": "agentReply", "agentId": from_id, "name": from_name, "hops": hops(origin) });
                if let Err(e) = crate::agents::deliver(app, to, &text, &back).await {
                    eprintln!("yardd: could not deliver {from_name}'s answer to agent {to}: {e:#}");
                }
            }
            _ => {}
        }
    }
}

/// The reply an agent gets: the answer, or one line on why there is none.
pub fn answer_text(name: &str, status: &str, text: &str) -> String {
    match status {
        "completed" if !text.is_empty() => clip(text, REPLY_BYTES),
        "completed" => format!("{name} finished without writing an answer."),
        "failed" if !text.is_empty() => format!("{name}'s turn failed before it answered: {}", clip(text, 2000)),
        "failed" => format!("{name}'s turn failed before it answered."),
        "interrupted" => format!("{name} was stopped before it answered."),
        "unread" => format!("{name} was stopped before it read your message."),
        _ => format!("Your message did not reach {name}: {}", clip(text, 2000)),
    }
}

/// Answers owed by turns that ended, handed out as they come.
pub async fn run(app: Arc<App>, mut rx: mpsc::UnboundedReceiver<Owed>) {
    while let Some(owed) = rx.recv().await {
        let app = app.clone();
        tokio::spawn(async move { reply(&app, owed).await });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_owes_its_last_answer_or_its_error() {
        let events = vec![
            json!({ "type": "turn_started" }),
            json!({ "type": "assistant_message", "text": "old" }),
            json!({ "type": "turn_completed", "status": "completed" }),
            json!({ "type": "turn_started" }),
            json!({ "type": "assistant_message", "text": "first" }),
            json!({ "type": "assistant_message", "text": " last " }),
            json!({ "type": "error", "message": "boom" }),
        ];
        assert_eq!(Owed::of_turn("s", vec![], "completed", &events).text, "last");
        assert_eq!(Owed::of_turn("s", vec![], "failed", &events).text, "boom");
        assert_eq!(Owed::of_turn("s", vec![], "interrupted", &events[..4]).text, "");
    }

    #[test]
    fn hops_grow_from_the_longest_chain_answered() {
        assert_eq!(next_hops(&[]), 1);
        let a = json!({ "kind": "agent", "hops": 1 });
        let r = json!({ "kind": "agentReply", "hops": 2 });
        assert_eq!(next_hops(&[a.clone(), r.clone()]), 3);
        assert!(reply_able(&a) && !reply_able(&r));
        assert!(!is_tagged(Some(&json!({ "kind": "user" }))) && !is_tagged(None) && is_tagged(Some(&a)));
    }

    #[test]
    fn the_engine_reads_who_a_message_is_from() {
        assert_eq!(engine_text("hi", None), "hi");
        let a = json!({ "kind": "agent", "agentId": "g1", "name": "Builder", "hops": 1 });
        let t = engine_text("check #12", Some(&a));
        assert!(t.starts_with("<message from=\"@Builder\">\ncheck #12\n</message>") && t.contains("sent back to Builder"));
        let r = json!({ "kind": "agentReply", "agentId": "g2", "name": "Reviewer", "hops": 1 });
        assert!(engine_text("done", Some(&r)).starts_with("<reply from=\"@Reviewer\">\ndone\n</reply>"));
    }

    #[test]
    fn a_missing_answer_says_why() {
        assert_eq!(answer_text("Rev", "completed", "ok"), "ok");
        assert!(answer_text("Rev", "failed", "model down").contains("failed before it answered: model down"));
        assert!(answer_text("Rev", "interrupted", "").contains("stopped before it answered"));
        assert!(answer_text("Rev", "unread", "").contains("before it read your message"));
    }
}
