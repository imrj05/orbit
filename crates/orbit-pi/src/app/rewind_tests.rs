//! Rewind (fork at an earlier user message): the picker, the `fork` request,
//! and the session replacement its reply causes.
//!
//! The wire shapes below are pi 1.5's: `get_fork_messages` answers
//! `{messages:[{entryId, text}]}`, `fork` answers `{text, cancelled}` after
//! moving the process onto a branched session file, which only the follow-up
//! `get_state` names.
#![cfg(unix)]

use super::*;
use crate::commands::CommandId;
use crate::theme::{Theme, ThemeId};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

/// A pi stand-in that answers the three commands a rewind speaks, and logs
/// every line it received so a test can read back the `fork` it was given.
struct FakePi {
    dir: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

impl FakePi {
    fn start() -> Self {
        Self::with_cancelled(false)
    }

    /// `cancelled` is what an extension's `session_before_fork` handler makes
    /// pi answer instead of forking.
    fn with_cancelled(cancelled: bool) -> Self {
        Self::with_behavior(cancelled, "")
    }

    fn exiting_on(command: &str) -> Self {
        Self::with_behavior(false, command)
    }

    fn with_behavior(cancelled: bool, exit_on: &str) -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-rewind-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create fake-pi dir");
        let bin = dir.join("fake-pi");
        let log = dir.join("commands.log");
        let script = format!(
            r#"#!/bin/sh
LOG='{log}'
branched=0
emit() {{ printf '%s\n' "$1"; }}
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$LOG"
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"type":"get_fork_messages"'*)
      [ "{exit_on}" = 'get_fork_messages' ] && exit 0
      emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_fork_messages\",\"success\":true,\"data\":{{\"messages\":[{{\"entryId\":\"e1\",\"text\":\"first prompt\"}},{{\"entryId\":\"e2\",\"text\":\"second prompt\"}},{{\"entryId\":\"e3\",\"text\":\"third prompt\"}}]}}}}"
      ;;
    *'"type":"fork"'*)
      [ "{exit_on}" = 'fork' ] && exit 0
      branched=1
      entry=$(printf '%s' "$line" | sed -n 's/.*"entryId":"\([^"]*\)".*/\1/p')
      case "$entry" in
        e1) text="first prompt" ;;
        e2) text="second prompt" ;;
        *) text="third prompt" ;;
      esac
      emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"fork\",\"success\":true,\"data\":{{\"text\":\"$text\",\"cancelled\":{cancelled}}}}}"
      ;;
    *'"type":"switch_session"'*)
      branched=0
      emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"switch_session\",\"success\":true,\"data\":{{}}}}"
      ;;
    *'"type":"get_messages"'*)
      if [ "$branched" = 1 ]; then
        case "$entry" in
          e1)
            emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_messages\",\"success\":true,\"data\":{{\"messages\":[]}}}}"
            ;;
          e2)
            emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_messages\",\"success\":true,\"data\":{{\"messages\":[{{\"role\":\"user\",\"content\":\"first prompt\"}},{{\"role\":\"assistant\",\"content\":\"first answer\"}}]}}}}"
            ;;
          *)
            emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_messages\",\"success\":true,\"data\":{{\"messages\":[{{\"role\":\"user\",\"content\":\"first prompt\"}},{{\"role\":\"assistant\",\"content\":\"first answer\"}},{{\"role\":\"user\",\"content\":\"second prompt\"}},{{\"role\":\"assistant\",\"content\":\"second answer\"}}]}}}}"
            ;;
        esac
      else
        emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_messages\",\"success\":true,\"data\":{{\"messages\":[{{\"role\":\"user\",\"content\":\"first prompt\"}},{{\"role\":\"assistant\",\"content\":\"first answer\"}},{{\"role\":\"user\",\"content\":\"second prompt\"}},{{\"role\":\"assistant\",\"content\":\"second answer\"}},{{\"role\":\"user\",\"content\":\"third prompt\"}},{{\"role\":\"assistant\",\"content\":\"third answer\"}}]}}}}"
      fi
      ;;
    *'"type":"get_state"'*)
      if [ "$branched" = 1 ]; then
        emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_state\",\"success\":true,\"data\":{{\"sessionFile\":\"{forked}\",\"sessionId\":\"forked-id\"}}}}"
      else
        emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_state\",\"success\":true,\"data\":{{\"sessionId\":\"source-id\"}}}}"
      fi
      ;;
    *'"type":"get_session_stats"'*)
      emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_session_stats\",\"success\":true,\"data\":{{}}}}"
      ;;
  esac
done
"#,
            log = log.display(),
            forked = dir.join("forked.jsonl").display(),
            cancelled = if cancelled { "true" } else { "false" }
        );
        std::fs::write(&bin, script).expect("write fake pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake pi");
        Self { dir, bin, log }
    }

    /// Every command line the app sent, oldest first.
    fn commands(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn forked_session_path(&self) -> PathBuf {
        self.dir.join("forked.jsonl")
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

fn adopt_fake_pi(app: &mut OrbitApp, fake: &FakePi) {
    let workspace = app
        .current_workspace
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let client = PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
        .expect("spawn fake pi");
    app.adopt_client(client);
}

/// Three user turns with answers between them, the shape a real session has.
fn conversation() -> serde_json::Value {
    serde_json::json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "first prompt"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "first answer"}]},
            {"role": "user", "content": [{"type": "text", "text": "second prompt"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "second answer"}]},
            {"role": "user", "content": [{"type": "text", "text": "third prompt"}]},
            {"role": "assistant", "content": [{"type": "text", "text": "third answer"}]},
        ]
    })
}

/// Tick until `done` holds; the fake's replies arrive on the reader thread.
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
        assert!(Instant::now() < deadline, "the app never reached the state");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Click the real transcript footer and picker row, not the test-only picker helpers.
#[gpui::test]
fn pointer_from_transcript_to_picker_forks_the_clicked_user_turn(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let (app, cx) = cx.add_window_view(|_, cx| OrbitApp::new(cx));
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
        });
    });
    let viewport = cx.update(|window, _| window.viewport_size());
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };
    draw(cx);
    draw(cx);
    let footer = cx
        .debug_bounds("rewind-turn-2")
        .expect("second user message footer")
        .center();
    cx.simulate_mouse_move(footer, None, gpui::Modifiers::none());
    draw(cx);
    cx.simulate_click(footer, gpui::Modifiers::none());
    cx.update(|_, cx| assert!(app.read(cx).rewind_picker.is_some(), "footer click at {footer:?}, viewport {viewport:?} did not open the picker; commands: {}", fake.commands()));
    pump_until(&app, cx, |app| {
        app.rewind_picker.is_some() && !app.rewind_fetching
    });
    cx.update(|_, cx| {
        let picker = app.read(cx).rewind_picker.clone().unwrap();
        assert_eq!(picker.read(cx).highlighted_entry().as_deref(), Some("e2"));
    });
    draw(cx);
    let row = cx
        .debug_bounds("rewind-row-1")
        .expect("second fork point")
        .center();
    cx.simulate_mouse_move(row, None, gpui::Modifiers::none());
    draw(cx);
    cx.simulate_click(row, gpui::Modifiers::none());
    cx.update(|_, cx| {
        assert!(
            app.read(cx).rewind_fork_id.is_some(),
            "picker row did not send fork; commands: {}",
            fake.commands()
        )
    });
    let forked = fake.forked_session_path();
    pump_until(&app, cx, |app| {
        app.current_session_path.as_deref() == Some(forked.as_path())
    });
    assert!(
        fake.commands().contains(r#""entryId":"e2""#),
        "{}",
        fake.commands()
    );
}

#[gpui::test]
fn process_exit_during_fork_point_fetch_clears_picker_and_reconnects_to_source(
    cx: &mut gpui::TestAppContext,
) {
    let dying = FakePi::exiting_on("get_fork_messages");
    let replacement = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let source = dying.dir.join("source.jsonl");
    std::fs::write(&source, "original session").unwrap();
    let old_id = cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &dying);
            app.current_session_path = Some(source.clone());
            app.transcript.load_from(&conversation());
            app.input
                .update(cx, |input, cx| input.set_text("unfinished draft", cx));
            app.open_rewind_picker(window, cx);
            app.rewind_fetch_id.clone().expect("fetch pending")
        })
    });
    pump_until(&app, cx, |app| app.runtime.exited);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(
                app.should_auto_restart_now(),
                "the exited client must not block automatic reconnection"
            );
            assert!(app.rewind_picker.is_none());
            assert!(!app.rewind_fetching);
            assert!(app.rewind_fetch_id.is_none());
            assert_ne!(
                app.error_source.as_deref(),
                Some(REWIND_INTERRUPTED_ERROR_SOURCE)
            );
            assert_eq!(app.current_session_path.as_ref(), Some(&source));
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            app.on_response(
                "get_fork_messages",
                true,
                Some(
                    &serde_json::json!({"messages":[{"entryId":"stale", "text":"second prompt"}]}),
                ),
                None,
                &old_id,
                &mut false,
                cx,
            );
            assert!(app.rewind_picker.is_none());
            app.drop_client();
            adopt_fake_pi(app, &replacement);
            app.reattach_current_session();
        })
    });
    pump_until(&app, cx, |app| {
        app.session_id.as_deref() == Some("source-id")
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            app.rewind_to_turn(2, window, cx);
        })
    });
    pump_until(&app, cx, |app| {
        app.rewind_picker.is_some() && !app.rewind_fetching
    });
    cx.update(|_, cx| {
        let picker = app.read(cx).rewind_picker.clone().unwrap();
        assert_eq!(picker.read(cx).highlighted_entry().as_deref(), Some("e2"));
    });
    let commands = replacement.commands();
    assert!(
        commands.contains("\"type\":\"switch_session\""),
        "{commands}"
    );
    assert!(
        commands.contains(&source.to_string_lossy().to_string()),
        "{commands}"
    );
    assert_eq!(std::fs::read_to_string(source).unwrap(), "original session");
}

#[gpui::test]
fn process_exit_during_fork_keeps_source_and_draft_on_reconnect(cx: &mut gpui::TestAppContext) {
    let dying = FakePi::exiting_on("fork");
    let replacement = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let source = dying.dir.join("source.jsonl");
    std::fs::write(&source, "original session").unwrap();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &dying);
            app.current_session_path = Some(source.clone());
            app.transcript.load_from(&conversation());
            app.input
                .update(cx, |input, cx| input.set_text("unfinished draft", cx));
            app.files.push(AttachedFile {
                path: "/tmp/notes.md".into(),
                name: "notes.md".into(),
                size: 1,
            });
            app.open_rewind_picker(window, cx);
        })
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    let old_id = cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.request_rewind("e2".into(), cx);
            app.rewind_fork_id.clone().expect("fork pending")
        })
    });
    pump_until(&app, cx, |app| app.runtime.exited);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(app.rewind_fork_id.is_none());
            assert_eq!(app.error_source.as_deref(), Some("rewind_interrupted"));
            assert_eq!(
                app.error.as_deref(),
                Some(tr!("rewind.interrupted").as_str())
            );
            assert_eq!(app.current_session_path.as_ref(), Some(&source));
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            assert_eq!(app.files[0].name, "notes.md");
            app.on_response(
                "fork",
                true,
                Some(&serde_json::json!({"cancelled": false, "text": "stale"})),
                None,
                &old_id,
                &mut false,
                cx,
            );
            assert_eq!(app.current_session_path.as_ref(), Some(&source));
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            app.drop_client();
            adopt_fake_pi(app, &replacement);
            app.reattach_current_session();
        })
    });
    pump_until(&app, cx, |app| {
        app.session_id.as_deref() == Some("source-id")
    });
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert_eq!(app.current_session_path.as_ref(), Some(&source));
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            assert_eq!(app.files[0].name, "notes.md");
            assert_eq!(app.error_source.as_deref(), Some("rewind_interrupted"));
            assert_eq!(
                app.transcript.user_turn_texts(),
                vec!["first prompt", "second prompt", "third prompt"]
            );
        })
    });
    assert!(replacement
        .commands()
        .contains(&source.to_string_lossy().to_string()));
    assert_eq!(std::fs::read_to_string(source).unwrap(), "original session");
}

/// Isolate PI_BIN and HOME in a child test process; changing process-wide
/// environment variables in the normal test runner would race other tests.
#[test]
fn delayed_fork_exit_respawns_and_reattaches() {
    let replacement = FakePi::start();
    let home = replacement.dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "app::rewind_tests::delayed_fork_exit_respawns_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("ORBIT_REWIND_RESPAWN_CHILD", "1")
        .env("ORBIT_REWIND_RESPAWN_LOG", &replacement.log)
        .env("PI_BIN", &replacement.bin)
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("PATH", "/usr/bin:/bin")
        // An invalid explicit root prevents the patcher from discovering and
        // modifying the user's real Homebrew or managed pi installation.
        .env("PI_PACKAGE_ROOT", home.join("no-pi-package"))
        .env_remove("PI_MANAGED_INSTALL_ROOT")
        .output()
        .expect("run isolated respawn test");
    assert!(
        output.status.success(),
        "child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let commands = replacement.commands();
    assert!(
        commands.contains("\"type\":\"switch_session\""),
        "{commands}"
    );
    assert!(commands.contains("\"type\":\"get_messages\""), "{commands}");
}

#[gpui::test]
fn delayed_fork_exit_respawns_child(cx: &mut gpui::TestAppContext) {
    if std::env::var_os("ORBIT_REWIND_RESPAWN_CHILD").is_none() {
        return;
    }
    let dying = FakePi::exiting_on("fork");
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let source = dying.dir.join("source.jsonl");
    std::fs::write(&source, "original session").unwrap();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &dying);
            app.current_session_path = Some(source.clone());
            app.transcript.load_from(&conversation());
            // The resumed session must replace this local-only row.
            app.transcript
                .append_user_message("local-only row", Vec::new());
            app.input
                .update(cx, |input, cx| input.set_text("unfinished draft", cx));
            app.files.push(AttachedFile {
                path: "/tmp/notes.md".into(),
                name: "notes.md".into(),
                size: 1,
            });
            app.open_rewind_picker(window, cx);
        })
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    cx.update(|_, cx| app.update(cx, |app, cx| app.request_rewind("e2".into(), cx)));
    pump_until(&app, cx, |app| app.runtime.exited);
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            assert!(app.auto_restart_scheduled);
            assert!(app.rewind_fork_id.is_none());
            assert_eq!(app.current_session_path.as_ref(), Some(&source));
        })
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let reattached = cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.tick(cx);
                app.session_id.as_deref() == Some("source-id")
                    && !app.runtime.exited
                    && app.transcript.user_turn_texts().len() == 3
            })
        });
        if reattached {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "automatic respawn did not reattach the source"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(app.runtime_is_live());
            assert_eq!(
                app.error_source.as_deref(),
                Some(REWIND_INTERRUPTED_ERROR_SOURCE)
            );
            assert_eq!(app.current_session_path.as_ref(), Some(&source));
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            assert_eq!(app.files[0].name, "notes.md");
            assert_eq!(
                app.transcript.user_turn_texts(),
                vec!["first prompt", "second prompt", "third prompt"]
            );
        })
    });
    let log =
        std::fs::read_to_string(std::env::var_os("ORBIT_REWIND_RESPAWN_LOG").unwrap()).unwrap();
    assert!(log.contains(&source.to_string_lossy().to_string()), "{log}");
    assert!(log.contains("\"type\":\"get_messages\""), "{log}");
    assert_eq!(std::fs::read_to_string(source).unwrap(), "original session");
}

#[gpui::test]
fn exhausted_restart_budget_does_not_replace_the_unconfirmed_fork_warning(
    cx: &mut gpui::TestAppContext,
) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.runtime.exited = true;
            app.auto_restarts = AUTO_RESTART_LIMIT;
            app.auto_restart_scheduled = false;
            app.set_error_from(REWIND_INTERRUPTED_ERROR_SOURCE, tr!("rewind.interrupted"));
            app.on_process_exited(cx);
            assert_eq!(
                app.error_source.as_deref(),
                Some(REWIND_INTERRUPTED_ERROR_SOURCE)
            );
            assert_eq!(
                app.error.as_deref(),
                Some(tr!("rewind.interrupted").as_str())
            );
        })
    });
}

/// The palette command opens the list pi reported, with the newest message
/// chosen — rewinding usually means "the prompt I just sent".
#[gpui::test]
fn the_rewind_command_lists_pi_fork_points(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.run_command(CommandId::RewindConversation, window, cx);
            assert!(app.rewind_picker.is_some(), "the picker must open");
            assert!(app.rewind_fetching, "the list is read before it is shown");
        });
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let picker = app.rewind_picker.clone().expect("picker open");
            assert_eq!(
                picker.read(cx).highlighted_entry().as_deref(),
                Some("e3"),
                "the newest user message is the default rewind point"
            );
        });
    });
    assert!(fake.commands().contains("\"get_fork_messages\""));
}

/// A response to a dismissed picker must not fill or fail its replacement.
#[gpui::test]
fn stale_fork_message_replies_do_not_update_current_picker(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
            let current_id = app.rewind_fetch_id.clone().expect("request id");
            let mut refresh = false;
            app.on_response(
                "get_fork_messages",
                true,
                Some(&serde_json::json!({"messages":[]})),
                None,
                "old-request",
                &mut refresh,
                cx,
            );
            app.on_response(
                "get_fork_messages",
                false,
                None,
                Some("old failure"),
                "old-request",
                &mut refresh,
                cx,
            );
            assert_eq!(app.rewind_fetch_id.as_deref(), Some(current_id.as_str()));
            assert!(app.rewind_fetching);
            assert!(app.rewind_picker.is_some());
            assert!(app.error.is_none());
        });
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
}

/// Starting from a transcript row chooses that message, not the newest one.
#[gpui::test]
fn a_rewind_started_from_a_message_preselects_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            // Transcript rows are user, assistant, user: the second user
            // message is row 2, not user-turn ordinal 1.
            app.rewind_to_turn(2, window, cx);
        });
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let picker = app.rewind_picker.clone().expect("picker open");
            assert_eq!(
                picker.read(cx).highlighted_entry().as_deref(),
                Some("e2"),
                "the clicked message is preselected"
            );
        });
    });
}

#[gpui::test]
fn a_rewind_from_an_assistant_response_selects_its_user_prompt(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let opener = cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.rewind_opener(cx)
        })
    });
    // Row 3 is the answer to user row 2, not the newest turn.
    cx.update(|window, cx| opener(3, window, cx));
    pump_until(&app, cx, |app| !app.rewind_fetching);
    cx.update(|_, cx| {
        let picker = app.read(cx).rewind_picker.clone().expect("picker open");
        assert_eq!(picker.read(cx).highlighted_entry().as_deref(), Some("e2"));
    });
}

#[gpui::test]
fn an_assistant_response_with_duplicate_prompts_requires_a_choice(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let opener = cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            let mut messages = conversation();
            messages["messages"][4]["content"][0]["text"] = serde_json::json!("second prompt");
            app.transcript.load_from(&messages);
            app.rewind_opener(cx)
        })
    });
    cx.update(|window, cx| opener(3, window, cx));
    pump_until(&app, cx, |app| !app.rewind_fetching);
    cx.update(|_, cx| {
        let picker = app.read(cx).rewind_picker.clone().expect("picker open");
        assert_eq!(picker.read(cx).highlighted_entry(), None);
    });
}

/// Confirming sends `fork` with the entry id pi reported for that message.
#[gpui::test]
fn confirming_sends_a_fork_for_the_chosen_entry(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
        });
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    let picker = cx.update(|_, cx| app.read(cx).rewind_picker.clone().expect("picker open"));
    cx.update(|window, cx| {
        // One step up from the newest row: "second prompt". Confirming inside
        // the picker is what sends the fork, so the app entity must not already
        // be borrowed here.
        picker.update(cx, |picker, cx| {
            picker.step_for_test(-1, cx);
            picker.confirm_for_test(window, cx);
        });
    });
    // The picker closes synchronously; the reply is what proves the fork went
    // out, so wait for the branched session to be adopted.
    let forked = fake.forked_session_path();
    pump_until(&app, cx, |app| {
        app.current_session_path.as_deref() == Some(forked.as_path())
    });
    assert!(
        fake.commands().contains(r#""entryId":"e2""#),
        "the fork must name the chosen message's entry id, got:\n{}",
        fake.commands()
    );
    assert!(fake.commands().contains("\"type\":\"fork\""));
}

/// A successful fork is a session replacement: the branched transcript is
/// reloaded, the previous prompt is handed back to the composer, and the new
/// session file/id are adopted.
#[gpui::test]
fn a_successful_fork_reloads_the_branch_and_restores_the_prompt(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
        });
    });
    let forked = fake.forked_session_path();
    pump_until(&app, cx, |app| !app.rewind_fetching);
    let picker = cx.update(|_, cx| app.read(cx).rewind_picker.clone().expect("picker open"));
    cx.update(|window, cx| {
        picker.update(cx, |picker, cx| picker.confirm_for_test(window, cx));
    });
    pump_until(&app, cx, |app| {
        app.current_session_path.as_deref() == Some(forked.as_path())
    });
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let texts = app.transcript.user_turn_texts();
            assert_eq!(
                texts,
                vec!["first prompt".to_string(), "second prompt".to_string()],
                "the transcript is the branch pi kept, not the conversation that was left"
            );
            assert_eq!(
                app.input.read(cx).text(),
                "third prompt",
                "the forked-from (newest) prompt comes back to the composer to be edited"
            );
            assert_eq!(app.session_id.as_deref(), Some("forked-id"));
            assert!(
                app.error.is_none(),
                "a completed rewind is not a failure, got {:?}",
                app.error
            );
        });
    });
}

/// A run in flight cannot be rewound: pi would replace the session under a
/// live turn. The app says so instead of forking.
#[gpui::test]
fn a_running_session_refuses_to_rewind(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.busy = true;
            app.open_rewind_picker(window, cx);
            assert!(app.rewind_picker.is_none(), "no picker while running");
            assert!(!app.rewind_fetching, "nothing is fetched while running");
        });
    });
    assert!(!fake.commands().contains("get_fork_messages"));
}

#[gpui::test]
fn fork_confirmation_refuses_a_run_started_after_the_picker_opened(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
            app.busy = true;
            app.request_rewind("e2".into(), cx);
            assert!(app.rewind_picker.is_some());
            assert!(app.rewind_fork_id.is_none());
        });
    });
    assert!(!fake.commands().contains("\"type\":\"fork\""));
}

/// An empty conversation has nothing to fork from.
#[gpui::test]
fn an_empty_conversation_has_nothing_to_rewind(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.clear();
            app.open_rewind_picker(window, cx);
            assert!(app.rewind_picker.is_none());
        });
    });
    assert!(!fake.commands().contains("get_fork_messages"));
}

/// Switching sessions drops the picker: a row left over it would fork the
/// session that replaced the one the list was read from.
#[gpui::test]
fn a_session_change_closes_the_rewind_picker(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
            assert!(app.rewind_picker.is_some());
            app.reset_turns();
            assert!(app.rewind_picker.is_none());
            assert!(!app.rewind_fetching);
            assert!(app.rewind_preselect.is_none());
        });
    });
}

#[gpui::test]
fn dropping_the_process_invalidates_a_pending_fork_picker(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
            assert!(app.rewind_fetching);
            app.drop_client();
            assert!(app.rewind_picker.is_none());
            assert!(!app.rewind_fetching);
            assert!(app.rewind_fetch_id.is_none());
            assert!(app.rewind_preselect.is_none());
        });
    });
}

#[gpui::test]
fn successful_fork_parks_original_draft_and_attachments(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let original = PathBuf::from("/tmp/orbit-rewind-draft-original.jsonl");
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(original.clone());
            app.composer_is_new_task = false;
            app.input
                .update(cx, |input, cx| input.set_text("unfinished draft", cx));
            app.files.push(AttachedFile {
                path: PathBuf::from("/tmp/notes.md"),
                name: "notes.md".into(),
                size: 12,
            });
            app.pasted_texts.push(PastedText::new("long paste".into()));
            // A paste may still be held by the input when the fork response
            // arrives, before the app's next heartbeat drains it.
            app.input.update(cx, |input, _| {
                input.pasted_texts.push("pending paste".into())
            });
            app.attachments.push(Attachment {
                name: "image.png".into(),
                mime: "image/png".into(),
                data: "payload".into(),
                preview: None,
                original_len: 7,
                stored_len: 7,
                compressed: false,
            });
            app.on_fork_response(
                Some(&serde_json::json!({"text":"old prompt", "cancelled":false})),
                &mut false,
                cx,
            );
            assert_eq!(app.input.read(cx).text(), "old prompt");
            assert!(
                app.files.is_empty() && app.attachments.is_empty() && app.pasted_texts.is_empty()
            );
            app.restore_composer_draft(Some(&original), cx);
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            assert_eq!(app.files[0].name, "notes.md");
            assert_eq!(app.pasted_texts[0].content, "long paste");
            assert_eq!(app.pasted_texts[1].content, "pending paste");
            assert_eq!(app.attachments[0].data, "payload");
        });
    });
}

#[gpui::test]
fn successful_fork_without_known_original_path_keeps_the_draft_in_place(
    cx: &mut gpui::TestAppContext,
) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = None;
            app.input
                .update(cx, |input, cx| input.set_text("unsaved", cx));
            app.files.push(AttachedFile {
                path: "/tmp/a".into(),
                name: "a".into(),
                size: 1,
            });
            app.on_fork_response(
                Some(&serde_json::json!({"text":"old prompt", "cancelled":false})),
                &mut false,
                cx,
            );
            assert_eq!(app.input.read(cx).text(), "unsaved");
            assert_eq!(app.files[0].name, "a");
        });
    });
}

#[gpui::test]
fn cancelled_fork_keeps_the_original_draft_and_attachments(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let original = PathBuf::from("/tmp/orbit-rewind-cancel-draft.jsonl");
            app.current_session_path = Some(original.clone());
            app.input
                .update(cx, |input, cx| input.set_text("draft", cx));
            app.files.push(AttachedFile {
                path: "/tmp/a".into(),
                name: "a".into(),
                size: 1,
            });
            app.on_fork_response(Some(&serde_json::json!({"cancelled":true})), &mut false, cx);
            assert_eq!(app.current_session_path.as_ref(), Some(&original));
            assert_eq!(app.input.read(cx).text(), "draft");
            assert_eq!(app.files[0].name, "a");
            assert!(!app.drafts.contains_key(&original));
        });
    });
}

#[gpui::test]
fn an_in_flight_fork_blocks_session_replacement_and_prompt_submission(
    cx: &mut gpui::TestAppContext,
) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let original = PathBuf::from("/tmp/orbit-fork-source.jsonl");
            app.current_session_path = Some(original.clone());
            app.rewind_fork_id = Some("fork-id".into());
            app.input
                .update(cx, |input, cx| input.set_text("draft", cx));
            app.on_new_session(&crate::NewSession, window, cx);
            app.start_task_in_folder_quiet(PathBuf::from("/tmp"), cx);
            app.runtime_stop(cx);
            app.runtime_restart(cx);
            app.submit_as("draft".into(), SendMode::FollowUp, cx);
            assert_eq!(app.current_session_path.as_ref(), Some(&original));
            assert_eq!(app.input.read(cx).text(), "draft");
            let mut refresh = false;
            app.on_response(
                "fork",
                false,
                None,
                Some("old"),
                "other-id",
                &mut refresh,
                cx,
            );
            assert_eq!(app.rewind_fork_id.as_deref(), Some("fork-id"));
            app.on_response(
                "fork",
                false,
                None,
                Some("failed"),
                "fork-id",
                &mut refresh,
                cx,
            );
            assert!(app.rewind_fork_id.is_none());
            assert_eq!(app.input.read(cx).text(), "draft");
        });
    });
}

/// A quit requested while pi is forking waits for the reply that moves the
/// process onto the branch; a second request goes now, so a fork that never
/// answers cannot trap the app or leave the updater's helper waiting.
#[gpui::test]
fn quitting_waits_for_a_pending_fork_and_a_second_request_goes_now(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            assert_eq!(app.quit_plan(), crate::app::QuitPlan::Now);
            app.rewind_fork_id = Some("fork-id".into());
            assert_eq!(app.quit_plan(), crate::app::QuitPlan::Wait);
            assert!(app.quit_when_fork_settles);
            assert_eq!(
                app.quit_plan(),
                crate::app::QuitPlan::Now,
                "the second request skips the wait"
            );
        });
    });
}

/// The heartbeat releases the deferred quit only once the branch has been
/// adopted, and a lost fork releases it instead of waiting forever.
#[gpui::test]
fn a_deferred_quit_waits_for_the_fork_and_the_resend_it_queues(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.rewind_fork_id = Some("fork-id".into());
            assert_eq!(app.quit_plan(), crate::app::QuitPlan::Wait);
            assert!(!app.take_deferred_quit());

            // A regenerate's resend is queued after the fork reply; quitting
            // between the two would leave a branch it never resent.
            app.rewind_fork_id = None;
            app.pending_regenerate = Some(PendingRegenerate {
                state_id: "state-1".into(),
                text: "prompt".into(),
                images: Vec::new(),
            });
            assert!(!app.take_deferred_quit());

            app.pending_regenerate = None;
            assert!(app.take_deferred_quit());
            assert!(!app.quit_when_fork_settles);
            assert!(
                !app.take_deferred_quit(),
                "a deferred quit is released once"
            );
            app.tick(cx);
            assert!(
                !app.quit_when_fork_settles,
                "the heartbeat has nothing left to quit for"
            );
        });
    });
}

/// A fork whose reply is lost to a teardown must not hold the deferred quit:
/// pi may have written the branch, but there is no reply left to apply.
#[gpui::test]
fn a_lost_fork_releases_a_deferred_quit(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.rewind_fork_id = Some("fork-id".into());
            assert_eq!(app.quit_plan(), crate::app::QuitPlan::Wait);
            assert!(app.take_pending_fork(), "the lost reply is reported");
            assert!(app.take_deferred_quit());
        });
    });
}

/// A cancelled fork leaves the session exactly as it was: pi never wrote a
/// branch, so nothing is reloaded and no session file moves.
#[gpui::test]
fn a_cancelled_fork_changes_nothing(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::with_cancelled(true);
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
        });
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    let picker = cx.update(|_, cx| app.read(cx).rewind_picker.clone().expect("picker open"));
    cx.update(|window, cx| {
        picker.update(cx, |picker, cx| picker.confirm_for_test(window, cx));
    });
    // The reply carries `cancelled`; wait for the fork line to be spoken, then
    // let the app drain it.
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fake.commands().contains("\"type\":\"fork\"") {
        assert!(Instant::now() < deadline, "the fork never went out");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(100));
    pump_until(&app, cx, |_| true);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert_eq!(
                app.transcript.user_turn_texts(),
                vec![
                    "first prompt".to_string(),
                    "second prompt".to_string(),
                    "third prompt".to_string()
                ],
                "a cancelled fork keeps the conversation"
            );
            assert!(
                app.current_session_path.is_none(),
                "a cancelled fork adopts no new session file"
            );
            assert_eq!(app.input.read(cx).text(), "", "the composer is untouched");
        });
    });
}

/// Without a live pi there is no list to show. The app reports the missing
/// process instead of leaving a picker stuck on its loading row.
#[gpui::test]
fn without_a_pi_process_the_picker_does_not_open(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.client = None;
            app.transcript.load_from(&conversation());
            app.open_rewind_picker(window, cx);
            assert!(app.rewind_picker.is_none());
            assert!(!app.rewind_fetching);
            assert!(app.rewind_preselect.is_none());
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("pi")),
                "the missing pi is reported as a toast"
            );
        });
    });
}

// ── regenerate (fork at the response's prompt, then send it again) ──────

/// pi has no regenerate command: the response's prompt is forked, and pi's
/// own copy of it is sent again once the branch has loaded. The prompt must
/// come after the snapshot's state reply, or `get_messages` would replace the
/// optimistic user row.
#[gpui::test]
fn regenerating_a_response_forks_its_prompt_and_resends_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let opener = cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.regenerate_opener(cx)
        })
    });
    // Row 3 is the answer to row 2's prompt, not the newest turn.
    cx.update(|window, cx| opener(3, window, cx));
    let forked = fake.forked_session_path();
    pump_until(&app, cx, |app| {
        app.current_session_path.as_deref() == Some(forked.as_path())
            && app.busy
            && app.pending_regenerate.is_none()
            && fake.commands().contains("\"type\":\"prompt\"")
    });

    let commands = fake.commands();
    assert!(
        commands.contains("\"entryId\":\"e2\""),
        "the fork must name the response's own prompt, got:\n{commands}"
    );
    let fork_at = commands.find("\"type\":\"fork\"").expect("fork was sent");
    let prompt_at = commands
        .find("\"type\":\"prompt\"")
        .expect("the prompt was sent again");
    assert!(
        prompt_at > fork_at,
        "the resend must follow the fork, got:\n{commands}"
    );
    assert!(
        commands[prompt_at..].contains("second prompt"),
        "the resend must carry pi's copy of the prompt, got:\n{commands}"
    );
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert!(
            app.rewind_picker.is_none(),
            "a regenerate chooses its message without the list"
        );
        assert_eq!(
            app.input.read(cx).text(),
            "",
            "the resend clears the composer"
        );
        assert_eq!(
            app.transcript.user_turn_texts(),
            vec!["first prompt".to_string(), "second prompt".to_string()],
            "the branch keeps its earlier turn and shows the resent one"
        );
    });
}

/// Duplicate prompt text cannot identify one pi entry. Regenerate refuses
/// instead of forking a different prompt; the rewind list is the honest path.
#[gpui::test]
fn regenerating_with_duplicate_prompts_is_refused(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            adopt_fake_pi(app, &fake);
            let mut messages = conversation();
            messages["messages"][2]["content"][0]["text"] = serde_json::json!("first prompt");
            app.transcript.load_from(&messages);
        });
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.regenerate_turn(3, window, cx));
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    assert!(!fake.commands().contains("\"type\":\"fork\""));
    assert!(!fake.commands().contains("\"type\":\"prompt\""));
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.rewind_intent, ForkIntent::Edit, "no stale intent");
        assert!(
            app.toasts
                .items()
                .iter()
                .any(|toast| toast.title.contains("cannot be identified")),
            "the refusal is explained"
        );
    });
}

/// A run in flight owns the process; regenerating would replace the session
/// under it, so Orbit asks for a stop instead.
#[gpui::test]
fn regenerating_a_running_session_is_refused(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
            app.busy = true;
            app.regenerate_turn(3, window, cx);
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("Stop the run")),
                "the stop is requested"
            );
        });
    });
    assert!(!fake.commands().contains("get_fork_messages"));
}

/// Compaction replaces the prompt behind an early response; there is nothing
/// reliable to resend, and Orbit says so rather than forking the wrong entry.
#[gpui::test]
fn regenerating_a_response_before_a_summary_is_refused(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&serde_json::json!({"messages": [
                {"role":"compactionSummary", "summary":"earlier work", "tokensBefore":100},
                {"role":"assistant", "content":"orphan answer"},
                {"role":"user", "content":"second prompt"},
                {"role":"assistant", "content":"second answer"}
            ]}));
            app.regenerate_turn(1, window, cx);
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("no longer visible")),
                "the hidden prompt is reported"
            );
        });
    });
    assert!(!fake.commands().contains("get_fork_messages"));
}

#[gpui::test]
fn a_cancelled_regenerate_sends_no_prompt(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::with_cancelled(true);
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
        });
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.regenerate_turn(3, window, cx));
    });
    pump_until(&app, cx, |app| {
        app.toasts
            .items()
            .iter()
            .any(|toast| toast.title.contains("cancelled"))
    });
    assert!(fake.commands().contains("\"type\":\"fork\""));
    assert!(!fake.commands().contains("\"type\":\"prompt\""));
    cx.update(|_, cx| {
        assert!(app.read(cx).pending_regenerate.is_none());
        assert_eq!(app.read(cx).current_session_path, None);
    });
}

/// Without a known source session the composer may hold a different draft.
/// A regenerate keeps it rather than fold its attachments into a resent
/// prompt, and reports that the new session is waiting.
#[gpui::test]
fn a_regenerate_keeps_an_unsaved_draft_and_sends_nothing(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = None;
            app.rewind_intent = ForkIntent::Regenerate;
            app.input
                .update(cx, |input, cx| input.set_text("unfinished draft", cx));
            app.files.push(AttachedFile {
                path: PathBuf::from("/tmp/notes.md"),
                name: "notes.md".into(),
                size: 12,
            });
            let mut refresh = false;
            app.on_fork_response(
                Some(&serde_json::json!({"text":"old prompt","cancelled":false})),
                &mut refresh,
                cx,
            );
            assert_eq!(app.input.read(cx).text(), "unfinished draft");
            assert_eq!(app.files[0].name, "notes.md");
            assert!(app.pending_regenerate.is_none());
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("unsaved draft")),
                "the skipped resend is explained"
            );
        });
    });
}

/// The queued prompt belongs to one specific state reload; a later, unrelated
/// `get_state` must not start it.
#[gpui::test]
fn a_queued_regenerate_fires_only_for_its_own_state_reply(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.pending_regenerate = Some(PendingRegenerate {
                state_id: "wanted".into(),
                text: "second prompt".into(),
                images: Vec::new(),
            });
            let mut refresh = false;
            app.on_response(
                "get_state",
                true,
                Some(&serde_json::json!({})),
                None,
                "unrelated",
                &mut refresh,
                cx,
            );
            assert!(app.pending_regenerate.is_some());
            assert!(app.transcript.user_turn_texts().is_empty());
            app.on_response(
                "get_state",
                true,
                Some(&serde_json::json!({})),
                None,
                "wanted",
                &mut refresh,
                cx,
            );
            assert!(
                app.pending_regenerate.is_none(),
                "the matching reply consumes the queued prompt"
            );
        });
    });
}

#[gpui::test]
fn a_manual_send_supersedes_a_queued_regenerate(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.pending_regenerate = Some(PendingRegenerate {
                state_id: "wanted".into(),
                text: "second prompt".into(),
                images: Vec::new(),
            });
            app.submit_as("typed instead".into(), SendMode::FollowUp, cx);
            assert!(app.pending_regenerate.is_none());
        });
    });
}

/// An error-only turn has no response text, so the error card's Retry is the
/// only way back. It is the same fork-and-resend: the prompt behind the failed
/// response is sent again on a new branch.
#[gpui::test]
fn retrying_an_error_only_turn_resends_its_prompt(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let opener = cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&serde_json::json!({"messages": [
                {"role":"user", "content":"first prompt"},
                {
                    "role":"assistant",
                    "content":[],
                    "stopReason":"error",
                    "errorMessage":"provider overloaded"
                }
            ]}));
            app.regenerate_opener(cx)
        })
    });
    cx.update(|window, cx| opener(1, window, cx));
    let forked = fake.forked_session_path();
    pump_until(&app, cx, |app| {
        app.current_session_path.as_deref() == Some(forked.as_path())
            && app.busy
            && app.pending_regenerate.is_none()
            && fake.commands().contains("\"type\":\"prompt\"")
    });
    let commands = fake.commands();
    assert!(
        commands.contains("\"entryId\":\"e1\""),
        "the fork must name the failed turn's prompt, got:\n{commands}"
    );
    assert!(
        commands.contains("\"message\":\"first prompt\""),
        "the retry resends pi's copy of the prompt, got:\n{commands}"
    );
    cx.update(|_, cx| {
        assert_eq!(
            app.read(cx).transcript.user_turn_texts(),
            vec!["first prompt".to_string()],
            "the failed answer row is not carried onto the new branch"
        );
    });
}

/// A prompt can carry images. `fork` hands back only the text, so the images
/// are captured from the transcript before the fork replaces it and ride the
/// resent prompt.
#[gpui::test]
fn regenerating_an_image_prompt_resends_its_images(cx: &mut gpui::TestAppContext) {
    let prompt = serde_json::json!({"messages": [
        {"role": "user", "content": [
            {"type": "text", "text": "first prompt"},
            {"type": "image", "mimeType": "image/png", "data": "aGVsbG8="}
        ]},
        {"role": "assistant", "content": "first answer"}
    ]});
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let opener = cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&prompt);
            app.regenerate_opener(cx)
        })
    });
    cx.update(|window, cx| opener(1, window, cx));
    pump_until(&app, cx, |_| {
        fake.commands().contains("\"type\":\"prompt\"")
            && fake.commands().contains("\"data\":\"aGVsbG8=\"")
    });
    let commands = fake.commands();
    let resend = commands
        .lines()
        .find(|line| line.contains("\"type\":\"prompt\""))
        .expect("the prompt was resent");
    assert!(
        resend.contains("\"images\":["),
        "the resend must carry the prompt's images, got:\n{resend}"
    );
    assert!(
        resend.contains("\"mimeType\":\"image/png\""),
        "the image keeps its media type, got:\n{resend}"
    );
    assert!(resend.contains("\"message\":\"first prompt\""));
    cx.update(|_, cx| {
        assert_eq!(
            app.read(cx).transcript.user_turn_images(0).len(),
            1,
            "the resent prompt shows its image in the transcript"
        );
    });
}

/// A text-only prompt must not grow an empty `images` payload.
#[gpui::test]
fn regenerating_a_text_prompt_omits_images(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&conversation());
        });
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.regenerate_turn(3, window, cx));
    });
    pump_until(&app, cx, |_| {
        fake.commands().contains("\"type\":\"prompt\"")
    });
    let commands = fake.commands();
    let resend = commands
        .lines()
        .find(|line| line.contains("\"type\":\"prompt\""))
        .expect("the prompt was resent");
    assert!(
        !resend.contains("\"images\""),
        "a text-only prompt carries no images field, got:\n{resend}"
    );
}

/// Rewind hands the prompt back for editing; images stay out of the composer
/// because a picker row can name a different entry than the one that opened it.
#[gpui::test]
fn rewinding_an_image_prompt_hands_back_text_only(cx: &mut gpui::TestAppContext) {
    let prompt = serde_json::json!({"messages": [
        {"role": "user", "content": [
            {"type": "text", "text": "first prompt"},
            {"type": "image", "mimeType": "image/png", "data": "aGVsbG8="}
        ]},
        {"role": "assistant", "content": "first answer"}
    ]});
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.transcript.load_from(&prompt);
            app.rewind_to_turn(1, window, cx);
        });
    });
    pump_until(&app, cx, |app| !app.rewind_fetching);
    let picker = cx.update(|_, cx| app.read(cx).rewind_picker.clone().expect("picker open"));
    cx.update(|window, cx| {
        picker.update(cx, |picker, cx| picker.confirm_for_test(window, cx));
    });
    let forked = fake.forked_session_path();
    pump_until(&app, cx, |app| {
        app.current_session_path.as_deref() == Some(forked.as_path())
    });
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.input.read(cx).text(), "first prompt");
        assert!(
            app.attachments.is_empty(),
            "rewind leaves images to the person editing"
        );
    });
    assert!(!fake.commands().contains("\"type\":\"prompt\""));
}

// ── process teardown and restart paths ────────────────────────────────

/// Any teardown outside the event loop can lose the fork reply. It must say
/// so and reload the list, exactly as a process exit does.
#[gpui::test]
fn a_lost_fork_reply_warns_and_refreshes_the_session_list(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.rewind_fork_id = Some("fork-id".into());
            app.rewind_intent = ForkIntent::Regenerate;
            app.rewind_preselect = Some(0);
            app.pending_regenerate = Some(PendingRegenerate {
                state_id: "state".into(),
                text: "prompt".into(),
                images: Vec::new(),
            });
            app.drop_client();
            assert!(app.rewind_fork_id.is_none());
            assert!(app.rewind_preselect.is_none());
            assert_eq!(app.rewind_intent, ForkIntent::Edit);
            assert!(app.pending_regenerate.is_none());
            assert_eq!(
                app.error_source.as_deref(),
                Some(REWIND_INTERRUPTED_ERROR_SOURCE)
            );
            assert!(
                app.error
                    .as_deref()
                    .is_some_and(|error| error.contains("before confirming the fork")),
                "the lost reply is explained, got {:?}",
                app.error
            );
            assert!(app.sessions_refresh_pending);
        });
    });
    // The flag is consumed even when no client and no event follows.
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.tick(cx);
            assert!(!app.sessions_refresh_pending);
        });
    });
}

#[gpui::test]
fn a_pending_fork_defers_the_mcp_restart(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.current_session_path = Some(PathBuf::from("/tmp/orbit-mcp-fork.jsonl"));
            app.session_id = Some("session".into());
            app.rewind_fork_id = Some("fork-id".into());
            app.mcp_apply_forced = true;
            app.mcp_apply_now(cx);
            assert!(app.mcp_apply_pending, "the apply stays queued");
            assert!(app.mcp_apply_at.is_some(), "the debounce is re-armed");
            assert_eq!(app.rewind_fork_id.as_deref(), Some("fork-id"));
            assert!(app.client.is_some(), "the process was not replaced");
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("conversation fork")),
                "the deferral is explained"
            );
        });
    });
    assert!(
        !fake.commands().contains("\"type\":\"switch_session\""),
        "no restart may switch the session mid-fork"
    );
}

#[gpui::test]
fn a_pending_fork_refuses_the_credential_restart(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let original = PathBuf::from("/tmp/orbit-credentials-fork.jsonl");
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.current_session_path = Some(original.clone());
            app.session_id = Some("session".into());
            app.rewind_fork_id = Some("fork-id".into());
            app.provider_apply_credentials(cx);
            assert!(app.client.is_some(), "the process was not replaced");
            assert_eq!(app.rewind_fork_id.as_deref(), Some("fork-id"));
            assert_eq!(app.current_session_path.as_ref(), Some(&original));
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("conversation fork")),
                "the refusal is explained"
            );
        });
    });
    assert!(!fake.commands().contains("\"type\":\"switch_session\""));
}

#[gpui::test]
fn the_catalog_restart_rechecks_the_process_before_dropping_it(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.current_session_path = Some(PathBuf::from("/tmp/orbit-catalog.jsonl"));
            app.session_id = Some("session".into());

            // The refresh finished while a fork is in flight.
            app.rewind_fork_id = Some("fork-id".into());
            app.restart_pi_for_catalog(cx);
            assert!(app.client.is_some(), "the process was not replaced");
            assert_eq!(app.rewind_fork_id.as_deref(), Some("fork-id"));

            // The refresh finished while a run started after the click.
            app.rewind_fork_id = None;
            app.busy = true;
            app.restart_pi_for_catalog(cx);
            assert!(app.client.is_some(), "a live run keeps its process");
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("Finish the current turn")),
                "the busy refusal is explained"
            );
        });
    });
    assert!(!fake.commands().contains("\"type\":\"switch_session\""));
}

/// Model, thinking and compaction commands would be applied to the branch a
/// pending fork is about to open, not to the session the control described.
#[gpui::test]
fn a_pending_fork_refuses_model_thinking_and_compaction(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.rewind_fork_id = Some("fork-id".into());
            app.set_model("gpt-5".into(), "openai".into(), cx);
            app.set_thinking_level("high".into(), cx);
            app.compact_now(cx);
            assert_eq!(app.rewind_fork_id.as_deref(), Some("fork-id"));
            assert!(!app.is_compacting, "no compaction was started");
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("conversation fork")),
                "the refusal is explained"
            );
        });
    });
    let commands = fake.commands();
    assert!(!commands.contains("\"type\":\"set_model\""), "{commands}");
    assert!(
        !commands.contains("\"type\":\"set_thinking_level\""),
        "{commands}"
    );
    assert!(!commands.contains("\"type\":\"compact\""), "{commands}");
}

/// Rename and title generation both rewrite the session the fork reply is
/// about to replace; both wait for the fork instead.
#[gpui::test]
fn a_pending_fork_refuses_rename_and_title_generation(cx: &mut gpui::TestAppContext) {
    let fake = FakePi::start();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            adopt_fake_pi(app, &fake);
            app.rewind_fork_id = Some("fork-id".into());
            app.session_id = Some("session".into());
            app.slash_commands = vec![SlashCommand {
                name: "generate-title".into(),
                description: String::new(),
                scope: crate::mentions::CommandScope::Orbit,
            }];
            app.session_name_input
                .update(cx, |input, cx| input.set_text("Renamed", cx));
            app.rename_session(cx);
            app.generate_session_title(cx);
            assert_eq!(app.rewind_fork_id.as_deref(), Some("fork-id"));
            assert!(app.pending_session_rename.is_none());
            assert!(app.session_name.is_none(), "no optimistic rename");
            assert!(!app.title_generating);
            assert!(
                app.toasts
                    .items()
                    .iter()
                    .any(|toast| toast.title.contains("conversation fork")),
                "the refusal is explained"
            );
        });
    });
    let commands = fake.commands();
    assert!(
        !commands.contains("\"type\":\"set_session_name\""),
        "{commands}"
    );
    assert!(!commands.contains("/generate-title"), "{commands}");
}
