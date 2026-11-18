//! Compact keyboard key for inline shortcut hints.

use crate::theme::tokens::{ButtonSize, DynamicSpacing, Radius, TextSize};
use crate::theme::Theme;
use gpui::{div, AnyElement, IntoElement, ParentElement, SharedString, Styled};

/// Render a key label supplied by the caller.
pub(crate) fn kbd_key(label: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div()
        .h(ButtonSize::Compact.height(theme))
        .min_w(ButtonSize::Compact.height(theme))
        .px(DynamicSpacing::Base04.px(theme))
        .rounded(Radius::Small.px(theme))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.overlay_strong)
        .text_size(TextSize::XSmall.px(theme))
        .text_color(theme.text_secondary)
        .child(label.into())
        .into_any_element()
}
