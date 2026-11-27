//! In-memory workspace tree for the Explorer.
//!
//! Pure data: [`TreeIndex`] is built once from a [`super::walk::Snapshot`],
//! and [`visible_rows`] flattens it for a virtualized list. Expand/collapse is
//! a set of paths, filter is a string, and neither mutates the index — so the
//! panel can re-render from immutable state and the whole model is unit-tested
//! without a window.
//!
//! Shape follows `review::tree_rows` (directory auto-expand, filter, collapse)
//! but is keyed by workspace-relative `/`-separated paths instead of diff file
//! indices.

use std::collections::{HashMap, HashSet};

use super::walk::Snapshot;

/// A file's git status as a single letter. Derived from [`crate::git::StatusRow`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusBadge {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

impl StatusBadge {
    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Untracked => 'U',
            Self::Conflicted => '!',
        }
    }

    /// Reduce a porcelain status row to the one badge a tree shows.
    pub fn from_status(row: &crate::git::StatusRow) -> Self {
        if row.untracked() {
            return Self::Untracked;
        }
        if row.index == 'U' || row.worktree == 'U' {
            return Self::Conflicted;
        }
        if row.index == 'D' || row.worktree == 'D' {
            return Self::Deleted;
        }
        if matches!(row.index, 'R' | 'C') || matches!(row.worktree, 'R' | 'C') {
            return Self::Renamed;
        }
        if row.index == 'A' {
            return Self::Added;
        }
        Self::Modified
    }
}

/// One directory or file in the tree. `path` is workspace-relative and
/// `/`-separated; the root's path is empty.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Node {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<Node>,
}

/// A built tree plus the decorations a render needs. Immutable after
/// [`TreeIndex::build`].
#[derive(Clone, Debug, Default)]
pub struct TreeIndex {
    pub root: Node,
    /// Workspace-relative path → badge for changed files only.
    pub badges: HashMap<String, StatusBadge>,
    /// Directories that contain a changed descendant (Zed's dot on folders).
    pub dirty_dirs: HashSet<String>,
    pub file_count: usize,
    /// The walker hit its entry cap; the tree is incomplete.
    pub truncated: bool,
}

impl TreeIndex {
    /// Build the tree from a flat snapshot and a badge map. Missing
    /// intermediate directories are synthesized, so a file always has its
    /// ancestors even if the walker's directory list is sparse.
    pub fn build(snapshot: Snapshot, badges: HashMap<String, StatusBadge>) -> Self {
        let file_count = snapshot.files.len();
        // One merged list rather than directories then files. The order matters,
        // not only for the tree but for `insert`: see the comment on the sort.
        let mut entries: Vec<(String, bool)> = snapshot
            .dirs
            .into_iter()
            .map(|dir| (dir, true))
            .chain(snapshot.files.into_iter().map(|file| (file, false)))
            .collect();
        // Sort by *path components*, so `a/b` comes before `a/b-c` before
        // `a/b/d`. Plain byte order is not that: `/` sorts after `-` and `.`, so
        // `a/b-c` would land before `a/b/d` and `a` would gain `b` twice. NUL
        // cannot appear in a path, so keying on the path with `/` replaced by NUL
        // makes ordinary byte order the component order. Directories go first on
        // an identical path, so a file that also carries that path still wins the
        // tie — the old two-pass loop inserted the file list last and did the
        // same. The keys are built once and dropped with the sort. `split('/')`
        // keeps this independent of the platform separator: the walker joins
        // components with `/` itself.
        entries.sort_by_cached_key(|(path, is_dir)| (path.replace('/', "\0"), !*is_dir));

        let mut root = Node {
            is_dir: true,
            ..Node::default()
        };
        for (path, is_dir) in &entries {
            let parts: Vec<&str> = parts(path).collect();
            insert(&mut root, &parts, *is_dir);
        }
        sort_children(&mut root);

        let mut dirty_dirs = HashSet::new();
        for path in badges.keys() {
            for ancestor in ancestors(path) {
                dirty_dirs.insert(ancestor);
            }
        }

        Self {
            root,
            badges,
            dirty_dirs,
            file_count,
            truncated: snapshot.truncated,
        }
    }

    /// Every directory path in the tree — "expand all".
    pub fn all_dir_paths(&self) -> HashSet<String> {
        let mut out = HashSet::new();
        collect_dirs(&self.root, &mut out);
        out
    }
}

/// A workspace-relative `/`-separated path, split into the names that address
/// it in the tree. Empty components (a leading or doubled `/`) are dropped.
fn parts(path: &str) -> impl Iterator<Item = &str> {
    path.split('/').filter(|part| !part.is_empty())
}

/// Insert one path, in the component order [`TreeIndex::build`] visits them.
///
/// That order makes the sibling lookup O(1): paths that share a parent sort
/// together, so a parent's children are created in non-decreasing name order
/// and a sibling that already exists is always the *last* child — nothing under
/// a larger sibling name can follow it. The old code scanned every sibling on
/// each insert, which made one wide directory quadratic: 20k files in a single
/// folder cost ~1.5 s of the UI thread.
fn insert(node: &mut Node, parts: &[&str], is_dir: bool) {
    let Some((name, rest)) = parts.split_first() else {
        return;
    };
    let index = match node.children.last() {
        Some(last) if last.name == *name => node.children.len() - 1,
        _ => {
            let path = if node.path.is_empty() {
                (*name).to_string()
            } else {
                format!("{}/{}", node.path, name)
            };
            node.children.push(Node {
                name: (*name).to_string(),
                path,
                is_dir: rest.is_empty() && is_dir,
                children: Vec::new(),
            });
            node.children.len() - 1
        }
    };
    if rest.is_empty() {
        node.children[index].is_dir = is_dir;
    } else {
        node.children[index].is_dir = true;
        insert(&mut node.children[index], rest, is_dir);
    }
}

fn sort_children(node: &mut Node) {
    // Keyed once per child instead of rebuilding both lowercase names on every
    // comparison: a directory with many files is where this shows up.
    node.children
        .sort_by_cached_key(|child| (!child.is_dir, child.name.to_lowercase(), child.name.clone()));
    for child in &mut node.children {
        sort_children(child);
    }
}

fn collect_dirs(node: &Node, out: &mut HashSet<String>) {
    for child in &node.children {
        if child.is_dir {
            out.insert(child.path.clone());
            collect_dirs(child, out);
        }
    }
}

/// Every ancestor directory of a `/`-separated path, outermost first.
pub fn ancestors(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let parts: Vec<&str> = path.split('/').collect();
    for part in &parts[..parts.len().saturating_sub(1)] {
        if part.is_empty() {
            continue;
        }
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        out.push(current.clone());
    }
    out
}

/// One painted row. Directories carry their expanded state so the list can
/// draw the right chevron without re-querying the set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RowKind {
    Dir { expanded: bool },
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub kind: RowKind,
    pub badge: Option<StatusBadge>,
    /// A directory holding a changed descendant.
    pub dirty: bool,
}

impl Row {
    pub fn is_dir(&self) -> bool {
        matches!(self.kind, RowKind::Dir { .. })
    }

    pub fn expanded(&self) -> bool {
        matches!(self.kind, RowKind::Dir { expanded: true })
    }
}

/// Flatten the tree into the rows the panel paints. With an empty filter this
/// is a plain expand/collapse walk; a non-empty filter reveals only matching
/// files, their ancestor directories, and — when a directory name matches —
/// that directory's whole subtree.
pub fn visible_rows(index: &TreeIndex, expanded: &HashSet<String>, filter: &str) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    let mut rows = Vec::new();
    if needle.is_empty() {
        for child in &index.root.children {
            push_expanded(index, child, 0, expanded, &mut rows);
        }
    } else {
        for child in &index.root.children {
            push_matching(index, child, 0, &needle, &mut rows);
        }
    }
    rows
}

fn push_expanded(
    index: &TreeIndex,
    node: &Node,
    depth: usize,
    expanded: &HashSet<String>,
    out: &mut Vec<Row>,
) {
    let is_expanded = node.is_dir && expanded.contains(&node.path);
    out.push(row_for(index, node, depth, is_expanded));
    if is_expanded {
        for child in &node.children {
            push_expanded(index, child, depth + 1, expanded, out);
        }
    }
}

fn push_matching(
    index: &TreeIndex,
    node: &Node,
    depth: usize,
    needle: &str,
    out: &mut Vec<Row>,
) -> bool {
    let self_match = node.name.to_lowercase().contains(needle);
    if node.is_dir {
        if self_match {
            out.push(row_for(index, node, depth, true));
            for child in &node.children {
                push_all(index, child, depth + 1, out);
            }
            return true;
        }
        let mut child_rows = Vec::new();
        let mut any = false;
        for child in &node.children {
            any |= push_matching(index, child, depth + 1, needle, &mut child_rows);
        }
        if any {
            out.push(row_for(index, node, depth, true));
            out.extend(child_rows);
        }
        any
    } else if self_match {
        out.push(row_for(index, node, depth, false));
        true
    } else {
        false
    }
}

fn push_all(index: &TreeIndex, node: &Node, depth: usize, out: &mut Vec<Row>) {
    out.push(row_for(index, node, depth, node.is_dir));
    for child in &node.children {
        push_all(index, child, depth + 1, out);
    }
}

fn row_for(index: &TreeIndex, node: &Node, depth: usize, expanded: bool) -> Row {
    Row {
        path: node.path.clone(),
        name: node.name.clone(),
        depth,
        kind: if node.is_dir {
            RowKind::Dir { expanded }
        } else {
            RowKind::File
        },
        badge: index.badges.get(&node.path).copied(),
        dirty: index.dirty_dirs.contains(&node.path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(dirs: &[&str], files: &[&str]) -> Snapshot {
        Snapshot {
            dirs: dirs.iter().map(|s| s.to_string()).collect(),
            files: files.iter().map(|s| s.to_string()).collect(),
            truncated: false,
        }
    }

    fn index() -> TreeIndex {
        TreeIndex::build(
            snapshot(
                &["src", "src/app", "empty"],
                &["README.md", "src/main.rs", "src/app/view.rs"],
            ),
            HashMap::from([("src/main.rs".to_string(), StatusBadge::Modified)]),
        )
    }

    fn paths(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|row| row.path.as_str()).collect()
    }

    /// The fast path in `insert` only holds when paths are sorted by component.
    /// A sibling that already exists must be the *last* child, and a plain byte
    /// sort does not deliver that: `/` sorts after `-` and `.`, so `a/b-c` would
    /// come before `a/b/d` and `a` would gain a second `b`.
    #[test]
    fn insert_does_not_duplicate_a_sibling_around_the_separator() {
        let index = TreeIndex::build(
            snapshot(&["a/b", "a/b-c"], &["a/b/d/x.rs", "a/b-c/y.rs"]),
            HashMap::new(),
        );
        let names: Vec<&str> = index
            .root
            .children
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        assert_eq!(names, vec!["a"]);

        let under_a = &index.root.children[0].children;
        let names: Vec<&str> = under_a.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, vec!["b", "b-c"]);

        // `a/b` holds both the synthesized `d` and nothing else.
        let names: Vec<&str> = under_a[0]
            .children
            .iter()
            .map(|node| node.name.as_str())
            .collect();
        assert_eq!(names, vec!["d"]);
    }

    /// A path listed as both a directory and a file resolves to a file, which is
    /// what the old two-pass build did: the file list went in last and overwrote
    /// the directory flag.
    #[test]
    fn a_path_that_is_both_a_dir_and_a_file_resolves_to_a_file() {
        let index = TreeIndex::build(snapshot(&["a", "a/b"], &["a/b"]), HashMap::new());
        let b = &index.root.children[0].children[0];
        assert_eq!(b.name, "b");
        assert!(!b.is_dir);
    }

    /// The wide-directory shape is why `insert` stopped scanning siblings. A
    /// duplicate here would mean the fast path is wrong.
    #[test]
    fn a_wide_directory_builds_one_node_per_entry() {
        let files: Vec<String> = (0..20_000)
            .map(|ix| format!("assets/file-{ix:05}.png"))
            .collect();
        let snapshot = Snapshot {
            dirs: vec!["assets".to_string()],
            files,
            truncated: false,
        };
        let index = TreeIndex::build(snapshot, HashMap::new());
        assert_eq!(index.file_count, 20_000);
        assert_eq!(index.root.children.len(), 1);
        assert_eq!(index.root.children[0].children.len(), 20_000);
    }

    /// The sorted `build` has to agree with the naive shape it replaced, so a
    /// merge that dropped or duplicated an entry would show up here.
    #[test]
    fn build_matches_a_naive_reference() {
        let snapshot = snapshot(
            &["src", "src/app", "empty", "a/b", "a/b-c"],
            &[
                "README.md",
                "src/main.rs",
                "src/app/view.rs",
                "a/b/d/x.rs",
                "a/b-c/y.rs",
                "a/z.rs",
            ],
        );
        let index = TreeIndex::build(snapshot.clone(), HashMap::new());

        fn flatten(node: &Node, out: &mut Vec<(String, bool)>) {
            for child in &node.children {
                out.push((child.path.clone(), child.is_dir));
                flatten(child, out);
            }
        }
        let mut built = Vec::new();
        flatten(&index.root, &mut built);

        // Every path in the snapshot, plus the ancestors the walker omitted.
        let mut expected: Vec<String> = snapshot
            .dirs
            .iter()
            .chain(&snapshot.files)
            .cloned()
            .collect();
        for path in snapshot.dirs.iter().chain(&snapshot.files) {
            expected.extend(ancestors(path));
        }
        expected.sort();
        expected.dedup();

        let mut got: Vec<String> = built.iter().map(|(path, _)| path.clone()).collect();
        got.sort();
        assert_eq!(got, expected);

        for (path, is_dir) in &built {
            let listing_only = snapshot.dirs.contains(path);
            if snapshot.files.contains(path) {
                assert!(!is_dir, "{path} is listed as a file");
            } else if listing_only {
                assert!(is_dir, "{path} is listed as a directory");
            }
        }
    }

    #[test]
    fn build_orders_dirs_first_then_case_insensitively() {
        let index = index();
        let names: Vec<&str> = index
            .root
            .children
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        assert_eq!(names, vec!["empty", "src", "README.md"]);
    }

    #[test]
    fn collapsed_root_shows_only_top_level() {
        let index = index();
        let rows = visible_rows(&index, &HashSet::new(), "");
        assert_eq!(paths(&rows), vec!["empty", "src", "README.md"]);
        assert!(rows[1].is_dir());
        assert!(!rows[1].expanded());
        assert!(rows[1].dirty, "src holds a changed file");
        assert_eq!(rows[2].badge, None);
    }

    #[test]
    fn expanding_reveals_children_with_indentation() {
        let index = index();
        let expanded = HashSet::from(["src".to_string()]);
        let rows = visible_rows(&index, &expanded, "");
        assert_eq!(
            paths(&rows),
            vec!["empty", "src", "src/app", "src/main.rs", "README.md"]
        );
        assert_eq!(rows[2].depth, 1);
        assert!(rows[3].badge == Some(StatusBadge::Modified));
        // Depth-0 rows stay out of `src`'s subtree.
        assert_eq!(rows[1].depth, 0);
    }

    #[test]
    fn filter_reveals_matches_and_ancestors() {
        let index = index();
        let rows = visible_rows(&index, &HashSet::new(), "view");
        assert_eq!(paths(&rows), vec!["src", "src/app", "src/app/view.rs"]);
        assert!(
            rows[0].expanded(),
            "ancestor directories are force-expanded"
        );
        assert!(rows[2].badge.is_none());
    }

    #[test]
    fn filter_on_directory_reveals_whole_subtree() {
        let index = index();
        let rows = visible_rows(&index, &HashSet::new(), "app");
        assert_eq!(
            paths(&rows),
            vec!["src", "src/app", "src/app/view.rs"],
            "a matching directory expands its contents"
        );
    }

    #[test]
    fn filter_is_case_insensitive_and_prunes() {
        let index = index();
        let rows = visible_rows(&index, &HashSet::new(), "README");
        assert_eq!(paths(&rows), vec!["README.md"]);
        assert!(visible_rows(&index, &HashSet::new(), "nope").is_empty());
    }

    #[test]
    fn missing_intermediate_dirs_are_synthesized() {
        let index = TreeIndex::build(snapshot(&[], &["a/b/c.rs"]), HashMap::new());
        let rows = visible_rows(&index, &index.all_dir_paths(), "");
        assert_eq!(paths(&rows), vec!["a", "a/b", "a/b/c.rs"]);
    }

    #[test]
    fn dirty_dirs_are_ancestors_of_badged_files() {
        let index = index();
        assert!(index.dirty_dirs.contains("src"));
        assert!(!index.dirty_dirs.contains("empty"));
        assert_eq!(ancestors("a/b/c.rs"), vec!["a", "a/b"]);
    }

    #[test]
    fn all_dir_paths_expands_every_directory() {
        let index = index();
        let all = index.all_dir_paths();
        assert_eq!(all.len(), 3, "src, src/app, empty");
        assert_eq!(visible_rows(&index, &all, "").len(), 6);
    }

    /// Numbers for the build, not a gate. A wide directory is the case that
    /// used to hang the UI thread (one linear scan per inserted sibling), so
    /// it is the shape worth timing:
    ///
    /// ```sh
    /// cargo test -p orbit-pi --bin orbit-pi tree_build -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "benchmark: prints timings, asserts nothing about speed"]
    fn tree_build_costs() {
        fn best_of(label: &str, build: impl Fn() -> Snapshot) {
            let mut best = std::time::Duration::MAX;
            let mut entries = 0;
            for _ in 0..5 {
                let snapshot = build();
                entries = snapshot.len();
                let start = std::time::Instant::now();
                std::hint::black_box(TreeIndex::build(snapshot, HashMap::new()));
                best = best.min(start.elapsed());
            }
            println!("{label}: {entries} entries, best of 5 = {best:?}");
        }

        best_of("wide (1 dir, 20k files)", || Snapshot {
            dirs: vec!["assets".to_string()],
            files: (0..20_000)
                .map(|ix| format!("assets/file-{ix:05}.png"))
                .collect(),
            truncated: false,
        });

        best_of("realistic (2k dirs, 20k files)", || {
            let mut dirs = Vec::new();
            let mut files = Vec::new();
            for dir in 0..2_000 {
                dirs.push(format!("crates/c{dir:04}/src"));
                for file in 0..10 {
                    files.push(format!("crates/c{dir:04}/src/m{file}.rs"));
                }
            }
            Snapshot {
                dirs,
                files,
                truncated: false,
            }
        });
    }

    #[test]
    fn status_badge_reduces_porcelain() {
        let mut row = crate::git::StatusRow {
            path: "x".into(),
            orig_path: None,
            index: ' ',
            worktree: '?',
            staged_additions: 0,
            staged_deletions: 0,
            unstaged_additions: 0,
            unstaged_deletions: 0,
        };
        assert_eq!(StatusBadge::from_status(&row), StatusBadge::Untracked);
        row.worktree = 'M';
        assert_eq!(StatusBadge::from_status(&row), StatusBadge::Modified);
        row.worktree = 'D';
        assert_eq!(StatusBadge::from_status(&row), StatusBadge::Deleted);
    }
}
