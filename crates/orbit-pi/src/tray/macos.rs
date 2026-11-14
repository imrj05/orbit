//! The macOS menu-bar icon: an `NSStatusItem` with the app icon and a menu
//! of Show Orbit / New task / Quit. Clicking the icon opens the menu, the
//! menu-bar convention; each item pushes a [`TrayEvent`].

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSCopying, NSSize, NSString};

use super::{push, TrayEvent, TrayLabels};

define_class!(
    // SAFETY: `NSObject` has no subclassing requirements, the class holds no
    // ivars, and AppKit sends menu actions on the main thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "OrbitTrayTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(showOrbit:))]
        fn show_orbit(&self, _sender: Option<&AnyObject>) {
            push(TrayEvent::Show);
        }

        #[unsafe(method(newTask:))]
        fn new_task(&self, _sender: Option<&AnyObject>) {
            push(TrayEvent::NewTask);
        }

        #[unsafe(method(quitOrbit:))]
        fn quit_orbit(&self, _sender: Option<&AnyObject>) {
            push(TrayEvent::Quit);
        }
    }
);

impl Target {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: `NSObject`'s `init` has no requirements; the object was
        // just allocated with this class and its (empty) ivars.
        unsafe { msg_send![super(this), init] }
    }
}

/// The live icon. The menu items hold their target weakly, so the target is
/// kept here for as long as the icon is up.
struct Icon {
    item: Retained<NSStatusItem>,
    _target: Retained<Target>,
    labels: TrayLabels,
}

thread_local! {
    static ICON: RefCell<Option<Icon>> = const { RefCell::new(None) };
}

pub(super) fn icon_up() -> bool {
    ICON.with(|icon| icon.borrow().is_some())
}

pub(super) fn set_icon(labels: Option<&TrayLabels>) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return icon_up();
    };
    ICON.with(|slot| {
        let mut slot = slot.borrow_mut();
        match labels {
            None => {
                if let Some(icon) = slot.take() {
                    NSStatusBar::systemStatusBar().removeStatusItem(&icon.item);
                }
                false
            }
            Some(labels) => {
                if slot.as_ref().is_some_and(|icon| &icon.labels == labels) {
                    return true;
                }
                if let Some(icon) = slot.as_ref() {
                    // Same icon, new language: rebuild only the menu.
                    let target = icon._target.clone();
                    icon.item.setMenu(Some(&menu(labels, &target, mtm)));
                    set_tooltip(&icon.item, &labels.tooltip, mtm);
                    let item = icon.item.clone();
                    *slot = Some(Icon {
                        item,
                        _target: target,
                        labels: labels.clone(),
                    });
                    return true;
                }
                *slot = Some(create(labels, mtm));
                true
            }
        }
    })
}

fn create(labels: &TrayLabels, mtm: MainThreadMarker) -> Icon {
    let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = item.button(mtm) {
        // The app icon, copied so resizing it cannot touch the Dock's.
        if let Some(image) = NSApplication::sharedApplication(mtm).applicationIconImage() {
            let image = image.copy();
            image.setSize(NSSize::new(18., 18.));
            button.setImage(Some(&image));
        } else {
            button.setTitle(&NSString::from_str("Orbit"));
        }
    }
    set_tooltip(&item, &labels.tooltip, mtm);
    let target = Target::new(mtm);
    item.setMenu(Some(&menu(labels, &target, mtm)));
    Icon {
        item,
        _target: target,
        labels: labels.clone(),
    }
}

fn set_tooltip(item: &NSStatusItem, tooltip: &str, mtm: MainThreadMarker) {
    if let Some(button) = item.button(mtm) {
        button.setToolTip(Some(&NSString::from_str(tooltip)));
    }
}

fn menu(labels: &TrayLabels, target: &Target, mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    let entries = [
        (labels.show.as_str(), sel!(showOrbit:)),
        (labels.new_task.as_str(), sel!(newTask:)),
    ];
    for (title, action) in entries {
        menu.addItem(&menu_item(title, action, target, mtm));
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    menu.addItem(&menu_item(&labels.quit, sel!(quitOrbit:), target, mtm));
    menu
}

fn menu_item(
    title: &str,
    action: objc2::runtime::Sel,
    target: &Target,
    mtm: MainThreadMarker,
) -> Retained<NSMenuItem> {
    // SAFETY: `action` names a method `Target` implements with the
    // `(sender)` signature AppKit calls menu actions with, and the target
    // outlives the item (it is kept in `ICON` with the status item).
    unsafe {
        let item = NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(""),
        );
        item.setTarget(Some(target));
        item
    }
}
