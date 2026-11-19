//! Framed sections with a title, optional supporting text and caller-owned content.
//!
//! The usage board uses the unpadded card for its full-bleed metric cells and
//! the section wrapper for padded content. Neither owns interaction or focus.

use gpui::{div, prelude::*, AnyElement, FontWeight, IntoElement, SharedString};

use crate::theme::tokens::{DynamicSpacing, TextSize};
use crate::theme::Theme;
use crate::ui::style::surface::SurfaceExt;

/// A card with a padded content area, for a section inside a page.
pub(crate) fn section(
    id: &'static str,
    title: &str,
    description: Option<&str>,
    meta: Option<String>,
    right: Option<AnyElement>,
    content: AnyElement,
    theme: Theme,
) -> AnyElement {
    card(
        id,
        title,
        description,
        meta,
        right,
        div()
            .w_full()
            .px(DynamicSpacing::Base12.px(&theme))
            .pt(DynamicSpacing::Base12.px(&theme))
            .pb(DynamicSpacing::Base12.px(&theme))
            .child(content)
            .into_any_element(),
        theme,
        false,
    )
}

/// A framed header and full-bleed body; callers own the content and clipping choice.
#[allow(clippy::too_many_arguments)]
pub(crate) fn card(
    id: &'static str,
    title: &str,
    description: Option<&str>,
    meta: Option<String>,
    right: Option<AnyElement>,
    content: AnyElement,
    theme: Theme,
    clip: bool,
) -> AnyElement {
    let mut root = div()
        .id(SharedString::from(id))
        .flex_none()
        .w_full()
        // The fill stays translucent behind wallpaper even without backdrop blur.
        .card_surface(
            &theme,
            theme.hairline(),
            theme.card_fill(theme.surface_elevated),
        )
        .flex()
        .flex_col()
        .child(
            div()
                .flex_none()
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base12.px(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base04.px(&theme))
                .border_b_1()
                .border_color(theme.hairline())
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(
                            div()
                                .text_size(TextSize::Large.px(&theme))
                                .line_height(theme.ui_px(20.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(title.to_string()),
                        )
                        .children(meta.map(|meta| {
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(meta)
                        }))
                        .child(div().flex_1())
                        .children(right),
                )
                .children(description.map(|description| {
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .line_height(theme.ui_px(16.))
                        .text_color(theme.text_muted)
                        .child(description.to_string())
                })),
        )
        .child(content);
    if clip {
        root = root.overflow_hidden();
    }
    root.into_any_element()
}
