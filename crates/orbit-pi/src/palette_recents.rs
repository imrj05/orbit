//! The commands most recently run from the command palette, newest first.
//!
//! An empty palette query lists these at the head of its Commands section, so
//! the actions a user reaches for most are one Enter away; a typed query uses
//! them as the tie-break between equal matches.
//!
//! The list is persisted to `~/.orbit-pi/palette.json`:
//!
//! ```jsonc
//! { "recent_commands": ["NewSession", "OpenGit"] }
//! ```
//!
//! Each entry is the command's `CommandId` name, resolved back through the
//! registry. Parsing is lenient: an unknown name (a command renamed or removed
//! since) is skipped, and a missing or malformed file is an empty list.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::commands::{CommandId, COMMANDS};

/// How many commands the palette remembers.
pub const MAX_RECENT: usize = 5;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recents {
    commands: Vec<CommandId>,
}

impl Recents {
    /// Read `~/.orbit-pi/palette.json`, defaulting on any failure.
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
        let mut recents = Self::default();
        for name in value
            .get("recent_commands")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if let Some(id) = command_named(name) {
                if !recents.commands.contains(&id) && recents.commands.len() < MAX_RECENT {
                    recents.commands.push(id);
                }
            }
        }
        recents
    }

    /// Persist to `~/.orbit-pi/palette.json`. Best-effort: a failed write
    /// leaves the in-memory list active for this run. Unused under test, where
    /// the app must not write the user's real home directory.
    #[cfg_attr(test, allow(dead_code))]
    pub fn persist(&self) {
        self.persist_to(&persist_path());
    }

    pub(crate) fn persist_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let names: Vec<String> = self.commands.iter().map(|id| format!("{id:?}")).collect();
        let body = json!({ "recent_commands": names });
        let _ = std::fs::write(path, body.to_string());
    }

    /// Move `id` to the front, dropping the oldest past [`MAX_RECENT`].
    pub fn record(&mut self, id: CommandId) {
        self.commands.retain(|recent| *recent != id);
        self.commands.insert(0, id);
        self.commands.truncate(MAX_RECENT);
    }

    /// The remembered commands, newest first.
    pub fn commands(&self) -> &[CommandId] {
        &self.commands
    }
}

/// The registry command whose `CommandId` name is `name`.
fn command_named(name: &str) -> Option<CommandId> {
    COMMANDS
        .iter()
        .map(|spec| spec.id)
        .find(|id| format!("{id:?}") == name)
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("palette.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("orbit-palette-recents-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn recording_moves_a_command_to_the_front_and_caps_the_list() {
        let mut recents = Recents::default();
        recents.record(CommandId::NewSession);
        recents.record(CommandId::OpenGit);
        recents.record(CommandId::NewSession);
        assert_eq!(
            recents.commands(),
            &[CommandId::NewSession, CommandId::OpenGit],
            "a repeat moves to the front instead of appearing twice"
        );
        for id in [
            CommandId::ToggleSidebar,
            CommandId::ToggleTerminal,
            CommandId::OpenSettings,
            CommandId::OpenWorktrees,
        ] {
            recents.record(id);
        }
        assert_eq!(recents.commands().len(), MAX_RECENT);
        assert_eq!(recents.commands()[0], CommandId::OpenWorktrees);
        assert!(
            !recents.commands().contains(&CommandId::OpenGit),
            "the oldest falls off"
        );
    }

    #[test]
    fn the_list_round_trips_through_its_file() {
        let path = temp("round-trip.json");
        let mut recents = Recents::default();
        recents.record(CommandId::OpenGit);
        recents.record(CommandId::NewSession);
        recents.persist_to(&path);
        assert_eq!(Recents::load_from(&path), recents);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_bad_or_stale_file_reads_leniently() {
        let path = temp("lenient.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(Recents::load_from(&path), Recents::default());

        std::fs::write(
            &path,
            r#"{ "recent_commands": ["OpenGit", "NoLongerACommand", 7, "OpenGit", "NewSession"] }"#,
        )
        .unwrap();
        assert_eq!(
            Recents::load_from(&path).commands(),
            &[CommandId::OpenGit, CommandId::NewSession],
            "unknown names and duplicates are skipped"
        );
        assert_eq!(
            Recents::load_from(&temp("missing.json")),
            Recents::default()
        );
        let _ = std::fs::remove_file(&path);
    }
}
