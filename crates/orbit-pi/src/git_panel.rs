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
    canvas, div, fill, hsla, img, point, prelude::*, px, radians, size, Animation, AnimationExt,
    AnyElement, Background, Bounds, ClickEvent, Context, Entity, Focusable, FontWeight, Hsla,
    MouseDownEvent, ObjectFit, PathBuilder, Pixels, Render, Transformation, Window,
};

use crate::app::{icon, nerd_font_family};
use crate::commit_message;
use crate::git::{self, CommitEntry, RefKind, StatusRow};
use crate::theme::{self, Theme, ThemeMode};

/// Callback the app installs so a changed-file row can open its diff in the
/// Review pane.
pub type OpenFile = std::rc::Rc<dyn Fn(String, &mut Window, &mut gpui::App)>;

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
}

/// Which commit-bar action is running, for the button state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitAction {
    Commit,
    CommitAndPush,
    Push,
    Pull,
    Merge,
}

/// A failed Git operation shown on the page until dismissed.
///
/// `title` is a short human summary (usually including the fix); `detail`
/// keeps the command's own output, including the multi-line `hint:` blocks
/// that a rejected push or a merge conflict prints. Unlike the transient
/// [`GitPanel::status`] line, a failure is never truncated and never expires
/// on a timer, so push/pull/merge errors are actually readable.
#[derive(Clone, Debug)]
struct GitFailure {
    title: String,
    detail: String,
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

    // ── changes ──
    staged: Vec<StatusRow>,
    unstaged: Vec<StatusRow>,
    changes_loading: bool,
    changes_error: Option<String>,
    /// The last operation failure, painted as a persistent banner above the
    /// tab body. Cleared by a later success or the dismiss button.
    failure: Option<GitFailure>,
    /// Path awaiting a discard confirmation.
    pending_discard: Option<String>,

    // ── history ──
    history: Vec<CommitEntry>,
    history_loading: bool,
    history_error: Option<String>,

    // ── graph (all local branches) ──
    graph: Vec<git::GraphRow>,
    graph_loading: bool,
    graph_error: Option<String>,

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
            staged: Vec::new(),
            unstaged: Vec::new(),
            changes_loading: false,
            changes_error: None,
            failure: None,
            pending_discard: None,
            history: Vec::new(),
            history_loading: false,
            history_error: None,
            graph: Vec::new(),
            graph_loading: false,
            graph_error: None,
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
        match self.tab {
            GitTab::History => self.refresh_history(cx),
            GitTab::Graph => self.refresh_graph(cx),
            GitTab::Changes => {}
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
            move || git::status_rows(&cwd),
            |panel, result, cx| {
                panel.changes_loading = false;
                match result {
                    Ok(rows) => {
                        panel.staged = rows.iter().filter(|row| row.staged()).cloned().collect();
                        panel.unstaged =
                            rows.iter().filter(|row| row.unstaged()).cloned().collect();
                    }
                    Err(err) => panel.changes_error = Some(err),
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
                ))
            },
            |panel, result, cx| {
                if let Ok((branch, ahead_behind, branches, has_commits, remote_web)) = result {
                    panel.branch = branch;
                    panel.ahead_behind = ahead_behind;
                    panel.branches = branches;
                    panel.has_commits = has_commits;
                    panel.remote_web = remote_web;
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
        self.spawn_data(
            cx,
            move || git::history(&cwd, limit, 0),
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

    /// Fetch the all-branches graph. Grows the window by a page each call, the
    /// same way History does, so "Load more" deepens the graph.
    fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.graph_loading = true;
        self.graph_error = None;
        let limit = self.graph.len().max(HISTORY_PAGE) + HISTORY_PAGE;
        self.spawn_data(
            cx,
            move || {
                let commits = git::graph_history(&cwd, limit)?;
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
        let (title, detail) = classify_git_failure(&raw);
        self.failure = Some(GitFailure { title, detail });
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

    /// Dismiss the stage confirmation (Escape / Cancel / outside click).
    pub fn dismiss_modal(&mut self, cx: &mut Context<Self>) {
        if self.stage_prompt.take().is_some() {
            cx.notify();
        }
    }

    pub fn has_modal(&self) -> bool {
        self.stage_prompt.is_some()
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

    fn toggle_branch_menu(&mut self, cx: &mut Context<Self>) {
        // mouse-down-out closes the menu; the chip's mouse-up would otherwise
        // toggle it open again on the same click (see AGENT.md popovers).
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
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
                    Err(err) => panel.set_failure(err),
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
        let tabs = [
            (
                GitTab::Changes,
                "icons/git-compare.svg",
                "git_panel.tab_changes",
            ),
            (GitTab::History, "icons/clock.svg", "git_panel.tab_history"),
            (GitTab::Graph, "icons/git-fork.svg", "git_panel.tab_graph"),
        ];
        div()
            .h(px(40.))
            .flex_none()
            .px(theme.space(16.))
            .flex()
            .items_center()
            .gap(theme.space(6.))
            .border_b_1()
            .border_color(theme.border)
            .children(tabs.map(|(tab, tab_icon, key)| {
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
                        ),
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
        let busy = self.pending.is_some() || self.generating;
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
        if self.staged.is_empty() && self.unstaged.is_empty() {
            return vec![empty_note(
                theme,
                "icons/check.svg",
                &tr!("git_panel.working_tree_clean"),
                Some(&tr!("git_panel.nothing_to_commit")),
            )];
        }
        let mut out = Vec::new();
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
        if let Some(error) = &self.history_error {
            return empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.history_unavailable"),
                Some(error),
            );
        }
        if self.history.is_empty() && self.history_loading {
            return empty_note(
                theme,
                "icons/clock.svg",
                &tr!("git_panel.reading_history"),
                None,
            );
        }
        if self.history.is_empty() {
            return empty_note(
                theme,
                "icons/clock.svg",
                &tr!("git_panel.no_commits_yet"),
                Some(&tr!("git_panel.commits_will_show_up")),
            );
        }
        let mut list = div().flex().flex_col().py(px(6.));
        let total = self.history.len();
        for (ix, commit) in self.history.iter().enumerate() {
            let url = self
                .remote_web
                .as_ref()
                .map(|remote| remote.commit_url(&commit.hash));
            list = list.child(commit_row(commit, url.as_deref(), theme));
            if ix + 1 < total {
                list = list.child(commit_separator(theme));
            }
        }
        list = list.child(load_more(
            theme,
            self.history_loading,
            cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_history(cx)),
        ));
        div()
            .id("git-history-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(list)
            .into_any_element()
    }

    fn graph_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        if let Some(error) = &self.graph_error {
            return empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.graph_unavailable"),
                Some(error),
            );
        }
        if self.graph.is_empty() && self.graph_loading {
            return empty_note(
                theme,
                "icons/git-fork.svg",
                &tr!("git_panel.reading_history"),
                None,
            );
        }
        if self.graph.is_empty() {
            return empty_note(
                theme,
                "icons/git-fork.svg",
                &tr!("git_panel.no_commits_yet"),
                Some(&tr!("git_panel.commits_across_branches")),
            );
        }
        let mut list = div().flex().flex_col().py(px(6.));
        let total = self.graph.len();
        for (ix, row) in self.graph.iter().enumerate() {
            let url = self
                .remote_web
                .as_ref()
                .map(|remote| remote.commit_url(&row.commit.hash));
            list = list.child(graph_row(row, url.as_deref(), theme));
            if ix + 1 < total {
                list = list.child(commit_separator(theme));
            }
        }
        list = list.child(load_more(
            theme,
            self.graph_loading,
            cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_graph(cx)),
        ));
        div()
            .id("git-graph-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(list)
            .into_any_element()
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
            .w(px(240.))
            .max_h(px(320.))
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
        for branch in &self.branches {
            let selected = self.branch.as_deref() == Some(branch.as_str());
            menu = menu.child(
                div()
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
                    )
                    .when(selected, |row| {
                        row.child(icon("icons/check.svg", 11., theme.accent))
                    }),
            );
        }
        Some(menu.into_any_element())
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
            .child(content)
            .children(self.branch_menu(theme, cx))
            .children(self.stage_prompt_popup(theme, cx))
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

// ── shared bits ────────────────────────────────────────────────────────────

fn status_color(badge: char, theme: Theme) -> Hsla {
    match badge {
        'A' | '?' => theme.add_green,
        'D' => theme.del_red,
        'U' => theme.add_green,
        'R' | 'C' => theme.warn,
        _ => theme.warn,
    }
}

fn check_box(checked: bool, theme: Theme) -> gpui::Div {
    div()
        .size(px(16.))
        .rounded(px(4.))
        .border_1()
        .border_color(if checked {
            theme.accent
        } else {
            theme.border_strong
        })
        .bg(if checked {
            theme.accent
        } else {
            theme.bg_composer
        })
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |box_| {
            box_.child(icon("icons/check.svg", 10., theme.send_fg))
        })
}

/// A rotating loader for in-flight buttons (matches the Review pane spinner).
fn spinner(id: &'static str, size: f32, theme: Theme) -> AnyElement {
    gpui::svg()
        .path("icons/loader.svg")
        .flex_none()
        .size(px(size))
        .text_color(theme.accent)
        .with_animation(
            id,
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
        .into_any_element()
}

fn action_button(
    id: &'static str,
    label: &str,
    leading: Option<AnyElement>,
    primary: bool,
    disabled: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let mut button = div()
        .id(id)
        .h(px(28.))
        .px(px(10.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.))
        .text_size(theme.ui_px(12.))
        .font_weight(FontWeight::MEDIUM)
        .border_1();
    if disabled {
        button = button
            .border_color(theme.border)
            .bg(theme.overlay)
            .text_color(theme.text_3);
    } else if primary {
        button = button
            .border_color(gpui::transparent_black())
            .bg(theme.send_bg)
            .text_color(theme.send_fg)
            .cursor_pointer()
            .hover(|s| s.bg(theme.send_bg_hover))
            .on_click(listener);
    } else {
        button = button
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .text_color(theme.text)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover).border_color(theme.border_strong))
            .on_click(listener);
    }
    if let Some(leading) = leading {
        button = button.child(leading);
    }
    button.child(label.to_string()).into_any_element()
}

fn row_button(
    id: impl Into<gpui::SharedString>,
    icon_path: &'static str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id(gpui::ElementId::Name(id.into()))
        .size(px(22.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(theme.overlay))
        .on_click(move |event, window, cx| {
            cx.stop_propagation();
            listener(event, window, cx);
        })
        .child(icon(icon_path, 12., theme.text_3))
        .into_any_element()
}

fn ref_badge(name: &str, kind: RefKind, theme: Theme) -> AnyElement {
    let (fg, border) = match kind {
        RefKind::Head => (theme.accent, theme.accent),
        RefKind::Branch => (theme.text_2, theme.border_strong),
        RefKind::Remote => (theme.text_3, theme.border),
        RefKind::Tag => (theme.warn, theme.warn),
    };
    div()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(4.))
        .border_1()
        .border_color(border.opacity(0.6))
        .flex()
        .items_center()
        .text_size(theme.ui_px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .child(name.to_string())
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
fn graph_row(row: &git::GraphRow, url: Option<&str>, theme: Theme) -> AnyElement {
    let commit = &row.commit;
    let target = url.map(str::to_string);
    let linked = target.is_some();

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

    div()
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
        .when(linked, |row| {
            row.cursor_pointer()
                .hover(|s| s.bg(theme.bg_hover))
                .on_click(move |_, _, cx| {
                    if let Some(target) = target.clone() {
                        cx.open_url(&target);
                    }
                })
        })
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
                .flex()
                .items_center()
                .gap(px(5.))
                .child(
                    div()
                        .font_family(theme::code_font_family())
                        .text_size(theme.ui_px(11.))
                        .text_color(theme.text_3)
                        .child(commit.short.clone()),
                )
                .when(linked, |s| {
                    s.child(icon("icons/arrow-up-right.svg", 12., theme.text_3))
                }),
        )
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
fn commit_row(commit: &CommitEntry, url: Option<&str>, theme: Theme) -> AnyElement {
    let target = url.map(str::to_string);
    let linked = target.is_some();

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

    div()
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
        .when(linked, |row| {
            row.cursor_pointer()
                .hover(|s| s.bg(theme.bg_hover))
                .on_click(move |_, _, cx| {
                    if let Some(target) = target.clone() {
                        cx.open_url(&target);
                    }
                })
        })
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
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(5.))
                .child(
                    div()
                        .font_family(theme::code_font_family())
                        .text_size(theme.ui_px(11.))
                        .text_color(theme.text_3)
                        .child(commit.short.clone()),
                )
                .when(linked, |s| {
                    s.child(icon("icons/arrow-up-right.svg", 12., theme.text_3))
                }),
        )
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

fn load_more(
    theme: Theme,
    loading: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .id("git-load-more")
        .mx(px(12.))
        .mt(px(6.))
        .h(px(32.))
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .cursor_pointer()
        .text_size(theme.ui_px(12.))
        .text_color(theme.text_2)
        .hover(|s| s.bg(theme.bg_hover))
        .on_click(listener)
        .child(if loading {
            tr!("git_panel.loading")
        } else {
            tr!("git_panel.load_more")
        })
        .into_any_element()
}

fn empty_note(theme: Theme, glyph: &'static str, title: &str, detail: Option<&str>) -> AnyElement {
    let mut column = div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(16.))
        .py(px(60.))
        .child(
            div()
                .size(px(32.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.bg_raised)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(glyph, 16., theme.text_2)),
        )
        .child(
            div()
                .mt(px(12.))
                .text_size(theme.ui_px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title.to_string()),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(px(6.))
                .max_w(px(320.))
                .text_align(gpui::TextAlign::Center)
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_3)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}

/// Map raw `git` stderr to a one-line title plus the (cleaned) raw detail.
/// The title says what happened and, where possible, what to do next; the
/// detail preserves the command's own words — including multi-line `hint:`
/// blocks — so nothing is lost to truncation.
fn classify_git_failure(raw: &str) -> (String, String) {
    let detail = clean_git_detail(raw);
    let haystack = raw.to_lowercase();
    let title = if haystack.contains("[rejected]")
        || haystack.contains("non-fast-forward")
        || haystack.contains("failed to push some refs")
        || haystack.contains("fetch first")
    {
        tr!("git_panel.fail_push_rejected")
    } else if haystack.contains("not possible to fast-forward")
        || haystack.contains("divergent branches")
        || haystack.contains("need to specify how to reconcile")
    {
        tr!("git_panel.fail_diverged")
    } else if haystack.contains("automatic merge failed")
        || haystack.contains("merge conflict")
        || haystack.contains("conflict")
    {
        tr!("git_panel.fail_merge_conflicts")
    } else if haystack.contains("would be overwritten") || haystack.contains("your local changes") {
        tr!("git_panel.fail_local_overwritten")
    } else if haystack.contains("authentication failed")
        || haystack.contains("could not read username")
        || haystack.contains("could not read password")
        || haystack.contains("permission denied")
        || haystack.contains("403 forbidden")
        || haystack.contains("401 unauthorized")
    {
        tr!("git_panel.fail_auth")
    } else if haystack.contains("repository not found")
        || haystack.contains("does not appear to be a git repository")
        || haystack.contains("couldn't find remote ref")
        || haystack.contains("no such remote")
    {
        tr!("git_panel.fail_remote_not_found")
    } else if haystack.contains("could not resolve host")
        || haystack.contains("unable to access")
        || haystack.contains("network is unreachable")
        || haystack.contains("timed out")
    {
        tr!("git_panel.fail_network")
    } else if haystack.contains("no upstream branch")
        || haystack.contains("has no upstream branch")
        || haystack.contains("no branch to push")
    {
        tr!("git_panel.fail_no_upstream")
    } else if haystack.contains("not a git repository") {
        tr!("git_panel.fail_not_a_repo")
    } else {
        tr!("git_panel.fail_generic")
    };
    (title, detail)
}

/// Tidy raw Git output for display: trim trailing whitespace, drop repeated
/// blank lines, and never hand the banner an empty string.
fn clean_git_detail(raw: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut prev_blank = false;
    for line in raw.lines() {
        let line = line.trim_end();
        let blank = line.is_empty();
        if blank && prev_blank {
            continue;
        }
        lines.push(line);
        prev_blank = blank;
    }
    let joined = lines.join("\n").trim().to_string();
    if joined.is_empty() {
        tr!("git_panel.git_no_error_message")
    } else {
        joined
    }
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

    #[test]
    fn rejected_push_is_explained() {
        let raw = "To https://github.com/acme/repo.git\n \
                   ! [rejected]        main -> main (fetch first)\n\
                   error: failed to push some refs to 'https://github.com/acme/repo.git'\n\
                   hint: Updates were rejected because the remote contains work\n\
                   hint: that you do not have locally.";
        let (title, detail) = classify_git_failure(raw);
        assert!(title.contains("Push rejected"), "{title}");
        // The full multi-line stderr survives for the banner to wrap.
        assert!(detail.contains("failed to push some refs"));
        assert!(detail.contains("Updates were rejected"));
    }

    #[test]
    fn diverged_and_conflict_failures_are_recognized() {
        let (diverged, _) = classify_git_failure("fatal: Not possible to fast-forward, aborting.");
        assert!(diverged.contains("diverged"), "{diverged}");

        let raw = "CONFLICT (content): Merge conflict in src/lib.rs\n\
                   Automatic merge failed; fix conflicts and then commit the result.";
        let (conflict, detail) = classify_git_failure(raw);
        assert!(conflict.contains("Merge conflicts"), "{conflict}");
        assert!(detail.contains("Automatic merge failed"));
    }

    #[test]
    fn auth_and_network_failures_are_recognized() {
        let (auth, _) = classify_git_failure(
            "remote: Invalid username or password.\nfatal: Authentication failed for 'https://github.com/acme/repo.git/'",
        );
        assert!(auth.contains("Authentication failed"), "{auth}");

        let (network, _) = classify_git_failure(
            "fatal: unable to access 'https://github.com/acme/repo.git/': Could not resolve host: github.com",
        );
        assert!(network.contains("Network error"), "{network}");
    }

    #[test]
    fn clean_git_detail_collapses_blank_runs_and_defaults() {
        assert_eq!(clean_git_detail(""), "git exited without an error message");
        assert_eq!(
            clean_git_detail("  \n \n"),
            "git exited without an error message"
        );
        assert_eq!(clean_git_detail("one\n\n\n\ntwo\n"), "one\n\ntwo",);
    }
}
