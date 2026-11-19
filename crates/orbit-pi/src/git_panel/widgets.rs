//! Small, reusable Git-page widgets.
//!
//! These are the pieces every tab shares — status badges, checkboxes, buttons,
//! ref chips, the spinner, the pager, and the empty-state note. They are
//! deliberately page-agnostic so a History row, a Graph row, an Issues row, or
//! a future Pulls row can reuse them without caring which tab paints them.

use gpui::{div, prelude::*, px, AnyElement, ClickEvent, FontWeight, Hsla, Window};

use crate::git::RefKind;
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, Radius, TextSize};
use crate::theme::Theme;
use crate::ui::button::{Button, ButtonVariant, IconButton};
use crate::ui::chip::{Badge, BadgeFill};
use crate::ui::prelude::empty_state_glyph;
use crate::ui::prelude::icon;

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

/// A Medium action button with an optional leading glyph: the shared
/// [`ButtonVariant::Primary`] when `primary`, otherwise the outlined secondary
/// the Git bar uses. `disabled` renders the disabled state and drops the click.
pub fn action_button(
    id: &'static str,
    label: &str,
    leading: Option<AnyElement>,
    primary: bool,
    disabled: bool,
    theme: Theme,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    let variant = if primary {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Outline
    };
    let mut button = Button::new(id, variant)
        .label(label.to_string())
        .disabled(disabled);
    if let Some(leading) = leading {
        button = button.leading(leading);
    }
    button.on_click(&theme, listener).into_any_element()
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
    IconButton::new(gpui::ElementId::Name(id.into()), icon_path)
        .tip(label)
        .on_click(&theme, move |event, window, cx| {
            cx.stop_propagation();
            listener(event, window, cx);
        })
        .into_any_element()
}

/// A small ref badge (`main`, `origin/main`, `v1.2.0`), colored by kind.
pub fn ref_badge(name: &str, kind: RefKind, theme: Theme) -> AnyElement {
    let (fg, border) = match kind {
        RefKind::Head => (theme.accent_text, theme.accent),
        RefKind::Branch => (theme.text_secondary, theme.border_strong),
        RefKind::Remote => (theme.text_muted, theme.hairline()),
        RefKind::Tag => (theme.warning, theme.warning),
    };
    Badge::new(
        name.to_string(),
        fg,
        BadgeFill::Outline(border.opacity(0.6)),
    )
    .render(&theme)
}

/// The pager at the end of a paginated list.
pub fn load_more(
    theme: Theme,
    loading: bool,
    listener: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    Button::new("git-load-more", ButtonVariant::Secondary)
        .size(ButtonSize::Large)
        .label(if loading {
            tr!("git_panel.loading")
        } else {
            tr!("git_panel.load_more")
        })
        .loading(loading)
        .on_click(&theme, listener)
        .mx(DynamicSpacing::Base12.px(&theme))
        .mt(DynamicSpacing::Base06.px(&theme))
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
