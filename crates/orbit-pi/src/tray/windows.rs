//! The Windows notification-area icon: `Shell_NotifyIconW` on a hidden
//! message-only window. A left click shows Orbit; a right click opens the
//! Show Orbit / New task / Quit menu. GPUI's message loop (`GetMessageW` for
//! every window on the thread) dispatches this window's messages too, so no
//! thread of its own is needed.
//!
//! Explorer forgets every icon when it restarts and broadcasts
//! `TaskbarCreated`; the icon is added again then.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetCursorPos, IsIconic, LoadIconW, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, ShowWindow, TrackPopupMenu, HICON, HWND_MESSAGE, IDI_APPLICATION,
    MF_SEPARATOR, MF_STRING, SW_HIDE, SW_RESTORE, SW_SHOW, TPM_BOTTOMALIGN, TPM_RIGHTBUTTON,
    WM_APP, WM_COMMAND, WM_CONTEXTMENU, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASSW,
};

use super::{push, TrayEvent, TrayLabels};

/// The callback message the icon posts to the helper window.
const WM_TRAY: u32 = WM_APP + 1;
/// The icon's id within the helper window (one icon).
const ICON_ID: u32 = 1;
/// Menu command ids.
const CMD_SHOW: usize = 1;
const CMD_NEW_TASK: usize = 2;
const CMD_QUIT: usize = 3;
/// The application icon `build.rs` embeds (`1 ICON "icon.ico"`).
const APP_ICON_RESOURCE: usize = 1;

/// `RegisterWindowMessageW("TaskbarCreated")`, read by the window procedure.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

struct Icon {
    hwnd: HWND,
    labels: TrayLabels,
}

thread_local! {
    static ICON: RefCell<Option<Icon>> = const { RefCell::new(None) };
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(super) fn icon_up() -> bool {
    ICON.with(|icon| icon.borrow().is_some())
}

pub(super) fn set_icon(labels: Option<&TrayLabels>) -> bool {
    ICON.with(|slot| {
        let mut slot = slot.borrow_mut();
        match labels {
            None => {
                if let Some(icon) = slot.take() {
                    // SAFETY: `icon.hwnd` is the helper window this thread
                    // created; deleting an icon that is already gone is a
                    // harmless failure.
                    unsafe {
                        notify(NIM_DELETE, icon.hwnd, &icon.labels.tooltip);
                        DestroyWindow(icon.hwnd);
                    }
                }
                false
            }
            Some(labels) => {
                if let Some(icon) = slot.as_mut() {
                    if &icon.labels != labels {
                        icon.labels = labels.clone();
                        // SAFETY: as above; a modify only updates the tooltip.
                        unsafe { notify(NIM_MODIFY, icon.hwnd, &labels.tooltip) };
                    }
                    return true;
                }
                // SAFETY: plain Win32 calls on this (the UI) thread; the
                // window and icon are torn down by the `None` arm.
                match unsafe { create(&labels.tooltip) } {
                    Some(hwnd) => {
                        *slot = Some(Icon {
                            hwnd,
                            labels: labels.clone(),
                        });
                        true
                    }
                    None => false,
                }
            }
        }
    })
}

/// Register the helper class (once), create the message-only window and add
/// the icon. `None` when any step fails, so the caller can fall back to
/// quitting on close.
unsafe fn create(tooltip: &str) -> Option<HWND> {
    let instance = GetModuleHandleW(std::ptr::null());
    let class = wide("OrbitTrayWindow");
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: std::ptr::null_mut(),
        hCursor: std::ptr::null_mut(),
        hbrBackground: std::ptr::null_mut(),
        lpszMenuName: std::ptr::null(),
        lpszClassName: class.as_ptr(),
    };
    // Registering twice fails with "class already exists", which is fine:
    // the window below is created from whichever registration won.
    RegisterClassW(&wc);
    let taskbar = wide("TaskbarCreated");
    TASKBAR_CREATED.store(RegisterWindowMessageW(taskbar.as_ptr()), Ordering::Relaxed);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        class.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        HWND_MESSAGE,
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        return None;
    }
    if notify(NIM_ADD, hwnd, tooltip) == 0 {
        DestroyWindow(hwnd);
        return None;
    }
    Some(hwnd)
}

/// Add, modify or delete the icon on `hwnd`; returns the `BOOL` result.
unsafe fn notify(message: u32, hwnd: HWND, tooltip: &str) -> i32 {
    // SAFETY: `NOTIFYICONDATAW` is plain data; all-zero is a valid start.
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = ICON_ID;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    data.hIcon = app_icon();
    // The tip holds 127 characters plus the terminator.
    for (slot, unit) in data.szTip.iter_mut().zip(tooltip.encode_utf16().take(127)) {
        *slot = unit;
    }
    Shell_NotifyIconW(message, &data)
}

/// The embedded application icon, or the stock one when it is missing.
unsafe fn app_icon() -> HICON {
    let instance = GetModuleHandleW(std::ptr::null());
    let icon = LoadIconW(instance, APP_ICON_RESOURCE as *const u16);
    if icon.is_null() {
        LoadIconW(std::ptr::null_mut(), IDI_APPLICATION)
    } else {
        icon
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_TRAY => {
            match (lparam as u32) & 0xFFFF {
                WM_LBUTTONUP => push(TrayEvent::Show),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            match wparam & 0xFFFF {
                CMD_SHOW => push(TrayEvent::Show),
                CMD_NEW_TASK => push(TrayEvent::NewTask),
                CMD_QUIT => push(TrayEvent::Quit),
                _ => {}
            }
            0
        }
        _ if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) => {
            // Explorer restarted and dropped every icon: add ours back.
            let tooltip = ICON.with(|icon| {
                icon.try_borrow()
                    .ok()
                    .and_then(|icon| icon.as_ref().map(|icon| icon.labels.tooltip.clone()))
            });
            if let Some(tooltip) = tooltip {
                notify(NIM_ADD, hwnd, &tooltip);
            }
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// The right-click menu, at the pointer. `SetForegroundWindow` before and
/// the `WM_NULL` after are the documented dance that makes the menu close
/// when the user clicks elsewhere.
unsafe fn show_menu(hwnd: HWND) {
    let labels = ICON.with(|icon| {
        icon.try_borrow()
            .ok()
            .and_then(|icon| icon.as_ref().map(|icon| icon.labels.clone()))
    });
    let Some(labels) = labels else {
        return;
    };
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let show = wide(&labels.show);
    let new_task = wide(&labels.new_task);
    let quit = wide(&labels.quit);
    AppendMenuW(menu, MF_STRING, CMD_SHOW, show.as_ptr());
    AppendMenuW(menu, MF_STRING, CMD_NEW_TASK, new_task.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, CMD_QUIT, quit.as_ptr());
    let mut at = POINT { x: 0, y: 0 };
    GetCursorPos(&mut at);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
        at.x,
        at.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
}

/// Hide the main window; it stays alive with everything it renders.
pub(super) fn hide_window(hwnd: *mut c_void) {
    // SAFETY: `hwnd` is GPUI's live main window handle.
    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
}

/// Show the main window again and bring it to the front, restoring it if it
/// was minimized before it was hidden.
pub(super) fn show_window(hwnd: *mut c_void) {
    let hwnd = hwnd as HWND;
    // SAFETY: as above.
    unsafe {
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        } else {
            ShowWindow(hwnd, SW_SHOW);
        }
        SetForegroundWindow(hwnd);
    }
}
