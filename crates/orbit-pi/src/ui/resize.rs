//! Resize handles: the grab strips that resize panes and splits.
//!
//! The handle owns what every grab strip must agree on — the resize cursor for
//! its axis and how it answers the pointer — so a pane edge never looks like a
//! different control on a different surface. Where the strip sits, how wide it
//! is, and the drag it starts (each surface's own payload type) stay with the
//! caller.

use gpui::{
    div, CursorStyle, ElementId, InteractiveElement, ParentElement, SharedString, Stateful, Styled,
};

use crate::theme::{tokens::BORDER_WIDTH, Theme};
use crate::ui::style::control::{AccentWash, ControlExt};

/// Which way a handle resizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizeAxis {
    /// Drag sideways: a vertical edge between columns.
    Horizontal,
    /// Drag up and down: a horizontal edge above a bottom panel.
    Vertical,
}

/// How a handle answers the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizeLook {
    /// The whole strip takes the strong accent wash — a docked card's edge,
    /// where the strip sits over the card's own border.
    Wash,
    /// A 1px seam at the strip's trailing edge, lighting to accent — a pane
    /// edge with no card border of its own (the sidebar).
    Seam,
    /// The strip carries the leading hairline itself, which strengthens — a
    /// divider between two halves of one surface.
    Border,
    /// The strip thickens like a rule — a splitter on a code surface, where
    /// a colour wash would read as a selection.
    Divider,
}

/// A resize handle with the given look. Callers position and size it and add
/// `on_drag` with their surface's payload.
pub(crate) fn resize_handle(
    id: impl Into<SharedString>,
    theme: &Theme,
    axis: ResizeAxis,
    look: ResizeLook,
) -> Stateful<gpui::Div> {
    let id: SharedString = id.into();
    let el = div().id(ElementId::Name(id.clone())).cursor(match axis {
        ResizeAxis::Horizontal => CursorStyle::ResizeLeftRight,
        ResizeAxis::Vertical => CursorStyle::ResizeUpDown,
    });
    match look {
        ResizeLook::Wash => el.hover_accent_wash(theme, AccentWash::Strong),
        ResizeLook::Divider => el.hover_divider(theme),
        ResizeLook::Border => {
            let strong = theme.border_strong;
            el.group(id.clone())
                .border_l_1()
                .border_color(theme.hairline())
                .group_hover(id, move |style| style.border_color(strong))
        }
        ResizeLook::Seam => {
            let lit = theme.accent.opacity(0.4);
            el.group(id.clone()).child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right_0()
                    .w(BORDER_WIDTH)
                    .bg(theme.hairline())
                    .group_hover(id, move |style| style.bg(lit)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_follows_the_axis() {
        let theme = Theme::dark();
        for look in [
            ResizeLook::Wash,
            ResizeLook::Seam,
            ResizeLook::Border,
            ResizeLook::Divider,
        ] {
            let mut sideways = resize_handle("a", &theme, ResizeAxis::Horizontal, look);
            let mut upright = resize_handle("b", &theme, ResizeAxis::Vertical, look);
            assert_eq!(
                sideways.style().mouse_cursor,
                Some(CursorStyle::ResizeLeftRight)
            );
            assert_eq!(
                upright.style().mouse_cursor,
                Some(CursorStyle::ResizeUpDown)
            );
        }
    }
}
