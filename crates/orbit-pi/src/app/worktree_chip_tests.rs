//! The status row's "Work in" chip: the one control that answers "where does
//! this task run?" — this device, or a configured remote host.
//!
//! It replaced a pair of controls that each knew half the answer — a folder
//! chip that restarted the session, and a location chip that only appeared
//! inside a linked worktree. The chip is therefore present in a repository in
//! every *session* state (at the main checkout it names the project), and is
//! absent outside a repository. On the new-task page it sits in the composer
//! header instead of the status row (`new_task_layout_tests` pins that).

use super::*;
use crate::app::worktrees::WorkInRow;
use crate::theme::tokens::picker;
use crate::theme::{Theme, ThemeId};
use crate::worktree::Worktree;
use std::collections::HashSet;
use std::fs;

/// These tests are about the status row, which owns the location control once
/// there is a session. An app starts on the new-task page, where the chip is
/// hidden (the folder row above the composer asks instead), so the helper puts
/// one message in the transcript first.
fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        let app = cx.new(OrbitApp::new);
        app.update(cx, |app, cx| {
            app.transcript
                .append_user_message("where does this run?", Vec::new());
            cx.notify();
        });
        app
    })
}

/// A `git worktree list` entry, with only the fields these tests read set.
fn worktree(path: &Path, name: &str, is_main: bool, prunable: bool) -> Worktree {
    Worktree {
        path: path.to_path_buf(),
        name: name.into(),
        head: "0".repeat(40),
        branch: Some(name.into()),
        detached: false,
        is_main,
        bare: false,
        locked: false,
        lock_reason: None,
        prunable,
        prune_reason: None,
    }
}

fn recent_session(path: PathBuf, id: usize) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(format!("session-{id}")),
        id: id.to_string(),
        cwd: path,
        title: String::new(),
        first_message: String::new(),
        modified: SystemTime::now(),
    }
}

/// A configured SSH host, as `remote-hosts.json` would parse it.
fn remote_host(name: &str, dir: &str) -> crate::remote_hosts::RemoteHost {
    crate::remote_hosts::RemoteHost {
        name: name.into(),
        ssh: format!("{name}.example.com"),
        user: None,
        port: None,
        dir: dir.into(),
        enabled: true,
        local_dir: None,
    }
}

/// A self-cleaning directory that looks like a main checkout (`<dir>/.git` is a
/// directory) and a linked worktree of it (`<dir>/.git` names the admin dir).
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "orbit-work-in-test-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".git/worktrees")).expect("fake repo");
        Self { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.clone()
    }

    /// A linked worktree directory whose `.git` file points at `admin/<name>`.
    fn worktree_dir(&self, name: &str) -> PathBuf {
        let admin = self.root.join(".git/worktrees").join(name);
        fs::create_dir_all(&admin).expect("admin dir");
        let path = self.root.join(".worktrees").join(name);
        fs::create_dir_all(&path).expect("worktree dir");
        fs::write(path.join(".git"), format!("gitdir: {}\n", admin.display())).expect("gitfile");
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[gpui::test]
fn the_work_in_chip_is_absent_outside_a_repository(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees.clear();
            app.worktree_repo_root = None;
            app.current_workspace = Some(PathBuf::from("/tmp/notes"));
            assert!(
                app.work_in_chip(cx).is_none(),
                "a plain folder has no worktrees to choose from"
            );
        });
    });
}

#[gpui::test]
fn the_chip_names_the_project_at_the_main_checkout(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("local");
    let other = Fixture::new("other-project");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            assert_eq!(
                app.work_in_label(),
                Some(sessions::workspace_label(&fixture.repo())),
                "the main checkout identifies the active project"
            );
            assert!(
                app.work_in_chip(cx).is_some(),
                "the chip is how a worktree is reached from the composer"
            );
            app.worktree_repo_root = Some(other.repo());
            app.current_workspace = Some(other.repo());
            assert_eq!(
                app.work_in_label(),
                Some(sessions::workspace_label(&other.repo())),
                "the chip follows the active project when it changes"
            );
        });
    });
}

#[gpui::test]
fn the_chip_names_the_active_linked_worktree(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("linked");
    let worktree_path = fixture.worktree_dir("113");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true, false),
                worktree(&worktree_path, "113", false, false),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(worktree_path);
            assert_eq!(
                app.work_in_label().as_deref(),
                Some("113"),
                "a worktree is named by its worktree name, not its branch"
            );
            assert!(app.work_in_chip(cx).is_some());
        });
    });
}

#[gpui::test]
fn the_picker_lists_this_device_then_the_remote_hosts(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("rows");
    let second = fixture.worktree_dir("fix");
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _cx| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true, false),
                worktree(&fixture.worktree_dir("113"), "113", false, false),
                worktree(&second, "fix", false, true),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.work_in_hosts = vec![remote_host("prod-vps", "/srv/app")];

            let rows = app.work_in_rows().expect("a repository has rows");
            assert_eq!(rows.len(), 2, "this device, then the host");
            let WorkInRow::Folder(local) = &rows[0] else {
                panic!("the first row is this device");
            };
            assert_eq!(local.label, tr!("working_folder.local"));
            assert!(local.selected, "the active folder is the selected row");
            let WorkInRow::Remote(host) = &rows[1] else {
                panic!("the second row is the host");
            };
            assert_eq!(host.name, "prod-vps");
            assert_eq!(
                app.work_in_selected_ix(&rows),
                0,
                "the highlight starts on this device"
            );
        });
    });
}

#[test]
fn missing_worktree_local_is_found_only_while_git_lists_it() {
    let fixture = Fixture::new("missing-owner");
    let repo = fixture.root.join("project");
    fs::create_dir(&repo).unwrap();
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    git(&["init"]);
    git(&["config", "user.name", "Test"]);
    git(&["config", "user.email", "test@example.com"]);
    fs::write(repo.join("file"), "content").unwrap();
    git(&["add", "file"]);
    git(&["commit", "-m", "initial"]);
    let missing = fixture.root.join("linked");
    git(&["worktree", "add", "-b", "other", missing.to_str().unwrap()]);
    fs::remove_dir_all(&missing).unwrap();

    let local =
        super::worktrees::missing_worktree_local(&missing, std::slice::from_ref(&repo)).unwrap();
    assert!(crate::worktree::same_path(&local, &repo));
    assert!(
        super::worktrees::missing_worktree_local(&missing, &[repo.clone(), repo.clone()]).is_some()
    );
    assert!(super::worktrees::missing_worktree_local(
        &fixture.root.join("unrelated"),
        std::slice::from_ref(&repo)
    )
    .is_none());
    git(&["worktree", "prune"]);
    assert!(super::worktrees::missing_worktree_local(&missing, &[repo]).is_none());
}

#[gpui::test]
fn resuming_a_missing_cold_session_does_not_redirect_it(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("missing-session");
    let app = test_app(cx);
    let missing = fixture.root.join("missing");
    let session = recent_session(missing.clone(), 42);
    cx.update(|cx| app.update(cx, |app, cx| {
        app.lives.remove(&session.path);
        app.current_session_path = None;
        app.switch_to_session(session.clone(), true, cx);
        assert_eq!(app.current_session_path.as_ref(), Some(&session.path));
        assert_eq!(app.current_workspace.as_deref(), Some(missing.as_path()));
        assert!(matches!(&app.worktree_dialog, Some(WorktreeDialog::Unavailable { path, local: None }) if *path == missing));
        assert!(app.client.is_none());
    }));
}

#[gpui::test]
fn missing_worktree_only_offers_local_when_an_owner_is_known(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("unavailable-dialog");
    let app = test_app(cx);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    let missing = fixture.root.join("gone");
    window_cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.worktree_dialog = Some(WorktreeDialog::Unavailable {
                path: missing.clone(),
                local: None,
            });
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(window_cx
        .debug_bounds("worktree-unavailable-local")
        .is_none());
    window_cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.worktree_dialog = Some(WorktreeDialog::Unavailable {
                path: missing.clone(),
                local: Some(fixture.repo()),
            });
            cx.notify();
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(window_cx
        .debug_bounds("worktree-unavailable-local")
        .is_some());
}

#[test]
fn ambiguous_missing_worktree_owner_has_no_local_fallback() {
    let fixture = Fixture::new("ambiguous-owner");
    let missing = fixture.root.join("linked");
    let mut repos = Vec::new();
    for index in 0..2 {
        let repo = fixture.root.join(format!("project-{index}"));
        fs::create_dir(&repo).unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(&repo)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        git(&["init"]);
        git(&["config", "user.name", "Test"]);
        git(&["config", "user.email", "test@example.com"]);
        fs::write(repo.join("file"), "content").unwrap();
        git(&["add", "file"]);
        git(&["commit", "-m", "initial"]);
        git(&["worktree", "add", "-b", "other", missing.to_str().unwrap()]);
        fs::remove_dir_all(&missing).unwrap();
        repos.push(repo);
    }
    assert!(super::worktrees::missing_worktree_local(&missing, &repos).is_none());
}

#[gpui::test]
fn clone_dialog_rejects_missing_inputs_without_starting(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_clone_dialog(window, cx);
            app.confirm_worktree_dialog(cx);
            assert!(app.worktree_dialog_error.is_some());
            assert!(app.clone_operation.is_none());
        });
    });
}

#[gpui::test]
fn clone_dialog_keyboard_walks_controls_and_submits_validation(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("clone-keyboard");
    let app = test_app(cx);
    cx.update(crate::bind_keys);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_clone_dialog(window, cx);
            app.clone_test_parent = Some(fixture.repo());
        })
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.update(|window, cx| {
        assert!(app
            .read(cx)
            .clone_name_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window))
    });
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.update(|window, cx| assert!(app.read(cx).clone_browse_focus.is_focused(window)));
    window_cx.simulate_keystrokes("space");
    window_cx.update(|_, cx| {
        assert_eq!(
            app.read(cx).clone_parent.as_deref(),
            Some(fixture.repo().as_path())
        )
    });
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.update(|window, cx| assert!(app.read(cx).clone_confirm_focus.is_focused(window)));
    window_cx.simulate_keystrokes("enter");
    window_cx.update(|_, cx| assert!(app.read(cx).worktree_dialog_error.is_some()));
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.update(|window, cx| assert!(app.read(cx).clone_cancel_focus.is_focused(window)));
    window_cx.simulate_keystrokes("space");
    window_cx.update(|_, cx| assert!(app.read(cx).worktree_dialog.is_none()));
}

#[gpui::test]
fn clone_dialog_keyboard_clones_a_local_repository(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("clone-keyboard-end-to-end");
    let source = fixture.repo().join("source.git");
    assert!(std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&source)
        .output()
        .unwrap()
        .status
        .success());
    let destination = fixture.repo().join("copy");
    let app = test_app(cx);
    cx.update(crate::bind_keys);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_clone_dialog(window, cx);
            app.clone_test_parent = Some(fixture.repo());
            app.clone_url_input.update(cx, |input, cx| {
                input.set_text(source.to_string_lossy().into_owned(), cx)
            });
            app.clone_name_input
                .update(cx, |input, cx| input.set_text("copy", cx));
        })
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.simulate_keystrokes("space");
    window_cx.update(|_, cx| {
        assert_eq!(
            app.read(cx).clone_parent.as_deref(),
            Some(fixture.repo().as_path())
        )
    });
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.simulate_keystrokes("enter");
    window_cx.update(|_, cx| {
        assert!(
            app.read(cx).clone_operation.is_some() || app.read(cx).worktree_dialog.is_none(),
            "submit should start clone: {:?}",
            app.read(cx).worktree_dialog_error
        )
    });
    for _ in 0..100 {
        window_cx.run_until_parked();
        if window_cx.update(|_, cx| app.read(cx).clone_operation.is_none()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    window_cx.update(|window, cx| {
        assert!(app.read(cx).clone_operation.is_none());
        assert!(
            app.read(cx).worktree_dialog.is_none(),
            "clone should complete successfully"
        );
        assert!(app
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window));
    });
    assert!(destination.join(".git").is_dir());
}

/// A finished clone is the only thing that may add a project to the sidebar.
/// Asserted at both ends of `add_workspace`: the project list, and the grouped
/// sidebar rows the render path derives from it. An empty project still gets a
/// header, so the clone is reachable before any session exists in it — which is
/// exactly the state a fresh clone is in.
#[gpui::test]
fn a_cloned_repository_registers_and_appears_in_the_sidebar(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("clone-sidebar");
    // `/var` and `/tmp` are symlinks on macOS and `add_workspace` stores a
    // canonical path, so resolve the fixture root up front: the destination
    // does not exist yet, and a path that cannot be canonicalized is kept
    // verbatim — the two sides would disagree before and after the clone.
    let root = fs::canonicalize(fixture.repo()).expect("fixture root");
    let source = root.join("source.git");
    assert!(std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&source)
        .output()
        .unwrap()
        .status
        .success());
    let destination = root.join("copy");
    let canonical = sessions::canonical_workspace_path(&destination);
    let app = test_app(cx);
    cx.update(crate::bind_keys);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            // A group header per project is the mode that shows a clone as a
            // sidebar entry; the other modes flatten or filter the list.
            app.sidebar_group_by = SidebarGroupBy::Workspace;
            app.open_clone_dialog(window, cx);
            app.clone_test_parent = Some(root.clone());
            app.clone_url_input.update(cx, |input, cx| {
                input.set_text(source.to_string_lossy().into_owned(), cx)
            });
            app.clone_name_input
                .update(cx, |input, cx| input.set_text("copy", cx));
        })
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };
    // Tab to Browse and activate it: choosing the destination is the step the
    // clone cannot proceed without.
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.simulate_keystrokes("space");
    draw(window_cx);
    window_cx.update(|_, cx| {
        assert_eq!(app.read(cx).clone_parent.as_deref(), Some(root.as_path()));
        assert!(
            !app.read(cx).workspaces.contains(&canonical),
            "a destination chosen in the dialog is not a project yet"
        );
    });
    window_cx.simulate_keystrokes("tab");
    draw(window_cx);
    window_cx.simulate_keystrokes("enter");
    for _ in 0..100 {
        window_cx.run_until_parked();
        if window_cx.update(|_, cx| app.read(cx).clone_operation.is_none()) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    window_cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(app.clone_operation.is_none(), "the clone finished");
        assert!(
            app.worktree_dialog.is_none(),
            "the dialog closes only on success"
        );
        assert_eq!(app.current_workspace.as_deref(), Some(canonical.as_path()));
        assert!(
            app.workspaces.contains(&canonical),
            "the clone joins the project list: {:?}",
            app.workspaces
        );
        let rows = app.sidebar_rows(&app.sidebar_sessions_filtered(), &HashSet::new());
        assert!(
            rows.iter().any(|row| matches!(
                row,
                SideRow::Workspace { cwd, .. } if *cwd == canonical
            )),
            "the clone has a sidebar group among {} rows",
            rows.len()
        );
    });
    assert!(destination.join(".git").is_dir());
}

#[gpui::test]
fn clone_dialog_cancellation_signals_the_operation(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
            app.worktree_dialog = Some(WorktreeDialog::Clone);
            app.clone_operation = Some(CloneOperation {
                cancelled: cancelled.clone(),
                progress: Arc::new(std::sync::Mutex::new(String::new())),
            });
            app.cancel_worktree_dialog(cx);
            assert!(cancelled.load(std::sync::atomic::Ordering::Acquire));
            assert!(app.clone_operation.is_none());
            assert!(app.worktree_dialog.is_none());
        });
    });
}

#[gpui::test]
fn working_folder_hints_match_the_registered_keys(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::bind_keys(cx);
        let action = |key: &str, context: &str| {
            let (bindings, _) = cx.key_bindings().borrow().bindings_for_input(
                &[gpui::Keystroke::parse(key).unwrap()],
                &[gpui::KeyContext::parse(context).unwrap()],
            );
            bindings
                .first()
                .map(|binding| binding.action().boxed_clone())
                .unwrap()
        };
        assert!(action("up", "WorkInMenu")
            .as_any()
            .is::<crate::WorkInMenuPrev>());
        assert!(action("down", "WorkInMenu")
            .as_any()
            .is::<crate::WorkInMenuNext>());
        assert!(action("enter", "WorkInMenu")
            .as_any()
            .is::<crate::WorkInMenuConfirm>());
        assert!(action("escape", "WorkInMenu")
            .as_any()
            .is::<crate::WorkInMenuClose>());
        assert!(action("up", "Composer Picker")
            .as_any()
            .is::<crate::PickerSelectPrev>());
        assert!(action("down", "Composer Picker")
            .as_any()
            .is::<crate::PickerSelectNext>());
        assert!(action("enter", "Composer Picker")
            .as_any()
            .is::<crate::PickerConfirm>());
        assert!(action("escape", "Composer Picker")
            .as_any()
            .is::<crate::PickerCancel>());
    });
}

#[gpui::test]
fn work_in_keyboard_hints_fit_at_maximum_ui_scale(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("keyboard-hints");
    let app = test_app(cx);
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.ui_font_size = 20.;
        theme.ui.spacing_density = 120;
        cx.set_global(theme);
    });
    let window_cx = cx.add_empty_window();
    let viewport = gpui::size(px(960.), px(640.));
    window_cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.work_in_menu_open = true;
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let popup = window_cx.debug_bounds("work-in-popup").expect("popup");
    let footer = window_cx
        .debug_bounds("working-folder-keyboard-hints")
        .expect("hints");
    assert!(footer.left() >= popup.left() - px(1.) && footer.right() <= popup.right() + px(1.));
    assert!(footer.top() >= popup.top() && footer.bottom() <= popup.bottom() + px(1.));
    for id in [
        "working-folder-hint-navigate",
        "working-folder-hint-select",
        "working-folder-hint-close",
    ] {
        let group = window_cx.debug_bounds(id).expect(id);
        assert!(
            group.left() >= footer.left() - px(1.)
                && group.right() <= footer.right() + px(1.)
                && group.bottom() <= footer.bottom() + px(1.),
            "{id} overflows: {group:?} in {footer:?}"
        );
    }
}

/// A worktree Git lists but cannot find (`prunable`) is repaired on choose, not
/// silently switched to — the directory is not there to switch to.
#[gpui::test]
fn a_prunable_row_goes_through_the_repair_path(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("prunable");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, true)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());

            app.run_work_in_item(0, window, cx);

            assert!(
                app.worktree_operation.is_some(),
                "the repair runs in the background"
            );
            assert_eq!(
                app.current_workspace.as_deref(),
                Some(fixture.repo().as_path()),
                "a missing directory is never made the working folder"
            );
        });
    });
}

/// Choosing a folder is a change of working folder, and the agent moves there
/// with it. Picking the folder already in use is not a change: re-opening the
/// active worktree must not restart the session.
#[gpui::test]
fn choosing_the_active_folder_is_not_a_change(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("change");
    let worktree_path = fixture.worktree_dir("113");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, _cx| {
            app.current_workspace = Some(worktree_path.clone());
            assert!(
                !app.folder_change(&worktree_path),
                "the folder already in use is not a change"
            );
            assert!(
                app.folder_change(&fixture.repo()),
                "the main checkout is a different folder"
            );
            // Symlinked spellings of one folder are one folder.
            assert!(!app.folder_change(&worktree_path.join(".")));
        });
    });
}

/// The status bar's picker is built from the same two-line row body as the
/// new-task page's, so a device row and a host row tile at
/// `picker::two_line_entry_height` and never paint over each other.
#[gpui::test]
fn the_status_bar_picker_rows_tile_at_one_two_line_height(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("popover-layout");
    let app = test_app(cx);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|_window, cx| {
        app.update(cx, |app, _cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.work_in_hosts = vec![remote_host("prod-vps", "/srv/app")];
            // Open both the picker and the chat's status bar, which is where
            // the chip (and its anchored popover) live.
            app.work_in_menu_open = true;
        });
    });

    // Two frames: the picker's rows come from the cached list, which the first
    // draw settles.
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

    let list = window_cx.debug_bounds("work-in-list").expect("the list");
    let device = window_cx
        .debug_bounds("work-in-row-0")
        .expect("the device row laid out");
    let host = window_cx
        .debug_bounds("work-in-row-1")
        .expect("the host row laid out");

    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let row_h = f32::from(picker::two_line_entry_height(&theme));
    for (name, bounds) in [("device", device), ("host", host)] {
        assert!(
            (f32::from(bounds.size.height) - row_h).abs() <= 1.,
            "the {name} row is one two-line entry ({row_h}px): was {}",
            f32::from(bounds.size.height)
        );
    }
    assert!(
        host.top() >= device.bottom(),
        "the host row follows the device row: {device:?} then {host:?}"
    );
    assert!(
        (host.left() - device.left()).abs() < px(0.5)
            && (host.right() - device.right()).abs() < px(0.5),
        "both rows share the list's width: {device:?} {host:?}"
    );
    assert!(
        device.left() >= list.left() - px(1.) && host.right() <= list.right() + px(1.),
        "rows stay inside the list: {device:?} {host:?} in {list:?}"
    );

    // The row body is the flexible child, so the trailing slot is pushed to
    // the row's right edge. Wrapped in a plain div the body sized to its text
    // and the check landed immediately after the label — the misalignment this
    // guards.
    let check = window_cx
        .debug_bounds("work-in-check")
        .expect("the selected row's check laid out");
    assert!(
        check.center().x > device.center().x,
        "the check sits in the trailing slot, not next to the label: check {check:?}, rows {device:?}"
    );
}
