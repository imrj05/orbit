//! Text roles that are not a size: the numeric face.

use gpui::{Font, FontFeatures};
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
