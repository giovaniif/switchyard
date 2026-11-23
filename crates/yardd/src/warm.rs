//! Warm worktrees. A new worktree starts with the build caches of the
//! checkout it was cut from (`node_modules`, Cargo's `target`, Mix's `_build`
//! and `deps`, `.gradle`), so its first install and build only redo what the
//! branch changed instead of starting cold. Every repository gets this; there
//! is nothing to set up.
//!
//! The copies are copy-on-write clones (reflink on Btrfs and XFS, clonefile on
//! APFS): they share the original's disk blocks until a build rewrites a file,
//! and no two worktrees ever write into the same file, as hard links would. On
//! a filesystem without clones (ext4) nothing is copied: a full copy of every
//! cache per worktree would spend the disk this is here to save.
//!
//! Only gitignored folders with a known cache name are taken, never sources,
//! `.env` files or anything tracked, and a folder the worktree already has is
//! left alone. A cache caught mid-write (a build running in the original
//! checkout) is no worse than a stale one: the build tools check their own
//! fingerprints. A failure here never stops the worktree; it only logs.
//! `YARD_WORKTREE_CACHE=off` turns it off.
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use tokio::process::Command;

/// Cloning is metadata work, but a huge `node_modules` is many files.
const LIMIT: Duration = Duration::from_secs(120);
/// Deep enough for `apps/web/node_modules`; ignored folders are not walked.
const MAX_DEPTH: usize = 4;

fn off() -> bool {
    matches!(std::env::var("YARD_WORKTREE_CACHE").unwrap_or_default().trim(), "off" | "0" | "none")
}

/// Whether the ignored folder at `rel` (inside `top`) is a build cache worth
/// cloning. A plain `target` or `build` folder can be anything, so each name
/// is only taken with the mark its tool leaves.
pub fn is_cache(top: &Path, rel: &str) -> bool {
    let rel = rel.trim_end_matches('/');
    let path = Path::new(rel);
    if path.components().count() > MAX_DEPTH {
        return false;
    }
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else { return false };
    let dir = top.join(rel);
    let parent = dir.parent().unwrap_or(top);
    match name {
        "node_modules" => true,
        // Cargo tags its target folder; a `target` without it is someone's data.
        "target" => dir.join("CACHEDIR.TAG").is_file(),
        "_build" | "deps" => parent.join("mix.exs").is_file(),
        ".gradle" => true,
        _ => false,
    }
}

/// Clone the original checkout's build caches into a new worktree. Returns
/// the folders cloned, relative to the worktree, in the order cloned.
pub async fn clone_caches(top: &Path, worktree: &Path) -> Vec<String> {
    if off() {
        return Vec::new();
    }
    let started = Instant::now();
    match tokio::time::timeout(LIMIT, clone_all(top, worktree)).await {
        Ok(Ok(done)) => {
            if !done.is_empty() {
                eprintln!(
                    "yardd: warm worktree {}: cloned {} in {:.1}s",
                    worktree.display(),
                    done.join(", "),
                    started.elapsed().as_secs_f32()
                );
            }
            done
        }
        Ok(Err(e)) => {
            eprintln!("yardd: warm worktree {}: {e:#}", worktree.display());
            Vec::new()
        }
        Err(_) => {
            eprintln!("yardd: warm worktree {}: gave up after {}s", worktree.display(), LIMIT.as_secs());
            Vec::new()
        }
    }
}

async fn clone_all(top: &Path, worktree: &Path) -> Result<Vec<String>> {
    let candidates = caches(top).await?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    if !can_clone(&top.join(&candidates[0]), worktree).await {
        return Ok(Vec::new());
    }
    let mut done = Vec::new();
    for rel in candidates {
        let src = top.join(&rel);
        let dst = worktree.join(&rel);
        // A tracked parent that the branch deleted, or a folder already there.
        if dst.symlink_metadata().is_ok() || !dst.parent().is_some_and(Path::is_dir) {
            continue;
        }
        if let Err(e) = clone(&src, &dst).await {
            // Half a cache is worse than none: the tool would trust it.
            let _ = tokio::fs::remove_dir_all(&dst).await;
            bail!("could not clone {rel}: {e:#}");
        }
        done.push(rel);
    }
    Ok(done)
}

/// The original checkout's ignored folders that are build caches. Git stops
/// at an ignored folder (`--directory`), so this never walks into one.
async fn caches(top: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(top)
        .args(["ls-files", "-z", "--others", "--ignored", "--exclude-standard", "--directory"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .await
        .context("could not run git ls-files")?;
    if !out.status.success() {
        bail!("git ls-files failed in {}", top.display());
    }
    let mut found: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        // Folders end in `/`; a symlinked `node_modules` (linked by a setup
        // script) has none and is not a cache of this checkout's own.
        .filter_map(|e| e.strip_suffix('/'))
        .filter(|rel| !rel.is_empty() && is_cache(top, rel))
        .filter(|rel| top.join(rel).symlink_metadata().is_ok_and(|m| m.is_dir()))
        .map(str::to_string)
        .collect();
    found.sort();
    Ok(found)
}

/// Whether files in `cache` can be cloned into `worktree` without copying
/// their data. Tried once per worktree with one file from the cache itself:
/// a tiny file git just wrote could be stored inline (Btrfs) and clone
/// differently from the cache's own files.
async fn can_clone(cache: &Path, worktree: &Path) -> bool {
    if !same_device(cache, worktree) {
        return false;
    }
    #[cfg(target_os = "macos")]
    {
        // macOS `cp -c` quietly falls back to a full copy where clonefile is
        // not supported, so ask the filesystem instead.
        return fs_type(worktree).await.as_deref() == Some("apfs");
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Some(source) = first_file(cache) else { return false };
        let probe = worktree.join(".yard-clone-probe");
        // `--reflink=always` fails at once where the filesystem cannot clone.
        let ok = clone(&source, &probe).await.is_ok();
        let _ = tokio::fs::remove_file(&probe).await;
        ok
    }
}

/// A regular file somewhere near the top of `dir`, looking at a bounded
/// number of entries.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn first_file(dir: &Path) -> Option<PathBuf> {
    let mut queue = std::collections::VecDeque::from([dir.to_path_buf()]);
    let mut seen = 0;
    while let Some(d) = queue.pop_front() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > 500 {
                return None;
            }
            match entry.file_type() {
                Ok(t) if t.is_file() => return Some(entry.path()),
                Ok(t) if t.is_dir() => queue.push_back(entry.path()),
                _ => {}
            }
        }
    }
    None
}

#[cfg(unix)]
fn same_device(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_device(_: &Path, _: &Path) -> bool {
    false
}

/// A copy-on-write clone of `src` (a file or a whole folder) at `dst`,
/// keeping modification times: Cargo and Mix compare them.
async fn clone(src: &Path, dst: &Path) -> Result<()> {
    let mut cmd = Command::new("cp");
    #[cfg(target_os = "macos")]
    cmd.args(["-a", "-c"]);
    #[cfg(not(target_os = "macos"))]
    cmd.args(["-a", "--reflink=always", "--"]);
    let out = cmd
        .arg(src)
        .arg(dst)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .context("could not run cp")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().next().unwrap_or("cp failed").trim());
    }
    Ok(())
}

/// The type of the filesystem `path` is on, from `df` and `mount` (macOS
/// prints it as `/dev/disk3s5 on /System/Volumes/Data (apfs, local, …)`).
#[cfg(target_os = "macos")]
async fn fs_type(path: &Path) -> Option<String> {
    let df = Command::new("df").arg("-P").arg(path).output().await.ok()?;
    let mount_point = mount_point(&String::from_utf8_lossy(&df.stdout))?;
    let mounts = Command::new("mount").output().await.ok()?;
    mount_type(&String::from_utf8_lossy(&mounts.stdout), &mount_point)
}

/// The mount point in `df -P` output: the last column of the second line,
/// which may itself contain spaces.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mount_point(df: &str) -> Option<String> {
    let line = df.lines().nth(1)?;
    // Filesystem, 1024-blocks, Used, Available, Capacity, then the mount point.
    let mut rest = line;
    for _ in 0..5 {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        rest = &rest[end..];
    }
    let mp = rest.trim();
    (!mp.is_empty()).then(|| mp.to_string())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn mount_type(mount: &str, mount_point: &str) -> Option<String> {
    let needle = format!(" on {mount_point} (");
    let line = mount.lines().find(|l| l.contains(&needle))?;
    let after = &line[line.find(&needle)? + needle.len()..];
    Some(after.split([',', ')']).next()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_tool_marked_folders_are_caches() {
        let top = std::env::temp_dir().join(format!("yard-warm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&top);
        for d in ["ui/node_modules", "target", "data/target", "app/_build", "lib/_build", ".venv", "a/b/c/d/node_modules"] {
            std::fs::create_dir_all(top.join(d)).unwrap();
        }
        std::fs::write(top.join("target/CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55").unwrap();
        std::fs::write(top.join("app/mix.exs"), "").unwrap();
        assert!(is_cache(&top, "ui/node_modules/"));
        assert!(is_cache(&top, "target/"));
        assert!(!is_cache(&top, "data/target/"), "a target without Cargo's tag");
        assert!(is_cache(&top, "app/_build/"));
        assert!(!is_cache(&top, "lib/_build/"), "a _build without mix.exs beside it");
        assert!(!is_cache(&top, ".venv/"), "virtualenvs hold absolute paths");
        assert!(!is_cache(&top, "a/b/c/d/node_modules/"), "deeper than {MAX_DEPTH}");
        let _ = std::fs::remove_dir_all(&top);
    }

    fn sh(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new(args[0]).args(&args[1..]).current_dir(dir).output().unwrap().status.success();
        assert!(ok, "{args:?}");
    }

    /// Clones where the filesystem can (run with TMPDIR on XFS or Btrfs to see
    /// it), and copies nothing where it cannot (ext4, as on CI).
    #[tokio::test]
    async fn a_new_worktree_gets_the_checkouts_caches_only_as_clones() {
        let root = std::env::temp_dir().join(format!("yard-warm-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let top = root.join("repo");
        std::fs::create_dir_all(top.join("ui/node_modules/pkg")).unwrap();
        std::fs::create_dir_all(top.join("target/debug")).unwrap();
        std::fs::create_dir_all(top.join("logs")).unwrap();
        std::fs::write(top.join(".gitignore"), "node_modules/\ntarget/\nlogs/\n.env\n").unwrap();
        std::fs::write(top.join("ui/index.ts"), "").unwrap();
        std::fs::write(top.join("ui/node_modules/pkg/index.js"), "x".repeat(70_000)).unwrap();
        std::fs::write(top.join("target/CACHEDIR.TAG"), "Signature: 8a477f597d28d172789f06886806bc55").unwrap();
        std::fs::write(top.join("target/debug/app"), "bin").unwrap();
        std::fs::write(top.join("logs/today.log"), "not a cache").unwrap();
        std::fs::write(top.join(".env"), "SECRET=1").unwrap();
        sh(&top, &["git", "init", "-q"]);
        sh(&top, &["git", "add", "."]);
        sh(&top, &["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "init"]);
        let wt = root.join("wt");
        sh(&top, &["git", "worktree", "add", "-q", "-b", "b", wt.to_str().unwrap()]);
        // A cache the worktree already has stays its own.
        std::fs::create_dir_all(wt.join("target")).unwrap();

        let probe = std::process::Command::new("cp")
            .args(["--reflink=always", "--"])
            .arg(top.join("ui/node_modules/pkg/index.js"))
            .arg(root.join("probe"))
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());

        let done = clone_caches(&top, &wt).await;
        if probe && cfg!(not(target_os = "macos")) {
            assert_eq!(done, vec!["ui/node_modules".to_string()]);
            assert_eq!(std::fs::read_to_string(wt.join("ui/node_modules/pkg/index.js")).unwrap().len(), 70_000);
            let mtime = |p: PathBuf| std::fs::metadata(p).unwrap().modified().unwrap();
            assert_eq!(mtime(top.join("ui/node_modules/pkg/index.js")), mtime(wt.join("ui/node_modules/pkg/index.js")));
            assert!(!wt.join("target/debug").exists(), "an existing folder is left alone");
        } else {
            assert!(done.is_empty(), "{done:?}");
            assert!(!wt.join("ui/node_modules").exists(), "no full copies where clones are impossible");
        }
        assert!(!wt.join("logs").exists() && !wt.join(".env").exists());
        assert!(!wt.join(".yard-clone-probe").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn mount_type_is_read_from_df_and_mount() {
        let df = "Filesystem   1024-blocks      Used Available Capacity  Mounted on\n\
                  /dev/disk3s5   971350180 600000000 300000000    67%    /System/Volumes/Data\n";
        assert_eq!(mount_point(df).as_deref(), Some("/System/Volumes/Data"));
        let spaced = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk5s1 100 50 50 50% /Volumes/My Disk\n";
        assert_eq!(mount_point(spaced).as_deref(), Some("/Volumes/My Disk"));
        let mount = "/dev/disk3s1s1 on / (apfs, sealed, local, read-only, journaled)\n\
                     /dev/disk3s5 on /System/Volumes/Data (apfs, local, journaled, nobrowse, protect)\n\
                     /dev/disk5s1 on /Volumes/My Disk (exfat, local, nodev, nosuid, noowners)\n";
        assert_eq!(mount_type(mount, "/System/Volumes/Data").as_deref(), Some("apfs"));
        assert_eq!(mount_type(mount, "/Volumes/My Disk").as_deref(), Some("exfat"));
        assert_eq!(mount_type(mount, "/nowhere"), None);
    }
}
