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
    }
}

#[test]
fn picker_lists_hosts_only_without_a_setup_row() {
    let empty = OrbitApp::remote_actions(&[]);
    assert_eq!(
        empty.iter().map(|a| a.id).collect::<Vec<_>>(),
        Vec::<&str>::new(),
        "with no hosts there is no machine row (host config lives in Settings)"
    );

    let rows = OrbitApp::remote_actions(&[host("a"), host("b")]);
    assert_eq!(
        rows.iter().map(|a| a.id).collect::<Vec<_>>(),
        ["remote-host", "remote-host"]
    );
    assert_eq!(rows[0].hint.as_deref(), Some("a.example:/srv/app"));
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

#[test]
fn remote_rows_open_their_own_group_and_set_locations_in_mono() {
    let with_hosts = OrbitApp::remote_actions(&[host("a"), host("b")]);
    assert!(with_hosts[0].group_start, "the first host opens the group");
    assert!(!with_hosts[1].group_start);
    assert!(with_hosts[0].mono && with_hosts[1].mono);

    assert!(
        OrbitApp::remote_actions(&[]).is_empty(),
        "with no hosts the machine group is gone entirely"
    );
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
            app.remote_host_remove_confirm = Some("alpha".into());
            app.set_settings_section(SettingsSection::General, cx);
            assert!(app.remote_host_editor.is_none());
            assert!(app.remote_host_remove_confirm.is_none());
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
