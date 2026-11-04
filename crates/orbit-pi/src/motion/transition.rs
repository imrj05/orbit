//! A one-shot value change that can be interrupted.
//!
//! `with_animation` keys its clock to the element id, so it cannot express a
//! reversal: reusing the id freezes the delta at 1 after the first run, and a
//! fresh id restarts it at 0. A [`Transition`] fixes both by choosing the new
//! element id *and* remembering what was on screen when the target changed. The
//! animator then interpolates from that painted value, so entering and leaving a
//! control never snaps, however fast the pointer moves.
//!
//! The caller owns the element id (a generation suffix) and the animator; this
//! type owns the bookkeeping.

use std::time::{Duration, Instant};

use crate::theme::tokens::Motion;
use crate::theme::Theme;

/// One value moving toward another over a role's duration.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Transition {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
    ease: fn(f32) -> f32,
    generation: u64,
}

impl Transition {
    /// A transition resting at `value`, with nothing in flight.
    pub(crate) fn settled(value: f32, now: Instant) -> Self {
        Self {
            from: value,
            to: value,
            started: now,
            duration: Duration::ZERO,
            ease: crate::motion::ease::linear,
            generation: 0,
        }
    }

    /// A transition from an explicit value toward `to`.
    pub(crate) fn new(
        from: f32,
        to: f32,
        duration: Duration,
        ease: fn(f32) -> f32,
        now: Instant,
    ) -> Self {
        Self {
            from,
            to,
            started: now,
            duration,
            ease,
            generation: 0,
        }
    }

    /// A transition between two values on a role.
    ///
    /// The role carries both halves of the design system's contract: its easing
    /// and its Reduce Motion guard. A suppressed role resolves to
    /// `Duration::ZERO`, so the transition reports its target immediately and
    /// the caller's animator paints the end state on the first frame.
    pub(crate) fn role(from: f32, to: f32, role: Motion, theme: &Theme, now: Instant) -> Self {
        Self::new(from, to, role.duration(theme), role.easing(), now)
    }

    /// Set the curve the transition eases with.
    ///
    /// Builder rather than a constructor argument for the common case: a
    /// settled transition has no duration, so its easing only matters once the
    /// caller retargets it.
    pub(crate) fn easing(mut self, ease: fn(f32) -> f32) -> Self {
        self.ease = ease;
        self
    }

    /// The value the transition started from — what the animator calls `from`.
    pub(crate) fn from(&self) -> f32 {
        self.from
    }

    /// The value it is heading for.
    pub(crate) fn to(&self) -> f32 {
        self.to
    }

    /// Bumped on every retarget. The caller keys its element id on this, which
    /// is what restarts `with_animation` at a delta of zero.
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// Linear progress, 0 to 1. The caller applies the role's easing; use
    /// [`Self::progress_at`] for the eased value.
    pub(crate) fn elapsed_progress(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return 1.0;
        }
        (now.saturating_duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32())
            .clamp(0.0, 1.0)
    }

    /// Eased progress, 0 to 1: what an animator interpolates with.
    pub(crate) fn progress_at(&self, now: Instant) -> f32 {
        (self.ease)(self.elapsed_progress(now))
    }

    /// The value on screen at `now`.
    pub(crate) fn value_at(&self, now: Instant) -> f32 {
        self.from + (self.to - self.from) * self.progress_at(now)
    }

    /// Whether the value has reached its target.
    pub(crate) fn is_done(&self, now: Instant) -> bool {
        self.elapsed_progress(now) >= 1.0
    }

    /// Point the transition at a new target.
    ///
    /// The new start is the value currently on screen, so a reversal resumes
    /// from where the eye last saw it. Returns `true` when the target or the
    /// duration actually changed — the caller uses that to repaint only when
    /// something moved.
    pub(crate) fn retarget(&mut self, to: f32, duration: Duration, now: Instant) -> bool {
        if (to - self.to).abs() <= f32::EPSILON && duration == self.duration {
            return false;
        }
        self.from = self.value_at(now);
        self.to = to;
        self.started = now;
        self.duration = duration;
        self.generation = self.generation.wrapping_add(1);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LENGTH: Duration = Duration::from_millis(200);

    fn at(start: Instant, millis: u64) -> Instant {
        start + Duration::from_millis(millis)
    }

    #[test]
    fn a_transition_starts_where_it_says_it_starts() {
        let start = Instant::now();
        let transition = Transition::new(0.0, 1.0, LENGTH, crate::motion::ease::linear, start);
        assert_eq!(transition.value_at(start), 0.0);
        assert!(!transition.is_done(start));
        assert_eq!(transition.value_at(at(start, 200)), 1.0);
        assert!(transition.is_done(at(start, 200)));
    }

    /// The load-bearing guarantee: a reversal resumes from the painted value.
    #[test]
    fn a_transition_restarts_from_the_painted_value() {
        let start = Instant::now();
        let mut transition = Transition::new(0.0, 1.0, LENGTH, crate::motion::ease::linear, start);
        let reversal = at(start, 80);
        let painted = transition.value_at(reversal);
        assert!((painted - 0.4).abs() < 1e-6, "expected 40 %, got {painted}");

        let generation = transition.generation();
        assert!(transition.retarget(0.0, LENGTH, reversal));
        assert_eq!(
            transition.generation(),
            generation + 1,
            "the id must change"
        );
        assert!(
            (transition.value_at(reversal) - painted).abs() < 1e-6,
            "the first frame of the reversal must match the last painted frame"
        );
        // Halfway back down from 0.4 is 0.2, not 0.5 (which is where a restart
        // from `from` would put it).
        let midway = transition.value_at(at(reversal, 100));
        assert!((midway - 0.2).abs() < 1e-6, "expected 0.2, got {midway}");
        assert!(!transition.is_done(at(reversal, 100)));
        assert_eq!(transition.value_at(at(reversal, 200)), 0.0);
        assert!(transition.is_done(at(reversal, 200)));
    }

    #[test]
    fn a_retarget_to_the_same_target_is_a_no_op() {
        let start = Instant::now();
        let mut transition = Transition::new(0.0, 1.0, LENGTH, crate::motion::ease::linear, start);
        let generation = transition.generation();
        assert!(!transition.retarget(1.0, LENGTH, at(start, 50)));
        assert_eq!(transition.generation(), generation, "no repaint is needed");
        // A different duration is a real change, even with the same target.
        assert!(transition.retarget(1.0, Duration::from_millis(80), at(start, 50)));
    }

    #[test]
    fn reduce_motion_collapses_the_transition_to_its_target() {
        let mut theme = Theme::default();
        theme.ui.motion = crate::theme::MotionPreference::Reduced;
        let start = Instant::now();
        let transition = Transition::role(0.0, 1.0, Motion::Interaction, &theme, start);
        assert_eq!(transition.value_at(start), 1.0);
        assert!(transition.is_done(start));
    }

    #[test]
    fn a_role_transition_uses_the_roles_duration_and_easing() {
        let theme = Theme::default();
        let start = Instant::now();
        let transition = Transition::role(0.0, 1.0, Motion::Layout, &theme, start);
        assert_eq!(transition.value_at(start), 0.0);
        assert!(transition.value_at(at(start, 200)) == 1.0);
        // The role's easing, not linear: quint is already past the halfway
        // point at half the duration.
        let halfway = transition.value_at(at(start, 100));
        assert!(halfway > 0.5, "Layout decelerates, got {halfway}");
    }
}
