//! Pseudonymous installation identity and persisted opt-out state.
//!
//! The installation id is a random v4 UUID generated on first run. It is not
//! derived from hardware, account, or machine data. The session id is a fresh
//! UUID per launch. Both are UUIDs with no embedded information; the
//! installation id is *pseudonymous*, not legally anonymous.

use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::{json, Value};
use uuid::Uuid;

/// The persisted slice of analytics state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StoredState {
    pub installation_id: Option<String>,
    /// The user's explicit opt-out choice, when they have made one.
    pub enabled: Option<bool>,
}

/// Persistence seam so the client can be tested without touching disk.
pub trait StateStore: Send + Sync {
    fn load(&self) -> StoredState;
    fn save(&self, state: &StoredState);
}

/// In-memory store for tests.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    state: Mutex<StoredState>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_state(state: StoredState) -> Self {
        Self {
            state: Mutex::new(state),
        }
    }
}

impl StateStore for InMemoryStore {
    fn load(&self) -> StoredState {
        self.state.lock().unwrap().clone()
    }

    fn save(&self, state: &StoredState) {
        *self.state.lock().unwrap() = state.clone();
    }
}

/// JSON file store, written atomically via a temporary file + rename so a crash
/// can never leave a half-written identity.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl StateStore for FileStore {
    fn load(&self) -> StoredState {
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return StoredState::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return StoredState::default();
        };
        StoredState {
            installation_id: value
                .get("installation_id")
                .and_then(Value::as_str)
                .filter(|id| is_valid_installation_id(id))
                .map(str::to_string),
            enabled: value.get("enabled").and_then(Value::as_bool),
        }
    }

    fn save(&self, state: &StoredState) {
        let Some(parent) = self.path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let mut value = serde_json::Map::new();
        if let Some(id) = &state.installation_id {
            value.insert("installation_id".into(), json!(id));
        }
        if let Some(enabled) = state.enabled {
            value.insert("enabled".into(), json!(enabled));
        }
        let temporary = self.path.with_extension("json.tmp");
        if std::fs::write(&temporary, Value::Object(value).to_string()).is_ok() {
            let _ = std::fs::rename(&temporary, &self.path);
        } else {
            let _ = std::fs::remove_file(&temporary);
        }
    }
}

/// Generate a fresh pseudonymous installation id.
pub fn generate_installation_id() -> String {
    Uuid::new_v4().to_string()
}

/// Generate a fresh per-launch session id.
pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}

/// A stored installation id is only trusted if it is a plain UUID; anything
/// else is regenerated rather than propagated.
pub fn is_valid_installation_id(value: &str) -> bool {
    Uuid::parse_str(value.trim()).is_ok()
}

/// Load the installation id, generating and persisting one on first run.
pub fn load_or_create_installation_id(store: &dyn StateStore, current: StoredState) -> String {
    if let Some(id) = current.installation_id.as_deref() {
        if is_valid_installation_id(id) {
            return id.to_string();
        }
    }
    let id = generate_installation_id();
    store.save(&StoredState {
        installation_id: Some(id.clone()),
        enabled: current.enabled,
    });
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_uuid_v4() {
        let id = generate_installation_id();
        assert!(Uuid::parse_str(&id).is_ok());
        assert_ne!(generate_installation_id(), generate_session_id());
    }

    #[test]
    fn file_store_round_trips() {
        let dir = std::env::temp_dir().join(format!("orbit-analytics-{}", Uuid::new_v4()));
        let path = dir.join("analytics.json");
        let store = FileStore::new(&path);
        let state = StoredState {
            installation_id: Some("550e8400-e29b-41d4-a716-446655440000".into()),
            enabled: Some(false),
        };
        store.save(&state);
        assert_eq!(store.load(), state);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_stored_id_is_discarded() {
        let store = InMemoryStore::with_state(StoredState {
            installation_id: Some("not-a-uuid".into()),
            enabled: Some(true),
        });
        let state = store.load();
        let id = load_or_create_installation_id(&store, state);
        assert!(is_valid_installation_id(&id));
        assert_ne!(id, "not-a-uuid");
    }

    #[test]
    fn valid_stored_id_survives() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        let store = InMemoryStore::with_state(StoredState {
            installation_id: Some(id.into()),
            enabled: None,
        });
        let state = store.load();
        assert_eq!(load_or_create_installation_id(&store, state), id);
    }
}
