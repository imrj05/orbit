# UI/UX Modernization — Phase 1 audit

**Issue:** OPI-31 (Phase 1 — Audit UI architecture and refine design tokens)
**Epic:** OPI-30 — UI/UX Modernization: Orbit design language
**Status:** Deliverables complete, pending review (phase 1's done-gate is
"the plan is reviewed and agreed").
**Date:** 2026-11-01
**Branch:** `feat/ui-redesign` @ `885d9993`
**Baseline:** `cargo check -p orbit-pi` clean; `cargo test -p orbit-pi --bin orbit-pi theme`
= 64 passed.

This document is the inventory, the gap analysis, and the corrected plan for
phases 2–9 that §1 of the spec asks for. It is written against the code as it
stands, not against the design documents.

---

## 1. Method

* Read `DESIGN.md`, `PRODUCT.md`, `INTENT.md` (D11, D13, D15), `AGENT.md`, and
  `zed-design-tokens.md`.
* Read the whole theme layer: `theme.rs` (2,585 lines), `theme/tokens.rs`
  (1,546), `theme/appearance.rs` (793).
* Counted interaction-state, motion, and colour usage mechanically across
  `crates/orbit-pi/src` (182 `.rs` files, 173,752 lines) and then read every
  site that mattered by hand.
* Walked each surface in §1's list: sidebar, transcript, composer, command
  palette, settings, Explorer, Review, Git, Terminal, plus the shell.
* Ran the theme test suite to establish the baseline.

Counts below are exact as of this commit and are reproducible with `grep`; the
commands are listed in the appendix.

---

## 2. Headline: most of this epic is already implemented

The epic was written as if Orbit had no design system. It has a mature one.
What Orbit lacks is **adoption**, not construction:

| Layer | State |
| -- | -- |
| Semantic colour tokens | **Done.** 40 roles on `Theme`, 12 palettes, per-palette contrast tests. |
| Geometry vocabulary | **Done.** Radius / spacing / text / icon / button / elevation / component metrics, ported 1:1 from Zed and tested. |
| Dark / Light / System resolution | **Done.** `AppearanceMode::System` resolves through `WindowAppearance`, live, with preferences persisted per design system. |
| Empty states | **Mostly done.** One shared primitive, 18 call sites. |
| Command palette | **Done.** Sections, fuzzy scoring, inline shortcuts, context-aware enablement, focus restore. |
| Workspace persistence | **Half done.** Four panel sizes persist; presets and custom layouts do not exist. |
| Interaction states | **Not done.** Geometry is shared; colours are re-decided at 289 call sites. |
| Motion | **Not done.** 26 animation sites, 2 read a token, 7 respect reduce-motion. |
| Error states | **Not done.** The shared error primitive is a truncated red string. |

The consequence for planning: **phases 4, 6, 7 and 8 are far smaller than their
estimates assume**, and phases 2 and 3 are larger. The sequence is still
correct — the dependency chain is real — but the point estimates are not.
Recommendation in §6.

---

## 3. Surface inventory

`hover` counts `.hover(|_)` styling closures. `focus` counts `.focus(|_)`.
`anim` counts `Animation::new` / `with_animation`. A dash means the surface
solves the state another way (noted).

| Surface | File | Lines | hover | focus | anim | Notes |
| -- | -- | --: | --: | --: | --: | -- |
| Transcript | `transcript_view.rs` | 11,461 | 22 | 0 | 8 | Largest surface. Owns tool cards, copy actions, streaming. No focus treatment. |
| Git panel | `git_panel/mod.rs` | 9,486 | 39 | 0 | 0 | Second-largest. Most hover sites of any file after `view.rs`. |
| Settings | `app/settings.rs` | 9,370 | 33 | 2 | 0 | 7 popup layers, 20+ card/row renderers. |
| Shell / view | `app/view.rs` | 5,436 | 46 | 2 | 18 | Owns the workbench layout and most top-bar controls. |
| Usage | `usage/view.rs` | 4,744 | 14 | 0 | 2 | Tables + charts drawn with `div`/`canvas`. |
| App + sessions | `app/session.rs` | 3,519 | 10 | 0 | 2 | Session lifecycle, quota popup, details popup. |
| Composer text | `composer.rs` | 3,542 | — | — | — | Text engine; chrome lives in `view.rs`. |
| Sidebar | `app/sidebar.rs` | 3,382 | 15 | 0 | 3 | Groups, pin, archive, sort, three menus. |
| Side pane | `sidepane.rs` | 3,069 | 13 | 0 | 0 | Right pane host. |
| Terminal | `terminal.rs` | 2,992 | 7 | 0 | 0 | ANSI palette derived from semantic roles. |
| Theme | `theme.rs` | 2,585 | — | — | — | Palettes + resolver + 23 tests. |
| Model selector | `model_selector.rs` | 2,453 | 3 | 0 | 0 | |
| Explorer panel | `explorer/panel.rs` | 1,757 | 6 | 0 | 0 | |
| Explorer viewer | `explorer/viewer.rs` | 1,619 | 4 | 0 | 0 | |
| Tokens | `theme/tokens.rs` | 1,546 | — | — | — | 16 tests. |
| Command palette | `command_palette.rs` | 1,533 | — | — | — | Hover is `on_mouse_move` driving the keyboard highlight, by design. |
| Review | `review.rs` | 1,406 | 0 | 0 | 0 | Renders through `diff_view`. |
| Helpers (primitives) | `app/helpers.rs` | 1,279 | 2 | 2 | 4 | `button_frame`, `icon_button_frame`, `context_menu_*`, `empty_state`. |
| Dialog | `dialog.rs` | 847 | — | — | — | One `on_hover`; keyboard highlight doubles as hover. |
| Appearance | `theme/appearance.rs` | 793 | — | — | — | Mode + palette resolution, 18 tests. |
| Diff view | `diff_view.rs` | 583 | 0 | 0 | 0 | |
| Toasts | `app/toast_ui.rs` | 417 | 5 | 0 | 4 | Only surface with a complete reduce-motion path. |

### 3.1 Shared primitives that already exist

These are the seams phases 2 and 8 should build on — they exist and are used.

| Primitive | Location | Call sites |
| -- | -- | --: |
| `button_frame` | `app/helpers.rs` | 182 |
| `icon_button_frame` | `app/helpers.rs` | 73 |
| `input_field_frame` | `app/helpers.rs` | 17 |
| `empty_state` / `empty_state_glyph` | `app/helpers.rs` | 18 |
| `segmented_segment` | `app/helpers.rs` | 7 |
| `context_menu_surface` / `context_menu_entry` | `app/helpers.rs` | 2 / 2 |
| `RaisedExt::raised` / `raised_flat` | `theme/tokens.rs` | 209 / 18 |

`button_frame`'s own doc comment states the gap: *"Callers keep the colors,
border, hover, press, and click handling."* That sentence is the whole of
phase 2.

---

## 4. Token inventory

### 4.1 Colour roles (`Theme`, `theme.rs:333`)

Surfaces `background`, `surface`, `surface_secondary`, `surface_elevated`,
`surface_hover`, `surface_active`, `surface_selected`, `selection` ·
Text `text_primary`, `text_secondary`, `text_muted`, `text_disabled` ·
Accent `accent`, `accent_hover`, `accent_pressed`, `accent_fg`, `accent_text`,
`focus_ring` · Action `action`, `action_hover`, `action_pressed`, `action_fg` ·
Status `success`, `warning`, `error`, `error_hover`, `error_fg`, `info` ·
Specialised `toggle_knob`, `overlay`, `overlay_strong`, `border_strong`,
`assistant_text`, `code_bg`, `code_text`, `inline_code_bg`, `shadow_contact` ·
Syntax `syn_string`, `syn_number`, `syn_function`, `syn_type`, `syn_comment`,
`syn_literal`, `syn_meta`, `syn_operator`.

Covered by tests that would fail on a broken palette:
`every_palette_exposes_the_semantic_vocabulary`,
`default_palettes_ship_the_agreed_tokens`,
`every_theme_has_readable_foregrounds`,
`interactive_and_muted_foregrounds_clear_aa`,
`selection_rows_keep_their_metadata_readable`,
`every_theme_holds_its_surface_grammar`,
`the_project_name_ink_stays_readable_in_every_palette`.

### 4.2 Geometry and motion (`theme/tokens.rs`)

`Radius`, `UiDensity`, `DynamicSpacing` (13 steps), `TextSize`, `HeadlineSize`,
`BufferLineHeight`, `IconSize`, `ButtonSize`, `ListItemSpacing`,
`ElevationIndex`, `AnimationDuration`, `ScrollbarStyle`; component modules
`button`, `list_item`, `panel`, `list`, `picker`, `popover`, `context_menu`,
`toast`, `modal`, `tooltip`, `input`, `scrollbar`; `RaisedExt`; `FADE_IN_*`,
`SLIDE_IN_DISTANCE`.

Tested by 16 tests, including `default_settings_resolve_to_zeds_default_pixels`,
`elevation_shadows_match_zed`, `durations_are_zeds`.

### 4.3 Where the token layer leaks

| Leak | Evidence | Verdict |
| -- | -- | -- |
| Hardcoded colours in UI code | 639 hex literals total, but **600 of them are palette data** in `theme.rs` + `theme/appearance.rs`. Outside the theme: `mentions.rs` (40, file-type badge brand colours), `providers.rs` (21, provider brand colours), `terminal.rs` (4, ANSI fallback). | **Acceptable.** Brand colours are content, not chrome. |
| Interaction-state colours | 289 hover / 53 active / 8 focus closures, each naming `surface_hover` / `surface_active` / `focus_ring` itself. | **Real leak.** Fixed in this phase — see §5.1. |
| Motion | 26 animation sites; 2 use `AnimationDuration`. 151 `Duration::from_millis`, of which ~125 are timeouts/sleeps/tests and ~26 are animation durations typed at the call site. | **Real leak.** Phase 3. |
| Disabled | No shared treatment. Three ad-hoc sites: `usage/filters.rs` (`bg(overlay)`), `sidepane.rs`, `git_panel` (`cursor_not_allowed`). | **Real leak.** Phase 2. |
| Doc-to-code names | `DESIGN.md` names `menu_bg`, `popover_shadow`, `card_shadow`: **0 references in code.** The code has `surface_elevated` and `ElevationIndex`. §Interaction System says focus uses `border_strong` and selection uses `active`; the code uses `focus_ring` and `surface_selected`. | **Doc drift.** OPI-40. |

---

## 5. Gap analysis

### 5.1 Interaction states (phase 2's raw material)

Every interactive surface must answer the same six questions: rest, hover,
pressed, selected, focused, disabled. Today:

* **Geometry is inherited** through `button_frame` / `icon_button_frame` /
  `input_field_frame` — 255 button call sites across ~20 files.
* **Colour is not.** The dominant idiom (`.hover(|s| s.bg(theme.surface_hover))`
  plus its `raised` twins) appears ~200 times in five spellings; a further
  10 call sites use `overlay_strong`, `accent.opacity(0.4)`, or `opacity(PRESS_DIM)`.
* **Focus is the thinnest state.** 32 references to `focus_ring`, of which 8
  are `.focus()` closures and 2 are the frame helpers. That leaves most of the
  255 buttons, every menu entry outside `context_menu_surface`, and every list
  row without a keyboard-focus signal — which contradicts §4 of
  `DESIGN.md` ("Focus is a first-class state") and phase 9's gate.
* **Disabled has no contract.** Three unrelated treatments, and no
  non-interactivity guarantee beyond one `cursor_not_allowed`.

**This phase adds the missing vocabulary** — `interaction::InteractionState`
in `theme/tokens.rs`, resolving fill / border / ink for the four states that
are not per-component. Adoption (turning 255 call sites into inherited state)
is phase 2, deliberately.

### 5.2 Motion (phase 3's raw material)

* Tokens: `AnimationDuration` = 50 / 150 / 300 ms (Zed's port) plus the
  `Motion` roles layered on it — `Instant` 50, `Interaction` 150 (both Zed's),
  `Layout` 200 (Orbit's, from the 160–220 band). One easing re-export
  (`ease_out_quint`) before; `Motion::easing` now names it per role.
* Spec (`DESIGN.md` §Motion System): Instant 0–80, Interaction 120–160,
  Layout 160–220. The token scale matches the first two bands and misses the
  third (`Slow` = 300).
* Observed one-shot durations at call sites: 110, 130, 170, 200, 240, 450 ms —
  all inside or just outside the spec bands, none named.
* Loops: sidebar shimmer 2,000 ms, transcript pulses 1,600 / 2,000 ms,
  updater 1,100 ms, usage chart 1,100 ms. `DESIGN.md` sanctions **one** loop
  (the new-task backdrop dither field). Five unsanctioned loops ship.
* Reduce-motion: the setting exists (`theme.ui.reduce_motion`, Settings →
  Accessibility) and is honoured in 6 places — toasts, backdrop, sidebar
  shimmer, two spinners, one session gesture. Nineteen of the 26 animation
  sites never consult it.

**Decision (resolved).** `AnimationDuration` stays Zed's port — retuning it would
break the module's stated 1:1 contract and change nothing at the 24 call sites that
ignore it. Instead the layers now sit one on top of the other: `Motion::Instant` and
`Motion::Interaction` resolve to Zed's `Instant` and `Fast`, and `Motion::Layout` is
Orbit's own 200 ms because Zed's `Slow` (300 ms) reads as hesitation on a workbench
panel. `Motion::duration` carries the reduced-motion guard, so a surface cannot
forget it. Migrating the 26 sites is phase 3. `DESIGN.md` §Motion System now
documents the roles and the loop policy (state loops allowed; one non-state loop).

### 5.3 Duplicated components

| Duplication | Evidence | Owner |
| -- | -- | -- |
| Popup/dialog shells | 20 hand-rolled `*_popup` / `*_dialog` / `*_layer` renderers across 9 files, each re-deriving surface, shadow, radius, and dismissal. | Phase 2 |
| Row renderers | 20+ `*_row` functions; each re-decides hover fill and ink. | Phase 2 |
| List highlight | Command palette drives highlight from `on_mouse_move`; sidebar/dialogs use `.hover()`. Two mechanisms for one visual state. | Phase 2 |
| Error presentation | `helpers::runtime_error` (truncated red text) vs `provider_error_card` vs `mcp_notice_card` vs worktree error card. | Phase 8 |

### 5.4 Implemented vs genuinely missing, per phase

| Phase | Already implemented | Genuinely missing |
| -- | -- | -- |
| 4 Sidebar | Workspace grouping + collapse, sort, archived filter, pins (`pins.rs`), session context menu, workspace menu + picker, status lanes, empty state, shortcut hints | A **recent** group in the sidebar (recents exist only in the palette), a dedicated session search field, deterministic ordering contract for recent/pinned |
| 5 Workspace | Resizable sidebar / side pane / explorer / terminal, debounced persistence of all four sizes (`layout.rs`), panel show/hide commands | Layout **presets** (Focus / Developer / Review), custom layouts, restoring *collapsed* state, enforcing min sizes as a contract, schema tolerance beyond range-clamping |
| 6 Palette | Sections, fuzzy scoring, inline shortcut display, context-aware enable/disable, single-frame filtering (last commit), focus restore on dismiss | Verification only — every menu-bar action reachable, disabled reasons discoverable |
| 7 Transcript | Tool-call model, tool cards with collapsed/expanded output, streaming, queued follow-ups with edit, message copy, context meter, approval (`ask.rs`) and access (`access.rs`) states | Presentation polish: state legibility without colour, streaming that never re-lays out, inline actionable errors |
| 8 Empty/error | One shared `empty_state` with 18 callers; loading spinners; skeletons in 3 surfaces; dependency rows (`render_dependency_row`), install/onboarding paths | Actionable error copy — the shared primitive violates the epic's own rule; no unified error layout; empty states do not all offer their resolving action |
| 9 Perf/a11y | Per-palette contrast tests (AA floors), reduce-motion setting | Keyboard reachability on most surfaces, contrast verification for *muted* text on *selected rows* outside the tested pairs, baseline measurements (none recorded) |

---

## 6. Corrected plan for phases 2–9

The order stands: every dependency in the chain is real. The scope and the
estimates do not.

| Phase | Corrected scope | Current | Suggested |
| -- | -- | --: | --: |
| 2 Component states | Consume `InteractionState`; add a `Control`/`Row` style primitive so hover/press/focus/disabled are inherited; migrate 255 button sites + 20 popup shells + 20 row renderers; delete per-site colour decisions | 5 | 8 |
| 3 Motion | Adopt the shipped `Motion` roles across the 26 animation sites; retire decorative motion; verify Reduce Motion on every looping affordance | 5 | 5 |
| 4 Sidebar | Add the recent group and session search; tighten ordering rules; verify persisted sidebar state. Grouping, pins, menus already ship | 5 | 3 |
| 5 Workspace | Presets, custom layout, collapsed-state restore, min-size contract. Resize + dimension persistence already ship | 7 | 5 |
| 6 Palette | Verification and polish only | 5 | 2 |
| 7 Transcript | Presentation polish against the existing model. Do not rebuild the RPC mapping | 7 | 5 |
| 8 States | One error primitive with *what happened + what to do next*; migrate the three bespoke error cards; audit all 18 empty states for a resolving action | 5 | 3 |
| 9 Validation | Measure transcript scroll, panel resize, palette filter against a recorded baseline; keyboard sweep; contrast sweep | 5 | 5 |
| 40 Docs | Update `DESIGN.md` to the shipped vocabulary (§5.3 leak) and record the phase 1 inventory | 3 | 3 |

Two ordering notes:

* **Phase 2 must not start before this token vocabulary is agreed.** If
  `InteractionState` changes shape later, 255 call sites move twice.
* **Phase 3 can run in parallel with phase 2** — motion does not depend on the
  state table. The chain in Linear says 2 → 3; that is stricter than the code
  requires. Keeping it serial is fine, but it is a scheduling choice, not a
  technical one.

---

## 7. What phase 1 changed in code

`theme/tokens.rs` gained the vocabulary the audit found missing:

* `interaction::InteractionState` — rest, hover, pressed, selected, focused,
  disabled — with `.fill(theme)`, `.border(theme)`, `.row_ink(theme)`,
  `.control_ink(theme)`. The six answers are resolved from semantic roles once.
* `FOCUS_RING_WIDTH` — the 1px border-based ring the code already draws.
* `Motion` — named roles on top of the Zed port. `Instant` (50 ms) and
  `Interaction` (150 ms) resolve to `AnimationDuration::Instant` / `Fast`;
  `Layout` (200 ms) is Orbit's own value from the spec's 160–220 band. `easing()`
  is part of the role, and `duration(theme)` returns zero under Reduce Motion, so
  the guard is a property of the token rather than of each call site.

Nothing consumes either yet; `#![allow(dead_code)]` covers the gap until phases 2
and 3. The interaction module documents its two deliberate exceptions:
primary/action buttons resolve hover/press through `action_hover` /
`action_pressed`, and raised buttons apply a gradient *treatment* on top of the
fill, which is a component concern and not a colour token.

Tests added alongside: hover, pressed and selected never collide in any of the 12
palettes; only `Focused` carries the ring; disabled ink is the disabled role in
both row and control contexts; the motion roles sit inside the spec bands, ease
from 0 to 1 without overshoot, and collapse to zero under Reduce Motion.

`DESIGN.md` was corrected in the same change: the three shadow names that no
longer exist were replaced with the real `ElevationIndex` vocabulary, the
focus/selection model now matches the shipped `focus_ring` / `surface_selected`,
the motion section documents the named roles and the loop policy, and the doc
gained the sections it was missing (Linear alignment, theme anatomy, palette
matrix, geometry reference, verification).

Geometry was then unified on the Linear ramp: `Radius::px`, `TextSize::XSmall`,
`input::composer_radius`, `ElevationIndex::shadow` and `RaisedExt` no longer branch
on the palette family, so every palette draws 6px controls, 8px panels, a 6px
composer, 11px captions, flat button faces and one-layer popover/modal shadows.
`Theme.design_system` was removed with the branch it fed.

---

## 8. Open decisions

Resolved in this phase:

1. **Motion scale** — resolved as layered roles: keep Zed's `AnimationDuration`
   intact and put `Motion::Instant` / `Interaction` / `Layout` on top, with
   `Layout` at Orbit's 200 ms rather than Zed's 300 ms. Documented in
   `DESIGN.md` §Motion System and asserted by `motion_roles_fill_the_spec_bands`.
2. **Unsanctioned loops** — resolved as a policy, not a per-loop ruling: a loop is
   allowed when it reports live state (running session, in-flight work, arriving
   output); the backdrop dither is the only non-state loop. The four product loops
   therefore stay; what phase 3 retires is decorative motion, if any is found.
3. **`DESIGN.md` ownership** — fixed now rather than at OPI-40. The shadow names,
   focus/selection model, button scale and token list now match the code, and the
   missing sections were added.
4. **Full Linear geometry** — taken, not deferred. The radius ramp, 11px caption,
   composer radius, flat button face and restrained elevation are now the *only*
   geometry: they no longer branch on the palette family. `DesignSystem` is a palette
   family selector, and every one of the twelve palettes draws 6px controls, 8px
   panels, a 6px composer and one-layer popover/modal shadows. `Theme.design_system`
   was removed with the branch it fed. `one_linear_geometry_ramp_serves_every_palette`
   holds it across all twelve.

Still open, for the reviewer:

- **Phase 2 scope.** Adopting the state contract across 255 button sites, 20 popup
  shells and 20+ row renderers is the largest single change in the epic. It should
  start once this table is agreed, not before.

---

## 9. Phase 2 — complete

Two layers, one contract:

* `ControlExt` — `hover_state`, `hover_quiet`, `hover_bordered`,
  `hover_bordered_ink`, `hover_action`, `hover_danger`, `hover_danger_soft`,
  `hover_accent`, `hover_accent_wash(AccentWash)`, `hover_selected`,
  `hover_elevated`, `hover_overlay`, `hover_overlay_bordered`,
  `hover_overlay_strong`, `hover_on_glass`, `hover_border`, `hover_ink`,
  `hover_ink_quiet`, `hover_ink_accent`, `hover_inverse`, `hover_divider`,
  `hover_dimmed`, `hover_state_open(open)`, `focus_state`.
* `StatefulControlExt` — `pressed_state(PressedRamp)` (Wash / Selected / Action /
  Accent / Danger) and `disabled_state`.

**Zero call sites name an interaction colour.** `grep -rn '\.hover(|'` outside
`app/helpers.rs` returns nothing, and `grep -rn '\.active(|'` likewise. The 24
treatments above are the whole vocabulary; every remaining hover in the app is
one of them.

* 217 hover sites migrated (195 `hover_state` + 16 glass + 6 border, before the
  tail pass folded the last 78 into named treatments).
* 54 press sites with a declared ramp; 3 disabled sites.
* Focus centralized in `button_frame`, `icon_button_frame`, `picker_entry`.
* 19 anchored popovers (`anchored_popover`, `_in_window`, `_at`) and every modal
  on `modal_card` + `modal_scrim`.

**Enforced, not just done.** `scripts/check_states.py` fails the build when a
`.hover(…)`/`.active(…)` closure names a state token, with `app/helpers.rs` as
the one allowed file. That is the gate for "a component added later inherits
correct states by default".

## 10. Phase 3 — complete

Primitives, all in `theme/tokens.rs`:

* `Motion` — `Instant` 50 / `Interaction` 150 / `Layout` 200, each with its
  easing and the Reduce Motion guard inside `Motion::duration`.
* `looping` — the cadences for loops that report live state: `SPIN` 900,
  `SPIN_FAST` 700, `SHIMMER` 2000, `PULSE` 1600, `PROGRESS` 1100.
* `chart` — `DRAW` 900 / `MORPH` 500 / `REVEAL` 1100 and their symmetric cubic,
  deliberately outside the transition bands, with a test that keeps it there.

One duration is written by hand, by verdict: the toast's life bar runs
`Duration::from_secs_f32(ttl + fade)`, because it counts down the toast's own
lifetime. It is guarded like everything else.

**Enforced.** `scripts/check_motion.py` fails when an animation types its own
millisecond duration, so a new surface cannot reintroduce one.

**Every animated surface, and how its motion is bounded:**

| Surface | Motion | Reduce Motion |
| -- | -- | -- |
| Sidebar navigation | slide `Layout`; running-session shimmer | slide snaps; shimmer freezes on a frame |
| Buttons and icon buttons | none — states are instant | n/a |
| Tabs and active indicators | none — states are instant | n/a |
| Dropdowns and popovers | fade on `Interaction`; the access and workflow menus also travel 8 px out of the anchor edge | appears at once |
| Dialogs and modals | fade in on `Layout` (all 8 modals); no scale, because gpui has no box transform | the card appears at once |
| Toasts | enter on `Interaction`; the life bar runs the toast's own TTL | card is static; TTL still expires it |
| Expandable sections | chevron rotation on `Interaction` | rotation completes instantly |
| Command palette | none — it appears fully formed | n/a |
| Tool execution states | spinner (900ms loop), thought pulse (1600ms loop) | glyph stays, motion drops |
| Loading and progress | spinner, refresh glyph, updater bar (1100ms loop) | all three static; the bar parks its segment |
| Drag-and-drop | drop overlay fade on `Interaction` | overlay appears at once |
| Panel resizing | sidebar slide `Layout`; drags follow the pointer, never animate | slide snaps |
| Composer dock (new task → session) | the composer's column and the space under it travel on `Travel`, with the folder row's collapse and the transcript's fade on the same clock (`new_task::settle`) | the session frame is painted on the frame of the send |

Two durations stay outside the roles, by verdict rather than omission:

* **The toast's life bar** is a countdown over the toast's TTL — data, not a
  motion choice. It is guarded like everything else.
* **Chart reveals** use the `chart` module; a test asserts they are longer than
  every transition so the cadence cannot leak into chrome.

### Micro-interaction layer (shipping, not finished)

`DESIGN.md` §Motion System specifies the layer every interactive surface answers
through: hover and selection crossfades, a press ramp, a focus-ring fade, a
travelling switch knob, a segment that fades both states, a rotating disclosure
chevron, a travelling popover, a fading dialog, panel slides and the composer's
send morph.

Shipped since: the five primitives (`src/motion/`, 29 headless tests), the fourth
role `Travel` (260 ms, band-asserted), the three-way Reduce Motion preference,
`motion::interactive` (`hover`, `control`, `state`), `motion::enter`, and
`helpers::{switch, disclosure_chevron}`. Two rows of the spec cannot be built on
gpui 0.2.2 — a press has no transform to scale with, and an unreserved focus ring
would shift layout — so `DESIGN.md` states both limits instead of promising them.

Coverage today. "Specified" is the layer's contract, not a promise for this file:

| Coverage | Today | Specified |
| -- | --: | --: |
| `with_animation` sites in the tree | 26 | — |
| Of those, inside `src/motion/` (hover, control, state, enter) | 4 | every transition and entrance routes through the layer |
| Of those, loops (`looping::`) and chart cadences (`chart::`) | 7 + 3 | unchanged by the layer, by design |
| `hover_state(…)` call sites still swapping | 208 | fade through `ControlExt` |
| `pressed_state(…)` call sites | 53 | inherit the press ramp |
| Controls whose hover fades (`motion::hover`) | 2 helpers + 2 segments | every interactive surface |
| Anchored popovers entering through the chokepoint | 19 of 19 (16 travel) | every anchored surface |
| Modal cards entering through `motion::enter` | 8 of 8 | 8 of 8 |
| Switches travelling through `motion::state` | 3 of 3 | 3 of 3 |
| Segments fading hover *and* selection | 2 of 2 in Settings | every segmented control |
| Disclosure chevrons that rotate | every one — no glyph swap remains | all, via `helpers::disclosure_chevron` |
| Send button morph | 1 of 1 | 1 of 1 |
| Panel slides | 0 of 4 (explorer, Review, terminal, Git) | every panel |
| `Motion::Instant` uses | 0 | press and focus acknowledgement |
| `Motion::*::nominal()` outside `theme/tokens.rs` | 0 | enforced by `check_motion.py` |

The two `nominal()` call sites are fixed: the copy-section fade resolves its
duration through `Motion::duration(&theme)`, so Reduce Motion collapses it, and it
never mounts the animation element when the role resolves to zero — gpui divides
by the duration, and a zero-duration `Animation` is a NaN, not a still frame.
`wallpaper.rs` had the same `nominal()` and now resolves through the role too;
`check_motion.py` fails the build on any future call.

That fade is the shape the motion layer generalises: `SectionHover` samples the
opacity on screen at every flip and `hover.generation` rides in the element id
(`transcript_view.rs:4271, 4297`), so a reversal resumes from the painted value
instead of snapping. What it still lacks is a home outside one surface's state —
`motion::Transition` plus `motion::Transitions` is that home.

## 11. Phase 6 verification

The palette was already built; the phase's job was to prove its done-gate. A new
registry test, `every_chorded_command_has_a_route`, found that the gate as written
("every action reachable from a menu or a shortcut is also reachable from the palette")
does not hold literally, and cannot:

* **Context-local commands** (`help_only`) — the review tree's ↑/↓, the Git tab
  chords. A global palette row would offer an action that does nothing until a
  specific pane has focus.
* **The palette itself** — ⌘P/⌘K is `help_only`; a command cannot open the palette
  from inside the palette.
* **`OpenSessionSlot`** — one command whose chord carries the slot (⌘1–⌘9), so a
  row would have to pick a slot for the user.

The shipped reading is therefore "a palette row **or** a Shortcuts-page entry", and
the test asserts that for every chorded spec and every binding. Everything global is
in the palette; the three shapes above are in the help reference.

## 12. Phase 8 progress

Shipped:

* **`error_state`** — one card answering what failed, the underlying detail
  (wrapped, never truncated), and the next step (a button, or a hint sentence).
  `provider_error_card` and `mcp_notice_card` now delegate to it; `runtime_error`
  keeps the same two facts in a settings-row shape and no longer truncates.
* **Empty states now offer their action.** The sidebar's empty session list
  carries a primary "New session" button instead of only telling the user to
  pick a folder.

Remaining: the other 17 `empty_state` call sites are not audited for a
resolving action, and `error_state` has no automated copy check — nothing
prevents a future error from arriving with a title and no next step.

## 13. Phase 9 progress

`docs/performance-baseline.md` records what can be measured headlessly and what
cannot:

* Measured: palette resolve **171 ns**, layout restore **638 ns**, registry scan
  **0.3 µs** — the whole launch-time token path is under a microsecond.
* Unmeasured: transcript scroll, panel resize, palette filtering per keystroke,
  streaming frame budget. All four need a GPU harness that does not exist.
* Consequence: **no phase in this epic can yet claim a performance
  non-regression.** The correctness gates (contrast, state collision, motion
  bands) are enforced in CI.

Measured with `cargo test -p orbit-pi --bin orbit-pi baseline -- --ignored
--nocapture` (the test is ignored so CI does not gate on timing).

## 14. Phase 4 — complete

* **Session search** — one field under the section label; the query mirrors into
  `sidebar_query`, so the row builder and ⌘↑/↓ navigation read the same filtered
  list. Matches title, first message, workspace label and session id; results are
  one flat list; no-match names the query and offers Clear.
* **Recent view** — a third grouping beside Workspace and All sessions. It lists
  every pinned, open and running session (those are the ones being worked with),
  then the six most recent of the rest. Ordering is activity-desc with the path as
  tiebreak, so two sessions written in the same second cannot swap between frames.
  An empty result uses the sidebar's own empty state rather than a second one.
* **Keyboard route to the search field** — `⌘⇧F` (`FocusSessionSearch`) reveals the
  sidebar and focuses the field. `⌘F` stays transcript find, which is why the
  sidebar takes the shifted chord. The registry guard keeps it honest: a chorded
  command must have a palette row or a help entry.
* **Sidebar state persists** — collapsed groups, expanded "Show more" counts, the
  open panels and the sidebar's visibility all live in the layout store, so a
  restart reopens the list as it was left.
* **Context-aware entries** — the session menu's Delete is absent for the open,
  running or live session (`deletable`), and Pin/Archive read their own state for
  the label. Hover actions are `flex_none` and fade in, so revealing one never
  shifts a row's text.

Not done, and stated rather than implied: **scroll position** is not persisted —
the list reopens scrolled to the open session, which is the row that matters.

## 15. Phase 5 — complete

* **Resizable panels** with clamps, and **four sizes persisted** through a
  debounced store (`layout.json`), with per-field tolerance: a missing key keeps
  its default, an unknown key is ignored, a bad type or out-of-range value is
  dropped.
* **Three presets** (Focus / Developer / Review) that set sizes and open panels.
  A preset is a **starting point, not a mode**: applying one saves the user's own
  arrangement first, and **Back to custom** restores it. Touching a panel ends the
  preset, so nothing stale can be restored.
* **Named layouts** — save the current arrangement under a name, apply it, delete
  it. A named layout is a snapshot, not a live binding: later edits do not rewrite
  it, and applying it leaves the `custom` slot alone. Capped at eight, blank names
  refused, duplicates replace.
* **The arrangement survives a restart**, not just the sizes: open panels, sidebar
  visibility, collapsed groups and expanded "Show more" counts. The restore runs on
  the first frame (opening the terminal needs a window), and panel open/close is
  recorded by observers on the three dock entities.
* **Responsive rule** — below 880px the sidebar yields whatever the toggle says, so
  the workspace keeps a usable column; ⌘⇧B still brings the list back. Extracted as
  a pure `sidebar_shows` and tested.
* **Minimum-size contract** — `the_store_range_brackets_every_panel_clamp` asserts
  the store's accepted range contains every panel's own clamp, so a stored layout
  can always be rendered and dragged back to a usable size.

Done-gate: *compose a three-pane layout, quit, relaunch, land exactly where you
left off — including collapsed panels and split sizes* — satisfied.

## 16. Phase 7 — audit

Phase 7 is presentation polish on a surface whose behaviour already ships. The audit
below separates the two: what exists and is verified by code, and what needs eyes on
a running app.

| Scope item | State | Evidence |
| -- | -- | -- |
| User / assistant message hierarchy | Shipped | distinct step kinds; assistant text on its own role (`assistant_text`) |
| Tool execution cards | Shipped | `render_activity_card` with collapsed/expanded forms, args + captured output, error strip, diff stats |
| Agent activity states | Shipped | `SessionStatus` → `StatusLane` → glyph + words (DESIGN.md §State Model) |
| Streaming | Shipped | append-only steps, `MessageScrollerState` keeps the reader's position, composer stays put |
| Queued follow-ups | Shipped | visible as an inline row, editable in place, cancellable (`view.queue_cancel`); reordering is not implemented — the epic allows "reorderable **or** cancellable" |
| Message actions | Shipped | per-message copy, `CopyLastResponse` command, copy on tool cards |
| Context usage | Shipped | `context_meter` ring + popover |
| Error and approval states | Shipped | approvals inline (`dialog.rs`, `ask.rs`); errors inline via `error_banner`, now on `error_state` |

Changed in this pass:

* **The run-failure banner now renders through `error_state`.** It was the last
  hand-built error card in the app — its own error wash, border, icon and spacing.
  It now carries the same three facts as every other error (what failed, the detail,
  the next step: "Retry what failed, or copy the message to report it.") with
  reconnect / copy / dismiss as its action row.
* **The agent-activity state machine is documented** against the shipped
  `SessionStatus` rather than the nine aspirational states the doc had before, with
  the lane, glyph and words each state resolves to.

Not done, and only because it cannot be judged without the app running:

* Whether a long streaming session *feels* smooth (the append path is verified; the
  frame cost is not — see Phase 9's missing GPU harness).
* Whether each state is *visually* distinct at a glance (each has a glyph and, where
  it matters, a word — but the spacing and weight are a judgement call).
* Tool-card density: collapsed vs expanded proportions, and where the output's own
  scroll region should end.

## 17. Phase 8 — complete

| State | Where | Resolving action |
| -- | -- | -- |
| Empty workspace | `explorer/panel` empty state | the folder button in the pane header |
| Empty sessions | sidebar | **New session** button in the empty state |
| Empty Git changes | Git panel's clean tree | none needed — a clean tree is the success state |
| Loading | skeletons in usage/gh templates, spinner elsewhere | n/a, never shifts layout |
| Failed operations | `error_state` card (provider, MCP, run failure) | retry / reconnect where one exists |
| Runtime errors | the inline banner above the composer | reconnect, copy, dismiss + a next-step hint |
| Missing dependencies | `render_dependency_row`, install/onboarding | the install step, linked |
| Permission requests | `access.rs` / `ask.rs` cards | Allow / Deny inline |

Rules that came out of the audit:

* **One error layout.** `error_state` is the card: what failed, the detail (wrapped,
  never truncated), and the next step. The run-failure banner was the last
  hand-built card and now uses it.
* **Inline errors are allowed where the action is adjacent** — a validation line
  under a field, a provider row's error beside its Remove button, a dialog's error
  above its actions. A string with no adjacent action is the card. This is now a
  stated rule rather than a silent exception, and it is why the twelve remaining
  inline error strings are not converted: each one sits beside the control that
  resolves it.
* **Empty states offer their action** when one exists in scope. "Select a file" and
  "clean tree" do not get a button: the tree is the control, and a clean tree is the
  success state.

## 18. Phase 9 — validation

**Measured (headless, reproducible):** palette resolve 171 ns, layout restore
638 ns, registry scan 0.3 µs. The whole launch-time token path is under a
microsecond.

**Contrast — audited, and fixed.** `metadata_stays_readable_on_every_row_state`
walks every palette and checks muted ink on hover, active and selected rows, plus
assistant ink on the canvas and code ink on the code plane. It failed 20 of 36
combinations: muted ink was chosen against the canvas and never re-checked against
the washes a row can sit on. Worst cases: Flexoki on active **3.30:1**, Orbit Azure
on selected **3.65:1**, Orbit Azure on active **3.79:1**.

Eleven palettes had `text_muted` raised by the smallest step that clears 4.5:1 on
their worst row wash (Operational already passed):

| Palette | muted before → after |
| -- | -- |
| Orbit | `#8A8A8A` → `#919191` |
| Orbit Darker | `#8C8C8C` → `#909090` |
| Orbit Cobalt | `#8E8E8E` → `#919191` |
| Orbit Azure | `#909090` → `#A1A1A1` |
| Flexoki | `#8C8C8C` → `#A6A6A2` |
| Orbit Light | `#696969` → `#646464` |
| Orbit Azure Light | `#696969` → `#646464` |
| Orbit Paper | `#686870` → `#606068` |
| Orbit Contrast | `#666666` → `#606060` |
| Flexoki Light | `#66645E` → `#595752` |
| Operational Light | `#6A6E66` → `#636760` |

**Motion — verified on all twelve surfaces** (see §10): every animated surface
uses a primitive and every loop is Reduce Motion guarded.

**Still open, and blocking the done-gate:**

* **Rendering is now measurable, not yet measured.** `src/perf.rs` (debug builds
  only) samples the interval between frames the app actually produced; ⌘⌥P starts
  and stops a capture, the summary lands in `~/.orbit-pi/perf.jsonl`, and
  `scripts/read-perf.py` prints it. It samples rather than forcing frames, drops
  idle gaps, and refuses to report an empty capture. A capture has not been taken
  yet, so no phase can claim a performance non-regression — but the instrument
  exists, and the method is written down in `docs/performance-baseline.md`.
* **First capture taken** (2026-11-01, 60 Hz display): 577 frames over 10 s —
  mean 17.3 ms, p50 16.7 (vsync), p95 21.0, p99 30.2, max 226.5 (first paint).
  Run at 60 Hz pacing with ~5 % of frames over the 16.67 ms budget. It includes
  startup, so it is a baseline rather than a scroll-only result; the sampler now
  drops the first second of a launch-started capture.
* **Scroll baseline recorded** (22:03, chord-started, 30.9 s): 1805 frames —
  mean 17.0, p50 16.7, p95 20.7, p99 22.0, max 56.7. The 226 ms stall was startup.
  What remains is a **structural tail**: p95 agrees with the launch capture to
  0.3 ms across two different runs, so ~5 % of frames consistently take 24 % over
  the 60 Hz budget. That is the number to improve, and the one future changes are
  compared against.
* ~~The OS Reduce Motion setting is ignored.~~ **Closed.** `platform::os_reduce_motion()`
  reads `NSWorkspace.accessibilityDisplayShouldReduceMotion` (the `NSAccessibility`
  feature was added to `objc2-app-kit`), `theme` caches it and refreshes it at launch
  and on window activation, and every surface reads `theme.reduce_motion()` — the OR
  of the two sources. A test asserts the OS half suppresses `Motion::duration` on its
  own. Linux and Windows have no equivalent read.
* **No automated keyboard sweep** across the twelve surfaces.

## 19. Phase 9 — the keyboard sweep

Five mechanical checks now run over the whole app. Together they are the part of
"every keyboard surface is verified" that can be proven without a person at the
keyboard.

| Check | Where | Result |
| -- | -- | -- |
| Every advertised chord parses | `commands::tests::every_binding_chord_parses` | pass |
| No two bindings share a chord in one context (a shadowed command) | `commands::tests::no_two_bindings_share_a_chord_in_one_context` | pass, 26 bindings |
| Every binding is scoped to a key context some element declares | `scripts/check_keyboard.py` | pass — 28 contexts declared, 2 used by bindings |
| Every chorded command has a palette row or a Shortcuts entry | `commands::tests::every_chorded_command_has_a_route` | pass, with three documented exceptions |
| Focus is inherited, not re-invented | `button_frame` / `icon_button_frame` / `picker_entry` apply `focus_state` | every button and picker row |

The command registry covers **59 commands** across seven categories — Session 11,
Agent 10, Navigation 10, Review 10, Panels 8, Git 6, Application 4 — and every one
is reachable from the palette (or, for the three documented shapes, from the help
reference).

**What the sweep cannot prove**, and why it is listed rather than claimed:

* **That a given surface is usable without a pointer.** That needs a person
  tabbing through the app; the checks above prove the routes exist and none is
  shadowed, not that focus lands where a reader expects.
* **That every pointer-only control has a keyboard counterpart.** The composer's
  chips are the known case: each has a `command()` route, and a test asserts the
  agent controls are reachable without a pointer
  (`composer_controls_are_reachable_without_a_pointer`), but no test enumerates
  every mouse handler in the app and demands a counterpart.

## Appendix — reproduction

```sh
cd crates/orbit-pi/src

grep -rn '\.hover(|'  --include=*.rs . | wc -l    # 289
grep -rn '\.active(|' --include=*.rs . | wc -l    # 53
grep -rn '\.focus(|'  --include=*.rs . | wc -l    # 8
grep -rn 'focus_ring' --include=*.rs . | wc -l    # 32
grep -rn 'Animation::new(' --include=*.rs . | wc -l                  # 26
grep -rn 'AnimationDuration' --include=*.rs . | grep -v tokens.rs    # 2
grep -rn 'reduce_motion' --include=*.rs . | wc -l                    # 58
# of which 8 branch a render path on it; 7 animation sites are guarded
grep -rn 'button_frame(' --include=*.rs . | grep -v 'fn ' | wc -l    # 182
grep -rn '0x[0-9a-fA-F]\{6\}' --include=*.rs . | wc -l               # 639

cd /Users/rajeshwar/Personal/orbit
cargo test -p orbit-pi --bin orbit-pi theme    # 64 passed
```
