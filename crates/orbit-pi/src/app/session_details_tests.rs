//! The session-details chip in the top bar: it toggles a panel in the right
//! dock (not a popover), and that dock is shared one at a time with Review and
//! the Explorer.

use super::*;
use gpui::{Modifiers, MouseButton, MouseUpEvent};

fn mouse_up() -> MouseUpEvent {
    MouseUpEvent {
        button: MouseButton::Left,
        position: point(px(0.), px(0.)),
        modifiers: Modifiers::none(),
        click_count: 1,
    }
}

fn new_app(cx: &mut gpui::TestAppContext) -> (Entity<OrbitApp>, &mut gpui::VisualTestContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    // A session with a conversation: the new-task page has no details to show.
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.transcript.append_user_message("hello", Vec::new());
        });
    });
    (app, cx)
}

/// The `ⓘ` chip opens the panel and the same chip closes it again. A panel is
/// not dismissed by outside clicks, so a click that also closed a top-bar
/// popover (arming `menu_dismissed_at`) still toggles it.
#[gpui::test]
fn the_details_chip_toggles_the_panel(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);

    cx.update(|window, cx| {
        let event = mouse_up();
        app.update(cx, |app, cx| {
            app.on_session_details_click(&event, window, cx)
        });
    });
    assert!(
        cx.update(|_, cx| app.read(cx).session_details_open),
        "the chip opens the panel"
    );
    assert!(
        cx.update(|_, cx| app.read(cx).session_details_visible(cx)),
        "with no other dock open, the panel takes the right edge"
    );

    cx.update(|window, cx| {
        let event = mouse_up();
        app.update(cx, |app, cx| {
            app.on_session_details_click(&event, window, cx)
        });
    });
    assert!(
        !cx.update(|_, cx| app.read(cx).session_details_open),
        "the same chip closes it"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.menu_dismissed_at.arm();
        });
    });
    cx.update(|window, cx| {
        let event = mouse_up();
        app.update(cx, |app, cx| {
            app.on_session_details_click(&event, window, cx)
        });
    });
    assert!(
        cx.update(|_, cx| app.read(cx).session_details_open),
        "a click that closed a popover still opens the panel"
    );
}

/// The panel takes width from the chat column while it shows, and gives it
/// back when it closes.
#[gpui::test]
fn the_panel_takes_its_width_from_the_main_area(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);

    let before = cx.update(|window, cx| app.read(cx).main_area_width(window, cx));
    cx.update(|_, cx| app.update(cx, |app, cx| app.open_session_details(cx)));
    let during = cx.update(|window, cx| app.read(cx).main_area_width(window, cx));
    let dock = px(super::session::SESSION_DETAILS_GUTTER + super::session::SESSION_DETAILS_W);
    assert_eq!(
        during,
        (before - dock).max(px(0.)),
        "the open panel narrows the chat by its card and gutter"
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.close_session_details(cx)));
    let after = cx.update(|window, cx| app.read(cx).main_area_width(window, cx));
    assert_eq!(after, before, "closing it gives the width back");
}

/// Review and the details panel share the right dock: opening either closes
/// the other.
#[gpui::test]
fn review_and_the_details_panel_exclude_each_other(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_side_pane(cx)));
    assert!(cx.update(|_, cx| app.read(cx).sidepane.read(cx).is_open()));

    cx.update(|_, cx| app.update(cx, |app, cx| app.open_session_details(cx)));
    assert!(cx.update(|_, cx| app.read(cx).session_details_visible(cx)));
    assert!(
        !cx.update(|_, cx| app.read(cx).sidepane.read(cx).is_open()),
        "opening the details panel closes Review"
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_side_pane(cx)));
    assert!(
        !cx.update(|_, cx| app.read(cx).session_details_open),
        "opening Review closes the details panel"
    );
}

/// The Explorer is the dock's third tenant, under the same rule.
#[gpui::test]
fn the_explorer_and_the_details_panel_exclude_each_other(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_project_panel(cx)));
    assert!(cx.update(|_, cx| app.read(cx).project_panel.read(cx).is_open()));

    cx.update(|_, cx| app.update(cx, |app, cx| app.open_session_details(cx)));
    assert!(
        !cx.update(|_, cx| app.read(cx).project_panel.read(cx).is_open()),
        "opening the details panel closes the Explorer"
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_project_panel(cx)));
    assert!(
        !cx.update(|_, cx| app.read(cx).session_details_open),
        "opening the Explorer closes the details panel"
    );
}

/// The quota popover floats over the bar; opening it leaves the docked panel
/// where it is, and opening the panel closes a popover that was showing.
#[gpui::test]
fn the_quota_popover_leaves_the_panel_docked(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);

    cx.update(|_, cx| app.update(cx, |app, cx| app.open_session_details(cx)));
    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_quota_popup(cx)));
    assert!(cx.update(|_, cx| app.read(cx).quota_popup_open));
    assert!(
        cx.update(|_, cx| app.read(cx).session_details_open),
        "the quota popover does not close the panel"
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.close_session_details(cx)));
    cx.update(|window, cx| {
        let event = mouse_up();
        app.update(cx, |app, cx| {
            app.on_session_details_click(&event, window, cx)
        });
    });
    assert!(!cx.update(|_, cx| app.read(cx).quota_popup_open));
}

/// Drawn for real: the chip's panel is laid out as a dock on the window's
/// right edge at its card width plus gutter, not as a floating popover.
#[gpui::test]
fn the_panel_draws_on_the_right_edge(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // The test host is not asked to have pi installed to lay out.
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_session_details(cx);
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let bounds = cx
        .debug_bounds("session-details-panel")
        .expect("the details panel is on screen");

    let dock = px(super::session::SESSION_DETAILS_GUTTER + super::session::SESSION_DETAILS_W);
    assert_eq!(bounds.size.width, dock, "card plus gutter: {bounds:?}");
    assert!(
        (bounds.origin.x + bounds.size.width - viewport.width).abs() < px(1.),
        "docked flush with the right edge: {bounds:?} in {viewport:?}"
    );
    assert!(
        bounds.size.height > viewport.height * 0.5,
        "a full-height dock, not a popover: {bounds:?}"
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.close_session_details(cx)));
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("session-details-panel").is_none(),
        "closing removes the dock"
    );
}

/// The Changes section reports the working tree as it is: Review is always
/// one click away, and Commit appears only once there is something to commit.
#[gpui::test]
fn the_changes_section_offers_commit_only_when_dirty(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.added = 0;
            app.removed = 0;
            app.open_session_details(cx);
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("sess-review").is_some(),
        "Review is always offered"
    );
    assert!(
        cx.debug_bounds("sess-commit").is_none(),
        "a clean tree has nothing to commit"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.added = 12;
            app.removed = 3;
            cx.notify();
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("sess-commit").is_some(),
        "a dirty tree offers Commit"
    );
}

/// The name row shows the title as text until the edit button switches it to
/// the rename field; Cancel (or Escape) goes back without committing, and
/// closing the panel never leaves a half-edit behind for the next open.
#[gpui::test]
fn the_title_row_switches_between_text_and_the_rename_field(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.session_id = Some("abc".into());
            app.session_name = Some("Fix the sidebar".into());
            app.open_session_details(cx);
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("sess-title").is_some(),
        "at rest: the title as text"
    );
    assert!(
        cx.debug_bounds("sess-edit-title").is_some(),
        "with an edit button"
    );
    assert!(
        cx.debug_bounds("sess-rename").is_none(),
        "and no Update button"
    );

    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.begin_session_name_edit(window, cx));
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    // `debug_bounds` keeps the previous manual draw's entries, so the text's
    // absence is asserted through the state that gates it.
    assert!(
        cx.update(|_, cx| app.read(cx).session_name_editing),
        "editing replaces the text"
    );
    assert!(
        cx.debug_bounds("sess-rename").is_some(),
        "Update while editing"
    );
    assert!(
        cx.debug_bounds("sess-rename-cancel").is_some(),
        "Cancel too"
    );
    assert_eq!(
        cx.update(|_, cx| app.read(cx).session_name_input.read(cx).text().to_string()),
        "Fix the sidebar",
        "the field starts from the live title"
    );

    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.cancel_session_name_edit(window, cx));
    });
    assert!(!cx.update(|_, cx| app.read(cx).session_name_editing));

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.begin_session_name_edit(window, cx);
            app.close_session_details(cx);
            app.open_session_details(cx);
        });
    });
    assert!(
        !cx.update(|_, cx| app.read(cx).session_name_editing),
        "a reopened panel shows the title, not a stale edit"
    );
}

/// The palette's Rename Session lands straight in the rename field.
#[gpui::test]
fn palette_rename_opens_the_panel_in_edit_mode(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.begin_session_rename(window, cx));
    });
    assert!(cx.update(|_, cx| app.read(cx).session_details_open));
    assert!(cx.update(|_, cx| app.read(cx).session_name_editing));
}

/// A new task has nothing to describe: the chip is hidden and a panel left
/// open stays out of the dock until the first message arrives.
#[gpui::test]
fn a_new_task_hides_the_details_until_the_first_message(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let viewport = cx.update(|window, _| window.viewport_size());

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_session_details(cx);
        });
    });
    assert!(!cx.update(|_, cx| app.read(cx).session_has_details()));
    assert!(
        !cx.update(|_, cx| app.read(cx).session_details_visible(cx)),
        "the new-task page keeps the dock empty"
    );
    let before = cx.update(|window, cx| app.read(cx).main_area_width(window, cx));
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("session-details-panel").is_none(),
        "no panel on the new-task page"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.transcript.append_user_message("hello", Vec::new());
            cx.notify();
        });
    });
    assert!(
        cx.update(|_, cx| app.read(cx).session_details_visible(cx)),
        "the first message brings the panel back"
    );
    let after = cx.update(|window, cx| app.read(cx).main_area_width(window, cx));
    assert!(after < before, "and it takes its width again");
}

/// The Explorer has its own top-bar button (no "…" menu in between): it is
/// drawn in the bar and toggles the project panel, which then takes the dock.
#[gpui::test]
fn the_explorer_has_its_own_top_bar_button(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
        });
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("header-explorer").is_some(),
        "the Explorer button sits in the top bar"
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_project_panel(cx)));
    assert!(cx.update(|_, cx| app.read(cx).project_panel.read(cx).is_open()));
    cx.update(|_, cx| app.update(cx, |app, cx| app.toggle_project_panel(cx)));
    assert!(!cx.update(|_, cx| app.read(cx).project_panel.read(cx).is_open()));
}

/// Opening file after file must not push the Explorer off the window: the
/// Files page shrinks (its tab strip scrolls) and the dock keeps its width
/// flush with the right edge.
#[gpui::test]
fn many_open_files_keep_the_explorer_inside_the_window(cx: &mut gpui::TestAppContext) {
    let root = std::env::temp_dir().join("orbit-explorer-many-tabs");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let names: Vec<String> = (0..14)
        .map(|i| format!("a-rather-long-file-name-number-{i}.toml"))
        .collect();
    for name in &names {
        std::fs::write(root.join(name), "x = 1\n").unwrap();
    }

    let (app, cx) = new_app(cx);
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.current_workspace = Some(root.clone());
            app.toggle_project_panel(cx);
            // A roomy dock, independent of the width saved on this machine.
            app.project_panel
                .update(cx, |panel, cx| panel.set_width(px(300.), cx));
            for name in &names {
                app.open_file_in_viewer(root.join(name), name.clone(), cx);
            }
        });
    });
    cx.run_until_parked();
    assert!(cx.update(|_, cx| app.read(cx).file_viewer.read(cx).is_open()));

    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let panel = cx
        .debug_bounds("project-panel")
        .expect("the Explorer is on screen");
    let width = cx.update(|_, cx| app.read(cx).project_panel.read(cx).width());
    assert_eq!(
        panel.size.width, width,
        "the dock keeps its width: {panel:?}"
    );
    assert!(
        panel.origin.x + panel.size.width <= viewport.width + px(0.5),
        "the dock ends at the window's right edge, not past it: {panel:?} in {viewport:?}"
    );
}
