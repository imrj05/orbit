//! Buttons: the one place a button's variant is decided.
//!
//! Two tiers live here (docs/plans/ui-component-layer.md §3.2):
//!
//! - **Frames and recipes** (`button_frame`, `icon_button_frame`,
//!   `segmented_segment`, `quiet_icon_button`, …) — zero-cost seams that
//!   return `Self` for call sites that own their whole tree.
//! - **[`Button`]** — the labeled action. It owns the variant → treatment map
//!   (fill, hairline, ink, hover, press), the leading glyph, the loading
//!   spinner and the disabled state, and it attaches the caller's handler only
//!   while the button can act. Surfaces keep the label (`tr!` at the call
//!   site) and what the click does.

use gpui::{
    div, relative, AnyElement, ClickEvent, Div, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement, SharedString, Stateful,
    StatefulInteractiveElement, Styled, Window,
};

use crate::motion::Hover;
use crate::theme::{
    tokens::{button, ButtonSize, RaisedExt},
    Theme,
};
use crate::ui::feedback::spinner;
use crate::ui::icon::{icon, BUTTON_GROUP};
use crate::ui::style::control::{
    press, AccentWash, ControlExt, PressedRamp, StatefulControlExt, TipExt,
};

/// What a [`Button`] means. Each variant is one treatment, defined once in
/// [`apply_variant`]; a surface picks a meaning, never a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonVariant {
    /// The one filled action of a group: the `action` role, pressing by its
    /// own colour ramp rather than the generic dim.
    Primary,
    /// The alternative beside a primary: raised surface, hairline, secondary
    /// ink, the shared wash on hover.
    Secondary,
    /// A secondary defined by its outline: primary ink, and hover lifts both
    /// the wash and the hairline (Git's action bar).
    Outline,
    /// A soft accent call to action: an accent wash and an accent hairline
    /// with accent ink (worktree and clone dialogs).
    Tinted,
    /// A destructive action that is quiet at rest: secondary chrome with error
    /// ink, warming to the error wash under the pointer.
    Danger,
    /// No fill and no hairline; the wash appears under the pointer only.
    Ghost,
}

impl ButtonVariant {
    /// The ink a glyph inside this variant takes at rest, so a caller's own
    /// leading icon matches the label.
    pub(crate) fn ink(self, theme: &Theme) -> Hsla {
        match self {
            Self::Primary => theme.action_fg,
            Self::Secondary | Self::Ghost => theme.text_secondary,
            Self::Outline => theme.text_primary,
            Self::Tinted => theme.accent_text,
            Self::Danger => theme.error,
        }
    }
}

/// The variant's rest and press treatment, with the one-frame hover swap when
/// `swap_hover` (otherwise the caller fades it in with [`ButtonVariant::fade`]).
fn variant_treatment<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    variant: ButtonVariant,
    swap_hover: bool,
) -> E {
    let el = if swap_hover {
        hover_swap(el, theme, variant)
    } else {
        el
    };
    match variant {
        ButtonVariant::Primary => el
            .border_1()
            .border_color(gpui::transparent_black())
            .raised(theme.action, theme)
            .text_color(theme.action_fg)
            .pressed_state(theme, PressedRamp::Action),
        ButtonVariant::Secondary => press(
            el.border_1()
                .border_color(theme.border)
                .raised(theme.surface_elevated, theme)
                .text_color(theme.text_secondary),
        ),
        ButtonVariant::Outline => press(
            el.border_1()
                .border_color(theme.border)
                .raised(theme.surface_elevated, theme)
                .text_color(theme.text_primary),
        ),
        ButtonVariant::Tinted => el
            .border_1()
            .border_color(theme.accent.opacity(0.5))
            .raised(theme.accent.opacity(0.14), theme)
            .text_color(theme.accent_text),
        ButtonVariant::Danger => press(
            el.border_1()
                .border_color(theme.border)
                .raised(theme.surface_elevated, theme)
                .text_color(theme.error),
        ),
        ButtonVariant::Ghost => el
            .text_color(theme.text_secondary)
            .pressed_state(theme, PressedRamp::Wash),
    }
}

/// The variant's one-frame hover.
fn hover_swap<E: Styled + InteractiveElement>(el: E, theme: &Theme, variant: ButtonVariant) -> E {
    match variant {
        ButtonVariant::Primary => el.hover_action(theme),
        ButtonVariant::Secondary | ButtonVariant::Ghost => el.hover_state(theme),
        ButtonVariant::Outline => el.hover_bordered(theme),
        ButtonVariant::Tinted => el.hover_accent_wash(theme, AccentWash::Faint),
        ButtonVariant::Danger => el.hover_danger_soft(theme),
    }
}

impl ButtonVariant {
    /// The variant's hover as a cross-fade on `Motion::Interaction`, or `None`
    /// for the variants whose hover changes more than a fill (danger's
    /// hairline, tinted's accent wash) and keep the one-frame swap.
    pub(crate) fn fade(self, theme: &Theme) -> Option<Hover> {
        match self {
            Self::Primary => Some(Hover::fill(theme.action, theme.action_hover)),
            Self::Secondary | Self::Outline => {
                Some(Hover::fill(theme.surface_elevated, theme.surface_hover))
            }
            Self::Ghost => Some(Hover::wash(theme)),
            Self::Tinted | Self::Danger => None,
        }
    }
}

/// A labeled action button.
///
/// ```ignore
/// Button::new("save", ButtonVariant::Primary)
///     .label(save_label)          // localized by the caller
///     .icon("icons/check.svg")
///     .tip(save_hint)
///     .disabled(!dirty)
///     .on_click(&theme, |_, window, cx| save(window, cx))
/// ```
///
/// The result is the keyed `Div`, so a caller can still add a tooltip, focus
/// tracking, a key context or a debug selector. Disabled and loading buttons
/// render the disabled state and **never** receive the handler.
pub(crate) struct Button {
    id: SharedString,
    variant: ButtonVariant,
    size: ButtonSize,
    label: Option<SharedString>,
    icon: Option<&'static str>,
    leading: Option<AnyElement>,
    disabled: bool,
    loading: bool,
    fade: bool,
    tip: Option<SharedString>,
}

impl Button {
    pub(crate) fn new(id: impl Into<SharedString>, variant: ButtonVariant) -> Self {
        Self {
            id: id.into(),
            variant,
            size: ButtonSize::Medium,
            label: None,
            icon: None,
            leading: None,
            disabled: false,
            loading: false,
            fade: false,
            tip: None,
        }
    }

    /// A tooltip that stays available while the button is disabled or busy,
    /// so the reason it cannot act is still readable.
    pub(crate) fn tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tip = Some(tip.into());
        self
    }

    /// Frame height, padding, label and icon size all follow this; Medium by
    /// default, the settings and dialog-footer size.
    pub(crate) fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// The localized label. Resolve it with `tr!` at the call site.
    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// A leading glyph at the frame's icon size, tinted with the variant's ink.
    pub(crate) fn icon(mut self, path: &'static str) -> Self {
        self.icon = Some(path);
        self
    }

    /// A caller-built leading element, for a glyph at a non-standard size or a
    /// live indicator. Takes precedence over [`Self::icon`].
    pub(crate) fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Work in flight: a spinner replaces the leading glyph, the label stays so
    /// the button keeps its width, and the button cannot be pressed again.
    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    fn can_act(&self) -> bool {
        !self.disabled && !self.loading
    }

    /// The finished button with no handler — for a caller that wires its own
    /// event (and is then responsible for honouring `disabled`).
    pub(crate) fn render(self, theme: &Theme) -> Stateful<Div> {
        let can_act = self.can_act();
        let id = self.id;
        let el = button_frame(div().id(ElementId::Name(id.clone())), theme, self.size)
            .group(BUTTON_GROUP);
        let el = if self.variant == ButtonVariant::Ghost {
            el
        } else {
            el.font_weight(FontWeight::MEDIUM)
        };
        let swap_hover = !(self.fade && self.variant.fade(theme).is_some());
        let el = if can_act {
            variant_treatment(el, theme, self.variant, swap_hover).cursor_pointer()
        } else if self.loading {
            // In flight reads as the variant, not as greyed out: the work is
            // real and the button will come back. No hover: it cannot act.
            variant_treatment(el, theme, self.variant, false).cursor_default()
        } else if self.variant == ButtonVariant::Ghost {
            // A ghost has no frame at rest, so it gains none when disabled:
            // only the ink drops.
            el.cursor_not_allowed().text_color(theme.text_disabled)
        } else {
            el.disabled_state(theme)
        };
        let ink = if self.disabled && !self.loading {
            theme.text_disabled
        } else {
            self.variant.ink(theme)
        };
        let icon_px = self.size.icon_size().px(theme);
        let leading = if self.loading {
            Some(spinner(
                ElementId::Name(SharedString::from(format!("{id}-loading"))),
                icon_px,
                ink,
                *theme,
            ))
        } else if let Some(leading) = self.leading {
            Some(leading)
        } else {
            self.icon
                .map(|path| icon(path, icon_px, ink).into_any_element())
        };
        let el = match self.tip {
            Some(tip) => el.tip(tip),
            None => el,
        };
        el.children(leading)
            .children(self.label.map(|label| div().child(label)))
    }

    /// The button acting on a click (press and release on the button).
    pub(crate) fn on_click(
        self,
        theme: &Theme,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let can_act = self.can_act();
        let el = self.render(theme);
        if can_act {
            el.on_click(handler)
        } else {
            el
        }
    }

    /// The button acting on the left button's release — the spelling surfaces
    /// inside clickable rows use so the row's own mouse-down never fires it.
    pub(crate) fn on_mouse_up(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let can_act = self.can_act();
        let el = self.render(theme);
        if can_act {
            el.on_mouse_up(MouseButton::Left, handler)
        } else {
            el
        }
    }

    /// The button acting on the left button's press, for toolbars whose
    /// popovers dismiss on mouse-down.
    pub(crate) fn on_mouse_down(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let can_act = self.can_act();
        let el = self.render(theme);
        if can_act {
            el.on_mouse_down(MouseButton::Left, handler)
        } else {
            el
        }
    }

    /// The button with its hover cross-faded on `Motion::Interaction` rather
    /// than swapped on one frame — the settings and session-panel register.
    /// `attach` wires the caller's events and runs only while the button can
    /// act; a variant with no fade ([`ButtonVariant::fade`]) keeps its swap.
    pub(crate) fn faded(
        mut self,
        theme: &Theme,
        attach: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
    ) -> AnyElement {
        self.fade = true;
        let can_act = self.can_act();
        let fade = self.variant.fade(theme).filter(|_| can_act);
        let name = self.id.clone();
        let el = self.render(theme);
        let el = if can_act { attach(el) } else { el };
        match fade {
            Some(hover) => crate::motion::hover(el, name, theme, hover),
            None => el.into_any_element(),
        }
    }
}

/// A labeled button's frame, Zed's `ButtonLike`: the size's height and
/// horizontal padding, Base04 between icon and label, [`button::radius`],
/// and a one-line label at 1× line height (Zed's `UiLabel`) so it sits
/// inside even a 22px button. The label size follows [`button::label_size`]
/// and an icon inside it takes [`ButtonSize::icon_size`], so every button's
/// glyph matches its frame. Callers keep colors, border, hover, press, and
/// click handling.
pub(crate) fn button_frame<E: Styled + InteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
) -> E {
    el.flex_none()
        .h(size.height(theme))
        .px(size.padding_x(theme))
        .flex()
        .items_center()
        .justify_center()
        .gap(button::gap(theme))
        .rounded(button::radius(size).px(theme))
        .whitespace_nowrap()
        .text_size(button::label_size(size).px(theme))
        .line_height(relative(1.))
        .focus_state(theme)
}

/// Geometry and focus treatment for an icon-only button; callers own its content and actions.
pub(crate) fn icon_button_frame<E: Styled + InteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
) -> E {
    el.flex_none()
        .size(size.height(theme))
        .flex()
        .items_center()
        .justify_center()
        .rounded(button::radius(size).px(theme))
        .focus_state(theme)
}

/// A segment's position in a group sharing one outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentPosition {
    First,
    Middle,
    Last,
    Only,
}

impl SegmentPosition {
    pub(crate) fn at(index: usize, count: usize) -> Self {
        match (index, count) {
            (_, 0 | 1) => Self::Only,
            (0, _) => Self::First,
            (index, count) if index + 1 == count => Self::Last,
            _ => Self::Middle,
        }
    }
}

/// Button geometry with only the group's outer corners rounded and one shared hairline.
pub(crate) fn segmented_segment<E: Styled + InteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
    position: SegmentPosition,
) -> E {
    let radius = button::radius(size).px(theme);
    let el = button_frame(el, theme, size).rounded_none().border_1();
    match position {
        SegmentPosition::Only => el.rounded(radius),
        SegmentPosition::First => el.rounded_l(radius),
        SegmentPosition::Last => el.rounded_r(radius).border_l_0(),
        SegmentPosition::Middle => el.border_l_0(),
    }
}

/// Active/rest treatment for outlined segments. Callers own events and labels.
/// Unlike `settings_segment`, the active fill is `surface_active` and the
/// frame keeps its default font weight and flat background.
pub(crate) fn active_segment<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
    position: SegmentPosition,
    selected: bool,
) -> E {
    let el = segmented_segment(el, theme, size, position)
        .cursor_pointer()
        .border_color(theme.border);
    if selected {
        el.bg(theme.surface_active).text_color(theme.text_primary)
    } else {
        el.bg(theme.surface_elevated)
            .text_color(theme.text_secondary)
            .hover_state(theme)
    }
}

/// Selected/rest treatment for medium settings segments. Callers own events and labels.
pub(crate) fn settings_segment<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    position: SegmentPosition,
    active: bool,
) -> E {
    let el = segmented_segment(el, theme, ButtonSize::Medium, position)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .border_color(theme.border);
    if active {
        el.raised_flat(theme.surface_selected)
            .text_color(theme.text_primary)
    } else {
        el.raised_flat(theme.surface_elevated)
            .text_color(theme.text_secondary)
            .hover_state(theme)
    }
}

/// Quiet icon action; callers supply content, tooltip and event handling.
pub(crate) fn quiet_icon_button<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
) -> E {
    icon_button_frame(el, theme, size)
        .cursor_pointer()
        .hover_state(theme)
        .pressed_state(theme, PressedRamp::Wash)
}

/// Icon navigation control with the existing open and pressed treatments.
/// The caller supplies the localized tooltip, icon and mouse action.
pub(crate) fn navigation_icon_button<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
    open: bool,
) -> E {
    let button = icon_button_frame(el, theme, size);
    let button = if open {
        button.bg(theme.surface_active)
    } else {
        button
    };
    button
        .cursor_pointer()
        .hover_state_open(theme, open)
        .pressed_state(
            theme,
            if open {
                PressedRamp::Selected
            } else {
                PressedRamp::Wash
            },
        )
}

/// Where an icon button sits, which decides its hover wash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconButtonVariant {
    /// On an opaque surface: the shared `surface_hover` wash.
    Ghost,
    /// Floating over glass or the transcript: ink at low alpha, so what is
    /// behind stays visible.
    Glass,
}

/// An icon-only action: one tinted glyph in a square frame, with a tooltip
/// naming it (an icon alone is never the only label).
///
/// ```ignore
/// IconButton::new("copy", "icons/copy.svg")
///     .size(ButtonSize::Compact)
///     .tip(copy_label)            // localized by the caller
///     .on_click(&theme, |_, _, cx| copy(cx))
/// ```
///
/// Like [`Button`], it attaches the handler only while it can act, and
/// returns the keyed `Div` for a caller that needs focus or a key context.
pub(crate) struct IconButton {
    id: ElementId,
    icon: &'static str,
    glyph: Option<AnyElement>,
    size: ButtonSize,
    variant: IconButtonVariant,
    ink: Option<Hsla>,
    tip: Option<SharedString>,
    disabled: bool,
    open: bool,
    fade: bool,
}

impl IconButton {
    pub(crate) fn new(id: impl Into<ElementId>, icon: &'static str) -> Self {
        Self {
            id: id.into(),
            icon,
            glyph: None,
            size: ButtonSize::Default,
            variant: IconButtonVariant::Ghost,
            ink: None,
            tip: None,
            disabled: false,
            open: false,
            fade: false,
        }
    }

    /// The button's popover or panel is showing: it holds the active fill,
    /// and its glyph takes primary ink.
    pub(crate) fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// An icon button whose glyph is caller-built — a live refresh glyph that
    /// turns while work runs, a status dot. Size it to the frame's icon size.
    pub(crate) fn glyph(id: impl Into<ElementId>, glyph: impl IntoElement) -> Self {
        let mut button = Self::new(id, "");
        button.glyph = Some(glyph.into_any_element());
        button
    }

    pub(crate) fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    pub(crate) fn variant(mut self, variant: IconButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The glyph's ink at rest; `text_muted` when unset.
    pub(crate) fn ink(mut self, ink: Hsla) -> Self {
        self.ink = Some(ink);
        self
    }

    pub(crate) fn tip(mut self, tip: impl Into<SharedString>) -> Self {
        self.tip = Some(tip.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// The finished button with no handler.
    pub(crate) fn render(self, theme: &Theme) -> Stateful<Div> {
        let el = icon_button_frame(div().id(self.id), theme, self.size).group(BUTTON_GROUP);
        let el = if self.disabled {
            el.cursor_not_allowed()
        } else {
            let el = el.cursor_pointer();
            let el = match (self.fade, self.variant) {
                // `faded` paints the hover itself.
                (true, _) => el,
                (false, IconButtonVariant::Ghost) => el.hover_state(theme),
                (false, IconButtonVariant::Glass) => el.hover_on_glass(theme),
            };
            el.pressed_state(theme, PressedRamp::Wash)
        };
        let ink = if self.disabled {
            theme.text_disabled
        } else if self.open {
            theme.text_primary
        } else {
            self.ink.unwrap_or(theme.text_muted)
        };
        let el = match self.tip {
            Some(tip) => el.tip(tip),
            None => el,
        };
        match self.glyph {
            Some(glyph) => el.child(glyph),
            None => el.child(icon(self.icon, self.size.icon_size().px(theme), ink)),
        }
    }

    pub(crate) fn on_click(
        self,
        theme: &Theme,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let enabled = !self.disabled;
        let el = self.render(theme);
        if enabled {
            el.on_click(handler)
        } else {
            el
        }
    }

    pub(crate) fn on_mouse_up(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseUpEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Stateful<Div> {
        let enabled = !self.disabled;
        let el = self.render(theme);
        if enabled {
            el.on_mouse_up(MouseButton::Left, handler)
        } else {
            el
        }
    }

    /// The button with its hover faded in on `Motion::Interaction` instead of
    /// swapped on one frame, and the `open` fill held under the pointer.
    /// `attach` adds the caller's handler and any extras (a reveal-on-row-hover
    /// opacity, an anchored popover child); it runs only while the button can
    /// act.
    pub(crate) fn faded(
        mut self,
        theme: &Theme,
        attach: impl FnOnce(Stateful<Div>) -> Stateful<Div>,
    ) -> AnyElement {
        let enabled = !self.disabled;
        let open = self.open;
        let variant = self.variant;
        let name = match &self.id {
            ElementId::Name(name) => name.clone(),
            other => SharedString::from(format!("{other}")),
        };
        // Render without the swap: the fade below owns the hover.
        self.fade = true;
        let el = self.render(theme);
        if !enabled {
            return el.into_any_element();
        }
        let el = attach(el);
        crate::motion::hover(el, name, theme, faded_icon_hover(theme, variant, open))
    }
}

/// A faded icon button's hover. Open holds the active fill at rest and under
/// the pointer, so hovering an open control never reads as closing it.
fn faded_icon_hover(theme: &Theme, variant: IconButtonVariant, open: bool) -> Hover {
    match (open, variant) {
        (true, _) => Hover::over(Some(theme.surface_active), theme.surface_active),
        (false, IconButtonVariant::Ghost) => Hover::wash(theme),
        (false, IconButtonVariant::Glass) => Hover::over(None, theme.hover_wash()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;
    use gpui::{div, prelude::FluentBuilder, Fill};

    fn background(button: Button, theme: &Theme) -> Option<Fill> {
        let mut el = button.render(theme);
        el.style().background.clone()
    }

    #[test]
    fn primary_resolves_to_the_action_role_in_every_palette() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            for size in [ButtonSize::Default, ButtonSize::Medium, ButtonSize::Large] {
                let fill = background(
                    Button::new("p", ButtonVariant::Primary)
                        .size(size)
                        .label("Go"),
                    &theme,
                );
                assert_eq!(fill, Some(theme.action.into()), "{id:?} {size:?}");
            }
        }
    }

    #[test]
    fn disabled_and_loading_buttons_cannot_act() {
        let enabled = Button::new("a", ButtonVariant::Primary);
        assert!(enabled.can_act());
        assert!(!Button::new("b", ButtonVariant::Primary)
            .disabled(true)
            .can_act());
        assert!(!Button::new("c", ButtonVariant::Secondary)
            .loading(true)
            .can_act());
    }

    #[test]
    fn disabled_button_takes_the_shared_disabled_state() {
        let theme = Theme::dark();
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Outline,
            ButtonVariant::Tinted,
            ButtonVariant::Danger,
        ] {
            let mut actual = Button::new("d", variant).disabled(true).render(&theme);
            let mut expected = button_frame(div().id("d"), &theme, ButtonSize::Medium)
                .font_weight(FontWeight::MEDIUM)
                .disabled_state(&theme);
            assert_eq!(
                actual.style().background,
                expected.style().background,
                "{variant:?}"
            );
            assert_eq!(
                actual.style().border_color,
                expected.style().border_color,
                "{variant:?}"
            );
        }
    }

    #[test]
    fn disabled_ghost_stays_flat() {
        let theme = Theme::dark();
        let fill = background(
            Button::new("g", ButtonVariant::Ghost)
                .label("x")
                .disabled(true),
            &theme,
        );
        assert_eq!(fill, None);
    }

    #[test]
    fn loading_keeps_the_variant_fill() {
        let theme = Theme::dark();
        let fill = background(
            Button::new("l", ButtonVariant::Primary)
                .label("x")
                .loading(true),
            &theme,
        );
        assert_eq!(fill, Some(theme.action.into()));
    }

    #[test]
    fn a_disabled_icon_button_does_not_offer_to_act() {
        let theme = Theme::dark();
        let mut live = IconButton::new("a", "icons/plus.svg").render(&theme);
        let mut off = IconButton::new("b", "icons/plus.svg")
            .disabled(true)
            .render(&theme);
        assert_eq!(
            live.style().mouse_cursor,
            Some(gpui::CursorStyle::PointingHand)
        );
        assert_eq!(
            off.style().mouse_cursor,
            Some(gpui::CursorStyle::OperationNotAllowed)
        );
        // Same frame either way: disabling never moves the layout.
        assert_eq!(live.style().size, off.style().size);
    }

    #[test]
    fn an_open_icon_button_keeps_its_fill_under_the_pointer() {
        let theme = Theme::dark();
        for variant in [IconButtonVariant::Ghost, IconButtonVariant::Glass] {
            let open = faded_icon_hover(&theme, variant, true);
            assert_eq!(open.at(0.).0, Some(theme.surface_active));
            assert_eq!(open.at(1.).0, Some(theme.surface_active));
            let closed = faded_icon_hover(&theme, variant, false).at(0.).0;
            assert!(
                closed.is_none_or(|fill| fill.a == 0.),
                "{variant:?} rests unfilled"
            );
        }
    }

    #[test]
    fn only_fill_only_variants_cross_fade() {
        let theme = Theme::dark();
        assert!(ButtonVariant::Primary.fade(&theme).is_some());
        assert!(ButtonVariant::Secondary.fade(&theme).is_some());
        assert!(ButtonVariant::Ghost.fade(&theme).is_some());
        assert!(ButtonVariant::Danger.fade(&theme).is_none());
        assert!(ButtonVariant::Tinted.fade(&theme).is_none());
    }

    #[test]
    fn active_segment_preserves_state_and_geometry_for_both_sizes() {
        for theme in [
            Theme::dark(),
            Theme::for_id(crate::theme::ThemeId::Operational),
        ] {
            for size in [ButtonSize::Default, ButtonSize::Medium] {
                for position in [
                    SegmentPosition::Only,
                    SegmentPosition::First,
                    SegmentPosition::Middle,
                    SegmentPosition::Last,
                ] {
                    for selected in [false, true] {
                        let mut actual =
                            active_segment(div().id("actual"), &theme, size, position, selected);
                        let radius = button::radius(size).px(&theme);
                        let expected = button_frame(div().id("expected"), &theme, size)
                            .rounded_none()
                            .border_1();
                        let mut expected = match position {
                            SegmentPosition::Only => expected.rounded(radius),
                            SegmentPosition::First => expected.rounded_l(radius),
                            SegmentPosition::Middle => expected.border_l_0(),
                            SegmentPosition::Last => expected.rounded_r(radius).border_l_0(),
                        }
                        .cursor_pointer()
                        .border_color(theme.border)
                        .bg(if selected {
                            theme.surface_active
                        } else {
                            theme.surface_elevated
                        })
                        .text_color(if selected {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        });
                        assert_eq!(
                            actual.style(),
                            expected.style(),
                            "{size:?}, {position:?}, selected={selected}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn settings_segment_preserves_selected_and_resting_styles() {
        for theme in [
            Theme::dark(),
            Theme::for_id(crate::theme::ThemeId::Operational),
        ] {
            for position in [
                SegmentPosition::Only,
                SegmentPosition::First,
                SegmentPosition::Middle,
                SegmentPosition::Last,
            ] {
                for active in [false, true] {
                    let mut actual = settings_segment(div().id("actual"), &theme, position, active);
                    let mut expected = segmented_segment(
                        div().id("expected"),
                        &theme,
                        ButtonSize::Medium,
                        position,
                    )
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .border_color(theme.border)
                    .when(active, |el| {
                        el.raised_flat(theme.surface_selected)
                            .text_color(theme.text_primary)
                    })
                    .when(!active, |el| {
                        el.raised_flat(theme.surface_elevated)
                            .text_color(theme.text_secondary)
                            .hover_state(&theme)
                    });
                    assert_eq!(
                        actual.style(),
                        expected.style(),
                        "{position:?}, active={active}"
                    );
                }
            }
        }
    }
}
