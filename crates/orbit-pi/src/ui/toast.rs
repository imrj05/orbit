//! The toast card: one notification in the shadcn shape — an inline kind
//! glyph, the title over a muted description, and a quiet × on the right.
//!
//! The card owns its anatomy and how each [`ToastKind`] reads (glyph and
//! tint). Everything about *time* stays with the toast model and its layer
//! (`toast.rs`, `app/toast_ui.rs`): TTL, the deck, hold-on-hover, and the
//! enter/exit fade, which the layer wraps around the card this returns. The
//! caller localizes the text and owns dismissal; a click anywhere on the card
//! (the × included) runs `on_dismiss`.
//!
//! A *prompt* card ([`ToastCard::on_close`]) is the persistent shape: it waits
//! for a choice, so only its × closes it. It adds a count beside the title,
//! caller content (built from [`update_row`] / [`separator`]) and an action
//! row under a hairline.

use gpui::{
    div, prelude::FluentBuilder, AnyElement, App, Div, ElementId, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseUpEvent, ParentElement, SharedString,
    Stateful, Styled, Window,
};

use crate::theme::{self as theme_mod, tokens::toast as toast_tokens, tokens::IconSize, Theme};
use crate::toast::ToastKind;
use crate::ui::button::{icon_button_frame, IconButton};
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
    glyph: Option<(&'static str, Hsla)>,
    count: Option<usize>,
    content: Vec<AnyElement>,
    actions: Vec<AnyElement>,
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
            glyph: None,
            count: None,
            content: Vec::new(),
            actions: Vec::new(),
        }
    }

    pub(crate) fn body(mut self, body: Option<impl Into<SharedString>>) -> Self {
        self.body = body.map(Into::into);
        self
    }

    /// A glyph and tint of the caller's choosing in place of the kind's.
    pub(crate) fn glyph(mut self, path: &'static str, tint: Hsla) -> Self {
        self.glyph = Some((path, tint));
        self
    }

    /// Prompt cards: a count badge at the end of the title row.
    pub(crate) fn count(mut self, count: usize) -> Self {
        self.count = Some(count);
        self
    }

    /// Prompt cards: rows under the title (see [`update_row`], [`separator`]).
    pub(crate) fn content(mut self, rows: impl IntoIterator<Item = AnyElement>) -> Self {
        self.content.extend(rows);
        self
    }

    /// Prompt cards: buttons in the footer, right-aligned under a hairline.
    pub(crate) fn actions(mut self, actions: impl IntoIterator<Item = AnyElement>) -> Self {
        self.actions.extend(actions);
        self
    }

    /// The finished prompt card. It stays until a choice is made: clicking the
    /// card does nothing, and only the × (keyed `close_id`) runs `on_close`.
    /// It stands on its own outside the deck, so it carries the toast width.
    pub(crate) fn on_close(
        self,
        theme: &Theme,
        close_id: impl Into<ElementId>,
        on_close: impl Fn(&MouseUpEvent, &mut Window, &mut App) + 'static,
    ) -> Stateful<Div> {
        let (glyph, tint) = self.glyph.unwrap_or_else(|| kind_glyph(self.kind, theme));
        let close_button = IconButton::new(close_id, "icons/x.svg")
            .size(toast_tokens::CLOSE_BUTTON)
            .on_mouse_up(theme, on_close);
        let count = self.count.map(|count| {
            div()
                .min_w(toast_tokens::badge_min_width(theme))
                .h(toast_tokens::badge_height(theme))
                .px(toast_tokens::badge_padding_x(theme))
                .flex()
                .items_center()
                .justify_center()
                .rounded(toast_tokens::BADGE_RADIUS.px(theme))
                .bg(theme.overlay)
                .text_size(toast_tokens::BODY.px(theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(count.to_string())
        });
        let footer = (!self.actions.is_empty()).then(|| {
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_end()
                .gap(toast_tokens::action_gap(theme))
                .children(self.actions)
        });
        let has_footer = footer.is_some();
        toast_tokens::surface(div().id(self.id), theme)
            .w(toast_tokens::WIDTH)
            .p(toast_tokens::padding(theme))
            .flex()
            .items_start()
            .gap(toast_tokens::content_gap(theme))
            .child(
                // Inline with the title row, as on every toast card.
                div()
                    .flex_none()
                    .h(toast_tokens::CLOSE_BUTTON.height(theme))
                    .flex()
                    .items_center()
                    .child(icon(glyph, toast_tokens::ICON.px(theme), tint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(toast_tokens::content_gap(theme))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(toast_tokens::header_gap(theme))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(toast_tokens::TITLE.px(theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(self.title),
                            )
                            .children(count)
                            .child(close_button),
                    )
                    .when(!self.content.is_empty(), |el| {
                        el.child(div().w_full().flex().flex_col().children(self.content))
                    })
                    .when(has_footer, |el| el.child(separator(theme)))
                    .children(footer),
            )
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

/// A full-width hairline between a prompt card's rows.
pub(crate) fn separator(theme: &Theme) -> Div {
    div()
        .w_full()
        .h(toast_tokens::separator_size())
        .bg(theme.hairline())
}

/// One "name  current → latest" row in a prompt card: the name truncates, the
/// two versions keep their width in the code face. The caller shortens the
/// versions.
pub(crate) fn update_row(
    name: impl Into<SharedString>,
    current: impl Into<SharedString>,
    latest: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    let version = |text: SharedString, ink: Hsla| {
        div()
            .font_family(theme_mod::code_font_family())
            .text_size(toast_tokens::version_text_size(theme))
            .text_color(ink)
            .child(text)
    };
    div()
        .w_full()
        .min_w_0()
        .py(toast_tokens::package_row_padding_y(theme))
        .flex()
        .items_center()
        .justify_between()
        .gap(toast_tokens::package_row_gap(theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(toast_tokens::BODY.px(theme))
                .text_color(theme.text_secondary)
                .child(name.into()),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(toast_tokens::version_gap(theme))
                .child(version(current.into(), theme.text_muted))
                .child(icon(
                    "icons/arrow-right.svg",
                    IconSize::XSmall.px(theme),
                    theme.text_muted,
                ))
                .child(version(latest.into(), theme.text_primary)),
        )
        .into_any_element()
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

    struct PromptView(Theme);

    impl gpui::Render for PromptView {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            use gpui::{px, Styled as _};
            let theme = self.0;
            ToastCard::new("prompt", ToastKind::Info, "Updates available")
                .glyph("icons/extensions.svg", theme.accent_text)
                .count(2)
                .content([
                    update_row("one", "abc1234", "def5678", &theme),
                    separator(&theme).into_any_element(),
                    update_row("two", "1.0.0", "1.1.0", &theme),
                ])
                .actions([div()
                    .debug_selector(|| "prompt-action".to_string())
                    .w(px(40.))
                    .h(px(20.))
                    .into_any_element()])
                .on_close(&theme, "prompt-close", |_, _, _| {})
                .debug_selector(|| "prompt-card".to_string())
        }
    }

    /// The prompt card stands alone at the toast width with its rows and
    /// actions laid out, even in an unconstrained column.
    #[gpui::test]
    fn a_prompt_card_keeps_the_toast_width(cx: &mut gpui::TestAppContext) {
        use gpui::{point, px, AppContext as _, AvailableSpace};

        let theme = Theme::for_id(ThemeId::Orbit);
        cx.update(|cx| cx.set_global(theme));
        let cx = cx.add_empty_window();
        let _ = cx.draw(
            point(px(0.), px(0.)),
            AvailableSpace::min_size(),
            move |_, cx| cx.new(|_| PromptView(theme)),
        );
        let card = cx
            .debug_bounds("prompt-card")
            .expect("prompt card laid out");
        let action = cx
            .debug_bounds("prompt-action")
            .expect("action row laid out");
        assert_eq!(card.size.width, toast_tokens::WIDTH, "{card:?}");
        assert!(
            card.contains(&action.origin),
            "action outside card: {action:?}"
        );
        assert!(
            action.origin.y > card.origin.y + px(40.),
            "actions not under the rows: {action:?}"
        );
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
