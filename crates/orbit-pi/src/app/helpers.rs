use super::*;
use crate::motion::Hover;
use crate::platform::WindowCommand;
use crate::theme::tokens::{
    self, button, context_menu, input, list, list_item, looping, panel, picker, popover,
    BufferLineHeight, ButtonSize, DynamicSpacing, IconSize, InteractionState, Motion, Radius,
    StyledExt, TextSize,
};
use crate::usage::tooltip::Tooltip;
use gpui::{Font, FontFeatures, FontStyle, InteractiveElement};

/// Attach Orbit's native tooltip to any interactive element.
///
/// The send / stop button set the register: a localized label, usually with the
/// keyboard shortcut appended (e.g. `Send (↵)`). This keeps every icon-only
/// button's hover affordance one line at the call site instead of repeating the
/// `Tooltip::new` closure.
pub(crate) trait TipExt: InteractiveElement {
    fn tip(mut self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.interactivity()
            .tooltip(move |_, cx| cx.new(|_| Tooltip::new(label.clone())).into());
        self
    }
}

impl<T: InteractiveElement> TipExt for T {}

/// A compact keyboard key, suitable for an inline shortcut hint.
pub(crate) fn kbd_key(label: impl Into<SharedString>, theme: &Theme) -> AnyElement {
    div()
        .h(ButtonSize::Compact.height(theme))
        .min_w(ButtonSize::Compact.height(theme))
        .px(DynamicSpacing::Base04.px(theme))
        .rounded(Radius::Small.px(theme))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .bg(theme.overlay_strong)
        .text_size(TextSize::XSmall.px(theme))
        .text_color(theme.text_secondary)
        .child(label.into())
        .into_any_element()
}

/// Keyboard legend shared by both working-folder pickers. The remote level
/// uses Escape to go back; elsewhere it closes the picker.
pub(crate) fn working_folder_keyboard_hints(theme: &Theme, back: bool) -> gpui::Div {
    let group = |id: &'static str, keys: &[&str], label: String| {
        div()
            .debug_selector(move || id.to_string())
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base02.px(theme))
            .children(
                keys.iter()
                    .map(|key| kbd_key(crate::platform::shortcuts::label(key), theme)),
            )
            .child(div().whitespace_nowrap().child(label))
    };
    div()
        .debug_selector(|| "working-folder-keyboard-hints".to_string())
        .flex_none()
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(DynamicSpacing::Base08.px(theme))
        .px(picker::search_padding_x(theme))
        .py(DynamicSpacing::Base06.px(theme))
        .border_t_1()
        .border_color(theme.border)
        .text_size(TextSize::XSmall.px(theme))
        .text_color(theme.text_muted)
        .child(group(
            "working-folder-hint-navigate",
            &["up", "down"],
            tr!("working_folder.hint_navigate"),
        ))
        .child(group(
            "working-folder-hint-select",
            &["enter"],
            tr!("working_folder.hint_select"),
        ))
        .child(group(
            if back {
                "working-folder-hint-back"
            } else {
                "working-folder-hint-close"
            },
            &["escape"],
            if back {
                tr!("common.back")
            } else {
                tr!("common.close")
            },
        ))
}

/// Group the platform-specific key labels of a canonical shortcut chord.
pub(crate) fn kbd_shortcut(chord: &str, theme: &Theme) -> AnyElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base02.px(theme))
        .children(
            chord
                .split('-')
                .filter(|part| !part.is_empty())
                .map(|part| kbd_key(crate::platform::shortcuts::label(part), theme)),
        )
        .into_any_element()
}

/// The interaction-state contract, applied to an element.
///
/// Every interactive surface answers the same six states, and the colours come
/// from [`InteractionState`] rather than from the call site. Phase 2 of the
/// UI/UX Modernization epic routes existing call sites through this trait, so a
/// surface that wants a hover no longer names `surface_hover` itself.
///
/// ```ignore
/// button_frame(div().id("save"), &theme, ButtonSize::Default)
///     .hover_state(&theme)
///     .child(label)
/// ```
pub(crate) trait ControlExt: InteractiveElement + Styled + Sized {
    /// Hover: the contract's fill under primary ink. Replaces
    /// `.hover(|s| s.bg(theme.surface_hover))` and its `raised` twins.
    fn hover_state(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let ink = InteractionState::Hover.control_ink(theme);
        self.hover(move |style| style.bg(fill).text_color(ink))
    }

    /// Hover for a control that floats over a translucent surface or the
    /// wallpaper (header chips, in-scroller affordances, glass trays). The
    /// wash is ink at low alpha — `overlay_strong` — so what is behind the
    /// control stays visible; `surface_hover` would paint an opaque slab.
    fn hover_on_glass(self, theme: &Theme) -> Self {
        let wash = InteractionState::Glass.glass_fill(theme);
        self.hover(move |style| style.bg(wash))
    }

    /// Hover for a field or track: the frame steps to `border_strong` and the
    /// fill does not change. Used where the control is defined by its outline
    /// — text fields, sliders, and segmented tracks.
    fn hover_border(self, theme: &Theme) -> Self {
        let edge = InteractionState::Hover
            .border(theme)
            .unwrap_or(theme.border_strong);
        self.hover(move |style| style.border_color(edge))
    }

    /// Hover for a primary action: the fill steps to `action_hover`, the step
    /// the `action` role exists for.
    fn hover_action(self, theme: &Theme) -> Self {
        let fill = theme.action_hover;
        self.hover(move |style| style.bg(fill))
    }

    /// Hover for a destructive control: the fill steps to `error_hover`.
    fn hover_danger(self, theme: &Theme) -> Self {
        let fill = theme.error_hover;
        self.hover(move |style| style.bg(fill))
    }

    /// Hover for a destructive control that is quiet at rest: an error-tinted
    /// border and wash, with the error ink appearing only under the pointer.
    fn hover_danger_soft(self, theme: &Theme) -> Self {
        let border = theme.error.opacity(0.6);
        let fill = theme.error.opacity(0.08);
        let ink = theme.error;
        self.hover(move |style| style.border_color(border).bg(fill).text_color(ink))
    }

    /// Hover for an accent-filled control: the fill steps to `accent_hover`.
    fn hover_accent(self, theme: &Theme) -> Self {
        let fill = theme.accent_hover;
        self.hover(move |style| style.bg(fill))
    }

    /// Hover on a raised surface: the elevated fill.
    fn hover_elevated(self, theme: &Theme) -> Self {
        let fill = theme.surface_elevated;
        self.hover(move |style| style.bg(fill))
    }

    /// Hover for a control that is defined by its outline: the wash plus the
    /// strong hairline, no ink change.
    fn hover_bordered(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let edge = theme.border_strong;
        self.hover(move |style| style.bg(fill).border_color(edge))
    }

    /// `hover_bordered`, and the ink lifts to primary as well.
    fn hover_bordered_ink(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let edge = theme.border_strong;
        let ink = theme.text_primary;
        self.hover(move |style| style.bg(fill).border_color(edge).text_color(ink))
    }

    /// A wash that keeps secondary ink — a row whose label must not brighten
    /// just because the pointer crossed it.
    fn hover_quiet(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let ink = theme.text_secondary;
        self.hover(move |style| style.bg(fill).text_color(ink))
    }

    /// Ink only: a control with no fill of its own brightens to primary.
    fn hover_ink(self, theme: &Theme) -> Self {
        let ink = theme.text_primary;
        self.hover(move |style| style.text_color(ink))
    }

    /// Ink only, stepping *down* to secondary: used where the resting state is
    /// primary and the pointer should quiet it.
    fn hover_ink_quiet(self, theme: &Theme) -> Self {
        let ink = theme.text_secondary;
        self.hover(move |style| style.text_color(ink))
    }

    /// Ink only, to the accent's readable role (a link brightening).
    fn hover_ink_accent(self, theme: &Theme) -> Self {
        let ink = theme.accent_text;
        self.hover(move |style| style.text_color(ink))
    }

    /// The translucent ink wash for a surface that is itself translucent.
    fn hover_overlay(self, theme: &Theme) -> Self {
        let wash = theme.overlay;
        self.hover(move |style| style.bg(wash))
    }

    /// The stronger translucent wash, with primary ink.
    fn hover_overlay_strong(self, theme: &Theme) -> Self {
        let wash = theme.overlay_strong;
        let ink = theme.text_primary;
        self.hover(move |style| style.bg(wash).text_color(ink))
    }

    /// The hover a control takes while its surface is already open: it stays
    /// on the selected step, because a second wash would read as a new state.
    fn hover_state_open(self, theme: &Theme, open: bool) -> Self {
        let fill = if open {
            InteractionState::Pressed
                .fill(theme)
                .expect("Pressed fills")
        } else {
            InteractionState::Hover.fill(theme).expect("Hover fills")
        };
        self.hover(move |style| style.bg(fill))
    }

    /// An accent-tinted wash. The hue is always the accent; the call site names
    /// how strong, never which colour.
    fn hover_accent_wash(self, theme: &Theme, wash: AccentWash) -> Self {
        let fill = theme.accent.opacity(wash.alpha());
        self.hover(move |style| style.bg(fill))
    }

    /// A hairline that strengthens under the pointer (dividers, tracks).
    fn hover_divider(self, theme: &Theme) -> Self {
        let edge = theme.border_strong;
        self.hover(move |style| style.bg(edge))
    }

    /// A row wash whose ink steps to the accent's readable role — a link row.
    fn hover_accent_ink(self, theme: &Theme) -> Self {
        let fill = InteractionState::Hover.fill(theme).expect("Hover fills");
        let ink = theme.accent_text;
        self.hover(move |style| style.bg(fill).text_color(ink))
    }

    /// An inverse fill: the ink colour as a background, for a small sample or
    /// swatch that previews the foreground it would paint.
    fn hover_inverse(self, theme: &Theme) -> Self {
        let fill = theme.text_primary;
        self.hover(move |style| style.bg(fill))
    }

    /// A dim rather than a colour change: image thumbs and previews.
    fn hover_dimmed(self) -> Self {
        self.hover(|style| style.opacity(0.9))
    }

    /// Focus: the 1px ring, drawn from the token, on every focusable control.
    /// A no-op on an element that cannot take focus.
    fn focus_state(self, theme: &Theme) -> Self {
        let ring = InteractionState::Focused
            .border(theme)
            .expect("Focused draws the ring");
        self.focus(move |style| style.border_1().border_color(ring))
    }
}

impl<T: InteractiveElement + Styled> ControlExt for T {}

/// The fill a control takes while the pointer is down.
///
/// A ghost or neutral control deepens its wash ([`PressedRamp::Wash`]).
/// A filled control steps its own role instead: `surface_active` would paint a
/// grey wash over a primary, accent or danger fill and change what the button
/// means while it is being clicked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PressedRamp {
    /// `surface_active` — the contract's pressed fill.
    Wash,
    /// `surface_hover` — for a control that is already sitting on
    /// `surface_active` because it is selected or its surface is open. The
    /// ramp has no deeper step, so the press steps back to the hover wash:
    /// the movement is the feedback, and the control still reads as held.
    Selected,
    /// `action_pressed` — a primary button.
    Action,
    /// `accent_pressed` — an accent-filled control.
    Accent,
    /// `error_hover` — a destructive control.
    Danger,
}

impl PressedRamp {
    fn fill(self, theme: &Theme) -> Hsla {
        match self {
            Self::Wash => InteractionState::Pressed
                .fill(theme)
                .expect("Pressed fills"),
            Self::Selected => InteractionState::Hover.fill(theme).expect("Hover fills"),
            Self::Action => theme.action_pressed,
            Self::Accent => theme.accent_pressed,
            Self::Danger => theme.error_hover,
        }
    }
}

/// The three strengths an accent-tinted hover may use. Named so a call site
/// says how strong, not which colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccentWash {
    /// A quiet tint over a large surface.
    Faint,
    /// The default: ask/approve actions.
    Soft,
    /// A deliberate highlight, e.g. a drag handle under the pointer.
    Strong,
}

impl AccentWash {
    fn alpha(self) -> f32 {
        match self {
            Self::Faint => 0.20,
            Self::Soft => 0.26,
            Self::Strong => 0.40,
        }
    }
}

/// The states GPUI only tracks on a keyed element.
///
/// Press and pointer affordances live on `StatefulInteractiveElement`, so this
/// half of the contract is a separate trait; a plain `div()` gets the hover and
/// focus halves, a `div().id(…)` gets all four.
pub(crate) trait StatefulControlExt:
    gpui::StatefulInteractiveElement + Styled + Sized
{
    /// Pressed: the control's own ramp, never a colour picked at the call site.
    fn pressed_state(self, theme: &Theme, ramp: PressedRamp) -> Self {
        let fill = ramp.fill(theme);
        self.active(move |style| style.bg(fill))
    }

    /// Disabled: a state, not an opacity. The disabled fill, ink and quiet
    /// frame, plus a non-interactive pointer. Callers drop their handlers — a
    /// control that looks disabled must not still activate.
    fn disabled_state(self, theme: &Theme) -> Self {
        let fill = InteractionState::Disabled
            .fill(theme)
            .expect("Disabled fills");
        let border = InteractionState::Disabled
            .border(theme)
            .expect("Disabled keeps its frame");
        let ink = InteractionState::Disabled.control_ink(theme);
        self.cursor_not_allowed()
            .bg(fill)
            .text_color(ink)
            .border_1()
            .border_color(border)
    }
}

impl<T: gpui::StatefulInteractiveElement + Styled> StatefulControlExt for T {}

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

/// A queued message's position in the whole strip, steering rows first — the
/// one number element ids key on, so a row keeps its identity when the lanes
/// are rendered in one pass.
pub(crate) fn queue_row_slot(queue: &PendingQueue, target: QueuedMessage) -> u64 {
    match target.lane {
        QueueLane::Steer => target.index as u64,
        QueueLane::FollowUp => (queue.steering.len() + target.index) as u64,
    }
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
/// muted `text_secondary`/`text_muted` glyph visibly brightens.
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
///
/// `size` is px: a literal, or a token such as `IconSize::Small.px(&theme)`.
pub(crate) fn icon(path: &'static str, size: impl Into<Pixels>, color: Hsla) -> gpui::Svg {
    let hover = icon_hover_ink(color);
    gpui::svg()
        .path(path)
        .flex_none()
        .size(size.into())
        .text_color(color)
        .group_hover(BUTTON_GROUP, move |style| style.text_color(hover))
}

/// Same as [`icon`] but for runtime-computed paths (per-provider marks).
pub(crate) fn icon_dyn(path: SharedString, size: impl Into<Pixels>, color: Hsla) -> gpui::Svg {
    let hover = icon_hover_ink(color);
    gpui::svg()
        .path(path)
        .flex_none()
        .size(size.into())
        .text_color(color)
        .group_hover(BUTTON_GROUP, move |style| style.text_color(hover))
}

/// The advance width of one line of UI text at `size`, shaped with the app's
/// UI face.
///
/// A control that reserves room for a label it is about to reveal needs the
/// label's real width: a fixed reserve stops fitting as soon as the UI font
/// size, the spacing density or the locale changes. Shaping goes through the
/// window's text system, so callers pass the window they are rendering.
pub(crate) fn text_width(window: &Window, text: &str, size: Pixels) -> Pixels {
    if text.is_empty() {
        return px(0.);
    }
    let run = gpui::TextRun {
        len: text.len(),
        font: gpui::font(theme::ui_font_family()),
        color: Hsla::transparent_black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    window
        .text_system()
        .shape_line(SharedString::from(text.to_string()), size, &[run], None)
        .width
}

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
/// `idle_color` and lifts its ink while a [`BUTTON_GROUP`] ancestor is
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

/// Anchor a popover to its trigger, deferred so it paints above the surface it
/// belongs to.
///
/// Every menu, dropdown and picker in the workbench is positioned this way: the
/// caller supplies the corner and the offset, the shell supplies the snap-to-
/// window margin. A menu that measures in window space cannot flip, so it takes
/// [`anchored_popover_in_window`] instead and does not snap.
pub(crate) fn anchored_popover(
    corner: Corner,
    offset: Point<Pixels>,
    popup: impl IntoElement + 'static,
    id: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(corner)
        .offset(offset)
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        // Every menu enters here, so a new one cannot forget its entrance.
        .child(deferred(popover_enter_from(
            corner,
            id,
            div().child(popup),
            theme,
        )))
        .into_any_element()
}

/// Anchor a popover at a point the caller computed — a context menu at the
/// pointer, a menu that opens beside a row rather than under a trigger.
pub(crate) fn anchored_popover_at(
    position: Point<Pixels>,
    popup: impl IntoElement + 'static,
    id: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    anchored()
        .position(position)
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        // A context menu at the pointer has no anchor edge to travel from, so it
        // fades only.
        .child(deferred(popover_enter(id, div().child(popup), theme)))
        .into_any_element()
}

/// Anchor a popover measured in window space. No snap: the position is
/// absolute, and sliding it would move the menu away from its trigger.
pub(crate) fn anchored_popover_in_window(
    corner: Corner,
    offset: Point<Pixels>,
    popup: impl IntoElement + 'static,
    id: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    anchored()
        .position_mode(AnchoredPositionMode::Window)
        .anchor(corner)
        .offset(offset)
        .child(deferred(popover_enter_from(
            corner,
            id,
            div().child(popup),
            theme,
        )))
        .into_any_element()
}

/// A popover's entrance: fade in on [`Motion::Interaction`], or appear at once
/// when Reduce Motion is on. The guard and the cadence belong to the shell, so
/// a new popup cannot forget either.
pub(crate) fn popover_enter<E>(id: impl Into<SharedString>, el: E, theme: &Theme) -> AnyElement
where
    E: Styled + IntoElement + 'static,
{
    crate::motion::enter(el, id, theme, Motion::Interaction, None)
}

/// A menu's entrance: the fade above, plus an 8px travel out of the anchor edge
/// so the card reads as arriving from the control that opened it. Dismissal
/// stays instant — a menu that lingers after a click is a tax on the next one.
pub(crate) fn popover_enter_from<E>(
    corner: Corner,
    id: impl Into<SharedString>,
    el: E,
    theme: &Theme,
) -> AnyElement
where
    E: Styled + IntoElement + 'static,
{
    crate::motion::enter(el, id, theme, Motion::Interaction, Some(corner))
}

/// The frame every modal shares: the elevated surface, its floating shadow and
/// panel corner, a clipped column, and a hitbox that stops a click on the card
/// from reaching the scrim beneath it.
///
/// Callers add the layer id, the width and height bounds, the key context, and
/// the children. Nothing about a dialog's size belongs here — only what every
/// dialog must be.
pub(crate) fn modal_card(theme: &Theme) -> gpui::Div {
    div()
        .elevation_3(theme)
        .flex()
        .flex_col()
        .overflow_hidden()
        .occlude()
        .font_family(theme::ui_font_family())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// The backdrop every modal shares: a full-window scrim that centres its card
/// and dims the workbench beneath. Callers add the layer id, the dismissal
/// handler, the key context, and the card.
pub(crate) fn modal_scrim(theme: &Theme) -> gpui::Div {
    div()
        .absolute()
        .inset_0()
        .occlude()
        .bg(theme.scrim_modal())
        .px(DynamicSpacing::Base24.px(theme))
        .py(DynamicSpacing::Base24.px(theme))
        .flex()
        .items_center()
        .justify_center()
}

/// The shell of a context menu, built to Zed's `ContextMenu`: the elevated
/// surface (`elevation_2`), a 200px minimum width, `List`'s vertical padding,
/// and the Comfortable line height a menu forces so its entries measure the
/// same wherever it is deferred from. Callers keep positioning, occlusion,
/// dismissal, and a fixed width when the menu needs one.
pub(crate) fn context_menu_surface<E: Styled>(el: E, theme: &Theme) -> E {
    el.elevation_2(theme)
        .min_w(context_menu::MIN_WIDTH)
        .py(list::padding_y(theme))
        .font_family(theme::ui_font_family())
        .text_size(context_menu::TEXT.px(theme))
        .line_height(context_menu::LINE_HEIGHT.relative())
}

/// One context-menu entry, laid out like Zed's inset `ListItem`: Base04
/// outside, Base06 inside, 6px corners, one Comfortable line tall, and the
/// entry label's `gap_1p5` between icon and label. Callers add the id,
/// colors, hover, and click handling; icons take `context_menu::ICON`.
pub(crate) fn context_menu_entry<E: Styled>(el: E, theme: &Theme) -> E {
    el.h(context_menu::entry_height(theme))
        .mx(list_item::inset(theme))
        .px(list_item::padding_x(theme))
        .rounded(list_item::RADIUS.px(theme))
        .flex()
        .items_center()
        .gap(context_menu::icon_gap(theme))
        .text_size(context_menu::TEXT.px(theme))
}

/// Zed's `ListSeparator`: a full-width hairline with Base06 above and below.
pub(crate) fn context_menu_separator(theme: &Theme) -> gpui::Div {
    div()
        .h(tokens::BORDER_WIDTH)
        .w_full()
        .my(list::separator_margin_y(theme))
        .bg(theme.border)
}

/// Zed's inset `ListSubHeader`, the section label inside a menu or picker:
/// a 20px row of Small, muted text, Base02 + `px_2` in from the edges, with
/// Base04 below.
pub(crate) fn menu_header(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div()
        .w_full()
        .flex_none()
        .flex()
        .px(list::sub_header_padding_x(theme))
        .pb(list::sub_header_padding_bottom(theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .h(list::sub_header_height(theme))
                .px(list::sub_header_inset_x(theme))
                .flex()
                .items_center()
                .gap(theme.rems(0.25))
                .truncate()
                .text_size(list::SUB_HEADER_TEXT.px(theme))
                .text_color(theme.text_muted)
                .child(label.into()),
        )
}

/// The shell of a picker (search + list), built to Zed's `Picker`: the modal
/// elevation it draws even as a popover, and the UI face at Default size.
/// Callers keep their width, positioning, occlusion, and dismissal.
pub(crate) fn picker_surface<E: Styled>(el: E, theme: &Theme) -> E {
    el.elevation_3(theme)
        .font_family(theme::ui_font_family())
        .text_size(picker::TEXT.px(theme))
}

/// A picker's search row, Zed's picker head: 36px tall, `px_2p5`, a hairline
/// below, and Default-size input text. Put the `ComposerInput` in a
/// `flex_1` child; a leading search icon takes `IconSize::Small`.
pub(crate) fn picker_search_frame<E: Styled>(el: E, theme: &Theme) -> E {
    el.flex_none()
        .h(picker::search_height(theme))
        .px(picker::search_padding_x(theme))
        .flex()
        .items_center()
        .gap(input::gap(theme))
        .overflow_hidden()
        .border_b_1()
        .border_color(theme.border)
        .text_size(TextSize::Default.px(theme))
}

/// One picker row, Zed's inset `ListItem` at `Sparse` spacing: Base04
/// outside, Base06 inside, 4px above and below, 6px corners, and at least
/// one Comfortable line of Default text (31px). A row with a secondary line
/// grows to fit it (see `picker::two_line_entry_height`). Callers add the id,
/// colors, hover, selection, and click handling.
pub(crate) fn picker_entry<E: Styled + InteractiveElement>(el: E, theme: &Theme) -> E {
    el.min_h(picker::entry_height(theme))
        .mx(list_item::inset(theme))
        .px(list_item::padding_x(theme))
        .py(picker::ROW_SPACING.padding_y(theme))
        .rounded(list_item::RADIUS.px(theme))
        .flex()
        .items_center()
        .gap(list_item::content_gap(theme))
        .text_size(picker::TEXT.px(theme))
        .focus_state(theme)
}

/// One fixed text line box inside a picker row, at the Comfortable leading
/// [`picker::two_line_entry_height`] measures. Used by [`PickerRow`].
fn picker_line_height(size: TextSize, theme: &Theme) -> gpui::Pixels {
    BufferLineHeight::Comfortable.resolve(size.px(theme))
}

/// The text body of a two-line picker row: a title over a secondary detail
/// line, each on a fixed line box so the pair is exactly
/// [`picker::two_line_entry_height`] tall.
///
/// Every picker uses this one builder — the model selector's label column, the
/// "Work in" lists, the folder picker — so their rows share a rhythm and a row's
/// text can never spill into the row below it (which is what happens when a
/// two-line body is given a one-line height).
pub(crate) struct PickerRow {
    title: String,
    detail: Option<String>,
    detail_mono: bool,
    weight: gpui::FontWeight,
    title_color: Hsla,
    detail_color: Hsla,
    /// A leading glyph, drawn on the **title's** line.
    icon: Option<(&'static str, Hsla)>,
}

impl PickerRow {
    /// A row body with the picker's regular ink: `title_color` for the title,
    /// `detail_color` for the second line.
    pub(crate) fn new(title: impl Into<String>, title_color: Hsla, detail_color: Hsla) -> Self {
        Self {
            title: title.into(),
            detail: None,
            detail_mono: false,
            weight: gpui::FontWeight::NORMAL,
            title_color,
            detail_color,
            icon: None,
        }
    }

    /// The leading glyph. It is drawn on the title's line and the detail line
    /// is indented to clear it, so one row reads as one block: glyph and title
    /// paired, description underneath. Centering the glyph in a two-line row
    /// instead leaves it floating between the lines, belonging to neither —
    /// which is what makes a scrolling list look like two things sliding past
    /// each other.
    pub(crate) fn icon(mut self, path: &'static str, color: Hsla) -> Self {
        self.icon = Some((path, color));
        self
    }

    /// The second line: a path, a branch, a hint.
    pub(crate) fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Draw the second line in the code face — paths and branches are machine
    /// text, the same register the model picker uses for an id.
    pub(crate) fn mono(mut self) -> Self {
        self.detail_mono = true;
        self
    }

    /// The title's weight. Pickers mark the highlighted/selected row with
    /// Medium rather than a second color.
    pub(crate) fn weight(mut self, weight: gpui::FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// The row body. It is the flexible child (`flex_1` + `min_w_0`), so the
    /// row's trailing slot — a check, an age, a tag — lands at the right edge
    /// and long text ellipsizes inside the row instead of spilling past it.
    /// Wrap it in another div and that promise is gone.
    pub(crate) fn into_element(self, theme: &Theme) -> AnyElement {
        let gap = list_item::content_gap(theme);
        let icon_size = context_menu::ICON.px(theme);
        let leading = self.icon.map(|(path, color)| {
            div()
                .debug_selector(|| "picker-row-leading".to_string())
                .flex_none()
                // Exactly the title's line box, so the glyph and the title
                // share one optical line whatever the row's total height.
                .h(picker_line_height(picker::TEXT, theme))
                .flex()
                .items_center()
                .child(icon(path, icon_size, color))
        });
        let indent = if leading.is_some() {
            icon_size + gap
        } else {
            gpui::px(0.)
        };
        let detail = self.detail.map(|detail| {
            let line = div()
                .w_full()
                .pl(indent)
                .truncate()
                .text_size(picker::SECONDARY_TEXT.px(theme))
                .line_height(picker_line_height(picker::SECONDARY_TEXT, theme))
                .text_color(self.detail_color)
                .child(detail);
            if self.detail_mono {
                line.font_family(theme::code_font_family())
                    .into_any_element()
            } else {
                line.into_any_element()
            }
        });
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(gap)
                    .children(leading)
                    .child(
                        div()
                            .debug_selector(|| "picker-row-title".to_string())
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(self.weight)
                            .line_height(picker_line_height(picker::TEXT, theme))
                            .text_color(self.title_color)
                            .child(self.title),
                    ),
            )
            .children(detail)
            .into_any_element()
    }
}

/// The row height a two-line [`PickerRow`] fills exactly.
pub(crate) fn picker_row_height(theme: &Theme) -> gpui::Pixels {
    picker::two_line_entry_height(theme)
}

/// A labeled button's frame, Zed's `ButtonLike`: the size's height and
/// horizontal padding, Base04 between icon and label, [`button::radius`],
/// and a one-line label at 1× line height (Zed's `UiLabel`) so it sits
/// inside even a 22px button. The label size follows [`button::label_size`]
/// and an icon inside it takes [`ButtonSize::icon_size`], so every button's
/// glyph matches its frame. Callers keep colors, border, hover, press, and
/// click handling.
pub(crate) fn button_frame<E: Styled + gpui::InteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
) -> E {
    el.flex_none()
        .h(size.height(theme))
        .px(size.padding_x(theme))
        .flex()
        .items_center()
        .justify_center()
        .gap(button::gap(theme))
        .rounded(button::radius(size).px(theme))
        .whitespace_nowrap()
        .text_size(button::label_size(size).px(theme))
        .line_height(relative(1.))
        .focus_state(theme)
}

/// Where a segment sits inside a segmented control (a grouped row of
/// [`button_frame`]s sharing one control outline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentPosition {
    First,
    Middle,
    Last,
    /// The only segment — rounds every corner like a standalone button.
    Only,
}

impl SegmentPosition {
    /// The position of `index` in a group of `count` segments.
    pub(crate) fn at(index: usize, count: usize) -> Self {
        match (index, count) {
            (_, 0 | 1) => Self::Only,
            (0, _) => Self::First,
            (index, count) if index + 1 == count => Self::Last,
            _ => Self::Middle,
        }
    }
}

/// One segment of a segmented control: a [`button_frame`] trimmed to its
/// [`SegmentPosition`] so abutting segments round only the group's outer
/// corners and share a single hairline between them. The segments read as
/// one control while each keeps a real button's size, padding, label, and
/// [`button::RADIUS`]. Callers keep the colors, hover, press, and click
/// handling, exactly as with a standalone button.
pub(crate) fn segmented_segment<E: Styled + gpui::InteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
    position: SegmentPosition,
) -> E {
    let radius = button::radius(size).px(theme);
    let el = button_frame(el, theme, size).rounded_none().border_1();
    match position {
        SegmentPosition::Only => el.rounded(radius),
        SegmentPosition::First => el.rounded_l(radius),
        SegmentPosition::Last => el.rounded_r(radius).border_l_0(),
        SegmentPosition::Middle => el.border_l_0(),
    }
}

/// The app's one disclosure chevron: `chevron-right`, turning a quarter turn to
/// point down while `open`, on [`Motion::Interaction`].
///
/// One glyph rotates instead of two glyphs swapping, because a swap cannot be
/// animated — the eye reads a jump where the shape should carry through. Reduce
/// Motion turns it instantly, which is the same picture without the travel.
///
/// `id` must be unique per chevron: it keys the transition.
pub(crate) fn disclosure_chevron(
    id: impl Into<SharedString>,
    open: bool,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: &Theme,
) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/chevron-right.svg")
        .flex_none()
        .size(size.into())
        .text_color(color);
    crate::motion::state(
        svg,
        id,
        theme,
        Motion::Interaction,
        if open { 1.0 } else { 0.0 },
        |svg, value| svg.with_transformation(Transformation::rotate(radians(chevron_angle(value)))),
    )
}

/// The angle for a point on the chevron's turn: 0 while closed, a quarter turn
/// (90°) while open.
fn chevron_angle(value: f32) -> f32 {
    std::f32::consts::FRAC_PI_2 * value.clamp(0.0, 1.0)
}

/// The caret on a picker trigger: `chevron-down` at rest, turning a half turn
/// to point up while its menu is open.
///
/// This is deliberately not [`disclosure_chevron`]: a tree row's right/down pair
/// means collapsed/expanded, while a menu button that opens *below* its trigger
/// points down at rest. Using the disclosure glyph for both made every composer
/// chip read as a breadcrumb (`Full access ›`, `Build ›`, `Work in … ›`) even
/// though each one opens a picker in place.
///
/// One glyph rotates instead of two glyphs swapping, on the same
/// [`Motion::Interaction`] role and with the same Reduce Motion result.
pub(crate) fn dropdown_chevron(
    id: impl Into<SharedString>,
    open: bool,
    size: impl Into<Pixels>,
    color: Hsla,
    theme: &Theme,
) -> AnyElement {
    let svg = gpui::svg()
        .path("icons/chevron-down.svg")
        .flex_none()
        .size(size.into())
        .text_color(color);
    crate::motion::state(
        svg,
        id,
        theme,
        Motion::Interaction,
        if open { 1.0 } else { 0.0 },
        |svg, value| {
            svg.with_transformation(Transformation::rotate(radians(dropdown_angle(value))))
        },
    )
}

/// The angle for a point on a dropdown caret's turn: 0 while closed (pointing
/// down), a half turn (180°) while open (pointing up).
fn dropdown_angle(value: f32) -> f32 {
    std::f32::consts::PI * value.clamp(0.0, 1.0)
}

/// The app's one switch: a 36×20 track and a 14px knob that travels between the
/// ends on [`Motion::Interaction`], with the track cross-fading from
/// `surface_elevated` to `accent`.
///
/// The three settings switches (Reduce Motion, automatic updates, a skill's
/// enablement) all render through this, so a switch cannot drift in shape or in
/// motion. `id` must be unique per switch — it keys the transition as well as
/// the element.
pub(crate) fn switch(
    id: impl Into<SharedString>,
    on: bool,
    theme: &Theme,
    on_click: impl Fn(&gpui::MouseUpEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    let theme = *theme;
    let id = id.into();
    let padding = DynamicSpacing::Base02.px(&theme);
    let travel = switch_travel(&theme);
    let track = div()
        .id(gpui::ElementId::Name(id.clone()))
        .w(px(36.))
        .h(DynamicSpacing::Base20.px(&theme))
        .rounded_full()
        .p(padding)
        .border_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .cursor_pointer()
        .hover_border(&theme)
        .on_mouse_up(MouseButton::Left, on_click);
    crate::motion::state(
        track,
        id,
        &theme,
        Motion::Interaction,
        if on { 1.0 } else { 0.0 },
        move |track, value| {
            track
                .bg(crate::motion::blend(
                    theme.surface_elevated,
                    theme.accent,
                    value,
                ))
                .child(
                    div()
                        .flex_none()
                        .size(px(14.))
                        .rounded_full()
                        .bg(theme.toggle_knob)
                        // Relative, so the knob travels without re-laying out the
                        // track — and without moving the hitbox the click lands on.
                        .relative()
                        .left(travel * value),
                )
        },
    )
}

/// How far a switch's knob travels: the track, less its padding and the knob.
fn switch_travel(theme: &Theme) -> Pixels {
    px(36.) - DynamicSpacing::Base02.px(theme) * 2.0 - px(14.)
}

/// An icon-only button's frame, Zed's `IconButton`: a square as tall as its
/// [`ButtonSize`], [`button::radius`], the icon centered at
/// [`ButtonSize::icon_size`]. Callers keep colors, hover, press, tooltip, and
/// click handling.
pub(crate) fn icon_button_frame<E: Styled + gpui::InteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
) -> E {
    el.flex_none()
        .size(size.height(theme))
        .flex()
        .items_center()
        .justify_center()
        .rounded(button::radius(size).px(theme))
        .focus_state(theme)
}

/// A text field's box, Zed's `InputField`: at least 32px tall, `px_2`
/// `py_1p5`, a 1px hairline, [`input::RADIUS`], and Default-size text, which the
/// `ComposerInput` inside inherits. Callers set the fill (and a focus border)
/// and give the input a `flex_1` wrapper when an icon shares the row.
pub(crate) fn input_field_frame<E: Styled>(el: E, theme: &Theme) -> E {
    el.min_h(input::min_height(theme))
        .px(input::padding_x(theme))
        .py(input::padding_y(theme))
        .flex()
        .items_center()
        .gap(input::gap(theme))
        .rounded(input::RADIUS.px(theme))
        .border_1()
        .border_color(theme.border)
        .text_size(TextSize::Default.px(theme))
}

/// A field's label, Zed's `InputField` label: Small text.
pub(crate) fn input_label(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    div().text_size(input::LABEL.px(theme)).child(label.into())
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
        .border_color(theme.border)
        .bg(theme.surface_elevated)
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

/// The "star the project" call to action: a bordered card with the star glyph
/// over the repo name and a quiet reason, opening the repository in the OS
/// browser. Shared by the sidebar (which passes its close button) and
/// Settings → About (which mounts it permanently, `dismiss = None`).
///
/// `element_id` is the card's element/debug id; each mount passes its own so
/// the two instances never collide.
pub(crate) fn star_project_banner(
    theme: Theme,
    element_id: &'static str,
    dismiss: Option<AnyElement>,
) -> AnyElement {
    div()
        .id(element_id)
        .debug_selector(move || element_id.to_string())
        .px(DynamicSpacing::Base08.px(&theme))
        .py(DynamicSpacing::Base06.px(&theme))
        .rounded(panel::RADIUS.px(&theme))
        .border_1()
        .border_color(theme.border)
        .bg(theme.surface_elevated)
        .flex()
        .items_center()
        .gap(DynamicSpacing::Base08.px(&theme))
        .cursor_pointer()
        .hover_bordered(&theme)
        .pressed_state(&theme, PressedRamp::Wash)
        .on_mouse_up(MouseButton::Left, |_, _, cx| {
            cx.open_url(env!("CARGO_PKG_REPOSITORY"));
        })
        .child(icon(
            "icons/star.svg",
            ButtonSize::Medium.icon_size().px(&theme),
            theme.accent_text,
        ))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(TextSize::Small.px(&theme))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text_primary)
                        .child(tr!("sidebar.star_orbit")),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(TextSize::XSmall.px(&theme))
                        .text_color(theme.text_muted)
                        .child(tr!("sidebar.star_orbit_detail")),
                ),
        )
        .children(dismiss)
        .into_any_element()
}

/// Dress a top-bar chip: the [`header_fill`] glass in a hairline box, lifting
/// on hover. The caller owns the size and the rounding — a control may opt into
/// `rounded_full` when it is genuinely circular, while the rest of the row takes
/// [`header_ctrl_radius`] — and a radius set here would override whichever it
/// picked. The helper must not set a second hover of its own.
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
        theme.surface_hover
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
/// is installed) — shared by the @-mention rows, the Explorer tree, and tool
/// result file rows. Attachment tiles go through [`file_icon_tile`], which
/// additionally centres the glyph's ink.
///
/// `size` is the glyph's size, an [`IconSize`] resolved to px; the glyph
/// sits in that icon's square (the size plus Base02 either side).
pub(crate) fn file_glyph(
    path: &str,
    dark: bool,
    nerd_family: Option<&SharedString>,
    size: impl Into<Pixels>,
    fallback: AnyElement,
) -> AnyElement {
    let Some(family) = nerd_family else {
        return fallback;
    };
    let Some((glyph, color)) = dev_file_icon(path, dark) else {
        return fallback;
    };
    let size: Pixels = size.into();
    div()
        .w(size + px(4.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .font_family(family)
        .text_size(size)
        .text_color(color)
        .child(glyph.to_string())
        .into_any_element()
}

/// The vertical nudge, in canvas pixels, that centres a glyph's **ink** in its
/// line box instead of centring the font's ascent/descent box.
///
/// GPUI puts a line's baseline at `(line_height - ascent - descent) / 2 +
/// ascent`, so ordinary text lands centred: its ink runs from the baseline up
/// to cap height, which is what the ascent/descent box is shaped around. Icon
/// glyphs instead draw their art across most of the em box above the baseline,
/// so the same baseline leaves an icon sitting high beside a label. The needed
/// shift depends only on the glyph's ink and the font's vertical metrics — the
/// line height cancels out:
///
/// `shift = ink.bottom + ink.height / 2 - (ascent - descent) / 2`
///
/// Positive values move the glyph down. Zero when the glyph cannot be measured,
/// and clamped to a fraction of the type size so a platform whose metrics are
/// shaped differently can never fling the icon out of its tile.
pub(crate) fn glyph_ink_shift(cx: &App, font: &Font, size: Pixels, glyph: char) -> Pixels {
    let text_system = cx.text_system();
    let font_id = text_system.resolve_font(font);
    let Ok(ink) = text_system.typographic_bounds(font_id, size, glyph) else {
        return px(0.);
    };
    let ascent = text_system.ascent(font_id, size);
    let descent = text_system.descent(font_id, size);
    let shift = ink.origin.y + ink.size.height / 2. - (ascent - descent) / 2.;
    shift.clamp(-size * 0.35, size * 0.35)
}

/// The leading visual of a file attachment: the devicons glyph for the file
/// type — the real markdown/rust/PDF mark, not a monogram — centred by its ink
/// in the same square slot an image thumbnail occupies, with no tile painted
/// behind it.
///
/// Falls back to [`file_type_tile`] (the type's label on a tinted square) when
/// no Nerd Font is resolvable or the type has no glyph, so an attachment always
/// leads with something aligned and legible.
pub(crate) fn file_icon_tile(cx: &App, path: &str, tile: Pixels, theme: Theme) -> AnyElement {
    let Some(family) = nerd_font_family(cx) else {
        return file_type_tile(path, tile, theme);
    };
    let Some((glyph, color)) = dev_file_icon(path, theme.mode == ThemeMode::Dark) else {
        return file_type_tile(path, tile, theme);
    };
    let font = Font {
        family: family.clone(),
        features: FontFeatures::default(),
        fallbacks: None,
        weight: FontWeight::NORMAL,
        style: FontStyle::Normal,
    };
    let size = tile * 0.8;
    let shift = glyph_ink_shift(cx, &font, size, glyph);
    // The mark is the icon: no tinted square is painted behind it. The chip is
    // already the highlighted surface, and a second rounded square behind a
    // glyph whose art is itself a box reads as an avatar, not a file icon. The
    // square only reserves the slot that keeps it aligned with an image
    // thumbnail.
    div()
        .size(tile)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .relative()
                .top(shift)
                .font_family(family)
                .text_size(size)
                .text_color(color)
                .child(glyph.to_string()),
        )
        .into_any_element()
}

/// The label tile: the file type's short label (`MD`, `PDF`, `ZIP`, …) in a
/// square washed with the type's colour. The fallback for [`file_icon_tile`]
/// when the Nerd Font or the type's glyph is unavailable, and always centred by
/// construction because it is our own single-line label.
pub(crate) fn file_type_tile(path: &str, tile: Pixels, theme: Theme) -> AnyElement {
    let (label, dark_hex, light_hex) = mentions::file_type_badge(path);
    let color: Hsla = gpui::rgb(if theme.mode == ThemeMode::Dark {
        dark_hex
    } else {
        light_hex
    })
    .into();
    // Longer labels step down so `SVELTE` fits the same square `MD` does.
    let size = match label.chars().count() {
        0..=2 => tile * 0.4,
        3 => tile * 0.34,
        4 => tile * 0.29,
        _ => tile * 0.24,
    };
    div()
        .size(tile)
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(Radius::Small.px(&theme))
        .bg(color.opacity(0.16))
        .text_size(size)
        .line_height(size)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(label.to_string())
        .into_any_element()
}

#[cfg(test)]
mod ink_shift_tests {
    use super::*;

    /// The nudge must never be able to throw a glyph out of its tile, whatever
    /// the platform's font metrics look like. The test platform's text system
    /// reports stub metrics (a negative descent, identical ink for every
    /// glyph), so this pins the clamp rather than the centring itself; the
    /// centring is verified against the bundled face's real metrics in the
    /// `file_icon_tile` docs.
    #[gpui::test]
    fn the_shift_stays_within_the_clamp(cx: &mut gpui::TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, cx| {
            let mut style = window.text_style();
            style.font_size = px(16.).into();
            let font = style.font();
            for glyph in ['M', '|', 'g', ' ', '\u{f48a}'] {
                let shift = glyph_ink_shift(cx, &font, px(16.), glyph);
                assert!(
                    shift.abs() <= px(16. * 0.35),
                    "{glyph:?} must stay inside its tile, got {shift:?}"
                );
            }
        });
    }
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
        .rounded(Radius::Small.px(&theme))
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

/// Ink for a provider's brand mark: the provider's identity color when it has
/// one, otherwise the surface's existing neutral (`fallback`). Resolved per
/// theme like [`file_badge`], so a mark stays legible on both canvases.
pub(crate) fn provider_mark_color(provider: &str, theme: &Theme, fallback: Hsla) -> Hsla {
    match crate::providers::provider_brand(provider) {
        Some((dark, light)) => {
            let hex = if theme.mode == ThemeMode::Dark {
                dark
            } else {
                light
            };
            gpui::rgb(hex).into()
        }
        None => fallback,
    }
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

/// An error surface: what failed, and what to do next.
///
/// Every error in the workbench renders through this or [`error_text`]. The
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
    let mut body = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(DynamicSpacing::Base04.px(&theme))
        .child(
            div()
                .text_size(TextSize::Default.px(&theme))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_primary)
                .child(title.to_string()),
        );
    if let Some(detail) = detail.filter(|d| !d.trim().is_empty()) {
        body = body.child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_secondary)
                .child(detail.to_string()),
        );
    }
    if let Some(hint) = hint.filter(|h| !h.trim().is_empty()) {
        body = body.child(
            div()
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(hint.to_string()),
        );
    }

    let mut card = div()
        .w_full()
        .bg(theme.error.opacity(0.07))
        .border_1()
        .border_color(theme.error.opacity(0.35))
        .rounded(panel::RADIUS.px(&theme))
        .px(DynamicSpacing::Base12.px(&theme))
        .py(DynamicSpacing::Base12.px(&theme))
        .flex()
        .items_start()
        .gap(DynamicSpacing::Base08.px(&theme))
        .child(icon(
            "icons/info.svg",
            IconSize::Medium.px(&theme),
            theme.error,
        ))
        .child(body);
    if let Some(action) = action {
        card = card.child(action);
    }
    card.into_any_element()
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

    /// The chevron's turn is a quarter turn and nothing else: the same glyph in
    /// the same place, so the row does not appear to swap icons.
    #[test]
    fn a_disclosure_chevron_turns_exactly_a_quarter_turn() {
        assert_eq!(chevron_angle(0.0), 0.0, "closed points right");
        assert!(
            (chevron_angle(1.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "open points down"
        );
        assert!(
            (chevron_angle(0.5) - std::f32::consts::FRAC_PI_4).abs() < 1e-6,
            "halfway is halfway"
        );
        // Clamped, so a delayed frame cannot over-rotate the glyph.
        assert!((chevron_angle(2.0) - std::f32::consts::FRAC_PI_2).abs() < 1e-6);
        assert_eq!(chevron_angle(-1.0), 0.0);
    }

    /// A picker's caret starts pointing down and turns a half turn to point up;
    /// a quarter turn would leave it pointing right or down, which is the
    /// disclosure language, not the dropdown one.
    #[test]
    fn a_dropdown_caret_turns_exactly_a_half_turn() {
        assert_eq!(dropdown_angle(0.0), 0.0, "closed points down");
        assert!(
            (dropdown_angle(1.0) - std::f32::consts::PI).abs() < 1e-6,
            "open points up"
        );
        assert!(
            (dropdown_angle(0.5) - std::f32::consts::FRAC_PI_2).abs() < 1e-6,
            "halfway is halfway"
        );
        assert!((dropdown_angle(2.0) - std::f32::consts::PI).abs() < 1e-6);
        assert_eq!(dropdown_angle(-1.0), 0.0);
    }

    /// A switch's knob travels the track's inner span, and the span has to stay
    /// positive at every spacing density the user can choose — at Compact the
    /// padding shrinks and the knob has further to go, not less.
    #[test]
    fn a_switch_knob_travels_the_tracks_inner_span() {
        for density in crate::theme::SPACING_DENSITIES {
            let mut theme = Theme::dark();
            theme.ui.spacing_density = density;
            let travel = switch_travel(&theme);
            let padding = DynamicSpacing::Base02.px(&theme);
            assert!(
                travel > px(0.),
                "the knob would fall off the track at {density}% density"
            );
            // Track = padding + knob + travel + padding.
            assert_eq!(
                padding + px(14.) + travel + padding,
                px(36.),
                "the knob must land flush with the far edge at {density}%"
            );
        }
        // The default density is the geometry the design system states.
        assert_eq!(switch_travel(&Theme::dark()), px(18.));
    }

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

    /// A group of segments rounds only its outer corners: the first rounds
    /// its left edge, the last its right, and the middle stays square so
    /// abutting segments read as one control.
    #[test]
    fn segment_positions_trim_the_group_outline() {
        let theme = Theme::dark();
        let radius = button::RADIUS.px(&theme);
        let corners = |position| {
            segmented_segment(div(), &theme, ButtonSize::Medium, position)
                .style()
                .corner_radii
                .clone()
        };
        let explicit = |mut el: gpui::Div| el.style().corner_radii.clone();
        assert_eq!(
            corners(SegmentPosition::Only),
            explicit(div().rounded_none().rounded(radius))
        );
        assert_eq!(
            corners(SegmentPosition::First),
            explicit(div().rounded_none().rounded_l(radius))
        );
        assert_eq!(
            corners(SegmentPosition::Last),
            explicit(div().rounded_none().rounded_r(radius))
        );
        assert_eq!(
            corners(SegmentPosition::Middle),
            explicit(div().rounded_none())
        );
    }

    #[test]
    fn segment_positions_cover_a_group() {
        use SegmentPosition::{First, Last, Middle, Only};
        assert_eq!(SegmentPosition::at(0, 1), Only);
        assert_eq!(SegmentPosition::at(0, 0), Only);
        assert_eq!(SegmentPosition::at(0, 2), First);
        assert_eq!(SegmentPosition::at(1, 2), Last);
        assert_eq!(SegmentPosition::at(0, 3), First);
        assert_eq!(SegmentPosition::at(1, 3), Middle);
        assert_eq!(SegmentPosition::at(2, 3), Last);
    }
}
