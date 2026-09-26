//! The Review page — a first-class destination for AI review, separate from
//! the Review pane.
//!
//! The pane stays a pure diff viewer: it never starts a reviewer, shows a
//! finding, or owns review state. **This page is the only AI review UI.** It
//! has three working tabs plus history:
//!
//! - **New review** — pick a target (uncommitted changes, a branch, a commit,
//!   chosen files, the whole project), see its file count, and start a run.
//! - **Running** — every live run with its progress and Stop; finished runs
//!   stay in place with a *Review completed* status and their findings.
//! - **Changes** — the changed files of the selected target (the preview
//!   surface, and the file picker for the Files target).
//! - **History** — older runs across all workspaces.
//!
//! The page is deliberately dumb, like the Usage page: the app computes a
//! [`ReviewPageSnapshot`] each frame and the page only lays it out. Actions go
//! back through [`ReviewPageAction`] callbacks, so the page never borrows the
//! app or the review store.

use std::rc::Rc;

use gpui::{
    div, prelude::*, px, AnyElement, App, Context, FontWeight, Hsla, MouseButton, Render, Window,
};

use crate::ai_review::{Finding, ReviewKind, Severity};
use crate::app::helpers::{button_frame, icon, press, BUTTON_GROUP};
use crate::reviews::{ReviewRun, RunStatus};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, Radius, TextSize};
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
    pub const ALL: [ReviewTab; 4] = [
        Self::New,
        Self::Running,
        Self::Changes,
        Self::History,
    ];

    fn label(self) -> String {
        match self {
            Self::New => tr!("review_page.tab_new"),
            Self::Running => tr!("review_page.tab_running"),
            Self::Changes => tr!("review_page.tab_changes"),
            Self::History => tr!("review_page.tab_history"),
        }
    }
}

/// What the user picked to review on the New review tab.
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
    /// Files touched by uncommitted changes.
    pub uncommitted_files: Option<usize>,
    /// Added/deleted lines for the uncommitted changes.
    pub uncommitted_added: u64,
    pub uncommitted_deleted: u64,
    /// Files this branch adds over its base.
    pub branch_files: Option<usize>,
    /// The base the branch row compares against (`main`).
    pub base_branch: String,
    /// Files the selected commit touches, when one is selected.
    pub commit_files: Option<usize>,
    /// Tracked files in the workspace.
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

/// The app-computed state the page renders. Rebuilt each frame.
#[derive(Clone, Default, PartialEq)]
pub struct ReviewPageSnapshot {
    /// Label of the target the changed files belong to, for the Changes tab's
    /// scope line.
    pub target: String,
    /// The open workspace's folder name, for the page header.
    pub workspace: String,
    /// Facts for the target rows.
    pub facts: TargetFacts,
    /// Recent commits, newest first.
    pub commits: Vec<CommitRow>,
    /// Files of the currently selected target (the Changes tab).
    pub changed_files: Vec<ChangedFile>,
    /// Every run, newest first — running and history alike.
    pub runs: Vec<ReviewRun>,
    /// The model/thinking the next run starts on (`Opus 4.5 · high`).
    pub config_label: String,
    /// A run-level failure with no run to attach to (no workspace open).
    pub error: Option<String>,
}

/// One changed file in the Changes tab.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    /// Git's status letter (`M`, `A`, `D`, `R`).
    pub status: char,
    pub added: u64,
    pub deleted: u64,
}

/// Actions the page sends back to the app. The page never touches the store.
pub enum ReviewPageAction {
    /// Start a run for the selected target, with the model/thinking config.
    Start(ReviewKind),
    /// Stop a run.
    Cancel(u64),
    /// The selected target changed; the app collects its changed files for
    /// the Changes tab.
    TargetChanged(ReviewKind),
}

/// App-provided handler, rebuilt each frame like the pane's review opener.
pub type ReviewAction = Rc<dyn Fn(ReviewPageAction, &mut Window, &mut App)>;

pub struct ReviewPage {
    tab: ReviewTab,
    target: ReviewTarget,
    /// The commit the Commit target points at (kept so the picker survives a
    /// tab switch).
    selected_commit: Option<(String, String)>,
    snapshot: ReviewPageSnapshot,
    action: Option<ReviewAction>,
    /// Whether the page is the active main-area surface.
    open: bool,
    /// The target the app last collected changed files for, so we ask once per
    /// change rather than every frame.
    collected_kind: Option<ReviewKind>,
    /// The Back control sets this; the app drains it on the next heartbeat and
    /// closes the page (it has the window and the app context there).
    close_requested: bool,
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
        }
    }
}

impl ReviewPage {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self::default()
    }

    /// Whether the page owns the main area.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Enter the page. Always lands on New review: that is the action.
    pub fn open(&mut self, window: &mut Window, cx: &mut App) {
        self.open = true;
        self.tab = ReviewTab::New;
        self.collected_kind = None;
        self.notify_target(window, cx);
    }

    /// Leave the page.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Whether the Back control asked to leave; the app clears this and closes
    /// the page on the next heartbeat.
    pub fn take_close_request(&mut self) -> bool {
        std::mem::take(&mut self.close_requested)
    }

    /// Mirror the app's state into the page. Only a real change notifies, so
    /// the per-frame sync cannot loop back on itself.
    pub fn set_snapshot(&mut self, snapshot: ReviewPageSnapshot, cx: &mut Context<Self>) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }

    pub fn set_action(&mut self, action: ReviewAction) {
        self.action = Some(action);
    }

    fn show_tab(&mut self, tab: ReviewTab, cx: &mut Context<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        cx.notify();
    }

    fn select_target(&mut self, target: ReviewTarget, window: &mut Window, cx: &mut Context<Self>) {
        if self.target == target {
            return;
        }
        self.target = target;
        self.notify_target(window, cx);
        cx.notify();
    }

    /// Tell the app which target's changed files to collect. Called when the
    /// page opens and whenever the target changes.
    fn notify_target(&mut self, window: &mut Window, cx: &mut App) {
        let kind = self
            .target
            .to_kind(&self.snapshot.facts.base_branch, Vec::new());
        if self.collected_kind.as_ref() == Some(&kind) {
            return;
        }
        self.collected_kind = Some(kind.clone());
        if let Some(action) = self.action.as_ref() {
            action(ReviewPageAction::TargetChanged(kind), window, cx);
        }
    }

    fn select_commit(
        &mut self,
        sha: String,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_commit = Some((sha.clone(), title.clone()));
        self.target = ReviewTarget::Commit { sha, title };
        self.notify_target(window, cx);
        cx.notify();
    }

    /// The `ReviewKind` the selected target maps to.
    pub fn target_kind(&self) -> Option<ReviewKind> {
        Some(
            self.target
                .to_kind(&self.snapshot.facts.base_branch, Vec::new()),
        )
    }

    /// A short label for the selected target, for section headers.
    pub fn target_label(&self) -> String {
        match &self.target {
            ReviewTarget::Uncommitted => tr!("ai_review.kind_uncommitted"),
            ReviewTarget::Branch => tr!(
                "ai_review.kind_branch_base",
                base = self.snapshot.facts.base_branch.clone()
            ),
            ReviewTarget::Commit { sha, .. } => {
                let short: String = sha.chars().take(7).collect();
                tr!("ai_review.kind_commit", sha = short)
            }
            ReviewTarget::Files => tr!("review_page.target_files"),
            ReviewTarget::Project => tr!("ai_review.review_project"),
        }
    }

    fn start(&self, window: &mut Window, cx: &mut App) {
        let Some(action) = self.action.as_ref() else {
            return;
        };
        let kind = self.target.to_kind(&self.snapshot.facts.base_branch, Vec::new());
        action(ReviewPageAction::Start(kind), window, cx);
    }

    fn cancel(&self, id: u64, window: &mut Window, cx: &mut App) {
        let Some(action) = self.action.as_ref() else {
            return;
        };
        action(ReviewPageAction::Cancel(id), window, cx);
    }

    /// The number of files the selected target covers, when known.
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
            ReviewTarget::Files => Some(self.snapshot.changed_files.len()),
            ReviewTarget::Project => self.snapshot.facts.project_files,
        }
    }

    // ── pieces ────────────────────────────────────────────────────────────

    fn tab_badge(&self, tab: ReviewTab) -> Option<usize> {
        match tab {
            ReviewTab::Running => {
                let count = self.snapshot.runs.iter().filter(|run| run.is_active()).count();
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

    fn tab_strip(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut strip = div()
            .flex_none()
            .h(px(38.))
            .px(DynamicSpacing::Base16.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .border_b_1()
            .border_color(theme.border);
        for tab in ReviewTab::ALL {
            let active = self.tab == tab;
            let badge = self.tab_badge(tab);
            let mut row = div()
                .id(("review-tab", ReviewTab::ALL.iter().position(|t| *t == tab).unwrap_or(0)))
                .relative()
                .h_full()
                .px(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .cursor_pointer()
                .text_size(TextSize::Small.px(&theme))
                .text_color(if active { theme.text } else { theme.text_2 })
                .when(active, |row| row.font_weight(FontWeight::SEMIBOLD))
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
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.accent)
                        .child(count.to_string()),
                );
            }
            if active {
                row = row.child(
                    div()
                        .absolute()
                        .left(DynamicSpacing::Base06.px(&theme))
                        .right(DynamicSpacing::Base06.px(&theme))
                        .bottom(px(0.))
                        .h(px(2.))
                        .rounded_t(Radius::Small.px(&theme))
                        .bg(theme.accent),
                );
            }
            strip = strip.child(row);
        }
        strip.into_any_element()
    }

    fn target_row(
        &self,
        row: &TargetRow,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.target.key() == row.key;
        let selected = active;
        let target = row.target.clone();
        let files = self.target_files(&target);
        let detail = match files {
            Some(count) => tr!("ai_review.file_count", count = count),
            None => String::new(),
        };
        div()
            .id(("review-target", row.index))
            .w_full()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .rounded(Radius::Medium.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .cursor_pointer()
            .when(selected, |row| row.bg(theme.accent.opacity(0.09)))
            .hover(|style| style.bg(theme.bg_hover))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _: &gpui::MouseUpEvent, window, cx| {
                    this.select_target(target.clone(), window, cx)
                }),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(13.))
                    .h(px(13.))
                    .rounded_full()
                    .border_1()
                    .border_color(if selected { theme.accent } else { theme.border })
                    .when(selected, |dot| {
                        dot.child(div().m(px(2.5)).rounded_full().bg(theme.accent))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text)
                    .child(row.label.clone()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(detail),
            )
            .into_any_element()
    }

    fn verdict_color(&self, verdict: crate::ai_review::Verdict, theme: Theme) -> Hsla {
        match verdict {
            crate::ai_review::Verdict::Correct => theme.add_green,
            crate::ai_review::Verdict::NeedsAttention => theme.del_red,
        }
    }

    fn finding_row(&self, index: usize, finding: &Finding, theme: Theme) -> AnyElement {
        let (tint, label) = match finding.severity {
            Severity::Error => (theme.del_red, tr!("ai_review.severity_error")),
            Severity::Warning => (theme.warn, tr!("ai_review.severity_warning")),
            Severity::Info => (theme.text_3, tr!("ai_review.severity_info")),
        };
        let mut body = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base02.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text)
                    .child(finding.title.clone()),
            );
        if let Some(location) = finding.location() {
            body = body.child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(location),
            );
        }
        if !finding.detail.trim().is_empty() {
            body = body.child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
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
            .border_t_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_none()
                    .mt(px(1.))
                    .px(DynamicSpacing::Base06.px(&theme))
                    .rounded(Radius::Small.px(&theme))
                    .bg(tint.opacity(0.14))
                    .text_size(TextSize::XSmall.px(&theme))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(tint)
                    .child(label),
            )
            .child(body)
            .into_any_element()
    }

    fn run_card(&self, run: &ReviewRun, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let id = run.id;
        let (status_color, status_bg) = match &run.status {
            RunStatus::Completed => (theme.add_green, theme.add_green.opacity(0.12)),
            RunStatus::Failed(_) => (theme.del_red, theme.del_red.opacity(0.12)),
            RunStatus::Cancelled => (theme.text_3, theme.text_3.opacity(0.10)),
            RunStatus::Queued | RunStatus::Running => (theme.accent, theme.accent.opacity(0.12)),
        };
        // One card: the run line, then its findings when it has any.
        let mut card = div()
            .w_full()
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden();

        let mut header = div()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .flex_none()
                    .px(DynamicSpacing::Base08.px(&theme))
                    .py(px(2.))
                    .rounded(Radius::Full.px(&theme))
                    .bg(status_bg)
                    .text_size(TextSize::XSmall.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(status_color)
                    .child(run.status.label()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text)
                    .child(run.kind.label()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(run.summary_line()),
            );
        if run.is_active() {
            header = header.child(
                button_frame(
                    div().id(("review-stop", id as usize)),
                    &theme,
                    ButtonSize::Compact,
                )
                .group(BUTTON_GROUP)
                .flex_none()
                .border_1()
                .border_color(theme.border)
                .cursor_pointer()
                .hover(|style| style.bg(theme.bg_hover))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _: &gpui::MouseUpEvent, window, cx| {
                        this.cancel(id, window, cx)
                    }),
                )
                .child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_2)
                        .child(tr!("ai_review.stop")),
                ),
            );
        }
        card = card.child(header);

        if let Some(failure) = run.status.failure() {
            card = card.child(
                div()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .pb(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.del_red)
                    .whitespace_normal()
                    .child(failure.to_string()),
            );
        }
        if let Some(report) = run.report.as_ref() {
            if !report.summary.trim().is_empty() {
                card = card.child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .pb(DynamicSpacing::Base08.px(&theme))
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_2)
                        .whitespace_normal()
                        .child(report.summary.clone()),
                );
            }
            if report.findings.is_empty() {
                card = card.child(
                    div()
                        .px(DynamicSpacing::Base12.px(&theme))
                        .pb(DynamicSpacing::Base08.px(&theme))
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.add_green)
                        .child(tr!("ai_review.no_findings")),
                );
            } else {
                let verdict = div()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base06.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .w(px(8.))
                            .h(px(8.))
                            .rounded_full()
                            .bg(self.verdict_color(report.verdict, theme)),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(report.verdict.label()),
                    );
                card = card.child(verdict);
                for (index, finding) in report.findings.iter().enumerate() {
                    card = card.child(self.finding_row(index, finding, theme));
                }
            }
        }
        card.into_any_element()
    }

    // ── tabs ──────────────────────────────────────────────────────────────

    fn new_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut column = div()
            .w_full()
            .max_w(px(660.))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme));

        if let Some(error) = self.snapshot.error.as_ref() {
            column = column.child(
                div()
                    .w_full()
                    .rounded(Radius::Large.px(&theme))
                    .border_1()
                    .border_color(theme.del_red.opacity(0.4))
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.del_red)
                    .child(error.clone()),
            );
        }

        // ── target card ──
        let mut card = div()
            .w_full()
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden();

        // Changes summary line (uncommitted, always meaningful).
        let files = self.snapshot.facts.uncommitted_files.unwrap_or(0);
        if files > 0 {
            let summary = div()
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .border_b_1()
                .border_color(theme.border)
                .text_size(TextSize::XSmall.px(&theme))
                .text_color(theme.text_2)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(tr!("ai_review.files_changed", count = files)),
                )
                .child(
                    div()
                        .text_color(theme.add_green)
                        .child(format!("+{}", self.snapshot.facts.uncommitted_added)),
                )
                .child(
                    div()
                        .text_color(theme.del_red)
                        .child(format!("−{}", self.snapshot.facts.uncommitted_deleted)),
                );
            card = card.child(summary);
        }

        let base = if self.snapshot.facts.base_branch.is_empty() {
            "main".to_string()
        } else {
            self.snapshot.facts.base_branch.clone()
        };
        let rows = vec![
            TargetRow {
                index: 0,
                key: "uncommitted",
                label: tr!("ai_review.kind_uncommitted"),
                target: ReviewTarget::Uncommitted,
            },
            TargetRow {
                index: 1,
                key: "branch",
                label: tr!("ai_review.kind_branch_base", base = base),
                target: ReviewTarget::Branch,
            },
            TargetRow {
                index: 2,
                key: "commit",
                label: tr!("review_page.target_commit"),
                target: self
                    .selected_commit
                    .clone()
                    .map(|(sha, title)| ReviewTarget::Commit { sha, title })
                    .unwrap_or(ReviewTarget::Commit {
                        sha: String::new(),
                        title: String::new(),
                    }),
            },
            TargetRow {
                index: 3,
                key: "files",
                label: tr!("review_page.target_files"),
                target: ReviewTarget::Files,
            },
            TargetRow {
                index: 4,
                key: "project",
                label: tr!("ai_review.review_project"),
                target: ReviewTarget::Project,
            },
        ];
        let mut body = div()
            .px(DynamicSpacing::Base06.px(&theme))
            .py(DynamicSpacing::Base06.px(&theme))
            .flex()
            .flex_col()
            .gap(px(2.));
        for row in &rows {
            body = body.child(self.target_row(row, theme, cx));
            // The commit target is selected: expand the inline commit list.
            if row.key == "commit" && self.target.key() == "commit" {
                body = body.child(self.commit_list(theme, cx));
            }
        }
        card = card.child(body);

        // Start row: quiet config chip + the page's only accent button.
        let start = div()
            .px(DynamicSpacing::Base12.px(&theme))
            .pb(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(self.snapshot.config_label.clone()),
            )
            .child(div().flex_1())
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
            );
        card = card.child(start);
        column = column.child(card);
        column.into_any_element()
    }

    fn commit_list(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .ml(DynamicSpacing::Base32.px(&theme))
            .mr(DynamicSpacing::Base12.px(&theme))
            .mb(DynamicSpacing::Base06.px(&theme))
            .pl(DynamicSpacing::Base12.px(&theme))
            .border_l_1()
            .border_color(theme.border)
            .flex()
            .flex_col();
        if self.snapshot.commits.is_empty() {
            return list
                .child(
                    div()
                        .py(DynamicSpacing::Base06.px(&theme))
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_3)
                        .child(tr!("review_page.no_commits")),
                )
                .into_any_element();
        }
        for (index, commit) in self.snapshot.commits.iter().take(8).enumerate() {
            let selected = self
                .selected_commit
                .as_ref()
                .is_some_and(|(sha, _)| sha == &commit.sha);
            let sha = commit.sha.clone();
            let title = commit.subject.clone();
            list = list.child(
                div()
                    .id(("review-commit", index))
                    .py(DynamicSpacing::Base04.px(&theme))
                    .pl(DynamicSpacing::Base08.px(&theme))
                    .rounded(Radius::Small.px(&theme))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .when(selected, |row| row.bg(theme.accent.opacity(0.08)))
                    .hover(|style| style.bg(theme.bg_hover))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, _: &gpui::MouseUpEvent, window, cx| {
                            this.select_commit(sha.clone(), title.clone(), window, cx)
                        }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.accent)
                            .child(commit.short.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text)
                            .child(commit.subject.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_3)
                            .child(tr!("ai_review.file_count", count = commit.file_count)),
                    ),
            );
        }
        list.into_any_element()
    }

    fn running_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut column = div()
            .w_full()
            .max_w(px(660.))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme));
        if self.snapshot.runs.is_empty() {
            return self.empty_state(
                tr!("ai_review.no_reviews"),
                tr!("review_page.empty_running_hint"),
                theme,
            );
        }
        for run in &self.snapshot.runs {
            column = column.child(self.run_card(run, theme, cx));
        }
        column.into_any_element()
    }

    fn changes_tab(&self, theme: Theme) -> AnyElement {
        if self.snapshot.changed_files.is_empty() {
            return self.empty_state(
                tr!("review_page.no_changes"),
                tr!("review_page.empty_changes_hint"),
                theme,
            );
        }
        let mut card = div()
            .w_full()
            .max_w(px(660.))
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden();
        for (index, file) in self.snapshot.changed_files.iter().enumerate() {
            card = card.child(
                div()
                    .id(("review-file", index))
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base06.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .when(index > 0, |row| {
                        row.border_t_1().border_color(theme.border)
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(14.))
                            .text_size(TextSize::XSmall.px(&theme))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.warn)
                            .child(file.status.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
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
                    ),
            );
        }
        card.into_any_element()
    }

    fn history_tab(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let finished: Vec<&ReviewRun> = self
            .snapshot
            .runs
            .iter()
            .filter(|run| run.status.is_finished())
            .collect();
        if finished.is_empty() {
            return self.empty_state(
                tr!("review_page.no_history"),
                tr!("review_page.empty_history_hint"),
                theme,
            );
        }
        let mut column = div()
            .w_full()
            .max_w(px(660.))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme));
        for run in finished {
            column = column.child(self.run_card(run, theme, cx));
        }
        column.into_any_element()
    }

    fn empty_state(&self, title: String, hint: String, theme: Theme) -> AnyElement {
        div()
            .w_full()
            .py(px(48.))
            .flex()
            .flex_col()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_2)
                    .child(title),
            )
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_3)
                    .child(hint),
            )
            .into_any_element()
    }

    /// The page header controls the shell hosts in its shared top bar: the
    /// Back affordance (which closes the page through the app callback) and
    /// the page title.
    pub fn top_bar_leading(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                press(
                    button_frame(div().id("review-top-back"), &theme, ButtonSize::Medium)
                        .group(BUTTON_GROUP)
                        .cursor_pointer()
                        .hover(|style| style.bg(theme.bg_hover)),
                )
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.close();
                    this.close_requested = true;
                    cx.notify();
                }))
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

/// One row in the New review target list.
struct TargetRow {
    index: usize,
    key: &'static str,
    label: String,
    target: ReviewTarget,
}

impl Render for ReviewPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let content = match self.tab {
            ReviewTab::New => self.new_tab(theme, cx),
            ReviewTab::Running => self.running_tab(theme, cx),
            ReviewTab::Changes => self.changes_tab(theme),
            ReviewTab::History => self.history_tab(theme, cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .bg(theme.bg_main)
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

