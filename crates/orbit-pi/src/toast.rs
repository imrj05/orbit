//! The in-app toast stack: transient cards that confirm actions and carry
//! background notifications while the window is frontmost.
//!
//! A toast is born in [`Toasts::push`], lives for its kind's TTL, fades over
//! [`FADE`], and leaves when [`Toasts::expire`] runs or the user dismisses it.
//! The stack is capped at [`MAX_VISIBLE`]: a burst of events can never bury
//! the window, and the oldest card drops instead.
//!
//! The stack paints as a collapsed deck (the newest card in front, older ones
//! peeking above it) and fans out while the pointer is over it. Hovering holds
//! every card's clock ([`Toasts::hold`]) so nothing expires while it is read;
//! leaving resumes them where they stopped ([`Toasts::release`]).
//!
//! This is the model only — `app/toast_ui.rs` paints it. Failures keep their
//! persistent banner; a toast is for facts that survive being missed.

use std::time::{Duration, Instant};

/// Cards visible at once; the oldest is dropped when a new one arrives.
pub const MAX_VISIBLE: usize = 4;

/// How long a card spends fading out. It is removed at TTL + FADE, so the
/// paint animation (sized TTL + FADE) reaches zero exactly as it leaves.
pub const FADE: Duration = Duration::from_millis(180);

/// The four toast roles. The kind picks the glyph, the tint, and the TTL —
/// an error deserves more reading time than a copy confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    /// How long the card holds before it starts fading.
    pub fn ttl(self) -> Duration {
        match self {
            Self::Success => Duration::from_secs(3),
            Self::Info => Duration::from_secs(4),
            Self::Warning => Duration::from_secs(6),
            Self::Error => Duration::from_secs(8),
        }
    }

    /// The card for an extension `notify`'s `notifyType` (pi's
    /// `rpc-extension-ui.md`: `info`, `warning` or `error`, info when omitted).
    /// Anything else stays info — an extension never earns a success card.
    pub fn from_notify_type(notify_type: Option<&str>) -> Self {
        match notify_type {
            Some("warning") => Self::Warning,
            Some("error") => Self::Error,
            _ => Self::Info,
        }
    }
}

/// One queued card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    /// Optional second line (a turn summary, an error detail).
    pub body: Option<String>,
    /// When the card's life clock started. A hold shifts it forward, so the
    /// held time does not count toward the TTL.
    pub created: Instant,
    /// When the card arrived on screen (or was refreshed). Never shifted: the
    /// entrance runs on wall time even while the stack is held.
    pub shown: Instant,
    /// Bumped when a repeat push refreshes this card or a hold is released, so
    /// its life animation restarts instead of resuming a completed one.
    pub revision: u64,
}

impl Toast {
    /// Whether the card has finished its TTL and fade-out.
    pub fn is_expired(&self, now: Instant, fade: Duration) -> bool {
        now.saturating_duration_since(self.created) >= self.kind.ttl() + fade
    }
}

/// How far a card is into its entrance: 0 when it appears, 1 once `enter`
/// has passed since it was shown.
pub fn enter_progress(since_shown: Duration, enter: Duration) -> f32 {
    if enter.is_zero() {
        return 1.0;
    }
    (since_shown.as_secs_f32() / enter.as_secs_f32()).clamp(0.0, 1.0)
}

/// The card's exit opacity `elapsed` into its life: 1 through the TTL, then
/// down to 0 over `fade`.
pub fn exit_opacity(elapsed: Duration, ttl: Duration, fade: Duration) -> f32 {
    let Some(over) = elapsed.checked_sub(ttl) else {
        return 1.0;
    };
    if fade.is_zero() {
        return 0.0;
    }
    (1.0 - over.as_secs_f32() / fade.as_secs_f32()).clamp(0.0, 1.0)
}

/// The stack, oldest first (the layer paints newest nearest the corner).
#[derive(Debug, Default)]
pub struct Toasts {
    next_id: u64,
    items: Vec<Toast>,
    /// When the pointer entered the stack: every clock is frozen from here.
    held_since: Option<Instant>,
}

impl Toasts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a toast and return its id. A repeat of a card still on screen
    /// (same kind, title, and body — a retried command, a repeated settle)
    /// refreshes it instead of stacking a twin.
    pub fn push(&mut self, kind: ToastKind, title: impl Into<String>, body: Option<String>) -> u64 {
        let title = title.into();
        let now = Instant::now();
        if let Some(existing) = self
            .items
            .iter_mut()
            .find(|toast| toast.kind == kind && toast.title == title && toast.body == body)
        {
            existing.created = now;
            existing.shown = now;
            existing.revision += 1;
            return existing.id;
        }
        let id = self.next_id;
        self.next_id += 1;
        if self.len() >= MAX_VISIBLE {
            self.items.remove(0);
        }
        self.items.push(Toast {
            id,
            kind,
            title,
            body,
            created: now,
            shown: now,
            revision: 0,
        });
        id
    }

    /// Remove a card; returns whether it was on the stack.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|toast| toast.id != id);
        self.settle_if_empty();
        self.items.len() != before
    }

    /// Drop expired cards; returns whether the stack changed. A held stack
    /// never expires: the user is reading it.
    pub fn expire(&mut self, now: Instant, fade: Duration) -> bool {
        if self.held_since.is_some() {
            return false;
        }
        let before = self.items.len();
        self.items.retain(|toast| !toast.is_expired(now, fade));
        self.settle_if_empty();
        self.items.len() != before
    }

    /// Freeze every card's clock (the pointer entered the stack). Returns
    /// whether the hold state changed.
    pub fn hold(&mut self, now: Instant) -> bool {
        if self.held_since.is_some() || self.items.is_empty() {
            return false;
        }
        self.held_since = Some(now);
        true
    }

    /// Resume every clock where it stopped (the pointer left the stack).
    /// Returns whether the hold state changed.
    pub fn release(&mut self, now: Instant) -> bool {
        let Some(since) = self.held_since.take() else {
            return false;
        };
        let held = now.saturating_duration_since(since);
        for toast in &mut self.items {
            toast.created += held;
            toast.revision += 1;
        }
        true
    }

    /// Whether the pointer is over the stack (it paints fanned out).
    pub fn is_held(&self) -> bool {
        self.held_since.is_some()
    }

    /// How far `toast` is into its life, not counting time spent held.
    pub fn elapsed(&self, toast: &Toast, now: Instant) -> Duration {
        self.held_since
            .unwrap_or(now)
            .saturating_duration_since(toast.created)
    }

    /// An empty stack has no pointer over it: the next card starts collapsed
    /// with a running clock even if the hover-out event never arrived.
    fn settle_if_empty(&mut self) {
        if self.items.is_empty() {
            self.held_since = None;
        }
    }

    pub fn items(&self) -> &[Toast] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_extension_notify_type_picks_the_card_kind() {
        assert_eq!(ToastKind::from_notify_type(Some("warning")), ToastKind::Warning);
        assert_eq!(ToastKind::from_notify_type(Some("error")), ToastKind::Error);
        assert_eq!(ToastKind::from_notify_type(Some("info")), ToastKind::Info);
        // pi defaults an omitted type to info; an unknown one is not promoted.
        assert_eq!(ToastKind::from_notify_type(None), ToastKind::Info);
        assert_eq!(ToastKind::from_notify_type(Some("success")), ToastKind::Info);
    }

    #[test]
    fn repeat_push_refreshes_instead_of_stacking() {
        let mut toasts = Toasts::new();
        let first = toasts.push(ToastKind::Success, "Copied", None);
        let again = toasts.push(ToastKind::Success, "Copied", None);
        assert_eq!(first, again);
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts.items()[0].revision, 1);
        // A different kind or body is a different card.
        toasts.push(ToastKind::Success, "Copied", Some("path".into()));
        assert_eq!(toasts.len(), 2);
    }

    #[test]
    fn push_caps_the_stack_dropping_the_oldest() {
        let mut toasts = Toasts::new();
        let mut ids = Vec::new();
        for ix in 0..MAX_VISIBLE + 2 {
            ids.push(toasts.push(ToastKind::Info, format!("event {ix}"), None));
        }
        assert_eq!(toasts.len(), MAX_VISIBLE);
        let kept: Vec<u64> = toasts.items().iter().map(|toast| toast.id).collect();
        assert_eq!(kept, ids[2..].to_vec());
    }

    #[test]
    fn dismiss_removes_only_the_named_card() {
        let mut toasts = Toasts::new();
        let a = toasts.push(ToastKind::Info, "a", None);
        let b = toasts.push(ToastKind::Info, "b", None);
        assert!(toasts.dismiss(a));
        assert!(!toasts.dismiss(a));
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts.items()[0].id, b);
    }

    #[test]
    fn expire_honors_kind_ttl_and_fade() {
        let mut toasts = Toasts::new();
        toasts.push(ToastKind::Success, "done", None);
        let created = toasts.items()[0].created;
        let ttl = ToastKind::Success.ttl();
        // Still holding at the end of the TTL, gone once the fade elapsed.
        assert!(!toasts.expire(created + ttl, FADE));
        assert_eq!(toasts.len(), 1);
        assert!(toasts.expire(created + ttl + FADE, FADE));
        assert!(toasts.is_empty());
    }

    #[test]
    fn a_held_stack_does_not_expire_and_resumes_where_it_stopped() {
        let mut toasts = Toasts::new();
        toasts.push(ToastKind::Success, "done", None);
        let created = toasts.items()[0].created;
        let ttl = ToastKind::Success.ttl();
        let entered = created + Duration::from_secs(1);
        assert!(toasts.hold(entered));
        assert!(toasts.is_held());
        // Long past its TTL while held: still there, its clock frozen at 1 s.
        let later = created + ttl * 10;
        assert!(!toasts.expire(later, FADE));
        assert_eq!(
            toasts.elapsed(&toasts.items()[0], later),
            Duration::from_secs(1)
        );
        // Released: the remaining TTL runs from the release, not from birth.
        assert!(toasts.release(later));
        assert_eq!(toasts.items()[0].revision, 1);
        assert_eq!(
            toasts.elapsed(&toasts.items()[0], later),
            Duration::from_secs(1)
        );
        assert!(!toasts.expire(later + ttl - Duration::from_secs(1), FADE));
        assert!(toasts.expire(later + ttl + FADE, FADE));
    }

    #[test]
    fn emptying_the_stack_drops_a_stale_hold() {
        let mut toasts = Toasts::new();
        let id = toasts.push(ToastKind::Info, "a", None);
        assert!(toasts.hold(Instant::now()));
        toasts.dismiss(id);
        assert!(!toasts.is_held());
        // An empty stack cannot be held.
        assert!(!toasts.hold(Instant::now()));
    }

    #[test]
    fn a_card_pushed_while_held_holds_at_full_life() {
        let mut toasts = Toasts::new();
        toasts.push(ToastKind::Info, "a", None);
        let held = Instant::now();
        toasts.hold(held);
        toasts.push(ToastKind::Info, "b", None);
        let b = &toasts.items()[1];
        // Its life has not started, so it is not fading out.
        assert_eq!(
            toasts.elapsed(b, held + Duration::from_secs(60)),
            Duration::ZERO
        );
    }

    #[test]
    fn entrance_and_exit_curves() {
        let ttl = Duration::from_secs(3);
        let enter = Duration::from_millis(150);
        let ms = Duration::from_millis;
        assert_eq!(enter_progress(ms(0), enter), 0.0);
        assert!((enter_progress(ms(75), enter) - 0.5).abs() < 1e-3);
        assert_eq!(enter_progress(ms(900), enter), 1.0);
        assert_eq!(enter_progress(ms(0), Duration::ZERO), 1.0);
        assert_eq!(exit_opacity(ms(1500), ttl, FADE), 1.0);
        assert!((exit_opacity(ttl + FADE / 2, ttl, FADE) - 0.5).abs() < 1e-3);
        assert_eq!(exit_opacity(ttl + FADE, ttl, FADE), 0.0);
        // No fade configured: gone right at the TTL.
        assert_eq!(exit_opacity(ttl + ms(1), ttl, Duration::ZERO), 0.0);
    }

    #[test]
    fn longer_kinds_outlive_shorter_ones() {
        assert!(ToastKind::Error.ttl() > ToastKind::Warning.ttl());
        assert!(ToastKind::Warning.ttl() > ToastKind::Info.ttl());
        assert!(ToastKind::Info.ttl() > ToastKind::Success.ttl());
    }
}
