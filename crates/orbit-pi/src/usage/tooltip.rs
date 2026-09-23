//! A small native tooltip.
//!
//! GPUI owns the hover plumbing (`InteractiveElement::tooltip`); this is just
//! the view it builds — one line of text in the app's own popover register, so
//! a tooltip never arrives wearing another product's chrome.

use gpui::{div, prelude::*, px, Context, IntoElement, Render, SharedString, Window};

use crate::app::PopoverSurface;
use crate::theme;

/// A single-line tooltip.
pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *theme::get(cx);
        div()
            .max_w(px(320.))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .popover_surface(theme)
            .text_size(theme.ui_px(11.5))
            .text_color(theme.text)
            .whitespace_nowrap()
            .child(self.text.clone())
    }
}
