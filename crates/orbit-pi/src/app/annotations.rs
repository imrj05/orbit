//! Transcript quotes held in the composer (issue #187): the opener the
//! transcript's text menu calls, the quote cards above the input with their
//! one-line comment fields, and the controller that adds, edits and removes
//! them. The prompt shape a quote travels in is `crate::annotation`.

use super::*;
use crate::annotation::{Quote, QuoteSource, MAX_ANNOTATIONS};
use crate::theme::tokens::{ButtonSize, DynamicSpacing, IconSize, Radius, TextSize};
use crate::ui::button::{IconButton, IconButtonVariant};
use crate::ui::prelude::*;

/// One quoted passage waiting in the composer.
#[derive(Clone)]
pub(super) struct Annotation {
    pub(super) quote: Quote,
    pub(super) source: QuoteSource,
    /// The optional one-line comment. Its text is sent as it stands, so a
    /// comment typed and never confirmed still goes out with the quote.
    pub(super) comment: Entity<ComposerInput>,
    /// The comment as last confirmed with Enter: what Escape restores.
    pub(super) saved_comment: String,
}

impl OrbitApp {
    /// The transcript's Quote in Composer action.
    pub(super) fn quote_opener(&self, cx: &Context<Self>) -> crate::transcript_view::QuoteOpener {
        let this = cx.weak_entity();
        Rc::new(move |quote, source, window, cx| {
            this.update(cx, |app, cx| {
                app.add_annotation(quote, source, Some(window), cx);
            })
            .ok();
        })
    }

    /// Shift+F10: the transcript selection's menu, from the keyboard. With no
    /// selection on screen the key passes on (the sidebar's row menu, say).
    pub(super) fn on_open_selection_menu(
        &mut self,
        _: &crate::OpenSelectionMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.transcript.open_selection_menu(window, cx) {
            cx.propagate();
        }
    }

    /// Append a quote and hand its comment field the keyboard. Past the cap
    /// the quote is refused with a toast, never dropped silently. Returns
    /// whether it was added.
    pub(super) fn add_annotation(
        &mut self,
        quote: Quote,
        source: QuoteSource,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.annotations.len() >= MAX_ANNOTATIONS {
            self.push_toast(
                crate::toast::ToastKind::Warning,
                tr!("composer.annotation_limit", count = MAX_ANNOTATIONS),
                None,
            );
            return false;
        }
        let ix = self.annotations.len();
        let comment = cx.new(|cx| {
            ComposerInput::new(cx)
                .with_element_id(SharedString::from(format!("annotation-comment-{ix}")))
                .with_placeholder_key("composer.annotation_comment")
                .with_key_context("Composer AnnotationComment")
                .with_max_lines(1)
        });
        if let Some(window) = window {
            comment.read(cx).focus(window);
        }
        self.annotations.push(Annotation {
            quote,
            source,
            comment,
            saved_comment: String::new(),
        });
        cx.notify();
        true
    }

    /// Drop one quote and give the keyboard back to the composer.
    pub(super) fn remove_annotation(
        &mut self,
        ix: usize,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if ix >= self.annotations.len() {
            return;
        }
        self.annotations.remove(ix);
        if let Some(window) = window {
            self.input.read(cx).focus(window);
        }
        cx.notify();
    }

    /// Each quote with its comment as it stands, oldest first: what a send
    /// folds into the prompt.
    pub(super) fn annotation_texts(annotations: &[Annotation], cx: &App) -> Vec<(String, String)> {
        annotations
            .iter()
            .map(|annotation| {
                (
                    annotation.quote.text.clone(),
                    annotation.comment.read(cx).text().trim().to_string(),
                )
            })
            .collect()
    }

    /// Lead `text` with the composer's quotes, in the shape
    /// `crate::annotation::split` reads back.
    pub(super) fn fold_annotations(text: &str, annotations: &[Annotation], cx: &App) -> String {
        let texts = Self::annotation_texts(annotations, cx);
        let pairs = texts
            .iter()
            .map(|(quote, comment)| (quote.as_str(), comment.as_str()))
            .collect::<Vec<_>>();
        crate::annotation::fold(text, &pairs)
    }

    /// Enter in a comment field: keep the comment and return to the composer.
    fn confirm_annotation_comment(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(annotation) = self.annotations.get_mut(ix) else {
            return;
        };
        // Enter can be the key that confirms an IME candidate; the preedit is
        // still in the buffer (issue #100).
        if annotation.comment.read(cx).is_composing() {
            return;
        }
        annotation.saved_comment = annotation.comment.read(cx).text().trim().to_string();
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// Escape in a comment field: undo the edit, keep the quote, and return
    /// to the composer.
    fn cancel_annotation_comment(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(annotation) = self.annotations.get(ix) else {
            return;
        };
        let saved = annotation.saved_comment.clone();
        annotation
            .comment
            .update(cx, |input, cx| input.set_text(saved, cx));
        self.input.read(cx).focus(window);
        cx.notify();
    }

    /// The quote cards above the composer input, oldest first. `None` when
    /// nothing is quoted.
    pub(super) fn annotations_row(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.annotations.is_empty() {
            return None;
        }
        let theme = *theme::get(cx);
        let cards = self
            .annotations
            .iter()
            .enumerate()
            .map(|(ix, annotation)| self.annotation_card(ix, annotation, theme, cx));
        Some(
            div()
                .id("annotations")
                .debug_selector(|| "annotations-row".to_string())
                .w_full()
                .flex()
                .flex_col()
                .gap(DynamicSpacing::Base06.px(&theme))
                .children(cards)
                .into_any_element(),
        )
    }

    /// One quote: the passage on one line (all of it, and where it came from,
    /// on hover), a remove control, and the comment field beneath.
    fn annotation_card(
        &self,
        ix: usize,
        annotation: &Annotation,
        theme: Theme,
        cx: &Context<Self>,
    ) -> AnyElement {
        let source = match annotation.source {
            QuoteSource::User => tr!("composer.annotation_from_you"),
            QuoteSource::Assistant => tr!("composer.annotation_from_assistant"),
        };
        let first_line = annotation
            .quote
            .text
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or_default()
            .to_string();
        let tip = format!("{source}\n\n{}", annotation.quote.text);
        let icon_w = IconSize::Small.px(&theme);
        let gap = DynamicSpacing::Base06.px(&theme);
        let remove_id = format!("annotation-remove-{ix}");
        let quote_line = div()
            .flex()
            .items_center()
            .gap(gap)
            .child(icon("icons/compose.svg", icon_w, theme.text_muted))
            .child(
                div()
                    .id(SharedString::from(format!("annotation-quote-{ix}")))
                    .debug_selector(move || format!("annotation-quote-{ix}"))
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(theme.text_secondary)
                    .child(first_line)
                    .tip(tip),
            )
            .children(annotation.quote.truncated.then(|| {
                div()
                    .flex_none()
                    .text_size(TextSize::XSmall.px(&theme))
                    .text_color(theme.text_muted)
                    .child(tr!("composer.annotation_truncated"))
            }))
            .child(
                IconButton::glyph(
                    ElementId::Name(remove_id.clone().into()),
                    icon("icons/x.svg", IconSize::XSmall.px(&theme), theme.text_muted),
                )
                .size(ButtonSize::Compact)
                .variant(IconButtonVariant::Glass)
                .tip(tr!("composer.annotation_remove"))
                .on_mouse_up(
                    &theme,
                    cx.listener(move |this, _: &MouseUpEvent, window, cx| {
                        cx.stop_propagation();
                        this.remove_annotation(ix, Some(window), cx);
                    }),
                )
                .debug_selector(move || remove_id.clone()),
            );
        let comment = annotation.comment.clone();
        div()
            .id(ElementId::NamedInteger("annotation".into(), ix as u64))
            .debug_selector(move || format!("annotation-{ix}"))
            // The comment field's own key context: Enter keeps the comment
            // instead of submitting the composer, Escape undoes the edit
            // instead of aborting the run.
            .key_context("AnnotationComment")
            .on_action(cx.listener(move |this, _: &crate::AnnotationCommentConfirm, window, cx| {
                this.confirm_annotation_comment(ix, window, cx);
            }))
            .on_action(cx.listener(move |this, _: &crate::AnnotationCommentCancel, window, cx| {
                this.cancel_annotation_comment(ix, window, cx);
            }))
            // Backspace in an empty comment removes the quote: the keyboard
            // path to the remove control. A binding, not a key listener: GPUI
            // runs bindings first, so the field's own Backspace would win.
            .on_action(cx.listener(move |this, _: &crate::AnnotationCommentBackspace, window, cx| {
                let empty = this.annotations.get(ix).is_some_and(|annotation| {
                    let input = annotation.comment.read(cx);
                    input.text().is_empty() && !input.is_composing()
                });
                if empty {
                    this.remove_annotation(ix, Some(window), cx);
                } else {
                    // Text to delete: the field's own Backspace takes the key.
                    cx.propagate();
                }
            }))
            // A click on the card must not reach the composer's own click
            // handler, which would take the keyboard from the comment field.
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(DynamicSpacing::Base04.px(&theme))
            .px(DynamicSpacing::Base08.px(&theme))
            .py(DynamicSpacing::Base06.px(&theme))
            .rounded(Radius::Large.px(&theme))
            .border_1()
            .border_color(theme.hairline())
            .raised(theme.control_face(theme.surface_elevated), &theme)
            .child(quote_line)
            .child(
                div()
                    .min_w_0()
                    // The field lines up with the quote text, past the glyph.
                    .pl(icon_w + gap)
                    .text_size(TextSize::Small.px(&theme))
                    .child(comment),
            )
            .into_any_element()
    }
}
