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
            assert!(matches!(rows.last(), Some(WorkInRow::ChooseFolder)));
            assert!(
                rows.iter().any(|row| matches!(row, WorkInRow::RemoteRef)),
                "a remote branch is one of the ways in"
            );
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
    // Local + the linked worktree, and the four verbs.
    assert!(
        (f32::from(locations.size.height) - 2. * location_h).abs() <= 1.,
        "two location rows at {location_h}px each: was {}",
        f32::from(locations.size.height)
    );
    assert!(
        (f32::from(actions.size.height) - 4. * verb_h).abs() <= 1.,
        "four one-line verb rows at {verb_h}px each: was {}",
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
