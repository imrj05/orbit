//! The composer's send control is one state machine, not three buttons: what
//! it shows must match what a send will actually do. These tests pin the two
//! rules that are easy to break — the payload check (which decides whether the
//! control is live) and the running state (which decides queue vs steer).

use super::*;
use crate::composer_send::SendMode;
use crate::theme::{Theme, ThemeId};

fn app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

/// A collapsed paste is a sendable message on its own. The control used to
/// check only text and attachments, so a pill-only composer rendered as
/// disabled while Enter still sent it — the button lied about what it would do.
#[gpui::test]
fn a_pasted_pill_alone_makes_the_send_control_live(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            assert!(!app.send_has_payload(cx), "nothing typed yet");

            app.input.update(cx, |input, cx| input.set_text("", cx));
            app.pasted_texts
                .push(PastedText::new("fn main() {}".into()));

            assert!(
                app.send_has_payload(cx),
                "a pasted pill is a payload even with an empty box"
            );
        });
    });
}

/// An attachment alone is likewise sendable (the old rule had this one right).
#[gpui::test]
fn an_attachment_alone_makes_the_send_control_live(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            assert!(!app.send_has_payload(cx));
            app.attachments.push(Attachment {
                name: "shot.png".into(),
                mime: "image/png".into(),
                data: String::new(),
                preview: None,
                original_len: 0,
                stored_len: 0,
                compressed: false,
            });
            assert!(app.send_has_payload(cx));
        });
    });
}

/// Whitespace is not a message: the control stays disabled so a stray space
/// cannot arm a button that would no-op.
#[gpui::test]
fn whitespace_is_not_a_payload(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.input
                .update(cx, |input, cx| input.set_text("   \n\t ", cx));
            assert!(!app.send_has_payload(cx));
        });
    });
}

/// The control's "running" state must agree with the delivery policy: sending
/// mid-run queues or steers, and `is_running` is what decides. A control keyed
/// to `busy` alone would show Send while `submit_as` queued a follow-up.
#[gpui::test]
fn the_send_state_follows_the_delivery_policy(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _cx| {
            assert!(!app.is_running(), "idle: plain send");

            // A pending reply is a run in flight even before tokens arrive —
            // the same state `submit_as` reads to choose follow_up over prompt.
            app.transcript.expect_reply();
            assert!(app.is_running(), "pending reply counts as running");
            assert!(!app.busy, "…even though `busy` has not been set yet");

            // Idle sends always start a prompt, whatever the mode says.
            let (_, label) = SendMode::Steer.command(false, "hello".into(), None);
            assert_eq!(label, "prompt");
            // Mid-run the mode picks the command.
            let (_, label) = SendMode::FollowUp.command(true, "hello".into(), None);
            assert_eq!(label, "follow_up");
            let (_, label) = SendMode::Steer.command(true, "hello".into(), None);
            assert_eq!(label, "steer");
        });
    });
}
