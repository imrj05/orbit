//! A surface arriving: the entrance fade and the travel out of its anchor.
//!
//! [`Travel`] is the offset half of a popover or dialog entrance: a card starts
//! `distance` toward its anchor and travels away from it as it fades in, so the
//! eye reads the card as arriving from the control that opened it.

use gpui::{
    point, px, Animation, AnimationExt, AnyElement, Corner, ElementId, IntoElement, Pixels, Point,
    SharedString, Styled,
};

use crate::theme::tokens::Motion;
use crate::theme::Theme;

/// The offset a card enters from, relative to where it will rest.
///
/// `corner` is the corner of the card anchored to its trigger, which is all the
/// direction the layer needs: a card anchored by its top edge rests *below* the
/// trigger and therefore starts above its resting spot, and one anchored by its
/// bottom edge does the opposite. `progress` is 0 at the start of the entrance
/// and 1 at rest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Travel {
    pub(crate) corner: Corner,
    pub(crate) distance: Pixels,
}

impl Travel {
    /// The menu travel `DESIGN.md` names for popovers and dialogs.
    pub(crate) const DISTANCE: Pixels = px(8.0);

    pub(crate) fn new(corner: Corner) -> Self {
        Self {
            corner,
            distance: Self::DISTANCE,
        }
    }

    pub(crate) fn offset(&self, progress: f32) -> Point<Pixels> {
        let remaining = 1.0 - progress.clamp(0.0, 1.0);
        // Vertical only: every menu opens above or below its control, and a
        // sideways drift reads as sliding rather than arriving.
        let y = match self.corner {
            Corner::TopLeft | Corner::TopRight => -1.0,
            Corner::BottomLeft | Corner::BottomRight => 1.0,
        };
        point(px(0.0), self.distance * remaining * y)
    }
}

/// A surface arriving: fade in on `role`, and travel out of `corner` when one
/// is given.
///
/// The element's animation state lives in gpui's `element_states`, which is
/// dropped when the element leaves the tree — so a closed dialog and the one
/// that replaces it each get their own entrance. Nothing here needs a registry
/// or a mount counter.
///
/// `name` must be stable while the surface is mounted and distinct from any
/// other animated element at the same tree position.
pub(crate) fn enter<T>(
    element: T,
    name: impl Into<SharedString>,
    theme: &Theme,
    role: Motion,
    corner: Option<Corner>,
) -> AnyElement
where
    T: Styled + IntoElement + 'static,
{
    // Reduce Motion paints the arrived state: no animation element is mounted,
    // so the surface is also never offset by the travel.
    if role.duration(theme).is_zero() {
        return element.into_any_element();
    }
    let travel = corner.map(Travel::new);
    element
        .with_animation(
            ElementId::NamedInteger(name.into(), 0),
            Animation::new(role.duration(theme)).with_easing(role.easing()),
            move |element, progress| {
                let (opacity, offset) = entrance_frame(progress, travel);
                let element = element.opacity(opacity);
                match travel {
                    // Relative offsets move the paint without moving the layout,
                    // so the anchored frame stays where the anchor put it.
                    Some(_) => element.relative().left(offset.x).top(offset.y),
                    None => element,
                }
            },
        )
        .into_any_element()
}

/// The paint for a point on an entrance: its opacity, and the offset it still
/// has to travel.
fn entrance_frame(progress: f32, travel: Option<Travel>) -> (f32, Point<Pixels>) {
    let progress = progress.clamp(0.0, 1.0);
    let offset = travel
        .map(|travel| travel.offset(progress))
        .unwrap_or(point(px(0.0), px(0.0)));
    (progress, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn travel_moves_a_card_out_of_its_anchor_and_rests_at_zero() {
        for (corner, direction) in [
            (Corner::TopLeft, -1.0),
            (Corner::TopRight, -1.0),
            (Corner::BottomLeft, 1.0),
            (Corner::BottomRight, 1.0),
        ] {
            let travel = Travel::new(corner);
            assert_eq!(
                travel.offset(0.0),
                point(px(0.0), Travel::DISTANCE * direction),
                "{corner:?} enters from the wrong side of its anchor"
            );
            // Vertical only: a menu does not drift sideways as it arrives.
            assert_eq!(travel.offset(0.0).x, px(0.0));
            assert_eq!(travel.offset(1.0), point(px(0.0), px(0.0)));
            assert_eq!(travel.offset(2.0), point(px(0.0), px(0.0)), "clamped");
            assert_eq!(
                travel.offset(0.5),
                point(px(0.0), Travel::DISTANCE * 0.5 * direction)
            );
        }
    }

    #[test]
    fn an_entrance_fades_from_nothing_and_settles_at_rest() {
        let travel = Travel::new(Corner::BottomLeft);
        let (opacity, offset) = entrance_frame(0.0, Some(travel));
        assert_eq!(opacity, 0.0, "a surface starts invisible");
        assert_eq!(
            offset,
            point(px(0.0), Travel::DISTANCE),
            "it starts toward its anchor"
        );
        let (opacity, offset) = entrance_frame(1.0, Some(travel));
        assert_eq!(opacity, 1.0);
        assert_eq!(offset, point(px(0.0), px(0.0)), "it rests where it belongs");
        // Out-of-range progress is clamped, so a delayed first frame cannot
        // paint an offset nobody asked for.
        let (opacity, offset) = entrance_frame(2.0, Some(travel));
        assert_eq!((opacity, offset), (1.0, point(px(0.0), px(0.0))));
    }

    #[test]
    fn an_entrance_without_travel_only_fades() {
        let (opacity, offset) = entrance_frame(0.5, None);
        assert_eq!(opacity, 0.5);
        assert_eq!(offset, point(px(0.0), px(0.0)));
    }

}
