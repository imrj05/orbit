//! Keyboard keys for inline shortcut hints: one keycap, a chord of keycaps,
//! and the working-folder pickers' key legend.

use crate::theme::tokens::{picker, ButtonSize, DynamicSpacing, Radius, TextSize};
use crate::theme::Theme;
use gpui::{div, AnyElement, InteractiveElement, IntoElement, ParentElement, SharedString, Styled};

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

/// Group the platform-specific key labels of a canonical shortcut chord.
pub(crate) fn kbd_shortcut(chord: &str, theme: &Theme) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base02.px(theme))
        .children(
            chord
                .split('-')
                .filter(|part| !part.is_empty())
                .map(|part| kbd_key(crate::platform::shortcuts::label(part), theme)),
        )
        .into_any_element()
}

/// Keyboard legend shared by both working-folder pickers. The remote level
/// uses Escape to go back; elsewhere it closes the picker.
pub(crate) fn working_folder_keyboard_hints(theme: &Theme) -> gpui::Div {
    let group = |id: &'static str, keys: &[&str], label: String| {
        div()
            .debug_selector(move || id.to_string())
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base02.px(theme))
            .children(
                keys.iter()
                    .map(|key| kbd_key(crate::platform::shortcuts::label(key), theme)),
            )
            .child(div().whitespace_nowrap().child(label))
    };
    div()
        .debug_selector(|| "working-folder-keyboard-hints".to_string())
        .flex_none()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(DynamicSpacing::Base08.px(theme))
        .px(picker::search_padding_x(theme))
        .py(DynamicSpacing::Base06.px(theme))
        .border_t_1()
        .border_color(theme.hairline())
        .text_size(TextSize::XSmall.px(theme))
        .text_color(theme.text_muted)
        .child(group(
            "working-folder-hint-navigate",
            &["up", "down"],
            tr!("working_folder.hint_navigate"),
        ))
        .child(group(
            "working-folder-hint-select",
            &["enter"],
            tr!("working_folder.hint_select"),
        ))
        .child(group(
            "working-folder-hint-close",
            &["escape"],
            tr!("common.close"),
        ))
}
