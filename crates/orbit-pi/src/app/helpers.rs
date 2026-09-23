use super::*;
use crate::platform::WindowCommand;

/// Turn a wire command name into a short human label, e.g. `set_model` →
/// `Set model failed`.
pub(crate) fn humanize_command(command: &str) -> String {
    let spaced = command.replace(['_', '.'], " ");
    let mut chars = spaced.chars();
    let label = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => tr!("helpers.command"),
    };
    tr!("helpers.command_failed", label = label)
}

/// One queued-message chip: a kind tag plus the message text.
pub(crate) fn queue_chip(kind: &str, text: &str, follow: bool, theme: Theme) -> AnyElement {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_start()
        .gap(px(6.))
        .px(px(8.))
        .py(px(5.))
        .rounded(px(6.))
        .border_1()
        .border_color(theme.border)
        .bg(theme.bg_raised)
        .child(
            div()
                .flex_none()
                .pt(px(1.))
                .text_size(theme.ui_px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(if follow { theme.text_2 } else { theme.accent })
                .child(kind.to_string()),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .line_clamp(4)
                .text_size(theme.ui_px(11.5))
                .line_height(theme.ui_px(16.))
                .text_color(theme.text)
                .child(text.to_string()),
        )
        .into_any_element()
}

/// Render an embedded HugeIcons SVG tinted with the given color.
///
/// `flex_none` is load-bearing: an SVG defaults to `flex-shrink: 1`, so in a
/// Hover group every action button opts into with `.group(BUTTON_GROUP)`.
/// Any [`icon`] descendant then lifts its ink while that button is hovered —
/// the one hook that gives the whole app's buttons a consistent hover cue
/// without threading a theme or a unique group name through each one.
pub(crate) const BUTTON_GROUP: &str = "orbit-button";

/// How far an icon lightens toward white when its button is hovered. Small
/// enough that semantic icons (accent, danger) stay on-brand; enough that a
/// muted `text_2`/`text_3` glyph visibly brightens.
const ICON_HOVER_LIGHTEN: f32 = 0.12;

/// The hover ink for an icon: the same hue, lightened toward white.
fn icon_hover_ink(color: Hsla) -> Hsla {
    Hsla {
        l: (color.l + ICON_HOVER_LIGHTEN).min(1.0),
        ..color
    }
}

/// flex row beside wide text it collapses to zero width (the "icons render
/// tiny" bug). Icons must always keep their declared size.
///
/// Every icon also lifts its ink toward white while an ancestor wearing
/// [`BUTTON_GROUP`] is hovered. GPUI resolves a hover group through a balanced
/// push/pop stack, so one shared group name is safe across siblings and nested
/// buttons — the nearest opt-in button wins. An icon outside any such button
/// sees no group and stays inert, so decorative glyphs never react.
pub(crate) fn icon(path: &'static str, size: f32, color: Hsla) -> gpui::Svg {
    let hover = icon_hover_ink(color);
    gpui::svg()
        .path(path)
        .flex_none()
        .size(px(size))
        .text_color(color)
        .group_hover(BUTTON_GROUP, move |style| style.text_color(hover))
}

/// Same as [`icon`] but for runtime-computed paths (per-provider marks).
pub(crate) fn icon_dyn(path: SharedString, size: f32, color: Hsla) -> gpui::Svg {
    let hover = icon_hover_ink(color);
    gpui::svg()
        .path(path)
        .flex_none()
        .size(px(size))
        .text_color(color)
        .group_hover(BUTTON_GROUP, move |style| style.text_color(hover))
}

/// The app's one loading spinner: `loader.svg` rotating once per 900 ms, and
/// rendered static under reduce-motion so the perpetual loop can be turned
/// off. `id` must be unique per instance — GPUI tracks the animation by it.
///
/// Every in-flight affordance (buttons, panels, session rows, tool cards)
/// shares this so "working" reads identically everywhere.
pub(crate) fn spinner(
    id: impl Into<ElementId>,
    size: f32,
    color: Hsla,
    theme: Theme,
) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/loader.svg")
        .flex_none()
        .size(px(size))
        .text_color(color);
    if theme.ui.reduce_motion {
        return svg.into_any_element();
    }
    svg.with_animation(
        id,
        Animation::new(Duration::from_millis(900)).repeat(),
        |svg, delta| {
            svg.with_transformation(Transformation::rotate(radians(
                delta * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

/// The app's one refresh affordance: the refresh glyph itself turning in
/// place while `active`, rather than swapping to a different loader, so the
/// control keeps its shape through the click. Active, it wears the accent —
/// the same read as the provider-usage popover's control; idle, it wears
/// `idle_color` and lifts its ink while a [`BUTTON_GROUP`] ancestor is
/// hovered. Reduce-motion keeps the glyph and the accent but drops the spin.
///
/// `id` must be unique per instance — GPUI tracks the animation by it.
pub(crate) fn refresh_glyph(
    id: impl Into<ElementId>,
    size: f32,
    active: bool,
    idle_color: Hsla,
    theme: Theme,
) -> AnyElement {
    if !active {
        return icon("icons/refresh.svg", size, idle_color).into_any_element();
    }
    let svg = gpui::svg()
        .path("icons/refresh.svg")
        .flex_none()
        .size(px(size))
        .text_color(theme.accent);
    if theme.ui.reduce_motion {
        return svg.into_any_element();
    }
    svg.with_animation(
        id,
        Animation::new(Duration::from_millis(700)).repeat(),
        |svg, delta| {
            svg.with_transformation(Transformation::rotate(radians(
                delta * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

/// The chrome every floating surface shares — anchored menus, dropdowns,
/// popovers, tooltips, and modals: the raised menu fill, a strong 1px
/// hairline, and the layered popover shadow. Callers keep their own radius,
/// size, padding, and occlusion, so a menu, a tooltip, and a modal can't
/// drift apart.
pub(crate) trait PopoverSurface: Styled {
    fn popover_surface(self, theme: Theme) -> Self {
        self.border_1()
            .border_color(theme.border_strong)
            .bg(theme.menu_bg)
            .shadow(theme.popover_shadow())
    }
}

impl<T: Styled> PopoverSurface for T {}

/// How an empty/error state fills its parent: `Full` when the parent is a plain
/// sized box (a panel body), `Grow` when the parent is a flex column.
#[derive(Clone, Copy)]
pub(crate) enum EmptyFill {
    Full,
    Grow,
}

/// A centered empty/error state: a medium title over an optional tertiary
/// detail line. Shared by the Explorer panel, the file viewer, and the Review
/// pane so their empty/error copy sits identically; the Git page's iconed
/// variant is `git_panel::widgets::empty_note`.
pub(crate) fn empty_state(
    theme: Theme,
    title: &str,
    detail: Option<&str>,
    fill: EmptyFill,
) -> AnyElement {
    let column = match fill {
        EmptyFill::Full => div().size_full(),
        EmptyFill::Grow => div().flex_1().min_h_0().min_w_0(),
    };
    let mut column = column
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .px(px(16.))
        .pb(px(24.))
        .child(
            div()
                .text_size(theme.ui_px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(title.to_string()),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(px(6.))
                .max_w(px(320.))
                .text_align(TextAlign::Center)
                .text_size(theme.ui_px(12.))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_3)
                .child(detail.to_string()),
        );
    }
    column.into_any_element()
}

// ── top-bar chip primitives ──

/// Shared height of every top-bar control — the quota pill, the "open in"
/// split, the diff chip and the icon buttons — so a row of mixed
/// affordances reads as one instrument panel instead of assorted sizes.
pub(crate) const HEADER_CTRL_H: f32 = 28.;

/// Corner radius of the rectangular header chips (the quota pill keeps a
/// full round so it still reads as a meter, not a button).
pub(crate) const HEADER_CTRL_R: f32 = 8.;

/// The glass fill every resting top-bar chip shares: the text-wash overlay
/// with its alpha raised toward the top edge and eased off at the bottom,
/// so chips catch light like a meter surface rather than a flat sticker.
pub(crate) fn header_fill(theme: &Theme) -> gpui::Background {
    let mut top = theme.overlay;
    top.a *= 1.5;
    let mut bottom = theme.overlay;
    bottom.a *= 0.7;
    linear_gradient(
        180.,
        linear_color_stop(top, 0.),
        linear_color_stop(bottom, 1.),
    )
}

/// The lift a chip gets under the cursor (or while its surface is open):
/// denser glass, a stronger hairline, and a tight contact shadow that pulls
/// the chip off the titlebar. Shared by hover *and* open states so a
/// toggle's panel reads as "still held" after the click.
fn header_lifted(theme: &Theme) -> gpui::StyleRefinement {
    let mut top = theme.overlay_strong;
    top.a *= 1.15;
    let mut bottom = theme.overlay_strong;
    bottom.a *= 0.85;
    gpui::StyleRefinement::default()
        .bg(linear_gradient(
            180.,
            linear_color_stop(top, 0.),
            linear_color_stop(bottom, 1.),
        ))
        .border_color(theme.border_strong)
        .shadow(vec![gpui::BoxShadow {
            color: theme.shadow_contact,
            offset: point(px(0.), px(1.)),
            blur_radius: px(3.),
            spread_radius: px(-1.),
        }])
}

/// Force a chip into its lifted state — for controls whose surface is
/// currently open (the info popover, the open-in menu, a visible panel).
pub(crate) fn header_lift<S: Styled>(el: S, theme: &Theme) -> S {
    let mut el = el;
    el.style().refine(&header_lifted(theme));
    el
}

/// Opacity of an action button while the pointer is down.
///
/// GPUI 0.2.2 has no element transform (`Transformation` is SVG-only) and no
/// CSS-style transitions, so the usual `scale(0.96)` press cannot be expressed
/// on a `div`. This dim is its tactile stand-in — one value keeps filled,
/// ghost, and danger buttons feeling identical under the finger.
pub(crate) const PRESS_DIM: f32 = 0.85;

/// Give an action button its shared press (`:active`) feedback. Call it around
/// the finished, stateful element:
///
/// ```ignore
/// press(div().id("save").cursor_pointer().hover(...)).child(label)
/// ```
pub(crate) fn press<T: gpui::StatefulInteractiveElement>(element: T) -> T {
    element.active(|style| style.opacity(PRESS_DIM))
}

/// Dress a top-bar chip: the [`header_fill`] glass in a hairline box, lifting
/// on hover. The caller owns the size and the rounding — the quota pill stays
/// `rounded_full` while the rest of the row takes [`HEADER_CTRL_R`], and a
/// radius set here would override whichever it picked — and must not set a
/// second hover of its own.
pub(crate) fn header_chip<S: Styled + InteractiveElement>(el: S, theme: &Theme) -> S {
    let lifted = header_lifted(theme);
    el.border_1()
        .border_color(theme.border)
        .bg(header_fill(theme))
        .hover(move |_| lifted)
}

/// A top-bar icon button: a square glass chip ([`HEADER_CTRL_H`]) holding a
/// centered icon, lifting on hover and staying lifted while `active` (its
/// surface is open). Callers pass the icon already tinted; the chip itself
/// carries no label.
pub(crate) fn header_icon_button(
    id: &'static str,
    theme: &Theme,
    active: bool,
    child: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    let chip = press(header_chip(
        div()
            .id(id)
            .group(BUTTON_GROUP)
            .size(px(HEADER_CTRL_H))
            .rounded(px(HEADER_CTRL_R))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer(),
        theme,
    ))
    .child(child);
    if active {
        header_lift(chip, theme)
    } else {
        chip
    }
}

/// A bare top-bar icon button: the same square hit box as
/// [`header_icon_button`], but without the chip's hairline or glass fill so
/// it sits flat on the titlebar. Hover is the only affordance.
pub(crate) fn header_ghost_button(
    id: &'static str,
    theme: &Theme,
    child: impl IntoElement,
) -> gpui::Stateful<gpui::Div> {
    press(
        div()
            .id(id)
            .group(BUTTON_GROUP)
            .size(px(HEADER_CTRL_H))
            .rounded(px(HEADER_CTRL_R))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.bg(theme.bg_hover)),
    )
    .child(child)
}

/// Width of one caption button, matching the metric Windows uses for its own
/// (`platform::WINDOW_CONTROLS_W` is three of them).
pub(crate) const CAPTION_BUTTON_W: f32 = 46.;

/// The window's own minimize / maximize / close buttons, drawn by the app on
/// the platforms where it paints the caption (`platform::draws_window_controls`).
///
/// Each button runs the system's own command through
/// [`platform::window_command`] — the same one a real caption button sends — so
/// nothing about window management is reimplemented here.
pub(crate) fn window_controls(theme: Theme, maximized: bool) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .h_full()
        .child(caption_button(
            theme,
            "window-minimize",
            "icons/minus.svg",
            WindowCommand::Minimize,
            false,
        ))
        .child(caption_button(
            theme,
            "window-maximize",
            // The glyph follows the window state, as the OS button does.
            if maximized {
                "icons/window-restore.svg"
            } else {
                "icons/window-maximize.svg"
            },
            WindowCommand::ToggleMaximize,
            false,
        ))
        .child(caption_button(
            theme,
            "window-close",
            "icons/x.svg",
            WindowCommand::Close,
            true,
        ))
}

/// One caption button: a flat 46px hit area with a centred glyph. Close is the
/// one that turns red under the pointer, the way Windows' own does, and its
/// glyph brightens with it.
fn caption_button(
    theme: Theme,
    id: &'static str,
    glyph: &'static str,
    command: WindowCommand,
    close: bool,
) -> impl IntoElement {
    let fill = if close {
        theme.stop_red
    } else {
        theme.bg_hover
    };
    let mut glyph_el = icon(glyph, 14., theme.text_2);
    if close {
        // The close glyph rides the caption's red hover; the shared button ink
        // lift already brightens it.
        glyph_el = glyph_el.text_color(theme.text_2);
    }
    press(
        div()
            .id(id)
            .group(BUTTON_GROUP)
            .w(px(CAPTION_BUTTON_W))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(move |style| style.bg(fill)),
    )
    .on_mouse_up(MouseButton::Left, move |_, window, _| {
        crate::platform::window_command(window, command)
    })
    .child(glyph_el)
}

/// Turn a header strip into a window drag region.
///
/// The press is handed to the OS rather than to GPUI's `WindowControlArea::Drag`
/// on the platform where the app paints the caption — see
/// [`platform::start_window_drag`] for why that path is closed to this app.
/// Elsewhere the platform's own drag area is what moves the window.
pub(crate) fn window_drag_region(el: gpui::Div) -> gpui::Div {
    #[cfg(windows)]
    {
        el.on_mouse_down(MouseButton::Left, |_, window, _| {
            crate::platform::start_window_drag(window)
        })
    }
    #[cfg(not(windows))]
    {
        el.window_control_area(gpui::WindowControlArea::Drag)
    }
}

/// Render a compile-time-embedded raster image (PNG) at a fixed height, with
/// the width derived from the source's aspect ratio.
///
/// `img("name.png")` is a trap for embedded assets: gpui's `From<&str>` runs
/// the string through its URI heuristic, and a slashless filename parses as a
/// valid URI authority, so it is fetched over the network instead of loaded
/// from [`crate::assets::Assets`]. Passing [`Resource::Embedded`] explicitly
/// keeps it on the asset-source path.
pub(crate) fn embedded_image(path: &'static str, height: f32) -> impl IntoElement + use<> {
    img(ImageSource::Resource(Resource::Embedded(path.into())))
        .h(px(height))
        .flex_none()
}

/// Same as [`embedded_image`], but sized by width; the height follows the
/// source's aspect ratio. Use for the wordmark, which is laid out to span the
/// sidebar's content width.
pub(crate) fn embedded_image_w(path: &'static str, width: Pixels) -> impl IntoElement + use<> {
    img(ImageSource::Resource(Resource::Embedded(path.into())))
        .w(width)
        .flex_none()
}

// ── devicons (Nerd Font file glyphs) ───────────────────────────────

/// The Nerd Font family that covers devicons glyphs, detected once per
/// process (font availability doesn't change mid-run).
static NERD_FONT: std::sync::OnceLock<Option<SharedString>> = std::sync::OnceLock::new();

/// Preferred Nerd Font families, best first — used only to *rank* the
/// installed families; any family whose name contains "Nerd Font" (or the
/// short `NF` style, e.g. `MesloLGS NF`) is a candidate, verified by an
/// actual PUA-glyph probe.
const NERD_FONT_PREFERRED: [&str; 6] = [
    "SymbolsNerdFont",
    "Symbols Nerd Font",
    "JetBrainsMono Nerd Font",
    "Hack Nerd Font",
    "FiraCode Nerd Font",
    "CaskaydiaCove Nerd Font",
];

/// The Nerd Font family to paint devicons glyphs with, or `None` when no
/// Nerd Font is installed (callers fall back to extension text badges).
pub(crate) fn nerd_font_family(cx: &App) -> Option<SharedString> {
    NERD_FONT
        .get_or_init(|| {
            let installed = cx.text_system().all_font_names();
            // Any family named like a Nerd Font, best-known first.
            let mut candidates: Vec<String> = installed
                .iter()
                .filter(|name| {
                    let lower = name.to_lowercase();
                    lower.contains("nerd font")
                        || lower.contains("nerdfont")
                        || lower.ends_with(" nf")
                })
                .cloned()
                .collect();
            // Dedicated symbols fonts paint the widest glyph coverage first;
            // popular patched coding fonts next; the rest in name order.
            candidates.sort_by_key(|name| {
                let lower = name.to_lowercase();
                let pref = NERD_FONT_PREFERRED
                    .iter()
                    .position(|p| lower.eq_ignore_ascii_case(&p.to_lowercase()))
                    .unwrap_or(NERD_FONT_PREFERRED.len());
                (pref, !lower.starts_with("symbols"), name.clone())
            });
            candidates.into_iter().find_map(|family| {
                let font_id = cx.text_system().resolve_font(&gpui::font(family.clone()));
                // `\u{ea60}` sits in Nerd Font's Octicon range — present in any
                // complete Nerd Font, absent from plain coding fonts.
                cx.text_system()
                    .typographic_bounds(font_id, px(12.), '\u{ea60}')
                    .ok()?;
                Some(SharedString::from(family))
            })
        })
        .clone()
}

/// devicons glyph + color for a file path (`README.md` → its Nerd Font
/// markdown glyph). `dark` selects devicons' palette; `None` when the
/// color string is not parseable hex.
pub(crate) fn dev_file_icon(path: &str, dark: bool) -> Option<(char, Hsla)> {
    let theme = if dark {
        devicons::Theme::Dark
    } else {
        devicons::Theme::Light
    };
    let icon = devicons::icon_for_file(path, &Some(theme));
    let value = u32::from_str_radix(icon.color.trim_start_matches('#'), 16).ok()?;
    Some((icon.icon, gpui::rgb(value).into()))
}

/// Paint a devicons glyph (or fall back to `fallback` when no Nerd Font
/// is installed) — shared by the @-mention rows and attachment chips.
pub(crate) fn file_glyph(
    path: &str,
    dark: bool,
    nerd_family: Option<&SharedString>,
    font_size: f32,
    fallback: AnyElement,
) -> AnyElement {
    let Some(family) = nerd_family else {
        return fallback;
    };
    let Some((glyph, color)) = dev_file_icon(path, dark) else {
        return fallback;
    };
    div()
        .w(px(18.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .font_family(family)
        .text_size(px(font_size))
        .text_color(color)
        .child(glyph.to_string())
        .into_any_element()
}

/// Extension text badge (`MD`, `TSX`, `HTML`, …) — the no-Nerd-Font
/// fallback for [`file_glyph`], tinted with the language's brand color.
pub(crate) fn file_badge(path: &str, theme: Theme) -> AnyElement {
    let (label, dark_hex, light_hex) = mentions::file_type_badge(path);
    let color: Hsla = gpui::rgb(if theme.mode == ThemeMode::Dark {
        dark_hex
    } else {
        light_hex
    })
    .into();
    div()
        .size(px(17.))
        .flex_none()
        .rounded(px(4.))
        .bg(color.opacity(0.16))
        .flex()
        .items_center()
        .justify_center()
        .text_size(theme.ui_px(7.5))
        .line_height(theme.ui_px(8.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label.to_string())
        .into_any_element()
}

/// A Runtime detail value in the UI text color.
pub(crate) fn runtime_text(theme: Theme, text: String) -> AnyElement {
    div()
        .min_w_0()
        .truncate()
        .text_size(theme.ui_px(12.5))
        .text_color(theme.text)
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
        .text_color(theme.text_2)
        .child(text)
        .into_any_element()
}

/// A Runtime error value (critical color).
pub(crate) fn runtime_error(theme: Theme, text: String) -> AnyElement {
    div()
        .min_w_0()
        .truncate()
        .text_size(theme.ui_px(12.))
        .text_color(theme.crit)
        .child(text)
        .into_any_element()
}

/// The title shown in the top bar and session lists. An explicit
/// `set_session_name` value wins over pi's live auto-title; neither
/// present is a new, unnamed task.
pub(crate) fn session_display_title(
    session_name: Option<&str>,
    current_title: Option<&str>,
) -> String {
    session_name
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            current_title
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| tr!("settings.new_task"))
}

/// "42s" / "3m 12s" / "2h 5m" / "4d 3h" for the Runtime uptime readout.
pub(crate) fn format_uptime(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    match seconds {
        s if s < 60 => format!("{s}s"),
        s if s < 3_600 => format!("{}m {}s", s / 60, s % 60),
        s if s < 86_400 => format!("{}h {}m", s / 3_600, (s % 3_600) / 60),
        s => format!("{}d {}h", s / 86_400, (s % 86_400) / 3_600),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `header_chip` dresses the surface (hairline, glass, hover lift) but
    /// must leave the caller's rounding alone: the quota pill is
    /// `rounded_full` while every other chip takes [`HEADER_CTRL_R`], so a
    /// rest style that also set the corners would square the pill off.
    #[test]
    fn chip_glass_keeps_the_callers_rounding() {
        let theme = Theme::dark();
        let round = div().rounded_full().style().corner_radii.clone();
        let square = div()
            .rounded(px(HEADER_CTRL_R))
            .style()
            .corner_radii
            .clone();
        assert_ne!(round, square);
        assert_eq!(
            header_chip(div().rounded_full(), &theme)
                .style()
                .corner_radii
                .clone(),
            round
        );
        assert_eq!(
            header_chip(div().rounded(px(HEADER_CTRL_R)), &theme)
                .style()
                .corner_radii
                .clone(),
            square
        );
    }
}
