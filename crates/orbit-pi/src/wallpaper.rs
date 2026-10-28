//! Custom wallpaper: a local image behind the workbench, with a translucent
//! surface system layered over it.
//!
//! The image is deliberately *not* part of any one page. A single background
//! layer is painted in one of two places — behind the main column only
//! (`Hero`), or behind the whole window including the sessions sidebar
//! (`FullWindow`) — and every panel/card above it takes its fill from
//! [`Theme::card_fill`](crate::theme::Theme::card_fill) /
//! [`Theme::panel_fill`](crate::theme::Theme::panel_fill). That keeps one image
//! layer in the tree instead of a copy per card.
//!
//! Image work (decode, artwork effect, downscale, blur, brightness sampling,
//! accent extraction) runs on the background executor; the UI thread only
//! moves the finished BGRA buffer into a [`RenderImage`]. The cache is keyed
//! on the file's `(path, mtime, len)`, the blur radius, the artwork effect,
//! and the active appearance, so changing opacity, dimming, card opacity, or
//! the accent toggle never re-runs the pass.
//!
//! Readability is part of the pipeline, not a fixed alpha: [`surfaces`]
//! composites representative wallpaper samples with the active palette and
//! raises the card/panel alpha until primary and secondary text clear their
//! contrast targets. In hero mode it also raises the dim overlay for text
//! that sits directly on the image.

use std::{
    borrow::Cow,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, RwLock},
};

use gpui::{
    div, img, prelude::*, Animation, AnimationExt, AnyElement, ElementId, Hsla, IntoElement,
    ObjectFit, RenderImage,
};
use image::{Frame, RgbaImage};
use serde_json::{json, Value};

use crate::theme::{SurfaceTreatment, Theme};
use crate::wallpaper_fx::{self, Effect};

// ── public tuning ──────────────────────────────────────────────────────────

/// Longest edge kept for the painted wallpaper. Large enough for a retina
/// desktop, small enough that a 6000px photo does not become a 96MB texture.
const MAX_EDGE: u32 = 2560;
/// Artwork effects run per pixel on the CPU, so they are rendered at a
/// slightly smaller edge than an untreated photo. The difference is invisible
/// behind translucent surfaces, and an ASCII/halftone pass stays responsive.
const EFFECT_MAX_EDGE: u32 = 1800;
/// The blurred variant is generated at this edge and scaled at paint time.
const BLUR_MAX_EDGE: u32 = 1280;
/// Contrast targets for text on the composite surface. Primary text is body
/// copy (AA normal); secondary text is the slightly quieter role used for
/// metadata and folder rows (AA large).
const PRIMARY_TARGET: f32 = 4.5;
const SECONDARY_TARGET: f32 = 3.0;
/// Bounds for the computed card/panel alpha. The lower bound keeps a card a
/// card; the upper bound lets the wallpaper disappear on a hostile image
/// rather than making the UI unreadable.
const MIN_CARD_ALPHA: f32 = 0.35;
const MAX_CARD_ALPHA: f32 = 1.0;
/// The dim overlay may reach almost-but-not-quite opaque. A full-white image
/// with dark text in a dark theme may need almost all of it.
const MAX_DIM: f32 = 0.92;

pub const IMAGE_OPACITY_RANGE: (f32, f32) = (0.0, 1.0);
pub const BLUR_RANGE: (f32, f32) = (0.0, 40.0);
pub const DIM_RANGE: (f32, f32) = (0.0, 0.9);
pub const CARD_OPACITY_RANGE: (f32, f32) = (0.35, 1.0);
/// Slider step for the blur radius. A 0.5px step keeps a scrub cheap while
/// still feeling continuous.
pub const BLUR_STEP: f32 = 0.5;

// ── settings ───────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WallpaperMode {
    #[default]
    Disabled,
    /// Behind the main column, below the shared top bar. The sidebar stays
    /// opaque. This is the equivalent of the old page backdrop surface.
    Hero,
    /// Behind the whole window, including the sessions sidebar.
    FullWindow,
}

impl WallpaperMode {
    pub const ALL: [Self; 3] = [Self::Disabled, Self::Hero, Self::FullWindow];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Hero => "hero",
            Self::FullWindow => "full",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "disabled" | "off" | "none" => Some(Self::Disabled),
            "hero" | "hero-only" | "main" => Some(Self::Hero),
            "full" | "full-window" | "window" => Some(Self::FullWindow),
            _ => None,
        }
    }
}

/// Persisted wallpaper preferences. Every numeric value is clamped on read so
/// a hand-edited file cannot leave a slider outside its range.
#[derive(Debug, Clone, PartialEq)]
pub struct WallpaperSettings {
    pub mode: WallpaperMode,
    /// Artwork treatment applied before the picture is cached.
    pub effect: Effect,
    /// Absolute path to the app-managed copy (or a user path that was already
    /// inside the managed directory).
    pub path: Option<PathBuf>,
    /// The source file's name, shown in Settings. The managed copy keeps a
    /// stable name so replacing an image cannot leave a stale sibling.
    pub label: Option<String>,
    /// Image layer opacity, 0..=1.
    pub image_opacity: f32,
    /// Gaussian blur radius, 0..=40.
    pub blur: f32,
    /// Theme-coloured overlay over the image, 0..=0.9.
    pub dim: f32,
    /// Requested alpha for cards and panels, 0.35..=1. The contrast pass may
    /// raise this.
    pub card_opacity: f32,
    /// Derive the theme accent from the wallpaper.
    pub derive_accent: bool,
}

impl Default for WallpaperSettings {
    fn default() -> Self {
        Self {
            mode: WallpaperMode::Disabled,
            effect: Effect::None,
            path: None,
            label: None,
            image_opacity: 0.8,
            blur: 0.0,
            dim: 0.35,
            card_opacity: 0.82,
            derive_accent: true,
        }
    }
}

impl WallpaperSettings {
    /// The wallpaper actually paints. A missing file still leaves the
    /// preference in place (the disk may come back), but it cannot make
    /// surfaces translucent without an image beneath them.
    pub fn enabled(&self) -> bool {
        self.mode != WallpaperMode::Disabled && self.path.is_some()
    }

    pub fn display_label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    fn parse(raw: &str) -> Self {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return Self::default();
        };
        let mut settings = Self::default();
        if let Some(mode) = value.get("mode").and_then(Value::as_str) {
            if let Some(mode) = WallpaperMode::parse(mode) {
                settings.mode = mode;
            }
        }
        if let Some(effect) = value.get("effect").and_then(Value::as_str) {
            if let Some(effect) = Effect::parse(effect) {
                settings.effect = effect;
            }
        }
        settings.path = value
            .get("path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        settings.label = value
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .map(str::to_string);
        settings.image_opacity = clamp_json(
            value.get("image_opacity"),
            IMAGE_OPACITY_RANGE,
            settings.image_opacity,
        );
        settings.blur = clamp_json(value.get("blur"), BLUR_RANGE, settings.blur);
        settings.dim = clamp_json(value.get("dim"), DIM_RANGE, settings.dim);
        settings.card_opacity = clamp_json(
            value.get("card_opacity"),
            CARD_OPACITY_RANGE,
            settings.card_opacity,
        );
        if let Some(derive) = value.get("derive_accent").and_then(Value::as_bool) {
            settings.derive_accent = derive;
        }
        settings
    }

    fn persist_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            json!({
                "mode": self.mode.as_str(),
                "effect": self.effect.as_str(),
                "path": self.path.as_ref().map(|path| path.to_string_lossy()),
                "label": self.label,
                "image_opacity": self.image_opacity,
                "blur": self.blur,
                "dim": self.dim,
                "card_opacity": self.card_opacity,
                "derive_accent": self.derive_accent,
            })
            .to_string(),
        )
    }
}

fn clamp_json(value: Option<&Value>, range: (f32, f32), fallback: f32) -> f32 {
    value
        .and_then(Value::as_f64)
        .map(|value| value as f32)
        .filter(|value| value.is_finite())
        .map(|value| clamp(value, range))
        .unwrap_or(fallback)
}

fn clamp(value: f32, (lo, hi): (f32, f32)) -> f32 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        lo
    }
}

fn settings_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("wallpaper.json")
}

fn managed_dir() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("wallpapers")
}

static SETTINGS: OnceLock<RwLock<WallpaperSettings>> = OnceLock::new();

fn settings_store() -> &'static RwLock<WallpaperSettings> {
    SETTINGS.get_or_init(|| RwLock::new(load_settings()))
}

#[cfg(not(test))]
fn load_settings() -> WallpaperSettings {
    std::fs::read_to_string(settings_path())
        .map(|raw| WallpaperSettings::parse(&raw))
        .unwrap_or_default()
}

/// Unit tests must not read (or be influenced by) the developer's real
/// wallpaper setting. Pure parsing tests construct values explicitly.
#[cfg(test)]
fn load_settings() -> WallpaperSettings {
    WallpaperSettings::default()
}

/// Load the persisted setting eagerly so the first theme build already knows
/// whether wallpaper surfaces are requested.
pub fn init() {
    let _ = settings_store();
}

/// Current settings. Cheap to clone; never touches the disk after the first
/// load.
pub fn settings() -> WallpaperSettings {
    settings_store()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone()
}

/// Persist and publish a setting change. Returns whether anything changed so
/// callers can skip a theme rebuild.
pub fn update(next: WallpaperSettings) -> io::Result<bool> {
    let next = WallpaperSettings {
        image_opacity: clamp(next.image_opacity, IMAGE_OPACITY_RANGE),
        blur: (next.blur / BLUR_STEP).round() * BLUR_STEP,
        dim: clamp(next.dim, DIM_RANGE),
        card_opacity: clamp(next.card_opacity, CARD_OPACITY_RANGE),
        ..next
    };
    let mut store = settings_store()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if *store == next {
        return Ok(false);
    }
    next.persist_to(&settings_path())?;
    *store = next;
    Ok(true)
}

/// Reset preferences and remove only the app-managed copies. A source file
/// outside `~/.orbit-pi/wallpapers` is never touched.
pub fn reset() -> io::Result<()> {
    let defaults = WallpaperSettings::default();
    defaults.persist_to(&settings_path())?;
    let mut store = settings_store()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *store = defaults;
    remove_managed_files(None);
    invalidate();
    Ok(())
}

// ── importing ──────────────────────────────────────────────────────────────

/// Copy a picked file into the managed directory and prepare it. The copy and
/// the decode both run on the caller's (background) thread.
pub fn import_and_prepare(
    source: &Path,
    current: &WallpaperSettings,
    light: bool,
) -> Result<(WallpaperSettings, Prepared), String> {
    let decoded =
        image::open(source).map_err(|error| tr!("wallpaper.could_not_read", error = error))?;
    let (path, label) = copy_into_managed(source)?;
    let prepared = prepare_image(decoded, current.blur, current.effect, light);
    let mut next = current.clone();
    next.path = Some(path);
    next.label = Some(label);
    // Picking an image is an explicit request to see it. A disabled mode would
    // otherwise make the new import look like nothing happened.
    if next.mode == WallpaperMode::Disabled {
        next.mode = WallpaperMode::Hero;
    }
    Ok((next, prepared))
}

fn copy_into_managed(source: &Path) -> Result<(PathBuf, String), String> {
    let label = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "wallpaper".to_string());
    let extension = source
        .extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .filter(|ext| {
            !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|ch| ch.is_ascii_alphanumeric())
        })
        .unwrap_or_else(|| "png".to_string());
    let destination = managed_dir().join(format!("wallpaper.{extension}"));

    // A file already inside the managed directory is adopted in place. This
    // keeps re-selecting the current wallpaper from deleting the source before
    // it is copied.
    let source_is_managed = source
        .parent()
        .map(|parent| parent == managed_dir())
        .unwrap_or(false);
    if source_is_managed {
        remove_managed_files(Some(source));
        return Ok((source.to_path_buf(), label));
    }

    std::fs::create_dir_all(managed_dir())
        .map_err(|error| tr!("wallpaper.could_not_save", error = error))?;
    remove_managed_files(Some(&destination));
    if let Err(error) = std::fs::copy(source, &destination) {
        return Err(tr!("wallpaper.could_not_save", error = error));
    }
    Ok((destination, label))
}

fn remove_managed_files(keep: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(managed_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if Some(path.as_path()) == keep {
            continue;
        }
        let is_wallpaper = path
            .file_name()
            .map(|name| name.to_string_lossy().starts_with("wallpaper."))
            .unwrap_or(false);
        if is_managed(&path) && is_wallpaper {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Only delete files inside the managed directory. `keep` may be a user path
/// adopted from that same directory.
fn is_managed(path: &Path) -> bool {
    path.parent().map(|parent| parent == managed_dir()) == Some(true)
}

// ── processing ─────────────────────────────────────────────────────────────

/// Representative wallpaper colours, in sRGB (0..=1). Kept small and
/// serialisable so contrast is a pure CPU calculation on the UI thread.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Brightness {
    pub samples: Vec<[f32; 3]>,
}

/// The decoded result of one wallpaper pass, before it becomes a GPUI texture.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub width: u32,
    pub height: u32,
    pub main_bgra: Vec<u8>,
    pub blurred: Option<(u32, u32, Vec<u8>)>,
    pub accent: Option<u32>,
    pub brightness: Brightness,
}

/// Decode, apply the artwork effect, downscale, blur, and sample an image.
/// Runs off the UI thread; `light` selects the paper colour for effects that
/// print (ASCII, halftone, scanlines).
pub fn prepare(path: &Path, blur: f32, effect: Effect, light: bool) -> Result<Prepared, String> {
    let decoded =
        image::open(path).map_err(|error| tr!("wallpaper.could_not_read", error = error))?;
    Ok(prepare_image(decoded, blur, effect, light))
}

fn prepare_image(decoded: image::DynamicImage, blur: f32, effect: Effect, light: bool) -> Prepared {
    let max_edge = if effect == Effect::None {
        MAX_EDGE
    } else {
        EFFECT_MAX_EDGE
    };
    let fitted = fit(decoded, max_edge);
    // The effect is applied before sampling: the contrast guard must measure
    // the posterized picture that is actually painted, not the source photo.
    let rgba = wallpaper_fx::apply(fitted.to_rgba8(), effect, light);
    let (width, height) = rgba.dimensions();
    let brightness = sample_brightness(&rgba);
    let accent = extract_accent(&rgba);
    let main_bgra = to_bgra(&rgba);

    let blurred = (blur > 0.01).then(|| {
        let small = fit_rgba(&rgba, BLUR_MAX_EDGE);
        let (width, height) = small.dimensions();
        let blurred = image::imageops::fast_blur(&small, blur);
        (width, height, to_bgra(&blurred))
    });

    Prepared {
        width,
        height,
        main_bgra,
        blurred,
        accent,
        brightness,
    }
}

fn fit(image: image::DynamicImage, max_edge: u32) -> image::DynamicImage {
    if image.width() > max_edge || image.height() > max_edge {
        image.resize(max_edge, max_edge, image::imageops::FilterType::Triangle)
    } else {
        image
    }
}

fn fit_rgba(image: &RgbaImage, max_edge: u32) -> Cow<'_, RgbaImage> {
    let (width, height) = image.dimensions();
    if width > max_edge || height > max_edge {
        let scale = (max_edge as f32 / width as f32).min(max_edge as f32 / height as f32);
        let width = ((width as f32 * scale).round() as u32).max(1);
        let height = ((height as f32 * scale).round() as u32).max(1);
        Cow::Owned(image::imageops::resize(
            image,
            width,
            height,
            image::imageops::FilterType::Triangle,
        ))
    } else {
        Cow::Borrowed(image)
    }
}

fn to_bgra(image: &RgbaImage) -> Vec<u8> {
    // GPUI's sprite atlas samples BGRA, not RGBA.
    let mut bytes = image.as_raw().clone();
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    bytes
}

/// An 8×8-ish sample grid. `thumbnail` preserves the photograph's overall
/// brightness distribution while keeping the set small enough to evaluate
/// every contrast candidate against.
fn sample_brightness(image: &RgbaImage) -> Brightness {
    let thumb = image::imageops::thumbnail(image, 8, 8);
    let mut samples = Vec::new();
    for pixel in thumb.pixels() {
        if pixel.0[3] < 16 {
            continue;
        }
        samples.push([
            pixel.0[0] as f32 / 255.,
            pixel.0[1] as f32 / 255.,
            pixel.0[2] as f32 / 255.,
        ]);
    }
    Brightness { samples }
}

/// Pick a readable, representative accent: the busiest saturated hue bucket,
/// ignoring near-black, near-white, and grey pixels. Returns an `0xRRGGBB`
/// value, or `None` for a monochrome image.
fn extract_accent(image: &RgbaImage) -> Option<u32> {
    let thumb = image::imageops::thumbnail(image, 32, 32);
    let mut buckets = [[0f32; 4]; 24];
    for pixel in thumb.pixels() {
        let [r, g, b, a] = pixel.0;
        if a < 128 {
            continue;
        }
        let (r, g, b) = (r as f32 / 255., g as f32 / 255., b as f32 / 255.);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let value = max;
        let saturation = (max - min) / max.max(0.001);
        if !(0.08..=0.96).contains(&value) || saturation < 0.18 {
            continue;
        }
        let hue = hue_degrees(r, g, b, max, min);
        let bucket = ((hue / 360. * buckets.len() as f32) as usize).min(buckets.len() - 1);
        let weight = saturation * value;
        buckets[bucket][0] += r * weight;
        buckets[bucket][1] += g * weight;
        buckets[bucket][2] += b * weight;
        buckets[bucket][3] += weight;
    }
    buckets
        .into_iter()
        .filter(|bucket| bucket[3] > 0.001)
        .max_by(|a, b| a[3].total_cmp(&b[3]))
        .map(|bucket| {
            let weight = bucket[3].max(0.001);
            rgb_u32(
                (bucket[0] / weight * 255.).round() as u8,
                (bucket[1] / weight * 255.).round() as u8,
                (bucket[2] / weight * 255.).round() as u8,
            )
        })
}

fn hue_degrees(r: f32, g: f32, b: f32, max: f32, min: f32) -> f32 {
    let delta = max - min;
    if delta <= 0.0001 {
        return 0.;
    }
    let hue = if (max - r).abs() < f32::EPSILON {
        ((g - b) / delta) % 6.
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / delta + 2.
    } else {
        (r - g) / delta + 4.
    };
    (hue * 60.).rem_euclid(360.)
}

fn rgb_u32(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

// ── cache ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    path: PathBuf,
    modified: u64,
    len: u64,
    /// `blur × 100`, so the key is `Eq` and a scrub across one step hashes
    /// exactly like the painted result.
    blur: u32,
    /// Artwork effect and the appearance it prints in. Both change pixels, so
    /// both invalidate the cache.
    effect: u8,
    light: bool,
}

/// A prepared load that the UI thread hands to the background executor and
/// back. The key is private so a stale result cannot be installed after the
/// settings changed mid-flight.
#[derive(Debug, Clone)]
pub struct WallpaperJob {
    key: CacheKey,
    path: PathBuf,
    blur: f32,
    effect: Effect,
    light: bool,
}

impl WallpaperJob {
    pub fn prepare(&self) -> Result<Prepared, String> {
        prepare(&self.path, self.blur, self.effect, self.light)
    }
}

#[derive(Clone)]
pub struct WallpaperAssets {
    pub main: Arc<RenderImage>,
    pub blurred: Option<Arc<RenderImage>>,
    pub accent: Option<u32>,
    pub brightness: Brightness,
    key_id: u64,
}

struct Cached {
    key: CacheKey,
    main: Option<Arc<RenderImage>>,
    blurred: Option<Arc<RenderImage>>,
    accent: Option<u32>,
    brightness: Brightness,
    error: Option<String>,
}

static CACHE: RwLock<Option<Cached>> = RwLock::new(None);
static LOADING: Mutex<Option<CacheKey>> = Mutex::new(None);

fn key_for(settings: &WallpaperSettings, light: bool) -> Option<CacheKey> {
    let path = settings.path.clone()?;
    let (modified, len) = std::fs::metadata(&path)
        .map(|metadata| {
            let modified = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|since| since.as_millis() as u64)
                .unwrap_or(0);
            (modified, metadata.len())
        })
        .unwrap_or((0, 0));
    Some(CacheKey {
        path,
        modified,
        len,
        blur: (settings.blur * 100.).round() as u32,
        effect: settings.effect as u8,
        light,
    })
}

/// Start a load when the current settings have no matching cache entry.
/// Returns `None` when the cache is current, a load is already in flight, or
/// no wallpaper path is configured.
pub fn begin(light: bool) -> Option<WallpaperJob> {
    let settings = settings();
    if !settings.enabled() {
        return None;
    }
    let key = key_for(&settings, light)?;
    {
        let cache = CACHE
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if cache.as_ref().map(|cached| cached.key == key) == Some(true) {
            return None;
        }
    }
    let mut loading = LOADING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if loading.as_ref() == Some(&key) {
        return None;
    }
    *loading = Some(key.clone());
    Some(WallpaperJob {
        key,
        path: settings.path.clone()?,
        blur: settings.blur,
        effect: settings.effect,
        light,
    })
}

/// Install a finished load on the UI thread. Stale results (the settings
/// changed while decoding) are discarded. Returns whether the cache was
/// updated.
pub fn finish(job: WallpaperJob, result: Result<Prepared, String>, light: bool) -> bool {
    let current = key_for(&settings(), light);
    let mut loading = LOADING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if current.as_ref() != Some(&job.key) {
        if loading.as_ref() == Some(&job.key) {
            *loading = None;
        }
        return false;
    }
    let cached = match result {
        Ok(prepared) => Cached {
            key: job.key.clone(),
            main: Some(Arc::new(RenderImage::new(vec![Frame::new(bgra_image(
                prepared.width,
                prepared.height,
                prepared.main_bgra,
            ))]))),
            blurred: prepared.blurred.map(|(width, height, bytes)| {
                Arc::new(RenderImage::new(vec![Frame::new(bgra_image(
                    width, height, bytes,
                ))]))
            }),
            accent: prepared.accent,
            brightness: prepared.brightness,
            error: None,
        },
        Err(error) => Cached {
            key: job.key.clone(),
            main: None,
            blurred: None,
            accent: None,
            brightness: Brightness::default(),
            error: Some(error),
        },
    };
    *CACHE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(cached);
    if loading.as_ref() == Some(&job.key) {
        *loading = None;
    }
    true
}

fn bgra_image(width: u32, height: u32, bytes: Vec<u8>) -> RgbaImage {
    RgbaImage::from_raw(width, height, bytes).unwrap_or_else(|| RgbaImage::new(1, 1))
}

/// The current processed wallpaper, if the cache belongs to the configured
/// path. This deliberately does not touch the filesystem: render calls it
/// every frame, while `begin` owns the (once-per-frame at worst) metadata
/// check. A blur change therefore keeps the previous image on screen until
/// the new pass is ready instead of flashing the opaque base.
pub fn assets() -> Option<WallpaperAssets> {
    let settings = settings();
    let path = settings.path.as_ref()?;
    let cache = CACHE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let cached = cache.as_ref().filter(|cached| &cached.key.path == path)?;
    let main = cached.main.clone()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cached.key.hash(&mut hasher);
    Some(WallpaperAssets {
        main,
        blurred: cached.blurred.clone(),
        accent: cached.accent,
        brightness: cached.brightness.clone(),
        key_id: hasher.finish(),
    })
}

/// The error from the most recent failed load, for Settings feedback.
pub fn last_error() -> Option<String> {
    let settings = settings();
    let path = settings.path.as_ref()?;
    let cache = CACHE
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    cache
        .as_ref()
        .filter(|cached| &cached.key.path == path)
        .and_then(|cached| cached.error.clone())
}

/// Install a prepared import directly. The settings must already have been
/// published, because [`finish`] validates the job against them.
pub fn install(
    settings: &WallpaperSettings,
    result: Result<Prepared, String>,
    light: bool,
) -> bool {
    let Some(key) = key_for(settings, light) else {
        return false;
    };
    let Some(path) = settings.path.clone() else {
        return false;
    };
    finish(
        WallpaperJob {
            key,
            path,
            blur: settings.blur,
            effect: settings.effect,
            light,
        },
        result,
        light,
    )
}

fn invalidate() {
    *CACHE
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    *LOADING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

// ── contrast + surfaces ────────────────────────────────────────────────────

fn rgb_of(color: Hsla) -> [f32; 3] {
    let rgba = gpui::Rgba::from(color);
    [rgba.r, rgba.g, rgba.b]
}

fn linear(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance for an sRGB colour.
pub fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}

/// WCAG contrast ratio between two sRGB colours.
pub fn contrast_ratio(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn blend(from: [f32; 3], to: [f32; 3], alpha: f32) -> [f32; 3] {
    let alpha = alpha.clamp(0., 1.);
    [
        from[0] + (to[0] - from[0]) * alpha,
        from[1] + (to[1] - from[1]) * alpha,
        from[2] + (to[2] - from[2]) * alpha,
    ]
}

/// The wallpaper after image opacity and the theme-coloured dim overlay.
fn effective_backdrop(
    sample: [f32; 3],
    theme_bg: [f32; 3],
    image_opacity: f32,
    dim: f32,
) -> [f32; 3] {
    let image = blend(theme_bg, sample, image_opacity);
    blend(image, theme_bg, dim)
}

fn surface_over(backdrop: [f32; 3], surface: [f32; 3], alpha: f32) -> [f32; 3] {
    blend(backdrop, surface, alpha)
}

/// The first alpha at which every sample clears both text targets on `base`.
fn minimum_alpha(theme: Theme, base: Hsla, brightness: &Brightness) -> f32 {
    if brightness.samples.is_empty() {
        return MAX_CARD_ALPHA;
    }
    let theme_bg = rgb_of(theme.background);
    let surface = rgb_of(base);
    let (primary, secondary) = (rgb_of(theme.text_primary), rgb_of(theme.text_secondary));
    let at = |alpha: f32| {
        brightness.samples.iter().all(|sample| {
            let backdrop = effective_backdrop(
                *sample,
                theme_bg,
                theme.wallpaper.image_opacity.max(0.),
                theme.wallpaper.dim,
            );
            let composite = surface_over(backdrop, surface, alpha);
            contrast_ratio(primary, composite) >= PRIMARY_TARGET
                && contrast_ratio(secondary, composite) >= SECONDARY_TARGET
        })
    };
    if at(MIN_CARD_ALPHA) {
        return MIN_CARD_ALPHA;
    }
    if !at(MAX_CARD_ALPHA) {
        return MAX_CARD_ALPHA;
    }
    // Monotonic enough for a binary search: as alpha rises, the composite
    // tends toward the palette surface, which already passed at 1.0.
    let (mut lo, mut hi) = (MIN_CARD_ALPHA, MAX_CARD_ALPHA);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if at(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// The dim needed for text sitting directly on the image (hero mode).
fn minimum_dim(theme: Theme, brightness: &Brightness) -> f32 {
    if brightness.samples.is_empty() {
        return 0.;
    }
    let theme_bg = rgb_of(theme.background);
    let (primary, secondary) = (rgb_of(theme.text_primary), rgb_of(theme.text_secondary));
    let at = |dim: f32| {
        brightness.samples.iter().all(|sample| {
            let backdrop =
                effective_backdrop(*sample, theme_bg, theme.wallpaper.image_opacity, dim);
            contrast_ratio(primary, backdrop) >= PRIMARY_TARGET
                && contrast_ratio(secondary, backdrop) >= SECONDARY_TARGET
        })
    };
    if at(0.) {
        return 0.;
    }
    if !at(MAX_DIM) {
        return MAX_DIM;
    }
    let (mut lo, mut hi) = (0., MAX_DIM);
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if at(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

/// Compute the effective surface treatment for a theme. Pure (given the
/// settings and prepared samples) so it can be tested without a window.
pub fn surfaces(
    theme: Theme,
    settings: &WallpaperSettings,
    brightness: Option<&Brightness>,
) -> SurfaceTreatment {
    if !settings.enabled() || brightness.is_none() {
        return SurfaceTreatment::OPAQUE;
    }
    let brightness = brightness.expect("checked above");
    let full_window = settings.mode == WallpaperMode::FullWindow;

    // The initial treatment is used by `minimum_alpha` / `minimum_dim`; the
    // computed fields are then written back below.
    let mut treatment = SurfaceTreatment {
        active: true,
        full_window,
        image_opacity: clamp(settings.image_opacity, IMAGE_OPACITY_RANGE),
        dim: clamp(settings.dim, DIM_RANGE),
        card_alpha: clamp(settings.card_opacity, CARD_OPACITY_RANGE),
        panel_alpha: 1.0,
    };
    if !full_window {
        // Hero-only: headings and folder metadata sit on the image itself.
        // Raise the overlay until they clear the targets.
        treatment.dim = treatment
            .dim
            .max(minimum_dim(theme.with_wallpaper(treatment), brightness));
    }

    let decorated = theme.with_wallpaper(treatment);
    let mut alpha = treatment.card_alpha;
    for base in [
        theme.surface_elevated,
        theme.surface_secondary,
        theme.surface,
        theme.background,
    ] {
        alpha = alpha.max(minimum_alpha(decorated, base, brightness));
    }
    treatment.card_alpha = clamp(alpha, (MIN_CARD_ALPHA, MAX_CARD_ALPHA));
    treatment.panel_alpha = if full_window {
        treatment.card_alpha
    } else {
        1.0
    };
    treatment
}

/// Apply the wallpaper-derived accent and surface treatment to a base theme.
pub fn decorate(theme: Theme) -> Theme {
    let settings = settings();
    // Disabled means disabled: do not keep a tint or translucent surfaces
    // from a previously processed image.
    let assets = if settings.enabled() { assets() } else { None };
    let theme = match (
        settings.derive_accent,
        assets.as_ref().and_then(|a| a.accent),
    ) {
        (true, Some(accent)) => {
            crate::theme::appearance::with_accent(theme, gpui::rgb(accent).into())
        }
        _ => theme,
    };
    let treatment = surfaces(
        theme,
        &settings,
        assets.as_ref().map(|assets| &assets.brightness),
    );
    theme.with_wallpaper(treatment)
}

// ── painting ───────────────────────────────────────────────────────────────

/// The one wallpaper layer, absolutely filling its parent. `None` when no
/// image is ready, so callers can fall back to the normal page backdrop.
pub fn layer(theme: Theme, reduce_motion: bool) -> Option<AnyElement> {
    layer_inner(theme, reduce_motion, "wallpaper-layer")
}

fn layer_inner(theme: Theme, reduce_motion: bool, id: &'static str) -> Option<AnyElement> {
    let settings = settings();
    if !settings.enabled() {
        return None;
    }
    let assets = assets()?;
    let image = assets.blurred.clone().unwrap_or(assets.main.clone());
    let mut layer = div()
        .id(id)
        .debug_selector(move || id.to_string())
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .bottom_0()
        .overflow_hidden()
        .child(
            img(image)
                .size_full()
                .object_fit(ObjectFit::Cover)
                .opacity(theme.wallpaper.image_opacity),
        );
    if theme.wallpaper.dim > 0. {
        layer = layer.child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .bg(theme.background.opacity(theme.wallpaper.dim)),
        );
    }
    if reduce_motion {
        return Some(layer.into_any_element());
    }
    Some(
        layer
            .with_animation(
                ElementId::Name(format!("{id}-fade-{}", assets.key_id).into()),
                Animation::new(std::time::Duration::from_millis(240)),
                |layer, delta| layer.opacity(delta),
            )
            .into_any_element(),
    )
}

/// A preview is the same layer at rest, without the crossfade. It carries a
/// distinct element id so settings can show it while the window layer is also
/// mounted.
pub fn preview(theme: Theme) -> Option<AnyElement> {
    layer_inner(theme, true, "wallpaper-preview-layer")
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    fn temp_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("orbit-wallpaper-{tag}-{nanos}"))
    }

    fn solid(width: u32, height: u32, color: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(width, height, image::Rgba(color))
    }

    #[test]
    fn settings_clamp_out_of_range_values() {
        let parsed = WallpaperSettings::parse(
            r#"{
                "mode": "full",
                "path": "/tmp/photo.png",
                "image_opacity": 4.0,
                "blur": -2.0,
                "dim": 5.0,
                "card_opacity": 0.0,
                "derive_accent": false
            }"#,
        );
        assert_eq!(parsed.mode, WallpaperMode::FullWindow);
        assert_eq!(parsed.label, None);
        assert_eq!(parsed.image_opacity, 1.0);
        assert_eq!(parsed.blur, 0.0);
        assert_eq!(parsed.dim, DIM_RANGE.1);
        assert_eq!(parsed.card_opacity, CARD_OPACITY_RANGE.0);
        assert!(!parsed.derive_accent);
    }

    #[test]
    fn settings_round_trip_on_disk() {
        let dir = temp_dir("settings");
        let path = dir.join("wallpaper.json");
        let settings = WallpaperSettings {
            mode: WallpaperMode::Hero,
            effect: Effect::Halftone,
            path: Some(PathBuf::from("/tmp/a.png")),
            label: Some("a.png".into()),
            image_opacity: 0.42,
            blur: 3.5,
            dim: 0.2,
            card_opacity: 0.7,
            derive_accent: false,
        };
        settings.persist_to(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(WallpaperSettings::parse(&raw), settings);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_json_and_values_fall_back_independently() {
        assert_eq!(
            WallpaperSettings::parse("not json"),
            WallpaperSettings::default()
        );
        let parsed = WallpaperSettings::parse(
            r#"{"mode":"nonsense","image_opacity":"bad","blur":null,"dim":0.2}"#,
        );
        assert_eq!(parsed.mode, WallpaperMode::Disabled);
        assert_eq!(
            parsed.image_opacity,
            WallpaperSettings::default().image_opacity
        );
        assert_eq!(parsed.blur, WallpaperSettings::default().blur);
        assert_eq!(parsed.dim, 0.2);
    }

    #[test]
    fn mode_keys_round_trip() {
        for mode in WallpaperMode::ALL {
            assert_eq!(WallpaperMode::parse(mode.as_str()), Some(mode));
        }
    }

    #[test]
    fn prepare_downscales_large_images_and_reports_accent() {
        let dir = temp_dir("prepare");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("wide.png");
        solid(3000, 10, [220, 40, 30, 255]).save(&source).unwrap();
        let prepared = prepare(&source, 0., Effect::None, false).unwrap();
        assert!(prepared.width <= MAX_EDGE);
        assert!(prepared.height <= MAX_EDGE);
        let accent = prepared.accent.expect("red is a saturated accent");
        let red = (accent >> 16) & 0xFF;
        let green = (accent >> 8) & 0xFF;
        assert!(red > green, "red should dominate: {accent:#08x}");
        assert!(!prepared.brightness.samples.is_empty());
        assert!(prepared.blurred.is_none(), "blur 0 skips the variant");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepare_builds_a_blurred_variant() {
        let dir = temp_dir("blur");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("photo.png");
        solid(64, 64, [120, 80, 40, 255]).save(&source).unwrap();
        let prepared = prepare(&source, 6., Effect::None, false).unwrap();
        let (width, height, bytes) = prepared.blurred.expect("blur is requested");
        assert!(width <= BLUR_MAX_EDGE && height <= BLUR_MAX_EDGE);
        assert_eq!(bytes.len(), (width * height * 4) as usize);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn prepare_reports_unreadable_images() {
        let dir = temp_dir("broken");
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("broken.png");
        std::fs::write(&source, b"not an image").unwrap();
        assert!(prepare(&source, 0., Effect::None, false).is_err());
        assert!(prepare(&dir.join("missing.png"), 0., Effect::None, false).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn contrast_helpers_match_wcag_endpoints() {
        assert!((contrast_ratio([1., 1., 1.], [0., 0., 0.]) - 21.).abs() < 0.01);
        assert!((contrast_ratio([0.5, 0.5, 0.5], [0.5, 0.5, 0.5]) - 1.).abs() < 0.01);
        assert!((luminance([1., 1., 1.]) - 1.).abs() < 0.001);
    }

    #[test]
    fn minimum_alpha_rises_for_a_hostile_wallpaper() {
        let theme = Theme::for_id(ThemeId::OrbitAzure);
        let calm = Brightness {
            samples: vec![[0.02, 0.02, 0.02]],
        };
        let hostile = Brightness {
            samples: vec![[0.95, 0.95, 0.95]],
        };
        let calm_alpha = minimum_alpha(
            theme.with_wallpaper(SurfaceTreatment::OPAQUE),
            theme.surface_elevated,
            &calm,
        );
        let hostile_alpha = minimum_alpha(
            theme.with_wallpaper(SurfaceTreatment::OPAQUE),
            theme.surface_elevated,
            &hostile,
        );
        assert!(hostile_alpha >= calm_alpha);
        assert!(hostile_alpha > MIN_CARD_ALPHA);
    }

    #[test]
    fn hero_mode_raises_dim_for_direct_text() {
        let theme = Theme::for_id(ThemeId::OrbitAzure);
        let brightness = Brightness {
            samples: vec![[0.92, 0.92, 0.92]],
        };
        let settings = WallpaperSettings {
            mode: WallpaperMode::Hero,
            path: Some(PathBuf::from("/tmp/any")),
            ..WallpaperSettings::default()
        };
        let treatment = surfaces(theme, &settings, Some(&brightness));
        assert!(treatment.active);
        assert!(treatment.dim > settings.dim);
        assert_eq!(treatment.panel_alpha, 1.0);
        assert!(treatment.card_alpha >= settings.card_opacity);
    }

    #[test]
    fn disabled_wallpaper_keeps_opaque_surfaces() {
        let theme = Theme::for_id(ThemeId::OrbitAzure);
        let treatment = surfaces(theme, &WallpaperSettings::default(), None);
        assert!(!treatment.active);
        assert_eq!(treatment.card_alpha, 1.0);
        assert_eq!(treatment.panel_alpha, 1.0);
        assert_eq!(
            theme.card_fill(theme.surface_elevated),
            theme.surface_elevated
        );
        assert_eq!(theme.panel_fill(theme.background), theme.background);
    }

    #[test]
    fn full_window_mode_makes_panels_translucent() {
        let theme = Theme::for_id(ThemeId::OrbitAzure);
        let brightness = Brightness {
            samples: vec![[0.2, 0.2, 0.2], [0.7, 0.7, 0.7]],
        };
        let settings = WallpaperSettings {
            mode: WallpaperMode::FullWindow,
            path: Some(PathBuf::from("/tmp/any")),
            ..WallpaperSettings::default()
        };
        let treatment = surfaces(theme, &settings, Some(&brightness));
        assert!(treatment.active);
        assert!(treatment.full_window);
        assert_eq!(treatment.panel_alpha, treatment.card_alpha);
        let decorated = theme.with_wallpaper(treatment);
        assert!(decorated.panel_fill(theme.background).a < 1.0);
        assert!(decorated.card_fill(theme.surface_elevated).a < 1.0);
    }

    #[test]
    fn every_palette_keeps_text_readable_over_a_hostile_wallpaper() {
        let brightness = Brightness {
            samples: vec![[0.98, 0.98, 0.98], [0.72, 0.72, 0.72], [0.04, 0.04, 0.04]],
        };
        let settings = WallpaperSettings {
            mode: WallpaperMode::FullWindow,
            path: Some(PathBuf::from("/tmp/any")),
            ..WallpaperSettings::default()
        };
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let treatment = surfaces(theme, &settings, Some(&brightness));
            let theme_bg = rgb_of(theme.background);
            for (base, text, target) in [
                (theme.surface_elevated, theme.text_primary, PRIMARY_TARGET),
                (theme.surface_secondary, theme.text_primary, PRIMARY_TARGET),
                (theme.background, theme.text_secondary, SECONDARY_TARGET),
            ] {
                for sample in &brightness.samples {
                    let backdrop = effective_backdrop(
                        *sample,
                        theme_bg,
                        treatment.image_opacity,
                        treatment.dim,
                    );
                    let composite = surface_over(backdrop, rgb_of(base), treatment.card_alpha);
                    assert!(
                        contrast_ratio(rgb_of(text), composite) >= target,
                        "{id:?} {base:?} contrast {} below {target}",
                        contrast_ratio(rgb_of(text), composite)
                    );
                }
            }
        }
    }

    #[test]
    fn cache_key_tracks_blur_and_file_identity() {
        let dir = temp_dir("key");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("photo.png");
        solid(8, 8, [10, 20, 30, 255]).save(&path).unwrap();
        let mut settings = WallpaperSettings {
            mode: WallpaperMode::Hero,
            path: Some(path.clone()),
            ..WallpaperSettings::default()
        };
        let first = key_for(&settings, false).unwrap();
        settings.blur = 2.;
        let second = key_for(&settings, false).unwrap();
        assert_ne!(first, second, "blur is part of the cache identity");
        settings.blur = 0.;
        let third = key_for(&settings, false).unwrap();
        assert_eq!(first, third, "returning to a previous blur reuses its key");
        settings.effect = Effect::Ascii;
        let ascii = key_for(&settings, false).unwrap();
        assert_ne!(first, ascii, "the artwork effect is part of the identity");
        settings.effect = Effect::None;
        let light = key_for(&settings, true).unwrap();
        assert_ne!(first, light, "the appearance is part of the identity");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
