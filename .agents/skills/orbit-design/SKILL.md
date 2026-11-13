---
name: orbit-design
description: >-
  Design, build, review, or refine Orbit's native GPUI desktop interface and
  design system. Use for app UI, components, typography, spacing, color,
  accessibility, interaction states, motion, materials, and developer UX.
  Preserves Orbit's existing design language while identifying proposed changes.
---

# Orbit app design

## Scope and source of truth

Orbit is a native, keyboard-first GPUI workbench for the pi coding agent, not a
web app or a marketing site. Optimize for long coding sessions, information
density, stable spatial memory, responsiveness, and honest agent state. Apply
this skill to `crates/orbit-pi`; do not transfer web CSS or ARIA APIs literally.

This skill is the normative, self-contained **design specification** for new
and revised Orbit components: geometry, typography, interaction, icons, motion,
accessibility, quality, and the semantic colour-role vocabulary with the
theme-compatibility contract those roles have to satisfy. No other design
document or external skill is required to understand those rules. Palette
*values* stay out of scope: they live in `crates/orbit-pi/src/theme.rs` and are
described in `DESIGN.md` §Colors. Resolve every colour through a semantic role;
a hex value printed in a document describes a palette's identity, it is never an
instruction for a component.

When implementing, reuse the existing GPUI primitives and inspect the target
code. Executable behavior and tests establish what currently ships; if it differs
from this specification, report the discrepancy and reconcile the relevant code,
tests and documentation deliberately, rather than claiming an unshipped rule
already exists. Update this skill and `DESIGN.md` together when their shared
contract changes. Preserve Orbit's identity unless a redesign is requested.

## Design principles

- **Instrument panel, not website:** quiet, neutral chrome; useful data carries
  the visual weight. Favor tonal layers and 1px hairlines over decorative cards.
- **Premium means precise:** clear hierarchy, aligned edges, coherent rhythm,
  careful typography, immediate feedback, predictable behavior and polished
  empty/error states. It does not mean blur, glow, gradients or more animation.
- **Native and keyboard-first:** keep focus visible, distinct from selection;
  preserve platform conventions, text editing, shortcuts and focus restoration.
  A command-palette route complements rather than automatically replaces a
  usable keyboard path through a flow.
- **Context before clutter:** reveal secondary actions where relevant without
  making essential operations hover-only. Preserve spatial continuity.
- **Truth and agency:** reflect real runtime state; make side effects, errors and
  recovery clear. Prefer undo when practical; confirm irreversible actions in
  proportion to risk, identifying the affected object and consequences.
- **Accessible by design:** every interactive element requires a meaningful
  accessible name, role and state. Where GPUI/platform support is missing,
  record an implementation gap, not an exemption or a claim of accessibility.
  A tooltip is not proof of an accessible name or keyboard access.

## Existing design system: use what ships

Values below are baseline/default reference points, not a new set of constants.
Resolve tokens through the current theme and verify scaling at the call site.

| Dimension | Current implementation / intent |
| --- | --- |
| Color | Semantic `Theme` roles for canvas, surfaces, text, border, focus, action, selection, status, code and syntax across thirteen dark and light palettes. Accent and action are separate roles. See §Colour roles and theme compatibility. |
| Text | `TextSize`: 11, 12, 14, 16px default reference, plus UI/editor preferences. `HeadlineSize` and `BufferLineHeight` provide additional scales. UI and monospace families serve different roles. |
| Spacing | `DynamicSpacing`: 0, 1, 2, 3, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48 at default settings; density and UI scaling apply. |
| Radius | `Radius::px`: 0, 2, 4, 5, 6, 8, 10, 12px, and full, before the corner-rounding preference. Use component role tokens rather than assuming a radius from an enum name. |
| Controls | `ButtonSize` reference heights: 16, 18, 22, 28, 32px; `IconSize` has 10, 12, 14, 16 and 48px plus custom. |
| Border and focus | `BORDER_WIDTH` and `FOCUS_RING_WIDTH` are 1px. Validate that focus remains visible in real contexts rather than copying a web outline rule. |
| Depth | Flat structural surfaces; `ElevationIndex` reserves restrained shadows for elevated surfaces and modals. |
| Motion | `Motion`: 50ms instant, 150ms interaction, 200ms layout, 260ms travel; reduced motion resolves centrally. Loops and chart cadences have separate contracts. |

The values above are resolved by their respective tokens; they do not all scale
in the same way. `TextSize::XSmall` resolves to 11px despite its stale 10px
comment. A palette never changes geometry, but user font size, spacing density
and corner-rounding preferences can. Do not hardcode reference dimensions in
new components where a semantic token already exists.

### Normative scale and role mapping

All pixel values here are base values before user preferences, unless noted.

| Role | Contract |
| --- | --- |
| UI typography | Default 14px, large 16px, small 12px, extra-small 11px; use `TextSize` and user-adjustable UI/editor sizes. Use the UI family for interface text and the monospace family for code; preserve platform fallback. |
| Headings and leading | Use `TextSize::Large` for page titles and `HeadlineSize` for modal headlines. Buffer line-height roles are 1.3 (`Standard`) and 1.618 (`Comfortable`); select per context and respect custom settings. Do not apply global tracking. |
| Density | `DynamicSpacing` resolves the listed 14-step scale at compact 80%, default 100%, and comfortable 120%, interpolating between anchors before UI font scaling. Use semantic gaps and insets consistently. |
| Corners | `None` 0, `XSmall` 2, `Small` 4, `Medium` 5, `Large` 6, `XLarge` 8, `XXLarge` 10, `XXXLarge` 12, `Full` 9999px. The corner-rounding preference adds up to 4px to intermediate steps, not `None` or `Full`; it is not density- or font-scaled. |
| Component corners | Compact/none buttons and list entries use `Medium` 5px; default/medium/large buttons and inputs use `Large` 6px; the chat composer has a fixed 12px radius independent of preferences; panels use `XLarge` 8px; badges use `Small` 4px. Resolve through component tokens. |
| Controls | Button heights are `None` 16, `Compact` 18, `Default` 22, `Medium` 28 and `Large` 32px before UI scaling. Match the icon to the button's `icon_size`; the icon scale is 10, 12, 14, 16 and 48px, plus custom. Visual size is not proof of usable hit area. |
| Edges and depth | Borders and focus rings are 1px. Keep structural regions flat; floating surfaces may combine a hairline with a restrained shadow. Existing composer contact shadows are a documented exception, not a general card treatment. Focus remains distinct from selection. |
| Motion | Instant 50ms, interaction 150ms, layout 200ms and travel 260ms; instant is linear and the others decelerate. Hover, press, focus and selection do not use travel; a control changing size and meaning may. No bounce by default. Reduced motion resolves to the end state. |

### Colour roles and theme compatibility

A palette decides *what* a role resolves to; this section decides *which* role a
component may use. The roles exist on `Theme`; the thirteen palettes, their families
and their reference values are `DESIGN.md` §Colors.

| Need | Role | Contract |
| --- | --- | --- |
| Window canvas | `background` | The plane the work sits on; panels stand on `surface`, not on this. |
| Panel and chrome | `surface` | Sidebars, panes, title bar, composer. This is Orbit's secondary background. |
| Nested container | `surface_secondary` | A well inside a panel, not a substitute for one. |
| Card, chip, menu, popover, dialog | `surface_elevated` | The one raised plane. Reach it through `ElevationIndex` / `StyledExt`, which also supply the hairline and the layer's shadow. |
| Row and control states | `surface_hover`, `surface_active`, `surface_selected` | Pointer, pressed/open, open row. Choose a treatment, not a role: see §The State Contract in `DESIGN.md`. |
| Edges | `border`, `border_subtle`, `border_strong` | Region seam, quiet internal rule, floating-surface edge. |
| Ink | `text_primary`, `text_secondary`, `text_muted`, `text_disabled` | Emphasis comes from ink weight and size, never from hue. |
| Accent | `accent`, `accent_hover`, `accent_pressed`, `accent_fg`, `accent_text` | `accent_text` is the readable role for links, icons, keywords and mentions; `accent` is a fill, and never an automatic primary-button fill. |
| Primary action | `action`, `action_hover`, `action_pressed`, `action_fg` | A palette may key this to its accent or to a neutral step; the separation is what lets an identity keep its buttons. |
| Status | `success`, `warning`, `error`, `error_hover`, `error_fg`, `info` | `error` is the danger role. Status reinforces a label that already reads; it is never the only signal. |
| Focus | `focus_ring` | Distinct from selection and from the resting hairline; 1px, no layout cost. |
| Text selection | `selection`, `selection_fg` | A text wash (transcript, editor, terminal), not a surface. |
| Translucency | `overlay`, `overlay_strong` | Ink at low alpha for controls floating over the wallpaper; never a slab over a panel. |

Names from other design systems are not new tokens: `background_secondary` is
`surface`, `border_focused` is `focus_ring`, `danger` is `error`, and
`accent_muted` is `accent_text` or a named accent wash strength. Adding a second
field for an existing meaning splits the palette and is not done.

**The Palette Rule.** A component names a role or picks a shared treatment; it
never paints a literal colour and never re-derives one with an ad-hoc opacity.
`scripts/check_states.py` fails the build when a call site names a state colour.

**The Identity Rule.** Every role must hold in all thirteen palettes, dark and
light, with each palette's own accent and its own surface ramp. Do not introduce
a universal dark palette and do not normalise a palette toward the reference one.
Content that owns its colours is the exception: terminal ANSI, syntax roles,
file-type brand marks and diff washes.

**The Instant-Switch Rule.** A palette change replaces the `Theme` global and
every surface repaints from it; no widget caches a colour at construction, and a
child entity must be notified rather than rebuilt. `app/theme_switch_tests.rs`
is the guard.

**The Contrast Rule.** The floors are asserted per palette in `theme.rs`, not
assumed from the reference one: body ink ≥7:1 on the canvas, secondary ink ≥4:1,
muted ink ≥4.5:1 on the canvas and on every row wash it lands on, accent-as-text
≥4.5:1, every primary and error fill ≥4.5:1 against its foreground, focus ring
≥3:1 on canvas and raised surfaces, hairlines ≥1.15:1, status ≥3:1 as
reinforcement, and each palette's surface ramp monotonic. A new role, a new
palette or an adjusted hue extends these tests; it does not ship past them.

**The Visible Step Rule.** A step the eye cannot see is not a state. Each
consecutive ramp step — `surface_elevated` → `surface_hover` →
`surface_active` — holds 1.045:1 against the one below in every palette, and
`surface_hover` never equals `surface_selected`. When you deepen a step, re-check
the muted ink that is read on top of it.

**The Geometry Rule.** A palette never changes geometry. A role change may not
move a border, a radius, a size or a line height; only the user's font size,
spacing density and corner-rounding preferences do.

## Component and interaction contract

For each interactive component, account for rest, hover, press, selected, focus,
disabled, loading and error when relevant. Use shared interaction/state helpers;
do not invent per-surface colors or duplicate controls when an existing primitive
fits. Keep text and icons legible against each actual state and palette.

- Keep focus, selection and hover distinguishable; restore focus on dismissal.
- Make icon-only controls identifiable to keyboard and assistive technology;
  expose shortcut hints when useful, but never rely on hover-only tooltips.
- Keep long paths, branch names, errors and localized text inspectable through a
  bounded, keyboard-accessible surface; test small windows and large UI scaling.
- Show loading where work happens without unnecessary layout shifts. Give errors
  context and an actionable next step; show the real state rather than a fake
  progress value. Empty states should offer an action when one is meaningful.
- For streaming, tool calls and approvals, communicate activity and permission
  scope without blocking input unnecessarily. Keep destructive actions explicit.
- Design pointer targets for desktop precision while avoiding tiny or overlapping
  hit regions. Test at the 960×640 minimum window, maximum UI size and applicable
  density settings; scroll or collapse content rather than clipping essential UI.
- Every component contract specifies anatomy, variants, states, keyboard and
  pointer behavior, focus, accessibility semantics, motion and data; include
  loading, empty, error, clipping and long-content behavior where applicable.
- Use stable numeric alignment, preferably right-aligned with tabular figures
  when supported by the actual font and renderer. Verify support; record gaps.
- Do not claim a control is accessible without testing the path.

**Status:** the tokens and shared helpers exist, but the shared switch currently
has neither keyboard activation nor native accessible naming. GPUI 0.2.2 has no
ready-made native accessibility tree API; a platform bridge remains unverified.
Treat screen-reader support and complete keyboard flows as requirements to
implement and test, not as shipped features. In Explorer, an open file row uses
`surface_selected`, an unselected keyboard-cursor row uses `overlay`, and a
rename row uses `surface_active`. When the tree is focused and its cursor is on
the open file, a focus ring overlays the selected fill without changing layout.

## Component index

The following contracts apply to new work. Helper references are implementation
entry points, not claims that every current surface already uses them. Search
existing components before creating a new primitive.

| Component | Guidance and implementation starting point |
| --- | --- |
| Buttons and icon buttons | Use primary for the main action, secondary for alternatives, ghost for quiet contextual actions and destructive for irreversible operations. Use `ButtonSize` and `button::radius`; frames supply geometry and focus styling, not full behavior. Verify keyboard activation, accessible naming and disabled behavior. |
| Cards and boards | Use a flat section or list when a card adds no meaningful grouping. Use `panel::RADIUS` for grouped surfaces; reserve elevated roles and shadows for genuine floating surfaces. The existing `composer_shadow` contact treatment is also used on the composer, usage board and some terminal/Git surfaces; it is not a general card treatment. Avoid nested cards and repeated decorative boxes. |
| Chips and badges | Distinguish actionable chips from informational badges; use button sizing/states for controls and badge roles for status. Make selected/open states clear, handle long labels, and provide a keyboard path. |
| Sidebar and side panels | Preserve stable navigation, selected versus focused states, resizable bounds and focus on open/close. Support direct manipulation and a keyboard route for important pane actions; do not assume a pane steals focus. |
| Dropdowns, pickers and context menus | Anchor dropdowns and pickers to their trigger; pointer-invoked context menus may anchor to the pointer and need a keyboard equivalent. Stay within the viewport, support keyboard navigation and dismissal, and restore focus. Positioning and row helpers do not provide the full interaction contract: callers must implement and test bounds, keyboard behavior, dismissal and focus restoration. Window-space popovers do not snap automatically. Test long labels and large UI scaling. |
| Inputs and composer | Use `input::RADIUS` / fixed `input::composer_radius` and existing input behavior. The chat composer is an explicit glass exception: with enabled, supported wallpaper frost behind it use the contrast-adjusted card fill and backdrop blur; otherwise keep its opaque surface. Hero wallpaper is behind the composer only on the new-task page. Preserve native text editing, paste, selection, accessible labels, visible focus and nearby validation. Keep access and workflow controls visible; wrap the toolbar when space is tight. Keep the composer distinct from the transcript without making it a decorative card. |
| Transcript edge | Above the composer, the existing 3rem bottom settle appears only while scrolled away from the live edge. With wallpaper glass, smoothly blend sharp into frosted backdrop from top to bottom over the original unclipped bounds; add no palette wash or black dimming. Use one Gaussian pass, not stacked strips or blur animation. Without glass or renderer support, retain the composited-page fade. Let the jump button supply its own contrast; the composer follows its glass contract. |
| New-task page | The same composer in the middle of the page, not a second template: the chat column with an empty transcript, centered by flex weights (`new_task::settle`) at `new_task::COMPOSER_MAX_W` and narrowing with the window. Its context header shares the composer surface: folder and branch controls on the left, and the working-directory control on the right when in a repository. The location control names the project at its main checkout and the worktree inside a linked worktree; its picker retains the Local row. The folder control opens the workspace picker beneath itself; the status row does not duplicate these controls. No headline, mark or card above it. The composer border remains a quiet hairline while its input is focused; the caret indicates text focus. The empty footer leaves out the context meter until real usage exists. The first send docks the composer into the session layout on one `Travel` clock; Reduce Motion paints the session frame at once. |
| Dialogs | Identify the task and affected object; keep actions in a predictable footer. Trap focus while modal and restore it on close. Escape and scrim clicks cancel dismissible dialogs; a mandatory or in-progress dialog may prevent dismissal when interruption would be unsafe, while explaining the state and any available action. Make destructive consequences explicit. Verify the actual focus and keyboard implementation. |
| Rows, tables and lists | Use shared list metrics and interaction states; keep hover, selection and focus distinct, metadata readable, and dense content inspectable. Use stable alignment and keyboard-accessible disclosure for truncated values. |
| Toggles, tooltips and feedback | Inspect `switch`, `empty_state` and `error_state` in `app/helpers.rs`, and `usage/tooltip.rs`. `switch` provides the visual and mouse-up behavior, not keyboard activation or an accessible name; verify or supply those separately. Give switches clear state, avoid hover-only essential information, and place actionable errors near their cause. Use loading feedback without unnecessary layout shifts. |

## Icon policy

- Orbit's UI icons are primarily embedded **HugeIcons free stroke-rounded SVGs**
  in `crates/orbit-pi/assets/icons/` (MIT). The directory's `LICENSE.md` also
  identifies `zsh.svg`, `bash.svg` and `fish.svg` as Simple Icons (CC0).
  Check each asset's provenance, license and attribution requirements rather than
  assuming one license covers the directory. `crates/orbit-pi/src/assets.rs`
  embeds assets and `app/helpers.rs::icon` renders tinted SVGs. Search the
  existing set first.
- File glyphs use a bundled Nerd Fonts symbols face; provider and tool assets
  live in icon subdirectories. Keep brand marks faithful instead of forcing them
  into the generic UI icon style.
- Prefer a consistent stroke-rounded visual weight, optical alignment and
  semantic theme colors. Use `IconSize` or `ButtonSize::icon_size` rather than
  choosing an unrelated size; validate legibility at actual rendered sizes.
- Icon-only actions require an accessible name and keyboard behavior; record
  unsupported semantics as a gap. A tooltip alone is insufficient. Do not use
  color alone to convey state.
- If an existing asset cannot communicate the action, search the same icon family
  first, check its license and attribution, and follow the embedded asset workflow.
  Do not introduce a new pack or install a tool solely for one icon without a
  clear reason and approval. If another family is needed, assess visual and
  licensing consistency before proposing it.

## Component motion guide

Use these motion contracts for new work. Implementation entry points are
`crates/orbit-pi/src/motion/` and `app/helpers.rs`; not every existing control
has adopted each treatment.

| Interaction | Starting point |
| --- | --- |
| Hover, selection, press and focus | Shared interaction helpers and `Motion::Interaction` / `Instant`; retarget from the painted value and never delay the action. `.hover_state(...)` and its siblings swap on one frame — wrap the finished element in `motion::hover` (with a stable, unique name) when the surface should fade. Press is `pressed_state`/`press`, never a hand-picked fill. |
| Disclosure and switch | Existing disclosure and switch helpers; keep layout and indicator state consistent. |
| Menu, popover and dialog | `anchored_popover` applies entrance motion; `modal_card` supplies the frame, not animation. See `dialog.rs` for composing the card with `motion::enter`. Preserve the spatial relationship to the trigger. |
| New-task composer dock | One `Motion::Travel`: the gap under the composer closes, the column widens to the session's, the folder row collapses and the transcript's ink fades in, all from `new_task::settle`. Its cubic ease-out softens the long slide; the clock starts on the first visible chat frame. A resumed session never replays it. |
| Side-panel resize/open/close | Existing layout motion or retargetable spring where appropriate; track direct manipulation promptly and avoid overshoot. |
| Spinner and live progress | Existing loop helpers and cadence; retain a static state indicator under reduced motion. |
| Charts | Use the separate `theme::tokens::chart` cadence, not chart durations for control feedback. |

Apply reduced-motion behavior, maintain interruptibility, and test state changes
and focus even when animation is disabled. Check actual helper adoption rather
than assuming that a documented transition is present on every surface.

## Type, geometry and scale: proposals to evaluate

Orbit already has scales; missing work is primarily **semantic coverage and
verification**, not automatically more sizes. The following are proposals, not
shipped tokens or instructions to add abstractions prematurely:

1. **Typography roles:** inventory title, body, metadata, transcript, code,
   numeric and error styles as size + weight + line-height + font + truncation.
   Define shared role recipes only where real usage differs or repeats; verify
   line height, long translations, large text, fallback fonts and tabular numbers
   where GPUI/font support is confirmed. Avoid a global tracking adjustment.
2. **Radius roles:** map actual control, input, menu, panel, dialog and composer
   uses to resolved tokens, including rounding preferences. Keep documentation
   aligned with `Radius::px`. No new radius is justified solely to match Apple
   or Linear.
3. **Spacing and hit targets:** document semantic inset/gap roles and practical
   minimum hit areas from actual interactions. Test compact and comfortable
   density, large fonts, small windows and nested controls before adopting values.
4. **Icons and elevation:** inventory the large 16→48px icon gap and whether
   intermediate sizes recur; catalog overlay stacking, borders and shadows. Add
   a named step only if multiple real surfaces need it.
5. **States and a11y:** verify focus visibility, native accessibility names,
   contrast across actual surface/state pairs, and complete keyboard flows.
   Existing palette tests are useful but are not proof of full accessibility.
6. **Developer experience:** document discoverable component examples and state
   contracts alongside the code; use existing theme/test helpers and automated
   checks. Add screenshot or interaction coverage where tooling supports it;
   don't claim a visual or platform check that was not run.

## Apple-inspired materials: opt-in exploration only

Opaque chrome is the default; the wallpaper-glass chat composer is an explicit
exception, not a whole-app glassmorphism interface. If the user asks for material exploration, first present the
tradeoff and obtain approval before changing that contract or the default.

A possible **experimental** material system could have semantic roles such as
`opaque`, `floating` and `overlay`, not arbitrary per-component blur values.
Start with one small floating surface such as a popover; keep transcript, code,
inputs and structural workbench chrome reliably readable. A translucent fill,
pre-blurred wallpaper, transparent titlebar and true live backdrop blur are
**different techniques**: do not call one another or claim GPUI 0.2.2 or a
platform supports live blur without checking the actual API and implementation.

Before proposing implementation, establish:

- Platform and renderer capability, clipping, layering and hit testing; a solid
  semantic surface fallback on unsupported platforms or when transparency is
  reduced. Detect OS preferences only where the platform integration supports it.
- Measured text, icon, border and focus contrast over light, dark, busy and
  moving backgrounds; if contrast cannot be guaranteed, use opaque fill.
- No stacked translucent text surfaces; restrained separation and shadows;
  reduced motion and increased-contrast behavior; no blur animation by default.
- Real-device before/after frame-time, GPU and memory measurements during scroll,
  resize and overlays. No visual material justifies degrading long-session work.

Apple-inspired interaction is useful independently of glass: immediate feedback,
reversible interactions, spatially anchored overlays and direct manipulation.
Use Orbit's existing motion language; do not introduce bouncy springs by default.

## Verification and handoff

For changes, run focused tests and applicable repository checks; exercise the
feature with keyboard and pointer, relevant palettes, density, text sizes, window
sizes and reduced motion. Check OS accessibility settings and assistive technology
on available platforms; document what could not be tested. For visual/material
changes, inspect the actual app where possible and record performance evidence
when rendering cost may change. GPUI 0.2.2 has no `gpui::bench` harness and
headless test-window draws do not measure rendering; use in-app frame sampling or
a same-machine release A/B for rendering, and compare against recorded baselines.
The `gpui-bench` skill is optional further guidance.

Colour and theme work has its own commands: `cargo test -p orbit-pi --bin
orbit-pi theme` covers the palette, contrast, surface-ramp and interaction-state
contracts; `scripts/check_states.py`, `check_motion.py` and `check_keyboard.py`
enforce the call-site rules that a palette cannot express. Re-run them after any
role, palette or component-treatment change. A palette is not "tested" by
opening it once: check a light palette, a dark palette, one with a non-blue
accent, and Operational against the same screen.

In reviews and handoffs distinguish **shipped**, **verified**, **unverified** and
**proposed**. Cite the relevant files, describe user impact and recommend the
smallest change that solves it. Never imply a skill document implements features.

## Inspiration

Linear's restraint and information hierarchy are useful inspiration, not a token
specification: its web-specific rules and sampled values are not Orbit defaults.
Apple's principles of clarity, agency, responsiveness and adaptable materials
inform native craft; neither Apple's appearance nor its web/CSS examples should
be copied without adapting them to Orbit and GPUI.
