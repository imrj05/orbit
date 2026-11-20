//! Workspace picker — the new-task page's "Work in" selector.
//!
//! The same question the status bar's chip answers, asked before there is a
//! session: *where does this task run?* Two kinds of answer: this device (the
//! active workspace) and each configured remote host, plus the native dialog
//! ("Choose folder…") for anywhere else. Filterable and keyboard-driven,
//! following the popover conventions of [`crate::branch_picker::BranchPicker`].

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{
    div, point, prelude::*, px, App, Context, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, IntoElement, MouseDownEvent, ParentElement, Pixels, Render, ScrollHandle, Styled,
    Subscription, Window,
};

use crate::composer::ComposerInput;
use crate::theme::tokens::{context_menu, list, picker};
use crate::theme::{self, Theme};
use crate::ui::field::search_field;
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::prelude::{
    icon, picker_entry, picker_header, picker_row_height, working_folder_keyboard_hints,
    ControlExt, PickerRow, PressedRamp, StatefulControlExt,
};

/// The popover caps at this width — wide enough for a two-line row (name over
/// path, or a branch over its ref) and no wider, because a folder list is a
/// menu under a chip, not a page. Feeds [`width_for_window`].
pub const FIELD_MAX_W: f32 = 400.;
/// Side padding the popover keeps from the window's edges.
pub const PAGE_PAD: f32 = 24.;
/// Nominal rows visible before the list scrolls; short viewports lower the
/// cap in [`list_max_height`]. Deliberately fractional: the list ends on a cut
/// row, which is how the popover says there is more without a scrollbar.
/// Two-line rows are taller than the single-line rows this used to count, so
/// the count came down to keep the whole popover inside the window.
const VISIBLE_ROWS: f32 = 3.5;

/// Every row is a two-line picker entry — a title over a detail line (a path,
/// a hint, a branch) — so one height covers the folders, the actions, and the
/// browse row. Layout, the scroll viewport, and the keyboard reveal math all
/// read this one number, so they cannot disagree; a row that is taller than
/// the height the list assumes is exactly how text ends up overlapping the row
/// beneath it.
fn row_h(theme: &Theme) -> f32 {
    picker_row_height(theme).into()
}

/// Share of the window height the scrolling list may occupy. The centered
/// folder chip leaves roughly the lower half of the page below it; reserving
/// the rest for the search head, browse row and keyboard footer keeps the
/// popover below its trigger even at maximum UI scale and comfortable density.
const LIST_VIEWPORT_HEIGHT_SHARE: f32 = 0.21;

/// Tallest the list grows before scrolling. Large type still gets a useful
/// partial-row disclosure, but cannot turn the fixed footer into a card that
/// snaps back over the folder chip at the 960×640 minimum window.
pub fn list_max_height(theme: &Theme, viewport_height: Pixels) -> f32 {
    let row_h = row_h(theme);
    let viewport_budget =
        (f32::from(viewport_height) * LIST_VIEWPORT_HEIGHT_SHARE).max(1.5 * row_h);
    (VISIBLE_ROWS * row_h).min(viewport_budget)
}
/// Pick callback: the chosen folder plus the ambient window.
type WorkspacePick = Box<dyn Fn(PathBuf, &mut Window, &mut App)>;
/// Browse callback: open the native folder dialog.
type WorkspaceBrowse = Box<dyn Fn(&mut Window, &mut App)>;
/// Action callback: the action's index in [`PickerAction`] order.
type WorkspaceAction = Box<dyn Fn(usize, &mut Window, &mut App)>;
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type WorkspacePickerDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

/// Popover width for a given main-area width: the full width on a narrow
/// window, [`FIELD_MAX_W`] above that. Definite, not `w_full().max_w(_)`:
/// percent widths nested under a centered column resolve against the
/// unclamped page width in this gpui/taffy stack.
pub fn width_for_window(viewport_w: f32) -> f32 {
    (viewport_w - 2. * PAGE_PAD).clamp(0., FIELD_MAX_W)
}

/// What a row's leading glyph says about the place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryMark {
    /// This machine.
    Local,
    /// Any other folder.
    Folder,
}

/// One selectable place: this device's workspace, as its folder chip names it.
#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    /// Folder display name (`workspace_label`).
    pub name: String,
    pub path: PathBuf,
    /// The row's trailing note: the branch the workspace is on. `None` when
    /// there is nothing to add.
    pub note: Option<String>,
    /// Which glyph the row leads with.
    pub mark: EntryMark,
}

/// A row that is a machine rather than a folder (a remote host). Actions sit
/// below the folders, above the native-dialog row, and never take part in the
/// filter: they are always one step away.
#[derive(Debug, Clone)]
pub struct PickerAction {
    /// Stable id the app dispatches on (`remote-host`).
    pub id: &'static str,
    pub icon: &'static str,
    pub label: String,
    pub hint: Option<String>,
    /// Draw a hairline above this row: it opens a new group of machines.
    pub group_start: bool,
    /// Set the hint in the monospace family, as the folder rows set their
    /// paths: a host's `ssh:dir` is a location, not prose.
    pub mono: bool,
}

pub struct WorkspacePicker {
    entries: Vec<WorkspaceEntry>,
    /// The machine rows under the device (remote hosts, host config).
    actions: Vec<PickerAction>,
    current: Option<PathBuf>,
    /// The workspace's own mark (logo image, else the folder glyph) leading
    /// the device row — the same glyph the new-task header's folder chip
    /// shows. `None` keeps the plain folder glyph.
    device_mark: Option<Arc<gpui::Image>>,
    /// The active branch, shown as the device row's trailing note.
    branch_note: Option<String>,
    /// Matches the available chat-column width, measured with every dock
    /// excluded so the popover never overflows or juts past the folder chip.
    width: f32,
    /// Current viewport-bound list cap. Render refreshes it on resize and the
    /// keyboard reveal math reads the same value as the scroll viewport.
    list_max_h: f32,
    filter: Entity<ComposerInput>,
    scroll: ScrollHandle,
    /// `0..rows.len()` — one past the last folder row is "Choose folder…".
    highlighted: usize,
    last_filter: String,
    _filter_sub: Subscription,
    on_pick: WorkspacePick,
    on_browse: WorkspaceBrowse,
    on_action: WorkspaceAction,
    /// `bool` = dismissed by an outside mouse-down (vs. escape).
    on_dismiss: WorkspacePickerDismiss,
}

impl WorkspacePicker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        entries: Vec<WorkspaceEntry>,
        actions: Vec<PickerAction>,
        current: Option<PathBuf>,
        device_mark: Option<Arc<gpui::Image>>,
        branch_note: Option<String>,
        width: f32,
        list_max_h: f32,
        on_pick: WorkspacePick,
        on_browse: WorkspaceBrowse,
        on_action: WorkspaceAction,
        on_dismiss: WorkspacePickerDismiss,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("workspace_picker.filter_folders")
                .with_key_context("Composer Picker")
        });
        // Keep the derived filter state in sync from the input's notifications
        // so `render` only reads it.
        let filter_sub = cx.observe(&filter, |picker: &mut Self, filter, cx| {
            let needle = filter.read(cx).text().to_lowercase();
            if needle != picker.last_filter {
                picker.last_filter = needle;
                picker.highlighted = 0;
                picker.scroll.set_offset(point(px(0.), px(0.)));
                cx.notify();
            }
        });
        Self {
            entries,
            actions,
            current,
            device_mark,
            branch_note,
            width,
            list_max_h,
            filter,
            scroll: ScrollHandle::new(),
            highlighted: 0,
            last_filter: String::new(),
            _filter_sub: filter_sub,
            on_pick,
            on_browse,
            on_action,
            on_dismiss,
        }
    }

    // ── actions (Picker key context) ─────────────────────────────────────

    fn on_cancel(&mut self, _: &crate::PickerCancel, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(false, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.filter.read(cx).is_composing() {
            return;
        }
        let rows = self.filtered(&self.last_filter);
        self.activate(self.highlighted, &rows, window, cx);
    }

    fn on_next(&mut self, _: &crate::PickerSelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn on_prev(&mut self, _: &crate::PickerSelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn on_outside_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(true, window, cx);
    }

    /// Follow the chat column's width while open, so a window resize or a
    /// dock opening keeps the popover attached to its chip instead of keeping
    /// the width it opened with.
    pub fn set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if (self.width - width).abs() > 0.5 {
            self.width = width;
            cx.notify();
        }
    }

    // ── internals ────────────────────────────────────────────────────────

    /// Test seam: replace the rows, so a layout test can drive a long list
    /// without depending on the machine's hosts or session store.
    #[cfg(test)]
    pub(crate) fn set_rows(
        &mut self,
        entries: Vec<WorkspaceEntry>,
        actions: Vec<PickerAction>,
        cx: &mut Context<Self>,
    ) {
        self.entries = entries;
        self.actions = actions;
        self.highlighted = 0;
        self.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    /// Test seam: type into the filter the way a user would, so a layout test
    /// can assert what the list does when nothing matches.
    #[cfg(test)]
    pub(crate) fn set_filter_text(&mut self, text: &str, cx: &mut Context<Self>) {
        self.filter
            .update(cx, |input, cx| input.set_text(text.to_string(), cx));
    }

    /// Folder rows matching the filter (name or path substring).
    fn filtered(&self, needle: &str) -> Vec<WorkspaceEntry> {
        let needle = needle.trim().to_lowercase();
        self.entries
            .iter()
            .filter(|entry| {
                needle.is_empty()
                    || entry.name.to_lowercase().contains(&needle)
                    || entry
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&needle)
            })
            .cloned()
            .collect()
    }

    /// Selectable rows: folders, then actions, then "Choose folder…".
    fn row_count(&self, rows: &[WorkspaceEntry]) -> usize {
        rows.len() + self.actions.len() + 1
    }

    fn step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let rows = self.filtered(&self.last_filter);
        let count = self.row_count(&rows);
        if count == 0 {
            return;
        }
        let pos = self.highlighted.min(count - 1) as isize;
        let next = (pos + dir).rem_euclid(count as isize) as usize;
        self.highlighted = next;
        self.ensure_visible(rows.len(), theme::get(cx));
        cx.notify();
    }

    /// Scroll the least amount that shows the highlighted row, measured from
    /// the list's last layout rather than from a row count: the list has
    /// vertical padding and hairlines between groups, and a count-based model
    /// that leaves them out caps the scroll short of the last row.
    ///
    /// Keyboard navigation calls this; render does not. Re-revealing the
    /// highlighted row every frame fought the scroll wheel — each frame pulled
    /// the list back to the highlight, so it snapped between the folders and
    /// the verbs instead of scrolling. gpui clamps the offset to the content on
    /// layout, so a resize needs nothing from here either.
    fn ensure_visible(&mut self, folder_rows: usize, theme: &Theme) {
        let viewport = self.scroll.bounds();
        if viewport.size.height <= px(0.) {
            return; // not laid out yet; the first frame shows the top
        }
        let Some((top, bottom)) = self.highlighted_span(folder_rows, px(row_h(theme))) else {
            return;
        };
        let current = -self.scroll.offset().y;
        let mut target = current;
        if top < current {
            target = top;
        } else if bottom > current + viewport.size.height {
            target = bottom - viewport.size.height;
        }
        let max = self.scroll.max_offset().height.max(px(0.));
        self.scroll
            .set_offset(point(px(0.), -target.clamp(px(0.), max)));
    }

    /// The highlighted row's top and bottom in the list's content space (0 is
    /// the top of the scrolled content). Rows live in two groups — the folders
    /// (child 0) and the verbs (child 1) — and gpui records each group's laid
    /// out bounds, so only the offset inside a group is computed: every row is
    /// [`row_h`] tall and a verb that opens a group carries a 1px hairline.
    /// `None` for the browse row, which is outside the scrolling list.
    fn highlighted_span(&self, folder_rows: usize, row_h: Pixels) -> Option<(Pixels, Pixels)> {
        let list_top = self.scroll.bounds().top();
        let ix = self.highlighted;
        if ix < folder_rows {
            let group = self.scroll.bounds_for_item(0)?;
            let top = group.top() - list_top + row_h * ix as f32;
            return Some((top, top + row_h));
        }
        let k = ix - folder_rows;
        if k >= self.actions.len() {
            return None;
        }
        let hairline = |i: usize| i == 0 || self.actions[i].group_start;
        let before = (0..k).filter(|&i| hairline(i)).count();
        let group = self.scroll.bounds_for_item(1)?;
        let top = group.top() - list_top + row_h * k as f32 + px(before as f32);
        let bottom = top + row_h + if hairline(k) { px(1.) } else { px(0.) };
        Some((top, bottom))
    }

    fn activate(
        &mut self,
        ix: usize,
        rows: &[WorkspaceEntry],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ix < rows.len() {
            match &rows[ix] {
                entry if self.current.as_ref() == Some(&entry.path) => {
                    (self.on_dismiss)(false, window, cx)
                }
                entry => (self.on_pick)(entry.path.clone(), window, cx),
            }
            return;
        }
        let action_ix = ix - rows.len();
        if action_ix < self.actions.len() {
            (self.on_action)(action_ix, window, cx);
            return;
        }
        (self.on_browse)(window, cx);
    }
}

impl Focusable for WorkspacePicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.read(cx).focus_handle(cx)
    }
}

impl Render for WorkspacePicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::get(cx).for_floating();
        self.list_max_h = list_max_height(&theme, window.viewport_size().height);
        let this = cx.entity();

        let rows = self.filtered(&self.last_filter);
        let current_path = self.current.clone();
        // The workspace's own mark (logo image or folder glyph) leads the
        // device row, the same glyph the folder chip in the header shows.
        let device_mark = self.device_mark.clone();
        let branch_note = self.branch_note.clone();

        // ── scrolling body ───────────────────────────────────────────────
        // Folder rows, then the machine group (remote hosts) under a
        // hairline; both groups never take part in the filter's count but
        // stay put while it narrows the folders.
        let mut list = div()
            .id("workspace-picker-list")
            .debug_selector(|| "workspace-picker-list".to_string())
            .w_full()
            .max_h(px(self.list_max_h))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col();
        let mut entry_list = div()
            .debug_selector(|| "workspace-rows".to_string())
            .w_full()
            .flex()
            .flex_col();
        // Zed's no-match state: one muted two-line row where the folders
        // would be. The verbs below stay put, because "Choose folder…" is
        // exactly what a search that found nothing should still offer.
        if rows.is_empty() {
            entry_list = entry_list.child(
                picker_entry(div(), &theme)
                    .h(px(row_h(&theme)))
                    .flex_none()
                    .child(
                        PickerRow::new(
                            tr!("workspace_picker.no_folders_match"),
                            theme.text_muted,
                            theme.text_muted,
                        )
                        .icon("icons/search.svg", theme.text_muted)
                        .detail(tr!(
                            "workspace_picker.try_another_name_or_choose_a_folder_below"
                        ))
                        .weight(FontWeight::MEDIUM)
                        .into_element(&theme),
                    ),
            );
        }
        for (ix, entry) in rows.iter().enumerate() {
            let is_current = current_path.as_ref() == Some(&entry.path);
            let highlighted = ix == self.highlighted;
            let this = this.clone();
            entry_list = entry_list.child(
                picker_entry(
                    div().id(ElementId::NamedInteger("workspace-row".into(), ix as u64)),
                    &theme,
                )
                .h(px(row_h(&theme)))
                .flex_none()
                .cursor_pointer()
                .pressed_state(&theme, PressedRamp::Wash)
                .when(highlighted, |row| row.bg(theme.selected_wash()))
                // The current folder keeps the `active` fill at rest (the
                // same "chosen" register the model picker uses); the accent
                // check alone is never the only signal.
                .when(!highlighted && is_current, |row| {
                    row.bg(theme.selected_wash())
                })
                .when(!highlighted && !is_current, |row| row.hover_state(&theme))
                // Pointer movement moves the highlight; a scroll sliding rows
                // under a stationary pointer must not hijack ↑/↓ navigation.
                .on_mouse_move({
                    let this = this.clone();
                    move |_, _, cx| {
                        this.update(cx, |picker, cx| {
                            if picker.highlighted != ix {
                                picker.highlighted = ix;
                                cx.notify();
                            }
                        });
                    }
                })
                .on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(move |picker, _, window, cx| {
                        let rows = picker.filtered(&picker.last_filter.clone());
                        picker.activate(ix, &rows, window, cx);
                    }),
                )
                .child(
                    PickerRow::new(
                        entry.name.clone(),
                        if highlighted {
                            theme.text_primary
                        } else if is_current {
                            theme.selection_fg
                        } else {
                            theme.text_secondary
                        },
                        if highlighted || is_current {
                            theme.text_secondary
                        } else {
                            theme.text_muted
                        },
                    )
                    // The workspace mark (logo or folder glyph) leads the row's
                    // title line; the detail path sits under it, indented to
                    // clear the glyph.
                    .mark(
                        device_mark.clone(),
                        match entry.mark {
                            EntryMark::Local => crate::ui::prelude::EntryMarkLike::Local,
                            EntryMark::Folder => crate::ui::prelude::EntryMarkLike::Folder,
                        },
                        &theme,
                    )
                    .detail(entry.path.to_string_lossy().into_owned())
                    .mono()
                    .weight(if highlighted || is_current {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .into_element(&theme),
                )
                // The branch rides the title line, right-aligned, as a quiet
                // muted note — not a third text line.
                .when_some(entry.note.clone().or(branch_note.clone()), |row, note| {
                    row.child(
                        div()
                            .flex_none()
                            .text_size(picker::SECONDARY_TEXT.px(&theme))
                            .text_color(if highlighted || is_current {
                                theme.text_secondary
                            } else {
                                theme.text_muted
                            })
                            .child(note),
                    )
                })
                .when(is_current, |row| {
                    row.child(icon(
                        "icons/check.svg",
                        context_menu::ICON.px(&theme),
                        theme.accent_text,
                    ))
                }),
            );
        }

        list = list.child(entry_list);

        // ── machine rows (remote hosts) ──────────────────────────────────
        // Under a hairline so they never read as another folder. With no
        // hosts configured the group is omitted — host setup lives in
        // Settings → Remote hosts, not here.
        if !self.actions.is_empty() {
            let mut action_list = div()
                .debug_selector(|| "workspace-actions".to_string())
                .w_full()
                .flex()
                .flex_col();
            for (action_ix, action) in self.actions.iter().enumerate() {
                let ix = rows.len() + action_ix;
                let highlighted = self.highlighted == ix;
                let this = this.clone();
                action_list = action_list.child(
                    div()
                        .debug_selector(move || format!("workspace-action-{action_ix}"))
                        // A hairline above the group, drawn inside the first
                        // action's own box: the list's row count stays exactly
                        // what the reveal math assumes.
                        .when(action_ix == 0 || action.group_start, |row| {
                            row.border_t_1().border_color(theme.hairline())
                        })
                        .child(
                            picker_entry(
                                // Keyed by position: several rows can share an id
                                // (one `remote-host` per host), and a shared
                                // element id would share one hover/press state.
                                div().id(ElementId::NamedInteger(
                                    "workspace-action".into(),
                                    action_ix as u64,
                                )),
                                &theme,
                            )
                            .h(px(row_h(&theme)))
                            .flex_none()
                            .cursor_pointer()
                            .pressed_state(&theme, PressedRamp::Wash)
                            .when(highlighted, |row| row.bg(theme.selected_wash()))
                            .when(!highlighted, |row| row.hover_state(&theme))
                            .on_mouse_move({
                                let this = this.clone();
                                move |_, _, cx| {
                                    this.update(cx, |picker, cx| {
                                        if picker.highlighted != ix {
                                            picker.highlighted = ix;
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(move |picker, _, window, cx| {
                                    (picker.on_action)(action_ix, window, cx);
                                }),
                            )
                            .child({
                                let mut row = PickerRow::new(
                                    action.label.clone(),
                                    if highlighted {
                                        theme.text_primary
                                    } else {
                                        theme.text_secondary
                                    },
                                    theme.text_muted,
                                )
                                .icon(
                                    action.icon,
                                    if highlighted {
                                        theme.accent_text
                                    } else {
                                        theme.text_muted
                                    },
                                )
                                .weight(if highlighted {
                                    FontWeight::MEDIUM
                                } else {
                                    FontWeight::NORMAL
                                });
                                if let Some(hint) = action.hint.clone() {
                                    row = row.detail(hint);
                                }
                                if action.mono {
                                    row = row.mono();
                                }
                                row.into_element(&theme)
                            })
                            // A remote host is an experimental feature; its
                            // row says so where the machine is chosen.
                            .when(action.id == "remote-host", |entry| {
                                entry.child(crate::ui::chip::experimental_badge(&theme))
                            }),
                        ),
                );
            }

            list = list.child(action_list);
        }

        let browse_highlighted = self.highlighted == rows.len() + self.actions.len();

        popover_picker(div(), &theme)
            .debug_selector(|| "workspace-picker-popup".to_string())
            .w(px(self.width))
            .dismiss_on_outside(cx.listener(Self::on_outside_down))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            // search row — Zed's picker head.
            .child(search_field(self.filter.clone(), &theme).text_color(theme.text_primary))
            // section label — "This device" over the folder rows, with a
            // count on the right, like every other list header.
            .child(
                picker_header(tr!("workspace_picker.this_device"), &theme).when(
                    !rows.is_empty(),
                    |header| {
                        header.child(
                            div()
                                .flex_none()
                                .h(list::sub_header_height(&theme))
                                .px(list::sub_header_inset_x(&theme))
                                .flex()
                                .items_center()
                                .text_size(list::SUB_HEADER_TEXT.px(&theme))
                                .text_color(theme.text_muted)
                                .child(rows.len().to_string()),
                        )
                    },
                ),
            )
            .child(list)
            // browse row — the OS dialog, one step away, below a full-width
            // hairline like Zed's picker footer.
            .child(
                div()
                    .py(picker::list_padding_y(&theme))
                    .border_t_1()
                    .border_color(theme.hairline())
                    .child(
                        picker_entry(div().id("workspace-browse-row"), &theme)
                            .h(px(row_h(&theme)))
                            .cursor_pointer()
                            .pressed_state(&theme, PressedRamp::Wash)
                            .when(browse_highlighted, |row| row.bg(theme.selected_wash()))
                            .when(!browse_highlighted, |row| row.hover_state(&theme))
                            .on_mouse_move({
                                let this = this.clone();
                                move |_, _, cx| {
                                    this.update(cx, |picker, cx| {
                                        let browse_ix = picker.filtered(&picker.last_filter).len()
                                            + picker.actions.len();
                                        if picker.highlighted != browse_ix {
                                            picker.highlighted = browse_ix;
                                            cx.notify();
                                        }
                                    });
                                }
                            })
                            .on_mouse_up(
                                gpui::MouseButton::Left,
                                cx.listener(|picker, _, window, cx| {
                                    (picker.on_browse)(window, cx);
                                }),
                            )
                            // The browse action opens the OS dialog, so it
                            // leads with a launch glyph, not a second folder.
                            .child(icon(
                                "icons/arrow-up-right.svg",
                                context_menu::ICON.px(&theme),
                                theme.text_secondary,
                            ))
                            .child(
                                div()
                                    .flex_1()
                                    .text_color(theme.text_secondary)
                                    .child(tr!("workspace_picker.choose_folder")),
                            ),
                    ),
            )
            .child(working_folder_keyboard_hints(&theme))
    }
}
