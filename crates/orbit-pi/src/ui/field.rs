//! Shared input field geometry and labels.

use crate::theme::{
    tokens::{input, picker, TextSize},
    Theme,
};
use crate::ui::icon::icon;
use gpui::{div, IntoElement, ParentElement, SharedString, Styled};

/// A text field's box, Zed's `InputField`: at least 32px tall, `px_2`
/// `py_1p5`, a 1px hairline, [`input::RADIUS`], and Default-size text, which the
/// `ComposerInput` inside inherits. Callers set the fill (and a focus border)
/// and give the input a `flex_1` wrapper when an icon shares the row.
pub(crate) fn input_field_frame<E: Styled>(el: E, theme: &Theme) -> E {
    el.min_h(input::min_height(theme))
        .px(input::padding_x(theme))
        .py(input::padding_y(theme))
        .flex()
        .items_center()
        .gap(input::gap(theme))
        .rounded(input::RADIUS.px(theme))
        .border_1()
        .border_color(theme.border)
        .text_size(TextSize::Default.px(theme))
}

/// A field's label, Zed's `InputField` label: Small text.
pub(crate) fn input_label(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div().text_size(input::LABEL.px(theme)).child(label.into())
}

/// A picker's search row, Zed's picker head: 36px tall, `px_2p5`, a hairline
/// below, and Default-size input text. Put the `ComposerInput` in a
/// `flex_1` child; a leading search icon takes `IconSize::Small`.
pub(crate) fn picker_search_frame<E: Styled>(el: E, theme: &Theme) -> E {
    el.flex_none()
        .h(picker::search_height(theme))
        .px(picker::search_padding_x(theme))
        .flex()
        .items_center()
        .gap(input::gap(theme))
        .overflow_hidden()
        .border_b_1()
        .border_color(theme.border)
        .text_size(TextSize::Default.px(theme))
}

/// The search field every picker, menu and filter shares: the picker head
/// ([`picker_search_frame`]), a muted search glass, and `input` (a
/// `ComposerInput` entity, whose placeholder resolves at paint through
/// `with_placeholder_key`) filling the rest of the row. Callers add a test
/// selector, a width or an ink.
pub(crate) fn search_field(input: impl IntoElement, theme: &Theme) -> gpui::Div {
    picker_search_frame(div(), theme)
        .child(icon(
            "icons/search.svg",
            input::ICON.px(theme),
            theme.text_muted,
        ))
        .child(div().flex_1().min_w_0().child(input))
}
