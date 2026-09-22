//! The **Git page** — a full main-area surface (like Settings) with three tabs
//! and a commit bar.
//!
//! - **Changes**: staged/unstaged file lists with per-file stage, unstage, and
//!   discard; the commit bar combines the branch, a conventional-commit input,
//!   a one-shot generate button, and Commit / Commit and push / Push.
//! - **History**: the current branch's commit log with refs, author, and
//!   relative time.
//! - **Graph**: every local branch drawn as a lane graph over the same commit
//!   metadata, laid out by [`crate::git::layout_graph`].
//!
//! All Git I/O runs on the background executor; the page paints cached state
//! and is owned by [`crate::app::OrbitApp`].

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, fill, hsla, img, point, prelude::*, px, size, AnyElement, Background, Bounds,
    ClickEvent, Context, Entity, Focusable, FontWeight, Hsla, MouseDownEvent, ObjectFit,
    PathBuilder, Pixels, Render, Window,
};

use crate::app::{icon, nerd_font_family};
use crate::commit_message;
use crate::gh;
use crate::git::{self, CommitEntry, StatusRow};
use crate::git_ops::{self, InProgress};
use crate::theme::{self, Theme, ThemeMode};

mod failure;
mod widgets;

use failure::{ActionError, RecoveryAction};
use widgets::{
    action_button, check_box, empty_note, load_more, ref_badge, row_button, spinner, status_color,
};

/// Callback the app installs so a changed-file row can open its diff in the
/// Review pane.
pub type OpenFile = std::rc::Rc<dyn Fn(String, &mut Window, &mut gpui::App)>;

/// Callback the app installs so a history/conflict file can open in the Files
/// editor. The path is absolute; the label is workspace-relative.
pub type OpenPath = std::rc::Rc<dyn Fn(PathBuf, String, &mut Window, &mut gpui::App)>;

/// Callback the app installs so the panel's Back button also leaves the Git
/// page (the app owns that flag, not the panel).
pub type Close = std::rc::Rc<dyn Fn(&mut Window, &mut gpui::App)>;

const HISTORY_PAGE: usize = 40;

/// How long the header refresh button keeps spinning after a click, so a
/// fast git read still reads as acknowledged (the same floor Settings uses).
const REFRESH_FEEDBACK: Duration = Duration::from_millis(650);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitTab {
    Changes,
    History,
    Graph,
    Issues,
    Pulls,
}

/// Which commit-bar action is running, for the button state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitAction {
    Commit,
    CommitAndPush,
    Push,
    Pull,
    Merge,
    Rebase,
    ForcePush,
}

/// A destructive action waiting for the user's confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingConfirm {
    /// Overwrite the remote with the local branch (`--force-with-lease`).
    ForcePush,
    /// Abandon the in-progress merge/rebase/cherry-pick.
    Abort(InProgress),
    /// Delete a branch (force-discarding unmerged commits).
    DeleteBranch(String),
}

/// Which ref picker is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefTarget {
    /// "Merge branch…": merge the chosen ref into the current branch.
    Merge,
    /// "Rebase onto…": rebase the current branch onto the chosen ref.
    Rebase,
}

/// What the branch-name prompt is for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum BranchPrompt {
    /// Create and check out a branch from HEAD.
    New,
    /// Rename the current branch (carries its name).
    Rename(String),
}

/// What to do once the user answers the "stage unstaged changes?" prompt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingAfterStage {
    Generate,
    Commit(GitAction),
}

pub struct GitPanel {
    open: bool,
    tab: GitTab,
    workspace: Option<PathBuf>,
    /// Provider/model used for one-shot commit-message generation.
    provider: String,
    model: String,

    // ── commit bar ──
    message: Entity<crate::composer::ComposerInput>,
    include_unstaged: bool,
    pending: Option<GitAction>,
    generating: bool,
    /// A pending "stage unstaged changes first?" confirmation.
    stage_prompt: Option<PendingAfterStage>,
    /// A destructive action waiting for confirmation (force push / abort).
    pending_confirm: Option<PendingConfirm>,
    /// The last commit-bar action, so a failed one can be retried. Kept even
    /// across failures because the failure banner's Retry reads it.
    last_action: Option<(GitAction, Option<String>)>,
    /// True while a continue/abort/skip runs off-thread.
    operation_busy: bool,

    // ── changes ──
    staged: Vec<StatusRow>,
    unstaged: Vec<StatusRow>,
    changes_loading: bool,
    changes_error: Option<String>,
    /// The last operation failure, painted as a persistent banner above the
    /// tab body. Cleared by a later success or the dismiss button.
    failure: Option<ActionError>,
    /// Path awaiting a discard confirmation.
    pending_discard: Option<String>,
    /// Merge/rebase/cherry-pick/revert/bisect Git has left half-finished, if
    /// any. Drives the operation bar and the conflict recovery actions.
    git_operation: Option<InProgress>,
    /// Files with unresolved conflicts while `git_operation` is active.
    conflicts: Vec<String>,

    // ── history ──
    history: Vec<CommitEntry>,
    history_loading: bool,
    history_error: Option<String>,
    /// The expanded commit's detail (message body + changed files).
    commit_detail: Option<git::CommitDetail>,
    commit_detail_loading: bool,
    /// The commit row currently expanded, if any.
    selected_commit: Option<String>,
    /// Active History filters (all-branches, author, path, grep).
    history_filter: git::HistoryFilter,
    /// An open per-file actions menu: (workspace-relative path, commit hash).
    file_menu: Option<(String, String)>,

    // ── graph (all local branches) ──
    graph: Vec<git::GraphRow>,
    graph_loading: bool,
    graph_error: Option<String>,
    /// Whether the graph also includes remote-tracking branches and tags.
    graph_all_refs: bool,

    // ── refs / branches / stash ──
    /// Merge/rebase targets: local branches, remote-tracking branches, tags.
    refs: Vec<git_ops::RefEntry>,
    /// Which ref picker is open, if any.
    ref_menu: Option<RefTarget>,
    /// How a branch merge combines the target (ff / merge commit / squash).
    merge_mode: git_ops::MergeMode,
    /// The branch-name field used by the New/Rename prompt.
    branch_input: Entity<crate::composer::ComposerInput>,
    /// A pending New/Rename branch prompt.
    branch_prompt: Option<BranchPrompt>,
    /// The stash stack, newest first.
    stashes: Vec<git_ops::StashEntry>,
    /// Whether the Stashes section on the Changes tab is expanded.
    stash_open: bool,

    // ── github (gh CLI) ──
    /// Whether the `gh` binary is installed and runnable.
    gh_installed: bool,
    /// Whether `gh` has a signed-in host.
    gh_authenticated: bool,
    /// Auth error or version line, shown in the Issues/Pulls setup state.
    gh_detail: String,

    // ── header ──
    branch: Option<String>,
    ahead_behind: Option<(usize, usize)>,
    /// Whether HEAD resolves (drives "Publish branch" on a fresh branch).
    has_commits: bool,
    branches: Vec<String>,
    branch_menu_open: bool,
    /// Set when the branch menu is dismissed by an outside mouse-down so the
    /// chip's following mouse-up does not toggle it straight back open.
    menu_dismissed_at: Option<Instant>,
    branch_operation: bool,
    /// Manual-refresh feedback: the header button spins until this instant
    /// (and while the active tab's data is in flight), so a click is always
    /// acknowledged even when the git read finishes inside one frame.
    refresh_spin_until: Option<Instant>,

    /// One-line feedback (success/failure) with a short TTL.
    status: Option<(String, Instant)>,
    /// `origin` mapped to a commit-permalink base (GitHub/GitLab/Bitbucket),
    /// when the repo has one. `None` hides every commit link.
    remote_web: Option<git::RemoteWeb>,
    /// Leading inset for the header, refreshed by the app each render. Wider
    /// when the sessions sidebar is collapsed: the page then owns the window's
    /// left edge, so its Back affordance has to clear the OS window buttons
    /// and the titlebar's left controls overlaid at the same height.
    chrome_leading: f32,
    /// Opens a changed file's diff in the Review pane (installed by the app).
    on_open_file: Option<OpenFile>,
    /// Opens a file in the Files editor (installed by the app).
    on_open_path: Option<OpenPath>,
    /// Leaves the Git page entirely (installed by the app).
    on_close: Option<Close>,
}

impl GitPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let message = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.commit_message_leave_blank_to_generate")
                .with_key_context("Composer Picker")
                .with_max_lines(6)
        });
        let branch_input = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.branch_name_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
        });
        Self {
            open: false,
            tab: GitTab::Changes,
            workspace: None,
            provider: String::new(),
            model: String::new(),
            message,
            include_unstaged: false,
            pending: None,
            generating: false,
            stage_prompt: None,
            pending_confirm: None,
            last_action: None,
            operation_busy: false,
            staged: Vec::new(),
            unstaged: Vec::new(),
            changes_loading: false,
            changes_error: None,
            failure: None,
            pending_discard: None,
            git_operation: None,
            conflicts: Vec::new(),
            history: Vec::new(),
            history_loading: false,
            history_error: None,
            commit_detail: None,
            commit_detail_loading: false,
            selected_commit: None,
            history_filter: git::HistoryFilter::default(),
            file_menu: None,
            graph: Vec::new(),
            graph_loading: false,
            graph_error: None,
            graph_all_refs: false,
            refs: Vec::new(),
            ref_menu: None,
            merge_mode: git_ops::MergeMode::Merge,
            branch_input,
            branch_prompt: None,
            stashes: Vec::new(),
            stash_open: false,
            gh_installed: false,
            gh_authenticated: false,
            gh_detail: String::new(),
            branch: None,
            ahead_behind: None,
            has_commits: false,
            branches: Vec::new(),
            branch_menu_open: false,
            menu_dismissed_at: None,
            branch_operation: false,
            refresh_spin_until: None,
            status: None,
            remote_web: None,
            chrome_leading: 12.,
            on_open_file: None,
            on_open_path: None,
            on_close: None,
        }
    }

    pub fn show(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        self.refresh_all(cx);
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        self.branch_menu_open = false;
        cx.notify();
    }

    /// Reload the page when the workspace changes under it (file edit, stage,
    /// commit, checkout). A no-op while the page is closed.
    ///
    /// Only status + branch are re-read: `refresh_all` grows the History/Graph
    /// page window, and this fires on every debounced tree change, so growing
    /// here would inflate pagination during a run. Those tabs still refresh on
    /// open, on their own actions, and via the panel's refresh button.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.refresh_status(cx);
            self.refresh_branch(cx);
            cx.notify();
        }
    }

    /// Keep the panel's workspace and agent in sync with the app.
    pub fn set_context(
        &mut self,
        workspace: Option<PathBuf>,
        provider: String,
        model: String,
        cx: &mut Context<Self>,
    ) {
        if workspace != self.workspace {
            self.workspace = workspace;
            self.staged.clear();
            self.unstaged.clear();
            self.history.clear();
            self.graph.clear();
            self.remote_web = None;
            self.status = None;
            self.selected_commit = None;
            self.commit_detail = None;
            self.history_filter = git::HistoryFilter::default();
            self.file_menu = None;
            if self.open {
                self.refresh_all(cx);
            }
        }
        self.provider = provider;
        self.model = model;
    }

    /// Install the callback that opens a changed file's diff in Review.
    pub fn set_open_file(&mut self, open: OpenFile) {
        self.on_open_file = Some(open);
    }

    /// Install the callback that opens a workspace file in the Files editor.
    pub fn set_open_path(&mut self, open: OpenPath) {
        self.on_open_path = Some(open);
    }

    /// Set by `OrbitApp` on every render: how far the header's leading edge sits
    /// from the page's left edge. It widens when the sessions sidebar is
    /// collapsed, because the page then spans the window and its own Back
    /// affordance would otherwise sit under the macOS traffic lights (and the
    /// overlaid sidebar/history controls).
    pub fn set_chrome_leading(&mut self, leading: f32, cx: &mut Context<Self>) {
        if (self.chrome_leading - leading).abs() > 0.5 {
            self.chrome_leading = leading;
            cx.notify();
        }
    }

    /// Install the callback that leaves the Git page (Back button).
    pub fn set_on_close(&mut self, close: Close) {
        self.on_close = Some(close);
    }

    fn cwd(&self) -> Option<PathBuf> {
        self.workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
    }

    // ── data loading ───────────────────────────────────────────────────

    fn refresh_all(&mut self, cx: &mut Context<Self>) {
        self.refresh_status(cx);
        self.refresh_branch(cx);
        self.refresh_gh(cx);
        match self.tab {
            GitTab::History => self.refresh_history(cx),
            GitTab::Graph => self.refresh_graph(cx),
            GitTab::Changes | GitTab::Issues | GitTab::Pulls => {}
        }
    }

    /// Refresh from the header button: same reload, plus a short minimum spin
    /// so the click is visibly acknowledged even when the read is instant.
    fn refresh_from_button(&mut self, cx: &mut Context<Self>) {
        self.refresh_all(cx);
        self.refresh_spin_until = Some(Instant::now() + REFRESH_FEEDBACK);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_FEEDBACK).await;
            let _ = this.update(cx, |panel, cx| {
                // A second click extends the floor; only the last timer clears.
                if panel
                    .refresh_spin_until
                    .is_some_and(|until| Instant::now() >= until)
                {
                    panel.refresh_spin_until = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Whether the active tab's data is in flight.
    fn tab_loading(&self) -> bool {
        match self.tab {
            GitTab::Changes => self.changes_loading,
            GitTab::History => self.history_loading,
            GitTab::Graph => self.graph_loading,
            GitTab::Issues | GitTab::Pulls => false,
        }
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.changes_loading = true;
        self.changes_error = None;
        self.spawn_data(
            cx,
            move || {
                let rows = git::status_rows(&cwd)?;
                let operation = git_ops::operation_state(&cwd);
                let conflicts = match operation {
                    Some(_) => git_ops::conflicted_files(&cwd),
                    None => Vec::new(),
                };
                Ok((rows, operation, conflicts))
            },
            |panel, result, cx| {
                panel.changes_loading = false;
                match result {
                    Ok((rows, operation, conflicts)) => {
                        panel.staged = rows.iter().filter(|row| row.staged()).cloned().collect();
                        panel.unstaged =
                            rows.iter().filter(|row| row.unstaged()).cloned().collect();
                        panel.git_operation = operation;
                        panel.conflicts = conflicts;
                    }
                    Err(err) => {
                        panel.changes_error = Some(err);
                        panel.git_operation = None;
                        panel.conflicts.clear();
                    }
                }
                cx.notify();
            },
        );
    }

    fn refresh_branch(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.spawn_data(
            cx,
            move || {
                Ok((
                    git::current_branch(&cwd),
                    git::ahead_behind(&cwd),
                    git::list_branches(&cwd).unwrap_or_default(),
                    git::has_commits(&cwd),
                    git::remote_web(&cwd),
                    git_ops::list_refs(&cwd),
                    git_ops::list_stashes(&cwd),
                ))
            },
            |panel, result, cx| {
                if let Ok((
                    branch,
                    ahead_behind,
                    branches,
                    has_commits,
                    remote_web,
                    refs,
                    stashes,
                )) = result
                {
                    panel.branch = branch;
                    panel.ahead_behind = ahead_behind;
                    panel.branches = branches;
                    panel.has_commits = has_commits;
                    panel.remote_web = remote_web;
                    panel.refs = refs;
                    panel.stashes = stashes;
                }
                cx.notify();
            },
        );
    }

    /// Probe the `gh` CLI (availability + sign-in) for the Issues and Pull
    /// requests tabs. Two quick subprocesses, run off-thread; never blocks a
    /// frame.
    fn refresh_gh(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.spawn_data(
            cx,
            move || Ok(gh::status(&cwd)),
            |panel, result, cx| {
                if let Ok(status) = result {
                    panel.gh_installed = status.installed;
                    panel.gh_authenticated = status.authenticated;
                    panel.gh_detail = status.detail;
                }
                cx.notify();
            },
        );
    }

    fn refresh_history(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.history_loading = true;
        self.history_error = None;
        let limit = self.history.len().max(HISTORY_PAGE) + HISTORY_PAGE;
        let filter = self.history_filter.clone();
        self.spawn_data(
            cx,
            move || git::history_filtered(&cwd, &filter, limit, 0),
            |panel, result, cx| {
                panel.history_loading = false;
                match result {
                    Ok(commits) => panel.history = commits,
                    Err(err) => panel.history_error = Some(err),
                }
                cx.notify();
            },
        );
    }

    /// Expand or collapse a commit's detail panel.
    fn toggle_commit(&mut self, hash: String, cx: &mut Context<Self>) {
        if self.selected_commit.as_deref() == Some(hash.as_str()) {
            self.selected_commit = None;
            self.commit_detail = None;
            cx.notify();
            return;
        }
        self.selected_commit = Some(hash.clone());
        self.commit_detail = None;
        self.commit_detail_loading = true;
        self.file_menu = None;
        cx.notify();
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || git::commit_detail(&cwd, &hash),
            |panel, result, cx| {
                panel.commit_detail_loading = false;
                match result {
                    Ok(detail) => panel.commit_detail = Some(detail),
                    Err(err) => panel.set_failure(err),
                }
                cx.notify();
            },
        );
    }

    /// Filter History to one path. `follow` tracks renames — that is the
    /// "File history" action.
    fn filter_history_path(&mut self, path: String, follow: bool, cx: &mut Context<Self>) {
        self.history_filter.path = Some(path);
        self.history_filter.follow = follow;
        self.selected_commit = None;
        self.commit_detail = None;
        self.file_menu = None;
        self.refresh_history(cx);
        cx.notify();
    }

    fn filter_history_author(&mut self, author: String, cx: &mut Context<Self>) {
        self.history_filter.author = Some(author);
        self.selected_commit = None;
        self.commit_detail = None;
        self.refresh_history(cx);
        cx.notify();
    }

    fn toggle_all_branches(&mut self, cx: &mut Context<Self>) {
        self.history_filter.all_branches = !self.history_filter.all_branches;
        self.refresh_history(cx);
        cx.notify();
    }

    fn clear_history_filter(&mut self, cx: &mut Context<Self>) {
        self.history_filter = git::HistoryFilter::default();
        self.selected_commit = None;
        self.commit_detail = None;
        self.refresh_history(cx);
        cx.notify();
    }

    /// Reveal a workspace-relative path in the OS file manager.
    fn reveal_path(&self, path: &str) {
        let Some(base) = self
            .workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        else {
            return;
        };
        crate::platform::reveal_in_file_manager(&base.join(path));
    }

    fn open_file_menu(&mut self, path: String, hash: String, cx: &mut Context<Self>) {
        self.file_menu = Some((path, hash));
        cx.notify();
    }

    fn close_file_menu(&mut self, cx: &mut Context<Self>) {
        if self.file_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Fetch the all-branches graph. Grows the window by a page each call, the
    /// same way History does, so "Load more" deepens the graph.
    fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.graph_loading = true;
        self.graph_error = None;
        let limit = self.graph.len().max(HISTORY_PAGE) + HISTORY_PAGE;
        let all_refs = self.graph_all_refs;
        self.spawn_data(
            cx,
            move || {
                let commits = git::graph_history(&cwd, limit, all_refs)?;
                Ok(git::layout_graph(&commits))
            },
            |panel, result, cx| {
                panel.graph_loading = false;
                match result {
                    Ok(rows) => panel.graph = rows,
                    Err(err) => panel.graph_error = Some(err),
                }
                cx.notify();
            },
        );
    }

    /// Run blocking Git work off-thread and apply the result on the UI thread.
    fn spawn_data<T: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
        apply: impl FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + Send + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { work() }).await;
            let _ = this.update(cx, |panel, cx| apply(panel, result, cx));
        })
        .detach();
    }

    // ── staging actions ────────────────────────────────────────────────

    fn stage(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || git::stage_paths(&cwd, &[path]),
            |panel, result, cx| panel.after_git(result, &tr!("git_panel.staged"), cx),
        );
    }

    fn unstage(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || git::unstage_paths(&cwd, &[path]),
            |panel, result, cx| panel.after_git(result, &tr!("git_panel.unstaged"), cx),
        );
    }

    fn stage_all(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || git::stage_all(&cwd),
            |panel, result, cx| panel.after_git(result, &tr!("git_panel.staged_all"), cx),
        );
    }

    fn unstage_all(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || git::unstage_all(&cwd),
            |panel, result, cx| panel.after_git(result, &tr!("git_panel.unstaged_all"), cx),
        );
    }

    fn discard(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.pending_discard = None;
        self.spawn_data(
            cx,
            move || git::discard_paths(&cwd, &[path]),
            |panel, result, cx| panel.after_git(result, &tr!("git_panel.discarded_changes"), cx),
        );
    }

    fn after_git(&mut self, result: Result<(), String>, success: &str, cx: &mut Context<Self>) {
        // Staging/discard failures are not commit-bar actions, so a stale
        // Retry from an earlier push/pull must not linger.
        self.last_action = None;
        match result {
            Ok(()) => {
                self.clear_failure();
                self.set_status(success);
            }
            Err(err) => self.set_failure(err),
        }
        self.refresh_status(cx);
        self.refresh_branch(cx);
        if self.tab == GitTab::History {
            self.refresh_history(cx);
        }
        cx.notify();
    }

    fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), Instant::now()));
    }

    /// Record a Git failure so it survives on the page until dismissed or a
    /// later action succeeds. The raw stderr is classified into a readable
    /// title; both it and the full output are shown (and copyable).
    fn set_failure(&mut self, raw: impl Into<String>) {
        let raw = raw.into();
        self.failure = Some(ActionError::from_raw(&raw));
        // The transient success line and the failure banner never co-exist.
        self.status = None;
    }

    fn clear_failure(&mut self) {
        self.failure = None;
    }

    // ── commit flow ────────────────────────────────────────────────────

    fn on_commit(&mut self, action: GitAction, cx: &mut Context<Self>) {
        if self.pending.is_some() || self.generating {
            return;
        }
        let message = self.message.read(cx).text();
        if message.trim().is_empty() {
            self.request_generate(PendingAfterStage::Commit(action), cx);
        } else {
            self.run_git_action(action, Some(message), cx);
        }
    }

    /// The Generate button (and blank-message commits): produce a message.
    fn generate(&mut self, cx: &mut Context<Self>) {
        self.request_generate(PendingAfterStage::Generate, cx);
    }

    /// Generate a message, first asking to stage unstaged changes when there
    /// are any and the user has not already opted into including them.
    fn request_generate(&mut self, pending: PendingAfterStage, cx: &mut Context<Self>) {
        if !self.unstaged.is_empty() && !self.include_unstaged {
            self.stage_prompt = Some(pending);
            cx.notify();
            return;
        }
        self.perform_pending(pending, cx);
    }

    fn perform_pending(&mut self, pending: PendingAfterStage, cx: &mut Context<Self>) {
        match pending {
            PendingAfterStage::Generate => self.generate_then(None, cx),
            PendingAfterStage::Commit(action) => self.generate_then(Some(action), cx),
        }
    }

    /// Prompt choice: stage every change, then run the pending generate.
    fn stage_all_and_generate(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.stage_prompt.take() else {
            return;
        };
        let Some(cwd) = self.cwd() else { return };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { git::stage_all(&cwd) })
                .await;
            let _ = this.update(cx, |panel, cx| {
                match result {
                    Ok(()) => {
                        panel.clear_failure();
                        panel.set_status(tr!("git_panel.staged_all_changes"));
                        panel.refresh_status(cx);
                        panel.perform_pending(pending, cx);
                    }
                    Err(err) => panel.set_failure(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Prompt choice: generate from whatever is already staged.
    fn generate_from_staged(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.stage_prompt.take() {
            self.perform_pending(pending, cx);
        }
        cx.notify();
    }

    /// Dismiss the active modal (stage prompt, confirm, or branch prompt).
    pub fn dismiss_modal(&mut self, cx: &mut Context<Self>) {
        let stage = self.stage_prompt.take().is_some();
        let confirm = self.pending_confirm.take().is_some();
        let branch = self.branch_prompt.take().is_some();
        if stage || confirm || branch {
            cx.notify();
        }
    }

    pub fn has_modal(&self) -> bool {
        self.stage_prompt.is_some()
            || self.pending_confirm.is_some()
            || self.branch_prompt.is_some()
    }

    fn generate_then(&mut self, action: Option<GitAction>, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.generating = true;
        self.set_status(tr!("git_panel.generating_commit_message"));
        cx.notify();
        let provider = self.provider.clone();
        let model = self.model.clone();
        let staged_rows = self.staged.clone();
        let include_unstaged = self.include_unstaged;
        cx.spawn(async move |this, cx| {
            let result: Result<String, String> = cx
                .background_executor()
                .spawn(async move {
                    let staged = git::staged_patch(&cwd)?;
                    let unstaged = if include_unstaged {
                        git::unstaged_patch(&cwd).unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let provider = (!provider.is_empty()).then_some(provider.as_str());
                    let model = (!model.is_empty()).then_some(model.as_str());
                    match commit_message::generate(
                        &cwd,
                        provider,
                        model,
                        &staged,
                        &unstaged,
                        &staged_rows,
                    ) {
                        Ok(message) => Ok(message),
                        Err(_) => Ok(commit_message::heuristic(&staged_rows)),
                    }
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.generating = false;
                match result {
                    Ok(message) => {
                        let len = panel.message.read(cx).text().len();
                        panel
                            .message
                            .update(cx, |input, cx| input.replace_range(0..len, &message, cx));
                        panel.set_status(tr!("git_panel.commit_message_ready"));
                        if let Some(action) = action {
                            panel.run_git_action(action, Some(message), cx);
                            return;
                        }
                    }
                    Err(err) => panel.set_failure(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Run a commit-bar Git action off-thread. `message` is only used by the
    /// commit actions; push/pull ignore it and never touch the message input.
    fn run_git_action(
        &mut self,
        action: GitAction,
        message: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(cwd) = self.cwd() else { return };
        self.pending = Some(action);
        // Remember the attempt so the failure banner's Retry can repeat it.
        self.last_action = Some((action, message.clone()));
        cx.notify();
        let include_unstaged = self.include_unstaged;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let message = message.unwrap_or_default();
                    match action {
                        GitAction::Commit => git::commit(&cwd, &message, include_unstaged),
                        GitAction::CommitAndPush => git::commit(&cwd, &message, include_unstaged)
                            .and_then(|_| git::push(&cwd)),
                        GitAction::Push => git::push(&cwd),
                        GitAction::Pull => git::pull(&cwd),
                        GitAction::Merge => git::merge_upstream(&cwd),
                        GitAction::Rebase => git::rebase_upstream(&cwd),
                        GitAction::ForcePush => git::push_force_with_lease(&cwd),
                    }
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.pending = None;
                match result {
                    Ok(note) => {
                        panel.clear_failure();
                        panel.set_status(note);
                        if matches!(action, GitAction::Commit | GitAction::CommitAndPush) {
                            panel.message.update(cx, |input, cx| input.clear(cx));
                        }
                    }
                    Err(err) => {
                        panel.set_failure(err);
                        // A rejected push means the remote moved ahead of us:
                        // refresh the remote-tracking refs so the commit bar
                        // can offer Pull/Merge instead of another doomed push.
                        if matches!(action, GitAction::Push | GitAction::CommitAndPush) {
                            panel.fetch_remote(cx);
                        }
                    }
                }
                panel.refresh_all(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Best-effort `git fetch` after a rejected push. It only updates
    /// remote-tracking refs, so the ahead/behind counts — and therefore the
    /// Pull/Merge button — reflect what the remote actually has.
    fn fetch_remote(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || git::fetch(&cwd),
            |panel, _result, cx| {
                panel.refresh_branch(cx);
                cx.notify();
            },
        );
    }

    // ── recovery ───────────────────────────────────────────────────────

    /// Run one recovery action offered by the failure banner.
    fn recover(&mut self, action: RecoveryAction, window: &mut Window, cx: &mut Context<Self>) {
        match action {
            RecoveryAction::Pull => self.run_git_action(GitAction::Pull, None, cx),
            RecoveryAction::Merge => self.run_git_action(GitAction::Merge, None, cx),
            RecoveryAction::Rebase => self.run_git_action(GitAction::Rebase, None, cx),
            RecoveryAction::PublishBranch => self.run_git_action(GitAction::Push, None, cx),
            RecoveryAction::ForceWithLease => {
                // Force pushing rewrites the remote; always confirm first.
                self.pending_confirm = Some(PendingConfirm::ForcePush);
                cx.notify();
            }
            RecoveryAction::Retry => {
                if let Some((action, message)) = self.last_action.clone() {
                    self.run_git_action(action, message, cx);
                }
            }
            RecoveryAction::Reauthenticate => {
                // The terminal panel has no "run command" API, so hand the user
                // the exact command and a place to run it rather than fake it.
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("gh auth login".into()));
                self.set_status(tr!("git_panel.reauth_copied"));
                cx.notify();
            }
            RecoveryAction::OpenConflicts => self.open_conflicts(window, cx),
            RecoveryAction::AbortOperation => {
                if let Some(op) = self.git_operation {
                    // Aborting discards resolution work; confirm first.
                    self.pending_confirm = Some(PendingConfirm::Abort(op));
                    cx.notify();
                }
            }
            RecoveryAction::ContinueOperation => self.continue_operation(cx),
            RecoveryAction::SkipOperation => self.skip_operation(cx),
        }
    }

    /// Open the first conflicted file in the Files editor. Conflicts are
    /// resolved there; the operation bar then continues the merge/rebase.
    fn open_conflicts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.conflicts.first().cloned() else {
            self.set_status(tr!("git_panel.no_conflicts"));
            cx.notify();
            return;
        };
        self.open_path(path, window, cx);
    }

    /// Open a workspace-relative path in the Files editor (installed by the
    /// app). No-op when the callback is missing.
    fn open_path(&self, relative: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(on_open) = self.on_open_path.clone() else {
            return;
        };
        let Some(base) = self
            .workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
        else {
            return;
        };
        on_open(base.join(&relative), relative, window, cx);
    }

    fn continue_operation(&mut self, cx: &mut Context<Self>) {
        let Some(op) = self.git_operation else { return };
        self.run_operation(move |cwd| git_ops::continue_operation(cwd, op), cx);
    }

    fn skip_operation(&mut self, cx: &mut Context<Self>) {
        let Some(op) = self.git_operation else { return };
        self.run_operation(move |cwd| git_ops::skip_operation(cwd, op), cx);
    }

    fn abort_operation(&mut self, op: InProgress, cx: &mut Context<Self>) {
        self.run_operation(move |cwd| git_ops::abort_operation(cwd, op), cx);
    }

    /// Run a continue/abort/skip off-thread and reconcile the page with Git's
    /// resulting state.
    fn run_operation<F>(&mut self, work: F, cx: &mut Context<Self>)
    where
        F: FnOnce(&std::path::Path) -> Result<String, String> + Send + 'static,
    {
        let Some(cwd) = self.cwd() else { return };
        self.operation_busy = true;
        self.clear_failure();
        cx.notify();
        self.spawn_data(
            cx,
            move || work(&cwd),
            |panel, result, cx| {
                panel.operation_busy = false;
                match result {
                    Ok(note) => {
                        panel.clear_failure();
                        panel.set_status(note);
                    }
                    Err(err) => panel.set_failure(err),
                }
                panel.refresh_all(cx);
                cx.notify();
            },
        );
    }

    /// Execute the confirmed destructive action.
    fn confirm_pending(&mut self, cx: &mut Context<Self>) {
        let Some(confirm) = self.pending_confirm.take() else {
            return;
        };
        match confirm {
            PendingConfirm::ForcePush => self.run_git_action(GitAction::ForcePush, None, cx),
            PendingConfirm::Abort(op) => self.abort_operation(op, cx),
            PendingConfirm::DeleteBranch(name) => {
                self.run_operation(move |cwd| git_ops::delete_branch(cwd, &name, true), cx)
            }
        }
    }

    // ── merge / rebase / branches / stash ──────────────────────────────

    /// Merge the chosen ref into the current branch.
    fn merge_ref(&mut self, reference: &str, cx: &mut Context<Self>) {
        let reference = reference.to_string();
        let mode = self.merge_mode;
        self.close_ref_picker(cx);
        self.run_operation(move |cwd| git_ops::merge_ref(cwd, &reference, mode), cx);
    }

    /// Rebase the current branch onto the chosen ref. A dirty worktree is
    /// autostashed rather than refused.
    fn rebase_ref(&mut self, reference: &str, cx: &mut Context<Self>) {
        let reference = reference.to_string();
        let autostash = !self.staged.is_empty() || !self.unstaged.is_empty();
        self.close_ref_picker(cx);
        self.run_operation(
            move |cwd| git_ops::rebase_ref(cwd, &reference, autostash),
            cx,
        );
    }

    fn close_ref_picker(&mut self, cx: &mut Context<Self>) {
        self.ref_menu = None;
        self.branch_menu_open = false;
        self.menu_dismissed_at = Some(Instant::now());
        cx.notify();
    }

    /// Open the branch-name prompt, prefilled for a rename.
    fn open_branch_prompt(
        &mut self,
        prompt: BranchPrompt,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = match &prompt {
            BranchPrompt::New => String::new(),
            BranchPrompt::Rename(name) => name.clone(),
        };
        let len = self.branch_input.read(cx).text().len();
        self.branch_input
            .update(cx, |input, cx| input.replace_range(0..len, &text, cx));
        self.branch_prompt = Some(prompt);
        self.branch_menu_open = false;
        self.menu_dismissed_at = Some(Instant::now());
        let focus = self.branch_input.read(cx).focus_handle(cx);
        window.focus(&focus);
        cx.notify();
    }

    /// Create the new branch or apply the rename.
    fn confirm_branch_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(prompt) = self.branch_prompt.take() else {
            return;
        };
        let name = self.branch_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.set_failure(tr!("git.branch_name_empty"));
            cx.notify();
            return;
        }
        match prompt {
            BranchPrompt::New => self.run_operation(
                move |cwd| {
                    git::create_and_checkout_branch(cwd, &name)?;
                    Ok(tr!("git_panel.branch_created", name = name))
                },
                cx,
            ),
            BranchPrompt::Rename(from) => {
                self.run_operation(move |cwd| git_ops::rename_branch(cwd, &from, &name), cx)
            }
        }
    }

    /// Delete a branch, trying the safe `-d` first. An unmerged branch then
    /// asks for confirmation before the forced `-D`.
    fn delete_branch(&mut self, name: String, cx: &mut Context<Self>) {
        self.branch_menu_open = false;
        self.menu_dismissed_at = Some(Instant::now());
        let Some(cwd) = self.cwd() else { return };
        let for_work = name.clone();
        self.spawn_data(
            cx,
            move || git_ops::delete_branch(&cwd, &for_work, false),
            move |panel, result, cx| {
                match result {
                    Ok(note) => {
                        panel.clear_failure();
                        panel.set_status(note);
                    }
                    // The safe delete refused because the branch has unmerged
                    // commits; offer the forced delete behind a confirmation.
                    Err(err) if err.contains("not fully merged") => {
                        panel.pending_confirm = Some(PendingConfirm::DeleteBranch(name.clone()));
                    }
                    Err(err) => panel.set_failure(err),
                }
                panel.refresh_branch(cx);
                cx.notify();
            },
        );
    }

    fn stash_push(&mut self, cx: &mut Context<Self>) {
        // Stash everything, new files included, so a rebase/checkout can start.
        self.run_operation(move |cwd| git_ops::stash_push(cwd, None, true), cx);
    }

    fn stash_pop(&mut self, index: usize, cx: &mut Context<Self>) {
        self.run_operation(move |cwd| git_ops::stash_pop(cwd, index), cx);
    }

    fn stash_apply(&mut self, index: usize, cx: &mut Context<Self>) {
        self.run_operation(move |cwd| git_ops::stash_apply(cwd, index), cx);
    }

    fn stash_drop(&mut self, index: usize, cx: &mut Context<Self>) {
        self.run_operation(move |cwd| git_ops::stash_drop(cwd, index), cx);
    }

    fn toggle_branch_menu(&mut self, cx: &mut Context<Self>) {
        // mouse-down-out closes the menu; the chip's mouse-up would otherwise
        // toggle it open again on the same click (see AGENT.md popovers).
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        // A click on the chip closes the ref picker and toggles the branch menu.
        self.ref_menu = None;
        self.branch_menu_open = !self.branch_menu_open;
        cx.notify();
    }

    fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) {
        if self.branch_menu_open {
            self.branch_menu_open = false;
            self.menu_dismissed_at = Some(Instant::now());
            cx.notify();
        }
    }

    fn checkout_branch(&mut self, branch: String, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.branch_menu_open = false;
        self.menu_dismissed_at = Some(Instant::now());
        self.branch_operation = true;
        cx.notify();
        self.spawn_data(
            cx,
            move || git::checkout_branch(&cwd, &branch),
            |panel, result, cx| {
                panel.branch_operation = false;
                match result {
                    Ok(()) => {
                        panel.clear_failure();
                        panel.set_status(tr!("git_panel.branch_checked_out"));
                    }
                    Err(err) => {
                        panel.last_action = None;
                        panel.set_failure(err)
                    }
                }
                panel.refresh_branch(cx);
                panel.refresh_status(cx);
                cx.notify();
            },
        );
    }

    // ── rendering ──────────────────────────────────────────────────────

    fn header(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let branch = self
            .branch
            .clone()
            .unwrap_or_else(|| tr!("git_panel.detached"));
        let (additions, deletions) = self.stats();
        div()
            .h(px(44.))
            .flex_none()
            // The page spans the window when the sessions sidebar is collapsed,
            // so the leading inset clears the macOS traffic lights and the
            // sidebar/history controls overlaid in the titlebar.
            .pl(px(self.chrome_leading))
            .pr(px(12.))
            // The Git page spans the window, so its header's right end (branch
            // chip, ±stats, search) clears the app's caption buttons.
            .when(crate::platform::draws_window_controls(), |row| {
                row.pr(px(crate::platform::WINDOW_CONTROLS_W))
            })
            .flex()
            .items_center()
            .gap(theme.space(8.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("git-back")
                    .px(px(8.))
                    .h(px(28.))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        // Close the panel here (we already hold it), then tell
                        // the app to leave the Git page. Calling back into the
                        // panel from that callback would double-borrow it.
                        this.open = false;
                        cx.notify();
                        if let Some(on_close) = this.on_close.clone() {
                            on_close(window, cx);
                        }
                    }))
                    .child(icon("icons/arrow-left.svg", 14., theme.text_2))
                    .child(
                        div()
                            .text_size(theme.ui_px(12.5))
                            .text_color(theme.text_2)
                            .child(tr!("git_panel.back")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon("icons/git-commit.svg", 15., theme.text_2))
                    .child(
                        div()
                            .text_size(theme.ui_px(15.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(tr!("git_panel.title")),
                    ),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("git-branch-chip")
                    .h(px(28.))
                    .px(px(8.))
                    .rounded_md()
                    .border_1()
                    .border_color(theme.border)
                    .bg(if self.branch_menu_open {
                        theme.active
                    } else {
                        theme.bg_raised
                    })
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_branch_menu(cx);
                    }))
                    .child(icon(
                        "icons/branch.svg",
                        12.,
                        if self.branch_menu_open {
                            theme.active_fg
                        } else {
                            theme.text_2
                        },
                    ))
                    .child(
                        div()
                            .max_w(px(180.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(theme.ui_px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(branch),
                    )
                    .child(icon("icons/chevron-down.svg", 10., theme.text_3)),
            )
            .children(self.ahead_behind.and_then(|(ahead, behind)| {
                (ahead > 0 || behind > 0).then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(theme.ui_px(11.5))
                        .child(div().text_color(theme.text_3).child(format!("↑{ahead}")))
                        .child(div().text_color(theme.text_3).child(format!("↓{behind}")))
                })
            }))
            .children((additions > 0 || deletions > 0).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(theme.ui_px(11.5))
                    .font_weight(FontWeight::MEDIUM)
                    .child(
                        div()
                            .text_color(theme.add_green)
                            .child(format!("+{additions}")),
                    )
                    .child(
                        div()
                            .text_color(theme.del_red)
                            .child(format!("-{deletions}")),
                    )
            }))
            .child(
                div()
                    .id("git-refresh")
                    .p_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.refresh_from_button(cx);
                    }))
                    .child(
                        if (self.refresh_spin_until.is_some() || self.tab_loading())
                            && !theme.ui.reduce_motion
                        {
                            spinner("git-refresh-spinner", 13., theme)
                        } else {
                            icon("icons/refresh.svg", 13., theme.text_3).into_any_element()
                        },
                    ),
            )
            .into_any_element()
    }

    fn stats(&self) -> (u64, u64) {
        let mut additions = 0;
        let mut deletions = 0;
        for row in &self.staged {
            additions += row.staged_additions;
            deletions += row.staged_deletions;
        }
        for row in &self.unstaged {
            additions += row.unstaged_additions;
            deletions += row.unstaged_deletions;
        }
        (additions, deletions)
    }

    fn tab_bar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut tabs = vec![
            (
                GitTab::Changes,
                "icons/git-compare.svg",
                "git_panel.tab_changes",
            ),
            (GitTab::History, "icons/clock.svg", "git_panel.tab_history"),
            (GitTab::Graph, "icons/git-fork.svg", "git_panel.tab_graph"),
        ];
        // Host tabs only appear when `gh` exists, so a repo without it keeps
        // the page exactly as it was. Sign-in state is handled inside the tab.
        if self.gh_installed {
            tabs.push((GitTab::Issues, "icons/github.svg", "git_panel.tab_issues"));
            tabs.push((
                GitTab::Pulls,
                "icons/git-pull-request.svg",
                "git_panel.tab_pulls",
            ));
        }
        div()
            .h(px(40.))
            .flex_none()
            .px(theme.space(16.))
            .flex()
            .items_center()
            .gap(theme.space(6.))
            .border_b_1()
            .border_color(theme.border)
            .children(tabs.into_iter().map(|(tab, tab_icon, key)| {
                let label = tr!(key);
                let selected = self.tab == tab;
                div()
                    .id(gpui::ElementId::Name(
                        format!("git-tab-{}", key.rsplit('.').next().unwrap_or(key)).into(),
                    ))
                    .h(px(28.))
                    .px(px(10.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(theme.ui_px(12.5))
                    .font_weight(if selected {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .when(selected, |tab| {
                        tab.bg(theme.active).text_color(theme.active_fg)
                    })
                    .when(!selected, |tab| {
                        tab.text_color(theme.text_3)
                            .hover(|s| s.bg(theme.bg_hover).text_color(theme.text_2))
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if this.tab != tab {
                            this.tab = tab;
                        }
                        this.branch_menu_open = false;
                        this.refresh_all(cx);
                        cx.notify();
                    }))
                    .child(icon(
                        tab_icon,
                        13.,
                        if selected {
                            theme.active_fg
                        } else {
                            theme.text_3
                        },
                    ))
                    .child(label.to_string())
            }))
            // right-aligned status text
            .child(div().flex_1())
            .children(self.status.as_ref().map(|(message, _)| {
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme.ui_px(11.5))
                    .text_color(theme.text_3)
                    .child(message.clone())
            }))
            .into_any_element()
    }

    /// The persistent failure banner: a friendly title plus the raw command
    /// output, wrapped in full and copyable. It sits under the tab bar on
    /// every tab, so a push/pull/merge failure cannot be clipped to one
    /// truncated line or expire before it is read.
    fn failure_banner(&self, theme: Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let failure = self.failure.as_ref()?;
        let copy_text = format!("{}: {}", failure.title, failure.detail);
        let mut actions = failure.recovery_actions();
        // Retry only makes sense when there is an attempt to repeat.
        if self.last_action.is_none() {
            actions.retain(|action| *action != RecoveryAction::Retry);
        }
        Some(
            div()
                .id("git-failure")
                .flex_none()
                .mx(theme.space(20.))
                .mt(theme.space(12.))
                .bg(theme.crit.opacity(0.1))
                .border_1()
                .border_color(theme.crit.opacity(0.45))
                .rounded_lg()
                .px(px(12.))
                .py(px(9.))
                .flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .mt(px(1.))
                        .child(icon("icons/stop.svg", 15., theme.crit)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(
                            div()
                                .text_size(theme.ui_px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(failure.title.clone()),
                        )
                        .child(
                            div()
                                .id("git-failure-detail")
                                .max_h(px(160.))
                                .overflow_y_scroll()
                                .text_size(theme.ui_px(11.5))
                                .line_height(theme.ui_px(16.))
                                .text_color(theme.text_2)
                                .whitespace_normal()
                                .child(failure.detail.clone()),
                        )
                        .when(!actions.is_empty(), |column| {
                            column.child(
                                div().flex().flex_wrap().gap(px(6.)).pt(px(3.)).children(
                                    actions
                                        .into_iter()
                                        .map(|action| recovery_button(action, theme, cx)),
                                ),
                            )
                        }),
                )
                .child(
                    div()
                        .id("git-failure-copy")
                        .flex_none()
                        .size(px(20.))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_color(theme.text_2)
                        .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    copy_text.clone(),
                                ));
                            }),
                        )
                        .child(icon("icons/copy.svg", 12., theme.text_2)),
                )
                .child(
                    div()
                        .id("git-failure-dismiss")
                        .flex_none()
                        .size(px(20.))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_size(theme.ui_px(13.))
                        .text_color(theme.text_2)
                        .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.clear_failure();
                                cx.notify();
                            }),
                        )
                        .child("×"),
                )
                .into_any_element(),
        )
    }

    /// The single "operation in progress" strip: which merge/rebase/cherry-pick
    /// Git is waiting on, the conflicting files, and Continue / Skip / Abort.
    /// It is the page's only source of truth for "you are mid-merge".
    fn operation_bar(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let op = self.git_operation?;
        let conflicts = self.conflicts.len();
        let can_continue = conflicts == 0 && !self.operation_busy;
        let count_label = if conflicts == 1 {
            tr!("git_panel.op_conflicts_one", count = conflicts)
        } else {
            tr!("git_panel.op_conflicts_other", count = conflicts)
        };

        let mut buttons = div().flex().items_center().gap(px(6.)).flex_none();
        if op.can_skip() {
            buttons = buttons.child(action_button(
                "git-op-skip",
                &tr!("git_panel.op_skip"),
                None,
                false,
                self.operation_busy,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.skip_operation(cx)),
            ));
        }
        buttons = buttons
            .child(action_button(
                "git-op-abort",
                &tr!("git_panel.op_abort"),
                None,
                false,
                self.operation_busy,
                theme,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pending_confirm = Some(PendingConfirm::Abort(op));
                    cx.notify();
                }),
            ))
            .child(action_button(
                "git-op-continue",
                &tr!("git_panel.op_continue"),
                Some(if self.operation_busy {
                    spinner("git-op-spinner", 12., theme)
                } else {
                    icon("icons/check.svg", 12., theme.send_fg).into_any_element()
                }),
                true,
                !can_continue,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.continue_operation(cx)),
            ));

        Some(
            div()
                .id("git-operation")
                .flex_none()
                .mx(theme.space(20.))
                .mt(theme.space(12.))
                .bg(theme.warn.opacity(0.1))
                .border_1()
                .border_color(theme.warn.opacity(0.45))
                .rounded_lg()
                .px(px(12.))
                .py(px(9.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(icon("icons/git-merge.svg", 15., theme.warn))
                        .child(
                            div()
                                .text_size(theme.ui_px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(tr!(op.label_key())),
                        )
                        .children((conflicts > 0).then(|| {
                            div()
                                .text_size(theme.ui_px(11.5))
                                .text_color(theme.text_2)
                                .child(count_label)
                        }))
                        .child(div().flex_1())
                        .child(buttons),
                )
                .children((!self.conflicts.is_empty()).then(|| {
                    div().flex().flex_wrap().gap(px(6.)).children(
                        self.conflicts.iter().take(8).map(|path| {
                            let target = path.clone();
                            div()
                                .id(gpui::ElementId::Name(format!("git-conflict-{path}").into()))
                                .h(px(22.))
                                .px(px(7.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.bg_raised)
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .text_size(theme.ui_px(11.))
                                .text_color(theme.text_2)
                                .hover(|s| s.bg(theme.bg_hover).text_color(theme.text))
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.open_path(target.clone(), window, cx)
                                }))
                                .child(path.clone())
                        }),
                    )
                }))
                .into_any_element(),
        )
    }

    fn changes_tab(&self, theme: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.commit_bar(theme, window, cx))
            .child(
                div()
                    .id("git-changes-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .pb(theme.space(20.))
                    .children(self.changes_body(theme, cx)),
            )
            .into_any_element()
    }

    fn commit_bar(&self, theme: Theme, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.pending.is_some() || self.generating || self.operation_busy;
        let has_changes = !self.staged.is_empty() || !self.unstaged.is_empty();
        let can_commit = !busy
            && (!self.staged.is_empty() || (self.include_unstaged && !self.unstaged.is_empty()));
        let (ahead, has_upstream) = match self.ahead_behind {
            Some((ahead, _)) => (ahead, true),
            None => (0, false),
        };
        let behind = self.ahead_behind.map(|(_, behind)| behind).unwrap_or(0);
        let actions = bar_actions(has_changes, ahead, has_upstream, self.has_commits, behind);
        let message_focused = self.message.read(cx).focus_handle(cx).is_focused(window);
        let commit_and_push_label = tr!("git_panel.commit_and_push");
        let commit_label = if self.generating {
            tr!("git_panel.generating_short")
        } else {
            tr!("git_panel.commit")
        };
        let push_label = if matches!(actions, BarActions::Push { publish: true }) {
            tr!("git_panel.publish_branch")
        } else {
            tr!("git_panel.push")
        };
        div()
            .flex_none()
            .px(theme.space(20.))
            .pt(theme.space(16.))
            .pb(theme.space(16.))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .gap(theme.space(12.))
            .child(
                // The message field: wraps and grows to a few rows, then
                // scrolls internally. A fixed min height keeps the bar stable,
                // and the actions sit on their own row below so a long
                // generated body can never overlap them.
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h(px(56.))
                    .px(theme.space(12.))
                    .py(theme.space(10.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if message_focused {
                        theme.border_strong
                    } else {
                        theme.border
                    })
                    .bg(theme.bg_composer)
                    .flex()
                    .child(div().flex_1().min_w_0().child(self.message.clone())),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme.space(12.))
                    .children((!self.unstaged.is_empty()).then(|| {
                        div()
                            .id("git-include-unstaged-row")
                            .h(px(28.))
                            .px(px(6.))
                            .ml(px(-6.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.bg_hover))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.include_unstaged = !this.include_unstaged;
                                cx.notify();
                            }))
                            .child(check_box(self.include_unstaged, theme))
                            .child(
                                div()
                                    .text_size(theme.ui_px(12.))
                                    .text_color(theme.text_2)
                                    .child(tr!("git_panel.include_unstaged_changes")),
                            )
                            .children(self.include_unstaged.then(|| {
                                let (additions, deletions) = self.unstaged_stats();
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(theme.ui_px(11.5))
                                    .child(
                                        div()
                                            .text_color(theme.add_green)
                                            .child(format!("+{additions}")),
                                    )
                                    .child(
                                        div()
                                            .text_color(theme.del_red)
                                            .child(format!("-{deletions}")),
                                    )
                            }))
                    }))
                    .child(div().flex_1())
                    .children(has_changes.then(|| {
                        // Generate sits left of the commit actions on the
                        // footer row, where it no longer competes with the
                        // message field for horizontal space.
                        div()
                            .id("git-generate")
                            .h(px(28.))
                            .px(px(10.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(if self.generating {
                                theme.overlay
                            } else {
                                theme.bg_raised
                            })
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(theme.ui_px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if self.generating {
                                theme.text_3
                            } else {
                                theme.text
                            })
                            .when(!self.generating, |button| {
                                button
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(theme.bg_hover).border_color(theme.border_strong)
                                    })
                                    .on_click(
                                        cx.listener(|this, _: &ClickEvent, _, cx| {
                                            this.generate(cx)
                                        }),
                                    )
                            })
                            .child(if self.generating {
                                spinner("git-generate-spinner", 12., theme)
                            } else {
                                icon("icons/magic-wand.svg", 12., theme.text_2).into_any_element()
                            })
                            .child(if self.generating {
                                tr!("git_panel.generating_short")
                            } else {
                                tr!("git_panel.generate")
                            })
                    }))
                    .child(match actions {
                        BarActions::Commit => div()
                            .flex()
                            .items_center()
                            .gap(theme.space(8.))
                            .child(action_button(
                                "git-commit-push",
                                &commit_and_push_label,
                                Some(
                                    icon("icons/cloud-upload.svg", 13., theme.text_2)
                                        .into_any_element(),
                                ),
                                false,
                                !can_commit,
                                theme,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.on_commit(GitAction::CommitAndPush, cx)
                                }),
                            ))
                            .child(action_button(
                                "git-commit",
                                &commit_label,
                                Some(if self.generating {
                                    spinner("git-commit-spinner", 13., theme)
                                } else {
                                    icon(
                                        "icons/git-commit.svg",
                                        13.,
                                        if can_commit {
                                            theme.send_fg
                                        } else {
                                            theme.text_3
                                        },
                                    )
                                    .into_any_element()
                                }),
                                true,
                                !can_commit,
                                theme,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.on_commit(GitAction::Commit, cx)
                                }),
                            ))
                            .into_any_element(),
                        BarActions::Push { publish: _ } => action_button(
                            "git-push",
                            &push_label,
                            Some(
                                icon(
                                    "icons/upload.svg",
                                    13.,
                                    if busy { theme.text_3 } else { theme.send_fg },
                                )
                                .into_any_element(),
                            ),
                            true,
                            busy,
                            theme,
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.run_git_action(GitAction::Push, None, cx)
                            }),
                        ),
                        BarActions::Pull { behind } => div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(theme.ui_px(11.5))
                                    .text_color(theme.text_3)
                                    .child(tr!("git_panel.behind", count = behind)),
                            )
                            .child(action_button(
                                "git-pull",
                                &tr!("git_panel.pull"),
                                Some(
                                    icon("icons/arrow-down.svg", 13., theme.text_2)
                                        .into_any_element(),
                                ),
                                false,
                                busy,
                                theme,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.run_git_action(GitAction::Pull, None, cx)
                                }),
                            ))
                            .into_any_element(),
                        BarActions::Merge { behind } => div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(theme.ui_px(11.5))
                                    .text_color(theme.text_3)
                                    .child(tr!("git_panel.diverged_behind", count = behind)),
                            )
                            .child(action_button(
                                "git-merge",
                                &tr!("git_panel.merge"),
                                Some(
                                    icon("icons/git-merge.svg", 13., theme.text_2)
                                        .into_any_element(),
                                ),
                                false,
                                busy,
                                theme,
                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                    this.run_git_action(GitAction::Merge, None, cx)
                                }),
                            ))
                            .into_any_element(),
                        BarActions::UpToDate => div()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_3)
                            .child(tr!("git_panel.up_to_date"))
                            .into_any_element(),
                    }),
            )
            .into_any_element()
    }

    fn unstaged_stats(&self) -> (u64, u64) {
        let additions = self.unstaged.iter().map(|row| row.unstaged_additions).sum();
        let deletions = self.unstaged.iter().map(|row| row.unstaged_deletions).sum();
        (additions, deletions)
    }

    fn changes_body(&self, theme: Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if let Some(error) = &self.changes_error {
            return vec![empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.git_unavailable"),
                Some(error),
            )];
        }
        if self.changes_loading && self.staged.is_empty() && self.unstaged.is_empty() {
            return vec![empty_note(
                theme,
                "icons/git-compare.svg",
                &tr!("git_panel.reading_working_tree"),
                None,
            )];
        }
        let has_changes = !self.staged.is_empty() || !self.unstaged.is_empty();
        if !has_changes && self.stashes.is_empty() {
            return vec![empty_note(
                theme,
                "icons/check.svg",
                &tr!("git_panel.working_tree_clean"),
                Some(&tr!("git_panel.nothing_to_commit")),
            )];
        }
        let mut out = Vec::new();
        // Stashes (and the changes that could become one) always render first,
        // so a stash-only working tree still has something to show.
        if has_changes || !self.stashes.is_empty() {
            out.push(self.stash_section(theme, cx));
        }
        if !self.staged.is_empty() {
            out.push(self.section_header(
                theme,
                cx,
                &tr!("git_panel.section_staged"),
                "staged",
                self.staged.len(),
                true,
                Some(cx.listener(|this, _: &ClickEvent, _, cx| this.unstage_all(cx))),
            ));
            out.push(self.change_rows(self.staged.clone(), true, theme, cx));
        }
        if !self.unstaged.is_empty() {
            out.push(self.section_header(
                theme,
                cx,
                &tr!("git_panel.section_changes"),
                "changes",
                self.unstaged.len(),
                false,
                Some(cx.listener(|this, _: &ClickEvent, _, cx| this.stage_all(cx))),
            ));
            out.push(self.change_rows(self.unstaged.clone(), false, theme, cx));
        }
        out
    }

    /// The collapsible Stashes section at the top of the Changes tab.
    fn stash_section(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let count = self.stashes.len();
        let has_changes = !self.staged.is_empty() || !self.unstaged.is_empty();
        let mut header = div()
            .id("git-stash-header")
            .px(theme.space(20.))
            .pt(theme.space(12.))
            .pb(theme.space(8.))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(theme.space(8.))
            .cursor_pointer()
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.stash_open = !this.stash_open;
                cx.notify();
            }))
            .child(icon(
                if self.stash_open {
                    "icons/arrow-down.svg"
                } else {
                    "icons/arrow-right.svg"
                },
                11.,
                theme.text_3,
            ))
            .child(
                div()
                    .text_size(theme.ui_px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child(format!(
                        "{} \u{b7} {count}",
                        tr!("git_panel.stashes").to_uppercase()
                    )),
            )
            .child(div().flex_1());
        if has_changes {
            header = header.child(
                div()
                    .id("git-stash-all")
                    .h(px(28.))
                    .px(theme.space(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(theme.space(4.))
                    .cursor_pointer()
                    .text_size(theme.ui_px(11.5))
                    .text_color(theme.text_2)
                    .hover(|s| s.bg(theme.bg_hover).text_color(theme.text))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.stash_push(cx)))
                    .child(icon("icons/arrow-down.svg", 11., theme.text_3))
                    .child(tr!("git_panel.stash_changes")),
            );
        }
        let mut section = div().flex().flex_col().child(header);
        if self.stash_open {
            if self.stashes.is_empty() {
                section = section.child(
                    div()
                        .px(theme.space(20.))
                        .py(theme.space(10.))
                        .text_size(theme.ui_px(12.))
                        .text_color(theme.text_3)
                        .child(tr!("git_panel.no_stashes")),
                );
            }
            for stash in &self.stashes {
                section = section.child(stash_row(stash, theme, cx));
            }
        }
        section.into_any_element()
    }

    fn change_rows(
        &self,
        files: Vec<StatusRow>,
        staged: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut rows = div()
            .flex()
            .flex_col()
            .pt(theme.space(4.))
            .pb(theme.space(4.));
        for (ix, row) in files.into_iter().enumerate() {
            if ix > 0 {
                rows = rows.child(changes_separator(theme));
            }
            rows = rows.child(self.change_row(row, staged, theme, cx));
        }
        rows.into_any_element()
    }

    fn section_header(
        &self,
        theme: Theme,
        _cx: &Context<Self>,
        label: &str,
        slug: &str,
        count: usize,
        staged_section: bool,
        action: Option<impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static>,
    ) -> AnyElement {
        let mut row = div()
            .px(theme.space(20.))
            .pt(theme.space(20.))
            .pb(theme.space(8.))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(theme.space(8.))
            .child(
                div()
                    .text_size(theme.ui_px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child(format!("{} · {}", label.to_uppercase(), count)),
            )
            .child(div().flex_1());
        if let Some(action) = action {
            row = row.child(
                div()
                    .id(gpui::ElementId::Name(
                        format!("git-section-action-{slug}").into(),
                    ))
                    .h(px(28.))
                    .px(theme.space(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(theme.space(4.))
                    .cursor_pointer()
                    .text_size(theme.ui_px(11.5))
                    .text_color(theme.text_2)
                    .hover(|s| s.bg(theme.bg_hover).text_color(theme.text))
                    .on_click(action)
                    .child(if staged_section {
                        tr!("git_panel.unstage_all")
                    } else {
                        tr!("git_panel.stage_all")
                    }),
            );
        }
        row.into_any_element()
    }

    fn change_row(
        &self,
        row: StatusRow,
        staged: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (badge, color) = if staged {
            (row.staged_badge(), status_color(row.staged_badge(), theme))
        } else {
            (row.change_badge(), status_color(row.change_badge(), theme))
        };
        let (additions, deletions) = if staged {
            (row.staged_additions, row.staged_deletions)
        } else {
            (row.unstaged_additions, row.unstaged_deletions)
        };
        let path = row.path.clone();
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let dir = path
            .rsplit_once('/')
            .map(|(dir, _)| format!("{dir}/"))
            .unwrap_or_default();
        let nerd = nerd_font_family(cx);
        let dark = theme.mode == ThemeMode::Dark;
        let fallback = icon("icons/file.svg", 12., theme.text_3).into_any_element();
        let glyph = crate::app::file_glyph(&path, dark, nerd.as_ref(), 12., fallback);
        let confirm = self.pending_discard.as_deref() == Some(path.as_str());
        let row_path = path.clone();

        let mut actions = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_none()
            .opacity(if confirm { 1. } else { 0. })
            .group_hover("git-row", |s| s.opacity(1.));
        if staged {
            actions = actions.child(row_button(
                format!("git-unstage-{path}"),
                "icons/minus.svg",
                theme,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.unstage(row_path.clone(), cx)),
            ));
        } else {
            actions = actions.child(row_button(
                format!("git-stage-{path}"),
                "icons/plus.svg",
                theme,
                cx.listener(move |this, _: &ClickEvent, _, cx| this.stage(row_path.clone(), cx)),
            ));
            if confirm {
                actions = actions
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                format!("git-discard-yes-{path}").into(),
                            ))
                            .h(px(22.))
                            .px(px(6.))
                            .rounded(px(6.))
                            .bg(theme.crit)
                            .text_size(theme.ui_px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.send_fg)
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .on_click(cx.listener({
                                let p = path.clone();
                                move |this, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    this.discard(p.clone(), cx)
                                }
                            }))
                            .child(tr!("git_panel.discard")),
                    )
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                format!("git-discard-no-{path}").into(),
                            ))
                            .h(px(22.))
                            .px(px(6.))
                            .rounded(px(6.))
                            .text_size(theme.ui_px(11.))
                            .text_color(theme.text_3)
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.overlay))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.pending_discard = None;
                                cx.notify();
                            }))
                            .child(tr!("git_panel.cancel")),
                    );
            } else {
                actions = actions.child(row_button(
                    format!("git-discard-{path}"),
                    "icons/trash.svg",
                    theme,
                    cx.listener({
                        let p = path.clone();
                        move |this, _: &ClickEvent, _, cx| {
                            this.pending_discard = Some(p.clone());
                            cx.notify();
                        }
                    }),
                ));
            }
        }

        div()
            .id(gpui::ElementId::Name(format!("git-row-{path}").into()))
            .group("git-row")
            .mx(theme.space(12.))
            .h(px(32.))
            .px(theme.space(8.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(theme.space(10.))
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if let Some(on_open) = this.on_open_file.clone() {
                    on_open(path.clone(), window, cx);
                }
            }))
            .child(
                div()
                    .w(px(16.))
                    .flex_none()
                    .text_size(theme.ui_px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color)
                    .child(badge.to_string()),
            )
            .child(glyph)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme.ui_px(12.))
                    .child(div().text_color(theme.text_3).child(dir))
                    .child(div().text_color(theme.text).child(name)),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.add_green)
                    .child(format!("+{additions}")),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.del_red)
                    .child(format!("-{deletions}")),
            )
            .child(actions)
            .into_any_element()
    }

    fn history_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(error) = &self.history_error {
            body = body.child(empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.history_unavailable"),
                Some(error),
            ));
        } else if self.history.is_empty() && self.history_loading {
            body = body.child(empty_note(
                theme,
                "icons/clock.svg",
                &tr!("git_panel.reading_history"),
                None,
            ));
        } else if self.history.is_empty() {
            let detail = if self.history_filter.is_active() {
                tr!("git_panel.no_matching_commits")
            } else {
                tr!("git_panel.commits_will_show_up")
            };
            body = body.child(empty_note(
                theme,
                "icons/clock.svg",
                &tr!("git_panel.no_commits_yet"),
                Some(&detail),
            ));
        } else {
            let history_entity = cx.entity();
            let mut list = div().flex().flex_col().py(px(6.));
            let total = self.history.len();
            for (ix, commit) in self.history.iter().enumerate() {
                let url = self
                    .remote_web
                    .as_ref()
                    .map(|remote| remote.commit_url(&commit.hash));
                let selected = self.selected_commit.as_deref() == Some(commit.hash.as_str());
                let on_author = {
                    let entity = history_entity.clone();
                    move |author: String, _window: &mut Window, cx: &mut gpui::App| {
                        entity.update(cx, |panel, cx| panel.filter_history_author(author, cx));
                    }
                };
                list = list.child(commit_row(
                    commit,
                    url.as_deref(),
                    selected,
                    theme,
                    cx.listener({
                        let hash = commit.hash.clone();
                        move |this, _: &ClickEvent, _, cx| this.toggle_commit(hash.clone(), cx)
                    }),
                    on_author,
                ));
                if selected {
                    match self
                        .commit_detail
                        .as_ref()
                        .filter(|detail| detail.hash == commit.hash)
                    {
                        Some(detail) => {
                            list = list.child(self.commit_detail_body(
                                detail,
                                url.as_deref(),
                                theme,
                                cx,
                            ));
                        }
                        None => {
                            list = list.child(detail_note(
                                theme,
                                &tr!(if self.commit_detail_loading {
                                    "git_panel.reading_commit"
                                } else {
                                    "git_panel.commit_unavailable"
                                }),
                            ));
                        }
                    }
                }
                if ix + 1 < total {
                    list = list.child(commit_separator(theme));
                }
            }
            list = list.child(load_more(
                theme,
                self.history_loading,
                cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_history(cx)),
            ));
            body = body.child(
                div()
                    .id("git-history-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.history_filter_bar(theme, cx))
            .child(body)
            .into_any_element()
    }

    /// The History filter chips: all-branches toggle plus removable author and
    /// path (file-history) chips.
    fn history_filter_bar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let filter = &self.history_filter;
        let mut bar = div()
            .id("git-history-filters")
            .flex_none()
            .px(theme.space(16.))
            .py(theme.space(8.))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(px(6.))
            .child(filter_toggle_chip(
                "git-filter-all",
                &tr!("git_panel.all_branches"),
                filter.all_branches,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_all_branches(cx)),
            ));
        if let Some(author) = filter.author.clone() {
            bar = bar.child(filter_chip(
                "git-filter-author",
                &tr!("git_panel.author_chip", name = author),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.history_filter.author = None;
                    this.refresh_history(cx);
                    cx.notify();
                }),
            ));
        }
        if let Some(path) = filter.path.clone() {
            let label = if filter.follow {
                tr!("git_panel.file_history_chip", path = path)
            } else {
                tr!("git_panel.path_chip", path = path)
            };
            bar = bar.child(filter_chip(
                "git-filter-path",
                &label,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.history_filter.path = None;
                    this.history_filter.follow = false;
                    this.refresh_history(cx);
                    cx.notify();
                }),
            ));
        }
        let active = filter.is_active();
        bar.child(div().flex_1())
            .children(active.then(|| {
                filter_chip(
                    "git-filter-clear",
                    &tr!("git_panel.clear_filter"),
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_history_filter(cx)),
                )
            }))
            .into_any_element()
    }

    /// The expanded commit: full message, committer facts, and changed files
    /// with per-file actions.
    fn commit_detail_body(
        &self,
        detail: &git::CommitDetail,
        url: Option<&str>,
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let nerd = nerd_font_family(cx);
        let dark = theme.mode == ThemeMode::Dark;
        let mut column = div()
            .mx(theme.space(12.))
            .mb(theme.space(6.))
            .px(theme.space(12.))
            .py(theme.space(10.))
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .gap(theme.space(8.));
        if !detail.body.is_empty() {
            column = column.child(
                div()
                    .text_size(theme.ui_px(12.))
                    .line_height(theme.ui_px(18.))
                    .text_color(theme.text_2)
                    .whitespace_normal()
                    .child(detail.body.clone()),
            );
        }
        let hash = detail.short.clone();
        let mut meta = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(theme.ui_px(11.))
            .text_color(theme.text_3)
            .child(format!("{} \u{b7} {}", detail.author, detail.author_date))
            .when(detail.committer != detail.author, |row| {
                row.child(format!(
                    "committed by {} \u{b7} {}",
                    detail.committer, detail.committer_date
                ))
            })
            .child(div().flex_1());
        if let Some(url) = url.map(str::to_string) {
            meta = meta.child(row_button(
                "git-detail-open",
                "icons/arrow-up-right.svg",
                theme,
                move |_, _, cx| cx.open_url(&url),
            ));
        }
        meta = meta.child(row_button(
            "git-detail-copy-hash",
            "icons/copy.svg",
            theme,
            move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(hash.clone())),
        ));
        column = column.child(meta);

        for file in &detail.files {
            let path = file.path.clone();
            let fallback = icon("icons/file.svg", 12., theme.text_3).into_any_element();
            let glyph = crate::app::file_glyph(&path, dark, nerd.as_ref(), 12., fallback);
            let color = status_color(file.status, theme);
            let hash = detail.hash.clone();
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme.space(8.))
                    .h(px(26.))
                    .child(
                        div()
                            .w(px(14.))
                            .flex_none()
                            .text_size(theme.ui_px(10.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color)
                            .child(file.status.to_string()),
                    )
                    .child(glyph)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text)
                            .child(path.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme.ui_px(10.5))
                            .text_color(theme.add_green)
                            .child(format!("+{}", file.additions)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme.ui_px(10.5))
                            .text_color(theme.del_red)
                            .child(format!("-{}", file.deletions)),
                    )
                    .child(row_button(
                        format!("git-detail-more-{path}"),
                        "icons/more.svg",
                        theme,
                        cx.listener({
                            let path = path.clone();
                            let hash = hash.clone();
                            move |this, _: &ClickEvent, _, cx| {
                                this.open_file_menu(path.clone(), hash.clone(), cx)
                            }
                        }),
                    )),
            );
        }
        column.into_any_element()
    }

    /// The per-file actions menu: open, reveal, copy path, file history.
    fn file_actions_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (path, _hash) = self.file_menu.clone()?;
        let menu = div()
            .id("git-file-menu")
            .absolute()
            .top(px(42.))
            .right(px(12.))
            .w(px(220.))
            .py(px(4.))
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.close_file_menu(cx)),
            )
            .child(menu_row(
                "git-file-open",
                "icons/arrow-up-right.svg",
                &tr!("git_panel.open_file"),
                theme,
                cx.listener({
                    let path = path.clone();
                    move |this, _: &ClickEvent, window, cx| {
                        this.close_file_menu(cx);
                        this.open_path(path.clone(), window, cx);
                    }
                }),
            ))
            .child(menu_row(
                "git-file-diff",
                "icons/git-compare.svg",
                &tr!("git_panel.open_diff"),
                theme,
                cx.listener({
                    let path = path.clone();
                    move |this, _: &ClickEvent, window, cx| {
                        this.close_file_menu(cx);
                        if let Some(on_open) = this.on_open_file.clone() {
                            on_open(path.clone(), window, cx);
                        }
                    }
                }),
            ))
            .child(menu_row(
                "git-file-reveal",
                "icons/folder.svg",
                &tr!("git_panel.reveal_file"),
                theme,
                cx.listener({
                    let path = path.clone();
                    move |this, _: &ClickEvent, _, cx| {
                        this.close_file_menu(cx);
                        this.reveal_path(&path);
                    }
                }),
            ))
            .child(menu_row(
                "git-file-copy",
                "icons/copy.svg",
                &tr!("git_panel.copy_path"),
                theme,
                cx.listener({
                    let path = path.clone();
                    move |this, _: &ClickEvent, _, cx| {
                        this.close_file_menu(cx);
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.clone()));
                    }
                }),
            ))
            .child(menu_separator(theme))
            .child(menu_row(
                "git-file-history",
                "icons/clock.svg",
                &tr!("git_panel.file_history"),
                theme,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.filter_history_path(path.clone(), true, cx)
                }),
            ));
        Some(menu.into_any_element())
    }

    fn graph_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(error) = &self.graph_error {
            body = body.child(empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.graph_unavailable"),
                Some(error),
            ));
        } else if self.graph.is_empty() && self.graph_loading {
            body = body.child(empty_note(
                theme,
                "icons/git-fork.svg",
                &tr!("git_panel.reading_history"),
                None,
            ));
        } else if self.graph.is_empty() {
            body = body.child(empty_note(
                theme,
                "icons/git-fork.svg",
                &tr!("git_panel.no_commits_yet"),
                Some(&tr!("git_panel.commits_across_branches")),
            ));
        } else {
            let mut list = div().flex().flex_col().py(px(6.));
            let total = self.graph.len();
            for (ix, row) in self.graph.iter().enumerate() {
                let url = self
                    .remote_web
                    .as_ref()
                    .map(|remote| remote.commit_url(&row.commit.hash));
                let selected = self.selected_commit.as_deref() == Some(row.commit.hash.as_str());
                list = list.child(graph_row(
                    row,
                    url.as_deref(),
                    selected,
                    theme,
                    cx.listener({
                        let hash = row.commit.hash.clone();
                        move |this, _: &ClickEvent, _, cx| this.toggle_commit(hash.clone(), cx)
                    }),
                ));
                if selected {
                    match self
                        .commit_detail
                        .as_ref()
                        .filter(|detail| detail.hash == row.commit.hash)
                    {
                        Some(detail) => {
                            list = list.child(self.commit_detail_body(
                                detail,
                                url.as_deref(),
                                theme,
                                cx,
                            ));
                        }
                        None => {
                            list = list.child(detail_note(
                                theme,
                                &tr!(if self.commit_detail_loading {
                                    "git_panel.reading_commit"
                                } else {
                                    "git_panel.commit_unavailable"
                                }),
                            ));
                        }
                    }
                }
                if ix + 1 < total {
                    list = list.child(commit_separator(theme));
                }
            }
            list = list.child(load_more(
                theme,
                self.graph_loading,
                cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_graph(cx)),
            ));
            body = body.child(
                div()
                    .id("git-graph-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(list),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("git-graph-filters")
                    .flex_none()
                    .px(theme.space(16.))
                    .py(theme.space(8.))
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(filter_toggle_chip(
                        "git-graph-all-refs",
                        &tr!("git_panel.all_refs"),
                        self.graph_all_refs,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.graph_all_refs = !this.graph_all_refs;
                            this.selected_commit = None;
                            this.commit_detail = None;
                            this.refresh_graph(cx);
                            cx.notify();
                        }),
                    )),
            )
            .child(body)
            .into_any_element()
    }

    /// The Issues / Pull requests body until those browsers land. It is honest
    /// about the `gh` requirement — install or sign in — and never paints fake
    /// data. The host tabs only render when `gh` is installed (see
    /// [`Self::tab_bar`]).
    fn gh_setup_tab(&self, theme: Theme, title_key: &str, body_key: &str) -> AnyElement {
        if !self.gh_installed {
            return empty_note(
                theme,
                "icons/github.svg",
                &tr!("git_panel.gh_missing_title"),
                Some(&tr!("git_panel.gh_missing_body")),
            );
        }
        if !self.gh_authenticated {
            let detail = if self.gh_detail.is_empty() {
                tr!("git_panel.gh_auth_body")
            } else {
                self.gh_detail.clone()
            };
            return empty_note(
                theme,
                "icons/github.svg",
                &tr!("git_panel.gh_auth_title"),
                Some(&detail),
            );
        }
        empty_note(
            theme,
            "icons/github.svg",
            &tr!(title_key),
            Some(&tr!(body_key)),
        )
    }

    fn branch_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.branch_menu_open {
            return None;
        }
        let mut menu = div()
            .id("git-branch-menu")
            .absolute()
            .top(px(42.))
            .right(px(12.))
            .w(px(260.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .py(px(4.))
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dismiss_branch_menu(cx);
            }));

        // Merge/rebase onto any ref (local, remote, or tag).
        menu = menu.child(menu_row(
            "git-menu-merge",
            "icons/git-merge.svg",
            &tr!("git_panel.merge_branch"),
            theme,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.branch_menu_open = false;
                this.ref_menu = Some(RefTarget::Merge);
                cx.notify();
            }),
        ));
        menu = menu.child(menu_row(
            "git-menu-rebase",
            "icons/git-compare.svg",
            &tr!("git_panel.rebase_onto"),
            theme,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.branch_menu_open = false;
                this.ref_menu = Some(RefTarget::Rebase);
                cx.notify();
            }),
        ));
        menu = menu.child(menu_separator(theme));

        for branch in &self.branches {
            let selected = self.branch.as_deref() == Some(branch.as_str());
            let mut row = div()
                .id(gpui::ElementId::Name(format!("git-branch-{branch}").into()))
                .h(px(28.))
                .mx(px(4.))
                .px(px(8.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .text_size(theme.ui_px(12.))
                .when(selected, |row| row.bg(theme.active))
                .when(!selected, |row| row.hover(|s| s.bg(theme.overlay)))
                .on_click(cx.listener({
                    let branch = branch.clone();
                    move |this, _: &ClickEvent, _, cx| this.checkout_branch(branch.clone(), cx)
                }))
                .child(icon("icons/branch.svg", 11., theme.text_3))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(if selected {
                            theme.active_fg
                        } else {
                            theme.text_2
                        })
                        .child(branch.clone()),
                );
            if selected {
                row = row.child(icon("icons/check.svg", 11., theme.accent));
            } else {
                // Delete a non-current branch. The safe `-d` is tried first; an
                // unmerged branch then asks before the forced `-D`.
                row = row.child(
                    div()
                        .id(gpui::ElementId::Name(
                            format!("git-branch-delete-{branch}").into(),
                        ))
                        .size(px(18.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.overlay))
                        .on_click(cx.listener({
                            let branch = branch.clone();
                            move |this, _: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                                this.delete_branch(branch.clone(), cx);
                            }
                        }))
                        .child(icon("icons/trash.svg", 11., theme.text_3)),
                );
            }
            menu = menu.child(row);
        }

        menu = menu.child(menu_separator(theme));
        menu = menu.child(menu_row(
            "git-menu-new-branch",
            "icons/plus.svg",
            &tr!("git_panel.new_branch"),
            theme,
            cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_branch_prompt(BranchPrompt::New, window, cx)
            }),
        ));
        if let Some(current) = self.branch.clone() {
            menu = menu.child(menu_row(
                "git-menu-rename-branch",
                "icons/pencil.svg",
                &tr!("git_panel.rename_branch"),
                theme,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.open_branch_prompt(BranchPrompt::Rename(current.clone()), window, cx)
                }),
            ));
        }
        Some(menu.into_any_element())
    }

    /// The ref picker opened by "Merge branch…" / "Rebase onto…".
    fn ref_picker_popup(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.ref_menu?;
        let title = match target {
            RefTarget::Merge => tr!("git_panel.merge_branch"),
            RefTarget::Rebase => tr!("git_panel.rebase_onto"),
        };
        let mut menu = div()
            .id("git-ref-menu")
            .absolute()
            .top(px(42.))
            .right(px(12.))
            .w(px(300.))
            .max_h(px(360.))
            .overflow_y_scroll()
            .py(px(4.))
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_ref_picker(cx);
            }))
            .child(
                div()
                    .px(px(10.))
                    .py(px(4.))
                    .text_size(theme.ui_px(11.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child(title),
            );
        if target == RefTarget::Merge {
            // How the merge combines the target, chosen before picking a ref.
            menu = menu.child(
                div().px(px(6.)).pb(px(4.)).flex().gap(px(4.)).children(
                    [
                        (git_ops::MergeMode::Merge, "git_panel.merge_mode_ff"),
                        (git_ops::MergeMode::NoFf, "git_panel.merge_mode_commit"),
                        (git_ops::MergeMode::Squash, "git_panel.merge_mode_squash"),
                    ]
                    .into_iter()
                    .map(|(mode, key)| {
                        let selected = self.merge_mode == mode;
                        div()
                            .id(gpui::ElementId::Name(
                                format!("git-merge-mode-{key}").into(),
                            ))
                            .h(px(22.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .text_size(theme.ui_px(11.))
                            .when(selected, |s| s.bg(theme.active).text_color(theme.active_fg))
                            .when(!selected, |s| {
                                s.text_color(theme.text_3)
                                    .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                            })
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.merge_mode = mode;
                                cx.notify();
                            }))
                            .child(tr!(key))
                    }),
                ),
            );
        }
        if self.refs.is_empty() {
            menu = menu.child(
                div()
                    .px(px(10.))
                    .py(px(6.))
                    .text_size(theme.ui_px(12.))
                    .text_color(theme.text_3)
                    .child(tr!("git_panel.no_refs")),
            );
        }
        for entry in &self.refs {
            let name = entry.name.clone();
            let label = entry.name.clone();
            menu = menu.child(
                div()
                    .id(gpui::ElementId::Name(format!("git-ref-{name}").into()))
                    .h(px(28.))
                    .mx(px(4.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(theme.ui_px(12.))
                    .text_color(theme.text_2)
                    .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| match target {
                            RefTarget::Merge => this.merge_ref(&name, cx),
                            RefTarget::Rebase => this.rebase_ref(&name, cx),
                        }),
                    )
                    .child(icon(ref_icon(entry.kind), 11., theme.text_3))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(label),
                    ),
            );
        }
        Some(menu.into_any_element())
    }

    /// The New/Rename branch prompt.
    fn branch_prompt_popup(
        &self,
        theme: Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let prompt = self.branch_prompt.as_ref()?;
        let (title, confirm_label) = match prompt {
            BranchPrompt::New => (
                tr!("git_panel.new_branch_title"),
                tr!("git_panel.create_branch"),
            ),
            BranchPrompt::Rename(_) => (
                tr!("git_panel.rename_branch_title"),
                tr!("git_panel.rename_branch"),
            ),
        };
        let focused = self
            .branch_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let card = div()
            .w_full()
            .max_w(px(420.))
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.card_shadow())
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_modal(cx)),
            )
            .child(
                div()
                    .text_size(theme.ui_px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(title),
            )
            .child(
                div()
                    .px(theme.space(10.))
                    .py(theme.space(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if focused {
                        theme.border_strong
                    } else {
                        theme.border
                    })
                    .bg(theme.bg_composer)
                    .flex()
                    .child(div().flex_1().min_w_0().child(self.branch_input.clone())),
            )
            .child(
                div()
                    .mt(px(2.))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .child(action_button(
                        "git-branch-prompt-cancel",
                        &tr!("git_panel.cancel"),
                        None,
                        false,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_modal(cx)),
                    ))
                    .child(action_button(
                        "git-branch-prompt-ok",
                        &confirm_label,
                        None,
                        true,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_branch_prompt(cx)),
                    )),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .px(px(16.))
                .bg(theme.bg_main.opacity(0.55))
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }

    /// The "stage unstaged changes?" confirmation, painted over the page.
    fn stage_prompt_popup(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.stage_prompt?;
        let unstaged = self.unstaged.len();
        let staged_empty = self.staged.is_empty();
        let body = if staged_empty {
            if unstaged == 1 {
                tr!("git_panel.stage_prompt_body_empty_one", count = unstaged)
            } else {
                tr!("git_panel.stage_prompt_body_empty_other", count = unstaged)
            }
        } else if unstaged == 1 {
            tr!("git_panel.stage_prompt_body_one", count = unstaged)
        } else {
            tr!("git_panel.stage_prompt_body_other", count = unstaged)
        };
        let card = div()
            .w_full()
            .max_w(px(420.))
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.card_shadow())
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_modal(cx)),
            )
            .child(
                div()
                    .text_size(theme.ui_px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(if staged_empty {
                        tr!("git_panel.stage_prompt_title_generate")
                    } else {
                        tr!("git_panel.stage_prompt_title_rest")
                    }),
            )
            .child(
                div()
                    .text_size(theme.ui_px(12.5))
                    .line_height(theme.ui_px(17.))
                    .text_color(theme.text_2)
                    .whitespace_normal()
                    .child(body),
            )
            .child(
                div()
                    .mt(px(2.))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .child(action_button(
                        "git-prompt-cancel",
                        &tr!("git_panel.cancel"),
                        None,
                        false,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_modal(cx)),
                    ))
                    .children((!staged_empty).then(|| {
                        action_button(
                            "git-prompt-staged",
                            &tr!("git_panel.use_staged"),
                            None,
                            false,
                            false,
                            theme,
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.generate_from_staged(cx)
                            }),
                        )
                    }))
                    .child(action_button(
                        "git-prompt-stage-all",
                        &tr!("git_panel.stage_all_and_generate"),
                        Some(icon("icons/magic-wand.svg", 13., theme.send_fg).into_any_element()),
                        true,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.stage_all_and_generate(cx)),
                    )),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .px(px(16.))
                .bg(theme.bg_main.opacity(0.55))
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }

    /// The confirmation for a destructive recovery action (force push, abort).
    fn confirm_popup(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let confirm = self.pending_confirm.clone()?;
        let (title, body, confirm_label) = match confirm {
            PendingConfirm::ForcePush => (
                tr!("git_panel.confirm_force_title"),
                tr!("git_panel.confirm_force_body"),
                tr!("git_panel.recover_force"),
            ),
            PendingConfirm::Abort(_) => (
                tr!("git_panel.confirm_abort_title"),
                tr!("git_panel.confirm_abort_body"),
                tr!("git_panel.op_abort"),
            ),
            PendingConfirm::DeleteBranch(name) => (
                tr!("git_panel.confirm_delete_title", name = name),
                tr!("git_panel.confirm_delete_body"),
                tr!("git_panel.delete_branch"),
            ),
        };
        let card = div()
            .w_full()
            .max_w(px(420.))
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.card_shadow())
            .p(px(16.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_modal(cx)),
            )
            .child(
                div()
                    .text_size(theme.ui_px(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(title),
            )
            .child(
                div()
                    .text_size(theme.ui_px(12.5))
                    .line_height(theme.ui_px(17.))
                    .text_color(theme.text_2)
                    .whitespace_normal()
                    .child(body),
            )
            .child(
                div()
                    .mt(px(2.))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .child(action_button(
                        "git-confirm-cancel",
                        &tr!("git_panel.cancel"),
                        None,
                        false,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.dismiss_modal(cx)),
                    ))
                    .child(action_button(
                        "git-confirm-ok",
                        &confirm_label,
                        None,
                        true,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.confirm_pending(cx)),
                    )),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .px(px(16.))
                .bg(theme.bg_main.opacity(0.55))
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }
}

// (The `OpenFile` callback type is declared near the top of the module.)

impl Render for GitPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = *theme::get(cx);
        // Expire a stale status line.
        if self
            .status
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed() > Duration::from_secs(6))
        {
            self.status = None;
        }
        let content = match self.tab {
            GitTab::Changes => self.changes_tab(theme, window, cx),
            GitTab::History => self.history_tab(theme, cx),
            GitTab::Graph => self.graph_tab(theme, cx),
            GitTab::Issues => self.gh_setup_tab(
                theme,
                "git_panel.gh_issues_title",
                "git_panel.gh_issues_body",
            ),
            GitTab::Pulls => {
                self.gh_setup_tab(theme, "git_panel.gh_pulls_title", "git_panel.gh_pulls_body")
            }
        };
        div()
            .id("git-page")
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .bg(theme.bg_main)
            .font_family(theme::ui_font_family())
            .flex()
            .flex_col()
            .child(self.header(theme, cx))
            .child(self.tab_bar(theme, cx))
            // Failures stay pinned under the tabs (never inside the scrolling
            // body) so push/pull/merge errors are readable on every tab.
            .children(self.failure_banner(theme, cx))
            // An unfinished merge/rebase sits directly under the failure banner,
            // above every tab, so it is the page's single "you are mid-merge"
            // signal regardless of tab.
            .children(self.operation_bar(theme, cx))
            .child(content)
            .children(self.branch_menu(theme, cx))
            .children(self.ref_picker_popup(theme, cx))
            .children(self.file_actions_menu(theme, cx))
            .children(self.stage_prompt_popup(theme, cx))
            .children(self.confirm_popup(theme, cx))
            .children(self.branch_prompt_popup(theme, window, cx))
            .into_any_element()
    }
}

/// What the commit bar offers for the current repository state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BarActions {
    /// Working tree has changes: Commit / Commit and push.
    Commit,
    /// Working tree is clean but there is something to send: Push (or the
    /// first-time Publish branch).
    Push { publish: bool },
    /// Clean and strictly behind the upstream: Pull.
    Pull { behind: usize },
    /// Clean but diverged (ahead *and* behind): pushing is rejected and a
    /// fast-forward pull cannot apply, so offer a real merge.
    Merge { behind: usize },
    /// Clean and in sync: a quiet status label.
    UpToDate,
}

fn bar_actions(
    has_changes: bool,
    ahead: usize,
    has_upstream: bool,
    has_commits: bool,
    behind: usize,
) -> BarActions {
    if has_changes {
        BarActions::Commit
    } else if !has_upstream && has_commits {
        BarActions::Push { publish: true }
    } else if ahead > 0 && behind > 0 {
        BarActions::Merge { behind }
    } else if ahead > 0 {
        BarActions::Push { publish: false }
    } else if behind > 0 {
        BarActions::Pull { behind }
    } else {
        BarActions::UpToDate
    }
}

/// One recovery button in the failure banner. The action is a plain intent; the
/// panel's `recover` decides how to run it.
fn recovery_button(action: RecoveryAction, theme: Theme, cx: &Context<GitPanel>) -> AnyElement {
    let (id, key, primary) = match action {
        RecoveryAction::Pull => ("git-recover-pull", "git_panel.recover_pull", false),
        RecoveryAction::Merge => ("git-recover-merge", "git_panel.recover_merge", false),
        RecoveryAction::Rebase => ("git-recover-rebase", "git_panel.recover_rebase", false),
        RecoveryAction::ForceWithLease => ("git-recover-force", "git_panel.recover_force", false),
        RecoveryAction::AbortOperation => ("git-recover-abort", "git_panel.recover_abort", false),
        RecoveryAction::ContinueOperation => {
            ("git-recover-continue", "git_panel.recover_continue", true)
        }
        RecoveryAction::SkipOperation => ("git-recover-skip", "git_panel.recover_skip", false),
        RecoveryAction::OpenConflicts => (
            "git-recover-conflicts",
            "git_panel.recover_open_conflicts",
            false,
        ),
        RecoveryAction::Reauthenticate => ("git-recover-reauth", "git_panel.recover_reauth", false),
        RecoveryAction::Retry => ("git-recover-retry", "git_panel.recover_retry", true),
        RecoveryAction::PublishBranch => ("git-recover-publish", "git_panel.recover_publish", true),
    };
    action_button(
        id,
        &tr!(key),
        None,
        primary,
        false,
        theme,
        cx.listener(move |this, _: &ClickEvent, window, cx| this.recover(action, window, cx)),
    )
}

/// A labeled row in the branch/ref popovers.
fn menu_row(
    id: &'static str,
    icon_path: &'static str,
    label: &str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .h(px(28.))
        .mx(px(4.))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .text_size(theme.ui_px(12.))
        .text_color(theme.text_2)
        .hover(|s| s.bg(theme.overlay).text_color(theme.text))
        .on_click(listener)
        .child(icon(icon_path, 11., theme.text_3))
        .child(label.to_string())
        .into_any_element()
}

/// A hairline between menu groups.
fn menu_separator(theme: Theme) -> AnyElement {
    div()
        .my(px(4.))
        .mx(px(8.))
        .h(px(1.))
        .bg(theme.border)
        .into_any_element()
}

/// The glyph for a ref kind in the picker.
fn ref_icon(kind: git::RefKind) -> &'static str {
    match kind {
        git::RefKind::Head | git::RefKind::Branch => "icons/branch.svg",
        git::RefKind::Remote => "icons/cloud.svg",
        git::RefKind::Tag => "icons/tag-01.svg",
    }
}

/// One row in the Stashes section: message plus pop / apply / drop.
fn stash_row(stash: &git_ops::StashEntry, theme: Theme, cx: &Context<GitPanel>) -> AnyElement {
    let index = stash.index;
    div()
        .id(gpui::ElementId::Name(format!("git-stash-{index}").into()))
        .mx(theme.space(12.))
        .h(px(30.))
        .px(theme.space(8.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .gap(theme.space(8.))
        .hover(|s| s.bg(theme.bg_hover))
        .child(icon("icons/clock.svg", 12., theme.text_3))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(theme.ui_px(12.))
                .text_color(theme.text)
                .child(stash.message.clone()),
        )
        .child(row_button(
            format!("git-stash-pop-{index}"),
            "icons/upload.svg",
            theme,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.stash_pop(index, cx)),
        ))
        .child(row_button(
            format!("git-stash-apply-{index}"),
            "icons/check.svg",
            theme,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.stash_apply(index, cx)),
        ))
        .child(row_button(
            format!("git-stash-drop-{index}"),
            "icons/trash.svg",
            theme,
            cx.listener(move |this, _: &ClickEvent, _, cx| this.stash_drop(index, cx)),
        ))
        .into_any_element()
}

/// A removable filter chip (label plus an × affordance).
fn filter_chip(
    id: &'static str,
    label: &str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .h(px(22.))
        .px(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.bg_raised)
        .flex()
        .items_center()
        .gap(px(5.))
        .cursor_pointer()
        .text_size(theme.ui_px(11.))
        .text_color(theme.text_2)
        .hover(|s| s.bg(theme.bg_hover).text_color(theme.text))
        .on_click(listener)
        .child(label.to_string())
        .child(icon("icons/x.svg", 9., theme.text_3))
        .into_any_element()
}

/// An on/off filter chip ("All branches").
fn filter_toggle_chip(
    id: &'static str,
    label: &str,
    active: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .h(px(22.))
        .px(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(if active { theme.accent } else { theme.border })
        .when(active, |chip| {
            chip.bg(theme.accent).text_color(theme.send_fg)
        })
        .when(!active, |chip| {
            chip.text_color(theme.text_3)
                .hover(|s| s.bg(theme.bg_hover).text_color(theme.text))
        })
        .flex()
        .items_center()
        .gap(px(5.))
        .cursor_pointer()
        .text_size(theme.ui_px(11.))
        .on_click(listener)
        .child(icon(
            "icons/git-fork.svg",
            10.,
            if active { theme.send_fg } else { theme.text_3 },
        ))
        .child(label.to_string())
        .into_any_element()
}

/// A small placeholder shown inside an expanding commit row while its detail
/// loads (or when it could not be loaded).
fn detail_note(theme: Theme, label: &str) -> AnyElement {
    div()
        .mx(theme.space(12.))
        .mb(theme.space(6.))
        .px(theme.space(12.))
        .py(theme.space(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.bg_raised)
        .text_size(theme.ui_px(12.))
        .text_color(theme.text_3)
        .child(label.to_string())
        .into_any_element()
}

// ── Graph tab drawing ──────────────────────────────────────────────────────

/// Horizontal pitch of one lane, padding around the gutter, and the fixed
/// height that keeps every row's lanes aligned.
const LANE_WIDTH: f32 = 14.;
const LANE_PADDING: f32 = 8.;
const GRAPH_STROKE: f32 = 2.;
const GRAPH_NODE_RADIUS: f32 = 4.5;
const GRAPH_ROW_HEIGHT: f32 = 38.;

/// One row in the Graph list: the lane drawing on the left, then the same
/// subject, refs, author meta, and short hash as a History row.
fn graph_row(
    row: &git::GraphRow,
    url: Option<&str>,
    selected: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let commit = &row.commit;

    let meta = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(theme.ui_px(11.))
        .child(author_avatar(&commit.author, &commit.author_email, theme))
        .child(div().text_color(theme.text_2).child(commit.author.clone()))
        .child(
            div()
                .size(px(3.))
                .rounded_full()
                .bg(theme.text_3.opacity(0.7)),
        )
        .child(
            div()
                .text_color(theme.text_3)
                .child(commit.relative.clone()),
        );

    let mut element = div()
        .id(gpui::ElementId::Name(
            format!("git-graph-{}", commit.short).into(),
        ))
        .mx(px(12.))
        .px(px(8.))
        .py(px(6.))
        .rounded_md()
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        .hover(|s| s.bg(theme.bg_hover))
        .when(selected, |s| s.bg(theme.overlay))
        .on_click(listener)
        .child(graph_gutter(row, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .min_w_0()
                        .children(
                            commit
                                .refs
                                .iter()
                                .map(|label| ref_badge(&label.name, label.kind, theme)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(theme.ui_px(12.5))
                                .text_color(theme.text)
                                .child(commit.subject.clone()),
                        ),
                )
                .child(meta),
        )
        .child(
            div()
                .flex_none()
                .font_family(theme::code_font_family())
                .text_size(theme.ui_px(11.))
                .text_color(theme.text_3)
                .child(commit.short.clone()),
        );
    if let Some(url) = url.map(str::to_string) {
        element = element.child(row_button(
            "git-graph-open",
            "icons/arrow-up-right.svg",
            theme,
            move |_, _, cx| cx.open_url(&url),
        ));
    }
    element
        .child(icon(
            if selected {
                "icons/arrow-down.svg"
            } else {
                "icons/arrow-right.svg"
            },
            12.,
            theme.text_3,
        ))
        .into_any_element()
}

/// The lane drawing for one row: a canvas sized to the row's widest lane, with
/// a fixed height so lines meet cleanly across rows.
fn graph_gutter(row: &git::GraphRow, theme: Theme) -> AnyElement {
    let lanes = row.lane_count.max(1);
    let width = LANE_PADDING * 2. + LANE_WIDTH * lanes as f32;
    let row = row.clone();
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| paint_graph(window, bounds, &row, theme),
    )
    .flex_none()
    .w(px(width))
    .h(px(GRAPH_ROW_HEIGHT))
    .into_any_element()
}

/// Draw one row's lanes: pass-through verticals, the node's incoming edge, the
/// node-to-parent edges, and the node disc itself.
fn paint_graph(window: &mut Window, bounds: Bounds<Pixels>, row: &git::GraphRow, theme: Theme) {
    let stroke = px(GRAPH_STROKE);
    let lane_x = |lane: usize| -> Pixels {
        bounds.origin.x + px(LANE_PADDING + LANE_WIDTH * lane as f32 + LANE_WIDTH / 2.)
    };
    let top = bounds.origin.y;
    let mid = bounds.origin.y + bounds.size.height / 2.;
    let bottom = bounds.origin.y + bounds.size.height;
    let node_x = lane_x(row.node_lane);

    for &lane in &row.verticals {
        let x = lane_x(lane);
        if let Ok(path) = graph_line(point(x, top), point(x, bottom), stroke) {
            window.paint_path(path, Background::from(lane_color(lane, theme).to_rgb()));
        }
    }
    if row.node_has_incoming {
        if let Ok(path) = graph_line(point(node_x, top), point(node_x, mid), stroke) {
            window.paint_path(
                path,
                Background::from(lane_color(row.node_lane, theme).to_rgb()),
            );
        }
    }
    for &lane in &row.parents {
        if let Ok(path) = graph_line(point(node_x, mid), point(lane_x(lane), bottom), stroke) {
            window.paint_path(path, Background::from(lane_color(lane, theme).to_rgb()));
        }
    }

    let center = point(node_x, mid);
    let origin = point(
        center.x - px(GRAPH_NODE_RADIUS),
        center.y - px(GRAPH_NODE_RADIUS),
    );
    let size = size(px(GRAPH_NODE_RADIUS * 2.), px(GRAPH_NODE_RADIUS * 2.));
    window.paint_quad(
        fill(
            Bounds::new(origin, size),
            Background::from(lane_color(row.node_lane, theme).to_rgb()),
        )
        .corner_radii(px(GRAPH_NODE_RADIUS)),
    );
}

/// A stroked line between two points, or an error when the geometry is invalid.
fn graph_line(
    from: gpui::Point<Pixels>,
    to: gpui::Point<Pixels>,
    stroke: Pixels,
) -> anyhow::Result<gpui::Path<Pixels>> {
    let mut builder = PathBuilder::stroke(stroke);
    builder.move_to(from);
    builder.line_to(to);
    builder.build()
}

/// A stable color per lane column, so a branch keeps its hue down the graph.
fn lane_color(lane: usize, theme: Theme) -> Hsla {
    const HUES: [f32; 8] = [202., 274., 158., 44., 330., 14., 96., 236.];
    let (saturation, lightness) = match theme.mode {
        ThemeMode::Dark => (0.60, 0.62),
        ThemeMode::Light => (0.60, 0.46),
    };
    hsla(HUES[lane % HUES.len()] / 360., saturation, lightness, 1.)
}

/// One row in the History list: an author monogram, the subject with its ref
/// badges, an author · relative-time meta line, and the short hash with an
/// external-link mark when the commit can be opened on the remote.
fn commit_row(
    commit: &CommitEntry,
    url: Option<&str>,
    selected: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    on_author: impl Fn(String, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let author = commit.author.clone();
    let meta = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(theme.ui_px(11.))
        .child(author_avatar(&commit.author, &commit.author_email, theme))
        .child({
            // Clicking the author filters History to their commits.
            let author_for_click = author.clone();
            div()
                .id(gpui::ElementId::Name(
                    format!("git-commit-author-{}", commit.short).into(),
                ))
                .text_color(theme.text_2)
                .cursor_pointer()
                .hover(|s| s.text_color(theme.accent))
                .on_click(move |_, window, cx: &mut gpui::App| {
                    cx.stop_propagation();
                    on_author(author_for_click.clone(), window, cx);
                })
                .child(author.clone())
        })
        .child(
            div()
                .size(px(3.))
                .rounded_full()
                .bg(theme.text_3.opacity(0.7)),
        )
        .child(
            div()
                .text_color(theme.text_3)
                .child(commit.relative.clone()),
        );

    let mut row = div()
        .id(gpui::ElementId::Name(
            format!("git-commit-{}", commit.short).into(),
        ))
        .mx(px(12.))
        .px(px(8.))
        .py(px(9.))
        .rounded_md()
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        .hover(|s| s.bg(theme.bg_hover))
        .when(selected, |row| row.bg(theme.overlay))
        .on_click(listener)
        .child(history_marker(theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .min_w_0()
                        .children(
                            commit
                                .refs
                                .iter()
                                .map(|label| ref_badge(&label.name, label.kind, theme)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(theme.ui_px(12.5))
                                .text_color(theme.text)
                                .child(commit.subject.clone()),
                        ),
                )
                .child(meta),
        )
        .child(
            div().flex_none().flex().items_center().gap(px(5.)).child(
                div()
                    .font_family(theme::code_font_family())
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.text_3)
                    .child(commit.short.clone()),
            ),
        );
    // Opening the commit on the forge is a separate affordance from expanding
    // its detail, so it gets its own button that stops propagation.
    if let Some(url) = url.map(str::to_string) {
        row = row.child(row_button(
            "git-commit-open",
            "icons/arrow-up-right.svg",
            theme,
            move |_, _, cx| cx.open_url(&url),
        ));
    }
    row.child(icon(
        if selected {
            "icons/arrow-down.svg"
        } else {
            "icons/arrow-right.svg"
        },
        12.,
        theme.text_3,
    ))
    .into_any_element()
}

/// Diameter of a row's leading git marker. The separator inset below is
/// derived from it so the two stay aligned.
const HISTORY_LEADING: f32 = 26.;

/// Diameter of the small author avatar in the meta line.
const AUTHOR_AVATAR: f32 = 14.;

/// The far-left marker for a commit row: a git-commit glyph, not a person —
/// identity lives in the small avatar beside the author name.
fn history_marker(theme: Theme) -> AnyElement {
    div()
        .flex_none()
        .size(px(HISTORY_LEADING))
        .rounded_full()
        .bg(theme.bg_raised)
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_center()
        .child(icon("icons/git-commit.svg", 14., theme.text_3))
        .into_any_element()
}

/// A small round avatar shown before the author name: the real GitHub photo
/// when the author name is a GitHub handle, with the monogram as its fallback
/// (while loading, for accounts with no photo, or for real names that aren't
/// handles). Never leaves a blank slot.
fn author_avatar(name: &str, email: &str, theme: Theme) -> AnyElement {
    avatar_image(github_avatar_url(name), name, email, theme)
}

/// A small round avatar from an explicit URL (a GitHub event carries the exact
/// one), falling back to the monogram. `name`/`email` seed the fallback.
fn avatar_image(url: Option<String>, name: &str, email: &str, theme: Theme) -> AnyElement {
    match url {
        Some(url) => {
            let name = name.to_string();
            let email = email.to_string();
            img(url)
                .size(px(AUTHOR_AVATAR))
                .rounded_full()
                .object_fit(ObjectFit::Cover)
                .with_loading({
                    let (name, email) = (name.clone(), email.clone());
                    move || monogram_chip(&name, &email, theme)
                })
                .with_fallback(move || monogram_chip(&name, &email, theme))
                .into_any_element()
        }
        None => monogram_chip(name, email, theme),
    }
}

/// The GitHub avatar URL for a commit author whose name looks like a handle.
/// Real names ("Ada Lovelace") return `None` and fall back to the monogram
/// rather than 404-ing. GitHub usernames are alphanumeric or single hyphens.
fn github_avatar_url(author: &str) -> Option<String> {
    let handle = author.trim();
    let valid = !handle.is_empty()
        && handle.len() <= 39
        && !handle.starts_with('-')
        && !handle.ends_with('-')
        && !handle.contains("--")
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    valid.then(|| format!("https://avatars.githubusercontent.com/{handle}?size=48"))
}

/// A tiny initials chip — the avatar fallback. The tint is a stable function
/// of the author's email so the same person keeps their color across sessions;
/// saturation stays low so identity doesn't read as confetti.
fn monogram_chip(name: &str, email: &str, theme: Theme) -> AnyElement {
    let (saturation, lightness) = match theme.mode {
        ThemeMode::Dark => (0.40, 0.34),
        ThemeMode::Light => (0.46, 0.34),
    };
    div()
        .flex_none()
        .size(px(AUTHOR_AVATAR))
        .rounded_full()
        .bg(hsla(avatar_hue(email, name), saturation, lightness, 1.))
        .flex()
        .items_center()
        .justify_center()
        .text_size(theme.ui_px(7.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(hsla(0., 0., 0.985, 1.))
        .child(initials_for(name))
        .into_any_element()
}

/// Up to two initials from an author name.
fn initials_for(name: &str) -> String {
    let pick = |word: &str| word.chars().next().map(|c| c.to_uppercase().to_string());
    let mut words = name.split_whitespace();
    let first = words.next().unwrap_or("");
    let last = words.last();
    match (pick(first), last.and_then(pick)) {
        (Some(a), Some(b)) => format!("{a}{b}"),
        (Some(a), None) => {
            let mut chars = first.chars();
            chars.next();
            match chars.next() {
                Some(b) => format!("{a}{}", b.to_uppercase()),
                None => a,
            }
        }
        _ => "?".to_string(),
    }
}

/// A stable hue in `0..1` derived from the author's email (falling back to the
/// name). FNV-1a keeps it cheap and stable across runs.
fn avatar_hue(email: &str, name: &str) -> f32 {
    const HUES: [f32; 8] = [14., 44., 96., 158., 202., 236., 274., 330.];
    let seed = if email.trim().is_empty() { name } else { email };
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.trim().to_ascii_lowercase().bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    HUES[(hash % HUES.len() as u64) as usize] / 360.
}

/// A hairline between changed-file rows, inset to the 20px page gutter so it
/// lines up with the section label and the path — not a full-bleed rule.
fn changes_separator(theme: Theme) -> AnyElement {
    div()
        .mx(theme.space(20.))
        .h(px(1.))
        .bg(theme.border)
        .into_any_element()
}

/// A hairline between History rows, inset to the row content the way a commit
/// list separates entries without boxing each one.
fn commit_separator(theme: Theme) -> AnyElement {
    // 12 (row margin) + 8 (row padding) + leading marker + 10 (row gap).
    div()
        .ml(px(12. + 8. + HISTORY_LEADING + 10.))
        .mr(px(20.))
        .h(px(1.))
        .bg(theme.border)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_actions_follow_git_state() {
        // Uncommitted changes → commit actions (regardless of unpushed commits).
        assert_eq!(bar_actions(true, 0, true, true, 0), BarActions::Commit);
        assert_eq!(bar_actions(true, 3, true, true, 0), BarActions::Commit);
        // Clean with unpushed commits → Push.
        assert_eq!(
            bar_actions(false, 2, true, true, 0),
            BarActions::Push { publish: false }
        );
        // Clean, no upstream, has commits → Publish branch.
        assert_eq!(
            bar_actions(false, 0, false, true, 0),
            BarActions::Push { publish: true }
        );
        // Clean, in sync → up to date.
        assert_eq!(bar_actions(false, 0, true, true, 0), BarActions::UpToDate);
        // Clean, behind → Pull with the behind count.
        assert_eq!(
            bar_actions(false, 0, true, true, 4),
            BarActions::Pull { behind: 4 }
        );
        // Clean but diverged (ahead and behind) → Merge, not a doomed push.
        assert_eq!(
            bar_actions(false, 2, true, true, 3),
            BarActions::Merge { behind: 3 }
        );
        // Empty repo (no commits, no upstream) is not a publish.
        assert_eq!(bar_actions(false, 0, false, false, 0), BarActions::UpToDate);
    }
}
