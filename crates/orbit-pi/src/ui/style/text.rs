//! Text roles that are not a size: the numeric face.

use gpui::{px, Font, FontFeatures, Hsla, Pixels, SharedString, Window};
use std::sync::Arc;

use crate::theme;

/// The UI face with tabular, lining figures, so numbers in a column (a table
/// cell, a meter, a token count) line up digit for digit.
pub(crate) fn num_font() -> Font {
    let mut font = gpui::font(theme::ui_font_family());
    font.features = FontFeatures(Arc::new(vec![
        ("tnum".to_string(), 1),
        ("lnum".to_string(), 1),
    ]));
    font
}

/// The advance width of one line of UI text at `size`, shaped with the app's
/// UI face.
///
/// A control that reserves room for a label it is about to reveal needs the
/// label's real width: a fixed reserve stops fitting as soon as the UI font
/// size, the spacing density or the locale changes. Shaping goes through the
/// window's text system, so callers pass the window they are rendering.
pub(crate) fn text_width(window: &Window, text: &str, size: Pixels) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let run = gpui::TextRun {
        len: text.len(),
        font: gpui::font(theme::ui_font_family()),
        color: Hsla::transparent_black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(SharedString::from(text.to_string()), size, &[run], None)
        .width
}
