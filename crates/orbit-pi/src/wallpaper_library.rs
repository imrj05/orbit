//! "Your backgrounds": pictures the user keeps in a folder on this machine,
//! listed in Settings → Appearance → Background beside the built-ins.
//!
//! The folder is `~/.orbit-pi/wallpapers/library/`. Orbit only *reads* it: the
//! files are never bundled, uploaded or rewritten, so an image licensed for
//! use on the user's own device stays on that device. Picking one imports it
//! through the ordinary wallpaper path (the same copy a picked file gets), so
//! placement, blur, the artwork effects and accent-from-image all apply.
//!
//! Listing is two passes, both off the UI thread. [`list`] reads the folder
//! and pairs each picture with a thumbnail that is already cached, so the row
//! appears at once; [`make_thumbnails`] then decodes the pictures still
//! missing one (a photo can be 8000 px wide) into small JPEGs under
//! `~/.orbit-pi/wallpapers/library-thumbs/`. A thumbnail's name is a hash of
//! the picture's name, size and modification time, so editing or replacing a
//! file makes a fresh one, and thumbnails of removed files are pruned.
//!
//! The folder sits inside the managed wallpaper folder but is never touched by
//! its cleanup: `wallpaper::remove_managed_files` only deletes `wallpaper.*`
//! files lying directly in `wallpapers/`, not anything in a child folder.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, SystemTime};

use notify_debouncer_mini::{notify::RecommendedWatcher, Debouncer};

/// The extensions the wallpaper importer decodes (the same list the "Choose
/// image" dialog filters on).
const EXTENSIONS: [&str; 8] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff", "tif"];

/// The thumbnail's size: the built-in strip's 240 px width at its 1.6 ratio.
const THUMB_W: u32 = 240;
const THUMB_H: u32 = 150;

/// The folder the user drops pictures into.
pub fn dir() -> PathBuf {
    crate::wallpaper::managed_dir().join("library")
}

/// Where the small previews are cached.
pub fn thumbs_dir() -> PathBuf {
    crate::wallpaper::managed_dir().join("library-thumbs")
}

/// One picture in the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryImage {
    /// The picture itself, inside [`dir`].
    pub path: PathBuf,
    /// Its file name: the tooltip, and the wallpaper label once picked.
    pub name: String,
    /// Where its thumbnail lives (or will, once made).
    pub thumb: PathBuf,
    /// Whether `thumb` exists yet. Until it does the tile paints a plain face.
    pub thumb_ready: bool,
}

/// Whether `path` names a picture the importer can decode.
pub fn is_image(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|ext| EXTENSIONS.contains(&ext.as_str()))
}

/// The cached thumbnail's file for a picture of this name, size and time.
fn thumb_name(name: &str, len: u64, modified: Option<SystemTime>) -> String {
    // `DefaultHasher::new()` uses fixed keys, so a name is stable across
    // launches of the same build; a toolchain change at worst regenerates.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    len.hash(&mut hasher);
    modified
        .and_then(|time| time.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|since| since.as_nanos())
        .hash(&mut hasher);
    format!("{:016x}.jpg", hasher.finish())
}

/// Read `dir` (not recursively): every picture in it, sorted by name without
/// regard to case, each paired with its thumbnail path in `thumbs`. Hidden
/// files and anything that is not a regular file are skipped. A missing
/// folder is an empty library, not an error.
pub fn list(dir: &Path, thumbs: &Path) -> Vec<LibraryImage> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut images: Vec<LibraryImage> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !is_image(&path) {
                return None;
            }
            let meta = entry.metadata().ok().filter(|meta| meta.is_file())?;
            let thumb = thumbs.join(thumb_name(&name, meta.len(), meta.modified().ok()));
            let thumb_ready = thumb.is_file();
            Some(LibraryImage {
                path,
                name,
                thumb,
                thumb_ready,
            })
        })
        .collect();
    images.sort_by_key(|image| image.name.to_lowercase());
    images
}

/// Make every missing thumbnail and prune the ones no picture uses any more.
/// A picture that does not decode keeps `thumb_ready == false` and is still
/// listed: picking it reports the importer's own error.
pub fn make_thumbnails(images: &mut [LibraryImage], thumbs: &Path) {
    let _ = std::fs::create_dir_all(thumbs);
    for image in images.iter_mut().filter(|image| !image.thumb_ready) {
        let Ok(decoded) = image::open(&image.path) else {
            continue;
        };
        let small =
            decoded.resize_to_fill(THUMB_W, THUMB_H, image::imageops::FilterType::Triangle);
        // JPEG has no alpha: flatten onto RGB before encoding.
        image.thumb_ready = small.to_rgb8().save(&image.thumb).is_ok();
    }
    prune_thumbnails(images, thumbs);
}

/// Delete cached thumbnails that belong to no listed picture. Only `.jpg`
/// files directly inside `thumbs` are touched.
fn prune_thumbnails(images: &[LibraryImage], thumbs: &Path) {
    let keep: HashSet<&Path> = images.iter().map(|image| image.thumb.as_path()).collect();
    let Ok(entries) = std::fs::read_dir(thumbs) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let cached = path.extension().is_some_and(|ext| ext == "jpg");
        if cached && !keep.contains(path.as_path()) {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Watches the folder so a picture dropped in (or removed) shows up while
/// Settings is open. The heartbeat polls [`Self::take_dirty`].
pub struct LibraryWatcher {
    _debouncer: Debouncer<RecommendedWatcher>,
    dirty: Receiver<()>,
}

impl LibraryWatcher {
    /// Create the folder if it is missing (so there is something to watch and
    /// to open), then watch it. `None` when the platform watcher cannot start;
    /// the row still lists the folder as it was when Settings opened.
    pub fn start() -> Option<Self> {
        let dir = dir();
        std::fs::create_dir_all(&dir).ok()?;
        let (debouncer, dirty) = crate::watch::debounced_watch(
            &dir,
            Duration::from_millis(400),
            // Any picture coming or going, plus the folder itself.
            |path| is_image(path) || path.extension().is_none(),
        )?;
        Some(Self {
            _debouncer: debouncer,
            dirty,
        })
    }

    pub fn take_dirty(&self) -> bool {
        crate::watch::drain(&self.dirty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "orbit-library-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_png(path: &Path, w: u32, h: u32) {
        image::RgbImage::from_pixel(w, h, image::Rgb([40, 90, 160]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn lists_only_pictures_sorted_by_name_ignoring_case() {
        let root = scratch("list");
        let (dir, thumbs) = (root.join("library"), root.join("thumbs"));
        std::fs::create_dir_all(&dir).unwrap();
        write_png(&dir.join("b-sunset.PNG"), 8, 8);
        write_png(&dir.join("A lake (1).jpg"), 8, 8);
        std::fs::write(dir.join("notes.txt"), "not a picture").unwrap();
        std::fs::write(dir.join(".hidden.png"), "skip").unwrap();
        std::fs::create_dir_all(dir.join("folder.png")).unwrap();

        let images = list(&dir, &thumbs);
        let names: Vec<_> = images.iter().map(|image| image.name.as_str()).collect();
        assert_eq!(names, ["A lake (1).jpg", "b-sunset.PNG"]);
        assert!(images.iter().all(|image| !image.thumb_ready));
        assert!(images.iter().all(|image| image.thumb.starts_with(&thumbs)));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_folder_is_an_empty_library() {
        let root = scratch("missing");
        assert!(list(&root.join("nope"), &root.join("thumbs")).is_empty());
        let _ = std::fs::remove_dir_all(root);
    }

    /// Thumbnails are made once at the strip's size, reused by the next
    /// listing, and pruned when their picture goes away.
    #[test]
    fn thumbnails_are_made_reused_and_pruned() {
        let root = scratch("thumbs");
        let (dir, thumbs) = (root.join("library"), root.join("thumbs"));
        std::fs::create_dir_all(&dir).unwrap();
        write_png(&dir.join("wide.png"), 900, 300);
        write_png(&dir.join("tall.png"), 200, 700);
        std::fs::write(dir.join("broken.jpg"), "not really a jpeg").unwrap();

        let mut images = list(&dir, &thumbs);
        make_thumbnails(&mut images, &thumbs);
        for image in &images {
            if image.name == "broken.jpg" {
                assert!(!image.thumb_ready, "an undecodable file keeps a plain tile");
                continue;
            }
            assert!(image.thumb_ready, "{}", image.name);
            let thumb = image::open(&image.thumb).unwrap();
            assert_eq!((thumb.width(), thumb.height()), (THUMB_W, THUMB_H));
        }
        // The next listing finds them cached.
        let again = list(&dir, &thumbs);
        assert!(again
            .iter()
            .filter(|image| image.name != "broken.jpg")
            .all(|image| image.thumb_ready));

        // Removing a picture prunes its thumbnail on the next pass.
        let tall_thumb = again.iter().find(|i| i.name == "tall.png").unwrap().thumb.clone();
        std::fs::remove_file(dir.join("tall.png")).unwrap();
        let mut after = list(&dir, &thumbs);
        make_thumbnails(&mut after, &thumbs);
        assert!(!tall_thumb.exists());
        assert!(after.iter().any(|image| image.name == "wide.png" && image.thumb.exists()));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_changed_file_gets_a_new_thumbnail_name() {
        let now = SystemTime::now();
        let base = thumb_name("lake.jpg", 10, Some(now));
        assert_eq!(base, thumb_name("lake.jpg", 10, Some(now)));
        assert_ne!(base, thumb_name("lake.jpg", 11, Some(now)));
        assert_ne!(base, thumb_name("lake.jpg", 10, Some(now + Duration::from_secs(1))));
        assert_ne!(base, thumb_name("river.jpg", 10, Some(now)));
    }

    #[test]
    fn the_library_never_overlaps_the_managed_wallpaper_files() {
        // `remove_managed_files` deletes `wallpaper.*` directly inside the
        // managed folder; the library is a child folder, never that folder.
        assert_eq!(dir().parent(), Some(crate::wallpaper::managed_dir().as_path()));
        assert_ne!(dir(), crate::wallpaper::managed_dir());
        assert_ne!(dir(), thumbs_dir());
    }
}
