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

/// The composer's own row, so the chips' presence rules can be read off the
/// laid-out tree rather than inferred from the state they are built from.
struct ComposerRowProbe {
    app: Entity<OrbitApp>,
}

impl Render for ComposerRowProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div().w(px(900.)).child(app.composer_row(false, cx))
        })
    }
}

impl Render for QueueBarProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div().w(self.width).children(app.queue_bar(cx))
        })
    }
}

/// The queue strip above the composer is one grouped surface: a 30px label
/// header plus one compact row per pending message. Pin that shape so the rows
/// stay flat (no card nested inside the card) and the strip stays compact as
/// messages accumulate.
///
/// A row carries its own edit / send-now controls (issue #79), so it stands
/// 33px tall — the 18px icon buttons plus the row's 14px padding and hairline —
/// instead of the bare message's 30px. Still one flat row, never a second card.
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
        one.size.height >= px(58.) && one.size.height <= px(66.),
        "one row should be ~63px, was {}",
        one.size.height
    );

    // The clear control is the header's one action, inset from the right.
    let clear = cx.debug_bounds("clear-queue").expect("clear button drawn");
    assert!(clear.center().y < one.top() + px(30.));
    assert!((one.right() - px(12.) - clear.right()).abs() <= px(1.));

    // The row's own actions sit on the row, at its right edge.
    let edit = cx
        .debug_bounds("queue-row-0-edit")
        .expect("edit control drawn");
    assert!(edit.center().y > one.top() + px(30.), "below the header");
    assert!((one.right() - px(12.) - edit.right()).abs() <= px(30.));
    // A follow-up can always jump the queue, whatever its position.
    assert!(
        cx.debug_bounds("queue-row-0-send-now").is_some(),
        "a follow-up can always jump the queue"
    );

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
        row >= px(29.) && row <= px(35.),
        "one more row should add ~33px, added {row}"
    );
    // Every row is editable…
    assert!(cx.debug_bounds("queue-row-0-edit").is_some());
    assert!(cx.debug_bounds("queue-row-1-edit").is_some());
    // …but the leading steer is already pi's next delivery, so it offers no
    // "send now" while the queued follow-up behind it does.
    assert!(
        cx.debug_bounds("queue-row-0-send-now").is_none(),
        "the message pi delivers next has nowhere to be promoted to"
    );
    assert!(cx.debug_bounds("queue-row-1-send-now").is_some());
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
            // Only the running half of the rule is measured here — the idle
            // half has its own test, because an absence can only be observed
            // in a window that has never painted the element.
            app.busy = true;
            app
        })
    });

    for mode in [SendMode::FollowUp, SendMode::Steer] {
        cx.update(|_, cx| {
            cx.global_mut::<Theme>().ui.composer_send_mode = mode;
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

/// The hints teach the steering chords, and those exist only while a run is in
/// flight. Idle, they restate the send button a few pixels above, so they stay
/// out of the chrome and the context indicator carries the right edge alone.
///
/// Its own test because `debug_bounds` reads a registry that survives a redraw:
/// a window that has once painted the hints keeps reporting them, so an absence
/// has to be observed in a window that never has.
#[gpui::test]
fn an_idle_status_row_keeps_the_send_hints_out_of_the_chrome(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

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

    for mode in [SendMode::FollowUp, SendMode::Steer] {
        for width in [320., 600., 960.] {
            cx.update(|_, cx| {
                cx.global_mut::<Theme>().ui.composer_send_mode = mode;
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
            assert!(
                !app.update(cx, |app, _| app.is_running()),
                "the fixture is idle, so the hints have nothing to teach"
            );
            assert!(
                cx.debug_bounds("composer-send-hints").is_none(),
                "an idle status row keeps the send hints out of the chrome"
            );
            let indicator = cx
                .debug_bounds("composer-context-indicator")
                .expect("context indicator laid out");
            assert_eq!(indicator.right(), width);
        }
    }
}

/// `["off"]` is what pi reports for a model without reasoning and an empty list
/// is what it reports before the levels arrive, so neither offers a level to
/// pick. The chip, the ⌘T shortcut, and the palette entry all read the same
/// predicate, so they cannot disagree about whether the picker has options.
///
/// Each case gets its own window: `debug_bounds` keeps reporting an element once
/// the window has painted it, so presence and absence cannot be measured against
/// each other in one frame registry.
#[gpui::test]
fn the_thinking_chip_appears_only_when_the_model_has_a_ladder(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let app = cx.update(|cx| cx.new(OrbitApp::new));

    for (levels, expected) in [
        (vec!["off".to_string()], false),
        (Vec::new(), false),
        (vec!["off".into(), "low".into(), "high".into()], true),
        // A provider that drops `off` still leaves more than one choice.
        (vec!["low".into(), "high".into(), "max".into()], true),
    ] {
        let window = cx.add_empty_window();
        let can_choose = window.update(|_, cx| {
            app.update(cx, |app, _| {
                app.available_thinking_levels = levels.clone();
                app.can_choose_thinking()
            })
        });
        let _ = window.draw(
            point(px(0.), px(0.)),
            gpui::size(px(900.), px(60.)),
            |_, cx| cx.new(|_| ComposerRowProbe { app: app.clone() }),
        );
        assert_eq!(
            can_choose,
            expected,
            "levels {levels:?} should {} a thinking choice",
            if expected { "offer" } else { "not offer" }
        );
        assert_eq!(
            window.debug_bounds("thinking-chip").is_some(),
            expected,
            "levels {levels:?} should {} the thinking chip",
            if expected { "show" } else { "hide" }
        );
    }
}

/// Full access is the default, so its name is not news and the chip keeps the
/// glyph alone; a tightened mode is a departure from the default and keeps the
/// label that says so. The chip itself never leaves either way — hiding it
/// outright would strand the mode on a control that no longer exists.
#[gpui::test]
fn the_access_chip_names_the_mode_only_when_it_is_not_the_default(cx: &mut gpui::TestAppContext) {
    use crate::access::AccessMode;
    use crate::theme::{Theme, ThemeId};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let app = cx.update(|cx| cx.new(OrbitApp::new));

    for (mode, labelled) in [
        (AccessMode::Supervised, false),
        (AccessMode::AutoAcceptEdits, true),
        (AccessMode::FullAccess, true),
    ] {
        let window = cx.add_empty_window();
        window.update(|_, cx| {
            app.update(cx, |app, _| app.access_mode = mode);
        });
        let _ = window.draw(
            point(px(0.), px(0.)),
            gpui::size(px(900.), px(60.)),
            |_, cx| cx.new(|_| ComposerRowProbe { app: app.clone() }),
        );
        assert!(
            window.debug_bounds("access-chip").is_some(),
            "{mode:?} keeps the access chip on the row"
        );
        assert_eq!(
            window.debug_bounds("access-chip-label").is_some(),
            labelled,
            "{mode:?} should {} the mode name",
            if labelled { "show" } else { "drop" }
        );

        // The chip must not resize when the picker opens: a control that grows
        // a label on click shoves the workflow chip sideways under the pointer.
        let closed = window
            .debug_bounds("access-chip")
            .expect("access chip laid out");
        window.update(|window, cx| {
            app.update(cx, |app, cx| app.toggle_access_menu(window, cx));
        });
        let _ = window.draw(
            point(px(0.), px(0.)),
            gpui::size(px(900.), px(60.)),
            |_, cx| cx.new(|_| ComposerRowProbe { app: app.clone() }),
        );
        let open = window
            .debug_bounds("access-chip")
            .expect("access chip still laid out");
        assert_eq!(
            open.size.width, closed.size.width,
            "{mode:?} keeps the chip's width while its picker is open"
        );
        window.update(|window, cx| {
            app.update(cx, |app, cx| app.close_access_menu(window, cx));
        });
    }
}

/// Editing a queued message turns that one row into a real field: the input
/// sits inside the row (never over the bar's edge), Save and Cancel follow it,
/// and the strip keeps its width. Issue #79's inline editor has no modal, so
/// this is the whole affordance.
#[gpui::test]
fn an_edited_row_holds_its_inline_field(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};
    use orbit_rpc::PendingQueue;

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut app = OrbitApp::new(cx);
            app.branch = None;
            app.status_at = None;
            app.queue = PendingQueue {
                steering: vec![],
                follow_up: vec!["tighten the hero copy".into()],
            };
            app
        })
    });

    let width = px(600.);
    let draw = |cx: &mut gpui::VisualTestContext| {
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
    };

    draw(cx);
    let row = cx
        .debug_bounds("queue-row-follow-up-0")
        .expect("row laid out");

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.begin_queue_edit(
                QueuedMessage {
                    lane: QueueLane::Steer,
                    index: 0,
                },
                window,
                cx,
            );
        });
    });
    // The target above is a follow-up, not a steer: nothing opens.
    assert!(app.read_with(cx, |app, _| app.queue_edit.is_none()));

    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.begin_queue_edit(
                QueuedMessage {
                    lane: QueueLane::FollowUp,
                    index: 0,
                },
                window,
                cx,
            );
        });
    });
    draw(cx);
    let field = cx
        .debug_bounds("queue-edit-input")
        .expect("the inline field is laid out");
    let edited = cx
        .debug_bounds("queue-row-follow-up-0")
        .expect("the edited row is still laid out");
    assert!(
        field.left() >= edited.left() && field.right() <= edited.right(),
        "the field sits inside its row"
    );
    assert!(
        edited.size.height > row.size.height,
        "the row grows to hold a real field ({:?} → {:?})",
        row.size.height,
        edited.size.height
    );
    // Save and Cancel ride the same row, at its right edge.
    let save = cx
        .debug_bounds("queue-row-0-edit-save")
        .expect("save drawn");
    let cancel = cx
        .debug_bounds("queue-row-0-edit-cancel")
        .expect("cancel drawn");
    assert!(cancel.left() > save.left(), "cancel follows save");
    assert!((edited.right() - px(12.) - cancel.right()).abs() <= px(1.));
}

struct SuggestionsProbe {
    app: Entity<OrbitApp>,
    width: Pixels,
}

impl Render for SuggestionsProbe {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.app.update(cx, |app, cx| {
            div()
                .w(self.width)
                .children(app.extension_widgets_above(cx))
        })
    }
}

/// Long suggestions stay compact and inside the composer column, and the
/// dismiss button removes only the suggestions without changing the input.
#[gpui::test]
fn follow_up_suggestions_stay_inside_the_composer_column(cx: &mut gpui::TestAppContext) {
    use crate::theme::{Theme, ThemeId};
    use crate::widgets::{ExtensionWidget, WidgetPlacement};

    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let cx = cx.add_empty_window();
    let app = cx.update(|_, cx| {
        cx.new(|cx| {
            let mut app = OrbitApp::new(cx);
            app.branch = None;
            app.status_at = None;
            app.suggestions.enabled = true;
            app.access_mode = crate::access::AccessMode::FullAccess;
            app.workflow_mode = crate::workflow::WorkflowMode::Build;
            app.busy = false;
            app.input.update(cx, |input, cx| input.set_text("", cx));
            let payload = serde_json::json!({
                "mode": "full-access",
                "workflow": "build",
                "suggestions": [
                    "Refactor the session restore path so a parked process cannot be resumed after its worktree has been removed, then add a regression test for the missing-directory case.",
                    "Show me every call site that writes to the settings file and group them by whether they can run concurrently with the app heartbeat.",
                    "Tighten the composer layout so long follow-up suggestion text wraps inside the column instead of clipping at the window edge."
                ]
            })
            .to_string();
            app.extension_widgets = vec![ExtensionWidget {
                key: "orbit-follow-up-suggestions".into(),
                placement: WidgetPlacement::AboveEditor,
                lines: vec![payload],
            }];
            app
        })
    });

    let width = px(360.);
    let probe = cx.update(|_, cx| {
        cx.new(|_| SuggestionsProbe {
            app: app.clone(),
            width,
        })
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.draw(
            point(px(0.), px(0.)),
            gpui::size(width, px(500.)),
            |_, _| probe.clone(),
        );
    };
    draw(cx);

    let mut previous = None;
    for (index, selector) in [
        "follow-up-suggestion-0",
        "follow-up-suggestion-1",
        "follow-up-suggestion-2",
    ]
    .into_iter()
    .enumerate()
    {
        let chip = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("suggestion {index} laid out"));
        assert!(
            chip.left() >= px(0.) && chip.right() <= width + px(1.),
            "suggestion {index} must stay inside the composer column: {chip:?} vs width {width:?}"
        );
        assert!(
            chip.size.height <= px(32.),
            "suggestions stay single-line: {chip:?}"
        );
        let text_selector = ["follow-up-text-0", "follow-up-text-1", "follow-up-text-2"][index];
        let text = cx
            .debug_bounds(text_selector)
            .expect("suggestion text laid out");
        assert!(
            text.size.width > px(30.),
            "suggestion text must have readable width: {text:?}"
        );
        if let Some(previous) = previous {
            let previous: gpui::Bounds<Pixels> = previous;
            assert_eq!(chip.top(), previous.top(), "suggestions share one row");
            assert!(
                chip.left() >= previous.right(),
                "suggestions do not overlap"
            );
        }
        previous = Some(chip);
    }
    assert!(cx.debug_bounds("dismiss-follow-up-suggestions").is_some());
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.dismiss_suggestions(window, cx));
    });
    cx.update(|_, cx| {
        assert!(app.read(cx).extension_widgets.is_empty());
        assert!(app.read(cx).input.read(cx).text().is_empty());
    });
}
