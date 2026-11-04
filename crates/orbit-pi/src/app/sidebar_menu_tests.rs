//! The sidebar's view menu.
//!
//! The popup is built *inside* `OrbitApp`'s own render, which means it runs while
//! the app entity is already leased. Reading the entity from there is a double
//! lease — gpui aborts (`double_lease_panic`), and clicking the `⋮` button crashes
//! the app. That is exactly what happened when the layout section started reading
//! `self.naming_layout`; this test draws the menu open so it cannot happen again.

use super::*;
use crate::theme::ThemeId;
use gpui::size;

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::OrbitAzure));
        cx.new(OrbitApp::new)
    })
}

#[gpui::test]
fn the_sidebar_view_menu_renders_while_open(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();

    // A first paint with the menu closed.
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
        app.clone().into_any_element()
    });

    // Open it: the next frame builds the popup, including the Layout section and
    // the saved-layout rows.
    app.update(cx, |app, cx| {
        app.sidebar_sort_menu = true;
        cx.notify();
    });
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
        app.clone().into_any_element()
    });

    // The naming variant is a different branch of the same popup — the one that
    // embeds the name field and both icon buttons.
    app.update(cx, |app, cx| {
        app.naming_layout = true;
        cx.notify();
    });
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
        app.clone().into_any_element()
    });
}
