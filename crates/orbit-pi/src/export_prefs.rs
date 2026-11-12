//! What an exported transcript includes beyond the visible answer text.
//!
//! Markdown and PDF exports are the visible projection of the conversation:
//! the prompts, the answers, and a note for images they cannot carry. The
//! reasoning and tool trail that pi recorded stays out unless the user opts
//! in from Settings → General, because it is the agent's working noise rather
//! than the conversation.
//!
//! The choice is persisted to `~/.orbit-pi/exports.json`:
//!
//! ```jsonc
//! { "include_thinking_and_tools": true }
//! ```
//!
//! Orbit-owned, and never sent anywhere — export is local by definition.
//! Parsing is lenient: a hand-edited or half-written file falls back to
//! "leave it out", never to including more than the user asked for.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// Export preferences, as shown in Settings → General.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    /// Append each assistant turn's reasoning and tool calls (with the output
    /// pi recorded) to Markdown and PDF exports.
    pub include_thinking_and_tools: bool,
}

impl Prefs {
    /// Read `~/.orbit-pi/exports.json`, defaulting on any failure.
    pub fn load() -> Self {
        Self::load_from(&persist_path())
    }

    pub(crate) fn load_from(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        Self {
            include_thinking_and_tools: value
                .get("include_thinking_and_tools")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// Persist to `~/.orbit-pi/exports.json`. Best-effort: a failed write
    /// leaves the in-memory choice active for this run.
    pub fn persist(self) {
        self.persist_to(&persist_path());
    }

    pub(crate) fn persist_to(self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let body = json!({ "include_thinking_and_tools": self.include_thinking_and_tools });
        let _ = std::fs::write(path, body.to_string());
    }
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("exports.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("orbit-export-prefs-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn missing_or_corrupt_files_mean_omitted() {
        let dir = temp("missing");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            Prefs::load_from(&dir.join("exports.json")),
            Prefs::default()
        );

        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("exports.json");
        for raw in [
            "",
            "{",
            "null",
            "[]",
            r#"{"include_thinking_and_tools":"yes"}"#,
        ] {
            std::fs::write(&path, raw).unwrap();
            assert!(
                !Prefs::load_from(&path).include_thinking_and_tools,
                "{raw:?} must not enable the trail"
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_persisted_choice_round_trips() {
        let dir = temp("roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("exports.json");
        let on = Prefs {
            include_thinking_and_tools: true,
        };
        on.persist_to(&path);
        assert_eq!(Prefs::load_from(&path), on);
        Prefs::default().persist_to(&path);
        assert!(!Prefs::load_from(&path).include_thinking_and_tools);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let dir = temp("unknown");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("exports.json");
        std::fs::write(
            &path,
            r#"{"include_thinking_and_tools":true,"future":"value"}"#,
        )
        .unwrap();
        assert!(Prefs::load_from(&path).include_thinking_and_tools);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
