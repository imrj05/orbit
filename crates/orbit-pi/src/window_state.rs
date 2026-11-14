//! Where the main window was, so the next launch opens it there (#131).
//!
//! Persisted to `~/.orbit-pi/window.json`, beside `ui.json`:
//!
//! ```jsonc
//! {
//!   "x": 120, "y": 80, "width": 1280, "height": 820,
//!   "maximized": false,
//!   "display": "8F1A…"   // the display's UUID, when the platform has one
//! }
//! ```
//!
//! `x`/`y`/`width`/`height` are the *restore* bounds: the windowed frame the
//! window returns to when it is un-maximized. They are in the coordinate
//! space GPUI reports for that display — display-relative on macOS, the
//! virtual desktop on Windows and X11 — which is the same space the display's
//! own bounds use, so a clamp against the display is meaningful on every
//! platform.
//!
//! Restoring is conservative. A saved display that is no longer attached
//! means the configuration changed, and the window opens the first-launch
//! way rather than somewhere the user never put it. A frame that no longer
//! fits is clamped onto its display, so unplugging a monitor or lowering a
//! resolution can never strand the window off-screen. Parsing is lenient: a
//! missing, corrupt or nonsensical file means "first launch".

use std::path::{Path, PathBuf};

use gpui::{point, px, size, Bounds, Pixels, Size, WindowBounds};
use serde_json::{json, Value};

/// The main window's last frame, as written to `window.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// The windowed (restore) frame, never the maximized one.
    pub bounds: Bounds<Pixels>,
    /// Whether the window was maximized (or full screen) when last seen.
    pub maximized: bool,
    /// UUID of the display the window was on, when the platform reports one.
    pub display: Option<String>,
}

/// A display the window could open on, as [`restore`] needs to see it.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayInfo {
    pub uuid: Option<String>,
    pub bounds: Bounds<Pixels>,
}

/// How to open the window: on `displays[display]`, at `bounds`.
#[derive(Clone, Debug, PartialEq)]
pub struct Restore {
    pub bounds: Bounds<Pixels>,
    pub maximized: bool,
    pub display: usize,
}

impl Restore {
    /// The [`WindowBounds`] to hand GPUI's `WindowOptions`.
    pub fn window_bounds(&self) -> WindowBounds {
        if self.maximized {
            WindowBounds::Maximized(self.bounds)
        } else {
            WindowBounds::Windowed(self.bounds)
        }
    }
}

impl Saved {
    /// Read `~/.orbit-pi/window.json`; `None` means "open the default way".
    pub fn load() -> Option<Self> {
        Self::load_from(&persist_path())
    }

    pub(crate) fn load_from(path: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(path).ok()?;
        let value: Value = serde_json::from_str(&raw).ok()?;
        let num = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_f64)
                .map(|n| n as f32)
                .filter(|n| n.is_finite())
        };
        let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
        if w <= 0. || h <= 0. {
            return None;
        }
        Some(Self {
            bounds: Bounds::new(point(px(x), px(y)), size(px(w), px(h))),
            maximized: value
                .get("maximized")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            display: value
                .get("display")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        })
    }

    /// Persist to `~/.orbit-pi/window.json`. Best-effort: a failed write
    /// only means the next launch opens the default way.
    pub fn persist(&self) {
        self.persist_to(&persist_path());
    }

    pub(crate) fn persist_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut body = json!({
            "x": f32::from(self.bounds.origin.x),
            "y": f32::from(self.bounds.origin.y),
            "width": f32::from(self.bounds.size.width),
            "height": f32::from(self.bounds.size.height),
            "maximized": self.maximized,
        });
        if let Some(display) = &self.display {
            body["display"] = json!(display);
        }
        let _ = std::fs::write(path, body.to_string());
    }

    /// The frame to remember for a window that now reports `reported`.
    ///
    /// Windows and Linux report the restore frame inside
    /// [`WindowBounds::Maximized`]. macOS reports a zoomed window as
    /// `Windowed` with the zoomed frame, so `is_maximized` is the only
    /// signal; there the restore frame is the one remembered before the
    /// zoom, when there is one. Full screen restores as maximized: reopening
    /// into a separate Space is more than the user asked a relaunch to do.
    pub fn observe(
        previous: Option<&Saved>,
        reported: WindowBounds,
        is_maximized: bool,
        display: Option<String>,
    ) -> Saved {
        let (bounds, maximized) = match reported {
            WindowBounds::Windowed(bounds) if is_maximized => {
                (previous.map_or(bounds, |p| p.bounds), true)
            }
            WindowBounds::Windowed(bounds) => (bounds, false),
            WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
        };
        Saved {
            bounds,
            maximized,
            display,
        }
    }
}

/// Where to reopen a window last seen as `saved`, given today's displays;
/// `None` means "open the first-launch way".
///
/// A saved display UUID must still be attached. Without one (a platform
/// that cannot name its displays) the window goes to the display it
/// overlaps most, and to none — the default — when it overlaps nothing.
/// The frame is then shrunk to fit that display (not below `min_size`
/// unless the display itself is smaller) and moved fully onto it.
pub fn restore(saved: &Saved, displays: &[DisplayInfo], min_size: Size<Pixels>) -> Option<Restore> {
    let display = match &saved.display {
        Some(uuid) => displays
            .iter()
            .position(|d| d.uuid.as_deref() == Some(uuid.as_str()))?,
        None => {
            displays
                .iter()
                .enumerate()
                .map(|(i, d)| (i, overlap_area(&saved.bounds, &d.bounds)))
                .filter(|(_, area)| *area > 0.)
                .max_by(|a, b| a.1.total_cmp(&b.1))?
                .0
        }
    };
    Some(Restore {
        bounds: clamp(saved.bounds, displays[display].bounds, min_size),
        maximized: saved.maximized,
        display,
    })
}

/// `bounds` resized to fit `display` and moved fully onto it.
pub(crate) fn clamp(
    bounds: Bounds<Pixels>,
    display: Bounds<Pixels>,
    min_size: Size<Pixels>,
) -> Bounds<Pixels> {
    let fit = |want: f32, min: f32, room: f32| want.max(min.min(room)).min(room);
    let width = fit(
        f32::from(bounds.size.width),
        f32::from(min_size.width),
        f32::from(display.size.width),
    );
    let height = fit(
        f32::from(bounds.size.height),
        f32::from(min_size.height),
        f32::from(display.size.height),
    );
    let place = |at: f32, start: f32, room: f32, len: f32| at.clamp(start, start + room - len);
    let x = place(
        f32::from(bounds.origin.x),
        f32::from(display.origin.x),
        f32::from(display.size.width),
        width,
    );
    let y = place(
        f32::from(bounds.origin.y),
        f32::from(display.origin.y),
        f32::from(display.size.height),
        height,
    );
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
}

fn overlap_area(a: &Bounds<Pixels>, b: &Bounds<Pixels>) -> f32 {
    let i = a.intersect(b);
    f32::from(i.size.width).max(0.) * f32::from(i.size.height).max(0.)
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("window.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn min() -> Size<Pixels> {
        size(px(960.), px(640.))
    }

    fn laptop() -> DisplayInfo {
        DisplayInfo {
            uuid: Some("laptop".into()),
            bounds: rect(0., 0., 1512., 982.),
        }
    }

    fn monitor() -> DisplayInfo {
        DisplayInfo {
            uuid: Some("monitor".into()),
            bounds: rect(1512., 0., 2560., 1440.),
        }
    }

    fn saved(bounds: Bounds<Pixels>, maximized: bool, display: Option<&str>) -> Saved {
        Saved {
            bounds,
            maximized,
            display: display.map(str::to_owned),
        }
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("orbit-window-state-{}", std::process::id()))
            .join(name)
    }

    #[test]
    fn a_frame_that_fits_restores_unchanged_on_its_display() {
        let frame = rect(1700., 100., 1400., 900.);
        let got = restore(
            &saved(frame, false, Some("monitor")),
            &[laptop(), monitor()],
            min(),
        )
        .expect("the monitor is still attached");
        assert_eq!(got.bounds, frame);
        assert_eq!(got.display, 1);
        assert!(matches!(got.window_bounds(), WindowBounds::Windowed(b) if b == frame));
    }

    #[test]
    fn a_maximized_window_reopens_maximized_with_its_restore_frame() {
        let frame = rect(40., 30., 1200., 800.);
        let got = restore(&saved(frame, true, Some("laptop")), &[laptop()], min()).unwrap();
        assert!(matches!(got.window_bounds(), WindowBounds::Maximized(b) if b == frame));
    }

    #[test]
    fn a_frame_too_large_for_its_display_is_shrunk_and_moved_onto_it() {
        // Saved on the big monitor's coordinates, now on a smaller panel with
        // the same UUID (a lowered resolution).
        let shrunk = DisplayInfo {
            uuid: Some("monitor".into()),
            bounds: rect(1512., 0., 1280., 800.),
        };
        let got = restore(
            &saved(rect(2400., 500., 2000., 1200.), false, Some("monitor")),
            &[laptop(), shrunk.clone()],
            min(),
        )
        .unwrap();
        assert_eq!(
            got.bounds, shrunk.bounds,
            "fills the display, nothing off it"
        );
    }

    #[test]
    fn an_offscreen_origin_is_pulled_back_onto_the_display() {
        let got = restore(
            &saved(rect(-500., 5000., 1100., 700.), false, Some("laptop")),
            &[laptop()],
            min(),
        )
        .unwrap();
        assert_eq!(got.bounds, rect(0., 282., 1100., 700.));
    }

    #[test]
    fn a_missing_display_falls_back_to_the_default() {
        assert_eq!(
            restore(
                &saved(rect(1700., 100., 1400., 900.), false, Some("monitor")),
                &[laptop()],
                min()
            ),
            None,
            "the monitor was unplugged: open the first-launch way"
        );
    }

    #[test]
    fn without_a_uuid_the_most_overlapped_display_wins() {
        let anonymous = |d: DisplayInfo| DisplayInfo { uuid: None, ..d };
        let displays = [anonymous(laptop()), anonymous(monitor())];
        // Straddles the seam, mostly on the monitor.
        let got = restore(
            &saved(rect(1400., 0., 1200., 800.), false, None),
            &displays,
            min(),
        )
        .unwrap();
        assert_eq!(got.display, 1);
        assert_eq!(got.bounds, rect(1512., 0., 1200., 800.));

        assert_eq!(
            restore(
                &saved(rect(9000., 9000., 1200., 800.), false, None),
                &displays,
                min()
            ),
            None,
            "a frame on no display opens the default way"
        );
    }

    #[test]
    fn a_tiny_frame_grows_to_the_minimum_size() {
        let got = restore(
            &saved(rect(10., 10., 300., 200.), false, Some("laptop")),
            &[laptop()],
            min(),
        )
        .unwrap();
        assert_eq!(got.bounds.size, min());
    }

    #[test]
    fn observe_keeps_the_restore_frame_through_a_macos_zoom() {
        let windowed = rect(100., 100., 1200., 800.);
        let before = Saved::observe(None, WindowBounds::Windowed(windowed), false, None);
        assert_eq!(before, saved(windowed, false, None));

        // macOS: zoomed, reported as Windowed with the full-screen frame.
        let zoomed = rect(0., 25., 1512., 957.);
        let after = Saved::observe(Some(&before), WindowBounds::Windowed(zoomed), true, None);
        assert_eq!(after, saved(windowed, true, None));

        // Windows/Linux: the restore frame comes with the report.
        let reported = Saved::observe(
            Some(&before),
            WindowBounds::Maximized(rect(5., 5., 900., 700.)),
            true,
            Some("d".into()),
        );
        assert_eq!(reported, saved(rect(5., 5., 900., 700.), true, Some("d")));

        let full = Saved::observe(None, WindowBounds::Fullscreen(windowed), false, None);
        assert!(full.maximized, "full screen relaunches maximized");
    }

    #[test]
    fn saved_state_round_trips_through_the_file() {
        let dir = temp("roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("window.json");
        for state in [
            saved(rect(-1200.5, 40., 1000., 700.), true, Some("ABC-123")),
            saved(rect(0., 0., 1280., 800.), false, None),
        ] {
            state.persist_to(&path);
            assert_eq!(Saved::load_from(&path), Some(state));
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_or_nonsense_files_mean_first_launch() {
        let dir = temp("nonsense");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("window.json");
        assert_eq!(Saved::load_from(&path), None);
        std::fs::create_dir_all(&dir).unwrap();
        for raw in [
            "",
            "{",
            "null",
            "[]",
            r#"{"x":0,"y":0,"width":1200}"#,
            r#"{"x":0,"y":0,"width":0,"height":800}"#,
            r#"{"x":0,"y":0,"width":-5,"height":800}"#,
            r#"{"x":"0","y":0,"width":1200,"height":800}"#,
        ] {
            std::fs::write(&path, raw).unwrap();
            assert_eq!(Saved::load_from(&path), None, "{raw:?}");
        }
        // Unknown keys and a bad `maximized` are tolerated.
        std::fs::write(
            &path,
            r#"{"x":1,"y":2,"width":1200,"height":800,"maximized":"yes","future":1}"#,
        )
        .unwrap();
        assert_eq!(
            Saved::load_from(&path),
            Some(saved(rect(1., 2., 1200., 800.), false, None))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
