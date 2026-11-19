//! Built-in backgrounds the user can pick in one click from Settings →
//! Appearance → Background: generated gradients, and a few bundled images
//! (a nature photo, anime-style illustrations, Japanese woodblock prints).
//!
//! Picking one writes it into the managed wallpaper folder
//! (`~/.orbit-pi/wallpapers/builtin-<id>.<ext>`) and imports that file through
//! the ordinary wallpaper path, so placement, blur, the artwork effects and
//! accent-from-image all apply to it exactly as to a picked photo. A gradient
//! is rendered on the spot; an image is copied out of the embedded assets
//! (`assets/wallpapers/`, with small `thumbs/` for the Settings strip). The
//! images are from Unsplash under the Unsplash License; the prints are public
//! domain. Credits are in `assets/wallpapers/CREDITS.md`.
//!
//! The generator reproduces GPUI's own `linear_gradient` (angle convention,
//! the short side stretched to the long one, sRGB mixing), so the thumbnail
//! Settings paints with `linear_gradient` is the same picture in miniature.
//! A deterministic sub-LSB dither keeps the large flat ramps from banding.

use std::path::{Path, PathBuf};

use image::{Rgb, RgbImage};

/// The generated image's size. Large enough for a full-window wallpaper on a
/// 5K display after the wallpaper pipeline's own downscale.
const WIDTH: u32 = 2560;
const HEIGHT: u32 = 1600;

/// The groups Settings lists the built-ins in, one row each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Gradients,
    Nature,
    Anime,
    JapaneseArt,
}

impl Category {
    pub const ALL: [Self; 4] = [Self::Gradients, Self::Nature, Self::Anime, Self::JapaneseArt];

    pub fn id(self) -> &'static str {
        match self {
            Self::Gradients => "gradients",
            Self::Nature => "nature",
            Self::Anime => "anime",
            Self::JapaneseArt => "japanese-art",
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Gradients => tr!("wallpaper.builtin_gradients"),
            Self::Nature => tr!("wallpaper.builtin_nature"),
            Self::Anime => tr!("wallpaper.builtin_anime"),
            Self::JapaneseArt => tr!("wallpaper.builtin_japanese_art"),
        }
        .to_string()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinBackground {
    Dusk,
    Aurora,
    Ember,
    Ocean,
    Forest,
    Mist,
    Sand,
    MountainLake,
    BeachSunset,
    NightWalk,
    PurpleDusk,
    GreatWave,
    TamaRiver,
    PlumMoon,
    NobotoBay,
}

/// What a built-in is made of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    /// Start colour, end colour (sRGB hex) and GPUI's angle in degrees.
    Gradient(u32, u32, f32),
    /// A bundled image under `assets/wallpapers/` (and its `thumbs/`).
    Image(&'static str),
}

impl BuiltinBackground {
    pub const ALL: [Self; 15] = [
        Self::Dusk,
        Self::Aurora,
        Self::Ember,
        Self::Ocean,
        Self::Forest,
        Self::Mist,
        Self::Sand,
        Self::MountainLake,
        Self::BeachSunset,
        Self::NightWalk,
        Self::PurpleDusk,
        Self::GreatWave,
        Self::TamaRiver,
        Self::PlumMoon,
        Self::NobotoBay,
    ];

    pub fn in_category(category: Category) -> impl Iterator<Item = Self> {
        Self::ALL
            .into_iter()
            .filter(move |background| background.category() == category)
    }

    pub fn category(self) -> Category {
        match self.source() {
            Source::Gradient(..) => Category::Gradients,
            Source::Image(_) => match self {
                Self::MountainLake => Category::Nature,
                Self::BeachSunset | Self::NightWalk | Self::PurpleDusk => Category::Anime,
                _ => Category::JapaneseArt,
            },
        }
    }

    pub fn source(self) -> Source {
        match self {
            Self::Dusk => Source::Gradient(0x1E1B4B, 0x9D174D, 135.),
            Self::Aurora => Source::Gradient(0x042F2E, 0x4C1D95, 160.),
            Self::Ember => Source::Gradient(0x3B0764, 0xEA580C, 135.),
            Self::Ocean => Source::Gradient(0x0F172A, 0x0E7490, 180.),
            Self::Forest => Source::Gradient(0x052E16, 0x4D7C0F, 140.),
            Self::Mist => Source::Gradient(0xE0E7FF, 0xFCE7F3, 135.),
            Self::Sand => Source::Gradient(0xFEF3C7, 0xE7E5E4, 120.),
            Self::MountainLake => Source::Image("nature-lake.jpg"),
            Self::BeachSunset => Source::Image("anime-sunset-beach.jpg"),
            Self::NightWalk => Source::Image("anime-night-walk.jpg"),
            Self::PurpleDusk => Source::Image("anime-purple-dusk.jpg"),
            Self::GreatWave => Source::Image("jp-great-wave.jpg"),
            Self::TamaRiver => Source::Image("jp-tama-river.jpg"),
            Self::PlumMoon => Source::Image("jp-plum-moon.jpg"),
            Self::NobotoBay => Source::Image("jp-noboto-bay.jpg"),
        }
    }

    /// Who made a bundled image (a proper name, not translated).
    pub fn credit(self) -> Option<&'static str> {
        match self {
            Self::MountainLake => Some("Adam Vradenburg"),
            Self::BeachSunset => Some("karem adem"),
            Self::NightWalk => Some("Ands Mahardika"),
            Self::PurpleDusk => Some("Akanksha Maurya"),
            Self::GreatWave | Self::TamaRiver | Self::NobotoBay => Some("Katsushika Hokusai"),
            Self::PlumMoon => Some("Utagawa Hiroshige"),
            _ => None,
        }
    }

    /// The tooltip: the name, and for an image who made it.
    pub fn tooltip(self) -> String {
        match self.credit() {
            Some(credit) => format!("{} · {credit}", self.label()),
            None => self.label(),
        }
    }

    /// The Settings thumbnail of a bundled image, as an asset path.
    pub fn thumbnail(self) -> Option<String> {
        match self.source() {
            Source::Image(file) => Some(format!("wallpapers/thumbs/{file}")),
            Source::Gradient(..) => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Dusk => "dusk",
            Self::Aurora => "aurora",
            Self::Ember => "ember",
            Self::Ocean => "ocean",
            Self::Forest => "forest",
            Self::Mist => "mist",
            Self::Sand => "sand",
            Self::MountainLake => "mountain-lake",
            Self::BeachSunset => "beach-sunset",
            Self::NightWalk => "night-walk",
            Self::PurpleDusk => "purple-dusk",
            Self::GreatWave => "great-wave",
            Self::TamaRiver => "tama-river",
            Self::PlumMoon => "plum-moon",
            Self::NobotoBay => "noboto-bay",
        }
    }

    /// The localized name shown in the tooltip and as the wallpaper's label.
    pub fn label(self) -> String {
        match self {
            Self::Dusk => tr!("wallpaper.builtin_dusk"),
            Self::Aurora => tr!("wallpaper.builtin_aurora"),
            Self::Ember => tr!("wallpaper.builtin_ember"),
            Self::Ocean => tr!("wallpaper.builtin_ocean"),
            Self::Forest => tr!("wallpaper.builtin_forest"),
            Self::Mist => tr!("wallpaper.builtin_mist"),
            Self::Sand => tr!("wallpaper.builtin_sand"),
            Self::MountainLake => tr!("wallpaper.builtin_mountain_lake"),
            Self::BeachSunset => tr!("wallpaper.builtin_beach_sunset"),
            Self::NightWalk => tr!("wallpaper.builtin_night_walk"),
            Self::PurpleDusk => tr!("wallpaper.builtin_purple_dusk"),
            Self::GreatWave => tr!("wallpaper.builtin_great_wave"),
            Self::TamaRiver => tr!("wallpaper.builtin_tama_river"),
            Self::PlumMoon => tr!("wallpaper.builtin_plum_moon"),
            Self::NobotoBay => tr!("wallpaper.builtin_noboto_bay"),
        }
        .to_string()
    }

    /// A gradient's thumbnail fill: GPUI's own `linear_gradient` with the
    /// colours and angle the generated image uses.
    pub fn gradient(self) -> Option<gpui::Background> {
        match self.source() {
            Source::Gradient(from, to, angle) => Some(gpui::linear_gradient(
                angle,
                gpui::linear_color_stop(gpui::rgb(from), 0.),
                gpui::linear_color_stop(gpui::rgb(to), 1.),
            )),
            Source::Image(_) => None,
        }
    }

    /// Where the written file lives.
    pub fn file_path(self) -> PathBuf {
        let extension = match self.source() {
            Source::Gradient(..) => "png",
            Source::Image(_) => "jpg",
        };
        crate::wallpaper::managed_dir().join(format!("builtin-{}.{extension}", self.id()))
    }

    /// The built-in background a wallpaper path is, if any.
    pub fn of_path(path: &Path) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|background| background.file_path() == path)
    }
}

/// Render a gradient at `width` × `height`.
pub fn render(from: u32, to: u32, angle: f32, width: u32, height: u32) -> RgbImage {
    let channels = |hex: u32| {
        [
            ((hex >> 16) & 0xFF) as f32,
            ((hex >> 8) & 0xFF) as f32,
            (hex & 0xFF) as f32,
        ]
    };
    let (from, to) = (channels(from), channels(to));
    let (w, h) = (width as f32, height as f32);
    // GPUI's shader: rotate -90° so 0 points up, then stretch the short side.
    let radians = (angle.rem_euclid(360.) - 90.).to_radians();
    let (mut dx, mut dy) = (radians.cos(), radians.sin());
    if w > h {
        dy *= h / w;
    } else {
        dx *= w / h;
    }
    let length = (dx * dx + dy * dy).sqrt();
    let along_x = dx.abs() > dy.abs();
    RgbImage::from_fn(width, height, |x, y| {
        let px = x as f32 + 0.5 - w / 2.;
        let py = y as f32 + 0.5 - h / 2.;
        let projected = (px * dx + py * dy) / length;
        let t = if along_x {
            (projected + w / 2.) / w
        } else {
            (projected + h / 2.) / h
        }
        .clamp(0., 1.);
        let noise = dither(x, y);
        Rgb(std::array::from_fn(|i| {
            (from[i] + (to[i] - from[i]) * t + noise).round().clamp(0., 255.) as u8
        }))
    })
}

/// A deterministic offset in [-0.5, 0.5): enough to break 8-bit banding on a
/// long ramp, too small to read as grain.
fn dither(x: u32, y: u32) -> f32 {
    let mut n = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    n ^= n >> 15;
    n = n.wrapping_mul(0x2C1B_3C6D);
    n ^= n >> 12;
    (n & 0xFFFF) as f32 / 65536. - 0.5
}

/// Write `background` to its file, creating the managed folder if needed.
/// Runs on a background thread.
pub fn write(background: BuiltinBackground) -> Result<PathBuf, String> {
    let path = background.file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| tr!("wallpaper.could_not_save", error = error).to_string())?;
    }
    let saved = match background.source() {
        Source::Gradient(from, to, angle) => render(from, to, angle, WIDTH, HEIGHT)
            .save(&path)
            .map_err(|error| error.to_string()),
        Source::Image(file) => match crate::assets::bytes(&format!("wallpapers/{file}")) {
            Some(bytes) => std::fs::write(&path, bytes).map_err(|error| error.to_string()),
            None => Err(format!("missing bundled image {file}")),
        },
    };
    saved.map_err(|error| tr!("wallpaper.could_not_save", error = error).to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient_of(background: BuiltinBackground) -> (u32, u32, f32) {
        match background.source() {
            Source::Gradient(from, to, angle) => (from, to, angle),
            Source::Image(_) => panic!("{background:?} is an image"),
        }
    }

    /// Every bundled image and its thumbnail are embedded and decode, so a
    /// built-in can never point at a file the binary does not carry.
    #[test]
    fn every_bundled_image_and_thumbnail_is_embedded() {
        for background in BuiltinBackground::ALL {
            let Source::Image(file) = background.source() else {
                continue;
            };
            for path in [format!("wallpapers/{file}"), background.thumbnail().unwrap()] {
                let bytes = crate::assets::bytes(&path)
                    .unwrap_or_else(|| panic!("{path} is not embedded"));
                image::load_from_memory(bytes)
                    .unwrap_or_else(|error| panic!("{path} does not decode: {error}"));
            }
            assert!(background.credit().is_some(), "{background:?} names its artist");
        }
    }

    /// `include_dir!` embeds the whole `assets/` tree, so an image left in
    /// `assets/wallpapers/` ships in every binary even when no built-in uses
    /// it. Only catalogued, credited images may live there; the user's own
    /// pictures belong in the library folder (`wallpaper_library`).
    #[test]
    fn the_bundled_folder_holds_only_catalogued_images() {
        let catalogued: Vec<&str> = BuiltinBackground::ALL
            .iter()
            .filter_map(|background| match background.source() {
                Source::Image(file) => Some(file),
                Source::Gradient(..) => None,
            })
            .collect();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/wallpapers");
        for folder in [root.clone(), root.join("thumbs")] {
            for entry in std::fs::read_dir(&folder).expect("wallpapers folder").flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if entry.path().is_dir() || name == "CREDITS.md" || name.starts_with('.') {
                    continue;
                }
                assert!(
                    catalogued.contains(&name.as_str()),
                    "{} is bundled but no built-in uses it; move it to the library folder",
                    entry.path().display()
                );
            }
        }
    }

    #[test]
    fn every_category_lists_something() {
        for category in Category::ALL {
            assert!(BuiltinBackground::in_category(category).next().is_some(), "{category:?}");
        }
    }

    fn rgb(hex: u32) -> [u8; 3] {
        [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
    }

    fn close(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= 3)
    }

    /// The image runs from the start colour to the end colour along GPUI's
    /// angle: at 180° (pointing down) the top row is the start and the bottom
    /// row the end, and a column is constant across its width.
    #[test]
    fn a_downward_gradient_runs_top_to_bottom() {
        let (from, to, angle) = gradient_of(BuiltinBackground::Ocean);
        let image = render(from, to, angle, 64, 40);
        assert!(close(image.get_pixel(10, 0).0, rgb(from)));
        assert!(close(image.get_pixel(10, 39).0, rgb(to)));
        assert!(close(image.get_pixel(0, 20).0, image.get_pixel(63, 20).0));
    }

    /// At 135° the start is the top-left corner and the end the bottom-right,
    /// as GPUI paints the thumbnail.
    #[test]
    fn a_diagonal_gradient_runs_corner_to_corner() {
        let (from, to, angle) = gradient_of(BuiltinBackground::Dusk);
        let image = render(from, to, angle, 80, 50);
        assert!(close(image.get_pixel(0, 0).0, rgb(from)));
        assert!(close(image.get_pixel(79, 49).0, rgb(to)));
    }

    #[test]
    fn every_background_has_its_own_file_and_round_trips() {
        let mut ids: Vec<_> = BuiltinBackground::ALL.iter().map(|b| b.id()).collect();
        ids.dedup();
        assert_eq!(ids.len(), BuiltinBackground::ALL.len());
        for background in BuiltinBackground::ALL {
            assert_eq!(
                BuiltinBackground::of_path(&background.file_path()),
                Some(background)
            );
        }
        assert_eq!(BuiltinBackground::of_path(Path::new("/tmp/photo.png")), None);
    }

    #[test]
    fn the_dither_stays_below_one_step() {
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1234, 987), (u32::MAX, 7)] {
            let n = dither(x, y);
            assert!((-0.5..0.5).contains(&n), "{n}");
        }
    }
}
