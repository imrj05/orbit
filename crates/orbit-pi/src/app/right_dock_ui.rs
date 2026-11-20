//! The right pane's tabs: Review plus the delegated subagents the reader
//! opened from a delegation card (see [`crate::right_dock`]).
//!
//! The tabs live on the session's transcript, so they park and return with
//! it. Showing the pane is the app-wide pane flag: only "Open in side pane"
//! sets it, so coming back to a session never pops a pane the reader hid.

use super::*;
use crate::right_dock::{DockTab, SubagentTab};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, TextSize};
use crate::ui::tab::{tab_strip, Tab};

impl OrbitApp {
    /// Handed to the transcript: a delegation card's "Open in side pane".
    pub(super) fn dock_opener(&self, cx: &Context<Self>) -> crate::transcript_view::DockOpener {
        let this = cx.weak_entity();
        Rc::new(move |tab, _window, cx| {
            this.update(cx, |app, cx| app.open_dock_subagent(tab, cx))
                .ok();
        })
    }

    /// Open `tab` in the right pane and show the pane: this is the reader's
    /// own click, so it may bring a hidden pane forward.
    pub(super) fn open_dock_subagent(&mut self, tab: SubagentTab, cx: &mut Context<Self>) {
        self.transcript.dock_tabs_mut().open(tab);
        if self.sidepane.read(cx).is_open() {
            self.sidepane.update(cx, |_, cx| cx.notify());
        } else {
            self.sidepane.update(cx, |pane, cx| pane.show_review(cx));
        }
        cx.notify();
    }

    pub(super) fn activate_dock_tab(&mut self, tab: DockTab, cx: &mut Context<Self>) {
        self.transcript.dock_tabs_mut().activate(tab);
        self.sidepane.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    /// Close a subagent tab. The pane stays as it is: closing the last tab
    /// leaves Review, it does not hide the pane.
    pub(super) fn close_dock_subagent(&mut self, tab: SubagentTab, cx: &mut Context<Self>) {
        if self.transcript.dock_tabs_mut().close(tab) {
            self.sidepane.update(cx, |_, cx| cx.notify());
            cx.notify();
        }
    }

    /// A tab's label: the child's agent name, from the live row.
    fn dock_tab_label(&self, tab: SubagentTab) -> SharedString {
        self.transcript
            .subagent_run_at(tab.message_ix, tab.tool_ix)
            .and_then(|run| run.children.get(tab.child_ix).map(|child| child.agent.clone()))
            .filter(|agent| !agent.trim().is_empty())
            .map(SharedString::from)
            .unwrap_or_else(|| tr!("right_dock.subagent").into())
    }

    /// The tab strip, or `None` while the session has no subagent tabs: the
    /// pane is then plain Review with no strip at all.
    pub(super) fn dock_tab_strip(
        &self,
        app: &Entity<OrbitApp>,
        theme: Theme,
        cx: &App,
    ) -> Option<AnyElement> {
        let _ = cx;
        let tabs = self.transcript.dock_tabs();
        if tabs.is_empty() {
            return None;
        }
        let active = tabs.active();
        let mut strip = tab_strip(
            "right-dock-tabs",
            &self.right_dock_scroll,
            ButtonSize::Default,
            &theme,
        )
        .min_w_0()
        .flex_1();
        let review = app.clone();
        strip = strip.child(
            Tab::new("right-dock-tab-review", tr!("right_dock.review"))
                .active(active == DockTab::Review)
                .render(&theme)
                .debug_selector(|| "right-dock-tab-review".to_string())
                .on_click(move |_, _, cx| {
                    review.update(cx, |app, cx| app.activate_dock_tab(DockTab::Review, cx));
                }),
        );
        for (ix, tab) in tabs.subagents().iter().copied().enumerate() {
            let select = app.clone();
            let close = app.clone();
            strip = strip.child(
                Tab::new(
                    ElementId::NamedInteger("right-dock-tab".into(), ix as u64),
                    self.dock_tab_label(tab),
                )
                .active(active == DockTab::Subagent(tab))
                .close(
                    ElementId::NamedInteger("right-dock-tab-close".into(), ix as u64),
                    tr!("right_dock.close_tab"),
                    move |_, _, cx| {
                        close.update(cx, |app, cx| app.close_dock_subagent(tab, cx));
                    },
                )
                .render(&theme)
                .debug_selector(move || format!("right-dock-tab-{ix}"))
                .on_click(move |_, _, cx| {
                    select.update(cx, |app, cx| {
                        app.activate_dock_tab(DockTab::Subagent(tab), cx)
                    });
                }),
            );
        }
        Some(
            div()
                .flex_none()
                .w_full()
                .h(px(36.))
                .px(DynamicSpacing::Base06.px(&theme))
                .flex()
                .items_center()
                .border_b_1()
                .border_color(theme.hairline())
                .child(strip)
                .into_any_element(),
        )
    }

    /// The right pane while a subagent tab is active: the shared pane frame,
    /// the tab strip, and that child's live log.
    pub(super) fn render_subagent_dock(&self, theme: Theme, cx: &mut Context<Self>) -> AnyElement {
        let DockTab::Subagent(tab) = self.transcript.dock_tabs().active() else {
            return div().into_any_element();
        };
        let app = cx.entity();
        let header = self.dock_tab_strip(&app, theme, cx);
        let run = self.transcript.subagent_run_at(tab.message_ix, tab.tool_ix);
        let child = run.as_ref().and_then(|run| run.children.get(tab.child_ix));
        let content = match (run.as_ref(), child) {
            (Some(run), Some(_)) => crate::transcript_view::render_subagent_panel(
                run,
                tab.child_ix,
                (tab.message_ix, tab.tool_ix),
                self.transcript.copied_sections(),
                theme,
            ),
            // The row changed shape under the tab: say so instead of a blank.
            _ => div()
                .debug_selector(|| "right-dock-subagent-missing".to_string())
                .p(DynamicSpacing::Base16.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("right_dock.subagent_missing"))
                .into_any_element(),
        };
        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_col()
            .children(header)
            .child(
                div()
                    .id("right-dock-subagent-scroll")
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .child(content),
            )
            .into_any_element();
        let width = self.sidepane.read(cx).width();
        crate::sidepane::dock_shell("right-dock-subagent", width, theme, body).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, ThemeId};
    use serde_json::json;

    /// One delegation with one running child, the shape the extension sends.
    fn delegation() -> serde_json::Value {
        json!({"messages": [
            {"role":"user", "content":"look into auth"},
            {"role":"assistant", "content":[{
                "type":"toolCall", "id":"t1", "name":"subagent",
                "arguments":{"agent":"reviewer", "task":"review the auth flow"}
            }]},
            {"role":"toolResult", "toolCallId":"t1", "toolName":"subagent",
             "content":[{"type":"text", "text":"working"}],
             "details": {
                "orbit":"subagent/1", "mode":"single",
                "children":[{"agent":"reviewer", "status":"running",
                             "task":"review the auth flow", "phase":"thinking"}]
             }}
        ]})
    }

    fn tab() -> SubagentTab {
        SubagentTab {
            message_ix: 1,
            tool_ix: 0,
            child_ix: 0,
        }
    }

    fn app_with_delegation(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let app = cx.update(|cx| cx.new(OrbitApp::new));
        cx.update(|cx| {
            app.update(cx, |app, _| app.transcript.load_from(&delegation()));
        });
        app
    }

    #[gpui::test]
    fn the_fixture_is_a_delegation_the_dock_can_resolve(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            let app = app.read(cx);
            let run = app.transcript.subagent_run_at(1, 0).expect("a delegation");
            assert_eq!(run.children[0].agent, "reviewer");
        });
    }

    #[gpui::test]
    fn opening_a_subagent_shows_the_pane_on_that_tab(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                assert!(!app.sidepane.read(cx).is_open());
                app.open_dock_subagent(tab(), cx);
                assert!(app.sidepane.read(cx).is_open(), "the click shows the pane");
                assert_eq!(app.transcript.dock_tabs().active(), DockTab::Subagent(tab()));
            });
        });
    }

    /// Upstream aafd644f's rule, structural here: the tabs ride on the
    /// session, the pane flag is app-wide, and a restore never touches it.
    #[gpui::test]
    fn a_hidden_pane_stays_hidden_when_the_session_returns(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.open_dock_subagent(tab(), cx);
                app.sidepane.update(cx, |pane, cx| pane.toggle(cx));
                assert!(!app.sidepane.read(cx).is_open());
                // Switch away (another session's transcript) and back.
                let parked = std::mem::replace(&mut app.transcript, Transcript::new());
                assert!(app.transcript.dock_tabs().is_empty());
                app.transcript = parked;
                assert_eq!(
                    app.transcript.dock_tabs().active(),
                    DockTab::Subagent(tab()),
                    "the session keeps its tab"
                );
                assert!(
                    !app.sidepane.read(cx).is_open(),
                    "coming back must not pop the hidden pane open"
                );
            });
        });
    }

    #[gpui::test]
    fn a_reload_of_the_same_rows_keeps_the_tab(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.open_dock_subagent(tab(), cx);
                app.transcript.load_from(&delegation());
                assert_eq!(app.transcript.dock_tabs().subagents(), &[tab()]);
                app.transcript.load_from(&json!({"messages": []}));
                assert!(app.transcript.dock_tabs().is_empty(), "the delegation is gone");
            });
        });
    }

    #[gpui::test]
    fn closing_the_last_tab_leaves_review_and_keeps_the_pane(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.open_dock_subagent(tab(), cx);
                app.close_dock_subagent(tab(), cx);
                assert_eq!(app.transcript.dock_tabs().active(), DockTab::Review);
                assert!(app.sidepane.read(cx).is_open());
            });
        });
    }

    #[gpui::test]
    fn the_dock_paints_the_strip_and_the_childs_log(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        let cx = cx.add_empty_window();
        cx.update(|_, cx| {
            app.update(cx, |app, cx| app.open_dock_subagent(tab(), cx));
        });
        let viewport = gpui::size(px(1400.), px(900.));
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("right-dock-subagent").is_some(), "the subagent view");
        assert!(cx.debug_bounds("right-dock-tab-review").is_some(), "Review stays a tab");
        assert!(cx.debug_bounds("right-dock-tab-0").is_some(), "the subagent's tab");
        assert!(cx.debug_bounds("subagent-panel-header").is_some(), "the panel header");
        assert!(cx.debug_bounds("subagent-panel-task").is_some(), "the full task");

        cx.update(|_, cx| {
            app.update(cx, |app, cx| app.activate_dock_tab(DockTab::Review, cx));
        });
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("side-pane").is_some(), "Review is back");
        assert!(cx.debug_bounds("right-dock-tab-0").is_some(), "with the strip");
        assert!(cx.debug_bounds("right-dock-subagent").is_none());
    }
}
