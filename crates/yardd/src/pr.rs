//! The PR pane: the GitHub pull request for a session folder's branch.
//!
//! Read-only. Everything goes through the user's own `gh` login, run in the
//! session folder, so yardd never holds a GitHub token.

use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::{
    process::Command,
    sync::{mpsc, OnceCell},
};

use crate::state::App;

const GH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CHECKS: usize = 100;
const MAX_THREADS: usize = 100;
const MAX_COMMENTS: usize = 20;
const MAX_BODY_CHARS: usize = 4_000;

const PR_FIELDS: &str =
    "number,title,url,state,isDraft,baseRefName,headRefName,reviewDecision,mergeable,mergeStateStatus,statusCheckRollup";

/// What a hover card on a PR link shows (`pr.peek`).
const PEEK_FIELDS: &str = "number,title,url,state,isDraft,author,baseRefName,headRefName,reviewDecision,mergeStateStatus,additions,deletions,updatedAt,statusCheckRollup";
/// Hovering down a list of links should not fork a gh per hover.
const PEEK_FRESH: Duration = Duration::from_secs(60);
/// A failure (gh logged out, a timeout) is retried sooner than an answer.
const PEEK_ERROR_FRESH: Duration = Duration::from_secs(10);
const MAX_FAILING_NAMES: usize = 5;
const MAX_TITLE_CHARS: usize = 300;
const MAX_NAME_CHARS: usize = 100;

/// One `pr.peek` per URL: concurrent asks share the gh run, and its answer
/// (or failure) is kept until it goes stale.
type Peek = Arc<OnceCell<(Instant, std::result::Result<Value, String>)>>;
static PEEKS: LazyLock<Mutex<HashMap<String, Peek>>> = LazyLock::new(Default::default);

const THREADS_QUERY: &str = "query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100) {
        nodes {
          isResolved isOutdated path line originalLine
          comments(first: 20) { nodes { author { login } body url } }
        }
      }
    }
  }
}";

/// Entry point for every `pr.*` op, dispatched from `ws::handle`.
pub async fn handle(app: &Arc<App>, direct: &mpsc::UnboundedSender<String>, msg: &Value) -> Result<()> {
    match msg["op"].as_str().unwrap_or("") {
        "pr.get" => {
            let sid = msg["sessionId"].as_str().context("missing `sessionId`")?;
            let meta = app.meta(sid).context("no such session")?;
            let pr = match get(&meta.cwd, msg["linkOnly"].as_bool() == Some(true)).await {
                Ok(pr) => pr,
                Err(e) => json!({ "error": format!("{e:#}") }),
            };
            let _ = direct.send(json!({ "op": "pr", "sessionId": sid, "pr": pr }).to_string());
        }
        "pr.peek" => {
            let sid = msg["sessionId"].as_str().context("missing `sessionId`")?;
            let meta = app.meta(sid).context("no such session")?;
            let raw = msg["url"].as_str().context("missing `url`")?;
            let mut reply = json!({ "op": "pr.peek", "sessionId": sid, "url": raw });
            if !msg["ref"].is_null() {
                reply["ref"] = msg["ref"].clone();
            }
            match canonical_pr_url(raw) {
                None => reply["error"] = json!("not a GitHub pull request link"),
                Some(url) => {
                    reply["url"] = json!(url);
                    match peek(&meta.cwd, &url).await {
                        Ok(pr) => reply["pr"] = pr,
                        Err(e) => reply["error"] = json!(e),
                    }
                }
            }
            let _ = direct.send(reply.to_string());
        }
        other => bail!("unknown op `{other}`"),
    }
    Ok(())
}

async fn get(cwd: &str, link_only: bool) -> Result<Value> {
    crate::git::ensure_folder(cwd)?;
    let (is_repo, _) = git(cwd, &["rev-parse", "--is-inside-work-tree"]).await?;
    if !is_repo {
        // An answer, not a failure: the session header asks every minute, and
        // a plain folder can still become a repository later (`git init`).
        return Ok(json!({ "repo": false }));
    }
    // Past this point the folder is a repository, so the pane can still offer
    // a review (it reads `git diff`, not GitHub) when the PR cannot be read.
    // That review needs a base this repository has; a repo with no origin
    // has no origin/HEAD.
    match get_in_repo(cwd, link_only).await {
        Ok(pr) => Ok(pr),
        Err(e) => Ok(json!({
            "error": format!("{e:#}"),
            "repo": true,
            "defaultBranch": crate::git::local_base(cwd).await,
        })),
    }
}

async fn get_in_repo(cwd: &str, link_only: bool) -> Result<Value> {
    let (_, branch) = git(cwd, &["branch", "--show-current"]).await?;
    let branch = branch.trim().to_string();
    if branch.is_empty() {
        bail!("no branch is checked out");
    }

    let fields = if link_only { "number,title,url" } else { PR_FIELDS };
    let view = match gh(cwd, &["pr", "view", "--json", fields]).await? {
        Ok(out) => out,
        Err(stderr) if stderr.contains("no pull requests found") => {
            return Ok(json!({
                "none": true,
                "branch": branch,
                "defaultBranch": crate::git::local_base(cwd).await,
            }));
        }
        Err(stderr) => bail!("{}", short_gh_error(&stderr)),
    };
    let pr: Value = serde_json::from_str(&view).context("gh returned something that is not JSON")?;

    if link_only {
        return Ok(json!({ "number": pr["number"], "title": pr["title"], "url": pr["url"] }));
    }

    let rollup = pr["statusCheckRollup"].as_array().cloned().unwrap_or_default();
    let checks: Vec<Value> = rollup.iter().take(MAX_CHECKS).map(check).collect();

    let mut out = json!({
        "number": pr["number"],
        "title": pr["title"],
        "url": pr["url"],
        "state": pr["state"],
        "isDraft": pr["isDraft"],
        "base": pr["baseRefName"],
        "head": pr["headRefName"],
        "reviewDecision": pr["reviewDecision"],
        "mergeable": pr["mergeable"],
        "mergeState": pr["mergeStateStatus"],
        "checks": checks,
        "checksTruncated": rollup.len() > MAX_CHECKS,
        "threads": [],
    });

    // Threads are a second call; a failure there should not hide the PR.
    match threads(cwd, &pr).await {
        Ok(t) => out["threads"] = t,
        Err(e) => out["threadsError"] = json!(format!("{e:#}")),
    }
    Ok(out)
}

/// `https://github.com/<owner>/<repo>/pull/<number>`, from a link that may
/// add a tab (`/files`), an anchor or a query. `None` for anything else.
/// The result goes to gh as an argument, so it is rebuilt from checked
/// parts rather than passed through.
pub fn canonical_pr_url(raw: &str) -> Option<String> {
    if raw.len() > 2048 {
        return None;
    }
    let rest = raw.strip_prefix("https://github.com/")?;
    let path = rest.split(['#', '?']).next()?;
    let mut parts = path.split('/');
    let (owner, repo, pull, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let owner_ok = (1..=39).contains(&owner.len())
        && !owner.starts_with('-')
        && owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    let repo_ok = (1..=100).contains(&repo.len())
        && repo != "."
        && repo != ".."
        && repo.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    let number_ok = (1..=9).contains(&number.len()) && !number.starts_with('0') && number.bytes().all(|b| b.is_ascii_digit());
    // A tab of the PR page (`files`, `commits/<sha>`, `checks`), or a trailing slash.
    let tail_ok = parts.all(|p| p.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-')));
    (owner_ok && repo_ok && pull == "pull" && number_ok && tail_ok).then(|| format!("https://github.com/{owner}/{repo}/pull/{number}"))
}

async fn peek(cwd: &str, url: &str) -> std::result::Result<Value, String> {
    let cell = {
        let mut peeks = PEEKS.lock().unwrap();
        let now = Instant::now();
        // Drops stale answers, so a later ask runs gh again; runs still in flight stay.
        peeks.retain(|_, cell| match cell.get() {
            None => true,
            Some((at, Ok(_))) => now.duration_since(*at) < PEEK_FRESH,
            Some((at, Err(_))) => now.duration_since(*at) < PEEK_ERROR_FRESH,
        });
        peeks.entry(url.to_string()).or_default().clone()
    };
    let (_, answer) = cell.get_or_init(|| async { (Instant::now(), fetch_peek(cwd, url).await) }).await;
    answer.clone()
}

async fn fetch_peek(cwd: &str, url: &str) -> std::result::Result<Value, String> {
    // A full URL needs no repository; a removed session folder still works.
    let dir = if Path::new(cwd).is_dir() { cwd.to_string() } else { std::env::temp_dir().to_string_lossy().into_owned() };
    let out = match gh(&dir, &["pr", "view", url, "--json", PEEK_FIELDS]).await {
        Err(e) => return Err(format!("{e:#}")),
        Ok(Err(stderr)) => {
            let lower = stderr.to_lowercase();
            // gh says the same for a private repository the login cannot see.
            if lower.contains("could not resolve to a") {
                return Err("no such pull request, or no access to it".into());
            }
            return Err(short_gh_error(&stderr));
        }
        Ok(Ok(out)) => out,
    };
    let pr: Value = serde_json::from_str(&out).map_err(|_| "gh returned something that is not JSON".to_string())?;
    Ok(peek_summary(url, &pr))
}

/// The hover card's fields. Title and names come from GitHub: capped, and
/// rendered as text by the UI.
fn peek_summary(url: &str, pr: &Value) -> Value {
    let (mut passed, mut failed, mut running) = (0, 0, 0);
    let mut failing = Vec::new();
    for c in pr["statusCheckRollup"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(check) {
        match c["result"].as_str() {
            Some("passed") => passed += 1,
            Some("running") => running += 1,
            Some("failed") => {
                failed += 1;
                if failing.len() < MAX_FAILING_NAMES {
                    failing.push(clip(c["name"].as_str().unwrap_or("check"), MAX_NAME_CHARS));
                }
            }
            _ => {}
        }
    }
    let repo = url.trim_start_matches("https://github.com/").split("/pull/").next().unwrap_or("");
    let text = |v: &Value, max: usize| v.as_str().map(|s| json!(clip(s, max))).unwrap_or(Value::Null);
    json!({
        "number": pr["number"],
        "title": text(&pr["title"], MAX_TITLE_CHARS),
        "url": url,
        "repo": repo,
        "state": pr["state"],
        "isDraft": pr["isDraft"].as_bool().unwrap_or(false),
        // A deleted account has no author.
        "author": pr["author"]["login"].as_str().map(|s| clip(s, MAX_NAME_CHARS)).unwrap_or_else(|| "ghost".into()),
        "base": text(&pr["baseRefName"], MAX_NAME_CHARS * 3),
        "head": text(&pr["headRefName"], MAX_NAME_CHARS * 3),
        "reviewDecision": pr["reviewDecision"],
        "mergeState": pr["mergeStateStatus"],
        "additions": pr["additions"],
        "deletions": pr["deletions"],
        "updatedAt": pr["updatedAt"],
        "checks": { "passed": passed, "failed": failed, "running": running, "failing": failing },
    })
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// One row of `statusCheckRollup`, either a CheckRun or a commit StatusContext,
/// reduced to what the pane shows.
fn check(c: &Value) -> Value {
    let (name, url, result) = if c["__typename"] == "StatusContext" {
        let result = match c["state"].as_str().unwrap_or("") {
            "SUCCESS" => "passed",
            "FAILURE" | "ERROR" => "failed",
            _ => "running",
        };
        (&c["context"], &c["targetUrl"], result)
    } else {
        let result = if c["status"].as_str() != Some("COMPLETED") {
            "running"
        } else {
            match c["conclusion"].as_str().unwrap_or("") {
                "SUCCESS" => "passed",
                "SKIPPED" => "skipped",
                "NEUTRAL" => "neutral",
                _ => "failed",
            }
        };
        (&c["name"], &c["detailsUrl"], result)
    };
    json!({ "name": name, "workflow": c["workflowName"], "result": result, "url": url })
}

async fn threads(cwd: &str, pr: &Value) -> Result<Value> {
    // The PR may live on the upstream of a fork, so take owner/name from its URL.
    let url = pr["url"].as_str().context("the PR has no URL")?;
    let parts: Vec<&str> = url.trim_end_matches('/').rsplit('/').collect();
    let (Some(number), Some(name), Some(owner)) = (parts.first(), parts.get(2), parts.get(3)) else {
        bail!("cannot read the repository from {url}");
    };
    let out = gh(
        cwd,
        &[
            "api",
            "graphql",
            "-f",
            &format!("query={THREADS_QUERY}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
            "-F",
            &format!("number={number}"),
        ],
    )
    .await?
    .map_err(|stderr| anyhow::anyhow!("{}", short_gh_error(&stderr)))?;
    let data: Value = serde_json::from_str(&out).context("gh returned something that is not JSON")?;
    let nodes = data["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let threads: Vec<Value> = nodes
        .iter()
        .take(MAX_THREADS)
        .map(|t| {
            let comments: Vec<Value> = t["comments"]["nodes"]
                .as_array()
                .map(|cs| {
                    cs.iter()
                        .take(MAX_COMMENTS)
                        .map(|c| {
                            json!({
                                // A deleted account has no author.
                                "author": c["author"]["login"].as_str().unwrap_or("ghost"),
                                "body": cap(c["body"].as_str().unwrap_or("")),
                                "url": c["url"],
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            json!({
                "path": t["path"],
                // Outdated threads lose `line`; the original still says where it was.
                "line": if t["line"].is_null() { t["originalLine"].clone() } else { t["line"].clone() },
                "resolved": t["isResolved"].as_bool().unwrap_or(false),
                "outdated": t["isOutdated"].as_bool().unwrap_or(false),
                "comments": comments,
            })
        })
        .collect();
    Ok(json!(threads))
}

fn cap(body: &str) -> String {
    if body.chars().count() <= MAX_BODY_CHARS {
        return body.to_string();
    }
    let mut s: String = body.chars().take(MAX_BODY_CHARS).collect();
    s.push_str(" [cut]");
    s
}

/// gh's own messages are long and point at flags the user never typed.
fn short_gh_error(stderr: &str) -> String {
    let lower = stderr.to_lowercase();
    if lower.contains("gh auth login") || lower.contains("not logged") || lower.contains("authentication") {
        return "gh is not logged in".into();
    }
    if lower.contains("none of the git remotes") || lower.contains("no git remotes") {
        return "no GitHub remote".into();
    }
    let first = stderr.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("gh failed");
    first.chars().take(200).collect()
}

/// Outer error: gh could not run at all. Inner: gh ran and failed, with stderr.
async fn gh(cwd: &str, args: &[&str]) -> Result<std::result::Result<String, String>> {
    let child = Command::new("gh")
        .args(args)
        .current_dir(cwd)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .kill_on_drop(true)
        .output();
    let out = match tokio::time::timeout(GH_TIMEOUT, child).await {
        Err(_) => bail!("gh did not answer within {} s", GH_TIMEOUT.as_secs()),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => bail!("gh is not installed"),
        Ok(Err(e)) => return Err(e).context("could not run gh"),
        Ok(Ok(out)) => out,
    };
    if out.status.success() {
        Ok(Ok(String::from_utf8_lossy(&out.stdout).into_owned()))
    } else {
        Ok(Err(String::from_utf8_lossy(&out.stderr).into_owned()))
    }
}

async fn git(cwd: &str, args: &[&str]) -> Result<(bool, String)> {
    let out = Command::new("git").arg("-C").arg(cwd).args(args).output().await?;
    Ok((out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pr_link_is_reduced_to_its_canonical_url() {
        let canon = "https://github.com/acme-co/my.repo_1/pull/42";
        for raw in [
            canon,
            "https://github.com/acme-co/my.repo_1/pull/42/",
            "https://github.com/acme-co/my.repo_1/pull/42/files",
            "https://github.com/acme-co/my.repo_1/pull/42/commits/0a1b2c3",
            "https://github.com/acme-co/my.repo_1/pull/42#issuecomment-1",
            "https://github.com/acme-co/my.repo_1/pull/42?w=1",
            "https://github.com/acme-co/my.repo_1/pull/42/files?diff=split#r9",
        ] {
            assert_eq!(canonical_pr_url(raw).as_deref(), Some(canon), "{raw}");
        }
    }

    #[test]
    fn anything_else_is_refused() {
        for raw in [
            "",
            "--repo=x",
            "-R https://github.com/a/b/pull/1",
            "http://github.com/a/b/pull/1",
            "https://github.com.evil.example/a/b/pull/1",
            "https://gist.github.com/a/b/pull/1",
            "https://github.com/a/b/issues/1",
            "https://github.com/a/b/pull/",
            "https://github.com/a/b/pull/0",
            "https://github.com/a/b/pull/012",
            "https://github.com/a/b/pull/1x",
            "https://github.com/a/b/pull/1234567890",
            "https://github.com/a/b",
            "https://github.com//b/pull/1",
            "https://github.com/-a/b/pull/1",
            "https://github.com/a_b/c/pull/1",
            "https://github.com/a/../pull/1",
            "https://github.com/a/b c/pull/1",
            "https://github.com/a/b/pull/1/files evil",
            " https://github.com/a/b/pull/1",
        ] {
            assert_eq!(canonical_pr_url(raw), None, "{raw}");
        }
        // The tail never reaches gh: the URL is rebuilt without it.
        assert_eq!(canonical_pr_url("https://github.com/a/b/pull/1/--json").as_deref(), Some("https://github.com/a/b/pull/1"));
        assert_eq!(canonical_pr_url(&format!("https://github.com/a/b/pull/1#{}", "x".repeat(3000))), None);
    }

    #[test]
    fn the_peek_counts_checks_and_names_the_failing_ones() {
        let pr = json!({
            "number": 7, "title": "Fix it", "state": "OPEN", "isDraft": true,
            "author": { "login": "octo" }, "baseRefName": "main", "headRefName": "fix",
            "reviewDecision": "APPROVED", "mergeStateStatus": "CLEAN",
            "additions": 12, "deletions": 3, "updatedAt": "2026-11-16T10:00:00Z",
            "statusCheckRollup": [
                { "__typename": "CheckRun", "name": "test", "status": "COMPLETED", "conclusion": "SUCCESS" },
                { "__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "FAILURE" },
                { "__typename": "CheckRun", "name": "build", "status": "IN_PROGRESS" },
                { "__typename": "CheckRun", "name": "docs", "status": "COMPLETED", "conclusion": "SKIPPED" },
                { "__typename": "StatusContext", "context": "ci/legacy", "state": "ERROR" },
            ],
        });
        let p = peek_summary("https://github.com/o/r/pull/7", &pr);
        assert_eq!(p["repo"], "o/r");
        assert_eq!(p["author"], "octo");
        assert_eq!(p["isDraft"], true);
        assert_eq!(p["checks"], json!({ "passed": 1, "failed": 2, "running": 1, "failing": ["lint", "ci/legacy"] }));
        let ghost = peek_summary("https://github.com/o/r/pull/7", &json!({ "title": "x".repeat(400) }));
        assert_eq!(ghost["author"], "ghost");
        assert_eq!(ghost["title"].as_str().unwrap().chars().count(), MAX_TITLE_CHARS);
    }
}
