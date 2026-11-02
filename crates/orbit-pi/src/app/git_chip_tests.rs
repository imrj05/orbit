//! Regression test: the top bar's dirty-tree `+N −N` chip opens the Git page's
//! Changes tab, so the counts reveal the files they name instead of opening
//! Review. The clean panel glyph remains the Review toggle.

use super::*;
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    })
}

#[gpui::test]
fn dirty_counts_open_the_git_changes_tab(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.added = 69;
            app.removed = 74;

            app.open_git_changes(cx);

            assert!(app.git_open, "the Git page must open");
            assert_eq!(
                app.git_panel.read(cx).active_tab_index(),
                0,
                "the counts open Changes, not another tab",
            );
            assert!(
                !app.sidepane.read(cx).is_open(),
                "Review must close when the Git page takes the window",
            );
        });
    });
}
