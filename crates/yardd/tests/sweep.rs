//! The merged-worktree sweep: a clean worktree whose HEAD is the head of a
//! merged PR goes away with its branch, in any repository; a dirty one, an
//! unmerged one and one with a running session stay. `gh` is the fake in
//! `verify/fakes/gh` (needs Bun).

mod common;

use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    time::{Duration, Instant},
};

use common::*;
use serde_json::json;

/// A session in its own worktree with one commit there; returns its id, folder, branch and HEAD.
async fn worktree_with_commit(c: &mut Client, sb: &Sandbox, repo: &str, text: &str) -> (String, String, String, String) {
    let sid = c.create(repo, text, json!({ "worktree": true })).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let meta = c.wait_for("the worktree session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    let wt = meta["cwd"].as_str().unwrap().to_string();
    let home = sb.home();
    fs::write(Path::new(&wt).join(format!("{sid}.txt")), "work\n").unwrap();
    git(&wt, &["add", "."], &home);
    git(&wt, &["commit", "-q", "-m", "work"], &home);
    let head = git(&wt, &["rev-parse", "HEAD"], &home);
    (sid, wt, meta["branch"].as_str().unwrap().to_string(), head)
}

#[tokio::test]
async fn a_merged_worktree_is_removed_and_nothing_else() {
    let sb = Sandbox::new("sweep");
    let bin = sb.root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    symlink(repo_root().join("verify/fakes/gh"), bin.join("gh")).unwrap();
    let path = format!("{}:{}:{}", bin.display(), fake_bin().display(), std::env::var("PATH").unwrap_or_default());
    let repo = make_repo(&sb, "repo");
    let other = make_repo(&sb, "other");
    let d = sb.start_with(&[("PATH", &path), ("YARD_WORKTREE_SWEEP_SECS", "1")]);
    let mut c = d.client().await;
    let home = sb.home();

    let (merged, merged_wt, merged_branch, merged_head) = worktree_with_commit(&mut c, &sb, &repo, "merged").await;
    let (_, dirty_wt, _, dirty_head) = worktree_with_commit(&mut c, &sb, &repo, "dirty").await;
    let (_, open_wt, _, _) = worktree_with_commit(&mut c, &sb, &repo, "open").await;
    // Another repository is swept the same way.
    let (_, other_wt, _, other_head) = worktree_with_commit(&mut c, &sb, &other, "elsewhere").await;
    // Merged, then committed to again: its HEAD is no PR's head.
    let (_, later_wt, _, later_head) = worktree_with_commit(&mut c, &sb, &repo, "later").await;
    fs::write(Path::new(&later_wt).join("more.txt"), "more\n").unwrap();
    git(&later_wt, &["add", "."], &home);
    git(&later_wt, &["commit", "-q", "-m", "more"], &home);
    fs::write(Path::new(&dirty_wt).join("unsaved.txt"), "unsaved\n").unwrap();

    let pr = |n: u64, head: &str| json!({ "number": n, "headRefOid": head });
    let merged_prs = json!([pr(41, &merged_head), pr(42, &dirty_head), pr(43, &other_head), pr(44, &later_head)]);
    fs::write(sb.fake_state().join("gh-pr.json"), json!({ "merged": merged_prs }).to_string()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(30);
    while (Path::new(&merged_wt).exists() || Path::new(&other_wt).exists()) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(!Path::new(&merged_wt).exists(), "the merged worktree is removed\n{}", d.log_text());
    assert!(!Path::new(&other_wt).exists(), "in any repository");
    assert_eq!(git(&repo, &["branch", "--list", &merged_branch], &home), "", "its branch goes too");
    let note = c.wait_event(&merged, "note", 0).await;
    assert_eq!(note["text"], format!("Worktree and branch {merged_branch} removed: pull request #41 was merged."));

    // A few more sweeps leave the rest alone.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(Path::new(&dirty_wt).join("unsaved.txt").is_file(), "uncommitted work stays");
    assert!(Path::new(&open_wt).is_dir(), "an unmerged worktree stays");
    assert!(Path::new(&later_wt).is_dir(), "commits after the merge stay");
}

#[tokio::test]
async fn a_worktree_with_a_running_session_or_process_stays() {
    let sb = Sandbox::new("sweepbusy");
    let bin = sb.root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    symlink(repo_root().join("verify/fakes/gh"), bin.join("gh")).unwrap();
    let path = format!("{}:{}:{}", bin.display(), fake_bin().display(), std::env::var("PATH").unwrap_or_default());
    let repo = make_repo(&sb, "repo");
    let d = sb.start_with(&[("PATH", &path), ("YARD_WORKTREE_SWEEP_SECS", "1")]);
    let mut c = d.client().await;

    let (_, shell_wt, _, shell_head) = worktree_with_commit(&mut c, &sb, &repo, "shell").await;
    let (busy, busy_wt, _, busy_head) = worktree_with_commit(&mut c, &sb, &repo, "busy").await;
    // A process sitting in the folder, like a dev server or a terminal.
    let mut sleeper = std::process::Command::new("sleep").arg("30").current_dir(&shell_wt).spawn().unwrap();
    // A turn still running there.
    let seq = c.next_seq(&busy);
    c.send(json!({ "op": "session.send", "sessionId": busy, "text": "[slow] keep going" })).await;
    c.wait_event(&busy, "turn_started", seq).await;

    let merged_prs = json!([{ "number": 1, "headRefOid": shell_head }, { "number": 2, "headRefOid": busy_head }]);
    fs::write(sb.fake_state().join("gh-pr.json"), json!({ "merged": merged_prs }).to_string()).unwrap();
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(Path::new(&shell_wt).is_dir(), "a process inside keeps it");
    assert!(Path::new(&busy_wt).is_dir(), "a running session keeps it");

    // Once the process ends it goes.
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Path::new(&shell_wt).exists() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(!Path::new(&shell_wt).exists(), "{}", d.log_text());
}

/// A session in its own worktree, left as created (HEAD on the base branch).
async fn worktree_session(c: &mut Client, repo: &str, text: &str) -> (String, String, String) {
    let sid = c.create(repo, text, json!({ "worktree": true })).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let meta = c.wait_for("the worktree session", |m| m["op"] == "session" && m["session"]["id"] == sid).await["session"].clone();
    (sid, meta["cwd"].as_str().unwrap().to_string(), meta["branch"].as_str().unwrap().to_string())
}

/// Automation runs that end without a PR leave their worktree behind; once
/// every session there is archived and its HEAD is already on another
/// branch, nothing is lost by removing it. Unpushed commits, uncommitted
/// files and a session still in the list keep it.
#[tokio::test]
async fn an_archived_worktree_with_nothing_of_its_own_is_removed() {
    let sb = Sandbox::new("sweeparch");
    let bin = sb.root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    symlink(repo_root().join("verify/fakes/gh"), bin.join("gh")).unwrap();
    let path = format!("{}:{}:{}", bin.display(), fake_bin().display(), std::env::var("PATH").unwrap_or_default());
    let repo = make_repo(&sb, "repo");
    let d = sb.start_with(&[("PATH", &path), ("YARD_WORKTREE_SWEEP_SECS", "1")]);
    let mut c = d.client().await;
    let home = sb.home();
    fs::write(sb.fake_state().join("gh-pr.json"), json!({ "merged": [] }).to_string()).unwrap();

    let (spent, spent_wt, spent_branch) = worktree_session(&mut c, &repo, "spent").await;
    // Its commit is also on another branch, as after a push.
    let (pushed, pushed_wt, _, _) = worktree_with_commit(&mut c, &sb, &repo, "pushed").await;
    git(&pushed_wt, &["branch", "elsewhere"], &home);
    let (own, own_wt, _, _) = worktree_with_commit(&mut c, &sb, &repo, "own").await;
    let (dirty, dirty_wt, _) = worktree_session(&mut c, &repo, "dirty").await;
    fs::write(Path::new(&dirty_wt).join("unsaved.txt"), "unsaved\n").unwrap();
    let (_, listed_wt, _) = worktree_session(&mut c, &repo, "listed").await;
    for sid in [&spent, &pushed, &own, &dirty] {
        c.send(json!({ "op": "session.update", "sessionId": sid, "archived": true })).await;
    }

    let deadline = Instant::now() + Duration::from_secs(30);
    while (Path::new(&spent_wt).exists() || Path::new(&pushed_wt).exists()) && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    assert!(!Path::new(&spent_wt).exists(), "an archived worktree on its base is removed\n{}", d.log_text());
    assert!(!Path::new(&pushed_wt).exists(), "so is one whose commits are on another branch");
    assert_eq!(git(&repo, &["branch", "--list", &spent_branch], &home), "", "its branch goes too");
    let note = c.wait_event(&spent, "note", 0).await;
    assert_eq!(
        note["text"],
        format!("Worktree and branch {spent_branch} removed: its sessions are archived and its commits are on another branch.")
    );

    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(Path::new(&own_wt).is_dir(), "commits on no other branch stay");
    assert!(Path::new(&dirty_wt).join("unsaved.txt").is_file(), "uncommitted work stays");
    assert!(Path::new(&listed_wt).is_dir(), "a session still in the list keeps it");
}
