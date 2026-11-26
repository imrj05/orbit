//! The confirmation prompt: the one shape the app asks "are you sure?" in.
//!
//! A [`ConfirmPrompt`] is a blocking card over the workbench — the question,
//! the consequence, what is about to go, and a Cancel / destructive pair in
//! the footer. It is the only owner of that shape: a surface that needs a
//! confirmation builds one of these instead of swapping its own row into a
//! confirm state or hand-rolling another card.
//!
//! ```ignore
//! let layer = ConfirmPrompt::new("explorer-delete", tr!("explorer.delete_title", name = name))
//!     .body(tr!("explorer.delete_body_file_trash"))
//!     .confirm_label(tr!("explorer.delete"))
//!     .focus(&self.delete_cancel_focus, &self.delete_confirm_focus)
//!     .on_confirm(move |window, cx| { … })
//!     .on_cancel(move |window, cx| { … })
//!     .layer(&theme);
//! ```
//!
//! **Keyboard.** The layer binds `PickerCancel` (Escape) and `PickerConfirm`
//! (Enter) to the same two handlers as the buttons, and the owner opens it
//! with [`ModalFocus`](crate::ui::dialog::ModalFocus) on
//! [`ConfirmPrompt::focus`]'s cancel handle — so Enter on arrival takes the
//! safe way out. Space or Enter on the focused destructive button confirms.
//!
//! **Wording.** `ModalHeader` carries the question and the consequence
//! together (its `description` slot is built for this), the optional
//! [`target`](Self::target) block names the object in its own row (a folder, a
//! branch, a path), and the two labels say what happens rather than "OK".
//! Callers localize every string.

use std::rc::Rc;

use gpui::{
    div, prelude::FluentBuilder, px, AnyElement, App, Div, FocusHandle, InteractiveElement,
    IntoElement, MouseButton, ParentElement, Pixels, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Window,
};

use crate::theme::tokens::{ButtonSize, DynamicSpacing, Motion, TextSize};
use crate::theme::Theme;
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::{modal_card, modal_footer, modal_scrim, ModalHeader, ModalWidthExt};

/// One prompt's answer: what pressing a footer button (or Escape / Enter on
/// it) does. `Window` is there for the actions that move focus themselves.
type ConfirmAnswer = Rc<dyn Fn(&mut Window, &mut App)>;

/// A confirmation the user has to answer before an irreversible action runs.
pub(crate) struct ConfirmPrompt {
    id: SharedString,
    title: SharedString,
    body: Option<SharedString>,
    target: Option<AnyElement>,
    leading: Option<AnyElement>,
    target_scroll: Option<ScrollHandle>,
    confirm_label: SharedString,
    cancel_label: SharedString,
    cancel_focus: Option<FocusHandle>,
    confirm_focus: Option<FocusHandle>,
    width: Pixels,
    on_confirm: Option<ConfirmAnswer>,
    on_cancel: Option<ConfirmAnswer>,
}

impl ConfirmPrompt {
    /// `id` names the prompt's elements (`<id>-card`, `<id>-layer`,
    /// `<id>-cancel`, `<id>-confirm`); the title is the question itself
    /// ("Delete “orbit”?") with the affected object already in it.
    pub(crate) fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            body: None,
            target: None,
            leading: None,
            target_scroll: None,
            confirm_label: tr!("common.remove").into(),
            cancel_label: tr!("view.cancel").into(),
            cancel_focus: None,
            confirm_focus: None,
            width: px(420.),
            on_confirm: None,
            on_cancel: None,
        }
    }

    /// What the action does, and whether it can be undone. Blank text is
    /// skipped by the header.
    pub(crate) fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// The object the action takes, on its own: a name over a path, a branch
    /// label, the file's folder. Sits between the question and the footer.
    pub(crate) fn target(mut self, target: impl IntoElement) -> Self {
        self.target = Some(target.into_any_element());
        self
    }

    /// An optional media mark centered above the question.
    pub(crate) fn leading(mut self, leading: impl IntoElement) -> Self {
        self.leading = Some(leading.into_any_element());
        self
    }

    /// Let a long target fill the available height between header and footer.
    /// Page Up / Page Down and the wheel move the same viewport.
    pub(crate) fn scroll_target(mut self, scroll: &ScrollHandle) -> Self {
        self.target_scroll = Some(scroll.clone());
        self
    }

    /// The destructive button's label — the verb, not "OK".
    pub(crate) fn confirm_label(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm_label = label.into();
        self
    }

    pub(crate) fn cancel_label(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel_label = label.into();
        self
    }

    /// The two footer stops. Focus the cancel handle when the prompt opens:
    /// it is the safe arrival point, and Tab walks to the destructive one.
    pub(crate) fn focus(mut self, cancel: &FocusHandle, confirm: &FocusHandle) -> Self {
        self.cancel_focus = Some(cancel.clone());
        self.confirm_focus = Some(confirm.clone());
        self
    }

    /// The card's width. A confirm naming a long path wants more than one
    /// naming a branch.
    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Runs when the destructive button is pressed, or on Enter with it
    /// focused.
    pub(crate) fn on_confirm(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_confirm = Some(Rc::new(handler));
        self
    }

    /// Runs on Cancel, Escape, or a click on the scrim — every way out that is
    /// not the destructive button.
    pub(crate) fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    /// The scrim and the card: what a surface adds to its top layer.
    pub(crate) fn layer(self, theme: &Theme) -> gpui::Stateful<Div> {
        let theme = theme.for_floating();
        let Self {
            id,
            title,
            body,
            target,
            leading,
            target_scroll,
            confirm_label,
            cancel_label,
            cancel_focus,
            confirm_focus,
            width,
            on_confirm,
            on_cancel,
        } = self;

        let mut header = ModalHeader::new(title)
            .centered()
            .divider(false)
            .heading_selector(format!("{id}-heading"));
        if let Some(body) = body {
            header = header.description(body);
        }
        if let Some(leading) = leading {
            header = header.leading(leading);
        }

        let cancel = {
            let mut button = Button::new(format!("{id}-cancel"), ButtonVariant::Outline)
                .label(cancel_label)
                .size(ButtonSize::Large);
            if let Some(focus) = &cancel_focus {
                button = button.focus(focus);
            }
            let handler = on_cancel.clone();
            button
                .on_click(&theme, move |_, window, cx| {
                    if let Some(handler) = &handler {
                        handler(window, cx);
                    }
                })
                .debug_selector(|| format!("{id}-cancel-button"))
                .flex_1()
                .min_w_0()
        };
        let confirm = {
            let mut button = Button::new(format!("{id}-confirm"), ButtonVariant::Destructive)
                .label(confirm_label)
                .size(ButtonSize::Large);
            if let Some(focus) = &confirm_focus {
                button = button.focus(focus);
            }
            let handler = on_confirm.clone();
            button
                .on_click(&theme, move |_, window, cx| {
                    if let Some(handler) = &handler {
                        handler(window, cx);
                    }
                })
                .debug_selector(|| format!("{id}-confirm-button"))
                .flex_1()
                .min_w_0()
        };

        let card = modal_card(&theme)
            .debug_selector({
                let id = id.clone();
                move || format!("{id}-card")
            })
            .modal_width(width)
            .max_h(gpui::relative(1.))
            .child(header.render(&theme).debug_selector({
                let id = id.clone();
                move || format!("{id}-title")
            }))
            .children(target.map(|target| {
                let body = div()
                    .debug_selector({
                        let id = id.clone();
                        move || format!("{id}-body")
                    })
                    .id(SharedString::from(format!("{id}-target-scroll")))
                    .w_full()
                    .min_w_0()
                    .overflow_y_scroll()
                    .px(DynamicSpacing::Base16.px(&theme))
                    .pb(DynamicSpacing::Base16.px(&theme))
                    .text_center()
                    .text_size(TextSize::Small.px(&theme))
                    .text_color(theme.text_secondary)
                    .child(target);
                if let Some(scroll) = &target_scroll {
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .min_h_0()
                        .child(body.min_h_0().track_scroll(scroll))
                        .child(target_scroll_indicator(scroll.clone(), &theme))
                        .into_any_element()
                } else {
                    body.max_h(px(200.)).into_any_element()
                }
            }))
            .child(
                modal_footer(&theme)
                    .p(DynamicSpacing::Base16.px(&theme))
                    .bg(theme.overlay)
                    .debug_selector({
                        let id = id.clone();
                        move || format!("{id}-actions")
                    })
                    .child(cancel)
                    .child(confirm),
            );

        let dismiss = on_cancel.clone();
        let cancelled = move |_: &crate::PickerCancel, window: &mut Window, cx: &mut App| {
            if let Some(handler) = &dismiss {
                handler(window, cx);
            }
        };
        let safe_answer = on_cancel.clone();
        let accepted = move |_: &crate::PickerConfirm, window: &mut Window, cx: &mut App| {
            // GPUI dispatches the Picker Enter binding before the focused
            // button's keyboard click. Only Delete focus may confirm it.
            let answer = if confirm_focus
                .as_ref()
                .is_some_and(|focus| focus.is_focused(window))
            {
                &on_confirm
            } else {
                &safe_answer
            };
            if let Some(handler) = answer {
                handler(window, cx);
            }
        };

        modal_scrim(&theme)
            .id(gpui::ElementId::Name(id.clone()))
            .key_context("Picker")
            .on_action(cancelled)
            .on_action(accepted)
            .when_some(target_scroll, |layer, scroll| {
                let overlap = TextSize::Default.px(&theme);
                layer.on_key_down(move |event, window, cx| {
                    if event.keystroke.modifiers.modified() {
                        return;
                    }
                    let direction = match event.keystroke.key.as_str() {
                        "pageup" => 1.,
                        "pagedown" => -1.,
                        _ => return,
                    };
                    cx.stop_propagation();
                    let height = scroll.bounds().size.height;
                    let page = (height - overlap).max(height / 2.);
                    let offset = scroll.offset();
                    let y =
                        (offset.y + page * direction).clamp(-scroll.max_offset().height, px(0.));
                    if y != offset.y {
                        scroll.set_offset(gpui::point(offset.x, y));
                        window.refresh();
                    }
                })
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if let Some(handler) = &on_cancel {
                    handler(window, cx);
                }
            })
            .child(crate::motion::enter(
                card,
                SharedString::from(format!("{id}-card-in")),
                &theme,
                Motion::Layout,
                None,
            ))
    }
}

/// A passive thumb at the body's edge, like the composer's: show scroll
/// position only during overflow. Draw after the body so its scroll handle
/// already knows the viewport and content size for this frame.
fn target_scroll_indicator(scroll: ScrollHandle, theme: &Theme) -> impl IntoElement {
    let width = DynamicSpacing::Base03.px(theme);
    let inset = DynamicSpacing::Base04.px(theme);
    let min_thumb = DynamicSpacing::Base24.px(theme);
    let ink = theme.text_muted;
    gpui::canvas(
        |_, _, _| (),
        move |bounds, (), window, _| {
            let max = scroll.max_offset().height;
            let track = bounds.size.height - inset * 2.;
            if max <= px(0.) || track <= px(0.) {
                return;
            }
            let thumb = (track * (track / (track + max))).max(min_thumb).min(track);
            let fraction = (-scroll.offset().y / max).clamp(0., 1.);
            window.paint_quad(
                gpui::fill(
                    gpui::Bounds::new(
                        gpui::point(
                            bounds.right() - inset - width,
                            bounds.top() + inset + (track - thumb) * fraction,
                        ),
                        gpui::size(width, thumb),
                    ),
                    ink,
                )
                .corner_radii(width / 2.),
            );
        },
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn enter_uses_the_focused_confirmation_button(cx: &mut gpui::TestAppContext) {
        use std::cell::Cell;

        struct PromptProbe {
            cancel: FocusHandle,
            confirm: FocusHandle,
            answer: Rc<Cell<Option<bool>>>,
        }

        impl gpui::Render for PromptProbe {
            fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
                let theme = crate::theme::get(cx);
                let cancel = self.answer.clone();
                let confirm = self.answer.clone();
                div().relative().size_full().child(
                    ConfirmPrompt::new("confirm-enter-probe", "Delete this?")
                        .focus(&self.cancel, &self.confirm)
                        .on_cancel(move |_, _| cancel.set(Some(false)))
                        .on_confirm(move |_, _| confirm.set(Some(true)))
                        .layer(theme),
                )
            }
        }

        cx.update(|cx| {
            let mut theme = Theme::for_id(crate::theme::ThemeId::Orbit);
            theme.ui.motion = crate::theme::MotionPreference::Reduced;
            cx.set_global(theme);
            crate::bind_keys(cx);
        });
        let answer = Rc::new(Cell::new(None));
        let (probe, cx) = cx.add_window_view(|window, cx| {
            let cancel = cx.focus_handle();
            window.focus(&cancel);
            PromptProbe {
                cancel,
                confirm: cx.focus_handle(),
                answer: answer.clone(),
            }
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        assert_eq!(answer.take(), Some(false), "Enter on Cancel is safe");

        cx.update(|window, cx| window.focus(&probe.read(cx).confirm));
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        assert_eq!(answer.take(), Some(true), "Enter on Delete confirms");
    }

    /// A prompt without handlers still paints: the footer buttons exist and
    /// the labels are the caller's, so a half-wired prompt is a visible
    /// dialog rather than a panic in a render path.
    #[test]
    fn a_prompt_without_handlers_still_names_its_buttons() {
        let theme = Theme::for_id(crate::theme::ThemeId::ALL[0]);
        let prompt = ConfirmPrompt::new("test-confirm", "Delete this?")
            .body("It cannot be undone.")
            .confirm_label("Delete");
        assert_eq!(prompt.confirm_label, "Delete");
        assert_eq!(prompt.cancel_label, tr!("view.cancel"));
        // Building the layer needs a GPUI app for the motion element, so the
        // card's shape is what the unit test pins: the id prefix and width.
        assert_eq!(prompt.id, "test-confirm");
        assert_eq!(prompt.width, px(420.));
        let _ = theme;
    }
}
