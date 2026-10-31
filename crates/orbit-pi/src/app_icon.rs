//! App icon — dock mark on macOS, PNG for in-app chrome.
//!
//! Source of truth: workspace `assets/icons/icon.png` (1024px, full-bleed).
//! The macOS assets inset that artwork to Apple's icon grid — 824px centered on
//! a 1024px canvas — so Dock/Finder render it at the same visual size as native
//! apps; `icon.icns` and the 512px copy in this crate are derived that way, and
//! the 512px copy is what the running binary embeds. `icon.ico` keeps the
//! full-bleed art, which is what Windows expects.
//!
//! macOS **debug** builds swap in the Alpha mark (`assets/icons/alpha-logo.png`,
//! inset the same way) so a `cargo run` build is unmistakable in the Dock.
//! Release builds — `cargo build --release`, `scripts/make-dmg.sh`, CI — never
//! take those branches, so the production icon and bundle id are untouched.

/// 512×512 PNG used by the dock (macOS) and Settings → About. The Alpha mark
/// in macOS debug builds, the production mark everywhere else.
#[cfg(all(debug_assertions, target_os = "macos"))]
pub const PNG: &[u8] = include_bytes!("../dev-assets/alpha-app-icon.png");
#[cfg(not(all(debug_assertions, target_os = "macos")))]
pub const PNG: &[u8] = include_bytes!("../assets/app-icon.png");

/// Asset path for [`gpui::img`] via [`crate::assets::Assets`]. Mirrors [`PNG`]:
/// Alpha in macOS debug builds, production everywhere else.
#[cfg(all(debug_assertions, target_os = "macos"))]
pub const ASSET: &str = "alpha-app-icon.png";
#[cfg(not(all(debug_assertions, target_os = "macos")))]
pub const ASSET: &str = "app-icon.png";

/// The production mark's monogram, for the new-task page's halftone backdrop.
///
/// Deliberately not [`PNG`]: the debug Alpha plate is the same monogram with an
/// "Alpha" pill appended, and a Dock affordance must not change the shape the
/// workbench draws. One byte array, one shape, in every build.
pub const MARK_PNG: &[u8] = include_bytes!("../assets/app-icon.png");

/// The "Orbit Pi" wordmark, shown as the sidebar brand header. Source of
/// truth: workspace `assets/icons/logo.png`; the copy in this crate is what
/// the running binary embeds. White ink, for dark sidebars.
pub const LOGO_ASSET: &str = "logo.png";

/// The wordmark for light sidebars — dark ink with a white halo, so it reads
/// on a light background where the white mark would vanish. Source of truth:
/// workspace `assets/icons/logo-dark.png`.
pub const LOGO_DARK_ASSET: &str = "logo-dark.png";

/// Set the macOS Dock icon for `cargo run` (no `.app` bundle).
///
/// Bundled builds pick the icon up from `icon.icns` / cargo-bundle metadata;
/// this still runs with [`PNG`]'s mark — the Alpha logo in debug builds, which
/// is what the ad-hoc dev bundle in `scripts/run-bundled.sh` shows.
pub fn set_dock_icon() {
    // Keep the PNG in the binary on every OS so About / tests share one embed.
    let _png = PNG;
    #[cfg(target_os = "macos")]
    {
        use objc2::{AnyThread, MainThreadMarker};
        use objc2_app_kit::{NSApplication, NSImage};
        use objc2_foundation::NSData;

        // `Some` on the main thread only — app startup always is, inside the
        // gpui run loop; off the main thread the icon simply is not set.
        if let Some(mtm) = MainThreadMarker::new() {
            let data = NSData::with_bytes(_png);
            if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
                // SAFETY: `image` is a live AppKit object, used on the main
                // thread that owns it, which is the only requirement AppKit
                // puts on the application's icon.
                unsafe {
                    NSApplication::sharedApplication(mtm)
                        .setApplicationIconImage(Some(&image));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_is_png() {
        assert_eq!(&PNG[..4], b"\x89PNG");
        assert!(PNG.len() > 1024);
    }

    #[cfg(all(debug_assertions, target_os = "macos"))]
    #[test]
    fn debug_build_uses_the_alpha_mark() {
        assert_eq!(ASSET, "alpha-app-icon.png");
    }

    #[cfg(not(all(debug_assertions, target_os = "macos")))]
    #[test]
    fn release_build_uses_the_production_mark() {
        assert_eq!(ASSET, "app-icon.png");
    }
}
