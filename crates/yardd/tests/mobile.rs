mod common;
use common::Sandbox;
use serde_json::json;

#[tokio::test]
async fn a_phone_connection_updates_other_clients_and_survives_restart() {
    let sandbox = Sandbox::new("mobile");
    let daemon = sandbox.start_with(&[
        ("YARD_TESTFLIGHT_URL", "https://testflight.apple.com/join/AbCd1234"),
        ("YARD_ALLOW_ORIGIN", "https://yard.example.ts.net,http://100.64.0.1:47717"),
    ]);
    let mut desktop = daemon.client().await;
    let snapshot = desktop.wait_for("snapshot", |m| m["op"] == "snapshot").await;
    assert_eq!(snapshot["mobile"]["configured"], false);
    assert_eq!(snapshot["mobile"]["testFlightUrl"], "https://testflight.apple.com/join/AbCd1234");
    // Only an address a phone can open is offered for its QR code.
    assert_eq!(snapshot["mobile"]["origins"], json!(["https://yard.example.ts.net"]));

    let mut phone = daemon.client().await;
    phone.send(json!({ "op": "mobile.connected", "device": "Test iPhone" })).await;
    let status = desktop.wait_for("phone connected", |m| m["op"] == "mobile.status").await;
    assert_eq!(status["mobile"]["configured"], true);
    assert_eq!(status["mobile"]["live"], json!(["Test iPhone"]));
    assert_eq!(status["mobile"]["devices"][0]["name"], "Test iPhone");
    // Saying so again on the same socket is the same phone, not a second one.
    phone.send(json!({ "op": "mobile.connected", "device": "Test iPhone" })).await;

    drop(phone);
    let status = desktop.wait_for("phone gone", |m| m["op"] == "mobile.status" && m["mobile"]["live"] == json!([])).await;
    assert!(status["mobile"]["devices"][0]["lastSeen"].as_u64().unwrap() > 0);
    assert_eq!(status["mobile"]["devices"].as_array().unwrap().len(), 1);
    drop(desktop);
    daemon.stop();

    let daemon = sandbox.start();
    let snapshot = daemon.client().await.wait_for("restored snapshot", |m| m["op"] == "snapshot").await;
    assert_eq!(snapshot["mobile"]["configured"], true);
    assert_eq!(snapshot["mobile"]["devices"][0]["name"], "Test iPhone");
    assert_eq!(snapshot["mobile"]["live"], json!([]));
}
