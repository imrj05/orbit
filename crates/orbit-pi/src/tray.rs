//! The notification-area / menu-bar icon and what closing the window does
//! (#131).
//!
//! One close policy, [`CloseBehavior`], persisted in `ui.json`:
//!
//! - **Hide to tray** keeps Orbit — and any run in flight — alive with the
//!   window gone, and the icon brings it back. On macOS that is today's
//!   behaviour (the shell is parked and the Dock reopens it); the menu-bar
//!   icon is the second way back. On Linux the window closes the same way,
//!   with GPUI told to keep its event loop running (a vendored patch). On
//!   Windows the window is hidden instead of closed, because closing GPUI's
//!   last window there ends the event loop.
//! - **Quit** quits, through `request_quit`, so a quit still waits for a
//!   fork in flight exactly as ⌘Q / Ctrl+Q does.
//!
//! Without an icon there is no way back to a hidden window, which is worse
//! than quitting: a desktop with no StatusNotifierItem host (a bare window
//! manager, GNOME without the AppIndicator extension) makes the icon fail to
//! come up, and closing the window then quits.
//!
//! The platform icon only *reports* clicks: each backend pushes a
//! [`TrayEvent`] onto a queue that the app heartbeat drains on the main
//! thread with a full `App` in hand (`main_window::handle_tray_events`).

use std::sync::Mutex;

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// What the window's close button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseBehavior {
    HideToTray,
    Quit,
}

impl CloseBehavior {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::HideToTray => "hide",
            Self::Quit => "quit",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "hide" => Some(Self::HideToTray),
            "quit" => Some(Self::Quit),
            _ => None,
        }
    }

    /// macOS keeps running when its window closes, as Mac apps do and as
    /// Orbit always has. Windows and Linux keep quitting until the user asks
    /// for the tray, so nothing changes silently for existing users.
    pub fn platform_default() -> Self {
        if cfg!(target_os = "macos") {
            Self::HideToTray
        } else {
            Self::Quit
        }
    }
}

/// What a close request turns into on this platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Let the window close; the app stays alive with the shell parked
    /// (macOS, and Linux while the icon is up).
    CloseWindow,
    /// Keep the window but hide it; the tray icon shows it again (Windows).
    HideWindow,
    /// Quit the app through `request_quit`.
    Quit,
}

/// Decide a close: `tray_up` says whether the icon actually exists, so a
/// failed tray install degrades to quitting rather than hiding the window
/// with no way back.
pub fn close_action(policy: CloseBehavior, tray_up: bool) -> CloseAction {
    match policy {
        CloseBehavior::Quit => CloseAction::Quit,
        // The Dock reopens a Mac app with no windows, tray or not.
        CloseBehavior::HideToTray if cfg!(target_os = "macos") => CloseAction::CloseWindow,
        CloseBehavior::HideToTray if !tray_up => CloseAction::Quit,
        // Closing GPUI's last window ends the Windows event loop; Linux has
        // the keep-alive patch instead.
        CloseBehavior::HideToTray if cfg!(windows) => CloseAction::HideWindow,
        CloseBehavior::HideToTray => CloseAction::CloseWindow,
    }
}

/// Whether this platform has a tray icon at all.
pub const fn supported() -> bool {
    cfg!(any(
        target_os = "macos",
        windows,
        target_os = "linux",
        target_os = "freebsd"
    ))
}

/// A click on the icon or one of its menu items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    NewTask,
    Quit,
}

/// The icon's menu and tooltip, already translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLabels {
    pub tooltip: String,
    pub show: String,
    pub new_task: String,
    pub quit: String,
}

static EVENTS: Mutex<Vec<TrayEvent>> = Mutex::new(Vec::new());

/// Called by the platform backends from their click handlers.
fn push(event: TrayEvent) {
    if let Ok(mut events) = EVENTS.lock() {
        events.push(event);
    }
}

/// Take the clicks that arrived since the last heartbeat.
pub fn drain() -> Vec<TrayEvent> {
    EVENTS
        .lock()
        .map(|mut events| std::mem::take(&mut *events))
        .unwrap_or_default()
}

/// Put the icon up (`Some(labels)`) or take it down (`None`); returns whether
/// it is up afterwards. Idempotent: an icon already up only has its labels
/// refreshed, so a language change reaches the menu.
pub fn set_icon(labels: Option<&TrayLabels>) -> bool {
    #[cfg(target_os = "macos")]
    return macos::set_icon(labels);
    #[cfg(windows)]
    return windows::set_icon(labels);
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        let up = linux::set_icon(labels);
        // A closed window may only leave the app running while the icon can
        // bring it back.
        gpui::set_keep_alive_without_windows(up);
        up
    }
    #[cfg(not(any(
        target_os = "macos",
        windows,
        target_os = "linux",
        target_os = "freebsd"
    )))]
    {
        let _ = labels;
        false
    }
}

/// Whether the icon is currently up.
pub fn icon_up() -> bool {
    #[cfg(target_os = "macos")]
    return macos::icon_up();
    #[cfg(windows)]
    return windows::icon_up();
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    return linux::icon_up();
    #[cfg(not(any(
        target_os = "macos",
        windows,
        target_os = "linux",
        target_os = "freebsd"
    )))]
    false
}

/// Hide the main window without closing it (Windows' Hide to tray).
pub fn hide_window(window: &gpui::Window) {
    #[cfg(windows)]
    if let Some(hwnd) = hwnd(window) {
        windows::hide_window(hwnd);
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// Show a window hidden by [`hide_window`] and bring it to the front.
pub fn show_window(window: &gpui::Window) {
    #[cfg(windows)]
    if let Some(hwnd) = hwnd(window) {
        windows::show_window(hwnd);
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// The window's `HWND`, the same way `platform::windows_chrome` reads it.
#[cfg(windows)]
fn hwnd(window: &gpui::Window) -> Option<*mut std::ffi::c_void> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut std::ffi::c_void),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_behavior_round_trips_and_rejects_junk() {
        for behavior in [CloseBehavior::HideToTray, CloseBehavior::Quit] {
            assert_eq!(CloseBehavior::parse(behavior.as_str()), Some(behavior));
        }
        assert_eq!(CloseBehavior::parse("minimize"), None);
    }

    #[test]
    fn quit_always_quits() {
        for tray_up in [false, true] {
            assert_eq!(
                close_action(CloseBehavior::Quit, tray_up),
                CloseAction::Quit
            );
        }
    }

    #[test]
    fn hide_without_an_icon_never_strands_the_window() {
        // (macOS still closes: the Dock is always a way back.)
        let action = close_action(CloseBehavior::HideToTray, false);
        if cfg!(target_os = "macos") {
            assert_eq!(
                action,
                CloseAction::CloseWindow,
                "the Dock reopens a Mac app"
            );
        } else {
            assert_eq!(action, CloseAction::Quit, "no icon, no way back: quit");
        }
    }

    #[test]
    fn hide_with_an_icon_keeps_the_app() {
        let expected = if cfg!(windows) {
            CloseAction::HideWindow
        } else {
            CloseAction::CloseWindow
        };
        assert_eq!(close_action(CloseBehavior::HideToTray, true), expected);
    }

    #[test]
    fn events_drain_once_in_order() {
        let _ = drain();
        push(TrayEvent::Show);
        push(TrayEvent::Quit);
        assert_eq!(drain(), [TrayEvent::Show, TrayEvent::Quit]);
        assert!(drain().is_empty());
    }

    #[test]
    fn defaults_keep_each_platforms_current_behavior() {
        let expected = if cfg!(target_os = "macos") {
            CloseBehavior::HideToTray
        } else {
            CloseBehavior::Quit
        };
        assert_eq!(CloseBehavior::platform_default(), expected);
    }
}
