//! Appearance preferences are separate from the palette currently being painted.

use std::{io, path::Path};

use gpui::{App, Global, Window, WindowAppearance};
use serde_json::{json, Value};

use super::{get, persist_path, DesignSystem, Theme, ThemeId, ThemeMode};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceMode {
    Light,
    Dark,
    #[default]
    System,
}

impl AppearanceMode {
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::System];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AccentColor {
    #[default]
    Default,
    Blue,
    Coral,
    Orange,
    Lime,
    Shu,
    Kaki,
    Sakura,
    Matcha,
    Asagi,
    Ruri,
    Fuji,
    Kogane,
    Ume,
    Gin,
}

impl AccentColor {
    pub const ALL: [Self; 15] = [
        Self::Default,
        Self::Blue,
        Self::Coral,
        Self::Orange,
        Self::Lime,
        Self::Shu,
        Self::Kaki,
        Self::Sakura,
        Self::Matcha,
        Self::Asagi,
        Self::Ruri,
        Self::Fuji,
        Self::Kogane,
        Self::Ume,
        Self::Gin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::Blue => "Blue",
            Self::Coral => "Coral",
            Self::Orange => "Orange",
            Self::Lime => "Lime",
            Self::Shu => "Shu",
            Self::Kaki => "Kaki",
            Self::Sakura => "Sakura",
            Self::Matcha => "Matcha",
            Self::Asagi => "Asagi",
            Self::Ruri => "Ruri",
            Self::Fuji => "Fuji",
            Self::Kogane => "Kogane",
            Self::Ume => "Ume",
            Self::Gin => "Gin",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Blue => "blue",
            Self::Coral => "coral",
            Self::Orange => "orange",
            Self::Lime => "lime",
            Self::Shu => "shu",
            Self::Kaki => "kaki",
            Self::Sakura => "sakura",
            Self::Matcha => "matcha",
            Self::Asagi => "asagi",
            Self::Ruri => "ruri",
            Self::Fuji => "fuji",
            Self::Kogane => "kogane",
            Self::Ume => "ume",
            Self::Gin => "gin",
        }
    }

    pub fn apply(self, mut theme: Theme) -> Theme {
        let value = match self {
            Self::Shu => Some(0xE8603C),
            Self::Kaki => Some(0xE58A4E),
            Self::Sakura => Some(0xE89BB0),
            Self::Matcha => Some(0x9DB86B),
            Self::Asagi => Some(0x4FB3B0),
            Self::Ruri => Some(0x5B8DEF),
            Self::Fuji => Some(0x9D8CE0),
            Self::Kogane => Some(0xE3B341),
            Self::Ume => Some(0xC2577E),
            Self::Gin => Some(0xC9C5BD),
            _ => None,
        };
        if let Some(value) = value {
            return with_accent(theme, gpui::rgb(value).into());
        }
        let id = match (self, theme.mode) {
            (Self::Default, _) => return theme,
            (Self::Blue, ThemeMode::Dark) => ThemeId::OrbitAzure,
            (Self::Blue, ThemeMode::Light) => ThemeId::OrbitAzureLight,
            (Self::Coral, ThemeMode::Dark) => ThemeId::Orbit,
            (Self::Coral, ThemeMode::Light) => ThemeId::OrbitLight,
            (Self::Orange, ThemeMode::Dark) => ThemeId::Flexoki,
            (Self::Orange, ThemeMode::Light) => ThemeId::FlexokiLight,
            (Self::Lime, ThemeMode::Dark) => ThemeId::Operational,
            (Self::Lime, ThemeMode::Light) => ThemeId::OperationalLight,
            _ => unreachable!(),
        };
        let colors = Theme::for_id(id);
        theme.accent = colors.accent;
        theme.accent_hover = colors.accent_hover;
        theme.accent_pressed = colors.accent_pressed;
        theme.accent_fg = colors.accent_fg;
        theme.accent_text = colors.accent_text;
        theme.focus_ring = colors.focus_ring;
        theme.action = colors.action;
        theme.action_hover = colors.action_hover;
        theme.action_pressed = colors.action_pressed;
        theme.action_fg = colors.action_fg;
        theme.inline_code_bg = colors.inline_code_bg;
        theme
    }
}

/// Recolour a theme's interaction roles from one base hue. The fill keeps the
/// requested colour; only the text role is lightened/darkened until it clears
/// AA on the palette's canvas and card surfaces. Shared by the named accent
/// choices and the wallpaper's extracted accent.
pub(crate) fn with_accent(mut theme: Theme, base: gpui::Hsla) -> Theme {
    let mut ink = base;
    let surfaces = [
        theme.background,
        theme.surface,
        theme.surface_secondary,
        theme.surface_elevated,
        theme.surface_hover,
        theme.surface_active,
    ];
    while surfaces.iter().any(|surface| contrast(ink, *surface) < 4.5) {
        ink.l = match theme.mode {
            ThemeMode::Dark => (ink.l + 0.005).min(1.0),
            ThemeMode::Light => (ink.l - 0.005).max(0.0),
        };
        if ink.l == 0.0 || ink.l == 1.0 {
            break;
        }
    }
    let mut hover = base;
    hover.l = (hover.l + 0.05).min(1.0);
    let mut pressed = base;
    pressed.l = (pressed.l + 0.09).min(1.0);
    let foreground = gpui::rgb(0x000000).into();
    theme.accent = base;
    theme.accent_hover = hover;
    theme.accent_pressed = pressed;
    theme.accent_fg = foreground;
    theme.accent_text = ink;
    theme.focus_ring = ink;
    theme.action = base;
    theme.action_hover = hover;
    theme.action_pressed = pressed;
    theme.action_fg = foreground;
    theme.surface_selected = base.opacity(0.14);
    theme.selection = base.opacity(0.14);
    theme.inline_code_bg = base.opacity(0.14);
    theme
}

fn contrast(a: gpui::Hsla, b: gpui::Hsla) -> f32 {
    let luminance = |color: gpui::Hsla| {
        let color = gpui::Rgba::from(color);
        let linear = |v: f32| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearancePrefs {
    pub mode: AppearanceMode,
    pub accent_color: AccentColor,
    /// Which visual language is active. Existing palettes keep
    /// [`DesignSystem::Classic`], the default.
    pub design_system: DesignSystem,
    /// The remembered Classic palette for each appearance. Operational
    /// palettes are fixed by the design system and do not overwrite these, so
    /// switching back restores the user's previous Classic choice.
    light_theme: ThemeId,
    dark_theme: ThemeId,
}

impl Global for AppearancePrefs {}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            mode: AppearanceMode::System,
            accent_color: AccentColor::Default,
            design_system: DesignSystem::Classic,
            light_theme: ThemeId::OrbitAzureLight,
            dark_theme: ThemeId::OrbitAzure,
        }
    }
}

impl AppearancePrefs {
    /// Read the previous plain theme ID as an explicit appearance choice.
    /// JSON fields are validated independently so one bad value doesn't discard
    /// the user's other selections.
    fn parse(raw: &str) -> Self {
        let mut prefs = Self::default();
        if let Some(id) = ThemeId::parse(raw) {
            prefs.mode = match id.appearance() {
                ThemeMode::Light => AppearanceMode::Light,
                ThemeMode::Dark => AppearanceMode::Dark,
            };
            prefs.design_system = id.design_system();
            prefs.store_theme(id.appearance(), id);
            return prefs;
        }
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            return prefs;
        };
        prefs.mode = match value["mode"].as_str() {
            Some("light") => AppearanceMode::Light,
            Some("dark") => AppearanceMode::Dark,
            _ => AppearanceMode::System,
        };
        prefs.accent_color = AccentColor::ALL
            .into_iter()
            .find(|color| Some(color.as_str()) == value["accent_color"].as_str())
            .unwrap_or_default();
        let design_system_field = value["design_system"].as_str();
        if let Some(system) = design_system_field.and_then(DesignSystem::parse) {
            prefs.design_system = system;
        }
        for (key, mode) in [
            ("light_theme", ThemeMode::Light),
            ("dark_theme", ThemeMode::Dark),
        ] {
            if let Some(id) = value[key].as_str().and_then(ThemeId::parse) {
                // Without an explicit system field, an Operational palette in
                // the legacy theme slot means the user chose that system.
                if design_system_field.is_none() && id.design_system() != DesignSystem::Classic {
                    prefs.design_system = id.design_system();
                }
                if id.design_system() == DesignSystem::Classic {
                    prefs.store_theme(mode, id);
                }
            }
        }
        prefs
    }

    pub fn theme(self, mode: ThemeMode) -> ThemeId {
        if self.design_system == DesignSystem::Operational {
            return self.design_system.theme_id(mode);
        }
        match mode {
            ThemeMode::Light => self.light_theme,
            ThemeMode::Dark => self.dark_theme,
        }
    }

    fn store_theme(&mut self, mode: ThemeMode, id: ThemeId) {
        if id.appearance() != mode || id.design_system() != DesignSystem::Classic {
            return;
        }
        match mode {
            ThemeMode::Light => self.light_theme = id,
            ThemeMode::Dark => self.dark_theme = id,
        }
    }

    /// Select a palette. A Classic palette also selects the Classic design
    /// system; an Operational palette selects the Operational system. The
    /// design-system control is the normal way to switch systems; this keeps
    /// direct palette choices coherent too.
    pub fn set_theme(&mut self, mode: ThemeMode, id: ThemeId) {
        if id.appearance() != mode {
            return;
        }
        self.design_system = id.design_system();
        self.store_theme(mode, id);
    }

    pub fn set_design_system(&mut self, design_system: DesignSystem) {
        self.design_system = design_system;
    }

    pub fn resolve(self, system: WindowAppearance) -> ThemeId {
        let mode = match self.mode {
            AppearanceMode::Light => ThemeMode::Light,
            AppearanceMode::Dark => ThemeMode::Dark,
            AppearanceMode::System => match system {
                WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeMode::Light,
                WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
            },
        };
        self.theme(mode)
    }

    pub fn apply(self, theme: Theme) -> Theme {
        self.accent_color.apply(theme)
    }

    pub(super) fn load() -> Self {
        match std::fs::read_to_string(persist_path()) {
            Ok(raw) => Self::parse(&raw),
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    eprintln!("Failed to load appearance preferences: {error}");
                }
                Self::default()
            }
        }
    }

    fn persist_to(self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(
            path,
            json!({
                "mode": self.mode.as_str(),
                "accent_color": self.accent_color.as_str(),
                "design_system": self.design_system.as_str(),
                "light_theme": self.light_theme.as_str(),
                "dark_theme": self.dark_theme.as_str(),
            })
            .to_string(),
        )
    }
}

/// Every palette of `mode` in [`ThemeId::ALL`] order, across every design
/// system. The settings dropdown filters this by the active system.
pub fn themes_for(mode: ThemeMode) -> impl Iterator<Item = ThemeId> {
    ThemeId::ALL
        .into_iter()
        .filter(move |id| id.appearance() == mode)
}

/// Every palette of `mode` that belongs to `design_system`, in [`ThemeId::ALL`]
/// order: the set the Theme dropdown offers for that appearance.
pub fn themes_for_system(
    mode: ThemeMode,
    design_system: DesignSystem,
) -> impl Iterator<Item = ThemeId> {
    themes_for(mode).filter(move |id| id.design_system() == design_system)
}

pub fn appearance_prefs(cx: &App) -> AppearancePrefs {
    *cx.global::<AppearancePrefs>()
}

/// Only user edits are persisted; OS changes must never overwrite the preference.
pub fn set_appearance_prefs(cx: &mut App, prefs: AppearancePrefs) -> io::Result<()> {
    if appearance_prefs(cx) == prefs {
        return Ok(());
    }
    prefs.persist_to(&persist_path())?;
    cx.set_global(prefs);
    // Notify even when editing the inactive palette or choosing a mode that
    // resolves to the same palette, so the settings controls stay in sync.
    let ui = get(cx).ui;
    super::rebuild_theme_with_ui(cx, ui);
    Ok(())
}

fn apply_system_appearance(cx: &mut App, system: WindowAppearance) {
    let id = appearance_prefs(cx).resolve(system);
    if get(cx).theme_id != id {
        let ui = get(cx).ui;
        super::rebuild_theme_for(cx, ui, system);
    }
}

pub fn watch_system_appearance(window: &Window, cx: &mut App) {
    apply_system_appearance(cx, window.appearance());
    // The subscription belongs to this window and ends when the window closes.
    window
        .observe_window_appearance(|window, cx| {
            apply_system_appearance(cx, window.appearance());
        })
        .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_preferences_follow_system() {
        let prefs = AppearancePrefs::default();
        assert_eq!(prefs.mode, AppearanceMode::System);
        assert_eq!(prefs.design_system, DesignSystem::Classic);
    }

    #[test]
    fn operational_is_opt_in_and_preserves_the_classic_choice() {
        let mut prefs = AppearancePrefs {
            mode: AppearanceMode::Dark,
            dark_theme: ThemeId::Flexoki,
            ..AppearancePrefs::default()
        };
        assert_eq!(prefs.resolve(WindowAppearance::Dark), ThemeId::Flexoki);
        prefs.set_design_system(DesignSystem::Operational);
        assert_eq!(
            prefs.theme(ThemeMode::Dark),
            ThemeId::Operational,
            "Operational uses its own dark palette"
        );
        assert_eq!(
            prefs.theme(ThemeMode::Light),
            ThemeId::OperationalLight,
            "Operational uses its own light palette"
        );
        prefs.set_design_system(DesignSystem::Classic);
        assert_eq!(
            prefs.theme(ThemeMode::Dark),
            ThemeId::Flexoki,
            "switching back restores the remembered Classic palette"
        );
    }

    #[test]
    fn design_system_is_persisted_and_parsed() {
        let json = r#"{"mode":"light","design_system":"operational","light_theme":"orbit-paper","dark_theme":"orbit-cobalt"}"#;
        let prefs = AppearancePrefs::parse(json);
        assert_eq!(prefs.mode, AppearanceMode::Light);
        assert_eq!(prefs.design_system, DesignSystem::Operational);
        assert_eq!(
            prefs.resolve(WindowAppearance::Light),
            ThemeId::OperationalLight
        );
        // The remembered Classic choice is not discarded by the system change.
        let mut prefs = prefs;
        prefs.set_design_system(DesignSystem::Classic);
        assert_eq!(prefs.theme(ThemeMode::Light), ThemeId::OrbitPaper);
        assert_eq!(prefs.theme(ThemeMode::Dark), ThemeId::OrbitCobalt);
    }

    #[test]
    fn legacy_operational_theme_key_selects_the_system() {
        let prefs = AppearancePrefs::parse("operational-light");
        assert_eq!(prefs.mode, AppearanceMode::Light);
        assert_eq!(prefs.design_system, DesignSystem::Operational);
        assert_eq!(
            prefs.resolve(WindowAppearance::Light),
            ThemeId::OperationalLight
        );
    }

    #[test]
    fn legacy_dark_theme_preserves_explicit_choice() {
        assert_eq!(
            AppearancePrefs::parse("flexoki\n"),
            AppearancePrefs {
                mode: AppearanceMode::Dark,
                dark_theme: ThemeId::Flexoki,
                ..AppearancePrefs::default()
            }
        );
    }

    #[test]
    fn legacy_light_alias_preserves_explicit_choice() {
        assert_eq!(
            AppearancePrefs::parse("one-light"),
            AppearancePrefs {
                mode: AppearanceMode::Light,
                light_theme: ThemeId::OrbitLight,
                ..AppearancePrefs::default()
            }
        );
    }

    #[test]
    fn system_resolves_normal_and_vibrant_appearances() {
        let prefs = AppearancePrefs {
            dark_theme: ThemeId::Flexoki,
            ..AppearancePrefs::default()
        };
        for (system, expected) in [
            (WindowAppearance::Light, ThemeId::OrbitAzureLight),
            (WindowAppearance::VibrantLight, ThemeId::OrbitAzureLight),
            (WindowAppearance::Dark, ThemeId::Flexoki),
            (WindowAppearance::VibrantDark, ThemeId::Flexoki),
        ] {
            assert_eq!(prefs.resolve(system), expected);
        }
    }

    #[test]
    fn explicit_modes_ignore_system() {
        for (mode, expected) in [
            (AppearanceMode::Light, ThemeId::OrbitAzureLight),
            (AppearanceMode::Dark, ThemeId::Flexoki),
        ] {
            let prefs = AppearancePrefs {
                mode,
                dark_theme: ThemeId::Flexoki,
                ..AppearancePrefs::default()
            };
            for system in [WindowAppearance::Light, WindowAppearance::Dark] {
                assert_eq!(prefs.resolve(system), expected);
            }
        }
    }

    #[test]
    fn inactive_theme_edit_does_not_change_resolved_palette() {
        let mut prefs = AppearancePrefs {
            mode: AppearanceMode::Light,
            ..AppearancePrefs::default()
        };
        prefs.set_theme(ThemeMode::Dark, ThemeId::OrbitDarker);
        assert_eq!(
            prefs.resolve(WindowAppearance::Dark),
            ThemeId::OrbitAzureLight
        );
        prefs.mode = AppearanceMode::Dark;
        assert_eq!(prefs.resolve(WindowAppearance::Light), ThemeId::OrbitDarker);
    }

    #[test]
    fn invalid_fields_fall_back_independently() {
        let prefs = AppearancePrefs::parse(
            r#"{"mode":"unknown","light_theme":"nord","dark_theme":"flexoki"}"#,
        );
        assert_eq!(
            prefs,
            AppearancePrefs {
                dark_theme: ThemeId::Flexoki,
                ..AppearancePrefs::default()
            }
        );
        assert_eq!(
            AppearancePrefs::parse("broken JSON"),
            AppearancePrefs::default()
        );
        assert_eq!(
            AppearancePrefs::parse(r#"{"dark_theme":"orbit-light"}"#),
            AppearancePrefs::default()
        );
    }

    #[test]
    fn preferences_round_trip_on_disk() {
        let dir = std::env::temp_dir().join(format!("orbit-appearance-{}", std::process::id()));
        let path = dir.join("prefs/theme");
        for mode in AppearanceMode::ALL {
            let prefs = AppearancePrefs {
                mode,
                accent_color: AccentColor::Coral,
                dark_theme: ThemeId::OrbitCobalt,
                ..AppearancePrefs::default()
            };
            prefs.persist_to(&path).unwrap();
            assert_eq!(
                AppearancePrefs::parse(&std::fs::read_to_string(&path).unwrap()),
                prefs
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn accent_choices_round_trip_and_apply_in_both_modes() {
        for color in AccentColor::ALL {
            let prefs = AppearancePrefs {
                accent_color: color,
                ..AppearancePrefs::default()
            };
            let raw = json!({"accent_color": color.as_str()}).to_string();
            assert_eq!(AppearancePrefs::parse(&raw).accent_color, color);
            for id in [ThemeId::OrbitAzure, ThemeId::OrbitAzureLight] {
                let base = Theme::for_id(id);
                let applied = prefs.apply(base);
                assert_eq!(applied.theme_id, id);
                assert_eq!(applied.background, base.background);
                if color == AccentColor::Default {
                    assert_eq!(applied.accent, base.accent);
                }
            }
        }
        assert_eq!(
            AppearancePrefs::parse(r#"{"accent_color":"invalid"}"#).accent_color,
            AccentColor::Default
        );
    }

    #[test]
    fn named_accents_keep_requested_bases_and_contrast_on_all_palettes() {
        for (color, value) in [
            (AccentColor::Shu, 0xE8603C),
            (AccentColor::Kaki, 0xE58A4E),
            (AccentColor::Sakura, 0xE89BB0),
            (AccentColor::Matcha, 0x9DB86B),
            (AccentColor::Asagi, 0x4FB3B0),
            (AccentColor::Ruri, 0x5B8DEF),
            (AccentColor::Fuji, 0x9D8CE0),
            (AccentColor::Kogane, 0xE3B341),
            (AccentColor::Ume, 0xC2577E),
            (AccentColor::Gin, 0xC9C5BD),
        ] {
            for id in ThemeId::ALL {
                let base = Theme::for_id(id);
                let theme = color.apply(base);
                assert_eq!(theme.accent, gpui::Hsla::from(gpui::rgb(value)));
                for surface in [
                    theme.background,
                    theme.surface,
                    theme.surface_secondary,
                    theme.surface_elevated,
                    theme.surface_hover,
                    theme.surface_active,
                ] {
                    assert!(
                        contrast(theme.accent_text, surface) >= 4.5,
                        "{color:?} {id:?}: {}",
                        contrast(theme.accent_text, surface)
                    );
                }
                for fill in [theme.action, theme.action_hover, theme.action_pressed] {
                    assert!(
                        contrast(theme.action_fg, fill) >= 4.5,
                        "{color:?} {id:?} action: {}",
                        contrast(theme.action_fg, fill)
                    );
                }
                assert_eq!(theme.success, base.success);
                assert_eq!(theme.error, base.error);
                assert_eq!(theme.info, base.info);
            }
        }
    }

    #[test]
    fn persistence_errors_are_returned() {
        let dir =
            std::env::temp_dir().join(format!("orbit-appearance-error-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(AppearancePrefs::default().persist_to(&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[gpui::test]
    fn explicit_mode_ignores_live_system_changes(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let prefs = AppearancePrefs {
                mode: AppearanceMode::Dark,
                dark_theme: ThemeId::Flexoki,
                ..AppearancePrefs::default()
            };
            cx.set_global(prefs);
            cx.set_global(Theme::for_id(ThemeId::Flexoki));
            apply_system_appearance(cx, WindowAppearance::Light);
            assert_eq!(get(cx).theme_id, ThemeId::Flexoki);
            assert_eq!(appearance_prefs(cx), prefs);
        });
    }

    #[test]
    fn dropdown_choices_partition_the_catalog() {
        let light: Vec<_> = themes_for(ThemeMode::Light).collect();
        let dark: Vec<_> = themes_for(ThemeMode::Dark).collect();
        assert_eq!(
            light,
            vec![
                ThemeId::OrbitLight,
                ThemeId::OrbitAzureLight,
                ThemeId::OrbitPaper,
                ThemeId::OrbitContrast,
                ThemeId::FlexokiLight,
                ThemeId::OperationalLight,
            ]
        );
        assert_eq!(
            dark,
            vec![
                ThemeId::Orbit,
                ThemeId::OrbitDarker,
                ThemeId::OrbitCobalt,
                ThemeId::OrbitAzure,
                ThemeId::Flexoki,
                ThemeId::Operational,
            ]
        );
        assert_eq!(light.len() + dark.len(), ThemeId::ALL.len());
        assert!(dark.iter().all(|id| id.appearance() == ThemeMode::Dark));
    }

    #[test]
    fn dropdown_choices_are_filtered_by_design_system() {
        let classic_dark: Vec<_> =
            themes_for_system(ThemeMode::Dark, DesignSystem::Classic).collect();
        assert_eq!(
            classic_dark,
            vec![
                ThemeId::Orbit,
                ThemeId::OrbitDarker,
                ThemeId::OrbitCobalt,
                ThemeId::OrbitAzure,
                ThemeId::Flexoki,
            ]
        );
        let operational_dark: Vec<_> =
            themes_for_system(ThemeMode::Dark, DesignSystem::Operational).collect();
        assert_eq!(operational_dark, vec![ThemeId::Operational]);
        let operational_light: Vec<_> =
            themes_for_system(ThemeMode::Light, DesignSystem::Operational).collect();
        assert_eq!(operational_light, vec![ThemeId::OperationalLight]);
    }

    #[test]
    fn every_theme_round_trips_through_its_key() {
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::parse(id.as_str()), Some(id));
        }
    }

    #[test]
    fn retired_theme_keys_still_load() {
        assert_eq!(ThemeId::parse("dracula"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("nord"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("one-light"), Some(ThemeId::OrbitLight));
        assert_eq!(ThemeId::parse("gruvbox-light"), Some(ThemeId::OrbitLight));
    }

    #[gpui::test]
    fn system_changes_apply_live_and_preserve_ui_preferences(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let prefs = AppearancePrefs {
                accent_color: AccentColor::Coral,
                dark_theme: ThemeId::Flexoki,
                ..AppearancePrefs::default()
            };
            cx.set_global(prefs);
            let ui = super::super::UiPrefs {
                reduce_motion: true,
                ..super::super::UiPrefs::default()
            };
            cx.set_global(Theme::for_id(ThemeId::Flexoki).with_ui(ui));
            apply_system_appearance(cx, WindowAppearance::Light);
            assert_eq!(get(cx).theme_id, ThemeId::OrbitAzureLight);
            assert_eq!(get(cx).accent, Theme::for_id(ThemeId::OrbitLight).accent);
            assert_eq!(get(cx).ui, ui);
            apply_system_appearance(cx, WindowAppearance::Dark);
            assert_eq!(get(cx).theme_id, ThemeId::Flexoki);
            assert_eq!(get(cx).accent, Theme::for_id(ThemeId::Orbit).accent);
            assert_eq!(appearance_prefs(cx), prefs);
        });
    }
}
