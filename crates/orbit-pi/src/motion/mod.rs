//! The micro-interaction layer's primitives.
//!
//! `DESIGN.md` §Motion System says what a surface may do; this module is how it
//! does it. Five primitives, each with its curve exposed as a pure function so
//! the shape is asserted without a window:
//!
//! * [`ease`] — the curves. `Motion::easing` and `chart::ease` resolve through
//!   it, so a transition and a chart cannot drift apart.
//! * [`Transition`] — a one-shot value change that can be interrupted: a
//!   reversal resumes from the value on screen, not from `from`.
//! * [`Spring`] — a critically damped move that reaches its target without
//!   passing it, which is what keeps it inside "nothing bounces".
//! * [`Disclosure`] — a section whose height is measured at the *current* width
//!   and then eased, so a resize cannot snap it to an estimate.
//! * [`Travel`] — the offset a card entering from its anchor starts at.
//!
//! [`registry`] keeps a [`Transition`] alive across frames, keyed by element.
//!
//! **Status: primitives only.** No surface consumes them yet. The adoption
//! order and the call-site contract live in `docs/plans/micro-interactions.md`.
//! That is why this module allows dead code: a primitive with no caller is the
//! expected state while the layer is built, the same way `theme::tokens`
//! documents its own not-yet-adopted vocabulary.
#![allow(dead_code)]

pub(crate) mod ease;
pub(crate) mod interactive;
pub(crate) mod registry;
pub(crate) mod reveal;
pub(crate) mod spring;
pub(crate) mod transition;

// The front door. A re-export no surface reads yet is expected while the layer
// is built, which is the same reason `theme::tokens` allows its unused imports.
#[allow(unused_imports)]
pub(crate) use ease::{bezier, ease_out_quint, linear};
#[allow(unused_imports)]
pub(crate) use interactive::{blend, control, hover, state, Hover};
#[allow(unused_imports)]
pub(crate) use registry::Transitions;
#[allow(unused_imports)]
pub(crate) use reveal::{enter, Disclosure, Travel};
#[allow(unused_imports)]
pub(crate) use spring::{Spring, SpringSample};
#[allow(unused_imports)]
pub(crate) use transition::Transition;
