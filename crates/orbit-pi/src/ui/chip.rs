//! Chips and badges: compact labels. A [`Badge`] reports a state and never
//! acts; a [`ChipButton`] is a trigger that opens a picker.
//!
//! A badge reports; it never acts. Anything a user presses is a
//! [`crate::ui::button::Button`] (or a chip trigger), so a decorative label
//! can never be mistaken for a control. The tone is the caller's: ref kinds,
//! provider states and file types own their colours, which the design system
//! allows for content that carries meaning in its hue — the badge owns only
//! the geometry and type.

use gpui::{
    div, AnyElement, App, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement, Pixels, SharedString, Styled, Window,
};

use crate::motion::Hover;
use crate::theme::{
    tokens::{ButtonSize, DynamicSpacing, IconSize, Radius, RaisedExt, TextSize},
    Theme,
};
use crate::ui::button::button_frame;
use crate::ui::icon::{dropdown_chevron, icon};
use crate::ui::style::control::{ControlExt, TipExt};

/// How a badge carries its tone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BadgeFill {
    /// A tinted face behind the ink (provider state, plan tier).
    Solid(Hsla),
    /// A hairline in the tone, no face (git refs).
    Outline(Hsla),
}

/// A one-line label at Compact height, `Radius::Small`, XSmall medium ink.
pub(crate) struct Badge {
    label: SharedString,
    ink: Hsla,
    fill: BadgeFill,
}

impl Badge {
    pub(crate) fn new(label: impl Into<SharedString>, ink: Hsla, fill: BadgeFill) -> Self {
        Self {
            label: label.into(),
            ink,
            fill,
        }
    }

    pub(crate) fn render(self, theme: &Theme) -> AnyElement {
        let label = self.label.clone();
        self.frame(theme).child(label).into_any_element()
    }

    fn frame(self, theme: &Theme) -> gpui::Div {
        let el = div()
            .flex_none()
            .h(ButtonSize::Compact.height(theme))
            .px(DynamicSpacing::Base06.px(theme))
            .rounded(Radius::Small.px(theme))
            .flex()
            .items_center()
            .whitespace_nowrap()
            .text_size(TextSize::XSmall.px(theme))
            .font_weight(FontWeight::MEDIUM)
            .text_color(self.ink);
        match self.fill {
            BadgeFill::Solid(face) => el.bg(face),
            BadgeFill::Outline(edge) => el.border_1().border_color(edge),
        }
    }
}

/// What a trigger chip looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChipStyle {
    /// No frame of its own: sits in a toolbar (the composer's access,
    /// workflow, model and thinking chips). The open fill and the hover wash
    /// fade on `Motion::Interaction`; the caret turns.
    Quiet,
    /// A hairline and a raised face (Usage's filters). `inset` drops the face
    /// to the canvas so it reads as a well inside a card.
    Outlined { inset: bool },
}

/// A trigger chip: an optional glyph, an optional label and a caret, opening
/// a picker. `open` is whether that picker is showing.
///
/// The chip owns its look in every state; the caller owns the label (localized),
/// the picker, and the handler, which [`ChipButton::on_mouse_up`] /
/// [`ChipButton::on_mouse_down`] attach.
pub(crate) struct ChipButton {
    id: SharedString,
    style: ChipStyle,
    size: ButtonSize,
    icon: Option<&'static str>,
    label: Option<SharedString>,
    label_max_w: Option<Pixels>,
    open: bool,
    caret: bool,
    tip: Option<SharedString>,
    selector: Option<&'static str>,
    trailing: Option<AnyElement>,
}

impl ChipButton {
    pub(crate) fn new(id: impl Into<SharedString>, style: ChipStyle) -> Self {
        Self {
            id: id.into(),
            style,
            size: match style {
                ChipStyle::Quiet => ButtonSize::Default,
                ChipStyle::Outlined { .. } => ButtonSize::Medium,
            },
            icon: None,
            label: None,
            label_max_w: None,
            open: false,
            caret: true,
            tip: None,
            selector: None,
            trailing: None,
        }
    }

    /// A glyph after the label, before the caret — a lock on a read-only mode.
    pub(crate) fn trailing(mut self, trailing: impl IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    pub(crate) fn icon(mut self, path: &'static str) -> Self {
        self.icon = Some(path);
        self
    }

    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Truncate the label past this width (a long model or folder name).
    pub(crate) fn label_max_w(mut self, width: Pixels) -> Self {
        self.label_max_w = Some(width);
        self
    }

    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Hide the caret — an icon-only chip whose tooltip names it.
    pub(crate) fn caret(mut self, caret: bool) -> Self {
        self.caret = caret;
        self
    }

    pub(crate) fn tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tip = Some(tip.into());
        self
    }

    /// A test hook (`debug_selector`); the label gets `<selector>-label`.
    pub(crate) fn selector(mut self, selector: &'static str) -> Self {
        self.selector = Some(selector);
        self
    }

    fn frame(&self, theme: &Theme) -> gpui::Stateful<gpui::Div> {
        let open = self.open;
        let mut el = button_frame(div().id(self.id.clone()), theme, self.size).cursor_pointer();
        if let Some(selector) = self.selector {
            el = el.debug_selector(move || selector.to_string());
        }
        el = match self.style {
            ChipStyle::Quiet => el.when_open(open, |el| {
                el.bg(theme.surface_active).text_color(theme.text_primary)
            }),
            ChipStyle::Outlined { inset } => el
                .border_1()
                .border_color(if open {
                    theme.border_strong
                } else {
                    theme.border
                })
                .raised(
                    if open {
                        theme.surface_active
                    } else if inset {
                        theme.background
                    } else {
                        theme.surface_elevated
                    },
                    theme,
                )
                .hover_state_open(theme, open),
        };
        if let Some(tip) = self.tip.clone() {
            el = el.tip(tip);
        }
        el
    }

    fn content(self, el: gpui::Stateful<gpui::Div>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
        let open = self.open;
        let quiet = self.style == ChipStyle::Quiet;
        let icon_ink = if open && !quiet {
            theme.text_primary
        } else {
            theme.text_muted
        };
        let label_ink = if open {
            theme.text_primary
        } else {
            theme.text_secondary
        };
        let caret_ink = if open {
            theme.text_primary
        } else {
            theme.text_muted
        };
        let caret_id = SharedString::from(format!("{}-caret-turn", self.id));
        el.children(
            self.icon
                .map(|path| icon(path, self.size.icon_size().px(theme), icon_ink)),
        )
        .children(self.label.map(|label| {
            let mut text = div().text_color(label_ink);
            if let Some(selector) = self.selector {
                text = text.debug_selector(move || format!("{selector}-label"));
            }
            let text = match self.label_max_w {
                Some(width) => text.max_w(width).truncate(),
                None => text,
            };
            text.child(label)
        }))
        .children(self.trailing)
        .children(self.caret.then(|| {
            if quiet {
                dropdown_chevron(caret_id, open, IconSize::XSmall.px(theme), caret_ink, theme)
            } else {
                icon("icons/chevron-down.svg", IconSize::XSmall.px(theme), caret_ink)
                    .into_any_element()
            }
        }))
    }

    fn finish(self, el: gpui::Stateful<gpui::Div>, theme: &Theme) -> AnyElement {
        let quiet = self.style == ChipStyle::Quiet;
        let open = self.open;
        let name = self.id.clone();
        let el = self.content(el, theme);
        if quiet {
            crate::motion::hover(
                el,
                name,
                theme,
                Hover::over(open.then_some(theme.surface_active), theme.hover_wash()),
            )
        } else {
            el.into_any_element()
        }
    }

    pub(crate) fn on_mouse_up(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let el = self.frame(theme).on_mouse_up(MouseButton::Left, handler);
        self.finish(el, theme)
    }

    pub(crate) fn on_mouse_down(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let el = self.frame(theme).on_mouse_down(MouseButton::Left, handler);
        self.finish(el, theme)
    }
}

/// `when` for a bool, without pulling `FluentBuilder` into every caller.
trait WhenOpen: Sized {
    fn when_open(self, open: bool, f: impl FnOnce(Self) -> Self) -> Self {
        if open {
            f(self)
        } else {
            self
        }
    }
}

impl<T> WhenOpen for T {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn solid_and_outline_badges_share_one_geometry_in_every_palette() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let mut outline =
                Badge::new("main", theme.accent_text, BadgeFill::Outline(theme.accent))
                    .frame(&theme);
            let mut solid =
                Badge::new("Pro", theme.action_fg, BadgeFill::Solid(theme.action)).frame(&theme);
            let (outline, solid) = (outline.style(), solid.style());
            assert_eq!(outline.size.height, solid.size.height, "{id:?}");
            assert_eq!(outline.padding.left, solid.padding.left, "{id:?}");
            assert_eq!(outline.corner_radii, solid.corner_radii, "{id:?}");
            // Each fill carries its tone in exactly one channel.
            assert!(outline.background.is_none(), "{id:?}");
            assert_eq!(outline.border_color, Some(theme.accent), "{id:?}");
            assert_eq!(solid.background, Some(theme.action.into()), "{id:?}");
            assert!(solid.border_color.is_none(), "{id:?}");
        }
    }
}
