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

use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, Context, Entity, FocusHandle, FontWeight,
    Hsla, KeyDownEvent, ListAlignment, ListState, MouseButton, MouseDownEvent, Pixels, Render,
    Subscription, Window,
};

use super::ops;
use super::tree::{self, Row, StatusBadge, TreeIndex};
use super::walk;
use crate::composer::ComposerInput;
use crate::git;
use crate::platform;
use crate::theme::tokens::{
    context_menu, input, popover, ButtonSize, DynamicSpacing, IconSize, TextSize,
};
use crate::theme::{self, Theme, ThemeMode};
use crate::ui::banner::{Banner, BannerTone};
use crate::ui::button::IconButton;
use crate::ui::confirm::ConfirmPrompt;
use crate::ui::list::virtual_list;
use crate::ui::menu::menu_hint;
use crate::ui::popover::PopoverExt;
use crate::ui::popover::{dropdown_below, DropdownAlign};
use crate::ui::prelude::{
    anchored_popover_at, context_menu_entry, context_menu_separator,
    context_menu_surface, empty_state, file_badge, file_glyph, icon, nerd_font_family, ControlExt, EmptyFill, TipExt, };
use crate::ui::resize::{resize_handle, ResizeAxis, ResizeLook};
use crate::ui::style::surface::SurfaceExt;
use crate::ui::tree::{TreeNode, TreeRow, TreeRowState};

/// Panel width defaults / drag clamps. The default matches the layout
/// presets' Explorer width.
pub const PANEL_DEFAULT_W: f32 = 280.;
/// The narrowest dock: room for a readable workspace name (about 120px)
/// beside New File, New Folder, "…" and Close, and for a nested file name
/// three levels deep. The drag clamps to it, a narrower saved width (older
/// minimums were 172 and 220) is clamped up on load, and the dock's root
/// holds it as a hard floor so no surrounding layout can squeeze it.
pub const PANEL_MIN_W: f32 = 260.;
pub const PANEL_MAX_W: f32 = 460.;

/// Fixed tree row height — 28px, the design system's navigation-row height.
const ROW_H: f32 = 28.;
/// Two-line header: the workspace name over its `~/` path.
const HEADER_H: f32 = 48.;
const FOOTER_H: f32 = 24.;

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

/// Which row the context menu targets, and where it was opened (window
/// coordinates) so the menu can be placed at the pointer.
struct PanelMenu {
    path: String,
    is_dir: bool,
    expanded: bool,
    position: gpui::Point<Pixels>,
    /// The keyboard cursor over [`menu_items`]. Painted only once the menu is
    /// driven from the keyboard (`keyed`), so a pointer-opened menu stays
    /// quiet until an arrow key is pressed.
    highlight: crate::ui::menu::MenuState,
    keyed: bool,
}

/// One action in the row context menu. The list is the single source for the
/// pointer and the keyboard: both run [`ProjectPanel::run_menu_item`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuItem {
    Toggle,
    NewFile,
    NewFolder,
    Open,
    Rename,
    Delete,
    OpenDefault,
    Reveal,
    CopyPath,
    CopyRelativePath,
}

/// The menu for a row, top to bottom; `None` is a separator.
fn menu_layout(is_dir: bool) -> Vec<Option<MenuItem>> {
    use MenuItem::*;
    let mut layout = if is_dir {
        vec![
            Some(Toggle),
            None,
            Some(NewFile),
            Some(NewFolder),
            None,
            Some(Rename),
            Some(Delete),
            None,
        ]
    } else {
        vec![Some(Open), None, Some(Rename), Some(Delete), None]
    };
    layout.extend([
        Some(OpenDefault),
        Some(Reveal),
        None,
        Some(CopyPath),
        Some(CopyRelativePath),
    ]);
    layout
}

/// The actionable entries of [`menu_layout`], in order: what the keyboard
/// cursor walks.
fn menu_items(is_dir: bool) -> Vec<MenuItem> {
    menu_layout(is_dir).into_iter().flatten().collect()
}

/// A file operation the panel asks the app to perform. Paths and names are
/// workspace-relative (the app resolves them against the active workspace and
/// runs the I/O on the background executor), so the panel itself never touches
/// the filesystem.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileOpRequest {
    NewFile { dir: String, name: String },
    NewFolder { dir: String, name: String },
    Rename { path: String, name: String },
    Delete { path: String, is_dir: bool },
}

/// The app's handler for a [`FileOpRequest`].
pub type FileOpHandler = Rc<dyn Fn(FileOpRequest, &mut App)>;

/// Which inline name prompt is open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryKind {
    NewFile,
    NewFolder,
    Rename { is_dir: bool },
}

/// A single inline name prompt — new file, new folder, or rename.
struct EntryEditor {
    kind: EntryKind,
    /// Parent directory (workspace-relative, `""` = root) a new entry lands
    /// in, or the renamed entry's directory.
    dir: String,
    /// The row path being renamed; `Some` renders the editor inside that row.
    target: Option<String>,
    input: Entity<ComposerInput>,
    /// A rejected name, shown in the notice banner until the user edits.
    error: Option<String>,
    _sub: Subscription,
}

/// A pending delete confirmation.
struct DeleteConfirm {
    path: String,
    name: String,
    is_dir: bool,
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
    focus_subscriptions: Option<(gpui::AnyWindowHandle, Subscription, Subscription)>,
    /// Workspace-relative path of the file the app is showing.
    active: Option<String>,
    /// Right-click context menu, if open.
    menu: Option<PanelMenu>,
    /// Whether the header's "…" menu (refresh, collapse all, hidden files)
    /// is open.
    header_menu_open: bool,
    /// When the header menu was dismissed by an outside click, so that same
    /// click's mouse-up on the "…" button cannot reopen it.
    /// Arms when the context menu is dismissed, so it is not repainted under
    /// the pointer by the same click.
    menu_dismissed_at: crate::ui::popover::DismissGuard,
    header_menu_dismissed_at: crate::ui::popover::DismissGuard,
    /// Inline name prompt (new file/folder or rename), if open.
    entry: Option<EntryEditor>,
    /// A pending delete confirmation, if any.
    pending_delete: Option<DeleteConfirm>,
    /// The delete prompt's two footer stops: Cancel takes the keyboard when
    /// the prompt opens, and Tab reaches the destructive one.
    delete_cancel_focus: FocusHandle,
    delete_confirm_focus: FocusHandle,
    /// A background file-op failure, shown above the tree.
    notice: Option<String>,
    /// The app's file-open callback.
    on_open: OpenHandler,
    /// The app's file-operation callback.
    on_file_op: FileOpHandler,
    /// The app's close callback (the header's X).
    on_close: Rc<dyn Fn(&mut App)>,
}

impl ProjectPanel {
    pub fn new(
        on_open: OpenHandler,
        on_file_op: FileOpHandler,
        on_close: Rc<dyn Fn(&mut App)>,
        cx: &mut Context<Self>,
    ) -> Self {
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
            width: px(crate::layout::explorer_width()
                .unwrap_or(PANEL_DEFAULT_W)
                .clamp(PANEL_MIN_W, PANEL_MAX_W)),
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
            focus: cx.focus_handle().tab_stop(true),
            focus_subscriptions: None,
            active: None,
            menu: None,
            header_menu_open: false,
            menu_dismissed_at: crate::ui::popover::DismissGuard::default(),
            header_menu_dismissed_at: crate::ui::popover::DismissGuard::default(),
            entry: None,
            pending_delete: None,
            delete_cancel_focus: cx.focus_handle().tab_stop(true),
            delete_confirm_focus: cx.focus_handle().tab_stop(true),
            notice: None,
            on_open,
            on_file_op,
            on_close,
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
            crate::layout::set_explorer_width(f32::from(width));
            cx.notify();
        }
    }

    pub fn toggle(&mut self, cx: &mut Context<Self>) {
        self.open = !self.open;
        if !self.open {
            self.menu = None;
            self.header_menu_open = false;
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
        self.entry = None;
        self.pending_delete = None;
        self.notice = None;
        self.list.reset(0);
        self.reload(cx);
    }

    /// The workspace watcher fired (or a run settled): rebuild off-thread.
    pub fn mark_stale(&mut self, cx: &mut Context<Self>) {
        if self.workspace.is_some() {
            self.stale = true;
            self.ensure_loaded(cx);
        }
    }

    /// Mark the tree stale for a change of *what* to load — a different
    /// workspace, or the hidden-files toggle — and rebuild it off-thread.
    /// Bumping the generation retires a walk already in flight, so its result
    /// is never published over the newer request. [`Self::mark_stale`] is the
    /// other case: the watcher firing while a walk runs describes the same
    /// settings, so that walk may still land and the next one corrects it.
    fn reload(&mut self, cx: &mut Context<Self>) {
        self.stale = true;
        self.generation = self.generation.wrapping_add(1);
        self.ensure_loaded(cx);
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
            let index = cx
                .background_executor()
                .spawn(async move {
                    let snapshot = walk::snapshot(&root, show_hidden);
                    // Git status is a subprocess; it must not run on the UI thread.
                    let badges = git::status_rows(&root)
                        .map(|rows| {
                            rows.into_iter()
                                .map(|row| (row.path.clone(), StatusBadge::from_status(&row)))
                                .collect()
                        })
                        .unwrap_or_default();
                    // Building the tree is the expensive half (one insert per
                    // entry, then a sort per directory), so it belongs on this
                    // side of the await too: a wide directory used to spend ~1.5 s
                    // here and freeze the window while it did.
                    TreeIndex::build(snapshot, badges)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    // Superseded while this walk ran. The request that
                    // superseded it could not start — `ensure_loaded` gates on
                    // `loading` — so hand the slot over here.
                    this.loading = false;
                    this.stale = true;
                    this.ensure_loaded(cx);
                    return;
                }
                this.index = index;
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
        self.cursor = self.cursor.filter(|index| *index < self.rows.len());
        // A rename prompt whose row vanished (an external change, a filter)
        // has nowhere to render; retire it rather than stranding the keyboard.
        if let Some(target) = self.entry.as_ref().and_then(|entry| entry.target.clone()) {
            if !self.rows.iter().any(|row| row.path == target) {
                self.entry = None;
            }
        }
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

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        // The inline prompt owns the keyboard while it is open (Enter/Escape
        // are bound to the `ExplorerEntry` context); don't also drive the tree.
        if self.entry.is_some() {
            return;
        }
        let key = event.keystroke.key.as_str();
        let platform = event.keystroke.modifiers.platform;
        // An open row menu owns the keyboard (Escape below still closes it).
        if key != "escape" && self.menu_key(key, window, cx) {
            cx.stop_propagation();
            return;
        }
        // The keyboard route to the pointer's context menu.
        if (key == "f10" && event.keystroke.modifiers.shift) || key == "menu" {
            self.open_menu_at_cursor(cx);
            cx.stop_propagation();
            return;
        }
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
                    if self.rows.get(index).is_some_and(Row::is_dir) && self.rows[index].expanded()
                    {
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

    fn open_menu(&mut self, row: Row, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        // A right-click anywhere takes focus, so retire any open prompt.
        self.entry = None;
        let is_dir = row.is_dir();
        let expanded = row.expanded();
        self.menu = Some(PanelMenu {
            path: row.path,
            is_dir,
            expanded,
            position,
            highlight: crate::ui::menu::MenuState::default(),
            keyed: false,
        });
        cx.notify();
    }

    /// Open the context menu on the cursor row from the keyboard (Shift-F10
    /// or the Menu key), under the row, with its first entry highlighted.
    fn open_menu_at_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.cursor else { return };
        let Some(row) = self.rows.get(index).cloned() else {
            return;
        };
        let bounds = self
            .list
            .bounds_for_item(index)
            .unwrap_or_else(|| self.list.viewport_bounds());
        let position = gpui::point(bounds.left() + px(24.), bounds.bottom());
        self.open_menu(row, position, cx);
        if let Some(menu) = self.menu.as_mut() {
            menu.keyed = true;
        }
    }

    /// Keys while the row menu is open. Returns whether the key was the
    /// menu's (it always is: an open menu owns the keyboard).
    fn menu_key(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.menu.as_mut() else {
            return false;
        };
        let len = menu_items(menu.is_dir).len();
        match key {
            "down" => menu.highlight.next(len),
            "up" => menu.highlight.prev(len),
            "home" => menu.highlight.first(len, |_| true),
            "end" => menu.highlight.last(len, |_| true),
            "enter" | "space" => {
                let item = menu
                    .keyed
                    .then(|| menu.highlight.selected(len))
                    .flatten()
                    .and_then(|ix| menu_items(menu.is_dir).get(ix).copied());
                if let Some(item) = item {
                    self.run_menu_item(item, window, cx);
                }
                return true;
            }
            _ => return true,
        }
        // The first arrow shows the cursor where it already is (the first
        // entry) instead of skipping past it.
        if !menu.keyed {
            menu.keyed = true;
            menu.highlight.first(len, |_| true);
        }
        cx.notify();
        true
    }

    /// Run one menu action — the same path for a click and for Enter.
    fn run_menu_item(&mut self, item: MenuItem, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.as_ref() else {
            return;
        };
        let relative = menu.path.clone();
        let is_dir = menu.is_dir;
        let target = self.menu_target_path();
        match item {
            MenuItem::Delete => {
                // The confirmation replaces the menu; it dismisses itself.
                self.request_delete(relative, is_dir, window, cx);
                return;
            }
            _ => self.dismiss_menu(cx),
        }
        match item {
            MenuItem::Toggle => self.toggle_dir(relative, cx),
            MenuItem::NewFile => self.start_new_file_in(relative, window, cx),
            MenuItem::NewFolder => self.start_new_folder_in(relative, window, cx),
            MenuItem::Open => self.open_file(relative, cx),
            MenuItem::Rename => self.start_rename(relative, is_dir, window, cx),
            MenuItem::OpenDefault => {
                if let Some(target) = target {
                    platform::open_path_default(&target);
                }
            }
            MenuItem::Reveal => {
                if let Some(target) = target {
                    platform::reveal_in_file_manager(&target);
                }
            }
            MenuItem::CopyPath => {
                if let Some(target) = target {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        target.to_string_lossy().to_string(),
                    ));
                }
            }
            MenuItem::CopyRelativePath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(relative));
            }
            MenuItem::Delete => {}
        }
    }

    fn dismiss_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            self.menu_dismissed_at.arm();
            cx.notify();
        }
    }

    fn menu_target_path(&self) -> Option<PathBuf> {
        let menu = self.menu.as_ref()?;
        let root = self.workspace.clone()?;
        Some(walk::absolute(&root, &menu.path))
    }

    // ── file operations ────────────────────────────────────────────────

    /// The directory a new entry should land in: the selected directory, the
    /// parent of the selected file, or the workspace root.
    fn selected_dir(&self) -> String {
        let Some(row) = self.cursor.and_then(|index| self.rows.get(index)) else {
            return String::new();
        };
        if row.is_dir() {
            row.path.clone()
        } else {
            parent_of(&row.path)
        }
    }

    fn start_new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.selected_dir();
        self.start_new_file_in(dir, window, cx);
    }

    fn start_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dir = self.selected_dir();
        self.start_new_folder_in(dir, window, cx);
    }

    fn start_new_file_in(&mut self, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_entry(
            EntryKind::NewFile,
            dir,
            None,
            "",
            "explorer.name_file",
            window,
            cx,
        );
    }

    fn start_new_folder_in(&mut self, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_entry(
            EntryKind::NewFolder,
            dir,
            None,
            "",
            "explorer.name_folder",
            window,
            cx,
        );
    }

    fn start_rename(
        &mut self,
        path: String,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = path.rsplit('/').next().unwrap_or(path.as_str()).to_string();
        let dir = parent_of(&path);
        self.begin_entry(
            EntryKind::Rename { is_dir },
            dir,
            Some(path.clone()),
            &name,
            "explorer.name_rename",
            window,
            cx,
        );
        if let Some(index) = self.rows.iter().position(|row| row.path == path) {
            self.list.scroll_to_reveal_item(index);
        }
    }

    /// Open an inline prompt and focus it. Replaces any prompt already open.
    #[allow(clippy::too_many_arguments)]
    fn begin_entry(
        &mut self,
        kind: EntryKind,
        dir: String,
        target: Option<String>,
        initial: &str,
        placeholder_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("explorer-entry-input")
                .with_placeholder_key(placeholder_key)
                .with_key_context("Composer ExplorerEntry")
                .with_max_lines(1)
                .with_wrap(false)
                .with_text(initial)
        });
        let sub = cx.observe(&input, |this, _, cx| {
            // Clear a rejected-name error as soon as the user edits.
            if let Some(entry) = this.entry.as_mut() {
                if entry.error.take().is_some() {
                    cx.notify();
                }
            }
        });
        self.menu = None;
        self.pending_delete = None;
        self.notice = None;
        self.entry = Some(EntryEditor {
            kind,
            dir,
            target,
            input: input.clone(),
            error: None,
            _sub: sub,
        });
        input.read(cx).focus(window);
        cx.notify();
    }

    fn commit_entry(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.entry.as_ref() else {
            return;
        };
        let raw = entry.input.read(cx).text();
        let name = match ops::validate_name(&raw) {
            Ok(name) => name,
            Err(error) => {
                let message = error.message();
                if let Some(entry) = self.entry.as_mut() {
                    entry.error = Some(message);
                }
                cx.notify();
                return;
            }
        };
        let request = match entry.kind {
            EntryKind::NewFile => FileOpRequest::NewFile {
                dir: entry.dir.clone(),
                name,
            },
            EntryKind::NewFolder => FileOpRequest::NewFolder {
                dir: entry.dir.clone(),
                name,
            },
            EntryKind::Rename { .. } => FileOpRequest::Rename {
                path: entry.target.clone().unwrap_or_default(),
                name,
            },
        };
        let handler = self.on_file_op.clone();
        self.entry = None;
        self.notice = None;
        cx.notify();
        handler(request, cx);
    }

    fn cancel_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.entry.take().is_some() {
            window.focus(&self.focus);
            cx.notify();
        }
    }

    fn on_entry_confirm(
        &mut self,
        _: &crate::ExplorerEntryConfirm,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_entry(cx);
    }

    fn on_entry_cancel(
        &mut self,
        _: &crate::ExplorerEntryCancel,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_entry(window, cx);
    }

    fn request_delete(
        &mut self,
        path: String,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = path.rsplit('/').next().unwrap_or(path.as_str()).to_string();
        self.menu = None;
        self.pending_delete = Some(DeleteConfirm { path, name, is_dir });
        // The prompt opens with the safe choice focused, so a stray Enter
        // cancels instead of deleting.
        window.focus(&self.delete_cancel_focus);
        cx.notify();
    }

    fn confirm_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.pending_delete.take() else {
            return;
        };
        let handler = self.on_file_op.clone();
        self.notice = None;
        window.focus(&self.focus);
        cx.notify();
        handler(
            FileOpRequest::Delete {
                path: target.path,
                is_dir: target.is_dir,
            },
            cx,
        );
    }

    fn cancel_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_delete.take().is_some() {
            window.focus(&self.focus);
            cx.notify();
        }
    }

    /// Show (or clear) a background file-op failure above the tree.
    pub fn set_notice(&mut self, notice: Option<String>, cx: &mut Context<Self>) {
        if self.notice != notice {
            self.notice = notice;
            cx.notify();
        }
    }
}

// ── rendering ──────────────────────────────────────────────────────────

/// The parent directory of a `/`-separated workspace path (`""` at the root).
fn parent_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(parent, _)| parent.to_string())
        .unwrap_or_default()
}

fn badge_color(badge: StatusBadge, theme: &Theme) -> Hsla {
    match badge {
        StatusBadge::Added | StatusBadge::Untracked => theme.success,
        StatusBadge::Modified => theme.warning,
        StatusBadge::Deleted => theme.error,
        StatusBadge::Renamed => theme.accent_text,
        StatusBadge::Conflicted => theme.error,
    }
}

impl ProjectPanel {
    /// The panel's header: the workspace's name over its `~/` path on the
    /// left; New File, New Folder, a "…" menu for the occasional actions
    /// (refresh, collapse all, hidden files) and the close button on the
    /// right. A spinner beside the name says a snapshot is loading.
    fn header(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let title = self
            .workspace
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| tr!("explorer.files"));
        let subtitle = self.workspace.as_deref().map(crate::skills::display_path);
        div()
            .h(px(HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .pl(DynamicSpacing::Base12.px(&theme))
            .pr(px(if self.reserve_controls {
                crate::platform::WINDOW_CONTROLS_W
            } else {
                6.
            }))
            .child(
                div()
                    .id("explorer-title")
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .when_some(
                        self.workspace
                            .as_ref()
                            .map(|path| path.to_string_lossy().into_owned()),
                        |el, path| el.tip(path),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(TextSize::Small.px(&theme))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(title),
                            )
                            .when(self.loading, |row| {
                                row.child(crate::ui::prelude::spinner(
                                    "explorer-header-loading",
                                    IconSize::XSmall.px(&theme),
                                    theme.text_muted,
                                    theme,
                                ))
                            }),
                    )
                    // The path gets its own flex row, like the title: a bare
                    // block in a column is measured at min-content, which
                    // ellipsizes the whole path down to "…".
                    .children(subtitle.map(|path| {
                        div().flex().min_w_0().child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(path),
                        )
                    })),
            )
            .child(ghost_icon(
                &theme,
                "explorer-new-file",
                "icons/file-plus.svg",
                theme.text_muted,
                tr!("explorer.new_file"),
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.start_new_file(window, cx);
                }),
            ))
            .child(ghost_icon(
                &theme,
                "explorer-new-folder",
                "icons/folder-plus.svg",
                theme.text_muted,
                tr!("explorer.new_folder"),
                cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.start_new_folder(window, cx);
                }),
            ))
            .child(self.header_menu(theme, cx))
            .child(ghost_icon(
                &theme,
                "explorer-close",
                "icons/x.svg",
                theme.text_muted,
                tr!("explorer.hide"),
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    // Close here — this listener already holds the panel's
                    // lease, so the app callback must not re-enter `update`
                    // on this entity (that would double-lease and abort).
                    let on_close = this.on_close.clone();
                    this.close(cx);
                    on_close(cx);
                }),
            ))
            .into_any_element()
    }

    /// The header's "…" button and the menu it anchors: Refresh, Collapse
    /// All Folders, and the hidden-files toggle (checked while shown).
    fn header_menu(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let theme = theme.for_floating();
        let open = self.header_menu_open;
        let button = IconButton::new("explorer-more", "icons/more.svg")
            .open(open)
            .tip(tr!("sidebar.more_actions"))
            .faded(&theme, |el| {
                el.debug_selector(|| "explorer-more".into()).on_click(
                    cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_header_menu(cx)),
                )
            });
        let popup = open.then(|| {
            let menu = context_menu_surface(div().id("explorer-header-menu"), &theme)
                .w(px(220.))
                .flex()
                .flex_col()
                .dismiss_on_outside(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    if this.header_menu_open {
                        this.header_menu_open = false;
                        this.header_menu_dismissed_at.arm();
                        cx.notify();
                    }
                }))
                .child(menu_row(
                    theme,
                    "icons/refresh.svg",
                    tr!("common.refresh"),
                    None,
                    false,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.header_menu_open = false;
                        this.mark_stale(cx);
                    }),
                ))
                .child(menu_row(
                    theme,
                    "icons/collapse-all.svg",
                    tr!("explorer.collapse_all"),
                    None,
                    false,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.header_menu_open = false;
                        this.collapse_all(cx);
                    }),
                ))
                .child(separator(theme))
                .child(menu_row(
                    theme,
                    if self.show_hidden {
                        "icons/eye-off.svg"
                    } else {
                        "icons/eye.svg"
                    },
                    if self.show_hidden {
                        tr!("explorer.hide_hidden")
                    } else {
                        tr!("explorer.show_hidden")
                    },
                    None,
                    false,
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.header_menu_open = false;
                        this.show_hidden = !this.show_hidden;
                        this.reload(cx);
                        cx.notify();
                    }),
                ));
            dropdown_below(
                DropdownAlign::End,
                popover::MENU_OFFSET,
                menu,
                "explorer-header-menu-in",
                &theme,
            )
        });
        div()
            .relative()
            .flex_none()
            .child(button)
            .children(popup)
            .into_any_element()
    }

    /// Flip the header menu. The click that dismissed it from outside also
    /// lands here on mouse-up; the stamp keeps it from reopening.
    fn toggle_header_menu(&mut self, cx: &mut Context<Self>) {
        if self.header_menu_dismissed_at.swallows() {
            return;
        }
        self.header_menu_open = !self.header_menu_open;
        if self.header_menu_open {
            self.menu = None;
        }
        cx.notify();
    }

    /// The filter: an inset, rounded field on the panel's nested surface,
    /// with a quiet hairline (no focus ring while it holds the caret).
    ///
    /// It shares the tree's edges: the field's box sits on the same 4px inset
    /// as a row's hover box, and the search glyph lands on the column the
    /// rows' chevrons start in, so the panel reads as two edges, not four.
    fn filter_row(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let has_text = !self.filter.read(cx).text().is_empty();
        div()
            .flex_none()
            .px(DynamicSpacing::Base04.px(&theme))
            .pb(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .h(ButtonSize::Medium.height(&theme))
                    .pl(DynamicSpacing::Base06.px(&theme))
                    .pr(DynamicSpacing::Base02.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .rounded(input::RADIUS.px(&theme))
                    .bg(theme.page_fill(theme.surface_secondary))
                    .border_1()
                    .border_color(theme.hairline_subtle())
                    .overflow_hidden()
                    .text_size(TextSize::Small.px(&theme))
                    .child(icon(
                        "icons/search.svg",
                        IconSize::Small.px(&theme),
                        theme.text_muted,
                    ))
                    .child(div().flex_1().min_w_0().child(self.filter.clone()))
                    .when(has_text, |row| {
                        row.child(ghost_icon(
                            &theme,
                            "explorer-filter-clear",
                            "icons/x.svg",
                            theme.text_muted,
                            tr!("view.clear_filters"),
                            cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.filter.update(cx, |input, cx| input.clear(cx));
                                this.refresh_rows(cx);
                                cx.notify();
                            }),
                        ))
                    }),
            )
            .into_any_element()
    }

    fn tree(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity().downgrade();
        let list = virtual_list(self.list.clone(), move |index, _window, cx| {
            entity
                .upgrade()
                .map(|entity| entity.update(cx, |this, cx| this.render_row(index, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .w_full()
        .py(DynamicSpacing::Base04.px(&theme));
        let no_files_detail = tr!("explorer.no_files_detail");
        let query = self.filter.read(cx).text().trim().to_string();
        let body: AnyElement = if self.loading && self.rows.is_empty() {
            loading_state(&theme)
        } else if let Some(error) = &self.error {
            empty_state(
                theme,
                "icons/stop.svg",
                &tr!("explorer.load_error"),
                Some(error.as_str()),
                EmptyFill::Full,
            )
        } else if self.rows.is_empty() && !query.is_empty() {
            // A filter that matches nothing is not an empty workspace: say
            // what was searched and how to get the tree back.
            let detail = tr!("explorer.no_matches_detail");
            div()
                .debug_selector(|| "explorer-no-matches".to_string())
                .size_full()
                .child(empty_state(
                    theme,
                    "icons/search.svg",
                    &tr!("explorer.no_matches", query = query),
                    Some(detail.as_str()),
                    EmptyFill::Full,
                ))
                .into_any_element()
        } else if self.rows.is_empty() {
            empty_state(
                theme,
                "icons/folder.svg",
                &tr!("explorer.no_files"),
                Some(no_files_detail.as_str()),
                EmptyFill::Full,
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
            .flex()
            .flex_col()
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                this.on_key_down(event, window, cx)
            }))
            .on_action(cx.listener(Self::on_entry_confirm))
            .on_action(cx.listener(Self::on_entry_cancel))
            .children(self.entry_row(theme))
            .children(self.notice_banner(theme))
            .child(div().flex_1().min_h_0().relative().child(body))
            .into_any_element()
    }

    /// The synthetic "new file / new folder" row, pinned above the tree while
    /// its prompt is open. A rename prompt renders inside its own row instead.
    fn entry_row(&self, theme: Theme) -> Option<AnyElement> {
        let entry = self.entry.as_ref()?;
        if entry.target.is_some() {
            return None;
        }
        let (icon_path, indent) = match entry.kind {
            EntryKind::NewFolder => ("icons/folder-plus.svg", 7.0),
            _ => ("icons/file-plus.svg", 23.0),
        };
        let mut row = div()
            .h(px(ROW_H))
            .flex_none()
            .mx(DynamicSpacing::Base04.px(&theme))
            .pl(px(indent))
            .pr(DynamicSpacing::Base08.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(icon(
                icon_path,
                IconSize::Small.px(&theme),
                theme.text_muted,
            ));
        if !entry.dir.is_empty() {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(format!("{}/", entry.dir)),
            );
        }
        Some(
            row.child(div().flex().flex_1().min_w_0().child(entry.input.clone()))
                .into_any_element(),
        )
    }

    /// A one-line error strip above the tree: a rejected name or a failed
    /// background operation.
    fn notice_banner(&self, theme: Theme) -> Option<AnyElement> {
        let message = self
            .entry
            .as_ref()
            .and_then(|entry| entry.error.clone())
            .or_else(|| self.notice.clone())?;
        Some(Banner::strip(BannerTone::Error, message, &theme).into_any_element())
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
            .px(DynamicSpacing::Base12.px(&theme))
            .border_t_1()
            .border_color(theme.hairline_subtle())
            .flex()
            .items_center()
            .text_size(TextSize::XSmall.px(&theme))
            .text_color(if self.index.truncated {
                theme.warning
            } else {
                theme.text_muted
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
        // A row with an open rename prompt owns the keyboard; it must not also
        // activate the file or open its context menu on click.
        let renaming = self
            .entry
            .as_ref()
            .is_some_and(|entry| entry.target.as_deref() == Some(row.path.as_str()));
        let entry_input = renaming
            .then(|| self.entry.as_ref().map(|entry| entry.input.clone()))
            .flatten();
        let state = if renaming {
            TreeRowState::Editing
        } else if active {
            TreeRowState::Selected
        } else if cursor {
            TreeRowState::Cursor
        } else {
            TreeRowState::Rest
        };
        let node = if row.is_dir() {
            TreeNode::Directory {
                expanded: row.expanded(),
            }
        } else {
            let nerd = nerd_font_family(cx);
            let dark = theme.mode == ThemeMode::Dark;
            let fallback = file_badge(&row.path, theme);
            TreeNode::File {
                glyph: file_glyph(
                    &row.path,
                    dark,
                    nerd.as_ref(),
                    IconSize::Small.px(&theme),
                    fallback,
                ),
            }
        };
        let mut tree_row = TreeRow::new(
            format!("explorer-row-{}", row.path),
            row.depth,
            node,
            row.name.clone(),
        )
        .height(px(ROW_H))
        .state(state)
        .guides(true)
        .editor(entry_input);
        if row.is_dir() && row.dirty {
            tree_row = tree_row.trailing(
                div()
                    .size(px(5.))
                    .flex_none()
                    .rounded_full()
                    .bg(theme.warning),
            );
        }
        if let (false, Some(badge)) = (row.is_dir(), row.badge) {
            tree_row = tree_row.trailing(
                div()
                    .flex_none()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(badge_color(badge, &theme))
                    .child(badge.letter().to_string()),
            );
        }
        let mut content = tree_row.render(&theme).w_full();
        if !renaming {
            // A name the dock's width ellipsizes is still reachable: the
            // row's tooltip carries its full workspace-relative path.
            content = content
                .tip(row.path.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    // Clicking away from an open prompt cancels it rather than
                    // activating the row under the pointer.
                    if this.entry.is_some() {
                        this.cancel_entry(window, cx);
                        return;
                    }
                    window.focus(&this.focus);
                    this.cursor = Some(index);
                    this.activate(index, cx);
                }))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener({
                        let row = row.clone();
                        move |this, event: &MouseDownEvent, window, cx| {
                            window.focus(&this.focus);
                            this.cursor = Some(index);
                            this.open_menu(row.clone(), event.position, cx);
                        }
                    }),
                );
        }

        // The list measures each row with a definite width, but a bare flex
        // row sizes to its content — so `flex_1` children (the name, and the
        // rename `ComposerInput`, which has no intrinsic width) would collapse.
        // A full-width wrapper gives them the row's width to fill; the 4px side
        // inset rides on this wrapper instead of a margin on the row.
        div()
            .w_full()
            .px(DynamicSpacing::Base04.px(&theme))
            .child(content)
            .into_any_element()
    }

    fn context_menu(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let menu = self.menu.as_ref()?;
        if self.menu_dismissed_at.is_recent() {
            return None;
        }
        let is_dir = menu.is_dir;
        let expanded = menu.expanded;
        let highlighted = menu
            .keyed
            .then(|| menu.highlight.selected(menu_items(is_dir).len()))
            .flatten();
        let mut popup = context_menu_surface(div().id("explorer-context-menu"), &theme)
            .w(px(220.))
            .flex()
            .flex_col()
            .dismiss_on_outside(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.dismiss_menu(cx);
            }));
        let mut index = 0;
        for entry in menu_layout(is_dir) {
            let Some(item) = entry else {
                popup = popup.child(separator(theme));
                continue;
            };
            let (icon_path, label) = match item {
                MenuItem::Toggle if expanded => {
                    ("icons/chevron-down.svg", tr!("explorer.collapse"))
                }
                MenuItem::Toggle => ("icons/chevron-right.svg", tr!("explorer.expand")),
                MenuItem::NewFile => ("icons/file-plus.svg", tr!("explorer.new_file")),
                MenuItem::NewFolder => ("icons/folder-plus.svg", tr!("explorer.new_folder")),
                MenuItem::Open => ("icons/file.svg", tr!("explorer.open")),
                MenuItem::Rename => ("icons/pencil.svg", tr!("explorer.rename")),
                MenuItem::Delete => ("icons/trash.svg", tr!("explorer.delete")),
                MenuItem::OpenDefault => ("icons/arrow-up-right.svg", tr!("explorer.open_default")),
                MenuItem::Reveal => ("icons/folder.svg", tr!("explorer.reveal")),
                MenuItem::CopyPath => ("icons/copy.svg", tr!("explorer.copy_path")),
                MenuItem::CopyRelativePath => {
                    ("icons/copy.svg", tr!("explorer.copy_relative_path"))
                }
            };
            popup = popup.child(menu_row(
                theme,
                icon_path,
                label,
                None,
                highlighted == Some(index),
                cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.run_menu_item(item, window, cx);
                }),
            ));
            index += 1;
        }
        Some(
            // Place the menu at the pointer (window coordinates), flipping it
            // above/left when it would overflow the window — the default
            // `SwitchAnchor` fit mode. Previously it was pinned to the panel's
            // top-left regardless of where the row was clicked.
            anchored_popover_at(menu.position, popup, "anchored_popover_at-in-17", &theme),
        )
    }

    /// The delete confirmation, painted over the panel. Worded from
    /// [`platform::TRASH_IS_RECOVERABLE`] so it never promises a restore the
    /// platform cannot deliver.
    fn delete_prompt(&self, theme: Theme, cx: &mut Context<Self>) -> Option<AnyElement> {
        let target = self.pending_delete.as_ref()?;
        let body_key = match (target.is_dir, platform::TRASH_IS_RECOVERABLE) {
            (true, true) => "explorer.delete_body_dir_trash",
            (true, false) => "explorer.delete_body_dir_permanent",
            (false, true) => "explorer.delete_body_file_trash",
            (false, false) => "explorer.delete_body_file_permanent",
        };
        let this = cx.entity();
        let cancel = this.clone();
        Some(
            ConfirmPrompt::new(
                "explorer-delete",
                tr!("explorer.delete_title", name = target.name.clone()),
            )
            .body(crate::i18n::translate(body_key))
            .cancel_label(tr!("explorer.cancel"))
            .confirm_label(tr!("explorer.delete"))
            .focus(&self.delete_cancel_focus, &self.delete_confirm_focus)
            .on_confirm(move |window, cx| {
                this.update(cx, |panel, cx| panel.confirm_delete(window, cx))
            })
            .on_cancel(move |window, cx| {
                cancel.update(cx, |panel, cx| panel.cancel_delete(window, cx))
            })
            .layer(&theme)
            .into_any_element(),
        )
    }
}

/// A header/toolbar ghost icon control: a Default icon button, hover fill
/// only.
fn ghost_icon(
    theme: &Theme,
    id: &'static str,
    path: &'static str,
    color: Hsla,
    label: String,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    IconButton::new(id, path)
        .ink(color)
        .tip(label)
        .faded(theme, |el| el.on_click(listener))
}

fn separator(theme: Theme) -> AnyElement {
    context_menu_separator(&theme).into_any_element()
}

/// One Explorer context-menu entry, on Zed's context menu metrics.
fn menu_row(
    theme: Theme,
    icon_path: &'static str,
    label: String,
    hint: Option<String>,
    highlighted: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    context_menu_entry(
        div().id(gpui::ElementId::Name(
            format!("explorer-menu-{label}").into(),
        )),
        &theme,
    )
    .cursor_pointer()
    // The keyboard cursor reads as the pointer's hover.
    .when(highlighted, |row| row.bg(theme.hover_wash()))
    .hover_state(&theme)
    .on_click(listener)
    .child(icon(
        icon_path,
        context_menu::ICON.px(&theme),
        theme.text_muted,
    ))
    .child(
        div()
            .min_w_0()
            .flex_1()
            .truncate()
            .text_color(theme.text_secondary)
            .child(label),
    )
    .children(hint.map(|hint| menu_hint(hint, &theme)))
    .into_any_element()
}

/// A page-level spinner; static under reduce-motion.
fn loading_state(theme: &Theme) -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(DynamicSpacing::Base08.px(theme))
        .child(crate::ui::prelude::spinner(
            "explorer-loading",
            IconSize::Small.px(theme),
            theme.text_muted,
            *theme,
        ))
        .child(
            div()
                .text_size(TextSize::Small.px(theme))
                .text_color(theme.text_muted)
                .child(tr!("explorer.loading")),
        )
        .into_any_element()
}

impl Render for ProjectPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // No `!self.open` short circuit: the app decides when the panel is in
        // the tree. While it is closing the dock slides shut over 200 ms
        // (`motion::panel::slide`), and returning nothing here would leave an
        // empty frame travelling to zero.
        if self
            .focus_subscriptions
            .as_ref()
            .is_none_or(|(handle, _, _)| *handle != window.window_handle())
        {
            self.focus_subscriptions = Some((
                window.window_handle(),
                cx.on_focus(&self.focus, window, |_, _, cx| cx.notify()),
                cx.on_blur(&self.focus, window, |_, _, cx| cx.notify()),
            ));
        }
        self.ensure_loaded(cx);
        // Rows are refreshed by the events that change them (a new index, an
        // expand/collapse, the filter observer) — re-flattening here would walk
        // the whole expanded tree on every frame.
        let theme = *theme::get(cx);
        // The dock is the row's last card, so it keeps the card gutter on
        // every open side: beside its neighbour, at the window's right edge
        // and at its bottom edge.
        let gutter = crate::theme::tokens::panel::gutter(&theme);
        div()
            .id("project-panel")
            .debug_selector(|| "project-panel".into())
            .relative()
            .flex_none()
            .w(self.width)
            .min_w(px(PANEL_MIN_W))
            .h_full()
            .pl(gutter)
            .pr(gutter)
            .pb(gutter)
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                resize_handle(
                    "explorer-resize-handle",
                    &theme,
                    ResizeAxis::Horizontal,
                    ResizeLook::Wash,
                )
                .absolute()
                .top_0()
                .bottom_0()
                // Sit on the card's left border (the gutter, minus half the
                // handle width), not the panel's outer edge, so the edge
                // users see is the edge they can grab.
                .left(gutter - px(3.))
                .w(px(6.))
                .on_drag(ExplorerResize, |_, _, _, cx| cx.new(|_| ExplorerDragGhost)),
            )
            // The dock reads as a rounded card, like the Git/Usage cards in
            // the main column. The popups stay outside this clipped shell so
            // they are not cut off at its corners.
            // Over a full-window wallpaper the card is glass, like the pages.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .bg(theme.surface)
                    .overflow_hidden()
                    .dock_card(
                        &theme,
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .child(self.header(theme, cx))
                            .child(self.filter_row(theme, cx))
                            .child(self.tree(theme, cx))
                            .child(self.footer(theme, cx)),
                    ),
            )
            .children(self.context_menu(theme, cx))
            .children(self.delete_prompt(theme, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The header's "…" menu toggles, closes with the panel, and the click
    /// that dismissed it from outside cannot reopen it on mouse-up.
    #[gpui::test]
    fn header_menu_toggles_and_guards_the_dismissing_click(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|_, app| app.set_global(theme::Theme::for_id(theme::ThemeId::Orbit)));
        let open: OpenHandler = Rc::new(|_, _, _| {});
        let op: FileOpHandler = Rc::new(|_, _| {});
        let panel = cx.update(|_, app| {
            app.new(|cx| ProjectPanel::new(open, op, Rc::new(|_: &mut App| {}), cx))
        });

        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.open = true;
                panel.toggle_header_menu(cx);
            })
        });
        assert!(cx.update(|_, app| panel.read(app).header_menu_open));

        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.header_menu_open = false;
                panel.header_menu_dismissed_at.arm();
                panel.toggle_header_menu(cx);
            })
        });
        assert!(
            !cx.update(|_, app| panel.read(app).header_menu_open),
            "the dismissing click must not reopen the menu"
        );

        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.toggle_header_menu(cx);
                panel.toggle(cx);
            })
        });
        assert!(
            !cx.update(|_, app| panel.read(app).header_menu_open),
            "closing the panel closes its menu"
        );
    }

    #[test]
    fn the_row_menu_lists_each_action_once_with_separators_between_groups() {
        for is_dir in [true, false] {
            let layout = menu_layout(is_dir);
            assert!(layout.first().unwrap().is_some(), "starts with an action");
            assert!(layout.last().unwrap().is_some(), "ends with an action");
            assert!(
                layout
                    .windows(2)
                    .all(|pair| pair[0].is_some() || pair[1].is_some()),
                "no doubled separators"
            );
            let items = menu_items(is_dir);
            let mut unique = items.clone();
            unique.dedup();
            assert_eq!(unique.len(), items.len());
        }
        assert_eq!(menu_items(true)[0], MenuItem::Toggle);
        assert_eq!(menu_items(false)[0], MenuItem::Open);
        assert!(!menu_items(false).contains(&MenuItem::NewFile));
    }

    /// The pointer's context menu has a keyboard route: Shift-F10 opens it on
    /// the cursor row, arrows walk it (wrapping), and Enter runs the same
    /// action a click would.
    #[gpui::test]
    fn the_row_menu_opens_and_runs_from_the_keyboard(cx: &mut gpui::TestAppContext) {
        let root = std::env::temp_dir().join("orbit-explorer-panel-menu-keys");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

        let cx = cx.add_empty_window();
        cx.update(|_, app| app.set_global(theme::Theme::for_id(theme::ThemeId::Orbit)));
        let open: OpenHandler = Rc::new(|_, _, _| {});
        let op: FileOpHandler = Rc::new(|_, _| {});
        let panel = cx.update(|_, app| {
            app.new(|cx| ProjectPanel::new(open, op, Rc::new(|_: &mut App| {}), cx))
        });
        let snapshot = walk::snapshot(&root, false);
        cx.update(|window, app| {
            panel.update(app, |panel, cx| {
                panel.open = true;
                panel.workspace = Some(root.clone());
                panel.index = TreeIndex::build(snapshot, HashMap::new());
                panel.stale = false;
                panel.refresh_rows(cx);
                panel.cursor = Some(0);
                panel.open_menu_at_cursor(cx);
                let menu = panel.menu.as_ref().expect("Shift-F10 opens the menu");
                assert!(menu.keyed, "a keyboard-opened menu shows its cursor");
                assert_eq!(menu.highlight.selected(menu_items(false).len()), Some(0));
                // Up from the first entry wraps to the last: Copy Relative Path.
                panel.menu_key("up", window, cx);
                panel.menu_key("enter", window, cx);
                assert!(panel.menu.is_none(), "running an action closes the menu");
            });
        });
        let copied = cx.update(|_, app| app.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("main.rs"));
    }

    #[test]
    fn parent_of_splits_workspace_paths() {
        assert_eq!(parent_of("src/main.rs"), "src");
        assert_eq!(parent_of("main.rs"), "");
        assert_eq!(parent_of("a/b/c.rs"), "a/b");
    }

    /// The dock never draws narrower than its minimum, even when a width
    /// below it reaches the panel (an old saved width, a direct assignment):
    /// the root holds `PANEL_MIN_W` as a hard floor, and `set_width` clamps.
    #[gpui::test]
    fn the_dock_holds_its_minimum_width(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|_, app| app.set_global(theme::Theme::for_id(theme::ThemeId::Orbit)));
        let panel = cx.update(|_, app| {
            app.new(|cx| {
                ProjectPanel::new(Rc::new(|_, _, _| {}), Rc::new(|_, _| {}), Rc::new(|_| {}), cx)
            })
        });
        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.open = true;
                panel.width = px(120.);
                panel.stale = false;
                cx.notify();
            })
        });
        let drawn = panel.clone();
        cx.draw(gpui::point(px(0.), px(0.)), gpui::size(px(800.), px(600.)), move |_, _| {
            drawn.clone()
        });
        let bounds = cx.debug_bounds("project-panel").expect("the dock");
        assert!(
            f32::from(bounds.size.width) >= PANEL_MIN_W,
            "the dock is {:?} wide",
            bounds.size.width
        );
        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.set_width(px(100.), cx);
                assert_eq!(panel.width(), px(PANEL_MIN_W), "a drag clamps to the floor");
            })
        });
    }

    /// A filter that matches nothing says so — it is not an empty workspace.
    #[gpui::test]
    fn a_filter_with_no_matches_names_it(cx: &mut gpui::TestAppContext) {
        let root = std::env::temp_dir().join("orbit-explorer-no-matches");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.rs"), "").unwrap();
        let cx = cx.add_empty_window();
        cx.update(|_, app| app.set_global(theme::Theme::for_id(theme::ThemeId::Orbit)));
        let panel = cx.update(|_, app| {
            app.new(|cx| {
                ProjectPanel::new(Rc::new(|_, _, _| {}), Rc::new(|_, _| {}), Rc::new(|_| {}), cx)
            })
        });
        let snapshot = walk::snapshot(&root, false);
        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.open = true;
                panel.width = px(280.);
                panel.workspace = Some(root.clone());
                panel.index = TreeIndex::build(snapshot, HashMap::new());
                panel.stale = false;
                panel.filter.update(cx, |input, cx| input.set_text("zzz-no-such-file", cx));
                panel.refresh_rows(cx);
                cx.notify();
            })
        });
        let drawn = panel.clone();
        cx.draw(gpui::point(px(0.), px(0.)), gpui::size(px(280.), px(600.)), move |_, _| {
            drawn.clone()
        });
        assert!(cx.debug_bounds("explorer-no-matches").is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    /// A `ComposerInput` has no intrinsic width, so on a content-sized row its
    /// `flex_1` collapses to zero and the inline prompt shows only the devicon.
    /// Render the real panel and assert both prompts get a usable width.
    #[gpui::test]
    fn inline_prompt_input_fills_the_row(cx: &mut gpui::TestAppContext) {
        let root = std::env::temp_dir().join("orbit-explorer-panel-prompt");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();

        let cx = cx.add_empty_window();
        cx.update(|_, app| app.set_global(theme::Theme::for_id(theme::ThemeId::Orbit)));

        let open: OpenHandler = Rc::new(|_, _, _| {});
        let op: FileOpHandler = Rc::new(|_, _| {});
        let panel = cx.update(|_, app| {
            app.new(|cx| ProjectPanel::new(open, op, Rc::new(|_: &mut App| {}), cx))
        });

        let snapshot = walk::snapshot(&root, false);
        cx.update(|window, app| {
            panel.update(app, |panel, cx| {
                panel.open = true;
                panel.width = px(280.);
                panel.workspace = Some(root.clone());
                panel.index = TreeIndex::build(snapshot, HashMap::new());
                panel.stale = false;
                panel.refresh_rows(cx);
                // Rename renders inside its row; New File renders above the
                // tree. Both wrap the same prompt input.
                panel.start_rename("main.rs".to_string(), false, window, cx);
            });
        });
        let drawn = panel.clone();
        cx.draw(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(280.), px(600.)),
            {
                let drawn = drawn.clone();
                move |_, _| drawn.clone()
            },
        );
        assert_prompt_has_width(cx, "rename");

        cx.update(|window, app| {
            panel.update(app, |panel, cx| panel.start_new_file(window, cx));
        });
        let drawn = panel.clone();
        cx.draw(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(280.), px(600.)),
            {
                let drawn = drawn.clone();
                move |_, _| drawn.clone()
            },
        );
        assert_prompt_has_width(cx, "new file");
    }

    #[gpui::test]
    fn the_tree_draws_no_focus_ring(cx: &mut gpui::TestAppContext) {
        let root = std::env::temp_dir().join("orbit-explorer-no-focus-ring");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("other.rs"), "").unwrap();

        // `draw` accumulates debug bounds, so use a fresh window for each
        // state rather than treating old bounds as the current frame.
        // Focused or not, on the open file or another: no ring, ever.
        for (focused, cursor_path, expected_ring) in [
            (true, "main.rs", false),
            (false, "main.rs", false),
            (true, "other.rs", false),
        ] {
            let visual = cx.add_empty_window();
            visual.update(|_, app| app.set_global(Theme::for_id(theme::ThemeId::Orbit)));
            let panel = visual.update(|_, app| {
                app.new(|cx| {
                    ProjectPanel::new(
                        Rc::new(|_, _, _| {}),
                        Rc::new(|_, _| {}),
                        Rc::new(|_| {}),
                        cx,
                    )
                })
            });
            let snapshot = walk::snapshot(&root, false);
            visual.update(|window, app| {
                panel.update(app, |panel, cx| {
                    panel.open = true;
                    panel.width = px(280.);
                    panel.workspace = Some(root.clone());
                    panel.index = TreeIndex::build(snapshot, HashMap::new());
                    panel.stale = false;
                    panel.refresh_rows(cx);
                    panel.active = Some("main.rs".into());
                    panel.cursor = panel.rows.iter().position(|row| row.path == cursor_path);
                    assert!(panel.cursor.is_some());
                    if focused {
                        window.focus(&panel.focus);
                    }
                    cx.notify();
                });
            });
            let drawn = panel.clone();
            visual.draw(
                gpui::point(px(0.), px(0.)),
                gpui::size(px(280.), px(600.)),
                move |_, _| drawn.clone(),
            );
            assert_eq!(
                visual.debug_bounds("tree-row-focus-ring").is_some(),
                expected_ring
            );
        }
        let _ = std::fs::remove_dir_all(root);
    }

    fn assert_prompt_has_width(cx: &mut gpui::VisualTestContext, label: &str) {
        let bounds = cx
            .debug_bounds("explorer-entry-input")
            .unwrap_or_else(|| panic!("the {label} prompt input was not rendered"));
        assert!(
            f32::from(bounds.size.width) > 40.,
            "the {label} prompt input collapsed to {}px wide",
            f32::from(bounds.size.width)
        );
    }

    /// A fresh panel over `root`, as a real caller sets it up.
    fn panel_for(cx: &mut gpui::VisualTestContext, root: &std::path::Path) -> Entity<ProjectPanel> {
        cx.update(|_, app| app.set_global(theme::Theme::for_id(theme::ThemeId::Orbit)));
        let panel = cx.update(|_, app| {
            app.new(|cx| {
                ProjectPanel::new(
                    Rc::new(|_, _, _| {}),
                    Rc::new(|_, _| {}),
                    Rc::new(|_| {}),
                    cx,
                )
            })
        });
        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel.set_workspace(Some(root.to_path_buf()), cx)
            });
        });
        panel
    }
    /// Opening a large directory froze the window because the tree was built
    /// inside the walk's `update` on the foreground executor. The build now
    /// happens in the background task, so the panel holds no index when the
    /// request returns and publishes one only after the executor is pumped.
    ///
    /// Note what this does *not* prove: GPUI's test dispatcher runs both
    /// executors on one thread, so a build moved back to the `update` would pass
    /// it. The placement is checked by reading the code; this test pins the
    /// observable contract around it (nothing published synchronously, a walk
    /// reported in flight, and the result landing complete).
    #[gpui::test]
    fn setting_a_workspace_publishes_the_tree_asynchronously(cx: &mut gpui::TestAppContext) {
        let root = std::env::temp_dir().join("orbit-explorer-off-thread");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("README.md"), "# hi\n").unwrap();

        let cx = cx.add_empty_window();
        let panel = panel_for(cx, &root);

        let entries = cx.update(|_, app| panel.read(app).index.root.children.len());
        assert_eq!(
            entries, 0,
            "the tree must not be built while setting the workspace"
        );
        assert!(
            cx.update(|_, app| panel.read(app).loading),
            "a walk was kicked off"
        );

        cx.run_until_parked();
        cx.update(|_, app| {
            let panel = panel.read(app);
            assert!(!panel.loading, "the walk settled");
            let rows = tree::visible_rows(&panel.index, &panel.expanded, "");
            assert_eq!(
                rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
                vec!["src", "README.md"]
            );
        });
        let _ = std::fs::remove_dir_all(root);
    }

    /// `render` no longer re-flattens the tree per frame, so the filter's
    /// observer is the only thing that turns typing into rows. Pin that: set the
    /// text and pump, with no `refresh_rows` call and no draw.
    #[gpui::test]
    fn the_filter_observer_refreshes_rows_without_a_render(cx: &mut gpui::TestAppContext) {
        let root = std::env::temp_dir().join("orbit-explorer-filter-observer");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(root.join("README.md"), "# hi\n").unwrap();

        let cx = cx.add_empty_window();
        let panel = panel_for(cx, &root);
        cx.run_until_parked();
        assert_eq!(cx.update(|_, app| panel.read(app).rows.len()), 2);

        cx.update(|_, app| {
            panel.update(app, |panel, cx| {
                panel
                    .filter
                    .update(cx, |input, cx| input.set_text("main", cx));
            });
        });
        assert_eq!(
            cx.update(|_, app| {
                panel
                    .read(app)
                    .rows
                    .iter()
                    .map(|row| row.path.clone())
                    .collect::<Vec<_>>()
            }),
            vec!["src", "src/main.rs"],
            "the observer alone must rebuild the rows"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    /// A result that arrives after the requested workspace changed must not be
    /// published, and must not leave the panel stuck in `loading` — the request
    /// that superseded it was refused while the walk ran.
    #[gpui::test]
    fn a_superseded_walk_never_publishes_or_sticks_loading(cx: &mut gpui::TestAppContext) {
        let old = std::env::temp_dir().join("orbit-explorer-supersede-old");
        let new = std::env::temp_dir().join("orbit-explorer-supersede-new");
        for (root, file) in [(&old, "old.rs"), (&new, "new.rs")] {
            let _ = std::fs::remove_dir_all(root);
            std::fs::create_dir_all(root).unwrap();
            std::fs::write(root.join(file), "\n").unwrap();
        }

        let cx = cx.add_empty_window();
        let panel = panel_for(cx, &old);
        // Switch before the first walk lands: its snapshot describes `old`.
        cx.update(|_, app| {
            panel.update(app, |panel, cx| panel.set_workspace(Some(new.clone()), cx));
        });

        cx.run_until_parked();
        cx.update(|_, app| {
            let panel = panel.read(app);
            assert!(!panel.loading, "a superseded walk must release the slot");
            assert_eq!(
                panel.workspace.as_deref(),
                Some(new.as_path()),
                "the panel kept the newer workspace"
            );
            let rows = tree::visible_rows(&panel.index, &panel.expanded, "");
            assert_eq!(
                rows.iter().map(|row| row.path.as_str()).collect::<Vec<_>>(),
                vec!["new.rs"],
                "the stale snapshot from the old workspace was not published"
            );
        });
        let _ = std::fs::remove_dir_all(&old);
        let _ = std::fs::remove_dir_all(&new);
    }
}
