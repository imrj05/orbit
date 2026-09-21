//! Right side pane — the workbench's second column: a
//! **Review** panel showing the workspace's git changes.
//!
//! The diff reads from a selectable [`review::Source`] — the last agent turn
//! (from [`crate::checkpoint`] snapshots), uncommitted, unstaged, staged,
//! committed, or the whole branch. The patch is parsed into a virtualized list
//! of rows with sticky file headers and expandable context gaps, colored from
//! [`crate::highlight`] tokens. A filterable changed-files tree sits alongside
//! it and hides on narrow panes.
//!
//! The pane is a GPUI [`Entity`] owned by [`crate::app::OrbitApp`], so it can
//! hold its own list state and background-job state.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, px, radians, Animation, AnimationExt, AnyElement, ClickEvent, Context,
    CursorStyle, Entity, Font, FontFeatures, FontStyle, FontWeight, Hsla, KeyDownEvent,
    ListAlignment, ListOffset, ListState, MouseDownEvent, Pixels, Render, SharedString, StyledText,
    TextAlign, TextRun, Transformation, Window,
};

use crate::app::{file_glyph, icon, nerd_font_family};
use crate::composer::ComposerInput;
use crate::git;
use crate::review::{self, ExpansionDirection, GapPosition, LineKind, Snapshot, Source};
use crate::theme::{self, Theme, ThemeMode};

/// Pane width defaults / drag clamps.
const PANE_DEFAULT_W: f32 = 460.;
const PANE_MIN_W: f32 = 300.;
/// Below this width the ±stats collapse out of the toolbar.
const STATS_MIN_PANE_W: f32 = 380.;
/// Below this width the changed-files tree is hidden (responsive).
const TREE_MIN_PANE_W: f32 = 440.;
/// Directory tree column width range.
const TREE_MIN_COL_W: f32 = 180.;
const TREE_MAX_COL_W: f32 = 240.;
/// Review diff row metrics.
const DIFF_TEXT_SIZE: f32 = 12.5;
const REVIEW_FILE_HEADER_HEIGHT: f32 = 36.;
const REVIEW_HUNK_HEIGHT: f32 = 24.;
const REVIEW_GAP_HEIGHT: f32 = 32.;

/// How long the Review refresh button spins after a click, so a fast diff
/// read still reads as acknowledged (the same floor Settings uses).
const REFRESH_FEEDBACK: Duration = Duration::from_millis(650);

/// Drag marker for the side-pane resize handle (gpui typed drag state).
pub struct SidePaneResize;

pub struct SidePane {
    /// Whether the pane is shown at all (toggled from the top bar).
    open: bool,
    /// Pane width in pixels — adjusted by dragging its left edge.
    width: Pixels,

    /// Workspace the pane operates on (kept in sync by the app).
    workspace: Option<PathBuf>,
    /// pi session id — keys the turn checkpoints behind `Last Turn`.
    session: Option<String>,
    /// Latest completed turn for this session, if any.
    latest_turn: Option<usize>,

    // ── Review ──
    review: Option<Arc<Snapshot>>,
    review_loading: bool,
    /// Set when a run settles (or the workspace changes); the diff reloads
    /// next time the pane is visible.
    review_stale: bool,
    review_error: Option<String>,
    /// Manual-refresh feedback: the header button spins until this instant,
    /// so a click is acknowledged even when the diff read finishes instantly.
    refresh_spin_until: Option<Instant>,
    /// Which git snapshot is shown; changing it reloads the diff.
    source: Source,
    /// Monotonic id so a slow load for a previous source can be discarded.
    load_generation: u64,
    /// The source filter dropdown is open.
    source_menu_open: bool,
    /// Guards against the outside-dismiss click's mouse-up re-opening the
    /// menu through the chip.
    menu_dismissed_at: Option<Instant>,
    /// Virtualized diff rows.
    diff_list: ListState,

    // ── Changed-files tree ──
    /// User toggle (still auto-hidden on narrow panes).
    tree_open: bool,
    tree_filter: Entity<ComposerInput>,
    tree_list: ListState,
    /// Directories explicitly expanded; every directory a fresh snapshot
    /// introduces is auto-expanded.
    expanded_paths: HashSet<String>,
    /// File whose diff is highlighted.
    selected_file: Option<usize>,
    /// A file to select once the next snapshot loads (Git page → Review).
    pending_select: Option<String>,
    /// Cached visible tree rows (kept in step with `tree_list`).
    tree_rows: Vec<review::TreeRow>,
    /// Keyboard cursor within the tree.
    tree_cursor: Option<usize>,
    tree_focus: gpui::FocusHandle,
    /// Cached filter text + dirty flag so the tree rebuilds only on change.
    last_tree_filter: String,
    tree_dirty: bool,
}

impl SidePane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let tree_filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("sidepane.filter_files")
                .with_key_context("Composer Picker")
        });
        Self {
            open: false,
            width: px(PANE_DEFAULT_W),
            workspace: None,
            session: None,
            latest_turn: None,
            review: None,
            review_loading: false,
            review_stale: true,
            review_error: None,
            refresh_spin_until: None,
            source: Source::default(),
            load_generation: 0,
            source_menu_open: false,
            menu_dismissed_at: None,
            diff_list: ListState::new(0, ListAlignment::Top, px(400.)),
            tree_open: true,
            tree_filter,
            tree_list: ListState::new(0, ListAlignment::Top, px(200.)),
            expanded_paths: HashSet::new(),
            selected_file: None,
            pending_select: None,
            tree_rows: Vec::new(),
            tree_cursor: None,
            tree_focus: cx.focus_handle(),
            last_tree_filter: String::new(),
            tree_dirty: true,
        }
    }

    // ── state entry points (called from the app shell) ─────────────────

    /// Show/hide the pane (top-bar toggle).
    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        if !self.open {
            self.source_menu_open = false;
        } else if self.review_stale && !self.review_loading {
            self.load_review(cx);
        }
        cx.notify();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn width(&self) -> Pixels {
        self.width
    }

    /// Whether the source filter dropdown is open (Escape closes it before
    /// falling through to `abort`).
    pub fn is_source_menu_open(&self) -> bool {
        self.source_menu_open
    }

    /// Close the source filter dropdown (Escape / app-level dismiss).
    pub fn close_source_menu(&mut self, cx: &mut Context<Self>) {
        if self.source_menu_open {
            self.source_menu_open = false;
            self.menu_dismissed_at = Some(Instant::now());
            cx.notify();
        }
    }

    /// Drag-resize from the pane's left edge.
    pub fn set_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let clamped = width.max(px(PANE_MIN_W));
        if clamped != self.width {
            self.width = clamped;
            cx.notify();
        }
    }

    /// Keep the pane's workspace in sync with the app (called every frame;
    /// cheap no-op when unchanged). A workspace change invalidates Review.
    pub fn set_workspace(&mut self, workspace: Option<PathBuf>, cx: &mut Context<Self>) {
        if workspace != self.workspace {
            self.workspace = workspace;
            self.review_stale = true;
            if self.open {
                self.load_review(cx);
            }
            cx.notify();
        }
    }

    /// Keep the session id + latest turn in sync so `Last Turn` can resolve.
    pub fn set_turn_context(
        &mut self,
        session: Option<String>,
        latest_turn: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let session_changed = session != self.session;
        let latest_changed = latest_turn != self.latest_turn;
        if !session_changed && !latest_changed {
            return;
        }
        self.session = session;
        self.latest_turn = latest_turn;
        // A session swap can strand the pane on `Last Turn`; fall back to the
        // always-available uncommitted view rather than showing an error.
        if session_changed
            && matches!(self.source, Source::LastTurn { .. })
            && self.last_turn_source().is_none()
        {
            self.source = Source::Uncommitted;
        }
        self.review_stale = true;
        if self.open && !self.review_loading {
            self.load_review(cx);
        }
        cx.notify();
    }

    /// A run settled — Review is stale; reload immediately when visible.
    pub fn mark_review_stale(&mut self, cx: &mut Context<Self>) {
        self.review_stale = true;
        if self.open && !self.review_loading {
            self.load_review(cx);
        }
    }

    /// Close the pane if it is open. Used when the Explorer opens — the two
    /// right docks are mutually exclusive.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if !self.open {
            return;
        }
        self.open = false;
        self.source_menu_open = false;
        cx.notify();
    }

    /// Open Review on the working tree's **Uncommitted** changes — the
    /// top-bar `+N -M` diff-stat chip's action.
    pub fn show_uncommitted(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        if self.source != Source::Uncommitted {
            self.source = Source::Uncommitted;
            self.source_menu_open = false;
            self.clear_diff();
        }
        self.review_stale = true;
        if !self.review_loading {
            self.load_review(cx);
        }
        cx.notify();
    }

    /// Open Review on the working tree and select `path` — the Git page's
    /// changed-file rows call this so a row opens its diff.
    pub fn show_file(&mut self, path: String, cx: &mut Context<Self>) {
        self.open = true;
        if self.source != Source::Uncommitted {
            self.source = Source::Uncommitted;
            self.clear_diff();
        }
        self.pending_select = Some(path);
        self.review_stale = true;
        if !self.review_loading {
            self.load_review(cx);
        }
        cx.notify();
    }

    /// Open the pane on Review — wired to the transcript's changed-files
    /// cards' "Review" buttons and the command palette.
    pub fn show_review(&mut self, cx: &mut Context<Self>) {
        self.open = true;
        if self.review_stale && !self.review_loading {
            self.load_review(cx);
        }
        cx.notify();
    }

    // ── Review ─────────────────────────────────────────────────────────

    /// Load the workspace's git diff off-thread.
    fn load_review(&mut self, cx: &mut Context<Self>) {
        let cwd = self
            .workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let source = self.source;
        let session = self.session.clone();
        self.load_generation = self.load_generation.wrapping_add(1);
        let generation = self.load_generation;
        self.review_loading = true;
        self.review_stale = false;
        // Paint the loading state on the click's own frame; without this the
        // spinner can be replaced by the result before it is ever drawn.
        cx.notify();
        cx.spawn(async move |this, cx| {
            let parsed = cx
                .background_executor()
                .spawn(async move {
                    let data = git::collect_review_diff(&cwd, source, session.as_deref())?;
                    Ok::<_, String>(review::parse_collected(
                        source,
                        &data.numstat,
                        &data.patch,
                        data.complete_context,
                    ))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                // A newer load (e.g. the source changed while this ran) wins.
                if this.load_generation != generation {
                    return;
                }
                this.review_loading = false;
                match parsed {
                    Ok(snapshot) => {
                        this.apply_snapshot(snapshot);
                        this.review_error = None;
                    }
                    Err(error) => {
                        if this.review.is_none() {
                            this.review_error = Some(error);
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_snapshot(&mut self, snapshot: Snapshot) {
        let previous_dirs = self.review.as_ref().map_or_else(HashSet::new, |snapshot| {
            review::directory_paths(&snapshot.files)
        });
        let previous_path = self
            .selected_file
            .and_then(|index| self.review.as_ref()?.files.get(index))
            .map(|file| file.path.clone());

        let directories = review::directory_paths(&snapshot.files);
        if self.review.is_some() {
            self.expanded_paths
                .retain(|path| directories.contains(path));
            self.expanded_paths
                .extend(directories.difference(&previous_dirs).cloned());
        } else {
            self.expanded_paths = directories;
        }
        let pending = self.pending_select.take();
        self.selected_file = pending
            .as_deref()
            .and_then(|path| snapshot.files.iter().position(|file| file.path == path))
            .or_else(|| {
                previous_path
                    .as_deref()
                    .and_then(|path| snapshot.files.iter().position(|file| file.path == path))
            })
            .or_else(|| (!snapshot.files.is_empty()).then_some(0));
        if let Some(index) = self.selected_file {
            if let Some(line) = snapshot.files.get(index).and_then(|file| file.diff_line) {
                self.diff_list.scroll_to(ListOffset {
                    item_ix: line,
                    offset_in_item: px(0.),
                });
            }
        }
        self.tree_cursor = None;
        self.diff_list.reset(snapshot.lines.len());
        self.review = Some(Arc::new(snapshot));
        self.mark_tree_dirty();
    }

    /// Recompute the visible tree rows against the snapshot + filter. The
    /// rebuild only runs when the filter text or tree structure changed, so a
    /// scroll frame is O(visible rows), not O(files).
    fn sync_tree_rows(&mut self, cx: &Context<Self>) {
        let filter = self.tree_filter.read(cx).text();
        if !self.tree_dirty && filter == self.last_tree_filter {
            return;
        }
        self.tree_dirty = false;
        self.last_tree_filter = filter.clone();
        let rows = self.review.as_ref().map_or_else(Vec::new, |snapshot| {
            review::tree_rows(&snapshot.files, &self.expanded_paths, &filter)
        });
        self.set_tree_rows(rows);
    }

    fn mark_tree_dirty(&mut self) {
        self.tree_dirty = true;
    }

    fn set_tree_rows(&mut self, rows: Vec<review::TreeRow>) {
        if rows.len() != self.tree_list.item_count() {
            self.tree_list.reset(rows.len());
        }
        self.tree_rows = rows;
    }

    fn refresh_review(&mut self, cx: &mut Context<Self>) {
        if !self.review_loading {
            self.load_review(cx);
        }
    }

    /// Refresh from the header button: same reload, plus a short minimum spin
    /// so the click is visibly acknowledged even when the diff is instant.
    fn refresh_from_button(&mut self, cx: &mut Context<Self>) {
        self.refresh_review(cx);
        self.refresh_spin_until = Some(Instant::now() + REFRESH_FEEDBACK);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REFRESH_FEEDBACK).await;
            let _ = this.update(cx, |pane, cx| {
                // A second click extends the floor; only the last timer clears.
                if pane
                    .refresh_spin_until
                    .is_some_and(|until| Instant::now() >= until)
                {
                    pane.refresh_spin_until = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    // ── source filter ──────────────────────────────────────────────────

    fn toggle_source_menu(&mut self, cx: &mut Context<Self>) {
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(at) = self.menu_dismissed_at.take() {
            if at.elapsed() < GESTURE {
                return;
            }
        }
        self.source_menu_open = !self.source_menu_open;
        cx.notify();
    }

    fn dismiss_source_menu(&mut self, cx: &mut Context<Self>) {
        self.close_source_menu(cx);
    }

    fn set_source(&mut self, source: Source, cx: &mut Context<Self>) {
        self.source_menu_open = false;
        self.menu_dismissed_at = Some(Instant::now());
        if source == self.source {
            cx.notify();
            return;
        }
        self.source = source;
        self.clear_diff();
        self.load_review(cx);
        cx.notify();
    }

    /// Drop the rendered diff + tree so a source change paints a fresh load.
    fn clear_diff(&mut self) {
        self.selected_file = None;
        self.tree_cursor = None;
        self.review = None;
        self.review_error = None;
        self.expanded_paths.clear();
        self.mark_tree_dirty();
        self.set_tree_rows(Vec::new());
        self.diff_list.reset(0);
    }

    /// Open Review on the **Last Turn** git diff — what the transcript's
    /// changed-files cards' Review buttons call, so the panel shows only the
    /// changes that turn made rather than the whole working tree.
    pub fn show_review_turn(&mut self, turn: Option<usize>, cx: &mut Context<Self>) {
        self.open = true;
        let target = turn
            .filter(|turn| *turn > 0)
            .map(|turn_count| Source::LastTurn { turn_count });
        match target {
            Some(target) => {
                if self.source != target {
                    self.source = target;
                    self.clear_diff();
                }
            }
            None => {
                // No turn checkpoint yet — fall back to the working tree
                // instead of stranding the panel on an unavailable source.
                if matches!(self.source, Source::LastTurn { .. }) {
                    self.source = Source::Uncommitted;
                    self.clear_diff();
                }
            }
        }
        self.review_stale = true;
        if !self.review_loading {
            self.load_review(cx);
        }
        cx.notify();
    }

    /// The latest completed turn this session can diff (drives the menu's
    /// `Last Turn` availability).
    fn last_turn_source(&self) -> Option<Source> {
        self.latest_turn
            .filter(|turn| *turn > 0)
            .map(|turn_count| Source::LastTurn { turn_count })
    }

    fn source_label(&self, source: Source) -> String {
        match source {
            Source::LastTurn { turn_count } if self.latest_turn == Some(turn_count) => {
                tr!("sidepane.last_turn")
            }
            Source::LastTurn { turn_count } => tr!("sidepane.turn_n", count = turn_count),
            Source::Uncommitted => tr!("sidepane.uncommitted"),
            Source::Unstaged => tr!("git_panel.unstaged"),
            Source::Staged => tr!("git_panel.staged"),
            Source::Committed => tr!("sidepane.committed"),
            Source::Branch => tr!("sidepane.branch"),
        }
    }

    // ── changed-files tree ─────────────────────────────────────────────

    fn toggle_tree(&mut self, cx: &mut Context<Self>) {
        self.tree_open = !self.tree_open;
        cx.notify();
    }

    fn toggle_dir(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.expanded_paths.remove(&path) {
            self.expanded_paths.insert(path);
        }
        self.mark_tree_dirty();
        self.sync_tree_rows(cx);
        cx.notify();
    }

    fn select_file(&mut self, file_index: usize, cx: &mut Context<Self>) {
        self.selected_file = Some(file_index);
        if let Some(line) = self
            .review
            .as_ref()
            .and_then(|snapshot| snapshot.files.get(file_index))
            .and_then(|file| file.diff_line)
        {
            self.diff_list.scroll_to(ListOffset {
                item_ix: line,
                offset_in_item: px(0.),
            });
        }
        cx.notify();
    }

    fn on_filter_cancel(
        &mut self,
        _: &crate::PickerCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_source_menu(cx);
        self.tree_filter.update(cx, |input, cx| input.clear(cx));
        self.sync_tree_rows(cx);
        cx.notify();
    }

    fn on_tree_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let rows_len = self.tree_rows.len();
        if rows_len == 0 {
            return;
        }
        match key {
            "down" => {
                let next = self.tree_cursor.map_or(0, |ix| (ix + 1).min(rows_len - 1));
                self.move_tree_cursor(next, cx);
            }
            "up" => {
                let next = self.tree_cursor.map_or(0, |ix| ix.saturating_sub(1));
                self.move_tree_cursor(next, cx);
            }
            "enter" | "space" => {
                if let Some(index) = self.tree_cursor {
                    match self.tree_rows.get(index).cloned() {
                        Some(review::TreeRow::Directory { path, .. }) => {
                            self.toggle_dir(path, cx);
                        }
                        Some(review::TreeRow::File { file_index, .. }) => {
                            self.select_file(file_index, cx);
                        }
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn move_tree_cursor(&mut self, index: usize, cx: &mut Context<Self>) {
        self.tree_cursor = Some(index);
        self.tree_list.scroll_to_reveal_item(index);
        cx.notify();
    }

    // ── rendering ──────────────────────────────────────────────────────

    fn body(&mut self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        self.sync_tree_rows(cx);
        let truncated = self
            .review
            .as_ref()
            .is_some_and(|snapshot| snapshot.truncated);
        let (added, removed) = self
            .review
            .as_ref()
            .map(|snapshot| (snapshot.additions, snapshot.deletions))
            .unwrap_or((0, 0));
        let compact = self.width < px(STATS_MIN_PANE_W);
        let tree_available = self.width >= px(TREE_MIN_PANE_W);
        let tree_visible = tree_available && self.tree_open;

        // Header row: title + tree toggle + refresh + close.
        let head = div()
            .h(px(40.))
            .flex()
            .items_center()
            .gap_1()
            .pl(px(12.))
            // The pane owns the window's right edge, so where the app paints
            // the caption its buttons land here and the header stops short.
            .pr(px(if crate::platform::draws_window_controls() {
                crate::platform::WINDOW_CONTROLS_W
            } else {
                6.
            }))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme.ui_px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(tr!("sidepane.review")),
            )
            .children(tree_available.then(|| {
                div()
                    .id("review-tree-toggle")
                    .p_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_tree(cx)))
                    .child(icon(
                        "icons/folder.svg",
                        14.,
                        if self.tree_open {
                            theme.text
                        } else {
                            theme.text_3
                        },
                    ))
            }))
            .child(
                div()
                    .id("review-refresh")
                    .p_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.refresh_from_button(cx);
                    }))
                    .child(
                        if self.review_loading || self.refresh_spin_until.is_some() {
                            spinner("review-spinner", theme)
                        } else {
                            icon("icons/refresh.svg", 13., theme.text_3).into_any_element()
                        },
                    ),
            )
            .child(
                div()
                    .id("pane-close")
                    .p_1()
                    .rounded_sm()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)))
                    .child(icon("icons/x.svg", 14., theme.text_3)),
            );

        // Toolbar: the source filter chip + live ±stats + refresh.
        let toolbar = div()
            .h(px(40.))
            .flex()
            .items_center()
            .gap_2()
            .px(px(10.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .id("review-source")
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(if self.source_menu_open {
                        theme.border_strong
                    } else {
                        theme.border
                    })
                    .bg(theme.bg_raised)
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_source_menu(cx)),
                    )
                    .child(icon("icons/file-diff.svg", 12., theme.text_3))
                    .child(
                        div()
                            .max_w(px(120.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(theme.ui_px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(self.source_label(self.source)),
                    )
                    .child(icon("icons/chevron-down.svg", 10., theme.text_3)),
            )
            .child(div().flex_1())
            .when(truncated, |row| {
                row.child(
                    div()
                        .text_size(theme.ui_px(11.))
                        .text_color(theme.warn)
                        .child(tr!("sidepane.partial")),
                )
            })
            .children((!compact).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(theme.ui_px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .child(div().text_color(theme.add_green).child(format!("+{added}")))
                    .child(div().text_color(theme.del_red).child(format!("-{removed}")))
            }));

        let content = self.render_content(theme, tree_visible, cx);

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(head)
            .child(toolbar)
            .child(content)
            .into_any_element()
    }

    fn render_content(
        &mut self,
        theme: Theme,
        tree_visible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let diff = if self.review_loading && self.review.is_none() {
            centered_message(theme, &tr!("sidepane.loading_changes"), None).into_any_element()
        } else if let Some(error) = self.review_error.as_deref() {
            centered_message(theme, &tr!("sidepane.changes_unavailable"), Some(error))
                .into_any_element()
        } else if let Some(snapshot) = self.review.clone() {
            if snapshot.files.is_empty() {
                let empty = self.source.empty_description();
                centered_message(theme, &tr!("sidepane.no_changes"), Some(&empty))
                    .into_any_element()
            } else {
                self.render_diff(snapshot, theme, cx)
            }
        } else {
            centered_message(theme, &tr!("sidepane.no_changes"), None).into_any_element()
        };

        let mut content = div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_row()
            .child(diff);
        if tree_visible && self.review.is_some() && self.review_error.is_none() {
            content = content.child(self.render_tree(theme, cx));
        }
        content.into_any_element()
    }

    fn render_diff(
        &self,
        snapshot: Arc<Snapshot>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity().downgrade();
        let list_el = gpui::list(self.diff_list.clone(), move |index, _window, cx| {
            entity
                .upgrade()
                .map(|entity| entity.update(cx, |this, cx| this.render_diff_line(index, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .size_full();

        let nerd = nerd_font_family(cx);
        let dark = theme.mode == ThemeMode::Dark;
        let sticky = self.render_sticky_header(&snapshot, theme, nerd.as_ref(), dark);
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .child(list_el)
            .children(sticky)
            .into_any_element()
    }

    fn render_sticky_header(
        &self,
        snapshot: &Snapshot,
        theme: Theme,
        nerd: Option<&SharedString>,
        dark: bool,
    ) -> Option<AnyElement> {
        let scroll_top = self.diff_list.logical_scroll_top();
        let (header_index, next_header_index) = snapshot.file_headers_around(scroll_top.item_ix)?;
        let needs_sticky = header_index < scroll_top.item_ix
            || (header_index == scroll_top.item_ix && scroll_top.offset_in_item > px(0.));
        if !needs_sticky {
            return None;
        }
        let line = snapshot.lines.get(header_index)?;
        let file = snapshot.files.get(line.file_index)?;
        let top_offset = next_header_index
            .and_then(|next_header_index| {
                let bounds = self.diff_list.bounds_for_item(next_header_index)?;
                let viewport = self.diff_list.viewport_bounds();
                let y_in_viewport = bounds.origin.y - viewport.origin.y;
                (y_in_viewport < bounds.size.height).then_some(y_in_viewport - bounds.size.height)
            })
            .unwrap_or(px(0.));
        Some(
            div()
                .absolute()
                .top(top_offset)
                .left_0()
                .w_full()
                .child(render_file_header(file, theme, nerd, dark))
                .into_any_element(),
        )
    }

    fn render_diff_line(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(snapshot) = self.review.as_ref() else {
            return div().into_any_element();
        };
        let Some(line) = snapshot.lines.get(index) else {
            return div().into_any_element();
        };
        let Some(file) = snapshot.files.get(line.file_index) else {
            return div().into_any_element();
        };
        let theme = *theme::get(cx);
        match &line.kind {
            LineKind::FileHeader => {
                let nerd = nerd_font_family(cx);
                let dark = theme.mode == ThemeMode::Dark;
                render_file_header(file, theme, nerd.as_ref(), dark)
            }
            LineKind::Gap(gap) => self.render_gap(index, gap.clone(), theme, cx),
            LineKind::HunkHeader => render_hunk_header(&line.content, theme),
            LineKind::Meta => render_meta(&line.content, theme),
            LineKind::Context | LineKind::Addition | LineKind::Deletion => {
                render_code_row(line, theme)
            }
        }
    }

    fn render_gap(
        &self,
        index: usize,
        gap: review::Gap,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chunked = gap.count() > review::DEFAULT_EXPANSION_LINE_COUNT as u32;
        let directions: &[ExpansionDirection] = match (gap.position, chunked) {
            (GapPosition::Leading, _) => &[ExpansionDirection::End],
            (GapPosition::Trailing, _) => &[ExpansionDirection::Start],
            (GapPosition::Between, false) => &[ExpansionDirection::Both],
            (GapPosition::Between, true) => &[ExpansionDirection::Start, ExpansionDirection::End],
        };
        let expandable = gap.is_expandable();
        let two = directions.len() > 1;
        let mut gutter = div()
            .w(px(46.))
            .h_full()
            .flex_none()
            .flex()
            .when(two, |gutter| gutter.flex_col())
            .border_r_1()
            .border_color(theme.border)
            .bg(theme.overlay);
        if expandable {
            for (button_index, direction) in directions.iter().copied().enumerate() {
                gutter = gutter.child(
                    div()
                        .id(gpui::ElementId::Name(
                            format!("review-gap-{}-{index}-{button_index}", gap.id).into(),
                        ))
                        .flex_1()
                        .h_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(two && button_index == 0, |button| {
                            button
                                .h(px(16.))
                                .flex_none()
                                .border_b_1()
                                .border_color(theme.border)
                        })
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.overlay_strong))
                        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                            let direction = if event.modifiers().shift {
                                ExpansionDirection::All
                            } else {
                                direction
                            };
                            this.expand_gap(index, direction, cx);
                        }))
                        .child(icon(gap_icon(direction), 10., theme.text_3)),
                );
            }
        }
        let label = div()
            .id(gpui::ElementId::Name(
                format!("review-gap-label-{}", gap.id).into(),
            ))
            .flex_1()
            .h_full()
            .min_w_0()
            .px(px(12.))
            .flex()
            .items_center()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_size(theme.ui_px(11.5))
            .text_color(theme.text_3)
            .bg(theme.overlay)
            .when(expandable, |label| {
                label
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay_strong))
                    .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                        let direction = if event.modifiers().shift {
                            ExpansionDirection::All
                        } else {
                            ExpansionDirection::Both
                        };
                        this.expand_gap(index, direction, cx);
                    }))
            })
            .child(tr!(
                "sidepane.unmodified_lines",
                count = gap.count()
            ));

        div()
            .h(px(REVIEW_GAP_HEIGHT))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .child(gutter)
            .child(label)
            .into_any_element()
    }

    fn expand_gap(
        &mut self,
        line_index: usize,
        direction: ExpansionDirection,
        cx: &mut Context<Self>,
    ) {
        let expansion = self
            .review
            .as_mut()
            .and_then(|snapshot| Arc::make_mut(snapshot).expand_gap(line_index, direction));
        if let Some(expansion) = expansion {
            self.diff_list
                .splice(line_index..line_index + 1, expansion.replacement_count);
            cx.notify();
        }
    }

    fn render_tree(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let tree_el = gpui::list(self.tree_list.clone(), move |index, _window, cx| {
            entity
                .upgrade()
                .map(|entity| entity.update(cx, |this, cx| this.render_tree_row(index, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .size_full()
        .py(px(4.));

        let column_w = (f32::from(self.width) * 0.42).clamp(TREE_MIN_COL_W, TREE_MAX_COL_W);
        let filter_row = div()
            .h(px(40.))
            .flex_none()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .border_b_1()
            .border_color(theme.border)
            .child(icon("icons/search.svg", 13., theme.text_3))
            .child(self.tree_filter.clone());

        div()
            .w(px(column_w))
            .flex_none()
            .h_full()
            .min_h_0()
            .border_l_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .child(filter_row)
            .child(
                div()
                    .id("review-tree")
                    .track_focus(&self.tree_focus)
                    .key_context("ReviewDiffTree")
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        this.on_tree_key_down(event, cx)
                    }))
                    .child(tree_el),
            )
            .into_any_element()
    }

    fn render_tree_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.tree_rows.get(index).cloned() else {
            return div().h(px(30.)).into_any_element();
        };
        let Some(snapshot) = self.review.as_ref() else {
            return div().h(px(30.)).into_any_element();
        };
        let theme = *theme::get(cx);
        let cursor = self.tree_cursor == Some(index);
        match row {
            review::TreeRow::Directory {
                path,
                name,
                depth,
                expanded,
            } => div()
                .w_full()
                .h(px(30.))
                .px(px(6.))
                .flex()
                .items_center()
                .child(
                    div()
                        .id(gpui::ElementId::Name(format!("review-dir-{path}").into()))
                        .h(px(26.))
                        .flex_1()
                        .min_w_0()
                        .pl(px(7. + depth as f32 * 14.))
                        .pr(px(7.))
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .when(cursor, |row| row.bg(theme.overlay_strong))
                        .when(!cursor, |row| row.hover(|row| row.bg(theme.overlay)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            window.focus(&this.tree_focus);
                            this.tree_cursor = Some(index);
                            this.toggle_dir(path.clone(), cx);
                        }))
                        .child(icon(
                            if expanded {
                                "icons/chevron-down.svg"
                            } else {
                                "icons/chevron-right.svg"
                            },
                            10.,
                            theme.text_3,
                        ))
                        .child(icon("icons/folder.svg", 13., theme.text_3))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(theme.ui_px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_2)
                                .child(name),
                        ),
                )
                .into_any_element(),
            review::TreeRow::File { file_index, depth } => {
                let Some(file) = snapshot.files.get(file_index) else {
                    return div().h(px(30.)).into_any_element();
                };
                let name = file
                    .path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&file.path)
                    .to_owned();
                let selected = self.selected_file == Some(file_index);
                let (status, status_color) = match file.status {
                    review::FileStatus::Added => ("A", theme.add_green),
                    review::FileStatus::Deleted => ("D", theme.del_red),
                    review::FileStatus::Binary => ("B", theme.warn),
                    review::FileStatus::Modified => ("M", theme.warn),
                };
                let nerd = nerd_font_family(cx);
                let dark = theme.mode == ThemeMode::Dark;
                let fallback = icon("icons/file.svg", 13., theme.text_3).into_any_element();
                let glyph = file_glyph(&file.path, dark, nerd.as_ref(), 13., fallback);
                div()
                    .w_full()
                    .h(px(30.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id(gpui::ElementId::Name(
                                format!("review-file-{file_index}").into(),
                            ))
                            .h(px(26.))
                            .flex_1()
                            .min_w_0()
                            .pl(px(23. + depth as f32 * 14.))
                            .pr(px(7.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .cursor_pointer()
                            .when(selected && cursor, |row| row.bg(theme.overlay_strong))
                            .when(selected ^ cursor, |row| row.bg(theme.overlay))
                            .when(!selected && !cursor, |row| {
                                row.hover(|row| row.bg(theme.overlay))
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.tree_focus);
                                this.tree_cursor = Some(index);
                                this.select_file(file_index, cx);
                            }))
                            .child(glyph)
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_size(theme.ui_px(12.5))
                                    .text_color(if selected { theme.text } else { theme.text_2 })
                                    .child(name),
                            )
                            .child(
                                div()
                                    .w(px(18.))
                                    .h(px(18.))
                                    .flex_none()
                                    .rounded(px(4.))
                                    .border_1()
                                    .border_color(status_color.opacity(0.65))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(theme.ui_px(11.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(status_color)
                                    .child(status),
                            ),
                    )
                    .into_any_element()
            }
        }
    }

    /// The source filter dropdown, painted over the pane body.
    fn source_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.source_menu_open {
            return None;
        }
        let mut menu = div()
            .id("review-source-menu")
            .absolute()
            .top(px(84.))
            .left(px(10.))
            .w(px(200.))
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
                cx.listener(|this, _: &MouseDownEvent, _, cx| this.dismiss_source_menu(cx)),
            );

        let last_turn = self.last_turn_source();
        menu = menu.child(source_row(
            &tr!("sidepane.last_turn"),
            last_turn.is_some(),
            last_turn.is_some() && last_turn == Some(self.source),
            last_turn,
            theme,
            cx,
        ));
        menu = menu.child(separator(theme));
        for choice in [Source::Uncommitted, Source::Unstaged, Source::Staged] {
            menu = menu.child(source_row(
                &self.source_label(choice),
                true,
                choice == self.source,
                Some(choice),
                theme,
                cx,
            ));
        }
        menu = menu.child(separator(theme));
        for choice in [Source::Committed, Source::Branch] {
            menu = menu.child(source_row(
                &self.source_label(choice),
                true,
                choice == self.source,
                Some(choice),
                theme,
                cx,
            ));
        }
        Some(menu.into_any_element())
    }
}

fn source_row(
    label: &str,
    enabled: bool,
    selected: bool,
    choice: Option<Source>,
    theme: Theme,
    cx: &mut Context<SidePane>,
) -> AnyElement {
    let row = div()
        .id(gpui::ElementId::Name(
            format!("review-source-{label}").into(),
        ))
        .h(px(28.))
        .mx(px(4.))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(theme.ui_px(12.))
        .when(enabled, |row| row.cursor_pointer())
        .when(selected, |row| row.bg(theme.active))
        .when(enabled && !selected, |row| {
            row.hover(|s| s.bg(theme.overlay))
        })
        .when(enabled && choice.is_some(), |row| {
            row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(choice) = choice {
                    this.set_source(choice, cx);
                }
            }))
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(if !enabled {
                    theme.text_3
                } else if selected {
                    theme.active_fg
                } else {
                    theme.text_2
                })
                .child(label.to_string()),
        )
        .when(!enabled, |row| {
            row.child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(10.))
                    .text_color(theme.text_3)
                    .child(tr!("sidepane.no_turns_yet")),
            )
        })
        .when(selected, |row| {
            row.child(icon("icons/check.svg", 11., theme.accent))
        });
    row.into_any_element()
}

fn separator(theme: Theme) -> AnyElement {
    div()
        .h(px(1.))
        .my(px(4.))
        .mx(px(8.))
        .bg(theme.border)
        .into_any_element()
}

// ── diff row rendering ─────────────────────────────────────────────────────

/// Sticky/normal file header: icon, path, +additions, -deletions.
fn render_file_header(
    file: &review::File,
    theme: Theme,
    nerd: Option<&SharedString>,
    dark: bool,
) -> AnyElement {
    let fallback = icon("icons/file.svg", 13., theme.text_3).into_any_element();
    let glyph = file_glyph(&file.path, dark, nerd, 13., fallback);
    div()
        .w_full()
        .min_w_0()
        .h(px(REVIEW_FILE_HEADER_HEIGHT))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.bg_raised)
        .child(glyph)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(theme::code_font_family())
                .text_size(theme.ui_px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_2)
                .child(file.path.clone()),
        )
        .child(
            div()
                .text_size(theme.ui_px(12.))
                .text_color(theme.add_green)
                .child(format!("+{}", file.additions)),
        )
        .child(
            div()
                .text_size(theme.ui_px(12.))
                .text_color(theme.del_red)
                .child(format!("-{}", file.deletions)),
        )
        .into_any_element()
}

fn render_hunk_header(content: &str, theme: Theme) -> AnyElement {
    let gutter_w = diff_gutter_width();
    div()
        .min_h(px(REVIEW_HUNK_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .text_color(theme.text_3)
        .child(
            div()
                .w(px(gutter_w))
                .min_h(px(REVIEW_HUNK_HEIGHT))
                .flex_none()
                .border_r_1()
                .border_color(theme.border)
                .bg(theme.overlay),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .px(px(12.))
                .py(px(4.))
                .overflow_hidden()
                .whitespace_normal()
                .bg(theme.overlay)
                .child(content.to_string()),
        )
        .into_any_element()
}

fn render_meta(content: &str, theme: Theme) -> AnyElement {
    let gutter_w = diff_gutter_width();
    div()
        .min_h(px(REVIEW_HUNK_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .text_color(theme.text_3)
        .child(
            div()
                .w(px(gutter_w))
                .min_h(px(REVIEW_HUNK_HEIGHT))
                .flex_none(),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .py(px(4.))
                .pr(px(10.))
                .overflow_hidden()
                .whitespace_normal()
                .child(content.to_string()),
        )
        .into_any_element()
}

fn diff_gutter_width() -> f32 {
    (DIFF_TEXT_SIZE * 3. + 14.).round()
}

fn diff_row_height() -> f32 {
    (DIFF_TEXT_SIZE * 1.5).round()
}

/// One context/addition/deletion row: a single line-number gutter (the new
/// number, falling back to the old one) and syntax-coloured code.
fn render_code_row(line: &review::Line, theme: Theme) -> AnyElement {
    let row_height = diff_row_height();
    let (body_bg, gutter_bg, edge, number_color) = match line.kind {
        LineKind::Addition => (
            Some(theme.add_green.opacity(body_wash(theme))),
            Some(theme.add_green.opacity(gutter_wash(theme))),
            Some(theme.add_green),
            theme.add_green,
        ),
        LineKind::Deletion => (
            Some(theme.del_red.opacity(body_wash(theme))),
            Some(theme.del_red.opacity(gutter_wash(theme))),
            Some(theme.del_red),
            theme.del_red,
        ),
        _ => (None, None, None, theme.text_3),
    };
    let shown_line = line.new_line.or(line.old_line);
    let number = shown_line.map(|n| n.to_string()).unwrap_or_default();
    let content = code_text(line, theme);
    div()
        .w_full()
        .min_w_0()
        .min_h(px(row_height))
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .when_some(edge, |row, edge| row.border_l_2().border_color(edge))
        .child(
            div()
                .w(px(diff_gutter_width()))
                .min_h(px(row_height))
                .flex_none()
                .pr(px(9.))
                .flex()
                .justify_end()
                .border_r_1()
                .border_color(theme.border)
                .text_color(number_color)
                .when_some(gutter_bg, |gutter, bg| gutter.bg(bg))
                .child(number),
        )
        .child(
            div()
                .min_h(px(row_height))
                .min_w_0()
                .flex_1()
                .pl(px(12.))
                .pr(px(10.))
                .overflow_hidden()
                .whitespace_normal()
                .when_some(body_bg, |body, bg| body.bg(bg))
                .child(content),
        )
        .into_any_element()
}

fn body_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.20
    } else {
        0.12
    }
}

fn gutter_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.15
    } else {
        0.09
    }
}

/// Build syntax-colored text for one diff line.
fn code_text(line: &review::Line, theme: Theme) -> StyledText {
    let base = theme.text_2;
    let font = mono_font();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut offset = 0usize;
    for token in &line.tokens {
        let start = token.range.start.min(line.content.len());
        let end = token.range.end.min(line.content.len());
        if start > offset {
            runs.push(run(start - offset, base, &font));
        }
        if end > start {
            runs.push(run(end - start, theme.token_color(token.class), &font));
        }
        offset = offset.max(end);
    }
    if offset < line.content.len() {
        runs.push(run(line.content.len() - offset, base, &font));
    }
    if runs.is_empty() {
        runs.push(run(line.content.len(), base, &font));
    }
    StyledText::new(line.content.clone()).with_runs(runs)
}

fn run(len: usize, color: Hsla, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn mono_font() -> Font {
    Font {
        family: theme::code_font_family(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}

fn gap_icon(direction: ExpansionDirection) -> &'static str {
    match direction {
        // A leading gap reveals context below it; a trailing gap above.
        ExpansionDirection::Start | ExpansionDirection::Both | ExpansionDirection::All => {
            "icons/chevron-down.svg"
        }
        ExpansionDirection::End => "icons/chevron-up.svg",
    }
}

// ── shared helpers ─────────────────────────────────────────────────────────

fn spinner(id: &'static str, theme: Theme) -> AnyElement {
    gpui::svg()
        .path("icons/loader.svg")
        .flex_none()
        .size(px(13.))
        .text_color(theme.text_3)
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

fn centered_message(theme: Theme, title: &str, detail: Option<&str>) -> AnyElement {
    let mut column = div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(16.))
        .pb(px(32.))
        .child(
            div()
                .text_size(theme.ui_px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title.to_string()),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(px(6.))
                .max_w(px(300.))
                .text_align(TextAlign::Center)
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_3)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}

impl Source {
    /// Reader-facing empty-state copy for each source.
    fn empty_description(self) -> String {
        match self {
            Source::LastTurn { .. } => tr!("sidepane.empty_last_turn"),
            Source::Uncommitted => tr!("sidepane.empty_uncommitted"),
            Source::Unstaged => tr!("sidepane.empty_unstaged"),
            Source::Staged => tr!("sidepane.empty_staged"),
            Source::Committed => tr!("sidepane.empty_committed"),
            Source::Branch => tr!("sidepane.empty_branch"),
        }
    }
}

impl Render for SidePane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = *theme::get(cx);
        div()
            .id("side-pane")
            .relative()
            .flex_none()
            .w(self.width)
            .h_full()
            .bg(theme.bg_main)
            .border_l_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .id("side-pane-resize-handle")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-3.))
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|style| style.bg(theme.accent.opacity(0.4)))
                    .on_drag(SidePaneResize, |_, _, _, cx| cx.new(|_| DragGhost)),
            )
            .child(self.body(theme, cx))
            .children(self.source_menu(theme, cx))
            .on_action(cx.listener(Self::on_filter_cancel))
            .into_any_element()
    }
}

/// An invisible drag ghost — resizing leaves no floating preview.
struct DragGhost;

impl Render for DragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}
