//! The context meter's refresh cadence: a run in flight grows the context
//! between settles, so the meter re-reads pi after each finalized message
//! instead of waiting for `agent_settled`.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeId};
use std::os::unix::fs::PermissionsExt as _;

/// A pi stand-in that opens a run, finalizes one assistant step, and then
/// answers `get_session_stats` with a 30% window — but never settles, so the
/// test can observe the meter while the run is still in flight.
struct FakePi {
    _dir: tempfile::TempDir,
    bin: PathBuf,
}

impl FakePi {
    fn start() -> Self {
        let dir = tempfile::tempdir().expect("create fake-pi dir");
        let bin = dir.path().join("fake-pi");
        std::fs::write(
            &bin,
            r#"#!/bin/sh
emit() { printf '%s\n' "$1"; }
emit '{"type":"agent_start"}'
emit '{"type":"message_start","message":{"role":"assistant"}}'
emit '{"type":"message_end","message":{"role":"assistant","content":[{"type":"text","text":"hello"}],"stopReason":"stop","usage":{"input":50000,"output":10,"cacheRead":0,"cacheWrite":0}}}'
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"type":"get_session_stats"'*)
      emit "{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_session_stats\",\"success\":true,\"data\":{\"contextUsage\":{\"tokens\":60000,\"contextWindow\":200000,\"percent\":30}}}"
      ;;
    *) emit "{\"type\":\"response\",\"id\":\"$id\",\"success\":true,\"data\":{}}" ;;
  esac
done
"#,
        )
        .expect("write fake pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake pi");
        Self { _dir: dir, bin }
    }
}

fn pump_until(
    app: &Entity<OrbitApp>,
    cx: &mut gpui::VisualTestContext,
    mut done: impl FnMut(&OrbitApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reached = cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.tick(cx);
                done(app)
            })
        });
        if reached {
            return;
        }
        assert!(Instant::now() < deadline, "the meter never refreshed");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[gpui::test]
fn the_meter_refreshes_while_a_run_is_still_in_flight(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let (app, cx) = cx.add_window_view(|_, cx| OrbitApp::new(cx));
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            let workspace = app
                .current_workspace
                .clone()
                .or_else(|| std::env::current_dir().ok())
                .unwrap_or_else(|| PathBuf::from("."));
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
                    .expect("spawn fake pi");
            app.adopt_client(client);
        });
    });
    // No `agent_settled` is ever emitted: the only thing that can move the
    // meter is the per-step refresh.
    pump_until(&app, cx, |app| {
        app.context
            .as_ref()
            .is_some_and(|u| u.percent == Some(30.0))
    });
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(
            app.busy,
            "the run must still be in flight when the meter moves"
        );
        assert!(
            app.context_stats_in_flight.is_none(),
            "the reply releases the gate"
        );
    });
}
