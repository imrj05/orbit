//! Material treatments that preserve the caller's existing fill and geometry.

use gpui::{Hsla, ParentElement, Pixels, Styled};

use crate::theme::{tokens::frosted_edge, Theme};

pub(crate) trait SurfaceExt: Styled + ParentElement + Sized {
    /// Preserve a card's hairline while its border channel requests frost.
    /// The caller chooses the fill and radius, including the non-glass fallback.
    fn frosted_card(self, theme: &Theme, border: Hsla, radius: Pixels) -> Self {
        let el = self.border_1();
        if let Some(sigma) = theme.glass_sigma() {
            el.border_color(gpui::backdrop_blur_marker(sigma))
                .child(frosted_edge(border, radius))
        } else {
            el.border_color(border)
        }
    }

    /// [`Self::frosted_card`] for a floating control (an `elevation_2` layer
    /// with its own radius): frosts under the floating rule, so it also frosts
    /// the workbench in dark palettes with no wallpaper.
    fn frosted_floating_card(self, theme: &Theme, border: Hsla, radius: Pixels) -> Self {
        let el = self.border_1();
        if let Some(sigma) = theme.floating_glass_sigma() {
            el.border_color(gpui::backdrop_blur_marker(sigma))
                .child(frosted_edge(border, radius))
        } else {
            el.border_color(border)
        }
    }

    /// Frost an edge-less structural pane without adding a border or hairline.
    fn frosted_pane(self, theme: &Theme) -> Self {
        if let Some(sigma) = theme.glass_sigma() {
            self.border_color(gpui::backdrop_blur_marker(sigma))
        } else {
            self
        }
    }
}

impl<E: Styled + ParentElement> SurfaceExt for E {}
