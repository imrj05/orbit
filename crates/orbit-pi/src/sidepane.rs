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
    div, prelude::*, px, AnyElement, ClickEvent, Context, CursorStyle, Entity, FontWeight,
    KeyDownEvent, ListAlignment, ListOffset, ListState, MouseDownEvent, Pixels, Render,
    ScrollHandle, StatefulInteractiveElement, Window,
};

use crate::app::{
    button_frame, context_menu_entry, context_menu_separator, context_menu_surface, empty_state,
    file_glyph, icon, icon_button_frame, nerd_font_family, picker_search_frame, refresh_glyph,
    EmptyFill, BUTTON_GROUP, PRESS_DIM,
};
use crate::composer::ComposerInput;
use crate::diff_view::{
    diff_gutter_width, render_code_row, render_file_header, render_hunk_header, render_meta,
    render_split_row, SplitPairing,
};
use crate::git;
use crate::review::{self, ExpansionDirection, GapPosition, LineKind, Snapshot, Source};
use crate::theme::tokens::{context_menu, input, popover, ButtonSize, IconSize, Radius, TextSize};
use crate::theme::{self, Theme, ThemeMode};
use crate::usage::tooltip::Tooltip;

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
/// The minimized pane's rail width: one icon column, still a resize handle's
/// grab away from coming back.
const PANE_RAIL_W: f32 = 44.;
/// Review diff row metrics. The row painters live in [`crate::diff_view`], so
/// the pane and the Review page's preview share them; only the gap height is
/// the pane's alone (the page's preview lists whole files).
const REVIEW_GAP_HEIGHT: f32 = 32.;
/// The pane's header and toolbar rows; the dropdowns hang off their buttons.
const PANE_ROW_H: f32 = 40.;

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
    /// The unwrapped diff's horizontal scroller. The pane reads it while
    /// rendering to keep the file header fixed at the viewport's edges.
    h_scroll: ScrollHandle,
    /// Diff presentation: wrap long rows and side-by-side layout.
    wrap: bool,
    split: bool,
    /// Files whose diff rows are hidden; only their headers draw. Empty means
    /// every changed file is expanded (the loaded state).
    collapsed_files: HashSet<usize>,
    /// Unified-line → side-by-side mapping, rebuilt whenever the line stream
    /// changes (load, gap expansion, file collapse).
    split_pairing: SplitPairing,
    /// Widest row in monospace cells, for an unwrapped row's scroll range.
    content_cells: usize,
    /// The window width Full width expands to — set by the app every render,
    /// so it tracks a sidebar toggle or a window resize.
    available_width: Pixels,
    /// The width Full width / Minimize return to.
    restore_width: Pixels,
    full_width: bool,
    minimized: bool,

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
        let width = px(crate::layout::sidepane_width()
            .unwrap_or(PANE_DEFAULT_W)
            .max(PANE_MIN_W));
        Self {
            open: false,
            width,
            available_width: width,
            restore_width: width,
            full_width: false,
            minimized: false,
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
            h_scroll: ScrollHandle::new(),
            wrap: true,
            split: false,
            collapsed_files: HashSet::new(),
            split_pairing: SplitPairing::default(),
            content_cells: 0,
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

    /// Whether the pane has taken the window's full width (the chat column
    /// and the sessions sidebar yield to it).
    pub fn is_full_width(&self) -> bool {
        self.full_width
    }

    /// Leave Full width and restore the docked width, without touching
    /// Minimize. The app calls this when a feature page opens or the sessions
    /// sidebar is toggled — both need the window back.
    pub fn leave_full_width(&mut self, cx: &mut Context<Self>) {
        if !self.full_width {
            return;
        }
        self.full_width = false;
        let width = self.restore_width.max(px(PANE_MIN_W));
        self.apply_width(width, cx);
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

    /// Drag-resize from the pane's left edge. A drag leaves Full width /
    /// Minimize and becomes the width every later restore returns to.
    pub fn set_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        self.full_width = false;
        self.minimized = false;
        let clamped = width.max(px(PANE_MIN_W));
        if clamped != self.width {
            self.width = clamped;
            self.restore_width = clamped;
            crate::layout::set_sidepane_width(f32::from(clamped));
            cx.notify();
        }
    }

    /// The window width the pane may fill — the app sets it every render, so
    /// Full width tracks a sidebar toggle or a window resize live, and a
    /// docked pane never outgrows the space left beside the sidebar.
    pub fn set_available_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let width = width.max(px(PANE_MIN_W));
        let changed = (self.available_width - width).abs() >= px(0.5);
        if changed {
            self.available_width = width;
        }
        if self.full_width {
            // Full width fills the ceiling, so it follows a window resize.
            if changed {
                self.apply_width(width, cx);
            }
            return;
        }
        // A window that shrank under a docked pane tightens it to fit; the
        // user's stored width is untouched and comes back with a wider window.
        if !self.minimized && self.width > self.available_width {
            self.apply_width(self.available_width, cx);
        }
    }

    fn apply_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        if width != self.width {
            self.width = width;
            cx.notify();
        }
    }

    /// The width Full width / Minimize return to: the pane's width before the
    /// first transient mode, not the transient width itself.
    fn remember_restore_width(&mut self) {
        if !self.full_width && !self.minimized {
            self.restore_width = self.width;
        }
    }

    /// Expand the pane to the window's full width (the chat column and the
    /// sessions sidebar yield to it), or restore the width the reader had
    /// dialed in.
    pub(crate) fn toggle_full_width(&mut self, cx: &mut Context<Self>) {
        if self.full_width {
            self.leave_full_width(cx);
            return;
        }
        self.remember_restore_width();
        self.minimized = false;
        self.full_width = true;
        let width = self.available_width.max(px(PANE_MIN_W));
        self.apply_width(width, cx);
    }

    /// Collapse the pane to a rail that keeps its restore control in reach.
    /// From Full width the first step is back to the docked pane, not the
    /// rail: minimizing a full-page diff means giving the window back, not
    /// hiding the panel.
    fn toggle_minimize(&mut self, cx: &mut Context<Self>) {
        if self.minimized {
            self.minimized = false;
            let width = self.restore_width.max(px(PANE_MIN_W));
            self.apply_width(width, cx);
            return;
        }
        if self.full_width {
            self.leave_full_width(cx);
            return;
        }
        self.remember_restore_width();
        self.minimized = true;
        self.source_menu_open = false;
        self.apply_width(px(PANE_RAIL_W), cx);
    }

    /// Toggle wrapped and unwrapped diff rows.
    fn toggle_wrap(&mut self, cx: &mut Context<Self>) {
        self.wrap = !self.wrap;
        cx.notify();
    }

    /// Toggle unified and side-by-side rows. Heights change without a width
    /// change, so the list must re-measure; the reader keeps their line.
    fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.split = !self.split;
        self.remeasure_diff_rows();
        cx.notify();
    }

    /// Collapse every changed file to its header, or expand them all back.
    fn toggle_all_files(&mut self, cx: &mut Context<Self>) {
        let Some(files) = self.review.as_ref().map(|review| review.files.len()) else {
            return;
        };
        if files == 0 {
            return;
        }
        if self.collapsed_files.is_empty() {
            self.collapsed_files.extend(0..files);
        } else {
            self.collapsed_files.clear();
        }
        self.remeasure_diff_rows();
        cx.notify();
    }

    /// Hide one file's diff rows, or bring them back.
    fn toggle_file(&mut self, file_index: usize, cx: &mut Context<Self>) {
        if !self.collapsed_files.remove(&file_index) {
            self.collapsed_files.insert(file_index);
        }
        self.remeasure_diff_rows();
        cx.notify();
    }

    /// Force the virtualized list to re-measure every row after a height-only
    /// change (collapsing files, switching layouts), keeping the reader's row.
    fn remeasure_diff_rows(&mut self) {
        let top = self.diff_list.logical_scroll_top();
        self.diff_list.reset(self.diff_list.item_count());
        self.diff_list.scroll_to(top);
    }

    /// Keep the selected file's header in view across a line-count change.
    fn scroll_to_selected_file(&mut self) {
        if let Some(line) = self
            .selected_file
            .and_then(|index| self.review.as_ref()?.files.get(index)?.diff_line)
        {
            self.diff_list.scroll_to(ListOffset {
                item_ix: line,
                offset_in_item: px(0.),
            });
        }
    }

    /// Rebuild the split pairing after the line stream changes.
    fn rebuild_split_pairing(&mut self) {
        self.split_pairing = self
            .review
            .as_ref()
            .map_or_else(SplitPairing::default, |review| {
                SplitPairing::for_lines(&review.lines)
            });
    }

    /// Refresh what a view toggle reads: how wide an unwrapped row is and how
    /// unified rows pair into side-by-side ones.
    fn rebuild_view_metrics(&mut self) {
        match self.review.as_ref() {
            Some(review) => self.content_cells = review.max_content_cells(),
            None => self.content_cells = 0,
        }
        self.rebuild_split_pairing();
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
        // A pane that comes back later must not come back as a rail or a
        // full-width sheet; restore the width the reader dialed in.
        self.minimized = false;
        self.full_width = false;
        self.width = self.restore_width.max(px(PANE_MIN_W));
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
        self.collapsed_files.clear();
        self.rebuild_view_metrics();
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
        self.collapsed_files.clear();
        self.rebuild_view_metrics();
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
        // Picking a file is an ask to see it; a collapsed row would scroll to
        // its header and show nothing.
        if self.collapsed_files.remove(&file_index) {
            self.remeasure_diff_rows();
        }
        self.scroll_to_selected_file();
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

    fn body(&mut self, window: &Window, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
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

        // Header row: title + tree toggle + refresh + the panel-window
        // controls (full width, minimize, close).
        let head = div()
            .h(px(PANE_ROW_H))
            .flex()
            .items_center()
            .gap_1()
            .pl(px(12.))
            // The top bar above owns the caption buttons; the dock below it
            // never shares their row.
            .pr(px(6.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(tr!("sidepane.review")),
            )
            .children(tree_available.then(|| {
                icon_button_frame(div().id("review-tree-toggle"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .active(|s| s.opacity(PRESS_DIM))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_tree(cx)))
                    .child(icon(
                        "icons/folder.svg",
                        ButtonSize::Default.icon_size().px(&theme),
                        if self.tree_open {
                            theme.text
                        } else {
                            theme.text_3
                        },
                    ))
            }))
            .child(
                icon_button_frame(div().id("review-refresh"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .active(|s| s.opacity(PRESS_DIM))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.refresh_from_button(cx);
                    }))
                    .child(refresh_glyph(
                        "review-spinner",
                        ButtonSize::Default.icon_size().px(&theme),
                        self.review_loading || self.refresh_spin_until.is_some(),
                        theme.text_3,
                        theme,
                    )),
            )
            .child(self.view_toggle(
                "review-full-width",
                if self.full_width {
                    "icons/arrow-shrink.svg"
                } else {
                    "icons/arrow-expand.svg"
                },
                self.full_width,
                true,
                if self.full_width {
                    tr!("sidepane.restore_width")
                } else {
                    tr!("sidepane.full_width")
                },
                Self::toggle_full_width,
                theme,
                cx,
            ))
            .child(self.view_toggle(
                "review-minimize",
                "icons/chevrons-right.svg",
                false,
                true,
                tr!("sidepane.minimize"),
                Self::toggle_minimize,
                theme,
                cx,
            ))
            .child(
                icon_button_frame(div().id("pane-close"), &theme, ButtonSize::Default)
                    .group(BUTTON_GROUP)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .active(|s| s.opacity(PRESS_DIM))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close(cx)))
                    .child(icon(
                        "icons/x.svg",
                        ButtonSize::Default.icon_size().px(&theme),
                        theme.text_3,
                    )),
            );

        // Toolbar: the source filter chip, the diff view toggles, and the
        // live ±stats.
        let toolbar = div()
            .h(px(PANE_ROW_H))
            .flex()
            .items_center()
            .gap_2()
            .px(px(10.))
            .border_b_1()
            .border_color(theme.border)
            .child(
                button_frame(div().id("review-source"), &theme, ButtonSize::Medium)
                    .group(BUTTON_GROUP)
                    .border_1()
                    .border_color(if self.source_menu_open {
                        theme.border_strong
                    } else {
                        theme.border
                    })
                    .bg(theme.bg_raised)
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.bg_hover))
                    .active(|s| s.opacity(PRESS_DIM))
                    .on_click(
                        cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_source_menu(cx)),
                    )
                    .child(icon(
                        "icons/file-diff.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.text_3,
                    ))
                    .child(
                        div()
                            .max_w(px(120.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(self.source_label(self.source)),
                    )
                    .child(icon(
                        "icons/chevron-down.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_3,
                    )),
            )
            .child(div().flex_1())
            .when(truncated, |row| {
                row.child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.warn)
                        .child(tr!("sidepane.partial")),
                )
            })
            .child(
                self.view_toggle(
                    "review-files-toggle",
                    if self.collapsed_files.is_empty() {
                        "icons/collapse-all.svg"
                    } else {
                        "icons/expand-all.svg"
                    },
                    false,
                    self.review
                        .as_ref()
                        .is_some_and(|review| !review.files.is_empty()),
                    if self.collapsed_files.is_empty() {
                        tr!("sidepane.collapse_all")
                    } else {
                        tr!("sidepane.expand_all")
                    },
                    Self::toggle_all_files,
                    theme,
                    cx,
                ),
            )
            .child(self.view_toggle(
                "review-wrap",
                "icons/text-wrap.svg",
                self.wrap,
                true,
                if self.wrap {
                    tr!("sidepane.unwrap_lines")
                } else {
                    tr!("sidepane.wrap_lines")
                },
                Self::toggle_wrap,
                theme,
                cx,
            ))
            .child(self.view_toggle(
                "review-split",
                if self.split {
                    "icons/unified-view.svg"
                } else {
                    "icons/side-by-side.svg"
                },
                self.split,
                true,
                if self.split {
                    tr!("sidepane.unified_view")
                } else {
                    tr!("sidepane.split_view")
                },
                Self::toggle_split,
                theme,
                cx,
            ))
            .children((!compact).then(|| {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .child(div().text_color(theme.add_green).child(format!("+{added}")))
                    .child(div().text_color(theme.del_red).child(format!("-{removed}")))
            }));

        let content = self.render_content(window, theme, tree_visible, cx);

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

    /// One icon-only view toggle. The glyph and tooltip name the action, the
    /// ink marks the active mode, and an inert toggle stays visible so the
    /// toolbar never reflows around it.
    #[allow(clippy::too_many_arguments)]
    fn view_toggle(
        &self,
        id: &'static str,
        icon_path: &'static str,
        active: bool,
        enabled: bool,
        tooltip: String,
        action: fn(&mut SidePane, &mut Context<SidePane>),
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = if !enabled {
            theme.text_3.opacity(0.4)
        } else if active {
            theme.text
        } else {
            theme.text_3
        };
        let button = icon_button_frame(div().id(id), &theme, ButtonSize::Default)
            .group(BUTTON_GROUP)
            .child(icon(
                icon_path,
                ButtonSize::Default.icon_size().px(&theme),
                color,
            ));
        if !enabled {
            return button.into_any_element();
        }
        button
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_hover))
            .active(|style| style.opacity(PRESS_DIM))
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| action(this, cx)))
            .into_any_element()
    }

    /// The minimized pane: a slim rail holding the one control that brings
    /// the panel back — nothing else fits legibly at this width.
    fn render_rail(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(8.))
            .gap(px(6.))
            .child(self.view_toggle(
                "review-restore",
                "icons/chevrons-left.svg",
                false,
                true,
                tr!("sidepane.restore_panel"),
                Self::toggle_minimize,
                theme,
                cx,
            ))
            .child(icon(
                "icons/file-diff.svg",
                IconSize::Small.px(&theme),
                theme.text_3,
            ))
            .into_any_element()
    }

    fn render_content(
        &mut self,
        window: &Window,
        theme: Theme,
        tree_visible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let diff = if self.review_loading && self.review.is_none() {
            empty_state(
                theme,
                &tr!("sidepane.loading_changes"),
                None,
                EmptyFill::Grow,
            )
        } else if let Some(error) = self.review_error.as_deref() {
            empty_state(
                theme,
                &tr!("sidepane.changes_unavailable"),
                Some(error),
                EmptyFill::Grow,
            )
        } else if let Some(snapshot) = self.review.clone() {
            if snapshot.files.is_empty() {
                let empty = self.source.empty_description();
                empty_state(
                    theme,
                    &tr!("sidepane.no_changes"),
                    Some(&empty),
                    EmptyFill::Grow,
                )
            } else {
                self.render_diff(window, snapshot, theme, cx)
            }
        } else {
            empty_state(theme, &tr!("sidepane.no_changes"), None, EmptyFill::Grow)
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
        window: &Window,
        snapshot: Arc<Snapshot>,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entity = cx.entity().downgrade();
        // Unwrapped rows want their natural width. The list gets a minimum
        // equal to the widest row and a scroll container lets the pointer pan
        // it; the file header is counter-shifted by the pan so its name and
        // counts stay fixed at the viewport's edges (see `pinned_row`), and
        // the sticky header sits outside the scroller and never pans.
        let min_w = if self.wrap {
            px(0.)
        } else {
            self.unwrapped_content_width(window, theme)
        };
        let list_el = gpui::list(self.diff_list.clone(), move |index, _window, cx| {
            entity
                .upgrade()
                .map(|entity| entity.update(cx, |this, cx| this.render_diff_line(index, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .h_full()
        .w_full()
        .min_w(min_w);
        let rows = if self.wrap {
            list_el.into_any_element()
        } else {
            div()
                .id("review-diff-scroll")
                .size_full()
                .overflow_scroll()
                .track_scroll(&self.h_scroll)
                .child(list_el)
                .into_any_element()
        };

        let sticky = self.render_sticky_header(&snapshot, theme, cx);
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .overflow_hidden()
            .child(rows)
            .children(sticky)
            .into_any_element()
    }

    /// A row that must not pan with unwrapped content. The file header keeps
    /// its name and counts at the viewport's edges: the row is sized to the
    /// scroller's visible width and counter-shifted by its horizontal offset.
    /// The shift lives on an inner wrapper — the virtualized list paints item
    /// roots at its own origins, so an inset on the root itself is ignored.
    fn pinned_row(&self, row: AnyElement) -> AnyElement {
        if self.wrap {
            return row;
        }
        let offset = self.h_scroll.offset().x;
        let viewport = self.h_scroll.bounds().size.width;
        div()
            .w_full()
            .child(div().w(viewport).left(-offset).child(row))
            .into_any_element()
    }

    /// The horizontal extent an unwrapped row needs: the widest row in
    /// monospace cells at the diff font's advance, plus the row chrome. Split
    /// view gives each half that extent, so it doubles.
    fn unwrapped_content_width(&self, window: &Window, theme: Theme) -> Pixels {
        let cell = crate::diff_view::code_cell_width(window, &theme);
        let row = cell * self.content_cells.max(1) as f32 + px(diff_gutter_width() + 22.);
        if self.split {
            row * 2. + px(1.)
        } else {
            row
        }
    }

    /// The pinned copy of the current file's header. It lives outside the
    /// horizontal scroller, so it never pans: the name and counts stay fixed
    /// at the viewport's edges while the code slides beneath.
    fn render_sticky_header(
        &self,
        snapshot: &Snapshot,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let scroll_top = self.diff_list.logical_scroll_top();
        let (header_index, next_header_index) = snapshot.file_headers_around(scroll_top.item_ix)?;
        let needs_sticky = header_index < scroll_top.item_ix
            || (header_index == scroll_top.item_ix && scroll_top.offset_in_item > px(0.));
        if !needs_sticky {
            return None;
        }
        let line = snapshot.lines.get(header_index)?;
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
                .child(self.file_header_row(line.file_index, true, theme, cx))
                .into_any_element(),
        )
    }

    /// One changed file's header, made actionable: its chevron says whether
    /// the file's rows follow, and a click toggles them. `sticky` separates
    /// the pinned copy's element id from the in-list row's, so a partially
    /// scrolled header never registers two live handlers on one id.
    fn file_header_row(
        &self,
        file_index: usize,
        sticky: bool,
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(file) = self
            .review
            .as_ref()
            .and_then(|review| review.files.get(file_index))
            .cloned()
        else {
            return div().into_any_element();
        };
        let collapsed = self.collapsed_files.contains(&file_index);
        let nerd = nerd_font_family(cx);
        let dark = theme.mode == ThemeMode::Dark;
        render_file_header(&file, theme, nerd.as_ref(), dark, collapsed)
            .id(gpui::ElementId::Name(
                format!(
                    "review-file-{}-{file_index}",
                    if sticky { "sticky" } else { "row" }
                )
                .into(),
            ))
            .debug_selector(move || {
                if sticky {
                    "review-file-header-sticky"
                } else {
                    "review-file-header"
                }
                .to_string()
            })
            .cursor_pointer()
            .hover(|style| style.bg(theme.bg_hover))
            .on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_file(file_index, cx)),
            )
            .into_any_element()
    }

    fn render_diff_line(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(snapshot) = self.review.as_ref() else {
            return div().into_any_element();
        };
        let Some(line) = snapshot.lines.get(index) else {
            return div().into_any_element();
        };
        if snapshot.files.get(line.file_index).is_none() {
            return div().into_any_element();
        }
        let theme = *theme::get(cx);
        match &line.kind {
            LineKind::FileHeader => {
                self.pinned_row(self.file_header_row(line.file_index, false, theme, cx))
            }
            // A collapsed file keeps its header and nothing else.
            _ if self.collapsed_files.contains(&line.file_index) => {
                div().h(px(0.)).into_any_element()
            }
            LineKind::Gap(gap) => self.render_gap(index, gap.clone(), theme, cx),
            LineKind::HunkHeader => render_hunk_header(&line.content, theme, self.wrap),
            LineKind::Meta => render_meta(&line.content, theme, self.wrap),
            LineKind::Context | LineKind::Addition | LineKind::Deletion => {
                if !self.split {
                    return render_code_row(line, theme, self.wrap);
                }
                if self
                    .split_pairing
                    .secondary
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                {
                    // The paired deletion's row already draws this line; its
                    // own row collapses so the pair reads as one row.
                    return div().h(px(0.)).into_any_element();
                }
                let partner = self.split_pairing.partner.get(index).copied().flatten();
                match (&line.kind, partner) {
                    (LineKind::Addition, _) => render_split_row(None, Some(line), theme, self.wrap),
                    (LineKind::Deletion, Some(partner)) => {
                        render_split_row(Some(line), snapshot.lines.get(partner), theme, self.wrap)
                    }
                    (LineKind::Deletion, None) => {
                        render_split_row(Some(line), None, theme, self.wrap)
                    }
                    // Context rides both halves, numbered per side.
                    _ => render_split_row(Some(line), Some(line), theme, self.wrap),
                }
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
                        .child(icon(
                            gap_icon(direction),
                            IconSize::Indicator.px(&theme),
                            theme.text_3,
                        )),
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
            .text_size(TextSize::Small.px(&theme))
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
            .child(tr!("sidepane.unmodified_lines", count = gap.count()));

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
            // The line stream shifted under the pairing, and a cap-limited
            // reveal can leave another gap behind.
            self.rebuild_view_metrics();
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
        let filter_row = picker_search_frame(div(), &theme)
            .child(icon(
                "icons/search.svg",
                input::ICON.px(&theme),
                theme.text_3,
            ))
            .child(div().flex_1().min_w_0().child(self.tree_filter.clone()));

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
                            IconSize::Indicator.px(&theme),
                            theme.text_3,
                        ))
                        .child(icon(
                            "icons/folder.svg",
                            IconSize::Small.px(&theme),
                            theme.text_3,
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_size(TextSize::Small.px(&theme))
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
                let fallback = icon("icons/file.svg", IconSize::Small.px(&theme), theme.text_3)
                    .into_any_element();
                let glyph = file_glyph(
                    &file.path,
                    dark,
                    nerd.as_ref(),
                    IconSize::Small.px(&theme),
                    fallback,
                );
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
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(if selected { theme.text } else { theme.text_2 })
                                    .child(name),
                            )
                            .child(
                                div()
                                    .w(px(18.))
                                    .h(px(18.))
                                    .flex_none()
                                    .rounded(Radius::Small.px(&theme))
                                    .border_1()
                                    .border_color(status_color.opacity(0.65))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_size(TextSize::Small.px(&theme))
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
        let mut menu = context_menu_surface(div().id("review-source-menu"), &theme)
            .absolute()
            // Below the toolbar's source button.
            .top(menu_top(1. + PANE_ROW_H, ButtonSize::Medium, &theme))
            .left(px(10.))
            .w(px(200.))
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
    let row = context_menu_entry(
        div().id(gpui::ElementId::Name(
            format!("review-source-{label}").into(),
        )),
        &theme,
    )
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
        // Zed's `ml_4` before a trailing hint, less the row gap already there.
        row.child(
            div()
                .flex_none()
                .ml(context_menu::keybinding_gap(&theme) - context_menu::icon_gap(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_3)
                .child(tr!("sidepane.no_turns_yet")),
        )
    })
    .when(selected, |row| {
        row.child(icon(
            "icons/check.svg",
            context_menu::ICON.px(&theme),
            theme.accent,
        ))
    });
    row.into_any_element()
}

fn separator(theme: Theme) -> AnyElement {
    context_menu_separator(&theme).into_any_element()
}

/// The top of a dropdown hung from a button centered in the pane row that
/// starts `row_top` px below the pane: the button's bottom edge plus Zed's
/// menu offset.
fn menu_top(row_top: f32, trigger: ButtonSize, theme: &Theme) -> Pixels {
    px(row_top + PANE_ROW_H / 2.) + trigger.height(theme) * 0.5 + popover::MENU_OFFSET
}

// ── diff row rendering ─────────────────────────────────────────────────────

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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        let theme = *theme::get(cx);
        let body = if self.minimized {
            self.render_rail(theme, cx)
        } else {
            self.body(window, theme, cx)
        };
        div()
            .id("side-pane")
            .relative()
            .flex_none()
            .w(self.width)
            .h_full()
            .pl(px(12.))
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .id("side-pane-resize-handle")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    // Sit on the card's left border (the 12px gutter, minus
                    // half the handle width), not the panel's outer edge, so
                    // the edge users see is the edge they can grab.
                    .left(px(9.))
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|style| style.bg(theme.accent.opacity(0.4)))
                    .on_drag(SidePaneResize, |_, _, _, cx| cx.new(|_| DragGhost)),
            )
            // The dock reads as a rounded card, like the Git/Usage cards in
            // the main column. The menus stay outside this clipped shell so
            // they are not cut off at its corners.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .bg(theme.bg_main)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(Radius::Large.px(&theme))
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(body),
            )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;
    use gpui::point;

    fn pane(cx: &mut gpui::TestAppContext) -> Entity<SidePane> {
        cx.update(|cx| {
            cx.set_global(Theme::for_id(ThemeId::Orbit));
            cx.new(SidePane::new)
        })
    }

    /// A 10-line file with one change, so collapsing leaves a leading and a
    /// trailing gap.
    fn gap_patch() -> String {
        let mut patch = String::from(
            "diff --git a/src/lib.rs b/src/lib.rs\n\
             index 1111111..2222222 100644\n\
             --- a/src/lib.rs\n\
             +++ b/src/lib.rs\n\
             @@ -1,10 +1,10 @@\n",
        );
        for line in 1..=5 {
            patch.push_str(&format!(" context line {line}\n"));
        }
        patch.push_str("-change();\n+changed();\n");
        for line in 6..=10 {
            patch.push_str(&format!(" context line {line}\n"));
        }
        patch
    }

    #[gpui::test]
    fn full_width_minimize_and_dock_share_one_restore_point(cx: &mut gpui::TestAppContext) {
        let pane = pane(cx);
        cx.update(|cx| {
            pane.update(cx, |pane, cx| {
                pane.set_available_width(px(1200.), cx);
                pane.set_width(px(500.), cx);

                pane.toggle_full_width(cx);
                assert_eq!(pane.width(), px(1200.));
                assert!(pane.full_width);

                // From Full width, Minimize gives the window back to the
                // docked pane rather than hiding the panel in a rail...
                pane.toggle_minimize(cx);
                assert_eq!(pane.width(), px(500.));
                assert!(!pane.full_width);
                assert!(!pane.minimized);

                // ...and only a second Minimize, from the docked pane, does
                // that.
                pane.toggle_minimize(cx);
                assert_eq!(pane.width(), px(PANE_RAIL_W));
                assert!(pane.minimized);
                pane.toggle_minimize(cx);
                assert_eq!(pane.width(), px(500.));
                assert!(!pane.minimized);

                // `leave_full_width` — the app's feature-page and sidebar
                // path — docks without touching Minimize.
                pane.toggle_full_width(cx);
                assert_eq!(pane.width(), px(1200.));
                pane.leave_full_width(cx);
                assert_eq!(pane.width(), px(500.));
                assert!(!pane.full_width);

                // While full width, the pane tracks a resized window.
                pane.toggle_full_width(cx);
                pane.set_available_width(px(900.), cx);
                assert_eq!(pane.width(), px(900.));

                // Docked, a window that shrinks tightens the pane to fit.
                pane.leave_full_width(cx);
                assert_eq!(pane.width(), px(500.));
                pane.set_available_width(px(420.), cx);
                assert_eq!(pane.width(), px(420.));
            });
        });
    }

    #[gpui::test]
    fn collapse_all_hides_every_file_body_and_expand_all_restores_them(
        cx: &mut gpui::TestAppContext,
    ) {
        let pane = pane(cx);
        let snapshot = crate::review::parse_collected(
            Source::Uncommitted,
            "1\t1\tsrc/lib.rs\n",
            &gap_patch(),
            true,
        );
        let files = snapshot.files.len();
        cx.update(|cx| {
            pane.update(cx, |pane, cx| {
                pane.apply_snapshot(snapshot);
                assert!(pane.collapsed_files.is_empty());

                pane.toggle_all_files(cx);
                assert_eq!(pane.collapsed_files.len(), files);

                pane.toggle_all_files(cx);
                assert!(pane.collapsed_files.is_empty());
            });
        });
    }

    #[gpui::test]
    fn one_file_collapses_independently_and_selecting_it_expands_it(cx: &mut gpui::TestAppContext) {
        let pane = pane(cx);
        let snapshot = crate::review::parse_collected(
            Source::Uncommitted,
            "1\t1\tsrc/lib.rs\n",
            &gap_patch(),
            true,
        );
        cx.update(|cx| {
            pane.update(cx, |pane, cx| {
                pane.apply_snapshot(snapshot);

                pane.toggle_file(0, cx);
                assert!(pane.collapsed_files.contains(&0));

                // Picking the file in the tree is an ask to see its diff.
                pane.select_file(0, cx);
                assert!(pane.collapsed_files.is_empty());
            });
        });
    }

    #[gpui::test]
    fn wrap_and_split_toggles_flip_their_state(cx: &mut gpui::TestAppContext) {
        let pane = pane(cx);
        cx.update(|cx| {
            pane.update(cx, |pane, cx| {
                assert!(pane.wrap);
                pane.toggle_wrap(cx);
                assert!(!pane.wrap);

                assert!(!pane.split);
                pane.toggle_split(cx);
                assert!(pane.split);
            });
        });
    }

    /// Every view mode lays out its real element tree without panicking —
    /// split rows, collapsed files, and the unwrapped scroller only exist at
    /// render time.
    #[gpui::test]
    fn every_view_mode_draws(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let pane = cx.update(|_, cx| cx.new(SidePane::new));
        cx.update(|_, cx| {
            pane.update(cx, |pane, cx| {
                pane.open = true;
                pane.set_available_width(px(900.), cx);
                pane.apply_snapshot(crate::review::parse_collected(
                    Source::Uncommitted,
                    "1\t1\tsrc/lib.rs\n",
                    &gap_patch(),
                    true,
                ));
            });
        });
        let viewport = || gpui::size(px(900.), px(800.));
        for (wrap, split, collapse) in [
            (true, false, false),
            (false, false, false),
            (true, true, false),
            (false, true, false),
            (true, true, true),
            (false, true, true),
        ] {
            cx.update(|_, cx| {
                pane.update(cx, |pane, cx| {
                    pane.wrap = wrap;
                    pane.split = split;
                    let should_collapse = collapse && pane.collapsed_files.is_empty();
                    if should_collapse || (!collapse && !pane.collapsed_files.is_empty()) {
                        pane.toggle_all_files(cx);
                    }
                    cx.notify();
                });
            });
            let _ = cx.draw(point(px(0.), px(0.)), viewport(), |_, _| pane.clone());
        }
        // The minimized rail draws too.
        cx.update(|_, cx| {
            pane.update(cx, |pane, cx| {
                pane.toggle_minimize(cx);
                assert!(pane.minimized);
            });
        });
        let _ = cx.draw(point(px(0.), px(0.)), viewport(), |_, _| pane.clone());
    }

    /// Unwrapped text keeps the file header fixed: its name and change counts
    /// stay at the viewport's edges while only the code pans sideways beneath.
    #[gpui::test]
    fn unwrapped_file_header_stays_fixed_while_code_pans(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let pane = cx.update(|_, cx| cx.new(SidePane::new));
        let long = "x".repeat(200);
        let patch = format!(
            "diff --git a/src/lib.rs b/src/lib.rs\n\
             index 1111111..2222222 100644\n\
             --- a/src/lib.rs\n\
             +++ b/src/lib.rs\n\
             @@ -1,3 +1,3 @@\n\
              context one\n\
             -old line\n\
             +{long}\n\
              context two\n"
        );
        cx.update(|_, cx| {
            pane.update(cx, |pane, cx| {
                pane.open = true;
                pane.set_width(px(460.), cx);
                pane.wrap = false;
                pane.apply_snapshot(crate::review::parse_collected(
                    Source::Uncommitted,
                    "1\t1\tsrc/lib.rs\n",
                    &patch,
                    false,
                ));
            });
        });
        let viewport = || gpui::size(px(900.), px(700.));
        let _ = cx.draw(point(px(0.), px(0.)), viewport(), |_, _| pane.clone());

        let scroller = cx.update(|_, cx| pane.read(cx).h_scroll.bounds());
        assert!(
            scroller.size.width < px(700.),
            "the scroller is the pane-sized viewport, got {:?}",
            scroller.size.width
        );
        let header = cx
            .debug_bounds("review-file-header")
            .expect("the in-list header is laid out");
        assert!(
            (header.size.width - scroller.size.width).abs() < px(1.),
            "the header spans the visible pane, not the wide content: {:?} vs {:?}",
            header.size.width,
            scroller.size.width
        );

        // Pan the content right: the offset moves, the header must not.
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: point(px(200.), px(400.)),
            delta: gpui::ScrollDelta::Pixels(point(px(-240.), px(0.))),
            ..Default::default()
        });
        let _ = cx.draw(point(px(0.), px(0.)), viewport(), |_, _| pane.clone());
        let offset = cx.update(|_, cx| pane.read(cx).h_scroll.offset().x);
        assert!(
            offset < px(0.),
            "the scroller must pan right, got {offset:?}"
        );
        let after = cx
            .debug_bounds("review-file-header")
            .expect("the in-list header is still laid out");
        assert!(
            (after.origin.x - header.origin.x).abs() < px(1.),
            "the header is fixed: {:?} -> {:?}",
            header.origin.x,
            after.origin.x
        );
    }
}
