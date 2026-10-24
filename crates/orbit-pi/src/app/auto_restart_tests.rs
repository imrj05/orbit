//! Regression tests for issue #76, recovery half: after the pi process died,
//! Orbit left a banner up and every later command and shortcut went nowhere —
//! the user had to find Reconnect in Settings before the app did anything
//! again. The app now respawns pi on its own, so the next command just works.
//!
//! The budget exists because "respawn automatically" without a limit turns one
//! broken pi into a crash loop. It refills only for a process that actually ran
//! ([`AUTO_RESTART_HEALTHY_UPTIME`]), never per attempt.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeId};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

/// Mark the active process as having started `uptime` ago and gone: the client
/// is dropped (which kills the child) and the runtime reads as exited. `new`
/// spawns pi, so an exit has to be modeled rather than assumed.
fn started_ago(app: &mut OrbitApp, uptime: Duration) {
    app.client = None;
    app.runtime.started_at = Some(Instant::now() - uptime);
    app.runtime.alive = false;
    app.runtime.exited = true;
}

/// A crash loop is bounded: three respawns, then the app stops trying and the
/// Reconnect banner is the way back.
#[gpui::test]
fn a_crash_loop_is_respawned_at_most_three_times(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            // Each start dies almost immediately, so no attempt refills the
            // budget: this is one crash loop, not four failures.
            for attempt in 0..AUTO_RESTART_LIMIT {
                started_ago(app, Duration::from_millis(50));
                assert!(
                    app.take_auto_restart(),
                    "attempt {attempt} is within the budget and must respawn"
                );
            }
            started_ago(app, Duration::from_millis(50));
            assert!(
                !app.take_auto_restart(),
                "a fourth exit in the same loop must stop and leave the banner"
            );
        });
    });
}

/// One broken pi cannot burn the budget forever: a process that stayed up long
/// enough to answer commands refills it, so the next failure recovers too.
#[gpui::test]
fn a_healthy_run_refills_the_respawn_budget(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            for _ in 0..AUTO_RESTART_LIMIT {
                started_ago(app, Duration::from_millis(50));
                assert!(app.take_auto_restart());
            }
            started_ago(app, Duration::from_millis(50));
            assert!(!app.take_auto_restart(), "budget spent");

            // This one ran for a while before dying: a fresh failure.
            started_ago(app, AUTO_RESTART_HEALTHY_UPTIME + Duration::from_secs(1));
            assert!(
                app.take_auto_restart(),
                "a process that stayed up was healthy; its death is not part of \
                 the earlier crash loop"
            );
        });
    });
}

/// Only one respawn may be in flight: one death arrives as two events (the
/// writer reports a dead stdin, the reader its EOF), and the second must not
/// spend another attempt or raise a banner over a recovery on its way.
#[gpui::test]
fn an_exit_that_lands_twice_schedules_one_respawn(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            started_ago(app, Duration::from_secs(30));
            app.on_process_exited(cx);
            assert!(
                app.auto_restart_scheduled,
                "the first exit queues a respawn"
            );

            app.on_process_exited(cx);
            assert!(
                app.auto_restart_scheduled,
                "the queued respawn is still the only one; it is not queued twice"
            );
            assert_eq!(
                app.auto_restarts, 1,
                "a duplicate exit event must not spend a second attempt"
            );
            assert!(
                app.error.is_none(),
                "a banner must not cover a respawn that is already on its way"
            );
        });
    });
}

/// Once the budget is spent the app stops respawning and says so, so the
/// Reconnect button is what the user sees instead of a silent dead app.
#[gpui::test]
fn an_exhausted_budget_raises_the_banner(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            for _ in 0..AUTO_RESTART_LIMIT {
                started_ago(app, Duration::from_millis(50));
                app.on_process_exited(cx);
                // The queued respawn fires and starts pi again; model that so the
                // next exit is a separate decision.
                app.auto_restart_scheduled = false;
                assert!(app.error.is_none(), "a respawn is still expected");
            }

            started_ago(app, Duration::from_millis(50));
            app.on_process_exited(cx);
            assert!(
                app.error.is_some(),
                "with the budget spent the failure must be visible and actionable"
            );
            assert!(
                !app.auto_restart_scheduled,
                "and no fourth respawn is queued behind it"
            );
            assert!(
                app.error_source.is_none(),
                "an exit is raised by the process, not by a command"
            );
        });
    });
}

/// The user wins the race: a Stop in the half-second before the respawn fires
/// clears `exited`, so nothing is spawned behind their back.
#[gpui::test]
fn a_queued_respawn_yields_to_a_stop(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            started_ago(app, Duration::from_secs(30));
            assert!(
                app.should_auto_restart_now(),
                "the runtime is gone and idle"
            );

            // What `runtime_stop`/`drop_client` leaves: no client, and a runtime
            // status that no longer reads as exited.
            app.runtime = RuntimeStatus::default();
            assert!(
                !app.should_auto_restart_now(),
                "a stopped runtime must not be respawned by the queued callback"
            );
        });
    });
}

/// A pi stand-in that records every command it is handed, so a test can assert
/// what the app said to a freshly spawned process.
struct LoggingPi {
    dir: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

impl LoggingPi {
    fn start() -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-restart-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create logging-pi dir");
        let bin = dir.join("fake-pi");
        let log = dir.join("stdin.log");
        std::fs::write(&log, "").expect("create log");
        let script = format!(
            r#"#!/bin/sh
while IFS= read -r line; do
  printf '%s\n' "$line" >> "{}"
done
"#,
            log.display()
        );
        std::fs::write(&bin, script).expect("write logging pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir, bin, log }
    }
}

impl Drop for LoggingPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A restart — automatic after a crash, or the Restart/Reconnect button — must
/// put the new process back on the session the window is showing. A fresh
/// launch opens a *new* session, so without the reattach pi would answer on an
/// empty one while the transcript kept rendering the old conversation, and the
/// next prompt would reach a model with none of it.
#[gpui::test]
fn a_restart_points_the_new_process_back_at_the_session(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = LoggingPi::start();
    let session = fake.dir.join("session.jsonl");
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let workspace = app
                .current_workspace
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8"), &workspace, None)
                    .expect("spawn logging pi");
            app.adopt_client(client);
            app.current_session_path = Some(session.clone());
            // What `runtime_restart` does once the new process is up.
            app.reattach_current_session();
        });
    });

    let expected = session.to_string_lossy().into_owned();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let log = std::fs::read_to_string(&fake.log).unwrap_or_default();
        if log.contains("switch_session") {
            assert!(
                log.contains(&expected),
                "the respawn must be pointed at the session on screen, got {log}"
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the restarted process was never told which session to open; got {log}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
