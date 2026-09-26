//! Per-mode session defaults — the model and thinking level a session starts
//! on for each workflow mode.
//!
//! Orbit sessions run as **Plan**, **Build**, or **Ask** (see
//! [`crate::workflow`]). This module owns the choice of starting model and
//! thinking level for each mode, persisted to
//! `~/.orbit-pi/session-defaults.json`:
//!
//! ```jsonc
//! {
//!   "plan":  { "provider": "anthropic", "model_id": "claude-sonnet-4-5", "thinking": "high" },
//!   "build": { "provider": null,        "model_id": null,               "thinking": null },
//!   "ask":   { "provider": "openai",    "model_id": "gpt-5.6",          "thinking": "low" }
//! }
//! ```
//!
//! An empty slot means "leave pi's own startup default alone" — Orbit never
//! forces a model the user did not pick. The file is Orbit-owned; it is
//! deliberately *not* pi's `~/.pi/agent/settings.json`, which is global to the
//! machine and would silently change terminal `pi` sessions.
//!
//! Two apply paths use this config (see `OrbitApp::apply_mode_defaults`):
//! [`ModeDefault::cli_args`] for the process that hosts a session's first
//! turn, and `set_model` / `set_thinking_level` RPC for every later session
//! born inside a live process. Parsing is lenient so a hand-edited or
//! half-written file can only fall back to "all slots unset".

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::workflow::WorkflowMode;

/// One workflow mode's default. Every field is optional: an unset `model_id`
/// is an empty slot (thinking is ignored without a model).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct ModeDefault {
    /// pi provider id (`anthropic`). `None` lets pi fuzzy-match the model.
    pub provider: Option<String>,
    /// pi model id (`claude-sonnet-4-5`). `None` means the slot is unset.
    pub model_id: Option<String>,
    /// Thinking level (`off` … `max`). Validated against the model's reported
    /// levels at apply time, never at parse time.
    pub thinking: Option<String>,
}

impl ModeDefault {
    /// Whether this slot asks for nothing at all.
    pub fn is_unset(&self) -> bool {
        self.model_id.is_none() && self.thinking.is_none()
    }

    /// The extra `pi` CLI args this slot implies at process spawn — empty when
    /// unset. `--thinking` is independent of the `--model` suffix so the two
    /// stay readable; pi clamps the level to the model's capabilities.
    pub fn cli_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(id) = self.model_id.as_deref().filter(|id| !id.is_empty()) {
            match self
                .provider
                .as_deref()
                .filter(|provider| !provider.is_empty())
            {
                Some(provider) => {
                    args.push("--provider".into());
                    args.push(provider.to_string());
                    args.push("--model".into());
                    args.push(format!("{provider}/{id}"));
                }
                None => {
                    args.push("--model".into());
                    args.push(id.to_string());
                }
            }
            // Thinking only means something alongside a model; a bare level
            // would be applied to whatever pi happens to start with.
            if let Some(level) = self.thinking.as_deref().filter(|level| !level.is_empty()) {
                args.push("--thinking".into());
                args.push(level.to_string());
            }
        }
        args
    }
}

/// The three mode slots, persisted as one JSON object.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct SessionDefaults {
    pub plan: ModeDefault,
    pub build: ModeDefault,
    pub ask: ModeDefault,
}

impl SessionDefaults {
    /// Read `~/.orbit-pi/session-defaults.json`, defaulting on any failure.
    pub fn load() -> Self {
        Self::load_at(&store_path())
    }

    /// Persist to `~/.orbit-pi/session-defaults.json`. Best-effort: the app
    /// simply keeps the previous choice if the write fails.
    pub fn persist(&self) {
        self.write_at(&store_path());
    }

    /// The slot for a workflow mode.
    pub fn for_mode(&self, mode: WorkflowMode) -> &ModeDefault {
        match mode {
            WorkflowMode::Plan => &self.plan,
            WorkflowMode::Build => &self.build,
            WorkflowMode::Ask => &self.ask,
        }
    }

    /// Replace a mode's slot.
    pub fn set(&mut self, mode: WorkflowMode, slot: ModeDefault) {
        match mode {
            WorkflowMode::Plan => self.plan = slot,
            WorkflowMode::Build => self.build = slot,
            WorkflowMode::Ask => self.ask = slot,
        }
    }

    /// Parse the persisted shape. Unknown keys are ignored and a malformed
    /// file loads as "all unset" rather than erroring.
    pub fn from_json(raw: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Self::default();
        };
        Self {
            plan: parse_slot(value.get("plan")),
            build: parse_slot(value.get("build")),
            ask: parse_slot(value.get("ask")),
        }
    }

    /// The wire shape written to disk.
    pub fn to_json(&self) -> String {
        let value = json!({
            "plan": slot_json(&self.plan),
            "build": slot_json(&self.build),
            "ask": slot_json(&self.ask),
        });
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string())
    }

    fn load_at(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .map(|raw| Self::from_json(&raw))
            .unwrap_or_default()
    }

    fn write_at(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Temp-file + rename so a concurrent reader (or a crash) never sees a
        // partial file — the same convention `workflow.rs` uses.
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, self.to_json()).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

/// `~/.orbit-pi/session-defaults.json`.
pub(crate) fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("session-defaults.json")
}

fn parse_slot(value: Option<&Value>) -> ModeDefault {
    let Some(object) = value.and_then(Value::as_object) else {
        return ModeDefault::default();
    };
    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let model_id = text("model_id").or_else(|| text("modelId"));
    // A thinking level without a model would be applied to pi's own startup
    // model; treat the slot as empty instead.
    let thinking = model_id.as_ref().and_then(|_| text("thinking"));
    ModeDefault {
        provider: text("provider"),
        model_id,
        thinking,
    }
}

fn slot_json(slot: &ModeDefault) -> Value {
    json!({
        "provider": slot.provider,
        "model_id": slot.model_id,
        "thinking": slot.thinking,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        let unique = format!(
            "orbit-session-defaults-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir()
            .join(unique)
            .join("session-defaults.json")
    }

    #[test]
    fn empty_slots_emit_no_args() {
        assert!(ModeDefault::default().cli_args().is_empty());
        assert!(ModeDefault::default().is_unset());
    }

    #[test]
    fn model_with_thinking_emits_provider_model_and_thinking() {
        let slot = ModeDefault {
            provider: Some("anthropic".into()),
            model_id: Some("claude-sonnet-4-5".into()),
            thinking: Some("high".into()),
        };
        assert_eq!(
            slot.cli_args(),
            vec![
                "--provider",
                "anthropic",
                "--model",
                "anthropic/claude-sonnet-4-5",
                "--thinking",
                "high",
            ]
        );
    }

    #[test]
    fn provider_less_model_emits_just_the_model() {
        let slot = ModeDefault {
            model_id: Some("claude-sonnet-4-5".into()),
            ..Default::default()
        };
        assert_eq!(slot.cli_args(), vec!["--model", "claude-sonnet-4-5"]);
    }

    #[test]
    fn thinking_without_a_model_is_dropped_at_parse_time() {
        let parsed = SessionDefaults::from_json(r#"{"plan":{"thinking":"high"}}"#);
        assert!(parsed.plan.is_unset());
        assert!(parsed.plan.cli_args().is_empty());
    }

    #[test]
    fn json_round_trips_every_slot() {
        let defaults = SessionDefaults {
            plan: ModeDefault {
                provider: Some("anthropic".into()),
                model_id: Some("claude-sonnet-4-5".into()),
                thinking: Some("high".into()),
            },
            build: ModeDefault::default(),
            ask: ModeDefault {
                provider: Some("openai".into()),
                model_id: Some("gpt-5.6".into()),
                thinking: Some("low".into()),
            },
        };
        assert_eq!(SessionDefaults::from_json(&defaults.to_json()), defaults);
    }

    #[test]
    fn malformed_or_unknown_shapes_load_as_unset() {
        assert_eq!(
            SessionDefaults::from_json("not json"),
            SessionDefaults::default()
        );
        assert_eq!(SessionDefaults::from_json("{}"), SessionDefaults::default());
        assert_eq!(
            SessionDefaults::from_json(r#"{"yolo":{"provider":"x","model_id":"y"}}"#),
            SessionDefaults::default()
        );
        // A slot of the wrong type is simply empty.
        assert_eq!(
            SessionDefaults::from_json(r#"{"plan":"nope"}"#),
            SessionDefaults::default()
        );
    }

    #[test]
    fn for_mode_and_set_address_the_right_slot() {
        let mut defaults = SessionDefaults::default();
        let plan = ModeDefault {
            provider: Some("anthropic".into()),
            model_id: Some("m".into()),
            thinking: None,
        };
        defaults.set(WorkflowMode::Plan, plan.clone());
        assert_eq!(*defaults.for_mode(WorkflowMode::Plan), plan);
        assert!(defaults.for_mode(WorkflowMode::Build).is_unset());
        assert!(defaults.for_mode(WorkflowMode::Ask).is_unset());
    }

    #[test]
    fn store_round_trips_on_disk() {
        let path = temp_path("store");
        let defaults = SessionDefaults {
            build: ModeDefault {
                provider: Some("ollama".into()),
                model_id: Some("glm-5.3".into()),
                thinking: Some("low".into()),
            },
            ..Default::default()
        };
        defaults.write_at(&path);
        assert_eq!(SessionDefaults::load_at(&path), defaults);
        // A malformed file degrades to empty, never an error.
        std::fs::write(&path, "{ broken").unwrap();
        assert_eq!(SessionDefaults::load_at(&path), SessionDefaults::default());
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn store_path_is_orbit_owned() {
        // Deliberately not `~/.pi/agent/settings.json` — see the module docs.
        assert!(store_path().ends_with(".orbit-pi/session-defaults.json"));
    }
}
