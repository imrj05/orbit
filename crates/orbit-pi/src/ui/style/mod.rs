//! Style seams: extension traits that inherit the design system without owning
//! a tree.
//!
//! A trait here returns `Self`, so it composes with any GPUI builder chain at
//! zero cost. That is the right tool for the ~250 sites that only need the
//! *state* vocabulary; a named element in `ui/` is for the few that own
//! composition or behaviour.

pub(crate) mod control;
pub(crate) mod surface;
pub(crate) mod text;
