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

/// Settings → Providers & models (#131): a provider with models in the
/// catalog is connected, and its models sit under its card on the same page.
/// A search for one model narrows the page to that model's provider.
#[gpui::test]
fn connected_providers_list_their_models_on_the_providers_page(cx: &mut gpui::TestAppContext) {
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
            })
        });
        draw(cx, &app);
        app
    };

    let all = cx.add_empty_window();
    open(all, "");
    assert!(all.debug_bounds("provider-models-anthropic").is_some());
    assert!(all.debug_bounds("provider-models-opencode-go").is_some());

    // A fresh window: `debug_bounds` remembers anything a window ever painted.
    let searched = cx.add_empty_window();
    open(searched, "anthropic-model");
    assert!(searched.debug_bounds("provider-models-anthropic").is_some());
    assert!(
        searched
            .debug_bounds("provider-models-opencode-go")
            .is_none(),
        "a model search keeps only the providers that offer a match"
    );
}
