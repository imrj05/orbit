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

/// The "Experimental" tag on a feature that is not stable yet (remote SSH
/// sessions): an outline badge in the warning tone. The word carries the
/// meaning; the tone only reinforces it.
pub(crate) fn experimental_badge(theme: &Theme) -> AnyElement {
    gpui::div()
        .flex_none()
        .debug_selector(|| "experimental-badge".to_string())
        .child(
            Badge::new(
                tr!("common.experimental"),
                theme.warning,
                BadgeFill::Outline(theme.warning),
            )
            .render(theme),
        )
        .into_any_element()
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
    max_w: Option<Pixels>,
    open: bool,
    caret: bool,
    tip: Option<SharedString>,
    selector: Option<SharedString>,
    trailing: Option<AnyElement>,
    leading: Option<AnyElement>,
    disabled: bool,
    shrink: bool,
    mark: Option<ChipMark>,
    mono: bool,
    label_tip: Option<SharedString>,
    applied: bool,
    subdued: bool,
}

/// What a chip that is not a trigger reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChipMark {
    /// One choice among several (a radio): `true` is the one in force.
    Choice(bool),
    /// An independent on/off: `true` is on, and a check says so.
    Toggle(bool),
}

impl ChipMark {
    fn on(self) -> bool {
        matches!(self, Self::Choice(true) | Self::Toggle(true))
    }
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
            max_w: None,
            open: false,
            caret: true,
            tip: None,
            selector: None,
            trailing: None,
            leading: None,
            disabled: false,
            shrink: false,
            mark: None,
            mono: false,
            label_tip: None,
            applied: false,
            subdued: false,
        }
    }

    /// Rest the label on muted ink: the qualifier half of a joined pair (the
    /// thinking level after the model name) reads as a modifier of its
    /// neighbour, not a second control. Open and lit still take primary ink.
    pub(crate) fn subdued(mut self) -> Self {
        self.subdued = true;
        self
    }

    /// The full value on hover over a truncated label — a folder path, a long
    /// branch — separate from the chip's own [`Self::tip`] naming the control.
    pub(crate) fn label_tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.label_tip = Some(tip.into());
        self
    }

    /// Draw the label in the code face — a tool or command name is machine
    /// text, the register the transcript uses for an identifier.
    pub(crate) fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// Make the chip a **choice** — one of several, like a radio — instead of
    /// a trigger. The choice in force takes `surface_selected` and a strong
    /// hairline, held under the pointer so hovering never reads as
    /// deselecting it. No caret: a choice opens nothing.
    pub(crate) fn selected(mut self, selected: bool) -> Self {
        self.mark = Some(ChipMark::Choice(selected));
        self.caret = false;
        self
    }

    /// Make the chip an **on/off toggle**. On, it reads like a selected
    /// choice and carries a leading check, so the state never rests on the
    /// fill alone. No caret.
    pub(crate) fn checked(mut self, checked: bool) -> Self {
        self.mark = Some(ChipMark::Toggle(checked));
        self.caret = false;
        self
    }

    /// Whether the chip shows the held "on" treatment (open trigger,
    /// selected choice or checked toggle).
    fn lit(&self) -> bool {
        self.open || self.applied || self.mark.is_some_and(ChipMark::on)
    }

    /// A trigger whose filter is **in force** (a label picked, a scope
    /// narrowed): the strong hairline and primary ink, with the caret and the
    /// ordinary hover kept — it still opens its picker. Unlike
    /// [`Self::selected`] it says nothing about the fill, so `open` still reads.
    pub(crate) fn applied(mut self, applied: bool) -> Self {
        self.applied = applied;
        self
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

    /// Cap the whole chip; a truncating label gives way first. For a chip in
    /// a crowded row where its label may not push anything off — a long
    /// GitHub label beside the search field this one sits beside.
    pub(crate) fn max_w(mut self, width: Pixels) -> Self {
        self.max_w = Some(width);
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
    pub(crate) fn selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    fn frame(&self, theme: &Theme) -> gpui::Stateful<gpui::Div> {
        let open = self.open;
        let marked = self.mark.is_some_and(ChipMark::on);
        let mut el = button_frame(div().id(self.id.clone()), theme, self.size);
        el = if self.disabled {
            el.disabled_state(theme)
        } else {
            el.cursor_pointer()
        };
        if self.shrink {
            el = el.min_w_0().max_w_full();
        }
        if let Some(max_w) = self.max_w {
            el = el.max_w(max_w);
        }
        if let Some(selector) = self.selector.clone() {
            el = el.debug_selector(move || selector.to_string());
        }
        el = match self.style {
            _ if self.disabled => el,
            ChipStyle::Quiet => el
                .when_open(open, |el| {
                    el.bg(theme.pressed_wash()).text_color(theme.text_primary)
                })
                .when_open(marked, |el| {
                    el.bg(theme.selected_wash()).text_color(theme.text_primary)
                }),
            ChipStyle::Outlined { inset } => {
                let face = if open {
                    theme.surface_active
                } else if marked {
                    theme.surface_selected
                } else if inset {
                    theme.background
                } else {
                    theme.surface_elevated
                };
                let el = el
                    .border_1()
                    .border_color(if open || marked || self.applied {
                        theme.border_strong
                    } else {
                        theme.hairline()
                    })
                    .raised(face, theme);
                // A selected choice or a checked toggle holds its fill under
                // the pointer; everything else lifts its own face one step,
                // which stays opaque over glass.
                if marked {
                    el
                } else {
                    el.hover_face_open(theme, face, open)
                }
            }
        };
        if let Some(tip) = self.tip.clone() {
            el = el.tip(tip);
        }
        el
    }

    fn content(self, el: gpui::Stateful<gpui::Div>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
        let lit = self.lit();
        let open = self.open;
        let check = (self.mark == Some(ChipMark::Toggle(true))).then(|| {
            icon(
                "icons/check.svg",
                self.size.icon_size().px(theme),
                theme.text_primary,
            )
            .into_any_element()
        });
        let quiet = self.style == ChipStyle::Quiet;
        let icon_ink = if self.disabled {
            theme.text_disabled
        } else if lit && !quiet {
            theme.text_primary
        } else {
            theme.text_muted
        };
        let label_ink = if self.disabled {
            theme.text_disabled
        } else if lit {
            theme.text_primary
        } else if self.subdued {
            theme.text_muted
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
        el.children(check)
            .children(leading)
            .children(self.label.map(|label| {
                let mut text = div().text_color(label_ink);
                if self.mono {
                    text = text
                        .font_family(crate::theme::code_font_family())
                        .text_size(theme.code_px(11.));
                }
                if let Some(selector) = self.selector.clone() {
                    text = text.debug_selector(move || format!("{selector}-label"));
                }
                let text = match self.label_max_w {
                    Some(width) => text.min_w_0().max_w(width).truncate(),
                    None if self.shrink || self.max_w.is_some() => text.min_w_0().truncate(),
                    None => text,
                };
                match self.label_tip {
                    Some(tip) => text
                        .id(SharedString::from(format!("{}-label", self.id)))
                        .tip(tip)
                        .child(label)
                        .into_any_element(),
                    None => text.child(label).into_any_element(),
                }
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
        let marked = self.mark.is_some_and(ChipMark::on);
        let name = self.id.clone();
        let disabled = self.disabled;
        let el = self.content(el, theme);
        if quiet && !disabled {
            let hover = if marked {
                Hover::over(Some(theme.selected_wash()), theme.selected_wash())
            } else {
                Hover::over(open.then_some(theme.pressed_wash()), theme.hover_wash())
            };
            crate::motion::hover(el, name, theme, hover)
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

    /// The chip with no handler — a disabled or placeholder trigger.
    pub(crate) fn render(self, theme: &Theme) -> AnyElement {
        let el = self.frame(theme);
        self.finish(el, theme)
    }

    pub(crate) fn on_click(
        self,
        theme: &Theme,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let el = self.frame(theme);
        let el = if self.disabled {
            el
        } else {
            gpui::StatefulInteractiveElement::on_click(el, handler)
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

type RemoveHandler = Box<dyn Fn(&MouseUpEvent, &mut Window, &mut App)>;
type OpenHandler = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut App)>;

/// A file attached to something being composed — a prompt's attachment, a bug
/// report's screenshot: a thumbnail or glyph, the truncated name (its full
/// value in the tooltip), optional metadata (a size), and a nested remove
/// button.
///
/// **Only the remove button removes.** A chip large enough to read the name is
/// large enough to hit by accident, so the body never deletes; with
/// [`Self::on_open`] it previews instead.
pub(crate) struct FileChip {
    id: gpui::ElementId,
    visual: AnyElement,
    name: SharedString,
    name_tip: Option<SharedString>,
    meta: Option<SharedString>,
    selector: Option<SharedString>,
    max_name_w: Pixels,
    remove_tip: SharedString,
    remove: Option<RemoveHandler>,
    open: Option<OpenHandler>,
}

impl FileChip {
    pub(crate) fn new(
        id: impl Into<gpui::ElementId>,
        visual: impl IntoElement,
        name: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            visual: visual.into_any_element(),
            name: name.into(),
            name_tip: None,
            meta: None,
            selector: None,
            max_name_w: gpui::px(180.),
            remove_tip: SharedString::default(),
            remove: None,
            open: None,
        }
    }

    /// The full name (or path) on hover over the truncated label.
    pub(crate) fn name_tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.name_tip = Some(tip.into());
        self
    }

    pub(crate) fn meta(mut self, meta: impl Into<SharedString>) -> Self {
        self.meta = Some(meta.into());
        self
    }

    /// A test hook; the remove button gets `remove-<selector>`.
    pub(crate) fn selector(mut self, selector: impl Into<SharedString>) -> Self {
        self.selector = Some(selector.into());
        self
    }

    pub(crate) fn max_name_width(mut self, width: Pixels) -> Self {
        self.max_name_w = width;
        self
    }

    /// The nested remove button: its localized tooltip and handler.
    pub(crate) fn on_remove(
        mut self,
        tip: impl Into<SharedString>,
        handler: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.remove_tip = tip.into();
        self.remove = Some(Box::new(handler));
        self
    }

    /// Clicking the chip's body previews the file.
    pub(crate) fn on_open(
        mut self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.open = Some(Box::new(handler));
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> AnyElement {
        use crate::ui::button::{IconButton, IconButtonVariant};
        let remove_id = format!(
            "remove-{}",
            self.selector.clone().unwrap_or_else(|| format!("{}", self.id).into())
        );
        let mut chip = button_frame(div().id(self.id), theme, ButtonSize::Medium)
            .border_1()
            .border_color(theme.hairline())
            .raised(theme.surface_elevated, theme)
            .hover_border(theme);
        if let Some(selector) = self.selector.clone() {
            chip = chip.debug_selector(move || selector.to_string());
        }
        let mut name = div()
            .id(SharedString::from(format!("{remove_id}-name")))
            .min_w_0()
            .max_w(self.max_name_w)
            .truncate()
            .text_color(theme.text_secondary);
        if let Some(tip) = self.name_tip {
            name = name.tip(tip);
        }
        let chip = chip
            .child(self.visual)
            .child(name.child(self.name))
            .children(self.meta.map(|meta| {
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(theme))
                    .text_color(theme.text_muted)
                    .child(meta)
            }))
            .children(self.remove.map(|remove| {
                let selector = remove_id.clone();
                IconButton::glyph(
                    gpui::ElementId::Name(remove_id.clone().into()),
                    icon("icons/x.svg", IconSize::XSmall.px(theme), theme.text_muted),
                )
                .size(ButtonSize::Compact)
                .variant(IconButtonVariant::Glass)
                .tip(self.remove_tip.clone())
                .on_mouse_up(theme, move |event, window, cx| {
                    // Nested in the chip: the chip's own open must not fire.
                    cx.stop_propagation();
                    remove(event, window, cx);
                })
                .debug_selector(move || selector.clone())
            }));
        match self.open {
            Some(open) => gpui::StatefulInteractiveElement::on_click(
                chip.cursor_pointer(),
                move |event, window, cx| open(event, window, cx),
            )
            .into_any_element(),
            None => chip.into_any_element(),
        }
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

    fn fill_and_edge(chip: ChipButton, theme: &Theme) -> (Option<gpui::Fill>, Option<gpui::Hsla>) {
        let mut el = chip.frame(theme);
        let style = el.style();
        (style.background.clone(), style.border_color)
    }

    #[test]
    fn open_selected_and_checked_each_take_their_role_in_every_palette() {
        for id in crate::theme::ThemeId::ALL {
            let theme = Theme::for_id(id);
            let chip = || ChipButton::new("c", ChipStyle::Outlined { inset: false });
            let rest = fill_and_edge(chip(), &theme);
            assert_eq!(rest, (Some(theme.surface_elevated.into()), Some(theme.border)), "{id:?}");
            let open = fill_and_edge(chip().open(true), &theme);
            assert_eq!(open, (Some(theme.surface_active.into()), Some(theme.border_strong)), "{id:?}");
            for on in [chip().selected(true), chip().checked(true)] {
                assert_eq!(
                    fill_and_edge(on, &theme),
                    (Some(theme.surface_selected.into()), Some(theme.border_strong)),
                    "{id:?}"
                );
            }
            for off in [chip().selected(false), chip().checked(false)] {
                assert_eq!(fill_and_edge(off, &theme), rest, "{id:?}");
            }
        }
    }

    #[test]
    fn choices_and_toggles_open_nothing() {
        let chip = || ChipButton::new("c", ChipStyle::Outlined { inset: false });
        assert!(chip().caret);
        assert!(!chip().selected(true).caret);
        assert!(!chip().checked(false).caret);
        assert!(chip().checked(true).lit());
        assert!(!chip().checked(false).lit());
    }

    #[test]
    fn an_applied_trigger_keeps_its_face_and_caret_but_takes_the_strong_edge() {
        for id in crate::theme::ThemeId::ALL {
            let theme = Theme::for_id(id);
            let chip = || ChipButton::new("c", ChipStyle::Outlined { inset: false });
            assert_eq!(
                fill_and_edge(chip().applied(true), &theme),
                (Some(theme.surface_elevated.into()), Some(theme.border_strong)),
                "{id:?}"
            );
            assert_eq!(
                fill_and_edge(chip().applied(true).open(true), &theme),
                (Some(theme.surface_active.into()), Some(theme.border_strong)),
                "{id:?}"
            );
        }
        let applied = ChipButton::new("c", ChipStyle::Outlined { inset: false }).applied(true);
        assert!(applied.caret);
        assert!(applied.lit());
    }
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
