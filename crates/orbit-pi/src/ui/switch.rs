//! The shared switch control.

use gpui::{
    div, prelude::FluentBuilder, px, AnyElement, App, InteractiveElement, IntoElement, MouseButton,
    ParentElement, Pixels, RenderOnce, SharedString, Styled, Window,
};

use crate::theme::{
    tokens::{DynamicSpacing, Motion},
    Theme,
};
use crate::ui::style::control::ControlExt;

/// The app's one switch: a 36×20 track and a 14px knob that travels between the
/// ends on [`Motion::Interaction`], with the track cross-fading from
/// `surface_elevated` to `accent`.
///
/// The settings switches (Reduce Motion, automatic updates, a skill's or a
/// subagent's enablement) all render through this, so a switch cannot drift in
/// shape, motion or behavior. `id` must be unique per switch — it keys the
/// transition, the element and the switch's focus.
///
/// It is a keyboard control as well: Tab reaches it, it draws the focus ring
/// while focused, and Space or Enter flips it (`SwitchToggle` in the `Switch`
/// context) through the same `on_toggle` a click runs. A native accessible
/// name is still a GPUI gap.
pub(crate) fn switch(
    id: impl Into<SharedString>,
    on: bool,
    theme: &Theme,
    on_toggle: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    Switch {
        id: id.into(),
        on,
        theme: *theme,
        on_toggle: std::rc::Rc::new(on_toggle),
    }
    .into_any_element()
}

/// What a switch runs when it flips, shared by its click and key paths.
type SwitchToggleFn = std::rc::Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(IntoElement)]
struct Switch {
    id: SharedString,
    on: bool,
    theme: Theme,
    on_toggle: SwitchToggleFn,
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Switch {
            id,
            on,
            theme,
            on_toggle,
        } = self;
        // The handle has to outlive the frame so focus survives a repaint;
        // keyed state lives as long as this switch keeps being drawn.
        let focus = window
            .use_keyed_state(
                gpui::ElementId::Name(format!("{id}-focus").into()),
                cx,
                |_, cx| cx.focus_handle().tab_stop(true),
            )
            .read(cx)
            .clone();
        let focused = focus.is_focused(window);
        let padding = DynamicSpacing::Base02.px(&theme);
        let travel = switch_travel(&theme);
        let selector = id.clone();
        let track = div()
            .id(gpui::ElementId::Name(id.clone()))
            // Tests find a switch by its id.
            .debug_selector(move || selector.to_string())
            .track_focus(&focus)
            .key_context("Switch")
            .w(px(36.))
            .h(DynamicSpacing::Base20.px(&theme))
            .rounded_full()
            .p(padding)
            .border_1()
            .flex()
            .items_center()
            .cursor_pointer()
            // Focus wins over the hover edge, so the ring never blinks off
            // under the pointer.
            .when(focused, |track| track.border_color(theme.focus_ring))
            .when(!focused, |track| {
                track.border_color(theme.hairline()).hover_border(&theme)
            })
            .on_mouse_up(MouseButton::Left, {
                let on_toggle = on_toggle.clone();
                move |_, window, cx| on_toggle(window, cx)
            })
            .on_action(move |_: &crate::SwitchToggle, window, cx| on_toggle(window, cx));
        crate::motion::state(
            track,
            id,
            &theme,
            Motion::Interaction,
            if on { 1.0 } else { 0.0 },
            move |track, value| {
                track
                    .bg(crate::motion::blend(
                        theme.surface_elevated,
                        theme.accent,
                        value,
                    ))
                    .child(
                        div()
                            .flex_none()
                            .size(px(14.))
                            .rounded_full()
                            .bg(theme.toggle_knob)
                            // Relative, so the knob travels without re-laying out the
                            // track — and without moving the hitbox the click lands on.
                            .relative()
                            .left(travel * value),
                    )
            },
        )
    }
}

/// How far a switch's knob travels: the track, less its padding and the knob.
fn switch_travel(theme: &Theme) -> Pixels {
    px(36.) - DynamicSpacing::Base02.px(theme) * 2.0 - px(14.)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A switch's knob travels the track's inner span, and the span has to stay
    /// positive at every spacing density the user can choose — at Compact the
    /// padding shrinks and the knob has further to go, not less.
    #[test]
    fn a_switch_knob_travels_the_tracks_inner_span() {
        for density in crate::theme::SPACING_DENSITIES {
            let mut theme = Theme::dark();
            theme.ui.spacing_density = density;
            let travel = switch_travel(&theme);
            let padding = DynamicSpacing::Base02.px(&theme);
            assert!(
                travel > px(0.),
                "the knob would fall off the track at {density}% density"
            );
            // Track = padding + knob + travel + padding.
            assert_eq!(
                padding + px(14.) + travel + padding,
                px(36.),
                "the knob must land flush with the far edge at {density}%"
            );
        }
        // The default density is the geometry the design system states.
        assert_eq!(switch_travel(&Theme::dark()), px(18.));
    }
}
