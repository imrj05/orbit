//! Regression test: the new-task page's "Work in" picker draws one row per
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

/// A main checkout plus one linked worktree, so the picker shows the pinned
/// current folder, a real worktree, and the action rows.
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

    fn linked(&self) -> PathBuf {
        self.root.join(".worktrees/113")
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

#[gpui::test]
fn picker_rows_tile_at_one_two_line_height(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let fixture = Fixture::new();
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    let window_cx = cx.add_empty_window();
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true),
                worktree(&fixture.linked(), "113", false),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.workspace_picker = None;
            app.toggle_workspace_picker(window, cx);
        });
    });

    let _ = window_cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(420.), px(640.)),
        |_, cx| cx.new(|_| PickerProbe { app: app.clone() }),
    );

    let list = window_cx
        .debug_bounds("workspace-picker-list")
        .expect("the list laid out");
    let entries_group = window_cx
        .debug_bounds("workspace-rows")
        .expect("the folder rows laid out");
    let actions_group = window_cx
        .debug_bounds("workspace-actions")
        .expect("the action rows laid out");

    let entries = app.update(window_cx, |app, _cx| app.recent_workspaces().len());
    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let row_h = f32::from(picker::two_line_entry_height(&theme));
    assert!(
        entries >= 2,
        "the fixture contributes a folder and a worktree"
    );

    // Each group is exactly one two-line entry per row. A row whose text is
    // taller than the height the list assumes is the bug this pins: the group
    // would measure taller than its row count × the token.
    let expected_entries = entries as f32 * row_h;
    assert!(
        (f32::from(entries_group.size.height) - expected_entries).abs() <= 1.,
        "{entries} folder rows must tile at {row_h}px each: expected {expected_entries}, was {}",
        f32::from(entries_group.size.height)
    );
    let expected_actions = 4. * row_h;
    assert!(
        (f32::from(actions_group.size.height) - expected_actions).abs() <= 2.,
        "4 action rows tile at {row_h}px each (plus the hairline): expected ~{expected_actions}, was {}",
        f32::from(actions_group.size.height)
    );

    // The verbs sit under the folders, not on top of them.
    assert!(
        f32::from(actions_group.top()) >= f32::from(entries_group.bottom()) - 1.,
        "the actions must follow the folders: folders {entries_group:?}, actions {actions_group:?}"
    );

    // No scroll offset is applied when the picker opens, so the first folder
    // is fully inside the viewport.
    assert!(
        f32::from(entries_group.top()) >= f32::from(list.top()) - 1.,
        "the first row must be fully visible: list {list:?}, rows {entries_group:?}"
    );

    // The viewport is taller than one row, so it never shows a single clipped
    // row with nothing else.
    assert!(
        f32::from(list.size.height) >= row_h,
        "the list is at least one row tall: {}",
        f32::from(list.size.height)
    );
}

/// A filter that matches no folder keeps the verbs. "New worktree…" is exactly
/// what a search that found nothing should still offer, and dropping it is how
/// the old no-match branch took the way out away with the way in.
#[gpui::test]
fn a_filter_that_matches_nothing_keeps_the_action_rows(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let fixture = Fixture::new();
    let app = cx.update(|cx| cx.new(OrbitApp::new));
    let window_cx = cx.add_empty_window();
    window_cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.worktrees = vec![
                worktree(&fixture.repo(), "repo", true),
                worktree(&fixture.linked(), "113", false),
            ];
            app.worktree_repo_root = Some(fixture.repo());
            app.current_workspace = Some(fixture.repo());
            app.toggle_workspace_picker(window, cx);
            let picker = app.workspace_picker.clone().expect("picker open");
            picker.update(cx, |picker, cx| {
                picker.set_filter_text("no-such-folder", cx)
            });
        });
    });

    let _ = window_cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(420.), px(640.)),
        |_, cx| cx.new(|_| PickerProbe { app: app.clone() }),
    );

    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let row_h = f32::from(picker::two_line_entry_height(&theme));

    let rows = window_cx
        .debug_bounds("workspace-rows")
        .expect("the empty state laid out");
    let actions = window_cx
        .debug_bounds("workspace-actions")
        .expect("the verbs stayed");
    assert!(
        (f32::from(rows.size.height) - row_h).abs() <= 1.,
        "one no-match row: was {}",
        f32::from(rows.size.height)
    );
    assert!(
        (f32::from(actions.size.height) - 4. * row_h).abs() <= 2.,
        "all four verbs remain: was {}",
        f32::from(actions.size.height)
    );
}
