//! Connectors Switchyard holds itself (`engine: "switchyard"`), end to end:
//! the sign-in against a fake OAuth-protected MCP server that runs inside
//! the test, the tokens' file, the bridge a scripted ACP session is given,
//! and `yardd mcp-proxy` run the way that session's agent would run it.

mod common;

use std::{
    collections::{HashMap, HashSet},
    os::unix::fs::PermissionsExt,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Form, Json, Router,
};
use base64::Engine as _;
use common::*;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const ENGINE: &str = "switchyard";

/// The fake server's state, which a test reads and steers.
#[derive(Default)]
struct Fake {
    base: String,
    n: u64,
    /// Authorization codes not yet spent: code -> (challenge, redirect_uri, resource).
    codes: HashMap<String, (String, String, String)>,
    access: HashSet<String>,
    refresh: HashSet<String>,
    registrations: Vec<Value>,
    authorizations: Vec<HashMap<String, String>>,
    token_forms: Vec<HashMap<String, String>>,
    refreshes: u64,
    /// `expires_in` of the tokens a code buys; refreshed ones last an hour.
    code_expires_in: u64,
    fail_refresh: bool,
    no_registration: bool,
    /// Every `/mcp` request with a valid token: method, session and version
    /// headers, and whether it was answered as an event stream.
    seen: Vec<(String, String, String, bool)>,
    calls: Vec<Value>,
    posts: u64,
}

type Shared = Arc<Mutex<Fake>>;

fn challenge_of(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()))
}

async fn resource_meta(State(f): State<Shared>) -> Json<Value> {
    let base = f.lock().unwrap().base.clone();
    Json(json!({ "resource": format!("{base}/mcp"), "authorization_servers": [base], "scopes_supported": ["read", "write"] }))
}

async fn server_meta(State(f): State<Shared>) -> Json<Value> {
    let f = f.lock().unwrap();
    let base = &f.base;
    let mut meta = json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/authorize"),
        "token_endpoint": format!("{base}/token"),
        "code_challenge_methods_supported": ["S256"],
    });
    if !f.no_registration {
        meta["registration_endpoint"] = json!(format!("{base}/register"));
    }
    Json(meta)
}

async fn register(State(f): State<Shared>, Json(body): Json<Value>) -> Response {
    let mut f = f.lock().unwrap();
    f.registrations.push(body);
    let id = format!("TEST-client-{}", f.registrations.len());
    (StatusCode::CREATED, Json(json!({ "client_id": id, "token_endpoint_auth_method": "none" }))).into_response()
}

/// The consent page, already agreed to: straight back with a code.
async fn authorize(State(f): State<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    let mut f = f.lock().unwrap();
    f.n += 1;
    let code = format!("TEST-code-{}", f.n);
    let get = |k: &str| q.get(k).cloned().unwrap_or_default();
    f.codes.insert(code.clone(), (get("code_challenge"), get("redirect_uri"), get("resource")));
    let to = format!("{}?code={code}&state={}", get("redirect_uri"), get("state"));
    f.authorizations.push(q);
    (StatusCode::FOUND, [("location", to)]).into_response()
}

async fn token(State(f): State<Shared>, Form(form): Form<HashMap<String, String>>) -> Response {
    let mut f = f.lock().unwrap();
    f.token_forms.push(form.clone());
    let refuse = |why: &str| (StatusCode::BAD_REQUEST, Json(json!({ "error": "invalid_grant", "error_description": why }))).into_response();
    let get = |k: &str| form.get(k).cloned().unwrap_or_default();
    let expires_in = match get("grant_type").as_str() {
        "authorization_code" => {
            let Some((challenge, redirect, resource)) = f.codes.remove(&get("code")) else { return refuse("unknown code") };
            if challenge_of(&get("code_verifier")) != challenge || get("redirect_uri") != redirect || get("resource") != resource {
                return refuse("the verifier, redirect or resource does not match");
            }
            f.code_expires_in
        }
        "refresh_token" => {
            if f.fail_refresh || !f.refresh.remove(&get("refresh_token")) {
                return refuse("the refresh token is not valid");
            }
            f.refreshes += 1;
            3600
        }
        _ => return refuse("unknown grant"),
    };
    // Rotation: the old access tokens stop working.
    f.access.clear();
    f.n += 1;
    let (access, refresh) = (format!("TEST-access-{}", f.n), format!("TEST-refresh-{}", f.n));
    f.access.insert(access.clone());
    f.refresh.insert(refresh.clone());
    Json(json!({ "access_token": access, "token_type": "Bearer", "refresh_token": refresh, "expires_in": expires_in })).into_response()
}

async fn mcp(State(f): State<Shared>, headers: HeaderMap, Json(msg): Json<Value>) -> Response {
    let mut f = f.lock().unwrap();
    let header = |k: &str| headers.get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    let bearer = header("authorization");
    if !bearer.strip_prefix("Bearer ").is_some_and(|t| f.access.contains(t)) {
        let meta = format!("Bearer resource_metadata=\"{}/.well-known/oauth-protected-resource/mcp\"", f.base);
        return (StatusCode::UNAUTHORIZED, [("www-authenticate", meta)]).into_response();
    }
    let method = msg["method"].as_str().unwrap_or("").to_string();
    if msg.get("id").is_none() {
        f.seen.push((method, header("mcp-session-id"), header("mcp-protocol-version"), false));
        return StatusCode::ACCEPTED.into_response();
    }
    let result = match method.as_str() {
        "initialize" => json!({ "protocolVersion": "2025-06-18", "capabilities": { "tools": {} }, "serverInfo": { "name": "fake", "version": "1" } }),
        "tools/list" => json!({ "tools": [
            { "name": "lookup", "description": "Look something up.", "inputSchema": { "type": "object" } },
            { "name": "create", "description": "Create something.", "inputSchema": { "type": "object" } },
        ] }),
        "tools/call" => {
            f.calls.push(msg["params"].clone());
            json!({ "content": [{ "type": "text", "text": format!("fake ran {} with {}", msg["params"]["name"].as_str().unwrap_or(""), msg["params"]["arguments"]) }], "isError": false })
        }
        _ => Value::Null,
    };
    let answer = json!({ "jsonrpc": "2.0", "id": msg["id"], "result": result });
    // One answer as plain JSON, the next as an event stream, and so on.
    f.posts += 1;
    let stream = f.posts % 2 == 0;
    f.seen.push((method.clone(), header("mcp-session-id"), header("mcp-protocol-version"), stream));
    let session = [("mcp-session-id", "TEST-session")];
    if stream {
        let note = json!({ "jsonrpc": "2.0", "method": "notifications/message", "params": { "level": "info", "data": "working" } });
        let body = format!("event: message\ndata: {note}\n\nevent: message\ndata: {answer}\n\n");
        return (session, [("content-type", "text/event-stream")], body).into_response();
    }
    (session, Json(answer)).into_response()
}

async fn fake_server() -> Shared {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let fake: Shared = Arc::new(Mutex::new(Fake { base, code_expires_in: 3600, ..Default::default() }));
    let router = Router::new()
        .route("/.well-known/oauth-protected-resource/mcp", get(resource_meta))
        .route("/.well-known/oauth-authorization-server", get(server_meta))
        .route("/register", post(register))
        .route("/authorize", get(authorize))
        .route("/token", post(token))
        .route("/mcp", post(mcp).delete(|| async { StatusCode::OK }))
        .with_state(fake.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    fake
}

/// A browser's GET: the status, the `Location` it was sent to, and the body.
async fn browse(url: &str) -> (u16, String, String) {
    let rest = url.strip_prefix("http://").unwrap_or_else(|| panic!("not a local address: {url}"));
    let (host, path) = rest.split_once('/').unwrap();
    let mut s = tokio::net::TcpStream::connect(host).await.unwrap();
    s.write_all(format!("GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut raw = String::new();
    tokio::time::timeout(WAIT, s.read_to_string(&mut raw)).await.expect("the page did not answer").unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
    let status = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let location = head.lines().find_map(|l| l.to_ascii_lowercase().starts_with("location:").then(|| l[9..].trim().to_string())).unwrap_or_default();
    (status, location, body.to_string())
}

fn query(url: &str) -> HashMap<String, String> {
    reqwest::Url::parse(url).unwrap().query_pairs().into_owned().collect()
}

fn row<'a>(status: &'a Value, name: &str) -> &'a Value {
    status["servers"].as_array().and_then(|l| l.iter().find(|s| s["name"] == name)).unwrap_or_else(|| panic!("no server {name} in {status}"))
}

async fn list_after(c: &mut Client, from: usize) -> Value {
    c.wait_after(from, "an `mcp` list", |m| m["op"] == "mcp" && m["engine"] == ENGINE).await
}

/// A list, asked for until `name` reads `status`: a list already on its way
/// may be older than what the test just did.
async fn settled(c: &mut Client, op: &str, name: &str, status: &str) -> Value {
    let mut last = Value::Null;
    for _ in 0..20 {
        let from = c.mark();
        c.send(json!({ "op": op, "engine": ENGINE, "server": name })).await;
        last = list_after(c, from).await;
        if row(&last, name)["status"] == status {
            return last;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("{name} never read {status}: {last}");
}

async fn auth_state(c: &mut Client, from: usize, server: &str, state: &str) -> Value {
    let what = format!("mcp.auth {state} for {server}");
    c.wait_after(from, &what, |m| m["op"] == "mcp.auth" && m["engine"] == ENGINE && m["server"] == server && m["state"] == state).await
}

async fn add(c: &mut Client, name: &str, url: &str) -> Value {
    let from = c.mark();
    c.send(json!({ "op": "mcp.add", "engine": ENGINE, "name": name, "url": url })).await;
    let change = c.wait_after(from, "mcp.change", |m| m["op"] == "mcp.change" && m["engine"] == ENGINE && m["name"] == name).await;
    assert_eq!((change["state"].as_str(), change["account"].as_str()), (Some("done"), Some("default")), "{change}");
    list_after(c, from).await
}

/// Start a sign-in and let the fake's consent page send the browser back:
/// the `open` message and the address the browser ends on.
async fn start_sign_in(c: &mut Client, name: &str) -> (Value, String) {
    let from = c.mark();
    c.send(json!({ "op": "mcp.auth", "engine": ENGINE, "server": name })).await;
    let open = auth_state(c, from, name, "open").await;
    let (status, location, _) = browse(open["url"].as_str().unwrap()).await;
    assert_eq!(status, 302);
    (open, location)
}

fn file(sb: &Sandbox) -> String {
    std::fs::read_to_string(sb.data().join("held-connectors.json")).unwrap_or_default()
}

/// The `mcpServers` of each session the scripted agent opened or reopened.
fn servers_given(sb: &Sandbox, method: &str) -> Vec<Value> {
    let tag = format!("opencode servers {method} ");
    std::fs::read_to_string(sb.fake_state().join("acp.log"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix(&tag))
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn names(servers: &Value) -> Vec<&str> {
    servers.as_array().unwrap().iter().filter_map(|s| s["name"].as_str()).collect()
}

/// Run a bridge the way the agent does (the entry's command, args and env),
/// send it these lines, and read what it prints until it has answered `ids`.
async fn bridge(entry: &Value, lines: &[Value], ids: &[u64]) -> Vec<Value> {
    let mut cmd = tokio::process::Command::new(entry["command"].as_str().unwrap());
    cmd.args(entry["args"].as_array().unwrap().iter().filter_map(Value::as_str)).env_clear();
    for e in entry["env"].as_array().unwrap() {
        cmd.env(e["name"].as_str().unwrap(), e["value"].as_str().unwrap());
    }
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).kill_on_drop(true).spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut got: Vec<Value> = Vec::new();
    let answered = |got: &[Value], id: u64| got.iter().any(|m| m["id"] == id);
    for line in lines {
        stdin.write_all(format!("{line}\n").as_bytes()).await.unwrap();
        // A client waits for `initialize` before it says anything else.
        if line["method"] == "initialize" {
            while !answered(&got, line["id"].as_u64().unwrap()) {
                let next = tokio::time::timeout(WAIT, out.next_line()).await.expect("the bridge did not answer initialize").unwrap().expect("the bridge closed");
                got.push(serde_json::from_str(&next).unwrap());
            }
        }
    }
    while !ids.iter().all(|id| answered(&got, *id)) {
        let next = tokio::time::timeout(WAIT, out.next_line()).await.unwrap_or_else(|_| panic!("the bridge did not answer {ids:?}: {got:?}")).unwrap().expect("the bridge closed");
        got.push(serde_json::from_str(&next).unwrap());
    }
    drop(stdin);
    let _ = tokio::time::timeout(WAIT, child.wait()).await.expect("the bridge did not exit when its input closed");
    got
}

fn answer(got: &[Value], id: u64) -> &Value {
    got.iter().find(|m| m["id"] == id).unwrap()
}

fn opencode() -> Value {
    json!({ "engine": "opencode", "model": "opencode/fake-free", "effort": "default" })
}

async fn agent(c: &mut Client, name: &str, mcp: Option<Value>) -> String {
    let from = c.mark();
    let r = format!("save-{from}");
    let mut record = json!({ "name": name, "engine": "opencode", "model": "opencode/fake-free" });
    if let Some(mcp) = mcp {
        record["mcp"] = mcp;
    }
    c.send(json!({ "op": "agent.save", "agent": record, "ref": r })).await;
    let reply = c.wait_after(from, "agent.save reply", |m| m["ref"] == r.as_str()).await;
    assert_eq!(reply["op"], "agent.saved", "{reply}");
    reply["agent"]["id"].as_str().unwrap().to_string()
}

async fn ask(c: &mut Client, agent: &str, text: &str) -> String {
    let from = c.mark();
    let r = format!("ask-{from}");
    c.send(json!({ "op": "agent.ask", "agentId": agent, "text": text, "ref": r })).await;
    let reply = c.wait_after(from, "agent.ask reply", |m| m["ref"] == r.as_str() && (m["op"] == "created" || m["op"] == "error")).await;
    assert_eq!(reply["op"], "created", "{reply}");
    reply["sessionId"].as_str().unwrap().to_string()
}

/// Nothing a window was sent, and nothing yardd logged, holds a credential.
fn no_secret_left_the_file(c: &Client, d: &Daemon) {
    for secret in ["TEST-access", "TEST-refresh", "TEST-code"] {
        assert!(!c.log.iter().any(|m| m.to_string().contains(secret)), "a broadcast carries {secret}");
        assert!(!d.log_text().contains(secret), "yardd's log carries {secret}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_connector_signs_in_on_the_loopback_page_and_sessions_get_its_bridge() {
    let fake = fake_server().await;
    let url = format!("{}/mcp", fake.lock().unwrap().base);
    let sb = Sandbox::new("held-loopback");
    let d = sb.start();
    let mut c = d.client().await;
    c.wait_for("opencode models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["opencode"].as_array().is_some_and(|l| !l.is_empty())).await;

    // Only the engines yardd knows connectors for; and only an address that is https or local.
    let from = c.mark();
    c.send(json!({ "op": "mcp.status", "engine": "gemini" })).await;
    assert!(c.wait_error(from, "mcp.status").await.contains("Unknown engine for connectors: gemini"));
    let from = c.mark();
    c.send(json!({ "op": "mcp.add", "engine": ENGINE, "name": "far", "url": "http://mcp.example.com:80/mcp" })).await;
    assert!(c.wait_error(from, "mcp.add").await.contains("https://"));

    let from = c.mark();
    c.send(json!({ "op": "mcp.status", "engine": ENGINE })).await;
    let empty = list_after(&mut c, from).await;
    assert_eq!((empty["account"].as_str(), empty["servers"].as_array().map(Vec::len)), (Some("default"), Some(0)), "{empty}");

    let listed = add(&mut c, "fake", &url).await;
    let r = row(&listed, "fake");
    assert_eq!((r["status"].as_str(), r["source"].as_str(), r["appTools"].as_u64()), (Some("needs-auth"), Some("switchyard"), Some(0)), "{r}");
    assert_eq!(r["origin"].as_str(), Some(fake.lock().unwrap().base.as_str()));
    let mode = std::fs::metadata(sb.data().join("held-connectors.json")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    // A second one of the same name is refused, and says so.
    let from = c.mark();
    c.send(json!({ "op": "mcp.add", "engine": ENGINE, "name": "fake", "url": &url })).await;
    let again = c.wait_after(from, "mcp.change", |m| m["op"] == "mcp.change" && m["name"] == "fake").await;
    assert_eq!(again["state"], "failed", "{again}");

    // A session opened before the sign-in gets no bridge yet.
    let early = c.create(&sb.folder("early"), "hi", opencode()).await;
    c.wait_turn_done(&early, 0).await;
    c.wait_status(&early, "idle").await;
    assert_eq!(servers_given(&sb, "session/new"), [json!([])]);

    let (open, landed) = start_sign_in(&mut c, "fake").await;
    assert_eq!(open["paste"], true);
    let q = query(open["url"].as_str().unwrap());
    let redirect = format!("http://127.0.0.1:{}/connector/callback", d.port);
    assert!(open["url"].as_str().unwrap().starts_with(&format!("{}/authorize?", fake.lock().unwrap().base)));
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["client_id"], "TEST-client-1");
    assert_eq!(q["redirect_uri"], redirect);
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["code_challenge"].len(), 43);
    assert!(q["state"].len() >= 22, "{q:?}");
    assert_eq!(q["resource"], url);
    assert_eq!(q["scope"], "read write");
    let registered = fake.lock().unwrap().registrations[0].clone();
    assert_eq!(registered["redirect_uris"], json!([redirect]));
    assert_eq!((registered["client_name"].as_str(), registered["token_endpoint_auth_method"].as_str()), (Some("Switchyard"), Some("none")));
    assert_eq!(registered["grant_types"], json!(["authorization_code", "refresh_token"]));

    // A redirect with a state that is not this sign-in's is refused, and spends nothing.
    let (status, _, body) = browse(&format!("{redirect}?code=TEST-code-1&state=not-the-state")).await;
    assert_eq!(status, 400, "{body}");
    assert!(fake.lock().unwrap().token_forms.is_empty());

    // The browser lands on yardd's own page, which finishes the sign-in.
    assert!(landed.starts_with(&redirect), "{landed}");
    let from = c.mark();
    let (status, _, body) = browse(&landed).await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Connected"), "{body}");
    auth_state(&mut c, from, "fake", "done").await;
    let after = list_after(&mut c, from).await;
    assert_eq!((row(&after, "fake")["status"].as_str(), row(&after, "fake")["tools"].as_u64()), (Some("connected"), Some(2)), "{after}");
    {
        let f = fake.lock().unwrap();
        let form = &f.token_forms[0];
        assert_eq!((form["grant_type"].as_str(), form["resource"].as_str(), form["client_id"].as_str()), ("authorization_code", url.as_str(), "TEST-client-1"));
        assert_eq!(challenge_of(&form["code_verifier"]), q["code_challenge"]);
        // The check answered once as JSON and once as an event stream.
        assert!(f.seen.iter().any(|s| s.3) && f.seen.iter().any(|s| !s.3 && s.0 != "notifications/initialized"), "{:?}", f.seen);
    }
    // The same address a second time: the state was spent.
    let (status, _, body) = browse(&landed).await;
    assert_eq!(status, 400, "{body}");
    assert!(file(&sb).contains("TEST-access-"), "the token is kept in the file");

    // A new session of the owner's gets the bridge; the idle one gets it with its next turn.
    let sid = c.create(&sb.folder("proj"), "hi", opencode()).await;
    c.wait_turn_done(&sid, 0).await;
    c.wait_status(&sid, "idle").await;
    let given = servers_given(&sb, "session/new");
    let entry = given[1][0].clone();
    assert_eq!(names(&given[1]), ["fake"], "{given:?}");
    assert_eq!(entry["args"], json!(["mcp-proxy"]));
    assert!(!entry.to_string().contains("TEST-"), "the agent is given a key of yardd's, not the token");
    let next = c.next_seq(&early);
    c.send(json!({ "op": "session.send", "sessionId": early, "text": "again" })).await;
    assert_eq!(c.wait_turn_done(&early, next).await["status"], "completed");
    c.wait_status(&early, "idle").await;
    let reopened = servers_given(&sb, "session/resume");
    assert_eq!(names(&reopened[0]), ["fake"], "{reopened:?}");
    // Nothing changed since: its next turn does not reopen it.
    let next = c.next_seq(&early);
    c.send(json!({ "op": "session.send", "sessionId": early, "text": "and again" })).await;
    c.wait_turn_done(&early, next).await;
    c.wait_status(&early, "idle").await;
    assert_eq!(servers_given(&sb, "session/resume").len(), 1);

    // The bridge, run as the agent runs it, reaches the fake's tools.
    fake.lock().unwrap().seen.clear();
    let got = bridge(
        &entry,
        &[
            json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "t", "version": "0" } } }),
            json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
            json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
            json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "lookup", "arguments": { "q": "TEST-q" } } }),
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "create", "arguments": {} } }),
        ],
        &[2, 3, 4],
    )
    .await;
    assert_eq!(answer(&got, 1)["result"]["serverInfo"]["name"], "fake", "{got:?}");
    let tools: Vec<&str> = answer(&got, 2)["result"]["tools"].as_array().unwrap().iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(tools, ["lookup", "create"]);
    assert_eq!(answer(&got, 3)["result"]["content"][0]["text"], r#"fake ran lookup with {"q":"TEST-q"}"#);
    assert_eq!(answer(&got, 4)["result"]["isError"], false);
    assert!(got.iter().any(|m| m["method"] == "notifications/message"), "what a stream carries besides the answer is passed on: {got:?}");
    {
        let f = fake.lock().unwrap();
        assert_eq!(f.calls.len(), 2);
        let after_init: Vec<_> = f.seen.iter().filter(|s| s.0 != "initialize").collect();
        assert!(after_init.len() >= 4 && after_init.iter().all(|s| s.1 == "TEST-session" && s.2 == "2025-06-18"), "the session and version travel with every later message: {:?}", f.seen);
        assert!(f.seen.iter().any(|s| s.3) && f.seen.iter().any(|s| !s.3 && s.0.starts_with("tools/")), "{:?}", f.seen);
    }

    // An agent's conversation is scoped: a new agent's list is empty, so it
    // gets Switchyard's tools only; one whose list names the connector gets both.
    let ada = agent(&mut c, "Ada", None).await;
    let ada_sid = ask(&mut c, &ada, "hi").await;
    c.wait_turn_done(&ada_sid, 0).await;
    let bo = agent(&mut c, "Bo", Some(json!(["fake"]))).await;
    let bo_sid = ask(&mut c, &bo, "hi").await;
    c.wait_turn_done(&bo_sid, 0).await;
    let given = servers_given(&sb, "session/new");
    assert_eq!(names(&given[2]), ["switchyard"], "{given:?}");
    assert_eq!(names(&given[3]), ["switchyard", "fake"], "{given:?}");
    // Ada's key would not be honoured even if it were made: the bridge of a
    // session answers for that session only, so Bo's entry differs from the owner's.
    assert_ne!(given[3][1]["env"], entry["env"]);

    // Remove: gone from the list and the file, tokens and registration included.
    let from = c.mark();
    c.send(json!({ "op": "mcp.remove", "engine": ENGINE, "server": "fake" })).await;
    let change = c.wait_after(from, "mcp.change", |m| m["op"] == "mcp.change" && m["name"] == "fake").await;
    assert_eq!(change["state"], "done", "{change}");
    assert_eq!(list_after(&mut c, from).await["servers"], json!([]));
    assert!(!file(&sb).contains("TEST-"), "{}", file(&sb));
    assert!(!file(&sb).contains("fake"));
    // Its bridge stops answering, and the idle session loses it with its next turn.
    let got = bridge(&entry, &[json!({ "jsonrpc": "2.0", "id": 9, "method": "tools/list" })], &[9]).await;
    assert!(answer(&got, 9)["error"]["message"].as_str().unwrap().contains("unknown key"), "{got:?}");
    let next = c.next_seq(&sid);
    c.send(json!({ "op": "session.send", "sessionId": sid, "text": "still there?" })).await;
    assert_eq!(c.wait_turn_done(&sid, next).await["status"], "completed");
    assert_eq!(servers_given(&sb, "session/resume").last().unwrap(), &json!([]));

    no_secret_left_the_file(&c, &d);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pasted_address_signs_in_and_the_token_is_refreshed_until_that_fails() {
    let fake = fake_server().await;
    let url = format!("{}/mcp", fake.lock().unwrap().base);
    let sb = Sandbox::new("held-paste");
    let d = sb.start();
    let mut c = d.client().await;
    c.wait_for("opencode models", |m| (m["op"] == "snapshot" || m["op"] == "models") && m["models"]["opencode"].as_array().is_some_and(|l| !l.is_empty())).await;
    add(&mut c, "fake", &url).await;

    // A server that takes no new clients: said plainly.
    fake.lock().unwrap().no_registration = true;
    let from = c.mark();
    c.send(json!({ "op": "mcp.auth", "engine": ENGINE, "server": "fake" })).await;
    let failed = auth_state(&mut c, from, "fake", "failed").await;
    assert!(failed["error"].as_str().unwrap().contains("registered with it in advance"), "{failed}");
    fake.lock().unwrap().no_registration = false;

    // The first token is about to expire as soon as it arrives.
    fake.lock().unwrap().code_expires_in = 1;
    let (_, landed) = start_sign_in(&mut c, "fake").await;
    let state = query(&landed)["state"].clone();

    // Another state is refused and leaves the real sign-in open.
    let from = c.mark();
    c.send(json!({ "op": "mcp.callback", "engine": ENGINE, "server": "fake", "url": landed.replace(&state, "someone-elses") })).await;
    let failed = auth_state(&mut c, from, "fake", "failed").await;
    assert!(failed["error"].as_str().unwrap().contains("not in progress"), "{failed}");
    assert!(fake.lock().unwrap().token_forms.is_empty());

    // The address the browser ended on, pasted.
    let from = c.mark();
    c.send(json!({ "op": "mcp.callback", "engine": ENGINE, "server": "fake", "url": &landed })).await;
    auth_state(&mut c, from, "fake", "done").await;
    // The list that follows had to refresh the token before it could ask.
    let after = settled(&mut c, "mcp.status", "fake", "connected").await;
    assert_eq!(row(&after, "fake")["tools"], 2);
    {
        let f = fake.lock().unwrap();
        assert!(f.refreshes >= 1, "the expiring token was refreshed");
        let form = f.token_forms.iter().find(|t| t["grant_type"] == "refresh_token").unwrap();
        assert_eq!((form["refresh_token"].as_str(), form["resource"].as_str(), form["client_id"].as_str()), ("TEST-refresh-2", url.as_str(), "TEST-client-1"));
    }
    assert!(!file(&sb).contains("TEST-access-2"), "the first token was replaced: {}", file(&sb));
    // Pasting it again: the state was spent.
    let from = c.mark();
    c.send(json!({ "op": "mcp.callback", "engine": ENGINE, "server": "fake", "url": &landed })).await;
    auth_state(&mut c, from, "fake", "failed").await;

    // The server stops honouring the token before its time: one refresh, and it works again.
    let before = {
        let mut f = fake.lock().unwrap();
        f.access.clear();
        f.refreshes
    };
    // A list already on its way may predate the change: wait for the refresh itself.
    for _ in 0..100 {
        settled(&mut c, "mcp.reconnect", "fake", "connected").await;
        if fake.lock().unwrap().refreshes > before {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    settled(&mut c, "mcp.reconnect", "fake", "connected").await;
    assert_eq!(fake.lock().unwrap().refreshes, before + 1, "one refresh, however many checks ran into the refusal");

    // The refresh is refused: the tokens go, and it needs a sign-in again.
    {
        let mut f = fake.lock().unwrap();
        f.access.clear();
        f.fail_refresh = true;
    }
    let after = settled(&mut c, "mcp.reconnect", "fake", "needs-auth").await;
    assert!(row(&after, "fake")["error"].is_null(), "{after}");
    assert!(!file(&sb).contains("TEST-access") && !file(&sb).contains("TEST-refresh"), "{}", file(&sb));
    assert!(file(&sb).contains("TEST-client-1"), "the registration is kept for the next sign-in");
    let sid = c.create(&sb.folder("proj"), "hi", opencode()).await;
    c.wait_turn_done(&sid, 0).await;
    assert_eq!(servers_given(&sb, "session/new"), [json!([])], "a connector that needs a sign-in is given to no session");

    // Signing in again reuses the registration.
    fake.lock().unwrap().fail_refresh = false;
    let (open, landed) = start_sign_in(&mut c, "fake").await;
    assert_eq!(query(open["url"].as_str().unwrap())["client_id"], "TEST-client-1");
    assert_eq!(fake.lock().unwrap().registrations.len(), 1);
    let from = c.mark();
    c.send(json!({ "op": "mcp.callback", "engine": ENGINE, "server": "fake", "url": &landed })).await;
    auth_state(&mut c, from, "fake", "done").await;
    settled(&mut c, "mcp.status", "fake", "connected").await;

    no_secret_left_the_file(&c, &d);
}
