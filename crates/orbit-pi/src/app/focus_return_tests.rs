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
