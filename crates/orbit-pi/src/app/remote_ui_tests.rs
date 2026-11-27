//! Remote (SSH) sessions in the app: the picker rows, and the local-disk
//! surfaces staying closed while the agent runs on another machine.

use super::*;
use crate::remote_hosts::RemoteHost;
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

fn host(name: &str) -> RemoteHost {
    RemoteHost {
        name: name.into(),
        ssh: format!("{name}.example"),
        user: None,
        port: None,
        dir: "/srv/app".into(),
        enabled: true,
        local_dir: None,
    }
}

/// Hosts are a *where*, so they live in the Work in picker only; the
/// new-task folder picker lists folders and nothing else.
#[gpui::test]
fn hosts_are_offered_by_work_in_not_by_the_folder_picker(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _cx| {
            app.work_in_hosts = vec![host("a"), host("b")];
            app.sessions = vec![];
            app.workspaces = vec![];
            let folder = std::env::temp_dir();
            app.current_workspace = Some(folder.clone());
            let entries = app.workspace_entries();
            assert!(
                entries.iter().all(|entry| entry.path.is_dir()),
                "every folder-picker row is a local folder"
            );
            assert_eq!(entries.len(), 1, "no host rows among the folders");
        });
    });
}

#[gpui::test]
fn leaving_a_remote_session_clears_it_instead_of_parking(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _cx| {
            app.client = None;
            app.remote = Some(host("vps"));
            app.park_active_session();
            assert!(
                app.remote.is_none(),
                "remote state must not outlive the session"
            );
        });
    });
}

#[gpui::test]
fn explorer_and_review_stay_closed_for_a_remote_session(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.remote = Some(host("vps"));
            app.toggle_project_panel(cx);
            app.toggle_side_pane(cx);
            app.open_review(cx);
            assert!(
                !app.project_panel.read(cx).is_open(),
                "explorer reads the local disk"
            );
            assert!(
                !app.sidepane.read(cx).is_open(),
                "review reads the local disk"
            );
        });
    });
}

#[gpui::test]
fn file_mentions_are_off_for_a_remote_session(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.current_workspace = std::env::current_dir().ok();
            app.remote = Some(host("vps"));
            assert!(
                app.at_paths("", cx).is_empty(),
                "local paths are not on the host"
            );
        });
    });
}

fn attached(name: &str) -> AttachedFile {
    AttachedFile {
        path: PathBuf::from("/Users/me/project").join(name),
        name: name.into(),
        size: 12,
    }
}

/// A file rides as a local path the remote agent cannot open: it is refused
/// on the way into the tray, and a tray filled before the session went
/// remote is held back at send instead of sending dead mentions.
#[gpui::test]
fn file_attachments_are_refused_for_a_remote_session(cx: &mut gpui::TestAppContext) {
    let refused = |app: &OrbitApp| {
        let expected = tr!("remote.files_unavailable", host = "vps");
        app.toasts
            .items()
            .iter()
            .any(|toast| toast.title == expected)
    };
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.push_file(attached("local.txt"), cx);
            assert_eq!(app.files.len(), 1, "a local session takes files");
            assert!(!refused(app));

            app.remote = Some(host("vps"));
            app.push_file(attached("notes.md"), cx);
            assert_eq!(
                app.files.len(),
                1,
                "the remote session refused the new file"
            );
            assert!(refused(app), "and said why");
        });
    });

    // A tray filled before the session went remote is held at send.
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.files.push(attached("local.txt"));
            app.remote = Some(host("vps"));
            app.submit_as("read this".into(), SendMode::FollowUp, cx);
            assert_eq!(app.files.len(), 1, "the file stays in the tray");
            assert!(refused(app), "the send was refused with the reason");
        });
    });
}

/// The host is pinned to Supervised whatever this Mac's mode is, so the
/// menu that would change this Mac's mode stays shut while remote.
#[gpui::test]
fn the_access_menu_stays_shut_for_a_remote_session(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.remote = Some(host("vps"));
            app.toggle_access_menu(window, cx);
            assert!(!app.access_menu_open, "remote: no mode menu");

            app.remote = None;
            app.toggle_access_menu(window, cx);
            assert!(app.access_menu_open, "local: the menu opens");
        });
    });
}

/// A remote pi starts in the New Task choice, keeps the live mode when an
/// existing session respawns, and otherwise starts in Build.
#[gpui::test]
fn a_remote_launch_carries_the_right_workflow_mode(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.remote = Some(host("vps"));
            app.session_id = None;
            app.workflow_pending = None;
            app.workflow_mode = WorkflowMode::Plan;
            assert_eq!(
                app.remote_launch_workflow(),
                Some(WorkflowMode::Build),
                "a new session never inherits the last session's mode"
            );
            app.workflow_pending = Some(WorkflowMode::Ask);
            assert_eq!(app.remote_launch_workflow(), Some(WorkflowMode::Ask));
            app.workflow_pending = None;
            app.session_id = Some("remote-1".into());
            assert_eq!(
                app.remote_launch_workflow(),
                None,
                "missing store evidence falls through to the session branch"
            );
        });
    });
}

// ── Settings → Remote hosts ───────────────────────────────────────────────
// These never save: `remote_hosts::upsert` writes the real config file, and
// its writes are covered against temp files in `remote_hosts`' own tests.

fn settings_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            app.settings_open = true;
            app.set_settings_section(SettingsSection::RemoteHosts, cx);
        });
    });
    app
}

fn draw(cx: &mut gpui::VisualTestContext, app: &Entity<OrbitApp>) {
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1100.), px(760.)),
        |_, _| app.clone(),
    );
}

#[gpui::test]
fn the_page_lists_each_host_with_its_actions(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.remote_hosts = vec![host("alpha"), host("beta")];
            app.remote_hosts_skipped.clear();
        })
    });
    draw(cx, &app);
    assert!(
        cx.debug_bounds("remote-host-add").is_some(),
        "the toolbar offers Add host"
    );
    let alpha = cx.debug_bounds("remote-host-row-alpha").expect("alpha row");
    let beta = cx.debug_bounds("remote-host-row-beta").expect("beta row");
    assert!(alpha.bottom() <= beta.top() + px(1.), "rows stack in order");
}

#[gpui::test]
fn the_form_reports_a_problem_instead_of_saving(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.remote_host_add(window, cx);
            let editor = app.remote_host_editor.as_ref().expect("form open");
            editor
                .ssh
                .update(cx, |input, cx| input.set_text("vps".to_string(), cx));
            // No directory: validation stops before anything is written.
            app.remote_host_editor_save(cx);
            let editor = app.remote_host_editor.as_ref().expect("form stays open");
            assert_eq!(
                editor.error.as_deref(),
                Some(tr!("remote_hosts.error_dir_required").as_str())
            );
        })
    });
    draw(cx, &app);
    assert!(
        cx.debug_bounds("remote-host-editor").is_some(),
        "the modal laid out"
    );
}

#[gpui::test]
fn picking_an_alias_fills_the_host_and_an_empty_name(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.remote_host_add(window, cx);
            let editor = app.remote_host_editor.as_mut().expect("form open");
            editor.aliases = vec!["devbox".into(), "prod".into()];
            app.remote_host_pick_alias("devbox".into(), cx);
            let editor = app.remote_host_editor.as_ref().expect("form open");
            assert_eq!(editor.ssh.read(cx).text().to_string(), "devbox");
            assert_eq!(editor.name.read(cx).text().to_string(), "devbox");

            // A name the user typed is theirs: picking again leaves it.
            editor
                .name
                .update(cx, |input, cx| input.set_text("Work box".to_string(), cx));
            app.remote_host_pick_alias("prod".into(), cx);
            let editor = app.remote_host_editor.as_ref().expect("form open");
            assert_eq!(editor.ssh.read(cx).text().to_string(), "prod");
            assert_eq!(editor.name.read(cx).text().to_string(), "Work box");
        })
    });
}

#[gpui::test]
fn leaving_the_section_discards_the_form(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.remote_host_add(window, cx);
            app.set_settings_section(SettingsSection::General, cx);
            assert!(app.remote_host_editor.is_none());
        })
    });
}

// ── SSH password flow ─────────────────────────────────────────────────────
// Driven from probe outcomes: no test here starts an ssh process.

use super::remote_hosts_ui::HostCheck;
use super::ssh_auth_ui::AuthThen;
use crate::remote_hosts::ProbeFailure;

/// An app with one saved host whose row is mid-check, so a result for it
/// still has somewhere to land.
fn checking_row(cx: &mut gpui::TestAppContext) -> (Entity<OrbitApp>, RemoteHost) {
    let app = test_app(cx);
    let saved = host("vps");
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.remote_hosts = vec![saved.clone()];
            app.remote_host_checks
                .insert(saved.name.clone(), HostCheck::Checking);
        })
    });
    (app, saved)
}

#[cfg(unix)]
#[gpui::test]
fn a_refused_key_asks_for_a_password_and_cancel_reports_it(cx: &mut gpui::TestAppContext) {
    let (app, saved) = checking_row(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                0,
                Err(ProbeFailure::NeedsPassword { offered: true }),
                cx,
            );
            let prompt = app.remote_password_prompt.as_ref().expect("password asked");
            assert_eq!(prompt.attempt, 0);
            assert!(prompt.error.is_none());
            assert!(app.remote_password_focus_pending, "the field takes focus");

            // An empty password is refused in place, not sent.
            app.remote_password_submit(cx);
            let prompt = app.remote_password_prompt.as_ref().expect("still open");
            assert_eq!(
                prompt.error.as_deref(),
                Some(tr!("remote_hosts.password_required").as_str())
            );
            assert!(app.remote_host_secrets.is_empty(), "nothing remembered");

            app.remote_password_cancel(cx);
            assert!(app.remote_password_prompt.is_none());
            assert_eq!(
                app.remote_host_checks.get(&saved.name),
                Some(&HostCheck::Failed(tr!("remote_hosts.password_cancelled")))
            );
        })
    });
}

#[cfg(unix)]
#[gpui::test]
fn a_wrong_password_is_forgotten_and_asked_again_until_the_limit(cx: &mut gpui::TestAppContext) {
    let (app, saved) = checking_row(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.remote_host_secrets
                .insert(saved.account_key(), "wrong".into());
            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                1,
                Err(ProbeFailure::WrongPassword),
                cx,
            );
            assert!(
                app.remote_host_secrets.is_empty(),
                "a refused password is dropped"
            );
            let prompt = app.remote_password_prompt.take().expect("asked again");
            assert_eq!(
                prompt.error.as_deref(),
                Some(tr!("remote_hosts.wrong_password").as_str())
            );

            // The last try: report instead of asking a fourth time.
            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                3,
                Err(ProbeFailure::WrongPassword),
                cx,
            );
            assert!(app.remote_password_prompt.is_none());
            assert_eq!(
                app.remote_host_checks.get(&saved.name),
                Some(&HostCheck::Failed(tr!("remote_hosts.wrong_password")))
            );
        })
    });
}

#[gpui::test]
fn keys_only_servers_and_host_keys_are_explained_not_prompted(cx: &mut gpui::TestAppContext) {
    let (app, saved) = checking_row(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                0,
                Err(ProbeFailure::NeedsPassword { offered: false }),
                cx,
            );
            assert!(app.remote_password_prompt.is_none());
            assert_eq!(
                app.remote_host_checks.get(&saved.name),
                Some(&HostCheck::Failed(tr!("remote_hosts.keys_only")))
            );

            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                0,
                Err(ProbeFailure::HostKey(
                    orbit_rpc::remote::HostKeyProblem::Unknown,
                )),
                cx,
            );
            assert!(
                app.remote_password_prompt.is_none(),
                "host keys are never trusted here"
            );
            let Some(HostCheck::Failed(message)) = app.remote_host_checks.get(&saved.name) else {
                panic!("a failure is shown");
            };
            assert!(
                message.contains("ssh vps.example"),
                "names the command: {message}"
            );
        })
    });
}

#[gpui::test]
fn a_result_for_an_older_form_or_a_failed_start_changes_nothing(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.remote_host_add(window, cx);
            app.remote_host_editor.as_mut().expect("form").check_gen = 6;
            app.finish_remote_probe(
                host("vps"),
                AuthThen::EditorTest(5),
                0,
                Ok("pi 1.0".into()),
                cx,
            );
            assert!(
                app.remote_host_editor
                    .as_ref()
                    .expect("form")
                    .check
                    .is_none(),
                "a stale result is dropped"
            );

            app.finish_remote_probe(
                host("vps"),
                AuthThen::StartTask,
                0,
                Err(ProbeFailure::Other("orbit: pi is not on the PATH".into())),
                cx,
            );
            assert!(app.remote.is_none(), "no session starts on a failed check");
        })
    });
}

#[gpui::test]
fn a_cloud_root_guard_offers_the_account_it_asks_for(cx: &mut gpui::TestAppContext) {
    let (app, saved) = checking_row(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                0,
                Err(ProbeFailure::LoginRefused {
                    suggested: Some("opc".into()),
                    message: "Please login as the user \"opc\" rather than the user \"root\"."
                        .into(),
                }),
                cx,
            );
            assert!(
                app.remote_password_prompt.is_none(),
                "no password can fix this"
            );
            let Some(HostCheck::WrongUser { user, message }) =
                app.remote_host_checks.get(&saved.name)
            else {
                panic!("the row offers a fix");
            };
            assert_eq!(user, "opc");
            assert!(
                message.contains("opc") && message.contains("root"),
                "{message}"
            );
        })
    });
}

#[gpui::test]
fn a_missing_pi_offers_the_installer_command(cx: &mut gpui::TestAppContext) {
    let (app, saved) = checking_row(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.finish_remote_probe(
                saved.clone(),
                AuthThen::RowTest(saved.name.clone()),
                0,
                Err(ProbeFailure::PiMissing),
                cx,
            );
            let Some(HostCheck::PiMissing { command }) = app.remote_host_checks.get(&saved.name)
            else {
                panic!("the row says pi is missing");
            };
            assert_eq!(command, &saved.install_command());
            assert!(command.contains("https://pi.dev/install.sh"));
        })
    });
}

// ── Remote hosts cards ────────────────────────────────────────────────────

#[gpui::test]
fn the_card_menu_opens_closes_and_leaves_with_the_section(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.toggle_remote_host_menu("vps".into(), cx);
            assert_eq!(app.remote_host_menu.as_deref(), Some("vps"));
            app.toggle_remote_host_menu("vps".into(), cx);
            assert!(app.remote_host_menu.is_none(), "a second press closes it");
            app.toggle_remote_host_menu("vps".into(), cx);
            // Pressing ⋯ while its menu is open: mouse-down dismisses it as an
            // outside click, and that press's mouse-up must not re-open it.
            app.remote_host_menu = None;
            app.menu_dismissed_at.arm();
            app.toggle_remote_host_menu("vps".into(), cx);
            assert!(
                app.remote_host_menu.is_none(),
                "the click that dismissed the menu does not re-open it"
            );
            app.toggle_remote_host_menu("vps".into(), cx);
            assert_eq!(
                app.remote_host_menu.as_deref(),
                Some("vps"),
                "the next press opens it"
            );
            app.set_settings_section(SettingsSection::General, cx);
            assert!(app.remote_host_menu.is_none(), "leaving the page closes it");
        })
    });
}

#[gpui::test]
fn a_failed_install_lands_on_the_row(cx: &mut gpui::TestAppContext) {
    let (app, saved) = checking_row(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.remote_host_checks
                .insert(saved.name.clone(), HostCheck::Installing);
            app.finish_remote_install(
                saved.clone(),
                super::ssh_auth_ui::InstallTarget::Row(saved.name.clone()),
                0,
                Err(ProbeFailure::Other("orbit: curl is required".into())),
                cx,
            );
            assert_eq!(
                app.remote_host_checks.get(&saved.name),
                Some(&HostCheck::Failed(tr!(
                    "remote_hosts.install_failed",
                    error = "orbit: curl is required"
                )))
            );
        })
    });
}

#[gpui::test]
fn a_missing_pi_card_offers_install_with_buttons_at_their_own_width(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            let saved = host("vps");
            app.remote_host_checks.insert(
                saved.name.clone(),
                HostCheck::PiMissing {
                    command: saved.install_command(),
                },
            );
            app.remote_hosts = vec![saved];
            app.remote_hosts_skipped.clear();
        })
    });
    draw(cx, &app);
    let card = cx
        .debug_bounds("remote-host-row-vps")
        .expect("the card laid out");
    let actions = cx
        .debug_bounds("remote-host-install-actions-vps")
        .expect("Install pi and Copy command laid out");
    assert!(
        card.contains(&actions.origin),
        "the actions sit inside the card"
    );
    assert!(
        actions.size.height < px(60.),
        "two buttons on one line, not stacked full-width: {actions:?}"
    );
}

/// Settings → Providers says which machine its controls act on during a
/// remote session, and says nothing otherwise.
#[gpui::test]
fn the_providers_page_names_the_host_only_while_remote(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.set_settings_section(SettingsSection::Providers, cx);
        })
    });
    draw(cx, &app);
    assert!(cx.debug_bounds("providers-remote-notice").is_none());

    cx.update(|_, cx| app.update(cx, |app, _| app.remote = Some(host("vps"))));
    draw(cx, &app);
    assert!(
        cx.debug_bounds("providers-remote-notice").is_some(),
        "a remote session names its host on the Providers page"
    );
}

// ── Providers & models per machine ────────────────────────────────────────
// State is set directly: selecting a machine through the UI would run
// `pi --list-models` over ssh.

use super::remote_providers_ui::RemoteModels;
use crate::remote_hosts::ListedModel;

fn listed(provider: &str, id: &str) -> ListedModel {
    ListedModel {
        provider: provider.into(),
        id: id.into(),
        context: "200K".into(),
        max_output: "64K".into(),
        thinking: true,
        images: true,
    }
}

fn providers_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    let app = settings_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.settings_section = SettingsSection::Providers;
            app.remote_hosts = vec![host("vps")];
            app.remote_hosts_skipped.clear();
            app.providers_machine = None;
            cx.notify();
        })
    });
    app
}

#[gpui::test]
fn the_machine_selector_appears_only_with_an_enabled_host(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    let cx = cx.add_empty_window();
    draw(cx, &app);
    assert!(
        cx.debug_bounds("providers-machine").is_some(),
        "a host to choose"
    );

    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.remote_hosts[0].enabled = false;
        })
    });
    draw(cx, &app);
    assert!(
        cx.debug_bounds("providers-machine").is_none(),
        "a switched-off host is not a choice"
    );
}

#[gpui::test]
fn a_selected_host_shows_its_own_signed_in_providers(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.providers_machine = Some("vps".into());
            app.remote_model_lists.insert(
                "vps".into(),
                RemoteModels::Loaded(vec![
                    listed("anthropic", "claude-a"),
                    listed("anthropic", "claude-b"),
                    listed("openai", "gpt-a"),
                ]),
            );
        })
    });
    draw(cx, &app);
    assert!(cx.debug_bounds("providers-remote-anthropic").is_some());
    assert!(cx.debug_bounds("providers-remote-openai").is_some());
    assert!(cx.debug_bounds("providers-remote-empty").is_none());

    // Nothing signed in there: the empty state offers Sign in instead.
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.remote_model_lists
                .insert("vps".into(), RemoteModels::Loaded(Vec::new()));
        })
    });
    draw(cx, &app);
    assert!(cx.debug_bounds("providers-remote-empty").is_some());
    assert!(cx.debug_bounds("providers-remote-anthropic").is_none());
}

#[gpui::test]
fn a_host_listing_routes_each_outcome(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.remote_model_lists
                .insert("vps".into(), RemoteModels::Loading);
            app.finish_remote_model_list(host("vps"), 0, Ok(vec![listed("openai", "gpt-a")]), cx);
            assert!(matches!(
                app.remote_model_lists.get("vps"),
                Some(RemoteModels::Loaded(models)) if models.len() == 1
            ));

            app.finish_remote_model_list(host("vps"), 0, Err(ProbeFailure::PiMissing), cx);
            assert_eq!(
                app.remote_model_lists.get("vps"),
                Some(&RemoteModels::PiMissing)
            );

            app.finish_remote_model_list(
                host("vps"),
                0,
                Err(ProbeFailure::Other("orbit: timed out".into())),
                cx,
            );
            assert_eq!(
                app.remote_model_lists.get("vps"),
                Some(&RemoteModels::Failed("orbit: timed out".into()))
            );
        })
    });
}

#[cfg(unix)]
#[gpui::test]
fn a_host_listing_that_wants_a_password_asks_for_it(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.remote_model_lists
                .insert("vps".into(), RemoteModels::Loading);
            app.finish_remote_model_list(
                host("vps"),
                0,
                Err(ProbeFailure::NeedsPassword { offered: true }),
                cx,
            );
            let prompt = app.remote_password_prompt.as_ref().expect("password asked");
            assert_eq!(prompt.then, AuthThen::ProviderList("vps".into()));
        })
    });
}

// ── Add API key on a host ─────────────────────────────────────────────────

#[gpui::test]
fn the_key_dialog_refuses_until_a_provider_and_key_are_given(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_remote_key_editor(host("vps"), window, cx);
            // No provider chosen.
            app.save_remote_key(false, cx);
            let editor = app.remote_key_editor.as_ref().expect("dialog open");
            assert_eq!(
                editor.error.as_deref(),
                Some(tr!("providers_remote.key_pick_provider").as_str())
            );
            assert!(!editor.saving, "nothing was sent");

            // A provider, but no key.
            app.remote_key_editor.as_mut().unwrap().provider = Some("openrouter");
            app.save_remote_key(false, cx);
            let editor = app.remote_key_editor.as_ref().expect("dialog open");
            assert_eq!(
                editor.error.as_deref(),
                Some(tr!("providers.api_key_required").as_str())
            );
            assert!(!editor.saving);
        })
    });
    draw(cx, &app);
    assert!(
        cx.debug_bounds("remote-key-dialog").is_some(),
        "the dialog laid out"
    );
}

#[gpui::test]
fn a_failed_key_save_explains_itself_in_the_dialog(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_remote_key_editor(host("vps"), window, cx);
            app.remote_key_editor.as_mut().unwrap().saving = true;
            app.finish_remote_key(
                host("vps"),
                "openrouter",
                Err(ProbeFailure::Other(
                    "orbit: auth.json on the server is not valid JSON; fix it by hand first".into(),
                )),
                cx,
            );
            let editor = app.remote_key_editor.as_ref().expect("dialog stays open");
            assert!(!editor.saving);
            assert!(editor
                .error
                .as_deref()
                .unwrap_or_default()
                .contains("not valid JSON"));
        })
    });
}

#[gpui::test]
fn leaving_the_section_closes_the_key_dialog(cx: &mut gpui::TestAppContext) {
    let app = providers_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_remote_key_editor(host("vps"), window, cx);
            app.set_settings_section(SettingsSection::General, cx);
            assert!(app.remote_key_editor.is_none());
        })
    });
}

/// The card menu is as wide as its longest label, so "Add a project from this
/// Mac…" is not cut off, and it still hangs from the ⋯ button's right edge.
#[gpui::test]
fn the_card_menu_fits_its_longest_label(cx: &mut gpui::TestAppContext) {
    let app = settings_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.remote_hosts = vec![host("vps")];
            app.remote_hosts_skipped.clear();
            app.remote_host_menu = Some("vps".into());
        })
    });
    draw(cx, &app);
    let menu = cx.debug_bounds("remote-host-menu-vps").expect("menu open");
    let more = cx.debug_bounds("remote-host-more-vps").expect("⋯ button");
    let theme = cx.update(|_, cx| *cx.global::<crate::theme::Theme>());
    let label = cx.update(|window, _| {
        crate::ui::style::text::text_width(
            window,
            &tr!("server_clone.menu_item"),
            crate::theme::tokens::context_menu::TEXT.px(&theme),
        )
    });
    let icon = crate::theme::tokens::context_menu::ICON.px(&theme);
    assert!(
        menu.size.width >= label + icon * 2.,
        "menu {:?} is narrower than its label {label:?} plus its icon",
        menu.size.width
    );
    assert!(
        (menu.right() - more.right()).abs() <= px(1.),
        "the menu stays aligned to the ⋯ button's right edge"
    );
}

/// A real press on ⋯ toggles its menu both ways. The press on an open menu's
/// button is an outside click to the menu (mouse-down dismisses it), and the
/// same press's mouse-up must not re-open it.
#[gpui::test]
fn pressing_the_more_button_toggles_the_card_menu(cx: &mut gpui::TestAppContext) {
    /// The app as the window's root, so a click is hit-tested against the real
    /// frame (an empty window repaints its empty root before dispatching).
    struct Root(Entity<OrbitApp>);
    impl Render for Root {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.0.clone()
        }
    }

    let app = settings_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.remote_hosts = vec![host("vps")];
            app.remote_hosts_skipped.clear();
        })
    });
    let root = app.clone();
    let (_, cx) = cx.add_window_view(|_, _| Root(root));
    cx.run_until_parked();
    let menu_open = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| app.read(cx).remote_host_menu.is_some())
    };
    let more = cx
        .debug_bounds("remote-host-more-vps")
        .expect("⋯ button")
        .center();

    cx.simulate_click(more, gpui::Modifiers::none());
    assert!(menu_open(cx), "the first press opens the menu");
    cx.run_until_parked();
    cx.simulate_click(more, gpui::Modifiers::none());
    assert!(!menu_open(cx), "a second press on ⋯ closes it");
    cx.run_until_parked();
    cx.simulate_click(more, gpui::Modifiers::none());
    assert!(menu_open(cx), "and the next press opens it again");
}
