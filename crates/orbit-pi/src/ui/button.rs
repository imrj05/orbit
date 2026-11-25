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

use crate::motion::{Hover, Hovered};
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
    /// The confirming action of an irreversible step (Delete, in the
    /// confirmation that asked): the error fill with its own foreground.
    /// Reserve it for the confirm itself — the entry that opens the
    /// confirmation stays [`Self::Danger`].
    Destructive,
    /// A control floating on glass or a translucent panel (the ask panel's
    /// Back, an action in a popover over the workbench, a banner's Reconnect):
    /// the `overlay` ink wash instead of a slab, so what is behind stays
    /// visible, and no hairline.
    Glass,
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
            Self::Destructive => theme.error_fg,
            Self::Glass => theme.text_secondary,
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
                .border_color(theme.hairline())
                .raised(theme.control_face(theme.surface_elevated), theme)
                .text_color(theme.text_secondary),
        ),
        ButtonVariant::Outline => press(
            el.border_1()
                .border_color(theme.hairline())
                .raised(theme.control_face(theme.surface_elevated), theme)
                .text_color(theme.text_primary),
        ),
        ButtonVariant::Tinted => el
            .border_1()
            .border_color(theme.accent.opacity(0.5))
            .raised(theme.accent.opacity(0.14), theme)
            .text_color(theme.accent_text),
        ButtonVariant::Danger => press(
            el.border_1()
                .border_color(theme.hairline())
                .raised(theme.control_face(theme.surface_elevated), theme)
                .text_color(theme.error),
        ),
        ButtonVariant::Ghost => el
            .text_color(theme.text_secondary)
            .pressed_state(theme, PressedRamp::Wash),
        ButtonVariant::Destructive => press(
            el.border_1()
                .border_color(gpui::transparent_black())
                .raised(theme.error, theme)
                .text_color(theme.error_fg),
        ),
        ButtonVariant::Glass => press(
            el.border_1()
                .border_color(gpui::transparent_black())
                .raised(theme.overlay, theme)
                .text_color(theme.text_secondary),
        ),
    }
}

/// The variant's one-frame hover.
fn hover_swap<E: Styled + InteractiveElement>(el: E, theme: &Theme, variant: ButtonVariant) -> E {
    match variant {
        ButtonVariant::Primary => el.hover_action(theme),
        ButtonVariant::Secondary => el.hover_face(theme, theme.surface_elevated),
        ButtonVariant::Ghost => el.hover_state(theme),
        ButtonVariant::Outline => el.hover_face_bordered(theme, theme.surface_elevated),
        ButtonVariant::Tinted => el.hover_accent_wash(theme, AccentWash::Faint),
        ButtonVariant::Danger => el.hover_danger_soft(theme),
        ButtonVariant::Destructive => el.hover_danger(theme),
        ButtonVariant::Glass => el.hover_overlay_strong(theme),
    }
}

impl ButtonVariant {
    /// The variant's hover as a cross-fade on `Motion::Interaction`, or `None`
    /// for the variants whose hover changes more than a fill (danger's
    /// hairline, tinted's accent wash) and keep the one-frame swap.
    pub(crate) fn fade(self, theme: &Theme) -> Option<Hover> {
        match self {
            Self::Primary => Some(Hover::fill(theme.action, theme.action_hover)),
            Self::Secondary | Self::Outline => Some(Hover::fill(
                theme.control_face(theme.surface_elevated),
                theme.face_hover(theme.surface_elevated),
            )),
            Self::Ghost => Some(Hover::wash(theme)),
            Self::Tinted | Self::Danger | Self::Destructive | Self::Glass => None,
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
    id: ElementId,
    variant: ButtonVariant,
    size: ButtonSize,
    label: Option<SharedString>,
    icon: Option<&'static str>,
    leading: Option<AnyElement>,
    disabled: bool,
    loading: bool,
    tip: Option<SharedString>,
    focus: Option<gpui::FocusHandle>,
}

impl Button {
    pub(crate) fn new(id: impl Into<SharedString>, variant: ButtonVariant) -> Self {
        Self::keyed(ElementId::Name(id.into()), variant)
    }

    /// [`Self::new`] for an id that is not a string — a row's
    /// `ElementId::NamedInteger`, so a list of buttons keys each by index.
    pub(crate) fn keyed(id: impl Into<ElementId>, variant: ButtonVariant) -> Self {
        Self {
            id: id.into(),
            variant,
            size: ButtonSize::Medium,
            label: None,
            icon: None,
            leading: None,
            disabled: false,
            loading: false,
            tip: None,
            focus: None,
        }
    }

    /// Make the button a keyboard stop: it tracks `focus`, shows the focus
    /// treatment the frame inherits, and Enter / Space run its `on_click`
    /// (GPUI fires a focused element's click listeners on those keys). Wire
    /// the handler with [`Self::on_click`] — a mouse-up or mouse-down
    /// handler is pointer-only.
    pub(crate) fn focus(mut self, focus: &gpui::FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
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

    /// Work in flight: a spinner replaces the icon (a caller-built `leading`
    /// stays — it is its own indicator), the label stays so the button keeps
    /// its width, and the button cannot be pressed again.
    pub(crate) fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    fn can_act(&self) -> bool {
        !self.disabled && !self.loading
    }

    /// The finished button with no handler — for a caller that wires its own
    /// event (and is then responsible for honouring `disabled`).
    pub(crate) fn render(self, theme: &Theme) -> Hovered<Stateful<Div>> {
        let can_act = self.can_act();
        let id = self.id.clone();
        let name = SharedString::from(format!("{id}"));
        let mut el = button_frame(div().id(id.clone()), theme, self.size).group(BUTTON_GROUP);
        if let Some(focus) = &self.focus {
            el = el.track_focus(focus);
        }
        let el = if self.variant == ButtonVariant::Ghost {
            el
        } else {
            el.font_weight(FontWeight::MEDIUM)
        };
        // A variant with a fade descriptor paints its hover through the fade
        // below; only the ones that change more than a fill (a hairline, an
        // accent wash) keep the one-frame swap.
        let hover = self.variant.fade(theme);
        let swap_hover = hover.is_none();
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
        // A caller's own leading element wins even while loading: it is the
        // live indicator (a refresh glyph that turns while it works).
        let leading = if let Some(leading) = self.leading {
            Some(leading)
        } else if self.loading {
            Some(spinner(
                ElementId::Name(SharedString::from(format!("{id}-loading"))),
                icon_px,
                ink,
                *theme,
            ))
        } else {
            self.icon
                .map(|path| icon(path, icon_px, ink).into_any_element())
        };
        let el = match self.tip {
            Some(tip) => el.tip(tip),
            None => el,
        };
        // The label gives way before the frame does: in a button squeezed by
        // its row (a half-width sidebar action, a long translation) it ends
        // in an ellipsis instead of spilling past the edge; the icon stays.
        let el = el.children(leading).children(
            self.label
                .map(|label| div().min_w_0().truncate().child(label)),
        );
        // A button that cannot act has nothing to fade to.
        let hover = if can_act {
            hover.unwrap_or_else(Hover::none)
        } else {
            Hover::none()
        };
        crate::motion::fade(el, name, theme, hover)
    }

    /// The button acting on a click (press and release on the button).
    pub(crate) fn on_click(
        self,
        theme: &Theme,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Hovered<Stateful<Div>> {
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
    ) -> Hovered<Stateful<Div>> {
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
    ) -> Hovered<Stateful<Div>> {
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
    /// Kept for its call sites: [`Self::render`] already fades, so this only
    /// lets a caller attach handlers to the control before it becomes an
    /// element.
    pub(crate) fn faded(
        self,
        theme: &Theme,
        attach: impl FnOnce(Hovered<Stateful<Div>>) -> Hovered<Stateful<Div>>,
    ) -> AnyElement {
        attach(self.render(theme)).into_any_element()
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
        .focus_ring_reserved(theme)
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
        .focus_ring_reserved(theme)
}

/// A text disclosure: a medium, muted label beside a `disclosure_chevron`,
/// whose ink lifts under the pointer — a transcript section or turn that folds.
/// The caller adds the chevron (leading or trailing), the label and any note,
/// and the click; the frame owns the look so every fold reads alike.
pub(crate) fn disclosure_toggle(
    id: impl Into<ElementId>,
    theme: &Theme,
    size: ButtonSize,
) -> Stateful<Div> {
    button_frame(div().id(id), theme, size)
        .cursor_pointer()
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_muted)
        .hover_ink(theme)
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
        button.bg(theme.pressed_wash())
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
    pub(crate) fn render(self, theme: &Theme) -> Hovered<Stateful<Div>> {
        // The fade keys off the button's own id, so two icon buttons in one row
        // never share a clock.
        let name = SharedString::from(format!("{}", self.id));
        let el = icon_button_frame(div().id(self.id), theme, self.size).group(BUTTON_GROUP);
        let el = if self.disabled {
            el.cursor_not_allowed()
        } else {
            // The fade owns the hover, so the frame adds none.
            el.cursor_pointer().pressed_state(theme, PressedRamp::Wash)
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
        let el = match self.glyph {
            Some(glyph) => el.child(glyph),
            None => el.child(icon(self.icon, self.size.icon_size().px(theme), ink)),
        };
        // A disabled control cannot act, so it has nothing to fade to.
        let hover = if self.disabled {
            Hover::none()
        } else {
            faded_icon_hover(theme, self.variant, self.open)
        };
        crate::motion::fade(el, name, theme, hover)
    }

    pub(crate) fn on_click(
        self,
        theme: &Theme,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Hovered<Stateful<Div>> {
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
    ) -> Hovered<Stateful<Div>> {
        let enabled = !self.disabled;
        let el = self.render(theme);
        if enabled {
            el.on_mouse_up(MouseButton::Left, handler)
        } else {
            el
        }
    }

    pub(crate) fn on_mouse_down(
        self,
        theme: &Theme,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Hovered<Stateful<Div>> {
        let enabled = !self.disabled;
        let el = self.render(theme);
        if enabled {
            el.on_mouse_down(MouseButton::Left, handler)
        } else {
            el
        }
    }

    /// The button with its hover faded in on `Motion::Interaction` instead of
    /// swapped on one frame, and the `open` fill held under the pointer.
    /// `attach` adds the caller's handler and any extras (a reveal-on-row-hover
    /// opacity, an anchored popover child); it runs only while the button can
    /// act.
    /// Kept for its call sites: [`Self::render`] already fades, so this only
    /// lets a caller attach handlers to the control before it becomes an
    /// element.
    pub(crate) fn faded(
        self,
        theme: &Theme,
        attach: impl FnOnce(Hovered<Stateful<Div>>) -> Hovered<Stateful<Div>>,
    ) -> AnyElement {
        attach(self.render(theme)).into_any_element()
    }
}

/// A faded icon button's hover. Open holds the active fill at rest and under
/// the pointer, so hovering an open control never reads as closing it.
fn faded_icon_hover(theme: &Theme, variant: IconButtonVariant, open: bool) -> Hover {
    match (open, variant) {
        (true, _) => Hover::over(Some(theme.pressed_wash()), theme.pressed_wash()),
        (false, IconButtonVariant::Ghost) => Hover::wash(theme),
        (false, IconButtonVariant::Glass) => Hover::over(None, theme.hover_wash()),
    }
}

/// Which fill marks the chosen segment of a [`ButtonGroup`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentFill {
    /// `surface_selected` — a persistent preference (Settings rows).
    Selected,
    /// `surface_active` — a mode or view switch (a dialog's type picker,
    /// a popover's tabs).
    Active,
}

type SegmentHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static>;

struct Segment {
    id: ElementId,
    label: AnyElement,
    selected: bool,
    enabled: bool,
    selector: Option<SharedString>,
    on_select: SegmentHandler,
}

/// A segmented control: options that abut as one bordered control, exactly
/// one chosen. The group owns the geometry (only its outer corners round,
/// neighbours share a hairline), the resting face, the chosen fill, the
/// medium label, disabled options and how both states move — the hover wash
/// and the chosen fill cross-fade together on `Motion::Interaction`, so a
/// click reads as one transition. Callers own the labels (localized) and what
/// choosing does.
///
/// ```ignore
/// ButtonGroup::new(SegmentFill::Selected)
///     .segment("mode-plan", tr!("subagents_ui.mode_plan"), mode == Plan, on_plan)
///     .segment("mode-build", tr!("subagents_ui.mode_build"), mode == Build, on_build)
///     .render(&theme)
/// ```
pub(crate) struct ButtonGroup {
    fill: SegmentFill,
    size: ButtonSize,
    inset: bool,
    segments: Vec<Segment>,
}

impl ButtonGroup {
    pub(crate) fn new(fill: SegmentFill) -> Self {
        Self {
            fill,
            size: ButtonSize::Medium,
            inset: false,
            segments: Vec::new(),
        }
    }

    pub(crate) fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Rest on the canvas instead of a raised face, for a group sitting in a
    /// card (Usage's metric switch).
    pub(crate) fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    /// One option. `on_select` runs on a click; it is not attached to the
    /// chosen option or a disabled one.
    pub(crate) fn segment(
        mut self,
        id: impl Into<ElementId>,
        label: impl IntoElement,
        selected: bool,
        on_select: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> Self {
        self.segments.push(Segment {
            id: id.into(),
            label: label.into_any_element(),
            selected,
            enabled: true,
            selector: None,
            on_select: Box::new(on_select),
        });
        self
    }

    /// Disable the last added option: it keeps its place (the group keeps
    /// its shape as availability changes) but cannot be chosen.
    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        if let Some(segment) = self.segments.last_mut() {
            segment.enabled = !disabled;
        }
        self
    }

    /// A test selector for the last added option.
    pub(crate) fn selector(mut self, selector: impl Into<SharedString>) -> Self {
        if let Some(segment) = self.segments.last_mut() {
            segment.selector = Some(selector.into());
        }
        self
    }

    fn chosen_fill(&self, theme: &Theme) -> Hsla {
        match self.fill {
            SegmentFill::Selected => theme.selected_wash(),
            SegmentFill::Active => theme.pressed_wash(),
        }
    }

    fn resting_fill(&self, theme: &Theme) -> Hsla {
        if self.inset {
            theme.control_inset(theme.background)
        } else {
            theme.control_face(theme.surface_elevated)
        }
    }

    pub(crate) fn render(self, theme: &Theme) -> Div {
        let count = self.segments.len();
        let chosen = self.chosen_fill(theme);
        let resting = self.resting_fill(theme);
        let size = self.size;
        let theme = *theme;
        let mut row = div().flex().flex_none().items_center();
        for (index, segment) in self.segments.into_iter().enumerate() {
            let name = match &segment.id {
                ElementId::Name(name) => name.clone(),
                other => SharedString::from(format!("{other}")),
            };
            let mut el = segmented_segment(
                div().id(segment.id),
                &theme,
                size,
                SegmentPosition::at(index, count),
            )
            .border_color(theme.hairline())
            .font_weight(FontWeight::MEDIUM)
            .child(segment.label);
            if let Some(selector) = segment.selector {
                el = el.debug_selector(move || selector.to_string());
            }
            if !segment.enabled {
                row = row.child(
                    el.bg(resting)
                        .text_color(theme.text_disabled)
                        .cursor_default(),
                );
                continue;
            }
            let el = if segment.selected {
                el.cursor_default()
            } else {
                el.cursor_pointer().on_click(segment.on_select)
            };
            row = row.child(crate::motion::control(
                el,
                name,
                &theme,
                crate::theme::tokens::Motion::Interaction,
                crate::theme::tokens::Motion::Interaction,
                if segment.selected { 1.0 } else { 0.0 },
                move |el, hover, selected| {
                    let fill = crate::motion::blend(
                        crate::motion::blend(resting, theme.face_hover(resting), hover),
                        chosen,
                        selected,
                    );
                    el.bg(fill).text_color(crate::motion::blend(
                        theme.text_secondary,
                        theme.text_primary,
                        selected,
                    ))
                },
            ));
        }
        row
    }
}

/// An inline text link: accent ink that deepens toward the accent under the
/// pointer, an optional trailing "opens elsewhere" arrow, and no frame — it
/// sits in a line of text or a meta row, not in a toolbar. Anything framed is
/// a [`Button`]; a link is for navigation (a URL, a page), never a command.
///
/// ```ignore
/// LinkButton::new("update-changelog-link", tr!("updater_ui.view_changelog"))
///     .external()
///     .size(TextSize::Small)
///     .on_click(&theme, move |_, _, cx| cx.open_url(&url))
/// ```
pub(crate) struct LinkButton {
    id: ElementId,
    label: SharedString,
    external: bool,
    underline: bool,
    grow: bool,
    size: Option<crate::theme::tokens::TextSize>,
}

impl LinkButton {
    pub(crate) fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            external: false,
            underline: false,
            grow: false,
            size: None,
        }
    }

    /// The trailing arrow: the link leaves the app (a browser page).
    pub(crate) fn external(mut self) -> Self {
        self.external = true;
        self
    }

    /// Underline the label, for a link that is the line's whole content (a
    /// fetched URL) and would otherwise read as plain accent text.
    pub(crate) fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    /// Take the row's remaining width and truncate the label past it.
    pub(crate) fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    /// The label size; it inherits the line's size when unset. A medium
    /// weight goes with an explicit size (a standalone link).
    pub(crate) fn size(mut self, size: crate::theme::tokens::TextSize) -> Self {
        self.size = Some(size);
        self
    }

    fn frame(&self, theme: &Theme) -> Stateful<Div> {
        let mut el = div()
            .id(self.id.clone())
            .flex()
            .items_center()
            .gap(crate::theme::tokens::DynamicSpacing::Base04.px(theme))
            .cursor_pointer()
            .text_color(theme.accent_text);
        el = if self.grow {
            el.flex_1().min_w_0()
        } else {
            el.flex_none()
        };
        if let Some(size) = self.size {
            el = el.text_size(size.px(theme)).font_weight(FontWeight::MEDIUM);
        }
        el
    }

    pub(crate) fn on_click(
        self,
        theme: &Theme,
        handler: impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> AnyElement {
        let name = match &self.id {
            ElementId::Name(name) => name.clone(),
            other => SharedString::from(format!("{other}")),
        };
        let mut label = div().min_w_0().truncate().child(self.label.clone());
        if self.underline {
            label = label.underline();
        }
        let el = self
            .frame(theme)
            .on_click(handler)
            .child(label)
            .children(self.external.then(|| {
                icon(
                    "icons/arrow-up-right.svg",
                    crate::theme::tokens::IconSize::XSmall.px(theme),
                    theme.accent_text,
                )
            }));
        crate::motion::hover(el, name, theme, Hover::ink(theme.accent_text, theme.accent))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;
    use gpui::{div, Fill};

    fn background(button: Button, theme: &Theme) -> Option<Fill> {
        let mut el = button.render(theme);
        el.style().background.clone()
    }

    #[test]
    fn a_button_group_chooses_its_fill_role_and_rests_inset_on_request() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let settings = ButtonGroup::new(SegmentFill::Selected);
            let mode = ButtonGroup::new(SegmentFill::Active).inset(true);
            assert_eq!(
                settings.chosen_fill(&theme),
                theme.surface_selected,
                "{id:?}"
            );
            assert_eq!(mode.chosen_fill(&theme), theme.surface_active, "{id:?}");
            assert_eq!(
                settings.resting_fill(&theme),
                theme.surface_elevated,
                "{id:?}"
            );
            assert_eq!(mode.resting_fill(&theme), theme.background, "{id:?}");
        }
        let group = ButtonGroup::new(SegmentFill::Active)
            .segment("a", "A", true, |_, _, _| {})
            .segment("b", "B", false, |_, _, _| {})
            .disabled(true);
        assert!(group.segments[0].enabled);
        assert!(
            !group.segments[1].enabled,
            "disabled() marks the last segment"
        );
    }

    #[test]
    fn a_link_is_accent_ink_and_grows_only_when_asked() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let link = LinkButton::new("l", "Changelog");
            let mut quiet = link.frame(&theme);
            assert_eq!(
                quiet.style().text.as_ref().and_then(|text| text.color),
                Some(theme.accent_text),
                "{id:?}"
            );
            assert_eq!(quiet.style().flex_shrink, Some(0.), "{id:?}");
            let mut grown = LinkButton::new("g", "https://example.com")
                .grow()
                .frame(&theme);
            assert_eq!(grown.style().flex_grow, Some(1.), "{id:?}");
        }
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
    fn reduced_motion_preserves_swap_hover_variants() {
        let mut theme = Theme::dark();
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        for variant in [ButtonVariant::Tinted, ButtonVariant::Danger, ButtonVariant::Destructive, ButtonVariant::Glass] {
            Button::new("swap-hover", variant).render(&theme).into_any_element();
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
}

#[cfg(test)]
mod segment_position_tests {
    use super::*;
    use crate::theme::tokens::button;
    use gpui::{div, Styled};

    /// A group of segments rounds only its outer corners: the first rounds
    /// its left edge, the last its right, and the middle stays square so
    /// abutting segments read as one control.
    #[test]
    fn segment_positions_trim_the_group_outline() {
        let theme = Theme::dark();
        let radius = button::RADIUS.px(&theme);
        let corners = |position| {
            segmented_segment(div(), &theme, ButtonSize::Medium, position)
                .style()
                .corner_radii
                .clone()
        };
        let explicit = |mut el: gpui::Div| el.style().corner_radii.clone();
        assert_eq!(
            corners(SegmentPosition::Only),
            explicit(div().rounded_none().rounded(radius))
        );
        assert_eq!(
            corners(SegmentPosition::First),
            explicit(div().rounded_none().rounded_l(radius))
        );
        assert_eq!(
            corners(SegmentPosition::Last),
            explicit(div().rounded_none().rounded_r(radius))
        );
        assert_eq!(
            corners(SegmentPosition::Middle),
            explicit(div().rounded_none())
        );
    }

    #[test]
    fn segment_positions_cover_a_group() {
        use SegmentPosition::{First, Last, Middle, Only};
        assert_eq!(SegmentPosition::at(0, 1), Only);
        assert_eq!(SegmentPosition::at(0, 0), Only);
        assert_eq!(SegmentPosition::at(0, 2), First);
        assert_eq!(SegmentPosition::at(1, 2), Last);
        assert_eq!(SegmentPosition::at(0, 3), First);
        assert_eq!(SegmentPosition::at(1, 3), Middle);
        assert_eq!(SegmentPosition::at(2, 3), Last);
    }
}
