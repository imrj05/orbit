//! The toast card: one notification in the shadcn shape — an inline kind
//! glyph, the title over a muted description, and a quiet × on the right.
//!
//! The card owns its anatomy and how each [`ToastKind`] reads (glyph and
//! tint). Everything about *time* stays with the toast model and its layer
//! (`toast.rs`, `app/toast_ui.rs`): TTL, the deck, hold-on-hover, and the
//! enter/exit fade, which the layer wraps around the card this returns. The
//! caller localizes the text and owns dismissal; a click anywhere on the card
//! (the × included) runs `on_dismiss`.

use gpui::{
    div, App, ElementId, FontWeight, InteractiveElement, MouseButton, MouseUpEvent, ParentElement,
    SharedString, Stateful, Styled, Window,
};

use crate::theme::{tokens::toast as toast_tokens, Theme};
use crate::toast::ToastKind;
use crate::ui::button::icon_button_frame;
use crate::ui::icon::icon;
use crate::ui::style::control::{press, ControlExt};

/// A kind's glyph and tint. Status reinforces the title; it is never the only
/// signal. Info and success stay in secondary ink so the stack reads calm, and
/// only the kinds that need attention take a status hue.
pub(crate) fn kind_glyph(kind: ToastKind, theme: &Theme) -> (&'static str, gpui::Hsla) {
    match kind {
        ToastKind::Info => ("icons/info.svg", theme.text_secondary),
        ToastKind::Success => ("icons/circle-check.svg", theme.success),
        ToastKind::Warning => ("icons/alert-circle.svg", theme.warning),
        ToastKind::Error => ("icons/circle-x.svg", theme.error),
    }
}

/// One toast card.
///
/// ```ignore
/// ToastCard::new(id_key, kind, title)
///     .body(body)
///     .on_dismiss(&theme, close_id, handler)
/// ```
pub(crate) struct ToastCard {
    id: ElementId,
    kind: ToastKind,
    title: SharedString,
    body: Option<SharedString>,
}

impl ToastCard {
    /// `id` keys the card (re-key it when the toast refreshes so its
    /// animation restarts).
    pub(crate) fn new(
        id: impl Into<ElementId>,
        kind: ToastKind,
        title: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            body: None,
        }
    }

    pub(crate) fn body(mut self, body: Option<impl Into<SharedString>>) -> Self {
        self.body = body.map(Into::into);
        self
    }

    /// The finished card. `close_id` keys the × button; `on_dismiss` runs on a
    /// left-click release anywhere on the card.
    pub(crate) fn on_dismiss(
        self,
        theme: &Theme,
        close_id: impl Into<ElementId>,
        on_dismiss: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<gpui::Div> {
        let (glyph, tint) = kind_glyph(self.kind, theme);
        let close_button = press(icon_button_frame(
            div()
                .id(close_id.into())
                .flex_none()
                .cursor_pointer()
                .hover_state(theme),
            theme,
            toast_tokens::CLOSE_BUTTON,
        ))
        .child(icon(
            "icons/x.svg",
            toast_tokens::CLOSE_BUTTON.icon_size().px(theme),
            theme.text_muted,
        ));
        toast_tokens::surface(div().id(self.id), theme)
            .relative()
            .w_full()
            .p(toast_tokens::padding(theme))
            .flex()
            .items_center()
            .gap(toast_tokens::content_gap(theme))
            .cursor_pointer()
            .hover_border(theme)
            .on_mouse_up(MouseButton::Left, on_dismiss)
            .child(
                div()
                    .flex_none()
                    .child(icon(glyph, toast_tokens::ICON.px(theme), tint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(toast_tokens::title_body_gap(theme))
                    .child(
                        div()
                            .line_clamp(2)
                            .text_size(toast_tokens::TITLE.px(theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(self.title),
                    )
                    .children(self.body.map(|body| {
                        div()
                            .whitespace_normal()
                            .line_clamp(3)
                            .text_size(toast_tokens::DESCRIPTION.px(theme))
                            .text_color(theme.text_muted)
                            .child(body)
                    })),
            )
            .child(close_button)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn only_kinds_that_need_attention_take_a_status_hue() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            assert_eq!(kind_glyph(ToastKind::Info, &theme).1, theme.text_secondary);
            assert_eq!(kind_glyph(ToastKind::Success, &theme).1, theme.success);
            assert_eq!(kind_glyph(ToastKind::Warning, &theme).1, theme.warning);
            assert_eq!(kind_glyph(ToastKind::Error, &theme).1, theme.error);
        }
    }

    #[test]
    fn every_kind_has_its_own_glyph() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let kinds = [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Error,
        ];
        let mut glyphs: Vec<_> = kinds.iter().map(|k| kind_glyph(*k, &theme).0).collect();
        glyphs.dedup();
        assert_eq!(glyphs.len(), kinds.len());
    }
}
