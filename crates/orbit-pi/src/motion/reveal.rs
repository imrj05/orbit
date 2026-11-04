//! The two shapes a surface needs beyond a value transition: one that grows a
//! section out of nothing, and one that moves a card out of its anchor.
//!
//! [`Disclosure`] exists because a height read from a previous layout is a
//! guess. A section's height depends on the width it wraps at, so a panel being
//! resized while its section opens would ease from a stale measurement and snap
//! when it settled. The element measures its child at the *current* width on
//! every frame and interpolates that, so the height it eases from is always the
//! one it is about to reach.
//!
//! [`Travel`] is the offset half of a popover or dialog entrance: a card starts
//! `distance` toward its anchor and travels away from it as it fades in, so the
//! eye reads the card as arriving from the control that opened it.

use gpui::{
    point, px, Animation, AnimationExt, AnyElement, App, AvailableSpace, Bounds, Corner, Element,
    ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId, Pixels, Point,
    SharedString, Size, Style, Styled, Window,
};

use crate::theme::tokens::Motion;
use crate::theme::Theme;

/// A section that grows from a fraction of its measured height.
///
/// `progress` is the eased 0..1 the caller drives — a [`super::Transition`] for
/// a one-shot open, or a ratio for a drag. It is a *fraction of the child's
/// current natural height*, not a pixel value, so the same progress means the
/// same reveal at any width.
pub(crate) struct Disclosure {
    child: AnyElement,
    width: Pixels,
    progress: f32,
}

impl Disclosure {
    pub(crate) fn new(child: impl IntoElement, width: Pixels, progress: f32) -> Self {
        Self {
            child: child.into_any_element(),
            width,
            progress: progress.clamp(0.0, 1.0),
        }
    }
}

impl IntoElement for Disclosure {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Disclosure {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        // The child is measured at the width it will actually be painted at,
        // before the height is chosen. This is the whole point of the element.
        let natural = self.child.layout_as_root(
            Size {
                width: AvailableSpace::Definite(self.width),
                height: AvailableSpace::MinContent,
            },
            window,
            cx,
        );
        let mut style = Style::default();
        style.size.width = self.width.into();
        style.size.height = (natural.height * self.progress).into();
        (window.request_layout(style, [], cx), ())
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
        self.child.prepaint_at(bounds.origin, window, cx);
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // Clip to the revealed height, so the part of the child that has not
        // been disclosed yet cannot paint over what follows it.
        window.with_content_mask(
            Some(gpui::ContentMask {
                bounds: Bounds {
                    origin: bounds.origin,
                    size: Size {
                        width: bounds.size.width,
                        height: bounds.size.height,
                    },
                },
            }),
            |window| self.child.paint(window, cx),
        );
    }
}

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
    use gpui::prelude::*;
    use gpui::{div, point, px, AppContext};

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

    /// A disclosure's height follows the width it is measured at, so a resize
    /// cannot make it settle to an estimate taken at another width.
    #[gpui::test]
    fn a_disclosure_measures_at_the_current_width(cx: &mut gpui::TestAppContext) {
        struct Probe {
            width: Pixels,
            progress: f32,
        }

        impl gpui::Render for Probe {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                // Ten 100 px cells that wrap: 600 px lays out 6 per row (two
                // rows), 250 px lays out 2 per row (five rows).
                let cells = (0..10).map(|ix| {
                    div()
                        .w(px(100.0))
                        .h(px(40.0))
                        .flex_none()
                        .child(ix.to_string())
                });
                let child = div().w_full().flex().flex_wrap().children(cells);
                div()
                    .id("disclosure-probe")
                    .debug_selector(|| "disclosure-probe".to_string())
                    .w(self.width)
                    .child(Disclosure::new(child, self.width, self.progress))
            }
        }

        for (width, natural) in [(600.0_f32, 80.0_f32), (250.0, 200.0)] {
            let width = px(width);
            let window = cx.add_empty_window();
            let _ = window.draw(
                point(px(0.0), px(0.0)),
                gpui::size(px(1200.0), px(800.0)),
                |_, cx| {
                    cx.new(|_| Probe {
                        width,
                        progress: 0.5,
                    })
                },
            );
            let bounds = window
                .debug_bounds("disclosure-probe")
                .expect("the probe lays out");
            assert!(
                (f32::from(bounds.size.height) - natural * 0.5).abs() < 1.0,
                "at {width:?}, half of {natural} px was expected, got {:?}",
                bounds.size.height
            );
        }
    }
}
