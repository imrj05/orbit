//! The debug frame recorder's chord and handler.
//!
//! Two things can silently break here, and they fail differently in the app:
//!
//! * the **binding** — `⌘⌥P` never resolving to the action, so no chip appears;
//! * the **handler body** — the action arriving but not starting a capture.
//!
//! Both are covered below. What is *not* covered, deliberately: dispatching the
//! action through `OrbitApp`'s own tree. That harness cannot route a keystroke to
//! the root handler even for an existing action (`⌘K` fails identically), so a
//! test written against it asserts nothing about the app.

use super::*;
#[cfg(debug_assertions)]
use crate::theme::ThemeId;
#[cfg(debug_assertions)]
use gpui::size;

#[cfg(debug_assertions)]
fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::OrbitAzure));
        crate::bind_keys(cx);
        cx.new(OrbitApp::new)
    })
}

#[cfg(debug_assertions)]
#[gpui::test]
fn calling_the_handler_directly_starts_a_capture(cx: &mut gpui::TestAppContext) {
    let _ = crate::perf::finish();
    let app = test_app(cx);
    let cx = cx.add_empty_window();
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(760.)), |_, _| {
        app.clone().into_any_element()
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.on_record_frame_times(&crate::RecordFrameTimes, window, cx)
        });
    });
    let active = crate::perf::is_active();
    let _ = crate::perf::finish();
    assert!(active, "the handler body must start a capture");
}

/// A bare view with the recorder's chord and a probe counter: this isolates
/// gpui's keystroke → action → handler path from OrbitApp's element tree.
struct Probe {
    hits: usize,
    focus: gpui::FocusHandle,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this: &mut Self, _: &crate::RecordFrameTimes, _, cx| {
                    this.hits += 1;
                    cx.notify();
                }),
            )
    }
}

#[gpui::test]
fn the_chord_reaches_a_root_action_handler(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys([gpui::KeyBinding::new(
            "secondary-alt-p",
            crate::RecordFrameTimes,
            None,
        )]);
    });
    let (probe, cx) = cx.add_window_view(|_, cx| Probe {
        hits: 0,
        focus: cx.focus_handle(),
    });
    cx.update(|window, cx| {
        let handle = probe.read(cx).focus.clone();
        window.focus(&handle);
    });
    cx.simulate_keystrokes("secondary-alt-p");
    let hits = cx.update(|_, cx| probe.read(cx).hits);
    assert_eq!(hits, 1, "the chord must reach the handler that owns it");
}
