//! Shared button frames and the existing sidebar navigation control treatment.

use gpui::{relative, InteractiveElement, StatefulInteractiveElement, Styled};

use crate::theme::{
    tokens::{button, ButtonSize},
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
