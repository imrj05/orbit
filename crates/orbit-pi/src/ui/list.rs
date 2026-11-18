//! The virtualized list: GPUI's `list()` with its one load-bearing layout
//! rule pinned.
//!
//! `list()`'s default sizing is `Auto`, which has no intrinsic height. Laid out
//! as a plain block child it gets height 0, `prepaint_items` clears its items,
//! and **nothing paints** — while the overdraw probe still renders a few rows,
//! which makes the bug look intermittent. [`virtual_list`] always fills its
//! parent's height; the parent must be a `flex_1` + `min_h_0` column (or have
//! a definite height), and the list must never be wrapped in an outer
//! `overflow_y_scroll()` — it scrolls itself.
//!
//! The state (`ListState`: count, alignment, overdraw, scroll handler) and the
//! row renderer stay with the surface; this owns only the element's layout.

use gpui::{list, AnyElement, App, List, ListState, Styled, Window};

/// A virtualized list over `state` that fills its parent's height. Chain
/// `.w_full()` (or `.size_full()`) where the parent does not stretch it.
pub(crate) fn virtual_list(
    state: ListState,
    render_item: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
) -> List {
    list(state, render_item).h_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        div, point, px, size, AppContext, Context, InteractiveElement, IntoElement, ListAlignment,
        ParentElement, Render,
    };

    struct Host(ListState);

    impl Render for Host {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().flex().flex_col().child(
                div()
                    .id("list-parent")
                    .flex_1()
                    .min_h_0()
                    .child(virtual_list(self.0.clone(), |ix, _, _| {
                        div()
                            .id(("row", ix))
                            .debug_selector(move || format!("row-{ix}"))
                            .h(px(20.))
                            .into_any_element()
                    })),
            )
        }
    }

    /// The contract the component exists for: inside a `flex_1 min_h_0`
    /// parent the list gets a real height and paints rows past the overdraw
    /// probe, not an empty 0px box.
    #[gpui::test]
    fn the_list_fills_its_flex_parent_and_paints_its_rows(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let state = ListState::new(100, ListAlignment::Top, px(0.));
        let view = cx.update(|_, cx| cx.new(|_| Host(state.clone())));
        let _ = cx.draw(point(px(0.), px(0.)), size(px(300.), px(400.)), |_, _| {
            view.clone()
        });
        assert_eq!(state.viewport_bounds().size.height, px(400.));
        // 400px of 20px rows: the last visible row painted, the next one not.
        assert!(cx.debug_bounds("row-19").is_some(), "visible rows paint");
        assert!(cx.debug_bounds("row-30").is_none(), "rows stay virtualized");
    }
}
