//! A composer edit must repaint the app only when the chrome that reads the
//! draft changed. Notifying on every keystroke rebuilt the whole window —
//! sidebar, header, git panel, every visible transcript row — to move the send
//! control between two states, which is what typing felt like (issue #119).
//!
//! These tests pin that boundary from both sides: the key ignores edits the
//! chrome cannot see, and the observer above it still repaints when they can.

use super::*;
use crate::theme::{Theme, ThemeId};

fn app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

/// Type `text` into the composer and report how many times the app was asked to
/// repaint. The count is read after the update flushes its effects, so an
/// observer on the app has already run.
fn type_and_count_repaints(
    cx: &mut gpui::TestAppContext,
    app: &Entity<OrbitApp>,
    text: &str,
) -> usize {
    let count = Rc::new(std::cell::Cell::new(0usize));
    let probe = count.clone();
    cx.update(|cx| {
        cx.observe(app, move |_, _| probe.set(probe.get() + 1))
            .detach();
        app.update(cx, |app, cx| {
            app.input.update(cx, |input, cx| input.set_text(text, cx));
        });
    });
    count.get()
}

/// The first character readies the send control, so the window repaints.
#[gpui::test]
fn the_first_character_of_a_message_repaints_the_app(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    assert_eq!(
        type_and_count_repaints(cx, &app, "fix the flaky test"),
        1,
        "an empty composer becoming sendable is a visible change"
    );
}

/// Typing on inside a message changes nothing the chrome reads: the send
/// control was already live and the text is drawn by the composer itself.
#[gpui::test]
fn typing_inside_a_message_does_not_repaint_the_app(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    assert_eq!(type_and_count_repaints(cx, &app, "fix"), 1);

    assert_eq!(
        type_and_count_repaints(cx, &app, "fix the flaky test"),
        0,
        "a longer message is the composer's own repaint, not the window's"
    );
}

/// Deleting back to nothing is visible twice over: the send control goes quiet
/// and the follow-up chips come back.
#[gpui::test]
fn clearing_the_composer_repaints_the_app(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    type_and_count_repaints(cx, &app, "fix");

    assert_eq!(type_and_count_repaints(cx, &app, ""), 1);
}

/// A collapsed paste counts as a payload even with an empty box, so the tray and
/// the send control both follow it. The key has to notice an attachment that
/// arrives without a keystroke, or a later edit would leave the tray behind.
#[gpui::test]
fn an_attachment_with_an_empty_box_moves_the_key(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            let empty = app.composer_chrome(cx);
            app.pasted_texts
                .push(PastedText::new("fn main() {}".into()));
            let pill = app.composer_chrome(cx);
            assert!(
                pill != empty,
                "the chip that makes the control live moved nothing"
            );

            // And a pill alone is not the same as a pill plus a draft: the tray
            // count and the follow-up chips are separate facts.
            app.input
                .update(cx, |input, cx| input.set_text("and then", cx));
            assert!(app.composer_chrome(cx) != pill);
        });
    });
}

/// The slash/`@` menu is app-level chrome ranked from the draft, so opening it
/// and re-ranking it for a new query must repaint — otherwise the popup would
/// show the previous query's entries.
#[gpui::test]
fn an_open_completion_menu_repaints_the_app(cx: &mut gpui::TestAppContext) {
    let app = app(cx);
    // The menu needs somewhere to look: without a workspace `@` returns nothing.
    let root = std::env::temp_dir().join("orbit-composer-chrome");
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.current_workspace = Some(root.clone());
            app.mention_index_workspace = Some(root);
            app.mention_index = Some(std::sync::Arc::new(
                crate::mentions::MentionIndex::from_paths(vec!["src/main.rs".to_string()]),
            ));
            app.mention_index_stale = false;
            app.mention_query_memo = None;
            app.composer_chrome = app.composer_chrome(cx);
        });
    });

    // The menu opens on `@` and closes on the space that ends the token, so both
    // edits move the key while the send control's state does not.
    assert_eq!(type_and_count_repaints(cx, &app, "look at @"), 1);
    assert_eq!(
        type_and_count_repaints(cx, &app, "look at @src/m"),
        1,
        "a new query is a new list of entries"
    );
    assert_eq!(
        type_and_count_repaints(cx, &app, "look at @src/main.rs now"),
        1,
        "closing the menu is a repaint too"
    );
}
