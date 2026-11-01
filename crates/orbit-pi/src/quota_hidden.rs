//! Hidden quota providers — the top-bar usage popover's user-curated
//! shortlist of accounts the user does not want to see there.
//!
//! The set lives on the app (like [`crate::session_defaults::SessionDefault`])
//! so the popover reads it without a lock on the render path and tests can
//! seed it directly. It is persisted to
//! `~/.orbit-pi/hidden-quota-providers.json` as `{ "providers": ["cursor-acp",
//! …] }`, keyed by account id: provider ids that resolve one account (`ollama`
//! / `ollama-cloud`) collapse to the shared id, so an entry written before
//! that collapse still hides — and can still be cleared from — the shared
//! card. Orbit-owned: this only hides the provider from the top-bar popover,
//! never from Settings → Providers or from pi itself.
//!
//! A hidden provider stays reachable: the popover keeps a collapsed "Hidden"
//! group so a mistoggle costs one click, not a hunt through the config.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::quota::canonical_provider;

/// The provider ids hidden from the top-bar usage popover.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HiddenProviders {
    entries: Vec<String>,
}

impl HiddenProviders {
    /// Read the persisted set, defaulting on any failure so a hand-edited or
    /// half-written file can only mean "nothing hidden".
    pub fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(persist_path()) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let mut entries = Vec::new();
        for provider in value
            .get("providers")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(provider) = provider.as_str().filter(|provider| !provider.is_empty()) {
                push_account(&mut entries, provider);
            }
        }
        Self { entries }
    }

    /// Write the set, best-effort (a failed write never blocks the UI).
    pub fn persist(&self) {
        let path = persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, json!({ "providers": self.entries }).to_string());
    }

    /// Whether a provider is hidden from the popover.
    pub fn contains(&self, provider: &str) -> bool {
        let account = canonical_provider(provider);
        self.entries.iter().any(|entry| entry == account)
    }

    /// Flip a provider's hidden state; returns the new hidden state.
    pub fn toggle(&mut self, provider: &str) -> bool {
        let account = canonical_provider(provider);
        match self.entries.iter().position(|entry| entry == account) {
            Some(ix) => {
                self.entries.remove(ix);
                false
            }
            None => {
                self.entries.push(account.to_owned());
                true
            }
        }
    }

    #[cfg(test)]
    pub fn from_providers(providers: &[&str]) -> Self {
        let mut entries = Vec::new();
        for provider in providers {
            push_account(&mut entries, provider);
        }
        Self { entries }
    }
}

/// Append `provider`'s account id unless it is already listed. Sibling ids
/// resolve one account, so the set holds one entry per account and hiding it
/// survives a model switch between the siblings.
fn push_account(entries: &mut Vec<String>, provider: &str) {
    let account = canonical_provider(provider);
    if !entries.iter().any(|entry| entry == account) {
        entries.push(account.to_owned());
    }
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("hidden-quota-providers.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_adds_then_removes() {
        let mut hidden = HiddenProviders::default();
        assert!(!hidden.contains("cursor-acp"));
        assert!(hidden.toggle("cursor-acp"));
        assert!(hidden.contains("cursor-acp"));
        assert!(!hidden.toggle("cursor-acp"));
        assert!(!hidden.contains("cursor-acp"));
    }

    #[test]
    fn identity_is_the_provider_id() {
        let hidden = HiddenProviders::from_providers(&["cursor-acp"]);
        assert!(hidden.contains("cursor-acp"));
        assert!(!hidden.contains("deepseek"));
    }

    #[test]
    fn distinct_providers_are_independent() {
        let mut hidden = HiddenProviders::from_providers(&["cursor-acp"]);
        assert!(hidden.toggle("deepseek"));
        assert!(hidden.contains("cursor-acp"));
        assert!(hidden.contains("deepseek"));
    }

    #[test]
    fn a_legacy_sibling_entry_hides_the_shared_account() {
        // Written before the sibling collapse, which stored the clicked id.
        let mut hidden = HiddenProviders::from_providers(&["ollama-cloud"]);
        assert!(hidden.contains("ollama"), "the account must stay hidden");
        assert!(!hidden.toggle("ollama"), "showing it must clear the entry");
        assert!(!hidden.contains("ollama-cloud"));
    }

    #[test]
    fn one_account_is_one_entry() {
        let hidden = HiddenProviders::from_providers(&["ollama", "ollama-cloud"]);
        assert_eq!(hidden.entries.len(), 1);
        assert!(hidden.contains("ollama"));
    }
}
