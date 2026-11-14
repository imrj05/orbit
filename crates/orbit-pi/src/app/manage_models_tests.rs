//! The model picker's **Manage models…** (#131): it lands on Settings →
//! Models, filtered to the provider the picker was showing.

use super::*;
use crate::theme::{Theme, ThemeId};

fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
    cx.update(|cx| {
        cx.set_global(Theme::for_id(ThemeId::Orbit));
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
    })
}

#[gpui::test]
fn manage_models_opens_the_models_page_filtered_to_the_provider(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.open_models_settings(Some("anthropic".into()), cx);
            assert!(app.settings_open, "settings opened");
            assert_eq!(app.settings_section, SettingsSection::Models);
            assert_eq!(app.models_provider_filter.as_deref(), Some("anthropic"));
        });
    });
}

/// A provider the catalog does not list would filter the page to nothing;
/// the page shows every provider instead.
#[gpui::test]
fn an_unknown_or_missing_provider_shows_every_provider(cx: &mut gpui::TestAppContext) {
    let app = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |app, cx| {
            app.models_provider_filter = Some("anthropic".into());
            app.open_models_settings(Some("gone".into()), cx);
            assert_eq!(app.settings_section, SettingsSection::Models);
            assert_eq!(app.models_provider_filter, None);

            app.models_provider_filter = Some("anthropic".into());
            app.open_models_settings(None, cx);
            assert_eq!(app.models_provider_filter, None);
        });
    });
}

/// A model turned off in Settings → Models leaves the picker's catalog, but
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
