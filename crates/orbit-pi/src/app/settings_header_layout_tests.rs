use super::*;
use gpui::VisualTestContext;

/// A settings page in a 1470×922 window. One configured remote host puts the
/// Machine selector above the toolbar — the tallest header any section has.
fn open_settings(cx: &mut VisualTestContext, section: SettingsSection) -> Entity<OrbitApp> {
    cx.update(|_, cx: &mut gpui::App| {
        cx.new(|cx| {
            let mut app = OrbitApp::new(cx);
            app.setup_open = false;
            app.layout_restored = true;
            app.settings_open = true;
            app.settings_section = section;
            app.remote_hosts = vec![crate::remote_hosts::RemoteHost {
                name: "vps".into(),
                ssh: "example.com".into(),
                user: None,
                port: None,
                dir: "/srv/app".into(),
                enabled: true,
                local_dir: None,
            }];
            app
        })
    })
}

fn draw(cx: &mut VisualTestContext, app: &Entity<OrbitApp>) {
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1470.), px(922.)),
        |_, _| app.clone(),
    );
}

fn wheel(cx: &mut VisualTestContext, dy: f32) {
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: point(px(700.), px(700.)),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(dy))),
        ..Default::default()
    });
}

/// The pinned header (title, Machine selector, toolbar) ends above the
/// scrolling body, so the toolbar never paints over the first card.
#[gpui::test]
fn the_providers_toolbar_ends_above_the_first_card(cx: &mut gpui::TestAppContext) {
    let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
    theme.ui.motion = crate::theme::MotionPreference::Reduced;
    cx.update(|cx| cx.set_global(theme));

    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(1470.), px(922.)));
    let app = open_settings(cx, SettingsSection::Providers);
    draw(cx, &app);
    let toolbar = cx.debug_bounds("providers-toolbar").expect("toolbar");
    let content = cx.debug_bounds("settings-content").expect("content");
    let card = cx.debug_bounds("default-model-card").expect("default card");
    assert!(
        content.top() >= toolbar.bottom(),
        "the scrolling body starts under the pinned toolbar: toolbar {toolbar:?}, body {content:?}"
    );
    assert!(
        card.top() >= toolbar.bottom(),
        "the toolbar overlaps the first card: toolbar {toolbar:?}, card {card:?}"
    );
}

/// A section is a new page, so it opens at the top. The content element keeps
/// one stable id across sections, so without an explicit reset the previous
/// page's offset carried over: the new page opened scrolled down and its
/// first card was clipped under the pinned toolbar.
#[gpui::test]
fn a_new_section_starts_at_the_top_of_its_content(cx: &mut gpui::TestAppContext) {
    let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
    theme.ui.motion = crate::theme::MotionPreference::Reduced;
    cx.update(|cx| cx.set_global(theme));

    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(1470.), px(922.)));
    let app = open_settings(cx, SettingsSection::Providers);
    draw(cx, &app);

    wheel(cx, -240.);
    draw(cx, &app);
    let scrolled = app.read_with(cx, |app, _| app.settings_content_scroll.offset().y);
    assert!(
        scrolled < px(0.),
        "the page should scroll for this test to mean anything, got {scrolled:?}"
    );
    // The symptom: scrolled down, the first card sits above the viewport's
    // top edge and its header is cut off under the pinned toolbar.
    let content = cx.debug_bounds("settings-content").expect("content");
    let card = cx.debug_bounds("default-model-card").expect("default card");
    assert!(
        card.top() < content.top(),
        "a scrolled body should carry the first card past the top edge: card {card:?}, content {content:?}"
    );

    app.update(cx, |app, cx| {
        app.set_settings_section(SettingsSection::General, cx)
    });
    draw(cx, &app);
    assert_eq!(
        app.read_with(cx, |app, _| app.settings_content_scroll.offset().y),
        px(0.),
        "a new section must open at its top"
    );
    let card = cx.debug_bounds("default-model-card").expect("default card");
    let content = cx.debug_bounds("settings-content").expect("content");
    assert!(
        card.top() >= content.top(),
        "the new page's first card must start inside the viewport: card {card:?}, content {content:?}"
    );
}

/// A narrow window still fits the content column: the page's block is capped
/// at `CONTENT_MAX_W`, which is wider than the space a short window leaves
/// after the nav column.
#[gpui::test]
fn the_settings_content_fits_a_narrow_window(cx: &mut gpui::TestAppContext) {
    let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
    theme.ui.motion = crate::theme::MotionPreference::Reduced;
    cx.update(|cx| cx.set_global(theme));

    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(1178.), px(922.)));
    let app = open_settings(cx, SettingsSection::Providers);
    let _ = cx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(1178.), px(922.)),
        |_, _| app.clone(),
    );

    let content = cx.debug_bounds("settings-content").expect("content");
    assert!(
        content.right() <= px(1178.),
        "the content column spills out of a 1178 px window: {content:?}"
    );
    assert!(
        content.left() >= px(256.),
        "the content column must clear the nav column: {content:?}"
    );
}

/// A disabled package keeps its card, says so twice — a badge on the row and
/// one line explaining what "disabled" means — and sorts below the active ones
/// (the ordering itself is `plugins::sort_for_page`).
#[gpui::test]
fn a_disabled_package_is_badged_and_explained(cx: &mut gpui::TestAppContext) {
    let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
    theme.ui.motion = crate::theme::MotionPreference::Reduced;
    cx.update(|cx| cx.set_global(theme));

    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(1470.), px(922.)));
    let app = open_settings(cx, SettingsSection::Plugins);
    app.update(cx, |app, _| {
        app.plugins = vec![crate::plugins::PluginPackage {
            source: "npm:pi-better".into(),
            name: "pi-better".into(),
            scope: crate::plugins::PackageScope::User,
            kind: crate::plugins::PackageKind::Npm,
            install_path: std::env::temp_dir().join("pi-better"),
            installed: true,
            version: Some("1.0.0".into()),
            enabled: false,
        }];
    });
    draw(cx, &app);

    assert!(
        cx.debug_bounds("plugins-disabled-hint").is_some(),
        "a page with a disabled package explains what that means"
    );
    assert!(
        cx.debug_bounds("plugin-toggle-npm:pi-better").is_some(),
        "the package keeps its card and its switch"
    );
}
