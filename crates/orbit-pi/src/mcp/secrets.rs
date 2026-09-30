//! Orbit's MCP secret store.
//!
//! Pi expands `${NAME}` references in `mcp.json` from its process
//! environment. Orbit keeps the values behind those references in a
//! permissions-restricted file (`~/.orbit-pi/mcp-secrets.json`, mode 0600 on
//! Unix) and injects them into every Pi spawn and every `pi mcp list` probe,
//! so the configuration file itself never needs a raw credential.
//!
//! The trust model is deliberately the same as Pi's own credential files
//! under `~/.pi/agent/`: local, user-scoped, never logged. Everything Orbit
//! displays or logs passes through [`McpSecrets::redact`]. A keychain-backed
//! store can replace this module behind the same API later.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::{config, McpError};

/// The secrets Orbit tracks for MCP `${NAME}` references.
pub(crate) struct McpSecrets {
    path: PathBuf,
    values: BTreeMap<String, String>,
}

impl McpSecrets {
    /// Load from an explicit path — the seam the manager and tests drive.
    pub(crate) fn load_from(path: PathBuf) -> Self {
        let values = read(&path);
        Self { path, values }
    }

    #[cfg(test)]
    pub(crate) fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Store (or replace) one secret and persist immediately. A failed write
    /// leaves the in-memory value in place and reports the error; the caller
    /// must not write a config that references it then.
    pub(crate) fn set(&mut self, name: &str, value: &str) -> Result<(), McpError> {
        self.values.insert(name.to_string(), value.to_string());
        self.persist()
    }

    /// Remove one secret, if present (test-facing; production pruning goes
    /// through [`Self::prune_unreferenced`]).
    #[cfg(test)]
    pub(crate) fn remove(&mut self, name: &str) -> Result<(), McpError> {
        if self.values.remove(name).is_none() {
            return Ok(());
        }
        self.persist()
    }

    /// Every value, for injection into a child process environment. The key
    /// is the `${NAME}` reference's name.
    pub(crate) fn env(&self) -> Vec<(String, String)> {
        self.values
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    /// Mask every known secret value found in `text`, plus any `Bearer`
    /// credential. Used on Pi-reported errors and connection stderr before
    /// they reach the UI or a log.
    pub(crate) fn redact(&self, text: &str) -> String {
        let known: Vec<String> = self.values.values().cloned().collect();
        config::scrub_error(text, &known)
    }

    /// Drop Orbit-generated secrets that no configured server references
    /// anymore. Names outside Orbit's `MCP_` namespace are left alone even
    /// when unreferenced — they may belong to the user's shell environment.
    pub(crate) fn prune_unreferenced(
        &mut self,
        referenced: &std::collections::BTreeSet<String>,
    ) -> Result<(), McpError> {
        let stale: Vec<String> = self
            .values
            .keys()
            .filter(|name| name.starts_with("MCP_") && !referenced.contains(*name))
            .cloned()
            .collect();
        if stale.is_empty() {
            return Ok(());
        }
        for name in stale {
            self.values.remove(&name);
        }
        self.persist()
    }

    fn persist(&self) -> Result<(), McpError> {
        let mut secrets = Map::new();
        for (name, value) in &self.values {
            secrets.insert(name.clone(), Value::String(value.clone()));
        }
        let mut root = Map::new();
        root.insert("version".into(), Value::Number(1.into()));
        root.insert("secrets".into(), Value::Object(secrets));
        let doc = Value::Object(root);
        let text = config::serialize(&doc, b"  ")?;
        config::write_atomic(&self.path, &text)?;
        restrict_permissions(&self.path);
        Ok(())
    }
}

/// The store's location under a given home: `<home>/.orbit-pi/mcp-secrets.json`.
pub(crate) fn default_path_for(home: &Path) -> PathBuf {
    home.join(".orbit-pi").join("mcp-secrets.json")
}

/// Read the store; any failure (missing, malformed, wrong shape) degrades to
/// an empty store rather than blocking the MCP page.
fn read(path: &Path) -> BTreeMap<String, String> {
    let Ok(text) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(doc) = serde_json::from_str::<Value>(&text) else {
        return BTreeMap::new();
    };
    let Some(secrets) = doc.get("secrets").and_then(Value::as_object) else {
        return BTreeMap::new();
    };
    secrets
        .iter()
        .filter_map(|(name, value)| {
            value
                .as_str()
                .map(|value| (name.clone(), value.to_string()))
        })
        .collect()
}

/// Owner-only read/write on Unix. On Windows the file lives in the user
/// profile, which inherits the user's ACLs.
fn restrict_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn temp_store(tag: &str) -> (PathBuf, McpSecrets) {
        let dir = std::env::temp_dir().join(format!(
            "orbit-mcp-secrets-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mcp-secrets.json");
        (path.clone(), McpSecrets::load_from(path))
    }

    #[test]
    fn set_round_trips_through_disk() {
        let (path, mut secrets) = temp_store("roundtrip");
        secrets.set("MCP_GITHUB_TOKEN", "ghp_secret").unwrap();
        let reloaded = McpSecrets::load_from(path);
        assert_eq!(reloaded.get("MCP_GITHUB_TOKEN"), Some("ghp_secret"));
        assert_eq!(
            reloaded.env(),
            vec![("MCP_GITHUB_TOKEN".to_string(), "ghp_secret".to_string())]
        );
    }

    #[test]
    fn remove_drops_the_entry() {
        let (path, mut secrets) = temp_store("remove");
        secrets.set("A", "1").unwrap();
        secrets.remove("A").unwrap();
        assert!(!McpSecrets::load_from(path).contains("A"));
    }

    #[test]
    fn redaction_masks_known_values_and_bearer_credentials() {
        let (_path, mut secrets) = temp_store("redact");
        secrets.set("MCP_TOKEN", "supersecretvalue").unwrap();
        let text = secrets.redact("failed with supersecretvalue and Bearer abcdefghijklmnop");
        assert!(!text.contains("supersecretvalue"));
        assert!(!text.contains("abcdefghijklmnop"));
    }

    #[test]
    fn pruning_only_touches_orbit_names() {
        let (path, mut secrets) = temp_store("prune");
        secrets.set("MCP_GENERATED", "a").unwrap();
        secrets.set("USER_PROVIDED", "b").unwrap();
        let referenced = BTreeSet::new();
        secrets.prune_unreferenced(&referenced).unwrap();
        let reloaded = McpSecrets::load_from(path);
        assert!(!reloaded.contains("MCP_GENERATED"));
        assert!(reloaded.contains("USER_PROVIDED"));
    }

    #[test]
    fn a_malformed_store_degrades_to_empty() {
        let (path, _) = temp_store("malformed");
        fs::write(&path, "not json").unwrap();
        assert!(McpSecrets::load_from(path).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn the_store_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let (path, mut secrets) = temp_store("perms");
        secrets.set("A", "1").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
