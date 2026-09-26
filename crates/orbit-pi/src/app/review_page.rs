//! The Review page's controller: open/close, the snapshot the page renders,
//! and the actions it sends back.
//!
//! The page itself ([`crate::review_page`]) is a dumb renderer; everything
//! that touches the review store or Git lives here.

use super::*;
use crate::git;
use crate::ai_review::ReviewKind;
use crate::review_page::{
    ChangedFile, CommitRow, ReviewPageAction, ReviewPageSnapshot, TargetFacts,
};

/// The commits shown in the New review tab's inline list.
const COMMIT_ROWS: usize = 8;

/// The Review page's Git facts, collected off the UI thread and cached on the
/// app. Collecting Git inside layout would block a frame, so the page reads
/// this cache and a background task refreshes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ReviewPageFacts {
    /// The workspace the facts describe; a different workspace invalidates
    /// them.
    pub workspace: PathBuf,
    pub facts: TargetFacts,
    pub commits: Vec<CommitRow>,
    /// The changed files of the target the cache was collected for, for the
    /// Changes tab.
    pub changed_files: Vec<ChangedFile>,
    /// The target the changed files belong to; a different target invalidates
    /// them.
    pub kind: Option<ReviewKind>,
}

impl OrbitApp {
    /// Refresh the Review page's Git facts off the UI thread. Called when the
    /// page opens, on every heartbeat while it stays open, and when the
    /// selected target changes. A refresh already describes `kind` in the
    /// current workspace, so this is a no-op then.
    pub(super) fn refresh_review_facts(
        &mut self,
        kind: Option<ReviewKind>,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.current_workspace.clone() else {
            return;
        };
        let kind = kind.or_else(|| self.review_page.read(cx).target_kind());
        if self.review_facts_inflight {
            return;
        }
        if self.review_facts.workspace == workspace && self.review_facts.kind == kind {
            return;
        }
        self.review_facts_inflight = true;
        let _ = cx.spawn(async move |this, cx| {
            let kind_for_files = kind.clone();
            let collected = cx
                .background_executor()
                .spawn(async move {
                    let mut facts = collect_review_facts(&workspace);
                    if let Some(kind) = kind_for_files.as_ref() {
                        facts.changed_files = git::kind_diff_files(&workspace, kind)
                            .map(|files| {
                                files
                                    .into_iter()
                                    .map(|file| ChangedFile {
                                        path: file.path,
                                        status: file.status,
                                        added: file.added,
                                        deleted: file.deleted,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                    }
                    facts.kind = kind_for_files;
                    facts
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.review_facts_inflight = false;
                app.review_facts = collected;
                cx.notify();
            });
        });
    }
}

/// Collect the page's target counts and commit list. Runs on the background
/// executor: every call here shells out to Git.
fn collect_review_facts(workspace: &Path) -> ReviewPageFacts {
    let mut facts = TargetFacts {
        base_branch: default_base_branch(workspace),
        project_files: git::tracked_file_count(workspace),
        ..TargetFacts::default()
    };
    if let Ok(rows) = git::status_rows(workspace) {
        facts.uncommitted_files = Some(rows.len());
        facts.uncommitted_added = rows
            .iter()
            .map(|row| row.staged_additions + row.unstaged_additions)
            .sum();
        facts.uncommitted_deleted = rows
            .iter()
            .map(|row| row.staged_deletions + row.unstaged_deletions)
            .sum();
    }
    // The branch target's count is a full diff against its base; collecting it
    // eagerly would be the expensive part, so the row reports it only once a
    // review has measured it.
    facts.branch_files = None;
    let commits = git::recent_commits_with_stats(workspace, COMMIT_ROWS)
        .into_iter()
        .map(|commit| CommitRow {
            sha: commit.hash,
            short: commit.short,
            subject: commit.subject,
            file_count: commit.file_count,
            added: commit.added,
            deleted: commit.deleted,
        })
        .collect();
    ReviewPageFacts {
        workspace: workspace.to_path_buf(),
        facts,
        commits,
        changed_files: Vec::new(),
        kind: None,
    }
}

impl OrbitApp {
    /// Open the Review page. One main-area feature at a time.
    pub(super) fn open_review_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.git_open = false;
        self.usage_open = false;
        self.session_details_open = false;
        self.close_files(cx);
        self.review_page.update(cx, |page, cx| {
            page.open(window, cx);
            cx.notify();
        });
        self.refresh_review_facts(None, cx);
        cx.notify();
    }

    /// Leave the Review page.
    pub(super) fn close_review_page(&mut self, cx: &mut Context<Self>) {
        self.review_page.update(cx, |page, cx| {
            page.close();
            cx.notify();
        });
        cx.notify();
    }

    /// Sidebar nav row: the Review page is a destination, toggled like Usage.
    pub(super) fn on_review_nav_click(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.review_page.read(cx).is_open() {
            self.close_review_page(cx);
        } else {
            self.open_review_page(window, cx);
        }
    }

    /// The snapshot the Review page renders, computed fresh for the frame.
    /// Facts come from the off-thread cache (see [`ReviewPageFacts`]); runs
    /// come from the store.
    pub(super) fn review_page_snapshot(&self, cx: &Context<Self>) -> ReviewPageSnapshot {
        let facts = self.review_facts.clone();
        ReviewPageSnapshot {
            workspace: self
                .current_workspace
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            facts: facts.facts,
            commits: facts.commits,
            changed_files: facts.changed_files,
            target: self.review_page.read(cx).target_label(),
            runs: self.reviews.runs().to_vec(),
            config_label: self.review_config().label(),
            error: self.reviews_error.clone(),
        }
    }

    /// The page's action callback: start a run, stop one, or leave.
    pub(super) fn review_page_action(&self, cx: &Context<Self>) -> crate::review_page::ReviewAction {
        let this = cx.weak_entity();
        Rc::new(move |action, _window, cx| match action {
            ReviewPageAction::Start(kind) => {
                let _ = this.update(cx, |app, cx| app.start_review(kind, cx));
            }
            ReviewPageAction::Cancel(id) => {
                let _ = this.update(cx, |app, cx| app.cancel_review(id, cx));
            }
            ReviewPageAction::TargetChanged(kind) => {
                let _ = this.update(cx, |app, cx| app.refresh_review_facts(Some(kind), cx));
            }
        })
    }
}

/// The branch the branch review compares against: the conventional default
/// when the repository has one, else `main`.
fn default_base_branch(workspace: &Path) -> String {
    let branches = git::list_branches(workspace).unwrap_or_default();
    ["main", "master"]
        .into_iter()
        .find(|candidate| branches.iter().any(|branch| branch == candidate))
        .unwrap_or("main")
        .to_string()
}
