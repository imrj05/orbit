//! A modal surface hands focus back to the control that opened it, and to the
//! composer when that control is out of sight by the time the surface closes.
//!
//! The app is the window's root view here, not an element drawn into an empty
//! window: the focus-lost hook reads the window's own frames, and an empty root
//! would make every self-scheduled redraw look like focus going nowhere.

use super::*;

fn session(title: &str, cwd: &str, id: &str) -> SessionInfo {
    SessionInfo {
        path: std::path::PathBuf::from(format!("/sessions/{id}.jsonl")),
        id: id.to_string(),
        cwd: std::path::PathBuf::from(cwd),
        title: title.to_string(),
        first_message: String::new(),
        modified: std::time::SystemTime::UNIX_EPOCH,
    }
}

fn open_app(cx: &mut gpui::TestAppContext) -> (Entity<OrbitApp>, &mut gpui::VisualTestContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.set_global(crate::theme::AppearancePrefs::default());
        crate::bind_keys(cx);
    });
    let (app, cx) = cx.add_window_view(|window, cx| {
        let mut app = OrbitApp::new(cx);
        for dependency in &mut app.deps {
            dependency.installed = true;
        }
        app.setup_open = false;
        app.settings_open = false;
        app.sidebar_visible = true;
        // A workspace with sessions, so the list is painted and can hold focus.
        app.workspaces = vec![std::path::PathBuf::from("/work/focus-return")];
        app.sessions = vec![
            session("First", "/work/focus-return", "first"),
            session("Second", "/work/focus-return", "second"),
        ];
        app.current_session_path = None;
        app.watch_window_activation(window, cx);
        app.input.read(cx).focus(window);
        app
    });
    cx.run_until_parked();
    (app, cx)
}

#[gpui::test]
fn settings_keyboard_navigation_moves_focus_before_opening_a_section(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = open_app(cx);
    cx.simulate_resize(gpui::size(px(960.), px(640.)));
    cx.simulate_keystrokes(crate::platform::shortcuts::FOCUS_SESSIONS);
    cx.simulate_keystrokes(crate::platform::shortcuts::SETTINGS);
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(app.settings_back_focus.is_focused(window));
        assert!(app.settings_section == SettingsSection::General);
    });

    cx.simulate_keystrokes("tab");
    cx.update(|window, cx| assert!(app.read(cx).settings_nav_focus.is_focused(window)));
    cx.simulate_keystrokes("down");
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.settings_nav_cursor, 1);
        assert!(app.settings_section == SettingsSection::General);
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| assert!(app.read(cx).settings_section == SettingsSection::Appearance));

    cx.simulate_keystrokes("end");
    cx.update(|_, cx| {
        let app = app.read(cx);
        let viewport = app.settings_nav_scroll.bounds();
        let row = app
            .settings_nav_scroll
            .bounds_for_item(app.settings_nav_cursor)
            .expect("the focused row is laid out");
        let offset = app.settings_nav_scroll.offset().y;
        assert!(row.bottom() + offset <= viewport.bottom());
        assert!(app.settings_section == SettingsSection::Appearance);
    });
    cx.simulate_keystrokes("home space");
    cx.update(|_, cx| assert!(app.read(cx).settings_section == SettingsSection::General));

    cx.simulate_keystrokes("shift-tab");
    cx.update(|window, cx| assert!(app.read(cx).settings_back_focus.is_focused(window)));
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").expect("a valid key"),
    });
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(!app.settings_open);
        assert!(app.sidebar_focus.is_focused(window));
    });

    cx.simulate_keystrokes(crate::platform::shortcuts::SETTINGS);
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(!app.settings_open);
        assert!(app.sidebar_focus.is_focused(window));
    });
}

/// Opens the update modal over the focused session list and lets it take
/// focus on the next paint, the way a finished update check does.
fn open_update_dialog_over_the_sidebar(app: &Entity<OrbitApp>, cx: &mut gpui::VisualTestContext) {
    cx.simulate_keystrokes(crate::platform::shortcuts::FOCUS_SESSIONS);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            app.read(cx).sidebar_focus.is_focused(window),
            "the list holds focus"
        );
        app.update(cx, |app, cx| {
            app.updater_dialog = Some(UpdateDialog::UpToDate);
            app.updater_dialog_focus_pending = true;
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            app.read(cx).updater_dialog_focus.is_focused(window),
            "the modal takes focus when it opens"
        );
    });
}

#[gpui::test]
fn closing_a_modal_returns_focus_to_the_control_that_opened_it(cx: &mut gpui::TestAppContext) {
    let (app, cx) = open_app(cx);
    open_update_dialog_over_the_sidebar(&app, cx);

    app.update_in(cx, |app, window, cx| app.dismiss_update_dialog(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            app.read(cx).sidebar_focus.is_focused(window),
            "focus goes back to the session list, not the composer"
        );
    });
}

#[gpui::test]
fn an_opener_in_a_hidden_sidebar_hands_focus_to_the_composer(cx: &mut gpui::TestAppContext) {
    let (app, cx) = open_app(cx);
    open_update_dialog_over_the_sidebar(&app, cx);

    // The sidebar slides shut while the modal is up. It stays mounted, so only
    // the hidden-sidebar guard keeps focus from landing on it.
    app.update(cx, |app, cx| {
        app.sidebar_visible = false;
        cx.notify();
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.dismiss_update_dialog(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(
            app.input.read(cx).focus_handle(cx).is_focused(window),
            "a list nobody can see must not take the keyboard"
        );
    });
}

/// The worktree rename field takes focus when its dialog opens (it opens from a
/// row menu, with no window to focus with) and hands it back to the opener on
/// the first frame after the dialog closes, whichever path closed it.
#[gpui::test]
fn the_rename_field_takes_focus_and_returns_it_when_the_dialog_closes(
    cx: &mut gpui::TestAppContext,
) {
    let (app, cx) = open_app(cx);
    cx.simulate_keystrokes(crate::platform::shortcuts::FOCUS_SESSIONS);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(app.read(cx).sidebar_focus.is_focused(window));
        app.update(cx, |app, cx| {
            app.start_rename_worktree(std::path::PathBuf::from("/work/focus-return"), cx)
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(
            app.worktree_field_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            "the field is ready to type into"
        );
    });

    app.update(cx, |app, cx| app.cancel_worktree_dialog(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            app.read(cx).sidebar_focus.is_focused(window),
            "closing hands focus back to the session list"
        );
    });
}

/// Opens Settings with its search field holding the keyboard, then the
/// remote-host editor over it, the way Settings → Remote hosts → Add does.
fn open_remote_host_editor_from_settings(app: &Entity<OrbitApp>, cx: &mut gpui::VisualTestContext) {
    app.update_in(cx, |app, window, cx| {
        app.settings_open = true;
        app.settings_search.read(cx).focus(window);
        cx.notify();
    });
    cx.run_until_parked();
    app.update_in(cx, |app, window, cx| app.remote_host_add(window, cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let app = app.read(cx);
        let editor = app.remote_host_editor.as_ref().expect("the editor is open");
        assert!(
            editor.ssh.read(cx).focus_handle(cx).is_focused(window),
            "the editor's first field takes the keyboard"
        );
    });
}

/// A modal that focuses its own field as it opens remembers the opener first,
/// and hands the keyboard back on the first frame after it closes — the shared
/// `ModalFocus` contract.
#[gpui::test]
fn the_remote_host_editor_returns_focus_to_its_opener(cx: &mut gpui::TestAppContext) {
    let (app, cx) = open_app(cx);
    open_remote_host_editor_from_settings(&app, cx);

    app.update(cx, |app, cx| app.remote_host_editor_close(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert!(
            app.settings_search
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            "closing hands focus back to the field that opened it"
        );
    });
}

/// Tab and Shift-Tab walk the modal's own fields, then its footer (Cancel,
/// then Save), and wrap, instead of leaving for the workbench behind the
/// scrim.
#[gpui::test]
fn tab_cycles_inside_an_open_form_modal(cx: &mut gpui::TestAppContext) {
    let (app, cx) = open_app(cx);
    open_remote_host_editor_from_settings(&app, cx);
    let focused_field = |app: &Entity<OrbitApp>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let editor = app.read(cx).remote_host_editor.as_ref().expect("open");
            [
                ("ssh", &editor.ssh),
                ("user", &editor.user),
                ("port", &editor.port),
                ("dir", &editor.dir),
                ("name", &editor.name),
            ]
            .into_iter()
            .find(|(_, field)| field.read(cx).focus_handle(cx).is_focused(window))
            .map(|(name, _)| name)
            .or_else(|| {
                let app = app.read(cx);
                if app.modal_cancel_focus.is_focused(window) {
                    Some("cancel")
                } else if app.modal_confirm_focus.is_focused(window) {
                    Some("save")
                } else {
                    None
                }
            })
        })
    };

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(focused_field(&app, cx), Some("user"));

    cx.simulate_keystrokes("shift-tab shift-tab");
    cx.run_until_parked();
    assert_eq!(
        focused_field(&app, cx),
        Some("save"),
        "Shift-Tab wraps to the footer's primary action"
    );

    cx.simulate_keystrokes("shift-tab shift-tab");
    cx.run_until_parked();
    assert_eq!(
        focused_field(&app, cx),
        Some("name"),
        "then up through Cancel"
    );

    cx.simulate_keystrokes("tab tab tab");
    cx.run_until_parked();
    assert_eq!(
        focused_field(&app, cx),
        Some("ssh"),
        "Tab passes Cancel and Save, then wraps to the first field"
    );
}

/// The main composer keeps Tab as focus traversal when no modal is open.
#[gpui::test]
fn without_a_modal_tab_still_leaves_the_composer(cx: &mut gpui::TestAppContext) {
    let (app, cx) = open_app(cx);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            !app.read(cx)
                .input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window),
            "Tab moves on from the composer"
        );
    });
}
