use super::*;
use crate::theme::tokens::{Radius, TextSize};
use crate::theme::ThemeId;
use gpui::size;

/// The new-task backdrop must span the whole chat column below the top bar.
/// It used to live inside the empty-state slot — the `flex_1` region *above*
/// the composer column — so the backdrop stopped at the composer's top edge
/// and left the bottom of the window on a flat slab. This pins the layer to
/// the column instead, so the floating composer and the status bar below it
/// paint over the backdrop.
#[gpui::test]
fn new_task_backdrop_spans_the_composer_region(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    });
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1000.), px(700.)), |_, cx| {
        let theme = Theme::for_id(ThemeId::Orbit);
        let backdrop = app.update(cx, |app, _| app.new_task_backdrop(theme, 0., true, false));
        div()
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .child(
                div()
                    .id("test-top-bar")
                    .debug_selector(|| "test-top-bar".to_string())
                    .flex_none()
                    .h(px(super::view::TOP_BAR_H))
                    .w_full(),
            )
            .children(backdrop)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("test-composer")
                            .debug_selector(|| "test-composer".to_string())
                            .flex_none()
                            .h(px(125.))
                            .w_full(),
                    ),
            )
    });

    let backdrop = cx
        .debug_bounds("new-task-backdrop")
        .expect("backdrop layer laid out");
    let composer = cx.debug_bounds("test-composer").expect("composer");
    assert_eq!(
        backdrop.origin.y,
        px(super::view::TOP_BAR_H),
        "backdrop must start below the top bar"
    );
    assert_eq!(
        backdrop.origin.y + backdrop.size.height,
        px(700.),
        "backdrop must run to the window's bottom edge"
    );
    assert!(
        backdrop.origin.y + backdrop.size.height > composer.origin.y,
        "backdrop must extend below the composer's top edge: {backdrop:?} vs {composer:?}"
    );
}

/// The field's clock runs only while the new-task page is on screen, and never
/// under Reduce Motion. Those are the two ways an animated backdrop goes wrong:
/// repainting a page nobody is looking at, and ignoring the platform's motion
/// preference.
#[gpui::test]
fn the_backdrop_clock_runs_only_on_the_new_task_page(cx: &mut gpui::TestAppContext) {
    let app = cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        cx.new(OrbitApp::new)
    });
    // Not on screen: the heartbeat must not even start the clock.
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.backdrop_live = false;
            app.tick(cx);
            assert!(
                app.backdrop_clock.is_none(),
                "an off-screen backdrop keeps no clock"
            );
            assert_eq!(app.backdrop_phase, 0., "and no phase");
        })
    });

    // On screen: the phase follows wall-clock time.
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.backdrop_live = true;
            app.tick(cx);
            assert!(app.backdrop_clock.is_some(), "the clock starts");
        })
    });
    std::thread::sleep(std::time::Duration::from_millis(25));
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.tick(cx);
            assert!(
                app.backdrop_phase > 0.,
                "the phase advanced with time: {}",
                app.backdrop_phase
            );
        })
    });

    // Reduce Motion freezes it on the frame it was showing.
    let frozen = cx.update(|cx| app.read(cx).backdrop_phase);
    cx.update(|cx| {
        let mut theme = Theme::for_id(ThemeId::Orbit);
        theme.ui.reduce_motion = true;
        cx.set_global(theme);
    });
    std::thread::sleep(std::time::Duration::from_millis(25));
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.tick(cx);
            assert_eq!(
                app.backdrop_phase, frozen,
                "Reduce Motion holds the field still"
            );
            assert!(app.backdrop_clock.is_none(), "and drops the clock");
        })
    });
}

/// The chat page deliberately has no backdrop.
#[gpui::test]
fn new_task_backdrop_is_absent_on_a_session(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.update(|_, cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        let app = cx.new(OrbitApp::new);
        assert!(
            app.update(cx, |app, _| app.new_task_backdrop(
                Theme::for_id(ThemeId::Orbit),
                0.,
                false,
                false
            ))
            .is_none(),
            "a session page has no backdrop"
        );
    });
}

/// Regression: a provider card's name/id column must get a real width.
/// `truncate()` collapses to a bare ellipsis when the text element's width
/// resolves to 0 — which happens when `w_full()`/`min_w_0()` are set on the
/// text inside a `flex_1` column. Relying on the parent's stretch is what
/// keeps the name visible.
/// Regression: a provider card's name/id column must get a real width.
/// `truncate()` renders a bare ellipsis when its element resolves to zero
/// width, which happens when `w_full()` is used inside a flex-grow column.
/// The card relies on the parent's stretch and gives the name the full
/// width (the status pill lives on the badges row, not beside the name).
#[gpui::test]
fn provider_name_column_has_a_real_width(cx: &mut gpui::TestAppContext) {
    use gpui::{point, size};

    struct CardHeaderTestView;
    impl Render for CardHeaderTestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::for_id(ThemeId::Orbit);
            // One 3-column grid cell's worth of width.
            div()
                .w(px(320.))
                .flex()
                .flex_col()
                .p(px(14.))
                .gap_2p5()
                .child(
                    div()
                        .flex()
                        .items_start()
                        .gap_3()
                        .child(div().size(px(52.)).flex_none())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    div()
                                        .id("provider-name")
                                        .debug_selector(|| "provider-name".to_string())
                                        .text_size(TextSize::Default.px(&theme))
                                        .truncate()
                                        .child("Amazon Bedrock"),
                                )
                                .child(
                                    div()
                                        .id("provider-id")
                                        .debug_selector(|| "provider-id".to_string())
                                        .text_size(theme.code_px(10.5))
                                        .truncate()
                                        .child("amazon-bedrock"),
                                ),
                        ),
                )
                .child(
                    div()
                        .id("provider-endpoint")
                        .debug_selector(|| "provider-endpoint".to_string())
                        .text_size(theme.code_px(10.5))
                        .truncate()
                        .child("https://api.example.com/v1 · openai-completions"),
                )
        }
    }

    let cx = cx.add_empty_window();
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1000.), px(300.)), |_, cx| {
        cx.new(|_| CardHeaderTestView)
    });
    let name = cx.debug_bounds("provider-name").expect("name laid out");
    let id = cx.debug_bounds("provider-id").expect("id laid out");
    let endpoint = cx
        .debug_bounds("provider-endpoint")
        .expect("endpoint laid out");
    assert!(
        name.size.width > px(150.),
        "provider name collapsed to an ellipsis: {:?}",
        name.size
    );
    assert!(
        id.size.width > px(150.),
        "provider id collapsed: {:?}",
        id.size
    );
    assert!(
        endpoint.size.width > px(250.),
        "provider endpoint collapsed: {:?}",
        endpoint.size
    );
}

/// Regression: the Agent tab's long card descriptions blew the settings
/// content column past the window. A text element's min-content width is
/// its full unwrapped line in gpui, so a `flex_1` column with default
/// `min-width: auto` refuses to shrink and the card (and its toggle)
/// overflow off-screen. `min_w_0` on the settings root and content column
/// lets the column take the window width; the description then wraps.
#[gpui::test]
fn settings_cards_stay_inside_the_content_column(cx: &mut gpui::TestAppContext) {
    use gpui::{point, size};

    struct SettingsCardTestView;
    impl Render for SettingsCardTestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::for_id(ThemeId::Orbit);
            let card = div()
                    .id("settings-card")
                    .debug_selector(|| "settings-card".to_string())
                    .bg(theme.surface_secondary)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(Radius::XLarge.px(&theme))
                    .px(px(14.))
                    .py(px(12.))
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .text_size(TextSize::Default.px(&theme))
                                    .child("Follow-up messages"),
                            )
                            .child(
                                div()
                                    .id("settings-card-desc")
                                    .debug_selector(|| "settings-card-desc".to_string())
                                    .text_size(TextSize::Small.px(&theme))
                                    .child("Messages sent while the agent is running wait in the queue above the composer and are delivered once the current task finishes. All delivers the whole queue at once; One at a time delivers one per run."),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().h(px(28.)).px(px(10.)).child("One at a time"))
                            .child(div().h(px(28.)).px(px(10.)).child("All")),
                    );

            div()
                .size_full()
                .flex()
                .child(div().flex_none().w(px(240.)).h_full())
                .child(
                    div()
                        .id("settings-content-column")
                        .debug_selector(|| "settings-content-column".to_string())
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id("settings-scroll")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .child(
                                    div()
                                        .w_full()
                                        .px(px(24.))
                                        .flex()
                                        .flex_col()
                                        .gap_3()
                                        .child(card),
                                ),
                        ),
                )
        }
    }

    let cx = cx.add_empty_window();
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(700.)), |_, cx| {
        cx.new(|_| SettingsCardTestView)
    });
    let card = cx.debug_bounds("settings-card").expect("card");
    let desc = cx.debug_bounds("settings-card-desc").expect("description");
    let content = cx
        .debug_bounds("settings-content-column")
        .expect("content column");
    assert_eq!(
        content.size.width,
        px(1000.),
        "content column must take the space beside the 240 px nav"
    );
    assert!(
        card.origin.x + card.size.width <= px(1240.),
        "card overflows the window: {:?}",
        card
    );
    assert!(
        desc.size.height > px(30.),
        "long description must wrap to multiple lines, got {:?}",
        desc.size
    );
    assert!(
        desc.size.width < card.size.width,
        "description must be narrower than the card so the control fits: {:?}",
        desc.size
    );
}

/// Regression: the Providers page's 3-column grid blew past the content
/// column and clipped its third column off the window. `grid_cols(3)` is
/// `repeat(3, minmax(0, 1fr))`, but `1fr` resolves against max-content when
/// the grid's parent is given only `w_full().max_w()` *inside* a vertical
/// scroll container — the tracks size to the widest card instead of the
/// column. The max-width belongs on the scroll container itself, so the
/// grid is laid out against a definite width and the columns stay equal.
#[gpui::test]
fn provider_grid_columns_fit_inside_the_content_column(cx: &mut gpui::TestAppContext) {
    use gpui::{point, size};

    const NAMES: [&str; 3] = ["grid-card-0", "grid-card-1", "grid-card-2"];

    struct ProviderGridTestView;
    impl Render for ProviderGridTestView {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let theme = Theme::for_id(ThemeId::Orbit);
            let card = |i: usize| {
                let name = NAMES[i];
                div()
                    .id(ElementId::Name(name.to_string().into()))
                    .debug_selector(move || name.to_string())
                    .w_full()
                    .min_w_0()
                    .bg(theme.surface_secondary)
                    .border_1()
                    .border_color(theme.border)
                    .rounded(Radius::XLarge.px(&theme))
                    .p(px(14.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(TextSize::Default.px(&theme))
                            .child(format!("Provider {i}")),
                    )
                    .child(
                        div()
                            .font_family(theme::code_font_family())
                            .text_size(theme.code_px(10.5))
                            .truncate()
                            .child("https://api.example.com/v1 · openai-completions plus a long trailing note"),
                    )
            };

            div()
                .size_full()
                .flex()
                .child(div().flex_none().w(px(240.)).h_full())
                .child(
                    div().flex_1().min_w_0().min_h_0().flex().flex_col().child(
                        div()
                            .id("grid-scroll")
                            .debug_selector(|| "grid-scroll".to_string())
                            .flex_1()
                            .min_h_0()
                            .w_full()
                            .max_w(px(CONTENT_MAX_W))
                            .mx_auto()
                            .overflow_y_scroll()
                            .child(
                                div().w_full().px(px(24.)).flex().flex_col().gap_3().child(
                                    div()
                                        .w_full()
                                        .grid()
                                        .grid_cols(3)
                                        .gap_3()
                                        .children((0..3).map(card)),
                                ),
                            ),
                    ),
                )
        }
    }

    let cx = cx.add_empty_window();
    let _ = cx.draw(point(px(0.), px(0.)), size(px(1240.), px(700.)), |_, cx| {
        cx.new(|_| ProviderGridTestView)
    });
    let scroll = cx.debug_bounds("grid-scroll").expect("scroll container");
    assert_eq!(scroll.size.width, px(CONTENT_MAX_W));
    for (i, name) in NAMES.iter().enumerate() {
        let card = cx
            .debug_bounds(name)
            .unwrap_or_else(|| panic!("card {i} laid out"));
        assert!(
            card.size.width > px(250.),
            "grid column {i} collapsed: {:?}",
            card.size
        );
        assert!(
            card.size.width < px(400.),
            "grid column {i} blew past its share — `fr` resolved to max-content: {:?}",
            card.size
        );
        assert!(
            card.origin.x + card.size.width <= scroll.origin.x + scroll.size.width,
            "card {i} overflows the content column: {:?}",
            card
        );
    }
}

/// Regression: an SVG defaults to `flex-shrink: 1`, so beside wide text
/// (e.g. the transcript's activity label + copy button) it collapses to
/// zero width — the "icons render tiny" bug. `icon`/`icon_dyn`/`glyph`
/// set `flex_none`; this pins both halves of that behavior.
#[gpui::test]
fn flex_none_keeps_icons_from_collapsing(cx: &mut gpui::TestAppContext) {
    use gpui::{point, size};

    struct Probe;
    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(120.))
                .h(px(40.))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .whitespace_nowrap()
                        .child("a very long unwrapping label that will not shrink at all"),
                )
                .child(
                    gpui::svg()
                        .path("icons/copy.svg")
                        .id("probe-icon-fixed")
                        .debug_selector(|| "probe-icon-fixed".to_string())
                        .flex_none()
                        .size(px(16.)),
                )
                .child(
                    gpui::svg()
                        .path("icons/copy.svg")
                        .id("probe-icon-shrunk")
                        .debug_selector(|| "probe-icon-shrunk".to_string())
                        .size(px(16.)),
                )
        }
    }

    let cx = cx.add_empty_window();
    let _ = cx.draw(point(px(0.), px(0.)), size(px(400.), px(200.)), |_, cx| {
        cx.new(|_| Probe)
    });
    let fixed = cx.debug_bounds("probe-icon-fixed").expect("fixed icon");
    let shrunk = cx.debug_bounds("probe-icon-shrunk").expect("shrunk icon");
    assert_eq!(
        fixed.size.width,
        px(16.),
        "flex_none icon must keep its size: {:?}",
        fixed
    );
    assert!(
        shrunk.size.width < px(16.),
        "without flex_none the icon collapses: {:?}",
        shrunk
    );
}
