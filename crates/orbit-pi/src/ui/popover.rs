//! Popovers: anchoring, entrance motion, outside-click dismissal and the
//! click-through guard every trigger needs.
//!
//! A popover's lifecycle has one hazard GPUI does not solve: the outside
//! mouse-down that dismisses a menu is followed by that same click's mouse-up
//! on the trigger, which would toggle the menu straight back open.
//! [`DismissGuard`] is the one answer to that, and [`PopoverExt::dismiss_on_outside`]
//! is the one place a popover listens for the outside click.

use std::time::{Duration, Instant};

use gpui::{
    anchored, deferred, div, AnchoredPositionMode, AnyElement, App, Corner, InteractiveElement,
    IntoElement, MouseDownEvent, ParentElement, Pixels, Point, SharedString, Styled, Window,
};

use crate::theme::{
    self,
    tokens::{picker, popover, Motion, StyledExt},
    Theme,
};

/// How long after an outside dismissal a trigger ignores its own click: long
/// enough to cover the mouse-up of the dismissing click, short enough that a
/// deliberate second click always works.
pub(crate) const CLICK_THROUGH: Duration = Duration::from_millis(200);

/// Remembers the last outside dismissal so the dismissing click's own mouse-up
/// cannot re-open the menu it just closed. One per owner of popovers; every
/// outside dismissal [`arm`](Self::arm)s it, every trigger asks
/// [`swallows`](Self::swallows) first.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DismissGuard(Option<Instant>);

impl DismissGuard {
    /// A popover was just dismissed from outside.
    pub(crate) fn arm(&mut self) {
        self.0 = Some(Instant::now());
    }

    /// Forget a pending dismissal: the next trigger click is deliberate (a
    /// keyboard route, or a control that is not a popover trigger).
    pub(crate) fn clear(&mut self) {
        self.0 = None;
    }

    /// Whether a trigger's click is the tail of a dismissal and must be
    /// ignored. Consumes the arm either way: only one click is swallowed.
    pub(crate) fn swallows(&mut self) -> bool {
        self.swallows_at(Instant::now())
    }

    /// Whether a dismissal happened just now, without consuming it — for a
    /// surface that must not re-show itself during the dismissing click (a
    /// context menu that would otherwise repaint under the pointer).
    pub(crate) fn is_recent(&self) -> bool {
        self.is_recent_at(Instant::now())
    }

    fn is_recent_at(&self, now: Instant) -> bool {
        self.0
            .is_some_and(|armed| now.saturating_duration_since(armed) < CLICK_THROUGH)
    }

    fn swallows_at(&mut self, now: Instant) -> bool {
        self.0
            .take()
            .is_some_and(|armed| now.saturating_duration_since(armed) < CLICK_THROUGH)
    }
}

/// The floating panel a picker or dropdown sits on: the modal-tier elevated
/// surface (`elevation_3`), the UI face, and the picker's text size. Callers set
/// the width and the column.
pub(crate) fn picker_surface<E: Styled + ParentElement>(el: E, theme: &Theme) -> E {
    el.elevation_3(theme)
        .font_family(theme::ui_font_family())
        .text_size(picker::TEXT.px(theme))
}

/// A popover that is a list of actions or options: the floating surface, the
/// list's vertical inset and a hairline gap between rows, in a column. The
/// caller sets the width and supplies the rows.
pub(crate) fn popover_list<E: Styled + ParentElement>(el: E, theme: &Theme) -> E {
    picker_surface(el, theme)
        .py(picker::list_padding_y(theme))
        .flex()
        .flex_col()
        .gap(crate::theme::tokens::DynamicSpacing::Base01.px(theme))
}

/// A popover that is a picker — a search field over a list: the floating
/// surface in a column, clipped to its rounded corners. The caller sets the
/// width and supplies the search row and list.
pub(crate) fn popover_picker<E: Styled + ParentElement>(el: E, theme: &Theme) -> E {
    picker_surface(el, theme)
        .flex()
        .flex_col()
        .overflow_hidden()
}

/// The popover half of the interaction contract.
pub(crate) trait PopoverExt: InteractiveElement + Sized {
    /// Paint above what is beneath (so hover and clicks stop here) and run
    /// `on_dismiss` on any mouse-down outside. The handler should close the
    /// popover and [`DismissGuard::arm`] its owner's guard.
    fn dismiss_on_outside(
        self,
        on_dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.occlude().on_mouse_down_out(on_dismiss)
    }
}

impl<E: InteractiveElement> PopoverExt for E {}

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

/// Which edge of its trigger a dropdown's menu lines up with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropdownAlign {
    /// The trigger's leading edge.
    Start,
    /// The trigger's trailing edge — a menu opened from a control at the right
    /// of a toolbar, so it grows back into the page.
    End,
}

/// A dropdown's menu, hung `gap` below its trigger and lined up with the
/// trigger's `align` edge: a zero-size anchor at that bottom corner, and the
/// anchored, window-snapped, entering popover. Mount it as a child of the
/// trigger (or of a `relative()` wrapper around it); the trigger keeps its
/// open state, its [`DismissGuard`] and its handler.
pub(crate) fn dropdown_below(
    align: DropdownAlign,
    gap: Pixels,
    menu: impl IntoElement + 'static,
    id: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    let anchor = div().absolute().bottom_0().size(gpui::px(0.));
    let (anchor, corner) = match align {
        DropdownAlign::Start => (anchor.left_0(), Corner::TopLeft),
        DropdownAlign::End => (anchor.right_0(), Corner::TopRight),
    };
    anchor
        .child(anchored_popover(
            corner,
            gpui::point(gpui::px(0.), gap),
            menu,
            id,
            theme,
        ))
        .into_any_element()
}

/// A menu that opens upward out of a column of controls (the composer's
/// access, workflow, add, model and thinking menus, the "Work in" picker and
/// the `@`/`/` autocomplete): the menu is the column's first child, painted
/// [`popover::MENU_OFFSET`] above the trigger below it, snapped to the window.
/// Unlike [`dropdown_above`] it needs no zero-size anchor, because the column
/// already places it.
pub(crate) fn column_menu(
    menu: impl IntoElement + 'static,
    id: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    anchored_popover(
        Corner::BottomLeft,
        gpui::point(gpui::px(0.), -crate::theme::tokens::popover::MENU_OFFSET),
        menu,
        id,
        theme,
    )
    .into_any_element()
}

/// [`dropdown_below`]'s twin for a trigger near the bottom of the window (the
/// composer, a commit bar): the menu sits `gap` above the trigger's top edge.
pub(crate) fn dropdown_above(
    align: DropdownAlign,
    gap: Pixels,
    menu: impl IntoElement + 'static,
    id: impl Into<SharedString>,
    theme: &Theme,
) -> AnyElement {
    let anchor = div().absolute().top_0().size(gpui::px(0.));
    let (anchor, corner) = match align {
        DropdownAlign::Start => (anchor.left_0(), Corner::BottomLeft),
        DropdownAlign::End => (anchor.right_0(), Corner::BottomRight),
    };
    anchor
        .child(anchored_popover(
            corner,
            gpui::point(gpui::px(0.), -gap),
            menu,
            id,
            theme,
        ))
        .into_any_element()
}

/// A row's action menu: at the pointer when a right-click opened it (`at`, in
/// window coordinates), otherwise hung `below` the row's top edge from its
/// `corner` — the keyboard / palette route, where there is no pointer. Snapped
/// inside the window and deferred above the list. No entrance travel: a menu at
/// the pointer has no edge to travel from, and the keyboard twin matches it.
pub(crate) fn row_menu(
    at: Option<Point<Pixels>>,
    corner: Corner,
    below: Pixels,
    popup: impl IntoElement + 'static,
) -> AnyElement {
    match at {
        Some(at) => anchored()
            .anchor(Corner::TopLeft)
            .position(at)
            .snap_to_window_with_margin(popover::WINDOW_MARGIN)
            .child(deferred(popup))
            .into_any_element(),
        None => anchored_menu(corner, gpui::point(gpui::px(0.), below), popup),
    }
}

/// A floating panel anchored by its `corner` at `offset` from where it is
/// mounted, snapped inside the window and deferred above the surface — with
/// no entrance motion (a hover card, a filter panel, a host menu). For a menu
/// that should travel out of its trigger, use [`anchored_popover`].
pub(crate) fn anchored_menu(
    corner: Corner,
    offset: Point<Pixels>,
    popup: impl IntoElement + 'static,
) -> AnyElement {
    anchored()
        .position_mode(AnchoredPositionMode::Local)
        .anchor(corner)
        .offset(offset)
        .snap_to_window_with_margin(popover::WINDOW_MARGIN)
        .child(deferred(popup))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_guard_lets_every_click_through() {
        let mut guard = DismissGuard::default();
        assert!(!guard.swallows());
    }

    #[test]
    fn the_dismissing_click_is_swallowed_once() {
        let mut guard = DismissGuard::default();
        guard.arm();
        let armed = guard.0.expect("armed");
        assert!(guard.swallows_at(armed + Duration::from_millis(50)));
        // Consumed: a second click right after is deliberate.
        assert!(!guard.swallows_at(armed + Duration::from_millis(60)));
    }

    #[test]
    fn a_late_click_is_deliberate() {
        let mut guard = DismissGuard::default();
        guard.arm();
        let armed = guard.0.expect("armed");
        assert!(!guard.swallows_at(armed + CLICK_THROUGH));
        assert!(guard.0.is_none(), "a late check still consumes the arm");
    }

    #[test]
    fn recency_is_read_without_consuming() {
        let mut guard = DismissGuard::default();
        guard.arm();
        let armed = guard.0.expect("armed");
        assert!(guard.is_recent_at(armed + Duration::from_millis(10)));
        assert!(guard.is_recent_at(armed + Duration::from_millis(20)));
        assert!(!guard.is_recent_at(armed + CLICK_THROUGH));
        assert!(guard.swallows_at(armed + Duration::from_millis(30)));
    }

    #[test]
    fn clearing_forgets_the_dismissal() {
        let mut guard = DismissGuard::default();
        guard.arm();
        guard.clear();
        assert!(!guard.swallows());
    }
}
