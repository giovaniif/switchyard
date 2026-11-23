//! Git: working-tree changes for the Changes pane, branches, and session
//! worktrees with their setup and removal.

use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tokio::{io::AsyncReadExt, process::Command};

const MAX_DIFF_BYTES: usize = 400_000;
const MAX_BRANCHES: usize = 200;
/// Setup scripts may install dependencies, but not forever.
const SETUP_TIMEOUT: Duration = Duration::from_secs(300);
/// The tail of a setup script's output that is kept for the error message.
const SETUP_OUTPUT_CAP: usize = 16_000;
const SETUP_ERROR_LINES: usize = 20;
/// An automation fetches before each run; a hung remote must not stall it.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

pub struct NewWorktree {
    /// Where the session runs: the worktree, or the same subfolder inside it.
    pub cwd: String,
    pub branch: String,
    /// The checkout it was cut from.
    pub repo: String,
    /// The setup script that ran in it, if any.
    pub setup: Option<String>,
    /// Build caches cloned in from the original checkout (see `warm`).
    pub warmed: Vec<String>,
}

/// Where yardd puts the worktrees it makes: `<data dir>/wt`.
pub fn worktree_root() -> PathBuf {
    crate::store::data_dir().join("wt")
}

/// `Fix the add function!` -> `fix-the-add-function`
fn slug(text: &str) -> String {
    // Whole words only, so the name never ends mid-word.
    let mut out = String::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).take(5) {
        if out.len() + word.len() + 1 > 32 {
            break;
        }
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(&word.to_ascii_lowercase());
    }
    if out.is_empty() { "session".to_string() } else { out }
}

/// Cut a new branch and worktree from `base` (the folder's current HEAD when
/// unset), under `<data dir>/wt/<repo>/<branch>`, clone the original
/// checkout's build caches into it (`warm`), then run the repo's setup
/// script in it. Nothing in the original checkout is touched.
///
/// Callers check [`can_branch`] first: every session in a repository gets
/// its own worktree, and a folder that cannot have one runs in place. A failed
/// `git worktree add` stops here, before any setup; a failed setup removes
/// the worktree and branch it just made.
pub async fn add_worktree(
    folder: &str,
    branch: Option<&str>,
    base: Option<&str>,
    title: &str,
    suffix: &str,
) -> Result<NewWorktree> {
    let (is_repo, top) = git(folder, &["rev-parse", "--show-toplevel"]).await?;
    if !is_repo {
        bail!("{folder} is not a git repository, so it cannot have a worktree");
    }
    let top = top.trim().to_string();
    let (has_head, _) = git(&top, &["rev-parse", "--verify", "HEAD"]).await?;
    if !has_head {
        bail!("the repository has no commits yet, so there is nothing to branch from");
    }
    // Resolved to a commit id up front, so the name can never be read as an
    // option and `worktree add` gets exactly what was checked.
    let start = match base.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => resolve_base(&top, b).await?,
        None => "HEAD".to_string(),
    };
    // The subfolder the session was pointed at, e.g. `apps/web/`.
    let (_, prefix) = git(folder, &["rev-parse", "--show-prefix"]).await?;

    let branch = match branch.map(str::trim).filter(|b| !b.is_empty()) {
        Some(b) => b.to_string(),
        None => format!("yard/{}-{suffix}", slug(title)),
    };
    let (valid, _) = git(&top, &["check-ref-format", "--branch", &branch]).await?;
    if !valid || branch.starts_with('-') {
        bail!("`{branch}` is not a valid branch name");
    }
    let (exists, _) = git(&top, &["show-ref", "--verify", "--quiet", &format!("refs/heads/{branch}")]).await?;
    if exists {
        bail!("branch `{branch}` already exists");
    }

    let repo_name = Path::new(&top)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo")
        .to_string();
    let path: PathBuf = worktree_root().join(&repo_name).join(branch.replace('/', "-"));
    if path.exists() {
        bail!("{} already exists", path.display());
    }
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let path_str = path.to_str().context("worktree path is not valid UTF-8")?;

    let out = Command::new("git")
        .arg("-C")
        .arg(&top)
        .args(["worktree", "add", "-b", &branch, path_str, &start])
        .output()
        .await?;
    if !out.status.success() {
        bail!("git worktree add failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }

    // Before setup, so an install or build in the setup script starts warm.
    let warmed = crate::warm::clone_caches(Path::new(&top), &path).await;
    let setup = match run_setup(&top, &path, &repo_name).await {
        Ok(script) => script,
        Err(e) => {
            discard_worktree(&top, path_str, &branch).await;
            return Err(e);
        }
    };

    let cwd = path.join(prefix.trim());
    Ok(NewWorktree {
        cwd: cwd.to_str().unwrap_or(path_str).trim_end_matches('/').to_string(),
        branch,
        repo: top,
        setup,
        warmed,
    })
}

async fn resolve_base(top: &str, base: &str) -> Result<String> {
    if base.starts_with('-') {
        bail!("the base `{base}` cannot start with `-`");
    }
    let spec = format!("{base}^{{commit}}");
    let (ok, sha) = git(top, &["rev-parse", "--verify", "--quiet", "--end-of-options", &spec]).await?;
    if !ok || sha.trim().is_empty() {
        bail!("`{base}` is not a branch or commit in this repository");
    }
    Ok(sha.trim().to_string())
}

/// Undo a worktree whose setup failed: exactly the path and branch just made.
async fn discard_worktree(top: &str, path: &str, branch: &str) {
    for args in [["worktree", "remove", "--force", path], ["branch", "-D", "--", branch]] {
        if !matches!(git(top, &args).await, Ok((true, _))) {
            eprintln!("yardd: cleanup `git {}` failed in {top}", args.join(" "));
        }
    }
}

/// The setup script for a new worktree, if the user or the repo has one: a
/// user-level `<data dir>/setup/<repo>.sh` first, for repos where nothing may
/// be committed, then the repo's own `.yard/worktree-setup.sh`.
fn setup_script(worktree: &Path, repo_name: &str) -> Option<PathBuf> {
    let user = crate::store::data_dir().join("setup").join(format!("{repo_name}.sh"));
    let tracked = worktree.join(".yard").join("worktree-setup.sh");
    [user, tracked].into_iter().find(|p| p.is_file())
}

/// Run the setup script inside the new worktree. Returns the script's path
/// when one ran. It never runs in the original checkout: a setup that once ran
/// there after a failed `worktree add` clobbered it.
async fn run_setup(top: &str, worktree: &Path, repo_name: &str) -> Result<Option<String>> {
    let Some(script) = setup_script(worktree, repo_name) else {
        return Ok(None);
    };
    if !worktree.starts_with(worktree_root()) || Path::new(top) == worktree || !worktree.is_dir() {
        bail!("refusing to run worktree setup outside {}", worktree_root().display());
    }
    let shown = script.display().to_string();

    let mut child = Command::new("bash")
        // stderr joins stdout so the tail shows both in order.
        .args(["-c", "exec bash -- \"$0\" 2>&1"])
        .arg(&script)
        .current_dir(worktree)
        .env("YARD_REPO", top)
        .env("YARD_WORKTREE", worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Its own process group, so a timeout can stop everything it started.
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .with_context(|| format!("could not run {shown}"))?;
    let pid = child.id();
    let mut stdout = child.stdout.take().context("no output pipe")?;
    let reader = tokio::spawn(async move {
        let mut tail: Vec<u8> = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok(n) = stdout.read(&mut buf).await {
            if n == 0 {
                break;
            }
            tail.extend_from_slice(&buf[..n]);
            if tail.len() > SETUP_OUTPUT_CAP * 2 {
                tail.drain(..tail.len() - SETUP_OUTPUT_CAP);
            }
        }
        tail
    });

    let status = tokio::time::timeout(SETUP_TIMEOUT, child.wait()).await;
    if status.is_err() {
        if let Some(pid) = pid {
            let _ = std::process::Command::new("kill").args(["-KILL", "--", &format!("-{pid}")]).status();
        }
        let _ = child.kill().await;
    }
    // A background process left holding the pipe must not hold us up.
    let tail = match tokio::time::timeout(Duration::from_secs(5), reader).await {
        Ok(Ok(t)) => t,
        _ => Vec::new(),
    };
    let output = last_lines(&String::from_utf8_lossy(&tail), SETUP_ERROR_LINES);
    let failure = match status {
        Err(_) => Some(format!("timed out after {} minutes", SETUP_TIMEOUT.as_secs() / 60)),
        Ok(Err(e)) => Some(format!("could not be waited on: {e}")),
        Ok(Ok(s)) if !s.success() => Some(format!("failed ({s})")),
        Ok(Ok(_)) => None,
    };
    match failure {
        None => Ok(Some(shown)),
        Some(why) if output.is_empty() => bail!("worktree setup {shown} {why}; the worktree was removed"),
        Some(why) => bail!("worktree setup {shown} {why}; the worktree was removed. Last output:\n{output}"),
    }
}

fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    lines[lines.len().saturating_sub(n)..].join("\n")
}

/// Local branches and `origin/*`, most recently committed first.
/// Whether `folder` is in a repository with at least one commit, so a
/// worktree can be cut from it.
pub async fn can_branch(folder: &str) -> Result<bool> {
    let (is_repo, top) = git(folder, &["rev-parse", "--show-toplevel"]).await?;
    if !is_repo {
        return Ok(false);
    }
    Ok(git(top.trim(), &["rev-parse", "--verify", "--quiet", "HEAD"]).await?.0)
}

pub async fn branches(cwd: &str) -> Result<Value> {
    let (is_repo, top) = git(cwd, &["rev-parse", "--show-toplevel"]).await?;
    if !is_repo {
        bail!("{cwd} is not a git repository");
    }
    let top = top.trim();
    let (_, current) = git(top, &["branch", "--show-current"]).await?;
    let current = match current.trim() {
        "" => git(top, &["rev-parse", "--short", "HEAD"]).await?.1.trim().to_string(),
        name => name.to_string(),
    };
    let (_, list) = git(
        top,
        &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads", "refs/remotes/origin"],
    )
    .await?;
    let branches: Vec<&str> = list
        .lines()
        .map(str::trim)
        // `origin/HEAD` is a pointer, not a branch.
        .filter(|b| !b.is_empty() && *b != "origin" && !b.ends_with("/HEAD"))
        .take(MAX_BRANCHES)
        .collect();
    Ok(json!({ "current": current, "branches": branches }))
}

/// What removing a session's worktree did.
pub struct Removed {
    pub branch_deleted: bool,
    pub message: String,
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The top of the worktree a session runs in, checked to be one yardd made
/// and safe to remove: inside the worktree root, not the original checkout,
/// and with nothing uncommitted or untracked.
pub async fn removable_worktree(cwd: &str, repo: &str) -> Result<String> {
    let (ok, top) = git(cwd, &["rev-parse", "--show-toplevel"]).await?;
    if !ok {
        bail!("{cwd} is no longer a git worktree");
    }
    let top = top.trim().to_string();
    let root = canon(&worktree_root());
    let at = canon(Path::new(&top));
    if !at.starts_with(&root) || at == root || at == canon(Path::new(repo)) {
        bail!("{top} is not a worktree yardd made, so it is not removed");
    }
    if !Path::new(repo).is_dir() {
        bail!("the original checkout {repo} is gone, so the worktree cannot be removed from it");
    }
    let (ok, status) = git(&top, &["status", "--porcelain"]).await?;
    if !ok {
        bail!("could not read the worktree's status");
    }
    let dirty = status.lines().filter(|l| !l.trim().is_empty()).count();
    if dirty > 0 {
        bail!(
            "The worktree has uncommitted or untracked changes ({dirty} file{}). Commit or discard them first. Nothing was deleted.",
            if dirty == 1 { "" } else { "s" }
        );
    }
    Ok(top)
}

/// `git worktree remove` (never forced), then `git branch -d`, which itself
/// keeps a branch with unmerged commits.
pub async fn remove_worktree(repo: &str, path: &str, branch: &str) -> Removed {
    let kept = |why: String| Removed { branch_deleted: false, message: why };
    match git_err(repo, &["worktree", "remove", "--", path]).await {
        Ok(()) => {}
        Err(e) => return kept(format!("Session deleted, but the worktree was kept: {e}")),
    }
    match git_err(repo, &["branch", "-d", "--", branch]).await {
        Ok(()) => Removed { branch_deleted: true, message: format!("Worktree removed and branch {branch} deleted.") },
        Err(e) => kept(format!("Worktree removed. Branch {branch} was kept: {e}")),
    }
}

/// Run git; an error carries its stderr.
async fn git_err(cwd: &str, args: &[&str]) -> Result<()> {
    let out = Command::new("git").arg("-C").arg(cwd).args(args).output().await?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

async fn git(cwd: &str, args: &[&str]) -> Result<(bool, String)> {
    // Paths as written: by default git prints a name with non-ASCII letters
    // as quoted octal ("caf\303\251.txt"), and the pane would show that.
    let out = Command::new("git").arg("-C").arg(cwd).args(["-c", "core.quotePath=false"]).args(args).output().await?;
    Ok((out.status.success(), String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// A session's folder can go away outside Switchyard (a worktree pruned, a
/// folder moved). Git would call that "not a git repository" and a shell
/// would quietly start in the home folder, so say what happened.
pub fn ensure_folder(cwd: &str) -> Result<()> {
    if !Path::new(cwd).is_dir() {
        bail!("this session's folder no longer exists: {cwd}");
    }
    Ok(())
}

/// An untracked file as `git diff` would show it once added: every line a
/// `+`. Read here rather than one `git diff --no-index` process per file. At
/// most `room` bytes are read, so the caller's cap still cuts a big file. A
/// binary file (a NUL byte), an empty one, or anything that is not a regular
/// file (a symlink may point outside the folder) gets the header alone, which
/// the pane shows as "No text diff".
async fn new_file_diff(cwd: &str, path: &str, room: usize) -> String {
    let mut out = format!("diff --git a/{path} b/{path}\nnew file mode 100644\n");
    let full = Path::new(cwd).join(path);
    if !matches!(tokio::fs::symlink_metadata(&full).await, Ok(m) if m.is_file()) {
        return out;
    }
    let mut bytes = Vec::new();
    let Ok(file) = tokio::fs::File::open(&full).await else {
        return out;
    };
    if file.take(room as u64).read_to_end(&mut bytes).await.is_err() || bytes.is_empty() || bytes.contains(&0) {
        return out;
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.split_terminator('\n').collect();
    out.push_str(&format!("--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n", lines.len()));
    for line in lines {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    if !text.ends_with('\n') {
        out.push_str("\\ No newline at end of file\n");
    }
    out
}

/// `git diff --numstat -z`. Without `-z` a rename is one compact path,
/// `src/{a.ts => b.ts}`; with it, the old and new names come apart: the row
/// ends with an empty path, then the old name and the new one, NUL-separated.
fn parse_numstat(out: &str) -> Vec<Value> {
    let mut fields = out.split('\0');
    let mut files = Vec::new();
    while let Some(row) = fields.next() {
        let mut parts = row.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let mut file = json!({
            "path": path,
            "added": added.parse::<u64>().ok(),
            "removed": removed.parse::<u64>().ok(),
        });
        if path.is_empty() {
            let (Some(from), Some(to)) = (fields.next(), fields.next()) else { break };
            file["path"] = json!(to);
            file["from"] = json!(from);
        }
        files.push(file);
    }
    files
}

pub async fn changes(cwd: &str) -> Result<Value> {
    ensure_folder(cwd)?;
    let (is_repo, top) = git(cwd, &["rev-parse", "--show-toplevel"]).await?;
    if !is_repo {
        bail!("not a git repository");
    }
    // A session can start in a subfolder (`apps/web/`), but `git diff` names
    // files from the root and covers the whole repository. New files are
    // listed and read from the root too, so the pane names every file one
    // way and keeps the ones the agent made outside the subfolder.
    let top = top.trim();
    // A repo with no commits has no HEAD to diff against.
    let (has_head, _) = git(cwd, &["rev-parse", "--verify", "HEAD"]).await?;
    let base: &[&str] = if has_head { &["HEAD"] } else { &[] };

    let (_, numstat) = git(cwd, &[&["diff", "--numstat", "-z"], base].concat()).await?;
    let mut files = parse_numstat(&numstat);

    let (_, untracked) = git(top, &["ls-files", "--others", "--exclude-standard"]).await?;
    let untracked: Vec<&str> = untracked.lines().take(200).collect();
    for path in &untracked {
        files.push(json!({ "path": path, "added": null, "removed": null, "untracked": true }));
    }

    let (_, mut diff) = git(cwd, &[&["diff"], base].concat()).await?;
    // `git diff` leaves untracked files out, and a file the agent created is
    // the change the user most wants to read. Each joins the diff as a new
    // file, until the cap below cuts it off.
    for path in &untracked {
        if diff.len() > MAX_DIFF_BYTES {
            break;
        }
        let room = MAX_DIFF_BYTES + 1 - diff.len();
        diff.push_str(&new_file_diff(top, path, room).await);
    }
    let truncated = diff.len() > MAX_DIFF_BYTES;
    if truncated {
        let mut cut = MAX_DIFF_BYTES;
        while !diff.is_char_boundary(cut) {
            cut -= 1;
        }
        diff.truncate(cut);
    }

    let (_, branch) = git(cwd, &["branch", "--show-current"]).await?;
    Ok(json!({
        "files": files,
        "diff": diff,
        "truncated": truncated,
        "branch": branch.trim(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numstat_names_a_rename_by_its_old_and_new_path() {
        let out = "1\t1\t\0src/greet.ts\0src/hello.ts\03\t0\tREADME.md\0-\t-\tlogo.png\0";
        let files = parse_numstat(out);
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], json!({ "path": "src/hello.ts", "from": "src/greet.ts", "added": 1, "removed": 1 }));
        assert_eq!(files[1], json!({ "path": "README.md", "added": 3, "removed": 0 }));
        assert_eq!(files[2], json!({ "path": "logo.png", "added": null, "removed": null }));
    }

    #[test]
    fn keeps_the_last_non_empty_lines() {
        assert_eq!(last_lines("a\n\nb\nc\n", 2), "b\nc");
        assert_eq!(last_lines("", 3), "");
    }

    fn run(dir: &std::path::Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=TEST", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    #[tokio::test]
    async fn new_files_show_their_lines_in_the_diff() {
        let dir = std::env::temp_dir().join(format!("yardd-changes-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("tracked.txt"), "a\n").unwrap();
        run(&dir, &["init", "-q"]);
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "first"]);
        std::fs::write(dir.join("tracked.txt"), "a\nb\n").unwrap();
        std::fs::write(dir.join("src/new.txt"), "one\ntwo").unwrap();
        std::fs::write(dir.join("blob.bin"), b"x\0y").unwrap();

        let changes = changes(dir.to_str().unwrap()).await.unwrap();
        let diff = changes["diff"].as_str().unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(diff.contains("diff --git a/tracked.txt b/tracked.txt"), "{diff}");
        assert!(
            diff.contains("diff --git a/src/new.txt b/src/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/src/new.txt\n@@ -0,0 +1,2 @@\n+one\n+two\n\\ No newline at end of file\n"),
            "{diff}"
        );
        // A binary file is named with no lines, like `git diff` does.
        assert!(diff.contains("diff --git a/blob.bin b/blob.bin\n"), "{diff}");
        assert!(!diff.contains("+x"), "{diff}");
        assert_eq!(changes["truncated"], false);
    }

    #[tokio::test]
    async fn non_ascii_paths_come_out_as_written() {
        let dir = std::env::temp_dir().join(format!("yardd-quotepath-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("ação.txt"), "a\n").unwrap();
        run(&dir, &["init", "-q"]);
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "first"]);
        std::fs::write(dir.join("ação.txt"), "a\nb\n").unwrap();
        std::fs::write(dir.join("café.txt"), "new\n").unwrap();

        let changes = changes(dir.to_str().unwrap()).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let paths: Vec<&str> = changes["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert_eq!(paths, ["ação.txt", "café.txt"]);
        let diff = changes["diff"].as_str().unwrap();
        assert!(diff.contains("diff --git a/ação.txt b/ação.txt\n"), "{diff}");
        // A new file is read through `ls-files`, which quotes too.
        assert!(diff.contains("+++ b/café.txt\n@@ -0,0 +1,1 @@\n+new\n"), "{diff}");
    }

    #[tokio::test]
    async fn a_subfolder_session_lists_every_file_from_the_repository_root() {
        let dir = std::env::temp_dir().join(format!("yardd-subfolder-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("apps/web")).unwrap();
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        std::fs::write(dir.join("apps/web/a.txt"), "a\n").unwrap();
        std::fs::write(dir.join("docs/d.txt"), "d\n").unwrap();
        run(&dir, &["init", "-q"]);
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "first"]);
        std::fs::write(dir.join("apps/web/a.txt"), "a\nb\n").unwrap();
        std::fs::write(dir.join("apps/web/new.txt"), "inside\n").unwrap();
        std::fs::write(dir.join("docs/other.txt"), "outside\n").unwrap();

        let changes = changes(dir.join("apps/web").to_str().unwrap()).await.unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        // `git diff` names files from the root, so new files must too, or
        // the pane names one folder two ways and drops the other new files.
        let paths: Vec<&str> = changes["files"].as_array().unwrap().iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert_eq!(paths, ["apps/web/a.txt", "apps/web/new.txt", "docs/other.txt"]);
        let diff = changes["diff"].as_str().unwrap();
        assert!(diff.contains("+++ b/apps/web/new.txt\n@@ -0,0 +1,1 @@\n+inside\n"), "{diff}");
        assert!(diff.contains("+++ b/docs/other.txt\n@@ -0,0 +1,1 @@\n+outside\n"), "{diff}");
    }
}

/// The ref an automation's worktree is cut from, after fetching `origin`.
/// `base` is a branch the user picked; empty means the repository's default
/// branch (`origin/HEAD`, else main or master). A branch that also exists on
/// origin is taken from there, so a run starts from what was just fetched
/// and not from a stale local copy. `None` outside a repository with commits
/// (the session then runs in place) or when nothing better than HEAD exists.
pub async fn fresh_base(folder: &str, base: &str) -> Result<Option<String>> {
    if !can_branch(folder).await? {
        return Ok(None);
    }
    let (_, top) = git(folder, &["rev-parse", "--show-toplevel"]).await?;
    let top = top.trim().to_string();
    // No credentials prompt: an unattended run must not hang on one. A failed
    // fetch (offline, no origin) is not fatal; the run uses what is local.
    let fetch = Command::new("git")
        .arg("-C")
        .arg(&top)
        .args(["fetch", "--quiet", "origin"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(FETCH_TIMEOUT, fetch).await {
        Ok(Ok(out)) if !out.status.success() => {
            eprintln!("yardd: git fetch in {top}: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(Err(e)) => eprintln!("yardd: git fetch in {top}: {e}"),
        Err(_) => eprintln!("yardd: git fetch in {top} timed out"),
        _ => {}
    }
    let base = base.trim();
    if !base.is_empty() {
        if !base.starts_with("origin/") && ref_exists(&top, &format!("origin/{base}")).await {
            return Ok(Some(format!("origin/{base}")));
        }
        return Ok(Some(base.to_string()));
    }
    Ok(local_base(&top).await)
}

/// The branch a change in `folder` is measured against, from what is already
/// local: origin's HEAD, then the usual names. None when nothing matches, so
/// a caller never names a ref the repository does not have.
pub async fn local_base(folder: &str) -> Option<String> {
    if let Ok((true, head)) = git(folder, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]).await {
        let head = head.trim();
        if !head.is_empty() && ref_exists(folder, head).await {
            return Some(head.to_string());
        }
    }
    for name in ["origin/main", "origin/master", "main", "master"] {
        if ref_exists(folder, name).await {
            return Some(name.to_string());
        }
    }
    None
}

async fn ref_exists(folder: &str, name: &str) -> bool {
    matches!(git(folder, &["rev-parse", "--verify", "--quiet", &format!("{name}^{{commit}}")]).await, Ok((true, _)))
}
