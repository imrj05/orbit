use super::*;
use crate::theme::tokens::{
    list, panel, picker, ButtonSize, DynamicSpacing, TextSize,
};
use crate::ui::kbd::kbd_key;
use gpui::{InteractiveElement};

// The interaction-state traits live in `ui::style::control`; re-exported so
// `crate::app::{ControlExt, …}` and `helpers::{…}` keep resolving while
// call sites migrate (docs/plans/ui-component-layer.md, phase 1a).
pub(crate) use crate::ui::button::{
    button_frame, icon_button_frame, segmented_segment, Button, ButtonVariant, SegmentPosition,
};
pub(crate) use crate::ui::dialog::{
    modal_card, modal_footer, modal_header, modal_scrim, ModalSubtitle,
};
pub(crate) use crate::ui::feedback::{
    empty_state, empty_state_glyph, error_state, refresh_glyph, running_spinner, spinner, EmptyFill,
};
pub(crate) use crate::ui::field::{input_field_frame, input_label};
pub(crate) use crate::ui::row::{picker_entry, EntryMarkLike, PickerRow};
pub(crate) use crate::ui::icon::{
    disclosure_chevron, dropdown_chevron, icon, icon_dyn, BUTTON_GROUP,
};
pub(crate) use crate::ui::menu::{
    context_menu_entry, context_menu_separator, context_menu_surface,
};
pub(crate) use crate::ui::popover::{
    anchored_popover, anchored_popover_at, anchored_popover_in_window, picker_surface,
    popover_enter,
};
// Preserve the helper API while callers migrate to the UI layer.
#[allow(unused_imports)]
pub(crate) use crate::ui::popover::popover_enter_from;
pub(crate) use crate::ui::style::control::{
    AccentWash, ControlExt, PressedRamp, StatefulControlExt, TipExt,
};
pub(crate) use crate::ui::switch::switch;

/// Keyboard legend shared by both working-folder pickers. The remote level
/// uses Escape to go back; elsewhere it closes the picker.
pub(crate) fn working_folder_keyboard_hints(theme: &Theme) -> gpui::Div {
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
            "working-folder-hint-close",
            &["escape"],
            tr!("common.close"),
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

/// The one-line preview a queue row shows for a pending message: its text
/// with every run of whitespace — newlines included — folded to one space.
///
/// `line_clamp` in gpui 0.2.2 bounds wrapped lines but still emits each
/// explicit newline, so a queued skill file (hundreds of short lines) would
/// grow the strip over the whole session page (issue #110). Folded to one
/// logical line, the row's two-line clamp holds. The queued message itself is
/// untouched: pi receives the text as written, and the row's editor opens the
/// original.
pub(crate) fn queue_preview(text: &str) -> String {
    let mut preview = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !preview.is_empty() {
            preview.push(' ');
        }
        preview.push_str(word);
    }
    preview
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

pub(crate) use crate::ui::field::picker_search_frame;

/// The row height a two-line [`PickerRow`] fills exactly.
pub(crate) fn picker_row_height(theme: &Theme) -> gpui::Pixels {
    picker::two_line_entry_height(theme)
}

pub(crate) use crate::ui::style::control::press;
pub(crate) use crate::ui::glyph::{
    embedded_image, embedded_image_w, file_badge, file_glyph, file_icon_tile, nerd_font_family,
    provider_mark_color,
};
pub(crate) use crate::ui::chrome::{
    header_chip, header_ctrl_radius, header_ghost_button, header_icon_button, header_lift,
    window_controls, window_drag_region, HEADER_CTRL_H,
};

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
    use crate::theme::tokens::button;

    /// A queued skill file arrives with hundreds of newlines, and gpui's
    /// `line_clamp` still emits every one of them. The row preview folds them
    /// into one logical line so its two-line clamp holds (issue #110).
    #[test]
    fn a_queue_preview_folds_line_breaks_into_spaces() {
        assert_eq!(
            queue_preview("<skill name=\"design\">\n\n## Heading\nline one\n  line two  \n"),
            "<skill name=\"design\"> ## Heading line one line two"
        );
        assert_eq!(queue_preview(""), "");
        assert_eq!(queue_preview("one\ntwo"), "one two");
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
