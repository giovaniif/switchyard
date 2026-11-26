//! Terminals: any number of PTYs per session, each running the user's login
//! shell in the session folder. Output is kept in a ring buffer so a client
//! that attaches later (or reconnects) gets the scrollback.

use std::{
    collections::{HashMap, VecDeque},
    io::{Read, Write},
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde_json::{json, Value};

use crate::state::App;

const SCROLLBACK_BYTES: usize = 256 * 1024;

/// `total` counts every byte the PTY ever produced. Chunks carry their offset
/// so a client can drop what its scrollback snapshot already covered.
struct Scroll {
    buf: VecDeque<u8>,
    total: u64,
}

struct Term {
    session_id: String,
    title: String,
    /// Creation order across all sessions, for listing.
    seq: u64,
    /// The shell is gone but the tab stays, with its output, until closed.
    exited: bool,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn Child + Send + Sync>,
    scroll: Arc<Mutex<Scroll>>,
}

#[derive(Default)]
struct Inner {
    map: HashMap<String, Term>,
    seq: u64,
    /// Last title number handed out per session, so titles are not reused.
    titles: HashMap<String, u32>,
}

#[derive(Default)]
pub struct Terms {
    inner: Mutex<Inner>,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        cols: cols.clamp(20, 500),
        rows: rows.clamp(5, 300),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn opened(term_id: &str, term: &Term) -> Value {
    let s = term.scroll.lock().unwrap();
    let bytes: Vec<u8> = s.buf.iter().copied().collect();
    json!({
        "op": "term.opened",
        "sessionId": term.session_id,
        "termId": term_id,
        "title": term.title,
        "exited": term.exited,
        "offset": s.total - bytes.len() as u64,
        "data": b64(&bytes),
    })
}

fn list_changed(app: &App, session_id: &str) {
    app.broadcast(json!({ "op": "term.listChanged", "sessionId": session_id }));
}

/// Attach to the session's first live terminal, or start one. Kept for
/// clients that know only one terminal per session.
pub fn open(app: &Arc<App>, session_id: &str, cols: u16, rows: u16) -> Result<Value> {
    {
        let inner = app.terms.inner.lock().unwrap();
        let first = inner
            .map
            .iter()
            .filter(|(_, t)| t.session_id == session_id && !t.exited)
            .min_by_key(|(_, t)| t.seq);
        if let Some((id, term)) = first {
            let _ = term.master.resize(size(cols, rows));
            return Ok(opened(id, term));
        }
    }
    new(app, session_id, cols, rows)
}

/// Start another terminal in the session.
pub fn new(app: &Arc<App>, session_id: &str, cols: u16, rows: u16) -> Result<Value> {
    let meta = app.meta(session_id).context("no such session")?;
    crate::git::ensure_folder(&meta.cwd)?;
    let pair = native_pty_system().openpty(size(cols, rows))?;

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let mut cmd = CommandBuilder::new(&shell);
    cmd.arg("-l");
    cmd.cwd(&meta.cwd);
    cmd.env("TERM", "xterm-256color");
    // yardd may have been started from inside a Claude Code session.
    for (key, _) in std::env::vars() {
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") || key.starts_with("CLAUDE_AGENT_SDK") {
            cmd.env_remove(&key);
        }
    }

    let child = pair.slave.spawn_command(cmd)?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader()?;
    let writer = pair.master.take_writer()?;

    let term_id = app.id("t");
    let scroll = Arc::new(Mutex::new(Scroll {
        buf: VecDeque::new(),
        total: 0,
    }));
    let reply = {
        let mut inner = app.terms.inner.lock().unwrap();
        inner.seq += 1;
        let seq = inner.seq;
        let n = inner.titles.entry(session_id.to_string()).or_insert(0);
        *n += 1;
        let term = Term {
            session_id: session_id.to_string(),
            title: n.to_string(),
            seq,
            exited: false,
            writer,
            master: pair.master,
            child,
            scroll: scroll.clone(),
        };
        let reply = opened(&term_id, &term);
        inner.map.insert(term_id.clone(), term);
        reply
    };
    list_changed(app, session_id);

    // PTY reads block, so they get a plain thread.
    let reader_app = app.clone();
    let reader_id = term_id.clone();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            let n = match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let offset = {
                let mut s = scroll.lock().unwrap();
                let offset = s.total;
                s.buf.extend(&chunk[..n]);
                s.total += n as u64;
                let excess = s.buf.len().saturating_sub(SCROLLBACK_BYTES);
                s.buf.drain(..excess);
                offset
            };
            reader_app.broadcast(json!({
                "op": "term.data",
                "termId": reader_id,
                "offset": offset,
                "data": b64(&chunk[..n]),
            }));
        }
        // The shell exited (or the PTY closed). A closed terminal is already
        // gone from the map; an exited one stays listed until closed.
        let session = {
            let mut inner = reader_app.terms.inner.lock().unwrap();
            inner.map.get_mut(&reader_id).map(|term| {
                term.exited = true;
                let _ = term.child.kill();
                let _ = term.child.wait();
                term.session_id.clone()
            })
        };
        reader_app.broadcast(json!({ "op": "term.exit", "termId": reader_id }));
        if let Some(sid) = session {
            list_changed(&reader_app, &sid);
        }
    });

    Ok(reply)
}

/// The session's terminals, oldest first.
pub fn list(app: &Arc<App>, session_id: &str) -> Value {
    let inner = app.terms.inner.lock().unwrap();
    let mut terms: Vec<(&String, &Term)> = inner.map.iter().filter(|(_, t)| t.session_id == session_id).collect();
    terms.sort_by_key(|(_, t)| t.seq);
    let terms: Vec<Value> = terms
        .into_iter()
        .map(|(id, t)| json!({ "termId": id, "title": t.title, "exited": t.exited }))
        .collect();
    json!({ "op": "term.list", "sessionId": session_id, "terms": terms })
}

/// Scrollback for one terminal, sized to the client that attaches.
pub fn attach(app: &Arc<App>, term_id: &str, cols: u16, rows: u16) -> Result<Value> {
    let inner = app.terms.inner.lock().unwrap();
    let term = inner.map.get(term_id).context("terminal is closed")?;
    if !term.exited {
        let _ = term.master.resize(size(cols, rows));
    }
    Ok(opened(term_id, term))
}

pub fn input(app: &Arc<App>, term_id: &str, data: &str) -> Result<()> {
    let mut inner = app.terms.inner.lock().unwrap();
    let term = inner.map.get_mut(term_id).filter(|t| !t.exited).context("terminal is closed")?;
    term.writer.write_all(data.as_bytes())?;
    term.writer.flush()?;
    Ok(())
}

pub fn resize(app: &Arc<App>, term_id: &str, cols: u16, rows: u16) -> Result<()> {
    let inner = app.terms.inner.lock().unwrap();
    let term = inner.map.get(term_id).context("terminal is closed")?;
    // A client fits its view before the attach reply says the shell exited;
    // with no shell there is nothing to resize, and nothing went wrong.
    if !term.exited {
        term.master.resize(size(cols, rows))?;
    }
    Ok(())
}

/// Kill a terminal's shell (if it still runs) and drop its tab.
pub fn close(app: &Arc<App>, term_id: &str) -> Result<()> {
    let term = app.terms.inner.lock().unwrap().map.remove(term_id).context("terminal is closed")?;
    let session_id = term.session_id.clone();
    reap(term);
    list_changed(app, &session_id);
    Ok(())
}

/// Killing waits out a grace period after SIGHUP, so it runs off the caller's
/// thread. Dropping the master ends the reader thread.
fn reap(mut term: Term) {
    if term.exited {
        return;
    }
    std::thread::spawn(move || {
        let _ = term.child.kill();
        let _ = term.child.wait();
    });
}

/// Each live terminal's session and shell process, for per-session usage.
pub fn shell_pids(app: &Arc<App>) -> Vec<(String, u32)> {
    let inner = app.terms.inner.lock().unwrap();
    inner
        .map
        .values()
        .filter(|t| !t.exited)
        .filter_map(|t| Some((t.session_id.clone(), t.child.process_id()?)))
        .collect()
}

/// Kill every terminal of the session, which is being deleted. No
/// `term.listChanged`: an open pane would ask for the list of a session that
/// is gone by then, and the refusal showed as a "no such session" banner.
/// `session.deleted` closes the pane.
pub fn close_session(app: &Arc<App>, session_id: &str) {
    let gone: Vec<Term> = {
        let mut inner = app.terms.inner.lock().unwrap();
        inner.titles.remove(session_id);
        let ids: Vec<String> = inner
            .map
            .iter()
            .filter(|(_, t)| t.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        ids.iter().filter_map(|id| inner.map.remove(id)).collect()
    };
    for term in gone {
        reap(term);
    }
}
