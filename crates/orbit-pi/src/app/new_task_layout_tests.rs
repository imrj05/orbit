//! The new-task page's geometry, pinned to the real widget tree.
//!
//! The page is not a second layout — it is the chat column with an empty
//! transcript — so these tests are about the *column*: the composer's width and
//! place on the page, its context header, where the picker opens, and what
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

/// The page: one centered composer, narrower than a session's, with its
/// context header inside and room below that the session will not have.
#[gpui::test]
fn the_new_task_composer_is_narrow_centered_and_contains_its_header(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app, 1200., 760.);

    let box_ = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    let row = cx
        .debug_bounds("new-task-folder")
        .expect("the folder row laid out");
    let header = cx
        .debug_bounds("new-task-context-header")
        .expect("the composer header laid out");
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
    assert!(
        header.left() >= box_.left()
            && header.right() <= box_.right()
            && header.top() >= box_.top()
            && header.bottom() < box_.bottom(),
        "the header is inside the composer: {header:?} vs {box_:?}"
    );
    assert!(
        (folder_name.center().y - row.center().y).abs() <= px(1.),
        "the folder name shares the chip's centreline"
    );
    assert!(row.left() >= header.left() && row.bottom() <= header.bottom());

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
    // split. The header is inside the composer, so the split is measured from
    // the box's top edge.
    let above = f32::from(box_.top() - slot.top());
    let below = f32::from(gap.size.height) + f32::from(px(760.) - gap.bottom());
    assert!(slot.size.height > px(0.), "the slot holds the room above");
    assert!(
        (above / (above + below) - new_task::CENTER_SHARE).abs() < 0.02,
        "the page splits its free space {above} / {below}"
    );
}

/// Folder, branch and working directory share one line inside the composer.
/// After the dock, the branch returns to the status row rather than appearing
/// in both places.
#[gpui::test]
fn the_branch_and_working_directory_share_the_composer_header(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.branch = Some(crate::app::BranchStatus {
                name: "refector/new-session-composer".into(),
                ahead_behind: Some((1, 2)),
            });
            app.worktree_repo_root = app.current_workspace.clone();
            cx.notify();
        });
    });
    let cx = cx.add_empty_window();
    draw(cx, &app, 1100., 720.);

    let composer = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    let branch = cx
        .debug_bounds("branch-chip")
        .expect("the branch chip laid out");
    let folder = cx.debug_bounds("new-task-folder").expect("folder control");
    let location = cx
        .debug_bounds("status-work-in")
        .expect("working directory");
    let header = cx.debug_bounds("new-task-context-header").expect("header");
    assert!(folder.right() <= branch.left());
    assert!(branch.right() <= location.left());
    assert!((folder.center().y - branch.center().y).abs() <= px(1.));
    assert!((folder.center().y - location.center().y).abs() <= px(1.));
    assert!(location.right() <= header.right() && location.right() > header.center().x);
    assert!(header.top() >= composer.top() && header.bottom() < composer.bottom());

    // After the first send, the context row is gone and the status row takes
    // the branch back — never both at once.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.transcript.append_user_message("hello", Vec::new());
            app.new_task_settle_at = Some(Instant::now() - Duration::from_secs(2));
            cx.notify();
        });
    });
    draw(cx, &app, 1100., 720.);
    let composer = cx
        .debug_bounds("composer-box")
        .expect("the composer laid out");
    let branch = cx
        .debug_bounds("branch-chip")
        .expect("the status row's branch chip laid out");
    assert!(
        branch.top() > composer.bottom(),
        "the branch moved below the composer with the session"
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

/// Sending the first prompt must actually move the composer from the new-task
/// position to the bottom, rather than just arming an animation in isolation.
#[cfg(unix)]
#[gpui::test]
fn first_send_slides_the_composer_to_the_bottom(cx: &mut gpui::TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;

    let app = new_task_app(cx);
    cx.update(|cx| {
        cx.global_mut::<Theme>().ui.motion = crate::theme::MotionPreference::Full;
    });
    let fake = tempfile::tempdir().expect("fake pi directory");
    let bin = fake.path().join("pi");
    std::fs::write(&bin, "#!/bin/sh\nexec cat >/dev/null\n").expect("fake pi");
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(1200.), px(760.)));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let client = PiClient::spawn_with_bin(bin.to_str().unwrap(), fake.path(), None)
                .expect("spawn fake pi");
            app.adopt_client(client);
            app.input
                .update(cx, |input, cx| input.set_text("hello", cx));
        });
    });
    draw(cx, &app, 1200., 760.);
    let before = cx
        .debug_bounds("composer-box")
        .expect("composer before send");
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.submit_as("hello".into(), SendMode::FollowUp, cx);
            assert!(app.new_task_settle_pending, "send arms the dock");
            assert!(
                app.new_task_settle_at.is_none(),
                "the clock starts on the first frame"
            );
        });
    });
    // The first frame may arrive late while the runtime or UI is busy. The
    // animation must start when it is actually drawn, not expire offscreen.
    std::thread::sleep(Duration::from_millis(320));
    draw(cx, &app, 1200., 760.);
    let start = cx.debug_bounds("composer-box").expect("first frame");
    std::thread::sleep(Duration::from_millis(80));
    draw(cx, &app, 1200., 760.);
    let middle = cx.debug_bounds("composer-box").expect("middle frame");
    let middle_age = cx.update(|_, cx| app.read(cx).new_task_settle_at.unwrap().elapsed());
    std::thread::sleep(Duration::from_millis(300));
    draw(cx, &app, 1200., 760.);
    let end = cx.debug_bounds("composer-box").expect("docked frame");
    assert!(
        (start.top() - before.top()).abs() <= px(8.),
        "the slide starts from the centered page: {before:?} -> {start:?}"
    );
    assert!(end.top() > start.top(), "the composer docks at the bottom");
    // A heavily loaded test runner can miss the middle of this 260ms animation.
    if middle_age < Duration::from_millis(180) {
        assert!(
            middle.top() > start.top() && middle.top() < end.top(),
            "the composer slides rather than jumping: {start:?} -> {middle:?} -> {end:?}"
        );
    }

    // A later prompt in the same session must not restart the entrance.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let generation = app.new_task_settle_gen;
            app.busy = false;
            app.transcript.abandon_reply();
            app.submit_as("follow up".into(), SendMode::FollowUp, cx);
            assert_eq!(app.new_task_settle_gen, generation);
            assert!(!app.new_task_settle_pending);

            // Leaving before the first frame must not play this animation on
            // the session opened from the sidebar.
            app.busy = false;
            app.transcript.clear();
            app.submit_as("another task".into(), SendMode::FollowUp, cx);
            assert!(app.new_task_settle_pending);
            app.switch_to_session(
                SessionInfo {
                    path: fake.path().join("existing.jsonl"),
                    id: "existing".into(),
                    cwd: fake.path().join("missing"),
                    title: "Existing".into(),
                    first_message: String::new(),
                    modified: SystemTime::UNIX_EPOCH,
                },
                false,
                cx,
            );
            assert!(!app.new_task_settle_pending);
            assert!(app.new_task_settle_at.is_none());
        });
    });
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

/// Long names still leave all three header controls inside the composer at
/// the minimum window size and largest supported UI scale.
#[gpui::test]
fn the_header_controls_fit_with_long_names_at_maximum_scale(cx: &mut gpui::TestAppContext) {
    let app = new_task_app(cx);
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.ui_font_size = 20.;
        theme.ui.spacing_density = 120;
        cx.set_global(theme);
        app.update(cx, |app, _| {
            app.current_workspace = Some(
                std::env::temp_dir()
                    .join("an-extraordinarily-long-project-name-that-needs-to-truncate"),
            );
            app.worktree_repo_root = app.current_workspace.clone();
            app.branch = Some(BranchStatus {
                name: "a-very-long-branch-name-that-needs-to-truncate-as-well".into(),
                ahead_behind: None,
            });
        });
    });
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(960.), px(640.)));
    draw(cx, &app, 960., 640.);
    let header = cx.debug_bounds("new-task-context-header").expect("header");
    let folder = cx.debug_bounds("new-task-folder").expect("folder");
    let branch = cx.debug_bounds("branch-chip").expect("branch");
    let location = cx.debug_bounds("status-work-in").expect("location");
    assert!(folder.size.width > px(24.) && branch.size.width > px(24.));
    assert!(
        folder.right() <= branch.left(),
        "folder {folder:?}, branch {branch:?}"
    );
    assert!(
        branch.right() <= location.left(),
        "branch {branch:?}, location {location:?}"
    );
    assert!(
        location.right() <= header.right(),
        "location {location:?}, header {header:?}"
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

/// Moving the triggers inside the composer must not let a click on the
/// header steal focus back from the picker.
#[gpui::test]
fn clicking_the_header_folder_keeps_the_picker_focused(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let (app, cx) = cx.add_window_view(|_, cx| OrbitApp::new(cx));
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            app.sidebar_visible = false;
            app.sidebar_width = px(0.);
            app.current_workspace = Some(std::env::temp_dir());
            app.worktree_repo_root = app.current_workspace.clone();
        });
    });
    cx.simulate_resize(gpui::size(px(1100.), px(720.)));
    draw(cx, &app, 1100., 720.);
    let folder = cx.debug_bounds("new-task-folder").expect("folder").center();
    cx.simulate_mouse_move(folder, None, gpui::Modifiers::none());
    draw(cx, &app, 1100., 720.);
    cx.simulate_click(folder, gpui::Modifiers::none());
    cx.update(|window, cx| {
        let app = app.read(cx);
        let picker = app
            .workspace_picker
            .as_ref()
            .expect("folder click opens picker");
        assert!(
            picker.read(cx).focus_handle(cx).is_focused(window),
            "picker retains focus"
        );
    });

    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.toggle_workspace_picker(window, cx));
    });
    draw(cx, &app, 1100., 720.);
    let location = cx
        .debug_bounds("status-work-in")
        .expect("location")
        .center();
    cx.simulate_mouse_move(location, None, gpui::Modifiers::none());
    draw(cx, &app, 1100., 720.);
    cx.simulate_click(location, gpui::Modifiers::none());
    assert!(cx.update(|window, cx| {
        let app = app.read(cx);
        app.workspace_picker.is_none()
            && app.work_in_menu_open
            && app.work_in_menu_focus.is_focused(window)
    }));
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

/// Focus leaves the composer's hairline unchanged; a file drag still gets an accent.
#[test]
fn the_composer_border_only_changes_for_file_drag() {
    let theme = Theme::for_id(ThemeId::Orbit);
    assert_eq!(OrbitApp::composer_border_color(&theme, false), theme.border);
    assert_eq!(OrbitApp::composer_border_color(&theme, true), theme.accent);
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

    // pi answers `get_session_stats` for a brand-new session with the window
    // and zero tokens. That snapshot is real but carries no usage, so the
    // footer must stay quiet rather than painting a 0% ring.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.context = Some(orbit_rpc::ContextUsage {
                tokens: Some(0),
                context_window: 128_000,
                percent: Some(0.0),
            });
            cx.notify();
        });
    });
    draw(cx, &app, 1100., 720.);
    assert!(
        cx.debug_bounds("composer-context-indicator").is_none(),
        "a zero-token snapshot is not usage"
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

/// The branch menu belongs beside its chip, never over it. The composer
/// header's chip opens the menu below it, right-aligned; once the session is
/// docked the chip moves to the status row near the bottom of the window, and
/// the same menu must open above the chip's top edge rather than covering it.
///
/// Reduce Motion gives the settled frame: the entrance travels 8px out of the
/// anchor while it fades in, and that is motion, not anchoring.
#[gpui::test]
fn the_branch_menu_opens_beside_its_chip_not_over_it(cx: &mut gpui::TestAppContext) {
    use crate::branch_picker::BranchPicker;
    use crate::git::BranchEntry;

    let app = new_task_app(cx);
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        cx.set_global(theme);
    });
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.branch = Some(BranchStatus {
                name: "main".into(),
                ahead_behind: None,
            });
            app.worktree_repo_root = app.current_workspace.clone();
            let picker = cx.new(|cx| {
                BranchPicker::new(
                    "orbit".into(),
                    vec![BranchEntry {
                        name: "main".into(),
                        commit_date: None,
                    }],
                    "main".into(),
                    Box::new(|_: String, _: &mut Window, _: &mut App| {}),
                    Box::new(|_: String, _: &mut Window, _: &mut App| {}),
                    Box::new(|_: bool, _: &mut Window, _: &mut App| {}),
                    cx,
                )
            });
            app.branch_picker = Some(picker);
            cx.notify();
        });
    });
    let cx = cx.add_empty_window();
    draw(cx, &app, 1200., 760.);

    let chip = cx
        .debug_bounds("branch-chip")
        .expect("the composer header's branch chip");
    let menu = cx.debug_bounds("branch-picker").expect("the branch menu");
    assert!(
        menu.top() >= chip.bottom(),
        "the header's menu opens below its chip: menu {menu:?}, chip {chip:?}"
    );
    assert!(
        (menu.right() - chip.right()).abs() <= px(1.),
        "and shares the chip's right edge: menu {menu:?}, chip {chip:?}"
    );

    // The first send docks the composer; the branch chip returns to the status
    // row, and the menu opens upward from the chip's top edge.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.transcript.append_user_message("hello", Vec::new());
            app.new_task_settle_at = Some(Instant::now() - Duration::from_secs(2));
            cx.notify();
        });
    });
    draw(cx, &app, 1200., 760.);
    let chip = cx
        .debug_bounds("branch-chip")
        .expect("the status row's branch chip");
    let menu = cx.debug_bounds("branch-picker").expect("the branch menu");
    assert!(
        menu.bottom() <= chip.top(),
        "the status row's menu opens above its chip: menu {menu:?}, chip {chip:?}"
    );
    assert!(
        (menu.left() - chip.left()).abs() <= px(1.),
        "and shares the chip's left edge: menu {menu:?}, chip {chip:?}"
    );
}
