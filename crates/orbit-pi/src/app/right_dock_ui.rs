//! The right pane's tabs: Review plus the delegated subagents the reader
//! opened from a delegation card (see [`crate::right_dock`]).
//!
//! The tabs live on the session's transcript, so they park and return with
//! it. Showing the pane is the app-wide pane flag: only "Open in side pane"
//! sets it, so coming back to a session never pops a pane the reader hid.

use super::*;
use crate::right_dock::{DockTab, SubagentTab};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, TextSize};
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
        // The child's transcript goes with its tab; reopening replays the log.
        if let Some(path) = self.dock_child_log_path(tab) {
            self.child_logs.remove(&path);
        }
        if self.transcript.dock_tabs_mut().close(tab) {
            self.sidepane.update(cx, |_, cx| cx.notify());
            cx.notify();
        }
    }

    /// A tab's agent name, from the live row.
    fn dock_tab_agent(&self, tab: SubagentTab) -> String {
        self.transcript
            .subagent_run_at(tab.message_ix, tab.tool_ix)
            .and_then(|run| run.children.get(tab.child_ix).map(|child| child.agent.clone()))
            .filter(|agent| !agent.trim().is_empty())
            .unwrap_or_else(|| tr!("right_dock.subagent"))
    }

    /// A tab's label: the agent name, numbered by its place in the run when
    /// another open tab has the same name, so two `explore` children read
    /// `explore · 1` and `explore · 2` instead of one name twice.
    fn dock_tab_label(&self, tab: SubagentTab) -> SharedString {
        let agent = self.dock_tab_agent(tab);
        let shared = self
            .transcript
            .dock_tabs()
            .subagents()
            .iter()
            .filter(|other| **other != tab && self.dock_tab_agent(**other) == agent)
            .count()
            > 0;
        if shared {
            tr!("right_dock.tab_numbered", agent = agent, number = tab.child_ix + 1).into()
        } else {
            agent.into()
        }
    }

    /// The child a tab shows, when its delegation row still has it.
    fn dock_child(&self, tab: SubagentTab) -> Option<crate::subagent::SubagentChild> {
        self.transcript
            .subagent_run_at(tab.message_ix, tab.tool_ix)
            .and_then(|run| run.children.get(tab.child_ix).cloned())
    }

    /// The child's event log, when it has one Orbit may read on this machine
    /// (a remote host's log, or an older extension's run, has none).
    fn dock_child_log_path(&self, tab: SubagentTab) -> Option<PathBuf> {
        let raw = self.dock_child(tab)?.log?;
        crate::subagent_log::trusted_log_path(&raw, &crate::platform::home_dir())
    }

    /// Heartbeat: read what the open subagent tab's log gained since the last
    /// read, off the UI thread. Only the visible tab is read; a hidden one
    /// catches up from its offset when it is shown again.
    pub(super) fn tick_child_logs(&mut self, cx: &mut Context<Self>) {
        let DockTab::Subagent(tab) = self.transcript.dock_tabs().active() else {
            return;
        };
        if !self.sidepane.read(cx).is_open() {
            return;
        }
        let Some(path) = self.dock_child_log_path(tab) else {
            return;
        };
        let live = self.dock_child(tab).is_some_and(|child| child.is_live());
        let log = self
            .child_logs
            .entry(path.clone())
            .or_insert_with(crate::subagent_log::ChildLog::new);
        if log.reading || log.unreadable || log.drained {
            return;
        }
        log.reading = true;
        let offset = log.offset();
        cx.spawn(async move |this, cx| {
            let read_path = path.clone();
            let result = cx
                .background_executor()
                .spawn(async move { crate::subagent_log::read_appended(&read_path, offset) })
                .await;
            this.update(cx, |app, cx| app.on_child_log_read(&path, result, live, cx))
                .ok();
        })
        .detach();
    }

    fn on_child_log_read(
        &mut self,
        path: &Path,
        result: std::io::Result<Vec<u8>>,
        live: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(log) = self.child_logs.get_mut(path) else {
            return;
        };
        match result {
            Ok(bytes) => {
                let read_nothing = bytes.is_empty();
                if log.apply(&bytes) {
                    cx.notify();
                }
                // Settled and caught up: the file will not grow again.
                if read_nothing && !live {
                    log.drained = true;
                }
            }
            Err(_) => {
                log.reading = false;
                log.unreadable = true;
                cx.notify();
            }
        }
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
            let mut tab_el = Tab::new(
                ElementId::NamedInteger("right-dock-tab".into(), ix as u64),
                self.dock_tab_label(tab),
            );
            // The child's state leads its tab, the same mark its card row
            // wears, so a running, finished or failed child reads at a glance.
            if let Some(child) = self.dock_child(tab) {
                tab_el = tab_el.glyph(crate::transcript_view::subagent_status_mark(
                    &child,
                    child.is_live(),
                    ElementId::NamedInteger("right-dock-tab-spin".into(), ix as u64),
                    IconSize::XSmall.px(&theme),
                    theme,
                ));
            }
            strip = strip.child(
                tab_el
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
    /// the tab strip, and that child's live transcript — or, when the child
    /// has no readable event log, its summarized log.
    pub(super) fn render_subagent_dock(
        &self,
        theme: Theme,
        viewport_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let DockTab::Subagent(tab) = self.transcript.dock_tabs().active() else {
            return div().into_any_element();
        };
        let app = cx.entity();
        let header = self.dock_tab_strip(&app, theme, cx);
        let width = self.sidepane.read(cx).width();
        // The live transcript, once its first lines have been read: pi's own
        // events for this child, painted by the chat's own rows.
        let live_log = self
            .dock_child_log_path(tab)
            .and_then(|path| self.child_logs.get(&path))
            .filter(|log| !log.unreadable && !log.transcript.is_empty());
        if let (Some(log), Some(child)) = (live_log, self.dock_child(tab)) {
            let body = div()
                .debug_selector(|| "right-dock-subagent-transcript".to_string())
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                .children(header)
                .child(self.subagent_status_line(&child, theme))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .w_full()
                        .relative()
                        .child(log.transcript.render(
                            None,
                            viewport_height,
                            width,
                            self.agent_calls,
                            None,
                            Some(self.image_opener(cx)),
                            None,
                            None,
                            None,
                            None,
                            // A subagent's log has no composer of its own.
                            None,
                            None,
                            None,
                            cx,
                        )),
                )
                .into_any_element();
            return crate::sidepane::dock_shell("right-dock-subagent", width, theme, body)
                .into_any_element();
        }
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
        crate::sidepane::dock_shell("right-dock-subagent", width, theme, body).into_any_element()
    }

    /// One quiet line above a child's transcript: what state it is in and
    /// for how long. The task itself is the transcript's first message.
    fn subagent_status_line(
        &self,
        child: &crate::subagent::SubagentChild,
        theme: Theme,
    ) -> impl IntoElement {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0);
        div()
            .debug_selector(|| "right-dock-subagent-status".to_string())
            .flex_none()
            .w_full()
            .h(ButtonSize::Default.height(&theme))
            .px(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .border_b_1()
            .border_color(theme.hairline())
            .text_size(TextSize::Small.px(&theme))
            .text_color(theme.text_secondary)
            .child(crate::transcript_view::subagent_status_mark(
                child,
                child.is_live(),
                ElementId::Name("right-dock-subagent-status-spin".into()),
                IconSize::XSmall.px(&theme),
                theme,
            ))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(crate::transcript_view::subagent_status_label(child)),
            )
            .children(
                crate::subagent::format_elapsed(child.started_at, child.ended_at, now_ms).map(
                    |elapsed| {
                        div()
                            .flex_none()
                            .font(crate::ui::style::text::num_font())
                            .text_color(theme.text_muted)
                            .child(elapsed)
                    },
                ),
            )
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

    /// Two children of one delegation, both `explore`, each with an event log
    /// in Orbit's log directory (named only: nothing here touches the disk).
    fn twin_delegation() -> serde_json::Value {
        let log = |child: &str| {
            crate::subagent_log::log_dir(&crate::platform::home_dir())
                .join(format!("run1-{child}.jsonl"))
                .to_string_lossy()
                .into_owned()
        };
        json!({"messages": [
            {"role":"user", "content":"look around"},
            {"role":"assistant", "content":[{
                "type":"toolCall", "id":"t1", "name":"subagent",
                "arguments":{"tasks":[{"agent":"explore","task":"a"},{"agent":"explore","task":"b"}]}
            }]},
            {"role":"toolResult", "toolCallId":"t1", "toolName":"subagent",
             "content":[{"type":"text", "text":"working"}],
             "details": {
                "orbit":"subagent/1", "mode":"parallel",
                "children":[
                    {"agent":"explore", "status":"running", "task":"a", "phase":"tool", "log": log("c1")},
                    {"agent":"explore", "status":"done", "task":"b", "phase":"settled", "log": log("c2")}
                ]
             }}
        ]})
    }

    #[gpui::test]
    fn a_childs_log_paints_its_live_transcript(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let app = cx.update(|cx| cx.new(OrbitApp::new));
        let first = SubagentTab { message_ix: 1, tool_ix: 0, child_ix: 0 };
        let second = SubagentTab { child_ix: 1, ..first };
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.transcript.load_from(&twin_delegation());
                let path = app.dock_child_log_path(first).expect("a trusted log");
                let mut log = crate::subagent_log::ChildLog::new();
                let lines = [
                    json!({"type":"message_start","message":{"role":"user","content":"Task: a"}}),
                    json!({"type":"message_end","message":{"role":"user","content":"Task: a"}}),
                ]
                .map(|line| format!("{line}\n"))
                .concat();
                assert!(log.apply(lines.as_bytes()));
                app.child_logs.insert(path, log);
                app.open_dock_subagent(second, cx);
                app.open_dock_subagent(first, cx);
                // Same agent twice: each tab is numbered by its place in the run.
                assert_eq!(app.dock_tab_label(first).as_ref(), "explore · 1");
                assert_eq!(app.dock_tab_label(second).as_ref(), "explore · 2");
            });
        });
        let cx = cx.add_empty_window();
        let viewport = gpui::size(px(1400.), px(900.));
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("right-dock-subagent-transcript").is_some());
        assert!(cx.debug_bounds("right-dock-subagent-status").is_some());
        assert!(
            cx.debug_bounds("subagent-panel-header").is_none(),
            "the summary gives way to the transcript"
        );

        // The second child has a log path but nothing read yet: its summary.
        cx.update(|_, cx| {
            app.update(cx, |app, cx| app.activate_dock_tab(DockTab::Subagent(second), cx));
        });
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("right-dock-subagent-transcript").is_none());
        assert!(cx.debug_bounds("subagent-panel-header").is_some());

        // Closing a tab drops its transcript; reopening replays the log.
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.close_dock_subagent(first, cx);
                assert!(app.child_logs.is_empty());
            });
        });
    }
}
