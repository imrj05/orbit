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

    /// A floating card with a caller-owned semantic edge and base surface.
    /// The tint and backdrop follow the same rules as menus and the composer.
    fn floating_card_surface(self, theme: &Theme, edge: Hsla, base: Hsla) -> Self {
        let radius = crate::theme::tokens::panel::RADIUS.px(theme);
        self.rounded(radius)
            .frosted_floating_card(theme, edge, radius)
            .bg(theme.floating_fill(base))
    }

    /// A settings or Usage board: the page-scoped wallpaper treatment, or
    /// contrast-controlled floating glass over a bare dark workbench.
    fn board_surface(self, theme: &Theme, base: Hsla) -> Self {
        let page = theme.for_page();
        let radius = crate::theme::tokens::panel::RADIUS.px(&page);
        let fill = if page.wallpaper.active {
            page.card_fill(base)
        } else {
            page.inspector_fill(base)
        };
        self.rounded(radius)
            .frosted_floating_card(&page, page.hairline(), radius)
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

    /// A band pinned over a frosted structural pane's scrolling rows (the
    /// sidebar's sticky project header). Under glass it takes the pane's own
    /// translucent fill and frosts the rows sliding beneath, so it reads as
    /// the same material as the rail around it. Without glass there is no
    /// blur to hide those rows, so it keeps the opaque `base`.
    fn pane_sticky_frost(self, theme: &Theme, base: Hsla) -> Self {
        match theme.glass_sigma() {
            Some(sigma) => self
                .bg(theme.panel_fill(base))
                .border_color(gpui::backdrop_blur_marker(sigma)),
            None => self.bg(base),
        }
    }

    /// A rounded card with no edge (the reader's message bubble): the panel
    /// corner and [`Theme::glass_fill`] of `base`, frosted only while
    /// wallpaper glass is on. Unlike [`Self::card_surface`] it draws no
    /// hairline; unlike [`Self::frosted_pane`] it is card-shaped.
    fn edgeless_card(self, theme: &Theme, base: Hsla) -> Self {
        let el = self
            .rounded(crate::theme::tokens::panel::RADIUS.px(theme))
            .bg(theme.glass_fill(base));
        match theme.glass_sigma() {
            Some(sigma) => el.border_color(gpui::backdrop_blur_marker(sigma)),
            None => el,
        }
    }

    /// An inspector card in the transcript (a tool call, a thought, a
    /// delegation): the panel corner, frosted under the inspector rule
    /// ([`Theme::inspector_fill`]). It draws **no outline and no edge
    /// highlight**: the tint and the frost carry the shape (an opaque card
    /// keeps the `overlay` plate), so a column of cards reads as one material
    /// instead of a stack of boxes. The 1px border is kept, transparent,
    /// because its channel carries the blur request and keeps a frosted and an
    /// opaque card the same size. `fill` comes from the caller through
    /// `inspector_fill`.
    fn inspector_card(self, theme: &Theme, fill: impl Into<gpui::Fill>) -> Self {
        let radius = crate::theme::tokens::panel::RADIUS.px(theme);
        let el = self.rounded(radius).border_1();
        let el = match theme.floating_glass_sigma() {
            Some(sigma) => el.border_color(gpui::backdrop_blur_marker(sigma)),
            None => el.border_color(gpui::transparent_black()),
        };
        el.bg(fill)
    }

    /// The lift an inspector card takes while its detail is open: the larger,
    /// thicker surface reads as raised over the turn. Only while it frosts —
    /// an opaque card stays flat, like the rest of the transcript.
    fn inspector_lift(self, theme: &Theme, open: bool) -> Self {
        if open && theme.floating_glass_sigma().is_some() {
            self.shadow(
                crate::theme::tokens::ElevationIndex::ElevatedSurface.shadow(theme),
            )
        } else {
            self
        }
    }

    /// The frame of a right-dock card (session details, Review, Explorer).
    /// Over a full-window wallpaper it is the page glass; with no wallpaper
    /// it is inspector glass ([`Theme::dock_glass`]); otherwise the plain
    /// bordered card. The dock's root paints [`Theme::dock_canvas`] so the
    /// glass shows through it.
    fn dock_card(self, theme: &Theme, body: impl IntoElement) -> Self {
        let radius = crate::theme::tokens::panel::RADIUS.px(theme);
        match theme.floating_glass_sigma() {
            Some(sigma) if theme.dock_glass() && !theme.page_glass() => self
                .relative()
                .rounded(radius)
                .border_1()
                .border_color(gpui::backdrop_blur_marker(sigma))
                .bg(theme.inspector_fill(theme.surface))
                .child(body)
                .child(frosted_edge(theme.hairline(), radius)),
            _ => self.page_card(theme, body),
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

/// The corner of a child that reaches an inspector card's edge (its header
/// wash, an error strip, the detail plane): the card's radius less its 1px
/// border, so the inner curve runs concentric with the outer one instead of
/// cutting a tighter corner inside it.
pub(crate) fn inspector_inner_radius(theme: &Theme) -> Pixels {
    crate::theme::tokens::panel::RADIUS.px(theme) - crate::theme::tokens::BORDER_WIDTH
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::SurfaceTreatment;
    use gpui::{div, Styled};

    fn treatment(active: bool, full_window: bool, glass: bool) -> SurfaceTreatment {
        SurfaceTreatment {
            active,
            full_window,
            glass,
            card_alpha: 0.8,
            panel_alpha: 0.7,
            ..SurfaceTreatment::OPAQUE
        }
    }

    #[test]
    fn floating_card_preserves_its_tint_and_edge_with_and_without_frost() {
        for theme in [
            Theme::dark(),
            Theme::light(),
            Theme::dark().with_wallpaper(treatment(true, true, true)),
            Theme::dark().with_wallpaper(treatment(true, true, false)),
        ] {
            let edge = theme.accent;
            let mut card = div().floating_card_surface(&theme, edge, theme.surface_elevated);
            let style = card.style();
            assert_eq!(
                style.background,
                Some(theme.floating_fill(theme.surface_elevated).into())
            );
            assert_eq!(
                style.border_color,
                Some(match theme.floating_glass_sigma() {
                    Some(sigma) => gpui::backdrop_blur_marker(sigma),
                    None => edge,
                })
            );
        }
    }

    #[test]
    fn board_material_ignores_hero_wallpaper_and_respects_full_window_setting() {
        let base = Theme::dark();
        let hero = base.with_wallpaper(treatment(true, false, true));
        let mut hero_board = div().board_surface(&hero, hero.surface_secondary);
        let mut bare_board = div().board_surface(&base, base.surface_secondary);
        assert_eq!(hero_board.style().background, bare_board.style().background);
        assert_eq!(hero_board.style().border_color, bare_board.style().border_color);

        let full = base.with_wallpaper(treatment(true, true, false));
        let mut full_board = div().board_surface(&full, full.surface_secondary);
        let style = full_board.style();
        assert_eq!(
            style.background,
            Some(full.card_fill(full.surface_secondary).into())
        );
        assert_eq!(style.border_color, Some(full.hairline()));
    }

    /// The sidebar's sticky header is the rail's material: the rail's
    /// translucent fill under glass, the opaque surface everywhere else, so
    /// rows scrolling beneath never print through a translucent band.
    #[test]
    fn pane_sticky_frost_matches_the_rail_and_stays_opaque_without_glass() {
        for base in [Theme::dark(), Theme::light()] {
            for (active, full_window, glass) in [
                (false, false, false),
                (true, true, false),
                (true, false, true),
                (true, true, true),
            ] {
                let theme = base.with_wallpaper(treatment(active, full_window, glass));
                let mut band = div().pane_sticky_frost(&theme, theme.surface);
                let expected = if theme.glass_sigma().is_some() {
                    theme.panel_fill(theme.surface)
                } else {
                    theme.surface
                };
                assert_eq!(band.style().background, Some(expected.into()));
                if active && full_window && glass {
                    assert!(expected.a < 1.0, "glass lets the frost through");
                }
            }
        }
    }

    /// A child reaching an inspector card's edge nests its corner inside the
    /// 1px border (outer = inner + border), at every corner preference.
    #[test]
    fn inspector_children_run_concentric_with_the_card() {
        let theme = Theme::dark();
        let outer = crate::theme::tokens::panel::RADIUS.px(&theme);
        assert_eq!(
            inspector_inner_radius(&theme) + crate::theme::tokens::BORDER_WIDTH,
            outer
        );
    }

    /// An open card lifts only while it frosts; an opaque card, or a closed
    /// one, stays flat like the rest of the transcript.
    #[test]
    fn inspector_lift_only_raises_an_open_frosted_card() {
        let lifted = crate::theme::tokens::ElevationIndex::ElevatedSurface;
        for base in [Theme::dark(), Theme::light()] {
            for glass in [false, true] {
                let theme = base.with_wallpaper(treatment(true, true, glass));
                for open in [false, true] {
                    let mut card = div().inspector_lift(&theme, open);
                    let shadows = card.style().box_shadow.clone();
                    if open && theme.floating_glass_sigma().is_some() {
                        assert_eq!(shadows, Some(lifted.shadow(&theme)));
                    } else {
                        assert_eq!(shadows, None, "flat: open {open}, glass {glass}");
                    }
                }
            }
        }
    }

    /// A transcript inspector card draws no outline in any mode: frosted, its
    /// border channel only carries the blur request; opaque, the 1px border is
    /// transparent so the card keeps its size. No visible hairline either way.
    #[test]
    fn inspector_cards_draw_no_outline() {
        for base in [Theme::dark(), Theme::light()] {
            for (active, glass) in [(false, false), (true, false), (true, true)] {
                let theme = base.with_wallpaper(treatment(active, true, glass));
                let mut card = div().inspector_card(&theme, theme.overlay);
                let edge = card.style().border_color;
                match theme.floating_glass_sigma() {
                    Some(sigma) => {
                        assert_eq!(edge, Some(gpui::backdrop_blur_marker(sigma)))
                    }
                    None => assert_eq!(edge, Some(gpui::transparent_black())),
                }
                assert_ne!(edge, Some(theme.hairline()), "no hairline outline");
            }
        }
    }

    /// Settings sections that own their layout (Skills, Subagents) must stay
    /// clear like `settings_body`, and the surfaces this guard names must keep
    /// their glass-aware fills instead of drifting back to opaque slabs.
    #[test]
    fn glass_surfaces_do_not_paint_opaque_slabs() {
        let skills = include_str!("../../app/skills_ui.rs");
        let subagents = include_str!("../../app/subagents_ui.rs");
        for (name, source, root) in [
            ("skills", skills, "fn render_skills_page("),
            ("subagents", subagents, "fn render_subagents_page("),
        ] {
            let start = source.find(root).expect("page root");
            let body = &source[start..start + 900];
            assert!(!body.contains(".bg(theme.background)"), "{name} page root is opaque");
            assert!(
                source.contains(".bg(theme.panel_fill(theme.surface))\n            .frosted_pane(&theme.for_page())"),
                "{name} list column is the Settings nav's twin"
            );
        }
        let view = include_str!("../../app/view.rs");
        assert!(view.contains(".pane_sticky_frost(&theme, theme.surface)"));
        let usage = include_str!("../../usage/view.rs");
        let start = usage.find("fn skeleton(").expect("usage skeleton");
        let skeleton = &usage[start..start + 6000];
        assert!(!skeleton.contains(".bg(theme.surface_secondary)"));
    }

    /// The bubble shape: glass tint and frost only while glass is on, the
    /// opaque base everywhere else, and never a hairline of its own.
    #[test]
    fn edgeless_card_is_glass_only_with_active_glass() {
        for base in [Theme::dark(), Theme::light()] {
            for (active, glass) in [(false, false), (false, true), (true, false), (true, true)] {
                let theme = base.with_wallpaper(treatment(active, true, glass));
                let mut card = div().edgeless_card(&theme, theme.surface_elevated);
                let style = card.style();
                assert_eq!(style.background, Some(theme.glass_fill(theme.surface_elevated).into()));
                assert!(style.border_widths.left.is_none(), "no hairline");
                if active && glass {
                    assert!(theme.glass_fill(theme.surface_elevated).a < 1.0);
                } else {
                    assert_eq!(theme.glass_fill(theme.surface_elevated), theme.surface_elevated);
                }
            }
        }
    }

    /// Material decided at a call site instead of by the theme: hand-mixed
    /// modal scrims, a page-canvas film over the Git page, container opacity
    /// on the sidebar drag preview, and a hand-rolled `card_surface`.
    #[test]
    fn material_stays_with_the_shared_helpers() {
        for (name, source) in [
            ("settings", include_str!("../../app/settings.rs")),
            ("mcp", include_str!("../../app/mcp_ui.rs")),
            ("accounts", include_str!("../../app/provider_accounts.rs")),
        ] {
            assert!(!source.contains("a: 0.42"), "{name} mixes its own modal scrim");
        }
        let git = include_str!("../../git_panel/mod.rs");
        assert!(!git.contains("theme.background.opacity("));
        let sidebar = include_str!("../../app/sidebar.rs");
        let start = sidebar.find("fn drag_ghost(").expect("drag ghost");
        let ghost = &sidebar[start..start + 900];
        assert!(ghost.contains(".elevation_2(&theme)") && !ghost.contains(".opacity("));
        let settings = include_str!("../../app/settings.rs");
        let start = settings.find("fn empty_resource_card(").expect("empty card");
        assert!(settings[start..start + 700].contains(".board_surface("));
        // The open Settings section is a selection, not a hover.
        let start = settings.find("let row = div()\n                                        .id(ElementId::Name(format!(\"settings-nav-").expect("nav row");
        assert!(settings[start..start + 1500].contains(".bg(theme.selected_wash())"));
        // Switches are the shared one; the chrome's chip colours are theme roles.
        let mcp = include_str!("../../app/mcp_ui.rs");
        for (source, root) in [(mcp, "fn mcp_switch("), (settings, "fn sidebar_toggle(")] {
            let start = source.find(root).expect("switch fn");
            let body = &source[start..start + 700];
            assert!(body.contains("switch(") && !body.contains("toggle_knob"), "{root} is hand-built");
        }
        let chrome = include_str!("../chrome.rs");
        assert!(!chrome.contains(".a *="));
    }
}
