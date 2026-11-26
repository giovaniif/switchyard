//! Phones on this host: which ones connected, when they were last seen, which
//! are connected now, and the addresses a phone can reach this host at.
//! A phone counts only after its app has received this hub's snapshot.
use std::{path::PathBuf, sync::Mutex};

use anyhow::Result;
use serde_json::{json, Value};

/// Phones remembered, most recent first.
const MAX_DEVICES: usize = 5;

/// Switchyard for iPhone's public TestFlight beta, so the setup card can offer
/// the install without any configuration. `YARD_TESTFLIGHT_URL` replaces it
/// (a fork with its own app); `none` offers no install link.
const TESTFLIGHT: &str = "https://testflight.apple.com/join/BygX29JY";

/// Empty counts as unset: docker-compose passes the variable through empty.
fn testflight_url(var: Option<String>) -> Option<String> {
    match var.as_deref().map(str::trim) {
        None | Some("") => Some(TESTFLIGHT.into()),
        Some("none") => None,
        Some(v) => Some(v.into()),
    }
}

pub struct Mobile {
    data: PathBuf,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    configured: bool,
    /// `{ name, lastSeen }`, most recent first.
    devices: Vec<Value>,
    /// Phone connections open now, by device name.
    live: Vec<String>,
}

impl Mobile {
    pub fn load(data: PathBuf) -> Self {
        let saved = std::fs::read(data.join("mobile.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .unwrap_or(Value::Null);
        let inner = Inner {
            configured: saved["configured"] == true,
            devices: saved["devices"].as_array().cloned().unwrap_or_default(),
            live: Vec::new(),
        };
        Self { data, inner: Mutex::new(inner) }
    }

    pub fn status(&self) -> Value {
        let inner = self.inner.lock().unwrap();
        json!({
            "configured": inner.configured,
            "testFlightUrl": testflight_url(std::env::var("YARD_TESTFLIGHT_URL").ok()),
            "devices": inner.devices,
            "live": inner.live,
            "origins": phone_origins(&crate::ws::allowed_origins()),
        })
    }

    /// A phone's app got the snapshot. Returns the name it is known by, for
    /// `disconnected` when its socket closes.
    pub fn connected(&self, device: Option<&str>) -> Result<String> {
        let name = device.map(|d| d.trim().chars().take(60).collect::<String>()).filter(|d| !d.is_empty()).unwrap_or_else(|| "iPhone".into());
        let mut inner = self.inner.lock().unwrap();
        inner.configured = true;
        inner.live.push(name.clone());
        seen(&mut inner, &name);
        self.save(&inner)?;
        Ok(name)
    }

    pub fn disconnected(&self, name: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(i) = inner.live.iter().position(|n| n == name) {
            inner.live.remove(i);
        }
        seen(&mut inner, name);
        let _ = self.save(&inner);
    }

    fn save(&self, inner: &Inner) -> Result<()> {
        std::fs::create_dir_all(&self.data)?;
        let tmp = self.data.join("mobile.json.tmp");
        std::fs::write(&tmp, json!({ "configured": inner.configured, "devices": inner.devices }).to_string())?;
        std::fs::rename(tmp, self.data.join("mobile.json"))?;
        Ok(())
    }
}

fn seen(inner: &mut Inner, name: &str) {
    inner.devices.retain(|d| d["name"] != name);
    inner.devices.insert(0, json!({ "name": name, "lastSeen": crate::state::now_ms() }));
    inner.devices.truncate(MAX_DEVICES);
}

/// The page origins a phone could open: HTTPS, and not this machine's own
/// loopback, which on a phone is the phone. Keep in step with `mobileHost`
/// in ui/src/mobile.ts.
fn phone_origins(allowed: &[String]) -> Vec<String> {
    allowed
        .iter()
        .filter(|o| {
            let Some(host) = o.strip_prefix("https://") else { return false };
            let host = host.split(':').next().unwrap_or("").to_ascii_lowercase();
            !(host.is_empty() || host == "localhost" || host.ends_with(".localhost") || host == "0.0.0.0" || host.starts_with("127.") || host.starts_with('['))
                && !o["https://".len()..].contains('/')
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{phone_origins, testflight_url, TESTFLIGHT};

    #[test]
    fn the_public_beta_is_offered_unless_replaced_or_turned_off() {
        assert_eq!(testflight_url(None).as_deref(), Some(TESTFLIGHT));
        assert_eq!(testflight_url(Some(" ".into())).as_deref(), Some(TESTFLIGHT));
        assert_eq!(testflight_url(Some("https://testflight.apple.com/join/AbCd1234".into())).as_deref(), Some("https://testflight.apple.com/join/AbCd1234"));
        assert_eq!(testflight_url(Some("none".into())), None);
    }

    #[test]
    fn only_https_addresses_a_phone_can_reach_are_offered() {
        let allowed: Vec<String> = ["https://switchyard.example.ts.net", "http://100.64.0.1:47717", "https://localhost:8443", "https://127.0.0.1", "https://yard.test:8443", "https://[::1]"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(phone_origins(&allowed), ["https://switchyard.example.ts.net", "https://yard.test:8443"]);
    }
}
