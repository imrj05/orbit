//! Rewind picker — choose the user message to fork the conversation before.
//!
//! A modal list, not an anchored popover: a rewind can be started from a
//! transcript row anywhere on the page or from the command palette, and neither
//! has a chip to hang a popover under. Rows are pi's own fork points
//! (`get_fork_messages`), so what the list shows is exactly what `fork` accepts.
//!
//! Nothing here is destructive: pi writes the branch to a **new** session file
//! and leaves the open one on disk, so the footer says so instead of asking for
//! a confirmation nobody needs.

use gpui::{
    deferred, div, point, prelude::*, px, AnyElement, App, Context, ElementId, Entity, FocusHandle,
    Focusable, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Subscription, Window,
};

use crate::app::{
    icon, menu_header, picker_entry, picker_search_frame, picker_surface,
    working_folder_keyboard_hints, ControlExt,
};
use crate::composer::ComposerInput;
use crate::rewind::{self, ForkPoint};
use crate::theme::tokens::{input, list, picker, DynamicSpacing, TextSize};
use crate::theme::{self, Theme};

const CARD_W: f32 = 460.;
/// Rows visible before the list scrolls.
const VISIBLE_ROWS: f32 = 8.;

fn row_h(theme: &Theme) -> f32 {
    picker::entry_height(theme).into()
}

fn row_gap(theme: &Theme) -> f32 {
    DynamicSpacing::Base01.px(theme).into()
}

fn row_stride(theme: &Theme) -> f32 {
    row_h(theme) + row_gap(theme)
}

fn list_max_h(theme: &Theme) -> f32 {
    VISIBLE_ROWS * row_stride(theme)
}

/// The chosen entry id plus the ambient window.
type RewindAction = Box<dyn Fn(String, &mut Window, &mut App)>;
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type RewindDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

/// One drawn row: the preview line and the ordinal pi's list implies.
struct Row {
    entry_id: String,
    preview: SharedString,
    ordinal: SharedString,
}

impl Row {
    fn new(ix: usize, point: &ForkPoint) -> Self {
        let preview = rewind::preview(&point.text);
        Self {
            entry_id: point.entry_id.clone(),
            // An all-whitespace prompt has no preview line; say what it is
            // rather than drawing an empty row a click could still pick.
            preview: if preview.is_empty() {
                tr!("rewind.empty_message").into()
            } else {
                preview.into()
            },
            ordinal: format!("#{}", ix + 1).into(),
        }
    }
}

pub struct RewindPicker {
    rows: Vec<Row>,
    loading: bool,
    filter: Entity<ComposerInput>,
    last_filter: String,
    highlighted: usize,
    list_scroll: ScrollHandle,
    _filter_sub: Subscription,
    on_pick: RewindAction,
    on_dismiss: RewindDismiss,
}

impl RewindPicker {
    /// Open the picker before pi has answered: the list arrives in
    /// [`Self::set_points`], and until then one quiet row says so.
    pub fn new(on_pick: RewindAction, on_dismiss: RewindDismiss, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("rewind.search")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
        });
        let filter_sub = cx.observe(&filter, |picker: &mut Self, filter, cx| {
            let needle = filter.read(cx).text().to_lowercase();
            if needle != picker.last_filter {
                picker.last_filter = needle;
                picker.highlighted = 0;
                picker.list_scroll.set_offset(point(px(0.), px(0.)));
                cx.notify();
            }
        });
        Self {
            rows: Vec::new(),
            loading: true,
            filter,
            last_filter: String::new(),
            highlighted: 0,
            list_scroll: ScrollHandle::new(),
            _filter_sub: filter_sub,
            on_pick,
            on_dismiss,
        }
    }

    fn rows(points: &[ForkPoint]) -> Vec<Row> {
        points
            .iter()
            .enumerate()
            .map(|(ix, point)| Row::new(ix, point))
            .collect()
    }

    /// Refill the list once `get_fork_messages` answers an open picker.
    ///
    /// The highlight follows an entry id rather than an index: rows can arrive
    /// in a different order and the reader's selection must not silently become
    /// a different message. `preselect` wins when the async answer finally
    /// names the turn the reader clicked.
    pub fn set_points(
        &mut self,
        points: Vec<ForkPoint>,
        preselect: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;
        let anchor = preselect.or_else(|| {
            self.filtered(&self.last_filter)
                .get(self.highlighted)
                .map(|row| row.entry_id.clone())
        });
        self.rows = Self::rows(&points);
        let filtered = self.filtered(&self.last_filter);
        let count = filtered.len();
        self.highlighted = match anchor {
            Some(entry) => filtered
                .iter()
                .position(|row| row.entry_id == entry)
                .unwrap_or(usize::MAX),
            None => filtered.len().saturating_sub(1),
        };
        // The list has not been laid out yet when the RPC reply arrives, so
        // ScrollHandle::scroll_to_item cannot use its viewport bounds here.
        // Position the fixed-height row at the top; GPUI clamps at the end.
        let offset = if self.highlighted < count {
            self.highlighted as f32 * row_stride(theme::get(cx))
        } else {
            0.
        };
        self.list_scroll.set_offset(point(px(0.), px(-offset)));
        cx.notify();
    }

    /// An ambiguous transcript-to-entry match must not default to a
    /// different prompt. Enter is inert until the user selects a row.
    pub(crate) fn clear_highlight(&mut self, cx: &mut Context<Self>) {
        self.highlighted = usize::MAX;
        self.list_scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    fn filtered(&self, needle: &str) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|row| needle.is_empty() || row.preview.to_lowercase().contains(needle))
            .collect()
    }

    fn on_cancel(&mut self, _: &crate::PickerCancel, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(false, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Enter confirms an IME candidate before it picks a message: a
        // half-composed query must not fork the conversation (issue #100).
        if self.filter.read(cx).is_composing() {
            return;
        }
        let entry = self
            .filtered(&self.last_filter)
            .get(self.highlighted)
            .map(|row| row.entry_id.clone());
        if let Some(entry) = entry {
            (self.on_pick)(entry, window, cx);
        }
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

    fn step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let count = self.filtered(&self.last_filter).len();
        if count == 0 {
            return;
        }
        let next = if self.highlighted >= count {
            if dir > 0 {
                0
            } else {
                count - 1
            }
        } else if dir > 0 {
            (self.highlighted + 1).min(count - 1)
        } else {
            self.highlighted.saturating_sub(1)
        };
        self.highlighted = next;
        self.list_scroll.scroll_to_item(next);
        cx.notify();
    }

    /// The message the keyboard would fork before (tests).
    #[cfg(test)]
    pub(crate) fn highlighted_entry(&self) -> Option<String> {
        self.filtered(&self.last_filter)
            .get(self.highlighted)
            .map(|row| row.entry_id.clone())
    }

    /// Move the highlight without a key event (tests).
    #[cfg(test)]
    pub(crate) fn step_for_test(&mut self, dir: isize, cx: &mut Context<Self>) {
        self.step(dir, cx);
    }

    /// Confirm the highlighted row without a key event (tests).
    #[cfg(test)]
    pub(crate) fn confirm_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.on_confirm(&crate::PickerConfirm, window, cx);
    }

    fn render_row(&self, ix: usize, row: &Row, theme: Theme, this: &Entity<Self>) -> AnyElement {
        let highlighted = ix == self.highlighted;
        let entry = row.entry_id.clone();
        let this_hover = this.clone();
        let this_click = this.clone();
        picker_entry(
            div().id(ElementId::NamedInteger("rewind-row".into(), ix as u64)),
            &theme,
        )
        .debug_selector(move || format!("rewind-row-{ix}"))
        .h(px(row_h(&theme)))
        .flex_none()
        .cursor_pointer()
        .when(highlighted, |row| row.bg(theme.surface_selected))
        .when(!highlighted, |row| row.hover_state(&theme))
        // Pointer movement moves the highlight; a scroll sliding a row under a
        // stationary pointer must not hijack ↑/↓ navigation.
        .on_mouse_move(move |_, _, cx| {
            this_hover.update(cx, |picker, cx| {
                if picker.highlighted != ix {
                    picker.highlighted = ix;
                    cx.notify();
                }
            });
        })
        .on_click(move |_, window, cx| {
            this_click.update(cx, |picker, cx| {
                picker.highlighted = ix;
                (picker.on_pick)(entry.clone(), window, cx);
            });
        })
        .child(
            div()
                .flex_none()
                .text_size(picker::SECONDARY_TEXT.px(&theme))
                .text_color(if highlighted {
                    theme.text_secondary
                } else {
                    theme.text_muted
                })
                .child(row.ordinal.clone()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_weight(if highlighted {
                    FontWeight::MEDIUM
                } else {
                    FontWeight::NORMAL
                })
                .text_color(if highlighted {
                    theme.text_primary
                } else {
                    theme.text_secondary
                })
                .child(row.preview.clone()),
        )
        .into_any_element()
    }
}

impl Focusable for RewindPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.read(cx).focus_handle(cx)
    }
}

impl Render for RewindPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *theme::get(cx);
        let this = cx.entity();
        let rows = self.filtered(&self.last_filter);

        let mut list = div()
            .id("rewind-list")
            .debug_selector(|| "rewind-list".to_string())
            .w_full()
            .max_h(px(list_max_h(&theme)))
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(px(row_gap(&theme)));
        for (ix, row) in rows.iter().enumerate() {
            list = list.child(self.render_row(ix, row, theme, &this));
        }

        // The picker family's own surface, not the heavier modal card an
        // extension dialog wears: this is a list, beside the command palette.
        let card = picker_surface(div(), &theme)
            .id("rewind-card")
            .debug_selector(|| "rewind-card".to_string())
            .w(px(CARD_W))
            .max_w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            // Clicks inside the card must not reach the scrim's dismiss.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .child(
                picker_search_frame(div(), &theme)
                    .child(icon(
                        "icons/search.svg",
                        input::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(div().flex_1().min_w_0().child(self.filter.clone())),
            )
            .child(
                menu_header(tr!("rewind.title"), &theme)
                    .pt(picker::list_padding_y(&theme))
                    .when(!rows.is_empty(), |header| {
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
                    }),
            )
            .when(
                !self.loading && self.highlighted == usize::MAX && !rows.is_empty(),
                |card| {
                    card.child(
                        div()
                            .id("rewind-choose-message")
                            .debug_selector(|| "rewind-choose-message".to_string())
                            .px(picker::search_padding_x(&theme))
                            .pb(DynamicSpacing::Base02.px(&theme))
                            .whitespace_normal()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!("rewind.choose_message")),
                    )
                },
            )
            .child(if self.loading {
                div()
                    .py(picker::list_padding_y(&theme))
                    .child(
                        picker_entry(div().id("rewind-loading"), &theme)
                            .text_color(theme.text_muted)
                            .child(tr!("rewind.loading")),
                    )
                    .into_any_element()
            } else if rows.is_empty() {
                div()
                    .py(picker::list_padding_y(&theme))
                    .child(
                        picker_entry(div().id("rewind-empty"), &theme)
                            .text_color(theme.text_muted)
                            .child(tr!("rewind.empty")),
                    )
                    .into_any_element()
            } else {
                list.into_any_element()
            })
            // What a rewind does, stated once where the choice is made: a new
            // session is branched off, and this one is not thrown away.
            .child(
                div()
                    .px(picker::search_padding_x(&theme))
                    .py(DynamicSpacing::Base06.px(&theme))
                    .border_t_1()
                    .border_color(theme.border)
                    .flex_none()
                    .w_full()
                    .whitespace_normal()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("rewind.note")),
            )
            .child(working_folder_keyboard_hints(&theme, false));

        div()
            .id("rewind-layer")
            .debug_selector(|| "rewind-layer".to_string())
            .absolute()
            .inset_0()
            .occlude()
            .bg(theme.scrim())
            .px(DynamicSpacing::Base24.px(&theme))
            .py(DynamicSpacing::Base24.px(&theme))
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down_out(cx.listener(Self::on_outside_down))
            .child(card)
    }
}

/// The picker is a window-scale modal: above every chip popover and the
/// palette's own deferred surface, below a blocking extension dialog.
pub fn layer(picker: Entity<RewindPicker>) -> impl IntoElement {
    deferred(picker).with_priority(8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, ThemeId};
    use std::cell::RefCell;
    use std::rc::Rc;

    fn points() -> Vec<ForkPoint> {
        vec![
            ForkPoint {
                entry_id: "e1".into(),
                text: "fix the failing test".into(),
            },
            ForkPoint {
                entry_id: "e2".into(),
                text: "now rename the helper".into(),
            },
            ForkPoint {
                entry_id: "e3".into(),
                text: "  ".into(),
            },
        ]
    }

    /// A picker that records what it was asked to fork, filled the way the app
    /// fills it: opened first, given pi's answer second.
    fn picker(points: Vec<ForkPoint>, cx: &mut Context<RewindPicker>) -> RewindPicker {
        let mut picker = RewindPicker::new(
            Box::new(|_: String, _: &mut Window, _: &mut App| {}),
            Box::new(|_: bool, _: &mut Window, _: &mut App| {}),
            cx,
        );
        picker.set_points(points, None, cx);
        picker
    }

    #[test]
    fn rows_carry_the_first_line_of_each_prompt() {
        let rows: Vec<Row> = points()
            .iter()
            .enumerate()
            .map(|(ix, point)| Row::new(ix, point))
            .collect();
        assert_eq!(rows[0].preview.as_ref(), "fix the failing test");
        assert_eq!(rows[0].ordinal.as_ref(), "#1");
        assert_eq!(
            rows[2].preview.as_ref(),
            "(no text)",
            "an empty prompt still draws a row a click can name"
        );
    }

    #[gpui::test]
    fn the_newest_message_is_chosen_first(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let entity = cx.new(|cx| picker(points(), cx));
            let picker = entity.read(cx);
            let rows = picker.filtered("");
            assert_eq!(rows.len(), 3);
            assert_eq!(
                rows[picker.highlighted].entry_id, "e3",
                "the newest prompt is where a rewind usually starts"
            );
            let _ = window;
        });
    }

    #[gpui::test]
    fn filtering_matches_the_prompt_text(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        cx.update(|_, cx| {
            let entity = cx.new(|cx| picker(points(), cx));
            entity.update(cx, |picker, _| {
                let rows = picker.filtered("rename");
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].entry_id, "e2");
                assert!(picker.filtered("nothing like this").is_empty());
            });
        });
    }

    #[gpui::test]
    fn the_keyboard_walks_the_list_without_wrapping_past_the_ends(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let entity = cx.new(|cx| picker(points(), cx));
            entity.update(cx, |picker, cx| {
                assert_eq!(picker.highlighted, 2);
                picker.step_for_test(1, cx);
                assert_eq!(picker.highlighted, 2, "the last row is the end");
                picker.step_for_test(-1, cx);
                picker.step_for_test(-1, cx);
                assert_eq!(picker.highlighted, 0);
                picker.step_for_test(-1, cx);
                assert_eq!(picker.highlighted, 0, "the first row is the start");
            });
            entity.update(cx, |picker, cx| {
                picker.step_for_test(1, cx);
                picker.confirm_for_test(window, cx);
            });
        });
    }

    #[gpui::test]
    fn ambiguous_message_requires_an_explicit_selection(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let picks = Rc::new(RefCell::new(Vec::new()));
            let recorded = picks.clone();
            let entity = cx.new(|cx| {
                RewindPicker::new(
                    Box::new(move |entry, _, _| recorded.borrow_mut().push(entry)),
                    Box::new(|_, _, _| {}),
                    cx,
                )
            });
            entity.update(cx, |picker, cx| {
                picker.set_points(points(), None, cx);
                picker.clear_highlight(cx);
                picker.confirm_for_test(window, cx);
                assert!(
                    picks.borrow().is_empty(),
                    "Enter must not fork the wrong prompt"
                );
                picker.step_for_test(1, cx);
                picker.confirm_for_test(window, cx);
                assert_eq!(&*picks.borrow(), &["e1".to_string()]);
            });
        });
    }

    #[gpui::test]
    fn confirming_forks_the_highlighted_row(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let picks: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
            let recorded = picks.clone();
            let entity = cx.new(|cx| {
                let on_pick = Box::new(move |entry: String, _: &mut Window, _: &mut App| {
                    recorded.borrow_mut().push(entry);
                }) as Box<dyn Fn(String, &mut Window, &mut App)>;
                let on_dismiss = Box::new(|_: bool, _: &mut Window, _: &mut App| {})
                    as Box<dyn Fn(bool, &mut Window, &mut App)>;
                RewindPicker::new(on_pick, on_dismiss, cx)
            });
            entity.update(cx, |picker, cx| picker.set_points(points(), None, cx));
            entity.update(cx, |picker, cx| {
                // From the newest row to the first, then pick it.
                picker.step_for_test(-1, cx);
                picker.step_for_test(-1, cx);
                picker.confirm_for_test(window, cx);
            });
            assert_eq!(*picks.borrow(), vec!["e1".to_string()]);
        });
    }

    #[gpui::test]
    fn a_loading_picker_says_so_instead_of_showing_an_empty_list(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        cx.update(|_, cx| {
            let entity = cx.new(|cx| {
                RewindPicker::new(
                    Box::new(|_: String, _: &mut Window, _: &mut App| {}),
                    Box::new(|_: bool, _: &mut Window, _: &mut App| {}),
                    cx,
                )
            });
            assert!(entity.read(cx).loading);
            entity.update(cx, |picker, cx| {
                picker.set_points(points(), Some("e2".into()), cx);
                assert!(!picker.loading);
                assert_eq!(
                    picker.highlighted_entry().as_deref(),
                    Some("e2"),
                    "the deferred preselect lands with the list"
                );
            });
        });
    }

    #[gpui::test]
    fn preselected_message_is_visible_in_a_long_list(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let (entity, cx) = cx.add_window_view(|_, cx| {
            RewindPicker::new(Box::new(|_, _, _| {}), Box::new(|_, _, _| {}), cx)
        });
        let viewport = cx.update(|window, _| window.viewport_size());
        let draw = |cx: &mut gpui::VisualTestContext| {
            let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| entity.clone());
        };
        draw(cx); // The picker first appears while pi is loading.
        let points = (0..17)
            .map(|ix| ForkPoint {
                entry_id: format!("e{ix}"),
                text: format!("prompt {ix}"),
            })
            .collect();
        cx.update(|_, cx| {
            entity.update(cx, |picker, cx| {
                picker.set_points(points, Some("e15".into()), cx)
            })
        });
        draw(cx);
        draw(cx);
        cx.update(|_, cx| assert_eq!(entity.read(cx).highlighted_entry().as_deref(), Some("e15")));
        let list = cx.debug_bounds("rewind-list").unwrap();
        let selected = cx.debug_bounds("rewind-row-15").unwrap();
        assert!(
            selected.top() >= list.top() && selected.bottom() <= list.bottom(),
            "selected {selected:?} outside list {list:?}"
        );
        for _ in 0..15 {
            cx.update(|_, cx| entity.update(cx, |picker, cx| picker.step_for_test(-1, cx)));
            draw(cx);
        }
        let first = cx.debug_bounds("rewind-row-0").unwrap();
        let list = cx.debug_bounds("rewind-list").unwrap();
        assert!(first.top() >= list.top() && first.bottom() <= list.bottom());
        cx.update(|_, cx| entity.update(cx, |picker, cx| picker.clear_highlight(cx)));
        draw(cx);
        assert!(cx.debug_bounds("rewind-choose-message").is_some());
        cx.update(|_, cx| entity.update(cx, |picker, cx| picker.step_for_test(1, cx)));
        cx.update(|_, cx| assert_eq!(entity.read(cx).highlighted_entry().as_deref(), Some("e0")));
    }

    #[gpui::test]
    fn the_picker_draws_its_card_and_list(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let viewport = cx.update(|window, _| window.viewport_size());
        let entity = cx.update(|_, cx| cx.new(|cx| picker(points(), cx)));
        let opened = entity.clone();
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| opened);
        assert!(
            cx.debug_bounds("rewind-card").is_some(),
            "the card draws inside the scrim"
        );
        assert!(
            cx.debug_bounds("rewind-list").is_some(),
            "the fork points draw as a list"
        );
        assert!(
            cx.debug_bounds("rewind-layer").is_some(),
            "the modal layer covers the window"
        );
    }
}
