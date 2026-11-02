//! The status row's "Work in" chip: the one control that answers "where does
//! this task run?".
//!
//! It replaced a pair of controls that each knew half the answer — a folder
//! chip that restarted the session, and a location chip that only appeared
//! inside a linked worktree. The chip is therefore present in a repository in
//! *every* state (at the main checkout it reads `Local`, and it is how a
//! worktree is reached from the composer), and absent outside one.

use super::*;
use crate::app::worktrees::WorkInRow;
use crate::theme::tokens::picker;
use crate::theme::{Theme, ThemeId};
use crate::worktree::Worktree;
use std::fs;

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
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
fn the_chip_is_present_at_the_main_checkout_and_reads_local(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("local");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            assert_eq!(
                app.work_in_label().as_deref(),
                Some(tr!("working_folder.local").as_str()),
                "the main checkout is Local"
            );
            assert!(
                app.work_in_chip(cx).is_some(),
                "the chip is how a worktree is reached from the composer"
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
fn the_picker_lists_locations_then_the_ways_to_a_new_one(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("rows");
    let first = fixture.worktree_dir("113");
    let second = fixture.worktree_dir("fix");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, _cx| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true, false),
                worktree(&first, "113", false, false),
                worktree(&second, "fix", false, true),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(first.clone());

            let rows = app.work_in_rows().expect("a repository has rows");
            let labels: Vec<String> = rows
                .iter()
                .filter_map(|row| match row {
                    WorkInRow::Folder(folder) => Some(folder.label.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels,
                vec![tr!("working_folder.local"), "113".into(), "fix".into()]
            );
            assert_eq!(
                app.work_in_selected_ix(&rows),
                1,
                "the highlight starts on the active worktree"
            );
            assert!(
                matches!(rows[3], WorkInRow::NewWorktree),
                "the ways to a new folder follow the locations"
            );
            assert!(rows
                .iter()
                .any(|row| matches!(row, WorkInRow::ChooseFolder)));
            assert!(
                rows.iter().any(|row| matches!(row, WorkInRow::RemoteRef)),
                "a remote branch is one of the ways in"
            );
        });
    });
}

#[gpui::test]
fn recent_folders_are_deduplicated_from_worktrees_and_limited(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("recents");
    let linked = fixture.worktree_dir("linked");
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true, false),
                worktree(&linked, "linked", false, false),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.sessions = vec![
                recent_session(sessions::canonical_workspace_path(&fixture.repo()), 0),
                recent_session(sessions::canonical_workspace_path(&linked), 1),
            ];
            for i in 0..10 {
                app.sessions.push(recent_session(
                    fixture.root.join(format!("recent-{i}")),
                    i + 2,
                ));
            }
            app.sessions
                .insert(3, recent_session(fixture.root.join("recent-0"), 99));
            let rows = app.work_in_rows().unwrap();
            let recent: Vec<_> = rows
                .iter()
                .filter_map(|row| match row {
                    WorkInRow::Recent(path) => Some(path.clone()),
                    _ => None,
                })
                .collect();
            assert_eq!(recent.len(), crate::workspace_picker::MAX_RECENTS);
            assert_eq!(recent[0], fixture.root.join("recent-0"));
            assert_eq!(recent[1], fixture.root.join("recent-1"));
            assert_eq!(recent[7], fixture.root.join("recent-7"));
        })
    });
}

#[gpui::test]
fn a_missing_recent_folder_does_not_change_the_workspace(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("missing-recent");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.sessions = vec![recent_session(fixture.root.join("gone"), 1)];
            let rows = app.work_in_rows().unwrap();
            let ix = rows
                .iter()
                .position(|row| matches!(row, WorkInRow::Recent(_)))
                .unwrap();
            app.run_work_in_item(ix, window, cx);
            assert_eq!(
                app.current_workspace.as_deref(),
                Some(fixture.repo().as_path())
            );
        })
    });
}

#[gpui::test]
fn a_broken_recent_worktree_does_not_change_the_workspace(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("broken-recent");
    let broken = fixture.root.join("broken");
    fs::create_dir_all(&broken).unwrap();
    fs::write(
        broken.join(".git"),
        format!("gitdir: {}\n", fixture.root.join("missing-admin").display()),
    )
    .unwrap();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.sessions = vec![recent_session(broken, 1)];
            let ix = app
                .work_in_rows()
                .unwrap()
                .iter()
                .position(|row| matches!(row, WorkInRow::Recent(_)))
                .unwrap();
            app.run_work_in_item(ix, window, cx);
            assert_eq!(
                app.current_workspace.as_deref(),
                Some(fixture.repo().as_path())
            );
        })
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
fn clone_action_opens_a_dialog_without_starting_a_task(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("clone-action");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            let rows = app.work_in_rows().unwrap();
            let index = rows
                .iter()
                .position(|row| matches!(row, WorkInRow::Clone))
                .unwrap();
            app.run_work_in_item(index, window, cx);
            assert!(matches!(app.worktree_dialog, Some(WorktreeDialog::Clone)));
            assert!(app.clone_operation.is_none());
            assert_eq!(
                app.current_workspace.as_deref(),
                Some(fixture.repo().as_path())
            );
        });
    });
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

/// Choosing a remote ref has to end in a **local branch that tracks it**:
/// `git worktree add <path> origin/feat` would leave a detached HEAD.
#[gpui::test]
fn a_remote_row_opens_the_dialog_on_a_tracking_branch(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("remote");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.worktree_local_branches = vec!["feat".into()];
            app.worktree_remote_names = vec!["origin".into()];
            app.worktree_remotes = vec![crate::git_ops::RefEntry {
                name: "origin/feat".into(),
                kind: crate::git::RefKind::Remote,
                commit_date: None,
            }];

            let row = app
                .work_in_remote_rows()
                .into_iter()
                .next()
                .expect("one remote row");
            assert_eq!(
                row.local_branch, "feat-2",
                "the local name is free because feat is taken"
            );
            app.open_worktree_from_remote(row, window, cx);

            assert!(matches!(app.worktree_dialog, Some(WorktreeDialog::Create)));
            assert_eq!(app.worktree_branch_mode, WorktreeBranchMode::Existing);
            assert_eq!(
                app.worktree_branch_choice.as_deref(),
                Some("origin/feat"),
                "the dialog's existing-branch choice is the remote ref"
            );
            assert_eq!(
                app.worktree_name_input.read(cx).text().trim(),
                "feat",
                "the suggested worktree name follows the branch"
            );
        });
    });
}

#[gpui::test]
fn remote_rows_fit_without_overlap_and_keyboard_reveals_the_last(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("remote-layout");
    let app = test_app(cx);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|_, cx| app.update(cx, |app, _| {
        app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
        app.worktree_repo_root = Some(fixture.repo());
        app.current_workspace = Some(fixture.repo());
        app.worktree_remote_names = vec!["origin".into()];
        app.worktree_remotes = (0..12).map(|ix| crate::git_ops::RefEntry {
            name: format!("origin/feature/a-very-long-branch-name-that-must-stay-within-the-dropdown-{ix}"),
            kind: crate::git::RefKind::Remote,
            commit_date: None,
        }).collect();
        app.worktree_remotes_busy = false;
        app.work_in_menu_open = true;
        app.work_in_remote_open = true;
        app.work_in_remote_highlight = 0;
    }));
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let first = window_cx
        .debug_bounds("work-in-remote-0")
        .expect("first remote row");
    let second = window_cx
        .debug_bounds("work-in-remote-1")
        .expect("second remote row");
    let list = window_cx.debug_bounds("work-in-list").expect("remote list");
    let theme = window_cx.update(|_, cx| *theme::get(cx));
    let expected = picker::two_line_entry_height(&theme);
    assert!(
        first.size.height >= expected - px(1.),
        "remote row must fit both lines: {first:?}"
    );
    assert!(
        second.top() >= first.bottom() - px(1.),
        "rows must not overlap: {first:?} {second:?}"
    );
    assert!(
        first.right() <= list.right() + px(1.),
        "long labels must fit the list: {first:?} {list:?}"
    );
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_work_in_prev(&crate::WorkInMenuPrev, window, cx)
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let list = window_cx.debug_bounds("work-in-list").expect("remote list");
    let last = window_cx
        .debug_bounds("work-in-remote-11")
        .expect("last remote row");
    assert!(
        last.top() >= list.top() - px(1.) && last.bottom() <= list.bottom() + px(1.),
        "keyboard selection must be visible: {last:?} in {list:?}"
    );
}

#[gpui::test]
fn long_remote_details_are_keyboard_accessible_and_bounded(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("remote-details");
    let app = test_app(cx);
    cx.update(|cx| {
        crate::bind_keys(cx);
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.ui_font_size = 20.;
        theme.ui.spacing_density = 120;
        cx.set_global(theme);
    });
    let window_cx = cx.add_empty_window();
    let viewport = gpui::size(px(960.), px(640.));
    let remote = format!("origin/{}end", "longbranch/".repeat(100));
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.worktree_remote_names = vec!["origin".into()];
            app.worktree_remotes = vec![crate::git_ops::RefEntry {
                name: remote.clone(),
                kind: crate::git::RefKind::Remote,
                commit_date: None,
            }];
            app.work_in_menu_open = true;
            app.work_in_remote_open = true;
            app.work_in_details_open = false;
            window.focus(&app.work_in_menu_focus);
            cx.notify();
        })
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };
    draw(window_cx);
    window_cx.simulate_keystrokes("space");
    draw(window_cx);
    window_cx.update(|_, cx| assert!(app.read(cx).work_in_details_open));
    let popup = window_cx.debug_bounds("work-in-popup").expect("popup");
    assert!(
        popup.top() >= px(0.)
            && popup.bottom() <= viewport.height
            && popup.right() <= viewport.width,
        "details popup outside window: {popup:?}"
    );
    let list = window_cx.debug_bounds("work-in-list").expect("list");
    let detail = window_cx
        .debug_bounds("work-in-remote-details")
        .expect("details");
    assert!(
        list.top() >= px(0.) && list.bottom() <= viewport.height && list.right() <= viewport.width,
        "details viewport outside window: {list:?}"
    );
    assert!(
        detail.size.height > list.size.height,
        "long content should scroll"
    );
    for _ in 0..80 {
        window_cx.simulate_keystrokes("down");
    }
    draw(window_cx);
    let local = window_cx
        .debug_bounds("work-in-remote-full-local")
        .expect("local branch");
    assert!(
        local.bottom() <= list.bottom() + px(1.) && local.bottom() > list.top(),
        "the end of the local branch must be reachable: {local:?} in {list:?}"
    );
    window_cx.simulate_keystrokes("escape");
    window_cx.update(|_, cx| {
        assert!(!app.read(cx).work_in_details_open);
        assert!(app.read(cx).work_in_remote_open);
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
    window_cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.work_in_remote_open = true;
            cx.notify();
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(window_cx.debug_bounds("working-folder-hint-back").is_some());
    assert!(window_cx
        .debug_bounds("working-folder-hint-close")
        .is_none());
}

/// The remote level is a second screen: Escape steps back to the locations
/// instead of closing the picker.
#[gpui::test]
fn escape_leaves_the_remote_level_before_closing_the_picker(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("back");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.work_in_menu_open = true;
            app.work_in_remote_open = true;

            app.on_work_in_close(&crate::WorkInMenuClose, window, cx);
            assert!(!app.work_in_remote_open, "the remote level closes first");
            assert!(app.work_in_menu_open, "the picker is still open");

            app.on_work_in_close(&crate::WorkInMenuClose, window, cx);
            assert!(
                !app.work_in_menu_open,
                "the second Escape closes the picker"
            );
        });
    });
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
            let gone = fixture.root.join(".worktrees/gone");
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true, false),
                worktree(&gone, "gone", false, true),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());

            app.run_work_in_item(1, window, cx);

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

#[gpui::test]
fn keyboard_navigation_reveals_the_last_recent_folder(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("recent-scroll");
    let app = test_app(cx);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true, false)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.sessions = (0..8)
                .map(|i| recent_session(fixture.root.join(format!("recent-{i}")), i))
                .collect();
            app.work_in_menu_open = true;
            app.work_in_menu_highlight = 0;
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_work_in_prev(&crate::WorkInMenuPrev, window, cx);
            assert!(matches!(
                app.work_in_rows().unwrap().last(),
                Some(WorkInRow::Recent(_))
            ));
            assert!(
                app.work_in_scroll.offset().y < px(0.),
                "the last recent is scrolled into view"
            );
        })
    });
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let list = window_cx.debug_bounds("work-in-list").expect("list bounds");
    let last = window_cx
        .debug_bounds("work-in-recent-13")
        .expect("last recent bounds");
    assert!(
        last.top() >= list.top() - px(1.) && last.bottom() <= list.bottom() + px(1.),
        "last recent {last:?} must be visible in {list:?}"
    );
}

/// The status bar's picker is built from the same two-line row body as the
/// new-task page's, so its rows tile at `picker::two_line_entry_height` and the
/// verbs never paint over the folder above them.
#[gpui::test]
fn the_status_bar_picker_rows_tile_at_one_two_line_height(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new("popover-layout");
    let app = test_app(cx);
    let window_cx = cx.add_empty_window();
    let viewport = window_cx.update(|window, _| window.viewport_size());
    window_cx.update(|_window, cx| {
        app.update(cx, |app, _cx| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true, false),
                worktree(&fixture.worktree_dir("113"), "113", false, false),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            // Open both the picker and the chat's status bar, which is where
            // the chip (and its anchored popover) live.
            app.work_in_menu_open = true;
        });
    });

    // Two frames: the picker's rows come from the cached list, which the first
    // draw settles.
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let _ = window_cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

    let locations = window_cx
        .debug_bounds("work-in-locations")
        .expect("the location rows laid out");
    let actions = window_cx
        .debug_bounds("work-in-actions")
        .expect("the action rows laid out");

    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let location_h = f32::from(picker::two_line_entry_height(&theme));
    // A verb is one line: its sentence-length hint lives in the tooltip, not in
    // a 300px menu row that would ellipsize it mid-word.
    let verb_h = f32::from(picker::entry_height(&theme));
    // Local + the linked worktree, and the five verbs.
    assert!(
        (f32::from(locations.size.height) - 2. * location_h).abs() <= 1.,
        "two location rows at {location_h}px each: was {}",
        f32::from(locations.size.height)
    );
    assert!(
        (f32::from(actions.size.height) - 5. * verb_h).abs() <= 1.,
        "five one-line verb rows at {verb_h}px each: was {}",
        f32::from(actions.size.height)
    );
    assert!(
        f32::from(actions.top()) >= f32::from(locations.bottom()) - 1.,
        "the verbs follow the folders: {locations:?} then {actions:?}"
    );

    // The row body is the flexible child, so the trailing slot is pushed to
    // the row's right edge. Wrapped in a plain div the body sized to its text
    // and the check landed immediately after the label — the misalignment this
    // guards.
    let check = window_cx
        .debug_bounds("work-in-check")
        .expect("the selected location's check laid out");
    assert!(
        check.center().x > locations.center().x,
        "the check sits in the trailing slot, not next to the label: check {check:?}, rows {locations:?}"
    );
}
