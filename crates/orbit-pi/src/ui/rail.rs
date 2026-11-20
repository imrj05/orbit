//! The navigation rail: a column of short ticks, one per stop (a user turn),
//! that fan out around the hovered tick and jump on click — with fades at the
//! scrolled edges and a floating card beside it.
//!
//! The rail owns its look: tick geometry and ink, the fan-out, the edge
//! fades and the card shell. What the ticks stand for, which one is active,
//! hover state, scrolling and what the card says stay with the surface (the
//! transcript), which passes plain values in.

use gpui::{
    div, linear_color_stop, linear_gradient, px, ElementId, Hsla, InteractiveElement, Pixels,
    Stateful, Styled,
};

use crate::theme::{tokens::StyledExt, Theme};
use crate::ui::style::control::ControlExt;

/// The rail column's width; ticks are left-aligned inside it.
pub(crate) const WIDTH: f32 = 44.0;
/// A tick's full (hovered) length.
pub(crate) const TICK_WIDTH: f32 = 32.0;
/// A tick's stroke.
pub(crate) const TICK_HEIGHT: f32 = 2.0;
/// The vertical pitch of one stop: a tick plus the hit area around it.
pub(crate) const STOP_HEIGHT: f32 = 12.0;
/// Rest ink for a tick that is neither active nor hovered.
const INACTIVE_OPACITY: f32 = 0.45;
/// Length scale by distance from the hovered tick: the hovered one full, its
/// neighbours stepping down, everything else at the rest scale.
const EMPHASIS_SCALE: [f32; 4] = [1.0, 0.68, 0.44, 0.25];
/// The gap between the rail and a card beside it.
pub(crate) const CARD_GAP: f32 = 12.0;

/// A tick's length for its distance from the hovered tick (`None` when
/// nothing is hovered): the fan-out that lets a pointer find its place.
pub(crate) fn tick_width(distance_from_hover: Option<usize>) -> f32 {
    let scale = distance_from_hover
        .and_then(|distance| EMPHASIS_SCALE.get(distance))
        .copied()
        .unwrap_or(EMPHASIS_SCALE[EMPHASIS_SCALE.len() - 1]);
    TICK_WIDTH * scale
}

/// A tick's ink: primary when it is the active or hovered stop, muted
/// otherwise.
pub(crate) fn tick_ink(prominent: bool, theme: &Theme) -> Hsla {
    if prominent {
        theme.text_primary
    } else {
        theme.text_muted.opacity(INACTIVE_OPACITY)
    }
}

/// The tick's visible bar, `width` long (see [`tick_width`]). The caller
/// wraps it in the stop's hit area and attaches the jump.
pub(crate) fn tick_bar(
    id: impl Into<ElementId>,
    width: f32,
    ink: Hsla,
    theme: &Theme,
) -> Stateful<gpui::Div> {
    div()
        .id(id.into())
        .h(px(TICK_HEIGHT))
        .w(px(width))
        .rounded_full()
        .bg(ink)
        .hover_inverse(theme)
}

/// An edge fade over the rail's scroller, 20px from `page` to `clear`, so
/// ticks dissolve at a scrolled edge instead of clipping. The stops are the
/// surface's (the page fill behind the transcript, clear over glass).
pub(crate) fn edge_fade(top: bool, clear: Hsla, page: Hsla) -> gpui::Div {
    let base = div().absolute().left_0().right_0().h(px(20.));
    if top {
        base.top_0().bg(linear_gradient(
            180.,
            linear_color_stop(page, 0.),
            linear_color_stop(clear, 1.),
        ))
    } else {
        base.bottom_0().bg(linear_gradient(
            180.,
            linear_color_stop(clear, 0.),
            linear_color_stop(page, 1.),
        ))
    }
}

/// A floating card beside the rail (the hover preview, the one-time hint):
/// elevated, `width` wide, `top` from the rail's top. Content and padding are
/// the caller's.
pub(crate) fn side_card(top: Pixels, width: Pixels, theme: &Theme) -> gpui::Div {
    div()
        .absolute()
        .left(px(WIDTH + CARD_GAP))
        .top(top)
        .w(width)
        .elevation_2(theme)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_fan_out_around_the_hovered_one_and_rest_short() {
        assert_eq!(tick_width(Some(0)), TICK_WIDTH);
        assert!(tick_width(Some(1)) < tick_width(Some(0)));
        assert!(tick_width(Some(2)) < tick_width(Some(1)));
        assert_eq!(tick_width(Some(9)), tick_width(None), "far ticks rest");
        assert_eq!(tick_width(None), TICK_WIDTH * 0.25);
    }

    #[test]
    fn only_the_active_or_hovered_tick_takes_primary_ink() {
        let theme = Theme::for_id(crate::theme::ThemeId::ALL[0]);
        assert_eq!(tick_ink(true, &theme), theme.text_primary);
        assert_ne!(tick_ink(false, &theme), theme.text_primary);
    }
}
