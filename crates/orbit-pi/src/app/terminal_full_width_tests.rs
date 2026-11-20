//! The terminal's expand has to reach the app's layout: the panel flips its own
//! flag from its header, but only `OrbitApp::render` decides whether the shell
//! is a strip at the bottom of the chat or a page of its own.

use super::*;

/// Expanding from the panel's own header — not from an app method — is what the
/// user clicks, so the app has to hear about it through the observer and mount
/// the shell as the page on the next frame. The docks stay where they are: like
/// Git and Usage, a page is the column, not the window.
#[gpui::test]
fn expanding_the_terminal_from_its_own_header_fills_the_page(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            // The dock is gated on the runtime probe; the test host is not
            // asked to have pi installed just to lay the window out.
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.sidepane.update(cx, |pane, cx| pane.toggle(cx));
            app.terminal_panel
                .update(cx, |panel, cx| panel.new_tab(window, cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let docked = cx
        .debug_bounds("terminal-panel")
        .expect("the shell is on screen");

    // Exactly what the header chip runs. Nothing here reaches the app directly:
    // the observer is the only path from the panel's flag to the shell's render.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.terminal_panel
                .update(cx, |panel, cx| panel.toggle_full_width(cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let expanded = cx
        .debug_bounds("terminal-panel")
        .expect("the shell is still on screen");

    assert!(
        expanded.size.height > docked.size.height,
        "the expanded shell owns the page: {:?} → {:?}",
        docked,
        expanded
    );
    assert!(
        expanded.size.height > viewport.height * 0.6,
        "a page, not a strip: {:?} in a {:?} window",
        expanded.size.height,
        viewport.height
    );

    // Back in the docked strip: the app has to follow that direction too.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.terminal_panel
                .update(cx, |panel, cx| panel.toggle_full_width(cx));
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let restored = cx
        .debug_bounds("terminal-panel")
        .expect("the shell is still on screen");
    assert_eq!(
        restored.size.height, docked.size.height,
        "closing the page puts the same strip back"
    );
}
