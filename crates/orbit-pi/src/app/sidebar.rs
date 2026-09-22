use super::helpers::*;
use super::*;

use gpui::{point, Pixels};

use crate::sessions::cap_chars;
use crate::shimmer::ShimmerText;

/// Whether a workspace group is collapsed in the sidebar. The active
/// workspace is expanded by default; all others are collapsed unless the
/// user has toggled them.
pub(crate) fn is_workspace_group_collapsed(
    label: &str,
    working_label: &str,
    collapsed_workspaces: &HashSet<String>,
    expanded_workspace_groups: &HashSet<String>,
) -> bool {
    if label == working_label {
        collapsed_workspaces.contains(label)
    } else {
        !expanded_workspace_groups.contains(label)
    }
}

/// Toggle a workspace group's open/closed state against the defaults above.
pub(crate) fn toggle_workspace_group(
    label: String,
    working_label: &str,
    collapsed_workspaces: &mut HashSet<String>,
    expanded_workspace_groups: &mut HashSet<String>,
) {
    if label == working_label {
        if !collapsed_workspaces.remove(&label) {
            collapsed_workspaces.insert(label);
        }
    } else if !expanded_workspace_groups.remove(&label) {
        expanded_workspace_groups.insert(label);
    }
}

/// Sessions visible under one workspace group: at most `limit` rows, with
/// the open session swapped into the last slot when it falls outside.
pub(crate) fn visible_sessions_in_group(
    ixs: &[usize],
    sessions: &[SessionInfo],
    limit: usize,
    active_path: &Option<PathBuf>,
) -> Vec<usize> {
    if ixs.len() <= limit {
        return ixs.to_vec();
    }

    let mut indices: Vec<usize> = ixs.iter().take(limit).copied().collect();
    if let Some(active) = active_path {
        if let Some(active_ix) = sessions.iter().position(|s| &s.path == active) {
            if ixs.contains(&active_ix) && !indices.contains(&active_ix) {
                indices.pop();
                indices.push(active_ix);
                indices.sort_by_key(|ix| ixs.iter().position(|&i| i == *ix).unwrap_or(usize::MAX));
            }
        }
    }
    indices
}

/// Sidebar session list: `sessions` (newest-first, from disk) plus a
/// placeholder row for the open session when its file is not in the store
/// yet — see [`OrbitApp::sidebar_sessions`]. The placeholder carries the
/// workspace the pi process runs in, pi's live title when it has already
/// named the session, and a `now` stamp so it sorts to the top of the
/// sidebar. A session that hasn't started (`session_started` = false — no
/// user message sent yet) gets no placeholder: a draft is not listed
/// (Waku drafts parity).
pub(crate) fn sessions_with_placeholder(
    sessions: &[SessionInfo],
    current_path: Option<&Path>,
    current_title: Option<&str>,
    current_first_message: Option<&str>,
    current_workspace: Option<&Path>,
    session_started: bool,
) -> Vec<SessionInfo> {
    let mut rows = sessions.to_vec();
    let Some(path) = current_path else {
        return rows;
    };
    if rows.iter().any(|s| s.path == path) {
        return rows;
    }
    if !session_started {
        return rows;
    }
    let workspace = current_workspace
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    // The placeholder carries the same two lines a disk row would: pi's live
    // title (or the first message when pi hasn't named it yet) over the first
    // message preview.
    let first_message = cap_chars(current_first_message.unwrap_or_default(), 110);
    let title = current_title
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            if first_message.is_empty() {
                tr!("menu.new_task")
            } else {
                cap_chars(current_first_message.unwrap_or_default(), 80)
            }
        });
    rows.insert(
        0,
        SessionInfo {
            path: path.to_path_buf(),
            id: path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            cwd: workspace,
            title,
            first_message,
            modified: SystemTime::now(),
        },
    );
    rows
}

/// Build the grouped sidebar session list from Orbit's own project list. A
/// workspace appears because the user added it, not because pi happens to
/// have sessions there; sessions in unlisted folders are omitted entirely
/// (they stay on disk). Each group shows at most
/// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`] sessions plus one step of
/// [`SIDEBAR_GROUP_SESSIONS_VISIBLE`] per "Show more" click, so a long
/// history never lands in one go.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_sidebar_rows(
    sessions: &[SessionInfo],
    workspaces: &[PathBuf],
    working_label: &str,
    collapsed_workspaces: &HashSet<String>,
    expanded_workspace_groups: &HashSet<String>,
    expanded_session_groups: &HashMap<String, usize>,
    pinned: &HashSet<PathBuf>,
    active_path: &Option<PathBuf>,
    // Sessions with a live pi process that is currently mid-run. A running
    // session stays visible under a collapsed group header, like the open
    // session, so active work is never lost behind a collapse.
    running_paths: &HashSet<PathBuf>,
) -> Vec<SideRow> {
    let mut side_rows: Vec<SideRow> = Vec::new();
    // One group per listed project, in the order the user added them — an
    // empty project still gets a header (and its `+`) so a task can start
    // there. Groups are keyed by label, so two paths with the same basename
    // share one header (the first listed path wins).
    let mut groups: Vec<(String, PathBuf, Vec<usize>)> = Vec::new();
    for ws in workspaces {
        let label = sessions::workspace_label(ws);
        if groups.iter().any(|(l, _, _)| *l == label) {
            continue;
        }
        groups.push((label, ws.clone(), Vec::new()));
    }
    // Attach each session to its project; a folder that is not listed simply
    // finds no group and stays out of the sidebar.
    for (ix, session) in sessions.iter().enumerate() {
        let label = sessions::workspace_label(&session.cwd);
        if let Some((_, _, ixs)) = groups.iter_mut().find(|(l, _, _)| *l == label) {
            ixs.push(ix);
        }
    }
    // Pinned sessions lead their project group; recency order is preserved
    // within the pinned and unpinned partitions (the sort is stable). Doing
    // this here rather than in `load_sessions` keeps an Orbit-owned
    // preference out of the pi-store scan — and means a pinned session can
    // never be hidden by the per-group truncation below.
    for (_, _, ixs) in groups.iter_mut() {
        ixs.sort_by_key(|&ix| !pinned.contains(&sessions[ix].path));
    }
    for (label, cwd, ixs) in groups {
        let collapsed = is_workspace_group_collapsed(
            &label,
            working_label,
            collapsed_workspaces,
            expanded_workspace_groups,
        );
        side_rows.push(SideRow::Workspace {
            label: label.clone(),
            count: ixs.len(),
            collapsed,
            cwd,
        });
        if collapsed {
            // A collapsed group hides its sessions — except the open one,
            // any running (busy background) ones, and pinned ones. The live
            // session and a deliberate mark stay reachable under the header.
            // `ixs` is already pinned-first, so the pinned rows keep their
            // place at the top.
            for &ix in &ixs {
                let open = active_path.as_deref() == Some(sessions[ix].path.as_path());
                let running = running_paths.contains(&sessions[ix].path);
                if open || running || pinned.contains(&sessions[ix].path) {
                    side_rows.push(SideRow::Session(ix));
                }
            }
            continue;
        }

        // Sessions start at the base cap and grow one step per "Show more"
        // click, so a group with a long history reveals ten rows at a time.
        let extra = expanded_session_groups.get(&label).copied().unwrap_or(0);
        let limit = SIDEBAR_GROUP_SESSIONS_VISIBLE.saturating_add(extra);
        let visible = visible_sessions_in_group(&ixs, sessions, limit, active_path);
        for ix in &visible {
            side_rows.push(SideRow::Session(*ix));
        }

        let hidden_count = ixs.len().saturating_sub(visible.len());
        if hidden_count > 0 {
            side_rows.push(SideRow::ShowMore {
                label: label.clone(),
                count: hidden_count.min(SIDEBAR_GROUP_SESSIONS_VISIBLE),
                // Past the base cap the same row carries the collapse
                // affordance on its right edge, so the group never needs a
                // second toggle row.
                can_collapse: extra > 0,
            });
        } else if extra > 0 {
            // Everything is shown — keep one quiet row to collapse the group
            // back to the base cap.
            side_rows.push(SideRow::ShowLess { label });
        }
    }
    side_rows
}

/// The active workspace's header pinned over the session list, plus how far
/// the next group's header has pushed it up.
pub(crate) struct StickyHeader {
    /// Row index of the pinned [`SideRow::Workspace`] header.
    pub ix: usize,
    /// Pixels to shift the pinned row up (0 while it sits at the list top).
    pub top_offset: Pixels,
}

/// Resolve the sticky header for the session list. Only the active
/// workspace's group pins — and only while it is expanded — so the open
/// session's project stays named while its own sessions scroll. The header
/// scrolls away with its section once the next group's header takes over
/// (same behaviour as the side pane's sticky file header).
pub(crate) fn sticky_sidebar_header(
    list: &ListState,
    rows: &[SideRow],
    active_label: &str,
) -> Option<StickyHeader> {
    let header_ix = rows.iter().position(|row| {
        matches!(
            row,
            SideRow::Workspace {
                label,
                collapsed: false,
                ..
            } if label == active_label
        )
    })?;
    let scroll_top = list.logical_scroll_top();
    if scroll_top.item_ix < header_ix {
        // The real header has not reached the top of the viewport yet.
        return None;
    }
    // The section ends at the next workspace header (or the list end).
    let next_header_ix = rows[header_ix + 1..]
        .iter()
        .position(|row| matches!(row, SideRow::Workspace { .. }))
        .map(|offset| header_ix + 1 + offset);
    if next_header_ix.is_some_and(|next| scroll_top.item_ix >= next) {
        // The section scrolled fully past; its header belongs above the view.
        return None;
    }
    if scroll_top.item_ix == header_ix && scroll_top.offset_in_item <= px(0.) {
        // The real header is exactly at the top — nothing to pin over it.
        return None;
    }
    // Push the pinned header up as the next group's header arrives. Item
    // bounds are in window coordinates, so compare against the list's
    // viewport; unmeasured items are too far away to need a push.
    let top_offset = next_header_ix
        .and_then(|next| {
            let bounds = list.bounds_for_item(next)?;
            let viewport = list.viewport_bounds();
            let y_in_viewport = bounds.origin.y - viewport.origin.y;
            (y_in_viewport < bounds.size.height).then_some(y_in_viewport - bounds.size.height)
        })
        .unwrap_or(px(0.));
    Some(StickyHeader {
        ix: header_ix,
        top_offset,
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_side_row(
    rows: &Rc<Vec<SideRow>>,
    sessions_data: &Rc<Vec<SessionInfo>>,
    active_path: Option<&Path>,
    ix: usize,
    this: &Entity<OrbitApp>,
    agent_running: bool,
    running_paths: &Rc<HashSet<PathBuf>>,
    // Sessions the user pinned — they lead their project group and take a
    // small pin glyph on the title line.
    pinned_paths: &Rc<HashSet<PathBuf>>,
    // Every session with a live (running or warm-idle) pi process. Guards
    // delete, which would otherwise let an alive process recreate the file.
    live_paths: &Rc<HashSet<PathBuf>>,
    session_menu: Option<&SessionMenu>,
    workspace_menu: Option<&WorkspaceMenu>,
    theme: Theme,
) -> impl IntoElement {
    match &rows[ix] {
        SideRow::Workspace {
            label,
            count,
            collapsed,
            cwd,
        } => {
            let label = label.clone();
            let label_for_click = label.clone();
            let cwd_for_new = cwd.clone();
            let this_toggle = this.clone();
            let this_new = this.clone();
            let this_menu = this.clone();
            let menu_open = workspace_menu.is_some_and(|m| m.label == label);
            // Outer shell: inter-group spacing only — horizontal inset comes
            // from the list's `px_2`, so the hover pill lines up with the
            // session rows' (inside the same container) and the chevron lands
            // under the "Projects" label. Hover lives on the inner card so the
            // highlight doesn't bleed into the padding. The header is a minimal
            // label row: chevron + folder + name, the session count pinned to
            // the very end, and the row actions (a `…` menu, then the
            // new-session `+`) fading in to the count's left on hover (all
            // flex_none, so nothing shifts when they appear).
            div()
                .w_full()
                .pt(px(12.))
                .pb(px(2.))
                .group("workspace-row")
                .child(
                    div()
                        .w_full()
                        .h(px(26.))
                        .pl(px(6.))
                        .pr(px(8.))
                        .rounded_md()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.bg_hover))
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            let label = label_for_click.clone();
                            this_toggle.update(cx, |app, cx| {
                                let working = app.workspace_label();
                                toggle_workspace_group(
                                    label,
                                    &working,
                                    &mut app.collapsed_workspaces,
                                    &mut app.expanded_workspace_groups,
                                );
                                cx.notify();
                            });
                        })
                        .child(icon(
                            if *collapsed {
                                "icons/chevron-right.svg"
                            } else {
                                "icons/chevron-down.svg"
                            },
                            10.,
                            theme.text_3,
                        ))
                        .child(icon("icons/folder.svg", 13., theme.text_3))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(theme.ui_px(11.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_3)
                                .child(label.clone()),
                        )
                        // Row actions, revealed on hover: a `…` menu
                        // (remove / copy path) beside the direct new-task `+`.
                        .child(workspace_menu_button(
                            label.clone(),
                            cwd.clone(),
                            menu_open,
                            this_menu.clone(),
                            theme,
                        ))
                        .child(
                            div()
                                .id(ElementId::Name(format!("workspace-new-{label}").into()))
                                .flex_none()
                                .size(px(18.))
                                .rounded_sm()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                // Revealed on row hover — the quiet default
                                // keeps group headers to just label + count.
                                .opacity(0.)
                                .group_hover("workspace-row", |s| s.opacity(1.))
                                .hover(|s| s.bg(theme.overlay))
                                .on_mouse_up(MouseButton::Left, {
                                    let this = this_new.clone();
                                    move |_, window, cx| {
                                        cx.stop_propagation();
                                        let cwd = cwd_for_new.clone();
                                        this.update(cx, |app, cx| {
                                            app.on_new_session_in_workspace(cwd, window, cx);
                                        });
                                    }
                                })
                                .child(icon("icons/plus.svg", 13., theme.text_3)),
                        )
                        // Session count, pinned to the header's right edge.
                        .child(
                            div()
                                .flex_none()
                                .text_size(theme.ui_px(10.5))
                                .text_color(theme.text_3)
                                .child(format!("{count}")),
                        ),
                )
                .into_any_element()
        }
        SideRow::ShowMore {
            label,
            count,
            can_collapse,
        } => {
            let this = this.clone();
            let label_for_click = label.clone();
            // One click reveals exactly one step (or the tail remainder), so
            // the group can never overshoot its session count.
            let step = *count;
            let can_collapse = *can_collapse;
            let this_for_collapse = this.clone();
            let label_for_collapse = label.clone();
            // Left side is the whole "show more" target; the collapse
            // chevron on the right is its own quiet button.
            let more = div()
                .flex_1()
                .h_full()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .cursor_pointer()
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let label = label_for_click.clone();
                    this.update(cx, |app, cx| {
                        app.expanded_session_groups
                            .entry(label)
                            .and_modify(|extra| *extra += step)
                            .or_insert(step);
                        cx.notify();
                    });
                })
                .child(icon("icons/chevron-down.svg", 11., theme.text_3))
                .child(
                    div()
                        .text_size(theme.ui_px(11.))
                        .text_color(theme.text_3)
                        .child(tr!("sidebar.show_count_more", count = count)),
                );
            let mut row = div()
                .w_full()
                .h(px(26.))
                .pl(px(22.))
                .pr(px(4.))
                .flex()
                .items_center()
                .rounded_md()
                .hover(|s| s.bg(theme.bg_hover))
                .child(more);
            if can_collapse {
                row = row.child(
                    div()
                        .flex_none()
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(4.))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme.overlay))
                        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let label = label_for_collapse.clone();
                            this_for_collapse.update(cx, |app, cx| {
                                app.expanded_session_groups.remove(&label);
                                cx.notify();
                            });
                        })
                        .child(icon("icons/chevron-up.svg", 11., theme.text_3)),
                );
            }
            row.into_any_element()
        }
        SideRow::ShowLess { label } => {
            let this = this.clone();
            let label_for_click = label.clone();
            div()
                .w_full()
                .h(px(26.))
                .pl(px(22.))
                .pr_2()
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(theme.bg_hover))
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let label = label_for_click.clone();
                    this.update(cx, |app, cx| {
                        app.expanded_session_groups.remove(&label);
                        cx.notify();
                    });
                })
                .child(icon("icons/chevron-up.svg", 11., theme.text_3))
                .child(
                    div()
                        .text_size(theme.ui_px(11.))
                        .text_color(theme.text_3)
                        .child(tr!("sidebar.show_less")),
                )
                .into_any_element()
        }
        SideRow::Session(ix) => {
            let session = sessions_data[*ix].clone();
            let session_for_click = session.clone();
            let active = active_path == Some(session.path.as_path());
            // The open session runs live; parked (background) sessions run
            // in their own pi processes — both get the loader (Waku).
            let running = (active && agent_running) || running_paths.contains(&session.path);
            let pinned = pinned_paths.contains(&session.path);
            let this = this.clone();
            let this_for_row = this.clone();
            let this_for_menu = this.clone();
            let menu = session_menu.filter(|m| m.path == session.path);
            // Sessions with a live pi process (running or warm) must not be
            // deleted — the process would recreate the file mid-run.
            let deletable = !active && !running && !live_paths.contains(&session.path);
            // Running sessions lead with a small spinner and a shimmering
            // title (shadcn's Marker + `shimmer`); row actions stay available
            // on hover.
            let title = session_title(*ix, session.title.clone().into(), theme, active, running);
            // Indented under its workspace group so the list reads as a
            // tree. The open session takes the `active` fill with `active_fg`
            // ink — the same selected-destination grammar as the nav rows;
            // row actions are revealed on hover. A unique first-message
            // preview adds a second line; a title that already is the prompt
            // stays one line.
            // Outer item carries the inter-row spacing (padding) and the click
            // handler; the inner card holds the background/hover so the gap
            // between cards stays clear. Padding (not margin) is used because
            // the list measures each item's border-box — margins are dropped.
            let mut row = div()
                .id(ElementId::NamedInteger("side-session".into(), *ix as u64))
                .w_full()
                .py(px(1.))
                .cursor_pointer()
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    let session = session_for_click.clone();
                    this_for_row.update(cx, |app, cx| {
                        app.on_open_session(session, cx);
                    });
                });
            let mut card = div()
                .group("srow")
                .w_full()
                .pl(px(22.))
                .pr(px(8.))
                .py(theme.space(5.))
                .rounded_md()
                .flex()
                .items_center()
                .gap(px(6.))
                .when(active, |card| card.bg(theme.active))
                .when(!active, |card| card.hover(|s| s.bg(theme.bg_hover)));
            // Text column: title + actions, then the first-message preview
            // with the age. Every row with a message keeps the same two-line
            // shape — even when the title repeats it — so the list scans
            // evenly. Only a row without a message (a just-named session pi
            // has not flushed) keeps the age on the title line.
            let age = sessions::relative_time(session.modified);
            let show_preview = !session.first_message.trim().is_empty();
            card = card.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    .gap(px(2.))
                    // Line 1 — title with the row actions pinned to its
                    // right: a small running spinner leads, the hover-revealed
                    // `…` menu trails. Wrapped in a flex row so the
                    // `flex_1 min_w_0` title takes the full column width and
                    // paints the name — instead of collapsing to "…" as a bare
                    // flex-column child.
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .when(running, |line| line.child(running_loader(theme, *ix)))
                            .when(pinned, |line| {
                                line.child(icon("icons/pin.svg", 16., theme.text_3))
                            })
                            .child(title)
                            .child(session_menu_button(
                                *ix,
                                menu,
                                session.path.clone(),
                                session.title.clone(),
                                deletable,
                                this_for_menu,
                                theme,
                            ))
                            .when(!show_preview, |line| {
                                line.child(
                                    div()
                                        .flex_none()
                                        .text_size(theme.ui_px(10.5))
                                        .text_color(theme.text_3)
                                        .child(age.clone()),
                                )
                            }),
                    )
                    // Line 2 — first-message preview with the age at the very
                    // end, both tertiary metadata (accent is reserved for the
                    // running signal, not timestamps).
                    .when(show_preview, |col| {
                        col.child(
                            div()
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(theme.ui_px(11.))
                                        .line_height(px(14.))
                                        .text_color(theme.text_3)
                                        .child(session.first_message.clone()),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .text_size(theme.ui_px(10.5))
                                        .text_color(theme.text_3)
                                        .child(age),
                                ),
                        )
                    }),
            );
            row = row.child(card);
            row.into_any_element()
        }
    }
}

/// The hover-revealed '…' button on a quiet session row. Clicking it opens
/// the row's actions popup; propagation stops so the row's open handler
/// doesn't also fire.
pub(crate) fn session_menu_button(
    ix: usize,
    menu: Option<&SessionMenu>,
    path: PathBuf,
    title: String,
    deletable: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let menu_open = menu.is_some();
    let this_for_popup = this.clone();
    div()
        .id(ElementId::NamedInteger("side-more".into(), ix as u64))
        .relative()
        .flex_none()
        .size(px(18.))
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        // Hidden until the row (or the button itself) is hovered, or while
        // this row's menu is open. Icon-only, no background — a filled hover
        // square reads as a patch covering the row's right edge.
        .opacity(if menu_open { 1.0 } else { 0.0 })
        .group_hover("srow", |s| s.opacity(1.))
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            // Keep the click from also opening the session via the row.
            cx.stop_propagation();
            let (path, title, deletable) = (path.clone(), title.clone(), deletable);
            this.update(cx, |app, cx| {
                app.toggle_session_menu(
                    SessionMenu {
                        path,
                        title,
                        deletable,
                        confirm_delete: false,
                    },
                    window,
                    cx,
                );
            });
        })
        .child(icon("icons/more.svg", 14., theme.text_3))
        // The dropdown hangs off a zero-size anchor pinned to the button's
        // top-left corner. The button centers its icon (`items_center` +
        // `justify_center`), and Taffy lays absolutely-positioned children out
        // with the container's alignment — without the pin, the popup's
        // static position is pulled toward the button's center and it opens
        // up-left of the trigger instead of just below it.
        .children(menu.map(|menu| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size(px(0.))
                .child(session_menu_popup(menu, this_for_popup.clone(), theme))
        }))
}

/// A small spinner at the start of a running session row — the shadcn
/// Marker + Spinner pattern. `with_animation` rotates it; no app tick.
fn running_loader(theme: Theme, id: usize) -> impl IntoElement + use<> {
    gpui::svg()
        .path("icons/loader.svg")
        .flex_none()
        .size(px(11.))
        .text_color(theme.accent)
        .with_animation(
            ElementId::NamedInteger("side-spin".into(), id as u64),
            Animation::new(Duration::from_millis(900)).repeat(),
            |svg, delta| {
                svg.with_transformation(Transformation::rotate(radians(
                    delta * std::f32::consts::TAU,
                )))
            },
        )
}

/// A session row's title. Quiet rows render as one truncated line; a running
/// row paints the shadcn `shimmer` — a highlight band sweeping across the
/// glyphs — driven by `with_animation` (self-repainting, no app tick).
fn session_title(
    id: usize,
    title: SharedString,
    theme: Theme,
    active: bool,
    running: bool,
) -> AnyElement {
    let color = if active {
        theme.active_fg
    } else {
        theme.text_2
    };
    let weight = if active {
        FontWeight::MEDIUM
    } else {
        FontWeight::NORMAL
    };
    let size = theme.ui_px(13.);
    let line_height = px(18.);
    if !running {
        return div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(size)
            .line_height(line_height)
            .font_weight(weight)
            .text_color(color)
            .child(title)
            .into_any_element();
    }
    // Reduce-motion: keep the running signal (accent title) without the
    // perpetual sweep.
    if theme.ui.reduce_motion {
        return div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(size)
            .line_height(line_height)
            .font_weight(weight)
            .text_color(theme.accent)
            .child(title)
            .into_any_element();
    }
    // The sweep catches the accent: ink at the edges, ember through the band
    // (shadcn's highlight, spent on the one hue the system rations).
    let highlight = theme.accent;
    div()
        .flex_1()
        .min_w_0()
        .text_size(size)
        .line_height(line_height)
        .font_weight(weight)
        .child(ShimmerText::new(title, color, highlight).with_animation(
            ElementId::NamedInteger("side-shimmer".into(), id as u64),
            Animation::new(Duration::from_millis(2000)).repeat(),
            |mut text, delta| {
                text.phase = delta;
                text
            },
        ))
        .into_any_element()
}

// ── sidebar row-actions popup ──────────────────────────────────────────

/// The actions popup anchored to a session row: Copy path / Reveal in
/// Finder / Delete, or the delete confirmation once armed. Painted via
/// `deferred` + `anchored` (same convention as the composer pickers),
/// dismissed by any outside mouse-down.
pub(crate) fn session_menu_popup(
    menu: &SessionMenu,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let deletable = menu.deletable;
    let confirm = menu.confirm_delete;
    let pinned = crate::pins::contains(&menu.path);

    let body: AnyElement = if confirm {
        // Delete confirmation — the destructive step gets a named victim.
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(theme.ui_px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(tr!("sidebar.delete_this_session")),
                    )
                    .child(
                        div()
                            .text_size(theme.ui_px(11.))
                            .text_color(theme.text_2)
                            .child(tr!("sidebar.removes_the_session_file_from_disk")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.))
                    .child(
                        div()
                            .id("menu-cancel")
                            .h(px(24.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(theme.bg_raised)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.bg_hover))
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_2)
                            .on_mouse_down(MouseButton::Left, {
                                let this = this.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    this.update(cx, |app, cx| app.on_menu_cancel(cx));
                                }
                            })
                            .child(tr!("sidebar.cancel")),
                    )
                    .child(
                        div()
                            .id("menu-confirm-delete")
                            .h(px(24.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(theme.stop_red)
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.stop_red_hover))
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.send_fg)
                            .on_mouse_down(MouseButton::Left, {
                                let this = this.clone();
                                move |_, _, cx| {
                                    cx.stop_propagation();
                                    this.update(cx, |app, cx| app.on_menu_delete_confirm(cx));
                                }
                            })
                            .child(tr!("sidebar.delete")),
                    ),
            )
            .into_any_element()
    } else {
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(menu_item(
                "menu-pin",
                "icons/pin.svg",
                if pinned {
                    tr!("sidebar.unpin_session")
                } else {
                    tr!("sidebar.pin_session")
                },
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_toggle_pin(cx),
            ))
            .child(menu_item(
                "menu-copy-path",
                "icons/copy.svg",
                tr!("sidebar.copy_path"),
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_copy_path(cx),
            ))
            .child(menu_item(
                "menu-reveal",
                "icons/folder.svg",
                tr!("sidebar.reveal_in_finder"),
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_reveal(cx),
            ))
            .child(menu_item(
                "menu-clone",
                "icons/git-fork.svg",
                tr!("sidebar.clone_session"),
                theme,
                this.clone(),
                false,
                |app, cx| app.on_menu_clone_session(cx),
            ))
            .when(deletable, |menu| {
                menu.child(div().h(px(1.)).w_full().bg(theme.border).my(px(4.)))
                    .child(menu_item(
                        "menu-delete",
                        "icons/trash.svg",
                        tr!("sidebar.delete_session_menu"),
                        theme,
                        this.clone(),
                        true,
                        |app, cx| app.on_menu_delete_request(cx),
                    ))
            })
            .into_any_element()
    };

    let popup = div()
        .w(px(190.))
        .p(px(4.))
        .when(confirm, |pop| pop.w(px(210.)).p(px(10.)))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.menu_bg)
        .shadow(theme.popover_shadow())
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        // Any mouse-down outside dismisses; clicks inside are stopped by
        // the item handlers, so they never read as "outside".
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at = Some(Instant::now());
                    app.session_menu = None;
                    cx.notify();
                })
            }
        })
        .child(body);

    // Float the popup: `anchored` takes it out of the layout (no other row
    // moves) and pins its top-left corner just below the '…' button (the
    // button is 18px, so +20px drops the top edge 2px under it, left-aligned
    // to the trigger — the standard app dropdown position). The wrapping
    // zero-size anchor in `session_menu_button` guarantees the static origin
    // is the button's top-left regardless of the button's own centering.
    // `deferred` paints it above the rest of the list — same convention as
    // the composer chip pickers. `snap_to_window` keeps it inside the window
    // near the edges.
    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(Corner::TopLeft)
        .offset(point(px(0.), px(20.)))
        .snap_to_window()
        .child(deferred(popup))
        .into_any_element()
}

/// The hover-revealed '…' button on a workspace group header. Clicking it
/// opens the header's actions popup; propagation stops so the header's
/// collapse toggle doesn't also fire.
pub(crate) fn workspace_menu_button(
    label: String,
    cwd: PathBuf,
    menu_open: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let this_for_popup = this.clone();
    let label_for_click = label.clone();
    let cwd_for_click = cwd.clone();
    div()
        .id(ElementId::Name(format!("workspace-more-{label}").into()))
        .relative()
        .flex_none()
        .size(px(18.))
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        // Revealed on row hover, or while this header's menu is open.
        .opacity(if menu_open { 1.0 } else { 0.0 })
        .group_hover("workspace-row", |s| s.opacity(1.))
        .on_mouse_up(MouseButton::Left, move |_, window, cx| {
            cx.stop_propagation();
            let menu = WorkspaceMenu {
                label: label_for_click.clone(),
                cwd: cwd_for_click.clone(),
            };
            this.update(cx, |app, cx| app.toggle_workspace_menu(menu, window, cx));
        })
        .child(icon("icons/more.svg", 14., theme.text_3))
        .children(menu_open.then(|| {
            div()
                .absolute()
                .top_0()
                .left_0()
                .size(px(0.))
                .child(workspace_menu_popup(this_for_popup.clone(), theme))
        }))
}

/// The actions popup anchored to a workspace header: Copy path / Remove
/// from sidebar. Painted with the same deferred + anchored convention as
/// the session row menu, dismissed by any outside mouse-down.
pub(crate) fn workspace_menu_popup(this: Entity<OrbitApp>, theme: Theme) -> AnyElement {
    let popup = div()
        .w(px(200.))
        .p(px(4.))
        .rounded(px(10.))
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.menu_bg)
        .shadow(theme.popover_shadow())
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        .on_mouse_down_out({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at = Some(Instant::now());
                    app.workspace_menu = None;
                    cx.notify();
                })
            }
        })
        .child(menu_item(
            "wm-copy-path",
            "icons/copy.svg",
            tr!("sidebar.copy_path"),
            theme,
            this.clone(),
            false,
            |app, cx| app.on_workspace_copy_path(cx),
        ))
        .child(div().h(px(1.)).w_full().bg(theme.border).my(px(4.)))
        .child(menu_item(
            "wm-remove",
            "icons/minus.svg",
            tr!("sidebar.remove_from_sidebar"),
            theme,
            this.clone(),
            false,
            |app, cx| app.on_workspace_remove(cx),
        ));

    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(Corner::TopLeft)
        .offset(point(px(0.), px(20.)))
        .snap_to_window()
        .child(deferred(popup))
        .into_any_element()
}

pub(crate) fn menu_item<C, L>(
    id: &'static str,
    icon_path: &'static str,
    label: L,
    theme: Theme,
    this: Entity<OrbitApp>,
    danger: bool,
    on_click: C,
) -> impl IntoElement + use<C, L>
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let label: SharedString = label.into();
    let on_click = on_click;
    let (hover_bg, text_color, icon_color) = if danger {
        (theme.stop_red_hover, theme.send_fg, theme.send_fg)
    } else {
        (theme.bg_hover, theme.text_2, theme.text_3)
    };
    div()
        .id(id)
        .h(px(30.))
        .px(px(8.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .when(danger, |s| s.bg(theme.stop_red))
        .hover(move |s| s.bg(hover_bg))
        .text_size(theme.ui_px(12.5))
        .text_color(text_color)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| (on_click)(app, cx));
        })
        .child(icon(icon_path, 13., icon_color))
        .child(label.to_string())
}

/// Reveal a session file in the OS file manager (macOS first, matching the
/// product's platform stance; other platforms get the containing folder).
pub(crate) fn reveal_in_file_manager(path: PathBuf) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .spawn();
    #[cfg(not(target_os = "macos"))]
    if let Some(dir) = path.parent() {
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("explorer").arg(dir).spawn();
        let _ = dir;
    }
}

/// Empty-state panel for a fresh pi store: what this space is for and how
/// to fill it. No cards, no illustration — one calm statement.
pub(crate) fn empty_sessions_state(theme: Theme) -> impl IntoElement + use<> {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .px(px(20.))
        .pb(px(40.))
        .child(
            div()
                .size(px(36.))
                .rounded_full()
                .bg(theme.bg_raised)
                .flex()
                .items_center()
                .justify_center()
                .child(icon("icons/spark.svg", 16., theme.accent)),
        )
        .child(
            div()
                .text_size(theme.ui_px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_2)
                .child(tr!("sidebar.no_projects_yet")),
        )
        .child(
            div()
                .text_size(theme.ui_px(11.5))
                .text_color(theme.text_3)
                .text_align(TextAlign::Center)
                .child(tr!("sidebar.pick_a_folder_to_start_your_first_task")),
        )
}

// ── controller ────────────────────────────────────────────────────
impl OrbitApp {
    /// Open (or toggle closed) the row-actions popup for a session row.
    pub(super) fn toggle_session_menu(
        &mut self,
        menu: SessionMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Same-click dismissal must not re-open (see toggle_session_picker).
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.session_menu.as_ref() == Some(&menu) {
            self.session_menu = None;
            cx.notify();
            return;
        }
        if self.command_palette.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.branch_picker.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.workspace_picker.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.workspace_menu = None;
        self.session_menu = Some(menu);
        cx.notify();
    }

    pub(super) fn on_menu_copy_path(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.session_menu {
            cx.write_to_clipboard(ClipboardItem::new_string(
                menu.path.to_string_lossy().into_owned(),
            ));
        }
        self.session_menu = None;
        cx.notify();
    }

    pub(super) fn on_menu_reveal(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.session_menu {
            reveal_in_file_manager(menu.path.clone());
        }
        self.session_menu = None;
        cx.notify();
    }

    /// Duplicate the session the open row menu belongs to, so it shows up as
    /// its own row to branch off. The source is only read, so — unlike Delete —
    /// this is safe for a session with a live pi process.
    pub(super) fn on_menu_clone_session(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.session_menu.take() else {
            return;
        };
        match sessions::clone_session_file(&menu.path) {
            Ok(_) => {
                self.sessions = sessions::load_sessions();
            }
            Err(err) => self.toast_error(tr!("sidebar.clone_failed", error = err)),
        }
        cx.notify();
    }

    /// Pin or unpin the session the open row menu belongs to. The pin lives
    /// in Orbit's own store (`~/.orbit-pi/pinned-sessions.json`); pi's
    /// session file is never touched.
    pub(super) fn on_menu_toggle_pin(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.take() {
            crate::pins::toggle(&menu.path);
        }
        cx.notify();
    }

    /// First Delete click: swap the popup to the confirmation state.
    pub(super) fn on_menu_delete_request(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.as_mut() {
            menu.confirm_delete = true;
            cx.notify();
        }
    }

    /// Confirmed: remove the session file from disk and refresh the list.
    pub(super) fn on_menu_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.take() {
            // Defensive: a warm parked process would recreate the file.
            if self.lives.contains_key(&menu.path) {
                self.toast_warning(tr!("sidebar.live_process_delete"));
                cx.notify();
                return;
            }
            if let Err(err) = fs::remove_file(&menu.path) {
                self.toast_error(tr!("sidebar.delete_failed", error = err));
            } else {
                // A deleted session must not leave a stale pin behind.
                crate::pins::remove(&menu.path);
            }
            self.sessions = sessions::load_sessions();
            cx.notify();
        }
    }

    pub(super) fn on_menu_cancel(&mut self, cx: &mut Context<Self>) {
        self.session_menu = None;
        cx.notify();
    }

    /// Open (or toggle closed) the row-actions popup for a workspace header.
    pub(super) fn toggle_workspace_menu(
        &mut self,
        menu: WorkspaceMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Same-click dismissal must not re-open (see toggle_session_menu).
        const GESTURE: Duration = Duration::from_millis(200);
        if let Some(dismissed) = self.menu_dismissed_at.take() {
            if dismissed.elapsed() < GESTURE {
                return;
            }
        }
        if self.workspace_menu.as_ref() == Some(&menu) {
            self.workspace_menu = None;
            cx.notify();
            return;
        }
        if self.command_palette.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.branch_picker.take().is_some() {
            self.input.read(cx).focus(window);
        }
        if self.workspace_picker.take().is_some() {
            self.input.read(cx).focus(window);
        }
        self.session_menu = None;
        if self.model_selector.is_some() {
            self.close_model_selector(window, cx);
        }
        self.workspace_menu = Some(menu);
        cx.notify();
    }

    pub(super) fn on_workspace_copy_path(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.workspace_menu {
            cx.write_to_clipboard(ClipboardItem::new_string(
                menu.cwd.to_string_lossy().into_owned(),
            ));
        }
        self.workspace_menu = None;
        cx.notify();
    }

    /// Remove the workspace the open header menu belongs to from Orbit's own
    /// project list. pi's session files are never touched — the folder can be
    /// added again by picking it to work in.
    pub(super) fn on_workspace_remove(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.workspace_menu.take() else {
            return;
        };
        self.remove_workspace(&menu.cwd);
        // Forget the dropped group's view state so re-adding it starts fresh.
        self.collapsed_workspaces.remove(&menu.label);
        self.expanded_workspace_groups.remove(&menu.label);
        self.expanded_session_groups.remove(&menu.label);
        cx.notify();
    }

    /// Keep the row menu honest: close it when its session vanishes from
    /// the store (deleted externally, session ended, …).
    pub(super) fn sync_session_menu(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = &self.session_menu {
            if !menu.path.exists() {
                self.session_menu = None;
                cx.notify();
            }
        }
    }
}
