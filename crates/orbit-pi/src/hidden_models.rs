//! Models the user has turned off for the composer's model picker (#131).
//!
//! "Removing" a model from the UI deselects it rather than deleting it: the
//! provider's catalog (pi's, or `models.json` for a custom provider) is left
//! alone, so a model the provider adds later still appears, and turning one
//! back on is a switch rather than retyping its id. This is the hide-list
//! answer to the open question in #131 — an override of the provider's list
//! would have dropped models the provider adds later.
//!
//! The set is process-global (like favorites) so the picker and Settings →
//! Models can read and toggle it without threading state through GPUI
//! entities. It is persisted to `~/.orbit-pi/hidden-models.json` as
//! `{ "models": [{"provider", "id"}, …] }`, keyed by provider + id.

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde_json::{json, Value};

use crate::app::ModelEntry;

/// The user's hidden models, identified by provider + id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HiddenModels {
    entries: Vec<(String, String)>,
}

impl HiddenModels {
    fn load_from(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let entries = value
            .get("models")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|entry| {
                        let provider = entry.get("provider").and_then(Value::as_str)?;
                        let id = entry.get("id").and_then(Value::as_str)?;
                        Some((provider.to_string(), id.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self { entries }
    }

    fn persist_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let models: Vec<Value> = self
            .entries
            .iter()
            .map(|(provider, id)| json!({ "provider": provider, "id": id }))
            .collect();
        let _ = std::fs::write(path, json!({ "models": models }).to_string());
    }

    pub fn contains(&self, provider: &str, id: &str) -> bool {
        self.entries.iter().any(|(p, i)| p == provider && i == id)
    }

    /// Show or hide one model.
    pub fn set(&mut self, provider: &str, id: &str, hidden: bool) {
        let position = self
            .entries
            .iter()
            .position(|(p, i)| p == provider && i == id);
        match (position, hidden) {
            (None, true) => self.entries.push((provider.to_string(), id.to_string())),
            (Some(ix), false) => {
                self.entries.remove(ix);
            }
            _ => {}
        }
    }

    /// The picker's catalog: every model the user has not hidden, plus the
    /// active one even when it is hidden, so the picker can always show what
    /// the session is running on.
    pub fn picker_models(
        &self,
        models: &[ModelEntry],
        active_provider: &str,
        active_id: &str,
    ) -> Vec<ModelEntry> {
        models
            .iter()
            .filter(|model| {
                !self.contains(&model.provider, &model.id)
                    || (model.provider == active_provider && model.id == active_id)
            })
            .cloned()
            .collect()
    }

    #[cfg(test)]
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            entries: pairs
                .iter()
                .map(|(p, id)| (p.to_string(), id.to_string()))
                .collect(),
        }
    }
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("hidden-models.json")
}

/// Lazily-loaded global store. Reads and writes lock briefly; the set is tiny.
static STORE: RwLock<Option<HiddenModels>> = RwLock::new(None);

fn with_store<R>(f: impl FnOnce(&mut HiddenModels) -> R) -> R {
    let mut guard = STORE.write().unwrap();
    f(guard.get_or_insert_with(|| {
        // Tests run against the real home directory; they start empty and
        // never read the user's list.
        if cfg!(test) {
            HiddenModels::default()
        } else {
            HiddenModels::load_from(&persist_path())
        }
    }))
}

/// A snapshot of the hidden set for rendering.
pub fn all() -> HiddenModels {
    with_store(|store| store.clone())
}

/// Show or hide a model in the picker and persist the change.
pub fn set(provider: &str, id: &str, hidden: bool) {
    with_store(|store| {
        store.set(provider, id, hidden);
        // Only a running app writes the user's list.
        if !cfg!(test) {
            store.persist_to(&persist_path());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(provider: &str, id: &str) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            name: id.into(),
            provider: provider.into(),
            context_window: None,
            thinking_levels: Vec::new(),
        }
    }

    #[test]
    fn set_hides_and_shows_without_duplicates() {
        let mut hidden = HiddenModels::default();
        hidden.set("openai", "gpt-5", true);
        hidden.set("openai", "gpt-5", true);
        assert!(hidden.contains("openai", "gpt-5"));
        assert!(
            !hidden.contains("anthropic", "gpt-5"),
            "keyed by provider too"
        );
        assert_eq!(hidden.entries.len(), 1);
        hidden.set("openai", "gpt-5", false);
        assert_eq!(hidden, HiddenModels::default());
    }

    #[test]
    fn the_picker_drops_hidden_models_but_keeps_the_active_one() {
        let models = vec![
            entry("openai", "a"),
            entry("openai", "b"),
            entry("anthropic", "c"),
        ];
        let hidden = HiddenModels::from_pairs(&[("openai", "a"), ("anthropic", "c")]);
        let ids = |list: Vec<ModelEntry>| list.into_iter().map(|m| m.id).collect::<Vec<_>>();
        assert_eq!(ids(hidden.picker_models(&models, "openai", "b")), ["b"]);
        assert_eq!(
            ids(hidden.picker_models(&models, "anthropic", "c")),
            ["b", "c"],
            "the session's own model stays pickable"
        );
    }

    #[test]
    fn the_hidden_list_round_trips_and_tolerates_junk() {
        let dir = std::env::temp_dir().join(format!("orbit-hidden-models-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("hidden-models.json");
        assert_eq!(HiddenModels::load_from(&path), HiddenModels::default());

        let hidden = HiddenModels::from_pairs(&[("openai", "a"), ("custom", "x/y")]);
        hidden.persist_to(&path);
        assert_eq!(HiddenModels::load_from(&path), hidden);

        std::fs::write(
            &path,
            r#"{"models":[{"provider":"p"},{"provider":"p","id":"m"},7]}"#,
        )
        .unwrap();
        assert_eq!(
            HiddenModels::load_from(&path),
            HiddenModels::from_pairs(&[("p", "m")])
        );
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(HiddenModels::load_from(&path), HiddenModels::default());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
