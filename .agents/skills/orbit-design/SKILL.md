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

This skill is the normative, self-contained **non-color design specification**
for new and revised Orbit components: geometry, typography, interaction, icons,
motion, accessibility and quality. No other design document or external skill
is required to understand those rules. Palette and background values are outside
its scope: resolve them through Orbit's current semantic theme roles, never from
this file. `DESIGN.md` documents the theme families and their visual appearance.

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
| Color | Semantic `Theme` roles for canvas, surfaces, text, border, focus, action, selection, status, code and syntax; dark and light palettes. Accent and action are separate roles. |
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
| Component corners | Compact/none buttons and list entries use `Medium` 5px; default/medium/large buttons and inputs, including composer, use `Large` 6px; panels use `XLarge` 8px; badges use `Small` 4px. Resolve through component tokens. |
| Controls | Button heights are `None` 16, `Compact` 18, `Default` 22, `Medium` 28 and `Large` 32px before UI scaling. Match the icon to the button's `icon_size`; the icon scale is 10, 12, 14, 16 and 48px, plus custom. Visual size is not proof of usable hit area. |
| Edges and depth | Borders and focus rings are 1px. Keep structural regions flat; floating surfaces may combine a hairline with a restrained shadow. Existing composer contact shadows are a documented exception, not a general card treatment. Focus remains distinct from selection. |
| Motion | Instant 50ms, interaction 150ms, layout 200ms and travel 260ms; instant is linear and the others decelerate. Hover, press, focus and selection do not use travel; a control changing size and meaning may. No bounce by default. Reduced motion resolves to the end state. |

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
| Inputs and composer | Use `input::RADIUS` / `input::composer_radius` and existing input behavior. Preserve native text editing, paste, selection, accessible labels, visible focus and nearby validation. Keep the composer distinct from the transcript without making it a decorative card. |
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
| Hover, selection, press and focus | Shared interaction helpers and `Motion::Interaction` / `Instant`; retarget from the painted value and never delay the action. |
| Disclosure and switch | Existing disclosure and switch helpers; keep layout and indicator state consistent. |
| Menu, popover and dialog | `anchored_popover` applies entrance motion; `modal_card` supplies the frame, not animation. See `dialog.rs` for composing the card with `motion::enter`. Preserve the spatial relationship to the trigger. |
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

Opaque chrome is the default; a whole-app glassmorphism interface is not part
of this system. If the user asks for material exploration, first present the
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

In reviews and handoffs distinguish **shipped**, **verified**, **unverified** and
**proposed**. Cite the relevant files, describe user impact and recommend the
smallest change that solves it. Never imply a skill document implements features.

## Inspiration

Linear's restraint and information hierarchy are useful inspiration, not a token
specification: its web-specific rules and sampled values are not Orbit defaults.
Apple's principles of clarity, agency, responsiveness and adaptable materials
inform native craft; neither Apple's appearance nor its web/CSS examples should
be copied without adapting them to Orbit and GPUI.
