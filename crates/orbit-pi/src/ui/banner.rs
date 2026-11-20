//! Banners: an inline notice with a glyph, a title, supporting lines and an
//! optional action, in a warning or an error tone.
//!
//! Every in-page notice — a failed operation, an inactive extension, a
//! configuration problem — has the same anatomy (DESIGN.md: what happened →
//! detail → next step → action). The tone picks the glyph and the tint; the
//! caller supplies localized text and the action, and keeps any outer margin.

use gpui::{
    div, AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    SharedString, Styled,
};

use crate::theme::{
    tokens::{panel, DynamicSpacing, IconSize, TextSize},
    Theme,
};
use crate::ui::icon::icon;
use crate::ui::style::control::TipExt;

/// What a banner reports. The tone decides the glyph and the tint, never the
/// geometry: every tone has the same padding, radius and type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BannerTone {
    /// Neutral progress or a fact (a setup script running).
    Info,
    /// Work finished as asked (a setup script that succeeded).
    Success,
    /// Something is off but the surface still works (an inactive extension).
    Warning,
    /// An operation failed. The tint is the error role at low alpha, so the
    /// title and detail keep their own ink.
    Error,
}

impl BannerTone {
    fn glyph(self) -> &'static str {
        match self {
            Self::Info | Self::Warning => "icons/info.svg",
            Self::Success => "icons/check.svg",
            Self::Error => "icons/circle-x.svg",
        }
    }

    /// The card's fill, hairline and glyph ink. Only an error tints the card;
    /// every other tone sits on the trough and speaks through its glyph.
    ///
    /// The trough is `surface_secondary` through [`Theme::page_fill`], so on a
    /// glass page it is translucent like every raised surface there.
    fn colors(self, theme: &Theme) -> (Hsla, Hsla, Hsla) {
        let trough = theme.page_fill(theme.surface_secondary);
        match self {
            Self::Info => (trough, theme.hairline(), theme.text_muted),
            Self::Success => (trough, theme.hairline(), theme.success),
            Self::Warning => (trough, theme.hairline(), theme.warning),
            Self::Error => (
                theme.error.opacity(0.07),
                theme.error.opacity(0.35),
                theme.error,
            ),
        }
    }

    /// A strip's fill and ink: the tone itself carries the message.
    fn strip_colors(self, theme: &Theme) -> (Hsla, Hsla) {
        match self {
            Self::Info => (
                theme.page_fill(theme.surface_secondary),
                theme.text_secondary,
            ),
            Self::Success => (theme.success.opacity(0.12), theme.success),
            Self::Warning => (theme.warning.opacity(0.12), theme.warning),
            Self::Error => (theme.error.opacity(0.12), theme.error),
        }
    }
}

/// An inline notice. Build it, then [`Banner::render`] it with the theme.
///
/// ```ignore
/// Banner::new(BannerTone::Error)
///     .title(title)               // localized by the caller
///     .detail(stderr)
///     .hint(next_step)
///     .action(retry_button)
///     .render(&theme)
/// ```
pub(crate) struct Banner {
    tone: BannerTone,
    id: Option<SharedString>,
    glyph: Option<AnyElement>,
    title: Option<SharedString>,
    lines: Vec<Line>,
    action: Option<AnyElement>,
}

impl Banner {
    pub(crate) fn new(tone: BannerTone) -> Self {
        Self {
            tone,
            id: None,
            glyph: None,
            title: None,
            lines: Vec::new(),
            action: None,
        }
    }

    /// A leading glyph in place of the tone's (a live spinner while work
    /// runs), sized `IconSize::Medium` by the caller.
    pub(crate) fn glyph(mut self, glyph: impl IntoElement) -> Self {
        self.glyph = Some(glyph.into_any_element());
        self
    }

    /// An element id, also exposed as the banner's debug selector.
    pub(crate) fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// What happened, in one line of medium primary ink.
    pub(crate) fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The underlying message: wrapped, never truncated. Blank text is skipped
    /// so a caller can pass an optional detail straight through.
    pub(crate) fn detail(self, detail: impl Into<SharedString>) -> Self {
        let detail = detail.into();
        self.text_line(detail, Line::Detail)
    }

    /// The next step, in words, when there is no button to press.
    pub(crate) fn hint(self, hint: impl Into<SharedString>) -> Self {
        let hint = hint.into();
        self.text_line(hint, Line::Hint)
    }

    /// A caller-built line for content the text lines cannot express (a
    /// monospace error, a truncated path with its tooltip).
    pub(crate) fn line(mut self, line: impl IntoElement) -> Self {
        self.lines.push(Line::Custom(line.into_any_element()));
        self
    }

    /// The trailing action: a [`crate::ui::button::Button`] or a dismiss.
    pub(crate) fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    fn text_line(mut self, text: SharedString, line: fn(SharedString) -> Line) -> Self {
        if !text.trim().is_empty() {
            self.lines.push(line(text));
        }
        self
    }

    /// A compact notice inside a form, dialog or menu (a failed save, a
    /// rejected password, a dirty worktree before a branch switch): the
    /// tone's own card tint, its glyph at `IconSize::Small`, and one wrapped
    /// line of Small text. An error's text takes the error ink, as the
    /// dialogs always showed it; every other tone keeps secondary ink and
    /// lets the glyph carry the tone, since a status colour only reinforces.
    /// `message` is localized by the caller; outer margins stay with it.
    pub(crate) fn note(
        tone: BannerTone,
        message: impl Into<SharedString>,
        theme: &Theme,
    ) -> gpui::Div {
        let (fill, _, glyph_ink) = tone.colors(theme);
        let ink = match tone {
            BannerTone::Error => theme.error,
            BannerTone::Info | BannerTone::Success | BannerTone::Warning => theme.text_secondary,
        };
        div()
            .px(DynamicSpacing::Base08.px(theme))
            .py(DynamicSpacing::Base08.px(theme))
            .rounded(panel::RADIUS.px(theme))
            .bg(fill)
            .flex()
            .items_start()
            .gap(DynamicSpacing::Base08.px(theme))
            .text_size(TextSize::Small.px(theme))
            .text_color(ink)
            .child(icon(tone.glyph(), IconSize::Small.px(theme), glyph_ink))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_normal()
                    .child(message.into()),
            )
    }

    /// A one-line notice across the top of a pane (Explorer's operation
    /// errors): full-bleed, a hairline below, Small text in the tone's ink, no
    /// glyph or card. `message` is localized by the caller.
    pub(crate) fn strip(
        tone: BannerTone,
        message: impl Into<SharedString>,
        theme: &Theme,
    ) -> gpui::Div {
        let (fill, ink) = tone.strip_colors(theme);
        div()
            .flex_none()
            .px(DynamicSpacing::Base08.px(theme))
            .py(DynamicSpacing::Base04.px(theme))
            .border_b_1()
            .border_color(theme.hairline())
            .bg(fill)
            .text_size(TextSize::Small.px(theme))
            .text_color(ink)
            .child(message.into())
    }

    pub(crate) fn render(self, theme: &Theme) -> AnyElement {
        let theme = *theme;
        let (fill, border, glyph_ink) = self.tone.colors(&theme);
        let column = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base04.px(&theme))
            .children(self.title.map(|title| {
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(title)
            }))
            .children(self.lines.into_iter().map(|line| line.render(&theme)));
        let card = div()
            .w_full()
            .min_w_0()
            .bg(fill)
            .border_1()
            .border_color(border)
            .rounded(panel::RADIUS.px(&theme))
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_start()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(self.glyph.unwrap_or_else(|| {
                icon(self.tone.glyph(), IconSize::Medium.px(&theme), glyph_ink).into_any_element()
            }))
            .child(column)
            .children(self.action);
        match self.id {
            Some(id) => {
                let selector = id.to_string();
                card.id(id)
                    .debug_selector(move || selector.clone())
                    .into_any_element()
            }
            None => card.into_any_element(),
        }
    }
}

enum Line {
    /// Secondary ink: the underlying message.
    Detail(SharedString),
    /// Muted ink: the next step in words.
    Hint(SharedString),
    /// Caller-styled.
    Custom(AnyElement),
}

impl Line {
    fn render(self, theme: &Theme) -> AnyElement {
        let (text, ink) = match self {
            Line::Detail(text) => (text, theme.text_secondary),
            Line::Hint(text) => (text, theme.text_muted),
            Line::Custom(element) => return element,
        };
        div()
            .text_size(TextSize::Small.px(theme))
            .text_color(ink)
            .child(text)
            .into_any_element()
    }
}

/// A one-line, truncated banner line whose full text is its tooltip — for a
/// path or an extension's own sentence that may be long.
pub(crate) fn truncated_line(
    id: impl Into<SharedString>,
    text: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    let text = text.into();
    div()
        .id(id.into())
        .text_size(TextSize::Small.px(theme))
        .text_color(theme.text_muted)
        .truncate()
        .tip(text.clone())
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn only_an_error_tints_the_card_and_every_tone_has_its_glyph_ink() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            for tone in [BannerTone::Info, BannerTone::Success, BannerTone::Warning] {
                assert_eq!(
                    tone.colors(&theme).0,
                    theme.surface_secondary,
                    "{id:?} {tone:?}"
                );
            }
            assert_eq!(BannerTone::Error.colors(&theme).2, theme.error);
            assert_eq!(BannerTone::Success.colors(&theme).2, theme.success);
            assert_eq!(BannerTone::Warning.colors(&theme).2, theme.warning);
        }
    }

    #[test]
    fn a_strip_carries_its_tone_in_the_ink() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let mut strip = Banner::strip(BannerTone::Error, "Could not rename", &theme);
        let style = strip.style();
        assert_eq!(
            style.text.as_ref().and_then(|text| text.color),
            Some(theme.error)
        );
        assert_eq!(style.background, Some(theme.error.opacity(0.12).into()));
    }

    /// A note takes the tone's own card tint, so a dialog's inline error and
    /// a page's error banner can never drift to different strengths.
    #[test]
    fn a_note_shares_the_banner_tint_and_takes_the_tone_ink() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            for tone in [BannerTone::Info, BannerTone::Success, BannerTone::Warning, BannerTone::Error] {
                let mut note = Banner::note(tone, "x", &theme);
                assert_eq!(note.style().background, Some(tone.colors(&theme).0.into()));
            }
            let mut error = Banner::note(BannerTone::Error, "x", &theme);
            assert_eq!(error.style().text.as_ref().and_then(|t| t.color), Some(theme.error));
        }
    }

    /// Inline errors and the other notices are built from this module, never
    /// from a hand-mixed error tint at the call site.
    #[test]
    fn notices_are_not_hand_mixed_at_call_sites() {
        for (name, source) in [
            ("settings", include_str!("../app/settings.rs")),
            ("accounts", include_str!("../app/provider_accounts.rs")),
            ("ssh", include_str!("../app/ssh_auth_ui.rs")),
            ("hosts", include_str!("../app/remote_hosts_ui.rs")),
            ("mcp", include_str!("../app/mcp_ui.rs")),
            ("remote providers", include_str!("../app/remote_providers_ui.rs")),
            ("server clone", include_str!("../app/server_clone_ui.rs")),
            ("bug report", include_str!("../app/bug_report_ui.rs")),
            ("transcript", include_str!("../transcript_view.rs")),
        ] {
            assert!(!source.contains(".bg(theme.error.opacity(0.1"), "{name} mixes an error tint");
        }
        let git = include_str!("../git_panel/mod.rs");
        assert!(!git.contains(".bg(theme.warning.opacity(0.1))"));
    }
}
