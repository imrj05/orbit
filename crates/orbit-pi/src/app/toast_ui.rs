use super::*;
use crate::theme::tokens::{toast as toast_tokens, IconSize, Motion};
use crate::toast::{Toast, ToastKind};
use crate::ui::button::{Button, ButtonVariant};
use crate::ui::prelude::*;
use crate::ui::toast as toast_card;

fn short_revision(revision: &str) -> &str {
    if revision.len() > 12 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        &revision[..7]
    } else {
        revision
    }
}

impl OrbitApp {
    /// Queue an in-app toast. A repeat of a card still on screen refreshes it
    /// instead of stacking a twin. The caller owns the repaint — push from a
    /// path that ends in `cx.notify()`.
    pub(super) fn push_toast(
        &mut self,
        kind: ToastKind,
        title: impl Into<String>,
        body: Option<String>,
    ) {
        self.toasts.push(kind, title, body);
    }

    /// A one-line confirmation — the common case.
    pub(super) fn toast_success(&mut self, message: impl Into<String>) {
        self.push_toast(ToastKind::Success, message, None);
    }

    pub(super) fn toast_info(&mut self, message: impl Into<String>) {
        self.push_toast(ToastKind::Info, message, None);
    }

    pub(super) fn toast_warning(&mut self, message: impl Into<String>) {
        self.push_toast(ToastKind::Warning, message, None);
    }

    pub(super) fn toast_error(&mut self, message: impl Into<String>) {
        self.push_toast(ToastKind::Error, message, None);
    }

    /// Dismiss one card: the click handler for the whole card (and its ×).
    pub(super) fn dismiss_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.toasts.dismiss(id) {
            cx.notify();
        }
    }

    /// Pointer entered or left the toast deck: fan it out and hold every
    /// clock while it is read, then resume where they stopped.
    fn hover_toast_stack(&mut self, hovered: bool, cx: &mut Context<Self>) {
        let now = Instant::now();
        let changed = if hovered {
            self.toasts.hold(now)
        } else {
            self.toasts.release(now)
        };
        if changed {
            cx.notify();
        }
    }

    /// The toast stack: bottom-right, newest nearest the corner. At rest it is
    /// a collapsed deck — the newest card in front, up to
    /// [`toast_tokens::DECK_DEPTH`] older ones peeking above it, each a step
    /// narrower. Under the pointer it fans out into a column. It sits above
    /// every surface, so a fact that lands while Settings / Git / Usage owns
    /// the main area is still announced.
    pub(super) fn toast_layer(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.toasts.is_empty() && self.plugin_update_prompt.is_none() {
            return None;
        }
        let theme = theme::get(cx).for_floating();
        let reduce_motion = theme.reduce_motion();
        let now = Instant::now();
        let items = self.toasts.items();
        let deck = (!items.is_empty()).then(|| {
            let expanded = self.toasts.is_held() || items.len() == 1;
            let stack = div()
                .id("toast-stack")
                .debug_selector(|| "toast-stack".to_string())
                .relative()
                .w(toast_tokens::WIDTH)
                .flex()
                .flex_col()
                .gap(toast_tokens::stack_gap(&theme))
                .on_hover(
                    cx.listener(|this, hovered: &bool, _, cx| this.hover_toast_stack(*hovered, cx)),
                );
            if expanded {
                // Oldest at the top, newest nearest the corner.
                stack.children(
                    items
                        .iter()
                        .map(|toast| self.toast_card(toast, now, theme, reduce_motion, cx)),
                )
            } else {
                // Collapsed: empty slabs for the older cards, deepest first so
                // the front card paints over them. Each slab is the front
                // card's height (it is inset from the card's own box), raised
                // by the peek and narrowed by the scale step.
                let peek = toast_tokens::deck_peek(&theme);
                let behind = items.len().saturating_sub(1).min(toast_tokens::DECK_DEPTH);
                let slabs = (1..=behind).rev().map(|depth| {
                    let inset =
                        toast_tokens::WIDTH * (toast_tokens::DECK_SCALE_STEP * depth as f32 / 2.0);
                    let lift = peek * depth as f32;
                    toast_tokens::surface(
                        div().debug_selector(move || format!("toast-deck-{depth}")),
                        &theme,
                    )
                    .absolute()
                    .top(-lift)
                    .bottom(lift)
                    .left(inset)
                    .right(inset)
                });
                let front = items
                    .last()
                    .map(|toast| self.toast_card(toast, now, theme, reduce_motion, cx));
                stack.children(slabs).children(front)
            }
        });
        Some(
            div()
                .absolute()
                .bottom(toast_tokens::stack_inset(&theme))
                .right(toast_tokens::stack_inset(&theme))
                .flex()
                .flex_col()
                .items_end()
                .gap(toast_tokens::stack_gap(&theme))
                // The container is not itself a hitbox: only the deck and the
                // cards take pointer events, so the area around them stays
                // click-through.
                .children(deck)
                .children(self.plugin_update_prompt.as_ref().map(|updates| {
                    self.plugin_update_prompt_card(updates, theme, reduce_motion, cx)
                }))
                .into_any_element(),
        )
    }

    /// The update notice stays visible until the user chooses Update now,
    /// Skip, or closes it; ordinary toasts retain their short TTL.
    fn plugin_update_prompt_card(
        &self,
        updates: &[crate::plugins::PluginUpdate],
        theme: Theme,
        reduce_motion: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut rows: Vec<AnyElement> = Vec::new();
        for (index, update) in updates.iter().take(3).enumerate() {
            if index > 0 {
                rows.push(toast_card::separator(&theme).into_any_element());
            }
            rows.push(toast_card::update_row(
                update.name.clone(),
                short_revision(&update.current).to_string(),
                short_revision(&update.latest).to_string(),
                &theme,
            ));
        }

        let this = cx.entity();
        let skip_button = Button::keyed(
            ElementId::Name("plugin-update-skip".into()),
            ButtonVariant::Ghost,
        )
        .size(toast_tokens::ACTION_BUTTON)
        .label(tr!("git_panel.recover_skip"))
        .on_mouse_up(&theme, move |_, _, cx| {
            this.update(cx, |app, cx| app.skip_plugin_updates(cx));
        });

        let this = cx.entity();
        let update_button = Button::keyed(
            ElementId::Name("plugin-update-now".into()),
            ButtonVariant::Primary,
        )
        .size(toast_tokens::ACTION_BUTTON)
        .leading(icon(
            "icons/refresh.svg",
            IconSize::XSmall.px(&theme),
            theme.action_fg,
        ))
        .label(tr!("updater_ui.update_now"))
        .on_mouse_up(&theme, move |_, _, cx| {
            this.update(cx, |app, cx| app.apply_plugin_updates(cx));
        });

        let card = toast_card::ToastCard::new(
            ElementId::Name("plugin-update-toast".into()),
            ToastKind::Info,
            tr!("updater_ui.update_available"),
        )
        .glyph("icons/extensions.svg", theme.accent_text)
        .count(updates.len())
        .content(rows)
        .actions([
            skip_button.into_any_element(),
            update_button.into_any_element(),
        ])
        .on_close(
            &theme,
            "plugin-update-close",
            cx.listener(|this, _, _, cx| this.skip_plugin_updates(cx)),
        )
        .debug_selector(|| "plugin-update-toast".to_string());

        if reduce_motion {
            card.into_any_element()
        } else {
            card.with_animation(
                ElementId::Name("plugin-update-toast-enter".into()),
                Animation::new(toast_tokens::ENTER),
                |card, progress| card.opacity(progress),
            )
            .into_any_element()
        }
    }

    /// One toast card in the shadcn shape: an inline kind glyph, the title
    /// over a muted description, and a quiet × centred on the right. Clicking
    /// anywhere dismisses it before its TTL lapses.
    fn toast_card(
        &self,
        toast: &Toast,
        now: Instant,
        theme: Theme,
        reduce_motion: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let id = toast.id;
        let card = toast_card::ToastCard::new(
            ElementId::Name(format!("toast-{id}-{}", toast.revision).into()),
            toast.kind,
            toast.title.clone(),
        )
        .body(toast.body.clone())
        .on_dismiss(
            &theme,
            ElementId::Name(format!("toast-close-{id}").into()),
            cx.listener(move |this, _, _, cx| this.dismiss_toast(id, cx)),
        )
        .debug_selector(move || format!("toast-{id}"));

        // Reduce Motion keeps the card static (and `tick` expires it at the
        // bare TTL, with no fade grace).
        if reduce_motion {
            return card.into_any_element();
        }
        // The animation only drives repaints across the card's life; the
        // frame itself is read from the model's clocks, so a hold freezes the
        // fade-out and a release resumes it (the revision re-keys the driver).
        // Its duration is the toast's own lifetime, not a motion choice: it is
        // a countdown over data rather than a `Motion` role.
        let ttl = toast.kind.ttl();
        let fade = crate::toast::FADE;
        let enter = toast_tokens::ENTER;
        let elapsed = self.toasts.elapsed(toast, now);
        let shown = toast.shown;
        let lifetime = (ttl + fade).saturating_sub(elapsed).max(enter);
        let held = self.toasts.is_held();
        let rendered_at = now;
        let ease = Motion::Interaction.easing();
        card.with_animation(
            ElementId::Name(format!("toast-life-{id}-{}", toast.revision).into()),
            Animation::new(Duration::from_secs_f32(lifetime.as_secs_f32())),
            move |card, _| {
                let now = Instant::now();
                let arrived = ease(crate::toast::enter_progress(
                    now.saturating_duration_since(shown),
                    enter,
                ));
                let life = if held {
                    elapsed
                } else {
                    elapsed + now.saturating_duration_since(rendered_at)
                };
                let leaving = crate::toast::exit_opacity(life, ttl, fade);
                // A relative offset moves the paint, not the layout, so the
                // stack never reflows while a card rises into place.
                card.opacity(arrived * leaving)
                    .top(toast_tokens::ENTER_RISE * (1.0 - arrived))
            },
        )
        .into_any_element()
    }
}
