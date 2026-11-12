//! Sidebar session search (OPI-34).
//!
//! The query narrows the same list the archived filter produced, and both the
//! render path and ⌘↑/↓ navigation read that list — so a row the cursor can
//! reach is always a row the user can see.

use super::sidebar::*;
use super::*;
use std::path::PathBuf;
use std::time::SystemTime;

fn store_session(title: &str, first_message: &str, cwd: &str, id: &str) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(format!("/sessions/{id}.jsonl")),
        id: id.to_string(),
        cwd: PathBuf::from(cwd),
        title: title.to_string(),
        first_message: first_message.to_string(),
        modified: SystemTime::UNIX_EPOCH,
    }
}

#[test]
fn an_empty_query_matches_every_session() {
    let s = store_session(
        "Refactor the parser",
        "split the lexer out",
        "/work/orbit",
        "abc123",
    );
    assert!(session_matches(&s, ""));
    assert!(session_matches(&s, "   "));
}

#[test]
fn the_query_matches_title_preview_workspace_and_id() {
    let s = store_session(
        "Refactor the parser",
        "split the lexer out",
        "/work/orbit",
        "abc123",
    );
    // Title.
    assert!(session_matches(&s, "parser"));
    // First message.
    assert!(session_matches(&s, "lexer"));
    // Workspace label (the folder name the sidebar paints).
    assert!(session_matches(&s, "orbit"));
    // Session id, so a pasted identifier still finds its session.
    assert!(session_matches(&s, "abc12"));
    // Nothing in the row.
    assert!(!session_matches(&s, "deploy"));
}

#[test]
fn matching_is_case_insensitive_and_ignores_surrounding_space() {
    let s = store_session("Refactor the parser", "", "/work/orbit", "abc123");
    assert!(session_matches(&s, "REFACTOR"));
    assert!(session_matches(&s, "  Parser  "));
}

#[test]
fn filtering_composes_with_the_archived_filter() {
    // The search runs after the archived filter, so "archived only" plus a
    // query means "archived sessions that match" — not every match.
    let kept = store_session("Keep me", "", "/work/a", "1");
    let archived = store_session("Archived match", "", "/work/a", "2");
    let sessions = vec![kept.clone(), archived.clone()];
    let archived_paths: HashSet<PathBuf> = [archived.path.clone()].into_iter().collect();

    let shown = filter_sidebar_sessions(
        sessions.clone(),
        SidebarArchivedFilter::Hide,
        &archived_paths,
        "match",
    );
    assert!(shown.is_empty(), "a hidden archived match must not surface");

    let only = filter_sidebar_sessions(
        sessions,
        SidebarArchivedFilter::Only,
        &archived_paths,
        "match",
    );
    assert_eq!(only.len(), 1);
    assert_eq!(only[0].id, "2");
}

#[test]
fn search_results_and_row_indices_use_the_filtered_snapshot() {
    let sessions = vec![
        store_session("Unrelated", "", "/work/a", "first"),
        store_session("Matching result", "", "/work/b", "second"),
        store_session("Another match", "", "/work/c", "third"),
    ];
    let filtered = filter_sidebar_sessions(
        sessions.clone(),
        SidebarArchivedFilter::Show,
        &HashSet::new(),
        "match",
    );
    let rows = build_flat_sidebar_rows(&filtered, &HashSet::new(), &None, &HashSet::new());
    let ids: Vec<_> = rows
        .iter()
        .filter_map(|row| match row {
            SideRow::Session(ix) => filtered.get(*ix).map(|session| session.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, ["second", "third"]);
    assert!(filter_sidebar_sessions(
        sessions.clone(),
        SidebarArchivedFilter::Show,
        &HashSet::new(),
        "no such result",
    )
    .is_empty());
    assert_eq!(
        filter_sidebar_sessions(sessions, SidebarArchivedFilter::Show, &HashSet::new(), "").len(),
        3,
        "clearing the query restores the list"
    );
}

#[gpui::test]
fn search_input_and_navigation_use_matching_sessions(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.workspaces = vec![PathBuf::from("/work/sidebar-search-test")];
            app.sessions = vec![
                store_session("Unrelated", "", "/work/sidebar-search-test", "first"),
                store_session("Matching result", "", "/work/sidebar-search-test", "second"),
            ];
            app.current_session_path = None;
            app.sidebar_archived_filter = SidebarArchivedFilter::Show;
            app.sidebar_search
                .update(cx, |input, cx| input.set_text("MATCH", cx));
        });
    });
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            assert_eq!(app.sidebar_query, "match");
            let visible = app.sidebar_sessions_filtered();
            assert_eq!(visible.len(), 1);
            assert_eq!(visible[0].id, "second");
            let navigable = app.navigable_sessions();
            assert_eq!(navigable.len(), 1);
            assert_eq!(navigable[0].id, "second");
        });
    });
}

#[gpui::test]
fn session_search_shortcut_focuses_the_sidebar_input(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        crate::bind_keys(cx);
    });
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            app.settings_open = false;
            app.sidebar_visible = true;
        });
    });
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(820.)),
        |_, _| app.clone(),
    );
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.input.read(cx).focus(window));
    });
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1200.), px(820.)),
        |_, _| app.clone(),
    );
    cx.simulate_keystrokes(crate::platform::shortcuts::SESSION_SEARCH);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            assert!(
                app.sidebar_search
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window),
                "the advertised shortcut must focus the session search field"
            );
        });
    });
}

#[gpui::test]
fn holding_the_session_modifier_reveals_and_hides_number_hints(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};
    use gpui::Modifiers;

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            for dependency in &mut app.deps {
                dependency.installed = true;
            }
            app.setup_open = false;
            app.settings_open = false;
            app.sidebar_visible = true;
            app.sidebar_group_by = SidebarGroupBy::OneList;
            app.sidebar_archived_filter = SidebarArchivedFilter::Show;
            app.workspaces = vec![PathBuf::from("/work/sidebar-shortcut-hints")];
            app.sessions = vec![
                store_session("First", "", "/work/sidebar-shortcut-hints", "first"),
                store_session("Second", "", "/work/sidebar-shortcut-hints", "second"),
            ];
            app.current_session_path = None;
        });
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(1200.), px(820.)),
            |_, _| app.clone(),
        );
    };
    draw(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.input.read(cx).focus(window));
    });
    draw(cx);
    assert!(cx.debug_bounds("session-shortcut-1").is_none());

    let mut modifier = Modifiers::none();
    if cfg!(target_os = "macos") {
        modifier.platform = true;
    } else {
        modifier.control = true;
    }
    cx.simulate_modifiers_change(modifier);
    draw(cx);
    assert!(cx.debug_bounds("session-shortcut-1").is_some());
    assert!(cx.debug_bounds("session-shortcut-2").is_some());
    assert!(cx.debug_bounds("session-shortcut-3").is_none());

    cx.simulate_modifiers_change(Modifiers::none());
    draw(cx);
    assert!(!cx.update(|_, cx| app.read(cx).sidebar_shortcut_hints));
    cx.simulate_modifiers_change(modifier);
    draw(cx);
    assert!(cx.debug_bounds("session-shortcut-1").is_some());

    modifier.shift = true;
    cx.simulate_modifiers_change(modifier);
    draw(cx);
    assert!(!cx.update(|_, cx| app.read(cx).sidebar_shortcut_hints));

    cx.simulate_modifiers_change(Modifiers::none());
    draw(cx);
    assert!(!cx.update(|_, cx| app.read(cx).sidebar_shortcut_hints));
}

// ── Recent view ────────────────────────────────────────────────────────────

fn aged(name: &str, cwd: &str, secs: u64) -> SessionInfo {
    SessionInfo {
        path: PathBuf::from(format!("/store/{name}.jsonl")),
        id: name.into(),
        cwd: PathBuf::from(cwd),
        title: name.into(),
        first_message: String::new(),
        modified: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
    }
}

#[test]
fn recent_pins_first_without_promoting_open_or_running() {
    let sessions = vec![
        aged("open", "/work/a", 10),
        aged("running", "/work/a", 20),
        aged("idle", "/work/a", 30),
        aged("pin", "/work/a", 5),
    ];
    let pinned = HashSet::from([sessions[3].path.clone()]);
    let active = Some(sessions[0].path.clone());
    let running = HashSet::from([sessions[1].path.clone()]);
    let rows = build_recent_sidebar_rows(&sessions, &pinned, &active, &running);
    let order: Vec<usize> = rows
        .into_iter()
        .filter_map(|row| match row {
            SideRow::Session(ix) => Some(ix),
            _ => None,
        })
        .collect();
    assert_eq!(order, vec![3, 2, 1, 0]);
}

#[test]
fn recent_lists_sessions_by_newest_activity() {
    let sessions = vec![
        aged("old", "/work/a", 10),
        aged("fresh", "/work/b", 90),
        aged("mid", "/work/a", 50),
    ];
    let pinned: HashSet<PathBuf> = HashSet::new();
    let rows = build_recent_sidebar_rows(&sessions, &pinned, &None, &HashSet::new());
    let order: Vec<usize> = rows
        .into_iter()
        .map(|row| match row {
            SideRow::Session(ix) => ix,
            _ => panic!("recent rows are sessions"),
        })
        .collect();
    assert_eq!(order, vec![1, 2, 0], "newest activity first");
}

#[test]
fn recent_keeps_every_pinned_open_and_running_session() {
    let sessions = vec![
        aged("pinned-old", "/work/a", 1),
        aged("open-old", "/work/a", 2),
        aged("running-old", "/work/a", 3),
        aged("fresh", "/work/a", 99),
    ];
    let pinned: HashSet<PathBuf> = [sessions[0].path.clone()].into_iter().collect();
    let running: HashSet<PathBuf> = [sessions[2].path.clone()].into_iter().collect();
    let active = Some(sessions[1].path.clone());
    let rows = build_recent_sidebar_rows(&sessions, &pinned, &active, &running);
    let listed: HashSet<usize> = rows
        .into_iter()
        .filter_map(|row| match row {
            SideRow::Session(ix) => Some(ix),
            _ => None,
        })
        .collect();
    assert!(listed.contains(&0), "a pin is always listed");
    assert!(listed.contains(&1), "the open session is always listed");
    assert!(listed.contains(&2), "a running session is always listed");
    assert!(
        listed.contains(&3),
        "the newest untouched session is listed"
    );
}

#[test]
fn recent_caps_the_untouched_tail() {
    let mut sessions: Vec<SessionInfo> = (0..20)
        .map(|ix| aged(&format!("s{ix}"), "/work/a", ix as u64))
        .collect();
    // One leading session so the cap applies to the tail only.
    let pinned: HashSet<PathBuf> = [sessions[0].path.clone()].into_iter().collect();
    sessions.reverse();
    let rows = build_recent_sidebar_rows(&sessions, &pinned, &None, &HashSet::new());
    assert_eq!(
        rows.len(),
        1 + SIDEBAR_RECENT_OTHERS,
        "one pinned session plus the capped tail"
    );
}

#[test]
fn recent_breaks_ties_by_path_so_frames_cannot_swap_rows() {
    let mut a = aged("a", "/work/a", 42);
    let mut b = aged("b", "/work/b", 42);
    a.path = PathBuf::from("/store/a.jsonl");
    b.path = PathBuf::from("/store/b.jsonl");
    let forward = build_recent_sidebar_rows(
        &[a.clone(), b.clone()],
        &HashSet::new(),
        &None,
        &HashSet::new(),
    );
    let reverse = build_recent_sidebar_rows(
        &[b.clone(), a.clone()],
        &HashSet::new(),
        &None,
        &HashSet::new(),
    );
    // Same wall-clock second: the path decides, so both input orders list the
    // same session first — `/store/a.jsonl` — whichever index it holds.
    let first_path = |sessions: &[SessionInfo], rows: Vec<SideRow>| match rows[0] {
        SideRow::Session(ix) => sessions[ix].path.clone(),
        _ => panic!("expected a session"),
    };
    assert_eq!(forward.len(), reverse.len());
    let forward_path = first_path(&[a.clone(), b.clone()], forward);
    let reverse_path = first_path(&[b, a], reverse);
    assert_eq!(forward_path.as_os_str(), "/store/a.jsonl");
    assert_eq!(reverse_path.as_os_str(), "/store/a.jsonl");
}

// ── Responsive sidebar ─────────────────────────────────────────────────────

#[test]
fn the_sidebar_yields_before_the_workspace_squashes() {
    use gpui::px;
    // Wide enough: the user's toggle decides.
    assert!(super::view::sidebar_shows(true, false, px(1200.)));
    // The user hid it.
    assert!(!super::view::sidebar_shows(false, false, px(1200.)));
    // A full-window surface owns the screen.
    assert!(!super::view::sidebar_shows(true, true, px(1200.)));
    // Narrow: the sidebar yields whatever the toggle says, so the workspace
    // keeps a usable column; ⌘⇧B still brings the list back.
    assert!(!super::view::sidebar_shows(true, false, px(720.)));
}
