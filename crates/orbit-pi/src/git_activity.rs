//! What the sidebar's Git button flags: local changes, and issues and pull
//! requests opened on GitHub since the user last looked.
//!
//! GitHub has no "unread" state for issues or pull requests, so "new" means
//! *opened after the last time the user viewed that tab for that repository*.
//! Those view times, and the switch for the background GitHub check, live in
//! `~/.orbit-pi/git-activity.json`:
//!
//! ```jsonc
//! {
//!   "github_checks": true,
//!   "seen": { "owner/name": { "issues": "2026-11-23T00:12:34Z", "pulls": "…" } }
//! }
//! ```
//!
//! A repository seen for the first time is baselined at that moment, so the
//! existing backlog never shows up as "new". Timestamps are GitHub's own UTC
//! `YYYY-MM-DDTHH:MM:SSZ` form, which orders correctly as plain strings.
//! Parsing is lenient: a missing or corrupt file means defaults.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Map, Value};

/// How often the GitHub check runs while the app is open.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);

/// The shortest gap between two checks, so a window flicking in and out of
/// focus does not run `gh` on every activation.
pub const MIN_POLL_GAP: Duration = Duration::from_secs(60);

/// The two GitHub lists the button watches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Issues,
    Pulls,
}

impl Kind {
    fn key(self) -> &'static str {
        match self {
            Self::Issues => "issues",
            Self::Pulls => "pulls",
        }
    }
}

/// One open issue or pull request, as much of it as "is it new?" needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stamp {
    /// GitHub's `createdAt`, UTC.
    pub created_at: String,
    /// The author's login, so the user's own items never count as new.
    pub author: Option<String>,
}

/// How many of `items` were opened after `since` by someone other than `me`.
pub fn count_new(items: &[Stamp], since: &str, me: Option<&str>) -> usize {
    items
        .iter()
        .filter(|item| item.created_at.as_str() > since)
        .filter(|item| match (item.author.as_deref(), me) {
            (Some(author), Some(me)) => !author.eq_ignore_ascii_case(me),
            _ => true,
        })
        .count()
}

/// The current time in GitHub's timestamp form.
pub fn now_stamp() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// What the Git button reports right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    /// Changed files in the working tree (staged or not).
    pub changes: usize,
    pub new_issues: usize,
    pub new_pulls: usize,
    /// Open pull requests in this repository waiting on the user's review.
    /// Not a "since you looked" count: one stays until it is reviewed.
    pub review_requests: usize,
}

impl Activity {
    /// Whether the button shows its dot.
    pub fn any(&self) -> bool {
        self.changes > 0 || self.new_issues > 0 || self.new_pulls > 0 || self.review_requests > 0
    }

    /// The reasons for the dot, in reading order, each already worded
    /// ("4 changed files"). Empty when there is nothing to report.
    pub fn reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        match self.changes {
            0 => {}
            1 => reasons.push(tr!("git_activity.changed_files_one", count = 1)),
            n => reasons.push(tr!("git_activity.changed_files_other", count = n)),
        }
        match self.new_issues {
            0 => {}
            1 => reasons.push(tr!("git_activity.new_issues_one", count = 1)),
            n => reasons.push(tr!("git_activity.new_issues_other", count = n)),
        }
        match self.new_pulls {
            0 => {}
            1 => reasons.push(tr!("git_activity.new_pulls_one", count = 1)),
            n => reasons.push(tr!("git_activity.new_pulls_other", count = n)),
        }
        match self.review_requests {
            0 => {}
            1 => reasons.push(tr!("git_activity.review_requests_one", count = 1)),
            n => reasons.push(tr!("git_activity.review_requests_other", count = n)),
        }
        reasons
    }
}

/// The persisted switch and per-repository view times.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    /// Whether the background GitHub check runs at all.
    pub github_checks: bool,
    /// `owner/name` → kind key → last viewed (GitHub timestamp form).
    seen: BTreeMap<String, BTreeMap<String, String>>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            github_checks: true,
            seen: BTreeMap::new(),
        }
    }
}

impl Store {
    /// The user's store; defaults under test, so a test never reads (or,
    /// through [`Self::persist`], writes) the real `~/.orbit-pi` file.
    pub fn load() -> Self {
        if cfg!(test) {
            return Self::default();
        }
        Self::load_from(&persist_path())
    }

    pub(crate) fn load_from(path: &Path) -> Self {
        let Some(value) = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        else {
            return Self::default();
        };
        let mut store = Self::default();
        if let Some(on) = value.get("github_checks").and_then(Value::as_bool) {
            store.github_checks = on;
        }
        if let Some(repos) = value.get("seen").and_then(Value::as_object) {
            for (repo, kinds) in repos {
                let Some(kinds) = kinds.as_object() else {
                    continue;
                };
                let times: BTreeMap<String, String> = kinds
                    .iter()
                    .filter_map(|(kind, at)| Some((kind.clone(), at.as_str()?.to_owned())))
                    .collect();
                if !times.is_empty() {
                    store.seen.insert(repo.clone(), times);
                }
            }
        }
        store
    }

    /// Best-effort write; a failure only means the next launch re-baselines.
    pub fn persist(&self) {
        if cfg!(test) {
            return;
        }
        self.persist_to(&persist_path());
    }

    pub(crate) fn persist_to(&self, path: &Path) {
        let seen: Map<String, Value> = self
            .seen
            .iter()
            .map(|(repo, kinds)| (repo.clone(), json!(kinds)))
            .collect();
        let body = json!({ "github_checks": self.github_checks, "seen": seen });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Write beside the target and rename over it, so a crash mid-write
        // never leaves a half file that would reset every repository.
        let temp = path.with_extension("json.tmp");
        if std::fs::write(&temp, body.to_string()).is_ok() {
            let _ = std::fs::rename(&temp, path);
        }
    }

    /// When the user last viewed `kind` for `repo`, if ever.
    pub fn seen_since(&self, repo: &str, kind: Kind) -> Option<&str> {
        self.seen.get(repo)?.get(kind.key()).map(String::as_str)
    }

    /// Record that the user viewed `kind` for `repo` at `at`.
    pub fn mark_seen(&mut self, repo: &str, kind: Kind, at: &str) {
        self.seen
            .entry(repo.to_owned())
            .or_default()
            .insert(kind.key().to_owned(), at.to_owned());
    }

    /// The first time a repository is checked, start both lists at `at`, so
    /// its existing backlog never reads as new. Returns whether anything was
    /// recorded (and so needs persisting).
    pub fn baseline(&mut self, repo: &str, at: &str) -> bool {
        let mut changed = false;
        for kind in [Kind::Issues, Kind::Pulls] {
            if self.seen_since(repo, kind).is_none() {
                self.mark_seen(repo, kind, at);
                changed = true;
            }
        }
        changed
    }
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("git-activity.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(at: &str, author: &str) -> Stamp {
        Stamp {
            created_at: at.into(),
            author: Some(author.into()),
        }
    }

    #[test]
    fn only_items_opened_after_the_last_look_by_someone_else_are_new() {
        let items = [
            stamp("2026-11-23T09:00:00Z", "ana"),
            stamp("2026-11-23T08:00:00Z", "me"),
            stamp("2026-11-22T23:59:59Z", "bo"),
            Stamp {
                created_at: "2026-11-23T10:00:00Z".into(),
                author: None,
            },
        ];
        let since = "2026-11-23T00:00:00Z";
        assert_eq!(count_new(&items, since, Some("ME")), 2, "own item and old item skipped");
        assert_eq!(count_new(&items, since, None), 3, "unknown user: only age decides");
        assert_eq!(count_new(&items, "2026-11-24T00:00:00Z", Some("me")), 0);
    }

    #[test]
    fn now_stamp_orders_against_github_timestamps() {
        let now = now_stamp();
        assert_eq!(now.len(), "2026-11-23T00:00:00Z".len(), "{now}");
        assert!(now.ends_with('Z'), "{now}");
        assert!(now.as_str() > "2000-01-01T00:00:00Z");
    }

    #[test]
    fn a_first_check_baselines_the_repository_once() {
        let mut store = Store::default();
        assert!(store.baseline("o/r", "2026-11-23T00:00:00Z"));
        assert_eq!(store.seen_since("o/r", Kind::Issues), Some("2026-11-23T00:00:00Z"));
        assert_eq!(store.seen_since("o/r", Kind::Pulls), Some("2026-11-23T00:00:00Z"));
        store.mark_seen("o/r", Kind::Pulls, "2026-11-24T00:00:00Z");
        assert!(!store.baseline("o/r", "2026-11-25T00:00:00Z"), "already baselined");
        assert_eq!(store.seen_since("o/r", Kind::Pulls), Some("2026-11-24T00:00:00Z"));
    }

    #[test]
    fn the_store_round_trips_and_tolerates_a_bad_file() {
        let dir = std::env::temp_dir().join(format!("orbit-git-activity-{}", std::process::id()));
        let path = dir.join("git-activity.json");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(Store::load_from(&path), Store::default(), "missing file");
        let mut store = Store {
            github_checks: false,
            ..Store::default()
        };
        store.mark_seen("o/r", Kind::Issues, "2026-11-23T00:00:00Z");
        store.persist_to(&path);
        assert_eq!(Store::load_from(&path), store);
        assert!(!dir.join("git-activity.json.tmp").exists(), "no temp file left");

        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Store::load_from(&path), Store::default(), "corrupt file");
        std::fs::write(&path, r#"{"seen":{"o/r":{"issues":7},"x/y":[]}}"#).unwrap();
        assert_eq!(Store::load_from(&path), Store::default(), "wrong shapes are dropped");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_dot_needs_a_reason_and_says_it() {
        assert!(!Activity::default().any());
        assert!(Activity::default().reasons().is_empty());
        let activity = Activity {
            changes: 4,
            new_issues: 0,
            new_pulls: 1,
            review_requests: 0,
        };
        assert!(activity.any());
        assert_eq!(activity.reasons().len(), 2, "only the non-zero reasons");
    }
}
