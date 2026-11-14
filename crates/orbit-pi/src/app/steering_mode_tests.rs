//! Settings → Agent's steering delivery mode (#131): the lane that jumps the
//! queue gets the same `One at a time` / `All` choice as follow-ups, read
//! back from pi's `get_state`.

use super::*;
use crate::theme::{Theme, ThemeId};
use serde_json::json;

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

#[gpui::test]
fn get_state_reports_the_steering_mode(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            assert_eq!(app.steering_mode, "one-at-a-time", "pi's default");
            let mut refresh = false;
            let state = json!({ "steeringMode": "all", "followUpMode": "one-at-a-time" });
            app.on_response(
                "get_state",
                true,
                Some(&state),
                None,
                "s1",
                &mut refresh,
                cx,
            );
            assert_eq!(app.steering_mode, "all");
            assert_eq!(
                app.follow_up_mode, "one-at-a-time",
                "the lanes are independent"
            );
        });
    });
}

#[gpui::test]
fn choosing_a_steering_mode_updates_it_locally(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.set_steering_mode(true, cx);
            assert_eq!(app.steering_mode, "all");
            app.set_steering_mode(false, cx);
            assert_eq!(app.steering_mode, "one-at-a-time");
            assert_eq!(app.follow_up_mode, "one-at-a-time", "follow-ups untouched");
        });
    });
}
