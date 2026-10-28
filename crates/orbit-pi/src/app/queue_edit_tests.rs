//! Regression tests for issue #79: a pending steer or follow-up message can be
//! edited before pi delivers it, and can be pushed ahead of the rest of the
//! queue.
//!
//! pi's protocol has no per-message queue command — `queue_update` reports the
//! lanes as bare strings and `clear_queue` drops everything at once — so an
//! edit or a promotion is a rebuild: clear, then re-queue the survivors in
//! their lanes and order. These tests pin the wire sequence that rebuild
//! produces and the mirror the bar reads, because both are easy to get subtly
//! wrong (a dropped message, a lane swap, the stale `clear_queue` echo
//! resurrecting the old rows).
#![cfg(unix)]

use super::*;
use crate::theme::{Theme, ThemeId};
use std::os::unix::fs::PermissionsExt as _;
use std::sync::atomic::{AtomicU64, Ordering};

/// A pi stand-in that records every command line it receives, so a test can
/// assert on the exact sequence Orbit put on the wire.
struct LoggingPi {
    dir: PathBuf,
    bin: PathBuf,
    log: PathBuf,
}

impl LoggingPi {
    fn start(tag: &str) -> Self {
        static NEXT_SCRATCH: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "orbit-queue-{tag}-{}-{}",
            std::process::id(),
            NEXT_SCRATCH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("create logging-pi dir");
        let bin = dir.join("logging-pi");
        let log = dir.join("commands.jsonl");
        let script = format!(
            "#!/bin/sh\nwhile IFS= read -r line; do\n  printf '%s\\n' \"$line\" >> '{log}'\ndone\n",
            log = log.display()
        );
        std::fs::write(&bin, script).expect("write logging pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod logging pi");
        Self { dir, bin, log }
    }

    /// The commands pi has read so far.
    fn commands(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect()
    }

    /// Block until pi has read `count` commands, then return them. Writes cross
    /// a pipe to a child process, so the first read cannot assume they landed.
    fn wait_for(&self, count: usize) -> Vec<Value> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let commands = self.commands();
            if commands.len() >= count {
                return commands;
            }
            assert!(
                Instant::now() < deadline,
                "pi never received {count} commands: {commands:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Drop everything recorded so far, so a test asserts on its own action.
    fn clear(&self) {
        let _ = std::fs::remove_file(&self.log);
    }
}

impl Drop for LoggingPi {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn app_with_logging_pi(cx: &mut gpui::TestAppContext, tag: &str) -> (Entity<OrbitApp>, LoggingPi) {
    let fake = LoggingPi::start(tag);
    let app = cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    });
    cx.update(|cx| {
        app.update(cx, |app, _| {
            let workspace = std::env::temp_dir();
            let client =
                PiClient::spawn_with_bin(fake.bin.to_str().expect("utf-8 path"), &workspace, None)
                    .expect("spawn logging pi");
            app.adopt_client(client);
        })
    });
    (app, fake)
}

fn queue(steering: &[&str], follow_up: &[&str]) -> PendingQueue {
    PendingQueue {
        steering: steering.iter().map(|s| (*s).to_owned()).collect(),
        follow_up: follow_up.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn steer(index: usize) -> QueuedMessage {
    QueuedMessage {
        lane: QueueLane::Steer,
        index,
    }
}

fn follow_up(index: usize) -> QueuedMessage {
    QueuedMessage {
        lane: QueueLane::FollowUp,
        index,
    }
}

fn wire_types(commands: &[Value]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|command| command["type"].as_str().map(str::to_owned))
        .collect()
}

fn wire_message(command: &Value) -> &str {
    command["message"].as_str().unwrap_or("")
}

/// Editing one queued message must leave the rest of the queue exactly as it
/// was: same lanes, same order, only the edited text changed.
#[gpui::test]
fn editing_a_queued_message_requeues_the_survivors_in_order(cx: &mut gpui::TestAppContext) {
    let (app, fake) = app_with_logging_pi(cx, "edit");
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&["tighten the hero"], &["then add tests"]);
            fake.clear();

            app.begin_queue_edit(steer(0), window, cx);
            app.queue_edit
                .as_ref()
                .expect("the field opened")
                .input
                .update(cx, |input, cx| input.set_text("tighten the hero copy", cx));
            app.commit_queue_edit(window, cx);

            assert_eq!(app.queue.steering, vec!["tighten the hero copy"]);
            assert_eq!(app.queue.follow_up, vec!["then add tests"]);
            assert!(app.queue_edit.is_none(), "the field closes on save");
        });
    });

    let commands = fake.wait_for(3);
    assert_eq!(
        wire_types(&commands),
        vec!["clear_queue", "steer", "follow_up"],
        "the rebuild clears once, then re-queues each lane"
    );
    assert_eq!(wire_message(&commands[1]), "tighten the hero copy");
    assert_eq!(wire_message(&commands[2]), "then add tests");
}

/// "Send now" on a follow-up moves it into the steering lane at the front:
/// steering is delivered before the next model call, which is the earliest
/// point short of aborting the run.
#[gpui::test]
fn send_now_promotes_a_follow_up_into_the_steer_lane(cx: &mut gpui::TestAppContext) {
    let (app, fake) = app_with_logging_pi(cx, "promote");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&["keep going"], &["first", "send me first"]);
            fake.clear();

            app.send_queued_now(follow_up(1), cx);

            assert_eq!(app.queue.steering, vec!["send me first", "keep going"]);
            assert_eq!(app.queue.follow_up, vec!["first"]);
        });
    });

    let commands = fake.wait_for(4);
    assert_eq!(
        wire_types(&commands),
        vec!["clear_queue", "steer", "steer", "follow_up"]
    );
    assert_eq!(wire_message(&commands[1]), "send me first");
    assert_eq!(wire_message(&commands[2]), "keep going");
    assert_eq!(wire_message(&commands[3]), "first");
}

/// The message pi delivers next has nowhere to move, so the action must not
/// rebuild the whole queue to say so.
#[gpui::test]
fn send_now_is_a_no_op_for_the_message_pi_delivers_next(cx: &mut gpui::TestAppContext) {
    let (app, fake) = app_with_logging_pi(cx, "noop");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&["already first"], &["later"]);
            fake.clear();

            app.send_queued_now(steer(0), cx);

            assert_eq!(app.queue, queue(&["already first"], &["later"]));
            assert!(!app.queue_replaced, "nothing was sent");
        });
    });

    // Give the writer thread a beat, then prove it stayed quiet.
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        fake.commands().is_empty(),
        "a promotion with nowhere to go must not touch pi's queue"
    );
}

/// A blank draft is not a message: the field stays open instead of dropping
/// the queued text behind the user's back.
#[gpui::test]
fn an_empty_draft_keeps_the_field_open(cx: &mut gpui::TestAppContext) {
    let (app, fake) = app_with_logging_pi(cx, "blank");
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&[], &["do not lose me"]);
            fake.clear();

            app.begin_queue_edit(follow_up(0), window, cx);
            app.queue_edit
                .as_ref()
                .expect("the field opened")
                .input
                .update(cx, |input, cx| input.set_text("   ", cx));
            app.commit_queue_edit(window, cx);

            assert!(app.queue_edit.is_some(), "the field stays open");
            assert_eq!(app.queue.follow_up, vec!["do not lose me"]);
        });
    });

    std::thread::sleep(Duration::from_millis(50));
    assert!(
        fake.commands().is_empty(),
        "an empty draft must not rebuild the queue"
    );
}

/// pi's `clear_queue` reply carries the list from *before* the rebuild. Applying
/// it would blink the old rows back in (and, worse, leave the mirror wrong
/// until the next `queue_update`).
#[gpui::test]
fn the_clear_queue_echo_does_not_resurrect_the_replaced_queue(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_logging_pi(cx, "echo");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&[], &["old text"]);
            app.send_queued_now(follow_up(0), cx);
            assert_eq!(app.queue.steering, vec!["old text"]);

            let echo = serde_json::json!({"steering": [], "followUp": ["old text"]});
            let mut refresh = false;
            app.on_response(
                "clear_queue",
                true,
                Some(&echo),
                None,
                "1",
                &mut refresh,
                cx,
            );

            assert_eq!(
                app.queue.steering,
                vec!["old text"],
                "the rebuilt queue stands until pi reports a new one"
            );
            assert!(app.queue.follow_up.is_empty());
            assert!(!app.queue_replaced, "the echo consumed the flag");
        });
    });
}

/// A queued message can be delivered while its editor is open. The field then
/// belongs to no queued message and must close — otherwise Save would rewrite
/// whatever now sits at that index.
#[gpui::test]
fn a_delivered_message_closes_its_editor(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_logging_pi(cx, "delivered");
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&[], &["shipped", "still queued"]);
            app.begin_queue_edit(follow_up(0), window, cx);

            // pi delivered the first follow-up: only the survivor remains.
            app.queue = queue(&[], &["still queued"]);
            app.reanchor_queue_edit(cx);

            assert!(
                app.queue_edit.is_none(),
                "the field closed with the message it was editing"
            );
        });
    });
}

/// The message keeps its text across a `queue_update`, so the editor follows it
/// to the new position instead of closing or saving onto its neighbour.
#[gpui::test]
fn a_shifted_message_reanchors_instead_of_closing(cx: &mut gpui::TestAppContext) {
    let (app, _fake) = app_with_logging_pi(cx, "shifted");
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.queue = queue(&[], &["still queued"]);
            app.begin_queue_edit(follow_up(0), window, cx);

            // A steer landed ahead of it; the text is now second in its lane.
            app.queue = queue(&[], &["ahead now", "still queued"]);
            app.reanchor_queue_edit(cx);

            let edit = app.queue_edit.as_ref().expect("the field is still open");
            assert_eq!(edit.target.lane, QueueLane::FollowUp);
            assert_eq!(edit.target.index, 1);
        });
    });
}
