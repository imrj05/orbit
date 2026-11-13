//! Small, reusable Git-page widgets.
//!
//! These are the pieces every tab shares — status badges, checkboxes, buttons,
//! ref chips, the spinner, the pager, and the empty-state note. They are
//! deliberately page-agnostic so a History row, a Graph row, an Issues row, or
//! a future Pulls row can reuse them without caring which tab paints them.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, FontWeight, Hsla, Window};

use crate::app::helpers::empty_state_glyph;
use crate::app::{
    button_frame, icon, icon_button_frame, press, ControlExt, PressedRamp, StatefulControlExt,
    BUTTON_GROUP,
};
use crate::git::RefKind;
use crate::theme::tokens::RaisedExt;
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, Radius, TextSize};
use crate::theme::Theme;
use crate::usage::tooltip::Tooltip;

/// The color for a `git status` porcelain badge byte.
pub fn status_color(badge: char, theme: Theme) -> Hsla {
    match badge {
        'A' | '?' => theme.success,
        'D' => theme.error,
        'U' => theme.success,
        'R' | 'C' => theme.warning,
        _ => theme.warning,
    }
}

/// A 16px checkbox, drawn (not a native control) so it matches the theme.
pub fn check_box(checked: bool, theme: Theme) -> gpui::Div {
    div()
        .size(px(16.))
        .rounded(Radius::Small.px(&theme))
        .border_1()
        .border_color(if checked {
            theme.accent
        } else {
            theme.border_strong
        })
        .bg(if checked {
            theme.accent
        } else {
            theme.surface_secondary
        })
        .flex()
        .items_center()
        .justify_center()
        .when(checked, |box_| {
            box_.child(icon(
                "icons/check.svg",
                IconSize::Indicator.px(&theme),
                theme.accent_fg,
            ))
        })
}

/// A Medium action button with an optional leading glyph. `primary` paints it
/// in the send/accent colors; `disabled` mutes it and drops the click handler.
pub fn action_button(
    id: &'static str,
    label: &str,
    leading: Option<AnyElement>,
    primary: bool,
    disabled: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let mut button = button_frame(div().id(id), &theme, ButtonSize::Medium)
        .font_weight(FontWeight::MEDIUM)
        .border_1();
    if disabled {
        button = button
            .border_color(theme.border)
            .bg(theme.overlay)
            .text_color(theme.text_disabled);
    } else if primary {
        // Accent buttons press by color (the theme's pressed step), not the
        // generic opacity dim, so the primary action keeps a real edge.
        button = button
            .group(BUTTON_GROUP)
            .border_color(gpui::transparent_black())
            .raised(theme.action, &theme)
            .text_color(theme.action_fg)
            .cursor_pointer()
            .hover_action(&theme)
            .pressed_state(&theme, PressedRamp::Action)
            .on_click(listener);
    } else {
        button = press(button)
            .group(BUTTON_GROUP)
            .border_color(theme.border)
            .raised(theme.surface_elevated, &theme)
            .text_color(theme.text_primary)
            .cursor_pointer()
            .hover_bordered(&theme)
            .on_click(listener);
    }
    if let Some(leading) = leading {
        button = button.child(leading);
    }
    button.child(label.to_string()).into_any_element()
}

/// An icon-only row-action button (`ButtonSize::Default`), with a tooltip so
/// the glyph is never a guess. Stops propagation so clicking it never also triggers the
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
        icon_button_frame(
            div().id(gpui::ElementId::Name(id.into())),
            &theme,
            ButtonSize::Default,
        )
        .group(BUTTON_GROUP)
        .cursor_pointer()
        .hover_state(&theme),
    )
    .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into())
    .on_click(move |event, window, cx| {
        cx.stop_propagation();
        listener(event, window, cx);
    })
    .child(icon(
        icon_path,
        ButtonSize::Default.icon_size().px(&theme),
        theme.text_muted,
    ))
    .into_any_element()
}

/// A small ref chip (`main`, `origin/main`, `v1.2.0`), colored by kind.
pub fn ref_badge(name: &str, kind: RefKind, theme: Theme) -> AnyElement {
    let (fg, border) = match kind {
        RefKind::Head => (theme.accent_text, theme.accent),
        RefKind::Branch => (theme.text_secondary, theme.border_strong),
        RefKind::Remote => (theme.text_muted, theme.border),
        RefKind::Tag => (theme.warning, theme.warning),
    };
    div()
        .h(px(18.))
        .px(DynamicSpacing::Base06.px(&theme))
        .rounded(Radius::Small.px(&theme))
        .border_1()
        .border_color(border.opacity(0.6))
        .flex()
        .items_center()
        .text_size(TextSize::XSmall.px(&theme))
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
        button_frame(div().id("git-load-more"), &theme, ButtonSize::Large)
            .group(BUTTON_GROUP)
            .mx(DynamicSpacing::Base12.px(&theme))
            .mt(DynamicSpacing::Base06.px(&theme))
            .border_1()
            .border_color(theme.border)
            .cursor_pointer()
            .text_color(theme.text_secondary)
            .hover_state(&theme),
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
        .px(DynamicSpacing::Base16.px(&theme))
        .py(px(60.))
        .child(empty_state_glyph(theme, glyph))
        .child(
            div()
                .mt(DynamicSpacing::Base16.px(&theme))
                .text_size(TextSize::Default.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(title.to_string()),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(DynamicSpacing::Base06.px(&theme))
                .max_w(px(360.))
                .text_align(gpui::TextAlign::Center)
                .text_size(TextSize::Small.px(&theme))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_muted)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}

/// One line of text that ellipsizes at the width it is painted at.
///
/// GPUI's text element truncates when it is first measured and keeps that
/// result for the frame, and flex layout can measure a `min_w_0` descendant
/// at zero width before it settles the pane. A truncated title then paints as
/// a lone `…`. This element takes its parent's width and the line height, and
/// shapes the text only in prepaint, when its final bounds are known. Font,
/// size, weight and color come from the inherited text style.
pub struct EllipsizedLine {
    text: gpui::SharedString,
    painted: std::rc::Rc<std::cell::RefCell<gpui::SharedString>>,
}

impl EllipsizedLine {
    pub fn new(text: impl Into<gpui::SharedString>) -> Self {
        Self {
            text: text.into(),
            painted: Default::default(),
        }
    }

    /// The text as last painted, ellipsis included.
    #[cfg(test)]
    pub fn painted(&self) -> std::rc::Rc<std::cell::RefCell<gpui::SharedString>> {
        self.painted.clone()
    }
}

impl IntoElement for EllipsizedLine {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl gpui::Element for EllipsizedLine {
    type RequestLayoutState = ();
    type PrepaintState = Option<gpui::ShapedLine>;

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) -> (gpui::LayoutId, ()) {
        let mut style = gpui::Style::default();
        style.size.width = gpui::relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut gpui::App,
    ) -> Self::PrepaintState {
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let mut runs = vec![style.to_run(self.text.len())];
        let display = window
            .text_system()
            .line_wrapper(style.font(), font_size)
            .truncate_line(self.text.clone(), bounds.size.width, "\u{2026}", &mut runs);
        *self.painted.borrow_mut() = display.clone();
        Some(
            window
                .text_system()
                .shape_line(display, font_size, &runs, None),
        )
    }

    fn paint(
        &mut self,
        _: Option<&gpui::GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: gpui::Bounds<gpui::Pixels>,
        _: &mut (),
        line: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        let Some(line) = line else { return };
        let line_height = window.line_height();
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            line.paint(bounds.origin, line_height, window, cx).ok();
        });
    }
}
