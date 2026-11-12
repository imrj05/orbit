//! The new-task page's geometry: one centered composer, with the folder it
//! will run in named directly above it.
//!
//! The page is not a separate layout. It is the chat column with the
//! transcript empty, so the composer has to sit somewhere else on the page
//! than it does in a session — centered, and narrower. That difference is a
//! pair of flex weights and a width, both expressed here as pure numbers so the
//! settle transition and the resting states can be asserted without a window.
//!
//! Two rules hold the motion together:
//!
//! * **One geometry.** The empty page is `p = 0` of [`settle`] and the session
//!   is `p = 1`. Neither state invents its own numbers, so the send transition
//!   cannot start from a frame the page was not already painting.
//! * **Weights, not pixels.** The composer's height changes with its content
//!   and the UI scale, so where it sits is decided by the share of the free
//!   space above it. A pixel offset would have to be measured; a share is
//!   computed.

use gpui::{px, relative, Pixels, Styled};

use crate::theme::tokens::Motion;
use crate::theme::Theme;

/// Widest the centered new-task composer gets. It remains narrower than a
/// session's [`super::view::CONTENT_MAX_W`] and shrinks with the window.
pub(super) const COMPOSER_MAX_W: f32 = 800.;

/// Side padding the page keeps around the column, matching the composer
/// wrapper's own `px_4` in `view.rs`.
pub(super) const PAGE_PAD_X: f32 = 16.;

/// Share of the free space that sits *above* the centered composer. Slightly
/// under half, because a box that is geometrically centered reads low: the
/// eye measures from the top bar to the box's top edge, and the folder row
/// and status bar under it belong to the same group.
pub(super) const CENTER_SHARE: f32 = 0.44;

/// Set a flex weight on an element.
///
/// `flex_grow()` offers only 1, and this page needs the fraction between its
/// two resting layouts: the empty page splits the free space 44/56 around the
/// composer, a session gives all of it to the transcript.
pub(super) fn weight<E: Styled>(mut el: E, grow: f32) -> E {
    el.style().flex_grow = Some(grow.max(0.));
    el.style().flex_shrink = Some(1.);
    el.style().flex_basis = Some(relative(0.).into());
    el
}

/// The composer column's width for a given main-area width.
pub(super) fn column_width(main_width: Pixels) -> Pixels {
    px((f32::from(main_width) - 2. * PAGE_PAD_X).clamp(0., COMPOSER_MAX_W))
}

/// Where the composer sits at `progress` through the send transition.
///
/// `progress` is the eased 0..1 the caller drives ([`Motion::Travel`] on send).
/// The two shares are flex weights on the two elements that bracket the
/// composer: the transcript slot above it and a spacer below it. Their *ratio*
/// is what fixes the position — `share / (share + gap)` is the fraction of the
/// free space above the composer — so a weight of 1 above and 0 below is the
/// session page, and 1 above and `(1 - CENTER_SHARE) / CENTER_SHARE` below is
/// the centered new-task page. The weights are deliberately unnormalised: `p`
/// has to reach the session page *exactly* at 1, and `gap = 0` is the only
/// value that does, however the pair is scaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Settle {
    /// Flex weight above the composer. Always 1: the slot above is the
    /// transcript, which fills whatever it is given.
    pub(super) slot: f32,
    /// Flex weight below the composer; `0` once the composer is docked.
    pub(super) gap: f32,
    /// The composer column's width: the centered page's, the session's, or the
    /// one between them mid-transition.
    pub(super) width: Pixels,
}

/// The two resting layouts and the transition between them.
///
/// `page` is [`column_width`] for the current main area; `session` is the
/// session column's width (the caller's `px_4` wrapper already accounts for the
/// page padding). Reduce Motion is the caller's to resolve: it paints
/// [`settle_session`] and never mounts the transition, so nothing here has to
/// know about the preference.
pub(super) fn settle(progress: f32, page: Pixels, session: Pixels) -> Settle {
    let p = progress.clamp(0.0, 1.0);
    let width = f32::from(page) + (f32::from(session) - f32::from(page)) * p;
    Settle {
        slot: 1.,
        gap: gap_for(page_share(p)),
        width: px(width),
    }
}

/// The centered page at rest: the composer with room above and below.
pub(super) fn settle_page(page: Pixels) -> Settle {
    settle(0., page, page)
}

/// The session at rest: the transcript fills, the composer docks.
pub(super) fn settle_session(session: Pixels) -> Settle {
    Settle {
        slot: 1.,
        gap: 0.,
        width: session,
    }
}

/// Share of the free space above the composer at `progress`: the centered
/// page's [`CENTER_SHARE`] easing up to the session's whole column.
fn page_share(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    // Linear in the *share*, not in the weight: the composer's travel should
    // read as one steady descent, and the share is what the eye sees.
    CENTER_SHARE + (1. - CENTER_SHARE) * p
}

/// The flex weight below the composer that puts `share` of the free space
/// above it, given a weight of 1 above.
fn gap_for(share: f32) -> f32 {
    let share = share.clamp(0.0, 1.0);
    if share >= 1. {
        return 0.;
    }
    (1. - share) / share
}

/// A gentle deceleration for the composer's longer vertical journey. The
/// shared Travel duration stays unchanged, but the slide does not race through
/// most of its distance in the first few frames.
pub(super) fn slide_easing(progress: f32) -> f32 {
    1. - (1. - progress.clamp(0., 1.)).powi(3)
}

/// The folder row's collapse on send: its natural height and its ink, both
/// `1 - progress`. It disappears at the end of the transition, when the status
/// bar's Work-in chip has taken over the same question.
pub(super) fn folder_row(progress: f32) -> (f32, f32) {
    let remaining = 1. - progress.clamp(0.0, 1.0);
    (remaining, remaining)
}

/// The transcript's fade-in on send. Its slot is already growing with the
/// composer's descent; this is the ink arriving in it.
pub(super) fn transcript_ink(progress: f32) -> f32 {
    progress.clamp(0.0, 1.0)
}

/// Whether the send transition is still running. `None` (never sent from this
/// page) and Reduce Motion both answer false; the elapsed window is
/// [`Motion::Travel`]'s, so the painted end frame and the unmounted one are the
/// same frame.
pub(super) fn settling(at: Option<std::time::Instant>, theme: &Theme) -> bool {
    let Some(at) = at else {
        return false;
    };
    let duration = Motion::Travel.duration(theme);
    !duration.is_zero() && at.elapsed() < duration
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeId;

    #[test]
    fn the_page_rests_centered_and_the_session_rests_docked() {
        let page = settle_page(px(COMPOSER_MAX_W));
        let session = settle_session(px(960.));

        // The composer's top edge as a fraction of the free space: what the
        // eye reads as "centered".
        let centered = page.slot / (page.slot + page.gap);
        assert!(
            (centered - CENTER_SHARE).abs() < 1e-6,
            "centered share was {centered}"
        );

        assert_eq!(session.gap, 0., "a docked composer has no room below it");
        assert_eq!(
            session.slot / (session.slot + session.gap),
            1.,
            "the transcript owns the whole column"
        );
        assert_eq!(page.width, px(COMPOSER_MAX_W));
        assert_eq!(session.width, px(960.));
    }

    #[test]
    fn the_send_transition_is_continuous_at_both_ends() {
        let page = px(COMPOSER_MAX_W);
        let session = px(960.);

        let start = settle(0., page, session);
        let end = settle(1., page, session);
        assert_eq!(
            start.gap,
            settle_page(page).gap,
            "starts where the page was"
        );
        assert_eq!(start.width, page);
        assert_eq!(end.gap, 0., "ends exactly docked");
        assert_eq!(end.width, session);

        // Monotone on the way: the composer only ever descends, and the column
        // only ever widens.
        let mut prev = settle(0., page, session);
        for step in 1..=20 {
            let next = settle(step as f32 / 20., page, session);
            assert!(next.gap <= prev.gap + 1e-6, "the gap never grows back");
            assert!(next.width >= prev.width, "the column never narrows");
            let share = next.slot / (next.slot + next.gap);
            assert!(
                share >= prev.slot / (prev.slot + prev.gap) - 1e-6,
                "the composer never rises"
            );
            prev = next;
        }
    }

    #[test]
    fn the_slide_decelerates_without_a_first_frame_jump() {
        assert_eq!(slide_easing(0.), 0.);
        assert_eq!(slide_easing(1.), 1.);
        assert!(slide_easing(0.25) > 0.25);
        assert!(slide_easing(0.25) < 0.7);
        assert!(slide_easing(0.5) < slide_easing(0.75));
    }

    #[test]
    fn the_folder_row_and_transcript_cross_fade() {
        let (height, ink) = folder_row(0.);
        assert_eq!((height, ink), (1., 1.), "the row starts whole");
        let (height, ink) = folder_row(1.);
        assert_eq!((height, ink), (0., 0.), "and ends gone");
        assert_eq!(transcript_ink(0.), 0.);
        assert_eq!(transcript_ink(1.), 1.);
        // Halfway, both are half — one crossfade, not two fades at different
        // speeds.
        assert!((folder_row(0.5).1 - (0.5)).abs() < 1e-6);
        assert!((transcript_ink(0.5) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_transition_only_runs_on_its_own_travel_window() {
        let theme = Theme::for_id(ThemeId::Orbit);
        assert!(!settling(None, &theme), "a page that never sent");
        assert!(settling(Some(std::time::Instant::now()), &theme));

        // Reduce Motion paints the settled session frame instead.
        let mut reduced = Theme::for_id(ThemeId::Orbit);
        reduced.ui.motion = crate::theme::MotionPreference::Reduced;
        assert!(
            !settling(Some(std::time::Instant::now()), &reduced),
            "no transition to run under Reduce Motion"
        );
    }

    #[test]
    fn a_narrow_window_narrows_the_column_before_it_scrolls() {
        assert_eq!(column_width(px(1200.)), px(COMPOSER_MAX_W));
        assert_eq!(column_width(px(500.)), px(500. - 2. * PAGE_PAD_X));
        // Never negative, however small the column gets.
        assert_eq!(column_width(px(10.)), px(0.));
    }
}
