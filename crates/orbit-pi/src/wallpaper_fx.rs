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

/// Bounds for the artwork effect's pixel-size multiplier. `1` is the effect's
/// native cell size; lower values make finer grain, higher values make larger
/// dither blocks, halftone dots, ASCII glyphs, and scanline spacing.
pub const EFFECT_SCALE_RANGE: (f32, f32) = (0.5, 4.0);

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
/// `intensity` in `0..=1` scales how much of the treatment replaces the
/// photograph; `0` is a passthrough and `1` is the effect's full strength.
/// `scale` multiplies the effect's native pixel/cell size.
pub fn apply(
    image: RgbaImage,
    effect: Effect,
    light: bool,
    intensity: f32,
    scale: f32,
) -> RgbaImage {
    let intensity = intensity.clamp(0.0, 1.0);
    let scale = scale.clamp(EFFECT_SCALE_RANGE.0, EFFECT_SCALE_RANGE.1);
    if effect == Effect::None || intensity <= 0.0 {
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
        Effect::Scanlines => scanlines(&source, light, intensity, scale),
        Effect::Ascii => ascii(&source, light, intensity, scale),
        Effect::Halftone => halftone(&source, light, intensity, scale),
        Effect::Dither => dither(&source, intensity, scale),
    };
    RgbaImage::from_raw(width, height, bytes).unwrap_or(image)
}

fn scanlines(s: &Source, light: bool, intensity: f32, scale: f32) -> Vec<u8> {
    let period = ((3.0 * scale).round() as u32).max(1);
    let mut out = Vec::with_capacity(s.rgba.len());
    for y in 0..s.h {
        let gain = if y % period == 0 {
            1.0 - (1.0 - 0.52) * intensity
        } else {
            1.0
        };
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

fn ascii(s: &Source, light: bool, intensity: f32, scale: f32) -> Vec<u8> {
    // Five-column bitmap glyphs, one column/row of spacing at scale 1.
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
    let cell_w = ((6.0 * scale).round() as u32).max(3);
    let cell_h = ((8.0 * scale).round() as u32).max(3);
    let inner_w = cell_w.saturating_sub(1).max(1);
    let inner_h = cell_h.saturating_sub(1).max(1);
    let mut out = Vec::with_capacity(s.rgba.len());
    for y in 0..s.h {
        for x in 0..s.w {
            let sx = (x / cell_w * cell_w + cell_w / 2).min(s.w - 1);
            let sy = (y / cell_h * cell_h + cell_h / 2).min(s.h - 1);
            let l = s.luma(sx, sy);
            let density = if light { 255 - l } else { l };
            let index = ((density as f32 / 255.0).sqrt() * 9.0) as usize;
            let local_x = x % cell_w;
            let local_y = y % cell_h;
            let ink = local_x < inner_w && local_y < inner_h && {
                let gx = (local_x * 5 / inner_w).min(4);
                let gy = (local_y * 7 / inner_h).min(6);
                GLYPHS[index.min(9)][gy as usize] & (1 << (4 - gx)) != 0
            };
            let [r, g, b, a] = s.color(x, y);
            let [cr, cg, cb, _] = s.color(sx, sy);
            let paper = if light { 255.0 } else { 0.0 };
            let weight = 0.40 * intensity;
            let mix = |base: u8, glyph: u8| {
                (base as f32 * (1.0 - weight)
                    + if ink {
                        glyph as f32 * weight
                    } else {
                        paper * weight
                    }) as u8
            };
            out.extend_from_slice(&[mix(r, cr), mix(g, cg), mix(b, cb), a]);
        }
    }
    out
}

fn halftone(s: &Source, light: bool, intensity: f32, scale: f32) -> Vec<u8> {
    let paper = if light { 255u8 } else { 0 };
    let weight = 0.40 * intensity;
    let cell = ((4.0 * scale).round() as u32).max(2);
    let center = (cell as f32 - 1.0) / 2.0;
    let max_radius = cell as f32 / 2.0;
    let mut out = vec![0u8; s.rgba.len()];
    for px in out.as_chunks_mut::<4>().0.iter_mut() {
        px.copy_from_slice(&[paper, paper, paper, 255]);
    }
    for y in (0..s.h).step_by(cell as usize) {
        for x in (0..s.w).step_by(cell as usize) {
            let l = s.luma(x, y);
            let l = if light { 255 - l } else { l };
            let radius = max_radius * (0.3 + 0.7 * (l as f32 / 255.0).sqrt());
            let [r, g, b, a] = s.color((x + cell / 2).min(s.w - 1), (y + cell / 2).min(s.h - 1));
            for dy in 0..cell.min(s.h - y) {
                for dx in 0..cell.min(s.w - x) {
                    let distance =
                        ((dx as f32 - center).powi(2) + (dy as f32 - center).powi(2)).sqrt();
                    let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0) * a as f32 / 255.0;
                    let [sr, sg, sb, sa] = s.color(x + dx, y + dy);
                    let blend = |source: u8, dot: u8| {
                        (source as f32 * (1.0 - weight)
                            + (dot as f32 * coverage + paper as f32 * (1.0 - coverage)) * weight)
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

fn dither(s: &Source, intensity: f32, scale: f32) -> Vec<u8> {
    const BAYER: [[u8; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];
    // Ditherit's scale is the output pixel size, not a multiplier on a
    // pre-existing two-pixel cell. Extract a small palette from the image so
    // Bayer screening retains its colours rather than boosting the brightest
    // channel to white.
    let cell = (scale.round() as u32).max(1);
    let palette = dither_palette(s);
    let mut out = vec![0u8; s.rgba.len()];
    for y in (0..s.h).step_by(cell as usize) {
        for x in (0..s.w).step_by(cell as usize) {
            let [r, g, b, _] = s.color((x + cell / 2).min(s.w - 1), (y + cell / 2).min(s.h - 1));
            let threshold = BAYER[y as usize / cell as usize % 4][x as usize / cell as usize % 4];
            let offset = 128.0 - (threshold as f32 * 16.0);
            let adjusted = [r, g, b].map(|v| (v as f32 + offset).clamp(0.0, 255.0));
            let nearest = palette
                .iter()
                .min_by(|a, b| {
                    let distance = |color: &[f32; 3]| {
                        0.2126 * (adjusted[0] - color[0]).powi(2)
                            + 0.7152 * (adjusted[1] - color[1]).powi(2)
                            + 0.0722 * (adjusted[2] - color[2]).powi(2)
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .expect("palette is nonempty");
            let mix = |source: u8, target: f32| {
                (source as f32 + (target - source as f32) * intensity).round() as u8
            };

            for dy in 0..cell.min(s.h - y) {
                for dx in 0..cell.min(s.w - x) {
                    let i = (((y + dy) * s.w + x + dx) * 4) as usize;
                    let [sr, sg, sb, sa] = s.color(x + dx, y + dy);
                    out[i..i + 4].copy_from_slice(&[
                        mix(sr, nearest[0]),
                        mix(sg, nearest[1]),
                        mix(sb, nearest[2]),
                        sa,
                    ]);
                }
            }
        }
    }
    out
}

// Deterministic eight-colour palette, fitted to a bounded sample grid.
fn dither_palette(s: &Source<'_>) -> Vec<[f32; 3]> {
    let mut samples = Vec::new();
    for y in 0..s.h.min(32) {
        for x in 0..s.w.min(32) {
            let [r, g, b, a] = s.color(x * s.w / s.w.min(32), y * s.h / s.h.min(32));
            if a != 0 {
                samples.push([r as f32, g as f32, b as f32]);
            }
        }
    }
    if samples.is_empty() {
        return vec![[0.0; 3]];
    }
    samples.sort_by(|a, b| {
        let luma = |v: &[f32; 3]| 0.2126 * v[0] + 0.7152 * v[1] + 0.0722 * v[2];
        luma(a).total_cmp(&luma(b))
    });
    let mut palette: Vec<_> = (0..8)
        .map(|i| samples[i * (samples.len() - 1) / 7])
        .collect();
    for _ in 0..6 {
        let mut sums = [[0.0f32; 3]; 8];
        let mut counts = [0u32; 8];
        for sample in &samples {
            let (index, _) = palette
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    let distance = |color: &[f32; 3]| {
                        0.2126 * (sample[0] - color[0]).powi(2)
                            + 0.7152 * (sample[1] - color[1]).powi(2)
                            + 0.0722 * (sample[2] - color[2]).powi(2)
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .expect("palette is nonempty");
            counts[index] += 1;
            for channel in 0..3 {
                sums[index][channel] += sample[channel];
            }
        }
        for i in 0..8 {
            if counts[i] > 0 {
                for channel in 0..3 {
                    palette[i][channel] = sums[i][channel] / counts[i] as f32;
                }
            }
        }
    }
    palette
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
        assert_eq!(apply(source.clone(), Effect::None, true, 1.0, 1.0), source);
    }

    #[test]
    fn zero_intensity_is_a_passthrough() {
        let source = image();
        for effect in Effect::ALL {
            assert_eq!(
                apply(source.clone(), effect, true, 0.0, 1.0),
                source,
                "{effect:?}"
            );
        }
    }

    #[test]
    fn every_effect_keeps_dimensions_and_changes_pixels() {
        let source = image();
        for effect in Effect::ALL.into_iter().filter(|e| *e != Effect::None) {
            for light in [true, false] {
                let out = apply(source.clone(), effect, light, 1.0, 1.0);
                assert_eq!(out.dimensions(), source.dimensions(), "{effect:?}");
                assert_ne!(out, source, "{effect:?} light={light} did nothing");
                let half = apply(source.clone(), effect, light, 0.5, 1.0);
                assert_ne!(half, source, "{effect:?} at half intensity did nothing");
                assert_ne!(half, out, "{effect:?} half matches full intensity");
            }
        }
    }

    #[test]
    fn every_effect_changes_with_pixel_size() {
        let source = image();
        for effect in Effect::ALL.into_iter().filter(|e| *e != Effect::None) {
            let fine = apply(source.clone(), effect, true, 1.0, 1.0);
            let native = apply(source.clone(), effect, true, 1.0, 2.0);
            let coarse = apply(source.clone(), effect, true, 1.0, 3.0);
            assert_ne!(fine, native, "{effect:?} ignores a smaller cell size");
            assert_ne!(native, coarse, "{effect:?} ignores a larger cell size");
        }
    }

    #[test]
    fn dither_scale_four_produces_four_pixel_cells_and_preserves_alpha() {
        let mut source = image();
        source.get_pixel_mut(1, 1).0[3] = 42;
        let output = apply(source, Effect::Dither, false, 1.0, 4.0);
        let first = output.get_pixel(0, 0).0;
        for y in 0..4 {
            for x in 0..4 {
                let pixel = output.get_pixel(x, y).0;
                assert_eq!(&pixel[..3], &first[..3]);
            }
        }
        assert_eq!(output.get_pixel(1, 1).0[3], 42);
    }

    #[test]
    fn scanlines_differ_between_appearances() {
        let source = image();
        let dark = apply(source.clone(), Effect::Scanlines, false, 1.0, 1.0);
        let light = apply(source, Effect::Scanlines, true, 1.0, 1.0);
        assert_ne!(dark, light);
        // In dark mode the first row is the darkened scanline; the next is
        // untouched.
        assert!(dark.get_pixel(0, 0).0[0] < dark.get_pixel(0, 1).0[0]);
    }

    #[test]
    fn scanlines_soften_with_intensity() {
        let source = image();
        let full = apply(source.clone(), Effect::Scanlines, false, 1.0, 1.0);
        let half = apply(source.clone(), Effect::Scanlines, false, 0.5, 1.0);
        // The darkened row must stay between the source pixel and the full
        // effect, so lowering intensity never overshoots.
        let row = full.get_pixel(0, 0).0[0];
        let softened = half.get_pixel(0, 0).0[0];
        let original = source.get_pixel(0, 0).0[0];
        assert!(row < softened && softened < original);
    }

    #[test]
    fn tiny_images_do_not_panic() {
        for effect in Effect::ALL {
            let _ = apply(RgbaImage::new(1, 1), effect, true, 1.0, 1.0);
            let _ = apply(RgbaImage::new(0, 0), effect, false, 1.0, 4.0);
        }
    }
}
