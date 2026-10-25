//! Composer state that belongs to a *target*, not to the app: the unsent
//! draft a session carries while the user is elsewhere, and the long-paste
//! pills that hold text compactly until it is sent.

use super::*;

/// Long pastes collapse into a pill; anything a person would type inline
/// stays inline. The gate is the whole feature — a false positive swallows
/// text the user was still arranging, a false negative wrecks the box.
#[gpui::test]
fn long_pastes_pill_and_short_ones_stay_inline(_cx: &mut gpui::TestAppContext) {
    // Short, ordinary pastes ride the caret.
    assert!(!crate::composer::should_pill_paste("hello"));
    assert!(!crate::composer::should_pill_paste("a line\nand another"));
    assert!(!crate::composer::should_pill_paste(
        &"x".repeat(crate::composer::PASTE_PILL_MIN_CHARS - 1)
    ));

    // Many lines, or one very long line, becomes a pill.
    let many = (0..crate::composer::PASTE_PILL_MIN_LINES)
        .map(|ix| format!("line {ix}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(crate::composer::should_pill_paste(&many));
    assert!(crate::composer::should_pill_paste(
        &"x".repeat(crate::composer::PASTE_PILL_MIN_CHARS)
    ));
}

/// A draft is keyed to its session file, so switching away and back restores
/// exactly what was being written — and switching to a *different* session
/// never shows the first one's text.
#[gpui::test]
fn drafts_follow_their_session(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));

    let first = PathBuf::from("/tmp/orbit-draft-a.jsonl");
    let second = PathBuf::from("/tmp/orbit-draft-b.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            // Writing in the first session.
            app.current_session_path = Some(first.clone());
            app.input
                .update(cx, |input, cx| input.set_text("half a thought", cx));
            app.pasted_texts.push(PastedText::new("pasted".into()));
            app.save_composer_draft(cx);

            // Leaving parks it and leaves nothing behind.
            app.restore_composer_draft(Some(&second), cx);
            assert_eq!(app.input.read(cx).text(), "");
            assert!(app.pasted_texts.is_empty());

            // Coming back brings it, attachments included.
            app.restore_composer_draft(Some(&first), cx);
            assert_eq!(app.input.read(cx).text(), "half a thought");
            assert_eq!(app.pasted_texts.len(), 1);
            assert_eq!(app.pasted_texts[0].content, "pasted");
        });
    });
}

/// An empty composer parks no draft, so an unedited session does not leave a
/// blank entry behind (and a stale draft can never resurrect old text).
#[gpui::test]
fn empty_composers_park_no_draft(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let path = PathBuf::from("/tmp/orbit-draft-empty.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = Some(path.clone());
            app.input
                .update(cx, |input, cx| input.set_text("   \n  ", cx));
            app.save_composer_draft(cx);
            assert!(app.drafts.is_empty(), "whitespace is not a draft");
        });
    });
}

/// The New Task page has no session file yet, so its draft lives in its own
/// slot — typing there then opening a session must not leak it.
#[gpui::test]
fn the_new_task_draft_has_its_own_slot(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    let path = PathBuf::from("/tmp/orbit-draft-task.jsonl");

    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.current_session_path = None;
            app.input
                .update(cx, |input, cx| input.set_text("brand new task", cx));
            app.save_composer_draft(cx);
            assert!(app.new_task_draft.is_some());

            app.restore_composer_draft(Some(&path), cx);
            assert_eq!(app.input.read(cx).text(), "");

            app.restore_composer_draft(None, cx);
            assert_eq!(app.input.read(cx).text(), "brand new task");
        });
    });
}

/// Pills are only a compact way to *hold* text: on submit the paste rejoins
/// the prompt as its own block, so the agent never reads it as a continuation
/// of the sentence above.
#[gpui::test]
fn pills_fold_back_into_the_prompt(_cx: &mut gpui::TestAppContext) {
    let paste = PastedText::new("fn main() {\n    todo!()\n}".into());
    assert_eq!(
        OrbitApp::fold_pasted_texts("explain this", std::slice::from_ref(&paste)),
        "explain this\n\nfn main() {\n    todo!()\n}"
    );
    // A pill on its own is the whole prompt — no leading blank block.
    assert_eq!(
        OrbitApp::fold_pasted_texts("", std::slice::from_ref(&paste)),
        "fn main() {\n    todo!()\n}"
    );
    assert_eq!(OrbitApp::fold_pasted_texts("just text", &[]), "just text");
}
