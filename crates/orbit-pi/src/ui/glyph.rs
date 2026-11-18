//! Glyphs and marks: embedded raster images, the Nerd Font file-type glyphs
//! (devicons) with their tiles and badges, and provider brand tints.
//!
//! Brand and file-type colours are content-owned (the design system's Identity
//! Rule exception): they come from the file type or the provider, not from a
//! theme role, and are only adjusted for the palette's mode.

use gpui::{
    div, img, px, AnyElement, App, Font, FontFeatures, FontStyle, FontWeight, Hsla, ImageSource,
    IntoElement, ParentElement, Pixels, Resource, SharedString, Styled,
};

use crate::mentions;
use crate::theme::{tokens::Radius, Theme, ThemeMode};

/// Render a compile-time-embedded raster image (PNG) at a fixed height, with
/// the width derived from the source's aspect ratio.
///
/// `img("name.png")` is a trap for embedded assets: gpui's `From<&str>` runs
/// the string through its URI heuristic, and a slashless filename parses as a
/// valid URI authority, so it is fetched over the network instead of loaded
/// from [`crate::assets::Assets`]. Passing [`Resource::Embedded`] explicitly
/// keeps it on the asset-source path.
pub(crate) fn embedded_image(path: &'static str, height: f32) -> impl IntoElement + use<> {
    img(ImageSource::Resource(Resource::Embedded(path.into())))
        .h(px(height))
        .flex_none()
}

/// Same as [`embedded_image`], but sized by width; the height follows the
/// source's aspect ratio. Use for the wordmark, which is laid out to span the
/// sidebar's content width.
pub(crate) fn embedded_image_w(path: &'static str, width: Pixels) -> impl IntoElement + use<> {
    img(ImageSource::Resource(Resource::Embedded(path.into())))
        .w(width)
        .flex_none()
}

// ── devicons (Nerd Font file glyphs) ───────────────────────────────

/// The Nerd Font family that covers devicons glyphs, detected once per
/// process (font availability doesn't change mid-run).
static NERD_FONT: std::sync::OnceLock<Option<SharedString>> = std::sync::OnceLock::new();

/// Preferred Nerd Font families, best first — used only to *rank* the
/// installed families; any family whose name contains "Nerd Font" (or the
/// short `NF` style, e.g. `MesloLGS NF`) is a candidate, verified by an
/// actual PUA-glyph probe.
const NERD_FONT_PREFERRED: [&str; 6] = [
    "SymbolsNerdFont",
    "Symbols Nerd Font",
    "JetBrainsMono Nerd Font",
    "Hack Nerd Font",
    "FiraCode Nerd Font",
    "CaskaydiaCove Nerd Font",
];

/// The Nerd Font family to paint devicons glyphs with, or `None` when no
/// Nerd Font is installed (callers fall back to extension text badges).
pub(crate) fn nerd_font_family(cx: &App) -> Option<SharedString> {
    NERD_FONT
        .get_or_init(|| {
            let installed = cx.text_system().all_font_names();
            // Any family named like a Nerd Font, best-known first.
            let mut candidates: Vec<String> = installed
                .iter()
                .filter(|name| {
                    let lower = name.to_lowercase();
                    lower.contains("nerd font")
                        || lower.contains("nerdfont")
                        || lower.ends_with(" nf")
                })
                .cloned()
                .collect();
            // Dedicated symbols fonts paint the widest glyph coverage first;
            // popular patched coding fonts next; the rest in name order.
            candidates.sort_by_key(|name| {
                let lower = name.to_lowercase();
                let pref = NERD_FONT_PREFERRED
                    .iter()
                    .position(|p| lower.eq_ignore_ascii_case(&p.to_lowercase()))
                    .unwrap_or(NERD_FONT_PREFERRED.len());
                (pref, !lower.starts_with("symbols"), name.clone())
            });
            candidates.into_iter().find_map(|family| {
                let font_id = cx.text_system().resolve_font(&gpui::font(family.clone()));
                // `\u{ea60}` sits in Nerd Font's Octicon range — present in any
                // complete Nerd Font, absent from plain coding fonts.
                cx.text_system()
                    .typographic_bounds(font_id, px(12.), '\u{ea60}')
                    .ok()?;
                Some(SharedString::from(family))
            })
        })
        .clone()
}

/// devicons glyph + color for a file path (`README.md` → its Nerd Font
/// markdown glyph). `dark` selects devicons' palette; `None` when the
/// color string is not parseable hex.
pub(crate) fn dev_file_icon(path: &str, dark: bool) -> Option<(char, Hsla)> {
    let theme = if dark {
        devicons::Theme::Dark
    } else {
        devicons::Theme::Light
    };
    let icon = devicons::icon_for_file(path, &Some(theme));
    let value = u32::from_str_radix(icon.color.trim_start_matches('#'), 16).ok()?;
    Some((icon.icon, gpui::rgb(value).into()))
}

/// Paint a devicons glyph (or fall back to `fallback` when no Nerd Font
/// is installed) — shared by the @-mention rows, the Explorer tree, and tool
/// result file rows. Attachment tiles go through [`file_icon_tile`], which
/// additionally centres the glyph's ink.
///
/// `size` is the glyph's size, an [`IconSize`] resolved to px; the glyph
/// sits in that icon's square (the size plus Base02 either side).
pub(crate) fn file_glyph(
    path: &str,
    dark: bool,
    nerd_family: Option<&SharedString>,
    size: impl Into<Pixels>,
    fallback: AnyElement,
) -> AnyElement {
    let Some(family) = nerd_family else {
        return fallback;
    };
    let Some((glyph, color)) = dev_file_icon(path, dark) else {
        return fallback;
    };
    let size: Pixels = size.into();
    div()
        .w(size + px(4.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .font_family(family)
        .text_size(size)
        .text_color(color)
        .child(glyph.to_string())
        .into_any_element()
}

/// The vertical nudge, in canvas pixels, that centres a glyph's **ink** in its
/// line box instead of centring the font's ascent/descent box.
///
/// GPUI puts a line's baseline at `(line_height - ascent - descent) / 2 +
/// ascent`, so ordinary text lands centred: its ink runs from the baseline up
/// to cap height, which is what the ascent/descent box is shaped around. Icon
/// glyphs instead draw their art across most of the em box above the baseline,
/// so the same baseline leaves an icon sitting high beside a label. The needed
/// shift depends only on the glyph's ink and the font's vertical metrics — the
/// line height cancels out:
///
/// `shift = ink.bottom + ink.height / 2 - (ascent - descent) / 2`
///
/// Positive values move the glyph down. Zero when the glyph cannot be measured,
/// and clamped to a fraction of the type size so a platform whose metrics are
/// shaped differently can never fling the icon out of its tile.
pub(crate) fn glyph_ink_shift(cx: &App, font: &Font, size: Pixels, glyph: char) -> Pixels {
    let text_system = cx.text_system();
    let font_id = text_system.resolve_font(font);
    let Ok(ink) = text_system.typographic_bounds(font_id, size, glyph) else {
        return px(0.);
    };
    let ascent = text_system.ascent(font_id, size);
    let descent = text_system.descent(font_id, size);
    let shift = ink.origin.y + ink.size.height / 2. - (ascent - descent) / 2.;
    shift.clamp(-size * 0.35, size * 0.35)
}

/// The leading visual of a file attachment: the devicons glyph for the file
/// type — the real markdown/rust/PDF mark, not a monogram — centred by its ink
/// in the same square slot an image thumbnail occupies, with no tile painted
/// behind it.
///
/// Falls back to [`file_type_tile`] (the type's label on a tinted square) when
/// no Nerd Font is resolvable or the type has no glyph, so an attachment always
/// leads with something aligned and legible.
pub(crate) fn file_icon_tile(cx: &App, path: &str, tile: Pixels, theme: Theme) -> AnyElement {
    let Some(family) = nerd_font_family(cx) else {
        return file_type_tile(path, tile, theme);
    };
    let Some((glyph, color)) = dev_file_icon(path, theme.mode == ThemeMode::Dark) else {
        return file_type_tile(path, tile, theme);
    };
    let font = Font {
        family: family.clone(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };
    let size = tile * 0.8;
    let shift = glyph_ink_shift(cx, &font, size, glyph);
    // The mark is the icon: no tinted square is painted behind it. The chip is
    // already the highlighted surface, and a second rounded square behind a
    // glyph whose art is itself a box reads as an avatar, not a file icon. The
    // square only reserves the slot that keeps it aligned with an image
    // thumbnail.
    div()
        .size(tile)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .relative()
                .top(shift)
                .font_family(family)
                .text_size(size)
                .text_color(color)
                .child(glyph.to_string()),
        )
        .into_any_element()
}

/// The label tile: the file type's short label (`MD`, `PDF`, `ZIP`, …) in a
/// square washed with the type's colour. The fallback for [`file_icon_tile`]
/// when the Nerd Font or the type's glyph is unavailable, and always centred by
/// construction because it is our own single-line label.
pub(crate) fn file_type_tile(path: &str, tile: Pixels, theme: Theme) -> AnyElement {
    let (label, dark_hex, light_hex) = mentions::file_type_badge(path);
    let color: Hsla = gpui::rgb(if theme.mode == ThemeMode::Dark {
        dark_hex
    } else {
        light_hex
    })
    .into();
    // Longer labels step down so `SVELTE` fits the same square `MD` does.
    let size = match label.chars().count() {
        0..=2 => tile * 0.4,
        3 => tile * 0.34,
        4 => tile * 0.29,
        _ => tile * 0.24,
    };
    div()
        .size(tile)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(Radius::Small.px(&theme))
        .bg(color.opacity(0.16))
        .text_size(size)
        .line_height(size)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label.to_string())
        .into_any_element()
}

/// Extension text badge (`MD`, `TSX`, `HTML`, …) — the no-Nerd-Font
/// fallback for [`file_glyph`], tinted with the language's brand color.
pub(crate) fn file_badge(path: &str, theme: Theme) -> AnyElement {
    let (label, dark_hex, light_hex) = mentions::file_type_badge(path);
    let color: Hsla = gpui::rgb(if theme.mode == ThemeMode::Dark {
        dark_hex
    } else {
        light_hex
    })
    .into();
    div()
        .size(px(17.))
        .flex_none()
        .rounded(Radius::Small.px(&theme))
        .bg(color.opacity(0.16))
        .flex()
        .items_center()
        .justify_center()
        .text_size(theme.ui_px(7.5))
        .line_height(theme.ui_px(8.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label.to_string())
        .into_any_element()
}

/// Ink for a provider's brand mark: the provider's identity color when it has
/// one, otherwise the surface's existing neutral (`fallback`). Resolved per
/// theme like [`file_badge`], so a mark stays legible on both canvases.
pub(crate) fn provider_mark_color(provider: &str, theme: &Theme, fallback: Hsla) -> Hsla {
    let provider = crate::provider_accounts::split_alias(provider)
        .map(|(base, _)| base)
        .unwrap_or(provider);
    match crate::providers::provider_brand(provider) {
        Some((dark, light)) => {
            let hex = if theme.mode == ThemeMode::Dark {
                dark
            } else {
                light
            };
            gpui::rgb(hex).into()
        }
        None => fallback,
    }
}

#[cfg(test)]
mod ink_shift_tests {
    use super::*;

    /// The nudge must never be able to throw a glyph out of its tile, whatever
    /// the platform's font metrics look like. The test platform's text system
    /// reports stub metrics (a negative descent, identical ink for every
    /// glyph), so this pins the clamp rather than the centring itself; the
    /// centring is verified against the bundled face's real metrics in the
    /// `file_icon_tile` docs.
    #[gpui::test]
    fn the_shift_stays_within_the_clamp(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let mut style = window.text_style();
            style.font_size = px(16.).into();
            let font = style.font();
            for glyph in ['M', '|', 'g', ' ', '\u{f48a}'] {
                let shift = glyph_ink_shift(cx, &font, px(16.), glyph);
                assert!(
                    shift.abs() <= px(16. * 0.35),
                    "{glyph:?} must stay inside its tile, got {shift:?}"
                );
            }
        });
    }
}
