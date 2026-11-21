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

/// Keyboard legend shared by the working-folder pickers, as their whole footer
/// (no note). The remote level uses Escape to go back; elsewhere it closes the
/// picker.
pub(crate) fn working_folder_keyboard_hints(theme: &Theme) -> gpui::Div {
    working_folder_footer(None, theme)
}

/// A picker's footer zone: **one** hairline, then an optional note (what the
/// choice does — "Choosing a folder starts a new task there"), then the key
/// legend. Note and legend share the header's and the rows' 10px edge, and the
/// note wraps rather than clipping a longer translation. Grouped under a single
/// rule so a note never stacks a second hairline on the legend's.
pub(crate) fn working_folder_footer(note: Option<SharedString>, theme: &Theme) -> gpui::Div {
    div()
        .debug_selector(|| "picker-footer".to_string())
        .flex_none()
        .w_full()
        .flex()
        .flex_col()
        .border_t_1()
        .border_color(theme.hairline())
        .children(note.map(|note| {
            div()
                .debug_selector(|| "picker-footer-note".to_string())
                .w_full()
                .px(picker::search_padding_x(theme))
                .pt(DynamicSpacing::Base06.px(theme))
                .whitespace_normal()
                .text_size(TextSize::XSmall.px(theme))
                .text_color(theme.text_muted)
                .child(
                    div()
                        .debug_selector(|| "picker-footer-note-text".to_string())
                        .w_full()
                        .child(note),
                )
        }))
        .child(keyboard_legend(theme))
}

fn keyboard_legend(theme: &Theme) -> gpui::Div {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::row::{picker_entry, PickerRow};
    use gpui::{point, px, Context, IntoElement, Render, TestAppContext, Window};

    /// A picker row over the shared footer, as a picker lays them out.
    struct FooterProbe;

    impl Render for FooterProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let theme = *crate::theme::get(cx);
            div()
                .w(px(300.))
                .flex()
                .flex_col()
                .child(
                    picker_entry(div(), &theme).child(
                        PickerRow::new("Row", theme.text_primary, theme.text_muted)
                            .icon("icons/folder.svg", theme.text_muted)
                            .into_element(&theme),
                    ),
                )
                .child(working_folder_footer(
                    Some("Choosing a folder starts a new task there.".into()),
                    &theme,
                ))
        }
    }

    /// The footer's note starts on the rows' edge — the glyph's — instead of
    /// the 4px inset a vertical padding token once gave it.
    #[gpui::test]
    fn the_footer_note_shares_the_rows_edge(cx: &mut TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeId::Orbit))
        });
        let (view, cx) = cx.add_window_view(|_, _| FooterProbe);
        cx.draw(point(px(0.), px(0.)), gpui::size(px(400.), px(300.)), |_, _| {
            view.clone()
        });
        let glyph = cx.debug_bounds("picker-row-leading").expect("row glyph");
        let text = cx
            .debug_bounds("picker-footer-note-text")
            .expect("note text");
        assert!(
            (text.left() - glyph.left()).abs() <= px(1.),
            "the note's text starts on the rows' edge: {text:?} vs {glyph:?}"
        );
    }
}
