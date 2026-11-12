//! Access mode — how much latitude the agent has, and where it is persisted.
//!
//! Orbit ships a bundled pi extension (`contrib/orbit-guard-extension/`) that
//! intercepts every `tool_call` and confirms the ones the active mode does not
//! auto-approve. The mode is the single source of truth on both sides:
//!
//! - the app reads/writes it here, persisting to `~/.orbit-pi/access.json`
//! - the extension reads the same file fresh on every tool call, so changing
//!   the mode re-arms live sessions without a restart
//!
//! Modes mirror the workbench convention (Supervised / Auto-accept edits /
//! Full access). `Supervised` is the default, and it is also what every
//! unreadable or unrecognised state resolves to: the guard is a confirmation
//! boundary, so a missing, truncated, or hand-edited file has to fall back to
//! *asking*, never to allowing. The extension applies the same rule
//! (`DEFAULT_MODE` in `policy.js`) — the two defaults must not drift, or the
//! chip would name one mode while the guard enforced another.

use std::path::PathBuf;

use serde_json::json;

/// The active access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccessMode {
    /// Ask before file changes and commands; reads pass through.
    #[default]
    Supervised,
    /// Auto-approve edits; ask before commands and other actions.
    AutoAcceptEdits,
    /// Allow commands and edits without prompts.
    FullAccess,
}

impl AccessMode {
    /// Every mode, in picker order.
    pub const ALL: [AccessMode; 3] = [Self::Supervised, Self::AutoAcceptEdits, Self::FullAccess];

    /// The wire/persistence id. Kept in sync with `MODES` in the extension's
    /// `policy.js`.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Supervised => "supervised",
            Self::AutoAcceptEdits => "auto-accept-edits",
            Self::FullAccess => "full-access",
        }
    }

    /// Parse a stored id, falling back to [`AccessMode::default`] on anything
    /// unknown so a hand-edited file can never widen access by accident.
    pub fn from_wire(value: &str) -> Self {
        match value {
            "supervised" => Self::Supervised,
            "auto-accept-edits" => Self::AutoAcceptEdits,
            "full-access" => Self::FullAccess,
            _ => Self::default(),
        }
    }

    /// Short label for the composer chip and picker rows.
    pub fn label(self) -> String {
        match self {
            Self::Supervised => tr!("access.supervised"),
            Self::AutoAcceptEdits => tr!("access.auto_accept_edits"),
            Self::FullAccess => tr!("access.full_access"),
        }
    }

    /// One-line explanation shown under the label in the picker.
    pub fn description(self) -> String {
        match self {
            Self::Supervised => tr!("access.supervised_hint"),
            Self::AutoAcceptEdits => tr!("access.auto_accept_edits_hint"),
            Self::FullAccess => tr!("access.full_access_hint"),
        }
    }

    /// The bundled icon path for the chip and picker row.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Supervised | Self::FullAccess => "icons/lock.svg",
            Self::AutoAcceptEdits => "icons/compose.svg",
        }
    }

    /// Load the persisted mode, falling back to the default when the file is
    /// missing, unreadable, or malformed. The default is the strictest mode, so
    /// every one of those states prompts rather than allows.
    pub fn load() -> Self {
        Self::load_at(&store_path())
    }

    fn load_at(path: &std::path::Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
            return Self::default();
        };
        value
            .get("mode")
            .and_then(serde_json::Value::as_str)
            .map(Self::from_wire)
            .unwrap_or_default()
    }

    /// Persist the mode to `~/.orbit-pi/access.json`. This is also the file
    /// the guard extension reads, so a write re-arms live sessions.
    ///
    /// The write is atomic (temp + rename) and owner-only, so a crash cannot
    /// leave a half-written file behind — which the guard would have to treat
    /// as malformed — and the error is returned rather than swallowed: a mode
    /// the app could not persist is a mode the guard will not enforce.
    pub fn persist(self) -> Result<(), String> {
        Self::persist_at(&store_path(), self)
    }

    fn persist_at(path: &std::path::Path, mode: Self) -> Result<(), String> {
        let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(parent).map_err(|err| {
            tr!(
                "errors.could_not_create",
                path = parent.display().to_string(),
                error = err
            )
        })?;
        let temporary = parent.join(format!(".access.json.{}.tmp", std::process::id()));
        let write = || -> Result<(), String> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|err| {
                    tr!(
                        "errors.could_not_write",
                        path = temporary.display().to_string(),
                        error = err
                    )
                })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(std::fs::Permissions::from_mode(0o600))
                    .map_err(|err| {
                        tr!(
                            "errors.could_not_write",
                            path = temporary.display().to_string(),
                            error = err
                        )
                    })?;
            }
            use std::io::Write;
            file.write_all(json!({ "mode": mode.as_wire() }).to_string().as_bytes())
                .map_err(|err| {
                    tr!(
                        "errors.could_not_write",
                        path = temporary.display().to_string(),
                        error = err
                    )
                })?;
            std::fs::rename(&temporary, path).map_err(|err| {
                tr!(
                    "errors.could_not_replace",
                    path = path.display().to_string(),
                    error = err
                )
            })
        };
        let result = write();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }
}

/// `~/.orbit-pi/access-allow.json` — the guard extension's "Always allow this
/// tool" grants, keyed by mode. Orbit only ever reads it to report what is
/// remembered and to clear it; the extension does the writing.
pub(crate) fn allowlist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("access-allow.json")
}

/// Tool names the user has chosen "Always allow" for, across every mode, in a
/// stable order. Anything unreadable or malformed reads as "nothing granted" —
/// the safe direction, since a missing grant only means the prompt returns.
pub(crate) fn remembered_approvals() -> Vec<String> {
    remembered_approvals_at(&allowlist_path())
}

fn remembered_approvals_at(path: &std::path::Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(modes)) = serde_json::from_str(&raw) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for list in modes.values() {
        let Some(list) = list.as_array() else {
            continue;
        };
        for name in list.iter().filter_map(serde_json::Value::as_str) {
            if !out.iter().any(|known| known == name) {
                out.push(name.to_string());
            }
        }
    }
    out.sort();
    out
}

/// Forget every "Always allow" grant. Returns how many tool names were dropped
/// so the caller can report it; a missing file is not an error.
pub(crate) fn clear_remembered_approvals() -> Result<usize, String> {
    clear_remembered_approvals_at(&allowlist_path())
}

fn clear_remembered_approvals_at(path: &std::path::Path) -> Result<usize, String> {
    let dropped = remembered_approvals_at(path).len();
    match std::fs::remove_file(path) {
        Ok(()) => Ok(dropped),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(dropped),
        Err(err) => Err(tr!(
            "errors.could_not_replace",
            path = path.display().to_string(),
            error = err
        )),
    }
}

/// `~/.orbit-pi/access.json` — the app and the guard extension share it.
fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("access.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("orbit-access-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn every_unreadable_state_falls_back_to_the_strictest_mode() {
        // The guard is a confirmation boundary: a missing, truncated, or
        // hand-edited file has to prompt, never allow.
        assert_eq!(AccessMode::default(), AccessMode::Supervised);
        assert_eq!(
            AccessMode::from_wire("bypass-everything"),
            AccessMode::Supervised
        );

        let dir = temp_dir("load");
        let path = dir.join("access.json");
        assert_eq!(
            AccessMode::load_at(&path),
            AccessMode::Supervised,
            "missing"
        );
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(
            AccessMode::load_at(&path),
            AccessMode::Supervised,
            "corrupt"
        );
        std::fs::write(&path, r#"{"mode":"bogus"}"#).unwrap();
        assert_eq!(
            AccessMode::load_at(&path),
            AccessMode::Supervised,
            "unknown"
        );
        std::fs::write(&path, r#"{"mode":"full-access"}"#).unwrap();
        assert_eq!(AccessMode::load_at(&path), AccessMode::FullAccess, "chosen");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn persist_writes_owner_only_and_round_trips() {
        let dir = temp_dir("persist");
        let path = dir.join("access.json");
        AccessMode::persist_at(&path, AccessMode::FullAccess).unwrap();
        assert_eq!(AccessMode::load_at(&path), AccessMode::FullAccess);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "access.json must stay owner-only");
        }
        // Rewriting leaves no temp file behind for the guard to trip over.
        AccessMode::persist_at(&path, AccessMode::Supervised).unwrap();
        assert_eq!(AccessMode::load_at(&path), AccessMode::Supervised);
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "stray temp files: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remembered_approvals_lists_and_clears() {
        let dir = temp_dir("allowlist");
        let path = dir.join("access-allow.json");
        assert!(remembered_approvals_at(&path).is_empty(), "missing file");
        std::fs::write(
            &path,
            r#"{"supervised":["edit"],"full-access":["write","edit"]}"#,
        )
        .unwrap();
        assert_eq!(
            remembered_approvals_at(&path),
            vec!["edit".to_string(), "write".to_string()],
            "deduped across modes, sorted"
        );
        assert_eq!(clear_remembered_approvals_at(&path).unwrap(), 2);
        assert!(!path.exists(), "the file is gone, not emptied");
        assert_eq!(
            clear_remembered_approvals_at(&path).unwrap(),
            0,
            "idempotent"
        );
        std::fs::write(&path, "{not json").unwrap();
        assert!(
            remembered_approvals_at(&path).is_empty(),
            "corrupt reads as none"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wire_round_trips() {
        for mode in AccessMode::ALL {
            assert_eq!(AccessMode::from_wire(mode.as_wire()), mode);
        }
    }

    #[test]
    fn unknown_wire_falls_back_to_default() {
        assert_eq!(
            AccessMode::from_wire("bypass-everything"),
            AccessMode::default()
        );
        assert_eq!(AccessMode::from_wire(""), AccessMode::default());
    }

    #[test]
    fn every_mode_has_distinct_copy() {
        let labels: std::collections::HashSet<_> =
            AccessMode::ALL.iter().map(|m| m.label()).collect();
        assert_eq!(labels.len(), AccessMode::ALL.len());
        for mode in AccessMode::ALL {
            assert!(!mode.description().is_empty());
            assert!(mode.icon().starts_with("icons/"));
        }
    }
}
