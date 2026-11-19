//! Tabs: a horizontally scrolling strip of [`Tab`]s, each a glyph, a
//! truncating label, an optional unsaved dot and a close button.
//!
//! The tab owns its look in both states — the active fill and primary ink,
//! the resting muted ink and quiet hover, the press — and its density, which
//! follows its [`ButtonSize`]. The caller owns what the tab is (glyph, label,
//! localized close tooltip), what selecting it does (`on_click` on the returned
//! element) and what closing it does. [`tab_strip`] owns the row: one line,
//! never wrapping, scrolling sideways past its edge instead of crushing tabs.

use gpui::{
    div, px, AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement, IntoElement,
    ParentElement, Pixels, ScrollHandle, SharedString, Stateful, StatefulInteractiveElement,
    Styled, Window,
};

use crate::theme::{
    tokens::{button, ButtonSize, DynamicSpacing, TextSize},
    Theme,
};
use crate::ui::button::IconButton;
use crate::ui::style::control::{press, ControlExt};

type CloseHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// The inline padding and the glyph-label gap for a tab of `size`: the
/// terminal's Default tabs are a step tighter than the file viewer's Medium.
fn density(size: ButtonSize, theme: &Theme) -> (Pixels, Pixels) {
    match size {
        ButtonSize::Medium | ButtonSize::Large => (
            DynamicSpacing::Base08.px(theme),
            DynamicSpacing::Base06.px(theme),
        ),
        _ => (
            DynamicSpacing::Base06.px(theme),
            DynamicSpacing::Base04.px(theme),
        ),
    }
}

/// One tab.
///
/// ```ignore
/// Tab::new("viewer-tab-3", name)
///     .size(ButtonSize::Medium)
///     .glyph(file_glyph)
///     .active(index == self.active)
///     .dirty(tab.dirty)
///     .close("viewer-tab-close-3", tr!("explorer.close_tab"), on_close)
///     .render(&theme)
///     .on_click(on_select)
/// ```
pub(crate) struct Tab {
    id: ElementId,
    label: SharedString,
    glyph: Option<AnyElement>,
    size: ButtonSize,
    max_w: Pixels,
    active: bool,
    dirty: bool,
    close: Option<(ElementId, SharedString, CloseHandler)>,
}

impl Tab {
    pub(crate) fn new(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            glyph: None,
            size: ButtonSize::Default,
            max_w: px(220.),
            active: false,
            dirty: false,
            close: None,
        }
    }

    pub(crate) fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// The label truncates past this width.
    pub(crate) fn max_w(mut self, max_w: Pixels) -> Self {
        self.max_w = max_w;
        self
    }

    /// The leading mark (a file glyph, a shell profile icon), XSmall.
    pub(crate) fn glyph(mut self, glyph: impl IntoElement) -> Self {
        self.glyph = Some(glyph.into_any_element());
        self
    }

    pub(crate) fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Unsaved changes: an accent dot after the label.
    pub(crate) fn dirty(mut self, dirty: bool) -> Self {
        self.dirty = dirty;
        self
    }

    /// A close button keyed `id`, named by `tip`. Its click does not reach the
    /// tab's own `on_click`: closing shrinks the strip, and a bubbling select
    /// would run against a stale index.
    pub(crate) fn close(
        mut self,
        id: impl Into<ElementId>,
        tip: impl Into<SharedString>,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.close = Some((id.into(), tip.into(), Box::new(on_close)));
        self
    }

    /// The tab; the caller attaches `on_click` (select) and any tooltip.
    pub(crate) fn render(self, theme: &Theme) -> Stateful<gpui::Div> {
        let active = self.active;
        let (padding, gap) = density(self.size, theme);
        let ink = if active {
            theme.text_primary
        } else {
            theme.text_muted
        };
        let mut el = div()
            .id(self.id)
            .h(self.size.height(theme))
            .max_w(self.max_w)
            // Never shrink below a readable width: past the strip's edge the
            // row scrolls instead of crushing every tab.
            .flex_none()
            .px(padding)
            .flex()
            .items_center()
            .gap(gap)
            .rounded(button::RADIUS.px(theme))
            .cursor_pointer()
            .text_size(TextSize::Small.px(theme));
        el = if active {
            el.bg(theme.pressed_wash())
        } else {
            el.text_color(theme.text_muted).hover_quiet(theme)
        };
        let el = press(el)
            .children(self.glyph)
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(if active {
                        FontWeight::MEDIUM
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(ink)
                    .child(self.label),
            )
            .children(self.dirty.then(|| {
                div()
                    .size(DynamicSpacing::Base06.px(theme))
                    .flex_none()
                    .rounded_full()
                    .bg(theme.accent)
            }));
        match self.close {
            Some((id, tip, on_close)) => el.child(
                IconButton::new(id, "icons/x.svg")
                    .size(ButtonSize::None)
                    .ink(ink)
                    .tip(tip)
                    .on_click(theme, move |event, window, cx| {
                        on_close(event, window, cx);
                        cx.stop_propagation();
                    }),
            ),
            None => el,
        }
    }
}

/// The strip a run of [`Tab`]s of `size` sits in: full height, one line, its
/// own horizontal scroller tracked by `scroll` (so a caller can reveal a tab
/// with `scroll_to_item`).
pub(crate) fn tab_strip(
    id: impl Into<ElementId>,
    scroll: &ScrollHandle,
    size: ButtonSize,
    theme: &Theme,
) -> Stateful<gpui::Div> {
    div()
        .id(id.into())
        .h_full()
        .flex()
        .items_center()
        .gap(density(size, theme).1)
        .overflow_x_scroll()
        .track_scroll(scroll)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn active_tabs_fill_and_resting_tabs_stay_quiet() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let mut active = Tab::new("a", "main.rs").active(true).render(&theme);
            let mut resting = Tab::new("b", "lib.rs").render(&theme);
            assert_eq!(
                active.style().background,
                Some(theme.surface_active.into()),
                "{id:?}"
            );
            assert!(resting.style().background.is_none(), "{id:?}");
            assert_eq!(
                resting.style().text.as_ref().and_then(|text| text.color),
                Some(theme.text_muted),
                "{id:?}"
            );
        }
    }

    #[test]
    fn density_follows_the_tab_size() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let (default_pad, default_gap) = density(ButtonSize::Default, &theme);
        let (medium_pad, medium_gap) = density(ButtonSize::Medium, &theme);
        assert!(medium_pad > default_pad);
        assert!(medium_gap > default_gap);
    }
}
