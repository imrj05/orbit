//! Shared tinted SVG icon primitives.

use gpui::{
    radians, AnyElement, Hsla, InteractiveElement, Pixels, SharedString, Styled, Transformation,
};

use crate::theme::{tokens::Motion, Theme};

/// Hover group every action button opts into with `.group(BUTTON_GROUP)`.
/// Any [`icon`] descendant then lifts its ink while that button is hovered —
/// the one hook that gives the whole app's buttons a consistent hover cue
/// without threading a theme or a unique group name through each one.
pub(crate) const BUTTON_GROUP: &str = "orbit-button";

/// How far an icon lightens toward white when its button is hovered. Small
/// enough that semantic icons (accent, danger) stay on-brand; enough that a
/// muted `text_secondary`/`text_muted` glyph visibly brightens.
const ICON_HOVER_LIGHTEN: f32 = 0.12;

/// The hover ink for an icon: the same hue, lightened toward white.
fn icon_hover_ink(color: Hsla) -> Hsla {
    Hsla {
        l: (color.l + ICON_HOVER_LIGHTEN).min(1.0),
        ..color
    }
}

/// Render an embedded HugeIcons SVG tinted with the given color.
///
/// `flex_none` is load-bearing: an SVG defaults to `flex-shrink: 1`, so in a
/// flex row beside wide text it collapses to zero width (the "icons render
/// tiny" bug). Icons must always keep their declared size.
///
/// Every icon also lifts its ink toward white while an ancestor wearing
/// [`BUTTON_GROUP`] is hovered. GPUI resolves a hover group through a balanced
/// push/pop stack, so one shared group name is safe across siblings and nested
/// buttons — the nearest opt-in button wins. An icon outside any such button
/// sees no group and stays inert, so decorative glyphs never react.
///
/// `size` is px: a literal, or a token such as `IconSize::Small.px(&theme)`.
pub(crate) fn icon(path: &'static str, size: impl Into<Pixels>, color: Hsla) -> gpui::Svg {
    let hover = icon_hover_ink(color);
    gpui::svg()
        .path(path)
        .flex_none()
        .size(size.into())
        .text_color(color)
        .group_hover(BUTTON_GROUP, move |style| style.text_color(hover))
}

/// Same as [`icon`] but for runtime-computed paths (per-provider marks).
pub(crate) fn icon_dyn(path: SharedString, size: impl Into<Pixels>, color: Hsla) -> gpui::Svg {
    let hover = icon_hover_ink(color);
    gpui::svg()
        .path(path)
        .flex_none()
        .size(size.into())
        .text_color(color)
        .group_hover(BUTTON_GROUP, move |style| style.text_color(hover))
}

/// The app's one disclosure chevron: `chevron-right`, turning a quarter turn to
/// point down while `open`, on [`Motion::Interaction`].
///
/// One glyph rotates instead of two glyphs swapping, because a swap cannot be
/// animated — the eye reads a jump where the shape should carry through. Reduce
/// Motion turns it instantly, which is the same picture without the travel.
///
/// `id` must be unique per chevron: it keys the transition.
pub(crate) fn disclosure_chevron(
    id: impl Into<SharedString>,
    open: bool,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: &Theme,
) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/chevron-right.svg")
        .flex_none()
        .size(size.into())
        .text_color(color);
    crate::motion::state(
        svg,
        id,
        theme,
        Motion::Interaction,
        if open { 1.0 } else { 0.0 },
        |svg, value| svg.with_transformation(Transformation::rotate(radians(chevron_angle(value)))),
    )
}

/// The angle for a point on the chevron's turn: 0 while closed, a quarter turn
/// (90°) while open.
fn chevron_angle(value: f32) -> f32 {
    std::f32::consts::FRAC_PI_2 * value.clamp(0.0, 1.0)
}

/// The caret on a picker trigger: `chevron-down` at rest, turning a half turn
/// to point up while its menu is open.
///
/// This is deliberately not [`disclosure_chevron`]: a tree row's right/down pair
/// means collapsed/expanded, while a menu button that opens *below* its trigger
/// points down at rest. Using the disclosure glyph for both made every composer
/// chip read as a breadcrumb (`Full access ›`, `Build ›`, `Work in … ›`) even
/// though each one opens a picker in place.
///
/// One glyph rotates instead of two glyphs swapping, on the same
/// [`Motion::Interaction`] role and with the same Reduce Motion result.
pub(crate) fn dropdown_chevron(
    id: impl Into<SharedString>,
    open: bool,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: &Theme,
) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/chevron-down.svg")
        .flex_none()
        .size(size.into())
        .text_color(color);
    crate::motion::state(
        svg,
        id,
        theme,
        Motion::Interaction,
        if open { 1.0 } else { 0.0 },
        |svg, value| {
            svg.with_transformation(Transformation::rotate(radians(dropdown_angle(value))))
        },
    )
}

/// The angle for a point on a dropdown caret's turn: 0 while closed (pointing
/// down), a half turn (180°) while open (pointing up).
fn dropdown_angle(value: f32) -> f32 {
    std::f32::consts::PI * value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The chevron's turn is a quarter turn and nothing else: the same glyph in
    /// the same place, so the row does not appear to swap icons.
    #[test]
    fn a_disclosure_chevron_turns_exactly_a_quarter_turn() {
        assert_eq!(chevron_angle(0.0), 0.0, "closed points right");
        assert!(
            (chevron_angle(1.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "open points down"
        );
        assert!(
            (chevron_angle(0.5) - std::f32::consts::FRAC_PI_4).abs() < 1e-6,
            "halfway is halfway"
        );
        // Clamped, so a delayed frame cannot over-rotate the glyph.
        assert!((chevron_angle(2.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(chevron_angle(-1.0), 0.0);
    }

    /// A picker's caret starts pointing down and turns a half turn to point up;
    /// a quarter turn would leave it pointing right or down, which is the
    /// disclosure language, not the dropdown one.
    #[test]
    fn a_dropdown_caret_turns_exactly_a_half_turn() {
        assert_eq!(dropdown_angle(0.0), 0.0, "closed points down");
        assert!(
            (dropdown_angle(1.0) - std::f32::consts::PI).abs() < 1e-6,
            "open points up"
        );
        assert!(
            (dropdown_angle(0.5) - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "halfway is halfway"
        );
        assert!((dropdown_angle(2.0) - std::f32::consts::PI).abs() < 1e-6);
        assert_eq!(dropdown_angle(-1.0), 0.0);
    }

    #[test]
    fn hover_ink_lightens_without_changing_other_channels_and_clamps() {
        let color = Hsla {
            h: 0.3,
            s: 0.4,
            l: 0.5,
            a: 0.6,
        };
        let hover = icon_hover_ink(color);
        assert!((hover.l - 0.62).abs() < f32::EPSILON);
        assert_eq!((hover.h, hover.s, hover.a), (color.h, color.s, color.a));
        assert_eq!(icon_hover_ink(Hsla { l: 0.95, ..color }).l, 1.0);
    }
}
