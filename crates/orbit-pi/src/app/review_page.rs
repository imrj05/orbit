//! The Review page's controller: open/close, the snapshot the page renders,
//! and the actions it sends back.
//!
//! The page itself ([`crate::review_page`]) is a dumb renderer; everything
//! that touches the review store or Git lives here.

use std::sync::Arc;

use super::*;
use crate::ai_review::ReviewKind;
use crate::git;
use crate::review_page::{
    ChangedFile, CommitRow, ModelOption, ReviewPageAction, ReviewPageSnapshot, TargetFacts,
    WorkspaceOption,
};

/// The commits shown in the New review tab's inline list.
const COMMIT_ROWS: usize = 8;

/// The Review page's Git facts, collected off the UI thread and cached on the
/// app. Collecting Git inside layout would block a frame, so the page reads
/// this cache and a background task refreshes it.
#[derive(Clone, Debug, Default)]
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
    /// The parsed diff for the Changes tab's per-file preview. Compare with
    /// `Arc::ptr_eq`, not `==`: the snapshot is large and has no cheap equality.
    pub snapshot: Option<Arc<crate::review::Snapshot>>,
    /// `REVIEW_GUIDELINES.md` contents, when the workspace has one. Appended
    /// verbatim to every review prompt for that workspace.
    pub guidelines: Option<String>,
}

impl PartialEq for ReviewPageFacts {
    fn eq(&self, other: &Self) -> bool {
        self.workspace == other.workspace
            && self.facts == other.facts
            && self.commits == other.commits
            && self.changed_files == other.changed_files
            && self.kind == other.kind
            && self.guidelines == other.guidelines
            && match (&self.snapshot, &other.snapshot) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                _ => false,
            }
    }
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
        let Some(workspace) = self
            .review_workspace
            .clone()
            .or_else(|| self.current_workspace.clone())
        else {
            return;
        };
        // Never read the page here: this runs from the page's own action
        // callback, while that entity is leased. The caller passes the kind.
        let kind = kind.or_else(|| self.review_facts.kind.clone());
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
                        match git::collect_kind_diff(&workspace, kind) {
                            Ok(diff) => {
                                // The page parses the collected diff exactly like
                                // the pane does, so the preview and the pane show
                                // the same rows for the same target.
                                let snapshot = crate::review::parse_collected(
                                    crate::review::Source::Uncommitted,
                                    &diff.numstat,
                                    &diff.patch,
                                    diff.complete_context,
                                );
                                facts.changed_files = snapshot
                                    .files
                                    .iter()
                                    .map(|file| ChangedFile {
                                        path: file.path.clone(),
                                        status: 'M',
                                        added: file.additions,
                                        deleted: file.deletions,
                                    })
                                    .collect();
                                facts.snapshot = Some(Arc::new(snapshot));
                            }
                            Err(_) => {
                                facts.changed_files = Vec::new();
                                facts.snapshot = None;
                            }
                        }
                    }
                    facts.guidelines = load_review_guidelines(&workspace);
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
        snapshot: None,
        guidelines: None,
    }
}

impl OrbitApp {
    /// Open the Review page. One main-area feature at a time.
    pub(super) fn open_review_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The form starts on the open workspace; the page can re-point it.
        if self.review_workspace.is_none() {
            self.review_workspace = self.current_workspace.clone();
        }
        self.git_open = false;
        self.usage_open = false;
        self.session_details_open = false;
        self.close_files(cx);
        // Read the target kind before leasing the page: the collection runs
        // after, but the page is the authority on which target it shows.
        let kind = self.review_page.read(cx).target_kind();
        self.review_page.update(cx, |page, cx| {
            page.open(window, cx);
            cx.notify();
        });
        self.refresh_review_facts(kind, cx);
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
        let workspace = self
            .review_workspace
            .clone()
            .or_else(|| self.current_workspace.clone())
            .unwrap_or_default();
        let config = self.review_config();
        ReviewPageSnapshot {
            workspace: workspace
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            workspace_path: workspace.to_string_lossy().into_owned(),
            workspaces: self.review_workspace_options(),
            models: self.review_model_options(),
            thinking_levels: self.review_thinking_levels(),
            model: config.model.clone().unwrap_or_default(),
            provider: config.provider.clone().unwrap_or_default(),
            thinking: config.thinking.clone().unwrap_or_default(),
            branches: self.review_branches(),
            target: self.review_page.read(cx).target_label(),
            facts: facts.facts,
            commits: facts.commits,
            changed_files: facts.changed_files,
            snapshot: facts.snapshot.clone(),
            guidelines: facts.guidelines.clone(),
            runs: self.reviews.runs().to_vec(),
            error: self.reviews_error.clone(),
        }
    }

    /// The repositories the review form can target: every tracked workspace
    /// plus the current one, the review's own choice marked.
    fn review_workspace_options(&self) -> Vec<WorkspaceOption> {
        let mut paths = self.workspaces.clone();
        if let Some(current) = self.current_workspace.as_ref() {
            if !paths.iter().any(|path| path == current) {
                paths.push(current.clone());
            }
        }
        let selected = self.review_workspace.clone();
        paths
            .into_iter()
            .map(|path| {
                let label = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                WorkspaceOption {
                    is_current: selected.as_deref() == Some(path.as_path()),
                    path: path.to_string_lossy().into_owned(),
                    label,
                }
            })
            .collect()
    }

    /// The models the reviewer can run on, current first.
    fn review_model_options(&self) -> Vec<ModelOption> {
        let selected = self.review_config();
        let mut options: Vec<ModelOption> = self
            .available_models
            .iter()
            .map(|model| ModelOption {
                id: model.id.clone(),
                provider: model.provider.clone(),
                label: model.name.clone(),
            })
            .collect();
        options.sort_by(|a, b| {
            let a_selected = selected.model.as_deref() == Some(a.id.as_str());
            let b_selected = selected.model.as_deref() == Some(b.id.as_str());
            b_selected.cmp(&a_selected).then_with(|| a.label.cmp(&b.label))
        });
        options
    }

    /// The selected model's thinking levels, from the catalog entry.
    fn review_thinking_levels(&self) -> Vec<String> {
        let selected = self.review_config();
        self.available_models
            .iter()
            .find(|model| selected.model.as_deref() == Some(model.id.as_str()))
            .map(|model| model.thinking_levels.clone())
            .unwrap_or_else(|| self.available_thinking_levels.clone())
    }

    /// Local branches for the branch target's base picker.
    fn review_branches(&self) -> Vec<String> {
        let Some(workspace) = self.review_workspace.as_ref() else {
            return Vec::new();
        };
        git::list_branches(workspace).unwrap_or_default()
    }

    /// The page's action callback: start a run, stop one, or leave.
    pub(super) fn review_page_action(&self, cx: &Context<Self>) -> crate::review_page::ReviewAction {
        let this = cx.weak_entity();
        Rc::new(move |action, _window, cx| match action {
            ReviewPageAction::Start {
                kind,
                workspace,
                model,
                provider,
                thinking,
            } => {
                let _ = this.update(cx, |app, cx| {
                    app.set_review_selection(&workspace, &model, &provider, &thinking);
                    app.start_review(kind, cx);
                });
            }
            ReviewPageAction::Cancel(id) => {
                let _ = this.update(cx, |app, cx| app.cancel_review(id, cx));
            }
            ReviewPageAction::TargetChanged(kind) => {
                let _ = this.update(cx, |app, cx| app.refresh_review_facts(Some(kind), cx));
            }
            ReviewPageAction::WorkspaceChanged(path, kind) => {
                let _ = this.update(cx, |app, cx| {
                    app.review_workspace = Some(PathBuf::from(path));
                    // Force a recollect: the cached facts describe the old repo.
                    app.review_facts = Default::default();
                    app.refresh_review_facts(Some(kind), cx);
                });
            }
            ReviewPageAction::ModelChanged { id, provider } => {
                let _ = this.update(cx, |app, cx| {
                    app.review_model = Some((id, provider));
                    // The ladder belongs to the model, so re-default the level.
                    app.review_thinking = None;
                    cx.notify();
                });
            }
            ReviewPageAction::ThinkingChanged(level) => {
                let _ = this.update(cx, |app, cx| {
                    app.review_thinking = Some(level);
                    cx.notify();
                });
            }
            ReviewPageAction::BaseChanged(_) => {
                let _ = this.update(cx, |app, cx| app.refresh_review_facts(None, cx));
            }
            ReviewPageAction::Refresh(kind) => {
                let _ = this.update(cx, |app, cx| {
                    app.review_facts = Default::default();
                    app.refresh_review_facts(Some(kind), cx);
                });
            }
        })
    }
}

/// Read `REVIEW_GUIDELINES.md` from the nearest ancestor that has a `.pi`
/// directory (the same rule the reference extension uses). Missing or empty
/// files mean no guidelines; a workspace is never left without a reviewer
/// because the file could not be read.
fn load_review_guidelines(workspace: &Path) -> Option<String> {
    let mut dir = workspace.to_path_buf();
    loop {
        if dir.join(".pi").is_dir() {
            let guidelines = dir.join("REVIEW_GUIDELINES.md");
            let text = std::fs::read_to_string(guidelines).ok()?;
            let trimmed = text.trim();
            return (!trimmed.is_empty()).then(|| trimmed.to_string());
        }
        if !dir.pop() {
            return None;
        }
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
