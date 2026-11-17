//! Shared button frames and control treatments.

use gpui::{relative, FontWeight, InteractiveElement, StatefulInteractiveElement, Styled};

use crate::theme::{
    tokens::{button, ButtonSize, RaisedExt},
    Theme,
};
use crate::ui::style::control::{ControlExt, PressedRamp, StatefulControlExt};

/// A labeled button's frame, Zed's `ButtonLike`: the size's height and
/// horizontal padding, Base04 between icon and label, [`button::radius`],
/// and a one-line label at 1× line height (Zed's `UiLabel`) so it sits
/// inside even a 22px button. The label size follows [`button::label_size`]
/// and an icon inside it takes [`ButtonSize::icon_size`], so every button's
/// glyph matches its frame. Callers keep colors, border, hover, press, and
/// click handling.
pub(crate) fn button_frame<E: Styled + InteractiveElement>(
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

/// Geometry and focus treatment for an icon-only button; callers own its content and actions.
pub(crate) fn icon_button_frame<E: Styled + InteractiveElement>(
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

/// A segment's position in a group sharing one outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SegmentPosition {
    First,
    Middle,
    Last,
    Only,
}

impl SegmentPosition {
    pub(crate) fn at(index: usize, count: usize) -> Self {
        match (index, count) {
            (_, 0 | 1) => Self::Only,
            (0, _) => Self::First,
            (index, count) if index + 1 == count => Self::Last,
            _ => Self::Middle,
        }
    }
}

/// Button geometry with only the group's outer corners rounded and one shared hairline.
pub(crate) fn segmented_segment<E: Styled + InteractiveElement>(
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

/// Selected/rest treatment for medium settings segments. Callers own events and labels.
pub(crate) fn settings_segment<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    position: SegmentPosition,
    active: bool,
) -> E {
    let el = segmented_segment(el, theme, ButtonSize::Medium, position)
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .border_color(theme.border);
    if active {
        el.raised_flat(theme.surface_selected)
            .text_color(theme.text_primary)
    } else {
        el.raised_flat(theme.surface_elevated)
            .text_color(theme.text_secondary)
            .hover_state(theme)
    }
}

/// Quiet icon action; callers supply content, tooltip and event handling.
pub(crate) fn quiet_icon_button<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
) -> E {
    icon_button_frame(el, theme, size)
        .cursor_pointer()
        .hover_state(theme)
        .pressed_state(theme, PressedRamp::Wash)
}

/// Icon navigation control with the existing open and pressed treatments.
/// The caller supplies the localized tooltip, icon and mouse action.
pub(crate) fn navigation_icon_button<E: Styled + StatefulInteractiveElement>(
    el: E,
    theme: &Theme,
    size: ButtonSize,
    open: bool,
) -> E {
    let button = icon_button_frame(el, theme, size);
    let button = if open {
        button.bg(theme.surface_active)
    } else {
        button
    };
    button
        .cursor_pointer()
        .hover_state_open(theme, open)
        .pressed_state(
            theme,
            if open {
                PressedRamp::Selected
            } else {
                PressedRamp::Wash
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{div, prelude::FluentBuilder};

    #[test]
    fn settings_segment_preserves_selected_and_resting_styles() {
        for theme in [
            Theme::dark(),
            Theme::for_id(crate::theme::ThemeId::Operational),
        ] {
            for position in [
                SegmentPosition::Only,
                SegmentPosition::First,
                SegmentPosition::Middle,
                SegmentPosition::Last,
            ] {
                for active in [false, true] {
                    let mut actual = settings_segment(div().id("actual"), &theme, position, active);
                    let mut expected = segmented_segment(
                        div().id("expected"),
                        &theme,
                        ButtonSize::Medium,
                        position,
                    )
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .border_color(theme.border)
                    .when(active, |el| {
                        el.raised_flat(theme.surface_selected)
                            .text_color(theme.text_primary)
                    })
                    .when(!active, |el| {
                        el.raised_flat(theme.surface_elevated)
                            .text_color(theme.text_secondary)
                            .hover_state(&theme)
                    });
                    assert_eq!(
                        actual.style(),
                        expected.style(),
                        "{position:?}, active={active}"
                    );
                }
            }
        }
    }
}
