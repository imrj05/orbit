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
    // The machine group only exists while a host is configured; on a machine
    // without `remote-hosts.json` entries the group is omitted entirely.
    let actions_group = window_cx.debug_bounds("workspace-actions");

    let entries = app.update(window_cx, |app, _cx| app.workspace_entries().len());
    let theme = window_cx.update(|_window, cx| *theme::get(cx));
    let row_h = f32::from(picker::two_line_entry_height(&theme));
    assert_eq!(entries, 1, "the picker lists this device, once");

    // Each group is exactly one two-line entry per row. A row whose text is
    // taller than the height the list assumes is the bug this pins: the group
    // would measure taller than its row count × the token.
    let expected_entries = entries as f32 * row_h;
    assert!(
        (f32::from(entries_group.size.height) - expected_entries).abs() <= 1.,
        "{entries} folder rows must tile at {row_h}px each: expected {expected_entries}, was {}",
        f32::from(entries_group.size.height)
    );
    let action_rows = expected_action_rows();
    if action_rows == 1 {
        // No configured host: the "new project" verb is still there, so the
        // machine group exists with exactly that one row.
        let actions_group = actions_group.expect("the verb row laid out");
        let expected_actions = row_h + 1.; // one row plus its hairline
        assert!(
            (f32::from(actions_group.size.height) - expected_actions).abs() <= 2.,
            "with no configured host only the new-project verb remains: expected ~{expected_actions}, was {}",
            f32::from(actions_group.size.height)
        );
    } else {
        let actions_group = actions_group.expect("the action rows laid out");
        let expected_actions = action_rows as f32 * row_h + 1.;
        assert!(
            (f32::from(actions_group.size.height) - expected_actions).abs() <= 2.,
            "{action_rows} action rows tile at {row_h}px each (plus the hairline): expected ~{expected_actions}, was {}",
            f32::from(actions_group.size.height)
        );

        // The verbs sit under the folders, not on top of them.
        assert!(
            f32::from(actions_group.top()) >= f32::from(entries_group.bottom()) - 1.,
            "the actions must follow the folders: folders {entries_group:?}, actions {actions_group:?}"
        );
    }

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
    let actions = window_cx.debug_bounds("workspace-actions");
    assert!(
        (f32::from(rows.size.height) - row_h).abs() <= 1.,
        "one no-match row: was {}",
        f32::from(rows.size.height)
    );
    let action_rows = expected_action_rows();
    if action_rows == 1 {
        // No configured host: the "new project" verb still keeps the group.
        let actions = actions.expect("the verb row stayed");
        let expected_actions = row_h + 1.; // one row plus its hairline
        assert!(
            (f32::from(actions.size.height) - expected_actions).abs() <= 2.,
            "with no configured host only the new-project verb remains: was {}",
            f32::from(actions.size.height)
        );
    } else {
        let actions = actions.expect("the verbs stayed");
        assert!(
            (f32::from(actions.size.height) - (action_rows as f32 * row_h + 1.)).abs() <= 2.,
            "all {action_rows} machine rows remain: was {}",
            f32::from(actions.size.height)
        );
    }
}

/// The machine rows the picker builds: one row per enabled remote host, then
/// "Add this project to a server…" when there is a host (the picker is open
/// over a project), then "New project". Hosts come from the machine's own
/// `remote-hosts.json`, so the count is read, not assumed; `1` means no host.
fn expected_action_rows() -> usize {
    let hosts: Vec<_> = crate::remote_hosts::load()
        .0
        .into_iter()
        .filter(|host| host.enabled)
        .collect();
    let add_to_server = usize::from(!hosts.is_empty());
    OrbitApp::remote_actions(&hosts).len() + add_to_server + 1
}

/// Open the picker over a repository with one linked worktree: enough folder
/// and verb rows that the list scrolls.
fn open_scrolling_picker<'a>(
    cx: &'a mut gpui::TestAppContext,
    fixture: &Fixture,
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
            // The machine's own host list would decide how many rows there
            // are; a layout test needs a list long enough to scroll.
            let picker = app.workspace_picker.clone().expect("picker open");
            let entries = app.workspace_entries();
            let actions: Vec<crate::workspace_picker::PickerAction> = (0..10)
                .map(|ix| crate::workspace_picker::PickerAction {
                    id: "remote-host",
                    icon: "icons/server-stack.svg",
                    label: format!("host-{ix}"),
                    hint: Some(format!("ssh:/srv/app-{ix}")),
                    group_start: ix == 0,
                    mono: true,
                })
                .collect();
            picker.update(cx, |picker, cx| picker.set_rows(entries, actions, cx));
        });
    });
    draw_picker(window_cx, &app);
    (app, window_cx)
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
/// the list back to the first folder and it flipped between the folders and
/// the verbs instead of scrolling.
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

/// Arrowing down to the last verb shows all of it. The reveal used to model
/// the list as rows × height, missing the list's padding and the group
/// hairlines, so it capped the scroll short of the last row.
#[gpui::test]
fn arrowing_to_the_last_verb_reveals_it_whole(cx: &mut gpui::TestAppContext) {
    let fixture = Fixture::new();
    let (app, cx) = open_scrolling_picker(cx, &fixture);
    // The scrolling fixture swaps in ten host rows, so the last machine row is
    // the ninth of them (the picker's own list has no setup row anymore).
    let (entries, actions) = app.update(cx, |app, _| (app.workspace_entries().len(), 10));
    for _ in 0..(entries + actions - 1) {
        cx.dispatch_action(crate::PickerSelectNext);
        draw_picker(cx, &app);
    }
    let list = cx.debug_bounds("workspace-picker-list").expect("list");
    // `debug_bounds` wants a `'static` selector.
    let last_selector: &'static str =
        Box::leak(format!("workspace-action-{}", actions - 1).into_boxed_str());
    let last = cx
        .debug_bounds(last_selector)
        .expect("the last verb laid out");
    assert!(
        last.top() >= list.top() - px(0.5) && last.bottom() <= list.bottom() + px(0.5),
        "the last verb is fully inside the list: {last:?} in {list:?}"
    );

    // And back up to the first folder brings the top of the list back.
    for _ in 0..(entries + actions - 1) {
        cx.dispatch_action(crate::PickerSelectPrev);
        draw_picker(cx, &app);
    }
    let first = cx.debug_bounds("workspace-rows").expect("rows");
    assert!(
        first.top() >= list.top() - px(0.5),
        "the first folder is visible again: {first:?} in {list:?}"
    );
}
