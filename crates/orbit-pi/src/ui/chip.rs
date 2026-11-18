//! Chips, badges and pills: compact labels. A [`Badge`] reports a state and
//! never acts; a [`Pill`] reports a state with a leading glyph (a status, a
//! count); a [`ChipButton`] is a trigger that opens a picker.
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
use crate::ui::style::control::{ControlExt, StatefulControlExt, TipExt};

/// How a badge carries its tone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BadgeFill {
    /// A tinted face behind the ink (provider state, plan tier).
    Solid(Hsla),
    /// A hairline in the tone, no face (git refs).
    Outline(Hsla),
    /// A raised face inside a neutral hairline (a file's save state): the
    /// tone lives in the ink, not the frame.
    Framed { face: Hsla, edge: Hsla },
}

/// A one-line label at Compact height, `Radius::Small`, XSmall medium ink.
pub(crate) struct Badge {
    label: SharedString,
    ink: Hsla,
    fill: BadgeFill,
    leading: Option<AnyElement>,
}

impl Badge {
    pub(crate) fn new(label: impl Into<SharedString>, ink: Hsla, fill: BadgeFill) -> Self {
        Self {
            label: label.into(),
            ink,
            fill,
            leading: None,
        }
    }

    /// A glyph before the label (a lock on a read-only file), sized
    /// `IconSize::Indicator`.
    pub(crate) fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    pub(crate) fn render(mut self, theme: &Theme) -> AnyElement {
        let label = self.label.clone();
        let leading = self.leading.take();
        self.frame(theme)
            .gap(DynamicSpacing::Base04.px(theme))
            .children(leading)
            .child(label)
            .into_any_element()
    }

    fn frame(&self, theme: &Theme) -> gpui::Div {
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
            BadgeFill::Framed { face, edge } => el.bg(face).border_1().border_color(edge),
        }
    }
}

/// A status read-out: a leading glyph (an icon, a live spinner) and a short
/// label in one tone. Like a [`Badge`] it reports and never acts.
///
/// Two shapes, decided by whether it has a face:
///
/// - **bare** — glyph and Small label on the row's own surface (an MCP
///   server's connection state);
/// - **faced** ([`Pill::face`]) — a fully rounded capsule at a button height,
///   XSmall label, tighter gap (the composer's queued-messages count).
pub(crate) struct Pill {
    label: SharedString,
    ink: Hsla,
    leading: Option<AnyElement>,
    face: Option<(Hsla, ButtonSize)>,
}

impl Pill {
    pub(crate) fn new(label: impl Into<SharedString>, ink: Hsla) -> Self {
        Self {
            label: label.into(),
            ink,
            leading: None,
            face: None,
        }
    }

    /// The glyph before the label, sized by the caller (`IconSize::XSmall`).
    pub(crate) fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// A capsule face behind the pill, `size` tall.
    pub(crate) fn face(mut self, fill: Hsla, size: ButtonSize) -> Self {
        self.face = Some((fill, size));
        self
    }

    /// The finished pill, for a caller that adds an id, a selector or a tip.
    pub(crate) fn render(self, theme: &Theme) -> gpui::Div {
        let el = div()
            .flex()
            .items_center()
            .text_color(self.ink)
            .children(self.leading)
            .child(self.label);
        match self.face {
            Some((fill, size)) => el
                .flex_none()
                .h(size.height(theme))
                .px(DynamicSpacing::Base06.px(theme))
                .rounded_full()
                .bg(fill)
                .gap(DynamicSpacing::Base02.px(theme))
                .text_size(TextSize::XSmall.px(theme)),
            None => el
                .gap(DynamicSpacing::Base04.px(theme))
                .text_size(TextSize::Small.px(theme)),
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
    leading: Option<AnyElement>,
    disabled: bool,
    shrink: bool,
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
            leading: None,
            disabled: false,
            shrink: false,
        }
    }

    pub(crate) fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// A caller-built mark in place of [`Self::icon`] — a provider logo in
    /// its brand tint. Size it to `size.icon_size()`.
    pub(crate) fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// The trigger cannot open now (a run is in flight): the shared disabled
    /// treatment, disabled ink, no hover and no handler. A disabled chip is
    /// never shown open.
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        if disabled {
            self.open = false;
        }
        self
    }

    /// Let the chip shrink inside its row (`min_w_0`, never wider than the
    /// row) so a truncating label gives way first.
    pub(crate) fn shrink(mut self) -> Self {
        self.shrink = true;
        self
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
        self.open = open && !self.disabled;
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
        let mut el = button_frame(div().id(self.id.clone()), theme, self.size);
        el = if self.disabled {
            el.disabled_state(theme)
        } else {
            el.cursor_pointer()
        };
        if self.shrink {
            el = el.min_w_0().max_w_full();
        }
        if let Some(selector) = self.selector {
            el = el.debug_selector(move || selector.to_string());
        }
        el = match self.style {
            _ if self.disabled => el,
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
        let icon_ink = if self.disabled {
            theme.text_disabled
        } else if open && !quiet {
            theme.text_primary
        } else {
            theme.text_muted
        };
        let label_ink = if self.disabled {
            theme.text_disabled
        } else if open {
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
        let leading = self.leading.or_else(|| {
            self.icon.map(|path| {
                icon(path, self.size.icon_size().px(theme), icon_ink).into_any_element()
            })
        });
        el.children(leading)
            .children(self.label.map(|label| {
                let mut text = div().text_color(label_ink);
                if let Some(selector) = self.selector {
                    text = text.debug_selector(move || format!("{selector}-label"));
                }
                let text = match self.label_max_w {
                    Some(width) => text.min_w_0().max_w(width).truncate(),
                    None => text,
                };
                text.child(label)
            }))
            .children(self.trailing)
            .children(self.caret.then(|| {
                if quiet {
                    dropdown_chevron(caret_id, open, IconSize::XSmall.px(theme), caret_ink, theme)
                } else {
                    icon(
                        "icons/chevron-down.svg",
                        IconSize::XSmall.px(theme),
                        caret_ink,
                    )
                    .into_any_element()
                }
            }))
    }

    fn finish(self, el: gpui::Stateful<gpui::Div>, theme: &Theme) -> AnyElement {
        let quiet = self.style == ChipStyle::Quiet;
        let open = self.open;
        let name = self.id.clone();
        let disabled = self.disabled;
        let el = self.content(el, theme);
        if quiet && !disabled {
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
        let el = self.frame(theme);
        let el = if self.disabled {
            el
        } else {
            el.on_mouse_up(MouseButton::Left, handler)
        };
        self.finish(el, theme)
    }

    pub(crate) fn on_mouse_down(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let el = self.frame(theme);
        let el = if self.disabled {
            el
        } else {
            el.on_mouse_down(MouseButton::Left, handler)
        };
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
    fn a_disabled_chip_is_never_open_and_takes_the_disabled_treatment() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let chip = ChipButton::new("c", ChipStyle::Quiet)
                .disabled(true)
                .open(true);
            assert!(!chip.open, "{id:?}");
            let mut frame = chip.frame(&theme);
            let mut quiet = ChipButton::new("q", ChipStyle::Quiet).frame(&theme);
            assert_ne!(frame.style().background, Some(theme.surface_active.into()));
            assert_ne!(frame.style().background, quiet.style().background, "{id:?}");
        }
    }

    #[test]
    fn a_faced_pill_is_a_capsule_and_a_bare_one_has_no_frame() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let mut faced = Pill::new("2", theme.text_muted)
                .face(theme.surface_elevated, ButtonSize::Medium)
                .render(&theme);
            let mut bare = Pill::new("Connected", theme.success).render(&theme);
            let (faced, bare) = (faced.style(), bare.style());
            assert_eq!(
                faced.background,
                Some(theme.surface_elevated.into()),
                "{id:?}"
            );
            assert!(faced.size.height.is_some(), "{id:?}");
            assert!(bare.background.is_none(), "{id:?}");
            assert!(bare.size.height.is_none(), "{id:?}");
            assert_eq!(
                bare.text.as_ref().and_then(|text| text.color),
                Some(theme.success),
                "{id:?}"
            );
        }
    }

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
