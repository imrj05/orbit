//! Context-menu styling and highlight navigation.
//!
//! [`MenuState`] is the one implementation of a menu's keyboard cursor:
//! wrap-around arrows, Home/End, disabled-row skipping and reconciliation when
//! the list changes. It is index-based and pure, so any menu — whatever its
//! row type — keeps its items and asks this for the cursor. Callers still own
//! positioning, dismissal and focus restore.

use gpui::{div, ParentElement, SharedString, Styled};

use crate::theme::{
    self,
    tokens::{self, context_menu, list, list_item, StyledExt},
    Theme,
};

/// The shell of a context menu, built to Zed's `ContextMenu`: the elevated
/// surface (`elevation_2`), a 200px minimum width, `List`'s vertical padding,
/// and the Comfortable line height a menu forces so its entries measure the
/// same wherever it is deferred from. Callers keep positioning, occlusion,
/// dismissal, and a fixed width when the menu needs one.
pub(crate) fn context_menu_surface<E: Styled + ParentElement>(el: E, theme: &Theme) -> E {
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

/// The trailing hint on a menu entry — a shortcut, or why the entry is
/// disabled: Small muted text after Zed's `ml_4` keybinding gap, less the
/// entry's own icon gap. It never shrinks, so the label truncates first.
pub(crate) fn menu_hint(text: impl Into<gpui::SharedString>, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .ml(context_menu::keybinding_gap(theme) - context_menu::icon_gap(theme))
        .text_size(tokens::TextSize::Small.px(theme))
        .text_color(theme.text_muted)
        .child(text.into())
}

/// Zed's `ListSeparator`: a full-width hairline with Base06 above and below.
pub(crate) fn context_menu_separator(theme: &Theme) -> gpui::Div {
    div()
        .h(tokens::BORDER_WIDTH)
        .w_full()
        .my(list::separator_margin_y(theme))
        .bg(theme.hairline())
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MenuState {
    highlighted: usize,
}

impl MenuState {
    /// The raw cursor, for painting the highlighted row. Use
    /// [`Self::selected`] to act on it, which bounds it by the current length.
    pub(crate) fn highlighted(&self) -> usize {
        self.highlighted
    }

    pub(crate) fn highlight(&mut self, index: usize) {
        self.highlighted = index;
    }

    pub(crate) fn selected(&self, len: usize) -> Option<usize> {
        (self.highlighted < len).then_some(self.highlighted)
    }

    /// Down, wrapping. Every row can act.
    pub(crate) fn next(&mut self, len: usize) {
        self.next_enabled(len, |_| true);
    }

    /// Up, wrapping. Every row can act.
    pub(crate) fn prev(&mut self, len: usize) {
        self.prev_enabled(len, |_| true);
    }

    /// Home: the first row that can act.
    pub(crate) fn first(&mut self, len: usize, enabled: impl Fn(usize) -> bool) {
        if let Some(index) = (0..len).find(|&index| enabled(index)) {
            self.highlighted = index;
        }
    }

    /// End: the last row that can act.
    pub(crate) fn last(&mut self, len: usize, enabled: impl Fn(usize) -> bool) {
        if let Some(index) = (0..len).rev().find(|&index| enabled(index)) {
            self.highlighted = index;
        }
    }

    /// Down, wrapping, skipping rows that cannot act. With no enabled row the
    /// highlight does not move.
    pub(crate) fn next_enabled(&mut self, len: usize, enabled: impl Fn(usize) -> bool) {
        let start = self.selected(len);
        for step in 1..=len {
            let index = match start {
                Some(from) => (from + step) % len,
                None => step - 1,
            };
            if enabled(index) {
                self.highlighted = index;
                return;
            }
        }
    }

    /// Up, wrapping, skipping rows that cannot act.
    pub(crate) fn prev_enabled(&mut self, len: usize, enabled: impl Fn(usize) -> bool) {
        let start = self.selected(len);
        for step in 1..=len {
            let index = match start {
                Some(from) => (from + len - step) % len,
                None => len - step,
            };
            if enabled(index) {
                self.highlighted = index;
                return;
            }
        }
    }

    /// Reconcile after the list changed under the menu (a filter narrowed it,
    /// an item disappeared): keep the highlight when it still lands on an
    /// enabled row, otherwise move to the nearest enabled row at or before it,
    /// then after it.
    pub(crate) fn reconcile(&mut self, len: usize, enabled: impl Fn(usize) -> bool) {
        if len == 0 {
            self.highlighted = 0;
            return;
        }
        let anchor = self.highlighted.min(len - 1);
        if let Some(index) = (0..=anchor)
            .rev()
            .chain(anchor + 1..len)
            .find(|&index| enabled(index))
        {
            self.highlighted = index;
        } else {
            self.highlighted = anchor;
        }
    }
}

/// A key that moves a menu's cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MenuKey {
    Next,
    Prev,
    First,
    Last,
}

/// The keyboard cursor of a menu that opens from the pointer too: hidden until
/// the first key (a right-clicked menu stays quiet), then a [`MenuState`] that
/// skips rows that cannot act.
///
/// It lives inside the menu's own state, and **equality ignores it**: a
/// cursor is where the keyboard is, not which menu is open, so a toggle that
/// compares menus still recognises the same menu after the cursor moved.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct MenuCursor {
    state: MenuState,
    keyed: bool,
}

impl PartialEq for MenuCursor {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl MenuCursor {
    /// A cursor for a menu opened from the keyboard: shown at once, on the
    /// first row that can act.
    pub(crate) fn keyed(len: usize, enabled: impl Fn(usize) -> bool) -> Self {
        let mut cursor = Self {
            state: MenuState::default(),
            keyed: true,
        };
        cursor.state.first(len, enabled);
        cursor
    }

    /// The highlighted row, once the keyboard has shown the cursor.
    pub(crate) fn highlighted(&self, len: usize) -> Option<usize> {
        if self.keyed {
            self.state.selected(len)
        } else {
            None
        }
    }

    /// Move the cursor. The first key on a pointer-opened menu shows the
    /// cursor on the first row that can act instead of moving past it.
    pub(crate) fn step(&mut self, key: MenuKey, len: usize, enabled: impl Fn(usize) -> bool) {
        if !self.keyed {
            self.keyed = true;
            self.state.first(len, enabled);
            return;
        }
        match key {
            MenuKey::Next => self.state.next_enabled(len, enabled),
            MenuKey::Prev => self.state.prev_enabled(len, enabled),
            MenuKey::First => self.state.first(len, enabled),
            MenuKey::Last => self.state.last(len, enabled),
        }
    }
}

/// Zed's inset `ListSubHeader`, the section label inside a menu or picker:
/// a 20px row of Small, muted text, Base02 + `px_2` in from the edges, with
/// Base04 below.
/// A [`menu_header`] inside a `popover_picker`. That surface has no vertical
/// inset of its own (a search row sits flush at its top), so a header that
/// opens a picker, or follows its search row, carries the list's inset above
/// it — otherwise the title sits on the popover's top edge.
pub(crate) fn picker_header(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    menu_header(label, theme).pt(crate::theme::tokens::picker::list_padding_y(theme))
}

/// A [`menu_header`] that opens a later section, directly under the previous
/// section's rows. Without the break the title reads as one more row of the
/// group above it; a menu's *first* title takes [`menu_header`] (in a padded
/// list) or [`picker_header`] (at a picker's top edge) instead.
pub(crate) fn menu_section_header(label: impl Into<SharedString>, theme: &Theme) -> gpui::Div {
    menu_header(label, theme).pt(crate::theme::tokens::DynamicSpacing::Base08.px(theme))
}

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

#[cfg(test)]
mod tests {
    use super::MenuState;

    #[test]
    fn a_cursor_is_hidden_until_a_key_and_skips_disabled_rows() {
        use super::{MenuCursor, MenuKey};
        // Row 0 is a disabled "Move up" at the top of its list.
        let enabled = |ix: usize| ix != 0;
        let mut cursor = MenuCursor::default();
        assert_eq!(cursor.highlighted(4), None);
        cursor.step(MenuKey::Next, 4, enabled);
        assert_eq!(
            cursor.highlighted(4),
            Some(1),
            "first key shows the first enabled row"
        );
        cursor.step(MenuKey::Prev, 4, enabled);
        assert_eq!(
            cursor.highlighted(4),
            Some(3),
            "up wraps past the disabled row"
        );
        assert_eq!(MenuCursor::keyed(4, enabled).highlighted(4), Some(1));
        assert!(
            cursor == MenuCursor::default(),
            "equality ignores the cursor"
        );
    }

    #[test]
    fn a_menu_hint_never_shrinks() {
        use gpui::Styled;
        let theme = crate::theme::Theme::dark();
        let mut hint = super::menu_hint("⌘K", &theme);
        assert_eq!(hint.style().flex_shrink, Some(0.));
    }

    #[test]
    fn empty_menu_has_no_selection_and_navigation_does_not_panic() {
        let mut menu = MenuState::default();
        menu.next(0);
        menu.prev(0);
        assert_eq!(menu.selected(0), None);
    }

    #[test]
    fn navigation_wraps_in_both_directions() {
        let mut menu = MenuState::default();
        menu.prev(3);
        assert_eq!(menu.selected(3), Some(2));
        menu.next(3);
        assert_eq!(menu.selected(3), Some(0));
        menu.next(3);
        assert_eq!(menu.selected(3), Some(1));
    }

    #[test]
    fn single_item_stays_selected() {
        let mut menu = MenuState::default();
        menu.next(1);
        menu.prev(1);
        assert_eq!(menu.selected(1), Some(0));
    }

    fn all(_: usize) -> bool {
        true
    }

    #[test]
    fn home_and_end_land_on_enabled_rows() {
        let mut menu = MenuState::default();
        let enabled = |index: usize| index != 0 && index != 4;
        menu.last(5, enabled);
        assert_eq!(menu.selected(5), Some(3));
        menu.first(5, enabled);
        assert_eq!(menu.selected(5), Some(1));
        menu.last(3, all);
        assert_eq!(menu.selected(3), Some(2));
    }

    #[test]
    fn arrows_skip_disabled_rows_and_wrap() {
        let mut menu = MenuState::default();
        let enabled = |index: usize| index.is_multiple_of(2); // 0, 2, 4
        menu.next_enabled(5, enabled);
        assert_eq!(menu.selected(5), Some(2));
        menu.next_enabled(5, enabled);
        assert_eq!(menu.selected(5), Some(4));
        menu.next_enabled(5, enabled);
        assert_eq!(menu.selected(5), Some(0));
        menu.prev_enabled(5, enabled);
        assert_eq!(menu.selected(5), Some(4));
    }

    #[test]
    fn all_disabled_and_empty_menus_are_safe() {
        let mut menu = MenuState::default();
        menu.highlight(1);
        menu.next_enabled(3, |_| false);
        menu.prev_enabled(3, |_| false);
        menu.first(3, |_| false);
        menu.last(3, |_| false);
        assert_eq!(menu.selected(3), Some(1));
        menu.next_enabled(0, all);
        menu.prev_enabled(0, all);
        menu.reconcile(0, all);
        assert_eq!(menu.selected(0), None);
    }

    #[test]
    fn reconcile_keeps_or_moves_to_the_nearest_enabled_row() {
        let mut menu = MenuState::default();
        menu.highlight(6);
        // The list shrank to 4: clamp to the last row.
        menu.reconcile(4, all);
        assert_eq!(menu.selected(4), Some(3));
        // That row became disabled: walk back first.
        menu.reconcile(4, |index| index != 3);
        assert_eq!(menu.selected(4), Some(2));
        // Nothing at or before it is enabled: walk forward.
        menu.highlight(1);
        menu.reconcile(4, |index| index >= 3);
        assert_eq!(menu.selected(4), Some(3));
    }

    #[test]
    fn hover_selection_and_shrinking_list_are_handled() {
        let mut menu = MenuState::default();
        menu.highlight(2);
        assert_eq!(menu.selected(3), Some(2));
        assert_eq!(menu.selected(2), None);
        menu.prev(2);
        assert_eq!(menu.selected(2), Some(1));
        menu.highlight(9);
        menu.next(2);
        assert_eq!(menu.selected(2), Some(0));
    }
}
