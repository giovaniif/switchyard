//! Plan limits as each engine reports them, into what yardd keeps in
//! `usage.json` and sends as `usage` (shapes in `ui/src/protocol.ts`).
//!
//! - Claude: `{ status, windows: { <name>: { utilization, resetsAt, label? } } }`.
//!   The usage report is the whole picture and replaces what was kept; a
//!   turn's `rate_limit_event`s name only the windows that turn touched, so
//!   they are laid over it (`merge_claude_turn`).
//! - Codex: the app-server's `rateLimits` as sent, plus `byLimitId` (every
//!   limit by id, the main one included) and `resetCredits` (the free "Full
//!   reset" credits) from `account/rateLimits/read`.
//! - Grok: its agent's credit allowance (`_x.ai/billing`) as one window in
//!   Claude's shape, so the meters read both the same way.

use serde_json::{json, Value};

/// A turn's windows over the kept ones: a window the turn did not mention
/// keeps its last value (its reset time says when it stops counting).
pub fn merge_claude_turn(kept: &Value, turn: &Value) -> Value {
    let mut out = turn.clone();
    if let (Some(old), Some(new)) = (kept["windows"].as_object(), out.get_mut("windows").and_then(Value::as_object_mut)) {
        for (name, window) in old {
            new.entry(name.clone()).or_insert_with(|| window.clone());
        }
    }
    out
}

/// What `account/rateLimits/read` answered, as kept. None without limits.
pub fn codex_from_read(answer: &Value) -> Option<Value> {
    let mut out = answer.get("rateLimits").filter(|l| l.is_object())?.clone();
    let obj = out.as_object_mut()?;
    if let Some(by) = answer.get("rateLimitsByLimitId").filter(|v| v.is_object()) {
        obj.insert("byLimitId".into(), by.clone());
    }
    if let Some(credits) = answer.get("rateLimitResetCredits").filter(|v| v.is_object()) {
        obj.insert("resetCredits".into(), credits.clone());
    }
    Some(out)
}

/// An `account/rateLimits/updated` notification over the kept limits. It
/// carries one limit and nothing else, so the reset credits and the other
/// limits stay; one for another limit than the main one updates only that.
pub fn merge_codex_update(kept: &Value, update: &Value) -> Value {
    let Some(old) = kept.as_object() else { return update.clone() };
    let id = update["limitId"].as_str();
    let main = old.get("limitId").and_then(Value::as_str);
    let mut out = match (id, main) {
        (Some(id), Some(main)) if id != main => kept.clone(),
        _ => {
            let mut out = update.clone();
            if let Some(new) = out.as_object_mut() {
                for key in ["byLimitId", "resetCredits"] {
                    if let (false, Some(v)) = (new.contains_key(key), old.get(key)) {
                        new.insert(key.into(), v.clone());
                    }
                }
            }
            out
        }
    };
    if let (Some(id), Some(by)) = (id, out.get_mut("byLimitId").and_then(Value::as_object_mut)) {
        by.insert(id.to_string(), update.clone());
    }
    out
}

/// Epoch seconds from a number (seconds or milliseconds), an RFC 3339 time,
/// a bare date (midnight UTC) or a number in a string.
fn seconds(v: &Value) -> Option<f64> {
    if let Some(n) = v.as_f64() {
        return Some(if n > 1e12 { n / 1000.0 } else { n });
    }
    let s = v.as_str()?.trim();
    if let Ok(n) = s.parse::<f64>() {
        return seconds(&json!(n));
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(t.timestamp_millis() as f64 / 1000.0);
    }
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map(|t| t.and_utc().timestamp() as f64)
}

/// WEEKLY or MONTHLY, wherever the answer names its cycle.
fn cycle(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.to_ascii_uppercase()).filter(|s| s.contains("WEEK") || s.contains("MONTH")),
        Value::Object(o) => o.values().find_map(cycle),
        _ => None,
    }
}

/// Grok's credit allowance from its agent's `_x.ai/billing` answer: how much
/// of the period's credits is used and when the period ends. The field names
/// are grok 1.0.77's (`creditUsagePercent`, `currentPeriod`, `billingCycle`);
/// the nesting and value types are read loosely, since no signed-in account
/// was at hand to read a real answer from. None when it has no percentage:
/// a guess is never shown.
pub fn grok_from_billing(answer: &Value) -> Option<Value> {
    let config = [&answer["config"], &answer["billing"], answer]
        .into_iter()
        .find(|c| c["creditUsagePercent"].is_number())?;
    let percent = config["creditUsagePercent"].as_f64()?;
    let period = &config["currentPeriod"];
    let start = seconds(&period["start"]).or_else(|| seconds(&config["billingPeriodStart"]));
    let end = seconds(&period["end"]).or_else(|| seconds(&config["billingPeriodEnd"]));
    let named = cycle(&config["billingCycle"])
        .or_else(|| cycle(&answer["billingCycle"]))
        .or_else(|| cycle(&config["history"][0]["billingCycle"]));
    let days = start.zip(end).map(|(s, e)| (e - s) / 86400.0);
    let (key, label) = match (named.as_deref(), days) {
        (Some(c), _) if c.contains("WEEK") => ("week", "Week"),
        (Some(c), _) if c.contains("MONTH") => ("month", "Month"),
        (_, Some(d)) if d >= 25.0 => ("month", "Month"),
        (_, Some(d)) if d >= 6.0 => ("week", "Week"),
        _ => ("period", "Billing period"),
    };
    Some(json!({ "windows": { key: { "utilization": percent / 100.0, "resetsAt": end, "label": label } } }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_keeps_the_windows_it_did_not_mention() {
        let kept = json!({ "windows": { "five_hour": { "utilization": 0.1, "resetsAt": 5 }, "seven_day_fable": { "utilization": 0.04, "resetsAt": 9 } } });
        let turn = json!({ "status": "allowed", "windows": { "five_hour": { "utilization": 0.47, "resetsAt": 6 } } });
        assert_eq!(
            merge_claude_turn(&kept, &turn),
            json!({ "status": "allowed", "windows": { "five_hour": { "utilization": 0.47, "resetsAt": 6 }, "seven_day_fable": { "utilization": 0.04, "resetsAt": 9 } } })
        );
        assert_eq!(merge_claude_turn(&Value::Null, &turn), turn);
    }

    #[test]
    fn codex_keeps_its_full_resets_and_other_limits() {
        // The shape codex 0.171 answers (one real answer, ids shortened).
        let answer = json!({
            "rateLimits": { "limitId": "codex", "primary": { "usedPercent": 52, "windowDurationMins": 10080, "resetsAt": 1795787772 }, "secondary": null },
            "rateLimitsByLimitId": { "codex": { "limitId": "codex" } },
            "rateLimitResetCredits": { "availableCount": 3, "credits": [{ "id": "c1", "status": "available", "expiresAt": 1796164004, "title": "Full reset" }] },
        });
        let kept = codex_from_read(&answer).unwrap();
        assert_eq!(kept["primary"]["usedPercent"], 52);
        assert_eq!(kept["resetCredits"]["availableCount"], 3);
        assert_eq!(kept["byLimitId"]["codex"]["limitId"], "codex");
        assert!(codex_from_read(&json!({ "rateLimits": null })).is_none());

        // A notification for the main limit keeps the credits.
        let update = json!({ "limitId": "codex", "primary": { "usedPercent": 60, "windowDurationMins": 10080, "resetsAt": 1795787772 } });
        let merged = merge_codex_update(&kept, &update);
        assert_eq!(merged["primary"]["usedPercent"], 60);
        assert_eq!(merged["resetCredits"]["availableCount"], 3);
        assert_eq!(merged["byLimitId"]["codex"]["primary"]["usedPercent"], 60);

        // One for another limit leaves the main windows alone.
        let other = json!({ "limitId": "codex_spark", "primary": { "usedPercent": 9 } });
        let merged = merge_codex_update(&merged, &other);
        assert_eq!(merged["primary"]["usedPercent"], 60);
        assert_eq!(merged["byLimitId"]["codex_spark"]["primary"]["usedPercent"], 9);

        // Nothing kept yet: the notification is all there is.
        assert_eq!(merge_codex_update(&Value::Null, &update), update);
    }

    #[test]
    fn grok_billing_becomes_one_window_with_its_reset() {
        let answer = json!({
            "creditUsagePercent": 23,
            "currentPeriod": { "start": "2026-11-24T00:00:00Z", "end": "2026-12-01T00:00:00Z" },
            "billingCycle": "WEEKLY",
        });
        let end = chrono::DateTime::parse_from_rfc3339("2026-12-01T00:00:00Z").unwrap().timestamp() as f64;
        assert_eq!(grok_from_billing(&answer).unwrap(), json!({ "windows": { "week": { "utilization": 0.23, "resetsAt": end, "label": "Week" } } }));

        // Nested under `config`, epoch milliseconds, the cycle told by the period's length.
        let nested = json!({ "config": { "creditUsagePercent": 50, "currentPeriod": { "start": 1_700_000_000_000u64, "end": 1_702_592_000_000u64 } } });
        let w = &grok_from_billing(&nested).unwrap()["windows"]["month"];
        assert_eq!((w["utilization"].as_f64(), w["resetsAt"].as_f64(), w["label"].as_str()), (Some(0.5), Some(1_702_592_000.0), Some("Month")));

        // A bare date; no cycle and no start.
        let dated = json!({ "creditUsagePercent": 5, "billingPeriodEnd": "2026-12-01" });
        assert_eq!(grok_from_billing(&dated).unwrap()["windows"]["period"]["resetsAt"], json!(end));

        // No percentage, no meter.
        assert!(grok_from_billing(&json!({ "prepaidBalance": 0 })).is_none());
        assert!(grok_from_billing(&Value::Null).is_none());
    }
}
