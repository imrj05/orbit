//! Zed's design tokens, ported onto Orbit's [`Theme`].
//!
//! A one-to-one port of the sizing vocabulary in Zed's `ui` crate: the
//! `styles/{units,spacing,typography,elevation,animation}.rs` modules plus
//! the metrics its `Icon`, `Button`, `ListItem`, `List`, `Popover`,
//! `ContextMenu`, `Modal`, `Tooltip`, `InputField`, and `Scrollbar`
//! components are built from (zed-industries/zed `main`, read 2026-09-25).
//! Names and numbers follow Zed's source so any value can be checked against
//! it line for line. Colors stay Orbit's semantic palette: Zed's token set is
//! sizes, spacing, elevation, and motion only.
//!
//! # Scaling
//!
//! Zed authors sizes in rems, where one rem is the user's UI font size
//! (16px by default). Orbit authors its chrome against a 14px UI font
//! ([`DEFAULT_UI_FONT_SIZE`](super::DEFAULT_UI_FONT_SIZE)), so a token
//! resolves as `zed_px × ui_font_size / DEFAULT_UI_FONT_SIZE`, the same rule
//! as [`Theme::ui_px`]. At Orbit's default settings every token is exactly
//! Zed's default pixel value, and it grows and shrinks with the UI font size
//! the way a Zed rem does. `theme.ui_px(v)` is Zed's `rems_from_px(v)`;
//! [`Theme::rems`] is Zed's `rems(v)`. Values Zed pins in `px(…)` (hairlines,
//! the context menu's 200px minimum width, scrollbar metrics) stay fixed.
//!
//! # Density
//!
//! Zed has three UI densities. Orbit's Spacing Density setting is a
//! percentage, so the three are anchored at 80 % (Compact), 100 % (Default),
//! and 120 % (Comfortable): at those settings [`DynamicSpacing`] returns
//! Zed's values exactly, and between anchors it interpolates linearly.
//!
//! Resolve a token through the theme — `DynamicSpacing::Base08.px(&theme)`,
//! `TextSize::Small.px(&theme)`, `div().elevation_2(&theme)`.

// This is the whole Zed vocabulary; surfaces adopt it one at a time, so a
// token no surface reads yet is expected rather than dead.
#![allow(dead_code)]

use std::time::Duration;

use gpui::{hsla, point, px, relative, BoxShadow, DefiniteLength, Hsla, Pixels, Styled};

use super::Theme;

// ── Units (`ui/src/styles/units.rs`) ────────────────────────────────────

/// Zed's rem, in px, at the default UI font size.
pub const BASE_REM_PX: f32 = 16.;

impl Theme {
    /// Zed's `rems(value)`: `value` rems of [`BASE_REM_PX`], tracking the UI
    /// font size setting like every other token.
    pub fn rems(&self, value: f32) -> Pixels {
        self.ui_px(value * BASE_REM_PX)
    }
}

// ── Corner radius + borders (`gpui_macros/src/styles.rs`) ───────────────

/// Orbit's corner scale, in pixels before the corner-rounding preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Radius {
    /// 0px.
    None,
    /// 2px.
    XSmall,
    /// 4px.
    Small,
    /// 5px — compact buttons and list items.
    Medium,
    /// 6px — standard buttons and input fields.
    Large,
    /// 8px — panels and elevated surfaces.
    XLarge,
    /// 10px.
    XXLarge,
    /// 12px.
    XXXLarge,
    /// 9999px — pills and discs.
    Full,
}

impl Radius {
    /// One radius ramp for every palette: the Linear-aligned scale. Shapes are
    /// part of the design language, not of a palette, so a Classic palette and
    /// an Operational one draw the same button and the same card.
    ///
    /// The user's corner-rounding setting still shifts every corner except
    /// [`Radius::None`] and [`Radius::Full`], which stay square and circular.
    pub fn px(self, theme: &Theme) -> Pixels {
        let base = match self {
            Self::None => return px(0.),
            Self::Full => return px(9999.),
            Self::XSmall => px(2.),
            Self::Small => px(4.),
            Self::Medium => px(5.),
            Self::Large => px(6.),
            Self::XLarge => px(8.),
            Self::XXLarge => px(10.),
            Self::XXXLarge => px(12.),
        };
        base + px(theme
            .ui
            .corner_rounding
            .min(crate::theme::MAX_CORNER_ROUNDING) as f32)
    }
}

/// GPUI's `border_1()`: a 1px hairline that never scales.
pub const BORDER_WIDTH: Pixels = px(1.);

// ── Density + dynamic spacing (`ui/src/styles/spacing.rs`) ──────────────

/// Zed's UI density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiDensity {
    Compact,
    #[default]
    Default,
    Comfortable,
}

impl UiDensity {
    /// The Spacing Density percentage this density is anchored to.
    pub const fn percent(self) -> u32 {
        match self {
            Self::Compact => 80,
            Self::Default => 100,
            Self::Comfortable => 120,
        }
    }

    /// The density nearest a Spacing Density percentage.
    pub fn nearest(percent: u32) -> Self {
        if percent < 90 {
            Self::Compact
        } else if percent > 110 {
            Self::Comfortable
        } else {
            Self::Default
        }
    }

    /// The active density — Zed's `ui_density(cx)`. For UI that has to branch
    /// on density; never compute a spacing from it, use [`DynamicSpacing`].
    pub fn of(theme: &Theme) -> Self {
        Self::nearest(theme.ui.spacing_density)
    }
}

/// Zed's dynamic spacing scale. The number after `Base` is the pixel value
/// at the default UI font size and density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DynamicSpacing {
    Base00,
    Base01,
    Base02,
    Base03,
    Base04,
    Base06,
    Base08,
    Base12,
    Base16,
    Base20,
    Base24,
    Base32,
    Base40,
    Base48,
}

impl DynamicSpacing {
    pub const ALL: [Self; 14] = [
        Self::Base00,
        Self::Base01,
        Self::Base02,
        Self::Base03,
        Self::Base04,
        Self::Base06,
        Self::Base08,
        Self::Base12,
        Self::Base16,
        Self::Base20,
        Self::Base24,
        Self::Base32,
        Self::Base40,
        Self::Base48,
    ];

    /// `(compact, default, comfortable)` px @16px/rem — the table Zed feeds
    /// `derive_dynamic_spacing!`. Base24 and up use its `(n - 4, n, n + 4)`
    /// formula.
    pub const fn values(self) -> (f32, f32, f32) {
        match self {
            Self::Base00 => (0., 0., 0.),
            Self::Base01 => (1., 1., 2.),
            Self::Base02 => (1., 2., 4.),
            Self::Base03 => (2., 3., 4.),
            Self::Base04 => (2., 4., 6.),
            Self::Base06 => (3., 6., 8.),
            Self::Base08 => (4., 8., 10.),
            Self::Base12 => (10., 12., 14.),
            Self::Base16 => (14., 16., 18.),
            Self::Base20 => (18., 20., 22.),
            Self::Base24 => (20., 24., 28.),
            Self::Base32 => (28., 32., 36.),
            Self::Base40 => (36., 40., 44.),
            Self::Base48 => (44., 48., 52.),
        }
    }

    /// Unscaled px at one of Zed's densities.
    pub const fn at(self, density: UiDensity) -> f32 {
        let (compact, default, comfortable) = self.values();
        match density {
            UiDensity::Compact => compact,
            UiDensity::Default => default,
            UiDensity::Comfortable => comfortable,
        }
    }

    /// Unscaled px at a Spacing Density percentage: Zed's value at each
    /// anchor, linear between anchors, and held past the outer two.
    pub fn at_percent(self, percent: u32) -> f32 {
        let (compact, default, comfortable) = self.values();
        let t = ((percent as f32 - 100.) / 20.).clamp(-1., 1.);
        if t < 0. {
            default + (default - compact) * t
        } else {
            default + (comfortable - default) * t
        }
    }

    /// The resolved spacing: density first, then the UI font scale — Zed's
    /// `DynamicSpacing::px(cx)`.
    pub fn px(self, theme: &Theme) -> Pixels {
        theme.ui_px(self.at_percent(theme.ui.spacing_density))
    }
}

// ── Typography (`ui/src/styles/typography.rs`, `theme/src/buffer_line_height.rs`)

/// Semantic UI text sizes — Zed's `TextSize` (its `LabelSize` shares the
/// scale).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TextSize {
    /// 14px — regular UI text.
    #[default]
    Default,
    /// 16px.
    Large,
    /// 12px.
    Small,
    /// 10px.
    XSmall,
    /// The UI font size setting itself (Orbit's default is 14px; Zed's, 16px).
    Ui,
    /// The Editor font size setting.
    Editor,
}

impl TextSize {
    pub fn px(self, theme: &Theme) -> Pixels {
        match self {
            Self::Large => theme.ui_px(16.),
            Self::Default => theme.ui_px(14.),
            Self::Small => theme.ui_px(12.),
            Self::XSmall => theme.ui_px(11.),
            Self::Ui => px(theme.ui.ui_font_size),
            Self::Editor => px(theme.ui.editor_font_size),
        }
    }
}

/// Headline sizes on a Major Second scale — Zed's `HeadlineSize`.
/// Used for page headings and reserved by the modal metrics ([`modal::HEADLINE`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HeadlineSize {
    /// ~14px.
    XSmall,
    /// 16px.
    Small,
    /// ~18px.
    #[default]
    Medium,
    /// ~20px.
    Large,
    /// ~23px.
    XLarge,
}

impl HeadlineSize {
    pub const fn rems(self) -> f32 {
        match self {
            Self::XSmall => 0.88,
            Self::Small => 1.0,
            Self::Medium => 1.125,
            Self::Large => 1.27,
            Self::XLarge => 1.43,
        }
    }

    pub fn px(self, theme: &Theme) -> Pixels {
        theme.rems(self.rems())
    }

    /// Zed gives every headline `rems(1.6)` — an absolute 25.6px at the
    /// default size, not a 1.6× multiplier.
    pub fn line_height(self, theme: &Theme) -> Pixels {
        theme.rems(1.6)
    }
}

/// Line-height multipliers — Zed's `BufferLineHeight`. Context menus force
/// `Comfortable` whatever the surrounding text style says.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BufferLineHeight {
    /// 1.618× — the golden ratio, GPUI's own default.
    #[default]
    Comfortable,
    /// 1.3×.
    Standard,
    Custom(f32),
}

impl BufferLineHeight {
    pub const fn value(self) -> f32 {
        match self {
            Self::Comfortable => 1.618,
            Self::Standard => 1.3,
            Self::Custom(value) => value,
        }
    }

    /// The multiplier as a relative length, for `.line_height(…)`.
    pub fn relative(self) -> DefiniteLength {
        relative(self.value())
    }

    /// The laid-out line height at `font_size`. GPUI rounds line heights to
    /// whole pixels, so this is the height a line of text actually occupies.
    pub fn resolve(self, font_size: Pixels) -> Pixels {
        (font_size * self.value()).round()
    }
}

// ── Icons (`ui/src/components/icon.rs`) ─────────────────────────────────

/// Zed's `IconSize`. There is no `Large`: the scale jumps from Medium (16px)
/// to XLarge (48px).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum IconSize {
    /// 10px.
    Indicator,
    /// 12px.
    XSmall,
    /// 14px — context menu entries.
    Small,
    /// 16px.
    #[default]
    Medium,
    /// 48px.
    XLarge,
    /// A size in rems. Zed pads a custom size by the size itself (a TODO in
    /// its source); here a custom size has no padding.
    Custom(f32),
}

impl IconSize {
    pub fn px(self, theme: &Theme) -> Pixels {
        match self {
            Self::Indicator => theme.ui_px(10.),
            Self::XSmall => theme.ui_px(12.),
            Self::Small => theme.ui_px(14.),
            Self::Medium => theme.ui_px(16.),
            Self::XLarge => theme.ui_px(48.),
            Self::Custom(rems) => theme.rems(rems),
        }
    }

    /// Padding on each side of the icon inside its square hit target.
    pub fn padding(self, theme: &Theme) -> Pixels {
        match self {
            Self::Indicator | Self::Custom(_) => DynamicSpacing::Base00.px(theme),
            _ => DynamicSpacing::Base02.px(theme),
        }
    }

    /// The side of the square hit target (icon + padding) — an `IconButton`'s
    /// size when its shape is square.
    pub fn square(self, theme: &Theme) -> Pixels {
        self.px(theme) + self.padding(theme) * 2.
    }
}

// ── Buttons (`ui/src/components/button/button_like.rs`) ────────────────

/// Button heights — Zed's `ButtonSize`. Also sizes non-button rows that must
/// line up with buttons (`Default` matches a dense list row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// 32px.
    Large,
    /// 28px.
    Medium,
    /// 22px.
    #[default]
    Default,
    /// 18px.
    Compact,
    /// 16px.
    None,
}

impl ButtonSize {
    pub fn height(self, theme: &Theme) -> Pixels {
        match self {
            Self::Large => theme.ui_px(32.),
            Self::Medium => theme.ui_px(28.),
            Self::Default => theme.ui_px(22.),
            Self::Compact => theme.ui_px(18.),
            Self::None => theme.ui_px(16.),
        }
    }

    pub fn padding_x(self, theme: &Theme) -> Pixels {
        match self {
            Self::Large | Self::Medium => DynamicSpacing::Base08.px(theme),
            Self::Default | Self::Compact => DynamicSpacing::Base04.px(theme),
            // Zed's `px_px()`: one unscaled pixel.
            Self::None => px(1.),
        }
    }

    /// The [`IconSize`] an icon inside a button frame draws at — the one
    /// mapping every icon in a `button_frame` / `icon_button_frame` resolves
    /// through, so icon buttons, labeled buttons, and chips share one scale
    /// instead of each surface picking its own glyph size.
    ///
    /// The icon grows with the frame: `None` 16px → Indicator 10px, `Compact`
    /// 18px → XSmall 12px, `Default` 22px and `Medium` 28px → Small 14px, and
    /// `Large` 32px → Medium 16px. A chevron or other disclosure glyph inside
    /// a button is not its icon: those take `IconSize::XSmall` (see the fold
    /// rows), and dense tree rows keep `IconSize::Indicator`.
    pub fn icon_size(self) -> IconSize {
        match self {
            Self::None => IconSize::Indicator,
            Self::Compact => IconSize::XSmall,
            Self::Default | Self::Medium => IconSize::Small,
            Self::Large => IconSize::Medium,
        }
    }
}

/// Metrics every [`ButtonSize`] shares.
pub mod button {
    use super::*;

    /// Between a button's icon and its label.
    pub fn gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// Between a button's label and its keybinding.
    pub fn keybinding_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base06.px(theme)
    }

    /// Per corner for a full-size button. Compact frames take
    /// [`Radius::Medium`] instead — see [`radius`].
    pub const RADIUS: Radius = Radius::Large;

    /// Per corner by frame size: compact controls read tighter than the
    /// normal button scale, whose corners match every other control's.
    pub fn radius(size: ButtonSize) -> Radius {
        match size {
            ButtonSize::Compact | ButtonSize::None => Radius::Medium,
            _ => Radius::Large,
        }
    }

    /// A button label's size. Zed's `Button` defaults to a Default label and
    /// its compact buttons pass `LabelSize::Small`; Orbit encodes that
    /// convention by size so a label always fits its button.
    pub fn label_size(size: ButtonSize) -> TextSize {
        match size {
            ButtonSize::Large | ButtonSize::Medium => TextSize::Default,
            ButtonSize::Default | ButtonSize::Compact | ButtonSize::None => TextSize::Small,
        }
    }
}

// ── Lists (`ui/src/components/list/{list,list_item,list_separator}.rs`) ──

/// Vertical padding a list row adds around its content — Zed's
/// `ListItemSpacing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListItemSpacing {
    /// No extra padding.
    #[default]
    Dense,
    /// −1px each side (Zed's `py_neg_px`, unscaled).
    ExtraDense,
    /// 4px each side (`py_1`).
    Sparse,
}

impl ListItemSpacing {
    pub fn padding_y(self, theme: &Theme) -> Pixels {
        match self {
            Self::Dense => px(0.),
            Self::ExtraDense => px(-1.),
            Self::Sparse => theme.rems(0.25),
        }
    }

    /// A row's height: Zed's list item has no fixed height; it is one line
    /// of its label plus this spacing above and below.
    pub fn row_height(self, line_height: Pixels, theme: &Theme) -> Pixels {
        line_height + self.padding_y(theme) * 2.
    }
}

/// `ListItem` metrics.
pub mod list_item {
    use super::*;

    /// Inner horizontal padding.
    pub fn padding_x(theme: &Theme) -> Pixels {
        DynamicSpacing::Base06.px(theme)
    }

    /// Outer horizontal inset of an `.inset(true)` item (every menu entry).
    pub fn inset(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// Indent per nesting level; fixed px in Zed.
    pub const INDENT_STEP: Pixels = px(12.);

    /// Between the disclosure/start slot and the content (`gap_1`).
    pub fn slot_gap(theme: &Theme) -> Pixels {
        theme.rems(0.25)
    }

    /// Inside the content slot.
    pub fn content_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base06.px(theme)
    }

    /// `.rounded()` / `.outlined()` / inset items — the same corner as a
    /// sidebar destination.
    pub const RADIUS: Radius = Radius::Medium;
}

/// Cards, panels, and other meaningful rounded content groups. One radius for
/// every card-like surface keeps nested boards from stepping through unrelated
/// corner values; list rows and badges have their own roles below.
pub mod panel {
    use super::*;

    /// Standard panel/card corner: 8px before the rounding preference,
    /// shared by all palettes.
    pub const RADIUS: Radius = Radius::XLarge;
}

/// `List` and `ListSeparator` metrics.
pub mod list {
    use super::*;

    /// Above the first and below the last row.
    pub fn padding_y(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// Above and below a 1px separator.
    pub fn separator_margin_y(theme: &Theme) -> Pixels {
        DynamicSpacing::Base06.px(theme)
    }

    /// A `ListSubHeader`'s label row (`h_5`).
    pub fn sub_header_height(theme: &Theme) -> Pixels {
        theme.rems(1.25)
    }

    /// A `ListSubHeader`'s outer horizontal padding.
    pub fn sub_header_padding_x(theme: &Theme) -> Pixels {
        DynamicSpacing::Base02.px(theme)
    }

    /// An inset `ListSubHeader`'s inner horizontal padding (`px_2`).
    pub fn sub_header_inset_x(theme: &Theme) -> Pixels {
        theme.rems(0.5)
    }

    /// Below a `ListSubHeader`.
    pub fn sub_header_padding_bottom(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// A `ListSubHeader`'s label: Small and muted.
    pub const SUB_HEADER_TEXT: TextSize = TextSize::Small;
}

// ── Pickers (`picker/src/{picker,render}.rs`) ───────────────────────────────

/// `Picker` metrics: a search row over a list of inset, `Sparse` list items.
pub mod picker {
    use super::*;

    /// Zed pickers draw the modal elevation even as popovers.
    pub const ELEVATION: ElevationIndex = ElevationIndex::ModalSurface;

    /// `DEFAULT_MODAL_WIDTH`: 34rem.
    pub fn default_width(theme: &Theme) -> Pixels {
        theme.rems(34.)
    }

    /// `DEFAULT_MODAL_MAX_HEIGHT`: 24rem.
    pub fn default_max_height(theme: &Theme) -> Pixels {
        theme.rems(24.)
    }

    /// The search row (`h_9`), with a hairline between it and the list.
    pub fn search_height(theme: &Theme) -> Pixels {
        theme.rems(2.25)
    }

    /// The search row's horizontal padding (`px_2p5`).
    pub fn search_padding_x(theme: &Theme) -> Pixels {
        theme.rems(0.625)
    }

    /// Above the first and below the last row.
    pub fn list_padding_y(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// Row spacing: pickers use `Sparse` list items.
    pub const ROW_SPACING: ListItemSpacing = ListItemSpacing::Sparse;

    /// A row's label size.
    pub const TEXT: TextSize = TextSize::Default;

    /// A row's secondary line (description, path, metadata).
    pub const SECONDARY_TEXT: TextSize = TextSize::Small;

    /// A one-line row: one Comfortable line of Default text plus `Sparse`
    /// padding — 31px at the default size.
    pub fn entry_height(theme: &Theme) -> Pixels {
        ROW_SPACING.row_height(BufferLineHeight::Comfortable.resolve(TEXT.px(theme)), theme)
    }

    /// A two-line row: a Default label over a Small secondary line.
    pub fn two_line_entry_height(theme: &Theme) -> Pixels {
        let line = |size: TextSize| BufferLineHeight::Comfortable.resolve(size.px(theme));
        ROW_SPACING.row_height(line(TEXT) + line(SECONDARY_TEXT), theme)
    }
}

// ── Popovers + menus (`popover.rs`, `popover_menu.rs`, `context_menu.rs`) ─

/// `Popover` and `PopoverMenu` metrics.
pub mod popover {
    use super::*;

    /// `POPOVER_Y_PADDING`: vertical padding added around a popover's
    /// contents, split between top and bottom. Fixed px.
    pub const Y_PADDING: Pixels = px(8.);

    /// Padding on each of the top and bottom edges.
    pub fn padding_y() -> Pixels {
        Y_PADDING * 0.5
    }

    /// Horizontal padding of a popover's aside panel (`px_1`).
    pub fn aside_padding_x(theme: &Theme) -> Pixels {
        theme.rems(0.25)
    }

    /// Between a popover and its aside (`gap_1`).
    pub fn aside_gap(theme: &Theme) -> Pixels {
        theme.rems(0.25)
    }

    pub const ELEVATION: ElevationIndex = ElevationIndex::ElevatedSurface;

    /// `PopoverMenu`'s default distance from its trigger: 4px of padding plus
    /// 1px of border compensation.
    pub const MENU_OFFSET: Pixels = px(5.);

    /// Margin kept from the window edges when a menu snaps into view
    /// (`snap_to_window_with_margin`).
    pub const WINDOW_MARGIN: Pixels = px(8.);
}

/// `ContextMenu` metrics.
pub mod context_menu {
    use super::*;

    /// Minimum width when the menu has no fixed width. Fixed px in Zed.
    pub const MIN_WIDTH: Pixels = px(200.);

    /// Tallest the menu grows, as a fraction of the viewport height.
    pub const MAX_HEIGHT_FRACTION: f32 = 0.75;

    /// Entry label size (Zed's default `Label`).
    pub const TEXT: TextSize = TextSize::Default;

    /// Forced on the whole menu, whatever the text style it is deferred from.
    pub const LINE_HEIGHT: BufferLineHeight = BufferLineHeight::Comfortable;

    /// Default entry icon.
    pub const ICON: IconSize = IconSize::Small;

    /// A link entry's trailing arrow.
    pub const LINK_ICON: IconSize = IconSize::XSmall;

    /// Between an entry's icon and its label (`gap_1p5`).
    pub fn icon_gap(theme: &Theme) -> Pixels {
        theme.rems(0.375)
    }

    /// Before an entry's keybinding (`ml_4`).
    pub fn keybinding_gap(theme: &Theme) -> Pixels {
        theme.rems(1.)
    }

    /// One entry: a Comfortable line of Default text in a dense list item —
    /// 23px at the default size.
    pub fn entry_height(theme: &Theme) -> Pixels {
        ListItemSpacing::Dense.row_height(LINE_HEIGHT.resolve(TEXT.px(theme)), theme)
    }

    /// Width of the documentation aside: `max_w_96` in a wide window,
    /// `max_w_48` otherwise.
    pub fn aside_width(theme: &Theme, wide_window: bool) -> Pixels {
        if wide_window {
            theme.rems(24.)
        } else {
            theme.rems(12.)
        }
    }

    /// Window width from which the aside sits beside the menu, not above it.
    pub fn wide_window_min_width(theme: &Theme) -> Pixels {
        theme.ui_px(800.)
    }
}

// ── Toasts + notifications ─────────────────────────────────────────────

/// Metrics shared by transient notifications and actionable update notices.
pub mod toast {
    use super::*;

    /// One width keeps every toast in a stack aligned to the same edge.
    pub const WIDTH: Pixels = px(360.);
    pub const BADGE_RADIUS: Radius = Radius::Small;
    pub const ENTER: Duration = Motion::Interaction.nominal();
    pub const TITLE: TextSize = TextSize::Default;
    pub const BODY: TextSize = TextSize::Small;
    pub const ICON: IconSize = IconSize::Small;
    pub const CLOSE_BUTTON: ButtonSize = ButtonSize::Default;
    pub const ACTION_BUTTON: ButtonSize = ButtonSize::Medium;
    pub const ICON_RADIUS: Radius = Radius::Large;

    pub fn surface<E: Styled>(element: E, theme: &Theme) -> E {
        element.elevation_2(theme)
    }

    pub fn stack_inset(theme: &Theme) -> Pixels {
        DynamicSpacing::Base16.px(theme)
    }

    pub fn stack_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    pub fn padding_x(theme: &Theme) -> Pixels {
        DynamicSpacing::Base12.px(theme)
    }

    pub fn padding_y(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    pub fn content_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    pub fn icon_tile_size(theme: &Theme) -> Pixels {
        ACTION_BUTTON.height(theme)
    }

    pub fn title_body_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base02.px(theme)
    }

    pub fn header_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    pub fn package_row_padding_y(theme: &Theme) -> Pixels {
        DynamicSpacing::Base06.px(theme)
    }

    pub fn package_row_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    pub fn version_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    pub fn version_text_size(theme: &Theme) -> Pixels {
        theme.code_px(11.)
    }

    pub fn badge_height(theme: &Theme) -> Pixels {
        ButtonSize::Default.height(theme)
    }

    pub fn badge_min_width(theme: &Theme) -> Pixels {
        badge_height(theme)
    }

    pub fn badge_padding_x(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    pub fn separator_size() -> Pixels {
        BORDER_WIDTH
    }

    pub fn action_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base06.px(theme)
    }
}

// ── Modals (`ui/src/components/modal.rs`) ───────────────────────────────

/// `Modal`, `ModalHeader`, `Section`, and `ModalFooter` metrics.
pub mod modal {
    use super::*;

    pub fn header_padding_x(theme: &Theme) -> Pixels {
        DynamicSpacing::Base12.px(theme)
    }

    pub fn header_padding_top(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    pub fn header_padding_bottom(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// Between the header's back button, headline column, and dismiss button.
    pub fn header_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    /// The header's headline (drawn muted in Zed).
    pub const HEADLINE: HeadlineSize = HeadlineSize::Small;

    /// Between sections.
    pub fn body_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    /// A section's horizontal padding: Base12 when contained, otherwise
    /// Base06 + Base06 (the same 12px at Default density).
    pub fn section_padding_x(theme: &Theme, contained: bool) -> Pixels {
        if contained {
            DynamicSpacing::Base12.px(theme)
        } else {
            DynamicSpacing::Base06.px(theme) * 2.
        }
    }

    /// Below a section's content (`pb_2`).
    pub fn section_padding_bottom(theme: &Theme) -> Pixels {
        theme.rems(0.5)
    }

    /// Between a section's rows.
    pub fn section_gap(theme: &Theme) -> Pixels {
        DynamicSpacing::Base04.px(theme)
    }

    /// A `SectionHeader` row (`h_7`).
    pub fn section_header_height(theme: &Theme) -> Pixels {
        theme.rems(1.75)
    }

    pub fn section_header_padding_x(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    /// All four sides of the footer, which carries a 1px top border.
    pub fn footer_padding(theme: &Theme) -> Pixels {
        DynamicSpacing::Base08.px(theme)
    }

    /// Between the footer's start and end slots (`gap_1`).
    pub fn footer_gap(theme: &Theme) -> Pixels {
        theme.rems(0.25)
    }

    pub const ELEVATION: ElevationIndex = ElevationIndex::ModalSurface;
}

// ── Tooltips (`ui/src/components/tooltip.rs`) ───────────────────────────

/// `Tooltip` metrics.
pub mod tooltip {
    use super::*;

    /// Left offset that keeps the tooltip off the cursor (`pl_2`).
    pub fn offset_x(theme: &Theme) -> Pixels {
        theme.rems(0.5)
    }

    /// Top offset that keeps the tooltip off the cursor (`pt_2p5`).
    pub fn offset_y(theme: &Theme) -> Pixels {
        theme.rems(0.625)
    }

    /// `px_2`.
    pub fn padding_x(theme: &Theme) -> Pixels {
        theme.rems(0.5)
    }

    /// `py_1`.
    pub fn padding_y(theme: &Theme) -> Pixels {
        theme.rems(0.25)
    }

    /// The title's wrap width (`max_w_72`).
    pub fn max_width(theme: &Theme) -> Pixels {
        theme.rems(18.)
    }

    /// Between the title and its keybinding (`gap_4`).
    pub fn keybinding_gap(theme: &Theme) -> Pixels {
        theme.rems(1.)
    }

    pub const TEXT: TextSize = TextSize::Default;

    /// The optional muted line under the title.
    pub const META_TEXT: TextSize = TextSize::Small;

    pub const ELEVATION: ElevationIndex = ElevationIndex::ElevatedSurface;
}

// ── Text inputs (`ui_input/src/input_field.rs`) ─────────────────────────

/// `InputField` metrics.
pub mod input {
    use super::*;

    /// Fixed px in Zed.
    pub const MIN_WIDTH: Pixels = px(192.);

    /// `min_h_8`.
    pub fn min_height(theme: &Theme) -> Pixels {
        theme.rems(2.)
    }

    /// `px_2`.
    pub fn padding_x(theme: &Theme) -> Pixels {
        theme.rems(0.5)
    }

    /// `py_1p5`.
    pub fn padding_y(theme: &Theme) -> Pixels {
        theme.rems(0.375)
    }

    /// Between the label and the field, and between a start icon and the
    /// text (`gap_1`).
    pub fn gap(theme: &Theme) -> Pixels {
        theme.rems(0.25)
    }

    pub const RADIUS: Radius = Radius::Large;
    pub const LABEL: TextSize = TextSize::Small;
    pub const ICON: IconSize = IconSize::Small;

    /// The composer is a field, so it takes the field's corner. A large
    /// multi-line box is still an input, and one corner value across every
    /// field is what makes the workbench read as one system.
    pub fn composer_radius(theme: &Theme) -> Pixels {
        RADIUS.px(theme)
    }
}

// ── Elevation (`ui/src/styles/elevation.rs`, `ui/src/traits/styled_ext.rs`)

/// How close a surface sits to the user — Zed's `ElevationIndex`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElevationIndex {
    /// The app background, under panels and panes.
    Background,
    /// Panels, panes, title bar, and tab bar.
    Surface,
    /// The editable content area.
    EditorSurface,
    /// Popovers, menus, palettes, notifications, floating panels.
    ElevatedSurface,
    /// Modals and dialogs, above the scrim.
    ModalSurface,
}

impl ElevationIndex {
    /// The shadow stack for this layer. Only the two elevated layers cast one,
    /// and each is restrained: a floating surface is defined by its hairline
    /// first and its shadow second. No broad ambient layer, and nothing below
    /// the elevated surfaces casts anything at all.
    pub fn shadow(self, _theme: &Theme) -> Vec<BoxShadow> {
        match self {
            Self::ElevatedSurface => vec![drop_shadow(1., 2., 0.10)],
            Self::ModalSurface => vec![drop_shadow(2., 6., 0.16)],
            Self::Background | Self::Surface | Self::EditorSurface => Vec::new(),
        }
    }

    /// This layer's fill in Orbit's palette: the window chrome for the base
    /// layers, the content pane for the editor surface, and the floating
    /// fill for both elevated layers (Zed's `elevated_surface_background`).
    pub fn bg(self, theme: &Theme) -> Hsla {
        match self {
            Self::Background | Self::Surface => theme.surface,
            Self::EditorSurface => theme.background,
            Self::ElevatedSurface | Self::ModalSurface => theme.surface_elevated,
        }
    }
}

fn drop_shadow(offset_y: f32, blur: f32, alpha: f32) -> BoxShadow {
    BoxShadow {
        color: hsla(0., 0., 0., alpha),
        offset: point(px(0.), px(offset_y)),
        blur_radius: px(blur),
        spread_radius: px(0.),
    }
}

/// Zed's `StyledExt::elevation_*`: the elevated fill, the shared
/// [`panel::RADIUS`] corner, a 1px hairline, and the layer's shadow. The hairline is Orbit's
/// floating-surface stroke (`border_strong`), the role Zed's `border_variant`
/// plays.
pub trait StyledExt: Styled + Sized {
    /// Title bar, panels, tab bar.
    fn elevation_1(self, theme: &Theme) -> Self {
        elevated(self, theme, ElevationIndex::Surface, true)
    }

    fn elevation_1_borderless(self, theme: &Theme) -> Self {
        elevated(self, theme, ElevationIndex::Surface, false)
    }

    /// Popovers, menus, tooltips, palettes, notifications.
    fn elevation_2(self, theme: &Theme) -> Self {
        elevated(self, theme, ElevationIndex::ElevatedSurface, true)
    }

    fn elevation_2_borderless(self, theme: &Theme) -> Self {
        elevated(self, theme, ElevationIndex::ElevatedSurface, false)
    }

    /// Modals and dialogs: anything that must be answered or dismissed before
    /// the rest of the window responds.
    fn elevation_3(self, theme: &Theme) -> Self {
        elevated(self, theme, ElevationIndex::ModalSurface, true)
    }

    fn elevation_3_borderless(self, theme: &Theme) -> Self {
        elevated(self, theme, ElevationIndex::ModalSurface, false)
    }
}

impl<E: Styled> StyledExt for E {}

// ── Raised buttons ──────────────────────────────────────────────────────

/// A filled button's treatment. The fill and its 1px border carry the
/// affordance, so the face is flat: no gradient, no lift shadow. This is the
/// one place a filled control's treatment is decided, so a caller never paints
/// its own.
pub trait RaisedExt: Styled + Sized {
    /// The button's face in `color`. Kept as a method, rather than a bare
    /// `.bg()`, so the treatment can change once instead of at 209 call sites.
    fn raised(self, color: Hsla, _theme: &Theme) -> Self {
        self.bg(color)
    }

    /// A segment of a grouped control. Identical to [`Self::raised`] now that
    /// nothing lifts; retained so a segmented control reads as a deliberate
    /// choice at the call site.
    fn raised_flat(self, color: Hsla) -> Self {
        self.bg(color)
    }
}

impl<E: Styled> RaisedExt for E {}

// ── Interaction states (§2) ─────────────────────────────────────────────

/// The six states every interactive surface answers to.
///
/// This is the colour half of the interaction-state contract, and the one
/// place a state's fill, frame, and ink are decided. *How* a state is
/// painted — a flat wash versus a `raised` gradient, a ring versus a border —
/// stays with the component, because that belongs to the surface's role.
///
/// Today most surfaces name `theme.surface_hover` themselves (289 `.hover()`
/// closures across the crate). Phase 2 of the UI/UX Modernization epic turns
/// this table into shared primitives and migrates those call sites; until
/// then the vocabulary exists so the contract can be reviewed before 255
/// buttons are rewritten against it.
///
/// Two roles are deliberately outside this table: primary/action buttons take
/// their hover and pressed steps from the palette's `action` ramp
/// (`action_hover` / `action_pressed`), and raised buttons apply a gradient
/// *treatment* over whatever fill this returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionState {
    /// No pointer, no focus: the resting treatment.
    Rest,
    /// The pointer is over the control.
    Hover,
    /// The pointer or space bar is held down.
    Pressed,
    /// The control is the object currently being operated on.
    Selected,
    /// Keyboard focus is on the control.
    Focused,
    /// Present but not operable.
    Disabled,
    /// Not a state: the translucent treatment a control takes when it floats
    /// over the wallpaper. Ink at low alpha, so the surface behind it stays
    /// visible. Painted by `ControlExt::hover_on_glass`.
    Glass,
}

impl InteractionState {
    pub const ALL: [Self; 6] = [
        Self::Rest,
        Self::Hover,
        Self::Pressed,
        Self::Selected,
        Self::Focused,
        Self::Disabled,
    ];

    /// The wash painted behind a row, a ghost control, or a menu entry.
    ///
    /// `None` means paint nothing: the control keeps the surface beneath it.
    /// That covers `Rest` and `Focused` (focus is a frame, never a fill —
    /// `DESIGN.md` §Focus) and the two role exceptions above.
    pub fn fill(self, theme: &Theme) -> Option<Hsla> {
        match self {
            Self::Rest | Self::Focused => None,
            Self::Hover => Some(theme.surface_hover),
            Self::Pressed => Some(theme.surface_active),
            Self::Selected => Some(theme.surface_selected),
            Self::Disabled | Self::Glass => Some(theme.overlay),
        }
    }

    /// The frame around the control.
    ///
    /// Only `Focused` carries the focus ring. `Disabled` gets the quiet
    /// hairline so a disabled field still reads as a field, and every other
    /// state keeps whatever border its role defines.
    pub fn border(self, theme: &Theme) -> Option<Hsla> {
        match self {
            Self::Focused => Some(theme.focus_ring),
            Self::Disabled => Some(theme.border_subtle),
            _ => None,
        }
    }

    /// The ink for a label or icon in a row-like control: nav rows, list
    /// rows, table rows, menu entries.
    ///
    /// Rows rest at secondary ink and lift to primary on hover, so the
    /// hierarchy of a list survives its own pointer feedback.
    pub fn row_ink(self, theme: &Theme) -> Hsla {
        match self {
            Self::Rest => theme.text_secondary,
            Self::Hover | Self::Pressed | Self::Focused => theme.text_primary,
            Self::Selected => theme.selection_fg,
            Self::Disabled => theme.text_disabled,
            Self::Glass => theme.text_primary,
        }
    }

    /// The ink for a label or icon in a button, chip, or field — controls
    /// that already carry primary ink at rest.
    pub fn control_ink(self, theme: &Theme) -> Hsla {
        match self {
            Self::Selected => theme.selection_fg,
            Self::Disabled => theme.text_disabled,
            _ => theme.text_primary,
        }
    }

    /// The translucent hover wash for glass controls.
    pub fn glass_fill(self, theme: &Theme) -> Hsla {
        theme.overlay_strong
    }
}

/// Width of the focus ring. Orbit draws focus as a 1px border in
/// [`Theme::focus_ring`] rather than a GPUI outline, so the ring costs no
/// layout and reads the same inside a clipped row.
pub const FOCUS_RING_WIDTH: Pixels = BORDER_WIDTH;

fn elevated<E: Styled>(el: E, theme: &Theme, index: ElevationIndex, bordered: bool) -> E {
    let el = el
        .bg(theme.card_fill(ElevationIndex::ElevatedSurface.bg(theme)))
        .rounded(panel::RADIUS.px(theme))
        .shadow(index.shadow(theme));
    if bordered {
        el.border_1().border_color(theme.border_strong)
    } else {
        el
    }
}

// ── Motion (`ui/src/styles/animation.rs`) ───────────────────────────────

/// Zed's `AnimationDuration`.
///
/// Only one-shot transitions use this scale. Orbit's looping affordances (the
/// spinner, shimmer, streaming pulses) and its feedback timers (copy / refresh
/// acknowledgment, gesture guards) run their own cadences, which Zed's
/// three-value scale does not cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AnimationDuration {
    Instant = 50,
    /// The default for animate-in transitions.
    Fast = 150,
    Slow = 300,
}

impl AnimationDuration {
    pub const fn duration(self) -> Duration {
        Duration::from_millis(self as u64)
    }
}

impl From<AnimationDuration> for Duration {
    fn from(duration: AnimationDuration) -> Self {
        duration.duration()
    }
}

/// Orbit's named motion roles, layered on [`AnimationDuration`].
///
/// A surface picks a role, not a number. The roles are the bands `DESIGN.md`
/// §Motion System defines, and they are the only durations a one-shot
/// transition may use:
///
/// | Role | Duration | Band | Used for |
/// | -- | --: | -- | -- |
/// | `Instant` | 50 ms | 0–80 | Focus, press acknowledgement |
/// | `Interaction` | 150 ms | 120–160 | Hover, selection, menu appearance |
/// | `Layout` | 200 ms | 160–220 | Panel open/close, disclosure |
/// | `Travel` | 260 ms | 220–320 | A card moving as a whole: popover travel, dialog entrance, send morph |
///
/// `Instant` and `Interaction` are Zed's own values
/// ([`AnimationDuration::Instant`] / `Fast`). `Layout` and `Travel` are
/// Orbit's: Zed's `Slow` is 300 ms, which reads as hesitation on a workbench
/// panel, and a card crossing 200 px in 150 ms reads as a teleport rather than
/// travel. `Travel` moves a card, never a row — hover, press, focus and
/// selection stay inside `Interaction`.
///
/// **Reduce Motion is a property of the role, not of each call site.**
/// [`Motion::duration`] returns zero when the setting is on, so a transition
/// collapses to the new state instead of disappearing. Looping affordances
/// that report live work (the running-session shimmer, the spinner, the
/// streaming pulse) are not roles: they read the setting themselves and keep
/// their glyph while dropping the motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    /// 50 ms: the state change is already legible, motion only confirms it.
    Instant,
    /// 150 ms: the pointer or keyboard moved; the surface follows immediately.
    Interaction,
    /// 200 ms: a region of the layout changed size or presence.
    Layout,
    /// 260 ms: a surface moves as a whole. Never a row: a hover or a press that
    /// took this long would lag the pointer.
    Travel,
}

impl Motion {
    pub const ALL: [Self; 4] = [Self::Instant, Self::Interaction, Self::Layout, Self::Travel];

    /// The role's duration before Reduce Motion is considered.
    pub const fn nominal(self) -> Duration {
        match self {
            Self::Instant => AnimationDuration::Instant.duration(),
            Self::Interaction => AnimationDuration::Fast.duration(),
            Self::Layout => Duration::from_millis(200),
            Self::Travel => Duration::from_millis(260),
        }
    }

    /// The duration a surface should animate for: [`Self::nominal`], or zero
    /// when the user has asked for reduced motion.
    pub fn duration(self, theme: &Theme) -> Duration {
        if theme.reduce_motion() {
            Duration::ZERO
        } else {
            self.nominal()
        }
    }

    /// The role's easing. Orbit does not bounce: every role decelerates out of
    /// the transition, and `Instant` is linear because it is feedback rather
    /// than travel.
    pub fn easing(self) -> fn(f32) -> f32 {
        fn ease_out_quint(delta: f32) -> f32 {
            gpui::ease_out_quint()(delta)
        }
        match self {
            Self::Instant => gpui::linear,
            Self::Interaction | Self::Layout | Self::Travel => ease_out_quint,
        }
    }
}

/// Zed's default easing for one-shot transitions. Re-exported vocabulary,
/// like the rest of this module, before any surface animates with it.
#[allow(unused_imports)]
pub use gpui::ease_out_quint;

/// How far an animate-in travels.
pub const SLIDE_IN_DISTANCE: Pixels = px(40.);

/// Where a fading animate-in starts.
pub const FADE_IN_START_OPACITY: f32 = 0.4;

// ── Looping affordances (§3) ────────────────────────────────────────────

/// Cadences for the loops that report live state: a spinner, a shimmer, a
/// streaming pulse, an indeterminate progress bar.
///
/// A loop is not a transition. It runs for as long as the state it reports
/// lasts, so it has a cadence rather than a [`Motion`] role — and every one of
/// them is guarded by Reduce Motion at its call site, where the glyph stays and
/// the motion drops.
pub mod looping {
    use std::time::Duration;

    /// A spinner turning in place.
    pub const SPIN: Duration = Duration::from_millis(900);
    /// The refresh glyph: a shorter turn, because it reads as "again" rather
    /// than "working".
    pub const SPIN_FAST: Duration = Duration::from_millis(700);
    /// The running-session shimmer sweeping a title.
    pub const SHIMMER: Duration = Duration::from_millis(2000);
    /// A streaming or thinking pulse.
    pub const PULSE: Duration = Duration::from_millis(1600);
    /// An indeterminate progress segment crossing its track.
    pub const PROGRESS: Duration = Duration::from_millis(1100);
}

// ── Data visualisation (§3) ─────────────────────────────────────────────

/// Chart motion: a longer, gentler arc than any UI transition.
///
/// These durations are deliberately **outside** the [`Motion`] bands. A chart
/// reveal is the data arriving, not a control changing state, and it must never
/// be used for chrome. Every caller still guards on Reduce Motion — a chart that
/// cannot animate just paints its final frame.
pub mod chart {
    use std::time::Duration;

    /// First paint of a chart or sparkline.
    pub const DRAW: Duration = Duration::from_millis(900);
    /// Morph between two data sets.
    pub const MORPH: Duration = Duration::from_millis(500);
    /// A canvas reveal — the usage chart's one-shot fill.
    pub const REVEAL: Duration = Duration::from_millis(1100);

    /// The chart easing: a symmetric cubic that starts and ends quietly, so a
    /// reveal reads as drawing rather than sliding. Built by the motion layer's
    /// one curve builder, so a chart and a transition cannot drift apart.
    pub fn ease() -> impl Fn(f32) -> f32 {
        crate::motion::ease::bezier(0.85, 0.0, 0.15, 1.0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// A chart cadence is not a transition: it must sit outside the Motion
        /// bands, so nobody reuses it for chrome.
        #[test]
        fn chart_cadence_is_longer_than_any_transition_band() {
            for duration in [DRAW, MORPH, REVEAL] {
                for motion in crate::theme::tokens::Motion::ALL {
                    assert!(
                        duration > motion.nominal(),
                        "{duration:?} must stay outside the {motion:?} band"
                    );
                }
            }
        }

        #[test]
        fn the_chart_easing_starts_and_ends_on_the_endpoints() {
            let ease = ease();
            assert!(ease(0.).abs() < 1e-6);
            assert!((ease(1.) - 1.).abs() < 1e-6);
            let mut previous = 0.0;
            for step in 0..=100 {
                let value = ease(step as f32 / 100.);
                assert!(value >= previous - 1e-6, "the curve never travels back");
                previous = value;
            }
        }
    }
}

// ── Scrollbars (`ui/src/components/scrollbar.rs`) ───────────────────────

/// Zed's `ScrollbarStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollbarStyle {
    #[default]
    Regular,
    Editor,
}

impl ScrollbarStyle {
    pub const fn width(self) -> Pixels {
        match self {
            Self::Regular => px(6.),
            Self::Editor => px(15.),
        }
    }
}

/// Scrollbar metrics shared by both styles.
pub mod scrollbar {
    use super::*;

    /// Around the thumb.
    pub const PADDING: Pixels = px(4.);
    pub const TRACK_BORDER: Pixels = BORDER_WIDTH;
    pub const MIN_THUMB_LENGTH: Pixels = px(25.);
    /// Idle time before an auto-hiding scrollbar fades.
    pub const AUTOHIDE_DELAY: Duration = Duration::from_millis(1000);
    pub const HIDE_DURATION: Duration = Duration::from_millis(400);
    pub const SHOW_DURATION: Duration = Duration::from_millis(50);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::UiPrefs;

    #[test]
    fn corner_rounding_adjusts_shared_shapes_without_changing_pills_or_square_edges() {
        let mut theme = Theme::dark();
        let base = Radius::Large.px(&theme);
        theme.ui.corner_rounding = crate::theme::MAX_CORNER_ROUNDING;
        assert_eq!(Radius::Large.px(&theme), base + px(4.));
        assert_eq!(Radius::None.px(&theme), px(0.));
        assert_eq!(Radius::Full.px(&theme), px(9999.));
    }

    fn theme_with(ui_font_size: f32, spacing_density: u32) -> Theme {
        let ui = UiPrefs {
            ui_font_size,
            spacing_density,
            ..UiPrefs::default()
        };
        Theme::dark().with_ui(ui)
    }

    fn default_theme() -> Theme {
        Theme::dark()
    }

    fn close(a: Pixels, b: f32) -> bool {
        (f32::from(a) - b).abs() < 0.001
    }

    #[test]
    fn default_settings_resolve_to_zeds_default_pixels() {
        let theme = default_theme();
        let expected = [
            0., 1., 2., 3., 4., 6., 8., 12., 16., 20., 24., 32., 40., 48.,
        ];
        for (spacing, px) in DynamicSpacing::ALL.into_iter().zip(expected) {
            assert!(close(spacing.px(&theme), px), "{spacing:?}");
        }
        assert!(close(TextSize::Large.px(&theme), 16.));
        assert!(close(TextSize::Default.px(&theme), 14.));
        assert!(close(TextSize::Small.px(&theme), 12.));
        assert!(close(TextSize::XSmall.px(&theme), 11.));
        assert!(close(ButtonSize::Default.height(&theme), 22.));
        assert!(close(ButtonSize::Medium.height(&theme), 28.));
        assert!(close(Radius::Small.px(&theme), 4.));
        assert!(close(Radius::Large.px(&theme), 6.));
        assert!(close(tooltip::max_width(&theme), 288.));
        assert!(close(HeadlineSize::Small.px(&theme), 16.));
    }

    #[test]
    fn density_anchors_hit_zeds_three_tables() {
        for spacing in DynamicSpacing::ALL {
            for density in [
                UiDensity::Compact,
                UiDensity::Default,
                UiDensity::Comfortable,
            ] {
                assert_eq!(
                    spacing.at_percent(density.percent()),
                    spacing.at(density),
                    "{spacing:?} at {density:?}"
                );
            }
        }
        // Past the outer anchors the table is held, not extrapolated.
        assert_eq!(DynamicSpacing::Base08.at_percent(60), 4.);
        assert_eq!(DynamicSpacing::Base08.at_percent(140), 10.);
    }

    #[test]
    fn density_interpolates_between_anchors() {
        // Base12 is (10, 12, 14): halfway to Compact is 11, to Comfortable 13.
        assert_eq!(DynamicSpacing::Base12.at_percent(90), 11.);
        assert_eq!(DynamicSpacing::Base12.at_percent(110), 13.);
        let compact = theme_with(14., 80);
        assert!(close(DynamicSpacing::Base06.px(&compact), 3.));
    }

    #[test]
    fn tokens_scale_with_the_ui_font_like_rems() {
        // 16px UI font is 16/14 of Orbit's authored default.
        let theme = theme_with(16., 100);
        let scale = 16. / 14.;
        assert!(close(DynamicSpacing::Base08.px(&theme), 8. * scale));
        assert!(close(TextSize::Default.px(&theme), 14. * scale));
        assert!(close(IconSize::Small.px(&theme), 14. * scale));
        assert!(close(theme.rems(1.), 16. * scale));
        // Fixed-px tokens do not move.
        assert_eq!(context_menu::MIN_WIDTH, px(200.));
        assert_eq!(list_item::INDENT_STEP, px(12.));
        assert!(close(TextSize::Ui.px(&theme), 16.));
    }

    #[test]
    fn icon_squares_add_their_padding() {
        let theme = default_theme();
        assert!(close(IconSize::Indicator.square(&theme), 10.));
        assert!(close(IconSize::XSmall.square(&theme), 16.));
        assert!(close(IconSize::Small.square(&theme), 18.));
        assert!(close(IconSize::Medium.square(&theme), 20.));
        assert!(close(IconSize::XLarge.square(&theme), 52.));
    }

    #[test]
    fn button_padding_follows_size() {
        let theme = default_theme();
        assert!(close(ButtonSize::Large.padding_x(&theme), 8.));
        assert!(close(ButtonSize::Default.padding_x(&theme), 4.));
        assert_eq!(ButtonSize::None.padding_x(&theme), px(1.));
    }

    #[test]
    fn panel_and_composer_radii_are_semantic() {
        let theme = default_theme();
        assert!(close(panel::RADIUS.px(&theme), 8.));
        assert!(close(input::RADIUS.px(&theme), 6.));
        assert_eq!(
            input::composer_radius(&theme),
            input::RADIUS.px(&theme),
            "the composer shares the field radius"
        );
        assert_eq!(list_item::RADIUS, Radius::Medium);
    }

    /// Phase 9 baseline, not a gate: prints the cost of the two pure paths a
    /// launch pays for — resolving every shipped palette, and restoring the
    /// persisted layout. Run with
    /// `cargo test -p orbit-pi --bin orbit-pi baseline -- --ignored --nocapture`.
    ///
    /// Rendering paths (transcript scroll, panel resize, streaming frame
    /// budget) cannot be measured headlessly; they need the GPU harness and are
    /// listed as unmeasured in `docs/performance-baseline.md`.
    #[test]
    #[ignore = "baseline measurement"]
    fn baseline_palette_and_layout_costs() {
        use std::time::Instant;

        let rounds = 1_000;
        let start = Instant::now();
        for _ in 0..rounds {
            for id in crate::theme::ThemeId::ALL {
                std::hint::black_box(Theme::for_id(id));
            }
        }
        let per_palette = start.elapsed() / (rounds * crate::theme::ThemeId::ALL.len() as u32);
        println!("palette resolve: {per_palette:?} per palette");

        let payload = r#"{"sidebar_width":248.0,"sidepane_width":460.0,"explorer_width":300.0,"terminal_height":260.0}"#;
        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(crate::layout::from_json_for_baseline(payload));
        }
        let per_restore = start.elapsed() / rounds;
        println!("layout restore: {per_restore:?} per launch");

        let start = Instant::now();
        for _ in 0..rounds {
            std::hint::black_box(crate::commands::COMMANDS.len());
        }
        println!("registry scan: {:?} per 1000 launches", start.elapsed());
    }

    #[test]
    fn one_linear_geometry_ramp_serves_every_palette() {
        // Shapes belong to the design language, not to a palette. A Classic
        // palette and an Operational one must resolve every radius, caption
        // size and elevation the same way.
        for id in crate::theme::ThemeId::ALL {
            let theme = Theme::for_id(id);
            assert!(close(Radius::XSmall.px(&theme), 2.), "{id:?}");
            assert!(close(Radius::Small.px(&theme), 4.), "{id:?}");
            assert!(close(Radius::Medium.px(&theme), 5.), "{id:?}");
            assert!(close(Radius::Large.px(&theme), 6.), "{id:?}");
            assert!(close(Radius::XLarge.px(&theme), 8.), "{id:?}");
            assert!(close(Radius::XXLarge.px(&theme), 10.), "{id:?}");
            assert!(close(Radius::XXXLarge.px(&theme), 12.), "{id:?}");
            assert!(close(TextSize::XSmall.px(&theme), 11.), "{id:?}");
            assert_eq!(
                ElevationIndex::ElevatedSurface.shadow(&theme).len(),
                1,
                "{id:?}: one restrained popover layer"
            );
            assert_eq!(
                ElevationIndex::ModalSurface.shadow(&theme).len(),
                1,
                "{id:?}: one restrained modal layer"
            );
            for layer in [
                ElevationIndex::Background,
                ElevationIndex::Surface,
                ElevationIndex::EditorSurface,
            ] {
                assert!(layer.shadow(&theme).is_empty(), "{id:?}: {layer:?}");
            }
        }
    }

    #[test]
    fn button_icons_grow_with_the_frame() {
        let scale = [
            (ButtonSize::None, IconSize::Indicator),
            (ButtonSize::Compact, IconSize::XSmall),
            (ButtonSize::Default, IconSize::Small),
            (ButtonSize::Medium, IconSize::Small),
            (ButtonSize::Large, IconSize::Medium),
        ];
        for (button, icon) in scale {
            assert_eq!(button.icon_size(), icon, "{button:?}");
        }
    }

    #[test]
    fn menu_entries_are_one_comfortable_line_tall() {
        let theme = default_theme();
        // 14px × 1.618 = 22.65, rounded by GPUI to 23.
        assert!(close(context_menu::entry_height(&theme), 23.));
        let line = px(20.);
        assert_eq!(ListItemSpacing::Dense.row_height(line, &theme), px(20.));
        assert_eq!(
            ListItemSpacing::ExtraDense.row_height(line, &theme),
            px(18.)
        );
        assert!(close(ListItemSpacing::Sparse.row_height(line, &theme), 28.));
    }

    #[test]
    fn elevation_shadows_are_restrained() {
        let dark = Theme::dark();
        let light = Theme::light();
        for index in [
            ElevationIndex::Background,
            ElevationIndex::Surface,
            ElevationIndex::EditorSurface,
        ] {
            assert!(index.shadow(&dark).is_empty(), "{index:?} casts no shadow");
        }
        for theme in [dark, light] {
            let elevated = ElevationIndex::ElevatedSurface.shadow(&theme);
            assert_eq!(elevated.len(), 1, "one hairline-shaped popover layer");
            assert_eq!(elevated[0].offset, point(px(0.), px(1.)));
            assert_eq!(elevated[0].blur_radius, px(2.));

            let modal = ElevationIndex::ModalSurface.shadow(&theme);
            assert_eq!(modal.len(), 1, "one restrained modal layer");
            assert_eq!(modal[0].offset, point(px(0.), px(2.)));
            assert_eq!(modal[0].blur_radius, px(6.));

            for shadow in elevated.iter().chain(&modal) {
                assert_eq!(shadow.spread_radius, px(0.));
                assert_eq!(shadow.offset.x, px(0.));
                assert!(
                    shadow.color.a <= 0.2,
                    "a floating surface is defined by its hairline first"
                );
            }
        }
    }

    #[test]
    fn density_buckets_for_branching_ui() {
        assert_eq!(UiDensity::nearest(80), UiDensity::Compact);
        assert_eq!(UiDensity::nearest(85), UiDensity::Compact);
        assert_eq!(UiDensity::nearest(90), UiDensity::Default);
        assert_eq!(UiDensity::nearest(110), UiDensity::Default);
        assert_eq!(UiDensity::nearest(115), UiDensity::Comfortable);
    }

    #[test]
    fn durations_are_zeds() {
        assert_eq!(
            Duration::from(AnimationDuration::Instant),
            Duration::from_millis(50)
        );
        assert_eq!(
            AnimationDuration::Fast.duration(),
            Duration::from_millis(150)
        );
        assert_eq!(
            AnimationDuration::Slow.duration(),
            Duration::from_millis(300)
        );
    }

    #[test]
    fn motion_roles_fill_the_spec_bands() {
        let ms = |motion: Motion| motion.nominal().as_millis() as u64;
        // DESIGN.md §Motion System: instant 0–80, interaction 120–160,
        // layout 160–220, travel 220–320. The bands are a contract, not a
        // suggestion.
        assert!((0..=80).contains(&ms(Motion::Instant)));
        assert!((120..=160).contains(&ms(Motion::Interaction)));
        assert!((160..=220).contains(&ms(Motion::Layout)));
        assert!((220..=320).contains(&ms(Motion::Travel)));
        assert!(ms(Motion::Instant) < ms(Motion::Interaction));
        assert!(ms(Motion::Interaction) < ms(Motion::Layout));
        assert!(ms(Motion::Layout) < ms(Motion::Travel));
    }

    #[test]
    fn motion_collapses_to_nothing_under_reduce_motion() {
        let mut theme = Theme::dark();
        for motion in Motion::ALL {
            assert_eq!(motion.duration(&theme), motion.nominal());
        }
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        for motion in Motion::ALL {
            assert_eq!(motion.duration(&theme), Duration::ZERO, "{motion:?}");
        }
    }

    /// `System` follows the OS on its own: a user whose system asks for reduced
    /// motion gets instant transitions without touching Orbit's setting.
    #[test]
    fn the_os_reduce_motion_preference_is_honoured() {
        let mut theme = Theme::dark();
        theme.ui.motion = crate::theme::MotionPreference::System;
        crate::theme::set_os_reduce_motion_for_test(true);
        assert!(theme.reduce_motion(), "the OS asks for it");
        assert_eq!(Motion::Interaction.duration(&theme), Duration::ZERO);

        crate::theme::set_os_reduce_motion_for_test(false);
        assert!(!theme.reduce_motion());
        assert_eq!(
            Motion::Interaction.duration(&theme),
            Motion::Interaction.nominal()
        );
    }

    #[test]
    fn motion_roles_ease_from_zero_to_one_without_overshoot() {
        for motion in Motion::ALL {
            let easing = motion.easing();
            assert!(close_px(easing(0.), 0.), "{motion:?} starts at 0");
            assert!(close_px(easing(1.), 1.), "{motion:?} ends at 1");
            // A decelerating curve never travels backwards, and Orbit does not
            // bounce — a value outside the interval would be either.
            for step in 1..100 {
                let t = step as f32 / 100.;
                let eased = easing(t);
                assert!(
                    (0. ..=1.).contains(&eased),
                    "{motion:?} left the unit interval at {t}: {eased}"
                );
            }
        }
    }

    fn close_px(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.0001
    }

    #[test]
    fn interaction_states_never_collide() {
        // Hover, pressed and selected are three different answers. A palette
        // that collapsed any two would make a hovered row and a pressed row
        // indistinguishable, which is exactly the bug the state table exists
        // to prevent — so the check runs over every shipped palette, not just
        // the default one.
        for id in crate::theme::ThemeId::ALL {
            let theme = Theme::for_id(id);
            let hover = InteractionState::Hover.fill(&theme).expect("hover fills");
            let pressed = InteractionState::Pressed
                .fill(&theme)
                .expect("pressed fills");
            let selected = InteractionState::Selected
                .fill(&theme)
                .expect("selected fills");
            assert_ne!(hover, pressed, "{id:?}: hover and pressed are one colour");
            assert_ne!(selected, hover, "{id:?}: selected and hover are one colour");
            assert_ne!(
                selected, pressed,
                "{id:?}: selected and pressed are one colour"
            );
        }
    }

    #[test]
    fn only_focus_paints_the_ring_and_only_disabled_paints_the_hairline() {
        let theme = Theme::dark();
        assert_eq!(
            InteractionState::Focused.border(&theme),
            Some(theme.focus_ring)
        );
        assert_eq!(
            InteractionState::Disabled.border(&theme),
            Some(theme.border_subtle)
        );
        for state in [
            InteractionState::Rest,
            InteractionState::Hover,
            InteractionState::Pressed,
            InteractionState::Selected,
        ] {
            assert_eq!(state.border(&theme), None, "{state:?} draws no frame");
        }
        for state in [InteractionState::Rest, InteractionState::Focused] {
            assert_eq!(state.fill(&theme), None, "{state:?} paints no fill");
        }
        assert_eq!(FOCUS_RING_WIDTH, BORDER_WIDTH);
    }

    #[test]
    fn disabled_and_selected_ink_are_their_own_roles() {
        for theme in [Theme::dark(), Theme::light()] {
            assert_eq!(
                InteractionState::Disabled.row_ink(&theme),
                theme.text_disabled
            );
            assert_eq!(
                InteractionState::Disabled.control_ink(&theme),
                theme.text_disabled
            );
            assert_eq!(
                InteractionState::Selected.row_ink(&theme),
                theme.selection_fg
            );
            assert_eq!(
                InteractionState::Selected.control_ink(&theme),
                theme.selection_fg
            );
            // A control rests at primary ink; a row rests at secondary, so a
            // row's hover lift is visible against its own rest state.
            assert_eq!(
                InteractionState::Rest.control_ink(&theme),
                theme.text_primary
            );
            assert_eq!(InteractionState::Rest.row_ink(&theme), theme.text_secondary);
            assert_ne!(
                InteractionState::Hover.row_ink(&theme),
                InteractionState::Rest.row_ink(&theme)
            );
        }
    }
}
