//! Shared tinted SVG icon primitives.

use gpui::{Hsla, InteractiveElement, Pixels, SharedString, Styled};

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

#[cfg(test)]
mod tests {
    use super::*;

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
