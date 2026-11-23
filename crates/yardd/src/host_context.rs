//! Bounded, read-only evidence about a host, for the built-in host agent (see
//! `agents`). No model-generated command reaches a shell.
use crate::state::{now_ms, App};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: String,
    pub host: String,
    pub title: String,
    pub at: u64,
    pub text: String,
    pub unavailable: bool,
}

pub fn clip(s: &str, limit: usize) -> String {
    if s.len() <= limit {
        return s.to_string();
    }
    let mut end = limit.saturating_sub(24).min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncated]", &s[..end])
}

// Logs and history can contain credentials. Omit entire suspicious lines rather
// than trying to guess where a credential ends. Never collect environment values.
fn redact(s: &str) -> String {
    s.lines()
        .map(|line| {
            let lower = line.to_lowercase();
            if [
                "password",
                "passwd",
                "token",
                "secret",
                "api_key",
                "api-key",
                "authorization",
                "bearer ",
                "private key",
                "sk-",
            ]
            .iter()
            .any(|w| lower.contains(w))
            {
                "[credential-bearing line omitted]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

async fn command(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<String> {
    // Isolated verification supplies scripted host readers without replacing
    // systemctl/ps for the rest of yardd (installer and process control).
    let scripted = std::env::var_os("YARD_CONTEXT_COMMAND_DIR")
        .map(|p| PathBuf::from(p).join(program))
        .filter(|p| p.is_file());
    let mut cmd = Command::new(scripted.as_deref().unwrap_or_else(|| Path::new(program)));
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("SYSTEMD_PAGER", "");
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }
    let mut child = cmd
        .spawn()
        .with_context(|| format!("{program} is unavailable"))?;
    let mut stdout = child.stdout.take().context("command stdout")?.take(8192);
    let result = tokio::time::timeout(Duration::from_secs(3), async {
        let mut buf = Vec::new();
        stdout.read_to_end(&mut buf).await?;
        // A full pipe is enough evidence. Do not wait on a producer blocked on
        // output that we deliberately stopped consuming.
        if buf.len() == 8192 {
            let _ = child.kill().await;
        } else if !child.wait().await?.success() {
            anyhow::bail!("{program} failed or access was denied");
        }
        Ok(redact(&String::from_utf8_lossy(&buf)))
    })
    .await;
    match result {
        Ok(r) => r,
        Err(_) => {
            let _ = child.kill().await;
            anyhow::bail!("{program} timed out");
        }
    }
}

fn source(title: &str, result: Result<String>) -> Source {
    let (text, unavailable) = match result {
        Ok(s) => (s, false),
        Err(e) => (format!("{e:#}"), true),
    };
    Source {
        id: String::new(),
        host: "local".into(),
        title: title.into(),
        at: now_ms(),
        text: clip(&text, 1600),
        unavailable,
    }
}

fn keywords(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|s| s.len() > 2)
        .map(str::to_lowercase)
        .filter(|s| {
            ![
                "what", "where", "when", "with", "this", "that", "the", "and", "was", "are", "did",
                "have", "server", "running",
            ]
            .contains(&s.as_str())
        })
        .take(12)
        .collect()
}

fn rank(text: &str, words: &[String]) -> usize {
    let text = text.to_lowercase();
    words.iter().filter(|w| text.contains(w.as_str())).count()
}

fn repositories(app: &App, query: &str) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = app
        .sessions
        .lock()
        .unwrap()
        .values()
        .filter(|s| !crate::private::internal(&s.meta.id))
        .map(|s| PathBuf::from(&s.meta.cwd))
        .collect();
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    // Discover work outside Switchyard, without crawling the whole filesystem.
    let mut roots = vec![PathBuf::from(crate::state::workspace())];
    roots.extend(["projects", "src", "code", "repos", "services"].map(|p| home.join(p)));
    if let Ok(extra) = std::env::var("YARD_CONTEXT_ROOTS") {
        roots.extend(std::env::split_paths(&extra));
    }
    for root in roots.into_iter().take(16) {
        paths.push(root.clone());
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.filter_map(Result::ok).take(100) {
                if entry.file_type().is_ok_and(|t| t.is_dir())
                    && !entry.file_name().to_string_lossy().starts_with('.')
                {
                    paths.push(entry.path());
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths.retain(|p| p.join(".git").exists());
    let words = keywords(query);
    paths.sort_by_key(|p| std::cmp::Reverse(rank(&p.to_string_lossy(), &words)));
    paths.truncate(4);
    paths
}

fn sessions(app: &App, query: &str) -> String {
    let words = keywords(query);
    let map = app.sessions.lock().unwrap();
    let mut hits = Vec::new();
    // The host agent's own chats would only quote its earlier answers back.
    let quoted = |s: &crate::state::Session| s.meta.agent.as_deref().is_some_and(|a| app.agents.is_builtin(a));
    for s in map.values().filter(|s| !crate::private::internal(&s.meta.id) && !quoted(s)) {
        let mut snippets: Vec<_> = s
            .events
            .iter()
            .enumerate()
            .filter_map(|(seq, e)| {
                if !matches!(
                    e["type"].as_str(),
                    Some("user_message" | "assistant_message" | "error")
                ) {
                    return None;
                }
                let text = e["text"].as_str().or(e["message"].as_str())?;
                Some((rank(text, &words), seq, clip(text, 700)))
            })
            .collect();
        snippets.sort_by_key(|(score, seq, _)| std::cmp::Reverse((*score, *seq)));
        let score = rank(&s.meta.title, &words) + snippets.first().map(|s| s.0).unwrap_or(0);
        let text = format!(
            "Session {} · {} · {} · updated {}\n{}\n{}",
            s.meta.id,
            s.meta.title,
            s.meta.status,
            s.meta.updated_at,
            s.meta.cwd,
            snippets
                .into_iter()
                .take(2)
                .map(|(_, seq, text)| format!("event {seq}: {text}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        hits.push((score, s.meta.updated_at, text));
    }
    hits.sort_by_key(|(score, at, _)| std::cmp::Reverse((*score, *at)));
    clip(
        &hits
            .into_iter()
            .take(3)
            .map(|(_, _, s)| s)
            .collect::<Vec<_>>()
            .join("\n\n"),
        3000,
    )
}

async fn history() -> Result<String> {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
    let mut out = String::new();
    for name in [".bash_history", ".zsh_history"] {
        if !tokio::fs::symlink_metadata(home.join(name))
            .await
            .is_ok_and(|m| m.is_file())
        {
            continue;
        }
        if let Ok(mut file) = tokio::fs::File::open(home.join(name)).await {
            use tokio::io::AsyncSeekExt;
            let size = file.metadata().await?.len();
            file.seek(std::io::SeekFrom::Start(size.saturating_sub(8192)))
                .await?;
            let mut buf = Vec::new();
            file.take(8192).read_to_end(&mut buf).await?;
            let text = String::from_utf8_lossy(&buf);
            let mut lines: Vec<_> = text.lines().rev().take(15).collect();
            lines.reverse();
            out.push_str(&format!(
                "{name} (saved history; may not include open shells):\n{}\n",
                redact(&lines.join("\n"))
            ));
        }
    }
    anyhow::ensure!(
        !out.is_empty(),
        "No readable saved shell history; open-shell activity is not captured"
    );
    Ok(out)
}

async fn services() -> Result<String> {
    if cfg!(target_os = "macos") {
        return command("launchctl", &["list"], None).await;
    }
    let (system, user) = tokio::join!(
        command(
            "systemctl",
            &[
                "list-units",
                "--type=service",
                "--all",
                "--no-pager",
                "--no-legend",
                "--plain"
            ],
            None
        ),
        command(
            "systemctl",
            &[
                "--user",
                "list-units",
                "--type=service",
                "--all",
                "--no-pager",
                "--no-legend",
                "--plain"
            ],
            None
        ),
    );
    anyhow::ensure!(
        system.is_ok() || user.is_ok(),
        "System and user service managers are unavailable"
    );
    let describe =
        |result: Result<String>| result.unwrap_or_else(|e| format!("Unavailable: {e:#}"));
    Ok(format!(
        "User services:\n{}\nSystem services:\n{}",
        clip(&describe(user), 750),
        clip(&describe(system), 750)
    ))
}

async fn ports() -> Result<String> {
    if cfg!(target_os = "macos") {
        command("lsof", &["-nP", "-iTCP", "-sTCP:LISTEN"], None).await
    } else {
        command("ss", &["-lnt"], None).await
    }
}

pub async fn collect(app: &Arc<App>, query: &str) -> Value {
    let (processes, services, containers, ports, history) = tokio::join!(
        command(
            "ps",
            if cfg!(target_os = "macos") {
                &["-eo", "pid,ppid,pcpu,pmem,comm", "-r"]
            } else {
                &["-eo", "pid,ppid,pcpu,pmem,comm", "--sort=-pcpu"]
            },
            None
        ),
        services(),
        command(
            "docker",
            &[
                "ps",
                "--all",
                "--format",
                "{{.Names}}\t{{.Status}}\t{{.Ports}}"
            ],
            None
        ),
        ports(),
        history(),
    );
    let mut sources = vec![
        source("Host resources", {
            let host = app.host.lock().unwrap();
            if host.is_null() {
                Err(anyhow::anyhow!("Host statistics have not been sampled yet"))
            } else {
                Ok(host.to_string())
            }
        }),
        source("Processes (including outside Switchyard)", processes),
        source("Services", services),
        source("Containers", containers),
        source("Listening TCP ports", ports),
        source("Recent saved terminal activity", history),
        source("Relevant Switchyard history", Ok(sessions(app, query))),
    ];
    // Git is invoked with fixed read-only arguments, never with question text.
    for path in repositories(app, query) {
        let (status, log) = tokio::join!(
            command(
                "git",
                &[
                    "-c",
                    "core.fsmonitor=false",
                    "status",
                    "--short",
                    "--branch",
                    "--untracked-files=no"
                ],
                Some(&path)
            ),
            command(
                "git",
                &["--no-pager", "log", "-5", "--format=%h %ai %s"],
                Some(&path)
            ),
        );
        sources.push(source(
            &format!("Repository {}", path.display()),
            status.and_then(|s| log.map(|l| format!("{s}\nRecent commits:\n{l}"))),
        ));
        let words = keywords(query);
        if !words.is_empty() {
            let mut args = vec!["--no-pager", "grep", "-n", "-I", "-F", "--max-count=3"];
            for word in words.iter().take(3) {
                args.extend(["-e", word]);
            }
            args.extend([
                "--",
                "*.md",
                "*.rs",
                "*.ts",
                "*.tsx",
                "*.py",
                "*.go",
                ":!*.lock",
                ":!vendor",
                ":!node_modules",
            ]);
            if let Ok(matches) = command("git", &args, Some(&path)).await {
                sources.push(source(
                    &format!("Matching tracked files in {}", path.display()),
                    Ok(matches),
                ));
            }
        }
    }
    let snapshot = sources.clone();
    let query = query.to_string();
    let historical =
        tokio::task::spawn_blocking(move || record_and_search(&snapshot, &query)).await;
    match historical {
        Ok(Ok(text)) if !text.is_empty() => {
            sources.push(source("Earlier host observations", Ok(text)))
        }
        Ok(Err(e)) => sources.push(source("Earlier host observations", Err(e))),
        Err(e) => sources.push(source("Earlier host observations", Err(e.into()))),
        _ => {}
    }
    for (i, s) in sources.iter_mut().enumerate() {
        s.id = format!("local:{}", i + 1);
    }
    let mut bytes = 0;
    sources.retain(|s| {
        bytes += serde_json::to_string(s).map(|v| v.len()).unwrap_or(2000);
        bytes < 20_000
    });
    json!({"op":"context.collect", "hostName":crate::hosts::local_name(), "sources": sources,
        "coverage":"Bounded snapshot: up to four repositories under known work folders and YARD_CONTEXT_ROOTS. Saved shell history may lag. Earlier observations exist only from previous questions. No environment, credential files or full filesystem crawl."})
}

/// Local FTS index retains distinct observations. Original snapshots stay on
/// their host; the hub only receives a few ranked passages for this question.
fn record_and_search(sources: &[Source], query: &str) -> Result<String> {
    use rusqlite::{params, Connection};
    // Named for Switchy, the assistant this served first; kept so earlier
    // observations survive the move to the host agent.
    let mut conn = Connection::open(crate::store::data_dir().join("switchy-context.db"))?;
    conn.busy_timeout(Duration::from_millis(500))?;
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS observations USING fts5(title, text, at UNINDEXED);",
    )?;
    let words = keywords(query);
    let mut matches: Vec<(String, String, u64)> = Vec::new();
    if !words.is_empty() {
        let search = words
            .iter()
            .map(|w| format!("\"{}\"", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut stmt = conn.prepare("SELECT title, text, at FROM observations WHERE observations MATCH ?1 ORDER BY rank LIMIT 8")?;
        for row in stmt.query_map([search], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
            matches.push(row?);
        }
    }
    if matches.is_empty() {
        let mut stmt =
            conn.prepare("SELECT title, text, at FROM observations ORDER BY rowid DESC LIMIT 8")?;
        for row in stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))? {
            matches.push(row?);
        }
    }
    let prior = matches
        .into_iter()
        .filter(|(title, text, _)| !sources.iter().any(|s| s.title == *title && s.text == *text))
        .take(3)
        .map(|(title, text, at)| format!("Observed at {at} · {title}\n{}", clip(&text, 600)))
        .collect::<Vec<_>>()
        .join("\n\n");
    let tx = conn.transaction()?;
    for s in sources.iter().filter(|s| {
        !s.unavailable && !s.text.is_empty() && s.title != "Relevant Switchyard history"
    }) {
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM observations WHERE title=?1 AND text=?2)",
            params![s.title, s.text],
            |r| r.get(0),
        )?;
        if !exists {
            tx.execute(
                "INSERT INTO observations(title,text,at) VALUES(?1,?2,?3)",
                params![s.title, s.text, s.at],
            )?;
        }
    }
    // Bound disk use as well as model context. These are cached observations,
    // not the canonical session logs.
    tx.execute("DELETE FROM observations WHERE rowid NOT IN (SELECT rowid FROM observations ORDER BY rowid DESC LIMIT 2000)", [])?;
    tx.commit()?;
    Ok(prior)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn byte_budget_preserves_unicode() {
        let s = clip(&"é🙂".repeat(200), 100);
        assert!(s.len() <= 100);
        assert!(s.ends_with("[truncated]"));
    }
    #[test]
    fn credentials_are_omitted() {
        assert_eq!(
            redact("git status\nTOKEN=abc curl foo\npassword=secret"),
            "git status\n[credential-bearing line omitted]\n[credential-bearing line omitted]"
        );
    }
}
