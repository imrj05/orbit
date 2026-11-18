use super::helpers::*;
use super::*;
use crate::theme::tokens::{toast as toast_tokens, IconSize, Motion};
use crate::toast::{Toast, ToastKind};

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
        let theme = *theme::get(cx);
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
        let title = tr!("updater_ui.update_available");
        let mut package_rows: Vec<AnyElement> = Vec::new();
        for (index, update) in updates.iter().take(3).enumerate() {
            if index > 0 {
                package_rows.push(
                    div()
                        .w_full()
                        .h(toast_tokens::separator_size())
                        .bg(theme.border)
                        .into_any_element(),
                );
            }
            package_rows.push(
                div()
                    .w_full()
                    .min_w_0()
                    .py(toast_tokens::package_row_padding_y(&theme))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(toast_tokens::package_row_gap(&theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(toast_tokens::BODY.px(&theme))
                            .text_color(theme.text_secondary)
                            .child(update.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(toast_tokens::version_gap(&theme))
                            .child(
                                div()
                                    .font_family(theme::code_font_family())
                                    .text_size(toast_tokens::version_text_size(&theme))
                                    .text_color(theme.text_muted)
                                    .child(short_revision(&update.current).to_string()),
                            )
                            .child(icon(
                                "icons/arrow-right.svg",
                                IconSize::XSmall.px(&theme),
                                theme.text_muted,
                            ))
                            .child(
                                div()
                                    .font_family(theme::code_font_family())
                                    .text_size(toast_tokens::version_text_size(&theme))
                                    .text_color(theme.text_primary)
                                    .child(short_revision(&update.latest).to_string()),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let close_button = crate::ui::button::IconButton::new("plugin-update-close", "icons/x.svg")
            .size(toast_tokens::CLOSE_BUTTON)
            .on_mouse_up(
                &theme,
                cx.listener(|this, _, _, cx| this.skip_plugin_updates(cx)),
            );

        let this = cx.entity();
        let skip_button = press(button_frame(
            div().id(ElementId::Name("plugin-update-skip".into())),
            &theme,
            toast_tokens::ACTION_BUTTON,
        ))
        .cursor_pointer()
        .text_color(theme.text_muted)
        .hover_state(&theme)
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| app.skip_plugin_updates(cx));
        })
        .child(tr!("git_panel.recover_skip"));

        let this = cx.entity();
        let update_button = press(button_frame(
            div().id(ElementId::Name("plugin-update-now".into())),
            &theme,
            toast_tokens::ACTION_BUTTON,
        ))
        .cursor_pointer()
        .raised(theme.action, &theme)
        .text_color(theme.action_fg)
        .hover_action(&theme)
        .pressed_state(&theme, PressedRamp::Action)
        .on_mouse_up(MouseButton::Left, move |_, _, cx| {
            this.update(cx, |app, cx| app.apply_plugin_updates(cx));
        })
        .child(icon(
            "icons/refresh.svg",
            IconSize::XSmall.px(&theme),
            theme.action_fg,
        ))
        .child(tr!("updater_ui.update_now"));

        let card = toast_tokens::surface(
            div()
                .id(ElementId::Name("plugin-update-toast".into()))
                .debug_selector(|| "plugin-update-toast".to_string()),
            &theme,
        )
        .w(toast_tokens::WIDTH)
        .p(toast_tokens::padding(&theme))
        .flex()
        .items_start()
        .gap(toast_tokens::content_gap(&theme))
        .child(
            // Inline with the title row, as on every toast card.
            div()
                .flex_none()
                .h(toast_tokens::CLOSE_BUTTON.height(&theme))
                .flex()
                .items_center()
                .child(icon(
                    "icons/extensions.svg",
                    toast_tokens::ICON.px(&theme),
                    theme.accent_text,
                )),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(toast_tokens::content_gap(&theme))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(toast_tokens::header_gap(&theme))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(toast_tokens::TITLE.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_primary)
                                .child(title),
                        )
                        .child(
                            div()
                                .min_w(toast_tokens::badge_min_width(&theme))
                                .h(toast_tokens::badge_height(&theme))
                                .px(toast_tokens::badge_padding_x(&theme))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(toast_tokens::BADGE_RADIUS.px(&theme))
                                .bg(theme.overlay)
                                .text_size(toast_tokens::BODY.px(&theme))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_secondary)
                                .child(updates.len().to_string()),
                        )
                        .child(close_button),
                )
                .child(div().w_full().flex().flex_col().children(package_rows))
                .child(
                    div()
                        .w_full()
                        .h(toast_tokens::separator_size())
                        .bg(theme.border),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(toast_tokens::action_gap(&theme))
                        .child(skip_button)
                        .child(update_button),
                ),
        );

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
        let card = crate::ui::toast::ToastCard::new(
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
