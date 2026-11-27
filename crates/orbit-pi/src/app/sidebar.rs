use super::format::*;
use super::*;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::menu::MenuKey;
use crate::ui::popover::anchored_menu;
use crate::ui::popover::row_menu;
use crate::ui::prelude::*;

use gpui::{point, Div, Hsla, Pixels};
use std::{cell::Cell, rc::Rc};

use crate::motion::Hover;
use crate::sessions::cap_chars;
use crate::shimmer::ShimmerText;
use crate::theme::tokens::{
    context_menu, list_item, looping, picker, popover, tooltip, BufferLineHeight, ButtonSize,
    DynamicSpacing, IconSize, Radius, StyledExt, TextSize, BORDER_WIDTH,
};
use crate::ui::button::IconButton;
use crate::ui::field::search_field;
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::row::{nav_row, nav_row_fill, NavRowHeight};

/// How far a grouped session row sits in from its project header: the
/// header's hairline, its leading inset and the chevron column, plus the gap
/// after it — so a session card's left edge lines up under the project mark
/// and the list reads as project → sessions.
fn session_indent(theme: &Theme) -> Pixels {
    BORDER_WIDTH
        + DynamicSpacing::Base06.px(theme)
        + IconSize::XSmall.px(theme)
        + DynamicSpacing::Base06.px(theme)
}

/// Horizontal centre of the header chevron, where the tree guide under an
/// expanded project runs.
fn tree_guide_x(theme: &Theme) -> Pixels {
    BORDER_WIDTH + DynamicSpacing::Base06.px(theme) + IconSize::XSmall.px(theme) / 2.
}

/// The project's tree guide, a 1px rule down the chevron column.
fn tree_guide(theme: &Theme) -> Div {
    div()
        .debug_selector(|| "sidebar-tree-guide".into())
        .absolute()
        .top_0()
        .bottom_0()
        .left(tree_guide_x(theme))
        .w(BORDER_WIDTH)
        .bg(theme.hairline_subtle())
}

/// Seat a row that belongs to a project group (a "show more/less" row) at the
/// session indent, on the group's tree guide.
pub(super) fn nested_group_row(row: impl IntoElement, theme: &Theme) -> AnyElement {
    div()
        .relative()
        .w_full()
        .py(DynamicSpacing::Base02.px(theme))
        .pl(session_indent(theme))
        .child(tree_guide(theme))
        .child(row)
        .into_any_element()
}
use crate::workspace_mark::{self, WorkspaceMark};

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
/// user message sent yet) gets no placeholder: a draft is not listed.
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
    // Sidebar ordering for the workspace groups, plus the per-workspace
    // added-at stamps used by [`WorkspaceSort::DateAdded`].
    sort: WorkspaceSort,
    added_at: &HashMap<PathBuf, SystemTime>,
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
    // Sessions carrying a review mark. They stay visible under a collapsed
    // group header for the same reason running ones do — the whole point of
    // the mark is that the result is still unread, and a collapse that hides
    // it turns the mark back into a search (issue #83).
    review_paths: &HashSet<PathBuf>,
    // Sessions inside a project group the user arranged by hand: path → place
    // in that group (`session_order`). Empty keeps plain recency, which is
    // what every group starts as.
    session_ranks: &HashMap<PathBuf, usize>,
) -> Vec<SideRow> {
    let mut side_rows: Vec<SideRow> = Vec::new();
    for (label, cwd, ixs) in
        sidebar_groups(sessions, workspaces, sort, added_at, pinned, session_ranks)
    {
        emit_workspace_group(
            &mut side_rows,
            sessions,
            label,
            cwd,
            &ixs,
            working_label,
            collapsed_workspaces,
            expanded_workspace_groups,
            expanded_session_groups,
            pinned,
            active_path,
            running_paths,
            review_paths,
        );
    }
    side_rows
}

/// Pinned sessions lead. Everything else keeps the store's newest-activity
/// order (stable sort). Opening a session, or a run that has not written a
/// message yet, must not move the row — only a new message changes
/// [`SessionInfo::modified`].
fn session_lead_rank(path: &Path, pinned: &HashSet<PathBuf>) -> u8 {
    if pinned.contains(path) {
        0
    } else {
        1
    }
}

/// A session's place inside its project group: pinned first, then the sessions
/// nobody has arranged yet in recency order, then the group's arranged block in
/// the order the user put it in.
///
/// A session missing from the arranged list is one created after the group was
/// arranged (or a group nobody reordered): it keeps its recency slot *above*
/// the block, so new work is still the first row under its header. The sort is
/// stable, so all the "not arranged" ties keep the store's newest-first order.
fn session_row_key(
    path: &Path,
    pinned: &HashSet<PathBuf>,
    session_ranks: &HashMap<PathBuf, usize>,
) -> (u8, u8, usize) {
    match session_ranks.get(path) {
        Some(rank) => (session_lead_rank(path, pinned), 1, *rank),
        None => (session_lead_rank(path, pinned), 0, 0),
    }
}

/// One group per listed project, in the order the user added them — an
/// empty project still gets a header (and its `+`) so a task can start
/// there. Groups are keyed by label, so two paths with the same basename
/// share one header (the first listed path wins). Attaches each session to
/// its project; a folder that is not listed simply finds no group. Used by
/// [`build_sidebar_rows`].
fn sidebar_groups(
    sessions: &[SessionInfo],
    workspaces: &[PathBuf],
    sort: WorkspaceSort,
    added_at: &HashMap<PathBuf, SystemTime>,
    pinned: &HashSet<PathBuf>,
    session_ranks: &HashMap<PathBuf, usize>,
) -> Vec<(String, PathBuf, Vec<usize>)> {
    let mut groups: Vec<(String, PathBuf, Vec<usize>)> = Vec::new();
    for ws in workspaces {
        let label = sessions::workspace_label(ws);
        if groups.iter().any(|(l, _, _)| *l == label) {
            continue;
        }
        groups.push((label, ws.clone(), Vec::new()));
    }
    for (ix, session) in sessions.iter().enumerate() {
        let label = sessions::workspace_label(&session.cwd);
        if let Some((_, _, ixs)) = groups.iter_mut().find(|(l, _, _)| *l == label) {
            ixs.push(ix);
        }
    }
    // Order the groups for the active sort mode before any rows are laid out;
    // a group's own session order is independent (pinned-first, then recency).
    sort_workspace_groups(&mut groups, sessions, sort, added_at);
    // Pins rise to the top of their project group, and a group the user
    // arranged by hand keeps that order (see [`session_row_key`]). Recency is
    // preserved for everything else (the sort is stable). Doing this here
    // rather than in `load_sessions` keeps an Orbit-owned preference out of
    // the pi-store scan — and means a pin can never be hidden by the
    // per-group truncation below. The open session and a live run stay where
    // their last message put them.
    for (_, _, ixs) in groups.iter_mut() {
        ixs.sort_by_key(|&ix| session_row_key(&sessions[ix].path, pinned, session_ranks));
    }
    groups
}

/// Lay one workspace group into rows: its header, then either the collapsed
/// always-visible rows or the capped session list with its overflow toggle.
#[allow(clippy::too_many_arguments)]
fn emit_workspace_group(
    side_rows: &mut Vec<SideRow>,
    sessions: &[SessionInfo],
    label: String,
    cwd: PathBuf,
    ixs: &[usize],
    working_label: &str,
    collapsed_workspaces: &HashSet<String>,
    expanded_workspace_groups: &HashSet<String>,
    expanded_session_groups: &HashMap<String, usize>,
    pinned: &HashSet<PathBuf>,
    active_path: &Option<PathBuf>,
    running_paths: &HashSet<PathBuf>,
    review_paths: &HashSet<PathBuf>,
) {
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
        // `ixs` is already leading-first, so the pinned rows keep their
        // place at the top.
        for &ix in ixs {
            let open = active_path.as_deref() == Some(sessions[ix].path.as_path());
            let running = running_paths.contains(&sessions[ix].path);
            if open
                || running
                || pinned.contains(&sessions[ix].path)
                || review_paths.contains(&sessions[ix].path)
            {
                side_rows.push(SideRow::Session(ix));
            }
        }
        return;
    }

    // Sessions start at the base cap and grow one step per "Show more"
    // click, so a group with a long history reveals three rows at a time.
    let extra = expanded_session_groups.get(&label).copied().unwrap_or(0);
    let limit = SIDEBAR_GROUP_SESSIONS_VISIBLE.saturating_add(extra);
    let mut visible = visible_sessions_in_group(ixs, sessions, limit, active_path);
    // Do not hide deliberate pins or live work behind the history cap, even
    // when there are more protected rows than the default number of slots.
    for &ix in ixs {
        let path = &sessions[ix].path;
        if (pinned.contains(path) || running_paths.contains(path)) && !visible.contains(&ix) {
            visible.push(ix);
        }
    }
    visible.sort_by_key(|ix| {
        ixs.iter()
            .position(|candidate| candidate == ix)
            .unwrap_or(usize::MAX)
    });
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

/// Build the flat sidebar list: every listed project's sessions in one
/// ungrouped list, with no workspace headers, show-more, or collapse rows.
/// Pins lead; the rest keep the store's newest-first order (the sort is
/// stable). Opening a session or a run with no new message does not reorder.
/// `active_path` and `running_paths` are accepted so callers stay aligned
/// with [`build_sidebar_rows`]; they do not affect order.
/// Used when the user turns workspace grouping off; the same
/// `SideRow::Session` rows mean the render and keyboard paths are unchanged.
pub(crate) fn build_flat_sidebar_rows(
    sessions: &[SessionInfo],
    pinned: &HashSet<PathBuf>,
    _active_path: &Option<PathBuf>,
    _running_paths: &HashSet<PathBuf>,
) -> Vec<SideRow> {
    let mut ixs: Vec<usize> = (0..sessions.len()).collect();
    // Stable: pins rise without disturbing the newest-first order of the rest.
    ixs.sort_by_key(|&ix| session_lead_rank(&sessions[ix].path, pinned));
    ixs.into_iter().map(SideRow::Session).collect()
}

/// How many untouched sessions the Recent view adds below the ones that matter.
pub(crate) const SIDEBAR_RECENT_OTHERS: usize = 6;

/// The Recent view's rows: every session that is pinned, open, or running —
/// those are the ones the user is actively working with — then the most recent
/// of everything else, capped at [`SIDEBAR_RECENT_OTHERS`].
///
/// Deterministic: ties on `modified` break by path, so two sessions written in
/// the same second cannot swap places between frames. An empty result is what
/// the sidebar's own empty state is for; there is no separate "no recents".
pub(crate) fn build_recent_sidebar_rows(
    sessions: &[SessionInfo],
    pinned: &HashSet<PathBuf>,
    active_path: &Option<PathBuf>,
    running_paths: &HashSet<PathBuf>,
) -> Vec<SideRow> {
    let included = |ix: usize| {
        let path = &sessions[ix].path;
        pinned.contains(path) || active_path.as_ref() == Some(path) || running_paths.contains(path)
    };
    let mut leading: Vec<usize> = (0..sessions.len()).filter(|&ix| included(ix)).collect();
    let mut others: Vec<usize> = (0..sessions.len()).filter(|&ix| !included(ix)).collect();
    // Newest activity first; the path breaks ties deterministically.
    let by_recency = |a: &usize, b: &usize| {
        sessions[*b]
            .modified
            .cmp(&sessions[*a].modified)
            .then_with(|| sessions[*a].path.cmp(&sessions[*b].path))
    };
    others.sort_by(by_recency);
    others.truncate(SIDEBAR_RECENT_OTHERS);
    leading.extend(others);
    leading.sort_by(|a, b| {
        session_lead_rank(&sessions[*a].path, pinned)
            .cmp(&session_lead_rank(&sessions[*b].path, pinned))
            .then_with(|| by_recency(a, b))
    });
    leading.into_iter().map(SideRow::Session).collect()
}

/// A day section of the date-grouped sidebar. Pinned sessions keep their own
/// section on top; everything else falls in the bucket of its last activity,
/// counted in local calendar days so "Yesterday" means the date, not 24h ago.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DateBucket {
    Pinned,
    Today,
    Yesterday,
    /// Two to six calendar days ago.
    ThisWeek,
    /// Seven to twenty-nine calendar days ago.
    ThisMonth,
    Older,
}

impl DateBucket {
    /// The bucket for a session last touched at `modified`, seen from `now`.
    /// A timestamp in the future (clock skew, a file from another machine)
    /// counts as today rather than vanishing.
    pub(crate) fn of(modified: SystemTime, now: SystemTime) -> Self {
        let day = |at: SystemTime| chrono::DateTime::<chrono::Local>::from(at).date_naive();
        match (day(now) - day(modified)).num_days() {
            i64::MIN..=0 => Self::Today,
            1 => Self::Yesterday,
            2..=6 => Self::ThisWeek,
            7..=29 => Self::ThisMonth,
            _ => Self::Older,
        }
    }

    pub(crate) fn label(self) -> String {
        match self {
            Self::Pinned => tr!("sidebar.date_pinned"),
            Self::Today => tr!("sidebar.date_today"),
            Self::Yesterday => tr!("sidebar.date_yesterday"),
            Self::ThisWeek => tr!("sidebar.date_this_week"),
            Self::ThisMonth => tr!("sidebar.date_this_month"),
            Self::Older => tr!("sidebar.date_older"),
        }
    }
}

/// The date-grouped rows: a [`SideRow::DateSection`] per non-empty bucket,
/// then that bucket's sessions newest first. Pins lead under their own
/// section, so a pinned session is never listed twice. Ties on `modified`
/// break by path, so rows cannot swap between frames.
pub(crate) fn build_date_sidebar_rows(
    sessions: &[SessionInfo],
    pinned: &HashSet<PathBuf>,
    now: SystemTime,
) -> Vec<SideRow> {
    let mut ixs: Vec<usize> = (0..sessions.len()).collect();
    ixs.sort_by(|a, b| {
        sessions[*b]
            .modified
            .cmp(&sessions[*a].modified)
            .then_with(|| sessions[*a].path.cmp(&sessions[*b].path))
    });
    let bucket = |ix: usize| {
        if pinned.contains(&sessions[ix].path) {
            DateBucket::Pinned
        } else {
            DateBucket::of(sessions[ix].modified, now)
        }
    };
    // Stable: within a bucket the newest-first order above holds.
    ixs.sort_by_key(|&ix| bucket(ix));
    let mut rows = Vec::with_capacity(ixs.len() + 6);
    let mut start = 0;
    while start < ixs.len() {
        let current = bucket(ixs[start]);
        let end = ixs[start..]
            .iter()
            .position(|&ix| bucket(ix) != current)
            .map_or(ixs.len(), |offset| start + offset);
        rows.push(SideRow::DateSection {
            bucket: current,
            count: end - start,
        });
        rows.extend(ixs[start..end].iter().copied().map(SideRow::Session));
        start = end;
    }
    rows
}

/// A day section's label: the bucket name and its count in the same quiet
/// eyebrow ink as the list's section heading, on a Header-height row so the
/// sessions under it keep the shared 28px rhythm. Not interactive — the keyboard
/// cursor may rest on it, but nothing opens.
fn date_section_row(bucket: DateBucket, count: usize, theme: &Theme) -> AnyElement {
    // Padding, not margin: the virtual list measures each item's border-box.
    let label = nav_row(
        div()
            .id(ElementId::Name(format!("sidebar-date-{bucket:?}").into()))
            .debug_selector(move || format!("sidebar-date-{bucket:?}")),
        theme,
        NavRowHeight::Header,
    )
    .px(DynamicSpacing::Base08.px(theme))
    .gap(DynamicSpacing::Base06.px(theme))
    .child(
        div()
            .text_size(TextSize::XSmall.px(theme))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_muted)
            .child(bucket.label()),
    )
    .child(
        div()
            .text_size(TextSize::XSmall.px(theme))
            .text_color(theme.text_muted)
            .child(count.to_string()),
    );
    div()
        .w_full()
        .pt(DynamicSpacing::Base06.px(theme))
        .child(label)
        .into_any_element()
}

/// Apply the archived-session filter. `Show` keeps every session, `Hide`
/// drops archived ones, and `Only` keeps just the archived ones. Runs before
/// row building so `SideRow::Session` indices stay aligned with the list the
/// render path receives.
/// The sidebar's "nothing matched" state: the query in the title and the way
/// out in a button, so a search with no hits is not a dead end.
pub(crate) fn empty_search_state(
    theme: &Theme,
    query: &str,
    input: Entity<ComposerInput>,
) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(DynamicSpacing::Base06.px(theme))
        .px(DynamicSpacing::Base16.px(theme))
        .child(
            div()
                .text_size(TextSize::Small.px(theme))
                .text_color(theme.text_muted)
                .text_align(TextAlign::Center)
                .child(tr!("sidebar.no_sessions_match", query = query)),
        )
        .child(
            Button::new("sidebar-search-empty-clear", ButtonVariant::Secondary)
                .size(ButtonSize::Compact)
                .label(tr!("common.clear"))
                .faded(theme, move |button| {
                    button.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        input.update(cx, |input, cx| input.clear(cx));
                    })
                }),
        )
        .into_any_element()
}

/// Number shortcuts are available only with the platform's primary modifier
/// held on its own, not while composing a different shortcut.
pub(crate) fn show_session_shortcuts(modifiers: gpui::Modifiers) -> bool {
    modifiers.secondary() && modifiers.number_of_modifiers() == 1
}

/// Whether a session answers the sidebar's search query.
///
/// Case-insensitive, matched against the four things a row shows: pi's title,
/// the first message, the workspace label, and the session id. Matching the id
/// means a pasted identifier finds its session even after its title changed.
pub(crate) fn session_matches(session: &SessionInfo, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    session.title.to_lowercase().contains(&needle)
        || session.first_message.to_lowercase().contains(&needle)
        || crate::sessions::workspace_label(&session.cwd)
            .to_lowercase()
            .contains(&needle)
        || session.id.to_lowercase().contains(&needle)
}

/// Apply the archive and search filters before building rows, so every row
/// index refers to the same session snapshot in rendering and navigation.
pub(crate) fn filter_sidebar_sessions(
    sessions: Vec<SessionInfo>,
    filter: SidebarArchivedFilter,
    archived: &HashSet<PathBuf>,
    query: &str,
) -> Vec<SessionInfo> {
    filter_archived(sessions, filter, archived)
        .into_iter()
        .filter(|session| session_matches(session, query))
        .collect()
}

pub(crate) fn filter_archived(
    sessions: Vec<SessionInfo>,
    filter: SidebarArchivedFilter,
    archived: &HashSet<PathBuf>,
) -> Vec<SessionInfo> {
    match filter {
        SidebarArchivedFilter::Show => sessions,
        SidebarArchivedFilter::Hide => sessions
            .into_iter()
            .filter(|session| !archived.contains(&session.path))
            .collect(),
        SidebarArchivedFilter::Only => sessions
            .into_iter()
            .filter(|session| archived.contains(&session.path))
            .collect(),
    }
}

/// Reorder `(label, cwd, session indices)` groups for the active sidebar
/// sort mode. `groups` is already deduped by label, so reordering never
/// merges two projects.
///
/// `LastUpdated` keys on each group's newest session activity — the group is
/// not necessarily in activity order after the pin sort, so it takes the max
/// rather than the first row. Opening a session does not override the selected
/// workspace sort. `DateAdded` keys on the persisted added-at
/// stamp. Ties break on the label so the order is stable across reloads, and
/// unknown/empty keys sort last.
fn sort_workspace_groups(
    groups: &mut [(String, PathBuf, Vec<usize>)],
    sessions: &[SessionInfo],
    sort: WorkspaceSort,
    added_at: &HashMap<PathBuf, SystemTime>,
) {
    if sort == WorkspaceSort::Manual {
        return;
    }
    let label_key = |label: &str| label.to_lowercase();
    let by_label = |a: &(String, PathBuf, Vec<usize>), b: &(String, PathBuf, Vec<usize>)| {
        label_key(&a.0).cmp(&label_key(&b.0))
    };
    match sort {
        WorkspaceSort::Manual => {}
        WorkspaceSort::AlphabeticalAsc => groups.sort_by(|a, b| by_label(a, b)),
        WorkspaceSort::AlphabeticalDesc => groups.sort_by(|a, b| by_label(b, a)),
        WorkspaceSort::LastUpdated => {
            groups.sort_by(|a, b| {
                group_updated(&b.2, sessions)
                    .cmp(&group_updated(&a.2, sessions))
                    .then_with(|| by_label(a, b))
            });
        }
        WorkspaceSort::DateAdded => groups.sort_by(|a, b| {
            added_at
                .get(&b.1)
                .cmp(&added_at.get(&a.1))
                .then_with(|| by_label(a, b))
        }),
        WorkspaceSort::SessionCount => {
            groups.sort_by(|a, b| b.2.len().cmp(&a.2.len()).then_with(|| by_label(a, b)))
        }
    }
}

/// Newest session activity in a group, or `None` for an empty project. Empty
/// groups sort last under [`WorkspaceSort::LastUpdated`].
fn group_updated(ixs: &[usize], sessions: &[SessionInfo]) -> Option<SystemTime> {
    ixs.iter().map(|&ix| sessions[ix].modified).max()
}

// ── project-group ordering (issue #143) ───────────────────────────────────

/// Rewrite the project list so its groups appear in `labels` order, keeping
/// every folder that shares a label together and in its stored order. A label
/// the list does not know is ignored; a project the labels miss keeps its
/// place at the end, so a stale order can never drop a project.
///
/// This is the step that turns "what the sidebar is showing right now" into a
/// stored order: a drag (or Move up/down) is applied to the displayed order,
/// not to whatever insertion order happened to be on disk.
pub(crate) fn order_workspaces_by_labels(
    workspaces: &[PathBuf],
    labels: &[String],
) -> Vec<PathBuf> {
    let label_of = |path: &PathBuf| crate::sessions::workspace_label(path);
    let mut ordered: Vec<PathBuf> = Vec::with_capacity(workspaces.len());
    for label in labels {
        ordered.extend(
            workspaces
                .iter()
                .filter(|path| label_of(path) == *label)
                .cloned(),
        );
    }
    ordered.extend(
        workspaces
            .iter()
            .filter(|path| !labels.contains(&label_of(path)))
            .cloned(),
    );
    ordered
}

/// Move the `moving` project group so it lands before (or after) the `target`
/// group, returning the new project list. Groups are the unit the sidebar
/// shows: two folders with the same basename share one header, so they travel
/// together and keep their relative order.
///
/// An unknown label, or moving a group onto itself, returns the list unchanged
/// — the caller compares and skips the write.
pub(crate) fn reorder_workspaces(
    workspaces: &[PathBuf],
    moving: &str,
    target: &str,
    after: bool,
) -> Vec<PathBuf> {
    if moving == target {
        return workspaces.to_vec();
    }
    let mut labels: Vec<String> = Vec::new();
    for path in workspaces {
        let label = crate::sessions::workspace_label(path);
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let (Some(from), Some(_)) = (
        labels.iter().position(|label| label == moving),
        labels.iter().position(|label| label == target),
    ) else {
        return workspaces.to_vec();
    };
    labels.remove(from);
    let Some(to) = labels.iter().position(|label| label == target) else {
        return workspaces.to_vec();
    };
    // Inserting *before* the target is the target's own index; *after* is one
    // past it.
    labels.insert(if after { to + 1 } else { to }, moving.to_string());
    order_workspaces_by_labels(workspaces, &labels)
}

/// Move `moving` before (or after) `target` in a session list, returning the
/// new order. An unknown path, or moving a row onto itself, returns the list
/// unchanged — the caller compares and skips the write.
pub(crate) fn reorder_session_paths(
    paths: &[PathBuf],
    moving: &Path,
    target: &Path,
    after: bool,
) -> Vec<PathBuf> {
    if moving == target {
        return paths.to_vec();
    }
    let (Some(from), Some(_)) = (
        paths.iter().position(|path| path == moving),
        paths.iter().position(|path| path == target),
    ) else {
        return paths.to_vec();
    };
    let mut next = paths.to_vec();
    let moved = next.remove(from);
    let Some(to) = next.iter().position(|path| path == target) else {
        return paths.to_vec();
    };
    // Inserting *before* the target is its own index; *after* is one past it.
    next.insert(if after { to + 1 } else { to }, moved);
    next
}

/// The scrolled-to workspace's header pinned over the session list, plus how
/// far the next group's header has pushed it up.
pub(crate) struct StickyHeader {
    /// Row index of the pinned [`SideRow::Workspace`] header.
    pub ix: usize,
    /// Pixels to shift the pinned row up (0 while it sits at the list top).
    pub top_offset: Pixels,
}

/// Resolve the sticky header for the session list: the header of the project
/// group that owns the row at the top of the viewport. Every expanded group
/// pins — not just the active one — so whichever long project the reader has
/// scrolled into keeps its name visible. The header scrolls away with its
/// section once the next group's header takes over (same behaviour as the
/// side pane's sticky file header).
pub(crate) fn sticky_sidebar_header(list: &ListState, rows: &[SideRow]) -> Option<StickyHeader> {
    let scroll_top = list.logical_scroll_top();
    // The last header at or above the top row is the pinned section. Clamp a
    // past-the-end offset (the list reads that way while following) to the
    // final row so a short list still resolves its group.
    let scan_end = scroll_top.item_ix.min(rows.len().checked_sub(1)?);
    let header_ix = rows[..=scan_end]
        .iter()
        .rposition(|row| matches!(row, SideRow::Workspace { .. }))?;
    // A collapsed group shows only its always-visible rows (the open, running,
    // pinned, or review-marked sessions), so there is no long history to label
    // — and pinning its header would mask the previous group's rows.
    if matches!(
        rows.get(header_ix),
        Some(SideRow::Workspace {
            collapsed: true,
            ..
        })
    ) {
        return None;
    }
    if scroll_top.item_ix == header_ix && scroll_top.offset_in_item <= px(0.) {
        // The real header is exactly at the top — nothing to pin over it.
        return None;
    }
    // The section ends at the next workspace header (or the list end).
    let next_header_ix = rows[header_ix + 1..]
        .iter()
        .position(|row| matches!(row, SideRow::Workspace { .. }))
        .map(|offset| header_ix + 1 + offset);
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

/// The one glyph a session row spends on execution state.
///
/// The row keeps a *lane* for it — the leading 16px of every row, at the same
/// x — so the state of the whole list is one vertical scan away, instead of a
/// word at the end of each title competing with the name for the line. Glyph,
/// ink, and words resolve separately from the state, so the mapping is
/// testable without a theme and paints from exactly one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusLane {
    /// A run is in flight; the glyph spins.
    Progress,
    /// The agent is blocked on a person, who is the only thing that can move it.
    Blocked(BlockReason),
    /// The last run finished and nobody has read the result yet.
    UnreadDone,
    /// A failed background run has not been opened yet.
    UnreadFailed,
    /// The completed result is visible or has been opened.
    Done,
    Failed,
    Stopped,
    /// Nothing has run yet.
    Idle,
    /// No execution state was ever observed — another client, an earlier launch.
    Unknown,
}

/// Which person the agent is waiting on. Two different problems, two glyphs: an
/// approval is a gate, a question is a conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockReason {
    Approval,
    Input,
}

/// The lane state for a row, from the two independent facts a row has: what the
/// process is doing (which may never have been observed) and whether its last
/// result was read. A live block outranks an unread mark — the agent is stuck
/// *now*, and a stale "unread" badge would bury that.
pub(crate) fn status_lane(status: Option<SessionStatus>, review: Option<ReviewMark>) -> StatusLane {
    match status {
        Some(SessionStatus::Running | SessionStatus::Retrying | SessionStatus::Compacting) => {
            StatusLane::Progress
        }
        Some(SessionStatus::WaitingApproval) => StatusLane::Blocked(BlockReason::Approval),
        Some(SessionStatus::WaitingInput) => StatusLane::Blocked(BlockReason::Input),
        Some(SessionStatus::Failed) if review == Some(ReviewMark::Failed) => {
            StatusLane::UnreadFailed
        }
        Some(SessionStatus::Failed | SessionStatus::Disconnected) => StatusLane::Failed,
        Some(SessionStatus::Stopped) => StatusLane::Stopped,
        Some(SessionStatus::Idle) => StatusLane::Idle,
        Some(SessionStatus::Completed) | None => match review {
            Some(ReviewMark::Failed) => StatusLane::UnreadFailed,
            Some(ReviewMark::Done) => StatusLane::UnreadDone,
            None if status.is_some() => StatusLane::Done,
            None => StatusLane::Unknown,
        },
    }
}

/// The lane glyph, or nothing. `Idle` and `Unknown` paint no glyph at all: a
/// session that has never run has no state to report, and a column of faint
/// dots would make the states that *do* matter harder to spot. The lane still
/// reserves its width either way, so every row's text stays on one x.
///
/// The spinner is the only glyph that moves: a run in flight is the only thing
/// on the row that is still happening.
pub(crate) fn status_lane_icon(lane: StatusLane) -> Option<&'static str> {
    Some(match lane {
        StatusLane::Progress => "icons/loader.svg",
        StatusLane::Blocked(BlockReason::Approval) => "icons/lock.svg",
        StatusLane::Blocked(BlockReason::Input) => "icons/chat.svg",
        StatusLane::UnreadDone => "icons/eye.svg",
        StatusLane::Done => "icons/circle-check.svg",
        StatusLane::UnreadFailed | StatusLane::Failed => "icons/circle-x.svg",
        StatusLane::Stopped => "icons/stop.svg",
        StatusLane::Idle | StatusLane::Unknown => return None,
    })
}

/// The ink the lane glyph wears. Colour is never the only signal — the glyph's
/// shape already says what happened — but hue is what makes the column
/// scannable: a block and a failure jump, a read result recedes to mute ink.
pub(crate) fn status_lane_ink(lane: StatusLane, theme: &Theme) -> Hsla {
    match lane {
        StatusLane::Progress => theme.accent_text,
        StatusLane::Blocked(_) => theme.warning,
        StatusLane::UnreadDone | StatusLane::Done => theme.success,
        StatusLane::UnreadFailed | StatusLane::Failed => theme.error,
        StatusLane::Stopped => theme.text_muted,
        StatusLane::Idle | StatusLane::Unknown => theme.text_disabled,
    }
}

/// Attention states get words as well as a glyph, so colour is not the only
/// distinction between a result needing review and a viewed result.
pub(crate) fn status_lane_words(lane: StatusLane) -> Option<String> {
    match lane {
        StatusLane::Blocked(BlockReason::Approval) => Some(tr!("sidebar.status_approval")),
        StatusLane::Blocked(BlockReason::Input) => Some(tr!("sidebar.status_input")),
        StatusLane::UnreadDone => Some(tr!("sidebar.status_review_required")),
        StatusLane::UnreadFailed | StatusLane::Failed => Some(tr!("sidebar.status_failed")),
        _ => None,
    }
}

/// Review state takes precedence over the execution label: a completed run
/// needing attention must not announce itself as merely completed.
pub(crate) fn status_lane_tip(lane: StatusLane, status: Option<SessionStatus>) -> Option<String> {
    match lane {
        StatusLane::UnreadDone => Some(tr!("sidebar.review_finished")),
        StatusLane::UnreadFailed => Some(tr!("sidebar.review_failed")),
        StatusLane::Done => Some(tr!("sidebar.status_viewed")),
        _ => status.map(SessionStatus::label),
    }
}

/// A fixed status slot keeps the header aligned as execution state changes.
const SIDEBAR_STATUS_LANE: f32 = 16.;

/// Paint the status independently of hover, with a tooltip describing its meaning.
fn session_status_lane(
    id: usize,
    lane: StatusLane,
    status: Option<SessionStatus>,
    theme: Theme,
) -> AnyElement {
    let ink = status_lane_ink(lane, &theme);
    let size = IconSize::XSmall.px(&theme);
    let glyph: Option<AnyElement> = if lane == StatusLane::Progress {
        Some(crate::ui::prelude::running_spinner(
            ElementId::NamedInteger("side-lane".into(), id as u64),
            size,
            ink,
            theme,
        ))
    } else {
        status_lane_icon(lane).map(|path| {
            // The font's baseline places its visible ink above the centered
            // line box. Lift the static glyph without moving the badge lane.
            icon(path, size, ink)
                .relative()
                .top(px(-1.))
                .into_any_element()
        })
    };
    let lane_el = div()
        .debug_selector(|| "session-status-lane".into())
        .flex_none()
        .size(px(SIDEBAR_STATUS_LANE))
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .children(glyph)
        .when(lane == StatusLane::UnreadFailed, |el| {
            el.child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .size(px(5.))
                    .rounded_full()
                    .bg(theme.warning),
            )
        });
    match status_lane_tip(lane, status) {
        Some(tip) => lane_el.tip(tip).into_any_element(),
        None => lane_el.into_any_element(),
    }
}

/// The two arrows a branch needs, as `(ahead, behind)` labels. A count of zero
/// has no label, and a branch with no upstream has no labels at all — `↑0 ↓0`
/// is a row of arrows that says nothing (`DESIGN.md` §Status Row).
pub(crate) fn divergence_labels(
    ahead_behind: Option<(usize, usize)>,
) -> (Option<String>, Option<String>) {
    let Some((ahead, behind)) = ahead_behind else {
        return (None, None);
    };
    (
        (ahead > 0).then(|| format!("↑{ahead}")),
        (behind > 0).then(|| format!("↓{behind}")),
    )
}

/// Whether a settled turn should leave a review mark on its row (issue #83).
///
/// `in_front` means the transcript is genuinely in front of the user right now:
/// the window is frontmost and no full-page surface covers the chat area. A
/// turn the user watched is not news — the transcript repainted in front of
/// them. Everything else leaves a mark: a background run, a parked session, or
/// a settle in the open session while Orbit sits behind another app (selecting
/// a row in a background window is not "reading" it).
pub(crate) fn should_mark_review(in_front: bool, settled: &Path, open: Option<&Path>) -> bool {
    !(in_front && open == Some(settled))
}

/// Spend the open session's review mark once that session is really on screen.
/// Returns whether anything changed, so the caller only repaints on a real
/// change. One rule for every path that can bring a session into view — a
/// click, a shortcut, the window coming forward, a page closing — keeps them
/// from drifting apart.
pub(crate) fn clear_review_mark(
    marks: &mut HashMap<PathBuf, ReviewMark>,
    open: Option<&Path>,
    in_front: bool,
) -> bool {
    if !in_front {
        return false;
    }
    open.is_some_and(|path| marks.remove(path).is_some())
}

/// A project's glyph, in precedence order: the user's picked mark, then an
/// automatically detected logo, then `status_icon` (the workspace status resolved
/// in `view.rs`: open / running / archived / plain folder), then the plain
/// folder. A custom mark is an explicit choice, so neither a detected logo nor a
/// status glyph overrides it.
///
/// `status_icon` is resolved per frame rather than here: a workspace's status is
/// a whole-column fact (it is "running" when *any* of its sessions is), and a
/// row must not re-derive it. `None` means the map had no entry, not "no
/// status".
fn project_icon(
    mark: &WorkspaceMark,
    logo: Option<&Arc<Image>>,
    status_icon: Option<&SharedString>,
    size: IconSize,
    theme: &Theme,
) -> AnyElement {
    // Untinted project glyphs inherit the theme accent; an explicitly neutral
    // mark stays neutral.
    let tint = match mark.tint.as_deref() {
        None => theme.accent_text,
        key => workspace_mark::tint_color(theme, key),
    };
    // Every project mark takes the same slot: one size, so names line up down
    // the list. The slot is bare — no face, no hairline — so the mark itself
    // is what shows: a glyph centred in it, a logo filling it (corners softly
    // clipped, letterboxed if wide), with the sidebar's own surface behind
    // both instead of a box around each.
    let tile = size.px(theme) + DynamicSpacing::Base08.px(theme);
    let radius = Radius::Small.px(theme);
    let frame = div()
        .flex_none()
        .size(tile)
        .rounded(radius)
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center();
    let has_custom_icon = mark
        .icon
        .as_deref()
        .is_some_and(|icon| workspace_mark::PROJECT_ICONS.contains(&icon));
    if !has_custom_icon {
        if let Some(logo) = logo {
            return frame
                .child(
                    // `Contain` so a wide wordmark is letterboxed in the slot
                    // rather than cropped.
                    img(ImageSource::Image(logo.clone()))
                        .flex_none()
                        .size(tile)
                        .rounded(radius)
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element();
        }
    }
    let inner_size = size.px(theme);
    let path = status_icon
        .cloned()
        .unwrap_or_else(|| workspace_mark::icon_path(mark.icon.as_deref()));
    frame
        .child(icon_dyn(path, inner_size, tint))
        .into_any_element()
}

#[derive(Clone)]
struct SessionTooltip {
    title: SharedString,
    summary: Option<SharedString>,
    shortcut: Option<SharedString>,
    workspace: SharedString,
    branch: Option<SharedString>,
    ahead: Option<SharedString>,
    behind: Option<SharedString>,
    path: SharedString,
    providers: Vec<sessions::SessionProvider>,
}

fn session_provider_label(provider: &sessions::SessionProvider) -> String {
    match &provider.model {
        Some(model) => format!("{} · {model}", provider.provider),
        None => provider.provider.clone(),
    }
}

fn session_card_summary(title: &str, first_message: &str) -> Option<SharedString> {
    // GPUI's line clamp does not bound explicit newlines; fold whitespace first.
    let summary = queue_preview(first_message);
    // A truncated fallback title may only be a prefix of the message, but a
    // named title can have the same prefix. Do not infer title provenance.
    if summary.is_empty() || first_message == title {
        None
    } else {
        Some(summary.into())
    }
}

/// Same wait as GPUI's own tooltips, so scrolling the list does not flash a
/// card on every row the pointer crosses.
const SESSION_CARD_DELAY: Duration = Duration::from_millis(500);
/// Render-time wrapper: dimensions come from the actual window, including resize.
#[derive(IntoElement)]
struct SessionCardPopup {
    hover: SessionTooltip,
    theme: Theme,
    app: Entity<OrbitApp>,
    path: PathBuf,
}

impl gpui::RenderOnce for SessionCardPopup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let path = self.path;
        // The app owns the handle: the wheel scrolls the card, and Page Up /
        // Page Down move the same handle for a reader with no pointer.
        let scroll = self.app.read(cx).session_card_scroll.clone();
        session_card_in_viewport(&self.hover, &self.theme, window.viewport_size(), &scroll)
            .on_hover(move |hovered, _, cx| {
                self.app.update(cx, |app, cx| {
                    app.hover_session_card_popup(path.clone(), *hovered, cx);
                });
            })
    }
}

/// Bespoke session preview: identity, location, providers, then the shortcut hint.
/// A definite tooltip width keeps wrapped data inside the shared floating frame.
/// This is read-only content; pointer and keyboard preview share this paint.
/// Where a Page Up / Page Down leaves the session preview.
///
/// `offset` is the card's current scroll offset — `0` at the top, negative as
/// the content moves up — `page` is how far one press travels, `max` is the
/// card's `max_offset.height`, and `direction` is `1.0` toward the top and
/// `-1.0` toward the end. The result is clamped into `-max..=0`, so a press at
/// either end stays put instead of scrolling into empty space.
fn paged_offset(offset: Pixels, page: Pixels, max: Pixels, direction: f32) -> Pixels {
    if max <= px(0.) {
        return px(0.);
    }
    (offset + page * direction).clamp(-max, px(0.))
}

fn session_card_in_viewport(
    hover: &SessionTooltip,
    theme: &Theme,
    viewport: gpui::Size<Pixels>,
    scroll: &gpui::ScrollHandle,
) -> gpui::Stateful<gpui::Div> {
    let theme = &theme.for_floating();
    let meta = tooltip::META_TEXT.px(theme);
    let icon_size = IconSize::XSmall.px(theme);
    let margin = crate::theme::tokens::popover::WINDOW_MARGIN;
    div()
        .id("session-preview-scroll")
        .occlude()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .max_h((viewport.height - margin * 2.).max(px(0.)))
        .line_height(gpui::relative(1.3))
        .debug_selector(|| "session-tooltip".into())
        .elevation_2(theme)
        .font_family(crate::theme::ui_font_family())
        .text_size(tooltip::TEXT.px(theme))
        .text_color(theme.text_primary)
        .p(DynamicSpacing::Base12.px(theme))
        .w((tooltip::max_width(theme) * 1.5).min((viewport.width - margin * 2.).max(px(0.))))
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base12.px(theme))
        .child(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base04.px(theme))
                .child(
                    div()
                        .w_full()
                        .whitespace_normal()
                        .font_weight(FontWeight::MEDIUM)
                        .child(hover.title.clone()),
                )
                .when_some(hover.summary.clone(), |header, summary| {
                    header.child(
                        div()
                            .debug_selector(|| "session-card-summary".into())
                            .w_full()
                            .line_clamp(3)
                            .text_size(meta)
                            .text_color(theme.text_secondary)
                            .child(summary),
                    )
                }),
        )
        .child(
            div()
                .debug_selector(|| "session-card-location".into())
                .w_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base04.px(theme))
                        .text_size(meta)
                        .child(
                            div()
                                .w_full()
                                .whitespace_normal()
                                .text_color(theme.text_secondary)
                                .font_weight(FontWeight::MEDIUM)
                                .flex()
                                .items_start()
                                .gap(DynamicSpacing::Base06.px(theme))
                                .child(div().flex_none().child(icon(
                                    "icons/folder.svg",
                                    icon_size,
                                    theme.text_muted,
                                )))
                                .child(div().flex_1().min_w_0().child(hover.workspace.clone())),
                        )
                        .child(
                            div()
                                .debug_selector(|| "session-card-path".into())
                                .w_full()
                                .pl(icon_size + DynamicSpacing::Base06.px(theme))
                                .font_family(crate::theme::code_font_family())
                                .whitespace_normal()
                                .text_color(theme.text_muted)
                                .child(hover.path.clone()),
                        )
                        .when(
                            hover.branch.is_some()
                                || hover.ahead.is_some()
                                || hover.behind.is_some(),
                            |location| {
                                location.child(
                                    div()
                                        .debug_selector(|| "session-card-branch".into())
                                        .w_full()
                                        .flex()
                                        .items_start()
                                        .gap(DynamicSpacing::Base06.px(theme))
                                        .child(div().flex_none().child(icon(
                                            "icons/branch.svg",
                                            icon_size,
                                            theme.text_muted,
                                        )))
                                        .when_some(hover.branch.clone(), |line, name| {
                                            line.child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .whitespace_normal()
                                                    .text_color(theme.text_secondary)
                                                    .child(name),
                                            )
                                        })
                                        .when_some(hover.ahead.clone(), |line, ahead| {
                                            line.child(
                                                div()
                                                    .flex_none()
                                                    .text_color(theme.success)
                                                    .child(ahead),
                                            )
                                        })
                                        .when_some(hover.behind.clone(), |line, behind| {
                                            line.child(
                                                div()
                                                    .flex_none()
                                                    .text_color(theme.warning)
                                                    .child(behind),
                                            )
                                        }),
                                )
                            },
                        ),
                ),
        )
        .when(!hover.providers.is_empty(), |card| {
            card.child(
                div()
                    .debug_selector(|| "session-card-providers".into())
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base04.px(theme))
                    .children(hover.providers.iter().map(|provider| {
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .gap(DynamicSpacing::Base06.px(theme))
                            .child(div().flex_none().child(icon_dyn(
                                provider_icon(&provider.provider),
                                icon_size,
                                provider_mark_color(&provider.provider, theme, theme.text_muted),
                            )))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .text_size(meta)
                                    .text_color(theme.text_secondary)
                                    .child(session_provider_label(provider)),
                            )
                    })),
            )
        })
        .when_some(hover.shortcut.clone(), |card, shortcut| {
            card.child(
                div()
                    .debug_selector(|| "session-card-shortcut".into())
                    .w_full()
                    .border_t_1()
                    .border_color(theme.border_subtle)
                    .pt(DynamicSpacing::Base08.px(theme))
                    .whitespace_normal()
                    .text_size(meta)
                    .text_color(theme.text_muted)
                    .child(shortcut),
            )
        })
}

/// What the sidebar's reordering contributes to one row's paint: the drags in
/// flight (for the insertion line and the in-flight dim), and whether this
/// paint may start one. The sticky header overlay paints a header a second time
/// and passes `interactive: false`, so a release can never be resolved against
/// the overlay's copy of the geometry.
pub(crate) struct SidebarDragPaint<'a> {
    /// A project group is being dragged.
    pub project: Option<&'a ProjectDragState>,
    /// A session row is being dragged.
    pub session: Option<&'a SessionDragState>,
    /// This paint may start and accept a drag.
    pub interactive: bool,
    /// Session rows can be rearranged here: the grouped list, where a row
    /// belongs to a project header. Flat, Recent and search lists keep
    /// recency order and offer no session drag.
    pub sessions: bool,
}

impl SidebarDragPaint<'_> {
    /// Nothing is being dragged and this paint cannot start one.
    pub(crate) fn idle() -> Self {
        Self {
            project: None,
            session: None,
            interactive: false,
            sessions: false,
        }
    }
}

/// The chip that follows the pointer while a project group is dragged. It
/// carries the group's own name, so the thing being moved is never in doubt —
/// a bare outline would make the user guess which header is in flight.
struct ProjectDragGhost {
    label: String,
}

impl Render for ProjectDragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drag_ghost(self.label.clone(), cx)
    }
}

/// The chip that follows the pointer while a session row is dragged. The
/// title is what the row is identified by, so it is what travels.
struct SessionDragGhost {
    title: String,
}

impl Render for SessionDragGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        drag_ghost(self.title.clone(), cx)
    }
}

/// Shared chip behind both drag ghosts: one line of the row's own name on the
/// raised surface, so what is in flight is never in doubt.
fn drag_ghost(text: String, cx: &mut App) -> AnyElement {
    // A floating layer while it follows the pointer: the shared elevation
    // gives it the frost, edge and shadow, instead of an opaque chip faded
    // with container opacity.
    let theme = theme::get(cx).for_floating();
    div()
        .px(DynamicSpacing::Base08.px(&theme))
        .py(DynamicSpacing::Base04.px(&theme))
        .elevation_2(&theme)
        .text_size(TextSize::Small.px(&theme))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_primary)
        .child(text)
        .into_any_element()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_side_row(
    rows: &Rc<Vec<SideRow>>,
    sessions_data: &Rc<Vec<SessionInfo>>,
    active_path: Option<&Path>,
    row_ix: usize,
    this: &Entity<OrbitApp>,
    agent_running: bool,
    running_paths: &Rc<HashSet<PathBuf>>,
    // Sessions the user pinned — they lead their project group and take a
    // small pin glyph on the title line.
    pinned_paths: &Rc<HashSet<PathBuf>>,
    // Sessions in the archive set — shown with a small archive glyph when the
    // filter includes them.
    archived_paths: &Rc<HashSet<PathBuf>>,
    // Flat lists have no workspace headers, so each row names its workspace.
    show_workspace: bool,
    show_first_message: bool,
    show_updated: bool,
    show_shortcuts: bool,
    // Per-workspace sidebar marks (icon + tint), keyed by path.
    marks: &Rc<HashMap<PathBuf, WorkspaceMark>>,
    // Resolved per-workspace glyphs: the user's mark icon, or the folder
    // status (current / running / archived-only / idle). Computed once per
    // frame alongside the list so a header row never re-derives it.
    workspace_icons: &Rc<HashMap<PathBuf, SharedString>>,
    logos: &Rc<HashMap<PathBuf, Arc<Image>>>,
    // Branch per workspace, shown on the row and hover card. Absent for a project
    // that is not a git repository (or sits on a detached HEAD).
    branches: &Rc<HashMap<PathBuf, BranchLine>>,
    // Sessions whose last turn settled while the user was looking elsewhere.
    // An unread glyph (plus its "Needs review" label) remains until the
    // session is shown in a foreground window (issue #83); the execution state
    // remains after that mark is spent.
    review_marks: &Rc<HashMap<PathBuf, ReviewMark>>,
    session_statuses: &Rc<HashMap<PathBuf, SessionStatus>>,
    session_menu: Option<&SessionMenu>,
    workspace_menu: Option<&WorkspaceMenu>,
    // Row drag & drop (issue #143): the drags in flight, and whether this
    // paint may start or accept one. The sticky header overlay paints the same
    // header a second time, so it must not — two drop targets for one header
    // would let a release land on the wrong group.
    drag: &SidebarDragPaint,
    // Whether this row is the keyboard cursor (see `OrbitApp::sidebar_cursor`).
    // Painted with a focused surface, distinct from hover and selection.
    cursor: bool,
    // Session whose hover card is open. The card is a child of that row, so
    // it stays pinned to the row's top-right instead of the pointer.
    card_path: Option<&Path>,
    keyboard_card: bool,
    theme: Theme,
) -> impl IntoElement {
    match &rows[row_ix] {
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
            let menu = workspace_menu.filter(|m| m.label == label);
            // The header whose menu is open keeps its wash while the pointer
            // is over the menu, so the menu reads as belonging to this row.
            let menu_open = menu.is_some();
            let mark = marks.get(cwd).cloned().unwrap_or_default();
            let workspace_logo = logos.get(cwd);
            let workspace_icon = workspace_icons.get(cwd);
            // Project reorder (issue #143): dim the header that is in flight,
            // and draw the insertion line on the header the drop would use.
            let project_dragging = drag.project.is_some_and(|state| state.dragging == label);
            let project_insertion = drag
                .project
                .and_then(|state| state.target.as_ref())
                .filter(|target| target.label == label)
                .map(|target| target.after);
            let drag_label = label.clone();
            let this_drop = this.clone();
            // Outer shell: inter-group spacing only — horizontal inset comes
            // from the list's `px_2`, so the hover pill lines up with the
            // session rows' (inside the same container) and the chevron lands
            // under the "Projects" label. Hover lives on the inner card so the
            // highlight doesn't bleed into the padding. The header is the
            // parent row above its sessions: chevron + mark + name, the
            // session count pinned to the very end, and the row actions (a `…`
            // menu, then the new-session `+`) fading in to the count's left on
            // hover (all flex_none, so nothing shifts when they appear). The
            // name and tinted mark distinguish the group from its sessions;
            // the count stays visible while the contextual actions reveal.
            div()
                .w_full()
                .pt(DynamicSpacing::Base12.px(&theme))
                .pb(DynamicSpacing::Base02.px(&theme))
                .group("workspace-row")
                .child(crate::motion::hover(
                    nav_row(
                        div().id(ElementId::Name(format!("workspace-header-{label}").into())),
                        &theme,
                        NavRowHeight::Header,
                    )
                    .pl(DynamicSpacing::Base06.px(&theme))
                    .pr(DynamicSpacing::Base08.px(&theme))
                    .relative()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .cursor_pointer()
                    // A header is a drag handle for its project group. The
                    // press still collapses the group: GPUI starts a drag
                    // only past its movement threshold, and a drag cancels
                    // the pending click, so a click and a drag never both
                    // fire.
                    .when(drag.interactive, |header| {
                        let ghost_label = drag_label.clone();
                        let this_drop = this_drop.clone();
                        header
                            .on_drag(
                                ProjectDrag {
                                    label: drag_label.clone(),
                                },
                                move |_: &ProjectDrag, _, _, cx| {
                                    let label = ghost_label.clone();
                                    cx.new(|_| ProjectDragGhost { label })
                                },
                            )
                            .on_drop::<ProjectDrag>(move |drag: &ProjectDrag, _window, cx| {
                                let drag = drag.clone();
                                this_drop.update(cx, |app, cx| app.on_project_drop(&drag, cx));
                            })
                            // A session drag released on a header is not a
                            // click: it must not collapse the group.
                            .on_drop::<SessionDrag>(|_: &SessionDrag, _, _| {})
                    })
                    .when(project_dragging, |header| header.opacity(0.45))
                    .when_some(project_insertion, |header, after| {
                        header.child(
                            div()
                                .debug_selector(|| "sidebar-project-drop-line".into())
                                .absolute()
                                .left_0()
                                .right_0()
                                .h(px(2.))
                                .rounded_full()
                                .bg(theme.accent)
                                .when(after, |line| line.bottom(px(-1.)))
                                .when(!after, |line| line.top(px(-1.))),
                        )
                    })
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
                            app.record_collapsed_groups();
                            cx.notify();
                        });
                    })
                    // Right-click anywhere on the header opens its
                    // actions menu at the pointer, context-menu style.
                    .on_mouse_down(MouseButton::Right, {
                        let label = label.clone();
                        let cwd = cwd.clone();
                        let this = this.clone();
                        move |event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.update(cx, |app, cx| {
                                let (can_move_up, can_move_down) = app.project_move_flags(&label);
                                let menu = WorkspaceMenu {
                                    label: label.clone(),
                                    cwd: cwd.clone(),
                                    at: Some(event.position),
                                    picker: false,
                                    can_move_up,
                                    can_move_down,
                                    cursor: Default::default(),
                                };
                                app.open_workspace_menu_at(menu, window, cx);
                            });
                        }
                    })
                    .child(icon(
                        if *collapsed {
                            "icons/chevron-right.svg"
                        } else {
                            "icons/chevron-down.svg"
                        },
                        IconSize::XSmall.px(&theme),
                        theme.text_muted,
                    ))
                    // The project mark and name are the sidebar's group
                    // anchors: a full-size glyph and a semibold name so they
                    // read at a glance over the sessions below, CJK names
                    // included (#166).
                    .child(project_icon(
                        &mark,
                        workspace_logo,
                        workspace_icon,
                        IconSize::Medium,
                        &theme,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Default.px(&theme))
                            .font_weight(FontWeight::SEMIBOLD)
                            // Keep the user's project hue on the name as
                            // well as its mark (issue #78).
                            .text_color(workspace_mark::label_ink(&theme, mark.tint.as_deref()))
                            .child(mark.display_name(&label).to_string()),
                    )
                    // Row actions, revealed on hover: a `…` menu
                    // (remove / copy path) beside the direct new-task `+`.
                    .child(workspace_menu_button(
                        label.clone(),
                        cwd.clone(),
                        menu,
                        mark.clone(),
                        this_menu.clone(),
                        theme,
                    ))
                    .child(
                        IconButton::new(
                            ElementId::Name(format!("workspace-new-{label}").into()),
                            "icons/plus.svg",
                        )
                        .size(ButtonSize::Compact)
                        .tip(crate::commands::tooltip(
                            crate::commands::CommandId::NewSession,
                            false,
                        ))
                        .faded(&theme, |button| {
                            // Revealed on row hover, fading in with the
                            // header's wash — the quiet default keeps group
                            // headers to just label + count.
                            crate::motion::follow(
                                button,
                                workspace_header_fade(&label),
                                "workspace-row",
                                &theme,
                                false,
                            )
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
                        }),
                    )
                    // Session count, pinned to the header's right edge.
                    // Bare muted figures, right-aligned in a fixed slot:
                    // metadata, not a control, so it carries no fill that
                    // would compete with the name or read as a button. A
                    // project with nothing in it says so by having no rows
                    // under it; a `0` here would only be a number doing no
                    // work.
                    .when(*count > 0, |header| {
                        header.child(
                            div()
                                .debug_selector(|| "sidebar-project-count".into())
                                .flex_none()
                                .min_w(ButtonSize::Compact.height(&theme))
                                .flex()
                                .justify_end()
                                .text_size(TextSize::XSmall.px(&theme))
                                .text_color(theme.text_muted)
                                .child(format!("{count}")),
                        )
                    }),
                    workspace_header_fade(&label),
                    &theme,
                    nav_row_fill(&theme, false, cursor || menu_open),
                ))
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
                .gap(DynamicSpacing::Base06.px(&theme))
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
                .child(icon(
                    "icons/chevron-down.svg",
                    IconSize::XSmall.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("sidebar.show_count_more", count = count)),
                );
            let mut row = nav_row(
                div().id(ElementId::Name(format!("sidebar-show-more-{label}").into())),
                &theme,
                NavRowHeight::Item,
            )
            .pl(DynamicSpacing::Base08.px(&theme))
            .pr(DynamicSpacing::Base04.px(&theme))
            // A drag released on this row must not expand the group too:
            // the release is not a click (issue #143).
            .when(drag.interactive, |row| {
                row.on_drop::<SessionDrag>(|_: &SessionDrag, _, _| {})
                    .on_drop::<ProjectDrag>(|_: &ProjectDrag, _, _| {})
            })
            .child(more);
            if can_collapse {
                row = row.child(
                    IconButton::glyph(
                        ElementId::Name(format!("sidebar-collapse-{label}").into()),
                        icon(
                            "icons/chevron-up.svg",
                            IconSize::XSmall.px(&theme),
                            theme.text_muted,
                        ),
                    )
                    .size(ButtonSize::Compact)
                    .tip(tr!("sidebar.show_less"))
                    .faded(&theme, |el| {
                        el.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            cx.stop_propagation();
                            let label = label_for_collapse.clone();
                            this_for_collapse.update(cx, |app, cx| {
                                app.expanded_session_groups.remove(&label);
                                cx.notify();
                            });
                        })
                    }),
                );
            }
            nested_group_row(
                crate::motion::hover(
                    row,
                    format!("sidebar-show-more-{label}"),
                    &theme,
                    nav_row_fill(&theme, false, cursor),
                ),
                &theme,
            )
        }
        SideRow::RemoteHost {
            name,
            count,
            collapsed,
            status,
        } => super::remote_sessions_ui::render_remote_host_row(
            name, *count, *collapsed, status, this, cursor, theme,
        ),
        // A host's session path (`/home/…/.pi/agent/sessions/…`) is never a
        // local one, so matching the open path is enough to mark it.
        SideRow::RemoteSession(row) => super::remote_sessions_ui::render_remote_session_row(
            row,
            active_path == Some(Path::new(&row.session.path)),
            this,
            theme,
        ),
        SideRow::DateSection { bucket, count } => date_section_row(*bucket, *count, &theme),
        SideRow::ShowLess { label } => {
            let this = this.clone();
            let label_for_click = label.clone();
            nested_group_row(
                crate::motion::hover(
                    nav_row(
                        div().id(ElementId::Name(format!("sidebar-show-less-{label}").into())),
                        &theme,
                        NavRowHeight::Item,
                    )
                    .pl(DynamicSpacing::Base08.px(&theme))
                    .pr(DynamicSpacing::Base08.px(&theme))
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .cursor_pointer()
                    // Same as the "show more" row: a released drag is not a
                    // click and must not collapse the group.
                    .when(drag.interactive, |row| {
                        row.on_drop::<SessionDrag>(|_: &SessionDrag, _, _| {})
                            .on_drop::<ProjectDrag>(|_: &ProjectDrag, _, _| {})
                    })
                    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        let label = label_for_click.clone();
                        this.update(cx, |app, cx| {
                            app.expanded_session_groups.remove(&label);
                            cx.notify();
                        });
                    })
                    .child(icon(
                        "icons/chevron-up.svg",
                        IconSize::XSmall.px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!("sidebar.show_less")),
                    ),
                    format!("sidebar-show-less-{label}"),
                    &theme,
                    nav_row_fill(&theme, false, cursor),
                ),
                &theme,
            )
        }
        SideRow::Session(ix) => {
            let session = sessions_data[*ix].clone();
            let session_for_click = session.clone();
            let active = active_path == Some(session.path.as_path());
            // The row's place among the visible sessions is the number `⌘1…⌘9`
            // opens it by — the same order `navigable_sessions` cycles.
            let slot = rows[..row_ix]
                .iter()
                .filter(|row| matches!(row, SideRow::Session(_)))
                .count()
                + 1;
            let shortcut_hint = (slot <= 9).then(|| {
                tr!(
                    "sidebar.open_with_shortcut",
                    keys = crate::platform::shortcuts::label(&format!("secondary-{slot}"))
                )
            });
            // The open session runs live; parked (background) sessions run
            // in their own pi processes — both get the loader.
            let running = (active && agent_running) || running_paths.contains(&session.path);
            // Files from other clients or earlier launches have no observed
            // execution state. Do not label them idle or completed by guessing.
            let status = session_statuses.get(&session.path).copied();
            let pinned = pinned_paths.contains(&session.path);
            let archived = archived_paths.contains(&session.path);
            let this = this.clone();
            let this_for_row = this.clone();
            let this_for_menu = this.clone();
            let menu = session_menu.filter(|m| m.path == session.path);
            // The row whose context menu is open holds its wash until the
            // menu closes: moving onto the menu is a hover-leave for the row.
            let menu_open = menu.is_some();
            // Session reorder (issue #143): dim the row in flight and draw the
            // insertion line on the row the drop would use. Only the grouped
            // list offers this — see `SidebarDragPaint::sessions`.
            let session_dragging = drag
                .session
                .is_some_and(|state| state.dragging == session.path);
            let session_insertion = drag
                .session
                .and_then(|state| state.target.as_ref())
                .filter(|target| target.path == session.path)
                .map(|target| target.after);
            let drag_path = session.path.clone();
            let drag_title = session.title.clone();
            // Only a run in flight blocks Delete. An open or warm idle
            // session can go: confirming stops its pi process first, so
            // nothing writes the file back (#166).
            let deletable = !running;
            // Keep titles still; the status icon communicates background activity.
            let title = session_title(*ix, session.title.clone().into(), theme, false);
            // Indented under its workspace group so the list reads as a
            // tree. The open session is marked by `surface_selected` alone:
            // no border, badge or heavier title repeats what the fill says.
            // That surface is the palette's neutral step under every accent
            // (`appearance::with_accent` never recolours it).
            // Row actions live on the right-click menu, so the card carries no
            // hover-revealed control at all.
            // Outer item carries the inter-row spacing (padding) and the click
            // handler; the inner card holds the background/hover so the gap
            // between cards stays clear. Padding (not margin) is used because
            // the list measures each item's border-box — margins are dropped.
            // Under a project header (grouped list) the row steps in by the
            // chevron column and a 1px guide runs down that column, so the
            // sessions read as the header's children. Flat lists stay flush.
            let nested = !show_workspace
                && rows[..row_ix]
                    .iter()
                    .rev()
                    .any(|row| matches!(row, SideRow::Workspace { .. }));
            let mut row = div()
                .id(ElementId::NamedInteger("side-session".into(), *ix as u64))
                .relative()
                .w_full()
                .py(DynamicSpacing::Base02.px(&theme))
                .when(nested, |row| {
                    row.pl(session_indent(&theme)).child(tree_guide(&theme))
                })
                .cursor_pointer()
                // A row is a drag handle for itself. A plain click still opens
                // the session: GPUI starts a drag only past its movement
                // threshold, and a drag cancels the pending click.
                .when(drag.interactive && drag.sessions, |row| {
                    let ghost_title = drag_title.clone();
                    let this_drop = this.clone();
                    row.on_drag(
                        SessionDrag {
                            path: drag_path.clone(),
                            title: drag_title.clone(),
                        },
                        move |_: &SessionDrag, _, _, cx| {
                            let title = ghost_title.clone();
                            cx.new(|_| SessionDragGhost { title })
                        },
                    )
                    .on_drop::<SessionDrag>(
                        move |drag: &SessionDrag, _window, cx| {
                            let drag = drag.clone();
                            this_drop.update(cx, |app, cx| app.on_session_drop(&drag, cx));
                        },
                    )
                })
                // Another kind of drag released here must not open the session
                // either: a project header can be dropped anywhere in the list
                // and the release is not a click.
                .when(drag.interactive, |row| {
                    row.on_drop::<ProjectDrag>(|_: &ProjectDrag, _, _| {})
                })
                .when(session_dragging, |row| row.opacity(0.45))
                .when_some(session_insertion, |row, after| {
                    row.child(
                        div()
                            .debug_selector(|| "sidebar-session-drop-line".into())
                            .absolute()
                            .left_0()
                            .right_0()
                            .h(px(2.))
                            .rounded_full()
                            .bg(theme.accent)
                            .when(after, |line| line.bottom(px(0.)))
                            .when(!after, |line| line.top(px(0.))),
                    )
                })
                .on_hover({
                    let path = session.path.clone();
                    let this = this.clone();
                    move |hovered, _, cx| {
                        this.update(cx, |app, cx| {
                            app.hover_session_card(path.clone(), *hovered, cx);
                        });
                    }
                })
                // Wheel and click do not synthesize a hover-leave, so the
                // card (and a timer that has not fired yet) would stay on
                // the row the pointer has already left.
                .on_scroll_wheel({
                    let this = this.clone();
                    move |_, _, cx| {
                        this.update(cx, |app, cx| app.clear_session_card(cx));
                    }
                })
                // Right-click anywhere on the row opens its actions menu at
                // the pointer. That context menu is the row's only action
                // surface — there is no hover-revealed '…' to compete with
                // the title for the line.
                .on_mouse_down(MouseButton::Right, {
                    let session = session.clone();
                    let this = this.clone();
                    move |event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.update(cx, |app, cx| {
                            let (can_move_up, can_move_down) =
                                app.session_move_flags(&session.path);
                            let menu = SessionMenu {
                                path: session.path.clone(),
                                title: session.title.clone(),
                                deletable,
                                at: Some(event.position),
                                reorderable: app.sessions_reorderable(),
                                can_move_up,
                                can_move_down,
                                cursor: Default::default(),
                            };
                            app.open_session_menu_at(menu, window, cx);
                        });
                    }
                })
                .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                    let session = session_for_click.clone();
                    this_for_row.update(cx, |app, cx| {
                        // A click is a mouse hand-off: leave keyboard nav and
                        // hand focus back to the composer so typing resumes.
                        app.clear_session_card(cx);
                        app.sidebar_cursor = None;
                        app.on_open_session(session, cx);
                        app.input.read(cx).focus(window);
                    });
                });
            // Status sits beside the task. A one-line row is 28px tall, the
            // sidebar row height every other navigation row uses.
            let row_name = format!("session-row-{}", session.id);
            // Focus remains visible even on the selected session.
            let mut card = nav_row(
                div()
                    .id(ElementId::Name(row_name.clone().into()))
                    .debug_selector(|| "session-row-card".into()),
                &theme,
                NavRowHeight::Item,
            )
            .group("srow")
            .relative()
            .px(DynamicSpacing::Base08.px(&theme))
            .py(DynamicSpacing::Base04.px(&theme))
            .gap(DynamicSpacing::Base06.px(&theme));
            let age = sessions::relative_time(session.modified);
            // When enabled, line 2 is the prompt itself: the title is capped
            // and the row is narrower still, so a title that happens to be the
            // same sentence is cut, not redundant — the message line is the
            // only place the ask is readable.
            let message = session.first_message.trim().to_string();
            let mark = marks.get(&session.cwd).cloned().unwrap_or_default();
            // A renamed project's rows carry its custom name too (#166).
            let workspace_label = mark
                .display_name(&sessions::workspace_label(&session.cwd))
                .to_string();
            let workspace_logo = logos.get(&session.cwd);
            let workspace_icon = workspace_icons.get(&session.cwd);
            let branch = branches.get(&session.cwd).cloned();
            // Under a workspace header every session in that folder is on the
            // header's branch, so the line would repeat itself down the list.
            // It stays where it adds something: a flat list (no header names
            // the workspace) and a session whose folder is not its header's.
            let group_cwd = rows[..row_ix].iter().rev().find_map(|row| match row {
                SideRow::Workspace { cwd, .. } => Some(cwd),
                _ => None,
            });
            let show_location = show_workspace || group_cwd != Some(&session.cwd);
            let (ahead, behind) = branch
                .as_ref()
                .map(|branch| divergence_labels(branch.ahead_behind))
                .unwrap_or((None, None));
            let providers = sessions::cached_session_providers(&session.path);
            let review = review_marks
                .get(&session.path)
                .copied()
                // A run in flight supersedes the mark: the lane's spinner is
                // the current truth, and the settle that ends this run sets a
                // fresh one.
                .filter(|_| !running);
            // Resolve the glyph and attention label from the same state.
            let lane = status_lane(status, review);
            let meta_ink = if active {
                theme.text_secondary
            } else {
                theme.text_muted
            };
            // Keep the status or age visible even while the pointer is over the row.
            let slot_words = if status == Some(SessionStatus::Disconnected) {
                Some(SessionStatus::Disconnected.label())
            } else {
                status_lane_words(lane)
            };
            let slot_ink = if slot_words.is_some() {
                status_lane_ink(lane, &theme)
            } else {
                meta_ink
            };
            let has_state = !matches!(lane, StatusLane::Idle | StatusLane::Unknown);
            let slot_text = sidebar_slot_text(slot_words, age, show_updated);
            let row_slot = div()
                .debug_selector(|| "session-status-text".into())
                .flex_none()
                .max_w(px(110.))
                .truncate()
                .text_size(TextSize::XSmall.px(&theme))
                .line_height(BufferLineHeight::Standard.relative())
                .text_color(slot_ink)
                .children(slot_text);
            let status_details = Some(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .when(pinned, |line| {
                        line.child(icon(
                            "icons/pin.svg",
                            IconSize::XSmall.px(&theme),
                            theme.warning,
                        ))
                    })
                    .when(archived, |line| {
                        line.child(icon(
                            "icons/archive.svg",
                            IconSize::XSmall.px(&theme),
                            meta_ink,
                        ))
                    })
                    .child(
                        div()
                            .debug_selector(|| "session-status-badge".into())
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base02.px(&theme))
                            .px(DynamicSpacing::Base04.px(&theme))
                            .py(DynamicSpacing::Base01.px(&theme))
                            .rounded(Radius::Small.px(&theme))
                            .when(has_state, |badge| {
                                badge.bg(status_lane_ink(lane, &theme).opacity(0.10))
                            })
                            .child(session_status_lane(*ix, lane, status, theme))
                            .child(row_slot),
                    ),
            );
            card = card.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .justify_center()
                    // Title and preview are one unit: a tight inner gap, the
                    // row's own inset doing the separating between sessions.
                    .gap(DynamicSpacing::Base02.px(&theme))
                    // Keep session indicators beside the title.
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(title)
                            .children(status_details)
                            .when(show_shortcuts && slot <= 9, |line| {
                                line.child(
                                    div()
                                        .debug_selector(move || format!("session-shortcut-{slot}"))
                                        .child(kbd_shortcut(&format!("secondary-{slot}"), &theme)),
                                )
                            }),
                    )
                    // Line 2 — what was actually asked, one line. An empty
                    // message (a session with nothing sent yet) drops the line.
                    // The text sits in a flex row with `flex_1 min_w_0` like the
                    // title: `truncate()` on a bare block collapses the line to
                    // its ellipsis instead of filling the column.
                    .when(show_first_message && !message.is_empty(), |col| {
                        col.child(
                            div().w_full().flex().items_center().child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .debug_selector(|| "session-preview".into())
                                    .text_size(TextSize::Small.px(&theme))
                                    .line_height(BufferLineHeight::Standard.relative())
                                    // Muted, like the age: the title is the
                                    // row's one primary line.
                                    .text_color(meta_ink)
                                    .tip(message.clone())
                                    .child(message),
                            ),
                        )
                    })
                    // Flat lists name the workspace; grouped rows already have a
                    // header, so they carry this line only when it differs.
                    .when(show_location, |col| {
                        col.child(
                            div()
                                .debug_selector(|| "session-location".into())
                                .w_full()
                                .flex()
                                .items_center()
                                .gap(DynamicSpacing::Base06.px(&theme))
                                .when(show_workspace, |line| {
                                    line.child(
                                        div()
                                            .max_w(px(110.))
                                            .min_w_0()
                                            .flex()
                                            .flex_shrink()
                                            .items_center()
                                            .gap(DynamicSpacing::Base04.px(&theme))
                                            .child(project_icon(
                                                &mark,
                                                workspace_logo,
                                                workspace_icon,
                                                IconSize::XSmall,
                                                &theme,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w_0()
                                                    .truncate()
                                                    .text_size(TextSize::XSmall.px(&theme))
                                                    .text_color(workspace_mark::label_ink(
                                                        &theme,
                                                        mark.tint.as_deref(),
                                                    ))
                                                    .tip(workspace_label.clone())
                                                    .child(workspace_label.clone()),
                                            ),
                                    )
                                })
                                .when_some(branch.as_ref(), |line, branch| {
                                    // Monochrome, like the age: a row's ink is
                                    // its meta tone, so status colour stays the
                                    // one thing in the list that can stand out.
                                    line.child(icon(
                                        "icons/branch.svg",
                                        IconSize::XSmall.px(&theme),
                                        meta_ink,
                                    ))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(TextSize::XSmall.px(&theme))
                                            .text_color(meta_ink)
                                            .tip(branch.name.clone())
                                            .child(branch.name.clone()),
                                    )
                                    // Divergence is read on the open session's row;
                                    // every other row carries it in its hover card.
                                    .when_some(ahead.clone().filter(|_| active), |line, ahead| {
                                        line.child(
                                            div()
                                                .flex_none()
                                                .text_size(TextSize::Small.px(&theme))
                                                .text_color(theme.success)
                                                .child(ahead),
                                        )
                                    })
                                    .when_some(
                                        behind.clone().filter(|_| active),
                                        |line, behind| {
                                            line.child(
                                                div()
                                                    .flex_none()
                                                    .text_size(TextSize::Small.px(&theme))
                                                    .text_color(theme.warning)
                                                    .child(behind),
                                            )
                                        },
                                    )
                                })
                                .when(branch.is_none(), |line| line.child(div().flex_1())),
                        )
                    }),
            );
            // Row actions live on the right-click context menu — the same
            // gesture the rest of the app uses for row actions — so there is
            // no hover-revealed '…' and the title owns the whole line. The
            // popup still needs a stable element to hang off: a zero-size
            // anchor at the card's top-left, which is what the keyboard and
            // palette paths (no pointer origin) anchor to.
            card = card.children(menu.map(|menu| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(0.))
                    .child(session_menu_popup(menu, this_for_menu.clone(), theme))
            }));
            let hover = SessionTooltip {
                title: session.title.clone().into(),
                summary: session_card_summary(&session.title, &session.first_message),
                shortcut: shortcut_hint.map(Into::into),
                workspace: workspace_label.into(),
                branch: branch.map(|branch| branch.name.into()),
                ahead: ahead.map(Into::into),
                behind: behind.map(Into::into),
                path: session.cwd.display().to_string().into(),
                providers,
            };
            if card_path == Some(session.path.as_path())
                || (card_path.is_none()
                    && cursor
                    && keyboard_card
                    && session_menu.is_none()
                    && workspace_menu.is_none())
            {
                card = card.child(div().absolute().top_0().right_0().size(px(0.)).child(
                    anchored_menu(
                        Corner::TopLeft,
                        point(tooltip::offset_x(&theme), px(0.)),
                        SessionCardPopup {
                            hover,
                            theme,
                            app: this.clone(),
                            path: session.path.clone(),
                        },
                    ),
                ));
            }
            row = row.child(crate::motion::hover(
                card,
                row_name,
                &theme,
                nav_row_fill(&theme, active, cursor || menu_open),
            ));
            row.into_any_element()
        }
    }
}

/// A session row's title. Quiet rows render as one truncated line; a running
/// row paints the shadcn `shimmer` — a highlight band sweeping across the
/// glyphs — driven by `with_animation` (self-repainting, no app tick).
fn session_title(id: usize, title: SharedString, theme: Theme, running: bool) -> AnyElement {
    let color = theme.text_primary;
    // One weight for every row: the open session is marked by its fill, so a
    // title never changes width when the selection moves.
    let weight = FontWeight::MEDIUM;
    let size = TextSize::Default.px(&theme);
    let line_height = BufferLineHeight::Standard.relative();
    if !running {
        return div()
            .debug_selector(|| "session-title".into())
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
    if theme.reduce_motion() {
        return div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(size)
            .line_height(line_height)
            .font_weight(weight)
            .text_color(theme.accent_text)
            .child(title)
            .into_any_element();
    }
    // The sweep catches the accent: ink at the edges, accent through the band
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
            Animation::new(looping::SHIMMER).repeat(),
            |mut text, delta| {
                text.phase = delta;
                text
            },
        ))
        .into_any_element()
}

// ── sidebar row-actions popup ──────────────────────────────────────────

/// The actions popup anchored to a session row: rename / pin / archive /
/// copy path / clone / delete, or the delete confirmation once armed. Painted via
/// `deferred` + `anchored` (same convention as the composer pickers),
/// dismissed by any outside mouse-down.
/// One action in a sidebar row menu. The item lists below feed the pointer
/// rows and the keyboard cursor alike, so both reach the same actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowMenuItem {
    SessionMoveUp,
    SessionMoveDown,
    Rename,
    Pin,
    Archive,
    CopyPath,
    Clone,
    Delete,
    ProjectMoveUp,
    ProjectMoveDown,
    ProjectCopyPath,
    Worktrees,
    RenameProject,
    IconAndColor,
    ArchiveSessions,
    RemoveProject,
    DeleteProjectFiles,
}

impl RowMenuItem {
    /// The row's element id (also its test selector).
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::SessionMoveUp => "menu-move-up",
            Self::SessionMoveDown => "menu-move-down",
            Self::Rename => "menu-rename",
            Self::Pin => "menu-pin",
            Self::Archive => "menu-archive",
            Self::CopyPath => "menu-copy-path",
            Self::Clone => "menu-clone",
            Self::Delete => "menu-delete",
            Self::ProjectMoveUp => "wm-move-up",
            Self::ProjectMoveDown => "wm-move-down",
            Self::ProjectCopyPath => "wm-copy-path",
            Self::Worktrees => "wm-worktrees",
            Self::RenameProject => "wm-rename",
            Self::IconAndColor => "wm-icon-color",
            Self::ArchiveSessions => "wm-archive",
            Self::RemoveProject => "wm-delete",
            Self::DeleteProjectFiles => "wm-delete-files",
        }
    }
}

/// A session row's menu, top to bottom, with whether each entry can act now.
/// Move rows appear only where the list can be rearranged.
pub(super) fn session_menu_items(menu: &SessionMenu) -> Vec<(RowMenuItem, bool)> {
    let mut items = Vec::with_capacity(8);
    if menu.reorderable {
        items.push((RowMenuItem::SessionMoveUp, menu.can_move_up));
        items.push((RowMenuItem::SessionMoveDown, menu.can_move_down));
    }
    items.extend([
        (RowMenuItem::Rename, true),
        (RowMenuItem::Pin, true),
        (RowMenuItem::Archive, true),
        (RowMenuItem::CopyPath, true),
        (RowMenuItem::Clone, true),
        (RowMenuItem::Delete, menu.deletable),
    ]);
    items
}

/// A project header's menu, top to bottom, with whether each entry can act.
fn workspace_menu_items(menu: &WorkspaceMenu) -> Vec<(RowMenuItem, bool)> {
    vec![
        (RowMenuItem::ProjectMoveUp, menu.can_move_up),
        (RowMenuItem::ProjectMoveDown, menu.can_move_down),
        (RowMenuItem::ProjectCopyPath, true),
        (RowMenuItem::Worktrees, true),
        (RowMenuItem::RenameProject, true),
        (RowMenuItem::IconAndColor, true),
        (RowMenuItem::ArchiveSessions, true),
        (RowMenuItem::RemoveProject, true),
        (RowMenuItem::DeleteProjectFiles, true),
    ]
}

/// The id of the row under the keyboard cursor, if the keyboard has shown it.
fn lit_row(
    cursor: &crate::ui::menu::MenuCursor,
    items: &[(RowMenuItem, bool)],
) -> Option<&'static str> {
    cursor
        .highlighted(items.len())
        .and_then(|ix| items.get(ix))
        .map(|(item, _)| item.id())
}

pub(crate) fn session_menu_popup(
    menu: &SessionMenu,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let theme = theme.for_floating();
    let lit = lit_row(&menu.cursor, &session_menu_items(menu));
    let deletable = menu.deletable;
    let pinned = crate::pins::contains(&menu.path);
    let archived = crate::archive::contains(&menu.path);

    let body: AnyElement = {
        div()
            .w_full()
            .flex()
            .flex_col()
            .py(picker::list_padding_y(&theme))
            .gap(DynamicSpacing::Base01.px(&theme))
            // Session order first: the pointer equivalent of dragging the row
            // inside its project (#143). An edge with nowhere to go is listed
            // disabled and says why; a list that cannot be rearranged (flat,
            // Recent, search) leaves these rows out entirely.
            .when(menu.reorderable, |list| {
                list.child(session_move_row(
                    "menu-move-up",
                    true,
                    menu.can_move_up,
                    menu,
                    lit == Some("menu-move-up"),
                    theme,
                    this.clone(),
                ))
                .child(session_move_row(
                    "menu-move-down",
                    false,
                    menu.can_move_down,
                    menu,
                    lit == Some("menu-move-down"),
                    theme,
                    this.clone(),
                ))
                .child(context_menu_separator(&theme))
            })
            .child(crate::motion::hover(
                picker_entry(div().id("menu-rename"), &theme)
                    .h(picker::entry_height(&theme))
                    .flex_none()
                    .cursor_pointer()
                    .text_color(theme.text_secondary)
                    .on_mouse_down(MouseButton::Left, {
                        let this = this.clone();
                        move |_, window, cx| {
                            cx.stop_propagation();
                            this.update(cx, |app, cx| app.on_menu_rename_session(window, cx));
                        }
                    })
                    .child(icon(
                        "icons/pencil.svg",
                        context_menu::ICON.px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(tr!("explorer.rename")),
                    ),
                "menu-rename",
                &theme,
                Hover::wash_in(theme.overlay).held(lit == Some("menu-rename")),
            ))
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
                lit == Some("menu-pin"),
                |app, cx| app.on_menu_toggle_pin(cx),
            ))
            .child(menu_item(
                "menu-archive",
                "icons/archive.svg",
                if archived {
                    tr!("sidebar.unarchive_session")
                } else {
                    tr!("sidebar.archive_session")
                },
                theme,
                this.clone(),
                false,
                lit == Some("menu-archive"),
                |app, cx| app.on_menu_toggle_archive(cx),
            ))
            .child(menu_item(
                "menu-copy-path",
                "icons/copy.svg",
                tr!("sidebar.copy_path"),
                theme,
                this.clone(),
                false,
                lit == Some("menu-copy-path"),
                |app, cx| app.on_menu_copy_path(cx),
            ))
            .child(menu_item(
                "menu-clone",
                "icons/git-fork.svg",
                tr!("sidebar.clone_session"),
                theme,
                this.clone(),
                false,
                lit == Some("menu-clone"),
                |app, cx| app.on_menu_clone_session(cx),
            ))
            // Delete is always listed. A session that is open or running
            // cannot be deleted — pi would recreate the file mid-run — but
            // hiding the row made the action look missing; a disabled row
            // that explains itself is the honest shape (#131).
            .child(context_menu_separator(&theme))
            .child(if deletable {
                menu_item(
                    "menu-delete",
                    "icons/trash.svg",
                    tr!("sidebar.delete_session_menu"),
                    theme,
                    this.clone(),
                    true,
                    lit == Some("menu-delete"),
                    |app, cx| app.on_menu_delete_request(cx),
                )
                .into_any_element()
            } else {
                menu_item_unavailable(
                    "menu-delete",
                    "icons/trash.svg",
                    tr!("sidebar.delete_session_menu"),
                    tr!("sidebar.delete_session_blocked"),
                    theme,
                )
            })
            .into_any_element()
    };

    // The branch selector's picker shell sizes the action list to its rows.
    let popup = picker_surface(div(), &theme)
        .flex()
        .flex_col()
        .overflow_hidden()
        // Any mouse-down outside dismisses; clicks inside are stopped by
        // the item handlers, so they never read as "outside".
        .dismiss_on_outside({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at.arm();
                    app.session_menu = None;
                    cx.notify();
                })
            }
        })
        .child(body);

    // Float the popup: `anchored` takes it out of the layout (no other row
    // moves). A right-clicked menu lands at the pointer in window
    // coordinates; a menu opened from the keyboard or the command palette
    // (no pointer origin) drops just under the row's title line, left-aligned
    // to its card — the standard app dropdown position. `deferred` paints it
    // above the rest of the list — same convention as the composer chip
    // pickers. `snap_to_window` keeps it inside the window near the edges.
    row_menu(
        menu.at,
        Corner::TopLeft,
        TextSize::Default.px(&theme) + DynamicSpacing::Base06.px(&theme),
        popup,
    )
}

/// The name a workspace header's hover fade is keyed by. Its row actions
/// read the same transition (`motion::follow`), so both must name it alike.
fn workspace_header_fade(label: &str) -> String {
    format!("workspace-header-{label}")
}

/// The hover-revealed '…' button on a workspace group header. Clicking it
/// opens the header's actions popup; propagation stops so the header's
/// collapse toggle doesn't also fire.
pub(crate) fn workspace_menu_button(
    label: String,
    cwd: PathBuf,
    menu: Option<&WorkspaceMenu>,
    mark: WorkspaceMark,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let this_for_popup = this.clone();
    let label_for_click = label.clone();
    let cwd_for_click = cwd.clone();
    let button = IconButton::new(
        ElementId::Name(format!("workspace-more-{label}").into()),
        "icons/more.svg",
    )
    .size(ButtonSize::Compact)
    .tip(tr!("sidebar.more_actions"))
    .on_mouse_up(&theme, move |_, window, cx| {
        cx.stop_propagation();
        this.update(cx, |app, cx| {
            let (can_move_up, can_move_down) = app.project_move_flags(&label_for_click);
            let menu = WorkspaceMenu {
                label: label_for_click.clone(),
                cwd: cwd_for_click.clone(),
                at: None,
                picker: false,
                can_move_up,
                can_move_down,
                cursor: Default::default(),
            };
            app.toggle_workspace_menu(menu, window, cx);
        });
    })
    .relative();
    // Revealed on row hover (fading in with the header's wash), or held while
    // this header's menu is open.
    crate::motion::follow(
        button,
        workspace_header_fade(&label),
        "workspace-row",
        &theme,
        menu.is_some(),
    )
    .children(menu.map(|menu| {
        div()
            .absolute()
            .top_0()
            .left_0()
            .size(px(0.))
            .child(workspace_menu_popup(
                menu,
                mark.clone(),
                this_for_popup.clone(),
                theme,
            ))
    }))
}

/// One row of the workspace header menu, on the branch selector's picker
/// metrics ([`picker_entry`]): a leading glyph beside the label, with the
/// picker row's inset, padding, height, and radius. The click closes the menu
/// through the app action it runs.
fn workspace_menu_item<C, L>(
    id: &'static str,
    icon_path: &'static str,
    label: L,
    theme: Theme,
    this: Entity<OrbitApp>,
    highlighted: bool,
    on_click: C,
) -> impl IntoElement + use<C, L>
where
    C: Fn(&mut OrbitApp, &mut Window, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let label: SharedString = label.into();
    crate::motion::hover(
        picker_entry(div().id(id), &theme)
            .h(picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                this.update(cx, |app, cx| (on_click)(app, window, cx));
            })
            .child(icon(
                icon_path,
                context_menu::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.text_secondary)
                    .child(label),
            ),
        id,
        &theme,
        Hover::row(&theme).held(highlighted),
    )
}

/// The destructive row of the workspace menu: same metrics as
/// [`workspace_menu_item`], in the error ramp, because “delete this folder”
/// must not look like “copy path”.
fn workspace_menu_item_danger<C, L>(
    id: &'static str,
    icon_path: &'static str,
    label: L,
    theme: Theme,
    this: Entity<OrbitApp>,
    highlighted: bool,
    on_click: C,
) -> impl IntoElement + use<C, L>
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let label: SharedString = label.into();
    crate::motion::hover(
        picker_entry(div().id(id), &theme)
            .h(picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .text_color(theme.error)
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.stop_propagation();
                this.update(cx, |app, cx| (on_click)(app, cx));
            })
            .child(icon(icon_path, context_menu::ICON.px(&theme), theme.error))
            .child(div().flex_1().min_w_0().truncate().child(label)),
        id,
        &theme,
        Hover::fill(theme.error.opacity(0.12), theme.error.opacity(0.18)).held(highlighted),
    )
}

/// One Move up / Move down row of a sidebar menu: the same row the drag
/// performs. Enabled it runs the reorder; at the top or bottom of its list it
/// is painted disabled with the reason, and carries no handler.
#[allow(clippy::too_many_arguments)]
fn move_menu_row<C, L>(
    id: &'static str,
    up: bool,
    enabled: bool,
    label: L,
    reason: impl Into<SharedString>,
    highlighted: bool,
    theme: Theme,
    this: Entity<OrbitApp>,
    on_click: C,
) -> AnyElement
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let icon_path = if up {
        "icons/arrow-up.svg"
    } else {
        "icons/arrow-down.svg"
    };
    if !enabled {
        return menu_item_unavailable(id, icon_path, label, reason.into().to_string(), theme);
    }
    workspace_menu_item(
        id,
        icon_path,
        label,
        theme,
        this,
        highlighted,
        move |app, _, cx| on_click(app, cx),
    )
    .into_any_element()
}

/// Move up / Move down for a project group's row menu.
fn workspace_move_row(
    id: &'static str,
    up: bool,
    enabled: bool,
    menu: &WorkspaceMenu,
    highlighted: bool,
    theme: Theme,
    this: Entity<OrbitApp>,
) -> AnyElement {
    let (label, reason) = if up {
        (
            tr!("sidebar.move_up"),
            tr!("sidebar.move_project_up_blocked"),
        )
    } else {
        (
            tr!("sidebar.move_down"),
            tr!("sidebar.move_project_down_blocked"),
        )
    };
    let menu_label = menu.label.clone();
    move_menu_row(
        id,
        up,
        enabled,
        label,
        reason,
        highlighted,
        theme,
        this,
        move |app, cx| app.on_workspace_move_project(&menu_label, up, cx),
    )
}

/// Move up / Move down for a session row's menu. A session moves inside its
/// project group, so this is the same reorder the row drag performs.
fn session_move_row(
    id: &'static str,
    up: bool,
    enabled: bool,
    menu: &SessionMenu,
    highlighted: bool,
    theme: Theme,
    this: Entity<OrbitApp>,
) -> AnyElement {
    let (label, reason) = if up {
        (
            tr!("sidebar.move_up"),
            tr!("sidebar.move_session_up_blocked"),
        )
    } else {
        (
            tr!("sidebar.move_down"),
            tr!("sidebar.move_session_down_blocked"),
        )
    };
    let path = menu.path.clone();
    move_menu_row(
        id,
        up,
        enabled,
        label,
        reason,
        highlighted,
        theme,
        this,
        move |app, cx| app.on_session_move(&path, up, cx),
    )
}

/// The actions popup anchored to a workspace header: Copy path / Worktrees /
/// Icon & color / Archive sessions / Delete project / Delete project & files,
/// or — once `Icon & color` is chosen — the picker itself, or the
/// delete-from-disk confirmation.
///
/// Uses the branch selector's picker surface and row metrics; deferred and
/// anchored like the session row menu, dismissed by any outside mouse-down.
/// The destructive row leads to a confirmation that names the folder and says
/// whether the platform can undo the delete, so the wording never promises a
/// recovery it cannot deliver.
pub(crate) fn workspace_menu_popup(
    menu: &WorkspaceMenu,
    mark: WorkspaceMark,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let theme = theme.for_floating();
    let lit = lit_row(&menu.cursor, &workspace_menu_items(menu));
    let body: AnyElement = if menu.picker {
        workspace_appearance_picker(menu, mark, this.clone(), theme)
    } else {
        // The action rows wear the branch selector's picker metrics — the
        // same Base04 inset, Base06 inner padding, 31px row, and Base01 row
        // gap — so the workspace menu and the branch/sort popovers read as
        // one family.
        div()
            .py(picker::list_padding_y(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base01.px(&theme))
            // Project order first: the pointer equivalent of dragging the
            // header (issue #143). An edge with nowhere to go is listed
            // disabled and says why, so the row never silently does nothing.
            .child(workspace_move_row(
                "wm-move-up",
                true,
                menu.can_move_up,
                menu,
                lit == Some("wm-move-up"),
                theme,
                this.clone(),
            ))
            .child(workspace_move_row(
                "wm-move-down",
                false,
                menu.can_move_down,
                menu,
                lit == Some("wm-move-down"),
                theme,
                this.clone(),
            ))
            .child(context_menu_separator(&theme))
            .child(workspace_menu_item(
                "wm-copy-path",
                "icons/copy.svg",
                tr!("sidebar.copy_path"),
                theme,
                this.clone(),
                lit == Some("wm-copy-path"),
                |app, _, cx| app.on_workspace_copy_path(cx),
            ))
            .child(workspace_menu_item(
                "wm-worktrees",
                "icons/branch.svg",
                tr!("worktree.page.title"),
                theme,
                this.clone(),
                lit == Some("wm-worktrees"),
                |app, _, cx| {
                    app.workspace_menu = None;
                    app.open_worktrees(cx);
                },
            ))
            .child(workspace_menu_item(
                "wm-rename",
                "icons/pencil.svg",
                tr!("sidebar.rename_project"),
                theme,
                this.clone(),
                lit == Some("wm-rename"),
                |app, window, cx| app.on_workspace_rename(window, cx),
            ))
            .child(workspace_menu_item(
                "wm-icon-color",
                "icons/contrast.svg",
                tr!("sidebar.icon_and_color"),
                theme,
                this.clone(),
                lit == Some("wm-icon-color"),
                |app, _, cx| app.on_workspace_open_picker(cx),
            ))
            .child(workspace_menu_item(
                "wm-archive",
                "icons/archive.svg",
                tr!("sidebar.archive_workspace_sessions"),
                theme,
                this.clone(),
                lit == Some("wm-archive"),
                |app, _, cx| app.on_workspace_archive_sessions(cx),
            ))
            .child(context_menu_separator(&theme))
            .child(workspace_menu_item(
                "wm-delete",
                "icons/minus.svg",
                tr!("sidebar.delete_project"),
                theme,
                this.clone(),
                lit == Some("wm-delete"),
                |app, _, cx| app.on_workspace_delete(cx),
            ))
            .child(workspace_menu_item_danger(
                "wm-delete-files",
                "icons/trash.svg",
                tr!("sidebar.delete_project_and_files"),
                theme,
                this.clone(),
                lit == Some("wm-delete-files"),
                |app, cx| app.on_workspace_delete_files_request(cx),
            ))
            .into_any_element()
    };

    // Same picker surface as the branch selector (and the sidebar's sort
    // menu). One fixed width across both modes means opening the icon picker
    // never resizes the popover.
    let popup = popover_picker(div(), &theme)
        .w(workspace_picker_width(&theme))
        .dismiss_on_outside({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at.arm();
                    app.workspace_menu = None;
                    cx.notify();
                })
            }
        })
        .child(body);

    row_menu(
        menu.at,
        Corner::TopLeft,
        ButtonSize::Compact.height(&theme) + popover::MENU_OFFSET,
        popup,
    )
}

/// A grid cell's side: a square hit target that holds an [`IconSize::Medium`]
/// glyph with even padding all round.
fn workspace_picker_cell(theme: &Theme) -> Pixels {
    DynamicSpacing::Base24.px(theme)
}

/// Spacing between grid cells and tint swatches.
fn workspace_picker_gap(theme: &Theme) -> Pixels {
    DynamicSpacing::Base06.px(theme)
}

/// The inset from the surface edge to the picker's content. Used for the
/// header, swatches, and grid alike so their left and right edges line up.
fn workspace_picker_padding_x(theme: &Theme) -> Pixels {
    DynamicSpacing::Base16.px(theme)
}

/// The width the picker surface needs — the icon grid plus its horizontal
/// padding, so the content fills the popup exactly with no trailing gap.
fn workspace_picker_width(theme: &Theme) -> Pixels {
    let columns = workspace_mark::ICON_COLUMNS as f32;
    workspace_picker_cell(theme) * columns
        + workspace_picker_gap(theme) * (columns - 1.)
        + workspace_picker_padding_x(theme) * 2.
}

/// The icon-and-color picker shown in place of the workspace menu: a row of
/// semantic tint dots over a grid of curated HugeIcons. Picking either writes
/// the mark and persists it; `Reset` clears both back to the folder default.
fn workspace_appearance_picker(
    menu: &WorkspaceMenu,
    mark: WorkspaceMark,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let cell = workspace_picker_cell(&theme);
    let gap = workspace_picker_gap(&theme);

    let mut dots = div().flex().w_full().justify_between();
    for (key, color_of) in workspace_mark::TINTS {
        let selected = mark
            .tint
            .as_deref()
            .map_or(*key == "default", |tint| tint == *key);
        let cwd = menu.cwd.clone();
        let mark = mark.clone();
        let this = this.clone();
        dots = dots.child(
            div()
                .id(ElementId::Name(format!("wm-tint-{key}").into()))
                .size(IconSize::Medium.px(&theme))
                .flex_none()
                .rounded_full()
                .bg(color_of(&theme))
                .border_2()
                .border_color(if selected {
                    theme.text_primary
                } else {
                    theme.border_strong
                })
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    let next = WorkspaceMark {
                        tint: Some((*key).to_string()),
                        ..mark.clone()
                    };
                    this.update(cx, |app, cx| {
                        app.set_workspace_mark(cwd.clone(), next);
                        cx.notify();
                    });
                }),
        );
    }

    let ink = workspace_mark::tint_color(&theme, mark.tint.as_deref());
    let mut grid = div().flex().flex_col().gap(gap);
    for row in workspace_mark::PROJECT_ICONS.chunks(workspace_mark::ICON_COLUMNS) {
        let mut cells = div().flex().gap(gap);
        for stem in row {
            let selected = mark
                .icon
                .as_deref()
                .map_or(*stem == "folder", |icon| icon == *stem);
            let cwd = menu.cwd.clone();
            let mark = mark.clone();
            let this = this.clone();
            cells = cells.child(crate::motion::hover(
                div()
                    .id(ElementId::Name(format!("wm-icon-{stem}").into()))
                    .size(cell)
                    .flex_none()
                    .rounded(list_item::RADIUS.px(&theme))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        cx.stop_propagation();
                        let next = WorkspaceMark {
                            icon: Some((*stem).to_string()),
                            ..mark.clone()
                        };
                        this.update(cx, |app, cx| {
                            app.set_workspace_mark(cwd.clone(), next);
                            cx.notify();
                        });
                    })
                    .child(icon_dyn(
                        workspace_mark::icon_path(Some(stem)),
                        IconSize::Medium.px(&theme),
                        ink,
                    )),
                format!("wm-icon-{stem}"),
                &theme,
                Hover::over(
                    selected.then_some(theme.selected_wash()),
                    theme.hover_wash(),
                ),
            ));
        }
        grid = grid.child(cells);
    }

    let reset_cwd = menu.cwd.clone();
    let this_reset = this.clone();
    let header = div()
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(tr!("sidebar.icon_and_color")),
        )
        .child(crate::motion::hover(
            div()
                .id("wm-mark-reset")
                .flex_none()
                .cursor_pointer()
                .px(DynamicSpacing::Base04.px(&theme))
                .py(DynamicSpacing::Base01.px(&theme))
                .rounded(list_item::RADIUS.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    this_reset.update(cx, |app, cx| {
                        app.set_workspace_mark(reset_cwd.clone(), WorkspaceMark::default());
                        cx.notify();
                    });
                })
                .child(tr!("sidebar.reset")),
            "wm-mark-reset",
            &theme,
            Hover::wash(&theme),
        ));

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base08.px(&theme))
        .px(workspace_picker_padding_x(&theme))
        .py(DynamicSpacing::Base08.px(&theme))
        .child(header)
        .child(dots)
        .child(grid)
        .into_any_element()
}

/// The view control in the sidebar toolbar. Opens the
/// sidebar-options menu anchored below: how sessions are grouped, ordered,
/// and filtered (each active choice checked).
#[allow(clippy::too_many_arguments)]
pub(crate) fn sidebar_options_button(
    group_by: SidebarGroupBy,
    sort: WorkspaceSort,
    archived_filter: SidebarArchivedFilter,
    show_first_message: bool,
    show_updated: bool,
    show_menu: bool,
    naming_layout: bool,
    layout_name: Entity<ComposerInput>,
    open: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> impl IntoElement + use<> {
    let this_for_popup = this.clone();
    IconButton::new("sidebar-options", "icons/sort.svg")
        .size(ButtonSize::Compact)
        .tip(tr!("sidebar.options"))
        .open(open)
        .faded(&theme, move |button| {
            button
                .relative()
                .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                    cx.stop_propagation();
                    this.update(cx, |app, cx| app.toggle_sidebar_sort_menu(window, cx));
                })
                .children(open.then(|| {
                    sidebar_options_popup(
                        group_by,
                        sort,
                        archived_filter,
                        show_first_message,
                        show_updated,
                        show_menu,
                        naming_layout,
                        layout_name,
                        this_for_popup,
                        theme,
                    )
                }))
        })
}

/// One row of the sidebar-options menu, on the picker metrics the branch
/// selector uses: a leading glyph, the label, and a trailing check when the
/// choice is active. The closure is the action the row runs.
fn sidebar_choice_item<C>(
    id: &'static str,
    icon_path: &'static str,
    label: String,
    active: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
    on_click: C,
) -> impl IntoElement
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
{
    let hover_app = this.clone();
    div()
        .id(SharedString::from(format!("choice-hover-{id}")))
        .on_hover(move |hovered: &bool, _, cx| {
            if *hovered && !id.starts_with("sidebar-show-") {
                hover_app.update(cx, |app, cx| {
                    if app.sidebar_show_menu {
                        app.sidebar_show_menu = false;
                        cx.notify();
                    }
                });
            }
        })
        .child(crate::motion::hover(
            picker_entry(div().id(id).debug_selector(move || id.to_string()), &theme)
                .h(picker::entry_height(&theme))
                .flex_none()
                .cursor_pointer()
                .when(active, |row| row.text_color(theme.selection_fg))
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    this.update(cx, |app, cx| on_click(app, cx));
                })
                .child(icon(
                    icon_path,
                    context_menu::ICON.px(&theme),
                    if active {
                        theme.text_primary
                    } else {
                        theme.text_muted
                    },
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if active {
                            theme.text_primary
                        } else {
                            theme.text_secondary
                        })
                        .child(label),
                )
                .when(active, |row| {
                    row.child(icon(
                        "icons/check.svg",
                        context_menu::ICON.px(&theme),
                        theme.accent_text,
                    ))
                }),
            id,
            &theme,
            Hover::over(
                active.then_some(theme.selected_wash()),
                if active {
                    theme.selected_wash()
                } else {
                    theme.hover_wash()
                },
            ),
        ))
}

#[cfg(test)]
#[test]
fn updated_visibility_never_hides_status_labels() {
    for show_updated in [false, true] {
        assert_eq!(
            sidebar_slot_text(Some("Running".into()), "2m".into(), show_updated).as_deref(),
            Some("Running")
        );
    }
    assert_eq!(
        sidebar_slot_text(None, "2m".into(), true).as_deref(),
        Some("2m")
    );
    assert_eq!(sidebar_slot_text(None, "2m".into(), false), None);
}

fn sidebar_slot_text(status: Option<String>, age: String, show_updated: bool) -> Option<String> {
    status.or_else(|| show_updated.then_some(age))
}

fn sidebar_show_row(
    updated: bool,
    summary: bool,
    open: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
    parent_bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
) -> impl IntoElement {
    let hover_app = this.clone();
    let click_app = this.clone();
    picker_entry(
        div()
            .id("sidebar-show")
            .debug_selector(|| "sidebar-show".into()),
        &theme,
    )
    .relative()
    .h(picker::entry_height(&theme))
    .flex_none()
    .cursor_pointer()
    .hover_quiet(&theme)
    .on_hover(move |hovered: &bool, _, cx| {
        if *hovered {
            hover_app.update(cx, |app, cx| {
                if !app.sidebar_show_menu {
                    app.sidebar_show_menu = true;
                    cx.notify();
                }
            });
        }
    })
    .on_mouse_up(MouseButton::Left, move |_, _, cx| {
        cx.stop_propagation();
        click_app.update(cx, |app, cx| {
            app.sidebar_show_menu = true;
            cx.notify();
        });
    })
    .child(icon(
        "icons/eye.svg",
        context_menu::ICON.px(&theme),
        theme.text_muted,
    ))
    .child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_color(theme.text_secondary)
            .child(tr!("sidebar.show")),
    )
    .child(icon(
        "icons/chevron-right.svg",
        context_menu::ICON.px(&theme),
        theme.text_muted,
    ))
    .when(open, |row| {
        row.child(
            div()
                .absolute()
                .top_0()
                .right_0()
                .size(px(0.))
                .child(sidebar_show_popup(
                    updated,
                    summary,
                    this,
                    theme,
                    parent_bounds,
                )),
        )
    })
}

fn sidebar_show_popup(
    updated: bool,
    summary: bool,
    this: Entity<OrbitApp>,
    theme: Theme,
    parent_bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
) -> AnyElement {
    let theme = theme.for_floating();
    let popup = popover_picker(div(), &theme)
        .w(px(220.))
        .dismiss_on_outside({
            let this = this.clone();
            move |event, _, cx| {
                // A click in the parent is outside the flyout, but must reach
                // the parent's mouse-up handler rather than dismissing it.
                if parent_bounds
                    .get()
                    .is_some_and(|bounds| bounds.contains(&event.position))
                {
                    return;
                }
                this.update(cx, |app, cx| {
                    app.menu_dismissed_at.arm();
                    app.close_sidebar_options();
                    cx.notify();
                });
            }
        })
        .child(
            div()
                .py(picker::list_padding_y(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base01.px(&theme))
                .child(sidebar_choice_item(
                    "sidebar-show-updated",
                    "icons/clock.svg",
                    tr!("sidebar.show_updated"),
                    updated,
                    this.clone(),
                    theme,
                    |app, cx| app.toggle_sidebar_updated(cx),
                ))
                .child(sidebar_choice_item(
                    "sidebar-show-summary",
                    "icons/chat.svg",
                    tr!("sidebar.show_summary"),
                    summary,
                    this,
                    theme,
                    |app, cx| app.toggle_sidebar_first_message(cx),
                )),
        );
    // Bespoke: a submenu inside the already-deferred options popup; deferring it
    // again (as anchored_menu does) would paint it apart from its parent.
    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(Corner::TopLeft)
        .offset(point(popover::MENU_OFFSET, px(0.)))
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(popup)
        .into_any_element()
}

/// Width of the sidebar-options popover. Wider than the branch picker's
/// compact width so the long "All conversations (show archived)" row reads
/// without truncating.
const OPTIONS_POPOVER_W: f32 = 300.;

/// The sidebar-options menu, anchored below the header control. Uses the
/// picker surface (search-less) and `picker_entry` rows so it matches the
/// branch selector; an outside mouse-down dismisses it. Grouping, ordering,
/// archive filtering, and the first-message preview preference live here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sidebar_options_popup(
    group_by: SidebarGroupBy,
    sort: WorkspaceSort,
    archived_filter: SidebarArchivedFilter,
    show_first_message: bool,
    show_updated: bool,
    show_menu: bool,
    naming_layout: bool,
    layout_name: Entity<ComposerInput>,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    let theme = theme.for_floating();
    let parent_bounds = Rc::new(Cell::new(None));
    let popup = picker_surface(div(), &theme)
        .relative()
        .w(px(OPTIONS_POPOVER_W))
        .flex()
        .flex_col()
        .child(
            gpui::canvas(
                {
                    let parent_bounds = parent_bounds.clone();
                    move |bounds, _, _| parent_bounds.set(Some(bounds))
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        )
        .dismiss_on_outside({
            let this = this.clone();
            move |_, _, cx| {
                this.update(cx, |app, cx| {
                    if app.sidebar_show_menu {
                        return;
                    }
                    // Arm the gesture guard so this same click's mouse-up
                    // cannot immediately re-open the menu.
                    app.menu_dismissed_at.arm();
                    app.close_sidebar_options();
                    cx.notify();
                })
            }
        })
        .child(
            div()
                .id("sidebar-options-list")
                .py(picker::list_padding_y(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base01.px(&theme))
                .on_hover({
                    let this = this.clone();
                    move |hovered: &bool, _, cx| {
                        if *hovered {
                            this.update(cx, |app, cx| {
                                if app.sidebar_show_menu {
                                    app.sidebar_show_menu = false;
                                    cx.notify();
                                }
                            });
                        }
                    }
                })
                .child(menu_header(tr!("sidebar.group_by"), &theme))
                .children(SidebarGroupBy::ALL.into_iter().map(|option| {
                    sidebar_choice_item(
                        option.as_str(),
                        option.icon_path(),
                        tr!(option.label_key()),
                        option == group_by,
                        this.clone(),
                        theme,
                        move |app, cx| app.set_sidebar_group_by(option, cx),
                    )
                }))
                .child(menu_header(tr!("sidebar.order_by"), &theme))
                .children(WorkspaceSort::MENU.into_iter().map(|option| {
                    sidebar_choice_item(
                        option.as_str(),
                        option.icon_path(),
                        tr!(option.label_key()),
                        option == sort,
                        this.clone(),
                        theme,
                        move |app, cx| app.set_workspace_sort(option, cx),
                    )
                }))
                .child(sidebar_show_row(
                    show_updated,
                    show_first_message,
                    show_menu,
                    this.clone(),
                    theme,
                    parent_bounds,
                ))
                .child(menu_header(tr!("sidebar.filter_sessions"), &theme))
                .children(SidebarArchivedFilter::ALL.into_iter().map(|option| {
                    sidebar_choice_item(
                        option.as_str(),
                        option.icon_path(),
                        tr!(option.label_key()),
                        option == archived_filter,
                        this.clone(),
                        theme,
                        move |app, cx| app.set_sidebar_archived_filter(option, cx),
                    )
                }))
                .child(context_menu_separator(&theme))
                .child(menu_header(tr!("sidebar.layout"), &theme))
                .child(save_layout_row(&this, naming_layout, &layout_name, &theme))
                .children(saved_layout_rows(&this, &theme))
                .children(
                    crate::layout::LayoutPreset::ALL
                        .into_iter()
                        .map(|preset| layout_preset_item(preset, this.clone(), theme)),
                )
                .children(
                    (crate::layout::active_preset().is_some()
                        && crate::layout::custom_layout().is_some())
                    .then(|| restore_custom_item(this.clone(), theme)),
                ),
        );

    // Under the options button it is mounted on.
    crate::ui::popover::dropdown_below(
        crate::ui::popover::DropdownAlign::Start,
        popover::MENU_OFFSET,
        popup,
        "sidebar-options-popover",
        &theme,
    )
    .into_any_element()
}

/// One workspace-shape entry. Unlike the other option rows this one needs the
/// window: opening the terminal spawns a shell and takes focus.
fn layout_preset_item(
    preset: crate::layout::LayoutPreset,
    this: Entity<OrbitApp>,
    theme: Theme,
) -> AnyElement {
    crate::motion::hover(
        picker_entry(div().id(preset.as_str()), &theme)
            .h(picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                this.update(cx, |app, cx| app.apply_layout_preset(preset, window, cx));
            })
            .child(icon(
                preset.icon_path(),
                context_menu::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(tr!(preset.label_key())),
            ),
        preset.as_str(),
        &theme,
        Hover::wash(&theme),
    )
}

/// "Save current layout…" — or the name field it opens. One row either way, so
/// the menu does not jump when the field appears.
fn save_layout_row(
    this: &Entity<OrbitApp>,
    naming: bool,
    layout_name: &Entity<ComposerInput>,
    theme: &Theme,
) -> AnyElement {
    if !naming {
        let this = this.clone();
        let theme = *theme;
        return crate::motion::hover(
            picker_entry(div().id("sidebar-save-layout"), &theme)
                .h(picker::entry_height(&theme))
                .flex_none()
                .cursor_pointer()
                .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    cx.stop_propagation();
                    this.update(cx, |app, cx| app.begin_naming_layout(cx));
                })
                .child(icon(
                    "icons/plus.svg",
                    context_menu::ICON.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(tr!("sidebar.layout_save")),
                ),
            "sidebar-save-layout",
            &theme,
            Hover::wash(&theme),
        );
    }

    let input = layout_name.clone();
    let for_cancel = this.clone();
    let for_save = this.clone();
    let save_theme = *theme;
    picker_entry(div().id("sidebar-save-layout-field"), theme)
        .h(picker::entry_height(theme))
        .flex_none()
        .gap_1()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(div().flex_1().min_w_0().child(input))
        .child(
            IconButton::new("layout-name-cancel", "icons/x.svg")
                .size(ButtonSize::Compact)
                .faded(theme, |el| {
                    el.on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        for_cancel.update(cx, |app, cx| app.cancel_naming_layout(window, cx));
                    })
                }),
        )
        .child(
            IconButton::new("layout-name-save", "icons/check.svg")
                .size(ButtonSize::Compact)
                .ink(save_theme.accent_text)
                .faded(theme, |el| {
                    el.on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        for_save.update(cx, |app, cx| app.save_named_layout(window, cx));
                    })
                }),
        )
        .into_any_element()
}

/// The saved arrangements, one row each: apply on click, delete on the x.
fn saved_layout_rows(this: &Entity<OrbitApp>, theme: &Theme) -> Vec<AnyElement> {
    crate::layout::named_layouts()
        .into_iter()
        .map(|named| {
            let name = named.name.clone();
            let for_apply = this.clone();
            let for_delete = this.clone();
            let delete_name = named.name.clone();
            let row_id = format!("sidebar-layout-saved-{}", named.name);
            crate::motion::hover(
                picker_entry(div().id(ElementId::Name(row_id.clone().into())), theme)
                    .h(picker::entry_height(theme))
                    .flex_none()
                    .cursor_pointer()
                    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        let name = name.clone();
                        for_apply.update(cx, |app, cx| app.apply_named_layout(name, window, cx));
                    })
                    .child(icon(
                        "icons/layout-left.svg",
                        context_menu::ICON.px(theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(named.name.clone()),
                    )
                    .child(
                        IconButton::new(
                            ElementId::Name(format!("layout-delete-{}", named.name).into()),
                            "icons/x.svg",
                        )
                        .size(ButtonSize::Compact)
                        .tip(tr!("sidebar.layout_delete"))
                        .faded(theme, |el| {
                            el.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                                cx.stop_propagation();
                                let name = delete_name.clone();
                                for_delete.update(cx, |app, cx| app.delete_named_layout(name, cx));
                            })
                        }),
                    ),
                row_id,
                theme,
                Hover::wash(theme),
            )
            .into_any_element()
        })
        .collect()
}

/// "Back to custom": shown only while a preset is active, and only when the
/// arrangement it replaced is still saved.
fn restore_custom_item(this: Entity<OrbitApp>, theme: Theme) -> AnyElement {
    crate::motion::hover(
        picker_entry(div().id("sidebar-layout-custom"), &theme)
            .h(picker::entry_height(&theme))
            .flex_none()
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                cx.stop_propagation();
                this.update(cx, |app, cx| app.restore_custom_layout(window, cx));
            })
            .child(icon(
                "icons/rotate-ccw.svg",
                context_menu::ICON.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(tr!("sidebar.layout_custom")),
            ),
        "sidebar-layout-custom",
        &theme,
        Hover::wash(&theme),
    )
}

/// One entry of the sidebar's session / workspace menus, on Zed's context
/// menu metrics ([`context_menu_entry`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn menu_item<C, L>(
    id: &'static str,
    icon_path: &'static str,
    label: L,
    theme: Theme,
    this: Entity<OrbitApp>,
    danger: bool,
    highlighted: bool,
    on_click: C,
) -> impl IntoElement + use<C, L>
where
    C: Fn(&mut OrbitApp, &mut Context<OrbitApp>) + 'static,
    L: Into<SharedString>,
{
    let label: SharedString = label.into();
    let on_click = on_click;
    let (hover_bg, text_color, icon_color) = if danger {
        (theme.error_hover, theme.error_fg, theme.error_fg)
    } else {
        (theme.overlay, theme.text_secondary, theme.text_muted)
    };
    let entry = picker_entry(div().id(id), &theme)
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_pointer()
        .when(danger, |s| s.bg(theme.error))
        .text_color(text_color)
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            cx.stop_propagation();
            this.update(cx, |app, cx| (on_click)(app, cx));
        })
        .child(icon(icon_path, context_menu::ICON.px(&theme), icon_color))
        .child(div().flex_1().min_w_0().truncate().child(label.to_string()));
    // The entry floats over a menu surface, so its hover is the glass wash
    // rather than `surface_hover`; a destructive entry steps its own error ramp
    // instead of washing a red row in grey.
    let hover = if danger {
        Hover::fill(theme.error, hover_bg)
    } else {
        Hover::wash_in(hover_bg)
    }
    .held(highlighted);
    crate::motion::hover(press(entry), id, &theme, hover)
}

/// A menu row for a command that exists but cannot run right now: same
/// metrics as [`menu_item`], muted, with the reason on hover. A row that
/// silently disappears reads as a missing feature, so a blocked command says
/// so instead (#131).
fn menu_item_unavailable(
    id: &'static str,
    icon_path: &'static str,
    label: impl Into<SharedString>,
    reason: String,
    theme: Theme,
) -> AnyElement {
    picker_entry(div().id(id), &theme)
        .h(picker::entry_height(&theme))
        .flex_none()
        .cursor_not_allowed()
        .text_color(theme.text_disabled)
        .child(icon(
            icon_path,
            context_menu::ICON.px(&theme),
            theme.text_disabled,
        ))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .tip(reason)
        .into_any_element()
}

/// Empty-state panel for a fresh pi store: what this space is for and how
/// to fill it. No cards, no illustration — one calm statement.
pub(crate) fn empty_sessions_state(
    theme: Theme,
    this: Entity<OrbitApp>,
) -> impl IntoElement + use<> {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(DynamicSpacing::Base06.px(&theme))
        .px(DynamicSpacing::Base20.px(&theme))
        .pb(DynamicSpacing::Base40.px(&theme))
        .child(
            div()
                .size(px(36.))
                .rounded_full()
                .bg(theme.control_face(theme.surface_elevated))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(
                    "icons/spark.svg",
                    IconSize::Medium.px(&theme),
                    theme.accent_text,
                )),
        )
        .child(
            div()
                .text_size(TextSize::Default.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(tr!("sidebar.no_projects_yet")),
        )
        .child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .text_align(TextAlign::Center)
                .child(tr!("sidebar.pick_a_folder_to_start_your_first_task")),
        )
        // The empty state offers the action that resolves it. A pane that says
        // "pick a folder" without a way to do it is a dead end.
        .child(
            Button::new("empty-sessions-new", ButtonVariant::Primary)
                .label(tr!("sidebar.new_session"))
                .faded(&theme, move |button| {
                    button.mt(DynamicSpacing::Base06.px(&theme)).on_mouse_up(
                        MouseButton::Left,
                        move |_, window, cx| {
                            this.update(cx, |app, cx| {
                                app.on_new_session(&crate::NewSession, window, cx)
                            });
                        },
                    )
                }),
        )
}

// ── controller ────────────────────────────────────────────────────
impl OrbitApp {
    fn close_sidebar_options(&mut self) {
        self.sidebar_sort_menu = false;
        self.sidebar_show_menu = false;
    }

    /// Drop the hover card and cancel a timer that has not fired yet.
    pub(super) fn clear_session_card(&mut self, cx: &mut Context<Self>) {
        self.session_card_wait = None;
        self.session_card_popup_hover = None;
        self.session_card_task = gpui::Task::ready(());
        // A card that comes back — the same row, or the next one under the
        // cursor — opens at the top rather than wherever the last one was left.
        self.session_card_scroll.set_offset(point(px(0.), px(0.)));
        if self.session_card.take().is_some() {
            cx.notify();
        }
    }

    /// Show the session hover card after the tooltip delay, pinned to that
    /// row. A short leave grace lets the pointer cross the gap into the
    /// scrollable card; entering either surface cancels pending dismissal.
    pub(super) fn hover_session_card(
        &mut self,
        path: PathBuf,
        hovered: bool,
        cx: &mut Context<Self>,
    ) {
        if hovered {
            if self.session_menu.is_some()
                || self.workspace_menu.is_some()
                || self.sidebar_sort_menu
                || self.sidebar_show_menu
            {
                return;
            }
            if self.session_card.as_ref() == Some(&path) {
                self.session_card_task = gpui::Task::ready(());
                self.session_card_wait = Some(path);
                return;
            }
            if self.session_card_wait.as_ref() == Some(&path) {
                return;
            }
            self.session_card_wait = Some(path.clone());
            if self.session_card.as_ref() != Some(&path) && self.session_card.take().is_some() {
                cx.notify();
            }
            self.session_card_task = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SESSION_CARD_DELAY).await;
                let _ = this.update(cx, |app, cx| {
                    if app.session_card_wait.as_ref() == Some(&path) {
                        app.session_card = Some(path);
                        cx.notify();
                    }
                });
            });
        } else if self.session_card_wait.as_ref() == Some(&path) {
            self.session_card_wait = None;
            self.session_card_task = gpui::Task::ready(());
            self.dismiss_unhovered_session_card(path, cx);
        }
    }

    /// Popup and row callbacks arrive in reverse paint order, not leave/enter
    /// order. Never let one surface's leave revoke the other's hover ownership.
    fn hover_session_card_popup(&mut self, path: PathBuf, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            // Ignore stale popup events after menu/keyboard suppression cleared it.
            if self.session_card.as_ref() == Some(&path) {
                self.session_card_popup_hover = Some(path);
                self.session_card_task = gpui::Task::ready(());
            }
        } else if self.session_card_popup_hover.as_ref() == Some(&path) {
            self.session_card_popup_hover = None;
            self.dismiss_unhovered_session_card(path, cx);
        }
    }

    fn dismiss_unhovered_session_card(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.session_card.as_ref() == Some(&path)
            && self.session_card_wait.as_ref() != Some(&path)
            && self.session_card_popup_hover.as_ref() != Some(&path)
        {
            self.session_card_task = cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(200))
                    .await;
                let _ = this.update(cx, |app, cx| app.clear_session_card(cx));
            });
        }
    }

    /// Open the session row menu from a right-click, floating it at the
    /// pointer ([`SessionMenu::at`]). Always (re)positions the menu — a
    /// right-click while another row's menu is up moves it to the new row
    /// instead of dismissing.
    pub(super) fn open_session_menu_at(
        &mut self,
        menu: SessionMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_transient_popups(window, cx);
        self.workspace_menu = None;
        self.clear_session_card(cx);
        self.session_menu = Some(menu);
        cx.notify();
    }

    /// Dismiss window-level overlays (palette, pickers, popup selectors) so a
    /// row menu never opens underneath one — every open path above routes
    /// through here.
    fn dismiss_transient_popups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_session_card(cx);
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
        self.close_sidebar_options();
    }

    pub(super) fn on_menu_rename_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.session_menu.take() else {
            return;
        };
        if self.current_session_path.as_ref() == Some(&menu.path) && self.session_id.is_some() {
            self.begin_session_rename(window, cx);
            cx.notify();
            return;
        }
        let Some(session) = self
            .sessions
            .iter()
            .find(|session| session.path == menu.path)
            .cloned()
        else {
            cx.notify();
            return;
        };
        self.open_session_rename(Some(session), window, cx);
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
                self.prune_workflow_store();
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

    /// Archive or unarchive the session whose row menu is open, then re-run
    /// the archived filter so the row appears or leaves per the view choice.
    pub(super) fn on_menu_toggle_archive(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.session_menu.take() {
            crate::archive::toggle(&menu.path);
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// First Delete click: close the menu and ask through the confirm
    /// prompt, which names the session and takes the keyboard.
    pub(super) fn on_menu_delete_request(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.session_menu.take() else {
            return;
        };
        let cwd = self.session_delete_workspace(&menu.path);
        self.request_confirm(
            super::confirm_ui::PendingConfirm::DeleteSession {
                path: menu.path,
                title: menu.title,
                cwd,
            },
            cx,
        );
    }

    /// The workspace from the session record, or the known live workspace
    /// for a local session that has not appeared on disk yet. The sidebar's
    /// placeholder row can fall back to Orbit's process cwd; don't show that
    /// guess in a destructive confirmation.
    fn session_delete_workspace(&self, path: &Path) -> Option<PathBuf> {
        self.sessions
            .iter()
            .find(|session| session.path == path)
            .map(|session| session.cwd.clone())
            .or_else(|| {
                if self.remote.is_none() && self.current_session_path.as_deref() == Some(path) {
                    self.current_workspace.clone()
                } else {
                    None
                }
            })
    }

    /// Whether Delete must wait: the session has a run in flight, open or
    /// parked. Deleting it then would abort work the user may still want,
    /// and the process would write the file straight back.
    pub(super) fn session_delete_blocked(&self, path: &Path) -> bool {
        let open_and_busy = self.current_session_path.as_deref() == Some(path)
            && (self.busy
                || self.transcript.is_streaming()
                || self.retrying
                || self.is_compacting
                || self.execution.status().is_in_progress());
        open_and_busy || self.lives.get(path).is_some_and(|parked| parked.busy)
    }

    /// Confirmed: remove the session file from disk and refresh the list.
    ///
    /// An idle session may still have a pi process — the open one, or a warm
    /// parked one. That process would write the file back, so it is stopped
    /// first: the open session hands the window to a new task (which parks
    /// it), and the parked process is dropped, which kills it (#166).
    pub(super) fn delete_session_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_delete_blocked(&path) {
            self.toast_warning(tr!("sidebar.live_process_delete"));
            cx.notify();
            return;
        }
        if self.current_session_path.as_ref() == Some(&path) {
            let cwd = self
                .current_workspace
                .clone()
                .unwrap_or_else(|| PathBuf::from("."));
            self.begin_new_task(cwd, window, cx);
            // The switch can be refused (a rewind or an account switch in
            // flight); the session is still open, so leave its file alone.
            if self.current_session_path.as_ref() == Some(&path) {
                cx.notify();
                return;
            }
        }
        // Dropping the parked client kills its process.
        self.lives.remove(&path);
        if let Err(err) = fs::remove_file(&path) {
            self.toast_error(tr!("sidebar.delete_failed", error = err));
        } else {
            // A deleted session must not leave a stale pin behind.
            crate::pins::remove(&path);
            crate::archive::remove(&path);
            // Nor a review mark for a row that can never come back.
            self.awaiting_review.remove(&path);
            self.session_outcomes.remove(&path);
        }
        self.sessions = sessions::load_sessions();
        self.prune_workflow_store();
        cx.notify();
    }

    /// Open (or toggle closed) the sidebar's workspace-sort menu. Follows the
    /// same gesture guard as the row menus, so the dismissing click cannot
    /// immediately re-open it.
    pub(super) fn toggle_sidebar_sort_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_dismissed_at.swallows() {
            return;
        }
        if self.sidebar_sort_menu {
            self.close_sidebar_options();
            cx.notify();
            return;
        }
        self.dismiss_transient_popups(window, cx);
        self.session_menu = None;
        self.workspace_menu = None;
        self.sidebar_sort_menu = true;
        cx.notify();
    }

    /// Switch the workspace ordering and persist the choice with the project
    /// list. Re-sorting is pure UI: pi's session files are never touched.
    pub(super) fn set_workspace_sort(&mut self, sort: WorkspaceSort, cx: &mut Context<Self>) {
        self.close_sidebar_options();
        if self.workspace_sort != sort {
            self.workspace_sort = sort;
            self.persist_workspace_prefs();
        }
        cx.notify();
    }

    /// Choose how the sidebar arranges sessions, persisting the choice beside
    /// the project list. Regrouping is pure UI: pi's session files are never
    /// touched.
    /// Apply a named workspace shape. The arrangement the user built is saved
    /// first, so "Back to custom" can bring it back.
    pub(super) fn apply_layout_preset(
        &mut self,
        preset: crate::layout::LayoutPreset,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(
            preset.shape().is_storable(),
            "{preset:?} would persist a size the loader drops"
        );
        crate::layout::apply_preset(preset);
        self.apply_stored_arrangement(window, cx);
        self.close_sidebar_options();
        cx.notify();
    }

    /// Open the name field for "save this arrangement".
    pub(super) fn begin_naming_layout(&mut self, cx: &mut Context<Self>) {
        self.layout_name.update(cx, |input, cx| input.clear(cx));
        self.naming_layout = true;
        cx.notify();
    }

    pub(super) fn cancel_naming_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.naming_layout = false;
        // Hand focus back to the composer: the menu is gone, so the caret must
        // not stay in a field nobody can see.
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// Save the live arrangement under the typed name. A blank name is refused
    /// rather than saved as "Untitled": an unnamed layout is not a layout.
    pub(super) fn save_named_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.layout_name.read(cx).text().trim().to_owned();
        if name.is_empty() {
            return;
        }
        if crate::layout::save_named(&name) {
            self.layout_name.update(cx, |input, cx| input.clear(cx));
            self.naming_layout = false;
            self.input.read(cx).focus(window);
            cx.notify();
        }
    }

    pub(super) fn apply_named_layout(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if crate::layout::apply_named(&name) {
            self.apply_stored_arrangement(window, cx);
            self.close_sidebar_options();
            cx.notify();
        }
    }

    pub(super) fn delete_named_layout(&mut self, name: String, cx: &mut Context<Self>) {
        if crate::layout::delete_named(&name) {
            cx.notify();
        }
    }

    /// Focus the sidebar's session search, revealing the sidebar first.
    pub(super) fn focus_session_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar_visible = true;
        self.record_arrangement(cx);
        self.sidebar_search.read(cx).focus(window);
        cx.notify();
    }

    pub(super) fn on_focus_session_search(
        &mut self,
        _: &crate::FocusSessionSearch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_session_search(window, cx);
    }

    /// Bring back the arrangement a preset replaced. A no-op when there is none.
    pub(super) fn restore_custom_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if crate::layout::restore_custom() {
            self.apply_stored_arrangement(window, cx);
            cx.notify();
        }
    }

    /// Push the stored arrangement onto the live panels. Idempotent: it is used
    /// at launch (terminal included, which is why it needs the window) and
    /// after applying or reverting a preset.
    pub(super) fn apply_stored_arrangement(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let arrangement = crate::layout::arrangement();
        let layout = &arrangement.layout;

        if let Some(width) = layout.sidebar_width {
            self.sidebar_width = px(width);
        }
        if let Some(visible) = layout.sidebar_visible {
            self.sidebar_visible = visible;
        }
        if let Some(width) = layout.explorer_width {
            self.project_panel
                .update(cx, |panel, cx| panel.set_width(px(width), cx));
        }
        if let Some(width) = layout.sidepane_width {
            self.sidepane
                .update(cx, |pane, cx| pane.set_width(px(width), cx));
        }
        if let Some(width) = layout.github_list_width {
            self.git_panel
                .update(cx, |panel, cx| panel.set_github_list_width(px(width), cx));
        }
        if let Some(height) = layout.terminal_height {
            self.terminal_panel
                .update(cx, |panel, cx| panel.set_height(px(height), cx));
        }

        if let Some(open) = layout.project_panel {
            if self.project_panel.read(cx).is_open() != open {
                self.project_panel.update(cx, |panel, cx| panel.toggle(cx));
            }
        }
        if let Some(open) = layout.side_pane {
            if self.sidepane.read(cx).is_open() != open {
                self.sidepane.update(cx, |pane, cx| pane.toggle(cx));
            }
        }
        if let Some(open) = layout.terminal_panel {
            if self.terminal_panel.read(cx).is_open() != open {
                self.terminal_panel
                    .update(cx, |panel, cx| panel.toggle(window, cx));
            }
        }
        self.collapsed_workspaces = layout.collapsed_workspaces.iter().cloned().collect();
        self.expanded_session_groups = layout.expanded_groups.iter().cloned().collect();
        // No notify here: the launch path runs inside `render`, where this
        // frame already reflects the change. Menu callers notify themselves.
    }

    /// Record how far each group has been expanded past its cap, so the list
    /// reopens the way it was left.
    pub(super) fn record_expanded_groups(&self) {
        crate::layout::set_expanded_groups(
            self.expanded_session_groups
                .iter()
                .map(|(label, extra)| (label.clone(), *extra))
                .collect(),
        );
    }

    /// Record which workspace groups are collapsed. A collapse is arrangement,
    /// not data: it must survive a restart the way a panel size does.
    pub(super) fn record_collapsed_groups(&self) {
        crate::layout::set_collapsed_workspaces(
            self.collapsed_workspaces.iter().cloned().collect(),
        );
    }

    /// Record the open panels after any toggle, so the next launch lands in the
    /// same shape. Cheap when nothing changed — the store compares first.
    pub(super) fn record_arrangement(&mut self, cx: &App) {
        crate::layout::set_open_state(
            self.sidebar_visible,
            self.project_panel.read(cx).is_open(),
            self.sidepane.read(cx).is_open(),
            self.terminal_panel.read(cx).is_open(),
        );
    }

    pub(super) fn set_sidebar_group_by(
        &mut self,
        group_by: SidebarGroupBy,
        cx: &mut Context<Self>,
    ) {
        self.close_sidebar_options();
        if self.sidebar_group_by != group_by {
            self.sidebar_group_by = group_by;
            self.persist_workspace_prefs();
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// Change preview visibility without touching the underlying session content.
    pub(super) fn toggle_sidebar_first_message(&mut self, cx: &mut Context<Self>) {
        self.sidebar_show_first_message = !self.sidebar_show_first_message;
        self.persist_workspace_prefs();
        // Invalidate cached heights while keeping the reader on the same row.
        let anchor = self.sidebar_list.logical_scroll_top();
        let count = self.sidebar_list.item_count();
        self.sidebar_list.splice(0..count, count);
        self.sidebar_list.scroll_to(anchor);
        cx.notify();
    }

    pub(super) fn toggle_sidebar_updated(&mut self, cx: &mut Context<Self>) {
        self.sidebar_show_updated = !self.sidebar_show_updated;
        self.persist_workspace_prefs();
        cx.notify();
    }

    /// Choose which archived sessions the sidebar lists and persist it.
    pub(super) fn set_sidebar_archived_filter(
        &mut self,
        filter: SidebarArchivedFilter,
        cx: &mut Context<Self>,
    ) {
        self.close_sidebar_options();
        if self.sidebar_archived_filter != filter {
            self.sidebar_archived_filter = filter;
            self.persist_workspace_prefs();
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// Open (or toggle closed) the row-actions popup for a workspace header.
    pub(super) fn toggle_workspace_menu(
        &mut self,
        menu: WorkspaceMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Same-click dismissal must not re-open (see toggle_workspace_menu).
        if self.menu_dismissed_at.swallows() {
            return;
        }
        if self.workspace_menu.as_ref() == Some(&menu) {
            self.workspace_menu = None;
            cx.notify();
            return;
        }
        self.dismiss_transient_popups(window, cx);
        self.session_menu = None;
        self.workspace_menu = Some(menu);
        cx.notify();
    }

    /// Open the workspace header menu from a right-click, floating it at the
    /// pointer ([`WorkspaceMenu::at`]). See [`Self::open_session_menu_at`] for
    /// why this bypasses the toggle's gesture guard and equality check.
    pub(super) fn open_workspace_menu_at(
        &mut self,
        menu: WorkspaceMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_transient_popups(window, cx);
        self.session_menu = None;
        self.workspace_menu = Some(menu);
        cx.notify();
    }

    /// Move the menu's session one place up (`up`) or down inside its project,
    /// then close the menu. Same path as ⌥↑/⌥↓ and a row drag.
    pub(super) fn on_session_move(&mut self, path: &Path, up: bool, cx: &mut Context<Self>) {
        self.session_menu = None;
        self.nudge_session(path, if up { -1 } else { 1 }, cx);
        cx.notify();
    }

    /// Move the menu's project group one place up (`up`) or down, then close
    /// the menu. Same path as ⌥↑/⌥↓ and the drag drop.
    pub(super) fn on_workspace_move_project(
        &mut self,
        label: &str,
        up: bool,
        cx: &mut Context<Self>,
    ) {
        self.workspace_menu = None;
        self.nudge_project_group(label, if up { -1 } else { 1 }, cx);
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

    /// Swap the open workspace menu for its icon/tint picker, in place.
    pub(super) fn on_workspace_open_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(menu) = self.workspace_menu.as_mut() {
            menu.picker = true;
        }
        cx.notify();
    }

    /// Archive every session in the workspace the open header menu belongs
    /// to (#131). Archiving is Orbit-owned and reversible — pi's files are
    /// untouched, and the sidebar's archived view brings each one back — so
    /// it needs no confirmation; the toast says how many moved.
    pub(super) fn on_workspace_archive_sessions(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.workspace_menu.take() else {
            return;
        };
        let paths: Vec<PathBuf> = self
            .sidebar_sessions()
            .into_iter()
            .filter(|session| sessions::workspace_label(&session.cwd) == menu.label)
            .map(|session| session.path)
            .collect();
        let archived = crate::archive::add_all(&paths);
        if archived == 0 {
            self.toast_info(tr!("sidebar.archive_workspace_nothing"));
        } else {
            self.toast_success(tr!(
                "sidebar.archive_workspace_done",
                count = archived,
                workspace = menu.label
            ));
        }
        self.clamp_sidebar_cursor(cx);
        cx.notify();
    }

    /// **Delete project**: drop the workspace from Orbit's own project list.
    /// pi's session files are never touched — the folder can be added again by
    /// picking it to work in.
    pub(super) fn on_workspace_delete(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.workspace_menu.take() else {
            return;
        };
        self.remove_workspace(&menu.cwd, cx);
        // Forget the dropped group's view state so re-adding it starts fresh.
        self.collapsed_workspaces.remove(&menu.label);
        self.expanded_workspace_groups.remove(&menu.label);
        self.expanded_session_groups.remove(&menu.label);
        cx.notify();
    }

    /// **Delete project & files**: arm the confirmation. The delete itself is
    /// irreversible on every platform Orbit ships, so it gets its own step that
    /// names the folder.
    pub(super) fn on_workspace_delete_files_request(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.workspace_menu.take() else {
            return;
        };
        self.request_confirm(
            super::confirm_ui::PendingConfirm::DeleteWorkspaceFiles {
                cwd: menu.cwd,
                label: menu.label,
            },
            cx,
        );
    }

    /// Confirmed: remove the folder from disk, then drop the project row.
    ///
    /// The guards in [`project_delete_refusal`] run first and refuse loudly
    /// (a root, the home directory, a parent of other projects, a folder a live
    /// process is working in). The removal runs off the UI thread; the row only
    /// leaves the sidebar once the folder is actually gone, so a failure leaves
    /// the project where the user can retry it.
    pub(super) fn delete_workspace_files(
        &mut self,
        cwd: PathBuf,
        label: String,
        cx: &mut Context<Self>,
    ) {
        let menu = WorkspaceMenu {
            label,
            cwd,
            at: None,
            picker: false,
            can_move_up: false,
            can_move_down: false,
            cursor: Default::default(),
        };
        let live = self.live_session_paths();
        if let Some(refusal) = project_delete_refusal(&menu.cwd, &self.sessions, &live) {
            self.toast_warning(refusal.reason());
            cx.notify();
            return;
        }
        let target = menu.cwd.clone();
        self.collapsed_workspaces.remove(&menu.label);
        self.expanded_workspace_groups.remove(&menu.label);
        self.expanded_session_groups.remove(&menu.label);
        cx.spawn(async move |this, cx| {
            let trash = target.clone();
            let result = cx
                .background_spawn(async move { crate::platform::trash_path(&trash) })
                .await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(()) => {
                        app.remove_workspace(&target, cx);
                        app.toast_success(tr!(
                            "sidebar.project_deleted",
                            name = menu.label.clone()
                        ));
                    }
                    Err(error) => app.toast_error(tr!(
                        "sidebar.project_delete_failed",
                        error = error.to_string()
                    )),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// The session files pi currently has open: the active process plus every
    /// parked one. The delete-from-disk guard reads this so a folder some pi is
    /// still writing into cannot be removed underneath it.
    fn live_session_paths(&self) -> HashSet<PathBuf> {
        let mut live: HashSet<PathBuf> = self.lives.keys().cloned().collect();
        if self.runtime_is_live() {
            if let Some(path) = &self.current_session_path {
                live.insert(path.clone());
            }
        }
        live
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

    // ── keyboard navigation ───────────────────────────────────────────

    /// The sidebar rows as currently laid out — grouped by workspace, or one
    /// flat list when grouping is off. Shared by the render path and keyboard
    /// navigation so the cursor can never point at a row the user cannot see.
    pub(super) fn sidebar_rows(
        &self,
        sessions: &[SessionInfo],
        pinned: &HashSet<PathBuf>,
    ) -> Vec<SideRow> {
        let working = self.workspace_label();
        let running: HashSet<PathBuf> = self
            .lives
            .iter()
            .filter(|(_, parked)| parked.busy)
            .map(|(path, _)| path.clone())
            .collect();
        // A marked session is unread news; collapsing a group must not hide it
        // (issue #83). The flat list has no collapse, so it needs no set.
        let reviewed: HashSet<PathBuf> = self.awaiting_review.keys().cloned().collect();
        // Arranged session order, read once per build (a snapshot of a small
        // Orbit-owned store; never pi's files).
        let session_ranks: HashMap<PathBuf, usize> = crate::session_order::ranks();
        let mut rows = match self.sidebar_group_by {
            // A search result set is one flat list: the query already decided
            // what is visible, and a group header per single match is noise.
            _ if !self.sidebar_query.is_empty() => {
                build_flat_sidebar_rows(sessions, pinned, &self.current_session_path, &running)
            }
            SidebarGroupBy::OneList => {
                build_flat_sidebar_rows(sessions, pinned, &self.current_session_path, &running)
            }
            SidebarGroupBy::Recent => {
                build_recent_sidebar_rows(sessions, pinned, &self.current_session_path, &running)
            }
            SidebarGroupBy::Date => build_date_sidebar_rows(sessions, pinned, SystemTime::now()),
            SidebarGroupBy::Workspace => build_sidebar_rows(
                sessions,
                &self.workspaces,
                self.workspace_sort,
                &self.workspace_added_at,
                &working,
                &self.collapsed_workspaces,
                &self.expanded_workspace_groups,
                &self.expanded_session_groups,
                pinned,
                &self.current_session_path,
                &running,
                &reviewed,
                &session_ranks,
            ),
        };
        // The servers' sessions follow this Mac's, one group per remote host
        // entry (`remote_sessions_ui.rs`); grouped by project, an entry linked
        // to a listed project lists inside that project's group. A search
        // filters local sessions only, so they step aside while one is typed.
        if self.sidebar_query.is_empty() {
            if self.sidebar_group_by == SidebarGroupBy::Workspace {
                self.place_remote_session_rows(&mut rows);
            } else {
                rows.extend(self.remote_session_rows());
            }
        }
        rows
    }

    /// The same filtered session snapshot used by rendering and navigation.
    pub(super) fn sidebar_sessions_filtered(&self) -> Vec<SessionInfo> {
        filter_sidebar_sessions(
            self.sidebar_sessions(),
            self.sidebar_archived_filter,
            &crate::archive::all().paths(),
            &self.sidebar_query,
        )
    }

    /// The sidebar rows with the pinned snapshot read fresh — the same
    /// grouping, ordering, and collapse state the render path uses.
    fn sidebar_rows_for_nav(&self) -> Vec<SideRow> {
        let sessions = self.sidebar_sessions_filtered();
        let pinned: HashSet<PathBuf> = crate::pins::all().paths();
        self.sidebar_rows(&sessions, &pinned)
    }

    // ── project reorder: drag & drop, menu, keyboard (issue #143) ──────

    /// The project groups in the order the sidebar currently shows them. A
    /// move is expressed against this order — never against the stored list —
    /// so a drag or Move up/down always does what the screen showed, whatever
    /// sort mode produced that order.
    pub(super) fn project_group_order(&self) -> Vec<String> {
        let sessions = self.sidebar_sessions_filtered();
        sidebar_groups(
            &sessions,
            &self.workspaces,
            self.workspace_sort,
            &self.workspace_added_at,
            &crate::pins::all().paths(),
            &crate::session_order::ranks(),
        )
        .into_iter()
        .map(|(label, _, _)| label)
        .collect()
    }

    /// Whether the project group has a neighbour above / below in the
    /// displayed order — the menu's Move up / Move down enabled state.
    pub(super) fn project_move_flags(&self, label: &str) -> (bool, bool) {
        let order = self.project_group_order();
        match order.iter().position(|candidate| candidate == label) {
            Some(ix) => (ix > 0, ix + 1 < order.len()),
            None => (false, false),
        }
    }

    /// Drag tracking for project groups: resolve which header the pointer is
    /// over from the list's own measured row bounds, and which edge of it.
    /// Nothing is stored on disk here — the order only changes on a drop.
    pub(super) fn on_project_drag_move(
        &mut self,
        event: &DragMoveEvent<ProjectDrag>,
        cx: &mut Context<Self>,
    ) {
        let moving = event.drag(cx).label.clone();
        let pointer = event.event.position;
        let target =
            self.sidebar_rows_for_nav()
                .iter()
                .enumerate()
                .find_map(|(ix, row)| match row {
                    // The group being dragged is not its own target, and neither
                    // is any row inside a group: a project header is the entry.
                    SideRow::Workspace { label, .. } if *label != moving => self
                        .sidebar_list
                        .bounds_for_item(ix)
                        .filter(|bounds| bounds.contains(&pointer))
                        .map(|bounds| ProjectDropTarget {
                            label: label.clone(),
                            after: pointer.y > bounds.origin.y + bounds.size.height / 2.,
                        }),
                    _ => None,
                });
        let next = Some(ProjectDragState {
            dragging: moving,
            target,
        });
        if self.project_drag != next {
            self.project_drag = next;
            cx.notify();
        }
    }

    /// A project header was released. The landing spot was decided while the
    /// pointer moved; a release with no target (over a session row, or outside
    /// the list) changes nothing.
    pub(super) fn on_project_drop(&mut self, drag: &ProjectDrag, cx: &mut Context<Self>) {
        let target = self
            .project_drag
            .take()
            .filter(|state| state.dragging == drag.label)
            .and_then(|state| state.target);
        match target {
            Some(target) => self.move_project_group(&drag.label, &target.label, target.after, cx),
            None => cx.notify(),
        }
    }

    /// Move a project group before (or after) another one, and persist the
    /// order. This is the one place the sidebar's project order is written:
    /// drag, the header menu and ⌥↑/⌥↓ all land here.
    ///
    /// A manual order only reads back when the sidebar sorts by it, so the
    /// first move pins the sort to Manual — otherwise the reorder would be
    /// silently re-sorted away and look like a no-op.
    pub(super) fn move_project_group(
        &mut self,
        moving: &str,
        target: &str,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        // Pin the displayed order first: from here on the stored list is what
        // the user is looking at, with the move applied.
        let base = order_workspaces_by_labels(&self.workspaces, &self.project_group_order());
        let next = reorder_workspaces(&base, moving, target, after);
        // The displayed order is the reference: a move that would not change
        // what the user sees changes nothing at all — no write, and no sort
        // switch for a drop that lands where the group already was.
        if next == base {
            cx.notify();
            return;
        }
        self.workspaces = next;
        if self.workspace_sort != WorkspaceSort::Manual {
            self.workspace_sort = WorkspaceSort::Manual;
            self.toast_info(tr!("sidebar.project_order_manual"));
        }
        self.persist_workspace_prefs();
        cx.notify();
    }

    // ── session order inside a project group (#143) ────────────────────

    /// One project group's sessions in the order the sidebar shows them —
    /// pinned first, then the unarranged ones by recency, then the arranged
    /// block. `sessions` is the list to read (the unfiltered snapshot when the
    /// caller wants hidden rows to keep their place).
    fn group_session_order(&self, label: &str, sessions: &[SessionInfo]) -> Vec<PathBuf> {
        sidebar_groups(
            sessions,
            &self.workspaces,
            self.workspace_sort,
            &self.workspace_added_at,
            &crate::pins::all().paths(),
            &crate::session_order::ranks(),
        )
        .into_iter()
        .find(|(group, _, _)| group == label)
        .map(|(_, _, ixs)| {
            ixs.into_iter()
                .map(|ix| sessions[ix].path.clone())
                .collect()
        })
        .unwrap_or_default()
    }

    /// Whether session rows can be rearranged where the sidebar is: the
    /// grouped list, where every row sits in a project block. A flat, Recent
    /// or search list orders by recency and offers no session drag or move.
    pub(super) fn sessions_reorderable(&self) -> bool {
        self.sidebar_group_by == SidebarGroupBy::Workspace && self.sidebar_query.is_empty()
    }

    /// Whether the session row has a neighbour above / below inside its
    /// project group — the row menu's Move up / Move down enabled state.
    pub(super) fn session_move_flags(&self, path: &Path) -> (bool, bool) {
        if !self.sessions_reorderable() {
            return (false, false);
        }
        let sessions = self.sidebar_sessions();
        let Some(session) = sessions.iter().find(|session| session.path == path) else {
            return (false, false);
        };
        let label = sessions::workspace_label(&session.cwd);
        let order = self.group_session_order(&label, &sessions);
        match order.iter().position(|candidate| candidate == path) {
            Some(ix) => (ix > 0, ix + 1 < order.len()),
            None => (false, false),
        }
    }

    /// Drag tracking for session rows. The landing row is resolved from the
    /// list's measured row bounds; a row in another project is not a target,
    /// because a session never leaves its folder.
    pub(super) fn on_session_drag_move(
        &mut self,
        event: &DragMoveEvent<SessionDrag>,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let pointer = event.event.position;
        let sessions = self.sidebar_sessions_filtered();
        let rows = self.sidebar_rows(&sessions, &crate::pins::all().paths());
        let label = sessions
            .iter()
            .find(|session| session.path == drag.path)
            .map(|session| sessions::workspace_label(&session.cwd));
        let target = label.as_deref().and_then(|label| {
            rows.iter().enumerate().find_map(|(ix, row)| {
                let SideRow::Session(session_ix) = row else {
                    return None;
                };
                let session = sessions.get(*session_ix)?;
                if session.path == drag.path || sessions::workspace_label(&session.cwd) != label {
                    return None;
                }
                self.sidebar_list
                    .bounds_for_item(ix)
                    .filter(|bounds| bounds.contains(&pointer))
                    .map(|bounds| SessionDropTarget {
                        path: session.path.clone(),
                        after: pointer.y > bounds.origin.y + bounds.size.height / 2.,
                    })
            })
        });
        let next = Some(SessionDragState {
            dragging: drag.path,
            target,
        });
        if self.session_drag != next {
            self.session_drag = next;
            cx.notify();
        }
    }

    /// A session row was released. The landing row was decided while the
    /// pointer moved; a release with no target changes nothing.
    pub(super) fn on_session_drop(&mut self, drag: &SessionDrag, cx: &mut Context<Self>) {
        let target = self
            .session_drag
            .take()
            .filter(|state| state.dragging == drag.path)
            .and_then(|state| state.target);
        match target {
            Some(target) => self.move_session(&drag.path, &target.path, target.after, cx),
            None => cx.notify(),
        }
    }

    /// Move a session before (or after) another one in the same project, then
    /// persist that group's order. This is the one place the sidebar's session
    /// order is written: a drag, the row menu and ⌥↑/⌥↓ all land here.
    ///
    /// The group's *current* display order is what gets rewritten, so the
    /// result matches what the user saw — pins keep leading and the arranged
    /// block keeps the rest. Sessions the sidebar is not showing (archived, or
    /// pushed past the "Show more" cap) keep their place in the stored order.
    pub(super) fn move_session(
        &mut self,
        moving: &Path,
        target: &Path,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        // Read the unfiltered snapshot: a move made while the archived filter
        // hides rows must not drop those rows out of the arranged order.
        let sessions = self.sidebar_sessions();
        let Some(moving_session) = sessions.iter().find(|session| session.path == moving) else {
            return;
        };
        let label = sessions::workspace_label(&moving_session.cwd);
        let target_label = sessions
            .iter()
            .find(|session| session.path == target)
            .map(|session| sessions::workspace_label(&session.cwd));
        // A session belongs to its folder: never reorder across projects.
        if target_label.as_deref() != Some(label.as_str()) {
            return;
        }
        let order = self.group_session_order(&label, &sessions);
        let next = reorder_session_paths(&order, moving, target, after);
        if next == order {
            cx.notify();
            return;
        }
        crate::session_order::set_group(&label, next);
        cx.notify();
    }

    /// Move a session one place up (`delta < 0`) or down inside its project.
    pub(super) fn nudge_session(&mut self, moving: &Path, delta: isize, cx: &mut Context<Self>) {
        let sessions = self.sidebar_sessions();
        let Some(session) = sessions.iter().find(|session| session.path == moving) else {
            return;
        };
        let label = sessions::workspace_label(&session.cwd);
        let order = self.group_session_order(&label, &sessions);
        let Some(ix) = order.iter().position(|candidate| candidate == moving) else {
            return;
        };
        let Some(neighbour) = ix
            .checked_add_signed(delta)
            .and_then(|ix| order.get(ix))
            .cloned()
        else {
            return;
        };
        self.move_session(moving, &neighbour, delta > 0, cx);
    }

    /// Move a project group one place up (`delta < 0`) or down in the
    /// displayed order. Used by the header menu and the keyboard shortcut.
    pub(super) fn nudge_project_group(
        &mut self,
        moving: &str,
        delta: isize,
        cx: &mut Context<Self>,
    ) {
        let order = self.project_group_order();
        let Some(ix) = order.iter().position(|label| label == moving) else {
            return;
        };
        let Some(neighbour) = ix.checked_add_signed(delta).and_then(|ix| order.get(ix)) else {
            return;
        };
        let target = neighbour.clone();
        self.move_project_group(moving, &target, delta > 0, cx);
    }

    /// ⌥↑ / ⌥↓: move the row the keyboard cursor is on — a project header
    /// moves its project group, a session row moves that session inside its
    /// group. The moved row keeps the cursor afterwards, so the same chord can
    /// be pressed again to keep carrying it.
    pub(super) fn move_cursor_row(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(cursor) = self.sidebar_cursor else {
            return;
        };
        let rows = self.sidebar_rows_for_nav();
        enum Moved {
            Project(String),
            Session(PathBuf),
        }
        let moved = match rows.get(cursor) {
            Some(SideRow::Workspace { label, .. }) => Moved::Project(label.clone()),
            Some(SideRow::Session(ix)) => {
                // A flat or search list has no project block to move a row
                // inside of; recency owns that order.
                if !self.sessions_reorderable() {
                    return;
                }
                let sessions = self.sidebar_sessions_filtered();
                match sessions.get(*ix) {
                    Some(session) => Moved::Session(session.path.clone()),
                    None => return,
                }
            }
            // Overflow rows carry no order of their own.
            _ => return,
        };
        match &moved {
            Moved::Project(label) => self.nudge_project_group(label, delta, cx),
            Moved::Session(path) => self.nudge_session(path, delta, cx),
        }
        // The rows were rebuilt around the moved entry: keep the cursor on it,
        // not on whatever row now happens to hold its old index.
        let rows = self.sidebar_rows_for_nav();
        let sessions = self.sidebar_sessions_filtered();
        let moved_row = match &moved {
            Moved::Project(label) => rows.iter().position(
                |row| matches!(row, SideRow::Workspace { label: candidate, .. } if candidate == label),
            ),
            Moved::Session(path) => sessions
                .iter()
                .position(|session| &session.path == path)
                .and_then(|session_ix| {
                    rows.iter().position(
                        |row| matches!(row, SideRow::Session(ix) if *ix == session_ix),
                    )
                }),
        };
        if let Some(ix) = moved_row {
            self.sidebar_cursor = Some(ix);
            self.sidebar_list.scroll_to_reveal_item(ix);
            cx.notify();
        }
    }

    /// Row index of the open session in `rows`, when it is visible.
    fn active_session_row(&self, rows: &[SideRow]) -> Option<usize> {
        let sessions = self.sidebar_sessions_filtered();
        let active = self.current_session_path.as_ref()?;
        let session_ix = sessions
            .iter()
            .position(|session| &session.path == active)?;
        rows.iter()
            .position(|row| matches!(row, SideRow::Session(ix) if *ix == session_ix))
    }

    /// The sidebar's session search: one field over the list. It is always
    /// mounted so the control never moves, and its
    /// text mirrors into `sidebar_query`, which both the row builder and
    /// keyboard navigation read.
    pub(super) fn sidebar_search_bar(&self, theme: Theme) -> AnyElement {
        let field = search_field(self.sidebar_search.clone(), &theme)
            .w_full()
            .when(self.sidebar_query.is_empty(), |field| {
                field.child(
                    div()
                        .id("sidebar-palette-shortcut")
                        .tip(crate::commands::tooltip(
                            crate::commands::CommandId::ToggleCommandPalette,
                            false,
                        ))
                        .child(kbd_shortcut(crate::platform::shortcuts::PALETTE, &theme)),
                )
            });
        let clear = (!self.sidebar_query.is_empty()).then(|| {
            let input = self.sidebar_search.clone();
            IconButton::new("sidebar-search-clear", "icons/x.svg")
                .size(ButtonSize::Compact)
                .tip(tr!("common.clear"))
                .faded(&theme, move |button| {
                    button.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                        input.update(cx, |input, cx| input.clear(cx));
                    })
                })
        });
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap_1()
            .child(div().flex_1().min_w_0().child(field))
            .children(clear)
            .into_any_element()
    }

    /// ⌘⇧B: reveal the sidebar if hidden, then focus it and place the cursor
    /// on the open session (or the first row).
    pub(super) fn on_focus_sessions(
        &mut self,
        _: &crate::FocusSessions,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Settings is a full-window surface with its own nav; the sidebar is
        // not mounted, so there is nothing to focus.
        if self.settings_open {
            return;
        }
        if !self.sidebar_visible {
            self.sidebar_visible = true;
        }
        self.clear_session_card(cx);
        let rows = self.sidebar_rows_for_nav();
        self.sidebar_cursor =
            (!rows.is_empty()).then(|| self.active_session_row(&rows).unwrap_or(0));
        window.focus(&self.sidebar_focus);
        cx.notify();
    }

    /// Visible sessions in the order the sidebar navigates them — the
    /// numbering `⌘1…⌘9` and session cycling share.
    pub(super) fn navigable_sessions(&self) -> Vec<SessionInfo> {
        let sessions = self.sidebar_sessions_filtered();
        self.sidebar_rows(&sessions, &crate::pins::all().paths())
            .iter()
            .filter_map(|row| match row {
                SideRow::Session(ix) => sessions.get(*ix).cloned(),
                _ => None,
            })
            .collect()
    }

    /// `⌘1…⌘9`: open the Nth session the sidebar shows. A slot past the end
    /// is a no-op, so a stray number never switches the session by accident.
    pub(super) fn open_session_slot(&mut self, slot: usize, cx: &mut Context<Self>) {
        if slot == 0 {
            return;
        }
        let Some(session) = self.navigable_sessions().into_iter().nth(slot - 1) else {
            return;
        };
        self.on_open_session(session, cx);
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab: cycle through the visible sessions, wrapping
    /// at both ends. With a single session there is nowhere to go.
    pub(super) fn cycle_session(&mut self, direction: isize, cx: &mut Context<Self>) {
        let sessions = self.navigable_sessions();
        if sessions.len() < 2 {
            return;
        }
        let current = self
            .current_session_path
            .as_ref()
            .and_then(|path| sessions.iter().position(|session| &session.path == path));
        let next = match current {
            Some(ix) => (ix as isize + direction).rem_euclid(sessions.len() as isize) as usize,
            None if direction > 0 => 0,
            None => sessions.len() - 1,
        };
        self.on_open_session(sessions[next].clone(), cx);
    }

    /// Action handler for ⌘1…⌘9.
    pub(super) fn on_open_session_slot(
        &mut self,
        action: &crate::OpenSessionSlot,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_session_slot(action.slot, cx);
    }

    /// Action handler for Ctrl+Tab.
    pub(super) fn on_next_session(
        &mut self,
        _: &crate::NextSession,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_session(1, cx);
    }

    /// Action handler for Ctrl+Shift+Tab.
    pub(super) fn on_prev_session(
        &mut self,
        _: &crate::PrevSession,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cycle_session(-1, cx);
    }

    pub(super) fn on_sidebar_prev(
        &mut self,
        _: &crate::SidebarPrev,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.row_menu_step(MenuKey::Prev, cx) {
            return;
        }
        self.move_sidebar_cursor(|ix| ix.saturating_sub(1), cx);
    }

    pub(super) fn on_sidebar_next(
        &mut self,
        _: &crate::SidebarNext,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.row_menu_step(MenuKey::Next, cx) {
            return;
        }
        self.move_sidebar_cursor(|ix| ix.saturating_add(1), cx);
    }

    pub(super) fn on_sidebar_home(
        &mut self,
        _: &crate::SidebarHome,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.row_menu_step(MenuKey::First, cx) {
            return;
        }
        self.move_sidebar_cursor(|_| 0, cx);
    }

    pub(super) fn on_sidebar_end(
        &mut self,
        _: &crate::SidebarEnd,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.row_menu_step(MenuKey::Last, cx) {
            return;
        }
        self.move_sidebar_cursor(|_| usize::MAX, cx);
    }

    /// Page Up on the sidebar: scroll the session preview toward its top.
    ///
    /// The card is the only place a keyboard reader sees a session's full path
    /// and provider list, and it can be taller than the window; the arrows are
    /// already moving the row cursor, so the card pages instead. Registered in
    /// the `Sidebar` context, which is the only place the card shows.
    pub(super) fn on_sidebar_preview_up(
        &mut self,
        _: &crate::SidebarPreviewUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_session_preview(1.0, cx);
    }

    /// Page Down on the sidebar: scroll the session preview toward its end.
    pub(super) fn on_sidebar_preview_down(
        &mut self,
        _: &crate::SidebarPreviewDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scroll_session_preview(-1.0, cx);
    }

    /// Move the session preview by `direction` pages: `1.0` toward the top,
    /// `-1.0` toward the end.
    ///
    /// A no-op when no card is on screen, so the chord stays quiet in a
    /// collapsed or empty sidebar instead of scrolling a view nobody can see.
    fn scroll_session_preview(&mut self, direction: f32, cx: &mut Context<Self>) {
        if !self.session_preview_showing() {
            return;
        }
        let handle = &self.session_card_scroll;
        let max = handle.max_offset().height;
        if max <= px(0.) {
            return;
        }
        // A page is the card's own visible height, less a line, so a reader
        // keeps one row of context across the jump.
        let page = (handle.bounds().size.height - px(24.)).max(px(40.));
        let y = paged_offset(handle.offset().y, page, max, direction);
        handle.set_offset(point(px(0.), y));
        cx.notify();
    }

    /// Whether a session preview is on screen: the pointer's, or the one the
    /// keyboard cursor paints for the row it is on.
    ///
    /// The card is a child of the row, so the two paths differ only in what
    /// selected the row — this mirrors the condition in [`render_side_row`].
    fn session_preview_showing(&self) -> bool {
        self.session_card.is_some()
            || (self.sidebar_visible
                && self.sidebar_cursor.is_some()
                && !self.sidebar_sort_menu
                && !self.sidebar_show_menu
                && self.session_menu.is_none()
                && self.workspace_menu.is_none())
    }

    /// The open row menu's entries while it shows its action list (not the
    /// delete confirmation or the icon picker, which keep their own controls).
    fn row_menu_items(&self) -> Option<Vec<(RowMenuItem, bool)>> {
        if let Some(menu) = &self.session_menu {
            return Some(session_menu_items(menu));
        }
        let menu = self.workspace_menu.as_ref()?;
        (!menu.picker).then(|| workspace_menu_items(menu))
    }

    fn row_menu_cursor(&mut self) -> Option<&mut crate::ui::menu::MenuCursor> {
        if let Some(menu) = self.session_menu.as_mut() {
            return Some(&mut menu.cursor);
        }
        self.workspace_menu.as_mut().map(|menu| &mut menu.cursor)
    }

    /// Arrows while a row menu is open move its cursor instead of the row
    /// cursor. Returns whether a menu took the key (an open menu always does).
    fn row_menu_step(&mut self, key: MenuKey, cx: &mut Context<Self>) -> bool {
        if self.session_menu.is_none() && self.workspace_menu.is_none() {
            return false;
        }
        if let Some(items) = self.row_menu_items() {
            if let Some(cursor) = self.row_menu_cursor() {
                cursor.step(key, items.len(), |ix| items[ix].1);
            }
            cx.notify();
        }
        true
    }

    /// Enter while a row menu is open runs its highlighted entry. Returns
    /// whether a menu took the key. The delete confirmation does not run on
    /// Enter: its destructive button needs an explicit press, and Escape
    /// cancels it.
    fn row_menu_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.session_menu.is_none() && self.workspace_menu.is_none() {
            return false;
        }
        let Some(items) = self.row_menu_items() else {
            return true;
        };
        let lit = self
            .row_menu_cursor()
            .and_then(|cursor| cursor.highlighted(items.len()));
        if let Some((item, true)) = lit.and_then(|ix| items.get(ix).copied()) {
            self.run_row_menu_item(item, window, cx);
        }
        true
    }

    /// Run one row-menu action through the same handler its row's click uses.
    fn run_row_menu_item(
        &mut self,
        item: RowMenuItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let session_path = self.session_menu.as_ref().map(|menu| menu.path.clone());
        let project_label = self.workspace_menu.as_ref().map(|menu| menu.label.clone());
        match item {
            RowMenuItem::SessionMoveUp | RowMenuItem::SessionMoveDown => {
                if let Some(path) = session_path {
                    self.on_session_move(&path, item == RowMenuItem::SessionMoveUp, cx);
                }
            }
            RowMenuItem::Rename => self.on_menu_rename_session(window, cx),
            RowMenuItem::Pin => self.on_menu_toggle_pin(cx),
            RowMenuItem::Archive => self.on_menu_toggle_archive(cx),
            RowMenuItem::CopyPath => self.on_menu_copy_path(cx),
            RowMenuItem::Clone => self.on_menu_clone_session(cx),
            RowMenuItem::Delete => self.on_menu_delete_request(cx),
            RowMenuItem::ProjectMoveUp | RowMenuItem::ProjectMoveDown => {
                if let Some(label) = project_label {
                    self.on_workspace_move_project(&label, item == RowMenuItem::ProjectMoveUp, cx);
                }
            }
            RowMenuItem::ProjectCopyPath => self.on_workspace_copy_path(cx),
            RowMenuItem::Worktrees => {
                self.workspace_menu = None;
                self.open_worktrees(cx);
            }
            RowMenuItem::RenameProject => self.on_workspace_rename(window, cx),
            RowMenuItem::IconAndColor => self.on_workspace_open_picker(cx),
            RowMenuItem::ArchiveSessions => self.on_workspace_archive_sessions(cx),
            RowMenuItem::RemoveProject => self.on_workspace_delete(cx),
            RowMenuItem::DeleteProjectFiles => self.on_workspace_delete_files_request(cx),
        }
    }

    /// Shift-F10 / the Menu key: the keyboard route to the row's context
    /// menu. Opens the menu of the row under the keyboard cursor, under the
    /// row, with its first entry that can act highlighted.
    pub(super) fn on_sidebar_open_row_menu(
        &mut self,
        _: &crate::SidebarOpenRowMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.sidebar_cursor else {
            return;
        };
        let sessions = self.sidebar_sessions_filtered();
        let rows = self.sidebar_rows(&sessions, &crate::pins::all().paths());
        match rows.get(index) {
            Some(SideRow::Session(ix)) => {
                let Some(session) = sessions.get(*ix).cloned() else {
                    return;
                };
                let (can_move_up, can_move_down) = self.session_move_flags(&session.path);
                let deletable = !self.session_delete_blocked(&session.path);
                let mut menu = SessionMenu {
                    path: session.path.clone(),
                    title: session.title.clone(),
                    deletable,
                    at: None,
                    reorderable: self.sessions_reorderable(),
                    can_move_up,
                    can_move_down,
                    cursor: Default::default(),
                };
                let items = session_menu_items(&menu);
                menu.cursor = crate::ui::menu::MenuCursor::keyed(items.len(), |ix| items[ix].1);
                self.open_session_menu_at(menu, window, cx);
            }
            Some(SideRow::Workspace { label, cwd, .. }) => {
                let (can_move_up, can_move_down) = self.project_move_flags(label);
                let mut menu = WorkspaceMenu {
                    label: label.clone(),
                    cwd: cwd.clone(),
                    at: None,
                    picker: false,
                        can_move_up,
                    can_move_down,
                    cursor: Default::default(),
                };
                let items = workspace_menu_items(&menu);
                menu.cursor = crate::ui::menu::MenuCursor::keyed(items.len(), |ix| items[ix].1);
                self.open_workspace_menu_at(menu, window, cx);
            }
            _ => {}
        }
    }

    /// Move the cursor by `step`, clamped to the visible rows, and scroll it
    /// into view.
    fn move_sidebar_cursor(&mut self, step: impl Fn(usize) -> usize, cx: &mut Context<Self>) {
        self.clear_session_card(cx);
        let count = self.sidebar_rows_for_nav().len();
        self.sidebar_cursor = clamp_cursor(step(self.sidebar_cursor.unwrap_or(0)), count);
        if let Some(next) = self.sidebar_cursor {
            self.sidebar_list.scroll_to_reveal_item(next);
        }
        cx.notify();
    }

    /// Enter on the cursor row: open a session (returning focus to the
    /// composer), toggle a workspace group, or reveal/collapse a group's
    /// overflow — the same actions the row's click affordances perform.
    pub(super) fn on_sidebar_confirm(
        &mut self,
        _: &crate::SidebarConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.row_menu_confirm(window, cx) {
            cx.notify();
            return;
        }
        let Some(index) = self.sidebar_cursor else {
            return;
        };
        let sessions = self.sidebar_sessions_filtered();
        let rows = self.sidebar_rows(&sessions, &crate::pins::all().paths());
        let Some(row) = rows.get(index) else {
            return;
        };
        match row {
            SideRow::Session(ix) => {
                if let Some(session) = sessions.get(*ix).cloned() {
                    self.on_open_session(session, cx);
                }
                // The row is a destination: hand focus back to the composer
                // so typing resumes immediately.
                self.sidebar_cursor = None;
                self.input.read(cx).focus(window);
            }
            SideRow::Workspace { label, .. } => {
                let working = self.workspace_label();
                toggle_workspace_group(
                    label.clone(),
                    &working,
                    &mut self.collapsed_workspaces,
                    &mut self.expanded_workspace_groups,
                );
                self.record_collapsed_groups();
                self.clamp_sidebar_cursor(cx);
            }
            SideRow::ShowMore { label, count, .. } => {
                self.expanded_session_groups
                    .entry(label.clone())
                    .and_modify(|extra| *extra += count)
                    .or_insert(*count);
                self.record_expanded_groups();
                self.clamp_sidebar_cursor(cx);
            }
            SideRow::ShowLess { label } => {
                self.expanded_session_groups.remove(label);
                self.record_expanded_groups();
                self.clamp_sidebar_cursor(cx);
            }
            SideRow::RemoteHost { name, .. } => {
                let name = name.clone();
                self.toggle_remote_session_group(&name, cx);
                self.clamp_sidebar_cursor(cx);
            }
            SideRow::RemoteSession(row) => {
                let (host, session) = (row.host.clone(), row.session.clone());
                self.open_remote_session(host, session, cx);
            }
            // A day label opens nothing; Enter on it is a no-op.
            SideRow::DateSection { .. } => {}
        }
        cx.notify();
    }

    /// Esc: dismiss an open row menu first, otherwise leave keyboard
    /// navigation and hand focus back to the composer.
    pub(super) fn on_sidebar_close(
        &mut self,
        _: &crate::SidebarClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session_menu.take().is_some()
            || self.workspace_menu.take().is_some()
            || self.sidebar_sort_menu
            || self.sidebar_show_menu
        {
            self.close_sidebar_options();
            cx.notify();
            return;
        }
        self.sidebar_cursor = None;
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// ⌥↑: move the cursor's row (a project group, or a session inside its
    /// group) one place up in the sidebar.
    pub(super) fn on_sidebar_move_row_up(
        &mut self,
        _: &crate::SidebarMoveRowUp,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursor_row(-1, cx);
    }

    /// ⌥↓: move the cursor's row one place down in the sidebar.
    pub(super) fn on_sidebar_move_row_down(
        &mut self,
        _: &crate::SidebarMoveRowDown,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_cursor_row(1, cx);
    }

    /// The command palette's Delete Session: reveal the sidebar on the active
    /// session's row and arm the same confirmation the row menu shows, so the
    /// destructive step still needs an explicit confirm.
    pub(super) fn arm_active_session_delete(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.current_session_path.clone() else {
            return;
        };
        let sessions = self.sidebar_sessions();
        let Some(session_ix) = sessions.iter().position(|session| session.path == path) else {
            return;
        };
        let session = sessions[session_ix].clone();
        if !self.sidebar_visible {
            self.sidebar_visible = true;
        }
        let visible_sessions = self.sidebar_sessions_filtered();
        let visible_ix = visible_sessions.iter().position(|item| item.path == path);
        let rows = self.sidebar_rows(&visible_sessions, &crate::pins::all().paths());
        self.sidebar_cursor = visible_ix.and_then(|session_ix| {
            rows.iter()
                .position(|row| matches!(row, SideRow::Session(ix) if *ix == session_ix))
        });
        if let Some(ix) = self.sidebar_cursor {
            self.sidebar_list.scroll_to_reveal_item(ix);
        }
        self.session_menu = None;
        let cwd = self.session_delete_workspace(&session.path);
        self.request_confirm(
            super::confirm_ui::PendingConfirm::DeleteSession {
                path: session.path,
                title: session.title,
                cwd,
            },
            cx,
        );
        window.focus(&self.sidebar_focus);
        cx.notify();
    }

    /// Keep the cursor inside the rows after one collapses the list under it.
    fn clamp_sidebar_cursor(&mut self, cx: &mut Context<Self>) {
        let count = self.sidebar_rows_for_nav().len();
        self.sidebar_cursor = self.sidebar_cursor.and_then(|ix| clamp_cursor(ix, count));
        cx.notify();
    }
}

/// Clamp a cursor position into `count` rows: `None` when there are none, the
/// nearest valid index otherwise.
fn clamp_cursor(index: usize, count: usize) -> Option<usize> {
    (count > 0).then(|| index.min(count - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A project mark shows itself, not a box: its slot paints no face and no
    /// hairline, and a logo fills the whole slot instead of sitting inside a
    /// border.
    #[test]
    fn project_marks_sit_without_a_box() {
        let source = include_str!("sidebar.rs");
        let start = source.find("fn project_icon(").expect("project_icon");
        let end = start + source[start..].find("\n}\n").expect("fn end");
        let body = &source[start..end];
        assert!(!body.contains(".bg("), "the slot paints no face");
        assert!(!body.contains(".border_1()"), "and no hairline");
        assert!(body.contains(".size(tile)\n"), "a logo fills the slot");
    }

    struct SessionRowProbe {
        app: Entity<OrbitApp>,
        session: SessionInfo,
        status: Option<SessionStatus>,
        active: bool,
        show_updated: bool,
        /// The workspace header above the row, for a grouped list. `None` is
        /// a flat list, where each row names its own workspace.
        header: Option<PathBuf>,
        theme: Theme,
    }

    impl gpui::Render for SessionRowProbe {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let card_path = self.app.read(cx).session_card.clone();
            let rows = Rc::new(match &self.header {
                Some(cwd) => vec![
                    SideRow::Workspace {
                        label: "project".into(),
                        count: 1,
                        collapsed: false,
                        cwd: cwd.clone(),
                    },
                    SideRow::Session(0),
                ],
                None => vec![SideRow::Session(0)],
            });
            let row_ix = rows.len() - 1;
            let branches = Rc::new(HashMap::from([(
                self.session.cwd.clone(),
                BranchLine {
                    name: "main".into(),
                    ahead_behind: None,
                },
            )]));
            let sessions = Rc::new(vec![self.session.clone()]);
            let statuses = Rc::new(
                self.status
                    .map(|s| (self.session.path.clone(), s))
                    .into_iter()
                    .collect(),
            );
            render_side_row(
                &rows,
                &sessions,
                self.active.then_some(self.session.path.as_path()),
                row_ix,
                &self.app,
                false,
                &Rc::default(),
                &Rc::default(),
                &Rc::default(),
                self.header.is_none(),
                false,
                self.show_updated,
                false,
                &Rc::default(),
                &Rc::default(),
                &Rc::default(),
                &branches,
                &Rc::default(),
                &statuses,
                None,
                None,
                &SidebarDragPaint::idle(),
                false,
                card_path.as_deref(),
                false,
                self.theme,
            )
            .into_any_element()
        }
    }

    #[gpui::test]
    fn session_row_badges_align_with_title_at_multiple_scales(cx: &mut gpui::TestAppContext) {
        use std::time::{Duration, SystemTime};

        cx.update(|cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        let session = SessionInfo {
            path: "/tmp/row-alignment.jsonl".into(),
            id: "alignment".into(),
            cwd: "/tmp".into(),
            title: "A long session title that should truncate in narrow rows".into(),
            first_message: String::new(),
            modified: SystemTime::now() - Duration::from_secs(60),
        };
        for (width, font_size, status, active, show_updated) in [
            (320., 14., Some(SessionStatus::Completed), true, true),
            (220., 18., Some(SessionStatus::Failed), true, true),
            (
                220.,
                18.,
                Some(SessionStatus::WaitingApproval),
                false,
                false,
            ),
            (320., 14., Some(SessionStatus::Running), false, true),
            (320., 14., None, false, true),
        ] {
            let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
            theme.ui.ui_font_size = font_size;
            let probe = cx.update(|_, cx| {
                cx.new(|_| SessionRowProbe {
                    app: app.clone(),
                    session: session.clone(),
                    status,
                    active,
                    show_updated,
                    header: None,
                    theme,
                })
            });
            cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(width), px(160.)),
                |_, _| probe.clone(),
            );
            let mut bounds = |name| {
                cx.debug_bounds(name)
                    .unwrap_or_else(|| panic!("missing {name}"))
            };
            let center = |b: gpui::Bounds<gpui::Pixels>| b.origin.y + b.size.height / 2.;
            let title = bounds("session-title");
            let badge = bounds("session-status-badge");
            let lane = bounds("session-status-lane");
            let text = bounds("session-status-text");
            for (name, b) in [("badge", badge), ("lane", lane), ("text", text)] {
                assert!((center(title) - center(b)).abs() <= px(0.51),
                    "{name} not centered: width={width}, font={font_size}, status={status:?}: title={title:?}, element={b:?}");
            }
            let expected_text = TextSize::XSmall.px(&theme) * 1.3;
            assert!(
                (text.size.height - expected_text).abs() <= px(0.51),
                "status text should use the metadata leading: {text:?}, expected {expected_text:?}"
            );
            // The open row is marked by its fill alone: no "current" badge.
            assert!(cx.debug_bounds("session-current-badge").is_none());
            assert!(badge.origin.x + badge.size.width <= px(width));
            assert!(title.origin.x + title.size.width <= badge.origin.x);
        }
    }

    /// Under a workspace header a session is one 28px line: the header already
    /// names the folder and its branch. The location line returns only where
    /// it says something new — a flat list, or a session in another folder.
    #[gpui::test]
    fn grouped_session_rows_drop_the_repeated_location_line(cx: &mut gpui::TestAppContext) {
        use std::time::{Duration, SystemTime};

        let theme = Theme::for_id(crate::theme::ThemeId::OrbitAzure);
        cx.update(|cx| cx.set_global(theme));
        let app = cx.update(|cx| cx.new(OrbitApp::new));
        let test_cx = cx;
        let session = SessionInfo {
            path: "/tmp/project/row.jsonl".into(),
            id: "grouped".into(),
            cwd: "/tmp/project".into(),
            title: "Fade hover on composer chips".into(),
            first_message: String::new(),
            modified: SystemTime::now() - Duration::from_secs(60),
        };
        for (header, location) in [
            (Some(PathBuf::from("/tmp/project")), false),
            (Some(PathBuf::from("/elsewhere/project")), true),
            (None, true),
        ] {
            // A window per case: debug bounds from an earlier draw would
            // otherwise answer for an element this draw no longer paints.
            let cx = test_cx.add_empty_window();
            let probe = cx.update(|_, cx| {
                cx.new(|_| SessionRowProbe {
                    app: app.clone(),
                    session: session.clone(),
                    status: None,
                    active: false,
                    show_updated: true,
                    header: header.clone(),
                    theme,
                })
            });
            cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(248.), px(160.)),
                |_, _| probe.clone(),
            );
            assert_eq!(
                cx.debug_bounds("session-location").is_some(),
                location,
                "header={header:?}"
            );
            // A grouped row nests under its project: it steps in by the
            // chevron column and sits on the group's tree guide. A flat row
            // stays flush with no guide.
            let card = cx
                .debug_bounds("session-row-card")
                .expect("session row card");
            let guide = cx.debug_bounds("sidebar-tree-guide");
            if header.is_some() {
                let guide = guide.expect("grouped rows draw the tree guide");
                assert!(
                    (card.origin.x - session_indent(&theme)).abs() <= px(0.51),
                    "grouped card starts at the session indent: {card:?}"
                );
                assert!(
                    guide.origin.x < card.origin.x,
                    "guide runs left of the card"
                );
            } else {
                assert!(guide.is_none(), "flat rows draw no tree guide");
                assert!(
                    card.origin.x.abs() <= px(0.51),
                    "flat card is flush: {card:?}"
                );
            }
            if !location {
                let row = cx
                    .debug_bounds("session-row-card")
                    .expect("session row card");
                assert!(
                    (row.size.height - ButtonSize::Medium.height(&theme)).abs() <= px(0.51),
                    "a one-line session row is a 28px navigation row: {row:?}"
                );
            }
        }
    }

    struct SessionCardProbe {
        hover: SessionTooltip,
        theme: Theme,
        origin: Option<Point<Pixels>>,
        scroll: gpui::ScrollHandle,
    }

    impl Render for SessionCardProbe {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let card = session_card_in_viewport(
                &self.hover,
                &self.theme,
                window.viewport_size(),
                &self.scroll,
            );
            if let Some(origin) = self.origin {
                div()
                    .size_full()
                    .child(
                        div()
                            .absolute()
                            .left(origin.x)
                            .top(origin.y)
                            .size(px(0.))
                            .child(anchored_menu(
                                Corner::TopLeft,
                                point(tooltip::offset_x(&self.theme), px(0.)),
                                card,
                            )),
                    )
                    .into_any_element()
            } else {
                card.into_any_element()
            }
        }
    }

    fn session_card_probe(
        hover: &SessionTooltip,
        theme: &Theme,
        origin: Option<Point<Pixels>>,
        cx: &mut App,
    ) -> Entity<SessionCardProbe> {
        session_card_probe_with_scroll(hover, theme, origin, gpui::ScrollHandle::new(), cx)
    }

    /// The same probe with the caller's scroll handle, so a test can drive the
    /// offset the card tracks.
    fn session_card_probe_with_scroll(
        hover: &SessionTooltip,
        theme: &Theme,
        origin: Option<Point<Pixels>>,
        scroll: gpui::ScrollHandle,
        cx: &mut App,
    ) -> Entity<SessionCardProbe> {
        cx.new(|_| SessionCardProbe {
            hover: hover.clone(),
            theme: *theme,
            origin,
            scroll,
        })
    }

    struct SessionCardHandoffProbe(Entity<SessionRowProbe>);

    impl Render for SessionCardHandoffProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(div().w(px(240.)).child(self.0.clone()))
        }
    }

    #[gpui::test]
    fn session_card_direct_row_popup_handoff(cx: &mut gpui::TestAppContext) {
        let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
        cx.update(|cx| cx.set_global(theme));
        let app = cx.update(|cx| cx.new(OrbitApp::new));
        let path = PathBuf::from("/tmp/handoff.jsonl");
        let row_probe = cx.update(|cx| {
            cx.new(|cx| {
                cx.observe(&app, |_, _, cx| cx.notify()).detach();
                SessionRowProbe {
                    app: app.clone(),
                    session: SessionInfo {
                        path: path.clone(),
                        id: "handoff".into(),
                        cwd: "/tmp".into(),
                        title: "Session".into(),
                        first_message: String::new(),
                        modified: std::time::SystemTime::now(),
                    },
                    status: None,
                    active: false,
                    show_updated: false,
                    header: None,
                    theme,
                }
            })
        });
        let (_, cx) = cx.add_window_view(|_, _| SessionCardHandoffProbe(row_probe));
        cx.simulate_resize(gpui::size(px(960.), px(640.)));
        cx.run_until_parked();
        let row = cx.debug_bounds("session-row-card").unwrap();
        cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.executor().advance_clock(SESSION_CARD_DELAY);
        cx.run_until_parked();
        assert_eq!(
            app.read_with(cx, |app, _| app.session_card.clone()),
            Some(path.clone())
        );
        // The real row now has its anchored popup child. Do not synthesize
        // a row leave: one direct move must dispatch both GPUI hover callbacks.
        let card = cx.debug_bounds("session-tooltip").unwrap();
        assert!(!row.contains(&card.center()));
        cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card_wait.is_none()));
        assert_eq!(
            app.read_with(cx, |app, _| app.session_card_popup_hover.clone()),
            Some(path.clone())
        );
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card.is_some()));
        // The reverse move must keep row ownership when popup leave runs.
        cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card_popup_hover.is_none()));
        assert_eq!(
            app.read_with(cx, |app, _| app.session_card_wait.clone()),
            Some(path.clone())
        );
        cx.executor().advance_clock(Duration::from_millis(300));
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card.is_some()));
        // Leaving both surfaces (from the popup) must still dismiss.
        cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
        cx.simulate_mouse_move(point(px(959.), px(639.)), None, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card.is_none()));
        assert!(app.read_with(cx, |app, _| app.session_card_popup_hover.is_none()));

        // Reopen, then right-click the real row while popup ownership is set.
        cx.simulate_mouse_move(row.center(), None, gpui::Modifiers::none());
        cx.run_until_parked();
        cx.executor().advance_clock(SESSION_CARD_DELAY);
        cx.run_until_parked();
        cx.simulate_mouse_move(card.center(), None, gpui::Modifiers::none());
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card_popup_hover.is_some()));
        cx.simulate_mouse_down(row.center(), MouseButton::Right, gpui::Modifiers::none());
        assert!(app.read_with(cx, |app, _| app.session_menu.is_some()));
        assert!(app.read_with(cx, |app, _| app.session_card.is_none()
            && app.session_card_wait.is_none()
            && app.session_card_popup_hover.is_none()));
        // Late events from the old popup, and row hover under a menu, must
        // not resurrect ownership or a delayed card.
        app.update(cx, |app, cx| {
            app.hover_session_card_popup(path.clone(), true, cx);
            app.hover_session_card(path.clone(), true, cx);
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert!(app.read_with(cx, |app, _| app.session_card.is_none()
            && app.session_card_wait.is_none()
            && app.session_card_popup_hover.is_none()));
    }

    #[gpui::test]
    fn session_card_anchored_bounds_fit_small_viewport_at_large_scale(
        cx: &mut gpui::TestAppContext,
    ) {
        for density in [80, 120] {
            for origin in [
                point(px(240.), px(0.)),
                point(px(240.), px(620.)),
                point(px(950.), px(320.)),
            ] {
                let cx = cx.add_empty_window();
                cx.simulate_resize(gpui::size(px(960.), px(640.)));
                let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
                theme.ui.ui_font_size = 20.;
                theme.ui.spacing_density = density;
                let hover = SessionTooltip {
                    title: "Long session title ".repeat(40).into(),
                    summary: None,
                    shortcut: Some("Open ⌘2".into()),
                    workspace: "Long workspace ".repeat(20).into(),
                    path: format!("/tmp/{}", "directory/".repeat(100)).into(),
                    branch: Some("feature/".repeat(100).into()),
                    ahead: None,
                    behind: None,
                    providers: vec![sessions::SessionProvider {
                        provider: "custom".into(),
                        model: Some("long-model-".repeat(100)),
                    }],
                };
                let viewport = gpui::size(px(960.), px(640.));
                cx.draw(point(px(0.), px(0.)), viewport, |_, cx| {
                    session_card_probe(&hover, &theme, Some(origin), cx)
                });
                let bounds = cx
                    .debug_bounds("session-tooltip")
                    .expect("anchored card painted");
                assert!(
                    bounds.left() >= px(0.) && bounds.top() >= px(0.),
                    "{bounds:?}"
                );
                assert!(
                    bounds.right() <= viewport.width && bounds.bottom() <= viewport.height,
                    "{bounds:?}"
                );
                assert!(bounds.size.height > px(0.));
            }
        }
    }

    #[test]
    fn a_page_press_moves_a_page_and_stops_at_both_ends() {
        let max = px(400.);
        let page = px(150.);
        // Down moves the content up: 0 at the top, `-max` at the end.
        assert_eq!(paged_offset(px(0.), page, max, -1.0), px(-150.));
        assert_eq!(paged_offset(px(-300.), page, max, -1.0), px(-400.));
        assert_eq!(paged_offset(px(-150.), page, max, 1.0), px(0.));
        assert_eq!(paged_offset(px(-50.), page, max, 1.0), px(0.));
        // Nothing scrollable, and a page larger than the content.
        assert_eq!(paged_offset(px(0.), page, px(0.), -1.0), px(0.));
        assert_eq!(paged_offset(px(-40.), px(900.), px(100.), -1.0), px(-100.));
    }

    #[test]
    fn session_provider_labels_keep_each_provider_and_optional_model() {
        let providers = [
            sessions::SessionProvider {
                provider: "anthropic".into(),
                model: Some("claude-sonnet".into()),
            },
            sessions::SessionProvider {
                provider: "custom-gateway".into(),
                model: None,
            },
        ];
        let labels: Vec<_> = providers.iter().map(session_provider_label).collect();
        assert_eq!(labels, ["anthropic · claude-sonnet", "custom-gateway"]);
        assert_eq!(
            provider_icon(&providers[1].provider).as_ref(),
            "icons/cloud.svg"
        );
    }

    #[test]
    fn session_card_summary_omits_empty_and_duplicate_text() {
        assert!(session_card_summary("Title", " \n\t").is_none());
        assert!(session_card_summary("A title", "A title").is_none());
        assert_eq!(
            session_card_summary("A title", " A\n title ").map(|s| s.to_string()),
            Some("A title".to_string())
        );
        assert_eq!(
            session_card_summary("Title", "first\n\nmessage\tcontinued").map(|s| s.to_string()),
            Some("first message continued".to_string())
        );
    }

    #[test]
    fn session_card_keeps_long_fallback_and_named_prefix_summaries() {
        let message = "a".repeat(150);
        let path = Path::new("/tmp/long-session.jsonl");
        let rows = sessions_with_placeholder(&[], Some(path), None, Some(&message), None, true);
        let session = &rows[0];
        assert_eq!(session.title, cap_chars(&message, 80));
        assert_eq!(session.first_message, cap_chars(&message, 110));
        assert_eq!(
            session_card_summary(&session.title, &session.first_message).map(|s| s.to_string()),
            Some(session.first_message.clone())
        );

        // An explicitly named title can also match the truncated fallback.
        let named = sessions_with_placeholder(
            &[],
            Some(path),
            Some(&session.title),
            Some(&message),
            None,
            true,
        );
        assert_eq!(
            session_card_summary(&named[0].title, &named[0].first_message).map(|s| s.to_string()),
            Some(named[0].first_message.clone())
        );
    }

    /// A preview can be taller than the window, so its overflow must be
    /// reachable without a wheel. The card tracks the app's scroll handle and
    /// `paged_offset` is what Page Up / Page Down move it by.
    #[gpui::test]
    fn the_session_preview_pages_its_overflow_without_a_wheel(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        let viewport = gpui::size(px(960.), px(320.));
        cx.simulate_resize(viewport);
        let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
        // Long enough to overflow a 320px window several times over.
        let hover = SessionTooltip {
            title: "A session with a title long enough to wrap twice over".into(),
            summary: Some("And a summary beneath it that also wraps.".into()),
            shortcut: Some("Open ⌘2".into()),
            workspace: "A workspace with a long descriptive name".into(),
            branch: Some(format!("feature/{}", "long-branch-".repeat(8)).into()),
            ahead: Some("↑12".into()),
            behind: Some("↓3".into()),
            path: format!("/Users/developer/{}", "nested-directory/".repeat(12)).into(),
            providers: (0..6)
                .map(|index| sessions::SessionProvider {
                    provider: format!("provider-{index}"),
                    model: Some("long-model-identifier-".repeat(40)),
                })
                .collect(),
        };
        let scroll = gpui::ScrollHandle::new();
        let draw = |cx: &mut gpui::VisualTestContext, scroll: &gpui::ScrollHandle| {
            cx.draw(point(px(0.), px(0.)), viewport, |_, cx| {
                session_card_probe_with_scroll(&hover, &theme, None, scroll.clone(), cx)
            });
        };

        draw(cx, &scroll);
        let max = scroll.max_offset().height;
        assert!(
            max > px(0.),
            "the card tracks the handle and overflows: max {max:?}"
        );
        let footer_at_rest = cx
            .debug_bounds("session-card-shortcut")
            .expect("the shortcut footer paints");
        assert_eq!(scroll.offset().y, px(0.), "it opens at the top");

        let page = (scroll.bounds().size.height - px(24.)).max(px(40.));
        // Page Down, as the handler does it.
        scroll.set_offset(point(
            px(0.),
            paged_offset(scroll.offset().y, page, max, -1.0),
        ));
        draw(cx, &scroll);
        assert_eq!(scroll.offset().y, -page, "one page, from the top");
        let footer_paged = cx
            .debug_bounds("session-card-shortcut")
            .expect("the footer is still painted");
        assert!(
            footer_paged.origin.y < footer_at_rest.origin.y,
            "the content moved up: {footer_paged:?} against {footer_at_rest:?}"
        );

        // Paging past the end stops at the end, and paging back stops at the top.
        for _ in 0..20 {
            scroll.set_offset(point(
                px(0.),
                paged_offset(scroll.offset().y, page, max, -1.0),
            ));
        }
        draw(cx, &scroll);
        assert_eq!(scroll.offset().y, -max, "clamped at the end");
        for _ in 0..20 {
            scroll.set_offset(point(
                px(0.),
                paged_offset(scroll.offset().y, page, max, 1.0),
            ));
        }
        draw(cx, &scroll);
        assert_eq!(scroll.offset().y, px(0.), "clamped at the top");
    }

    #[gpui::test]
    fn a_preview_page_press_is_quiet_with_no_card_on_screen(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|_, cx| cx.set_global(Theme::for_id(crate::theme::ThemeId::Orbit)));
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        app.update(cx, |app, cx| {
            app.sidebar_cursor = None;
            app.clear_session_card(cx);
        });
        let viewport = cx.update(|window, _| window.viewport_size());
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                app.on_sidebar_preview_down(&crate::SidebarPreviewDown, window, cx);
                app.on_sidebar_preview_up(&crate::SidebarPreviewUp, window, cx);
            });
        });
        assert_eq!(
            app.read_with(cx, |app, _| app.session_card_scroll.offset().y),
            px(0.),
            "a chord with nothing on screen must not scroll an invisible view"
        );
    }

    #[gpui::test]
    fn session_card_summary_is_bounded_for_multiline_text(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.simulate_resize(gpui::size(px(960.), px(640.)));
        let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
        let mut hover = SessionTooltip {
            title: "Session".into(),
            summary: None,
            shortcut: None,
            workspace: "Workspace".into(),
            branch: None,
            ahead: None,
            behind: None,
            path: "/tmp".into(),
            providers: vec![],
        };
        let draw = |cx: &mut gpui::VisualTestContext, hover: &SessionTooltip| {
            cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(960.), px(640.)),
                |_, cx| session_card_probe(hover, &theme, None, cx),
            );
            cx.debug_bounds("session-card-summary")
        };
        assert!(draw(cx, &hover).is_none());
        hover.summary = session_card_summary("Session", &"long text ".repeat(500));
        let long_height = draw(cx, &hover).unwrap().size.height;
        assert!(long_height > px(0.) && long_height <= px(80.));
        hover.summary = session_card_summary("Session", &"long text\n".repeat(500));
        assert_eq!(draw(cx, &hover).unwrap().size.height, long_height);
    }

    #[gpui::test]
    fn session_card_all_provider_rows_remain_scrollable(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.simulate_resize(gpui::size(px(960.), px(640.)));
        let theme = Theme::for_id(crate::theme::ThemeId::Orbit);
        let mut hover = SessionTooltip {
            title: "Session".into(),
            summary: None,
            shortcut: None,
            workspace: "Workspace".into(),
            branch: None,
            ahead: None,
            behind: None,
            path: "/tmp".into(),
            providers: (0..4)
                .map(|_| sessions::SessionProvider {
                    provider: "provider".into(),
                    model: Some("model".into()),
                })
                .collect(),
        };
        let draw = |cx: &mut gpui::VisualTestContext, hover: &SessionTooltip| {
            cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(960.), px(640.)),
                |_, cx| session_card_probe(hover, &theme, None, cx),
            );
            assert!(cx.debug_bounds("session-card-providers").is_some());
            cx.debug_bounds("session-tooltip").unwrap().size.height
        };
        let four_height = draw(cx, &hover);
        hover.providers.resize(
            100,
            sessions::SessionProvider {
                provider: "provider".into(),
                model: Some("model".into()),
            },
        );
        assert!(draw(cx, &hover) >= four_height);
        assert!(draw(cx, &hover) <= px(640.));
    }

    /// Exercise the real GPUI layout, including unbroken machine text, rather
    /// than estimating text width. Optional groups must not leave empty slots.
    #[gpui::test]
    fn session_card_long_metadata_stays_grouped_inside_the_frame(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.simulate_resize(gpui::size(px(960.), px(640.)));
        for palette in [
            crate::theme::ThemeId::Orbit,
            crate::theme::ThemeId::OrbitLight,
        ] {
            let theme = Theme::for_id(palette);
            let mut hover = SessionTooltip {
                title: "A session with a longer title to inspect".into(),
                summary: None,
                shortcut: Some("Open ⌘2".into()),
                workspace: "A workspace with a long descriptive name".into(),
                branch: Some(format!("feature/{}", "long-branch-".repeat(8)).into()),
                ahead: Some("↑12".into()),
                behind: Some("↓3".into()),
                path: format!(
                    "/Users/developer/projects/{}",
                    "nested-directory/".repeat(6)
                )
                .into(),
                providers: (0..3)
                    .map(|index| sessions::SessionProvider {
                        provider: format!("provider-{index}"),
                        model: Some("long-model-identifier-".repeat(80)),
                    })
                    .collect(),
            };
            cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(960.), px(640.)),
                |_, cx| session_card_probe(&hover, &theme, None, cx),
            );
            let card = cx.debug_bounds("session-tooltip").unwrap();
            let location = cx.debug_bounds("session-card-location").unwrap();
            let path = cx.debug_bounds("session-card-path").unwrap();
            let branch = cx.debug_bounds("session-card-branch").unwrap();
            let providers = cx.debug_bounds("session-card-providers").unwrap();
            let shortcut = cx.debug_bounds("session-card-shortcut").unwrap();
            assert_eq!(card.size.width, tooltip::max_width(&theme) * 1.5);
            assert!(card.size.height <= px(640.));
            for bounds in [location, path, branch, providers, shortcut] {
                assert!(
                    bounds.left() >= card.left() && bounds.right() <= card.right(),
                    "{bounds:?} outside {card:?}"
                );
                // The scroll body retains complete values beyond the viewport.
            }
            assert!(path.bottom() <= branch.top());
            let group_gap = DynamicSpacing::Base12.px(&theme) - px(1.);
            assert!(providers.top() - location.bottom() >= group_gap);
            assert!(shortcut.top() - providers.bottom() >= group_gap);

            for provider in &mut hover.providers {
                provider.model = Some("short-model".into());
            }
            let short_cx = cx.add_empty_window();
            short_cx.simulate_resize(gpui::size(px(960.), px(640.)));
            short_cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(960.), px(640.)),
                |_, cx| session_card_probe(&hover, &theme, None, cx),
            );
            assert!(
                providers.size.height
                    > short_cx
                        .debug_bounds("session-card-providers")
                        .unwrap()
                        .size
                        .height,
                "long model labels wrap instead of losing their full values"
            );

            hover.branch = None;
            hover.ahead = None;
            hover.behind = None;
            hover.providers.clear();
            hover.shortcut = None;
            cx.draw(
                point(px(0.), px(0.)),
                gpui::size(px(960.), px(640.)),
                |_, cx| session_card_probe(&hover, &theme, None, cx),
            );
            assert!(cx.debug_bounds("session-card-branch").is_none());
            assert!(cx.debug_bounds("session-card-providers").is_none());
            assert!(cx.debug_bounds("session-card-shortcut").is_none());
            assert!(cx.debug_bounds("session-tooltip").unwrap().size.height < card.size.height);
        }
    }

    #[gpui::test]
    fn row_rename_opens_and_focusses_before_switching_sessions(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        cx.update(|window, cx| {
            app.update(cx, |app, cx| {
                let current = PathBuf::from("/tmp/current.jsonl");
                let target = PathBuf::from("/tmp/rename-target.jsonl");
                app.current_session_path = Some(current.clone());
                app.session_id = Some("current".into());
                app.session_name = Some("Current title".into());
                app.sessions.push(SessionInfo {
                    path: target.clone(),
                    id: "target".into(),
                    cwd: PathBuf::from("/tmp"),
                    title: "Target title".into(),
                    first_message: String::new(),
                    modified: std::time::SystemTime::UNIX_EPOCH,
                });
                let menu = SessionMenu {
                    path: target.clone(),
                    title: "Target title".into(),
                    deletable: true,
                    at: None,
                    reorderable: false,
                    can_move_up: false,
                    can_move_down: false,
                    cursor: Default::default(),
                };
                app.session_menu = Some(menu.clone());
                app.on_menu_rename_session(window, cx);
                assert!(app.session_name_editing);
                assert_eq!(
                    app.session_rename_target.as_ref().map(|row| &row.path),
                    Some(&target)
                );
                let field = app.session_name_input.read(cx);
                assert_eq!(field.text(), "Target title");
                assert!(field.focus_handle(cx).is_focused(window));
                assert_eq!(app.current_session_path.as_ref(), Some(&current));
                assert!(app.pending_session_rename.is_none());

                app.cancel_session_name_edit(cx);
                assert!(!app.session_name_editing);
                assert!(app.session_rename_target.is_none());
                assert_eq!(app.current_session_path.as_ref(), Some(&current));

                app.session_menu = Some(menu);
                app.on_menu_rename_session(window, cx);
                app.rename_session(cx); // Saving the untouched sidebar preview is a no-op.
                assert!(!app.session_name_editing);
                assert_eq!(app.current_session_path.as_ref(), Some(&current));
                assert!(app.pending_session_rename.is_none());
            });
        });
    }

    #[gpui::test]
    fn failed_session_switch_cancels_pending_row_rename(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(crate::theme::Theme::for_id(crate::theme::ThemeId::Orbit));
            let app = cx.new(OrbitApp::new);
            app.update(cx, |app, cx| {
                app.pending_session_rename = Some(SessionRename {
                    path: PathBuf::from("/tmp/rename-target.jsonl"),
                    name: "Saved title".into(),
                });
                let mut refresh = false;
                app.on_response(
                    "switch_session",
                    false,
                    None,
                    Some("failed"),
                    "test",
                    &mut refresh,
                    cx,
                );
                assert!(app.pending_session_rename.is_none());
                assert!(!app.session_name_editing);
            });
        });
    }

    #[test]
    fn cursor_clamps_to_the_visible_rows() {
        assert_eq!(clamp_cursor(0, 0), None);
        assert_eq!(clamp_cursor(3, 2), Some(1));
        assert_eq!(clamp_cursor(1, 5), Some(1));
    }

    /// A branch arrow appears only for a count that exists: `↑0 ↓0` is a row of
    /// arrows that says nothing.
    #[test]
    fn divergence_arrows_appear_only_for_a_non_zero_count() {
        assert_eq!(divergence_labels(None), (None, None));
        assert_eq!(divergence_labels(Some((0, 0))), (None, None));
        assert_eq!(
            divergence_labels(Some((2, 0))),
            (Some("↑2".to_string()), None)
        );
        assert_eq!(
            divergence_labels(Some((0, 1))),
            (None, Some("↓1".to_string()))
        );
        assert_eq!(
            divergence_labels(Some((3, 4))),
            (Some("↑3".to_string()), Some("↓4".to_string()))
        );
    }

    /// The lane is the sidebar's whole answer to "what is this row doing", so
    /// its mapping has to be exact: a live block outranks a stale unread mark,
    /// and a read result recedes instead of disappearing.
    #[test]
    fn status_lane_orders_a_block_over_an_unread_mark() {
        use StatusLane::*;
        assert_eq!(status_lane(Some(SessionStatus::Running), None), Progress);
        assert_eq!(status_lane(Some(SessionStatus::Retrying), None), Progress);
        assert_eq!(status_lane(Some(SessionStatus::Compacting), None), Progress);
        assert_eq!(
            status_lane(Some(SessionStatus::WaitingApproval), None),
            Blocked(BlockReason::Approval)
        );
        assert_eq!(
            status_lane(Some(SessionStatus::WaitingInput), None),
            Blocked(BlockReason::Input)
        );
        // A block is happening now; the unread mark describes a run that is
        // already over, so it cannot be allowed to win.
        assert_eq!(
            status_lane(Some(SessionStatus::WaitingApproval), Some(ReviewMark::Done)),
            Blocked(BlockReason::Approval)
        );
        assert_eq!(status_lane(Some(SessionStatus::Failed), None), Failed);
        assert_eq!(status_lane(Some(SessionStatus::Disconnected), None), Failed);
        assert_eq!(status_lane(Some(SessionStatus::Stopped), None), Stopped);
        assert_eq!(status_lane(Some(SessionStatus::Completed), None), Done);
        assert_eq!(
            status_lane(Some(SessionStatus::Completed), Some(ReviewMark::Done)),
            UnreadDone
        );
        // No process was ever observed (another client, an earlier launch): the
        // read mark is then all the row knows.
        assert_eq!(status_lane(None, Some(ReviewMark::Done)), UnreadDone);
        assert_eq!(status_lane(None, Some(ReviewMark::Failed)), UnreadFailed);
        assert_eq!(
            status_lane(Some(SessionStatus::Failed), Some(ReviewMark::Failed)),
            UnreadFailed
        );
        assert_eq!(status_lane(Some(SessionStatus::Idle), None), Idle);
        assert_eq!(status_lane(None, None), Unknown);
    }

    /// Attention states get words; quiet states leave room for the age.
    #[test]
    fn attention_states_get_words() {
        assert!(status_lane_words(StatusLane::Blocked(BlockReason::Approval)).is_some());
        assert!(status_lane_words(StatusLane::Blocked(BlockReason::Input)).is_some());
        assert!(status_lane_words(StatusLane::Failed).is_some());
        assert!(status_lane_words(StatusLane::UnreadDone).is_some());
        assert!(status_lane_words(StatusLane::UnreadFailed).is_some());
        for quiet in [
            StatusLane::Progress,
            StatusLane::Done,
            StatusLane::Stopped,
            StatusLane::Idle,
            StatusLane::Unknown,
        ] {
            assert_eq!(status_lane_words(quiet), None);
        }
    }

    #[test]
    fn review_mark_changes_icon_and_tooltip_when_result_is_opened() {
        let status = Some(SessionStatus::Completed);
        let unread = status_lane(status, Some(ReviewMark::Done));
        let viewed = status_lane(status, None);
        assert_eq!(unread, StatusLane::UnreadDone);
        assert_eq!(viewed, StatusLane::Done);
        assert_ne!(status_lane_icon(unread), status_lane_icon(viewed));
        assert_eq!(
            status_lane_tip(unread, status),
            Some(tr!("sidebar.review_finished"))
        );
        assert_eq!(
            status_lane_tip(viewed, status),
            Some(tr!("sidebar.status_viewed"))
        );

        let failed = status_lane(Some(SessionStatus::Failed), Some(ReviewMark::Failed));
        assert_eq!(
            status_lane_tip(failed, Some(SessionStatus::Failed)),
            Some(tr!("sidebar.review_failed"))
        );
        assert_eq!(
            status_lane_tip(StatusLane::Progress, Some(SessionStatus::Retrying)),
            Some(SessionStatus::Retrying.label())
        );
        assert_eq!(
            status_lane_tip(StatusLane::Failed, Some(SessionStatus::Disconnected)),
            Some(SessionStatus::Disconnected.label())
        );
        assert_eq!(status_lane_tip(StatusLane::Unknown, None), None);
    }

    /// Every lane that has a state paints a glyph from the icon set, a lane
    /// with nothing to say paints nothing, and the tooltip never invents a
    /// state the row does not know.
    #[test]
    fn only_lanes_with_a_state_paint_a_glyph() {
        for lane in [
            StatusLane::Progress,
            StatusLane::Blocked(BlockReason::Approval),
            StatusLane::Blocked(BlockReason::Input),
            StatusLane::UnreadDone,
            StatusLane::UnreadFailed,
            StatusLane::Done,
            StatusLane::Failed,
            StatusLane::Stopped,
        ] {
            let icon = status_lane_icon(lane).expect("a state needs a glyph");
            assert!(icon.starts_with("icons/"), "{lane:?} → {icon}");
        }
        assert_eq!(status_lane_icon(StatusLane::Idle), None);
        assert_eq!(status_lane_icon(StatusLane::Unknown), None);
        for lane in [
            StatusLane::Progress,
            StatusLane::Blocked(BlockReason::Approval),
            StatusLane::Blocked(BlockReason::Input),
            StatusLane::UnreadDone,
            StatusLane::UnreadFailed,
            StatusLane::Done,
            StatusLane::Failed,
            StatusLane::Stopped,
            StatusLane::Idle,
            StatusLane::Unknown,
        ] {
            if status_lane_tip(lane, None).is_some() {
                assert!(matches!(
                    lane,
                    StatusLane::UnreadDone | StatusLane::UnreadFailed | StatusLane::Done
                ));
            }
        }
        assert!(status_lane_tip(StatusLane::Progress, Some(SessionStatus::Running)).is_some());
    }
}
