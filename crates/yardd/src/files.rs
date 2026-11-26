//! `file.get`: the file behind a link an agent wrote, for the page to download.
//!
//! Agents link the files they make in every dialect: an absolute path, one
//! relative to the session's folder, `~/…`, `file://…`, ChatGPT's
//! `sandbox:/mnt/data/…`, often with a line number (`a.ts:42`, `a.ts#L42`).
//! All of them resolve here, on the host the session runs on.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use base64::Engine as _;
use serde_json::{json, Value};

use crate::attachments::{human_size, MAX_BYTES};
use crate::state::App;

pub fn get(app: &App, sid: &str, raw: &str) -> Result<Value> {
    let meta = app.meta(sid).context("no such session")?;
    let home = std::env::var("HOME").unwrap_or_default();
    let path = resolve(raw, Path::new(&meta.cwd), Path::new(&home))?;
    let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    // The reply crosses the remote-host link as one frame, like an upload.
    if size > MAX_BYTES as u64 {
        bail!("{} is {}; files over {} can't be downloaded here yet", path.display(), human_size(size), human_size(MAX_BYTES as u64));
    }
    let bytes = fs::read(&path).with_context(|| format!("could not read {}", path.display()))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "file".into());
    Ok(json!({
        "op": "file.data",
        "sessionId": sid,
        "path": path.to_string_lossy(),
        "name": name,
        "data": base64::engine::general_purpose::STANDARD.encode(bytes),
    }))
}

/// The first existing file a link destination can mean.
pub fn resolve(raw: &str, cwd: &Path, home: &Path) -> Result<PathBuf> {
    let raw = raw.trim().trim_start_matches('<').trim_end_matches('>');
    if raw.is_empty() {
        bail!("the link has no path");
    }
    let (body, sandbox) = if let Some(rest) = strip_prefix_ci(raw, "file://") {
        // `file://host/path` names a host we can't reach; only local ones.
        let rest = rest.strip_prefix("localhost").unwrap_or(rest);
        if !rest.starts_with('/') {
            bail!("{raw} is not a local file");
        }
        (rest, false)
    } else if let Some(rest) = strip_prefix_ci(raw, "sandbox:") {
        (rest, true)
    } else {
        (raw, false)
    };

    let mut tried: Vec<PathBuf> = Vec::new();
    let decoded = percent_decode(body);
    for text in [Some(body.to_string()), decoded].into_iter().flatten() {
        for variant in without_position(&text) {
            let path = absolute(&variant, cwd, home);
            if !tried.contains(&path) {
                tried.push(path);
            }
        }
    }
    // ChatGPT's sandbox folder doesn't exist here; the agent meant the file it
    // wrote, which lives in the session's folder.
    if sandbox {
        if let Some(name) = Path::new(body).file_name() {
            tried.push(cwd.join(name));
        }
    }

    let mut folder = None;
    for path in &tried {
        match fs::metadata(path) {
            Ok(m) if m.is_file() => return Ok(path.clone()),
            Ok(m) if m.is_dir() => folder = folder.or(Some(path.clone())),
            _ => {}
        }
    }
    match folder {
        Some(path) => bail!("{} is a folder, not a file", path.display()),
        None => bail!("{} was not found", tried.first().map(|p| p.display().to_string()).unwrap_or_else(|| raw.to_string())),
    }
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    s.get(..prefix.len()).filter(|head| head.eq_ignore_ascii_case(prefix)).map(|_| &s[prefix.len()..])
}

fn absolute(path: &str, cwd: &Path, home: &Path) -> PathBuf {
    if path == "~" {
        return home.to_path_buf();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest);
    }
    let p = Path::new(path);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}

/// The path as written, then without a trailing position: `#L4`, `#L4-L9`,
/// `#L4C2`, `:4`, `:4:2`, `:4-9`. A file may really end in `:4`, so the full
/// text comes first.
fn without_position(path: &str) -> Vec<String> {
    let mut out = vec![path.to_string()];
    let mut cut = path;
    if let Some(i) = cut.rfind('#') {
        if is_anchor(&cut[i + 1..]) {
            cut = &cut[..i];
        }
    }
    for _ in 0..2 {
        match cut.rfind(':') {
            Some(i) if i > 0 && is_line_range(&cut[i + 1..]) => cut = &cut[..i],
            _ => break,
        }
    }
    if cut != path && !cut.is_empty() {
        out.push(cut.to_string());
    }
    out
}

fn is_anchor(s: &str) -> bool {
    let s = s.strip_prefix('L').unwrap_or(s);
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || matches!(c, '-' | 'L' | 'C'))
}

fn is_line_range(s: &str) -> bool {
    !s.is_empty() && s.chars().next().is_some_and(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_digit() || c == '-')
}

/// `%20` and friends, when the text has any; `None` when nothing changes or
/// the escapes are not valid UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    if !s.contains('%') {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).ok().filter(|d| d != s)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        fn new(name: &str) -> Tree {
            let root = std::env::temp_dir().join(format!("yardd-files-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(root.join("work/src")).unwrap();
            fs::create_dir_all(root.join("home/notes")).unwrap();
            fs::write(root.join("work/report.csv"), "a,b\n").unwrap();
            fs::write(root.join("work/src/main.ts"), "x").unwrap();
            fs::write(root.join("work/my report.csv"), "a").unwrap();
            fs::write(root.join("home/notes/todo.md"), "t").unwrap();
            Tree { root }
        }
        fn cwd(&self) -> PathBuf {
            self.root.join("work")
        }
        fn home(&self) -> PathBuf {
            self.root.join("home")
        }
        fn get(&self, raw: &str) -> Result<PathBuf> {
            resolve(raw, &self.cwd(), &self.home())
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn every_dialect_finds_the_file() {
        let t = Tree::new("dialects");
        let report = t.cwd().join("report.csv");
        let abs = report.to_string_lossy().to_string();
        for raw in [
            abs.clone(),
            "report.csv".into(),
            "./report.csv".into(),
            format!("<{abs}>"),
            format!("file://{abs}"),
            format!("file://localhost{abs}"),
            "sandbox:/mnt/data/report.csv".into(),
            format!("sandbox:{abs}"),
            format!("{abs}:12"),
            "report.csv:12:4".into(),
            "report.csv#L12".into(),
            "report.csv#L12-L20".into(),
        ] {
            assert_eq!(t.get(&raw).unwrap(), report, "{raw}");
        }
        assert_eq!(t.get("src/main.ts:42").unwrap(), t.cwd().join("src/main.ts"));
        assert_eq!(t.get("~/notes/todo.md").unwrap(), t.home().join("notes/todo.md"));
        assert_eq!(t.get("my%20report.csv").unwrap(), t.cwd().join("my report.csv"));
        assert_eq!(t.get("my report.csv").unwrap(), t.cwd().join("my report.csv"));
    }

    #[test]
    fn missing_files_folders_and_remote_urls_are_refused() {
        let t = Tree::new("refused");
        let missing = t.get("nope.csv").unwrap_err().to_string();
        assert!(missing.contains("nope.csv was not found"), "{missing}");
        let folder = t.get("src").unwrap_err().to_string();
        assert!(folder.contains("is a folder"), "{folder}");
        assert!(t.get("file://example.com/etc/passwd").is_err());
        assert!(t.get("   ").is_err());
    }

    #[test]
    fn positions_are_cut_only_from_the_end() {
        assert_eq!(without_position("a.ts"), vec!["a.ts"]);
        assert_eq!(without_position("a.ts:4"), vec!["a.ts:4", "a.ts"]);
        assert_eq!(without_position("a.ts:4:2"), vec!["a.ts:4:2", "a.ts"]);
        assert_eq!(without_position("a.ts#L4-L9"), vec!["a.ts#L4-L9", "a.ts"]);
        assert_eq!(without_position("notes#intro"), vec!["notes#intro"]);
        assert_eq!(without_position("C:12"), vec!["C:12", "C"]);
    }
}
