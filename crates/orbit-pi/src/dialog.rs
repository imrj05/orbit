//! Extension UI dialogs — native rendering of pi's `extension_ui_request`
//! sub-protocol (`select` / `confirm` / `input` / `editor`).
//!
//! In RPC mode an extension blocks until the host answers. Tools such as
//! `ask_user_question` walk their questionnaire through these primitives
//! (single-select → `select`, multi-select → an `input` of comma-separated
//! option numbers, custom answers → `input`), so answering them here is what
//! makes a multiple-choice question selectable on the message.
//!
//! The app owns one `Dialog` entity at a time. Its `on_respond` callback
//! carries the answer back to `OrbitApp`, which forwards it over RPC and drops
//! the entity — the same entity-with-callbacks shape as `CommandPalette`.

use gpui::{
    deferred, div, point, prelude::*, px, AnyElement, App, Context, ElementId, Entity, FocusHandle,
    Focusable, IntoElement, MouseButton, MouseDownEvent, Render, ScrollHandle, SharedString,
    Window,
};

use crate::composer::ComposerInput;
use crate::theme::tokens::{list_item, modal, ButtonSize, DynamicSpacing, TextSize};
use crate::theme::{self, Theme};
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::dialog::{focus_ring, ModalHeader};
use crate::ui::prelude::{modal_card, modal_scrim, ModalWidthExt, PressedRamp, StatefulControlExt};

/// Card width — a question plus its options without owning the window.
const CARD_W: f32 = 560.;
/// Option row height (label + inline description).
const OPTION_H: f32 = 44.;
/// Largest option-list height before it scrolls (≈ 7 rows).
const LIST_MAX_H: f32 = 320.;

/// Response callback: the answer plus the ambient window.
type DialogRespond = Box<dyn Fn(DialogResponse, &mut Window, &mut App)>;

/// The answer the host sends back over RPC (`extension_ui_response`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogResponse {
    /// `select` / `input` / `editor` → `{ "value": … }`.
    Value(String),
    /// `confirm` → `{ "confirmed": bool }`.
    Confirmed(bool),
    /// Any dialog → `{ "cancelled": true }` (the extension sees `undefined`).
    Cancelled,
}

/// What the extension is asking for, normalized from the request payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogKind {
    /// Pick one option (the option strings are already display-formatted).
    Select { options: Vec<String> },
    /// Yes / no.
    Confirm,
    /// A single line of free text.
    Input { placeholder: String },
    /// A multi-line editor, optionally prefilled.
    Editor { prefill: String },
}

/// One `extension_ui_request`, normalized for rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogRequest {
    /// RPC request id — echoed in the response.
    pub id: String,
    pub title: String,
    /// Supporting copy (`confirm`'s message), when present.
    pub body: Option<String>,
    pub kind: DialogKind,
}

impl DialogRequest {
    /// Build a request from the raw `extension_ui_request` payload. Returns
    /// `None` for a method Orbit does not render as a dialog.
    pub fn from_event(id: String, method: &str, value: &serde_json::Value) -> Option<Self> {
        let text = |key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let kind = match method {
            "select" => DialogKind::Select {
                options: value
                    .get("options")
                    .and_then(serde_json::Value::as_array)
                    .map(|options| {
                        options
                            .iter()
                            .filter_map(serde_json::Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            "confirm" => DialogKind::Confirm,
            "input" => DialogKind::Input {
                placeholder: text("placeholder"),
            },
            "editor" => DialogKind::Editor {
                prefill: text("prefill"),
            },
            _ => return None,
        };
        Some(Self {
            id,
            title: text("title"),
            body: match method {
                "confirm" => Some(text("message")),
                _ => None,
            },
            kind,
        })
    }
}

/// Marker the access-guard extension puts at the start of a `select` title so
/// Orbit routes it to the inline approval bar instead of the modal dialog.
/// Kept in sync with `GUARD_TITLE_PREFIX` in the extension's `policy.js`.
pub const GUARD_TITLE_PREFIX: &str = "[orbit-guard] ";

/// An inline access-guard approval: the guard extension asks before a mutating
/// tool call runs. Unlike [`DialogRequest`], this renders as a compact bar
/// above the composer — no scrim, no window-sized modal — with one button per
/// option (Allow once / Always allow this tool / Deny).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// RPC request id — echoed in the response.
    pub id: String,
    /// The tool pi wants to run (`bash`, `edit`, …).
    pub tool: String,
    /// The salient argument (command, path, …), already trimmed to one line.
    pub detail: String,
    /// Buttons to offer, in order; the extension maps the chosen string back.
    pub options: Vec<String>,
}

impl ApprovalRequest {
    /// Build an approval from an `extension_ui_request`, or `None` when it is
    /// not a guard request (wrong method, or no marker in the title).
    pub fn from_event(id: String, method: &str, value: &serde_json::Value) -> Option<Self> {
        if method != "select" {
            return None;
        }
        let title = value.get("title").and_then(serde_json::Value::as_str)?;
        let rest = title.strip_prefix(GUARD_TITLE_PREFIX)?;
        // The title is `tool\tdetail`; a missing tab degrades to the whole
        // remainder as the detail so a malformed request still renders.
        let (tool, detail) = rest.split_once('\t').unwrap_or(("", rest));
        let options = value
            .get("options")
            .and_then(serde_json::Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        Some(Self {
            id,
            tool: tool.to_string(),
            detail: detail.to_string(),
            options,
        })
    }
}

/// A blocking extension dialog, rendered as a modal card.
pub struct Dialog {
    request: DialogRequest,
    focus: FocusHandle,
    /// Highlighted option for a select dialog.
    highlighted: crate::ui::menu::MenuState,
    /// No, then Yes. The No button takes focus when a confirmation opens.
    confirm_buttons: Option<[FocusHandle; 2]>,
    /// Text field for `input` / `editor`.
    input: Option<Entity<ComposerInput>>,
    scroll: ScrollHandle,
    /// Guards against answering twice (e.g. a click double-firing).
    responded: bool,
    on_respond: DialogRespond,
}

impl Dialog {
    pub fn new(request: DialogRequest, on_respond: DialogRespond, cx: &mut Context<Self>) -> Self {
        let input = match &request.kind {
            DialogKind::Input { placeholder } => Some(cx.new(|cx| {
                ComposerInput::new(cx)
                    .with_element_id("dialog-input")
                    .with_placeholder(placeholder.clone())
                    .with_max_lines(1)
                    .with_wrap(false)
                    .with_key_context("Composer DialogInput")
            })),
            DialogKind::Editor { prefill } => Some(cx.new(|cx| {
                ComposerInput::new(cx)
                    .with_element_id("dialog-editor")
                    .with_text(prefill.clone())
                    .with_max_lines(12)
                    .with_key_context("Composer DialogInput")
            })),
            _ => None,
        };
        let confirm_buttons = matches!(request.kind, DialogKind::Confirm)
            .then(|| [cx.focus_handle(), cx.focus_handle()]);
        Self {
            request,
            focus: cx.focus_handle(),
            highlighted: crate::ui::menu::MenuState::default(),
            confirm_buttons,
            input,
            scroll: ScrollHandle::new(),
            responded: false,
            on_respond,
        }
    }

    /// The RPC id this dialog is answering.
    pub fn request_id(&self) -> &str {
        &self.request.id
    }

    /// The key context that decides which dialog bindings apply: text fields
    /// keep caret keys, the option list claims ↑/↓.
    fn key_context(&self) -> &'static str {
        match self.request.kind {
            DialogKind::Select { .. } | DialogKind::Confirm => "DialogSelect",
            DialogKind::Input { .. } | DialogKind::Editor { .. } => "DialogInput",
        }
    }

    fn respond(&mut self, response: DialogResponse, window: &mut Window, cx: &mut Context<Self>) {
        if self.responded {
            return;
        }
        self.responded = true;
        (self.on_respond)(response, window, cx);
    }

    fn on_cancel(&mut self, _: &crate::DialogCancel, window: &mut Window, cx: &mut Context<Self>) {
        self.respond(DialogResponse::Cancelled, window, cx);
    }

    fn on_confirm(
        &mut self,
        _: &crate::DialogConfirm,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // In an input/editor dialog, Enter is often the key confirming an IME
        // candidate; the field still holds half-composed text, which must not
        // become the dialog's answer (issue #100).
        if self
            .input
            .as_ref()
            .is_some_and(|input| input.read(cx).is_composing())
        {
            return;
        }
        let response = match &self.request.kind {
            DialogKind::Select { options } => match options.get(self.highlighted.highlighted()) {
                Some(option) => DialogResponse::Value(option.clone()),
                // Nothing to pick (an empty list) — treat as a dismissal.
                None => DialogResponse::Cancelled,
            },
            DialogKind::Confirm => DialogResponse::Confirmed(
                self.confirm_buttons
                    .as_ref()
                    .is_some_and(|[_, yes]| yes.is_focused(window)),
            ),
            DialogKind::Input { .. } | DialogKind::Editor { .. } => {
                let text = self
                    .input
                    .as_ref()
                    .map(|input| input.read(cx).text())
                    .unwrap_or_default();
                DialogResponse::Value(text)
            }
        };
        self.respond(response, window, cx);
    }

    fn on_next(&mut self, _: &crate::DialogNext, _: &mut Window, cx: &mut Context<Self>) {
        if let DialogKind::Select { options } = &self.request.kind {
            if !options.is_empty() {
                self.highlighted.next(options.len());
                self.reveal_highlighted();
                cx.notify();
            }
        }
    }

    fn on_prev(&mut self, _: &crate::DialogPrev, _: &mut Window, cx: &mut Context<Self>) {
        if let DialogKind::Select { options } = &self.request.kind {
            if !options.is_empty() {
                self.highlighted.prev(options.len());
                self.reveal_highlighted();
                cx.notify();
            }
        }
    }

    fn on_first(&mut self, _: &crate::DialogFirst, _: &mut Window, cx: &mut Context<Self>) {
        if let DialogKind::Select { options } = &self.request.kind {
            self.highlighted.first(options.len(), |_| true);
            self.reveal_highlighted();
            cx.notify();
        }
    }

    fn on_last(&mut self, _: &crate::DialogLast, _: &mut Window, cx: &mut Context<Self>) {
        if let DialogKind::Select { options } = &self.request.kind {
            self.highlighted.last(options.len(), |_| true);
            self.reveal_highlighted();
            cx.notify();
        }
    }

    fn on_scrim_down(&mut self, _: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.respond(DialogResponse::Cancelled, window, cx);
    }

    /// Scroll so the keyboard highlight stays visible (options are few, but a
    /// select may carry more than fit).
    fn reveal_highlighted(&self) {
        let top = self.highlighted.highlighted() as f32 * OPTION_H;
        let bottom = top + OPTION_H;
        let scroll_top = f32::from(self.scroll.offset().y);
        let max_h = LIST_MAX_H;
        if top < scroll_top {
            self.scroll.set_offset(point(px(0.), px(top)));
        } else if bottom > scroll_top + max_h {
            self.scroll.set_offset(point(px(0.), px(bottom - max_h)));
        }
    }
}

impl Focusable for Dialog {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input
            .as_ref()
            .map(|input| input.read(cx).focus_handle(cx))
            .or_else(|| self.confirm_buttons.as_ref().map(|[no, _]| no.clone()))
            .unwrap_or_else(|| self.focus.clone())
    }
}

impl Render for Dialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::get(cx).for_floating();
        let this = cx.entity();

        let mut card = modal_card(&theme)
            .id("dialog-card")
            .debug_selector(|| "dialog-card".to_string())
            .modal_width(px(CARD_W))
            .key_context(self.key_context())
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_cancel))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_next))
            .on_action(cx.listener(Self::on_prev))
            .on_action(cx.listener(Self::on_first))
            .on_action(cx.listener(Self::on_last));

        // ── header: origin, title, and (confirm) the message body ──
        // The body continues the question, so no hairline between them.
        let mut header = ModalHeader::new(SharedString::from(self.request.title.clone()))
            .eyebrow(tr!("dialog.question"))
            .divider(false);
        if let Some(body) = self.request.body.clone() {
            header = header.description(body);
        }
        let header = header.render(&theme);
        card = card.child(header);

        // ── body: options, buttons, or a text field ──
        match &self.request.kind {
            DialogKind::Select { options } => {
                card = card.child(self.render_options(options, &this, theme));
            }
            DialogKind::Confirm => {
                if let Some(stops) = &self.confirm_buttons {
                    let forward = stops.clone();
                    let backward = stops.clone();
                    card = card
                        .on_action(move |_: &crate::FocusNext, window, _| {
                            focus_ring(&forward, false, window);
                        })
                        .on_action(move |_: &crate::FocusPrev, window, _| {
                            focus_ring(&backward, true, window);
                        })
                        .child(self.render_confirm(stops, theme, cx));
                }
            }
            DialogKind::Input { .. } | DialogKind::Editor { .. } => {
                if let Some(input) = &self.input {
                    // A modal section holding a Zed `InputField`-sized box.
                    card = card.child(
                        div()
                            .px(modal::section_padding_x(&theme, false))
                            .pb(modal::section_padding_bottom(&theme))
                            .flex_none()
                            // The shared field, on `background` like every
                            // other field inside a dialog card.
                            .child(
                                crate::ui::field::TextField::new(input.clone(), theme.background)
                                    .render(&theme),
                            ),
                    );
                }
            }
        }

        // ── footer: name the keys once ──
        let hint = match self.request.kind {
            DialogKind::Select { .. } => "↑↓ Navigate · ⏎ Select · esc Cancel",
            DialogKind::Confirm => "⏎ Confirm · esc Cancel",
            DialogKind::Input { .. } | DialogKind::Editor { .. } => "⏎ Submit · esc Cancel",
        };
        // Zed's `ModalFooter` pads Base08 all round because its slots hold
        // buttons, whose own padding brings their labels in line with the
        // header; a bare hint takes the header's inset directly.
        card = card.child(
            div()
                .py(modal::footer_padding(&theme))
                .px(modal::header_padding_x(&theme))
                .flex_none()
                .flex()
                .items_center()
                .gap(modal::footer_gap(&theme))
                .border_t_1()
                .border_color(theme.hairline())
                .text_size(TextSize::Small.px(&theme))
                .text_color(theme.text_muted)
                .child(hint),
        );

        // ── scrim: dimmed backdrop; a click outside cancels the dialog ──
        modal_scrim(&theme)
            .id("dialog-layer")
            .debug_selector(|| "dialog-layer".to_string())
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_scrim_down))
            // The layer's dialog entrance: fade on the Layout role. gpui has no
            // box transform, so the card cannot scale — see DESIGN.md.
            .child(crate::motion::enter(
                card,
                "dialog-card-in",
                &theme,
                crate::theme::tokens::Motion::Layout,
                None,
            ))
    }
}

impl Dialog {
    /// The select option list: a scrollable column of full-width rows; the
    /// keyboard highlight doubles as hover.
    fn render_options(
        &self,
        options: &[String],
        this: &Entity<Dialog>,
        theme: Theme,
    ) -> AnyElement {
        if options.is_empty() {
            return div()
                .px(modal::section_padding_x(&theme, false))
                .pb(modal::section_padding_bottom(&theme))
                .text_size(TextSize::Default.px(&theme))
                .text_color(theme.text_muted)
                .child(tr!("dialog.no_options_were_provided"))
                .into_any_element();
        }
        let list_h = (options.len() as f32 * OPTION_H).min(LIST_MAX_H);
        let mut list = div()
            .id("dialog-options")
            .w_full()
            .h(px(list_h))
            .flex_none()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            // Rows are inset like Zed's list items, and pad their labels
            // back out to the header's inset.
            .px(list_item::inset(&theme))
            .pb(modal::section_padding_bottom(&theme))
            .flex()
            .flex_col();
        for (ix, option) in options.iter().enumerate() {
            let highlighted = ix == self.highlighted.highlighted();
            let this = this.clone();
            list = list.child(
                div()
                    .id(ElementId::NamedInteger("dialog-option".into(), ix as u64))
                    .w_full()
                    .min_h(px(OPTION_H))
                    .px(modal::header_padding_x(&theme) - list_item::inset(&theme))
                    .py(px(9.))
                    .rounded(list_item::RADIUS.px(&theme))
                    .border_1()
                    .border_color(if highlighted {
                        theme.border_strong
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .pressed_state(&theme, PressedRamp::Wash)
                    // Pointer movement moves the highlight; a scroll sliding a
                    // row under a stationary pointer must not hijack ↑/↓ nav.
                    .on_mouse_move({
                        let this = this.clone();
                        move |_, _, cx| {
                            this.update(cx, |dialog, cx| {
                                if dialog.highlighted.highlighted() != ix {
                                    dialog.highlighted.highlight(ix);
                                    cx.notify();
                                }
                            });
                        }
                    })
                    .on_click({
                        let this = this.clone();
                        move |_, window, cx| {
                            this.update(cx, |dialog, cx| {
                                dialog.highlighted.highlight(ix);
                                dialog.on_confirm(&crate::DialogConfirm, window, cx);
                            });
                        }
                    })
                    .when(highlighted, |row| row.bg(theme.selected_wash()))
                    .child(
                        div()
                            .whitespace_normal()
                            .text_size(TextSize::Default.px(&theme))
                            .text_color(if highlighted {
                                theme.text_primary
                            } else {
                                theme.text_secondary
                            })
                            .child(SharedString::from(option.clone())),
                    ),
            );
        }
        list.into_any_element()
    }

    fn render_confirm(
        &self,
        stops: &[FocusHandle; 2],
        theme: Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px(modal::section_padding_x(&theme, false))
            .pb(modal::section_padding_bottom(&theme))
            .flex_none()
            .flex()
            .gap(DynamicSpacing::Base08.px(&theme))
            .children([false, true].into_iter().zip(stops).map(|(value, focus)| {
                let id = if value { "dialog-yes" } else { "dialog-no" };
                Button::new(id, ButtonVariant::Secondary)
                    .size(ButtonSize::Large)
                    .focus(focus)
                    .on_click(
                        &theme,
                        cx.listener(move |dialog, event: &gpui::ClickEvent, window, cx| {
                            // DialogConfirm owns Enter. GPUI's synthetic click on
                            // key release need not have started in this dialog.
                            if matches!(event, gpui::ClickEvent::Mouse(_)) {
                                dialog.respond(DialogResponse::Confirmed(value), window, cx);
                            }
                        }),
                    )
                    .debug_selector(move || id.to_string())
                    .flex_1()
                    .child(if value { "Yes" } else { "No" })
            }))
            .into_any_element()
    }
}

/// The dialog paints above the command palette and every popover.
pub fn layer(dialog: Entity<Dialog>) -> impl IntoElement {
    deferred(dialog).with_priority(9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn select_parses_title_and_options() {
        let request = DialogRequest::from_event(
            "id-1".into(),
            "select",
            &json!({
                "title": "Allow dangerous command?",
                "options": ["1. Allow — run it", "2. Block — stop"],
            }),
        )
        .unwrap();
        assert_eq!(request.title, "Allow dangerous command?");
        assert_eq!(
            request.kind,
            DialogKind::Select {
                options: vec!["1. Allow — run it".into(), "2. Block — stop".into()],
            }
        );
    }

    #[test]
    fn select_tolerates_missing_options() {
        let request =
            DialogRequest::from_event("id".into(), "select", &json!({"title": "Pick"})).unwrap();
        assert_eq!(request.kind, DialogKind::Select { options: vec![] });
    }

    #[test]
    fn confirm_carries_message_as_body() {
        let request = DialogRequest::from_event(
            "id-2".into(),
            "confirm",
            &json!({"title": "Clear session?", "message": "All messages will be lost."}),
        )
        .unwrap();
        assert_eq!(request.body.as_deref(), Some("All messages will be lost."));
        assert_eq!(request.kind, DialogKind::Confirm);
    }

    #[gpui::test]
    fn confirm_defaults_to_no_even_when_yes_is_hovered(cx: &mut gpui::TestAppContext) {
        for hover_yes in [false, true] {
            let (cx, answer) = open_confirmation(cx);
            if hover_yes {
                let yes = cx.debug_bounds("dialog-yes").expect("Yes button").center();
                cx.simulate_mouse_move(yes, None, gpui::Modifiers::none());
            }
            cx.simulate_keystrokes("enter");
            assert_eq!(
                answer.take(),
                Some(DialogResponse::Confirmed(false)),
                "hover_yes={hover_yes}",
            );
        }
    }

    #[gpui::test]
    fn confirmation_can_be_chosen_by_keyboard_or_click(cx: &mut gpui::TestAppContext) {
        for (keys, expected) in [
            ("tab enter", DialogResponse::Confirmed(true)),
            ("tab shift-tab enter", DialogResponse::Confirmed(false)),
            ("escape", DialogResponse::Cancelled),
        ] {
            let (cx, answer) = open_confirmation(cx);
            cx.simulate_keystrokes(keys);
            assert_eq!(answer.take(), Some(expected), "{keys}");
        }

        let (cx, answer) = open_confirmation(cx);
        cx.simulate_keystrokes("tab");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").expect("a valid key"),
        });
        assert_eq!(answer.take(), None, "a stray key release does not answer");

        let (cx, answer) = open_confirmation(cx);
        let yes = cx.debug_bounds("dialog-yes").expect("Yes button").center();
        cx.simulate_click(yes, gpui::Modifiers::none());
        assert_eq!(answer.take(), Some(DialogResponse::Confirmed(true)));
    }

    fn open_confirmation(
        cx: &mut gpui::TestAppContext,
    ) -> (
        &mut gpui::VisualTestContext,
        std::rc::Rc<std::cell::Cell<Option<DialogResponse>>>,
    ) {
        cx.update(|cx| {
            cx.set_global(Theme::for_id(theme::ThemeId::Orbit));
            crate::bind_keys(cx);
        });
        let answer = std::rc::Rc::new(std::cell::Cell::new(None));
        let response = answer.clone();
        let (dialog, cx) = cx.add_window_view(|_, cx| {
            Dialog::new(
                DialogRequest {
                    id: "confirm".into(),
                    title: "Clear session?".into(),
                    body: None,
                    kind: DialogKind::Confirm,
                },
                Box::new(move |answer, _, _| response.set(Some(answer))),
                cx,
            )
        });
        cx.update(|window, cx| window.focus(&dialog.read(cx).focus_handle(cx)));
        cx.run_until_parked();
        (cx, answer)
    }

    #[test]
    fn input_and_editor_parse_their_fields() {
        let input = DialogRequest::from_event(
            "id-3".into(),
            "input",
            &json!({"title": "Enter a value", "placeholder": "type something…"}),
        )
        .unwrap();
        assert_eq!(
            input.kind,
            DialogKind::Input {
                placeholder: "type something…".into(),
            }
        );
        let editor = DialogRequest::from_event(
            "id-4".into(),
            "editor",
            &json!({"title": "Edit", "prefill": "Line 1\nLine 2"}),
        )
        .unwrap();
        assert_eq!(
            editor.kind,
            DialogKind::Editor {
                prefill: "Line 1\nLine 2".into(),
            }
        );
    }

    #[test]
    fn fire_and_forget_methods_are_not_dialogs() {
        for method in [
            "notify",
            "setStatus",
            "setWidget",
            "setTitle",
            "set_editor_text",
        ] {
            assert!(
                DialogRequest::from_event("id".into(), method, &json!({})).is_none(),
                "{method} must not open a modal"
            );
        }
    }

    #[test]
    fn guard_select_parses_tool_detail_and_options() {
        let request = ApprovalRequest::from_event(
            "id-1".into(),
            "select",
            &json!({
                "title": "[orbit-guard] bash\trm -rf build",
                "options": ["Allow once", "Always allow this tool", "Deny"],
            }),
        )
        .unwrap();
        assert_eq!(request.tool, "bash");
        assert_eq!(request.detail, "rm -rf build");
        assert_eq!(request.options.len(), 3);
    }

    #[test]
    fn a_guard_title_without_a_tab_still_renders() {
        let request = ApprovalRequest::from_event(
            "id".into(),
            "select",
            &json!({"title": "[orbit-guard] edit", "options": ["Deny"]}),
        )
        .unwrap();
        assert_eq!(request.tool, "");
        assert_eq!(request.detail, "edit");
    }

    #[test]
    fn plain_select_and_confirm_are_not_approvals() {
        let plain = ApprovalRequest::from_event(
            "id".into(),
            "select",
            &json!({"title": "Pick", "options": []}),
        );
        assert!(plain.is_none(), "an unmarked select is a normal dialog");
        let confirm =
            ApprovalRequest::from_event("id".into(), "confirm", &json!({"title": "Sure?"}));
        assert!(confirm.is_none(), "confirm is never an approval");
    }

    /// The select dialog lays out a bounded card with its option rows — the
    /// regression guard for a modal that paints nothing (a 0-size flex bug).
    #[gpui::test]
    fn select_dialog_lays_out_a_card(cx: &mut gpui::TestAppContext) {
        use crate::theme::{Theme, ThemeId};
        use gpui::size;

        cx.update(|cx| cx.set_global(Theme::for_id(ThemeId::Orbit)));
        let cx = cx.add_empty_window();
        let request = DialogRequest {
            id: "d1".into(),
            title: "Which approach?".into(),
            body: None,
            kind: DialogKind::Select {
                options: vec!["1. Fast — simpler".into(), "2. Safe — slower".into()],
            },
        };
        let _ = cx.draw(point(px(0.), px(0.)), size(px(800.), px(600.)), |_, cx| {
            let on_respond = Box::new(|_: DialogResponse, _: &mut Window, _: &mut App| {});
            cx.new(|cx| Dialog::new(request, on_respond, cx))
        });
        let card = cx
            .debug_bounds("dialog-card")
            .expect("dialog card laid out");
        assert!(card.size.width > px(0.), "card collapsed to zero width");
        assert!(card.size.width <= px(CARD_W));
        assert!(card.size.height > px(0.), "card collapsed to zero height");
    }

    /// Issue #100: Enter confirming an IME candidate must not answer the
    /// dialog with the preedit still in the field — the same rule the chat
    /// composer, the queue editor and the pickers follow.
    #[gpui::test]
    fn enter_during_an_ime_composition_does_not_answer_the_dialog(cx: &mut gpui::TestAppContext) {
        use gpui::EntityInputHandler as _;

        let cx = cx.add_empty_window();
        let answered = std::rc::Rc::new(std::cell::Cell::new(false));
        let request = DialogRequest {
            id: "ime".into(),
            title: "Name the branch".into(),
            body: None,
            kind: DialogKind::Input {
                placeholder: "type".into(),
            },
        };
        let dialog = {
            let answered = answered.clone();
            cx.update(|_, cx| {
                cx.new(|cx| {
                    Dialog::new(
                        request,
                        Box::new(move |_: DialogResponse, _: &mut Window, _: &mut App| {
                            answered.set(true);
                        }),
                        cx,
                    )
                })
            })
        };

        cx.update(|window, cx| {
            let input = dialog
                .read(cx)
                .input
                .clone()
                .expect("an input dialog has a field");
            // Sogou with the pinyin buffer emptied: session open, preedit empty.
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(None, "nihao", None, window, cx);
                input.replace_and_mark_text_in_range(None, "", None, window, cx);
            });

            dialog.update(cx, |dialog, cx| {
                dialog.on_confirm(&crate::DialogConfirm, window, cx)
            });
            assert!(!answered.get(), "the IME owns that Enter");
            assert!(!dialog.read(cx).responded, "the dialog is still open");

            // The commit ends the session, and Enter answers as usual.
            input.update(cx, |input, cx| {
                input.replace_text_in_range(None, "你好", window, cx)
            });
            dialog.update(cx, |dialog, cx| {
                dialog.on_confirm(&crate::DialogConfirm, window, cx)
            });
            assert!(answered.get(), "a committed value answers the dialog");
        });
    }
}
