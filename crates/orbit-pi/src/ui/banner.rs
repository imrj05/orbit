//! Banners: an inline notice with a glyph, a title, supporting lines and an
//! optional action, in a warning or an error tone.
//!
//! Every in-page notice — a failed operation, an inactive extension, a
//! configuration problem — has the same anatomy (DESIGN.md: what happened →
//! detail → next step → action). The tone picks the glyph and the tint; the
//! caller supplies localized text and the action, and keeps any outer margin.

use gpui::{
    div, AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled,
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
    /// Something is off but the surface still works (an inactive extension).
    Warning,
    /// An operation failed. The tint is the error role at low alpha, so the
    /// title and detail keep their own ink.
    Error,
}

impl BannerTone {
    fn glyph(self) -> &'static str {
        match self {
            Self::Warning => "icons/info.svg",
            Self::Error => "icons/circle-x.svg",
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
    title: Option<SharedString>,
    lines: Vec<Line>,
    action: Option<AnyElement>,
}

impl Banner {
    pub(crate) fn new(tone: BannerTone) -> Self {
        Self {
            tone,
            id: None,
            title: None,
            lines: Vec::new(),
            action: None,
        }
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

    pub(crate) fn render(self, theme: &Theme) -> AnyElement {
        let theme = *theme;
        let (fill, border, glyph_ink) = match self.tone {
            BannerTone::Warning => (theme.surface_secondary, theme.border, theme.warning),
            BannerTone::Error => (
                theme.error.opacity(0.07),
                theme.error.opacity(0.35),
                theme.error,
            ),
        };
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
            .child(icon(
                self.tone.glyph(),
                IconSize::Medium.px(&theme),
                glyph_ink,
            ))
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
