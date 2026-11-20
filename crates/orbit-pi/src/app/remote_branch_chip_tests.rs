//! The branch chip on a remote session: the host's own branch, read over ssh
//! (`remote_git_ui.rs`), in place of the local chip that would describe this
//! Mac's repository.

use super::*;
use crate::remote_hosts::{RemoteGit, RemoteGitRepo, RemoteHost};
use crate::theme::{Theme, ThemeId};

fn host(dir: &str) -> RemoteHost {
    RemoteHost {
        name: "vps".into(),
        ssh: "vps.example".into(),
        user: Some("deploy".into()),
        port: None,
        dir: dir.into(),
        enabled: true,
        local_dir: None,
    }
}

fn repo(branch: Option<&str>) -> RemoteGit {
    RemoteGit::Repository(RemoteGitRepo {
        branch: branch.map(str::to_string),
        ahead_behind: Some((2, 0)),
        ..RemoteGitRepo::default()
    })
}

/// An app on a remote session whose folder the host reported as `git`.
fn remote_app(cx: &mut gpui::TestAppContext, git: RemoteGit) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        let app = cx.new(OrbitApp::new);
        app.update(cx, |app, cx| {
            let host = host("/srv/app");
            app.remote = Some(host.clone());
            // Under test the read only marks itself started (no network).
            app.refresh_remote_git(cx);
            app.finish_remote_git(&host, Ok(git), cx);
        });
        app
    })
}

fn draw(cx: &mut gpui::TestAppContext, app: &Entity<OrbitApp>) -> gpui::VisualTestContext {
    let mut cx = cx.add_empty_window().clone();
    let _ = cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(1280.), px(860.)),
        |_, _| app.clone(),
    );
    cx
}

#[gpui::test]
fn a_remote_session_shows_the_hosts_branch(cx: &mut gpui::TestAppContext) {
    let app = remote_app(cx, repo(Some("feat/remote")));
    cx.update(|cx| {
        let app = app.read(cx);
        let repo = app.remote_repo().expect("the host's repository");
        assert_eq!(repo.branch.as_deref(), Some("feat/remote"));
    });
    let mut cx = draw(cx, &app);
    assert!(
        cx.debug_bounds("remote-branch-chip").is_some(),
        "the host's branch is in the header"
    );
    assert!(
        cx.debug_bounds("branch-chip").is_none(),
        "never this Mac's branch on a remote session"
    );
}

#[gpui::test]
fn no_chip_without_a_branch_on_the_host(cx: &mut gpui::TestAppContext) {
    for git in [repo(None), RemoteGit::NotARepository, RemoteGit::Missing] {
        let app = remote_app(cx, git.clone());
        let mut cx = draw(cx, &app);
        assert!(
            cx.debug_bounds("remote-branch-chip").is_none(),
            "{git:?}: a detached HEAD, a plain folder or no git shows no chip"
        );
    }
}

/// A read describes one folder: after the session moves to another folder on
/// the same host, the old answer is not shown for the new one.
#[gpui::test]
fn a_read_for_another_folder_is_not_shown(cx: &mut gpui::TestAppContext) {
    let app = remote_app(cx, repo(Some("main")));
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.remote = Some(host("/srv/other"));
            assert!(app.remote_repo().is_none());
            app.remote = None;
            assert!(app.remote_repo().is_none(), "a local session has none");
        })
    });
}

/// The remote chip is the local selector's own face: the same branch draws a
/// chip of exactly the same size either way.
#[gpui::test]
fn the_remote_chip_looks_like_the_local_selector(cx: &mut gpui::TestAppContext) {
    let local = cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        let app = cx.new(OrbitApp::new);
        app.update(cx, |app, _| {
            app.remote = None;
            app.branch = Some(BranchStatus {
                name: "main".into(),
                ahead_behind: None,
            });
        });
        app
    });
    let mut local_cx = draw(cx, &local);
    let local_chip = local_cx.debug_bounds("branch-chip").expect("local chip");
    // The app may have read this checkout's own branch by now; the remote
    // chip gets whatever name the local one painted.
    let name = cx.update(|cx| local.read(cx).branch.as_ref().map(|b| b.name.clone()));
    let remote = remote_app(
        cx,
        RemoteGit::Repository(RemoteGitRepo {
            branch: name,
            ..RemoteGitRepo::default()
        }),
    );
    let mut remote_cx = draw(cx, &remote);
    let remote_chip = remote_cx
        .debug_bounds("remote-branch-chip")
        .expect("remote chip");
    assert_eq!(remote_chip.size, local_chip.size);
}

#[test]
fn the_hosts_branch_lines_and_switch_results_are_read() {
    let listed = crate::remote_hosts::parse_branch_lines(
        "orbit-branches: started\nmain\t2026-11-01T10:00:00+00:00\nold\n\n",
    );
    let names: Vec<&str> = listed.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["main", "old"]);
    assert!(listed[0].commit_date.is_some() && listed[1].commit_date.is_none());

    use crate::remote_hosts::parse_switch_result;
    assert_eq!(
        parse_switch_result("orbit-branch: started\norbit-branch: ok\n"),
        Ok(())
    );
    assert_eq!(
        parse_switch_result(concat!(
            "orbit-branch: detail error: Your local changes would be overwritten\n",
            "orbit-branch: detail Aborting\n",
            "orbit-branch: failed\n",
        )),
        Err("error: Your local changes would be overwritten".into()),
        "git's own error line, not the trailing Aborting"
    );
}

/// The chip opens the same `BranchPicker` this Mac uses, with the host's
/// branches; the current one is always listed.
#[gpui::test]
fn the_remote_chip_opens_the_branch_picker(cx: &mut gpui::TestAppContext) {
    let app = remote_app(cx, repo(Some("main")));
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.toggle_branch_picker(window, cx);
            assert!(
                app.branch_operation_pending,
                "the host's branches are being read"
            );
            assert!(app.branch_picker.is_none(), "never this Mac's branches");

            app.branch_operation_pending = false;
            let host = app.remote.clone().unwrap();
            let branches = vec![crate::git::BranchEntry {
                name: "feat/x".into(),
                commit_date: None,
            }];
            app.show_remote_branch_picker(&host, branches, "main".into(), window, cx);
            assert!(app.branch_picker.is_some(), "the picker opened");
        })
    });
}

#[gpui::test]
fn a_switch_on_the_host_reports_and_rereads(cx: &mut gpui::TestAppContext) {
    let app = remote_app(cx, repo(Some("main")));
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let host = app.remote.clone().unwrap();
            let toasts = |app: &OrbitApp| -> Vec<String> {
                app.toasts.items().iter().map(|t| t.title.clone()).collect()
            };

            app.switch_remote_branch("bad name".into(), false, cx);
            assert!(!app.branch_operation_pending, "an invalid name never runs");
            assert!(toasts(app).contains(&tr!("git.branch_name_invalid")));

            app.branch_operation_pending = true;
            app.finish_remote_branch_switch(&host, "feat/x".into(), false, Ok(Ok(())), cx);
            assert!(!app.branch_operation_pending);
            assert!(toasts(app).contains(&tr!("pickers.switched_to", label = "feat/x")));
            assert!(app.remote_git.loading, "the folder is read again");

            app.finish_remote_branch_switch(
                &host,
                "feat/y".into(),
                true,
                Ok(Err("fatal: a branch named 'feat/y' already exists".into())),
                cx,
            );
            assert!(toasts(app).iter().any(|t| t.contains("already exists")));
        })
    });
}
