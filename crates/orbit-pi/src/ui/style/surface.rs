//! Material treatments that preserve the caller's existing fill and geometry.

use gpui::{Hsla, IntoElement, ParentElement, Pixels, Styled};

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

    /// The card treatment every frosted card shares: the panel corner, the
    /// frost with its `edge` hairline, and `fill` (resolved by the caller
    /// through `card_fill`, so glass off keeps the same geometry). Content
    /// and padding stay with the caller.
    fn card_surface(self, theme: &Theme, edge: Hsla, fill: impl Into<gpui::Fill>) -> Self {
        let radius = crate::theme::tokens::panel::RADIUS.px(theme);
        self.rounded(radius)
            .frosted_card(theme, edge, radius)
            .bg(fill)
    }

    /// The frame of a full page (Git, Usage, Files, Worktrees, the expanded
    /// terminal) or a dock card (Review, Explorer, session details): the panel
    /// corner and hairline, and `body` inside it.
    ///
    /// Over a full-window wallpaper the frame is glass — the contrast-floored
    /// card tint over a frosted backdrop — and the page inside paints
    /// [`Theme::page_canvas`] so the glass shows. The hairline ring is added
    /// after `body`, so nothing the page paints at its edge can cover it.
    /// Everywhere else this is the plain bordered card it always was.
    fn page_card(self, theme: &Theme, body: impl IntoElement) -> Self {
        let page = theme.for_page();
        let radius = crate::theme::tokens::panel::RADIUS.px(theme);
        let el = self.relative().rounded(radius).border_1();
        let tint = page.card_fill(theme.surface_secondary);
        match page.glass_sigma() {
            Some(sigma) => el
                .border_color(gpui::backdrop_blur_marker(sigma))
                .bg(tint)
                .child(body)
                .child(frosted_edge(theme.hairline(), radius)),
            None if page.wallpaper.active => el.border_color(theme.hairline()).bg(tint).child(body),
            None => el.border_color(theme.hairline()).child(body),
        }
    }

    /// Frost a band laid over scrolling page content (a sticky header): the
    /// rows sliding beneath it blur instead of printing through its
    /// translucent fill. Over a translucent page with the Glass setting off
    /// there is no blur to ask for, so the band takes the page's composited
    /// opaque appearance instead. Opaque pages need nothing: their rows
    /// already paint a solid fill.
    fn page_sticky_frost(self, theme: &Theme) -> Self {
        match theme.for_page().glass_sigma() {
            Some(sigma) => self.border_color(gpui::backdrop_blur_marker(sigma)),
            None if theme.page_glass() => self.bg(theme.page_appearance()),
            None => self,
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
