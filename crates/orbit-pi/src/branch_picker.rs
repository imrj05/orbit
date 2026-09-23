//! Git branch picker — search, checkout, and create-from-current-state.
//!
//! Opened from the status-bar branch chip; follows the same popover conventions
//! as [`crate::command_palette::CommandPalette`].

use gpui::{
    div, point, prelude::*, px, App, Context, ElementId, Entity, FocusHandle, Focusable,
    FontWeight, IntoElement, MouseDownEvent, ParentElement, Render, ScrollHandle, Styled, Window,
};

use crate::app::{icon, PopoverSurface};
use crate::composer::ComposerInput;
use crate::theme;

const POPOVER_W: f32 = 280.;
const ROW_H: f32 = 30.;
const ROW_GAP: f32 = 1.;
const ROW_STRIDE: f32 = ROW_H + ROW_GAP;
const LIST_MAX_H: f32 = 7. * ROW_STRIDE;

/// Checkout/create callback: the chosen branch plus the ambient window.
type BranchAction = Box<dyn Fn(String, &mut Window, &mut App)>;
/// Dismiss callback; `bool` is true when an outside mouse-down closed it.
type BranchPickerDismiss = Box<dyn Fn(bool, &mut Window, &mut App)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Browse,
    Create,
}

pub struct BranchPicker {
    branches: Vec<String>,
    current: String,
    filter: Entity<ComposerInput>,
    create_input: Entity<ComposerInput>,
    mode: Mode,
    list_scroll: ScrollHandle,
    highlighted: usize,
    last_filter: String,
    on_checkout: BranchAction,
    on_create: BranchAction,
    on_dismiss: BranchPickerDismiss,
}

impl BranchPicker {
    pub fn new(
        workspace_label: String,
        branches: Vec<String>,
        current: String,
        on_checkout: BranchAction,
        on_create: BranchAction,
        on_dismiss: BranchPickerDismiss,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("branch_picker.search_branches")
                .with_placeholder_var("workspace", workspace_label)
                .with_key_context("Composer Picker")
        });
        let create_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_placeholder_key("branch_picker.new_branch_name")
                .with_key_context("Composer Picker")
        });
        Self {
            branches,
            current,
            filter,
            create_input,
            mode: Mode::Browse,
            list_scroll: ScrollHandle::new(),
            highlighted: 0,
            last_filter: String::new(),
            on_checkout,
            on_create,
            on_dismiss,
        }
    }

    fn on_cancel(&mut self, _: &crate::PickerCancel, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == Mode::Create {
            self.mode = Mode::Browse;
            self.highlighted = 0;
            window.focus(&self.filter.read(cx).focus_handle(cx));
            cx.notify();
            return;
        }
        (self.on_dismiss)(false, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode == Mode::Create {
            let name = self.create_input.read(cx).text().trim().to_string();
            if !name.is_empty() {
                (self.on_create)(name, window, cx);
            }
            return;
        }
        let rows = self.filtered_branches(&self.last_filter);
        if let Some(branch) = rows.get(self.highlighted).cloned() {
            if branch != self.current {
                (self.on_checkout)(branch, window, cx);
            } else {
                (self.on_dismiss)(false, window, cx);
            }
        }
    }

    fn on_next(&mut self, _: &crate::PickerSelectNext, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Browse {
            return;
        }
        self.step(1, cx);
    }

    fn on_prev(&mut self, _: &crate::PickerSelectPrev, _: &mut Window, cx: &mut Context<Self>) {
        if self.mode != Mode::Browse {
            return;
        }
        self.step(-1, cx);
    }

    fn on_outside_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        (self.on_dismiss)(true, window, cx);
    }

    fn filtered_branches(&self, needle: &str) -> Vec<String> {
        self.branches
            .iter()
            .filter(|branch| needle.is_empty() || branch.to_lowercase().contains(needle))
            .cloned()
            .collect()
    }

    fn step(&mut self, dir: isize, cx: &mut Context<Self>) {
        let rows = self.filtered_branches(&self.last_filter);
        if rows.is_empty() {
            return;
        }
        let pos = self.highlighted.min(rows.len() - 1);
        let next = if dir > 0 {
            (pos + 1).min(rows.len() - 1)
        } else {
            pos.saturating_sub(1)
        };
        self.highlighted = next;
        let row_top = next as f32 * ROW_STRIDE;
        let current: f32 = self.list_scroll.offset().y.into();
        let n = rows.len() as f32;
        let content_h = (n * ROW_H + (n - 1.).max(0.) * ROW_GAP).max(0.);
        let viewport_h = content_h.min(LIST_MAX_H);
        let mut offset = current;
        if row_top < current {
            offset = row_top;
        } else if row_top + ROW_H > current + viewport_h {
            offset = row_top + ROW_H - viewport_h;
        }
        let max_offset = (content_h - viewport_h).max(0.);
        self.list_scroll
            .set_offset(point(px(0.), px(offset.clamp(0., max_offset))));
        cx.notify();
    }

    fn begin_create(
        &mut self,
        _: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.mode = Mode::Create;
        self.create_input.update(cx, |input, cx| {
            input.clear(cx);
        });
        window.focus(&self.create_input.read(cx).focus_handle(cx));
        cx.notify();
    }
}

impl Focusable for BranchPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.mode {
            Mode::Browse => self.filter.read(cx).focus_handle(cx),
            Mode::Create => self.create_input.read(cx).focus_handle(cx),
        }
    }
}

impl Render for BranchPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *theme::get(cx);
        let this = cx.entity();

        if self.mode == Mode::Create {
            return div()
                .w(px(POPOVER_W))
                .font_family(theme::ui_font_family())
                .pt(px(6.))
                .pb(px(6.))
                .rounded(px(10.))
                .popover_surface(theme)
                .flex()
                .flex_col()
                .overflow_hidden()
                .occlude()
                .on_mouse_down_out(cx.listener(Self::on_outside_down))
                .on_action(cx.listener(Self::on_cancel))
                .on_action(cx.listener(Self::on_confirm))
                .child(
                    div()
                        .px(px(12.))
                        .pt(px(4.))
                        .pb(px(8.))
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(
                            div()
                                .text_size(theme.ui_px(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(tr!("branch_picker.create_and_checkout_new_branch")),
                        )
                        .child(
                            div()
                                .text_size(theme.ui_px(11.5))
                                .text_color(theme.text_3)
                                .child(tr!("branch_picker.uncommitted_changes_come_with_you")),
                        )
                        .child(self.create_input.clone()),
                )
                .child(
                    div()
                        .mx(px(8.))
                        .mb(px(4.))
                        .h(px(28.))
                        .rounded(px(6.))
                        .bg(theme.send_bg)
                        .hover(|s| s.bg(theme.send_bg_hover))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(theme.ui_px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.send_fg)
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|picker, _, window, cx| {
                                let name = picker.create_input.read(cx).text().trim().to_string();
                                if !name.is_empty() {
                                    (picker.on_create)(name, window, cx);
                                }
                            }),
                        )
                        .child(tr!("branch_picker.create_branch")),
                );
        }

        let needle = self.filter.read(cx).text().to_lowercase();
        if needle != self.last_filter {
            self.last_filter = needle.clone();
            self.highlighted = 0;
            self.list_scroll.set_offset(point(px(0.), px(0.)));
        }
        let rows = self.filtered_branches(&needle);
        if !rows.is_empty() {
            self.highlighted = self.highlighted.min(rows.len() - 1);
        }

        let mut list = div()
            .id("branch-picker-list")
            .w_full()
            .max_h(px(LIST_MAX_H))
            .overflow_y_scroll()
            .track_scroll(&self.list_scroll)
            .px(px(4.))
            .py(px(2.))
            .flex()
            .flex_col()
            .gap(px(ROW_GAP));
        for (ix, branch) in rows.iter().enumerate() {
            let selected = *branch == self.current;
            let highlighted = ix == self.highlighted;
            let branch_label = branch.clone();
            let branch_compare = branch.clone();
            let branch_action = branch.clone();
            let this = this.clone();
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("branch-row".into(), ix as u64))
                    .h(px(ROW_H))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(selected || highlighted, |row| row.bg(theme.active))
                    .when(!selected && !highlighted, |row| {
                        row.hover(|s| s.bg(theme.overlay))
                    })
                    .on_hover({
                        let this = this.clone();
                        move |hovering, _, cx| {
                            if *hovering {
                                this.update(cx, |picker, cx| {
                                    if picker.highlighted != ix {
                                        picker.highlighted = ix;
                                        cx.notify();
                                    }
                                });
                            }
                        }
                    })
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(move |picker, _, window, cx| {
                            if branch_compare == picker.current {
                                (picker.on_dismiss)(false, window, cx);
                            } else {
                                (picker.on_checkout)(branch_action.clone(), window, cx);
                            }
                        }),
                    )
                    .child(icon("icons/branch.svg", 12., theme.text_3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(theme.ui_px(12.))
                            .text_color(if selected {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(branch_label),
                    )
                    .when(selected, |row| {
                        row.child(icon("icons/check.svg", 11., theme.accent))
                    }),
            );
        }

        div()
            .w(px(POPOVER_W))
            .font_family(theme::ui_font_family())
            .pt(px(6.))
            .pb(px(4.))
            .rounded(px(10.))
            .popover_surface(theme)
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(Self::on_outside_down))
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .child(
                div()
                    .h(px(34.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_size(theme.ui_px(12.5))
                    .child(icon("icons/search.svg", 13., theme.text_3))
                    .child(self.filter.clone()),
            )
            .child(
                div()
                    .px(px(12.))
                    .pt(px(8.))
                    .pb(px(4.))
                    .text_size(theme.ui_px(10.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_3)
                    .child(tr!("branch_picker.branches")),
            )
            .child(if rows.is_empty() {
                div()
                    .h(px(56.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(theme.ui_px(12.))
                    .text_color(theme.text_3)
                    .child(tr!("branch_picker.no_branches_match"))
                    .into_any_element()
            } else {
                list.into_any_element()
            })
            .child(
                div()
                    .mx(px(4.))
                    .mt(px(4.))
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .h(px(32.))
                            .mx(px(4.))
                            .my(px(4.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.overlay))
                            .on_mouse_up(gpui::MouseButton::Left, cx.listener(Self::begin_create))
                            .child(icon("icons/plus.svg", 12., theme.text_2))
                            .child(
                                div()
                                    .text_size(theme.ui_px(12.))
                                    .text_color(theme.text_2)
                                    .child(tr!("branch_picker.create_and_checkout_new_branch_2")),
                            ),
                    ),
            )
    }
}
