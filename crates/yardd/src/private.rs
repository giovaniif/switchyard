//! Private engine threads: engine sessions yardd runs for itself (a one-off
//! question such as naming a session). Their ids start with `private-` (or the
//! older `switchy-`); they stay in memory only, never reach a snapshot, the
//! session store or a client op, and their events go to `Private::events`
//! instead of a session log.

use serde_json::Value;
use tokio::sync::broadcast;

use crate::state::App;

pub const PREFIX: &str = "private-";

pub struct Private {
    /// Every event of every private thread, as `(session id, event)`.
    pub events: broadcast::Sender<(String, Value)>,
}

impl Default for Private {
    fn default() -> Self {
        Private { events: broadcast::channel(512).0 }
    }
}

/// A private engine thread: never saved, listed or reachable by a client.
pub fn internal(sid: &str) -> bool {
    sid.starts_with(PREFIX) || sid.starts_with("switchy-")
}

/// Runs with the tool-restricted profile: no tools, no MCP, read-only
/// sandbox, every approval declined, a fresh ephemeral engine thread. Private
/// threads in Plan.
pub fn restricted(app: &App, sid: &str) -> bool {
    internal(sid) && app.meta(sid).is_some_and(|m| m.permission == "plan")
}
