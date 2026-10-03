//! Shared debounced filesystem watching.
//!
//! `notify`'s raw event stream is noisy — atomic saves, editor swap files,
//! git internals, build output. [`debounced_watch`] coalesces a burst into
//! one signal and keeps only the paths each caller cares about. Callers
//! scan/refresh off-thread and expose a cheap `take_dirty` the ~90 ms UI
//! heartbeat can poll, so no I/O ever blocks a frame.

use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use notify_debouncer_mini::{
    new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
    DebounceEventResult, Debouncer,
};

/// Start a recursive, debounced watch on `dir`. Signals `()` only when a
/// changed path passes `keep`. Returns `None` when `dir` is not a directory
/// or the platform backend cannot start.
pub fn debounced_watch(
    dir: &Path,
    debounce: Duration,
    keep: impl Fn(&Path) -> bool + Send + 'static,
) -> Option<(Debouncer<RecommendedWatcher>, Receiver<()>)> {
    if !dir.is_dir() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(debounce, move |result: DebounceEventResult| {
        let Ok(events) = result else { return };
        if events.iter().any(|event| keep(&event.path)) {
            let _ = tx.send(());
        }
    })
    .ok()?;
    debouncer
        .watcher()
        .watch(dir, RecursiveMode::Recursive)
        .ok()?;
    Some((debouncer, rx))
}

/// Drain a dirty channel into a single flag.
pub fn drain(rx: &Receiver<()>) -> bool {
    let mut dirty = false;
    while rx.try_recv().is_ok() {
        dirty = true;
    }
    dirty
}

/// Watches a workspace tree so Review and the Git page refresh when files or
/// git state change without an RPC event (pi edits, the user in another tool,
/// a CLI commit/checkout).
pub struct WorkspaceWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
    dirty: Receiver<()>,
}

impl WorkspaceWatcher {
    /// Watch `dir` recursively. `None` when the backend can't start; the
    /// existing manual refresh buttons still cover that case.
    pub fn start(dir: &Path) -> Option<Self> {
        Self::watch(dir, Duration::from_millis(500))
    }

    fn watch(dir: &Path, debounce: Duration) -> Option<Self> {
        let (debouncer, dirty) = debounced_watch(dir, debounce, workspace_relevant)?;
        Some(Self {
            _debouncer: debouncer,
            dirty,
        })
    }

    /// Whether a relevant path changed since the last call.
    pub fn take_dirty(&self) -> bool {
        drain(&self.dirty)
    }
}

/// Generated trees and editor cruft churn constantly without changing what
/// Review or the Git page show.
const IGNORED_DIRS: &[&str] = &[
    "target",
    "node_modules",
    ".next",
    "dist",
    "build",
    "__pycache__",
    ".venv",
    "venv",
    ".cache",
    ".gradle",
    "DerivedData",
];

fn workspace_relevant(path: &Path) -> bool {
    let parts: Vec<&str> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();

    if parts.iter().any(|part| IGNORED_DIRS.contains(part)) {
        return false;
    }

    // Under `.git`, only user-visible state is worth refreshing: the HEAD,
    // the index, packed refs, and branch/tag/remote refs. Everything else is
    // git-internal churn (objects, logs) or this app's own checkpoint scratch
    // — `capture_worktree_commit` writes `.git/orbit-checkpoint-index-*` on
    // every Review load, and ref checks read `refs/orbit/*`; keeping those
    // would make a refresh trigger the next refresh, forever.
    if let Some(git) = parts.iter().position(|part| *part == ".git") {
        let inner = &parts[git + 1..];
        return matches!(inner, ["HEAD"] | ["index"] | ["packed-refs"])
            || matches!(inner, ["refs", kind, ..] if matches!(*kind, "heads" | "remotes" | "tags"));
    }

    if let Some(name) = parts.last() {
        if *name == ".DS_Store"
            || name.ends_with('~')
            || name.ends_with(".swp")
            || name.ends_with(".swx")
        {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn workspace_relevant_keeps_sources_and_git_state() {
        for path in [
            "/tmp/ws/src/main.rs",
            "/tmp/ws/Cargo.toml",
            "/tmp/ws/.git/HEAD",
            "/tmp/ws/.git/index",
            "/tmp/ws/.git/packed-refs",
            "/tmp/ws/.git/refs/heads/main",
            "/tmp/ws/.git/refs/remotes/origin/main",
            "/tmp/ws/.git/refs/tags/v1",
        ] {
            assert!(workspace_relevant(Path::new(path)), "kept {path}");
        }
    }

    #[test]
    fn workspace_relevant_drops_generated_trees_and_cruft() {
        for path in [
            "/tmp/ws/target/debug/orbit-pi",
            "/tmp/ws/node_modules/pkg/index.js",
            "/tmp/ws/dist/app.js",
            "/tmp/ws/.DS_Store",
            "/tmp/ws/src/main.rs~",
            "/tmp/ws/src/.main.rs.swp",
        ] {
            assert!(!workspace_relevant(Path::new(path)), "dropped {path}");
        }
    }

    /// Orbit's own checkpoint machinery writes inside `.git`; if those paths
    /// were kept, an open Review pane would refresh itself endlessly.
    #[test]
    fn workspace_relevant_drops_git_internals_and_orbit_scratch() {
        for path in [
            "/tmp/ws/.git/objects/ab/cdef",
            "/tmp/ws/.git/logs/HEAD",
            "/tmp/ws/.git/orbit-checkpoint-index-abc123",
            "/tmp/ws/.git/orbit-checkpoint-index-abc123.lock",
            "/tmp/ws/.git/refs/orbit/session-abc-turn-1",
            "/tmp/ws/.git/index.lock",
            "/tmp/ws/.git/config",
        ] {
            assert!(!workspace_relevant(Path::new(path)), "dropped {path}");
        }
    }

    fn git(cwd: &Path, args: &[&str]) {
        let status = crate::git::command(cwd).args(args).status().unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    /// The bug this guards: Review loads by calling
    /// `capture_worktree_commit`, which writes `.git/orbit-checkpoint-index-*`.
    /// If the watcher kept that path, each refresh would trigger the next.
    #[test]
    fn workspace_watcher_ignores_orbit_checkpoint_scratch() {
        let dir = std::env::temp_dir().join("orbit-watch-checkpoint-scratch-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(&dir, &["config", "user.name", "Orbit Test"]);
        git(&dir, &["config", "user.email", "orbit@example.com"]);
        fs::write(dir.join("tracked.txt"), "baseline\n").unwrap();
        git(&dir, &["add", "tracked.txt"]);
        git(&dir, &["commit", "--quiet", "-m", "baseline"]);

        let watcher = WorkspaceWatcher::watch(&dir, Duration::from_millis(50)).expect("watcher");
        // Let any backend registration event settle, then start clean.
        std::thread::sleep(Duration::from_millis(300));
        let _ = watcher.take_dirty();

        for _ in 0..3 {
            crate::checkpoint::capture_worktree_commit(&dir).unwrap();
        }
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            !watcher.take_dirty(),
            "Review's checkpoint scratch must not dirty the workspace watch"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// The bug this guards: the Changes tab reloads its diff by reading the
    /// staged tree, which copies `.git/index` to a throwaway file. On macOS,
    /// `std::fs::copy` emitted an FSEvents notification for the *source*
    /// index; the watcher keeps `.git/index`, so each review load dirtied the
    /// workspace watch and triggered the next refresh — forever.
    #[test]
    fn workspace_watcher_ignores_changes_tab_diff_loads() {
        use crate::review::Source;
        let dir = std::env::temp_dir().join("orbit-watch-changes-tab-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "--quiet", "--initial-branch=main"]);
        git(&dir, &["config", "user.name", "Orbit Test"]);
        git(&dir, &["config", "user.email", "orbit@example.com"]);
        fs::write(dir.join("tracked.txt"), "baseline\n").unwrap();
        git(&dir, &["add", "tracked.txt"]);
        git(&dir, &["commit", "--quiet", "-m", "baseline"]);
        fs::write(dir.join("tracked.txt"), "staged\n").unwrap();
        git(&dir, &["add", "tracked.txt"]);
        fs::write(dir.join("tracked.txt"), "unstaged\n").unwrap();

        let watcher =
            WorkspaceWatcher::watch(&dir, Duration::from_millis(50)).expect("watcher");
        std::thread::sleep(Duration::from_millis(300));
        let _ = watcher.take_dirty();

        // What the Git page's Changes tab loads on every `git status`
        // refresh: the staged and unstaged working-tree diffs.
        let _ = crate::git::collect_review_diff(&dir, Source::Staged, None);
        let _ = crate::git::collect_review_diff(&dir, Source::Unstaged, None);

        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            assert!(
                !watcher.take_dirty(),
                "loading the Changes tab diff must not dirty the workspace watch"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn workspace_watcher_reports_a_source_change() {
        let dir = std::env::temp_dir().join("orbit-workspace-watch-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).unwrap();
        let watcher = WorkspaceWatcher::watch(&dir, Duration::from_millis(50)).expect("watcher");

        fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();

        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut changed = false;
        while std::time::Instant::now() < deadline {
            if watcher.take_dirty() {
                changed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(changed, "watcher reported no change for a source edit");
        let _ = fs::remove_dir_all(&dir);
    }
}
