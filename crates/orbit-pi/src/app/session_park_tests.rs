//! Regression tests for issue #46: switching away from a session must park its
//! process, never drop it. A session that has not claimed its file yet (pi is
//! still booting, or the `new_session` reply is in flight) parks into the
//! pending pool and re-keys into `lives` once the boot handshake names the
//! file — otherwise the next `adopt_client` overwrites (and kills) a live run.
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeId};
use std::os::unix::fs::PermissionsExt as _;

/// A fake pi that swallows stdin, so the transport sees a live child without
/// spawning the real CLI. Deleted when the test ends.
struct FakePi {
    dir: PathBuf,
    bin: PathBuf,
}

impl FakePi {
    fn start(tag: &str) -> Self {
        Self::install(tag, "#!/bin/sh\nexec cat >/dev/null\n".to_string())
    }

    /// A pi that is slow to answer `new_session` (the boot window where the
    /// session file is unknown), then names its file on `get_state`.
    fn slow_boot(tag: &str, session_path: &str) -> Self {
        let script = format!(
            r#"#!/bin/sh
emit() {{ printf '%s\n' "$1" | sed "s/^{{/{{\"id\":\"$id\",/"; }}
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"type":"new_session"'*)
      sleep 1
      emit '{{"type":"response","command":"new_session","success":true,"data":{{"cancelled":false}}}}'
      ;;
    *'"type":"get_state"'*)
      emit '{{"type":"response","command":"get_state","success":true,"data":{{"sessionFile":"{session}","sessionId":"sid"}}}}'
      ;;
    *) emit '{{"type":"response","success":true}}' ;;
  esac
done
"#,
            session = session_path
        );
        Self::install(tag, script)
    }

    fn install(tag: &str, script: String) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "orbit-park-{tag}-{}-{:?}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create fake-pi dir");
        let bin = dir.join("fake-pi");
        std::fs::write(&bin, script).expect("write fake pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake pi");
        Self { dir, bin }
    }
}

impl Drop for FakePi {
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

fn spawn_fake(app: &mut OrbitApp, fake: &FakePi) -> PiClient {
    let workspace = app
        .current_workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
        .expect("spawn fake pi")
}

fn child_alive(pid: u32) -> bool {
    std::process::Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Drive the app's heartbeat until `done` holds or the timeout lapses.
fn pump_until(
    cx: &mut gpui::TestAppContext,
    app: &Entity<OrbitApp>,
    timeout: Duration,
    done: impl Fn(&OrbitApp) -> bool,
) {
    let deadline = Instant::now() + timeout;
    loop {
        cx.update(|cx| app.update(cx, |app, cx| app.tick(cx)));
        if cx.update(|cx| done(app.read(cx))) || Instant::now() > deadline {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A claimed session parks straight into the warm pool, as before.
#[gpui::test]
fn claimed_running_session_parks_into_the_warm_pool(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let session = PathBuf::from("/tmp/orbit-park-tests/claimed.jsonl");
    let fake = FakePi::start("claimed");
    let mut pid = 0;
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &fake);
            pid = client.child_pid();
            app.adopt_client(client);
            app.current_session_path = Some(session.clone());
            app.busy = true;

            app.park_active_session();

            assert!(
                app.pending_parks.is_empty(),
                "no pending entry for a claimed file"
            );
            let parked = app.lives.get(&session).expect("parked under its path");
            assert!(parked.busy, "a mid-run session parks busy");
            assert!(child_alive(pid), "the parked process must stay alive");
        });
    });
}

/// An unclaimed running session parks into the pending pool; the next
/// `adopt_client` cannot kill it, and the boot handshake re-keys it into
/// `lives` under its file.
#[gpui::test]
fn unclaimed_running_session_parks_pending_and_rekeys(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let session = "/tmp/orbit-park-tests/pending.jsonl";
    let running = FakePi::slow_boot("pending", session);
    let replacement = FakePi::start("pending-replacement");
    let mut running_pid = 0;
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let client = spawn_fake(app, &running);
            running_pid = client.child_pid();
            app.adopt_client(client);
            app.current_session_path = None;
            app.busy = true;
            app.send(CommandBody::NewSession, "new_session");

            app.park_active_session();

            assert_eq!(
                app.pending_parks.len(),
                1,
                "an unclaimed session must park pending, not stay active"
            );
            assert!(app.pending_parks[0].busy, "a mid-run session parks busy");
            assert!(app.lives.is_empty(), "no file path to key it by yet");

            // The kill scenario from #46: adopting the incoming session used
            // to overwrite (and drop) the outgoing client.
            let incoming = spawn_fake(app, &replacement);
            app.adopt_client(incoming);
            assert_eq!(app.pending_parks.len(), 1);
            assert!(
                child_alive(running_pid),
                "the pending run's process must survive the adopt"
            );
        });
    });

    pump_until(cx, &app, Duration::from_secs(5), |app| {
        app.pending_parks.is_empty() && app.lives.contains_key(Path::new(session))
    });

    cx.update(|cx| {
        app.update(cx, |app, _| {
            assert!(
                app.pending_parks.is_empty(),
                "the boot handshake must re-key it"
            );
            let parked = app
                .lives
                .get(Path::new(session))
                .expect("re-keyed into lives under its session file");
            assert!(parked.busy, "the run stays busy across the re-key");
            assert!(
                child_alive(running_pid),
                "the run's process must stay alive"
            );
        });
    });
}
