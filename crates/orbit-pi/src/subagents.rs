//! `~/.orbit-pi/subagents.json` — the Orbit-owned half of a subagent's
//! configuration, read fresh by the bundled extension on every call.
//!
//! Only the facts a portable agent file cannot hold live here (D-P): whether
//! the agent is enabled, and which workflow mode it is pinned to. The limits
//! are here too because they are Orbit's, not pi's. Everything else about an
//! agent — name, description, model, thinking, tools, prompt — lives in the
//! markdown file.
//!
//! Lenient like [`crate::workflow`] and [`crate::access`]: a missing, partial
//! or corrupt file falls back to the strict defaults, never to "unlimited".

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Hard maxima. Configuration may lower these, never raise them (D-I).
pub(crate) const HARD_MAX_TASKS: u32 = 8;
pub(crate) const HARD_MAX_CONCURRENT: u32 = 4;
pub(crate) const HARD_MAX_CHILDREN: u32 = 24;
pub(crate) const HARD_MAX_TIMEOUT_MINUTES: u32 = 120;

/// Every workflow mode an agent may pin.
pub(crate) const AGENT_MODES: [&str; 4] = ["inherit", "plan", "build", "ask"];

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Config {
    pub enabled: bool,
    pub max_concurrent: u32,
    pub max_children_per_session: u32,
    pub max_tasks_per_run: u32,
    pub timeout_minutes: u32,
    pub project_agents: bool,
    /// Per-agent entries. Unknown keys inside an entry are preserved, so a
    /// newer extension's setting is not lost by an older page.
    pub agents: BTreeMap<String, Value>,
    /// Agents switched off without a file to delete (the builtins).
    pub disabled: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            max_concurrent: 4,
            max_children_per_session: 24,
            max_tasks_per_run: 8,
            timeout_minutes: 30,
            project_agents: false,
            agents: BTreeMap::new(),
            disabled: Vec::new(),
        }
    }
}

impl Config {
    /// Whether the model may pick `name`.
    pub(crate) fn agent_enabled(&self, name: &str) -> bool {
        if self.disabled.iter().any(|disabled| disabled == name) {
            return false;
        }
        self.agents
            .get(name)
            .and_then(|entry| entry.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    /// The mode an agent is pinned to, or `inherit`.
    pub(crate) fn agent_mode(&self, name: &str) -> &str {
        self.agents
            .get(name)
            .and_then(|entry| entry.get("mode"))
            .and_then(Value::as_str)
            .filter(|mode| AGENT_MODES.contains(mode))
            .unwrap_or("inherit")
    }

    pub(crate) fn set_enabled(&mut self, name: &str, enabled: bool) {
        self.disabled.retain(|disabled| disabled != name);
        if !enabled {
            self.disabled.push(name.to_string());
        }
        // Keep an existing entry's own `enabled` truthful, so a reader that
        // only looks at the map agrees with the list.
        if let Some(entry) = self.agents.get_mut(name) {
            if let Some(object) = entry.as_object_mut() {
                object.insert("enabled".into(), Value::Bool(enabled));
            }
        }
    }

    pub(crate) fn set_mode(&mut self, name: &str, mode: &str) {
        let mode = if AGENT_MODES.contains(&mode) {
            mode
        } else {
            "inherit"
        };
        let entry = self
            .agents
            .entry(name.to_string())
            .or_insert_with(|| json!({ "enabled": true, "mode": "inherit" }));
        if let Some(object) = entry.as_object_mut() {
            object.insert("mode".into(), Value::String(mode.to_string()));
        }
    }

    /// Move everything recorded for `from` to `to`, so a renamed agent keeps its
    /// mode and the old name leaves nothing behind.
    pub(crate) fn rename_agent(&mut self, from: &str, to: &str) {
        if from == to {
            return;
        }
        if let Some(entry) = self.agents.remove(from) {
            self.agents.insert(to.to_string(), entry);
        }
        for name in self.disabled.iter_mut().filter(|name| *name == from) {
            *name = to.to_string();
        }
    }

    /// Forget `name` entirely. A deleted agent's file is gone, so a leftover
    /// `disabled` entry would silently apply to a new agent of the same name.
    pub(crate) fn forget_agent(&mut self, name: &str) {
        self.agents.remove(name);
        self.disabled.retain(|disabled| disabled != name);
    }

    /// The JSON document written to disk. Unknown entry fields survive because
    /// `agents` is kept as raw values.
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "enabled": self.enabled,
            "maxConcurrent": self.max_concurrent,
            "maxChildrenPerSession": self.max_children_per_session,
            "maxTasksPerRun": self.max_tasks_per_run,
            "timeoutMinutes": self.timeout_minutes,
            "projectAgents": self.project_agents,
            "disabled": self.disabled,
            "agents": self.agents,
        })
    }

    /// Parse a document, merged over the defaults and clamped to the maxima.
    pub(crate) fn from_value(value: &Value) -> Self {
        let defaults = Self::default();
        let int = |key: &str, fallback: u32, max: u32| {
            value
                .get(key)
                .and_then(Value::as_u64)
                .map(|raw| raw as u32)
                .filter(|raw| *raw > 0)
                .unwrap_or(fallback)
                .min(max)
        };
        let agents = value
            .get("agents")
            .and_then(Value::as_object)
            .map(|entries| {
                entries
                    .iter()
                    .map(|(name, entry)| (name.clone(), entry.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let disabled = value
            .get("disabled")
            .and_then(Value::as_array)
            .map(|names| {
                names
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            enabled: value
                .get("enabled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            max_concurrent: int(
                "maxConcurrent",
                defaults.max_concurrent,
                HARD_MAX_CONCURRENT,
            ),
            max_children_per_session: int(
                "maxChildrenPerSession",
                defaults.max_children_per_session,
                HARD_MAX_CHILDREN,
            ),
            max_tasks_per_run: int("maxTasksPerRun", defaults.max_tasks_per_run, HARD_MAX_TASKS),
            timeout_minutes: int(
                "timeoutMinutes",
                defaults.timeout_minutes,
                HARD_MAX_TIMEOUT_MINUTES,
            ),
            project_agents: value
                .get("projectAgents")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            agents,
            disabled,
        }
    }
}

/// `~/.orbit-pi/subagents.json`.
pub(crate) fn path() -> PathBuf {
    crate::platform::home_dir_opt()
        .unwrap_or_default()
        .join(".orbit-pi")
        .join("subagents.json")
}
/// Read the store; any failure is the default configuration.
pub(crate) fn load() -> Config {
    load_from(&path())
}

pub(crate) fn load_from(file: &Path) -> Config {
    match fs::read_to_string(file) {
        Ok(text) => serde_json::from_str(&text)
            .map(|value: Value| Config::from_value(&value))
            .unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

/// Write the store atomically. A failure is reported to the page, never
/// swallowed: the next delegation would silently use the old settings.
pub(crate) fn save(config: &Config) -> Result<(), String> {
    save_to(&path(), config)
}

pub(crate) fn save_to(file: &Path, config: &Config) -> Result<(), String> {
    let text =
        serde_json::to_string_pretty(&config.to_value()).map_err(|error| error.to_string())?;
    let parent = file
        .parent()
        .ok_or_else(|| "no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(".subagents.{}.tmp", std::process::id()));
    fs::write(&temporary, text).map_err(|error| error.to_string())?;
    fs::rename(&temporary, file).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })
}

/// Status files belong to a single spawned process, never to the home globally.
pub(crate) fn status_path(token: &str) -> PathBuf {
    crate::platform::home_dir_opt()
        .unwrap_or_default()
        .join(".orbit-pi")
        .join(format!("subagents-status-{token}.json"))
}

/// What the bundled extension last decided about the `subagent` name (D-C).
///
/// The extension writes this at every `session_start`, because its only other
/// channel is a toast: the page has to be able to say it in plain words rather
/// than showing a catalog that has no effect on the session.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Status {
    /// Whether Orbit's tool is the one the session can call.
    pub active: bool,
    /// Why it is not, in the extension's own words.
    pub reason: String,
    /// The file that owns the name when Orbit stood down.
    pub owner: Option<String>,
}

/// `None` when no session has reported yet — absence is not a verdict, so the
/// page shows nothing rather than claiming a state nobody checked.
pub(crate) fn load_status(token: &str, session_id: &str) -> Option<Status> {
    load_session_status_from(&status_path(token), session_id)
}

fn load_session_status_from(file: &Path, session_id: &str) -> Option<Status> {
    let text = fs::read_to_string(file).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    if value.get("sessionId").and_then(Value::as_str) != Some(session_id) {
        return None;
    }
    parse_status(&value)
}

/// Reads a report without checking which session wrote it; only tests need
/// that, since the app always asks for its own session's verdict.
#[cfg(test)]
pub(crate) fn load_status_from(file: &Path) -> Option<Status> {
    let text = fs::read_to_string(file).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    parse_status(&value)
}

fn parse_status(value: &Value) -> Option<Status> {
    let active = value.get("active").and_then(Value::as_bool)?;
    Some(Status {
        active,
        reason: value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        owner: value
            .get("owner")
            .and_then(Value::as_str)
            .filter(|owner| !owner.is_empty())
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orbit-subagents-config-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join("subagents.json")
    }

    #[test]
    fn a_missing_or_corrupt_file_is_the_defaults() {
        let file = scratch("missing");
        assert_eq!(load_from(&file), Config::default());
        fs::write(&file, "{not json").unwrap();
        assert_eq!(load_from(&file), Config::default());
    }

    #[test]
    fn partial_config_keeps_defaults_and_clamps_caps() {
        let file = scratch("partial");
        fs::write(
            &file,
            json!({
                "enabled": false,
                "maxConcurrent": 99,
                "maxTasksPerRun": 0,
                "timeoutMinutes": 999,
                "agents": { "worker": { "mode": "build" } }
            })
            .to_string(),
        )
        .unwrap();
        let config = load_from(&file);
        assert!(!config.enabled);
        assert_eq!(config.max_concurrent, HARD_MAX_CONCURRENT);
        assert_eq!(
            config.max_tasks_per_run,
            Config::default().max_tasks_per_run
        );
        assert_eq!(config.timeout_minutes, HARD_MAX_TIMEOUT_MINUTES);
        assert_eq!(config.agent_mode("worker"), "build");
    }

    #[test]
    fn unknown_agent_fields_and_entries_are_preserved() {
        let file = scratch("preserve");
        fs::write(
            &file,
            json!({ "agents": { "scout": { "mode": "plan", "futureKey": [1, 2] } } }).to_string(),
        )
        .unwrap();
        let mut config = load_from(&file);
        config.set_mode("scout", "ask");
        save_to(&file, &config).unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved["agents"]["scout"]["mode"], "ask");
        assert_eq!(saved["agents"]["scout"]["futureKey"], json!([1, 2]));
    }

    #[test]
    fn enabling_and_disabling_agree_between_the_list_and_the_entry() {
        let mut config = Config::default();
        config.set_enabled("scout", false);
        assert!(!config.agent_enabled("scout"));
        assert!(config.disabled.contains(&"scout".to_string()));

        config.set_mode("scout", "plan");
        assert_eq!(config.agent_mode("scout"), "plan");
        assert!(
            !config.agent_enabled("scout"),
            "a mode write does not re-enable"
        );

        config.set_enabled("scout", true);
        assert!(config.agent_enabled("scout"));
        assert!(!config.disabled.contains(&"scout".to_string()));
        // A reader that only looks at the map agrees with the list.
        assert_eq!(config.agents["scout"]["enabled"], Value::Bool(true));
    }

    #[test]
    fn an_unknown_mode_falls_back_to_inherit() {
        let mut config = Config::default();
        config.set_mode("scout", "sideways");
        assert_eq!(config.agent_mode("scout"), "inherit");
    }

    #[test]
    fn renaming_and_forgetting_agents_leave_no_stale_entries() {
        let file = scratch("rename");
        let mut config = Config::default();
        config.set_mode("scout", "plan");
        config.set_enabled("scout", false);

        config.rename_agent("scout", "scout-2");
        assert!(!config.agents.contains_key("scout"));
        assert_eq!(config.agent_mode("scout-2"), "plan");
        assert!(!config.agent_enabled("scout-2"));

        save_to(&file, &config).unwrap();
        config.forget_agent("scout-2");
        assert!(!config.agents.contains_key("scout-2"));
        assert!(
            config.agent_enabled("scout-2"),
            "a re-created agent is enabled"
        );
        save_to(&file, &config).unwrap();
        let saved: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved["disabled"], json!([]));
        assert_eq!(saved["agents"], json!({}));
    }

    #[test]
    fn the_document_round_trips_through_disk() {
        let file = scratch("roundtrip");
        let mut config = Config {
            enabled: false,
            project_agents: true,
            ..Config::default()
        };
        config.set_enabled("worker", false);
        config.set_mode("worker", "plan");
        save_to(&file, &config).unwrap();
        assert_eq!(load_from(&file), config);
        // The written shape is the one the extension reads.
        let saved: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved["maxConcurrent"], json!(4));
        assert_eq!(saved["agents"]["worker"]["mode"], "plan");
    }

    #[test]
    fn status_is_scoped_to_the_live_session() {
        let file = scratch("status-session");
        fs::write(
            &file,
            json!({"sessionId": "one", "active": false}).to_string(),
        )
        .unwrap();
        assert!(load_session_status_from(&file, "one").is_some());
        assert_eq!(load_session_status_from(&file, "two"), None);
        assert_ne!(status_path("process-one"), status_path("process-two"));
    }

    #[test]
    fn a_missing_or_corrupt_status_is_no_verdict() {
        let file = scratch("status-missing").with_file_name("subagents-status.json");
        assert_eq!(load_status_from(&file), None, "absence is not inactive");
        fs::write(&file, "{not json").unwrap();
        assert_eq!(load_status_from(&file), None);
        fs::write(
            &file,
            json!({ "reason": "owned by another extension" }).to_string(),
        )
        .unwrap();
        assert_eq!(
            load_status_from(&file),
            None,
            "a report without `active` says nothing"
        );
    }

    #[test]
    fn the_extension_s_report_reaches_the_page() {
        let file = scratch("status").with_file_name("subagents-status.json");
        fs::write(
            &file,
            json!({
                "active": false,
                "tool": "subagent",
                "reason": "owned by another extension",
                "owner": "/home/me/.pi/agent/extensions/pi-subagents/index.js",
                "updatedAt": 1767225600000i64
            })
            .to_string(),
        )
        .unwrap();
        let status = load_status_from(&file).expect("a report is a verdict");
        assert!(!status.active);
        assert_eq!(status.reason, "owned by another extension");
        assert_eq!(
            status.owner.as_deref(),
            Some("/home/me/.pi/agent/extensions/pi-subagents/index.js")
        );

        // Active reports clear an old stand-down, and carry no owner.
        fs::write(
            &file,
            json!({ "active": true, "tool": "subagent", "reason": "available", "owner": null })
                .to_string(),
        )
        .unwrap();
        let status = load_status_from(&file).expect("a report is a verdict");
        assert!(status.active);
        assert_eq!(status.owner, None);
    }
}
