//! The shared diff-row painters: file headers, hunk headers, meta lines, gap
//! rows, and syntax-coloured code rows.
//!
//! Extracted from the Review pane so the pane's diff and the Review page's
//! per-file preview render identically. The pane still owns its virtualized
//! list and gap expansion; this module owns the row vocabulary, which is why
//! it has no state and takes only a [`Theme`].

use gpui::{
    div, prelude::*, px, AnyElement, Font, FontFeatures, FontStyle, FontWeight, Hsla,
    SharedString, StyledText, TextRun,
};

use crate::app::{file_glyph, icon};
use crate::review::{self, LineKind};
use crate::theme::tokens::{IconSize, TextSize};
use crate::theme::{self, Theme, ThemeMode};

/// Review diff row metrics, shared by every surface that paints rows.
pub const DIFF_TEXT_SIZE: f32 = 12.5;
/// File headers are one fixed row.
pub const REVIEW_FILE_HEADER_HEIGHT: f32 = 36.;
/// Hunk and meta rows are one fixed row.
pub const REVIEW_HUNK_HEIGHT: f32 = 24.;

pub fn render_file_header(
    file: &review::File,
    theme: Theme,
    nerd: Option<&SharedString>,
    dark: bool,
) -> AnyElement {
    let fallback =
        icon("icons/file.svg", IconSize::Small.px(&theme), theme.text_3).into_any_element();
    let glyph = file_glyph(&file.path, dark, nerd, IconSize::Small.px(&theme), fallback);
    div()
        .w_full()
        .min_w_0()
        .h(px(REVIEW_FILE_HEADER_HEIGHT))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.bg_raised)
        .child(glyph)
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .font_family(theme::code_font_family())
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_2)
                .child(file.path.clone()),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.add_green)
                .child(format!("+{}", file.additions)),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.del_red)
                .child(format!("-{}", file.deletions)),
        )
        .into_any_element()
}

pub fn render_hunk_header(content: &str, theme: Theme) -> AnyElement {
    let gutter_w = diff_gutter_width();
    div()
        .min_h(px(REVIEW_HUNK_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .text_color(theme.text_3)
        .child(
            div()
                .w(px(gutter_w))
                .min_h(px(REVIEW_HUNK_HEIGHT))
                .flex_none()
                .border_r_1()
                .border_color(theme.border)
                .bg(theme.overlay),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .px(px(12.))
                .py(px(4.))
                .overflow_hidden()
                .whitespace_normal()
                .bg(theme.overlay)
                .child(content.to_string()),
        )
        .into_any_element()
}

pub fn render_meta(content: &str, theme: Theme) -> AnyElement {
    let gutter_w = diff_gutter_width();
    div()
        .min_h(px(REVIEW_HUNK_HEIGHT))
        .w_full()
        .min_w_0()
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .text_color(theme.text_3)
        .child(
            div()
                .w(px(gutter_w))
                .min_h(px(REVIEW_HUNK_HEIGHT))
                .flex_none(),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .py(px(4.))
                .pr(px(10.))
                .overflow_hidden()
                .whitespace_normal()
                .child(content.to_string()),
        )
        .into_any_element()
}

fn diff_gutter_width() -> f32 {
    (DIFF_TEXT_SIZE * 3. + 14.).round()
}

fn diff_row_height() -> f32 {
    (DIFF_TEXT_SIZE * 1.5).round()
}

/// One context/addition/deletion row: a single line-number gutter (the new
/// number, falling back to the old one) and syntax-coloured code.
pub fn render_code_row(line: &review::Line, theme: Theme) -> AnyElement {
    let row_height = diff_row_height();
    let (body_bg, gutter_bg, edge, number_color) = match line.kind {
        LineKind::Addition => (
            Some(theme.add_green.opacity(body_wash(theme))),
            Some(theme.add_green.opacity(gutter_wash(theme))),
            Some(theme.add_green),
            theme.add_green,
        ),
        LineKind::Deletion => (
            Some(theme.del_red.opacity(body_wash(theme))),
            Some(theme.del_red.opacity(gutter_wash(theme))),
            Some(theme.del_red),
            theme.del_red,
        ),
        _ => (None, None, None, theme.text_3),
    };
    let shown_line = line.new_line.or(line.old_line);
    let number = shown_line.map(|n| n.to_string()).unwrap_or_default();
    let content = code_text(line, theme);
    div()
        .w_full()
        .min_w_0()
        .min_h(px(row_height))
        .flex()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(DIFF_TEXT_SIZE))
        .line_height(theme.code_px(16.))
        .when_some(edge, |row, edge| row.border_l_2().border_color(edge))
        .child(
            div()
                .w(px(diff_gutter_width()))
                .min_h(px(row_height))
                .flex_none()
                .pr(px(9.))
                .flex()
                .justify_end()
                .border_r_1()
                .border_color(theme.border)
                .text_color(number_color)
                .when_some(gutter_bg, |gutter, bg| gutter.bg(bg))
                .child(number),
        )
        .child(
            div()
                .min_h(px(row_height))
                .min_w_0()
                .flex_1()
                .pl(px(12.))
                .pr(px(10.))
                .overflow_hidden()
                .whitespace_normal()
                .when_some(body_bg, |body, bg| body.bg(bg))
                .child(content),
        )
        .into_any_element()
}

fn body_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.20
    } else {
        0.12
    }
}

fn gutter_wash(theme: Theme) -> f32 {
    if theme.mode == ThemeMode::Dark {
        0.15
    } else {
        0.09
    }
}

/// Build syntax-colored text for one diff line.
fn code_text(line: &review::Line, theme: Theme) -> StyledText {
    let base = theme.text_2;
    let font = mono_font();
    let mut runs: Vec<TextRun> = Vec::new();
    let mut offset = 0usize;
    for token in &line.tokens {
        let start = token.range.start.min(line.content.len());
        let end = token.range.end.min(line.content.len());
        if start > offset {
            runs.push(run(start - offset, base, &font));
        }
        if end > start {
            runs.push(run(end - start, theme.token_color(token.class), &font));
        }
        offset = offset.max(end);
    }
    if offset < line.content.len() {
        runs.push(run(line.content.len() - offset, base, &font));
    }
    if runs.is_empty() {
        runs.push(run(line.content.len(), base, &font));
    }
    StyledText::new(line.content.clone()).with_runs(runs)
}

fn run(len: usize, color: Hsla, font: &Font) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn mono_font() -> Font {
    Font {
        family: theme::code_font_family(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    }
}
