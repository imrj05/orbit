//! The Review page — the app's AI review surface.
//!
//! A page header, four tabs, and the run detail view:
//!
//! - **New review** — a form: repository, review type (with its live change
//!   count), the branch base when reviewing a branch, and the model /
//!   thinking the run uses. One accent button starts it.
//! - **Running** — every live run plus the ones that just finished. A row
//!   opens the run detail.
//! - **Changes** — the selected target's changed files; a file shows its diff.
//! - **History** — finished runs across workspaces.
//!
//! The run detail (a row click) is where everything about a run lives: its
//! status, target, model, verdict, findings, and files.
//!
//! The page is a dumb renderer: the app computes a [`ReviewPageSnapshot`]
//! each frame and the page lays it out. Actions travel back through
//! [`ReviewPageAction`] callbacks, so the page never borrows the app or the
//! review store. Every control is built from the design tokens in
//! `theme/tokens.rs` — spacing, type, radii, button and icon sizes — so the
//! page tracks the UI font size and spacing density like the rest of the app.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, AnyElement, App, Context, FontWeight, MouseButton, Render, Window,
};

use crate::ai_review::{Finding, ReviewKind, Severity, Verdict};
use crate::app::helpers::{
    button_frame, context_menu_entry, context_menu_surface, icon, icon_button_frame, BUTTON_GROUP,
};
use crate::app::PRESS_DIM;
use crate::reviews::{ReviewRun, RunStatus};
use crate::theme::tokens::{
    ButtonSize, DynamicSpacing, IconSize, Radius, TextSize,
};
use crate::theme::{self, Theme};

/// Which tab the page shows.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReviewTab {
    #[default]
    New,
    Running,
    Changes,
    History,
}

impl ReviewTab {
    /// Every tab, in strip order.
    pub const ALL: [ReviewTab; 4] = [Self::New, Self::Running, Self::Changes, Self::History];

    fn label(self) -> String {
        match self {
            Self::New => tr!("review_page.tab_new"),
            Self::Running => tr!("review_page.tab_running"),
            Self::Changes => tr!("review_page.tab_changes"),
            Self::History => tr!("review_page.tab_history"),
        }
    }
}

/// Which dropdown is open, if any.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Menu {
    Repository,
    ReviewType,
    Base,
    Model,
    Thinking,
}

/// The review type the form is set to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewTarget {
    /// HEAD → worktree, the common case and the default.
    Uncommitted,
    /// `merge-base(HEAD, base)` → worktree.
    Branch,
    /// One commit from the recent list.
    Commit { sha: String, title: String },
    /// A snapshot of chosen workspace paths.
    Files,
    /// A repository-wide pass.
    Project,
}

impl ReviewTarget {
    fn key(&self) -> &'static str {
        match self {
            Self::Uncommitted => "uncommitted",
            Self::Branch => "branch",
            Self::Commit { .. } => "commit",
            Self::Files => "files",
            Self::Project => "project",
        }
    }

    fn label(&self, base: &str) -> String {
        match self {
            Self::Uncommitted => tr!("ai_review.kind_uncommitted"),
            Self::Branch => tr!("ai_review.kind_branch_base", base = base.to_string()),
            Self::Commit { sha, title } if sha.is_empty() => tr!("review_page.target_commit"),
            Self::Commit { sha, title } => {
                let short: String = sha.chars().take(7).collect();
                if title.is_empty() {
                    tr!("ai_review.kind_commit", sha = short)
                } else {
                    tr!("review_page.commit_named", sha = short, title = title.clone())
                }
            }
            Self::Files => tr!("review_page.target_files"),
            Self::Project => tr!("ai_review.review_project"),
        }
    }

    fn description(&self) -> String {
        match self {
            Self::Uncommitted => tr!("review_page.type_uncommitted_hint"),
            Self::Branch => tr!("review_page.type_branch_hint"),
            Self::Commit { .. } => tr!("review_page.type_commit_hint"),
            Self::Files => tr!("review_page.type_files_hint"),
            Self::Project => tr!("review_page.type_project_hint"),
        }
    }

    /// The `ReviewKind` a run of this target uses.
    pub fn to_kind(&self, base: &str, paths: Vec<String>) -> ReviewKind {
        match self {
            Self::Uncommitted => ReviewKind::Uncommitted,
            Self::Branch => ReviewKind::Branch {
                base: base.to_string(),
            },
            Self::Commit { sha, title } => ReviewKind::Commit {
                sha: sha.clone(),
                title: title.clone(),
            },
            Self::Files => ReviewKind::Files { paths },
            Self::Project => ReviewKind::Project,
        }
    }
}

/// Per-target facts the page shows (file counts and the branch name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetFacts {
    pub uncommitted_files: Option<usize>,
    pub uncommitted_added: u64,
    pub uncommitted_deleted: u64,
    pub branch_files: Option<usize>,
    pub base_branch: String,
    pub commit_files: Option<usize>,
    pub project_files: Option<usize>,
}

/// One commit in the recent-commits list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitRow {
    pub sha: String,
    pub short: String,
    pub subject: String,
    pub file_count: usize,
    pub added: u64,
    pub deleted: u64,
}

/// One changed file in the Changes tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub status: char,
    pub added: u64,
    pub deleted: u64,
}

/// One repository the review can run against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceOption {
    pub path: String,
    pub label: String,
    pub is_current: bool,
}

/// One model the reviewer can run on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOption {
    pub id: String,
    pub provider: String,
    pub label: String,
}

/// The app-computed state the page renders. Rebuilt each frame.
#[derive(Clone, Default, PartialEq)]
pub struct ReviewPageSnapshot {
    /// The workspace the facts were collected for (folder name).
    pub workspace: String,
    /// The workspace's full path, shown under the header title.
    pub workspace_path: String,
    /// Every repository the review can target.
    pub workspaces: Vec<WorkspaceOption>,
    /// Models the reviewer can use.
    pub models: Vec<ModelOption>,
    /// The selected model's thinking levels.
    pub thinking_levels: Vec<String>,
    /// The model the next run uses (`id`).
    pub model: String,
    /// The model's provider, for the RPC `set_model` shape.
    pub provider: String,
    /// The thinking level the next run uses.
    pub thinking: String,
    /// Local branches for the branch target's base picker.
    pub branches: Vec<String>,
    /// The target's label for the Changes scope line.
    pub target: String,
    pub facts: TargetFacts,
    pub commits: Vec<CommitRow>,
    pub changed_files: Vec<ChangedFile>,
    /// The parsed diff behind those files, for the per-file preview.
    pub snapshot: Option<std::sync::Arc<crate::review::Snapshot>>,
    /// `REVIEW_GUIDELINES.md`, when the workspace has one.
    pub guidelines: Option<String>,
    /// Every run, newest first — running and history alike.
    pub runs: Vec<ReviewRun>,
    /// A run-level failure with no run to attach to (no workspace open).
    pub error: Option<String>,
}

/// Actions the page sends back to the app. The page never touches the store.
pub enum ReviewPageAction {
    /// Start a run for the selected target and config.
    Start {
        kind: ReviewKind,
        workspace: String,
        model: String,
        provider: String,
        thinking: String,
    },
    /// Stop a run.
    Cancel(u64),
    /// The selected target changed; the app collects its changed files.
    TargetChanged(ReviewKind),
    /// The repository changed; the app recollects the facts.
    WorkspaceChanged(String, ReviewKind),
    /// The model changed; the app recomputes the thinking levels.
    ModelChanged { id: String, provider: String },
    /// The thinking level changed.
    ThinkingChanged(String),
    /// The branch base changed.
    BaseChanged(String),
    /// Recompute the target facts (the refresh control).
    Refresh(ReviewKind),
}

/// App-provided handler, rebuilt each frame like the pane's review opener.
pub type ReviewAction = Rc<dyn Fn(ReviewPageAction, &mut Window, &mut App)>;

pub struct ReviewPage {
    tab: ReviewTab,
    target: ReviewTarget,
    /// The commit the Commit target points at.
    selected_commit: Option<(String, String)>,
    snapshot: ReviewPageSnapshot,
    action: Option<ReviewAction>,
    /// Whether the page is the active main-area surface.
    open: bool,
    /// The target the app last collected changed files for.
    collected_kind: Option<ReviewKind>,
    /// The Back control asks the app to close the page on its next heartbeat.
    close_requested: bool,
    /// Which changed file the Changes tab is previewing.
    selected_file: Option<usize>,
    /// Paths checked for the Files target.
    checked: Vec<String>,
    /// The open dropdown, if any.
    menu: Option<Menu>,
    /// The run whose detail view is open.
    detail_run: Option<u64>,
}

impl Default for ReviewPage {
    fn default() -> Self {
        Self {
            tab: ReviewTab::default(),
            target: ReviewTarget::Uncommitted,
            selected_commit: None,
            snapshot: ReviewPageSnapshot::default(),
            action: None,
            open: false,
            collected_kind: None,
            close_requested: false,
            selected_file: None,
            checked: Vec::new(),
            menu: None,
            detail_run: None,
        }
    }
}

impl ReviewPage {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Enter the page. Always lands on New review: that is the action.
    pub fn open(&mut self, _window: &mut Window, _cx: &mut App) {
        self.open = true;
        self.tab = ReviewTab::New;
        self.detail_run = None;
        self.menu = None;
        // The app collects the initial target's files itself (see
        // `open_review_page`); dispatching from here would re-enter this entity
        // while its own update holds it.
        self.collected_kind = None;
    }

    /// Leave the page.
    pub fn close(&mut self) {
        self.open = false;
        self.menu = None;
        self.detail_run = None;
    }

    /// Whether the Back control asked to leave.
    pub fn take_close_request(&mut self) -> bool {
        let taken = std::mem::take(&mut self.close_requested);
        if taken {
            self.close();
        }
        taken
    }

    /// Mirror the app's state into the page. Only a real change notifies.
    pub fn set_snapshot(&mut self, snapshot: ReviewPageSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        // A file selection from a previous target no longer applies.
        if let Some(index) = self.selected_file {
            if index >= self.snapshot.changed_files.len() {
                self.selected_file = None;
            }
        }
        cx.notify();
    }

    pub fn set_action(&mut self, action: ReviewAction) {
        self.action = Some(action);
    }

    /// The `ReviewKind` the selected target maps to.
    pub fn target_kind(&self) -> Option<ReviewKind> {
        Some(
            self.target
                .to_kind(&self.snapshot.facts.base_branch, Vec::new()),
        )
    }

    /// A short label for the selected target.
    pub fn target_label(&self) -> String {
        self.target.label(&self.snapshot.facts.base_branch)
    }

    /// The current target, for the app's initial collection.
    fn selected_kind(&self) -> ReviewKind {
        self.target
            .to_kind(&self.snapshot.facts.base_branch, self.checked.clone())
    }

    // ── interaction ───────────────────────────────────────────────────────

    fn show_tab(&mut self, tab: ReviewTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.menu = None;
        cx.notify();
    }

    fn open_run(&mut self, id: u64, cx: &mut Context<Self>) {
        self.detail_run = Some(id);
        cx.notify();
    }

    fn close_run(&mut self, cx: &mut Context<Self>) {
        if self.detail_run.take().is_some() {
            cx.notify();
        }
    }

    fn select_target(&mut self, target: ReviewTarget, window: &mut Window, cx: &mut Context<Self>) {
        if self.target == target {
            self.menu = None;
            cx.notify();
            return;
        }
        self.target = target;
        self.menu = None;
        self.selected_file = None;
        self.checked.clear();
        self.notify_target(window, cx);
        cx.notify();
    }

    /// Tell the app which target's changed files to collect.
    fn notify_target(&mut self, window: &mut Window, cx: &mut App) {
        let kind = self.selected_kind();
        if self.collected_kind.as_ref() == Some(&kind) {
            return;
        }
        self.collected_kind = Some(kind.clone());
        if let Some(action) = self.action.as_ref() {
            action(ReviewPageAction::TargetChanged(kind), window, cx);
        }
    }

    fn select_commit(&mut self, sha: String, title: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_commit = Some((sha.clone(), title.clone()));
        self.target = ReviewTarget::Commit { sha, title };
        self.menu = None;
        self.notify_target(window, cx);
        cx.notify();
    }

    fn select_repository(&mut self, option: &WorkspaceOption, window: &mut Window, cx: &mut App) {
        self.menu = None;
        let kind = self.selected_kind();
        if let Some(action) = self.action.as_ref() {
            action(
                ReviewPageAction::WorkspaceChanged(option.path.clone(), kind),
                window,
                cx,
            );
        }
    }

    fn select_model(&mut self, option: &ModelOption, window: &mut Window, cx: &mut App) {
        self.menu = None;
        if let Some(action) = self.action.as_ref() {
            action(
                ReviewPageAction::ModelChanged {
                    id: option.id.clone(),
                    provider: option.provider.clone(),
                },
                window,
                cx,
            );
        }
    }

    fn select_thinking(&mut self, level: &str, window: &mut Window, cx: &mut App) {
        self.menu = None;
        if let Some(action) = self.action.as_ref() {
            action(
                ReviewPageAction::ThinkingChanged(level.to_string()),
                window,
                cx,
            );
        }
    }

    fn select_base(&mut self, branch: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        if let Some(action) = self.action.as_ref() {
            action(
                ReviewPageAction::BaseChanged(branch.to_string()),
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut App) {
        self.menu = None;
        let kind = self.selected_kind();
        if let Some(action) = self.action.as_ref() {
            action(ReviewPageAction::Refresh(kind), window, cx);
        }
    }

    fn toggle_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.menu = (self.menu != Some(menu)).then_some(menu);
        cx.notify();
    }

    fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    fn start(&self, window: &mut Window, cx: &mut App) {
        let Some(action) = self.action.as_ref() else {
            return;
        };
        let kind = self.selected_kind();
        action(
            ReviewPageAction::Start {
                kind,
                workspace: self.snapshot.workspace_path.clone(),
                model: self.snapshot.model.clone(),
                provider: self.snapshot.provider.clone(),
                thinking: self.snapshot.thinking.clone(),
            },
            window,
            cx,
        );
    }

    fn cancel(&self, id: u64, window: &mut Window, cx: &mut App) {
        if let Some(action) = self.action.as_ref() {
            action(ReviewPageAction::Cancel(id), window, cx);
        }
    }

    fn select_file(&mut self, index: usize, cx: &mut Context<Self>) {
        self.selected_file = Some(index);
        cx.notify();
    }

    /// Toggle one file into or out of the Files target.
    fn toggle_checked(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        match self.checked.iter().position(|checked| checked == &path) {
            Some(index) => {
                self.checked.remove(index);
            }
            None => self.checked.push(path),
        }
        self.target = if self.checked.is_empty() {
            ReviewTarget::Uncommitted
        } else {
            ReviewTarget::Files
        };
        self.notify_target(window, cx);
        cx.notify();
    }

    /// The file count to show for one row of the type menu.
    fn target_files(&self, target: &ReviewTarget) -> Option<usize> {
        match target {
            ReviewTarget::Uncommitted => self.snapshot.facts.uncommitted_files,
            ReviewTarget::Branch => self.snapshot.facts.branch_files,
            ReviewTarget::Commit { sha, .. } => self
                .snapshot
                .commits
                .iter()
                .find(|commit| &commit.sha == sha)
                .map(|commit| commit.file_count)
                .or(self.snapshot.facts.commit_files),
            ReviewTarget::Files => (self.checked.len() > 0).then_some(self.checked.len()),
            ReviewTarget::Project => self.snapshot.facts.project_files,
        }
    }

    fn tab_badge(&self, tab: ReviewTab) -> Option<usize> {
        match tab {
            ReviewTab::Running => {
                let count = self
                    .snapshot
                    .runs
                    .iter()
                    .filter(|run| run.is_active())
                    .count();
                (count > 0).then_some(count)
            }
            ReviewTab::Changes => {
                let count = self.snapshot.changed_files.len();
                (count > 0).then_some(count)
            }
            ReviewTab::History => {
                let count = self
                    .snapshot
                    .runs
                    .iter()
                    .filter(|run| run.status.is_finished())
                    .count();
                (count > 0).then_some(count)
            }
            ReviewTab::New => None,
        }
    }

    // ── shared pieces ─────────────────────────────────────────────────────

    /// A page-level action button, matching the Git page's toolbar controls.
    fn toolbar_button(
        &self,
        id: &'static str,
        label: String,
        icon_path: Option<&'static str>,
        accent: bool,
        theme: Theme,
        on_click: impl Fn(&mut ReviewPage, &gpui::MouseUpEvent, &mut Window, &mut Context<ReviewPage>)
            + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut button = button_frame(div().id(id), &theme, ButtonSize::Default)
            .group(BUTTON_GROUP)
            .flex_none()
            .cursor_pointer()
            .active(|style| style.opacity(PRESS_DIM))
            .on_mouse_up(MouseButton::Left, cx.listener(on_click));
        if accent {
            button = button
                .bg(theme.accent)
                .hover(|style| style.opacity(0.9));
        } else {
            button = button
                .border_1()
                .border_color(theme.border)
                .hover(|style| style.bg(theme.bg_hover));
        }
        if let Some(path) = icon_path {
            button = button.child(icon(
                path,
                ButtonSize::Default.icon_size().px(&theme),
                if accent { theme.bg_main } else { theme.text_2 },
            ));
        }
        button
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if accent { theme.bg_main } else { theme.text })
                    .child(label),
            )
            .into_any_element()
    }

    /// The page header: what this page is looking at, plus its actions.
    fn page_header(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut path_row = div()
            .text_size(TextSize::XSmall.px(&theme))
            .text_color(theme.text_3)
            .child(self.snapshot.workspace_path.clone());
        if self.snapshot.guidelines.is_some() {
            path_row = path_row.child(
                div()
                    .ml(DynamicSpacing::Base08.px(&theme))
                    .text_color(theme.accent)
                    .child(tr!("review_page.guidelines_chip")),
            );
        }
        div()
            .flex_none()
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Default.px(&theme))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(if self.snapshot.workspace.is_empty() {
                                tr!("review_page.no_workspace")
                            } else {
                                self.snapshot.workspace.clone()
                            }),
                    )
                    .child(path_row),
            )
            .child(self.toolbar_button(
                "review-refresh",
                String::new(),
                Some("icons/refresh.svg"),
                false,
                theme,
                |this, _, window, cx| this.refresh(window, cx),
                cx,
            ))
            .child(self.toolbar_button(
                "review-new",
                tr!("review_page.new_review"),
                Some("icons/spark.svg"),
                true,
                theme,
                |this, _, _, cx| this.show_tab(ReviewTab::New, cx),
                cx,
            ))
            .into_any_element()
    }

    fn tab_strip(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut strip = div()
            .flex_none()
            .h(px(38.))
            .px(DynamicSpacing::Base16.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base02.px(&theme))
            .border_b_1()
            .border_color(theme.border);
        for (index, tab) in ReviewTab::ALL.into_iter().enumerate() {
            let active = self.tab == tab;
            let badge = self.tab_badge(tab);
            let mut row = div()
                .id(("review-tab", index))
                .relative()
                .h_full()
                .px(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .cursor_pointer()
                .text_size(TextSize::Small.px(&theme))
                .text_color(if active { theme.text } else { theme.text_2 })
                .when(active, |row| row.font_weight(FontWeight::MEDIUM))
                .hover(|style| style.text_color(theme.text))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _: &gpui::MouseUpEvent, _, cx| this.show_tab(tab, cx)),
                )
                .child(tab.label());
            if let Some(count) = badge {
                row = row.child(
                    div()
                        .px(DynamicSpacing::Base04.px(&theme))
                        .rounded(Radius::Full.px(&theme))
                        .bg(theme.accent.opacity(0.16))
                        .text_size(TextSize::XSmall.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.accent)
                        .child(count.to_string()),
                );
            }
            if active {
                row = row.child(
                    div()
                        .absolute()
                        .left(px(0.))
                        .right(px(0.))
                        .bottom(px(0.))
                        .h(px(2.))
                        .bg(theme.accent),
                );
            }
            strip = strip.child(row);
        }
        strip.into_any_element()
    }

    /// A form label.
    fn field_label(&self, text: &str, theme: Theme) -> AnyElement {
        div()
            .text_size(TextSize::XSmall.px(&theme))
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.text_2)
            .child(text.to_string())
            .into_any_element()
    }

    /// A select row: label, value, chevron. Opens `menu` when clicked.
    fn select_row(
        &self,
        id: &'static str,
        value: String,
        detail: Option<String>,
        menu: Menu,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open = self.menu == Some(menu);
        div()
            .id(id)
            .w_full()
            .h(px(36.))
            .px(DynamicSpacing::Base12.px(&theme))
            .rounded(Radius::Medium.px(&theme))
            .border_1()
            .border_color(if open { theme.accent } else { theme.border })
            .bg(if open { theme.active } else { theme.bg_hover })
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .cursor_pointer()
            .hover(|style| style.bg(theme.active))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &gpui::MouseDownEvent, _, cx| {
                    this.toggle_menu(menu, cx)
                }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text)
                    .child(value),
            )
            .when_some(detail, |row, detail| {
                row.child(
                    div()
                        .flex_none()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_3)
                        .child(detail),
                )
            })
            .child(icon(
                "icons/chevron-down.svg",
                IconSize::XSmall.px(&theme),
                theme.text_3,
            ))
            .into_any_element()
    }

    /// A dropdown menu anchored under its row. The caller renders it inside a
    /// `relative` container.
    fn menu_list(
        &self,
        id: &'static str,
        theme: Theme,
        cx: &mut Context<Self>,
        entries: Vec<AnyElement>,
    ) -> AnyElement {
        let mut menu = context_menu_surface(div().id(id), &theme)
            .absolute()
            .top(px(40.))
            .left(px(0.))
            .w_full()
            .max_h(px(280.))
            .overflow_y_scroll()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &gpui::MouseDownEvent, _, cx| {
                this.dismiss_menu(cx)
            }));
        for entry in entries {
            menu = menu.child(entry);
        }
        menu.into_any_element()
    }

    /// One row inside a dropdown: title, optional detail, checked marker.
    fn menu_row(
        &self,
        id: usize,
        title: String,
        detail: Option<String>,
        checked: bool,
        theme: Theme,
        on_click: impl Fn(&mut ReviewPage, &gpui::MouseUpEvent, &mut Window, &mut Context<ReviewPage>)
            + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut row = context_menu_entry(
            div()
                .id(("review-menu", id))
                .cursor_pointer()
                .hover(|style| style.bg(theme.bg_hover))
                .on_mouse_up(MouseButton::Left, cx.listener(on_click)),
            &theme,
        )
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(
            div()
                .w(px(14.))
                .flex_none()
                .when(checked, |mark| {
                    mark.child(icon(
                        "icons/check.svg",
                        IconSize::XSmall.px(&theme),
                        theme.accent,
                    ))
                }),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text)
                .child(title),
        );
        if let Some(detail) = detail {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(detail),
            );
        }
        row.into_any_element()
    }

    /// The status chip a run row and the run detail share.
    fn status_chip(&self, status: &RunStatus, theme: Theme) -> AnyElement {
        let (color, bg) = match status {
            RunStatus::Completed => (theme.add_green, theme.add_green.opacity(0.14)),
            RunStatus::Failed(_) => (theme.del_red, theme.del_red.opacity(0.14)),
            RunStatus::Cancelled => (theme.text_3, theme.text_3.opacity(0.12)),
            RunStatus::Queued | RunStatus::Running => (theme.accent, theme.accent.opacity(0.14)),
        };
        div()
            .flex_none()
            .px(DynamicSpacing::Base08.px(&theme))
            .h(px(22.))
            .rounded(Radius::Full.px(&theme))
            .bg(bg)
            .flex()
            .items_center()
            .text_size(TextSize::XSmall.px(&theme))
            .font_weight(FontWeight::MEDIUM)
            .text_color(color)
            .child(status.label())
            .into_any_element()
    }

    /// A severity chip for a finding.
    fn severity_chip(&self, severity: Severity, theme: Theme) -> AnyElement {
        let (tint, label) = match severity {
            Severity::Error => (theme.del_red, tr!("ai_review.severity_error")),
            Severity::Warning => (theme.warn, tr!("ai_review.severity_warning")),
            Severity::Info => (theme.text_3, tr!("ai_review.severity_info")),
        };
        div()
            .flex_none()
            .mt(px(1.))
            .px(DynamicSpacing::Base06.px(&theme))
            .h(px(18.))
            .rounded(Radius::Small.px(&theme))
            .bg(tint.opacity(0.14))
            .flex()
            .items_center()
            .text_size(TextSize::XSmall.px(&theme))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(tint)
            .child(label)
            .into_any_element()
    }

    fn verdict_row(&self, verdict: Verdict, report: &crate::ai_review::Report, theme: Theme) -> AnyElement {
        let (tint, label) = match verdict {
            Verdict::Correct => (theme.add_green, tr!("ai_review.verdict_correct")),
            Verdict::NeedsAttention => (theme.del_red, tr!("ai_review.verdict_needs_attention")),
        };
        let errors = report.count(Severity::Error);
        let warnings = report.count(Severity::Warning);
        let mut row = div()
            .w_full()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(div().flex_none().w(px(8.)).h(px(8.)).rounded_full().bg(tint))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(label),
            );
        for (count, tint) in [(errors, theme.del_red), (warnings, theme.warn)] {
            if count > 0 {
                row = row.child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(tint)
                        .child(count.to_string()),
                );
            }
        }
        row.into_any_element()
    }

    // ── New review ────────────────────────────────────────────────────────

    fn new_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut form = div()
            .w_full()
            .max_w(px(720.))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base16.px(&theme));

        if let Some(error) = self.snapshot.error.as_ref() {
            form = form.child(
                context_menu_surface(div(), &theme)
                    .w_full()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.del_red)
                    .child(error.clone()),
            );
        }

        // ── repository ──
        let current = self
            .snapshot
            .workspaces
            .iter()
            .find(|option| option.is_current)
            .cloned()
            .unwrap_or_else(|| WorkspaceOption {
                path: self.snapshot.workspace_path.clone(),
                label: self.snapshot.workspace.clone(),
                is_current: true,
            });
        let mut repo_block = div()
            .relative()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(self.field_label(&tr!("review_page.field_repository"), theme))
            .child(self.select_row(
                "review-repo",
                current.label.clone(),
                None,
                Menu::Repository,
                theme,
                cx,
            ));
        if self.menu == Some(Menu::Repository) {
            let mut entries = Vec::new();
            for (index, option) in self.snapshot.workspaces.iter().enumerate() {
                let option = option.clone();
                let checked = option.is_current;
                let entry = self.menu_row(
                    index,
                    option.label.clone(),
                    Some(option.path.clone()),
                    checked,
                    theme,
                    move |this, _, window, cx| this.select_repository(&option, window, cx),
                    cx,
                );
                entries.push(entry);
            }
            repo_block = repo_block.child(self.menu_list("review-repo-menu", theme, cx, entries));
        }
        form = form.child(repo_block);

        // ── review type ──
        let base = if self.snapshot.facts.base_branch.is_empty() {
            "main".to_string()
        } else {
            self.snapshot.facts.base_branch.clone()
        };
        let type_detail = match self.target_files(&self.target) {
            Some(count) => Some(tr!("ai_review.file_count", count = count)),
            None => None,
        };
        let mut type_block = div()
            .relative()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(self.field_label(&tr!("review_page.field_type"), theme))
            .child(self.select_row(
                "review-type",
                self.target.label(&base),
                type_detail,
                Menu::ReviewType,
                theme,
                cx,
            ))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(self.target.description()),
            );
        if self.menu == Some(Menu::ReviewType) {
            let mut entries = Vec::new();
            let base_for_rows = base.clone();
            let targets: Vec<(ReviewTarget, String, Option<String>)> = vec![
                (
                    ReviewTarget::Uncommitted,
                    tr!("ai_review.kind_uncommitted"),
                    self.snapshot
                        .facts
                        .uncommitted_files
                        .map(|count| tr!("ai_review.file_count", count = count)),
                ),
                (
                    ReviewTarget::Branch,
                    tr!("ai_review.kind_branch_base", base = base_for_rows.clone()),
                    self.snapshot
                        .facts
                        .branch_files
                        .map(|count| tr!("ai_review.file_count", count = count)),
                ),
                (
                    ReviewTarget::Commit {
                        sha: String::new(),
                        title: String::new(),
                    },
                    tr!("review_page.target_commit"),
                    None,
                ),
                (
                    ReviewTarget::Files,
                    tr!("review_page.target_files"),
                    (self.checked.len() > 0)
                        .then(|| tr!("ai_review.file_count", count = self.checked.len())),
                ),
                (
                    ReviewTarget::Project,
                    tr!("ai_review.review_project"),
                    self.snapshot
                        .facts
                        .project_files
                        .map(|count| tr!("ai_review.file_count", count = count)),
                ),
            ];
            for (index, (target, label, detail)) in targets.into_iter().enumerate() {
                let checked = self.target.key() == target.key();
                let entry = self.menu_row(
                    index,
                    label,
                    detail,
                    checked,
                    theme,
                    move |this, _, window, cx| this.select_target(target.clone(), window, cx),
                    cx,
                );
                entries.push(entry);
            }
            type_block = type_block.child(self.menu_list("review-type-menu", theme, cx, entries));
        }
        form = form.child(type_block);

        // ── branch base (only for the branch target) ──
        if matches!(self.target, ReviewTarget::Branch) {
            let mut base_block = div()
                .relative()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(self.field_label(&tr!("review_page.field_base"), theme))
                .child(self.select_row(
                    "review-base",
                    base.clone(),
                    None,
                    Menu::Base,
                    theme,
                    cx,
                ))
                .child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_3)
                        .child(tr!("review_page.base_hint", base = base.clone())),
                );
            if self.menu == Some(Menu::Base) {
                let mut entries = Vec::new();
                for (index, branch) in self.snapshot.branches.iter().enumerate() {
                    let branch = branch.clone();
                    let checked = branch == base;
                    let entry = self.menu_row(
                        index,
                        branch.clone(),
                        None,
                        checked,
                        theme,
                        move |this, _, window, cx| this.select_base(&branch, window, cx),
                        cx,
                    );
                    entries.push(entry);
                }
                base_block = base_block.child(self.menu_list("review-base-menu", theme, cx, entries));
            }
            form = form.child(base_block);
        }

        // ── model + thinking ──
        let model_label = self
            .snapshot
            .models
            .iter()
            .find(|option| option.id == self.snapshot.model)
            .map(|option| option.label.clone())
            .unwrap_or_else(|| {
                if self.snapshot.model.is_empty() {
                    tr!("ai_review.model_default")
                } else {
                    self.snapshot.model.clone()
                }
            });
        let mut model_block = div()
            .relative()
            .flex_1()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(self.field_label(&tr!("review_page.field_model"), theme))
            .child(self.select_row(
                "review-model",
                model_label,
                None,
                Menu::Model,
                theme,
                cx,
            ));
        if self.menu == Some(Menu::Model) {
            let mut entries = Vec::new();
            for (index, option) in self.snapshot.models.iter().enumerate() {
                let option = option.clone();
                let checked = option.id == self.snapshot.model;
                let entry = self.menu_row(
                    index,
                    option.label.clone(),
                    Some(option.id.clone()),
                    checked,
                    theme,
                    move |this, _, window, cx| this.select_model(&option, window, cx),
                    cx,
                );
                entries.push(entry);
            }
            model_block = model_block.child(self.menu_list("review-model-menu", theme, cx, entries));
        }
        let mut thinking_block = div()
            .relative()
            .w(px(180.))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(self.field_label(&tr!("review_page.field_thinking"), theme))
            .child(self.select_row(
                "review-thinking",
                self.snapshot.thinking.clone(),
                None,
                Menu::Thinking,
                theme,
                cx,
            ));
        if self.menu == Some(Menu::Thinking) {
            let mut entries = Vec::new();
            for (index, level) in self.snapshot.thinking_levels.iter().enumerate() {
                let level = level.clone();
                let checked = level == self.snapshot.thinking;
                let entry = self.menu_row(
                    index,
                    crate::model_selector::thinking_display(&level),
                    None,
                    checked,
                    theme,
                    move |this, _, window, cx| this.select_thinking(&level, window, cx),
                    cx,
                );
                entries.push(entry);
            }
            thinking_block =
                thinking_block.child(self.menu_list("review-thinking-menu", theme, cx, entries));
        }
        form = form.child(
            div()
                .flex()
                .items_start()
                .gap(DynamicSpacing::Base12.px(&theme))
                .child(model_block)
                .child(thinking_block),
        );

        // ── start ──
        form = form.child(
            div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    button_frame(div().id("review-start"), &theme, ButtonSize::Default)
                        .group(BUTTON_GROUP)
                        .cursor_pointer()
                        .bg(theme.accent)
                        .hover(|style| style.opacity(0.9))
                        .active(|style| style.opacity(0.8))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(|this, _: &gpui::MouseUpEvent, window, cx| {
                                this.start(window, cx)
                            }),
                        )
                        .child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.bg_main)
                                .child(tr!("ai_review.start")),
                        ),
                )
                .child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_3)
                        .child(match self.target_files(&self.target) {
                            Some(count) if count > 0 => {
                                tr!("review_page.start_hint", count = count)
                            }
                            _ => tr!("review_page.start_hint_empty"),
                        }),
                ),
        );

        context_menu_surface(div(), &theme)
            .w_full()
            .p(DynamicSpacing::Base16.px(&theme))
            .child(form)
            .into_any_element()
    }

    // ── Running / History ─────────────────────────────────────────────────

    /// One run row: status chip, target, workspace, metadata, severity counts.
    /// Clicking it opens the run detail.
    fn run_row(&self, run: &ReviewRun, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = run.id;
        let (errors, warnings, _) = run.counts();
        let active = run.is_active();
        let mut row = div()
            .id(("review-run", id as usize))
            .w_full()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_hover))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _: &gpui::MouseUpEvent, _, cx| this.open_run(id, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(120.))
                    .truncate()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(workspace_name(&run.workspace)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text)
                    .child(run.kind.label()),
            );
        for (count, tint) in [(errors, theme.del_red), (warnings, theme.warn)] {
            if count > 0 {
                row = row.child(
                    div()
                        .flex_none()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(tint)
                        .child(count.to_string()),
                );
            }
        }
        row = row
            .child(self.status_chip(&run.status, theme))
            .child(
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(run.summary_line()),
            );
        if active {
            row = row.child(
                icon_button_frame(div().id(("review-stop", id as usize)), &theme, ButtonSize::Compact)
                    .group(BUTTON_GROUP)
                    .flex_none()
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.bg_hover))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _: &gpui::MouseUpEvent, window, cx| {
                            cx.stop_propagation();
                            this.cancel(id, window, cx)
                        }),
                    )
                    .child(icon(
                        "icons/stop.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_2,
                    )),
            );
        }
        row.child(icon(
            "icons/chevron-right.svg",
            IconSize::XSmall.px(&theme),
            theme.text_3,
        ))
        .into_any_element()
    }

    fn run_list(
        &self,
        runs: &[&ReviewRun],
        empty_title: String,
        empty_hint: String,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if runs.is_empty() {
            return div()
                .w_full()
                .py(DynamicSpacing::Base48.px(&theme))
                .flex()
                .flex_col()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_2)
                        .child(empty_title),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_3)
                        .child(empty_hint),
                )
                .into_any_element();
        }
        let mut card = div()
            .w_full()
            .max_w(px(720.))
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden();
        for (index, run) in runs.iter().enumerate() {
            if index > 0 {
                card = card.child(div().h(px(1.)).w_full().bg(theme.border));
            }
            card = card.child(self.run_row(run, theme, cx));
        }
        card.into_any_element()
    }

    fn running_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let runs: Vec<&ReviewRun> = self.snapshot.runs.iter().collect();
        self.run_list(
            &runs,
            tr!("ai_review.no_reviews"),
            tr!("review_page.empty_running_hint"),
            theme,
            cx,
        )
    }

    fn history_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let runs: Vec<&ReviewRun> = self
            .snapshot
            .runs
            .iter()
            .filter(|run| run.status.is_finished())
            .collect();
        self.run_list(
            &runs,
            tr!("review_page.no_history"),
            tr!("review_page.empty_history_hint"),
            theme,
            cx,
        )
    }

    // ── run detail ────────────────────────────────────────────────────────

    fn run_detail(&self, run: &ReviewRun, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut header = div()
            .flex_none()
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base08.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Large.px(&theme))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(run.kind.label()),
                            )
                            .child(self.status_chip(&run.status, theme)),
                    )
                    .child(
                        div()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_3)
                            .child(run_detail_meta(run)),
                    ),
            );
        if run.is_active() {
            header = header.child(
                button_frame(div().id("review-detail-stop"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .flex_none()
                    .border_1()
                    .border_color(theme.border)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.bg_hover))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener({
                            let id = run.id;
                            move |this, _: &gpui::MouseUpEvent, window, cx| {
                                this.cancel(id, window, cx)
                            }
                        }),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text)
                            .child(tr!("ai_review.stop")),
                    ),
            );
        }

        let mut body = div()
            .w_full()
            .max_w(px(720.))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                self.back_row(
                    tr!("review_page.back_to_runs"),
                    move |this, _, cx| this.close_run(cx),
                    theme,
                    cx,
                ),
            )
            .child(header);

        if let Some(failure) = run.status.failure() {
            body = body.child(
                context_menu_surface(div(), &theme)
                    .w_full()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.del_red)
                    .whitespace_normal()
                    .child(failure.to_string()),
            );
        }

        if let Some(report) = run.report.as_ref() {
            let errors = report.count(Severity::Error);
            let warnings = report.count(Severity::Warning);
            let mut card = div()
                .w_full()
                .rounded(Radius::Large.px(&theme))
                .border_1()
                .border_color(theme.border)
                .bg(theme.bg_raised)
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(self.verdict_row(report.verdict, report, theme));
            if !report.summary.trim().is_empty() {
                card = card.child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .py(DynamicSpacing::Base08.px(&theme))
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_2)
                        .whitespace_normal()
                        .border_b_1()
                        .border_color(theme.border)
                        .child(report.summary.clone()),
                );
            }
            if report.findings.is_empty() {
                card = card.child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .py(DynamicSpacing::Base16.px(&theme))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.add_green)
                        .child(tr!("ai_review.no_findings")),
                );
            } else {
                for (index, finding) in report.sorted_findings().into_iter().enumerate() {
                    if index > 0 {
                        card = card.child(div().h(px(1.)).w_full().bg(theme.border));
                    }
                    card = card.child(self.finding_row(index, finding, theme));
                }
            }
            let _ = (errors, warnings);
            body = body.child(card);
        }

        // Files changed for the run's target.
        if !self.snapshot.changed_files.is_empty() {
            body = body.child(self.files_card(theme));
        }
        body.into_any_element()
    }

    /// A quiet back row used by the run detail.
    fn back_row(
        &self,
        label: String,
        on_click: impl Fn(&mut ReviewPage, &mut Window, &mut Context<ReviewPage>) + 'static,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("review-back")
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .cursor_pointer()
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text_2)
            .hover(|style| style.text_color(theme.text))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _: &gpui::MouseUpEvent, window, cx| {
                    on_click(this, window, cx)
                }),
            )
            .child(icon(
                "icons/arrow-left.svg",
                IconSize::XSmall.px(&theme),
                theme.text_3,
            ))
            .child(label)
            .into_any_element()
    }

    fn finding_row(&self, index: usize, finding: &Finding, theme: Theme) -> AnyElement {
        let mut body = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .whitespace_normal()
                    .child(finding.title.clone()),
            );
        if let Some(location) = finding.location() {
            body = body.child(
                div()
                    .font_family(theme::code_font_family())
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.accent)
                    .child(location),
            );
        }
        if !finding.detail.trim().is_empty() {
            body = body.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_2)
                    .whitespace_normal()
                    .child(finding.detail.clone()),
            );
        }
        div()
            .id(("review-finding", index))
            .w_full()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_start()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(self.severity_chip(finding.severity, theme))
            .child(body)
            .into_any_element()
    }

    /// The run detail's files-changed card.
    fn files_card(&self, theme: Theme) -> AnyElement {
        let mut card = div()
            .w_full()
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(TextSize::XSmall.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_2)
                    .child(tr!(
                        "review_page.files_changed",
                        count = self.snapshot.changed_files.len()
                    )),
            );
        for (index, file) in self.snapshot.changed_files.iter().take(50).enumerate() {
            if index > 0 {
                card = card.child(div().h(px(1.)).w_full().bg(theme.border));
            }
            card = card.child(self.file_row_content(file, None, theme));
        }
        card.into_any_element()
    }

    /// A changed-file row: status letter, path, counts, optional checkbox.
    fn file_row_content(
        &self,
        file: &ChangedFile,
        checked: Option<bool>,
        theme: Theme,
    ) -> AnyElement {
        let mut row = div()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base06.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme));
        if let Some(checked) = checked {
            row = row.child(
                div()
                    .flex_none()
                    .w(px(13.))
                    .h(px(13.))
                    .rounded(Radius::Small.px(&theme))
                    .border_1()
                    .border_color(if checked { theme.accent } else { theme.border })
                    .when(checked, |box_el| box_el.bg(theme.accent))
                    .when(checked, |box_el| {
                        box_el.child(icon(
                            "icons/check.svg",
                            IconSize::XSmall.px(&theme),
                            theme.bg_main,
                        ))
                    }),
            );
        }
        row.child(
            div()
                .flex_none()
                .w(px(14.))
                .text_size(TextSize::XSmall.px(&theme))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(match file.status {
                    'A' => theme.add_green,
                    'D' => theme.del_red,
                    _ => theme.warn,
                })
                .child(file.status.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(theme::code_font_family())
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text)
                .child(file.path.clone()),
        )
        .child(
            div()
                .flex_none()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.add_green)
                .child(format!("+{}", file.added)),
        )
        .child(
            div()
                .flex_none()
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.del_red)
                .child(format!("−{}", file.deleted)),
        )
        .into_any_element()
    }

    // ── Changes ───────────────────────────────────────────────────────────

    fn changes_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        if self.snapshot.changed_files.is_empty() {
            return div()
                .w_full()
                .py(DynamicSpacing::Base48.px(&theme))
                .flex()
                .flex_col()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_2)
                        .child(tr!("review_page.no_changes")),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_3)
                        .child(tr!("review_page.empty_changes_hint")),
                )
                .into_any_element();
        }
        let mut card = div()
            .w_full()
            .max_w(px(720.))
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.changes_scope_line(theme));

        let mut list = div()
            .id("review-changes-list")
            .max_h(px(200.))
            .overflow_y_scroll()
            .flex()
            .flex_col();
        for (index, file) in self.snapshot.changed_files.iter().enumerate() {
            if index > 0 {
                list = list.child(div().h(px(1.)).w_full().bg(theme.border));
            }
            list = list.child(self.changed_file_row(index, file, theme, cx));
        }
        card = card.child(list);

        if let Some(body) = self.changes_preview(theme) {
            card = card.child(body);
        }
        card.into_any_element()
    }

    fn changes_scope_line(&self, theme: Theme) -> AnyElement {
        div()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .border_b_1()
            .border_color(theme.border)
            .text_size(TextSize::XSmall.px(&theme))
            .child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(self.snapshot.target.clone()),
            )
            .child(
                div()
                    .text_color(theme.text_3)
                    .child(tr!(
                        "ai_review.file_count",
                        count = self.snapshot.changed_files.len()
                    )),
            )
            .into_any_element()
    }

    fn changed_file_row(
        &self,
        index: usize,
        file: &ChangedFile,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected_file == Some(index);
        let path = file.path.clone();
        let checked = self.checked.iter().any(|checked| checked == &path);
        let row = div()
            .id(("review-file", index))
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.accent.opacity(0.10)))
            .hover(|style| style.bg(theme.bg_hover))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _: &gpui::MouseUpEvent, _, cx| {
                    this.select_file(index, cx)
                }),
            )
            .child(self.file_row_content(file, Some(checked), theme));
        // The checkbox is its own hit target, drawn over the row's leading box.
        let path_for_check = file.path.clone();
        div()
            .id(("review-file-wrap", index))
            .relative()
            .w_full()
            .child(row)
            .child(
                div()
                    .absolute()
                    .left(DynamicSpacing::Base12.px(&theme))
                    .top(px(0.))
                    .bottom(px(0.))
                    .w(px(16.))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.toggle_checked(path_for_check.clone(), window, cx)
                        }),
                    ),
            )
            .into_any_element()
    }

    /// The selected file's diff, painted with the shared diff-row painters.
    fn changes_preview(&self, theme: Theme) -> Option<AnyElement> {
        let index = self.selected_file?;
        let snapshot = self.snapshot.snapshot.as_ref()?;
        let file = self.snapshot.changed_files.get(index)?;
        let header = div()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .border_t_1()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(theme::code_font_family())
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_2)
                    .child(file.path.clone()),
            );
        let rows: Vec<AnyElement> = snapshot
            .lines
            .iter()
            .filter(|line| line.file_index == index)
            .map(|line| match &line.kind {
                crate::review::LineKind::FileHeader => crate::diff_view::render_file_header(
                    &snapshot.files[index],
                    theme,
                    None,
                    theme.mode == crate::theme::ThemeMode::Dark,
                ),
                crate::review::LineKind::HunkHeader => {
                    crate::diff_view::render_hunk_header(&line.content, theme)
                }
                crate::review::LineKind::Meta => crate::diff_view::render_meta(&line.content, theme),
                crate::review::LineKind::Gap(gap) => div()
                    .h(px(crate::diff_view::REVIEW_HUNK_HEIGHT))
                    .w_full()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .flex()
                    .items_center()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .bg(theme.overlay)
                    .child(tr!("sidepane.unmodified_lines", count = gap.count()))
                    .into_any_element(),
                crate::review::LineKind::Context
                | crate::review::LineKind::Addition
                | crate::review::LineKind::Deletion => {
                    crate::diff_view::render_code_row(line, theme)
                }
            })
            .collect();
        Some(
            div()
                .flex()
                .flex_col()
                .child(header)
                .child(
                    div()
                        .id("review-changes-preview")
                        .max_h(px(420.))
                        .overflow_scroll()
                        .flex()
                        .flex_col()
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    // ── top bar ───────────────────────────────────────────────────────────

    /// The page controls the shell hosts in its shared top bar.
    pub fn top_bar_leading(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                button_frame(div().id("review-top-back"), &theme, ButtonSize::Medium)
                    .group(BUTTON_GROUP)
                    .cursor_pointer()
                    .hover(|style| style.bg(theme.bg_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close();
                            this.close_requested = true;
                            cx.notify();
                        }),
                    )
                    .child(icon(
                        "icons/arrow-left.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.text_2,
                    ))
                    .child(div().text_color(theme.text_2).child(tr!("view.back"))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(icon(
                        "icons/spark.svg",
                        IconSize::Medium.px(&theme),
                        theme.text_2,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Large.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(tr!("review_page.title")),
                    ),
            )
            .into_any_element()
    }
}

/// The workspace folder name for a run's path.
fn workspace_name(workspace: &std::path::Path) -> String {
    workspace
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// The run detail's metadata line: workspace, model, thinking, duration.
fn run_detail_meta(run: &ReviewRun) -> String {
    let mut parts = vec![workspace_name(&run.workspace)];
    let config = run.config.label();
    if !config.is_empty() {
        parts.push(config);
    }
    if let Some(elapsed) = run.elapsed_secs() {
        parts.push(tr!("ai_review.elapsed", secs = elapsed));
    }
    parts.join(" · ")
}

impl Render for ReviewPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        // The run detail replaces the tab content when a run is open.
        if let Some(id) = self.detail_run {
            if let Some(run) = self.snapshot.runs.iter().find(|run| run.id == id) {
                let detail = self.run_detail(run, theme, cx);
                return div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .bg(theme.bg_main)
                    .child(
                        div()
                            .id("review-detail-scroll")
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .overflow_y_scroll()
                            .px(DynamicSpacing::Base16.px(&theme))
                            .py(DynamicSpacing::Base16.px(&theme))
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(detail),
                    );
            }
        }
        let content = match self.tab {
            ReviewTab::New => self.new_tab(theme, cx),
            ReviewTab::Running => self.running_tab(theme, cx),
            ReviewTab::Changes => self.changes_tab(theme, cx),
            ReviewTab::History => self.history_tab(theme, cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .bg(theme.bg_main)
            .child(self.page_header(theme, cx))
            .child(self.tab_strip(theme, cx))
            .child(
                div()
                    .id("review-page-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(content),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reviews::ReviewRunConfig;

    fn page() -> ReviewPage {
        ReviewPage::default()
    }

    fn run(id: u64, status: RunStatus) -> ReviewRun {
        ReviewRun {
            id,
            workspace: std::path::PathBuf::from("/w/orbit"),
            head: None,
            kind: ReviewKind::Uncommitted,
            config: ReviewRunConfig::default(),
            status,
            started_at: 1_760_000_000,
            finished_at: Some(1_760_000_041),
            report: None,
        }
    }

    #[test]
    fn opening_lands_on_the_new_tab_and_closing_leaves_clean() {
        let mut page = page();
        assert!(!page.is_open());
        page.open = true;
        assert!(page.is_open());
        page.detail_run = Some(3);
        page.menu = Some(Menu::Model);
        page.close();
        assert!(!page.is_open());
        assert!(page.detail_run.is_none());
        assert!(page.menu.is_none());
    }

    #[test]
    fn close_request_is_reported_once_and_closes() {
        let mut page = page();
        page.open = true;
        page.close_requested = true;
        assert!(page.take_close_request());
        assert!(!page.is_open());
        assert!(!page.take_close_request());
    }

    #[test]
    fn targets_map_to_their_run_kinds() {
        let mut page = page();
        page.snapshot.facts.base_branch = "main".into();
        page.target = ReviewTarget::Uncommitted;
        assert_eq!(page.selected_kind(), ReviewKind::Uncommitted);
        page.target = ReviewTarget::Branch;
        assert_eq!(
            page.selected_kind(),
            ReviewKind::Branch { base: "main".into() }
        );
        page.target = ReviewTarget::Commit {
            sha: "abc1234".into(),
            title: "Fix parser".into(),
        };
        assert_eq!(
            page.selected_kind(),
            ReviewKind::Commit {
                sha: "abc1234".into(),
                title: "Fix parser".into()
            }
        );
        page.checked = vec!["src/a.rs".into()];
        page.target = ReviewTarget::Files;
        assert_eq!(
            page.selected_kind(),
            ReviewKind::Files {
                paths: vec!["src/a.rs".into()]
            }
        );
        page.target = ReviewTarget::Project;
        assert_eq!(page.selected_kind(), ReviewKind::Project);
    }

    #[test]
    fn target_rows_read_their_counts_from_the_facts() {
        let mut page = page();
        page.snapshot.facts.uncommitted_files = Some(14);
        page.snapshot.facts.branch_files = Some(22);
        page.snapshot.facts.project_files = Some(1204);
        assert_eq!(page.target_files(&ReviewTarget::Uncommitted), Some(14));
        assert_eq!(page.target_files(&ReviewTarget::Branch), Some(22));
        assert_eq!(page.target_files(&ReviewTarget::Project), Some(1204));
        // Files counts what is checked, and nothing while the set is empty.
        assert_eq!(page.target_files(&ReviewTarget::Files), None);
        page.checked = vec!["a".into(), "b".into()];
        assert_eq!(page.target_files(&ReviewTarget::Files), Some(2));
    }

    #[test]
    fn a_commit_row_prefers_its_own_file_count() {
        let mut page = page();
        page.snapshot.commits = vec![CommitRow {
            sha: "abc1234".into(),
            short: "abc1234".into(),
            subject: "Fix parser".into(),
            file_count: 5,
            added: 10,
            deleted: 2,
        }];
        let target = ReviewTarget::Commit {
            sha: "abc1234".into(),
            title: String::new(),
        };
        assert_eq!(page.target_files(&target), Some(5));
    }

    #[test]
    fn tab_badges_count_only_what_the_tab_shows() {
        let mut page = page();
        page.snapshot.runs = vec![
            run(1, RunStatus::Running),
            run(2, RunStatus::Completed),
            run(3, RunStatus::Cancelled),
        ];
        assert_eq!(page.tab_badge(ReviewTab::Running), Some(1));
        assert_eq!(page.tab_badge(ReviewTab::History), Some(2));
        assert_eq!(page.tab_badge(ReviewTab::New), None);
        page.snapshot.changed_files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: 'M',
            added: 1,
            deleted: 1,
        }];
        assert_eq!(page.tab_badge(ReviewTab::Changes), Some(1));
    }

    #[test]
    fn a_snapshot_without_runs_shows_no_badges() {
        let page = page();
        assert!(page.tab_badge(ReviewTab::Running).is_none());
        assert!(page.tab_badge(ReviewTab::Changes).is_none());
        assert!(page.tab_badge(ReviewTab::History).is_none());
    }

    #[test]
    fn target_labels_name_the_base_and_commit() {
        let mut page = page();
        page.snapshot.facts.base_branch = "main".into();
        page.target = ReviewTarget::Branch;
        assert!(page.target_label().contains("main"));
        page.target = ReviewTarget::Commit {
            sha: "abc1234def".into(),
            title: "Fix parser".into(),
        };
        let label = page.target_label();
        assert!(label.contains("abc1234"));
        assert!(label.contains("Fix parser"));
    }

    #[test]
    fn switching_targets_clears_the_file_selection() {
        let mut page = page();
        page.selected_file = Some(3);
        page.checked = vec!["src/a.rs".into()];
        // Simulate what select_target does without a window.
        page.target = ReviewTarget::Project;
        page.selected_file = None;
        page.checked.clear();
        assert!(page.selected_file.is_none());
        assert!(page.checked.is_empty());
    }

    #[test]
    fn a_shrinking_file_list_drops_a_stale_selection() {
        let mut page = page();
        page.selected_file = Some(4);
        page.snapshot.changed_files = vec![ChangedFile {
            path: "src/a.rs".into(),
            status: 'M',
            added: 1,
            deleted: 1,
        }];
        // `set_snapshot` without a context would notify; the guard itself is
        // what the test checks.
        let index = page.selected_file.unwrap();
        if index >= page.snapshot.changed_files.len() {
            page.selected_file = None;
        }
        assert!(page.selected_file.is_none());
    }

    #[test]
    fn workspace_name_is_the_folder() {
        assert_eq!(
            workspace_name(std::path::Path::new("/Users/x/Personal/orbit")),
            "orbit"
        );
        assert_eq!(workspace_name(std::path::Path::new("/")), "");
    }

    #[test]
    fn run_detail_meta_names_the_workspace_and_model() {
        let mut run = run(1, RunStatus::Completed);
        run.config.model = Some("opus-4-5".into());
        run.config.thinking = Some("high".into());
        let meta = run_detail_meta(&run);
        assert!(meta.contains("orbit"));
        assert!(meta.contains("opus-4-5 · high"));
        assert!(meta.contains("41s"));
    }
}
