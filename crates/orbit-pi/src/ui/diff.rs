//! Diff rows: one line of a diff — an edge in the line's tone, a gutter
//! (a line number or a `+`/`−` marker) and the code body, each washed in the
//! tone.
//!
//! The row owns the tone → colour map (one set of washes for every diff in
//! the app) and the anatomy. It is **paint-only**: the caller builds the code
//! text (syntax runs, selection highlight) and sets the font, size and line
//! height on the returned row, so a streaming diff never reflows because of
//! the row. Parsing, pairing and highlighting stay with `review.rs`,
//! `highlight.rs` and the surfaces.

use gpui::{div, prelude::*, px, AnyElement, Hsla, IntoElement, Pixels};

use crate::theme::{tokens::DynamicSpacing, Theme, ThemeMode};

/// What a diff line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffTone {
    Context,
    Addition,
    Deletion,
}

impl DiffTone {
    fn role(self, theme: &Theme) -> Option<Hsla> {
        match self {
            Self::Context => None,
            Self::Addition => Some(theme.success),
            Self::Deletion => Some(theme.error),
        }
    }

    /// The gutter's ink: the tone, or muted for context.
    pub(crate) fn ink(self, theme: &Theme) -> Hsla {
        self.role(theme).unwrap_or(theme.text_muted)
    }

    /// The code body's wash.
    pub(crate) fn body_fill(self, theme: &Theme) -> Option<Hsla> {
        let alpha = if theme.mode == ThemeMode::Dark {
            0.20
        } else {
            0.12
        };
        self.role(theme).map(|role| role.opacity(alpha))
    }

    /// The gutter's wash: lighter than the body, so the code leads.
    pub(crate) fn gutter_fill(self, theme: &Theme) -> Option<Hsla> {
        let alpha = if theme.mode == ThemeMode::Dark {
            0.15
        } else {
            0.09
        };
        self.role(theme).map(|role| role.opacity(alpha))
    }
}

/// The gutter's shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum DiffGutter {
    /// A right-aligned line number, `width` wide, with a hairline after it
    /// when `divider` (Review's unified rows; a split half has none).
    Number { width: Pixels, divider: bool },
    /// A centred `+` / `−` marker, `width` wide (the transcript's edit
    /// diffs, which have no line numbers).
    Marker { width: Pixels },
}

/// One diff row.
///
/// ```ignore
/// DiffRow::new(DiffTone::Addition, number, code_text(line, theme))
///     .gutter(DiffGutter::Number { width: px(48.), divider: true })
///     .min_h(px(19.))
///     .render(&theme)
///     .font_family(theme::code_font_family())
/// ```
pub(crate) struct DiffRow {
    tone: DiffTone,
    gutter_content: AnyElement,
    body: AnyElement,
    gutter: DiffGutter,
    min_h: Pixels,
    wrap: bool,
    edge: bool,
}

impl DiffRow {
    pub(crate) fn new(tone: DiffTone, gutter: impl IntoElement, body: impl IntoElement) -> Self {
        Self {
            tone,
            gutter_content: gutter.into_any_element(),
            body: body.into_any_element(),
            gutter: DiffGutter::Marker { width: px(24.) },
            min_h: px(18.),
            wrap: true,
            edge: true,
        }
    }

    pub(crate) fn gutter(mut self, gutter: DiffGutter) -> Self {
        self.gutter = gutter;
        self
    }

    pub(crate) fn min_h(mut self, min_h: Pixels) -> Self {
        self.min_h = min_h;
        self
    }

    /// Wrap long lines (the default) or let the row run its natural width
    /// for a list that pans sideways.
    pub(crate) fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }

    /// The 2px edge in the line's tone. A split half leaves it to the row.
    pub(crate) fn edge(mut self, edge: bool) -> Self {
        self.edge = edge;
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> gpui::Div {
        let ink = self.tone.ink(theme);
        let gutter_fill = self.tone.gutter_fill(theme);
        let body_fill = self.tone.body_fill(theme);
        let edge = self.edge.then(|| self.tone.role(theme)).flatten();
        let gutter = match self.gutter {
            DiffGutter::Number { width, divider } => div()
                .w(width)
                .min_h(self.min_h)
                .flex_none()
                .pr(px(9.))
                .flex()
                .justify_end()
                .when(divider, |gutter| {
                    gutter.border_r_1().border_color(theme.hairline())
                }),
            DiffGutter::Marker { width } => div()
                .w(width)
                .flex_none()
                .flex()
                .items_start()
                .justify_center(),
        };
        let numbered = matches!(self.gutter, DiffGutter::Number { .. });
        let body = div()
            .min_h(self.min_h)
            .min_w_0()
            .flex_1()
            .when(numbered, |body| body.pl(DynamicSpacing::Base12.px(theme)))
            .pr(DynamicSpacing::Base12.px(theme))
            .overflow_hidden()
            .when(self.wrap, |body| body.whitespace_normal())
            .when(!self.wrap, |body| body.whitespace_nowrap())
            .when_some(body_fill, |body, fill| body.bg(fill))
            .child(self.body);
        div()
            .min_w_0()
            .min_h(self.min_h)
            .flex()
            .when_some(edge, |row, edge| row.border_l_2().border_color(edge))
            .child(
                gutter
                    .text_color(ink)
                    .when_some(gutter_fill, |gutter, fill| gutter.bg(fill))
                    .child(self.gutter_content),
            )
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn context_is_unwashed_and_changes_wash_their_body_more_than_their_gutter() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            assert_eq!(DiffTone::Context.body_fill(&theme), None);
            assert_eq!(DiffTone::Context.ink(&theme), theme.text_muted);
            for tone in [DiffTone::Addition, DiffTone::Deletion] {
                let body = tone.body_fill(&theme).unwrap();
                let gutter = tone.gutter_fill(&theme).unwrap();
                assert!(body.a > gutter.a, "{id:?} {tone:?}");
            }
            assert_eq!(DiffTone::Addition.ink(&theme), theme.success);
            assert_eq!(DiffTone::Deletion.ink(&theme), theme.error);
        }
    }

    #[test]
    fn only_a_changed_line_draws_its_edge() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let mut added = DiffRow::new(DiffTone::Addition, "+", "x").render(&theme);
        let mut context = DiffRow::new(DiffTone::Context, "", "x").render(&theme);
        assert_eq!(added.style().border_color, Some(theme.success));
        assert!(context.style().border_color.is_none());
    }
}
