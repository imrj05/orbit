//! Workspace picker — the new-task page's folder selector.
//!
//! One question only: *which folder?* It lists the recent folders (the
//! current one first, then session folders newest-first, then the sidebar's
//! projects) and the native dialog ("Choose folder…") for anywhere else.
//! *Where* the task runs — a worktree or a remote host — is the "Work in"
//! picker's question, so none of those rows appear here; mixing the two made
//! one control change the other's answer. Filterable and keyboard-driven,
//! following the popover conventions of [`crate::branch_picker::BranchPicker`].

use std::path::PathBuf;

use gpui::{
    div, point, prelude::*, px, App, Context, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, IntoElement, MouseDownEvent, ParentElement, Pixels, Render, ScrollHandle, Styled,
    Subscription, Window,
};

use crate::composer::ComposerInput;
use crate::theme::tokens::{context_menu, picker};
use crate::theme::{self, Theme};
use crate::ui::field::search_field;
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::prelude::{
    icon, picker_entry, picker_header, picker_row_height, working_folder_keyboard_hints,
    ControlExt, PickerRow, PressedRamp, StatefulControlExt,
};
use crate::ui::row::picker_title_slot;

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

/// Every row is a two-line picker entry — a folder name over its path — so
/// one height covers the folders and the browse row. Layout, the scroll viewport, and the keyboard reveal math all
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
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type WorkspacePickerDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

/// Popover width for a given main-area width: the full width on a narrow
/// window, [`FIELD_MAX_W`] above that. Definite, not `w_full().max_w(_)`:
/// percent widths nested under a centered column resolve against the
/// unclamped page width in this gpui/taffy stack.
pub fn width_for_window(viewport_w: f32) -> f32 {
    (viewport_w - 2. * PAGE_PAD).clamp(0., FIELD_MAX_W)
}

/// One selectable folder, as its folder chip names it.
#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    /// Folder display name (`workspace_label`).
    pub name: String,
    pub path: PathBuf,
    /// The row's trailing note: when a session last ran there. `None` when
    /// there is nothing to add; the current folder shows its branch instead.
    pub note: Option<String>,
}

pub struct WorkspacePicker {
    entries: Vec<WorkspaceEntry>,
    current: Option<PathBuf>,
    /// The active branch, shown as the current folder's trailing note.
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
    /// `bool` = dismissed by an outside mouse-down (vs. escape).
    on_dismiss: WorkspacePickerDismiss,
}

impl WorkspacePicker {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        entries: Vec<WorkspaceEntry>,
        current: Option<PathBuf>,
        branch_note: Option<String>,
        width: f32,
        list_max_h: f32,
        on_pick: WorkspacePick,
        on_browse: WorkspaceBrowse,
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
            current,
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
    /// without depending on the machine's session store.
    #[cfg(test)]
    pub(crate) fn set_rows(&mut self, entries: Vec<WorkspaceEntry>, cx: &mut Context<Self>) {
        self.entries = entries;
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

    /// Selectable rows: folders, then "Choose folder…".
    fn row_count(&self, rows: &[WorkspaceEntry]) -> usize {
        rows.len() + 1
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
    /// vertical padding, and a count-based model that leaves it out caps the
    /// scroll short of the last row.
    ///
    /// Keyboard navigation calls this; render does not. Re-revealing the
    /// highlighted row every frame fought the scroll wheel — each frame pulled
    /// the list back to the highlight, so it snapped back instead of
    /// scrolling. gpui clamps the offset to the content on
    /// layout, so a resize needs nothing from here either.
    fn ensure_visible(&mut self, folder_rows: usize, theme: &Theme) {
        let viewport = self.scroll.bounds();
        if viewport.size.height <= px(0.) {
            return; // not laid out yet; the first frame shows the top
        }
        let Some((top, bottom)) = self.highlighted_span(folder_rows, px(row_h(theme)), theme) else {
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
    /// the top of the scrolled content). gpui records the folder group's laid
    /// out bounds, so only the offset inside it is computed: every row is
    /// [`row_h`] tall with [`picker::row_gap`] between rows. `None` for the
    /// browse row, which is outside the scrolling list.
    fn highlighted_span(
        &self,
        folder_rows: usize,
        row_h: Pixels,
        theme: &Theme,
    ) -> Option<(Pixels, Pixels)> {
        let ix = self.highlighted;
        if ix >= folder_rows {
            return None;
        }
        let list_top = self.scroll.bounds().top();
        let group = self.scroll.bounds_for_item(0)?;
        let gaps = picker::row_gap(theme) * ix as f32;
        let top = group.top() - list_top + row_h * ix as f32 + gaps;
        Some((top, top + row_h))
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
        let branch_note = self.branch_note.clone();

        // ── scrolling body ───────────────────────────────────────────────
        // Folder rows only; the filter narrows them by name or path.
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
            .flex_col()
            .gap(picker::row_gap(&theme));
        // Zed's no-match state: one muted two-line row where the folders
        // would be. "Choose folder…" stays put below, because it is exactly
        // what a search that found nothing should still offer.
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
            // The current folder carries its active branch; any other folder
            // says when a session last ran there.
            let note = if is_current {
                branch_note.clone().or(entry.note.clone())
            } else {
                entry.note.clone()
            };
            let row_body = PickerRow::new(
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
            );
            // One icon family down the list: the stroke folder glyph. The
            // workspace logo stays on the header's folder chip — at a row's
            // 14px a raster wordmark reads as noise beside it.
            let row_body = row_body.mark(None, crate::ui::prelude::EntryMarkLike::Folder, &theme);
            // `~/…` for paths under home, as the Work in picker writes them;
            // the filter still matches the full path, and hovering shows it.
            let row_body = row_body
                .detail(crate::skills::display_path(&entry.path))
                .mono()
                .weight(if highlighted || is_current {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .into_element(&theme);
            entry_list = entry_list.child(
                div()
                    .debug_selector(move || format!("workspace-entry-{ix}"))
                    .child(
                picker_entry(
                    div().id(ElementId::NamedInteger("workspace-row".into(), ix as u64)),
                    &theme,
                )
                .h(px(row_h(&theme)))
                .flex_none()
                .cursor_pointer()
                .pressed_state(&theme, PressedRamp::Wash)
                // Only the highlight fills a row. The current folder is said
                // by its check, its Medium weight and its brighter ink — never
                // by the check alone, and never by a second filled slab.
                .when(highlighted, |row| row.bg(theme.selected_wash()))
                .when(!highlighted, |row| row.hover_state(&theme))
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
                // Children from the top, so the trailing slot shares the
                // title's line instead of centring between title and path.
                .items_start()
                .child(row_body)
                // The branch and the check ride the title line, right-aligned:
                // a quiet muted note, never a third text line.
                .when(note.is_some() || is_current, |row| {
                    row.child(
                        picker_title_slot(&theme)
                            .debug_selector(move || format!("workspace-entry-slot-{ix}"))
                            .children(note.map(|note| {
                                div()
                                    .flex_none()
                                    .text_size(picker::SECONDARY_TEXT.px(&theme))
                                    .text_color(if highlighted || is_current {
                                        theme.text_secondary
                                    } else {
                                        theme.text_muted
                                    })
                                    .child(note)
                            }))
                            .when(is_current, |slot| {
                                slot.child(icon(
                                    "icons/check.svg",
                                    context_menu::ICON.px(&theme),
                                    theme.accent_text,
                                ))
                            }),
                    )
                }),
                    ),
            );
        }

        list = list.child(entry_list);

        let browse_highlighted = self.highlighted == rows.len();

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
            // section label — "Recent folders" over the rows. No count: the
            // rows are right below it.
            .child(picker_header(tr!("working_folder.recent"), &theme))
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
                                        let browse_ix =
                                            picker.filtered(&picker.last_filter).len();
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
