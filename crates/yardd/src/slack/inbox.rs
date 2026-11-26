//! The owner's own Slack, kept in `yard.db`: every direct message and group
//! DM (new ones too), as read through his user token.
//!
//! - **Sync** (`sync`), in the background when his connection says hello the
//!   first time in this process, and again after a connection that dropped
//!   (not after Slack's routine `disconnect` refresh, which would refetch every
//!   DM every few hours): `users.conversations` lists his DMs and group DMs;
//!   one not loaded yet gets its last 60 days (`conversations.history`,
//!   `oldest`), and its read mark from `conversations.info` when it had any;
//!   one already loaded catches up from its newest stored message. One
//!   conversation at a time: the Web API client waits out rate limits.
//! - **Live** (`on_event`): his user events. A message in a DM or group DM is
//!   stored, an unknown conversation (someone new wrote) looked up first; an
//!   edit (`message_changed`) replaces the text, a deletion marks it deleted,
//!   a thread reply counts on its parent, reactions are kept per message.
//! - **Reading** (`handle`): `slack.inbox.list`, `slack.inbox.messages`
//!   (a page of the newest top-level messages, `before` a ts for older ones,
//!   fetched from Slack when the stored ones run out; `thread` a thread,
//!   fetched fresh), `slack.inbox.read` (his read mark, here and in Slack:
//!   `conversations.mark`). Changes are broadcast as `slack.inbox` (the list)
//!   and `slack.inbox.message` (one message, new or changed).
//! - **Replying, as him** (`slack.inbox.send { text, thread? }`:
//!   `chat.postMessage` with his user token; `&`, `<` and `>` escaped, so
//!   what he types is never read as a mention or a link markup;
//!   `slack.inbox.react { ts, name, add }`: `reactions.add`/`remove`). The
//!   message Slack answers with is stored and broadcast at once; its event,
//!   when it comes, changes nothing.
//!
//! Texts are kept as Slack sent them (mrkdwn); the page turns them into React
//! elements, never HTML. Names of the people a message is from or mentions
//! travel with it (`users`).

use std::{
    path::Path,
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::{Me, MyApp};
use crate::state::{now_ms, App};

/// How far back a conversation is loaded the first time.
const HISTORY_DAYS: u64 = 60;
/// Messages per page in the view.
const PAGE: usize = 50;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS slack_conversations (
        team TEXT NOT NULL,
        id TEXT NOT NULL,
        kind TEXT NOT NULL,
        name TEXT NOT NULL DEFAULT '',
        peer TEXT NOT NULL DEFAULT '',
        last_ts TEXT NOT NULL DEFAULT '',
        last_read TEXT NOT NULL DEFAULT '',
        synced INTEGER NOT NULL DEFAULT 0,
        complete INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (team, id)
    );
    CREATE TABLE IF NOT EXISTS slack_messages (
        team TEXT NOT NULL,
        channel TEXT NOT NULL,
        ts TEXT NOT NULL,
        thread_ts TEXT NOT NULL DEFAULT '',
        user TEXT NOT NULL DEFAULT '',
        username TEXT NOT NULL DEFAULT '',
        text TEXT NOT NULL DEFAULT '',
        subtype TEXT NOT NULL DEFAULT '',
        edited INTEGER NOT NULL DEFAULT 0,
        deleted INTEGER NOT NULL DEFAULT 0,
        reply_count INTEGER NOT NULL DEFAULT 0,
        reactions TEXT NOT NULL DEFAULT '[]',
        files TEXT NOT NULL DEFAULT '[]',
        PRIMARY KEY (team, channel, ts)
    );
    CREATE INDEX IF NOT EXISTS slack_messages_thread ON slack_messages (team, channel, thread_ts, ts);
    CREATE TABLE IF NOT EXISTS slack_users (
        team TEXT NOT NULL,
        id TEXT NOT NULL,
        name TEXT NOT NULL,
        PRIMARY KEY (team, id)
    );
";

pub struct Inbox {
    db: Mutex<Option<Connection>>,
    /// A sync runs; another waits for the next hello.
    syncing: AtomicBool,
    /// The next hello of his connection syncs (set at start and after a drop).
    pub(super) due: AtomicBool,
    /// Asks for the list to be broadcast (coalesced).
    changed: Mutex<Option<mpsc::UnboundedSender<()>>>,
}

impl Inbox {
    pub fn open(dir: &Path) -> Inbox {
        let db = (|| -> Result<Connection> {
            std::fs::create_dir_all(dir)?;
            let conn = Connection::open(dir.join("yard.db"))?;
            conn.busy_timeout(Duration::from_secs(5))?;
            conn.execute_batch("PRAGMA journal_mode = WAL;")?;
            conn.execute_batch(SCHEMA)?;
            Ok(conn)
        })();
        let db = match db {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("yardd: the Slack inbox cannot use yard.db: {e:#}");
                None
            }
        };
        Inbox { db: Mutex::new(db), syncing: AtomicBool::new(false), due: AtomicBool::new(true), changed: Mutex::new(None) }
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
        let db = self.db.lock().unwrap();
        let conn = db.as_ref()?;
        match f(conn) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("yardd: slack inbox: {e}");
                None
            }
        }
    }

    fn touched(&self) {
        if let Some(tx) = self.changed.lock().unwrap().as_ref() {
            let _ = tx.send(());
        }
    }

    /// Every conversation with a message, newest first, as the sidebar lists them.
    pub fn list(&self, me: &str) -> Value {
        let rows = self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT team, id, kind, name, peer, last_ts, last_read,
                   (SELECT COUNT(*) FROM slack_messages m WHERE m.team = v.team AND m.channel = v.id AND m.ts > v.last_read
                      AND m.user != ?1 AND m.deleted = 0 AND (m.thread_ts = '' OR m.thread_ts = m.ts OR m.subtype = 'thread_broadcast'))
                 FROM slack_conversations v WHERE last_ts != '' ORDER BY last_ts DESC",
            )?;
            let rows = stmt.query_map([me], |r| {
                Ok(json!({
                    "team": r.get::<_, String>(0)?,
                    "id": r.get::<_, String>(1)?,
                    "kind": r.get::<_, String>(2)?,
                    "name": r.get::<_, String>(3)?,
                    "peer": r.get::<_, String>(4)?,
                    "lastTs": r.get::<_, String>(5)?,
                    "lastRead": r.get::<_, String>(6)?,
                    "unread": r.get::<_, i64>(7)?,
                }))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        });
        let mut list = rows.unwrap_or_default();
        for v in &mut list {
            let (team, id) = (v["team"].as_str().unwrap_or("").to_string(), v["id"].as_str().unwrap_or("").to_string());
            if let Some((user, text, deleted)) = self.with(|c| {
                c.query_row(
                    "SELECT user, text, deleted FROM slack_messages WHERE team = ?1 AND channel = ?2 AND (thread_ts = '' OR thread_ts = ts OR subtype = 'thread_broadcast') ORDER BY ts DESC LIMIT 1",
                    params![team, id],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? != 0)),
                )
                .optional()
            })
            .flatten()
            {
                v["preview"] = json!(if deleted { String::new() } else { preview(&text) });
                v["previewFrom"] = json!(self.name_of(&team, &user).unwrap_or(user));
            }
        }
        Value::Array(list)
    }

    fn name_of(&self, team: &str, user: &str) -> Option<String> {
        self.with(|c| c.query_row("SELECT name FROM slack_users WHERE team = ?1 AND id = ?2", params![team, user], |r| r.get::<_, String>(0)).optional()).flatten()
    }

    fn conversation(&self, team: &str, id: &str) -> Option<(String, String, bool, bool)> {
        self.with(|c| {
            c.query_row("SELECT kind, last_ts, synced, complete FROM slack_conversations WHERE team = ?1 AND id = ?2", params![team, id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)? != 0, r.get::<_, i64>(3)? != 0))
            })
            .optional()
        })
        .flatten()
    }

    fn put_conversation(&self, team: &str, id: &str, kind: &str, name: &str, peer: &str) {
        self.with(|c| {
            c.execute(
                "INSERT INTO slack_conversations (team, id, kind, name, peer) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (team, id) DO UPDATE SET kind = excluded.kind,
                   name = CASE WHEN excluded.name != '' THEN excluded.name ELSE name END,
                   peer = CASE WHEN excluded.peer != '' THEN excluded.peer ELSE peer END",
                params![team, id, kind, name, peer],
            )
        });
    }

    fn put_user(&self, team: &str, id: &str, name: &str) {
        self.with(|c| c.execute("INSERT OR REPLACE INTO slack_users (team, id, name) VALUES (?1, ?2, ?3)", params![team, id, name]));
    }

    /// Stores one message as Slack sent it. `live`: it just happened (an
    /// event, or his own post), so a reply not seen before is one more on its
    /// parent; one loaded from history was already in the parent's count.
    fn put_message(&self, team: &str, channel: &str, m: &Value, live: bool) {
        let Some(ts) = m["ts"].as_str() else { return };
        let thread = m["thread_ts"].as_str().unwrap_or("");
        let text = match m["text"].as_str().unwrap_or("") {
            "" => super::message_text(m),
            t => t.to_string(),
        };
        let files: Vec<Value> = m["files"].as_array().map(|f| f.iter().map(|f| json!({ "name": f["name"].as_str().or(f["title"].as_str()).unwrap_or("file") })).collect()).unwrap_or_default();
        let reactions: Vec<Value> = m["reactions"].as_array().map(|r| r.iter().map(|r| json!({ "name": r["name"], "users": r["users"] })).collect()).unwrap_or_default();
        let username = m["user_profile"]["display_name"].as_str().filter(|s| !s.is_empty()).or(m["username"].as_str()).or(m["bot_profile"]["name"].as_str()).unwrap_or("");
        self.with(|c| {
            let new = c.query_row("SELECT 1 FROM slack_messages WHERE team = ?1 AND channel = ?2 AND ts = ?3", params![team, channel, ts], |_| Ok(())).optional()?.is_none();
            c.execute(
                "INSERT INTO slack_messages (team, channel, ts, thread_ts, user, username, text, subtype, edited, reply_count, reactions, files)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT (team, channel, ts) DO UPDATE SET thread_ts = excluded.thread_ts, user = excluded.user, username = excluded.username,
                   text = excluded.text, subtype = excluded.subtype, edited = excluded.edited,
                   reply_count = MAX(reply_count, excluded.reply_count), reactions = excluded.reactions, files = excluded.files",
                params![
                    team,
                    channel,
                    ts,
                    thread,
                    m["user"].as_str().or(m["bot_id"].as_str()).unwrap_or(""),
                    username,
                    text,
                    m["subtype"].as_str().unwrap_or(""),
                    m["edited"].is_object() as i64,
                    m["reply_count"].as_i64().unwrap_or(0),
                    Value::Array(reactions).to_string(),
                    Value::Array(files).to_string(),
                ],
            )?;
            // A new live reply is one more on its parent (whose count came
            // from Slack, with replies not kept here), and never fewer than
            // the replies kept.
            if !thread.is_empty() && thread != ts {
                c.execute(
                    "UPDATE slack_messages SET reply_count = MAX(reply_count + ?4, (SELECT COUNT(*) FROM slack_messages WHERE team = ?1 AND channel = ?2 AND thread_ts = ?3 AND ts != ?3))
                     WHERE team = ?1 AND channel = ?2 AND ts = ?3",
                    params![team, channel, thread, (new && live) as i64],
                )?;
            }
            let top = thread.is_empty() || thread == ts || m["subtype"] == "thread_broadcast";
            if top {
                c.execute("UPDATE slack_conversations SET last_ts = ?3 WHERE team = ?1 AND id = ?2 AND last_ts < ?3", params![team, channel, ts])?;
            }
            Ok(())
        });
    }

    fn message(&self, team: &str, channel: &str, ts: &str) -> Option<Value> {
        self.with(|c| c.query_row(&format!("{SELECT} WHERE team = ?1 AND channel = ?2 AND ts = ?3"), params![team, channel, ts], row).optional()).flatten()
    }

    /// The names a list of messages needs: their authors, the people they
    /// mention, and who reacted.
    fn users_for(&self, team: &str, messages: &[Value]) -> Value {
        let mut ids: Vec<String> = Vec::new();
        for m in messages {
            if let Some(u) = m["user"].as_str() {
                ids.push(u.to_string());
            }
            ids.extend(mentions(m["text"].as_str().unwrap_or("")));
            for r in m["reactions"].as_array().into_iter().flatten() {
                ids.extend(r["users"].as_array().into_iter().flatten().filter_map(|u| u.as_str().map(str::to_string)));
            }
        }
        ids.sort();
        ids.dedup();
        let map: serde_json::Map<String, Value> = ids.into_iter().filter_map(|id| self.name_of(team, &id).map(|n| (id, json!(n)))).collect();
        Value::Object(map)
    }
}

const SELECT: &str = "SELECT ts, thread_ts, user, username, text, subtype, edited, deleted, reply_count, reactions, files FROM slack_messages";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Value> {
    let ts: String = r.get(0)?;
    let thread: String = r.get(1)?;
    let deleted = r.get::<_, i64>(7)? != 0;
    let mut v = json!({
        "ts": ts,
        "user": r.get::<_, String>(2)?,
        "text": if deleted { String::new() } else { r.get::<_, String>(4)? },
        "edited": r.get::<_, i64>(6)? != 0,
        "deleted": deleted,
        "replyCount": r.get::<_, i64>(8)?,
        "reactions": serde_json::from_str::<Value>(&r.get::<_, String>(9)?).unwrap_or(json!([])),
        "files": serde_json::from_str::<Value>(&r.get::<_, String>(10)?).unwrap_or(json!([])),
    });
    if !thread.is_empty() && thread != ts {
        v["threadTs"] = json!(thread);
    }
    let username: String = r.get(3)?;
    if !username.is_empty() {
        v["username"] = json!(username);
    }
    let subtype: String = r.get(5)?;
    if !subtype.is_empty() {
        v["subtype"] = json!(subtype);
    }
    Ok(v)
}

/// The user ids a text mentions (`<@U123>`, `<@U123|name>`).
fn mentions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("<@") {
        rest = &rest[i + 2..];
        let id: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if id.len() >= 3 && id.starts_with(['U', 'W']) {
            out.push(id);
        }
    }
    out
}

/// One line for the sidebar: Slack markup read as text, clipped.
fn preview(text: &str) -> String {
    super::clip_line(&super::from_slack(text), 120)
}

/// `1700000000.000100`, or the start of the history window.
fn window_start() -> String {
    let now = now_ms() / 1000;
    format!("{}.000000", now.saturating_sub(HISTORY_DAYS * 86_400))
}

fn ready(app: &App) -> Option<MyApp> {
    app.slack.me().filter(|m| m.step() == "ready")
}

/// Starts the coalesced broadcast of the list (once per yardd).
pub fn start(app: &Arc<App>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<()>();
    *app.slack.inbox.changed.lock().unwrap() = Some(tx);
    let app = app.clone();
    tokio::spawn(async move {
        while rx.recv().await.is_some() {
            tokio::time::sleep(Duration::from_millis(300)).await;
            while rx.try_recv().is_ok() {}
            broadcast_list(&app);
        }
    });
}

fn broadcast_list(app: &App) {
    let me = ready(app).map(|m| m.user_id).unwrap_or_default();
    app.broadcast(json!({ "op": "slack.inbox", "conversations": app.slack.inbox.list(&me) }));
}

/// The list for the snapshot.
pub fn snapshot(app: &App) -> Value {
    let me = ready(app).map(|m| m.user_id).unwrap_or_default();
    app.slack.inbox.list(&me)
}

/// Remembers who a user is (name from Slack, cached).
async fn learn_user(app: &App, token: &str, team: &str, user: &str) {
    if user.is_empty() || app.slack.inbox.name_of(team, user).is_some() {
        return;
    }
    let name = app.slack.user_name(token, user).await;
    if name != user {
        app.slack.inbox.put_user(team, user, &name);
    }
}

async fn learn_message(app: &App, token: &str, team: &str, m: &Value) {
    if let Some(u) = m["user"].as_str() {
        learn_user(app, token, team, u).await;
    }
    for u in mentions(m["text"].as_str().unwrap_or("")) {
        learn_user(app, token, team, &u).await;
    }
}

/// A conversation's kind and name as the sidebar shows it: a DM is the other
/// person, a group DM the others' names.
async fn describe(app: &App, token: &str, team: &str, me: &str, c: &Value) -> (String, String, String) {
    let id = c["id"].as_str().unwrap_or("");
    if c["is_im"] == true {
        let peer = c["user"].as_str().unwrap_or("").to_string();
        learn_user(app, token, team, &peer).await;
        let name = app.slack.inbox.name_of(team, &peer).unwrap_or_else(|| peer.clone());
        return ("im".into(), name, peer);
    }
    if c["is_mpim"] == true {
        let members = app.slack.api(token, "conversations.members", &[("channel", id), ("limit", "20")]).await.ok();
        let mut names = Vec::new();
        for u in members.iter().flat_map(|v| v["members"].as_array().cloned().unwrap_or_default()) {
            let Some(u) = u.as_str().filter(|u| *u != me) else { continue };
            learn_user(app, token, team, u).await;
            names.push(app.slack.inbox.name_of(team, u).unwrap_or_else(|| u.to_string()));
        }
        let name = if names.is_empty() { c["name"].as_str().unwrap_or(id).to_string() } else { names.join(", ") };
        return ("mpim".into(), name, String::new());
    }
    let kind = if c["is_private"] == true { "group" } else { "channel" };
    (kind.into(), format!("#{}", c["name"].as_str().unwrap_or(id)), String::new())
}

/// Loads a conversation's messages from Slack from `oldest` on (all pages),
/// storing them. Answers the newest ts seen.
async fn load_since(app: &App, token: &str, team: &str, channel: &str, oldest: &str) -> Result<String> {
    let mut cursor = String::new();
    let mut newest = String::new();
    loop {
        let mut form = vec![("channel", channel), ("oldest", oldest), ("limit", "200")];
        if !cursor.is_empty() {
            form.push(("cursor", cursor.as_str()));
        }
        let v = app.slack.api(token, "conversations.history", &form).await?;
        for m in v["messages"].as_array().into_iter().flatten() {
            learn_message(app, token, team, m).await;
            app.slack.inbox.put_message(team, channel, m, false);
            if let Some(ts) = m["ts"].as_str().filter(|ts| *ts > newest.as_str()) {
                newest = ts.to_string();
            }
        }
        cursor = v["response_metadata"]["next_cursor"].as_str().unwrap_or("").to_string();
        if cursor.is_empty() || v["has_more"] != true {
            return Ok(newest);
        }
    }
}

/// The owner's DMs and group DMs, loaded and caught up (see the module notes).
pub async fn sync(app: Arc<App>) {
    let inbox = &app.slack.inbox;
    if inbox.syncing.swap(true, Ordering::SeqCst) {
        return;
    }
    if let Err(e) = sync_all(&app).await {
        eprintln!("yardd: slack inbox sync: {e:#}");
        inbox.due.store(true, Ordering::SeqCst);
    }
    inbox.syncing.store(false, Ordering::SeqCst);
    inbox.touched();
}

async fn sync_all(app: &Arc<App>) -> Result<()> {
    let Some(me) = ready(app) else { return Ok(()) };
    let (token, team) = (me.user_token.clone(), me.team_id.clone());
    let mut cursor = String::new();
    let mut found = Vec::new();
    loop {
        let mut form = vec![("types", "im,mpim"), ("limit", "200"), ("exclude_archived", "true")];
        if !cursor.is_empty() {
            form.push(("cursor", cursor.as_str()));
        }
        let v = app.slack.api(&token, "users.conversations", &form).await.context("listing your direct messages")?;
        found.extend(v["channels"].as_array().cloned().unwrap_or_default().into_iter().filter(|c| c["is_user_deleted"] != true));
        cursor = v["response_metadata"]["next_cursor"].as_str().unwrap_or("").to_string();
        if cursor.is_empty() {
            break;
        }
    }
    for c in &found {
        let Some(id) = c["id"].as_str() else { continue };
        // His Slack was disconnected meanwhile.
        if ready(app).is_none_or(|m| m.user_token != token) {
            return Ok(());
        }
        let (kind, name, peer) = describe(app, &token, &team, &me.user_id, c).await;
        app.slack.inbox.put_conversation(&team, id, &kind, &name, &peer);
        let (_, last_ts, synced, _) = app.slack.inbox.conversation(&team, id).unwrap_or_default();
        if synced {
            if !last_ts.is_empty() {
                load_since(app, &token, &team, id, &last_ts).await?;
            }
        } else {
            let newest = load_since(app, &token, &team, id, &window_start()).await?;
            // Slack's own read mark, where there is anything to read.
            let last_read = if newest.is_empty() {
                String::new()
            } else {
                match app.slack.api(&token, "conversations.info", &[("channel", id)]).await {
                    Ok(v) => v["channel"]["last_read"].as_str().filter(|s| !s.is_empty() && *s != "0000000000.000000").map(str::to_string).unwrap_or(newest.clone()),
                    Err(_) => newest.clone(),
                }
            };
            app.slack.inbox.with(|c| c.execute("UPDATE slack_conversations SET synced = 1, last_read = MAX(last_read, ?3) WHERE team = ?1 AND id = ?2", params![team, id, last_read]));
        }
        app.slack.inbox.touched();
    }
    Ok(())
}

/// One of the owner's own events (his user events): see the module notes.
pub(super) async fn on_event(app: &Arc<App>, me: &Me, payload: &Value) -> Result<()> {
    let ev = &payload["event"];
    let team = payload["team_id"].as_str().or(ev["team"].as_str()).unwrap_or(&me.team_id).to_string();
    let token = me.token.as_str();
    let inbox = &app.slack.inbox;
    match ev["type"].as_str().unwrap_or("") {
        "message" => {
            let Some(channel) = ev["channel"].as_str() else { return Ok(()) };
            let kind = ev["channel_type"].as_str().unwrap_or(if channel.starts_with('D') { "im" } else { "" });
            if kind != "im" && kind != "mpim" {
                return Ok(());
            }
            if inbox.conversation(&team, channel).is_none() {
                // Someone new wrote: a DM yardd has not seen.
                let info = app.slack.api(token, "conversations.info", &[("channel", channel)]).await?;
                let (kind, name, peer) = describe(app, token, &team, &me.bot_user, &info["channel"]).await;
                inbox.put_conversation(&team, channel, &kind, &name, &peer);
                inbox.with(|c| c.execute("UPDATE slack_conversations SET synced = 1 WHERE team = ?1 AND id = ?2", params![team, channel]));
            }
            let changed_ts = match ev["subtype"].as_str() {
                Some("message_changed") => {
                    let m = &ev["message"];
                    learn_message(app, token, &team, m).await;
                    inbox.put_message(&team, channel, m, false);
                    m["ts"].as_str().map(str::to_string)
                }
                Some("message_deleted") => {
                    let ts = ev["deleted_ts"].as_str().unwrap_or("").to_string();
                    inbox.with(|c| c.execute("UPDATE slack_messages SET deleted = 1, text = '' WHERE team = ?1 AND channel = ?2 AND ts = ?3", params![team, channel, ts]));
                    Some(ts)
                }
                Some("message_replied") => {
                    let m = &ev["message"];
                    let ts = m["ts"].as_str().unwrap_or("").to_string();
                    let n = m["reply_count"].as_i64().unwrap_or(0);
                    inbox.with(|c| c.execute("UPDATE slack_messages SET reply_count = MAX(reply_count, ?4) WHERE team = ?1 AND channel = ?2 AND ts = ?3", params![team, channel, ts, n]));
                    Some(ts)
                }
                Some("channel_join") | Some("channel_leave") | Some("group_join") | Some("group_leave") => None,
                _ => {
                    learn_message(app, token, &team, ev).await;
                    inbox.put_message(&team, channel, ev, true);
                    let ts = ev["ts"].as_str().map(str::to_string);
                    // A reply changes its parent's count too.
                    if let Some(parent) = ev["thread_ts"].as_str().filter(|t| Some(*t) != ts.as_deref()) {
                        send_message(app, &team, channel, parent);
                    }
                    ts
                }
            };
            if let Some(ts) = changed_ts {
                send_message(app, &team, channel, &ts);
            }
            inbox.touched();
        }
        kind @ ("reaction_added" | "reaction_removed") => {
            let item = &ev["item"];
            let (Some(channel), Some(ts), Some(name), Some(user)) = (item["channel"].as_str(), item["ts"].as_str(), ev["reaction"].as_str(), ev["user"].as_str()) else { return Ok(()) };
            let Some(m) = inbox.message(&team, channel, ts) else { return Ok(()) };
            let mut list: Vec<Value> = m["reactions"].as_array().cloned().unwrap_or_default();
            toggle(&mut list, name, user, kind == "reaction_added");
            learn_user(app, token, &team, user).await;
            inbox.with(|c| c.execute("UPDATE slack_messages SET reactions = ?4 WHERE team = ?1 AND channel = ?2 AND ts = ?3", params![team, channel, ts, Value::Array(list).to_string()]));
            send_message(app, &team, channel, ts);
        }
        _ => {}
    }
    Ok(())
}

/// A message's reactions with `user`'s `name` added or taken off.
fn toggle(list: &mut Vec<Value>, name: &str, user: &str, add: bool) {
    match list.iter_mut().find(|r| r["name"] == name) {
        Some(r) => {
            let mut users: Vec<Value> = r["users"].as_array().cloned().unwrap_or_default();
            users.retain(|u| u != user);
            if add {
                users.push(json!(user));
            }
            r["users"] = json!(users);
        }
        None if add => list.push(json!({ "name": name, "users": [user] })),
        None => {}
    }
    list.retain(|r| r["users"].as_array().is_some_and(|u| !u.is_empty()));
}

fn send_message(app: &App, team: &str, channel: &str, ts: &str) {
    let Some(m) = app.slack.inbox.message(team, channel, ts) else { return };
    let users = app.slack.inbox.users_for(team, std::slice::from_ref(&m));
    app.broadcast(json!({ "op": "slack.inbox.message", "team": team, "channel": channel, "message": m, "users": users }));
}

/// The page's reading ops (see the module notes). Answers the asking window.
pub async fn handle(app: &Arc<App>, direct: &mpsc::UnboundedSender<String>, msg: &Value) -> Result<()> {
    let inbox = &app.slack.inbox;
    let me = ready(app);
    let s = |k: &str| msg[k].as_str().unwrap_or("").to_string();
    let reply = match msg["op"].as_str().unwrap_or("") {
        "slack.inbox.list" => json!({ "op": "slack.inbox", "conversations": inbox.list(me.as_ref().map(|m| m.user_id.as_str()).unwrap_or("")) }),
        "slack.inbox.messages" => {
            let (team, channel, before, thread) = (s("team"), s("channel"), s("before"), s("thread"));
            anyhow::ensure!(!team.is_empty() && !channel.is_empty(), "missing `team` or `channel`");
            let (_, _, _, complete) = inbox.conversation(&team, &channel).context("no such conversation")?;
            let mut more = false;
            let messages = if !thread.is_empty() {
                // A thread is read fresh from Slack each time it opens.
                if let Some(m) = &me {
                    if let Ok(v) = app.slack.api(&m.user_token, "conversations.replies", &[("channel", &channel), ("ts", &thread), ("limit", "200")]).await {
                        for r in v["messages"].as_array().into_iter().flatten() {
                            learn_message(app, &m.user_token, &team, r).await;
                            inbox.put_message(&team, &channel, r, false);
                        }
                    }
                }
                inbox.with(|c| {
                    let mut stmt = c.prepare(&format!("{SELECT} WHERE team = ?1 AND channel = ?2 AND (ts = ?3 OR thread_ts = ?3) ORDER BY ts"))?;
                    let rows = stmt.query_map(params![team, channel, thread], row)?.collect::<rusqlite::Result<Vec<_>>>();
                    rows
                })
                .unwrap_or_default()
            } else {
                let page = |inbox: &Inbox| {
                    inbox
                        .with(|c| {
                            let top = "(thread_ts = '' OR thread_ts = ts OR subtype = 'thread_broadcast')";
                            let mut stmt = c.prepare(&format!("{SELECT} WHERE team = ?1 AND channel = ?2 AND {top} AND (?3 = '' OR ts < ?3) ORDER BY ts DESC LIMIT ?4"))?;
                            let rows = stmt.query_map(params![team, channel, before, PAGE as i64 + 1], row)?.collect::<rusqlite::Result<Vec<_>>>();
                            rows
                        })
                        .unwrap_or_default()
                };
                let mut rows = page(inbox);
                // Older than what is kept: from Slack, a page at a time.
                if rows.len() <= PAGE && !complete && !before.is_empty() {
                    if let Some(m) = &me {
                        let oldest = rows.last().and_then(|r| r["ts"].as_str()).unwrap_or(&before).to_string();
                        let v = app.slack.api(&m.user_token, "conversations.history", &[("channel", &channel), ("latest", &oldest), ("limit", "100")]).await?;
                        for r in v["messages"].as_array().into_iter().flatten() {
                            learn_message(app, &m.user_token, &team, r).await;
                            inbox.put_message(&team, &channel, r, false);
                        }
                        if v["has_more"] != true {
                            inbox.with(|c| c.execute("UPDATE slack_conversations SET complete = 1 WHERE team = ?1 AND id = ?2", params![team, channel]));
                        }
                        rows = page(inbox);
                    }
                }
                let (_, _, _, complete) = inbox.conversation(&team, &channel).unwrap_or_default();
                more = rows.len() > PAGE || !complete;
                rows.truncate(PAGE);
                rows.reverse();
                rows
            };
            let users = inbox.users_for(&team, &messages);
            json!({ "op": "slack.inbox.messages", "team": team, "channel": channel, "thread": if thread.is_empty() { Value::Null } else { json!(thread) }, "before": if before.is_empty() { Value::Null } else { json!(before) }, "messages": messages, "users": users, "more": more })
        }
        "slack.inbox.read" => {
            let (team, channel, ts) = (s("team"), s("channel"), s("ts"));
            anyhow::ensure!(!ts.is_empty(), "missing `ts`");
            let moved = inbox.with(|c| c.execute("UPDATE slack_conversations SET last_read = ?3 WHERE team = ?1 AND id = ?2 AND last_read < ?3", params![team, channel, ts])).unwrap_or(0) > 0;
            inbox.touched();
            // Read here is read in Slack too (best effort).
            if let (true, Some(m)) = (moved, me) {
                let app = app.clone();
                tokio::spawn(async move {
                    let _ = app.slack.api(&m.user_token, "conversations.mark", &[("channel", &channel), ("ts", &ts)]).await;
                });
            }
            return Ok(());
        }
        "slack.inbox.send" => {
            let m = me.context("Your Slack is not connected.")?;
            let (team, channel, thread) = (s("team"), s("channel"), s("thread"));
            let text = msg["text"].as_str().unwrap_or("").trim().to_string();
            anyhow::ensure!(!text.is_empty(), "Write something first.");
            anyhow::ensure!(text.len() <= 40_000, "That message is too long for Slack.");
            inbox.conversation(&team, &channel).context("no such conversation")?;
            let escaped = text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
            let mut form = vec![("channel", channel.as_str()), ("text", escaped.as_str())];
            if !thread.is_empty() {
                form.push(("thread_ts", thread.as_str()));
            }
            let v = app.slack.api(&m.user_token, "chat.postMessage", &form).await.map_err(|e| anyhow::anyhow!("Slack did not take it ({e:#})."))?;
            let mut posted = v["message"].clone();
            if posted["ts"].is_null() {
                posted["ts"] = v["ts"].clone();
            }
            if posted["user"].is_null() {
                posted["user"] = json!(m.user_id);
            }
            learn_message(app, &m.user_token, &team, &posted).await;
            inbox.put_message(&team, &channel, &posted, true);
            let ts = posted["ts"].as_str().unwrap_or("").to_string();
            send_message(app, &team, &channel, &ts);
            if !thread.is_empty() {
                send_message(app, &team, &channel, &thread);
            } else {
                // His own message: everything before it is read.
                inbox.with(|c| c.execute("UPDATE slack_conversations SET last_read = ?3 WHERE team = ?1 AND id = ?2 AND last_read < ?3", params![team, channel, ts]));
            }
            inbox.touched();
            json!({ "op": "slack.inbox.sent", "team": team, "channel": channel, "ts": ts })
        }
        "slack.inbox.react" => {
            let m = me.context("Your Slack is not connected.")?;
            let (team, channel, ts, name) = (s("team"), s("channel"), s("ts"), s("name"));
            anyhow::ensure!(!name.is_empty() && name.len() <= 100 && !name.contains(char::is_whitespace), "missing `name`");
            let add = msg["add"].as_bool().unwrap_or(true);
            let current = inbox.message(&team, &channel, &ts).context("no such message")?;
            let method = if add { "reactions.add" } else { "reactions.remove" };
            if let Err(e) = app.slack.api(&m.user_token, method, &[("channel", &channel), ("timestamp", &ts), ("name", &name)]).await {
                let text = format!("{e:#}");
                if !text.ends_with("already_reacted") && !text.ends_with("no_reaction") {
                    anyhow::bail!("Slack did not take it ({text}).");
                }
            }
            let mut list: Vec<Value> = current["reactions"].as_array().cloned().unwrap_or_default();
            toggle(&mut list, &name, &m.user_id, add);
            inbox.with(|c| c.execute("UPDATE slack_messages SET reactions = ?4 WHERE team = ?1 AND channel = ?2 AND ts = ?3", params![team, channel, ts, Value::Array(list).to_string()]));
            send_message(app, &team, &channel, &ts);
            json!({ "op": "slack.inbox.reacted", "team": team, "channel": channel, "ts": ts })
        }
        other => anyhow::bail!("unknown op `{other}`"),
    };
    let mut reply = reply;
    if !msg["ref"].is_null() {
        reply["ref"] = msg["ref"].clone();
    }
    let _ = direct.send(reply.to_string());
    Ok(())
}

/// Forgets every stored conversation (his Slack was disconnected).
pub fn forget(app: &App) {
    app.slack.inbox.with(|c| c.execute_batch("DELETE FROM slack_messages; DELETE FROM slack_conversations; DELETE FROM slack_users;"));
    app.slack.inbox.due.store(true, Ordering::SeqCst);
    app.slack.inbox.touched();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_are_user_ids_only() {
        assert_eq!(mentions("hi <@U1ABC> and <@W2XYZ|ana>, not <#C1|general> or <@B1>"), ["U1ABC", "W2XYZ"]);
    }

    #[test]
    fn messages_are_kept_edited_deleted_and_counted_in_threads() {
        let dir = std::env::temp_dir().join(format!("yard-inbox-test-{}", std::process::id()));
        let inbox = Inbox::open(&dir);
        inbox.put_conversation("T", "D1", "im", "Ana", "U2");
        inbox.put_message("T", "D1", &json!({ "ts": "1.000100", "user": "U2", "text": "hi" }), true);
        inbox.put_message("T", "D1", &json!({ "ts": "2.000100", "user": "U2", "text": "a thread", "reply_count": 0 }), true);
        inbox.put_message("T", "D1", &json!({ "ts": "3.000100", "user": "U1", "text": "reply", "thread_ts": "2.000100" }), true);
        inbox.put_message("T", "D1", &json!({ "ts": "1.000100", "user": "U2", "text": "hi!", "edited": { "ts": "4.0" } }), true);
        let parent = inbox.message("T", "D1", "2.000100").unwrap();
        assert_eq!(parent["replyCount"], 1);
        // A reply loaded from history was in Slack's count already; seen live again, nothing changes.
        inbox.put_message("T", "D1", &json!({ "ts": "2.000100", "user": "U2", "text": "a thread", "reply_count": 2 }), false);
        inbox.put_message("T", "D1", &json!({ "ts": "3.500100", "user": "U2", "text": "older reply", "thread_ts": "2.000100" }), false);
        inbox.put_message("T", "D1", &json!({ "ts": "3.000100", "user": "U1", "text": "reply", "thread_ts": "2.000100" }), true);
        assert_eq!(inbox.message("T", "D1", "2.000100").unwrap()["replyCount"], 2);
        let edited = inbox.message("T", "D1", "1.000100").unwrap();
        assert_eq!((edited["text"].as_str(), edited["edited"].as_bool()), (Some("hi!"), Some(true)));
        // The list: the newest top-level message, and what the owner has not read.
        let list = inbox.list("U1");
        assert_eq!(list[0]["lastTs"], "2.000100");
        assert_eq!(list[0]["unread"], 2);
        assert_eq!(list[0]["preview"], "a thread");
        let _ = std::fs::remove_dir_all(dir);
    }
}
