//! Small, reusable Git-page widgets.
//!
//! These are the pieces every tab shares — status badges, checkboxes, buttons,
//! ref chips, the spinner, the pager, and the empty-state note. They are
//! deliberately page-agnostic so a History row, a Graph row, an Issues row, or
//! a future Pulls row can reuse them without caring which tab paints them.

use gpui::{
    div, prelude::*, px, AnyElement, ClickEvent, FontWeight, Hsla, Window,
};

use crate::app::{icon, press, BUTTON_GROUP};
use crate::git::RefKind;
use crate::theme::Theme;
use crate::usage::tooltip::Tooltip;

/// The color for a `git status` porcelain badge byte.
pub fn status_color(badge: char, theme: Theme) -> Hsla {
    match badge {
        'A' | '?' => theme.add_green,
        'D' => theme.del_red,
        'U' => theme.add_green,
        'R' | 'C' => theme.warn,
        _ => theme.warn,
    }
}

/// A 16px checkbox, drawn (not a native control) so it matches the theme.
pub fn check_box(checked: bool, theme: Theme) -> gpui::Div {
    div()
        .size(px(16.))
        .rounded(px(4.))
        .border_1()
        .border_color(if checked {
            theme.accent
        } else {
            theme.border_strong
        })
        .bg(if checked {
            theme.accent
        } else {
            theme.bg_composer
        })
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |box_| {
            box_.child(icon("icons/check.svg", 10., theme.send_fg))
        })
}

/// A 28px action button with an optional leading glyph. `primary` paints it in
/// the send/accent colors; `disabled` mutes it and drops the click handler.
pub fn action_button(
    id: &'static str,
    label: &str,
    leading: Option<AnyElement>,
    primary: bool,
    disabled: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let mut button = div()
        .id(id)
        .h(px(28.))
        .px(px(10.))
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(5.))
        .text_size(theme.ui_px(12.))
        .font_weight(FontWeight::MEDIUM)
        .border_1();
    if disabled {
        button = button
            .border_color(theme.border)
            .bg(theme.overlay)
            .text_color(theme.text_3);
    } else if primary {
        button = press(button)
            .group(BUTTON_GROUP)
            .border_color(gpui::transparent_black())
            .bg(theme.send_bg)
            .text_color(theme.send_fg)
            .cursor_pointer()
            .hover(|s| s.bg(theme.send_bg_hover))
            .on_click(listener);
    } else {
        button = press(button)
            .group(BUTTON_GROUP)
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .text_color(theme.text)
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover).border_color(theme.border_strong))
            .on_click(listener);
    }
    if let Some(leading) = leading {
        button = button.child(leading);
    }
    button.child(label.to_string()).into_any_element()
}

/// A 24px icon-only button for a row action, with a tooltip so the glyph is
/// never a guess. Stops propagation so clicking it never also triggers the
/// row's own click handler.
pub fn row_button(
    id: impl Into<gpui::SharedString>,
    icon_path: &'static str,
    tip_key: &'static str,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let label = tr!(tip_key);
    press(
        div()
            .id(gpui::ElementId::Name(id.into()))
            .group(BUTTON_GROUP)
            .size(px(24.))
            .rounded(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(theme.overlay)),
    )
    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into())
    .on_click(move |event, window, cx| {
        cx.stop_propagation();
        listener(event, window, cx);
    })
    .child(icon(icon_path, 12., theme.text_3))
    .into_any_element()
}

/// A small ref chip (`main`, `origin/main`, `v1.2.0`), colored by kind.
pub fn ref_badge(name: &str, kind: RefKind, theme: Theme) -> AnyElement {
    let (fg, border) = match kind {
        RefKind::Head => (theme.accent, theme.accent),
        RefKind::Branch => (theme.text_2, theme.border_strong),
        RefKind::Remote => (theme.text_3, theme.border),
        RefKind::Tag => (theme.warn, theme.warn),
    };
    div()
        .h(px(18.))
        .px(px(6.))
        .rounded(px(4.))
        .border_1()
        .border_color(border.opacity(0.6))
        .flex()
        .items_center()
        .text_size(theme.ui_px(10.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(fg)
        .child(name.to_string())
        .into_any_element()
}

/// The pager at the end of a paginated list.
pub fn load_more(
    theme: Theme,
    loading: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    press(
        div()
            .id("git-load-more")
            .group(BUTTON_GROUP)
            .mx(px(12.))
            .mt(px(6.))
            .h(px(32.))
            .rounded_md()
            .border_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_size(theme.ui_px(12.))
            .text_color(theme.text_2)
            .hover(|s| s.bg(theme.bg_hover)),
    )
    .on_click(listener)
    .child(if loading {
        tr!("git_panel.loading")
    } else {
        tr!("git_panel.load_more")
    })
    .into_any_element()
}

/// A centered empty/error state: an icon tile, a title, and optional detail.
pub fn empty_note(
    theme: Theme,
    glyph: &'static str,
    title: &str,
    detail: Option<&str>,
) -> AnyElement {
    let mut column = div()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(16.))
        .py(px(60.))
        .child(
            div()
                .size(px(32.))
                .rounded(px(8.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.bg_raised)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(glyph, 16., theme.text_2)),
        )
        .child(
            div()
                .mt(px(12.))
                .text_size(theme.ui_px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title.to_string()),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(px(6.))
                .max_w(px(320.))
                .text_align(gpui::TextAlign::Center)
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_3)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}
