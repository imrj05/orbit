//! Manual order of sessions inside a project group (#143).
//!
//! The sidebar's default session order is recency — newest activity first —
//! with pinned sessions leading. Dragging a session row (or **Move up** /
//! **Move down**) pins a project group's order instead: the group's sessions
//! are stored in the order the user arranged them, and the row builder sorts a
//! group by that order.
//!
//! A session without an entry (one created after the group was arranged, or a
//! group nobody reordered yet) keeps its recency slot and stays *above* the
//! arranged block, so a new task is still the first thing under its project
//! header. Nothing here touches pi: the order is Orbit's own preference, keyed
//! by the session file's `.jsonl` path.
//!
//! Persisted to `~/.orbit-pi/session-order.json` as
//! `{ "version": 1, "groups": { "<project label>": ["<path>", …] } }`.
//! The project label is the sidebar's group key (the folder's basename — the
//! same key two folders with the same basename already share there).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
#[cfg(not(test))]
use std::sync::RwLock;

use serde_json::{json, Value};

/// Arranged session order, one list per project group label.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionOrder {
    groups: BTreeMap<String, Vec<PathBuf>>,
}

impl SessionOrder {
    /// Read the store, tolerating a missing or corrupt file (the order is a
    /// preference; losing it falls back to recency, never to an error).
    fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(persist_path()) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let mut groups = BTreeMap::new();
        if let Some(entries) = value.get("groups").and_then(Value::as_object) {
            for (label, paths) in entries {
                let Some(paths) = paths.as_array() else {
                    continue;
                };
                let paths: Vec<PathBuf> = paths
                    .iter()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from)
                    .collect();
                if !paths.is_empty() {
                    groups.insert(label.clone(), paths);
                }
            }
        }
        Self { groups }
    }

    fn persist(&self) {
        let path = persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let groups: serde_json::Map<String, Value> = self
            .groups
            .iter()
            .map(|(label, paths)| {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|entry| entry.to_string_lossy().into_owned())
                    .collect();
                (label.clone(), json!(paths))
            })
            .collect();
        let _ = std::fs::write(path, json!({ "version": 1, "groups": groups }).to_string());
    }

    /// Every arranged session path with its position inside its group. The row
    /// builder reads this once per sidebar build, so a rank lookup is O(1).
    pub fn ranks(&self) -> HashMap<PathBuf, usize> {
        let mut ranks = HashMap::new();
        for paths in self.groups.values() {
            for (rank, path) in paths.iter().enumerate() {
                ranks.entry(path.clone()).or_insert(rank);
            }
        }
        ranks
    }

    /// The arranged order of one project group, if it has one.
    #[cfg(test)]
    pub fn group(&self, label: &str) -> Option<&[PathBuf]> {
        self.groups.get(label).map(Vec::as_slice)
    }

    /// Replace a group's arranged order. An empty list drops the group, which
    /// is how a group returns to plain recency.
    pub fn set_group(&mut self, label: &str, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            self.groups.remove(label);
        } else {
            self.groups.insert(label.to_string(), paths);
        }
    }
}

fn persist_path() -> PathBuf {
    crate::app::workspace_store_dir().join("session-order.json")
}

/// Lazily-loaded global store. Reads and writes lock briefly; the data is a
/// few paths per arranged project. A test build reads the per-thread store
/// directory fresh on every call instead (see [`with_store`]).
#[cfg(not(test))]
static STORE: RwLock<Option<SessionOrder>> = RwLock::new(None);

/// Run `f` against the store. Production keeps one cached copy; a test build
/// reads and writes the per-thread isolated store directory instead, so one
/// test's arrangement can never leak into another's expectations.
#[cfg(not(test))]
fn with_store<R>(f: impl FnOnce(&mut SessionOrder) -> R) -> R {
    let mut guard = STORE.write().unwrap();
    f(guard.get_or_insert_with(SessionOrder::load))
}

#[cfg(test)]
fn with_store<R>(f: impl FnOnce(&mut SessionOrder) -> R) -> R {
    let mut store = SessionOrder::load();
    f(&mut store)
}

/// Every arranged session's position inside its group — the rank map the row
/// builder reads once per sidebar build.
pub fn ranks() -> HashMap<PathBuf, usize> {
    with_store(|store| store.ranks())
}

/// Store a group's new order and persist it. Never writes to pi.
pub fn set_group(label: &str, paths: Vec<PathBuf>) {
    with_store(|store| {
        store.set_group(label, paths);
        store.persist();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn order(groups: &[(&str, &[&str])]) -> SessionOrder {
        let mut order = SessionOrder::default();
        for (label, paths) in groups {
            order.set_group(label, paths.iter().map(PathBuf::from).collect::<Vec<_>>());
        }
        order
    }

    #[test]
    fn ranks_are_positions_inside_their_group() {
        let order = order(&[
            ("orbit", &["/s/a.jsonl", "/s/b.jsonl"]),
            ("waku", &["/s/c.jsonl"]),
        ]);
        let ranks = order.ranks();
        assert_eq!(ranks.get(Path::new("/s/a.jsonl")), Some(&0));
        assert_eq!(ranks.get(Path::new("/s/b.jsonl")), Some(&1));
        assert_eq!(ranks.get(Path::new("/s/c.jsonl")), Some(&0));
        assert_eq!(ranks.get(Path::new("/s/absent.jsonl")), None);
    }

    #[test]
    fn setting_a_group_replaces_only_that_group() {
        let mut order = order(&[
            ("orbit", &["/s/a.jsonl", "/s/b.jsonl"]),
            ("waku", &["/s/c.jsonl"]),
        ]);
        order.set_group(
            "orbit",
            vec![PathBuf::from("/s/b.jsonl"), PathBuf::from("/s/a.jsonl")],
        );
        assert_eq!(
            order.group("orbit"),
            Some([PathBuf::from("/s/b.jsonl"), PathBuf::from("/s/a.jsonl")].as_slice())
        );
        assert_eq!(
            order.group("waku"),
            Some([PathBuf::from("/s/c.jsonl")].as_slice())
        );
    }

    #[test]
    fn an_empty_group_returns_to_recency() {
        let mut order = order(&[("orbit", &["/s/a.jsonl"])]);
        order.set_group("orbit", Vec::new());
        assert_eq!(order.group("orbit"), None);
        assert!(order.ranks().is_empty());
    }

    #[test]
    fn the_store_round_trips_through_disk() {
        let order = order(&[
            ("orbit", &["/s/b.jsonl", "/s/a.jsonl"]),
            ("waku", &["/s/c.jsonl"]),
        ]);
        order.persist();

        let reloaded = SessionOrder::load();
        assert_eq!(reloaded, order);
        assert_eq!(
            reloaded.group("orbit"),
            Some([PathBuf::from("/s/b.jsonl"), PathBuf::from("/s/a.jsonl")].as_slice())
        );
    }

    /// Both the write and the read go through the isolated test directory, so
    /// a test never reads or clobbers the user's real arrangement.
    #[test]
    fn the_test_store_never_uses_the_real_orbit_directory() {
        assert_ne!(
            persist_path(),
            crate::platform::home_dir()
                .join(".orbit-pi")
                .join("session-order.json")
        );
    }
}
