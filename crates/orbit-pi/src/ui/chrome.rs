//! Window chrome: the top bar's glass chips and buttons, the app-drawn caption
//! buttons (minimize / maximize / close) and the window drag strip.
//!
//! Window management itself is the platform's: caption buttons send the system
//! command (`platform::window_command`), and the drag strip asks the platform
//! to move the window. This module owns only how that chrome looks.

use gpui::{
    div, linear_color_stop, linear_gradient, point, px, InteractiveElement, IntoElement,
    MouseButton, ParentElement, Pixels, Refineable, Styled,
};

use crate::motion::Hover;
use crate::platform::WindowCommand;
use crate::theme::{
    tokens::{button, IconSize},
    Theme,
};
use crate::ui::icon::{icon, BUTTON_GROUP};
use crate::ui::style::control::{press, ControlExt, TipExt};

// ── top-bar chip primitives ──

/// Shared height of every top-bar control — the quota chip, the "open in"
/// split, the diff chip and the icon buttons — so a row of mixed
/// affordances reads as one instrument panel instead of assorted sizes.
/// It is Zed's `ButtonSize::Medium` at the default UI size, held fixed
/// because the titlebar's traffic-light clearance is fixed px layout
/// (`view::TITLEBAR_CONTROLS_W`).
pub(crate) const HEADER_CTRL_H: f32 = 28.;

/// Corner radius of the rectangular header chips. Header chips are buttons, so
/// they take [`button::RADIUS`] rather than a fixed 8px: that keeps them in
/// step with the button scale when the Operational design system tightens it.
/// Every header chip, including the quota meter, shares this corner.
pub(crate) fn header_ctrl_radius(theme: &Theme) -> Pixels {
    button::RADIUS.px(theme)
}

/// The lift a chip gets under the cursor (or while its surface is open):
/// denser glass, a stronger hairline, and a tight contact shadow that pulls
/// the chip off the titlebar. Shared by hover *and* open states so a
/// toggle's panel reads as "still held" after the click.
fn header_lifted(theme: &Theme) -> gpui::StyleRefinement {
    let (top, bottom) = theme.header_chip_lift();
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

/// Dress a top-bar chip: ghost at rest, lifting onto the glass on hover. The
/// caller owns the size and the rounding — a control may opt into
/// `rounded_full` when it is genuinely circular, while the rest of the row takes
/// [`header_ctrl_radius`] — and a radius set here would override whichever it
/// picked. The helper must not set a second hover of its own.
///
/// At rest the chip carries no fill and a transparent 1px edge: a row of six
/// framed chips read as a toolbar of stickers, so the glass and hairline are
/// the hover / open affordance ([`header_lift`]) and the bar rests as ink on
/// the titlebar. The edge is reserved so lifting never moves the row.
pub(crate) fn header_chip<S: Styled + InteractiveElement>(el: S, theme: &Theme) -> S {
    let lifted = header_lifted(theme);
    el.border_1()
        .border_color(gpui::transparent_black())
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
            .rounded(header_ctrl_radius(theme))
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
            .rounded(header_ctrl_radius(theme))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover_state(theme),
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
            tr!("platform.minimize"),
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
            if maximized {
                tr!("platform.restore")
            } else {
                tr!("platform.maximize")
            },
        ))
        .child(caption_button(
            theme,
            "window-close",
            "icons/x.svg",
            WindowCommand::Close,
            true,
            tr!("platform.close_window"),
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
    label: String,
) -> impl IntoElement {
    let fill = if close {
        theme.error
    } else {
        theme.hover_wash()
    };
    let mut glyph_el = icon(glyph, IconSize::Small.px(&theme), theme.text_secondary);
    if close {
        // The close glyph rides the caption's red hover; the shared button ink
        // lift already brightens it.
        glyph_el = glyph_el.text_color(theme.text_secondary);
    }
    let button = press(
        div()
            .id(id)
            .group(BUTTON_GROUP)
            .w(px(CAPTION_BUTTON_W))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .on_mouse_up(MouseButton::Left, move |_, window, _| {
                crate::platform::window_command(window, command)
            })
            .tip(label)
            .child(glyph_el),
    );
    crate::motion::hover(button, id, &theme, Hover::wash_in(fill))
}

/// Turn a header strip into a window drag region.
///
/// The press is handed to the OS rather than to GPUI's `WindowControlArea::Drag`
/// on the platform where the app paints the caption — see
/// [`platform::start_window_drag`] for why that path is closed to this app.
/// Elsewhere the platform's own drag area is what moves the window.
///
/// A double-click on a titlebar is the platform's own gesture — zoom, minimise,
/// or nothing, per System Settings → Desktop & Dock. Because the app paints its
/// own titlebar, AppKit's titlebar view never sees the press, so the action is
/// asked for explicitly instead of being handled here: `titlebar_double_click`
/// reads `AppleActionOnDoubleClick` and runs exactly what every other app on the
/// machine would run. A single click stays a no-op, so the drag strip is
/// unaffected.
pub(crate) fn window_drag_region(el: gpui::Div) -> gpui::Div {
    #[cfg(target_os = "macos")]
    let el = el.on_mouse_down(
        MouseButton::Left,
        |event: &gpui::MouseDownEvent, window, _| {
            // The second press of a double-click carries the count; a drag starts
            // on the first, so this never fires for one.
            if event.click_count == 2 {
                window.titlebar_double_click();
            }
        },
    );
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

#[cfg(test)]
mod helpers_moved_tests {
    use super::*;
    use crate::theme::tokens::button;
    use gpui::{div, Styled};

    /// `header_chip` dresses the surface (hairline, glass, hover lift) but
    /// must leave the caller's rounding alone: a circular control may use
    /// `rounded_full` while the rectangular chips take [`header_ctrl_radius`],
    /// so a rest style that also set the corners would square the circle off.
    #[test]
    fn chip_glass_keeps_the_callers_rounding() {
        let theme = Theme::dark();
        let round = div().rounded_full().style().corner_radii.clone();
        let square = div()
            .rounded(header_ctrl_radius(&theme))
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
            header_chip(div().rounded(header_ctrl_radius(&theme)), &theme)
                .style()
                .corner_radii
                .clone(),
            square
        );
    }

    /// Header chips are buttons: their corner must track the button scale, not
    /// a fixed pixel value, so Operational's tighter radius reaches them too.
    #[test]
    fn header_radius_tracks_the_button_scale() {
        for theme in [
            Theme::dark(),
            Theme::for_id(crate::theme::ThemeId::Operational),
        ] {
            assert_eq!(
                header_ctrl_radius(&theme),
                button::RADIUS.px(&theme),
                "header chip radius drifted from the button radius"
            );
        }
    }
}
