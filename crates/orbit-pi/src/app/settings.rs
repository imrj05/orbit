use super::format::*;
use super::*;
use crate::ui::button::{Button, ButtonVariant};
use crate::git_panel::widgets::check_box;
use crate::motion::Hover;
use crate::quota::note_should_render;
use crate::theme::tokens::{
    button, context_menu, input, modal, panel, picker, popover, BufferLineHeight, ButtonSize,
    DynamicSpacing, HeadlineSize, IconSize, Radius, TextSize, BORDER_WIDTH,
};
use crate::theme::MotionPreference;
use crate::ui::button::{ButtonGroup, SegmentFill};
use crate::ui::chip::{Badge, BadgeFill, ChipButton, ChipStyle};
use crate::ui::feedback::{progress_bar, ProgressSize, ProgressTrack};
use crate::ui::field::search_field;
use crate::ui::field::TextField;
use crate::ui::popover::{popover_picker, PopoverExt};
use crate::ui::prelude::*;
use crate::ui::banner::{Banner, BannerTone};
use crate::ui::style::surface::SurfaceExt;
use gpui::{canvas, Bounds, DispatchPhase, Div, Hitbox, HitboxBehavior, MouseMoveEvent, Stateful};

/// The pinned GPUI release (`crates/orbit-pi/Cargo.toml`), shown on About.
const GPUI_VERSION: &str = "0.2.2";

// Font registration completes at startup, before settings can render. Cache the
// snapshot for this process rather than enumerating platform fonts each frame.
fn font_family_options(cx: &App, current: &str) -> Vec<String> {
    static AVAILABLE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    font_options_with_current(
        AVAILABLE.get_or_init(|| cx.text_system().all_font_names()),
        current,
    )
}

fn font_options_with_current(available: &[String], current: &str) -> Vec<String> {
    let mut options = available.to_vec();
    if !current.is_empty() && !options.iter().any(|family| family == current) {
        options.push(current.to_owned());
    }
    options.sort();
    options.dedup();
    options
}

/// A Plugins-page button action, dispatched through one entry point.
#[derive(Clone)]
pub(super) enum PluginAction {
    Install,
    Update { source: String },
    /// Ask through the confirm prompt, then remove the package.
    Remove {
        source: String,
        project: bool,
        name: String,
    },
    /// Install a package that is configured but missing from disk.
    InstallOne { source: String, project: bool },
    /// Flip a package's `autoload` state so it stops loading (or loads
    /// again) on pi's next start or `/reload`.
    ToggleEnabled {
        source: String,
        project: bool,
        enabled: bool,
    },
    SetScope { project: bool },
    Refresh,
    /// Restart pi so a package change loads now.
    RestartPi,
}

/// Toolbar vs. card button geometry. Settings' pinned toolbars size their
/// controls to `ButtonSize::Large`, the row Providers and Models use; card
/// actions stay compact at `ButtonSize::Medium`.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum PluginButtonSize {
    Compact,
    Toolbar,
}

/// How long the Plugins Refresh button turns after a click, so a synchronous
/// reload still reads as a deliberate action.
const PLUGIN_REFRESH_FEEDBACK: Duration = Duration::from_millis(650);

/// The settings select chip's size. Its dropdown is offset by this height
/// plus `popover::MENU_OFFSET`, so the two must move together.
const SELECT_CHIP_SIZE: ButtonSize = ButtonSize::Large;

/// Wallpaper slider geometry: the painted track width, rail height, handle
/// diameter, and the row they are centered in. The drag math reads the same
/// width/handle pair, so the handle always lands under the pointer rather
/// than a few pixels off it.
const SLIDER_TRACK_W: f32 = 184.;
const SLIDER_TRACK_H: f32 = 6.;
const SLIDER_KNOB: f32 = 16.;
const SLIDER_ROW_H: f32 = 24.;

/// The continuous wallpaper controls. They share one painted slider and one
/// pointer handler; only the range and the field they write differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WallpaperSlider {
    ImageOpacity,
    Blur,
    CardOpacity,
    EffectIntensity,
    EffectScale,
}

impl WallpaperSlider {
    fn range(self) -> (f32, f32) {
        match self {
            Self::ImageOpacity => crate::wallpaper::IMAGE_OPACITY_RANGE,
            Self::Blur => crate::wallpaper::BLUR_RANGE,
            Self::CardOpacity => crate::wallpaper::CARD_OPACITY_RANGE,
            Self::EffectIntensity => crate::wallpaper::EFFECT_INTENSITY_RANGE,
            Self::EffectScale => crate::wallpaper::EFFECT_SCALE_RANGE,
        }
    }

    fn value(self, settings: &crate::wallpaper::WallpaperSettings) -> f32 {
        match self {
            Self::ImageOpacity => settings.image_opacity,
            Self::Blur => settings.blur,
            Self::CardOpacity => settings.card_opacity,
            Self::EffectIntensity => settings.effect_intensity,
            Self::EffectScale => settings.effect_scale,
        }
    }

    fn set(self, settings: &mut crate::wallpaper::WallpaperSettings, value: f32) {
        match self {
            Self::ImageOpacity => settings.image_opacity = value,
            Self::Blur => settings.blur = value,
            Self::CardOpacity => settings.card_opacity = value,
            Self::EffectIntensity => settings.effect_intensity = value,
            Self::EffectScale => {
                settings.effect_scale = if settings.effect == crate::wallpaper_fx::Effect::Dither {
                    value.round().max(1.0)
                } else {
                    value
                };
            }
        }
    }

    fn ids(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::ImageOpacity => (
                "wallpaper-opacity-slider",
                "wallpaper-opacity-fill",
                "wallpaper-opacity-knob",
            ),
            Self::Blur => (
                "wallpaper-blur-slider",
                "wallpaper-blur-fill",
                "wallpaper-blur-knob",
            ),
            Self::CardOpacity => (
                "wallpaper-card-slider",
                "wallpaper-card-fill",
                "wallpaper-card-knob",
            ),
            Self::EffectIntensity => (
                "wallpaper-intensity-slider",
                "wallpaper-intensity-fill",
                "wallpaper-intensity-knob",
            ),
            Self::EffectScale => (
                "wallpaper-scale-slider",
                "wallpaper-scale-fill",
                "wallpaper-scale-knob",
            ),
        }
    }
}

/// The settings nav in display order: section, nav icon, and label key.
/// Grouped so the app's own preferences lead (General / Appearance /
/// Shortcuts), the agent and its resources follow (Runtime / Agent /
/// Providers / Models), the extensions pi loads close the list (Skills /
/// Plugins / MCP), and About and Report a bug sit alone at the end.
const SETTINGS_SECTIONS: &[(SettingsSection, &str, &str)] = &[
    (
        SettingsSection::General,
        "icons/settings.svg",
        "settings.general",
    ),
    (
        SettingsSection::Appearance,
        "icons/contrast.svg",
        "settings.appearance",
    ),
    (
        SettingsSection::Shortcuts,
        "icons/keyboard.svg",
        "settings.shortcuts",
    ),
    (
        SettingsSection::Privacy,
        "icons/lock.svg",
        "settings.privacy",
    ),
    (
        SettingsSection::Runtime,
        "icons/server-stack.svg",
        "settings.runtime",
    ),
    (SettingsSection::Agent, "icons/spark.svg", "settings.agent"),
    (
        SettingsSection::Worktrees,
        "icons/worktree.svg",
        "worktree.settings.title",
    ),
    (
        SettingsSection::GitAccounts,
        "icons/branch.svg",
        "git_accounts.title",
    ),
    (
        SettingsSection::Providers,
        "icons/cloud.svg",
        "settings.providers_and_models",
    ),
    (
        SettingsSection::Skills,
        "icons/magic-wand.svg",
        "settings.skills",
    ),
    (
        SettingsSection::Subagents,
        "icons/task.svg",
        "settings.subagents",
    ),
    (
        SettingsSection::Permissions,
        "icons/lock.svg",
        "settings.permissions",
    ),
    (
        SettingsSection::Plugins,
        "icons/extensions.svg",
        "settings.plugins",
    ),
    (SettingsSection::Mcp, "icons/tools/mcp.svg", "settings.mcp"),
    (
        SettingsSection::RemoteHosts,
        "icons/server-stack.svg",
        "settings.remote_hosts",
    ),
    (SettingsSection::About, "icons/info.svg", "settings.about"),
    (
        SettingsSection::ReportBug,
        "icons/github.svg",
        "settings.report_bug",
    ),
];

/// Every searchable setting, flattened: section, label key, and the extra
/// terms a query can match. The label is translated when rendered; the
/// keywords are the synonyms people type that the label does not spell out.
/// It lives beside the nav so a new section has one obvious place to
/// register its searchable rows.
const SETTINGS_SEARCH_INDEX: &[(SettingsSection, &str, &str)] = &[
    (
        SettingsSection::GitAccounts,
        "git_accounts.title",
        "git github account identity ssh credential authentication sign-in status cli update version release",
    ),
    // General
    (
        SettingsSection::General,
        "settings.pi_agent",
        "pi agent process stdio connection child spawn",
    ),
    (
        SettingsSection::General,
        "settings.local_by_default",
        "local data store sessions disk offline",
    ),
    (
        SettingsSection::General,
        "settings.workspace",
        "workspace directory folder path project",
    ),
    (
        SettingsSection::General,
        "updater_ui.automatic_updates",
        "updates update automatic release",
    ),
    (
        SettingsSection::General,
        "updater_ui.check_for_updates",
        "updates update check release version",
    ),
    (
        SettingsSection::General,
        "settings.reload_app",
        "reload restart app application process pi refresh",
    ),
    (
        SettingsSection::General,
        "settings.desktop_notifications",
        "notifications banner desktop system",
    ),
    (
        SettingsSection::General,
        "settings.in_app_toasts",
        "notifications toast in-app",
    ),
    (
        SettingsSection::General,
        "settings.notification_sound",
        "notifications sound alert audio",
    ),
    (
        SettingsSection::General,
        "settings.include_reasoning_and_tools",
        "export exports markdown pdf transcript save reasoning thinking tool calls output",
    ),
    // Runtime
    (
        SettingsSection::Runtime,
        "settings.status",
        "runtime process status start stop restart",
    ),
    (
        SettingsSection::Runtime,
        "settings.process_id",
        "runtime process id pid",
    ),
    (
        SettingsSection::Runtime,
        "settings.binary",
        "runtime binary executable pi",
    ),
    (
        SettingsSection::Runtime,
        "settings.uptime",
        "runtime uptime elapsed",
    ),
    (
        SettingsSection::Runtime,
        "settings.transport",
        "runtime transport stdio",
    ),
    (
        SettingsSection::Runtime,
        "settings.last_error",
        "runtime error stderr last",
    ),
    (
        SettingsSection::Runtime,
        "settings.parked_processes",
        "runtime background parked sessions",
    ),
    (
        SettingsSection::Runtime,
        "settings.recent_stderr",
        "runtime stderr log output console",
    ),
    // Agent
    (
        SettingsSection::Agent,
        "settings.enter_while_running",
        "enter send submit queue steer behavior",
    ),
    (
        SettingsSection::Agent,
        "settings.follow_up_messages",
        "follow-up queue messages behavior",
    ),
    (
        SettingsSection::Agent,
        "settings.auto_compaction",
        "compaction compact context auto",
    ),
    (
        SettingsSection::Agent,
        "settings.auto_retry",
        "retry auto error transient",
    ),
    (
        SettingsSection::Agent,
        "settings.auto_session_titles",
        "titles session name auto",
    ),
    (
        SettingsSection::Agent,
        "settings.title_model",
        "title model session names",
    ),
    (
        SettingsSection::Agent,
        "settings.default_model",
        "default model session new",
    ),
    (
        SettingsSection::Agent,
        "settings.default_thinking",
        "thinking level default session reasoning",
    ),
    // Worktrees
    (
        SettingsSection::Worktrees,
        "worktree.settings.directory",
        "worktree worktrees directory location .wt folder path git branch",
    ),
    (
        SettingsSection::Worktrees,
        "worktree.settings.setup_script",
        "worktree worktrees setup script install bootstrap hook",
    ),
    (
        SettingsSection::Worktrees,
        "worktree.settings.run_setup_auto",
        "worktree worktrees setup script automatic auto run",
    ),
    (
        SettingsSection::Worktrees,
        "worktree.settings.open_after_create",
        "worktree worktrees open after create switch workspace",
    ),
    // Appearance
    (
        SettingsSection::Appearance,
        "settings.appearance",
        "appearance mode light dark system theme",
    ),
    (
        SettingsSection::Appearance,
        "settings.accent_color",
        "accent color colour tint highlight swatch",
    ),
    (
        SettingsSection::Appearance,
        "settings.theme",
        "theme palette color light dark editor classic operational flexoki orbit",
    ),
    (
        SettingsSection::Appearance,
        "settings.background_section",
        "background wallpaper image photo desktop hero full window placement",
    ),
    (
        SettingsSection::Appearance,
        "wallpaper.effect",
        "wallpaper effect dither ascii halftone scanlines artwork",
    ),
    (
        SettingsSection::Appearance,
        "wallpaper.dithered_hero",
        "wallpaper preset dithered hero bottom fade",
    ),
    (
        SettingsSection::Appearance,
        "wallpaper.card_opacity",
        "wallpaper card opacity translucent surface",
    ),
    (
        SettingsSection::Appearance,
        "wallpaper.glass",
        "wallpaper glass frost blur frosted backdrop pane panel translucent",
    ),
    (
        SettingsSection::Appearance,
        "wallpaper.derive_accent",
        "wallpaper accent tint color extract",
    ),
    (
        SettingsSection::Appearance,
        "language.title",
        "language locale translation interface",
    ),
    (
        SettingsSection::Appearance,
        "settings.interface_font",
        "font interface ui typeface",
    ),
    (
        SettingsSection::Appearance,
        "settings.code_font",
        "font code mono typeface",
    ),
    (
        SettingsSection::Appearance,
        "settings.ui_font_size",
        "font size ui text scale",
    ),
    (
        SettingsSection::Appearance,
        "settings.terminal_size",
        "font size terminal",
    ),
    (
        SettingsSection::Appearance,
        "settings.editor_size",
        "font size editor",
    ),
    (
        SettingsSection::Appearance,
        "settings.spacing_density",
        "spacing density compact comfortable",
    ),
    (
        SettingsSection::Appearance,
        "settings.show_sidebar",
        "sidebar show layout navigation",
    ),
    (
        SettingsSection::Appearance,
        "settings.reduce_motion",
        "motion animation reduce layout system full",
    ),
    // Resources and extensions
    (
        SettingsSection::Skills,
        "settings.skills",
        "skills skill.md instructions agent",
    ),
    (
        SettingsSection::Subagents,
        "settings.subagents",
        "subagents agents delegate child task scout worker reviewer",
    ),
    (
        SettingsSection::Permissions,
        "settings.permissions",
        "permissions rules allow deny ask approve guard bash command tool access scope subagent",
    ),
    (
        SettingsSection::Plugins,
        "settings.plugins",
        "plugins packages extensions install npm git",
    ),
    (
        SettingsSection::Mcp,
        "settings.mcp",
        "mcp model context protocol servers tools",
    ),
    (
        SettingsSection::RemoteHosts,
        "settings.remote_hosts",
        "remote hosts ssh server vps machine environment connection devbox",
    ),
    (
        SettingsSection::Providers,
        "settings.providers_and_models",
        "providers models catalog favorites hide show api key auth credentials",
    ),
    (
        SettingsSection::Shortcuts,
        "settings.shortcuts",
        "shortcuts keyboard keys chords",
    ),
    (
        SettingsSection::Privacy,
        "settings.privacy",
        "privacy session retention cleanup local data",
    ),
    (
        SettingsSection::About,
        "settings.about",
        "about version updates source github",
    ),
    (
        SettingsSection::ReportBug,
        "settings.report_bug",
        "bug report issue crash problem broken github feedback",
    ),
    (
        SettingsSection::ReportBug,
        "bug_report.heading",
        "bug report issue title description steps reproduce diagnostics",
    ),
];

/// The nav icon and label key for a section, from [`SETTINGS_SECTIONS`].
fn settings_section_meta(section: SettingsSection) -> (&'static str, &'static str) {
    SETTINGS_SECTIONS
        .iter()
        .find(|(candidate, _, _)| *candidate == section)
        .map(|&(_, icon, label_key)| (icon, label_key))
        .unwrap_or(("icons/settings.svg", "settings.general"))
}

/// A section's translated nav label, for prompts that name where leaving goes.
pub(super) fn settings_section_label(section: SettingsSection) -> String {
    tr!(settings_section_meta(section).1)
}

/// Settings whose translated label or keyword list contains the already
/// lowercased `needle`, in index order. An empty needle matches nothing —
/// callers only mount results while a query is active.
fn settings_search_hits(needle: &str) -> Vec<(SettingsSection, &'static str, &'static str)> {
    if needle.is_empty() {
        return Vec::new();
    }
    SETTINGS_SEARCH_INDEX
        .iter()
        .filter(|(_, label_key, keywords)| {
            tr!(*label_key).to_lowercase().contains(needle) || keywords.contains(needle)
        })
        .copied()
        .collect()
}

/// The label a Reduce Motion segment carries.
fn motion_preference_label(preference: MotionPreference) -> String {
    match preference {
        MotionPreference::System => tr!("settings.motion_system"),
        MotionPreference::Reduced => tr!("settings.motion_reduced"),
        MotionPreference::Full => tr!("settings.motion_full"),
    }
}

/// What the Reduce Motion row says is actually in force.
///
/// `System` is not a state the user can read off the segments, so the row names
/// the resolution: following the system, following it into reduced motion, the
/// user's own answer either way, or `Full` overriding a system that asks.
fn motion_status(preference: MotionPreference, os_asks: bool) -> String {
    match preference {
        MotionPreference::System if os_asks => tr!("settings.motion_status_system_reduced"),
        MotionPreference::System => tr!("settings.motion_status_system"),
        MotionPreference::Reduced => tr!("settings.motion_status_reduced"),
        MotionPreference::Full if os_asks => tr!("settings.motion_status_full_overriding"),
        MotionPreference::Full => tr!("settings.motion_status_full"),
    }
}

/// The operation a background plugin task runs.
#[derive(Clone, Copy)]
enum PluginOp {
    Install,
    Update,
    Remove,
}

impl OrbitApp {
    // ── settings surface ───────────────────────────────────────────
    // Left nav (Back + sections), right column of setting
    // rows. Every control maps to real app state; read-only rows show
    // real pi/runtime facts (PRODUCT.md: nothing decorative that
    // pretends to be functional).

    pub(super) fn render_settings(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        if self.settings_section == SettingsSection::Providers {
            // Each provider (including account aliases) keeps its own draft
            // across collapse/reopen, without sharing the page-wide filter.
            for view in self.provider_views() {
                self.provider_model_filters
                    .entry(view.id.clone())
                    .or_insert_with(|| {
                        let input = cx.new(|cx| {
                            ComposerInput::new(cx)
                                .with_element_id(format!("provider-model-filter-{}", view.id))
                                .with_placeholder_key("app.search_models")
                                .with_key_context("Composer Picker")
                                .with_max_lines(1)
                                .with_wrap(false)
                        });
                        let subscription = cx.observe(&input, |_, _, cx| cx.notify());
                        (input, subscription)
                    });
            }
        }
        let this = cx.entity();
        let theme = *theme::get(cx);

        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .relative()
            .bg(theme.panel_fill(theme.background))
            .font_family(theme::ui_font_family())
            // ── nav column ──
            .child(
                div()
                    .w(px(256.))
                    .h_full()
                    .flex_shrink_0()
                    .bg(theme.panel_fill(theme.surface))
                    // Settings' nav is the session sidebar's twin: over a
                    // full-window wallpaper it frosts the picture the same way.
                    .frosted_pane(&theme.for_page())
                    .border_r_1()
                    .border_color(theme.hairline())
                    .flex()
                    .flex_col()
                    // Window drag strip — same height as the session
                    // sidebar's so the two nav columns line up. It holds the
                    // macOS traffic lights inside the transparent titlebar;
                    // elsewhere it is simply how the column can be dragged.
                    .child(window_drag_region(
                        div().h(px(super::view::TOP_BAR_H)).w_full(),
                    ))
                    .child(
                        div()
                            .px(DynamicSpacing::Base12.px(&theme))
                            .pt(DynamicSpacing::Base08.px(&theme))
                            .pb(DynamicSpacing::Base16.px(&theme))
                            .child(crate::motion::hover(
                                press(
                                    div()
                                        .id("settings-back")
                                        .track_focus(&self.settings_back_focus)
                                        .focus_ring_reserved(&theme)
                                        .w_full()
                                        .h(ButtonSize::Large.height(&theme))
                                        .px(DynamicSpacing::Base08.px(&theme))
                                        .rounded(button::RADIUS.px(&theme))
                                        .flex()
                                        .items_center()
                                        .gap(DynamicSpacing::Base08.px(&theme))
                                        .cursor_pointer(),
                                )
                                .on_click(cx.listener(Self::on_settings_back))
                                .child(icon(
                                    "icons/arrow-left.svg",
                                    IconSize::Small.px(&theme),
                                    theme.text_muted,
                                ))
                                .child(
                                    div()
                                        .text_size(TextSize::Default.px(&theme))
                                        .text_color(theme.text_secondary)
                                        .child(tr!("settings.back")),
                                ),
                                "settings-back",
                                &theme,
                                Hover::wash(&theme),
                            )),
                    )
                    // Search — the first control in the column, just above
                    // the list it reaches into, so a query is one click from
                    // any setting in any section.
                    .child(self.settings_search_bar(theme))
                    .child(
                        div()
                            .pl(DynamicSpacing::Base20.px(&theme))
                            .pb(DynamicSpacing::Base08.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Default.px(&theme))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text_primary)
                                    .child(tr!("settings.settings")),
                            ),
                    )
                    // Keep the navigation reachable at short window heights.
                    .child(
                        div()
                            .id("settings-navigation")
                            .debug_selector(|| "settings-navigation".to_string())
                            .track_focus(&self.settings_nav_focus)
                            .on_key_down(cx.listener(Self::on_settings_nav_key))
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.settings_nav_scroll)
                            .px(DynamicSpacing::Base12.px(&theme))
                            .pb(DynamicSpacing::Base12.px(&theme))
                            .flex()
                            .flex_col()
                            .gap(DynamicSpacing::Base02.px(&theme))
                            .children(SETTINGS_SECTIONS.iter().enumerate().map(
                                |(index, &(section, section_icon, label_key))| {
                                    let this = this.clone();
                                    let label = tr!(label_key).to_string();
                                    let selected = self.settings_section == section;
                                    // Group starts: Runtime opens the agent
                                    // block, Skills opens the extensions
                                    // block, and About closes the list alone.
                                    let group_start = matches!(
                                        section,
                                        SettingsSection::Runtime
                                            | SettingsSection::Skills
                                            | SettingsSection::About
                                            | SettingsSection::ReportBug
                                    );
                                    let row = div()
                                        .id(ElementId::Name(format!("settings-nav-{label}").into()))
                                        .w_full()
                                        .h(ButtonSize::Large.height(&theme))
                                        .flex_none()
                                        .debug_selector(move || format!("settings-nav-{label_key}"))
                                        .px(DynamicSpacing::Base08.px(&theme))
                                        .rounded(Radius::Medium.px(&theme))
                                        .border_1()
                                        .border_color(gpui::transparent_black())
                                        .text_size(TextSize::Default.px(&theme))
                                        .flex()
                                        .items_center()
                                        .gap(DynamicSpacing::Base12.px(&theme))
                                        .cursor_pointer()
                                        .when(selected, |row| {
                                            row.font_weight(FontWeight::MEDIUM)
                                                .bg(theme.selected_wash())
                                        })
                                        .on_click(move |_, window, cx| {
                                            this.update(cx, |app, cx| {
                                                window.focus(&app.settings_nav_focus);
                                                app.activate_settings_nav_row(index, cx);
                                            });
                                        })
                                        .child(icon(
                                            section_icon,
                                            IconSize::Medium.px(&theme),
                                            if selected {
                                                theme.accent_text
                                            } else {
                                                theme.text_muted
                                            },
                                        ))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .text_color(if selected {
                                                    theme.text_primary
                                                } else {
                                                    theme.text_secondary
                                                })
                                                .child(label.to_string()),
                                        )
                                        .when(section == SettingsSection::RemoteHosts, |row| {
                                            row.child(crate::ui::chip::experimental_badge(
                                                &theme,
                                            ))
                                        });
                                    // The open section holds its wash; the
                                    // others fade it in under the pointer.
                                    let row = if selected {
                                        row.into_any_element()
                                    } else {
                                        crate::motion::hover(
                                            row,
                                            format!("settings-nav-{label_key}"),
                                            &theme,
                                            Hover::wash(&theme),
                                        )
                                    };
                                    if group_start {
                                        div()
                                            .flex_none()
                                            .flex()
                                            .flex_col()
                                            .gap(DynamicSpacing::Base02.px(&theme))
                                            .pt(DynamicSpacing::Base08.px(&theme))
                                            .mt(DynamicSpacing::Base08.px(&theme))
                                            .child(row)
                                            .into_any_element()
                                    } else {
                                        row.into_any_element()
                                    }
                                },
                            )),
                    )
                    // Footer: the build identity, pinned bottom-left in the
                    // same tertiary register the About page uses — a quiet
                    // closer for the column. A hairline caps the scrollable
                    // area above it, and the left inset keeps the icon
                    // column's 16px edge.
                    .child(
                        div()
                            .mt_auto()
                            .w_full()
                            .px(DynamicSpacing::Base16.px(&theme))
                            .py(DynamicSpacing::Base12.px(&theme))
                            .border_t_1()
                            .border_color(theme.hairline())
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Small.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_muted)
                                    .child(tr!("app.name")),
                            )
                            .child(
                                div()
                                    .text_size(TextSize::Small.px(&theme))
                                    .font(crate::usage::view::num_font())
                                    .text_color(theme.text_muted)
                                    .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                            ),
                    ),
            )
            // ── content column ──
            // While the sidebar search carries a query the column shows
            // matches instead of the active section. Skills owns a full-bleed
            // master-detail surface; every other section is a fixed header + a
            // scrolling column of cards.
            //
            // A section is a navigation, so its content arrives with a fade
            // (`motion::arrive`) inside a column that never moves — the nav
            // beside it stays put. The name is the section's own label key, so
            // the destination identifies the clock and searching (which only
            // updates the content of one destination) does not re-fade on every
            // keystroke.
            .child({
                let searching = !self.settings_search.read(cx).text().trim().is_empty();
                let (page, body) = if searching {
                    (
                        "settings.search",
                        self.settings_search_results(theme, this.clone(), cx),
                    )
                } else if self.settings_section == SettingsSection::Skills {
                    (
                        "settings.skills",
                        self.render_skills_page(theme, this.clone(), cx),
                    )
                } else if self.settings_section == SettingsSection::Subagents {
                    (
                        "settings.subagents",
                        self.render_subagents_page(theme, this.clone(), cx),
                    )
                } else {
                    (
                        settings_section_meta(self.settings_section).1,
                        self.settings_body(theme, this.clone(), cx),
                    )
                };
                crate::motion::arrive(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .child(body),
                    page,
                    &theme,
                    Motion::Layout,
                )
            })
            // ── provider editor modals (models.json + API key) ──
            .children(self.provider_editor_layer(theme, this.clone(), cx))
            .children(self.provider_usage_layer(theme, this.clone(), cx))
            // ── MCP add/edit modal ──
            .children(self.mcp_editor_layer(theme, this.clone(), cx))
            // ── Remote hosts add/edit modal ──
            .children(self.remote_host_editor_layer(theme, this.clone(), cx))
            // ── a host's Add API key dialog ──
            .children(self.remote_key_layer(theme, this.clone(), cx))
            // ── a provider's Add-account modal ──
            .children(self.account_editor_layer(theme, this.clone(), cx))
            .children(self.provider_key_layer(theme, this, cx))
    }

    /// The standard settings content column: a pinned header (title +
    /// section toolbar) over a scrolling body of cards.
    fn settings_body(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            // The title (and, on Providers/Plugins, the toolbar) stay pinned;
            // gpui has no sticky positioning, so they live outside the scroll
            // container.
            .child(
                div()
                    .w_full()
                    .max_w(px(CONTENT_MAX_W))
                    .mx_auto()
                    .flex_none()
                            .child(
                        div()
                            .w_full()
                            .px(DynamicSpacing::Base24.px(&theme))
                            .pt(DynamicSpacing::Base20.px(&theme))
                            .pb(DynamicSpacing::Base12.px(&theme))
                            .flex()
                            .flex_col()
                            .gap(DynamicSpacing::Base12.px(&theme))
                            .child(self.settings_header(theme))
                            .children(self.settings_toolbar(theme, this.clone(), cx)),
                    )
                    .child(
                        div()
                            .w_full()
                            .px(DynamicSpacing::Base24.px(&theme))
                            .child(div().w_full().h(BORDER_WIDTH).bg(theme.hairline())),
                    ),
            )
            .child(
                div()
                    .id("settings-content")
                    .debug_selector(|| "settings-content".to_string())
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .max_w(px(CONTENT_MAX_W))
                    .mx_auto()
                    .overflow_y_scroll()
                    .track_scroll(&self.settings_content_scroll)
                    .child(
                        div()
                            .w_full()
                            .px(DynamicSpacing::Base24.px(&theme))
                            .pt(DynamicSpacing::Base20.px(&theme))
                            .pb(DynamicSpacing::Base24.px(&theme))
                            .flex()
                            .flex_col()
                            .gap(DynamicSpacing::Base20.px(&theme))
                            .children(self.error_banner(theme, cx))
                            .children(self.settings_rows(&this, theme, cx)),
                    ),
            )
            .into_any_element()
    }

    /// The global settings search, at the top of the nav column so a query
    /// can reach any setting without hunting through the section list.
    fn settings_search_bar(&self, theme: Theme) -> AnyElement {
        let search = search_field(self.settings_search.clone(), &theme).w_full();
        div()
            .w_full()
            .flex_shrink_0()
            .px(DynamicSpacing::Base12.px(&theme))
            .pb(DynamicSpacing::Base20.px(&theme))
            .child(search)
            .into_any_element()
    }

    /// The search results: every setting whose label or keywords match,
    /// grouped as one board. A row opens its section and clears the query, so
    /// the section it points at is visible immediately.
    fn settings_search_results(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let needle = self.settings_search.read(cx).text().trim().to_lowercase();
        let hits = settings_search_hits(&needle);
        let body: AnyElement = if hits.is_empty() {
            self.empty_resource_card(
                theme,
                "icons/search.svg",
                &tr!("settings.no_matches"),
                &tr!("settings.try_a_different_search"),
            )
        } else {
            let rows = hits
                .into_iter()
                .map(|(section, label_key, _)| {
                    self.settings_search_row(section, label_key, theme, this.clone())
                })
                .collect();
            self.settings_group(theme, rows)
        };
        div()
            .id("settings-search-results")
            .flex_1()
            .min_h_0()
            .w_full()
            .max_w(px(CONTENT_MAX_W))
            .mx_auto()
            .overflow_y_scroll()
            .child(
                div()
                    .w_full()
                    .px(DynamicSpacing::Base24.px(&theme))
                    .pt(DynamicSpacing::Base20.px(&theme))
                    .pb(DynamicSpacing::Base24.px(&theme))
                    .child(body),
            )
            .into_any_element()
    }

    /// One search result: the setting's label leading, its section trailing,
    /// with the section's nav icon at the start of the row.
    fn settings_search_row(
        &self,
        section: SettingsSection,
        label_key: &'static str,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let (section_icon, section_key) = settings_section_meta(section);
        crate::motion::hover(
            press(
                div()
                    .id(ElementId::Name(
                        format!("settings-result-{label_key}").into(),
                    ))
                    .w_full()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .py(DynamicSpacing::Base12.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .cursor_pointer(),
            )
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.settings_search.update(cx, |input, cx| input.clear(cx));
                    app.set_settings_section(section, cx);
                });
            })
            .child(icon(
                section_icon,
                IconSize::Small.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Default.px(&theme))
                    .text_color(theme.text_primary)
                    .child(tr!(label_key).to_string()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!(section_key).to_string()),
            ),
            format!("settings-result-{label_key}"),
            &theme,
            Hover::wash(&theme),
        )
        .into_any_element()
    }

    pub(super) fn settings_header(&self, theme: Theme) -> AnyElement {
        let (title, subtitle): (String, String) = match self.settings_section {
            SettingsSection::General => {
                (tr!("settings.general"), tr!("settings.general_description"))
            }
            SettingsSection::Runtime => {
                (tr!("settings.runtime"), tr!("settings.runtime_description"))
            }
            SettingsSection::Agent => (tr!("settings.agent"), tr!("settings.agent_description")),
            SettingsSection::Worktrees => (
                tr!("worktree.settings.title"),
                tr!("worktree.settings.description"),
            ),
            SettingsSection::GitAccounts => {
                (tr!("git_accounts.title"), tr!("git_accounts.description"))
            }
            SettingsSection::Skills => (tr!("settings.skills"), tr!("settings.skills_description")),
            SettingsSection::Subagents => (
                tr!("settings.subagents"),
                tr!("settings.subagents_description"),
            ),
            SettingsSection::Permissions => (
                tr!("settings.permissions"),
                tr!("settings.permissions_description"),
            ),
            SettingsSection::Plugins => {
                (tr!("settings.plugins"), tr!("settings.plugins_description"))
            }
            SettingsSection::Mcp => (tr!("settings.mcp"), tr!("settings.mcp_description")),
            SettingsSection::RemoteHosts => (
                tr!("settings.remote_hosts"),
                tr!("settings.remote_hosts_description"),
            ),
            SettingsSection::Appearance => (
                tr!("settings.appearance"),
                tr!("settings.appearance_description"),
            ),
            SettingsSection::Providers => (
                tr!("settings.providers_and_models"),
                tr!("settings.providers_and_models_description"),
            ),
            SettingsSection::Shortcuts => (
                tr!("settings.shortcuts"),
                tr!("settings.shortcuts_description"),
            ),
            SettingsSection::Privacy => {
                (tr!("settings.privacy"), tr!("settings.privacy_description"))
            }
            // About names the product itself: the nav already says "About".
            SettingsSection::About => (
                tr!("settings.orbit_pi"),
                tr!("settings.native_workbench_for_the_pi_coding_agent"),
            ),
            SettingsSection::ReportBug => (
                tr!("settings.report_bug"),
                tr!("settings.report_bug_description"),
            ),
        };
        let about = self.settings_section == SettingsSection::About;
        let text = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .debug_selector(|| "settings-page-title".to_string())
                            .text_size(HeadlineSize::Large.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(BufferLineHeight::Standard.relative())
                            .text_color(theme.text_primary)
                            .child(title.to_string()),
                    )
                    .when(self.settings_section == SettingsSection::RemoteHosts, |line| {
                        line.child(crate::ui::chip::experimental_badge(&theme))
                    }),
            )
            // One line: the description sits in the pinned header, and a
            // wrapped sentence there is measured as a single line while it
            // paints as several, which under-sized the header and let the
            // toolbar paint over the first card. The full sentence is on
            // hover.
            // One line: a sentence that wraps here is measured as a single
            // line while it paints as several, so the pinned header came out
            // short and its toolbar painted over the first card. The full
            // sentence is on hover.
            .child(
                div()
                    .id("settings-section-description")
                    .text_size(TextSize::Default.px(&theme))
                    .text_color(theme.text_secondary)
                    .line_clamp(1)
                    .tooltip({
                        let subtitle = subtitle.clone();
                        move |_, cx| {
                            cx.new(|_| crate::ui::tooltip::Tooltip::new(subtitle.clone()))
                                .into()
                        }
                    })
                    .child(subtitle),
            )
            // What "Experimental" means here, said once on the page itself.
            .when(self.settings_section == SettingsSection::RemoteHosts, |header| {
                header.child(
                    div()
                        .debug_selector(|| "remote-hosts-experimental-note".to_string())
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("remote_hosts.experimental_note")),
                )
            })
            // The build facts in one quiet line, in place of a board row each.
            .when(about, |header| {
                header.child(
                    div()
                        .debug_selector(|| "about-version".to_string())
                        .text_size(TextSize::Small.px(&theme))
                        .font(crate::ui::style::text::num_font())
                        .text_color(theme.text_muted)
                        .child(tr!(
                            "settings.about_meta",
                            version = env!("CARGO_PKG_VERSION"),
                            gpui = GPUI_VERSION
                        )),
                )
            });
        if !about {
            return text.into_any_element();
        }
        // The app mark beside its name: an identity block, not a page title
        // stacked under an icon.
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base16.px(&theme))
            .child(
                div()
                    .debug_selector(|| "about-identity-mark".to_string())
                    .flex_none()
                    // A definite box: the mark is a height-driven image, so its
                    // width would otherwise come from the decoded asset and the
                    // identity row would reflow when it arrived.
                    .size(IconSize::XLarge.px(&theme))
                    .child(embedded_image(
                        crate::app_icon::ASSET,
                        f32::from(IconSize::XLarge.px(&theme)),
                    )),
            )
            // The identity text takes the row's remaining width, so the
            // subtitle and the version line wrap at the page rather than at the
            // title's own width. It is `flex_1` *here* and not on `text`
            // itself: every other section hands this column to a column parent,
            // where growing would stretch the header instead of widening it.
            .child(
                text.flex_1()
                    .debug_selector(|| "about-identity-text".to_string()),
            )
            .into_any_element()
    }

    /// The rows for the active section, as card elements. `this` rides
    /// along for closures in interactive controls (rows themselves are
    /// built read-only from app state).
    pub(super) fn settings_rows(
        &self,
        this: &Entity<OrbitApp>,
        theme: Theme,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        match self.settings_section {
            SettingsSection::General => {
                let workspace = self
                    .current_workspace
                    .clone()
                    .or_else(|| std::env::current_dir().ok())
                    .unwrap_or_default();
                let mut rows = vec![self.settings_section(
                    theme,
                    &tr!("settings.this_machine"),
                    vec![
                        self.setting_row(
                            theme,
                            &tr!("settings.pi_agent"),
                            Some(&tr!(
                                "settings.spawned_as_a_child_process_newline_delimited_jso"
                            )),
                            None,
                            Some(self.connection_status(theme)),
                        ),
                        self.setting_row(
                            theme,
                            &tr!("settings.local_by_default"),
                            Some(&tr!(
                                "settings.sessions_live_in_pi_s_own_store_on_this_computer"
                            )),
                            Some(&sessions::sessions_dir().to_string_lossy()),
                            None,
                        ),
                        self.setting_row(
                            theme,
                            &tr!("settings.workspace"),
                            Some(&tr!("settings.new_tasks_start_in_this_directory")),
                            Some(&workspace.to_string_lossy()),
                            None,
                        ),
                    ],
                )];
                if let Some(updates) = self.updater_section(theme, this.clone(), cx) {
                    rows.push(updates);
                }
                rows.push(self.notification_rows(theme, this.clone()));
                rows.push(self.export_rows(theme, this.clone()));
                // The last group is the escape hatch: a clean relaunch kills
                // every live run, so it sits below the preferences rather than
                // beside the everyday controls.
                let mut application = Vec::new();
                if crate::tray::supported() {
                    application.push(self.setting_row(
                        theme,
                        &tr!("settings.closing_the_window"),
                        Some(&if cfg!(target_os = "macos") {
                            tr!("settings.closing_the_window_hint_macos")
                        } else {
                            tr!("settings.closing_the_window_hint")
                        }),
                        None,
                        Some(self.close_behavior_toggle(theme, this.clone())),
                    ));
                }
                application.push(self.setting_row(
                    theme,
                    &tr!("settings.reload_app"),
                    Some(&tr!("settings.reload_app_description")),
                    None,
                    Some(self.runtime_button(
                        "reload-app",
                        &tr!("settings.reload_app_action"),
                        false,
                        theme,
                        this.clone(),
                        OrbitApp::reload_app,
                    )),
                ));
                rows.push(self.settings_section(theme, &tr!("settings.application"), application));
                rows
            }
            SettingsSection::Runtime => self.runtime_rows(theme, this.clone(), cx),
            SettingsSection::Agent => self.agent_rows(theme, this.clone(), cx),
            SettingsSection::Worktrees => self.worktree_settings_rows(theme, this.clone(), cx),
            SettingsSection::GitAccounts => self.git_accounts_rows(theme, this.clone()),
            // Rendered by `skills_ui::render_skills_page`, not the card body.
            SettingsSection::Skills => Vec::new(),
            // Rendered by `subagents_ui::render_subagents_page`.
            SettingsSection::Subagents => Vec::new(),
            SettingsSection::Permissions => self.permissions_rows(theme, this.clone(), cx),
            SettingsSection::Plugins => self.plugin_rows(theme, this.clone(), cx),
            // The MCP page's rows are owned by `mcp_ui`; it needs the cursor
            // and filter state that lives beside the manager.
            SettingsSection::Mcp => self.mcp_rows(theme, this.clone(), cx),
            SettingsSection::RemoteHosts => self.remote_hosts_rows(theme, this.clone()),
            SettingsSection::Appearance => self.appearance_rows(theme, this.clone(), cx),
            SettingsSection::Providers => match self.providers_remote_host() {
                // A host shows its own providers, read from its pi.
                Some(host) => self.remote_provider_rows(&host, theme, this.clone()),
                None => self.provider_rows(theme, this.clone(), cx),
            },
            SettingsSection::Shortcuts => self.shortcut_rows(theme),
            SettingsSection::Privacy => self.privacy_rows(theme, this.clone(), cx),
            SettingsSection::About => {
                // The header carries the identity (mark, name, version), so
                // the board holds only what the user can act on or check:
                // updates, the agent runtime, and the project links.
                let mut about = Vec::new();
                if let Some(update) = self.about_update_row(theme, this.clone(), cx) {
                    about.push(update);
                }
                about.push(self.setting_row(
                    theme,
                    &tr!("settings.pi_cli"),
                    Some(&tr!(
                        "settings.the_only_agent_runtime_pi_speaks_its_own_rpc_pro"
                    )),
                    None,
                    Some(self.connection_status(theme)),
                ));
                about.push(self.setting_row(
                    theme,
                    &tr!("settings.source"),
                    Some(&tr!(
                        "settings.open_source_under_apache_2_0_code_issues_and_rel"
                    )),
                    None,
                    Some(
                        div()
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .child(self.repo_star_button(theme, cx))
                            .child(self.about_github_button(theme))
                            .into_any_element(),
                    ),
                ));
                vec![self.settings_group(theme, about)]
            }
            SettingsSection::ReportBug => self.bug_report_rows(theme, cx),
        }
    }

    /// The pinned toolbar rows for the active section (Providers search + add,
    /// Plugins install + refresh). Empty on sections without one.
    ///
    /// The rows are returned flat rather than wrapped in one column: the pinned
    /// header already stacks them, and a nested auto-height column here measured
    /// shorter than the row it held — the toolbar then painted over the card
    /// below it.
    pub(super) fn settings_toolbar(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        match self.settings_section {
            SettingsSection::Providers => match self.providers_remote_host() {
                Some(host) => vec![self.remote_provider_toolbar(&host, theme, this)],
                None => match self.providers_machine_selector(theme, this.clone()) {
                    // The selector sits above this Mac's own toolbar.
                    Some(selector) => {
                        vec![selector, self.provider_toolbar(theme, this, cx)]
                    }
                    None => vec![self.provider_toolbar(theme, this, cx)],
                },
            },
            SettingsSection::Plugins => vec![self.plugin_toolbar(theme, this, cx)],
            SettingsSection::Mcp => vec![self.mcp_toolbar(theme, this, cx)],
            SettingsSection::RemoteHosts => vec![self.remote_hosts_toolbar(theme, this)],
            SettingsSection::GitAccounts => vec![self.git_accounts_toolbar(theme, this)],
            _ => Vec::new(),
        }
    }

    // ── Settings → Providers & models ──────────────────────────────────────────────

    /// The favorites filter chip on the Providers & models toolbar: narrows
    /// the page to providers with a starred model, and to those models.
    fn models_favorites_button(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        // A filter chip in the toolbar's button language: `surface_elevated` + a
        // hairline when off, the standard `selection` fill when on — never an
        // accent wash, which is reserved for the favorite stars themselves.
        ChipButton::new("models-favorites-filter", ChipStyle::Outlined { inset: false })
            .size(ButtonSize::Large)
            .icon("icons/star.svg")
            .label(tr!("settings.favorites"))
            .checked(self.models_favorites_only)
            .on_mouse_up(&theme, {
                let this = this.clone();
                move |_, _, cx| {
                    this.update(cx, |app, cx| app.toggle_models_favorites_only(cx));
                }
            })
    }

    /// Refresh pi's provider/model catalog (`pi update --models`).
    fn models_update_button(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        // Refresh pi's provider/model catalog (`pi update --models`) and
        // reload it. The catalog is pi's own data, so this is the one control
        // that fetches new providers/models rather than filtering what we have.
        Button::keyed("models-update", ButtonVariant::Secondary)
.size(ButtonSize::Large)
.faded(&theme, |button| button
                .when(self.models_refreshing, |button| button.opacity(0.6))
                .on_mouse_up(MouseButton::Left, {
                    let this = this.clone();
                    move |_, _, cx| {
                        this.update(cx, |app, cx| app.models_refresh(cx));
                    }
                })
                .child(refresh_glyph(
                    "models-update-spin",
                    ButtonSize::Large.icon_size().px(&theme),
                    self.models_refreshing,
                    theme.text_secondary,
                    theme,
                ))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary)
                        .child(tr!("settings.models_update")),
                ))
    }

    /// The shared new-session default, above the model catalog: the model
    /// and its thinking level, the same two rows (and the same persisted
    /// `session-defaults.json`) as Settings → Agent, so either page edits one
    /// setting. The thinking row follows the model: until one is picked it is
    /// a disabled `pi default` chip, and its options are that model's levels.
    fn default_model_card(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let model = self.default_model_select(theme, this.clone(), cx);
        let thinking = self.default_thinking_select(theme, this, cx);
        // The shared settings board (no `overflow_hidden`: the selects' popups
        // anchor inside their rows and must escape it).
        div()
            .w_full()
            .debug_selector(|| "default-model-card".to_string())
            .child(self.settings_group(
                theme,
                vec![
                    self.setting_row(
                        theme,
                        &tr!("settings.default_model"),
                        Some(&tr!("settings.default_model_hint")),
                        None,
                        Some(model),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.default_thinking"),
                        Some(&tr!("settings.default_thinking_hint")),
                        None,
                        Some(thinking),
                    ),
                ],
            ))
            .into_any_element()
    }

    /// Flip the Models page's favorites-only filter.
    pub(super) fn toggle_models_favorites_only(&mut self, cx: &mut Context<Self>) {
        self.models_favorites_only = !self.models_favorites_only;
        cx.notify();
    }

    /// Update the provider/model catalog: run pi's documented `pi update
    /// --models`, then restart the agent so it loads the refreshed catalog and
    /// re-query. Refused mid-run, like every other restart.
    pub(super) fn models_refresh(&mut self, cx: &mut Context<Self>) {
        if self.models_refreshing {
            return;
        }
        // A live run still holds the process; swapping pi's catalog files under
        // it and restarting would kill the turn.
        if self.busy || self.transcript.is_streaming() {
            self.toast_warning(tr!("settings.finish_turn_before_restart"));
            cx.notify();
            return;
        }
        self.models_refreshing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async { crate::pi_install::run_model_catalog_refresh() })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.models_refreshing = false;
                match result {
                    Ok(output) if output.status.success() => {
                        // pi keeps the catalog it loaded at startup, so reload
                        // it by restarting and resuming the active session.
                        app.restart_pi_for_catalog(cx);
                        app.toast_success(tr!("settings.models_updated"));
                    }
                    Ok(output) => {
                        let detail =
                            crate::pi_update::failure_detail(&output.stdout, &output.stderr);
                        app.toast_error(tr!("settings.models_update_failed", detail = detail));
                    }
                    Err(err) => {
                        app.toast_error(tr!(
                            "settings.models_update_failed",
                            detail = err.to_string()
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Restart the agent so a catalog refreshed on disk is loaded, resuming the
    /// active session when one exists. Mirrors the credential-reload restart.
    ///
    /// The refresh runs off the main thread, so the state is re-checked here:
    /// a run or a fork may have started while `pi --models` was running, and
    /// neither may be yanked out from under.
    pub(super) fn restart_pi_for_catalog(&mut self, cx: &mut Context<Self>) {
        if self.rewind_transition_pending(cx) {
            return;
        }
        if self.busy || self.transcript.is_streaming() {
            self.toast_warning(tr!("settings.finish_turn_before_restart"));
            cx.notify();
            return;
        }
        let resume = self
            .current_session_path
            .clone()
            .and_then(|path| self.sessions.iter().find(|s| s.path == path).cloned());
        self.drop_client();
        self.current_session_path = None;
        if let Some(session) = resume {
            self.switch_to_session(session, false, cx);
        } else {
            self.runtime_start(cx);
        }
    }

    /// A section label on the Providers & models page, with its count.
    fn provider_section_label(&self, label: String, count: usize, theme: Theme) -> AnyElement {
        // The same label register as `settings_section`, with the group's
        // count trailing in muted ink.
        div()
            .w_full()
            .px(DynamicSpacing::Base04.px(&theme))
            .flex()
            .items_baseline()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(label),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .font(crate::usage::view::num_font())
                    .text_color(theme.text_muted)
                    .child(count.to_string()),
            )
            .into_any_element()
    }

    // ── Settings → Plugins ─────────────────────────────────────────────

    /// Installed pi packages plus the install field. Plugins are managed
    /// through `pi install/remove/update`, so pi owns the network fetch and
    /// the settings write.
    pub(super) fn plugin_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = Vec::new();
        if let Some(error) = &self.plugins_error {
            rows.push(self.provider_error_card(
                theme,
                &tr!("settings.could_not_read_settings"),
                error,
            ));
        }
        if self.plugins.is_empty() {
            rows.push(self.empty_resource_card(
                theme,
                "icons/extensions.svg",
                &tr!("settings.no_plugins_installed"),
                &tr!("settings.no_plugins_installed_hint"),
            ));
            return rows;
        }

        let needle = self.plugins_filter.read(cx).text().trim().to_lowercase();
        let visible: Vec<&PluginPackage> = self
            .plugins
            .iter()
            .filter(|package| {
                needle.is_empty()
                    || package.source.to_lowercase().contains(&needle)
                    || package
                        .version
                        .as_deref()
                        .is_some_and(|version| version.to_lowercase().contains(&needle))
            })
            .collect();

        if visible.is_empty() {
            rows.push(self.empty_resource_card(
                theme,
                "icons/search.svg",
                &tr!("settings.no_plugins_match"),
                &tr!("settings.no_plugins_match_hint"),
            ));
            return rows;
        }
        // The pinned toolbar already states the total; spell the count out
        // only when a search narrows the list, on the rows it describes.
        if !needle.is_empty() {
            let installed = self
                .plugins
                .iter()
                .filter(|package| package.installed)
                .count();
            rows.push(
                div()
                    .w_full()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!(
                        "settings.plugins_shown_installed",
                        shown = visible.len(),
                        total = self.plugins.len(),
                        installed = installed
                    ))
                    .into_any_element(),
            );
        }
        // Say what a disabled package is, once, and only when one is on the
        // page: the switch is the only place this could be guessed at.
        if self.plugins.iter().any(|package| !package.enabled) {
            rows.push(
                div()
                    .debug_selector(|| "plugins-disabled-hint".to_string())
                    .w_full()
                    .px(DynamicSpacing::Base04.px(&theme))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("settings.plugin_disabled_hint"))
                    .into_any_element(),
            );
        }
        let mut cards = Vec::new();
        for package in visible {
            cards.push(self.plugin_card(package, theme, this.clone()));
        }
        rows.push(self.settings_group(theme, cards));
        rows
    }

    fn plugin_card(
        &self,
        package: &PluginPackage,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let source = package.source.clone();
        let project = package.scope == PackageScope::Project;
        let name = package.name.clone();

        let mut badges = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .pl(DynamicSpacing::Base40.px(&theme))
            .child(self.provider_badge(
                package.scope.label(),
                theme.text_muted,
                theme.overlay_strong,
                theme,
            ))
            .child(self.provider_badge(
                package.kind.label(),
                theme.text_muted,
                theme.overlay_strong,
                theme,
            ));
        if let Some(version) = &package.version {
            badges = badges.child(self.provider_badge(
                &format!("v{version}"),
                theme.text_muted,
                theme.overlay_strong,
                theme,
            ));
        }
        if !package.installed {
            badges = badges.child(self.provider_badge(
                &tr!("settings.not_installed"),
                theme.error,
                theme.error.opacity(0.12),
                theme,
            ));
        }
        if !package.enabled {
            badges = badges.child(self.provider_badge(
                &tr!("settings.plugin_disabled_badge"),
                theme.text_muted,
                theme.overlay_strong,
                theme,
            ));
        }

        let actions: AnyElement = {
            let mut actions = div()
                .flex_none()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme));
            // One switch per package: the id keys the transition, so two rows
            // sharing one id would share a knob.
            let toggle_this = this.clone();
            let toggle_source = source.clone();
            let toggle_enabled = package.enabled;
            actions = actions.child(switch(
                format!("plugin-toggle-{source}"),
                package.enabled,
                &theme,
                move |_, cx| {
                    toggle_this.update(cx, |app, cx| {
                        app.apply_plugin_action(
                            PluginAction::ToggleEnabled {
                                source: toggle_source.clone(),
                                project,
                                enabled: !toggle_enabled,
                            },
                            cx,
                        );
                    });
                },
            ));
            // A configured package whose install directory is gone has one
            // fix: install it. Update and Remove only make sense on disk.
            if package.installed {
                actions = actions.child(self.plugin_button(
                    format!("plugin-update-{source}"),
                    &tr!("settings.update"),
                    false,
                    PluginButtonSize::Compact,
                    Some("icons/refresh.svg"),
                    false,
                    theme,
                    this.clone(),
                    PluginAction::Update {
                        source: source.clone(),
                    },
                ));
            } else {
                actions = actions.child(self.plugin_button(
                    format!("plugin-reinstall-{source}"),
                    &tr!("settings.install"),
                    false,
                    PluginButtonSize::Compact,
                    Some("icons/plus.svg"),
                    false,
                    theme,
                    this.clone(),
                    PluginAction::InstallOne {
                        source: source.clone(),
                        project,
                    },
                ));
            }
            actions = actions.child(self.plugin_button(
                format!("plugin-remove-{source}"),
                &tr!("settings.remove"),
                false,
                PluginButtonSize::Compact,
                Some("icons/trash.svg"),
                false,
                theme,
                this,
                PluginAction::Remove {
                    source: source.clone(),
                    project,
                    name,
                },
            ));
            actions.into_any_element()
        };

        div()
            .w_full()
            .min_w_0()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base12.px(&theme))
                    .child(
                        div()
                            .size(px(28.))
                            .flex_none()
                            .rounded(Radius::Large.px(&theme))
                            .bg(theme.overlay)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(
                                "icons/extensions.svg",
                                IconSize::Small.px(&theme),
                                theme.text_secondary,
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(DynamicSpacing::Base02.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Default.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .truncate()
                                    .child(package.name.clone()),
                            )
                            .child(
                                div()
                                    .font_family(theme::code_font_family())
                                    .text_size(theme.code_px(10.5))
                                    .text_color(theme.text_muted)
                                    .truncate()
                                    .child(source),
                            ),
                    )
                    .child(actions),
            )
            .child(badges)
            .into_any_element()
    }

    /// The pinned Plugins toolbar: the search field, then the install field,
    /// scope target, and Install, then the status line and Refresh.
    pub(super) fn plugin_toolbar(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let field = input_field_frame(div(), &theme)
            .flex_1()
            .min_w_0()
            .child(icon(
                "icons/extensions.svg",
                IconSize::Small.px(&theme),
                theme.text_muted,
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(self.plugin_source_input.clone()),
            );

        let scope = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(self.plugin_scope_chip(
                "Global",
                !self.plugin_install_project,
                false,
                theme,
                this.clone(),
            ))
            .child(self.plugin_scope_chip(
                "Project",
                self.plugin_install_project,
                true,
                theme,
                this.clone(),
            ));

        let install = self.plugin_button(
            "plugin-install".to_string(),
            &tr!("settings.install"),
            true,
            PluginButtonSize::Toolbar,
            Some("icons/plus.svg"),
            false,
            theme,
            this.clone(),
            PluginAction::Install,
        );

        let refresh = self.plugin_button(
            "plugin-refresh".to_string(),
            &tr!("settings.refresh"),
            false,
            PluginButtonSize::Toolbar,
            Some("icons/refresh.svg"),
            self.plugin_refresh_spin_until.is_some(),
            theme,
            this.clone(),
            PluginAction::Refresh,
        );

        // A plugin edit only takes effect when pi next starts. Restart it
        // here rather than sending the reader to another page.
        let restart = self.client.is_some().then(|| {
            self.plugin_button(
                "plugin-restart-pi".to_string(),
                &tr!("settings.restart_pi"),
                false,
                PluginButtonSize::Toolbar,
                Some("icons/refresh.svg"),
                false,
                theme,
                this.clone(),
                PluginAction::RestartPi,
            )
        });

        let plugin_spinner: AnyElement = crate::ui::prelude::spinner(
            "plugin-action-spin",
            IconSize::Small.px(&theme),
            theme.text_secondary,
            theme,
        );
        let status: AnyElement = match &self.plugin_action {
            Some(action) => div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base08.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_secondary)
                .child(plugin_spinner)
                .child(action.clone())
                .into_any_element(),
            // The outcome of the last operation stays here, where the action
            // was taken, instead of a toast in the corner.
            None => match &self.plugin_result {
                Some(result) => div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(result.clone())
                    .into_any_element(),
                None => div()
                    .flex_1()
                    .min_w_0()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("settings.n_packages", count = self.plugins.len()))
                    .into_any_element(),
            },
        };

        let search = search_field(self.plugins_filter.clone(), &theme).w_full();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            // Search leads, like Providers and Models — the filter belongs
            // with the list it narrows, above the controls that add to it.
            .child(search)
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(field)
                    .child(scope)
                    .child(install),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(status)
                    .children(restart)
                    .child(refresh),
            )
            .into_any_element()
    }

    fn plugin_scope_chip(
        &self,
        label: &str,
        active: bool,
        project: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let id = format!(
            "plugin-scope-{}",
            if project { "project" } else { "global" }
        );
        ChipButton::new(id, ChipStyle::Outlined { inset: false })
            .size(ButtonSize::Large)
            .label(label.to_string())
            .selected(active)
            .on_mouse_up(&theme, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.apply_plugin_action(PluginAction::SetScope { project }, cx)
                });
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn plugin_button(
        &self,
        id: String,
        label: &str,
        primary: bool,
        size: PluginButtonSize,
        icon_path: Option<&'static str>,
        spinning: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: PluginAction,
    ) -> AnyElement {
        let button_size = match size {
            PluginButtonSize::Compact => ButtonSize::Medium,
            PluginButtonSize::Toolbar => ButtonSize::Large,
        };
        let variant = if primary {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        };
        let spin_id = ElementId::Name(format!("{id}-spin").into());
        let mut button = Button::new(id, variant)
            .size(button_size)
            .label(label.to_string());
        if let Some(path) = icon_path {
            button = if spinning {
                // Refresh turns its own glyph in place; the synchronous
                // reload still reads as a deliberate action.
                button.leading(refresh_glyph(
                    spin_id,
                    button_size.icon_size().px(&theme),
                    true,
                    variant.ink(&theme),
                    theme,
                ))
            } else {
                button.icon(path)
            };
        }
        button
            .on_mouse_up(&theme, move |_, _, cx| {
                let action = action.clone();
                this.update(cx, |app, cx| app.apply_plugin_action(action, cx));
            })
            .into_any_element()
    }

    fn empty_resource_card(
        &self,
        theme: Theme,
        icon_path: &'static str,
        title: &str,
        body: &str,
    ) -> AnyElement {
        div()
            .w_full()
            .board_surface(&theme, theme.surface_secondary)
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base40.px(&theme))
            .flex()
            .flex_col()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(empty_state_glyph(theme, icon_path))
            .child(
                div()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(title.to_string()),
            )
            .child(
                div()
                    .max_w(px(420.))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(body.to_string()),
            )
            .into_any_element()
    }

    /// The sticky provider header: the search field plus the
    /// count / Refresh / Add toolbar. Rendered outside the scroll container
    /// so it stays put while the grid scrolls (gpui 0.2.2 has no
    /// `position: sticky`).
    pub(super) fn provider_toolbar(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> AnyElement {
        let search = search_field(self.provider_filter.clone(), &theme).w_full();

        let refresh_icon = refresh_glyph(
            "providers-refresh-spin",
            ButtonSize::Large.icon_size().px(&theme),
            self.providers_refreshing,
            theme.text_secondary,
            theme,
        );
        let refresh_button = Button::keyed("providers-refresh", ButtonVariant::Secondary)
.size(ButtonSize::Large)
.faded(&theme, |button| button
                .when(self.providers_refreshing, |button| button.opacity(0.6))
                .on_mouse_up(MouseButton::Left, {
                    let this = this.clone();
                    move |_, _, cx| {
                        this.update(cx, |app, cx| app.provider_refresh(cx));
                    }
                })
                .child(refresh_icon)
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary)
                        .child(tr!("settings.refresh")),
                ));

        let add_button = Button::new("providers-add", ButtonVariant::Primary)
            .size(ButtonSize::Large)
            .icon("icons/plus.svg")
            .label(tr!("settings.add_provider"))
            .faded(&theme, |button| {
                let this = this.clone();
                button.on_mouse_up(MouseButton::Left, move |_, window, cx| {
                    this.update(cx, |app, cx| app.provider_editor_open(None, window, cx));
                })
            });

        // Search and the page's actions share one row; on a narrow window
        // the buttons wrap under the field instead of clipping.
        let controls = div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(div().flex_1().min_w(px(240.)).child(search))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(self.models_favorites_button(theme, this.clone()))
                    .child(self.models_update_button(theme, this.clone()))
                    .child(refresh_button)
                    .child(add_button),
            );

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .debug_selector(|| "providers-toolbar".to_string())
            .child(controls)
            .into_any_element()
    }

    /// The Providers page: every provider pi ships with (plus models.json-only
    /// endpoints) as a 3-column grid, each with real auth status and actions.
    pub(super) fn provider_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let all = self.provider_views();
        let needle = self.provider_filter.read(cx).text().trim().to_lowercase();
        let favorites = crate::favorites::all();
        let favorites_only = self.models_favorites_only;
        let provider_hit = |view: &ProviderView| {
            needle.is_empty()
                || view.name.to_lowercase().contains(&needle)
                || view.id.to_lowercase().contains(&needle)
                || view
                    .env_names
                    .iter()
                    .any(|name| name.to_lowercase().contains(&needle))
        };
        // A provider's models on this page: all of them when the search names
        // the provider, else the ones the search names; starred ones only
        // under the favorites filter.
        let models_of = |view: &ProviderView| -> Vec<&ModelEntry> {
            let model_needle = if provider_hit(view) {
                ""
            } else {
                needle.as_str()
            };
            self.available_models
                .iter()
                .filter(|model| {
                    model_visible(
                        model,
                        model_needle,
                        favorites_only,
                        &favorites,
                        Some(&view.id),
                    )
                })
                .collect()
        };

        let mut views: Vec<&ProviderView> = all
            .iter()
            .filter(|view| {
                if favorites_only {
                    !models_of(view).is_empty()
                } else {
                    provider_hit(view) || !models_of(view).is_empty()
                }
            })
            .collect();
        let rank = |view: &ProviderView| -> u8 {
            if view.active {
                0
            } else if view.connected() {
                1
            } else if view.custom {
                2
            } else {
                3
            }
        };
        // An account sits beside its provider: same group, then provider
        // name, then the provider before its accounts.
        let base_name = |view: &ProviderView| -> String {
            match crate::provider_accounts::split_alias(&view.id) {
                Some((base, _)) => providers::provider_display_name(base).to_lowercase(),
                None => view.name.to_lowercase(),
            }
        };
        let is_account =
            |view: &ProviderView| crate::provider_accounts::split_alias(&view.id).is_some();
        views.sort_by(|a, b| {
            rank(a)
                .cmp(&rank(b))
                .then_with(|| base_name(a).cmp(&base_name(b)))
                .then_with(|| is_account(a).cmp(&is_account(b)))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        let mut rows: Vec<AnyElement> = Vec::new();

        // ── remote session: which machine each control acts on ──
        if let Some(host) = self.remote_host_name() {
            rows.push(
                div()
                    .debug_selector(|| "providers-remote-notice".to_string())
                    .w_full()
                    .border_1()
                    .border_color(theme.hairline())
                    .rounded(panel::RADIUS.px(&theme))
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .items_start()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(icon(
                        "icons/server-stack.svg",
                        IconSize::Medium.px(&theme),
                        theme.text_muted,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(tr!(
                                "remote_hosts.providers_notice_local",
                                host = host.to_string()
                            )),
                    )
                    .into_any_element(),
            );
        }

        // ── credential-changed banner ──
        if self.provider_auth_dirty {
            rows.push(
                div()
                    .w_full()
                    .bg(theme.accent.opacity(0.1))
                    .border_1()
                    .border_color(theme.accent.opacity(0.35))
                    .rounded(panel::RADIUS.px(&theme))
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(icon(
                        "icons/info.svg",
                        IconSize::Medium.px(&theme),
                        theme.accent_text,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(DynamicSpacing::Base02.px(&theme))
                            .child(
                                div()
                                    .text_size(TextSize::Small.px(&theme))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .child(tr!("settings.credentials_changed")),
                            )
                            .child(
                                div()
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_secondary)
                                    .child(tr!("settings.restart_pi_credentials_hint")),
                            ),
                    )
                    .child(self.provider_button(
                        "providers-apply".into(),
                        &tr!("settings.restart_pi"),
                        ButtonVariant::Primary,
                        theme,
                        this.clone(),
                        ProviderAction::Restart,
                    ))
                    .into_any_element(),
            );
        }

        if let Some(error) = &self.custom_providers_error {
            rows.push(self.provider_error_card(
                theme,
                &tr!("settings.models_json_read_error"),
                error,
            ));
        }
        if let Some(error) = &self.provider_auth_error {
            rows.push(self.provider_error_card(
                theme,
                &tr!("settings.auth_json_read_error"),
                error,
            ));
        }

        // The default remains editable even when the catalog or search is empty.
        rows.push(self.default_model_card(theme, this.clone(), cx));

        // ── providers ──
        if views.is_empty() {
            rows.push(self.empty_resource_card(
                theme,
                "icons/search.svg",
                &tr!("settings.no_providers_match"),
                &tr!("settings.try_a_different_search"),
            ));
        } else {
            // The current selection leads, then one board for the providers
            // in use and another for the rest (#131). Each provider is a row
            // on the board — the grouped-surface pattern every settings page
            // uses — and its models open beneath it, so the page reads as one
            // list instead of a wall of uneven cards.
            let (connected, other): (Vec<&ProviderView>, Vec<&ProviderView>) = views
                .into_iter()
                .partition(|view| view.active || view.connected());
            for (label, group) in [
                (tr!("settings.connected_providers"), connected),
                (tr!("settings.not_connected_providers"), other),
            ] {
                if group.is_empty() {
                    continue;
                }
                let count = group.len();
                let board: Vec<AnyElement> = group
                    .into_iter()
                    .map(|view| {
                        let models = models_of(view);
                        // A filter narrowed this provider's list when the
                        // favorites filter is on, or the search matched its
                        // models rather than the provider itself.
                        let narrowed =
                            favorites_only || (!needle.is_empty() && !provider_hit(view));
                        self.provider_row(view, &models, narrowed, theme, this.clone(), cx)
                    })
                    .collect();
                rows.push(
                    div()
                        .w_full()
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(self.provider_section_label(label, count, theme))
                        .child(self.settings_group(theme, board))
                        .into_any_element(),
                );
            }
        }

        rows
    }

    /// A red note card for a config read error.
    pub(super) fn provider_error_card(&self, theme: Theme, title: &str, error: &str) -> AnyElement {
        error_state(
            theme,
            title,
            Some(error),
            Some(&tr!(
                "settings.fix_or_remove_the_file_before_editing_providers_"
            )),
            None,
        )
    }

    /// Every provider pi ships with, merged with the live catalog, models.json,
    /// and auth.json. Built-ins come first so nothing is hidden behind auth.
    pub(super) fn provider_views(&self) -> Vec<ProviderView> {
        let mut views: Vec<ProviderView> = Vec::new();
        // Prefer pi's own provider metadata (introspected from pi-ai). Fall
        // back to the curated table when node/pi-ai aren't reachable.
        if !self.provider_metadata.is_empty() {
            for provider in &self.provider_metadata {
                views.push(self.provider_view_for(
                    &provider.id,
                    Some(provider.name.clone()),
                    provider.oauth,
                    provider.api_key,
                    &provider.env_vars,
                    providers::provider_note(&provider.id),
                ));
            }
        } else {
            for builtin in providers::BUILTIN_PROVIDERS {
                let env_vars: Vec<String> = if builtin.env_var.is_empty() {
                    Vec::new()
                } else {
                    vec![builtin.env_var.to_string()]
                };
                views.push(self.provider_view_for(
                    builtin.id,
                    Some(builtin.name.to_string()),
                    builtin.oauth,
                    builtin.api_key,
                    &env_vars,
                    builtin.note,
                ));
            }
        }
        // Providers pi ships but neither source knows yet — discovered from
        // pi's bundled catalog data, so a new pi release lists its providers
        // without an Orbit change.
        for id in self.provider_catalog_counts.keys() {
            if !views.iter().any(|view| &view.id == id) {
                views.push(self.provider_view_for(
                    id,
                    None,
                    false,
                    true,
                    &[],
                    providers::provider_note(id),
                ));
            }
        }
        // Catalog providers that aren't built-ins (extension providers, or
        // custom endpoints pi has already loaded).
        for model in &self.available_models {
            if !views.iter().any(|view| view.id == model.provider) {
                views.push(self.provider_view_for(&model.provider, None, false, true, &[], ""));
            }
        }
        // models.json providers pi hasn't loaded yet.
        for provider in &self.custom_providers {
            if !views.iter().any(|view| view.id == provider.id) {
                views.push(self.provider_view_for(&provider.id, None, false, true, &[], ""));
            }
        }
        // Providers the running pi advertises through `auth.list` but no other
        // source knows about still get a card and capability-driven buttons.
        for capability in self.auth.providers() {
            if !views.iter().any(|view| view.id == capability.id) {
                views.push(self.provider_view_for(
                    &capability.id,
                    Some(capability.name.clone()),
                    capability.supports_oauth(),
                    capability.supports_api_key(),
                    &[],
                    providers::provider_note(&capability.id),
                ));
            }
        }
        // Extra accounts pi has not reported yet still get a row to sign in on.
        let accounts = self.account_views(&views);
        views.extend(accounts);
        views
    }

    /// Build one grid row from all sources.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn provider_view_for(
        &self,
        id: &str,
        name: Option<String>,
        oauth: bool,
        api_key: bool,
        env_names: &[String],
        note: &'static str,
    ) -> ProviderView {
        let custom = self
            .custom_providers
            .iter()
            .find(|provider| provider.id == id);
        let live_count = self
            .available_models
            .iter()
            .filter(|model| model.provider == id)
            .count();
        let catalog_count = self.provider_catalog_counts.get(id).copied().unwrap_or(0);
        let custom_count = custom.map(|provider| provider.model_ids.len()).unwrap_or(0);
        let env_hit = env_names
            .iter()
            .find(|name| std::env::var_os(name.as_str()).is_some())
            .cloned();
        ProviderView {
            id: id.to_string(),
            name: custom
                .and_then(|provider| provider.name.clone())
                .or(name)
                .unwrap_or_else(|| providers::provider_display_name(id)),
            active: live_count > 0,
            model_count: if live_count > 0 {
                live_count
            } else if catalog_count > 0 {
                catalog_count
            } else {
                custom_count
            },
            catalog_count,
            custom: custom.is_some(),
            has_api_key: custom.is_some_and(|provider| provider.has_api_key),
            base_url: custom
                .map(|provider| provider.base_url.clone())
                .unwrap_or_default(),
            api: custom
                .map(|provider| provider.api.clone())
                .unwrap_or_default(),
            oauth,
            api_key,
            env_names: env_names.to_vec(),
            env_authed: env_hit.is_some(),
            env_var: env_hit,
            note,
            auth: self.provider_auth.get(id).copied(),
            live_status: self.auth.status(id).cloned(),
            quota: self.quota.report(id).cloned(),
        }
    }

    /// The live auth block shown on a card while a login is Connecting,
    /// waiting on a device code, or showing a Success/Error/Cancelled result.
    /// `None` means the provider has no session and the card renders its
    /// normal Connect / Disconnect actions.
    pub(super) fn provider_auth_section(
        &self,
        view: &ProviderView,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> Option<AnyElement> {
        let session = self.auth.login_for(&view.id)?;
        let (headline, detail, tint) = match session.phase {
            LoginPhase::Connecting => (
                tr!("status.connecting"),
                tr!("auth.asking_pi"),
                theme.warning,
            ),
            LoginPhase::AwaitingBrowser => (
                tr!("auth.waiting_browser"),
                tr!("auth.finish_in_browser"),
                theme.accent_text,
            ),
            LoginPhase::AwaitingDeviceCode => (
                tr!("auth.enter_device_code"),
                tr!("auth.approve_in_browser"),
                theme.accent_text,
            ),
            LoginPhase::Succeeded => (
                tr!("status.connected"),
                tr!("auth.credential_saved"),
                theme.success,
            ),
            LoginPhase::Error => (
                tr!("status.sign_in_failed"),
                session
                    .error
                    .as_ref()
                    .map(|(_, message)| message.clone())
                    .filter(|message| !message.is_empty())
                    .unwrap_or_else(|| tr!("auth.sign_in_incomplete")),
                theme.error,
            ),
            LoginPhase::Cancelled => (
                tr!("status.cancelled"),
                session
                    .message
                    .clone()
                    .unwrap_or_else(|| tr!("auth.sign_in_cancelled")),
                theme.text_muted,
            ),
        };

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(div().size(px(7.)).rounded_full().bg(tint))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(tint)
                            .child(headline),
                    ),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(
                        session
                            .prompt
                            .as_ref()
                            .map(|prompt| prompt.message.clone())
                            .unwrap_or(detail),
                    ),
            );

        if let Some(prompt) = &session.prompt {
            body = body.child(super::auth_prompt::AuthPromptView {
                app: this.clone(),
                request_id: prompt.request_id.clone(),
            });
        }
        if let Some(device) = &session.device_code {
            body = body.child(
                div()
                    .w_full()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .rounded(panel::RADIUS.px(&theme))
                    .border_1()
                    .border_color(theme.hairline())
                    // A well inside a frosted card: tinted, not opaque.
                    .bg(theme.card_fill(theme.background))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::XSmall.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!("settings.device_code")),
                    )
                    .child(
                        div()
                            .font_family(theme::code_font_family())
                            .text_size(theme.code_px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_primary)
                            .child(device.user_code.clone()),
                    )
                    .child(
                        div()
                            .font_family(theme::code_font_family())
                            .text_size(theme.code_px(10.5))
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(device.verification_uri.clone()),
                    ),
            );
        }
        if session.device_code.is_none() {
            if let Some(url) = &session.url {
                body = body.child(
                    div()
                        .truncate()
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(10.5))
                        .text_color(theme.text_muted)
                        .child(url.clone()),
                );
            }
        }

        let id = view.id.clone();
        let name = view.name.clone();
        let method = session.method.clone();
        let mut buttons = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme));
        match session.phase {
            LoginPhase::Connecting
            | LoginPhase::AwaitingBrowser
            | LoginPhase::AwaitingDeviceCode => {
                if let Some(device) = &session.device_code {
                    let open = device
                        .verification_uri_complete
                        .clone()
                        .unwrap_or_else(|| device.verification_uri.clone());
                    buttons = buttons
                        .child(self.provider_button(
                            format!("provider-auth-open-{}", view.id),
                            &tr!("auth.open_page"),
                            ButtonVariant::Primary,
                            theme,
                            this.clone(),
                            ProviderAction::AuthOpenUrl(open),
                        ))
                        .child(self.provider_button(
                            format!("provider-auth-copy-{}", view.id),
                            &tr!("auth.copy_code"),
                            ButtonVariant::Secondary,
                            theme,
                            this.clone(),
                            ProviderAction::AuthCopy(device.user_code.clone()),
                        ));
                }
                buttons = buttons.child(self.provider_button(
                    format!("provider-auth-cancel-{}", view.id),
                    &tr!("git_panel.cancel"),
                    ButtonVariant::Secondary,
                    theme,
                    this.clone(),
                    ProviderAction::AuthCancel,
                ));
            }
            LoginPhase::Succeeded => {
                buttons = buttons.child(self.provider_button(
                    format!("provider-auth-done-{}", view.id),
                    &tr!("common.done"),
                    ButtonVariant::Primary,
                    theme,
                    this.clone(),
                    ProviderAction::AuthDismiss,
                ));
            }
            LoginPhase::Error | LoginPhase::Cancelled => {
                buttons = buttons
                    .child(self.provider_button(
                        format!("provider-auth-retry-{}", view.id),
                        &tr!("view.try_again"),
                        ButtonVariant::Primary,
                        theme,
                        this.clone(),
                        ProviderAction::AuthStart {
                            id: id.clone(),
                            name: name.clone(),
                            method: method.clone(),
                        },
                    ))
                    .child(self.provider_button(
                        format!("provider-auth-dismiss-{}", view.id),
                        "Dismiss",
                        ButtonVariant::Secondary,
                        theme,
                        this.clone(),
                        ProviderAction::AuthDismiss,
                    ));
            }
        }
        body = body.child(buttons);
        Some(body.into_any_element())
    }

    /// Account quota/balance/spend for a connected provider, from the
    /// `quota.*` RPC namespace. `None` when there is nothing to render, so a
    /// provider with no usage surface adds no empty block to its card.
    pub(super) fn provider_quota_section(
        &self,
        view: &ProviderView,
        theme: Theme,
    ) -> Option<AnyElement> {
        let report = view.quota.as_ref()?;
        if !report.has_data() && report.error.is_none() && report.note.is_none() {
            return None;
        }

        let amount = |value: f64| -> String {
            if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                format!("{value:.2}")
            }
        };
        let reset_label = |ms: i64| tr!("session.resets_at", time = format_epoch_ms(ms));

        let mut head = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_muted)
                    .child(tr!("settings.usage")),
            );
        if let Some(plan) = &report.plan {
            head = head.child(self.provider_badge(
                plan,
                theme.accent_text,
                theme.accent.opacity(0.12),
                theme,
            ));
        }

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base08.px(&theme))
            .rounded(panel::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.hairline())
            // A well inside a frosted card: tinted, not opaque.
            .bg(theme.card_fill(theme.background))
            .child(head);

        for window in &report.windows {
            let value = if let Some(percent) = window.used_percent {
                tr!("session.percent_used", percent = format!("{percent:.0}"))
            } else if let (Some(used), Some(limit)) = (window.used, window.limit) {
                tr!(
                    "session.used_of_limit",
                    used = amount(used),
                    limit = amount(limit)
                )
            } else if let Some(used) = window.used {
                match &window.unit {
                    Some(unit) => tr!("session.amount_unit", amount = amount(used), unit = unit),
                    None => amount(used),
                }
            } else {
                continue;
            };

            let mut row = div()
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base04.px(&theme))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(DynamicSpacing::Base08.px(&theme))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_secondary)
                                .child(window.label.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_primary)
                                .child(value),
                        ),
                );

            if let Some(fraction) = window.fraction() {
                let tint = if fraction >= 0.9 {
                    theme.error
                } else if fraction >= 0.75 {
                    theme.warning
                } else {
                    theme.success
                };
                row = row.child(progress_bar(
                    &theme,
                    fraction,
                    tint,
                    ProgressSize::Small,
                    ProgressTrack::Glass,
                    None,
                ));
            }
            if let Some(resets_at) = window.resets_at {
                row = row.child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(reset_label(resets_at)),
                );
            }
            body = body.child(row);
        }

        for balance in &report.balances {
            let text = if balance.currency.is_empty() {
                amount(balance.amount)
            } else {
                format!("{} {}", amount(balance.amount), balance.currency)
            };
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(balance.label.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_primary)
                            .child(text),
                    ),
            );
        }

        if let Some(error) = &report.error {
            body = body.child(
                div()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.error)
                    .child(error.to_string()),
            );
        } else if note_should_render(report) {
            if let Some(note) = &report.note {
                body = body.child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(note.to_string()),
                );
            }
        }

        Some(body.into_any_element())
    }

    /// One provider as a row on its group's board.
    ///
    /// The head is one calm line: brand mark, name and status over a single
    /// line of facts, and on the right only the model count, the one action
    /// that connects an unconnected provider, and the disclosure chevron.
    /// Everything else — every sign-in method, the editor, usage, disconnect,
    /// remove — lives in the panel that opens beneath the head, where a
    /// full-width action bar can wrap without spilling over the next row,
    /// followed by the provider's models.
    ///
    /// `models` is what this page shows for the provider (already narrowed by
    /// the search and the favorites filter); `narrowed` says a filter did the
    /// narrowing, which opens the panel so the match is visible.
    pub(super) fn provider_row(
        &self,
        view: &ProviderView,
        models: &[&ModelEntry],
        narrowed: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        // Usage opens in a popup (see `provider_usage_layer`).
        let has_usage = view.quota.as_ref().is_some_and(|report| {
            report.has_data() || report.error.is_some() || report.note.is_some()
        });
        let session = self.auth.login_for(&view.id);
        let (status_label, status_color) = if let Some(session) = session {
            match session.phase {
                LoginPhase::Connecting => (tr!("status.connecting"), theme.warning),
                LoginPhase::AwaitingBrowser | LoginPhase::AwaitingDeviceCode => {
                    (tr!("status.connecting"), theme.accent_text)
                }
                LoginPhase::Succeeded => (tr!("status.connected"), theme.success),
                LoginPhase::Error => (tr!("status.sign_in_failed"), theme.error),
                LoginPhase::Cancelled => (tr!("status.cancelled"), theme.text_muted),
            }
        } else if view.active {
            (tr!("status.active"), theme.success)
        } else if view.connected() {
            (tr!("status.connected"), theme.accent_text)
        } else if view.custom {
            (tr!("status.not_loaded"), theme.warning)
        } else {
            (tr!("status.not_configured"), theme.text_muted)
        };
        let lit = view.active || view.connected();
        let connected = view.auth.is_some()
            || view
                .live_status
                .as_ref()
                .is_some_and(|status| status.authenticated);
        let auth_section = self.provider_auth_section(view, theme, this.clone());

        // ── identity: mark, name, status, facts ──
        let tile = div()
            .size(ButtonSize::Large.height(&theme) + DynamicSpacing::Base04.px(&theme))
            .flex_none()
            .rounded(Radius::Large.px(&theme))
            .bg(theme.control_face(theme.surface_elevated))
            .border_1()
            .border_color(theme.hairline_subtle())
            .flex()
            .items_center()
            .justify_center()
            .child(icon_dyn(
                provider_icon(&view.id),
                IconSize::Medium.px(&theme) + DynamicSpacing::Base04.px(&theme),
                provider_mark_color(
                    &view.id,
                    &theme,
                    if lit {
                        theme.text_primary
                    } else {
                        theme.text_secondary
                    },
                ),
            ));

        // The status reads as a word; the dot only reinforces it.
        let status = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base04.px(&theme))
            .child(div().size(px(6.)).rounded_full().bg(status_color))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(status_label),
            );

        let mut title = div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .min_w_0()
                    .text_size(TextSize::Default.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .truncate()
                    .child(view.name.clone()),
            )
            .child(status);
        if view.custom {
            title = title.child(self.provider_badge(
                &tr!("settings.provider_custom"),
                theme.accent_text,
                theme.accent.opacity(0.12),
                theme,
            ));
        }

        // One quiet line: id, catalog size (when no model count states it),
        // how it is signed in, and where it points.
        let mut facts: Vec<String> = vec![view.id.clone()];
        // A provider with extra accounts says how many; each is its own row.
        let accounts = self
            .account_profile_map
            .get(&view.id)
            .map(Vec::len)
            .unwrap_or(0);
        if accounts > 0 && self.remote.is_none() {
            facts.push(tr!("accounts.n_more", count = accounts));
        }
        if models.is_empty() {
            if !view.active && view.catalog_count > 0 {
                facts.push(tr!("settings.n_in_catalog", count = view.catalog_count));
            } else if view.model_count > 0 {
                facts.push(tr!("settings.n_models", count = view.model_count));
            }
        }
        if let Some(kind) = view.credential_kind() {
            facts.push(match kind {
                "oauth" => tr!("settings.cred_oauth"),
                "api_key" => tr!("settings.cred_api_key"),
                other => other.to_string(),
            });
            if let Some(status) = &view.live_status {
                if let Some(account) = &status.account {
                    facts.push(account.clone());
                }
                if let Some(expires_at) = status.expires_at {
                    facts.push(tr!(
                        "settings.expires_at",
                        time = format_epoch_ms(expires_at)
                    ));
                }
            }
        } else if let Some(env_var) = &view.env_var {
            facts.push(tr!("settings.via_env_var", env_var = env_var));
        }
        if view.has_api_key {
            facts.push(tr!("settings.models_json_key"));
        }
        if !view.base_url.is_empty() {
            facts.push(if view.api.is_empty() {
                view.base_url.clone()
            } else {
                format!("{} · {}", view.base_url, view.api)
            });
        } else if !view.note.is_empty() {
            facts.push(providers::localize_note(view.note));
        }

        // gpui 0.2.2 caches a truncated text's first measurement for the
        // frame. As a block in a column, that first pass hands the text a
        // zero width (the column's flex basis), so the whole line painted as a
        // bare "…". Inside a flex row the text is first measured at its full
        // width, like the name above it; the line then clips at the column.
        let identity = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(tile)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(title)
                    .child(
                        div()
                            .debug_selector({
                                let id = view.id.clone();
                                move || format!("provider-facts-{id}")
                            })
                            // A row, not a block: see the note above.
                            .flex()
                            .min_w_0()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .truncate()
                            .child(facts.join(" · ")),
                    ),
            );

        // ── actions ──
        // Methods come from pi's `auth.list` capability discovery — the UI
        // never hardcodes provider ids or assumes a login flow. Sign-in
        // actions are collected first so the head can lift the first one out
        // as the row's call to action while the provider is not connected.
        let capability = if self.auth.support() == AuthSupport::Supported {
            self.auth.provider(&view.id)
        } else {
            None
        };
        // Every way to connect is a secondary button: the page lists dozens
        // of unconnected providers, and a filled button on each one made the
        // list a wall of calls to action. `lead` still decides which method
        // is lifted into the row head.
        let style = |_lead: bool| ButtonVariant::Secondary;
        let edit_key = |primary: bool| {
            let update = view.credential_kind() == Some("api_key");
            self.provider_button(
                format!("provider-key-{}", view.id),
                &if update {
                    tr!("settings.update_key")
                } else {
                    tr!("settings.add_api_key")
                },
                style(primary),
                theme,
                this.clone(),
                ProviderAction::EditKey {
                    id: view.id.clone(),
                    name: view.name.clone(),
                    oauth: view.oauth,
                    note: view.note,
                },
            )
        };
        let mut sign_in: Vec<AnyElement> = Vec::new();
        let mut no_methods = false;
        if let Some(capability) = capability {
            // A subscription sign-in leads; the API key follows it.
            let mut methods: Vec<_> = capability.methods.iter().collect();
            methods.sort_by_key(|method| method.id == "api_key");
            for method in methods {
                let primary = !connected && sign_in.is_empty();
                if method.id == "api_key" {
                    sign_in.push(edit_key(primary));
                } else {
                    let label = method_label(&method.id, &method.label, view.connected());
                    sign_in.push(self.provider_button(
                        format!("provider-auth-{}-{}", method.id, view.id),
                        &label,
                        style(primary),
                        theme,
                        this.clone(),
                        ProviderAction::AuthStart {
                            id: view.id.clone(),
                            name: view.name.clone(),
                            method: method.id.clone(),
                        },
                    ));
                }
            }
            no_methods = capability.methods.is_empty();
        } else {
            // File-based fallback for a pi without the auth RPC.
            if view.oauth {
                let reconnect = view.credential_kind() == Some("oauth");
                sign_in.push(self.provider_button(
                    format!("provider-signin-{}", view.id),
                    &if reconnect {
                        tr!("settings.reconnect")
                    } else {
                        tr!("settings.sign_in")
                    },
                    style(!connected),
                    theme,
                    this.clone(),
                    ProviderAction::SignIn {
                        id: view.id.clone(),
                        name: view.name.clone(),
                    },
                ));
            }
            if view.api_key {
                sign_in.push(edit_key(!connected && sign_in.is_empty()));
            }
        }
        // The head's call to action: the first way to connect, only while
        // nothing is connected and no sign-in is running.
        let cta = (!connected && auth_section.is_none() && !sign_in.is_empty())
            .then(|| sign_in.remove(0));

        let mut bar: Vec<AnyElement> = Vec::new();
        if auth_section.is_none() {
            // Every provider keeps the way into its editor — a built-in with
            // no models.json entry included (#131). "Configure" promises an
            // override; "Models" promises the ids/endpoint list.
            bar.push(self.provider_button(
                format!("provider-configure-{}", view.id),
                &if view.custom {
                    tr!("settings.configure")
                } else {
                    tr!("settings.models")
                },
                ButtonVariant::Secondary,
                theme,
                this.clone(),
                ProviderAction::Configure {
                    id: view.id.clone(),
                },
            ));
            if has_usage {
                bar.push(self.provider_button(
                    format!("provider-usage-{}", view.id),
                    &tr!("session.usage"),
                    ButtonVariant::Secondary,
                    theme,
                    this.clone(),
                    ProviderAction::ShowUsage {
                        id: view.id.clone(),
                    },
                ));
            }
            bar.extend(sign_in);
            if no_methods {
                bar.push(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("settings.no_sign_in_methods_available"))
                        .into_any_element(),
                );
            }
            if connected {
                bar.push(self.provider_button(
                    format!("provider-signout-{}", view.id),
                    &tr!("settings.disconnect"),
                    ButtonVariant::Secondary,
                    theme,
                    this.clone(),
                    ProviderAction::SignOut {
                        id: view.id.clone(),
                    },
                ));
            }
            if view.custom {
                bar.push(self.provider_button(
                    format!("provider-remove-{}", view.id),
                    &tr!("settings.remove"),
                    ButtonVariant::Danger,
                    theme,
                    this.clone(),
                    ProviderAction::Remove {
                        id: view.id.clone(),
                        name: view.name.clone(),
                    },
                ));
            }
        }

        // ── head ──
        // Open by default for the provider the session's model comes from
        // (`view.active` only means pi serves models for it, which is true of
        // every provider with a list here), while a filter narrowed the list,
        // and while a sign-in runs.
        let open = self
            .provider_models_open
            .get(&view.id)
            .copied()
            .unwrap_or(narrowed || view.id == self.model_provider)
            || auth_section.is_some();

        let mut trailing = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(&theme));
        if !models.is_empty() {
            trailing = trailing.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("settings.n_models", count = models.len())),
            );
        }
        trailing = trailing.children(cta).child(disclosure_chevron(
            format!("provider-row-chevron-{}", view.id),
            open,
            IconSize::Small.px(&theme),
            theme.text_muted,
            &theme,
        ));

        let head_id = format!("provider-models-toggle-{}", view.id);
        let head = div()
            .id(ElementId::Name(head_id.clone().into()))
            .debug_selector(move || head_id.clone())
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base16.px(&theme))
            .cursor_pointer()
            .tip(if open {
                tr!("settings.hide_provider_details")
            } else {
                tr!("settings.show_provider_details")
            })
            .on_mouse_up(MouseButton::Left, {
                let this = this.clone();
                let id = view.id.clone();
                move |_, _, cx| {
                    let id = id.clone();
                    this.update(cx, |app, cx| app.toggle_provider_models(id, open, cx));
                }
            })
            .child(identity)
            .child(trailing);

        // ── panel ──
        // Indented to the text column, so the panel reads as the row's own.
        let indent = DynamicSpacing::Base16.px(&theme)
            + ButtonSize::Large.height(&theme)
            + DynamicSpacing::Base04.px(&theme)
            + DynamicSpacing::Base12.px(&theme);
        let accounts_section = self.provider_accounts_section(view, theme, this.clone());
        let panel = open.then(|| {
            div()
                .w_full()
                .pl(indent)
                .pr(DynamicSpacing::Base16.px(&theme))
                .pb(DynamicSpacing::Base16.px(&theme))
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base12.px(&theme))
                .children(accounts_section)
                .when(!bar.is_empty(), |panel| {
                    panel.child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(DynamicSpacing::Base06.px(&theme))
                            .children(bar),
                    )
                })
                .children(auth_section)
                .when(!models.is_empty(), |panel| {
                    panel.child(self.provider_model_list(&view.id, models, theme, this.clone(), cx))
                })
        });

        div()
            .debug_selector({
                let id = view.id.clone();
                move || format!("provider-row-{id}")
            })
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .child(head)
            .children(panel)
            .into_any_element()
    }

    /// Open or close a provider row's model list by hand.
    pub(super) fn toggle_provider_models(
        &mut self,
        id: String,
        currently_open: bool,
        cx: &mut Context<Self>,
    ) {
        self.provider_models_open.insert(id, !currently_open);
        cx.notify();
    }

    /// A provider's models as click-to-select chips, in a well under its row
    /// (#131).
    ///
    /// A chip is *selected* when the composer's picker offers that model; a
    /// click writes the hide-list ([`crate::hidden_models`]) and never deletes
    /// anything — the provider's catalog (pi's, or `models.json`) is what it is.
    /// The context window is shown only when pi reports one. The list is
    /// height-bounded and scrolls on its own so a long catalog never buries
    /// the page.
    fn provider_model_list(
        &self,
        provider: &str,
        models: &[&ModelEntry],
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let filter = self
            .provider_model_filters
            .get(provider)
            .map(|(input, _)| input);
        let needle = filter
            .map(|input| input.read(cx).text().trim().to_lowercase())
            .unwrap_or_default();
        let matches: Vec<_> = models
            .iter()
            .copied()
            .filter(|model| {
                model.id.to_lowercase().contains(&needle)
                    || model.name.to_lowercase().contains(&needle)
            })
            .collect();
        // Visibility counts and bulk actions retain their section-wide scope;
        // typing a local search never silently changes what "all" means.
        let hidden = crate::hidden_models::all();
        let shown = models
            .iter()
            .filter(|model| !hidden.contains(provider, &model.id))
            .count();
        let total = models.len();
        let all_hidden = shown == 0;

        let chips = div()
            .w_full()
            .grid()
            .grid_cols(2)
            .gap(DynamicSpacing::Base04.px(&theme))
            .children(
                matches
                    .iter()
                    .map(|model| self.provider_model_chip(model, &hidden, theme, this.clone())),
            );
        let scroll = div()
            .id(ElementId::Name(
                format!("provider-models-scroll-{provider}").into(),
            ))
            .debug_selector({
                let provider = provider.to_string();
                move || format!("provider-models-scroll-{provider}")
            })
            .w_full()
            .max_h(px(240.))
            .overflow_y_scroll()
            // GPUI's overflow handler does not consume the wheel event; keep
            // it from scrolling the settings page as well, even at the ends.
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .child(chips);

        let ids: Vec<(String, String)> = models
            .iter()
            .map(|model| (model.provider.clone(), model.id.clone()))
            .collect();
        let bulk = Button::new(
            format!("provider-models-visibility-{provider}"),
            ButtonVariant::Ghost,
        )
        .label(if all_hidden {
            tr!("settings.show_all_models")
        } else {
            tr!("settings.hide_all_models")
        })
        .on_mouse_up(&theme, move |_, _, cx| {
            for (provider, id) in &ids {
                crate::hidden_models::set(provider, id, !all_hidden);
            }
            this.update(cx, |_, cx| cx.notify());
        })
        .flex_none();

        div()
            .debug_selector({
                let provider = provider.to_string();
                move || format!("provider-models-{provider}")
            })
            .w_full()
            .child(
                div()
                    .w_full()
                    .rounded(Radius::Large.px(&theme))
                    .bg(theme.card_fill(theme.surface))
                    .border_1()
                    .border_color(theme.hairline_subtle())
                    .p(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .child(
                        div()
                            .w_full()
                            .pl(DynamicSpacing::Base04.px(&theme))
                            .flex()
                            .items_center()
                            .gap(DynamicSpacing::Base08.px(&theme))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(TextSize::Small.px(&theme))
                                    .text_color(theme.text_muted)
                                    .child(tr!(
                                        "settings.n_models_shown",
                                        shown = shown,
                                        total = total
                                    )),
                            )
                            .child(bulk),
                    )
                    .children(filter.map(|input| {
                        search_field(input.clone(), &theme)
                            .w_full()
                            .debug_selector({
                                let provider = provider.to_string();
                                move || format!("provider-model-search-{provider}")
                            })
                    }))
                    .when(matches.is_empty(), |panel| {
                        panel.child(
                            div()
                                .text_size(TextSize::Small.px(&theme))
                                .text_color(theme.text_muted)
                                .child(tr!("settings.no_models_match")),
                        )
                    })
                    .child(scroll),
            )
            .into_any_element()
    }

    /// One model chip in a provider's list: a check when the composer's
    /// picker offers it, the id, and its context window when pi reports one.
    /// The whole chip is the toggle — there is no second control to find.
    fn provider_model_chip(
        &self,
        model: &ModelEntry,
        hidden: &crate::hidden_models::HiddenModels,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let provider = model.provider.clone();
        let id = model.id.clone();
        let label = id.clone();
        let shown = !hidden.contains(&provider, &id);
        let chip_id = format!("provider-model-chip-{provider}-{id}");
        // Bespoke: a full-width checkbox row — a row family, not a chip; its
        // check stays explicit when hidden, which a ChipButton check (on-only)
        // would not.
        let mut chip = button_frame(
            div()
                .id(ElementId::Name(chip_id.clone().into()))
                .debug_selector({
                    let chip_id = chip_id.clone();
                    move || chip_id.clone()
                }),
            &theme,
            ButtonSize::Medium,
        )
        .w_full()
        .min_w_0()
        .justify_start()
        .text_color(if shown {
            theme.text_primary
        } else {
            theme.text_muted
        })
        .cursor_pointer()
        .hover_state(&theme)
        .tip(format!(
            "{} — {}",
            label,
            if shown {
                tr!("settings.hide_from_model_picker")
            } else {
                tr!("settings.show_in_model_picker")
            }
        ))
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            crate::hidden_models::set(&provider, &id, shown);
            this.update(cx, |_, cx| cx.notify());
        })
        .child(check_box(shown, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .font_family(theme::code_font_family())
                .text_size(theme.code_px(11.5))
                .child(label),
        );
        if let Some(context) = model.context_window {
            chip = chip.child(
                div()
                    .flex_none()
                    .text_color(theme.text_muted)
                    .text_size(TextSize::XSmall.px(&theme))
                    .font(crate::usage::view::num_font())
                    .child(crate::context_meter::format_tokens(context)),
            );
        }
        chip.into_any_element()
    }

    /// A small status/source pill on a provider card.
    pub(super) fn provider_badge(
        &self,
        label: &str,
        fg: Hsla,
        bg: Hsla,
        theme: Theme,
    ) -> AnyElement {
        Badge::new(label.to_string(), fg, BadgeFill::Solid(bg)).render(&theme)
    }

    /// A provider-card action button. One closure per card would be a lot of
    /// near-identical code, so every button routes through
    /// [`Self::apply_provider_action`]. The glyph is derived from the action.
    pub(super) fn provider_button(
        &self,
        id: String,
        label: &str,
        style: ButtonVariant,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: ProviderAction,
    ) -> AnyElement {
        self.provider_button_sized(id, label, style, ButtonSize::Medium, theme, this, action)
    }

    /// [`Self::provider_button`] at a caller-chosen frame height. A modal
    /// footer whose Cancel is `Large` uses this for its primary action, so the
    /// two buttons share one baseline and one height.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn provider_button_sized(
        &self,
        id: String,
        label: &str,
        style: ButtonVariant,
        size: ButtonSize,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: ProviderAction,
    ) -> AnyElement {
        let action_id = id.clone();
        let icon_path = Self::provider_action_icon(&action);
        let mut button = Button::new(id, style).size(size).label(label.to_string());
        if let Some(path) = icon_path {
            // Provider actions keep their compact 12px glyph at every frame size.
            button = button.leading(icon(path, IconSize::XSmall.px(&theme), style.ink(&theme)));
        }
        button
            .on_mouse_up(&theme, move |_, window, cx| {
                // A button may sit inside a clickable row (the provider row's
                // head); the press is the button's alone.
                cx.stop_propagation();
                let action = action.clone();
                this.update(cx, |app, cx| app.apply_provider_action(action, window, cx));
            })
            .debug_selector(move || format!("provider-action-{action_id}"))
            .into_any_element()
    }

    /// The glyph for a provider action (none for plain text buttons).
    pub(super) fn provider_action_icon(action: &ProviderAction) -> Option<&'static str> {
        match action {
            ProviderAction::SignIn { .. } | ProviderAction::AuthStart { .. } => {
                Some("icons/lock.svg")
            }
            ProviderAction::AuthOpenUrl(_) => Some("icons/arrow-up-right.svg"),
            ProviderAction::AuthCopy(_) => Some("icons/copy.svg"),
            ProviderAction::AuthCancel => Some("icons/x.svg"),
            ProviderAction::AuthDismiss => Some("icons/check.svg"),
            ProviderAction::EditKey { .. } => Some("icons/at-sign.svg"),
            ProviderAction::AddAccount { .. } => Some("icons/plus.svg"),
            ProviderAction::RenameAccount { .. } => Some("icons/pencil.svg"),
            ProviderAction::RemoveAccount { .. } => Some("icons/trash.svg"),
            ProviderAction::ShowUsage { .. } => Some("icons/usage-total.svg"),
            ProviderAction::SignOut { .. } => Some("icons/stop.svg"),
            ProviderAction::Configure { .. } => Some("icons/settings.svg"),
            ProviderAction::Remove { .. } => Some("icons/trash.svg"),
            ProviderAction::Restart => Some("icons/refresh.svg"),
            ProviderAction::SaveKey | ProviderAction::SaveAccount => None,
        }
    }

    /// Single dispatch point for every provider-card button.
    pub(super) fn apply_provider_action(
        &mut self,
        action: ProviderAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match action {
            ProviderAction::SignIn { id, name } => {
                // Signing in from the key modal replaces it.
                self.provider_key_editor = None;
                self.provider_oauth_login(id, name, cx);
            }
            ProviderAction::AuthStart { id, name, method } => {
                self.provider_key_editor = None;
                self.auth_start_login(id, name, method, cx);
            }
            ProviderAction::AuthCancel => self.auth_cancel_login(cx),
            ProviderAction::AuthDismiss => {
                self.auth.dismiss_result();
                cx.notify();
            }
            ProviderAction::AuthOpenUrl(url) => {
                if let Err(err) = platform::open_url(&url) {
                    self.toast_warning(tr!("runtime.browser_failed", error = err));
                }
            }
            ProviderAction::AuthCopy(value) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(value));
                self.toast_success(tr!("settings.copied_to_clipboard"));
            }
            ProviderAction::EditKey {
                id,
                name,
                oauth,
                note,
            } => self.provider_key_open(id, name, oauth, note, window, cx),
            ProviderAction::AddAccount { id, name } => {
                self.account_editor_open(id, name, None, window, cx)
            }
            ProviderAction::RenameAccount {
                provider,
                name,
                id,
                label,
            } => self.account_editor_open(provider, name, Some((id, label)), window, cx),
            ProviderAction::RemoveAccount {
                id,
                label,
                alias,
            } => {
                self.request_confirm(
                    super::confirm_ui::PendingConfirm::RemoveAccount { id, label, alias },
                    cx,
                );
            }
            ProviderAction::SignOut { id } => self.provider_sign_out(id, cx),
            ProviderAction::ShowUsage { id } => {
                self.provider_usage_open = Some(id);
                cx.notify();
            }
            ProviderAction::Configure { id } => self.provider_editor_open(Some(id), window, cx),
            ProviderAction::Remove { id, name } => {
                self.request_confirm(
                    super::confirm_ui::PendingConfirm::RemoveProvider { id, name },
                    cx,
                );
            }
            ProviderAction::SaveKey => self.provider_key_save(window, cx),
            ProviderAction::SaveAccount => self.account_save(window, cx),
            ProviderAction::Restart => self.provider_apply_credentials(cx),
        }
    }

    /// The provider usage/quota modal, or `None` when closed. The card keeps
    /// only a Usage button; windows, balances, and spend render here so a
    /// connected card stays compact.
    pub(super) fn provider_usage_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let id = self.provider_usage_open.as_deref()?;
        let view = self
            .provider_views()
            .into_iter()
            .find(|view| view.id == id)?;
        let body = self.provider_quota_section(&view, theme)?;

        let close = Button::keyed("provider-usage-close", ButtonVariant::Secondary)
.size(ButtonSize::Large)
.render(&theme)
            .on_mouse_up(MouseButton::Left, {
                let this = this.clone();
                move |_, _, cx| {
                    this.update(cx, |app, cx| {
                        app.provider_usage_open = None;
                        cx.notify();
                    });
                }
            })
            .child(
                div()
                    .text_color(theme.text_secondary)
                    .child(tr!("settings.close")),
            );

        let card = modal_card(&theme)
            .modal_width(px(460.))
            .max_h(relative(1.))
            .on_action(cx.listener(Self::provider_editor_cancel))
            .child(modal_header(
                &theme,
                tr!("settings.usage_for", name = view.name),
                Some(ModalSubtitle::Id(view.id.clone().into())),
            ))
            .child(
                div()
                    .id("provider-usage-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body),
            )
            .child(modal_footer(&theme).child(close));

        // The backdrop every modal shares (`ui::dialog::modal_scrim`'s role).
        let scrim = theme.scrim_modal();
        Some(
            div()
                .id("provider-usage-layer")
                .absolute()
                .inset_0()
                .occlude()
                .bg(scrim)
                .p(DynamicSpacing::Base24.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::provider_editor_scrim))
                .child(crate::motion::enter(
                    card,
                    "provider-usage-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }

    /// The API-key modal, or `None` when closed.
    pub(super) fn provider_key_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let editor = self.provider_key_editor.as_ref()?;

        let field_label = tr!("settings.api_key");
        let hint = tr!("settings.api_key_hint");

        let mut body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(input::gap(&theme))
                    .child(
                        input_label(field_label, &theme)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary),
                    )
                    .child(TextField::new(editor.key.clone(), theme.background).render(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(hint),
                    ),
            );
        if !editor.note.is_empty() {
            body = body.child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_muted)
                    .child(providers::localize_note(editor.note)),
            );
        }
        if editor.oauth {
            let id = editor.provider_id.clone();
            let name = editor.provider_name.clone();
            let this_signin = this.clone();
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(
                        div()
                            .flex_1()
                            .text_size(TextSize::Small.px(&theme))
                            .text_color(theme.text_muted)
                            .child(tr!(
                                "settings.prefer_a_subscription_sign_in_with_oauth_instead"
                            )),
                    )
                    .child(self.provider_button(
                        format!("provider-key-signin-{id}"),
                        &tr!("settings.sign_in"),
                        ButtonVariant::Secondary,
                        theme,
                        this_signin,
                        ProviderAction::SignIn { id, name },
                    )),
            );
        }
        if let Some(error) = &editor.error {
            body = body.child(
                Banner::note(BannerTone::Error, error.clone(), &theme),
            );
        }

        let cancel = Button::keyed("provider-key-close", ButtonVariant::Secondary)
.size(ButtonSize::Large)
.render(&theme)
            .on_mouse_up(MouseButton::Left, {
                let this = this.clone();
                move |_, _, cx| {
                    this.update(cx, |app, cx| {
                        app.provider_key_editor = None;
                        cx.notify();
                    });
                }
            })
            .child(
                div()
                    .text_color(theme.text_secondary)
                    .child(tr!("settings.cancel")),
            );
        let save = self.provider_button_sized(
            "provider-key-save".into(),
            &tr!("settings.save_key"),
            ButtonVariant::Primary,
            ButtonSize::Large,
            theme,
            this.clone(),
            ProviderAction::SaveKey,
        );

        let card = modal_card(&theme)
            .debug_selector(|| "provider-key-card".to_string())
            .modal_width(px(460.))
            .max_h(relative(1.))
            .on_action(cx.listener(Self::provider_editor_cancel))
            .on_action(cx.listener(Self::provider_editor_confirm))
            .child(modal_header(
                &theme,
                tr!("settings.api_key_for", name = editor.provider_name),
                Some(ModalSubtitle::Id(editor.provider_id.clone().into())),
            ))
            .child(
                div()
                    .id("provider-key-body")
                    .debug_selector(|| "provider-key-body".to_string())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body.debug_selector(|| "provider-key-content".to_string())),
            )
            .child(modal_footer(&theme).child(cancel).child(save));

        // The backdrop every modal shares (`ui::dialog::modal_scrim`'s role).
        let scrim = theme.scrim_modal();
        Some(
            div()
                .id("provider-key-layer")
                .absolute()
                .inset_0()
                .occlude()
                .bg(scrim)
                .p(DynamicSpacing::Base24.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::provider_editor_scrim))
                .child(crate::motion::enter(
                    card,
                    "provider-key-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }

    /// **Fetch models**, under the editor's model field: a bordered secondary
    /// button that reads **Fetching…** and stops taking clicks while the
    /// request is out.
    fn provider_fetch_button(
        &self,
        fetching: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let label = if fetching {
            tr!("settings.fetching_models")
        } else {
            tr!("settings.fetch_models")
        };
        div()
            .flex()
            .child(
                Button::new("provider-fetch-models", ButtonVariant::Secondary)
                    .label(label)
                    .icon("icons/refresh.svg")
                    .tip(tr!("settings.fetch_models_hint"))
                    .loading(fetching)
                    .faded(&theme, move |button| {
                        button.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                            this.update(cx, |app, cx| app.provider_fetch_models(cx));
                        })
                    }),
            )
            .into_any_element()
    }

    /// The provider editor modal (add / configure), or `None` when closed.
    pub(super) fn provider_editor_layer(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        let editor = self.provider_editor.as_ref()?;
        let editing = editor.original_id.is_some();
        let is_custom_entry = editor.original_id.as_ref().is_some_and(|id| {
            self.custom_providers
                .iter()
                .any(|provider| &provider.id == id)
        });
        let subtitle = if editing && editor.in_catalog && !is_custom_entry {
            tr!("settings.builtin_provider_hint")
        } else {
            tr!("settings.saved_to_models_json")
        };

        let field = |label: &str, hint: Option<&str>, input: Entity<ComposerInput>| -> AnyElement {
            let mut column = div()
                .flex()
                .flex_col()
                .gap(input::gap(&theme))
                .child(
                    input_label(label.to_string(), &theme)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary),
                )
                .child(TextField::new(input, theme.background).render(&theme));
            if let Some(hint) = hint {
                column = column.child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_muted)
                        .child(hint.to_string()),
                );
            }
            column.into_any_element()
        };

        // API family chips.
        let mut api_chips = div()
            .flex()
            .flex_wrap()
            .gap(DynamicSpacing::Base06.px(&theme));
        for api in PROVIDER_APIS {
            let selected = editor.api == api;
            let this = this.clone();
            api_chips = api_chips.child(
                ChipButton::new(format!("provider-api-{api}"), ChipStyle::Outlined { inset: false })
                    .size(ButtonSize::Default)
                    .label(api)
                    .mono()
                    .selected(selected)
                    .on_mouse_up(&theme, move |_, _, cx| {
                        this.update(cx, |app, cx| {
                            if let Some(editor) = app.provider_editor.as_mut() {
                                editor.api = api.to_string();
                                cx.notify();
                            }
                        });
                    }),
            );
        }

        // Identity: editable only when adding.
        let identity: AnyElement = if editing {
            div()
                .flex()
                .flex_col()
                .gap(input::gap(&theme))
                .child(
                    input_label(tr!("settings.provider_id"), &theme)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_secondary),
                )
                .child(
                    div()
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(12.))
                        .text_color(theme.text_primary)
                        .child(editor.original_id.clone().unwrap_or_default()),
                )
                .into_any_element()
        } else {
            field(
                &tr!("settings.provider_id"),
                Some(&tr!("settings.provider_id_hint")),
                editor.id.clone(),
            )
        };

        let api_key_hint = if editor.had_api_key {
            tr!("settings.api_key_stored_hint")
        } else {
            tr!("settings.api_key_optional_hint")
        };
        let models_builtin_hint = tr!("settings.models_builtin_hint");
        let models_custom_hint = tr!("settings.models_custom_hint");

        let body = div()
            .w_full()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base12.px(&theme))
            .child(identity)
            .child(field(
                &tr!("settings.display_name"),
                Some(&tr!("settings.optional")),
                editor.name.clone(),
            ))
            .child(field(
                &tr!("settings.base_url"),
                Some(&tr!("settings.base_url_hint")),
                editor.base_url.clone(),
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(input::gap(&theme))
                    .child(
                        input_label(tr!("settings.api_type"), &theme)
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary),
                    )
                    .child(api_chips),
            )
            .child(field(
                &tr!("settings.api_key"),
                Some(&api_key_hint),
                editor.api_key.clone(),
            ))
            .child(field(
                &tr!("settings.models_field"),
                Some(if editor.in_catalog {
                    &models_builtin_hint
                } else {
                    &models_custom_hint
                }),
                editor.models.clone(),
            ))
            // The model list itself is on the provider's card (#131): this
            // dialog edits the ids and the endpoint, it does not repeat them.
            .when(
                providers::models_endpoint("https://x", &editor.api).is_some(),
                |body| body.child(self.provider_fetch_button(editor.fetching, theme, this.clone())),
            );

        let mut card = modal_card(&theme)
            .modal_width(px(520.))
            .max_h(relative(1.))
            .on_action(cx.listener(Self::provider_editor_cancel))
            .on_action(cx.listener(Self::provider_editor_confirm))
            .child(modal_header(
                &theme,
                if editing {
                    tr!("settings.configure_provider")
                } else {
                    tr!("settings.add_provider")
                },
                Some(ModalSubtitle::Text(subtitle.into())),
            ))
            .child(
                div()
                    .id("provider-editor-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(modal::section_padding_x(&theme, false))
                    .py(DynamicSpacing::Base16.px(&theme))
                    .child(body),
            );

        if let Some(error) = &editor.error {
            card = card.child(
                Banner::note(BannerTone::Error, error.clone(), &theme)
                    .mx(modal::section_padding_x(&theme, false))
                    .mb(DynamicSpacing::Base04.px(&theme)),
            );
        }

        let cancel = {
            let this = this.clone();
            Button::keyed("provider-editor-cancel", ButtonVariant::Secondary)
.size(ButtonSize::Large)
.render(&theme)
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    app.provider_editor = None;
                    cx.notify();
                });
            })
            .child(
                div()
                    .text_color(theme.text_secondary)
                    .child(tr!("settings.cancel")),
            )
        };
        let save = {
            let this = this.clone();
            Button::new("provider-editor-save", ButtonVariant::Primary)
                .size(ButtonSize::Large)
                .label(if editing {
                            tr!("settings.save_changes")
                        } else {
                            tr!("settings.add_provider")
                        })
                .on_mouse_up(&theme, move |_, window, cx| {
                    this.update(cx, |app, cx| app.provider_save(window, cx));
                })
        };
        card = card.child(modal_footer(&theme).child(cancel).child(save));

        // The backdrop every modal shares (`ui::dialog::modal_scrim`'s role).
        let scrim = theme.scrim_modal();
        Some(
            div()
                .id("provider-editor-layer")
                .absolute()
                .inset_0()
                .occlude()
                .bg(scrim)
                .px(DynamicSpacing::Base24.px(&theme))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, cx.listener(Self::provider_editor_scrim))
                .child(crate::motion::enter(
                    card,
                    "provider-editor-card-in",
                    &theme,
                    crate::theme::tokens::Motion::Layout,
                    None,
                ))
                .into_any_element(),
        )
    }

    /// A settings section: a readable label over one grouped board.
    /// The label sits closer to its board than to the section above it.
    pub(super) fn settings_section(
        &self,
        theme: Theme,
        label: &str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.settings_section_desc(theme, label, None, rows)
    }

    /// Like [`Self::settings_section`], with one line of context under the
    /// label. Used where several rows share a trigger, so it is stated once
    /// instead of repeated on every row.
    pub(super) fn settings_section_desc(
        &self,
        theme: Theme,
        label: &str,
        desc: Option<&str>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .px(DynamicSpacing::Base04.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base02.px(&theme))
                    .child(
                        div()
                            .text_size(TextSize::Default.px(&theme))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_primary)
                            .child(label.to_string()),
                    )
                    .children(desc.map(|desc| {
                        div()
                            .min_w_0()
                            .text_size(TextSize::Default.px(&theme))
                            .text_color(theme.text_secondary)
                            .whitespace_normal()
                            .child(desc.to_string())
                    })),
            )
            .child(self.settings_group(theme, rows))
            .into_any_element()
    }

    /// One bordered board whose rows are divided by inset 1px hairlines —
    /// the grouped-surface pattern from DESIGN.md, never a stack of cards.
    /// No `overflow_hidden`: select popups anchor inside these rows and
    /// must escape the board's rounded box.
    pub(super) fn settings_group(&self, theme: Theme, rows: Vec<AnyElement>) -> AnyElement {
        let last = rows.len().saturating_sub(1);
        let mut board = div()
            .w_full()
            .board_surface(&theme, theme.surface_secondary)
            .flex()
            .flex_col();
        for (i, row) in rows.into_iter().enumerate() {
            board = board.child(row);
            if i != last {
                board = board.child(
                    div()
                        .h(BORDER_WIDTH)
                        .flex_none()
                        .mx(DynamicSpacing::Base16.px(&theme))
                        .bg(theme.hairline()),
                );
            }
        }
        board.into_any_element()
    }

    /// A row inside a settings board: title (and optional description or
    /// dimmed path) on the left, an optional control on the right. Every
    /// control shares one right-aligned axis down the board.
    pub(super) fn setting_row(
        &self,
        theme: Theme,
        title: &str,
        desc: Option<&str>,
        meta: Option<&str>,
        control: Option<AnyElement>,
    ) -> AnyElement {
        crate::ui::row::setting_row(theme, title, desc, meta, control)
    }

    /// Connection state: green dot + "Connected" / red dot + "Not running".
    pub(super) fn connection_status(&self, theme: Theme) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(if self.client.is_some() {
                        theme.success
                    } else {
                        theme.error
                    }),
            )
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(if self.client.is_some() {
                        tr!("status.connected")
                    } else {
                        tr!("settings.not_running")
                    }),
            )
            .into_any_element()
    }

    // ── Settings → About: the repository link ──────────────────────────

    /// The About page's repository link: a ghost button that opens the
    /// project's GitHub page in the OS browser.
    pub(super) fn about_github_button(&self, theme: Theme) -> AnyElement {
        Button::keyed("about-github", ButtonVariant::Secondary)
.size(ButtonSize::Medium)
.render(&theme)
            .child(icon(
                "icons/arrow-up-right.svg",
                ButtonSize::Medium.icon_size().px(&theme),
                theme.text_secondary,
            ))
            .child(tr!("settings.github"))
            .on_mouse_up(MouseButton::Left, |_, _, cx| {
                cx.open_url(env!("CARGO_PKG_REPOSITORY"));
            })
            .debug_selector(|| "about-github".to_string())
            .into_any_element()
    }

    // ── Settings → Shortcuts ─────────────────────────────────────

    /// The Shortcuts reference: every workbench-level chord, grouped by the
    /// surface it acts on. The app-command rows are generated from the
    /// command registry (`crate::commands`), so the page is complete and can
    /// never advertise a chord the keymap does not bind. Only the composer's
    /// text-entry conventions (send/newline/accept — not commands) are
    /// listed literally, because their labels depend on live preferences.
    pub(super) fn shortcut_rows(&self, theme: Theme) -> Vec<AnyElement> {
        use crate::commands::{self, Category};

        let mut rows: Vec<AnyElement> = Vec::new();
        for category in Category::ORDER {
            let section: Vec<AnyElement> = commands::COMMANDS
                .iter()
                .filter(|spec| spec.help && spec.category == category)
                .map(|spec| {
                    let title = tr!(spec.title_key);
                    let chips = spec.shortcuts();
                    if chips.is_empty() {
                        self.setting_row(theme, &title, None, None, None)
                    } else {
                        let keys: Vec<&str> = chips.iter().map(String::as_str).collect();
                        self.shortcut_row(theme, &title, &keys)
                    }
                })
                .collect();
            if !section.is_empty() {
                rows.push(self.settings_section(theme, &tr!(category.group_key()), section));
            }
            if category == Category::Agent {
                rows.push(self.composer_shortcut_section(theme));
            }
        }
        rows
    }

    /// The composer's text-entry conventions — not commands, so kept out of
    /// the registry — with the live send-mode labels the preference drives.
    fn composer_shortcut_section(&self, theme: Theme) -> AnyElement {
        use crate::platform::shortcuts as keys;
        self.settings_section(
            theme,
            &tr!("shortcut.group_composer"),
            vec![
                self.shortcut_row(
                    theme,
                    &tr!("shortcut.send_idle"),
                    &[keys::label(keys::SEND).as_str()],
                ),
                self.shortcut_row(
                    theme,
                    &tr!(
                        "shortcut.send_while_running",
                        mode = SendMode::FollowUp.label()
                    ),
                    &[SendMode::FollowUp
                        .shortcut_label(theme.ui.composer_send_mode)
                        .as_str()],
                ),
                self.shortcut_row(
                    theme,
                    &tr!(
                        "shortcut.send_while_running",
                        mode = SendMode::Steer.label()
                    ),
                    &[
                        SendMode::Steer
                            .shortcut_label(theme.ui.composer_send_mode)
                            .as_str(),
                        keys::label(keys::STEER).as_str(),
                    ],
                ),
                self.shortcut_row(
                    theme,
                    &tr!("shortcut.newline"),
                    &[keys::label(keys::NEWLINE).as_str()],
                ),
                self.shortcut_row(
                    theme,
                    &tr!("shortcut.accept"),
                    &[keys::label(keys::ACCEPT).as_str()],
                ),
                self.shortcut_row(
                    theme,
                    &tr!("shortcut.stop"),
                    &[keys::label(keys::STOP).as_str()],
                ),
            ],
        )
    }

    /// One shortcut reference row: the command label on the left, its chord as
    /// a keycap chip on the shared right axis. `setting_row` owns the layout so
    /// the page reads like every other settings board.
    fn shortcut_row(&self, theme: Theme, title: &str, keys: &[&str]) -> AnyElement {
        self.setting_row(
            theme,
            title,
            None,
            None,
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base04.px(&theme))
                    .children(keys.iter().map(|key| keycap(key, theme)))
                    .into_any_element(),
            ),
        )
    }

    // ── Settings → General: notifications ──────────────────────────────

    /// The notification board: three real channels plus the honest system
    /// state when macOS is blocking banners or the build is unbundled. A
    /// switch the OS ignores must not look like it is working.
    pub(super) fn notification_rows(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let mut rows = vec![
            self.setting_row(
                theme,
                &tr!("settings.desktop_notifications"),
                Some(&tr!(
                    "settings.show_a_system_banner_when_a_run_finishes_while_o"
                )),
                None,
                Some(self.settings_toggle(
                    "notification-desktop-toggle",
                    self.notification_prefs.desktop,
                    theme,
                    this.clone(),
                    Self::toggle_desktop_notifications,
                )),
            ),
            self.setting_row(
                theme,
                &tr!("settings.in_app_toasts"),
                Some(&tr!(
                    "settings.show_a_toast_in_the_window_when_a_run_finishes_o"
                )),
                None,
                Some(self.settings_toggle(
                    "notification-toasts-toggle",
                    self.notification_prefs.toasts,
                    theme,
                    this.clone(),
                    Self::toggle_toast_notifications,
                )),
            ),
            self.setting_row(
                theme,
                &tr!("settings.notification_sound"),
                Some(&tr!(
                    "settings.play_the_system_alert_sound_when_a_background_ru"
                )),
                None,
                Some(self.settings_toggle(
                    "notification-sound-toggle",
                    self.notification_prefs.sound,
                    theme,
                    this.clone(),
                    Self::toggle_notification_sound,
                )),
            ),
        ];
        if self.notification_prefs.desktop {
            match self.notification_auth {
                notifications::DesktopAuth::Denied => rows.push(self.setting_row(
                    theme,
                    &tr!("settings.blocked_in_system_settings"),
                    Some(&tr!(
                        "settings.macos_is_not_allowing_orbit_pi_to_post_notificat"
                    )),
                    None,
                    Some(self.runtime_button(
                        "notification-open-settings",
                        &tr!("settings.open_system_settings"),
                        false,
                        theme,
                        this,
                        |_, _| {
                            let _ = platform::open_notification_settings();
                        },
                    )),
                )),
                notifications::DesktopAuth::Unbundled => rows.push(self.setting_row(
                    theme,
                    &tr!("settings.developer_build"),
                    Some(&tr!(
                        "settings.orbit_is_running_from_a_bare_binary_so_banners_a"
                    )),
                    None,
                    None,
                )),
                notifications::DesktopAuth::Granted | notifications::DesktopAuth::Unknown => {}
            }
        }
        self.settings_section_desc(
            theme,
            &tr!("settings.notifications"),
            Some(&tr!("settings.notifications_description")),
            rows,
        )
    }

    /// Flip the desktop channel, ask for permission on the way on, and
    /// re-read the OS state so the board never shows a stale state.
    pub(super) fn toggle_desktop_notifications(&mut self, cx: &mut Context<Self>) {
        self.notification_prefs.desktop = !self.notification_prefs.desktop;
        notifications::Prefs::persist(self.notification_prefs);
        if self.notification_prefs.desktop {
            notifications::request_permission();
            self.refresh_notification_auth(cx);
        } else {
            self.notification_auth = notifications::DesktopAuth::Unknown;
        }
        cx.notify();
    }

    /// Flip the in-app toast channel; turning it on previews a card so the
    /// switch shows what it toggles.
    pub(super) fn toggle_toast_notifications(&mut self, cx: &mut Context<Self>) {
        self.notification_prefs.toasts = !self.notification_prefs.toasts;
        notifications::Prefs::persist(self.notification_prefs);
        if self.notification_prefs.toasts {
            self.toast_info(tr!("settings.in_app_toasts_are_on"));
        }
        cx.notify();
    }

    /// Flip the sound channel; turning it on previews the sound so the
    /// switch is heard before it matters.
    pub(super) fn toggle_notification_sound(&mut self, cx: &mut Context<Self>) {
        self.notification_prefs.sound = !self.notification_prefs.sound;
        notifications::Prefs::persist(self.notification_prefs);
        if self.notification_prefs.sound {
            notifications::play_sound();
        }
        cx.notify();
    }

    /// Read macOS's banner permission without blocking the UI. Called when
    /// General opens and after the desktop switch flips on.
    pub(super) fn refresh_notification_auth(&mut self, cx: &mut Context<Self>) {
        if self.notification_auth_pending || !self.notification_prefs.desktop {
            return;
        }
        self.notification_auth_pending = true;
        cx.spawn(async move |this, cx| {
            // The framework answers on a background queue; the blocking read
            // runs off the UI thread.
            let auth = cx
                .background_executor()
                .spawn(async { notifications::permission() })
                .await;
            this.update(cx, |app, cx| {
                app.notification_auth = auth;
                app.notification_auth_pending = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── Settings → General: exports ─────────────────────────────────────

    /// The export board: what a Markdown/PDF export carries beyond the answer.
    /// Off by default — the reasoning and tool trail is the agent's working
    /// noise, not the conversation.
    pub(super) fn export_rows(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let rows = vec![self.setting_row(
            theme,
            &tr!("settings.include_reasoning_and_tools"),
            Some(&tr!("settings.include_reasoning_and_tools_description")),
            None,
            Some(self.settings_toggle(
                "export-work-toggle",
                self.export_prefs.include_thinking_and_tools,
                theme,
                this,
                Self::toggle_export_work,
            )),
        )];
        self.settings_section_desc(
            theme,
            &tr!("settings.transcript_export"),
            Some(&tr!("settings.transcript_export_description")),
            rows,
        )
    }

    /// Flip whether exports append reasoning and tool calls, and persist it so
    /// the next export in any session honours the choice.
    pub(super) fn toggle_export_work(&mut self, cx: &mut Context<Self>) {
        self.export_prefs.include_thinking_and_tools =
            !self.export_prefs.include_thinking_and_tools;
        self.export_prefs.persist();
        if self.export_prefs.include_thinking_and_tools {
            self.toast_info(tr!("settings.exports_include_reasoning_and_tools"));
        }
        cx.notify();
    }

    // ── Settings → Runtime ─────────────────────────────────────────────

    /// The Runtime section: the live pi process, its details, and
    /// start/stop/restart controls. Orbit has no socket server — the
    /// transport is stdio, so there is no host or port to report.
    pub(super) fn runtime_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        _cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let state = self.runtime_state();
        let (state_label, state_color) = match state {
            RuntimeState::Running => (tr!("runtime.state_running"), theme.success),
            RuntimeState::Exited => (tr!("runtime.state_exited"), theme.error),
            RuntimeState::Stopped => (tr!("runtime.state_stopped"), theme.text_muted),
            RuntimeState::Failed => (tr!("runtime.state_failed"), theme.error),
        };
        let running = state == RuntimeState::Running;
        let has_client = self.client.is_some();

        let description = match state {
            RuntimeState::Running => {
                tr!("settings.spawned_child_process")
            }
            RuntimeState::Exited => tr!("settings.runtime_exited"),
            RuntimeState::Stopped => tr!("settings.runtime_stopped"),
            RuntimeState::Failed => tr!("settings.runtime_failed"),
        };

        let mut controls = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme));
        if has_client {
            controls = controls
                .child(self.runtime_button(
                    "runtime-restart",
                    "Restart",
                    false,
                    theme,
                    this.clone(),
                    OrbitApp::runtime_restart,
                ))
                .child(self.runtime_button(
                    "runtime-stop",
                    "Stop",
                    false,
                    theme,
                    this.clone(),
                    OrbitApp::runtime_stop,
                ));
        } else {
            controls = controls.child(self.runtime_button(
                "runtime-start",
                "Start",
                true,
                theme,
                this.clone(),
                OrbitApp::runtime_start,
            ));
        }

        let pid = self
            .client
            .as_ref()
            .map(|client| client.child_pid().to_string())
            .unwrap_or_else(|| "—".to_string());
        let uptime = if running {
            self.runtime
                .started_at
                .map(|started| format_uptime(started.elapsed()))
                .unwrap_or_else(|| "—".to_string())
        } else {
            "—".to_string()
        };

        let status = div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(div().size(px(7.)).rounded_full().bg(state_color))
            .child(
                div()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_primary)
                    .child(state_label),
            )
            .into_any_element();

        let mut process = vec![self.setting_row(
            theme,
            &tr!("settings.status"),
            Some(&description),
            None,
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base08.px(&theme))
                    .child(status)
                    .child(controls)
                    .into_any_element(),
            ),
        )];
        process.push(self.setting_row(
            theme,
            &tr!("settings.process_id"),
            None,
            None,
            Some(runtime_text(theme, pid)),
        ));
        process.push(self.setting_row(
            theme,
            &tr!("settings.binary"),
            None,
            None,
            Some(runtime_path(theme, orbit_rpc::pi_binary())),
        ));
        process.push(self.setting_row(
            theme,
            &tr!("settings.uptime"),
            None,
            None,
            Some(runtime_text(theme, uptime)),
        ));
        process.push(self.setting_row(
            theme,
            &tr!("settings.transport"),
            None,
            None,
            Some(runtime_text(theme, tr!("settings.transport_stdio"))),
        ));
        // The package root the patches land in is a deep `node_modules` path.
        // It rides the row's truncating meta line rather than the control
        // slot: a control never shrinks, so a path there eats the whole row
        // and leaves the title one letter per line. Home-shortened, it still
        // makes a stale saved root obvious.
        let package_root = self
            .rpc_patches
            .package_root
            .as_ref()
            .map(|root| crate::usage::format::short_path(&root.display().to_string()));
        process.push(self.setting_row(
            theme,
            "RPC patches",
            Some(&self.rpc_patches.summary()),
            package_root.as_deref(),
            Some(self.settings_toggle(
                "rpc-patches-toggle",
                self.rpc_patches.enabled,
                theme,
                this.clone(),
                OrbitApp::toggle_rpc_patches,
            )),
        ));
        if let Some(error) = &self.runtime.error {
            process.push(self.setting_row(
                theme,
                &tr!("settings.last_error"),
                None,
                None,
                Some(runtime_error(theme, error.clone())),
            ));
        }

        let mut rows = vec![self.settings_section(theme, &tr!("settings.process"), process)];

        // Background sessions — each owns its own pi process.
        if !self.lives.is_empty() {
            let count = self.lives.len();
            let parked_desc = tr!("settings.background_sessions", count = count);
            let mut background = vec![self.setting_row(
                theme,
                &tr!("settings.parked_processes"),
                Some(&parked_desc),
                None,
                None,
            )];
            for (path, parked) in &self.lives {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string_lossy().into_owned());
                background.push(self.setting_row(
                    theme,
                    &name,
                    None,
                    None,
                    Some(runtime_text(
                        theme,
                        tr!(
                            "settings.pid_state",
                            pid = parked.client.child_pid(),
                            state = if parked.busy {
                                tr!("settings.busy")
                            } else {
                                tr!("settings.idle")
                            }
                        ),
                    )),
                ));
            }
            rows.push(self.settings_section(theme, &tr!("settings.background"), background));
        }

        // Recent stderr — visible failures for "if any issue, show status".
        let stderr = self
            .client
            .as_ref()
            .map(|client| client.recent_stderr(8))
            .unwrap_or_default();
        // The panel is a window onto the child's raw stderr; a known MCP
        // secret value in a failure line must not become readable here.
        let stderr: Vec<String> = stderr
            .iter()
            .map(|line| self.mcp.redact_text(line))
            .collect();
        let body: AnyElement = if stderr.is_empty() {
            div()
                .px(DynamicSpacing::Base16.px(&theme))
                .py(DynamicSpacing::Base12.px(&theme))
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("settings.no_output_from_the_pi_process"))
                .into_any_element()
        } else {
            let mut block = div()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base02.px(&theme));
            for line in &stderr {
                block = block.child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(11.5))
                        .text_color(theme.code_text)
                        .child(line.clone()),
                );
            }
            div()
                .bg(theme.code_bg)
                .rounded(panel::RADIUS.px(&theme))
                .px(DynamicSpacing::Base12.px(&theme))
                .py(DynamicSpacing::Base08.px(&theme))
                .child(block)
                .into_any_element()
        };
        rows.push(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base08.px(&theme))
                .child(
                    div()
                        .px(DynamicSpacing::Base04.px(&theme))
                        .text_size(TextSize::XSmall.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_muted)
                        .child(tr!("settings.recent_stderr")),
                )
                .child(body)
                .into_any_element(),
        );

        rows
    }

    // ── Settings → Agent ───────────────────────────────────────────────

    /// Agent behavior: Orbit's composer preference plus pi's queue delivery,
    /// compaction, and retry controls. The composer preference is local; it
    /// chooses which existing RPC command the next send uses.
    pub(super) fn agent_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let mut behavior = Vec::new();
        // Spelling is AppKit's checker, so the switch only exists where it can
        // actually mark words: a toggle that did nothing on Windows or Linux
        // would be a lie.
        if cfg!(target_os = "macos") {
            behavior.push(self.setting_row(
                theme,
                &tr!("settings.check_spelling"),
                Some(&tr!("settings.check_spelling_hint")),
                None,
                Some(self.settings_toggle(
                    "spell-check-toggle",
                    theme.ui.spell_check,
                    theme,
                    this.clone(),
                    Self::toggle_spell_check,
                )),
            ));
        }
        behavior.extend([
            self.setting_row(
                theme,
                &tr!("settings.enter_while_running"),
                Some(&tr!(
                    "settings.enter_while_running_hint",
                    keys = platform::shortcuts::label(platform::shortcuts::SEND_ALTERNATE)
                )),
                None,
                Some(self.composer_send_mode_toggle(theme, this.clone())),
            ),
            self.setting_row(
                theme,
                &tr!("settings.steering_messages"),
                Some(&tr!("settings.steering_messages_hint")),
                None,
                Some(self.steering_mode_toggle(theme, this.clone())),
            ),
            self.setting_row(
                theme,
                &tr!("settings.follow_up_messages"),
                Some(&tr!(
                    "settings.messages_sent_while_the_agent_is_running_wait_in"
                )),
                None,
                Some(self.follow_up_mode_toggle(theme, this.clone())),
            ),
            self.setting_row(
                theme,
                &tr!("settings.auto_compaction"),
                Some(&tr!(
                    "settings.compact_conversation_context_automatically_when_"
                )),
                None,
                Some(self.settings_toggle(
                    "auto-compaction-toggle",
                    self.auto_compaction,
                    theme,
                    this.clone(),
                    Self::toggle_auto_compaction,
                )),
            ),
            self.setting_row(
                theme,
                &tr!("settings.auto_retry"),
                Some(&tr!(
                    "settings.retry_automatically_on_transient_errors_overload"
                )),
                None,
                Some(self.settings_toggle(
                    "auto-retry-toggle",
                    self.auto_retry,
                    theme,
                    this.clone(),
                    Self::toggle_auto_retry,
                )),
            ),
            self.setting_row(
                theme,
                &tr!("settings.agent_call_details"),
                Some(&tr!("settings.agent_call_details_hint")),
                None,
                Some(self.agent_calls_select(theme, this.clone(), cx)),
            ),
        ]);

        let mut sections = Vec::new();
        if self.retrying {
            sections.push(self.settings_section(
                theme,
                &tr!("settings.status"),
                vec![self.setting_row(
                    theme,
                    &tr!("settings.retrying"),
                    Some(&tr!(
                        "settings.pi_is_waiting_out_a_transient_provider_error_bef"
                    )),
                    None,
                    Some(self.runtime_button(
                        "abort-retry",
                        &tr!("settings.abort_retry"),
                        false,
                        theme,
                        this.clone(),
                        Self::abort_retry,
                    )),
                )],
            ));
        }
        if self.client.is_none() {
            sections.push(self.settings_section(
                theme,
                &tr!("settings.status"),
                vec![self.setting_row(
                    theme,
                    &tr!("settings.pi_is_not_connected"),
                    Some(&tr!(
                        "settings.start_the_runtime_from_settings_runtime_to_chang"
                    )),
                    None,
                    None,
                )],
            ));
        }
        sections.push(self.settings_section(theme, &tr!("settings.behavior"), behavior));

        // Auto session titles: the extension asks a model for a short title
        // from the first exchange; these two rows own whether and which.
        sections.push(self.settings_section(
            theme,
            &tr!("settings.session_titles"),
            vec![
                self.setting_row(
                    theme,
                    &tr!("settings.auto_session_titles"),
                    Some(&tr!(
                        "settings.name_each_session_from_its_first_exchange_so"
                    )),
                    None,
                    Some(self.settings_toggle(
                        "auto-title-toggle",
                        self.auto_title.enabled,
                        theme,
                        this.clone(),
                        Self::toggle_auto_title,
                    )),
                ),
                self.setting_row(
                    theme,
                    &tr!("settings.title_model"),
                    Some(&tr!(
                        "settings.model_that_writes_the_title_defaults_to_the_a"
                    )),
                    None,
                    Some(self.title_model_select(theme, this.clone(), cx)),
                ),
            ],
        ));

        sections.push(self.settings_section(
            theme,
            &tr!("settings.follow_up_suggestions"),
            vec![
                self.setting_row(
                    theme,
                    &tr!("settings.follow_up_suggestions"),
                    Some(&tr!("settings.follow_up_suggestions_hint")),
                    None,
                    Some(self.settings_toggle(
                        "suggestions-toggle",
                        self.suggestions.enabled,
                        theme,
                        this.clone(),
                        Self::toggle_suggestions,
                    )),
                ),
                self.setting_row(
                    theme,
                    &tr!("settings.suggestions_model"),
                    Some(&tr!("settings.suggestions_model_hint")),
                    None,
                    Some(self.suggestions_model_select(theme, this.clone(), cx)),
                ),
            ],
        ));

        // The model every new session starts on, and its thinking level.
        // Applied at session birth; existing sessions keep the model recorded
        // in their file.
        sections.push(self.settings_section(
            theme,
            &tr!("settings.session_defaults"),
            vec![
                self.setting_row(
                    theme,
                    &tr!("settings.default_model"),
                    Some(&tr!("settings.default_model_hint")),
                    None,
                    Some(self.default_model_select(theme, this.clone(), cx)),
                ),
                self.setting_row(
                    theme,
                    &tr!("settings.default_thinking"),
                    Some(&tr!("settings.default_thinking_hint")),
                    None,
                    Some(self.default_thinking_select(theme, this.clone(), cx)),
                ),
            ],
        ));
        sections
    }

    /// Settings → Agent: how agent call details open in the transcript. An
    /// explicit per-call toggle still wins; this only moves the default.
    pub(super) fn agent_calls_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        use crate::agent_call_display::AgentCallDisplay;
        let options: Vec<String> = AgentCallDisplay::OPTIONS
            .into_iter()
            .map(|mode| tr!(mode.label_key()))
            .collect();
        let selected = self.agent_calls.index();
        self.select_control(
            "agent-calls-select",
            SettingsSelect::AgentCalls,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// Flip auto session titles on/off and persist it for the extension.
    pub(super) fn toggle_auto_title(&mut self, cx: &mut Context<Self>) {
        self.auto_title.enabled = !self.auto_title.enabled;
        self.auto_title.persist();
        cx.notify();
    }

    /// Settings → Agent: the model the auto-title extension asks. The first
    /// option is the live session model; the rest is the reported catalog.
    pub(super) fn title_model_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut options: Vec<String> = vec![tr!("settings.active_session_model")];
        options.extend(self.available_models.iter().map(|model| {
            format!(
                "{} · {}",
                crate::providers::provider_display_name(&model.provider),
                model.name
            )
        }));
        let selected = self
            .auto_title
            .model
            .as_ref()
            .and_then(|wanted| {
                self.available_models
                    .iter()
                    .position(|model| model.provider == wanted.provider && model.id == wanted.id)
            })
            .map(|ix| ix + 1)
            .unwrap_or(0);
        self.select_control(
            "title-model-select",
            SettingsSelect::TitleModel,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    pub(super) fn toggle_suggestions(&mut self, cx: &mut Context<Self>) {
        self.suggestions.enabled = !self.suggestions.enabled;
        self.suggestions.persist_suggestions();
        self.clear_follow_up_suggestions();
        cx.notify();
    }

    pub(super) fn suggestions_model_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut options = vec![tr!("settings.active_session_model")];
        options.extend(self.available_models.iter().map(|model| {
            format!(
                "{} · {}",
                crate::providers::provider_display_name(&model.provider),
                model.name
            )
        }));
        let selected = self
            .suggestions
            .model
            .as_ref()
            .and_then(|wanted| {
                self.available_models
                    .iter()
                    .position(|model| model.provider == wanted.provider && model.id == wanted.id)
            })
            .map(|ix| ix + 1)
            .unwrap_or(0);
        self.select_control(
            "suggestions-model-select",
            SettingsSelect::SuggestionsModel,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// The model new sessions start on. The leading option clears the slot,
    /// leaving pi's own startup model untouched.
    pub(super) fn default_model_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut options: Vec<String> = vec![tr!("settings.pi_default")];
        options.extend(self.available_models.iter().map(|model| {
            format!(
                "{} · {}",
                crate::providers::provider_display_name(&model.provider),
                model.name
            )
        }));
        let slot = &self.session_default;
        let selected = slot
            .model_id
            .as_ref()
            .and_then(|id| {
                self.available_models.iter().position(|model| {
                    model.id == *id
                        && slot
                            .provider
                            .as_deref()
                            .is_none_or(|provider| model.provider == provider)
                })
            })
            .map(|ix| ix + 1)
            .unwrap_or_else(|| {
                if let Some(id) = &slot.model_id {
                    options.push(match &slot.provider {
                        Some(provider) => format!("{provider} · {id}"),
                        None => id.clone(),
                    });
                    options.len() - 1
                } else {
                    0
                }
            });
        self.select_control(
            "default-model-select",
            SettingsSelect::DefaultModel,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// The default model's thinking level. Options are the levels pi itself
    /// derives for the selected model (see `catalog_thinking_levels`); until a
    /// model is chosen the control is a disabled `pi default` chip.
    pub(super) fn default_thinking_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let Some(model) = self.default_model_entry() else {
            return ChipButton::new("default-thinking-select", ChipStyle::Outlined { inset: false })
                .size(SELECT_CHIP_SIZE)
                .selector("default-thinking-select")
                .label(tr!("settings.pi_default"))
                .caret(false)
                .disabled(true)
                .render(&theme);
        };
        let mut options: Vec<String> = vec![tr!("settings.pi_default")];
        options.extend(
            model
                .thinking_levels
                .iter()
                .map(|level| crate::model_selector::thinking_display(level)),
        );
        let selected = self
            .session_default
            .thinking
            .as_ref()
            .and_then(|level| model.thinking_levels.iter().position(|l| l == level))
            .map(|ix| ix + 1)
            .unwrap_or(0);
        self.select_control(
            "default-thinking-select",
            SettingsSelect::DefaultThinking,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// Orbit-only preference; existing pi queues are left untouched.
    fn composer_send_mode_toggle(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let mut group = ButtonGroup::new(SegmentFill::Active);
        for (index, mode) in [SendMode::FollowUp, SendMode::Steer]
            .into_iter()
            .enumerate()
        {
            let this = this.clone();
            group = group.segment(
                ("composer-send-mode", index),
                mode.label(),
                mode == theme.ui.composer_send_mode,
                move |_, _, cx| this.update(cx, |app, cx| app.set_composer_send_mode(mode, cx)),
            );
        }
        group.render(&theme).into_any_element()
    }

    /// Settings → General → Closing the window (#131): keep running with the
    /// tray / menu-bar icon, or quit.
    fn close_behavior_toggle(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        use crate::tray::CloseBehavior;
        let options = [
            (
                CloseBehavior::HideToTray,
                if cfg!(target_os = "macos") {
                    tr!("settings.keep_running_in_menu_bar")
                } else {
                    tr!("settings.hide_to_tray")
                },
            ),
            (CloseBehavior::Quit, tr!("settings.quit_on_close")),
        ];
        let current = theme.ui.close_behavior;
        let mut group = ButtonGroup::new(SegmentFill::Selected);
        for (index, (behavior, label)) in options.into_iter().enumerate() {
            let this = this.clone();
            group = group.segment(
                ("close-behavior", index),
                label,
                behavior == current,
                move |_, _, cx| this.update(cx, |app, cx| app.set_close_behavior(behavior, cx)),
            );
        }
        group.render(&theme).into_any_element()
    }

    fn set_close_behavior(&mut self, behavior: crate::tray::CloseBehavior, cx: &mut Context<Self>) {
        let mut ui = theme::get(cx).ui;
        ui.close_behavior = behavior;
        // `set_ui_prefs` persists the choice and puts the icon up or down.
        theme::set_ui_prefs(cx, ui);
        cx.notify();
    }

    fn set_composer_send_mode(&mut self, mode: SendMode, cx: &mut Context<Self>) {
        let mut ui = theme::get(cx).ui;
        ui.composer_send_mode = mode;
        theme::set_ui_prefs(cx, ui);
        cx.notify();
    }

    /// Settings → Agent → Check spelling. The composer reads the pref on every
    /// render, so an open draft picks the change up without a rebuild.
    fn toggle_spell_check(&mut self, cx: &mut Context<Self>) {
        let mut ui = theme::get(cx).ui;
        ui.spell_check = !ui.spell_check;
        theme::set_ui_prefs(cx, ui);
        cx.notify();
    }

    /// Appearance → Reduce motion: a three-way control, because a switch
    /// cannot express `Full` — the state that stops the OS from overruling the
    /// user's own answer.
    fn motion_preference_control(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let active = theme.ui.motion;
        let mut group = ButtonGroup::new(SegmentFill::Active);
        for (index, preference) in MotionPreference::ALL.into_iter().enumerate() {
            let this = this.clone();
            group = group.segment(
                ("motion-preference", index),
                motion_preference_label(preference),
                preference == active,
                move |_, _, cx| {
                    this.update(cx, |app, cx| app.set_motion_preference(preference, cx))
                },
            );
        }
        group.render(&theme).into_any_element()
    }

    /// Two-button segmented control for the follow-up delivery mode.
    pub(super) fn follow_up_mode_toggle(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        Self::delivery_mode_toggle(
            self.follow_up_mode == "all",
            ["follow-up-mode-one", "follow-up-mode-all"],
            Self::set_follow_up_mode,
            theme,
            this,
        )
    }

    /// Two-button segmented control for the steering delivery mode (#131):
    /// the same choice as follow-ups, for the lane that jumps the queue.
    pub(super) fn steering_mode_toggle(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        Self::delivery_mode_toggle(
            self.steering_mode == "all",
            ["steering-mode-one", "steering-mode-all"],
            Self::set_steering_mode,
            theme,
            this,
        )
    }

    /// `One at a time` / `All`: how pi drains one of its two message queues.
    fn delivery_mode_toggle(
        all: bool,
        ids: [&'static str; 2],
        set: fn(&mut OrbitApp, bool, &mut Context<OrbitApp>),
        theme: Theme,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let mut group = ButtonGroup::new(SegmentFill::Selected);
        for (label, value_all, id) in [
            (tr!("settings.one_at_a_time"), false, ids[0]),
            (tr!("settings.all"), true, ids[1]),
        ] {
            let this = this.clone();
            group = group.segment(id, label, all == value_all, move |_, _, cx| {
                this.update(cx, |app, cx| set(app, value_all, cx))
            });
        }
        group.render(&theme).into_any_element()
    }

    /// Update the follow-up mode locally and push it to pi.
    pub(super) fn set_follow_up_mode(&mut self, all: bool, cx: &mut Context<Self>) {
        let mode = if all { "all" } else { "one-at-a-time" }.to_string();
        self.follow_up_mode = mode.clone();
        self.send(CommandBody::SetFollowUpMode { mode }, "set_follow_up_mode");
        cx.notify();
    }

    /// Update the steering mode locally and push it to pi.
    pub(super) fn set_steering_mode(&mut self, all: bool, cx: &mut Context<Self>) {
        let mode = if all { "all" } else { "one-at-a-time" }.to_string();
        self.steering_mode = mode.clone();
        self.send(CommandBody::SetSteeringMode { mode }, "set_steering_mode");
        cx.notify();
    }

    /// A real toggle switch (accent when on), parameterized by its action.
    pub(super) fn settings_toggle(
        &self,
        id: &'static str,
        on: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: fn(&mut OrbitApp, &mut Context<OrbitApp>),
    ) -> AnyElement {
        switch(id, on, &theme, move |_, cx| {
            this.update(cx, action);
        })
    }

    /// Settings → Privacy: local-only controls such as session retention.
    pub(super) fn privacy_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        vec![
            self.settings_section(
                theme,
                &tr!("settings.session_retention"),
                vec![
                    self.setting_row(
                        theme,
                        &tr!("settings.session_retention_enabled"),
                        Some(&tr!("settings.session_retention_enabled_description")),
                        None,
                        Some(self.settings_toggle(
                            "session-retention-toggle",
                            self.retention.enabled,
                            theme,
                            this.clone(),
                            Self::toggle_session_retention,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.session_retention_action"),
                        Some(&tr!("settings.session_retention_action_description")),
                        None,
                        Some(self.retention_mode_select(theme, this.clone(), cx)),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.session_retention_age"),
                        Some(&tr!("settings.session_retention_age_description")),
                        None,
                        Some(self.retention_days_select(theme, this.clone(), cx)),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.session_retention_run"),
                        Some(&tr!("settings.session_retention_run_description")),
                        None,
                        Some(self.retention_run_button(theme, this.clone())),
                    ),
                ],
            ),
            // A background request Orbit makes on its own: say what it asks,
            // whom, and when, and let it be turned off.
            self.settings_section(
                theme,
                &tr!("settings.github_activity"),
                vec![self.setting_row(
                    theme,
                    &tr!("settings.github_checks"),
                    Some(&tr!("settings.github_checks_description")),
                    None,
                    Some(self.settings_toggle(
                        "github-checks-toggle",
                        self.git_activity_store.github_checks,
                        theme,
                        this.clone(),
                        Self::toggle_github_checks,
                    )),
                )],
            ),
            self.remembered_approvals_section(theme, this),
        ]
    }

    /// Settings → Privacy: the guard's "Always allow this tool" grants. The
    /// extension writes them and Orbit never needs to read them for anything but
    /// this row, so it is the only place a user can see or undo one.
    fn remembered_approvals_section(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        let remembered = crate::access::remembered_approvals();
        let detail = if remembered.is_empty() {
            tr!("settings.remembered_approvals_none")
        } else {
            tr!(
                "settings.remembered_approvals_list",
                tools = remembered.join(", ")
            )
        };
        let clear = Button::new("clear-remembered-approvals", ButtonVariant::Secondary)
            .size(ButtonSize::Large)
            .label(tr!("settings.remembered_approvals_clear"))
            .disabled(remembered.is_empty())
            .on_mouse_up(&theme, move |_, _, cx| {
                this.update(cx, |app, cx| app.clear_remembered_approvals(cx));
            });
        self.settings_section(
            theme,
            &tr!("settings.remembered_approvals"),
            vec![self.setting_row(
                theme,
                &tr!("settings.remembered_approvals"),
                Some(&detail),
                None,
                Some(clear.into_any_element()),
            )],
        )
    }

    /// Drop every remembered "Always allow this tool" grant. The guard reads the
    /// file on the next tool call, so live sessions re-arm without a restart.
    pub(super) fn clear_remembered_approvals(&mut self, cx: &mut Context<Self>) {
        match crate::access::clear_remembered_approvals() {
            Ok(0) => self.toast_info(tr!("settings.remembered_approvals_none")),
            Ok(count) => {
                self.toast_success(tr!("settings.remembered_approvals_cleared", count = count))
            }
            Err(error) => self.toast_error(error),
        }
        cx.notify();
    }

    /// Settings → Privacy: the retention action dropdown (archive / delete).
    fn retention_mode_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let options = vec![
            tr!("settings.session_retention_mode_archive"),
            tr!("settings.session_retention_mode_delete"),
        ];
        let selected = match self.retention.mode {
            crate::session_retention::RetentionMode::Archive => 0,
            crate::session_retention::RetentionMode::Delete => 1,
        };
        self.select_control(
            "session-retention-mode-select",
            SettingsSelect::RetentionMode,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// Settings → Privacy: the idle-threshold dropdown.
    fn retention_days_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        use crate::session_retention::DAY_PRESETS;
        let options: Vec<String> = DAY_PRESETS
            .iter()
            .map(|days| tr!("settings.session_retention_days", days = days))
            .collect();
        let selected = DAY_PRESETS
            .iter()
            .position(|days| *days == self.retention.days)
            .unwrap_or(0);
        self.select_control(
            "session-retention-days-select",
            SettingsSelect::RetentionDays,
            options[selected].clone(),
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// Settings → Privacy: run the policy immediately (the same pass the
    /// launch hook runs) and report the result as a toast.
    fn retention_run_button(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        Button::new("session-retention-run", ButtonVariant::Secondary)
            .size(ButtonSize::Large)
            .label(tr!("settings.session_retention_run"))
            .on_mouse_up(&theme, move |_, _, cx| {
                this.update(cx, |app, cx| {
                    // Deleting is the one mode that removes files; asking
                    // first is the same courtesy every other delete gets.
                    if app.retention.mode == crate::session_retention::RetentionMode::Delete {
                        app.request_confirm(
                            super::confirm_ui::PendingConfirm::RunSessionRetention,
                            cx,
                        );
                    } else {
                        app.run_session_retention(cx);
                    }
                });
            })
            .into_any_element()
    }

    /// Flip the session-retention policy and persist it. It takes effect on
    /// the next launch, or right away from "Clean up now".
    pub(super) fn toggle_session_retention(&mut self, cx: &mut Context<Self>) {
        self.retention.enabled = !self.retention.enabled;
        self.retention.persist();
        cx.notify();
    }

    /// Toggle auto-applying pi's RPC patches. Persisted; the patches run
    /// before the next launch, so an agent restart is needed to load a change.
    pub(super) fn toggle_rpc_patches(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.rpc_patches.enabled;
        crate::rpc_patches::set_enabled(enabled);
        self.rpc_patches.enabled = enabled;
        self.set_status(if enabled {
            "RPC patches enabled — restart the agent to load them".to_string()
        } else {
            "RPC patches disabled — pi keeps the current patches until it updates".to_string()
        });
        cx.notify();
    }

    pub(super) fn toggle_auto_compaction(&mut self, cx: &mut Context<Self>) {
        self.auto_compaction = !self.auto_compaction;
        self.send(
            CommandBody::SetAutoCompaction {
                enabled: self.auto_compaction,
            },
            "set_auto_compaction",
        );
        self.set_status(if self.auto_compaction {
            tr!("settings.auto_compaction_on")
        } else {
            tr!("settings.auto_compaction_off")
        });
        cx.notify();
    }

    pub(super) fn toggle_auto_retry(&mut self, cx: &mut Context<Self>) {
        self.auto_retry = !self.auto_retry;
        self.send(
            CommandBody::SetAutoRetry {
                enabled: self.auto_retry,
            },
            "set_auto_retry",
        );
        self.set_status(if self.auto_retry {
            tr!("settings.auto_retry_on")
        } else {
            tr!("settings.auto_retry_off")
        });
        cx.notify();
    }

    /// Appearance → Reduce motion: persist the three-way choice. Every paint
    /// site reads the resolved value through `Theme::reduce_motion`, so the
    /// change is live and `Full` overrides an OS that asks for reduced motion.
    pub(super) fn set_motion_preference(
        &mut self,
        preference: MotionPreference,
        cx: &mut Context<Self>,
    ) {
        let mut ui = theme::get(cx).ui;
        ui.motion = preference;
        theme::set_ui_prefs(cx, ui);
        self.set_status(motion_status(preference, theme::os_reduce_motion()));
        cx.notify();
    }

    pub(super) fn compact_now(&mut self, cx: &mut Context<Self>) {
        if self.rewind_transition_pending(cx) {
            return;
        }
        if self.is_compacting {
            return;
        }
        self.is_compacting = true;
        // The run-status strip shows the in-progress state.
        if !self.send(
            CommandBody::Compact {
                custom_instructions: None,
            },
            "compact",
        ) {
            self.is_compacting = false;
        }
        cx.notify();
    }

    pub(super) fn abort_retry(&mut self, cx: &mut Context<Self>) {
        // pi funnels `abort_retry` into `auto_retry_end { success: false,
        // finalError: "Retry cancelled" }` (pi-core `_finishCancelledRetry`).
        // Record the request — as Esc does — or that report reads as retries
        // that failed, and the banner says so for a cancel the user asked for.
        if self.retrying {
            self.abort_requested = true;
        }
        self.send(CommandBody::AbortRetry, "abort_retry");
        self.retrying = false;
        self.retry_detail = None;
        self.set_status(tr!("settings.retry_aborted"));
        cx.notify();
    }

    pub(super) fn rename_session(&mut self, cx: &mut Context<Self>) {
        // The fork reply resets the session under the popover; a rename now
        // would either land on the wrong branch or be wiped by that reset.
        if self.rewind_transition_pending(cx) {
            return;
        }
        if self.pending_session_rename.is_some()
            || (self.session_rename_target.is_none() && self.title_generation.is_some())
        {
            return;
        }
        // Enter while an IME candidate window is open confirms the candidate;
        // it must not commit a half-composed name. The click path is covered
        // by the same guard, since the preedit text is not a final answer.
        if self.session_name_input.read(cx).is_composing() {
            return;
        }
        let name = self.session_name_input.read(cx).text().trim().to_string();
        if name.is_empty() {
            self.toast_warning(tr!("settings.enter_session_name"));
            cx.notify();
            return;
        }
        if let Some(session) = self.session_rename_target.take() {
            // The sidebar may shorten a title. Confirming the prefill unchanged
            // must not replace the full persisted name with that preview.
            if name == session.title.trim() {
                self.cancel_session_name_edit(cx);
                return;
            }
            if self.current_session_path.as_ref() != Some(&session.path)
                || self.session_id.is_none()
            {
                let path = session.path.clone();
                self.cancel_session_name_edit(cx);
                self.switch_to_session(session, true, cx);
                if self.current_session_path.as_ref() == Some(&path) && self.runtime_is_live() {
                    self.pending_session_rename = Some(SessionRename { path, name });
                }
                return;
            }
        }
        self.apply_session_rename(name, cx);
    }

    pub(super) fn apply_session_rename(&mut self, name: String, cx: &mut Context<Self>) {
        if !self.send(
            CommandBody::SetSessionName { name: name.clone() },
            "set_session_name",
        ) {
            return;
        }
        // The header (and the open session's sidebar row) read this name
        // immediately; `session_info_changed` is not guaranteed on rename.
        self.session_name = Some(name.clone());
        self.current_title = Some(name.clone());
        if let Some(path) = &self.current_session_path {
            if let Some(session) = self.sessions.iter_mut().find(|s| &s.path == path) {
                session.title = name;
            }
        }
        // Flash a check beside the updated title in the details dock. Only
        // the timer for this exact stamp clears the confirmation.
        let saved_at = Instant::now();
        self.rename_saved_at = Some(saved_at);
        // The committed name is the title now; close the rename dialog.
        self.session_name_editing = false;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(RENAME_FEEDBACK).await;
            let _ = this.update(cx, |app, cx| {
                if app.rename_saved_at == Some(saved_at) {
                    app.rename_saved_at = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// A Runtime action button (Start / Stop / Restart).
    pub(super) fn runtime_button(
        &self,
        id: &'static str,
        label: &str,
        primary: bool,
        theme: Theme,
        this: Entity<OrbitApp>,
        action: fn(&mut OrbitApp, &mut Context<OrbitApp>),
    ) -> AnyElement {
        let variant = if primary {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Secondary
        };
        // The hover is a fade on the Interaction role, not a swap.
        Button::new(format!("runtime-button-{id}"), variant)
            .size(ButtonSize::Large)
            .label(label.to_string())
            .faded(&theme, move |button| {
                button.on_mouse_up(MouseButton::Left, move |_, _, cx| {
                    this.update(cx, action);
                })
            })
    }

    // ── Appearance ─────────────────────────────────────────────────────

    /// The Appearance section: Theme, Background, Type & density, Layout and
    /// Language. Theme and Background lead with what they paint — palette
    /// thumbnails and the wallpaper itself — so a choice reads on the page
    /// before you leave it.
    pub(super) fn appearance_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Vec<AnyElement> {
        let prefs = theme::appearance_prefs(cx);
        // One design system: every palette — the Orbit family, Flexoki and
        // Operational — sits in the same two galleries, one per appearance.
        // Each gallery remembers its own choice, so a System appearance has a
        // palette ready for either side.
        // One compact row per choice: the appearance as a segmented control,
        // and each palette and the accent as a dropdown whose trigger and
        // options carry a small preview of what they would paint.
        let in_use = tr!("settings.theme_in_use");
        let theme_rows = vec![
            self.setting_row(
                theme,
                &tr!("settings.appearance"),
                Some(&tr!("settings.appearance_mode_description")),
                None,
                Some(self.appearance_mode_toggle(theme, prefs, this.clone())),
            ),
            self.setting_row(
                theme,
                &tr!("settings.dark_theme"),
                None,
                (theme.mode == ThemeMode::Dark).then_some(in_use.as_str()),
                Some(self.theme_select(ThemeMode::Dark, theme, this.clone(), cx)),
            ),
            self.setting_row(
                theme,
                &tr!("settings.light_theme"),
                None,
                (theme.mode == ThemeMode::Light).then_some(in_use.as_str()),
                Some(self.theme_select(ThemeMode::Light, theme, this.clone(), cx)),
            ),
            self.setting_row(
                theme,
                &tr!("settings.accent_color"),
                Some(&tr!("settings.accent_color_description")),
                None,
                Some(self.accent_select(theme, this.clone(), cx)),
            ),
        ];
        let settings = crate::wallpaper::settings();
        let mut sections = vec![
            self.settings_section_desc(
                theme,
                &tr!("settings.theme"),
                Some(&tr!("settings.theme_section_description")),
                theme_rows,
            ),
            self.settings_section_desc(
                theme,
                &tr!("settings.background_section"),
                Some(&tr!("settings.background_section_description")),
                self.wallpaper_rows(theme, this.clone(), cx, &settings),
            ),
        ];
        // The tuning sliders only mean something once there is an image, so
        // the board appears with it instead of sitting inert under an empty
        // preview.
        if settings.desktop() {
            sections.push(self.settings_section(
                theme,
                &tr!("wallpaper.adjustments"),
                self.desktop_background_rows(theme, this.clone(), &settings),
            ));
        } else if settings.path.is_some() {
            sections.push(self.settings_section(
                theme,
                &tr!("wallpaper.adjustments"),
                self.wallpaper_adjustment_rows(theme, this.clone(), &settings),
            ));
        }
        sections.extend([
            self.settings_section(
                theme,
                &tr!("settings.type_and_density"),
                vec![
                    self.type_preview(theme),
                    self.setting_row(
                        theme,
                        &tr!("settings.interface_font"),
                        None,
                        None,
                        Some(self.font_family_select(
                            SettingsSelect::UiFontFamily,
                            theme,
                            this.clone(),
                            cx,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.code_font"),
                        None,
                        None,
                        Some(self.font_family_select(
                            SettingsSelect::CodeFontFamily,
                            theme,
                            this.clone(),
                            cx,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.ui_font_size"),
                        None,
                        None,
                        Some(self.preset_select(
                            SettingsSelect::UiFontSize,
                            theme,
                            this.clone(),
                            cx,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.terminal_size"),
                        None,
                        None,
                        Some(self.preset_select(
                            SettingsSelect::TerminalFont,
                            theme,
                            this.clone(),
                            cx,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.editor_size"),
                        None,
                        None,
                        Some(self.preset_select(
                            SettingsSelect::EditorFont,
                            theme,
                            this.clone(),
                            cx,
                        )),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.spacing_density"),
                        None,
                        None,
                        Some(self.preset_select(
                            SettingsSelect::SpacingDensity,
                            theme,
                            this.clone(),
                            cx,
                        )),
                    ),
                ],
            ),
            self.settings_section(
                theme,
                &tr!("settings.layout"),
                vec![
                    self.setting_row(
                        theme,
                        &tr!("settings.show_sidebar"),
                        Some(&tr!(
                            "settings.show_the_sessions_sidebar_also_toggleable_from_t",
                            shortcut = crate::platform::shortcuts::label(
                                crate::platform::shortcuts::SIDEBAR
                            )
                        )),
                        None,
                        Some(self.sidebar_toggle(theme, this.clone())),
                    ),
                    self.setting_row(
                        theme,
                        &tr!("settings.reduce_motion"),
                        Some(&tr!(
                            "settings.stop_looping_animations_spinners_and_the_running"
                        )),
                        // Name what is actually in force, so `System` on a Mac
                        // that asks for reduced motion is never a guess.
                        Some(&motion_status(theme.ui.motion, theme::os_reduce_motion())),
                        Some(self.motion_preference_control(theme, this.clone())),
                    ),
                ],
            ),
            self.settings_section(
                theme,
                &tr!("language.title"),
                vec![self.setting_row(
                    theme,
                    &tr!("language.interface"),
                    Some(&tr!("language.description")),
                    None,
                    Some(self.language_select(theme, this, cx)),
                )],
            ),
        ]);
        sections
    }

    /// Light / Dark / System as one segmented control.
    fn appearance_mode_toggle(
        &self,
        theme: Theme,
        prefs: theme::AppearancePrefs,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        use theme::AppearanceMode;
        let mut group = ButtonGroup::new(SegmentFill::Active);
        for (index, mode) in AppearanceMode::ALL.into_iter().enumerate() {
            let this = this.clone();
            let label = match mode {
                AppearanceMode::Light => tr!("settings.appearance_light"),
                AppearanceMode::Dark => tr!("settings.appearance_dark"),
                AppearanceMode::System => tr!("settings.appearance_system"),
            };
            group = group
                .segment(
                    ("appearance-mode", index),
                    label.to_string(),
                    mode == prefs.mode,
                    move |_, _, cx| this.update(cx, |app, cx| app.set_appearance_mode(mode, cx)),
                )
                .selector(format!("appearance-mode-{}", mode.as_str()));
        }
        group.render(&theme).into_any_element()
    }

    /// The palette dropdown for one appearance: every palette of `mode`, each
    /// option previewed by a small swatch painted with the current accent.
    fn theme_select(
        &self,
        mode: ThemeMode,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let current = theme::appearance_prefs(cx).theme(mode);
        let catalog = self.theme_catalog(mode);
        let selected = catalog.iter().position(|id| *id == current).unwrap_or(0);
        let (id, kind) = match mode {
            ThemeMode::Dark => ("dark-theme-select", SettingsSelect::DarkTheme),
            ThemeMode::Light => ("light-theme-select", SettingsSelect::LightTheme),
        };
        self.select_control(
            id,
            kind,
            current.label().to_string(),
            catalog.iter().map(|id| id.label().to_string()).collect(),
            selected,
            theme,
            this,
            cx,
        )
    }

    /// The accent dropdown: each option a dot of the accent it would give the
    /// palette on screen.
    fn accent_select(&self, theme: Theme, this: Entity<OrbitApp>, cx: &Context<Self>) -> AnyElement {
        let current = theme::appearance_prefs(cx).accent_color;
        let selected = theme::AccentColor::ALL
            .iter()
            .position(|color| *color == current)
            .unwrap_or(0);
        self.select_control(
            "accent-color-select",
            SettingsSelect::Accent,
            accent_label(current),
            theme::AccentColor::ALL.into_iter().map(accent_label).collect(),
            selected,
            theme,
            this,
            cx,
        )
    }

    /// The previews a dropdown's options carry, one per option in option
    /// order, or `None` for a dropdown without them.
    fn select_marks(&self, kind: SettingsSelect, cx: &App) -> Option<Vec<SelectMark>> {
        match kind {
            SettingsSelect::DarkTheme | SettingsSelect::LightTheme => {
                let prefs = theme::appearance_prefs(cx);
                let mode = if kind == SettingsSelect::DarkTheme {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                };
                Some(
                    self.theme_catalog(mode)
                        .into_iter()
                        .map(|id| SelectMark::palette(prefs.apply(Theme::for_id(id))))
                        .collect(),
                )
            }
            SettingsSelect::Accent => {
                let base = Theme::for_id(theme::get(cx).theme_id);
                Some(
                    theme::AccentColor::ALL
                        .into_iter()
                        .map(|color| SelectMark::Accent(color.apply(base).accent))
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// Remember `id` as the palette for `mode`. With an explicit Light or Dark
    /// appearance, a palette picked from the other gallery switches to that
    /// appearance, so a click always shows what was chosen. System keeps
    /// following the OS; the gallery's description says when it applies.
    pub(super) fn choose_palette(
        &mut self,
        mode: ThemeMode,
        id: theme::ThemeId,
        cx: &mut Context<Self>,
    ) {
        let mut prefs = theme::appearance_prefs(cx);
        prefs.set_theme(mode, id);
        if prefs.mode != theme::AppearanceMode::System {
            prefs.mode = match mode {
                ThemeMode::Light => theme::AppearanceMode::Light,
                ThemeMode::Dark => theme::AppearanceMode::Dark,
            };
        }
        if let Err(error) = theme::set_appearance_prefs(cx, prefs) {
            self.toast_error(tr!("settings.appearance_save_failed", error = error));
        }
        cx.notify();
    }

    /// Appearance → Background: the preview first, and with no image an empty
    /// state that offers the one action that matters. Once an image exists:
    /// the file, where it sits, and the artwork effect.
    /// One category of built-in backgrounds as small thumbnails: a gradient
    /// painted with the same gradient its image uses, an image from its
    /// bundled thumbnail. The one in use is ringed in the accent; arrows move
    /// within the row like the other radio groups.
    fn builtin_backgrounds(
        &self,
        category: crate::wallpaper_builtin::Category,
        theme: Theme,
        settings: &crate::wallpaper::WallpaperSettings,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        use crate::wallpaper_builtin::BuiltinBackground;
        let current = settings
            .path
            .as_deref()
            .and_then(BuiltinBackground::of_path);
        let choices: Vec<BuiltinBackground> = BuiltinBackground::in_category(category).collect();
        let count = choices.len();
        let keyboard_choices = choices.clone();
        let keyboard_this = this.clone();
        let group_id = format!("builtin-{}", category.id());
        settings_radio_group(div().id(ElementId::Name(group_id.clone().into())), &theme)
            .debug_selector(move || group_id.clone())
            // One line: the control slot sizes to its content, so a wrapping
            // row would collapse to its narrowest item and stack every
            // thumbnail over the rows below.
            .flex()
            .flex_row()
            .flex_nowrap()
            .flex_none()
            .items_center()
            .justify_end()
            .gap(DynamicSpacing::Base06.px(&theme))
            .on_key_down(move |event, _, cx| {
                let index = current
                    .and_then(|bg| keyboard_choices.iter().position(|b| *b == bg))
                    .unwrap_or(0);
                let Some(next) = radio_step(event.keystroke.key.as_str(), index, count, count)
                else {
                    return;
                };
                cx.stop_propagation();
                keyboard_this.update(cx, |app, cx| {
                    app.apply_builtin_background(keyboard_choices[next], cx);
                });
            })
            .children(choices.into_iter().map(|background| {
                let this = this.clone();
                let face = background_face(theme);
                let face = match (background.gradient(), background.thumbnail()) {
                    (Some(fill), _) => face.bg(fill),
                    (None, Some(path)) => face.child(
                        gpui::img(SharedString::from(path))
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    ),
                    (None, None) => face,
                };
                background_tile(
                    format!("builtin-{}", background.id()),
                    current == Some(background),
                    background.tooltip(),
                    face,
                    theme,
                    move |_, cx| {
                        this.update(cx, |app, cx| app.apply_builtin_background(background, cx));
                    },
                )
            }))
            .into_any_element()
    }

    /// "Your backgrounds": the library folder's pictures as the same tiles
    /// the built-in strips use, wrapping across the row's full width (there
    /// can be any number of them). The one in use is the picture whose name
    /// the current wallpaper carries, when that wallpaper is not a built-in.
    fn library_backgrounds(
        &self,
        theme: Theme,
        settings: &crate::wallpaper::WallpaperSettings,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let builtin = settings
            .path
            .as_deref()
            .and_then(crate::wallpaper_builtin::BuiltinBackground::of_path)
            .is_some();
        let current = (!builtin && settings.path.is_some())
            .then(|| settings.label.clone())
            .flatten()
            .and_then(|label| {
                self.wallpaper_library
                    .iter()
                    .position(|image| image.name == label)
            });
        let paths: Vec<PathBuf> = self
            .wallpaper_library
            .iter()
            .map(|image| image.path.clone())
            .collect();
        let count = paths.len();
        let keyboard_this = this.clone();
        settings_radio_group(div().id("wallpaper-library"), &theme)
            .debug_selector(|| "wallpaper-library".to_string())
            .mx(DynamicSpacing::Base12.px(&theme))
            .mb(DynamicSpacing::Base12.px(&theme))
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(DynamicSpacing::Base06.px(&theme))
            .on_key_down(move |event, _, cx| {
                let index = current.unwrap_or(0);
                let Some(next) = radio_step(event.keystroke.key.as_str(), index, count, count)
                else {
                    return;
                };
                cx.stop_propagation();
                let path = paths[next].clone();
                keyboard_this.update(cx, |app, cx| app.apply_library_background(path, cx));
            })
            .children(self.wallpaper_library.iter().enumerate().map(|(index, image)| {
                let this = this.clone();
                let path = image.path.clone();
                let face = background_face(theme);
                let face = if image.thumb_ready {
                    face.child(
                        gpui::img(image.thumb.clone())
                            .size_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    )
                } else {
                    // Until its thumbnail is made: a plain well, not a blank hole.
                    face.bg(theme.card_fill(theme.surface_secondary))
                };
                background_tile(
                    format!("library-{index}"),
                    current == Some(index),
                    image.name.clone(),
                    face,
                    theme,
                    move |_, cx| {
                        let path = path.clone();
                        this.update(cx, |app, cx| app.apply_library_background(path, cx));
                    },
                )
            }))
            .into_any_element()
    }

    fn wallpaper_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
        settings: &crate::wallpaper::WallpaperSettings,
    ) -> Vec<AnyElement> {
        let mut rows = vec![self.wallpaper_preview(theme, settings, this.clone())];
        if let Some(error) = crate::wallpaper::last_error() {
            rows.push(self.setting_row(theme, &tr!("wallpaper.error"), Some(&error), None, None));
        }
        for category in crate::wallpaper_builtin::Category::ALL {
            rows.push(self.setting_row(
                theme,
                &category.label(),
                None,
                None,
                Some(self.builtin_backgrounds(category, theme, settings, this.clone())),
            ));
        }
        let library_folder = crate::skills::display_path(&crate::wallpaper_library::dir());
        let library_description = if self.wallpaper_library.is_empty() {
            tr!("wallpaper.library_empty")
        } else {
            tr!("wallpaper.library_description")
        };
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.library"),
            Some(&library_description),
            Some(&library_folder),
            Some(self.runtime_button(
                "wallpaper-library-open",
                &tr!("wallpaper.library_open"),
                false,
                theme,
                this.clone(),
                OrbitApp::wallpaper_library_open,
            )),
        ));
        if !self.wallpaper_library.is_empty() {
            rows.push(self.library_backgrounds(theme, settings, this.clone()));
        }
        if settings.path.is_none() {
            return rows;
        }
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.image"),
            Some(&tr!("wallpaper.image_description")),
            settings.display_label(),
            Some(self.wallpaper_controls(theme, this.clone())),
        ));
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.placement"),
            Some(&tr!("wallpaper.mode_description")),
            None,
            Some(self.wallpaper_placement_control(theme, settings, this.clone())),
        ));
        let preset_app = this.clone();
        let apply_preset = Button::new("wallpaper-dithered-hero", ButtonVariant::Secondary)
            .label(tr!("wallpaper.apply_preset"))
            .focus(&self.wallpaper_preset_focus)
            .on_click(&theme, move |_, _, cx| {
                preset_app.update(cx, |app, cx| {
                    if let Some(next) = crate::wallpaper::settings().apply_dithered_hero() {
                        app.set_wallpaper(next, cx);
                    }
                });
            });
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.dithered_hero"),
            Some(&tr!("wallpaper.dithered_hero_description")),
            None,
            Some(apply_preset.into_any_element()),
        ));
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.effect"),
            Some(&tr!("wallpaper.effect_description")),
            None,
            Some(self.wallpaper_effect_select(theme, this.clone(), cx)),
        ));
        // Intensity and pixel size only mean something while an effect is
        // replacing the photograph, so the rows appear with the effect instead
        // of sitting inert next to `None`.
        if settings.effect != crate::wallpaper_fx::Effect::None {
            rows.push(self.setting_row(
                theme,
                &tr!("wallpaper.effect_intensity"),
                Some(&tr!("wallpaper.effect_intensity_description")),
                None,
                Some(self.wallpaper_slider(
                    WallpaperSlider::EffectIntensity,
                    theme,
                    this.clone(),
                    settings,
                )),
            ));
            rows.push(self.setting_row(
                theme,
                &tr!("wallpaper.effect_pixel_size"),
                Some(&tr!("wallpaper.effect_pixel_size_description")),
                None,
                Some(self.wallpaper_slider(WallpaperSlider::EffectScale, theme, this, settings)),
            ));
        }
        rows
    }

    /// Appearance → Image adjustments: how the image reads through the window
    /// and the panes. Shown only while an image exists; the pane controls
    /// only while it is placed somewhere.
    fn wallpaper_adjustment_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        settings: &crate::wallpaper::WallpaperSettings,
    ) -> Vec<AnyElement> {
        let mut rows = vec![self.setting_row(
            theme,
            &tr!("wallpaper.image_opacity"),
            Some(&tr!("wallpaper.image_opacity_description")),
            None,
            Some(self.wallpaper_slider(
                WallpaperSlider::ImageOpacity,
                theme,
                this.clone(),
                settings,
            )),
        )];
        if settings.mode == crate::wallpaper::WallpaperMode::Disabled {
            return rows;
        }
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.blur"),
            Some(&tr!("wallpaper.blur_description")),
            None,
            Some(self.wallpaper_slider(WallpaperSlider::Blur, theme, this.clone(), settings)),
        ));
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.card_opacity"),
            Some(&tr!("wallpaper.card_opacity_description")),
            None,
            Some(self.wallpaper_slider(
                WallpaperSlider::CardOpacity,
                theme,
                this.clone(),
                settings,
            )),
        ));
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.bottom_fade"),
            Some(&tr!("wallpaper.bottom_fade_description")),
            None,
            Some(self.settings_toggle(
                "wallpaper-bottom-fade",
                settings.bottom_fade,
                theme,
                this.clone(),
                |app, cx| {
                    let mut next = crate::wallpaper::settings();
                    next.bottom_fade = !next.bottom_fade;
                    app.set_wallpaper(next, cx);
                },
            )),
        ));
        if gpui::backdrop_blur_supported() {
            rows.push(self.setting_row(
                theme,
                &tr!("wallpaper.glass"),
                Some(&tr!("wallpaper.glass_description")),
                None,
                Some(self.settings_toggle(
                    "wallpaper-glass",
                    settings.glass,
                    theme,
                    this.clone(),
                    Self::toggle_wallpaper_glass,
                )),
            ));
        }
        rows.push(self.setting_row(
            theme,
            &tr!("wallpaper.derive_accent"),
            Some(&tr!("wallpaper.derive_accent_description")),
            None,
            Some(self.settings_toggle(
                "wallpaper-accent",
                settings.derive_accent,
                theme,
                this,
                Self::toggle_wallpaper_accent,
            )),
        ));
        rows
    }

    /// The wallpaper as it paints now, over the dot field. With no image the
    /// preview is the empty state and carries the Choose action; with an
    /// image that is placed nowhere it says so instead of showing dots alone.
    fn wallpaper_preview(
        &self,
        theme: Theme,
        settings: &crate::wallpaper::WallpaperSettings,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        let has_image = settings.path.is_some();
        let caption = |title: String, body: String| {
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .px(DynamicSpacing::Base24.px(&theme))
                .child(icon(
                    "icons/image.svg",
                    IconSize::Medium.px(&theme),
                    theme.text_muted,
                ))
                .child(
                    div()
                        .text_size(TextSize::Default.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(title),
                )
                .child(
                    div()
                        .text_size(TextSize::Small.px(&theme))
                        .text_color(theme.text_secondary)
                        .text_align(TextAlign::Center)
                        .whitespace_normal()
                        .child(body),
                )
        };
        let preview = div()
            .id("wallpaper-preview")
            .debug_selector(|| "wallpaper-preview".to_string())
            .relative()
            .w_full()
            .h(px(WALLPAPER_PREVIEW_H))
            .rounded(panel::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.hairline())
            .overflow_hidden()
            .when(settings.shows_bottom_fade(), |preview| {
                preview.bg(theme.background)
            })
            .when(!settings.shows_bottom_fade(), |preview| {
                preview.child(Self::dot_backdrop(theme))
            })
            .children(if settings.enabled() {
                crate::wallpaper::preview(theme)
            } else {
                None
            })
            .when(!has_image, |preview| {
                preview.child(
                    caption(tr!("wallpaper.empty_title"), tr!("wallpaper.empty_body")).child(
                        div()
                            .pt(DynamicSpacing::Base04.px(&theme))
                            .child(self.runtime_button(
                                "wallpaper-choose",
                                &tr!("settings.choose_image"),
                                true,
                                theme,
                                this,
                                OrbitApp::wallpaper_choose,
                            )),
                    ),
                )
            })
            .when(has_image && !settings.enabled(), |preview| {
                preview.child(caption(
                    tr!("wallpaper.hidden_title"),
                    tr!("wallpaper.hidden_body"),
                ))
            });
        div()
            .w_full()
            .p(DynamicSpacing::Base16.px(&theme))
            .child(preview)
            .into_any_element()
    }

    /// Replace or remove the image. Only shown once there is one; the empty
    /// preview carries the first Choose.
    fn wallpaper_controls(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(self.runtime_button(
                "wallpaper-choose",
                &tr!("settings.replace_ellipsis"),
                false,
                theme,
                this.clone(),
                OrbitApp::wallpaper_choose,
            ))
            .child(self.runtime_button(
                "wallpaper-reset",
                &tr!("wallpaper.reset_action"),
                false,
                theme,
                this,
                OrbitApp::wallpaper_reset,
            ))
            .into_any_element()
    }

    /// Where the image sits, as one keyboard-focusable segmented radio group:
    /// three short choices read faster side by side than in a dropdown.
    fn wallpaper_placement_control(
        &self,
        theme: Theme,
        settings: &crate::wallpaper::WallpaperSettings,
        this: Entity<OrbitApp>,
    ) -> AnyElement {
        use crate::wallpaper::WallpaperMode;
        let current = settings.mode;
        let keyboard_this = this.clone();
        let count = WallpaperMode::ALL.len();
        settings_radio_group(div().id("wallpaper-placement"), &theme)
            .flex()
            .items_center()
            .on_key_down(move |event, _, cx| {
                let index = WallpaperMode::ALL
                    .iter()
                    .position(|mode| *mode == current)
                    .unwrap_or(0);
                let Some(next) = radio_step(event.keystroke.key.as_str(), index, count, count)
                else {
                    return;
                };
                cx.stop_propagation();
                keyboard_this.update(cx, |app, cx| {
                    app.set_wallpaper_mode(WallpaperMode::ALL[next], cx);
                });
            })
            .child({
                let mut group = ButtonGroup::new(SegmentFill::Selected);
                for mode in WallpaperMode::ALL {
                    let this = this.clone();
                    group = group.segment(
                        ElementId::Name(format!("wallpaper-placement-{}", mode.as_str()).into()),
                        wallpaper_mode_label(mode),
                        current == mode,
                        move |_, _, cx| this.update(cx, |app, cx| app.set_wallpaper_mode(mode, cx)),
                    );
                }
                group.render(&theme)
            })
            .into_any_element()
    }

    /// The Desktop background's board: what still applies with no picture —
    /// how much paint sits over the frost, and the in-app glass. Where the OS
    /// cannot frost (a light palette, Reduce Transparency, Linux) it says so
    /// instead of offering knobs that would change nothing.
    fn desktop_background_rows(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        settings: &crate::wallpaper::WallpaperSettings,
    ) -> Vec<AnyElement> {
        if !crate::wallpaper::desktop_frost_available(&theme, crate::theme::os_reduce_transparency()) {
            return vec![self.setting_row(
                theme,
                &tr!("wallpaper.desktop_unavailable"),
                Some(&tr!("wallpaper.desktop_unavailable_description")),
                None,
                None,
            )];
        }
        let mut rows = vec![self.setting_row(
            theme,
            &tr!("wallpaper.card_opacity"),
            Some(&tr!("wallpaper.desktop_card_opacity_description")),
            None,
            Some(self.wallpaper_slider(
                WallpaperSlider::CardOpacity,
                theme,
                this.clone(),
                settings,
            )),
        )];
        if gpui::backdrop_blur_supported() {
            rows.push(self.setting_row(
                theme,
                &tr!("wallpaper.glass"),
                Some(&tr!("wallpaper.desktop_glass_description")),
                None,
                Some(self.settings_toggle(
                    "wallpaper-glass",
                    settings.glass,
                    theme,
                    this,
                    Self::toggle_wallpaper_glass,
                )),
            ));
        }
        rows
    }

    fn set_wallpaper_mode(
        &mut self,
        mode: crate::wallpaper::WallpaperMode,
        cx: &mut Context<Self>,
    ) {
        let mut settings = crate::wallpaper::settings();
        settings.mode = mode;
        self.set_wallpaper(settings, cx);
    }

    fn wallpaper_effect_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let settings = crate::wallpaper::settings();
        let selected = crate::wallpaper_fx::Effect::ALL
            .iter()
            .position(|effect| *effect == settings.effect)
            .unwrap_or(0);
        self.select_control(
            "wallpaper-effect-select",
            SettingsSelect::WallpaperEffect,
            wallpaper_effect_label(settings.effect),
            crate::wallpaper_fx::Effect::ALL
                .iter()
                .map(|effect| wallpaper_effect_label(*effect))
                .collect(),
            selected,
            theme,
            this,
            cx,
        )
    }

    fn wallpaper_slider(
        &self,
        kind: WallpaperSlider,
        theme: Theme,
        this: Entity<OrbitApp>,
        settings: &crate::wallpaper::WallpaperSettings,
    ) -> AnyElement {
        let value = kind.value(settings);
        let (lo, hi) = kind.range();
        let norm = ((value - lo) / (hi - lo)).clamp(0., 1.);
        let (slider_id, fill_id, knob_id) = kind.ids();
        let track_w = px(SLIDER_TRACK_W);
        let knob = px(SLIDER_KNOB);
        let usable = track_w - knob;
        let knob_left = usable * norm;
        let center_x = knob_left + knob / 2.;
        let track_top = px((SLIDER_ROW_H - SLIDER_TRACK_H) / 2.);
        let knob_top = px((SLIDER_ROW_H - SLIDER_KNOB) / 2.);
        let display = match kind {
            WallpaperSlider::Blur => format!("{value:.1}"),
            WallpaperSlider::EffectScale => {
                let value = if settings.effect == crate::wallpaper_fx::Effect::Dither {
                    value.round().max(1.0)
                } else {
                    value
                };
                if (value.fract()).abs() < 0.05 {
                    format!("{value:.0}×")
                } else {
                    format!("{value:.1}×")
                }
            }
            _ => format!("{}%", (value * 100.).round() as u32),
        };

        let events = canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _cx| {
                register_wallpaper_slider(this, kind, bounds, hitbox, window);
            },
        )
        .absolute()
        .inset_0()
        .cursor_pointer();

        div()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base08.px(&theme))
            .child(
                div()
                    .id(slider_id)
                    .debug_selector(move || slider_id.to_string())
                    .relative()
                    .group("wallpaper-slider")
                    .w(track_w)
                    .h(px(SLIDER_ROW_H))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(track_top)
                            .h(px(SLIDER_TRACK_H))
                            .rounded_full()
                            .bg(theme.surface_active)
                            .group_hover("wallpaper-slider", |s| s.bg(theme.surface_hover)),
                    )
                    .child(
                        div()
                            .id(fill_id)
                            .debug_selector(move || fill_id.to_string())
                            .absolute()
                            .left_0()
                            .top(track_top)
                            .h(px(SLIDER_TRACK_H))
                            .w(center_x)
                            .rounded_full()
                            .bg(theme.accent),
                    )
                    .child(
                        div()
                            .id(knob_id)
                            .debug_selector(move || knob_id.to_string())
                            .absolute()
                            .left(knob_left)
                            .top(knob_top)
                            .size(knob)
                            .rounded_full()
                            .border_1()
                            .border_color(theme.border_strong)
                            .bg(theme.toggle_knob)
                            .group_hover("wallpaper-slider", |s| s.border_color(theme.accent)),
                    )
                    .child(events),
            )
            .child(
                div()
                    .min_w(px(44.))
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .text_align(TextAlign::Right)
                    .child(display),
            )
            .into_any_element()
    }

    /// Frost the panes over the wallpaper, or show it sharp through them.
    pub(super) fn toggle_wallpaper_glass(&mut self, cx: &mut Context<Self>) {
        let mut settings = crate::wallpaper::settings();
        settings.glass = !settings.glass;
        self.set_wallpaper(settings, cx);
    }

    pub(super) fn toggle_wallpaper_accent(&mut self, cx: &mut Context<Self>) {
        let mut settings = crate::wallpaper::settings();
        settings.derive_accent = !settings.derive_accent;
        self.set_wallpaper(settings, cx);
    }

    fn set_accent_color(&mut self, color: theme::AccentColor, cx: &mut Context<Self>) {
        let mut prefs = theme::appearance_prefs(cx);
        prefs.accent_color = color;
        if let Err(error) = theme::set_appearance_prefs(cx, prefs) {
            self.toast_error(tr!("settings.appearance_save_failed", error = error));
        }
        cx.notify();
    }

    fn set_appearance_mode(&mut self, mode: theme::AppearanceMode, cx: &mut Context<Self>) {
        let mut prefs = theme::appearance_prefs(cx);
        prefs.mode = mode;
        if let Err(error) = theme::set_appearance_prefs(cx, prefs) {
            self.toast_error(tr!("settings.appearance_save_failed", error = error));
        }
        cx.notify();
    }

    /// Live previews of the chosen faces at their current sizes: the
    /// interface sample on the left, a mock terminal (chrome + prompt) on
    /// the right so the code/terminal font is legible at a glance without
    /// opening a session. Each is labelled as a preview.
    pub(super) fn type_preview(&self, theme: Theme) -> AnyElement {
        let column = |label: &str, body: AnyElement| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base06.px(&theme))
                .child(
                    div()
                        .text_size(TextSize::XSmall.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_muted)
                        .child(label.to_uppercase()),
                )
                .child(body)
                .into_any_element()
        };

        // ── interface sample ──
        let interface = div()
            .w_full()
            .flex_1()
            .px(DynamicSpacing::Base12.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .rounded(panel::RADIUS.px(&theme))
            .bg(theme.background)
            .flex()
            .flex_col()
            .justify_center()
            .gap(DynamicSpacing::Base06.px(&theme))
            .child(
                div()
                    .font_family(theme::ui_font_family())
                    .text_size(TextSize::Large.px(&theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(tr!("settings.new_task")),
            )
            .child(
                div()
                    .font_family(theme::ui_font_family())
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(tr!("settings.the_quick_brown_fox_jumps_over_the_lazy_dog")),
            );

        // ── terminal sample ──
        let prompt = |command: &str, cursor: bool| {
            let mut row = div()
                .flex()
                .items_center()
                .gap(DynamicSpacing::Base06.px(&theme))
                .font_family(theme::code_font_family())
                .text_size(theme.term_px(12.5))
                .child(div().text_color(theme.success).child("$"))
                .child(div().text_color(theme.code_text).child(command.to_string()));
            if cursor {
                row = row.child(
                    div()
                        .w(theme.term_px(7.))
                        .h(theme.term_px(14.))
                        .rounded_full()
                        .bg(theme.text_secondary),
                );
            }
            row
        };
        let terminal = div()
            .w_full()
            .flex_1()
            .rounded(panel::RADIUS.px(&theme))
            .border_1()
            .border_color(theme.hairline())
            .bg(theme.code_bg)
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .flex_none()
                    .h(px(26.))
                    .px(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .items_center()
                    .gap(DynamicSpacing::Base06.px(&theme))
                    .bg(theme.surface)
                    .border_b_1()
                    .border_color(theme.hairline())
                    .children(
                        [theme.error, theme.warning, theme.success]
                            .map(|color| div().size(px(7.)).rounded_full().bg(color)),
                    )
                    .child(
                        div()
                            .ml_1p5()
                            .font_family(theme::code_font_family())
                            .text_size(theme.term_px(10.5))
                            .text_color(theme.text_muted)
                            .child(tr!("settings.terminal_prompt")),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex_1()
                    .px(DynamicSpacing::Base12.px(&theme))
                    .py(DynamicSpacing::Base08.px(&theme))
                    .flex()
                    .flex_col()
                    .gap(DynamicSpacing::Base03.px(&theme))
                    .child(prompt("orbit-pi --session", false))
                    .child(
                        div()
                            .font_family(theme::code_font_family())
                            .text_size(theme.term_px(12.5))
                            .text_color(theme.text_secondary)
                            .child(tr!("settings.preview_prompt")),
                    )
                    .child(prompt("", true)),
            );

        div()
            .w_full()
            .px(DynamicSpacing::Base16.px(&theme))
            .py(DynamicSpacing::Base12.px(&theme))
            .flex()
            .gap(DynamicSpacing::Base16.px(&theme))
            .child(column(
                &tr!("settings.interface_preview"),
                interface.into_any_element(),
            ))
            .child(column(
                &tr!("settings.terminal_preview"),
                terminal.into_any_element(),
            ))
            .into_any_element()
    }

    // ── Wallpaper ──────────────────────────────────────────────────────

    /// Decode/process a wallpaper when the current setting has no matching
    /// cache entry. Called from `render`; `wallpaper::begin` is idempotent, so
    /// a frame that finds a load in flight does nothing.
    pub(super) fn ensure_wallpaper(&mut self, cx: &mut Context<Self>) {
        let light = crate::theme::get(cx).mode == crate::theme::ThemeMode::Light;
        let Some(job) = crate::wallpaper::begin(light) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let result = cx
                .background_executor()
                .spawn(async move { worker.prepare() })
                .await;
            let _ = this.update(cx, |app, cx| {
                let current_light = crate::theme::get(cx).mode == crate::theme::ThemeMode::Light;
                if crate::wallpaper::finish(job, result, current_light) {
                    // The extracted accent and the computed surface alphas
                    // are part of the theme, so the whole window repaints on
                    // one global update.
                    crate::theme::rebuild_theme(cx);
                    cx.notify();
                } else {
                    // Settings changed while the image decoded. Ask again for
                    // the new key instead of installing a stale result.
                    app.ensure_wallpaper(cx);
                }
            });
        })
        .detach();
    }

    /// Pick a wallpaper (native dialog), copy it into the managed directory,
    /// and process it off the UI thread.
    pub(super) fn wallpaper_choose(&mut self, cx: &mut Context<Self>) {
        let dialog = rfd::AsyncFileDialog::new()
            .set_title(tr!("wallpaper.choose_title"))
            .add_filter(
                tr!("settings.images_filter"),
                &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tiff"],
            );
        cx.spawn(async move |this, cx| {
            let Some(handle) = dialog.pick_file().await else {
                return;
            };
            let path = handle.path().to_path_buf();
            let _ = this.update(cx, |app, cx| app.import_wallpaper(path, cx));
        })
        .detach();
    }

    fn import_wallpaper(&mut self, source: PathBuf, cx: &mut Context<Self>) {
        self.import_wallpaper_from(source, None, cx);
    }

    /// Keep "Your backgrounds" current while Settings → Appearance shows it:
    /// the first time it is shown, start the folder watch and list the folder;
    /// after that, list it again whenever the watch reports a change. Changes
    /// made while Appearance is hidden wait in the watch's channel.
    pub(super) fn tick_wallpaper_library(&mut self, cx: &mut Context<Self>) {
        if !(self.settings_open && self.settings_section == SettingsSection::Appearance) {
            return;
        }
        if !self.wallpaper_library_started {
            self.wallpaper_library_started = true;
            self.wallpaper_library_watcher = crate::wallpaper_library::LibraryWatcher::start();
            self.refresh_wallpaper_library(cx);
        } else if self
            .wallpaper_library_watcher
            .as_ref()
            .is_some_and(crate::wallpaper_library::LibraryWatcher::take_dirty)
        {
            self.refresh_wallpaper_library(cx);
        }
    }

    /// List the library folder off the UI thread: first what is there with
    /// the thumbnails already cached, then again once the missing ones are
    /// made. A newer listing supersedes an older one still running.
    fn refresh_wallpaper_library(&mut self, cx: &mut Context<Self>) {
        self.wallpaper_library_generation += 1;
        let generation = self.wallpaper_library_generation;
        let dir = crate::wallpaper_library::dir();
        let thumbs = crate::wallpaper_library::thumbs_dir();
        cx.spawn(async move |this, cx| {
            let listed = {
                let (dir, thumbs) = (dir.clone(), thumbs.clone());
                cx.background_executor()
                    .spawn(async move { crate::wallpaper_library::list(&dir, &thumbs) })
                    .await
            };
            let current = this
                .update(cx, |app, cx| {
                    let current = app.wallpaper_library_generation == generation;
                    if current {
                        app.wallpaper_library = listed.clone();
                        cx.notify();
                    }
                    current
                })
                .unwrap_or(false);
            if !current {
                return;
            }
            let made = cx
                .background_executor()
                .spawn(async move {
                    let mut images = listed;
                    crate::wallpaper_library::make_thumbnails(&mut images, &thumbs);
                    images
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.wallpaper_library_generation == generation {
                    app.wallpaper_library = made;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Use a picture from the library: imported like a picked file, so the
    /// library's own file is only read, never moved or changed.
    pub(super) fn apply_library_background(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.import_wallpaper(path, cx);
    }

    /// Open the library folder in the file manager, creating it first.
    pub(super) fn wallpaper_library_open(&mut self, cx: &mut Context<Self>) {
        let dir = crate::wallpaper_library::dir();
        cx.background_executor()
            .spawn(async move {
                if std::fs::create_dir_all(&dir).is_ok() {
                    crate::platform::open_path_default(&dir);
                }
            })
            .detach();
    }

    /// Use a built-in background: render it into the managed folder, then
    /// import it like a picked file, labelled with its name.
    pub(super) fn apply_builtin_background(
        &mut self,
        background: crate::wallpaper_builtin::BuiltinBackground,
        cx: &mut Context<Self>,
    ) {
        self.import_wallpaper_from(background.file_path(), Some(background), cx);
    }

    fn import_wallpaper_from(
        &mut self,
        source: PathBuf,
        generate: Option<crate::wallpaper_builtin::BuiltinBackground>,
        cx: &mut Context<Self>,
    ) {
        let current = crate::wallpaper::settings();
        let light = crate::theme::get(cx).mode == crate::theme::ThemeMode::Light;
        self.set_status(tr!("wallpaper.importing"));
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some(background) = generate {
                        crate::wallpaper_builtin::write(background)?;
                    }
                    crate::wallpaper::import_and_prepare(&source, &current, light)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok((imported, prepared)) => {
                        let label = match generate {
                            Some(background) => background.label(),
                            None => imported.label.clone().unwrap_or_default(),
                        };
                        // Keep controls the user may have changed while the
                        // file decoded; only the path/label come from the
                        // import. If effect, blur, or appearance moved, the
                        // prepared key will not match and the normal loader
                        // re-runs the pass for the new settings.
                        let mut next = crate::wallpaper::settings();
                        next.path = imported.path;
                        next.label = match generate {
                            Some(background) => Some(background.label()),
                            None => imported.label,
                        };
                        // Picking an image means seeing it: off goes to Hero,
                        // and the image replaces the Desktop background.
                        match next.mode {
                            crate::wallpaper::WallpaperMode::Disabled => {
                                next.mode = crate::wallpaper::WallpaperMode::Hero;
                            }
                            crate::wallpaper::WallpaperMode::Desktop => {
                                next.mode = crate::wallpaper::WallpaperMode::FullWindow;
                            }
                            _ => {}
                        }
                        match crate::wallpaper::update(next.clone()) {
                            Ok(_) => {
                                // Publish the cache entry for the imported
                                // file before asking `render` to warm it; if
                                // the key does not match (a racing edit) the
                                // normal loader picks it up.
                                let current_light =
                                    crate::theme::get(cx).mode == crate::theme::ThemeMode::Light;
                                if current_light == light {
                                    crate::wallpaper::install(&next, Ok(prepared), light);
                                }
                                crate::theme::rebuild_theme(cx);
                                app.ensure_wallpaper(cx);
                                app.set_status(tr!("wallpaper.set", label = label));
                            }
                            Err(error) => {
                                app.toast_error(tr!("wallpaper.could_not_save", error = error));
                            }
                        }
                    }
                    Err(error) => app.toast_error(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn wallpaper_reset(&mut self, cx: &mut Context<Self>) {
        match crate::wallpaper::reset() {
            Ok(()) => {
                crate::theme::rebuild_theme(cx);
                self.set_status(tr!("wallpaper.reset"));
            }
            Err(error) => self.toast_error(tr!("wallpaper.could_not_save", error = error)),
        }
        cx.notify();
    }

    fn set_wallpaper(&mut self, next: crate::wallpaper::WallpaperSettings, cx: &mut Context<Self>) {
        match crate::wallpaper::update(next) {
            Ok(true) => {
                crate::theme::rebuild_theme(cx);
                self.ensure_wallpaper(cx);
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => self.toast_error(tr!("wallpaper.could_not_save", error = error)),
        }
    }

    /// The real sidebar toggle, wired to the same state as the top bar.
    pub(super) fn sidebar_toggle(&self, theme: Theme, this: Entity<OrbitApp>) -> AnyElement {
        switch(
            "settings-sidebar-toggle",
            self.sidebar_visible,
            &theme,
            move |window, cx| {
                this.update(cx, |app, cx| {
                    app.toggle_sidebar(window, cx);
                    cx.notify();
                });
            },
        )
    }

    /// The palettes a gallery lists, in the order it paints them: every
    /// palette of `mode` in [`theme::ThemeId::ALL`] order. Both the tiles and
    /// the gallery's keyboard handler read this, so an index always resolves
    /// to the palette it painted.
    pub(super) fn theme_catalog(&self, mode: ThemeMode) -> Vec<theme::ThemeId> {
        theme::themes_for(mode).collect()
    }

    // ── General-settings selects (language / font sizes) ──────────────

    /// The Language dropdown, listing every shipped locale (autonyms) plus
    /// `System`.
    pub(super) fn language_select(
        &self,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        use crate::theme::Language;
        let selected = Language::ALL
            .iter()
            .position(|language| *language == theme.ui.language)
            .unwrap_or(0);
        self.select_control(
            "language-select",
            SettingsSelect::Language,
            theme.ui.language.label(),
            Language::ALL
                .iter()
                .map(|language| language.label())
                .collect(),
            selected,
            theme,
            this,
            cx,
        )
    }

    /// The percentage / px preset dropdowns in the type & density board.
    pub(super) fn preset_select(
        &self,
        kind: SettingsSelect,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        use crate::theme::{FONT_SIZES, SPACING_DENSITIES};
        let (values, current, suffix): (Vec<f32>, f32, &str) = match kind {
            SettingsSelect::UiFontSize => (FONT_SIZES.to_vec(), theme.ui.ui_font_size, "px"),
            SettingsSelect::TerminalFont => {
                (FONT_SIZES.to_vec(), theme.ui.terminal_font_size, "px")
            }
            SettingsSelect::EditorFont => (FONT_SIZES.to_vec(), theme.ui.editor_font_size, "px"),
            SettingsSelect::SpacingDensity => (
                SPACING_DENSITIES.iter().map(|v| *v as f32).collect(),
                theme.ui.spacing_density as f32,
                "%",
            ),
            SettingsSelect::Language
            | SettingsSelect::SuggestionsModel
            | SettingsSelect::ReportModel
            | SettingsSelect::UiFontFamily
            | SettingsSelect::CodeFontFamily
            | SettingsSelect::WallpaperEffect
            | SettingsSelect::TitleModel
            | SettingsSelect::AgentCalls
            | SettingsSelect::DefaultModel
            | SettingsSelect::DefaultThinking
            | SettingsSelect::SubagentModel
            | SettingsSelect::RetentionMode
            | SettingsSelect::RetentionDays
            | SettingsSelect::AccessMode
            | SettingsSelect::DarkTheme
            | SettingsSelect::LightTheme
            | SettingsSelect::Accent => unreachable!(),
        };
        let selected = values
            .iter()
            .position(|v| (*v - current).abs() < 0.01)
            .unwrap_or(0);
        self.select_control(
            match kind {
                SettingsSelect::UiFontSize => "ui-font-size-select",
                SettingsSelect::TerminalFont => "terminal-font-select",
                SettingsSelect::EditorFont => "editor-font-select",
                SettingsSelect::SpacingDensity => "spacing-density-select",
                _ => "settings-select",
            },
            kind,
            format!("{} {}", current as u32, suffix),
            values
                .iter()
                .map(|v| format!("{} {}", *v as u32, suffix))
                .collect(),
            selected,
            theme,
            this,
            cx,
        )
    }

    /// The Interface / Code font-family dropdowns share GPUI's available families.
    pub(super) fn font_family_select(
        &self,
        kind: SettingsSelect,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let prefs = theme::font_prefs();
        let (id, current) = match kind {
            SettingsSelect::UiFontFamily => ("ui-font-family-select", prefs.ui_font_family),
            SettingsSelect::CodeFontFamily => ("code-font-family-select", prefs.code_font_family),
            _ => unreachable!(),
        };
        let choices = font_family_options(cx, current.as_ref());
        let selected = choices
            .iter()
            .position(|f| f == current.as_ref())
            .unwrap_or(0);
        self.select_control(
            id,
            kind,
            current.to_string(),
            choices,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// A select control: value chip + caret, dropdown below when open.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select_control(
        &self,
        id: &'static str,
        kind: SettingsSelect,
        label: String,
        options: Vec<String>,
        selected: usize,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.select_control_sized(
            id,
            kind,
            SELECT_CHIP_SIZE,
            label,
            options,
            selected,
            theme,
            this,
            cx,
        )
    }

    /// [`select_control`](Self::select_control) at an explicit chip size, so a
    /// toolbar control can choose its size independently of settings rows.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select_control_sized(
        &self,
        id: &'static str,
        kind: SettingsSelect,
        size: ButtonSize,
        label: String,
        options: Vec<String>,
        selected: usize,
        theme: Theme,
        this: Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> AnyElement {
        let open = self.settings_select == Some(kind);
        let model_provider = match kind {
            SettingsSelect::TitleModel
            | SettingsSelect::SuggestionsModel
            | SettingsSelect::ReportModel
                if selected == 0 =>
            {
                Some(self.model_provider.as_str())
            }
            SettingsSelect::TitleModel
            | SettingsSelect::SuggestionsModel
            | SettingsSelect::ReportModel
            | SettingsSelect::DefaultModel
            | SettingsSelect::SubagentModel
                if selected > 0 =>
            {
                self.available_models
                    .get(selected - 1)
                    .map(|model| model.provider.as_str())
            }
            _ => None,
        };
        let chip_this = this.clone();
        div()
            .flex()
            .flex_col()
            .items_end()
            // dropdown anchored below the chip when open
            .children(self.settings_select_popup(kind, size, options, selected, theme, &this, cx))
            .child(
                {
                    let chip = ChipButton::new(id, ChipStyle::Outlined { inset: false })
                        .size(size)
                        .label(label)
                        .open(open)
                        .selector(id);
                    let chip = match model_provider {
                        Some(provider) => chip.leading(icon_dyn(
                            provider_icon(provider),
                            context_menu::ICON.px(&theme),
                            provider_mark_color(provider, &theme, theme.text_muted),
                        )),
                        None => match self
                            .select_marks(kind, cx)
                            .and_then(|marks| marks.get(selected).copied())
                        {
                            Some(mark) => chip.leading(select_mark(mark, &theme)),
                            None => chip,
                        },
                    };
                    chip.on_mouse_up(&theme, move |_, window, cx| {
                        chip_this.update(cx, |app, cx| {
                            // A dismissal from this same click's mouse-down
                            // must not immediately re-open (see
                            // `toggle_workspace_menu`).
                            if app.menu_dismissed_at.swallows() {
                                return;
                            }
                            if app.settings_select == Some(kind) {
                                app.settings_select = None;
                            } else {
                                app.settings_select = Some(kind);
                                // Open with the cursor on the chosen option,
                                // scrolled into view in the list below.
                                app.settings_select_highlight = Some(selected);
                                app.settings_select_scroll
                                    .scroll_to_item(selected, ScrollStrategy::Center);
                                app.settings_select_query.clear();
                                app.settings_filter
                                    .update(cx, |filter, cx| filter.clear(cx));
                                let handle = app.settings_filter.read(cx).focus_handle(cx);
                                window.focus(&handle);
                            }
                            cx.notify();
                        });
                    })
                },
            )
            .into_any_element()
    }

    /// The open dropdown's option list, anchored below its chip.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn settings_select_popup(
        &self,
        kind: SettingsSelect,
        size: ButtonSize,
        options: Vec<String>,
        selected: usize,
        theme: Theme,
        this: &Entity<OrbitApp>,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let theme = theme.for_floating();
        if self.settings_select != Some(kind) {
            return None;
        }
        // Filter options, keeping the original index for click dispatch.
        // The cursor shares `settings_select_visible`, so the painted rows
        // and the keyboard cursor can never disagree about the query.
        let rows: Vec<(usize, String)> = self
            .settings_select_visible(&options, cx)
            .into_iter()
            .map(|ix| (ix, options[ix].clone()))
            .collect();
        let empty = rows.is_empty();
        let model_rows = matches!(
            kind,
            SettingsSelect::TitleModel
                | SettingsSelect::SuggestionsModel
                | SettingsSelect::ReportModel
                | SettingsSelect::DefaultModel
                | SettingsSelect::SubagentModel
        )
        .then(|| {
            self.available_models
                .iter()
                .map(|model| (model.provider.clone(), model.name.clone()))
                .collect::<Vec<_>>()
        });
        let active_provider = self.model_provider.clone();
        let active_model = self.model_label.clone();
        let marks = self.select_marks(kind, cx);
        let highlight = self
            .settings_select_highlight
            .filter(|ix| rows.iter().any(|(row_ix, _)| row_ix == ix))
            .or_else(|| rows.first().map(|(ix, _)| *ix));
        // Virtualized list — only the visible rows are laid out and painted.
        // The font-family dropdowns can have a thousand+ entries, and the
        // whole app re-renders on every scroll tick, so rendering every row
        // per frame is what made the theme/font selectors lag while scrolling.
        //
        // `uniform_list`'s Infer sizing reads the *available* height, and this
        // popup lives inside a 0×0 anchor div, so the available height is 0 and
        // the list collapses to nothing. Give it an explicit height computed
        // from the row count instead (one picker entry per row plus the
        // list's vertical padding, capped at the old 220 px max) — a real
        // viewport. Rows and height share `row_h`, so they cannot drift.
        let row_h = picker::entry_height(&theme);
        let list_pad_y = picker::list_padding_y(&theme);
        let list_h = (row_h * rows.len() as f32 + list_pad_y * 2.).min(px(220.));
        let list: AnyElement = if empty {
            div()
                .w_full()
                .py(list_pad_y)
                .child(
                    picker_entry(
                        div().debug_selector(|| "settings-select-empty".to_string()),
                        &theme,
                    )
                    .text_color(theme.text_muted)
                    .child(tr!("settings.no_matches")),
                )
                .into_any_element()
        } else {
            let this = this.clone();
            uniform_list(
                "settings-select-list",
                rows.len(),
                move |range, _window, _cx| {
                    let mut children = Vec::new();
                    for ix in range {
                        let (orig_ix, option) = &rows[ix];
                        let orig_ix = *orig_ix;
                        let selected_row = orig_ix == selected;
                        let highlighted_row = highlight == Some(orig_ix);
                        let this = this.clone();
                        let model = model_rows.as_ref().and_then(|models| {
                            orig_ix.checked_sub(1).and_then(|ix| models.get(ix))
                        });
                        let provider = model.map(|(provider, _)| provider.as_str()).or_else(|| {
                            (model_rows.is_some()
                                && matches!(
                                    kind,
                                    SettingsSelect::TitleModel | SettingsSelect::ReportModel
                                )
                                && orig_ix == 0)
                                .then_some(active_provider.as_str())
                        });
                        let primary = model.map_or(option.as_str(), |(_, name)| name.as_str());
                        let secondary = model
                            .map(|(provider, _)| crate::providers::provider_display_name(provider))
                            .or_else(|| {
                                (model_rows.is_some()
                                    && matches!(
                                        kind,
                                        SettingsSelect::TitleModel | SettingsSelect::ReportModel
                                    )
                                    && orig_ix == 0
                                    && !active_model.is_empty())
                                .then(|| active_model.clone())
                            });
                        // Every row is exactly one picker entry tall
                        // (`uniform_list` strides by the first row, and
                        // `list_h` uses the same `row_h`). The block shell
                        // fits the entry's inset margins inside the list.
                        let row_id =
                            ElementId::NamedInteger("settings-select-row".into(), orig_ix as u64);
                        children.push(
                            div().h(row_h).w_full().child(
                                picker_entry(
                                    div().id(row_id).debug_selector(move || {
                                        format!("settings-select-row-{orig_ix}")
                                    }),
                                    &theme,
                                )
                                .h(row_h)
                                .justify_between()
                                .cursor_pointer()
                                // The chosen option keeps the fill; the
                                // keyboard cursor rides the same wash.
                                .when(highlighted_row || selected_row, |row| {
                                    row.bg(theme.pressed_wash())
                                })
                                .when(!highlighted_row && !selected_row, |row| {
                                    row.hover_state(&theme)
                                })
                                .text_color(if selected_row || highlighted_row {
                                    theme.text_primary
                                } else {
                                    theme.text_secondary
                                })
                                .when_some(provider, |row, provider| {
                                    row.child(icon_dyn(
                                        provider_icon(provider),
                                        context_menu::ICON.px(&theme),
                                        provider_mark_color(provider, &theme, theme.text_muted),
                                    ))
                                })
                                .when_some(
                                    marks.as_ref().and_then(|marks| marks.get(orig_ix).copied()),
                                    |row, mark| row.child(select_mark(mark, &theme)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(primary.to_owned()),
                                )
                                .when_some(secondary, |row, provider| {
                                    row.child(
                                        div()
                                            .flex_none()
                                            .max_w(px(120.))
                                            .truncate()
                                            .text_size(TextSize::Small.px(&theme))
                                            .text_color(theme.text_muted)
                                            .child(provider),
                                    )
                                })
                                .when(selected_row, |row| {
                                    row.child(icon(
                                        "icons/check.svg",
                                        context_menu::ICON.px(&theme),
                                        theme.accent_text,
                                    ))
                                })
                                .on_mouse_up(
                                    MouseButton::Left,
                                    move |_, _, cx| {
                                        this.update(cx, |app, cx| {
                                            if app.apply_settings_select(kind, orig_ix, cx) {
                                                app.settings_select = None;
                                            }
                                            cx.notify();
                                        });
                                    },
                                ),
                            ),
                        );
                    }
                    children
                },
            )
            .w_full()
            .debug_selector(|| "settings-select-list".to_string())
            .h(list_h)
            .track_scroll(self.settings_select_scroll.clone())
            .py(list_pad_y)
            .into_any_element()
        };
        // An explicit width, not `min_w`: the list below is `w_full`, and a
        // percentage width inside an auto-width parent resolves to 0, which
        // laid every option row out at a negative width — an empty panel.
        let popup = popover_picker(
            div().debug_selector(|| "settings-select-popup".to_string()),
            &theme,
        )
        .w(px(360.))
        .dismiss_on_outside({
            let this = this.clone();
            move |_: &MouseDownEvent, _, cx: &mut App| {
                this.update(cx, |app, cx| {
                    if app.settings_select.take().is_some() {
                        // Arm the click-through guard so this same click's
                        // mouse-up on the chip cannot immediately re-open
                        // the dropdown.
                        app.menu_dismissed_at.arm();
                        cx.notify();
                    }
                });
            }
        })
        // The filter input carries the `Picker` context, so these ride
        // the same dispatch node as the model picker's: arrows move the
        // cursor, Enter chooses it, Escape (global) closes.
        .on_action({
            let this = this.clone();
            let options = options.clone();
            move |_: &crate::PickerSelectPrev, _, cx: &mut App| {
                this.update(cx, |app, cx| app.settings_select_step(&options, -1, cx));
            }
        })
        .on_action({
            let this = this.clone();
            let options = options.clone();
            move |_: &crate::PickerSelectNext, _, cx: &mut App| {
                this.update(cx, |app, cx| app.settings_select_step(&options, 1, cx));
            }
        })
        .on_action({
            let this = this.clone();
            let options = options.clone();
            move |_: &crate::PickerConfirm, _, cx: &mut App| {
                this.update(cx, |app, cx| app.settings_select_confirm(&options, cx));
            }
        })
        // Search field — filters the options below.
        .child(search_field(self.settings_filter.clone(), &theme))
        .child(list);
        // Anchor to the chip's top-right corner (via a zero-size point) and
        // drop the popup `popover::MENU_OFFSET` below the chip
        // (`SELECT_CHIP_SIZE`), right-aligned. `Window`
        // position mode (not `Local`) is deliberate: `anchored`'s switch-anchor
        // fit then flips the popup above the chip when the viewport bottom is
        // closer than the list is tall, instead of sliding it down over its own
        // chip (which made a second click land inside the popup). The
        // post-switch snap still clamps at the window edges as a backstop.
        Some(
            div()
                .absolute()
                .top_0()
                .right_0()
                .size(px(0.))
                .child(anchored_popover_in_window(
                    Corner::TopRight,
                    point(px(0.), size.height(&theme) + popover::MENU_OFFSET),
                    popup,
                    "anchored_popover_in_window-in-11",
                    &theme,
                ))
                .into_any_element(),
        )
    }

    /// Move the settings dropdown's keyboard cursor one visible option,
    /// clamped at the ends like the model picker. A query owns the list, so
    /// a cursor the filter dropped lands on the first (or last) match rather
    /// than doing nothing, and the list keeps it in view.
    pub(super) fn settings_select_step(
        &mut self,
        options: &[String],
        dir: isize,
        cx: &mut Context<Self>,
    ) {
        let visible = self.settings_select_visible(options, cx);
        let Some(next) = stepped_visible_position(&visible, self.settings_select_highlight, dir)
        else {
            return;
        };
        self.settings_select_highlight = Some(visible[next]);
        self.settings_select_scroll
            .scroll_to_item(next, ScrollStrategy::Center);
        cx.notify();
    }

    /// Choose the option under the settings dropdown's keyboard cursor (or
    /// the first match when the query filtered the cursor away) and close
    /// the popup.
    pub(super) fn settings_select_confirm(&mut self, options: &[String], cx: &mut Context<Self>) {
        let Some(kind) = self.settings_select else {
            return;
        };
        let visible = self.settings_select_visible(options, cx);
        let ix = self
            .settings_select_highlight
            .filter(|ix| visible.contains(ix))
            .or_else(|| visible.first().copied());
        let Some(ix) = ix else {
            return;
        };
        // A disclosure row ("All themes…") only reshapes the list, so the
        // dropdown stays open on what it just revealed.
        if self.apply_settings_select(kind, ix, cx) {
            self.settings_select = None;
        }
        cx.notify();
    }

    /// The original indices of `options` that survive the filter — the rows
    /// the popup actually paints, in display order.
    fn settings_select_visible(&self, options: &[String], cx: &Context<Self>) -> Vec<usize> {
        let needle = self.settings_filter.read(cx).text().to_lowercase();
        options
            .iter()
            .enumerate()
            .filter(|(_, option)| needle.is_empty() || option.to_lowercase().contains(&needle))
            .map(|(ix, _)| ix)
            .collect()
    }

    /// Apply a dropdown choice to the persisted UI customization.
    pub(super) fn apply_settings_select(
        &mut self,
        kind: SettingsSelect,
        ix: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        // Settings → Permissions owns four dropdowns; they live beside that
        // page's state rather than in this file.
        if self.apply_permission_select(kind, ix, cx) {
            return true;
        }
        match kind {
            SettingsSelect::UiFontFamily | SettingsSelect::CodeFontFamily => {
                let mut prefs = theme::font_prefs();
                let current = match kind {
                    SettingsSelect::UiFontFamily => &prefs.ui_font_family,
                    SettingsSelect::CodeFontFamily => &prefs.code_font_family,
                    _ => unreachable!(),
                };
                let choices = font_family_options(cx, current.as_ref());
                let Some(choice) = choices.get(ix) else {
                    return true;
                };
                match kind {
                    SettingsSelect::UiFontFamily => prefs.ui_font_family = choice.clone().into(),
                    SettingsSelect::CodeFontFamily => {
                        prefs.code_font_family = choice.clone().into()
                    }
                    _ => unreachable!(),
                }
                theme::set_font_prefs(prefs);
                return true;
            }
            SettingsSelect::SuggestionsModel => {
                let model = if ix == 0 {
                    None
                } else {
                    self.available_models
                        .get(ix - 1)
                        .map(|model| crate::auto_title::TitleModel {
                            provider: model.provider.clone(),
                            id: model.id.clone(),
                        })
                };
                self.suggestions.model = model;
                self.suggestions.persist_suggestions();
                self.clear_follow_up_suggestions();
                cx.notify();
                return true;
            }
            SettingsSelect::TitleModel => {
                let model = if ix == 0 {
                    None
                } else {
                    self.available_models
                        .get(ix - 1)
                        .map(|model| crate::auto_title::TitleModel {
                            provider: model.provider.clone(),
                            id: model.id.clone(),
                        })
                };
                self.auto_title.model = model;
                self.auto_title.persist();
                cx.notify();
                return true;
            }
            SettingsSelect::ReportModel => {
                if ix == 0 {
                    self.bug_report_model.model = None;
                } else if let Some(model) = self.available_models.get(ix - 1) {
                    self.bug_report_model.model = Some(crate::commit_model::CommitModel {
                        provider: model.provider.clone(),
                        id: model.id.clone(),
                    });
                } else {
                    // An unavailable saved model is shown for honesty, but
                    // clicking that row must not silently clear the choice.
                    return true;
                }
                self.bug_report_model.persist_report();
                cx.notify();
                return true;
            }
            SettingsSelect::AgentCalls => {
                self.agent_calls = crate::agent_call_display::AgentCallDisplay::from_index(ix);
                self.agent_calls.persist();
                cx.notify();
                return true;
            }
            SettingsSelect::DefaultModel => {
                // The trailing unavailable saved model is informational, not a reset.
                if ix > self.available_models.len() {
                    return true;
                }
                let model = if ix == 0 {
                    None
                } else {
                    self.available_models.get(ix - 1)
                };
                let slot = match model {
                    Some(model) => crate::session_defaults::SessionDefault {
                        provider: Some(model.provider.clone()),
                        model_id: Some(model.id.clone()),
                        // Keep the level only when the new model supports it;
                        // a bare level must not land on an arbitrary model.
                        thinking: self.session_default.thinking.clone().filter(|level| {
                            model
                                .thinking_levels
                                .iter()
                                .any(|candidate| candidate == level)
                        }),
                    },
                    None => crate::session_defaults::SessionDefault::default(),
                };
                self.set_default_model(slot);
                cx.notify();
                return true;
            }
            SettingsSelect::DefaultThinking => {
                let level = if ix == 0 {
                    None
                } else {
                    self.default_model_entry()
                        .and_then(|model| model.thinking_levels.get(ix - 1))
                        .cloned()
                };
                let mut slot = self.session_default.clone();
                slot.thinking = level;
                self.set_default_model(slot);
                cx.notify();
                return true;
            }
            SettingsSelect::SubagentModel => {
                // Index 0 inherits the session's model; the rest are this
                // machine's catalog, written as the file's `provider/id`.
                let value = match ix {
                    0 => String::new(),
                    _ => match self.available_models.get(ix - 1) {
                        Some(model) => format!("{}/{}", model.provider, model.id),
                        // A pinned model the catalog cannot resolve is shown
                        // for honesty; clicking that row must not rewrite it.
                        None => return true,
                    },
                };
                if let Some(editor) = self.subagent_editor.as_ref() {
                    let model = editor.model.clone();
                    model.update(cx, |input, cx| input.set_text(value, cx));
                }
                cx.notify();
                return true;
            }
            SettingsSelect::DarkTheme | SettingsSelect::LightTheme => {
                let mode = if kind == SettingsSelect::DarkTheme {
                    ThemeMode::Dark
                } else {
                    ThemeMode::Light
                };
                if let Some(id) = self.theme_catalog(mode).get(ix).copied() {
                    self.choose_palette(mode, id, cx);
                }
                return true;
            }
            SettingsSelect::Accent => {
                if let Some(color) = theme::AccentColor::ALL.get(ix).copied() {
                    self.set_accent_color(color, cx);
                }
                return true;
            }
            SettingsSelect::WallpaperEffect => {
                let effect = crate::wallpaper_fx::Effect::ALL
                    .get(ix)
                    .copied()
                    .unwrap_or_default();
                let mut settings = crate::wallpaper::settings();
                settings.effect = effect;
                self.set_wallpaper(settings, cx);
                return true;
            }
            SettingsSelect::RetentionMode => {
                self.retention.mode = match ix {
                    1 => crate::session_retention::RetentionMode::Delete,
                    _ => crate::session_retention::RetentionMode::Archive,
                };
                self.retention.persist();
                cx.notify();
                return true;
            }
            SettingsSelect::RetentionDays => {
                if let Some(days) = crate::session_retention::DAY_PRESETS.get(ix) {
                    self.retention.days = *days;
                    self.retention.persist();
                }
                cx.notify();
                return true;
            }
            _ => {}
        }
        use crate::theme::{Language, FONT_SIZES, SPACING_DENSITIES};
        let mut ui = theme::get(cx).ui;
        match kind {
            SettingsSelect::Language => {
                ui.language = Language::ALL.get(ix).copied().unwrap_or(Language::System);
            }
            SettingsSelect::UiFontSize => {
                ui.ui_font_size = FONT_SIZES.get(ix).copied().unwrap_or(14.);
            }
            SettingsSelect::TerminalFont => {
                ui.terminal_font_size = FONT_SIZES.get(ix).copied().unwrap_or(13.);
            }
            SettingsSelect::EditorFont => {
                ui.editor_font_size = FONT_SIZES.get(ix).copied().unwrap_or(13.);
            }
            SettingsSelect::SpacingDensity => {
                ui.spacing_density = SPACING_DENSITIES.get(ix).copied().unwrap_or(100);
            }
            SettingsSelect::SuggestionsModel
            | SettingsSelect::ReportModel
            | SettingsSelect::UiFontFamily
            | SettingsSelect::CodeFontFamily
            | SettingsSelect::WallpaperEffect
            | SettingsSelect::TitleModel
            | SettingsSelect::AgentCalls
            | SettingsSelect::DefaultModel
            | SettingsSelect::DefaultThinking
            | SettingsSelect::SubagentModel
            | SettingsSelect::RetentionMode
            | SettingsSelect::RetentionDays
            | SettingsSelect::AccessMode
            | SettingsSelect::DarkTheme
            | SettingsSelect::LightTheme
            | SettingsSelect::Accent => unreachable!(),
        }
        theme::set_ui_prefs(cx, ui);
        true
    }
}

// ── controller ────────────────────────────────────────────────────
impl OrbitApp {
    pub(super) fn open_settings(&mut self, cx: &mut Context<Self>) {
        self.refresh_updater(cx);
        self.settings_open = true;
        self.set_settings_section(SettingsSection::General, cx);
        self.sync_terminal_active(cx);
    }

    /// Settings → Providers & models, searched to `provider` when the
    /// catalog has it — the model picker's **Manage models…** (#131): the
    /// page then shows that provider's card with its models beneath. An
    /// unknown or missing provider clears the search instead of showing an
    /// empty page.
    pub(super) fn open_models_settings(
        &mut self,
        provider: Option<String>,
        cx: &mut Context<Self>,
    ) {
        // Switch before opening so a retained section is refreshed on re-entry.
        self.set_settings_section(SettingsSection::Providers, cx);
        if self.settings_section != SettingsSection::Providers {
            // An unsaved subagent edit asked to stay; its guard owns the
            // navigation from here.
            return;
        }
        let query = provider
            .filter(|p| {
                self.available_models
                    .iter()
                    .any(|model| &model.provider == p)
            })
            .unwrap_or_default();
        self.provider_filter
            .update(cx, |input, cx| input.set_text(query, cx));
        self.settings_open = true;
        self.sync_terminal_active(cx);
        cx.notify();
    }

    pub(super) fn on_open_settings(
        &mut self,
        _: &crate::OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_settings_back(window, cx);
        self.open_settings(cx);
    }

    fn focus_settings_back(&mut self, window: &mut Window, cx: &App) {
        if !self.settings_open {
            self.settings_return_focus.capture(&self.settings_back_focus, window, cx);
        }
        window.focus(&self.settings_back_focus);
    }

    /// App-menu “About Orbit Pi”: the same surface Settings → About owns, so
    /// there is one place that states the versions and upstream projects.
    pub(super) fn on_open_about(
        &mut self,
        _: &crate::OpenAbout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_settings_back(window, cx);
        self.refresh_updater(cx);
        self.settings_open = true;
        self.set_settings_section(SettingsSection::About, cx);
        self.sync_terminal_active(cx);
    }

    pub(super) fn on_settings_gear_click(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_open {
            self.close_settings_with_focus(window, cx);
        } else {
            self.focus_settings_back(window, cx);
            self.open_settings(cx);
        }
    }

    fn on_settings_back(
        &mut self,
        _: &gpui::ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_settings_with_focus(window, cx);
    }

    pub(super) fn close_settings_with_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut opener = std::mem::take(&mut self.settings_return_focus);
        if self.close_settings(cx) {
            self.return_focus(opener.take(), window, cx);
        } else {
            self.settings_return_focus = opener;
        }
    }

    fn on_settings_nav_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.modified() {
            return;
        }
        let index = self.settings_nav_cursor;
        let last = SETTINGS_SECTIONS.len() - 1;
        let next = match event.keystroke.key.as_str() {
            "up" => index.saturating_sub(1),
            "down" => (index + 1).min(last),
            "home" => 0,
            "end" => last,
            "enter" | "space" => {
                if !event.is_held {
                    self.activate_settings_nav_row(index, cx);
                }
                index
            }
            _ => return,
        };
        self.settings_nav_cursor = next;
        self.settings_nav_scroll.scroll_to_item(next);
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn activate_settings_nav_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let (section, _, _) = SETTINGS_SECTIONS[index];
        // The cursor follows the section that actually opened, never the row
        // that was clicked: a held navigation (Subagents asks before it drops
        // edits) leaves both the selection and the keyboard cursor where they
        // were, so the next arrow/Enter still reads the pane on screen.
        self.set_settings_section(section, cx);
        if self.settings_section == section && !self.settings_search.read(cx).text().is_empty() {
            self.settings_search.update(cx, |search, cx| search.clear(cx));
        }
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, cx: &mut Context<Self>) -> bool {
        if self.guard_subagent_navigation(subagents_ui::SubagentAction::CloseSettings, cx) {
            return false;
        }
        self.close_settings_unchecked(cx);
        true
    }

    pub(super) fn close_settings_unchecked(&mut self, cx: &mut Context<Self>) {
        self.subagent_editor = None;
        self.settings_open = false;
        self.settings_return_focus = Default::default();
        self.provider_editor = None;
        self.provider_key_editor = None;
        self.account_editor = None;
        self.sync_terminal_active(cx);
        cx.notify();
    }

    /// Switch sections, reloading the on-disk facts each page reads so CLI
    /// edits appear without a restart.
    pub(super) fn set_settings_section(
        &mut self,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) {
        if self.settings_open && self.settings_section == SettingsSection::Subagents {
            if section == SettingsSection::Subagents {
                return;
            }
            if self.guard_subagent_navigation(
                subagents_ui::SubagentAction::NavigateSettings(section),
                cx,
            ) {
                return;
            }
        }
        self.set_settings_section_unchecked(section, cx);
    }

    pub(super) fn set_settings_section_unchecked(
        &mut self,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) {
        if self.settings_section == SettingsSection::Subagents
            && section != SettingsSection::Subagents
        {
            self.subagent_editor = None;
        }
        self.settings_section = section;
        // A section is a new page: open it at its top, not at wherever the
        // previous page happened to be scrolled to.
        self.settings_content_scroll
            .set_offset(point(px(0.), px(0.)));
        if let Some(index) = SETTINGS_SECTIONS.iter().position(|entry| entry.0 == section) {
            self.settings_nav_cursor = index;
            self.settings_nav_scroll.scroll_to_item(index);
        }
        if section != SettingsSection::ReportBug
            && self.settings_select == Some(SettingsSelect::ReportModel)
        {
            self.settings_select = None;
        }
        self.provider_editor = None;
        self.provider_key_editor = None;
        self.account_editor = None;
        // The MCP add/edit modal belongs to the MCP page; leaving the page
        // discards it like the provider editors above.
        self.mcp_editor = None;
        // Likewise the Remote hosts form.
        self.remote_host_editor = None;
        self.remote_host_menu = None;
        self.providers_machine_menu = false;
        self.remote_key_editor = None;
        match section {
            SettingsSection::General => self.refresh_notification_auth(cx),
            SettingsSection::GitAccounts => self.refresh_git_accounts_for_workspace(cx),
            SettingsSection::Providers => {
                // Paint from cached state now; the file re-read (models.json,
                // auth.json) lands after this frame.
                self.reload_custom_providers_later(cx);
                self.refresh_auth();
                self.providers_section_opened(cx);
            }
            SettingsSection::Skills => self.refresh_skills(cx),
            SettingsSection::Subagents => self.refresh_subagents(cx),
            SettingsSection::Permissions => self.refresh_permissions(cx),
            SettingsSection::Plugins => {
                self.plugin_result = None;
                self.refresh_plugins(cx);
            }
            SettingsSection::Mcp => self.mcp_section_opened(cx),
            SettingsSection::RemoteHosts => self.remote_hosts_section_opened(cx),
            _ => {}
        }
        cx.notify();
    }

    /// The directory new tasks and discovery use — the workspace when one is
    /// selected, else the process cwd.
    pub(super) fn workspace_dir(&self) -> PathBuf {
        self.current_workspace
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default()
    }

    /// Re-read the user and project settings `packages` arrays.
    pub(super) fn refresh_plugins(&mut self, cx: &mut Context<Self>) {
        let (packages, error) = crate::plugins::discover(&self.workspace_dir());
        self.plugins = packages;
        self.plugins_error = error;
        cx.notify();
    }

    /// Install the source currently in the toolbar field.
    pub(super) fn plugin_install(&mut self, cx: &mut Context<Self>) {
        if self.plugin_action.is_some() {
            return;
        }
        let source = self.plugin_source_input.read(cx).text().trim().to_string();
        if source.is_empty() {
            self.toast_warning(tr!("settings.enter_package_source"));
            cx.notify();
            return;
        }
        self.plugin_run(PluginOp::Install, source, self.plugin_install_project, cx);
    }

    /// Remove an installed plugin after the confirm prompt answered.
    pub(super) fn plugin_remove_confirmed(
        &mut self,
        source: String,
        project: bool,
        cx: &mut Context<Self>,
    ) {
        self.plugin_run(PluginOp::Remove, source, project, cx);
    }

    /// Enable or disable one package by editing pi's settings file, then
    /// reload the list. The edit is a local file write, but it still runs off
    /// the UI thread so a slow disk never stalls a frame; pi applies the
    /// change on its next start or `/reload`.
    pub(super) fn plugin_toggle_enabled(
        &mut self,
        source: String,
        project: bool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if self.plugin_action.is_some() {
            return;
        }
        let scope = if project {
            crate::plugins::PackageScope::Project
        } else {
            crate::plugins::PackageScope::User
        };
        let verb = if enabled {
            tr!("settings.verb_enabling")
        } else {
            tr!("settings.verb_disabling")
        };
        self.plugin_action = Some(tr!(
            "settings.plugin_progress",
            verb = verb,
            source = source
        ));
        // The toast needs its own copy: `source` moves into the background
        // task below, while this clone outlives it for the result string.
        let toast_source = source.clone();
        let workspace = self.workspace_dir();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    crate::plugins::set_enabled(&source, scope, enabled, &workspace)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.plugin_action = None;
                match result {
                    Ok(()) => {
                        app.plugin_result = Some(if enabled {
                            tr!("settings.plugin_enabled", source = toast_source)
                        } else {
                            tr!("settings.plugin_disabled", source = toast_source)
                        });
                        app.refresh_plugins(cx);
                    }
                    Err(err) => app.set_error(err),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Run a plugin operation off the UI thread, then reload the list.
    fn plugin_run(&mut self, op: PluginOp, source: String, project: bool, cx: &mut Context<Self>) {
        if self.plugin_action.is_some() {
            return;
        }
        let verb = match op {
            PluginOp::Install => tr!("settings.verb_installing"),
            PluginOp::Update => tr!("settings.verb_updating"),
            PluginOp::Remove => tr!("settings.verb_removing"),
        };
        self.plugin_action = Some(tr!(
            "settings.plugin_progress",
            verb = verb,
            source = source
        ));
        self.plugin_result = None;
        // The fallback outcome, for the rare run whose output has no line.
        let done = tr!(
            "settings.plugin_progress_plain",
            verb = verb,
            source = source
        );
        let workspace = self.workspace_dir();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match op {
                        PluginOp::Install => crate::plugins::install(&source, project, &workspace),
                        PluginOp::Update => crate::plugins::update(&source, &workspace),
                        PluginOp::Remove => crate::plugins::remove(&source, project, &workspace),
                    }
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.plugin_action = None;
                match result {
                    Ok(output) => {
                        // The outcome lands on the page, in the toolbar's own
                        // status line: the user is looking here, and a corner
                        // toast for an in-place edit is noise.
                        app.plugin_result = Some(
                            output
                                .lines()
                                .rev()
                                .find(|line| !line.trim().is_empty())
                                .map(str::to_string)
                                .unwrap_or_else(|| tr!("settings.plugin_done", done = done)),
                        );
                        app.refresh_plugins(cx);
                    }
                    Err(err) => app.set_error(err),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Single dispatch point for every Plugins-page control.
    pub(super) fn apply_plugin_action(&mut self, action: PluginAction, cx: &mut Context<Self>) {
        match action {
            PluginAction::Install => self.plugin_install(cx),
            PluginAction::Update { source } => {
                self.plugin_run(PluginOp::Update, source, false, cx);
            }
            PluginAction::Remove {
                source,
                project,
                name,
            } => {
                self.request_confirm(
                    super::confirm_ui::PendingConfirm::RemovePlugin {
                        name,
                        source,
                        project,
                    },
                    cx,
                );
            }
            PluginAction::InstallOne { source, project } => {
                self.plugin_run(PluginOp::Install, source, project, cx);
            }
            PluginAction::ToggleEnabled {
                source,
                project,
                enabled,
            } => {
                self.plugin_toggle_enabled(source, project, enabled, cx);
            }
            PluginAction::SetScope { project } => {
                self.plugin_install_project = project;
                cx.notify();
            }
            PluginAction::RestartPi => self.provider_apply_credentials(cx),
            PluginAction::Refresh => {
                self.plugin_result = None;
                self.refresh_plugins(cx);
                self.check_plugin_updates_now(cx);
                self.toast_info(tr!("settings.reloaded_installed_plugins"));
                self.plugin_refresh_spin_until = Some(Instant::now() + PLUGIN_REFRESH_FEEDBACK);
                cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(PLUGIN_REFRESH_FEEDBACK)
                        .await;
                    let _ = this.update(cx, |app, cx| {
                        app.plugin_refresh_spin_until = None;
                        cx.notify();
                    });
                })
                .detach();
                cx.notify();
            }
        }
    }

    /// Apply the result of reading both provider files to state. Read errors
    /// are kept, not fatal, so a broken file can be seen and fixed rather than
    /// overwritten.
    fn apply_provider_reads(
        &mut self,
        custom: Result<Vec<CustomProvider>, String>,
        auth: Result<HashMap<String, providers::ProviderAuth>, String>,
    ) {
        match custom {
            Ok(list) => {
                self.custom_providers = list;
                self.custom_providers_error = None;
            }
            Err(err) => {
                self.custom_providers = Vec::new();
                self.custom_providers_error = Some(err);
            }
        }
        match auth {
            Ok(auth) => {
                self.provider_auth = auth;
                self.provider_auth_error = None;
            }
            Err(err) => {
                self.provider_auth = HashMap::new();
                self.provider_auth_error = Some(err);
            }
        }
    }

    /// Re-read `~/.pi/agent/models.json` and `~/.pi/agent/auth.json` on the
    /// calling thread. Used by explicit actions (Refresh, save/remove,
    /// credential load) where the result gates the next step.
    pub(super) fn reload_custom_providers(&mut self, cx: &mut Context<Self>) {
        let custom = providers::read_custom();
        let auth = super::auth_prompt::overlay_account_auth(
            providers::read_auth(),
            self.selected_auth_paths(),
        );
        self.apply_provider_reads(custom, auth);
        self.ensure_provider_metadata(cx);
        self.refresh_account_map();
        cx.notify();
    }

    /// The same re-read, but off the UI thread: the Providers page paints from
    /// cached state first and the fresh files land a frame later, so clicking
    /// the nav row never blocks on disk (no-blink page switch).
    pub(super) fn reload_custom_providers_later(&mut self, cx: &mut Context<Self>) {
        // Idempotent, already off-thread; kick it now so a first visit still
        // has metadata as soon as it arrives.
        self.ensure_provider_metadata(cx);
        let selection = self.provider_accounts.selection();
        let paths = self.selected_auth_paths();
        cx.spawn(async move |this, cx| {
            let (custom, auth) = cx
                .background_executor()
                .spawn(async move {
                    (
                        providers::read_custom(),
                        super::auth_prompt::overlay_account_auth(providers::read_auth(), paths),
                    )
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                // A slow read from the previous account must never repaint the new one.
                if app.provider_accounts.selection() != selection {
                    return;
                }
                app.apply_provider_reads(custom, auth);
                app.refresh_account_map();
                cx.notify();
            });
        })
        .detach();
    }

    /// Load pi's built-in catalog sizes and authoritative provider metadata
    /// once, off the UI thread (node introspection + ~600 KB of JSON parsing
    /// would otherwise hitch the first Providers render).
    pub(super) fn ensure_provider_metadata(&mut self, cx: &mut Context<Self>) {
        if self.provider_metadata_loaded {
            return;
        }
        self.provider_metadata_loaded = true;
        cx.spawn(async move |this, cx| {
            let (counts, metadata) = cx
                .background_executor()
                .spawn(async {
                    let counts = providers::builtin_catalog_counts().clone();
                    let metadata = providers::dynamic_providers()
                        .map(<[providers::DynamicProvider]>::to_vec)
                        .unwrap_or_default();
                    (counts, metadata)
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.provider_catalog_counts = counts;
                app.provider_metadata = metadata;
                cx.notify();
            });
        })
        .detach();
    }

    /// Refresh: re-query the running agent's catalog and re-read both files.
    pub(super) fn provider_refresh(&mut self, cx: &mut Context<Self>) {
        self.providers_refreshing = true;
        self.refresh_catalogs();
        self.reload_custom_providers(cx);
        self.refresh_auth();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(700))
                .await;
            let _ = this.update(cx, |app, cx| {
                app.providers_refreshing = false;
                cx.notify();
            });
        })
        .detach();
        self.toast_info(tr!("settings.refreshed_provider_catalog"));
        cx.notify();
    }

    /// Restart pi so a newly written credential is loaded. pi reads
    /// `auth.json` only at startup, so this is the "use it" step. Resumes the
    /// active session on the fresh process when one exists.
    pub(super) fn provider_apply_credentials(&mut self, cx: &mut Context<Self>) {
        if self.account_switching {
            self.toast_warning(tr!("accounts.switch_in_progress"));
            cx.notify();
            return;
        }
        // A fork owns the process until its reply lands; restarting now would
        // lose the branch and the answer that moves Orbit onto it.
        if self.rewind_transition_pending(cx) {
            return;
        }
        // Never yank the process out from under a live turn.
        if self.busy || self.transcript.is_streaming() {
            self.toast_warning(tr!("settings.finish_turn_before_restart"));
            cx.notify();
            return;
        }
        // A remote session's file lives on the host, not in the local list:
        // reattach to it there, the way a reconnect does.
        if self.remote.is_some() {
            self.drop_client();
            if !self.respawn_into_session(cx) {
                self.runtime_start(cx);
            }
            self.provider_auth_dirty = false;
            self.toast_success(tr!("settings.pi_restarted_credentials_loaded"));
            cx.notify();
            return;
        }
        let resume = self
            .current_session_path
            .clone()
            .and_then(|path| self.sessions.iter().find(|s| s.path == path).cloned());
        self.drop_client();
        self.current_session_path = None;
        if let Some(session) = resume {
            self.switch_to_session(session, false, cx);
        } else {
            self.runtime_start(cx);
        }
        self.provider_auth_dirty = false;
        self.reload_custom_providers(cx);
        self.toast_success(tr!("settings.pi_restarted_credentials_loaded"));
        cx.notify();
    }

    /// Open the API-key editor for a provider.
    pub(super) fn provider_key_open(
        &mut self,
        provider_id: String,
        provider_name: String,
        oauth: bool,
        note: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-key-input")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder("sk-…")
        });
        let focus = key.read(cx).focus_handle(cx);
        window.focus(&focus);
        self.provider_key_editor = Some(ProviderKeyEditor {
            provider_id,
            provider_name,
            oauth,
            note,
            key,
            error: None,
        });
        cx.notify();
    }

    /// Save the API key, then flag that pi needs a restart to load it.
    pub(super) fn provider_key_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.provider_key_editor.as_ref() else {
            return;
        };
        // A key field mid-composition still holds the preedit; ⏎/Save waits for
        // the IME rather than storing a half-composed key (issue #100).
        if crate::composer::any_composing(cx, &[&editor.key]) {
            return;
        }
        let id = editor.provider_id.clone();
        let name = editor.provider_name.clone();
        let key = editor.key.read(cx).text();
        if let Err(error) = self.account_auth_path(&id) {
            if let Some(editor) = self.provider_key_editor.as_mut() {
                editor.error = Some(error.to_string());
            }
            cx.notify();
            return;
        }
        if self.selected_account_id(&id).is_some() {
            if self.auth.support() != AuthSupport::Supported || self.client.is_none() {
                if let Some(editor) = self.provider_key_editor.as_mut() {
                    editor.error = Some(tr!("accounts.rpc_required"));
                }
                cx.notify();
                return;
            }
            if key.trim().is_empty() {
                if let Some(editor) = self.provider_key_editor.as_mut() {
                    editor.error = Some(tr!("providers.api_key_required"));
                }
                cx.notify();
                return;
            }
            self.save_profile_api_key(id, name, key, cx);
            return;
        }
        let result = providers::write_api_key(&id, &key);
        match result {
            Ok(()) => {
                self.provider_key_editor = None;
                self.provider_auth_dirty = true;
                self.reload_custom_providers(cx);
                let what = tr!("settings.api_key");
                self.toast_success(tr!("settings.key_saved_for", what = what, name = name));
            }
            Err(err) => {
                if let Some(editor) = self.provider_key_editor.as_mut() {
                    editor.error = Some(err);
                }
            }
        }
        cx.notify();
    }

    /// Sign in with OAuth by handing `pi /login <id>` to the user's terminal.
    pub(super) fn provider_oauth_login(
        &mut self,
        id: String,
        name: String,
        cx: &mut Context<Self>,
    ) {
        // A Terminal `pi /login` would write Default, not the selected profile.
        if self.selected_account_id(&id).is_some() {
            self.toast_warning(tr!("accounts.rpc_required"));
            cx.notify();
            return;
        }
        // The id reaches a shell script; only builtin ids are ever passed, but
        // validate anyway so a hand-edited file can never inject a command.
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        {
            self.toast_error(tr!("settings.refuse_invalid_login", id = id));
            cx.notify();
            return;
        }
        // This page acts on this Mac; a host's providers are signed in from
        // its own view (the Machine selector, `remote_providers_ui`).
        let command = format!("pi /login {id}");
        let done_hint = tr!("settings.finish_sign_in_terminal", name = name);
        match platform::open_terminal_command(&command) {
            Ok(()) => {
                self.provider_auth_dirty = true;
                self.toast_info(done_hint);
            }
            Err(err) => {
                self.toast_warning(tr!(
                    "settings.terminal_failed_hint",
                    error = err,
                    command = command
                ));
            }
        }
        cx.notify();
    }

    /// Sign out. With auth RPC available pi owns the credential store and
    /// removes it live; otherwise fall back to dropping the auth.json entry
    /// (env credentials are outside Orbit's reach and left alone).
    pub(super) fn provider_sign_out(&mut self, id: String, cx: &mut Context<Self>) {
        if let Err(error) = self.account_auth_path(&id) {
            self.provider_auth_error = Some(error.to_string());
            cx.notify();
            return;
        }
        if self.selected_account_id(&id).is_some()
            && (self.auth.support() != AuthSupport::Supported || self.client.is_none())
        {
            // Never delete Default as a fallback for a selected profile.
            self.provider_auth_error = Some(tr!("accounts.rpc_required"));
            cx.notify();
            return;
        }
        if self.selected_account_id(&id).is_some() {
            self.sign_out_profile(id, cx);
            return;
        }
        if self.auth.support() == AuthSupport::Supported {
            self.send(
                CommandBody::AuthLogout {
                    provider: id.clone(),
                },
                "auth.logout",
            );
            self.toast_info(tr!("settings.signing_out", id = id));
            cx.notify();
            return;
        }
        match providers::remove_auth(&id) {
            Ok(()) => {
                self.provider_auth_dirty = true;
                self.reload_custom_providers(cx);
                self.toast_success(tr!("settings.signed_out", id = id));
            }
            Err(err) => {
                self.provider_auth_error = Some(err);
            }
        }
        cx.notify();
    }

    /// Open the editor for an existing provider id, or a blank add form.
    pub(super) fn provider_editor_open(
        &mut self,
        provider_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = provider_id.as_ref().and_then(|id| {
            self.custom_providers
                .iter()
                .find(|provider| &provider.id == id)
        });
        let in_catalog = provider_id.as_ref().is_some_and(|id| {
            self.available_models
                .iter()
                .any(|model| &model.provider == id)
        });
        let id_text = provider_id.clone().unwrap_or_default();
        let name_text = existing
            .and_then(|provider| provider.name.clone())
            .unwrap_or_default();
        let base_url_text = existing
            .map(|provider| provider.base_url.clone())
            .unwrap_or_default();
        let api = existing
            .map(|provider| provider.api.clone())
            .filter(|api| !api.is_empty())
            .unwrap_or_else(|| {
                if in_catalog {
                    // Built-in: no override unless the user picks one.
                    String::new()
                } else {
                    "openai-completions".to_string()
                }
            });
        let models_text = existing
            .map(|provider| provider.model_ids.join(", "))
            .unwrap_or_default();
        let had_api_key = existing.is_some_and(|provider| provider.has_api_key);

        let id_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-editor-id")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder("my-gateway")
                .with_text(id_text)
        });
        let name_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-editor-name")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder_key("settings.optional_display_name")
                .with_text(name_text)
        });
        let base_url_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-editor-base-url")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder("https://api.example.com/v1")
                .with_text(base_url_text)
        });
        let api_key_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-editor-api-key")
                .with_key_context("Composer Picker")
                .with_max_lines(1)
                .with_wrap(false)
                .with_placeholder(if had_api_key {
                    "••••••••"
                } else {
                    "sk-…"
                })
        });
        let models_input = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id("provider-editor-models")
                .with_key_context("Composer Picker")
                .with_max_lines(4)
                .with_placeholder_key("settings.model_id_model_id_2")
                .with_text(models_text)
        });

        let focus = if provider_id.is_some() {
            name_input.read(cx).focus_handle(cx)
        } else {
            id_input.read(cx).focus_handle(cx)
        };
        window.focus(&focus);
        self.provider_editor = Some(ProviderEditor {
            original_id: provider_id,
            in_catalog,
            id: id_input,
            name: name_input,
            base_url: base_url_input,
            api_key: api_key_input,
            models: models_input,
            api,
            had_api_key,
            error: None,
            fetching: false,
            fetched: Vec::new(),
            _fetch: None,
        });
        cx.notify();
    }

    /// **Fetch models** in the provider editor (#131): ask the endpoint the
    /// user typed for its model list and add every id not yet in the field.
    /// The user prunes the list there, or turns models off later in
    /// Settings → Providers & models, which never deletes them.
    pub(super) fn provider_fetch_models(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.provider_editor.as_mut() else {
            return;
        };
        if editor.fetching {
            return;
        }
        let base_url = editor.base_url.read(cx).text();
        let Some(url) = providers::models_endpoint(&base_url, &editor.api) else {
            editor.error = Some(tr!("settings.fetch_models_unsupported"));
            cx.notify();
            return;
        };
        let api = editor.api.clone();
        let key = editor.api_key.read(cx).text();
        editor.fetching = true;
        editor.error = None;
        let fetch = cx.background_spawn(async move {
            providers::fetch_model_ids(&url, &api, Some(key.as_str()))
        });
        editor._fetch = Some(cx.spawn(async move |this, cx| {
            let result = fetch.await;
            this.update(cx, |app, cx| {
                let Some(editor) = app.provider_editor.as_mut() else {
                    return;
                };
                editor.fetching = false;
                match result {
                    Ok(ids) if ids.is_empty() => {
                        editor.error = Some(tr!("settings.fetch_models_empty"));
                    }
                    Ok(ids) => {
                        let merged =
                            providers::merge_model_ids(&editor.models.read(cx).text(), &ids);
                        editor
                            .models
                            .update(cx, |input, cx| input.set_text(merged, cx));
                        // The chips are the fetched list itself: click one to
                        // take it out again without retyping the field (#131).
                        editor.fetched = ids;
                    }
                    Err(error) => {
                        editor.error = Some(tr!("settings.fetch_models_failed", error = error));
                    }
                }
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    pub(super) fn provider_editor_cancel(
        &mut self,
        _: &crate::PickerCancel,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closed = self.provider_editor.take().is_some()
            | self.provider_key_editor.take().is_some()
            | self.account_editor.take().is_some()
            | self.provider_usage_open.take().is_some();
        if closed {
            cx.notify();
        }
    }

    pub(super) fn provider_editor_confirm(
        &mut self,
        _: &crate::PickerConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.provider_key_editor.is_some() {
            self.provider_key_save(window, cx);
        } else if self.account_editor.is_some() {
            self.account_save(window, cx);
        } else {
            self.provider_save(window, cx);
        }
    }

    /// Dismiss when the scrim (outside the card) is pressed.
    pub(super) fn provider_editor_scrim(
        &mut self,
        _: &MouseDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closed = self.provider_editor.take().is_some()
            | self.provider_key_editor.take().is_some()
            | self.account_editor.take().is_some()
            | self.provider_usage_open.take().is_some();
        if closed {
            cx.notify();
        }
    }

    /// Validate and write the editor's values to models.json.
    pub(super) fn provider_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.provider_editor.as_ref() else {
            return;
        };
        // Same rule as every other confirm: the fields hold the preedit while
        // an IME candidate window is open (issue #100).
        if crate::composer::any_composing(
            cx,
            &[
                &editor.id,
                &editor.name,
                &editor.base_url,
                &editor.api_key,
                &editor.models,
            ],
        ) {
            return;
        }
        let id = editor.id.read(cx).text().trim().to_string();
        let name = editor.name.read(cx).text().trim().to_string();
        let base_url = editor.base_url.read(cx).text().trim().to_string();
        let api_key = editor.api_key.read(cx).text();
        let api = editor.api.clone();
        let in_catalog = editor.in_catalog;
        let models: Vec<String> = editor
            .models
            .read(cx)
            .text()
            .split(['\n', ','])
            .map(|model| model.trim().to_string())
            .filter(|model| !model.is_empty())
            .collect();

        let error = if id.is_empty() {
            Some(tr!("settings.provider_id_required"))
        } else if !id
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
        {
            Some(tr!("settings.provider_id_invalid"))
        } else if !(base_url.is_empty()
            || base_url.starts_with("http://")
            || base_url.starts_with("https://"))
        {
            Some(tr!("settings.base_url_scheme"))
        } else if base_url.is_empty() && !in_catalog {
            Some(tr!("settings.base_url_required"))
        } else if models.is_empty() && providers::builtin(&id).is_none() {
            // Only a built-in provider may go without a model list — empty
            // there means "use pi's catalog". A custom provider needs at
            // least one id or the entry resolves to no models at all. Keyed
            // on the id rather than `in_catalog`, which also goes true once
            // the agent advertises a *custom* provider's models.
            Some(tr!("settings.add_model_id"))
        } else {
            None
        };

        if let Some(error) = error {
            if let Some(editor) = self.provider_editor.as_mut() {
                editor.error = Some(error);
            }
            cx.notify();
            return;
        }

        let key = (!api_key.trim().is_empty()).then_some(api_key.trim());
        match providers::write_provider(
            &id,
            (!name.is_empty()).then_some(name.as_str()),
            &base_url,
            &api,
            key,
            &models,
        ) {
            Ok(()) => {
                self.provider_editor = None;
                self.reload_custom_providers(cx);
                self.refresh_catalogs();
                self.toast_success(tr!("settings.provider_saved", id = id));
            }
            Err(err) => {
                if let Some(editor) = self.provider_editor.as_mut() {
                    editor.error = Some(err);
                }
            }
        }
        cx.notify();
    }

    /// Remove a provider entry from models.json.
    pub(super) fn provider_remove(&mut self, id: String, cx: &mut Context<Self>) {
        match providers::remove_provider(&id) {
            Ok(()) => {
                self.reload_custom_providers(cx);
                self.refresh_catalogs();
                self.toast_info(tr!("settings.provider_removed", id = id));
            }
            Err(err) => {
                self.custom_providers_error = Some(err);
            }
        }
        cx.notify();
    }
}

// ── Appearance pickers ────────────────────────────────────────────────────

/// The wallpaper preview's height; tall enough for the empty state's action.
const WALLPAPER_PREVIEW_H: f32 = 148.;

/// The shared frame of a settings radio group: one tab stop whose focus ring
/// wraps the whole group, with arrows choosing inside it. gpui 0.2 has no
/// `:focus-visible`, so a click must not move focus here or the ring would
/// stay behind after a mouse choice; the option's own `on_click` still fires.
/// The face of a background tile: the thumbnail's frame, at the strip's
/// 1.6 ratio on a Medium button height. The caller fills it.
fn background_face(theme: Theme) -> Div {
    let thumb_h = ButtonSize::Medium.height(&theme);
    div()
        .w(thumb_h * 1.6)
        .h(thumb_h)
        .rounded(Radius::Medium.px(&theme))
        .overflow_hidden()
        .border_1()
        .border_color(theme.hairline())
}

/// One background choice, built-in or from the library: the face in a
/// ring that takes the accent when it is the one in use.
fn background_tile(
    id: String,
    active: bool,
    tip: String,
    face: Div,
    theme: Theme,
    on_pick: impl Fn(&mut Window, &mut App) + 'static,
) -> AnyElement {
    let radius = Radius::Medium.px(&theme);
    let selector = id.clone();
    div()
        .id(ElementId::Name(id.into()))
        .debug_selector(move || selector.clone())
        .flex_none()
        .p(DynamicSpacing::Base02.px(&theme))
        .rounded(radius + DynamicSpacing::Base02.px(&theme))
        .border_1()
        .border_color(if active {
            theme.accent
        } else {
            gpui::transparent_black()
        })
        .when(!active, |ring| ring.hover_border(&theme))
        .cursor_pointer()
        .tip(tip)
        .on_click(move |_, window, cx| on_pick(window, cx))
        .child(face)
        .into_any_element()
}

fn settings_radio_group(el: Stateful<Div>, theme: &Theme) -> Stateful<Div> {
    el.focusable()
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded(button::RADIUS.px(theme))
        .p(DynamicSpacing::Base04.px(theme))
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

/// The option a radio-group key moves to: left/right step and wrap, up/down
/// step by a row of `columns` (and wrap like left/right when the whole group
/// is one row), home/end jump to the ends. `None` for keys the group ignores.
fn radio_step(key: &str, index: usize, count: usize, columns: usize) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let one_row = columns >= count;
    Some(match key {
        "left" => (index + count - 1) % count,
        "right" => (index + 1) % count,
        "up" if one_row => (index + count - 1) % count,
        "down" if one_row => (index + 1) % count,
        "up" => index.checked_sub(columns).unwrap_or(index),
        "down" if index + columns < count => index + columns,
        "down" => index,
        "home" => 0,
        "end" => count - 1,
        _ => return None,
    })
}

/// An accent choice's name, translated where the name is a word and kept as
/// the colour's own name otherwise.
/// A dropdown option's leading preview in Settings → Appearance.
#[derive(Clone, Copy)]
enum SelectMark {
    /// A palette: its canvas, its sidebar surface and its accent.
    Palette {
        canvas: gpui::Hsla,
        sidebar: gpui::Hsla,
        accent: gpui::Hsla,
    },
    /// An accent colour.
    Accent(gpui::Hsla),
}

impl SelectMark {
    fn palette(palette: Theme) -> Self {
        Self::Palette {
            canvas: palette.background,
            sidebar: palette.surface,
            accent: palette.accent,
        }
    }
}

/// Paint a [`SelectMark`] at the dropdown's icon size, so it lines up with the
/// provider marks other dropdowns lead with.
fn select_mark(mark: SelectMark, theme: &Theme) -> Div {
    let size = context_menu::ICON.px(theme);
    match mark {
        SelectMark::Palette {
            canvas,
            sidebar,
            accent,
        } => div()
            .flex_none()
            .w(size * 1.75)
            .h(size)
            .rounded(Radius::Small.px(theme))
            .border_1()
            .border_color(theme.hairline())
            .bg(canvas)
            .overflow_hidden()
            .flex()
            .items_center()
            .child(div().flex_none().w(size * 0.5).h_full().bg(sidebar))
            .child(
                div().flex_1().flex().justify_center().child(
                    div()
                        .size(size * 0.45)
                        .rounded_full()
                        .bg(accent),
                ),
            ),
        SelectMark::Accent(color) => div()
            .flex_none()
            .size(size * 0.85)
            .rounded_full()
            .border_1()
            .border_color(theme.hairline())
            .bg(color),
    }
}

fn accent_label(color: theme::AccentColor) -> String {
    use theme::AccentColor;
    match color {
        AccentColor::Default => tr!("settings.accent_default").to_string(),
        AccentColor::Blue => tr!("settings.accent_blue").to_string(),
        AccentColor::Coral => tr!("settings.accent_coral").to_string(),
        AccentColor::Orange => tr!("settings.accent_orange").to_string(),
        AccentColor::Lime => tr!("settings.accent_lime").to_string(),
        _ => color.label().to_string(),
    }
}

fn wallpaper_mode_label(mode: crate::wallpaper::WallpaperMode) -> String {
    match mode {
        crate::wallpaper::WallpaperMode::Disabled => tr!("wallpaper.mode_disabled"),
        crate::wallpaper::WallpaperMode::Hero => tr!("wallpaper.mode_hero"),
        crate::wallpaper::WallpaperMode::FullWindow => tr!("wallpaper.mode_full"),
        crate::wallpaper::WallpaperMode::Desktop => tr!("wallpaper.mode_desktop"),
    }
}

fn wallpaper_effect_label(effect: crate::wallpaper_fx::Effect) -> String {
    match effect {
        crate::wallpaper_fx::Effect::None => tr!("wallpaper.effect_none"),
        crate::wallpaper_fx::Effect::Dither => tr!("wallpaper.effect_dither"),
        crate::wallpaper_fx::Effect::Ascii => tr!("wallpaper.effect_ascii"),
        crate::wallpaper_fx::Effect::Halftone => tr!("wallpaper.effect_halftone"),
        crate::wallpaper_fx::Effect::Scanlines => tr!("wallpaper.effect_scanlines"),
    }
}

/// The wallpaper sliders' pointer handling: a press inside the track jumps
/// the value to that point and begins a scrub; moves keep updating even
/// outside the track; release ends it.
fn register_wallpaper_slider(
    this: Entity<OrbitApp>,
    kind: WallpaperSlider,
    bounds: Bounds<Pixels>,
    hitbox: Hitbox,
    window: &mut Window,
) {
    let scrubbing = Rc::new(Cell::new(false));
    window.on_mouse_event({
        let scrubbing = scrubbing.clone();
        let this = this.clone();
        move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Capture
                || event.button != MouseButton::Left
                || !hitbox.is_hovered(window)
            {
                return;
            }
            scrubbing.set(true);
            apply_wallpaper_slider_at(&this, kind, bounds, event.position.x, cx);
        }
    });
    window.on_mouse_event({
        let scrubbing = scrubbing.clone();
        let this = this.clone();
        move |event: &MouseMoveEvent, phase, _window, cx| {
            if phase != DispatchPhase::Capture || !scrubbing.get() {
                return;
            }
            apply_wallpaper_slider_at(&this, kind, bounds, event.position.x, cx);
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, _cx| {
        if phase != DispatchPhase::Capture
            || event.button != MouseButton::Left
            || !scrubbing.replace(false)
        {
            return;
        }
        window.refresh();
    });
}

fn apply_wallpaper_slider_at(
    this: &Entity<OrbitApp>,
    kind: WallpaperSlider,
    bounds: Bounds<Pixels>,
    x: Pixels,
    cx: &mut App,
) {
    let usable = (f32::from(bounds.size.width) - SLIDER_KNOB).max(1.);
    let norm =
        ((f32::from(x) - f32::from(bounds.origin.x) - SLIDER_KNOB / 2.) / usable).clamp(0., 1.);
    let (lo, hi) = kind.range();
    let value = lo + (hi - lo) * norm;
    let mut settings = crate::wallpaper::settings();
    kind.set(&mut settings, value);
    this.update(cx, |app, cx| app.set_wallpaper(settings, cx));
}

/// A keyboard chord painted as a keycap chip: the same 22px pill the command
/// palette uses, so a shortcut reads identically wherever Orbit shows one.
fn keycap(label: &str, theme: Theme) -> AnyElement {
    div()
        .h(ButtonSize::Default.height(&theme))
        .min_w(px(28.))
        .px(DynamicSpacing::Base06.px(&theme))
        .rounded(Radius::Large.px(&theme))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.overlay_strong)
        .text_size(TextSize::Small.px(&theme))
        .text_color(theme.text_muted)
        .child(label.to_string())
        .into_any_element()
}

/// Whether a catalog model survives the Models page's search, favorites, and
/// provider filters. Shared by the grid and the toolbar's live count so the
/// two can never disagree.
fn model_visible(
    model: &ModelEntry,
    needle: &str,
    favorites_only: bool,
    favorites: &crate::favorites::Favorites,
    provider: Option<&str>,
) -> bool {
    if provider.is_some_and(|provider| model.provider != provider) {
        return false;
    }
    if favorites_only && !favorites.contains(&model.provider, &model.id) {
        return false;
    }
    needle.is_empty()
        || model.name.to_lowercase().contains(needle)
        || model.id.to_lowercase().contains(needle)
        || model.provider.to_lowercase().contains(needle)
}

/// The position among `visible` that `dir` moves the settings dropdown's
/// keyboard cursor to: the current position ± 1, clamped at the ends. A
/// highlight the query filtered away lands on the first match going down,
/// the last going up. `None` when nothing is visible.
fn stepped_visible_position(
    visible: &[usize],
    highlight: Option<usize>,
    dir: isize,
) -> Option<usize> {
    if visible.is_empty() {
        return None;
    }
    Some(
        match highlight.and_then(|ix| visible.iter().position(|visible| *visible == ix)) {
            Some(pos) => (pos as isize + dir).clamp(0, visible.len() as isize - 1) as usize,
            None if dir >= 0 => 0,
            None => visible.len() - 1,
        },
    )
}

/// A Runtime detail value in the UI text color.
pub(crate) fn runtime_text(theme: Theme, text: String) -> AnyElement {
    div()
        .min_w_0()
        .truncate()
        .text_size(TextSize::Small.px(&theme))
        .text_color(theme.text_primary)
        .child(text)
        .into_any_element()
}

/// A Runtime detail value rendered as a path (monospace, dimmed).
pub(crate) fn runtime_path(theme: Theme, text: String) -> AnyElement {
    div()
        .min_w_0()
        .truncate()
        .font_family(theme::code_font_family())
        .text_size(theme.code_px(11.5))
        .text_color(theme.text_secondary)
        .child(text)
        .into_any_element()
}

/// A Runtime error value: the same two facts as [`error_state`], sized for a
/// settings row. It wraps instead of truncating — an error a user cannot read
/// is not an error message.
pub(crate) fn runtime_error(theme: Theme, text: String) -> AnyElement {
    div()
        .min_w_0()
        .text_size(TextSize::Small.px(&theme))
        .text_color(theme.error)
        .child(text)
        .into_any_element()
}

#[cfg(test)]
mod settings_select_tests {
    use super::{font_options_with_current, stepped_visible_position};

    #[test]
    fn font_options_include_all_families_and_missing_current_without_duplicates() {
        let available = ["Onest", ".ZedSans", "Onest", "Lilex"].map(str::to_owned);
        let options = font_options_with_current(&available, "Legacy Font");
        assert_eq!(options, [".ZedSans", "Legacy Font", "Lilex", "Onest"]);
        assert_eq!(
            font_options_with_current(&available, "Onest"),
            [".ZedSans", "Lilex", "Onest"]
        );
    }

    #[test]
    fn font_option_indices_match_filtered_keyboard_navigation() {
        let available = ["Beta", "Alpha", "Gamma"].map(str::to_owned);
        let options = font_options_with_current(&available, "Missing");
        let visible: Vec<_> = options
            .iter()
            .enumerate()
            .filter(|(_, family)| family.contains('a'))
            .map(|(index, _)| index)
            .collect();
        let position = stepped_visible_position(&visible, Some(0), 1).unwrap();
        let index = visible[position];
        assert_eq!(options[index], "Beta");
        assert_eq!(
            font_options_with_current(&available, "Missing")[index],
            "Beta"
        );
    }

    /// The cursor is an original option index; `visible` holds the indices
    /// the current query left in the list, in display order.
    #[test]
    fn stepping_starts_at_the_first_and_last_match() {
        let visible = [0, 3, 7];
        assert_eq!(stepped_visible_position(&visible, None, 1), Some(0));
        assert_eq!(stepped_visible_position(&visible, None, -1), Some(2));
    }

    #[test]
    fn stepping_clamps_at_the_ends() {
        let visible = [0, 3, 7];
        assert_eq!(stepped_visible_position(&visible, Some(0), -1), Some(0));
        assert_eq!(stepped_visible_position(&visible, Some(3), 1), Some(2));
        assert_eq!(stepped_visible_position(&visible, Some(7), 1), Some(2));
    }

    #[test]
    fn stepping_recovers_when_the_query_filtered_the_cursor_away() {
        let visible = [2, 5];
        assert_eq!(stepped_visible_position(&visible, Some(4), 1), Some(0));
        assert_eq!(stepped_visible_position(&visible, Some(4), -1), Some(1));
    }

    #[test]
    fn stepping_an_empty_list_stays_put() {
        assert_eq!(stepped_visible_position(&[], Some(0), 1), None);
        assert_eq!(stepped_visible_position(&[], None, -1), None);
    }
}

/// The palette galleries read one catalog per appearance — the Orbit family,
/// Flexoki and Operational together — and the tile order and the keyboard
/// handler both resolve through it.
#[cfg(test)]
mod theme_catalog_tests {
    use super::*;

    fn test_app(cx: &mut gpui::TestAppContext) -> Entity<OrbitApp> {
        cx.update(|cx| {
            cx.set_global(Theme::for_id(theme::ThemeId::Orbit));
            cx.set_global(theme::AppearancePrefs::default());
            cx.new(OrbitApp::new)
        })
    }

    #[gpui::test]
    fn one_catalog_lists_every_dark_palette_operational_included(cx: &mut gpui::TestAppContext) {
        let app = test_app(cx);
        let cx = cx.add_empty_window();
        cx.update(|_, cx| {
            app.update(cx, |app, _| {
                assert_eq!(
                    app.theme_catalog(ThemeMode::Dark),
                    vec![
                        theme::ThemeId::Orbit,
                        theme::ThemeId::OrbitDarker,
                        theme::ThemeId::OrbitCobalt,
                        theme::ThemeId::OrbitAzure,
                        theme::ThemeId::Flexoki,
                        theme::ThemeId::OrbitOnyx,
                        theme::ThemeId::Operational,
                    ]
                );
            });
        });
    }

    #[gpui::test]
    fn one_catalog_lists_every_light_palette_operational_included(cx: &mut gpui::TestAppContext) {
        let app = test_app(cx);
        let cx = cx.add_empty_window();
        cx.update(|_, cx| {
            app.update(cx, |app, _| {
                assert_eq!(
                    app.theme_catalog(ThemeMode::Light),
                    vec![
                        theme::ThemeId::OrbitLight,
                        theme::ThemeId::OrbitAzureLight,
                        theme::ThemeId::OrbitPaper,
                        theme::ThemeId::OrbitContrast,
                        theme::ThemeId::FlexokiLight,
                        theme::ThemeId::OperationalLight,
                    ]
                );
            });
        });
    }

    #[test]
    fn radio_keys_step_wrap_and_move_by_rows() {
        // One row (the appearance cards, the placement segments, the accent
        // swatches): every arrow steps and wraps.
        assert_eq!(radio_step("right", 2, 3, 3), Some(0));
        assert_eq!(radio_step("left", 0, 3, 3), Some(2));
        assert_eq!(radio_step("down", 1, 3, 3), Some(2));
        assert_eq!(radio_step("up", 0, 3, 3), Some(2));
        // A four-column gallery of seven: up/down move a row and stop at the
        // edge instead of wrapping into another column.
        assert_eq!(radio_step("down", 1, 7, 4), Some(5));
        assert_eq!(radio_step("down", 4, 7, 4), Some(4));
        assert_eq!(radio_step("up", 5, 7, 4), Some(1));
        assert_eq!(radio_step("up", 2, 7, 4), Some(2));
        assert_eq!(radio_step("right", 6, 7, 4), Some(0));
        assert_eq!(radio_step("home", 5, 7, 4), Some(0));
        assert_eq!(radio_step("end", 0, 7, 4), Some(6));
        // Keys the group does not own pass through.
        assert_eq!(radio_step("tab", 0, 7, 4), None);
        assert_eq!(radio_step("right", 0, 0, 4), None);
    }
}

#[cfg(test)]
mod model_filter_tests {
    use super::{model_visible, ModelEntry};
    use crate::favorites::Favorites;

    fn entry(provider: &str, id: &str, name: &str) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            name: name.into(),
            provider: provider.into(),
            context_window: None,
            thinking_levels: Vec::new(),
        }
    }

    #[test]
    fn search_matches_name_id_and_provider_case_insensitively() {
        // Callers hand the helper an already-lowercased needle.
        let favorites = Favorites::from_pairs(&[]);
        let model = entry("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5");
        assert!(model_visible(&model, "", false, &favorites, None));
        assert!(model_visible(&model, "sonnet", false, &favorites, None));
        assert!(model_visible(
            &model,
            "claude-sonnet",
            false,
            &favorites,
            None
        ));
        assert!(model_visible(&model, "anthropic", false, &favorites, None));
        assert!(!model_visible(&model, "gemini", false, &favorites, None));
    }

    #[test]
    fn favorites_only_keeps_starred_models_and_still_searches() {
        let favorites = Favorites::from_pairs(&[("openai", "gpt-5")]);
        let starred = entry("openai", "gpt-5", "GPT-5");
        let other = entry("openai", "gpt-5-mini", "GPT-5 mini");
        assert!(model_visible(&starred, "", true, &favorites, None));
        assert!(!model_visible(&other, "", true, &favorites, None));
        // The search still applies inside the favorites scope.
        assert!(!model_visible(&starred, "mini", true, &favorites, None));
    }

    #[test]
    fn provider_filter_scopes_the_grid_and_composes_with_search() {
        let favorites = Favorites::from_pairs(&[]);
        let opus = entry("anthropic", "claude-opus-4", "Claude Opus 4");
        let sonnet = entry("anthropic", "claude-sonnet-4-5", "Claude Sonnet 4.5");
        let gpt = entry("openai", "gpt-5", "GPT-5");

        // A provider scope admits only that provider's models.
        assert!(model_visible(
            &opus,
            "",
            false,
            &favorites,
            Some("anthropic")
        ));
        assert!(model_visible(
            &sonnet,
            "",
            false,
            &favorites,
            Some("anthropic")
        ));
        assert!(!model_visible(
            &gpt,
            "",
            false,
            &favorites,
            Some("anthropic")
        ));
        // The search still applies inside the provider scope.
        assert!(!model_visible(
            &sonnet,
            "opus",
            false,
            &favorites,
            Some("anthropic")
        ));
        // `None` means every provider.
        assert!(model_visible(&gpt, "", false, &favorites, None));
    }
}

#[cfg(test)]
mod about_banner_tests {
    use super::*;

    /// The identity header is one compact block — mark, name, version — and the
    /// board starts right after it.
    ///
    /// It regressed once. The text column was a flex item in a *row* with no
    /// growth, so it was sized to its max-content; for a nested flex column that
    /// is its narrowest child, the 98px title. The subtitle then wrapped into
    /// five lines and the version line into three, the header grew to ~1500px,
    /// and the board — the only thing on this page a reader can act on — began
    /// 1,600px down a scrolling page in a 640px window.
    #[gpui::test]
    fn the_about_identity_header_stays_compact(cx: &mut gpui::TestAppContext) {
        use crate::theme::{Theme, ThemeId};

        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.open_settings(cx);
                app.set_settings_section(SettingsSection::About, cx);
            })
        });
        let viewport = gpui::size(px(960.), px(640.));
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        let title = cx.debug_bounds("settings-page-title").expect("the name");
        let mark = cx.debug_bounds("about-identity-mark").expect("the mark");
        let text = cx
            .debug_bounds("about-identity-text")
            .expect("the identity text");
        let version = cx.debug_bounds("about-version").expect("the version line");
        let board = cx.debug_bounds("about-github").expect("the board");

        assert_eq!(
            mark.size,
            gpui::size(px(48.), px(48.)),
            "the mark has a definite box, not one the decoded asset supplies"
        );
        assert!(
            (mark.origin.y + mark.size.height / 2. - (text.origin.y + text.size.height / 2.))
                .abs()
                < px(1.),
            "the mark is centred on the identity text: {mark:?} beside {text:?}"
        );
        assert!(
            text.size.width > title.size.width * 4.,
            "the identity text takes the row's width, not the title's: {text:?} against {title:?}"
        );
        assert!(
            version.size.height < px(30.),
            "the version line is one line, not a wrapped column: {version:?}"
        );
        assert!(
            board.origin.y < viewport.height,
            "the board is on the first screen: {board:?} in {viewport:?}"
        );
    }

    /// Settings → About no longer stacks a star card over the board: the
    /// Star control lives on the Source row beside the GitHub link
    /// (`repo_star::tests::about_shows_identity_and_star_beside_github`).
    #[gpui::test]
    fn about_page_has_no_star_banner(cx: &mut gpui::TestAppContext) {
        use crate::theme::{Theme, ThemeId};

        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let app = cx.update(|_, cx| cx.new(OrbitApp::new));
        cx.update(|_, cx| {
            app.update(cx, |app, cx| {
                app.open_settings(cx);
                app.set_settings_section(SettingsSection::About, cx);
            })
        });
        let viewport = cx.update(|window, _| window.viewport_size());
        let _ = cx.draw(point(px(0.), px(0.)), viewport, |_, _| app.clone());

        assert!(cx.debug_bounds("about-star-banner").is_none());
        assert!(cx.debug_bounds("about-star-toggle").is_some());
    }
}

#[cfg(test)]
mod settings_search_tests {
    use super::{settings_search_hits, SettingsSection};

    #[test]
    fn empty_query_matches_nothing() {
        assert!(settings_search_hits("").is_empty());
    }

    #[test]
    fn label_and_keyword_matches_span_sections() {
        // A translated label match (Auto-compaction contains "compaction").
        assert!(settings_search_hits("compaction")
            .iter()
            .any(|(section, key, _)| *section == SettingsSection::Agent
                && *key == "settings.auto_compaction"));
        // A keyword synonym the label does not spell out.
        assert!(settings_search_hits("banner")
            .iter()
            .any(|(section, _, _)| *section == SettingsSection::General));
        // A section-level keyword resolves to its page.
        assert!(settings_search_hits("protocol")
            .iter()
            .any(|(section, _, _)| *section == SettingsSection::Mcp));
        // The app-wide reload control is reachable from Settings search.
        assert!(settings_search_hits("reload")
            .iter()
            .any(|(section, key, _)| *section == SettingsSection::General
                && *key == "settings.reload_app"));
    }

    #[test]
    fn no_match_is_empty() {
        assert!(settings_search_hits("zzzzzz").is_empty());
    }
}

#[cfg(test)]
mod select_mark_tests {
    use super::*;
    use crate::theme::ThemeId;

    /// A palette option previews that palette: its own canvas behind the
    /// swatch, at the dropdown's icon height so every row lines up with the
    /// marks other dropdowns lead with.
    #[test]
    fn a_palette_mark_paints_its_palette_at_icon_height() {
        for id in ThemeId::ALL {
            let theme = Theme::for_id(id);
            let mut mark = select_mark(SelectMark::palette(theme), &theme);
            let style = mark.style();
            assert_eq!(style.background, Some(theme.background.into()), "{id:?}");
            assert_eq!(
                style.size.height,
                Some(context_menu::ICON.px(&theme).into()),
                "{id:?}"
            );
        }
    }

    #[test]
    fn an_accent_mark_is_a_dot_of_that_accent() {
        let theme = Theme::dark();
        for color in theme::AccentColor::ALL {
            let accent = color.apply(theme).accent;
            let mut mark = select_mark(SelectMark::Accent(accent), &theme);
            assert_eq!(mark.style().background, Some(accent.into()), "{color:?}");
        }
    }
}
