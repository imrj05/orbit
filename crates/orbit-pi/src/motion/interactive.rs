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
//! GPUI's `with_animation` keys its clock to the element id and rebuilds the
//! element every frame, so the animator may restyle it but never reshape it.
//! That gives the shape used here:
//!
//! 1. `on_hover` writes a target into [`Transitions`] and bumps a generation.
//! 2. The element id carries that generation, so `with_animation` restarts at a
//!    delta of zero on every retarget.
//! 3. The animator ignores the delta and reads the transition instead, so a
//!    reversal resumes from the value on screen, and the easing lives in one
//!    place (`Transition`), not in two.
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
    prelude::*, Animation, AnimationExt, AnyElement, ElementId, Hsla, IntoElement, SharedString,
    Styled,
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
    /// secondary to primary.
    pub(crate) fn row(theme: &Theme) -> Self {
        Self {
            rest_fill: None,
            hover_fill: Some(theme.surface_hover),
            ink: Some((theme.text_secondary, theme.text_primary)),
        }
    }

    /// A control on a bare surface: the wash fades in, the ink is the caller's.
    pub(crate) fn wash(theme: &Theme) -> Self {
        Self {
            rest_fill: None,
            hover_fill: Some(theme.surface_hover),
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

    let key = ElementId::Name(name.clone());
    let event_key = key.clone();
    let ease = Motion::Interaction.easing();
    let element = element.on_hover(move |hovering, _, cx| {
        let target = if *hovering { 1.0 } else { 0.0 };
        let changed = Transitions::update(event_key.clone(), |current| {
            let now = Instant::now();
            let mut transition = current.unwrap_or_else(|| {
                // The first event for this control: it starts settled at the
                // state the pointer arrives in, so nothing animates on mount.
                Transition::settled(target, now).easing(ease)
            });
            let changed = transition.retarget(target, role.duration(&theme), now);
            (transition, changed)
        });
        if changed {
            cx.refresh_windows();
        }
    });

    let generation = Transitions::get(key.clone())
        .map(|transition| transition.generation())
        .unwrap_or(0);
    let value_key = key;
    element
        .with_animation(
            ElementId::NamedInteger(name, generation),
            // Linear on purpose: the animator reads `Transition`, which owns
            // the role's easing. Easing twice would bend the curve.
            Animation::new(role.duration(&theme)),
            move |element, _delta| {
                let value = Transitions::get(value_key.clone())
                    .map(|transition| transition.value_at(Instant::now()))
                    .unwrap_or(0.0);
                let (fill, ink) = hover.at(value);
                let element = match ink {
                    Some(ink) => element.text_color(ink),
                    None => element,
                };
                match fill {
                    Some(fill) => element.bg(fill),
                    None => element,
                }
            },
        )
        .into_any_element()
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
/// the animator both numbers in one pass.
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
    T: StatefulInteractiveElement + Styled + IntoElement + 'static,
{
    let theme = *theme;
    let name = name.into();
    let hover_key = ElementId::Name(name.clone());
    // A second key for the state, in the same store: `u64::MAX` cannot collide
    // with a generation count.
    let state_key = ElementId::NamedInteger(name.clone(), u64::MAX);

    let event_key = hover_key.clone();
    let hover_ease = hover_role.easing();
    let element = element.on_hover(move |hovering, _, cx| {
        let target = if *hovering { 1.0 } else { 0.0 };
        let changed = Transitions::update(event_key.clone(), |current| {
            let now = Instant::now();
            let mut transition =
                current.unwrap_or_else(|| Transition::settled(target, now).easing(hover_ease));
            let changed = transition.retarget(target, hover_role.duration(&theme), now);
            (transition, changed)
        });
        if changed {
            cx.refresh_windows();
        }
    });

    // The state target is the caller's, so it is retargeted here rather than by
    // a listener.
    let now = Instant::now();
    let hover =
        Transitions::get(hover_key.clone()).unwrap_or_else(|| Transition::settled(0.0, now));
    let state = Transitions::update(state_key.clone(), |current| {
        let mut transition = current
            .unwrap_or_else(|| Transition::settled(selected, now).easing(state_role.easing()));
        transition.retarget(selected, state_role.duration(&theme), now);
        (transition, transition)
    });

    if hover_role.duration(&theme).is_zero() && state_role.duration(&theme).is_zero() {
        return apply(element, hover.value_at(now), state.value_at(now)).into_any_element();
    }

    let animation_id = ElementId::Name(SharedString::from(format!(
        "{}#{}.{}",
        name,
        hover.generation(),
        state.generation()
    )));
    let (hover_key, state_key) = (
        ElementId::Name(name.clone()),
        ElementId::NamedInteger(name, u64::MAX),
    );
    element
        .with_animation(
            animation_id,
            // The pump has to outlast the slower of the two roles, or a frame
            // would freeze mid-fade.
            Animation::new(
                if hover_role.duration(&theme) > state_role.duration(&theme) {
                    hover_role.duration(&theme)
                } else {
                    state_role.duration(&theme)
                },
            ),
            move |element, _delta| {
                let now = Instant::now();
                let hover = Transitions::get(hover_key.clone())
                    .map(|transition| transition.value_at(now))
                    .unwrap_or(0.0);
                let selected = Transitions::get(state_key.clone())
                    .map(|transition| transition.value_at(now))
                    .unwrap_or(selected);
                apply(element, hover, selected)
            },
        )
        .into_any_element()
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

    /// Hovering writes a transition; hovering a second time does not restart it
    /// (a pointer resting on a row must not repaint forever).
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
                hover(row, "hover-probe", &self.theme, Hover::row(&self.theme))
            }
        }

        let theme = Theme::dark();
        let cx = cx.add_empty_window();
        let _ = cx.draw(
            point(px(0.0), px(0.0)),
            gpui::size(px(400.0), px(300.0)),
            |_, cx| cx.new(|_| Probe { theme }),
        );
        assert!(
            Transitions::get("hover-probe").is_none(),
            "nothing is in flight before the pointer arrives"
        );

        let bounds = cx.debug_bounds("hover-probe").expect("the probe lays out");
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        let entered = Transitions::get("hover-probe").expect("the hover started a transition");
        assert_eq!(entered.to(), 1.0);
        assert_eq!(entered.generation(), 1);

        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        let still = Transitions::get("hover-probe").expect("still in flight");
        assert_eq!(
            still.generation(),
            1,
            "a resting pointer must not restart the fade"
        );
    }

    /// Hover and selection are two values on one control: hovering the chosen
    /// half must not overwrite its selection, and selecting must not clear the
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
        let cx = cx.add_empty_window();
        let _ = cx.draw(
            point(px(0.0), px(0.0)),
            gpui::size(px(400.0), px(300.0)),
            |_, cx| cx.new(|_| Probe { theme }),
        );

        let selection_key = ElementId::NamedInteger("control-probe".into(), u64::MAX);
        let selection = Transitions::get(selection_key).expect("selection is tracked");
        assert_eq!(selection.to(), 1.0, "the caller's selection is the target");
        assert!(
            Transitions::get("control-probe").is_none(),
            "no hover until the pointer arrives"
        );

        let bounds = cx
            .debug_bounds("control-probe")
            .expect("the probe lays out");
        cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
        let hover = Transitions::get("control-probe").expect("the hover started");
        assert_eq!(hover.to(), 1.0);
        let selection = Transitions::get(ElementId::NamedInteger("control-probe".into(), u64::MAX))
            .expect("selection survives the hover");
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
