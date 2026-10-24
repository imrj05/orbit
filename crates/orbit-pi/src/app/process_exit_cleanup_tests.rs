//! Regression tests for issue #76, cleanup half: state that belonged to a pi
//! process must go when that process does.
//!
//! Two surfaces outlived their process. A `custom` UI (what `/review` opens)
//! stayed on screen after pi died — the extension behind it was gone, so the
//! form could never be answered, and its keys went into a dead client. And
//! `is_compacting` kept `is_running()` true, so the composer stayed on Stop over
//! a process that no longer existed.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeId};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

/// A pi that opens a custom surface and starts compacting, then dies — the
/// shape of a process that goes away with extension state still live.
struct DyingPi {
    dir: PathBuf,
    bin: PathBuf,
}

impl DyingPi {
    fn start() -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-dying-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create dying-pi dir");
        let bin = dir.join("fake-pi");
        // `custom` open → `compaction_start` → exit, so the reader sees EOF
        // and reports `ProcessExited` after the two frames.
        let script = r#"#!/bin/sh
emit() { printf '%s\n' "$1"; }
while IFS= read -r line; do
  case "$line" in
    *'"type":"prompt"'*)
      emit '{"type":"agent_start"}'
      emit '{"type":"extension_ui_request","id":"ui-1","method":"custom","phase":"open","title":"Select a review preset","lines":["Pick one"],"width":60,"height":10}'
      emit '{"type":"compaction_start"}'
      exit 0
      ;;
  esac
done
"#;
        std::fs::write(&bin, script).expect("write dying pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        Self { dir, bin }
    }
}

impl Drop for DyingPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

fn pump_until(
    app: &Entity<OrbitApp>,
    cx: &mut gpui::TestAppContext,
    done: impl Fn(&OrbitApp) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reached = app.update(cx, |app, cx| {
            app.tick(cx);
            done(app)
        });
        if reached {
            return;
        }
        assert!(Instant::now() < deadline, "the app never reached the state");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Drive the app to "pi died with a custom surface open and a compaction live".
fn die_with_extension_state(app: &Entity<OrbitApp>, cx: &mut gpui::TestAppContext, fake: &DyingPi) {
    app.update(cx, |app, _| {
        let workspace = app
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let client = PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8"), &workspace, None)
            .expect("spawn dying pi");
        app.adopt_client(client);
        app.busy = true;
        app.send(
            CommandBody::Prompt {
                message: "/review".into(),
                images: None,
                streaming_behavior: None,
            },
            "prompt",
        );
    });
    // The exit must arrive with the surface and the compaction already applied.
    pump_until(app, cx, |app| app.runtime.exited);
}

/// The custom surface goes with its process: nothing can answer it any more, and
/// leaving it up would swallow keystrokes meant for the composer.
#[gpui::test]
fn a_custom_surface_closes_when_its_process_dies(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = DyingPi::start();
    die_with_extension_state(&app, cx, &fake);

    app.update(cx, |app, _| {
        assert!(
            app.custom_ui.ids().next().is_none(),
            "a custom surface outlived the pi process that owned it"
        );
        assert!(
            !app.custom_ui_focus_pending,
            "focus must not be handed to a surface that no longer exists"
        );
    });
}

/// A dead process is not compacting and not naming a session: `is_compacting` is
/// part of `is_running()`, so leaving it set kept the composer on Stop.
#[gpui::test]
fn a_dead_process_is_not_running_anything(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let fake = DyingPi::start();
    die_with_extension_state(&app, cx, &fake);

    app.update(cx, |app, _| {
        assert!(!app.is_compacting, "compaction died with the process");
        assert!(!app.title_generating, "so did the title command");
        assert!(
            !app.is_running(),
            "nothing runs on a dead process; the composer must offer Send again"
        );
    });
}
