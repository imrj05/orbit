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
        cx.debug_bounds("session-details-panel").is_some(),
        "the dock slides out on `Motion::Layout`, so it is still mounted on the \
         first frame after the close"
    );
    // Run the frames the slide asked for — this test has no clock of its own —
    // then ask the render itself what the slot mounts. `debug_bounds` reports
    // the last frame the test host swapped in, which does not follow every
    // `draw` here; the mount rule is asserted exactly, and headless, in
    // `motion::panel::tests`.
    crate::motion::panel::settle(crate::motion::panel::RIGHT_DOCK, false);
    let mounted = cx.update(|window, cx| {
        let theme = *crate::theme::get(cx);
        app.update(cx, |app, cx| app.right_dock_slide(theme, &*window, cx))
    });
    assert!(
        mounted.is_none(),
        "the slot mounts no dock once it has slid shut"
    );
}

/// The slot remembers which dock held it. By the time a close is animating,
/// every `*_visible` flag is already false, so without that memory the slide
/// would be moving an empty frame instead of the card being dismissed.
#[gpui::test]
fn a_closing_dock_keeps_painting_its_own_card(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);

    let slide = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let theme = *crate::theme::get(cx);
            app.update(cx, |app, cx| app.right_dock_slide(theme, &*window, cx))
        })
    };

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // The test host is not asked to have pi installed to lay out.
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_session_details(cx);
        });
    });
    assert!(slide(cx).is_some(), "an open dock mounts its card");
    assert_eq!(
        cx.update(|_, cx| app.read(cx).right_dock_slot),
        RightDock::Details
    );

    cx.update(|_, cx| app.update(cx, |app, cx| app.close_session_details(cx)));
    assert!(slide(cx).is_some(), "the card stays mounted for the slide");
    assert_eq!(
        cx.update(|_, cx| app.read(cx).right_dock_slot),
        RightDock::Details,
        "the slot still knows which card is on its way out"
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

/// Opening the rename dialog doesn't replace the details row or lose the
/// title draft to a background title update.
#[gpui::test]
fn session_rename_dialog_preserves_the_details_and_the_draft(cx: &mut gpui::TestAppContext) {
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
        cx.debug_bounds("session-rename-card").is_none(),
        "and no rename dialog"
    );

    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.begin_session_rename(window, cx));
    });
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(cx.update(|_, cx| app.read(cx).session_name_editing));
    assert!(cx.debug_bounds("session-rename-card").is_some());
    assert!(cx.debug_bounds("session-rename-field").is_some());
    assert!(cx.debug_bounds("session-rename-actions").is_some());
    assert!(cx.debug_bounds("session-rename-generate-title").is_none());
    assert!(cx.update(|_, cx| app.read(cx).session_details_open));
    assert_eq!(
        cx.update(|_, cx| app.read(cx).session_name_input.read(cx).text().to_string()),
        "Fix the sidebar",
        "the field starts from the live title"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.session_name_input
                .update(cx, |field, cx| field.set_text("Draft", cx));
            app.session_name = Some("New live title".into());
            app.seed_session_name_input(cx);
            assert_eq!(app.session_name_input.read(cx).text(), "Draft");
            app.cancel_session_name_edit(cx);
            assert_eq!(app.session_name_input.read(cx).text(), "New live title");
            assert!(!app.session_name_editing);
        });
    });
}

#[gpui::test]
fn palette_rename_focusses_the_dialog_without_opening_the_dock(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(theme::ThemeId::Orbit));
        crate::bind_keys(cx);
    });
    let (app, cx) = cx.add_window_view(|_, cx| {
        let mut app = OrbitApp::new(cx);
        app.session_id = Some("abc".into());
        app.session_name = Some("Original title".into());
        app
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| {
        app.input.read(cx).focus(window);
        app.begin_session_rename(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(!app.session_details_open);
        assert!(app
            .session_name_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window));
    });
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(cx.update(|window, cx| app.read(cx).modal_cancel_focus.is_focused(window)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(!app.session_name_editing);
        assert_eq!(app.session_name.as_deref(), Some("Original title"));
        assert!(app.input.read(cx).focus_handle(cx).is_focused(window));
    });

    app.update_in(cx, |app, window, cx| {
        app.slash_commands.push(SlashCommand {
            name: "generate-title".into(),
            description: String::new(),
            scope: crate::mentions::CommandScope::Orbit,
        });
        app.begin_session_rename(window, cx);
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("session-rename-generate-title").is_some());
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(cx.update(|window, cx| app.read(cx).rename_generate_focus.is_focused(window)));
    cx.simulate_keystrokes("shift-tab shift-tab");
    cx.run_until_parked();
    assert!(cx.update(|window, cx| app.read(cx).rename_close_focus.is_focused(window)));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!cx.update(|_, cx| app.read(cx).session_name_editing));
}

#[gpui::test]
fn session_rename_generation_refreshes_only_the_unchanged_draft(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.session_id = Some("abc".into());
            app.session_name = Some("Original".into());
            app.begin_session_rename(window, cx);
            app.title_generation = Some(TitleGeneration {
                request_id: "title-request".into(),
                rename_draft: Some("Original".into()),
            });
            app.apply_title_to_rename_draft("Generated title", cx);
            assert_eq!(app.session_name_input.read(cx).text(), "Generated title");

            app.title_generation.as_mut().unwrap().rename_draft = Some("Generated title".into());
            app.session_name_input
                .update(cx, |input, cx| input.set_text("New draft", cx));
            app.apply_title_to_rename_draft("Another generated title", cx);
            assert_eq!(app.session_name_input.read(cx).text(), "New draft");

            let mut refresh = false;
            app.on_response("prompt", true, None, None, "other-prompt", &mut refresh, cx);
            assert!(app.title_generation.is_some());
            app.on_response(
                "prompt",
                true,
                None,
                None,
                "title-request",
                &mut refresh,
                cx,
            );
            assert!(app.title_generation.is_none());
        });
    });
}

#[gpui::test]
fn project_rename_dialog_fits_a_long_folder_name_at_max_ui_size(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    cx.update(|window, cx| {
        let mut theme = *theme::get(cx);
        theme.ui.ui_font_size = *theme::FONT_SIZES.last().unwrap();
        theme.ui.spacing_density = *theme::SPACING_DENSITIES.last().unwrap();
        theme.ui.motion = theme::MotionPreference::Reduced;
        cx.set_global(theme);
        app.update(cx, |app, cx| {
            let label = "A long original project folder name ".repeat(6);
            app.workspace_menu = Some(WorkspaceMenu {
                cwd: PathBuf::from(&label),
                label: label.clone(),
                at: None,
                picker: false,
                can_move_up: false,
                can_move_down: false,
                cursor: Default::default(),
            });
            app.on_workspace_rename(window, cx);
            assert_eq!(
                app.project_rename.as_ref().unwrap().input.read(cx).text(),
                label
            );
        });
    });
    let _ = cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(960.), px(640.)),
        |_, _| app.clone(),
    );
    let card = cx.debug_bounds("project-rename-card").unwrap();
    let header = cx.debug_bounds("project-rename-header").unwrap();
    let field = cx.debug_bounds("project-rename-field").unwrap();
    let footer = cx.debug_bounds("project-rename-actions").unwrap();
    assert!(header.bottom() <= field.top() + px(1.));
    assert!(field.bottom() <= footer.top() + px(1.));
    assert!(
        footer.bottom() <= card.bottom() + px(1.),
        "footer {footer:?} vs card {card:?}, header {header:?}, field {field:?}"
    );
    assert!(
        card.top() >= px(0.) && card.bottom() <= px(640.),
        "{card:?}"
    );
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

fn open_drawn(app: &Entity<OrbitApp>, cx: &mut gpui::VisualTestContext) {
    let viewport = cx.update(|window, _| window.viewport_size());
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.open_session_details(cx);
        });
    });
    let app = app.clone();
    let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, move |_, _| app);
}

/// Activity counts what the transcript holds, and Usage shows pi's own
/// session totals — only the buckets pi reported: no cache rows when nothing
/// was cached, and no Cost row for a model without a cost table.
#[gpui::test]
fn activity_and_usage_show_only_reported_figures(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.session_usage = Some(SessionUsage {
                input: 12_000,
                output: 3_400,
                cache_read: 0,
                cache_write: 0,
                total: 15_400,
                cost: None,
            });
        });
    });
    open_drawn(&app, cx);

    for id in ["sess-sent", "sess-received", "sess-tool-calls"] {
        assert!(cx.debug_bounds(id).is_some(), "{id} is on screen");
    }
    for id in ["sess-usage-input", "sess-usage-output"] {
        assert!(cx.debug_bounds(id).is_some(), "{id} is on screen");
    }
    for id in [
        "sess-usage-cache-read",
        "sess-usage-cache-write",
        "sess-usage-cost",
    ] {
        assert!(cx.debug_bounds(id).is_none(), "{id} has nothing to report");
    }
}

/// A long session file path wraps inside the card in full instead of
/// losing its tail (the file name) to an ellipsis.
#[gpui::test]
fn a_long_identifier_wraps_inside_the_panel(cx: &mut gpui::TestAppContext) {
    let (app, cx) = new_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.session_id = Some("019a6c2e-5b1f-7d33-a0c4-2f8e9b6d1c55".into());
            app.current_session_path = Some(PathBuf::from(
                "/Users/someone/.pi/agent/sessions/--Users-someone-Projects-a-rather-long-workspace-name--/2026-11-20T09-41-12-019a6c2e-5b1f-7d33-a0c4-2f8e9b6d1c55.jsonl",
            ));
        });
    });
    open_drawn(&app, cx);

    let panel = cx
        .debug_bounds("session-details-panel")
        .expect("the details panel is on screen");
    let value = cx
        .debug_bounds("sess-detail-value-1")
        .expect("the session file value is on screen");
    assert!(
        value.origin.x + value.size.width <= panel.origin.x + panel.size.width,
        "the path stays inside the card: {value:?} in {panel:?}"
    );
    assert!(
        value.size.height > px(30.),
        "the path wraps onto more lines rather than truncating: {value:?}"
    );
}
