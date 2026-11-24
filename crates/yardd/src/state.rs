use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, oneshot, OnceCell};

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
    pub id: String,
    pub title: String,
    /// Where the title came from: absent for the task's first line, `auto`
    /// when a model named the session from its conversation (see `naming`),
    /// `user` when typed in. Only the first two are ever replaced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_source: Option<String>,
    pub engine: String,
    pub model: String,
    pub effort: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    pub cwd: String,
    pub status: String,
    pub host: String,
    /// Set when yardd created a worktree for this session.
    pub branch: Option<String>,
    /// The checkout the worktree was cut from.
    pub repo: Option<String>,
    /// The session this one was handed off from. Rows saved before hand-off
    /// existed have no such key.
    #[serde(default)]
    pub parent: Option<String>,
    /// How much the engine may do unasked: `ask`, `acceptEdits`, `plan` or
    /// `full`. Rows saved before modes existed ran as `ask`.
    #[serde(default = "default_permission")]
    pub permission: String,
    /// The account the session runs under (see `accounts`): `default`, or the
    /// id of an added one. Fixed for the session's life. Rows saved before
    /// accounts existed ran on Default.
    #[serde(default = "default_account")]
    pub account: String,
    /// Kept at the top of the sidebar.
    #[serde(default)]
    pub pinned: bool,
    /// Moved out of the list into Archived; still there to open or restore.
    #[serde(default)]
    pub archived: bool,
    /// The sidebar group this session sits in. A group exists only while a
    /// session names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// The automation that started this session. Such runs stay out of the
    /// sidebar (see the Automations page) and archive themselves when their
    /// first turn ends.
    #[serde(default)]
    pub automation: Option<String>,
    /// The agent this session is a chat with (see `agents`): its first turn
    /// carries the agent's preamble, and the row and thread show who it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    /// An agent's conversation needs a new engine thread before its next
    /// turn, and why (see `agents::prepare`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fresh_context: Option<String>,
    /// The agent's instructions or memory changed: its next turn carries them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub notes_changed: bool,
    /// The start of an agent's last answer, for its card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    /// An agent asked for the owner (`notify_owner`): `{ message, at }`, kept
    /// until a window opens the conversation (`agent.seen`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_you: Option<Value>,
    /// Where the messages the running (or last) turn answers came from, when
    /// the owner did not type them (see `origins`). Set when a turn starts,
    /// grown by steered messages, handed to the reply hook when it ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answering: Vec<Value>,
    /// When an automation run's first turn ended: the run took this minus
    /// `created_at`. `updated_at` is no good for that, it moves on any later
    /// change (a pin, a rename, a reply in the reopened run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<u64>,
    /// When the session's latest turn ended, and how (`completed`, `failed`,
    /// `interrupted`): a window marks an answer its user has not opened yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_ended_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_turn: Option<String>,
    /// Messages sent while a turn ran and not yet given to the agent, oldest
    /// first: `{ id, text, attachments, at }`. Like Claude Code's queue, they
    /// go to the agent at its next step (see `Delivery`); Stop hands them back.
    #[serde(default)]
    pub pending: Vec<Value>,
    pub created_at: u64,
    pub updated_at: u64,
}

fn default_permission() -> String {
    "ask".to_string()
}

fn default_account() -> String {
    crate::accounts::DEFAULT.to_string()
}

pub const PERMISSIONS: [&str; 4] = ["ask", "acceptEdits", "plan", "full"];

/// Where a new session runs when it gets its own worktree.
pub struct Worktree {
    pub branch: String,
    pub repo: String,
}

pub enum Backend {
    Pending,
    Codex {
        thread_id: String,
        turn_id: Option<String>,
    },
    Claude {
        tx: mpsc::UnboundedSender<String>,
        /// Dropping this (with the session) kills the sidecar.
        _stop: oneshot::Sender<()>,
    },
    /// A session on an ACP agent (OpenCode, Grok; see `acp`).
    Acp {
        session_id: String,
        /// Which agent process knows the session; another one must resume it first.
        epoch: u64,
        /// The running prompt, so an answer that comes after Stop ended the
        /// turn here says nothing.
        turn: Option<u64>,
        /// Model, effort and permission last set on the agent's session.
        applied: Option<(String, String, String)>,
    },
}

pub struct Session {
    pub meta: Meta,
    pub events: Vec<Value>,
    pub backend: Backend,
    /// The engine's own id for this conversation (Codex thread id, Claude Code
    /// session id). Saved so a session restored after a restart can resume.
    pub engine_ref: Option<String>,
}

/// When queued messages reach the agent.
#[derive(Debug)]
pub enum Delivery {
    /// A step of the running turn ended: give them to the agent now, in this
    /// turn (Codex `turn/steer`; Claude Code folds a message that arrives
    /// mid-turn into it).
    Steer,
    SteerOne(String),
    /// The turn completed with no step left to join: they are the next turn.
    NextTurn,
}

pub struct App {
    pub root: PathBuf,
    pub port: u16,
    pub tx: broadcast::Sender<String>,
    pub sessions: Mutex<HashMap<String, Session>>,
    /// One `codex app-server` per account, started on first use.
    pub codex: Mutex<HashMap<String, Arc<OnceCell<Arc<crate::codex::Client>>>>>,
    /// One ACP agent per engine and account (`acp::key`), started on first use.
    pub acp: Mutex<HashMap<String, Arc<OnceCell<Arc<crate::acp::Client>>>>>,
    pub accounts: crate::accounts::Accounts,
    pub models: Mutex<Value>,
    pub usage: Mutex<Value>,
    pub host: Mutex<Value>,
    pub cpu_history: Mutex<VecDeque<f32>>,
    pub terms: crate::pty::Terms,
    pub guard: crate::guard::Guard,
    pub store: crate::store::Store,
    /// Engine threads yardd runs for itself (see `private`).
    pub private: crate::private::Private,
    pub agents: crate::agents::Agents,
    pub remotes: crate::remote::Remotes,
    pub automations: crate::automations::Automations,
    pub logins: crate::login::Logins,
    /// Connectors processes and replies owed by sidecars (see mcp.rs).
    pub mcp: crate::mcp::Connectors,
    pub mobile: crate::mobile::Mobile,
    /// Slack: the connection, its settings and posts owed to threads.
    pub slack: crate::slack::Slack,
    /// Agents' own tools: per-conversation keys and rate limits (see agent_tools).
    pub tools: crate::agent_tools::Tools,
    /// Sessions with queued messages to deliver; drained by a task in main
    /// (emit cannot talk to an engine itself).
    next_turns: mpsc::UnboundedSender<(String, Delivery)>,
    next_turns_rx: Mutex<Option<mpsc::UnboundedReceiver<(String, Delivery)>>>,
    /// Sessions to name from their conversation (see `naming`).
    naming: mpsc::UnboundedSender<String>,
    naming_rx: Mutex<Option<mpsc::UnboundedReceiver<String>>>,
    /// Turns that ended owing an answer to where their messages came from
    /// (see `origins::reply`); drained by a task in main.
    replies: mpsc::UnboundedSender<crate::origins::Owed>,
    replies_rx: Mutex<Option<mpsc::UnboundedReceiver<crate::origins::Owed>>>,
    next: AtomicU64,
}

/// Plan usage as last reported, kept in `usage.json` across restarts: Claude
/// reports its limits only during a turn, so without it a restart blanks the
/// meters and a usage-capped automation cannot skip. Each window carries its
/// `resetsAt`, so a window that reset meanwhile reads as unknown, as before.
fn load_usage(data: &std::path::Path) -> Value {
    std::fs::read(data.join("usage.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}))
}

fn save_usage(data: &std::path::Path, usage: &Value) {
    // Written aside, then renamed, so a crash never leaves half a file.
    let tmp = data.join("usage.json.tmp");
    if std::fs::write(&tmp, usage.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, data.join("usage.json"));
    }
}

impl App {
    pub fn new(root: PathBuf, port: u16, store: crate::store::Store) -> Self {
        let (tx, _) = broadcast::channel(2048);
        let (next_turns, next_turns_rx) = mpsc::unbounded_channel();
        let (naming, naming_rx) = mpsc::unbounded_channel();
        let (replies, replies_rx) = mpsc::unbounded_channel();
        App {
            root,
            port,
            tx,
            sessions: Mutex::new(HashMap::new()),
            codex: Mutex::new(HashMap::new()),
            acp: Mutex::new(HashMap::new()),
            accounts: crate::accounts::Accounts::load(crate::store::data_dir()),
            models: Mutex::new(json!({ "claude": claude_models(), "codex": [] })),
            usage: Mutex::new(load_usage(&crate::store::data_dir())),
            host: Mutex::new(Value::Null),
            cpu_history: Mutex::new(VecDeque::new()),
            terms: crate::pty::Terms::default(),
            guard: crate::guard::Guard::default(),
            store,
            private: crate::private::Private::default(),
            agents: crate::agents::Agents::load(crate::store::data_dir()),
            remotes: crate::remote::Remotes::new(crate::store::data_dir()),
            automations: crate::automations::Automations::load(crate::store::data_dir()),
            logins: crate::login::Logins::default(),
            mcp: crate::mcp::Connectors::default(),
            mobile: crate::mobile::Mobile::load(crate::store::data_dir()),
            slack: crate::slack::Slack::load(crate::store::data_dir()),
            tools: crate::agent_tools::Tools::default(),
            next_turns,
            next_turns_rx: Mutex::new(Some(next_turns_rx)),
            naming,
            naming_rx: Mutex::new(Some(naming_rx)),
            replies,
            replies_rx: Mutex::new(Some(replies_rx)),
            next: AtomicU64::new(1),
        }
    }

    /// The queue of sessions with messages to deliver. Taken once, by main.
    pub fn take_next_turns(&self) -> mpsc::UnboundedReceiver<(String, Delivery)> {
        self.next_turns_rx.lock().unwrap().take().expect("next_turns taken twice")
    }

    /// The queue of sessions to name. Taken once, by main.
    pub fn take_naming(&self) -> mpsc::UnboundedReceiver<String> {
        self.naming_rx.lock().unwrap().take().expect("naming taken twice")
    }

    /// Turns that owe an answer. Taken once, by main.
    pub fn take_replies(&self) -> mpsc::UnboundedReceiver<crate::origins::Owed> {
        self.replies_rx.lock().unwrap().take().expect("replies taken twice")
    }

    /// Hand an answer owed to messages' origins to the reply hook.
    pub fn owe(&self, owed: crate::origins::Owed) {
        let _ = self.replies.send(owed);
    }

    /// Queue a message sent while a turn runs. `origin`: where it came from,
    /// when the owner did not type it (see `origins`).
    pub fn queue_message(&self, sid: &str, text: &str, attachments: &[Value], origin: Option<&Value>) -> String {
        let id = self.id("q");
        let mut item = json!({ "id": id, "text": text, "attachments": attachments, "at": now_ms() });
        if let Some(origin) = origin {
            item["origin"] = origin.clone();
        }
        self.update_meta(sid, |m| m.pending.push(item));
        id
    }

    /// Take one queued message back, if it has not reached the agent yet.
    pub fn take_queued(&self, sid: &str, id: &str, only_running: bool) -> Option<Value> {
        let mut taken = None;
        self.update_meta(sid, |m| {
            if only_running && m.status != "running" {
                return;
            }
            if let Some(i) = m.pending.iter().position(|p| p["id"] == id) {
                taken = Some(m.pending.remove(i));
            }
        });
        taken
    }

    pub fn steer_queued(&self, sid: &str, id: &str) {
        let _ = self.next_turns.send((sid.to_string(), Delivery::SteerOne(id.to_string())));
    }

    /// Take every queued message, when the session's status passes `ok`.
    pub fn take_all_queued(&self, sid: &str, ok: impl Fn(&str) -> bool) -> Vec<Value> {
        {
            let map = self.sessions.lock().unwrap();
            match map.get(sid) {
                Some(s) if ok(&s.meta.status) && !s.meta.pending.is_empty() => {}
                _ => return Vec::new(),
            }
        }
        let mut taken = Vec::new();
        self.update_meta(sid, |m| {
            if ok(&m.status) {
                taken = std::mem::take(&mut m.pending);
            }
        });
        taken
    }

    /// Put messages back at the front of the queue (the engine refused them).
    pub fn requeue(&self, sid: &str, items: Vec<Value>) {
        self.update_meta(sid, |m| {
            let rest = std::mem::take(&mut m.pending);
            m.pending = items.into_iter().chain(rest).collect();
        });
    }

    /// The Codex client's slot for an account.
    pub fn codex_cell(&self, account: &str) -> Arc<OnceCell<Arc<crate::codex::Client>>> {
        self.codex.lock().unwrap().entry(account.to_string()).or_default().clone()
    }

    /// The account's app-server, if one is running.
    pub fn codex_running(&self, account: &str) -> Option<Arc<crate::codex::Client>> {
        self.codex.lock().unwrap().get(account).and_then(|c| c.get().cloned())
    }

    pub fn broadcast_accounts(&self) {
        self.broadcast(json!({ "op": "accounts", "accounts": self.accounts.list() }));
    }

    /// A short id that is unique within this yardd run.
    pub fn id(&self, prefix: &str) -> String {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        format!("{prefix}{:x}{:x}", now_ms(), n)
    }

    pub fn broadcast(&self, msg: Value) {
        let _ = self.tx.send(msg.to_string());
    }

    pub fn new_session(
        &self,
        engine: &str,
        model: &str,
        effort: &str,
        cwd: &str,
        title: &str,
        worktree: Option<Worktree>,
        account: &str,
    ) -> String {
        self.insert_session(engine, model, effort, cwd, title, worktree, false, account, None, None)
            .expect("a non-exclusive insert cannot be refused")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_session(
        &self,
        engine: &str,
        model: &str,
        effort: &str,
        cwd: &str,
        title: &str,
        worktree: Option<Worktree>,
        exclusive: bool,
        account: &str,
        automation: Option<&str>,
        agent: Option<&str>,
    ) -> anyhow::Result<String> {
        let id = self.id("s");
        let now = now_ms();
        let (branch, repo) = match worktree {
            Some(w) => (Some(w.branch), Some(w.repo)),
            None => (None, None),
        };
        let meta = Meta {
            id: id.clone(),
            title: title.to_string(),
            title_source: None,
            engine: engine.to_string(),
            model: model.to_string(),
            effort: effort.to_string(),
            service_tier: None,
            cwd: cwd.to_string(),
            status: "running".to_string(),
            host: "local".to_string(),
            branch,
            repo,
            parent: None,
            permission: default_permission(),
            account: account.to_string(),
            pinned: false,
            archived: false,
            group: None,
            automation: automation.map(str::to_string),
            agent: agent.map(str::to_string),
            fresh_context: None,
            notes_changed: false,
            last_message: None,
            needs_you: None,
            answering: Vec::new(),
            finished_at: None,
            turn_ended_at: None,
            last_turn: None,
            pending: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        {
            let mut map = self.sessions.lock().unwrap();
            if exclusive {
                let here = same_folder_key(cwd);
                let busy = map.values().find(|s| {
                    matches!(s.meta.status.as_str(), "running" | "needs_approval" | "queued")
                        && same_folder_key(&s.meta.cwd) == here
                });
                // Only a folder that cannot have a worktree (or a review in a
                // session's own one) gets here, so a worktree is no way out:
                // name the session so the user can find it.
                if let Some(s) = busy {
                    let who = match s.meta.title.trim() {
                        "" => "Another session".to_string(),
                        t => format!("\"{t}\""),
                    };
                    anyhow::bail!("{who} is working in this folder. Stop it or let it finish first.");
                }
            }
            self.store.meta(&meta);
            // Broadcast under the lock too, so no event of this session can reach a client first.
            self.broadcast(json!({ "op": "session", "session": meta.clone() }));
            map.insert(
                id.clone(),
                Session {
                    meta,
                    events: Vec::new(),
                    backend: Backend::Pending,
                    engine_ref: None,
                },
            );
        }
        Ok(id)
    }

    pub fn meta(&self, sid: &str) -> Option<Meta> {
        self.sessions.lock().unwrap().get(sid).map(|s| s.meta.clone())
    }

    /// Change session fields and tell every client. Returns false if it was removed.
    pub fn update_meta(&self, sid: &str, f: impl FnOnce(&mut Meta)) -> bool {
        let mut map = self.sessions.lock().unwrap();
        let Some(s) = map.get_mut(sid) else { return false };
        f(&mut s.meta);
        s.meta.updated_at = now_ms();
        self.store.meta(&s.meta);
        // Under the lock, like `emit`: clients see changes in the order they were made.
        self.broadcast(json!({ "op": "session", "session": s.meta }));
        true
    }

    /// Append a normalized event to the session log and fan it out.
    /// Session status is derived here so both engines behave the same.
    pub fn emit(&self, sid: &str, mut event: Value) {
        if crate::private::internal(sid) {
            let mut map = self.sessions.lock().unwrap();
            let Some(session) = map.get_mut(sid) else { return };
            if event["type"] == "turn_completed" { session.meta.status = "idle".into(); }
            let _ = self.private.events.send((sid.to_string(), event));
            return;
        }
        // A widget's HTML and data go to a file; the log keeps its id.
        crate::widgets::take(self, &mut event);
        // Turn boundaries carry the time, so the UI can say how long one took;
        // approvals too, so the time spent waiting for the user is left out.
        let timed = ["turn_started", "turn_completed", "approval_request", "approval_resolved"];
        if event["type"].as_str().is_some_and(|t| timed.contains(&t)) && event["at"].is_null() {
            event["at"] = json!(now_ms());
        }
        let status = match event["type"].as_str() {
            Some("turn_started") | Some("approval_resolved") => Some("running"),
            Some("approval_request") => Some("needs_approval"),
            Some("queued") => Some("queued"),
            Some("turn_completed") => Some("idle"),
            Some("fatal") => Some("error"),
            _ => None,
        };
        // A step ending is the agent's next chance to read queued messages;
        // a completed turn sends what is left as the next one. After Stop or
        // a failure the user decides.
        let step = matches!(event["type"].as_str(), Some("tool_started") | Some("tool_completed"));
        let turn_completed = event["type"] == "turn_completed";
        let completed = turn_completed && event["status"] == "completed";
        let turn_end = turn_completed.then(|| event["status"].as_str().map(str::to_string)).flatten();
        let next = {
            let mut map = self.sessions.lock().unwrap();
            let Some(s) = map.get_mut(sid) else { return };
            let seq = s.events.len();
            // Written and broadcast under the lock, so the disk and every
            // client see events and status changes in log order. Sent after
            // the lock, a status from one thread could overtake a newer one
            // from another and leave the UI showing `running` on an idle session.
            self.store.event(sid, &event);
            self.broadcast(json!({ "op": "event", "sessionId": sid, "seq": seq, "event": event }));
            // An agent's card quotes its last answer; a context that ran out
            // is replaced before the conversation's next turn.
            if let Some(agent) = s.meta.agent.as_deref() {
                if event["type"] == "assistant_message" {
                    let line: String = event["text"].as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ");
                    s.meta.last_message = Some(line.chars().take(160).collect());
                } else if event["type"] == "error" && !self.agents.is_builtin(agent) && crate::agents::context_ran_out(event["message"].as_str().unwrap_or("")) {
                    s.meta.fresh_context = Some("the engine's context was full".into());
                }
            }
            s.events.push(event);
            s.meta.updated_at = now_ms();
            if turn_completed {
                s.meta.turn_ended_at = Some(s.meta.updated_at);
                s.meta.last_turn = turn_end.clone();
                // Whoever sent the messages this turn answered gets its answer.
                if !s.meta.answering.is_empty() {
                    let origins = std::mem::take(&mut s.meta.answering);
                    let status = turn_end.clone().unwrap_or_else(|| "completed".into());
                    let _ = self.replies.send(crate::origins::Owed::of_turn(sid, origins, &status, &s.events));
                }
            }
            // Whoever waits in Slack hears, once a turn, that the turn waits on an approval.
            if s.events.last().is_some_and(|e| e["type"] == "approval_request") {
                let origins: Vec<Value> = s.meta.answering.iter().filter(|o| crate::origins::wants_approval_note(o)).cloned().collect();
                let start = s.events.iter().rposition(|e| e["type"] == "turn_started").unwrap_or(0);
                if !origins.is_empty() && s.events[start..].iter().filter(|e| e["type"] == "approval_request").count() == 1 {
                    let text = s.events.last().and_then(|e| e["title"].as_str()).unwrap_or("").to_string();
                    let _ = self.replies.send(crate::origins::Owed { sid: sid.to_string(), origins, status: "approval".into(), text });
                }
            }
            let moved = status.is_some_and(|st| s.meta.status != st);
            if let Some(st) = status {
                s.meta.status = st.to_string();
            }
            if moved || turn_completed {
                self.store.meta(&s.meta);
                self.broadcast(json!({ "op": "session", "session": s.meta }));
            }
            // A run of an automation is archived when its first turn ends, so
            // runs do not pile up in the sidebar. Later turns (someone
            // reopened it) and pinned runs are left alone.
            if turn_completed
                && s.meta.automation.is_some()
                && s.events.iter().filter(|e| e["type"] == "turn_completed").count() == 1
            {
                s.meta.finished_at = Some(s.meta.updated_at);
                if !s.meta.pinned {
                    s.meta.archived = true;
                }
                self.store.meta(&s.meta);
                self.broadcast(json!({ "op": "session", "session": s.meta }));
            }
            if completed {
                let done = s.events.iter().filter(|e| e["type"] == "turn_completed" && e["status"] == "completed").count();
                if crate::naming::due(&s.meta, done) {
                    let _ = self.naming.send(sid.to_string());
                }
            }
            if s.meta.pending.is_empty() {
                None
            } else if completed {
                Some(Delivery::NextTurn)
            } else if step && s.meta.status == "running" {
                Some(Delivery::Steer)
            } else {
                None
            }
        };
        if let Some(when) = next {
            let _ = self.next_turns.send((sid.to_string(), when));
        }
    }

    /// Remember the engine's id for this session, here and on disk.
    pub fn set_engine_ref(&self, sid: &str, engine_ref: &str) {
        let mut map = self.sessions.lock().unwrap();
        let Some(s) = map.get_mut(sid) else { return };
        if s.engine_ref.as_deref() != Some(engine_ref) {
            s.engine_ref = Some(engine_ref.to_string());
            // A restricted thread starts fresh every turn and never resumes.
            let fresh = crate::private::internal(sid) || s.meta.agent.as_deref().is_some_and(|a| self.agents.is_builtin(a));
            if !fresh {
                self.store.engine_ref(sid, engine_ref);
            }
        }
    }

    /// The saved engine id, but only while no engine is attached: that is a
    /// session restored from disk that has not resumed yet.
    pub fn resumable(&self, sid: &str) -> Option<String> {
        let map = self.sessions.lock().unwrap();
        let s = map.get(sid)?;
        match s.backend {
            Backend::Pending => s.engine_ref.clone(),
            _ => None,
        }
    }

    /// Put sessions read from disk back in memory. A turn that was open when
    /// yardd died can never finish, so it is closed out as failed. A queued
    /// session never started, so it goes back in the queue.
    pub fn restore(&self, loaded: Vec<crate::store::Loaded>) {
        let mut interrupted = Vec::new();
        let mut queued = Vec::new();
        {
            let mut map = self.sessions.lock().unwrap();
            for l in loaded {
                if l.meta.status == "queued" {
                    queued.push((l.meta.created_at, l.meta.id.clone(), queued_reason(&l.events)));
                } else if l.meta.status != "idle" {
                    interrupted.push(l.meta.id.clone());
                }
                map.insert(
                    l.meta.id.clone(),
                    Session {
                        meta: l.meta,
                        events: l.events,
                        backend: Backend::Pending,
                        engine_ref: l.engine_ref,
                    },
                );
            }
        }
        queued.sort();
        for (_, sid, reason) in queued {
            self.guard.enqueue(&sid, &reason);
        }
        for sid in interrupted {
            self.emit(&sid, json!({ "type": "error", "message": "yardd restarted during this turn." }));
            self.emit(&sid, json!({ "type": "turn_completed", "status": "failed" }));
        }
    }

    /// Forget a session here and on disk. Dropping its backend stops a Claude
    /// sidecar; the caller handles Codex and the terminal.
    pub fn remove_session(&self, sid: &str) -> bool {
        let removed = {
            let mut map = self.sessions.lock().unwrap();
            let removed = map.remove(sid);
            if removed.is_some() {
                self.store.delete(sid);
            }
            removed
        };
        if removed.is_none() {
            return false;
        }
        drop(removed);
        self.broadcast(json!({ "op": "session.deleted", "sessionId": sid }));
        true
    }

    pub fn history(&self, sid: &str) -> Value {
        let map = self.sessions.lock().unwrap();
        let events = map.get(sid).map(|s| s.events.clone()).unwrap_or_default();
        json!({ "op": "history", "sessionId": sid, "events": events })
    }

    /// Plan usage is per account: Default's is `usage[engine]`, an added
    /// account's is `usage.accounts[id]`.
    pub fn set_usage(&self, engine: &str, account: &str, value: Value) {
        let all = {
            let mut usage = self.usage.lock().unwrap();
            if account == crate::accounts::DEFAULT {
                usage[engine] = value;
            } else {
                usage["accounts"][account] = value;
            }
            usage.clone()
        };
        save_usage(&crate::store::data_dir(), &all);
        self.broadcast(json!({ "op": "usage", "usage": all }));
    }

    pub fn snapshot(&self) -> Value {
        let mut sessions: Vec<Meta> = self
            .sessions
            .lock()
            .unwrap()
            .values()
            .filter(|s| !crate::private::internal(&s.meta.id))
            .map(|s| s.meta.clone())
            .collect();
        sessions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        json!({
            "op": "snapshot",
            "sessions": sessions,
            "models": self.models.lock().unwrap().clone(),
            "usage": self.usage.lock().unwrap().clone(),
            "host": self.host.lock().unwrap().clone(),
            // `host` stays null until the first stats sample, a few seconds
            // after start; the name is known now.
            "hostName": crate::hosts::local_name(),
            "home": std::env::var("HOME").unwrap_or_default(),
            "workspace": workspace(),
            "hosts": self.remotes.list(),
            "accounts": self.accounts.list(),
            "automations": self.automations.list(),
            "agents": self.agents.list(),
            "mobile": self.mobile.status(),
            "slack": self.slack.view(),
        })
    }
}

/// Where new sessions start and the folder picker opens: YARD_WORKSPACE
/// (the container's project folder), else the home folder.
pub fn workspace() -> String {
    std::env::var("YARD_WORKSPACE")
        .ok()
        .filter(|w| !w.is_empty())
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_default()
}

/// One spelling per folder, so `~/x`, `~/x/` and a symlinked path compare equal.
fn same_folder_key(cwd: &str) -> std::path::PathBuf {
    let p = std::path::Path::new(cwd);
    std::fs::canonicalize(p).unwrap_or_else(|_| p.components().collect())
}

/// The reason and prompt a queued session recorded when it was queued.
pub fn queued_event(events: &[Value]) -> Option<&Value> {
    events.iter().rev().find(|e| e["type"] == "queued")
}

fn queued_reason(events: &[Value]) -> String {
    queued_event(events)
        .and_then(|e| e["reason"].as_str())
        .unwrap_or("waiting for room on the host")
        .to_string()
}

fn claude_models() -> Value {
    let efforts = json!(["low", "medium", "high", "xhigh", "max"]);
    json!([
        { "id": "claude-opus-5-5", "name": "Opus 5.5", "efforts": efforts, "defaultEffort": "high" },
        { "id": "claude-sonnet-5-5", "name": "Sonnet 5.5", "efforts": efforts, "defaultEffort": "medium" },
        { "id": "claude-haiku-5-5", "name": "Haiku 5.5", "efforts": efforts, "defaultEffort": "medium" },
    ])
}
