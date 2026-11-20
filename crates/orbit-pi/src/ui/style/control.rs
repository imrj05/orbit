//! The interaction-state contract, applied to an element.
//!
//! Everything here is a **style trait**: a zero-cost extension over a plain
//! GPUI tree that inherits the state vocabulary from
//! [`crate::theme::tokens::InteractionState`]. A trait returns `Self`, so the
//! caller keeps chaining `.id(…)`, `.on_click(…)`, `.child(…)` — no wrapper
//! struct, no allocation.
//!
//! A component (see `ui/`) uses these; a component never names a state colour.

use crate::theme::tokens::InteractionState;
use crate::theme::Theme;
use crate::ui::tooltip::Tooltip;
use gpui::{AppContext, Hsla, InteractiveElement, SharedString, Styled};

/// Opacity of an action button while the pointer is down.
///
/// GPUI 0.2.2 has no element transform (`Transformation` is SVG-only) and no
/// CSS-style transitions, so the usual `scale(0.96)` press cannot be expressed
/// on a `div`. This dim is its tactile stand-in — one value keeps filled,
/// ghost, and danger buttons feeling identical under the finger.
pub(crate) const PRESS_DIM: f32 = 0.85;

/// Give an action button its shared press (`:active`) feedback. Call it around
/// the finished, stateful element:
///
/// ```ignore
/// press(div().id("save").cursor_pointer().hover(...)).child(label)
/// ```
pub(crate) fn press<T: gpui::StatefulInteractiveElement>(element: T) -> T {
    element.active(|style| style.opacity(PRESS_DIM))
}

/// Attach Orbit's native tooltip to any interactive element.
///
/// The send / stop button set the register: a localized label, usually with the
/// keyboard shortcut appended (e.g. `Send (↵)`). This keeps every icon-only
/// button's hover affordance one line at the call site instead of repeating the
/// `Tooltip::new` closure.
pub(crate) trait TipExt: InteractiveElement {
    fn tip(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.interactivity()
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into());
        self
    }
}

impl<T: InteractiveElement> TipExt for T {}

/// The interaction-state contract, applied to an element.
///
/// Every interactive surface answers the same six states, and the colours come
/// from [`InteractionState`] rather than from the call site. Phase 2 of the
/// UI/UX Modernization epic routes existing call sites through this trait, so a
/// surface that wants a hover no longer names `surface_hover` itself.
///
/// ```ignore
/// button_frame(div().id("save"), &theme, ButtonSize::Default)
///     .hover_state(&theme)
///     .child(label)
/// ```
pub(crate) trait ControlExt: InteractiveElement + Styled + Sized {
    /// Hover: the contract's fill under primary ink. Replaces
    /// `.hover(|s| s.bg(theme.surface_hover))` and its `raised` twins.
    fn hover_state(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let ink = InteractionState::Hover.control_ink(theme);
        self.hover(move |style| style.bg(fill).text_color(ink))
    }

    /// Hover for a control that floats over a translucent surface or the
    /// wallpaper (header chips, in-scroller affordances, glass trays). The
    /// wash is ink at low alpha — `overlay_strong` — so what is behind the
    /// control stays visible; `surface_hover` would paint an opaque slab.
    fn hover_on_glass(self, theme: &Theme) -> Self {
        let wash = InteractionState::Glass.glass_fill(theme);
        self.hover(move |style| style.bg(wash))
    }

    /// Hover for a control painted on its own opaque `face`: the face lifts
    /// one subtle step ([`Theme::face_hover`]) under primary ink, and stays
    /// opaque on glass instead of turning into a see-through wash.
    fn hover_face(self, theme: &Theme, face: gpui::Hsla) -> Self {
        let fill = theme.face_hover(face);
        let ink = InteractionState::Hover.control_ink(theme);
        self.hover(move |style| style.bg(fill).text_color(ink))
    }

    /// [`Self::hover_face`] plus the strong hairline, for an outlined face.
    fn hover_face_bordered(self, theme: &Theme, face: gpui::Hsla) -> Self {
        let fill = theme.face_hover(face);
        let edge = theme.border_strong;
        self.hover(move |style| style.bg(fill).border_color(edge))
    }

    /// [`Self::hover_face`] for a face whose surface may be open: an open
    /// control holds its face under the pointer, a closed one lifts it.
    fn hover_face_open(self, theme: &Theme, face: gpui::Hsla, open: bool) -> Self {
        let fill = if open { face } else { theme.face_hover(face) };
        self.hover(move |style| style.bg(fill))
    }

    /// Hover for a field or track: the frame steps to `border_strong` and the
    /// fill does not change. Used where the control is defined by its outline
    /// — text fields, sliders, and segmented tracks.
    fn hover_border(self, theme: &Theme) -> Self {
        let edge = InteractionState::Hover
            .border(theme)
            .unwrap_or(theme.border_strong);
        self.hover(move |style| style.border_color(edge))
    }

    /// Hover for a primary action: the fill steps to `action_hover`, the step
    /// the `action` role exists for.
    fn hover_action(self, theme: &Theme) -> Self {
        let fill = theme.action_hover;
        self.hover(move |style| style.bg(fill))
    }

    /// Hover for a destructive control: the fill steps to `error_hover`.
    fn hover_danger(self, theme: &Theme) -> Self {
        let fill = theme.error_hover;
        self.hover(move |style| style.bg(fill))
    }

    /// Hover for a destructive control that is quiet at rest: an error-tinted
    /// border and wash, with the error ink appearing only under the pointer.
    fn hover_danger_soft(self, theme: &Theme) -> Self {
        let border = theme.error.opacity(0.6);
        let fill = theme.error.opacity(0.08);
        let ink = theme.error;
        self.hover(move |style| style.border_color(border).bg(fill).text_color(ink))
    }

    /// Hover for a control that is defined by its outline: the wash plus the
    /// strong hairline, no ink change.
    fn hover_bordered(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let edge = theme.border_strong;
        self.hover(move |style| style.bg(fill).border_color(edge))
    }

    /// A wash that keeps secondary ink — a row whose label must not brighten
    /// just because the pointer crossed it.
    fn hover_quiet(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let ink = theme.text_secondary;
        self.hover(move |style| style.bg(fill).text_color(ink))
    }

    /// Ink only: a control with no fill of its own brightens to primary.
    fn hover_ink(self, theme: &Theme) -> Self {
        let ink = theme.text_primary;
        self.hover(move |style| style.text_color(ink))
    }

    /// Ink only, to the accent's readable role (a link brightening).
    fn hover_ink_accent(self, theme: &Theme) -> Self {
        let ink = theme.accent_text;
        self.hover(move |style| style.text_color(ink))
    }

    /// The translucent ink wash for a surface that is itself translucent.
    fn hover_overlay(self, theme: &Theme) -> Self {
        let wash = theme.overlay;
        self.hover(move |style| style.bg(wash))
    }

    /// The stronger translucent wash, with primary ink.
    fn hover_overlay_strong(self, theme: &Theme) -> Self {
        let wash = theme.overlay_strong;
        let ink = theme.text_primary;
        self.hover(move |style| style.bg(wash).text_color(ink))
    }

    /// The hover a control takes while its surface is already open: it stays
    /// on the selected step, because a second wash would read as a new state.
    fn hover_state_open(self, theme: &Theme, open: bool) -> Self {
        let fill = if open {
            InteractionState::Pressed
                .fill(theme)
                .expect("Pressed fills")
        } else {
            InteractionState::Hover.fill(theme).expect("Hover fills")
        };
        self.hover(move |style| style.bg(fill))
    }

    /// An accent-tinted wash. The hue is always the accent; the call site names
    /// how strong, never which colour.
    fn hover_accent_wash(self, theme: &Theme, wash: AccentWash) -> Self {
        let fill = theme.accent.opacity(wash.alpha());
        self.hover(move |style| style.bg(fill))
    }

    /// A hairline that strengthens under the pointer (dividers, tracks).
    fn hover_divider(self, theme: &Theme) -> Self {
        let edge = theme.border_strong;
        self.hover(move |style| style.bg(edge))
    }

    /// An inverse fill: the ink colour as a background, for a small sample or
    /// swatch that previews the foreground it would paint.
    fn hover_inverse(self, theme: &Theme) -> Self {
        let fill = theme.text_primary;
        self.hover(move |style| style.bg(fill))
    }

    /// A dim rather than a colour change: image thumbs and previews.
    fn hover_dimmed(self) -> Self {
        self.hover(|style| style.opacity(0.9))
    }

    /// Focus: the 1px ring, drawn from the token, on every focusable control.
    /// A no-op on an element that cannot take focus.
    ///
    /// On its own this adds the ring's border only while focused, which costs
    /// layout on an element with no border at rest. A control that takes focus
    /// uses [`Self::focus_ring_reserved`] instead.
    fn focus_state(self, theme: &Theme) -> Self {
        let ring = InteractionState::Focused
            .border(theme)
            .expect("Focused draws the ring");
        self.focus(move |style| style.border_1().border_color(ring))
    }

    /// [`Self::focus_state`] at no layout cost: the ring's 1px is reserved at
    /// rest as a transparent border, so taking focus only changes the edge's
    /// colour. Adding the border on focus alone grew a borderless control by
    /// 2px and shifted everything after it — the new-task folder control moved
    /// its row each time the folder picker closed and handed focus back. A
    /// caller that draws its own hairline sets its colour after this, and the
    /// ring still wins while focused.
    fn focus_ring_reserved(self, theme: &Theme) -> Self {
        self.border_1()
            .border_color(gpui::transparent_black())
            .focus_state(theme)
    }
}

impl<T: InteractiveElement + Styled> ControlExt for T {}

/// The fill a control takes while the pointer is down.
///
/// A ghost or neutral control deepens its wash ([`PressedRamp::Wash`]).
/// A filled control steps its own role instead: `surface_active` would paint a
/// grey wash over a primary, accent or danger fill and change what the button
/// means while it is being clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PressedRamp {
    /// `surface_active` — the contract's pressed fill.
    Wash,
    /// `surface_hover` — for a control that is already sitting on
    /// `surface_active` because it is selected or its surface is open. The
    /// ramp has no deeper step, so the press steps back to the hover wash:
    /// the movement is the feedback, and the control still reads as held.
    Selected,
    /// `action_pressed` — a primary button.
    Action,
    /// `error_hover` — a destructive control.
    Danger,
}

impl PressedRamp {
    fn fill(self, theme: &Theme) -> Hsla {
        match self {
            Self::Wash => InteractionState::Pressed
                .fill(theme)
                .expect("Pressed fills"),
            Self::Selected => InteractionState::Hover.fill(theme).expect("Hover fills"),
            Self::Action => theme.action_pressed,
            Self::Danger => theme.error_hover,
        }
    }
}

/// The three strengths an accent-tinted hover may use. Named so a call site
/// says how strong, not which colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccentWash {
    /// A quiet tint over a large surface.
    Faint,
    /// The default: ask/approve actions.
    Soft,
    /// A deliberate highlight, e.g. a drag handle under the pointer.
    Strong,
}

impl AccentWash {
    fn alpha(self) -> f32 {
        match self {
            Self::Faint => 0.20,
            Self::Soft => 0.26,
            Self::Strong => 0.40,
        }
    }
}

/// The states GPUI only tracks on a keyed element.
///
/// Press and pointer affordances live on `StatefulInteractiveElement`, so this
/// half of the contract is a separate trait; a plain `div()` gets the hover and
/// focus halves, a `div().id(…)` gets all four.
pub(crate) trait StatefulControlExt:
    gpui::StatefulInteractiveElement + Styled + Sized
{
    /// Pressed: the control's own ramp, never a colour picked at the call site.
    fn pressed_state(self, theme: &Theme, ramp: PressedRamp) -> Self {
        let fill = ramp.fill(theme);
        self.active(move |style| style.bg(fill))
    }

    /// Disabled: a state, not an opacity. The disabled fill, ink and quiet
    /// frame, plus a non-interactive pointer. Callers drop their handlers — a
    /// control that looks disabled must not still activate.
    fn disabled_state(self, theme: &Theme) -> Self {
        let fill = InteractionState::Disabled
            .fill(theme)
            .expect("Disabled fills");
        let border = InteractionState::Disabled
            .border(theme)
            .expect("Disabled keeps its frame");
        let ink = InteractionState::Disabled.control_ink(theme);
        self.cursor_not_allowed()
            .bg(fill)
            .text_color(ink)
            .border_1()
            .border_color(border)
    }
}

impl<T: gpui::StatefulInteractiveElement + Styled> StatefulControlExt for T {}
