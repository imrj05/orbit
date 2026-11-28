//! The sidebar Git button's activity: the dot appears only with a reason and
//! the tooltip names it, and opening a GitHub tab marks that list seen.

use super::*;
use crate::git_activity::{Activity, Kind};
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> (Entity<OrbitApp>, &mut gpui::VisualTestContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut app = OrbitApp::new(cx);
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            app
        })
    });
    (app, cx)
}

#[gpui::test]
fn the_git_button_marks_activity_and_says_why(cx: &mut gpui::TestAppContext) {
    let (app, cx) = test_app(cx);
    let viewport = cx.update(|window, _| window.viewport_size());
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
    };

    cx.update(|_, cx| app.update(cx, |app, _| app.git_activity = Activity::default()));
    draw(cx);
    assert!(
        cx.debug_bounds("sidebar-git-dot").is_none(),
        "a clean tree with nothing new shows no dot"
    );
    let quiet = cx.update(|_, cx| app.read(cx).git_nav_tooltip());
    assert!(!quiet.contains(" · "), "no reasons to list: {quiet}");

    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.git_activity = Activity {
                changes: 3,
                new_issues: 0,
                new_pulls: 1,
                review_requests: 0,
            }
        })
    });
    draw(cx);
    let dot = cx.debug_bounds("sidebar-git-dot").expect("changes mark the button");
    let button = cx.debug_bounds("sidebar-git").expect("the Git button");
    assert!(button.contains(&dot.origin), "the dot sits on the button: {dot:?} in {button:?}");
    let tip = cx.update(|_, cx| app.read(cx).git_nav_tooltip());
    assert_eq!(tip.matches(" · ").count(), 2, "changes and the pull request: {tip}");
}

#[gpui::test]
fn opening_a_github_tab_marks_that_list_seen(cx: &mut gpui::TestAppContext) {
    let (app, cx) = test_app(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let workspace = app.current_workspace.clone().expect("a workspace");
            app.github_repo = Some((workspace.clone(), "owner/repo".into()));
            app.github_checked_for = Some(workspace);
            app.github_checked_at = Some(Instant::now());
            app.git_activity.new_issues = 2;
            app.git_activity.new_pulls = 1;

            app.open_git(cx);
            app.git_panel.update(cx, |panel, cx| panel.set_tab(4, cx));
            app.tick_git_activity(cx);

            assert_eq!(app.git_activity.new_issues, 0, "the issues were looked at");
            assert_eq!(app.git_activity.new_pulls, 1, "the pull requests were not");
            assert!(app
                .git_activity_store
                .seen_since("owner/repo", Kind::Issues)
                .is_some());

            // Staying on the tab does not re-mark it every tick.
            let seen = app
                .git_activity_store
                .seen_since("owner/repo", Kind::Issues)
                .map(str::to_owned);
            app.git_activity.new_issues = 5;
            app.tick_git_activity(cx);
            assert_eq!(app.git_activity.new_issues, 5, "marked once per arrival");
            assert_eq!(
                app.git_activity_store
                    .seen_since("owner/repo", Kind::Issues)
                    .map(str::to_owned),
                seen
            );
        });
    });
}
