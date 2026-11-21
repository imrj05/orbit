//! Per-workspace project-trust decisions.
//!
//! Pi's `--approve` flag loads project-local settings, extensions, skills and
//! MCP servers. Orbit keeps that choice separate from tool-call approval and
//! persists it here. Missing, malformed or unreadable state is always
//! untrusted; only an explicit `true` entry enables `--approve`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::remote_hosts::RemoteHost;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TrustDecision {
    #[default]
    Unknown,
    Trusted,
    Untrusted,
}

impl TrustDecision {
    pub(crate) fn is_trusted(self) -> bool {
        self == Self::Trusted
    }
}

#[derive(Serialize, Deserialize)]
struct TrustDocument {
    version: u32,
    #[serde(default)]
    decisions: BTreeMap<String, bool>,
}

pub(crate) struct ProjectTrust {
    path: PathBuf,
    decisions: BTreeMap<String, bool>,
    load_error: Option<String>,
    // Application tests build many windows against one process. Keep their
    // existing spawn behavior isolated from the user's real trust store; the
    // store's own tests use `load_from`, which always defaults to Unknown.
    #[cfg(test)]
    default_trusted: bool,
}

impl ProjectTrust {
    pub(crate) fn load() -> Self {
        #[cfg(test)]
        {
            Self {
                path: std::env::temp_dir().join(format!(
                    "orbit-project-trust-test-{}-{}.json",
                    std::process::id(),
                    NEXT_TEST_STORE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                )),
                decisions: BTreeMap::new(),
                load_error: None,
                default_trusted: true,
            }
        }
        #[cfg(not(test))]
        {
            Self::load_from(default_path())
        }
    }

    pub(crate) fn load_from(path: PathBuf) -> Self {
        let (decisions, load_error) = read(&path);
        Self {
            path,
            decisions,
            load_error,
            #[cfg(test)]
            default_trusted: false,
        }
    }

    pub(crate) fn decision(&self, key: &str) -> TrustDecision {
        match self.decisions.get(key) {
            Some(true) => TrustDecision::Trusted,
            Some(false) => TrustDecision::Untrusted,
            None => {
                #[cfg(test)]
                if self.default_trusted {
                    return TrustDecision::Trusted;
                }
                TrustDecision::Unknown
            }
        }
    }

    pub(crate) fn set(&mut self, key: &str, trusted: bool) -> Result<(), String> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        let previous = self.decisions.insert(key.to_string(), trusted);
        if let Err(error) = self.persist() {
            match previous {
                Some(value) => {
                    self.decisions.insert(key.to_string(), value);
                }
                None => {
                    self.decisions.remove(key);
                }
            }
            return Err(error);
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| "The project-trust store has no parent directory".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let document = TrustDocument {
            version: 1,
            decisions: self.decisions.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
        let temporary = self
            .path
            .with_file_name(format!(".project-trust.{}.tmp", std::process::id()));
        let write = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)
            .and_then(|mut file| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt as _;
                    file.set_permissions(fs::Permissions::from_mode(0o600))?;
                }
                file.write_all(&bytes)?;
                file.sync_all()
            });
        if let Err(error) = write {
            let _ = fs::remove_file(&temporary);
            return Err(error.to_string());
        }
        fs::rename(&temporary, &self.path).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            error.to_string()
        })
    }
}

#[cfg(test)]
static NEXT_TEST_STORE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(not(test))]
pub(crate) fn default_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("project-trust.json")
}

pub(crate) fn local_key(workspace: &Path) -> String {
    format!(
        "local:{}",
        crate::sessions::canonical_workspace_path(workspace).to_string_lossy()
    )
}

pub(crate) fn remote_key(host: &RemoteHost) -> String {
    format!("remote:{}:{}", host.account_key(), host.dir)
}

fn read(path: &Path) -> (BTreeMap<String, bool>, Option<String>) {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (BTreeMap::new(), None)
        }
        Err(error) => return (BTreeMap::new(), Some(error.to_string())),
    };
    match serde_json::from_slice::<TrustDocument>(&bytes) {
        Ok(document) if document.version == 1 => (document.decisions, None),
        Ok(_) => (
            BTreeMap::new(),
            Some("Unsupported project-trust store version".to_string()),
        ),
        Err(error) => (BTreeMap::new(), Some(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempStore(PathBuf);

    impl TempStore {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "orbit-project-trust-unit-{}-{}.json",
                std::process::id(),
                NEXT_TEST_STORE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            Self(path)
        }
    }

    impl Drop for TempStore {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn missing_state_is_unknown_and_decisions_round_trip() {
        let temp = TempStore::new();
        let mut store = ProjectTrust::load_from(temp.0.clone());
        let key = local_key(Path::new("/tmp/example"));
        assert_eq!(store.decision(&key), TrustDecision::Unknown);
        store.set(&key, false).unwrap();
        assert_eq!(store.decision(&key), TrustDecision::Untrusted);
        store.set(&key, true).unwrap();
        let reloaded = ProjectTrust::load_from(temp.0.clone());
        assert_eq!(reloaded.decision(&key), TrustDecision::Trusted);
    }

    #[test]
    fn malformed_state_fails_closed_and_is_not_overwritten() {
        let temp = TempStore::new();
        fs::write(&temp.0, b"not json").unwrap();
        let mut store = ProjectTrust::load_from(temp.0.clone());
        assert_eq!(store.decision("anything"), TrustDecision::Unknown);
        assert!(store.set("anything", true).is_err());
        assert_eq!(fs::read(&temp.0).unwrap(), b"not json");
    }

    #[test]
    fn remote_identity_includes_account_and_directory() {
        let host = RemoteHost {
            name: "prod".into(),
            ssh: "example.test".into(),
            user: Some("deploy".into()),
            port: Some(2222),
            dir: "/srv/app".into(),
            enabled: true,
            local_dir: None,
        };
        let key = remote_key(&host);
        assert!(key.contains("deploy@example.test"));
        assert!(key.contains("2222"));
        assert!(key.ends_with("/srv/app"));
    }
}
