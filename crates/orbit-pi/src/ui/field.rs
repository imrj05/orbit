//! Shared input field geometry and labels.

use crate::theme::{
    tokens::{input, TextSize},
    Theme,
};
use gpui::{div, ParentElement, SharedString, Styled};

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
