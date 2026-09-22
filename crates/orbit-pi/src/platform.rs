//! macOS helpers for detecting installed folder-capable apps and opening a
//! workspace path in one of them (editors, terminals, Finder, etc.).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{Image, SharedString, TitlebarOptions, Window};
// `point`/`px` place the macOS traffic lights; no other platform takes a
// position, so they stay behind the same gate as that titlebar.
#[cfg(target_os = "macos")]
use gpui::{point, px};
use serde_json::Value;

/// A folder-capable application the header's "open in" control can target,
/// resolved against what is installed on this machine.
#[derive(Clone)]
pub struct ExternalApp {
    /// Stable identifier persisted as the user's preferred target.
    pub id: &'static str,
    pub label: &'static str,
    /// The bundle id that resolved here, for launching.
    pub bundle_id: &'static str,
    pub icon: Arc<Image>,
}

/// Known folder-capable apps in menu order — editors, the file manager,
/// terminals, IDEs. An entry lists every bundle id it ships under; the first
/// installed one wins.
#[cfg(target_os = "macos")]
const TERMY_BUNDLE_ID: &str = "com.lassevestergaard.termy";

#[cfg(target_os = "macos")]
const OPEN_IN_CATALOG: &[(&str, &str, &[&str])] = &[
    ("vscode", "VS Code", &["com.microsoft.VSCode"]),
    ("cursor", "Cursor", &["com.todesktop.230313mzl4w4u92"]),
    ("zed", "Zed", &["dev.zed.Zed", "dev.zed.Zed-Preview"]),
    ("finder", "Finder", &["com.apple.finder"]),
    ("terminal", "Terminal", &["com.apple.Terminal"]),
    ("termy", "Termy", &[TERMY_BUNDLE_ID]),
    ("iterm2", "iTerm2", &["com.googlecode.iterm2"]),
    ("kitty", "Kitty", &["net.kovidgoyal.kitty"]),
    ("ghostty", "Ghostty", &["com.mitchellh.ghostty"]),
    ("warp", "Warp", &["dev.warp.Warp-Stable", "dev.warp.Warp"]),
    ("xcode", "Xcode", &["com.apple.dt.Xcode"]),
    (
        "rider",
        "Rider",
        &["com.jetbrains.rider", "com.jetbrains.rider-EAP"],
    ),
    (
        "android-studio",
        "Android Studio",
        &["com.google.android.studio"],
    ),
];

#[cfg(target_os = "macos")]
fn app_icon_for_application_path(
    application_path: &objc2_foundation::NSString,
) -> Option<Arc<Image>> {
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
    use objc2_foundation::{NSDictionary, NSSize};

    let image = NSWorkspace::sharedWorkspace().iconForFile(application_path);
    image.setSize(NSSize::new(32.0, 32.0));
    let tiff = image.TIFFRepresentation()?;
    let bitmap_rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    let properties = NSDictionary::new();
    let png_data = unsafe {
        bitmap_rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties)
    }?;
    let bytes = unsafe { png_data.as_bytes_unchecked() };
    (!bytes.is_empty()).then(|| Arc::new(Image::from_bytes(gpui::ImageFormat::Png, bytes.to_vec())))
}

/// Resolve which catalog apps are installed, with their icons.
#[cfg(target_os = "macos")]
pub fn detect_open_in_apps() -> Vec<ExternalApp> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let workspace = NSWorkspace::sharedWorkspace();
    OPEN_IN_CATALOG
        .iter()
        .filter_map(|&(id, label, bundle_ids)| {
            bundle_ids.iter().find_map(|&bundle_id| {
                let application_url = workspace
                    .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))?;
                let application_path = application_url.path()?;
                Some(ExternalApp {
                    id,
                    label,
                    bundle_id,
                    icon: app_icon_for_application_path(&application_path)?,
                })
            })
        })
        .collect()
}

#[cfg(not(target_os = "macos"))]
pub fn detect_open_in_apps() -> Vec<ExternalApp> {
    Vec::new()
}

/// Open `path` in the application `bundle_id`, activating it. Launch Services
/// delivers the open asynchronously, so this never blocks.
#[cfg(target_os = "macos")]
pub fn open_path_in_app(path: &Path, bundle_id: &str) {
    use objc2_app_kit::{NSWorkspace, NSWorkspaceOpenConfiguration};
    use objc2_foundation::{NSArray, NSString, NSURL};

    let workspace = NSWorkspace::sharedWorkspace();
    let Some(application_url) =
        workspace.URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))
    else {
        return;
    };
    let url = if bundle_id == TERMY_BUNDLE_ID {
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&termy_open_url(path))) else {
            return;
        };
        url
    } else {
        NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
    };
    workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
        &NSArray::from_retained_slice(&[url]),
        &application_url,
        &NSWorkspaceOpenConfiguration::configuration(),
        None,
    );
}

#[cfg(target_os = "macos")]
fn termy_open_url(path: &Path) -> String {
    let mut url = url::Url::parse("termy://new").expect("static Termy URL should be valid");
    url.query_pairs_mut()
        .append_pair("dir", &path.to_string_lossy());
    url.into()
}

#[cfg(not(target_os = "macos"))]
pub fn open_path_in_app(_: &Path, _: &str) {}

/// Reveal `path` in the OS file manager, selecting it. Used by the skills
/// page to open a skill's directory.
#[cfg(target_os = "macos")]
pub fn reveal_in_file_manager(path: &Path) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn reveal_in_file_manager(path: &Path) {
    #[cfg(target_os = "windows")]
    {
        // `explorer /select,<path>` needs the comma but no separating space;
        // it is also the only way Explorer highlights the file itself.
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        // Most Linux desktops have no "select" verb; open the containing
        // folder.
        let target = path.parent().unwrap_or(path);
        let _ = std::process::Command::new("xdg-open").arg(target).spawn();
    }
}

/// Whether [`trash_path`] moves a file to the OS trash (recoverable) or
/// deletes it outright. The Explorer's delete confirmation is worded from
/// this, so it never promises a recovery the platform cannot deliver.
pub const TRASH_IS_RECOVERABLE: bool = cfg!(target_os = "macos");

/// Delete `path`, preferring the OS trash so a mistake is recoverable.
///
/// macOS uses `NSFileManager`'s `trashItemAtURL:` — the same move Finder's
/// **Move to Trash** performs, including collision-renaming and restore.
/// Windows/Linux are best-effort and fall through to a permanent delete; the
/// caller checks [`TRASH_IS_RECOVERABLE`] before promising otherwise.
#[cfg(target_os = "macos")]
pub fn trash_path(path: &Path) -> std::io::Result<()> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    NSFileManager::defaultManager()
        .trashItemAtURL_resultingItemURL_error(&url, None)
        .map_err(|error| {
            std::io::Error::other(error.localizedDescription().to_string())
        })
}

#[cfg(not(target_os = "macos"))]
pub fn trash_path(path: &Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

/// Open `path` in the OS default application (the user's editor for a
/// `SKILL.md`). Used by the skills page's **Open SKILL.md** action.
#[cfg(target_os = "macos")]
pub fn open_path_default(path: &Path) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg(path)
        .spawn();
}

#[cfg(not(target_os = "macos"))]
pub fn open_path_default(path: &Path) {
    // Windows has no `xdg-open`; the shell's `start` verb is the equivalent
    // (`""` is the window title `start` would otherwise read as the path).
    #[cfg(target_os = "windows")]
    {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]).arg(path);
        orbit_rpc::hide_console(&mut command);
        let _ = command.spawn();
    }
    #[cfg(not(target_os = "windows"))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

/// The user's home directory — the root of both pi's store (`~/.pi`) and
/// Orbit's own (`~/.orbit-pi`).
///
/// `HOME` is the POSIX answer and the one macOS and Linux always set. Windows
/// sets it only inside a POSIX-ish shell: a process started from Explorer or
/// PowerShell has `USERPROFILE` (and, on domain/legacy setups, `HOMEDRIVE` +
/// `HOMEPATH`) instead. Reading `HOME` alone would resolve the session store to
/// a drive root there, so every platform gets the same fallback chain here.
pub fn home_dir() -> PathBuf {
    home_dir_opt().unwrap_or_else(|| PathBuf::from("."))
}

/// [`home_dir`] when the platform gives one. `None` keeps a caller that would
/// *write* there from writing to a relative path instead.
pub fn home_dir_opt() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(home));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(profile));
    }
    match (std::env::var_os("HOMEDRIVE"), std::env::var_os("HOMEPATH")) {
        // `HOMEPATH` is root-relative (`\Users\name`), so `join` appends it to
        // the drive rather than replacing it.
        (Some(drive), Some(path)) if !drive.is_empty() && !path.is_empty() => {
            Some(PathBuf::from(drive).join(path))
        }
        _ => None,
    }
}

fn open_in_prefs_path() -> PathBuf {
    home_dir().join(".orbit-pi").join("open-in.json")
}

/// Load the persisted preferred open-in app id, if any.
pub fn load_preferred_open_in_app() -> Option<String> {
    let raw = std::fs::read_to_string(open_in_prefs_path()).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    value
        .get("open_in_app")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// Remember the user's preferred open-in app id.
pub fn persist_preferred_open_in_app(app_id: &str) {
    let path = open_in_prefs_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(
        path,
        serde_json::json!({ "open_in_app": app_id }).to_string(),
    );
}

/// Run an interactive shell command in the user's terminal. Used for
/// `pi /login <provider>`, whose OAuth/device flow is a terminal UI Orbit
/// cannot render. Best-effort: failures leave the caller to surface a hint.
#[cfg(target_os = "macos")]
pub fn open_terminal_command(command: &str) -> Result<(), String> {
    // A `.command` file opened with `open` runs in Terminal.app without
    // requiring Automation (AppleScript) permission, and works even when a
    // non-default terminal is installed.
    let path = std::env::temp_dir().join(format!("orbit-{}.command", std::process::id()));
    let script = format!("#!/bin/zsh\n{command}\n");
    std::fs::write(&path, script).map_err(|err| tr!("platform.login_script_failed", error = err))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
    }
    std::process::Command::new("/usr/bin/open")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|err| tr!("platform.terminal_failed", error = err))
}

#[cfg(not(target_os = "macos"))]
pub fn open_terminal_command(command: &str) -> Result<(), String> {
    // Windows opens a console running the command through the shell; `start`
    // gives it its own window instead of attaching to this process.
    #[cfg(target_os = "windows")]
    {
        let mut launcher = std::process::Command::new("cmd");
        launcher.args(["/C", "start", "cmd", "/K", command]);
        orbit_rpc::hide_console(&mut launcher);
        launcher
            .spawn()
            .map(|_| ())
            .map_err(|err| tr!("platform.terminal_failed", error = err))
    }
    #[cfg(not(target_os = "windows"))]
    {
        for terminal in ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"] {
            let mut cmd = std::process::Command::new(terminal);
            match terminal {
                "gnome-terminal" => cmd.arg("--").args(["sh", "-c", command]),
                _ => cmd.args(["-e", "sh", "-c", command]),
            };
            if cmd.spawn().is_ok() {
                return Ok(());
            }
        }
        Err(tr!("platform.no_terminal_found"))
    }
}

/// Open an `http(s)` URL in the user's default browser. Used for the OAuth
/// authorization leg of a browser login; the URL comes from pi's `auth.*`
/// events, so the scheme is validated before launching.
#[cfg(target_os = "macos")]
pub fn open_url(url: &str) -> Result<(), String> {
    if !is_safe_browser_url(url) {
        return Err(tr!("platform.unsafe_url"));
    }
    std::process::Command::new("/usr/bin/open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|err| tr!("platform.browser_failed", error = err))
}

#[cfg(not(target_os = "macos"))]
pub fn open_url(url: &str) -> Result<(), String> {
    if !is_safe_browser_url(url) {
        return Err(tr!("platform.unsafe_url"));
    }
    let opener = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "xdg-open"
    };
    let mut command = std::process::Command::new(opener);
    if cfg!(target_os = "windows") {
        command.args(["/C", "start", "", url]);
    } else {
        command.arg(url);
    }
    orbit_rpc::hide_console(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|err| tr!("platform.browser_failed", error = err))
}

/// Rename the running process so macOS labels the application menu with the
/// product name instead of the executable (`orbit-pi` under `cargo run`).
///
/// AppKit ignores any title set on the first menu item and always paints the
/// resolved application name — `CFBundleName` for a bundle, else the process
/// name — so the title must be changed at the source. The Carbon Process
/// Manager call below is the supported-by-practice way to do that at runtime;
/// it also keeps the app menu bold. (`NSMenu.setTitle` on the app submenu is
/// ignored by current macOS, so it is not used.)
#[cfg(target_os = "macos")]
pub fn set_process_name(name: &str) {
    use std::ffi::{c_char, c_int, c_void, CString};

    #[repr(C)]
    struct ProcessSerialNumber {
        high_long_of_psn: u32,
        low_long_of_psn: u32,
    }
    type GetCurrentProcessFn = unsafe extern "C" fn(*mut ProcessSerialNumber) -> c_int;
    type CpSetProcessNameFn =
        unsafe extern "C" fn(*const ProcessSerialNumber, *const c_char) -> c_int;

    // `RTLD_DEFAULT` (-2): search images AppKit has already loaded.
    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
    let Ok(name) = CString::new(name) else {
        return;
    };

    // SAFETY: both symbols are resolved by name from ApplicationServices and
    // transmuted to their documented C signatures before use.
    unsafe {
        let get_sym = libc::dlsym(RTLD_DEFAULT, c"GetCurrentProcess".as_ptr());
        let set_sym = libc::dlsym(RTLD_DEFAULT, c"CPSSetProcessName".as_ptr());
        if get_sym.is_null() || set_sym.is_null() {
            return;
        }
        let get: GetCurrentProcessFn = std::mem::transmute(get_sym);
        let set: CpSetProcessNameFn = std::mem::transmute(set_sym);
        let mut psn = ProcessSerialNumber {
            high_long_of_psn: 0,
            low_long_of_psn: 0,
        };
        if get(&mut psn) == 0 {
            let _ = set(&psn, name.as_ptr());
        }
    }
}

/// Menu bars only exist on macOS; GPUI's Windows/Linux platforms store the
/// menu but never paint one, so there is nothing to relabel.
#[cfg(not(target_os = "macos"))]
pub fn set_process_name(_name: &str) {}

/// Only `http`/`https` URLs reach the OS opener, so a compromised or buggy
/// server can never hand the shell a `file:`/`javascript:` target.
fn is_safe_browser_url(url: &str) -> bool {
    let trimmed = url.trim();
    trimmed.starts_with("https://") || trimmed.starts_with("http://")
}

/// Open System Settings → Notifications so the user can unblock Orbit's
/// banners. macOS 13+ (the app's floor) uses the Notifications extension
/// URL; the legacy `com.apple.preference` pane is the older fallback.
#[cfg(target_os = "macos")]
pub fn open_notification_settings() -> Result<(), String> {
    const PANES: [&str; 2] = [
        "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
        "x-apple.systempreferences:com.apple.preference.notifications",
    ];
    for pane in PANES {
        let opened = std::process::Command::new("/usr/bin/open")
            .arg(pane)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if opened {
            return Ok(());
        }
    }
    Err(tr!("platform.open_settings_failed"))
}

#[cfg(not(target_os = "macos"))]
pub fn open_notification_settings() -> Result<(), String> {
    Ok(())
}

// ── host description ────────────────────────────────────────────────────

/// The machine Orbit is running on, as the setup page reports it.
pub struct Host {
    /// Human label: `Windows 11 (build 26100)`, `macOS 15.2`,
    /// `Ubuntu 24.04.1 LTS`.
    pub label: String,
    /// Rust's target architecture, e.g. `x86_64` or `aarch64`.
    pub arch: &'static str,
    /// Set when the host is older than this build supports, naming the floor.
    /// A host whose version can't be read is never reported as unsupported.
    pub unsupported: Option<String>,
}

/// A version read off the host, compared against the floors each bundle
/// declares. Only the fields the host exposes are filled: Windows reports a
/// build number, macOS a major/minor pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OsVersion {
    major: u32,
    minor: u32,
    build: u32,
}

/// Probe the host OS for the setup page.
///
/// A version is only reachable by running something — `sw_vers` on macOS,
/// `ver` on Windows — or by reading `/etc/os-release` on Linux; Orbit takes no
/// dependency for one field of one page. When the probe fails the label falls
/// back to the `std::env::consts::OS` name, which is still right about the
/// family.
pub fn host() -> Host {
    os_probe()
}

/// Run `command args`, returning trimmed stdout when it succeeds.
fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let mut command = std::process::Command::new(command);
    command.args(args);
    orbit_rpc::hide_console(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The bare OS family name, used when a version probe fails.
fn os_family() -> String {
    match std::env::consts::OS {
        "macos" => "macOS".into(),
        "windows" => "Windows".into(),
        other => other.to_owned(),
    }
}

/// macOS 13 is the floor in `package.metadata.bundle`'s
/// `osx_minimum_system_version` and in the `LSMinimumSystemVersion` both
/// bundles write, so the setup page states the same number the OS enforces.
#[cfg(target_os = "macos")]
fn os_probe() -> Host {
    let (label, version) = match command_output("sw_vers", &["-productVersion"])
        .as_deref()
        .and_then(parse_macos_version)
    {
        Some((label, version)) => (label, Some(version)),
        None => (os_family(), None),
    };
    Host {
        label,
        arch: std::env::consts::ARCH,
        unsupported: version.and_then(unsupported_reason),
    }
}

/// Windows 10 1809 (build 17763) is the floor: GPUI composes through DXGI
/// flip-model presentation, which earlier builds don't expose.
#[cfg(windows)]
fn os_probe() -> Host {
    // `ver` prints `Microsoft Windows [Version 10.0.26100.9168]`. The 10.0
    // major-minor pair covers both Windows 10 and 11, so the build number is
    // what distinguishes them.
    let (label, version) = match command_output("cmd", &["/C", "ver"])
        .as_deref()
        .and_then(parse_windows_version)
    {
        Some((label, version)) => (label, Some(version)),
        None => (os_family(), None),
    };
    Host {
        label,
        arch: std::env::consts::ARCH,
        unsupported: version.and_then(unsupported_reason),
    }
}

/// Whether a host version is below this target's floor, and what it needs.
///
/// One rule per platform, in one place, so the probe stays a probe and the
/// floors are testable without the machine the test runs on.
fn unsupported_reason(version: OsVersion) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        const FLOOR_MAJOR: u32 = 13;
        if version.major > 0 && version.major < FLOOR_MAJOR {
            return Some(tr!("platform.needs_macos", major = FLOOR_MAJOR));
        }
    }
    #[cfg(windows)]
    {
        const FLOOR_BUILD: u32 = 17763;
        if version.build > 0 && version.build < FLOOR_BUILD {
            return Some(tr!("platform.needs_windows").to_string());
        }
    }
    // Linux has no floor to compare against (see `os_probe`).
    let _ = version;
    None
}

/// Linux kernels don't carry a distribution, so the label comes from
/// `/etc/os-release`. There is no version floor to compare against: the same
/// build runs on any distro with a Vulkan or GL driver.
#[cfg(all(unix, not(target_os = "macos")))]
fn os_probe() -> Host {
    let label = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|contents| parse_os_release(&contents))
        .unwrap_or_else(os_family);
    Host {
        label,
        arch: std::env::consts::ARCH,
        unsupported: None,
    }
}

/// `15.2.1` → `("macOS 15.2", 15.2)`.
#[cfg(any(target_os = "macos", test))]
fn parse_macos_version(raw: &str) -> Option<(String, OsVersion)> {
    let mut parts = raw.trim().split('.').map(str::trim);
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    Some((
        format!("macOS {major}.{minor}"),
        OsVersion {
            major,
            minor,
            build: 0,
        },
    ))
}

/// `Microsoft Windows [Version 10.0.26100.9168]` →
/// `("Windows 11 (build 26100)", 10.0.26100)`. The marketing name follows the
/// build number: 22000 and up is Windows 11, everything the 10.0 line covers
/// below that is Windows 10.
#[cfg(any(windows, test))]
fn parse_windows_version(raw: &str) -> Option<(String, OsVersion)> {
    let inside = raw.split_once("Version ")?.1;
    let inside = inside.trim_end_matches(']').trim();
    let mut parts = inside.split('.').map(str::trim);
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let build: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let edition = if build >= 22000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    let label = if build > 0 {
        format!("{edition} (build {build})")
    } else {
        edition.to_string()
    };
    Some((
        label,
        OsVersion {
            major,
            minor,
            build,
        },
    ))
}

/// `PRETTY_NAME="Ubuntu 24.04.1 LTS"` out of `/etc/os-release`.
#[cfg(any(all(unix, not(target_os = "macos")), test))]
fn parse_os_release(contents: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "PRETTY_NAME").then(|| value.trim().trim_matches('"').to_owned())
    })
}

// ── window chrome ───────────────────────────────────────────────────────

/// Clearance the app's own titlebar controls must leave for the OS's window
/// buttons. macOS draws its traffic lights *inside* the transparent titlebar
/// (see [`titlebar_options`]), so the controls start past them; every other
/// platform keeps the system titlebar, whose buttons sit above the client
/// area, and only needs a plain inset from the window edge.
pub const WINDOW_CONTROLS_CLEARANCE: f32 = if cfg!(target_os = "macos") { 80. } else { 12. };

/// What one of the app's caption buttons asks the window to do. These mirror
/// the system commands, so the platform performs the action — the app never
/// reimplements window management.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowCommand {
    Minimize,
    /// Maximize when the window is not maximized, restore it when it is —
    /// what the system's own maximize button does.
    ToggleMaximize,
    Close,
}

/// Windows is the one platform where the app paints the caption itself: macOS
/// gets AppKit's traffic lights inside the transparent titlebar, and Linux
/// decorates through the window manager. See [`titlebar_options`].
pub fn draws_window_controls() -> bool {
    cfg!(windows)
}

/// Begin an OS-driven window move, the way grabbing a titlebar does.
///
/// Windows only, and not through GPUI's `WindowControlArea::Drag`: that path
/// needs the press to reach the platform unhandled, and this app's root
/// tracks focus, so GPUI's focus-transfer listener calls
/// `Window::prevent_default` on every press inside the window. A prevented
/// press is treated as handled, which makes GPUI drop the window-control
/// path entirely — no move loop, and no caption-button commands. So the press
/// hands the move to the OS itself (`WM_NCLBUTTONDOWN` + `HTCAPTION`), which
/// is the standard way a custom titlebar drags a Windows window.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn start_window_drag(window: &Window) {
    #[cfg(windows)]
    windows_chrome::start_drag(window);
    #[cfg(not(windows))]
    let _ = window;
}

/// Perform [`WindowCommand`] — the same system command the corresponding
/// caption button sends.
pub fn window_command(window: &Window, command: WindowCommand) {
    #[cfg(windows)]
    windows_chrome::command(window, command);
    #[cfg(not(windows))]
    let _ = (window, command);
}

/// The raw `user32` calls behind [`start_window_drag`] and [`window_command`].
///
/// Declared here rather than pulled in with a windows-sys feature: four
/// functions and six constants is less code than the dependency, and the
/// signatures are documented on MSDN (`ReleaseCapture`, `SendMessageW`,
/// `PostMessageW`).
#[cfg(windows)]
mod windows_chrome {
    use std::ffi::c_void;

    use gpui::Window;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::WindowCommand;

    const WM_NCLBUTTONDOWN: u32 = 0x00A1;
    const WM_SYSCOMMAND: u32 = 0x0112;
    const HTCAPTION: usize = 2;
    const SC_MINIMIZE: usize = 0xF020;
    const SC_MAXIMIZE: usize = 0xF030;
    const SC_RESTORE: usize = 0xF120;
    const SC_CLOSE: usize = 0xF060;

    #[link(name = "user32")]
    extern "system" {
        fn ReleaseCapture() -> i32;
        fn SendMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
    }

    /// The window's `HWND`. `None` if the platform refuses to hand out a
    /// handle, in which case the caller simply does nothing.
    fn hwnd(window: &Window) -> Option<*mut c_void> {
        // `Window::window_handle` is GPUI's own accessor; the raw handle is
        // the trait method, so it has to be named through the trait.
        let handle = HasWindowHandle::window_handle(window).ok()?;
        match handle.as_raw() {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as *mut c_void),
            _ => None,
        }
    }

    pub(super) fn start_drag(window: &Window) {
        let Some(hwnd) = hwnd(window) else {
            return;
        };
        // SAFETY: `hwnd` is the window's own handle for as long as `window`
        // lives, and both calls are the documented sequence for starting a
        // system move loop (`ReleaseCapture` drops the current capture so the
        // window can take it; `SendMessageW` of a non-client left-button-down
        // with `HTCAPTION` is what the OS reads as "the user grabbed the
        // titlebar"). `SendMessageW` does not return until the move ends.
        eprintln!("DRAG-DEBUG start_drag called");
        unsafe {
            ReleaseCapture();
            SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION, 0);
        }
        eprintln!("DRAG-DEBUG move loop returned");
    }

    pub(super) fn command(window: &Window, command: WindowCommand) {
        let Some(hwnd) = hwnd(window) else {
            return;
        };
        let sc = match command {
            WindowCommand::Minimize => SC_MINIMIZE,
            WindowCommand::Close => SC_CLOSE,
            WindowCommand::ToggleMaximize => {
                if window.is_maximized() {
                    SC_RESTORE
                } else {
                    SC_MAXIMIZE
                }
            }
        };
        // SAFETY: as above — the handle belongs to this window, and
        // `WM_SYSCOMMAND` is how a window asks the system to run one of its
        // own commands. Posted rather than sent so the command is handled
        // after the click returns, like a real caption button.
        unsafe {
            PostMessageW(hwnd, WM_SYSCOMMAND, sc, 0);
        }
    }
}

/// Width the app's caption buttons occupy at the right of the header row —
/// three 46px buttons, the metric Windows uses for its own. Headers that reach
/// the window's right edge keep their content clear of it.
pub const WINDOW_CONTROLS_W: f32 = 46. * 3.;

/// The window's titlebar configuration.
///
/// macOS gets the transparent titlebar the app draws its own controls into,
/// with the traffic lights moved onto the sidebar's 44px row; Windows gets one
/// too, because it paints its own caption buttons in that same row
/// (`draws_window_controls`) and a system caption above them would
/// double the header. That trade is deliberate: the app's buttons carry
/// `WindowControlArea` hit areas, so minimize/maximize/close, `Alt+Space`, and
/// double-click-to-maximize all still run the system's own commands, but the
/// Win11 snap-layouts flyout — which only a real system caption button opens —
/// is not available. Linux ignores [`TitlebarOptions`] (it decorates through
/// `WindowOptions::window_decorations`), so neither branch has to be right for
/// it.
#[cfg(target_os = "macos")]
pub fn titlebar_options() -> TitlebarOptions {
    TitlebarOptions {
        title: Some(SharedString::from("Orbit Pi")),
        // Transparent titlebar: the sidebar extends to the top and the native
        // traffic lights sit inside it.
        appears_transparent: true,
        // Center the lights in the 44px titlebar row so they share a line with
        // the window controls beside them.
        traffic_light_position: Some(point(px(12.), px(16.))),
    }
}

/// Transparent on Windows for the same reason as macOS — the app draws the
/// caption buttons itself — and `false` anywhere else, where nothing would.
#[cfg(not(target_os = "macos"))]
pub fn titlebar_options() -> TitlebarOptions {
    TitlebarOptions {
        title: Some(SharedString::from("Orbit Pi")),
        appears_transparent: draws_window_controls(),
        traffic_light_position: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn open_in_catalog_includes_rider_stable_and_eap() {
        let rider = OPEN_IN_CATALOG
            .iter()
            .find(|entry| entry.0 == "rider")
            .unwrap();
        assert_eq!(rider.1, "Rider");
        assert_eq!(rider.2, &["com.jetbrains.rider", "com.jetbrains.rider-EAP"]);
    }

    #[test]
    fn only_http_urls_are_opened() {
        assert!(is_safe_browser_url("https://claude.ai/oauth"));
        assert!(is_safe_browser_url("http://127.0.0.1:1455/callback"));
        assert!(!is_safe_browser_url("file:///etc/passwd"));
        assert!(!is_safe_browser_url("javascript:alert(1)"));
        assert!(!is_safe_browser_url("  "));
    }

    /// The Windows label distinguishes 10 from 11 off the build number, since
    /// both report 10.0 as their version.
    #[test]
    #[cfg(any(windows, test))]
    fn windows_label_follows_the_build_number() {
        let eleven = parse_windows_version("Microsoft Windows [Version 10.0.26100.9168]");
        assert_eq!(
            eleven.map(|(label, _)| label).as_deref(),
            Some("Windows 11 (build 26100)")
        );
        let ten = parse_windows_version("Microsoft Windows [Version 10.0.17763.1]");
        assert_eq!(
            ten.map(|(label, _)| label).as_deref(),
            Some("Windows 10 (build 17763)")
        );
        assert!(parse_windows_version("not a version").is_none());
    }

    #[test]
    #[cfg(any(target_os = "macos", test))]
    fn macos_label_keeps_major_and_minor() {
        let parsed = parse_macos_version("15.2.1\n");
        assert_eq!(
            parsed.map(|(label, _)| label).as_deref(),
            Some("macOS 15.2")
        );
        assert!(parse_macos_version("Sequoia").is_none());
    }

    #[test]
    #[cfg(any(all(unix, not(target_os = "macos")), test))]
    fn os_release_prefers_pretty_name() {
        let contents =
            "NAME=\"Ubuntu\"\nVERSION_ID=\"24.04\"\nPRETTY_NAME=\"Ubuntu 24.04.1 LTS\"\n";
        assert_eq!(
            parse_os_release(contents).as_deref(),
            Some("Ubuntu 24.04.1 LTS")
        );
        assert!(parse_os_release("ID=alpine\n").is_none());
    }

    /// The floors are what the bundles declare, so a host below one is called
    /// out rather than left looking supported.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_below_the_floor_is_unsupported() {
        let old = OsVersion {
            major: 12,
            minor: 7,
            build: 0,
        };
        assert!(unsupported_reason(old).is_some());
        let floor = OsVersion {
            major: 13,
            minor: 0,
            build: 0,
        };
        assert!(unsupported_reason(floor).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_below_the_floor_is_unsupported() {
        let old = OsVersion {
            major: 10,
            minor: 0,
            build: 17134,
        };
        assert!(unsupported_reason(old).is_some());
        let floor = OsVersion {
            major: 10,
            minor: 0,
            build: 17763,
        };
        assert!(unsupported_reason(floor).is_none());
    }

    /// The probe answers with a real label on the machine running the tests.
    #[test]
    fn host_label_is_never_empty() {
        let host = host();
        assert!(!host.label.trim().is_empty());
        assert!(!host.arch.trim().is_empty());
    }
}
