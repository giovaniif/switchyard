//! Accounts: more than one login per engine on this host. Each engine has a
//! Default account (the login the CLI already has, run with the environment
//! yardd was started in) plus added ones, each with its own config folder
//! under `<data dir>/accounts/<id>`: `CLAUDE_CONFIG_DIR` for Claude Code,
//! `CODEX_HOME` for Codex, `XDG_DATA_HOME` for OpenCode (its logins and
//! sessions live in `opencode/` there; its config stays the user's), `GROK_HOME`
//! for Grok, and `GEMINI_CLI_HOME` for Gemini (`.gemini/` inside). yardd never reads what is in those folders; the
//! CLIs keep their own login files there. Saved in `accounts.json`.

use std::{
    path::PathBuf,
    sync::Mutex,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The account id of an engine's own, unchanged login.
pub const DEFAULT: &str = "default";

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub engine: String,
    pub name: String,
}

/// Why an account cannot be removed yet, naming the sessions on it: the
/// user has to find and delete them, and an archived one is not in the
/// sidebar. `sessions` are (title, archived) pairs.
pub fn in_use(sessions: &[(String, bool)]) -> String {
    // Archived ones first: they are the ones the user cannot see, so they
    // must not fall under "N more".
    let mut sessions = sessions.to_vec();
    sessions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    // A title can be a whole first message; the banner has one line.
    let name = |(title, archived): &(String, bool)| {
        let short: String = if title.chars().count() > 50 { title.chars().take(49).collect::<String>() + "…" } else { title.clone() };
        format!("\"{short}\"{}", if *archived { " (archived)" } else { "" })
    };
    const SHOWN: usize = 3;
    match sessions.len() {
        0 => "no session uses this account".to_string(),
        1 => format!("The session {} still uses this account. Delete it first.", name(&sessions[0])),
        n => {
            let mut names: Vec<String> = sessions.iter().take(SHOWN).map(name).collect();
            if n > SHOWN {
                names.push(format!("{} more", n - SHOWN));
            }
            let last = names.pop().unwrap();
            format!("{n} sessions still use this account: {} and {last}. Delete them first.", names.join(", "))
        }
    }
}

pub struct Accounts {
    dir: PathBuf,
    file: PathBuf,
    list: Mutex<Vec<Account>>,
}

/// The variable that points an engine's CLI at an account's folder.
pub fn env_var(engine: &str) -> &'static str {
    match engine {
        "codex" => "CODEX_HOME",
        // OpenCode has no variable of its own for this. Commands its agent
        // runs see the moved data folder too.
        "opencode" => "XDG_DATA_HOME",
        "grok" => "GROK_HOME",
        "gemini" => "GEMINI_CLI_HOME",
        _ => "CLAUDE_CONFIG_DIR",
    }
}

impl Accounts {
    pub fn load(data: PathBuf) -> Self {
        let file = data.join("accounts.json");
        let list = match std::fs::read_to_string(&file) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                eprintln!("yardd: ignoring unreadable {}: {e}", file.display());
                Vec::new()
            }),
            Err(_) => Vec::new(),
        };
        Accounts { dir: data.join("accounts"), file, list: Mutex::new(list) }
    }

    /// The added accounts, for the UI. Default is implicit.
    pub fn list(&self) -> Value {
        json!(*self.list.lock().unwrap())
    }

    pub fn all(&self) -> Vec<Account> {
        self.list.lock().unwrap().clone()
    }

    /// Whether `account` is a real choice for `engine`: Default always is.
    pub fn check(&self, engine: &str, account: &str) -> Result<()> {
        if account == DEFAULT || self.list.lock().unwrap().iter().any(|a| a.id == account && a.engine == engine) {
            return Ok(());
        }
        bail!("no such {engine} account `{account}` on this host")
    }

    /// The folder to run an engine's CLI in for `account`, or `None` for
    /// Default, which runs with yardd's own environment.
    pub fn config_dir(&self, engine: &str, account: &str) -> Result<Option<PathBuf>> {
        if account == DEFAULT {
            return Ok(None);
        }
        self.check(engine, account)?;
        Ok(Some(self.dir.join(account)))
    }

    pub fn add(&self, id: String, engine: &str, name: &str) -> Result<Account> {
        let name = name.trim();
        if !crate::engines::is_engine(engine) {
            bail!("unknown engine {engine}");
        }
        if name.is_empty() {
            bail!("the account needs a name");
        }
        if name.chars().count() > 40 || name.chars().any(char::is_control) {
            bail!("the account name must be at most 40 plain characters");
        }
        let mut list = self.list.lock().unwrap();
        if name.eq_ignore_ascii_case("default") || list.iter().any(|a| a.engine == engine && a.name.eq_ignore_ascii_case(name)) {
            bail!("a {engine} account named {name} already exists");
        }
        let account = Account { id, engine: engine.to_string(), name: name.to_string() };
        let folder = self.dir.join(&account.id);
        std::fs::create_dir_all(&folder).with_context(|| format!("could not create {}", folder.display()))?;
        list.push(account.clone());
        self.save(&list)?;
        Ok(account)
    }

    /// Forget the account and delete its folder, and with it its login.
    pub fn remove(&self, id: &str) -> Result<Account> {
        let mut list = self.list.lock().unwrap();
        let i = list.iter().position(|a| a.id == id).context("no such account")?;
        let account = list.remove(i);
        self.save(&list)?;
        // Ids are `a<hex>`, so this can only be a folder yardd made.
        let _ = std::fs::remove_dir_all(self.dir.join(&account.id));
        Ok(account)
    }

    fn save(&self, list: &[Account]) -> Result<()> {
        let tmp = self.file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(list)?).with_context(|| format!("could not write {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.file).with_context(|| format!("could not write {}", self.file.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::in_use;

    fn s(title: &str, archived: bool) -> (String, bool) {
        (title.to_string(), archived)
    }

    #[test]
    fn in_use_names_three_sessions_then_counts_the_rest() {
        let all = [s("e", false), s("z", true), s("d", false), s("c", false), s("b", false)];
        assert_eq!(in_use(&all), "5 sessions still use this account: \"z\" (archived), \"b\", \"c\" and 2 more. Delete them first.");
        assert_eq!(in_use(&all[..3]), "3 sessions still use this account: \"z\" (archived), \"d\" and \"e\". Delete them first.");
    }

    #[test]
    fn in_use_shortens_a_long_title() {
        let long = "x".repeat(80);
        assert_eq!(in_use(&[s(&long, false)]), format!("The session \"{}…\" still uses this account. Delete it first.", "x".repeat(49)));
    }
}
