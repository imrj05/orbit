//! Wallpaper artwork effects.
//!
//! Adapted from Tern's `crates/tern/src/wallpaper_fx.rs` (MIT), itself adapted
//! from zeron's `crates/mobile/src/wallpaper.rs` artwork effects (MIT).
//!
//! An effect runs once per `(image, effect, appearance)` on the background
//! executor, not per frame. The pipeline applies it before brightness/contrast
//! sampling, so translucent surfaces are sized against the picture that is
//! actually painted.

use image::RgbaImage;

/// Artwork treatment for the wallpaper. `None` keeps the photograph; the
/// others print it in a stylised screen.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    #[default]
    None,
    Dither,
    Ascii,
    Halftone,
    Scanlines,
}

impl Effect {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Dither,
        Self::Ascii,
        Self::Halftone,
        Self::Scanlines,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Dither => "dither",
            Self::Ascii => "ascii",
            Self::Halftone => "halftone",
            Self::Scanlines => "scanlines",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "none" | "off" | "photo" => Some(Self::None),
            "dither" | "dithered" => Some(Self::Dither),
            "ascii" | "text" => Some(Self::Ascii),
            "halftone" | "dots" => Some(Self::Halftone),
            "scanlines" | "scan-lines" => Some(Self::Scanlines),
            _ => None,
        }
    }
}

struct Source<'a> {
    w: u32,
    h: u32,
    rgba: &'a [u8],
}

impl Source<'_> {
    fn color(&self, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * self.w + x) * 4) as usize;
        [
            self.rgba[i],
            self.rgba[i + 1],
            self.rgba[i + 2],
            self.rgba[i + 3],
        ]
    }

    fn luma(&self, x: u32, y: u32) -> u8 {
        let [r, g, b, _] = self.color(x, y);
        luma(r, g, b)
    }
}

fn luma(r: u8, g: u8, b: u8) -> u8 {
    // Rec. 709 weights, matching image-rs `to_luma8`.
    ((2126 * r as u32 + 7152 * g as u32 + 722 * b as u32) / 10000) as u8
}

/// Apply `effect` to an RGBA image. `light` is the appearance it is drawn in:
/// effects print on white paper in light mode and black in dark mode.
pub fn apply(image: RgbaImage, effect: Effect, light: bool) -> RgbaImage {
    if effect == Effect::None {
        return image;
    }
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return image;
    }
    let source = Source {
        w: width,
        h: height,
        rgba: image.as_raw(),
    };
    let bytes = match effect {
        Effect::None => unreachable!("handled above"),
        Effect::Scanlines => scanlines(&source, light),
        Effect::Ascii => ascii(&source, light),
        Effect::Halftone => halftone(&source, light),
        Effect::Dither => dither(&source),
    };
    RgbaImage::from_raw(width, height, bytes).unwrap_or(image)
}

fn scanlines(s: &Source, light: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.rgba.len());
    for y in 0..s.h {
        let gain = if y % 3 == 0 { 0.52 } else { 1.0 };
        for x in 0..s.w {
            let [r, g, b, a] = s.color(x, y);
            let channel = |v: u8| {
                if light {
                    (v as f32 + (255.0 - v as f32) * (1.0 - gain)) as u8
                } else {
                    (v as f32 * gain) as u8
                }
            };
            out.extend_from_slice(&[channel(r), channel(g), channel(b), a]);
        }
    }
    out
}

fn ascii(s: &Source, light: bool) -> Vec<u8> {
    // Five-column bitmap glyphs, one column/row of spacing.
    const GLYPHS: [[u8; 7]; 10] = [
        [0, 0, 0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0, 4, 0],
        [0, 4, 0, 0, 4, 0, 0],
        [0, 0, 0, 14, 0, 0, 0],
        [0, 0, 14, 0, 14, 0, 0],
        [0, 4, 4, 31, 4, 4, 0],
        [0, 21, 14, 31, 14, 21, 0],
        [10, 10, 31, 10, 31, 10, 10],
        [17, 2, 4, 4, 8, 16, 17],
        [14, 17, 23, 21, 23, 16, 14],
    ];
    let mut out = Vec::with_capacity(s.rgba.len());
    for y in 0..s.h {
        for x in 0..s.w {
            let sx = (x / 6 * 6 + 3).min(s.w - 1);
            let sy = (y / 8 * 8 + 4).min(s.h - 1);
            let l = s.luma(sx, sy);
            let density = if light { 255 - l } else { l };
            let index = ((density as f32 / 255.0).sqrt() * 9.0) as usize;
            let ink = x % 6 < 5
                && y % 8 < 7
                && GLYPHS[index.min(9)][y as usize % 8] & (1 << (4 - x % 6)) != 0;
            let [r, g, b, a] = s.color(x, y);
            let [cr, cg, cb, _] = s.color(sx, sy);
            let paper = if light { 255.0 } else { 0.0 };
            let mix = |base: u8, glyph: u8| {
                (base as f32 * 0.60
                    + if ink {
                        glyph as f32 * 0.40
                    } else {
                        paper * 0.40
                    }) as u8
            };
            out.extend_from_slice(&[mix(r, cr), mix(g, cg), mix(b, cb), a]);
        }
    }
    out
}

fn halftone(s: &Source, light: bool) -> Vec<u8> {
    let paper = if light { 255u8 } else { 0 };
    let mut out = vec![0u8; s.rgba.len()];
    for px in out.as_chunks_mut::<4>().0.iter_mut() {
        px.copy_from_slice(&[paper, paper, paper, 255]);
    }
    for y in (0..s.h).step_by(4) {
        for x in (0..s.w).step_by(4) {
            let l = s.luma(x, y);
            let l = if light { 255 - l } else { l };
            let radius = 2.0 * (0.3 + 0.7 * (l as f32 / 255.0).sqrt());
            let [r, g, b, a] = s.color((x + 2).min(s.w - 1), (y + 2).min(s.h - 1));
            for dy in 0..4.min(s.h - y) {
                for dx in 0..4.min(s.w - x) {
                    let distance = ((dx as f32 - 1.5).powi(2) + (dy as f32 - 1.5).powi(2)).sqrt();
                    let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0) * a as f32 / 255.0;
                    let [sr, sg, sb, sa] = s.color(x + dx, y + dy);
                    let blend = |source: u8, dot: u8| {
                        (source as f32 * 0.60
                            + (dot as f32 * coverage + paper as f32 * (1.0 - coverage)) * 0.40)
                            as u8
                    };
                    let i = (((y + dy) * s.w + x + dx) * 4) as usize;
                    out[i..i + 4].copy_from_slice(&[blend(sr, r), blend(sg, g), blend(sb, b), sa]);
                }
            }
        }
    }
    out
}

fn dither(s: &Source) -> Vec<u8> {
    const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    let mut out = vec![0u8; s.rgba.len()];
    for y in (0..s.h).step_by(2) {
        for x in (0..s.w).step_by(2) {
            let [r, g, b, a] = s.color((x + 1).min(s.w - 1), (y + 1).min(s.h - 1));
            let threshold = BAYER[y as usize / 2 % 4][x as usize / 2 % 4];
            let peak = r.max(g).max(b) as f32;
            let bright = peak / 255.0 > (threshold as f32 + 0.5) / 16.0;
            let gain = if bright { 255.0 / peak.max(1.0) } else { 0.08 };
            let color = [
                (r as f32 * gain).round() as u8,
                (g as f32 * gain).round() as u8,
                (b as f32 * gain).round() as u8,
                a,
            ];
            for dy in 0..2.min(s.h - y) {
                for dx in 0..2.min(s.w - x) {
                    let i = (((y + dy) * s.w + x + dx) * 4) as usize;
                    out[i..i + 4].copy_from_slice(&color);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> RgbaImage {
        // A two-band image so every effect has light and dark regions to read.
        let mut image = RgbaImage::new(16, 16);
        for y in 0..16 {
            for x in 0..16 {
                let value = if x < 8 { 30 } else { 220 };
                image.put_pixel(x, y, image::Rgba([value, value / 2, 40, 255]));
            }
        }
        image
    }

    #[test]
    fn effect_keys_round_trip() {
        for effect in Effect::ALL {
            assert_eq!(Effect::parse(effect.as_str()), Some(effect));
        }
        assert_eq!(Effect::parse("unknown"), None);
    }

    #[test]
    fn none_is_a_passthrough() {
        let source = image();
        assert_eq!(apply(source.clone(), Effect::None, true), source);
    }

    #[test]
    fn every_effect_keeps_dimensions_and_changes_pixels() {
        let source = image();
        for effect in Effect::ALL.into_iter().filter(|e| *e != Effect::None) {
            for light in [true, false] {
                let out = apply(source.clone(), effect, light);
                assert_eq!(out.dimensions(), source.dimensions(), "{effect:?}");
                assert_ne!(out, source, "{effect:?} light={light} did nothing");
            }
        }
    }

    #[test]
    fn scanlines_differ_between_appearances() {
        let source = image();
        let dark = apply(source.clone(), Effect::Scanlines, false);
        let light = apply(source, Effect::Scanlines, true);
        assert_ne!(dark, light);
        // In dark mode the first row is the darkened scanline; the next is
        // untouched.
        assert!(dark.get_pixel(0, 0).0[0] < dark.get_pixel(0, 1).0[0]);
    }

    #[test]
    fn tiny_images_do_not_panic() {
        for effect in Effect::ALL {
            let _ = apply(RgbaImage::new(1, 1), effect, true);
            let _ = apply(RgbaImage::new(0, 0), effect, false);
        }
    }
}
