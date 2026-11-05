//! The new-task page's geometry, pinned to the real widget tree.
//!
//! The page is not a second layout — it is the chat column with an empty
//! transcript — so these tests are about the *column*: the composer's width and
//! place on the page, the folder row over it, where its picker opens, and what
//! the first send does to all three. The transition's arithmetic lives in
//! `new_task`'s unit tests; what is checked here is that the widgets are wired to
//! it.

use super::*;
use crate::app::new_task;
use crate::theme::{Theme, ThemeId};
use std::time::{Duration, Instant};

/// An app on the new-task page with every dependency satisfied (otherwise the
/// setup page owns the column) and no sidebar (so the main column is the
/// viewport, which keeps the centring arithmetic readable).
fn new_task_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    cx.update(|cx| {
        app.update(cx, |app, _cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            app.sidebar_visible = false;
            app.sidebar_width = px(0.);
            app.current_workspace = Some(std::env::temp_dir());
            assert!(app.transcript.is_empty(), "the page starts empty");
        });
    });
    app
}

fn draw(cx: &mut gpui::VisualTestContext, app: &Entity<OrbitApp>, w: f32, h: f32) {
    let viewport = gpui::size(px(w), px(h));
    let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
}

/// The page: one composer, narrower than a session's, centered, with the folder
/// row sitting directly over it and room below it that the session will not have.
#[gpui::test]
fn the_new_task_composer_is_narrow_centered_and_under_its_folder_row(
    cx: &mut gpui::TestAppContext,
) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app, 1200., 760.);

    let box_ = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    let row = cx
        .debug_bounds("new-task-folder")
        .expect("the folder row laid out");
    let work_in = cx
        .debug_bounds("new-task-work-in-label")
        .expect("the workspace context label laid out");
    let folder_name = cx
        .debug_bounds("new-task-folder-name")
        .expect("the folder name laid out");
    let slot = cx
        .debug_bounds("transcript-slot")
        .expect("the transcript slot laid out");
    let gap = cx.debug_bounds("new-task-gap").expect("the gap laid out");

    // Narrow: the page's own ceiling, not the session's 960.
    assert_eq!(
        box_.size.width,
        px(new_task::COMPOSER_MAX_W),
        "the new-task composer is the page's column, not the session's"
    );
    // Centered in the page: equal room either side, because the column is
    // centered rather than pushed by a sibling.
    let page_center = px(600.);
    assert!(
        (box_.center().x - page_center).abs() <= px(1.),
        "composer is off center: {} vs {page_center}",
        box_.center().x
    );
    assert_eq!(row.left(), box_.left(), "the row aligns to the composer");
    assert!(
        (work_in.center().y - row.center().y).abs() <= px(1.)
            && (folder_name.center().y - row.center().y).abs() <= px(1.),
        "the context label and folder name share the chip's centreline"
    );
    assert!(
        folder_name.left() >= work_in.right(),
        "the quiet context label precedes the folder name without overlap"
    );
    assert!(
        row.bottom() <= box_.top(),
        "the row sits above the composer: {row:?} vs {box_:?}"
    );

    // Vertically the column is centered with the optical bias: the composer's
    // middle is above the page's, but not by much.
    let body_top = px(view::TOP_BAR_H);
    let body_center = px(f32::from(body_top) + (760. - f32::from(body_top)) / 2.);
    let composer_center = box_.center().y;
    assert!(
        composer_center < body_center,
        "the composer sits above the geometric middle: {composer_center} vs {body_center}"
    );
    assert!(
        f32::from(body_center - composer_center) < 0.14 * (760. - f32::from(body_top)),
        "and only slightly: {} away",
        f32::from(body_center - composer_center)
    );

    // The slot above and the gap below are the free space, in the page's 44/56
    // split. The weights position the *group* — folder row, composer, status
    // bar — so the split is measured from the row's top edge, not the box's.
    let above = f32::from(row.top() - slot.top());
    let below = f32::from(gap.size.height) + f32::from(px(760.) - gap.bottom());
    assert!(slot.size.height > px(0.), "the slot holds the room above");
    assert!(
        (above / (above + below) - new_task::CENTER_SHARE).abs() < 0.02,
        "the page splits its free space {above} / {below}"
    );
}

/// The folder question is asked once. The status bar's Work-in chip would be a
/// second control for the same setting two rows below the first, so it stays out
/// of the new-task page and comes back with the session.
#[gpui::test]
fn the_folder_row_is_the_only_folder_control_on_the_page(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app, 1100., 720.);

    assert!(
        cx.debug_bounds("new-task-folder").is_some(),
        "the page's own row"
    );
    assert!(
        cx.debug_bounds("status-work-in").is_none(),
        "the status bar must not repeat the question"
    );
    assert!(
        cx.debug_bounds("status-workspace").is_none(),
        "nor the plain folder chip outside a repository"
    );

    // Once there is a conversation, the row is gone and the status bar owns it.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.transcript.append_user_message("hello", Vec::new());
            app.new_task_settle_at = Some(Instant::now() - Duration::from_secs(2));
            cx.notify();
        });
    });
    draw(cx, &app, 1100., 720.);
    assert!(
        cx.debug_bounds("new-task-folder").is_none(),
        "the session page has no folder row"
    );
    assert!(
        cx.debug_bounds("new-task-gap").is_none(),
        "and no room under the composer"
    );
    assert!(
        cx.debug_bounds("status-workspace").is_some() ^ cx.debug_bounds("status-work-in").is_some(),
        "the settled status row owns exactly one folder control"
    );
}

/// The status row waits for the folder row's dock hand-off. In particular, the
/// first send must never paint two answers to "where does this task run?".
#[gpui::test]
fn the_settling_folder_row_is_the_only_location_control(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app, 1100., 720.);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.transcript.append_user_message("hello", Vec::new());
            app.new_task_settle_at = Some(Instant::now());
            cx.notify();
        });
    });
    draw(cx, &app, 1100., 720.);

    assert!(cx.debug_bounds("new-task-folder").is_some());
    assert!(
        cx.debug_bounds("status-work-in").is_none(),
        "the session chip must not appear beside the settling folder row"
    );
    assert!(
        cx.debug_bounds("status-workspace").is_none(),
        "nor may the plain-folder session chip appear mid-settle"
    );
}

/// The picker opens directly under its row, flush with it, and stays inside the
/// window — the row sits in the middle of the page, so a menu that opened upward
/// would run out of room at the minimum window height and be snapped over the
/// composer anyway.
#[gpui::test]
fn the_folder_picker_opens_under_its_row(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app, 1100., 720.);
    let row = cx
        .debug_bounds("new-task-folder")
        .expect("the row laid out");

    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.toggle_workspace_picker(window, cx));
    });
    draw(cx, &app, 1100., 720.);

    let popup = cx
        .debug_bounds("workspace-picker-popup")
        .expect("the picker laid out");
    assert!(
        popup.top() >= row.bottom(),
        "the picker opens under the row it belongs to: popup {popup:?}, row {row:?}"
    );
    assert!(
        popup.left() >= row.left() - px(1.) && popup.left() <= row.left() + px(1.),
        "and stays flush with its row's left edge"
    );
    assert!(
        popup.bottom() <= px(720.),
        "and inside the window: {popup:?}"
    );
}

/// Reduce Motion paints the session frame: no transition is mounted, so the very
/// first frame after the send is already docked and full width.
#[gpui::test]
fn reduce_motion_docks_the_composer_on_the_frame_it_sends(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        cx.set_global(theme);
    });
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.transcript.append_user_message("hello", Vec::new());
            app.new_task_settle_at = Some(Instant::now());
            cx.notify();
        });
    });
    draw(cx, &app, 1200., 760.);

    let box_ = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    assert_eq!(
        box_.size.width,
        px(CONTENT_MAX_W),
        "the session's column width, painted at once"
    );
    assert!(
        box_.bottom() >= px(760. - 64.),
        "docked to the bottom of the page the moment there is a session: {box_:?}"
    );
    assert!(
        cx.debug_bounds("new-task-gap").is_none(),
        "Reduce Motion also drops the spacer the page would have carried"
    );
}

/// The page narrows before it overflows: below the ceiling the column is the
/// window minus its padding, and the composer is never wider than the page.
#[gpui::test]
fn a_narrow_window_narrows_the_page_composer(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    // The column's width is read off the window's viewport, so the window — not
    // just the draw space — has to be the narrow one here.
    cx.simulate_resize(gpui::size(px(520.), px(700.)));
    draw(cx, &app, 520., 700.);

    let box_ = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    assert_eq!(
        box_.size.width,
        px(520. - 2. * new_task::PAGE_PAD_X),
        "the composer takes the window minus its page padding"
    );
    assert!(
        box_.right() <= px(520.),
        "and never runs past the window: {box_:?}"
    );
}

/// The folder chip is a keyboard button, not a pointer-only row. Enter and
/// Space open the same picker (whose current row carries the full path), and
/// Escape returns focus to the trigger instead of jumping into the composer.
#[gpui::test]
fn the_folder_chip_opens_from_the_keyboard_and_regains_focus_on_escape(
    cx: &mut gpui::TestAppContext,
) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| crate::bind_keys(cx));
    draw(cx, &app, 1100., 720.);

    cx.update(|window, cx| {
        let focus = app.read(cx).new_task_folder_focus.clone();
        window.focus(&focus);
    });
    assert!(cx.update(|window, cx| { app.read(cx).new_task_folder_focus.is_focused(window) }));
    // Rebuild the dispatch tree around the newly focused chip before sending
    // keys, matching GPUI's frame-driven focus dispatch.
    draw(cx, &app, 1100., 720.);

    cx.simulate_keystrokes("enter");
    assert!(cx.update(|_, cx| app.read(cx).workspace_picker.is_some()));
    assert!(cx.update(|window, cx| {
        app.read(cx)
            .workspace_picker
            .as_ref()
            .expect("picker opened")
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }));
    draw(cx, &app, 1100., 720.);
    assert!(
        cx.debug_bounds("workspace-picker-popup").is_some(),
        "the keyboard-opened picker exposes the current folder's full path"
    );

    cx.simulate_keystrokes("escape");
    assert!(cx.update(|_, cx| app.read(cx).workspace_picker.is_none()));
    assert!(cx.update(|window, cx| { app.read(cx).new_task_folder_focus.is_focused(window) }));
    draw(cx, &app, 1100., 720.);

    cx.simulate_keystrokes("space");
    assert!(
        cx.update(|_, cx| app.read(cx).workspace_picker.is_some()),
        "Space is the second native button activation key"
    );
}

/// At the minimum window, largest UI text and comfortable density, the picker
/// shortens its scrolling list rather than snapping over its focused trigger.
/// German exercises the wider keyboard legend and action labels at the same time.
#[gpui::test]
fn the_folder_picker_fits_the_minimum_window_at_maximum_ui_scale(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.ui_font_size = 20.;
        theme.ui.spacing_density = 120;
        theme.ui.language = crate::theme::Language::German;
        // Assert the resting geometry, not the popover's 8px entrance travel.
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        cx.set_global(theme);
    });
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(960.), px(640.)));
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.current_workspace =
                Some(std::env::temp_dir().join(
                    "a-very-long-workspace-name-that-must-truncate-inside-the-composer-column",
                ));
        });
    });
    draw(cx, &app, 960., 640.);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.toggle_workspace_picker(window, cx));
    });
    draw(cx, &app, 960., 640.);

    let row = cx
        .debug_bounds("new-task-folder")
        .expect("the folder row laid out");
    let popup = cx
        .debug_bounds("workspace-picker-popup")
        .expect("the picker laid out");
    let composer = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    assert!(
        row.right() <= composer.right(),
        "the context label and long folder name stay inside the composer column: {row:?} vs {composer:?}"
    );
    assert!(
        popup.top() >= row.bottom(),
        "the popup must not snap over its trigger: popup {popup:?}, row {row:?}"
    );
    assert!(
        popup.left() >= px(0.) && popup.right() <= px(960.),
        "the popup fits horizontally: {popup:?}"
    );
    assert!(
        popup.bottom() <= px(640.),
        "the popup fits vertically: {popup:?}"
    );
}

/// The popover's definite width comes from the chat column, not just the
/// window minus the sidebar. Explorer (and Review through the same helper)
/// must be excluded before that width is chosen.
#[gpui::test]
fn the_folder_picker_width_excludes_visible_docks(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(960.), px(640.)));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.sidebar_visible = true;
            app.sidebar_width = px(248.);
            app.project_panel.update(cx, |panel, cx| {
                panel.toggle(cx);
                panel.set_width(px(300.), cx);
            });
            app.toggle_workspace_picker(window, cx);
        });
    });
    draw(cx, &app, 960., 640.);

    let popup = cx
        .debug_bounds("workspace-picker-popup")
        .expect("the picker laid out");
    let main_width = 960. - 248. - 300.;
    assert_eq!(
        popup.size.width,
        px(crate::workspace_picker::width_for_window(main_width)),
        "the picker must use the dock-aware chat-column width"
    );
}

/// Focus is a visible border change, never a layout-changing outline. A file
/// drag keeps its stronger accent signal whether or not the input is focused.
#[test]
fn the_composer_border_distinguishes_rest_focus_and_file_drag() {
    let theme = Theme::for_id(ThemeId::Orbit);
    assert_eq!(
        OrbitApp::composer_border_color(&theme, false, false),
        theme.border
    );
    assert_eq!(
        OrbitApp::composer_border_color(&theme, false, true),
        theme.focus_ring
    );
    assert_eq!(
        OrbitApp::composer_border_color(&theme, true, false),
        theme.accent
    );
    assert_eq!(
        OrbitApp::composer_border_color(&theme, true, true),
        theme.accent
    );
}

/// The empty page does not spend its footer on a zero-valued context meter.
/// A real measurement may still appear immediately; nothing about the page
/// hides useful data once pi supplies it.
#[gpui::test]
fn the_new_task_context_meter_waits_for_real_data(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app, 1100., 720.);
    assert!(
        cx.debug_bounds("composer-context-indicator").is_none(),
        "an empty task has no context to measure"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.context = Some(orbit_rpc::ContextUsage {
                tokens: Some(64_000),
                context_window: 128_000,
                percent: Some(50.0),
            });
            cx.notify();
        });
    });
    draw(cx, &app, 1100., 720.);
    assert!(
        cx.debug_bounds("composer-context-indicator").is_some(),
        "real context data is never hidden just because the page is new"
    );
}
