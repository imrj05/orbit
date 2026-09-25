use super::helpers::*;
use super::*;
use crate::theme::tokens::{ButtonSize, IconSize, StyledExt};
use crate::toast::{Toast, ToastKind};

/// Width of a toast card. A stack hugs the window's bottom-right corner, so
/// a card never fights the centered transcript column for attention.
const TOAST_W: f32 = 320.;

/// How long the card takes to fade in. The paint animation spans the toast's
/// whole life (enter → hold → fade out); [`crate::toast::FADE`] is its tail.
const TOAST_ENTER: Duration = Duration::from_millis(140);

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

    /// The toast stack: bottom-right, newest nearest the corner, oldest at
    /// the top. It sits above every surface, so a fact that lands while
    /// Settings / Git / Usage owns the main area is still announced.
    pub(super) fn toast_layer(&self, cx: &Context<Self>) -> Option<AnyElement> {
        if self.toasts.is_empty() {
            return None;
        }
        let theme = *theme::get(cx);
        let reduce_motion = theme.ui.reduce_motion;
        Some(
            div()
                .debug_selector(|| "toast-stack".to_string())
                .absolute()
                .bottom(px(16.))
                .right(px(16.))
                .flex()
                .flex_col()
                .items_end()
                .gap(px(8.))
                // The container is not itself a hitbox: only the cards below
                // take clicks, so the area around them stays click-through.
                .children(
                    self.toasts
                        .items()
                        .iter()
                        .map(|toast| Self::toast_card(toast, theme, reduce_motion, cx)),
                )
                .into_any_element(),
        )
    }

    /// One toast card: kind glyph, title (+ optional body), and a ×. Clicking
    /// anywhere dismisses it before its TTL lapses.
    fn toast_card(
        toast: &Toast,
        theme: Theme,
        reduce_motion: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let (glyph, tint) = match toast.kind {
            ToastKind::Info => ("icons/info.svg", theme.accent),
            ToastKind::Success => ("icons/check.svg", theme.ok_green),
            ToastKind::Warning => ("icons/info.svg", theme.warn),
            ToastKind::Error => ("icons/stop.svg", theme.crit),
        };
        let id = toast.id;
        let card = div()
            .id(ElementId::Name(
                format!("toast-{id}-{}", toast.revision).into(),
            ))
            .debug_selector(move || format!("toast-{id}"))
            .w(px(TOAST_W))
            .elevation_2(&theme)
            .px(px(12.))
            .py(px(9.))
            .flex()
            .items_start()
            .gap(px(10.))
            .cursor_pointer()
            .hover(|style| style.border_color(theme.border_strong))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| this.dismiss_toast(id, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .mt(px(1.))
                    .size(px(24.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(tint.opacity(0.14))
                    .child(icon(glyph, IconSize::Small.px(&theme), tint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .line_clamp(2)
                            .text_size(theme.ui_px(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(toast.title.clone()),
                    )
                    .children(toast.body.as_ref().map(|body| {
                        div()
                            .whitespace_normal()
                            .line_clamp(3)
                            .text_size(theme.ui_px(11.5))
                            .text_color(theme.text_3)
                            .child(body.clone())
                    })),
            )
            .child(
                icon_button_frame(div(), &theme, ButtonSize::Compact).child(icon(
                    "icons/x.svg",
                    IconSize::XSmall.px(&theme),
                    theme.text_3,
                )),
            );

        // The animation spans the card's whole life: fade in, hold through
        // the TTL, then fade out so the eventual removal is never a snap.
        // Reduce-motion keeps the card static (and `tick` expires it at the
        // bare TTL, with no fade grace).
        if reduce_motion {
            return card.into_any_element();
        }
        let ttl = toast.kind.ttl().as_secs_f32();
        let fade = crate::toast::FADE.as_secs_f32();
        let enter = TOAST_ENTER.as_secs_f32();
        let total = ttl + fade;
        card.with_animation(
            ElementId::Name(format!("toast-life-{id}-{}", toast.revision).into()),
            Animation::new(Duration::from_secs_f32(total)),
            move |card, delta| {
                let elapsed = delta * total;
                let opacity = if elapsed < enter {
                    elapsed / enter
                } else if elapsed > ttl {
                    ((total - elapsed) / fade).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                card.opacity(opacity)
            },
        )
        .into_any_element()
    }
}
