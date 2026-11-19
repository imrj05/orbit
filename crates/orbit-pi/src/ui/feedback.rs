//! Shared loading, empty and error feedback.

use crate::theme::{
    tokens::{looping, panel, DynamicSpacing, IconSize, Radius, TextSize},
    Theme,
};
use crate::ui::banner::{Banner, BannerTone};
use crate::ui::icon::icon;
use gpui::{
    div, px, radians, Animation, AnimationExt, AnyElement, ElementId, FontWeight, Hsla,
    IntoElement, ParentElement, Pixels, Styled, TextAlign, Transformation,
};

/// The app's shared loading spinner: `loader.svg` rotating once per 900 ms,
/// and rendered static under reduce-motion so the perpetual loop can be turned
/// off. In-flight affordances (buttons, panels, tool cards) share this so
/// "working" reads identically everywhere. `id` must be unique per instance —
/// GPUI tracks the animation by it.
pub(crate) fn spinner(
    id: impl Into<ElementId>,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: Theme,
) -> AnyElement {
    spinning_icon("icons/loader.svg", id, size, color, theme)
}

/// One skeleton placeholder: a static bar in the trough (`surface_secondary`)
/// at `Radius::Small`, standing in for a line of text or a figure while a page
/// loads. A skeleton holds the loaded page's shape so the swap does not jump;
/// it never shimmers (a perpetual loop on a page that may load slowly is noise,
/// and reduce-motion would have to turn it off anyway).
pub(crate) fn skeleton_bar(
    width: impl Into<Pixels>,
    height: impl Into<Pixels>,
    theme: &Theme,
) -> gpui::Div {
    div()
        .w(width.into())
        .h(height.into())
        .rounded(Radius::Small.px(theme))
        .bg(theme.page_fill(theme.surface_secondary))
}

/// The sidebar's running-session indicator: one minimal accent arc turning in
/// place, so the row reads as in flight without adding a second ring. Shares
/// the same 900 ms cadence as [`spinner`], and renders static under
/// reduce-motion.
pub(crate) fn running_spinner(
    id: impl Into<ElementId>,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: Theme,
) -> AnyElement {
    spinning_icon("icons/running-spinner.svg", id, size, color, theme)
}

fn spinning_icon(
    path: &'static str,
    id: impl Into<ElementId>,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: Theme,
) -> AnyElement {
    let svg = gpui::svg()
        .path(path)
        .flex_none()
        .size(size.into())
        .text_color(color);
    if theme.reduce_motion() {
        return svg.into_any_element();
    }
    svg.with_animation(id, Animation::new(looping::SPIN).repeat(), |svg, delta| {
        svg.with_transformation(Transformation::rotate(radians(
            delta * std::f32::consts::TAU,
        )))
    })
    .into_any_element()
}

/// The app's one refresh affordance: the refresh glyph itself turning in
/// place while `active`, rather than swapping to a different loader, so the
/// control keeps its shape through the click. Active, it wears the accent —
/// the same read as the provider-usage popover's control; idle, it wears
/// `idle_color` and lifts its ink while a `BUTTON_GROUP` ancestor is
/// hovered. Reduce-motion keeps the glyph and the accent but drops the spin.
///
/// `id` must be unique per instance — GPUI tracks the animation by it.
pub(crate) fn refresh_glyph(
    id: impl Into<ElementId>,
    size: impl Into<Pixels>,
    active: bool,
    idle_color: Hsla,
    theme: Theme,
) -> AnyElement {
    let size: Pixels = size.into();
    if !active {
        return icon("icons/refresh.svg", size, idle_color).into_any_element();
    }
    let svg = gpui::svg()
        .path("icons/refresh.svg")
        .flex_none()
        .size(size)
        .text_color(theme.accent_text);
    if theme.reduce_motion() {
        return svg.into_any_element();
    }
    svg.with_animation(
        id,
        Animation::new(looping::SPIN_FAST).repeat(),
        |svg, delta| {
            svg.with_transformation(Transformation::rotate(radians(
                delta * std::f32::consts::TAU,
            )))
        },
    )
    .into_any_element()
}

/// How an empty/error state fills its parent: `Full` when the parent is a plain
/// sized box (a panel body), `Grow` when the parent is a flex column.
#[derive(Clone, Copy)]
pub(crate) enum EmptyFill {
    Full,
    Grow,
}

/// A centered empty/error state: a large glyph, a medium title, and an
/// optional tertiary detail line. Shared by the Explorer panel, the file
/// viewer, and the Review pane so their empty/error states sit identically;
/// the Git page's variant is `git_panel::widgets::empty_note`.
pub(crate) fn empty_state(
    theme: Theme,
    glyph: &'static str,
    title: &str,
    detail: Option<&str>,
    fill: EmptyFill,
) -> AnyElement {
    empty_state_with_action(theme, glyph, title, detail, fill, None)
}

/// [`empty_state`] with the action that resolves it — Retry after a failed
/// load, Open after an empty selection — centred under the detail line. An
/// empty state offers one when pressing something can actually fix it.
pub(crate) fn empty_state_with_action(
    theme: Theme,
    glyph: &'static str,
    title: &str,
    detail: Option<&str>,
    fill: EmptyFill,
    action: Option<AnyElement>,
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
        .px(DynamicSpacing::Base16.px(&theme))
        .pb(DynamicSpacing::Base24.px(&theme))
        .child(empty_state_glyph(theme, glyph))
        .child(
            div()
                .mt(DynamicSpacing::Base16.px(&theme))
                .text_size(TextSize::Default.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(title.to_string()),
        );
    if let Some(detail) = detail {
        column = column.child(
            div()
                .mt(DynamicSpacing::Base06.px(&theme))
                .max_w(px(360.))
                .text_align(TextAlign::Center)
                .text_size(TextSize::Small.px(&theme))
                .line_height(theme.ui_px(17.))
                .text_color(theme.text_muted)
                .child(detail.to_string()),
        );
    }
    if let Some(action) = action {
        column = column.child(div().mt(DynamicSpacing::Base16.px(&theme)).child(action));
    }
    column.into_any_element()
}

/// The glyph plate every empty state shares: a large, soft raised square with
/// the state's icon centered. Sized so the glyph reads as the subject of the
/// state rather than a stray toolbar icon — monochrome and functional, since
/// DESIGN.md rules out decorative illustration in the workbench.
pub(crate) fn empty_state_glyph(theme: Theme, glyph: &'static str) -> AnyElement {
    div()
        .size(px(64.))
        .flex_none()
        .rounded(panel::RADIUS.px(&theme))
        .border_1()
        .border_color(theme.hairline())
        .bg(theme.page_fill(theme.surface_elevated))
        .flex()
        .items_center()
        .justify_center()
        .child(icon(
            glyph,
            IconSize::Custom(32. / 16.).px(&theme),
            theme.text_secondary,
        ))
        .into_any_element()
}

/// An error surface: what failed, and what to do next.
///
/// Every error in the workbench renders through this or `error_text`. The
/// rule is the phase-8 one: a bare error string is a defect. `title` says what
/// failed, `detail` carries the underlying message (wrapped, never truncated),
/// and the last line is always an action — a button when there is something to
/// press, otherwise a `hint` that names the next step in words.
pub(crate) fn error_state(
    theme: Theme,
    title: &str,
    detail: Option<&str>,
    hint: Option<&str>,
    action: Option<AnyElement>,
) -> AnyElement {
    let mut banner = Banner::new(BannerTone::Error).title(title.to_string());
    if let Some(detail) = detail {
        banner = banner.detail(detail.to_string());
    }
    if let Some(hint) = hint {
        banner = banner.hint(hint.to_string());
    }
    if let Some(action) = action {
        banner = banner.action(action);
    }
    banner.render(&theme)
}

/// How thick a progress bar is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressSize {
    /// 4px — a meter inside a settings row.
    Small,
    /// 5px — a meter in a popover row.
    Medium,
    /// 6px — a meter that is the row's subject (Usage).
    Large,
}

impl ProgressSize {
    fn px(self) -> gpui::Pixels {
        match self {
            Self::Small => px(4.),
            Self::Medium => px(5.),
            Self::Large => px(6.),
        }
    }
}

/// What a progress bar's empty track is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgressTrack {
    /// An opaque well (`surface_secondary`).
    Solid,
    /// Ink at low alpha, for a bar on a translucent card.
    Glass,
}

/// A determinate progress bar: a rounded track and a fill `fraction` of its
/// width, in the caller's status `fill` (quota tints, a category colour).
/// `fraction` is clamped and a non-finite value draws an empty bar, so a bad
/// measurement never paints past the track. With `reveal`, the fill grows in
/// on `Motion::Layout` under that animation id (static under reduce motion).
/// Only show a bar for a real measurement — an unknown value has no bar.
pub(crate) fn progress_bar(
    theme: &Theme,
    fraction: f32,
    fill: Hsla,
    size: ProgressSize,
    track: ProgressTrack,
    reveal: Option<ElementId>,
) -> AnyElement {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let bar = div()
        .h_full()
        .w(gpui::relative(fraction))
        .rounded_full()
        .bg(fill);
    let bar = match reveal {
        Some(id) if !theme.reduce_motion() => bar
            .with_animation(
                id,
                Animation::new(crate::theme::tokens::Motion::Layout.duration(theme))
                    .with_easing(crate::theme::tokens::Motion::Layout.easing()),
                move |bar, progress| bar.w(gpui::relative(fraction * progress)),
            )
            .into_any_element(),
        _ => bar.into_any_element(),
    };
    div()
        .w_full()
        .h(size.px())
        .rounded_full()
        .overflow_hidden()
        .bg(match track {
            ProgressTrack::Solid => theme.surface_secondary,
            ProgressTrack::Glass => theme.overlay_strong,
        })
        .child(bar)
        .into_any_element()
}
