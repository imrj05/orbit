//! The status bar under the composer: one quiet line of where the session
//! runs (folder, host, branch) on the left and what is happening (a
//! transient message, send hints, the context meter) on the right.
//!
//! The bar owns the row — its rhythm, the Small muted type every item
//! inherits, the split between leading and trailing items, and the opaque
//! fill that protects the ink over a bright hero wallpaper — and the three
//! item shapes it carries. The surface owns what each item says and does.

use gpui::{
    div, px, AnyElement, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    Stateful, Styled,
};

use crate::theme::{
    tokens::{ButtonSize, DynamicSpacing, TextSize},
    Theme,
};
use crate::ui::button::button_frame;
use crate::ui::icon::icon;
use crate::ui::style::control::{ControlExt, PressedRamp, StatefulControlExt, TipExt};

/// The bar. Items keep their insertion order within each side.
#[derive(Default)]
pub(crate) struct StatusBar {
    leading: Vec<AnyElement>,
    trailing: Vec<AnyElement>,
    protect: bool,
}

impl StatusBar {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// An item on the left: where the session runs.
    pub(crate) fn leading(mut self, item: Option<impl IntoElement>) -> Self {
        self.leading.extend(item.map(IntoElement::into_any_element));
        self
    }

    /// An item on the right: what is happening.
    pub(crate) fn trailing(mut self, item: Option<impl IntoElement>) -> Self {
        self.trailing
            .extend(item.map(IntoElement::into_any_element));
        self
    }

    /// Paint the page background behind the line, for a hero wallpaper that
    /// may be bright where the bar sits (dimming the whole image instead
    /// would flatten it).
    pub(crate) fn protect(mut self, protect: bool) -> Self {
        self.protect = protect;
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> gpui::Div {
        let mut row = div()
            .pt_1p5()
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .text_size(TextSize::Small.px(theme))
            .text_color(theme.text_muted);
        // Keep the spacing, but don't paint a strip when there's no ink to protect.
        if self.protect && !(self.leading.is_empty() && self.trailing.is_empty()) {
            row = row.bg(theme.background);
        }
        row.children(self.leading)
            .child(div().flex_1())
            .children(self.trailing)
    }
}

/// A static item: a muted glyph and a label, with the detail in its tooltip
/// (a remote host, whose `ssh:dir` would not fit the line).
pub(crate) fn status_item(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    tip: impl Into<SharedString>,
    theme: &Theme,
) -> Stateful<gpui::Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base04.px(theme))
        .tip(tip.into())
        .child(icon(
            glyph,
            ButtonSize::Default.icon_size().px(theme),
            theme.text_muted,
        ))
        .child(label.into())
}

/// A quiet control on the bar (the workspace folder): a Default button frame
/// with no face until the pointer reaches it. The caller attaches the click.
pub(crate) fn status_button(
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> Stateful<gpui::Div> {
    button_frame(div().id(id.into()), theme, ButtonSize::Default)
        .cursor_pointer()
        .hover_quiet(theme)
        .pressed_state(theme, PressedRamp::Wash)
        .child(icon(
            glyph,
            ButtonSize::Default.icon_size().px(theme),
            theme.text_muted,
        ))
        .child(label.into())
}

/// A transient message (a send failure, a branch result): one truncated line
/// that never pushes the meter off the bar.
pub(crate) fn status_message(text: impl IntoElement, theme: &Theme) -> gpui::Div {
    div()
        .max_w(px(320.))
        .truncate()
        .text_color(theme.text_muted)
        .child(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn the_bar_protects_its_ink_only_when_asked() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let mut plain = StatusBar::new().leading(Some("Workspace")).render(&theme);
        let mut protected_leading = StatusBar::new()
            .protect(true)
            .leading(Some("Workspace"))
            .render(&theme);
        let mut protected_trailing = StatusBar::new()
            .protect(true)
            .trailing(Some("Status"))
            .render(&theme);
        assert!(plain.style().background.is_none());
        let fill = Some(theme.background.into());
        assert_eq!(protected_leading.style().background, fill);
        assert_eq!(protected_trailing.style().background, fill);
        assert_eq!(
            plain.style().text.as_ref().and_then(|text| text.color),
            Some(theme.text_muted)
        );
    }

    #[test]
    fn an_empty_protected_bar_does_not_paint_over_the_wallpaper() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let mut row = StatusBar::new().protect(true).render(&theme);
        assert!(row.style().background.is_none());
    }
}
