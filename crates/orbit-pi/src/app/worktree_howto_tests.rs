//! The Worktrees page's "How worktrees work" card shows for newcomers and
//! stays hidden once dismissed.

use super::*;
use crate::theme::{Theme, ThemeId};

#[gpui::test]
fn howto_card_shows_until_dismissed(cx: &mut gpui::TestAppContext) {
    let app = cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    });
    let cx = cx.add_empty_window();
    cx.update(|_window, cx| {
        app.update(cx, |app, cx| {
            app.worktree_howto_dismissed = false;
            assert!(
                app.worktree_howto_card(*theme::get(cx), cx.entity())
                    .is_some(),
                "the card is present until dismissed"
            );
            app.worktree_howto_dismissed = true;
            assert!(
                app.worktree_howto_card(*theme::get(cx), cx.entity())
                    .is_none(),
                "dismissing hides the card"
            );
        });
    });
}
