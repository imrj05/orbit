//! The micro-interaction layer's primitives.
//!
//! `DESIGN.md` §Motion System says what a surface may do; this module is how it
//! does it. Each primitive exposes its curve as a pure function so the shape
//! is asserted without a window:
//!
//! * [`ease`] — the curves. `Motion::easing` and `chart::ease` resolve through
//!   it, so a transition and a chart cannot drift apart.
//! * [`Transition`] — a one-shot value change that can be interrupted: a
//!   reversal resumes from the value on screen, not from `from`.
//! * [`Travel`] — the offset a card entering from its anchor starts at.
//!
//! [`registry`] keeps a [`Transition`] alive across frames, keyed by element.
//!
//! Only primitives a surface calls live here; the adoption order and the
//! call-site contract are in `docs/plans/micro-interactions.md`.

pub(crate) mod ease;
pub(crate) mod interactive;
pub(crate) mod registry;
pub(crate) mod reveal;
pub(crate) mod transition;

// The front door.
pub(crate) use interactive::{blend, control, hover, state, Hover};
pub(crate) use reveal::enter;
