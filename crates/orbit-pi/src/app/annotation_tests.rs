//! Quotes held in the composer (issue #187): they ride the draft like every
//! other composer item, refuse past the cap instead of dropping, come off by
//! mouse or keyboard, and lead the prompt pi receives in the documented order.

use super::*;
use crate::annotation::{normalize_quote, QuoteSource, MAX_ANNOTATIONS};
use crate::composer_send::SendMode;
use crate::theme::{Theme, ThemeId};

fn quote(text: &str) -> crate::annotation::Quote {
    normalize_quote(text).expect("a quote")
}

fn window_app(cx: &mut gpui::TestAppContext) -> (Entity<OrbitApp>, &mut gpui::VisualTestContext) {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        crate::bind_keys(cx);
    });
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    // `OrbitApp::new` starts the real pi; nothing here may ever reach it.
    cx.update(|_, cx| app.update(cx, |app, _| app.client = None));
    (app, cx)
}

/// A quote, and the comment typed beside it, belong to the session they were
/// written in: switching away parks them, coming back restores them.
#[gpui::test]
fn quotes_follow_their_session_draft(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    let first = PathBuf::from("/tmp/orbit-annotation-a.jsonl");
    let second = PathBuf::from("/tmp/orbit-annotation-b.jsonl");
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(first.clone());
            app.composer_is_new_task = false;
            assert!(app.add_annotation(quote("the passage"), QuoteSource::Assistant, None, cx));
            app.annotations[0]
                .comment
                .update(cx, |input, cx| input.set_text("why?", cx));
            app.save_composer_draft(cx);
            assert!(
                app.drafts.contains_key(&first),
                "a quote alone is a draft worth keeping"
            );

            app.current_session_path = Some(second.clone());
            app.restore_composer_draft(Some(&second), cx);
            assert!(app.annotations.is_empty(), "another session starts clean");
            app.save_composer_draft(cx);

            app.current_session_path = Some(first.clone());
            app.restore_composer_draft(Some(&first), cx);
            assert_eq!(app.annotations.len(), 1);
            assert_eq!(app.annotations[0].quote.text, "the passage");
            assert_eq!(app.annotations[0].comment.read(cx).text(), "why?");
        });
    });
}

/// Past the cap a quote is refused, and the person is told; quotes already
/// held stay put.
#[gpui::test]
fn a_quote_past_the_cap_is_refused_with_a_toast(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            for ix in 0..MAX_ANNOTATIONS {
                assert!(app.add_annotation(quote(&format!("q{ix}")), QuoteSource::User, None, cx));
            }
            let toasts = app.toasts.len();
            assert!(!app.add_annotation(quote("one more"), QuoteSource::User, None, cx));
            assert_eq!(app.annotations.len(), MAX_ANNOTATIONS);
            assert_eq!(app.toasts.len(), toasts + 1, "the refusal is announced");
        });
    });
}

/// The same passage quoted twice is two quotes: nothing is deduplicated, and
/// removing one leaves the other in order.
#[gpui::test]
fn removing_a_quote_keeps_the_rest_in_order(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.add_annotation(quote("same"), QuoteSource::User, None, cx);
            app.add_annotation(quote("same"), QuoteSource::User, None, cx);
            app.add_annotation(quote("last"), QuoteSource::Assistant, None, cx);
            assert_eq!(app.annotations.len(), 3);
            app.remove_annotation(0, None, cx);
            let texts = app
                .annotations
                .iter()
                .map(|annotation| annotation.quote.text.as_str())
                .collect::<Vec<_>>();
            assert_eq!(texts, ["same", "last"]);
            // An index past the end is a no-op, not a panic.
            app.remove_annotation(9, None, cx);
            assert_eq!(app.annotations.len(), 2);
        });
    });
}

/// A quote's comment field has the keyboard as soon as it is added. Enter
/// keeps the comment, Escape undoes an edit (never the quote), and
/// Backspace in an empty field removes the quote — and the composer gets the
/// keyboard back each time.
#[gpui::test]
fn the_comment_field_answers_enter_escape_and_backspace(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.add_annotation(quote("passage"), QuoteSource::Assistant, Some(window), cx);
        });
    });
    // The window's root is empty, so every refresh drops the app from the
    // dispatch tree: draw it before each key, as a real frame would.
    let draw = |app: &Entity<OrbitApp>, cx: &mut gpui::VisualTestContext| {
        let app = app.clone();
        cx.draw(
            gpui::point(px(0.), px(0.)),
            gpui::size(px(1000.), px(800.)),
            move |_, _| app,
        );
    };
    draw(&app, cx);
    let focused = |app: &Entity<OrbitApp>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let app = app.read(cx);
            (
                app.annotations
                    .first()
                    .is_some_and(|a| a.comment.read(cx).focus_handle(cx).is_focused(window)),
                app.input.read(cx).focus_handle(cx).is_focused(window),
            )
        })
    };
    assert_eq!(focused(&app, cx), (true, false), "the new quote's comment has focus");

    cx.simulate_input("too vague");
    draw(&app, cx);
    cx.simulate_keystrokes("enter");
    assert_eq!(focused(&app, cx), (false, true), "Enter returns to the composer");
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.annotations[0].saved_comment, "too vague");
        assert!(app.input.read(cx).text().is_empty(), "Enter did not submit");
    });

    // An edit Escape undoes; the quote stays.
    cx.update(|window, cx| app.read(cx).annotations[0].comment.read(cx).focus(window));
    draw(&app, cx);
    cx.simulate_input("!!");
    draw(&app, cx);
    cx.simulate_keystrokes("escape");
    assert_eq!(focused(&app, cx), (false, true));
    cx.update(|_, cx| {
        let app = app.read(cx);
        assert_eq!(app.annotations.len(), 1);
        assert_eq!(app.annotations[0].comment.read(cx).text(), "too vague");
    });

    // Backspace edits a comment that has text, and removes the quote once
    // the field is empty.
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.annotations[0]
                .comment
                .update(cx, |input, cx| input.set_text("x", cx));
            app.annotations[0].comment.read(cx).focus(window);
        });
    });
    draw(&app, cx);
    cx.simulate_keystrokes("backspace");
    cx.update(|window, cx| {
        let app = app.read(cx);
        assert_eq!(app.annotations.len(), 1, "it deleted the x");
        assert_eq!(app.annotations[0].comment.read(cx).text(), "");
        assert!(app.annotations[0].comment.read(cx).focus_handle(cx).is_focused(window));
    });
    draw(&app, cx);
    cx.simulate_keystrokes("backspace");
    cx.update(|_, cx| assert!(app.read(cx).annotations.is_empty()));
    assert_eq!(focused(&app, cx), (false, true));
}

/// A quote alone is something to send, so the send control is live.
#[gpui::test]
fn a_quote_alone_makes_the_send_control_live(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert!(!app.send_has_payload(cx));
            app.add_annotation(quote("passage"), QuoteSource::User, None, cx);
            assert!(app.send_has_payload(cx));
        });
    });
}

/// A send that fails (no pi to write to) keeps the quotes and comments: user
/// input is never dropped.
#[gpui::test]
fn a_failed_send_keeps_the_quotes(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.add_annotation(quote("passage"), QuoteSource::User, None, cx);
            app.annotations[0]
                .comment
                .update(cx, |input, cx| input.set_text("note", cx));
            app.submit_as("typed".into(), SendMode::FollowUp, cx);
            assert_eq!(app.annotations.len(), 1);
            assert_eq!(app.annotations[0].comment.read(cx).text(), "note");
        });
    });
}

/// A pi stand-in that records what Orbit writes to it.
#[cfg(unix)]
struct RecordingPi {
    dir: tempfile::TempDir,
    bin: PathBuf,
}

#[cfg(unix)]
impl RecordingPi {
    fn start() -> Self {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("create fake-pi dir");
        let bin = dir.path().join("fake-pi");
        let log = dir.path().join("stdin.jsonl");
        std::fs::write(
            &bin,
            format!("#!/bin/sh\nexec cat > '{}'\n", log.display()),
        )
        .expect("write fake pi");
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake pi");
        Self { dir, bin }
    }

    /// The `message` of the first `prompt` command written, once the writer
    /// thread has flushed it.
    fn prompt_message(&self) -> String {
        let log = self.dir.path().join("stdin.jsonl");
        for _ in 0..200 {
            let lines = std::fs::read_to_string(&log).unwrap_or_default();
            for line in lines.lines() {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                    continue;
                };
                if value.get("type").and_then(|t| t.as_str()) == Some("prompt") {
                    return value["message"].as_str().unwrap_or_default().to_string();
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no prompt reached pi");
    }
}

/// What pi receives: each quote (oldest first) with its comment, then the
/// typed message. A comment typed but never confirmed with Enter still goes —
/// what is on screen is what is sent. The composer is empty afterwards.
#[cfg(unix)]
#[gpui::test]
fn sending_leads_the_prompt_with_quotes_in_order(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    let fake = RecordingPi::start();
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let client = PiClient::spawn_with_bin(
                fake.bin.to_str().expect("utf-8 path"),
                fake.dir.path(),
                None,
            )
            .expect("spawn fake pi");
            app.adopt_client(client);
            app.current_workspace = Some(fake.dir.path().to_path_buf());
            app.add_annotation(quote("first **passage**"), QuoteSource::Assistant, None, cx);
            app.add_annotation(quote("second\npassage"), QuoteSource::User, None, cx);
            app.annotations[0]
                .comment
                .update(cx, |input, cx| input.set_text("too vague", cx));
            app.input
                .update(cx, |input, cx| input.set_text("what did you mean?", cx));
            app.submit_as("what did you mean?".into(), SendMode::FollowUp, cx);
            assert!(app.annotations.is_empty(), "sending clears the quotes");
        });
    });
    assert_eq!(
        fake.prompt_message(),
        "> first **passage**\nComment: too vague\n\n> second\n> passage\n\nwhat did you mean?"
    );
    // And the sent message reads back as two quotes and the typed text.
    let message = fake.prompt_message();
    let (quotes, body) = crate::annotation::split(&message);
    assert_eq!(quotes.len(), 2);
    assert_eq!(body, "what did you mean?");
}

/// Shift+F10 is the selection menu wherever the keyboard is (the composer,
/// say), but the sidebar's own row menu keeps it inside the sidebar.
#[gpui::test]
fn shift_f10_opens_the_selection_menu_except_in_the_sidebar(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        crate::bind_keys(cx);
        let action = |context: &str| {
            let (bindings, _) = cx.key_bindings().borrow().bindings_for_input(
                &[gpui::Keystroke::parse("shift-f10").unwrap()],
                &[gpui::KeyContext::parse(context).unwrap()],
            );
            bindings
                .first()
                .map(|binding| binding.action().boxed_clone())
                .unwrap()
        };
        assert!(action("Composer ChatComposer")
            .as_any()
            .is::<crate::OpenSelectionMenu>());
        assert!(action("Sidebar")
            .as_any()
            .is::<crate::SidebarOpenRowMenu>());
    });
}

/// The toolbar never takes the keyboard, so Escape reaches the app's abort
/// chain: there it closes the toolbar, keeps the selection, and stops — it
/// must not go on to abort a run.
#[gpui::test]
fn escape_closes_the_selection_toolbar_before_anything_else(cx: &mut gpui::TestAppContext) {
    let (app, cx) = window_app(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.transcript.test_open_selection_toolbar("a passage", cx);
            app.busy = true;
            app.on_abort(&crate::AbortRun, window, cx);
            assert!(!app.transcript.test_selection_toolbar_open());
            assert_eq!(app.transcript.selected_text().as_deref(), Some("a passage"));
            assert!(app.busy, "the first Escape did not abort the run");
        });
    });
}
