# Plan — Micro-interactions

**Status:** **Everything but P3b's rollout, P4e, and the two perf captures is complete.** The primitives (29 headless
tests), `Motion::Travel`, the three-way `MotionPreference`, and the `nominal()` fixes are
in. `motion::interactive` fades a hover's fill and ink on `Interaction`, keyed by the
transition registry, with `Hover::row` / `wash` / `fill` naming the pair; it is adopted on
the composer's send button, every Settings → Runtime action button, and the two segmented
controls (composer delivery mode, Reduce Motion). **A press cannot scale and an
unreserved focus ring cannot fade on gpui 0.2.2** — `DESIGN.md` §The interaction layer
records both limits. 208 `hover_state` call sites remain — the count grew from 202 while
this work was in flight, because the concurrent subagents feature added six — and ~190 of
them continue chaining, so the styling call has to move last before they can adopt. **P4c is complete** — all 8
modal cards fade in on `Layout` — and **P4b is complete**: all 19 anchored popovers enter
through the chokepoint, 16 of them with travel, and the geometry tests that measure a
popup's resting place now ask for the settled frame explicitly. **P4f is complete**: the
send button morphs between a square and a labelled button through one wrapper with two
roles. P0's two `⌘⌥P` captures are still pending because they need a running build; P4e
and the rest of P3b are open.

**What is left, precisely.** P3b: 208 `hover_state` sites, each needing the motion call
moved to the end of its chain (≈190 of them keep chaining) — mechanical, per site, and a
candidate for the `ParentElement`-forwarding wrapper described in §Mechanisms. P4e: the
four panels need a persistent mount, an out-of-entity width and a generation counter —
an `App`-state change rather than a call-site edit. P0/P5: the two `⌘⌥P` captures, which
need a running build.

## Status at a glance

| Phase | What ships | Status | Gate |
|---|---|---|---|
| P0 | Baseline capture and the motion test harness | Harness in; captures pending | Two `⌘⌥P` captures stored beside `docs/performance-baseline.md` |
| P1 | `src/motion/` primitives (`Transition`, `Spring`, `Disclosure`, `Travel`, `bezier`) | Complete | `cargo test -p orbit-pi motion::` — 29 tests, headless |
| P2 | `Motion::Travel`, three-way `MotionPreference`, the `nominal()` leak | Complete | Preference resolves in one place; `check_motion.py` forbids `nominal()` |
| P3 | The interaction layer, adopted control by control | P3a complete (hover, `control`, `state`); P3b is a per-site rollout — 208 `hover_state` sites | `hover_fades_*`, `a_control_tracks_hover_and_selection_separately` pass |
| P4a | One `disclosure_chevron`, adopted by every swap site | Complete — 0 glyph swaps left | No `chevron-down`/`chevron-right` ternary remains outside the helper |
| P4b | Popover entrance through `anchored_popover*` | Complete — all 19 sites enter through the chokepoint | Every anchored surface fades; the 16 with an anchor edge also travel |
| P4c | Dialog entrance composed around `modal_card` | Complete: all 8 modals fade on `Layout` | Callers apply the entrance; `modal_card` supplies the frame. The scrim is unchanged, and no scale is possible |
| P4d | Switch knob, segment selection | Complete | Knob travel is 18px at default settings and follows resolved padding; a segment fades both states |
| P4e | Panel slides (explorer, terminal, Git, side pane) | Not started | Every panel eases its own width |
| P4f | Composer send morph | Complete | Idle with a payload the button names Send; running it folds to a square |
| P5 | Guards, CI job, DESIGN/audit status flip, perf re-capture | Guards and the CI job done; perf capture pending a running build | `design-guards` job runs on every push and gates `build` |

Estimates: P1 2d · P2 1.5d · P3 3d · P4a 0.5d · P4b 1d · P4c 1d · P4d 1d · P4e 2d ·
P4f 1d · P5 1d. P0 → P1 → P2 are sequential. P4b and P4c need only P1 and can run in
parallel with P2/P3; P4a, P4d and P4f need P2's roles; P4e needs P1's `Spring`; P5 needs
everything.

**P1 progress.** `motion/{ease,transition,spring,reveal,registry}.rs` plus `mod.rs`.
`ease` owns the curves (`linear`, `ease_out_quint`, `bezier`) and `theme::tokens::chart`
now calls it, so a chart and a transition share one curve builder. `Transition` carries
`from`/`to`/`started`/`duration`/`ease`/`generation` and `role(…)` resolves duration and
easing — including the Reduce Motion guard — from a `Motion`. `Spring` is the closed-form
critically damped move with `retarget` preserving velocity. `Disclosure` measures its
child at the current width and clips to the revealed height. `Travel` is the entrance
offset out of an anchor corner. `Transitions` is the thread-local per-element store with
`sweep`. `cargo build`, `cargo clippy -p orbit-pi --all-targets` (only the pre-existing
`chunks_exact` warning), the motion, chart, token, motion-check, interaction-state and
keyboard guards all pass. No surface consumes the primitives yet, which is why `mod.rs`
allows dead code the way `theme::tokens` does.

**P2 progress.** `Motion::Travel` (260 ms, 220–320) joins `ALL`; the band test and the
chart-cadence guard both cover it. `MotionPreference` lives in `theme.rs` with
`suppresses_motion(os_asks)` as the single resolution point, `UiPrefs.motion` replaces the
bool, `from_value` migrates a legacy file, and `to_value` writes both keys for one
release. Settings → Layout shows a three-segment control plus a status line naming what is
in force. `transcript_view.rs` and `wallpaper.rs` resolve through `Motion::duration`, and
the copy-section fade skips the animation element when the role resolves to zero — a
zero-duration `Animation` divides 0 by 0 inside gpui. `check_motion.py` now fails on a
typed millisecond duration, a duration laundered through a binding, a hand-timed
`from_secs` outside the toast life bar, and any `.nominal()` outside
`theme/tokens.rs`; it caught `wallpaper.rs` on the first run. Seven new settings strings
fall back to English in the other nine locales.

**P3a progress.** `motion/interactive.rs` (new), `Transition::easing`, and the send
button's adoption. `check_motion.py` grew a fourth rule during the work — an
`Animation::new(<binding>)` anywhere in a line, not just at the end of one — and it
immediately caught the copy-section fade's `Animation::new(copy_duration)`, whose role is
now named at the animation site (`Interaction` entering, `Layout` leaving). 25 motion
tests pass, the full suite is green, and the four guards are clean.

## Why this exists

Measured on the current tree:

* 26 `with_animation` sites in a 90k-line crate, and **0** of them on hover, press,
  focus or selection.
* 216 `hover_state(…)`, 54 `pressed_state(…)`, 196 `button_frame(…)`, 79
  `icon_button_frame(…)`, 59 `picker_entry(…)`, 45 `press(…)` — all instant.
* `Motion::Instant` — the role `DESIGN.md` assigns to press and focus — has **0** uses
  outside token tests.
* Reduce Motion is **off** on the reference machine (`~/.orbit-pi/ui.json`
  `"reduce_motion": false`, macOS `ReduceMotionEnabled = 0`). The flat feel is the
  shipped design, not the guard.

The failure mode this plan fixes is not "too little motion". It is that a state change
and no state change look the same: a hover is indistinguishable from a repaint, and a
press is indistinguishable from a click that did nothing.

## Goal and non-goals

**Goal.** Every interaction the pointer or keyboard can produce resolves to a role in
the interaction layer, inherits its transition from a shared primitive, and degrades to
the instant end state under Reduce Motion.

**Non-goals, restated as rules** (`DESIGN.md` §Motion Rules):

* No entry animation on the transcript. Streaming is the motion there.
* No decorative loops, no parallax, no overshoot, no stagger-in on existing content.
* No animation that gates an action or delays a state commit.
* No per-call-site duration, easing, or threshold.

## Mechanisms

`with_animation` stores its clock in `window.element_states`, keyed by the
**`GlobalElementId`**. Changing the id mid-interaction restarts `delta` at 0, and keeping
the id freezes `delta` at 1 after the first run. Both are wrong for a reversible
transition, so the layer uses two mechanisms and picks one per surface group.

**A — rebuild per frame.** The element id carries a generation; the `from` value is
snapshotted into a `Cell` at retarget time; the animator interpolates
`from + (to - from) * eased(delta)`. Fill, ink, border and transform all animate because
the element is rebuilt each frame. Cost: the call is **terminal** — it returns
`AnyElement`, so it must be the last call on that element. This is the pattern already
shipped in `app/updater_ui.rs:380-400` (the updater pill) and the shape
`SectionHover` (`transcript_view.rs:168-215`) already does.

**B — paint-time.** A custom `Element` owns its hitbox, reads
`Hitbox::is_hovered(window)` in prepaint, keeps its `Transition` in `with_element_state`,
and paints an interpolated wash into its own rounded rect. No call-site reordering, no
element rebuild. Cost: only a **fill** can animate this way — a label or glyph colour is
baked before prepaint, so ink stays instant.

| Surface group | Mechanism | Reason |
|---|---|---|
| Buttons, icon buttons, chips, segments | A | scale + ramp + ink together |
| Popovers, dialogs | A | whole-card fade and travel |
| Toggles, disclosure | A | a knob and a rotation are geometry |
| Panels | A | the animated property is the panel's own width |
| Send morph | A | width, label and glyph change together |
| Picker rows, menu entries, sidebar rows | A, with B as fallback | ink lift on hover is part of the row's read |

Call-site shape today: of 237 `button_frame` / `icon_button_frame` / `press` /
`context_menu_entry` sites, roughly 87 continue chaining (`.child`, `.children`,
`.on_mouse_up`). Those need their motion call moved to the end. That is the only
mechanical refactor in this plan, and it is why adoption is staged per surface group
rather than by a global rewrite.

## Primitives — `crates/orbit-pi/src/motion/`

| File | Type | Contract |
|---|---|---|
| `ease.rs` | `bezier(t, x1, y1, x2, y2)` | Moved out of `theme::tokens::chart`; `chart::ease` and `Motion::easing` both resolve through it. Clamped to 0..1 (gpui `debug_assert`s the delta range). |
| `transition.rs` | `Transition { from, to, started, duration, generation }` | `retarget(to, now)` samples the value on screen and bumps `generation`. `at(now) -> (f32, bool)`. |
| `spring.rs` | `Spring { from, to, velocity, frequency, … }` | Critically damped: reaches the target without passing it, settles exact inside 0.2px, preserves velocity across `retarget`. |
| `reveal.rs` | `Disclosure`, `Travel` | `Disclosure` measures its child at the current width and eases height. `Travel` returns the offset for a card entering from a `Corner`. |
| `registry.rs` | `Transitions` | `thread_local` map from `ElementId` to `Transition`, pruned on settle. GPUI is single-threaded per window; this matches how the theme already caches `OS_REDUCE_MOTION`. |
| `mod.rs` | re-exports | The layer's only public surface. |

Every primitive exposes its curve as a pure function, so the shape is asserted without a
window (the test shape `pane_motion.rs` uses upstream). Required properties: monotonic
between endpoints, bounded by the endpoints, exact settle, and identical samples at 60,
90, 120 and 144 Hz.

## Phases

### P0 — Baseline and harness (0.5d)

* Capture two `⌘⌥P` runs (`scripts/read-perf.py`) — one idle, one with a long session
  scrolled — and append them to `docs/performance-baseline.md` so every later phase has
  a "before" to attribute against. Today's numbers: p50 16.7, p95 20.7, p99 22.0 on a
  16.67 ms budget.
* Add `crates/orbit-pi/src/motion/` with `ease.rs` only, plus the module test scaffold.

**Gate:** `cargo test -p orbit-pi motion::` runs; baselines recorded.

### P1 — Primitives (2d)

* `transition.rs`, `spring.rs`, `reveal.rs`, `registry.rs`.
* `theme/tokens.rs`: `chart::cubic_bezier` and `bezier` move to `motion::ease`;
  `chart::ease` calls it. No behaviour change.
* Tests:
  * `a_transition_restarts_from_the_painted_value` — retarget at 40 % and assert the
    first sample equals the last painted value, not `from`.
  * `a_spring_settles_without_overshoot_at_any_refresh_rate` — the `pane_motion` matrix:
    60/90/120/144 Hz × three distances × opening/closing.
  * `a_spring_reversal_keeps_position_and_velocity`.
  * `a_disclosure_measures_at_the_current_width` — a width change between construction
    and layout must not produce an estimated height.
  * `travel_offsets_point_out_of_the_anchor_edge` — all four `Corner`s.

**Gate:** primitives green, clippy clean, no UI change yet.

### P2 — Tokens, preference, and the live defect (1.5d)

* `theme/tokens.rs`: add `Motion::Travel` (260 ms, `ease_out_quint`), extend `ALL` and
  `motion_roles_fill_the_spec_bands` to the 220–320 band; assert
  `chart::*` still exceeds every role including `Travel`.
* `theme.rs`: `MotionPreference { System, Reduced, Full }` on `UiPrefs` (replacing the
  `reduce_motion: bool`), serialised as `"system" | "reduced" | "full"`.
  `from_value` maps a legacy `reduce_motion: true` to `Reduced`; `to_value` writes the new
  key and keeps `reduce_motion` as a derived bool for one release so a downgrade does not
  lose the setting. `Theme::reduce_motion()` keeps its signature and its 30-odd callers,
  and resolves the three-way preference against `OS_REDUCE_MOTION`; only the body and
  `theme::reduce_motion(cx)` change.
* `app/settings.rs:6379-6393`: the Reduce Motion row becomes a three-way control, with a
  status line naming the source ("Following macOS" / "Reduced" / "Full"). New strings go
  through `en.yml` and `scripts/gen_locales.py`.
* Fix the defect: `transcript_view.rs:97-98` use `Motion::*::nominal()`, so the
  copy-section hover fade ignores Reduce Motion. Resolve through `Motion::duration` and
  skip the fade entirely when suppressed.
* `scripts/check_motion.py`: fail when `.nominal()` appears outside `theme/tokens.rs`;
  catch durations passed through a binding (`Animation::new(copy_duration)`), and
  allow-list the toast life bar by name instead of by accident.

**Gate:** `the_motion_preference_resolves_in_one_place`, `check_motion.py`,
`check_i18n.py`, and `cargo test -p orbit-pi i18n` pass.

### P3 — Interaction layer core (3d)

**P3a, shipped.** `motion/interactive.rs`:

* `Hover` — the pair of states a control fades between, with `row`, `control`, `action`
  and `ink_only` constructors. Naming the pair is what lets a filled action button fade
  `action` → `action_hover` without the layer assuming every hover is a wash.
* `hover(element, name, theme, Hover)` — installs the `on_hover` listener, writes the
  target into `Transitions`, keys the element id on the transition's generation, and
  reads the value back in the animator. Under Reduce Motion it applies the same
  treatment through `.hover()`, so the state still changes, it just does not fade.
* The animator ignores gpui's delta and reads `Transition` instead, so the easing lives
  in one place and a reversal resumes from the painted value.
* Adopted on the composer's send button (`app/view.rs::send_button`), every Runtime
  action button (`app/settings.rs::runtime_button`, one helper covering Start / Stop /
  Restart), and the composer-delivery and Reduce Motion segmented controls. Those four
  cover three different shapes: a filled action, a raised secondary, and an unselected
  segment.
* `Hover` names the pair, not the surface: `row(theme)` (wash + ink lift), `wash(theme)`
  (wash, caller's ink), `fill(rest, hover)` (two fills cross-fade). `ink` is optional so
  the layer never overwrites a colour the control set for itself.

**Why P3b is blocked, not deferred.** Two rows of the interaction table cannot be built
on gpui 0.2.2's public surface:

* **Press scale.** `with_animation`'s animator must return the same element type, and
  the only scale primitive (`with_transformation`) is an `Svg` method — `Style` has no
  transform field and `Window` exposes no transform pass. A press is a ramp, not a
  scale, until gpui grows one.
* **Focus-ring fade.** A ring that is absent at rest adds a 1px border when it appears,
  moving every sibling. The fade needs the control to reserve the border first, which is
  a per-surface decision, not a layer default.

**P3b, remaining.** Adopt `hover` across the surfaces that have no competing rest fill:
settings rows, picker rows, the palette, sidebar rows, menu entries. Terminal-return
call sites adopt in one edit; the ~87 chained sites move their styling call last. The
copy-section fade keeps its own `SectionHover` for now — it is correct after P2 and the
file is under concurrent work — but the role is named at the animation site.

**Gate:** `hover_fades_from_the_painted_value`, `hovering_a_control_starts_one_transition`,
`reduce_motion_swaps_the_hover_instead_of_fading_it`; `check_motion.py` and
`check_states.py` stay green; a manual pass on the adopted control.

### P4a — Disclosure chevrons (0.5d)

**Shipped.** `helpers::disclosure_chevron(id, open, size, color, theme)` draws
`chevron-right` and turns it a quarter turn on `Interaction`; Reduce Motion turns it
instantly. One glyph rotates instead of two glyphs swapping, because a swap cannot be
animated — the eye reads a jump where the shape should carry through. It is built on
`motion::state`, so the rotation interpolates from whatever angle is on screen.

Adopted by `chip_caret` (which had its own bespoke rotation, now one implementation, and
every chip in the app routes through it), the explorer's file tree, and the transcript's
summary card.

**All ten remaining carets adopted**, each with a scoped id built from the identity it
already had: activity groups (`ix`, key), thought cards (key), tool cards, tool sections
(`section`), edit diffs, turn folds (`ix`), code blocks (`block_ix`), mermaid blocks, and
the changed-files list (`message_ix`). `transcript_view.rs` no longer contains a single
`chevron-down.svg`/`chevron-right.svg` literal.

One `disclosure_chevron(id, open, theme)` in `app/helpers.rs`, generalising
`app/view.rs:1887-1933` (`chip_caret`, the access popup's rotating chevron). Adopt at the swap sites:
`transcript_view.rs:2045, 2855, 3113, 3793, 4423, 4785, 5428, 7446, 8258`,
`explorer/panel.rs:1131`. The `git_panel` sites (2774, 3607, 3902, 5370, 7963) tint the
chevron by open state without swapping glyphs; they take the colour fade on
`Interaction` instead.

**Gate:** no `chevron-down`/`chevron-right` ternary remains outside the helper.

### P4b — Popovers (1d)

**Shipped.** The entrance moved into the chokepoint: `anchored_popover`,
`anchored_popover_at` and `anchored_popover_in_window` now take an `id` and a `theme` and
wrap the popup themselves, so a menu added tomorrow inherits its entrance instead of
opting in. All 19 call sites pass a stable id (the insertion was scripted, then the
compiler drove the three call sites whose scope had no `theme` — the two picker popups
and `chip_popup` — to take one).

`anchored_popover_at` fades only: a context menu at the pointer has no anchor edge to
travel from. The two menu sites that already wrapped their popup by hand lost that
wrapper, so nothing double-fades.

**Test fallout, and the fix.** Three geometry tests measure where a popup *rests*, and
the entrance starts it 8px toward its anchor, so they asserted a first frame that is now
mid-entrance. They now run with Reduce Motion on — which is the supported way to ask for
the settled frame — with a comment saying why.

Move the entrance **into the chokepoint**: `anchored_popover`, `anchored_popover_at` and
`anchored_popover_in_window` (`helpers.rs:577-620`) apply `popover_enter` themselves, so
`popover_enter` stops being something a call site opts into. Extend the entrance to
fade + 8px travel out of the anchor's resolved side (currently fade only,
`helpers.rs:622-637`). Dismissal stays instant.

Sites that gain it without edits: `session.rs:1981, 2367`, `composer_ops.rs:455`,
`sidebar.rs:2521`, `pickers.rs:876, 893`, `worktrees.rs:4034`, `open_in.rs:322`,
`settings.rs:7858`, `view.rs:2060, 2248, 2413, 2431, 3899`, `explorer/panel.rs:1414`,
`transcript_view.rs:1085`, `git_panel/mod.rs:2988, 3986, 4083`.

**Gate:** `popovers_enter_through_the_chokepoint`; a popup opened from a keyboard route
(no pointer) travels too.

### P4c — Dialogs (1d)

**Shipped.** Every modal card enters through `motion::enter` on `Layout`: `dialog.rs`,
`custom_ui.rs`, the updater dialog, the worktree dialog, the three Settings provider
modals, and the MCP editor. gpui drops an element's animation state when it leaves the
tree, so a closed dialog and the one that replaces it each get their own entrance — no
registry, no mount counter.

**No scale.** The row used to promise `0.985 → 1`; gpui 0.2.2 has no box transform, so
the card fades and `DESIGN.md` says so. The scrim is unchanged — fading it as well would
need the same entrance plumbed into every hand-rolled scrim.

`modal_card` (`helpers.rs:645`) gains the entrance: scale 0.985 → 1 and fade on `Layout`,
`modal_scrim` (`helpers.rs:659`) fades on `Interaction`. Call sites: `dialog.rs:332`,
`app/settings.rs:3911, 4115, 4380`, `app/mcp_ui.rs:2360`, `app/worktrees.rs:2307`,
`app/updater_ui.rs:586`, `custom_ui.rs:453`. Focus lands after the entrance starts, never
on its completion.

**Gate:** `modal_card` entrance test; Escape during the entrance still dismisses.

### P4d — Toggles and segments (1d)

**Switches shipped.** `motion::state(element, name, theme, role, target, apply)` is the
state-driven sibling of `hover`: the caller passes the value it wants now and the layer
fades from what is on screen, restarting the animation by generation. All three switches
now render through one `helpers::switch` — Reduce Motion, automatic updates, and each
skill's enablement — so the shape (`36×20` track, `14px` knob), the travel, and the
motion cannot drift between them. The knob moves on `left` (a relative offset, so the
track does not re-lay out and the click's hitbox stays on the track), and the track
crossfades `surface_elevated` → `accent` on the same transition. `switch_travel` is a
pure function with a test over every spacing density, because the padding scales with
the density setting and a wrong span would put the knob through the track's edge.

The skill toggle also gained a unique element id (`skill-toggle-<file>`): every row
shared the literal `skill-toggle` before, which collides as a gpui id and would have
made every skill share one transition.

**Segments shipped.** The selected half used to swap, because a hover wash painted over
a selected wash is not a blend of the two — and `hover` and `state` each return
`AnyElement`, which is not `Styled`, so they cannot nest. `motion::control` owns both
transitions instead: the hover comes from its listener, the selection from the caller's
value, and the animator gets both numbers and cross-fades
`surface_elevated → surface_hover → surface_active` in one pass. The composer delivery
mode and the Reduce Motion selector both use it. Reduce Motion still repaints — the
listener refreshes the window — it just resolves both values instantly, so the state
changes and the motion does not.

* `settings.rs:5807` — the knob positions with `justify_end`/`justify_start` today; the
  knob element gets a `Transition` on `Layout` and the track crossfades its fill.
* `segmented_segment` (`helpers.rs:978`) — the selected fill and the label ink move on
  `Interaction`. Where a continuous track exists (`settings.rs:5718, 5770, 6825, 6960`,
  `session.rs:2221`, `bug_report_ui.rs:214`) the indicator travels rather than swapping.

**Gate:** Reduce Motion renders the end state; both controls keep their keyboard path.

### P4e — Panels (2d)

Generalise the sidebar's slide (`app/view.rs:961-990, 1057-1075, 1339-1360`, keyed by
`sidebar_slide_gen`, `app.rs:360`) into `panel_slide(gen, open, from, to, theme)`, backed
by `Spring` for a width change and by `Layout` where the panel is not resizable. Apply to
the explorer (`explorer/panel.rs`), the side pane (`sidepane.rs`), the terminal
(`terminal.rs`) and the Git panel (`git_panel/mod.rs`). Reuse the existing generation
contract: increments at `session.rs:2755`, `sidebar.rs:3482, 3706`.

**Gate:** a panel reopened mid-slide resumes from the painted width; no panel animates on
launch (`gen == 0` renders settled).

### P4f — Composer send morph (1d)

**Shipped.** `motion::control` grew a second role: `hover_role` and `state_role`. The
send button uses `Interaction` for its fill fade and `Travel` for the morph, and one
wrapper drives both — the fill, the width and the label's opacity all come from the same
frame. Idle with something to send, it is a labelled button ("Send"); while a turn runs
it folds back to a square, because the glyph (queue or steer) is the whole message then.
The pump window is the slower of the two roles, or a frame would freeze mid-fade.

Two strings already existed (`composer.queue`, `composer.steer`), so no new keys.

`app/view.rs:4213 send_button` already maps `SendMode` to a glyph and label
(`:4223`). Animate the control's width and the label crossfade on `Travel`, with the
glyph rolling: empty → small square; sending → labelled button; running → square with a
stop mark.

**Gate:** switching `SendMode` mid-hover does not fight the hover transition; the click
target's final size is hit-testable before the morph ends.

### P5 — Guards, CI, docs (1d)

**Shipped.** A `design-guards` job in `.github/workflows/ci.yml` runs
`check_states.py`, `check_motion.py`, `check_keyboard.py` and `check_i18n.py` on every
push and pull request — no Rust build, so it finishes in seconds — and the required
`build` fan-in now needs it, so a violation is a red check rather than a convention.
`CONTRIBUTING.md` says so.

**Still pending.** The two `⌘⌥P` captures, which need a running build. They are the
before/after for the hover and entrance work, and the numbers to compare against are in
`docs/performance-baseline.md` (p50 16.7, p95 20.7, p99 22.0 on a 16.67 ms budget).

* `scripts/check_motion.py`: the `nominal()` rule from P2 plus a `reduce_motion` guard for
  every `looping::*` animation.
* Optional, only if the two-answer regression shows up in review:
  `scripts/check_states.py` gains a companion that flags a surface where one control uses
  `hover_state` and its sibling uses `hover_motion`. Ship it advisory (a report, exit 0)
  unless it proves precise enough to fail the build.
* `.github/workflows/ci.yml`: a `design guards` job running `check_states.py`,
  `check_motion.py`, `check_i18n.py`.
* `DESIGN.md` §Motion System: flip `Travel` and the preference from "specified" to
  shipped; drop the status note's "land surface by surface" once P4f merges.
* `docs/design-system-audit.md` §10: replace the "specified, not shipped" table with the
  shipped coverage counts; keep the two `nominal()` rows only if any remain.
* Re-capture `⌘⌥P` and append the after-numbers to `docs/performance-baseline.md`.

**Gate:** the three guards pass in CI; the audit table matches the tree.

## How to add a surface (the DX contract)

1. Build the element with its id, children and handlers as today.
2. Last call: the motion method — `hover_motion`, `press_motion`, `focus_motion`, or the
   composite `button` / `icon_button` / `row`.
3. Pick the role from `DESIGN.md` §The interaction layer; never type a duration.
4. Add the element-level test, and one assertion that Reduce Motion renders the end
   state.
5. Run the three guards.

A surface that cannot name its interaction in that table does not get motion — it gets a
`DESIGN.md` row first.

## Verification

* `cargo test -p orbit-pi motion` — primitives and the layer's element tests.
* `cargo test -p orbit-pi` — full bin suite; the interaction-state and motion tests must
  stay green.
* `python3 scripts/check_motion.py`, `check_states.py`, `check_i18n.py`,
  `check_keyboard.py`.
* Manual: settings (toggle, segments, rows), sidebar (collapse, row hover, session
  switch), palette, popover from keyboard, dialog, panel resize, composer send mode, all
  once with Reduce Motion on and once with it off.

## Risks

| Risk | Mitigation |
|---|---|
| A hover fade on every row repaints for 150 ms per pointer move, raising p95 | Adopt per surface group and re-measure after each; rows fall back to mechanism B (fill only), the transcript stays exempt |
| `with_animation` state accumulates once per retarget generation | `Transitions` prunes on settle; ids are stable per element, only the generation suffix changes |
| gpui `debug_assert`s `delta` in 0..1 | `bezier` and every easing clamp at both ends |
| Terminal-call refactor touches ~87 chained sites | Staged per group; composites added first so most sites change one call |
| Reduce-motion preference migration loses a user's choice | Legacy bool maps to `Reduced`, and both keys are written for one release |
| Two-answer regression: some surfaces animated, others instant | The audit table's per-surface counts are the checklist; the guard in P5 catches the duplicate-treatment case |
| Press scale on 16px icon buttons reads as jitter | `Instant` 50 ms, 0.985 only; measure on the smallest `ButtonSize` first |

## Open decisions

1. **Row ink.** Mechanism A fades the label colour; mechanism B does not. Recommended: A
   for rows, because the ink lift is half the hover read. Fall back to B only if a
   virtualized list shows a cost.
2. **Hover-out duration.** `DESIGN.md` uses `Interaction` for both directions. Recommended:
   keep one role; asymmetry (150 in / 200 out) is a real nicety but breaks "one answer per
   interaction" unless it becomes a rule with its own row.
3. **`Travel` at 260 ms.** Alternative: hold the spec at three roles and let popovers use
   `Interaction` (the reference's own popover is 170 ms). Recommended: keep `Travel`, because
   dialogs and the send morph genuinely need the longer band.
4. **Reduce Motion control shape.** A three-way segmented control, or the existing switch
   plus a "Follow system" checkbox. Recommended: segmented — the switch currently cannot
   express `Full`.
5. **Scope of hover adoption.** Every interactive surface (216 sites), or the curated set
   in P3 + P4 (buttons, icon buttons, rows, entries, segments, toggles). Recommended:
   curated, and let the guard plus the audit table show what is left.
6. **Press scale value.** 0.985 proposed. Verify against `ButtonSize::Compact` (16 px)
   before fixing it in `DESIGN.md`.
