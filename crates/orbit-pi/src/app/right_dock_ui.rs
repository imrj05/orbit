//! The Subagents dock: one entry per delegated subagent the reader opened
//! from a delegation card, above the selected child's own transcript (see
//! [`crate::right_dock`]). It is a right dock of its own — Review stays the
//! diff and nothing else (issue #190).
//!
//! The entries live on the session's transcript, so they park and return
//! with it. Showing the dock is the app-wide `subagent_dock_open` flag: only
//! a card's open control (or the top-bar chip) sets it, so coming back to a
//! session never pops a dock the reader hid.

use super::*;
use crate::right_dock::SubagentTab;
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, TextSize};
use crate::ui::button::IconButton;
use crate::ui::chip::{Badge, BadgeFill};
use crate::ui::prelude::*;
use crate::ui::row::{nav_row, nav_row_fill, NavRowHeight};

/// How many entries the list shows before it scrolls, so a wide fan-out
/// never pushes the selected child's transcript off the dock.
const ENTRY_ROWS_VISIBLE: f32 = 5.;

impl OrbitApp {
    /// Handed to the transcript: a delegation card's "Open in side pane".
    pub(super) fn dock_opener(&self, cx: &Context<Self>) -> crate::transcript_view::DockOpener {
        let this = cx.weak_entity();
        Rc::new(move |tab, _window, cx| {
            this.update(cx, |app, cx| app.open_dock_subagent(tab, cx))
                .ok();
        })
    }

    /// Open `tab` in the Subagents dock and show the dock: this is the
    /// reader's own click, so it may bring a hidden dock forward. It takes
    /// the right-dock slot from Review, the Explorer and session details.
    pub(super) fn open_dock_subagent(&mut self, tab: SubagentTab, cx: &mut Context<Self>) {
        self.transcript.dock_tabs_mut().open(tab);
        self.show_subagent_dock(cx);
    }

    /// Show the Subagents dock in the right-dock slot (one dock at a time).
    fn show_subagent_dock(&mut self, cx: &mut Context<Self>) {
        if self.sidepane.read(cx).is_open() {
            self.sidepane.update(cx, |pane, cx| pane.close(cx));
        }
        if self.project_panel.read(cx).is_open() {
            self.project_panel.update(cx, |panel, cx| panel.close(cx));
        }
        self.session_details_open = false;
        // The quota popover floats over the dock; do not leave it hanging.
        self.quota_popup_open = false;
        // Set last: closing Review above clears it through the pane observer.
        self.subagent_dock_open = true;
        cx.notify();
    }

    /// Hide the Subagents dock. Its entries stay on the session.
    pub(super) fn close_subagent_dock(&mut self, cx: &mut Context<Self>) {
        if self.subagent_dock_open {
            self.subagent_dock_open = false;
            cx.notify();
        }
    }

    /// The top-bar chip: show the dock, or hide it when it is showing.
    pub(super) fn toggle_subagent_dock(&mut self, cx: &mut Context<Self>) {
        if self.subagent_dock_visible(cx) {
            self.close_subagent_dock(cx);
        } else if !self.transcript.dock_tabs().is_empty() {
            self.show_subagent_dock(cx);
        }
    }

    pub(super) fn activate_dock_tab(&mut self, tab: SubagentTab, cx: &mut Context<Self>) {
        self.transcript.dock_tabs_mut().activate(tab);
        cx.notify();
    }

    /// Close one entry. Closing the last one hides the dock: there is
    /// nothing left for it to show, and Review is never the fallback.
    pub(super) fn close_dock_subagent(&mut self, tab: SubagentTab, cx: &mut Context<Self>) {
        // The child's transcript goes with its entry; reopening replays the log.
        if let Some(path) = self.dock_child_log_path(tab) {
            self.child_logs.remove(&path);
        }
        if self.transcript.dock_tabs_mut().close(tab) {
            if self.transcript.dock_tabs().is_empty() {
                self.subagent_dock_open = false;
            }
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

    /// Heartbeat: read what the selected entry's log gained since the last
    /// read, off the UI thread. Only the visible entry is read; a hidden one
    /// catches up from its offset when it is shown again.
    pub(super) fn tick_child_logs(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.transcript.dock_tabs().active() else {
            return;
        };
        if !self.subagent_dock_visible(cx) {
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

    /// The dock's header: what the surface is, how many entries it holds, and
    /// the dock's own close control — the same shape as session details'.
    fn subagent_dock_header(&self, theme: Theme, cx: &Context<Self>) -> impl IntoElement {
        let this = cx.entity();
        let count = self.transcript.dock_tabs().subagents().len();
        div()
            .debug_selector(|| "right-dock-subagents-header".to_string())
            .h(px(super::session::SESSION_DETAILS_HEADER_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .pl(DynamicSpacing::Base12.px(&theme))
            .pr(DynamicSpacing::Base06.px(&theme))
            .border_b_1()
            .border_color(theme.hairline())
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(tr!("right_dock.subagents")),
            )
            .children((count > 0).then(|| {
                Badge::new(
                    count.to_string(),
                    theme.text_muted,
                    BadgeFill::Solid(theme.overlay),
                )
                .render(&theme)
            }))
            .child(div().flex_1())
            .child(
                IconButton::new("right-dock-subagents-close", "icons/x.svg")
                    .tip(tr!("common.close"))
                    .faded(&theme, |button| {
                        button.on_click(move |_, _, cx| {
                            this.update(cx, |app, cx| app.close_subagent_dock(cx));
                        })
                    }),
            )
    }

    /// One row per opened subagent: its state mark, its name, how long it ran,
    /// and a close control. The selected row holds the selection wash; a
    /// click shows that child's response below.
    fn subagent_entry_list(&self, theme: Theme, cx: &Context<Self>) -> impl IntoElement {
        let app = cx.entity();
        let tabs = self.transcript.dock_tabs();
        let active = tabs.active();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or(0);
        let row_h = ButtonSize::Medium.height(&theme);
        let inset = DynamicSpacing::Base04.px(&theme);
        let rows = tabs.subagents().iter().copied().enumerate().map(|(ix, tab)| {
            let selected = active == Some(tab);
            let child = self.dock_child(tab);
            let mark = child.as_ref().map(|child| {
                crate::transcript_view::subagent_status_mark(
                    child,
                    child.is_live(),
                    ElementId::NamedInteger("right-dock-entry-spin".into(), ix as u64),
                    IconSize::XSmall.px(&theme),
                    theme,
                )
            });
            let elapsed = child.as_ref().and_then(|child| {
                crate::subagent::format_elapsed(child.started_at, child.ended_at, now_ms)
            });
            let select = app.clone();
            let close = app.clone();
            let row = nav_row(
                div().id(ElementId::NamedInteger("right-dock-entry".into(), ix as u64)),
                &theme,
                NavRowHeight::Item,
            )
            .debug_selector(move || format!("right-dock-tab-{ix}"))
            .pl(DynamicSpacing::Base08.px(&theme))
            .pr(DynamicSpacing::Base02.px(&theme))
            .gap(DynamicSpacing::Base06.px(&theme))
            .text_size(TextSize::Small.px(&theme))
            .children(mark)
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .when(selected, |label| label.font_weight(FontWeight::MEDIUM))
                    .text_color(if selected {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    })
                    .child(self.dock_tab_label(tab)),
            )
            .children(elapsed.map(|elapsed| {
                div()
                    .flex_none()
                    .font(crate::ui::style::text::num_font())
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(elapsed)
            }))
            .child(
                IconButton::new(
                    ElementId::NamedInteger("right-dock-entry-close".into(), ix as u64),
                    "icons/x.svg",
                )
                .size(ButtonSize::Compact)
                .tip(tr!("right_dock.close_entry"))
                .faded(&theme, |button| {
                    button.on_click(move |_, _, cx| {
                        // The row underneath must not select what is closing.
                        cx.stop_propagation();
                        close.update(cx, |app, cx| app.close_dock_subagent(tab, cx));
                    })
                }),
            )
            .on_click(move |_, _, cx| {
                select.update(cx, |app, cx| app.activate_dock_tab(tab, cx));
            });
            crate::motion::hover(
                row,
                format!("right-dock-entry-{ix}"),
                &theme,
                nav_row_fill(&theme, selected, false),
            )
        });
        div()
            .id("right-dock-subagent-entries")
            .debug_selector(|| "right-dock-subagent-entries".to_string())
            .flex_none()
            .w_full()
            .max_h(row_h * ENTRY_ROWS_VISIBLE + inset * 2.)
            .overflow_y_scroll()
            .track_scroll(&self.right_dock_scroll)
            .p(inset)
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme.hairline())
            .children(rows)
    }

    /// The Subagents dock: the shared dock frame, its header, one entry per
    /// opened subagent, and the selected child's live transcript — or, when
    /// the child has no readable event log, its summarized log.
    pub(super) fn render_subagent_dock(
        &self,
        theme: Theme,
        viewport_height: Pixels,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = self.sidepane.read(cx).docked_width();
        let frame = |content: AnyElement, this: &Self, cx: &Context<Self>| {
            let body = div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                .child(this.subagent_dock_header(theme, cx))
                .child(this.subagent_entry_list(theme, cx))
                .child(content)
                .into_any_element();
            crate::sidepane::dock_shell("right-dock-subagent", width, theme, body)
                .into_any_element()
        };
        // Closing the last entry slides the dock shut over its now-empty frame.
        let Some(tab) = self.transcript.dock_tabs().active() else {
            return frame(div().flex_1().into_any_element(), self, cx);
        };
        // The live transcript, once its first lines have been read: pi's own
        // events for this child, painted by the chat's own rows.
        let live_log = self
            .dock_child_log_path(tab)
            .and_then(|path| self.child_logs.get(&path))
            .filter(|log| !log.unreadable && !log.transcript.is_empty());
        if let (Some(log), Some(child)) = (live_log, self.dock_child(tab)) {
            let content = div()
                .debug_selector(|| "right-dock-subagent-transcript".to_string())
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
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
            return frame(content, self, cx);
        }
        let run = self.transcript.subagent_run_at(tab.message_ix, tab.tool_ix);
        let child = run.as_ref().and_then(|run| run.children.get(tab.child_ix));
        let summary = match (run.as_ref(), child) {
            (Some(run), Some(_)) => crate::transcript_view::render_subagent_panel(
                run,
                tab.child_ix,
                (tab.message_ix, tab.tool_ix),
                self.transcript.copied_sections(),
                theme,
            ),
            // The row changed shape under the entry: say so instead of a blank.
            _ => div()
                .debug_selector(|| "right-dock-subagent-missing".to_string())
                .p(DynamicSpacing::Base16.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("right_dock.subagent_missing"))
                .into_any_element(),
        };
        let content = div()
            .id("right-dock-subagent-scroll")
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .child(summary)
            .into_any_element();
        frame(content, self, cx)
    }

    /// The top bar's Subagents chip: shown once this session has an entry,
    /// it toggles the dock and stays lifted while the dock is showing.
    pub(super) fn render_subagents_control(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        if self.transcript.dock_tabs().is_empty() {
            return None;
        }
        let open = self.subagent_dock_visible(cx);
        Some(
            header_icon_button(
                "header-subagents",
                &theme,
                open,
                icon(
                    "icons/tools/task.svg",
                    ButtonSize::Medium.icon_size().px(&theme),
                    if open {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    },
                ),
            )
            .debug_selector(|| "header-subagents".into())
            .tip(if open {
                tr!("right_dock.hide_subagents")
            } else {
                tr!("right_dock.show_subagents")
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    this.menu_dismissed_at.clear();
                    this.toggle_subagent_dock(cx);
                }),
            ),
        )
    }

    /// One quiet line above a child's transcript: what state it is in. Its
    /// elapsed time is on its entry row; the task itself is the transcript's
    /// first message.
    fn subagent_status_line(
        &self,
        child: &crate::subagent::SubagentChild,
        theme: Theme,
    ) -> impl IntoElement {
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
    fn opening_a_subagent_shows_its_own_dock_not_review(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                assert!(!app.subagent_dock_visible(cx));
                app.open_dock_subagent(tab(), cx);
                assert!(app.subagent_dock_visible(cx), "the click shows the dock");
                assert!(!app.sidepane.read(cx).is_open(), "Review stays closed (#190)");
                assert_eq!(app.transcript.dock_tabs().active(), Some(tab()));
            });
        });
    }

    /// One right dock at a time: the Subagents dock takes the slot from
    /// Review, and opening Review (any path) takes it back.
    #[gpui::test]
    fn review_and_the_subagents_dock_trade_the_slot(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.sidepane.update(cx, |pane, cx| pane.show_review(cx));
                app.open_dock_subagent(tab(), cx);
                assert!(!app.sidepane.read(cx).is_open(), "opening a subagent closes Review");
                assert!(app.subagent_dock_visible(cx));
            });
        });
        cx.run_until_parked();
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.sidepane.update(cx, |pane, cx| pane.show_review(cx));
            });
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let app = app.read(cx);
            assert!(!app.subagent_dock_visible(cx), "Review takes the slot back");
            assert!(!app.subagent_dock_open, "and the dock does not linger behind it");
            assert_eq!(app.transcript.dock_tabs().subagents(), &[tab()], "the entry stays");
        });
    }

    #[gpui::test]
    fn the_top_bar_chip_toggles_the_dock(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.toggle_subagent_dock(cx);
                assert!(!app.subagent_dock_open, "no entries: nothing to show");
                app.open_dock_subagent(tab(), cx);
                app.toggle_subagent_dock(cx);
                assert!(!app.subagent_dock_visible(cx));
                app.toggle_subagent_dock(cx);
                assert!(app.subagent_dock_visible(cx));
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
                app.close_subagent_dock(cx);
                assert!(!app.subagent_dock_visible(cx));
                // Switch away (another session's transcript) and back.
                let parked = std::mem::replace(&mut app.transcript, Transcript::new());
                assert!(app.transcript.dock_tabs().is_empty());
                app.transcript = parked;
                assert_eq!(
                    app.transcript.dock_tabs().active(),
                    Some(tab()),
                    "the session keeps its entry"
                );
                assert!(
                    !app.subagent_dock_visible(cx),
                    "coming back must not pop the hidden dock open"
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
    fn closing_the_last_entry_hides_the_dock_without_opening_review(
        cx: &mut gpui::TestAppContext,
    ) {
        let app = app_with_delegation(cx);
        cx.update(|cx| {
            app.update(cx, |app, cx| {
                app.open_dock_subagent(tab(), cx);
                app.close_dock_subagent(tab(), cx);
                assert_eq!(app.transcript.dock_tabs().active(), None);
                assert!(!app.subagent_dock_open);
                assert!(!app.sidepane.read(cx).is_open(), "Review is not the fallback");
            });
        });
    }

    #[gpui::test]
    fn the_dock_paints_its_entries_and_the_childs_log(cx: &mut gpui::TestAppContext) {
        let app = app_with_delegation(cx);
        let cx = cx.add_empty_window();
        cx.update(|_, cx| {
            app.update(cx, |app, cx| app.open_dock_subagent(tab(), cx));
        });
        let viewport = gpui::size(px(1400.), px(900.));
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("right-dock-subagent").is_some(), "the subagent dock");
        assert!(cx.debug_bounds("right-dock-subagents-header").is_some(), "its header");
        assert!(cx.debug_bounds("right-dock-subagent-entries").is_some(), "its entries");
        assert!(cx.debug_bounds("right-dock-tab-0").is_some(), "the subagent's entry");
        assert!(cx.debug_bounds("header-subagents").is_some(), "the top-bar chip");
        assert!(cx.debug_bounds("side-pane").is_none(), "no Review beside it");
        assert!(cx.debug_bounds("subagent-panel-header").is_some(), "the panel header");
        assert!(cx.debug_bounds("subagent-panel-task").is_some(), "the full task");

        // Review is the diff alone: no subagent entry rides on it (#190).
        cx.update(|_, cx| {
            app.update(cx, |app, cx| app.open_review(cx));
        });
        cx.run_until_parked();
        let _ = cx.draw(gpui::point(px(0.), px(0.)), viewport, |_, _| app.clone());
        assert!(cx.debug_bounds("side-pane").is_some(), "Review is showing");
        assert!(cx.debug_bounds("right-dock-tab-0").is_none(), "with no subagent entry");
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
            app.update(cx, |app, cx| app.activate_dock_tab(second, cx));
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
