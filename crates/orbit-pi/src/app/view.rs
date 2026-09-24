use super::helpers::*;
use super::sidebar::*;
use super::*;

use gpui::StyledText;

use crate::widgets as ext_widgets;

/// Height of a page's top bar (DESIGN.md: 44px header rows). The new-task
/// backdrop is offset by it, so the picture starts below the title exactly
/// where it always has — this is the one value the two must agree on.
pub(super) const TOP_BAR_H: f32 = 44.;

/// Leading inset that clears the OS window buttons in the titlebar row: the
/// macOS traffic lights when the titlebar is transparent, or a plain edge
/// inset where the system titlebar holds them (see
/// [`platform::titlebar_options`]). The sidebar's drag strip and the main top
/// bar share it so the window's left controls never ride into the OS buttons.
pub(super) const TRAFFIC_LIGHT_CLEARANCE: f32 = platform::WINDOW_CONTROLS_CLEARANCE;

/// Space between the titlebar's left controls. Set to the same 8px the right
/// cluster spaces its chips with (`gap_2`), so the two ends of the bar share
/// one rhythm — a bare 2px let bordered chips read as one crowded block.
const TITLEBAR_CONTROLS_GAP: f32 = 8.;

/// Breathing room between the OS window buttons and the first titlebar
/// control. The traffic-light clearance alone ended flush against them, which
/// made the app's own controls read as part of the caption; the lead gives
/// the overlay its own left margin.
pub(super) const TITLEBAR_CONTROLS_LEAD: f32 = 12.;

/// Combined width of the titlebar's left controls (toggle + history) as laid
/// out by [`OrbitApp::titlebar_left_controls`]: three [`HEADER_CTRL_H`] boxes,
/// two [`TITLEBAR_CONTROLS_GAP`] gaps, and the trailing 6px gap.
pub(super) const TITLEBAR_CONTROLS_W: f32 = HEADER_CTRL_H * 3. + TITLEBAR_CONTROLS_GAP * 2. + 6.;

/// Space between the controls and the session title while the sidebar is
/// collapsed. The cluster's own trailing 6px pad is inside
/// [`TITLEBAR_CONTROLS_W`], so this is what actually separates the last chip
/// from the title — without it the two sit flush once the sidebar stops
/// providing the separation.
pub(super) const TITLEBAR_TITLE_GAP: f32 = 8.;

/// Where the main top bar's content starts when the sidebar is collapsed: past
/// the traffic lights, the controls' lead margin, the (overlaid) window
/// controls, and the gap that keeps the title off them.
pub(super) const TITLEBAR_LEADING: f32 =
    TRAFFIC_LIGHT_CLEARANCE + TITLEBAR_CONTROLS_LEAD + TITLEBAR_CONTROLS_W + TITLEBAR_TITLE_GAP;

/// Space kept between the last titlebar control and the sidebar's right edge
/// while the controls are right-aligned inside the sidebar's strip. The
/// sidebar's 6px resize handle lives against that edge, so the pad keeps the
/// chips off it (and off the sidebar's rounded boundary).
pub(super) const SIDEBAR_EDGE_PAD: f32 = 6.;

/// Where the titlebar's left controls sit. While the sidebar is open they
/// right-align inside its strip — [`SIDEBAR_EDGE_PAD`] off the column's right
/// edge, so they ride both a resize drag and the open/close slide. Collapsed,
/// there is no column to hug and the chips fall back to the fixed lead past
/// the OS window buttons.
///
/// Windows is the exception: it has no OS buttons on the left at all — the app
/// paints the caption itself, at the window's right
/// ([`platform::draws_window_controls`]) — so the cluster left-aligns on the
/// fixed lead whether the sidebar is open or not, the way a Windows app's own
/// titlebar controls sit. Nothing else changes there: the lead still clears
/// the window edge, and the sidebar's drag strip and wordmark live on other
/// rows of that column.
pub(super) fn titlebar_controls_left(sidebar_visible: bool, sidebar_width: f32) -> f32 {
    if sidebar_visible && !cfg!(windows) {
        sidebar_width - TITLEBAR_CONTROLS_W - SIDEBAR_EDGE_PAD
    } else {
        TRAFFIC_LIGHT_CLEARANCE + TITLEBAR_CONTROLS_LEAD
    }
}

/// Leading inset the full-window pages (Git, Usage, Files) give their headers.
/// With the sidebar open they start after it and only need the normal page
/// padding; collapsed, they own the window's left edge and must clear the
/// macOS traffic lights and the overlaid titlebar controls.
pub(super) fn page_header_leading(sidebar_visible: bool) -> f32 {
    if sidebar_visible {
        12.
    } else {
        TITLEBAR_LEADING
    }
}

/// Duration of the sidebar collapse/expand slide.
const SIDEBAR_SLIDE_MS: u64 = 180;

/// Page content kept clear above the terminal panel — the transcript *and*
/// the composer, which sits between them. Dragging the panel's top edge to the
/// top of the window must not collapse the conversation behind the input.
const TERMINAL_MAIN_RESERVE: f32 = 320.;

impl Render for OrbitApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A clicked banner asks for the window; bring it forward on the frame
        // that follows the click (macOS activates the app, but a minimized
        // window would otherwise stay behind).
        if self.activate_window_pending {
            self.activate_window_pending = false;
            window.activate_window();
        }
        let theme = *theme::get(cx);
        // `/`-command and `@`-file menu state derives from the composer text
        // every frame, so typing opens/closes/filters it without extra sync.
        self.sync_autocomplete(cx);
        let working_label = self.workspace_label();
        // Workspace groups (ordered by each group's most recently active
        // session). Only the active workspace is expanded by default; each
        // open group shows up to SIDEBAR_GROUP_SESSIONS_VISIBLE sessions
        // with per-group Show more / Show less toggles.
        let sidebar_sessions = self.sidebar_sessions();
        // A pinned session leads its project group; the sidebar sorts and
        // marks pinned rows from this snapshot (Orbit-owned state).
        let pinned: Rc<HashSet<PathBuf>> = Rc::new(crate::pins::all().paths());
        // Parked (background) sessions mid-run — they keep their row under a
        // collapsed workspace header, like the open session, so a live task
        // is never hidden by a collapse. Also drives the running loader.
        let running_paths: Rc<HashSet<PathBuf>> = Rc::new(
            self.lives
                .iter()
                .filter(|(_, parked)| parked.busy)
                .map(|(path, _)| path.clone())
                .collect(),
        );
        let side_rows = Rc::new(build_sidebar_rows(
            &sidebar_sessions,
            &self.workspaces,
            &working_label,
            &self.collapsed_workspaces,
            &self.expanded_workspace_groups,
            &self.expanded_session_groups,
            &pinned,
            &self.current_session_path,
            &running_paths,
        ));
        let old = self.sidebar_list.item_count();
        if old != side_rows.len() {
            self.sidebar_list.splice(0..old, side_rows.len());
        }
        let sessions_data = Rc::new(sidebar_sessions);
        let active_path = Rc::new(self.current_session_path.clone());
        let session_menu = Rc::new(self.session_menu.clone());
        let workspace_menu = Rc::new(self.workspace_menu.clone());
        let this = cx.entity();
        // The open session's agent activity drives the sidebar's running
        // loader (parked runs come in via `running_paths` above).
        let agent_running = self.busy || self.transcript.is_streaming();
        // Every live process (running or warm-idle) — guards delete.
        let live_paths: Rc<HashSet<PathBuf>> = Rc::new(self.lives.keys().cloned().collect());
        // Pinned header for the open, expanded workspace: it stays at the
        // top of the session list while its own sessions scroll, and the
        // next group's header pushes it away (see `sticky_sidebar_header`).
        // Its row is rendered twice while pinned (the real one scrolls under
        // the overlay), so the pinned index also tells the list to skip that
        // row's workspace menu — the overlay owns the single open popup.
        let sticky = sticky_sidebar_header(&self.sidebar_list, &side_rows, &working_label);
        let sticky_ix = sticky.as_ref().map(|sticky| sticky.ix);
        let sticky_header = sticky.map(|sticky| {
            div()
                .absolute()
                .top(sticky.top_offset)
                .left_0()
                .w_full()
                .bg(theme.bg_sidebar)
                .child(render_side_row(
                    &side_rows,
                    &sessions_data,
                    active_path.as_deref(),
                    sticky.ix,
                    &this,
                    agent_running,
                    &running_paths,
                    &pinned,
                    &live_paths,
                    session_menu.as_ref().as_ref(),
                    workspace_menu.as_ref().as_ref(),
                    theme,
                ))
                .into_any_element()
        });

        let workspace_label = self.workspace_label();
        // Focus ring on the composer box: the border strengthens while the
        // input is focused (focus changes refresh the window, so this
        // tracks without extra wiring).
        let composer_focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let review_workspace = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        // The rail gates on the main area's width (872px transcript
        // container), which excludes the sessions sidebar when visible.
        let viewport = window.viewport_size();
        // The right side pane is hidden while settings/onboarding own the
        // main area (same rule as the sessions sidebar).
        let pane_open = self.sidepane.read(cx).is_open();
        let pane_visible = pane_open
            && !self.settings_open
            && !self.usage_open
            && self.dependencies_ready();
        let pane_width = if pane_visible {
            self.sidepane.read(cx).width()
        } else {
            px(0.)
        };
        // Keep the pane's workspace in sync with the app (cheap no-op when
        // unchanged; a change marks Review stale).
        let pane_workspace = self.current_workspace.clone();
        self.sidepane
            .update(cx, |pane, cx| pane.set_workspace(pane_workspace, cx));

        // Bottom terminal panel. The chat branch is what renders it, so the
        // full-page surfaces (settings / Git / Usage) hide it by construction;
        // `terminal_visible` exists to stop a hidden shell requesting frames.
        let terminal_visible = self.terminal_panel.read(cx).is_open()
            && !self.settings_open
            && !self.usage_open
            && !self.git_open;
        let panel_workspace = review_workspace.clone();
        self.terminal_panel.update(cx, |panel, cx| {
            panel.set_workspace(panel_workspace, cx);
            panel.sync_active(terminal_visible, cx);
        });
        let pane_session = self.session_id.clone();
        let pane_latest_turn = self.latest_turn;
        self.sidepane.update(cx, |pane, cx| {
            pane.set_turn_context(pane_session, pane_latest_turn, cx)
        });
        let ai_action = self.ai_review_opener(cx);
        let ai_snapshot = self.ai_review_snapshot();
        self.sidepane.update(cx, |pane, cx| {
            pane.set_ai_review_action(ai_action);
            pane.set_ai_review(ai_snapshot, cx);
        });
        let git_workspace = self.current_workspace.clone();
        let git_provider = self.model_provider.clone();
        let git_model = self.model_id.clone();
        // Leading inset the full-window pages (Git/Usage/Files) give their
        // headers while the sidebar is collapsed.
        let page_leading = view::page_header_leading(self.sidebar_visible);
        self.git_panel.update(cx, |panel, cx| {
            panel.set_context(git_workspace, git_provider, git_model, cx);
            panel.set_chrome_leading(page_leading, cx);
        });
        // ── file viewer (Files surface) ── spans the main column, so it gives
        // its tab strip the same leading inset as the Git/Usage page headers;
        // it carries the caption-control clearance only while it owns the
        // window's right edge (no project panel, Review pane, or Settings).
        let viewer_reserve_controls = platform::draws_window_controls()
            && !self.settings_open
            && !self.project_panel.read(cx).is_open()
            && !pane_visible;
        self.file_viewer.update(cx, |viewer, cx| {
            viewer.set_chrome_leading(page_leading, cx);
            viewer.set_reserve_controls(viewer_reserve_controls, cx);
        });
        // ── project panel (right dock) ── hidden while Settings owns the
        // window, like the sessions sidebar. Sync the workspace each render;
        // a change rebuilds the tree off-thread. The Explorer and the Review
        // pane are mutually exclusive right docks: while Review is open the
        // tree stays closed.
        let explorer_visible = self.project_panel.read(cx).is_open()
            && !self.settings_open
            && !pane_open;
        let explorer_width = if explorer_visible {
            self.project_panel.read(cx).width()
        } else {
            px(0.)
        };
        let explorer_workspace = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok());
        let explorer_active = self.file_viewer.read(cx).active_display();
        // On Windows the caption is painted into the top-right corner. The dock
        // owns that corner whenever the Review pane is not open, so it carries
        // the control clearance itself (the pane does when it is open).
        let explorer_reserve_controls = platform::draws_window_controls() && !pane_visible;
        self.project_panel.update(cx, |panel, cx| {
            panel.set_workspace(explorer_workspace, cx);
            panel.set_active(explorer_active, cx);
            panel.set_reserve_controls(explorer_reserve_controls, cx);
            // Review is open, so the Explorer must not be: close it here to
            // catch every path that opens the pane (diff chip, transcript
            // cards, Git page file rows), not just the top-bar toggle.
            if pane_open {
                panel.close(cx);
            }
        });
        let main_width = viewport.width
            - if self.sidebar_visible && !self.settings_open {
                self.sidebar_width
            } else {
                px(0.)
            }
            - explorer_width
            - pane_width;
        // Composer toolbar compaction: below this column width the access
        // pill drops out and the model label clamps (Send stays reachable).
        let composer_compact = (main_width - px(32.)).min(px(CONTENT_MAX_W)) < px(600.);
        // The Usage page lays itself out against the real main-area width, so
        // its tables and grids never overflow the column it is given.
        if self.usage_open {
            let width = f32::from(main_width);
            self.usage.update(cx, |page, cx| {
                page.set_main_width(width, cx);
                page.set_header_leading(page_leading, cx);
            });
        }

        // ── top-bar right controls ──
        // When Review owns the right edge, or the chat column is tight, the
        // title and the chips collide. Compact the quota label and drop the
        // +/− chip (Review already shows the same stats).
        let compact_chrome = pane_visible || f32::from(main_width) < 720.;
        let mut top_controls = div().flex_none().flex().items_center().gap_2();
        // Provider quota — a compact, provider-independent headroom meter.
        // Hidden entirely on a pi without `quota.*` (or when no provider
        // reports anything), so the bar never shows a fabricated value.
        top_controls = top_controls.children(self.render_quota_pill(compact_chrome, cx));
        top_controls = top_controls.children(self.render_open_in_control(cx));
        if (self.added > 0 || self.removed > 0) && !pane_visible {
            top_controls = top_controls.child(
                header_chip(
                    div()
                        .id("top-diff-stats")
                        .h(px(HEADER_CTRL_H))
                        .px(px(9.))
                        .rounded(px(HEADER_CTRL_R))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer(),
                    &theme,
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(Self::on_open_uncommitted_review),
                )
                .child(
                    div()
                        .text_size(theme.ui_px(12.))
                        .text_color(theme.add_green)
                        .child(format!("+{}", self.added)),
                )
                .child(
                    div()
                        .text_size(theme.ui_px(12.))
                        .text_color(theme.del_red)
                        .child(format!("-{}", self.removed)),
                ),
            );
        }
        top_controls = top_controls
            .child(
                // Popup is a sibling of the info chip, not a child: clicks
                // inside the rename field must not bubble to the chip's
                // toggle (which would close the popover before Update runs).
                div()
                    .relative()
                    .children(self.render_session_details_popup(cx))
                    .child(
                        header_icon_button(
                            "info",
                            &theme,
                            self.session_details_open,
                            icon("icons/info.svg", 16., theme.text_2),
                        )
                        .on_mouse_up(MouseButton::Left, cx.listener(Self::on_info_click)),
                    ),
            )
            // Explorer (project panel) toggle — the right file tree.
            .child(
                header_icon_button(
                    "toggle-project-panel",
                    &theme,
                    explorer_visible,
                    icon(
                        "icons/folder.svg",
                        16.,
                        if explorer_visible {
                            theme.text
                        } else {
                            theme.text_2
                        },
                    ),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(Self::on_toggle_project_panel_click),
                ),
            )
            // side-pane toggle sits right after the about (info) button
            .child(
                header_icon_button(
                    "toggle-side-pane",
                    &theme,
                    pane_visible,
                    icon(
                        "icons/panel-right.svg",
                        16.,
                        if pane_visible {
                            theme.text
                        } else {
                            theme.text_2
                        },
                    ),
                )
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_toggle_side_pane)),
            )
            // Terminal toggle — the bottom panel (cmd-j).
            .child(
                header_icon_button(
                    "toggle-terminal",
                    &theme,
                    terminal_visible,
                    icon(
                        "icons/terminal.svg",
                        16.,
                        if terminal_visible {
                            theme.text
                        } else {
                            theme.text_2
                        },
                    ),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.on_toggle_terminal(&crate::ToggleTerminal, window, cx)
                    }),
                ),
            )
            // GitHub affordance: opens the full-page Git surface.
            .child(
                header_icon_button(
                    "open-git-github",
                    &theme,
                    self.git_open,
                    icon(
                        "icons/github.svg",
                        16.,
                        if self.git_open {
                            theme.text
                        } else {
                            theme.text_2
                        },
                    ),
                )
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_open_git_click)),
            );

        // A blocking extension dialog owns the keyboard while it is open. Focus
        // it (or its text field) once, on the first frame it appears — `tick`
        // has no window to focus with.
        let dialog_layer = self.dialog.clone();
        if let Some(dialog) = &dialog_layer {
            if self.dialog_focus_pending {
                self.dialog_focus_pending = false;
                window.focus(&dialog.read(cx).focus_handle(cx));
            }
        }
        // A custom-UI surface owns the keyboard while it is open; the newest
        // (top of the stack) takes focus, and closing the last hands focus back
        // to the composer so typing continues.
        if self.custom_ui_focus_pending {
            self.custom_ui_focus_pending = false;
            match self.custom_ui.last() {
                Some(ui) => window.focus(&ui.read(cx).focus_handle(cx)),
                None => self.input.read(cx).focus(window),
            }
        }
        // The inline approval bar owns the keyboard the same way.
        if self.approval_focus_pending {
            self.approval_focus_pending = false;
            window.focus(&self.approval_focus);
        }
        // The update modal owns the keyboard (Escape dismisses) once open.
        if self.updater_dialog_focus_pending {
            self.updater_dialog_focus_pending = false;
            window.focus(&self.updater_dialog_focus);
        }
        // The inline ask panel owns the keyboard while it is open.
        if self.ask_focus_pending {
            self.ask_focus_pending = false;
            let focus = self.ask_focus.clone();
            window.focus(&focus);
        }

        div()
            .size_full()
            .flex()
            .relative()
            .bg(theme.bg_main)
            .text_color(theme.text)
            .font_family(theme::ui_font_family())
            // Dropping files anywhere in the window attaches them (the
            // composer highlights when the drag passes over it).
            .on_drop(cx.listener(Self::on_file_drop))
            // ── sidebar ── (hidden while the settings surface is open —
            // settings is a full-window surface with its own nav, like the
            // reference UI). The panel stays mounted and slides: an animated
            // outer width clips a fixed-width inner column, so the content
            // never reflows mid-slide. The titlebar controls live in a fixed
            // overlay (below), so they hold their place while it moves.
            .children((!self.settings_open).then(|| {
                let open = self.sidebar_visible;
                let panel_w = f32::from(self.sidebar_width);
                let panel = div()
                    .flex_none()
                    .h_full()
                    .overflow_hidden()
                    .bg(theme.bg_sidebar)
                    .child(
                        div()
                            .id("sidebar")
                            .relative()
                            .w(self.sidebar_width)
                            .h_full()
                            .flex()
                            .flex_col()
                            // traffic-light strip (drag region); the window's
                            // left controls float above it in the titlebar
                            // overlay — right-aligned against this column's
                            // edge while the sidebar is open.
                            .child(window_drag_region(div().h(px(TOP_BAR_H)).w_full()))
                            // Resize handle: drag the sidebar's right edge to
                            // adjust its width. Kept fully inside the panel so
                            // the slide wrapper's clip doesn't halve its hit
                            // area. The drag-move listener lives on the root so
                            // the drag keeps tracking beyond the handle.
                            .child(
                                div()
                                    .id("sidebar-resize-handle")
                                    .absolute()
                                    .top_0()
                                    .bottom_0()
                                    .right_0()
                                    .w(px(6.))
                                    .cursor(CursorStyle::ResizeLeftRight)
                                    .hover(|style| style.bg(theme.accent.opacity(0.4)))
                                    .on_drag(SidebarResize, |_, _, _, cx| cx.new(|_| DragGhost)),
                            )
                            // brand — the Orbit wordmark, set over the nav column
                            .child(
                                div()
                                    .px_3()
                                    .pt(px(2.))
                                    .pb(px(6.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    // White mark on dark sidebars; the dark-ink
                                    // mark on light ones, where the white
                                    // wordmark vanishes. A compact fixed width
                                    // keeps the brand quiet above the nav rows.
                                    .child(embedded_image_w(
                                        if theme.mode == ThemeMode::Light {
                                            crate::app_icon::LOGO_DARK_ASSET
                                        } else {
                                            crate::app_icon::LOGO_ASSET
                                        },
                                        px(90.),
                                    )),
                            )
                            // nav — one primary action (New Task), one quiet row
                            // (Search); the switcher palette anchors under Search
                            .child(
                                div()
                                    .px_3()
                                    .pt_1()
                                    .pb_2()
                                    .flex()
                                    .flex_col()
                                    .gap_1()
                                    .child(self.sidebar_new_task_button(theme, cx))
                                    .child(self.sidebar_search_row(theme, cx))
                                    .child(self.sidebar_usage_row(theme, cx)),
                            )
                            // session list (scrolls), grouped by workspace — or
                            // the empty state when pi's store has no sessions
                            .child(if side_rows.is_empty() {
                                empty_sessions_state(theme).into_any_element()
                            } else {
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .flex()
                                    .flex_col()
                                    // section label — anchors the list below the nav
                                    .child(
                                        div()
                                            .px(px(14.))
                                            .pb(px(2.))
                                            .text_size(theme.ui_px(11.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_3)
                                            .child(tr!("sidebar.projects")),
                                    )
                                    .child(
                                        div()
                                            .id("sidebar-sessions")
                                            .flex_1()
                                            .min_h_0()
                                            .px_2()
                                            .relative()
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h_full()
                                                    .relative()
                                                    // Clips the pinned header as
                                                    // the next group pushes it
                                                    // up past the list top.
                                                    .overflow_hidden()
                                                    .child(
                                                        list(
                                                            self.sidebar_list.clone(),
                                                            move |ix, _window, cx| {
                                                                let row_workspace_menu =
                                                                    if sticky_ix == Some(ix) {
                                                                        None
                                                                    } else {
                                                                        workspace_menu
                                                                            .as_ref()
                                                                            .as_ref()
                                                                    };
                                                                render_side_row(
                                                                    &side_rows,
                                                                    &sessions_data,
                                                                    active_path.as_deref(),
                                                                    ix,
                                                                    &this,
                                                                    agent_running,
                                                                    &running_paths,
                                                                    &pinned,
                                                                    &live_paths,
                                                                    session_menu.as_ref().as_ref(),
                                                                    row_workspace_menu,
                                                                    *theme::get(cx),
                                                                )
                                                                .into_any_element()
                                                            },
                                                        )
                                                        .w_full()
                                                        .h_full(),
                                                    )
                                                    .children(sticky_header),
                                            ),
                                    )
                                    .into_any_element()
                            })
                            // footer — Settings row + connection status, set off
                            // from the session list by a hairline
                            .child(
                                div()
                                    .h(px(44.))
                                    .px_3()
                                    .border_t_1()
                                    .border_color(theme.border)
                                    .flex()
                                    .items_center()
                                    .child(
                                        div()
                                            .id("settings")
                                            .h(px(28.))
                                            .px(px(8.))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .gap(px(7.))
                                            .cursor_pointer()
                                            .hover(|s| s.bg(theme.bg_hover))
                                            .on_mouse_up(
                                                MouseButton::Left,
                                                cx.listener(Self::on_settings_gear_click),
                                            )
                                            .child(icon("icons/settings.svg", 14., theme.text_3))
                                            .child(
                                                div()
                                                    .text_size(theme.ui_px(12.))
                                                    .text_color(theme.text_2)
                                                    .child(tr!("common.settings")),
                                            ),
                                    )
                                    .child(div().flex_1())
                                    .when_some(
                                        self.sidebar_updater_button(theme, cx),
                                        |footer, button| {
                                            footer.child(button).child(div().w(px(8.)))
                                        },
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .child(div().size(px(6.)).rounded_full().bg(
                                                if self.client.is_some() {
                                                    theme.ok_green
                                                } else {
                                                    theme.stop_red
                                                },
                                            ))
                                            .child(
                                                div()
                                                    .text_size(theme.ui_px(11.))
                                                    .text_color(theme.text_3)
                                                    .child(if self.client.is_some() {
                                                        tr!("status.connected")
                                                    } else {
                                                        tr!("status.offline")
                                                    }),
                                            ),
                                    ),
                            ),
                    );
                let gen = self.sidebar_slide_gen;
                if gen == 0 || theme::reduce_motion(cx) {
                    // Settled state — including the first frame, so the panel
                    // doesn't slide open on launch.
                    panel
                        .w(px(if open { panel_w } else { 0. }))
                        .when(open, |p| p.border_r_1().border_color(theme.border))
                        .into_any_element()
                } else {
                    panel
                        .with_animation(
                            ElementId::Name(format!("sidebar-slide-{gen}").into()),
                            Animation::new(Duration::from_millis(SIDEBAR_SLIDE_MS))
                                .with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                            move |el, d| {
                                let w = if open {
                                    panel_w * d
                                } else {
                                    panel_w * (1.0 - d)
                                };
                                let el = el.w(px(w));
                                if w > 0.5 {
                                    el.border_r_1().border_color(theme.border)
                                } else {
                                    el
                                }
                            },
                        )
                        .into_any_element()
                }
            }))
            // ── main ──
            .child(if self.settings_open {
                self.render_settings(cx).into_any_element()
            } else if self.file_viewer.read(cx).is_open() {
                self.file_viewer.clone().into_any_element()
            } else if self.git_open {
                self.git_panel.clone().into_any_element()
            } else if self.usage_open {
                // Usage reads pi's store from disk, so it stays available even
                // when the runtime itself is missing.
                self.usage.clone().into_any_element()
            } else if !self.dependencies_ready() {
                // Missing runtime pieces (pi / node): show the setup page
                // with install commands instead of the empty composer.
                self.render_onboarding(cx).into_any_element()
            } else if self.setup_open {
                // The same page on request, from Settings → About.
                self.render_onboarding(cx).into_any_element()
            } else {
                let empty = self.transcript.is_empty();
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .flex_col()
                    // `min_w_0` lets the center shrink below its content's
                    // min-content width, so opening the side pane (or widening
                    // the sidebar) reflows the transcript instead of holding
                    // the column at a fixed width.
                    .min_w_0()
                    .min_h_0()
                    .relative()
                    // The new-task backdrop belongs to the whole column, not
                    // just the empty-state slot, so the floating composer and
                    // the status bar below it paint over the picture instead
                    // of sitting on a flat slab.
                    .children(Self::new_task_backdrop(theme, empty))
                    // top bar — the window's left controls are a fixed overlay
                    // (see below), so the title only has to clear them when the
                    // sidebar is collapsed. Its leading inset eases in step with
                    // the sidebar slide. The drag spacer between it and the
                    // right cluster drags the window.
                    .child({
                        let bar = div()
                            .h(px(TOP_BAR_H))
                            .w_full()
                            .flex()
                            .items_center()
                            // Caption buttons are drawn above the bar, so its
                            // right cluster stops short of them — unless the
                            // side pane owns the window's right edge, and
                            // carries the clearance itself.
                            .pr(px(
                                if platform::draws_window_controls()
                                    && !pane_visible
                                    && !explorer_visible
                                {
                                    platform::WINDOW_CONTROLS_W
                                } else {
                                    12.
                                },
                            ))
                            .child(
                                window_drag_region(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .h_full()
                                        .flex()
                                        .items_center(),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .truncate()
                                        .text_size(theme.ui_px(13.))
                                        .text_color(theme.text_2)
                                        .child(session_display_title(
                                            self.session_name.as_deref(),
                                            self.current_title.as_deref(),
                                        )),
                                ),
                            )
                            .child(top_controls);
                        let gen = self.sidebar_slide_gen;
                        if gen == 0 || theme::reduce_motion(cx) {
                            bar.pl(px(if self.sidebar_visible {
                                20.
                            } else {
                                TITLEBAR_LEADING
                            }))
                            .into_any_element()
                        } else {
                            let expanding = self.sidebar_visible;
                            bar.with_animation(
                                ElementId::Name(format!("topbar-lead-{gen}").into()),
                                Animation::new(Duration::from_millis(SIDEBAR_SLIDE_MS))
                                    .with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                                move |el, d| {
                                    let (from, to) = if expanding {
                                        (TITLEBAR_LEADING, 20.)
                                    } else {
                                        (20., TITLEBAR_LEADING)
                                    };
                                    el.pl(px(from + (to - from) * d))
                                },
                            )
                            .into_any_element()
                        }
                    })
                    // transcript (centered column) or empty state
                    .child(if empty {
                        self.render_empty_state(main_width, cx).into_any_element()
                    } else {
                        div()
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .relative()
                            .child(self.transcript.render(
                                review_workspace.as_deref(),
                                window.viewport_size().height,
                                main_width,
                                Some(self.review_opener(cx)),
                                Some(self.image_opener(cx)),
                                self.search_hits(),
                                self.search_active(),
                                cx,
                            ))
                            // In-transcript find bar (⌘F), floating over the
                            // top-right of the transcript.
                            .children(self.transcript_search_bar(cx))
                            .into_any_element()
                    })
                    // floating composer + status bar — one centered column
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .px_4()
                            .pb_4()
                            // One centered column: composer + status bar share
                            // the same max width so the folder/meta row always
                            // aligns to the composer's edges.
                            .child(
                                div()
                                    // A stable element identity keeps the
                                    // popovers and inline bars below from
                                    // re-keying when a sibling appears, so
                                    // their animations never restart mid-way.
                                    .id("composer-column")
                                    .max_w(px(CONTENT_MAX_W))
                                    .w_full()
                                    .flex()
                                    .flex_col()
                                    // `/`-command and `@`-file menu — anchored
                                    // above the composer box (same deferred
                                    // + anchored pattern as the chip pickers)
                                    .children(self.autocomplete_popup(cx))
                                    // Command/protocol failures, above the
                                    // queue and composer.
                                    .children(self.error_banner(theme, cx))
                                    // In-flight retry / compaction state —
                                    // persistent while active, never a
                                    // transient status line.
                                    .children(self.run_status_strip(cx))
                                    // Access-guard approval — inline, above
                                    // the queue and composer (no scrim modal).
                                    .children(self.ask_panel(cx))
                                    .children(self.approval_bar(cx))
                                    // Queued follow-ups wait here (sticky above
                                    // the composer) until the task finishes.
                                    .children(self.queue_bar(cx))
                                    // Extension `setWidget` blocks placed
                                    // above the editor.
                                    .children(self.extension_widgets_above(cx))
                                    // Plan progress (Plan/Build only) — a slim
                                    // strip hugging the composer, above it.
                                    .children(self.workflow_progress(cx))
                                    // composer box — the picker popups are
                                    // anchored above their own chips
                                    .child(
                                        div()
                                            .id("composer-box")
                                            .w_full()
                                            .relative()
                                            .bg(theme.bg_composer)
                                            .border_1()
                                            .border_color(if self.file_drag_hovered {
                                                theme.accent
                                            } else if composer_focused {
                                                theme.border_strong
                                            } else {
                                                theme.border
                                            })
                                            .rounded_xl()
                                            .shadow(theme.composer_shadow())
                                            .px(theme.space(12.))
                                            .pt(theme.space(8.))
                                            .pb(theme.space(8.))
                                            // Base interface font for the input
                                            // (scales with the UI font-size
                                            // setting); the editor inherits it.
                                            .text_size(theme.ui_px(14.))
                                            .flex()
                                            .flex_col()
                                            .gap(theme.space(8.))
                                            .on_mouse_up(
                                                MouseButton::Left,
                                                cx.listener(Self::on_composer_click),
                                            )
                                            .on_drag_move(cx.listener(Self::on_file_drag_move))
                                            .children(self.attachments_row(cx))
                                            .child(self.input.clone())
                                            .child(self.composer_row(composer_compact, cx))
                                            // Drop-target overlay: fades
                                            // in over the box while files are
                                            // dragged across it. Absolute, so
                                            // highlighting never shifts layout.
                                            .children(self.file_drag_hovered.then(|| {
                                                let overlay = div()
                                                    .absolute()
                                                    .inset_0()
                                                    .rounded_xl()
                                                    .bg(theme.bg_composer.opacity(0.92))
                                                    .border_1()
                                                    .border_color(theme.accent)
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .gap_2()
                                                    .child(icon(
                                                        "icons/plus.svg",
                                                        14.,
                                                        theme.accent,
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_size(theme.ui_px(12.5))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(theme.accent)
                                                            .child(tr!("composer.drop_to_attach")),
                                                    );
                                                if theme::reduce_motion(cx) {
                                                    overlay.into_any_element()
                                                } else {
                                                    overlay
                                                        .with_animation(
                                                            "drop-overlay",
                                                            Animation::new(Duration::from_millis(
                                                                120,
                                                            )),
                                                            |overlay, delta| overlay.opacity(delta),
                                                        )
                                                        .into_any_element()
                                                }
                                            })),
                                    )
                                    // Extension `setWidget` blocks placed
                                    // below the editor.
                                    .children(self.extension_widgets_below(cx))
                                    .child(self.status_bar(&workspace_label, cx)),
                            ),
                    )
                    // bottom terminal panel — the last row of the page, under
                    // the composer, so it hugs the window's bottom edge
                    .children(
                        terminal_visible.then(|| self.terminal_panel.clone().into_any_element()),
                    )
                    .into_any_element()
            })
            // ── project panel ── the Explorer's right dock, between the main
            // column and the Review pane so the pane keeps the window edge.
            .children(explorer_visible.then(|| self.project_panel.clone().into_any_element()))
            // ── right side pane (Review) ──
            .children(pane_visible.then(|| self.sidepane.clone().into_any_element()))
            // ── titlebar controls ── a fixed overlay pinned just past the
            // macOS traffic lights, above both the sidebar and the main column.
            // Shown on every surface except Settings, which owns its own nav
            // column — including the Git/Usage pages when the sidebar is
            // collapsed, so the toggle (and history arrows) stay reachable;
            // those pages inset their own headers to clear it. The controls
            // right-align inside the sidebar's strip when it is open (hugging
            // its edge, and tracking it through a resize or the slide) and
            // fall back to the fixed lead when it is collapsed; on Windows,
            // which has no left-hand OS buttons, they take that fixed lead
            // whether it is open or not. The container
            // is not itself a hitbox, so the drag strip beneath still drags the
            // window in the gaps between buttons while each button takes its
            // own clicks.
            .children((!self.settings_open).then(|| {
                let pinned = titlebar_controls_left(false, f32::from(self.sidebar_width));
                let hugging = titlebar_controls_left(true, f32::from(self.sidebar_width));
                let controls = div()
                    .absolute()
                    .top_0()
                    .h(px(TOP_BAR_H))
                    .flex()
                    .items_center()
                    .child(self.titlebar_left_controls(theme, cx));
                let gen = self.sidebar_slide_gen;
                if gen == 0 || theme::reduce_motion(cx) {
                    controls
                        .left(px(if self.sidebar_visible {
                            hugging
                        } else {
                            pinned
                        }))
                        .into_any_element()
                } else {
                    // Same easing and duration as the panel's own slide, so the
                    // chips ride the edge instead of snapping to it.
                    let expanding = self.sidebar_visible;
                    controls
                        .with_animation(
                            ElementId::Name(format!("titlebar-lead-{gen}").into()),
                            Animation::new(Duration::from_millis(SIDEBAR_SLIDE_MS))
                                .with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                            move |el, d| {
                                let (from, to) = if expanding {
                                    (pinned, hugging)
                                } else {
                                    (hugging, pinned)
                                };
                                el.left(px(from + (to - from) * d))
                            },
                        )
                        .into_any_element()
                }
            }))
            // ── window caption buttons ── the app owns the caption on the
            // platforms where it draws it (`platform::draws_window_controls`),
            // so these sit above *every* surface — settings, git, usage,
            // onboarding, and the session view — and only the headers that
            // reach the window's right edge keep their content clear of them.
            // Drawn before the deferred layers, so an open popover still wins.
            .children(platform::draws_window_controls().then(|| {
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .h(px(TOP_BAR_H))
                    .flex()
                    .items_center()
                    .child(window_controls(theme, window.is_maximized()))
                    .into_any_element()
            }))
            // ── command palette (⌘P) — a full-window deferred layer above
            // every other floating surface; the entity renders its own
            // absolute scrim + centered card.
            .children(
                self.command_palette
                    .clone()
                    .map(|palette| command_palette::layer(palette).into_any_element()),
            )
            // ── extension dialog (select / confirm / input / editor) — a
            // blocking modal above every other surface; pi holds the run until
            // the user answers. It cancels the incoming request otherwise.
            .children(dialog_layer.map(|dialog| crate::dialog::layer(dialog).into_any_element()))
            // ── extension custom UI (`ctx.ui.custom`) — the component's own
            // rendered frames, stacked with the newest on top.
            .children(
                self.custom_ui
                    .iter()
                    .map(|ui| crate::custom_ui::layer(ui.clone()).into_any_element()),
            )
            // ── update modal — the search, changelog, and install decision,
            // opened by the download control and Check for Updates. Below the
            // extension dialog (a run blocks on it) and the lightbox.
            .children(self.updater_dialog_layer(window, cx))
            // ── image lightbox — full-window, above everything; opened from a
            // transcript image tile, dismissed by click or Escape.
            .children(
                self.lightbox
                    .clone()
                    .map(|image| self.lightbox_layer(image, cx)),
            )
            // ── toasts — the in-app stack, topmost so a notification is
            // never buried by whatever surface happens to be open.
            .children(self.toast_layer(cx))
            .track_focus(&self.focus_handle(cx))
            // Sidebar resize: fires for every mouse move while the handle
            // drag is active, wherever the pointer travels.
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<SidebarResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    let max = event.bounds.size.width - px(400.);
                    let width = event
                        .event
                        .position
                        .x
                        .clamp(px(SIDEBAR_MIN_W), max.max(px(SIDEBAR_MIN_W)));
                    if width != app.sidebar_width {
                        app.sidebar_width = width;
                        cx.notify();
                    }
                },
            ))
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<crate::explorer::ExplorerResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    // The dock sits between the main column and the Review
                    // pane, so its width is the pointer's distance to its
                    // right edge — which the pane owns when it is open.
                    let pane = if app.sidepane.read(cx).is_open()
                        && !app.settings_open
                        && !app.usage_open
                        && app.dependencies_ready()
                    {
                        app.sidepane.read(cx).width()
                    } else {
                        px(0.)
                    };
                    let width = event.bounds.size.width - pane - event.event.position.x;
                    app.project_panel
                        .update(cx, |panel, cx| panel.set_width(width, cx));
                },
            ))
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<SidePaneResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    // The pane hugs the window's right edge, so its width is
                    // the distance from the pointer to that edge.
                    let width = event.bounds.size.width - event.event.position.x;
                    let max = event.bounds.size.width - px(PANE_MAX_RESERVE);
                    app.sidepane.update(cx, |pane, cx| {
                        pane.set_width(width.min(max), cx);
                    });
                },
            ))
            .on_drag_move(cx.listener(
                |app: &mut Self,
                 event: &DragMoveEvent<TerminalResize>,
                 _: &mut Window,
                 cx: &mut Context<Self>| {
                    // The panel hugs the window's bottom edge, so its height is
                    // the distance from the pointer to that edge, capped so the
                    // transcript above it keeps a usable height.
                    let max = (event.bounds.size.height - px(TERMINAL_MAIN_RESERVE))
                        .max(px(TERMINAL_MAIN_RESERVE));
                    let height = (event.bounds.size.height - event.event.position.y).min(max);
                    app.terminal_panel
                        .update(cx, |panel, cx| panel.set_height(height, cx));
                },
            ))
            .on_action(cx.listener(Self::on_submit))
            .on_action(cx.listener(Self::on_steer))
            .on_action(cx.listener(Self::on_autocomplete_accept))
            .on_action(cx.listener(Self::on_abort))
            .on_action(cx.listener(Self::on_new_session))
            .on_action(cx.listener(Self::on_refresh))
            .on_action(cx.listener(Self::on_copy_last_response))
            .on_action(cx.listener(Self::on_prev_turn))
            .on_action(cx.listener(Self::on_next_turn))
            .on_action(cx.listener(Self::on_open_settings))
            .on_action(cx.listener(Self::on_open_about))
            .on_action(cx.listener(Self::on_toggle_usage))
            .on_action(cx.listener(Self::on_toggle_project_panel))
            .on_action(cx.listener(Self::on_close_files))
            .on_action(cx.listener(Self::on_toggle_terminal))
            .on_action(cx.listener(Self::on_toggle_command_palette))
            .on_action(cx.listener(Self::on_check_for_updates))
            .on_action(cx.listener(Self::on_toggle_search))
            .on_action(cx.listener(|this, _: &crate::GitTabChanges, window, cx| {
                this.on_git_tab(0, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabHistory, window, cx| {
                this.on_git_tab(1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabGraph, window, cx| {
                this.on_git_tab(2, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabIssues, window, cx| {
                this.on_git_tab(3, window, cx)
            }))
            .on_action(cx.listener(|this, _: &crate::GitTabPulls, window, cx| {
                this.on_git_tab(4, window, cx)
            }))
    }
}

impl OrbitApp {
    /// The extension widgets placed above the composer, if any.
    pub(super) fn extension_widgets_above(&self, cx: &Context<Self>) -> Option<AnyElement> {
        self.extension_widgets_element(WidgetPlacement::AboveEditor, cx)
    }

    /// The extension widgets placed below the composer, if any.
    pub(super) fn extension_widgets_below(&self, cx: &Context<Self>) -> Option<AnyElement> {
        self.extension_widgets_element(WidgetPlacement::BelowEditor, cx)
    }

    /// Render every keyed `setWidget` block for one placement as a small
    /// monospace card. The lines keep the extension's own SGR coloring (see
    /// [`crate::widgets`]), so a todo list or status block reads as intended.
    fn extension_widgets_element(
        &self,
        placement: WidgetPlacement,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let items: Vec<&ExtensionWidget> = self
            .extension_widgets
            .iter()
            .filter(|widget| widget.placement == placement && !widget.lines.is_empty())
            .collect();
        if items.is_empty() {
            return None;
        }
        let theme = *theme::get(cx);
        let font = ext_widgets::widget_font();
        let mut column = div().w_full().flex().flex_col().gap(theme.space(6.));
        for widget in items {
            let mut card = div()
                .id(ElementId::Name(format!("ext-widget-{}", widget.key).into()))
                .w_full()
                .px(theme.space(12.))
                .py(theme.space(8.))
                .rounded(px(10.))
                .border_1()
                .border_color(theme.border)
                .bg(theme.code_bg)
                .font_family(theme::code_font_family())
                .text_size(theme.code_px(12.))
                .line_height(theme.code_px(18.))
                .text_color(theme.code_text)
                .flex()
                .flex_col()
                .gap(theme.space(2.));
            for line in &widget.lines {
                let (text, runs) = ext_widgets::styled_line(line, &theme, &font);
                card = card.child(StyledText::new(text).with_runs(runs));
            }
            column = column.child(card);
        }
        Some(column.into_any_element())
    }

    /// Bottom row inside the composer: the "+" add menu and the access-mode
    /// chip on the left; model / thinking chips and the round send button on
    /// the right. `compact` (narrow window) drops the access chip and clamps
    /// the model label so Send always stays reachable.
    pub(super) fn composer_row(
        &self,
        compact: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("composer-row")
            .flex()
            .items_center()
            .gap_2()
            .child(self.add_menu_button(cx))
            // Access mode: a real control now — it selects the guard policy
            // the bundled extension enforces. First thing to yield when the
            // row gets narrow.
            .when(!compact, |row| row.child(self.access_chip(cx)))
            // Workflow mode: the session's scope (Plan / Build / Ask), enforced
            // by the workflow extension. Sits beside the access chip.
            .when(!compact, |row| row.child(self.workflow_chip(cx)))
            .child(div().flex_1())
            .child(self.model_chip(compact, cx))
            .child(self.thinking_chip(cx))
            .children(self.steer_button(cx))
            .child(self.send_button(cx))
    }

    /// The access-mode chip: a lock glyph, the active mode's label, and a
    /// caret that turns over when the picker opens. Ghost style until
    /// hovered/open, matching the model chip.
    pub(super) fn access_chip(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.access_popup(cx))
            .child(
                div()
                    .id("access-chip")
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px(px(7.))
                    .h(px(24.))
                    .rounded_md()
                    .text_size(theme.ui_px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay))
                    .when(self.access_menu_open, |chip| {
                        chip.bg(theme.active).text_color(theme.active_fg)
                    })
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(Self::on_access_trigger_click),
                    )
                    .child(icon(self.access_mode.icon(), 12., theme.text_3))
                    .child(
                        div()
                            .text_color(if self.access_menu_open {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(self.access_mode.label()),
                    )
                    .child(Self::chip_caret(
                        self.access_menu_open,
                        theme.active_fg,
                        "access-caret-turn",
                        cx,
                    )),
            )
    }

    /// The shared chip caret. It turns a half-turn when the picker opens —
    /// animated on open (reduce-motion aware) so the turn reads as a
    /// transition, and resting flat when closed so there is no reverse
    /// flicker. `fg` is the caret color while open.
    fn chip_caret(
        open: bool,
        fg: gpui::Hsla,
        animation_id: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = *theme::get(cx);
        let svg = gpui::svg()
            .path("icons/chevron-down.svg")
            .flex_none()
            .size(px(10.))
            .text_color(if open { fg } else { theme.text_3 })
            .into_any_element();
        if !open {
            return svg;
        }
        if theme::reduce_motion(cx) {
            return gpui::svg()
                .path("icons/chevron-down.svg")
                .flex_none()
                .size(px(10.))
                .text_color(fg)
                .with_transformation(Transformation::rotate(radians(std::f32::consts::PI)))
                .into_any_element();
        }
        gpui::svg()
            .path("icons/chevron-down.svg")
            .flex_none()
            .size(px(10.))
            .text_color(fg)
            .with_animation(
                animation_id,
                Animation::new(Duration::from_millis(150)).with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                |svg, d| {
                    svg.with_transformation(Transformation::rotate(radians(
                        std::f32::consts::PI * d,
                    )))
                },
            )
            .into_any_element()
    }

    /// The access-mode picker popup, while open. Minimal rows: an icon tile,
    /// the mode name over its one-line description, and an accent check on the
    /// active mode. The popup owns the keyboard via the `AccessMenu` context
    /// and eases in rather than popping.
    pub(super) fn access_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.access_menu_open {
            return None;
        }
        let theme = *theme::get(cx);
        let this = cx.weak_entity();
        let mut list = div().w_full().p(px(5.)).flex().flex_col().gap(px(2.));
        for (ix, mode) in AccessMode::ALL.iter().enumerate() {
            let mode = *mode;
            let highlighted = ix == self.access_menu_highlight;
            let selected = mode == self.access_mode;
            let this = this.clone();
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("access-row".into(), ix as u64))
                    .w_full()
                    .px(px(8.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .when(highlighted, |row| row.bg(theme.overlay_strong))
                    .when(selected && !highlighted, |row| {
                        row.bg(theme.accent.opacity(0.1))
                    })
                    .hover(|style| style.bg(theme.overlay_strong))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.access_menu_highlight != ix {
                                    app.access_menu_highlight = ix;
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this = cx.weak_entity();
                        move |_, window, cx| {
                            this.update(cx, |app, cx| app.run_access_menu_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(px(26.))
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if selected {
                                theme.accent.opacity(0.16)
                            } else {
                                theme.overlay
                            })
                            .child(icon(
                                mode.icon(),
                                14.,
                                if selected { theme.accent } else { theme.text_3 },
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(theme.ui_px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if selected { theme.text } else { theme.text_2 })
                                    .child(mode.label()),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .whitespace_normal()
                                    .text_size(theme.ui_px(11.))
                                    .text_color(theme.text_3)
                                    .child(mode.description()),
                            ),
                    )
                    .when(selected, |row| {
                        row.child(icon("icons/check.svg", 13., theme.accent))
                    }),
            );
        }

        let popup = div()
            .w(px(300.))
            .font_family(theme::ui_font_family())
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .key_context("AccessMenu")
            .track_focus(&self.access_menu_focus)
            .on_action(cx.listener(Self::on_access_menu_next))
            .on_action(cx.listener(Self::on_access_menu_prev))
            .on_action(cx.listener(Self::on_access_menu_confirm))
            .on_action(cx.listener(Self::on_access_menu_close))
            .on_mouse_down_out(cx.listener(|app, _, window, cx| {
                app.menu_dismissed_at = Some(Instant::now());
                app.close_access_menu(window, cx);
            }))
            .child(list);

        let popup: AnyElement = if theme::reduce_motion(cx) {
            popup.into_any_element()
        } else {
            popup
                .with_animation(
                    "access-menu-in",
                    Animation::new(Duration::from_millis(130))
                        .with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                    |el, d| el.opacity(d),
                )
                .into_any_element()
        };

        Some(
            anchored()
                .position_mode(AnchoredPositionMode::Local)
                .anchor(Corner::BottomLeft)
                .offset(point(px(0.), px(-4.)))
                .snap_to_window()
                .child(deferred(popup))
                .into_any_element(),
        )
    }

    /// The workflow-mode chip: the active session's scope, opening the
    /// Plan / Build / Ask picker. Ghost style until hovered/open, matching the
    /// access and model chips.
    pub(super) fn workflow_chip(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.workflow_popup(cx))
            .child(
                div()
                    .id("workflow-chip")
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px(px(7.))
                    .h(px(24.))
                    .rounded_md()
                    .text_size(theme.ui_px(12.))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay))
                    .when(self.workflow_menu_open, |chip| {
                        chip.bg(theme.active).text_color(theme.active_fg)
                    })
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(Self::on_workflow_trigger_click),
                    )
                    .child(icon(self.workflow_mode.icon(), 12., theme.text_3))
                    .child(
                        div()
                            .text_color(if self.workflow_menu_open {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(self.workflow_mode.label()),
                    )
                    // Read-only modes cannot edit; the lock says so at a glance,
                    // matching the access chip's motif.
                    .when(self.workflow_mode.is_read_only(), |chip| {
                        chip.child(icon("icons/lock.svg", 10., theme.text_3))
                    })
                    .child(Self::chip_caret(
                        self.workflow_menu_open,
                        theme.active_fg,
                        "workflow-caret-turn",
                        cx,
                    )),
            )
    }

    /// The workflow-mode picker popup, while open. Same construction as the
    /// access popup: an icon tile, the mode over its one-line hint, and an
    /// accent check on the active mode.
    pub(super) fn workflow_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.workflow_menu_open {
            return None;
        }
        let theme = *theme::get(cx);
        let this = cx.weak_entity();
        let mut list = div().w_full().p(px(5.)).flex().flex_col().gap(px(2.));
        for (ix, mode) in WorkflowMode::ALL.iter().enumerate() {
            let mode = *mode;
            let highlighted = ix == self.workflow_menu_highlight;
            let selected = mode == self.workflow_mode;
            let this = this.clone();
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("workflow-row".into(), ix as u64))
                    .w_full()
                    .px(px(8.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .when(highlighted, |row| row.bg(theme.overlay_strong))
                    .when(selected && !highlighted, |row| {
                        row.bg(theme.accent.opacity(0.1))
                    })
                    .hover(|style| style.bg(theme.overlay_strong))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.workflow_menu_highlight != ix {
                                    app.workflow_menu_highlight = ix;
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this = cx.weak_entity();
                        move |_, window, cx| {
                            this.update(cx, |app, cx| app.run_workflow_menu_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(px(26.))
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(if selected {
                                theme.accent.opacity(0.16)
                            } else {
                                theme.overlay
                            })
                            .child(icon(
                                mode.icon(),
                                14.,
                                if selected { theme.accent } else { theme.text_3 },
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(theme.ui_px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if selected { theme.text } else { theme.text_2 })
                                    .child(mode.label()),
                            )
                            .child(
                                div()
                                    .mt(px(2.))
                                    .whitespace_normal()
                                    .text_size(theme.ui_px(11.))
                                    .text_color(theme.text_3)
                                    .child(mode.description()),
                            ),
                    )
                    .when(selected, |row| {
                        row.child(icon("icons/check.svg", 13., theme.accent))
                    }),
            );
        }

        let popup = div()
            .w(px(300.))
            .font_family(theme::ui_font_family())
            .rounded(px(12.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .key_context("WorkflowMenu")
            .track_focus(&self.workflow_menu_focus)
            .on_action(cx.listener(Self::on_workflow_menu_next))
            .on_action(cx.listener(Self::on_workflow_menu_prev))
            .on_action(cx.listener(Self::on_workflow_menu_confirm))
            .on_action(cx.listener(Self::on_workflow_menu_close))
            .on_mouse_down_out(cx.listener(|app, _, window, cx| {
                app.menu_dismissed_at = Some(Instant::now());
                app.close_workflow_menu(window, cx);
            }))
            .child(list);

        let popup: AnyElement = if theme::reduce_motion(cx) {
            popup.into_any_element()
        } else {
            popup
                .with_animation(
                    "workflow-menu-in",
                    Animation::new(Duration::from_millis(130))
                        .with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                    |el, d| el.opacity(d),
                )
                .into_any_element()
        };

        Some(
            anchored()
                .position_mode(AnchoredPositionMode::Local)
                .anchor(Corner::BottomLeft)
                .offset(point(px(0.), px(-4.)))
                .snap_to_window()
                .child(deferred(popup))
                .into_any_element(),
        )
    }

    /// While a run is in flight and the composer holds something to send, a
    /// quiet steer control sits beside Stop: it injects the text into the live
    /// turn (`steer`) rather than queuing a follow-up. Hidden when idle or
    /// empty, since there is nothing to redirect.
    pub(super) fn steer_button(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *theme::get(cx);
        if !self.is_running() {
            return None;
        }
        let has_text =
            !self.input.read(cx).text().trim().is_empty() || !self.attachments.is_empty();
        if !has_text {
            return None;
        }
        Some(
            div()
                .id("steer-btn")
                .group(BUTTON_GROUP)
                .size(px(28.))
                .rounded_full()
                .bg(theme.overlay)
                .hover(|s| s.bg(theme.overlay_strong))
                .active(|s| s.opacity(PRESS_DIM))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| this.steer_current(cx)),
                )
                .child(icon("icons/arrow-up-right.svg", 13., theme.accent))
                .into_any_element(),
        )
    }

    /// The composer's "+" button and its add menu (anchored above the
    /// button, same deferred pattern as the chip pickers). The menu carries
    /// the `AddMenu` key context, so ↑/↓/Enter/Escape drive it while open.
    pub(super) fn add_menu_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.add_menu_popup(cx))
            .child(
                div()
                    .id("attach-chip")
                    .group(BUTTON_GROUP)
                    .size(px(24.))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay))
                    .active(|s| s.opacity(PRESS_DIM))
                    .when(self.add_menu_open, |b| {
                        b.bg(theme.active).text_color(theme.active_fg)
                    })
                    .child(icon("icons/plus.svg", 13., theme.text_3))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_add_trigger_click)),
            )
    }

    /// The add menu popup, while open. Real actions only: attach an image,
    /// attach any file (by path at the caret), or start an @-mention.
    pub(super) fn add_menu_popup(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.add_menu_open {
            return None;
        }
        let theme = *theme::get(cx);
        let this = cx.weak_entity();
        let mut list = div().w_full().p(px(4.)).flex().flex_col().gap(px(2.));
        for (ix, (icon_path, label, hint)) in ADD_MENU_ITEMS.iter().enumerate() {
            let highlighted = ix == self.add_menu_highlight;
            let this = this.clone();
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("add-menu-row".into(), ix as u64))
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(highlighted, |row| row.bg(theme.active))
                    .hover(|style| style.bg(theme.overlay))
                    .on_hover(move |hovered, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.add_menu_highlight != ix {
                                    app.add_menu_highlight = ix;
                                    cx.notify();
                                }
                            })
                            .ok();
                        }
                    })
                    .on_mouse_up(MouseButton::Left, {
                        let this = cx.weak_entity();
                        move |_, window, cx| {
                            this.update(cx, |app, cx| app.run_add_menu_item(ix, window, cx))
                                .ok();
                        }
                    })
                    .child(icon(icon_path, 13., theme.text_3))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme.ui_px(12.))
                            .text_color(if highlighted {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(tr!(*label)),
                    )
                    .when(!hint.is_empty(), |row| {
                        row.child(
                            div()
                                .text_size(theme.ui_px(11.))
                                .text_color(theme.text_3)
                                .child(*hint),
                        )
                    }),
            );
        }

        let popup = div()
            .w(px(200.))
            .font_family(theme::ui_font_family())
            .rounded(px(8.))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            // The menu owns the keyboard while open (`AddMenu` bindings in
            // main.rs sit deeper than the global escape/enter).
            .key_context("AddMenu")
            .track_focus(&self.add_menu_focus)
            .on_action(cx.listener(Self::on_add_menu_next))
            .on_action(cx.listener(Self::on_add_menu_prev))
            .on_action(cx.listener(Self::on_add_menu_confirm))
            .on_action(cx.listener(Self::on_add_menu_close))
            .on_mouse_down_out(cx.listener(|app, _, window, cx| {
                // Arm the click-through guard so the same click's mouse-up
                // on the "+" button doesn't re-open the menu.
                app.menu_dismissed_at = Some(Instant::now());
                app.close_add_menu(window, cx);
            }))
            .child(list);

        Some(
            anchored()
                .position_mode(AnchoredPositionMode::Local)
                .anchor(Corner::BottomLeft)
                .offset(point(px(0.), px(-4.)))
                .snap_to_window()
                .child(deferred(popup))
                .into_any_element(),
        )
    }

    /// The anchored popup for `kind`, when that picker is open. `deferred`
    /// paints it on top of everything; `anchored` takes it out of the layout
    /// and pins its bottom-left corner just above the chip, flipping at the
    /// window edges via `snap_to_window`.
    pub(super) fn chip_popup(&self, kind: PickerKind) -> Option<impl IntoElement + use<>> {
        self.model_selector
            .clone()
            .and_then(|(open_kind, selector)| {
                (open_kind == kind).then(move || {
                    anchored()
                        .position_mode(AnchoredPositionMode::Local)
                        .anchor(Corner::BottomLeft)
                        .offset(point(px(0.), px(-4.)))
                        .snap_to_window()
                        .child(deferred(selector))
                })
            })
    }

    /// The model chip: provider glyph + model name + caret. Ghost style —
    /// configuration is secondary to the prompt, so chips carry no border
    /// or fill until hovered/open. `compact` clamps the label so narrow
    /// windows keep Send reachable.
    pub(super) fn model_chip(&self, compact: bool, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.chip_popup(PickerKind::Model))
            .child(
                div()
                    .id("model-chip")
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px(px(6.))
                    .h(px(22.))
                    .rounded(px(6.))
                    .text_size(theme.ui_px(11.5))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay))
                    .when(self.picker_is_open(PickerKind::Model), |chip| {
                        chip.bg(theme.active)
                    })
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_model_trigger_click))
                    .child(icon_dyn(
                        provider_icon(&self.model_provider),
                        11.,
                        theme.text_3,
                    ))
                    .child(
                        div()
                            .max_w(px(if compact { 120. } else { 220. }))
                            .truncate()
                            .text_color(if self.picker_is_open(PickerKind::Model) {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(self.model_label.clone()),
                    )
                    .child(Self::chip_caret(
                        self.picker_is_open(PickerKind::Model),
                        theme.active_fg,
                        "model-caret-turn",
                        cx,
                    )),
            )
    }

    /// The thinking-level chip: level icon + reasoning level + caret. Ghost
    /// style, same hierarchy as the model chip.
    pub(super) fn thinking_chip(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .flex()
            .flex_col()
            .items_start()
            .children(self.chip_popup(PickerKind::Thinking))
            .child(
                div()
                    .id("thinking-chip")
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px(px(6.))
                    .h(px(22.))
                    .rounded(px(6.))
                    .text_size(theme.ui_px(11.5))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay))
                    .when(self.picker_is_open(PickerKind::Thinking), |chip| {
                        chip.bg(theme.active)
                    })
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(Self::on_thinking_trigger_click),
                    )
                    .child({
                        let (path, _) = thinking_icon(&self.thinking_label, &theme);
                        icon(path, 13., theme.text_3)
                    })
                    .child(
                        div()
                            .text_color(if self.picker_is_open(PickerKind::Thinking) {
                                theme.active_fg
                            } else {
                                theme.text_2
                            })
                            .child(thinking_display(&self.thinking_label)),
                    )
                    .child(Self::chip_caret(
                        self.picker_is_open(PickerKind::Thinking),
                        theme.active_fg,
                        "thinking-caret-turn",
                        cx,
                    )),
            )
    }

    /// The new-task backdrop layer: the configured dithered image, or the
    /// default dot grid, absolutely filling the whole chat column below the
    /// top bar — behind the empty state, the floating composer, and the
    /// status bar alike, so the picture is continuous behind all three.
    ///
    /// It lives here rather than inside the empty state because the empty
    /// state is only the `flex_1` slot above the composer: a backdrop scoped
    /// to it ended at the composer's top edge and left the bottom of the
    /// window on a flat slab, at every fade setting including "None". The
    /// chat page deliberately has none.
    pub(super) fn new_task_backdrop(theme: Theme, empty: bool) -> Option<AnyElement> {
        if !empty {
            return None;
        }
        let backdrop = Self::dither_backdrop(theme);
        let dithered = backdrop.is_some();
        Some(
            div()
                .id("new-task-backdrop")
                .debug_selector(|| "new-task-backdrop".to_string())
                .absolute()
                .top(px(TOP_BAR_H))
                .left_0()
                .right_0()
                .bottom_0()
                .when(!dithered, |layer| layer.child(Self::dot_backdrop(theme)))
                .children(backdrop)
                .into_any_element(),
        )
    }

    /// The configured dithered background image, absolutely filling its
    /// parent — the new-task column, via [`Self::new_task_backdrop`].
    pub(super) fn dither_backdrop(theme: Theme) -> Option<AnyElement> {
        Self::backdrop_image(crate::dither::background(), crate::dither::tuning(), theme)
    }

    /// Wrap a dithered image so it fills its parent and nothing else, then
    /// drop it into the page: a bottom gradient to `bg_main` so the picture
    /// fades out under the composer instead of ending on a hard edge. The
    /// fade height comes from the backdrop's own tuning (Settings →
    /// Appearance); `None` skips the gradient entirely rather than painting
    /// a zero-height one.
    ///
    /// The wrapper clips: gpui's `ObjectFit::Cover` scales the image up and
    /// centers it, returning bounds *larger* than the element whenever the
    /// ratios differ — a 16:9 image in a narrower main area painted its
    /// overflow over the sessions sidebar until this clip was added.
    pub(super) fn backdrop_image(
        image: Option<std::sync::Arc<gpui::RenderImage>>,
        tuning: crate::dither::Tuning,
        theme: Theme,
    ) -> Option<AnyElement> {
        image.map(|image| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .right_0()
                .bottom_0()
                .overflow_hidden()
                .child(img(image).size_full().object_fit(ObjectFit::Cover))
                .when(tuning.fade > 0., |backdrop| {
                    backdrop.child(
                        div()
                            .id("backdrop-fade")
                            .debug_selector(|| "backdrop-fade".to_string())
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom_0()
                            // Proportional, so the drop stays put as the window
                            // resizes instead of turning into a band.
                            .h(relative(tuning.fade))
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(theme.bg_main.opacity(0.), 0.),
                                linear_color_stop(theme.bg_main.opacity(tuning.fade_peak()), 1.),
                            )),
                    )
                })
                .into_any_element()
        })
    }

    /// Fading dot-grid backdrop for the new-task page, used whenever no
    /// background image is configured. GPUI tints the SVG alpha mask with a
    /// theme color, so the art must be explicit circles (patterns/masks do
    /// not survive the renderer).
    pub(super) fn dot_backdrop(theme: Theme) -> impl IntoElement + use<> {
        let dot_color = match theme.mode {
            ThemeMode::Light => theme.text_3.opacity(0.75),
            ThemeMode::Dark => theme.text_3.opacity(0.65),
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .child(
                svg()
                    .path("backgrounds/new-task-dots.svg")
                    .size_full()
                    .text_color(dot_color),
            )
    }

    /// New-task empty state — minimal onboarding over the backdrop the caller
    /// paints (the configured dithered image, or the dot grid): one headline,
    /// a ghost workspace row, and the composer below for input.
    ///
    /// `main_width` comes from the caller (window minus sidebar/pane) because
    /// the field below gets a definite width: `w_full().max_w(_)` chains
    /// nested under the centered column resolve their percentages against the
    /// unclamped page width in this gpui/taffy stack, which painted the card
    /// to the window's right edge. `width_for_window` keeps the field and the
    /// picker popover the same width from one source of truth.
    pub(super) fn render_empty_state(
        &self,
        main_width: Pixels,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let field_w = px(crate::workspace_picker::width_for_window(main_width.into()));
        let cwd = self
            .current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let folder_name = sessions::workspace_label(&cwd);
        let path_label = cwd.to_string_lossy().into_owned();
        let picker_open = self.workspace_picker.is_some();

        // The backdrop belongs to the caller (`new_task_backdrop`, painted by
        // the chat column in `render`): one layer spans this state, the
        // floating composer, and the status bar. The chat page has none.
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .relative()
            .overflow_hidden()
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px(px(crate::workspace_picker::PAGE_PAD))
                    .pb(px(88.))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(crate::workspace_picker::FIELD_MAX_W))
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(20.))
                            // Mark over the dot grid: a quiet raised disc,
                            // not an accent wash. Ember is reserved for the
                            // open picker and the caret, not decoration.
                            .child(
                                div()
                                    .flex_none()
                                    .size(px(56.))
                                    .rounded_full()
                                    .bg(theme.bg_raised)
                                    .border_1()
                                    .border_color(theme.border)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon("icons/start-up.svg", 24., theme.text_2)),
                            )
                            // Title block — one idea, one line of guidance.
                            // 20px is the scale's display step (DESIGN.md).
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .text_size(theme.ui_px(20.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text)
                                            .child(tr!("view.start_a_task")),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.ui_px(13.))
                                            .text_color(theme.text_3)
                                            .text_align(TextAlign::Center)
                                            .child(tr!("workspace.pick_workspace_hint")),
                                    ),
                            )
                            // Workflow mode — the task's scope before the
                            // session exists: Plan / Build / Ask. Held pending
                            // and committed to the new session id.
                            .child(
                                div()
                                    .w(field_w)
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .px(px(2.))
                                            .text_size(theme.ui_px(10.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_3)
                                            .child(tr!("view.mode")),
                                    )
                                    .child(
                                        div().flex().gap(px(4.)).children(
                                            WorkflowMode::ALL.iter().map(|mode| {
                                                let mode = *mode;
                                                let selected = self.workflow_mode == mode;
                                                div()
                                                    .id(ElementId::Name(
                                                        format!("new-task-mode-{}", mode.as_wire())
                                                            .into(),
                                                    ))
                                                    .flex_1()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .gap(px(6.))
                                                    .py(px(7.))
                                                    .rounded(px(10.))
                                                    .border_1()
                                                    .border_color(if selected {
                                                        theme.accent.opacity(0.55)
                                                    } else {
                                                        theme.border
                                                    })
                                                    .bg(if selected {
                                                        theme.accent.opacity(0.12)
                                                    } else {
                                                        theme.bg_raised
                                                    })
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.border_color(theme.border_strong)
                                                            .bg(theme.overlay)
                                                    })
                                                    .on_mouse_up(
                                                        MouseButton::Left,
                                                        cx.listener(move |app, _, _, cx| {
                                                            app.choose_workflow_mode(mode, cx);
                                                        }),
                                                    )
                                                    .child(icon(
                                                        mode.icon(),
                                                        13.,
                                                        if selected {
                                                            theme.accent
                                                        } else {
                                                            theme.text_3
                                                        },
                                                    ))
                                                    .child(
                                                        div()
                                                            .text_size(theme.ui_px(12.))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(if selected {
                                                                theme.text
                                                            } else {
                                                                theme.text_2
                                                            })
                                                            .child(mode.label()),
                                                    )
                                            }),
                                        ),
                                    ),
                            )
                            // Workspace — a labeled select field, not a ghost
                            // row. Click opens the workspace picker (recent
                            // folders, filter, browse) anchored below; the
                            // border takes the accent while it's open.
                            //
                            // Definite `w` (not `w_full().max_w()`): percent
                            // widths nested under the centered `max_w` column
                            // resolve against the unclamped page width here,
                            // and the card painted to the window's edge.
                            .child(
                                div()
                                    .w(field_w)
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .child(
                                        div()
                                            .px(px(2.))
                                            .text_size(theme.ui_px(10.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_3)
                                            .child(tr!("view.workspace")),
                                    )
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .child(
                                                div()
                                                    .id("pick-folder")
                                                    .w_full()
                                                    .pl(px(8.))
                                                    .pr(px(10.))
                                                    .py(px(7.))
                                                    .rounded(px(12.))
                                                    .border_1()
                                                    .border_color(if picker_open {
                                                        theme.accent.opacity(0.55)
                                                    } else {
                                                        theme.border
                                                    })
                                                    .bg(theme.bg_raised)
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(10.))
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.border_color(theme.border_strong)
                                                            .bg(theme.overlay)
                                                    })
                                                    .on_mouse_up(
                                                        MouseButton::Left,
                                                        cx.listener(|app, _, window, cx| {
                                                            app.toggle_workspace_picker(window, cx);
                                                        }),
                                                    )
                                                    .child(
                                                        div()
                                                            .size(px(28.))
                                                            .flex_none()
                                                            .rounded(px(8.))
                                                            .bg(theme.overlay)
                                                            .flex()
                                                            .items_center()
                                                            .justify_center()
                                                            .child(icon(
                                                                "icons/folder.svg",
                                                                14.,
                                                                theme.text_2,
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .flex_1()
                                                            .min_w_0()
                                                            .flex()
                                                            .flex_col()
                                                            .gap(px(1.))
                                                            .child(
                                                                div()
                                                                    .text_size(theme.ui_px(13.))
                                                                    .font_weight(FontWeight::MEDIUM)
                                                                    .text_color(theme.text)
                                                                    .truncate()
                                                                    .child(folder_name),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(theme.ui_px(11.))
                                                                    .text_color(theme.text_3)
                                                                    .truncate()
                                                                    .child(path_label),
                                                            ),
                                                    )
                                                    // Open state: the affordance
                                                    // answers in accent, matching
                                                    // the border.
                                                    .child(icon(
                                                        "icons/chevron-down.svg",
                                                        12.,
                                                        if picker_open {
                                                            theme.accent
                                                        } else {
                                                            theme.text_3
                                                        },
                                                    )),
                                            )
                                            .children(self.workspace_picker_popup()),
                                    ),
                            ),
                    ),
            )
    }

    /// Whether every required runtime dependency is installed.
    pub(super) fn dependencies_ready(&self) -> bool {
        onboarding::all_required_installed(&self.deps)
    }

    /// Leave the setup page. Only reachable on request: the page also shows
    /// when something is missing, and then there is nothing to go back to.
    pub(super) fn close_setup(&mut self, cx: &mut Context<Self>) {
        self.setup_open = false;
        cx.notify();
    }

    /// Re-run the dependency probe and, if `pi` just became available, spawn
    /// the agent client. Runs the probe off the main thread and spins the
    /// setup page's Refresh button while it's in flight.
    pub(super) fn refresh_setup(&mut self, cx: &mut Context<Self>) {
        if self.refreshing {
            return; // ignore double-clicks while a refresh is running
        }
        self.refreshing = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let (deps, host) = cx
                .background_executor()
                .spawn(async {
                    // Short pause so the spinner reads as "working" rather than
                    // a flash before the probe returns.
                    std::thread::sleep(Duration::from_millis(250));
                    (onboarding::check_dependencies(), platform::host())
                })
                .await;
            let ready = onboarding::all_required_installed(&deps);
            let _ = this.update(cx, |app, cx| {
                app.deps = deps;
                app.host = host;
                app.refreshing = false;
                if ready && app.client.is_none() {
                    let workspace = app
                        .current_workspace
                        .clone()
                        .or_else(|| std::env::current_dir().ok())
                        .unwrap_or_else(|| PathBuf::from("."));
                    match app.extensions.spawn(&workspace) {
                        Ok(client) => {
                            app.client = Some(client);
                            app.send(CommandBody::GetState, "get_state");
                            app.refresh_catalogs();
                            app.toast_success(tr!("view.connected"));
                        }
                        Err(err) => app.toast_error(tr!("runtime.pi_spawn_failed", error = err)),
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Full-page setup screen shown when a required dependency is missing.
    pub(super) fn render_onboarding(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let missing = onboarding::missing_required_count(&self.deps);
        // Opened from Settings → About on a provisioned machine, the page is
        // a report rather than a checklist, so it says so.
        let asked_for = missing == 0;

        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .relative()
            .overflow_hidden()
            .child(Self::dot_backdrop(theme))
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .px(px(24.))
                    .pb(px(40.))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(560.))
                            .flex()
                            .flex_col()
                            .gap(px(18.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(10.))
                                    .child(
                                        div()
                                            .size(px(44.))
                                            .rounded_full()
                                            .bg(theme.accent.opacity(0.12))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(icon("icons/spark.svg", 18., theme.accent)),
                                    )
                                    .child(
                                        div()
                                            .text_size(theme.ui_px(22.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text)
                                            .child(tr!("view.set_up_orbit")),
                                    )
                                    .child(
                                        div()
                                            .max_w(px(420.))
                                            .text_size(theme.ui_px(13.))
                                            .text_color(theme.text_3)
                                            .text_align(TextAlign::Center)
                                            .child(if asked_for {
                                                tr!("view.setup_complete_hint")
                                            } else {
                                                tr!("view.setup_missing_hint")
                                            }),
                                    ),
                            )
                            .child(div().flex().flex_col().gap(px(10.)).children(
                                self.deps.iter().enumerate().map(|(ix, dep)| {
                                    self.render_dependency_row(dep, ix, cx).into_any_element()
                                }),
                            ))
                            .child(self.render_host_facts(cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(14.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .text_size(theme.ui_px(12.))
                                            .text_color(theme.text_3)
                                            .child(
                                                // Green once nothing is
                                                // missing: the page doubles
                                                // as a health report.
                                                div().size(px(8.)).rounded_full().bg(
                                                    if missing > 0 {
                                                        theme.stop_red
                                                    } else {
                                                        theme.ok_green
                                                    },
                                                ),
                                            )
                                            .child(if missing > 0 {
                                                tr!("setup.missing_count", count = missing)
                                            } else {
                                                tr!("setup.ready")
                                            }),
                                    )
                                    .child(
                                        div()
                                            .id("refresh-setup")
                                            .px(px(12.))
                                            .py(px(6.))
                                            .rounded_md()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.))
                                            .when(self.refreshing, |b| b.opacity(0.55))
                                            .when(!self.refreshing, |b| {
                                                b.cursor_pointer().hover(|s| s.bg(theme.bg_hover))
                                            })
                                            .on_click({
                                                let this = cx.entity();
                                                move |_, _window, cx| {
                                                    this.update(cx, |app, cx| {
                                                        app.refresh_setup(cx);
                                                    });
                                                }
                                            })
                                            .child(if self.refreshing {
                                                gpui::svg()
                                                    .path("icons/loader.svg")
                                                    .flex_none()
                                                    .size(px(13.))
                                                    .text_color(theme.text_2)
                                                    .with_animation(
                                                        "refresh-spin",
                                                        Animation::new(Duration::from_millis(800))
                                                            .repeat(),
                                                        |svg, delta| {
                                                            svg.with_transformation(
                                                                Transformation::rotate(radians(
                                                                    delta * std::f32::consts::TAU,
                                                                )),
                                                            )
                                                        },
                                                    )
                                                    .into_any_element()
                                            } else {
                                                icon("icons/refresh.svg", 13., theme.text_2)
                                                    .into_any_element()
                                            })
                                            .child(
                                                div()
                                                    .text_size(theme.ui_px(12.))
                                                    .text_color(theme.text_2)
                                                    .child(if self.refreshing {
                                                        tr!("common.checking")
                                                    } else {
                                                        tr!("common.refresh")
                                                    }),
                                            ),
                                    )
                                    // Only offered when the page was opened on
                                    // request — with something missing, the
                                    // setup page is the app.
                                    .when(asked_for, |footer| {
                                        footer.child(self.runtime_button(
                                            "setup-done",
                                            &tr!("common.done"),
                                            true,
                                            theme,
                                            cx.entity(),
                                            OrbitApp::close_setup,
                                        ))
                                    }),
                            ),
                    ),
            )
    }

    /// The machine and the paths Orbit reads and writes, under the
    /// dependency list: the OS it is running on, pi's session store, and
    /// Orbit's own config directory.
    ///
    /// Nothing here is installable, so these are quiet rows rather than
    /// dependency cards — but they are what the install commands above have
    /// to work against, and the first thing to check when the app comes up
    /// with an empty sidebar.
    pub(super) fn render_host_facts(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let facts = onboarding::host_facts(&self.host);
        div()
            .w_full()
            // A panel that hugs its rows: the setup page centres its column, so
            // a shrinkable child here would be clipped instead of grown.
            .flex_none()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .children(facts.iter().enumerate().map(|(ix, fact)| {
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(14.))
                    .py(px(8.))
                    // Rows are separated by a hairline, so the group reads as
                    // one panel rather than three.
                    .when(ix > 0, |row| row.border_t_1().border_color(theme.border))
                    .child(
                        div()
                            .w(px(88.))
                            .flex_none()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_3)
                            .child(fact.label.clone()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_2)
                            .child(fact.value.clone()),
                    )
                    .children(fact.note.clone().map(|note| {
                        div()
                            .flex_none()
                            .text_size(theme.ui_px(11.5))
                            .text_color(if fact.alert { theme.crit } else { theme.text_3 })
                            .child(note)
                    }))
            }))
    }

    /// One dependency row: status dot, name + detail, and either a version
    /// chip (installed) or the install command with a copy affordance.
    pub(super) fn render_dependency_row(
        &self,
        dep: &Dependency,
        ix: usize,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        let status_color = if dep.installed {
            theme.ok_green
        } else {
            theme.stop_red
        };

        div()
            .w_full()
            .px(px(14.))
            .py(px(12.))
            .rounded_lg()
            .bg(theme.bg_raised)
            .border_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(px(12.))
            .child(div().size(px(9.)).rounded_full().bg(status_color))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div()
                                    .text_size(theme.ui_px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(dep.name),
                            )
                            .child(
                                div()
                                    .text_size(theme.ui_px(11.))
                                    .text_color(theme.text_3)
                                    .child(if dep.required {
                                        tr!("setup.required")
                                    } else {
                                        tr!("setup.optional")
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_3)
                            .child(tr!(dep.detail_key)),
                    ),
            )
            .child(if dep.installed {
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_size(theme.ui_px(11.5))
                    .text_color(theme.ok_green)
                    .child(icon("icons/check.svg", 12., theme.ok_green))
                    .child(
                        dep.version
                            .clone()
                            .unwrap_or_else(|| tr!("setup.installed")),
                    )
                    .into_any_element()
            } else {
                let cmd = dep.install_hint;
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .px(px(8.))
                            .py(px(4.))
                            .rounded_md()
                            .bg(theme.code_bg)
                            .border_1()
                            .border_color(theme.border)
                            .text_size(theme.ui_px(11.))
                            .text_color(theme.code_text)
                            .child(cmd),
                    )
                    .child(
                        div()
                            .id(("copy", ix))
                            .p_1()
                            .rounded_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.bg_hover))
                            .on_click(move |_, _window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(cmd.to_string()));
                            })
                            .child(icon("icons/copy.svg", 13., theme.text_2)),
                    )
                    .into_any_element()
            })
    }

    /// Status bar under the composer: workspace / branch on the left,
    /// used-context percent + ring on the right.
    pub(super) fn status_bar(
        &self,
        workspace_label: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        div()
            .pt_1p5()
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .text_size(theme.ui_px(11.5))
            .text_color(theme.text_3)
            .child(
                div()
                    .id("status-workspace")
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .px(px(4.))
                    .py(px(2.))
                    .rounded_md()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.overlay).text_color(theme.text_2))
                    .active(|s| s.bg(theme.active).text_color(theme.active_fg))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_pick_folder_click))
                    .child(icon("icons/folder.svg", 12., theme.text_3))
                    .child(workspace_label.to_string()),
            )
            .children(self.branch.as_ref().map(|branch| {
                let open = self.branch_picker.is_some();
                let pending = self.branch_operation_pending;
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .relative()
                            .child(
                                div()
                                    .id("status-branch")
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .px(px(4.))
                                    .py(px(2.))
                                    .rounded_md()
                                    .when(!pending, |chip| chip.cursor_pointer())
                                    .when(open, |chip| {
                                        chip.bg(theme.active).text_color(theme.active_fg)
                                    })
                                    .when(!open && !pending, |chip| {
                                        chip.hover(|s| s.bg(theme.overlay).text_color(theme.text_2))
                                    })
                                    .when(pending, |chip| chip.opacity(0.6))
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(|app, _, window, cx| {
                                            if !app.branch_operation_pending {
                                                app.toggle_branch_picker(window, cx);
                                            }
                                        }),
                                    )
                                    .child(icon("icons/branch.svg", 12., theme.text_3))
                                    .child(branch.name.clone())
                                    .children(branch.ahead_behind.map(|(ahead, behind)| {
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_1p5()
                                            .child(format!("↑{ahead}"))
                                            .child(format!("↓{behind}"))
                                    })),
                            )
                            .children(self.branch_picker_popup()),
                    )
                    .children(branch.other_branches.map(|count| {
                        div()
                            .id("status-branch-count")
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .px(px(4.))
                            .py(px(2.))
                            .rounded_md()
                            .when(!pending, |chip| chip.cursor_pointer())
                            .when(open, |chip| {
                                chip.bg(theme.active).text_color(theme.active_fg)
                            })
                            .when(!open && !pending, |chip| {
                                chip.hover(|s| s.bg(theme.overlay).text_color(theme.text_2))
                            })
                            .when(pending, |chip| chip.opacity(0.6))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|app, _, window, cx| {
                                    if !app.branch_operation_pending {
                                        app.toggle_branch_picker(window, cx);
                                    }
                                }),
                            )
                            .child(icon("icons/git-fork.svg", 12., theme.text_3))
                            .child(tr!("view.count_more", count = count))
                    }))
                    .into_any_element()
            }))
            .child(div().flex_1())
            // Transient status (send failures, attachment limits, branch
            // results): fresh messages only — the tick lets them lapse.
            .children(
                self.status_at
                    .is_some_and(|at| at.elapsed() < STATUS_MESSAGE_TTL)
                    .then(|| {
                        div()
                            .max_w(px(320.))
                            .truncate()
                            .text_color(theme.text_3)
                            .child(self.status.clone())
                    }),
            )
            .child(self.context_button(cx))
    }

    pub(super) fn context_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let entity = cx.entity();
        let theme = *theme::get(cx);
        // Manual compaction is offered only while pi is connected: without a
        // session process there is nothing to compact.
        let on_compact = self.client.is_some().then(|| {
            Rc::new(
                |app: &mut OrbitApp, _window: &mut Window, cx: &mut Context<OrbitApp>| {
                    app.compact_now(cx);
                },
            ) as Rc<dyn Fn(&mut OrbitApp, &mut Window, &mut Context<OrbitApp>)>
        });
        context_meter::context_control(
            ContextMeterData {
                usage: self.context.as_ref(),
                session: self.session_usage.as_ref(),
                conversation_est: self.transcript.estimated_tokens(),
            },
            self.context_popup,
            &entity,
            theme,
            |app, hovered, cx| {
                if app.context_popup == ContextPopup::Details {
                    return;
                }
                app.context_popup = if hovered {
                    ContextPopup::Hover
                } else {
                    ContextPopup::None
                };
                cx.notify();
            },
            |app, _, cx| {
                const GESTURE: Duration = Duration::from_millis(200);
                if let Some(dismissed) = app.menu_dismissed_at.take() {
                    if dismissed.elapsed() < GESTURE {
                        return;
                    }
                }
                app.context_popup = if app.context_popup == ContextPopup::Details {
                    ContextPopup::None
                } else {
                    ContextPopup::Details
                };
                if app.context_popup == ContextPopup::Details {
                    app.refresh_context_stats();
                }
                cx.notify();
            },
            self.is_compacting,
            on_compact,
            |app, _, cx| {
                app.menu_dismissed_at = Some(Instant::now());
                app.context_popup = ContextPopup::None;
                cx.notify();
            },
        )
    }

    /// The window's left titlebar controls: sidebar toggle + session history.
    /// They sit beside the macOS traffic lights in the sidebar's drag strip
    /// when the sessions sidebar is open, and fall back to the main top bar
    /// (clearing the lights) when it is hidden. Windows has no OS buttons on
    /// that side, so the cluster stays left-aligned there either way.
    pub(super) fn titlebar_left_controls(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let back_enabled = self.history_index > 0;
        let forward_enabled = self.history_index + 1 < self.session_history.len();
        // The three controls wear the same glass chips as the right cluster
        // ([`header_icon_button`]), so the whole 44px bar reads as one row:
        // a fixed square hit box with a centered icon keeps the 16px toggle
        // and the 15px chevrons optically even (a bare `p_1` lets the toggle's
        // pill drift wider than the chevrons).
        div()
            .flex()
            .items_center()
            .gap(px(TITLEBAR_CONTROLS_GAP))
            .pr(px(6.))
            .child(
                header_ghost_button(
                    "toggle-sidebar",
                    &theme,
                    icon("icons/layout-left.svg", 16., theme.text_2),
                )
                .block_mouse_except_scroll()
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_toggle_sidebar)),
            )
            .child(
                div()
                    .id("history-back")
                    .block_mouse_except_scroll()
                    .size(px(HEADER_CTRL_H))
                    .rounded(px(HEADER_CTRL_R))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(back_enabled, |b| {
                        b.cursor_pointer()
                            .hover(|s| s.bg(theme.bg_hover))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_history_back))
                    })
                    .child(icon(
                        "icons/arrow-left.svg",
                        15.,
                        if back_enabled {
                            theme.text_2
                        } else {
                            theme.text_3
                        },
                    )),
            )
            .child(
                div()
                    .id("history-forward")
                    .block_mouse_except_scroll()
                    .size(px(HEADER_CTRL_H))
                    .rounded(px(HEADER_CTRL_R))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(forward_enabled, |b| {
                        b.cursor_pointer()
                            .hover(|s| s.bg(theme.bg_hover))
                            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_history_forward))
                    })
                    .child(icon(
                        "icons/arrow-right.svg",
                        15.,
                        if forward_enabled {
                            theme.text_2
                        } else {
                            theme.text_3
                        },
                    )),
            )
    }

    /// Sidebar nav row — fixed height,
    /// icon in a 20px slot, secondary label, rounded hover surface.
    /// The sidebar's primary action: a raised New Task button with the
    /// ⌘N shortcut hint — the one emphasized control in the nav column.
    pub(super) fn sidebar_new_task_button(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("sidebar-new-session")
            .group(BUTTON_GROUP)
            .w_full()
            .h(px(34.))
            .px(px(10.))
            .rounded_md()
            .bg(theme.bg_raised)
            .border_1()
            .border_color(theme.border)
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover).border_color(theme.border_strong))
            .active(|s| s.opacity(PRESS_DIM))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, w, cx| {
                    this.on_new_session(&crate::NewSession, w, cx)
                }),
            )
            .child(icon("icons/compose.svg", 13., theme.accent))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(theme.ui_px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .child(tr!("view.new_task")),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.text_3)
                    .child(crate::platform::shortcuts::NEW_SESSION),
            )
    }

    /// The quiet nav row under the primary button: opens the command
    /// palette. Ghost style — hover is the only affordance; the ⌘P hint
    /// mirrors the ⌘N hint on the button above.
    pub(super) fn sidebar_search_row(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        div()
            .id("sidebar-search")
            .w_full()
            .h(px(28.))
            .px(px(10.))
            .rounded_md()
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, w, cx| this.toggle_command_palette(w, cx)),
            )
            .child(icon("icons/search.svg", 13., theme.text_3))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(theme.ui_px(12.5))
                    .text_color(theme.text_3)
                    .child(tr!("view.search")),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.text_3)
                    .child(crate::platform::shortcuts::PALETTE),
            )
    }

    /// ⌘U: open the Usage page, or leave it if it is already open.
    pub(super) fn on_toggle_usage(
        &mut self,
        _: &crate::ToggleUsage,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.usage_open {
            self.close_usage(cx);
        } else {
            self.open_usage(cx);
        }
    }

    /// Sidebar nav row for the Usage page — a destination, not an action, so
    /// it carries the same shape as Settings and marks the active state with
    /// an `active` fill rather than accent color alone.
    pub(super) fn sidebar_usage_row(
        &self,
        theme: Theme,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let active = self.usage_open;
        div()
            .id("sidebar-usage")
            .w_full()
            .h(px(28.))
            .px(px(10.))
            .rounded_md()
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .when(active, |row| row.bg(theme.active))
            .hover(|s| s.bg(if active { theme.active } else { theme.bg_hover }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_usage_nav_click))
            .child(icon(
                "icons/usage-total.svg",
                13.,
                if active {
                    theme.active_fg
                } else {
                    theme.text_3
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(theme.ui_px(12.5))
                    .text_color(if active {
                        theme.active_fg
                    } else {
                        theme.text_2
                    })
                    .child(tr!("view.usage")),
            )
    }

    pub(super) fn send_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = *theme::get(cx);
        if self.busy {
            div()
                .id("stop-btn")
                .size(px(28.))
                .rounded_full()
                .bg(theme.stop_red)
                .hover(|s| s.bg(theme.stop_red_hover))
                .active(|s| s.bg(theme.stop_red))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.send_fg)
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_abort_mouse))
                .child(icon("icons/stop.svg", 12., theme.send_fg))
        } else {
            // Nothing to send yet: the button stays clickable (submit
            // no-ops on empty) but reads as quiet until there's a message
            // or an attachment.
            let empty = self.input.read(cx).text().trim().is_empty() && self.attachments.is_empty();
            div()
                .id("send-btn")
                .size(px(28.))
                .rounded_full()
                .bg(if empty { theme.overlay } else { theme.send_bg })
                .when(!empty, |btn| {
                    btn.hover(|s| s.bg(theme.send_bg_hover))
                        .active(|s| s.bg(theme.send_bg))
                        .cursor_pointer()
                })
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_send_click))
                .child(icon(
                    "icons/send.svg",
                    14.,
                    if empty { theme.text_3 } else { theme.send_fg },
                ))
        }
    }

    /// Persistent run-status strip above the composer: a calm, single-line
    /// account of what pi is doing between turns — waiting out a transient
    /// provider error (with the attempt counter and a real Cancel that sends
    /// `abort_retry`) or compacting the conversation context. Unlike the
    /// status bar's transient messages, this stays up for the whole state.
    /// The plan-progress panel above the composer (Plan and Build only).
    ///
    /// Collapsed it is one comfortable row — the current step, a meter, and the
    /// count; expanded it reveals the checklist. Sized like the app's other
    /// panels (14px gutters, 12–13px type) so it reads as a real section rather
    /// than a cramped status line. Hidden in Ask mode and when there is no plan.
    pub(super) fn workflow_progress(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *theme::get(cx);
        if !self.workflow_mode.tracks_todos() || self.workflow_todos.is_empty() {
            return None;
        }
        let (done, total) = self.workflow_todos.progress();
        let fraction = if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32).clamp(0.0, 1.0)
        };
        let all_done = self.workflow_todos.all_done();
        let expanded = self.workflow_todos_expanded;
        let current_step = self.workflow_todos.current().map(|todo| todo.step);
        let fill = if all_done { theme.ok_green } else { theme.accent };

        // A fixed-width meter reads as a status glyph beside the count; the
        // visible track keeps 0-of-N legible.
        let meter = div()
            .flex_none()
            .w(px(72.))
            .h(px(6.))
            .rounded_full()
            .overflow_hidden()
            .bg(theme.overlay_strong)
            .child(div().h_full().w(relative(fraction)).rounded_full().bg(fill));

        let next_step = if all_done {
            tr!("workflow.todos_done")
        } else {
            self.workflow_todos
                .current()
                .map(|todo| todo.text.clone())
                .unwrap_or_default()
        };

        // The whole row is one click target. Identity (icon + current step) on
        // the left, progress (meter + count + chevron) on the right.
        let header = div()
            .id("workflow-progress-toggle")
            .w_full()
            .h(px(30.))
            .group(BUTTON_GROUP)
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(6.))
            .rounded_md()
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.toggle_workflow_todos(cx)),
            )
            .child(
                icon(
                    if all_done {
                        "icons/circle-check.svg"
                    } else {
                        self.workflow_mode.icon()
                    },
                    14.,
                    if all_done { theme.ok_green } else { theme.text_2 },
                )
                .flex_none(),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(theme.ui_px(12.5))
                    .text_color(if all_done { theme.ok_green } else { theme.text_2 })
                    .child(next_step),
            )
            .child(meter)
            .child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(if all_done { theme.ok_green } else { theme.text_3 })
                    .child(tr!("workflow.todos_progress", done = done, total = total)),
            )
            .child(
                icon(
                    if expanded {
                        "icons/chevron-down.svg"
                    } else {
                        "icons/chevron-right.svg"
                    },
                    14.,
                    theme.text_3,
                )
                .flex_none(),
            );

        let mut panel = div()
            .id("workflow-progress")
            .w_full()
            .mb(px(8.))
            .px(px(14.))
            .py(px(6.))
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .child(header);

        if expanded {
            let mut list = div()
                .id("workflow-todos-list")
                .mt(px(8.))
                .pt(px(10.))
                .border_t_1()
                .border_color(theme.border)
                .flex()
                .flex_col()
                .gap(px(2.))
                .max_h(px(260.))
                .overflow_y_scroll();
            for todo in self.workflow_todos.todos() {
                // Hierarchy: the current step leads, pending steps sit a shade
                // back, completed ones recede behind a strike. Two-line clamp
                // keeps long plan steps on a regular rhythm.
                let is_current = !todo.done && Some(todo.step) == current_step;
                let text_color = if todo.done {
                    theme.text_3
                } else if is_current {
                    theme.text
                } else {
                    theme.text_2
                };
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .px(px(6.))
                        .py(px(4.))
                        .rounded_md()
                        .when(is_current, |row| row.bg(theme.accent.opacity(0.08)))
                        .child(
                            icon(
                                if todo.done {
                                    "icons/circle-check.svg"
                                } else {
                                    "icons/circle-dot.svg"
                                },
                                14.,
                                if todo.done { theme.ok_green } else { theme.text_3 },
                            )
                            .flex_none(),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .line_clamp(2)
                                .text_size(theme.ui_px(12.5))
                                .text_color(text_color)
                                .when(todo.done, |line| line.line_through())
                                .child(todo.text.clone()),
                        ),
                );
            }
            panel = panel.child(list);
        }

        Some(panel.into_any_element())
    }

    pub(super) fn run_status_strip(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = *theme::get(cx);
        if let Some(retry) = &self.retry_detail {
            let attempt = match retry.max {
                Some(max) => tr!("runtime.attempt_of", attempt = retry.attempt, max = max),
                None => tr!("runtime.attempt", attempt = retry.attempt),
            };
            return Some(
                div()
                    .w_full()
                    .mb(px(8.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.bg_raised)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().size(px(6.)).flex_none().rounded_full().bg(theme.warn))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_2)
                            .child(tr!(
                                "runtime.retrying",
                                attempt = attempt,
                                error = retry.error
                            )),
                    )
                    .child(
                        div()
                            .id("cancel-retry")
                            .flex_none()
                            .px(px(6.))
                            .py(px(1.))
                            .rounded(px(5.))
                            .text_size(theme.ui_px(11.))
                            .text_color(theme.text_2)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.abort_retry(cx);
                                }),
                            )
                            .child(tr!("view.cancel")),
                    )
                    .into_any_element(),
            );
        }
        if self.is_compacting {
            return Some(
                div()
                    .w_full()
                    .mb(px(8.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.bg_raised)
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(theme.accent),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_2)
                            .child(tr!("view.preparing_conversation_context")),
                    )
                    .into_any_element(),
            );
        }
        None
    }

    /// The inline `ask_user_question` panel: the live question and its options
    /// docked above the composer. It answers the extension's `select` /
    /// `input` requests in place, so a questionnaire never covers the
    /// transcript with a scrim. Every question is buffered locally — Back and
    /// Next step through them so an earlier answer can be changed — and only
    /// the final Submit replays them to the extension. The panel owns the
    /// `AskPanel` key context (↑/↓ move, ⏎ advances, esc declines); its text
    /// field carries `AskInput` so Enter advances rather than submitting the
    /// composer.
    pub(super) fn ask_panel(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let prompt = self.ask.as_ref()?;
        let question = prompt.question()?;
        let theme = *theme::get(cx);
        let submitted = prompt.submitted;
        let cursor = prompt.cursor;
        let total = prompt.total();
        let multi = prompt.is_multi();
        let highlighted = prompt.highlighted();

        let mut card = div()
            .id("ask-panel")
            .w_full()
            .mb(px(8.))
            .rounded(px(12.))
            .border_1()
            .border_color(theme.accent.opacity(0.3))
            .bg(theme.bg_raised)
            .flex()
            .flex_col()
            .overflow_hidden()
            .key_context("AskPanel")
            .track_focus(&self.ask_focus)
            .on_action(cx.listener(Self::on_ask_next))
            .on_action(cx.listener(Self::on_ask_prev))
            .on_action(cx.listener(Self::on_ask_confirm))
            .on_action(cx.listener(Self::on_ask_submit))
            .on_action(cx.listener(Self::on_ask_close));

        // ── header: icon, header chip, progress ──
        let mut header = div()
            .px(px(14.))
            .pt(px(12.))
            .flex()
            .items_center()
            .gap(px(8.));
        header = header.child(
            div()
                .flex_none()
                .size(px(24.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.accent.opacity(0.16))
                .child(icon("icons/task.svg", 13., theme.accent)),
        );
        if !question.header.trim().is_empty() {
            header = header.child(
                div()
                    .flex_none()
                    .px(px(7.))
                    .py(px(2.))
                    .rounded(px(5.))
                    .bg(theme.overlay_strong)
                    .text_size(theme.ui_px(10.5))
                    .line_height(theme.ui_px(14.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_2)
                    .child(SharedString::from(question.header.clone())),
            );
        }
        header = header.child(div().flex_1());
        if total > 1 {
            header = header.child(
                div()
                    .flex_none()
                    .text_size(theme.ui_px(11.))
                    .text_color(theme.text_3)
                    .child(tr!("ask.question_of", current = cursor + 1, total = total)),
            );
        }
        // A visible dismiss affordance beside the keyboard hint.
        header = header.child(
            div()
                .id("ask-close")
                .flex_none()
                .size(px(20.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|style| style.bg(theme.overlay_strong))
                .child(icon("icons/x.svg", 11., theme.text_3))
                .on_click(cx.listener(|this, _, window, cx| this.ask_cancel(window, cx))),
        );
        card = card.child(header);

        // ── the question ──
        card = card.child(
            div()
                .px(px(14.))
                .pt(px(8.))
                .pb(px(10.))
                .whitespace_normal()
                .text_size(theme.ui_px(13.5))
                .line_height(theme.ui_px(19.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(SharedString::from(question.question.clone())),
        );

        // ── body ──
        let mut rows = div().px(px(8.)).pb(px(8.)).flex().flex_col().gap(px(2.));
        for (ix, option) in question.options.iter().enumerate() {
            let row_highlighted = !submitted && ix == highlighted;
            let checked = multi
                && prompt
                    .checked
                    .get(cursor)
                    .and_then(|row| row.get(ix))
                    .copied()
                    .unwrap_or(false);
            let chosen = row_highlighted || checked;
            // A number reads as a question rather than a menu; multi
            // additionally marks its toggles with a trailing check.
            let marker = div()
                .flex_none()
                .w(px(16.))
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(if chosen { theme.accent } else { theme.text_3 })
                .child(format!("{}. ", ix + 1));
            let mut copy = div().min_w_0().flex_1().flex().flex_col().gap(px(1.));
            copy = copy.child(
                div()
                    .whitespace_normal()
                    .text_size(theme.ui_px(12.5))
                    .line_height(theme.ui_px(17.))
                    .text_color(if chosen { theme.text } else { theme.text_2 })
                    .child(SharedString::from(option.label.clone())),
            );
            // The description only under the focused row keeps the list
            // compact so every choice stays visible at a glance.
            if row_highlighted && !option.description.trim().is_empty() {
                copy = copy.child(
                    div()
                        .whitespace_normal()
                        .text_size(theme.ui_px(11.5))
                        .line_height(theme.ui_px(16.))
                        .text_color(theme.text_3)
                        .child(SharedString::from(option.description.clone())),
                );
            }
            let mut row = div()
                .id(ElementId::NamedInteger("ask-option".into(), ix as u64))
                .w_full()
                .min_w_0()
                .px(px(10.))
                .py(px(6.))
                .rounded(px(8.))
                .border_1()
                .border_color(if row_highlighted {
                    theme.border_strong
                } else {
                    gpui::transparent_black()
                })
                .when(row_highlighted, |row| row.bg(theme.overlay_strong))
                .when(!submitted, |row| row.cursor_pointer())
                .flex()
                .items_center()
                .gap(px(4.))
                .hover(|style| style.bg(theme.overlay_strong))
                .child(marker)
                .child(copy);
            if multi && checked {
                row = row.child(icon("icons/check.svg", 12., theme.accent));
            }
            if !submitted {
                row = row.on_click(cx.listener(move |this, _, _, cx| this.ask_choose(ix, cx)));
            }
            rows = rows.child(row);
        }
        // Single-select questions carry a trailing "Type something." row.
        if !multi {
            let trailing = question.options.len();
            let row_highlighted = !submitted && trailing == highlighted;
            let mut row = div()
                .id("ask-trailing")
                .w_full()
                .min_w_0()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .border_1()
                .border_color(if row_highlighted {
                    theme.border_strong
                } else {
                    gpui::transparent_black()
                })
                .when(row_highlighted, |row| row.bg(theme.overlay_strong))
                .when(!submitted, |row| row.cursor_pointer())
                .flex()
                .items_center()
                .gap(px(8.))
                .hover(|style| style.bg(theme.overlay_strong))
                .child(icon("icons/compose.svg", 12., theme.text_3))
                .child(
                    div()
                        .text_size(theme.ui_px(12.5))
                        .line_height(theme.ui_px(17.))
                        .text_color(if row_highlighted {
                            theme.text
                        } else {
                            theme.text_2
                        })
                        .child(tr!("view.type_something")),
                );
            if !submitted {
                row =
                    row.on_click(cx.listener(move |this, _, _, cx| this.ask_choose(trailing, cx)));
            }
            rows = rows.child(row);
        }
        card = card.child(rows);

        // The focused option's preview (single-select + preview only).
        // Labeled so authored mockups read as preview content, not a
        // second interface.
        if !multi {
            if let Some(option) = question.options.get(highlighted) {
                if let Some(preview) = option.preview.as_deref() {
                    let preview = if preview.chars().count() > 600 {
                        let mut capped: String = preview.chars().take(600).collect();
                        capped.push('…');
                        capped
                    } else {
                        preview.to_string()
                    };
                    card = card.child(
                        div()
                            .mx(px(12.))
                            .mb(px(10.))
                            .rounded(px(9.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.bg_main)
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px(px(10.))
                                    .py(px(5.))
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .child(icon("icons/eye.svg", 11., theme.text_3))
                                    .child(
                                        div()
                                            .text_size(theme.ui_px(10.5))
                                            .line_height(theme.ui_px(14.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(theme.text_3)
                                            .child(tr!("ask.preview", label = option.label)),
                                    ),
                            )
                            .child(
                                div()
                                    .px(px(10.))
                                    .py(px(8.))
                                    .font_family(theme::code_font_family())
                                    .text_size(theme.code_px(11.))
                                    .line_height(theme.code_px(16.))
                                    .text_color(theme.text_2)
                                    .whitespace_normal()
                                    .child(SharedString::from(preview)),
                            ),
                    );
                }
            }
        }

        // The custom-answer field: always on multi questions, and on a
        // single-select question once its "Type something." row is active.
        if prompt.custom_active() {
            card = card.child(
                div()
                    .mx(px(12.))
                    .mb(px(10.))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.bg_composer)
                    .px(px(10.))
                    .py(px(6.))
                    .child(prompt.input.clone()),
            );
        }

        // ── footer: hint + Back / Next (Submit on the last question) ──
        let hint = if submitted {
            tr!("ask.sending")
        } else if multi {
            tr!("ask.hint_multi")
        } else {
            tr!("ask.hint_single")
        };
        let mut back = div()
            .id("ask-back")
            .h(px(28.))
            .px(px(12.))
            .rounded(px(7.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(theme.ui_px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .border_1()
            .border_color(gpui::transparent_black())
            .bg(theme.overlay)
            .text_color(theme.text_2);
        let back_disabled = submitted || cursor == 0;
        if back_disabled {
            back = back.opacity(0.45);
        } else {
            back = back
                .cursor_pointer()
                .hover(|style| style.bg(theme.overlay_strong).text_color(theme.text))
                .on_click(cx.listener(|this, _, window, cx| this.ask_prev_question(window, cx)));
        }
        back = back.child(tr!("view.back"));

        let last = cursor + 1 >= total;
        let next_label = if last {
            tr!("ask.submit")
        } else {
            tr!("ask.next")
        };
        let mut next = div()
            .id("ask-next")
            .h(px(28.))
            .px(px(14.))
            .rounded(px(7.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(theme.ui_px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .border_1()
            .border_color(gpui::transparent_black())
            .bg(theme.accent.opacity(0.16))
            .text_color(theme.accent);
        if submitted {
            next = next.opacity(0.45);
        } else {
            next = next
                .cursor_pointer()
                .hover(|style| style.bg(theme.accent.opacity(0.26)))
                .on_click(cx.listener(|this, _, window, cx| this.ask_next_question(window, cx)));
        }
        next = next.child(next_label);

        card = card.child(
            div()
                .h(px(46.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .border_t_1()
                .border_color(theme.border)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(theme.ui_px(11.))
                        .text_color(theme.text_3)
                        .child(hint),
                )
                .child(back)
                .child(next),
        );

        Some(card.into_any_element())
    }

    /// The inline access-guard approval: a compact bar directly above the
    /// composer asking whether a mutating tool call may run. Rendered here
    /// rather than as the scrim modal so the transcript stays visible and the
    /// answer sits next to the composer. Each option is a button; the bar also
    /// owns the `Approval` key context (↑/↓ move, ⏎ confirms, esc denies). It
    /// reveals by animating its own height so the page never jumps.
    pub(super) fn approval_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        /// Fixed row height — a single-line prompt, so the reveal can animate
        /// a known height and the surrounding layout can never reflow abruptly.
        const BAR_H: f32 = 48.;
        const GAP: f32 = 8.;

        let request = self.approval.as_ref()?;
        let theme = *theme::get(cx);
        let heading = if request.tool.trim().is_empty() {
            tr!("approval.permission_needed")
        } else {
            tr!("approval.allow_tool", tool = request.tool)
        };
        let detail = request.detail.clone();

        let mut buttons = div().flex().items_center().gap(px(6.));
        for (ix, option) in request.options.iter().enumerate() {
            let highlighted = ix == self.approval_highlight;
            let is_deny = option.eq_ignore_ascii_case("deny");
            let is_primary = !is_deny && !option.to_ascii_lowercase().contains("always");
            let mut button = div()
                .id(ElementId::NamedInteger("approval-option".into(), ix as u64))
                .h(px(28.))
                .px(px(12.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme.ui_px(11.5))
                .font_weight(FontWeight::MEDIUM)
                .cursor_pointer()
                .border_1()
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| this.approval_choose(ix, window, cx)),
                );
            // Ember stays an accent, never a fill: the affirmative is an
            // ember-washed tile with ember ink, matching the One Accent Rule.
            button = if is_primary {
                button
                    .bg(theme.accent.opacity(0.16))
                    .text_color(theme.accent)
                    .hover(|s| s.bg(theme.accent.opacity(0.26)))
            } else if is_deny {
                button
                    .bg(theme.overlay)
                    .text_color(theme.text_2)
                    .hover(|s| s.bg(theme.crit.opacity(0.14)).text_color(theme.crit))
            } else {
                button
                    .bg(theme.overlay)
                    .text_color(theme.text_2)
                    .hover(|s| s.bg(theme.overlay_strong).text_color(theme.text))
            };
            // The keyboard cursor reads as a strong border.
            button = button.border_color(if highlighted {
                theme.border_strong
            } else {
                gpui::transparent_black()
            });
            buttons = buttons.child(button.child(option.clone()));
        }

        let bar = div()
            .id("approval-bar")
            .w_full()
            .h(px(BAR_H))
            .mb(px(GAP))
            .px(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(theme.accent.opacity(0.3))
            .bg(theme.bg_raised)
            .flex()
            .items_center()
            .gap(px(10.))
            .overflow_hidden()
            .key_context("Approval")
            .track_focus(&self.approval_focus)
            .on_action(cx.listener(Self::on_approval_next))
            .on_action(cx.listener(Self::on_approval_prev))
            .on_action(cx.listener(Self::on_approval_confirm))
            .on_action(cx.listener(Self::on_approval_close))
            .child(
                div()
                    .flex_none()
                    .size(px(28.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(theme.accent.opacity(0.16))
                    .child(icon("icons/lock.svg", 14., theme.accent)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(theme.ui_px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(heading),
                    )
                    .when(!detail.trim().is_empty(), |col| {
                        col.child(
                            div()
                                .mt(px(1.))
                                .font_family(theme::code_font_family())
                                .truncate()
                                .text_size(theme.ui_px(11.))
                                .text_color(theme.text_3)
                                .child(detail),
                        )
                    }),
            )
            .child(buttons);

        // Reveal by easing the bar's own height (and its gap) from zero, so the
        // prompt unfolds in place instead of the page snapping down a row.
        let bar: AnyElement = if theme::reduce_motion(cx) {
            bar.into_any_element()
        } else {
            bar.with_animation(
                "approval-in",
                Animation::new(Duration::from_millis(170)).with_easing(|d| 1.0 - (1.0 - d).powi(3)),
                move |el, d| el.max_h(px(BAR_H * d)).mb(px(GAP * d)).opacity(d),
            )
            .into_any_element()
        };
        Some(bar)
    }

    /// The pending queue pi is holding, shown as a bar directly above the
    /// composer. While a task is running, messages sent from the composer are
    /// queued as follow-ups here and delivered once the task finishes;
    /// `queue_update` mirrors the list live.
    pub(super) fn queue_bar(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.queue.is_empty() {
            return None;
        }
        let theme = *theme::get(cx);
        let mut chips = div().flex().flex_wrap().gap(px(6.));
        for text in &self.queue.steering {
            chips = chips.child(queue_chip("Steer", text, false, theme));
        }
        for text in &self.queue.follow_up {
            chips = chips.child(queue_chip("Follow-up", text, true, theme));
        }
        Some(
            div()
                .w_full()
                .mb(px(8.))
                .px(px(10.))
                .py(px(8.))
                .rounded_lg()
                .border_1()
                .border_color(theme.border)
                .bg(theme.bg_raised)
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(theme.ui_px(11.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_3)
                                .child(if self.queue.follow_up.is_empty() {
                                    tr!("view.steering_turn", count = self.queue.len())
                                } else if self.queue.steering.is_empty() {
                                    tr!("view.queued_after_task", count = self.queue.len())
                                } else {
                                    tr!("view.steering_plus_followups", count = self.queue.len())
                                }),
                        )
                        .child(
                            div()
                                .id("clear-queue")
                                .px(px(6.))
                                .py(px(1.))
                                .rounded(px(5.))
                                .text_size(theme.ui_px(11.))
                                .text_color(theme.text_2)
                                .cursor_pointer()
                                .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                                .on_mouse_up(MouseButton::Left, cx.listener(Self::on_clear_queue))
                                .child(tr!("view.clear")),
                        ),
                )
                .child(chips)
                .into_any_element(),
        )
    }

    /// The dismissible error banner: a command / protocol / extension failure
    /// surfaced per the docs' error contract. Persists until dismissed. A
    /// dead pi process offers a one-click Reconnect instead of sending the
    /// user to Settings; every banner can copy its text for a bug report.
    pub(super) fn error_banner(&self, theme: Theme, cx: &Context<Self>) -> Option<AnyElement> {
        let message = self.error.as_ref()?;
        let copy_text = message.clone();
        let reconnect = self.runtime.exited;
        Some(
            div()
                .w_full()
                .mb(px(8.))
                .bg(theme.crit.opacity(0.1))
                .border_1()
                .border_color(theme.crit.opacity(0.45))
                .rounded_lg()
                .px(px(12.))
                .py(px(9.))
                .flex()
                .items_center()
                .gap_2()
                .child(icon("icons/info.svg", 15., theme.crit))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(theme.ui_px(12.))
                        .text_color(theme.text)
                        .child(message.clone()),
                )
                .when(reconnect, |banner| {
                    banner.child(
                        div()
                            .id("reconnect-runtime")
                            .flex_none()
                            .px(px(8.))
                            .py(px(3.))
                            .rounded_md()
                            .text_size(theme.ui_px(11.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .cursor_pointer()
                            .bg(theme.overlay)
                            .hover(|s| s.bg(theme.overlay_strong))
                            .on_mouse_up(
                                MouseButton::Left,
                                cx.listener(|this, _, _, cx| {
                                    this.error = None;
                                    this.runtime_restart(cx);
                                }),
                            )
                            .child(tr!("view.reconnect")),
                    )
                })
                .child(
                    div()
                        .id("copy-error")
                        .size(px(20.))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_color(theme.text_2)
                        .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                        .on_mouse_up(
                            MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                            }),
                        )
                        .child(icon("icons/copy.svg", 12., theme.text_2)),
                )
                .child(
                    div()
                        .id("dismiss-error")
                        .size(px(20.))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .text_size(theme.ui_px(13.))
                        .text_color(theme.text_2)
                        .hover(|s| s.bg(theme.overlay).text_color(theme.text))
                        .on_mouse_up(MouseButton::Left, cx.listener(Self::dismiss_error))
                        .child("×"),
                )
                .into_any_element(),
        )
    }

    /// Discard the pending queue without aborting the run. The composer text
    /// stays untouched (unlike Escape, which restores queued text).
    pub(super) fn on_clear_queue(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.restore_queue_on_clear = false;
        self.send(CommandBody::ClearQueue, "clear_queue");
        cx.notify();
    }

    /// Full-window image lightbox: the clicked attachment at `Contain` scale
    /// over a dimmed scrim. Any click (or Escape) closes it. No new surface
    /// for the app — it reads `OrbitApp::lightbox`, set by `image_opener`.
    pub(super) fn lightbox_layer(
        &self,
        image: std::sync::Arc<Image>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = *theme::get(cx);
        let scrim = match theme.mode {
            theme::ThemeMode::Dark => gpui::hsla(0., 0., 0., 0.72),
            theme::ThemeMode::Light => gpui::hsla(0., 0., 0., 0.6),
        };
        div()
            .id("image-lightbox")
            .debug_selector(|| "image-lightbox".to_string())
            .absolute()
            .inset_0()
            .occlude()
            .bg(scrim)
            .p(px(48.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.lightbox = None;
                    cx.notify();
                }),
            )
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(ImageSource::Image(image))
                            .size_full()
                            .object_fit(ObjectFit::Contain),
                    ),
            )
            .into_any_element()
    }
}
