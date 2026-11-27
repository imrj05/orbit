//! The model picker's **Manage models…** (#131): it lands on Settings →
//! Providers & models, searched to the provider the picker was showing.

use super::*;
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
        test_app_in(cx)
    })
}

/// An app whose catalog has one model from each of two providers.
fn test_app_in(cx: &mut App) -> Entity<OrbitApp> {
    {
        let app = cx.new(OrbitApp::new);
        app.update(cx, |app, _| {
            app.available_models = ["opencode-go", "anthropic"]
                .into_iter()
                .map(|provider| ModelEntry {
                    id: format!("{provider}-model"),
                    name: format!("{provider} model"),
                    provider: provider.into(),
                    context_window: None,
                    thinking_levels: Vec::new(),
                })
                .collect();
        });
        app
    }
}

#[gpui::test]
fn manage_models_opens_the_providers_page_searched_to_the_provider(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_models_settings(Some("anthropic".into()), cx);
            assert!(app.settings_open, "settings opened");
            assert_eq!(app.settings_section, SettingsSection::Providers);
            assert_eq!(app.provider_filter.read(cx).text(), "anthropic");
        });
    });
}

/// A provider the catalog does not list would search the page down to
/// nothing; the search is cleared instead.
#[gpui::test]
fn an_unknown_or_missing_provider_clears_the_search(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_models_settings(Some("anthropic".into()), cx);
            app.open_models_settings(Some("gone".into()), cx);
            assert_eq!(app.settings_section, SettingsSection::Providers);
            assert_eq!(app.provider_filter.read(cx).text(), "", "no stale search");

            app.open_models_settings(Some("anthropic".into()), cx);
            app.open_models_settings(None, cx);
            assert_eq!(app.provider_filter.read(cx).text(), "");
        });
    });
}

#[gpui::test]
fn long_provider_catalog_is_bounded_to_its_card(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let window = cx.add_empty_window();
    let app = window.update(|_, cx| {
        let app = test_app_in(cx);
        app.update(cx, |app, cx| {
            app.settings_open = true;
            app.settings_section = SettingsSection::Providers;
            app.provider_filter
                .update(cx, |input, cx| input.set_text("anthropic", cx));
            app.provider_models_open.insert("anthropic".into(), true);
            app.available_models = (0..80)
                .map(|i| ModelEntry {
                    id: format!(
                        "very-long-anthropic-model-name-that-must-not-overflow-the-provider-card-{i}"
                    ),
                    name: format!("Model {i}"),
                    provider: "anthropic".into(),
                    context_window: Some(1_000_000),
                    thinking_levels: Vec::new(),
                })
                .collect();
        });
        app
    });
    let draw = |window: &mut gpui::VisualTestContext| {
        let _ = window.draw(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            gpui::size(gpui::px(960.), gpui::px(1200.)),
            |_, _| app.clone(),
        );
    };
    draw(window);
    let card = window.debug_bounds("provider-row-anthropic").unwrap();
    let list = window
        .debug_bounds("provider-models-scroll-anthropic")
        .unwrap();
    let chip = window
        .debug_bounds("provider-model-chip-anthropic-very-long-anthropic-model-name-that-must-not-overflow-the-provider-card-0")
        .unwrap();
    assert!(
        list.size.height <= gpui::px(240.),
        "catalog height is bounded"
    );
    assert!(
        chip.origin.x + chip.size.width <= card.origin.x + card.size.width,
        "model stays inside its row"
    );

    window.simulate_event(gpui::ScrollWheelEvent {
        position: list.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-10.))),
        ..Default::default()
    });
    draw(window);
    let card_after = window.debug_bounds("provider-row-anthropic").unwrap();
    let chip_after = window
        .debug_bounds("provider-model-chip-anthropic-very-long-anthropic-model-name-that-must-not-overflow-the-provider-card-0")
        .unwrap();
    assert_eq!(card_after.origin.y, card.origin.y, "page stays in place");
    assert!(chip_after.origin.y < chip.origin.y, "model list scrolls");

    window.simulate_event(gpui::ScrollWheelEvent {
        position: list.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-10000.))),
        ..Default::default()
    });
    draw(window);
    window.simulate_event(gpui::ScrollWheelEvent {
        position: list.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-100.))),
        ..Default::default()
    });
    draw(window);
    assert_eq!(
        window
            .debug_bounds("provider-row-anthropic")
            .unwrap()
            .origin
            .y,
        card.origin.y,
        "page stays in place at the end of the model list"
    );
}

/// A model turned off in Settings → Providers & models leaves the picker's catalog, but
/// the session's own model stays, so the picker can always show it (#131).
#[gpui::test]
fn hidden_models_leave_the_picker_except_the_active_one(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, _| {
            app.model_provider = "anthropic".into();
            app.model_id = "anthropic-model".into();
            crate::hidden_models::set("opencode-go", "opencode-go-model", true);
            crate::hidden_models::set("anthropic", "anthropic-model", true);
            let ids: Vec<String> = app.picker_models().into_iter().map(|m| m.id).collect();
            assert_eq!(ids, ["anthropic-model"]);
            crate::hidden_models::set("opencode-go", "opencode-go-model", false);
            crate::hidden_models::set("anthropic", "anthropic-model", false);
            assert_eq!(app.picker_models().len(), 2, "turned back on");
        });
    });
}

/// Settings → Providers & models: providers render as rows on grouped boards,
/// and each provider's models open **under its own row** as click-to-select
/// chips — the Models button opens the ids/endpoint editor and no longer
/// repeats the list. A search for one model still narrows the page to that
/// model's provider, and opens its list so the match is visible.
#[gpui::test]
fn provider_models_list_under_the_row_as_click_to_select_chips(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let draw = |cx: &mut gpui::VisualTestContext, app: &Entity<OrbitApp>| {
        let app = app.clone();
        let _ = cx.draw(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            gpui::size(gpui::px(1400.), gpui::px(3000.)),
            move |_, _| app,
        );
    };
    let open = |cx: &mut gpui::VisualTestContext, query: &str| {
        let app = cx.update(|_, cx| test_app_in(cx));
        let query = query.to_string();
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::Providers;
                app.provider_filter
                    .update(cx, |input, cx| input.set_text(query, cx));
                app.provider_models_open.insert("anthropic".into(), true);
            })
        });
        draw(cx, &app);
        app
    };

    let all = cx.add_empty_window();
    open(all, "");
    assert!(all.debug_bounds("provider-row-anthropic").is_some());
    assert!(all.debug_bounds("provider-row-opencode-go").is_some());
    // The list is on the card, and every model is its own chip there.
    assert!(
        all.debug_bounds("provider-models-anthropic").is_some(),
        "a provider's models are listed under its own row"
    );
    assert!(
        all.debug_bounds("provider-model-chip-anthropic-anthropic-model")
            .is_some(),
        "each model is a click-to-select chip"
    );
    // The dialog the Models button opens is still reachable for ids/endpoint.
    assert!(
        all.debug_bounds("provider-action-provider-configure-anthropic")
            .is_some(),
        "every provider row keeps its editor"
    );
    // A provider pi reports nothing for renders no list at all.
    assert!(
        all.debug_bounds("provider-models-openai").is_none(),
        "no empty grid for a provider with no models"
    );

    // A fresh window: `debug_bounds` remembers anything a window ever painted.
    let searched = cx.add_empty_window();
    open(searched, "anthropic-model");
    assert!(searched.debug_bounds("provider-row-anthropic").is_some());
    assert!(
        searched.debug_bounds("provider-row-opencode-go").is_none(),
        "a model search keeps only the providers that offer a match"
    );
}

/// A closed list stays closed until asked for, and a model search opens the
/// matching provider's list on its own so the hit is never hidden.
#[gpui::test]
fn provider_models_disclose_on_demand_and_on_a_model_search(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let open = |cx: &mut gpui::VisualTestContext, query: &str| {
        let app = cx.update(|_, cx| test_app_in(cx));
        let query = query.to_string();
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::Providers;
                app.provider_filter
                    .update(cx, |input, cx| input.set_text(query, cx));
            })
        });
        let drawn = app.clone();
        let _ = cx.draw(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            gpui::size(gpui::px(1400.), gpui::px(3000.)),
            move |_, _| drawn,
        );
        app
    };

    let closed = cx.add_empty_window();
    open(closed, "");
    assert!(
        closed
            .debug_bounds("provider-models-toggle-anthropic")
            .is_some(),
        "a provider with models offers the disclosure"
    );
    assert!(
        closed.debug_bounds("provider-models-anthropic").is_none(),
        "a provider other than the session model's starts closed"
    );

    let searched = cx.add_empty_window();
    open(searched, "anthropic-model");
    assert!(
        searched.debug_bounds("provider-models-anthropic").is_some(),
        "a model search opens the provider's list"
    );
}

#[gpui::test]
fn provider_local_model_search_filters_ids_and_names_independently(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    // Fresh windows avoid debug_bounds retaining previously painted chips.
    for (query, first, second) in [
        ("  ANTHROPIC-MODEL  ", true, false),
        ("friendly", false, true),
        ("no-such-model", false, false),
        ("", true, true),
    ] {
        let window = cx.add_empty_window();
        let app = window.update(|_, cx| {
            let app = test_app_in(cx);
            app.update(cx, |app, cx| {
                app.settings_open = true;
                app.settings_section = SettingsSection::Providers;
                app.provider_models_open.insert("anthropic".into(), true);
                app.provider_models_open.insert("opencode-go".into(), true);
                app.available_models.push(ModelEntry {
                    id: "second-model".into(),
                    name: "Friendly display name".into(),
                    provider: "anthropic".into(),
                    context_window: None,
                    thinking_levels: Vec::new(),
                });
                let _ = app.render_settings(cx);
                app.provider_model_filters["anthropic"]
                    .0
                    .update(cx, |input, cx| input.set_text(query, cx));
                app.toggle_provider_models("anthropic".into(), true, cx);
                app.toggle_provider_models("anthropic".into(), false, cx);
                assert_eq!(
                    app.provider_model_filters["anthropic"].0.read(cx).text(),
                    query
                );
                assert_eq!(app.provider_filter.read(cx).text(), "");
                assert_eq!(
                    app.provider_model_filters["opencode-go"].0.read(cx).text(),
                    ""
                );
            });
            app
        });
        let _ = window.draw(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            gpui::size(gpui::px(1400.), gpui::px(3000.)),
            move |_, _| app,
        );
        assert!(window
            .debug_bounds("provider-model-search-anthropic")
            .is_some());
        assert_eq!(
            window
                .debug_bounds("provider-model-chip-anthropic-anthropic-model")
                .is_some(),
            first,
            "{query}"
        );
        assert_eq!(
            window
                .debug_bounds("provider-model-chip-anthropic-second-model")
                .is_some(),
            second,
            "{query}"
        );
        assert!(window
            .debug_bounds("provider-model-chip-opencode-go-opencode-go-model")
            .is_some());
        assert!(
            window.debug_bounds("provider-models-anthropic").is_some(),
            "empty results keep the search section open"
        );
    }
}

/// At the smallest window the page supports, a provider row, its details line
/// and its actions stay inside the settings column instead of clipping (the
/// 3-column grid squeezed each card to ~200px and cut its actions off, and the
/// first list layout let the facts line collapse to "…").
#[gpui::test]
fn provider_rows_fit_a_narrow_settings_column(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let window = cx.add_empty_window();
    let app = window.update(|_, cx| {
        let app = test_app_in(cx);
        app.update(cx, |app, _| {
            app.settings_open = true;
            app.settings_section = SettingsSection::Providers;
            app.provider_models_open.insert("anthropic".into(), true);
        });
        app
    });
    let _ = window.draw(
        gpui::point(gpui::px(0.), gpui::px(0.)),
        gpui::size(gpui::px(960.), gpui::px(2400.)),
        |_, _| app.clone(),
    );
    let content = window.debug_bounds("settings-content").unwrap();
    let row = window.debug_bounds("provider-row-anthropic").unwrap();
    let configure = window
        .debug_bounds("provider-action-provider-configure-anthropic")
        .unwrap();
    let chip = window
        .debug_bounds("provider-model-chip-anthropic-anthropic-model")
        .unwrap();
    let right = |b: gpui::Bounds<gpui::Pixels>| b.origin.x + b.size.width;
    // The facts line under the name gets real width; with none it painted as
    // a bare ellipsis.
    let facts = window.debug_bounds("provider-facts-anthropic").unwrap();
    assert!(
        facts.size.width > gpui::px(200.),
        "facts line has room to read: {:?}",
        facts.size.width
    );
    let toggle = window
        .debug_bounds("provider-models-toggle-anthropic")
        .unwrap();
    assert!(right(toggle) <= right(row), "row head stays in the row");
    assert!(right(row) <= right(content), "row stays in the column");
    assert!(right(configure) <= right(row), "actions stay in the row");
    assert!(right(chip) <= right(row), "models stay in the row");
}

/// Every extra account is its own provider row, right after its provider, and
/// its models sit in the catalog beside the Default account's — all live at
/// once. The provider's own row lists its accounts with Rename / Remove.
#[gpui::test]
fn extra_accounts_are_their_own_rows_beside_their_provider(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let window = cx.add_empty_window();
    let app = window.update(|_, cx| {
        let app = test_app_in(cx);
        app.update(cx, |app, _| {
            app.available_models.push(ModelEntry {
                id: "anthropic-model".into(),
                name: "anthropic model".into(),
                provider: "anthropic~work".into(),
                context_window: None,
                thinking_levels: Vec::new(),
            });
            app.account_profile_map.insert(
                "anthropic".into(),
                vec![crate::provider_accounts::AccountProfile {
                    id: "account-work".into(),
                    label: "Work".into(),
                    alias: "anthropic~work".into(),
                    credential: Some(crate::provider_accounts::CredentialKind::OAuth),
                }],
            );
            app.settings_open = true;
            app.settings_section = SettingsSection::Providers;
            app.provider_models_open.insert("anthropic".into(), true);
        });
        app
    });
    let _ = window.draw(
        gpui::point(gpui::px(0.), gpui::px(0.)),
        gpui::size(gpui::px(1200.), gpui::px(3200.)),
        |_, _| app.clone(),
    );
    let base = window.debug_bounds("provider-row-anthropic").unwrap();
    let account = window
        .debug_bounds("provider-row-anthropic~work")
        .expect("the account has its own provider row");
    assert!(
        account.origin.y > base.origin.y,
        "the account follows its provider"
    );
    assert!(window.debug_bounds("provider-accounts-anthropic").is_some());
    assert!(window
        .debug_bounds("provider-account-account-work")
        .is_some());
    assert!(window
        .debug_bounds("provider-account-rename-account-work")
        .is_some());
    assert!(window
        .debug_bounds("provider-account-remove-account-work")
        .is_some());
    assert!(
        window
            .debug_bounds("provider-accounts-anthropic~work")
            .is_none(),
        "an account row has no accounts panel of its own"
    );
}

/// Providers & models edits the same new-session default as Settings → Agent:
/// the default card carries the thinking level beside the model. With no
/// default model it is the disabled `pi default` chip; with one, it offers
/// that model's levels. Render-only: picking a level writes the user's real
/// `~/.orbit-pi/session-defaults.json`, so the shared select path is covered
/// by the Agent page's controls, not exercised here.
#[gpui::test]
fn the_providers_default_card_carries_the_thinking_level(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
    let window = cx.add_empty_window();
    let app = window.update(|_, cx| {
        let app = test_app_in(cx);
        app.update(cx, |app, _| {
            app.settings_open = true;
            app.settings_section = SettingsSection::Providers;
            app.session_default = crate::session_defaults::SessionDefault::default();
        });
        app
    });
    let draw = |window: &mut gpui::VisualTestContext| {
        let _ = window.draw(
            gpui::point(gpui::px(0.), gpui::px(0.)),
            gpui::size(gpui::px(1200.), gpui::px(900.)),
            |_, _| app.clone(),
        );
    };
    let inside = |card: gpui::Bounds<gpui::Pixels>, chip: gpui::Bounds<gpui::Pixels>| {
        chip.top() >= card.top() && chip.bottom() <= card.bottom()
    };

    draw(window);
    let card = window.debug_bounds("default-model-card").expect("default card");
    let model = window.debug_bounds("default-model-select").expect("model select");
    let thinking = window
        .debug_bounds("default-thinking-select")
        .expect("thinking select, disabled until a model is picked");
    assert!(inside(card, model) && inside(card, thinking), "both rows on one card");
    assert!(thinking.top() > model.top(), "the thinking row follows the model row");

    // A default model with levels: the row now offers that model's levels.
    window.update(|_, cx| {
        app.update(cx, |app, _| {
            app.available_models[0].thinking_levels = vec!["low".into(), "high".into()];
            let model = &app.available_models[0];
            app.session_default = crate::session_defaults::SessionDefault {
                provider: Some(model.provider.clone()),
                model_id: Some(model.id.clone()),
                thinking: Some("high".into()),
            };
            let entry = app.default_model_entry().expect("the default resolves");
            assert_eq!(entry.thinking_levels, ["low", "high"]);
        });
    });
    draw(window);
    let card = window.debug_bounds("default-model-card").expect("default card");
    let thinking = window
        .debug_bounds("default-thinking-select")
        .expect("thinking select");
    assert!(inside(card, thinking));
}
