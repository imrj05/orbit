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

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::{
    canvas, deferred, div, fill, hsla, img, point, prelude::*, px, size, AnyElement, Background,
    Bounds, ClickEvent, Context, Corner, CursorStyle, Entity, Focusable, FontWeight, Hsla,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, PathBuilder, Pixels,
    Render, ScrollHandle, Subscription, Window,
};

use crate::app::{
    anchored_popover, button_frame, context_menu_separator, icon, icon_button_frame, icon_dyn,
    input_field_frame, menu_header, nerd_font_family, picker_entry, picker_search_frame,
    picker_surface, press, provider_mark_color, refresh_glyph, spinner, ControlExt, ModelEntry,
    PressedRamp, StatefulControlExt, TipExt, BUTTON_GROUP,
};
use crate::commit_message;
use crate::commit_model::{CommitModel, CommitModelConfig};
use crate::gh;
use crate::gh_templates::{self, RepoTemplate};
use crate::git::{self, CommitEntry, StatusRow};
use crate::git_ops::{self, InProgress};
use crate::issue_message::{self, DraftKind};
use crate::model_selector::provider_icon;
use crate::sidepane::{FileActions, SectionActions, SidePane};
use crate::theme::tokens::RaisedExt;
use crate::theme::tokens::{
    button, context_menu, input, panel, picker, ButtonSize, DynamicSpacing, IconSize, Radius,
    StyledExt, TextSize,
};
use crate::theme::{self, Theme, ThemeMode};
use crate::usage::tooltip::Tooltip;

mod failure;
pub(crate) mod widgets;

use failure::{ActionError, RecoveryAction};
use widgets::{
    action_button, check_box, empty_note, load_more, ref_badge, row_button, status_color,
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

/// Start a new task for a GitHub detail in the panel's workspace.
pub type StartGithubTask = std::rc::Rc<dyn Fn(PathBuf, String, &mut Window, &mut gpui::App)>;

const HISTORY_PAGE: usize = 40;

/// The most changed-file rows a commit/PR detail renders. Beyond this the
/// list is summarized, so one enormous commit cannot stall a frame.
const MAX_FILE_ROWS: usize = 300;

/// How long the header refresh button keeps spinning after a click, so a
/// fast git read still reads as acknowledged (the same floor Settings uses).
const REFRESH_FEEDBACK: Duration = Duration::from_millis(650);

/// Width of the branch / PR-base / ref pickers — one family, one width.
const BRANCH_MENU_W: f32 = 320.;

/// The GitHub list pane: its built-in width, the bounds a drag may reach, and
/// the width the detail pane beside it always keeps.
pub(crate) const GITHUB_LIST_DEFAULT_W: f32 = 320.;
pub(crate) const GITHUB_LIST_MIN_W: f32 = 240.;
pub(crate) const GITHUB_LIST_MAX_W: f32 = 420.;
const GITHUB_DETAIL_MIN_W: f32 = 400.;
/// The grab strip between the list and the detail. Its 1px divider is drawn
/// inside it, so the divider is a handle rather than a static rule.
const GITHUB_LIST_HANDLE_W: f32 = 7.;

/// Width of the commit bar's model picker, and how tall its list grows before
/// it scrolls (roughly eight rows).
const COMMIT_MODEL_MENU_W: f32 = 340.;
const COMMIT_MODEL_LIST_MAX_H: f32 = 300.;

/// Width of the "…" action dropdowns (branch actions, file actions).
const ACTION_MENU_W: f32 = 220.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitTab {
    Changes,
    History,
    Graph,
    Stashes,
    Issues,
    Pulls,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelMenuTarget {
    Filter,
    Issue,
}

/// Which commit-bar action is running, for the button state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GitAction {
    Commit,
    CommitAndPush,
    Push,
    Pull,
    Sync,
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
    /// Merge a pull request after confirmation.
    MergePull {
        number: u64,
        method: gh::MergeMethod,
        delete_branch: bool,
    },
    /// Discard every unstaged change in the working tree.
    DiscardAll,
    /// Discard one changed file's unstaged edits.
    DiscardPath(String),
}

/// Which ref picker is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RefTarget {
    /// "Merge branch…": merge the chosen ref into the current branch.
    Merge,
    /// "Rebase onto…": rebase the current branch onto the chosen ref.
    Rebase,
    /// The new-PR form's base branch picker.
    PrBase,
}

/// What the branch-name prompt is for.
#[derive(Clone, Debug, PartialEq, Eq)]
enum BranchPrompt {
    /// Create and check out a branch from HEAD.
    New,
    /// Rename the current branch (carries its name).
    Rename(String),
}

/// Which New form's repository-template picker is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DraftForm {
    Issue,
    PullRequest,
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
    /// The selected model's context window, when the catalog reports one. Sizes
    /// the commit-message prompt so a big change set still fits.
    context_window: Option<u64>,

    // ── commit bar ──
    message: Entity<crate::composer::ComposerInput>,
    /// The model the commit message is generated with: an explicit pick from
    /// the commit bar's chip, or the session model. Persisted by
    /// [`crate::commit_model`].
    commit_model: CommitModelConfig,
    /// pi's model catalog, for the chip's searchable list and the picked
    /// model's context window.
    models: Vec<ModelEntry>,
    /// Whether the commit bar's model picker is open, its search field, the
    /// highlighted row, and the list's scroll position.
    commit_model_menu_open: bool,
    commit_model_filter: Entity<crate::composer::ComposerInput>,
    _commit_model_filter_sub: Subscription,
    commit_model_needle: String,
    commit_model_highlight: usize,
    commit_model_scroll: ScrollHandle,
    /// Set when an outside mouse-down closed the menu, so the chip's following
    /// mouse-up does not toggle it back open.
    commit_model_dismissed_at: Option<Instant>,
    include_unstaged: bool,
    /// How many messages were generated for the current change set, and the
    /// last one shown. A regeneration is told about both so it cannot quietly
    /// repeat the message the user rejected.
    generate_attempt: usize,
    last_generated: Option<String>,
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
    /// The Changes tab's review browser — the same view as the Review pane,
    /// embedded so the Git page shows the identical diff UI (source filter,
    /// wrap/split toggles, changed-files tree, inline diffs).
    review: Entity<SidePane>,

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

    // ── github (gh CLI) ──
    /// Whether the `gh` binary is installed and runnable.
    gh_installed: bool,
    /// Whether `gh` has a signed-in host.
    gh_authenticated: bool,
    /// Auth error or version line, shown in the Issues/Pulls setup state.
    gh_detail: String,
    /// The signed-in account (login + avatar), for the branch row's chip.
    gh_user: Option<gh::GhUser>,
    /// Whether the branch row's Sync dropdown is open.
    sync_menu_open: bool,
    /// When the outside-click dismiss last fired. The chevron's click lands
    /// right after it on the same press; without this the click would toggle
    /// the just-dismissed menu back open, so it can never close.
    sync_menu_dismissed_at: Option<Instant>,
    /// Whether the `gh` probe has run for this workspace. The probe is two
    /// subprocesses, so it is not repeated on every tab switch — only on the
    /// first load, on a workspace change, and on the manual refresh.
    gh_probed: bool,

    // ── issues (gh) ──
    issues: Vec<gh::GhIssue>,
    issues_loading: bool,
    issues_error: Option<String>,
    issue_filter: gh::IssueFilter,
    issue_search: Entity<crate::composer::ComposerInput>,
    /// The issue whose detail view is open, if any.
    issue_detail: Option<gh::GhIssue>,
    issue_detail_loading: bool,
    issue_request: u64,
    selected_issue: Option<u64>,
    issue_comment: Entity<crate::composer::ComposerInput>,
    issue_comment_owner: Option<u64>,
    issue_comment_drafts: HashMap<u64, String>,
    /// Repository labels for the picker.
    issue_labels: Vec<gh::GhLabel>,
    label_menu: Option<LabelMenuTarget>,
    label_menu_dismissed: Option<(LabelMenuTarget, Instant)>,
    /// True while an issue mutation runs.
    issue_busy: bool,
    /// The new-issue form.
    issue_new_open: bool,
    issue_new_title: Entity<crate::composer::ComposerInput>,
    issue_new_body: Entity<crate::composer::ComposerInput>,
    /// Optional free-form notes that steer the generated issue draft.
    issue_new_hint: Entity<crate::composer::ComposerInput>,
    /// True while a one-shot pi draft request runs for the issue form.
    issue_generating: bool,

    // ── pull requests (gh) ──
    pulls: Vec<gh::GhPull>,
    pulls_loading: bool,
    pulls_error: Option<String>,
    pr_filter: gh::PrFilter,
    pr_search: Entity<crate::composer::ComposerInput>,
    /// The PR whose detail view is open, if any.
    pr_detail: Option<gh::GhPull>,
    pr_detail_loading: bool,
    /// Full-detail failure while the list's summary remains usable.
    pr_detail_error: Option<String>,
    pr_request: u64,
    selected_pr: Option<u64>,
    pr_comment: Entity<crate::composer::ComposerInput>,
    pr_comment_owner: Option<u64>,
    pr_comment_drafts: HashMap<u64, String>,
    pr_busy: bool,
    /// The merge method the merge confirmation will use.
    pr_merge_method: gh::MergeMethod,
    pr_delete_branch: bool,
    /// The new-PR form.
    pr_new_open: bool,
    pr_new_title: Entity<crate::composer::ComposerInput>,
    pr_new_body: Entity<crate::composer::ComposerInput>,
    /// Optional free-form notes that steer the generated PR draft.
    pr_new_hint: Entity<crate::composer::ComposerInput>,
    /// True while a one-shot pi draft request runs for the PR form.
    pr_generating: bool,
    pr_new_base: Option<String>,
    pr_new_draft: bool,

    // ── repository templates (issue / pull request) ──
    /// Issue templates discovered under `.github/ISSUE_TEMPLATE` (or the
    /// legacy single-file locations).
    issue_templates: Vec<RepoTemplate>,
    /// Pull-request templates discovered in the repository.
    pr_templates: Vec<RepoTemplate>,
    /// The selected issue template (`None` = a blank body).
    issue_template: Option<usize>,
    /// The selected pull-request template (`None` = a blank body).
    pr_template: Option<usize>,
    /// Which New form's template menu is open, if any.
    template_menu: Option<DraftForm>,

    /// Width of the Git page, supplied by its host after sidebar layout.
    page_width: Pixels,
    /// The width the user picked for the GitHub list pane in the Issues/Pulls
    /// split view. This is the preference: [`github_list_pane_width`] clamps
    /// only the displayed width, so a narrow window never overwrites it.
    github_list_width: Pixels,
    /// Where the divider drag started, so it tracks the pointer from the width
    /// that was on screen rather than re-anchoring every move.
    github_list_drag: Option<GithubListDrag>,

    // ── header ──
    branch: Option<String>,
    ahead_behind: Option<(usize, usize)>,
    /// Whether HEAD resolves (drives "Publish branch" on a fresh branch).
    has_commits: bool,
    branches: Vec<git::BranchEntry>,
    /// Branches most recently checked out (from the HEAD reflog), newest
    /// first: the picker's "Recent branches" section.
    recent_branches: Vec<String>,
    branch_menu_open: bool,
    /// The branch picker's search field.
    branch_filter: Entity<crate::composer::ComposerInput>,
    /// Keeps the picker re-rendering as the search field changes.
    _branch_filter_sub: Subscription,
    /// Set when the branch menu is dismissed by an outside mouse-down so the
    /// chip's following mouse-up does not toggle it straight back open.
    menu_dismissed_at: Option<Instant>,
    /// Whether the branch-actions ("…") dropdown beside Sync is open.
    branch_more_open: bool,
    /// Outside-click dismiss timestamp for the "…" menu (its own gesture
    /// guard, so it never collides with the branch chip's).
    branch_more_dismissed_at: Option<Instant>,
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
    /// Opens a changed file's diff in the Review pane (installed by the app).
    on_open_file: Option<OpenFile>,
    /// Opens a file in the Files editor (installed by the app).
    on_open_path: Option<OpenPath>,
    on_start_github_task: Option<StartGithubTask>,
    /// Leaves the Git page entirely (installed by the app).
    on_close: Option<Close>,
}

impl GitPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let message = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.commit_message_leave_blank_to_generate")
                // Not `Composer Picker`: the commit box owns Enter (newline)
                // and Cmd/Ctrl+Enter (commit) through the `GitCommitMessage`
                // bindings, so no picker chords are needed here.
                .with_key_context("Composer GitCommitMessage")
                .with_max_lines(6)
        });
        let branch_input = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.branch_name_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let issue_search = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.issue_search_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let issue_comment = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.issue_comment_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(5)
        });
        let issue_new_title = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.issue_title_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let issue_new_body = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.issue_body_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(8)
        });
        let issue_new_hint = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.draft_notes_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(3)
        });
        let pr_search = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.pr_search_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let pr_comment = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.pr_comment_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(5)
        });
        let pr_new_title = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.pr_title_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let pr_new_body = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.pr_body_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(10)
        });
        let pr_new_hint = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.draft_notes_placeholder")
                .with_key_context("Composer Picker")
                .with_max_lines(3)
        });
        let branch_filter = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.search_branches")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let branch_filter_sub = cx.observe(&branch_filter, |_, _, cx| cx.notify());
        let commit_model_filter = cx.new(|cx| {
            crate::composer::ComposerInput::new(cx)
                .with_placeholder_key("git_panel.search_models")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let commit_model_filter_sub = cx.observe(&commit_model_filter, |this, filter, cx| {
            let needle = filter.read(cx).text().to_lowercase();
            if needle != this.commit_model_needle {
                // Filtering lands on the first *model*, not on the session
                // row, so typing + Enter does not silently reset the choice.
                let rows = commit_model_rows(&this.models, &needle);
                this.commit_model_needle = needle;
                this.commit_model_highlight = if rows.len() > 1 { 1 } else { 0 };
                cx.notify();
            }
        });
        // The embedded review browser shares the Review pane's rendering, so
        // the Git page's Changes tab is exactly the Review UI.
        let review = cx.new(|cx| {
            let mut pane = SidePane::new(cx);
            pane.set_embedded(true, cx);
            pane
        });
        Self {
            open: false,
            tab: GitTab::Changes,
            workspace: None,
            provider: String::new(),
            model: String::new(),
            context_window: None,
            message,
            commit_model: CommitModelConfig::load(),
            models: Vec::new(),
            commit_model_menu_open: false,
            commit_model_filter,
            _commit_model_filter_sub: commit_model_filter_sub,
            commit_model_needle: String::new(),
            commit_model_highlight: 0,
            commit_model_scroll: ScrollHandle::new(),
            commit_model_dismissed_at: None,
            include_unstaged: false,
            generate_attempt: 0,
            last_generated: None,
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
            review,
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
            gh_installed: false,
            gh_authenticated: false,
            gh_detail: String::new(),
            gh_user: None,
            sync_menu_open: false,
            sync_menu_dismissed_at: None,
            gh_probed: false,
            issues: Vec::new(),
            issues_loading: false,
            issues_error: None,
            issue_filter: gh::IssueFilter::default(),
            issue_search,
            issue_detail: None,
            issue_detail_loading: false,
            issue_request: 0,
            selected_issue: None,
            issue_comment,
            issue_comment_owner: None,
            issue_comment_drafts: HashMap::new(),
            issue_labels: Vec::new(),
            label_menu: None,
            label_menu_dismissed: None,
            issue_busy: false,
            issue_new_open: false,
            issue_new_title,
            issue_new_body,
            issue_new_hint,
            issue_generating: false,
            pulls: Vec::new(),
            pulls_loading: false,
            pulls_error: None,
            pr_filter: gh::PrFilter::default(),
            pr_search,
            pr_detail: None,
            pr_detail_loading: false,
            pr_detail_error: None,
            pr_request: 0,
            selected_pr: None,
            pr_comment,
            pr_comment_owner: None,
            pr_comment_drafts: HashMap::new(),
            pr_busy: false,
            pr_merge_method: gh::MergeMethod::default(),
            pr_delete_branch: false,
            pr_new_open: false,
            pr_new_title,
            pr_new_body,
            pr_new_hint,
            pr_generating: false,
            pr_new_base: None,
            pr_new_draft: false,
            issue_templates: Vec::new(),
            pr_templates: Vec::new(),
            issue_template: None,
            pr_template: None,
            template_menu: None,
            page_width: px(0.),
            github_list_width: px(
                crate::layout::github_list_width().unwrap_or(GITHUB_LIST_DEFAULT_W)
            ),
            github_list_drag: None,
            branch: None,
            ahead_behind: None,
            has_commits: false,
            branches: Vec::new(),
            recent_branches: Vec::new(),
            branch_menu_open: false,
            branch_filter,
            _branch_filter_sub: branch_filter_sub,
            menu_dismissed_at: None,
            branch_more_open: false,
            branch_more_dismissed_at: None,
            branch_operation: false,
            refresh_spin_until: None,
            status: None,
            remote_web: None,
            on_open_file: None,
            on_open_path: None,
            on_start_github_task: None,
            on_close: None,
        }
    }

    pub fn show(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        // Wake the embedded review browser; it loads the diff on demand.
        self.review.update(cx, |pane, cx| pane.show_review(cx));
        self.refresh_all(cx);
        cx.notify();
    }

    pub fn hide(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        // Park the embedded review browser so it stops reloading while the
        // Git page is off screen.
        self.review.update(cx, |pane, cx| pane.close(cx));
        self.branch_menu_open = false;
        self.branch_more_open = false;
        self.commit_model_menu_open = false;
        cx.notify();
    }

    /// Switch to a tab by index (0=Changes, 1=History, 2=Graph, 3=Stashes,
    /// 4=Issues, 5=Pull requests), reloading it. Driven by the ⌘1–⌘6
    /// shortcuts.
    pub fn set_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let tab = match index {
            1 => GitTab::History,
            2 => GitTab::Graph,
            3 => GitTab::Stashes,
            4 => GitTab::Issues,
            5 => GitTab::Pulls,
            _ => GitTab::Changes,
        };
        if self.tab != tab {
            self.tab = tab;
            self.branch_menu_open = false;
            self.branch_more_open = false;
            self.commit_model_menu_open = false;
            self.ref_menu = None;
            self.refresh_all(cx);
        }
        cx.notify();
    }

    /// The active tab's shortcut index (`0` = Changes … `5` = Pull requests).
    /// Tests use it to assert which tab a chip or shortcut lands on.
    #[cfg(test)]
    pub(crate) fn active_tab_index(&self) -> usize {
        match self.tab {
            GitTab::Changes => 0,
            GitTab::History => 1,
            GitTab::Graph => 2,
            GitTab::Stashes => 3,
            GitTab::Issues => 4,
            GitTab::Pulls => 5,
        }
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
        context_window: Option<u64>,
        models: Vec<ModelEntry>,
        cx: &mut Context<Self>,
    ) {
        if workspace != self.workspace {
            self.workspace = workspace;
            self.commit_model_menu_open = false;
            // The embedded review browser follows the workspace too, so its
            // diff reloads for the new repository.
            let review_workspace = self.workspace.clone();
            self.review
                .update(cx, |pane, cx| pane.set_workspace(review_workspace, cx));
            self.staged.clear();
            self.unstaged.clear();
            // The old tree's per-section actions must not linger on the new
            // one.
            self.review
                .update(cx, |pane, cx| pane.set_section_actions(None, cx));
            self.history.clear();
            self.graph.clear();
            self.remote_web = None;
            self.status = None;
            self.selected_commit = None;
            self.commit_detail = None;
            self.history_filter = git::HistoryFilter::default();
            self.file_menu = None;
            self.issues.clear();
            self.selected_issue = None;
            self.issue_detail = None;
            self.issue_detail_loading = false;
            self.issue_comment_owner = None;
            self.issue_comment_drafts.clear();
            self.issue_comment.update(cx, |input, cx| input.clear(cx));
            self.label_menu = None;
            self.issue_new_open = false;
            self.issue_generating = false;
            self.issue_filter = gh::IssueFilter::default();
            self.pulls.clear();
            self.selected_pr = None;
            self.pr_detail = None;
            self.pr_detail_loading = false;
            self.pr_detail_error = None;
            self.pr_comment_owner = None;
            self.pr_comment_drafts.clear();
            self.pr_comment.update(cx, |input, cx| input.clear(cx));
            self.pr_new_open = false;
            self.pr_generating = false;
            self.pr_filter = gh::PrFilter::default();
            self.issue_templates.clear();
            self.pr_templates.clear();
            self.issue_template = None;
            self.pr_template = None;
            self.template_menu = None;
            self.gh_probed = false;
            if self.open {
                self.refresh_all(cx);
            }
        }
        self.provider = provider;
        self.model = model;
        self.context_window = context_window;
        self.models = models;
    }

    /// Install the callback that opens a changed file's diff in Review.
    pub fn set_open_file(&mut self, open: OpenFile) {
        self.on_open_file = Some(open);
    }

    /// Install the callback that opens a workspace file in the Files editor.
    pub fn set_open_path(&mut self, open: OpenPath) {
        self.on_open_path = Some(open);
    }

    /// Install the callback for starting a task from a GitHub detail.
    pub fn set_start_github_task(&mut self, start: StartGithubTask) {
        self.on_start_github_task = Some(start);
    }

    /// Queue a new task after the panel's event handler releases its borrow.
    /// `review` selects a PR review rather than an issue fix.
    fn start_github_task(
        &mut self,
        url: &str,
        review: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(start) = self.on_start_github_task.clone() else {
            return;
        };
        let Some(workspace) = self.workspace.clone() else {
            self.set_status("No workspace is available for this task.");
            cx.notify();
            return;
        };
        let url = url.trim();
        if !url.starts_with("https://") || url.chars().any(char::is_whitespace) {
            self.set_status("The GitHub item has no valid URL. Refresh and try again.");
            cx.notify();
            return;
        }
        let prompt = if review {
            format!("Review this pull request: {url}")
        } else {
            format!("Fix this issue: {url}")
        };
        window.defer(cx, move |window, cx| start(workspace, prompt, window, cx));
    }

    /// Keep the embedded review browser's session id and turn number current.
    /// The Git page does not render Last Turn; the pane stores the pair
    /// without reloading its diff.
    pub fn set_review_turn_context(
        &mut self,
        session: Option<String>,
        latest_turn: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.review.update(cx, |pane, cx| {
            pane.set_turn_context(session, latest_turn, cx)
        });
    }

    /// Feed the embedded review browser the width of the surface hosting it,
    /// so its responsive toolbar and changed-files tree match the page.
    pub fn set_review_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if self.page_width != width {
            self.page_width = width;
            cx.notify();
        }
        self.review
            .update(cx, |pane, cx| pane.set_available_width(width, cx));
    }

    /// Apply a stored GitHub list width. This is the user's preference, so it is
    /// clamped only to the draggable range — never to the detail reserve, which
    /// belongs to the displayed width alone.
    pub fn set_github_list_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let width = width.clamp(px(GITHUB_LIST_MIN_W), px(GITHUB_LIST_MAX_W));
        if self.github_list_width != width {
            self.github_list_width = width;
            cx.notify();
        }
    }

    /// Mark the embedded review browser stale so its diff reloads on the next
    /// paint. Skipped while the Git page is closed, so a settled run does not
    /// read a diff nobody is looking at.
    pub fn mark_review_stale(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.review
                .update(cx, |pane, cx| pane.mark_review_stale(cx));
        }
    }

    /// Rebuild the per-file action rows the embedded review shows beside each
    /// changed file (stage / unstage / discard), reflecting the latest status.
    /// Called whenever `git status` lands so the rows never go stale.
    fn install_review_actions(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        let staged: HashSet<String> = self.staged.iter().map(|row| row.path.clone()).collect();
        let unstaged: HashSet<String> = self.unstaged.iter().map(|row| row.path.clone()).collect();
        let actions: FileActions = std::rc::Rc::new(move |file, theme| {
            let path = file.path.clone();
            let is_staged = staged.contains(path.as_str());
            let is_unstaged = unstaged.contains(path.as_str());
            if !is_staged && !is_unstaged {
                return None;
            }
            let mut row = div()
                .flex()
                .flex_none()
                .items_center()
                .gap(DynamicSpacing::Base02.px(&theme))
                .opacity(0.)
                .group_hover("review-file-header", |s| s.opacity(1.));
            if is_unstaged {
                let stage_path = path.clone();
                let weak_stage = weak.clone();
                row = row.child(review_action_icon(
                    format!("review-stage-{path}"),
                    "icons/plus.svg",
                    "git_panel.tip_stage",
                    theme,
                    move |_, _, cx| {
                        let _ =
                            weak_stage.update(cx, |panel, cx| panel.stage(stage_path.clone(), cx));
                    },
                ));
            }
            if is_staged {
                let unstage_path = path.clone();
                let weak_unstage = weak.clone();
                row = row.child(review_action_icon(
                    format!("review-unstage-{path}"),
                    "icons/minus.svg",
                    "git_panel.tip_unstage",
                    theme,
                    move |_, _, cx| {
                        let _ = weak_unstage
                            .update(cx, |panel, cx| panel.unstage(unstage_path.clone(), cx));
                    },
                ));
            }
            if is_unstaged {
                let discard_path = path.clone();
                let weak_discard = weak.clone();
                row = row.child(review_action_icon(
                    format!("review-discard-{path}"),
                    "icons/trash.svg",
                    "git_panel.tip_discard",
                    theme,
                    move |_, _, cx| {
                        let _ = weak_discard.update(cx, |panel, cx| {
                            panel.pending_confirm =
                                Some(PendingConfirm::DiscardPath(discard_path.clone()));
                            cx.notify();
                        });
                    },
                ));
            }
            Some(row.into_any_element())
        });
        self.review
            .update(cx, |pane, cx| pane.set_file_actions(actions, cx));
    }

    /// Install the per-section bulk actions the embedded review attaches to
    /// its grouped strips: Stage all plus the whole-tree discard on the
    /// Changes section, and Unstage all on the Staged section. Rebuilt
    /// alongside the per-file rows so each strip's buttons always match the
    /// latest status, and cleared on a clean tree.
    fn install_review_section_actions(&mut self, cx: &mut Context<Self>) {
        if self.staged.is_empty() && self.unstaged.is_empty() {
            self.review
                .update(cx, |pane, cx| pane.set_section_actions(None, cx));
            return;
        }
        let weak = cx.entity().downgrade();
        let has_staged = !self.staged.is_empty();
        let has_unstaged = !self.unstaged.is_empty();
        // The strips carry the localized label the parser was fed, so the host
        // matches on the same strings to route each section its own action.
        let staged_label = tr!("git_panel.section_staged");
        let changes_label = tr!("git_panel.section_changes");
        let actions: SectionActions = std::rc::Rc::new(move |label, sticky, theme| {
            let id = |slug: &str| {
                gpui::ElementId::Name(
                    format!(
                        "review-section-{slug}-{}",
                        if sticky { "sticky" } else { "row" }
                    )
                    .into(),
                )
            };
            if label == changes_label.as_str() {
                let mut row = div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme));
                if has_unstaged {
                    let weak = weak.clone();
                    row = row.child(review_toolbar_button(
                        id("stage-all"),
                        "icons/plus.svg",
                        tr!("git_panel.stage_all"),
                        theme.text_secondary,
                        theme,
                        move |_, _, cx| {
                            let _ = weak.update(cx, |panel, cx| panel.stage_all(cx));
                        },
                    ));
                }
                // Discard every unstaged change behind a confirmation; the
                // Staged section is a deliberate state and stays untouched.
                if has_unstaged {
                    let weak = weak.clone();
                    row = row.child(review_toolbar_button(
                        id("discard-all"),
                        "icons/trash.svg",
                        tr!("git_panel.discard_all"),
                        theme.error,
                        theme,
                        move |_, _, cx| {
                            let _ = weak.update(cx, |panel, cx| {
                                panel.pending_confirm = Some(PendingConfirm::DiscardAll);
                                cx.notify();
                            });
                        },
                    ));
                }
                return (has_unstaged || !has_staged).then(|| row.into_any_element());
            }
            if has_staged && label == staged_label.as_str() {
                let weak = weak.clone();
                return Some(review_toolbar_button(
                    id("unstage-all"),
                    "icons/minus.svg",
                    tr!("git_panel.unstage_all"),
                    theme.text_secondary,
                    theme,
                    move |_, _, cx| {
                        let _ = weak.update(cx, |panel, cx| panel.unstage_all(cx));
                    },
                ));
            }
            None
        });
        self.review
            .update(cx, |pane, cx| pane.set_section_actions(Some(actions), cx));
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
        self.refresh_templates(cx);
        match self.tab {
            GitTab::History => self.refresh_history(cx),
            GitTab::Graph => self.refresh_graph(cx),
            GitTab::Issues => self.refresh_issues(cx),
            GitTab::Pulls => self.refresh_pulls(cx),
            // Stashes ride along with the branch refresh above.
            GitTab::Changes | GitTab::Stashes => {}
        }
    }

    /// Refresh from the header button: same reload, plus a short minimum spin
    /// so the click is visibly acknowledged even when the read is instant.
    fn refresh_from_button(&mut self, cx: &mut Context<Self>) {
        // The manual refresh re-probes `gh` (auth may have changed since).
        self.gh_probed = false;
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
            GitTab::Issues => self.issues_loading,
            GitTab::Pulls => self.pulls_loading,
            GitTab::Stashes => false,
        }
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.changes_loading = true;
        self.changes_error = None;
        // The embedded review browser owns the diff read; marking it stale
        // here reloads it in step with `git status`. Only the Changes tab
        // mounts it, so the reload waits for a tab that shows it.
        if self.tab == GitTab::Changes {
            self.review
                .update(cx, |pane, cx| pane.mark_review_stale(cx));
        }
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
                        let staged: Vec<StatusRow> =
                            rows.iter().filter(|row| row.staged()).cloned().collect();
                        let unstaged: Vec<StatusRow> =
                            rows.iter().filter(|row| row.unstaged()).cloned().collect();
                        // Only a changed change set retires the generated message:
                        // the watcher refreshes on every save, and a regeneration
                        // for the same diff must still see the rejected attempt.
                        if staged != panel.staged || unstaged != panel.unstaged {
                            panel.generate_attempt = 0;
                            panel.last_generated = None;
                        }
                        panel.staged = staged;
                        panel.unstaged = unstaged;
                        panel.git_operation = operation;
                        panel.conflicts = conflicts;
                        panel.install_review_actions(cx);
                        panel.install_review_section_actions(cx);
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
                    git::list_branch_entries(&cwd).unwrap_or_default(),
                    git::recent_branches(&cwd),
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
                    recent_branches,
                    has_commits,
                    remote_web,
                    refs,
                    stashes,
                )) = result
                {
                    panel.branch = branch;
                    panel.ahead_behind = ahead_behind;
                    panel.branches = branches;
                    panel.recent_branches = recent_branches;
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
        if self.gh_probed {
            return;
        }
        let Some(cwd) = self.cwd() else {
            return;
        };
        self.spawn_data(
            cx,
            move || {
                let status = gh::status(&cwd);
                // The account chip needs the login + avatar; only ask `gh api`
                // once auth status says there is a signed-in host.
                let user = status
                    .authenticated
                    .then(|| gh::current_user(&cwd).ok())
                    .flatten();
                Ok((status, user))
            },
            |panel, result, cx| {
                if let Ok((status, user)) = result {
                    panel.gh_installed = status.installed;
                    panel.gh_authenticated = status.authenticated;
                    panel.gh_detail = status.detail;
                    panel.gh_user = user;
                }
                panel.gh_probed = true;
                cx.notify();
            },
        );
    }

    // ── issues ─────────────────────────────────────────────────────────

    /// Load the issue list under the current filter. Repository labels for the
    /// picker are fetched once, on the first load.
    fn refresh_issues(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.issues_loading = true;
        self.issues_error = None;
        let filter = self.issue_filter.clone();
        let need_labels = self.issue_labels.is_empty();
        self.spawn_data(
            cx,
            move || {
                let issues = gh::list_issues(&cwd, &filter)?;
                let labels = if need_labels {
                    gh::list_labels(&cwd).unwrap_or_default()
                } else {
                    Vec::new()
                };
                Ok((issues, labels))
            },
            |panel, result, cx| {
                panel.issues_loading = false;
                match result {
                    Ok((issues, labels)) => {
                        panel.issues = issues;
                        if !labels.is_empty() {
                            panel.issue_labels = labels;
                        }
                    }
                    Err(err) => panel.issues_error = Some(err),
                }
                cx.notify();
            },
        );
    }

    /// Load one issue (body + comments) into the detail view.
    fn open_issue(&mut self, number: u64, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        switch_comment_draft(
            &self.issue_comment,
            &mut self.issue_comment_owner,
            &mut self.issue_comment_drafts,
            number,
            cx,
        );
        self.issue_detail = None;
        self.selected_issue = Some(number);
        self.issue_request = self.issue_request.wrapping_add(1);
        let request = self.issue_request;
        self.issue_detail_loading = true;
        self.label_menu = None;
        cx.notify();
        self.spawn_data(
            cx,
            move || gh::view_issue(&cwd, number),
            move |panel, result, cx| {
                if panel.selected_issue != Some(number) || panel.issue_request != request {
                    return;
                }
                panel.issue_detail_loading = false;
                match result {
                    Ok(issue) => panel.issue_detail = Some(issue),
                    Err(err) => panel.set_failure(err),
                }
                cx.notify();
            },
        );
    }

    fn close_issue_detail(&mut self, cx: &mut Context<Self>) {
        self.selected_issue = None;
        self.issue_detail_loading = false;
        self.issue_detail = None;
        self.issue_new_open = false;
        self.label_menu = None;
        cx.notify();
    }

    /// Run an issue mutation off-thread, then reload the list and any open
    /// detail so the page reflects GitHub's truth.
    fn run_issue_op<F>(&mut self, work: F, cx: &mut Context<Self>)
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        self.issue_busy = true;
        cx.notify();
        self.spawn_data(cx, work, |panel, result, cx| {
            panel.issue_busy = false;
            match result {
                Ok(note) => {
                    panel.clear_failure();
                    panel.set_status(note);
                }
                Err(err) => panel.set_failure(err),
            }
            if panel.tab == GitTab::Issues {
                panel.refresh_issues(cx);
            }
            if let Some(number) = panel.issue_detail.as_ref().map(|issue| issue.number) {
                panel.open_issue(number, cx);
            }
            cx.notify();
        });
    }

    fn comment_issue(&mut self, cx: &mut Context<Self>) {
        let Some(number) = self.issue_detail.as_ref().map(|issue| issue.number) else {
            return;
        };
        let body = self.issue_comment.read(cx).text();
        if body.trim().is_empty() {
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        self.issue_comment.update(cx, |input, cx| input.clear(cx));
        self.run_issue_op(move || gh::comment_issue(&cwd, number, &body), cx);
    }

    fn toggle_issue_state(&mut self, cx: &mut Context<Self>) {
        let Some(issue) = self.issue_detail.as_ref() else {
            return;
        };
        let (number, open) = (issue.number, issue.is_open());
        let Some(cwd) = self.cwd() else { return };
        self.run_issue_op(
            move || {
                if open {
                    gh::close_issue(&cwd, number, None)
                } else {
                    gh::reopen_issue(&cwd, number)
                }
            },
            cx,
        );
    }

    fn toggle_issue_label(&mut self, label: String, cx: &mut Context<Self>) {
        let Some(issue) = self.issue_detail.as_ref() else {
            return;
        };
        let number = issue.number;
        let has = issue.labels.iter().any(|entry| entry.name == label);
        let Some(cwd) = self.cwd() else { return };
        let add = if has { Vec::new() } else { vec![label.clone()] };
        let remove = if has { vec![label] } else { Vec::new() };
        self.run_issue_op(
            move || gh::edit_issue_labels(&cwd, number, &add, &remove),
            cx,
        );
    }

    fn create_issue(&mut self, cx: &mut Context<Self>) {
        let title = self.issue_new_title.read(cx).text().trim().to_string();
        if title.is_empty() {
            self.set_failure(tr!("git_panel.issue_title_required"));
            cx.notify();
            return;
        }
        let body = self.issue_new_body.read(cx).text();
        let labels = self
            .issue_template
            .and_then(|index| self.issue_templates.get(index))
            .map(|template| template.labels.clone())
            .unwrap_or_default();
        let Some(cwd) = self.cwd() else { return };
        self.issue_busy = true;
        cx.notify();
        self.spawn_data(
            cx,
            move || gh::create_issue(&cwd, None, &title, &body, &labels, &[], &[]),
            |panel, result, cx| {
                panel.issue_busy = false;
                match result {
                    Ok(url) => {
                        panel.clear_failure();
                        panel.set_status(tr!("git_panel.issue_created"));
                        panel.issue_new_open = false;
                        panel
                            .issue_new_title
                            .update(cx, |input, cx| input.clear(cx));
                        panel.issue_new_body.update(cx, |input, cx| input.clear(cx));
                        panel.issue_new_hint.update(cx, |input, cx| input.clear(cx));
                        if let Some(number) = url.rsplit('/').next().and_then(|n| n.parse().ok()) {
                            panel.open_issue(number, cx);
                        }
                    }
                    Err(err) => panel.set_failure(err),
                }
                panel.refresh_issues(cx);
                cx.notify();
            },
        );
    }

    /// Load the repository's issue and pull-request templates off-thread.
    fn refresh_templates(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.spawn_data(
            cx,
            move || {
                Ok::<_, String>((
                    gh_templates::issue_templates(&cwd),
                    gh_templates::pull_templates(&cwd),
                ))
            },
            |panel, result, cx| {
                if let Ok((issues, pulls)) = result {
                    let issues_were_empty = panel.issue_templates.is_empty();
                    let pulls_were_empty = panel.pr_templates.is_empty();
                    panel.issue_templates = issues;
                    panel.pr_templates = pulls;
                    panel.issue_template = panel
                        .issue_template
                        .filter(|index| *index < panel.issue_templates.len());
                    panel.pr_template = panel
                        .pr_template
                        .filter(|index| *index < panel.pr_templates.len());

                    // The templates can land after the form opened (they load
                    // off-thread). Apply the lone template the first time it
                    // becomes known, and only onto an untouched body.
                    let apply_issue = issues_were_empty
                        && panel.issue_new_open
                        && panel.issue_template.is_none()
                        && panel.issue_templates.len() == 1
                        && panel.issue_new_body.read(cx).text().trim().is_empty();
                    if apply_issue {
                        panel.apply_issue_template(Some(0), cx);
                    }
                    let apply_pull = pulls_were_empty
                        && panel.pr_new_open
                        && panel.pr_template.is_none()
                        && panel.pr_templates.len() == 1
                        && panel.pr_new_body.read(cx).text().trim().is_empty();
                    if apply_pull {
                        panel.apply_pull_template(Some(0), cx);
                    }
                }
                cx.notify();
            },
        );
    }

    /// The label for the issue form's template chip.
    fn issue_template_label(&self) -> String {
        match self
            .issue_template
            .and_then(|index| self.issue_templates.get(index))
        {
            Some(template) => template.label(),
            None => tr!("git_panel.template_blank"),
        }
    }

    /// The label for the pull-request form's template chip.
    fn pr_template_label(&self) -> String {
        match self
            .pr_template
            .and_then(|index| self.pr_templates.get(index))
        {
            Some(template) => template.label(),
            None => tr!("git_panel.template_blank"),
        }
    }

    /// Open the New issue form, applying the lone issue template when the repo
    /// has exactly one (a chooser would just add a click).
    fn open_issue_new(&mut self, cx: &mut Context<Self>) {
        self.issue_new_open = true;
        self.issue_generating = false;
        self.issue_detail_loading = false;
        self.label_menu = None;
        self.selected_issue = None;
        self.issue_detail = None;
        self.template_menu = None;
        if self.issue_templates.is_empty() {
            self.refresh_templates(cx);
        }
        let default = (self.issue_templates.len() == 1).then_some(0);
        self.apply_issue_template(default, cx);
    }

    /// Open the New pull request form, applying the lone PR template when the
    /// repo has exactly one.
    fn open_pull_new(&mut self, cx: &mut Context<Self>) {
        self.pr_new_base = Some(self.default_base());
        self.pr_new_open = true;
        self.pr_generating = false;
        self.pr_detail_loading = false;
        self.selected_pr = None;
        self.pr_detail = None;
        self.template_menu = None;
        self.ref_menu = None;
        if self.pr_templates.is_empty() {
            self.refresh_templates(cx);
        }
        let default = (self.pr_templates.len() == 1).then_some(0);
        self.apply_pull_template(default, cx);
    }

    /// Pick an issue template (`None` = blank), prefilling the title prefix and
    /// the body. A blank pick only clears a body that still holds a template
    /// skeleton, so typed prose is never lost.
    fn apply_issue_template(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.issue_template = index.filter(|index| *index < self.issue_templates.len());
        self.template_menu = None;
        match self
            .issue_template
            .and_then(|index| self.issue_templates.get(index))
            .cloned()
        {
            Some(template) => {
                self.prefill_title(
                    &self.issue_new_title.clone(),
                    template.title_prefix.as_deref(),
                    cx,
                );
                set_composer_text(&self.issue_new_body, &template.body, cx);
            }
            None => self.clear_template_body(&self.issue_new_body.clone(), cx),
        }
        cx.notify();
    }

    /// Pick a pull-request template (`None` = blank).
    fn apply_pull_template(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        self.pr_template = index.filter(|index| *index < self.pr_templates.len());
        self.template_menu = None;
        match self
            .pr_template
            .and_then(|index| self.pr_templates.get(index))
            .cloned()
        {
            Some(template) => {
                self.prefill_title(
                    &self.pr_new_title.clone(),
                    template.title_prefix.as_deref(),
                    cx,
                );
                set_composer_text(&self.pr_new_body, &template.body, cx);
            }
            None => self.clear_template_body(&self.pr_new_body.clone(), cx),
        }
        cx.notify();
    }

    /// Set the title to the template's prefix when the field is still empty.
    fn prefill_title(
        &self,
        input: &Entity<crate::composer::ComposerInput>,
        prefix: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(prefix) = prefix.filter(|value| !value.is_empty()) else {
            return;
        };
        if input.read(cx).text().trim().is_empty() {
            set_composer_text(input, prefix, cx);
        }
    }

    /// Clear a body field only when it still matches a known template skeleton,
    /// so switching back to Blank does not wipe the user's own words.
    fn clear_template_body(
        &self,
        input: &Entity<crate::composer::ComposerInput>,
        cx: &mut Context<Self>,
    ) {
        let current = input.read(cx).text();
        let is_skeleton = self
            .issue_templates
            .iter()
            .chain(self.pr_templates.iter())
            .any(|template| template.body == current);
        if is_skeleton {
            set_composer_text(input, "", cx);
        }
    }

    fn set_template(&mut self, target: DraftForm, index: Option<usize>, cx: &mut Context<Self>) {
        match target {
            DraftForm::Issue => self.apply_issue_template(index, cx),
            DraftForm::PullRequest => self.apply_pull_template(index, cx),
        }
    }

    /// The repository-template picker for either New form. Rendered inside the
    /// form card (which is `relative`), so it tracks the chip without a global
    /// anchor.
    /// A template chip that owns its popover, so the menu anchors to the chip
    /// itself rather than the form card.
    fn template_selector(
        &self,
        theme: Theme,
        target: DraftForm,
        chip_id: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = match target {
            DraftForm::Issue => self.issue_template_label(),
            DraftForm::PullRequest => self.pr_template_label(),
        };
        let open = self.template_menu == Some(target);
        let popup = if open {
            self.template_picker_popup(theme, target, cx)
        } else {
            None
        };
        div()
            .relative()
            .flex()
            .child(dropdown_chip(
                chip_id,
                &label,
                "icons/file.svg",
                open,
                theme,
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.template_menu = if this.template_menu == Some(target) {
                        None
                    } else {
                        Some(target)
                    };
                    cx.notify();
                }),
            ))
            .children(popup)
            .into_any_element()
    }

    /// The repository-template picker, styled like the top branch selector
    /// (picker surface, list inset, one picker row per entry) and anchored to
    /// the chip that opened it.
    fn template_picker_popup(
        &self,
        theme: Theme,
        target: DraftForm,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.template_menu != Some(target) {
            return None;
        }
        let (templates, current) = match target {
            DraftForm::Issue => (&self.issue_templates, self.issue_template),
            DraftForm::PullRequest => (&self.pr_templates, self.pr_template),
        };
        let id = match target {
            DraftForm::Issue => "git-issue-template-menu",
            DraftForm::PullRequest => "git-pr-template-menu",
        };
        let scroll_id = match target {
            DraftForm::Issue => "git-issue-template-scroll",
            DraftForm::PullRequest => "git-pr-template-scroll",
        };

        let mut list = div()
            .id(scroll_id)
            .w_full()
            .max_h(px(300.))
            .overflow_y_scroll()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();
        list = list.child(
            picker_entry(div().id("git-template-blank"), &theme)
                .cursor_pointer()
                .pressed_state(&theme, PressedRamp::Wash)
                .text_color(theme.text_secondary)
                .hover_state(&theme)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_template(target, None, cx)
                }))
                .child(icon(
                    "icons/file.svg",
                    context_menu::ICON.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(tr!("git_panel.template_blank")),
                )
                .when(current.is_none(), |row| {
                    row.child(icon(
                        "icons/check.svg",
                        context_menu::ICON.px(&theme),
                        theme.accent_text,
                    ))
                }),
        );
        if templates.is_empty() {
            list = list.child(
                picker_entry(div(), &theme)
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.template_none")),
            );
        }
        for (index, template) in templates.iter().enumerate() {
            let selected = current == Some(index);
            list = list.child(
                picker_entry(
                    div().id(gpui::ElementId::Name(
                        format!("git-template-{index}").into(),
                    )),
                    &theme,
                )
                .cursor_pointer()
                .text_color(theme.text_secondary)
                .hover_state(&theme)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.set_template(target, Some(index), cx)
                }))
                .child(icon(
                    "icons/file.svg",
                    context_menu::ICON.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(template.label())
                        .child(
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(template.path.clone()),
                        ),
                )
                .when(selected, |row| {
                    row.child(icon(
                        "icons/check.svg",
                        context_menu::ICON.px(&theme),
                        theme.accent_text,
                    ))
                }),
            );
        }

        let menu = picker_surface(div().id(id), &theme)
            .absolute()
            .left(px(0.))
            .w(px(BRANCH_MENU_W))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.template_menu = None;
                cx.notify();
            }))
            .child(menu_header(tr!("git_panel.template"), &theme))
            .child(list);
        // Anchor to the chip: the issue form's chip sits near the top, so the
        // menu opens downward; the PR form's chip sits at the bottom, so it
        // opens upward instead of being clipped by the scroll container.
        let menu = match target {
            DraftForm::Issue => menu.top(px(30.)),
            DraftForm::PullRequest => menu.bottom(px(30.)),
        };
        // `deferred` keeps the menu's layout in this subtree (so it anchors to
        // the chip) but paints it after the form's other fields, which would
        // otherwise draw over it.
        Some(deferred(menu).into_any_element())
    }

    /// A base-branch chip that owns its popover, mirroring [`Self::template_selector`]
    /// so the picker anchors to the chip.
    fn base_branch_selector(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let label = self
            .pr_new_base
            .clone()
            .unwrap_or_else(|| tr!("git_panel.choose_base"));
        let popup = if self.ref_menu == Some(RefTarget::PrBase) {
            self.pr_base_popup(theme, cx)
        } else {
            None
        };
        div()
            .relative()
            .flex()
            .child(dropdown_chip(
                "git-pr-new-base",
                &label,
                "icons/branch.svg",
                self.ref_menu == Some(RefTarget::PrBase),
                theme,
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    // mouse-down-out closes the picker; the chip's mouse-up would
                    // otherwise toggle it straight back open on the same gesture
                    // (see AGENT.md popovers).
                    const GESTURE: Duration = Duration::from_millis(200);
                    if let Some(dismissed) = this.menu_dismissed_at.take() {
                        if dismissed.elapsed() < GESTURE {
                            return;
                        }
                    }
                    let opening = this.ref_menu != Some(RefTarget::PrBase);
                    this.ref_menu = if opening {
                        Some(RefTarget::PrBase)
                    } else {
                        None
                    };
                    if opening {
                        this.clear_branch_filter(cx);
                        window.focus(&this.branch_filter.read(cx).focus_handle(cx));
                    }
                    cx.notify();
                }),
            ))
            .children(popup)
            .into_any_element()
    }

    /// The New-PR base picker, styled and anchored like the top branch
    /// selector: a search row over the repository's local branches.
    fn pr_base_popup(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.ref_menu != Some(RefTarget::PrBase) {
            return None;
        }
        let needle = self.branch_filter.read(cx).text().trim().to_lowercase();
        let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
        let current = self.pr_new_base.clone();
        let mut locals: Vec<String> = self
            .refs
            .iter()
            .filter(|entry| entry.kind == git::RefKind::Branch && matches(entry.name.as_str()))
            .map(|entry| entry.name.clone())
            .collect();
        locals.sort_by_key(|name| name.to_lowercase());

        let mut list = div()
            .id("git-pr-base-scroll")
            .w_full()
            .max_h(px(320.))
            .overflow_y_scroll()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();
        if locals.is_empty() {
            list = list.child(
                picker_entry(div(), &theme)
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.no_matching_branches")),
            );
        } else {
            list = list.child(branch_section_label(
                &tr!("git_panel.local_branches"),
                theme,
            ));
            for name in locals {
                let selected = current.as_deref() == Some(name.as_str());
                let branch = name.clone();
                let id = gpui::ElementId::Name(format!("git-pr-base-{name}").into());
                list = list.child(
                    picker_entry(div().id(id), &theme)
                        .cursor_pointer()
                        .when(selected, |row| {
                            row.bg(theme.surface_selected)
                                .text_color(theme.selection_fg)
                        })
                        .when(!selected, |row| {
                            row.text_color(theme.text_secondary).hover_state(&theme)
                        })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.pr_new_base = Some(branch.clone());
                            this.close_ref_picker(cx);
                        }))
                        .child(icon(
                            "icons/branch.svg",
                            context_menu::ICON.px(&theme),
                            if selected {
                                theme.text_primary
                            } else {
                                theme.text_muted
                            },
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(name.clone()),
                        )
                        .when(selected, |row| {
                            row.child(icon(
                                "icons/check.svg",
                                context_menu::ICON.px(&theme),
                                theme.text_primary,
                            ))
                        }),
                );
            }
        }

        let menu = picker_surface(div().id("git-pr-base-menu"), &theme)
            .absolute()
            .bottom(px(30.))
            .left(px(0.))
            .w(px(BRANCH_MENU_W))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_ref_picker(cx);
            }))
            .child(
                picker_search_frame(div(), &theme)
                    .child(icon(
                        "icons/search.svg",
                        input::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(div().flex_1().min_w_0().child(self.branch_filter.clone())),
            )
            .child(list);
        Some(deferred(menu).into_any_element())
    }

    /// Ask pi for an issue draft (title + body) from the optional notes field
    /// plus the repository context, falling back to a local heuristic.
    fn generate_issue_draft(&mut self, cx: &mut Context<Self>) {
        if self.issue_generating || self.issue_busy {
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        let hint = self.issue_new_hint.read(cx).text();
        let template = self
            .issue_template
            .and_then(|index| self.issue_templates.get(index))
            .cloned();
        let title_prefix = template
            .as_ref()
            .and_then(|entry| entry.title_prefix.clone());
        let provider = self.provider.clone();
        let model = self.model.clone();
        self.issue_generating = true;
        self.set_status(tr!("git_panel.generating_draft"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let provider = (!provider.is_empty()).then_some(provider.as_str());
                    let model = (!model.is_empty()).then_some(model.as_str());
                    match issue_message::generate(
                        &cwd,
                        provider,
                        model,
                        DraftKind::Issue,
                        &hint,
                        None,
                        template.as_ref(),
                    ) {
                        Ok(draft) => draft,
                        Err(_) => issue_message::heuristic(
                            &cwd,
                            DraftKind::Issue,
                            &hint,
                            None,
                            template.as_ref(),
                        ),
                    }
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.issue_generating = false;
                set_composer_text(
                    &panel.issue_new_title,
                    &with_title_prefix(&title_prefix, &result.title),
                    cx,
                );
                set_composer_text(&panel.issue_new_body, &result.body, cx);
                panel.set_status(tr!("git_panel.draft_ready"));
                cx.notify();
            });
        })
        .detach();
    }

    fn set_issue_state(&mut self, state: gh::IssueState, cx: &mut Context<Self>) {
        if self.issue_filter.state == state {
            return;
        }
        self.issue_filter.state = state;
        self.refresh_issues(cx);
        cx.notify();
    }

    fn apply_issue_search(&mut self, cx: &mut Context<Self>) {
        let text = self.issue_search.read(cx).text().trim().to_string();
        self.issue_filter.search = (!text.is_empty()).then_some(text);
        self.refresh_issues(cx);
        cx.notify();
    }

    fn toggle_label_menu(&mut self, target: LabelMenuTarget, cx: &mut Context<Self>) {
        let just_dismissed = self
            .label_menu_dismissed
            .take()
            .is_some_and(|(dismissed, at)| {
                dismissed == target && at.elapsed() < Duration::from_millis(200)
            });
        if !just_dismissed {
            self.label_menu = (self.label_menu != Some(target)).then_some(target);
            cx.notify();
        }
    }

    fn set_issue_label_filter(&mut self, label: Option<String>, cx: &mut Context<Self>) {
        self.issue_filter.label = label;
        self.label_menu = None;
        self.refresh_issues(cx);
        cx.notify();
    }

    fn clear_issue_filter(&mut self, cx: &mut Context<Self>) {
        self.issue_filter = gh::IssueFilter::default();
        let len = self.issue_search.read(cx).text().len();
        self.issue_search
            .update(cx, |input, cx| input.replace_range(0..len, "", cx));
        self.refresh_issues(cx);
        cx.notify();
    }

    // ── pull requests ──────────────────────────────────────────────────

    fn refresh_pulls(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.pulls_loading = true;
        self.pulls_error = None;
        let filter = self.pr_filter.clone();
        self.spawn_data(
            cx,
            move || gh::list_pulls(&cwd, &filter),
            |panel, result, cx| {
                panel.pulls_loading = false;
                match result {
                    Ok(pulls) => panel.pulls = pulls,
                    Err(err) => panel.pulls_error = Some(err),
                }
                cx.notify();
            },
        );
    }

    fn open_pull(&mut self, number: u64, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        switch_comment_draft(
            &self.pr_comment,
            &mut self.pr_comment_owner,
            &mut self.pr_comment_drafts,
            number,
            cx,
        );
        // The list already carries the summary. Keep painting it while the
        // larger description/timeline request is in flight, and on failure.
        self.pr_detail = None;
        self.pr_detail_error = None;
        self.selected_pr = Some(number);
        self.pr_request = self.pr_request.wrapping_add(1);
        let request = self.pr_request;
        self.pr_detail_loading = true;
        cx.notify();
        self.spawn_data(
            cx,
            move || gh::view_pull(&cwd, number),
            move |panel, result, cx| {
                if panel.selected_pr != Some(number) || panel.pr_request != request {
                    return;
                }
                panel.pr_detail_loading = false;
                match result {
                    Ok(pull) => panel.pr_detail = Some(pull),
                    Err(err) => {
                        panel.pr_detail_error = Some(err.clone());
                        panel.set_failure(err);
                    }
                }
                cx.notify();
            },
        );
    }

    fn close_pull_detail(&mut self, cx: &mut Context<Self>) {
        self.selected_pr = None;
        self.pr_detail_loading = false;
        self.pr_detail_error = None;
        self.pr_detail = None;
        self.pr_new_open = false;
        cx.notify();
    }

    /// Run a PR mutation off-thread, then reload the list and any open detail.
    fn run_pr_op<F>(&mut self, work: F, cx: &mut Context<Self>)
    where
        F: FnOnce() -> Result<String, String> + Send + 'static,
    {
        self.pr_busy = true;
        cx.notify();
        self.spawn_data(cx, work, |panel, result, cx| {
            panel.pr_busy = false;
            match result {
                Ok(note) => {
                    panel.clear_failure();
                    panel.set_status(note);
                }
                Err(err) => panel.set_failure(err),
            }
            if panel.tab == GitTab::Pulls {
                panel.refresh_pulls(cx);
            }
            if let Some(number) = panel.pr_detail.as_ref().map(|pull| pull.number) {
                panel.open_pull(number, cx);
            }
            cx.notify();
        });
    }

    fn comment_pull(&mut self, cx: &mut Context<Self>) {
        let Some(number) = self.pr_detail.as_ref().map(|pull| pull.number) else {
            return;
        };
        let body = self.pr_comment.read(cx).text();
        if body.trim().is_empty() {
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        self.pr_comment.update(cx, |input, cx| input.clear(cx));
        self.run_pr_op(move || gh::comment_pull(&cwd, number, &body), cx);
    }

    fn review_pull(&mut self, action: gh::ReviewAction, cx: &mut Context<Self>) {
        let Some(number) = self.pr_detail.as_ref().map(|pull| pull.number) else {
            return;
        };
        let body = self.pr_comment.read(cx).text();
        let Some(cwd) = self.cwd() else { return };
        self.pr_comment.update(cx, |input, cx| input.clear(cx));
        self.run_pr_op(
            move || gh::review_pull(&cwd, number, action, Some(&body)),
            cx,
        );
    }

    /// Confirm the merge first; the popup carries the chosen method.
    fn request_merge_pull(&mut self, cx: &mut Context<Self>) {
        let Some(number) = self.pr_detail.as_ref().map(|pull| pull.number) else {
            return;
        };
        self.pending_confirm = Some(PendingConfirm::MergePull {
            number,
            method: self.pr_merge_method,
            delete_branch: self.pr_delete_branch,
        });
        cx.notify();
    }

    fn toggle_pull_state(&mut self, cx: &mut Context<Self>) {
        let Some(pull) = self.pr_detail.as_ref() else {
            return;
        };
        let (number, open) = (pull.number, pull.is_open());
        let Some(cwd) = self.cwd() else { return };
        self.run_pr_op(
            move || {
                if open {
                    gh::close_pull(&cwd, number)
                } else {
                    gh::reopen_pull(&cwd, number)
                }
            },
            cx,
        );
    }

    fn checkout_pull(&mut self, cx: &mut Context<Self>) {
        let Some(number) = self.pr_detail.as_ref().map(|pull| pull.number) else {
            return;
        };
        // `gh pr checkout` rewrites the worktree; never run it dirty or
        // mid-operation.
        if self.git_operation.is_some() || !self.staged.is_empty() || !self.unstaged.is_empty() {
            self.set_failure(tr!("git_panel.checkout_dirty"));
            cx.notify();
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        self.run_pr_op(move || gh::checkout_pull(&cwd, number), cx);
    }

    fn create_pull(&mut self, cx: &mut Context<Self>) {
        let title = self.pr_new_title.read(cx).text().trim().to_string();
        if title.is_empty() {
            self.set_failure(tr!("git_panel.pr_title_required"));
            cx.notify();
            return;
        }
        let body = self.pr_new_body.read(cx).text();
        let base = self.pr_new_base.clone().unwrap_or_default();
        let draft = self.pr_new_draft;
        let labels = self
            .pr_template
            .and_then(|index| self.pr_templates.get(index))
            .map(|template| template.labels.clone())
            .unwrap_or_default();
        let Some(cwd) = self.cwd() else { return };
        self.pr_busy = true;
        cx.notify();
        self.spawn_data(
            cx,
            move || gh::create_pull(&cwd, &title, &body, &base, draft, &labels),
            |panel, result, cx| {
                panel.pr_busy = false;
                match result {
                    Ok(url) => {
                        panel.clear_failure();
                        panel.set_status(tr!("git_panel.pr_created"));
                        panel.pr_new_open = false;
                        panel.pr_new_title.update(cx, |input, cx| input.clear(cx));
                        panel.pr_new_body.update(cx, |input, cx| input.clear(cx));
                        panel.pr_new_hint.update(cx, |input, cx| input.clear(cx));
                        if let Some(number) = url.rsplit('/').next().and_then(|n| n.parse().ok()) {
                            panel.open_pull(number, cx);
                        }
                    }
                    Err(err) => panel.set_failure(err),
                }
                panel.refresh_pulls(cx);
                cx.notify();
            },
        );
    }

    /// Ask pi for a pull-request draft from the optional notes field plus the
    /// branch's commits and diff against the chosen base.
    fn generate_pull_draft(&mut self, cx: &mut Context<Self>) {
        if self.pr_generating || self.pr_busy {
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        let hint = self.pr_new_hint.read(cx).text();
        let base = self.pr_new_base.clone();
        let template = self
            .pr_template
            .and_then(|index| self.pr_templates.get(index))
            .cloned();
        let title_prefix = template
            .as_ref()
            .and_then(|entry| entry.title_prefix.clone());
        let provider = self.provider.clone();
        let model = self.model.clone();
        self.pr_generating = true;
        self.set_status(tr!("git_panel.generating_draft"));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let provider = (!provider.is_empty()).then_some(provider.as_str());
                    let model = (!model.is_empty()).then_some(model.as_str());
                    match issue_message::generate(
                        &cwd,
                        provider,
                        model,
                        DraftKind::PullRequest,
                        &hint,
                        base.as_deref(),
                        template.as_ref(),
                    ) {
                        Ok(draft) => draft,
                        Err(_) => issue_message::heuristic(
                            &cwd,
                            DraftKind::PullRequest,
                            &hint,
                            base.as_deref(),
                            template.as_ref(),
                        ),
                    }
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.pr_generating = false;
                set_composer_text(
                    &panel.pr_new_title,
                    &with_title_prefix(&title_prefix, &result.title),
                    cx,
                );
                set_composer_text(&panel.pr_new_body, &result.body, cx);
                panel.set_status(tr!("git_panel.draft_ready"));
                cx.notify();
            });
        })
        .detach();
    }

    fn set_pr_state(&mut self, state: gh::PrState, cx: &mut Context<Self>) {
        if self.pr_filter.state == state {
            return;
        }
        self.pr_filter.state = state;
        self.refresh_pulls(cx);
        cx.notify();
    }

    fn apply_pr_search(&mut self, cx: &mut Context<Self>) {
        let text = self.pr_search.read(cx).text().trim().to_string();
        self.pr_filter.search = (!text.is_empty()).then_some(text);
        self.refresh_pulls(cx);
        cx.notify();
    }

    fn clear_pr_filter(&mut self, cx: &mut Context<Self>) {
        self.pr_filter = gh::PrFilter::default();
        let len = self.pr_search.read(cx).text().len();
        self.pr_search
            .update(cx, |input, cx| input.replace_range(0..len, "", cx));
        self.refresh_pulls(cx);
        cx.notify();
    }

    /// The default base branch for a new PR (`main`/`master` when present).
    fn default_base(&self) -> String {
        for candidate in ["main", "master"] {
            if self.refs.iter().any(|entry| entry.name == candidate) {
                return candidate.to_string();
            }
        }
        self.refs
            .iter()
            .find(|entry| {
                entry.kind == git::RefKind::Branch
                    && Some(entry.name.as_str()) != self.branch.as_deref()
            })
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| "main".to_string())
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

    /// Discard every unstaged change in the working tree; staged changes stay.
    fn discard_all_changes(&mut self, cx: &mut Context<Self>) {
        if self.pending.is_some() || self.operation_busy {
            return;
        }
        let Some(cwd) = self.cwd() else { return };
        self.pending_discard = None;
        self.spawn_data(
            cx,
            move || git::discard_unstaged(&cwd),
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
        // The same gate the commit buttons get from `can_commit`: no work in
        // flight and something to commit. Cmd/Ctrl+Enter reaches this without
        // a disabled button in the way.
        if self.pending.is_some() || self.generating || self.operation_busy {
            return;
        }
        let can_commit =
            !self.staged.is_empty() || (self.include_unstaged && !self.unstaged.is_empty());
        if !can_commit {
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

    /// Whether a detail/new surface is showing inside a tab (an issue or PR
    /// detail, or a new-issue/new-PR form). Escape steps back to the list
    /// from here before it leaves the Git page. Not gated on the loading
    /// flags: while a detail loads the list is still on screen, so Escape
    /// keeps its page-level meaning.
    pub fn has_transient_view(&self) -> bool {
        self.selected_issue.is_some()
            || self.issue_new_open
            || self.selected_pr.is_some()
            || self.pr_new_open
    }

    /// Close the topmost detail/new surface inside a tab, returning to its
    /// list. Returns whether anything was showing.
    pub fn close_transient_view(&mut self, cx: &mut Context<Self>) -> bool {
        let had = self.has_transient_view();
        if had {
            self.selected_issue = None;
            self.issue_detail = None;
            self.issue_detail_loading = false;
            self.issue_new_open = false;
            self.issue_generating = false;
            self.selected_pr = None;
            self.pr_detail = None;
            self.pr_detail_loading = false;
            self.pr_new_open = false;
            self.pr_generating = false;
            self.label_menu = None;
            self.template_menu = None;
            self.ref_menu = None;
            cx.notify();
        }
        had
    }

    fn generate_then(&mut self, action: Option<GitAction>, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.generating = true;
        self.commit_model_menu_open = false;
        self.set_status(tr!("git_panel.generating_commit_message"));
        cx.notify();
        // The panel's own pick, else the session model (see
        // [`CommitModelConfig::resolve`]). An empty pair lets pi use its own
        // default rather than forcing a model the user never chose.
        let picked = self.commit_model.resolve(&self.provider, &self.model);
        let provider = picked
            .as_ref()
            .map(|model| model.provider.clone())
            .unwrap_or_default();
        let model = picked
            .as_ref()
            .map(|model| model.id.clone())
            .unwrap_or_default();
        let context_window = match &self.commit_model.model {
            Some(model) => self
                .models
                .iter()
                .find(|entry| entry.provider == model.provider && entry.id == model.id)
                .and_then(|entry| entry.context_window),
            None => self.context_window,
        };
        let include_unstaged = self.include_unstaged;
        let attempt = self.generate_attempt;
        let previous = self.last_generated.clone();
        cx.spawn(async move |this, cx| {
            let result: Result<(String, Option<String>), String> = cx
                .background_executor()
                .spawn(async move {
                    // Read the status here rather than trusting the cached rows:
                    // a commit flow stages its changes just before calling this,
                    // and the cached rows would still be the pre-stage set.
                    let rows = git::status_rows(&cwd)?;
                    // The scope decides which patch the prompt describes, so the
                    // message always matches what the commit records.
                    let scope = if include_unstaged {
                        commit_message::Scope::Worktree
                    } else {
                        commit_message::Scope::Staged
                    };
                    let rows: Vec<_> = rows.into_iter().filter(|row| scope.matches(row)).collect();
                    if rows.is_empty() {
                        return Err(tr!("git.no_staged_changes"));
                    }
                    let provider = (!provider.is_empty()).then_some(provider.as_str());
                    let model = (!model.is_empty()).then_some(model.as_str());
                    let options = commit_message::Options {
                        provider,
                        model,
                        scope,
                        attempt,
                        previous: previous.as_deref(),
                        context_window,
                    };
                    match commit_message::generate(&cwd, &options, &rows) {
                        Ok(message) => Ok((message, None)),
                        // Keep the button useful without a model, but say why:
                        // a filename-derived fallback passed off as a generated
                        // message is what makes a wrong message look like a bug.
                        Err(err) => Ok((commit_message::heuristic(&rows), Some(err))),
                    }
                })
                .await;
            let _ = this.update(cx, |panel, cx| {
                panel.generating = false;
                match result {
                    Ok((message, warning)) => {
                        let len = panel.message.read(cx).text().len();
                        panel
                            .message
                            .update(cx, |input, cx| input.replace_range(0..len, &message, cx));
                        panel.last_generated = Some(message.clone());
                        panel.generate_attempt += 1;
                        if let Some(action) = action {
                            panel.run_git_action(action, Some(message), cx);
                            return;
                        }
                        match warning {
                            Some(err) => panel
                                .set_failure(tr!("git_panel.commit_message_fallback", error = err)),
                            None => panel.set_status(tr!("git_panel.commit_message_ready")),
                        }
                    }
                    Err(err) => panel.set_failure(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The commit bar's model chip: the model Generate will ask, with the
    /// provider's mark and a caret. Clicking opens the picker; while a message
    /// is being generated the chip is inert, since the in-flight call already
    /// captured its model.
    fn commit_model_controls(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let open = self.commit_model_menu_open;
        let enabled = !self.generating;
        let provider = match &self.commit_model.model {
            Some(model) => model.provider.clone(),
            None => self.provider.clone(),
        };
        div()
            .relative()
            .flex_none()
            .children(self.commit_model_menu(theme, cx))
            .child(
                button_frame(div().id("git-commit-model"), &theme, ButtonSize::Medium)
                    .border_1()
                    .border_color(if open {
                        theme.border_strong
                    } else {
                        theme.border
                    })
                    .when(open, |chip| chip.bg(theme.overlay))
                    .when(!open, |chip| chip.raised(theme.surface_elevated, &theme))
                    .when(enabled, |chip| chip.cursor_pointer().hover_bordered(&theme))
                    .when(!enabled, |chip| chip.disabled_state(&theme))
                    .when(enabled, |chip| chip.text_color(theme.text_secondary))
                    .tip(tr!(
                        "git_panel.commit_model_tip",
                        name = self.commit_model_label()
                    ))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        // The commit box's own mouse-up refocuses the message
                        // editor; keep it from stealing the popup's search
                        // field.
                        cx.stop_propagation();
                        if this.generating {
                            return;
                        }
                        this.toggle_commit_model_menu(window, cx);
                    }))
                    .child(icon_dyn(
                        provider_icon(&provider),
                        ButtonSize::Medium.icon_size().px(&theme),
                        provider_mark_color(&provider, &theme, theme.text_muted),
                    ))
                    .child(
                        div()
                            .max_w(px(150.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(self.commit_model_label()),
                    )
                    .child(icon(
                        "icons/chevron-down.svg",
                        IconSize::XSmall.px(&theme),
                        if open {
                            theme.text_primary
                        } else {
                            theme.text_muted
                        },
                    )),
            )
            .into_any_element()
    }

    /// Catalog name for a provider/model pair, falling back to the raw id so an
    /// override pi has not listed (a custom provider) still reads as something.
    fn model_label(&self, provider: &str, id: &str) -> String {
        self.models
            .iter()
            .find(|entry| entry.id == id && (provider.is_empty() || entry.provider == provider))
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// The chip's label: the picked model, or the session model it follows.
    fn commit_model_label(&self) -> String {
        match &self.commit_model.model {
            Some(model) => self.model_label(&model.provider, &model.id),
            None if self.model.is_empty() => tr!("settings.active_session_model"),
            None => self.model_label(&self.provider, &self.model),
        }
    }

    /// The picker's selected row: 0 is "follow the session"; the rest index
    /// the visible catalog rows. `None` when an override is set but the search
    /// filtered its row out, so nothing is falsely marked as chosen.
    fn commit_model_selected_row(&self, rows: &[Option<usize>]) -> Option<usize> {
        let Some(picked) = self.commit_model.model.as_ref() else {
            return Some(0);
        };
        rows.iter().position(|row| {
            row.is_some_and(|ix| {
                let entry = &self.models[ix];
                entry.provider == picked.provider && entry.id == picked.id
            })
        })
    }

    fn toggle_commit_model_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // An outside mouse-down closed the menu on this same press; its
        // following mouse-up is the dismissal, not a reopen (the branch chip's
        // gesture guard).
        let just_dismissed = self
            .commit_model_dismissed_at
            .take()
            .is_some_and(|at| at.elapsed() < Duration::from_millis(200));
        if just_dismissed {
            // The click that dismissed the menu was this chip: the editor held
            // focus before the popup opened, so give it back instead of leaving
            // it on the now-hidden search field.
            self.message.read(cx).focus_handle(cx).focus(window);
            return;
        }
        if self.commit_model_menu_open {
            self.close_commit_model_menu(cx);
            self.message.read(cx).focus_handle(cx).focus(window);
            return;
        }
        self.commit_model_menu_open = true;
        self.commit_model_filter
            .update(cx, |input, cx| input.clear(cx));
        self.commit_model_needle.clear();
        let rows = commit_model_rows(&self.models, "");
        self.commit_model_highlight = self.commit_model_selected_row(&rows).unwrap_or(0);
        self.commit_model_scroll
            .scroll_to_item(self.commit_model_highlight);
        window.focus(&self.commit_model_filter.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn close_commit_model_menu(&mut self, cx: &mut Context<Self>) {
        if self.commit_model_menu_open {
            self.commit_model_menu_open = false;
            cx.notify();
        }
    }

    /// ↑/↓: move the highlight through the visible rows and keep it on screen.
    fn move_commit_model_highlight(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = commit_model_rows(&self.models, &self.commit_model_needle);
        let last = rows.len().saturating_sub(1);
        let current = self.commit_model_highlight.min(last);
        self.commit_model_highlight = if delta < 0 {
            current.saturating_sub(1)
        } else {
            (current + 1).min(last)
        };
        self.commit_model_scroll
            .scroll_to_item(self.commit_model_highlight);
        cx.notify();
    }

    /// Enter: activate the highlighted row.
    fn confirm_commit_model(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = commit_model_rows(&self.models, &self.commit_model_needle);
        let ix = self
            .commit_model_highlight
            .min(rows.len().saturating_sub(1));
        if let Some(row) = rows.get(ix).copied() {
            self.pick_commit_model(row, window, cx);
        }
    }

    /// Persist a pick (`None` = follow the session model) and return focus to
    /// the message editor so the next keystroke continues the commit message.
    fn pick_commit_model(
        &mut self,
        row: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_model.model = row
            .and_then(|ix| self.models.get(ix))
            .map(|entry| CommitModel {
                provider: entry.provider.clone(),
                id: entry.id.clone(),
            });
        self.commit_model.persist();
        self.commit_model_menu_open = false;
        self.message.read(cx).focus_handle(cx).focus(window);
        cx.notify();
    }

    /// The commit-model popover, anchored above the chip. Its rows are the
    /// session-model option followed by the filtered catalog, so the same
    /// search box serves a one-provider list and a full catalog.
    fn commit_model_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.commit_model_menu_open {
            return None;
        }
        let rows = commit_model_rows(&self.models, &self.commit_model_needle);
        let selected = self.commit_model_selected_row(&rows);
        let highlighted = self
            .commit_model_highlight
            .min(rows.len().saturating_sub(1));

        let mut list = div()
            .id("git-commit-model-scroll")
            .w_full()
            .max_h(px(COMMIT_MODEL_LIST_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.commit_model_scroll)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();
        for (ix, row) in rows.iter().enumerate() {
            list = list.child(self.commit_model_row(
                ix,
                *row,
                Some(ix) == selected,
                ix == highlighted,
                theme,
                cx,
            ));
        }

        let menu = picker_surface(div().id("git-commit-model-menu"), &theme)
            .w(px(COMMIT_MODEL_MENU_W))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            // A click inside the popup is the popup's business: without this
            // the commit box's mouse-up would refocus the message editor and
            // take the caret out of the search field.
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(|this, _: &crate::PickerCancel, window, cx| {
                this.close_commit_model_menu(cx);
                this.message.read(cx).focus_handle(cx).focus(window);
            }))
            .on_action(cx.listener(|this, _: &crate::PickerConfirm, window, cx| {
                this.confirm_commit_model(window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::PickerSelectNext, _, cx| {
                this.move_commit_model_highlight(1, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::PickerSelectPrev, _, cx| {
                this.move_commit_model_highlight(-1, cx)
            }))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.commit_model_dismissed_at = Some(Instant::now());
                this.close_commit_model_menu(cx);
            }))
            .child(
                picker_search_frame(div(), &theme)
                    .child(icon(
                        "icons/search.svg",
                        input::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.commit_model_filter.clone()),
                    ),
            )
            .child(list);

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size(px(0.))
                .child(anchored_popover(
                    Corner::BottomLeft,
                    point(px(0.), px(-6.)),
                    menu,
                    "anchored_popover-in-18",
                    &theme,
                ))
                .into_any_element(),
        )
    }

    /// One picker row: the session-model option (`None`) or a catalog model.
    /// Selection is a fill plus the trailing accent check; the keyboard cursor
    /// uses the same fill, so the two never look like different states.
    fn commit_model_row(
        &self,
        ix: usize,
        row: Option<usize>,
        selected: bool,
        highlighted: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (glyph, tint, primary, secondary) = match row {
            Some(model_ix) => {
                let model = &self.models[model_ix];
                (
                    provider_icon(&model.provider),
                    provider_mark_color(&model.provider, &theme, theme.text_muted),
                    model.name.clone(),
                    crate::providers::provider_display_name(&model.provider),
                )
            }
            None => (
                provider_icon(&self.provider),
                provider_mark_color(&self.provider, &theme, theme.text_muted),
                tr!("settings.active_session_model"),
                if self.model.is_empty() {
                    String::new()
                } else {
                    self.model_label(&self.provider, &self.model)
                },
            ),
        };
        let id = gpui::ElementId::NamedInteger("git-commit-model-row".into(), ix as u64);
        press(
            picker_entry(div().id(id).group(BUTTON_GROUP), &theme)
                .h(picker::entry_height(&theme))
                .flex_none()
                .cursor_pointer()
                .when(selected || highlighted, |row| {
                    row.bg(theme.surface_selected)
                        .text_color(theme.text_primary)
                })
                .when(!selected && !highlighted, |row| {
                    row.text_color(theme.text_secondary).hover_state(&theme)
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.pick_commit_model(row, window, cx);
                })),
        )
        .child(icon_dyn(glyph, context_menu::ICON.px(&theme), tint))
        .child(div().min_w_0().flex_1().truncate().child(primary))
        .children((!secondary.is_empty()).then(|| {
            div()
                .flex_none()
                .max_w(px(120.))
                .truncate()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(secondary)
        }))
        .when(selected, |row| {
            row.child(icon(
                "icons/check.svg",
                context_menu::ICON.px(&theme),
                theme.accent_text,
            ))
        })
        .into_any_element()
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
                        GitAction::Sync => git::sync(&cwd),
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
            PendingConfirm::MergePull {
                number,
                method,
                delete_branch,
            } => {
                let Some(cwd) = self.cwd() else { return };
                self.run_pr_op(
                    move || gh::merge_pull(&cwd, number, method, delete_branch),
                    cx,
                );
            }
            PendingConfirm::DiscardAll => self.discard_all_changes(cx),
            PendingConfirm::DiscardPath(path) => self.discard(path, cx),
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
        self.branch_more_open = false;
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
        self.branch_more_open = false;
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
        self.branch_more_open = false;
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

    fn toggle_branch_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.branch_more_open = false;
        let opening = !self.branch_menu_open;
        if opening {
            self.clear_branch_filter(cx);
        }
        self.branch_menu_open = opening;
        if opening {
            window.focus(&self.branch_filter.read(cx).focus_handle(cx));
        }
        cx.notify();
    }

    fn dismiss_branch_menu(&mut self, cx: &mut Context<Self>) {
        if self.branch_menu_open {
            self.branch_menu_open = false;
            self.menu_dismissed_at = Some(Instant::now());
            cx.notify();
        }
    }

    /// Toggle the branch-actions ("…") menu beside Sync.
    fn toggle_branch_more(&mut self, cx: &mut Context<Self>) {
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.branch_more_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        self.branch_menu_open = false;
        self.branch_more_open = !self.branch_more_open;
        cx.notify();
    }

    fn dismiss_branch_more(&mut self, cx: &mut Context<Self>) {
        if self.branch_more_open {
            self.branch_more_open = false;
            self.branch_more_dismissed_at = Some(Instant::now());
            cx.notify();
        }
    }

    /// Reset the branch picker's search field (on open, and after a switch).
    fn clear_branch_filter(&mut self, cx: &mut Context<Self>) {
        self.branch_filter.update(cx, |input, cx| input.clear(cx));
    }

    /// Check out a local branch, or a remote branch by creating its local
    /// tracking branch first.
    fn checkout_branch_ref(&mut self, branch: String, remote: bool, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        self.branch_menu_open = false;
        self.branch_more_open = false;
        self.menu_dismissed_at = Some(Instant::now());
        self.branch_operation = true;
        self.clear_branch_filter(cx);
        cx.notify();
        self.spawn_data(
            cx,
            move || {
                if remote {
                    git::checkout_remote_branch(&cwd, &branch)
                } else {
                    git::checkout_branch(&cwd, &branch)
                }
            },
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

    /// The Git card's title controls, hosted by the shell's top bar: the Back
    /// button and the "Git" title, replacing the session title while Git is
    /// open. Keeping them in the shared top bar means the card itself starts
    /// at its tab bar.
    pub fn top_bar_leading(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                press(
                    button_frame(div().id("git-back"), &theme, ButtonSize::Medium)
                        .cursor_pointer()
                        .hover_state(&theme),
                )
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
                .child(icon(
                    "icons/arrow-left.svg",
                    ButtonSize::Medium.icon_size().px(&theme),
                    theme.text_secondary,
                ))
                .child(
                    div()
                        .text_color(theme.text_secondary)
                        .child(tr!("git_panel.back")),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(icon(
                        "icons/git-commit.svg",
                        IconSize::Medium.px(&theme),
                        theme.text_secondary,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(tr!("git_panel.title")),
                    ),
            )
            .into_any_element()
    }

    /// The branch chip, ahead/behind, working-tree stats, and refresh. It
    /// lives on the branch bar under the tabs so the branch sits with the view
    /// it scopes without crowding the tab strip.
    fn branch_controls(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let branch = self
            .branch
            .clone()
            .unwrap_or_else(|| tr!("git_panel.detached"));
        let (additions, deletions) = self.stats();
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div().relative().child(
                    button_frame(div().id("git-branch-chip"), &theme, ButtonSize::Medium)
                        .border_1()
                        .border_color(theme.border)
                        .raised(
                            if self.branch_menu_open {
                                theme.surface_active
                            } else {
                                theme.surface_elevated
                            },
                            &theme,
                        )
                        .cursor_pointer()
                        .hover_state(&theme)
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.toggle_branch_menu(window, cx);
                        }))
                        .child(icon(
                            "icons/branch.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            if self.branch_menu_open {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            },
                        ))
                        .child(
                            div()
                                .max_w(px(180.))
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(branch),
                        )
                        .child(icon(
                            "icons/chevron-down.svg",
                            IconSize::XSmall.px(&theme),
                            theme.text_muted,
                        )),
                ),
            )
            .children(self.ahead_behind.and_then(|(ahead, behind)| {
                (ahead > 0 || behind > 0).then(|| {
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .text_size(TextSize::Small.px(&theme))
                        .child(
                            div()
                                .text_color(theme.text_muted)
                                .child(format!("↑{ahead}")),
                        )
                        .child(
                            div()
                                .text_color(theme.text_muted)
                                .child(format!("↓{behind}")),
                        )
                })
            }))
            .children((additions > 0 || deletions > 0).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .child(
                        div()
                            .text_color(theme.success)
                            .child(format!("+{additions}")),
                    )
                    .child(div().text_color(theme.error).child(format!("-{deletions}")))
            }))
            .child(
                icon_button_frame(div().id("git-refresh"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .cursor_pointer()
                    .hover_state(&theme)
                    .pressed_state(&theme, PressedRamp::Wash)
                    .tooltip({
                        let label = tr!("common.refresh");
                        move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.refresh_from_button(cx);
                    }))
                    .child(refresh_glyph(
                        "git-refresh-spinner",
                        ButtonSize::Default.icon_size().px(&theme),
                        self.refresh_spin_until.is_some() || self.tab_loading(),
                        theme.text_muted,
                        theme,
                    )),
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
            (
                GitTab::Stashes,
                "icons/archive.svg",
                "git_panel.tab_stashes",
            ),
        ];
        // Host tabs only appear when `gh` exists, so a repo without it keeps
        // the page exactly as it was. Sign-in state is handled inside the tab.
        if self.gh_installed {
            tabs.push((
                GitTab::Issues,
                "icons/circle-dot.svg",
                "git_panel.tab_issues",
            ));
            tabs.push((
                GitTab::Pulls,
                "icons/git-pull-request.svg",
                "git_panel.tab_pulls",
            ));
        }
        div()
            .h(DynamicSpacing::Base40.px(&theme))
            .flex_none()
            .px(DynamicSpacing::Base16.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .children(tabs.into_iter().map(|(tab, tab_icon, key)| {
                let label = tr!(key);
                let selected = self.tab == tab;
                press(button_frame(
                    div().id(gpui::ElementId::Name(
                        format!("git-tab-{}", key.rsplit('.').next().unwrap_or(key)).into(),
                    )),
                    &theme,
                    ButtonSize::Medium,
                ))
                .cursor_pointer()
                .font_weight(if selected {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .when(selected, |tab| {
                    tab.bg(theme.surface_active).text_color(theme.text_primary)
                })
                .when(!selected, |tab| {
                    tab.text_color(theme.text_muted).hover_quiet(&theme)
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if this.tab != tab {
                        this.tab = tab;
                    }
                    this.branch_menu_open = false;
                    this.branch_more_open = false;
                    this.refresh_all(cx);
                    cx.notify();
                }))
                .child(icon(
                    tab_icon,
                    ButtonSize::Medium.icon_size().px(&theme),
                    if selected {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    },
                ))
                .child(label.to_string())
            }))
            .child(div().flex_1())
            .into_any_element()
    }

    /// The branch / sync row, on its own line under the tabs so the branch
    /// context and the tab strip never crowd each other. The transient status
    /// message rides the right end of this row.
    fn branch_bar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h(DynamicSpacing::Base40.px(&theme))
            .flex_none()
            .px(DynamicSpacing::Base16.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(self.branch_controls(theme, cx))
            .child(div().flex_1())
            .children(
                self.status
                    .as_ref()
                    .filter(|(_, at)| at.elapsed() <= Duration::from_secs(6))
                    .map(|(message, _)| {
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(message.clone())
                    }),
            )
            .children(self.github_account_chip(theme))
            .child(self.branch_more_controls(theme, cx))
            .child(self.sync_controls(theme, cx))
            .into_any_element()
    }

    /// The signed-in GitHub account at the right of the branch row: a quiet
    /// avatar pill (the reference's account mark), with the login on hover.
    fn github_account_chip(&self, theme: Theme) -> Option<AnyElement> {
        let user = self.gh_user.as_ref()?;
        let login = user.login.clone();
        Some(
            icon_button_frame(div().id("git-account"), &theme, ButtonSize::Default)
                .rounded_full()
                .raised(theme.surface_elevated, &theme)
                .border_1()
                .border_color(theme.border)
                .tooltip(move |_, cx| cx.new(|_| Tooltip::new(login.clone())).into())
                .child(avatar_image(
                    user.avatar_url.clone(),
                    &user.login,
                    "",
                    theme,
                ))
                .into_any_element(),
        )
    }

    /// The Sync split button: the left half runs a full fetch/pull/push, the
    /// chevron opens the individual steps.
    fn sync_controls(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let busy = self.pending.is_some();
        // A split button: one bordered group whose halves fill its height and
        // round only its outer corners.
        div()
            .relative()
            .flex_none()
            .h(ButtonSize::Medium.height(&theme))
            .rounded(button::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.border)
            .raised(theme.surface_elevated, &theme)
            .flex()
            .items_center()
            .overflow_hidden()
            .child(
                button_frame(div().id("git-sync"), &theme, ButtonSize::Medium)
                    .h_full()
                    .rounded_r_none()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if busy {
                        theme.text_muted
                    } else {
                        theme.text_primary
                    })
                    .when(!busy, |button| {
                        button
                            .cursor_pointer()
                            .hover_state(&theme)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.sync_menu_open = false;
                                this.sync_menu_dismissed_at = None;
                                this.run_git_action(GitAction::Sync, None, cx);
                            }))
                    })
                    .child(if busy {
                        spinner(
                            "git-sync-spinner",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.accent_text,
                            theme,
                        )
                    } else {
                        icon(
                            "icons/refresh.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.text_secondary,
                        )
                        .into_any_element()
                    })
                    .child(tr!("git_panel.sync")),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(theme.border))
            .child(
                icon_button_frame(div().id("git-sync-more"), &theme, ButtonSize::Default)
                    .tip(tr!("git_panel.tip_sync_more"))
                    .h_full()
                    .rounded_l_none()
                    .cursor_pointer()
                    .when(self.sync_menu_open, |b| b.bg(theme.surface_active))
                    .hover_state(&theme)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        // The outside-click dismiss runs on mouse-down for the
                        // same press, so a click within its wake-up window is
                        // the close, not a reopen.
                        let just_dismissed = this
                            .sync_menu_dismissed_at
                            .take()
                            .is_some_and(|at| at.elapsed() < Duration::from_millis(200));
                        if !just_dismissed {
                            this.sync_menu_open = !this.sync_menu_open;
                        }
                        cx.notify();
                    }))
                    .child(icon(
                        "icons/chevron-down.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_muted,
                    )),
            )
            .children(self.sync_menu(theme, cx))
            .into_any_element()
    }

    /// The Sync dropdown: the steps the full Sync runs, exposed individually.
    fn sync_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.sync_menu_open {
            return None;
        }
        let menu = picker_surface(div().id("git-sync-popup"), &theme)
            .w(px(200.))
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.sync_menu_open = false;
                this.sync_menu_dismissed_at = Some(Instant::now());
                cx.notify();
            }))
            .child(menu_row(
                "git-sync-fetch",
                "icons/cloud.svg",
                &tr!("git_panel.fetch"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sync_menu_open = false;
                    this.fetch_remote(cx);
                    cx.notify();
                }),
            ))
            .child(menu_row(
                "git-sync-pull",
                "icons/arrow-down.svg",
                &tr!("git_panel.pull"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sync_menu_open = false;
                    this.run_git_action(GitAction::Pull, None, cx);
                }),
            ))
            .child(menu_row(
                "git-sync-push",
                "icons/upload.svg",
                &tr!("git_panel.push"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sync_menu_open = false;
                    this.run_git_action(GitAction::Push, None, cx);
                }),
            ))
            .child(menu_separator(theme))
            .child(menu_row(
                "git-sync-merge",
                "icons/git-merge.svg",
                &tr!("git_panel.merge"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sync_menu_open = false;
                    this.run_git_action(GitAction::Merge, None, cx);
                }),
            ))
            .child(menu_row(
                "git-sync-rebase",
                "icons/git-compare.svg",
                &tr!("git_panel.rebase_onto"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.sync_menu_open = false;
                    this.run_git_action(GitAction::Rebase, None, cx);
                }),
            ));
        Some(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .size(px(0.))
                .child(anchored_popover(
                    Corner::TopRight,
                    point(px(0.), px(6.)),
                    menu,
                    "anchored_popover-in-19",
                    &theme,
                ))
                .into_any_element(),
        )
    }

    /// The "…" branch-actions button beside Sync. Merge-onto / rebase-onto
    /// and rename live here so the branch picker stays a pure switcher.
    fn branch_more_controls(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .child(
                icon_button_frame(div().id("git-branch-more"), &theme, ButtonSize::Medium)
                    .group(BUTTON_GROUP)
                    .when(self.branch_more_open, |b| b.bg(theme.surface_active))
                    .cursor_pointer()
                    .hover_state(&theme)
                    .tooltip({
                        let label = tr!("git_panel.tip_more");
                        move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                    })
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.toggle_branch_more(cx);
                    }))
                    .child(icon(
                        "icons/more.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        if self.branch_more_open {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        },
                    )),
            )
            .children(self.branch_more_menu(theme, cx))
            .into_any_element()
    }

    /// The branch-actions ("…") dropdown: merge/rebase onto a ref, rename.
    fn branch_more_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.branch_more_open {
            return None;
        }
        let mut menu = picker_surface(div().id("git-branch-more-popup"), &theme)
            .w(px(ACTION_MENU_W))
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dismiss_branch_more(cx);
            }))
            .child(menu_row(
                "git-more-merge",
                "icons/git-merge.svg",
                &tr!("git_panel.merge_branch"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.branch_more_open = false;
                    this.ref_menu = Some(RefTarget::Merge);
                    cx.notify();
                }),
            ))
            .child(menu_row(
                "git-more-rebase",
                "icons/git-compare.svg",
                &tr!("git_panel.rebase_onto"),
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.branch_more_open = false;
                    this.ref_menu = Some(RefTarget::Rebase);
                    cx.notify();
                }),
            ));
        if let Some(current) = self.branch.clone() {
            menu = menu.child(menu_separator(theme)).child(menu_row(
                "git-more-rename",
                "icons/pencil.svg",
                &tr!("git_panel.rename_branch"),
                theme,
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.branch_more_open = false;
                    this.open_branch_prompt(BranchPrompt::Rename(current.clone()), window, cx)
                }),
            ));
        }
        Some(
            div()
                .absolute()
                .bottom_0()
                .right_0()
                .size(px(0.))
                .child(anchored_popover(
                    Corner::TopRight,
                    point(px(0.), px(6.)),
                    menu,
                    "anchored_popover-in-20",
                    &theme,
                ))
                .into_any_element(),
        )
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
                .mx(DynamicSpacing::Base20.px(&theme))
                .mt(DynamicSpacing::Base12.px(&theme))
                .bg(theme.error.opacity(0.1))
                .border_1()
                .border_color(theme.error.opacity(0.45))
                .rounded(panel::RADIUS.px(&theme))
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_start()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    div()
                        .flex_none()
                        .mt(DynamicSpacing::Base01.px(&theme))
                        .child(icon(
                            "icons/stop.svg",
                            IconSize::Medium.px(&theme),
                            theme.error,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base03.px(&theme))
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(failure.title.clone()),
                        )
                        .child(
                            div()
                                .id("git-failure-detail")
                                .max_h(px(160.))
                                .overflow_y_scroll()
                                .text_size(TextSize::Small.px(&theme))
                                .line_height(theme.ui_px(16.))
                                .text_color(theme.text_secondary)
                                .whitespace_normal()
                                .child(failure.detail.clone()),
                        )
                        .when(!actions.is_empty(), |column| {
                            column.child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(DynamicSpacing::Base06.px(&theme))
                                    .pt(DynamicSpacing::Base03.px(&theme))
                                    .children(
                                        actions
                                            .into_iter()
                                            .map(|action| recovery_button(action, theme, cx)),
                                    ),
                            )
                        }),
                )
                .child(
                    icon_button_frame(div().id("git-failure-copy"), &theme, ButtonSize::Compact)
                        .cursor_pointer()
                        .text_color(theme.text_secondary)
                        .hover_state(&theme)
                        .tooltip({
                            let label = tr!("git_panel.tip_copy_error");
                            move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                        })
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    copy_text.clone(),
                                ));
                            }),
                        )
                        .child(icon(
                            "icons/copy.svg",
                            ButtonSize::Compact.icon_size().px(&theme),
                            theme.text_secondary,
                        )),
                )
                .child(
                    icon_button_frame(div().id("git-failure-dismiss"), &theme, ButtonSize::Compact)
                        .cursor_pointer()
                        .text_color(theme.text_secondary)
                        .hover_state(&theme)
                        .tooltip({
                            let label = tr!("git_panel.tip_dismiss");
                            move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                        })
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.clear_failure();
                                cx.notify();
                            }),
                        )
                        .child(icon(
                            "icons/x.svg",
                            ButtonSize::Compact.icon_size().px(&theme),
                            theme.text_secondary,
                        )),
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

        let mut buttons = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .flex_none();
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
                    spinner(
                        "git-op-spinner",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.accent_text,
                        theme,
                    )
                } else {
                    icon(
                        "icons/check.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.action_fg,
                    )
                    .into_any_element()
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
                .mx(DynamicSpacing::Base20.px(&theme))
                .mt(DynamicSpacing::Base12.px(&theme))
                .bg(theme.warning.opacity(0.1))
                .border_1()
                .border_color(theme.warning.opacity(0.45))
                .rounded(panel::RADIUS.px(&theme))
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(icon(
                            "icons/git-merge.svg",
                            IconSize::Medium.px(&theme),
                            theme.warning,
                        ))
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(tr!(op.label_key())),
                        )
                        .children((conflicts > 0).then(|| {
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_secondary)
                                .child(count_label)
                        }))
                        .child(div().flex_1())
                        .child(buttons),
                )
                .children((!self.conflicts.is_empty()).then(|| {
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .children(self.conflicts.iter().take(8).map(|path| {
                            let target = path.clone();
                            button_frame(
                                div().id(gpui::ElementId::Name(
                                    format!("git-conflict-{path}").into(),
                                )),
                                &theme,
                                ButtonSize::Default,
                            )
                            .border_1()
                            .border_color(theme.border)
                            .raised(theme.surface_elevated, &theme)
                            .cursor_pointer()
                            .text_color(theme.text_secondary)
                            .hover_state(&theme)
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.open_path(target.clone(), window, cx)
                            }))
                            .child(path.clone())
                        }))
                }))
                .into_any_element(),
        )
    }

    fn changes_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        // The diff browser is the same view the Review pane shows; stashes
        // live on their own tab.
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.review.clone())
            .child(self.commit_bar(theme, cx))
            .into_any_element()
    }

    fn commit_bar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
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
            .px(DynamicSpacing::Base20.px(&theme))
            .pt(DynamicSpacing::Base12.px(&theme))
            .pb(DynamicSpacing::Base12.px(&theme))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(
                // The commit box mirrors the chat composer: the multi-line
                // message editor and its action row share one rounded, lifted
                // surface. The editor grows to six rows before scrolling, so
                // a conventional-commit body has room; Enter inserts a newline
                // and Cmd/Ctrl+Enter commits.
                div()
                    .id("git-commit-box")
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .px(DynamicSpacing::Base16.px(&theme))
                    .pt(DynamicSpacing::Base12.px(&theme))
                    .pb(DynamicSpacing::Base12.px(&theme))
                    .rounded(input::RADIUS.px(&theme))
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.surface_secondary)
                    .shadow(theme.composer_shadow())
                    .text_size(TextSize::Default.px(&theme))
                    // Clicking the box's padding focuses the editor, the way
                    // the chat composer does.
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.message.read(cx).focus_handle(cx).focus(window);
                        }),
                    )
                    .on_action(cx.listener(|this, _: &crate::GitCommitSubmit, _, cx| {
                        this.on_commit(GitAction::Commit, cx)
                    }))
                    .child(
                        // A fixed min height keeps the bar stable while the
                        // field is empty, and the editor stretches to fill it
                        // so the whole text area is clickable, not just the
                        // first line.
                        div()
                            .w_full()
                            .min_w_0()
                            .min_h(px(56.))
                            .flex()
                            .flex_col()
                            .child(self.message.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(DynamicSpacing::Base12.px(&theme))
                            .children((!self.unstaged.is_empty()).then(|| {
                                div()
                                    .id("git-include-unstaged-row")
                                    .h(ButtonSize::Medium.height(&theme))
                                    .px(DynamicSpacing::Base06.px(&theme))
                                    .ml(px(-6.))
                                    .rounded(button::RADIUS.px(&theme))
                                    .flex()
                                    .items_center()
                                    .gap(DynamicSpacing::Base08.px(&theme))
                                    .cursor_pointer()
                                    .hover_state(&theme)
                                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                        this.include_unstaged = !this.include_unstaged;
                                        cx.notify();
                                    }))
                                    .child(check_box(self.include_unstaged, theme))
                                    .child(
                                        div()
                                            .text_size(TextSize::Small.px(&theme))
                                            .text_color(theme.text_secondary)
                                            .child(tr!("git_panel.include_unstaged_changes")),
                                    )
                                    .children(self.include_unstaged.then(|| {
                                        let (additions, deletions) = self.unstaged_stats();
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(DynamicSpacing::Base08.px(&theme))
                                            .text_size(TextSize::Small.px(&theme))
                                            .child(
                                                div()
                                                    .text_color(theme.success)
                                                    .child(format!("+{additions}")),
                                            )
                                            .child(
                                                div()
                                                    .text_color(theme.error)
                                                    .child(format!("-{deletions}")),
                                            )
                                    }))
                            }))
                            .child(div().flex_1())
                            .children(has_changes.then(|| {
                                // The model chip and Generate sit left of the
                                // commit actions on the footer row, where they
                                // no longer compete with the message field for
                                // horizontal space.
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(DynamicSpacing::Base08.px(&theme))
                                    .child(self.commit_model_controls(theme, cx))
                                    .child(
                                        button_frame(
                                            div().id("git-generate"),
                                            &theme,
                                            ButtonSize::Medium,
                                        )
                                        .border_1()
                                        .border_color(theme.border)
                                        .when(self.generating, |button| button.bg(theme.overlay))
                                        .when(!self.generating, |button| {
                                            button.raised(theme.surface_elevated, &theme)
                                        })
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(if self.generating {
                                            theme.text_muted
                                        } else {
                                            theme.text_primary
                                        })
                                        .when(!self.generating, |button| {
                                            button.cursor_pointer().hover_bordered(&theme).on_click(
                                                cx.listener(|this, _: &ClickEvent, _, cx| {
                                                    this.generate(cx)
                                                }),
                                            )
                                        })
                                        .child(if self.generating {
                                            spinner(
                                                "git-generate-spinner",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                theme.accent_text,
                                                theme,
                                            )
                                        } else {
                                            icon(
                                                "icons/magic-wand.svg",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                theme.text_secondary,
                                            )
                                            .into_any_element()
                                        })
                                        .child(
                                            if self.generating {
                                                tr!("git_panel.generating_short")
                                            } else {
                                                tr!("git_panel.generate")
                                            },
                                        ),
                                    )
                            }))
                            .child(match actions {
                                BarActions::Commit => div()
                                    .flex()
                                    .items_center()
                                    .gap(DynamicSpacing::Base08.px(&theme))
                                    .child(action_button(
                                        "git-commit-push",
                                        &commit_and_push_label,
                                        Some(
                                            icon(
                                                "icons/cloud-upload.svg",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                theme.text_secondary,
                                            )
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
                                            spinner(
                                                "git-commit-spinner",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                theme.accent_text,
                                                theme,
                                            )
                                        } else {
                                            icon(
                                                "icons/git-commit.svg",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                if can_commit {
                                                    theme.action_fg
                                                } else {
                                                    theme.text_muted
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
                                            ButtonSize::Medium.icon_size().px(&theme),
                                            if busy {
                                                theme.text_muted
                                            } else {
                                                theme.action_fg
                                            },
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
                                    .gap(DynamicSpacing::Base08.px(&theme))
                                    .child(
                                        div()
                                            .text_size(TextSize::Small.px(&theme))
                                            .text_color(theme.text_muted)
                                            .child(tr!("git_panel.behind", count = behind)),
                                    )
                                    .child(action_button(
                                        "git-pull",
                                        &tr!("git_panel.pull"),
                                        Some(
                                            icon(
                                                "icons/arrow-down.svg",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                theme.text_secondary,
                                            )
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
                                    .gap(DynamicSpacing::Base08.px(&theme))
                                    .child(
                                        div()
                                            .text_size(TextSize::Small.px(&theme))
                                            .text_color(theme.text_muted)
                                            .child(tr!(
                                                "git_panel.diverged_behind",
                                                count = behind
                                            )),
                                    )
                                    .child(action_button(
                                        "git-merge",
                                        &tr!("git_panel.merge"),
                                        Some(
                                            icon(
                                                "icons/git-merge.svg",
                                                ButtonSize::Medium.icon_size().px(&theme),
                                                theme.text_secondary,
                                            )
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
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_muted)
                                    .child(tr!("git_panel.up_to_date"))
                                    .into_any_element(),
                            }),
                    ),
            )
            .into_any_element()
    }

    /// The unstaged section's aggregate `+N -M`, shown beside the commit
    /// bar's "Include unstaged changes" toggle.
    fn unstaged_stats(&self) -> (u64, u64) {
        let additions = self.unstaged.iter().map(|row| row.unstaged_additions).sum();
        let deletions = self.unstaged.iter().map(|row| row.unstaged_deletions).sum();
        (additions, deletions)
    }

    /// The Stashes tab: the stack with pop / apply / drop, plus the parking
    /// action. It lives on its own tab so the Changes tab stays the review
    /// browser plus commit bar.
    fn stashes_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let count = self.stashes.len();
        let has_changes = !self.staged.is_empty() || !self.unstaged.is_empty();
        let mut header = div()
            .id("git-stashes-header")
            .flex_none()
            .px(DynamicSpacing::Base20.px(&theme))
            .pt(DynamicSpacing::Base12.px(&theme))
            .pb(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(section_title(&tr!("git_panel.stashes"), count, theme))
            .child(div().flex_1());
        if has_changes {
            header = header.child(
                button_frame(div().id("git-stash-all"), &theme, ButtonSize::Default)
                    .border_1()
                    .border_color(theme.border)
                    .raised(theme.surface_elevated, &theme)
                    .cursor_pointer()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .hover_bordered_ink(&theme)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.stash_push(cx);
                    }))
                    .child(icon(
                        "icons/archive.svg",
                        ButtonSize::Default.icon_size().px(&theme),
                        theme.text_muted,
                    ))
                    .child(tr!("git_panel.stash_changes")),
            );
        }
        let body = if self.stashes.is_empty() {
            empty_note(
                theme,
                "icons/archive.svg",
                &tr!("git_panel.no_stashes"),
                None,
            )
        } else {
            let mut list = div()
                .flex()
                .flex_col()
                .py(DynamicSpacing::Base06.px(&theme));
            for stash in &self.stashes {
                list = list.child(stash_row(stash, theme, cx));
            }
            div()
                .id("git-stashes-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(list)
                .into_any_element()
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(body)
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
            let mut list = div()
                .flex()
                .flex_col()
                .py(DynamicSpacing::Base06.px(&theme));
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
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(filter_toggle_chip(
                "git-filter-all",
                &tr!("git_panel.all_branches"),
                Some("icons/branch.svg"),
                filter.all_branches,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_all_branches(cx)),
            ));
        if let Some(author) = filter.author.clone() {
            bar = bar.child(filter_chip(
                "git-filter-author",
                &tr!("git_panel.author_chip", name = author),
                "icons/x.svg",
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
                "icons/x.svg",
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
                    "icons/x.svg",
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
            .mx(DynamicSpacing::Base12.px(&theme))
            .mb(DynamicSpacing::Base06.px(&theme))
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .rounded(panel::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.surface_elevated)
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme));
        if !detail.body.is_empty() {
            column = column.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .line_height(theme.ui_px(18.))
                    .text_color(theme.text_secondary)
                    .whitespace_normal()
                    .child(detail.body.clone()),
            );
        }
        let hash = detail.short.clone();
        let mut meta = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text_muted)
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
                "git_panel.open_on_github",
                theme,
                move |_, _, cx| cx.open_url(&url),
            ));
        }
        meta = meta.child(row_button(
            "git-detail-copy-hash",
            "icons/copy.svg",
            "git_panel.tip_copy_hash",
            theme,
            move |_, _, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string(hash.clone())),
        ));
        column = column.child(meta);

        let glyph_size = IconSize::XSmall.px(&theme);
        for file in detail.files.iter().take(MAX_FILE_ROWS) {
            let path = file.path.clone();
            let fallback = icon("icons/file.svg", glyph_size, theme.text_muted).into_any_element();
            let glyph = crate::app::file_glyph(&path, dark, nerd.as_ref(), glyph_size, fallback);
            let color = status_color(file.status, theme);
            let hash = detail.hash.clone();
            column = column.child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .h(px(26.))
                    .child(letter_tile(file.status, color, theme))
                    .child(glyph)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_primary)
                            .child(path.clone()),
                    )
                    .child(delta_stats(file.additions, file.deletions, theme))
                    .child(row_button(
                        format!("git-detail-more-{path}"),
                        "icons/more.svg",
                        "git_panel.tip_more",
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
        if detail.files.len() > MAX_FILE_ROWS {
            column = column.child(detail_note(
                theme,
                &tr!(
                    "git_panel.more_files",
                    count = detail.files.len() - MAX_FILE_ROWS
                ),
            ));
        }
        column.into_any_element()
    }

    /// The per-file actions menu: open, reveal, copy path, file history.
    fn file_actions_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (path, _hash) = self.file_menu.clone()?;
        let menu = picker_surface(div().id("git-file-menu"), &theme)
            .absolute()
            .top(px(42.))
            .right(px(12.))
            .w(px(ACTION_MENU_W))
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
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
            let mut list = div()
                .flex()
                .flex_col()
                .py(DynamicSpacing::Base06.px(&theme));
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
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(filter_toggle_chip(
                        "git-graph-all-refs",
                        &tr!("git_panel.all_refs"),
                        Some("icons/branch.svg"),
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

    // ── issues rendering ───────────────────────────────────────────────

    /// The GitHub list/detail split at the width the divider last left it.
    fn github_split(
        &self,
        list: AnyElement,
        detail: Option<AnyElement>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = github_list_pane_width(self.page_width, self.github_list_width);
        // The pointer leaves the 7px grab strip as soon as the drag starts, so
        // the split itself tracks the move and the release. It is an ancestor
        // of the handle, which means the hit test always includes it.
        div()
            .flex_1()
            .h_full()
            .min_h_0()
            .min_w_0()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button != Some(MouseButton::Left) {
                    // A move without the button proves the gesture ended even
                    // if the release was delivered elsewhere.
                    this.github_list_drag = None;
                    return;
                }
                let Some((origin_width, origin_x)) = this
                    .github_list_drag
                    .as_ref()
                    .map(|drag| (drag.origin_width, drag.origin_x))
                else {
                    return;
                };
                // Only a change on screen becomes the new preference. When the
                // detail reserve holds the list at its ceiling (or the floor),
                // pushing against that edge leaves the stored width alone
                // instead of replacing it with a page-clamped value.
                let Some(width) = dragged_github_list_pane_width(
                    this.page_width,
                    this.github_list_width,
                    origin_width,
                    origin_x,
                    event.position.x,
                ) else {
                    return;
                };
                this.github_list_width = width;
                if let Some(drag) = this.github_list_drag.as_mut() {
                    drag.moved = true;
                }
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_github_list_resize(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_github_list_resize(cx)),
            )
            .child(github_split_view(
                list,
                detail,
                width,
                self.github_list_handle(theme, cx),
            ))
            .into_any_element()
    }

    /// The divider between the GitHub list and its detail: a 7px grab strip
    /// with the seam drawn inside it, replacing the static border the list used
    /// to carry. Dragging it resizes the list; the width is persisted when the
    /// drag ends, so one drag costs one write.
    fn github_list_handle(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("git-github-list-resize")
            .debug_selector(|| "github-split-handle".into())
            .group("github-list-resize")
            .flex_none()
            .w(px(GITHUB_LIST_HANDLE_W))
            .h_full()
            .flex()
            .justify_center()
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, _| {
                    // Do not stop propagation: the drag's pending-mouse-down
                    // listener is registered after this one, and swallowing the
                    // event here would leave the divider unable to drag.
                    let width = github_list_pane_width(this.page_width, this.github_list_width);
                    this.github_list_drag = Some(GithubListDrag {
                        origin_width: width,
                        origin_x: event.position.x,
                        moved: false,
                    });
                }),
            )
            .border_l_1()
            .border_color(theme.border)
            .group_hover("github-list-resize", |style| {
                style.border_color(theme.border_strong)
            })
            .into_any_element()
    }

    /// End a list-divider drag: persist the width it settled on, but only when
    /// the drag actually moved it and the result is inside the draggable range.
    fn finish_github_list_resize(&mut self, cx: &mut Context<Self>) {
        let drag = self.github_list_drag.take();
        if let Some(width) = github_list_width_to_persist(drag.as_ref(), self.github_list_width) {
            crate::layout::set_github_list_width(width);
            cx.notify();
        }
    }

    /// The Issues tab: the real browser once `gh` is installed and signed in,
    /// otherwise the honest setup state.
    fn issues_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        if !self.gh_installed || !self.gh_authenticated {
            return self.gh_setup_tab(
                theme,
                "git_panel.gh_issues_title",
                "git_panel.gh_issues_body",
            );
        }
        if self.issue_new_open {
            return self.issue_new_view(theme, cx);
        }
        if split_github_view(self.page_width) {
            let list = self.issues_list_view(theme, cx);
            let detail = if let Some(issue) = &self.issue_detail {
                Some(self.issue_detail_view(issue, theme, cx))
            } else if self.issue_detail_loading {
                Some(empty_note(
                    theme,
                    "icons/circle-dot.svg",
                    &tr!("git_panel.reading_issues"),
                    None,
                ))
            } else {
                Some(empty_note(
                    theme,
                    "icons/circle-dot.svg",
                    &tr!("git_panel.select_issue"),
                    None,
                ))
            };
            return self.github_split(list, detail, theme, cx);
        }
        if let Some(issue) = &self.issue_detail {
            return self.issue_detail_view(issue, theme, cx);
        }
        self.issues_list_view(theme, cx)
    }

    fn issue_filter_bar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let filter = &self.issue_filter;
        let state_id = |state: gh::IssueState| match state {
            gh::IssueState::Open => "git-issues-open",
            gh::IssueState::Closed => "git-issues-closed",
            gh::IssueState::All => "git-issues-all",
        };
        let label_label = match filter.label.as_deref() {
            Some(label) => tr!("git_panel.label_chip", name = label),
            None => tr!("git_panel.label_filter"),
        };
        let states = div()
            .debug_selector(|| "github-filter-states".into())
            .px(DynamicSpacing::Base08.px(&theme))
            .pt(DynamicSpacing::Base08.px(&theme))
            .pb(DynamicSpacing::Base04.px(&theme))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base02.px(&theme))
            .children(
                [
                    (gh::IssueState::Open, "git_panel.issue_open"),
                    (gh::IssueState::Closed, "git_panel.issue_closed"),
                    (gh::IssueState::All, "git_panel.issue_all"),
                ]
                .into_iter()
                .map(|(state, key)| {
                    github_state_tab(
                        state_id(state),
                        &tr!(key),
                        filter.state == state,
                        theme,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_issue_state(state, cx)
                        }),
                    )
                }),
            );
        let compact = compact_github_tools(self.page_width, self.github_list_width);
        let label_trigger = github_label_filter_button(
            "git-issue-label-filter",
            &label_label,
            self.label_menu == Some(LabelMenuTarget::Filter),
            filter.label.is_some(),
            compact,
            theme,
            cx.listener(|this, _: &ClickEvent, _, cx| {
                this.toggle_label_menu(LabelMenuTarget::Filter, cx);
            }),
        );
        let tools = div()
            .debug_selector(|| "github-filter-tools".into())
            .px(DynamicSpacing::Base08.px(&theme))
            .pb(DynamicSpacing::Base08.px(&theme))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(search_field(
                theme,
                self.issue_search.clone(),
                "git-issue-search",
                "git_panel.search",
                cx.listener(|this, _: &ClickEvent, _, cx| this.apply_issue_search(cx)),
            ))
            .child(self.label_selector(theme, label_trigger, LabelMenuTarget::Filter, cx))
            .children(filter.is_active().then(|| {
                row_button(
                    "git-issue-clear",
                    "icons/x.svg",
                    "git_panel.clear_filter",
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.clear_issue_filter(cx)),
                )
            }))
            .child(github_create_button(
                "git-issue-new",
                &tr!("git_panel.new_issue"),
                compact,
                self.issue_busy,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.open_issue_new(cx)),
            ));
        div()
            .id("git-issue-filters")
            .debug_selector(|| "github-filter-bar".into())
            .flex_none()
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(states)
            .child(tools)
            .into_any_element()
    }

    fn issues_list_view(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(error) = &self.issues_error {
            body = body.child(empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.issues_unavailable"),
                Some(error),
            ));
        } else if self.issues.is_empty() && self.issues_loading {
            body = body.child(empty_note(
                theme,
                "icons/circle-dot.svg",
                &tr!("git_panel.reading_issues"),
                None,
            ));
        } else if self.issues.is_empty() {
            body = body.child(empty_note(
                theme,
                "icons/circle-dot.svg",
                &tr!("git_panel.no_issues"),
                Some(&tr!("git_panel.no_issues_detail")),
            ));
        } else {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base02.px(&theme))
                .p(DynamicSpacing::Base06.px(&theme));
            for issue in &self.issues {
                list = list.child(issue_row(
                    issue,
                    self.selected_issue == Some(issue.number),
                    theme,
                    cx,
                ));
            }
            body = body.child(
                div()
                    .id("git-issue-scroll")
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
            .child(self.issue_filter_bar(theme, cx))
            .children(
                (self.issue_detail_loading && !split_github_view(self.page_width)).then(|| {
                    empty_note(
                        theme,
                        "icons/circle-dot.svg",
                        &tr!("git_panel.reading_issues"),
                        None,
                    )
                }),
            )
            .child(body)
            .into_any_element()
    }

    fn issue_detail_view(
        &self,
        issue: &gh::GhIssue,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state_color = if issue.is_open() {
            theme.success
        } else {
            theme.error
        };
        let state_label = if issue.is_open() {
            tr!("git_panel.issue_open")
        } else {
            tr!("git_panel.issue_closed")
        };
        let state_glyph = if issue.is_open() {
            "icons/circle-dot.svg"
        } else {
            "icons/circle-check.svg"
        };

        let comments = if issue.comments.len() == 1 {
            tr!("git_panel.issue_comment_one")
        } else {
            tr!("git_panel.issue_comments", count = issue.comments.len())
        };
        let mut meta = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text_muted)
            .child(author_avatar(&issue.author.login, "", theme))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(issue.author.login.clone()),
            )
            .child("·")
            .child(format!(
                "{} {}",
                tr!("git_panel.opened"),
                gh::relative_time(&issue.created_at)
            ))
            .child("·")
            .child(format!(
                "{} {}",
                tr!("git_panel.updated"),
                gh::relative_time(&issue.updated_at)
            ))
            .child("·")
            .child(comments);
        if !issue.assignees.is_empty() {
            meta = meta.child("·").children(issue.assignees.iter().map(|user| {
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .child(author_avatar(&user.login, "", theme))
                    .child(user.login.clone())
                    .into_any_element()
            }));
        }
        let label_editor = {
            let trigger = action_button(
                "git-issue-labels",
                &tr!("git_panel.edit_labels"),
                Some(
                    icon(
                        "icons/tag-01.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.text_secondary,
                    )
                    .into_any_element(),
                ),
                false,
                self.issue_busy,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.toggle_label_menu(LabelMenuTarget::Issue, cx);
                }),
            );
            self.label_selector(theme, trigger, LabelMenuTarget::Issue, cx)
        };
        let actions = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(label_editor)
            .child(
                div()
                    .debug_selector(|| "github-issue-agent-action".into())
                    .child({
                        let url = issue.url.clone();
                        action_button(
                            "git-issue-agent",
                            &tr!("git_panel.fix_with_agent"),
                            Some(
                                icon(
                                    "icons/spark.svg",
                                    ButtonSize::Medium.icon_size().px(&theme),
                                    theme.accent_text,
                                )
                                .into_any_element(),
                            ),
                            false,
                            url.is_empty() || self.on_start_github_task.is_none(),
                            theme,
                            cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.start_github_task(&url, false, window, cx);
                            }),
                        )
                    }),
            )
            .children((!issue.url.is_empty()).then(|| {
                let url = issue.url.clone();
                action_button(
                    "git-issue-web",
                    &tr!("git_panel.open_on_github"),
                    Some(
                        icon(
                            "icons/arrow-up-right.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.text_secondary,
                        )
                        .into_any_element(),
                    ),
                    false,
                    false,
                    theme,
                    cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)),
                )
            }));
        let header = github_detail_header(
            row_button(
                "git-issue-back",
                "icons/arrow-left.svg",
                "git_panel.all_issues",
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_issue_detail(cx)),
            ),
            &issue.title,
            theme,
        );
        let mut details = github_detail_details(theme)
            .child(github_detail_eyebrow(
                &tr!("git_panel.issue_kind"),
                &issue.url,
                issue.number,
                &state_label,
                state_glyph,
                state_color,
                theme,
            ))
            .child(meta)
            .child(actions);
        if !issue.labels.is_empty() {
            details = details.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .children(
                        issue
                            .labels
                            .iter()
                            .map(|label| issue_label_chip(label, theme)),
                    ),
            );
        }

        // Metadata and the document share one scroll surface below the fixed header.
        let mut main = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base16.px(&theme));
        if issue.body.trim().is_empty() {
            main = main.child(github_empty_description(theme));
        } else {
            main = main.child(
                div()
                    .debug_selector(|| "github-description".into())
                    .w_full()
                    .min_w_0()
                    .child(crate::transcript_view::render_markdown_document(
                        &issue.body,
                        theme,
                    )),
            );
        }
        for comment in &issue.comments {
            main = main.child(issue_comment_card(comment, theme));
        }
        main = main.child(composer_field(theme, self.issue_comment.clone(), px(88.)));
        // State toggle and submit share the footer row — close/reopen on the
        // left, comment on the right, the way GitHub groups them.
        main = main.child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(action_button(
                    "git-issue-state",
                    &if issue.is_open() {
                        tr!("git_panel.close_issue")
                    } else {
                        tr!("git_panel.reopen_issue")
                    },
                    Some(
                        icon(
                            if issue.is_open() {
                                "icons/x.svg"
                            } else {
                                "icons/refresh.svg"
                            },
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.text_secondary,
                        )
                        .into_any_element(),
                    ),
                    false,
                    self.issue_busy,
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_issue_state(cx)),
                ))
                .child(div().flex_1())
                .child(action_button(
                    "git-issue-comment",
                    &tr!("git_panel.comment"),
                    Some(
                        icon(
                            "icons/chat.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.action_fg,
                        )
                        .into_any_element(),
                    ),
                    true,
                    self.issue_busy,
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.comment_issue(cx)),
                )),
        );

        github_detail_layout("git-issue-detail-scroll", header, details, main, theme)
            .into_any_element()
    }

    fn issue_new_view(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let generate_leading = if self.issue_generating {
            spinner(
                "git-issue-generate-spinner",
                ButtonSize::Medium.icon_size().px(&theme),
                theme.accent_text,
                theme,
            )
        } else {
            icon(
                "icons/spark.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                theme.accent_text,
            )
            .into_any_element()
        };
        let generate_label = if self.issue_generating {
            tr!("git_panel.generating_draft")
        } else {
            tr!("git_panel.generate_draft")
        };
        let card = div()
            .debug_selector(|| "github-new-form".into())
            .w_full()
            .min_w_0()
            .max_w(px(820.))
            .relative()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(tr!("git_panel.new_issue_title")),
                    )
                    .child(action_button(
                        "git-issue-new-generate",
                        &generate_label,
                        Some(generate_leading),
                        false,
                        self.issue_busy || self.issue_generating,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.generate_issue_draft(cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!("git_panel.template")),
                    )
                    .child(self.template_selector(
                        theme,
                        DraftForm::Issue,
                        "git-issue-template",
                        cx,
                    )),
            )
            .child(composer_field(theme, self.issue_new_hint.clone(), px(0.)))
            .child(composer_field(theme, self.issue_new_title.clone(), px(0.)))
            .child(composer_field(theme, self.issue_new_body.clone(), px(220.)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(action_button(
                        "git-issue-new-cancel",
                        &tr!("git_panel.cancel"),
                        None,
                        false,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.issue_new_open = false;
                            cx.notify();
                        }),
                    ))
                    .child(action_button(
                        "git-issue-new-create",
                        &tr!("git_panel.create_issue"),
                        None,
                        true,
                        self.issue_busy,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.create_issue(cx)),
                    )),
            );
        div()
            .id("git-issue-new-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base20.px(&theme))
                    .child(card),
            )
            .into_any_element()
    }

    /// Anchor the picker to its trigger and paint it above the page.
    fn label_selector(
        &self,
        theme: Theme,
        trigger: AnyElement,
        target: LabelMenuTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let popup = if self.label_menu == Some(target) {
            self.label_popup(theme, target, cx)
        } else {
            None
        };
        div()
            .relative()
            .flex()
            .child(trigger)
            .children(popup)
            .into_any_element()
    }

    /// The repository-label picker acts on its trigger's explicit target.
    fn label_popup(
        &self,
        theme: Theme,
        target: LabelMenuTarget,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.label_menu != Some(target) {
            return None;
        }
        let align_right = target == LabelMenuTarget::Filter;
        let detail = self.issue_detail.clone();
        let mut list = div()
            .id("git-label-scroll")
            .w_full()
            .max_h(px(320.))
            .overflow_y_scroll()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();
        if self.issue_labels.is_empty() {
            list = list.child(
                picker_entry(div(), &theme)
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.no_labels")),
            );
        }
        for label in &self.issue_labels {
            let name = label.name.clone();
            let selected = match target {
                LabelMenuTarget::Issue => detail
                    .as_ref()
                    .is_some_and(|issue| issue.labels.iter().any(|entry| entry.name == name)),
                LabelMenuTarget::Filter => {
                    self.issue_filter.label.as_deref() == Some(name.as_str())
                }
            };
            let id = gpui::ElementId::Name(format!("git-label-{name}").into());
            list = list.child(
                picker_entry(div().id(id), &theme)
                    .cursor_pointer()
                    .text_color(theme.text_secondary)
                    .hover_state(&theme)
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| match target {
                            LabelMenuTarget::Issue => this.toggle_issue_label(name.clone(), cx),
                            LabelMenuTarget::Filter => {
                                this.set_issue_label_filter(Some(name.clone()), cx)
                            }
                        }),
                    )
                    .child(issue_label_chip(label, theme))
                    .child(div().flex_1())
                    .when(selected, |row| {
                        row.child(icon(
                            "icons/check.svg",
                            context_menu::ICON.px(&theme),
                            theme.accent_text,
                        ))
                    }),
            );
        }
        let menu = picker_surface(div().id("git-label-menu"), &theme)
            .w(px(260.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.label_menu = None;
                // Suppress only the dismissed trigger's subsequent mouse-up.
                this.label_menu_dismissed = Some((target, Instant::now()));
                cx.notify();
            }))
            .child(menu_header(tr!("git_panel.labels"), &theme))
            .child(list);
        let wrapper = if align_right {
            div().absolute().right(px(0.)).top(px(30.))
        } else {
            div().absolute().left(px(0.)).top(px(30.))
        };
        Some(
            wrapper
                .child(anchored_popover(
                    if align_right {
                        Corner::TopRight
                    } else {
                        Corner::TopLeft
                    },
                    point(px(0.), px(0.)),
                    menu,
                    "git-label-popover",
                    &theme,
                ))
                .into_any_element(),
        )
    }

    // ── pull request rendering ─────────────────────────────────────────

    /// The selected PR's best available data. A list row supplies title,
    /// author, branches, review/check state and diff totals immediately; the
    /// detail response replaces it with the complete document and timeline.
    fn selected_pull(&self) -> Option<&gh::GhPull> {
        self.pr_detail.as_ref().or_else(|| {
            let number = self.selected_pr?;
            self.pulls.iter().find(|pull| pull.number == number)
        })
    }

    fn pulls_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        if !self.gh_installed || !self.gh_authenticated {
            return self.gh_setup_tab(theme, "git_panel.gh_pulls_title", "git_panel.gh_pulls_body");
        }
        if self.pr_new_open {
            return self.pr_new_view(theme, cx);
        }
        if split_github_view(self.page_width) {
            let list = self.pulls_list_view(theme, cx);
            let detail = if let Some(pull) = self.selected_pull() {
                Some(self.pr_detail_view(pull, theme, cx))
            } else if self.pr_detail_loading {
                Some(empty_note(
                    theme,
                    "icons/git-pull-request.svg",
                    &tr!("git_panel.reading_pulls"),
                    None,
                ))
            } else {
                Some(empty_note(
                    theme,
                    "icons/git-pull-request.svg",
                    &tr!("git_panel.select_pull"),
                    None,
                ))
            };
            return self.github_split(list, detail, theme, cx);
        }
        if let Some(pull) = self.selected_pull() {
            return self.pr_detail_view(pull, theme, cx);
        }
        self.pulls_list_view(theme, cx)
    }

    fn pr_filter_bar(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let filter = &self.pr_filter;
        let state_id = |state: gh::PrState| match state {
            gh::PrState::Open => "git-prs-open",
            gh::PrState::Closed => "git-prs-closed",
            gh::PrState::Merged => "git-prs-merged",
            gh::PrState::All => "git-prs-all",
        };
        let states = div()
            .debug_selector(|| "github-filter-states".into())
            .px(DynamicSpacing::Base08.px(&theme))
            .pt(DynamicSpacing::Base08.px(&theme))
            .pb(DynamicSpacing::Base04.px(&theme))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base02.px(&theme))
            .children(
                [
                    (gh::PrState::Open, "git_panel.issue_open"),
                    (gh::PrState::Closed, "git_panel.issue_closed"),
                    (gh::PrState::Merged, "git_panel.pr_merged"),
                    (gh::PrState::All, "git_panel.issue_all"),
                ]
                .into_iter()
                .map(|(state, key)| {
                    github_state_tab(
                        state_id(state),
                        &tr!(key),
                        filter.state == state,
                        theme,
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.set_pr_state(state, cx)
                        }),
                    )
                }),
            );
        let compact = compact_github_tools(self.page_width, self.github_list_width);
        let tools = div()
            .debug_selector(|| "github-filter-tools".into())
            .px(DynamicSpacing::Base08.px(&theme))
            .pb(DynamicSpacing::Base08.px(&theme))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(search_field(
                theme,
                self.pr_search.clone(),
                "git-pr-search",
                "git_panel.search",
                cx.listener(|this: &mut GitPanel, _: &ClickEvent, _, cx| this.apply_pr_search(cx)),
            ))
            .children(filter.is_active().then(|| {
                row_button(
                    "git-pr-clear",
                    "icons/x.svg",
                    "git_panel.clear_filter",
                    theme,
                    cx.listener(|this: &mut GitPanel, _: &ClickEvent, _, cx| {
                        this.clear_pr_filter(cx)
                    }),
                )
            }))
            .child(github_create_button(
                "git-pr-new",
                &tr!("git_panel.new_pull"),
                compact,
                self.pr_busy,
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.open_pull_new(cx)),
            ));
        div()
            .id("git-pr-filters")
            .debug_selector(|| "github-filter-bar".into())
            .flex_none()
            .border_b_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(states)
            .child(tools)
            .into_any_element()
    }

    fn pulls_list_view(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div().flex_1().min_h_0().flex().flex_col();
        if let Some(error) = &self.pulls_error {
            body = body.child(empty_note(
                theme,
                "icons/stop.svg",
                &tr!("git_panel.pulls_unavailable"),
                Some(error),
            ));
        } else if self.pulls.is_empty() && self.pulls_loading {
            body = body.child(empty_note(
                theme,
                "icons/git-pull-request.svg",
                &tr!("git_panel.reading_pulls"),
                None,
            ));
        } else if self.pulls.is_empty() {
            body = body.child(empty_note(
                theme,
                "icons/git-pull-request.svg",
                &tr!("git_panel.no_pulls"),
                Some(&tr!("git_panel.no_pulls_detail")),
            ));
        } else {
            let mut list = div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base02.px(&theme))
                .p(DynamicSpacing::Base06.px(&theme));
            for pull in &self.pulls {
                list = list.child(pr_row(
                    pull,
                    self.selected_pr == Some(pull.number),
                    theme,
                    cx,
                ));
            }
            body = body.child(
                div()
                    .id("git-pr-scroll")
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
            .child(self.pr_filter_bar(theme, cx))
            .children(
                (self.pr_detail_loading && !split_github_view(self.page_width)).then(|| {
                    empty_note(
                        theme,
                        "icons/git-pull-request.svg",
                        &tr!("git_panel.reading_pulls"),
                        None,
                    )
                }),
            )
            .child(body)
            .into_any_element()
    }

    fn pr_detail_view(
        &self,
        pull: &gh::GhPull,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state_color = if pull.is_merged() {
            theme.accent_text
        } else if pull.is_open() {
            theme.success
        } else {
            theme.error
        };
        let state_label = if pull.is_merged() {
            tr!("git_panel.pr_merged")
        } else if pull.is_open() {
            tr!("git_panel.issue_open")
        } else {
            tr!("git_panel.issue_closed")
        };
        let state_glyph = if pull.is_merged() {
            "icons/git-merge.svg"
        } else if pull.is_open() {
            "icons/git-pull-request.svg"
        } else {
            "icons/circle-x.svg"
        };
        let meta = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text_muted)
            .child(author_avatar(&pull.author.login, "", theme))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(pull.author.login.clone()),
            )
            .child("·")
            .child(format!(
                "{} {}",
                tr!("git_panel.opened"),
                gh::relative_time(&pull.created_at)
            ))
            .child("·")
            .child(format!(
                "{} {}",
                tr!("git_panel.updated"),
                gh::relative_time(&pull.updated_at)
            ));
        let branches = div()
            .debug_selector(|| "github-pr-branches".into())
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .text_size(TextSize::Small.px(&theme))
            .font_family(theme::code_font_family())
            .text_color(theme.text_secondary)
            .child(icon(
                "icons/git-compare.svg",
                IconSize::XSmall.px(&theme),
                theme.text_muted,
            ))
            .child(pull.base_ref.clone())
            .child(div().text_color(theme.text_muted).child("←"))
            .child(pull.head_ref.clone())
            .children(pull.is_draft.then(|| {
                state_chip(
                    &tr!("git_panel.pr_draft"),
                    theme.text_muted,
                    Some("icons/pencil.svg"),
                    theme,
                )
            }));
        let actions = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(
                div()
                    .debug_selector(|| "github-pr-agent-action".into())
                    .child({
                        let url = pull.url.clone();
                        action_button(
                            "git-pr-agent",
                            &tr!("git_panel.review_with_agent"),
                            Some(
                                icon(
                                    "icons/spark.svg",
                                    ButtonSize::Medium.icon_size().px(&theme),
                                    theme.accent_text,
                                )
                                .into_any_element(),
                            ),
                            false,
                            url.is_empty() || self.on_start_github_task.is_none(),
                            theme,
                            cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.start_github_task(&url, true, window, cx);
                            }),
                        )
                    }),
            )
            .children((!pull.url.is_empty()).then(|| {
                let url = pull.url.clone();
                action_button(
                    "git-pr-web",
                    &tr!("git_panel.open_on_github"),
                    Some(
                        icon(
                            "icons/arrow-up-right.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.text_secondary,
                        )
                        .into_any_element(),
                    ),
                    false,
                    false,
                    theme,
                    cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url)),
                )
            }));
        let summary = div()
            .debug_selector(|| "github-pr-summary".into())
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .flex_wrap()
            .gap(DynamicSpacing::Base04.px(&theme))
            .children(review_chip(pull, theme))
            .children(check_bucket_chip(pull.checks_summary(), theme))
            .children(merge_state_summary(pull, theme))
            .child(delta_stats(pull.additions, pull.deletions, theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.files_count", count = pull.changed_files)),
            );
        let state_label = if pull.is_draft && pull.is_open() {
            tr!("git_panel.pr_draft")
        } else {
            state_label
        };
        let header = github_detail_header(
            row_button(
                "git-pr-back",
                "icons/arrow-left.svg",
                "git_panel.all_pulls",
                theme,
                cx.listener(|this, _: &ClickEvent, _, cx| this.close_pull_detail(cx)),
            ),
            &pull.title,
            theme,
        );
        let details = github_detail_details(theme)
            .child(github_detail_eyebrow(
                &tr!("git_panel.pull_request_kind"),
                &pull.url,
                pull.number,
                &state_label,
                state_glyph,
                state_color,
                theme,
            ))
            .child(meta)
            .child(branches)
            .child(summary)
            .child(actions);
        let mut content = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base20.px(&theme));
        if self.pr_detail_loading {
            content = content.child(
                div()
                    .debug_selector(|| "github-detail-loading".into())
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.reading_pulls")),
            );
        }
        if let Some(error) = &self.pr_detail_error {
            content = content.child(
                div()
                    .debug_selector(|| "github-detail-error".into())
                    .w_full()
                    .min_w_0()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .rounded(panel::RADIUS.px(&theme))
                    .border_1()
                    .border_color(theme.warning)
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.warning)
                    .whitespace_normal()
                    .child(format!("{}: {error}", tr!("git_panel.pulls_unavailable"))),
            );
        }
        if !pull.labels.is_empty() {
            content = content.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .children(
                        pull.labels
                            .iter()
                            .map(|label| issue_label_chip(label, theme)),
                    ),
            );
        }
        // A list-only summary does not know whether the description is empty.
        // Wait for the detail response instead of flashing "No description".
        if !self.pr_detail_loading && self.pr_detail_error.is_none() {
            if pull.body.trim().is_empty() {
                content = content.child(github_empty_description(theme));
            } else {
                content = content.child(
                    div()
                        .debug_selector(|| "github-description".into())
                        .w_full()
                        .min_w_0()
                        .child(crate::transcript_view::render_markdown_document(
                            &pull.body, theme,
                        )),
                );
            }
        }

        // Checks.
        if !pull.checks.is_empty() {
            let mut checks = div()
                .debug_selector(|| "github-checks".into())
                .rounded(panel::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .child(section_label(theme, &tr!("git_panel.checks")));
            for check in &pull.checks {
                let label = check.label().to_string();
                let link = check.link().to_string();
                let (status, status_color, status_glyph) =
                    check_status_parts(check.bucket_kind(), theme);
                let mut row = div()
                    .id(gpui::ElementId::Name(format!("git-check-{label}").into()))
                    .group("git-row")
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base06.px(&theme))
                    .rounded(Radius::Medium.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .hover_state(&theme)
                    .child(icon(
                        status_glyph,
                        IconSize::XSmall.px(&theme),
                        status_color,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(label.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(status_color)
                            .child(status),
                    );
                if !link.is_empty() {
                    row = row.child(div().flex_none().child(row_button(
                        "git-check-open",
                        "icons/arrow-up-right.svg",
                        "git_panel.open_on_github",
                        theme,
                        move |_, _, cx| cx.open_url(&link),
                    )));
                }
                checks = checks.child(row);
            }
            content = content.child(checks);
        }

        // Commits.
        if !pull.commits.is_empty() {
            let mut commits = div()
                .debug_selector(|| "github-commits".into())
                .rounded(panel::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .child(section_label(theme, &tr!("git_panel.commits")));
            for commit in &pull.commits {
                commits = commits.child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .py(DynamicSpacing::Base04.px(&theme))
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(
                            div()
                                .font_family(theme::code_font_family())
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(commit.oid.chars().take(7).collect::<String>()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_secondary)
                                .child(commit.headline.clone()),
                        ),
                );
            }
            content = content.child(commits);
        }

        // Changed files.
        if !pull.files.is_empty() {
            let mut files = div()
                .debug_selector(|| "github-files".into())
                .rounded(panel::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .child(section_label(theme, &tr!("git_panel.files_changed")));
            let nerd = nerd_font_family(cx);
            let dark = theme.mode == ThemeMode::Dark;
            let glyph_size = IconSize::XSmall.px(&theme);
            for file in pull.files.iter().take(MAX_FILE_ROWS) {
                let path = file.path.clone();
                let status = file.change_type.chars().next().unwrap_or('M');
                let (directory, file_name) = path
                    .rsplit_once('/')
                    .map(|(directory, name)| (format!("{directory}/"), name))
                    .unwrap_or_else(|| (String::new(), path.as_str()));
                let fallback =
                    icon("icons/file.svg", glyph_size, theme.text_muted).into_any_element();
                let glyph =
                    crate::app::file_glyph(&path, dark, nerd.as_ref(), glyph_size, fallback);
                files = files.child(
                    div()
                        .group("git-row")
                        .px(DynamicSpacing::Base12.px(&theme))
                        .py(DynamicSpacing::Base04.px(&theme))
                        .rounded(Radius::Medium.px(&theme))
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .hover_state(&theme)
                        .child(letter_tile(status, status_color(status, theme), theme))
                        .child(glyph)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(TextSize::Small.px(&theme))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_color(theme.text_muted)
                                        .child(directory),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .max_w(px(200.))
                                        .truncate()
                                        .text_color(theme.text_primary)
                                        .child(file_name.to_string()),
                                ),
                        )
                        .child(delta_stats(file.additions, file.deletions, theme))
                        .child(div().flex_none().child(row_button(
                            format!("git-pr-file-{path}"),
                            "icons/git-compare.svg",
                            "git_panel.open_diff",
                            theme,
                            cx.listener({
                                let path = path.clone();
                                move |this, _: &ClickEvent, window, cx| {
                                    if let Some(on_open) = this.on_open_file.clone() {
                                        on_open(path.clone(), window, cx);
                                    }
                                }
                            }),
                        ))),
                );
            }
            if pull.files.len() > MAX_FILE_ROWS {
                files = files.child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .py(DynamicSpacing::Base06.px(&theme))
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!(
                            "git_panel.more_files",
                            count = pull.files.len() - MAX_FILE_ROWS
                        )),
                );
            }
            content = content.child(files);
        }

        for comment in &pull.comments {
            content = content.child(issue_comment_card(comment, theme));
        }
        for review in &pull.reviews {
            content = content.child(review_card(review, theme));
        }

        // Merge controls come before the composer: the primary action of the
        // page sits in a card above the comment field, GitHub-style.
        if pull.is_open() {
            let merge_row = div()
                .debug_selector(|| "github-merge-controls".into())
                .rounded(panel::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.border)
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_center()
                .flex_wrap()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(DynamicSpacing::Base04.px(&theme))
                        .children(
                            [
                                (gh::MergeMethod::Merge, "git_panel.merge_method_merge"),
                                (gh::MergeMethod::Squash, "git_panel.merge_method_squash"),
                                (gh::MergeMethod::Rebase, "git_panel.merge_method_rebase"),
                            ]
                            .into_iter()
                            .map(|(method, key)| {
                                github_state_tab(
                                    match method {
                                        gh::MergeMethod::Merge => "git-merge-method-merge",
                                        gh::MergeMethod::Squash => "git-merge-method-squash",
                                        gh::MergeMethod::Rebase => "git-merge-method-rebase",
                                    },
                                    &tr!(key),
                                    self.pr_merge_method == method,
                                    theme,
                                    cx.listener(move |this, _: &ClickEvent, _, cx| {
                                        this.pr_merge_method = method;
                                        cx.notify();
                                    }),
                                )
                            }),
                        ),
                )
                .child(github_check_toggle(
                    "git-pr-delete-branch",
                    &tr!("git_panel.delete_branch_after"),
                    self.pr_delete_branch,
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.pr_delete_branch = !this.pr_delete_branch;
                        cx.notify();
                    }),
                ))
                .child(
                    div().w_full().flex().justify_center().child(action_button(
                        "git-pr-merge",
                        &tr!("git_panel.merge_pull"),
                        Some(
                            icon(
                                "icons/git-merge.svg",
                                ButtonSize::Medium.icon_size().px(&theme),
                                theme.action_fg,
                            )
                            .into_any_element(),
                        ),
                        true,
                        self.pr_busy,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.request_merge_pull(cx)),
                    )),
                );
            content = content.child(merge_row);
        } else {
            content = content.child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(action_button(
                        "git-pr-reopen",
                        &tr!("git_panel.reopen_pull"),
                        Some(
                            icon(
                                "icons/refresh.svg",
                                ButtonSize::Medium.icon_size().px(&theme),
                                theme.text_secondary,
                            )
                            .into_any_element(),
                        ),
                        false,
                        self.pr_busy,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pull_state(cx)),
                    )),
            );
        }
        // The composer follows the merge box; the review actions sit under it,
        // and the quieter utility actions (checkout / close) trail the page.
        content = content.child(composer_field(theme, self.pr_comment.clone(), px(88.)));
        content = content.child(
            div()
                .flex()
                .items_center()
                .flex_wrap()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(action_button(
                    "git-pr-comment",
                    &tr!("git_panel.comment"),
                    Some(
                        icon(
                            "icons/chat.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.action_fg,
                        )
                        .into_any_element(),
                    ),
                    true,
                    self.pr_busy,
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.comment_pull(cx)),
                ))
                .child(action_button(
                    "git-pr-approve",
                    &tr!("git_panel.approve"),
                    Some(
                        icon(
                            "icons/check.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.success,
                        )
                        .into_any_element(),
                    ),
                    false,
                    self.pr_busy,
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.review_pull(gh::ReviewAction::Approve, cx)
                    }),
                ))
                .child(action_button(
                    "git-pr-changes",
                    &tr!("git_panel.request_changes"),
                    Some(
                        icon(
                            "icons/pencil.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.warning,
                        )
                        .into_any_element(),
                    ),
                    false,
                    self.pr_busy,
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.review_pull(gh::ReviewAction::RequestChanges, cx)
                    }),
                )),
        );
        // Utility actions last: they are the least primary thing on the page,
        // and "Close" sits here, away from the primary Merge button, so it
        // cannot be hit by a merge-shaped reflex.
        content = content.child(
            div()
                .pt(DynamicSpacing::Base12.px(&theme))
                .border_t_1()
                .border_color(theme.border_subtle)
                .flex()
                .items_center()
                .flex_wrap()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(action_button(
                    "git-pr-checkout",
                    &tr!("git_panel.checkout_pull"),
                    Some(
                        icon(
                            "icons/branch.svg",
                            ButtonSize::Medium.icon_size().px(&theme),
                            theme.text_secondary,
                        )
                        .into_any_element(),
                    ),
                    false,
                    self.pr_busy || !pull.is_open(),
                    theme,
                    cx.listener(|this, _: &ClickEvent, _, cx| this.checkout_pull(cx)),
                ))
                .children(pull.is_open().then(|| {
                    action_button(
                        "git-pr-close",
                        &tr!("git_panel.close_pull"),
                        Some(
                            icon(
                                "icons/x.svg",
                                ButtonSize::Medium.icon_size().px(&theme),
                                theme.text_secondary,
                            )
                            .into_any_element(),
                        ),
                        false,
                        self.pr_busy,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_pull_state(cx)),
                    )
                })),
        );
        github_detail_layout("git-pr-detail-scroll", header, details, content, theme)
            .into_any_element()
    }

    fn pr_new_view(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let generate_leading = if self.pr_generating {
            spinner(
                "git-pr-generate-spinner",
                ButtonSize::Medium.icon_size().px(&theme),
                theme.accent_text,
                theme,
            )
        } else {
            icon(
                "icons/spark.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                theme.accent_text,
            )
            .into_any_element()
        };
        let generate_label = if self.pr_generating {
            tr!("git_panel.generating_draft")
        } else {
            tr!("git_panel.generate_draft")
        };
        let card = div()
            .debug_selector(|| "github-new-form".into())
            .w_full()
            .min_w_0()
            .max_w(px(820.))
            .relative()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(tr!("git_panel.new_pull_title")),
                    )
                    .child(action_button(
                        "git-pr-new-generate",
                        &generate_label,
                        Some(generate_leading),
                        false,
                        self.pr_busy || self.pr_generating,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.generate_pull_draft(cx)),
                    )),
            )
            .child(composer_field(theme, self.pr_new_hint.clone(), px(0.)))
            .child(composer_field(theme, self.pr_new_title.clone(), px(0.)))
            .child(composer_field(theme, self.pr_new_body.clone(), px(220.)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .flex_wrap()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!("git_panel.base_branch")),
                    )
                    .child(self.base_branch_selector(theme, cx))
                    .child(github_check_toggle(
                        "git-pr-new-draft",
                        &tr!("git_panel.pr_draft"),
                        self.pr_new_draft,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.pr_new_draft = !this.pr_new_draft;
                            cx.notify();
                        }),
                    ))
                    .child(self.template_selector(
                        theme,
                        DraftForm::PullRequest,
                        "git-pr-template",
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(action_button(
                        "git-pr-new-cancel",
                        &tr!("git_panel.cancel"),
                        None,
                        false,
                        false,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.pr_new_open = false;
                            cx.notify();
                        }),
                    ))
                    .child(action_button(
                        "git-pr-new-create",
                        &tr!("git_panel.create_pull"),
                        None,
                        true,
                        self.pr_busy,
                        theme,
                        cx.listener(|this, _: &ClickEvent, _, cx| this.create_pull(cx)),
                    )),
            );
        div()
            .id("git-pr-new-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_center()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base20.px(&theme))
                    .child(card),
            )
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

    /// The tip commit date of a local branch, for the menu's last-active
    /// column. `recent_branches` carries names only (it comes from the reflog),
    /// so its rows read the date from the branch list.
    fn branch_commit_date(&self, name: &str) -> Option<&str> {
        self.branches
            .iter()
            .find(|entry| entry.name == name)
            .and_then(|entry| entry.commit_date.as_deref())
    }

    fn branch_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.branch_menu_open {
            return None;
        }
        let needle = self.branch_filter.read(cx).text().trim().to_lowercase();
        let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
        let current = self.branch.clone();

        // The scroll container clips vertically; the surface around it clips
        // horizontally so a long branch name can never spill past the card.
        let mut list = div()
            .id("git-branch-scroll")
            .w_full()
            .max_h(px(460.))
            .overflow_y_scroll()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();

        // A dirty worktree can block a switch, so say so before the list.
        if !self.staged.is_empty() || !self.unstaged.is_empty() {
            list = list.child(
                div()
                    .mx(DynamicSpacing::Base04.px(&theme))
                    .mb(DynamicSpacing::Base04.px(&theme))
                    .px(DynamicSpacing::Base08.px(&theme))
                    .py(DynamicSpacing::Base06.px(&theme))
                    .rounded(Radius::Medium.px(&theme))
                    .bg(theme.warning.opacity(0.1))
                    .flex()
                    .items_start()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(icon(
                        "icons/info.svg",
                        IconSize::Small.px(&theme),
                        theme.warning,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .whitespace_normal()
                            .child(tr!("git_panel.dirty_switch_warning")),
                    ),
            );
        }

        list = list.child(
            press(
                picker_entry(div().id("git-menu-new-branch").group(BUTTON_GROUP), &theme)
                    .cursor_pointer()
                    .text_color(theme.text_primary)
                    .hover_state(&theme),
            )
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_branch_prompt(BranchPrompt::New, window, cx)
            }))
            .child(icon(
                "icons/plus.svg",
                context_menu::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(tr!("git_panel.create_new_branch")),
        );

        let mut locals: Vec<&git::BranchEntry> = self
            .branches
            .iter()
            .filter(|b| matches(b.name.as_str()))
            .collect();
        locals.sort_by_key(|b| b.name.to_lowercase());

        let remotes: Vec<&git_ops::RefEntry> = self
            .refs
            .iter()
            .filter(|r| r.kind == git::RefKind::Remote && matches(r.name.as_str()))
            .collect();

        // "Recent" is only meaningful before a search narrows the list.
        let recent: Vec<(&str, Option<&str>)> = if needle.is_empty() {
            self.recent_branches
                .iter()
                .take(5)
                .map(|name| (name.as_str(), self.branch_commit_date(name)))
                .collect()
        } else {
            Vec::new()
        };

        if !recent.is_empty() {
            list = list.child(branch_section_label(
                &tr!("git_panel.recent_branches"),
                theme,
            ));
            for (branch, date) in &recent {
                list = list.child(self.branch_row(
                    branch,
                    *date,
                    current.as_deref(),
                    false,
                    theme,
                    cx,
                ));
            }
        }
        if !locals.is_empty() {
            list = list.child(branch_section_label(
                &tr!("git_panel.local_branches"),
                theme,
            ));
            for entry in &locals {
                list = list.child(self.branch_row(
                    &entry.name,
                    entry.commit_date.as_deref(),
                    current.as_deref(),
                    false,
                    theme,
                    cx,
                ));
            }
        }
        if !remotes.is_empty() {
            list = list.child(branch_section_label(
                &tr!("git_panel.remote_branches"),
                theme,
            ));
            for entry in &remotes {
                list = list.child(self.branch_row(
                    &entry.name,
                    entry.commit_date.as_deref(),
                    current.as_deref(),
                    true,
                    theme,
                    cx,
                ));
            }
        }
        if locals.is_empty() && remotes.is_empty() && recent.is_empty() {
            list = list.child(
                picker_entry(div(), &theme)
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.no_matching_branches")),
            );
        }

        let menu = picker_surface(div().id("git-branch-menu"), &theme)
            .w(px(BRANCH_MENU_W))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dismiss_branch_menu(cx);
            }))
            .child(
                picker_search_frame(div(), &theme)
                    .child(icon(
                        "icons/search.svg",
                        input::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(div().flex_1().min_w_0().child(self.branch_filter.clone())),
            )
            .child(list);

        Some(
            div()
                .absolute()
                // Below the branch bar (tabs 40 + bar 40); the row's 4px
                // inline margin lands its edge under the branch chip.
                .top(px(78.))
                .left(DynamicSpacing::Base12.px(&theme))
                .child(deferred(menu))
                .into_any_element(),
        )
    }

    /// One row in the branch picker: the name, the age of its tip commit, a
    /// `Current` marker for HEAD, and a hover-revealed delete for a
    /// non-current local branch. The age is the row's last child so every
    /// date lands on the same right edge, whatever precedes it.
    fn branch_row(
        &self,
        name: &str,
        commit_date: Option<&str>,
        current: Option<&str>,
        remote: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = !remote && current == Some(name);
        let branch = name.to_string();
        let branch_delete = name.to_string();
        let id = gpui::ElementId::Name(format!("git-branch-{name}").into());
        let mut row = picker_entry(div().id(id).group("git-branch-row"), &theme)
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(theme.surface_selected)
                    .text_color(theme.selection_fg)
            })
            .when(!selected, |row| {
                row.text_color(theme.text_secondary).hover_state(&theme)
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if selected {
                    this.dismiss_branch_menu(cx);
                } else {
                    this.checkout_branch_ref(branch.clone(), remote, cx);
                }
            }))
            .child(icon(
                if remote {
                    "icons/cloud.svg"
                } else {
                    "icons/branch.svg"
                },
                context_menu::ICON.px(&theme),
                if selected {
                    theme.text_primary
                } else {
                    theme.text_muted
                },
            ))
            .child(div().min_w_0().flex_1().truncate().child(name.to_string()));
        if selected {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.accent_text)
                    .child(tr!("git_panel.current")),
            );
        } else if !remote {
            // Delete a non-current local branch. The safe `-d` is tried first;
            // an unmerged branch then asks before the forced `-D`.
            let delete_id = gpui::ElementId::Name(format!("git-branch-delete-{name}").into());
            row = row.child(
                icon_button_frame(div().id(delete_id), &theme, ButtonSize::Compact)
                    .cursor_pointer()
                    .opacity(0.)
                    .group_hover("git-branch-row", |s| s.opacity(1.))
                    .hover_state(&theme)
                    .tooltip({
                        let label = tr!("git_panel.tip_delete_branch");
                        move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.delete_branch(branch_delete.clone(), cx);
                    }))
                    .child(icon(
                        "icons/trash.svg",
                        ButtonSize::Compact.icon_size().px(&theme),
                        theme.text_muted,
                    )),
            );
        }
        if let Some(age) = commit_date.map(crate::gh::relative_time) {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(picker::SECONDARY_TEXT.px(&theme))
                    .text_color(if selected {
                        theme.text_secondary
                    } else {
                        theme.text_muted
                    })
                    .child(age),
            );
        }
        row.into_any_element()
    }

    /// The ref picker opened by "Merge branch…" / "Rebase onto…".
    fn ref_picker_popup(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.ref_menu?;
        // The New-PR base picker is rendered inside the form, anchored to its
        // chip (see [`Self::base_branch_selector`]); this page-level popup only
        // serves the Merge / Rebase targets launched from the branch bar.
        if target == RefTarget::PrBase {
            return None;
        }
        let title = match target {
            RefTarget::Merge => tr!("git_panel.merge_branch"),
            RefTarget::Rebase => tr!("git_panel.rebase_onto"),
            RefTarget::PrBase => tr!("git_panel.base_branch"),
        };
        let mut menu = picker_surface(div().id("git-ref-menu"), &theme)
            .absolute()
            .top(px(42.))
            .right(px(12.))
            .w(px(300.))
            .max_h(px(360.))
            .py(picker::list_padding_y(&theme))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.close_ref_picker(cx);
            }))
            .child(menu_header(title, &theme));
        if target == RefTarget::Merge {
            // How the merge combines the target, chosen before picking a ref.
            menu = menu.child(
                div()
                    .px(DynamicSpacing::Base06.px(&theme))
                    .pb(DynamicSpacing::Base04.px(&theme))
                    .flex()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .children(
                        [
                            (git_ops::MergeMode::Merge, "git_panel.merge_mode_ff"),
                            (git_ops::MergeMode::NoFf, "git_panel.merge_mode_commit"),
                            (git_ops::MergeMode::Squash, "git_panel.merge_mode_squash"),
                        ]
                        .into_iter()
                        .map(|(mode, key)| {
                            let selected = self.merge_mode == mode;
                            let id = gpui::ElementId::Name(format!("git-merge-mode-{key}").into());
                            button_frame(div().id(id), &theme, ButtonSize::Medium)
                                .cursor_pointer()
                                .when(selected, |s| {
                                    s.bg(theme.surface_selected).text_color(theme.selection_fg)
                                })
                                .when(!selected, |s| {
                                    s.text_color(theme.text_muted).hover_state(&theme)
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
                picker_entry(div(), &theme)
                    .h(picker::entry_height(&theme))
                    .flex_none()
                    .text_color(theme.text_muted)
                    .child(tr!("git_panel.no_refs")),
            );
        }
        for entry in &self.refs {
            // The new-PR base picker only offers branches, not tags/remotes.
            if target == RefTarget::PrBase && entry.kind != git::RefKind::Branch {
                continue;
            }
            let name = entry.name.clone();
            let label = entry.name.clone();
            let id = gpui::ElementId::Name(format!("git-ref-{name}").into());
            menu = menu.child(
                picker_entry(div().id(id), &theme)
                    .h(picker::entry_height(&theme))
                    .flex_none()
                    .cursor_pointer()
                    .text_color(theme.text_secondary)
                    .hover_state(&theme)
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| match target {
                            RefTarget::Merge => this.merge_ref(&name, cx),
                            RefTarget::Rebase => this.rebase_ref(&name, cx),
                            RefTarget::PrBase => {
                                this.pr_new_base = Some(name.clone());
                                this.close_ref_picker(cx);
                            }
                        }),
                    )
                    .child(icon(
                        ref_icon(entry.kind),
                        context_menu::ICON.px(&theme),
                        theme.text_muted,
                    ))
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
    fn branch_prompt_popup(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
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
        let card = div()
            .w_full()
            .max_w(px(420.))
            .elevation_2(&theme)
            .p(DynamicSpacing::Base16.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_modal(cx)),
            )
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(title),
            )
            .child(
                input_field_frame(div(), &theme)
                    .bg(theme.surface_secondary)
                    .child(div().flex_1().min_w_0().child(self.branch_input.clone())),
            )
            .child(
                div()
                    .mt(DynamicSpacing::Base02.px(&theme))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(DynamicSpacing::Base08.px(&theme))
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
                .px(DynamicSpacing::Base16.px(&theme))
                .bg(theme.background.opacity(0.55))
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
            .elevation_2(&theme)
            .p(DynamicSpacing::Base16.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_modal(cx)),
            )
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(if staged_empty {
                        tr!("git_panel.stage_prompt_title_generate")
                    } else {
                        tr!("git_panel.stage_prompt_title_rest")
                    }),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .line_height(theme.ui_px(17.))
                    .text_color(theme.text_secondary)
                    .whitespace_normal()
                    .child(body),
            )
            .child(
                div()
                    .mt(DynamicSpacing::Base02.px(&theme))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(DynamicSpacing::Base08.px(&theme))
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
                        Some(
                            icon(
                                "icons/magic-wand.svg",
                                ButtonSize::Medium.icon_size().px(&theme),
                                theme.action_fg,
                            )
                            .into_any_element(),
                        ),
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
                .px(DynamicSpacing::Base16.px(&theme))
                .bg(theme.background.opacity(0.55))
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
            PendingConfirm::MergePull { number, method, .. } => {
                let method_label = match method {
                    gh::MergeMethod::Merge => tr!("git_panel.merge_method_merge"),
                    gh::MergeMethod::Squash => tr!("git_panel.merge_method_squash"),
                    gh::MergeMethod::Rebase => tr!("git_panel.merge_method_rebase"),
                };
                (
                    tr!("git_panel.confirm_merge_title", number = number),
                    tr!("git_panel.confirm_merge_body", method = method_label),
                    tr!("git_panel.merge_pull"),
                )
            }
            PendingConfirm::DiscardAll => (
                tr!("git_panel.confirm_discard_all_title"),
                tr!("git_panel.confirm_discard_all_body"),
                tr!("git_panel.discard_all"),
            ),
            PendingConfirm::DiscardPath(path) => (
                tr!("git_panel.confirm_discard_title", name = path),
                tr!("git_panel.confirm_discard_body"),
                tr!("git_panel.discard"),
            ),
        };
        let card = div()
            .debug_selector(|| "git-confirm-card".into())
            // Give GPUI a definite width when measuring wrapped text. A
            // percentage width capped by max_w measures at the wider parent
            // width, leaving the footer outside the painted card.
            .w(px(420.))
            .max_w_full()
            .elevation_2(&theme)
            .p(DynamicSpacing::Base16.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .occlude()
            .on_mouse_down_out(
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_modal(cx)),
            )
            .child(
                div()
                    .debug_selector(|| "git-confirm-title".into())
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(title),
            )
            .child(
                div()
                    .debug_selector(|| "git-confirm-body".into())
                    .text_size(TextSize::Small.px(&theme))
                    .line_height(theme.ui_px(17.))
                    .text_color(theme.text_secondary)
                    .whitespace_normal()
                    .child(body),
            )
            .child(
                div()
                    .debug_selector(|| "git-confirm-actions".into())
                    .mt(DynamicSpacing::Base02.px(&theme))
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_end()
                    .gap(DynamicSpacing::Base08.px(&theme))
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
                .px(DynamicSpacing::Base16.px(&theme))
                .bg(theme.background.opacity(0.55))
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = *theme::get(cx);
        let content = match self.tab {
            GitTab::Changes => self.changes_tab(theme, cx),
            GitTab::History => self.history_tab(theme, cx),
            GitTab::Graph => self.graph_tab(theme, cx),
            GitTab::Stashes => self.stashes_tab(theme, cx),
            GitTab::Issues => self.issues_tab(theme, cx),
            GitTab::Pulls => self.pulls_tab(theme, cx),
        };
        div()
            .id("git-page")
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .bg(theme.background)
            .font_family(theme::ui_font_family())
            .flex()
            .flex_col()
            .child(self.tab_bar(theme, cx))
            // The branch / sync row sits under the tabs: the reference keeps
            // the branch context off the tab row so neither crowds the other.
            .child(self.branch_bar(theme, cx))
            // Failures stay pinned under the tabs (never inside the scrolling
            // body) so push/pull/merge errors are readable on every tab.
            .children(self.failure_banner(theme, cx))
            // An unfinished merge/rebase sits directly under the failure banner,
            // above every tab, so it is the page's single "you are mid-merge"
            // signal regardless of tab.
            .children(self.operation_bar(theme, cx))
            .child(content)
            .children(self.ref_picker_popup(theme, cx))
            .children(self.branch_menu(theme, cx))
            .children(self.file_actions_menu(theme, cx))
            .children(self.stage_prompt_popup(theme, cx))
            .children(self.confirm_popup(theme, cx))
            .children(self.branch_prompt_popup(theme, cx))
            .into_any_element()
    }
}

/// Reserve enough room for a compact browser and a usable detail surface.
fn split_github_view(width: Pixels) -> bool {
    width >= px(800.)
}

/// The list pane's width inside the split view: never below its own minimum,
/// never wider than the detail pane and its grab strip can afford. `stored` is
/// the persisted choice; clamping here means a narrow window narrows the list
/// without overwriting the width the user picked.
fn github_list_pane_width(page_width: Pixels, stored: Pixels) -> Pixels {
    let reserve = px(GITHUB_DETAIL_MIN_W) + px(GITHUB_LIST_HANDLE_W);
    let ceiling = (page_width - reserve)
        .min(px(GITHUB_LIST_MAX_W))
        .max(px(GITHUB_LIST_MIN_W));
    stored.clamp(px(GITHUB_LIST_MIN_W), ceiling)
}

/// The width a pointer drag asks for, or `None` when the pane is already held
/// at a clamped edge and the pointer push would not move it on screen. Returning
/// `None` is what keeps a narrow page from replacing the stored preference with
/// its clamped display width.
fn dragged_github_list_pane_width(
    page_width: Pixels,
    stored: Pixels,
    origin_width: Pixels,
    origin_x: Pixels,
    pointer_x: Pixels,
) -> Option<Pixels> {
    let shown = github_list_pane_width(page_width, stored);
    let width = github_list_pane_width(page_width, origin_width + pointer_x - origin_x);
    (width != shown).then_some(width)
}

/// The width a release should write, if any: a click or a push against a
/// clamped edge writes nothing, and the value must be inside the draggable
/// range rather than a page-clamped display width.
fn github_list_width_to_persist(drag: Option<&GithubListDrag>, width: Pixels) -> Option<f32> {
    let drag = drag?;
    if !drag.moved {
        return None;
    }
    let width = f32::from(width);
    (GITHUB_LIST_MIN_W..=GITHUB_LIST_MAX_W)
        .contains(&width)
        .then_some(width)
}

/// The list/detail split: the resizable list, the drag handle that replaces its
/// static border, and the detail surface taking the rest.
fn github_split_view(
    list: AnyElement,
    detail: Option<AnyElement>,
    list_width: Pixels,
    handle: AnyElement,
) -> AnyElement {
    div()
        .flex_1()
        .h_full()
        .min_h_0()
        .min_w_0()
        .flex()
        .child(
            div()
                .debug_selector(|| "github-split-list".into())
                .w(list_width)
                .flex_none()
                .min_h_0()
                .flex()
                .flex_col()
                .child(list),
        )
        .child(handle)
        .child(
            div()
                .debug_selector(|| "github-split-detail".into())
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .children(detail),
        )
        .into_any_element()
}

/// The fixed bar above a detail keeps the title beside the back button.
fn github_detail_header(back: AnyElement, title: &str, theme: Theme) -> gpui::Div {
    // Pull the button's frame out so its glyph, not its hover wash, sits on
    // the content edge the document below is aligned to.
    let inset =
        (ButtonSize::Default.height(&theme) - ButtonSize::Default.icon_size().px(&theme)) / 2.;
    div()
        .debug_selector(|| "github-detail-header".into())
        .w_full()
        .min_w_0()
        .flex_none()
        .px(DynamicSpacing::Base16.px(&theme))
        .py(DynamicSpacing::Base12.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(
            div()
                .debug_selector(|| "github-detail-back".into())
                .flex_none()
                .ml(-inset)
                .child(back),
        )
        .child(github_detail_pinned_title(title, theme))
}

fn github_detail_details(theme: Theme) -> gpui::Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base12.px(&theme))
}

fn github_detail_pinned_title(title: &str, theme: Theme) -> AnyElement {
    // Ellipsized at paint time: see `EllipsizedLine` for why the stock text
    // element painted this title as a lone `…` in the split view.
    let line = widgets::EllipsizedLine::new(single_line(title));
    #[cfg(test)]
    PINNED_TITLE_PAINTED.with(|slot| *slot.borrow_mut() = Some(line.painted()));
    div()
        .debug_selector(|| "github-detail-pinned-title".into())
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_size(TextSize::Large.px(&theme))
        .line_height(theme.ui_px(22.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.text_primary)
        .child(line)
        .into_any_element()
}

#[cfg(test)]
thread_local! {
    /// The pinned title as last painted, so a test reads the painted text
    /// rather than the bounds of its box.
    static PINNED_TITLE_PAINTED: std::cell::RefCell<
        Option<std::rc::Rc<std::cell::RefCell<gpui::SharedString>>>,
    > = const { std::cell::RefCell::new(None) };
}

/// Type, number, state and repository form one quiet, wrapping eyebrow. State
/// is communicated by both glyph and words, rather than a large colored pill.
fn github_detail_eyebrow(
    kind: &str,
    url: &str,
    number: u64,
    state: &str,
    glyph: &'static str,
    color: Hsla,
    theme: Theme,
) -> AnyElement {
    div()
        .debug_selector(|| "github-detail-eyebrow".into())
        .w_full()
        .min_w_0()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .text_size(TextSize::Small.px(&theme))
        .text_color(theme.text_muted)
        .child(icon(
            "icons/github.svg",
            IconSize::XSmall.px(&theme),
            theme.text_secondary,
        ))
        .child(format!("{kind} · #{number}"))
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .text_color(color)
                .child(icon(glyph, IconSize::XSmall.px(&theme), color))
                .child(state.to_string()),
        )
        .children(github_repo(url).map(|repo| {
            div()
                .min_w_0()
                .max_w(px(240.))
                .truncate()
                .child(format!("· {repo}"))
        }))
        .into_any_element()
}

fn github_empty_description(theme: Theme) -> AnyElement {
    div()
        .w_full()
        .py(DynamicSpacing::Base08.px(&theme))
        .text_size(TextSize::Default.px(&theme))
        .text_color(theme.text_muted)
        .child(tr!("git_panel.no_description"))
        .into_any_element()
}

/// Keep the identity fixed and let metadata and the document move. A centered maximum
/// width preserves long-line readability on large monitors; on the smallest
/// split detail it fills the available width with a consistent inset.
fn github_detail_layout(
    scroll_id: &'static str,
    header: impl IntoElement,
    details: gpui::Div,
    content: gpui::Div,
    theme: Theme,
) -> gpui::Div {
    div()
        .w_full()
        .flex_1()
        .min_w_0()
        .min_h_0()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex_none()
                .flex()
                .justify_center()
                .border_b_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .max_w(px(860.) + DynamicSpacing::Base16.px(&theme) * 2.)
                        .child(header),
                ),
        )
        .child(
            div()
                .id(scroll_id)
                .debug_selector(|| "github-detail-scroll".into())
                .flex_1()
                .min_h_0()
                .min_w_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .items_center()
                .child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex_none()
                        .max_w(px(860.) + DynamicSpacing::Base16.px(&theme) * 2.)
                        .px(DynamicSpacing::Base16.px(&theme))
                        .py(DynamicSpacing::Base20.px(&theme))
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base20.px(&theme))
                        .child(details)
                        .child(content),
                ),
        )
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

/// One entry of the Git page's dropdown menus (sync, branch "…", file
/// actions), on the branch selector's picker metrics: one `picker_entry`
/// row, `overlay` hover, and the shared menu icon size.
fn menu_row(
    id: &'static str,
    icon_path: &'static str,
    label: &str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    press(
        picker_entry(div().id(id).group(BUTTON_GROUP), &theme)
            .h(picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .text_color(theme.text_secondary)
            .hover_state(&theme),
    )
    .on_click(listener)
    .child(icon(
        icon_path,
        context_menu::ICON.px(&theme),
        theme.text_muted,
    ))
    .child(div().flex_1().min_w_0().truncate().child(label.to_string()))
    .into_any_element()
}

/// A hairline between menu groups (Zed's `ListSeparator`).
fn menu_separator(theme: Theme) -> AnyElement {
    context_menu_separator(&theme).into_any_element()
}

/// A section header in the branch picker ("Recent branches", …): Zed's
/// `ListSubHeader`.
fn branch_section_label(label: &str, theme: Theme) -> AnyElement {
    menu_header(label.to_string(), &theme).into_any_element()
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
        .group("git-row")
        .mx(DynamicSpacing::Base12.px(&theme))
        .h(px(32.))
        .px(DynamicSpacing::Base08.px(&theme))
        .rounded(Radius::Medium.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .hover_state(&theme)
        .child(icon(
            "icons/clock.svg",
            IconSize::XSmall.px(&theme),
            theme.text_muted,
        ))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_primary)
                .child(stash.message.clone()),
        )
        // Row actions reveal on hover, matching the changed-file rows above.
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .opacity(0.)
                .group_hover("git-row", |s| s.opacity(1.))
                .child(row_button(
                    format!("git-stash-pop-{index}"),
                    "icons/arrow-down.svg",
                    "git_panel.tip_stash_pop",
                    theme,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.stash_pop(index, cx)),
                ))
                .child(row_button(
                    format!("git-stash-apply-{index}"),
                    "icons/check.svg",
                    "git_panel.tip_stash_apply",
                    theme,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.stash_apply(index, cx)),
                ))
                .child(row_button(
                    format!("git-stash-drop-{index}"),
                    "icons/trash.svg",
                    "git_panel.tip_stash_drop",
                    theme,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.stash_drop(index, cx)),
                )),
        )
        .into_any_element()
}

/// Whether the create action collapses to its plus glyph. The label remains in
/// the tooltip, while wider panes show the full action beside the icon.
fn compact_github_tools(page_width: Pixels, stored_list_width: Pixels) -> bool {
    let width = if split_github_view(page_width) {
        github_list_pane_width(page_width, stored_list_width)
    } else {
        page_width
    };
    width < px(360.)
}

/// One quiet state tab in an Issues/Pulls toolbar. Selection is a tonal step,
/// not a row of accent-filled pills competing with the work items below.
fn github_state_tab(
    id: &'static str,
    label: &str,
    active: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    button_frame(div().id(id), &theme, ButtonSize::Default)
        .group(BUTTON_GROUP)
        .border_1()
        .border_color(gpui::transparent_black())
        .when(active, |tab| {
            tab.bg(theme.surface_selected)
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
        })
        .when(!active, |tab| tab.text_color(theme.text_muted))
        .cursor_pointer()
        .hover_state(&theme)
        .pressed_state(&theme, PressedRamp::Wash)
        .on_click(listener)
        .child(label.to_string())
        .into_any_element()
}

/// A quiet checkbox row used for consequential secondary options such as
/// deleting the PR branch after merge. The checkbox makes its state explicit.
fn github_check_toggle(
    id: &'static str,
    label: &str,
    checked: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    button_frame(div().id(id), &theme, ButtonSize::Default)
        .group(BUTTON_GROUP)
        .border_1()
        .border_color(gpui::transparent_black())
        .text_color(theme.text_secondary)
        .cursor_pointer()
        .hover_state(&theme)
        .pressed_state(&theme, PressedRamp::Wash)
        .on_click(listener)
        .child(check_box(checked, theme))
        .child(label.to_string())
        .into_any_element()
}

/// The repository-label picker trigger. Its text is capped and ellipsized so
/// even the longest GitHub label cannot push search or create off the toolbar.
fn github_label_filter_button(
    id: &'static str,
    label: &str,
    open: bool,
    active: bool,
    compact: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let button = button_frame(div().id(id), &theme, ButtonSize::Medium)
        .debug_selector(|| "github-label-filter".into())
        .max_w(px(104.))
        .border_1()
        .border_color(if active {
            theme.border_strong
        } else {
            theme.border
        })
        .raised(
            if open {
                theme.surface_active
            } else {
                theme.surface_elevated
            },
            &theme,
        )
        .cursor_pointer()
        .hover_state(&theme)
        .on_click(listener)
        .child(icon(
            "icons/tag-01.svg",
            ButtonSize::Medium.icon_size().px(&theme),
            if active {
                theme.accent_text
            } else {
                theme.text_muted
            },
        ));
    if compact {
        let tip = if active {
            tr!("git_panel.label_chip", name = label)
        } else {
            tr!("git_panel.label_filter")
        };
        return button
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
            .into_any_element();
    }
    button
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(theme.text_secondary)
                .child(label.to_string()),
        )
        .child(icon(
            "icons/chevron-down.svg",
            IconSize::Indicator.px(&theme),
            theme.text_muted,
        ))
        .into_any_element()
}

/// Create is the toolbar's one primary action. It keeps its full label when
/// there is room and becomes a labelled, tooltip-equipped plus in a narrow
/// list instead of forcing every other tool to wrap.
fn github_create_button(
    id: &'static str,
    label: &str,
    compact: bool,
    disabled: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    if !compact {
        return div()
            .debug_selector(|| "github-create".into())
            .child(action_button(
                id,
                label,
                Some(
                    icon(
                        "icons/plus.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.action_fg,
                    )
                    .into_any_element(),
                ),
                true,
                disabled,
                theme,
                listener,
            ))
            .into_any_element();
    }
    let tooltip = label.to_string();
    let button = icon_button_frame(div().id(id), &theme, ButtonSize::Medium)
        .debug_selector(|| "github-create".into())
        .group(BUTTON_GROUP)
        .border_1()
        .border_color(gpui::transparent_black())
        .raised(theme.action, &theme)
        .text_color(theme.action_fg)
        .cursor_pointer()
        .hover_action(&theme)
        .pressed_state(&theme, PressedRamp::Action)
        .child(icon(
            "icons/plus.svg",
            ButtonSize::Medium.icon_size().px(&theme),
            if disabled {
                theme.text_disabled
            } else {
                theme.action_fg
            },
        ));
    if disabled {
        return button
            .border_color(theme.border)
            .bg(theme.overlay)
            .text_color(theme.text_disabled)
            .into_any_element();
    }
    press(button)
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
        .on_click(listener)
        .into_any_element()
}

/// A removable filter chip (label plus a trailing glyph). `trailing` is an `×`
/// for chips that clear themselves on click, or a chevron for chips that open
/// a picker, so the affordance always matches the behaviour.
fn filter_chip(
    id: &'static str,
    label: &str,
    trailing: &'static str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .border_1()
        .border_color(theme.border)
        .raised(theme.surface_elevated, &theme)
        .cursor_pointer()
        .text_color(theme.text_secondary)
        .hover_state(&theme)
        .on_click(listener)
        .child(label.to_string())
        .child(icon(
            trailing,
            ButtonSize::Medium.icon_size().px(&theme),
            theme.text_muted,
        ))
        .into_any_element()
}

/// An on/off filter chip ("All branches"). `glyph` is shown only where it adds
/// meaning (a branch-scoped toggle); state and mode chips render label-only so
/// the same icon never stands for two different things.
fn filter_toggle_chip(
    id: &'static str,
    label: &str,
    glyph: Option<&'static str>,
    active: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .border_1()
        .border_color(if active { theme.accent } else { theme.border })
        .when(active, |chip| {
            // An accent-filled toggle: the fill matches the accent border, so
            // its interactive states ride the accent's own steps.
            chip.raised(theme.accent, &theme)
                .text_color(theme.accent_fg)
                .hover_accent(&theme)
                .pressed_state(&theme, PressedRamp::Accent)
        })
        .when(!active, |chip| {
            chip.text_color(theme.text_muted).hover_state(&theme)
        })
        .cursor_pointer()
        .on_click(listener)
        .children(glyph.map(|path| {
            icon(
                path,
                ButtonSize::Medium.icon_size().px(&theme),
                if active {
                    theme.action_fg
                } else {
                    theme.text_muted
                },
            )
        }))
        .child(label.to_string())
        .into_any_element()
}

/// A chip that opens a picker, styled like the top branch selector: the glyph,
/// the current value, and a chevron, filling with the active color while its
/// menu is open.
fn dropdown_chip(
    id: &'static str,
    label: &str,
    glyph: &'static str,
    open: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    button_frame(div().id(id), &theme, ButtonSize::Medium)
        .border_1()
        .border_color(theme.border)
        .raised(
            if open {
                theme.surface_active
            } else {
                theme.surface_elevated
            },
            &theme,
        )
        .cursor_pointer()
        .hover_state(&theme)
        .on_click(listener)
        .child(icon(
            glyph,
            ButtonSize::Medium.icon_size().px(&theme),
            if open {
                theme.text_primary
            } else {
                theme.text_secondary
            },
        ))
        .child(
            div()
                .max_w(px(180.))
                .overflow_hidden()
                .whitespace_nowrap()
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(label.to_string()),
        )
        .child(icon(
            "icons/chevron-down.svg",
            IconSize::XSmall.px(&theme),
            if open {
                theme.text_primary
            } else {
                theme.text_muted
            },
        ))
        .into_any_element()
}

/// The commit-model picker's rows: the follow-the-session option first, then
/// every catalog model matching `needle` on its name, id, or provider. `None`
/// is the session option; `Some` indexes the catalog.
fn commit_model_rows(models: &[ModelEntry], needle: &str) -> Vec<Option<usize>> {
    let mut rows = vec![None];
    for (ix, model) in models.iter().enumerate() {
        let provider = crate::providers::provider_display_name(&model.provider);
        if needle.is_empty()
            || model.name.to_lowercase().contains(needle)
            || model.id.to_lowercase().contains(needle)
            || model.provider.to_lowercase().contains(needle)
            || provider.to_lowercase().contains(needle)
        {
            rows.push(Some(ix));
        }
    }
    rows
}

/// A small placeholder shown inside an expanding commit row while its detail
/// loads (or when it could not be loaded).
fn detail_note(theme: Theme, label: &str) -> AnyElement {
    div()
        .mx(DynamicSpacing::Base12.px(&theme))
        .mb(DynamicSpacing::Base06.px(&theme))
        .px(DynamicSpacing::Base12.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .rounded(panel::RADIUS.px(&theme))
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_elevated)
        .text_size(TextSize::Small.px(&theme))
        .text_color(theme.text_muted)
        .child(label.to_string())
        .into_any_element()
}

// ── Issues ─────────────────────────────────────────────────────────

/// One in-flight divider drag. `origin_width` is what was displayed when the
/// pointer went down (not necessarily the stored preference), so a clamped
/// pane does not lose the width the user had picked.
#[derive(Debug)]
struct GithubListDrag {
    origin_width: Pixels,
    origin_x: Pixels,
    /// Whether the drag ever changed the width on screen. A click or a push
    /// against a clamped edge writes nothing.
    moved: bool,
}

/// The label chips for a list row: up to `take` real chips plus a `+N`
/// indicator when more are hidden, so a heavily labeled item never needs a
/// fourth line to show them all.
fn label_summary(labels: &[gh::GhLabel], take: usize, theme: Theme) -> Vec<AnyElement> {
    let mut chips: Vec<AnyElement> = labels
        .iter()
        .take(take)
        .map(|label| list_label_chip(label, theme))
        .collect();
    if labels.len() > take {
        chips.push(
            div()
                .debug_selector(|| "github-row-label-overflow".into())
                .h(px(18.))
                .flex_none()
                .px(DynamicSpacing::Base06.px(&theme))
                .rounded(Radius::Small.px(&theme))
                .border_1()
                .border_color(theme.border)
                .flex()
                .items_center()
                .text_size(TextSize::XSmall.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_muted)
                .child(format!("+{}", labels.len() - take))
                .into_any_element(),
        );
    }
    chips
}

/// A GitHub item's title flattened to one line. Titles come from GitHub and may
/// carry newlines and runs of whitespace; a row that let them through would grow
/// a second line and break the three-line anatomy.
fn single_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A compact draft marker for a pull-request row: a badge on the header line, so
/// a draft is visible without earning the row a fourth line. The detail header
/// repeats the full draft state.
fn github_draft_marker(theme: Theme) -> AnyElement {
    div()
        .debug_selector(|| "github-row-draft".into())
        .flex_none()
        .h(px(16.))
        .px(DynamicSpacing::Base04.px(&theme))
        .rounded(Radius::Small.px(&theme))
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .text_size(TextSize::XSmall.px(&theme))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_muted)
        .child(tr!("git_panel.pr_draft"))
        .into_any_element()
}

/// The GitHub action on a list row. Always visible — an essential link never
/// waits for a hover — with the tooltip naming the glyph rather than being the
/// only way to find it.
fn github_row_link(number: u64, url: &str, theme: Theme) -> AnyElement {
    let label = tr!("git_panel.open_on_github");
    let url = url.to_owned();
    press(
        icon_button_frame(
            div()
                .id(gpui::ElementId::Name(format!("git-open-{number}").into()))
                .debug_selector(|| "github-row-link".into()),
            &theme,
            ButtonSize::Compact,
        )
        .group(BUTTON_GROUP)
        .cursor_pointer()
        .hover_state(&theme),
    )
    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into())
    .on_click(move |_, _, cx| {
        cx.stop_propagation();
        cx.open_url(&url);
    })
    .child(icon(
        "icons/github.svg",
        ButtonSize::Compact.icon_size().px(&theme),
        theme.text_muted,
    ))
    .into_any_element()
}

/// Line one of every GitHub list row: state glyph, translated state and number,
/// an optional draft marker, the relative updated time right-aligned, and the
/// GitHub action. The state cluster truncates rather than wrapping, so this is
/// one line at any normal width; wrapping stays only as the fallback for an
/// extreme text scale, where the time cannot shrink.
fn github_row_header(parts: &GithubRowParts<'_>, theme: Theme) -> AnyElement {
    let mut header = div()
        .debug_selector(|| "github-row-header".into())
        .w_full()
        .min_w_0()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .child(
            div()
                .debug_selector(|| "github-row-state".into())
                // A zero basis keeps the (possibly long) state text out of the
                // line-break decision, so it truncates instead of pushing the
                // time onto another line.
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(div().flex_none().child(icon(
                    parts.glyph,
                    IconSize::Small.px(&theme),
                    parts.color,
                )))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(format!("{} · #{}", parts.state, parts.number)),
                ),
        )
        .children(parts.draft.then(|| github_draft_marker(theme)))
        .child(
            div()
                .debug_selector(|| "github-row-time".into())
                .flex_none()
                .whitespace_nowrap()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_muted)
                .child(parts.time.clone()),
        );
    if !parts.url.is_empty() {
        header = header.child(github_row_link(parts.number, parts.url, theme));
    }
    header.into_any_element()
}

/// Line two: the title, exactly one line at a definite width, ellipsized when it
/// does not fit. The detail pane shows it in full.
///
/// The run sits inside a flex row with `flex_1 min_w_0` on the text itself. A
/// percentage-width block that is a direct child of the row's column is measured
/// against an indefinite width, so its run truncates at zero and paints nothing
/// but the ellipsis (see DESIGN.md, sidebar rows).
fn github_row_title(title: &str, emphasized: bool, theme: Theme) -> AnyElement {
    div()
        .min_w_0()
        .flex()
        // Clip at the row's content edge, so a long title stops inside the
        // row's padding instead of running to the divider.
        .overflow_hidden()
        .child(
            div()
                .debug_selector(|| "github-row-title".into())
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if emphasized {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .child(single_line(title)),
        )
        .into_any_element()
}

/// Line three: the repository and up to two labels with the `+N` indicator. One
/// line — the repository truncates first, then the chips — because the author,
/// branches, checks, review state and diff stats all live in the detail pane.
fn github_row_meta(url: &str, labels: &[gh::GhLabel], theme: Theme) -> AnyElement {
    div()
        .debug_selector(|| "github-row-meta".into())
        .min_w_0()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .overflow_hidden()
        .children(github_repo(url).map(|repo| github_repo_label(repo, theme)))
        .children(label_summary(labels, 2, theme))
        .into_any_element()
}
/// Extract the repository from a GitHub item URL without fabricating a source.
fn github_repo(url: &str) -> Option<&str> {
    let path = url.split_once("://")?.1.split_once('/')?.1;
    let end = path.match_indices('/').nth(1)?.0;
    let repo = &path[..end];
    (!repo.starts_with('/') && !repo.ends_with('/')).then_some(repo)
}

fn github_repo_label(repo: &str, theme: Theme) -> AnyElement {
    div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_size(TextSize::XSmall.px(&theme))
        .text_color(theme.text_muted)
        .child(repo.to_owned())
        .into_any_element()
}

/// The pieces of a GitHub list row that differ between an issue and a pull
/// request. The anatomy itself — three lines, the truncation and the GitHub
/// action — is shared, so both lists read the same way.
struct GithubRowParts<'a> {
    /// Leading half of the row id (`git-issue-3`), keeping the two lists apart.
    kind: &'static str,
    number: u64,
    glyph: &'static str,
    color: Hsla,
    state: String,
    /// The compact draft marker on the header line. Only pull requests use it.
    draft: bool,
    time: String,
    title: &'a str,
    url: &'a str,
    labels: &'a [gh::GhLabel],
    /// Primary ink for an open item, secondary for a closed one.
    emphasized: bool,
}

/// One compact GitHub list row: three lines at any normal width. Line one is
/// state, number, time and the GitHub link; line two is the title in a single
/// ellipsized run; line three is the repository and labels. Everything else —
/// author, branches, review and check state, diff stats, comments — is in the
/// detail pane, so a row's height never depends on how much metadata an item
/// carries.
fn github_list_row<P>(
    parts: GithubRowParts<'_>,
    selected: bool,
    theme: Theme,
    on_click: P,
) -> AnyElement
where
    P: Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
{
    div()
        .id(gpui::ElementId::Name(
            format!("git-{}-{}", parts.kind, parts.number).into(),
        ))
        .debug_selector(|| "github-row".into())
        .group("git-row")
        .px(DynamicSpacing::Base08.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .rounded(Radius::Large.px(&theme))
        .border_1()
        .border_color(gpui::transparent_black())
        .flex()
        .flex_col()
        // Children stretch: a column that aligns them to the start also makes
        // their percentage widths indefinite, which zeroes the text runs.
        .gap(DynamicSpacing::Base04.px(&theme))
        // A title longer than the pane must stop at the row's own box. GPUI
        // only ellipsizes a run it measured at a definite width, which a flex
        // row does not provide, so the row clips as well as the run does.
        .overflow_hidden()
        .cursor_pointer()
        .hover_state(&theme)
        .when(selected, |row| {
            row.bg(theme.surface_selected).border_color(theme.border)
        })
        .on_click(on_click)
        .child(github_row_header(&parts, theme))
        .child(github_row_title(parts.title, parts.emphasized, theme))
        .child(github_row_meta(parts.url, parts.labels, theme))
        .into_any_element()
}

/// One issue row in the list: state, title, repository and labels.
fn issue_row(
    issue: &gh::GhIssue,
    selected: bool,
    theme: Theme,
    cx: &Context<GitPanel>,
) -> AnyElement {
    let number = issue.number;
    let open = issue.is_open();
    github_list_row(
        GithubRowParts {
            kind: "issue",
            number,
            glyph: if open {
                "icons/circle-dot.svg"
            } else {
                "icons/circle-check.svg"
            },
            color: if open { theme.success } else { theme.error },
            state: tr!("git_panel.issue_kind"),
            draft: false,
            time: gh::relative_time(&issue.updated_at),
            title: &issue.title,
            url: &issue.url,
            labels: &issue.labels,
            emphasized: open,
        },
        selected,
        theme,
        cx.listener(move |this, _: &ClickEvent, _, cx| this.open_issue(number, cx)),
    )
}

/// A quiet GitHub label badge. GitHub owns the small status dot's colour; the
/// badge itself stays on the theme's neutral ramp so labels never overpower
/// the title or compete with issue/PR state.
fn issue_label_chip(label: &gh::GhLabel, theme: Theme) -> AnyElement {
    div()
        .h(px(18.))
        .max_w(px(200.))
        .min_w_0()
        .px(DynamicSpacing::Base06.px(&theme))
        .rounded(Radius::Small.px(&theme))
        .bg(theme.surface_hover)
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme))
        .text_size(TextSize::XSmall.px(&theme))
        .text_color(theme.text_secondary)
        .child(
            div()
                .size(px(6.))
                .flex_none()
                .rounded_full()
                .bg(label_color(&label.color)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(label.name.clone()),
        )
        .into_any_element()
}

/// The same label as [`issue_label_chip`], capped for a one-line list meta
/// row. The detail pane allows a longer, still bounded label name.
fn list_label_chip(label: &gh::GhLabel, theme: Theme) -> AnyElement {
    div()
        .h(px(18.))
        // Sized to the label, not to a cap that shears ordinary names: GPUI
        // only ellipsizes a run it measured at a definite width, so a cap
        // narrower than the word cuts its last glyph in half. The row itself
        // clips an unusually long one, and a shrink floor keeps it in bounds.
        .max_w(px(132.))
        .min_w(px(44.))
        .px(DynamicSpacing::Base06.px(&theme))
        .rounded(Radius::Small.px(&theme))
        .bg(theme.surface_hover)
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme))
        .text_size(TextSize::XSmall.px(&theme))
        .text_color(theme.text_muted)
        .child(
            div()
                .size(px(6.))
                .flex_none()
                .rounded_full()
                .bg(label_color(&label.color)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .child(label.name.clone()),
        )
        .into_any_element()
}

/// A 6-digit hex label color (`d73a4a`) as an `Hsla`.
fn label_color(hex: &str) -> Hsla {
    let value = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0x80_80_80);
    Hsla::from(gpui::rgba((value << 8) | 0xff))
}

/// A pill chip for an issue/PR state: an optional leading glyph (circle-dot,
/// check, merge…) plus the label, the way GitHub marks its states.
fn state_chip(label: &str, color: Hsla, glyph: Option<&'static str>, theme: Theme) -> AnyElement {
    div()
        .h(px(20.))
        .px(DynamicSpacing::Base08.px(&theme))
        .rounded(Radius::Small.px(&theme))
        .bg(color.opacity(0.15))
        .border_1()
        .border_color(color.opacity(0.5))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(&theme))
        .text_size(TextSize::XSmall.px(&theme))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color)
        .children(glyph.map(|path| icon(path, IconSize::Indicator.px(&theme), color)))
        .child(label.to_string())
        .into_any_element()
}

fn switch_comment_draft(
    input: &Entity<crate::composer::ComposerInput>,
    owner: &mut Option<u64>,
    drafts: &mut HashMap<u64, String>,
    number: u64,
    cx: &mut Context<GitPanel>,
) {
    if *owner == Some(number) {
        return;
    }
    if let Some(previous) = *owner {
        let text = input.read(cx).text();
        if text.is_empty() {
            drafts.remove(&previous);
        } else {
            drafts.insert(previous, text);
        }
    }
    let draft = drafts.remove(&number).unwrap_or_default();
    input.update(cx, |field, cx| field.set_text(draft, cx));
    *owner = Some(number);
}

/// Replace a `ComposerInput`'s whole contents in one edit, so a generated
/// draft overwrites whatever the user had typed.
fn set_composer_text(
    input: &Entity<crate::composer::ComposerInput>,
    text: &str,
    cx: &mut Context<GitPanel>,
) {
    let len = input.read(cx).text().len();
    input.update(cx, |field, cx| field.replace_range(0..len, text, cx));
}

/// Prepend a template's title prefix (`bug: `) to a generated title, unless the
/// title already carries it.
fn with_title_prefix(prefix: &Option<String>, title: &str) -> String {
    let Some(prefix) = prefix.as_deref().filter(|value| !value.trim().is_empty()) else {
        return title.to_string();
    };
    let bare = prefix.trim_end();
    if title.starts_with(bare) {
        title.to_string()
    } else {
        format!("{prefix}{title}")
    }
}

/// A bordered field wrapper for a `ComposerInput` (issue title/body/comment).
/// `min_h` is a multi-line field's taller minimum; below the input frame's
/// own minimum it has no effect.
fn composer_field(
    theme: Theme,
    input: Entity<crate::composer::ComposerInput>,
    min_h: Pixels,
) -> AnyElement {
    items_stretch(input_field_frame(div(), &theme))
        .w_full()
        .min_w_0()
        .min_h(min_h.max(input::min_height(&theme)))
        .bg(theme.surface_secondary)
        // The column wrapper lets the input grow to the field's min height, so
        // the whole text area is clickable, not just the first line.
        .child(div().flex_1().min_w_0().flex().flex_col().child(input))
        .into_any_element()
}

/// `input_field_frame` centers a one-line input. A multi-line field stretches
/// its input column to the box instead — the flex default the frame
/// overrides, which GPUI 0.2.2 has no `items_stretch` for — so the whole
/// field takes clicks, not just its first line.
fn items_stretch<E: Styled>(mut el: E) -> E {
    el.style().align_items = Some(gpui::AlignItems::Stretch);
    el
}

/// A filter-bar search field: a leading glyph, the input, and the submit
/// affordance inside one frame. Keeping submit in the frame (rather than beside
/// it) leaves the input the row's remaining width, so its placeholder never
/// clips to a fragment in a narrow list pane.
fn search_field(
    theme: Theme,
    input: Entity<crate::composer::ComposerInput>,
    submit_id: &'static str,
    submit_tip: &'static str,
    on_submit: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let tip = tr!(submit_tip);
    div()
        .debug_selector(|| "github-search-field".into())
        .flex_1()
        .min_w_0()
        .h(ButtonSize::Medium.height(&theme))
        .pl(input::padding_x(&theme))
        .pr(DynamicSpacing::Base02.px(&theme))
        .rounded(input::RADIUS.px(&theme))
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_secondary)
        .flex()
        .items_center()
        .gap(input::gap(&theme))
        .overflow_hidden()
        .text_size(TextSize::Small.px(&theme))
        .child(icon(
            "icons/search.svg",
            input::ICON.px(&theme),
            theme.text_muted,
        ))
        .child(div().flex_1().min_w_0().child(input))
        .child(
            press(
                icon_button_frame(div().id(submit_id), &theme, ButtonSize::Compact)
                    .group(BUTTON_GROUP)
                    .cursor_pointer()
                    .hover_state(&theme),
            )
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tip.clone())).into())
            .on_click(on_submit)
            .child(icon(
                "icons/arrow-right.svg",
                ButtonSize::Compact.icon_size().px(&theme),
                theme.text_muted,
            )),
        )
}

/// One comment in the discussion: an author line followed by full Markdown.
/// Hairlines and whitespace separate entries; no nested elevated card adds
/// another surface competing with the message.
fn issue_comment_card(comment: &gh::GhComment, theme: Theme) -> AnyElement {
    let author = comment
        .author
        .as_ref()
        .map(|user| user.login.clone())
        .unwrap_or_default();
    let has_body = !comment.body.trim().is_empty();
    div()
        .debug_selector(|| "github-comment".into())
        .w_full()
        .min_w_0()
        .pt(DynamicSpacing::Base16.px(&theme))
        .border_t_1()
        .border_color(theme.border_subtle)
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(author_avatar(&author, "", theme))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(author),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(gh::relative_time(&comment.created_at)),
                ),
        )
        .when(has_body, |card| {
            card.child(div().w_full().min_w_0().child(
                crate::transcript_view::render_markdown_document(&comment.body, theme),
            ))
        })
        .into_any_element()
}

// ── Pull requests ───────────────────────────────────────────────────

/// One PR row in the list: state, title, repository and labels — the same
/// three lines as an issue row, plus the compact draft marker on the header
/// line. Review state, checks, diff stats and branches are in the detail pane.
fn pr_row(pull: &gh::GhPull, selected: bool, theme: Theme, cx: &Context<GitPanel>) -> AnyElement {
    let number = pull.number;
    // Same glyph logic as the detail header, so a merged PR reads as merged
    // here too instead of always drawing the open-PR glyph recoloured.
    let glyph = if pull.is_merged() {
        "icons/git-merge.svg"
    } else if pull.is_open() {
        "icons/git-pull-request.svg"
    } else {
        "icons/circle-x.svg"
    };
    let color = if pull.is_merged() {
        theme.accent_text
    } else if pull.is_open() {
        theme.success
    } else {
        theme.error
    };
    github_list_row(
        GithubRowParts {
            kind: "pr",
            number,
            glyph,
            color,
            state: tr!("git_panel.pull_request_kind"),
            draft: pull.is_draft,
            time: gh::relative_time(&pull.updated_at),
            title: &pull.title,
            url: &pull.url,
            labels: &pull.labels,
            emphasized: pull.is_open(),
        },
        selected,
        theme,
        cx.listener(move |this, _: &ClickEvent, _, cx| this.open_pull(number, cx)),
    )
}

/// A chip for the PR's overall CI state, or `None` when it has no checks.
fn check_bucket_chip(bucket: Option<gh::CheckBucket>, theme: Theme) -> Option<AnyElement> {
    let bucket = bucket?;
    let (label, color, glyph) = check_status_parts(bucket, theme);
    Some(state_chip(&label, color, Some(glyph), theme))
}

/// The words, semantic colour and glyph for a check. Detail rows use these
/// directly so a full status pill does not consume the check name's width.
fn check_status_parts(bucket: gh::CheckBucket, theme: Theme) -> (String, Hsla, &'static str) {
    match bucket {
        gh::CheckBucket::Passed => (
            tr!("git_panel.checks_passed"),
            theme.success,
            "icons/circle-check.svg",
        ),
        gh::CheckBucket::Failed => (
            tr!("git_panel.checks_failed"),
            theme.error,
            "icons/circle-x.svg",
        ),
        gh::CheckBucket::Pending => (
            tr!("git_panel.checks_pending"),
            theme.warning,
            "icons/clock.svg",
        ),
        gh::CheckBucket::Skipped => (
            tr!("git_panel.checks_skipped"),
            theme.text_muted,
            "icons/minus.svg",
        ),
        gh::CheckBucket::Unknown => (
            tr!("git_panel.checks_pending"),
            theme.text_muted,
            "icons/info.svg",
        ),
    }
}

/// Merge readiness from the fields already present on the list response.
/// This stays visible even when the optional detail/timeline request fails.
fn merge_state_summary(pull: &gh::GhPull, theme: Theme) -> Option<AnyElement> {
    let state = pull.merge_state_status.as_deref().unwrap_or("");
    let conflicting = pull.mergeable.as_deref() == Some("CONFLICTING") || state == "DIRTY";
    let (label, color, glyph) = if conflicting {
        (
            tr!("git_panel.merge_conflicts"),
            theme.error,
            "icons/circle-x.svg",
        )
    } else {
        match state {
            "CLEAN" => (
                tr!("git_panel.merge_ready"),
                theme.success,
                "icons/circle-check.svg",
            ),
            "BEHIND" => (
                tr!("git_panel.merge_behind"),
                theme.warning,
                "icons/arrow-down.svg",
            ),
            "BLOCKED" => (
                tr!("git_panel.merge_blocked"),
                theme.warning,
                "icons/stop.svg",
            ),
            "UNSTABLE" => (
                tr!("git_panel.merge_unstable"),
                theme.warning,
                "icons/info.svg",
            ),
            _ => return None,
        }
    };
    Some(
        div()
            .debug_selector(|| "github-merge-state".into())
            .h(px(20.))
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .text_size(TextSize::XSmall.px(&theme))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(icon(glyph, IconSize::Indicator.px(&theme), color))
            .child(label)
            .into_any_element(),
    )
}

/// A chip for the PR's review decision, or `None` when GitHub has none.
fn review_chip(pull: &gh::GhPull, theme: Theme) -> Option<AnyElement> {
    let decision = pull.review_decision.as_deref()?;
    let (label, color, glyph) = match decision {
        "APPROVED" => (
            tr!("git_panel.review_approved"),
            theme.success,
            "icons/circle-check.svg",
        ),
        "CHANGES_REQUESTED" => (
            tr!("git_panel.review_changes"),
            theme.error,
            "icons/circle-x.svg",
        ),
        "REVIEW_REQUIRED" => (
            tr!("git_panel.review_required"),
            theme.warning,
            "icons/eye.svg",
        ),
        _ => return None,
    };
    Some(state_chip(&label, color, Some(glyph), theme))
}

/// One submitted review: author header with a state chip and optional body.
fn review_card(review: &gh::GhReview, theme: Theme) -> AnyElement {
    let author = review
        .author
        .as_ref()
        .map(|user| user.login.clone())
        .unwrap_or_default();
    let (label, color, glyph) = match review.state.as_str() {
        "APPROVED" => (
            tr!("git_panel.review_approved"),
            theme.success,
            "icons/circle-check.svg",
        ),
        "CHANGES_REQUESTED" => (
            tr!("git_panel.review_changes"),
            theme.error,
            "icons/circle-x.svg",
        ),
        _ => (
            tr!("git_panel.review_commented"),
            theme.text_muted,
            "icons/chat.svg",
        ),
    };
    let has_body = !review.body.trim().is_empty();
    div()
        .debug_selector(|| "github-review".into())
        .w_full()
        .min_w_0()
        .pt(DynamicSpacing::Base16.px(&theme))
        .border_t_1()
        .border_color(theme.border_subtle)
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(
            div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(author_avatar(&author, "", theme))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(author),
                )
                .child(state_chip(&label, color, Some(glyph), theme))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(gh::relative_time(&review.submitted_at)),
                ),
        )
        .when(has_body, |card| {
            card.child(div().w_full().min_w_0().child(
                crate::transcript_view::render_markdown_document(&review.body, theme),
            ))
        })
        .into_any_element()
}

/// A small uppercase section header inside a bordered card.
fn section_label(theme: Theme, label: &str) -> AnyElement {
    div()
        .px(DynamicSpacing::Base12.px(&theme))
        .py(DynamicSpacing::Base06.px(&theme))
        .border_b_1()
        .border_color(theme.border)
        .text_size(TextSize::XSmall.px(&theme))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_muted)
        .child(label.to_uppercase())
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
        .gap(DynamicSpacing::Base06.px(&theme))
        .text_size(TextSize::Small.px(&theme))
        .child(author_avatar(&commit.author, &commit.author_email, theme))
        .child(
            div()
                .text_color(theme.text_secondary)
                .child(commit.author.clone()),
        )
        .child(
            div()
                .size(px(3.))
                .rounded_full()
                .bg(theme.text_muted.opacity(0.7)),
        )
        .child(
            div()
                .text_color(theme.text_muted)
                .child(commit.relative.clone()),
        );

    let mut element = div()
        .id(gpui::ElementId::Name(
            format!("git-graph-{}", commit.short).into(),
        ))
        .group("git-row")
        .mx(DynamicSpacing::Base12.px(&theme))
        .px(DynamicSpacing::Base08.px(&theme))
        .py(DynamicSpacing::Base06.px(&theme))
        .rounded(Radius::Medium.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .cursor_pointer()
        .hover_state(&theme)
        .when(selected, |s| s.bg(theme.surface_selected))
        .on_click(listener)
        .child(graph_gutter(row, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base03.px(&theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
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
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_primary)
                                .child(commit.subject.clone()),
                        ),
                )
                .child(meta),
        )
        .child(
            div()
                .flex_none()
                .font_family(theme::code_font_family())
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(commit.short.clone()),
        );
    if let Some(url) = url.map(str::to_string) {
        element = element.child(
            div()
                .flex_none()
                .opacity(0.)
                .group_hover("git-row", |s| s.opacity(1.))
                .child(row_button(
                    "git-graph-open",
                    "icons/arrow-up-right.svg",
                    "git_panel.open_on_github",
                    theme,
                    move |_, _, cx| cx.open_url(&url),
                )),
        );
    }
    element
        .child(icon(
            if selected {
                "icons/arrow-down.svg"
            } else {
                "icons/arrow-right.svg"
            },
            IconSize::XSmall.px(&theme),
            theme.text_muted,
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
        .gap(DynamicSpacing::Base06.px(&theme))
        .text_size(TextSize::Small.px(&theme))
        .child(author_avatar(&commit.author, &commit.author_email, theme))
        .child({
            // Clicking the author filters History to their commits.
            let author_for_click = author.clone();
            div()
                .id(gpui::ElementId::Name(
                    format!("git-commit-author-{}", commit.short).into(),
                ))
                .text_color(theme.text_secondary)
                .cursor_pointer()
                .hover_ink_accent(&theme)
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
                .bg(theme.text_muted.opacity(0.7)),
        )
        .child(
            div()
                .text_color(theme.text_muted)
                .child(commit.relative.clone()),
        );

    let mut row = div()
        .id(gpui::ElementId::Name(
            format!("git-commit-{}", commit.short).into(),
        ))
        .mx(DynamicSpacing::Base12.px(&theme))
        .px(DynamicSpacing::Base08.px(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .rounded(Radius::Medium.px(&theme))
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .cursor_pointer()
        .hover_state(&theme)
        .when(selected, |row| row.bg(theme.surface_selected))
        .on_click(listener)
        .child(history_marker(theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base03.px(&theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
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
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_primary)
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
                .gap(DynamicSpacing::Base04.px(&theme))
                .child(
                    div()
                        .font_family(theme::code_font_family())
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(commit.short.clone()),
                ),
        );
    // Opening the commit on the forge is a separate affordance from expanding
    // its detail, so it gets its own button that stops propagation.
    if let Some(url) = url.map(str::to_string) {
        row = row.child(row_button(
            "git-commit-open",
            "icons/arrow-up-right.svg",
            "git_panel.open_on_github",
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
        IconSize::XSmall.px(&theme),
        theme.text_muted,
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
        .bg(theme.surface_elevated)
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .justify_center()
        .child(icon(
            "icons/git-commit.svg",
            IconSize::Small.px(&theme),
            theme.text_muted,
        ))
        .into_any_element()
}

/// A small round avatar shown before the author name: the real GitHub photo
/// when the author's noreply email or name identifies a handle, with the
/// monogram as its fallback (while loading, for accounts with no photo, or for
/// real names that aren't handles). Never leaves a blank slot.
fn author_avatar(name: &str, email: &str, theme: Theme) -> AnyElement {
    avatar_image(github_avatar_url(name, email), name, email, theme)
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

/// The GitHub avatar URL for a commit author. A user-supplied email→login
/// override wins first (see [`avatar_login_overrides`]); then the login encoded
/// in a GitHub noreply email (`123+login@users.noreply.github.com`) is
/// authoritative over the display name; only when neither is available does a
/// name that already looks like a handle get used. This keeps display names
/// ("Ada Lovelace") from 404-ing and avoids showing the photo of an unrelated
/// account whose handle happens to equal the display name.
///
/// Resolves against GitHub's public profile image endpoint,
/// `https://github.com/<login>.png`, rather than the CDN host directly: it is
/// the documented, stable form and redirects to the same avatar, so every git
/// page (Changes, Commit, Issues, pull requests) shows a live photo.
fn github_avatar_url(name: &str, email: &str) -> Option<String> {
    let handle = resolve_github_login(name, email, avatar_login_overrides())?;
    Some(format!("https://github.com/{handle}.png?size=48"))
}

/// Resolve the GitHub login for an author, consulting the override map first.
/// Split from [`github_avatar_url`] so the map lookup is testable without
/// touching the user's home directory.
fn resolve_github_login(
    name: &str,
    email: &str,
    overrides: &HashMap<String, String>,
) -> Option<String> {
    if let Some(login) = overrides.get(&email.trim().to_ascii_lowercase()) {
        return Some(login.clone());
    }
    github_handle_from_email(email)
        .or_else(|| is_github_handle(name).then(|| name.trim().to_string()))
}

/// Email→login overrides from `~/.orbit-pi/git-avatars.json`, a flat
/// `{ "you@example.com": "github-login" }` map. Commits authored with a
/// personal (non-noreply) address carry no login, so this lets their rows show
/// the real photo instead of the monogram. Read once; a missing or malformed
/// file is simply ignored, and invalid login shapes are dropped.
fn avatar_login_overrides() -> &'static HashMap<String, String> {
    static OVERRIDES: OnceLock<HashMap<String, String>> = OnceLock::new();
    OVERRIDES.get_or_init(|| {
        let Some(home) = crate::platform::home_dir_opt() else {
            return HashMap::new();
        };
        let Ok(raw) = std::fs::read_to_string(home.join(".orbit-pi").join("git-avatars.json"))
        else {
            return HashMap::new();
        };
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&raw)
        else {
            return HashMap::new();
        };
        map.into_iter()
            .filter_map(|(email, login)| {
                let login = login.as_str()?;
                is_github_handle(login)
                    .then(|| (email.trim().to_ascii_lowercase(), login.to_string()))
            })
            .collect()
    })
}

/// The GitHub login from a noreply email, if the address is one. Modern
/// addresses are `12345+login@users.noreply.github.com`; legacy ones are plain
/// `login@users.noreply.github.com`.
fn github_handle_from_email(email: &str) -> Option<String> {
    let (local, domain) = email.trim().rsplit_once('@')?;
    if !domain.eq_ignore_ascii_case("users.noreply.github.com") {
        return None;
    }
    // The numeric segment before '+' is GitHub's account id, never the handle.
    let handle = local.rsplit_once('+').map_or(local, |(_, handle)| handle);
    is_github_handle(handle).then(|| handle.to_string())
}

/// Whether a string is shaped like a GitHub username (alphanumeric or single
/// hyphens).
fn is_github_handle(candidate: &str) -> bool {
    let handle = candidate.trim();
    !handle.is_empty()
        && handle.len() <= 39
        && !handle.starts_with('-')
        && !handle.ends_with('-')
        && !handle.contains("--")
        && handle
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
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

/// An icon-only action for the embedded review's file header. Stops
/// propagation so the header's collapse toggle never also fires.
fn review_action_icon(
    id: String,
    icon_path: &'static str,
    tip_key: &'static str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let label = tr!(tip_key);
    icon_button_frame(
        div().id(gpui::ElementId::Name(id.into())),
        &theme,
        ButtonSize::Default,
    )
    .group(BUTTON_GROUP)
    .cursor_pointer()
    .hover_state(&theme)
    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into())
    .on_click(move |event, window, cx| {
        cx.stop_propagation();
        listener(event, window, cx);
    })
    .child(icon(
        icon_path,
        ButtonSize::Default.icon_size().px(&theme),
        theme.text_muted,
    ))
    .into_any_element()
}

/// An icon-only bulk action on the embedded review's grouped strips — Stage
/// all / Unstage all / Revert all — wearing the same bare icon frame as the
/// strip's view toggles, with its label moved to the tooltip.
fn review_toolbar_button(
    id: impl Into<gpui::ElementId>,
    icon_path: &'static str,
    label: String,
    color: Hsla,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    icon_button_frame(div().id(id), &theme, ButtonSize::Medium)
        .group(BUTTON_GROUP)
        .cursor_pointer()
        .hover_state(&theme)
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into())
        .on_click(listener)
        .child(icon(
            icon_path,
            ButtonSize::Medium.icon_size().px(&theme),
            color,
        ))
        .into_any_element()
}

/// The Changes-tab section heading: a sentence-case label with the count set
/// inline as quiet metadata (no count pill), the quieter head the reference
/// uses.
fn section_title(label: &str, count: usize, theme: Theme) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(label.to_string()),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(count.to_string()),
        )
        .into_any_element()
}

/// A status letter on a softly tinted tile: the letter is the signal, the
/// semantic color only washes it (paired, never color alone). Shared by the
/// working-tree rows, the commit-detail files, and a PR's changed files.
fn letter_tile(letter: char, color: Hsla, theme: Theme) -> AnyElement {
    div()
        .size(px(18.))
        .flex_none()
        .rounded(Radius::Small.px(&theme))
        .bg(color.opacity(0.12))
        .flex()
        .items_center()
        .justify_center()
        .text_size(TextSize::XSmall.px(&theme))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(letter.to_string())
        .into_any_element()
}

/// A compact diffstat pair (`+N` / `−N`) in the semantic add/delete colors.
/// Right-aligned fixed slots keep the columns from jumping as counts change
/// width, and a zero side dims instead of vanishing — a zero is a real fact,
/// not missing data. Shared by the Changes headers and rows, the commit-detail
/// files, and the pull-request diffstats, so the same numbers always look
/// identical no matter which tab renders them.
fn delta_stats(additions: u64, deletions: u64, theme: Theme) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .text_size(TextSize::Small.px(&theme))
        .child(
            div()
                .min_w(px(26.))
                .text_align(gpui::TextAlign::Right)
                .text_color(if additions > 0 {
                    theme.success
                } else {
                    theme.text_muted
                })
                .child(format!("+{additions}")),
        )
        .child(
            div()
                .min_w(px(26.))
                .text_align(gpui::TextAlign::Right)
                .text_color(if deletions > 0 {
                    theme.error
                } else {
                    theme.text_muted
                })
                .child(format!("-{deletions}")),
        )
        .into_any_element()
}

/// A hairline between History rows, inset to the row content the way a commit
/// list separates entries without boxing each one.
fn commit_separator(theme: Theme) -> AnyElement {
    // 12 (row margin) + 8 (row padding) + leading marker + 10 (row gap).
    div()
        .ml(px(12. + 8. + HISTORY_LEADING + 10.))
        .mr(DynamicSpacing::Base20.px(&theme))
        .h(px(1.))
        .bg(theme.border)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn github_drafts_follow_items_and_reset_with_workspace(cx: &mut gpui::TestAppContext) {
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        cx.update(|cx| {
            panel.update(cx, |panel, cx| {
                for is_issue in [true, false] {
                    let (input, owner, drafts) = if is_issue {
                        (
                            &panel.issue_comment,
                            &mut panel.issue_comment_owner,
                            &mut panel.issue_comment_drafts,
                        )
                    } else {
                        (
                            &panel.pr_comment,
                            &mut panel.pr_comment_owner,
                            &mut panel.pr_comment_drafts,
                        )
                    };
                    switch_comment_draft(input, owner, drafts, 1, cx);
                    set_composer_text(input, "draft one", cx);
                    switch_comment_draft(input, owner, drafts, 2, cx);
                    assert_eq!(input.read(cx).text(), "");
                    set_composer_text(input, "draft two", cx);
                    switch_comment_draft(input, owner, drafts, 1, cx);
                    assert_eq!(input.read(cx).text(), "draft one");
                    switch_comment_draft(input, owner, drafts, 1, cx);
                    assert_eq!(input.read(cx).text(), "draft one");
                    switch_comment_draft(input, owner, drafts, 2, cx);
                    assert_eq!(input.read(cx).text(), "draft two");
                }
                panel.set_context(
                    Some(PathBuf::from("/different-workspace")),
                    String::new(),
                    String::new(),
                    None,
                    Vec::new(),
                    cx,
                );
                assert!(panel.issue_comment.read(cx).text().is_empty());
                assert!(panel.pr_comment.read(cx).text().is_empty());
                assert!(panel.issue_comment_drafts.is_empty());
                assert!(panel.pr_comment_drafts.is_empty());
            })
        });
    }

    #[gpui::test]
    fn switching_comment_drafts_clears_undo_history(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let panel = cx.update(|_, cx| {
            cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit));
            cx.new(GitPanel::new)
        });
        let input = cx.update(|_, cx| panel.read(cx).issue_comment.clone());
        cx.update(|_, cx| {
            panel.update(cx, |panel, cx| {
                switch_comment_draft(
                    &panel.issue_comment,
                    &mut panel.issue_comment_owner,
                    &mut panel.issue_comment_drafts,
                    1,
                    cx,
                );
                set_composer_text(&panel.issue_comment, "draft one", cx);
                switch_comment_draft(
                    &panel.issue_comment,
                    &mut panel.issue_comment_owner,
                    &mut panel.issue_comment_drafts,
                    2,
                    cx,
                );
            });
        });
        cx.draw(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(500.), px(400.)),
            |_, _| input.clone(),
        );
        cx.update(|window, cx| input.read(cx).focus_handle(cx).focus(window));
        cx.draw(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(500.), px(400.)),
            |_, _| input.clone(),
        );
        cx.update(|_, cx| input.update(cx, |input, cx| input.replace_range(0..0, "new", cx)));
        cx.dispatch_action(crate::Undo);
        assert_eq!(cx.update(|_, cx| input.read(cx).text()), "");
        let revision = cx.update(|_, cx| input.read(cx).revision());
        cx.dispatch_action(crate::Undo);
        assert_eq!(cx.update(|_, cx| input.read(cx).text()), "");
        assert_eq!(
            cx.update(|_, cx| input.read(cx).revision()),
            revision,
            "a leftover undo snapshot would restore the other item's draft"
        );
    }

    #[gpui::test]
    fn label_dismissal_only_blocks_the_same_trigger(cx: &mut gpui::TestAppContext) {
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        cx.update(|cx| {
            panel.update(cx, |panel, cx| {
                for (dismissed, opposite) in [
                    (LabelMenuTarget::Filter, LabelMenuTarget::Issue),
                    (LabelMenuTarget::Issue, LabelMenuTarget::Filter),
                ] {
                    panel.label_menu = None;
                    panel.label_menu_dismissed = Some((dismissed, Instant::now()));
                    panel.toggle_label_menu(dismissed, cx);
                    assert_eq!(panel.label_menu, None);

                    panel.label_menu_dismissed = Some((dismissed, Instant::now()));
                    panel.toggle_label_menu(opposite, cx);
                    assert_eq!(panel.label_menu, Some(opposite));

                    panel.label_menu = None;
                    panel.label_menu_dismissed =
                        Some((dismissed, Instant::now() - Duration::from_secs(1)));
                    panel.toggle_label_menu(dismissed, cx);
                    assert_eq!(panel.label_menu, Some(dismissed));
                }
            });
        });
    }

    #[gpui::test]
    fn github_new_forms_cancel_detail_loading_and_label_menu_is_targeted(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        cx.update(|cx| {
            panel.update(cx, |panel, cx| {
                panel.label_menu = Some(LabelMenuTarget::Filter);
                let theme = *theme::get(cx);
                assert!(panel
                    .label_popup(theme, LabelMenuTarget::Filter, cx)
                    .is_some());
                assert!(panel
                    .label_popup(theme, LabelMenuTarget::Issue, cx)
                    .is_none());
                panel.label_menu = Some(LabelMenuTarget::Issue);
                assert!(panel
                    .label_popup(theme, LabelMenuTarget::Issue, cx)
                    .is_some());
                assert!(panel
                    .label_popup(theme, LabelMenuTarget::Filter, cx)
                    .is_none());
                panel.selected_issue = Some(1);
                panel.issue_detail_loading = true;
                panel.open_issue_new(cx);
                assert!(!panel.issue_detail_loading);
                assert_eq!(panel.selected_issue, None);
                assert_eq!(panel.label_menu, None);
                panel.selected_pr = Some(2);
                panel.pr_detail_loading = true;
                panel.open_pull_new(cx);
                assert!(!panel.pr_detail_loading);
                assert_eq!(panel.selected_pr, None);
            })
        });
    }

    struct ConfirmPopupProbe(Entity<GitPanel>);

    impl Render for ConfirmPopupProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let popup = self
                .0
                .update(cx, |panel, cx| panel.confirm_popup(*theme::get(cx), cx));
            div().size_full().relative().children(popup)
        }
    }

    #[gpui::test]
    fn discard_confirmation_contains_actions_after_wrapped_content(cx: &mut gpui::TestAppContext) {
        use crate::theme::ThemeId;

        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        let cx = cx.add_empty_window();
        let mut short_card_height = px(0.);
        for (width, path) in [
            (800., "short.rs"),
            (
                800.,
                ".argent/recordings/screen-recording-43F6B111-4588-41BC-96D7-2A58D84E9BA1-1793269223730.mp4",
            ),
            (
                340.,
                ".argent/recordings/screen-recording-43F6B111-4588-41BC-96D7-2A58D84E9BA1-1793269223730.mp4",
            ),
        ] {
            cx.update(|_, cx| {
                panel.update(cx, |panel, cx| {
                    panel.pending_confirm = Some(PendingConfirm::DiscardPath(path.into()));
                    cx.notify();
                });
            });
            let _ = cx.draw(point(px(0.), px(0.)), size(px(width), px(600.)), |_, cx| {
                cx.new(|_| ConfirmPopupProbe(panel.clone()))
            });
            let card = cx.debug_bounds("git-confirm-card").unwrap();
            let title = cx.debug_bounds("git-confirm-title").unwrap();
            let body = cx.debug_bounds("git-confirm-body").unwrap();
            let actions = cx.debug_bounds("git-confirm-actions").unwrap();
            assert!(
                body.top() >= title.bottom(),
                "title {title:?}, body {body:?}"
            );
            assert!(
                actions.top() >= body.bottom(),
                "body {body:?}, actions {actions:?}"
            );
            assert!(
                actions.bottom() + px(16.) <= card.bottom(),
                "width {width}, card {card:?}, actions {actions:?}"
            );
            assert!(card.size.width <= px(width - 32.));
            if path == "short.rs" {
                short_card_height = card.size.height;
            } else {
                assert!(card.size.height > short_card_height);
            }
        }
    }

    struct GithubHeaderProbe;

    impl Render for GithubHeaderProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *theme::get(cx);
            let labels = [];
            github_row_header(
                &GithubRowParts {
                    kind: "issue",
                    number: u64::MAX,
                    glyph: "icons/circle-dot.svg",
                    color: theme.success,
                    state: "A very long translated issue state that must remain readable".into(),
                    draft: true,
                    time: "2h ago".into(),
                    title: "a title",
                    url: "https://github.com/orbit/app/issues/7",
                    labels: &labels,
                    emphasized: true,
                },
                theme,
            )
        }
    }

    /// Line one holds state, time and the GitHub action in one run: the state
    /// cluster truncates instead of pushing the time onto a second line, which
    /// is the extreme-scale fallback and not the normal case.
    #[gpui::test]
    fn github_header_keeps_state_time_and_link_on_one_line(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        for width in [208., 240., 320.] {
            cx.draw(point(px(0.), px(0.)), size(px(width), px(400.)), |_, cx| {
                cx.new(|_| GithubHeaderProbe)
            });
            let header = cx.debug_bounds("github-row-header").unwrap();
            let state = cx.debug_bounds("github-row-state").unwrap();
            let time = cx.debug_bounds("github-row-time").unwrap();
            let link = cx.debug_bounds("github-row-link").unwrap();
            assert!(state.right() <= header.right(), "{state:?} {header:?}");
            assert!(time.right() <= header.right(), "{time:?} {header:?}");
            assert!(link.right() <= header.right(), "{link:?} {header:?}");
            assert_eq!(
                state.center().y,
                time.center().y,
                "width {width}: the time shares the state's line"
            );
            assert_eq!(
                state.center().y,
                link.center().y,
                "width {width}: the GitHub action shares the state's line"
            );
            assert!(
                time.right() >= state.right(),
                "width {width}: the time sits right of the state cluster"
            );
            assert!(
                link.right() >= time.right(),
                "width {width}: the GitHub action sits right of the time"
            );
        }
    }

    /// Renders one row; `true` floods it with the worst content GitHub can
    /// hand a row.
    struct RowProbe(bool);

    impl Render for RowProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *theme::get(cx);
            let pathological = self.0;
            let long_title = format!(
                "{}\n\t second line that must not render \n  third line",
                "a title much longer than the list pane, so it has to be truncated ".repeat(3)
            );
            let long_labels = vec![
                gh::GhLabel {
                    name: "a label name long enough to be capped".into(),
                    color: "d73a4a".into(),
                    description: None,
                };
                4
            ];
            let short_labels = vec![gh::GhLabel {
                name: "bug".into(),
                color: "d73a4a".into(),
                description: None,
            }];
            let row = github_list_row(
                GithubRowParts {
                    kind: "issue",
                    number: if pathological { 12842 } else { 7 },
                    glyph: "icons/circle-dot.svg",
                    color: theme.success,
                    state: if pathological {
                        "A very long translated issue state that must remain readable".into()
                    } else {
                        "Open".into()
                    },
                    draft: pathological,
                    time: "2 days ago".into(),
                    title: if pathological {
                        &long_title
                    } else {
                        "Fix flaky test"
                    },
                    url: if pathological {
                        "https://github.com/a-very-long-owner-name/a-repository-longer-than-the-pane/issues/12842"
                    } else {
                        "https://github.com/orbit/app/issues/7"
                    },
                    labels: if pathological {
                        &long_labels
                    } else {
                        &short_labels
                    },
                    emphasized: true,
                },
                false,
                theme,
                |_, _, _| {},
            );
            div().w_full().child(row)
        }
    }

    /// A row's height is the same whatever it carries — a title with newlines, a
    /// long repository, four labels, a draft marker — because it is always
    /// exactly three lines. Height that followed content would be the fourth
    /// line the list does not allow.
    #[gpui::test]
    fn github_rows_stay_three_lines_for_any_content(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        for width in [240., 320.] {
            let mut heights = Vec::new();
            let mut title_heights = Vec::new();
            let mut title_widths = Vec::new();
            for pathological in [false, true] {
                cx.draw(point(px(0.), px(0.)), size(px(width), px(600.)), |_, cx| {
                    cx.new(|_| RowProbe(pathological))
                });
                let row = cx.debug_bounds("github-row").unwrap();
                let header = cx.debug_bounds("github-row-header").unwrap();
                let time = cx.debug_bounds("github-row-time").unwrap();
                let title = cx.debug_bounds("github-row-title").unwrap();
                let meta = cx.debug_bounds("github-row-meta").unwrap();
                assert_eq!(
                    header.size.height, time.size.height,
                    "width {width}: the header wrapped onto a second line"
                );
                assert!(title.size.width <= row.size.width, "{title:?} {row:?}");
                assert!(meta.top() >= title.bottom(), "{title:?} {meta:?}");
                heights.push(row.size.height);
                title_heights.push(title.size.height);
                title_widths.push(title.size.width);
            }
            assert_eq!(
                heights[0], heights[1],
                "width {width}: content changed the row's height"
            );
            assert_eq!(
                title_heights[0], title_heights[1],
                "width {width}: an embedded newline or a long title added a title line"
            );
            assert_eq!(
                title_widths[0], title_widths[1],
                "width {width}: the title's width is definite, not content-driven"
            );
        }
    }

    struct RowLinesProbe {
        title: String,
    }

    impl Render for RowLinesProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *theme::get(cx);
            let labels = [
                github_label("dependencies", "0366d6"),
                github_label("rust", "dea584"),
            ];
            let parts = GithubRowParts {
                kind: "pr",
                number: 113,
                glyph: "icons/git-pull-request.svg",
                color: theme.success,
                state: tr!("git_panel.pull_request_kind"),
                draft: false,
                time: "2d ago".into(),
                title: &self.title,
                url: "https://github.com/imrj05/orbit/pull/113",
                labels: &labels,
                emphasized: true,
            };
            div()
                .w_full()
                .child(github_list_row(parts, false, theme, |_, _, _| {}))
        }
    }

    /// Every line of a row fills the row's content box and stays inside it:
    /// a title measured at an indefinite width truncates to a bare ellipsis,
    /// and one that overruns paints under the pane edge. Both read as broken
    /// rows, so the three line boxes must agree on the row's width.
    #[gpui::test]
    fn github_rows_keep_three_full_width_lines_inside_the_row(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        for width in [240., 320., 420.] {
            for title in [
                "Fix the flaky test".to_string(),
                "chore(deps): bump the cargo group across 1 directory with 10 updates".repeat(3),
            ] {
                let _ = cx.draw(point(px(0.), px(0.)), size(px(width), px(600.)), |_, cx| {
                    cx.new(|_| RowLinesProbe {
                        title: title.clone(),
                    })
                });
                let row = cx.debug_bounds("github-row").unwrap();
                let header = cx.debug_bounds("github-row-header").unwrap();
                let line = cx.debug_bounds("github-row-title").unwrap();
                let meta = cx.debug_bounds("github-row-meta").unwrap();
                // One inset on both sides, and all three lines share it.
                let inset = line.left() - row.left();
                assert!(inset > px(0.), "width {width}: {line:?} {row:?}");
                assert_eq!(meta.left(), line.left(), "width {width}: {meta:?}");
                assert_eq!(header.left(), line.left(), "width {width}: {header:?}");
                for (name, bounds) in [("header", header), ("title", line), ("meta", meta)] {
                    assert!(
                        bounds.right() <= row.right(),
                        "width {width}: {name} runs past the row: {bounds:?} {row:?}"
                    );
                }
                assert!(
                    line.size.width >= row.size.width - inset * 2.,
                    "width {width}: title box is {} for a {} row",
                    line.size.width,
                    row.size.width
                );
            }
        }
    }

    #[test]
    fn github_titles_are_flattened_to_one_line() {
        assert_eq!(single_line("Fix  the\nflaky\ttest "), "Fix the flaky test");
        assert_eq!(single_line("\n\n"), "");
        assert_eq!(single_line("already one line"), "already one line");
    }

    #[derive(Clone, Copy)]
    enum GithubToolbarKind {
        Issues,
        Pulls,
    }

    struct GithubToolbarProbe {
        panel: Entity<GitPanel>,
        kind: GithubToolbarKind,
    }

    impl Render for GithubToolbarProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let toolbar = self.panel.update(cx, |panel, cx| {
                let theme = *theme::get(cx);
                match self.kind {
                    GithubToolbarKind::Issues => panel.issue_filter_bar(theme, cx),
                    GithubToolbarKind::Pulls => panel.pr_filter_bar(theme, cx),
                }
            });
            div().w_full().child(toolbar)
        }
    }

    /// Every tool stays inside its pane, with wrapping reserved for whole
    /// controls rather than clipped search text or an unreachable Create.
    #[gpui::test]
    fn github_toolbars_fit_narrow_and_wide_panes(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        panel.update(cx, |panel, _| {
            panel.issue_filter.label =
                Some("a-repository-label-long-enough-to-exercise-the-picker-cap".into());
        });
        let cx = cx.add_empty_window();
        for width in [240., 320., 420., 680.] {
            for kind in [GithubToolbarKind::Issues, GithubToolbarKind::Pulls] {
                panel.update(cx, |panel, _| {
                    // One pane: at this width the page has no split, so the
                    // list is the page and the toolbar sees its full width.
                    panel.page_width = px(width);
                    panel.github_list_width = px(width);
                });
                let _ = cx.draw(point(px(0.), px(0.)), size(px(width), px(480.)), |_, cx| {
                    cx.new(|_| GithubToolbarProbe {
                        panel: panel.clone(),
                        kind,
                    })
                });
                let bar = cx.debug_bounds("github-filter-bar").unwrap();
                let states = cx.debug_bounds("github-filter-states").unwrap();
                let tools = cx.debug_bounds("github-filter-tools").unwrap();
                let search = cx.debug_bounds("github-search-field").unwrap();
                let create = cx.debug_bounds("github-create").unwrap();
                assert!(bar.size.width <= px(width), "width {width}: {bar:?}");
                assert!(states.size.width <= bar.size.width, "{states:?} {bar:?}");
                assert!(search.right() <= tools.right(), "{search:?} {tools:?}");
                // The field owns the row's spare width: it is wide enough for
                // a readable query at the narrowest pane the split allows, so
                // its placeholder never reads as a fragment.
                assert!(
                    search.size.width >= px(120.),
                    "width {width}: {search:?} is too narrow"
                );
                assert!(create.right() <= tools.right(), "{create:?} {tools:?}");
                assert!(create.bottom() <= tools.bottom(), "{create:?} {tools:?}");
            }
        }
    }

    fn github_user(login: &str) -> gh::GhUser {
        gh::GhUser {
            login: login.into(),
            name: None,
            avatar_url: None,
        }
    }

    fn github_label(name: &str, color: &str) -> gh::GhLabel {
        gh::GhLabel {
            name: name.into(),
            color: color.into(),
            description: None,
        }
    }

    #[derive(Clone)]
    enum GithubDetailFixture {
        Issue(Box<gh::GhIssue>),
        Pull(Box<gh::GhPull>),
    }

    struct GithubDetailProbe {
        panel: Entity<GitPanel>,
        fixture: GithubDetailFixture,
    }

    impl Render for GithubDetailProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.panel.update(cx, |panel, cx| {
                let theme = *theme::get(cx);
                match &self.fixture {
                    GithubDetailFixture::Issue(issue) => {
                        panel.issue_detail_view(issue.as_ref(), theme, cx)
                    }
                    GithubDetailFixture::Pull(pull) => {
                        panel.pr_detail_view(pull.as_ref(), theme, cx)
                    }
                }
            })
        }
    }

    /// The bounded identity stays above the single document scroll surface.
    #[gpui::test]
    fn github_details_keep_header_and_content_inside_the_pane(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        let issue = gh::GhIssue {
            number: 148,
            title: "A detailed issue title that wraps across lines without crowding its actions or metadata".into(),
            state: "OPEN".into(),
            body: "## Summary\n\nThe whole Markdown description remains readable.\n\n- one\n- two\n\n```rust\nfn example() {}\n```".into(),
            author: github_user("a-long-author-login"),
            labels: vec![
                github_label("bug", "d73a4a"),
                github_label("a-long-label-that-wraps-as-a-badge", "0e8a16"),
            ],
            assignees: vec![github_user("reviewer")],
            comments: vec![gh::GhComment {
                author: Some(github_user("commenter")),
                body: "A **complete** comment with its author and time.".into(),
                created_at: "2026-01-02T00:00:00Z".into(),
                url: String::new(),
            }],
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            url: "https://github.com/orbit/a-repository-with-a-long-name/issues/148".into(),
        };
        let pull = gh::GhPull {
            number: 149,
            title: "A detailed pull request title that wraps without covering checks, branches or actions".into(),
            state: "OPEN".into(),
            is_draft: true,
            body: "## Changes\n\nDescription, checks and files all retain their own sections.".into(),
            author: github_user("pull-author"),
            labels: vec![github_label("ready-for-review", "0e8a16")],
            review_decision: Some("REVIEW_REQUIRED".into()),
            mergeable: Some("MERGEABLE".into()),
            merge_state_status: Some("CLEAN".into()),
            head_ref: "feature/a-long-but-readable-head-branch".into(),
            base_ref: "main".into(),
            checks: vec![gh::GhCheck {
                name: "macOS / build and test / a-long-check-name".into(),
                status: "COMPLETED".into(),
                conclusion: "SUCCESS".into(),
                details_url: "https://example.com/check".into(),
                ..Default::default()
            }],
            additions: 24,
            deletions: 4,
            changed_files: 1,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            url: "https://github.com/orbit/a-repository-with-a-long-name/pull/149".into(),
            comments: vec![gh::GhComment {
                author: Some(github_user("commenter")),
                body: "A PR comment.".into(),
                created_at: "2026-01-02T00:00:00Z".into(),
                url: String::new(),
            }],
            reviews: Vec::new(),
            commits: vec![gh::GhPullCommit {
                headline: "Keep the complete commit headline readable".into(),
                oid: "0123456789abcdef".into(),
                authors: vec![github_user("pull-author")],
            }],
            files: vec![gh::GhPullFile {
                path: "crates/orbit-pi/src/a-long-directory-name/file.rs".into(),
                additions: 24,
                deletions: 4,
                change_type: "MODIFIED".into(),
            }],
        };
        // Ensure every viewport has enough document to scroll the metadata away.
        let mut issue = issue;
        issue.body = issue.body.repeat(10);
        let mut pull = pull;
        pull.body = pull.body.repeat(10);
        let mut long_issue = issue.clone();
        long_issue.title = "An exceptionally long title stays bounded in the header ".repeat(40);
        let mut long_pull = pull.clone();
        long_pull.title = long_issue.title.clone();
        let cx = cx.add_empty_window();
        for (width, height) in [
            (400., 320.),
            (400., 720.),
            (560., 720.),
            (760., 720.),
            (1200., 720.),
        ] {
            for fixture in [
                GithubDetailFixture::Issue(Box::new(issue.clone())),
                GithubDetailFixture::Pull(Box::new(pull.clone())),
                GithubDetailFixture::Issue(Box::new(long_issue.clone())),
                GithubDetailFixture::Pull(Box::new(long_pull.clone())),
            ] {
                let is_pr = matches!(fixture, GithubDetailFixture::Pull(_));
                let probe = cx.update(|_, cx| {
                    cx.new(|_| GithubDetailProbe {
                        panel: panel.clone(),
                        fixture: fixture.clone(),
                    })
                });
                let draw = |cx: &mut gpui::VisualTestContext| {
                    let _ = cx.draw(
                        point(px(0.), px(0.)),
                        size(px(width), px(height)),
                        |_, _| {
                            div()
                                .w(px(width))
                                .h(px(height))
                                .flex()
                                .flex_col()
                                .child(probe.clone())
                        },
                    );
                };
                draw(cx);
                let header = cx.debug_bounds("github-detail-header").unwrap();
                let back = cx.debug_bounds("github-detail-back").unwrap();
                let pinned = cx.debug_bounds("github-detail-pinned-title").unwrap();
                let eyebrow = cx.debug_bounds("github-detail-eyebrow").unwrap();
                assert!(cx.debug_bounds("github-detail-title").is_none());
                let scroll = cx.debug_bounds("github-detail-scroll").unwrap();
                let description = cx.debug_bounds("github-description").unwrap();
                let agent_action = cx
                    .debug_bounds(if is_pr {
                        "github-pr-agent-action"
                    } else {
                        "github-issue-agent-action"
                    })
                    .unwrap();
                assert!(
                    agent_action.right() <= scroll.right(),
                    "{agent_action:?} {header:?}"
                );
                assert!(header.size.width <= px(width), "width {width}: {header:?}");
                // Back and the header title share one line, in that order.
                assert!(back.right() <= pinned.left(), "{back:?} {pinned:?}");
                assert_eq!(
                    back.top() + back.size.height / 2.,
                    pinned.top() + pinned.size.height / 2.,
                    "{back:?} {pinned:?}"
                );
                assert!(pinned.right() <= scroll.right(), "{pinned:?} {scroll:?}");
                // The back glyph, not its hover frame, sits on the content edge.
                let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
                assert_eq!(
                    back.left()
                        + (ButtonSize::Default.height(&theme)
                            - ButtonSize::Default.icon_size().px(&theme))
                            / 2.,
                    description.left()
                );
                assert_eq!(eyebrow.left(), description.left());
                assert!(eyebrow.right() <= scroll.right(), "{eyebrow:?} {scroll:?}");
                assert!(header.bottom() <= scroll.top(), "{header:?} {scroll:?}");
                assert!(
                    scroll.size.height > px(height / 2.),
                    "height {height}: {scroll:?}"
                );
                assert!(
                    description.left() >= scroll.left(),
                    "{description:?} {scroll:?}"
                );
                assert!(
                    description.right() <= scroll.right(),
                    "{description:?} {scroll:?}"
                );
                let comment = cx.debug_bounds("github-comment").unwrap();
                assert!(comment.right() <= scroll.right(), "{comment:?} {scroll:?}");
                cx.simulate_event(gpui::ScrollWheelEvent {
                    position: point(
                        scroll.left() + scroll.size.width / 2.,
                        scroll.top() + px(30.),
                    ),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-80.))),
                    ..Default::default()
                });
                draw(cx);
                let pinned_after = cx.debug_bounds("github-detail-pinned-title").unwrap();
                let eyebrow_after = cx.debug_bounds("github-detail-eyebrow").unwrap();
                assert_eq!(pinned.top(), pinned_after.top());
                assert!(eyebrow_after.top() < eyebrow.top(), "document must scroll at {width}x{height}, pr={is_pr}: {eyebrow:?} -> {eyebrow_after:?}");
                cx.simulate_event(gpui::ScrollWheelEvent {
                    position: point(
                        scroll.left() + scroll.size.width / 2.,
                        scroll.top() + px(30.),
                    ),
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-1000.))),
                    ..Default::default()
                });
                draw(cx);
                assert_eq!(
                    pinned.top(),
                    cx.debug_bounds("github-detail-pinned-title").unwrap().top()
                );
                assert_eq!(
                    header.size.height,
                    cx.debug_bounds("github-detail-header").unwrap().size.height
                );
                assert!(cx.debug_bounds("github-detail-title").is_none());
                if let Some(eyebrow) = cx.debug_bounds("github-detail-eyebrow") {
                    assert!(
                        eyebrow.bottom() <= scroll.top(),
                        "metadata should scroll away: {eyebrow:?} {scroll:?}"
                    );
                }
                if is_pr {
                    let summary = cx.debug_bounds("github-pr-summary").unwrap();
                    assert!(summary.right() <= scroll.right(), "{summary:?} {scroll:?}");
                    for selector in ["github-checks", "github-commits", "github-files"] {
                        let section = cx.debug_bounds(selector).unwrap();
                        assert!(
                            section.right() <= scroll.right(),
                            "{selector}: {section:?} {scroll:?}"
                        );
                    }
                }
            }
        }
    }

    /// The Git page as the app hosts it: a feature card in the main row.
    struct GitPageProbe(Entity<GitPanel>);

    impl Render for GitPageProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *theme::get(cx);
            div().size_full().flex().flex_col().child(
                div().flex_1().min_h_0().w_full().flex().relative().child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .pl(DynamicSpacing::Base12.px(&theme))
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .flex_col()
                                .overflow_hidden()
                                .border_1()
                                .child(self.0.clone()),
                        ),
                ),
            )
        }
    }

    /// The real page paints the title in its fixed header at every scroll
    /// position, in single and split layouts on both tabs. Long titles retain
    /// their words and end in `…` rather than painting a lone ellipsis.
    #[gpui::test]
    fn github_detail_pinned_title_paints_its_text_on_the_git_page(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let short = "Can Orbit directly invoke the pi on a remote server?";
        let long =
            "chore(deps): bump the cargo group across 1 directory with 11 updates ".repeat(4);
        let long = long.trim_end();
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        let cx = cx.add_empty_window();
        for tab in [GitTab::Issues, GitTab::Pulls] {
            for title in [short, long] {
                panel.update(cx, |panel, _| {
                    panel.open = true;
                    panel.tab = tab;
                    panel.gh_installed = true;
                    panel.gh_authenticated = true;
                    panel.issue_detail = Some(gh::GhIssue {
                        number: 123,
                        title: title.into(),
                        state: "OPEN".into(),
                        body: "As the title states.\n\n".repeat(40),
                        author: github_user("vimbackground"),
                        labels: Vec::new(),
                        assignees: Vec::new(),
                        comments: Vec::new(),
                        created_at: "2026-01-01T00:00:00Z".into(),
                        updated_at: "2026-01-02T00:00:00Z".into(),
                        url: "https://github.com/imrj05/orbit/issues/123".into(),
                    });
                    panel.pr_detail = Some(gh::GhPull {
                        number: 121,
                        title: title.into(),
                        state: "OPEN".into(),
                        is_draft: false,
                        body: "Bumps the cargo group.\n\n".repeat(40),
                        author: github_user("app/dependabot"),
                        labels: Vec::new(),
                        review_decision: Some("REVIEW_REQUIRED".into()),
                        mergeable: Some("MERGEABLE".into()),
                        merge_state_status: Some("BLOCKED".into()),
                        head_ref: "dependabot/cargo/cargo-29041ad6f9".into(),
                        base_ref: "main".into(),
                        checks: Vec::new(),
                        additions: 110,
                        deletions: 108,
                        changed_files: 2,
                        created_at: "2026-01-01T00:00:00Z".into(),
                        updated_at: "2026-01-02T00:00:00Z".into(),
                        url: "https://github.com/imrj05/orbit/pull/121".into(),
                        comments: Vec::new(),
                        reviews: Vec::new(),
                        commits: Vec::new(),
                        files: Vec::new(),
                    });
                });
                for width in [760., 1400.] {
                    panel.update(cx, |panel, cx| panel.set_review_width(px(width - 14.), cx));
                    let probe = cx.update(|_, cx| cx.new(|_| GitPageProbe(panel.clone())));
                    let draw = |cx: &mut gpui::VisualTestContext| {
                        let probe = probe.clone();
                        let _ = cx.draw(
                            point(px(0.), px(0.)),
                            size(px(width), px(640.)),
                            move |_, _| div().size_full().child(probe),
                        );
                    };
                    draw(cx);
                    let header = cx.debug_bounds("github-detail-header").unwrap();
                    let back = cx.debug_bounds("github-detail-back").unwrap();
                    let pinned = cx.debug_bounds("github-detail-pinned-title").unwrap();
                    let scroll = cx.debug_bounds("github-detail-scroll").unwrap();
                    let eyebrow = cx.debug_bounds("github-detail-eyebrow").unwrap();
                    assert!(cx.debug_bounds("github-detail-title").is_none());
                    assert!(back.right() <= pinned.left());
                    assert!(pinned.right() <= header.right());
                    assert!(header.bottom() <= scroll.top());
                    let painted_title = || {
                        PINNED_TITLE_PAINTED
                            .with(|slot| slot.borrow().clone())
                            .expect("pinned title rendered")
                            .borrow()
                            .to_string()
                    };
                    let painted = painted_title();
                    let context = format!("{tab:?} at {width}: {painted:?}");
                    if title == short {
                        assert_eq!(painted, title, "{context}");
                    } else {
                        let kept = painted.strip_suffix('\u{2026}').expect(&context);
                        assert!(kept.len() > 20 && title.starts_with(kept), "{context}");
                    }
                    cx.simulate_event(gpui::ScrollWheelEvent {
                        position: point(
                            scroll.left() + scroll.size.width / 2.,
                            scroll.top() + px(30.),
                        ),
                        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-400.))),
                        ..Default::default()
                    });
                    draw(cx);
                    assert!(
                        cx.debug_bounds("github-detail-eyebrow")
                            .is_none_or(|after| after.top() < eyebrow.top()),
                        "{context}"
                    );
                    assert_eq!(cx.debug_bounds("github-detail-header").unwrap(), header);
                    assert_eq!(
                        cx.debug_bounds("github-detail-pinned-title").unwrap(),
                        pinned
                    );
                    assert_eq!(painted_title(), painted, "{context}");
                    // Returning to the top leaves the same title in place.
                    cx.simulate_event(gpui::ScrollWheelEvent {
                        position: point(
                            scroll.left() + scroll.size.width / 2.,
                            scroll.top() + px(30.),
                        ),
                        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(400.))),
                        ..Default::default()
                    });
                    draw(cx);
                    assert_eq!(
                        cx.debug_bounds("github-detail-pinned-title").unwrap(),
                        pinned
                    );
                    assert_eq!(painted_title(), painted, "{context}");
                }
            }
        }
    }

    struct PullsPageProbe(Entity<GitPanel>);

    impl Render for PullsPageProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0
                .update(cx, |panel, cx| panel.pulls_tab(*theme::get(cx), cx))
        }
    }

    #[gpui::test]
    fn pull_summary_survives_a_detail_request_failure(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        panel.update(cx, |panel, _| {
            panel.gh_installed = true;
            panel.gh_authenticated = true;
            panel.page_width = px(1000.);
            panel.pulls = vec![gh::GhPull {
                number: 113,
                title: "List data keeps the summary visible".into(),
                state: "OPEN".into(),
                is_draft: false,
                body: String::new(),
                author: github_user("dependabot[bot]"),
                labels: vec![github_label("dependencies", "0366d6")],
                review_decision: Some("REVIEW_REQUIRED".into()),
                mergeable: Some("MERGEABLE".into()),
                merge_state_status: Some("BEHIND".into()),
                head_ref: "dependabot/cargo/updates".into(),
                base_ref: "main".into(),
                checks: vec![gh::GhCheck {
                    name: "CI / build".into(),
                    status: "IN_PROGRESS".into(),
                    ..Default::default()
                }],
                additions: 93,
                deletions: 103,
                changed_files: 2,
                created_at: "2026-01-01T00:00:00Z".into(),
                updated_at: "2026-01-02T00:00:00Z".into(),
                url: "https://github.com/orbit/app/pull/113".into(),
                comments: Vec::new(),
                reviews: Vec::new(),
                commits: Vec::new(),
                files: Vec::new(),
            }];
            panel.selected_pr = Some(113);
            panel.pr_detail = None;
            panel.pr_detail_error = Some("detail request failed".into());
        });
        let cx = cx.add_empty_window();
        let _ = cx.draw(point(px(0.), px(0.)), size(px(1000.), px(640.)), |_, cx| {
            cx.new(|_| PullsPageProbe(panel.clone()))
        });
        assert!(cx.debug_bounds("github-detail-pinned-title").is_some());
        assert!(cx.debug_bounds("github-pr-summary").is_some());
        assert!(cx.debug_bounds("github-merge-state").is_some());
        assert!(cx.debug_bounds("github-detail-error").is_some());
        assert!(
            cx.debug_bounds("github-description").is_none(),
            "an unknown description must not be replaced by an empty one"
        );
        assert_eq!(
            cx.update(|_, cx| panel.read(cx).selected_pull().map(|pull| pull.number)),
            Some(113)
        );
    }

    struct GithubSplitProbe(Entity<GitPanel>);

    impl Render for GithubSplitProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            self.0.update(cx, |panel, cx| {
                panel.github_split(
                    div().into_any_element(),
                    Some(div().into_any_element()),
                    *theme::get(cx),
                    cx,
                )
            })
        }
    }

    #[gpui::test]
    fn github_split_gives_detail_the_remaining_width(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let panel = cx.update(|cx| cx.new(GitPanel::new));
        panel.update(cx, |panel, _| {
            panel.page_width = px(1000.);
            panel.github_list_width = px(GITHUB_LIST_DEFAULT_W);
        });
        let cx = cx.add_empty_window();
        let _ = cx.draw(point(px(0.), px(0.)), size(px(1000.), px(600.)), |_, cx| {
            cx.new(|_| GithubSplitProbe(panel.clone()))
        });
        let list = cx.debug_bounds("github-split-list").unwrap();
        let handle = cx.debug_bounds("github-split-handle").unwrap();
        let detail = cx.debug_bounds("github-split-detail").unwrap();
        assert_eq!(list.size.width, px(GITHUB_LIST_DEFAULT_W));
        assert_eq!(handle.size.width, px(GITHUB_LIST_HANDLE_W));
        assert_eq!(
            detail.size.width,
            px(1000. - GITHUB_LIST_DEFAULT_W - GITHUB_LIST_HANDLE_W)
        );
        // The handle *is* the seam: the list has no border of its own.
        assert_eq!(list.right(), handle.left());
        assert_eq!(handle.right(), detail.left());
    }

    /// The pure drag arithmetic: a pointer delta moves the pane, but a push
    /// against a page-clamped edge reports no movement so the stored preference
    /// survives.
    #[test]
    fn github_list_drag_respects_clamped_edges() {
        assert_eq!(
            dragged_github_list_pane_width(px(1000.), px(320.), px(320.), px(323.), px(383.)),
            Some(px(380.))
        );
        // At 800 the pane is held at 393; pushing right cannot move it.
        assert_eq!(
            dragged_github_list_pane_width(px(800.), px(420.), px(393.), px(393.), px(443.)),
            None
        );
        // Pulling left does move it, so that becomes the new preference.
        assert_eq!(
            dragged_github_list_pane_width(px(800.), px(420.), px(393.), px(393.), px(392.)),
            Some(px(392.))
        );
    }

    /// Release writes the settled width, but a click or a push against a
    /// clamped edge writes nothing, and an out-of-range value never reaches the
    /// store.
    #[test]
    fn github_list_width_persists_only_after_a_moved_drag() {
        let moved = GithubListDrag {
            origin_width: px(320.),
            origin_x: px(320.),
            moved: true,
        };
        let still = GithubListDrag {
            moved: false,
            ..moved
        };
        assert_eq!(
            github_list_width_to_persist(Some(&moved), px(380.)),
            Some(380.)
        );
        assert_eq!(github_list_width_to_persist(Some(&still), px(380.)), None);
        assert_eq!(github_list_width_to_persist(None, px(380.)), None);
        assert_eq!(
            github_list_width_to_persist(Some(&moved), px(GITHUB_LIST_MAX_W + 1.)),
            None
        );
    }

    /// The list pane's own clamp: never narrower than it can render, never wider
    /// than the detail pane's reserve allows, whatever a stored value says.
    #[test]
    fn github_list_width_is_clamped_to_its_bounds_and_the_detail_reserve() {
        // A width the user picked survives a page that can afford it.
        assert_eq!(github_list_pane_width(px(1400.), px(320.)), px(320.));
        // The ceiling is the panel maximum, or page - 400 when that is smaller.
        assert_eq!(github_list_pane_width(px(1400.), px(600.)), px(420.));
        // 800 - (400 detail + 7 handle) leaves 393 for the list.
        assert_eq!(github_list_pane_width(px(800.), px(420.)), px(393.));
        assert_eq!(github_list_pane_width(px(807.), px(420.)), px(400.));
        // The floor holds even when the page cannot afford the detail reserve.
        assert_eq!(github_list_pane_width(px(700.), px(100.)), px(240.));
        assert_eq!(github_list_pane_width(px(0.), px(320.)), px(240.));
        // The default is inside the bounds the split view can render.
        assert_eq!(
            github_list_pane_width(px(800.), px(GITHUB_LIST_DEFAULT_W)),
            px(GITHUB_LIST_DEFAULT_W)
        );
    }

    #[test]
    fn github_item_repository_requires_owner_and_repo() {
        assert_eq!(
            github_repo("https://github.com/orbit/app/issues/42"),
            Some("orbit/app")
        );
        assert_eq!(
            github_repo("https://enterprise.example/team/repo/pull/8"),
            Some("team/repo")
        );
        assert_eq!(github_repo(""), None);
        assert_eq!(github_repo("https://github.com/owner//issues/1"), None);
    }

    #[test]
    fn github_split_respects_available_page_width() {
        assert!(!split_github_view(px(0.)));
        assert!(!split_github_view(px(799.)));
        assert!(split_github_view(px(800.)));
        assert!(split_github_view(px(1200.)));
    }

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
    fn commit_model_rows_pin_the_session_option_and_filter_the_catalog() {
        let models = vec![
            ModelEntry {
                id: "qwen3-coder".into(),
                name: "Qwen3 Coder".into(),
                provider: "ollama".into(),
                context_window: Some(128_000),
                thinking_levels: Vec::new(),
            },
            ModelEntry {
                id: "claude-sonnet-4-5".into(),
                name: "Claude Sonnet 4.5".into(),
                provider: "anthropic".into(),
                context_window: Some(200_000),
                thinking_levels: Vec::new(),
            },
        ];
        // "Follow the session model" is always reachable, at row 0, so a
        // search can never hide the way back to the session model.
        assert_eq!(commit_model_rows(&models, ""), vec![None, Some(0), Some(1)]);
        assert_eq!(commit_model_rows(&models, "ollama"), vec![None, Some(0)]);
        assert_eq!(commit_model_rows(&models, "claude"), vec![None, Some(1)]);
        assert_eq!(commit_model_rows(&models, "anthropic"), vec![None, Some(1)]);
        assert_eq!(commit_model_rows(&models, "missing"), vec![None]);
    }

    #[test]
    fn avatar_url_prefers_noreply_email_login() {
        // The noreply login wins even when the display name is a valid handle.
        assert_eq!(
            github_avatar_url("someone", "12345+real-login@users.noreply.github.com"),
            Some("https://github.com/real-login.png?size=48".into())
        );
        // A display name with spaces no longer hides the login in the email.
        assert_eq!(
            github_avatar_url(
                "Rajeshwar Kashyap",
                "76556671+imrj05@users.noreply.github.com"
            ),
            Some("https://github.com/imrj05.png?size=48".into())
        );
        // Legacy noreply addresses carry the login as the whole local part.
        assert_eq!(
            github_avatar_url("Ada Lovelace", "adal@users.noreply.github.com"),
            Some("https://github.com/adal.png?size=48".into())
        );
    }

    #[test]
    fn avatar_url_falls_back_to_handle_shaped_name() {
        // No usable email → a handle-shaped name is still a candidate.
        assert_eq!(
            github_avatar_url("work-rjkashyap", "work@example.com"),
            Some("https://github.com/work-rjkashyap.png?size=48".into())
        );
        // A real name and a non-noreply address resolve to no photo.
        assert_eq!(github_avatar_url("Ada Lovelace", "ada@example.com"), None);
        // Invalid handle shapes are rejected rather than 404-ing on the forge.
        assert_eq!(github_avatar_url("-bad-", ""), None);
        assert_eq!(github_avatar_url("a--b", ""), None);
    }

    #[test]
    fn avatar_url_uses_email_login_override() {
        // A personal (non-noreply) address resolves through the override map,
        // so historical commits show the real photo instead of the monogram.
        let overrides = HashMap::from([(
            "work.rjkashyap05@gmail.com".to_string(),
            "imrj05".to_string(),
        )]);
        assert_eq!(
            resolve_github_login(
                "Rajeshwar Kashyap",
                "work.rjkashyap05@gmail.com",
                &overrides
            ),
            Some("imrj05".into())
        );
        // The map is consulted case-insensitively and wins over the heuristic.
        assert_eq!(
            resolve_github_login(
                "Rajeshwar Kashyap",
                " Work.RJKashyap05@Gmail.com ",
                &overrides
            ),
            Some("imrj05".into())
        );
        // Without an entry, the heuristic decides as before.
        assert_eq!(
            resolve_github_login("Ada Lovelace", "ada@example.com", &overrides),
            None
        );
    }
}
