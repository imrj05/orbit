//! Regression test: the new-task page's folder picker draws one row per
//! two-line entry box and never lets a row's text spill into the row below.
//!
//! The rows used to be pinned to the *one-line* picker height while carrying two
//! lines of text (a title and a hint), so every hint painted over the next row's
//! title and a keyboard move scrolled the list to the wrong offset. The fix was
//! to give the whole list one height — `picker::two_line_entry_height` — and
//! fixed line boxes inside it. This test holds that shape: rows are exactly that
//! tall, they tile without overlapping, and they start at the top of the list.

use super::*;
use crate::theme::tokens::picker;
use crate::theme::{Theme, ThemeId};
use crate::ui::prelude::*;
use crate::worktree::Worktree;
use std::fs;

struct PickerProbe {
    app: Entity<OrbitApp>,
}

impl Render for PickerProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let popup = self.app.update(cx, |app, _cx| {
            app.workspace_picker_popup(&crate::theme::Theme::dark())
        });
        div().w(px(420.)).h(px(640.)).children(popup)
    }
}

struct ScrollingRowProbe;

impl Render for ScrollingRowProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *theme::get(cx);
        div()
            .id("scrolling-row-probe")
            .w(px(300.))
            .h(px(90.))
            .overflow_y_scroll()
            .child(div().h(px(30.)))
            .child(
                picker_entry(div(), &theme)
                    .h(picker::two_line_entry_height(&theme))
                    .child(
                        PickerRow::new("Local", theme.text_primary, theme.text_muted)
                            .icon("icons/monitor.svg", theme.text_muted)
                            .detail("main")
                            .into_element(&theme),
                    ),
            )
            .child(div().h(px(300.)))
    }
}

#[gpui::test]
fn picker_icon_and_title_move_together_when_scrolled(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let probe = cx.update(|_, cx| cx.new(|_| ScrollingRowProbe));
    let draw = |cx: &mut gpui::VisualTestContext| {
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(px(300.), px(90.)),
            |_, _| probe.clone(),
        );
    };
    draw(cx);
    let icon_before = cx.debug_bounds("picker-row-leading").expect("icon");
    let title_before = cx.debug_bounds("picker-row-title").expect("title");
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(150.), px(45.)),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-15.))),
        ..Default::default()
    });
    draw(cx);
    let icon_after = cx
        .debug_bounds("picker-row-leading")
        .expect("scrolled icon");
    let title_after = cx.debug_bounds("picker-row-title").expect("scrolled title");
    let icon_delta = icon_after.top() - icon_before.top();
    let title_delta = title_after.top() - title_before.top();
    assert!(icon_delta.abs() > px(1.), "the test must actually scroll");
    assert!(
        (icon_delta - title_delta).abs() < px(0.5),
        "icon and title must scroll together"
    );
    assert!(
        (icon_after.center().y - title_after.center().y).abs() < px(1.),
        "icon and title share a line"
    );
}

fn worktree(path: &Path, name: &str, is_main: bool) -> Worktree {
    Worktree {
        path: path.to_path_buf(),
        name: name.into(),
        head: "0".repeat(40),
        branch: Some(name.into()),
        detached: false,
        is_main,
        bare: false,
        locked: false,
        lock_reason: None,
        prunable: false,
        prune_reason: None,
    }
}

/// A main checkout plus one linked worktree on disk: the worktree must not
/// leak into the folder picker's rows.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "orbit-picker-layout-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".git/worktrees/113")).expect("fake repo");
        let linked = root.join(".worktrees/113");
        fs::create_dir_all(&linked).expect("worktree dir");
        fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", root.join(".git/worktrees/113").display()),
        )
        .expect("gitfile");
        Self { root }
    }

    fn repo(&self) -> PathBuf {
        self.root.clone()
    }

}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[gpui::test]
fn new_task_keyboard_hints_fit_the_picker_at_maximum_ui_scale(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.ui_font_size = 20.;
        theme.ui.spacing_density = 120;
        cx.set_global(theme);
    });
    let fixture = Fixture::new();
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    let window_cx = cx.add_empty_window();
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.toggle_workspace_picker(window, cx);
        })
    });
    let _ = window_cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(420.), px(640.)),
        |_, cx| cx.new(|_| PickerProbe { app: app.clone() }),
    );
    let popup = window_cx
        .debug_bounds("workspace-picker-popup")
        .expect("picker");
    let footer = window_cx
        .debug_bounds("working-folder-keyboard-hints")
        .expect("hints");
    assert!(footer.left() >= popup.left() - px(1.) && footer.right() <= popup.right() + px(1.));
    assert!(footer.top() >= popup.top() && footer.bottom() <= popup.bottom() + px(1.));
    assert!(window_cx
        .debug_bounds("working-folder-hint-close")
        .is_some());
    assert!(window_cx.debug_bounds("working-folder-hint-back").is_none());
    for id in [
        "working-folder-hint-navigate",
        "working-folder-hint-select",
        "working-folder-hint-close",
    ] {
        let group = window_cx.debug_bounds(id).expect(id);
        assert!(
            group.left() >= footer.left() - px(1.)
                && group.right() <= footer.right() + px(1.)
                && group.bottom() <= footer.bottom() + px(1.),
            "{id} overflows: {group:?} in {footer:?}"
        );
    }
}

/// A folder row as the picker lists it.
fn folder_entry(path: PathBuf, note: Option<&str>) -> WorkspaceEntry {
    WorkspaceEntry {
        name: sessions::workspace_label(&path),
        path,
        note: note.map(str::to_string),
    }
}

/// Open the picker over the fixture repository, then swap in `rows` so the
/// layout does not depend on the machine's session store.
fn open_picker_with<'a>(
    cx: &'a mut gpui::TestAppContext,
    fixture: &Fixture,
    rows: usize,
    filter: Option<&str>,
) -> (Entity<OrbitApp>, &'a mut gpui::VisualTestContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    let window_cx = cx.add_empty_window();
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![worktree(&fixture.repo(), "repo", true)];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.workspace_picker = None;
            app.toggle_workspace_picker(window, cx);
            let picker = app.workspace_picker.clone().expect("picker open");
            let mut entries = vec![folder_entry(fixture.repo(), None)];
            entries.extend(
                (1..rows).map(|ix| folder_entry(PathBuf::from(format!("/srv/app-{ix}")), Some("2d"))),
            );
            picker.update(cx, |picker, cx| {
                picker.set_rows(entries, cx);
                if let Some(filter) = filter {
                    picker.set_filter_text(filter, cx);
                }
            });
        });
    });
    draw_picker(window_cx, &app);
    (app, window_cx)
}

#[gpui::test]
fn picker_rows_tile_at_one_two_line_height(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new();
    let (_app, window_cx) = open_picker_with(cx, &fixture, 2, None);

    let list = window_cx
        .debug_bounds("workspace-picker-list")
        .expect("the list laid out");
    let entries_group = window_cx
        .debug_bounds("workspace-rows")
        .expect("the folder rows laid out");
    assert!(
        window_cx.debug_bounds("workspace-actions").is_none(),
        "the folder picker has no machine or worktree rows"
    );

    let entries = 2;
    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let row_h = f32::from(picker::two_line_entry_height(&theme));

    // The group is exactly one two-line entry per row. A row whose text is
    // taller than the height the list assumes is the bug this pins: the
    // group would measure taller than its row count × the token.
    let gap = f32::from(picker::row_gap(&theme));
    let expected_entries = entries as f32 * row_h + (entries as f32 - 1.) * gap;

    // Rows never touch: a row gap apart, so a highlight is one rounded row,
    // not a slab joined to the next.
    let next = window_cx
        .debug_bounds("workspace-entry-1")
        .expect("the second folder row");
    let first = window_cx
        .debug_bounds("workspace-entry-0")
        .expect("the current folder row");
    assert!(
        f32::from(next.top() - first.bottom()) >= gap - 0.5,
        "a row gap between rows: {first:?} then {next:?}"
    );

    // The current folder's check sits on its title line — the top of the
    // row's content — not centred between title and path.
    let slot = window_cx
        .debug_bounds("workspace-entry-slot-0")
        .expect("its trailing slot");
    let pad = crate::theme::tokens::picker::ROW_SPACING.padding_y(&theme);
    assert!(
        (slot.top() - (first.top() + pad)).abs() <= px(1.),
        "the trailing slot rides the title line: slot {slot:?} in row {first:?}"
    );
    assert!(
        (f32::from(entries_group.size.height) - expected_entries).abs() <= 1.,
        "{entries} folder rows must tile at {row_h}px each: expected {expected_entries}, was {}",
        f32::from(entries_group.size.height)
    );

    // No scroll offset is applied when the picker opens, so the first folder
    // is fully inside the viewport.
    assert!(
        f32::from(entries_group.top()) >= f32::from(list.top()) - 1.,
        "the first row must be fully visible: list {list:?}, rows {entries_group:?}"
    );
    assert!(
        f32::from(list.size.height) >= row_h,
        "the list is at least one row tall: {}",
        f32::from(list.size.height)
    );
}

/// A filter that matches no folder draws one muted no-match row; "Choose
/// folder…" stays in the footer as the way out.
#[gpui::test]
fn a_filter_that_matches_nothing_shows_one_no_match_row(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new();
    let (_app, window_cx) = open_picker_with(cx, &fixture, 3, Some("no-such-folder"));
    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let row_h = f32::from(picker::two_line_entry_height(&theme));
    let rows = window_cx
        .debug_bounds("workspace-rows")
        .expect("the empty state laid out");
    assert!(
        (f32::from(rows.size.height) - row_h).abs() <= 1.,
        "one no-match row: was {}",
        f32::from(rows.size.height)
    );
}

/// Enough folders that the list scrolls.
const SCROLLING_ROWS: usize = 12;

fn open_scrolling_picker<'a>(
    cx: &'a mut gpui::TestAppContext,
    fixture: &Fixture,
) -> (Entity<OrbitApp>, &'a mut gpui::VisualTestContext) {
    open_picker_with(cx, fixture, SCROLLING_ROWS, None)
}

fn draw_picker(cx: &mut gpui::VisualTestContext, app: &Entity<OrbitApp>) {
    let app = app.clone();
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(420.), px(640.)),
        move |_, cx| cx.new(|_| PickerProbe { app: app.clone() }),
    );
}

/// The wheel scrolls the list and the list stays where it was put. Render
/// used to re-reveal the highlighted row every frame, so each redraw pulled
/// the list back to the first folder instead of scrolling.
#[gpui::test]
fn a_wheel_scroll_stays_where_it_was_put(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new();
    let (app, cx) = open_scrolling_picker(cx, &fixture);
    let list = cx.debug_bounds("workspace-picker-list").expect("list");
    let rows_before = cx.debug_bounds("workspace-rows").expect("rows");

    cx.simulate_event(gpui::ScrollWheelEvent {
        position: list.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-60.))),
        ..Default::default()
    });
    draw_picker(cx, &app);
    let rows_scrolled = cx.debug_bounds("workspace-rows").expect("rows");
    assert!(
        rows_scrolled.top() < rows_before.top() - px(10.),
        "the wheel must move the list: {rows_before:?} → {rows_scrolled:?}"
    );

    // Later frames (a caret blink, a hover) must not snap it back.
    draw_picker(cx, &app);
    draw_picker(cx, &app);
    let rows_after = cx.debug_bounds("workspace-rows").expect("rows");
    assert!(
        (rows_after.top() - rows_scrolled.top()).abs() < px(0.5),
        "the list stays scrolled across redraws: {rows_scrolled:?} → {rows_after:?}"
    );
}

/// Arrowing down to the last folder shows all of it. The reveal used to model
/// the list as rows × height, missing the list's padding and row gaps, so it
/// capped the scroll short of the last row.
#[gpui::test]
fn arrowing_to_the_last_folder_reveals_it_whole(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new();
    let (app, cx) = open_scrolling_picker(cx, &fixture);
    for _ in 0..(SCROLLING_ROWS - 1) {
        cx.dispatch_action(crate::PickerSelectNext);
        draw_picker(cx, &app);
    }
    let list = cx.debug_bounds("workspace-picker-list").expect("list");
    // `debug_bounds` wants a `'static` selector.
    let last_selector: &'static str =
        Box::leak(format!("workspace-entry-{}", SCROLLING_ROWS - 1).into_boxed_str());
    let last = cx
        .debug_bounds(last_selector)
        .expect("the last folder laid out");
    assert!(
        last.top() >= list.top() - px(0.5) && last.bottom() <= list.bottom() + px(0.5),
        "the last folder is fully inside the list: {last:?} in {list:?}"
    );

    // And back up to the first folder brings the top of the list back.
    for _ in 0..(SCROLLING_ROWS - 1) {
        cx.dispatch_action(crate::PickerSelectPrev);
        draw_picker(cx, &app);
    }
    let first = cx.debug_bounds("workspace-rows").expect("rows");
    assert!(
        first.top() >= list.top() - px(0.5),
        "the first folder is visible again: {first:?} in {list:?}"
    );
}
