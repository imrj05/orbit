//! The per-element store that keeps a [`super::Transition`] alive across frames.
//!
//! A transition is state, and an element is rebuilt on every frame, so the state
//! has to live somewhere outside the element. GPUI is single-threaded per
//! window, so a thread-local map keyed by [`gpui::ElementId`] is the smallest
//! thing that works — the same shape the theme already uses for its cached
//! `OS_REDUCE_MOTION` flag.
//!
//! Entries are not pruned: the map holds one small `Copy` entry per element id
//! that has ever transitioned, and the ids are stable per element, so it grows
//! with the number of distinct controls rather than with frames.

use std::cell::RefCell;
use std::collections::HashMap;

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn name(label: &'static str) -> ElementId {
        ElementId::Name(label.into())
    }

    #[test]
    fn a_missing_entry_is_reported_as_missing() {
        assert!(Transitions::get(name("registry-absent")).is_none());
    }

    #[test]
    fn an_entry_survives_between_frames() {
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
    }


}
