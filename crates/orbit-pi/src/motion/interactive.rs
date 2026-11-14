//! The interaction half of the layer: a hover treatment that fades instead of
//! swapping on a single frame.
//!
//! A hover is the smallest state change a pointer can cause, and today every one
//! of them is instantaneous: `ControlExt::hover_state` re-evaluates a style
//! closure per frame with no notion of time, so the wash and the ink jump. This
//! module adds the time without changing what the control looks like at rest.
//!
//! # How it works
//!
//! The transition owns the clock, so a control only has to be rebuilt while one
//! is in flight. That gives the shape used here:
//!
//! 1. The pointer's position over a control is read from the hit test every
//!    frame, not remembered from enter/leave events: a wheel scroll or a
//!    collapsing group moves the geometry, and no event is dispatched at all.
//!    See [`PointerWatch`].
//! 2. The control paints the value its [`Transition`] reports, so a reversal
//!    resumes from the value on screen, and the easing lives in one place
//!    (`Transition`), not in two.
//! 3. While a transition is in flight the element asks for the next frame, so a
//!    fade that nothing else is repainting still moves — and a frame that
//!    painted a value the pointer has already left asks for one too, so a stale
//!    wash cannot outlive the frame that painted it.
//!
//! # What it deliberately does not do
//!
//! GPUI 0.2.2 exposes no transform for a `Div` — `with_transformation` is an
//! `Svg` method and `Style` has no transform field — so a press cannot scale.
//! A press fade is possible but nearly invisible, so it is not attempted here;
//! `DESIGN.md` §The interaction layer records the constraint. The same reason
//! defers the focus-ring fade: a ring that is absent at rest would add a 1px
//! border and shift the layout, so the caller has to reserve it first.

use std::time::Instant;

use gpui::{
    prelude::*, Animation, AnimationExt, AnyElement, App, Bounds, Element, ElementId,
    GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId, IntoElement, LayoutId,
    ParentElement, Pixels, SharedString, Styled, Window,
};

use super::registry::Transitions;
use super::transition::Transition;
use crate::theme::tokens::Motion;
use crate::theme::Theme;

/// A control's hover treatment: the two states to fade between.
///
/// The pair is explicit because a hovered control is not always a row on a bare
/// surface — a filled action button fades `action` into `action_hover`, and a
/// caller that already paints a rest fill (a raised card) must not have it
/// replaced by the wash.
///
/// `ink` is optional on purpose: most controls set their own ink and the layer
/// has no business overwriting it. An element that inherits its colour from the
/// row (a menu entry, a list row) asks for the lift explicitly with
/// [`Hover::row`].
#[derive(Clone, Copy)]
pub(crate) struct Hover {
    rest_fill: Option<Hsla>,
    hover_fill: Option<Hsla>,
    ink: Option<(Hsla, Hsla)>,
}

impl Hover {
    /// A row, menu entry or list item: no rest fill, and its ink lifts from
    /// secondary to primary. The wash follows the surface under the row — an
    /// ink wash on glass, `surface_hover` on an opaque one.
    pub(crate) fn row(theme: &Theme) -> Self {
        Self {
            rest_fill: None,
            hover_fill: Some(theme.hover_wash()),
            ink: Some((theme.text_secondary, theme.text_primary)),
        }
    }

    /// A control on a bare surface: the wash fades in, the ink is the caller's.
    pub(crate) fn wash(theme: &Theme) -> Self {
        Self {
            rest_fill: None,
            hover_fill: Some(theme.hover_wash()),
            ink: None,
        }
    }

    /// A wash the caller names, for a control whose hover is not
    /// `surface_hover`: a menu entry over glass takes `overlay`, and a copy
    /// button acknowledging its click keeps its own tint.
    pub(crate) fn wash_in(hover: Hsla) -> Self {
        Self {
            rest_fill: None,
            hover_fill: Some(hover),
            ink: None,
        }
    }

    /// A control whose rest fill is conditional — a chosen swatch, a row that
    /// is already open. `None` means the control paints nothing at rest and the
    /// wash fades in on `surface`; `Some(rest)` cross-fades from the fill it
    /// already shows.
    pub(crate) fn over(rest: Option<Hsla>, hover: Hsla) -> Self {
        Self {
            rest_fill: rest,
            hover_fill: Some(hover),
            ink: None,
        }
    }

    /// Ink-only hover: a link or an icon that brightens without a wash.
    pub(crate) fn ink(rest: Hsla, hover: Hsla) -> Self {
        Self {
            rest_fill: None,
            hover_fill: None,
            ink: Some((rest, hover)),
        }
    }

    /// A control with a rest fill of its own — a raised card, a filled action
    /// button, a segmented half: the two fills cross-fade.
    pub(crate) fn fill(rest: Hsla, hover: Hsla) -> Self {
        Self {
            rest_fill: Some(rest),
            hover_fill: Some(hover),
            ink: None,
        }
    }

    /// The same treatment with the ink lifting as well, for a control whose
    /// label inherits its colour from the control (a quiet text button).
    pub(crate) fn with_ink(self, rest: Hsla, hover: Hsla) -> Self {
        Self {
            ink: Some((rest, hover)),
            ..self
        }
    }

    /// The colours at a point on the fade, 0 = rest, 1 = hovered.
    fn at(&self, value: f32) -> (Option<Hsla>, Option<Hsla>) {
        let fill = match (self.rest_fill, self.hover_fill) {
            (None, None) => None,
            (None, Some(hover)) => Some(hover.opacity(value)),
            (Some(rest), Some(hover)) => Some(blend(rest, hover, value)),
            (Some(rest), None) => Some(rest),
        };
        (
            fill,
            self.ink.map(|(rest, hover)| blend(rest, hover, value)),
        )
    }
}

/// Fade a control's hover treatment.
///
/// `name` keys the transition in [`Transitions`]; it must be unique per control
/// and stable across frames — the same contract as an element id. Reduce Motion
/// swaps the state instead of fading it, which is the design system's answer,
/// not a fallback.
pub(crate) fn hover<T>(
    element: T,
    name: impl Into<SharedString>,
    theme: &Theme,
    hover: Hover,
) -> AnyElement
where
    T: StatefulInteractiveElement + Styled + IntoElement + 'static,
{
    // The role, named at every use: the guard in `scripts/check_motion.py`
    // rejects a duration passed through a binding, so a reader can always see
    // which band a surface is on.
    let role = Motion::Interaction;
    // `Theme` is `Copy`; the hover listener outlives this call, so it needs its
    // own copy rather than the borrow.
    let theme = *theme;
    let name = name.into();

    if role.duration(&theme).is_zero() {
        // Reduce Motion: the same treatment, applied on the frame the pointer
        // arrives. `.hover` is a style closure, so this costs no state.
        return element
            .hover(move |style| {
                let (fill, ink) = hover.at(1.0);
                let style = match fill {
                    Some(fill) => style.bg(fill),
                    None => style,
                };
                match ink {
                    Some(ink) => style.text_color(ink),
                    None => style,
                }
            })
            .into_any_element();
    }

    // The listener is only a repaint hint: a mouse move is the one input that
    // does not otherwise dirty the window, and the hover styles are painted by
    // whoever repaints next. The target itself is derived from the pointer in
    // [`PointerWatch::prepaint`], so a stale listener state cannot leave a wash
    // lit — at worst it costs one frame.
    let element = element.on_hover(|_, _, cx| cx.refresh_windows());

    // Paint the value on screen. The transition owns the clock, so the control
    // needs no animation of its own; the watch asks for frames while it runs.
    let value = Transitions::get(ElementId::Name(name.clone()))
        .map(|transition| transition.value_at(Instant::now()))
        .unwrap_or(0.0);
    let (fill, ink) = hover.at(value);
    let element = match ink {
        Some(ink) => element.text_color(ink),
        None => element,
    };
    let element = match fill {
        Some(fill) => element.bg(fill),
        None => element,
    };
    PointerWatch {
        name,
        pump: None,
        theme,
        baked: value,
        child: element.into_any_element(),
    }
    .into_any_element()
}

/// Drives a control's fade from the pointer, one frame at a time.
///
/// A listener alone cannot do this. `on_hover` fires only on a *mouse move* and
/// diffs state it remembers, so it never sees the geometry move under a resting
/// pointer: a wheel scroll, a collapsing group or a list re-sort all leave the
/// wash lit on the row the pointer has already left, and the row now under the
/// pointer stays dark. Prepaint runs on every frame the surface draws and can
/// ask the hit test directly, so the target is re-derived rather than
/// remembered — scrolling repaints, so the scroll case heals on the next frame.
///
/// The wrapper is layout- and paint-transparent: it contributes the child's own
/// layout id and delegates both phases, so nothing about the control's geometry
/// or stacking changes.
struct PointerWatch {
    /// The transition this element drives.
    name: SharedString,
    /// A caller-driven transition that has to keep its frames coming, so a fade
    /// started from an `apply` closure is not frozen half-way (a control's
    /// selection).
    pump: Option<ElementId>,
    theme: Theme,
    /// The value the child was built with: the transition's, read at render
    /// time, before this frame's hit test existed. Paint compares it with the
    /// target it derives to tell whether the frame it just drew is already out
    /// of date.
    baked: f32,
    child: AnyElement,
}

impl IntoElement for PointerWatch {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PointerWatch {
    type RequestLayoutState = ();
    /// The hitbox, so the pointer can be read during paint: that is the phase
    /// where `Window` has already re-run the hit test against *this* frame's
    /// geometry, even when no mouse event has happened since the last one.
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        // Stable for the life of the control: the mount flag below is stored
        // under it, and a generation here is what used to break hover tracking.
        Some(ElementId::NamedInteger(self.name.clone(), u64::MAX))
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        // A hitbox rather than `bounds.contains(mouse)`, so an occluding popover
        // takes the wash off the row behind it, exactly as it does for gpui's
        // own hover styles.
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        hitbox: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // A drag suppresses the wash, the same rule gpui applies to its own
        // hover styles: the pointer is carrying something, not browsing.
        let target = if hitbox.is_hovered(window) && !cx.has_active_drag() {
            1.0
        } else {
            0.0
        };
        // The first frame for this instance mounts at the pointer's state: a row
        // scrolled back into view must not replay a wash it had before it left.
        let mounted_before = window.with_optional_element_state::<(), _>(id, |state, _| {
            (matches!(state, Some(Some(_))), Some(()))
        });
        let duration = Motion::Interaction.duration(&self.theme);
        let ease = Motion::Interaction.easing();
        let in_flight = Transitions::update(ElementId::Name(self.name.clone()), |current| {
            let now = Instant::now();
            let mut transition = match current {
                Some(transition) if mounted_before => transition,
                _ => Transition::settled(target, now).easing(ease),
            };
            // Retarget only on a real change: the settled mount state carries no
            // duration, so passing the role's would otherwise bump its
            // generation and start a fade with nothing to fade.
            if mounted_before && (transition.to() - target).abs() > f32::EPSILON {
                transition.retarget(target, duration, now);
            }
            (transition, !transition.is_done(now))
        });
        let pumping = self.pump.as_ref().is_some_and(|key| {
            Transitions::get(key.clone()).is_some_and(|t| !t.is_done(Instant::now()))
        });
        if repaint_needed(in_flight, pumping, self.baked, target) {
            window.request_animation_frame();
        }

        self.child.paint(window, cx);
    }
}

/// A state-driven transition: the caller passes the value it wants now, and the
/// layer fades from whatever is on screen.
///
/// The difference from [`hover`] is where the target comes from — a switch's
/// `on`, a segment's selection — not how it is animated. `apply` restyles the
/// element for a point on the fade, so the same element can move a knob *and*
/// cross-fade a track in one pass.
pub(crate) fn state<T>(
    element: T,
    name: impl Into<SharedString>,
    theme: &Theme,
    role: Motion,
    target: f32,
    apply: impl Fn(T, f32) -> T + 'static,
) -> AnyElement
where
    T: Styled + IntoElement + 'static,
{
    let theme = *theme;
    // Reduce Motion paints the new state on the frame it changes.
    if role.duration(&theme).is_zero() {
        return apply(element, target).into_any_element();
    }

    let name = name.into();
    let key = ElementId::Name(name.clone());
    let now = Instant::now();
    let generation = Transitions::update(key.clone(), |current| {
        let mut transition =
            current.unwrap_or_else(|| Transition::settled(target, now).easing(role.easing()));
        transition.retarget(target, role.duration(&theme), now);
        let generation = transition.generation();
        (transition, generation)
    });

    let value_key = key;
    element
        .with_animation(
            ElementId::NamedInteger(name, generation),
            // Linear: `Transition` owns the role's easing.
            Animation::new(role.duration(&theme)),
            move |element, _delta| {
                let value = Transitions::get(value_key.clone())
                    .map(|transition| transition.value_at(Instant::now()))
                    .unwrap_or(target);
                apply(element, value)
            },
        )
        .into_any_element()
}

/// A control with two live states: the pointer's hover and whether it is
/// selected.
///
/// `hover` alone cannot animate a row that also carries a selection, because the
/// two treatments share the fill — a hover wash painted over a selected wash is
/// not a blend of the two. This wrapper owns both transitions, retargets the
/// hover from its listener and the selection from the caller's value, and hands
/// `apply` both numbers in one pass.
///
/// Reduce Motion still repaints (the listener refreshes the window); it just
/// resolves both values instantly instead of fading, so the state changes and
/// the motion does not.
pub(crate) fn control<T>(
    element: T,
    name: impl Into<SharedString>,
    theme: &Theme,
    hover_role: Motion,
    state_role: Motion,
    selected: f32,
    apply: impl Fn(T, f32, f32) -> T + 'static,
) -> AnyElement
where
    T: ParentElement + StatefulInteractiveElement + Styled + IntoElement + 'static,
{
    let theme = *theme;
    let name = name.into();
    let hover_key = ElementId::Name(name.clone());
    // A second key for the state, in the same store: `u64::MAX` cannot collide
    // with a generation count.
    let state_key = ElementId::NamedInteger(name.clone(), u64::MAX);
    // The pointer half is [`PointerWatch`]'s, exactly as for a plain hover; the
    // listener here is only the repaint hint that a mouse move would otherwise
    // not cause.
    let element = element.on_hover(|_, _, cx| cx.refresh_windows());

    // The state target is the caller's, so it is retargeted here rather than
    // derived from a pointer.
    let now = Instant::now();
    let hover = Transitions::get(hover_key).unwrap_or_else(|| Transition::settled(0.0, now));
    let state = Transitions::update(state_key.clone(), |current| {
        let mut transition = current
            .unwrap_or_else(|| Transition::settled(selected, now).easing(state_role.easing()));
        transition.retarget(selected, state_role.duration(&theme), now);
        (transition, transition)
    });

    if hover_role.duration(&theme).is_zero() && state_role.duration(&theme).is_zero() {
        return apply(element, hover.value_at(now), state.value_at(now)).into_any_element();
    }

    let baked = hover.value_at(now);
    PointerWatch {
        name,
        // The state fade is retargeted from the caller's value, so the watch has
        // to keep its frames coming too.
        pump: Some(state_key),
        theme,
        baked,
        child: apply(element, baked, state.value_at(now)).into_any_element(),
    }
    .into_any_element()
}

/// Two painted values closer than this are the same frame's worth of fade: the
/// difference is invisible, and treating it as a mismatch would ask for a frame
/// that would ask for another one, forever.
const PAINTED_TOLERANCE: f32 = 1e-3;

/// Whether the frame just painted needs a successor.
///
/// A fade in flight needs one to keep moving, and a pumped state fade needs one
/// on the caller's behalf. The third case is the one that bites a scrolling
/// list: the fill a frame carries is baked at render time, before the pointer's
/// hit test for that frame exists, so a control that leaves the screen while
/// washed and comes back is painted with the wash of the instance that left.
/// Its mount settles the transition, so nothing is in flight to repaint it and
/// the wash would stay for as long as the window had no other reason to draw.
fn repaint_needed(in_flight: bool, pumping: bool, baked: f32, target: f32) -> bool {
    in_flight || pumping || (baked - target).abs() > PAINTED_TOLERANCE
}

/// Mix two colours by `t` (0 = `a`, 1 = `b`), hue over the shortest arc.
pub(crate) fn blend(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    let mut dh = b.h - a.h;
    if dh > 0.5 {
        dh -= 1.0;
    } else if dh < -0.5 {
        dh += 1.0;
    }
    Hsla {
        h: (a.h + dh * t).rem_euclid(1.0),
        s: a.s + (b.s - a.s) * t,
        l: a.l + (b.l - a.l) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, point, px, AppContext, Modifiers};
    use std::time::Duration;

    /// Draw one real frame of the window, the way the platform's frame callback
    /// does. The hover target is derived in prepaint, so asserting on it needs a
    /// frame; `simulate_mouse_move` alone only updates the pointer and the hit
    /// test.
    fn frame(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear());
    }

    #[test]
    fn blend_hits_both_ends_and_the_middle() {
        let a = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.2,
            a: 1.0,
        };
        let b = Hsla {
            h: 0.9,
            s: 0.5,
            l: 0.8,
            a: 1.0,
        };
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        let mid = blend(a, b, 0.5);
        assert!((mid.l - 0.5).abs() < 1e-6);
        // The hue takes the short way round: 0.9 is -0.1 from 0.0, so the
        // midpoint is -0.05, which wraps to 0.95 rather than landing at 0.45.
        assert!(
            (mid.h - 0.95).abs() < 1e-6,
            "hue took the long arc: {}",
            mid.h
        );
    }

    #[test]
    fn a_row_hover_fades_from_no_fill_to_the_wash() {
        let theme = Theme::dark();
        let hover = Hover::row(&theme);
        let (fill, ink) = hover.at(0.0);
        let fill = fill.expect("a row has a hover wash");
        assert_eq!(fill.a, 0.0, "at rest the wash is fully transparent");
        assert_eq!(ink, Some(theme.text_secondary));
        let (fill, ink) = hover.at(1.0);
        assert_eq!(fill.expect("wash").a, theme.surface_hover.a);
        assert_eq!(ink, Some(theme.text_primary));
        let (fill, _) = hover.at(0.5);
        assert!((fill.expect("wash").a - theme.surface_hover.a * 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_filled_control_cross_fades_its_two_fills_and_leaves_the_ink_alone() {
        let theme = Theme::dark();
        let hover = Hover::fill(theme.action, theme.action_hover);
        assert_eq!(hover.at(0.0).0, Some(theme.action));
        assert_eq!(hover.at(1.0).0, Some(theme.action_hover));
        assert_eq!(hover.at(0.5).1, None, "the caller's ink is not overridden");
    }

    #[test]
    fn a_wash_fades_in_from_transparent_and_keeps_the_callers_ink() {
        let theme = Theme::dark();
        let hover = Hover::wash(&theme);
        assert_eq!(hover.at(0.0).0.expect("wash").a, 0.0);
        assert_eq!(hover.at(0.5).1, None);
    }

    /// The layer's contract, without a window: the registry holds the target,
    /// and a reversal starts from the value on screen.
    #[test]
    fn a_hover_reversal_resumes_from_the_painted_value() {
        let theme = Theme::dark();
        let duration = Motion::Interaction.duration(&theme);
        let ease = Motion::Interaction.easing();
        let key = ElementId::Name("interactive-reversal".into());
        let start = Instant::now();

        let enter = Transitions::update(key.clone(), |current| {
            assert!(current.is_none(), "a fresh control is settled at rest");
            let mut transition = Transition::settled(0.0, start).easing(ease);
            let changed = transition.retarget(1.0, duration, start);
            (transition, changed)
        });
        assert!(enter, "the pointer arriving is a change");

        let halfway = start + duration / 2;
        let painted = Transitions::update(key.clone(), |current| {
            let transition = current.expect("still in flight");
            let painted = transition.value_at(halfway);
            let mut next = transition;
            let changed = next.retarget(0.0, duration, halfway);
            (next, (painted, changed))
        });
        assert!(painted.1, "leaving is a change");
        assert!(
            painted.0 > 0.0 && painted.0 < 1.0,
            "mid-fade, got {}",
            painted.0
        );
        let resumed = Transitions::get(key.clone()).expect("in flight");
        assert_eq!(
            resumed.from(),
            painted.0,
            "the reversal starts where the eye is"
        );
        assert_eq!(resumed.to(), 0.0);
        assert!(resumed.generation() >= 2, "the element id must change");

        // Repeating the same target is not a change, so no repaint is requested.
        let repeat = Transitions::update(key, |current| {
            let mut transition = current.expect("in flight");
            let changed = transition.retarget(0.0, duration, halfway);
            (transition, changed)
        });
        assert!(!repeat);
    }

    /// Hovering writes a transition; a resting pointer does not restart it, and
    /// moving off retargets it back to rest.
    #[gpui::test]
    fn hovering_a_control_starts_one_transition(cx: &mut gpui::TestAppContext) {
        struct Probe {
            theme: Theme,
        }

        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let row = div()
                    .id("hover-probe")
                    .debug_selector(|| "hover-probe".to_string())
                    .w(px(120.0))
                    .h(px(28.0));
                // Clear of the window's corner, where the pointer starts.
                div().w_full().h_full().pt(px(200.0)).child(hover(
                    row,
                    "hover-probe",
                    &self.theme,
                    Hover::row(&self.theme),
                ))
            }
        }

        let theme = Theme::dark();
        let (_probe, cx) = cx.add_window_view(|_, _| Probe { theme });
        frame(cx);
        let rested = Transitions::get("hover-probe").expect("the control mounts tracked");
        assert_eq!(rested.to(), 0.0, "the pointer is elsewhere");
        assert!(
            rested.is_done(Instant::now()),
            "mounting must not start a fade"
        );

        let row = cx.debug_bounds("hover-probe").expect("the row lays out");
        cx.simulate_mouse_move(row.center(), None, Modifiers::none());
        frame(cx);
        let entered = Transitions::get("hover-probe").expect("the hover started a transition");
        assert_eq!(entered.to(), 1.0);
        assert_eq!(entered.generation(), 1);

        cx.simulate_mouse_move(
            row.center() + point(px(1.0), px(1.0)),
            None,
            Modifiers::none(),
        );
        frame(cx);
        assert_eq!(
            Transitions::get("hover-probe")
                .expect("still in flight")
                .generation(),
            1,
            "a resting pointer must not restart the fade"
        );

        cx.simulate_mouse_move(point(px(300.0), px(40.0)), None, Modifiers::none());
        frame(cx);
        assert_eq!(
            Transitions::get("hover-probe").expect("still tracked").to(),
            0.0,
            "the pointer left, so the wash must fade out"
        );
    }

    /// Hover and selection are two values on one control: hovering must not
    /// overwrite the caller's selection, and the selection must not clear the
    /// pointer's wash.
    #[gpui::test]
    fn a_control_tracks_hover_and_selection_separately(cx: &mut gpui::TestAppContext) {
        struct Probe {
            theme: Theme,
        }

        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let segment = div()
                    .id("control-probe")
                    .debug_selector(|| "control-probe".to_string())
                    .w(px(120.0))
                    .h(px(28.0));
                control(
                    segment,
                    "control-probe",
                    &self.theme,
                    Motion::Interaction,
                    Motion::Interaction,
                    // Selected from the first frame.
                    1.0,
                    |element, _hover, _selected| element,
                )
            }
        }

        let theme = Theme::dark();
        let (_probe, cx) = cx.add_window_view(|_, _| Probe { theme });
        cx.simulate_mouse_move(point(px(300.0), px(200.0)), None, Modifiers::none());
        frame(cx);

        let selection_key = ElementId::NamedInteger("control-probe".into(), u64::MAX);
        let selection = Transitions::get(selection_key.clone()).expect("selection is tracked");
        assert_eq!(selection.to(), 1.0, "the caller's selection is the target");
        let hover = Transitions::get("control-probe").expect("the control mounts tracked");
        assert_eq!(hover.to(), 0.0, "no hover until the pointer arrives");

        let segment = cx
            .debug_bounds("control-probe")
            .expect("the segment lays out");
        cx.simulate_mouse_move(segment.center(), None, Modifiers::none());
        frame(cx);
        assert_eq!(
            Transitions::get("control-probe")
                .expect("the hover started")
                .to(),
            1.0
        );
        let selection = Transitions::get(selection_key).expect("selection survives the hover");
        assert_eq!(selection.to(), 1.0, "hovering must not clear the selection");
    }

    /// Reduce Motion never mounts the animation: the treatment swaps instead.
    #[gpui::test]
    fn reduce_motion_swaps_the_hover_instead_of_fading_it(cx: &mut gpui::TestAppContext) {
        struct Probe {
            theme: Theme,
        }

        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                let row = div()
                    .id("hover-reduced-probe")
                    .debug_selector(|| "hover-reduced-probe".to_string())
                    .w(px(120.0))
                    .h(px(28.0));
                hover(
                    row,
                    "hover-reduced-probe",
                    &self.theme,
                    Hover::row(&self.theme),
                )
            }
        }

        let mut theme = Theme::dark();
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        let cx = cx.add_empty_window();
        let _ = cx.draw(
            point(px(0.0), px(0.0)),
            gpui::size(px(400.0), px(300.0)),
            |_, cx| cx.new(|_| Probe { theme }),
        );
        let bounds = cx
            .debug_bounds("hover-reduced-probe")
            .expect("the probe lays out");
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        assert!(
            Transitions::get("hover-reduced-probe").is_none(),
            "Reduce Motion must not animate"
        );
    }

    /// A leave must clear the wash even when no listener can see it. A listener
    /// fires only on a mouse move, and diffs state it remembers; this control's
    /// geometry is what moves during a wheel scroll, so the pointer is over a
    /// row one frame and over nothing the next with no event at all.
    #[gpui::test]
    fn a_row_that_moves_out_from_under_the_pointer_loses_its_wash(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Probe {
            theme: Theme,
            offset: Rc<Cell<Pixels>>,
        }

        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                // The row starts at the window's top-left corner, 120 x 28.
                let row = div()
                    .id("hover-scroll-probe")
                    .w(px(120.0))
                    .h(px(28.0))
                    .mt(self.offset.get());
                div().child(hover(
                    row,
                    "hover-scroll-probe",
                    &self.theme,
                    Hover::row(&self.theme),
                ))
            }
        }

        let theme = Theme::dark();
        let offset = Rc::new(Cell::new(px(0.)));
        let (probe, cx) = cx.add_window_view(|_, _| Probe {
            theme,
            offset: offset.clone(),
        });
        frame(cx);

        cx.simulate_mouse_move(point(px(60.0), px(14.0)), None, Modifiers::none());
        frame(cx);
        assert_eq!(
            Transitions::get("hover-scroll-probe")
                .expect("the pointer is over the row")
                .to(),
            1.0
        );

        // The list scrolls: the row slides out from under a pointer that has not
        // moved, and nothing dispatches an enter or a leave.
        offset.set(px(100.0));
        probe.update(cx, |_, cx| cx.notify());
        frame(cx);
        frame(cx);
        assert_eq!(
            Transitions::get("hover-scroll-probe")
                .expect("the transition is still there")
                .to(),
            0.0,
            "the pointer is no longer over the row, so the wash must fade out"
        );
    }

    /// A row that scrolls back into view mounts at rest, rather than replaying
    /// the wash it had when it scrolled away: the stored transition still says
    /// "hovered", but this instance has never been on screen before.
    #[gpui::test]
    fn a_row_that_remounts_does_not_replay_its_last_wash(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;
        use std::rc::Rc;

        struct Probe {
            theme: Theme,
            shown: Rc<Cell<bool>>,
        }

        impl gpui::Render for Probe {
            fn render(
                &mut self,
                _: &mut gpui::Window,
                _: &mut gpui::Context<Self>,
            ) -> impl IntoElement {
                if !self.shown.get() {
                    return div().into_any_element();
                }
                let row = div().id("hover-remount-probe").w(px(120.0)).h(px(28.0));
                div()
                    .child(hover(
                        row,
                        "hover-remount-probe",
                        &self.theme,
                        Hover::row(&self.theme),
                    ))
                    .into_any_element()
            }
        }

        let theme = Theme::dark();
        let shown = Rc::new(Cell::new(true));
        let (probe, cx) = cx.add_window_view(|_, _| Probe {
            theme,
            shown: shown.clone(),
        });
        frame(cx);
        cx.simulate_mouse_move(point(px(60.0), px(14.0)), None, Modifiers::none());
        frame(cx);
        assert_eq!(
            Transitions::get("hover-remount-probe")
                .expect("the pointer is over the row")
                .to(),
            1.0
        );

        // The row leaves the tree (scrolled out of view, group collapsed) with
        // the wash on, then comes back with the pointer somewhere else.
        shown.set(false);
        probe.update(cx, |_, cx| cx.notify());
        frame(cx);
        cx.simulate_mouse_move(point(px(300.0), px(200.0)), None, Modifiers::none());
        shown.set(true);
        probe.update(cx, |_, cx| cx.notify());
        frame(cx);

        let remounted = Transitions::get("hover-remount-probe").expect("tracked again");
        assert_eq!(remounted.to(), 0.0, "the pointer is somewhere else");
        assert!(
            remounted.is_done(Instant::now()),
            "a fresh instance mounts at rest instead of fading from the old wash"
        );
    }

    /// The fill a frame carries is baked at render time; the target is decided
    /// in paint, from the hit test for that same frame. When the two disagree,
    /// a fade in flight repaints itself — but a *settled* transition, which is
    /// what a remount leaves behind, asks for no frames at all. The wash would
    /// then stay lit on a row that scrolled out and came back until the window
    /// happened to draw for some other reason, and a sidebar at rest does not
    /// draw.
    #[test]
    fn a_frame_that_painted_a_value_the_pointer_left_asks_for_another() {
        assert!(
            !repaint_needed(false, false, 0.0, 0.0),
            "a frame that matches its target is done"
        );
        assert!(
            !repaint_needed(false, false, 1.0, 1.0),
            "a settled wash under the pointer is done"
        );
        assert!(
            !repaint_needed(false, false, 0.0, 0.0004),
            "a difference this small is not worth a frame"
        );
        assert!(
            repaint_needed(true, false, 0.4, 1.0),
            "a fade in flight keeps asking until it lands"
        );
        assert!(
            repaint_needed(false, true, 1.0, 1.0),
            "a pumped state fade keeps its frames coming"
        );
        assert!(
            repaint_needed(false, false, 1.0, 0.0),
            "a remount that painted the wash of the instance that left must repaint itself away"
        );
    }

    /// The transition is the role's, not a number typed here.
    /// The transition is the role's, not a number typed here.
    /// The transition is the role's, not a number typed here.
    #[test]
    fn the_hover_transition_uses_the_interaction_role() {
        let theme = Theme::dark();
        assert_eq!(
            Motion::Interaction.duration(&theme),
            Duration::from_millis(150)
        );
    }
}
