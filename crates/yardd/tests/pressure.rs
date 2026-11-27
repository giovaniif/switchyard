//! Pressure episodes: yardd records each stretch of time its host is
//! overloaded (load above the cores, memory under the floor, swap nearly
//! full), with its peaks and who used the most at the peak, keeps them across
//! restarts, and says which running turns went quiet meanwhile. The load,
//! swap and per-process numbers come from a readings file
//! (`YARD_TEST_READINGS`), so the test machine's own load decides nothing.

mod common;

use std::{
    path::Path,
    process::{Child, Command, Stdio},
};

use common::*;
use serde_json::{json, Value};

const GIB: u64 = 1024 * 1024 * 1024;

/// A process of the test's own, outside every session: the kind of thing
/// (another app, a test daemon left behind) that is often the real culprit.
struct Stray(Child);

impl Stray {
    fn start() -> Stray {
        Stray(Command::new("sleep").arg("317").stdout(Stdio::null()).stderr(Stdio::null()).spawn().expect("sleep starts"))
    }
    fn pid(&self) -> u32 {
        self.0.id()
    }
}

impl Drop for Stray {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn write_readings(path: &Path, readings: Value) {
    // Written aside and renamed, so a sample never reads half a file.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, readings.to_string()).unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

/// The next `host` message after `from` that satisfies `pred`.
async fn host_after(c: &mut Client, from: usize, what: &str, pred: impl Fn(&Value) -> bool) -> Value {
    c.wait_after(from, what, |m| m["op"] == "host" && pred(&m["host"])).await["host"].clone()
}

#[tokio::test]
async fn an_episode_records_the_top_session_and_a_stray_process_closes_and_survives_a_restart() {
    let sb = Sandbox::new("pressure");
    let readings = sb.root.join("readings.json");
    let readings_path = readings.to_str().unwrap().to_string();
    let sidecar = fake_bin().join("yard-claude");
    let env = [
        ("YARD_SIDECAR", sidecar.to_str().unwrap()),
        ("YARD_TEST_READINGS", readings_path.as_str()),
        ("YARD_TEST_PRESSURE_CALM_SECS", "1"),
        ("YARD_TEST_PRESSURE_NOTIFY_SECS", "0"),
    ];
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    let stray = Stray::start();

    // A Claude session (the scripted sidecar) keeps its sidecar after the
    // turn: its processes are what the sampler measures for it.
    let extra = json!({ "engine": "claude", "model": "claude-haiku-5-5", "effort": "low" });
    let sid = c.create(&sb.folder("proj"), "TEST-hog", extra).await;
    c.wait_turn_done(&sid, 0).await;
    let from = c.mark();
    let calm = host_after(&mut c, from, "a sample that measures the session", |h| {
        h["sessions"].as_array().is_some_and(|l| l.iter().any(|s| s["id"] == sid))
    })
    .await;
    assert!(calm["episode"].is_null(), "no pressure yet: {calm}");
    assert_eq!(calm["episodes"], json!([]), "nothing recorded yet");

    write_readings(
        &readings,
        json!({
            "load": 140.0,
            "swapUsed": 7 * GIB,
            "swapTotal": 8 * GIB,
            "sessions": { (sid.clone()): { "cpu": 800.0, "mem": 9 * GIB } },
            "procs": { (stray.pid().to_string()): { "cpu": 300.0, "mem": 3 * GIB } },
        }),
    );
    let from = c.mark();
    let host = host_after(&mut c, from, "an open episode", |h| h["episode"].is_object()).await;
    let ep = &host["episode"];
    assert!(ep["end"].is_null(), "still open: {ep}");
    assert_eq!(ep["peak"]["load"], 140.0);
    assert_eq!(ep["peak"]["swapUsed"], 7 * GIB);
    let reasons: Vec<&str> = ep["reasons"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
    assert!(reasons.contains(&"load"), "{reasons:?}");
    let top = &ep["top"]["sessions"][0];
    assert_eq!(top["id"], sid.as_str(), "{ep}");
    assert_eq!(top["title"], "TEST-hog");
    assert_eq!(top["engine"], "claude");
    assert_eq!(top["memBytes"], 9 * GIB);
    assert_eq!(top["cpuCores"], 8.0);
    let proc_ = ep["top"]["procs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["pid"] == stray.pid())
        .unwrap_or_else(|| panic!("the stray process is a top contributor: {ep}"));
    assert!(proc_["cmd"].as_str().unwrap().contains("sleep 317"), "{proc_}");
    assert_eq!(proc_["memBytes"], 3 * GIB);
    // The session's own processes are never listed as strays.
    assert!(ep["top"]["procs"].as_array().unwrap().iter().all(|p| !p["cmd"].as_str().unwrap_or("").contains("yard-claude")), "{ep}");
    let id = ep["id"].as_str().unwrap().to_string();

    // One OS notification per episode, through the windows' notify op. Load
    // is the trouble, so the worst is by CPU: the session (8 cores, not 3).
    let note = c.wait_after(from, "the episode's notification", |m| m["op"] == "notify").await;
    assert!(note["title"].as_str().unwrap().contains("overloaded"), "{note}");
    assert!(note["body"].as_str().unwrap().contains("TEST-hog"), "{note}");
    assert_eq!(note["sessionId"], sid.as_str());

    // Gone for the calm window: the episode closes and is kept.
    std::fs::remove_file(&readings).unwrap();
    let from = c.mark();
    let host = host_after(&mut c, from, "the episode closed", |h| h["episode"].is_null() && h["episodes"][0]["id"] == id.as_str()).await;
    let closed = &host["episodes"][0];
    assert!(closed["end"].as_u64().unwrap() >= closed["start"].as_u64().unwrap(), "{closed}");
    assert_eq!(closed["top"]["sessions"][0]["id"], sid.as_str());
    assert_eq!(c.log[from..].iter().filter(|m| m["op"] == "notify").count(), 0, "notified once");
    assert!(sb.data().join("pressure.json").exists(), "saved in the data dir");

    d.stop();
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    let host = host_after(&mut c, 0, "the episode after a restart", |h| h["episodes"][0]["id"] == id.as_str()).await;
    assert_eq!(host["episodes"][0]["top"]["procs"].as_array().unwrap().iter().filter(|p| p["pid"] == stray.pid()).count(), 1);

    // More, page by page.
    let from = c.mark();
    c.send(json!({ "op": "host.pressure", "ref": "p1" })).await;
    let page = c.wait_after(from, "host.pressure", |m| m["op"] == "host.pressure").await;
    assert_eq!(page["ref"], "p1");
    assert_eq!(page["episodes"][0]["id"], id.as_str());
    assert_eq!(page["more"], false);
    let start = page["episodes"][0]["start"].as_u64().unwrap();
    c.send(json!({ "op": "host.pressure", "before": start })).await;
    let older = c.wait_after(from, "host.pressure before the first", |m| m["op"] == "host.pressure" && m["ref"].is_null()).await;
    assert_eq!(older["episodes"], json!([]));
}

#[tokio::test]
async fn an_open_episode_is_closed_by_a_restart_rather_than_lost() {
    let sb = Sandbox::new("pressure-crash");
    let readings = sb.root.join("readings.json");
    let readings_path = readings.to_str().unwrap().to_string();
    let env = [("YARD_TEST_READINGS", readings_path.as_str())];
    write_readings(&readings, json!({ "load": 99.0 }));
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    let host = host_after(&mut c, 0, "an open episode", |h| h["episode"].is_object()).await;
    let id = host["episode"]["id"].as_str().unwrap().to_string();
    // As after a power cut: no clean stop.
    d.kill();
    std::fs::remove_file(&readings).unwrap();
    let d = sb.start_with(&env);
    let mut c = d.client().await;
    let host = host_after(&mut c, 0, "the episode, closed", |h| h["episodes"][0]["id"] == id.as_str()).await;
    assert!(host["episode"].is_null(), "{host}");
    assert!(host["episodes"][0]["end"].is_u64());
}

#[tokio::test]
async fn forced_pressure_opens_an_episode() {
    let sb = Sandbox::new("pressure-forced");
    let d = sb.start_with(&[("YARD_FORCE_PRESSURE", "1")]);
    let mut c = d.client().await;
    let host = host_after(&mut c, 0, "an open episode", |h| h["episode"].is_object()).await;
    assert_eq!(host["episode"]["reasons"], json!(["forced"]));
}

#[tokio::test]
async fn a_running_turn_quiet_under_pressure_is_listed_as_stalled_until_it_moves() {
    let sb = Sandbox::new("pressure-stall");
    let readings = sb.root.join("readings.json");
    let readings_path = readings.to_str().unwrap().to_string();
    let d = sb.start_with(&[("YARD_TEST_READINGS", readings_path.as_str()), ("YARD_TEST_STALL_SECS", "2")]);
    let mut c = d.client().await;
    let sid = c.create(&sb.folder("proj"), "[slow] TEST-stalled", json!({})).await;
    c.wait_event(&sid, "assistant_delta", 0).await;

    // Quiet for longer than the threshold, but the host is fine: nothing to say.
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let from = c.mark();
    let host = host_after(&mut c, from, "a sample", |_| true).await;
    assert_eq!(host["stalled"], json!([]), "{host}");

    write_readings(&readings, json!({ "load": 140.0 }));
    let from = c.mark();
    let host = host_after(&mut c, from, "the session listed as stalled", |h| {
        h["stalled"].as_array().is_some_and(|l| l.iter().any(|s| s["id"] == sid))
    })
    .await;
    let stalled = host["stalled"].as_array().unwrap().iter().find(|s| s["id"] == sid).unwrap().clone();
    assert!(stalled["quietSecs"].as_u64().unwrap() >= 2, "{stalled}");
    // Said, never acted on: the turn is still running.
    assert_eq!(c.statuses(&sid).last().map(String::as_str), Some("running"));
    assert!(!c.event_types(&sid).iter().any(|t| t == "error" || t == "turn_completed"));

    // The turn moves on (here it ends): no longer listed.
    let seq = c.next_seq(&sid);
    c.send(json!({ "op": "session.interrupt", "sessionId": sid })).await;
    c.wait_turn_done(&sid, seq).await;
    let from = c.mark();
    host_after(&mut c, from, "the session no longer stalled", |h| {
        h["stalled"].as_array().is_some_and(|l| !l.iter().any(|s| s["id"] == sid))
    })
    .await;
}
