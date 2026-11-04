//! Where a task runs: the pure half of the "Work in" control.
//!
//! A **working folder** is the absolute directory a session runs in. That is
//! already one identity in Orbit (INTENT D14): `current_workspace`, the pi
//! child's cwd, and the session's persisted `cwd` are all the same path. This
//! module adds no storage and no second filesystem abstraction — it answers
//! two questions the UI cannot answer from a path alone:
//!
//! 1. **What kind of place is this?** [`classify`] reads `<path>/.git` and
//!    decides between the repository's main checkout, a linked worktree, an
//!    ordinary folder, and a *broken* worktree whose administrative entry is
//!    gone. Broken is reported, never repaired implicitly: `git worktree
//!    repair` and `git worktree add` can both fail on that state, so the UI
//!    has to say so instead of opening a folder that is not what it claims.
//! 2. **What does working from a remote branch mean?** [`remote_ref_rows`]
//!    turns remote-tracking refs into the local branch a worktree would need.
//!    `git worktree add <path> origin/feat` silently produces a **detached**
//!    worktree (verified), so a remote ref always names the local branch that
//!    tracks it — see [`crate::worktree::WorktreeManager::create`].
//!
//! Everything here is filesystem-only and allocation-light: no GPUI, no
//! process spawning, so the picker can call it during layout.

use std::path::{Path, PathBuf};

use crate::git::RefKind;
use crate::git_ops::RefEntry;
use crate::worktree::{self, Worktree};

/// How a folder relates to Git, as far as the filesystem can tell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// The repository's main working directory.
    Local,
    /// A linked worktree. `owner_root` is the repository that owns it, derived
    /// from the `.git` file — a worktree of *another* repository classifies as
    /// a worktree here and is told apart by `owner_root`.
    Worktree { name: String, owner_root: PathBuf },
    /// A directory outside any worktree. `is_repo` separates a repository's
    /// main checkout that Orbit does not have a root for from a plain folder.
    PlainFolder { is_repo: bool },
    /// A directory that carries worktree metadata Git can no longer use.
    Broken(BrokenReason),
}

/// Why a worktree directory cannot be used as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokenReason {
    /// The directory is missing.
    Missing,
    /// `<path>/.git` names an administrative directory that does not exist —
    /// the state left behind by a deleted `.git/worktrees/<id>`.
    AdminMissing { gitdir: PathBuf },
    /// `<path>/.git` is neither a directory nor a readable `gitdir:` file.
    Unreadable,
}

impl Location {
    /// Whether the folder can be opened as a working folder at all. A broken
    /// worktree cannot: the picker offers a repair instead of switching to it.
    pub fn is_usable(&self) -> bool {
        !matches!(self, Self::Broken(_))
    }
}

/// Classify `path` against the repository whose main checkout is `root`.
///
/// Filesystem only: no Git process runs, so this is safe in a layout pass.
/// `root` may be `None` when the active workspace is not a repository; then
/// only `PlainFolder` and `Broken` can come back.
pub fn classify(path: &Path, root: Option<&Path>) -> Location {
    if !path.is_dir() {
        return Location::Broken(BrokenReason::Missing);
    }
    if let Some(root) = root {
        if worktree::same_path(path, root) {
            return Location::Local;
        }
    } else if is_repo_root(path) {
        // No cached root, but this path *is* a main checkout: the caller can
        // treat it as Local without a second Git call.
        return Location::Local;
    }
    let meta = match std::fs::symlink_metadata(path.join(".git")) {
        Ok(meta) => meta,
        Err(_) => {
            return Location::PlainFolder { is_repo: false };
        }
    };
    if meta.is_dir() {
        // A directory `.git` means a main working directory. For a repository
        // Orbit was not pointed at, it is still a usable folder, just not this
        // repository's Local.
        return Location::PlainFolder { is_repo: true };
    }
    let Ok(contents) = std::fs::read_to_string(path.join(".git")) else {
        return Location::Broken(BrokenReason::Unreadable);
    };
    let Some(gitdir) = parse_gitfile(&contents) else {
        return Location::Broken(BrokenReason::Unreadable);
    };
    if !gitdir.exists() {
        return Location::Broken(BrokenReason::AdminMissing { gitdir });
    }
    // `<owner>/.git/worktrees/<id>` — the administrative directory sits three
    // levels under the owning repository's root, so the root is derivable
    // without asking Git. A `.git` file that does not follow that layout is
    // reported as unreadable rather than guessed at.
    let Some(owner_root) = owner_root_for(&gitdir) else {
        return Location::Broken(BrokenReason::Unreadable);
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    Location::Worktree { name, owner_root }
}

/// Whether `path` itself looks like a repository's main working directory.
fn is_repo_root(path: &Path) -> bool {
    path.join(".git").is_dir()
}

/// Parse a Git worktree's `.git` file: `gitdir: /abs/path` (the only line).
fn parse_gitfile(contents: &str) -> Option<PathBuf> {
    let line = contents.lines().next()?.trim();
    let target = line.strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }
    Some(PathBuf::from(target))
}

/// `<owner>/.git/worktrees/<id>` → `<owner>`.
fn owner_root_for(gitdir: &Path) -> Option<PathBuf> {
    let admin = gitdir.parent()?; // .../.git/worktrees
    if admin.file_name()? != "worktrees" {
        return None;
    }
    let dot_git = admin.parent()?; // .../.git
    if dot_git.file_name()? != ".git" {
        return None;
    }
    dot_git.parent().map(Path::to_path_buf)
}

// ── choosing a folder ────────────────────────────────────────────────

/// A resolved working folder: the path plus what the chip should call it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingFolder {
    pub path: PathBuf,
    /// `Local`, a worktree name, or the folder name.
    pub label: String,
    pub location: Location,
}

impl WorkingFolder {
    /// Build a folder from a classified path. The label follows the location:
    /// `Local` for the main checkout, the worktree name for a worktree, the
    /// directory name otherwise.
    pub fn new(path: PathBuf, location: Location) -> Self {
        let label = match &location {
            Location::Local => tr!("working_folder.local"),
            Location::Worktree { name, .. } => name.clone(),
            Location::PlainFolder { .. } | Location::Broken(_) => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        };
        Self {
            path,
            label,
            location,
        }
    }
}

/// One row of the "Work in" list, built from Git's own worktree list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub folder: WorkingFolder,
    pub label: String,
    pub detail: Option<String>,
    /// The folder the active workspace is in.
    pub selected: bool,
    /// `prunable`: Git lists it, the directory is gone. Choosing it offers a
    /// repair instead of a silent failure.
    pub prunable: bool,
}

/// The location rows for `worktrees`: Local (the main checkout) first, then
/// every linked worktree in Git's order. `root` is the repository's main
/// checkout and `active` marks the row the working folder is on.
///
/// The main worktree is always the first row even when Git reports a different
/// order, so "Local" never moves.
pub fn location_rows(worktrees: &[Worktree], root: &Path, active: Option<&Path>) -> Vec<FolderRow> {
    let mut rows: Vec<FolderRow> = Vec::new();
    let Some(main) = worktrees
        .iter()
        .find(|worktree| worktree.is_main)
        .or_else(|| worktrees.first())
    else {
        return rows;
    };
    rows.push(FolderRow {
        folder: WorkingFolder::new(root.to_path_buf(), Location::Local),
        label: tr!("working_folder.local"),
        detail: main.branch.clone(),
        selected: active.is_some_and(|active| worktree::same_path(root, active)),
        prunable: main.prunable,
    });
    for worktree in worktrees.iter().filter(|worktree| !worktree.is_main) {
        let location = Location::Worktree {
            name: worktree.name.clone(),
            owner_root: root.to_path_buf(),
        };
        rows.push(FolderRow {
            folder: WorkingFolder::new(worktree.path.clone(), location),
            label: worktree.name.clone(),
            detail: Some(worktree.branch_label()),
            selected: active.is_some_and(|active| worktree::same_path(&worktree.path, active)),
            prunable: worktree.prunable,
        });
    }
    rows
}

/// One remote-tracking ref, resolved into what a worktree needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteRow {
    /// `origin/feat` — what the user picked and what the worktree tracks.
    pub remote_ref: String,
    /// The local branch that will be created: `feat`, `feat-2`, …
    pub local_branch: String,
    /// `origin`
    pub remote: String,
    /// `feat`
    pub short: String,
}

/// Turn remote-tracking refs into rows, skipping `*/HEAD` and anything whose
/// local branch name is already taken by a different ref.
///
/// `existing_branches` are local branch names: a taken name is suffixed rather
/// than replaced, so two remote branches called `feat` (on different remotes)
/// both stay reachable.
pub fn remote_ref_rows(
    refs: &[RefEntry],
    remotes: &[String],
    existing_branches: &[String],
) -> Vec<RemoteRow> {
    let mut taken: Vec<String> = existing_branches.to_vec();
    let mut rows = Vec::new();
    for entry in refs.iter().filter(|entry| entry.kind == RefKind::Remote) {
        let Some((remote, short)) = split_remote_ref(&entry.name, remotes) else {
            continue;
        };
        if short == "HEAD" || short.is_empty() {
            continue;
        }
        let local_branch = local_branch_for(&short, &taken);
        taken.push(local_branch.clone());
        rows.push(RemoteRow {
            remote_ref: entry.name.clone(),
            local_branch,
            remote,
            short,
        });
    }
    rows
}

/// Split `origin/feat` into (`origin`, `feat`), using the known remote names
/// so a remote containing a slash is still split in the right place.
fn split_remote_ref(remote_ref: &str, remotes: &[String]) -> Option<(String, String)> {
    let mut candidates: Vec<&String> = remotes.iter().collect();
    // Longest remote first: `up/stream` must win over `up`.
    candidates.sort_by_key(|remote| std::cmp::Reverse(remote.len()));
    for remote in candidates {
        if let Some(rest) = remote_ref
            .strip_prefix(remote.as_str())
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|rest| !rest.is_empty())
        {
            return Some((remote.clone(), rest.to_string()));
        }
    }
    // A ref Git reported as remote-tracking whose remote is no longer
    // configured (deleted remote): split at the first separator so the row is
    // still usable.
    let (remote, short) = remote_ref.split_once('/')?;
    (!short.is_empty()).then(|| (remote.to_string(), short.to_string()))
}

/// `origin/feat` → `feat`: the part of a remote-tracking ref a local branch is
/// named after. A name without a separator is returned unchanged.
pub fn short_branch_name(remote_ref: &str) -> String {
    match remote_ref.split_once('/') {
        Some((_, short)) if !short.is_empty() => short.to_string(),
        _ => remote_ref.to_string(),
    }
}

/// A free local branch name for `short`: the name itself, else `-2`, `-3`, …
/// matching the worktree-name suggestion loop.
pub fn local_branch_for(short: &str, existing_branches: &[String]) -> String {
    if !existing_branches.iter().any(|branch| branch == short) {
        return short.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{short}-{n}");
        if !existing_branches.iter().any(|branch| branch == &candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "orbit-folder-test-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn join(&self, name: &str) -> PathBuf {
            self.path.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn worktree(name: &str, is_main: bool, prunable: bool) -> Worktree {
        Worktree {
            path: if is_main {
                PathBuf::from("/repo")
            } else {
                PathBuf::from("/repo").join(name)
            },
            name: name.to_string(),
            head: "abc123".into(),
            branch: Some(name.to_string()),
            detached: false,
            is_main,
            bare: false,
            locked: false,
            lock_reason: None,
            prunable,
            prune_reason: None,
        }
    }

    #[test]
    fn the_main_checkout_classifies_as_local() {
        let dir = TempDir::new("local");
        let root = dir.path.clone();
        fs::create_dir_all(root.join(".git")).expect("fake .git");
        assert_eq!(classify(&root, Some(&root)), Location::Local);
        // Without a cached root the `.git` directory still identifies it.
        assert_eq!(classify(&root, None), Location::Local);
    }

    #[test]
    fn a_plain_folder_is_not_a_worktree() {
        let dir = TempDir::new("plain");
        let root = TempDir::new("plain-root");
        fs::create_dir_all(root.path.join(".git")).expect("fake .git");
        assert_eq!(
            classify(&dir.path, Some(&root.path)),
            Location::PlainFolder { is_repo: false }
        );
    }

    #[test]
    fn a_linked_worktree_reports_its_owner() {
        let dir = TempDir::new("linked");
        let owner = dir.join("owner");
        let admin = owner.join(".git/worktrees/113");
        fs::create_dir_all(&admin).expect("admin dir");
        let linked = dir.join("113");
        fs::create_dir_all(&linked).expect("worktree dir");
        fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", admin.display()),
        )
        .expect("gitfile");
        assert_eq!(
            classify(&linked, Some(&owner)),
            Location::Worktree {
                name: "113".into(),
                owner_root: owner.clone(),
            }
        );
        let location = classify(&linked, Some(&owner));
        assert!(location.is_usable());
    }

    #[test]
    fn a_worktree_whose_admin_entry_is_gone_is_broken_not_usable() {
        let dir = TempDir::new("broken");
        let owner = dir.join("owner");
        fs::create_dir_all(owner.join(".git/worktrees")).expect("admin parent");
        let admin = owner.join(".git/worktrees/113");
        let linked = dir.join("113");
        fs::create_dir_all(&linked).expect("worktree dir");
        fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", admin.display()),
        )
        .expect("gitfile");
        let location = classify(&linked, Some(&owner));
        assert_eq!(
            location,
            Location::Broken(BrokenReason::AdminMissing { gitdir: admin })
        );
        assert!(!location.is_usable(), "broken folders are never opened");
        assert_eq!(
            WorkingFolder::new(linked.clone(), location).label,
            "113",
            "a broken worktree is still named by its directory"
        );
    }

    #[test]
    fn a_missing_directory_is_broken() {
        let dir = TempDir::new("missing");
        let gone = dir.join("gone");
        assert_eq!(
            classify(&gone, None),
            Location::Broken(BrokenReason::Missing)
        );
        assert!(!classify(&gone, None).is_usable());
    }

    #[test]
    fn an_unreadable_gitfile_is_broken() {
        let dir = TempDir::new("unreadable");
        let folder = dir.join("weird");
        fs::create_dir_all(&folder).expect("folder");
        fs::write(folder.join(".git"), "not a gitfile\n").expect("write");
        assert_eq!(
            classify(&folder, None),
            Location::Broken(BrokenReason::Unreadable)
        );
    }

    #[test]
    fn location_rows_put_local_first_then_worktrees() {
        let rows = location_rows(
            &[
                worktree("repo", true, false),
                worktree("113", false, false),
                worktree("fix", false, true),
            ],
            Path::new("/repo"),
            Some(Path::new("/repo/113")),
        );
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, vec!["Local", "113", "fix"]);
        assert_eq!(
            rows.iter().position(|row| row.selected),
            Some(1),
            "the active worktree is selected"
        );
        assert!(rows[2].prunable, "prunable survives into the row");
    }

    #[test]
    fn location_rows_are_empty_without_a_main_worktree() {
        assert!(location_rows(&[], Path::new("/repo"), None).is_empty());
    }

    #[test]
    fn a_plain_folder_row_is_labelled_by_its_directory_name() {
        let folder = WorkingFolder::new(
            PathBuf::from("/tmp/notes"),
            Location::PlainFolder { is_repo: false },
        );
        assert_eq!(folder.label, "notes");
    }

    #[test]
    fn a_local_folder_is_labelled_local() {
        let folder = WorkingFolder::new(PathBuf::from("/repo"), Location::Local);
        assert_eq!(folder.label, tr!("working_folder.local"));
    }

    fn remote(name: &str) -> RefEntry {
        RefEntry {
            name: name.to_string(),
            kind: RefKind::Remote,
            commit_date: None,
        }
    }

    #[test]
    fn remote_rows_skip_head_and_build_a_local_branch() {
        let remotes = vec!["origin".to_string()];
        let rows = remote_ref_rows(
            &[
                remote("origin/HEAD"),
                remote("origin/feat"),
                remote("origin/fix"),
            ],
            &remotes,
            &[],
        );
        let names: Vec<(&str, &str)> = rows
            .iter()
            .map(|row| (row.remote_ref.as_str(), row.local_branch.as_str()))
            .collect();
        assert_eq!(
            names,
            vec![("origin/feat", "feat"), ("origin/fix", "fix")],
            "origin/HEAD is not a branch to check out"
        );
        assert_eq!(rows[0].remote, "origin");
        assert_eq!(rows[0].short, "feat");
    }

    #[test]
    fn remote_rows_suffix_a_local_branch_that_is_taken() {
        let rows = remote_ref_rows(
            &[remote("origin/feat")],
            &["origin".to_string()],
            &["feat".to_string(), "feat-2".to_string()],
        );
        assert_eq!(rows[0].local_branch, "feat-3");
    }

    #[test]
    fn two_remotes_with_the_same_branch_stay_reachable() {
        let rows = remote_ref_rows(
            &[remote("origin/feat"), remote("fork/feat")],
            &["fork".to_string(), "origin".to_string()],
            &[],
        );
        assert_eq!(rows[0].local_branch, "feat");
        assert_eq!(
            rows[1].local_branch, "feat-2",
            "the second remote branch gets its own local branch"
        );
    }

    #[test]
    fn a_remote_with_a_slash_is_split_at_the_longest_name() {
        let rows = remote_ref_rows(
            &[remote("up/stream/feat")],
            &["up".to_string(), "up/stream".to_string()],
            &[],
        );
        assert_eq!(rows[0].remote, "up/stream");
        assert_eq!(rows[0].short, "feat");
        assert_eq!(rows[0].local_branch, "feat");
    }

    #[test]
    fn a_deleted_remotes_ref_still_produces_a_row() {
        let rows = remote_ref_rows(&[remote("gone/feat")], &[], &[]);
        assert_eq!(rows[0].remote, "gone");
        assert_eq!(rows[0].local_branch, "feat");
    }

    #[test]
    fn local_branch_for_keeps_the_name_when_free() {
        assert_eq!(local_branch_for("feat", &[]), "feat");
        assert_eq!(local_branch_for("feat", &["fix".into()]), "feat");
    }
}
