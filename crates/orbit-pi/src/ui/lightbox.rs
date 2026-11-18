//! The image lightbox: one image at `Contain` scale over the media scrim.
//!
//! The lightbox owns its look — the full-window occluding scrim, the generous
//! inset that keeps the image off the window edge, and the centred,
//! aspect-preserving image. The caller owns which image is open and how it
//! closes: [`lightbox`] takes the close handler, which a click anywhere runs.
//! Escape is the caller's key binding, so the keyboard and the pointer share
//! the same close path.

use std::sync::Arc;

use gpui::{
    div, img, App, Image, ImageSource, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ObjectFit, ParentElement, Stateful, Styled, StyledImage, Window,
};

use crate::theme::{tokens::DynamicSpacing, Theme};

/// The full-window lightbox for `image`. `on_close` runs on any click.
pub(crate) fn lightbox(
    image: Arc<Image>,
    theme: &Theme,
    on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Stateful<gpui::Div> {
    div()
        .id("image-lightbox")
        .debug_selector(|| "image-lightbox".to_string())
        .absolute()
        .inset_0()
        .occlude()
        .bg(theme.scrim_media())
        .p(DynamicSpacing::Base48.px(theme))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, on_close)
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    img(ImageSource::Image(image))
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                ),
        )
}
