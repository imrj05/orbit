//! Binary ordered-dither backdrop for the new-task page. A configured
//! wallpaper takes precedence; the caller owns visibility and motion clocks.
//!
//! The two independently seeded 3D value-noise fields are sampled on a coarse
//! grid and interpolated before applying an 8×8 Bayer screen at raster resolution.
//! This is an independent implementation, not the preview's shader.

use std::sync::Arc;

use gpui::{
    canvas, div, AppContext, Corners, IntoElement, ParentElement, RenderImage, Rgba, Styled,
};
use image::{Frame, RgbaImage};

use crate::theme::Theme;

const BAYER: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

// A logical-pixel screen caps CPU work and atlas size on high-DPI displays.
// Update at 5 Hz, independent of the caller's faster UI heartbeat.
const FPS: f32 = 5.;
const GRID: usize = 8;
const MAX_SIDE: u32 = 2048;
const MARK_W: f32 = 0.36;
const MARK_MAX_W: f32 = 560.;
const MARK_MIN_W: f32 = 200.;
const MARK_CY: f32 = 0.74;
const MASK_COLS: u32 = 64;

type Key = (u32, u32, i64, [u8; 4], [u8; 4]);
#[derive(Default)]
struct Cache {
    image: Option<(Key, Arc<RenderImage>)>,
    /// The size the previous frame laid out. A raster is only requested once
    /// the size holds still, so an animated column (the sessions sidebar
    /// sliding) does not queue one full field per frame.
    size: (u32, u32),
    pending: bool,
}

/// What one frame does with the field it has: whether to ask for a new raster,
/// and whether the cached one still belongs on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FramePlan {
    request: bool,
    paint: bool,
}

/// Decide a frame from the key it wants and the field it has.
///
/// The two answers are independent. A size that is still moving is not worth a
/// raster yet — a slide would queue one full field per frame, each a texture
/// upload for a width that no longer exists — so the request waits for the
/// size to hold. The field it leaves behind is still this page's own colours at
/// an older size, and that keeps painting into the new bounds: stretching the
/// dither for the length of a slide is invisible, blanking it is a flash across
/// the whole page. Only a *colour* change blanks, because the wrong background
/// would be a lie rather than a stretch.
fn frame_plan(key: Key, cached: Option<Key>, laid_out: (u32, u32), pending: bool) -> FramePlan {
    let settled = laid_out == (key.0, key.1);
    FramePlan {
        request: !pending && cached.is_none_or(|old| old != key) && (settled || cached.is_none()),
        paint: cached.is_some_and(|old| old.3 == key.3 && old.4 == key.4),
    }
}

fn color(value: gpui::Hsla) -> [u8; 4] {
    let c: Rgba = value.into();
    [
        (c.b * 255.).round() as u8,
        (c.g * 255.).round() as u8,
        (c.r * 255.).round() as u8,
        255,
    ]
}

pub fn layer(theme: Theme, phase: f32) -> impl IntoElement + use<> {
    let background = color(theme.background);
    let ink = color(theme.accent);
    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .child(
            canvas(
                move |bounds, window, cx| {
                    let width = (f32::from(bounds.size.width).ceil() as u32).clamp(1, MAX_SIDE);
                    let height = (f32::from(bounds.size.height).ceil() as u32).clamp(1, MAX_SIDE);
                    let tick = (phase.max(0.) * FPS).floor() as i64;
                    let key = (width, height, tick, background, ink);
                    // State belongs to this element in this window, not to the UI thread.
                    let cache = window.use_keyed_state("dither-cache", cx, |window, cx| {
                        cx.on_release_in(window, |cache: &mut Cache, window, _| {
                            if let Some((_, image)) = cache.image.take() {
                                let _ = window.drop_image(image);
                            }
                        })
                        .detach();
                        Cache::default()
                    });
                    cache.update(cx, |cache, cx| {
                        let plan = frame_plan(
                            key,
                            cache.image.as_ref().map(|(old, _)| *old),
                            cache.size,
                            cache.pending,
                        );
                        cache.size = (width, height);
                        if plan.request {
                            cache.pending = true;
                            let task = cx.background_spawn(async move {
                                raster(width, height, tick as f32 / FPS, background, ink)
                            });
                            cx.spawn_in(window, async move |weak, cx| {
                                let pixels = task.await;
                                let _ = weak.update_in(cx, |cache, window, cx| {
                                    let image =
                                        Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
                                    if let Some((_, old)) = cache.image.replace((key, image)) {
                                        let _ = window.drop_image(old);
                                    }
                                    cache.pending = false;
                                    cx.notify();
                                });
                            })
                            .detach();
                        }
                        if !plan.paint {
                            return None;
                        }
                        cache.image.as_ref().map(|(_, image)| image.clone())
                    })
                },
                move |bounds, image, window, _| {
                    if let Some(image) = image {
                        if let Err(error) =
                            window.paint_image(bounds, Corners::default(), image, 0, false)
                        {
                            eprintln!("could not paint new-task backdrop: {error}");
                        }
                    }
                },
            )
            .size_full(),
        )
}

fn hash(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846c_a68b);
    h ^= h >> 16;
    h as f32 / u32::MAX as f32
}

fn noise(x: f32, y: f32, z: f32, seed: u32) -> f32 {
    let (ix, iy, iz) = (x.floor() as i32, y.floor() as i32, z.floor() as i32);
    let fade = |t: f32| t * t * (3. - 2. * t);
    let (fx, fy, fz) = (fade(x.fract()), fade(y.fract()), fade(z.fract()));
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let plane = |dz| {
        let a = lerp(
            hash(ix, iy, iz + dz, seed),
            hash(ix + 1, iy, iz + dz, seed),
            fx,
        );
        let b = lerp(
            hash(ix, iy + 1, iz + dz, seed),
            hash(ix + 1, iy + 1, iz + dz, seed),
            fx,
        );
        lerp(a, b, fy)
    };
    lerp(plane(0), plane(1), fz)
}

fn field(x: f32, y: f32, phase: f32) -> f32 {
    let a = noise(x * 2.3 + 4., y * 2.3 + 7., phase * 0.035 + 3., 41);
    let b = noise(x * 3.5 + 12., y * 3.5 + 5., phase * 0.024 + 9., 971);
    (0.5 + (a - 0.5) * 1.35 + (b - 0.5) * 0.85).clamp(0., 1.)
}

/// The production monogram, stripped of the icon plate and cached across frames.
struct Mark {
    mask: image::GrayImage,
}

impl Mark {
    fn get() -> &'static Self {
        static MARK: std::sync::OnceLock<Mark> = std::sync::OnceLock::new();
        MARK.get_or_init(|| {
            let source = image::load_from_memory(crate::app_icon::MARK_PNG)
                .expect("the embedded mark PNG must decode")
                .to_luma8();
            let (width, height) = source.dimensions();
            // Erode the thin plate highlight, preserving the thick glyphs.
            let mut horizontal = image::GrayImage::new(width, height);
            for y in 0..height {
                for x in 3..width.saturating_sub(3) {
                    if (x - 3..=x + 3).all(|xx| source.get_pixel(xx, y).0[0] > 128) {
                        horizontal.put_pixel(x, y, image::Luma([255]));
                    }
                }
            }
            let mut mask = image::GrayImage::new(width, height);
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0, 0);
            for y in 3..height.saturating_sub(3) {
                for x in 0..width {
                    if (y - 3..=y + 3).all(|yy| horizontal.get_pixel(x, yy).0[0] > 0) {
                        mask.put_pixel(x, y, image::Luma([255]));
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                }
            }
            assert!(
                min_x < max_x && min_y < max_y,
                "the embedded mark must contain glyphs"
            );
            let cropped = image::imageops::crop_imm(
                &mask,
                min_x,
                min_y,
                max_x - min_x + 1,
                max_y - min_y + 1,
            )
            .to_image();
            let rows = ((MASK_COLS as f32 * cropped.height() as f32 / cropped.width() as f32)
                .round() as u32)
                .max(1);
            Self {
                mask: image::imageops::resize(
                    &cropped,
                    MASK_COLS,
                    rows,
                    image::imageops::FilterType::Triangle,
                ),
            }
        })
    }

    fn rect(&self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let (width, height) = (width as f32, height as f32);
        let mark_w = (width * MARK_W).min(MARK_MAX_W);
        let mark_h = mark_w * self.mask.height() as f32 / self.mask.width() as f32;
        if mark_w < MARK_MIN_W || mark_h >= height * 0.45 {
            return None;
        }
        Some((
            (width * 0.5 - mark_w * 0.5).round() as u32,
            (height * MARK_CY - mark_h * 0.5).round() as u32,
            mark_w.round() as u32,
            mark_h.round() as u32,
        ))
    }
}

fn raster(width: u32, height: u32, phase: f32, bg: [u8; 4], ink: [u8; 4]) -> RgbaImage {
    let (w, h) = (width as usize, height as usize);
    let cols = w.div_ceil(GRID) + 1;
    let rows = h.div_ceil(GRID) + 1;
    let scale = width.min(height).max(1) as f32;
    let mut grid = vec![0.; cols * rows];
    for y in 0..rows {
        for x in 0..cols {
            grid[y * cols + x] = field((x * GRID) as f32 / scale, (y * GRID) as f32 / scale, phase);
        }
    }
    let mark = Mark::get().rect(width, height).map(|(left, top, w, h)| {
        let mask = image::imageops::resize(
            &Mark::get().mask,
            w,
            h,
            image::imageops::FilterType::Triangle,
        );
        (left as usize, top as usize, mask)
    });
    let mut pixels = vec![0; w * h * 4];
    for y in 0..h {
        let gy = y / GRID;
        let fy = (y % GRID) as f32 / GRID as f32;
        for x in 0..w {
            let gx = x / GRID;
            let fx = (x % GRID) as f32 / GRID as f32;
            let top = grid[gy * cols + gx] * (1. - fx) + grid[gy * cols + gx + 1] * fx;
            let bottom =
                grid[(gy + 1) * cols + gx] * (1. - fx) + grid[(gy + 1) * cols + gx + 1] * fx;
            let mut value = top * (1. - fy) + bottom * fy;
            if let Some((left, top, mask)) = &mark {
                let coverage = x
                    .checked_sub(*left)
                    .zip(y.checked_sub(*top))
                    .filter(|(mx, my)| *mx < mask.width() as usize && *my < mask.height() as usize)
                    .map_or(0., |(mx, my)| {
                        mask.get_pixel(mx as u32, my as u32).0[0] as f32 / 255.
                    });
                // Compress the field across the whole screen, not just the mark's
                // bounding box. Glyphs lift its density while retaining the same
                // moving field and Bayer screen; counters remain background.
                value = 0.12 + 0.38 * value + coverage * (0.46 - 0.06 * value);
            }
            let threshold = (BAYER[y % 8][x % 8] as f32 + 0.5) / 64.;
            let pixel = if value > threshold { ink } else { bg };
            pixels[(y * w + x) * 4..][..4].copy_from_slice(&pixel);
        }
    }
    RgbaImage::from_raw(width, height, pixels).expect("raster dimensions match buffer")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BACKGROUND: [u8; 4] = [0, 0, 0, 255];
    const INK: [u8; 4] = [255, 255, 255, 255];

    fn key(width: u32, height: u32, tick: i64) -> Key {
        (width, height, tick, BACKGROUND, INK)
    }

    /// The field used to blank whenever its size changed, because the cached
    /// raster was only painted at exactly the bounds it was made for. Toggling
    /// the sessions sidebar animates the chat column's width, so every frame of
    /// the slide failed that check and the whole page fell back to a flat slab
    /// until the slide ended — a flash on every show and hide.
    #[test]
    fn a_field_that_outgrew_its_raster_keeps_painting() {
        let plan = frame_plan(
            key(1150, 878, 3),
            Some(key(1133, 878, 3)),
            (1133, 878),
            false,
        );
        assert!(
            plan.paint,
            "a size change must not blank the page mid-slide"
        );
        assert!(
            !plan.request,
            "a size that is still moving is not worth a raster yet"
        );
    }

    /// Once the slide lands, exactly one raster is made for the settled size:
    /// the frame after it is asked for, and no frame before it is.
    #[test]
    fn the_settled_size_asks_for_one_raster() {
        assert_eq!(
            frame_plan(
                key(1410, 878, 3),
                Some(key(1133, 878, 3)),
                (1410, 878),
                false
            ),
            FramePlan {
                request: true,
                paint: true
            }
        );
        assert!(
            !frame_plan(
                key(1410, 878, 3),
                Some(key(1410, 878, 3)),
                (1410, 878),
                false
            )
            .request,
            "a field that already matches its bounds is not re-made"
        );
    }

    #[test]
    fn a_stale_theme_blanks_instead_of_painting_the_wrong_colours() {
        let recoloured = (1410, 878, 3, [255, 0, 0, 255], INK);
        assert!(
            !frame_plan(recoloured, Some(key(1410, 878, 3)), (1410, 878), false).paint,
            "an old palette must not stand in for the new one"
        );
    }

    #[test]
    fn the_first_frame_asks_immediately_even_while_the_window_settles() {
        assert_eq!(
            frame_plan(key(913, 817, 0), None, (0, 0), false),
            FramePlan {
                request: true,
                paint: false
            },
            "nothing on screen to keep, so there is nothing to wait for"
        );
        assert!(
            !frame_plan(key(913, 817, 0), None, (0, 0), true).request,
            "one raster in flight at a time"
        );
    }

    #[test]
    fn bayer_has_all_thresholds() {
        let mut thresholds = BAYER.into_iter().flatten().collect::<Vec<_>>();
        thresholds.sort();
        assert_eq!(thresholds, (0..64).collect::<Vec<_>>());
    }
    #[test]
    fn raster_is_binary_and_evolves() {
        let bg = [0, 0, 0, 255];
        let ink = [255, 255, 255, 255];
        let a = raster(256, 192, 0., bg, ink);
        let b = raster(256, 192, 30., bg, ink);
        let mut count = 0;
        let mut changed = 0;
        for (a, b) in a.pixels().zip(b.pixels()) {
            assert!(a.0 == bg || a.0 == ink);
            count += usize::from(a.0 == ink);
            changed += usize::from(a != b);
        }
        assert!((5000..44000).contains(&count));
        assert!(changed > 1000);
    }
    #[gpui::test]
    fn layer_draws_in_a_window(cx: &mut gpui::TestAppContext) {
        use gpui::{point, px, size, AppContext, Window};
        let cx = cx.add_empty_window();
        let view = cx.update(|_, cx| {
            cx.new(|_| {
                struct Layer;
                impl gpui::Render for Layer {
                    fn render(
                        &mut self,
                        _: &mut Window,
                        _: &mut gpui::Context<Self>,
                    ) -> impl IntoElement {
                        div()
                            .size_full()
                            .child(layer(Theme::for_id(crate::theme::ThemeId::Orbit), 0.))
                    }
                }
                Layer
            })
        });
        let _ = cx.draw(point(px(0.), px(0.)), size(px(320.), px(240.)), |_, _| {
            view.clone()
        });
        // Complete the background job and exercise painting its result too.
        cx.run_until_parked();
        let _ = cx.draw(point(px(0.), px(0.)), size(px(320.), px(240.)), |_, _| {
            view.clone()
        });
    }

    #[test]
    fn mark_placement_and_small_fallback() {
        let mark = Mark::get();
        assert!(mark.rect(360, 300).is_none());
        assert!(mark.rect(1280, 360).is_none());
        let (x, y, w, h) = mark.rect(1280, 820).unwrap();
        assert!((x as f32 + w as f32 / 2. - 640.).abs() < 1.);
        assert!((y as f32 + h as f32 / 2. - 820. * MARK_CY).abs() < 1.);
        assert!((2.0..2.4).contains(&(w as f32 / h as f32)));
    }

    #[test]
    fn mark_and_counters_remain_visible_across_phases_and_palettes() {
        let mark = Mark::get();
        let (left, top, w, h) = mark.rect(800, 600).unwrap();
        let mask = image::imageops::resize(&mark.mask, w, h, image::imageops::FilterType::Triangle);
        for (bg, ink) in [
            ([0, 0, 0, 255], [255, 255, 255, 255]),
            ([255, 255, 255, 255], [0, 0, 0, 255]),
        ] {
            let images = [
                raster(800, 600, 0., bg, ink),
                raster(800, 600, 30., bg, ink),
            ];
            let mut changed_glyph = 0;
            for (phase, image) in [0., 30.].into_iter().zip(&images) {
                let (mut glyph_ink, mut glyph_total, mut outside_ink, mut outside_total) =
                    (0, 0, 0, 0);
                for y in 0..h {
                    for x in 0..w {
                        let pixel = image.get_pixel(left + x, top + y).0;
                        assert!(pixel == bg || pixel == ink, "the mark must stay binary");
                        let coverage = mask.get_pixel(x, y).0[0];
                        if coverage == 255 {
                            glyph_total += 1;
                            glyph_ink += usize::from(pixel == ink);
                            changed_glyph += usize::from(
                                images[0].get_pixel(left + x, top + y)
                                    != images[1].get_pixel(left + x, top + y),
                            );
                        } else if coverage == 0 {
                            outside_total += 1;
                            outside_ink += usize::from(pixel == ink);
                            // Compare samples inside the mark box to the same
                            // interpolated field used everywhere outside it.
                            if x % 11 == 0 && y % 11 == 0 {
                                let (sx, sy) = ((left + x) as usize, (top + y) as usize);
                                let (gx, gy) = (sx / GRID, sy / GRID);
                                let (fx, fy) = (
                                    (sx % GRID) as f32 / GRID as f32,
                                    (sy % GRID) as f32 / GRID as f32,
                                );
                                let sample = |x: usize, y: usize| {
                                    field((x * GRID) as f32 / 600., (y * GRID) as f32 / 600., phase)
                                };
                                let a = sample(gx, gy) * (1. - fx) + sample(gx + 1, gy) * fx;
                                let b =
                                    sample(gx, gy + 1) * (1. - fx) + sample(gx + 1, gy + 1) * fx;
                                let value = 0.12 + 0.38 * (a * (1. - fy) + b * fy);
                                let threshold = (BAYER[sy % 8][sx % 8] as f32 + 0.5) / 64.;
                                assert_eq!(
                                    pixel,
                                    if value > threshold { ink } else { bg },
                                    "no plate behind glyphs"
                                );
                            }
                        }
                    }
                }
                assert!(
                    glyph_ink > 0 && glyph_ink < glyph_total,
                    "mixed glyph dither"
                );
                assert!(
                    outside_ink > 0 && outside_ink < outside_total,
                    "no solid plate"
                );
                assert!(
                    (glyph_ink as f32 / glyph_total as f32)
                        > (outside_ink as f32 / outside_total as f32) + 0.3,
                    "glyph density stands out"
                );
                // The O counter must contain both screen colors, not a solid fill.
                let (cx, cy) = ((0.21 * w as f32) as u32, (0.45 * h as f32) as u32);
                let mut counter_ink = 0;
                for y in cy - 4..cy + 4 {
                    for x in cx - 4..cx + 4 {
                        assert_eq!(mask.get_pixel(x, y).0[0], 0);
                        counter_ink += usize::from(image.get_pixel(left + x, top + y).0 == ink);
                    }
                }
                assert!(
                    (1..64).contains(&counter_ink),
                    "counter retains background dither"
                );
            }
            assert!(changed_glyph > 0, "glyph dither responds to animation");
        }
    }

    #[test]
    fn fields_are_smooth_and_time_dependent() {
        let a = noise(1.3, 2.2, 3.4, 41);
        assert!((a - noise(1.301, 2.2, 3.4, 41)).abs() < 0.01);
        assert_ne!(a, noise(1.3, 2.2, 4.4, 41));
        assert_ne!(a, noise(1.3, 2.2, 3.4, 971));
    }
}
