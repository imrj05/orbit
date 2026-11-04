//! The per-element store that keeps a [`super::Transition`] alive across frames.
//!
//! A transition is state, and an element is rebuilt on every frame, so the state
//! has to live somewhere outside the element. GPUI is single-threaded per
//! window, so a thread-local map keyed by [`gpui::ElementId`] is the smallest
//! thing that works — the same shape the theme already uses for its cached
//! `OS_REDUCE_MOTION` flag.
//!
//! The map is pruned by [`Transitions::sweep`], which the layer calls when it
//! renders a surface. Without it the map would keep one entry per element ever
//! hovered; with it, an entry lives only while its transition is in flight.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Instant;

use gpui::ElementId;

use super::transition::Transition;

thread_local! {
    static TRANSITIONS: RefCell<HashMap<ElementId, Transition>> = RefCell::new(HashMap::new());
}

/// The layer's transition store.
pub(crate) struct Transitions;

impl Transitions {
    /// Read the transition for `id` and write back what the closure returns.
    ///
    /// A missing entry is `None`, which lets the caller build a settled
    /// transition from whatever is painted now instead of inventing a `from`.
    pub(crate) fn update<R>(
        id: impl Into<ElementId>,
        update: impl FnOnce(Option<Transition>) -> (Transition, R),
    ) -> R {
        TRANSITIONS.with(|transitions| {
            let id = id.into();
            let mut transitions = transitions.borrow_mut();
            let current = transitions.get(&id).copied();
            let (next, result) = update(current);
            transitions.insert(id, next);
            result
        })
    }

    /// The transition for `id`, if one is in flight.
    pub(crate) fn get(id: impl Into<ElementId>) -> Option<Transition> {
        TRANSITIONS.with(|transitions| transitions.borrow().get(&id.into()).copied())
    }

    /// Drop every transition that has reached its target.
    pub(crate) fn sweep(now: Instant) -> usize {
        TRANSITIONS.with(|transitions| {
            let mut transitions = transitions.borrow_mut();
            let before = transitions.len();
            transitions.retain(|_, transition| !transition.is_done(now));
            before - transitions.len()
        })
    }

    /// How many transitions are in flight. Tests only.
    #[cfg(test)]
    pub(crate) fn len() -> usize {
        TRANSITIONS.with(|transitions| transitions.borrow().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn name(label: &'static str) -> ElementId {
        ElementId::Name(label.into())
    }

    #[test]
    fn a_missing_entry_is_reported_as_missing() {
        assert!(Transitions::get(name("registry-absent")).is_none());
    }

    #[test]
    fn an_entry_survives_between_frames_and_is_pruned_when_it_settles() {
        let start = Instant::now();
        let id = name("registry-lives");
        let value = Transitions::update(id.clone(), |current| {
            assert!(current.is_none(), "a fresh id has no transition");
            let transition = Transition::new(
                0.0,
                1.0,
                Duration::from_millis(100),
                crate::motion::ease::linear,
                start,
            );
            (transition, transition.value_at(start))
        });
        assert_eq!(value, 0.0);
        assert_eq!(
            Transitions::get(id.clone()).map(|t| t.value_at(start)),
            Some(0.0),
            "the transition is still there on the next frame"
        );

        // Settle it, then sweep: the entry is gone and the map does not grow.
        let later = start + Duration::from_millis(150);
        assert_eq!(Transitions::sweep(later), 1);
        assert!(Transitions::get(id).is_none());
    }

    #[test]
    fn an_in_flight_entry_is_not_swept() {
        let start = Instant::now();
        let id = name("registry-in-flight");
        let _ = Transitions::update(id.clone(), |_| {
            (
                Transition::new(
                    0.0,
                    1.0,
                    Duration::from_millis(200),
                    crate::motion::ease::linear,
                    start,
                ),
                0,
            )
        });
        assert_eq!(Transitions::sweep(start + Duration::from_millis(100)), 0);
        assert!(Transitions::get(id).is_some());
        assert_eq!(Transitions::sweep(start + Duration::from_millis(200)), 1);
    }
}
