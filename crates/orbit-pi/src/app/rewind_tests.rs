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
emit() {{ printf '%s\n' "$1"; }}
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$LOG"
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"type":"get_fork_messages"'*)
      emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"get_fork_messages\",\"success\":true,\"data\":{{\"messages\":[{{\"entryId\":\"e1\",\"text\":\"first prompt\"}},{{\"entryId\":\"e2\",\"text\":\"second prompt\"}},{{\"entryId\":\"e3\",\"text\":\"third prompt\"}}]}}}}"
      ;;
    *'"type":"fork"'*)
      emit "{{\"type\":\"response\",\"id\":\"$id\",\"command\":\"fork\",\"success\":true,\"data\":{{\"text\":\"second prompt\",\"cancelled\":{cancelled}}}}}"
      ;;
    *'"type":"get_messages"'*)
      emit '{{"type":"response","id":"4","command":"get_messages","success":true,"data":{{"messages":[{{"role":"user","content":[{{"type":"text","text":"first prompt"}}]}}]}}}}'
      ;;
    *'"type":"get_state"'*)
      emit '{{"type":"response","id":"5","command":"get_state","success":true,"data":{{"sessionFile":"{forked}","sessionId":"forked-id"}}}}'
      ;;
    *'"type":"get_session_stats"'*)
      emit '{{"type":"response","id":"6","command":"get_session_stats","success":true,"data":{{}}}}'
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
                vec!["first prompt".to_string()],
                "the transcript is the branch pi kept, not the conversation that was left"
            );
            assert_eq!(
                app.input.read(cx).text(),
                "second prompt",
                "the forked-from prompt comes back to the composer to be edited"
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
fn successful_fork_parks_original_draft_and_attachments(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let original = PathBuf::from("/tmp/orbit-rewind-draft-original.jsonl");
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(original.clone());
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
