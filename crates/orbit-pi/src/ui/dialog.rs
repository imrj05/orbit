//! Modals: the shared card and scrim, and the keyboard contract every modal
//! owes the person using it — focus moves in when it opens, Tab cycles inside
//! it, and focus goes back to the control that opened it when it closes,
//! whichever way it closes.

use gpui::{
    div, App, FocusHandle, FontWeight, InteractiveElement, MouseButton, ParentElement, Pixels,
    SharedString, Styled, Window,
};

use crate::theme::{
    self,
    tokens::{modal, DynamicSpacing, StyledExt, TextSize},
    Theme,
};

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

/// A modal card's width: `width`, narrowed to fit a smaller window.
pub(crate) trait ModalWidthExt: Styled + Sized {
    /// Use this rather than `w_full()` + `max_w(width)`. With that pair the
    /// card's height is measured at the unclamped full width, where wrapping
    /// text takes fewer lines, so the card comes out shorter than its content
    /// and clips the footer or scrolls a body that would fit. A definite
    /// width keeps the height in step with the content; the card then grows
    /// with it up to its `max_h`, and only a body taller than the window
    /// scrolls.
    fn modal_width(self, width: Pixels) -> Self {
        self.w(width).max_w_full()
    }
}

impl<T: Styled> ModalWidthExt for T {}

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

/// The line under a modal's title.
pub(crate) enum ModalSubtitle {
    /// A sentence: what the dialog does.
    Text(SharedString),
    /// An identifier the dialog acts on (a provider id), in the code face.
    Id(SharedString),
}

/// A modal's header: the headline over an optional subtitle, with a hairline
/// below. Callers localize the text. The shorthand for the common
/// [`ModalHeader`].
pub(crate) fn modal_header(
    theme: &Theme,
    title: impl Into<SharedString>,
    subtitle: Option<ModalSubtitle>,
) -> gpui::Div {
    let header = ModalHeader::new(title);
    match subtitle {
        Some(subtitle) => header.subtitle(subtitle),
        None => header,
    }
    .render(theme)
}

/// Every modal's header, Zed's `ModalHeader`: an optional eyebrow (where the
/// dialog comes from), the headline, an optional subtitle and description,
/// with an optional leading mark and trailing control (a close button) beside
/// the text, and a hairline below unless the body continues the header.
pub(crate) struct ModalHeader {
    title: SharedString,
    eyebrow: Option<SharedString>,
    subtitle: Option<ModalSubtitle>,
    description: Option<SharedString>,
    leading: Option<gpui::AnyElement>,
    trailing: Option<gpui::AnyElement>,
    divider: bool,
}

impl ModalHeader {
    pub(crate) fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            eyebrow: None,
            subtitle: None,
            description: None,
            leading: None,
            trailing: None,
            divider: true,
        }
    }

    /// A quiet line above the title: what is asking ("Question").
    pub(crate) fn eyebrow(mut self, eyebrow: impl Into<SharedString>) -> Self {
        self.eyebrow = Some(eyebrow.into());
        self
    }

    pub(crate) fn subtitle(mut self, subtitle: ModalSubtitle) -> Self {
        self.subtitle = Some(subtitle);
        self
    }

    /// A paragraph under the title in secondary ink (a confirm's message).
    /// Blank text is skipped.
    pub(crate) fn description(mut self, description: impl Into<SharedString>) -> Self {
        let description = description.into();
        if !description.trim().is_empty() {
            self.description = Some(description);
        }
        self
    }

    /// A mark before the text (an extension's icon tile).
    pub(crate) fn leading(mut self, leading: impl gpui::IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// A control after the text (a close button).
    pub(crate) fn trailing(mut self, trailing: impl gpui::IntoElement) -> Self {
        self.trailing = Some(trailing.into_any_element());
        self
    }

    /// The hairline below; on by default. Off when the body reads as the
    /// header's continuation (a question and its options).
    pub(crate) fn divider(mut self, divider: bool) -> Self {
        self.divider = divider;
        self
    }

    pub(crate) fn render(self, theme: &Theme) -> gpui::Div {
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(modal::header_gap(theme))
            .children(self.eyebrow.map(|eyebrow| {
                div()
                    .text_size(TextSize::Small.px(theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_muted)
                    .child(eyebrow)
            }))
            .child(
                div()
                    .whitespace_normal()
                    .text_size(modal::HEADLINE.px(theme))
                    .line_height(modal::HEADLINE.line_height(theme))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_primary)
                    .child(self.title),
            )
            .children(self.subtitle.map(|subtitle| {
                match subtitle {
                    ModalSubtitle::Text(text) => div()
                        .text_size(TextSize::Small.px(theme))
                        .text_color(theme.text_muted)
                        .child(text),
                    ModalSubtitle::Id(id) => div()
                        .font_family(theme::code_font_family())
                        .text_size(theme.code_px(11.))
                        .text_color(theme.text_muted)
                        .child(id),
                }
            }))
            .children(self.description.map(|description| {
                div()
                    .whitespace_normal()
                    .text_size(TextSize::Default.px(theme))
                    .text_color(theme.text_secondary)
                    .child(description)
            }));
        let frame = div()
            .px(modal::header_padding_x(theme))
            .pt(modal::header_padding_top(theme))
            .pb(modal::header_padding_bottom(theme))
            .flex_none()
            .flex()
            .items_center()
            .gap(DynamicSpacing::Base12.px(theme))
            .children(self.leading)
            .child(text)
            .children(self.trailing);
        if self.divider {
            frame.border_b_1().border_color(theme.hairline())
        } else {
            frame
        }
    }
}

/// A modal's footer: actions at the trailing edge (Cancel, then the primary),
/// with a hairline above. A leading control goes first with a `flex_1`
/// spacer after it.
pub(crate) fn modal_footer(theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .px(modal::footer_padding(theme))
        .py(modal::footer_padding(theme))
        .flex()
        .items_center()
        .justify_end()
        .gap(modal::footer_gap(theme))
        .border_t_1()
        .border_color(theme.hairline())
}

/// A modal footer's button as a keyboard stop: a [`Button`] tracking `focus`,
/// so the modal's Tab ring can reach it and Enter / Space press it. Finish it
/// with `.size(…)` if needed and `.on_click(…)`.
pub(crate) fn modal_button(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    variant: crate::ui::button::ButtonVariant,
    focus: &FocusHandle,
) -> crate::ui::button::Button {
    crate::ui::button::Button::new(id, variant)
        .label(label)
        .focus(focus)
}

/// The control to hand focus back to when a modal surface closes
/// (`DESIGN.md`: restore focus to the control that opened it).
///
/// Weak, because the opener can be torn down while the surface is up. Where
/// focus finally goes is the owner's call: it may also turn away an opener
/// that is still mounted but out of sight.
#[derive(Default)]
pub(crate) struct ReturnFocus(Option<gpui::WeakFocusHandle>);

impl ReturnFocus {
    /// Remember what holds focus now. A surface that already holds focus
    /// (re-focused after a session switch) keeps its original opener.
    pub(crate) fn capture(&mut self, surface: &FocusHandle, window: &Window, cx: &App) {
        if surface.contains_focused(window, cx) {
            return;
        }
        self.0 = window.focused(cx).map(|handle| handle.downgrade());
    }

    /// The remembered opener, if it is still alive. Clears the slot.
    pub(crate) fn take(&mut self) -> Option<FocusHandle> {
        self.0.take().and_then(|handle| handle.upgrade())
    }
}

/// One modal's focus lifecycle.
///
/// [`open`](Self::open) where the modal is created (it remembers the opener,
/// then moves the keyboard in); [`closed`](Self::closed) once per frame with
/// whether the modal still shows. Restoring on the first frame *after* the
/// modal is gone is what makes every close path — a button, the scrim,
/// Escape, a finished operation — hand focus back without each one
/// remembering to.
#[derive(Default)]
pub(crate) struct ModalFocus {
    opener: ReturnFocus,
    open: bool,
}

impl ModalFocus {
    /// The modal is opening and `target` takes the keyboard. Re-opening an
    /// open modal keeps the original opener.
    pub(crate) fn open(&mut self, target: &FocusHandle, window: &mut Window, cx: &App) {
        if !self.open {
            self.opener.capture(target, window, cx);
            self.open = true;
        }
        window.focus(target);
    }

    /// Call every frame. Returns `Some(opener)` exactly once, on the first
    /// frame the modal no longer shows; the opener is `None` when it is gone,
    /// and the owner picks a fallback.
    pub(crate) fn closed(&mut self, showing: bool) -> Option<Option<FocusHandle>> {
        self.closed_with(showing, |opener| opener.take())
    }

    fn closed_with<T>(
        &mut self,
        showing: bool,
        take: impl FnOnce(&mut ReturnFocus) -> T,
    ) -> Option<T> {
        if self.open && !showing {
            self.open = false;
            Some(take(&mut self.opener))
        } else {
            None
        }
    }
}

/// Where Tab (or Shift-Tab) goes in a ring of `len` stops when stop
/// `current` holds focus: the next one, wrapping, so focus never leaves the
/// modal. With nothing in the ring focused, Tab enters at the first stop and
/// Shift-Tab at the last.
pub(crate) fn ring_step(current: Option<usize>, len: usize, backwards: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (current, backwards) {
        (Some(ix), false) => (ix + 1) % len,
        (Some(ix), true) => (ix + len - 1) % len,
        (None, false) => 0,
        (None, true) => len - 1,
    })
}

/// Move focus one stop around `stops`, the modal's focusable controls in
/// order. The modal binds Tab / Shift-Tab in its key context to this, which is
/// what traps focus inside it: GPUI's own `focus_next` walks the whole window.
pub(crate) fn focus_ring(stops: &[FocusHandle], backwards: bool, window: &mut Window) {
    let current = stops.iter().position(|stop| stop.is_focused(window));
    if let Some(next) = ring_step(current, stops.len(), backwards) {
        window.focus(&stops[next]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_wraps_forward_and_back() {
        assert_eq!(ring_step(Some(0), 3, false), Some(1));
        assert_eq!(ring_step(Some(2), 3, false), Some(0));
        assert_eq!(ring_step(Some(0), 3, true), Some(2));
        assert_eq!(ring_step(Some(1), 3, true), Some(0));
    }

    #[test]
    fn tab_enters_the_ring_from_outside() {
        assert_eq!(ring_step(None, 4, false), Some(0));
        assert_eq!(ring_step(None, 4, true), Some(3));
    }

    #[test]
    fn an_empty_ring_goes_nowhere() {
        assert_eq!(ring_step(None, 0, false), None);
        assert_eq!(ring_step(Some(0), 0, true), None);
    }

    #[test]
    fn a_single_stop_keeps_focus() {
        assert_eq!(ring_step(Some(0), 1, false), Some(0));
        assert_eq!(ring_step(Some(0), 1, true), Some(0));
    }

    #[test]
    fn closing_reports_once_and_only_after_opening() {
        let mut focus = ModalFocus::default();
        // Never opened: nothing to restore.
        assert_eq!(focus.closed_with(false, |_| ()), None);
        focus.open = true;
        // Still showing: nothing yet.
        assert_eq!(focus.closed_with(true, |_| ()), None);
        // Gone: report once.
        assert_eq!(focus.closed_with(false, |_| ()), Some(()));
        assert_eq!(focus.closed_with(false, |_| ()), None);
    }
}

#[cfg(test)]
mod header_tests {
    use super::*;
    use crate::theme::ThemeId;
    use gpui::Styled;

    #[test]
    fn a_header_draws_its_hairline_unless_the_body_continues_it() {
        let theme = Theme::for_id(ThemeId::ALL[0]);
        let mut ruled = ModalHeader::new("Add server").render(&theme);
        let mut open = ModalHeader::new("Question").divider(false).render(&theme);
        assert_eq!(ruled.style().border_color, Some(theme.border));
        assert!(open.style().border_color.is_none());
    }
}
