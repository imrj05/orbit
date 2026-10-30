//! Regression test: the global Refresh (⌘R) started as "Refresh Sessions"
//! but must now reach every live surface, not just the session list.

use super::*;
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

#[gpui::test]
fn refresh_reaches_every_open_surface(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            // No agent: the RPC-backed refreshes are skipped, so this test
            // exercises only the local surfaces.
            app.client = None;

            // Worktrees: an open page re-lists (busy flips synchronously).
            app.worktrees_open = true;
            assert!(!app.worktrees_busy);
            // MCP: a probe re-reads the config, clearing the external-change
            // notice that prompted the refresh.
            app.settings_open = true;
            app.settings_section = SettingsSection::Mcp;
            app.mcp_external_change = true;

            app.on_refresh(&crate::RefreshSessions, window, cx);

            assert!(
                app.worktrees_busy,
                "Refresh must re-list the open Worktrees page"
            );
            assert!(
                !app.mcp_external_change,
                "Refresh must re-read the MCP config"
            );
        });
    });
}

#[gpui::test]
fn git_accounts_refresh_coalesces_and_invalidates_old_work(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.settings_open = true;
            app.settings_section = SettingsSection::GitAccounts;
            assert!(!app.git_accounts_loading);
            assert!(app.git_accounts_snapshot.is_none());
            assert_eq!(app.gh_latest, GhUpdateStatus::NotChecked);
            app.on_refresh(&crate::RefreshSessions, window, cx);
            assert_eq!(
                app.gh_latest,
                GhUpdateStatus::NotChecked,
                "local refresh must not check the network"
            );
            assert!(app.git_accounts_loading);
            let first = app.git_accounts_generation;
            app.on_refresh(&crate::RefreshSessions, window, cx);
            assert_eq!(
                app.git_accounts_generation, first,
                "duplicate refresh must coalesce"
            );
            app.refresh_git_accounts_for_workspace(cx);
            assert_ne!(
                app.git_accounts_generation, first,
                "workspace refresh invalidates older results"
            );
            assert!(app.git_accounts_snapshot.is_none());
        });
    });
}

/// A refresh with every page closed still re-reads the config catalogs, but
/// it must not start a worktree list just because ⌘R was pressed.
#[gpui::test]
fn refresh_is_quiet_with_no_page_open(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.worktrees_open = false;
            app.settings_open = false;
            app.mcp_probe_at = None;
            app.on_refresh(&crate::RefreshSessions, window, cx);
            assert!(!app.worktrees_busy, "no worktree list without the page");
            assert!(
                app.mcp_probe_at.is_none(),
                "no MCP server probe without the page"
            );
        });
    });
}

/// The global Refresh is a full resync, not a repaint of whichever Settings
/// page happens to be open: packages written to the workspace's `.pi`
/// settings are picked up with the Plugins page closed.
#[gpui::test]
fn refresh_rereads_plugins_with_the_plugins_page_closed(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let workspace = std::env::temp_dir().join(format!(
        "orbit-refresh-plugins-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let settings = workspace.join(".pi/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, r#"{"packages":["local:./extensions/demo"]}"#).unwrap();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.settings_open = false;
            app.current_workspace = Some(workspace.clone());
            assert!(app.plugins.is_empty());

            app.on_refresh(&crate::RefreshSessions, window, cx);

            assert!(
                app.plugins
                    .iter()
                    .any(|package| package.source == "local:./extensions/demo"),
                "Refresh must re-read the workspace plugin list"
            );
        });
    });
    let _ = std::fs::remove_dir_all(&workspace);
}
