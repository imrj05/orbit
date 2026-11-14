//! Ordered-dither backdrop for the new-task page. A configured wallpaper takes
//! precedence; the caller owns visibility and motion clocks.
//!
//! The page has one colour rule: the plane is the theme's own `background`, and
//! the accent lives in the dots and nowhere else. So the screen is a halftone,
//! not a pixel mask — every cell of an 8×8 Bayer matrix carries a dot whose
//! radius and accent strength come from that cell's level. A field of full-
//! strength pixels would tint the whole page; four dot sizes on the theme's
//! canvas read as paper texture instead, and the accent never becomes a wash.
//!
//! The two independently seeded 3D value-noise fields are sampled on a coarse
//! grid and interpolated before the screen quantizes them. This is an
//! independent implementation, not the wallpaper preview's shader.
//!
//! The app monogram is screened into the same field from the embedded mark PNG,
//! so the watermark cannot drift from the icon, and a band of light crosses its
//! glyphs every twenty seconds.

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
/// Cell pitch, in raster pixels: the ruling every dot sits on.
const PITCH: f32 = 9.;
/// Dot levels the screen quantizes into. Level 0 is bare canvas.
const LEVELS: usize = 4;
/// Dot radius per level, as a fraction of [`PITCH`]. Small on purpose: the
/// screen is the page's paper, and a halftone carrying much more ink than this
/// stops being texture and becomes a pattern. The steps are uneven because a
/// dot's area grows with the square of its radius.
const RADII: [f32; LEVELS] = [0., 0.10, 0.15, 0.20];
/// Accent strength per level, as a fraction of the accent's own alpha. The
/// accent is the dots' only job, so it tops out well short of full strength:
/// the page reads as the theme's `background` carrying a faint dot grid, never
/// as an accent-tinted plane.
const LEVEL_INK: [f32; LEVELS] = [0., 0.32, 0.42, 0.52];
/// The field's resting density, before the noise swings it and the mark takes
/// over. With [`SWING`] it spans roughly 0.16–0.44, so the screen ranges from
/// bare cells to the middle of the ladder and never reaches a solid field.
const DENSITY: f32 = 0.30;
/// How far the noise field can move the density, either side of [`DENSITY`].
const SWING: f32 = 0.14;
/// Density inside the glyphs when the light is elsewhere: denser than the page
/// grain, so the watermark is always legible, and never the largest dot.
const MARK_REST: f32 = 0.55;
/// Density inside a glyph the light is crossing: the largest dot the screen has.
const MARK_LIT: f32 = 1.;
/// Seconds for the light to cross the mark and come round again.
const SWEEP_PERIOD: f32 = 20.;
/// The light band's half-width in mark coordinates, and where it fades to
/// nothing. Wide and soft: a spotlight, not a scanline.
const LIT_IN: f32 = 0.30;
const LIT_OUT: f32 = 0.55;
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
    /// The field's next frame, rastered while the current one is on screen.
    /// The frame that reaches it paints it at once instead of painting the
    /// old field, asking for a raster and repainting the whole window again
    /// when it lands — two window paints per field frame, one of them for
    /// pixels that had not changed.
    next: Option<(Key, Arc<RenderImage>)>,
    /// The key the latest frame asked for. A raster ahead that lands after the
    /// page has already moved past the frame it was made for repaints, so the
    /// field catches up instead of waiting for the next beat.
    wanted: Option<Key>,
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

/// The field frame a phase lands on. The heartbeat asks for a repaint only
/// when this changes, and the raster cache is keyed on it, so both move at the
/// field's own cadence rather than at the caller's.
pub fn frame(phase: f32) -> i64 {
    (phase.max(0.) * FPS).floor() as i64
}

/// The frame to raster ahead of time, if any: the one after the frame on
/// screen, once that frame is current and nothing else is in flight. A still
/// field (Reduce Motion) never advances, so it asks for nothing.
fn ahead(
    key: Key,
    cached: Option<Key>,
    next: Option<Key>,
    pending: bool,
    live: bool,
) -> Option<Key> {
    let following = (key.0, key.1, key.2 + 1, key.3, key.4);
    (live && !pending && cached == Some(key) && next != Some(following)).then_some(following)
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

/// The field as a full-bleed layer. `live` says whether the field is moving,
/// so the next frame is worth rastering before it is due.
pub fn layer(theme: Theme, phase: f32, live: bool) -> impl IntoElement + use<> {
    // The plane is the palette's own canvas: the backdrop never paints a wash
    // over it. The accent is only the dots' ink.
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
                    let tick = frame(phase);
                    let key = (width, height, tick, background, ink);
                    // State belongs to this element in this window, not to the UI thread.
                    let cache = window.use_keyed_state("dither-cache", cx, |window, cx| {
                        cx.on_release_in(window, |cache: &mut Cache, window, _| {
                            for (_, image) in [cache.image.take(), cache.next.take()]
                                .into_iter()
                                .flatten()
                            {
                                let _ = window.drop_image(image);
                            }
                        })
                        .detach();
                        Cache::default()
                    });
                    cache.update(cx, |cache, cx| {
                        // The frame rastered ahead is due: it becomes the field.
                        if cache.next.as_ref().is_some_and(|(next, _)| *next == key) {
                            let promoted = cache.next.take();
                            if let Some((_, old)) = std::mem::replace(&mut cache.image, promoted) {
                                let _ = window.drop_image(old);
                            }
                        }
                        let plan = frame_plan(
                            key,
                            cache.image.as_ref().map(|(old, _)| *old),
                            cache.size,
                            cache.pending,
                        );
                        cache.size = (width, height);
                        cache.wanted = Some(key);
                        if plan.request {
                            // The field on screen is wrong for these bounds or
                            // colours: raster it now and repaint when it lands.
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
                        } else if let Some(following) = ahead(
                            key,
                            cache.image.as_ref().map(|(old, _)| *old),
                            cache.next.as_ref().map(|(next, _)| *next),
                            cache.pending,
                            live,
                        ) {
                            // Raster the next frame while this one shows. It
                            // lands silently: nothing on screen changes until
                            // the heartbeat reaches its frame and repaints.
                            cache.pending = true;
                            let task = cx.background_spawn(async move {
                                raster(width, height, following.2 as f32 / FPS, background, ink)
                            });
                            cx.spawn_in(window, async move |weak, cx| {
                                let pixels = task.await;
                                let _ = weak.update_in(cx, |cache, window, cx| {
                                    let image =
                                        Arc::new(RenderImage::new(vec![Frame::new(pixels)]));
                                    if let Some((_, old)) = cache.next.replace((following, image)) {
                                        let _ = window.drop_image(old);
                                    }
                                    cache.pending = false;
                                    // The page moved on while this was in
                                    // flight — to this frame, or to bounds or
                                    // colours that need a raster of their own,
                                    // which waited on this one. Either way the
                                    // next paint has work to do.
                                    if cache.wanted != Some(key) {
                                        cx.notify();
                                    }
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

    /// The mask at the size the mark is laid out at, so one sample per cell
    /// lands on the glyphs' own edges rather than the mask's coarse grid.
    fn at(rect: (u32, u32, u32, u32)) -> Placed {
        let (left, top, width, height) = rect;
        Placed {
            left: left as f32,
            top: top as f32,
            width: width as f32,
            height: height as f32,
            mask: image::imageops::resize(
                &Self::get().mask,
                width.max(1),
                height.max(1),
                image::imageops::FilterType::Triangle,
            ),
        }
    }
}

/// The monogram laid out on one layer: its box in raster pixels and its mask
/// resolved at that size.
struct Placed {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    mask: image::GrayImage,
}

impl Placed {
    /// How much of the glyph covers this point, or `None` outside its box.
    fn coverage(&self, x: f32, y: f32) -> Option<f32> {
        let (mx, my) = (x - self.left, y - self.top);
        if mx < 0. || my < 0. || mx >= self.width || my >= self.height {
            return None;
        }
        Some(self.mask.get_pixel(mx as u32, my as u32).0[0] as f32 / 255.)
    }

    /// Mark-local `x`, 0 at the first glyph and 1 at the last.
    fn local_u(&self, x: f32) -> f32 {
        if self.width <= 0. {
            0.
        } else {
            ((x - self.left) / self.width).clamp(0., 1.)
        }
    }
}

/// The page's density at raster pixel `(x, y)`, before the screen quantizes it.
fn sample(x: f32, y: f32, phase: f32, scale: f32, mark: Option<&Placed>) -> f32 {
    // A downward bias keeps the densest band behind the composer rather than
    // across the top of the page, where the eye lands first.
    let value = DENSITY + SWING * (field(x / scale, y / scale, phase) - 0.5) * 2.;
    let value = (value + 0.06 * (y / scale - 0.5)).clamp(0., 1.);
    let Some(mark) = mark else {
        return value;
    };
    let Some(coverage) = mark.coverage(x, y).filter(|coverage| *coverage > 0.) else {
        return value;
    };
    // The light travels across the mark and starts again, pausing off both ends
    // so the glyphs are fully dark and fully lit once per pass.
    let sweep = -LIT_OUT + (phase / SWEEP_PERIOD).fract() * (1. + 2. * LIT_OUT);
    let lit = smoothstep(LIT_OUT, LIT_IN, (mark.local_u(x) - sweep).abs());
    let glyph = MARK_REST + (MARK_LIT - MARK_REST) * lit;
    (value * (1. - coverage) + glyph * coverage).clamp(0., 1.)
}

/// Quantize one cell's density through the ordered matrix: the whole part picks
/// the level, the fractional part decides whether this cell rounds up.
fn dither(value: f32, threshold: u8) -> usize {
    let scaled = value * LEVELS as f32;
    let base = scaled.floor();
    let threshold = (threshold as f32 + 0.5) / 64.;
    let level = if scaled - base > threshold {
        base + 1.
    } else {
        base
    };
    (level.max(0.) as usize).min(LEVELS - 1)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// The screen, resolved into a cached image: the theme's `background` with a
/// dot per cell, each dot sized and inked by its level.
fn raster(width: u32, height: u32, phase: f32, bg: [u8; 4], ink: [u8; 4]) -> RgbaImage {
    let (w, h) = (width as usize, height as usize);
    let cols = (w as f32 / PITCH).ceil() as usize;
    let rows = (h as f32 / PITCH).ceil() as usize;
    let scale = width.min(height).max(1) as f32;

    // The mark's mask at the resolution it is laid out at, so one sample per
    // cell lands on the glyphs' own edges rather than the mask's coarse grid.
    let mark = Mark::get().rect(width, height).map(Mark::at);
    let mark = mark.as_ref();

    // The plane first: every pixel starts as the theme's canvas, and only the
    // dots put accent on top of it.
    let mut pixels = vec![0; w * h * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        *pixel = bg;
    }

    for row in 0..rows {
        for col in 0..cols {
            let center_x = (col as f32 + 0.5) * PITCH;
            let center_y = (row as f32 + 0.5) * PITCH;
            let value = sample(center_x, center_y, phase, scale, mark);
            let level = dither(value, BAYER[row % 8][col % 8]);
            let radius = PITCH * RADII[level];
            if radius <= 0. {
                continue;
            }
            // The accent's step for this level. Denser cells (the glyphs) get a
            // larger dot, never a different colour.
            let alpha = LEVEL_INK[level];
            // The dot's own pixels, with a one-pixel feather so a half-covered
            // edge pixel fades instead of stair-stepping the screen.
            let x0 = ((center_x - radius - 1.).floor().max(0.)) as usize;
            let y0 = ((center_y - radius - 1.).floor().max(0.)) as usize;
            let x1 = ((center_x + radius + 1.).ceil() as usize).min(w);
            let y1 = ((center_y + radius + 1.).ceil() as usize).min(h);
            for y in y0..y1 {
                for x in x0..x1 {
                    let dx = x as f32 + 0.5 - center_x;
                    let dy = y as f32 + 0.5 - center_y;
                    let coverage = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0., 1.);
                    if coverage <= 0. {
                        continue;
                    }
                    let a = alpha * coverage;
                    let pixel = &mut pixels[(y * w + x) * 4..][..4];
                    for channel in 0..3 {
                        let plane = f32::from(bg[channel]);
                        let dot = f32::from(ink[channel]);
                        pixel[channel] = (plane * (1. - a) + dot * a).round() as u8;
                    }
                }
            }
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
    fn the_frame_steps_at_the_field_cadence() {
        assert_eq!(frame(0.), 0);
        assert_eq!(
            frame(1. / FPS - 0.001),
            0,
            "a beat inside a frame stays on it"
        );
        assert_eq!(frame(1. / FPS + 0.001), 1);
        assert_eq!(frame(-3.), 0, "a phase never runs backwards past the start");
    }

    /// The next frame is rastered while the current one shows, so the beat
    /// that reaches it paints once instead of painting the old field and then
    /// again when the raster lands.
    #[test]
    fn the_next_frame_is_rastered_once_the_current_one_shows() {
        let now = key(1410, 878, 3);
        assert_eq!(
            ahead(now, Some(now), None, false, true),
            Some(key(1410, 878, 4))
        );
        assert_eq!(
            ahead(now, Some(now), Some(key(1410, 878, 4)), false, true),
            None,
            "a frame already rastered ahead is not made twice"
        );
        assert_eq!(
            ahead(now, Some(now), Some(key(1133, 878, 4)), false, true),
            Some(key(1410, 878, 4)),
            "one made for other bounds is replaced"
        );
    }

    #[test]
    fn nothing_is_rastered_ahead_of_a_field_that_is_not_current_or_not_moving() {
        let now = key(1410, 878, 3);
        assert_eq!(
            ahead(now, Some(key(1133, 878, 3)), None, false, true),
            None,
            "bounds still settling: the current frame comes first"
        );
        assert_eq!(ahead(now, None, None, false, true), None);
        assert_eq!(
            ahead(now, Some(now), None, true, true),
            None,
            "one raster in flight at a time"
        );
        assert_eq!(
            ahead(now, Some(now), None, false, false),
            None,
            "a still field (Reduce Motion, a thumbnail) never advances"
        );
    }

    #[test]
    fn bayer_has_all_thresholds() {
        let mut thresholds = BAYER.into_iter().flatten().collect::<Vec<_>>();
        thresholds.sort();
        assert_eq!(thresholds, (0..64).collect::<Vec<_>>());
    }

    /// The page's one colour rule: the plane is the theme's own background and
    /// the accent never leaves the dots. The screen used to paint full-strength
    /// accent pixels, so the page read as an accent-tinted slab rather than as
    /// the palette's canvas carrying a texture.
    #[test]
    fn the_plane_is_the_background_and_the_accent_only_inks_the_dots() {
        let (bg, ink) = ([8, 8, 10, 255], [255, 0, 0, 255]);
        let image = raster(320, 240, 0., bg, ink);
        let top = LEVEL_INK[LEVELS - 1];
        let (mut plain, mut inked) = (0, 0);
        for pixel in image.pixels() {
            let pixel = pixel.0;
            let share = f32::from(pixel[0] - bg[0]) / f32::from(ink[0] - bg[0]);
            assert!(
                share <= top + 0.01,
                "the accent flooded the plane: {pixel:?} is {share} of full ink"
            );
            if pixel == bg {
                plain += 1;
            } else {
                inked += 1;
            }
        }
        let total = f32::from((320 * 240) as u16);
        assert!(
            plain as f32 / total > 0.9,
            "the page must stay the theme's background: {}% plain",
            100. * plain as f32 / total
        );
        assert!(inked > 0, "the dots must still be there");
    }

    /// The field drifts, so the cached raster must change with the phase — and
    /// the four dot levels must all appear, or the screen has collapsed into a
    /// single hard mask again.
    #[test]
    fn the_screen_drifts_and_uses_its_whole_ladder() {
        let (bg, ink) = ([8, 8, 10, 255], [255, 0, 0, 255]);
        let images = [
            raster(320, 240, 0., bg, ink),
            raster(320, 240, 30., bg, ink),
        ];
        let changed = images[0]
            .pixels()
            .zip(images[1].pixels())
            .filter(|(a, b)| a != b)
            .count();
        assert!(changed > 100, "the field barely moved: {changed} pixels");

        let share =
            |pixel: &image::Rgba<u8>| f32::from(pixel.0[0] - bg[0]) / f32::from(ink[0] - bg[0]);
        let brightest = images[0].pixels().map(share).fold(0., f32::max);
        assert!(
            brightest > LEVEL_INK[1] + 0.05,
            "the field never reaches its larger dots: {brightest}"
        );
        assert!(
            brightest <= LEVEL_INK[LEVELS - 1] + 0.01,
            "a dot is stronger than the ladder allows: {brightest}"
        );
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
                        div().size_full().child(layer(
                            Theme::for_id(crate::theme::ThemeId::Orbit),
                            0.,
                            false,
                        ))
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
    fn the_watermark_stands_out_and_keeps_its_counters() {
        let (bg, ink) = ([8, 8, 10, 255], [255, 0, 0, 255]);
        let rect = Mark::get()
            .rect(800, 600)
            .expect("a layer this size holds the mark");
        let placed = Mark::at(rect);
        let (left, top, w, h) = rect;
        let share = |pixel: [u8; 4]| f32::from(pixel[0] - bg[0]) / f32::from(ink[0] - bg[0]);
        let images = [
            raster(800, 600, 0., bg, ink),
            raster(800, 600, 30., bg, ink),
        ];

        let mut glyph = [0f32; 2];
        let mut glyph_cells = 0f32;
        let mut field = [0f32; 2];
        let mut field_cells = 0f32;
        let mut changed = 0;
        for y in 0..h {
            for x in 0..w {
                let coverage = placed.mask.get_pixel(x, y).0[0];
                let values = [
                    share(images[0].get_pixel(left + x, top + y).0),
                    share(images[1].get_pixel(left + x, top + y).0),
                ];
                changed += usize::from(values[0] != values[1] && coverage > 0);
                if coverage == 255 {
                    glyph[0] += values[0];
                    glyph[1] += values[1];
                    glyph_cells += 1.;
                } else if coverage == 0 {
                    field[0] += values[0];
                    field[1] += values[1];
                    field_cells += 1.;
                }
            }
        }
        assert!(glyph_cells > 0. && field_cells > 0., "both regions sampled");
        assert!(changed > 0, "the light must cross the glyphs");
        for index in 0..2 {
            let glyph_mean = glyph[index] / glyph_cells;
            let field_mean = field[index] / field_cells;
            assert!(
                glyph_mean > field_mean * 1.5,
                "the watermark must read above the grain: {glyph_mean} vs {field_mean}"
            );
            assert!(field_mean > 0., "the grain around the mark must carry ink");
        }

        // The O's counter is a hole in the mask. It keeps the page's own grain
        // instead of filling with the glyph's denser screen.
        let (cx, cy) = ((0.21 * w as f32) as u32, (0.45 * h as f32) as u32);
        assert_eq!(
            placed.mask.get_pixel(cx, cy).0[0],
            0,
            "the counter is not covered by the mask"
        );
        let mut counter = 0f32;
        let mut cells = 0f32;
        for y in cy - 4..cy + 4 {
            for x in cx - 4..cx + 4 {
                counter += share(images[0].get_pixel(left + x, top + y).0);
                cells += 1.;
            }
        }
        assert!(
            counter / cells < glyph[0] / glyph_cells * 0.5,
            "the counter must keep the page's grain: {}",
            counter / cells
        );
    }

    #[test]
    fn the_watermark_works_on_a_light_palette_too() {
        // Ink darker than the plane: the same screen, inverted.
        let (bg, ink) = ([250, 250, 248, 255], [0, 0, 0, 255]);
        let rect = Mark::get()
            .rect(800, 600)
            .expect("a layer this size holds the mark");
        let placed = Mark::at(rect);
        let (left, top, w, h) = rect;
        let image = raster(800, 600, 12., bg, ink);
        let mut glyph = 0f32;
        let mut glyph_cells = 0f32;
        let mut field = 0f32;
        let mut field_cells = 0f32;
        for y in 0..h {
            for x in 0..w {
                let coverage = placed.mask.get_pixel(x, y).0[0];
                let pixel = image.get_pixel(left + x, top + y).0;
                let share = f32::from(bg[0] - pixel[0]) / f32::from(bg[0] - ink[0]);
                if coverage == 255 {
                    glyph += share;
                    glyph_cells += 1.;
                } else if coverage == 0 {
                    field += share;
                    field_cells += 1.;
                }
            }
        }
        assert!(
            glyph / glyph_cells > field / field_cells * 1.5,
            "the watermark must read on a light palette too"
        );
    }

    #[test]
    fn fields_are_smooth_and_time_dependent() {
        let a = noise(1.3, 2.2, 3.4, 41);
        assert!((a - noise(1.301, 2.2, 3.4, 41)).abs() < 0.01);
        assert_ne!(a, noise(1.3, 2.2, 4.4, 41));
        assert_ne!(a, noise(1.3, 2.2, 3.4, 971));
    }
}
