use super::*;

struct ComposerStatusProbe {
    app: Entity<OrbitApp>,
    width: Pixels,
}

impl Render for ComposerStatusProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div().w(self.width).child(app.status_bar("orbit", cx))
        })
    }
}

struct QueueBarProbe {
    app: Entity<OrbitApp>,
    width: Pixels,
}

impl Render for QueueBarProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div().w(self.width).children(app.queue_bar(cx))
        })
    }
}

/// The queue strip above the composer is one grouped surface: a 30px label
/// header plus one ~30px row per pending message. Pin that shape so the rows
/// stay flat (no card nested inside the card) and the strip stays compact as
/// messages accumulate.
#[gpui::test]
fn queue_bar_is_one_compact_grouped_surface(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};
    use orbit_rpc::PendingQueue;

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut app = OrbitApp::new(cx);
            app.branch = None;
            app.status_at = None;
            app
        })
    });

    let width = px(600.);
    let draw = |queue: PendingQueue, cx: &mut gpui::VisualTestContext| {
        app.update(cx, |app, _| app.queue = queue);
        let _ = cx.draw(
            point(px(0.), px(0.)),
            gpui::size(width, px(400.)),
            |_, cx| {
                cx.new(|_| QueueBarProbe {
                    app: app.clone(),
                    width,
                })
            },
        );
        cx.debug_bounds("queue-bar")
    };

    // Empty: no strip at all.
    assert!(draw(PendingQueue::default(), cx).is_none());

    // One follow-up: header + one row, compact — not two stacked cards.
    let one = draw(
        PendingQueue {
            steering: vec![],
            follow_up: vec!["i need the simple and minimal website design".into()],
        },
        cx,
    )
    .expect("queue strip drawn");
    assert!((one.size.width - width).abs() <= px(1.));
    assert!(
        one.size.height >= px(58.) && one.size.height <= px(64.),
        "one row should be ~60px, was {}",
        one.size.height
    );

    // The clear control is the header's one action, inset from the right.
    let clear = cx.debug_bounds("clear-queue").expect("clear button drawn");
    assert!(clear.center().y < one.top() + px(30.));
    assert!((one.right() - px(12.) - clear.right()).abs() <= px(1.));

    // A second message adds one hairline-separated row, not a second card.
    let two = draw(
        PendingQueue {
            steering: vec!["tighten the hero".into()],
            follow_up: vec!["i need the simple and minimal website design".into()],
        },
        cx,
    )
    .expect("queue strip drawn");
    let row = two.size.height - one.size.height;
    assert!(
        row >= px(27.) && row <= px(33.),
        "one more row should add ~30px, added {row}"
    );
}

#[gpui::test]
fn sending_hints_match_footer_metrics_and_share_the_context_row(cx: &mut gpui::TestAppContext) {
    use crate::theme::{
        tokens::{button, ButtonSize},
        Theme, ThemeId,
    };

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut app = OrbitApp::new(cx);
            app.branch = None;
            app.status_at = None;
            app.context = Some(orbit_rpc::ContextUsage {
                tokens: Some(64_000),
                context_window: 128_000,
                percent: Some(50.0),
            });
            app
        })
    });

    for mode in [SendMode::FollowUp, SendMode::Steer] {
        for running in [false, true] {
            cx.update(|_, cx| {
                cx.global_mut::<Theme>().ui.composer_send_mode = mode;
                app.update(cx, |app, _| app.busy = running);
            });
            for (width, font_size) in [(320., 14.), (600., 14.), (960., 14.), (600., 18.)] {
                let theme = cx.update(|_, cx| {
                    let theme = cx.global_mut::<Theme>();
                    theme.ui.ui_font_size = font_size;
                    *theme
                });
                let width = px(width);
                let _ = cx.draw(
                    point(px(0.), px(0.)),
                    gpui::size(width, px(80.)),
                    |_, cx| {
                        cx.new(|_| ComposerStatusProbe {
                            app: app.clone(),
                            width,
                        })
                    },
                );
                let hints = cx
                    .debug_bounds("composer-send-hints")
                    .expect("sending hints laid out in the status row");
                let indicator = cx
                    .debug_bounds("composer-context-indicator")
                    .expect("context indicator laid out");

                let label = cx
                    .debug_bounds("composer-hint-label")
                    .expect("hint label laid out");
                let keys = cx
                    .debug_bounds("composer-hint-keys")
                    .expect("hint shortcut laid out");
                let label_size = button::label_size(ButtonSize::Default).px(&theme);
                let key_size = if cfg!(target_os = "macos") {
                    ButtonSize::Default.icon_size().px(&theme)
                } else {
                    label_size
                };

                // GPUI rounds layout to whole pixels, including scaled UI sizes.
                assert!((label.size.height - label_size).abs() <= px(1.));
                assert!((keys.size.height - key_size).abs() <= px(1.));
                assert!((hints.size.height - ButtonSize::Default.height(&theme)).abs() <= px(1.));
                assert!(hints.size.width > px(0.));
                assert!(hints.right() < indicator.left());
                assert!((hints.center().y - indicator.center().y).abs() <= px(1.));
                assert_eq!(indicator.right(), width);
            }
        }
    }
}
