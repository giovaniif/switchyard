//! yardd: owns agent processes, the per-session event log and host stats.
//! Serves the UI and a WebSocket on 127.0.0.1 by default.

mod accounts;
mod agent_tools;
mod agents;
mod acp;
mod attachments;
mod automations;
mod claude;
mod codex;
mod engines;
mod files;
mod gemini;
mod git;
mod grok;
mod handoff;
mod guard;
mod hosts;
mod import;
mod login;
mod mcp;
mod mobile;
mod naming;
mod origins;
mod opencode;
mod pr;
mod private;
mod pty;
mod remote;
mod sessions;
mod slack;
mod state;
mod store;
mod sweep;
mod widgets;
mod ws;

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use axum::{
    extract::Request,
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use tower_http::services::ServeDir;

use state::App;

/// Keep in step with `PORT` in app/src-tauri/src/main.rs.
/// Not in 4510-4559: LocalStack publishes that whole range on 0.0.0.0, and a
/// loopback bind next to it succeeds silently.
const DEFAULT_PORT: u16 = 47717;

/// Lets the shell tell yardd apart from anything else on the port, and an
/// older yardd from the one it bundles: `yardd <version> <build id>`.
async fn health() -> &'static str {
    concat!("yardd ", env!("CARGO_PKG_VERSION"), " ", env!("YARD_BUILD_ID"))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // An agent's engine starts this binary as its Switchyard tools' MCP server.
    if std::env::args().nth(1).as_deref() == Some("agent-tools") {
        return agent_tools::serve_stdio().await;
    }
    let port: u16 = std::env::var("YARD_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);
    // Loopback by default: the desktop app and SSH hosts reach it locally or
    // through an SSH forward. The container image sets YARD_BIND=0.0.0.0, so
    // its published port or reverse proxy can reach it. A list lets a yardd on
    // a server keep loopback for SSH hosts and add the one address a reverse
    // proxy can reach, such as a Docker network's gateway.
    let binds = parse_binds(&std::env::var("YARD_BIND").unwrap_or_default())?;
    let root: PathBuf = match std::env::var("YARD_ROOT") {
        Ok(p) => PathBuf::from(p),
        Err(_) => std::env::current_dir()?,
    };

    let (store, loaded) = store::open(&store::data_dir())?;
    let app = Arc::new(App::new(root.clone(), port, store));
    // Agents' tools call back over loopback only, so they need yardd there.
    let loopback = binds.iter().find_map(|b| match b {
        b if b.is_loopback() => Some(*b),
        std::net::IpAddr::V4(v4) if v4.is_unspecified() => Some(std::net::Ipv4Addr::LOCALHOST.into()),
        std::net::IpAddr::V6(v6) if v6.is_unspecified() => Some(std::net::Ipv6Addr::LOCALHOST.into()),
        _ => None,
    });
    app.tools.set_url(loopback.map(|ip| format!("http://{}/agent-tools", std::net::SocketAddr::new(ip, port))));
    app.restore(loaded);
    agents::retire(&app).await;
    remote::start(&app);

    // On a normal stop, let the writer finish what is queued before exiting.
    let stopping = app.clone();
    tokio::spawn(async move {
        use tokio::signal::unix::{signal, SignalKind};
        let Ok(mut term) = signal(SignalKind::terminate()) else { return };
        tokio::select! {
            _ = term.recv() => {}
            _ = tokio::signal::ctrl_c() => {}
        }
        stopping.remotes.shutdown();
        stopping.store.flush(Duration::from_secs(2));
        std::process::exit(0);
    });

    tokio::spawn(hosts::run(app.clone()));
    tokio::spawn(automations::run_scheduler(app.clone()));
    tokio::spawn(claude::run_usage_refresh(app.clone()));
    tokio::spawn(sweep::run(app.clone()));
    tokio::spawn(slack::run(app.clone()));

    // Queued messages: into the running turn at a step, or the next turn.
    // In order per session (tokio's Mutex queues waiters first come, first
    // served), without one slow engine call holding up other sessions.
    let mut next_turns = app.take_next_turns();
    let runner = app.clone();
    tokio::spawn(async move {
        let mut locks: std::collections::HashMap<String, Arc<tokio::sync::Mutex<()>>> = Default::default();
        while let Some((sid, when)) = next_turns.recv().await {
            let lock = locks.entry(sid.clone()).or_default().clone();
            let app = runner.clone();
            tokio::spawn(async move {
                let _turn = lock.lock().await;
                sessions::deliver(&app, &sid, when).await;
            });
        }
    });

    tokio::spawn(naming::run(app.clone(), app.take_naming()));
    tokio::spawn(agents::run_turn_ends(app.clone(), app.agents.take_turn_ends()));
    tokio::spawn(origins::run(app.clone(), app.take_replies()));

    // Start Codex in the background so the model list is ready early.
    let boot = app.clone();
    tokio::spawn(async move {
        if let Err(e) = codex::load_models(&boot, accounts::DEFAULT).await {
            eprintln!("yardd: codex unavailable: {e:#}");
        }
    });
    // OpenCode's list too, where it is installed; elsewhere it stays out of the pickers.
    let boot = app.clone();
    tokio::spawn(async move {
        if let Err(e) = opencode::load_models(&boot).await {
            eprintln!("yardd: opencode unavailable: {e:#}");
        }
    });
    // Gemini lists its models when a prompt-free session opens.
    let boot = app.clone();
    tokio::spawn(async move {
        let _ = gemini::status(&boot, crate::accounts::DEFAULT).await;
    });
    // Grok lists its models when its agent starts, logged in or not.
    let boot = app.clone();
    tokio::spawn(async move {
        if let Err(e) = grok::load_models(&boot).await {
            eprintln!("yardd: grok unavailable: {e:#}");
        }
    });

    let router = Router::new()
        .route("/ws", get(ws::handler))
        .route("/health", get(health))
        .route("/agent-tools", post(agent_tools::http))
        .route("/slack/oauth", get(slack::oauth))
        .route("/slack/events", get(slack::events_probe).post(slack::events))
        .fallback_service(ServeDir::new(root.join("ui/dist")))
        .layer(middleware::from_fn(cache_headers))
        .with_state(app);

    if let Some(bind) = binds.iter().find(|b| !b.is_loopback()) {
        if ws::allowed_origins().is_empty() {
            eprintln!(
                "yardd: WARNING: listening on {bind} with no YARD_ALLOW_ORIGIN. Browsers on any address other than \
                 http://127.0.0.1:{port} will be refused. Set YARD_ALLOW_ORIGIN to the URL you open, such as https://yard.example.com"
            );
        }
    }
    // Bind every address before serving any, so a bad one fails the start
    // instead of leaving a yardd that answers on some addresses only.
    let mut listeners = Vec::new();
    for bind in &binds {
        let addr = std::net::SocketAddr::new(*bind, port);
        listeners.push(tokio::net::TcpListener::bind(addr).await.with_context(|| format!("cannot listen on {addr}"))?);
        eprintln!("yardd: http://{addr}");
    }
    let mut servers = tokio::task::JoinSet::new();
    for listener in listeners {
        let router = router.clone();
        // The peer address lets /agent-tools answer local processes only.
        servers.spawn(async move { axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>()).await });
    }
    // A server only returns on error; one that stops takes yardd down with it.
    if let Some(done) = servers.join_next().await {
        done??;
    }
    Ok(())
}

/// `YARD_BIND`: one IP address or a comma-separated list. Empty is loopback.
fn parse_binds(list: &str) -> anyhow::Result<Vec<std::net::IpAddr>> {
    let mut binds = Vec::new();
    for b in list.split(',').map(str::trim).filter(|b| !b.is_empty()) {
        let ip: std::net::IpAddr = b.parse().with_context(|| format!("YARD_BIND `{b}` is not an IP address"))?;
        if !binds.contains(&ip) {
            binds.push(ip);
        }
    }
    if binds.is_empty() {
        binds.push(std::net::Ipv4Addr::LOCALHOST.into());
    }
    Ok(binds)
}

/// The app's web view (WKWebView) caches a page sent without Cache-Control by
/// a guess based on its age, so after an update it kept showing the old
/// index.html and its old bundle. The page must be re-checked on every load;
/// Vite's hashed /assets/ files never change under one name.
fn cache_control(path: &str, status: StatusCode) -> &'static str {
    if path.starts_with("/assets/") && status.is_success() {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

async fn cache_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    let value = cache_control(&path, res.status());
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    res
}

#[cfg(test)]
mod tests {
    use super::{cache_control, parse_binds};
    use axum::http::StatusCode;

    #[test]
    fn the_page_is_rechecked_and_hashed_assets_are_kept() {
        assert_eq!(cache_control("/", StatusCode::OK), "no-cache");
        assert_eq!(cache_control("/index.html", StatusCode::OK), "no-cache");
        assert_eq!(cache_control("/favicon.svg", StatusCode::OK), "no-cache");
        assert_eq!(cache_control("/assets/index-3rHqg71e.js", StatusCode::OK), "public, max-age=31536000, immutable");
        // A missing asset must not be remembered as missing.
        assert_eq!(cache_control("/assets/gone.js", StatusCode::NOT_FOUND), "no-cache");
    }

    #[test]
    fn bind_list() {
        assert_eq!(parse_binds("").unwrap(), vec!["127.0.0.1".parse::<std::net::IpAddr>().unwrap()]);
        assert_eq!(parse_binds(" 0.0.0.0 ").unwrap(), vec!["0.0.0.0".parse::<std::net::IpAddr>().unwrap()]);
        let two = parse_binds("127.0.0.1, 10.0.1.1,127.0.0.1").unwrap();
        assert_eq!(two, ["127.0.0.1", "10.0.1.1"].map(|a| a.parse::<std::net::IpAddr>().unwrap()));
        assert!(parse_binds("127.0.0.1,yard.example.com").is_err());
    }
}
