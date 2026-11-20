//! Orbit's UI layer: named elements and the style seams they are built on.
//!
//! The layer exists so every element in the app has **one owner**
//! (`docs/plans/ui-component-layer.md`). It sits between the design tokens and
//! the surfaces:
//!
//! ```text
//! theme.rs / theme/tokens.rs   colour roles, geometry, interaction vocabulary
//!         │
//!         ▼
//! ui/style/                    zero-cost extension traits (return `Self`)
//! ui/*                         named elements, `IntoElement` (own composition)
//!         │
//!         ▼
//! app/*, transcript_view.rs, git_panel/*, usage/*, explorer/*, …  surfaces
//! ```
//!
//! Two rules hold the boundary:
//!
//! 1. **Nothing here imports a surface.** `ui/*` may use `theme`, `motion`,
//!    `i18n` and `gpui` — never `crate::app`, `OrbitApp`, or a surface module.
//!    `scripts/check_ui_layers.py` fails the build otherwise.
//! 2. **Material is resolved, never decided.** A component asks the theme
//!    (`card_fill`, `panel_fill`, `hover_wash`, `glass_sigma`) instead of naming
//!    an alpha, a state colour, or a backdrop-blur marker. Glass is a theme
//!    treatment, not a component variant — and glass off must leave geometry
//!    untouched.

pub(crate) mod avatar;
pub(crate) mod banner;
pub(crate) mod button;
pub(crate) mod card;
pub(crate) mod chip;
pub(crate) mod choice;
pub(crate) mod chrome;
pub(crate) mod dialog;
pub(crate) mod diff;
pub(crate) mod feedback;
pub(crate) mod field;
pub(crate) mod glyph;
pub(crate) mod icon;
pub(crate) mod kbd;
pub(crate) mod lightbox;
pub(crate) mod list;
pub(crate) mod menu;
pub(crate) mod popover;
pub(crate) mod prelude;
pub(crate) mod rail;
pub(crate) mod resize;
pub(crate) mod row;
pub(crate) mod status_bar;
pub(crate) mod style;
pub(crate) mod switch;
pub(crate) mod tab;
pub(crate) mod table;
pub(crate) mod toast;
pub(crate) mod tooltip;
pub(crate) mod tree;
