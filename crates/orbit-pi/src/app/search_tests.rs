//! Issue #100 for the find bar: Enter is the key that confirms an IME
//! candidate, so it must not also step the search to the next hit.

use super::search::TranscriptSearch;
use super::*;
use crate::composer::ComposerInput;
use crate::theme::{Theme, ThemeId};

fn app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

/// A find bar with three hits and the first one selected. The transcript is
/// empty on purpose: `step_search` only cycles `index` (and scrolls), so the
/// index alone says whether the keystroke was acted on.
fn open_search(app: &mut OrbitApp, cx: &mut Context<OrbitApp>) -> Entity<ComposerInput> {
    let input = cx.new(ComposerInput::new);
    app.transcript_search = Some(TranscriptSearch {
        input: input.clone(),
        query: "hit".into(),
        matches: vec![0, 1, 2],
        index: 0,
        matched_count: 3,
    });
    input
}

#[gpui::test]
fn enter_during_an_ime_composition_does_not_step_the_search(cx: &mut gpui::TestAppContext) {
    use gpui::EntityInputHandler as _;

    let app = app(cx);
    let cx = cx.add_empty_window();
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let input = open_search(app, cx);

            // Sogou with the pinyin buffer emptied: session open, preedit empty.
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "nihao", None, window, cx);
                input.replace_and_mark_text_in_range(None, "", None, window, cx);
            });
            assert!(input.read(cx).is_composing());

            app.on_search_next(&crate::SearchNext, window, cx);
            app.on_search_prev(&crate::SearchPrev, window, cx);
            assert_eq!(
                app.transcript_search.as_ref().expect("open").index,
                0,
                "the IME owns those keystrokes"
            );

            // Committed: Enter steps again, in both directions.
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "hit", window, cx)
            });
            app.on_search_next(&crate::SearchNext, window, cx);
            assert_eq!(app.transcript_search.as_ref().expect("open").index, 1);
            app.on_search_prev(&crate::SearchPrev, window, cx);
            assert_eq!(app.transcript_search.as_ref().expect("open").index, 0);
        });
    });
}
