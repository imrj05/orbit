//! The shared switch is a keyboard control: Tab reaches it and Space or Enter
//! flips it through the same callback a click runs.

use std::cell::Cell;
use std::rc::Rc;

use super::helpers::switch;
use super::*;

/// A window holding nothing but one switch, counting its flips. Its root holds
/// focus the way the app's composer does at launch, so Tab has somewhere to
/// travel from, and answers Tab with `focus_next` as the app's root does.
struct SwitchProbe {
    root: FocusHandle,
    flips: Rc<Cell<usize>>,
}

impl Render for SwitchProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let flips = self.flips.clone();
        div()
            .size_full()
            .track_focus(&self.root)
            .on_action(|_: &crate::FocusNext, window, _| window.focus_next())
            .child(switch(
                "probe-switch",
                flips.get() % 2 == 1,
                theme::get(cx),
                move |_, _| flips.set(flips.get() + 1),
            ))
    }
}

#[gpui::test]
fn tab_reaches_the_switch_and_space_or_enter_flips_it(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        crate::bind_keys(cx);
    });
    let flips = Rc::new(Cell::new(0));
    let (probe, cx) = cx.add_window_view({
        let flips = flips.clone();
        move |window, cx| {
            let root = cx.focus_handle();
            window.focus(&root);
            SwitchProbe { root, flips }
        }
    });
    cx.run_until_parked();

    cx.simulate_keystrokes("space");
    assert_eq!(flips.get(), 0, "a switch without focus ignores the key");

    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| {
            let focused = window.focused(cx);
            focused.is_some() && focused != Some(probe.read(cx).root.clone())
        }),
        "Tab moves focus onto the switch"
    );
    cx.simulate_keystrokes("space");
    assert_eq!(flips.get(), 1, "Space flips it");
    cx.simulate_keystrokes("enter");
    assert_eq!(flips.get(), 2, "Enter flips it too");
}
