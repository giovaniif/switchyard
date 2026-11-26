//! Switchyard shell: a window on yardd's UI.
//!
//! The shell holds no agent state. It starts yardd if nothing is listening,
//! then loads yardd's page. yardd is not a child that dies with the window:
//! sessions keep running after the app is closed.
//!
//! The .app carries a release yardd, the compiled Claude sidecar and the UI
//! under `Contents/Resources/yard` (see app/build.sh). Run from the repo with
//! `cargo run` and those are absent, so it falls back to the repo's builds.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    webview::{DownloadEvent, NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse},
    AppHandle, Listener, Manager, Url, WebviewUrl, WebviewWindowBuilder, Wry,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

/// Keep in step with `DEFAULT_PORT` in crates/yardd/src/main.rs.
const DEFAULT_PORT: u16 = 47717;

/// The build id of the yardd bundled with this shell; app/build.sh stamps
/// both with the same one.
const BUILD_ID: &str = env!("YARD_BUILD_ID");

const OLDER_DAEMON: &str = "an older daemon is running, quit it to update";

/// How often a running app looks for a new release, after the check at launch.
const UPDATE_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

const CHECK_UPDATES: &str = "check-updates";
const OPEN_SETTINGS: &str = "open-settings";
const IMPORT_SESSIONS: &str = "import-sessions";
const OPEN_NAVIGATION: &str = "open-navigation";

/// One update check at a time: the timer's and the menu's would otherwise
/// download the same release twice.
static CHECKING: AtomicBool = AtomicBool::new(false);

/// The release found by the last check, until the restart into it.
static FOUND: Mutex<Option<Found>> = Mutex::new(None);

/// The page's "What's new" and "Restart" (or "Update") on its update notice.
const UPDATE_ACTION: &str = "switchyard:update-action";

/// Notes longer than this end with a pointer to the release page: the
/// dialog grows with its text.
const NOTES_MAX: usize = 1800;

#[derive(Clone)]
struct Found {
    version: String,
    notes: String,
    /// None once it is installed over this copy: a restart runs it.
    update: Option<Update>,
}

/// "Check for Updates…", which says what a check found.
struct UpdateMenuItem(MenuItem<Wry>);

/// `SWITCHYARD_PORT` lets a second copy run next to the real one.
fn port() -> u16 {
    std::env::var("SWITCHYARD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// What to start: the yardd binary, the folder it serves `ui/dist` from, and
/// the compiled sidecar (None: yardd runs the TypeScript one with Bun).
struct Daemon {
    binary: PathBuf,
    root: PathBuf,
    sidecar: Option<PathBuf>,
}

fn bundled(resources: &Path) -> Option<Daemon> {
    let root = resources.join("yard");
    let binary = root.join("bin/yardd");
    let sidecar = root.join("bin/yard-claude");
    let complete = binary.is_file() && sidecar.is_file() && root.join("ui/dist/index.html").is_file();
    complete.then_some(Daemon { binary, root, sidecar: Some(sidecar) })
}

fn from_repo() -> Result<Daemon, String> {
    // app/src-tauri -> repo root
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = ["target/release/yardd", "target/debug/yardd"]
        .iter()
        .map(|p| root.join(p))
        .find(|p| p.exists())
        .ok_or_else(|| format!("yardd is not built. Run `cargo build -p yardd` in {}", root.display()))?;
    Ok(Daemon { binary, root, sidecar: None })
}

/// yardd's `/health` body, or None if nothing or something else answers.
/// A bare TCP connect is not enough: another program can hold the port
/// (LocalStack did, on 4517).
fn health(port: u16) -> Option<String> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let _ = stream.set_write_timeout(Some(Duration::from_millis(800)));
    let request = format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    if !response.starts_with("HTTP/1.1 200") {
        return None;
    }
    let body = response.split_once("\r\n\r\n")?.1.trim();
    body.starts_with("yardd ").then(|| body.to_string())
}

/// `yardd <version> <build id>`. A yardd from before build ids has none.
fn build_of(health: &str) -> Option<&str> {
    health.split_whitespace().nth(2)
}

/// Apps opened from Finder get a bare PATH with no `codex` or `claude`.
/// Ask the user's login shell for theirs.
fn login_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    let out = Command::new(shell)
        .args(["-lc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (out.status.success() && !path.is_empty()).then_some(path)
}

fn start_yardd(daemon: &Daemon, port: u16) -> Result<(), String> {
    // A test copy with its own data folder logs there, not in ~/.yard.
    let data_dir = std::env::var("YARD_DATA_DIR").ok().filter(|d| !d.is_empty());
    let log_dir = match &data_dir {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(std::env::var("HOME").map_err(|_| "HOME is not set".to_string())?).join(".yard"),
    };
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("yardd.log"))
        .map_err(|e| e.to_string())?;
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&daemon.binary);
    cmd.current_dir(&daemon.root)
        .env("YARD_ROOT", &daemon.root)
        .env("YARD_PORT", port.to_string())
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_err);
    match &daemon.sidecar {
        Some(sidecar) => cmd.env("YARD_SIDECAR", sidecar),
        None => cmd.env_remove("YARD_SIDECAR"),
    };
    if let Some(dir) = &data_dir {
        cmd.env("YARD_DATA_DIR", dir);
    }
    if let Some(path) = login_path() {
        cmd.env("PATH", path);
    }
    // Do not hand a parent Claude Code session's markers down to agents.
    for (key, _) in std::env::vars() {
        if key == "CLAUDECODE" || key.starts_with("CLAUDE_CODE_") || key.starts_with("CLAUDE_AGENT_SDK") {
            cmd.env_remove(key);
        }
    }
    // Spawned and not waited on: yardd outlives this process on purpose.
    cmd.spawn().map_err(|e| format!("could not start yardd: {e}"))?;

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if health(port).is_some() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err(format!("yardd did not start listening within 10 seconds. See {}", log_dir.join("yardd.log").display()))
}

/// Stops the yardd listening on `port` and waits for the port to free up.
/// yardd flushes its log on SIGTERM; sessions come back when it starts again.
fn stop_yardd(port: u16) -> Result<(), String> {
    let out = Command::new("/usr/sbin/lsof")
        .args(["-nP", "-t", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .map_err(|e| format!("lsof: {e}"))?;
    let pids: Vec<String> = String::from_utf8_lossy(&out.stdout).split_whitespace().map(str::to_string).collect();
    if pids.is_empty() {
        return Ok(());
    }
    Command::new("/bin/kill").args(&pids).status().map_err(|e| format!("kill: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if health(port).is_none() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(150));
    }
    Err(format!("yardd on port {port} did not stop"))
}

/// Restarts the app, and the background yardd with it, so both run the
/// installed version: sessions come back, a running turn stops.
fn restart_now(app: &AppHandle, port: u16) {
    if let Err(e) = stop_yardd(port) {
        eprintln!("switchyard: {e}");
    }
    app.restart();
}

/// Asked first: a running turn stops.
fn ask_restart(app: &AppHandle, port: u16, message: String) {
    let handle = app.clone();
    app.dialog()
        .message(message)
        .title("Switchyard")
        .buttons(MessageDialogButtons::OkCancelCustom("Restart now".into(), "Later".into()))
        .show(move |restart| {
            if restart {
                restart_now(&handle, port);
            }
        });
}

fn tell(app: &AppHandle, kind: MessageDialogKind, message: String) {
    app.dialog().message(message).title("Switchyard").kind(kind).show(|_| {});
}

fn found() -> Option<Found> {
    FOUND.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn set_found(app: &AppHandle, found: Option<Found>) {
    *FOUND.lock().unwrap_or_else(|e| e.into_inner()) = found;
    show_update(app);
}

/// Says what a check found where the user looks, without a dialog or
/// taking focus: the app menu's item, and a notice on the page.
fn show_update(app: &AppHandle) {
    let found = found();
    if let Some(item) = app.try_state::<UpdateMenuItem>() {
        let text = match &found {
            None => "Check for Updates…".to_string(),
            Some(f) if f.update.is_none() => format!("Restart to Update to {}…", f.version),
            Some(f) => format!("Update to {}…", f.version),
        };
        let _ = item.0.set_text(text);
        let _ = item.0.set_enabled(true);
    }
    if let Some(window) = app.get_webview_window("main") {
        tell_page(&window, found.as_ref());
    }
}

/// The page reads `window.__SWITCHYARD_UPDATE__` when it starts and hears
/// `switchyard:update` after. Only this window sets them, so a browser on
/// the same yardd never shows the notice.
fn tell_page(window: &tauri::WebviewWindow, found: Option<&Found>) {
    let state = match found {
        // The version comes from latest.json: JSON-encoded, never spliced in raw.
        Some(f) => serde_json::json!({ "version": f.version, "ready": f.update.is_none() }),
        None => serde_json::Value::Null,
    };
    let script = format!(
        "window.__SWITCHYARD_UPDATE__ = {state}; \
         window.dispatchEvent(new CustomEvent('switchyard:update', {{ detail: window.__SWITCHYARD_UPDATE__ }}))"
    );
    if let Err(e) = window.eval(&script) {
        eprintln!("switchyard: could not tell the page about the update: {e}");
    }
}

/// A CHANGELOG section as plain text for a native dialog.
fn plain_notes(markdown: &str) -> String {
    let mut out = String::new();
    for line in markdown.lines() {
        let indent = line.len() - line.trim_start().len();
        let mut text = line.trim().to_string();
        if let Some(rest) = text.strip_prefix("- ").or_else(|| text.strip_prefix("* ")) {
            text = format!("{}• {rest}", " ".repeat(indent));
        } else if text.starts_with('#') {
            text = text.trim_start_matches('#').trim().to_string();
        }
        text = text.replace("**", "").replace('`', "");
        // [label](url) -> label
        let mut from = 0;
        while let Some(open) = text[from..].find('[').map(|i| from + i) {
            let Some(end) = text[open..].find(']').map(|i| open + i) else { break };
            if !text[end..].starts_with("](") {
                from = open + 1;
                continue;
            }
            let Some(close) = text[end..].find(')').map(|i| end + i) else { break };
            text = format!("{}{}{}", &text[..open], &text[open + 1..end], &text[close + 1..]);
            from = end - 1;
        }
        // One blank line between paragraphs at most, none at the start.
        if text.is_empty() && (out.is_empty() || out.ends_with("\n\n")) {
            continue;
        }
        out.push_str(&text);
        out.push('\n');
    }
    let out = out.trim().to_string();
    if out.chars().count() <= NOTES_MAX {
        return out;
    }
    let cut: String = out.chars().take(NOTES_MAX).collect();
    let cut = cut.rsplit_once('\n').map_or(cut.as_str(), |(head, _)| head);
    format!("{cut}\n…and more in the release notes on GitHub.")
}

/// The one window an update shows: its notes, then Install and Restart or
/// Later. A release that is not downloaded yet is installed first.
fn show_notes(app: &AppHandle, port: u16, found: Found) {
    let current = app.package_info().version.to_string();
    let notes = match plain_notes(&found.notes) {
        n if n.is_empty() => "No release notes.".to_string(),
        n => n,
    };
    let message = format!(
        "You have {current}.\n\n{notes}\n\n\
         Installing restarts Switchyard. Sessions and their history are kept; a turn that is running now stops."
    );
    let handle = app.clone();
    app.dialog()
        .message(message)
        .title(format!("Switchyard {} is available", found.version))
        .buttons(MessageDialogButtons::OkCancelCustom("Install and Restart".into(), "Later".into()))
        .show(move |install| {
            if !install {
                return;
            }
            tauri::async_runtime::spawn(async move {
                match install_found(&handle, found).await {
                    Ok(()) => restart_now(&handle, port),
                    Err(e) => tell(&handle, MessageDialogKind::Error, format!("Could not install the update.\n\n{e}")),
                }
            });
        });
}

/// Waits for a running check: a background download of the same release
/// must finish, not race a second one.
async fn take_check() {
    while CHECKING.swap(true, Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Downloads and installs `found`, unless that already happened.
async fn install_found(app: &AppHandle, found: Found) -> Result<(), String> {
    let Some(update) = found.update.clone() else { return Ok(()) };
    take_check().await;
    // The background check may have installed it while this one waited.
    let result = if found_is_installed(&found.version) {
        Ok(())
    } else {
        if let Some(item) = app.try_state::<UpdateMenuItem>() {
            let _ = item.0.set_text(format!("Installing {}…", found.version));
            let _ = item.0.set_enabled(false);
        }
        eprintln!("switchyard: installing {}", found.version);
        update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())
    };
    CHECKING.store(false, Ordering::SeqCst);
    match &result {
        Ok(()) if !found_is_installed(&found.version) => set_found(app, Some(Found { update: None, ..found })),
        _ => show_update(app),
    }
    result
}

fn found_is_installed(version: &str) -> bool {
    found().is_some_and(|f| f.update.is_none() && f.version == version)
}

fn found_from(update: Update) -> Found {
    Found { version: update.version.clone(), notes: update.body.clone().unwrap_or_default(), update: Some(update) }
}

/// The timer's check: a newer release is downloaded and installed in place
/// with no dialog; the menu item and the page's notice then offer the
/// restart. Only for the packaged app: a build run from the repo has
/// nothing to replace.
async fn check_in_background(app: &AppHandle) -> Result<(), String> {
    if CHECKING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let result = async {
        let Some(update) = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())? else {
            return Ok(());
        };
        if found_is_installed(&update.version) {
            return Ok(());
        }
        let found = found_from(update.clone());
        // If the download fails, the menu and the notice still offer it.
        set_found(app, Some(found.clone()));
        eprintln!("switchyard: installing {}", found.version);
        update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())?;
        set_found(app, Some(Found { update: None, ..found }));
        Ok(())
    }
    .await;
    CHECKING.store(false, Ordering::SeqCst);
    result
}

/// The menu item: exactly one answer. A release already found gets its
/// notes window; otherwise a check, after any running one, answers "latest
/// version" or with the notes window.
async fn check_when_asked(app: &AppHandle, port: u16) {
    take_check().await;
    let result = match found() {
        Some(found) => Ok(Some(found)),
        None => async {
            let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
            Ok::<_, String>(update.map(found_from))
        }
        .await,
    };
    CHECKING.store(false, Ordering::SeqCst);
    match result {
        Ok(Some(found)) => {
            set_found(app, Some(found.clone()));
            show_notes(app, port, found);
        }
        Ok(None) => {
            let current = app.package_info().version.to_string();
            tell(app, MessageDialogKind::Info, format!("Switchyard {current} is the latest version."));
        }
        Err(e) => tell(app, MessageDialogKind::Error, format!("Could not check for updates.\n\n{e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_read_as_plain_text() {
        let md = "\n- **Comment on parts.** Select `text` and see [docs](https://x.test/a).\n- **Fixed:**\n  - A [b](u) and [c](v) link.\n\n\n### Install\n";
        assert_eq!(
            plain_notes(md),
            "• Comment on parts. Select text and see docs.\n• Fixed:\n  • A b and c link.\n\nInstall"
        );
    }

    #[test]
    fn long_notes_are_cut_at_a_line() {
        let md = (0..200).map(|i| format!("- Line {i} of the notes")).collect::<Vec<_>>().join("\n");
        let plain = plain_notes(&md);
        assert!(plain.chars().count() < NOTES_MAX + 60);
        assert!(plain.ends_with("\n…and more in the release notes on GitHub."));
        assert!(plain.lines().rev().nth(1).unwrap().ends_with("of the notes"));
    }

    #[test]
    fn a_bracket_without_a_link_stays() {
        assert_eq!(plain_notes("- Keys [Cmd] and [x](y)"), "• Keys [Cmd] and x");
    }
}

/// Make sure our yardd is serving `port`. Returns a problem for the title.
fn ensure_yardd(resources: Option<PathBuf>, port: u16) -> Option<String> {
    // SWITCHYARD_BUILD_ID stands in for the bundled id, to test a mismatch
    // without a second build.
    let expected = std::env::var("SWITCHYARD_BUILD_ID").unwrap_or_else(|_| BUILD_ID.to_string());
    match health(port) {
        Some(body) if build_of(&body) == Some(expected.as_str()) => None,
        // It may hold live sessions: never stop it, just say so.
        Some(body) => {
            eprintln!("switchyard: port {port} has `{body}`, this app bundles build {expected}");
            Some(OLDER_DAEMON.to_string())
        }
        None => {
            let daemon = match resources.as_deref().and_then(bundled) {
                Some(daemon) => Ok(daemon),
                None => from_repo(),
            };
            daemon.and_then(|d| start_yardd(&d, port)).err()
        }
    }
}

/// Links the page opens in a new tab (sign-in pages, pull requests,
/// notifications) go to the default browser. WebKit drops them otherwise:
/// the window has no tabs to put them in.
fn open_outside(url: &Url) {
    if !matches!(url.scheme(), "http" | "https") {
        eprintln!("switchyard: not opening {}: only web links leave the app", url.scheme());
        return;
    }
    if let Err(e) = Command::new("/usr/bin/open").arg(url.as_str()).spawn() {
        eprintln!("switchyard: could not open {url}: {e}");
    }
}

/// Keep infrequent navigation in the Mac app menu, beside the standard items.
fn app_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    let check = MenuItem::with_id(app, CHECK_UPDATES, "Check for Updates…", true, None::<&str>)?;
    app.manage(UpdateMenuItem(check.clone()));
    if let Some(first) = menu.items()?.first().and_then(|item| item.as_submenu()) {
        first.insert(&check, 1)?;
        if cfg!(target_os = "macos") {
            let settings = MenuItem::with_id(app, OPEN_SETTINGS, "Settings…", true, Some("Cmd+,"))?;
            let import = MenuItem::with_id(app, IMPORT_SESSIONS, "Import Sessions…", true, None::<&str>)?;
            let navigation = MenuItem::with_id(app, OPEN_NAVIGATION, "Go to…", true, Some("Cmd+K"))?;
            let separator = PredefinedMenuItem::separator(app)?;
            first.insert_items(&[&settings, &import, &navigation, &separator], 3)?;
        }
    }
    Ok(menu)
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(app_menu)
        .on_menu_event(|app, event| {
            let destination = match event.id().0.as_str() {
                OPEN_SETTINGS => Some("settings"),
                IMPORT_SESSIONS => Some("import"),
                OPEN_NAVIGATION => Some("navigation"),
                _ => None,
            };
            if let Some(destination) = destination {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    // Fixed menu destinations, never agent or session text.
                    let destination = serde_json::to_string(destination).expect("static destination");
                    let script = format!("window.dispatchEvent(new CustomEvent('switchyard:navigate', {{ detail: {destination} }}))");
                    if let Err(e) = window.eval(&script) {
                        eprintln!("switchyard: could not open menu destination: {e}");
                    }
                }
                return;
            }
            if event.id() != CHECK_UPDATES {
                return;
            }
            let packaged = app.path().resource_dir().ok().as_deref().and_then(bundled).is_some();
            if !packaged {
                tell(app, MessageDialogKind::Info, "This build runs from the repository. Updates come with the packaged app.".into());
                return;
            }
            let handle = app.clone();
            tauri::async_runtime::spawn(async move { check_when_asked(&handle, port()).await });
        })
        .setup(|app| {
            let port = port();
            let resources = app.path().resource_dir().ok();
            let packaged = resources.as_deref().and_then(bundled).is_some();
            let problem = ensure_yardd(resources, port);
            if let Some(message) = &problem {
                eprintln!("switchyard: {message}");
            }
            let url = format!("http://127.0.0.1:{port}/").parse().expect("static url");
            let title = match &problem {
                Some(message) => format!("Switchyard: {message}"),
                None => "Switchyard".to_string(),
            };
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                // A newer UI must keep its sidebar entry points in older shells.
                .initialization_script(if cfg!(target_os = "macos") {
                    "window.__SWITCHYARD_APP_MENU__ = true;"
                } else {
                    ""
                })
                .title(title)
                // A reload, or yardd restarting, loads a page that has not
                // heard about an update found earlier.
                .on_page_load(|window, payload| {
                    if payload.event() == PageLoadEvent::Finished {
                        tell_page(&window, found().as_ref());
                    }
                })
                .inner_size(1440.0, 900.0)
                .min_inner_size(900.0, 600.0)
                // Tauri's own file-drop handler would swallow drops, and the
                // composer attaches files through the page's HTML5 drop events.
                .disable_drag_drop_handler()
                .on_new_window(|url, _| {
                    open_outside(&url);
                    NewWindowResponse::Deny
                })
                // Dictation and voice mode: WebKit asks the host app before
                // the page opens the microphone. The window shows yardd's
                // page (links leave for the browser above), so the microphone
                // is granted without a WebKit prompt on top of macOS's own.
                // Without a handler wry grants every capture request; the
                // camera, which nothing uses, is now refused.
                // A file an agent linked downloads from yardd's page as a
                // blob. Without a handler wry cancels every download; with
                // one, WebKit saves it to ~/Downloads under a free name, and
                // Finder shows it, since the window has no download list.
                .on_download(|_, event| {
                    if let DownloadEvent::Finished { path: Some(path), success: true, .. } = event {
                        if let Err(e) = Command::new("/usr/bin/open").arg("-R").arg(&path).spawn() {
                            eprintln!("switchyard: could not show {}: {e}", path.display());
                        }
                    }
                    true
                })
                .on_permission_request(|_, kind| match kind {
                    PermissionKind::Microphone => PermissionResponse::Allow,
                    PermissionKind::Camera => PermissionResponse::Deny,
                    _ => PermissionResponse::Default,
                })
                .build()?;

            if packaged {
                // The app was updated while an older yardd kept running (the
                // restart was put off, or this is the first update-aware build).
                if problem.as_deref() == Some(OLDER_DAEMON) {
                    ask_restart(
                        app.handle(),
                        port,
                        "Switchyard was updated, but the background service still runs the old version. \
                         Restart it now?\n\nSessions and their history are kept. A turn that is running now stops."
                            .to_string(),
                    );
                }
                let handle = app.handle().clone();
                app.listen_any(UPDATE_ACTION, move |event| {
                    let action: String = serde_json::from_str(event.payload()).unwrap_or_default();
                    let Some(found) = found() else { return };
                    match action.as_str() {
                        "notes" => show_notes(&handle, port, found),
                        "restart" if found.update.is_none() => ask_restart(
                            &handle,
                            port,
                            format!(
                                "Restart to update to Switchyard {}?\n\n\
                                 Sessions and their history are kept. A turn that is running now stops.",
                                found.version
                            ),
                        ),
                        _ => {}
                    }
                });
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        if let Err(e) = check_in_background(&handle).await {
                            eprintln!("switchyard: update check failed: {e}");
                        }
                        tokio::time::sleep(UPDATE_EVERY).await;
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Switchyard");
}
