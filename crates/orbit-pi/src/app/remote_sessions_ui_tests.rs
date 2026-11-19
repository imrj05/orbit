//! Remote sessions in the sidebar: the server groups, how a list's failures
//! land, and a resume carrying no workflow mode. The host script is covered
//! in `orbit_rpc::remote` and the parsing and grouping in
//! `crate::remote_sessions`.

use std::time::{Duration, UNIX_EPOCH};

use super::remote_sessions_ui::RemoteListStatus;
use super::ssh_auth_ui::AuthThen;
use super::*;
use crate::remote_hosts::{ProbeFailure, RemoteHost};
use crate::remote_sessions::{RemoteSession, GROUP_VISIBLE};
use crate::theme::{Theme, ThemeId};

fn host(name: &str, dir: &str) -> RemoteHost {
    RemoteHost {
        name: name.into(),
        ssh: "vps.example".into(),
        user: Some("deploy".into()),
        port: None,
        dir: dir.into(),
        enabled: true,
        local_dir: None,
    }
}

fn session(id: &str, cwd: &str, age: u64) -> RemoteSession {
    RemoteSession {
        path: format!("/home/deploy/.pi/agent/sessions/x/{id}.jsonl"),
        id: id.into(),
        cwd: cwd.into(),
        title: format!("task {id}"),
        first_message: format!("task {id}"),
        modified: UNIX_EPOCH + Duration::from_secs(1_000_000 - age),
    }
}

fn app_with_hosts(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        let app = cx.new(OrbitApp::new);
        app.update(cx, |app, _| {
            app.remote_hosts = vec![
                host("vps", "/home/deploy"),
                host("demo on vps", "/home/deploy/demo"),
            ];
            app.remote_session_lists.insert(
                app.remote_hosts[0].account_key(),
                (0..12)
                    .map(|n| session(&format!("d{n}"), "/home/deploy/demo", n))
                    .chain([session("o", "/home/deploy/other", 50)])
                    .collect(),
            );
        });
        app
    })
}

/// What `remote_session_rows` laid out, as short labels.
fn layout(app: &OrbitApp) -> Vec<String> {
    app.remote_session_rows()
        .iter()
        .map(|row| match row {
            SideRow::RemoteHost { name, count, .. } => format!("# {name} ({count})"),
            SideRow::RemoteSession(row) => row.session.id.clone(),
            _ => "?".into(),
        })
        .collect()
}

#[gpui::test]
fn each_host_entry_lists_the_sessions_in_its_folder(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let rows = layout(app);
            assert_eq!(rows[0], "# vps (1)");
            assert_eq!(
                rows[1], "o",
                "the parent entry keeps what no closer one claims"
            );
            assert_eq!(rows[2], "# demo on vps (12)");
            assert_eq!(
                rows.len(),
                3 + GROUP_VISIBLE,
                "a group shows its newest few"
            );
            assert_eq!(rows[3], "d0", "newest first");

            app.toggle_remote_session_group("demo on vps", cx);
            assert_eq!(layout(app), ["# vps (1)", "o", "# demo on vps (12)"]);

            app.remote_hosts[0].enabled = false;
            assert_eq!(
                layout(app)[0],
                "# demo on vps (12)",
                "a switched-off entry has no group"
            );
        });
    });
}

#[gpui::test]
fn the_sidebar_lists_servers_except_while_searching(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let has_remote = |app: &OrbitApp| {
                app.sidebar_rows(&[], &HashSet::new())
                    .iter()
                    .any(|row| matches!(row, SideRow::RemoteHost { .. }))
            };
            assert!(has_remote(app));
            app.sidebar_query = "build".into();
            assert!(
                !has_remote(app),
                "a search filters this Mac's sessions only"
            );
        });
    });
}

/// Grouped by project, an entry linked to a listed local project lists inside
/// that project's group; the rest keep groups of their own at the end.
#[gpui::test]
fn a_linked_entry_lists_inside_its_local_project(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.sidebar_group_by = SidebarGroupBy::Workspace;
            app.workspaces = vec![
                PathBuf::from("/Users/me/code/demo"),
                PathBuf::from("/Users/me/code/zeta"),
            ];
            app.remote_hosts[1].local_dir = Some("/Users/me/code/demo".into());
            app.expanded_workspace_groups.insert("demo".into());
            app.expanded_workspace_groups.insert("zeta".into());
            let labels = |app: &OrbitApp| -> Vec<String> {
                app.sidebar_rows(&[], &HashSet::new())
                    .iter()
                    .filter_map(|row| match row {
                        SideRow::Workspace { label, .. } => Some(format!("@ {label}")),
                        SideRow::RemoteHost { name, .. } => Some(format!("# {name}")),
                        SideRow::RemoteSession(row) => Some(row.session.id.clone()),
                        _ => None,
                    })
                    .collect()
            };
            let rows = labels(app);
            let at = |needle: &str| rows.iter().position(|row| row == needle).expect(needle);
            assert!(
                at("@ demo") < at("# demo on vps") && at("# demo on vps") < at("d0"),
                "the linked entry sits inside its project: {rows:?}"
            );
            let demo_end = [at("@ zeta")]
                .into_iter()
                .chain(rows.iter().position(|row| row == "# vps"))
                .filter(|ix| *ix > at("@ demo"))
                .min()
                .expect("a row after the demo group");
            assert!(at("d0") < demo_end, "its sessions stay in the group");
            assert_eq!(
                rows[rows.len() - 2..],
                ["# vps".to_string(), "o".to_string()],
                "an unlinked entry keeps its own group, after the projects"
            );

            app.expanded_workspace_groups.remove("demo");
            let rows = labels(app);
            assert!(
                !rows.iter().any(|row| row == "# demo on vps" || row == "d0"),
                "a collapsed project hides its remote rows too: {rows:?}"
            );

            app.workspaces.remove(0);
            assert!(
                labels(app).iter().any(|row| row == "# demo on vps"),
                "linked to a project that is not listed, the entry keeps its own group"
            );
        });
    });
}

/// The launch and ⌘R fetches never ask: a password-only server says so on
/// its group. The refresh button may ask, through the usual prompt.
#[gpui::test]
fn a_quiet_list_notes_the_login_and_a_refresh_asks(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let vps = app.remote_hosts[0].clone();
            app.finish_remote_sessions(
                vps.clone(),
                0,
                false,
                Err(ProbeFailure::NeedsPassword { offered: true }),
                cx,
            );
            assert_eq!(
                app.remote_session_status.get(&vps.account_key()),
                Some(&RemoteListStatus {
                    loading: false,
                    error: Some(tr!("remote_sessions.needs_login")),
                })
            );
            assert!(
                app.remote_password_prompt.is_none(),
                "no prompt from a quiet list"
            );
            assert!(
                !app.remote_session_lists[&vps.account_key()].is_empty(),
                "the last list stays on screen"
            );

            app.finish_remote_sessions(
                vps.clone(),
                0,
                true,
                Err(ProbeFailure::NeedsPassword { offered: true }),
                cx,
            );
            if crate::remote_hosts::passwords_supported() {
                let prompt = app.remote_password_prompt.as_ref().expect("prompt");
                assert_eq!(prompt.then, AuthThen::SessionList("vps".into()));
            }

            app.finish_remote_sessions(vps.clone(), 0, true, Ok(vec![]), cx);
            let status = &app.remote_session_status[&vps.account_key()];
            assert_eq!(status.error, None, "a list clears the note");
        });
    });
}

#[gpui::test]
fn a_failed_resume_says_why(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let before = app.toasts.items().len();
            app.finish_remote_probe(
                host("demo on vps", "/home/deploy/demo"),
                AuthThen::ResumeSession("/p/a.jsonl".into()),
                0,
                Err(ProbeFailure::Other("Connection refused".into())),
                cx,
            );
            assert!(
                app.client.is_none() || app.remote.is_none(),
                "nothing started"
            );
            assert!(app.toasts.items().len() > before, "the failure is shown");
        });
    });
}

/// A resumed session keeps the mode it recorded; only new and respawned
/// sessions carry one at launch.
#[gpui::test]
fn a_resume_launches_without_a_workflow_mode(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.session_id = None;
            app.workflow_pending = None;
            app.remote_resuming = true;
            assert_eq!(app.remote_launch_workflow(), None);
            app.remote_resuming = false;
            assert_eq!(app.remote_launch_workflow(), Some(WorkflowMode::Build));
        });
    });
}

#[gpui::test]
fn the_server_groups_paint_in_the_sidebar(cx: &mut gpui::TestAppContext) {
    let app = app_with_hosts(cx);
    let cx = cx.add_empty_window();
    let _ = cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(1280.), px(900.)),
        |_, _| app.clone(),
    );
    assert!(
        cx.debug_bounds("remote-host-group-vps").is_some(),
        "header painted"
    );
    let header = cx
        .debug_bounds("remote-host-group-demo on vps")
        .expect("header");
    let row = cx
        .debug_bounds("remote-session-demo on vps-d0")
        .expect("session row");
    assert!(
        header.bottom() <= row.top() + px(1.),
        "rows sit under their header"
    );
}

/// On a remote session the Git page reads the host's own facts for the
/// session's folder; a read for another folder is dropped.
#[gpui::test]
fn the_git_page_shows_the_hosts_repository(cx: &mut gpui::TestAppContext) {
    use crate::remote_hosts::{RemoteGit, RemoteGitRepo};
    let app = app_with_hosts(cx);
    let demo = host("demo on vps", "/home/deploy/demo");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.remote = Some(demo.clone());
            app.git_open = true;
            app.refresh_remote_git(cx);
            assert!(app.remote_git.loading);
            app.finish_remote_git(
                &host("vps", "/home/deploy"),
                Ok(RemoteGit::NotARepository),
                cx,
            );
            assert!(
                app.remote_git.result.is_none(),
                "another folder's read is dropped"
            );
            app.finish_remote_git(
                &demo,
                Ok(RemoteGit::Repository(RemoteGitRepo {
                    branch: Some("main".into()),
                    changed: Some(2),
                    ..Default::default()
                })),
                cx,
            );
            assert!(!app.remote_git.loading);
        });
    });
    let cx = cx.add_empty_window();
    let _ = cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(1280.), px(900.)),
        |_, _| app.clone(),
    );
    assert!(
        cx.debug_bounds("remote-git-page").is_some(),
        "the host's Git card replaces the local page"
    );
}
