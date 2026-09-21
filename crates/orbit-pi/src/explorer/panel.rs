//! The Explorer's project panel: a Zed-style workspace tree in a right dock.
//!
//! The panel owns the built [`TreeIndex`], the expand/collapse set, the filter
//! input, and its virtualized list. It never reads the filesystem on the
//! render path: [`ProjectPanel::set_workspace`] / [`ProjectPanel::mark_stale`]
//! kick a background snapshot, and rendering only walks the in-memory tree.
//!
//! Clicking a file calls the [`OpenHandler`] the app installed at
//! construction — the panel does not know about the file viewer.

use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    div, prelude::*, px, Animation, AnimationExt, AnyElement, App, ClickEvent, Context,
    CursorStyle, Entity, FocusHandle, FontWeight, Hsla, KeyDownEvent, ListAlignment, ListState,
    MouseButton, MouseDownEvent, Pixels, Render, Subscription, TextAlign, Transformation, Window,
};

use super::tree::{self, Row, StatusBadge, TreeIndex};
use super::walk;
use crate::app::{file_badge, file_glyph, icon, nerd_font_family};
use crate::composer::ComposerInput;
use crate::git;
use crate::platform;
use crate::theme::{self, Theme, ThemeMode};

/// Panel width defaults / drag clamps.
pub const PANEL_DEFAULT_W: f32 = 248.;
pub const PANEL_MIN_W: f32 = 172.;
pub const PANEL_MAX_W: f32 = 460.;

/// Fixed tree row height — 28px, the design system's navigation-row height.
const ROW_H: f32 = 28.;
const HEADER_H: f32 = 40.;
const FILTER_H: f32 = 34.;
const FOOTER_H: f32 = 26.;

/// What a row click does when the app is told a file was picked. The string
/// is the workspace-relative path, used to label the file in the viewer.
pub type OpenHandler = Rc<dyn Fn(PathBuf, String, &mut App)>;

/// Drag marker for the panel's resize handle (gpui typed drag state).
pub struct ExplorerResize;

/// An invisible drag ghost — resizing leaves no floating preview.
struct ExplorerDragGhost;

impl Render for ExplorerDragGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Which row the context menu targets.
struct PanelMenu {
    path: String,
    is_dir: bool,
    expanded: bool,
}

pub struct ProjectPanel {
    open: bool,
    width: Pixels,
    /// On Windows, reserve the top-right caption-controls inset in the header
    /// when this dock is the window's rightmost column.
    reserve_controls: bool,
    /// The workspace root the panel is showing.
    workspace: Option<PathBuf>,
    index: TreeIndex,
    /// Visible rows flattened from `index` + `expanded` + filter.
    rows: Vec<Row>,
    expanded: HashSet<String>,
    show_hidden: bool,
    /// A snapshot needs to run (workspace changed, watcher fired, hidden toggled).
    stale: bool,
    loading: bool,
    /// Guards a late snapshot from a previous request.
    generation: u64,
    error: Option<String>,
    list: ListState,
    /// Keyboard cursor: index into `rows`.
    cursor: Option<usize>,
    filter: Entity<ComposerInput>,
    _filter_sub: Subscription,
    focus: FocusHandle,
    /// Workspace-relative path of the file the app is showing.
    active: Option<String>,
    /// Right-click context menu, if open.
    menu: Option<PanelMenu>,
    /// When the menu was dismissed by an outside click; guards re-open.
    menu_dismissed_at: Option<Instant>,
    /// The app's file-open callback.
    on_open: OpenHandler,
}

impl ProjectPanel {
    pub fn new(on_open: OpenHandler, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("explorer.filter_files")
                .with_key_context("Composer Picker")
        });
        let filter_sub = cx.observe(&filter, |this, _, cx| {
            this.refresh_rows(cx);
            cx.notify();
        });
        Self {
            open: false,
            width: px(PANEL_DEFAULT_W),
            reserve_controls: false,
            workspace: None,
            index: TreeIndex::default(),
            rows: Vec::new(),
            expanded: HashSet::new(),
            show_hidden: false,
            stale: false,
            loading: false,
            generation: 0,
            error: None,
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            cursor: None,
            filter,
            _filter_sub: filter_sub,
            focus: cx.focus_handle(),
            active: None,
            menu: None,
            menu_dismissed_at: None,
            on_open,
        }
    }

    // ── state entry points (called from the app shell) ─────────────────

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn width(&self) -> Pixels {
        self.width
    }

    pub fn set_width(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let width = width.clamp(px(PANEL_MIN_W), px(PANEL_MAX_W));
        if width != self.width {
            self.width = width;
            cx.notify();
        }
    }

    /// Reserve the window-control inset in the header (Windows only, and only
    /// while this dock is the window's rightmost column).
    pub fn set_reserve_controls(&mut self, reserve: bool, cx: &mut Context<Self>) {
        if reserve != self.reserve_controls {
            self.reserve_controls = reserve;
            cx.notify();
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        if !self.open {
            self.menu = None;
        } else {
            self.ensure_loaded(cx);
        }
        cx.notify();
    }

    /// Close the dock if it is open. Used while the Review pane is open — the
    /// two right docks are mutually exclusive.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.open = false;
            self.menu = None;
            cx.notify();
        }
    }

    /// The app calls this every render; a changed workspace rebuilds the tree.
    pub fn set_workspace(&mut self, workspace: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.workspace == workspace {
            return;
        }
        self.workspace = workspace;
        self.expanded.clear();
        self.index = TreeIndex::default();
        self.rows.clear();
        self.cursor = None;
        self.active = None;
        self.error = None;
        self.list.reset(0);
        self.stale = true;
        self.ensure_loaded(cx);
    }

    /// The workspace watcher fired (or a run settled): rebuild off-thread.
    pub fn mark_stale(&mut self, cx: &mut Context<Self>) {
        if self.workspace.is_some() {
            self.stale = true;
            self.ensure_loaded(cx);
        }
    }

    /// Highlight the file the viewer is showing, and reveal it.
    pub fn set_active(&mut self, active: Option<String>, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if let Some(path) = self.active.clone() {
            self.reveal_path(&path);
            self.refresh_rows(cx);
            self.reveal_active(cx);
        }
        cx.notify();
    }

    // ── loading ────────────────────────────────────────────────────────

    fn ensure_loaded(&mut self, cx: &mut Context<Self>) {
        if !self.stale || self.loading {
            return;
        }
        let Some(root) = self.workspace.clone() else {
            self.stale = false;
            return;
        };
        self.stale = false;
        self.loading = true;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let show_hidden = self.show_hidden;

        cx.spawn(async move |this, cx| {
            let (snapshot, badges) = cx
                .background_executor()
                .spawn(async move {
                    let snapshot = walk::snapshot(&root, show_hidden);
                    // Git status is a subprocess; it must not run on the UI thread.
                    let badges = git::status_rows(&root)
                        .map(|rows| {
                            rows.into_iter()
                                .map(|row| {
                                    (row.path.clone(), StatusBadge::from_status(&row))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    (snapshot, badges)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.index = TreeIndex::build(snapshot, badges);
                this.loading = false;
                this.error = None;
                this.refresh_rows(cx);
                cx.notify();
            });
        })
        .detach();
    }

    // ── rows ───────────────────────────────────────────────────────────

    fn refresh_rows(&mut self, cx: &Context<Self>) {
        let filter = self.filter.read(cx).text();
        let rows = tree::visible_rows(&self.index, &self.expanded, &filter);
        if rows.len() != self.list.item_count() {
            self.list.reset(rows.len());
        }
        self.rows = rows;
        self.cursor = self
            .cursor
            .filter(|index| *index < self.rows.len());
    }

    fn toggle_dir(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.expanded.remove(&path) {
            self.expanded.insert(path);
        }
        self.refresh_rows(cx);
        cx.notify();
    }

    fn expand_all(&mut self, cx: &mut Context<Self>) {
        self.expanded = self.index.all_dir_paths();
        self.refresh_rows(cx);
        cx.notify();
    }

    fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expanded.clear();
        self.refresh_rows(cx);
        cx.notify();
    }

    fn activate(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(index).cloned() else {
            return;
        };
        if row.is_dir() {
            self.toggle_dir(row.path, cx);
        } else {
            self.open_file(row.path, cx);
        }
    }

    fn open_file(&mut self, relative: String, cx: &mut Context<Self>) {
        let Some(root) = self.workspace.clone() else {
            return;
        };
        self.active = Some(relative.clone());
        (self.on_open)(walk::absolute(&root, &relative), relative, cx);
        cx.notify();
    }

    /// Expand every ancestor of `path` so its row becomes visible.
    fn reveal_path(&mut self, path: &str) {
        for ancestor in tree::ancestors(path) {
            self.expanded.insert(ancestor);
        }
    }

    /// Scroll the active/cursor row into view.
    fn reveal_active(&mut self, cx: &mut Context<Self>) {
        let Some(active) = self.active.as_deref() else {
            return;
        };
        if let Some(index) = self.rows.iter().position(|row| row.path == active) {
            self.list.scroll_to_reveal_item(index);
            cx.notify();
        }
    }

    // ── keyboard ───────────────────────────────────────────────────────

    fn on_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let key = event.keystroke.key.as_str();
        let platform = event.keystroke.modifiers.platform;
        // Escape first: close the row menu, or clear a non-empty filter, and
        // keep the key from falling through to the global abort action.
        if key == "escape" {
            if self.menu.is_some() {
                self.dismiss_menu(cx);
                cx.stop_propagation();
            } else if !self.filter.read(cx).text().is_empty() {
                self.filter.update(cx, |input, cx| input.clear(cx));
                self.refresh_rows(cx);
                cx.notify();
                cx.stop_propagation();
            }
            return;
        }
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        match key {
            "down" => {
                let next = self.cursor.map_or(0, |ix| (ix + 1).min(count - 1));
                self.move_cursor(next, cx);
            }
            "up" => {
                let next = self.cursor.map_or(0, |ix| ix.saturating_sub(1));
                self.move_cursor(next, cx);
            }
            "home" => self.move_cursor(0, cx),
            "end" => self.move_cursor(count - 1, cx),
            "right" if platform => self.expand_all(cx),
            "left" if platform => self.collapse_all(cx),
            "right" => {
                if let Some(index) = self.cursor {
                    if self.rows.get(index).is_some_and(Row::is_dir) {
                        let expanded = self.rows[index].expanded();
                        if !expanded {
                            let path = self.rows[index].path.clone();
                            self.toggle_dir(path, cx);
                        }
                    }
                }
            }
            "left" => {
                if let Some(index) = self.cursor {
                    if self.rows.get(index).is_some_and(Row::is_dir) && self.rows[index].expanded() {
                        let path = self.rows[index].path.clone();
                        self.toggle_dir(path, cx);
                    }
                }
            }
            "enter" | "space" => {
                if let Some(index) = self.cursor {
                    self.activate(index, cx);
                }
            }
            _ => {}
        }
    }

    fn move_cursor(&mut self, index: usize, cx: &mut Context<Self>) {
        self.cursor = Some(index);
        self.list.scroll_to_reveal_item(index);
        cx.notify();
    }

    // ── context menu ───────────────────────────────────────────────────

    fn open_menu(&mut self, row: Row, cx: &mut Context<Self>) {
        let is_dir = row.is_dir();
        let expanded = row.expanded();
        self.menu = Some(PanelMenu {
            path: row.path,
            is_dir,
            expanded,
        });
        cx.notify();
    }

    fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            self.menu_dismissed_at = Some(Instant::now());
            cx.notify();
        }
    }

    fn menu_target_path(&self) -> Option<PathBuf> {
        let menu = self.menu.as_ref()?;
        let root = self.workspace.clone()?;
        Some(walk::absolute(&root, &menu.path))
    }
}

// ── rendering ──────────────────────────────────────────────────────────

fn badge_color(badge: StatusBadge, theme: &Theme) -> Hsla {
    match badge {
        StatusBadge::Added | StatusBadge::Untracked => theme.add_green,
        StatusBadge::Modified => theme.warn,
        StatusBadge::Deleted => theme.del_red,
        StatusBadge::Renamed => theme.accent,
        StatusBadge::Conflicted => theme.crit,
    }
}

impl ProjectPanel {
    fn header(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .workspace
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| tr!("explorer.files"));
        div()
            .h(px(HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .pl(px(12.))
            .pr(px(if self.reserve_controls {
                crate::platform::WINDOW_CONTROLS_W
            } else {
                6.
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme.ui_px(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(title),
            )
            .child(ghost_icon(
                &theme,
                "explorer-refresh",
                "icons/refresh.svg",
                theme.text_3,
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.mark_stale(cx);
                }),
            ))
            .child(ghost_icon(
                &theme,
                "explorer-collapse",
                "icons/chevron-up.svg",
                theme.text_3,
                cx.listener(|this, _: &ClickEvent, _, cx| this.collapse_all(cx)),
            ))
            .child(ghost_icon(
                &theme,
                "explorer-hidden",
                if self.show_hidden {
                    "icons/eye.svg"
                } else {
                    "icons/eye-off.svg"
                },
                if self.show_hidden {
                    theme.text
                } else {
                    theme.text_3
                },
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.show_hidden = !this.show_hidden;
                    this.stale = true;
                    this.ensure_loaded(cx);
                    cx.notify();
                }),
            ))
            .into_any_element()
    }

    fn filter_row(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let has_text = !self.filter.read(cx).text().is_empty();
        div()
            .h(px(FILTER_H))
            .flex_none()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .border_b_1()
            .border_color(theme.border)
            .child(icon("icons/search.svg", 12., theme.text_3))
            .child(self.filter.clone())
            .when(has_text, |row| {
                row.child(ghost_icon(
                    &theme,
                    "explorer-filter-clear",
                    "icons/x.svg",
                    theme.text_3,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.filter.update(cx, |input, cx| input.clear(cx));
                        this.refresh_rows(cx);
                        cx.notify();
                    }),
                ))
            })
            .into_any_element()
    }

    fn tree(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let list = gpui::list(self.list.clone(), move |index, _window, cx| {
            entity
                .upgrade()
                .map(|entity| entity.update(cx, |this, cx| this.render_row(index, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .size_full()
        .py(px(4.));
        let no_files_detail = tr!("explorer.no_files_detail");
        let body: AnyElement = if self.loading && self.rows.is_empty() {
            loading_state(&theme)
        } else if let Some(error) = &self.error {
            centered_message(&theme, tr!("explorer.load_error"), Some(error.as_str()))
        } else if self.rows.is_empty() {
            centered_message(
                &theme,
                tr!("explorer.no_files"),
                Some(no_files_detail.as_str()),
            )
        } else {
            list.into_any_element()
        };
        div()
            .id("explorer-tree")
            .track_focus(&self.focus)
            .key_context("ProjectPanel")
            .flex_1()
            .min_h_0()
            .relative()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                this.on_key_down(event, cx)
            }))
            .child(body)
            .into_any_element()
    }

    fn footer(&self, theme: Theme, _cx: &mut Context<Self>) -> AnyElement {
        let label = if self.index.truncated {
            tr!("explorer.files_truncated", count = self.index.file_count)
        } else {
            tr!("explorer.files_count", count = self.index.file_count)
        };
        div()
            .h(px(FOOTER_H))
            .flex_none()
            .px(px(12.))
            .border_t_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .text_size(theme.ui_px(11.))
            .text_color(if self.index.truncated {
                theme.warn
            } else {
                theme.text_3
            })
            .child(label)
            .into_any_element()
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(index).cloned() else {
            return div().h(px(ROW_H)).into_any_element();
        };
        let theme = *theme::get(cx);
        let cursor = self.cursor == Some(index);
        let active = self.active.as_deref() == Some(row.path.as_str());
        let nerd = nerd_font_family(cx);
        let dark = theme.mode == ThemeMode::Dark;
        // Dirs lead with a chevron + folder; files with a devicon. The file
        // indent offsets that difference so names align in one column.
        let indent = (if row.is_dir() { 7.0 } else { 23.0 }) + row.depth as f32 * 14.0;
        let icon_color = if active { theme.active_fg } else { theme.text_3 };
        let name_color = if active {
            theme.active_fg
        } else {
            theme.text_2
        };

        let mut content = div()
            .id(gpui::ElementId::Name(
                format!("explorer-row-{}", row.path).into(),
            ))
            .h(px(ROW_H))
            .mx(px(4.))
            .pl(px(indent))
            .pr(px(8.))
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            // Selected (open in the viewer) wins; the keyboard cursor is a
            // quieter wash; pointer hover is the lightest step.
            .when(active, |el| el.bg(theme.active))
            .when(!active && cursor, |el| el.bg(theme.overlay))
            .when(!active && !cursor, |el| el.hover(|el| el.bg(theme.bg_hover)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus);
                this.cursor = Some(index);
                this.activate(index, cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let row = row.clone();
                    move |this, _: &MouseDownEvent, window, cx| {
                        window.focus(&this.focus);
                        this.cursor = Some(index);
                        this.open_menu(row.clone(), cx);
                    }
                }),
            );

        if row.is_dir() {
            content = content
                .child(icon(
                    if row.expanded() {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    10.,
                    icon_color,
                ))
                .child(icon("icons/folder.svg", 13., icon_color))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(theme.ui_px(12.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(name_color)
                        .child(row.name.clone()),
                );
            if row.dirty {
                content = content.child(
                    div()
                        .size(px(5.))
                        .flex_none()
                        .rounded_full()
                        .bg(theme.warn),
                );
            }
        } else {
            let fallback = file_badge(&row.path, theme);
            let glyph = file_glyph(&row.path, dark, nerd.as_ref(), 13., fallback);
            content = content
                .child(glyph)
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(theme.ui_px(12.5))
                        .text_color(name_color)
                        .child(row.name.clone()),
                );
            if let Some(badge) = row.badge {
                let color = badge_color(badge, &theme);
                content = content.child(
                    div()
                        .flex_none()
                        .text_size(theme.ui_px(11.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(badge.letter().to_string()),
                );
            }
        }

        content.into_any_element()
    }

    fn context_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        if self
            .menu_dismissed_at
            .is_some_and(|at| at.elapsed() < Duration::from_millis(150))
        {
            return None;
        }
        let target = self.menu_target_path()?;
        let relative = menu.path.clone();
        let is_dir = menu.is_dir;
        let expanded = menu.expanded;
        let mut popup = div()
            .id("explorer-context-menu")
            .absolute()
            .top(px(HEADER_H + FILTER_H + 4.))
            .left(px(8.))
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
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dismiss_menu(cx);
            }));

        if is_dir {
            // A directory's first action is its expand/collapse toggle.
            let (label, icon_path) = if expanded {
                (tr!("explorer.collapse"), "icons/chevron-down.svg")
            } else {
                (tr!("explorer.expand"), "icons/chevron-right.svg")
            };
            popup = popup
                .child(menu_row(
                    theme,
                    icon_path,
                    label,
                    None,
                    cx.listener({
                        let relative = relative.clone();
                        move |this, _: &ClickEvent, _, cx| {
                            this.dismiss_menu(cx);
                            this.toggle_dir(relative.clone(), cx);
                        }
                    }),
                ))
                .child(separator(theme));
        } else {
            popup = popup
                .child(menu_row(
                    theme,
                    "icons/file.svg",
                    tr!("explorer.open"),
                    None,
                    cx.listener({
                        let relative = relative.clone();
                        move |this, _: &ClickEvent, _, cx| {
                            this.dismiss_menu(cx);
                            this.open_file(relative.clone(), cx);
                        }
                    }),
                ))
                .child(separator(theme));
        }
        popup = popup
            .child(menu_row(
                theme,
                "icons/arrow-up-right.svg",
                tr!("explorer.open_default"),
                None,
                cx.listener({
                    let target = target.clone();
                    move |this, _: &ClickEvent, _, cx| {
                        platform::open_path_default(&target);
                        this.dismiss_menu(cx);
                    }
                }),
            ))
            .child(menu_row(
                theme,
                "icons/folder.svg",
                tr!("explorer.reveal"),
                None,
                cx.listener({
                    let target = target.clone();
                    move |this, _: &ClickEvent, _, cx| {
                        platform::reveal_in_file_manager(&target);
                        this.dismiss_menu(cx);
                    }
                }),
            ))
            .child(separator(theme))
            .child(menu_row(
                theme,
                "icons/copy.svg",
                tr!("explorer.copy_path"),
                None,
                cx.listener({
                    let text = target.to_string_lossy().to_string();
                    move |this, _: &ClickEvent, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                        this.dismiss_menu(cx);
                    }
                }),
            ))
            .child(menu_row(
                theme,
                "icons/copy.svg",
                tr!("explorer.copy_relative_path"),
                None,
                cx.listener({
                    let text = relative.clone();
                    move |this, _: &ClickEvent, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.clone()));
                        this.dismiss_menu(cx);
                    }
                }),
            ));
        Some(popup.into_any_element())
    }
}

/// A header/toolbar ghost icon control: 24px hit area, hover fill only.
fn ghost_icon(
    theme: &Theme,
    id: &'static str,
    path: &'static str,
    color: Hsla,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .size(px(24.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|el| el.bg(theme.bg_hover))
        .on_click(listener)
        .child(icon(path, 13., color))
        .into_any_element()
}

fn separator(theme: Theme) -> AnyElement {
    div()
        .h(px(1.))
        .mx(px(4.))
        .my(px(4.))
        .bg(theme.border)
        .into_any_element()
}

fn menu_row(
    theme: Theme,
    icon_path: &'static str,
    label: String,
    hint: Option<String>,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(gpui::ElementId::Name(
            format!("explorer-menu-{label}").into(),
        ))
        .h(px(28.))
        .mx(px(4.))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .hover(|el| el.bg(theme.overlay))
        .on_click(listener)
        .child(icon(icon_path, 12., theme.text_3))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .truncate()
                .text_size(theme.ui_px(12.))
                .text_color(theme.text_2)
                .child(label),
        )
        .children(hint.map(|hint| {
            div()
                .flex_none()
                .text_size(theme.ui_px(10.5))
                .text_color(theme.text_3)
                .child(hint)
        }))
        .into_any_element()
}

/// A page-level spinner; static under reduce-motion.
fn spinner(id: &'static str, theme: &Theme) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/loader.svg")
        .flex_none()
        .size(px(13.))
        .text_color(theme.text_3);
    if theme.ui.reduce_motion {
        return svg.into_any_element();
    }
    svg.with_animation(
        id,
        Animation::new(Duration::from_millis(900)).repeat(),
        |svg, delta| {
            svg.with_transformation(Transformation::rotate(gpui::radians(
                delta * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

fn loading_state(theme: &Theme) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .child(spinner("explorer-loading", theme))
        .child(
            div()
                .text_size(theme.ui_px(12.5))
                .text_color(theme.text_3)
                .child(tr!("explorer.loading")),
        )
        .into_any_element()
}

fn centered_message(theme: &Theme, title: String, detail: Option<&str>) -> AnyElement {
    let mut column = div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(16.))
        .pb(px(24.))
        .child(
            div()
                .text_size(theme.ui_px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(px(6.))
                .max_w(px(260.))
                .text_align(TextAlign::Center)
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_3)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}

impl Render for ProjectPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        self.ensure_loaded(cx);
        // The filter is read here; the observer marks rows dirty on change.
        self.refresh_rows(cx);
        let theme = *theme::get(cx);
        div()
            .id("project-panel")
            .relative()
            .flex_none()
            .w(self.width)
            .h_full()
            .bg(theme.bg_sidebar)
            .border_l_1()
            .border_r_1()
            .border_color(theme.border)
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .id("explorer-resize-handle")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|style| style.bg(theme.accent.opacity(0.4)))
                    .on_drag(ExplorerResize, |_, _, _, cx| {
                        cx.new(|_| ExplorerDragGhost)
                    }),
            )
            .child(self.header(theme, cx))
            .child(self.filter_row(theme, cx))
            .child(self.tree(theme, cx))
            .child(self.footer(theme, cx))
            .children(self.context_menu(theme, cx))
            .into_any_element()
    }
}
