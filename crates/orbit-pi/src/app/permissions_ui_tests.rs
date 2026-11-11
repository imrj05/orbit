//! Settings → Permissions surface coverage.
//!
//! The store's parsing/writing is unit-tested in `crate::permissions`, and the
//! enforcement is `node --test`-covered in
//! `contrib/orbit-guard-extension/`. These tests cover the glue those cannot
//! see: the page paints at the minimum window size with rows, children and the
//! inline pattern form mounted, the scope chips switch what the page edits, and
//! the form rejects an empty pattern without persisting anything.
//!
//! The one-click persistence path (`set_family_action` → `Rules::save`) is a
//! thin wrapper over code covered in `crate::permissions::tests`; it is not
//! exercised here because a test that ran it would rewrite the real
//! `~/.orbit-pi/access-rules.json`.

use super::*;
use crate::permissions::{Action, Rules, Scope, Target};
use crate::theme::{Theme, ThemeId};

/// `debug_bounds` takes `&'static str`; these tests name rows by index.
fn name(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

fn open_page(cx: &mut gpui::TestAppContext) -> gpui::Entity<OrbitApp> {
    let app_ctx = cx;
    app_ctx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = app_ctx.add_empty_window();
    let app = cx.update(|_, cx| cx.new(OrbitApp::new));
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.settings_open = true;
            // The mode is read from the machine's own store at launch; pin it
            // so "inherits" has one known meaning.
            app.access_mode = AccessMode::Supervised;
            // Opening the page re-reads the store and the agent files, so the
            // fixture is applied after it — the order a user sees when the page
            // is opened over an existing file.
            app.set_settings_section(SettingsSection::Permissions, cx);
            app.permission_rules = Rules::from_json(
                r#"{"version":1,"rules":[
                    {"scope":"any","target":"exec","pattern":"rm *","action":"deny"},
                    {"scope":"agent:worker","target":"exec","pattern":"","action":"allow"}
                ]}"#,
            );
            app.permission_agents = vec!["scout".to_string(), "worker".to_string()];
            cx.notify();
        });
    });
    app
}

#[gpui::test]
fn the_permissions_page_renders_the_scope_chips_rows_children_and_form(
    cx: &mut gpui::TestAppContext,
) {
    let cx = cx.add_empty_window();
    let app = open_page(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| app.open_pattern_editor(Target::Edit, cx));
    });
    let viewport = gpui::size(px(960.), px(640.));
    cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

    let title = cx
        .debug_bounds("settings-page-title")
        .expect("the Permissions header rendered");
    assert!(title.size.width > px(0.), "{title:?}");
    // The scope chips are the page's one navigation control.
    let chip = cx
        .debug_bounds("permissions-scope-0")
        .expect("the first scope chip rendered");
    assert!(chip.size.width > px(0.), "{chip:?}");
    // Every family has a four-state control; segment 0 is `Inherit`.
    let segment = cx
        .debug_bounds("permissions-family-exec-0-segment")
        .expect("the Commands control rendered");
    assert!(segment.size.width > px(0.), "{segment:?}");
    // The stored `rm *` child renders its own three-state control.
    let child = cx
        .debug_bounds("permissions-pattern-rm *-0-segment")
        .expect("the pattern rule's control rendered");
    assert!(child.size.width > px(0.), "{child:?}");
}

#[gpui::test]
fn the_scope_chips_switch_which_scope_the_page_edits(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = open_page(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            assert_eq!(
                app.scope_choices(),
                vec![
                    Scope::Any,
                    Scope::Session,
                    Scope::Agent("scout".into()),
                    Scope::Agent("worker".into()),
                ]
            );
            // The worker's own rule makes Commands Allow there; a main session
            // inherits the mode (Ask). That is what the row's `→ …` shows.
            app.set_permission_scope(Scope::Agent("worker".into()), cx);
            assert_eq!(
                app.permission_rules.effective_action(
                    &app.permission_scope,
                    Target::Exec,
                    app.access_mode
                ),
                Action::Allow
            );
            app.set_permission_scope(Scope::Session, cx);
            assert_eq!(
                app.permission_rules.effective_action(
                    &app.permission_scope,
                    Target::Exec,
                    app.access_mode
                ),
                Action::Ask
            );
            // A pattern form belongs to the scope it was opened in.
            app.open_pattern_editor(Target::Exec, cx);
            app.set_permission_scope(Scope::Any, cx);
            assert!(app.pattern_editor.is_none());
        });
    });
}

#[gpui::test]
fn an_empty_pattern_is_rejected_and_the_form_stays_open(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = open_page(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.set_permission_scope(Scope::Agent("scout".into()), cx);
            app.open_pattern_editor(Target::Exec, cx);
            assert_eq!(
                app.pattern_editor.as_ref().map(|editor| editor.target),
                Some(Target::Exec)
            );
            // "Every call" is the family row above this form, not a pattern, so
            // submitting nothing stores nothing and keeps the form open.
            app.add_pattern(cx);
            assert_eq!(
                app.permission_rules
                    .pattern_rules(&Scope::Agent("scout".into()), Target::Exec)
                    .len(),
                0
            );
            assert!(app.pattern_editor.is_some());

            // The form's action is editable before anything is added.
            app.set_pattern_action(Action::Allow, cx);
            assert_eq!(
                app.pattern_editor.as_ref().map(|editor| editor.action),
                Some(Action::Allow)
            );
            app.cancel_pattern_editor(cx);
            assert!(app.pattern_editor.is_none());
        });
    });
}

/// Geometry the paint test cannot see: a row is two lines tall, and the
/// four-state control sits inside the row at both the minimum window and a
/// roomy one. This is the class of bug a screenshot would catch (a collapsed
/// row, a control clipped past the board's edge), so it is asserted directly.
#[gpui::test]
fn every_row_is_two_lines_and_its_control_stays_inside(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = open_page(cx);
    for viewport in [
        gpui::size(px(960.), px(640.)),
        gpui::size(px(1440.), px(900.)),
    ] {
        cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
        let row = cx
            .debug_bounds("permissions-family-exec")
            .expect("the Commands row rendered");
        assert!(
            row.size.height > px(40.),
            "a family row is label + control: {row:?}"
        );
        let right = row.origin.x + row.size.width + px(1.);
        for index in 0..4 {
            let segment = cx
                .debug_bounds(name(format!("permissions-family-exec-{index}-segment")))
                .expect("every state has a segment");
            assert!(segment.size.width > px(0.), "{segment:?}");
            assert!(segment.origin.x >= row.origin.x - px(1.), "{segment:?}");
            assert!(
                segment.origin.x + segment.size.width <= right,
                "segment {index} escapes its row at {viewport:?}: {segment:?} vs {row:?}"
            );
        }
    }
}

/// Many agents must wrap the chips, not widen the page.
#[gpui::test]
fn the_scope_chips_wrap_inside_the_page(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = open_page(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            app.permission_agents = (0..8)
                .map(|index| format!("agent-{index}"))
                .collect::<Vec<_>>();
        });
    });
    let viewport = gpui::size(px(960.), px(640.));
    cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    let first = cx
        .debug_bounds("permissions-scope-0")
        .expect("the scope row rendered");
    let last = cx
        .debug_bounds(name("permissions-scope-9".to_string()))
        .expect("the last agent chip rendered");
    assert!(
        last.origin.y > first.origin.y,
        "ten chips cannot fit one line at 960px: {first:?} {last:?}"
    );
    for index in 0..10 {
        let chip = cx
            .debug_bounds(name(format!("permissions-scope-{index}")))
            .expect("chip");
        assert!(chip.origin.x >= px(0.), "{chip:?}");
        assert!(
            chip.origin.x + chip.size.width <= viewport.width,
            "chip {index} is clipped: {chip:?}"
        );
    }
}

/// Full access is answered before the rule table, so the page states what the
/// mode does instead of rendering rows whose settings cannot take effect.
#[gpui::test]
fn full_access_replaces_the_rule_board_with_a_statement(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = open_page(cx);
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.access_mode = AccessMode::FullAccess;
            cx.notify();
        });
    });
    let viewport = gpui::size(px(960.), px(640.));
    cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());
    assert!(
        cx.debug_bounds("permissions-family-exec").is_none(),
        "no family rows under Full access"
    );
    assert!(
        cx.debug_bounds("permissions-scope-0").is_none(),
        "no scope chips under Full access"
    );
    // The baseline row is still there, so the mode can be changed back.
    assert!(cx.debug_bounds("settings-page-title").is_some());
}
