//! Which MCP servers (connectors) an agent's or an automation's sessions get.
//!
//! Both records carry `mcp`, set by the owner: `Some(list)` is exactly those
//! servers (names as the account's Connectors page lists them, claude.ai
//! connectors included), `None` (a record saved before the field, or pushed by
//! a yardd that drops it) is every server the account has except
//! `DENIED_BY_DEFAULT`. Any other session is not scoped. Switchyard's own
//! agent tools are never affected.

use anyhow::{bail, Result};

use crate::state::App;

/// Servers a record without its own list does not get. A Metabase account was
/// disabled for the connections many idle threads held open.
pub const DENIED_BY_DEFAULT: [&str; 2] = ["metabase", "claude.ai Metabase"];

/// Agents whose record without a list keeps every server. Firefighter was set
/// up to read Metabase, and its record can arrive from a hub whose yardd
/// predates `mcp`, so its default cannot be the deny list.
const KEEPS_EVERY_SERVER: [&str; 1] = ["g1a212f7695f8"];

/// A list is a few names; this only keeps a script from saving megabytes.
const MAX_SERVERS: usize = 200;
const MAX_NAME: usize = 200;

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    /// Exactly these servers.
    Only(Vec<String>),
    /// Every server but these (matched ignoring case).
    Except(Vec<String>),
}

impl Scope {
    pub fn allows(&self, name: &str) -> bool {
        match self {
            Scope::Only(list) => list.iter().any(|n| n == name),
            Scope::Except(list) => !list.iter().any(|n| n.eq_ignore_ascii_case(name)),
        }
    }

    /// For the Claude sidecar's `start`.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Scope::Only(list) => serde_json::json!({ "only": list }),
            Scope::Except(list) => serde_json::json!({ "except": list }),
        }
    }
}

/// The scope of a record (`id` names an agent or an automation).
pub fn of_record(id: &str, mcp: Option<&Vec<String>>) -> Option<Scope> {
    match mcp {
        Some(list) => Some(Scope::Only(list.clone())),
        None if KEEPS_EVERY_SERVER.contains(&id) => None,
        None => Some(Scope::Except(DENIED_BY_DEFAULT.iter().map(|s| s.to_string()).collect())),
    }
}

/// The scope of a session: its agent's, else its automation's. `None` is
/// every server, as for any session of the owner's.
pub fn of_session(app: &App, sid: &str) -> Option<Scope> {
    if crate::private::internal(sid) {
        return None;
    }
    let meta = app.meta(sid)?;
    if let Some(a) = meta.agent.as_deref().and_then(|id| app.agents.get(id)) {
        return of_record(&a.id, a.mcp.as_ref());
    }
    if let Some(a) = meta.automation.as_deref().and_then(|id| app.automations.get(id)) {
        return of_record(&a.id, a.mcp.as_ref());
    }
    None
}

/// A saved list: trimmed, without blanks or repeats, of a sane size.
pub fn clean(list: &mut Vec<String>) -> Result<()> {
    let mut out: Vec<String> = Vec::new();
    for name in list.iter() {
        let name = name.trim();
        if name.is_empty() || out.iter().any(|n| n == name) {
            continue;
        }
        if name.chars().count() > MAX_NAME || name.chars().any(char::is_control) {
            bail!("`{}` is not a connector name", name.chars().take(40).collect::<String>());
        }
        out.push(name.to_string());
    }
    if out.len() > MAX_SERVERS {
        bail!("at most {MAX_SERVERS} connectors");
    }
    *list = out;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_list_denies_metabase_and_a_list_is_exact() {
        let default = of_record("g1", None).unwrap();
        assert!(default.allows("handoff"));
        assert!(!default.allows("metabase"));
        assert!(!default.allows("Metabase"));
        assert!(!default.allows("claude.ai Metabase"));
        let only = of_record("g1", Some(&vec!["handoff".into()])).unwrap();
        assert!(only.allows("handoff"));
        assert!(!only.allows("Handoff"));
        assert!(!only.allows("metabase"));
        assert_eq!(of_record("g1", Some(&vec![])), Some(Scope::Only(vec![])));
    }

    #[test]
    fn firefighter_keeps_every_server_until_it_has_a_list() {
        assert_eq!(of_record("g1a212f7695f8", None), None);
        assert_eq!(of_record("g1a212f7695f8", Some(&vec!["metabase".into()])), Some(Scope::Only(vec!["metabase".into()])));
    }

    #[test]
    fn lists_are_cleaned() {
        let mut list = vec![" a ".to_string(), "".into(), "a".into(), "claude.ai Metabase".into()];
        clean(&mut list).unwrap();
        assert_eq!(list, ["a", "claude.ai Metabase"]);
        assert!(clean(&mut vec!["bad\nname".into()]).is_err());
    }
}
