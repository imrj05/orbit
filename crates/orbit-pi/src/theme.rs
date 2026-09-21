//! App theme: a GPUI [`Global`] with a selectable palette.
//!
//! Colors are semantic roles (backgrounds, text, borders, transcript
//! surfaces) rather than raw steps, so every paint site reads from one
//! place. A [`ThemeId`] names a palette; [`AppearancePrefs`] chooses which
//! light or dark palette to use, optionally following the system. The UI
//! face is Zed's bundled IBM Plex Sans (`.ZedSans`); code surfaces use
//! Zed's Lilex (`.ZedMono`).

use std::path::PathBuf;
use std::sync::RwLock;

use gpui::{hsla, point, px, rgb, App, BoxShadow, Global, Hsla, Pixels, SharedString};
use serde_json::Value;

use crate::highlight::TokenClass;

mod appearance;
pub use appearance::*;

/// Dark or light appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

/// One of the selectable Orbit palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Orbit,
    OrbitLight,
    /// Ported Zed community themes (all dark).
    Vague,
    Batsignal,
    Ashwood,
    Obsidian,
    MatteBlack,
    AdwaitaPastel,
    Ashen,
    Discord,
    /// Curated editor themes, ported to the Orbit role model (all dark).
    Catppuccin,
    Dracula,
    Gruvbox,
    Nord,
    TokyoNight,
    OneDarkPro,
    OneDarkProMax,
    Monokai,
    SolarizedDark,
    AyuDark,
    Kanagawa,
    Aura,
    Carbonfox,
    FieldsOfTheShire,
    Flexoki,
    JetBrains,
    Mono,
    MonoPlus,
    NightOwl,
    OpenChamber,
    Vesper,
    Vitesse,
}

impl ThemeId {
    /// Selectable themes, in the order shown in the settings dropdown
    /// (the two Orbit palettes first, then the ported/curated dark themes).
    pub const ALL: [ThemeId; 32] = [
        Self::Orbit,
        Self::OrbitLight,
        Self::Vague,
        Self::Batsignal,
        Self::Ashwood,
        Self::Obsidian,
        Self::MatteBlack,
        Self::AdwaitaPastel,
        Self::Ashen,
        Self::Discord,
        Self::Aura,
        Self::AyuDark,
        Self::Carbonfox,
        Self::Catppuccin,
        Self::Dracula,
        Self::FieldsOfTheShire,
        Self::Flexoki,
        Self::Gruvbox,
        Self::JetBrains,
        Self::Kanagawa,
        Self::Mono,
        Self::MonoPlus,
        Self::Monokai,
        Self::NightOwl,
        Self::Nord,
        Self::OneDarkPro,
        Self::OneDarkProMax,
        Self::OpenChamber,
        Self::SolarizedDark,
        Self::TokyoNight,
        Self::Vesper,
        Self::Vitesse,
    ];

    /// Persisted key; also accepts legacy theme names (mapped to Orbit).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Orbit => "orbit",
            Self::OrbitLight => "orbit-light",
            Self::Vague => "vague",
            Self::Batsignal => "batsignal-dark",
            Self::Ashwood => "ashwood",
            Self::Obsidian => "obsidian-dark",
            Self::MatteBlack => "matte-black",
            Self::AdwaitaPastel => "adwaita-pastel-dark",
            Self::Ashen => "ashen",
            Self::Discord => "discord-dark",
            Self::Catppuccin => "catppuccin",
            Self::Dracula => "dracula",
            Self::Gruvbox => "gruvbox",
            Self::Nord => "nord",
            Self::TokyoNight => "tokyo-night",
            Self::OneDarkPro => "one-dark-pro",
            Self::OneDarkProMax => "one-dark-pro-max",
            Self::Monokai => "monokai",
            Self::SolarizedDark => "solarized-dark",
            Self::AyuDark => "ayu-dark",
            Self::Kanagawa => "kanagawa",
            Self::Aura => "aura",
            Self::Carbonfox => "carbonfox",
            Self::FieldsOfTheShire => "fields-of-the-shire",
            Self::Flexoki => "flexoki",
            Self::JetBrains => "jetbrains",
            Self::Mono => "mono",
            Self::MonoPlus => "mono-plus",
            Self::NightOwl => "night-owl",
            Self::OpenChamber => "open-chamber",
            Self::Vesper => "vesper",
            Self::Vitesse => "vitesse",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            // Current keys, plus legacy dark theme names → Orbit.
            "orbit"
            | "dark"
            | "one-dark"
            | "zedokai"
            | "zedokai-darker"
            | "zedokai-filter-octagon"
            | "zedokai-darker-filter-octagon"
            | "zedokai-filter-spectrum"
            | "zedokai-darker-filter-spectrum"
            | "flexoki-dark" => Some(Self::Orbit),
            // Current key, plus legacy light theme names → Orbit Light.
            "orbit-light" | "light" | "one-light" | "flexoki-light" => Some(Self::OrbitLight),
            // Ported Zed community themes (aliases included).
            "vague" => Some(Self::Vague),
            "batsignal-dark" | "batsignal" => Some(Self::Batsignal),
            "ashwood" => Some(Self::Ashwood),
            "obsidian-dark" | "obsidian" => Some(Self::Obsidian),
            "matte-black" | "matte-black-theme" => Some(Self::MatteBlack),
            "adwaita-pastel-dark" | "adwaita-pastel" => Some(Self::AdwaitaPastel),
            "ashen" => Some(Self::Ashen),
            "discord-dark" | "dark-discord" | "discord" => Some(Self::Discord),
            // Curated editor themes (plus common aliases).
            "catppuccin" | "catppuccin-mocha" | "catppuccin-dark" => Some(Self::Catppuccin),
            "dracula" | "dracula-dark" => Some(Self::Dracula),
            "gruvbox" | "gruvbox-dark" => Some(Self::Gruvbox),
            "nord" | "nord-dark" => Some(Self::Nord),
            "tokyo-night" | "tokyonight" | "tokyo-night-storm" => Some(Self::TokyoNight),
            "one-dark-pro" | "onedarkpro" => Some(Self::OneDarkPro),
            "one-dark-pro-max" | "onedarkpromax" | "odpm" => Some(Self::OneDarkProMax),
            "monokai" => Some(Self::Monokai),
            "solarized-dark" | "solarized" => Some(Self::SolarizedDark),
            "ayu-dark" | "ayu" => Some(Self::AyuDark),
            "kanagawa" | "kanagawa-wave" => Some(Self::Kanagawa),
            "aura" | "aura-dark" => Some(Self::Aura),
            "carbonfox" | "carbonfox-dark" => Some(Self::Carbonfox),
            "fields-of-the-shire" | "fields-of-the-shire-dark" => Some(Self::FieldsOfTheShire),
            "flexoki" => Some(Self::Flexoki),
            "jetbrains" | "jetbrains-dark" => Some(Self::JetBrains),
            "mono" => Some(Self::Mono),
            "mono-plus" => Some(Self::MonoPlus),
            "night-owl" | "nightowl" | "night-owl-dark" => Some(Self::NightOwl),
            "open-chamber" | "openchamber" => Some(Self::OpenChamber),
            "vesper" => Some(Self::Vesper),
            "vitesse" => Some(Self::Vitesse),
            _ => None,
        }
    }

    /// Human label for the settings dropdown.
    pub fn label(self) -> &'static str {
        match self {
            Self::Orbit => "Orbit",
            Self::OrbitLight => "Orbit Light",
            Self::Vague => "Vague",
            Self::Batsignal => "Batsignal (Dark)",
            Self::Ashwood => "Ashwood",
            Self::Obsidian => "Obsidian Dark",
            Self::MatteBlack => "Matte Black",
            Self::AdwaitaPastel => "Adwaita Pastel Dark",
            Self::Ashen => "Ashen",
            Self::Discord => "Discord Dark",
            Self::Catppuccin => "Catppuccin",
            Self::Dracula => "Dracula",
            Self::Gruvbox => "Gruvbox",
            Self::Nord => "Nord",
            Self::TokyoNight => "Tokyonight",
            Self::OneDarkPro => "One Dark Pro",
            Self::OneDarkProMax => "One Dark Pro Max",
            Self::Monokai => "Monokai",
            Self::SolarizedDark => "Solarized",
            Self::AyuDark => "Ayu",
            Self::Kanagawa => "Kanagawa",
            Self::Aura => "Aura",
            Self::Carbonfox => "Carbonfox",
            Self::FieldsOfTheShire => "Fields of the Shire",
            Self::Flexoki => "Flexoki",
            Self::JetBrains => "JetBrains",
            Self::Mono => "Mono",
            Self::MonoPlus => "Mono Plus",
            Self::NightOwl => "Night Owl",
            Self::OpenChamber => "OpenChamber",
            Self::Vesper => "Vesper",
            Self::Vitesse => "Vitesse",
        }
    }

    pub fn appearance(self) -> ThemeMode {
        match self {
            Self::OrbitLight => ThemeMode::Light,
            Self::Orbit
            | Self::Vague
            | Self::Batsignal
            | Self::Ashwood
            | Self::Obsidian
            | Self::MatteBlack
            | Self::AdwaitaPastel
            | Self::Ashen
            | Self::Discord
            | Self::Catppuccin
            | Self::Dracula
            | Self::Gruvbox
            | Self::Nord
            | Self::TokyoNight
            | Self::OneDarkPro
            | Self::OneDarkProMax
            | Self::Monokai
            | Self::SolarizedDark
            | Self::AyuDark
            | Self::Kanagawa
            | Self::Aura
            | Self::Carbonfox
            | Self::FieldsOfTheShire
            | Self::Flexoki
            | Self::JetBrains
            | Self::Mono
            | Self::MonoPlus
            | Self::NightOwl
            | Self::OpenChamber
            | Self::Vesper
            | Self::Vitesse => ThemeMode::Dark,
        }
    }
}

/// Semantic palette. Copy so list closures and hover styles can capture it
/// without a lifetime.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub theme_id: ThemeId,
    pub mode: ThemeMode,
    /// UI customization (Waku's General settings): language + font sizes.
    pub ui: UiPrefs,
    pub bg_main: Hsla,
    pub bg_sidebar: Hsla,
    pub bg_composer: Hsla,
    pub bg_raised: Hsla,
    pub bg_hover: Hsla,
    /// Background for pressed/active and highlighted (selected) elements.
    pub active: Hsla,
    /// Foreground (text/icon) drawn on [`Self::active`].
    pub active_fg: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_2: Hsla,
    pub text_3: Hsla,
    pub ok_green: Hsla,
    pub stop_red: Hsla,
    pub stop_red_hover: Hsla,
    pub add_green: Hsla,
    pub del_red: Hsla,
    /// Single brand accent — selection, links, spark, checks, caret, etc.
    pub accent: Hsla,
    pub menu_bg: Hsla,
    pub send_bg: Hsla,
    pub send_bg_hover: Hsla,
    pub send_fg: Hsla,
    pub overlay: Hsla,
    pub overlay_strong: Hsla,
    pub border_strong: Hsla,
    pub assistant_text: Hsla,
    pub code_bg: Hsla,
    pub code_text: Hsla,
    /// Syntax token colors (transcript code blocks + the review diff). A
    /// palette sets these explicitly so a ported editor theme keeps its own
    /// syntax identity instead of borrowing the UI's semantic roles.
    pub syn_string: Hsla,
    pub syn_number: Hsla,
    pub syn_function: Hsla,
    pub syn_type: Hsla,
    pub syn_comment: Hsla,
    pub syn_literal: Hsla,
    pub syn_meta: Hsla,
    pub syn_operator: Hsla,
    /// Inline-code chip wash (text uses [`Self::accent`]).
    pub inline_code_bg: Hsla,
    pub tool_border: Hsla,
    pub tool_meta: Hsla,
    pub ring_track: Hsla,
    pub ring_fill: Hsla,
    pub warn: Hsla,
    pub crit: Hsla,
    pub trough: Hsla,
    pub shadow_contact: Hsla,
    pub shadow_ambient: Hsla,
}

impl Global for Theme {}

/// Interface language for the workbench chrome. Re-exported from
/// [`crate::i18n`] so every persisted pref reads as a locale id while the
/// `Language` name stays stable for the settings surface.
pub use crate::i18n::AppLanguage as Language;

/// Waku General-settings customization: language, type sizes, and density.
/// Every size is px with the Waku defaults (UI 14, terminal / editor 13);
/// only spacing density remains a percentage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiPrefs {
    pub language: Language,
    /// Interface text size, px (Waku's "UI font size").
    pub ui_font_size: f32,
    /// Terminal / tool-output size, px (Waku's "Terminal Font Size").
    pub terminal_font_size: f32,
    /// Editor / diff / code-block size, px (Waku's "Editor Font Size").
    pub editor_font_size: f32,
    /// Global spacing multiplier, percent (Waku's "Spacing Density").
    pub spacing_density: u32,
    /// Honor reduce-motion: perpetual/looping animations (spinners, the
    /// running-session shimmer, the drop-overlay fade) render static.
    pub reduce_motion: bool,
}

/// The interface text size authored against, px. Chrome sizes scale relative
/// to this, exactly like Waku's `sp()` authored at its default UI font size.
pub const DEFAULT_UI_FONT_SIZE: f32 = 14.;
/// Selectable interface / terminal / editor font sizes, px.
pub const FONT_SIZES: [f32; 8] = [11., 12., 13., 14., 15., 16., 18., 20.];
/// Selectable spacing densities, percent.
pub const SPACING_DENSITIES: [u32; 9] = [80, 85, 90, 95, 100, 105, 110, 115, 120];

impl Default for UiPrefs {
    fn default() -> Self {
        Self {
            language: Language::System,
            ui_font_size: DEFAULT_UI_FONT_SIZE,
            terminal_font_size: 13.,
            editor_font_size: 13.,
            spacing_density: 100,
            reduce_motion: false,
        }
    }
}

impl UiPrefs {
    fn persist_path() -> PathBuf {
        crate::platform::home_dir()
            .join(".orbit-pi")
            .join("ui.json")
    }

    fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::persist_path()) else {
            return Self::default();
        };
        serde_json::from_str::<Value>(&raw)
            .map(|value| Self::from_value(&value))
            .unwrap_or_default()
    }

    /// Nearest selectable font size, for migrating legacy percentage scales.
    fn nearest_size(value: f32) -> f32 {
        *FONT_SIZES
            .iter()
            .min_by(|a, b| (*a - value).abs().partial_cmp(&(*b - value).abs()).unwrap())
            .unwrap()
    }

    /// Parse prefs from JSON, falling back to defaults per field. A legacy
    /// `interface_scale` percentage migrates onto the equivalent px size.
    fn from_value(value: &Value) -> Self {
        let mut prefs = Self::default();
        if let Some(language) = value
            .get("language")
            .and_then(Value::as_str)
            .and_then(Language::parse)
        {
            prefs.language = language;
        }
        if let Some(size) = value.get("ui_font_size").and_then(Value::as_f64) {
            let size = size as f32;
            if FONT_SIZES.contains(&size) {
                prefs.ui_font_size = size;
            }
        } else if let Some(scale) = value.get("interface_scale").and_then(Value::as_u64) {
            let px = scale as f32 / 100. * DEFAULT_UI_FONT_SIZE;
            prefs.ui_font_size = Self::nearest_size(px);
        }
        if let Some(size) = value.get("terminal_font_size").and_then(Value::as_f64) {
            if FONT_SIZES.contains(&(size as f32)) {
                prefs.terminal_font_size = size as f32;
            }
        }
        if let Some(size) = value.get("editor_font_size").and_then(Value::as_f64) {
            if FONT_SIZES.contains(&(size as f32)) {
                prefs.editor_font_size = size as f32;
            }
        } else if let Some(size) = value.get("code_font_size").and_then(Value::as_f64) {
            let size = size as f32;
            if FONT_SIZES.contains(&size) {
                prefs.editor_font_size = size;
            }
        }
        if let Some(density) = value.get("spacing_density").and_then(Value::as_u64) {
            let density = density as u32;
            if SPACING_DENSITIES.contains(&density) {
                prefs.spacing_density = density;
            }
        }
        if let Some(reduce) = value.get("reduce_motion").and_then(Value::as_bool) {
            prefs.reduce_motion = reduce;
        }
        prefs
    }

    fn persist(self) {
        let path = Self::persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            path,
            serde_json::json!({
                "language": self.language.as_str(),
                "ui_font_size": self.ui_font_size,
                "terminal_font_size": self.terminal_font_size,
                "editor_font_size": self.editor_font_size,
                "spacing_density": self.spacing_density,
                "reduce_motion": self.reduce_motion,
            })
            .to_string(),
        );
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::for_id(ThemeId::Orbit)
    }
}

/// Current UI / code font families. Stored as statics (readable without an
/// app handle) so transcript helpers, which don't carry `cx`, can look them
/// up — keeping [`Theme`] itself `Copy`. Persisted to `~/.orbit-pi/fonts.json`.
static UI_FONT_FAMILY: RwLock<SharedString> = RwLock::new(SharedString::new_static(".ZedSans"));
static CODE_FONT_FAMILY: RwLock<SharedString> = RwLock::new(SharedString::new_static(".ZedMono"));

/// The persisted font-families payload.
#[derive(Debug, Clone, PartialEq)]
pub struct FontPrefs {
    /// UI face family (`.ZedSans` → bundled IBM Plex Sans by default).
    pub ui_font_family: SharedString,
    /// Code/monospace face family (`.ZedMono` → bundled Lilex by default).
    pub code_font_family: SharedString,
}

impl Default for FontPrefs {
    fn default() -> Self {
        Self {
            ui_font_family: ".ZedSans".into(),
            code_font_family: ".ZedMono".into(),
        }
    }
}

impl FontPrefs {
    fn persist_path() -> PathBuf {
        crate::platform::home_dir()
            .join(".orbit-pi")
            .join("fonts.json")
    }

    fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::persist_path()) else {
            return Self::default();
        };
        let Ok(value) = serde_json::from_str::<Value>(&raw) else {
            return Self::default();
        };
        let mut prefs = Self::default();
        if let Some(family) = value.get("ui_font_family").and_then(Value::as_str) {
            if !family.is_empty() {
                prefs.ui_font_family = family.to_string().into();
            }
        }
        if let Some(family) = value.get("code_font_family").and_then(Value::as_str) {
            if !family.is_empty() {
                prefs.code_font_family = family.to_string().into();
            }
        }
        prefs
    }

    fn persist(&self) {
        let path = Self::persist_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(
            path,
            serde_json::json!({
                "ui_font_family": self.ui_font_family.to_string(),
                "code_font_family": self.code_font_family.to_string(),
            })
            .to_string(),
        );
    }
}

/// Current UI face family (readable without `cx`).
pub fn ui_font_family() -> SharedString {
    UI_FONT_FAMILY.read().unwrap().clone()
}

/// Current code/mono face family (readable without `cx`).
pub fn code_font_family() -> SharedString {
    CODE_FONT_FAMILY.read().unwrap().clone()
}

/// A named entry in the curated Interface / Code font pickers. `family` is
/// the real family gpui resolves; `label` is what the settings chip shows.
#[derive(Debug, Clone, Copy)]
pub struct FontChoice {
    pub label: &'static str,
    pub family: &'static str,
}

/// Orbit's curated interface faces (Waku's list + a System escape hatch).
pub const UI_FONTS: [FontChoice; 10] = [
    FontChoice {
        label: "Inter",
        family: "Inter",
    },
    FontChoice {
        label: "Fixel Text",
        family: "Fixel Text",
    },
    FontChoice {
        label: "Geist Sans",
        family: "Geist",
    },
    FontChoice {
        label: "Atkinson Hyperlegible",
        family: "Atkinson Hyperlegible",
    },
    FontChoice {
        label: "Source Sans 3",
        family: "Source Sans 3",
    },
    FontChoice {
        label: "Roboto",
        family: "Roboto",
    },
    FontChoice {
        label: "Noto Sans",
        family: "Noto Sans",
    },
    FontChoice {
        label: "DM Sans",
        family: "DM Sans",
    },
    FontChoice {
        label: "Manrope",
        family: "Manrope",
    },
    FontChoice {
        label: "IBM Plex Sans",
        family: ".ZedSans",
    },
];

/// Orbit's curated code faces (Waku's list + a System Mono escape hatch).
pub const CODE_FONTS: [FontChoice; 9] = [
    FontChoice {
        label: "JetBrains Mono",
        family: "JetBrains Mono",
    },
    FontChoice {
        label: "Fira Code",
        family: "Fira Code",
    },
    FontChoice {
        label: "Geist Mono",
        family: "Geist Mono",
    },
    FontChoice {
        label: "Commit Mono",
        family: "CommitMono",
    },
    FontChoice {
        label: "Source Code Pro",
        family: "Source Code Pro",
    },
    FontChoice {
        label: "Cascadia Code",
        family: "Cascadia Code",
    },
    FontChoice {
        label: "Roboto Mono",
        family: "Roboto Mono",
    },
    FontChoice {
        label: "Iosevka",
        family: "Iosevka",
    },
    FontChoice {
        label: "Lilex",
        family: ".ZedMono",
    },
];

/// Display label for a stored family name: a curated label when one exists,
/// otherwise the raw family (so legacy/installed choices still read right).
pub fn font_choice_label(family: &str) -> String {
    if family == ".ZedSans" {
        return "IBM Plex Sans".into();
    }
    if family == ".ZedMono" {
        return "Lilex".into();
    }
    for choice in UI_FONTS.iter().chain(CODE_FONTS.iter()) {
        if choice.family == family {
            return choice.label.into();
        }
    }
    family.to_string()
}

/// Current font families as a struct (for the settings UI).
pub fn font_prefs() -> FontPrefs {
    FontPrefs {
        ui_font_family: ui_font_family(),
        code_font_family: code_font_family(),
    }
}

/// The raw color tokens for one palette. Derived washes (hover overlay,
/// strong border, inline-code wash) and shadows are computed from these in
/// [`Theme::build`], so a palette only carries its actual colors.
#[derive(Clone, Copy)]
struct Palette {
    bg_main: u32,
    bg_sidebar: u32,
    bg_raised: u32,
    bg_hover: u32,
    active: u32,
    active_fg: u32,
    border: u32,
    text: u32,
    text_2: u32,
    text_3: u32,
    ok_green: u32,
    stop_red: u32,
    stop_red_hover: u32,
    add_green: u32,
    del_red: u32,
    accent: u32,
    menu_bg: u32,
    send_bg: u32,
    send_bg_hover: u32,
    send_fg: u32,
    assistant_text: u32,
    code_bg: u32,
    code_text: u32,
    syn_string: u32,
    syn_number: u32,
    syn_function: u32,
    syn_type: u32,
    syn_comment: u32,
    syn_literal: u32,
    syn_meta: u32,
    syn_operator: u32,
    tool_border: u32,
    tool_meta: u32,
    ring_track: u32,
    ring_fill: u32,
    warn: u32,
    crit: u32,
    trough: u32,
}

/// Waku dark: near-black canvas, light primary buttons, orange ring accent.
const ORBIT: Palette = Palette {
    bg_main: 0x1A1A1A,
    bg_sidebar: 0x181818,
    bg_raised: 0x232323,
    bg_hover: 0x262626,
    active: 0x2A2A2A,
    active_fg: 0xE2E2E2,
    border: 0x2A2A2A,
    text: 0xE2E2E2,
    text_2: 0xA3A3A3,
    text_3: 0x7D7D7D,
    ok_green: 0x62C987,
    stop_red: 0xE2726A,
    stop_red_hover: 0xEC8A82,
    add_green: 0x62C987,
    del_red: 0xE2726A,
    accent: 0xE2795B,
    menu_bg: 0x232323,
    send_bg: 0xE7E9EC,
    send_bg_hover: 0xF2F3F6,
    send_fg: 0x17181C,
    assistant_text: 0xE2E2E2,
    code_bg: 0x151515,
    code_text: 0xE2E2E2,
    // Syntax is semantic here: strings green, numbers amber, meta red.
    syn_string: 0x62C987,
    syn_number: 0xE0B36A,
    syn_function: 0xE2E2E2,
    syn_type: 0xE2E2E2,
    syn_comment: 0xA3A3A3,
    syn_literal: 0xE2795B,
    syn_meta: 0xE2726A,
    syn_operator: 0x7D7D7D,
    tool_border: 0x2A2A2A,
    tool_meta: 0xA3A3A3,
    ring_track: 0x232323,
    ring_fill: 0xE2E2E2,
    warn: 0xE0B36A,
    crit: 0xE2726A,
    trough: 0x232323,
};

/// Waku light: warm off-white canvas, dark primary buttons, orange accent.
const ORBIT_LIGHT: Palette = Palette {
    bg_main: 0xF6F5F6,
    bg_sidebar: 0xF3F3F3,
    bg_raised: 0xECECEC,
    bg_hover: 0xE8E8E8,
    active: 0xE2E2E2,
    active_fg: 0x242424,
    border: 0xE2E2E2,
    text: 0x242424,
    text_2: 0x666666,
    text_3: 0x858585,
    ok_green: 0x297E48,
    stop_red: 0xC3433B,
    stop_red_hover: 0xD5604F,
    add_green: 0x297E48,
    del_red: 0xC3433B,
    accent: 0xB55035,
    menu_bg: 0xECECEC,
    send_bg: 0x202227,
    send_bg_hover: 0x34363C,
    send_fg: 0xF8F8F9,
    assistant_text: 0x242424,
    code_bg: 0xE6E6E6,
    code_text: 0x242424,
    syn_string: 0x297E48,
    syn_number: 0x99631E,
    syn_function: 0x242424,
    syn_type: 0x242424,
    syn_comment: 0x666666,
    syn_literal: 0xB55035,
    syn_meta: 0xC3433B,
    syn_operator: 0x6F6F6F,
    tool_border: 0xE2E2E2,
    tool_meta: 0x666666,
    ring_track: 0xECECEC,
    ring_fill: 0x242424,
    warn: 0x99631E,
    crit: 0xC3433B,
    trough: 0xECECEC,
};

/// Vague (dark): a low-contrast, near-monochrome editor palette ported from
/// the Zed theme `Vague` (<https://github.com/vague-theme/vague-zed>), tuned
/// to Orbit's semantic roles. Muted blue is the lone accent; syntax uses the
/// theme's own token colors rather than the UI's greens/ambers.
const VAGUE: Palette = Palette {
    bg_main: 0x141415,
    bg_sidebar: 0x171718,
    bg_raised: 0x252530,
    bg_hover: 0x2E2E3B,
    active: 0x3C3C4E,
    active_fg: 0xCDCDCD,
    border: 0x2A2A37,
    text: 0xCDCDCD,
    text_2: 0x878787,
    text_3: 0x64647E,
    ok_green: 0x7FA563,
    stop_red: 0xD8647E,
    stop_red_hover: 0xE08398,
    add_green: 0x7FA563,
    del_red: 0xD8647E,
    accent: 0x6E94B2,
    menu_bg: 0x252530,
    send_bg: 0xCDCDCD,
    send_bg_hover: 0xBEBEBE,
    send_fg: 0x141415,
    assistant_text: 0xCDCDCD,
    code_bg: 0x18181F,
    code_text: 0xCDCDCD,
    syn_string: 0xE8B589,
    syn_number: 0xE0A363,
    syn_function: 0xC48282,
    syn_type: 0x9BB4BC,
    syn_comment: 0x63637C,
    syn_literal: 0xE0A363,
    syn_meta: 0xAEAED1,
    syn_operator: 0x90A0B5,
    tool_border: 0x2A2A37,
    tool_meta: 0x878787,
    ring_track: 0x30303E,
    ring_fill: 0xCDCDCD,
    warn: 0xF3BE7C,
    crit: 0xD8647E,
    trough: 0x19191A,
};

/// Batsignal (Dark): ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const BATSIGNAL: Palette = Palette {
    bg_main: 0x000000,
    bg_sidebar: 0x030303,
    bg_raised: 0x0F0F0F,
    bg_hover: 0x191919,
    active: 0x2A2A2A,
    active_fg: 0xB3B3B3,
    border: 0x212121,
    text: 0xB3B3B3,
    text_2: 0x767676,
    text_3: 0x5E5E5E,
    ok_green: 0x62C987,
    stop_red: 0xF44747,
    stop_red_hover: 0xF66868,
    add_green: 0x62C987,
    del_red: 0xF44747,
    accent: 0xE7C24A,
    menu_bg: 0x0F0F0F,
    send_bg: 0xB3B3B3,
    send_bg_hover: 0xA4A4A4,
    send_fg: 0x000000,
    assistant_text: 0xB3B3B3,
    code_bg: 0x070707,
    code_text: 0xB3B3B3,
    syn_string: 0xAAAAAA,
    syn_number: 0xAAAAAA,
    syn_function: 0xE7C24A,
    syn_type: 0xB3B3B3,
    syn_comment: 0x606060,
    syn_literal: 0xAAAAAA,
    syn_meta: 0x777777,
    syn_operator: 0xB3B3B3,
    tool_border: 0x212121,
    tool_meta: 0x767676,
    ring_track: 0x1C1C1C,
    ring_fill: 0xB3B3B3,
    warn: 0xCD9731,
    crit: 0xF44747,
    trough: 0x050505,
};

/// Ashwood: ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const ASHWOOD: Palette = Palette {
    bg_main: 0x111111,
    bg_sidebar: 0x151515,
    bg_raised: 0x1E1E1E,
    bg_hover: 0x282828,
    active: 0x2E2E2E,
    active_fg: 0xE1E1E1,
    border: 0x3A3A3A,
    text: 0xA9A9A9,
    text_2: 0x7E7E7E,
    text_3: 0x656565,
    ok_green: 0x62C987,
    stop_red: 0xC4909A,
    stop_red_hover: 0xCFA4AC,
    add_green: 0x62C987,
    del_red: 0xC4909A,
    accent: 0xB4B4B4,
    menu_bg: 0x1E1E1E,
    send_bg: 0xA9A9A9,
    send_bg_hover: 0x9A9A9A,
    send_fg: 0x111111,
    assistant_text: 0xA9A9A9,
    code_bg: 0x161616,
    code_text: 0xA9A9A9,
    syn_string: 0x8DBBA3,
    syn_number: 0xABB6E0,
    syn_function: 0xC0B0DF,
    syn_type: 0xABB6E0,
    syn_comment: 0x8D909C,
    syn_literal: 0xABB6E0,
    syn_meta: 0xC0B0DF,
    syn_operator: 0xDEA8B3,
    tool_border: 0x3A3A3A,
    tool_meta: 0x7E7E7E,
    ring_track: 0x2B2B2B,
    ring_fill: 0xA9A9A9,
    warn: 0xE99696,
    crit: 0xC4909A,
    trough: 0x161616,
};

/// Obsidian Dark: ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const OBSIDIAN: Palette = Palette {
    bg_main: 0x161616,
    bg_sidebar: 0x101010,
    bg_raised: 0x222222,
    bg_hover: 0x2C2C2C,
    active: 0x3D3D3D,
    active_fg: 0xDEDEDE,
    border: 0x2D2D2D,
    text: 0xDEDEDE,
    text_2: 0x888888,
    text_3: 0x686868,
    ok_green: 0x3DBA6F,
    stop_red: 0xD96B6B,
    stop_red_hover: 0xE08686,
    add_green: 0x3DBA6F,
    del_red: 0xD96B6B,
    accent: 0x5CA8CF,
    menu_bg: 0x222222,
    send_bg: 0xDEDEDE,
    send_bg_hover: 0xCFCFCF,
    send_fg: 0x161616,
    assistant_text: 0xDEDEDE,
    code_bg: 0x101010,
    code_text: 0xDEDEDE,
    syn_string: 0x5AAD7A,
    syn_number: 0x5CA8CF,
    syn_function: 0xE2E2E2,
    syn_type: 0xAAAAAA,
    syn_comment: 0x646464,
    syn_literal: 0x5CA8CF,
    syn_meta: 0x5CA8CF,
    syn_operator: 0x888888,
    tool_border: 0x2D2D2D,
    tool_meta: 0x888888,
    ring_track: 0x2F2F2F,
    ring_fill: 0xDEDEDE,
    warn: 0xC8964A,
    crit: 0xD96B6B,
    trough: 0x1B1B1B,
};

/// Matte Black: ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const MATTE_BLACK: Palette = Palette {
    bg_main: 0x1A1A1A,
    bg_sidebar: 0x1D1D1D,
    bg_raised: 0x252525,
    bg_hover: 0x3A3A3A,
    active: 0x4B4B4B,
    active_fg: 0xE0E0E0,
    border: 0x303030,
    text: 0xE0E0E0,
    text_2: 0x888888,
    text_3: 0x6A6A6A,
    ok_green: 0x69F0AE,
    stop_red: 0xFF5252,
    stop_red_hover: 0xFF7171,
    add_green: 0x69F0AE,
    del_red: 0xFF5252,
    accent: 0x40C4FF,
    menu_bg: 0x252525,
    send_bg: 0xE0E0E0,
    send_bg_hover: 0xD1D1D1,
    send_fg: 0x1A1A1A,
    assistant_text: 0xE0E0E0,
    code_bg: 0x252525,
    code_text: 0xE0E0E0,
    syn_string: 0xC3E88D,
    syn_number: 0xFF7043,
    syn_function: 0x0D7FD8,
    syn_type: 0xD4A574,
    syn_comment: 0x737373,
    syn_literal: 0xFF9E80,
    syn_meta: 0x80D8FF,
    syn_operator: 0xD0D0D0,
    tool_border: 0x303030,
    tool_meta: 0x888888,
    ring_track: 0x323232,
    ring_fill: 0xE0E0E0,
    warn: 0xFFD740,
    crit: 0xFF5252,
    trough: 0x1F1F1F,
};

/// Adwaita Pastel Dark: ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const ADWAITA_PASTEL: Palette = Palette {
    bg_main: 0x1E1E1E,
    bg_sidebar: 0x303030,
    bg_raised: 0x3B3B3B,
    bg_hover: 0x454545,
    active: 0x515151,
    active_fg: 0xE7E7E7,
    border: 0x4F4F4F,
    text: 0xE7E7E7,
    text_2: 0xC7C7C7,
    text_3: 0x808080,
    ok_green: 0x57E389,
    stop_red: 0xED333B,
    stop_red_hover: 0xF0585E,
    add_green: 0x57E389,
    del_red: 0xED333B,
    accent: 0x3686E7,
    menu_bg: 0x3B3B3B,
    send_bg: 0xE7E7E7,
    send_bg_hover: 0xD8D8D8,
    send_fg: 0x1E1E1E,
    assistant_text: 0xE7E7E7,
    code_bg: 0x303030,
    code_text: 0xE7E7E7,
    syn_string: 0xA6E3A1,
    syn_number: 0xFAB387,
    syn_function: 0x89B4FA,
    syn_type: 0xF9E2AF,
    syn_comment: 0x7F849C,
    syn_literal: 0xFAB387,
    syn_meta: 0xF9E2AF,
    syn_operator: 0x89DCEB,
    tool_border: 0x4F4F4F,
    tool_meta: 0xC7C7C7,
    ring_track: 0x484848,
    ring_fill: 0xE7E7E7,
    warn: 0xF8E45C,
    crit: 0xED333B,
    trough: 0x232323,
};

/// Ashen: ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const ASHEN: Palette = Palette {
    bg_main: 0x121212,
    bg_sidebar: 0x151515,
    bg_raised: 0x212121,
    bg_hover: 0x323232,
    active: 0x434343,
    active_fg: 0xC0C0C0,
    border: 0x323232,
    text: 0xB4B4B4,
    text_2: 0x949494,
    text_3: 0x656565,
    ok_green: 0x629C7D,
    stop_red: 0xC53030,
    stop_red_hover: 0xCF5555,
    add_green: 0x629C7D,
    del_red: 0xC53030,
    accent: 0x7FA9CE,
    menu_bg: 0x212121,
    send_bg: 0xB4B4B4,
    send_bg_hover: 0xA5A5A5,
    send_fg: 0x121212,
    assistant_text: 0xB4B4B4,
    code_bg: 0x151515,
    code_text: 0xB4B4B4,
    syn_string: 0xDF6464,
    syn_number: 0x4A8B8B,
    syn_function: 0xE5E5E5,
    syn_type: 0xC4693D,
    syn_comment: 0x737373,
    syn_literal: 0x4A8B8B,
    syn_meta: 0xA7A7A7,
    syn_operator: 0xD87C4A,
    tool_border: 0x323232,
    tool_meta: 0x949494,
    ring_track: 0x2E2E2E,
    ring_fill: 0xB4B4B4,
    warn: 0xE5A72A,
    crit: 0xC53030,
    trough: 0x171717,
};

/// Discord Dark: ported from the Zed community theme of the same name
/// (via zed-themes.com). Dark appearance.
const DISCORD: Palette = Palette {
    bg_main: 0x121214,
    bg_sidebar: 0x151517,
    bg_raised: 0x1E1E22,
    bg_hover: 0x28282C,
    active: 0x37373E,
    active_fg: 0xB5B4B4,
    border: 0x2A2A2E,
    text: 0xB5B4B4,
    text_2: 0x959DA5,
    text_3: 0x6D6D73,
    ok_green: 0x4D9375,
    stop_red: 0xCB7676,
    stop_red_hover: 0xD48F8F,
    add_green: 0x4D9375,
    del_red: 0xCB7676,
    accent: 0x5197ED,
    menu_bg: 0x1E1E22,
    send_bg: 0xB5B4B4,
    send_bg_hover: 0xA6A5A5,
    send_fg: 0x121214,
    assistant_text: 0xB5B4B4,
    code_bg: 0x1A1A1E,
    code_text: 0xB5B4B4,
    syn_string: 0xC98A7D,
    syn_number: 0x4C9A91,
    syn_function: 0x80A665,
    syn_type: 0x5D99A9,
    syn_comment: 0x687668,
    syn_literal: 0x4D9375,
    syn_meta: 0xB8A965,
    syn_operator: 0xCB7676,
    tool_border: 0x2A2A2E,
    tool_meta: 0x959DA5,
    ring_track: 0x2A2A30,
    ring_fill: 0xB5B4B4,
    warn: 0xE6CC77,
    crit: 0xCB7676,
    trough: 0x171719,
};

/// Catppuccin — ported from OpenChamber's `catppuccin-dark`.
const CATPPUCCIN: Palette = Palette {
    bg_main: 0x1E1E2E,
    bg_sidebar: 0x2A273B,
    bg_raised: 0x282841,
    bg_hover: 0x34344A,
    active: 0x44425C,
    active_fg: 0xF4F2FF,
    border: 0x35324A,
    text: 0xCDD6F4,
    text_2: 0x969CB1,
    text_3: 0x72768A,
    ok_green: 0xA6D189,
    stop_red: 0xF38BA8,
    stop_red_hover: 0xF5A0B8,
    add_green: 0xA6D189,
    del_red: 0xF38BA8,
    accent: 0x7D8FFF,
    menu_bg: 0x282841,
    send_bg: 0x7D8FFF,
    send_bg_hover: 0x99A7FF,
    send_fg: 0x1E1E2E,
    assistant_text: 0xCDD6F4,
    code_bg: 0x211F31,
    code_text: 0xCDD6F4,
    syn_string: 0xA6E3A1,
    syn_number: 0xF38BA8,
    syn_function: 0xB4BEFE,
    syn_type: 0xF9E2AF,
    syn_comment: 0xA6ADC8,
    syn_literal: 0xF38BA8,
    syn_meta: 0xF4B8E4,
    syn_operator: 0xF38BA8,
    tool_border: 0x35324A,
    tool_meta: 0x969CB1,
    ring_track: 0x323251,
    ring_fill: 0xCDD6F4,
    warn: 0xF4B8E4,
    crit: 0xF38BA8,
    trough: 0x222234,
};

/// Dracula — ported from OpenChamber's `dracula-dark`.
const DRACULA: Palette = Palette {
    bg_main: 0x14151F,
    bg_sidebar: 0x181926,
    bg_raised: 0x1E202E,
    bg_hover: 0x26283B,
    active: 0x33364F,
    active_fg: 0xFFFFFF,
    border: 0x2B2C38,
    text: 0xF8F8F2,
    text_2: 0x7C7E9C,
    text_3: 0x64657F,
    ok_green: 0x50FA7B,
    stop_red: 0xFF5555,
    stop_red_hover: 0xFF7474,
    add_green: 0x50FA7B,
    del_red: 0xFF5555,
    accent: 0xBD93F9,
    menu_bg: 0x1E202E,
    send_bg: 0xBD93F9,
    send_bg_hover: 0xCDAEFA,
    send_fg: 0x14151F,
    assistant_text: 0xF8F8F2,
    code_bg: 0x181926,
    code_text: 0xF8F8F2,
    syn_string: 0x50FA7B,
    syn_number: 0xFF79C6,
    syn_function: 0xBD93F9,
    syn_type: 0xFFB86C,
    syn_comment: 0xB6B9E4,
    syn_literal: 0xFF79C6,
    syn_meta: 0xFFB86C,
    syn_operator: 0xFF79C6,
    tool_border: 0x2B2C38,
    tool_meta: 0x7C7E9C,
    ring_track: 0x282B3D,
    ring_fill: 0xF8F8F2,
    warn: 0xFFB86C,
    crit: 0xFF5555,
    trough: 0x181925,
};

/// Gruvbox — ported from OpenChamber's `gruvbox-dark`.
const GRUVBOX: Palette = Palette {
    bg_main: 0x282828,
    bg_sidebar: 0x32302F,
    bg_raised: 0x313639,
    bg_hover: 0x534F42,
    active: 0x656151,
    active_fg: 0xFBF1C7,
    border: 0x453F3B,
    text: 0xEBDBB2,
    text_2: 0x948877,
    text_3: 0x7E7467,
    ok_green: 0xB8BB26,
    stop_red: 0xFB4934,
    stop_red_hover: 0xFC6A59,
    add_green: 0xB8BB26,
    del_red: 0xFB4934,
    accent: 0x83A598,
    menu_bg: 0x313639,
    send_bg: 0x83A598,
    send_bg_hover: 0x93B1A6,
    send_fg: 0x282828,
    assistant_text: 0xEBDBB2,
    code_bg: 0x32302F,
    code_text: 0xEBDBB2,
    syn_string: 0xB8BB26,
    syn_number: 0xFB4934,
    syn_function: 0x83A598,
    syn_type: 0xFABD2F,
    syn_comment: 0xA89984,
    syn_literal: 0xFB4934,
    syn_meta: 0xFABD2F,
    syn_operator: 0xFB4934,
    tool_border: 0x453F3B,
    tool_meta: 0x918574,
    ring_track: 0x3D4347,
    ring_fill: 0xEBDBB2,
    warn: 0xFABD2F,
    crit: 0xFB4934,
    trough: 0x2D2D2D,
};

/// Nord — ported from OpenChamber's `nord-dark`.
const NORD: Palette = Palette {
    bg_main: 0x1F2430,
    bg_sidebar: 0x222938,
    bg_raised: 0x2A303E,
    bg_hover: 0x313A46,
    active: 0x3E4A56,
    active_fg: 0xF8FAFC,
    border: 0x383D49,
    text: 0xE5E9F0,
    text_2: 0x808692,
    text_3: 0x6B727F,
    ok_green: 0xA3BE8C,
    stop_red: 0xBF616A,
    stop_red_hover: 0xCB7D85,
    add_green: 0xA3BE8C,
    del_red: 0xBF616A,
    accent: 0x88C0D0,
    menu_bg: 0x2A303E,
    send_bg: 0x88C0D0,
    send_bg_hover: 0x9CCBD8,
    send_fg: 0x1F2430,
    assistant_text: 0xE5E9F0,
    code_bg: 0x222938,
    code_text: 0xE5E9F0,
    syn_string: 0xA3BE8C,
    syn_number: 0xD57780,
    syn_function: 0x88C0D0,
    syn_type: 0xEAC196,
    syn_comment: 0xA4ADBF,
    syn_literal: 0xD57780,
    syn_meta: 0xD08770,
    syn_operator: 0xD57780,
    tool_border: 0x383D49,
    tool_meta: 0x7C828E,
    ring_track: 0x343C4D,
    ring_fill: 0xE5E9F0,
    warn: 0xD08770,
    crit: 0xBF616A,
    trough: 0x232936,
};

/// Tokyonight — ported from OpenChamber's `tokyonight-dark`.
const TOKYO_NIGHT: Palette = Palette {
    bg_main: 0x0F111A,
    bg_sidebar: 0x111428,
    bg_raised: 0x15192D,
    bg_hover: 0x272E49,
    active: 0x3F4152,
    active_fg: 0xEAEAFF,
    border: 0x2D2F43,
    text: 0xC0CAF5,
    text_2: 0x8890B3,
    text_3: 0x646A85,
    ok_green: 0x9ECE6A,
    stop_red: 0xF7768E,
    stop_red_hover: 0xF88FA2,
    add_green: 0x9ECE6A,
    del_red: 0xF7768E,
    accent: 0x7AA2F7,
    menu_bg: 0x15192D,
    send_bg: 0x7AA2F7,
    send_bg_hover: 0x94B5F9,
    send_fg: 0x0F111A,
    assistant_text: 0xC0CAF5,
    code_bg: 0x111428,
    code_text: 0xC0CAF5,
    syn_string: 0x9ECE6A,
    syn_number: 0xFF9E64,
    syn_function: 0xBB9AF7,
    syn_type: 0xE0AF68,
    syn_comment: 0x7A88CF,
    syn_literal: 0xFF9E64,
    syn_meta: 0xE0AF68,
    syn_operator: 0xFF9E64,
    tool_border: 0x2D2F43,
    tool_meta: 0x8890B3,
    ring_track: 0x1D233E,
    ring_fill: 0xC0CAF5,
    warn: 0xE0AF68,
    crit: 0xF7768E,
    trough: 0x131520,
};

/// One Dark Pro — ported from OpenChamber's `onedarkpro-dark`.
const ONE_DARK_PRO: Palette = Palette {
    bg_main: 0x1E222A,
    bg_sidebar: 0x212631,
    bg_raised: 0x282E39,
    bg_hover: 0x323640,
    active: 0x414652,
    active_fg: 0xF6F7FB,
    border: 0x313848,
    text: 0xACB3C0,
    text_2: 0x7D8392,
    text_3: 0x69707E,
    ok_green: 0x98C379,
    stop_red: 0xE06C75,
    stop_red_hover: 0xE6868E,
    add_green: 0x98C379,
    del_red: 0xE06C75,
    accent: 0x61AFEF,
    menu_bg: 0x282E39,
    send_bg: 0x61AFEF,
    send_bg_hover: 0x7ABCF2,
    send_fg: 0x1E222A,
    assistant_text: 0xACB3C0,
    code_bg: 0x212631,
    code_text: 0xABB2BF,
    syn_string: 0x98C379,
    syn_number: 0xE06C75,
    syn_function: 0x61AFEF,
    syn_type: 0xE5C07B,
    syn_comment: 0x818899,
    syn_literal: 0xE06C75,
    syn_meta: 0xE5C07B,
    syn_operator: 0xE06C75,
    tool_border: 0x313848,
    tool_meta: 0x737A89,
    ring_track: 0x333A48,
    ring_fill: 0xABB2BF,
    warn: 0xE5C07B,
    crit: 0xE06C75,
    trough: 0x222730,
};

/// One Dark Pro Max — ported from the Zed extension of the same name
/// (<https://github.com/bukitoka/one-dark-pro-max>): One Dark Pro's syntax
/// over a near-black `#080909` canvas with a flat, border-separated chrome.
/// The 2c313a family replaces OpenChamber's `element.*`, and the syntax hues
/// keep the upstream names (functions blue, keywords purple, strings green,
/// numbers/literals orange, types yellow, operators cyan) — with the UI
/// accent on the theme's blue and the purple carried by `syn_meta`.
const ONE_DARK_PRO_MAX: Palette = Palette {
    bg_main: 0x080909,
    bg_sidebar: 0x080909,
    bg_raised: 0x2C313A,
    bg_hover: 0x343B46,
    active: 0x3E4756,
    active_fg: 0xD7DAE0,
    border: 0x212121,
    text: 0xABB2BF,
    text_2: 0x7F838C,
    text_3: 0x636B78,
    ok_green: 0x98C379,
    stop_red: 0xE05561,
    stop_red_hover: 0xFF616E,
    add_green: 0x98C379,
    del_red: 0xE05561,
    accent: 0x61AFEF,
    menu_bg: 0x2C313A,
    send_bg: 0x61AFEF,
    send_bg_hover: 0x7CC0F5,
    send_fg: 0x080909,
    assistant_text: 0xABB2BF,
    code_bg: 0x0E1013,
    code_text: 0xABB2BF,
    syn_string: 0x98C379,
    syn_number: 0xD19A66,
    syn_function: 0x61AFEF,
    syn_type: 0xE5C07B,
    syn_comment: 0x7F838C,
    syn_literal: 0xD19A66,
    syn_meta: 0xC678DD,
    syn_operator: 0x56B6C2,
    tool_border: 0x212121,
    tool_meta: 0x7F838C,
    ring_track: 0x383F4A,
    ring_fill: 0xABB2BF,
    warn: 0xD19A66,
    crit: 0xE05561,
    trough: 0x0C0E10,
};

/// Monokai — ported from OpenChamber's `monokai-dark`.
const MONOKAI: Palette = Palette {
    bg_main: 0x23241E,
    bg_sidebar: 0x282A20,
    bg_raised: 0x323428,
    bg_hover: 0x3F4031,
    active: 0x475240,
    active_fg: 0xFFFFFF,
    border: 0x37382B,
    text: 0xF8F8F2,
    text_2: 0x939390,
    text_3: 0x71726E,
    ok_green: 0xA6E22E,
    stop_red: 0xF92672,
    stop_red_hover: 0xFA4D8B,
    add_green: 0xA6E22E,
    del_red: 0xF92672,
    accent: 0xAE81FF,
    menu_bg: 0x323428,
    send_bg: 0xAE81FF,
    send_bg_hover: 0xC09DFF,
    send_fg: 0x23241E,
    assistant_text: 0xF8F8F2,
    code_bg: 0x27281F,
    code_text: 0xF8F8F2,
    syn_string: 0xA6E22E,
    syn_number: 0xF92672,
    syn_function: 0xAE81FF,
    syn_type: 0xFD971F,
    syn_comment: 0xC5C5C0,
    syn_literal: 0xF92672,
    syn_meta: 0xFD971F,
    syn_operator: 0xF92672,
    tool_border: 0x37382B,
    tool_meta: 0x939390,
    ring_track: 0x404233,
    ring_fill: 0xF8F8F2,
    warn: 0xFD971F,
    crit: 0xF92672,
    trough: 0x282A23,
};

/// Solarized — ported from OpenChamber's `solarized-dark`.
const SOLARIZED_DARK: Palette = Palette {
    bg_main: 0x001E25,
    bg_sidebar: 0x02232E,
    bg_raised: 0x0A2B35,
    bg_hover: 0x16333B,
    active: 0x23454F,
    active_fg: 0xFDF6E3,
    border: 0x223A41,
    text: 0xA1ADAD,
    text_2: 0x797E7F,
    text_3: 0x5F6D71,
    ok_green: 0x859900,
    stop_red: 0xDC322F,
    stop_red_hover: 0xE25754,
    add_green: 0x859900,
    del_red: 0xDC322F,
    accent: 0x278BD2,
    menu_bg: 0x0A2B35,
    send_bg: 0x278BD2,
    send_bg_hover: 0x3B98DB,
    send_fg: 0x001F27,
    assistant_text: 0xA1ADAD,
    code_bg: 0x022733,
    code_text: 0x93A1A1,
    syn_string: 0x859900,
    syn_number: 0xD53E87,
    syn_function: 0x6C71C4,
    syn_type: 0xB58900,
    syn_comment: 0x6C7F80,
    syn_literal: 0xD53E87,
    syn_meta: 0xB58900,
    syn_operator: 0xD53E87,
    tool_border: 0x223A41,
    tool_meta: 0x6F7475,
    ring_track: 0x0E3C4A,
    ring_fill: 0x93A1A1,
    warn: 0xB58900,
    crit: 0xDC322F,
    trough: 0x00262F,
};

/// Ayu — ported from OpenChamber's `ayu-dark`.
const AYU_DARK: Palette = Palette {
    bg_main: 0x0F1419,
    bg_sidebar: 0x18222C,
    bg_raised: 0x202D3B,
    bg_hover: 0x273748,
    active: 0x2F4257,
    active_fg: 0xFBFBFD,
    border: 0x292C30,
    text: 0xD6DAE0,
    text_2: 0x777E86,
    text_3: 0x61676F,
    ok_green: 0x78D05C,
    stop_red: 0xF58572,
    stop_red_hover: 0xF79B8B,
    add_green: 0x78D05C,
    del_red: 0xF58572,
    accent: 0x3FB7E3,
    menu_bg: 0x202D3B,
    send_bg: 0x3FB7E3,
    send_bg_hover: 0x57C0E7,
    send_fg: 0x0F1419,
    assistant_text: 0xD6DAE0,
    code_bg: 0x18222C,
    code_text: 0xD6DAE0,
    syn_string: 0xB1C74A,
    syn_number: 0xF2856F,
    syn_function: 0x3FB7E3,
    syn_type: 0xE4A75C,
    syn_comment: 0xA3ADBA,
    syn_literal: 0xF2856F,
    syn_meta: 0xE4A75C,
    syn_operator: 0xF2856F,
    tool_border: 0x292C30,
    tool_meta: 0x777E86,
    ring_track: 0x293A4C,
    ring_fill: 0xD6DAE0,
    warn: 0xE4A75C,
    crit: 0xF58572,
    trough: 0x13191F,
};

/// Kanagawa — ported from OpenChamber's `kanagawa-dark`.
const KANAGAWA: Palette = Palette {
    bg_main: 0x1F1F28,
    bg_sidebar: 0x23232D,
    bg_raised: 0x2A2A37,
    bg_hover: 0x363646,
    active: 0x414D5C,
    active_fg: 0xDCD7BA,
    border: 0x333343,
    text: 0xDCD7BA,
    text_2: 0x8B8B85,
    text_3: 0x6F6F6D,
    ok_green: 0x98BB6C,
    stop_red: 0xE82424,
    stop_red_hover: 0xEC4B4B,
    add_green: 0x98BB6C,
    del_red: 0xE82424,
    accent: 0x7FB4CA,
    menu_bg: 0x2A2A37,
    send_bg: 0x7FB4CA,
    send_bg_hover: 0x93C0D2,
    send_fg: 0x1F1F28,
    assistant_text: 0xDCD7BA,
    code_bg: 0x16161D,
    code_text: 0xDCD7BA,
    syn_string: 0x98BB6C,
    syn_number: 0xFF9E3B,
    syn_function: 0xE6C384,
    syn_type: 0xC8C093,
    syn_comment: 0x61617E,
    syn_literal: 0xFF9E3B,
    syn_meta: 0xFF9E3B,
    syn_operator: 0xC34043,
    tool_border: 0x333343,
    tool_meta: 0x8B8B85,
    ring_track: 0x353545,
    ring_fill: 0xDCD7BA,
    warn: 0xFF9E3B,
    crit: 0xE82424,
    trough: 0x23232E,
};

/// Aura — ported from OpenChamber's `aura-dark`.
const AURA: Palette = Palette {
    bg_main: 0x15141B,
    bg_sidebar: 0x1A1921,
    bg_raised: 0x201E2B,
    bg_hover: 0x262835,
    active: 0x343749,
    active_fg: 0xFFFFFF,
    border: 0x2C2B37,
    text: 0xEDECEE,
    text_2: 0x8A8282,
    text_3: 0x6B6567,
    ok_green: 0x61FFCA,
    stop_red: 0xFF6767,
    stop_red_hover: 0xFF8282,
    add_green: 0x61FFCA,
    del_red: 0xFF6767,
    accent: 0xA277FF,
    menu_bg: 0x201E2B,
    send_bg: 0xA277FF,
    send_bg_hover: 0xB593FF,
    send_fg: 0x15141B,
    assistant_text: 0xEDECEE,
    code_bg: 0x1A1921,
    code_text: 0xEDECEE,
    syn_string: 0x61FFCA,
    syn_number: 0xFF6767,
    syn_function: 0xA277FF,
    syn_type: 0xFFCA85,
    syn_comment: 0x6D6D6D,
    syn_literal: 0xFF6767,
    syn_meta: 0xFFCA85,
    syn_operator: 0xFF6767,
    tool_border: 0x2C2B37,
    tool_meta: 0x8A8282,
    ring_track: 0x2B283A,
    ring_fill: 0xEDECEE,
    warn: 0xFFCA85,
    crit: 0xFF6767,
    trough: 0x1A1821,
};

/// Carbonfox — ported from OpenChamber's `carbonfox-dark`.
const CARBONFOX: Palette = Palette {
    bg_main: 0x161616,
    bg_sidebar: 0x222222,
    bg_raised: 0x2D2D2D,
    bg_hover: 0x373737,
    active: 0x434343,
    active_fg: 0xFFFFFF,
    border: 0x2E2D2D,
    text: 0xF2F4F8,
    text_2: 0x8B8A8A,
    text_3: 0x686767,
    ok_green: 0x42BE65,
    stop_red: 0xFF8389,
    stop_red_hover: 0xFF999E,
    add_green: 0x42BE65,
    del_red: 0xFF8389,
    accent: 0x33B1FF,
    menu_bg: 0x2D2D2D,
    send_bg: 0x33B1FF,
    send_bg_hover: 0x4FBCFF,
    send_fg: 0x161616,
    assistant_text: 0xF2F4F8,
    code_bg: 0x262626,
    code_text: 0xF2F4F8,
    syn_string: 0x42BE65,
    syn_number: 0xFF8389,
    syn_function: 0x78A9FF,
    syn_type: 0x08BDBA,
    syn_comment: 0x8D8D8D,
    syn_literal: 0xFF8389,
    syn_meta: 0xF1C21B,
    syn_operator: 0xFF8389,
    tool_border: 0x2E2D2D,
    tool_meta: 0x8B8A8A,
    ring_track: 0x3A3A3A,
    ring_fill: 0xF2F4F8,
    warn: 0xF1C21B,
    crit: 0xFF8389,
    trough: 0x1B1B1B,
};

/// Fields of the Shire — ported from OpenChamber's `fields-of-the-shire-dark`.
const FIELDS_OF_THE_SHIRE: Palette = Palette {
    bg_main: 0x1B1815,
    bg_sidebar: 0x23201C,
    bg_raised: 0x2B2622,
    bg_hover: 0x37302B,
    active: 0x47423C,
    active_fg: 0xEBE0D1,
    border: 0x35322F,
    text: 0xEBE0D1,
    text_2: 0x847A70,
    text_3: 0x71685F,
    ok_green: 0x7F905E,
    stop_red: 0xB34D3B,
    stop_red_hover: 0xC16D5E,
    add_green: 0x7F905E,
    del_red: 0xB34D3B,
    accent: 0x9CAF72,
    menu_bg: 0x2B2622,
    send_bg: 0x7A8A5A,
    send_bg_hover: 0x899B65,
    send_fg: 0x0C0A08,
    assistant_text: 0xEBE0D1,
    code_bg: 0x23201C,
    code_text: 0xEBE0D1,
    syn_string: 0x93A56B,
    syn_number: 0xC47A3A,
    syn_function: 0xC47A3A,
    syn_type: 0x9CAF72,
    syn_comment: 0x76685B,
    syn_literal: 0xC47A3A,
    syn_meta: 0xC47A3A,
    syn_operator: 0xA89888,
    tool_border: 0x35322F,
    tool_meta: 0x83796F,
    ring_track: 0x39332D,
    ring_fill: 0xEBE0D1,
    warn: 0xC47A3A,
    crit: 0xB34D3B,
    trough: 0x211D19,
};

/// Flexoki — ported from OpenChamber's `flexoki-dark`.
const FLEXOKI: Palette = Palette {
    bg_main: 0x171515,
    bg_sidebar: 0x1C1B1A,
    bg_raised: 0x252221,
    bg_hover: 0x2D2B2B,
    active: 0x3E3B3B,
    active_fg: 0xCECDC3,
    border: 0x2D2C2A,
    text: 0xCECDC3,
    text_2: 0x807E79,
    text_3: 0x696764,
    ok_green: 0xA0AF54,
    stop_red: 0xD14D41,
    stop_red_hover: 0xD96D63,
    add_green: 0xA0AF54,
    del_red: 0xD14D41,
    accent: 0xDA702C,
    menu_bg: 0x252221,
    send_bg: 0xDA702C,
    send_bg_hover: 0xDE8044,
    send_fg: 0x171515,
    assistant_text: 0xCECDC3,
    code_bg: 0x1C1B1A,
    code_text: 0xCECDC3,
    syn_string: 0x3AA99F,
    syn_number: 0x8B7EC8,
    syn_function: 0xDA702C,
    syn_type: 0xD0A215,
    syn_comment: 0x878580,
    syn_literal: 0x8B7EC8,
    syn_meta: 0xDA702C,
    syn_operator: 0xD14D41,
    tool_border: 0x2D2C2A,
    tool_meta: 0x807E79,
    ring_track: 0x322E2D,
    ring_fill: 0xCECDC3,
    warn: 0xDA702C,
    crit: 0xD14D41,
    trough: 0x1C1A1A,
};

/// JetBrains — ported from OpenChamber's `jetbrains-dark`.
const JETBRAINS: Palette = Palette {
    bg_main: 0x1E1F22,
    bg_sidebar: 0x26282B,
    bg_raised: 0x2B2D30,
    bg_hover: 0x3C3E41,
    active: 0x4B4D53,
    active_fg: 0xDFE1E5,
    border: 0x383A3F,
    text: 0xDFE1E5,
    text_2: 0x9FA0A2,
    text_3: 0x78797C,
    ok_green: 0x57965D,
    stop_red: 0xFA6675,
    stop_red_hover: 0xFB828E,
    add_green: 0x57965D,
    del_red: 0xFA6675,
    accent: 0x6796F5,
    menu_bg: 0x2B2D30,
    send_bg: 0x6796F5,
    send_bg_hover: 0x81A8F7,
    send_fg: 0x1E1F22,
    assistant_text: 0xDFE1E5,
    code_bg: 0x212226,
    code_text: 0xBCBEC4,
    syn_string: 0x6AAB73,
    syn_number: 0x2AACB8,
    syn_function: 0x6AA2D7,
    syn_type: 0xA6BB77,
    syn_comment: 0x7A7E85,
    syn_literal: 0x2AACB8,
    syn_meta: 0xF2C55C,
    syn_operator: 0xBCBEC4,
    tool_border: 0x383A3F,
    tool_meta: 0x9FA0A2,
    ring_track: 0x373A3D,
    ring_fill: 0xDFE1E5,
    warn: 0xF2C55C,
    crit: 0xFA6675,
    trough: 0x232427,
};

/// Mono — ported from OpenChamber's `mono-dark`.
const MONO: Palette = Palette {
    bg_main: 0x000000,
    bg_sidebar: 0x0A0A0A,
    bg_raised: 0x141414,
    bg_hover: 0x1F1F1F,
    active: 0x303030,
    active_fg: 0xE5E5E5,
    border: 0x2F2F2F,
    text: 0xE5E5E5,
    text_2: 0x808080,
    text_3: 0x5E5E5E,
    ok_green: 0xE5E5E5,
    stop_red: 0x666666,
    stop_red_hover: 0x828282,
    add_green: 0xE5E5E5,
    del_red: 0x666666,
    accent: 0xFFFFFF,
    menu_bg: 0x141414,
    send_bg: 0xFFFFFF,
    send_bg_hover: 0xF0F0F0,
    send_fg: 0x000000,
    assistant_text: 0xE5E5E5,
    code_bg: 0x0A0A0A,
    code_text: 0xE5E5E5,
    syn_string: 0xB3B3B3,
    syn_number: 0x999999,
    syn_function: 0xE5E5E5,
    syn_type: 0xD9D9D9,
    syn_comment: 0x666666,
    syn_literal: 0x999999,
    syn_meta: 0x999999,
    syn_operator: 0x808080,
    tool_border: 0x2F2F2F,
    tool_meta: 0x808080,
    ring_track: 0x212121,
    ring_fill: 0xE5E5E5,
    warn: 0x999999,
    crit: 0x666666,
    trough: 0x050505,
};

/// Mono Plus — ported from OpenChamber's `mono-plus-dark`.
const MONO_PLUS: Palette = Palette {
    bg_main: 0x000000,
    bg_sidebar: 0x0A0A0A,
    bg_raised: 0x141414,
    bg_hover: 0x1F1F1F,
    active: 0x303030,
    active_fg: 0xE5E5E5,
    border: 0x2F2F2F,
    text: 0xE5E5E5,
    text_2: 0x808080,
    text_3: 0x5E5E5E,
    ok_green: 0x6A8E6A,
    stop_red: 0x9E6A6A,
    stop_red_hover: 0xAF8585,
    add_green: 0x6A8E6A,
    del_red: 0x9E6A6A,
    accent: 0xA2BEE8,
    menu_bg: 0x141414,
    send_bg: 0xA2BEE8,
    send_bg_hover: 0xB8CEEE,
    send_fg: 0x000000,
    assistant_text: 0xE5E5E5,
    code_bg: 0x0A0A0A,
    code_text: 0xD4D4D4,
    syn_string: 0xB6D8E6,
    syn_number: 0xE3BDAD,
    syn_function: 0xA2BEE8,
    syn_type: 0xEDCBB8,
    syn_comment: 0x8C8C8C,
    syn_literal: 0xE3BDAD,
    syn_meta: 0x9E8A6A,
    syn_operator: 0xDCCDBE,
    tool_border: 0x2F2F2F,
    tool_meta: 0x808080,
    ring_track: 0x212121,
    ring_fill: 0xE5E5E5,
    warn: 0x9E8A6A,
    crit: 0x9E6A6A,
    trough: 0x050505,
};

/// Night Owl — ported from OpenChamber's `nightowl-dark`.
const NIGHT_OWL: Palette = Palette {
    bg_main: 0x011627,
    bg_sidebar: 0x0B253A,
    bg_raised: 0x133149,
    bg_hover: 0x173C59,
    active: 0x1C486B,
    active_fg: 0xFFFFFF,
    border: 0x2B3339,
    text: 0xD6DEEB,
    text_2: 0x7D8892,
    text_3: 0x5A6874,
    ok_green: 0xC5E478,
    stop_red: 0xEF5350,
    stop_red_hover: 0xF27270,
    add_green: 0xC5E478,
    del_red: 0xEF5350,
    accent: 0x82AAFF,
    menu_bg: 0x133149,
    send_bg: 0x82AAFF,
    send_bg_hover: 0x9EBDFF,
    send_fg: 0x011627,
    assistant_text: 0xD6DEEB,
    code_bg: 0x0B253A,
    code_text: 0xD6DEEB,
    syn_string: 0xECC48D,
    syn_number: 0xF78C6C,
    syn_function: 0x82AAFF,
    syn_type: 0xC5E478,
    syn_comment: 0x5F7E97,
    syn_literal: 0xF78C6C,
    syn_meta: 0xECC48D,
    syn_operator: 0xF78C6C,
    tool_border: 0x2B3339,
    tool_meta: 0x7D8892,
    ring_track: 0x183F5D,
    ring_fill: 0xD6DEEB,
    warn: 0xECC48D,
    crit: 0xEF5350,
    trough: 0x011C31,
};

/// OpenChamber — ported from OpenChamber's `openchamber-dark`.
const OPEN_CHAMBER: Palette = Palette {
    bg_main: 0x120F0E,
    bg_sidebar: 0x171615,
    bg_raised: 0x1F1D1B,
    bg_hover: 0x2A2625,
    active: 0x3A3736,
    active_fg: 0xC9C5BA,
    border: 0x2A2929,
    text: 0xC9C5BA,
    text_2: 0x8F8B81,
    text_3: 0x6A665E,
    ok_green: 0x76AD4F,
    stop_red: 0xDA5B4A,
    stop_red_hover: 0xE1796B,
    add_green: 0x76AD4F,
    del_red: 0xDA5B4A,
    accent: 0xDA7C47,
    menu_bg: 0x1F1D1B,
    send_bg: 0xDA7C47,
    send_bg_hover: 0xDF8D5E,
    send_fg: 0x000000,
    assistant_text: 0xC9C5BA,
    code_bg: 0x161211,
    code_text: 0xC9C5BA,
    syn_string: 0xD58373,
    syn_number: 0x279E93,
    syn_function: 0x78A952,
    syn_type: 0x479CB1,
    syn_comment: 0x728772,
    syn_literal: 0x279E93,
    syn_meta: 0xC67F13,
    syn_operator: 0xDA6B6D,
    tool_border: 0x2A2929,
    tool_meta: 0x8F8B81,
    ring_track: 0x2D2A27,
    ring_fill: 0xC9C5BA,
    warn: 0xC67F13,
    crit: 0xDA5B4A,
    trough: 0x181412,
};

/// Vesper — ported from OpenChamber's `vesper-dark`.
const VESPER: Palette = Palette {
    bg_main: 0x151515,
    bg_sidebar: 0x1C1B1B,
    bg_raised: 0x242121,
    bg_hover: 0x2F2F2F,
    active: 0x403F3F,
    active_fg: 0xFFFFFF,
    border: 0x373636,
    text: 0xE8E5E5,
    text_2: 0x848484,
    text_3: 0x676767,
    ok_green: 0x99FFE4,
    stop_red: 0xFF8080,
    stop_red_hover: 0xFF9797,
    add_green: 0x99FFE4,
    del_red: 0xFF8080,
    accent: 0xFFC799,
    menu_bg: 0x242121,
    send_bg: 0xFFC799,
    send_bg_hover: 0xFFD6B5,
    send_fg: 0x101010,
    assistant_text: 0xE8E5E5,
    code_bg: 0x141414,
    code_text: 0xFFFFFF,
    syn_string: 0x99FFE4,
    syn_number: 0xFF8080,
    syn_function: 0xFFC799,
    syn_type: 0xFFC799,
    syn_comment: 0xA0A0A0,
    syn_literal: 0xFF8080,
    syn_meta: 0xFFC799,
    syn_operator: 0xFF8080,
    tool_border: 0x373636,
    tool_meta: 0x848484,
    ring_track: 0x312D2D,
    ring_fill: 0xE8E5E5,
    warn: 0xFFC799,
    crit: 0xFF8080,
    trough: 0x1A1A1A,
};

/// Vitesse — ported from OpenChamber's `vitesse-dark-dark`.
const VITESSE: Palette = Palette {
    bg_main: 0x121212,
    bg_sidebar: 0x171717,
    bg_raised: 0x1F1F1F,
    bg_hover: 0x292929,
    active: 0x3A3A3A,
    active_fg: 0xDBD7CA,
    border: 0x313131,
    text: 0xDBD7CA,
    text_2: 0xC0BEB9,
    text_3: 0x8C8A87,
    ok_green: 0x4D9375,
    stop_red: 0xCB7676,
    stop_red_hover: 0xD48F8F,
    add_green: 0x4D9375,
    del_red: 0xCB7676,
    accent: 0x6C9BD1,
    menu_bg: 0x1F1F1F,
    send_bg: 0x6C9BD1,
    send_bg_hover: 0x82ABDA,
    send_fg: 0x10141A,
    assistant_text: 0xDBD7CA,
    code_bg: 0x161616,
    code_text: 0xDBD7CA,
    syn_string: 0xC98A7D,
    syn_number: 0x4C9A91,
    syn_function: 0x80A665,
    syn_type: 0x5D99A9,
    syn_comment: 0x758575,
    syn_literal: 0x4C9A91,
    syn_meta: 0xD4976C,
    syn_operator: 0xCB7676,
    tool_border: 0x313131,
    tool_meta: 0xC0BEB9,
    ring_track: 0x2C2C2C,
    ring_fill: 0xDBD7CA,
    warn: 0xD4976C,
    crit: 0xCB7676,
    trough: 0x171717,
};

fn palette(id: ThemeId) -> Palette {
    match id {
        ThemeId::Orbit => ORBIT,
        ThemeId::OrbitLight => ORBIT_LIGHT,
        ThemeId::Vague => VAGUE,
        ThemeId::Batsignal => BATSIGNAL,
        ThemeId::Ashwood => ASHWOOD,
        ThemeId::Obsidian => OBSIDIAN,
        ThemeId::MatteBlack => MATTE_BLACK,
        ThemeId::AdwaitaPastel => ADWAITA_PASTEL,
        ThemeId::Ashen => ASHEN,
        ThemeId::Discord => DISCORD,
        ThemeId::Catppuccin => CATPPUCCIN,
        ThemeId::Dracula => DRACULA,
        ThemeId::Gruvbox => GRUVBOX,
        ThemeId::Nord => NORD,
        ThemeId::TokyoNight => TOKYO_NIGHT,
        ThemeId::OneDarkPro => ONE_DARK_PRO,
        ThemeId::OneDarkProMax => ONE_DARK_PRO_MAX,
        ThemeId::Monokai => MONOKAI,
        ThemeId::SolarizedDark => SOLARIZED_DARK,
        ThemeId::AyuDark => AYU_DARK,
        ThemeId::Kanagawa => KANAGAWA,
        ThemeId::Aura => AURA,
        ThemeId::Carbonfox => CARBONFOX,
        ThemeId::FieldsOfTheShire => FIELDS_OF_THE_SHIRE,
        ThemeId::Flexoki => FLEXOKI,
        ThemeId::JetBrains => JETBRAINS,
        ThemeId::Mono => MONO,
        ThemeId::MonoPlus => MONO_PLUS,
        ThemeId::NightOwl => NIGHT_OWL,
        ThemeId::OpenChamber => OPEN_CHAMBER,
        ThemeId::Vesper => VESPER,
        ThemeId::Vitesse => VITESSE,
    }
}

impl Theme {
    pub fn for_id(id: ThemeId) -> Self {
        Self::build(id.appearance(), UiPrefs::default(), id, palette(id))
    }

    #[cfg(test)]
    pub fn dark() -> Self {
        Self::for_id(ThemeId::Orbit)
    }

    #[cfg(test)]
    pub fn light() -> Self {
        Self::for_id(ThemeId::OrbitLight)
    }

    /// Keep the current UI customization across a palette switch.
    pub fn with_ui(mut self, ui: UiPrefs) -> Self {
        self.ui = ui;
        self
    }

    /// Scale an interface text size by the UI font size setting, the way
    /// Waku's `sp()` rems resolve against its UI font size (`ui_font_size /
    /// DEFAULT_UI_FONT_SIZE`). Chrome sizes are authored at the 14px default,
    /// so at the default setting this resolves to the authored pixel value.
    pub fn ui_px(&self, value: f32) -> Pixels {
        px(value * (self.ui.ui_font_size / DEFAULT_UI_FONT_SIZE))
    }

    /// Scale an editor-surface size (code blocks, diffs) by the Editor font
    /// size setting (`editor_font_size / 13`, the Waku default).
    pub fn code_px(&self, value: f32) -> Pixels {
        px(value * (self.ui.editor_font_size / 13.))
    }

    /// Scale a terminal/tool-output size by the Terminal font size setting
    /// (`terminal_font_size / 13`, the Waku default).
    pub fn term_px(&self, value: f32) -> Pixels {
        px(value * (self.ui.terminal_font_size / 13.))
    }

    /// Scale a spacing value by the Spacing Density setting
    /// (`spacing_density / 100`).
    pub fn space(&self, value: f32) -> Pixels {
        px(value * (self.ui.spacing_density as f32 / 100.))
    }

    /// Paint color for one syntax token, shared by transcript code blocks
    /// and the review diff so both read as the same editor. Each palette
    /// carries its own `syn_*` colors (so a ported editor theme keeps its
    /// syntax identity); comments are explicitly legible on the code wash.
    pub fn token_color(self, class: TokenClass) -> Hsla {
        match class {
            TokenClass::Keyword => self.accent,
            TokenClass::Literal => self.syn_literal,
            TokenClass::String => self.syn_string,
            TokenClass::Comment => self.syn_comment,
            TokenClass::Number => self.syn_number,
            TokenClass::Type => self.syn_type,
            TokenClass::Function => self.syn_function,
            TokenClass::Meta => self.syn_meta,
            // Terminal vocabulary: command, flag, path, control operator.
            TokenClass::Command => self.accent,
            TokenClass::Flag => self.syn_number,
            TokenClass::Path => self.text,
            TokenClass::Operator => self.syn_operator,
            TokenClass::Added => self.add_green,
            TokenClass::Removed => self.del_red,
        }
    }

    /// Paint color for a `/command` token in the composer. Commands are
    /// actions, so they take the accent — the role terminal command words
    /// and syntax keywords already carry.
    pub fn mention_command(self) -> Hsla {
        self.accent
    }

    /// Paint color for an `@file` mention in the composer. Files take the
    /// accent's complement, so an `@reference` never reads as a `/command`
    /// while still riding the palette's tuned chroma and lightness. Palettes
    /// with no chroma (Ashwood, Mono) stay monochrome and separate by ink.
    pub fn mention_file(self) -> Hsla {
        if self.accent.s < 0.15 {
            return self.accent.opacity(0.7);
        }
        let mut color = self.accent;
        color.h = (color.h + 0.5).fract();
        color.s = color.s.max(0.5);
        color
    }

    /// Build a full [`Theme`] from its appearance + raw palette tokens.
    fn build(mode: ThemeMode, ui: UiPrefs, id: ThemeId, p: Palette) -> Self {
        let text = hex(p.text);
        let accent = hex(p.accent);
        let (wash, wash_strong, border_strong, contact, ambient) = match mode {
            ThemeMode::Dark => (0.05, 0.09, 0.14, 0.32, 0.40),
            ThemeMode::Light => (0.06, 0.10, 0.16, 0.10, 0.14),
        };
        Self {
            theme_id: id,
            mode,
            ui,
            bg_main: hex(p.bg_main),
            bg_sidebar: hex(p.bg_sidebar),
            bg_composer: hex(p.bg_sidebar),
            bg_raised: hex(p.bg_raised),
            bg_hover: hex(p.bg_hover),
            active: hex(p.active),
            active_fg: hex(p.active_fg),
            border: hex(p.border),
            text,
            text_2: hex(p.text_2),
            text_3: hex(p.text_3),
            ok_green: hex(p.ok_green),
            stop_red: hex(p.stop_red),
            stop_red_hover: hex(p.stop_red_hover),
            add_green: hex(p.add_green),
            del_red: hex(p.del_red),
            accent,
            menu_bg: hex(p.menu_bg),
            send_bg: hex(p.send_bg),
            send_bg_hover: hex(p.send_bg_hover),
            send_fg: hex(p.send_fg),
            overlay: text.opacity(wash),
            overlay_strong: text.opacity(wash_strong),
            border_strong: text.opacity(border_strong),
            assistant_text: hex(p.assistant_text),
            code_bg: hex(p.code_bg),
            code_text: hex(p.code_text),
            syn_string: hex(p.syn_string),
            syn_number: hex(p.syn_number),
            syn_function: hex(p.syn_function),
            syn_type: hex(p.syn_type),
            syn_comment: hex(p.syn_comment),
            syn_literal: hex(p.syn_literal),
            syn_meta: hex(p.syn_meta),
            syn_operator: hex(p.syn_operator),
            inline_code_bg: accent.opacity(0.10),
            tool_border: hex(p.tool_border),
            tool_meta: hex(p.tool_meta),
            ring_track: hex(p.ring_track),
            ring_fill: hex(p.ring_fill),
            warn: hex(p.warn),
            crit: hex(p.crit),
            trough: hex(p.trough),
            shadow_contact: hsla(0., 0., 0., contact),
            shadow_ambient: hsla(0., 0., 0., ambient),
        }
    }

    /// Layered drop shadow (tight contact + wide ambient), like Zed's
    /// `ElevationIndex::ModalSurface`.
    pub fn popover_shadow(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.shadow_contact,
                offset: point(px(0.), px(4.)),
                blur_radius: px(12.),
                spread_radius: px(-2.),
            },
            BoxShadow {
                color: self.shadow_ambient,
                offset: point(px(0.), px(12.)),
                blur_radius: px(32.),
                spread_radius: px(-8.),
            },
        ]
    }

    /// Soft lift for the floating composer: one tight contact layer. The
    /// hairline border carries definition, so no wide ambient here (a
    /// border under a broad shadow reads as a ghost card).
    pub fn composer_shadow(self) -> Vec<BoxShadow> {
        vec![BoxShadow {
            color: self.shadow_contact,
            offset: point(px(0.), px(2.)),
            blur_radius: px(10.),
            spread_radius: px(-3.),
        }]
    }

    /// Compact hover-card shadow (slightly tighter than the picker).
    pub fn card_shadow(self) -> Vec<BoxShadow> {
        vec![
            BoxShadow {
                color: self.shadow_contact,
                offset: point(px(0.), px(4.)),
                blur_radius: px(12.),
                spread_radius: px(-2.),
            },
            BoxShadow {
                color: self.shadow_ambient,
                offset: point(px(0.), px(8.)),
                blur_radius: px(24.),
                spread_radius: px(-6.),
            },
        ]
    }
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

fn persist_path() -> PathBuf {
    crate::platform::home_dir().join(".orbit-pi").join("theme")
}

/// Install the persisted appearance (System by default) and font preferences.
pub fn init(cx: &mut App) {
    let prefs = AppearancePrefs::load();
    cx.set_global(prefs);
    cx.set_global(Theme::for_id(prefs.resolve(cx.window_appearance())).with_ui(UiPrefs::load()));
    let prefs = FontPrefs::load();
    *UI_FONT_FAMILY.write().unwrap() = prefs.ui_font_family;
    *CODE_FONT_FAMILY.write().unwrap() = prefs.code_font_family;
}

/// Current theme. Panics if [`init`] has not run.
pub fn get(cx: &App) -> &Theme {
    cx.global::<Theme>()
}

/// Whether the user asked to reduce motion. Looping animations should render
/// static when this is true.
pub fn reduce_motion(cx: &App) -> bool {
    get(cx).ui.reduce_motion
}

/// Apply new UI / code font families to the statics and persist them.
pub fn set_font_prefs(prefs: FontPrefs) {
    *UI_FONT_FAMILY.write().unwrap() = prefs.ui_font_family.clone();
    *CODE_FONT_FAMILY.write().unwrap() = prefs.code_font_family.clone();
    prefs.persist();
}

/// Apply new UI customization (language / font sizes), persist it, and
/// notify global observers — every paint site reads sizes through the
/// theme, so the change is live.
pub fn set_ui_prefs(cx: &mut App, ui: UiPrefs) {
    if get(cx).ui == ui {
        return;
    }
    // Adopt the new locale before the theme global changes so the repaint
    // driven by that change already reads the right strings, then rebuild
    // the native menu bar (which only GPUI can refresh).
    crate::i18n::set_language(ui.language);
    crate::set_app_menus(cx);
    ui.persist();
    let id = get(cx).theme_id;
    cx.set_global(Theme::for_id(id).with_ui(ui));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_theme_id() {
        assert_eq!(ThemeId::parse("orbit"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("vague"), Some(ThemeId::Vague));
        assert_eq!(ThemeId::parse("orbit-light"), Some(ThemeId::OrbitLight));
        assert_eq!(ThemeId::parse("system"), None);
        assert_eq!(ThemeId::Vague.as_str(), "vague");
        assert_eq!(ThemeId::Vague.appearance(), ThemeMode::Dark);
        // Legacy theme names map onto Orbit / Orbit Light.
        assert_eq!(ThemeId::parse("dark"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("light"), Some(ThemeId::OrbitLight));
        assert_eq!(ThemeId::parse("one-dark"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("one-light"), Some(ThemeId::OrbitLight));
        assert_eq!(ThemeId::parse("zedokai"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("flexoki-dark"), Some(ThemeId::Orbit));
        assert_eq!(ThemeId::parse("flexoki-light"), Some(ThemeId::OrbitLight));
        // Ported Zed themes + their aliases.
        assert_eq!(ThemeId::parse("batsignal-dark"), Some(ThemeId::Batsignal));
        assert_eq!(ThemeId::parse("batsignal"), Some(ThemeId::Batsignal));
        assert_eq!(ThemeId::parse("ashwood"), Some(ThemeId::Ashwood));
        assert_eq!(ThemeId::parse("obsidian-dark"), Some(ThemeId::Obsidian));
        assert_eq!(ThemeId::parse("obsidian"), Some(ThemeId::Obsidian));
        assert_eq!(ThemeId::parse("matte-black"), Some(ThemeId::MatteBlack));
        assert_eq!(
            ThemeId::parse("matte-black-theme"),
            Some(ThemeId::MatteBlack)
        );
        assert_eq!(
            ThemeId::parse("adwaita-pastel-dark"),
            Some(ThemeId::AdwaitaPastel)
        );
        assert_eq!(ThemeId::parse("ashen"), Some(ThemeId::Ashen));
        assert_eq!(ThemeId::parse("discord-dark"), Some(ThemeId::Discord));
        // Every selectable theme round-trips through its persisted key.
        for id in ThemeId::ALL {
            assert_eq!(ThemeId::parse(id.as_str()), Some(id));
        }
    }

    /// Composer mention tokens stay legible and distinct from each other in
    /// every palette, including the monochrome ones (Ashwood, Mono) where the
    /// split is by ink rather than hue.
    #[test]
    fn mention_token_colors_are_distinct_across_palettes() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let command = theme.mention_command();
            let file = theme.mention_file();
            assert!(
                command != file,
                "mention colors collide in {id:?}: {command:?}"
            );
            assert!(command.a > 0., "command token is transparent in {id:?}");
            assert!(file.a > 0., "file token is transparent in {id:?}");
        }
    }

    #[test]
    fn ported_zed_themes_are_dark_and_labelled() {
        let ported = [
            ThemeId::Batsignal,
            ThemeId::Ashwood,
            ThemeId::Obsidian,
            ThemeId::MatteBlack,
            ThemeId::AdwaitaPastel,
            ThemeId::Ashen,
            ThemeId::Discord,
        ];
        for id in ported {
            assert_eq!(id.appearance(), ThemeMode::Dark);
            assert_eq!(Theme::for_id(id).mode, ThemeMode::Dark);
            assert!(!id.label().is_empty());
        }
        assert_eq!(ThemeId::Batsignal.label(), "Batsignal (Dark)");
        assert_eq!(ThemeId::Obsidian.label(), "Obsidian Dark");
        assert_eq!(ThemeId::AdwaitaPastel.label(), "Adwaita Pastel Dark");
        assert_eq!(ThemeId::Discord.label(), "Discord Dark");
    }

    /// The full curated catalog (Waku's list) is selectable under its names.
    #[test]
    fn full_curated_theme_catalog_is_present() {
        let labels: Vec<&str> = ThemeId::ALL.iter().map(|id| id.label()).collect();
        for wanted in [
            "Aura",
            "Ayu",
            "Carbonfox",
            "Catppuccin",
            "Dracula",
            "Fields of the Shire",
            "Flexoki",
            "Gruvbox",
            "JetBrains",
            "Kanagawa",
            "Mono",
            "Mono Plus",
            "Monokai",
            "Night Owl",
            "Nord",
            "One Dark Pro",
            "One Dark Pro Max",
            "OpenChamber",
            "Solarized",
            "Tokyonight",
            "Vesper",
            "Vitesse",
        ] {
            assert!(labels.contains(&wanted), "missing theme {wanted}");
        }
    }

    fn rel_lum(v: u32) -> f64 {
        let f = |c: u32| {
            let c = c as f64 / 255.;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f((v >> 16) & 0xff) + 0.7152 * f((v >> 8) & 0xff) + 0.0722 * f(v & 0xff)
    }

    fn contrast(a: u32, b: u32) -> f64 {
        let (l1, l2) = (rel_lum(a), rel_lum(b));
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    /// Every palette must keep its foreground readable against the surfaces
    /// it is painted on. The bars are the delivered floor: ink clears AA on
    /// the canvas, the accent is legible as text (links, inline code) rather
    /// than only as a wash, and every state pair (active row, send button,
    /// focus accent) holds at 4.5:1.
    #[test]
    fn every_theme_has_readable_foregrounds() {
        for id in ThemeId::ALL {
            let p = palette(id);
            let c = |a, b| contrast(a, b);
            assert!(c(p.text, p.bg_main) >= 7.0, "{id:?}: body text on canvas");
            assert!(c(p.text, p.bg_raised) >= 4.5, "{id:?}: body text on raised");
            assert!(c(p.text_2, p.bg_main) >= 4.0, "{id:?}: secondary text");
            assert!(c(p.text_3, p.bg_main) >= 3.0, "{id:?}: tertiary/label text");
            assert!(c(p.accent, p.bg_main) >= 4.5, "{id:?}: accent as text");
            assert!(c(p.active_fg, p.active) >= 4.5, "{id:?}: active row");
            assert!(c(p.send_fg, p.send_bg) >= 4.5, "{id:?}: send button");
            assert!(c(p.border, p.bg_main) >= 1.15, "{id:?}: hairline vs canvas");
            // Syntax must stay legible on the code wash, comments included.
            for (label, token) in [
                ("string", p.syn_string),
                ("number", p.syn_number),
                ("function", p.syn_function),
                ("type", p.syn_type),
                ("literal", p.syn_literal),
                ("meta", p.syn_meta),
                ("operator", p.syn_operator),
            ] {
                assert!(
                    c(token, p.code_bg) >= 3.5,
                    "{id:?}: syntax {label} on code wash"
                );
            }
            assert!(
                c(p.syn_comment, p.code_bg) >= 3.0,
                "{id:?}: comment on code wash"
            );
        }
    }

    /// One surface grammar for all thirty-one palettes: on dark, each step up
    /// the ramp is lighter than the one below (canvas → raised → hover →
    /// active); on light it is the inverse. Menu surfaces float at `raised`,
    /// and the send button always answers a hover with a visible step.
    #[test]
    fn every_theme_holds_its_surface_grammar() {
        for id in ThemeId::ALL {
            let p = palette(id);
            let l = |v: u32| rel_lum(v);
            match id.appearance() {
                ThemeMode::Dark => {
                    assert!(
                        l(p.bg_raised) > l(p.bg_main),
                        "{id:?}: raised must lift off the canvas"
                    );
                    assert!(
                        l(p.bg_raised) >= l(p.bg_sidebar),
                        "{id:?}: raised must float above the chrome plane"
                    );
                    assert!(
                        l(p.bg_hover) >= l(p.bg_raised),
                        "{id:?}: hover must sit above raised"
                    );
                    assert!(
                        l(p.active) >= l(p.bg_hover),
                        "{id:?}: active must sit above hover"
                    );
                }
                ThemeMode::Light => {
                    assert!(
                        l(p.bg_raised) < l(p.bg_main),
                        "{id:?}: raised must recess in light mode"
                    );
                    assert!(
                        l(p.bg_raised) <= l(p.bg_sidebar),
                        "{id:?}: raised must sit below the light chrome plane"
                    );
                    assert!(
                        l(p.bg_hover) <= l(p.bg_raised),
                        "{id:?}: hover must recess below raised"
                    );
                    assert!(
                        l(p.active) <= l(p.bg_hover),
                        "{id:?}: active must recess below hover"
                    );
                }
            }
            assert_eq!(p.menu_bg, p.bg_raised, "{id:?}: menus float at raised");
            assert_ne!(p.send_bg, p.send_bg_hover, "{id:?}: send hover feedback");
            // The accent is the lone interactive hue; it can never double as
            // a status color or a second accent would read as state.
            assert_ne!(p.accent, p.ok_green, "{id:?}: accent collides with success");
            assert_ne!(p.accent, p.add_green, "{id:?}: accent collides with added");
            assert_ne!(p.accent, p.stop_red, "{id:?}: accent collides with stop");
            assert_ne!(p.accent, p.crit, "{id:?}: accent collides with critical");
        }
    }

    /// Only Ashwood and Mono are authored chroma-less; every other palette
    /// must carry a real accent hue (the composer derives `@file` by
    /// inverting it), so a grey accent there is a defect.
    #[test]
    fn accent_carries_chroma_where_expected() {
        for id in ThemeId::ALL {
            if matches!(id, ThemeId::Ashwood | ThemeId::Mono) {
                continue;
            }
            let accent = Theme::for_id(id).accent;
            assert!(
                accent.s > 0.05,
                "{id:?}: accent is nearly achromatic (s = {:.3})",
                accent.s
            );
        }
    }

    #[test]
    fn theme_appearance_matches_label() {
        assert_eq!(ThemeId::Orbit.appearance(), ThemeMode::Dark);
        assert_eq!(ThemeId::OrbitLight.appearance(), ThemeMode::Light);
        assert_eq!(ThemeId::Orbit.label(), "Orbit");
        assert_eq!(ThemeId::OrbitLight.label(), "Orbit Light");
    }

    #[test]
    fn parse_language() {
        assert_eq!(Language::parse("system"), Some(Language::System));
        assert_eq!(Language::parse("en"), Some(Language::English));
        assert_eq!(Language::parse("zh-CN"), Some(Language::SimplifiedChinese));
        assert_eq!(Language::parse("fr"), Some(Language::French));
        assert_eq!(Language::parse("xx"), None);
        assert_eq!(Language::English.label(), "English");
    }

    #[test]
    fn ui_prefs_parse_rejects_out_of_range_sizes() {
        use serde_json::json;
        let prefs = UiPrefs::from_value(&json!({
            "language": "en",
            "ui_font_size": 500,
            "terminal_font_size": 15,
            "editor_font_size": 12,
            "spacing_density": 100,
        }));
        // 500px isn't selectable — falls back to the default; the rest apply.
        assert_eq!(prefs.language, Language::English);
        assert_eq!(prefs.ui_font_size, 14.);
        assert_eq!(prefs.terminal_font_size, 15.);
        assert_eq!(prefs.editor_font_size, 12.);
        assert_eq!(prefs.spacing_density, 100);
        // Malformed payload → all defaults.
        assert_eq!(
            UiPrefs::from_value(&json!({"language": 3})),
            UiPrefs::default()
        );
        // Legacy percentage / px keys migrate onto the px scales.
        let legacy = UiPrefs::from_value(&json!({
            "interface_scale": 115,
            "code_font_size": 15,
        }));
        assert_eq!(legacy.ui_font_size, 16.);
        assert_eq!(legacy.editor_font_size, 15.);
    }

    #[test]
    fn font_scale_helpers() {
        fn close(a: Pixels, b: Pixels) -> bool {
            (f32::from(a) - f32::from(b)).abs() < 0.01
        }
        let mut theme = Theme::dark();
        // Defaults are the Waku values — no scaling.
        assert_eq!(theme.ui_px(14.), px(14.));
        assert_eq!(theme.code_px(13.), px(13.));
        assert_eq!(theme.term_px(13.), px(13.));
        assert_eq!(theme.space(12.), px(12.));
        theme.ui.ui_font_size = 18.;
        theme.ui.editor_font_size = 11.;
        theme.ui.terminal_font_size = 15.;
        theme.ui.spacing_density = 75;
        assert_eq!(theme.ui_px(14.), px(18.));
        assert!(close(theme.code_px(13.), px(11.)));
        assert!(close(theme.term_px(13.), px(15.)));
        assert_eq!(theme.space(10.), px(7.5));
        // A palette switch carries UI customization over.
        let light = Theme::for_id(ThemeId::OrbitLight).with_ui(theme.ui);
        assert_eq!(light.theme_id, ThemeId::OrbitLight);
        assert_eq!(light.ui, theme.ui);
    }

    #[test]
    fn palettes_differ() {
        let dark = Theme::dark();
        let light = Theme::light();
        assert_ne!(dark.bg_main, light.bg_main);
        assert_ne!(dark.text, light.text);
        assert_eq!(dark.theme_id, ThemeId::Orbit);
        assert_eq!(light.theme_id, ThemeId::OrbitLight);
        // Every palette is distinct from the default.
        for id in ThemeId::ALL {
            assert_eq!(Theme::for_id(id).theme_id, id);
        }
        // Orbit is dark; Orbit Light is light.
        for id in ThemeId::ALL {
            assert_eq!(
                Theme::for_id(id).mode,
                if id == ThemeId::OrbitLight {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                }
            );
        }
    }

    #[test]
    fn vague_keeps_its_own_syntax_palette() {
        let vague = Theme::for_id(ThemeId::Vague);
        let orbit = Theme::dark();
        assert_eq!(vague.mode, ThemeMode::Dark);
        // Vague's syntax is its own identity — warm strings, muted-blue
        // keywords — not Orbit's semantic green/amber reuse.
        assert_ne!(
            vague.token_color(TokenClass::String),
            orbit.token_color(TokenClass::String)
        );
        assert_eq!(vague.token_color(TokenClass::String), hex(0xE8B589));
        assert_eq!(vague.token_color(TokenClass::Keyword), hex(0x6E94B2));
        assert_eq!(vague.token_color(TokenClass::Function), hex(0xC48282));
        assert_eq!(vague.token_color(TokenClass::Type), hex(0x9BB4BC));
    }
}
