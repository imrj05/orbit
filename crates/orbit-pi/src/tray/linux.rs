//! The Linux tray icon: a StatusNotifierItem through `ksni`, with a DBusMenu
//! of Show Orbit / New task / Quit. Activating the icon (usually a left
//! click) shows Orbit. The service runs on `ksni`'s own thread; clicks only
//! push a [`TrayEvent`].
//!
//! Spawning fails when no StatusNotifierWatcher is running (a bare window
//! manager, GNOME without the AppIndicator extension). The icon is then
//! "not up" and closing the window quits, so it can never vanish with no
//! way back.

use std::sync::{Mutex, OnceLock};

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::StandardItem;
use ksni::{Icon, MenuItem, ToolTip, Tray};

use super::{push, TrayEvent, TrayLabels};

/// The installed icon's theme name (`scripts/bundle-linux.sh`).
const ICON_NAME: &str = "dev.orbit.pi";

struct OrbitTray {
    labels: TrayLabels,
}

impl Tray for OrbitTray {
    fn id(&self) -> String {
        ICON_NAME.into()
    }

    fn title(&self) -> String {
        self.labels.tooltip.clone()
    }

    fn icon_name(&self) -> String {
        ICON_NAME.into()
    }

    /// The bundled icon as pixels too, for a build run outside an install
    /// (no `dev.orbit.pi` in the icon theme).
    fn icon_pixmap(&self) -> Vec<Icon> {
        pixmap().to_vec()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.labels.tooltip.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        push(TrayEvent::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        // DBusMenu treats `_` as an access-key marker; double it so a label
        // shows the character itself.
        let label = |text: &str| text.replace('_', "__");
        vec![
            StandardItem {
                label: label(&self.labels.show),
                activate: Box::new(|_: &mut Self| push(TrayEvent::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: label(&self.labels.new_task),
                activate: Box::new(|_: &mut Self| push(TrayEvent::NewTask)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: label(&self.labels.quit),
                activate: Box::new(|_: &mut Self| push(TrayEvent::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// The app icon at the sizes panels ask for, decoded once.
fn pixmap() -> &'static [Icon] {
    static PIXMAP: OnceLock<Vec<Icon>> = OnceLock::new();
    PIXMAP.get_or_init(|| {
        let Ok(image) =
            image::load_from_memory(include_bytes!("../../../../assets/icons/icon.png"))
        else {
            return Vec::new();
        };
        [32u32, 64]
            .into_iter()
            .map(|size| {
                let rgba = image
                    .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
                    .to_rgba8();
                // SNI wants ARGB32 in network byte order.
                let data = rgba
                    .pixels()
                    .flat_map(|p| [p[3], p[0], p[1], p[2]])
                    .collect();
                Icon {
                    width: size as i32,
                    height: size as i32,
                    data,
                }
            })
            .collect()
    })
}

static HANDLE: Mutex<Option<Handle<OrbitTray>>> = Mutex::new(None);

pub(super) fn icon_up() -> bool {
    HANDLE
        .lock()
        .map(|handle| handle.as_ref().is_some_and(|handle| !handle.is_closed()))
        .unwrap_or(false)
}

pub(super) fn set_icon(labels: Option<&TrayLabels>) -> bool {
    let Ok(mut slot) = HANDLE.lock() else {
        return false;
    };
    if slot.as_ref().is_some_and(Handle::is_closed) {
        *slot = None;
    }
    match labels {
        None => {
            if let Some(handle) = slot.take() {
                handle.shutdown();
            }
            false
        }
        Some(labels) => {
            if let Some(handle) = slot.as_ref() {
                let labels = labels.clone();
                handle.update(move |tray| tray.labels = labels);
                return true;
            }
            match (OrbitTray {
                labels: labels.clone(),
            })
            .spawn()
            {
                Ok(handle) => {
                    *slot = Some(handle);
                    true
                }
                Err(_) => false,
            }
        }
    }
}
