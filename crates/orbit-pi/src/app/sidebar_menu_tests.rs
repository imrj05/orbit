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
fn session_card_waits_and_cancels_on_leave_or_menu(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let path = PathBuf::from("/sessions/hover.jsonl");
    app.update(cx, |app, cx| app.hover_session_card(path.clone(), true, cx));
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.session_card.is_none()));
    cx.executor().advance_clock(Duration::from_millis(499));
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.session_card.is_none()));
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(
        app.read_with(cx, |app, _| app.session_card.clone()),
        Some(path.clone())
    );
    app.update(cx, |app, cx| {
        app.hover_session_card(path.clone(), false, cx)
    });
    assert!(app.read_with(cx, |app, _| app.session_card.is_none()));
    app.update(cx, |app, cx| {
        app.sidebar_sort_menu = true;
        app.hover_session_card(path.clone(), true, cx);
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| app.session_card.is_none()));
}

#[gpui::test]
fn keyboard_navigation_clears_hover_card_and_pending_timer(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let path = PathBuf::from("/sessions/hover.jsonl");
    app.update(cx, |app, cx| app.hover_session_card(path.clone(), true, cx));
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| app.read(cx).session_card.clone()),
        Some(path.clone())
    );
    app.update_in(cx, |app, window, cx| {
        app.on_focus_sessions(&crate::FocusSessions, window, cx)
    });
    assert!(cx.update(|_, cx| app.read(cx).session_card.is_none()));

    app.update(cx, |app, cx| app.hover_session_card(path, true, cx));
    app.update_in(cx, |app, window, cx| {
        app.on_sidebar_next(&crate::SidebarNext, window, cx)
    });
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).session_card.is_none()));
}

#[gpui::test]
fn escape_closes_both_sidebar_option_menus(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    app.update(cx, |app, _| {
        app.sidebar_sort_menu = true;
        app.sidebar_show_menu = true;
    });
    app.update_in(cx, |app, window, cx| {
        app.on_sidebar_close(&crate::SidebarClose, window, cx)
    });
    assert!(cx.update(|_, cx| !app.read(cx).sidebar_sort_menu && !app.read(cx).sidebar_show_menu));
}

#[gpui::test]
fn show_submenu_allows_parent_choice_and_outside_dismissal(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::OrbitAzure)));
    let (app, cx) = cx.add_window_view(|_, cx| OrbitApp::new(cx));
    cx.simulate_resize(size(px(1240.), px(760.)));
    app.update(cx, |app, cx| {
        for dependency in &mut app.deps {
            dependency.installed = true;
        }
        app.setup_open = false;
        app.sidebar_visible = true;
        app.sidebar_group_by = SidebarGroupBy::Workspace;
        app.sidebar_sort_menu = true;
        app.sidebar_show_menu = true;
        cx.notify();
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
            app.clone()
        });
    };
    draw(cx);
    let choice = cx.debug_bounds("one_list").expect("parent choice").center();
    cx.simulate_mouse_move(choice, None, gpui::Modifiers::none());
    // Reopen without moving the pointer: mouse-down must work even if hover
    // has not already closed the flyout.
    app.update(cx, |app, cx| {
        app.sidebar_show_menu = true;
        cx.notify();
    });
    draw(cx);
    cx.simulate_mouse_down(choice, MouseButton::Left, gpui::Modifiers::none());
    assert!(cx.update(|_, cx| app.read(cx).sidebar_sort_menu));
    cx.simulate_mouse_up(choice, MouseButton::Left, gpui::Modifiers::none());
    assert_eq!(
        cx.update(|_, cx| app.read(cx).sidebar_group_by),
        SidebarGroupBy::OneList
    );
    assert!(cx.update(|_, cx| !app.read(cx).sidebar_sort_menu && !app.read(cx).sidebar_show_menu));

    app.update(cx, |app, cx| {
        app.sidebar_sort_menu = true;
        app.sidebar_show_menu = true;
        cx.notify();
    });
    draw(cx);
    let updated = cx
        .debug_bounds("sidebar-show-updated")
        .expect("submenu choice")
        .center();
    cx.simulate_mouse_move(updated, None, gpui::Modifiers::none());
    draw(cx);
    let was_visible = cx.update(|_, cx| app.read(cx).sidebar_show_updated);
    cx.simulate_click(updated, gpui::Modifiers::none());
    assert_eq!(
        cx.update(|_, cx| app.read(cx).sidebar_show_updated),
        !was_visible
    );
    assert!(cx.update(|_, cx| app.read(cx).sidebar_sort_menu && app.read(cx).sidebar_show_menu));

    draw(cx);
    let outside = point(px(1200.), px(700.));
    cx.simulate_mouse_move(outside, None, gpui::Modifiers::none());
    app.update(cx, |app, cx| {
        app.sidebar_show_menu = true;
        cx.notify();
    });
    draw(cx);
    cx.simulate_mouse_down(outside, MouseButton::Left, gpui::Modifiers::none());
    assert!(cx.update(|_, cx| !app.read(cx).sidebar_sort_menu && !app.read(cx).sidebar_show_menu));
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

    // The flyout is painted within the already deferred parent menu.
    app.update(cx, |app, cx| {
        app.sidebar_show_menu = true;
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
