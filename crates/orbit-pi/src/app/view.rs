use super::format::*;
use super::session::{SESSION_DETAILS_GUTTER, SESSION_DETAILS_W};
use super::sidebar::*;
use super::*;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::prelude::*;

use crate::commands::{self, CommandId};
use crate::motion::Hover;
use crate::ui::button::{IconButton, IconButtonVariant};
use crate::ui::chip::{ChipButton, ChipStyle};
use crate::ui::menu::menu_hint;
use crate::ui::popover::{popover_list, PopoverExt};
use crate::ui::resize::{resize_handle, ResizeAxis, ResizeLook};
use crate::ui::row::option_row;
use crate::ui::status_bar::{status_button, status_item, status_message, StatusBar};
use gpui::StyledText;

use crate::theme::tokens::{
    button, context_menu, input, looping, panel, picker, ButtonSize, DynamicSpacing, IconSize,
    Radius, TextSize, BORDER_WIDTH,
};
use crate::ui::button::navigation_icon_button;
use crate::ui::style::surface::SurfaceExt;
use crate::ui::tooltip::Tooltip;
use crate::widgets as ext_widgets;
use crate::workspace_mark::{self, WorkspaceMark};

/// Height of a page's top bar (DESIGN.md: 44px header rows). The new-task
/// backdrop is offset by it, so the picture starts below the title exactly
/// where it always has — this is the one value the two must agree on.
pub(super) const TOP_BAR_H: f32 = 44.;

/// Leading inset that clears the OS window buttons in the titlebar row: the
/// macOS traffic lights when the titlebar is transparent, or a plain edge
/// inset where the system titlebar holds them (see
/// [`platform::titlebar_options`]). The sidebar's drag strip and the main top
/// bar share it so the window's left controls never ride into the OS buttons.
pub(super) const TRAFFIC_LIGHT_CLEARANCE: f32 = platform::WINDOW_CONTROLS_CLEARANCE;

/// Space between the titlebar's left controls. Set to the same 8px the right
/// cluster spaces its chips with (`gap_2`), so the two ends of the bar share
/// one rhythm — a bare 2px let bordered chips read as one crowded block.
const TITLEBAR_CONTROLS_GAP: f32 = 8.;

/// Breathing room between the OS window buttons and the first titlebar
/// control. The traffic-light clearance alone ended flush against them, which
/// made the app's own controls read as part of the caption; the lead gives
/// the overlay its own left margin.
pub(super) const TITLEBAR_CONTROLS_LEAD: f32 = 12.;

/// Combined width of the titlebar's left controls (toggle + history) as laid
/// out by [`OrbitApp::titlebar_left_controls`]: three [`HEADER_CTRL_H`] boxes,
/// two [`TITLEBAR_CONTROLS_GAP`] gaps, and the trailing 6px gap.
pub(super) const TITLEBAR_CONTROLS_W: f32 = HEADER_CTRL_H * 3. + TITLEBAR_CONTROLS_GAP * 2. + 6.;

/// Space between the controls and the session title while the sidebar is
/// collapsed. The cluster's own trailing 6px pad is inside
/// [`TITLEBAR_CONTROLS_W`], so this is what actually separates the last chip
/// from the title — without it the two sit flush once the sidebar stops
/// providing the separation.
pub(super) const TITLEBAR_TITLE_GAP: f32 = 8.;

/// Where the main top bar's content starts when the sidebar is collapsed: past
/// the traffic lights, the controls' lead margin, the (overlaid) window
/// controls, and the gap that keeps the title off them.
pub(super) const TITLEBAR_LEADING: f32 =
    TRAFFIC_LIGHT_CLEARANCE + TITLEBAR_CONTROLS_LEAD + TITLEBAR_CONTROLS_W + TITLEBAR_TITLE_GAP;

/// Space kept between the last titlebar control and the sidebar's right edge
/// while the controls are right-aligned inside the sidebar's strip. The
/// sidebar's 6px resize handle lives against that edge, so the pad keeps the
/// chips off it (and off the sidebar's rounded boundary).
pub(super) const SIDEBAR_EDGE_PAD: f32 = 6.;
/// Width of the wordmark in the sidebar's titlebar drag strip.
pub(super) const SIDEBAR_WORDMARK_W: f32 = 60.;

/// Where the titlebar's left controls sit. While the sidebar is open they
/// right-align inside its strip — [`SIDEBAR_EDGE_PAD`] off the column's right
/// edge, so they ride both a resize drag and the open/close slide. Collapsed,
/// there is no column to hug and the chips fall back to the fixed lead past
/// the OS window buttons.
///
/// Windows is the exception: it has no OS buttons on the left at all — the app
/// paints the caption itself, at the window's right
/// ([`platform::draws_window_controls`]) — so the cluster left-aligns on the
/// fixed lead whether the sidebar is open or not, the way a Windows app's own
/// titlebar controls sit. Nothing else changes there: the lead still clears
/// the window edge, and the wordmark sits after the controls in the drag strip.
/// The debug recorder's live chip: `REC · 128` while a capture runs, nothing
/// when it does not. Debug builds only.
#[cfg(debug_assertions)]
fn perf_recording_chip(theme: &Theme) -> Option<AnyElement> {
    crate::perf::is_active().then(|| {
        let frames = crate::perf::sample_count();
        div()
            .flex_none()
            .h(px(20.))
            .px(px(6.))
            .rounded(button::RADIUS.px(theme))
            .border_1()
            .border_color(theme.accent_text.opacity(0.4))
            .bg(theme.accent_text.opacity(0.12))
            .flex()
            .items_center()
            .gap(px(4.))
            .text_size(TextSize::XSmall.px(theme))
            .font_family(theme::code_font_family())
            .text_color(theme.accent_text)
            .child(format!("REC · {frames}"))
            .into_any_element()
    })
}

pub(super) fn titlebar_controls_left(sidebar_visible: bool, sidebar_width: f32) -> f32 {
    if sidebar_visible && !cfg!(windows) {
        sidebar_width - TITLEBAR_CONTROLS_W - SIDEBAR_EDGE_PAD
    } else {
        TRAFFIC_LIGHT_CLEARANCE + TITLEBAR_CONTROLS_LEAD
    }
}

/// Wrap a feature page (Files / Git / Usage) in the card the main column shows
/// below the shared top bar: a gap from the sidebar on the left, rounded
/// corners, a hairline, and a clipped body. Keeps the top bar and the session
/// context in place instead of swapping the whole column for an edge-to-edge
/// page.
fn feature_card(theme: Theme, body: AnyElement) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        // A flex item's minimum width defaults to its content's: without this
        // a page with a wide intrinsic row (the Files tab strip) refuses to
        // shrink and pushes the right dock past the window edge.
        .min_w_0()
        .w_full()
        .pl(DynamicSpacing::Base12.px(&theme))
        .child(
            // Over a full-window wallpaper the card is glass and the page
            // inside paints `page_canvas` (clear), so the frost shows; only
            // the surface is translucent, never its text or controls.
            div()
                .size_full()
                .flex()
                .flex_col()
                .overflow_hidden()
                .page_card(&theme, body),
        )
        .into_any_element()
}

/// The Access and Workflow mode picker popups share one width.
const MODE_POPUP_W: f32 = 300.;

/// Page content kept clear above the terminal panel — the transcript *and*
/// the composer, which sits between them. Dragging the panel's top edge to the
/// top of the window must not collapse the conversation behind the input.
const TERMINAL_MAIN_RESERVE: f32 = 320.;

/// The narrowest window that still fits the sidebar and a usable workspace.
/// Below it the sidebar yields: a cramped column beside a cramped transcript is
/// worse than a transcript with ⌘⇧B to bring the list back.
const SIDEBAR_MIN_WINDOW_W: f32 = 880.;

/// Whether the sessions sidebar is on screen. The user's toggle is the first
/// word, but a full-window surface (Settings) and a window too narrow for both
/// panes each override it — the panel stays mounted and simply does not paint.
pub(super) fn sidebar_shows(visible: bool, settings_open: bool, window_width: Pixels) -> bool {
    visible && !settings_open && window_width >= px(SIDEBAR_MIN_WINDOW_W)
}

impl Render for OrbitApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Cleared here and set again only where the new-task backdrop is drawn,
        // so a frame that covers it (Settings, a feature page) stops the
        // heartbeat repainting for a field nobody can see.
        self.backdrop_live = false;
        // Land in the arrangement the user left: sizes (read when each panel
        // was built), open panels, sidebar visibility, collapsed groups. It
        // needs a window — opening the terminal spawns a shell — so the restore
        // waits for the first frame instead of running in `new`.
        // Debug builds only: a capture started from `tick` has no window, so the
        // frame hook is armed here, on the first frame after it starts.
        #[cfg(debug_assertions)]
        crate::perf::ensure_armed(window);
        if !self.layout_restored {
            self.layout_restored = true;
            self.apply_stored_arrangement(window, cx);
        }
        if self.session_rename_ready {
            self.session_rename_ready = false;
            self.begin_session_rename(window, cx);
        }
        // A clicked banner asks for the window; bring it forward on the frame
        // that follows the click (macOS activates the app, but a minimized
        // window would otherwise stay behind).
        if self.activate_window_pending {
            self.activate_window_pending = false;
            window.activate_window();
        }
        // Model / thinking choices are fixed for the duration of a turn. A
        // picker left open when a run begins is dropped so its list cannot
        // still change the model mid-turn; the chips render disabled.
        if self.is_running() {
            self.close_model_selector(window, cx);
        }
        let theme = *theme::get(cx);
        // Warm or refresh the wallpaper cache before the tree is built. The
        // call is a no-op while a load is in flight and when the cache key is
        // current; image work itself runs on the background executor.
        self.ensure_wallpaper(cx);
        // Diagram layout sizes node boxes from shaped text, and the markdown
        // renderer it runs inside has no `window`; hand it the text system for
        // the duration of this frame.
        crate::mermaid::install(window);
        // Sessions the sidebar lists, plus a placeholder for the open one
        // when its file is not on disk yet. Apply both filters before building
        // rows so their indices match the sessions rendered and navigated;
        // unfiltered sessions still drive the workspace status glyphs.
        let all_sessions = self.sidebar_sessions();
        let archived_set: HashSet<PathBuf> = crate::archive::all().paths();
        let sidebar_sessions = filter_sidebar_sessions(
            all_sessions.clone(),
            self.sidebar_archived_filter,
            &archived_set,
            &self.sidebar_query,
        );
        // A pinned session leads its project group; the sidebar sorts and
        // marks pinned rows from this snapshot (Orbit-owned state).
        let pinned: Rc<HashSet<PathBuf>> = Rc::new(crate::pins::all().paths());
        // Archived sessions, from the Orbit-owned set — rows in "show
        // archived" mode wear a small archive glyph.
        let archived_paths: Rc<HashSet<PathBuf>> = Rc::new(archived_set);
        // Flat lists, including search results, name each row's workspace.
        let show_workspace =
            self.sidebar_group_by == SidebarGroupBy::OneList || !self.sidebar_query.is_empty();
        let show_first_message = self.sidebar_show_first_message;
        let show_updated = self.sidebar_show_updated;
        let show_shortcuts = self.sidebar_shortcut_hints;
        // Parked (background) sessions mid-run — they keep their row under a
        // collapsed workspace header, like the open session, so a live task
        // is never hidden by a collapse. Also drives the running loader.
        let running_paths: Rc<HashSet<PathBuf>> = Rc::new(
            self.lives
                .iter()
                .filter(|(_, parked)| parked.busy)
                .map(|(path, _)| path.clone())
                .collect(),
        );
        // Per-workspace glyph: the user's mark icon when set, otherwise a
        // folder status (current / running / archived-only / idle).
        let running_workspaces: HashSet<PathBuf> = filter_archived(
            all_sessions.clone(),
            self.sidebar_archived_filter,
            &archived_paths,
        )
        .iter()
        .filter(|session| running_paths.contains(&session.path))
        .map(|session| session.cwd.clone())
        .collect();
        let archived_only_workspaces: HashSet<PathBuf> = {
            let mut totals: HashMap<PathBuf, (usize, usize)> = HashMap::new();
            for session in &all_sessions {
                let entry = totals.entry(session.cwd.clone()).or_insert((0, 0));
                entry.0 += 1;
                if archived_paths.contains(&session.path) {
                    entry.1 += 1;
                }
            }
            totals
                .into_iter()
                .filter(|(_, (total, archived))| *total > 0 && total == archived)
                .map(|(workspace, _)| workspace)
                .collect()
        };
        let workspace_icons: Rc<HashMap<PathBuf, gpui::SharedString>> = Rc::new(
            self.workspaces
                .iter()
                .map(|workspace| {
                    let mark = self
                        .workspace_marks
                        .get(workspace)
                        .cloned()
                        .unwrap_or_default();
                    let status = if Some(workspace.as_path()) == self.current_workspace.as_deref() {
                        workspace_mark::WorkspaceStatus::Current
                    } else if running_workspaces.contains(workspace) {
                        workspace_mark::WorkspaceStatus::Running
                    } else if archived_only_workspaces.contains(workspace) {
                        workspace_mark::WorkspaceStatus::Archived
                    } else {
                        workspace_mark::WorkspaceStatus::Idle
                    };
                    (
                        workspace.clone(),
                        workspace_mark::resolved_icon_path(&mark, status),
                    )
                })
                .collect(),
        );
        let side_rows = Rc::new(self.sidebar_rows(&sidebar_sessions, &pinned));
        // The section label's count: the local sessions the list shows (a
        // collapsed group still counts its sessions in its own header).
        let listed_sessions = sidebar_sessions.len();
        let old = self.sidebar_list.item_count();
        if old != side_rows.len() {
            self.sidebar_list.splice(0..old, side_rows.len());
        }
        // A row drag (issue #143): the state is cleared the moment the mouse
        // is up, so an insertion line can never outlive the drag that painted
        // it — including a release outside every drop target, which dispatches
        // no drop at all.
        if !cx.has_active_drag() {
            if self.project_drag.is_some() {
                self.project_drag = None;
            }
            if self.session_drag.is_some() {
                self.session_drag = None;
            }
        }
        let project_drag: Rc<Option<ProjectDragState>> = Rc::new(self.project_drag.clone());
        let session_drag: Rc<Option<SessionDragState>> = Rc::new(self.session_drag.clone());
        // Session rows are rearranged in the grouped list only: a flat list
        // and a search result set order by recency, and a row there has no
        // project block to sit in.
        let sessions_reorderable = self.sessions_reorderable();
        let sessions_data = Rc::new(sidebar_sessions);
        let active_path = Rc::new(self.current_session_path.clone());
        let session_menu = Rc::new(self.session_menu.clone());
        let workspace_menu = Rc::new(self.workspace_menu.clone());
        let sidebar_cursor = self.sidebar_cursor;
        let keyboard_card = !self.sidebar_sort_menu && !self.sidebar_show_menu;
        let this = cx.entity();
        // The open session's agent activity drives the sidebar's running
        // loader (parked runs come in via `running_paths` above).
        let agent_running = self.busy || self.transcript.is_streaming();
        // Every live process (running or warm-idle) — guards delete.
        let live_paths: Rc<HashSet<PathBuf>> = Rc::new(self.lives.keys().cloned().collect());
        // Per-workspace sidebar marks (icon + tint), read by every header row.
        let marks: Rc<HashMap<PathBuf, WorkspaceMark>> = Rc::new(self.workspace_marks.clone());
        // Branch per project, for the rows' metadata line. Fetched off-thread
        // (`refresh_sidebar_branches`); render only reads it.
        let branches: Rc<HashMap<PathBuf, crate::app::BranchLine>> =
            Rc::new(self.sidebar_branches.clone());
        let logos = Rc::new(self.sidebar_logos.clone());
        // Rows whose last turn settled while the user was elsewhere. The row
        // paints the mark; it never replaces a running / pin / archive glyph
        // (issue #83).
        let review_marks: Rc<HashMap<PathBuf, crate::app::ReviewMark>> =
            Rc::new(self.awaiting_review.clone());
        let session_statuses = Rc::new(self.sidebar_session_statuses());
        // Pinned header for the expanded workspace group currently at the top
        // of the session list: it stays there while that group's own sessions
        // scroll, and the next group's header pushes it away (see
        // `sticky_sidebar_header`). Its row is rendered twice while pinned (the
        // real one scrolls under the overlay), so the pinned index also tells
        // the list to skip that row's workspace menu — the overlay owns the
        // single open popup.
        let sticky = sticky_sidebar_header(&self.sidebar_list, &side_rows);
        let sticky_ix = sticky.as_ref().map(|sticky| sticky.ix);
        let sticky_header = sticky.map(|sticky| {
            div()
                .absolute()
                .top(sticky.top_offset)
                .left_0()
                .w_full()
                // The rail under it frosts, so the band does too.
                .pane_sticky_frost(&theme, theme.surface)
                .child(render_side_row(
                    &side_rows,
                    &sessions_data,
                    active_path.as_deref(),
                    sticky.ix,
                    &this,
                    agent_running,
                    &running_paths,
                    &pinned,
                    &archived_paths,
                    show_workspace,
                    show_first_message,
                    show_updated,
                    show_shortcuts,
                    &live_paths,
                    &marks,
                    &workspace_icons,
                    &logos,
                    &branches,
                    &review_marks,
                    &session_statuses,
                    session_menu.as_ref().as_ref(),
                    workspace_menu.as_ref().as_ref(),
                    // The overlay is a second paint of a header that already
                    // has a drop target: it never starts or accepts one.
                    &SidebarDragPaint::idle(),
                    sidebar_cursor == Some(sticky.ix),
                    self.session_card.as_deref(),
                    keyboard_card,
                    theme,
                ))
                .into_any_element()
        });

        let workspace_label = self.workspace_label();
        let review_workspace = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        // The rail gates on the main area's width (872px transcript
        // container), which excludes the sessions sidebar when visible.
        let viewport = window.viewport_size();
        // The right side pane is hidden while settings/onboarding own the
        // main area (same rule as the sessions sidebar).
        let pane_open = self.sidepane.read(cx).is_open();
        // A remote session has no local worktree to review or browse.
        let pane_visible = pane_open
            && !self.settings_open
            && !self.usage_open
            && self.dependencies_ready()
            && self.remote.is_none();
        // Feature pages (Files / Git / Usage / Worktrees) own the main column, so a
        // full-page Review pane yields to the one that just opened rather than
        // covering it. `OrbitApp::chat_column_covered` (events.rs) answers the
        // same question for the review-mark bookkeeping — keep them in step.
        let viewer_open = self.file_viewer.read(cx).is_open();
        let feature_open = viewer_open || self.git_open || self.usage_open || self.worktrees_open;
        // A page that opens under a full-page Review pane gets the window
        // back; one that was already open does not block maximizing over it.
        if pane_visible
            && feature_open
            && !self.feature_open_last
            && self.sidepane.read(cx).is_full_width()
        {
            self.sidepane
                .update(cx, |pane, cx| pane.leave_full_width(cx));
        }
        self.feature_open_last = feature_open;
        // Expanding the Review pane hides the session view, but it opens the
        // sessions sidebar: switching sessions stays one click away while the
        // review owns the page. The sidebar snaps open with the pane rather
        // than sliding, so the session view never peeks through mid-slide.
        let pane_full = pane_visible && self.sidepane.read(cx).is_full_width();
        if pane_full && !self.pane_full_last && !self.sidebar_visible {
            self.sidebar_visible = true;
        }
        self.pane_full_last = pane_full;
        let sidebar_shown = sidebar_shows(self.sidebar_visible, self.settings_open, viewport.width);

        // Bottom terminal panel. The chat branch is what renders it, so the
        // full-page surfaces (settings / Git / Usage) hide it by construction;
        // `terminal_visible` exists to stop a hidden shell requesting frames.
        // A full-page Review pane collapses the chat column, terminal
        // included, so it counts as hidden too.
        let terminal_visible = self.terminal_panel.read(cx).is_open()
            && !self.settings_open
            && !self.usage_open
            && !self.git_open
            && !self.worktrees_open
            && !pane_full;
        // An expanded terminal is a page of its own, like Git or Usage: the
        // chat is skipped and the panel fills the card under the top bar. The
        // docks keep their place — it is the column, not the window.
        // `terminal_visible` already covers the surfaces that hide the panel.
        let terminal_full = terminal_visible && self.terminal_panel.read(cx).is_full_width();
        // The embedded Review browser on the Git page's Changes tab lays out
        // against the page width, so its stats and tree follow the column.
        let git_review_width = viewport.width
            - if sidebar_shown {
                self.sidebar_width
            } else {
                px(0.)
            };
        // Leading inset the full-window pages (Git/Usage/Files) give their
        // headers while the sidebar is collapsed. They now sit in a card below
        // the top bar, so they only need the normal page padding — which is
        // already their default; nothing to sync per frame.
        self.git_panel
            .update(cx, |panel, cx| panel.set_review_width(git_review_width, cx));
        // The folder picker was sized when it opened; a resize or a dock
        // toggled while it is open must not leave it wider than the column it
        // hangs under, or narrower than it needs to be.
        if let Some(picker) = self.workspace_picker.clone() {
            let width =
                crate::workspace_picker::width_for_window(self.main_area_width(window, cx).into());
            picker.update(cx, |picker, cx| picker.set_width(width, cx));
        }
        // ── project panel (right dock) ── hidden while Settings owns the
        // window, like the sessions sidebar. Sync the workspace each render;
        // a change rebuilds the tree off-thread. The Explorer and the Review
        // pane are mutually exclusive right docks: while Review is open the
        // tree stays closed.
        let explorer_visible = self.project_panel.read(cx).is_open()
            && !self.settings_open
            && !pane_open
            && self.remote.is_none();
        let explorer_width = if explorer_visible {
            self.project_panel.read(cx).width()
        } else {
            px(0.)
        };
        // ── session details (right dock) ── shares the slot with Review and
        // the Explorer; opening any one of them closes the others.
        let details_visible = self.session_details_visible(cx);
        let details_width = self.session_details_width(cx);
        // The pane's ceiling is the window minus the sidebar and the Explorer;
        // Full width expands to it and collapses the chat column. The Explorer
        // is closed whenever the pane opens, so this is usually the sidebar's
        // width alone.
        let pane_ceiling = viewport.width
            - if sidebar_shown {
                self.sidebar_width
            } else {
                px(0.)
            }
            - explorer_width
            - details_width;
        self.sidepane
            .update(cx, |pane, cx| pane.set_available_width(pane_ceiling, cx));
        // Read the pane's width after the ceiling update: a full-width pane
        // has just grown and this frame's layout must see it.
        let pane_width = if pane_visible {
            self.sidepane.read(cx).width()
        } else {
            px(0.)
        };
        let main_width = self.main_area_width(window, cx);
        // The new-task page: no transcript yet. That one fact decides where the
        // composer sits, how wide it is, and whether the folder row above it
        // asks where the task runs (see `new_task`).
        let empty = self.transcript.is_empty();
        // Scale the wrapping threshold with the controls' UI size, so large
        // text cannot crowd Send or hide the mode controls.
        let page_column = f32::from(new_task::column_width(main_width));
        let session_column = f32::from(main_width - px(32.)).min(CONTENT_MAX_W);
        let composer_compact = (if empty
            || self.new_task_settle_pending
            || new_task::settling(self.new_task_settle_at, &theme)
        {
            page_column
        } else {
            session_column
        }) < f32::from(theme.ui_px(600.));
        // The Usage page lays itself out against the real main-area width, so
        // its tables and grids never overflow the column it is given.
        if self.usage_open {
            let width = f32::from(main_width);
            self.usage
                .update(cx, |page, cx| page.set_main_width(width, cx));
        }

        // ── top-bar right controls ──
        // When Review owns the right edge, or the chat column is tight, the
        // title and the chips collide. Compact the quota label (the window
        // name drops, the percentage stays).
        // The top bar spans the whole main area, so the docks do not squeeze
        // it — compact only when the main area itself is tight.
        let compact_chrome =
            f32::from(main_width + explorer_width + pane_width + details_width) < 720.;
        // While a feature card is open, its title moves up into the shared top
        // bar (see `GitPanel::top_bar_leading` / `UsagePage::top_bar_leading`);
        // the card's own chrome (branch selector, filters) stays with the view
        // it scopes.
        let git_leading = self.git_open.then(|| {
            self.git_panel
                .update(cx, |panel, cx| panel.top_bar_leading(theme, cx))
        });
        let usage_leading = self.usage_open.then(|| {
            self.usage
                .update(cx, |page, cx| page.top_bar_leading(theme, cx))
        });
        let worktrees_leading = self
            .worktrees_open
            .then(|| self.worktrees_top_bar_leading(theme, cx));
        // The expanded shell is a page too, so its Back and title take the same
        // place in the bar (see `TerminalPanel::top_bar_leading`) — but only
        // while it actually owns the column: a Files page opened over it keeps
        // the bar's own leading.
        let terminal_leading = (terminal_full && !feature_open).then(|| {
            self.terminal_panel
                .update(cx, |panel, cx| panel.top_bar_leading(theme, cx))
        });
        let mut top_controls = div().flex_none().flex().items_center().gap_2();
        // Provider quota — a compact, provider-independent headroom meter.
        // Hidden entirely on a pi without `quota.*` (or when no provider
        // reports anything), so the bar never shows a fabricated value.
        top_controls = top_controls.children(self.render_quota_chip(compact_chrome, cx));
        // Session details — its own chip right after the usage meter. It
        // toggles the details panel in the right dock (see
        // `render_session_details_panel`).
        // A new task has no session to describe yet, so the chip waits for the
        // first message, like the panel it opens.
        top_controls = top_controls.children(
            self.session_has_details()
                .then(|| self.render_session_details_control(theme, cx)),
        );
        // Debug builds only: while a frame-time capture runs, the bar carries a
        // live count. It is the only feedback that proves frames are arriving —
        // a capture that samples nothing writes no report.
        #[cfg(debug_assertions)]
        {
            top_controls = top_controls.children(perf_recording_chip(&theme));
        }
        top_controls = top_controls.children(self.render_open_in_control(cx));
        top_controls = top_controls
            // Review dock — the side-panel toggle and the working tree's
            // change counts folded into one chip
            // (see `render_review_panel_control`).
            .child(self.render_review_panel_control(theme, pane_visible, cx))
            // Terminal toggle — the bottom panel is a live surface a keystroke
            // away (⌘J), so it keeps its own chip beside Review rather than
            // hiding in a menu. See `render_terminal_control`.
            .child(self.render_terminal_control(theme, terminal_visible, cx))
            // Explorer — the project panel's own toggle, after the terminal's.
            .child(self.render_explorer_control(theme, explorer_visible, cx));

        // A blocking extension dialog owns the keyboard while it is open. Focus
        // it (or its text field) once, on the first frame it appears — `tick`
        // has no window to focus with.
        let dialog_layer = self.dialog.clone();
        if let Some(dialog) = &dialog_layer {
            if self.dialog_focus_pending {
                self.dialog_focus_pending = false;
                let focus = dialog.read(cx).focus_handle(cx);
                self.dialog_return_focus.capture(&focus, window, cx);
                window.focus(&focus);
            }
        }
        // A custom-UI surface owns the keyboard while it is open; the newest
        // (top of the stack) takes focus, and closing the last hands focus back
        // to the composer so typing continues.
        if self.custom_ui_focus_pending {
            self.custom_ui_focus_pending = false;
            match self.custom_ui.last() {
                Some(ui) => window.focus(&ui.read(cx).focus_handle(cx)),
                None => self.input.read(cx).focus(window),
            }
        }
        // The inline approval bar owns the keyboard the same way.
        if self.approval_focus_pending {
            self.approval_focus_pending = false;
            self.approval_return_focus
                .capture(&self.approval_focus, window, cx);
            window.focus(&self.approval_focus);
        }
        // The rename/move field takes the keyboard when its dialog opens, and
        // hands it back on the first frame after the dialog is gone. Closing
        // runs through many paths (buttons, scrim, Escape, a finished move),
        // and this is the one place that sees all of them.
        if self.worktree_field_focus_pending {
            self.worktree_field_focus_pending = false;
            if matches!(
                self.worktree_dialog,
                Some(WorktreeDialog::Rename { .. } | WorktreeDialog::Move { .. })
            ) {
                let field = self.worktree_field_input.read(cx).focus_handle(cx);
                self.worktree_return_focus.capture(&field, window, cx);
                window.focus(&field);
            }
        } else if self.worktree_dialog.is_none() {
            if let Some(opener) = self.worktree_return_focus.take() {
                self.return_focus(Some(opener), window, cx);
            }
        }
        // The update modal owns the keyboard (Escape dismisses) once open.
        if self.updater_dialog_focus_pending {
            self.updater_dialog_focus_pending = false;
            self.updater_return_focus
                .capture(&self.updater_dialog_focus, window, cx);
            window.focus(&self.updater_dialog_focus);
        }
        // The inline ask panel owns the keyboard while it is open.
        if self.ask_focus_pending {
            self.ask_focus_pending = false;
            let focus = self.ask_focus.clone();
            window.focus(&focus);
        }
        // The SSH password dialog takes the keyboard when it opens, and a
        // remote task started from a background check hands it to the
        // composer once the session is up.
        if self.remote_password_focus_pending {
            self.remote_password_focus_pending = false;
            if let Some(prompt) = &self.remote_password_prompt {
                let focus = prompt.input.read(cx).focus_handle(cx);
                self.remote_password_focus.open(&focus, window, cx);
            }
        }
        // Each modal's opener gets the keyboard back on the first frame after
        // the modal is gone, however it closed. Runs before the composer
        // hand-off below so a remote task that just started still wins.
        // Settings paints four of them: closing Settings closes them too.
        let in_settings = self.settings_open;
        let closed = [
            self.mcp_editor_focus
                .closed(in_settings && self.mcp_editor.is_some()),
            self.remote_host_editor_focus
                .closed(in_settings && self.remote_host_editor.is_some()),
            self.account_editor_focus
                .closed(in_settings && self.account_editor.is_some()),
            self.remote_key_editor_focus
                .closed(in_settings && self.remote_key_editor.is_some()),
            self.server_clone_focus.closed(self.server_clone.is_some()),
            self.remote_password_focus
                .closed(self.remote_password_prompt.is_some()),
        ];
        if let Some(opener) = closed.into_iter().flatten().last() {
            self.return_focus(opener, window, cx);
        }
        if self.composer_focus_pending && self.remote_password_prompt.is_none() {
            self.composer_focus_pending = false;
            self.input.read(cx).focus(window);
        }
        // Settings → MCP hands the keyboard to the server list so ↑/↓ walk
        // the rows without a mouse.
        if self.mcp_focus_pending {
            self.mcp_focus_pending = false;
            if self.settings_open && self.settings_section == SettingsSection::Mcp {
                window.focus(&self.mcp_list_focus);
            }
        }

        div()
            .size_full()
            .flex()
            .relative()
            .bg(if sidebar_shown {
                theme.surface
            } else {
                theme.background
            })
            .text_color(theme.text_primary)
            .font_family(theme::ui_font_family())
            // One wallpaper layer behind every surface. Hero-only places it in
            // the main column instead (`new_task_backdrop`); full-window owns
            // the whole root, so the sidebar's translucent fill reveals it.
            .children(if theme.wallpaper.full_window {
                crate::wallpaper::layer(theme, theme::reduce_motion(cx))
            } else {
                None
            })
            // Dropping files anywhere in the window attaches them (the
            // composer highlights when the drag passes over it).
            .on_drop(cx.listener(Self::on_file_drop))
            // ── sidebar ── (hidden while the settings surface is open —
            // settings is a full-window surface with its own nav, like the
            // reference UI). The panel stays mounted and slides: an animated
            // outer width clips a fixed-width inner column, so the content
            // never reflows mid-slide. The titlebar row above is outside the
            // panel, so the controls hold their place while it moves.
            .children((!self.settings_open).then(|| {
                let open = sidebar_shown;
                let panel_w = f32::from(self.sidebar_width);
                let panel = div()
                    .flex_none()
                    .h_full()
                    .overflow_hidden()
                    .bg(theme.panel_fill(theme.surface))
                    // The rail is the one structural pane that frosts: the
                    // wallpaper stays sharp beside it, so the image reads as
                    // something the rail is laid over rather than painted on.
                    .frosted_pane(&theme)
                    .child(
                        div()
                            .id("sidebar")
                            .relative()
                            .w(self.sidebar_width)
                            .h_full()
                            .flex()
                            .flex_col()
                            // Keyboard navigation: while this column holds
                            // focus (⌘⇧B) the `Sidebar` context routes the
                            // arrows/Enter/Escape to the row cursor.
                            .track_focus(&self.sidebar_focus)
                            .key_context("Sidebar")
                            // The minimum sidebar width (272px) leaves the
                            // wordmark, OS buttons and overlaid controls
                            // clear of each other at its own lead.
                            .child(window_drag_region(
                                div().h(px(TOP_BAR_H)).w_full().relative().child({
                                    let controls_left = titlebar_controls_left(true, panel_w);
                                    let mark_left = if cfg!(windows) {
                                        controls_left + TITLEBAR_CONTROLS_W + TITLEBAR_TITLE_GAP
                                    } else {
                                        // A little breathing room past the traffic
                                        // lights: 80px clearance lands the wordmark
                                        // flush on their edge, so give it its own
                                        // margin like the controls' TITLEBAR_CONTROLS_LEAD.
                                        TRAFFIC_LIGHT_CLEARANCE + TITLEBAR_CONTROLS_LEAD
                                    };
                                    div()
                                        .absolute()
                                        .left(px(mark_left))
                                        .top_0()
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .child(embedded_image_w(
                                            if theme.mode == ThemeMode::Light {
                                                crate::app_icon::LOGO_DARK_ASSET
                                            } else {
                                                crate::app_icon::LOGO_ASSET
                                            },
                                            px(SIDEBAR_WORDMARK_W),
                                        ))
                                }),
                            ))
                            // Resize handle: drag the sidebar's right edge to
                            // adjust its width. Kept fully inside the panel so
                            // the slide wrapper's clip doesn't halve its hit
                            // area. The drag-move listener lives on the root so
                            // the drag keeps tracking beyond the handle.
                            .child(
                                resize_handle(
                                    "sidebar-resize-handle",
                                    &theme,
                                    ResizeAxis::Horizontal,
                                    ResizeLook::Seam,
                                )
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right_0()
                                .w(px(6.))
                                .on_drag(SidebarResize, |_, _, _, cx| cx.new(|_| DragGhost)),
                            )
                            // Search and session actions share a stable toolbar.
                            .child(
                                div()
                                    .px_2()
                                    .pb_1()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(self.sidebar_search_bar(theme))
                                    .child(sidebar_options_button(
                                        self.sidebar_group_by,
                                        self.workspace_sort,
                                        self.sidebar_archived_filter,
                                        show_first_message,
                                        show_updated,
                                        self.sidebar_show_menu,
                                        self.naming_layout,
                                        self.layout_name.clone(),
                                        self.sidebar_sort_menu,
                                        cx.entity(),
                                        theme,
                                    ))
                                    .child(self.sidebar_new_task_button(theme, cx)),
                            )
                            // session list (scrolls), grouped by workspace — or
                            // one flat list when grouping is off. The section
                            // header stays mounted either way so the view
                            // toggle is reachable even with no sessions.
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    // Section label stays visible in every grouping.
                                    // A quiet eyebrow: small semibold muted
                                    // label, then a hairline to the edge, so
                                    // the section reads as a heading without a
                                    // box. The left inset matches a project
                                    // header's chevron column.
                                    .child(
                                        div()
                                            .px(DynamicSpacing::Base16.px(&theme))
                                            .pt(DynamicSpacing::Base04.px(&theme))
                                            .flex()
                                            .items_center()
                                            .gap(DynamicSpacing::Base08.px(&theme))
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_size(TextSize::XSmall.px(&theme))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_muted)
                                                    .child(match self.sidebar_group_by {
                                                        SidebarGroupBy::OneList => {
                                                            tr!("sidebar.all_sessions")
                                                        }
                                                        SidebarGroupBy::Recent => {
                                                            tr!("sidebar.recent")
                                                        }
                                                        SidebarGroupBy::Date => {
                                                            tr!("sidebar.all_sessions")
                                                        }
                                                        SidebarGroupBy::Workspace => {
                                                            tr!("sidebar.projects")
                                                        }
                                                    }),
                                            )
                                            // How many sessions the list holds,
                                            // in the label's own quiet ink.
                                            .when(listed_sessions > 0, |label| {
                                                label.child(
                                                    div()
                                                        .debug_selector(|| {
                                                            "sidebar-session-count".into()
                                                        })
                                                        .flex_none()
                                                        .text_size(TextSize::XSmall.px(&theme))
                                                        .text_color(theme.text_muted)
                                                        .child(listed_sessions.to_string()),
                                                )
                                            })
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w(DynamicSpacing::Base16.px(&theme))
                                                    .h(BORDER_WIDTH)
                                                    .bg(theme.hairline_subtle()),
                                            ),
                                    )
                                    .child(if side_rows.is_empty() {
                                        if self.sidebar_query.is_empty() {
                                            empty_sessions_state(theme, cx.entity())
                                                .into_any_element()
                                        } else {
                                            empty_search_state(
                                                &theme,
                                                &self.sidebar_query,
                                                self.sidebar_search.clone(),
                                            )
                                        }
                                    } else {
                                        div()
                                            .id("sidebar-sessions")
                                            .on_scroll_wheel(cx.listener(|app, _, _, cx| {
                                                app.clear_session_card(cx);
                                            }))
                                            .flex_1()
                                            .min_h_0()
                                            .px_2()
                                            .pb_2()
                                            .relative()
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h_full()
                                                    .relative()
                                                    // Clips the pinned header as
                                                    // the next group pushes it
                                                    // up past the list top.
                                                    .overflow_hidden()
                                                    .child(
                                                        crate::ui::list::virtual_list(
                                                            self.sidebar_list.clone(),
                                                            move |ix, _window, cx| {
                                                                let row_workspace_menu =
                                                                    if sticky_ix == Some(ix) {
                                                                        None
                                                                    } else {
                                                                        workspace_menu
                                                                            .as_ref()
                                                                            .as_ref()
                                                                    };
                                                                render_side_row(
                                                                    &side_rows,
                                                                    &sessions_data,
                                                                    active_path.as_deref(),
                                                                    ix,
                                                                    &this,
                                                                    agent_running,
                                                                    &running_paths,
                                                                    &pinned,
                                                                    &archived_paths,
                                                                    show_workspace,
                                                                    show_first_message,
                                                                    show_updated,
                                                                    show_shortcuts,
                                                                    &live_paths,
                                                                    &marks,
                                                                    &workspace_icons,
                                                                    &logos,
                                                                    &branches,
                                                                    &review_marks,
                                                                    &session_statuses,
                                                                    session_menu.as_ref().as_ref(),
                                                                    row_workspace_menu,
                                                                    &SidebarDragPaint {
                                                                        project: project_drag
                                                                            .as_ref()
                                                                            .as_ref(),
                                                                        session: session_drag
                                                                            .as_ref()
                                                                            .as_ref(),
                                                                        interactive: true,
                                                                        sessions:
                                                                            sessions_reorderable,
                                                                    },
                                                                    sidebar_cursor == Some(ix),
                                                                    this.read(cx)
                                                                        .session_card
                                                                        .as_deref(),
                                                                    keyboard_card,
                                                                    *theme::get(cx),
                                                                )
                                                                .into_any_element()
                                                            },
                                                        )
                                                        .w_full(),
                                                    )
                                                    .children(sticky_header),
                                            )
                                            .into_any_element()
                                    }),
                            )
                            // Connection status and global actions share a quiet footer.
                            .child(
                                div()
                                    .id("sidebar-footer")
                                    .debug_selector(|| "sidebar-footer".to_string())
                                    .h(px(48.))
                                    .flex_none()
                                    .px(px(12.))
                                    .bg(theme.panel_fill(theme.surface_secondary))
                                    .border_t_1()
                                    .border_color(theme.hairline())
                                    .flex()
                                    .items_center()
                                    .child({
                                        let live = self.runtime_is_live();
                                        let retrying = self.sidebar_runtime_retrying();
                                        let (label, dot) = if live {
                                            (tr!("status.connected"), theme.success)
                                        } else if retrying {
                                            (tr!("sidebar.status_retrying"), theme.warning)
                                        } else {
                                            (tr!("status.offline"), theme.error)
                                        };
                                        div()
                                            .id("sidebar-runtime-status")
                                            .flex()
                                            .flex_shrink()
                                            .min_w_0()
                                            .h(px(28.))
                                            .items_center()
                                            .gap(DynamicSpacing::Base06.px(&theme))
                                            .when(!live && !retrying, |status| {
                                                status
                                                    .px(px(4.))
                                                    .rounded(button::RADIUS.px(&theme))
                                                    .cursor_pointer()
                                                    .tip(tr!("view.reconnect"))
                                                    .hover_state(&theme)
                                                    .pressed_state(&theme, PressedRamp::Wash)
                                                    .on_mouse_up(
                                                        MouseButton::Left,
                                                        cx.listener(
                                                            |this, _: &MouseUpEvent, _, cx| {
                                                                this.runtime_restart(cx)
                                                            },
                                                        ),
                                                    )
                                            })
                                            .text_color(theme.text_secondary)
                                            .child({
                                                let dot = div()
                                                    .flex_none()
                                                    .size(px(6.))
                                                    .rounded_full()
                                                    .bg(dot);
                                                if retrying && !theme::reduce_motion(cx) {
                                                    dot.with_animation(
                                                        "sidebar-runtime-retrying",
                                                        Animation::new(looping::SPIN).repeat(),
                                                        |dot, phase| {
                                                            let pulse = (phase
                                                                * std::f32::consts::TAU)
                                                                .cos();
                                                            dot.opacity(0.7 + 0.3 * pulse)
                                                        },
                                                    )
                                                    .into_any_element()
                                                } else {
                                                    dot.into_any_element()
                                                }
                                            })
                                            .child(
                                                div()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_size(TextSize::Small.px(&theme))
                                                    .child(label),
                                            )
                                    })
                                    .child(div().flex_1())
                                    .when_some(
                                        self.sidebar_updater_button(theme, cx),
                                        |footer, button| {
                                            // Reserve the pill's expanded width
                                            // so its hover reveal grows into
                                            // empty space, not over the label.
                                            footer
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .w(theme.ui_px(
                                                            super::updater_ui::
                                                                UPDATER_PILL_EXPANDED_W,
                                                        ))
                                                        .h_full()
                                                        .flex()
                                                        .items_center()
                                                        .justify_end()
                                                        .child(button),
                                                )
                                                .child(div().w(px(8.)))
                                        },
                                    )
                                    .child(self.sidebar_footer_actions(theme, cx)),
                            ),
                    );
                let gen = self.sidebar_slide_gen;
                if gen == 0 || theme::reduce_motion(cx) {
                    // Settled state — including the first frame, so the panel
                    // doesn't slide open on launch.
                    panel
                        .w(px(if open { panel_w } else { 0. }))
                        .into_any_element()
                } else {
                    panel
                        .with_animation(
                            ElementId::Name(format!("sidebar-slide-{gen}").into()),
                            Animation::new(Motion::Layout.duration(&theme))
                                .with_easing(Motion::Layout.easing()),
                            move |el, d| {
                                let w = if open {
                                    panel_w * d
                                } else {
                                    panel_w * (1.0 - d)
                                };
                                el.w(px(w))
                            },
                        )
                        .into_any_element()
                }
            }))
            // ── main ──
            .child(if self.settings_open {
                // Settings owns the whole column — it has its own nav.
                self.render_settings(window, cx).into_any_element()
            } else if !self.dependencies_ready() {
                // Missing runtime pieces (pi / node): show the setup page
                // with install commands instead of the empty composer.
                self.render_onboarding(cx).into_any_element()
            } else if self.setup_open {
                // The same page on request, from Settings → About.
                self.render_onboarding(cx).into_any_element()
            } else {
                // The shared top bar is always the column's first row. A
                // feature page (Files / Git / Usage) opens as a card below it;
                // the chat fills the column itself.
                // `new_task_backdrop` sets `backdrop_live` when it actually
                // paints the field; nothing above needs to guess.
                // top bar — the window's left controls float over it in the
                // titlebar overlay, so its leading clears them when the sidebar
                // is collapsed. The drag spacer between the title and the right
                // cluster drags the window.
                let main_top_bar = {
                    let bar = div()
                        .h(px(TOP_BAR_H))
                        .flex_none()
                        .w_full()
                        .flex()
                        .items_center()
                        // The bar spans the main area to the window's right
                        // edge, so it owns the caption buttons.
                        .pr(px(if platform::draws_window_controls() {
                            platform::WINDOW_CONTROLS_W
                        } else {
                            12.
                        }))
                        .child({
                            let left = div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .h_full()
                                .flex()
                                .items_center();
                            if let Some(leading) = git_leading
                                .or(usage_leading)
                                .or(worktrees_leading)
                                .or(terminal_leading)
                            {
                                // A feature page's leading controls (Back and the
                                // page title) are interactive, so they sit outside
                                // the drag region — pressing them must not start a
                                // window move. The bar after them still drags the
                                // window.
                                left.child(leading)
                                    .child(window_drag_region(div().flex_1().min_w_0().h_full()))
                            } else {
                                // `folder / session`: the folder the session
                                // runs in, then its title. The folder keeps its
                                // width; a long title truncates first.
                                let folder = top_bar_folder_label(
                                    self.remote.as_ref().map(|host| host.dir.as_str()),
                                    self.current_workspace.as_deref(),
                                );
                                window_drag_region(left).child(
                                    div()
                                        .debug_selector(|| "top-bar-title".into())
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .items_center()
                                        .gap(DynamicSpacing::Base06.px(&theme))
                                        .text_size(TextSize::Default.px(&theme))
                                        .when_some(folder, |line, folder| {
                                            line.child(icon(
                                                "icons/folder.svg",
                                                IconSize::Small.px(&theme),
                                                theme.text_muted,
                                            ))
                                            .child(
                                                div()
                                                    .flex_shrink()
                                                    .min_w_0()
                                                    .max_w(px(200.))
                                                    .truncate()
                                                    .text_color(theme.text_muted)
                                                    .child(folder),
                                            )
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .text_color(theme.text_muted)
                                                    .child("/"),
                                            )
                                        })
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_color(theme.text_secondary)
                                                .child(session_display_title(
                                                    self.session_name.as_deref(),
                                                    self.current_title.as_deref(),
                                                )),
                                        ),
                                )
                            }
                        })
                        .child(top_controls);
                    let gen = self.sidebar_slide_gen;
                    if gen == 0 || theme::reduce_motion(cx) {
                        bar.pl(px(if sidebar_shown { 20. } else { TITLEBAR_LEADING }))
                            .into_any_element()
                    } else {
                        let expanding = sidebar_shown;
                        bar.with_animation(
                            ElementId::Name(format!("topbar-lead-{gen}").into()),
                            Animation::new(Motion::Layout.duration(&theme))
                                .with_easing(Motion::Layout.easing()),
                            move |el, d| {
                                let (from, to) = if expanding {
                                    (TITLEBAR_LEADING, 20.)
                                } else {
                                    (20., TITLEBAR_LEADING)
                                };
                                el.pl(px(from + (to - from) * d))
                            },
                        )
                        .into_any_element()
                    }
                };
                let body: AnyElement = if pane_full {
                    // The expanded Review pane owns the page; the chat is not
                    // just covered, it is skipped — same as a feature page.
                    div().into_any_element()
                } else if self.git_open && self.remote.is_some() {
                    // The host's own Git facts for the session's folder; the
                    // local Git page would describe this machine instead.
                    self.render_remote_git_page(theme, cx)
                } else if let (Some(host), true) =
                    (self.remote_host_name(), viewer_open || self.worktrees_open)
                {
                    // These pages read the local disk; on a remote session
                    // they would describe the wrong machine.
                    remote_unavailable(theme, host)
                } else if viewer_open {
                    self.file_viewer.clone().into_any_element()
                } else if self.git_open {
                    self.git_panel.clone().into_any_element()
                } else if self.usage_open {
                    self.usage.clone().into_any_element()
                } else if self.worktrees_open {
                    self.render_worktrees_page(cx).into_any_element()
                } else {
                    // ── the chat column ──
                    // One column for both pages. The new-task page is not a
                    // second layout: it is this column with the transcript
                    // empty, so the composer takes the middle of the page by
                    // growing the slot above it and keeping a spacer below.
                    // The first send eases both weights to the session's (see
                    // `new_task::settle`), and that is the composer docking.
                    if !empty && std::mem::take(&mut self.new_task_settle_pending) {
                        self.new_task_settle_at = Some(Instant::now());
                    }
                    let settling = !empty && new_task::settling(self.new_task_settle_at, &theme);
                    let composer_theme = theme.for_composer(empty);
                    let settle_gen = self.new_task_settle_gen;
                    let page_w = new_task::column_width(main_width);
                    let session_w = px(session_column);
                    let page_frame = new_task::settle_page(page_w);
                    let session_frame = new_task::settle_session(session_w);
                    // The column's width for this frame: the page's narrow one,
                    // the session's, or the one between them mid-dock (the
                    // animation below owns that last case).
                    let column_w = if empty { page_w } else { session_w };

                    // The transcript's slot. Empty on the new-task page — the
                    // composer owns the page and the slot is the room above it —
                    // and the conversation otherwise. Its element id is stable
                    // across that boundary, so the slot is the same box before
                    // and after the first send.
                    let mut slot = div()
                        .id("transcript-slot")
                        .debug_selector(|| "transcript-slot".to_string())
                        .min_h_0()
                        .w_full()
                        .relative();
                    if !empty {
                        slot = slot
                            .child(self.transcript.render(
                                review_workspace.as_deref(),
                                window.viewport_size().height,
                                main_width,
                                self.agent_calls,
                                Some(self.review_opener(cx)),
                                Some(self.image_opener(cx)),
                                Some(self.rewind_opener(cx)),
                                Some(self.regenerate_opener(cx)),
                                Some(self.response_exporter(cx)),
                                self.search_hits(),
                                self.search_active(),
                                cx,
                            ))
                            // In-transcript find bar (⌘F), floating over the
                            // top-right of the transcript.
                            .children(self.transcript_search_bar(cx));
                    }
                    // The slot always owns whatever the composer does not: on
                    // the page that is the space above it, in a session the whole
                    // column (`settle_page`/`settle_session` carry the weights).
                    let slot = new_task::weight(
                        slot,
                        if empty {
                            page_frame.slot
                        } else {
                            session_frame.slot
                        },
                    );
                    // Ink arrives with the dock rather than before it, so the
                    // first message is read in the session layout, not through a
                    // half-moved page.
                    let slot = if settling {
                        slot.with_animation(
                            ElementId::NamedInteger("new-task-settle-ink".into(), settle_gen),
                            Animation::new(Motion::Travel.duration(&theme))
                                .with_easing(new_task::slide_easing),
                            move |el, progress| el.opacity(new_task::transcript_ink(progress)),
                        )
                        .into_any_element()
                    } else {
                        slot.into_any_element()
                    };

                    // The room under the composer. A session has none — the
                    // composer is docked — so this exists only while the page is
                    // centered, and eases away as the composer docks.
                    let gap = if empty {
                        Some(
                            new_task::weight(
                                div()
                                    .id("new-task-gap")
                                    .debug_selector(|| "new-task-gap".to_string())
                                    .w_full(),
                                page_frame.gap,
                            )
                            .into_any_element(),
                        )
                    } else if settling {
                        Some(
                            div()
                                .id("new-task-gap")
                                .debug_selector(|| "new-task-gap".to_string())
                                .w_full()
                                .with_animation(
                                    ElementId::NamedInteger(
                                        "new-task-settle-gap".into(),
                                        settle_gen,
                                    ),
                                    Animation::new(Motion::Travel.duration(&theme))
                                        .with_easing(new_task::slide_easing),
                                    move |el, progress| {
                                        new_task::weight(
                                            el,
                                            new_task::settle(progress, page_w, session_w).gap,
                                        )
                                    },
                                )
                                .into_any_element(),
                        )
                    } else {
                        None
                    };

                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .w_full()
                        .flex()
                        .flex_col()
                        .relative()
                        .child(slot)
                        // floating composer + status bar — one centered column
                        .child({
                            // The column's own width, not its child's: the
                            // padding that used to centre it (`px_4`) is now
                            // inside the number, so the same element can carry
                            // the page's narrow width, the session's, and the
                            // dock between them.
                            let wrap = div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .mx_auto()
                                // The page padding stays on this box, so the
                                // column inside it is exactly `column_w`: the
                                // same 16px the column's width is computed
                                // against, wherever the composer sits.
                                .w(px(f32::from(column_w) + 2. * new_task::PAGE_PAD_X))
                                .px_4()
                                .pb_4()
                                // One centered column: composer + status bar share
                                // the same width so the folder/meta row always
                                // aligns to the composer's edges.
                                .child(
                                    div()
                                        // A stable element identity keeps the
                                        // popovers and inline bars below from
                                        // re-keying when a sibling appears, so
                                        // their animations never restart mid-way.
                                        .id("composer-column")
                                        .max_w(px(CONTENT_MAX_W))
                                        .w_full()
                                        .flex()
                                        .flex_col()
                                        // `/`-command and `@`-file menu — anchored
                                        // above the composer box (same deferred
                                        // + anchored pattern as the chip pickers)
                                        .children(self.autocomplete_popup(cx))
                                        // Command/protocol failures, above the
                                        // queue and composer.
                                        .children(self.error_banner(theme, cx))
                                        // In-flight retry / compaction state —
                                        // persistent while active, never a
                                        // transient status line.
                                        .children(self.run_status_strip(cx))
                                        // Access-guard approval — inline, above
                                        // the queue and composer (no scrim modal).
                                        .children(self.ask_panel(cx))
                                        .children(self.approval_bar(cx))
                                        // Queued follow-ups wait here (sticky above
                                        // the composer) until the task finishes.
                                        .children(self.queue_bar(cx))
                                        // Extension `setWidget` blocks placed
                                        // above the editor.
                                        .children(self.extension_widgets_above(cx))
                                        // Composer and its context share one surface.
                                        // Picker popups remain anchored to their chips.
                                        .child(
                                            div()
                                                .id("composer-box")
                                                .debug_selector(|| "composer-box".to_string())
                                                .w_full()
                                                .relative()
                                                .bg(composer_theme.composer_fill())
                                                .rounded(input::composer_radius(&theme))
                                                .frosted_card(
                                                    &composer_theme,
                                                    Self::composer_border_color(
                                                        &theme,
                                                        self.file_drag_hovered,
                                                    ),
                                                    input::composer_radius(&theme),
                                                )
                                                .shadow(theme.composer_shadow())
                                                .text_size(TextSize::Default.px(&theme))
                                                .flex()
                                                .flex_col()
                                                .on_drag_move(cx.listener(Self::on_file_drag_move))
                                                .children((empty || settling).then(|| {
                                                    self.new_task_folder_row(
                                                        theme, settling, settle_gen, cx,
                                                    )
                                                }))
                                                .child(
                                                    div()
                                                        .min_w_0()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(DynamicSpacing::Base16.px(&theme))
                                                        .px(DynamicSpacing::Base16.px(&theme))
                                                        .py(DynamicSpacing::Base12.px(&theme))
                                                        .on_mouse_up(
                                                            MouseButton::Left,
                                                            cx.listener(Self::on_composer_click),
                                                        )
                                                        .children(self.attachments_row(cx))
                                                        .child(self.input.clone())
                                                        .child(self.composer_row(
                                                            composer_compact,
                                                            window,
                                                            cx,
                                                        )),
                                                )
                                                // Drop-target overlay: fades
                                                // in over the box while files are
                                                // dragged across it. Absolute, so
                                                // highlighting never shifts layout.
                                                .children(self.file_drag_hovered.then(|| {
                                                    let overlay = div()
                                                        .absolute()
                                                        .inset_0()
                                                        .rounded(input::composer_radius(&theme))
                                                        .bg(theme.surface_secondary.opacity(0.92))
                                                        .border_1()
                                                        .border_color(theme.accent)
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .gap_2()
                                                        .child(icon(
                                                            "icons/plus.svg",
                                                            IconSize::Small.px(&theme),
                                                            theme.accent_text,
                                                        ))
                                                        .child(
                                                            div()
                                                                .text_size(
                                                                    TextSize::Small.px(&theme),
                                                                )
                                                                .font_weight(FontWeight::MEDIUM)
                                                                .text_color(theme.accent_text)
                                                                .child(tr!(
                                                                    "composer.drop_to_attach"
                                                                )),
                                                        );
                                                    popover_enter("drop-overlay", overlay, &theme)
                                                })),
                                        )
                                        // Extension `setWidget` blocks placed
                                        // below the editor.
                                        .children(self.extension_widgets_below(cx))
                                        .child(self.status_bar(
                                            &workspace_label,
                                            empty || settling,
                                            cx,
                                        )),
                                );

                            // The width rides the dock: the composer widens
                            // from the page's column to the session's as it
                            // descends, on the same role and clock as the gap
                            // above, so the two read as one move.
                            if settling {
                                wrap.with_animation(
                                    ElementId::NamedInteger(
                                        "new-task-settle-column".into(),
                                        settle_gen,
                                    ),
                                    Animation::new(Motion::Travel.duration(&theme))
                                        .with_easing(new_task::slide_easing),
                                    move |el, progress| {
                                        let width =
                                            new_task::settle(progress, page_w, session_w).width;
                                        el.w(px(f32::from(width) + 2. * new_task::PAGE_PAD_X))
                                    },
                                )
                                .into_any_element()
                            } else {
                                wrap.into_any_element()
                            }
                        })
                        // Room under the composer while the page is centered;
                        // it eases to nothing as the composer docks, and is not
                        // mounted at all in a session.
                        .children(gap)
                        // bottom terminal panel — the last row of the page, under
                        // the composer, so it hugs the window's bottom edge. An
                        // expanded panel is the page itself (see below).
                        .children(
                            (terminal_visible && !terminal_full)
                                .then(|| self.terminal_panel.clone().into_any_element()),
                        )
                        .into_any_element()
                };
                // The top bar stays put; a feature page opens as a card below
                // it, the chat fills the column itself.
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .min_h_0()
                    .relative()
                    // Full-window wallpaper shows through the main column; in
                    // every other mode this resolves to the same opaque
                    // background the window already paints.
                    .bg(theme.panel_fill(theme.background))
                    .children(if feature_open || pane_full {
                        None
                    } else {
                        self.new_task_backdrop(
                            theme,
                            self.backdrop_phase,
                            empty,
                            theme::reduce_motion(cx),
                        )
                    })
                    .children(if feature_open || pane_full {
                        None
                    } else {
                        self.chat_backdrop(theme, empty)
                    })
                    .child(main_top_bar)
                    // The top bar spans the whole main area; the docks and
                    // the chat/feature sit side by side below it.
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .flex()
                            .relative()
                            .child(if pane_full {
                                div().into_any_element()
                            } else if feature_open {
                                feature_card(theme, body)
                            } else if terminal_full {
                                // The expanded shell owns the page the way Git
                                // and Usage do: the chat is skipped and the
                                // panel fills the card under the top bar.
                                feature_card(theme, self.terminal_panel.clone().into_any_element())
                            } else {
                                body
                            })
                            .children(
                                explorer_visible
                                    .then(|| self.project_panel.clone().into_any_element()),
                            )
                            .children(
                                pane_visible.then(|| self.sidepane.clone().into_any_element()),
                            )
                            .children(
                                details_visible.then(|| self.render_session_details_panel(cx)),
                            ),
                    )
                    .into_any_element()
            })
            // ── titlebar controls ── a fixed overlay pinned just past the
            // macOS traffic lights, above both the sidebar and the main column.
            // Shown on every surface except Settings, which owns its own nav
            // column — including the Git/Usage pages when the sidebar is
            // collapsed, so the toggle (and history arrows) stay reachable;
            // those pages inset their own headers to clear it. The controls
            // right-align inside the sidebar's strip when it is open (hugging
            // its edge, and tracking it through a resize or the slide) and
            // fall back to the fixed lead when it is collapsed; on Windows,
            // which has no left-hand OS buttons, they take that fixed lead
            // whether it is open or not. The container
            // is not itself a hitbox, so the drag strip beneath still drags the
            // window in the gaps between buttons while each button takes its
            // own clicks.
            .children((!self.settings_open).then(|| {
                let pinned = titlebar_controls_left(false, f32::from(self.sidebar_width));
                let hugging = titlebar_controls_left(true, f32::from(self.sidebar_width));
                let controls = div()
                    .absolute()
                    .top_0()
                    .h(px(TOP_BAR_H))
                    .flex()
                    .items_center()
                    .child(self.titlebar_left_controls(theme, cx));
                let gen = self.sidebar_slide_gen;
                if gen == 0 || theme::reduce_motion(cx) {
                    controls
                        .left(px(if sidebar_shown { hugging } else { pinned }))
                        .into_any_element()
                } else {
                    // Same easing and duration as the panel's own slide, so the
                    // chips ride the edge instead of snapping to it.
                    let expanding = sidebar_shown;
                    controls
                        .with_animation(
                            ElementId::Name(format!("titlebar-lead-{gen}").into()),
                            Animation::new(Motion::Layout.duration(&theme))
                                .with_easing(Motion::Layout.easing()),
                            move |el, d| {
                                let (from, to) = if expanding {
                                    (pinned, hugging)
                                } else {
                                    (hugging, pinned)
                                };
                                el.left(px(from + (to - from) * d))
                            },
                        )
                        .into_any_element()
                }
            }))
            // ── window caption buttons ── the app owns the caption on the
            // platforms where it draws it (`platform::draws_window_controls`),
            // so these sit above *every* surface — settings, git, usage,
            // onboarding, and the session view — and only the headers that
            // reach the window's right edge keep their content clear of them.
            // Drawn before the deferred layers, so an open popover still wins.
            .children(platform::draws_window_controls().then(|| {
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .h(px(TOP_BAR_H))
                    .flex()
                    .items_center()
                    .child(window_controls(theme, window.is_maximized()))
                    .into_any_element()
            }))
            // ── command palette (⌘P) — a full-window deferred layer above
            // every other floating surface; the entity renders its own
            // absolute scrim + centered card.
            .children(
                self.command_palette
                    .clone()
                    .map(|palette| command_palette::layer(palette).into_any_element()),
            )
            // ── rewind picker — the conversation's fork points, a modal opened
            // from a message's footer or from the palette.
            .children(
                self.rewind_picker
                    .clone()
                    .map(|picker| crate::rewind_picker::layer(picker).into_any_element()),
            )
            // ── extension dialog (select / confirm / input / editor) — a
            // blocking modal above every other surface; pi holds the run until
            // the user answers. It cancels the incoming request otherwise.
            .children(dialog_layer.map(|dialog| crate::dialog::layer(dialog).into_any_element()))
            // ── extension custom UI (`ctx.ui.custom`) — the component's own
            // rendered frames, stacked with the newest on top.
            .children(
                self.custom_ui
                    .iter()
                    .map(|ui| crate::custom_ui::layer(ui.clone()).into_any_element()),
            )
            // ── worktrees dialog — create / rename / move / remove,
            // opened from the page and its row menus.
            .children(self.worktree_dialog_layer(theme, cx.entity(), cx))
            // ── SSH password — asked when a remote host refuses every key;
            // above Settings (the Remote hosts form) and the New Task page.
            // ── Add to a server — the clone dialog; under the password prompt
            // its connection check may raise.
            .children(self.server_clone_layer(theme, cx.entity(), cx))
            .children(self.remote_password_layer(theme, cx.entity(), cx))
            // ── update modal — the search, changelog, and install decision,
            // opened by the download control and Check for Updates. Below the
            // extension dialog (a run blocks on it) and the lightbox.
            .children(self.updater_dialog_layer(window, cx))
            // ── image lightbox — full-window, above everything; opened from a
            // transcript image tile, dismissed by click or Escape.
            .children(
                self.lightbox
                    .clone()
                    .map(|image| self.lightbox_layer(image, cx)),
            )
            // ── toasts — the in-app stack, topmost so a notification is
            // never buried by whatever surface happens to be open.
            .children(self.toast_layer(cx))
            .track_focus(&self.focus_handle(cx))
            .on_modifiers_changed(
                cx.listener(|app, event: &gpui::ModifiersChangedEvent, _, cx| {
                    let show = app.window_active && show_session_shortcuts(event.modifiers);
                    if app.sidebar_shortcut_hints != show {
                        app.sidebar_shortcut_hints = show;
                        cx.notify();
                    }
                }),
            )
            // Sidebar resize: fires for every mouse move while the handle
            // drag is active, wherever the pointer travels.
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<SidebarResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    let max = event.bounds.size.width - px(400.);
                    let width = event
                        .event
                        .position
                        .x
                        .clamp(px(SIDEBAR_MIN_W), max.max(px(SIDEBAR_MIN_W)));
                    if width != app.sidebar_width {
                        app.sidebar_width = width;
                        crate::layout::set_sidebar_width(f32::from(width));
                        cx.notify();
                    }
                },
            ))
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<crate::explorer::ExplorerResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    // The dock sits between the main column and the Review
                    // pane, so its width is the pointer's distance to its
                    // right edge — which the pane owns when it is open.
                    let pane = if app.sidepane.read(cx).is_open()
                        && !app.settings_open
                        && !app.usage_open
                        && app.dependencies_ready()
                    {
                        app.sidepane.read(cx).width()
                    } else {
                        px(0.)
                    };
                    let width = event.bounds.size.width - pane - event.event.position.x;
                    app.project_panel
                        .update(cx, |panel, cx| panel.set_width(width, cx));
                },
            ))
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<SidePaneResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    // The pane hugs the window's right edge, so its width is
                    // the distance from the pointer to that edge.
                    let width = event.bounds.size.width - event.event.position.x;
                    let max = event.bounds.size.width - px(PANE_MAX_RESERVE);
                    app.sidepane.update(cx, |pane, cx| {
                        pane.set_width(width.min(max), cx);
                    });
                },
            ))
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<TerminalResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    // The panel hugs the window's bottom edge, so its height is
                    // the distance from the pointer to that edge, capped so the
                    // transcript above it keeps a usable height.
                    let max = (event.bounds.size.height - px(TERMINAL_MAIN_RESERVE))
                        .max(px(TERMINAL_MAIN_RESERVE));
                    let height = (event.bounds.size.height - event.event.position.y).min(max);
                    app.terminal_panel
                        .update(cx, |panel, cx| panel.set_height(height, cx));
                },
            ))
            // A project header is being dragged across the sidebar: resolve
            // the landing group from the list's measured row bounds. Root-level
            // so a move between headers keeps tracking wherever the pointer is.
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<ProjectDrag>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    app.on_project_drag_move(event, cx);
                },
            ))
            // The same for a session row, whose landing row must be in the
            // session's own project group.
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<SessionDrag>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    app.on_session_drag_move(event, cx);
                },
            ))
            .on_action(cx.listener(Self::on_submit))
            .on_action(cx.listener(Self::on_steer))
            .on_action(cx.listener(Self::on_send_alternate))
            .on_action(cx.listener(Self::on_autocomplete_accept))
            .on_action(cx.listener(Self::on_abort))
            .on_action(cx.listener(Self::on_new_session))
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_copy_last_response))
            .on_action(cx.listener(Self::on_prev_turn))
            .on_action(cx.listener(Self::on_next_turn))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_open_about))
            .on_action(cx.listener(Self::on_toggle_usage))
            .on_action(cx.listener(Self::on_toggle_project_panel))
            .on_action(cx.listener(Self::on_close_files))
            .on_action(cx.listener(Self::on_toggle_terminal))
            .on_action(cx.listener(Self::on_new_terminal_tab))
            .on_action(cx.listener(Self::on_close_terminal_tab))
            .on_action(cx.listener(Self::on_next_terminal_tab))
            .on_action(cx.listener(Self::on_prev_terminal_tab))
            .on_action(cx.listener(Self::on_toggle_sidebar_action))
            .on_action(cx.listener(Self::on_focus_sessions))
            .on_action(cx.listener(Self::on_focus_session_search))
            .on_action(cx.listener(Self::on_open_session_slot))
            .on_action(cx.listener(Self::on_next_session))
            .on_action(cx.listener(Self::on_prev_session))
            .on_action(cx.listener(Self::on_sidebar_prev))
            .on_action(cx.listener(Self::on_sidebar_next))
            .on_action(cx.listener(Self::on_sidebar_home))
            .on_action(cx.listener(Self::on_sidebar_end))
            .on_action(cx.listener(Self::on_sidebar_confirm))
            .on_action(cx.listener(Self::on_sidebar_close))
            .on_action(cx.listener(Self::on_sidebar_move_row_up))
            .on_action(cx.listener(Self::on_sidebar_move_row_down))
            .on_action(cx.listener(Self::on_sidebar_open_row_menu))
            .on_action(cx.listener(Self::on_toggle_command_palette))
            // ⌘⌥P starts or stops a frame-time capture (debug builds bind the
            // chord; the body compiles away in release). Registered at the root
            // so it works from any surface.
            .on_action(cx.listener(Self::on_record_frame_times))
            .on_action(cx.listener(Self::on_check_for_updates))
            .on_action(cx.listener(Self::on_toggle_search))
            .on_action(cx.listener(Self::on_toggle_model_menu))
            .on_action(cx.listener(Self::on_toggle_thinking_menu))
            .on_action(cx.listener(Self::on_review_changes))
            .on_action(cx.listener(Self::on_open_git))
            .on_action(cx.listener(Self::on_open_shortcut_help))
            .on_action(cx.listener(Self::on_review_close))
            .on_action(cx.listener(Self::on_focus_next))
            .on_action(cx.listener(Self::on_focus_prev))
            .on_action(cx.listener(|this, _: &crate::GitTabChanges, window, cx| {
                this.on_git_tab(0, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabHistory, window, cx| {
                this.on_git_tab(1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabGraph, window, cx| {
                this.on_git_tab(2, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabStashes, window, cx| {
                this.on_git_tab(3, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabIssues, window, cx| {
                this.on_git_tab(4, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabPulls, window, cx| {
                this.on_git_tab(5, window, cx)
            }))
    }
}

impl OrbitApp {
    /// The extension widgets placed above the composer, if any.
    pub(super) fn extension_widgets_above(&self, cx: &Context<Self>) -> Option<AnyElement> {
        self.extension_widgets_element(WidgetPlacement::AboveEditor, cx)
    }

    /// The extension widgets placed below the composer, if any.
    pub(super) fn extension_widgets_below(&self, cx: &Context<Self>) -> Option<AnyElement> {
        self.extension_widgets_element(WidgetPlacement::BelowEditor, cx)
    }

    /// Render every keyed `setWidget` block for one placement as a small
    /// monospace card. The lines keep the extension's own SGR coloring (see
    /// [`crate::widgets`]), so a todo list or status block reads as intended.
    fn extension_widgets_element(
        &self,
        placement: WidgetPlacement,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let items: Vec<&ExtensionWidget> = self
            .extension_widgets
            .iter()
            .filter(|widget| widget.placement == placement && !widget.lines.is_empty())
            .collect();
        if items.is_empty() {
            return None;
        }
        let theme = *theme::get(cx);
        let font = ext_widgets::widget_font();
        let mut column = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base06.px(&theme));
        for widget in items {
            if widget.key == "orbit-follow-up-suggestions" {
                if !self.suggestions.enabled
                    || self.busy
                    || !self.input.read(cx).text().is_empty()
                    || self.attachment_slots_used() > 0
                {
                    continue;
                }
                let Some(payload) = widget
                    .lines
                    .first()
                    .and_then(|line| serde_json::from_str::<Value>(line).ok())
                else {
                    continue;
                };
                if payload.get("mode").and_then(Value::as_str) != Some(self.access_mode.as_wire())
                    || payload.get("workflow").and_then(Value::as_str)
                        != Some(self.workflow_mode.as_wire())
                {
                    continue;
                }
                let Some(suggestions) = payload.get("suggestions").and_then(Value::as_array) else {
                    continue;
                };
                // One row of chips, side by side: a suggestion is a prompt
                // the user scans and picks, not something to read in place.
                let mut list = div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap(DynamicSpacing::Base04.px(&theme));
                let mut count = 0;
                for (index, suggestion) in suggestions.iter().take(3).enumerate() {
                    let Some(text) = suggestion
                        .as_str()
                        .filter(|text| !text.is_empty() && text.len() <= 180)
                    else {
                        continue;
                    };
                    let text = text.to_owned();
                    let selected = text.clone();
                    let keyboard_text = text.clone();
                    count += 1;
                    list = list.child(
                        div()
                            .id(ElementId::Name(format!("suggestion-{index}").into()))
                            .debug_selector(move || format!("follow-up-suggestion-{index}"))
                            .focusable()
                            .focus(|style| style.border_color(theme.focus_ring))
                            .on_key_down(cx.listener(
                                move |app, event: &gpui::KeyDownEvent, window, cx| {
                                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                        cx.stop_propagation();
                                        app.insert_suggestion(&keyboard_text, window, cx);
                                    }
                                },
                            ))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .px(DynamicSpacing::Base08.px(&theme))
                            .py(DynamicSpacing::Base04.px(&theme))
                            .rounded(panel::RADIUS.px(&theme))
                            .border_1()
                            .border_color(theme.hairline())
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .tip(text.clone())
                            .cursor_pointer()
                            .hover_overlay(&theme)
                            .on_click(cx.listener(move |app, _, window, cx| {
                                app.insert_suggestion(&selected, window, cx);
                            }))
                            // The chip shows the start of the suggestion;
                            // the complete text stays on hover, and the
                            // suggestion is inserted unchanged on activation.
                            .child(
                                div()
                                    .debug_selector(move || format!("follow-up-text-{index}"))
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(text),
                            ),
                    );
                }
                if count > 0 {
                    let close = IconButton::new("dismiss-follow-up-suggestions", "icons/x.svg")
                        .size(ButtonSize::Compact)
                        .variant(IconButtonVariant::Glass)
                        .tip(tr!("common.dismiss"))
                        .render(&theme)
                        .debug_selector(|| "dismiss-follow-up-suggestions".to_string())
                        .focusable()
                        .focus(|style| style.border_1().border_color(theme.focus_ring))
                        .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                app.dismiss_suggestions(window, cx);
                            }
                        }))
                        .on_click(cx.listener(|app, _, window, cx| {
                            app.dismiss_suggestions(window, cx);
                        }));
                    column = column.child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(DynamicSpacing::Base04.px(&theme))
                            .pb(DynamicSpacing::Base04.px(&theme))
                            .child(list)
                            .child(close),
                    );
                }
                continue;
            }
            let selector = format!("ext-widget-{}", widget.key);
            let mut card = div()
                .id(ElementId::Name(selector.clone().into()))
                .debug_selector(move || selector.clone())
                .w_full()
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .rounded(panel::RADIUS.px(&theme))
                .border_1()
                .border_color(theme.hairline())
                .bg(theme.code_bg)
                .font_family(theme::code_font_family())
                .text_size(theme.code_px(12.))
                .line_height(theme.code_px(18.))
                .text_color(theme.code_text)
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base02.px(&theme))
                .max_h(px(160.))
                .overflow_y_scroll();
            for line in &widget.lines {
                let (text, runs) = ext_widgets::styled_line(line, &theme, &font);
                card = card.child(StyledText::new(text).with_runs(runs));
            }
            column = column.child(card);
        }
        Some(column.into_any_element())
    }

    /// Drop the follow-up suggestion chips. Every path that changes what they
    /// were generated for (a new run, a mode switch, a settings toggle) calls
    /// this, so the key lives here alone.
    pub(super) fn clear_follow_up_suggestions(&mut self) {
        self.extension_widgets
            .retain(|widget| widget.key != "orbit-follow-up-suggestions");
    }

    pub(super) fn dismiss_suggestions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_follow_up_suggestions();
        self.input.read(cx).focus(window);
        cx.notify();
    }

    fn insert_suggestion(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.suggestions.enabled
            && !self.busy
            && self.input.read(cx).text().is_empty()
            && self.attachment_slots_used() == 0
            && self
                .extension_widgets
                .iter()
                .any(|widget| widget.key == "orbit-follow-up-suggestions")
        {
            self.input.update(cx, |input, cx| input.set_text(text, cx));
            self.clear_follow_up_suggestions();
            self.input.read(cx).focus(window);
            cx.notify();
        }
    }

    /// Composer controls: access and workflow stay visible at every width.
    /// On a narrow column, they wrap above the model, thinking and send row.
    pub(super) fn composer_row(
        &self,
        compact: bool,
        window: &Window,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let modes = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.add_menu_button(cx))
            .child(self.access_chip(cx))
            .child(self.workflow_chip(cx));
        let send = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .when(compact, |row| row.w_full())
            .when(!compact, |row| row.flex_1())
            .child(div().flex_1().min_w_0())
            // Model and thinking level read as one setting — "Opus 5.5
            // Medium ▾" — so they sit nearly flush, with the caret on the end.
            // Each half still opens its own picker; the hairline gap keeps
            // their hover washes from merging into one blob.
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(self.model_chip(compact, cx))
                    .children(self.thinking_chip(cx)),
            )
            .child(self.send_controls(window, cx));
        div()
            .id("composer-row")
            .min_w_0()
            .flex()
            .when(compact, |row| row.flex_col().items_start())
            .when(!compact, |row| row.items_center())
            .gap_2()
            .child(modes)
            .child(send)
    }

    /// The access-mode chip: a lock glyph, the active mode's label, and a
    /// caret that turns over when the picker opens. Ghost style until
    /// hovered/open, matching the model chip.
    pub(super) fn access_chip(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        // Supervised is the default, so its name is not news: the chip keeps the
        // glyph alone and names the mode on hover instead. A widened mode
        // (Full access, Auto-accept edits) is a deliberate departure from the
        // default, so that one keeps its label — there the label is the warning,
        // and it should be readable without hovering.
        //
        // The rule reads the mode only, never the open menu: a chip that grew a
        // label on click would shove the workflow chip sideways under the
        // pointer, and the open picker already names the active mode.
        // A remote host runs Supervised whatever this Mac's mode is (its
        // launch script pins it), so the chip shows that and says why.
        let remote = self.remote_host_name().map(str::to_owned);
        let mode = if remote.is_some() {
            AccessMode::Supervised
        } else {
            self.access_mode
        };
        let label = mode.label();
        let terse = mode == AccessMode::default();
        let tip = match &remote {
            Some(host) => tr!("remote.access_chip_tip", host = host.clone()),
            None => label.clone(),
        };
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.access_popup(cx))
            .child({
                let chip = ChipButton::new("access-chip", ChipStyle::Quiet)
                    .selector("access-chip")
                    .icon(mode.icon())
                    .open(self.access_menu_open)
                    // The label already names the mode when it is visible;
                    // the terse (icon-only) chip names it in the tooltip.
                    .caret(!terse);
                let chip = if terse {
                    chip.tip(tip)
                } else {
                    chip.label(label)
                };
                chip.on_mouse_up(&theme, cx.listener(Self::on_access_trigger_click))
            })
    }

    /// The shared picker caret. `chevron-down` at rest, so the chip reads as a
    /// menu that opens below it; a half-turn while the menu is open (animated,
    /// reduce-motion aware) so the turn reads as a transition. Resting flat when
    /// closed keeps there from being a reverse flicker. `fg` is the caret color
    /// while open.
    pub(super) fn chip_caret(
        open: bool,
        fg: gpui::Hsla,
        animation_id: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = *theme::get(cx);
        // One implementation for every picker caret in the app: a half turn on
        // the Interaction role, instant under Reduce Motion, and the ink
        // quietens while the menu is closed. A tree disclosure keeps the
        // right/down pair in `disclosure_chevron`; these chips open a menu.
        dropdown_chevron(
            animation_id,
            open,
            IconSize::XSmall.px(&theme),
            if open { fg } else { theme.text_muted },
            &theme,
        )
    }

    /// The access-mode picker popup, while open. Minimal rows: an icon tile,
    /// the mode name over its one-line description, and an accent check on the
    /// active mode. The popup owns the keyboard via the `AccessMenu` context
    /// and eases in rather than popping.
    pub(super) fn access_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.access_menu_open {
            return None;
        }
        let theme = theme::get(cx).for_floating();
        let this = cx.weak_entity();
        let mut list = div().w_full().flex().flex_col();
        for (ix, mode) in AccessMode::ALL.iter().enumerate() {
            let mode = *mode;
            let highlighted =
                self.access_menu_highlight.selected(AccessMode::ALL.len()) == Some(ix);
            let selected = mode == self.access_mode;
            let this = this.clone();
            let entry = div().id(ElementId::NamedInteger("access-row".into(), ix as u64));
            list = list.child(
                picker_entry(entry, &theme)
                    .cursor_pointer()
                    .when(selected || highlighted, |row| {
                        row.bg(theme.selected_wash())
                    })
                    .when(!selected && !highlighted, |row| row.hover_state(&theme))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.access_menu_highlight.selected(AccessMode::ALL.len())
                                    != Some(ix)
                                {
                                    app.access_menu_highlight.highlight(ix);
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this = cx.weak_entity();
                        move |_, window, cx| {
                            this.update(cx, |app, cx| app.run_access_menu_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .children(option_row(
                        &theme,
                        mode.icon(),
                        mode.label(),
                        mode.description(),
                        selected,
                        highlighted,
                    )),
            );
        }

        let popup = popover_list(div(), &theme)
            .w(px(MODE_POPUP_W))
            .overflow_hidden()
            .key_context("AccessMenu")
            .track_focus(&self.access_menu_focus)
            .on_action(cx.listener(Self::on_access_menu_next))
            .on_action(cx.listener(Self::on_access_menu_prev))
            .on_action(cx.listener(Self::on_access_menu_confirm))
            .on_action(cx.listener(Self::on_access_menu_close))
            .dismiss_on_outside(cx.listener(|app, _, window, cx| {
                app.menu_dismissed_at.arm();
                app.close_access_menu(window, cx);
            }))
            .child(list);

        Some(crate::ui::popover::column_menu(
            popup,
            "anchored_popover-in-12",
            &theme,
        ))
    }

    /// The workflow-mode chip: the active session's scope, opening the
    /// Plan / Build / Ask picker. Ghost style until hovered/open, matching the
    /// access and model chips.
    pub(super) fn workflow_chip(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.workflow_popup(cx))
            .child({
                let chip = ChipButton::new("workflow-chip", ChipStyle::Quiet)
                    .selector("workflow-chip")
                    .icon(self.workflow_mode.icon())
                    .label(self.workflow_mode.label())
                    .open(self.workflow_menu_open);
                // Read-only modes cannot edit; the lock says so at a glance,
                // matching the access chip's motif.
                let chip = if self.workflow_mode.is_read_only() {
                    chip.trailing(icon(
                        "icons/lock.svg",
                        IconSize::Indicator.px(&theme),
                        theme.text_muted,
                    ))
                } else {
                    chip
                };
                chip.on_mouse_up(&theme, cx.listener(Self::on_workflow_trigger_click))
            })
    }

    /// The workflow-mode picker popup, while open. Same construction as the
    /// access popup: an icon tile, the mode over its one-line hint, and an
    /// accent check on the active mode.
    pub(super) fn workflow_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.workflow_menu_open {
            return None;
        }
        let theme = theme::get(cx).for_floating();
        let this = cx.weak_entity();
        let mut list = div().w_full().flex().flex_col();
        for (ix, mode) in WorkflowMode::ALL.iter().enumerate() {
            let mode = *mode;
            let highlighted = ix == self.workflow_menu_highlight.highlighted();
            let selected = mode == self.workflow_mode;
            let this = this.clone();
            let entry = div().id(ElementId::NamedInteger("workflow-row".into(), ix as u64));
            list = list.child(
                picker_entry(entry, &theme)
                    .cursor_pointer()
                    .when(selected || highlighted, |row| {
                        row.bg(theme.selected_wash())
                    })
                    .when(!selected && !highlighted, |row| row.hover_state(&theme))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.workflow_menu_highlight.highlighted() != ix {
                                    app.workflow_menu_highlight.highlight(ix);
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this = cx.weak_entity();
                        move |_, window, cx| {
                            this.update(cx, |app, cx| app.run_workflow_menu_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .children(option_row(
                        &theme,
                        mode.icon(),
                        mode.label(),
                        mode.description(),
                        selected,
                        highlighted,
                    )),
            );
        }

        let popup = popover_list(div(), &theme)
            .w(px(MODE_POPUP_W))
            .overflow_hidden()
            .key_context("WorkflowMenu")
            .track_focus(&self.workflow_menu_focus)
            .on_action(cx.listener(Self::on_workflow_menu_next))
            .on_action(cx.listener(Self::on_workflow_menu_prev))
            .on_action(cx.listener(Self::on_workflow_menu_confirm))
            .on_action(cx.listener(Self::on_workflow_menu_close))
            .dismiss_on_outside(cx.listener(|app, _, window, cx| {
                app.menu_dismissed_at.arm();
                app.close_workflow_menu(window, cx);
            }))
            .child(list);

        Some(crate::ui::popover::column_menu(
            popup,
            "anchored_popover-in-13",
            &theme,
        ))
    }

    /// Quiet inline hints immediately left of the context meter, including
    /// with an empty draft so running-send modes remain discoverable.
    fn composer_send_hints(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .id("composer-send-hints")
            .debug_selector(|| "composer-send-hints".to_string())
            .min_w_0()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .text_color(theme.text_muted)
            .children(
                composer_send::shortcut_hints(theme.ui.composer_send_mode, self.is_running())
                    .into_iter()
                    .map(|(keys, action)| {
                        // Match the adjacent footer controls without adding click behavior.
                        // Bespoke: inert — the send hints borrow a button frame only to align with the footer controls; nothing here is pressable.
                        button_frame(div(), &theme, ButtonSize::Default)
                            .flex_shrink()
                            .min_w_0()
                            .child(
                                div()
                                    .debug_selector(|| "composer-hint-keys".to_string())
                                    .flex_none()
                                    .child(kbd_shortcut(keys, &theme)),
                            )
                            .child(
                                div()
                                    .debug_selector(|| "composer-hint-label".to_string())
                                    .truncate()
                                    .child(action),
                            )
                    }),
            )
    }

    /// The composer's "+" button and its add menu (anchored above the
    /// button, same deferred pattern as the chip pickers). The menu carries
    /// the `AddMenu` key context, so ↑/↓/Enter/Escape drive it while open.
    pub(super) fn add_menu_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.add_menu_popup(cx))
            .child(
                IconButton::new("attach-chip", "icons/plus.svg")
                    .open(self.add_menu_open)
                    .tip(tr!("composer.add"))
                    .faded(&theme, |el| {
                        el.on_mouse_up(MouseButton::Left, cx.listener(Self::on_add_trigger_click))
                    }),
            )
    }

    /// The add menu popup, while open. Real actions only: attach an image,
    /// attach any file (by path at the caret), or start an @-mention.
    pub(super) fn add_menu_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.add_menu_open {
            return None;
        }
        let theme = theme::get(cx).for_floating();
        let this = cx.weak_entity();
        let mut list = div().w_full().flex().flex_col();
        for (ix, (icon_path, label, hint)) in ADD_MENU_ITEMS.iter().enumerate() {
            let highlighted = self.add_menu_highlight.selected(ADD_MENU_ITEMS.len()) == Some(ix);
            let this = this.clone();
            let entry = div().id(ElementId::NamedInteger("add-menu-row".into(), ix as u64));
            list = list.child(
                picker_entry(entry, &theme)
                    .h(picker::entry_height(&theme))
                    .flex_none()
                    .cursor_pointer()
                    .when(highlighted, |row| row.bg(theme.selected_wash()))
                    .hover_state(&theme)
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.add_menu_highlight.selected(ADD_MENU_ITEMS.len()) != Some(ix)
                                {
                                    app.add_menu_highlight.highlight(ix);
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this = cx.weak_entity();
                        move |_, window, cx| {
                            this.update(cx, |app, cx| app.run_add_menu_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .child(icon(
                        icon_path,
                        context_menu::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(if highlighted {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(tr!(*label)),
                    )
                    .when(!hint.is_empty(), |row| row.child(menu_hint(*hint, &theme))),
            );
        }

        let popup = picker_surface(div(), &theme)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            .overflow_hidden()
            // The menu owns the keyboard while open (`AddMenu` bindings in
            // main.rs sit deeper than the global escape/enter).
            .key_context("AddMenu")
            .track_focus(&self.add_menu_focus)
            .on_action(cx.listener(Self::on_add_menu_next))
            .on_action(cx.listener(Self::on_add_menu_prev))
            .on_action(cx.listener(Self::on_add_menu_confirm))
            .on_action(cx.listener(Self::on_add_menu_close))
            .dismiss_on_outside(cx.listener(|app, _, window, cx| {
                // Arm the click-through guard so the same click's mouse-up
                // on the "+" button doesn't re-open the menu.
                app.menu_dismissed_at.arm();
                app.close_add_menu(window, cx);
            }))
            .child(list);

        Some(crate::ui::popover::column_menu(
            popup,
            "anchored_popover-in-14",
            &theme,
        ))
    }

    /// The anchored popup for `kind`, when that picker is open. `deferred`
    /// paints it on top of everything; `anchored` takes it out of the layout
    /// and pins its bottom-left corner just above the chip, flipping at the
    /// window edges via `snap_to_window_with_margin`.
    pub(super) fn chip_popup(
        &self,
        kind: PickerKind,
        theme: &Theme,
    ) -> Option<impl IntoElement + use<>> {
        self.model_selector
            .clone()
            .and_then(|(open_kind, selector)| {
                (open_kind == kind).then(move || {
                    crate::ui::popover::column_menu(selector, "anchored_popover-in-15", theme)
                })
            })
    }

    /// The model chip: provider glyph + model name, then a caret only when no
    /// thinking chip follows. Ghost style — configuration is secondary to the
    /// prompt, so chips carry no border or fill until hovered/open. `compact`
    /// clamps the label so narrow windows keep Send reachable.
    pub(super) fn model_chip(&self, compact: bool, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        // A run in flight dims the chip and swallows clicks until it settles.
        let disabled = self.is_running();
        let open = !disabled && self.picker_is_open(PickerKind::Model);
        let chip = ChipButton::new("model-chip", ChipStyle::Quiet)
            // The thinking level is the pair's trailing half and carries the
            // one caret; without it the model chip ends the unit itself.
            .caret(!self.can_choose_thinking())
            .size(ButtonSize::Medium)
            .selector("model-chip")
            .shrink()
            .leading(icon_dyn(
                provider_icon(&self.model_provider),
                ButtonSize::Medium.icon_size().px(&theme),
                provider_mark_color(
                    &self.model_provider,
                    &theme,
                    if disabled {
                        theme.text_disabled
                    } else {
                        theme.text_muted
                    },
                ),
            ))
            .label(self.model_label.clone())
            .label_max_w(px(if compact { 120. } else { 220. }))
            .tip(self.model_label.clone())
            .disabled(disabled)
            .open(open)
            .on_mouse_up(&theme, cx.listener(Self::on_model_trigger_click));
        div()
            .min_w_0()
            .flex()
            .flex_col()
            .items_start()
            .children(self.chip_popup(PickerKind::Model, &theme))
            .child(chip)
    }

    /// The thinking-level chip: the reasoning level in muted ink + caret,
    /// flush after the model chip so the two read as one "Model Level ▾"
    /// setting. No level glyph — the word names the level, and a second
    /// icon beside the provider mark doubled the toolbar's iconography.
    ///
    /// Absent when the active model offers nothing to pick. pi reports `["off"]`
    /// for a model without reasoning, and an empty list before the levels have
    /// arrived, so a chip there would advertise a setting that cannot move. It
    /// appears with the first real ladder, and ⌘T plus the palette's Choose
    /// Thinking Level keep the picker reachable whenever it has options.
    pub(super) fn thinking_chip(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.can_choose_thinking() {
            return None;
        }
        let theme = *theme::get(cx);
        // A run in flight dims the chip and swallows clicks until it settles.
        let disabled = self.is_running();
        let open = !disabled && self.picker_is_open(PickerKind::Thinking);
        let chip = ChipButton::new("thinking-chip", ChipStyle::Quiet)
            .size(ButtonSize::Medium)
            .selector("thinking-chip")
            .subdued()
            .tip(commands::tooltip(CommandId::ChooseThinking, false))
            .label(thinking_display(&self.thinking_label))
            .disabled(disabled)
            .open(open)
            .on_mouse_up(&theme, cx.listener(Self::on_thinking_trigger_click));
        Some(
            div()
                .flex()
                .flex_col()
                .items_start()
                .children(self.chip_popup(PickerKind::Thinking, &theme))
                .child(chip)
                .into_any_element(),
        )
    }

    /// The new-task backdrop layer: the configured wallpaper, or the default
    /// dot grid, absolutely filling the whole chat column below the top bar —
    /// behind the empty state, the floating composer, and the status bar
    /// alike, so the surface is continuous behind all three.
    ///
    /// It lives here rather than inside the empty state because the empty
    /// state is only the `flex_1` slot above the composer: a backdrop scoped
    /// to it ended at the composer's top edge and left the bottom of the
    /// window on a flat slab. The active chat paints the quiet
    /// [`Self::chat_backdrop`] wash instead — the living field is the
    /// new-task page's own.
    pub(super) fn new_task_backdrop(
        &mut self,
        theme: Theme,
        phase: f32,
        empty: bool,
        reduce_motion: bool,
    ) -> Option<AnyElement> {
        if !empty {
            return None;
        }
        // A configured wallpaper owns this region. Full-window paints it at
        // the root (behind the sidebar too); hero-only paints exactly this
        // backdrop rect, below the shared top bar. With no wallpaper, the
        // procedural dot field is the page's only backdrop.
        let wallpaper = crate::wallpaper::settings();
        if wallpaper.enabled() {
            self.backdrop_live = false;
            if wallpaper.mode == crate::wallpaper::WallpaperMode::FullWindow {
                return None;
            }
            return Some(
                div()
                    .id("new-task-backdrop")
                    .debug_selector(|| "new-task-backdrop".to_string())
                    .absolute()
                    .top(px(TOP_BAR_H))
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .children(crate::wallpaper::layer(theme, reduce_motion))
                    .into_any_element(),
            );
        }
        // The procedural field animates; a wallpaper is a still, so the
        // heartbeat only runs on the no-wallpaper path. `render` clears the
        // flag before this runs, so a frame that skips the backdrop leaves it
        // false.
        self.backdrop_live = true;
        Some(
            div()
                .id("new-task-backdrop")
                .debug_selector(|| "new-task-backdrop".to_string())
                .absolute()
                .top(px(TOP_BAR_H))
                .left_0()
                .right_0()
                .bottom_0()
                .child(crate::backdrop::layer(theme, phase, !reduce_motion))
                .into_any_element(),
        )
    }

    /// The active chat's backdrop: a static, ambient wash where
    /// [`Self::new_task_backdrop`] leaves a flat slab — no field, no motion,
    /// nothing behind the words the eye could track. A whisper of the accent
    /// colour fades down from the top edge to lift the chrome; a barely
    /// denser neutral band grounds the page under the composer. Both stay in
    /// the low single digits of alpha — legible only as "not a flat slab",
    /// never as texture — and neither animates, so reading the transcript is
    /// exactly as quiet as before.
    pub(super) fn chat_backdrop(&self, theme: Theme, empty: bool) -> Option<AnyElement> {
        if empty {
            // The new-task page shows its own living backdrop.
            return None;
        }
        // Full-window wallpaper is the page's backdrop; the accent wash would
        // only add a second tint over it. Hero-only never reaches the chat
        // column, so the existing wash stays.
        if theme.wallpaper.full_window {
            return None;
        }
        Some(
            div()
                .id("chat-backdrop")
                .debug_selector(|| "chat-backdrop".to_string())
                .absolute()
                .top(px(TOP_BAR_H))
                .left_0()
                .right_0()
                .bottom_0()
                .child(
                    // Accent halo, strongest at the top edge and gone by the
                    // middle of the column.
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(relative(0.45))
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(theme.accent_text.opacity(0.05), 0.),
                            linear_color_stop(theme.accent_text.opacity(0.), 1.),
                        )),
                )
                .child(
                    // Neutral lift under the composer, so the bottom of the
                    // page reads grounded rather than ending on the slab.
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .h(relative(0.30))
                        .bg(linear_gradient(
                            180.,
                            linear_color_stop(theme.text_primary.opacity(0.), 0.),
                            linear_color_stop(theme.text_primary.opacity(0.02), 1.),
                        )),
                )
                .into_any_element(),
        )
    }

    /// The dot field at rest, for surfaces that show it as a thumbnail
    /// (Settings → Appearance's wallpaper preview) rather than the live page.
    pub(super) fn dot_backdrop(theme: Theme) -> impl IntoElement + use<> {
        crate::backdrop::layer(theme, 0., false)
    }

    /// Keep the composer's hairline steady on focus; file drags use the accent.
    pub(super) fn composer_border_color(theme: &Theme, file_drag_hovered: bool) -> Hsla {
        if file_drag_hovered {
            theme.accent
        } else {
            theme.hairline()
        }
    }

    /// Width of the chat column after every visible dock has taken its share.
    /// The workspace picker uses this same measurement instead of treating the
    /// window minus the sidebar as the available area, so Explorer and Review
    /// cannot push its fixed-width card away from the folder chip.
    pub(super) fn main_area_width(&self, window: &Window, cx: &App) -> Pixels {
        let viewport = window.viewport_size();
        let pane_open = self.sidepane.read(cx).is_open();
        // A remote session has no local worktree to review or browse.
        let pane_visible = pane_open
            && !self.settings_open
            && !self.usage_open
            && self.dependencies_ready()
            && self.remote.is_none();
        let sidebar_shown = sidebar_shows(self.sidebar_visible, self.settings_open, viewport.width);
        let explorer_visible = self.project_panel.read(cx).is_open()
            && !self.settings_open
            && !pane_open
            && self.remote.is_none();
        let explorer_width = if explorer_visible {
            self.project_panel.read(cx).width()
        } else {
            px(0.)
        };
        let pane_width = if pane_visible {
            self.sidepane.read(cx).width()
        } else {
            px(0.)
        };
        (viewport.width
            - if sidebar_shown {
                self.sidebar_width
            } else {
                px(0.)
            }
            - explorer_width
            - pane_width
            - self.session_details_width(cx))
        .max(px(0.))
    }

    /// Whether the session-details panel takes the right dock this frame. It
    /// is hidden while Settings owns the window, and gives way to Review or
    /// the Explorer if either is showing (opening them clears it anyway).
    pub(super) fn session_details_visible(&self, cx: &App) -> bool {
        self.session_details_open
            && self.session_has_details()
            && !self.settings_open
            && !self.sidepane.read(cx).is_open()
            && !self.project_panel.read(cx).is_open()
    }

    /// Whether there is a session to describe. The new-task page (no
    /// transcript yet) has no name, context or identifiers worth showing, so
    /// the chip and the panel stay hidden until the first message lands; a
    /// panel the user left open comes back with it.
    pub(super) fn session_has_details(&self) -> bool {
        !self.transcript.is_empty()
    }

    /// The width the session-details dock takes from the main area: its card
    /// plus the gutter beside the chat, or nothing while it is hidden.
    pub(super) fn session_details_width(&self, cx: &App) -> Pixels {
        if self.session_details_visible(cx) {
            px(SESSION_DETAILS_GUTTER + SESSION_DETAILS_W)
        } else {
            px(0.)
        }
    }

    /// The new-task composer's context header: folder and branch on the left,
    /// working directory on the right. Each control opens its existing picker.
    /// `settling` collapses the header as the session status bar takes over.
    pub(super) fn new_task_folder_row(
        &self,
        theme: Theme,
        settling: bool,
        settle_gen: u64,
        cx: &Context<Self>,
    ) -> AnyElement {
        let picker_open = self.workspace_picker.is_some();
        let cwd = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        // A remote task names its host, and the tooltip says exactly where
        // the agent runs; the local folder is not where this task works.
        let (folder_name, path_label) = match &self.remote {
            Some(host) => (host.name.clone(), host.location()),
            None => (
                sessions::workspace_label(&cwd),
                cwd.to_string_lossy().into_owned(),
            ),
        };
        let remote = self.remote.is_some();
        // The folder control is one of three peers in this header (folder,
        // branch, working directory), so it is the same shared button: one
        // height, padding, radius, type size and icon size for all three.
        let size = ButtonSize::Default;
        let row_h = size.height(&theme);
        let glyph = size.icon_size().px(&theme);
        let gap = DynamicSpacing::Base08.px(&theme);

        // Bespoke: one of the composer header's three peer controls (folder,
        // branch, working directory), sharing the header's own metrics; it
        // needs track_focus plus a key context and action the chip owner does
        // not expose.
        let row = button_frame(div().id("pick-folder"), &theme, size)
            .debug_selector(|| "new-task-folder".to_string())
            // It truncates inside its slot instead of pushing the branch and
            // working-directory controls off the row on a narrow window.
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .text_color(theme.text_primary)
            .when(picker_open, |row| row.bg(theme.pressed_wash()))
            .when(!picker_open, |row| row.hover_quiet(&theme))
            .cursor_pointer()
            .pressed_state(
                &theme,
                if picker_open {
                    PressedRamp::Selected
                } else {
                    PressedRamp::Wash
                },
            )
            .track_focus(&self.new_task_folder_focus)
            .tip(path_label)
            .key_context("NewTaskFolder")
            .on_action(
                cx.listener(|app, _: &crate::OpenWorkspacePicker, window, cx| {
                    app.toggle_workspace_picker(window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|app, _, window, cx| app.toggle_workspace_picker(window, cx)),
            )
            .child(if remote {
                icon("icons/server-stack.svg", glyph, theme.text_muted).into_any_element()
            } else {
                self.workspace_mark(&theme, glyph)
            })
            .child(
                div()
                    .debug_selector(|| "new-task-folder-name".to_string())
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(folder_name),
            )
            .child(Self::chip_caret(
                picker_open,
                theme.text_primary,
                "workspace-caret-turn",
                cx,
            ));

        // Folder and branch are adjacent; the working directory stays at the
        // trailing edge. All three sit inside the composer's own surface.
        let container = div()
            .debug_selector(|| "new-task-context-header".to_string())
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(gap)
            .px(DynamicSpacing::Base08.px(&theme))
            .py(DynamicSpacing::Base06.px(&theme))
            .border_b_1()
            .border_color(theme.hairline())
            .text_color(theme.text_secondary)
            .child(
                div()
                    .relative()
                    .min_w_0()
                    .max_w(px(220.))
                    .child(row)
                    .children(self.workspace_picker_popup(&theme)),
            )
            // Branch and working-directory controls describe the local
            // repository, so a remote task leaves them out.
            .children(if remote {
                // The host's own branch, read over ssh: read-only, since a
                // checkout there would change the server's files.
                self.remote_branch_chip(theme, true, cx)
            } else {
                self.branch_chip(theme, true, cx)
            })
            .child(div().flex_1().min_w_0())
            .children((!remote).then(|| self.work_in_chip(cx)).flatten());

        if !settling {
            return container.into_any_element();
        }
        container
            .overflow_hidden()
            .with_animation(
                ElementId::NamedInteger("new-task-folder-row".into(), settle_gen),
                Animation::new(Motion::Travel.duration(&theme)).with_easing(new_task::slide_easing),
                move |el, progress| {
                    let (height, ink) = new_task::folder_row(progress);
                    let header_h = row_h + DynamicSpacing::Base06.px(&theme) * 2. + px(1.);
                    el.h(px(f32::from(header_h) * height)).opacity(ink)
                },
            )
            .into_any_element()
    }

    /// The folder row's leading mark: the project's own logo when one was
    /// found, the folder glyph otherwise. One size for both, so the name's
    /// leading edge never shifts between projects.
    pub(super) fn workspace_mark(&self, theme: &Theme, size: Pixels) -> AnyElement {
        match &self.workspace_logo {
            Some(logo) => img(ImageSource::Image(logo.clone()))
                .size(size)
                .flex_none()
                .rounded(Radius::Small.px(theme))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => icon("icons/folder.svg", size, theme.text_secondary).into_any_element(),
        }
    }

    /// Whether every required runtime dependency is installed.
    pub(super) fn dependencies_ready(&self) -> bool {
        onboarding::all_required_installed(&self.deps)
    }

    /// Leave the setup page. Only reachable on request: the page also shows
    /// when something is missing, and then there is nothing to go back to.
    pub(super) fn close_setup(&mut self, cx: &mut Context<Self>) {
        self.setup_open = false;
        cx.notify();
    }

    /// Re-run the dependency probe and, if `pi` just became available, spawn
    /// the agent client. Runs the probe off the main thread and spins the
    /// setup page's Refresh button while it's in flight.
    pub(super) fn refresh_setup(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            return; // ignore double-clicks while a refresh is running
        }
        self.refreshing = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let (deps, host, pi_install) = cx
                .background_executor()
                .spawn(async {
                    // Short pause so the spinner reads as "working" rather than
                    // a flash before the probe returns.
                    std::thread::sleep(Duration::from_millis(250));
                    let deps = onboarding::check_dependencies();
                    let mut installation =
                        crate::pi_install::detect_from_bin(onboarding::locate("pi"));
                    installation.version = deps
                        .iter()
                        .find(|dep| dep.bin == "pi")
                        .and_then(|dep| dep.version.clone());
                    (deps, platform::host(), installation)
                })
                .await;
            let ready = onboarding::all_required_installed(&deps);
            let _ = this.update(cx, |app, cx| {
                app.deps = deps;
                app.host = host;
                app.pi_install = pi_install;
                app.refreshing = false;
                app.sync_terminal_active(cx);
                if ready && app.client.is_none() {
                    let workspace = app
                        .current_workspace
                        .clone()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_else(|| PathBuf::from("."));
                    match app.spawn_pi(&workspace, true) {
                        Ok(client) => {
                            app.client = Some(client);
                            app.mcp_stamp = app.mcp.fingerprint_for(Some(&workspace));
                            app.send(CommandBody::GetState, "get_state");
                            app.refresh_catalogs();
                            app.toast_success(tr!("view.connected"));
                        }
                        Err(err) => app.toast_error(tr!("runtime.pi_spawn_failed", error = err)),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Full-page setup screen shown when a required dependency is missing.
    pub(super) fn render_onboarding(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let missing = onboarding::missing_required_count(&self.deps);
        // Opened from Settings → About on a provisioned machine, the page is
        // a report rather than a checklist, so it says so.
        let asked_for = missing == 0;

        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .relative()
            .overflow_hidden()
            // Setup contains text throughout the page; the binary new-task
            // screen cannot provide a reliable contrast behind that text.
            .bg(theme.background)
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px(DynamicSpacing::Base24.px(&theme))
                    .pb(DynamicSpacing::Base40.px(&theme))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(560.))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .size(px(44.))
                                            .rounded_full()
                                            .bg(theme.accent.opacity(0.12))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(icon(
                                                "icons/spark.svg",
                                                IconSize::Custom(18. / 16.).px(&theme),
                                                theme.accent_text,
                                            )),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.ui_px(22.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_primary)
                                            .child(tr!("view.set_up_orbit")),
                                    )
                                    .child(
                                        div()
                                            .max_w(px(420.))
                                            .text_size(TextSize::Default.px(&theme))
                                            .text_color(theme.text_muted)
                                            .text_align(TextAlign::Center)
                                            .child(if asked_for {
                                                tr!("view.setup_complete_hint")
                                            } else {
                                                tr!("view.setup_missing_hint")
                                            }),
                                    ),
                            )
                            .child(div().flex().flex_col().gap(px(10.)).children(
                                self.deps.iter().enumerate().map(|(ix, dep)| {
                                    self.render_dependency_row(dep, ix, cx).into_any_element()
                                }),
                            ))
                            .child(self.render_host_facts(cx))
                            // An npm-managed pi cannot update in place; the
                            // official installer migrates it. The click is the
                            // confirmation before it removes the npm package.
                            .when(self.pi_install.kind.is_npm(), |col| {
                                col.child(self.render_pi_migration(cx))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(14.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(DynamicSpacing::Base06.px(&theme))
                                            .text_size(TextSize::Small.px(&theme))
                                            .text_color(theme.text_muted)
                                            .child(
                                                // Green once nothing is
                                                // missing: the page doubles
                                                // as a health report.
                                                div().size(px(8.)).rounded_full().bg(
                                                    if missing > 0 {
                                                        theme.error
                                                    } else {
                                                        theme.success
                                                    },
                                                ),
                                            )
                                            .child(if missing > 0 {
                                                tr!("setup.missing_count", count = missing)
                                            } else {
                                                tr!("setup.ready")
                                            }),
                                    )
                                    .child(
                                        // Medium, like the Done button beside it.
                                        Button::new("refresh-setup", ButtonVariant::Ghost)
                                            .leading(refresh_glyph(
                                            "refresh-spin",
                                            ButtonSize::Medium.icon_size().px(&theme),
                                            self.refreshing,
                                            theme.text_secondary,
                                            theme,
                                        ))
                                            .label(if self.refreshing {
                                                    tr!("common.checking")
                                                } else {
                                                    tr!("common.refresh")
                                                })
                                            .loading(self.refreshing)
                                            .on_click(&theme, {
                                            let this = cx.entity();
                                            move |_, _window, cx| {
                                                this.update(cx, |app, cx| {
                                                    app.refresh_setup(cx);
                                                });
                                            }
                                        }),
                                    )
                                    // Only offered when the page was opened on
                                    // request — with something missing, the
                                    // setup page is the app.
                                    .when(asked_for, |footer| {
                                        footer.child(self.runtime_button(
                                            "setup-done",
                                            &tr!("common.done"),
                                            true,
                                            theme,
                                            cx.entity(),
                                            OrbitApp::close_setup,
                                        ))
                                    }),
                            ),
                    ),
            )
    }

    /// The machine and the paths Orbit reads and writes, under the
    /// dependency list: the OS it is running on, pi's session store, and
    /// Orbit's own config directory.
    ///
    /// Nothing here is installable, so these are quiet rows rather than
    /// dependency cards — but they are what the install commands above have
    /// to work against, and the first thing to check when the app comes up
    /// with an empty sidebar.
    pub(super) fn render_host_facts(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let mut facts = onboarding::host_facts(&self.host);
        // The pi install rows follow the OS and store paths: how pi arrived,
        // where it resolves, and its version.
        facts.extend(onboarding::pi_install_facts(&self.pi_install));
        div()
            .w_full()
            // A panel that hugs its rows: the setup page centres its column, so
            // a shrinkable child here would be clipped instead of grown.
            .flex_none()
            .rounded(panel::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.hairline())
            .overflow_hidden()
            .children(facts.iter().enumerate().map(|(ix, fact)| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .px(px(14.))
                    .py(DynamicSpacing::Base08.px(&theme))
                    // Rows are separated by a hairline, so the group reads as
                    // one panel rather than three.
                    .when(ix > 0, |row| row.border_t_1().border_color(theme.hairline()))
                    .child(
                        div()
                            .w(px(88.))
                            .flex_none()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(fact.label.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(fact.value.clone()),
                    )
                    .children(fact.note.clone().map(|note| {
                        div()
                            .flex_none()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(if fact.alert {
                                theme.error
                            } else {
                                theme.text_muted
                            })
                            .child(note)
                    }))
            }))
    }

    /// The npm → official-install migration panel. Shown only when pi was
    /// installed with npm; the button is the explicit confirmation before the
    /// official installer removes the npm package. User data is untouched.
    pub(super) fn render_pi_migration(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let migrating = self.pi_migrating;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .px(px(14.))
            .py(DynamicSpacing::Base12.px(&theme))
            .rounded(panel::RADIUS.px(&theme))
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.hairline())
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(tr!("pi_update.migrate_title")),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("pi_update.migrate_note")),
            )
            .child(
                Button::new("pi-migrate", ButtonVariant::Ghost)
                    .label(if migrating {
                        tr!("pi_update.migrating")
                    } else {
                        tr!("pi_update.migrate")
                    })
                    .loading(migrating)
                    .on_click(&theme, {
                        let this = cx.entity();
                        move |_, _window, cx| {
                            this.update(cx, |app, cx| app.migrate_pi_install(cx));
                        }
                    }),
            )
    }

    /// One dependency row: status dot, name + detail, and either a version
    /// chip (installed) or the install command with a copy affordance.
    pub(super) fn render_dependency_row(
        &self,
        dep: &Dependency,
        ix: usize,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let status_color = if dep.installed {
            theme.success
        } else {
            theme.error
        };

        div()
            .w_full()
            .px(px(14.))
            .py(DynamicSpacing::Base12.px(&theme))
            .rounded(panel::RADIUS.px(&theme))
            .bg(theme.surface_elevated)
            .border_1()
            .border_color(theme.hairline())
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(div().size(px(9.)).rounded_full().bg(status_color))
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
                            .gap(DynamicSpacing::Base08.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Default.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(dep.name),
                            )
                            .child(
                                div()
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_muted)
                                    .child(if dep.required {
                                        tr!("setup.required")
                                    } else {
                                        tr!("setup.optional")
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!(dep.detail_key)),
                    ),
            )
            .child(if dep.installed {
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.success)
                    .child(icon(
                        "icons/check.svg",
                        IconSize::XSmall.px(&theme),
                        theme.success,
                    ))
                    .child(
                        dep.version
                            .clone()
                            .unwrap_or_else(|| tr!("setup.installed")),
                    )
                    .into_any_element()
            } else {
                let cmd = dep.install_hint;
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(
                        div()
                            .px(DynamicSpacing::Base08.px(&theme))
                            .py(DynamicSpacing::Base04.px(&theme))
                            .rounded(Radius::Small.px(&theme))
                            .bg(theme.code_bg)
                            .border_1()
                            .border_color(theme.hairline())
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.code_text)
                            .child(cmd),
                    )
                    .child(
                        IconButton::new(("copy", ix), "icons/copy.svg")
                            .size(ButtonSize::Default)
                            .ink(theme.text_secondary)
                            .tip(tr!("common.copy"))
                            .on_click(&theme, move |_, _window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(cmd.to_string()));
                            }),
                    )
                    .into_any_element()
            })
    }

    /// Status bar under the composer: workspace / branch on the left,
    /// sending hints followed by used-context percent + ring on the right.
    pub(super) fn status_bar(
        &self,
        workspace_label: &str,
        folder_row_owns_location: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        // Inside a repository the location control is "Work in" — it names the
        // main checkout (as `Local`), a worktree, and the ways to reach a new
        // place, including switching to any other folder. Outside a repository
        // there is nothing to choose, so the plain folder chip stays: it names
        // the workspace and the native dialog changes it.
        //
        // The new-task page is the one exception: the composer header names
        // the folder, and a second control below it would read as a different
        // setting. That ownership
        // lasts through the dock transition; this row takes over only after the
        // folder row has gone, so the two never appear together mid-settle.
        // A remote session runs on its host: the local folder, Work-in and
        // branch controls would name the wrong machine, so one static chip
        // names the host instead (its tooltip carries `ssh:dir`).
        let remote = self.remote.as_ref();
        let local_location = !folder_row_owns_location && remote.is_none();
        let remote_chip = remote.filter(|_| !folder_row_owns_location).map(|host| {
            status_item(
                "status-remote-host",
                "icons/server-stack.svg",
                tr!("remote.status_chip", host = host.name.clone()),
                host.location(),
                &theme,
            )
            .debug_selector(|| "status-remote-host".to_string())
        });
        let work_in = local_location.then(|| self.work_in_chip(cx)).flatten();
        let folder_chip = (work_in.is_none() && local_location).then(|| {
            status_button(
                "status-workspace",
                "icons/folder.svg",
                workspace_label.to_string(),
                &theme,
            )
            .debug_selector(|| "status-workspace".to_string())
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pick_folder_click))
        });
        StatusBar::new()
            .protect(
                folder_row_owns_location && theme.wallpaper.active && !theme.wallpaper.full_window,
            )
            .leading(remote_chip)
            .leading(
                remote
                    .filter(|_| !folder_row_owns_location)
                    .and_then(|_| self.remote_branch_chip(theme, false, cx)),
            )
            .leading(folder_chip)
            .leading(work_in)
            // The new-task header owns the branch until the dock completes;
            // the status row then takes it back alongside the Work-in chip.
            .leading(
                local_location
                    .then(|| self.branch_chip(theme, false, cx))
                    .flatten(),
            )
            // A quiet MCP indicator: only when servers are configured, and
            // only a dot + count — clicking opens Settings → MCP. Color is
            // never the only signal (the label names the state).
            .leading(self.mcp_status_chip(theme, cx))
            // Transient status (send failures, attachment limits, branch
            // results): fresh messages only — the tick lets them lapse.
            .trailing(
                self.status_at
                    .is_some_and(|at| at.elapsed() < STATUS_MESSAGE_TTL)
                    .then(|| status_message(ext_widgets::strip_ansi(&self.status, &theme), &theme)),
            )
            // While a run is in flight the hints carry the one thing that is
            // not obvious from the composer: which chord steers the live turn
            // and which queues a follow-up. Idle they only restate the send
            // button a few pixels above, so they stay out of the chrome until
            // there is something to teach.
            .trailing(self.is_running().then(|| self.composer_send_hints(cx)))
            // A fresh task has no context to measure. Leave that space quiet
            // until pi supplies real usage instead of filling the footer with a
            // decorative zero; an active session still gets the neutral waiting
            // state while its first snapshot is pending. pi reports the window
            // with zero tokens on a new session, so `Some` alone is not enough.
            .trailing(
                (!folder_row_owns_location
                    || crate::context_meter::has_usage(self.context.as_ref()))
                .then(|| {
                    div()
                        .debug_selector(|| "composer-context-indicator".to_string())
                        .flex_none()
                        .child(self.context_button(cx))
                }),
            )
            .render(&theme)
    }

    /// The branch control: the branch mark, name, ahead/behind counts and the
    /// shared picker caret, opening the branch picker. One implementation serves
    /// the status row in a session and the new-task composer header, so the
    /// two cannot drift.
    ///
    /// `opens_below` places the menu under the new-task header's chip; the
    /// status row sits near the bottom of the window and keeps its menu above.
    pub(super) fn branch_chip(
        &self,
        theme: Theme,
        opens_below: bool,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let branch = self.branch.as_ref()?;
        let open = self.branch_picker.is_some();
        let pending = self.branch_operation_pending;
        let face = Self::branch_chip_face(
            "branch-chip",
            branch.name.clone(),
            branch.ahead_behind,
            open,
            pending,
            None,
            theme,
            cx,
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|app, _, window, cx| {
                if !app.branch_operation_pending {
                    app.toggle_branch_picker(window, cx);
                }
            }),
        );
        let popup = if opens_below {
            self.branch_picker_popup_below(&theme)
        } else {
            self.branch_picker_popup(&theme)
        };
        Some(Self::branch_chip_shell(face, popup))
    }

    /// The branch chip's slot: it yields before the row can crowd, so a long
    /// ref truncates in the name instead of pushing the folder chip or the
    /// status readout off the line. `popup` is the picker hanging off it.
    pub(super) fn branch_chip_shell(
        face: gpui::Stateful<gpui::Div>,
        popup: Option<AnyElement>,
    ) -> AnyElement {
        div()
            .min_w_0()
            .max_w(px(220.))
            .child(
                div()
                    .relative()
                    .min_w_0()
                    .max_w_full()
                    .child(face)
                    .children(popup),
            )
            .into_any_element()
    }

    /// One look for every branch chip — this Mac's and a remote host's — so
    /// the two never drift: the branch glyph, the name (truncated, the whole
    /// ref on hover), ahead/behind when there is something to push or pull,
    /// and the caret. The caller attaches the press. `tip` names the control
    /// when it is not the local picker (the remote chip says whose branch).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn branch_chip_face(
        id: &'static str,
        name: String,
        ahead_behind: Option<(usize, usize)>,
        open: bool,
        pending: bool,
        tip: Option<String>,
        theme: Theme,
        cx: &Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // Bespoke: a trigger with a live pending state (dimmed, inert while a branch
        // operation runs) and ahead/behind counts; ChipButton has no pending state yet.
        button_frame(div().id(id), &theme, ButtonSize::Default)
            .min_w_0()
            .max_w_full()
            .debug_selector(move || id.to_string())
            // The status row and composer header share the
            // same compact type size.
            .text_size(TextSize::Small.px(&theme))
            .when(!pending, |chip| chip.cursor_pointer())
            .when(open, |chip| {
                chip.bg(theme.pressed_wash()).text_color(theme.text_primary)
            })
            .when(!open && !pending, |chip| chip.hover_quiet(&theme))
            .when(pending, |chip| chip.opacity(0.6))
            .when_some(tip, |chip, tip| chip.tip(tip))
            .child(icon(
                "icons/branch.svg",
                ButtonSize::Default.icon_size().px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    // A long branch truncates inside the
                    // chip; the tooltip keeps the whole ref
                    // inspectable without widening the row.
                    .min_w_0()
                    .max_w(px(220.))
                    .truncate()
                    .tip(name.clone())
                    .child(name),
            )
            .children(
                ahead_behind
                    // `↑0 ↓0` is a row of arrows that says
                    // nothing; the counts appear when there
                    // is something to push or pull.
                    .filter(|(ahead, behind)| *ahead > 0 || *behind > 0)
                    .map(|(ahead, behind)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(format!("↑{ahead}"))
                            .child(format!("↓{behind}"))
                    }),
            )
            .child(Self::chip_caret(
                open,
                theme.text_primary,
                "branch-caret-turn",
                cx,
            ))
    }

    pub(super) fn context_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let entity = cx.entity();
        let theme = *theme::get(cx);
        // Manual compaction is offered only while pi is connected: without a
        // session process there is nothing to compact.
        let on_compact = self.client.is_some().then(|| {
            Rc::new(
                |app: &mut OrbitApp, _window: &mut Window, cx: &mut Context<OrbitApp>| {
                    app.compact_now(cx);
                },
            ) as Rc<dyn Fn(&mut OrbitApp, &mut Window, &mut Context<OrbitApp>)>
        });
        context_meter::context_control(
            ContextMeterData {
                usage: self.context.as_ref(),
                session: self.session_usage.as_ref(),
                conversation_est: self.transcript.estimated_tokens(),
            },
            self.context_popup,
            &entity,
            theme,
            |app, hovered, cx| {
                if app.context_popup == ContextPopup::Details {
                    return;
                }
                app.context_popup = if hovered {
                    ContextPopup::Hover
                } else {
                    ContextPopup::None
                };
                cx.notify();
            },
            |app, _, cx| {
                if app.menu_dismissed_at.swallows() {
                    return;
                }
                app.context_popup = if app.context_popup == ContextPopup::Details {
                    ContextPopup::None
                } else {
                    ContextPopup::Details
                };
                if app.context_popup == ContextPopup::Details {
                    app.refresh_context_stats();
                }
                cx.notify();
            },
            self.is_compacting,
            on_compact,
            |app, _, cx| {
                app.menu_dismissed_at.arm();
                app.context_popup = ContextPopup::None;
                cx.notify();
            },
        )
    }

    /// The window's left titlebar controls: sidebar toggle + session history.
    /// They sit beside the macOS traffic lights in the sidebar's drag strip
    /// when the sessions sidebar is open, and fall back to the main top bar
    /// (clearing the lights) when it is hidden. Windows has no OS buttons on
    /// that side, so the cluster stays left-aligned there either way.
    pub(super) fn titlebar_left_controls(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let back_enabled = self.history_index > 0;
        let forward_enabled = self.history_index + 1 < self.session_history.len();
        // The three controls wear the same glass chips as the right cluster
        // ([`header_icon_button`]), so the whole 44px bar reads as one row:
        // a fixed square hit box with a centered Medium icon keeps the toggle
        // and the chevrons optically even (a bare `p_1` lets the toggle's
        // pill drift wider than the chevrons).
        div()
            .flex()
            .items_center()
            .gap(px(TITLEBAR_CONTROLS_GAP))
            .pr(DynamicSpacing::Base06.px(&theme))
            .child(
                header_ghost_button(
                    "toggle-sidebar",
                    &theme,
                    icon(
                        "icons/sidebar-toggle.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        theme.text_secondary,
                    ),
                )
                .block_mouse_except_scroll()
                .tooltip({
                    let label = commands::tooltip(CommandId::ToggleSidebar, self.sidebar_visible);
                    move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                })
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_toggle_sidebar)),
            )
            .child({
                let button = div()
                    .id("history-back")
                    .block_mouse_except_scroll()
                    .size(px(HEADER_CTRL_H))
                    .rounded(header_ctrl_radius(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(back_enabled, |b| {
                        b.cursor_pointer()
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_history_back))
                    })
                    .tooltip({
                        let label = tr!("view.back");
                        move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                    })
                    .child(icon(
                        "icons/arrow-left.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        if back_enabled {
                            theme.text_secondary
                        } else {
                            theme.text_muted
                        },
                    ));
                if back_enabled {
                    crate::motion::hover(button, "history-back", &theme, Hover::wash(&theme))
                } else {
                    button.into_any_element()
                }
            })
            .child({
                let button = div()
                    .id("history-forward")
                    .block_mouse_except_scroll()
                    .size(px(HEADER_CTRL_H))
                    .rounded(header_ctrl_radius(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(forward_enabled, |b| {
                        b.cursor_pointer()
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_history_forward))
                    })
                    .tooltip({
                        let label = tr!("view.forward");
                        move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into()
                    })
                    .child(icon(
                        "icons/arrow-right.svg",
                        ButtonSize::Medium.icon_size().px(&theme),
                        if forward_enabled {
                            theme.text_secondary
                        } else {
                            theme.text_muted
                        },
                    ));
                if forward_enabled {
                    crate::motion::hover(button, "history-forward", &theme, Hover::wash(&theme))
                } else {
                    button.into_any_element()
                }
            })
    }

    /// The right dock's top-bar affordance: the working tree's `+N −N` change
    /// counts when the tree is dirty, and the side-panel toggle otherwise. The
    /// counts open the Git page's **Changes** tab (the diff they name); the
    /// plain glyph toggles Review. Either state stays lifted while its target
    /// is visible.
    pub(super) fn render_review_panel_control(
        &self,
        theme: Theme,
        pane_visible: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        // The counts stay off while the pane or the Git page already shows the
        // same diff, matching the standalone chip's old visibility rule.
        let dirty = (self.added > 0 || self.removed > 0) && !pane_visible && !self.git_open;
        let chip = press(header_chip(
            div()
                .id("toggle-side-pane")
                .group(BUTTON_GROUP)
                .h(px(HEADER_CTRL_H))
                .min_w(px(HEADER_CTRL_H))
                .px(if dirty {
                    ButtonSize::Medium.padding_x(&theme)
                } else {
                    px(0.)
                })
                .rounded(header_ctrl_radius(&theme))
                .flex()
                .items_center()
                .justify_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .cursor_pointer(),
            &theme,
        ))
        // One or the other, never both: the counts stand in for the glyph
        // while the tree is dirty, so the chip cannot grow two things wide.
        .children((!dirty).then(|| {
            icon(
                "icons/sidebar-toggle-right.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                if pane_visible {
                    theme.text_primary
                } else {
                    theme.text_secondary
                },
            )
        }))
        .children(dirty.then(|| {
            div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.success)
                        .child(format!("+{}", self.added)),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.error)
                        .child(format!("-{}", self.removed)),
                )
        }));
        let chip = if pane_visible {
            header_lift(chip, &theme)
        } else {
            chip
        };
        // The dirty chip is the Git page's Changes tab; the clean chip is the
        // Review toggle. Label the control by what the current glyph does.
        let tooltip = if dirty {
            commands::tooltip(CommandId::OpenGit, false)
        } else {
            commands::tooltip(CommandId::ToggleSidePanel, pane_visible)
        };
        chip.tip(tooltip).on_mouse_up(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                if dirty {
                    this.open_git_changes(cx);
                } else {
                    this.on_toggle_side_pane(event, window, cx);
                }
            }),
        )
    }

    /// The top bar's terminal toggle (⌘J): one chip, lifting while the bottom
    /// panel is open. It sits directly after the Review chip because the two
    /// panels share the window — each is one click, never a menu away.
    pub(super) fn render_terminal_control(
        &self,
        theme: Theme,
        terminal_visible: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        header_icon_button(
            "header-terminal",
            &theme,
            terminal_visible,
            icon(
                "icons/terminal.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                if terminal_visible {
                    theme.text_primary
                } else {
                    theme.text_secondary
                },
            ),
        )
        .tip(commands::tooltip(
            CommandId::ToggleTerminal,
            terminal_visible,
        ))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _: &MouseUpEvent, window, cx| {
                this.on_toggle_terminal(&crate::ToggleTerminal, window, cx);
            }),
        )
    }

    /// The top bar's session-details chip: toggles the details panel in the
    /// right dock, and stays lifted while that panel is showing.
    pub(super) fn render_session_details_control(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let open = self.session_details_open;
        div().relative().child(
            header_icon_button(
                "header-session-details",
                &theme,
                open,
                icon(
                    "icons/info.svg",
                    ButtonSize::Medium.icon_size().px(&theme),
                    if open {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    },
                ),
            )
            .tip(tr!("session.session_details"))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(Self::on_session_details_click),
            ),
        )
    }

    /// The top bar's Explorer toggle (⌘⇧E): its own chip beside the
    /// terminal's, lifting while the project panel is showing. It used to sit
    /// alone in a "…" menu, one click further away than its neighbours.
    pub(super) fn render_explorer_control(
        &self,
        theme: Theme,
        explorer_visible: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        header_icon_button(
            "header-explorer",
            &theme,
            explorer_visible,
            icon(
                "icons/folder-panel.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                if explorer_visible {
                    theme.text_primary
                } else {
                    theme.text_secondary
                },
            ),
        )
        .debug_selector(|| "header-explorer".into())
        .tip(commands::tooltip(
            CommandId::ToggleProjectPanel,
            explorer_visible,
        ))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _: &MouseUpEvent, _, cx| this.toggle_project_panel(cx)),
        )
    }

    /// The sidebar's New Task action, with its shortcut in the tooltip.
    pub(super) fn sidebar_new_task_button(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        IconButton::new("sidebar-new-session", "icons/task-edit-01.svg")
            .size(ButtonSize::Large)
            .ink(theme.text_primary)
            .tip(crate::commands::tooltip(
                crate::commands::CommandId::NewSession,
                false,
            ))
            .on_mouse_up(
                &theme,
                cx.listener(|this, _: &MouseUpEvent, w, cx| {
                    this.on_new_session(&crate::NewSession, w, cx)
                }),
            )
            // The sidebar's primary action rests on a filled face.
            .bg(theme.overlay)
    }

    /// The sidebar footer's utility icons: Git, Usage and Settings. Icon-only —
    /// each glyph is labelled by its tooltip, which
    /// also names its chord — so the column keeps labels for the primary
    /// action and the session list alone.
    pub(super) fn sidebar_footer_actions(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        // Git — opens (or leaves) the Git page (⌘⇧G). Lives in the footer,
        // ahead of Usage, because it is a workbench destination, not a
        // readout; the top bar keeps it in the "…" overflow.
        let git = navigation_icon_button(
            div().id("sidebar-git"),
            &theme,
            ButtonSize::Medium,
            self.git_open,
        )
        .tip(commands::tooltip(CommandId::OpenGit, false))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_git_nav_click))
        .child(icon(
            "icons/sidebar-git.svg",
            ButtonSize::Medium.icon_size().px(&theme),
            if self.git_open {
                theme.text_primary
            } else {
                theme.text_secondary
            },
        ));
        // Usage — opens (or leaves) the Usage page (⌘U).
        let usage = navigation_icon_button(
            div().id("sidebar-usage"),
            &theme,
            ButtonSize::Medium,
            self.usage_open,
        )
        .tip(commands::tooltip(CommandId::ToggleUsage, false))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_usage_nav_click))
        .child(icon(
            "icons/chart-analysis.svg",
            ButtonSize::Medium.icon_size().px(&theme),
            if self.usage_open {
                theme.text_primary
            } else {
                theme.text_secondary
            },
        ));
        // Settings — toggles the Settings surface (⌘,).
        let settings = navigation_icon_button(
            div().id("settings"),
            &theme,
            ButtonSize::Medium,
            self.settings_open,
        )
        .tip(commands::tooltip(CommandId::OpenSettings, false))
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_settings_gear_click))
        .child(icon(
            "icons/settings.svg",
            ButtonSize::Medium.icon_size().px(&theme),
            if self.settings_open {
                theme.text_primary
            } else {
                theme.text_secondary
            },
        ));
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(git)
            .child(usage)
            .child(settings)
    }

    /// ⌘U: open the Usage page, or leave it if it is already open.
    pub(super) fn on_toggle_usage(
        &mut self,
        _: &crate::ToggleUsage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.usage_open {
            self.close_usage(cx);
        } else {
            self.open_usage(cx);
        }
    }

    /// The command palette's Choose Model (⌘⇧M): open the model popover.
    pub(super) fn on_toggle_model_menu(
        &mut self,
        _: &crate::ToggleModelMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_picker(PickerKind::Model, window, cx);
    }

    /// The command palette's Choose Thinking Level (⌘⇧T).
    pub(super) fn on_toggle_thinking_menu(
        &mut self,
        _: &crate::ToggleThinkingMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.toggle_picker(PickerKind::Thinking, window, cx);
    }

    /// ⌘⇧R: open the Review pane on the current diff. The pane owns the
    /// reveal/toggle rules; the app only asks for it.
    pub(super) fn on_review_changes(
        &mut self,
        _: &crate::ReviewChanges,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_review(cx);
    }

    /// ⌘/: open Settings → Shortcuts — the full keyboard reference.
    pub(super) fn on_open_shortcut_help(
        &mut self,
        _: &crate::OpenShortcutHelp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_open = true;
        self.set_settings_section(SettingsSection::Shortcuts, cx);
        self.sync_terminal_active(cx);
    }

    /// Escape from the Review tree: leave keyboard navigation and hand focus
    /// back to the composer, matching the sidebar's Escape behavior.
    pub(super) fn on_review_close(
        &mut self,
        _: &crate::ReviewClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// Tab: step to the next workbench surface (composer → sidebar → review
    /// tree → terminal → Explorer → back).
    pub(super) fn on_focus_next(
        &mut self,
        _: &crate::FocusNext,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_focus_step(false, window, cx) {
            return;
        }
        if self.focus_traversal_blocked() {
            return;
        }
        window.focus_next();
        cx.notify();
    }

    /// Shift-Tab: step to the previous workbench surface.
    pub(super) fn on_focus_prev(
        &mut self,
        _: &crate::FocusPrev,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal_focus_step(true, window, cx) {
            return;
        }
        if self.focus_traversal_blocked() {
            return;
        }
        window.focus_prev();
        cx.notify();
    }

    /// The open form modal's fields, in the order they are painted — its Tab
    /// ring. `None` when no such modal is open. Only fields that are on screen
    /// are listed: focusing a hidden one would drop the keyboard.
    pub(super) fn modal_fields(&self) -> Option<Vec<Entity<crate::composer::ComposerInput>>> {
        if let Some(prompt) = &self.remote_password_prompt {
            return Some(vec![prompt.input.clone()]);
        }
        if let Some(dialog) = &self.server_clone {
            return Some(vec![dialog.folder.clone()]);
        }
        // The rest are painted by the Settings page, so they own the keyboard
        // only while it is open.
        if !self.settings_open {
            return None;
        }
        if let Some(editor) = &self.remote_key_editor {
            return Some(vec![editor.input.clone()]);
        }
        if let Some(editor) = &self.account_editor {
            return Some(vec![editor.label.clone()]);
        }
        if let Some(editor) = &self.remote_host_editor {
            return Some(vec![
                editor.ssh.clone(),
                editor.user.clone(),
                editor.port.clone(),
                editor.dir.clone(),
                editor.name.clone(),
            ]);
        }
        if let Some(editor) = &self.mcp_editor {
            let mut fields = Vec::new();
            if editor.original.is_none() {
                fields.push(editor.name.clone());
            }
            fields.push(editor.description.clone());
            match editor.kind {
                crate::mcp::McpTransportKind::Stdio => fields.extend([
                    editor.command.clone(),
                    editor.args.clone(),
                    editor.env.clone(),
                ]),
                crate::mcp::McpTransportKind::StreamableHttp => {
                    fields.extend([editor.url.clone(), editor.headers.clone()]);
                    if editor.oauth_open {
                        fields.extend([
                            editor.oauth_client_id.clone(),
                            editor.oauth_client_secret.clone(),
                            editor.oauth_callback_port.clone(),
                            editor.oauth_callback_url.clone(),
                            editor.oauth_scope.clone(),
                        ]);
                    }
                }
            }
            return Some(fields);
        }
        None
    }

    /// Tab / Shift-Tab inside an open form modal: step around its own fields
    /// so focus cannot wander into the workbench behind the scrim. Returns
    /// whether a modal owned the key. A field mid-composition keeps the key
    /// (Tab can page IME candidates).
    pub(super) fn modal_focus_step(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(fields) = self.modal_fields() else {
            return false;
        };
        let composing = fields.iter().any(|field| {
            let field = field.read(cx);
            field.is_composing() && field.focus_handle(cx).is_focused(window)
        });
        if !composing {
            // The fields, then the footer: Cancel, then the primary action.
            let mut stops: Vec<FocusHandle> = fields
                .iter()
                .map(|field| field.read(cx).focus_handle(cx))
                .collect();
            stops.push(self.modal_cancel_focus.clone());
            stops.push(self.modal_confirm_focus.clone());
            crate::ui::dialog::focus_ring(&stops, backwards, window);
            cx.notify();
        }
        true
    }

    /// A keyboard-modal surface owns the input while it is open; Tab must not
    /// walk the focus out from under it.
    fn focus_traversal_blocked(&self) -> bool {
        self.command_palette.is_some()
            || self.model_selector.is_some()
            || self.branch_picker.is_some()
            || self.workspace_picker.is_some()
            || self.rewind_picker.is_some()
            || self.dialog.is_some()
            || self.lightbox.is_some()
            || self.updater_dialog.is_some()
            || self.session_menu.is_some()
            || self.workspace_menu.is_some()
            || self.sidebar_sort_menu
            || self.settings_select.is_some()
    }

    /// The composer's send area: one primary control whose glyph, weight, and
    /// tooltip state what ↵ will do right now, plus a quiet stop while a run is
    /// in flight.
    ///
    /// Sending mid-run queues or steers per the configured send mode, so the
    /// primary slot wears that mode's glyph instead of always showing a paper
    /// plane. Stop is always the *secondary* slot: stopping is a normal action,
    /// not a warning, so it rests neutral and only turns red under the pointer.
    pub(super) fn send_controls(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let theme = *theme::get(cx);
        let running = self.is_running();
        // The same rule `submit_as` uses: text, images, or a collapsed paste
        // all make a sendable message.
        let has_payload = self.send_has_payload(cx);
        let mut row = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme));
        // Queued messages and the control that queued them agree at a glance.
        if let Some(badge) = self.queue_badge(theme) {
            row = row.child(badge);
        }
        if !running {
            return row
                .child(self.send_button(has_payload, false, theme, window, cx))
                .into_any_element();
        }
        // Mid-run with nothing typed, stop is the only action and takes the
        // primary seat rather than sitting beside a dead send. With something
        // typed, both lanes are on screen (#131): the quiet alternate first,
        // then the default lane in the primary seat, so a one-off steer (or a
        // one-off queue) never needs the Enter preference flipped.
        if has_payload {
            row = row
                .child(self.alternate_send_button(theme, cx))
                .child(self.send_button(true, true, theme, window, cx));
        }
        row.child(self.stop_button(theme, cx)).into_any_element()
    }

    /// Whether the composer holds anything a send would deliver: typed text,
    /// an image, a file, or a collapsed paste. The one rule the control's
    /// enabled state reads, so it can never disagree with `submit_as`.
    pub(super) fn send_has_payload(&self, cx: &Context<Self>) -> bool {
        !self.input.read(cx).text().trim().is_empty() || self.attachment_slots_used() > 0
    }

    /// How many messages are waiting behind the live turn — a quiet count of
    /// small width beside the send control, so a queued follow-up is visible
    /// without moving the eye up to the queue bar.
    fn queue_badge(&self, theme: Theme) -> Option<AnyElement> {
        let count = self.queue.len();
        if count == 0 {
            return None;
        }
        Some(
            crate::ui::chip::Pill::new(count.to_string(), theme.text_muted)
                .leading(icon(
                    "icons/clock.svg",
                    IconSize::XSmall.px(&theme),
                    theme.text_muted,
                ))
                .face(theme.surface_elevated, ButtonSize::Medium)
                .render(&theme)
                .id("queue-badge")
                .debug_selector(|| "queue-badge".to_string())
                .tip(tr!("view.queued_after_task", count = count))
                .into_any_element(),
        )
    }

    /// The primary send control. `running` selects the glyph that matches
    /// what a send actually does (queue a follow-up / steer the live turn),
    /// and `has_payload` drives the resting state: a disabled control when
    /// there is nothing to send, so the button never accepts a click that
    /// silently does nothing.
    pub(super) fn send_button(
        &self,
        has_payload: bool,
        running: bool,
        theme: Theme,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mode = theme.ui.composer_send_mode;
        let (icon_path, action, label) = match (running, mode) {
            (true, SendMode::FollowUp) => (
                "icons/arrow-up.svg",
                tr!("composer.queue_follow_up"),
                tr!("composer.queue"),
            ),
            (true, SendMode::Steer) => (
                "icons/arrow-up-right.svg",
                tr!("composer.steer_task"),
                tr!("composer.steer"),
            ),
            (false, _) => ("icons/send.svg", tr!("composer.send"), tr!("composer.send")),
        };
        if !has_payload {
            // Resting, unclickable: a darker well and a disabled glyph, with no
            // pointer, hover, or click handler to invite a press that cannot
            // land. The tooltip says why instead of naming a shortcut that
            // would do nothing.
            let tooltip = tr!("composer.nothing_to_send");
            // Bespoke: the composer's round send/stop controls morph, fill with
            // the action role and own their press ramps.
            return icon_button_frame(
                div()
                    .id("send-btn")
                    .debug_selector(|| "send-btn".to_string()),
                &theme,
                ButtonSize::Medium,
            )
            .rounded_full()
            .bg(theme.overlay)
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
            .child(icon(
                icon_path,
                ButtonSize::Medium.icon_size().px(&theme),
                theme.text_disabled,
            ))
            .into_any_element();
        }
        let shortcut = if running {
            mode.shortcut_label(mode)
        } else {
            platform::shortcuts::label(platform::shortcuts::SEND)
        };
        let tooltip = tr!(
            "composer.shortcut_hint",
            keys = shortcut,
            action = action.clone()
        );
        let square = ButtonSize::Medium.height(&theme);
        let icon_size = ButtonSize::Medium.icon_size().px(&theme);
        let label_size = button::label_size(ButtonSize::Medium).px(&theme);
        let padding_x = ButtonSize::Medium.padding_x(&theme);
        let gap = button::gap(&theme);
        // The labelled pill is exactly its content plus the padding every
        // labelled button uses, measured for this label at this UI size. A
        // fixed reserve used to stand in for the label, so the padding shrank
        // as the UI font grew (and a longer translation would clip), while the
        // icon and the label sat flush with no gap between them.
        let label_width = text_width(window, &label, label_size);
        let labelled = (padding_x * 2. + icon_size + gap + label_width).max(square);
        // One ink for the whole control: the glyph and the label both read
        // `action_fg`, the foreground role of the fill they sit on. The label
        // used to inherit the page ink, so in dark mode it stayed near-white
        // while the fill was light — invisible on the button, however readable
        // it was on the canvas.
        let label_color = theme.action_fg;
        // Bespoke: the composer's round send/stop controls morph, fill with
        // the action role and own their press ramps.
        let control = icon_button_frame(
            div()
                .id("send-btn")
                .debug_selector(|| "send-btn".to_string()),
            &theme,
            ButtonSize::Medium,
        )
        .rounded_full()
        .raised(theme.action, &theme)
        .pressed_state(&theme, PressedRamp::Action)
        .cursor_pointer()
        .overflow_hidden()
        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_send_click))
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into());
        // One wrapper owns both values: the hover fades `action` into
        // `action_hover`, and the state morphs the square into a labelled
        // button on the longer Travel role. Width and label ride the same
        // transition, so the control never changes shape in two steps.
        crate::motion::control(
            control,
            "send-btn",
            &theme,
            crate::theme::tokens::Motion::Interaction,
            crate::theme::tokens::Motion::Travel,
            // Idle with something to send: the button names the action. While a
            // turn runs it folds back to a square, because the glyph (queue or
            // steer) is the whole message then.
            if running { 0.0 } else { 1.0 },
            move |control, hover, morphed| {
                control
                    .bg(crate::motion::blend(
                        theme.action,
                        theme.action_hover,
                        hover,
                    ))
                    .w(square + (labelled - square) * morphed)
                    .child(
                        icon(icon_path, icon_size, label_color)
                            .debug_selector(|| "send-btn-icon".to_string()),
                    )
                    // The label owns its share of the width, so an icon-only
                    // morph collapses it to nothing: a hidden label that kept
                    // its space pushed the glyph off the circle's centre.
                    .child(
                        div()
                            .debug_selector(|| "send-btn-label".to_string())
                            .flex_none()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(label_size)
                            .line_height(relative(1.))
                            .text_color(label_color)
                            .ml(gap * morphed)
                            .w(label_width * morphed)
                            .child(label.clone())
                            .opacity(morphed),
                    )
            },
        )
    }

    /// The lane the Enter preference does *not* pick, as a quiet round button
    /// beside the primary send: steer when Enter queues, queue when Enter
    /// steers. Same action as the alternate chord, which its tooltip names.
    fn alternate_send_button(&self, theme: Theme, cx: &Context<Self>) -> AnyElement {
        let mode = theme.ui.composer_send_mode.opposite();
        let glyph = match mode {
            SendMode::FollowUp => "icons/arrow-up.svg",
            SendMode::Steer => "icons/arrow-up-right.svg",
        };
        let tooltip = tr!(
            "composer.shortcut_hint",
            keys = mode.shortcut_label(theme.ui.composer_send_mode),
            action = mode.label()
        );
        // Bespoke: the composer's round send/stop controls morph, fill with
        // the action role and own their press ramps.
        let button = icon_button_frame(
            div()
                .id("send-alternate-btn")
                .debug_selector(|| "send-alternate-btn".to_string()),
            &theme,
            ButtonSize::Medium,
        )
        .rounded_full()
        .border_1()
        .border_color(theme.hairline())
        // `overlay`, not an opaque raised face: the composer is glass over a
        // wallpaper, and this disc is a control floating on it.
        .bg(theme.overlay)
        .pressed_state(&theme, PressedRamp::Wash)
        .cursor_pointer()
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(Self::on_send_alternate_click),
        )
        .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
        .child(icon(
            glyph,
            ButtonSize::Medium.icon_size().px(&theme),
            theme.text_secondary,
        ));
        crate::motion::hover(
            button,
            "send-alternate-btn",
            &theme,
            crate::motion::Hover::fill(theme.overlay, theme.hover_wash())
                .with_ink(theme.text_secondary, theme.text_primary),
        )
    }

    /// Stop the run. Neutral at rest — stopping is routine, not a failure — and
    /// only takes the error color under the pointer, so it is unmissable when
    /// actually aimed at and never the loudest thing on a streaming page.
    ///
    /// While a stop is out and pi has not answered, the control says so: the
    /// icon goes quiet and the tooltip names what is being waited on. That is the
    /// state a stuck pi used to hide — the button looked live and did nothing.
    fn stop_button(&self, theme: Theme, cx: &Context<Self>) -> AnyElement {
        let stopping = self.is_stopping();
        let tooltip = if stopping {
            tr!("runtime.stopping_pi")
        } else {
            tr!(
                "composer.shortcut_hint",
                keys = platform::shortcuts::label(platform::shortcuts::STOP),
                action = tr!("shortcut.stop")
            )
        };
        // Bespoke: the composer's round send/stop controls morph, fill with
        // the action role and own their press ramps.
        icon_button_frame(div().id("stop-btn"), &theme, ButtonSize::Medium)
            .group(BUTTON_GROUP)
            .rounded_full()
            .raised(theme.surface_elevated, &theme)
            .hover_danger(&theme)
            .pressed_state(&theme, PressedRamp::Danger)
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_abort_mouse))
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(tooltip.clone())).into())
            .child(icon(
                "icons/stop-run.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                if stopping {
                    theme.text_muted
                } else {
                    theme.text_primary
                },
            ))
            .into_any_element()
    }

    /// Persistent run-status strip above the composer: a calm, single-line
    /// account of what pi is doing between turns — waiting out a transient
    /// provider error (with the attempt counter and a real Cancel that sends
    /// `abort_retry`) or compacting the conversation context. Unlike the
    /// status bar's transient messages, this stays up for the whole state.
    pub(super) fn run_status_strip(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *theme::get(cx);
        if let Some(retry) = &self.retry_detail {
            let attempt = match retry.max {
                Some(max) => tr!("runtime.attempt_of", attempt = retry.attempt, max = max),
                None => tr!("runtime.attempt", attempt = retry.attempt),
            };
            return Some(
                div()
                    .w_full()
                    .mb(DynamicSpacing::Base08.px(&theme))
                    .px(px(10.))
                    .py(px(7.))
                    // A floating strip over the wallpaper, like the approval
                    // bar below it: translucent, and frosted wherever the
                    // renderer can blur.
                    .card_surface(
                        &theme,
                        theme.hairline(),
                        theme.card_fill(theme.surface_elevated),
                    )
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(theme.warning),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!(
                                "runtime.retrying",
                                attempt = attempt,
                                error = retry.error
                            )),
                    )
                    .child(Button::new("cancel-retry", ButtonVariant::Ghost)
                        .size(ButtonSize::Compact)
                        .label(tr!("view.cancel"))
                        .faded(&theme, |el| {
                            el.on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.abort_retry(cx);
                                }),
                            )
                        }))
                    .into_any_element(),
            );
        }
        if self.is_compacting {
            return Some(
                div()
                    .w_full()
                    .mb(DynamicSpacing::Base08.px(&theme))
                    .px(px(10.))
                    .py(px(7.))
                    .card_surface(
                        &theme,
                        theme.hairline(),
                        theme.card_fill(theme.surface_elevated),
                    )
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(theme.accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!("view.preparing_conversation_context")),
                    )
                    .into_any_element(),
            );
        }
        None
    }

    /// The inline `ask_user_question` panel: the live question and its options
    /// docked above the composer. It answers the extension's `select` /
    /// `input` requests in place, so a questionnaire never covers the
    /// transcript with a scrim. Every question is buffered locally — Back and
    /// Next step through them so an earlier answer can be changed — and only
    /// the final Submit replays them to the extension. The panel owns the
    /// `AskPanel` key context (↑/↓ move, ⏎ advances, esc declines); its text
    /// field carries `AskInput` so Enter advances rather than submitting the
    /// composer.
    pub(super) fn ask_panel(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let prompt = self.ask.as_ref()?;
        let question = prompt.question()?;
        let theme = *theme::get(cx);
        let submitted = prompt.submitted;
        let cursor = prompt.cursor;
        let total = prompt.total();
        let multi = prompt.is_multi();
        let highlighted = prompt.highlighted();

        let mut card = div()
            .id("ask-panel")
            .w_full()
            .mb(DynamicSpacing::Base08.px(&theme))
            // Sits in the composer column beside the approval bar and the run
            // strip, so it takes the same glass: a pane over the wallpaper,
            // not an opaque slab dropped on it.
            .card_surface(
                &theme,
                theme.accent.opacity(0.3),
                theme.card_fill(theme.surface_elevated),
            )
            .flex()
            .flex_col()
            .overflow_hidden()
            .key_context("AskPanel")
            .track_focus(&self.ask_focus)
            .on_action(cx.listener(Self::on_ask_next))
            .on_action(cx.listener(Self::on_ask_prev))
            .on_action(cx.listener(Self::on_ask_confirm))
            .on_action(cx.listener(Self::on_ask_submit))
            .on_action(cx.listener(Self::on_ask_close));

        // ── header: icon, header chip, progress ──
        let mut header = div()
            .px(px(14.))
            .pt(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme));
        header = header.child(
            div()
                .flex_none()
                .size(px(24.))
                .rounded(Radius::Medium.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.accent.opacity(0.16))
                .child(icon(
                    "icons/task.svg",
                    IconSize::Small.px(&theme),
                    theme.accent_text,
                )),
        );
        if !question.header.trim().is_empty() {
            header = header.child(
                div()
                    .flex_none()
                    .px(px(7.))
                    .py(px(2.))
                    .rounded(Radius::Small.px(&theme))
                    .bg(theme.overlay_strong)
                    .text_size(TextSize::XSmall.px(&theme))
                    .line_height(theme.ui_px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(SharedString::from(question.header.clone())),
            );
        }
        header = header.child(div().flex_1());
        if total > 1 {
            header = header.child(
                div()
                    .flex_none()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("ask.question_of", current = cursor + 1, total = total)),
            );
        }
        // A visible dismiss affordance beside the keyboard hint.
        header = header.child(
            IconButton::new("ask-close", "icons/x.svg")
                .size(ButtonSize::Compact)
                .variant(IconButtonVariant::Glass)
                .ink(theme.text_muted)
                .tip(tr!("common.close"))
                .on_click(
                    &theme,
                    cx.listener(|this, _, window, cx| this.ask_cancel(window, cx)),
                ),
        );
        card = card.child(header);

        // ── the question ──
        card = card.child(
            div()
                .px(px(14.))
                .pt(DynamicSpacing::Base08.px(&theme))
                .pb(px(10.))
                .whitespace_normal()
                .text_size(TextSize::Default.px(&theme))
                .line_height(theme.ui_px(19.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(SharedString::from(question.question.clone())),
        );

        // ── body ──
        let mut rows = div()
            .px(DynamicSpacing::Base08.px(&theme))
            .pb(DynamicSpacing::Base08.px(&theme))
            .flex()
            .flex_col()
            .gap(px(2.));
        for (ix, option) in question.options.iter().enumerate() {
            let row_highlighted = !submitted && ix == highlighted;
            let checked = multi
                && prompt
                    .checked
                    .get(cursor)
                    .and_then(|row| row.get(ix))
                    .copied()
                    .unwrap_or(false);
            let chosen = row_highlighted || checked;
            // A number reads as a question rather than a menu; multi
            // additionally marks its toggles with a trailing check.
            let marker = div()
                .flex_none()
                .w(px(16.))
                .text_size(TextSize::Small.px(&theme))
                .line_height(theme.ui_px(17.))
                .text_color(if chosen {
                    theme.accent_text
                } else {
                    theme.text_muted
                })
                .child(format!("{}. ", ix + 1));
            let mut copy = div().min_w_0().flex_1().flex().flex_col().gap(px(1.));
            copy = copy.child(
                div()
                    .whitespace_normal()
                    .text_size(TextSize::Small.px(&theme))
                    .line_height(theme.ui_px(17.))
                    .text_color(if chosen {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(SharedString::from(option.label.clone())),
            );
            // The description only under the focused row keeps the list
            // compact so every choice stays visible at a glance.
            if row_highlighted && !option.description.trim().is_empty() {
                copy = copy.child(
                    div()
                        .whitespace_normal()
                        .text_size(TextSize::Small.px(&theme))
                        .line_height(theme.ui_px(16.))
                        .text_color(theme.text_muted)
                        .child(SharedString::from(option.description.clone())),
                );
            }
            let mut row = div()
                .id(ElementId::NamedInteger("ask-option".into(), ix as u64))
                .w_full()
                .min_w_0()
                .px(px(10.))
                .py(DynamicSpacing::Base06.px(&theme))
                .rounded(Radius::Medium.px(&theme))
                .border_1()
                .border_color(if row_highlighted {
                    theme.border_strong
                } else {
                    gpui::transparent_black()
                })
                .when(row_highlighted, |row| row.bg(theme.selected_wash()))
                .when(!submitted, |row| row.cursor_pointer())
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base04.px(&theme))
                .hover_on_glass(&theme)
                .child(marker)
                .child(copy);
            if multi && checked {
                row = row.child(icon(
                    "icons/check.svg",
                    IconSize::XSmall.px(&theme),
                    theme.accent_text,
                ));
            }
            if !submitted {
                row = row.on_click(cx.listener(move |this, _, _, cx| this.ask_choose(ix, cx)));
            }
            rows = rows.child(row);
        }
        // Single-select questions carry a trailing "Type something." row.
        if !multi {
            let trailing = question.options.len();
            let row_highlighted = !submitted && trailing == highlighted;
            let mut row = div()
                .id("ask-trailing")
                .w_full()
                .min_w_0()
                .px(px(10.))
                .py(px(7.))
                .rounded(Radius::Medium.px(&theme))
                .border_1()
                .border_color(if row_highlighted {
                    theme.border_strong
                } else {
                    gpui::transparent_black()
                })
                .when(row_highlighted, |row| row.bg(theme.selected_wash()))
                .when(!submitted, |row| row.cursor_pointer())
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .hover_on_glass(&theme)
                .child(icon(
                    "icons/compose.svg",
                    IconSize::XSmall.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .line_height(theme.ui_px(17.))
                        .text_color(if row_highlighted {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        })
                        .child(tr!("view.type_something")),
                );
            if !submitted {
                row =
                    row.on_click(cx.listener(move |this, _, _, cx| this.ask_choose(trailing, cx)));
            }
            rows = rows.child(row);
        }
        card = card.child(rows);

        // The focused option's preview (single-select + preview only).
        // Labeled so authored mockups read as preview content, not a
        // second interface.
        if !multi {
            if let Some(option) = question.options.get(highlighted) {
                if let Some(preview) = option.preview.as_deref() {
                    let preview = if preview.chars().count() > 600 {
                        let mut capped: String = preview.chars().take(600).collect();
                        capped.push('…');
                        capped
                    } else {
                        preview.to_string()
                    };
                    card = card.child(
                        div()
                            .mx(DynamicSpacing::Base12.px(&theme))
                            .mb(px(10.))
                            .rounded(panel::RADIUS.px(&theme))
                            .border_1()
                            .border_color(theme.hairline())
                            .bg(theme.background)
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px(px(10.))
                                    .py(px(5.))
                                    .flex()
                                    .items_center()
                                    .gap(DynamicSpacing::Base06.px(&theme))
                                    .border_b_1()
                                    .border_color(theme.hairline())
                                    .child(icon(
                                        "icons/eye.svg",
                                        IconSize::XSmall.px(&theme),
                                        theme.text_muted,
                                    ))
                                    .child(
                                        div()
                                            .text_size(TextSize::XSmall.px(&theme))
                                            .line_height(theme.ui_px(14.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_muted)
                                            .child(tr!("ask.preview", label = option.label)),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(10.))
                                    .py(DynamicSpacing::Base08.px(&theme))
                                    .font_family(theme::code_font_family())
                                    .text_size(theme.code_px(11.))
                                    .line_height(theme.code_px(16.))
                                    .text_color(theme.text_secondary)
                                    .whitespace_normal()
                                    .child(SharedString::from(preview)),
                            ),
                    );
                }
            }
        }

        // The custom-answer field: always on multi questions, and on a
        // single-select question once its "Type something." row is active.
        if prompt.custom_active() {
            card = card.child(
                input_field_frame(div(), &theme)
                    .mx(DynamicSpacing::Base12.px(&theme))
                    .mb(DynamicSpacing::Base08.px(&theme))
                    .border_color(theme.border_strong)
                    .bg(theme.surface_secondary)
                    .child(prompt.input.clone()),
            );
        }

        // ── footer: hint + Back / Next (Submit on the last question) ──
        let hint = if submitted {
            tr!("ask.sending")
        } else if multi {
            tr!("ask.hint_multi")
        } else {
            tr!("ask.hint_single")
        };
        let back_disabled = submitted || cursor == 0;
        let back = Button::new("ask-back", ButtonVariant::Glass)
            .label(tr!("view.back"))
            .disabled(back_disabled)
            .on_click(
                &theme,
                cx.listener(|this, _, window, cx| this.ask_prev_question(window, cx)),
            );

        let last = cursor + 1 >= total;
        let next_label = if last {
            tr!("ask.submit")
        } else {
            tr!("ask.next")
        };
        let next = Button::new("ask-next", ButtonVariant::Tinted)
            .label(next_label)
            .disabled(submitted)
            .on_click(
                &theme,
                cx.listener(|this, _, window, cx| this.ask_next_question(window, cx)),
            );

        card = card.child(
            div()
                .h(px(46.))
                .px(DynamicSpacing::Base12.px(&theme))
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .border_t_1()
                .border_color(theme.hairline())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(hint),
                )
                .child(back)
                .child(next),
        );

        Some(card.into_any_element())
    }

    /// The inline access-guard approval: a compact bar directly above the
    /// composer asking whether a mutating tool call may run. Rendered here
    /// rather than as the scrim modal so the transcript stays visible and the
    /// answer sits next to the composer. Each option is a button; the bar also
    /// owns the `Approval` key context (↑/↓ move, ⏎ confirms, esc denies). It
    /// reveals by animating its own height so the page never jumps.
    pub(super) fn approval_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        /// Fixed row height — a single-line prompt, so the reveal can animate
        /// a known height and the surrounding layout can never reflow abruptly.
        const BAR_H: f32 = 48.;
        const GAP: f32 = 8.;

        let request = self.approval.as_ref()?;
        let theme = *theme::get(cx);
        // An MCP tool's raw name is `mcp__server__tool`; the bar names the
        // server and tool instead so a permission decision is readable.
        let heading =
            if let Some((server, tool)) = crate::transcript_view::mcp_tool_parts(&request.tool) {
                tr!("mcp.allow_tool", server = server, tool = tool)
            } else if request.tool.trim().is_empty() {
                tr!("approval.permission_needed")
            } else {
                tr!("approval.allow_tool", tool = request.tool)
            };
        let detail = request.detail.clone();

        let mut buttons = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme));
        for (ix, option) in request.options.iter().enumerate() {
            let highlighted = ix == self.approval_highlight;
            let is_deny = option.eq_ignore_ascii_case("deny");
            let is_primary = !is_deny && !option.to_ascii_lowercase().contains("always");
            let id = ElementId::NamedInteger("approval-option".into(), ix as u64);
            // Bespoke: approval tiles — the deny option wants a danger hover
            // on the glass face, and the keyboard cursor swaps the hairline
            // per render; no `Button` variant covers both.
            let mut button = button_frame(div().id(id), &theme, ButtonSize::Medium)
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .border_1()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| this.approval_choose(ix, window, cx)),
                );
            // The accent is a text/icon tone, never a full fill here: the
            // affirmative is an accent-washed tile with accent ink, matching the
            // One Accent Rule.
            button = if is_primary {
                button
                    .raised(theme.accent.opacity(0.16), &theme)
                    .text_color(theme.accent_text)
                    .hover_accent_wash(&theme, AccentWash::Soft)
            } else if is_deny {
                button
                    .raised(theme.overlay, &theme)
                    .text_color(theme.text_secondary)
                    .hover_danger_soft(&theme)
            } else {
                button
                    .raised(theme.overlay, &theme)
                    .text_color(theme.text_secondary)
                    .hover_overlay_strong(&theme)
            };
            // The keyboard cursor reads as a strong border.
            button = button.border_color(if highlighted {
                theme.border_strong
            } else {
                gpui::transparent_black()
            });
            buttons = buttons.child(button.child(option.clone()));
        }

        let bar = div()
            .id("approval-bar")
            .w_full()
            .h(px(BAR_H))
            .mb(px(GAP))
            .px(DynamicSpacing::Base12.px(&theme))
            .card_surface(
                &theme,
                theme.accent.opacity(0.3),
                theme.card_fill(theme.surface_elevated),
            )
            .flex()
            .items_center()
            .gap(px(10.))
            .overflow_hidden()
            .key_context("Approval")
            .track_focus(&self.approval_focus)
            .on_action(cx.listener(Self::on_approval_next))
            .on_action(cx.listener(Self::on_approval_prev))
            .on_action(cx.listener(Self::on_approval_confirm))
            .on_action(cx.listener(Self::on_approval_close))
            .child(
                div()
                    .flex_none()
                    .size(px(28.))
                    .rounded(button::RADIUS.px(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.accent.opacity(0.16))
                    .child(icon(
                        "icons/lock.svg",
                        IconSize::Small.px(&theme),
                        theme.accent_text,
                    )),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(heading),
                    )
                    .when(!detail.trim().is_empty(), |col| {
                        col.child(
                            div()
                                .mt(px(1.))
                                .font_family(theme::code_font_family())
                                .truncate()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(detail),
                        )
                    }),
            )
            .child(buttons);

        // Reveal by easing the bar's own height (and its gap) from zero, so the
        // prompt unfolds in place instead of the page snapping down a row.
        let bar: AnyElement = if theme::reduce_motion(cx) {
            bar.into_any_element()
        } else {
            bar.with_animation(
                "approval-in",
                Animation::new(Motion::Layout.duration(&theme))
                    .with_easing(Motion::Layout.easing()),
                move |el, d| el.max_h(px(BAR_H * d)).mb(px(GAP * d)).opacity(d),
            )
            .into_any_element()
        };
        Some(bar)
    }

    /// The pending queue pi is holding, shown as a bar directly above the
    /// composer. While a task is running, messages sent from the composer are
    /// queued as follow-ups here and delivered once the task finishes;
    /// `queue_update` mirrors the list live. One raised card in the sibling
    /// strips' shape, a quiet label header, and flat hairline-separated rows.
    ///
    /// Each row can be edited in place or promoted ahead of the rest of the
    /// queue (issue #79); one row at a time is open in an inline field.
    pub(super) fn queue_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.queue.is_empty() {
            return None;
        }
        let theme = *theme::get(cx);
        let count = self.queue.len();
        let only_steering = self.queue.follow_up.is_empty();
        // The label names the queue's dominant state in the app's 11px label
        // style; the full delivery sentence stays on the tooltip so the
        // header can be one quiet line.
        let label = if only_steering {
            tr!("view.queue_steering")
        } else {
            tr!("view.queue_queued")
        };
        let detail = if only_steering {
            tr!("view.steering_turn", count = count)
        } else if self.queue.steering.is_empty() {
            tr!("view.queued_after_task", count = count)
        } else {
            tr!("view.steering_plus_followups", count = count)
        };

        let mut rows: Vec<AnyElement> = Vec::with_capacity(count);
        for (lane, kind) in [
            (QueueLane::Steer, tr!("view.queue_steer")),
            (QueueLane::FollowUp, tr!("view.queue_follow_up")),
        ] {
            for (index, text) in self.queue.lane(lane).iter().enumerate() {
                let target = QueuedMessage { lane, index };
                rows.push(self.queue_row(target, &kind, text, theme, cx));
            }
        }

        Some(
            div()
                .id("queue-bar")
                .debug_selector(|| "queue-bar".to_string())
                .w_full()
                .mb(DynamicSpacing::Base08.px(&theme))
                .card_surface(
                    &theme,
                    theme.hairline(),
                    theme.card_fill(theme.surface_elevated),
                )
                .flex()
                .flex_col()
                // Clip the rows' hairline separators to the rounded corners.
                .overflow_hidden()
                // The inline editor's own key context, so Enter saves the
                // message instead of submitting the composer.
                .key_context("QueueEdit")
                .on_action(cx.listener(Self::on_queue_edit_confirm))
                .on_action(cx.listener(Self::on_queue_edit_cancel))
                .child(
                    // A clock, `QUEUED · 2`, and the one action: a compact
                    // dismiss. No second button, no sentence in the header.
                    div()
                        .h(px(30.))
                        .px(DynamicSpacing::Base12.px(&theme))
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base06.px(&theme))
                        .child(icon(
                            "icons/clock.svg",
                            IconSize::XSmall.px(&theme),
                            theme.text_muted,
                        ))
                        .child(
                            div()
                                .id("queue-label")
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .tip(detail)
                                .text_size(TextSize::XSmall.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_muted)
                                .child(format!("{label} · {count}").to_uppercase()),
                        )
                        .child(
                            IconButton::new("clear-queue", "icons/x.svg")
                                .size(ButtonSize::Compact)
                                .tip(tr!("view.clear_queue"))
                                .faded(&theme, |el| {
                                    el.debug_selector(|| "clear-queue".to_string()).on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::on_clear_queue),
                                    )
                                }),
                        ),
                )
                .children(rows)
                .into_any_element(),
        )
    }

    /// One pending-message row: the lane label, the message, and that
    /// message's two actions (edit, send now). Flat, with no border or fill of
    /// its own — the bar around it is already the raised surface, and a card
    /// inside a card doubles the chrome (DESIGN.md's card budget).
    ///
    /// The row under edit swaps its text for an inline field with Save and
    /// Cancel, keeping its lane label so the message never loses its context.
    fn queue_row(
        &self,
        target: QueuedMessage,
        kind: &str,
        text: &str,
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        // The open editor, but only while its message is still queued: the
        // target is an index into a list pi owns, so a delivery or a settle can
        // strand it between events.
        let edit = self
            .queue_edit
            .as_ref()
            .filter(|edit| self.queued_message(edit.target).is_some());
        let steer = target.lane == QueueLane::Steer;
        // Only one row is edited at a time, and its siblings' actions would
        // re-index under it, so they stay out of the way until it closes.
        let actions_inert = edit.is_some();
        // Steering is already delivered before the next model call, so the
        // row that leads that lane is as early as the protocol can get.
        let can_promote = !(steer && target.index == 0);
        // One number for the row's element ids, steering rows first.
        let slot = queue_row_slot(&self.queue, target);
        let row_id = ElementId::NamedInteger("queue-row".into(), slot);

        let label = div()
            .flex_none()
            .whitespace_nowrap()
            .pt(px(1.))
            .text_size(TextSize::XSmall.px(&theme))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if steer {
                theme.accent_text
            } else {
                theme.text_muted
            })
            .child(kind.to_uppercase());

        let body: AnyElement = match edit {
            Some(edit) if edit.target == target => {
                let draft = edit.input.read(cx).text();
                let save_live = !draft.trim().is_empty();
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .child(
                        input_field_frame(div(), &theme)
                            .flex_1()
                            .min_w_0()
                            .bg(theme.background)
                            .overflow_hidden()
                            .child(edit.input.clone()),
                    )
                    .child({
                        let save = IconButton::new(
                            ElementId::NamedInteger("queue-edit-save".into(), slot),
                            "icons/check.svg",
                        )
                        .size(ButtonSize::Compact)
                        .ink(theme.accent_text)
                        .disabled(!save_live);
                        let save = if save_live {
                            save.tip(tr!("view.queue_save"))
                        } else {
                            save
                        };
                        save.on_mouse_up(&theme, cx.listener(Self::on_queue_edit_save))
                            .debug_selector(move || format!("queue-row-{slot}-edit-save"))
                    })
                    .child(
                        IconButton::new(
                            ElementId::NamedInteger("queue-edit-cancel".into(), slot),
                            "icons/x.svg",
                        )
                        .size(ButtonSize::Compact)
                        .tip(tr!("view.queue_cancel"))
                        .faded(&theme, |el| {
                            el.debug_selector(move || format!("queue-row-{slot}-edit-cancel"))
                                .on_mouse_up(
                                    MouseButton::Left,
                                    cx.listener(Self::on_queue_edit_cancel_click),
                                )
                        }),
                    )
                    .into_any_element()
            }
            _ => div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_start()
                .gap(DynamicSpacing::Base04.px(&theme))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .line_clamp(2)
                        .text_size(TextSize::Small.px(&theme))
                        .line_height(theme.ui_px(16.))
                        .text_color(theme.text_primary)
                        .child(queue_preview(text)),
                )
                .when(!actions_inert, |row| {
                    row.child(
                        // Two quiet icon actions sit at the row's right edge,
                        // in the same register as the header's clear control.
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .child(
                                IconButton::new(
                                    ElementId::NamedInteger("queue-edit".into(), slot),
                                    "icons/pencil.svg",
                                )
                                .size(ButtonSize::Compact)
                                .tip(tr!("view.queue_edit"))
                                .faded(&theme, |el| {
                                    el.debug_selector(move || format!("queue-row-{slot}-edit"))
                                        .on_mouse_up(
                                            MouseButton::Left,
                                            cx.listener(move |this, _, window, cx| {
                                                this.begin_queue_edit(target, window, cx);
                                            }),
                                        )
                                }),
                            )
                            .when(can_promote, |actions| {
                                actions.child(
                                    IconButton::new(
                                        ElementId::NamedInteger("queue-send-now".into(), slot),
                                        "icons/arrow-up-right.svg",
                                    )
                                    .size(ButtonSize::Compact)
                                    .tip(tr!("view.queue_send_now"))
                                    .on_mouse_up(
                                        &theme,
                                        cx.listener(move |this, _, _, cx| {
                                            this.send_queued_now(target, cx)
                                        }),
                                    )
                                    .debug_selector(move || format!("queue-row-{slot}-send-now")),
                                )
                            }),
                    )
                })
                .into_any_element(),
        };

        div()
            .id(row_id)
            .debug_selector(move || match target.lane {
                QueueLane::Steer => format!("queue-row-steer-{}", target.index),
                QueueLane::FollowUp => format!("queue-row-follow-up-{}", target.index),
            })
            .w_full()
            .flex()
            .items_start()
            .gap(px(10.))
            .px(DynamicSpacing::Base12.px(&theme))
            .py(px(7.))
            .border_t_1()
            .border_color(theme.hairline())
            .child(label)
            .child(body)
            .into_any_element()
    }

    fn on_queue_edit_confirm(
        &mut self,
        _: &crate::QueueEditConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_queue_edit(window, cx);
    }

    fn on_queue_edit_cancel(
        &mut self,
        _: &crate::QueueEditCancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_queue_edit(window, cx);
    }

    fn on_queue_edit_save(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_queue_edit(window, cx);
    }

    fn on_queue_edit_cancel_click(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_queue_edit(window, cx);
    }

    /// The dismissible error banner: a command / protocol / extension failure
    /// surfaced per the docs' error contract. Persists until dismissed. A
    /// dead pi process offers a one-click Reconnect instead of sending the
    /// user to Settings; every banner can copy its text for a bug report.
    /// ⌘⌥P (debug builds): toggle a frame-time capture and report the summary.
    ///
    /// The first press starts recording and arms the frame hook; the second
    /// writes the summary to `~/.orbit-pi/perf.jsonl` and reports it in a toast.
    /// A capture that nobody stops files itself after two minutes.
    pub(super) fn on_record_frame_times(
        &mut self,
        _: &crate::RecordFrameTimes,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(not(debug_assertions))]
        {
            let _ = (&*window, &*cx);
        }
        // Proves the chord reached the handler at all, which is otherwise
        // indistinguishable from "the key never matched".
        #[cfg(debug_assertions)]
        eprintln!("perf: chord received");
        // Both channels on purpose: the toast is for a window that is running,
        // and stderr is for a terminal that is watching. A debug tool that only
        // reports into the UI is hard to trust while wiring it up.
        #[cfg(debug_assertions)]
        if crate::perf::start("capture") {
            crate::perf::arm(window);
            eprintln!("perf: recording — press ⌘⌥P again to stop");
            self.toast_info(tr!("perf.recording"));
        } else if let Some(summary) = crate::perf::finish() {
            eprintln!("perf: {}", summary.line());
            self.toast_info(summary.line());
        } else {
            eprintln!("perf: no frames were recorded");
            self.toast_warning(tr!("perf.no_frames"));
        }
        cx.notify();
    }

    pub(super) fn error_banner(&self, theme: Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let message = self.error.clone()?;
        let copy_text = message.clone();
        let reconnect = self.runtime.exited;
        let mut actions = div().flex().flex_none().items_center().gap_1();
        if reconnect {
            actions = actions.child(
                Button::new("reconnect-runtime", ButtonVariant::Glass)
                    .size(ButtonSize::Default)
                    .label(tr!("view.reconnect"))
                    .on_mouse_up(&theme, cx.listener(|this, _, _, cx| {
                            if this.error_source.as_deref() != Some(REWIND_INTERRUPTED_ERROR_SOURCE)
                            {
                                this.error = None;
                            }
                            this.runtime_restart(cx);
                        })),
            );
        }
        actions = actions
            .child(
                IconButton::new("copy-error", "icons/copy.svg")
                    .size(ButtonSize::Compact)
                    .ink(theme.text_secondary)
                    .tip(tr!("common.copy"))
                    .faded(&theme, |el| {
                        el.on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                            }),
                        )
                    }),
            )
            .child(
                IconButton::new("dismiss-error", "icons/x.svg")
                    .size(ButtonSize::Compact)
                    .ink(theme.text_secondary)
                    .tip(tr!("common.dismiss"))
                    .faded(&theme, |el| {
                        el.on_mouse_up(MouseButton::Left, cx.listener(Self::dismiss_error))
                    }),
            );
        Some(
            div()
                .w_full()
                .mb(DynamicSpacing::Base08.px(&theme))
                .child(error_state(
                    theme,
                    &message,
                    None,
                    Some(&tr!("view.error_hint")),
                    Some(actions.into_any_element()),
                ))
                .into_any_element(),
        )
    }

    /// Discard the pending queue without aborting the run. The composer text
    /// stays untouched (unlike Escape, which restores queued text).
    pub(super) fn on_clear_queue(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_queue_on_clear = false;
        self.send(CommandBody::ClearQueue, "clear_queue");
        cx.notify();
    }

    /// Full-window image lightbox: the clicked attachment at `Contain` scale
    /// over a dimmed scrim. Any click (or Escape) closes it. No new surface
    /// for the app — it reads `OrbitApp::lightbox`, set by `image_opener`.
    pub(super) fn lightbox_layer(
        &self,
        image: std::sync::Arc<Image>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = *theme::get(cx);
        crate::ui::lightbox::lightbox(
            image,
            &theme,
            cx.listener(|this, _, _, cx| {
                this.lightbox = None;
                cx.notify();
            }),
        )
        .into_any_element()
    }
}
