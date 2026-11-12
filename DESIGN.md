---
name: Orbit Pi
description: Native GPUI workbench for the pi coding agent — quiet chrome, dense data, GPU-drawn.
colors:
  # Reference palette: Orbit Azure (dark), the blue design. Every role below
  # exists on `Theme`; the values are this palette's, not every palette's.
  canvas: "#181818"
  sidebar: "#1F1F1F"
  surface-secondary: "#242424"
  raised: "#292929"
  hover: "#303030"
  active: "#363636"
  selection: "#2F3949"
  surface-selected: "#333333"
  selection-foreground: "#E5E5E5"
  ink: "#E5E5E5"
  ink-secondary: "#A3A3A3"
  ink-tertiary: "#A1A1A1"
  ink-disabled: "#606060"
  hairline: "#343434"
  hairline-subtle: "#292929"
  border-strong: "rgba(229,229,229,0.14)"
  overlay: "rgba(229,229,229,0.05)"
  overlay-strong: "rgba(229,229,229,0.09)"
  accent: "#286CF0"
  accent-hover: "#2D6FEF"
  accent-pressed: "#1F5DD8"
  accent-foreground: "#FFFFFF"
  accent-text: "#6EA0FF"
  action: "#286CF0"
  action-hover: "#2D6FEF"
  action-pressed: "#1F5DD8"
  action-foreground: "#FFFFFF"
  focus-ring: "#6EA0FF"
  success: "#4CAF86"
  danger: "#E47478"
  caution: "#E5AD55"
  info: "#65A4E8"
  code-bg: "#151515"
  code-text: "#E5E5E5"
  inline-code-bg: "rgba(40,108,240,0.10)"
  toggle-knob: "#E5E5E5"
  shadow-contact: "rgba(0,0,0,0.32)"
  syntax-string: "#4CAF86"
  syntax-number: "#E5AD55"
  syntax-function: "#E5E5E5"
  syntax-type: "#E5E5E5"
  syntax-comment: "#A3A3A3"
  syntax-literal: "#E2795B"
  syntax-meta: "#E16D70"
  syntax-operator: "#8A8A8A"
  canvas-light: "#F5F5F5"
  sidebar-light: "#FFFFFF"
  surface-secondary-light: "#F8F8F8"
  raised-light: "#F0F0F0"
  hover-light: "#EBEBEB"
  active-light: "#E2E2E2"
  selection-light: "#E8EEFA"
  surface-selected-light: "#E6E6E6"
  selection-foreground-light: "#171717"
  ink-light: "#171717"
  ink-secondary-light: "#525252"
  ink-tertiary-light: "#646464"
  ink-disabled-light: "#A3A3A3"
  hairline-light: "#E5E5E5"
  hairline-subtle-light: "#EEEEEE"
  accent-light: "#286CF0"
  accent-text-light: "#1F5DD8"
  action-light: "#286CF0"
  action-hover-light: "#1F5DD8"
  action-pressed-light: "#194BB5"
  focus-ring-light: "#286CF0"
  code-bg-light: "#EEEEEE"
  code-text-light: "#171717"
typography:
  title:
    fontFamily: ".ZedSans, IBM Plex Sans, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 500
    lineHeight: "20px"
  body:
    fontFamily: ".ZedSans, IBM Plex Sans, system-ui, sans-serif"
    fontSize: "14px"
    lineHeight: "20px"
  label:
    fontFamily: ".ZedSans, IBM Plex Sans, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    letterSpacing: "0.02em"
  number:
    fontFamily: ".ZedSans, IBM Plex Sans, system-ui, sans-serif"
    fontSize: "20px"
    fontWeight: 500
    lineHeight: "24px"
  mono:
    fontFamily: ".ZedMono, Lilex, ui-monospace, monospace"
    fontSize: "13px"
    lineHeight: "20px"
# Radius values are before the user's corner-rounding preference.
rounded:
  none: "0px"
  xs: "2px"
  sm: "4px"
  md: "5px"
  lg: "6px"
  xl: "8px"
  xxl: "10px"
  xxxl: "12px"
  full: "9999px"
spacing:
  hair: "1px"
  xxs: "2px"
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
  xxl: "24px"
  xxxl: "32px"
motion:
  instant: "50ms"
  interaction: "150ms"
  layout: "200ms"
  easing: "ease-out-quint"
interaction:
  hover-fill: "{colors.hover}"
  pressed-fill: "{colors.active}"
  selected-fill: "{colors.selection}"
  focus-ring: "{colors.focus-ring}"
  disabled-fill: "{colors.overlay}"
  disabled-ink: "{colors.ink-disabled}"
components:
  nav-row:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink-secondary}"
    rounded: "{rounded.md}"
    height: "28px"
    padding: "0 10px"
  nav-row-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.ink}"
  nav-row-active:
    backgroundColor: "{colors.surface-selected}"
    textColor: "{colors.ink}"
  primary-button:
    backgroundColor: "{colors.action}"
    textColor: "{colors.action-foreground}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  accent-button:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-foreground}"
    rounded: "{rounded.lg}"
    height: "32px"
    padding: "0 10px"
  chip:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    height: "28px"
    padding: "0 8px"
  metric-board:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.xl}"
    padding: "0"
  composer:
    backgroundColor: "{colors.sidebar}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "8px 12px"
---

# Design System: Orbit Pi

## Source of truth

The Orbit design skill (`.agents/skills/orbit-design/SKILL.md`) defines the
shared non-color component grammar and scales for new app work. This document
records the theme families, palette and background behavior, and their application
in the workbench. The Rust theme implementation and tests determine what ships;
when a design rule changes, update the skill, this document, and the applicable
implementation together. The frontmatter colors above are an Orbit Azure reference,
not universal colors for every theme.

## Overview

**Creative North Star: "The Instrument Panel"**

Orbit is a workbench, not a website. Every surface is drawn by the GPU through GPUI,
so the chrome stays flat, honest, and quiet while the data — transcripts, diffs,
graphs, usage — carries the density. There is one accent per palette — ember in Orbit,
blue in Orbit Azure, lime in Operational — and it is rationed: selection, focus, the
caret, the active series. It is not automatically a button fill: a palette may key its
primary action to the neutral `action` step instead. Everything else is ink on canvas
at four weights of emphasis.

The system is monochrome-by-default with semantic color reserved for state, so a
screenshot in any of the thirteen palettes still reads as Orbit. Depth is delivered by
hairlines and tonal steps rather than shadows; floating surfaces use restrained
shadows, with the existing composer contact-shadow treatment as an exception.

**Key Characteristics:**
- One accent per palette, rationed: ember (Orbit), blue (Orbit Azure), lime
  (Operational) mark the selected, focused or active thing — and nothing else.
- Hairlines over boxes: `border_1` at `theme.border` separates; cards are the exception.
- Four ink weights: `text_primary` (primary), `text_secondary` (secondary),
  `text_muted` (tertiary/labels), and `text_disabled` (inactive controls).
- Density with air: 22–32px control heights, 4–8px padding, 44px page headers. The
  Appearance panel exposes this as UI, Terminal/Editor font sizes (px), and
  Spacing Density (a global multiplier applied through `DynamicSpacing::px`).
- Two palette families, one grammar: **Classic** is the ember/blue/graphite/paper
  catalog; **Operational** is the near-black, lime-accented colorway. Shapes, control
  heights, caption size and elevation are shared by both — a palette changes color, not
  geometry. Every value is read from `theme::get(cx)`, never hardcoded.
- Twelve palettes: Appearance lists only the palettes belonging to the chosen palette
  family and appearance mode, so the common choice is never made inside a
  catalog-sized list.

## Linear Alignment

The epic's north star is Linear's design *philosophy*, not its brand. What Orbit
takes from it is a set of rules. Each rule below records what the code actually
does today, so this section never describes a surface that has not shipped.

| Linear rule | Orbit's expression | Status |
| -- | -- | -- |
| Surfaces are neutral, not tinted | `background` → `surface` → `surface_elevated` differ by a few lightness points only | Shipped |
| Separation is a hairline, not a shadow | 1px `border` between regions; `border_subtle` inside one | Shipped |
| Shadows are restrained and primarily for floating overlays | `elevation_2` / `elevation_3` for floating surfaces; existing contact-shadow exceptions are documented below | Shipped |
| Controls are 6px, panels 8px | `Radius::Large` 6px, `panel::RADIUS` 8px, for every palette | Shipped |
| Density over air: 28px rows, 13–14px body | `ButtonSize::Medium` 28px, `list_item` rows, `TextSize::Default` | Shipped |
| Accent and action are separate roles | `accent` marks identity; primary fills read `action`, which a palette may key to the accent | Shipped |
| Restrained UI type, 400–600 | `TextSize` 11–16 at default settings; a separate monospace family serves code | Shipped |
| Motion is fast and decelerating | `Motion` roles (`Instant` / `Interaction` / `Layout` / `Travel`) plus `motion::hover` · `control` · `state` · `enter` · `disclosure_chevron` · `switch` | Shipped in §Motion System; the hover fade covers the whole sidebar (rows, workspace headers, group rows, menu entries, every control), the Settings back/search/action controls, the transcript copy button, the changelog link and the caption buttons; every other pane still swaps on one frame |
| Interactive surfaces share state treatments | `interaction::InteractionState` resolves colours; `motion::hover` fades them | Primitives shipped and press treatments adopted; the sidebar and part of Settings fade their hovers, the rest of the app still swaps on one frame (see §The interaction layer) |
| Keyboard is a first-class path | Command registry + palette; every chip has a `command()` route | Shipped |

Four things Orbit deliberately does **not** take from Linear:

- **The brand hue.** Orbit keeps its own palette family. The grammar travels;
  the color does not.
- **Accent-filled primary buttons by default.** A palette may key `action` to its
  accent (Orbit Azure does) or to a neutral step (Orbit's light primary buttons).
  The accent keeps its meaning either way.
- **Translucency and glass.** Chrome is opaque. The wallpaper surface treatment is
  opt-in and never the default.
- **Marketing motion.** No parallax, no spring overshoot, no scroll-triggered
  reveals. Motion confirms a state change and stops.

## Colors

Classic is a charcoal canvas whose accent is the palette's own — ember in Orbit, blue in
Orbit Azure, and so on; light mode is warm-neutral off-white, not white. Operational is a
near-black neutral canvas with a lime interaction accent. Every palette keeps its own
signature hue inside the same grammar.

### Primary
- **Accent** (`#286CF0` in the reference Orbit Azure palette, both modes): selection
  accents, links, the active data series, the caret, and the inline-code wash. `accent`
  is the fill role; `accent_text` (`#6EA0FF` dark / `#1F5DD8` light) is the readable
  foreground role for links, icons, keywords, and mentions. `accent_fg` is the foreground
  on the fill. Never decoration, and never an automatic primary-button fill — the
  `action` role exists so the identity hue can stay the selection/text hue. Focus is
  `focus_ring`, not the accent.
- **Action** (`action` / `action_hover` / `action_pressed` / `action_fg`): the primary
  button's fill. Orbit Azure keys it to the accent (blue on blue); Orbit keys it to a
  neutral step (light buttons with dark ink, the ember-era look).
- **Selection** (#2F3949 dark / #E8EEFA light): the text-selection and find-match wash
  (transcript, editor, terminal). It is a restrained blue-tinted wash, not a bright
  accent fill, and pairs with `selection_fg`. The dark wash is one step below the
  reference #303A4B so `accent_text` labels and secondary metadata on a selected row
  still clear 4.5:1.
- **Selected row** (`surface_selected`): a neutral, theme-matched surface for
  active navigation and list rows, distinct from the text-selection wash. It
  sits between hover and pressed on each palette's surface ramp.

### Neutral
- **Canvas** (#181818 / #F5F5F5): the main content plane and window background.
- **Surface** (#1F1F1F / #FFFFFF): sidebars, panels, and the composer plane.
- **Surface Secondary** (#242424 / #F8F8F8): nested containers and quiet wells.
- **Elevated** (#292929 / #F0F0F0): chips, menus, popovers, modals, grouped rows.
- **Hover** (#303030 / #EBEBEB): pointer feedback only.
- **Active** (#363636 / #E2E2E2): pressed/open controls; distinct from
  `surface_selected`.
- **Hairline** (#343434 / #E5E5E5) and **Hairline Subtle** (#292929 / #EEEEEE):
  1px separators, table rules, section boundaries.
- **Ink** (#E5E5E5 / #171717), **Ink Secondary** (#A3A3A3 / #525252),
  **Ink Muted** (#A1A1A1 / #646464), **Ink Disabled** (#606060 / #A3A3A3).
  Muted clears 4.5:1 against every row wash it can land on — hover, active and
  selected — not just against the canvas. That is what sets it, and it is why a
  palette's muted ink sits closer to its secondary ink than its disabled ink.

### Named Rules
**The One Accent Rule.** The accent appears on ≤10% of any screen. If two things are
accent-colored, one of them is wrong.

**The Ink Weight Rule.** Emphasis comes from ink weight and size, never from hue.
A number that must shout gets `text` + a larger size, not a color.

**The Surface Ramp Rule.** On dark palettes `surface_elevated` clears both the
background and the panel plane, and each state steps up from there — `background ≤
surface_elevated`, `surface ≤ surface_elevated`, `surface_elevated ≤ surface_hover ≤
surface_active` — so a card, a hovered row and a selected row never invert into holes.
Light palettes mirror the ramp downward. `surface_elevated` is where a menu sits; a
popover floats, it never recesses. Every palette is asserted against this monotonicity,
not just Orbit.

**The Visible Step Rule.** Monotonic is not enough: an interaction step the eye cannot
see is not feedback. Every palette holds each consecutive step of the ramp —
`surface_elevated` → `surface_hover` → `surface_active` — at 1.045:1 or better
against the step below, and never gives `surface_hover` and `surface_selected` the
same fill. Deepening a step also moves the muted ink floor, because a pressed row is
one of the washes metadata is read on.
`every_interaction_step_is_visible_on_its_surface` is the guard.

**The Accent-Is-Not-Status Rule.** The accent may never equal `success`, `warning`,
`error`, or `info`. If the theme's signature hue is also its success or error color,
the accent moves to the palette's second hue; a green selection over a green checkmark
reads as one state, not two.

### Composer tokens
The composer paints two inline token roles. They are read as content — siblings of the
`syn_*` editor colors — not as chrome accents, so they sit outside the accent economy:
- **`/command`** takes `accent_text` (`mention_command`): commands are actions, the same
  role terminal command words and syntax keywords already carry.
- **`@file`** takes the accent's complement (`mention_file`), so a reference never reads as
  a command.

Both are derived in `Theme`, so all thirteen palettes stay legible without per-palette tuning.

## Typography

**Display Font:** IBM Plex Sans (bundled as `.ZedSans`)
**Body Font:** IBM Plex Sans (`.ZedSans`)
**Label/Mono Font:** Lilex (bundled as `.ZedMono`) for code, paths, and raw values.

**Character:** One workhorse face for the whole interface — a developer tool does not
need a display pairing. Mono appears only where content is machine text: code blocks,
diffs, paths, token counts in tables.

**The Catalog.** Appearance exposes a curated Interface and Code catalog
(`theme::UI_FONTS` / `CODE_FONTS`) of bundled OFL faces — Inter, Fixel Text,
Geist Sans, Atkinson Hyperlegible, Source Sans 3, Roboto, Noto Sans, DM Sans,
Manrope, Onest, and JetBrains Mono, Fira Code, Geist Mono, Commit Mono, Source Code
Pro, Cascadia Code, Roboto Mono, Iosevka, DM Mono, IBM Plex Mono, Inconsolata,
Noto Sans Mono, Space Mono, Anonymous Pro, Martian Mono. Every family ships as
statically instanced, subset TTFs at the 400–700 weights it offers, under
`assets/fonts/bundled/` (glyph set
is Latin + punctuation + box-drawing + powerline), so a picked face never
depends on the OS. The bundled IBM Plex Sans and Lilex faces stay in the list
under their own names (their gpui aliases are `.ZedSans` / `.ZedMono`).

**The Palette Catalog.** `ThemeId::ALL` ships thirteen palettes: Orbit's own family plus
Flexoki, Orbit Onyx, and the two Operational palettes. The Classic system keeps six dark (Orbit,
Orbit Darker, Orbit Cobalt, Orbit Azure, Flexoki, Orbit Onyx) and five light (Orbit Light, Orbit
Azure Light, Orbit Paper, Orbit Contrast, Flexoki Light) palettes. The blue design is
**Orbit Azure** (dark) and **Orbit Azure Light**; the original ember palette is simply
**Orbit** / **Orbit Light**. Settings → Appearance → **Theme** is a single control, not
one row per appearance mode: it shows and edits the palette currently in effect, listing
only the palettes of that mode and design system. `themes_for_system(mode, system)` is
the single ordering the dropdown paints and a click resolves, so a row index always maps
to the palette it painted. Retired editor themes still load from a saved preference:
dark keys fall back to Orbit, light keys to Orbit Light.

Orbit Onyx uses a near-black `#010102` canvas, neutral charcoal surfaces, `#f7f8f8`
primary ink and a `#5e6ad2` action fill. Its muted ink and button states are
adjusted for Orbit's contrast requirements; error and warning retain distinct
semantic colours. The surface steps are Orbit adaptations, not sourced Linear tokens.

**The Design-System Switch.** Settings → Appearance → **Design system** chooses
**Classic** (default) or **Operational**. It selects the *palette family* — which
themes exist, and the accent and surface colors they carry. It does not change
geometry: every palette draws the same corners, control heights, captions, and
elevation, so a Classic palette and an Operational one are two colorways of one
system. The choice is persisted separately from each family's remembered palette, so
switching to Operational and back restores the previous Classic palette.
`AppearancePrefs::resolve` still honors Light/Dark/System; only the `ThemeId` pair
changes.

### Restrained Operational Minimalism

| Token | Operational dark | Operational light |
|---|---|---|
| `background` | `#080A0B` | `#F4F4F1` |
| `surface` | `#101112` | `#FFFFFF` |
| `surface_secondary` | `#141618` | `#FAFAF8` |
| `surface_elevated` | `#181A1D` | `#F7F7F2` |
| `surface_hover` | `#1D1F22` | `#EEEEEA` |
| `surface_active` | `#232629` | `#E4E5DF` |
| `surface_selected` (rows) | `#1B2115` | `#EEF3DF` |
| `selection` (text) | `#303A4B` | `#E1E9D1` |
| `border` | `#25272A` | `#E1E2DC` |
| `border_subtle` | `#1B1D20` | `#ECEDE7` |
| `text_primary` | `#E6E7E8` | `#171916` |
| `text_secondary` | `#A2A4A7` | `#50544D` |
| `text_muted` | `#8A8C90` | `#6A6E66` |
| `text_disabled` | `#5C5F63` | `#989B93` |
| `accent` | `#C5F34A` | `#5C791C` |
| `accent_hover` | `#D4FA6A` | `#4D6716` |
| `accent_pressed` | `#ACD936` | `#405611` |
| `accent_fg` | `#171A11` | `#FFFFFF` |
| `accent_text` | `#C5F34A` | `#4D6716` |
| `focus_ring` | `#C5F34A` | `#5C791C` |
| `success` | `#36C979` | `#187847` |
| `warning` | `#F5A623` | `#94600D` |
| `error` | `#F0444E` | `#C4313D` |
| `info` | `#65A4E8` | `#2864A8` |

A few values are deliberately corrected from the first draft for contrast: dark
`text_muted` is lightened to clear 4.5:1 on elevated panels and selected rows, dark
`text_secondary` is lifted slightly to clear 4.5:1 on the text-selection wash, and light
`text_muted` is darkened to clear 4.5:1 on the canvas and selected rows. Operational light's `surface_elevated` is a hair
off white so the shared light toggle knob stays visible on its off track. The row wash
is its own `surface_selected` role: Classic sets it equal to `selection`, so existing
palettes are pixel-identical, while Operational gets the olive-tinted selected row.

Operational's colorway is the near-black canvas, the lime interaction accent, and the
olive-tinted selected row. Geometry is not part of that list: every palette resolves the
same ramp — 6px controls, 8px cards and panels, 11px captions (`TextSize::XSmall`), flat
buttons, and one restrained shadow layer for popovers/modals. The `DesignSystem` enum
names a palette family, not a shape language.

**Shape roles.** Corner values are chosen by component role, not by call site:
`button::RADIUS` / `button::radius(size)` for buttons, segmented controls, tabs, and
header chips; `input::RADIUS` for text fields; `panel::RADIUS` for cards, panels,
popovers, menus, and dialogs; `list_item::RADIUS` for navigation/list rows; and
`Radius::Small` for badges and checkboxes. The chat composer uses its own fixed
12px `input::composer_radius`, independent of the rounding preference. Fully round
`Radius::Full` is reserved for genuinely circular or pill-shaped controls (send/stop
actions, toggles, avatars, status dots, meters) and is not mixed into the rectangular
card/field/button family.

**The Primary-Action Role.** `accent` is a palette's *identity* hue — selection washes,
focus, tints, and accent-colored text derive from it. A primary button's fill is
`action` / `action_hover` / `action_pressed` with `action_fg` ink, a separate role so a
palette can key its buttons to its accent (Orbit Azure: blue on blue) or to a neutral step
(Orbit: light primary buttons with dark ink, the original ember-era look) without losing
the identity hue everywhere else. Both roles are covered by the AA floors in the theme
tests.

**Orbit Cobalt.** The graphite-and-blue dark palette shipped as "Orbit Minimal" and is
renamed for what it is: a hue. Minimal is a property of the interface, never of a
palette. The persisted key `orbit-cobalt` still accepts `orbit-minimal`, `codex`, and
`codex-dark`, so a saved preference survives the rename.

### Hierarchy
- **Title** (`TextSize::Large`, 500): page titles in a 44px header row.
- **Body** (`TextSize::Default`, 400): default UI text, list rows, table cells. 14px at
the default UI size; setting the UI font size to 13 yields the denser 13px body rhythm
Linear uses, and every step below follows it.
- **Body small** (`TextSize::Small`, 400): secondary row text, chips, buttons.
- **Label** (`TextSize::XSmall`, 500, +0.02em, uppercase): section labels in `text_muted`,
always paired with a count or meta on the right of the same row.
- **Number** (500, 20px): one per metric cell, no accent color.
- **Mono** (`theme.code_px`, 400): tokens, hashes, durations, exact figures.

### Named Rules
**The Size-Not-Weight Rule.** `TextSize` steps 11 → 12 → 14 → 16 at default
settings; 10px uses are separate, not an `XSmall` token step. Weights generally use
400, 500 and 600. Define each role by family, size, weight, line height and truncation;
avoid a global tracking adjustment.

**The Numeric Alignment Rule.** Keep numeric columns stably aligned, preferably
right-aligned with tabular figures where the selected font and renderer support them.
Verify support rather than assuming it. Use compact forms in the main UI (`18.4M`),
exact forms in tooltips and detail tables (`18,421,842`).

## Layout

A three-part shell: a fixed sidebar column (200–320px, default 248px), the content
plane, and an optional right pane. The content plane is full-bleed with a centered
inner column (960px for reading surfaces, 1180px for data surfaces) and 20px gutters.

Vertical rhythm: 44px header rows with a bottom hairline, 20px between sections, 12px
inside a section's header-to-content gap, and 6–8px between dense rows. Long lists are
virtualized (`uniform_list`) and scroll inside the plane — the page itself never scrolls
as a whole when a table is present.

Responsive behavior is structural, never fluid type: grids drop columns (4 → 2), paired
columns stack into one, and the sidebar hides before the content squashes. Test at the
960×640 minimum window with maximum UI size and applicable density settings; scroll or
collapse content instead of clipping essential controls.

## Elevation & Depth

Flat by default. Depth comes from tonal steps (canvas → raised) and 1px hairlines.
Floating surfaces read a restrained shadow stack from `ElevationIndex`; the
existing contact-shadow exception is listed separately below:

### Elevation Vocabulary
- **`ElevationIndex::ElevatedSurface`** (`elevation_2`, one tight layer): anchored
  menus, dropdowns, tooltips, popovers.
- **`ElevationIndex::ModalSurface`** (`elevation_3`, one restrained layer): modals,
  dialogs, the command palette.
- **`ElevationIndex::Background` / `Surface` / `EditorSurface`** cast nothing. A panel
  is a tonal step and a hairline, not a shadow.
- **`Theme::composer_shadow`** is an existing tight contact-shadow treatment used by
  the composer, usage board and some terminal/Git surfaces. It is an exception to the
  flat structural default, not permission to add shadows to ordinary cards.

### Named Rules
**The Separation Rule.** Structural surfaces use hairlines and spacing, not
shadows. Floating surfaces may combine a 1px hairline with a restrained elevation
shadow; the shared elevation helper does both. Existing contact shadows are the
exception above, not a general card treatment.

**The Rectangle-Clip Rule.** gpui 0.2.2 clips `overflow_hidden` to a rectangle,
not the border radius (`Style::overflow_mask` builds a `ContentMask` from
`bounds` alone), so a child background that reaches a rounded container's edge
paints square corners over it. Such a child must carry the matching radius
itself — `rounded_t` on the first, `rounded_b` on the last — or the container
must be a popover whose content is `deferred` (deferred draws bypass ancestor
masks). Applies to the transcript cards, the code-block title strip, and
list-board rows (`settings_group`, the MCP server rows).

## Shapes

Corners are soft but restrained. Before the corner-rounding preference, list
items and compact button frames use 5px, standard buttons and inputs use 6px,
and panels and elevated surfaces use 8px. The chat composer is fixed at 12px. `full` is
reserved for circular or pill-shaped elements such as status dots, avatar
monograms, the send button, and the toggle-switch track.

Borders are 1px at `theme.border`; `border_strong` (ink at ~14% alpha) is reserved for
focus and hover emphasis. Charts and graphs draw with 1.5px strokes; grid lines are 1px
at low-opacity hairline. No drop-shadowed text or decorative gradients; raised-button
fills are flat (see Buttons).

### New-task backdrop

The new-task page is the one surface with a background treatment, and it has two forms
under one grammar — ordered dither, the same screen the wallpaper's `Dither` effect uses:

- **No image configured:** a painted halftone field (`backdrop`) — an 8×8 Bayer screen of
  four dot sizes over a soft density field, densest in a band behind the composer and
  calmer — never empty — toward the window's edges, so the screen runs edge to edge and
  its drift is visible to the frame. **The plane is the palette's own `background`, never a
  wash:** the accent appears only as the dots' ink, at 32/42/52% of its own alpha across
  the ladder, so a full page still reads as the theme's canvas carrying a texture rather
  than an accent-tinted slab. That keeps it a paper texture, never a second content layer,
  and works in all thirteen palettes without a per-palette asset.
- **The monogram is screened into that field.** Where the app mark sits — in the band of
  open grain under the centered composer column, at `MARK_CY` — the same screen takes over its
  shape: a coverage
  mask sampled from the shipped `app-icon.png` (never redrawn by hand, so the backdrop
  cannot drift from the icon), glyphs holding a density above the page grain at rest while
  a soft band of light crosses them every twenty seconds, filling each letter with the
  largest dot in turn. The mark is denser, not a different colour, and a layer too small to
  hold it drops it rather than stretching a wordmark into mush.
- **A wallpaper configured:** the processed image (`wallpaper`), with an optional artwork
  effect (`None`, `Dither`, `ASCII`, `Halftone`, or `Scanlines`), an effect intensity, an
  effect pixel size, and a **black** overlay — never a palette tint, which washed a bright
  picture out while claiming to dim it. Hero mode paints it only in the main column;
  full-window mode paints it behind the sidebar too, with translucent surfaces above.
  The reader's own message bubble remains opaque. The chat composer is an explicit
  exception: with supported wallpaper glass behind it, it uses the contrast-adjusted card fill
  and a rounded backdrop frost; otherwise it retains its opaque surface. Hero mode
  has no wallpaper behind the composer in active chat. The page itself never goes below
  88% of the palette canvas, so an artwork effect's dots tint the page instead of drawing
  behind the transcript; panels and cards keep the card-opacity treatment and its frost,
  and the black overlay is resolved against the image's own dot extremes (not its cell
  averages) wherever text reads on the sharp picture. Edge fades — the transcript's
  bottom settle above the composer, the navigation rail's ends — **frost** what they
  cover instead of painting the page's colour over it: a colour stop cannot match a
  translucent page over a picture (every extra layer pulls the strip toward the palette
  canvas and prints a band that looks separate from the page), while a blur keeps the
  picture's own brightness, so the strip reads as the page it sits on and the text under
  it stops being text. The transcript's 3rem bottom settle blends the sharp and
  frosted backdrop with a smoothstep from clear at the top to full frost at the
  bottom, measured over the original strip bounds even when clipped. It adds no
  palette wash or black dimming; only the blur coverage changes, using one Gaussian
  pass rather than stacked strips. The jump button supplies its own contrast. Without a backdrop pass the strip falls back to the
  page's composited appearance.

Both are the page's floor, not an illustration: nothing on the page depends on them, and
the chat page has neither.

## Components

### Buttons
- **Shape:** `button::radius(size)` — 5px on compact frames, 6px on the standard scale;
  32px (`Large`), 28px (`Medium`), 22px (`Default`), 18px (`Compact`), 16px (`None`).
- **Primary:** `surface_elevated` + 1px `border`, hover steps to `surface_hover` with a
  `border_strong` edge. One per screen region. A palette may instead key its primary
  fill to `action` / `action_hover` / `action_pressed` — that is the Linear-aligned
  reading, and the role exists so the accent never has to be a button color.
- **Raised:** every filled button (primary, accent, danger, chip) paints its face
  through `RaisedExt::raised`, which is a flat fill: the color and its 1px border carry
  the affordance, with no gradient and no lift shadow. Segmented controls use
  `raised_flat`. Ghost and disabled buttons stay flat too.
- **Ghost:** no fill until hover (`surface_hover`); used for every icon action in a header.
- **Disabled:** `overlay` fill with `text_disabled` content; never opacity-faded color.

### Chips
- **Style:** 28px tall, 6px radius, `surface_elevated` + hairline, 12px ink, optional leading
  Small (14px) icon and XSmall (12px) trailing chevron — `ButtonSize::icon_size`
  for the leading glyph, the chip-caret size for the chevron.
- **State:** open/selected = `surface_active` fill + `text_primary`; hover = `surface_hover`.

### Metric cells
- **Style:** a single bordered board (`surface_elevated` + hairline + 8px radius) whose cells
  are divided by 1px hairlines — never a grid of separate floating cards.
- **Content order:** 11px uppercase label → 20px number → 11.5px secondary line
  (comparison or average). Missing data reads "Unavailable", never a fabricated `0`.

### Cards / Containers
- **Corner Style:** 8px (`panel::RADIUS`).
- **Background:** `surface`/`surface_elevated`; nested cards are forbidden.
- **Border:** 1px `border`.
- **Internal Padding:** 12–14px; lists use full-bleed rows with 8px inner padding and a
  hover fill instead.

### Inputs / Fields
- **Style:** 1px hairline, fixed 12px radius on the chat composer
  (`input::composer_radius`), 6px before preferences on inline fields (`input::RADIUS`).
  The composer uses `surface_secondary`, with contrast-adjusted glass when available.
- **Focus:** other fields step their border to `focus_ring` (1px). The composer keeps
  its resting hairline when focused; its caret indicates text focus.
- **Placeholder:** `text_muted` — always a real prompt ("Search sessions…"), never a label.
- **Tokens:** a leading `/command` and any `@file` mention paint inline in their token
  colors while typing; the caret, selection, and wrapping are unaffected (paint only).
- **Caret & selection:** a 2px accent caret that holds solid while the field is being
  edited and blinks at the platform cadence (~1s) once idle. The selection wash is the
  accent at 25%; double-click selects the character run, triple-click the line, and
  shift-click extends — matching native text fields.

### Modals and dialogs

A modal is the only live layer while it is open, and every one of them is built from
the same two pieces (`modal_card`, `modal_scrim` in `app/helpers.rs`):

- **Scrim:** `scrim_modal` over the whole window, `Base24` padding, card centred. A
  click on the scrim cancels a dismissible dialog; a click on the card never reaches
  it. Mandatory or in-progress dialogs may prevent dismissal when interruption would
  be unsafe; explain the state and any available action. There is one scrim value for
  every modal — a dialog that wants a darker backdrop changes `scrim_modal`, not itself.
- **Card:** `elevation_3` (elevated surface, `border_strong`, one restrained shadow),
  the panel corner, a clipped column, and the UI font. Width and height bounds belong
  to the caller; everything else belongs to the shell.
- **Anatomy:** header (`modal::header_padding_*`, Small headline) → body → footer, with
  hairlines between. The footer holds the actions; a destructive action is separated
  from the primary one.
- **Keyboard:** Escape cancels dismissible dialogs; the same mandatory or unsafe
  interruption exception applies as for the scrim. The primary action takes Return
  where appropriate, and focus is trapped inside the card until it closes, then
  returned to the control that opened it.

A dialog that paints its own surface, corner, or backdrop is wrong even if it looks
plausible — that is how the worktree dialogs drifted to `surface_secondary` while every
other modal floated on `surface_elevated`.

### Navigation
- Sidebar rows: 28px tall, 5px radius, 14px icon (Small) in a fixed 20px slot, label in
  `text_secondary`/`text_muted`; hover fills `surface_hover`; the active destination is
  marked by `surface_selected` fill with `selection_fg`, never by accent color alone.
- A running session row leads with a small accent arc turning in place and its title
  carries a shimmer — an accent highlight band swept left-to-right across the ink on a 2s
  loop. This is the only per-row motion in the sidebar; a settled row is still.
- Page headers: 44px, back affordance first, title in 15px/500, actions right-aligned
  as 28px ghost controls.

### Tables
- Header row: 11px uppercase `text_muted` labels, hairline underneath, sticky.
- Rows: 26px, hairline separators, hover fills `surface_hover`, selected fills
  `surface_selected`,
  numeric columns stably aligned, preferably right-aligned with tabular figures where
  the selected font and renderer support them; first column left-aligned.
- Clipped cell text always ends in `…`: the character budget is computed from the
  column's width rather than left to the renderer, so no cell is ever cut mid-glyph.
- Row-level actions are revealed on row hover, never as a permanent icon rail.
- Sort indicators are small chevrons in `text_muted`; the sorted column's label steps to
  `text_secondary`.
- **Data tables and plots are drawn with GPUI's own elements.** The usage tables
  are built from `div` in `usage::table` and the timeline from `canvas` in
  `usage::chart`, each reading colors only from `theme::get(cx)` — the same
  surface as the rest of the page. There is no component layer and no second
  palette to synchronise.

**The Bridge Rule.** A table or chart reads every color from `theme::get(cx)`. If
it renders in a color this file does not list, the element is wrong — fix the
element, not the palette.

## Interaction System

Orbit is a native workbench. Visual hierarchy and interaction hierarchy must reinforce
the same model: navigation is quiet, the workspace is primary, and contextual tools
appear only when relevant.

### Interaction Hierarchy

Every interactive element belongs to one of five levels:

1. **Primary** — the main action for the current surface.
2. **Secondary** — supporting actions that remain visible when useful.
3. **Tertiary** — low-emphasis actions exposed through ghost controls or menus.
4. **Contextual** — actions revealed by selection, hover, right-click, or focused content.
5. **Destructive** — actions that can remove or irreversibly change user data.

Do not give two competing actions the same visual weight.

### Focus

Focus is a first-class state, not a stronger hover state. It is one token, drawn one way.

- Pointer hover fills `surface_hover`.
- Keyboard focus draws a **1px border in `focus_ring`** (`FOCUS_RING_WIDTH`), never a
  fill and never an outline that changes layout. On a filled control the border sits
  on the control's own edge; on a row it sits on the row's rounded rect.
- Focus is never signalled by accent color alone — the ring is its own role, and it is
  asserted at ≥3:1 against `background` and `surface_elevated` for every palette.
- Focused selection must remain distinguishable from an unfocused selection: the
  selected fill stays, the ring is added on top.
- After dismissing a popover or modal, restore focus to the control that opened it.
- Opening a contextual panel must not unexpectedly steal focus from the active editor
  or composer.

### Selection

Selection means "the object currently being operated on"; focus means "the object currently
receiving keyboard input." They must remain visually distinct.

- Selected rows fill `surface_selected`. Classic palettes resolve it equal to
  `selection`, so their paint is unchanged; Operational gives rows their own
  olive-tinted step.
- Text selection (transcript, editor, terminal, find matches) fills `selection` and
  writes `selection_fg`.
- Hover fills `surface_hover`; pressed fills `surface_active`. Neither may equal the
  selected fill in any palette — `interaction_states_never_collide` holds this over all
  thirteen.
- A focused selected row adds the ring without changing its fill.

### The State Contract

Interactive surfaces should answer the same six states using
`theme::tokens::interaction::InteractionState` rather than re-deciding them at
the call site. Existing adoption is still in progress:

| State | Fill | Frame | Row ink | Control ink |
| -- | -- | -- | -- | -- |
| `Rest` | none | role's own | `text_secondary` | `text_primary` |
| `Hover` | `surface_hover` | role's own | `text_primary` | `text_primary` |
| `Pressed` | `surface_active` | role's own | `text_primary` | `text_primary` |
| `Selected` | `surface_selected` | role's own | `selection_fg` | `selection_fg` |
| `Focused` | none | `focus_ring` | `text_primary` | `text_primary` |
| `Disabled` | `overlay` | `border_subtle` | `text_disabled` | `text_disabled` |

Filled controls step their own role instead of the wash, because `surface_active`
over a primary, accent or danger fill changes what the button means while it is being
clicked:

| Ramp | Pressed fill | Used by |
| -- | -- | -- |
| `Wash` | `surface_active` | Ghost controls, neutral filled buttons, rows |
| `Selected` | `surface_hover` | A control already open/selected on `surface_active` — the ramp has no deeper step, so the press steps back and the movement is the feedback |
| `Action` | `action_pressed` | The primary action |
| `Accent` | `accent_pressed` | An accent-filled control |
| `Danger` | `error_hover` | A destructive control |

The states are inherited from **named treatments**, never re-decided at the call
site. A component picks one of these; it cannot name a state colour, and
`scripts/check_states.py` fails the build if it tries:

| Group | Treatments | Paints |
| -- | -- | -- |
| Wash | `hover_state`, `hover_quiet` | `surface_hover`; primary ink vs secondary ink kept |
| Bordered | `hover_bordered`, `hover_bordered_ink` | `surface_hover` + `border_strong`, ink optional |
| Filled role | `hover_action`, `hover_danger`, `hover_danger_soft`, `hover_accent` | `action_hover` / `error_hover` / accent, per role |
| Accent wash | `hover_accent_wash(Faint \| Soft \| Strong)` | the accent at a named strength |
| Translucent | `hover_overlay`, `hover_overlay_bordered`, `hover_overlay_strong`, `hover_on_glass` | ink at low alpha, for surfaces over the wallpaper |
| Ink only | `hover_ink`, `hover_ink_quiet`, `hover_ink_accent` | a text colour, no fill |
| Frame only | `hover_border`, `hover_divider` | `border_strong` with no wash |
| Selected-aware | `hover_state_open(open)`, `hover_selected` | a control already open stays on its selected step |
| Neutral | `hover_elevated`, `hover_inverse`, `hover_dimmed` | the elevated fill, an inverse swatch, a dim |

A filled face may also press by dimming (`press()`, `PRESS_DIM`) where it has no
semantic pressed step to move to. That is one shared constant, never a per-call-site
colour, and it is the only press treatment outside the table.

**The Disabled Rule.** Disabled is a state, not an opacity. The control keeps its
frame, takes `text_disabled` ink and an `overlay` fill, and is **non-interactive**: no
pointer activation, no keyboard activation, no hover lift. Never fade a live control
to pretend it is disabled, and never leave a control that looks disabled still
clickable.

### Command System

The command palette is a first-class navigation surface.

- Default shortcut: `Cmd+K` on macOS, `Ctrl+K` elsewhere.
- Search commands, pages, sessions, files, and supported actions from one surface.
- Results are keyboard navigable.
- Commands expose shortcuts when one exists.
- Escape closes the palette and restores the previous focus target.
- Destructive commands require explicit confirmation.
- Commands must operate on the same underlying action handlers as buttons and menus;
  keyboard and pointer interactions must not create separate behavior paths.
- A chordless `command(…)` provides a palette route without a keymap row. That
  route does not by itself prove that the visible control or its workflow is
  keyboard accessible; verify the complete path.
- Every command draws an icon, and GPUI resolves it through the embedded asset source, so
  a path the source cannot find fails when the row is drawn. `every_command_icon_is_embedded`
  holds the whole registry against the assets that ship — add the missing icon rather than
  softening the reference.

### Context Menus

Context menus expose actions that are useful for the currently focused object.

- Prefer contextual actions over permanent icon rails.
- Menu order follows action importance, not implementation order.
- Destructive actions are visually separated.
- Menus use `surface_elevated` and the `elevation_2` shadow.
- Right-click and keyboard context-menu invocation must expose the same actions.

## Workbench Layout

### Shell

The canonical Orbit shell is:

**Navigation → Workspace → Context**

- **Navigation**: sidebar, project/session navigation, global destinations.
- **Workspace**: the primary editor, transcript, table, diff, or task surface.
- **Context**: optional right or bottom pane for details, review, activity, metadata, or
  secondary controls.

The workspace owns the visual hierarchy. Navigation and context should not compete with it.

### Sidebar

The sidebar is quiet infrastructure.

- Default width: 248px.
- Supported range: 200–320px.
- Collapse/hide the sidebar before forcing the workspace to become cramped.
- Sidebar sections use whitespace and labels rather than excessive containers.
- Project marks sit on small theme-tinted tiles (the default inherits the accent;
  custom tints and logos remain intact), with a quiet tonal session counter.
  Session titles use primary ink and medium weight, stepping to semibold for the
  selected session; previews use secondary ink and metadata stays muted. Real
  execution states get semantic badge washes, never invented activity. Selection
  combines the existing selected fill with a subtle accent outline; keyboard focus
  has its own full-strength outline, including on the selected row. Resting rows
  stay unboxed, and provider marks keep their individual hover targets.
- A project group header carries a session count only when it is non-zero. A project
  with nothing in it already says so by having no rows under it.
- A session row's title line is its anchor and is always there. The sidebar view
  menu's **Show first message** toggle controls the preview below it (on by default,
  remembered across restarts in both grouped and flat lists). When enabled, it carries
  the whole first message: pi titles a session from that message, so the two often start the
  same way, but the title is capped at 80 characters and the row shows far fewer — the
  message line is the only place the ask reads to its end. Line 3 is the metadata strip —
  age, workspace (flat list only: under a project header the name is already one line up),
  and branch with `↑n`/`↓n` against its upstream. Rows are three lines tall and drop to two
  when previews are hidden or nothing has been sent yet. The branch truncates last, and a divergence count
  appears only when it is non-zero.
- Every line of a session row is a truncating text run inside a flex row, with
  `flex_1 min_w_0 truncate` on the text itself. `truncate()` on a bare block collapses to
  its own ellipsis instead of filling the column.
- The list has three groupings: **Projects** (one group per workspace), **All
  sessions** (one flat list), and **Recent** (every pinned, open or running
  session, then the six most recent of the rest). Recent answers "what was I
  working on"; the other two answer "where is it".
- A session row's actions live on its right-click context menu, the same gesture every
  other row in the app uses. There is no hover-revealed `…` on a session row: the title
  owns the full line width. The project group header keeps its own `…` (and `+`) because
  a header is a section control, not a row.
- Active navigation uses `selection`, not accent color alone.
- Execution state lives in one **status lane**: a 16px slot at the leading edge of
  the row's state line (the title line under a project header, the project line in
  the flat list), so the list's activity is one vertical scan away instead of a
  word competing with every title. The lane paints one glyph — a spinner while
  running, a lock for an approval gate, a chat glyph for a question, a check when a
  result has been read, an eye when it has not, a cross for a failure or a dead
  process, a stop for an aborted run — and nothing at all for `Idle` / `Unknown`;
  the slot keeps its width either way so every row's text stays on the same x. Only
  the spinner moves. Attention states (blocked, needs review, failed) add words in
  place of the age, so colour is never the only distinction.
- A session whose turn settled while the user was not reading it keeps an **unread
  mark** on its row — a green eye plus "Needs review" until the session is actually
  shown in a foreground window, then a green check for the read result; a failed
  background turn keeps a red cross with a small amber corner dot. The mark is
  separate from the running, pin, and archive glyphs (it never replaces one), and a
  marked row stays visible under a collapsed project group the way a running one
  does. Selecting a row in a background window is not "shown": the mark survives
  until Orbit is frontmost with that session on screen. Shape and label both carry
  the state, so the marks remain distinguishable without colour.
- Session state may use semantic indicators, but decorative animation is forbidden except
  for the currently running session treatment already defined above.
- Sidebar width and collapsed state should persist per workspace/window when practical.

### Status Row

The status row under the composer is a readout, not a toolbar.

- It carries live state only: where the workspace is, which branch it is on, and how much
  context is left. Anything that restates a control sitting a few pixels above it belongs
  to that control, not here.
- **Ahead/behind counts appear only when they are non-zero.** `↑0 ↓0` is a row of arrows
  that says nothing; a clean branch shows its name alone.
- **Send hints appear only while a run is in flight.** Idle they restate the send button —
  a hint the user already acted on is instruction, not information. Running they carry the
  one thing the button cannot say: which chord steers the live turn and which queues a
  follow-up.
- **The MCP chip appears only when a server needs attention.** A fully connected fleet is
  the steady state, and the steady state earns no pixels. Its label names the counts, so
  colour is never the only signal.
- **The "Work in" chip is always present in a session inside a repository.** It is not a readout of the default:
  it is the control that decides where the next task runs, and its rows include the places — a
  linked worktree, a new one, an existing folder, a remote branch, or a clone — that have no other
  affordance in the composer. At the main checkout it names the project; inside a worktree it names
  that worktree; on a broken worktree it says so instead of naming a branch. Outside a repository the
  chip is absent, and the plain folder chip takes its place; the new-task picker still offers
  cloning and folder selection outside a repository. Choosing a different location starts a task
  there; a running session is parked, and an idle session's transcript remains on disk. Creating a worktree opens a dialog. See
  `the_work_in_chip_is_absent_outside_a_repository` and
  `the_chip_names_the_project_at_the_main_checkout`.
- **On the new-task page both status controls stand down.** The composer's context header already
  provides folder, branch and working-directory controls, so the status row does not duplicate
  them. The session status row takes those roles back after the dock. See
  `the_folder_row_is_the_only_folder_control_on_the_page` and
  `the_branch_and_working_directory_share_the_composer_header`.
- Workspace and branch chips yield before the row can crowd; the context indicator holds
  the right edge.

### Context Panes

Context panes are contextual, not permanent dashboard columns.

- Open when the current task benefits from additional information.
- Preserve the workspace's primary reading/editing width.
- Allow resizing.
- Remember size during the current workspace lifecycle.
- Close with Escape where appropriate.
- Do not duplicate information already visible in the workspace.

### Resizable Splits

Resizable panels use a small, low-contrast hit target around a 1px divider.

- Divider is visually quiet at rest.
- Hover increases contrast.
- Active drag uses the focus/interaction treatment.
- Do not animate layout during manual resizing.
- Persist useful split positions where doing so does not create surprising layouts.

### Workspace Presets

Three named shapes ship, reachable from the sidebar's view menu. A preset is a
**starting point, not a mode**: it sets all four panel sizes and opens the panels
it names, then gets out of the way. Dragging any panel afterwards is the user's
layout, and nothing re-applies the preset.

| Preset | Sidebar | Project tree | Review pane | Terminal |
| -- | --: | -- | -- | -- |
| **Focus** | 240px | closed | closed | closed |
| **Developer** | 248px | open, 300px | closed | open, 260px |
| **Review** | 220px | closed | open, 560px | closed |

**A preset is reversible.** Applying one saves the arrangement you built first;
the view menu then offers **Back to custom layout**, which restores it exactly.
Touching any panel after a preset ends the preset — the arrangement is yours
again, and there is nothing stale left to go back to.

**Named layouts** sit beside the presets. Saving captures the current arrangement
under a name; applying one restores that snapshot, and later edits never rewrite
it. A named layout is the user's own, so applying one leaves the custom slot
alone. Eight at most — past that the menu stops being a menu.

**The window has a floor.** Below 880px the sidebar yields whatever the toggle
says, because a cramped column beside a cramped transcript is worse than a
transcript with ⌘⇧B to bring the list back.

Sizes, open panels, sidebar visibility and collapsed workspace groups all land in
the same store a drag writes to, so a preset — and a hand-built layout — survive a
restart. Every field is read independently on load: a missing key keeps its
default, an unknown key is ignored, and a value with the wrong type or outside the
accepted range is dropped, so one corrupt field can never wipe a workspace. Every
preset value is inside that range, because a preset must never persist a size the
loader would silently drop.

## Desktop Interaction Model

Orbit should behave like a native desktop application rather than a responsive website.

### Keyboard-first Behavior

Every primary workflow must be possible without a mouse.

Minimum expectations:

- Command palette.
- Session navigation.
- Project/file navigation.
- Search.
- Focus movement between major panes.
- Open/close contextual panels.
- Submit/abort/steer agent runs.
- File operations.
- Diff navigation.
- Menu dismissal with Escape.
- Standard text editing shortcuts.

Composer controls currently have command or shortcut routes; the
`composer_controls_are_reachable_without_a_pointer` test covers agent controls
(model, thinking, workflow mode, access mode). A palette-only command can expose
an action, but it does not prove the visible control or complete flow is usable
without a pointer. Verify traversal, focus and operation separately and record
remaining gaps.

Do not invent custom shortcuts where standard macOS or Windows behavior already exists.

### Pointer Behavior

Pointer interaction should reveal information progressively.

- Hover reveals secondary actions.
- Selected objects retain their state after the pointer leaves.
- Tooltips explain unfamiliar icon-only actions.
- Never require hover to discover a destructive action.
- Avoid permanent action rails when row-level actions can remain contextual.

### Native Text Behavior

Text fields and editors should follow platform expectations:

- Standard selection behavior.
- Standard copy/paste/cut.
- Shift-based range extension.
- Double-click word selection.
- Triple-click line selection where supported.
- Platform-standard modifier keys.
- Correct focus and caret behavior.

## Motion System

Motion communicates state; it does not decorate the interface.

Motion is layered. A surface never picks a duration — it picks a **role** — and every
interaction the pointer or keyboard can produce answers through the **interaction
layer** rather than with a colour swapped on a single frame. States defined without
transitions read as reduced motion even when the setting is off, which is why the layer
is a contract and not a polish pass.

The roles ship today, and so do the primitives (`crates/orbit-pi/src/motion/`) and the
three-way preference. The interaction layer that composes them lands surface by surface;
`docs/design-system-audit.md` §10 records which surfaces have adopted it and which still
render the instant swap.

### Roles

These are the durations for one-shot UI transitions, in `theme::tokens::Motion`.
Chart reveals and morphs use the separate data-visualisation cadence below:

| Role | Duration | Band | Used for | Status |
| -- | --: | -- | -- | -- |
| `Instant` | 50ms | 0–80ms | Press acknowledgement, focus, simple visual feedback | Shipped |
| `Interaction` | 150ms | 120–160ms | Hover, selection, menu appearance | Shipped |
| `Layout` | 200ms | 160–220ms | Panel open/close, disclosure, dialog entrance, contextual transitions | Shipped |
| `Travel` | 260ms | 220–320ms | A surface moving as a whole, composer dock, send morph | Shipped |

`Instant` and `Interaction` are Zed's own values (`AnimationDuration::Instant` /
`Fast`), so a surface ported from Zed keeps its cadence. `Layout` is Orbit's: Zed's
`Slow` is 300ms, which reads as hesitation on a workbench panel. `Travel` is Orbit's
too: a card crossing 200px in 150ms reads as a teleport, and in 260ms it reads as
travel.

- **Easing is part of the role.** `Instant` is linear (it is feedback, not travel);
  `Interaction`, `Layout`, and `Travel` decelerate (ease-out quint). Nothing bounces,
  nothing overshoots.
- **Streaming is not a role.** Content updates are driven by incoming state and must
  not introduce layout animation; nothing animates text position.
- **`Travel` moves a card, never a row.** Hover, press, focus, and selection are
  bounded by `Interaction`; only a floating surface, or a control that changes both
  size and meaning, may travel.

### The interaction layer

The target contract is for interactive surfaces to share state treatments and
transitions through `app/helpers.rs`, rather than deciding them per component.
The primitives ship, but adoption is still in progress; some surfaces still
swap states instantly rather than transitioning.

| Interaction | Where it applies | Role | What moves | Reduce Motion |
| -- | -- | -- | -- | -- |
| Hover in | row, button, icon button, menu entry | `Interaction` | fill and ink crossfade out of the painted value | both swap at once |
| Hover out | the same | `Interaction` | the same fade in reverse, from what is on screen | both swap |
| Press | button, icon button, chip, segment | `Instant` | the pressed ramp; the toolkit offers no transform for a box, so a press cannot scale | the ramp still applies |
| Release | the same | settle | back to rest, no overshoot | snap to rest |
| Focus | field, row, card, dialog | `Instant` | the focus ring fades in; the control reserves that 1px border at rest so no layout moves | ring appears at once |
| Selection | row, tab, segment | `Interaction` | selected wash and ink crossfade, on top of the hover's own fade (`motion::control` owns both) | swap |
| Disclosure | chevron, expandable section | `Interaction` | the chevron turns a quarter turn (`helpers::disclosure_chevron`); the section's height is measured at the current width, then eased | rotation completes instantly, height snaps |
| Picker caret | chip, dropdown trigger | `Interaction` | the trailing caret points down at rest and turns a half turn to point up while the menu is open (`helpers::dropdown_chevron`); the menu opens below the trigger | rotation completes instantly |
| Switch | settings toggle (`helpers::switch`) | `Interaction` | the knob travels 18px (the track's inner span) and the track crossfades `surface_elevated` → `accent` | the knob jumps |
| Menu / popover open | anchored card, context menu | `Interaction` + travel | fade plus an 8px travel out of the anchor edge, as one card; `anchored_popover_at` (a context menu at the pointer) fades only, because it has no edge | the card appears at once |
| Dialog open | modal card | `Layout` | the card fades in over the scrim; it cannot scale for the reason below | the card appears at once |
| Panel open / close | sidebar, side pane, explorer, terminal, Git | `Layout` / `Spring` | the panel's own width; chips on its edge ride the same curve | panels snap |
| Send morph | composer submit | `Travel` (state) + `Interaction` (hover) | the square grows into a labelled button that names the action, and folds back to a square while a turn runs; width and label ride one transition | the end state is drawn immediately |
| Composer dock | first send from the new-task page | `Travel` | the composer leaves the page's centered column for the session's: the space below it closes, the column widens 800 → 960, the folder row collapses, and the transcript's ink fades in, all on one clock (`new_task::settle`) | the session frame is painted on the frame of the send |

Four rules hold across the table:

- **One answer per interaction.** New work uses the shared treatment; existing
  surfaces still using an instant swap are migration work, not a separate standard.
- **Interruptible.** A reversal restarts from the last painted value, never from the
  role's start. That is why a retargetable primitive carries a generation key: a fresh
  element id snaps the value back to `from`.
- **Motion never gates the action.** The new state is committed before the frame is
  requested, so a suppressed or interrupted transition cannot swallow a click.
- **Hover, press, focus, and selection never exceed `Interaction`.** Only a surface
  that moves across the window, or a control that changes size and meaning, may use
  `Travel`.

`anchored_popover` supplies its entrance motion. `modal_card` supplies the
modal frame, not an animation: callers compose it with entrance motion, as in
`dialog.rs`. Neither helper alone establishes the complete focus and dismissal
contract.

Two cells above are shaped by the toolkit rather than by taste, and both are recorded
because a future upgrade can reopen them:

- **A press cannot scale.** gpui 0.2.2 has no transform for a box: `with_transformation`
  is an `Svg` method and `Style` carries no transform field. A press therefore reports
  through its ramp — a real but quiet state change — instead of shrinking by 1.5 %.
- **A focus ring cannot fade unless it is reserved.** A ring that is absent at rest
  costs a 1px border the moment it appears, which moves every sibling. A control that
  wants the fade reserves the border at rest and fades its colour.

### Primitives

Every treatment above resolves to one of five primitives, kept together in
`src/motion/` so a call site composes them rather than inventing a curve:

- **`Transition`** — `from` / `to` / `started` / `generation`. `retarget()` samples the
  value currently on screen and fades from there, so a reversal resumes instead of
  snapping. `SectionHover` in `transcript_view.rs` is the shape it generalises.
- **`Spring`** — critically damped, velocity preserved across a retarget, exact settle
  within 0.2px. It reaches the target without passing it, which is what keeps it inside
  *nothing bounces*.
- **`Disclosure`** — measures the child at the current width and interpolates its
  height, so a section that grows while the window is being resized eases from a real
  measurement instead of an estimate.
- **`Travel`** — fade plus an offset out of an anchor side, for popovers and dialogs.
- **`bezier(t, x1, y1, x2, y2)`** — the one easing-curve builder; `Motion::easing` and
  `chart::ease` both resolve through it.

A frame-driven primitive owns its own start instant and is sampled at paint; it does
not start a timer that competes with the display. Each exposes its curve as a pure
function, so the shape is asserted without a window: monotonic, bounded, settle exact,
and identical at 60, 90, 120, and 144 Hz.

### Data visualisation

Charts do not use the transition roles. `theme::tokens::chart` owns their
cadence — `DRAW` 900 ms, `MORPH` 500 ms, `REVEAL` 1100 ms — and their symmetric
cubic easing. A chart reveal is the data arriving, not a control changing state,
so the durations sit deliberately outside the `Motion` bands and must never be
used for chrome. A test asserts they stay longer than every transition.

### Reduce Motion

Reduce Motion is a property of the role, not of each call site: `Motion::duration`
returns zero when motion is suppressed, so a transition collapses to the new state
instead of disappearing. A looping affordance keeps its glyph and drops the motion.

The preference is three-way — `System`, `Reduced`, or `Full` — and **the in-app
choice is authoritative**:

| Setting | macOS Reduce Motion | Result |
| -- | -- | -- |
| `System` (default) | off | motion |
| `System` (default) | on | suppressed |
| `Reduced` | either | suppressed |
| `Full` | either | motion |

`System` is the default, so a user who turns Reduce Motion on system-wide does not have
to find Orbit's switch. `Full` exists because the OS value can arrive without a
deliberate choice — a migrated profile or a preset — and an app-level setting the user
changed on purpose must not be silently overruled in the other direction either.

The OS read is `NSWorkspace.accessibilityDisplayShouldReduceMotion` on macOS, cached
and re-read at launch and whenever a window becomes active: the preference can only
have changed while the user was elsewhere. Linux and Windows expose no equivalent read,
so `System` and `Full` both resolve to motion there and `Reduced` is the only way to
suppress it.

Surfaces read `theme.reduce_motion()` and never the raw preference, so the three-way
resolution happens in exactly one place — `MotionPreference::suppresses_motion` in
`theme.rs`. Settings → Accessibility names the source in force ("Following the system",
"Motion suppressed", or "Animating anyway — the system asks for reduced motion"), so the
resolved state is never a guess. A legacy `reduce_motion: bool` in `ui.json` migrates to
`Reduced`, and both keys are written for one release so a downgrade keeps the choice.

### Loops

Loops report live state, with one documented decorative exception for the
new-task backdrop below.

- **State loops (allowed):** the running-session shimmer (a session is running), the
  spinner and refresh glyph (work is in flight), the streaming pulse (output is
  arriving). Each freezes or stops when the state it reports ends, and each honours
  Reduce Motion.
- **The one non-state loop:** the new-task backdrop's dither field, whose screen drifts
  so dots cross their Bayer thresholds a few at a time (~2% of the cells a second), and
  whose light crosses the monogram on a twenty-second pass. It is the page's texture
  rather than an event, it is drawn behind everything, it never moves a glyph or a box,
  and Reduce Motion freezes it on the frame it was showing.
- No other decorative looping animation. No parallax, no overshoot, no bounce; a
  critically damped spring is allowed only where the interaction layer names one.
- Loading animation must communicate actual work, not elapsed time.

### Motion Rules

- **A surface picks a role, never a number.** A one-shot UI transition uses
  `Motion::<role>.duration(&theme)`; charts use their separate cadence.
- **Never call `.nominal()` outside `theme/tokens.rs`.** It skips the Reduce Motion
  guard, and the guard belongs to the role rather than to the call site.
- **Never hand-roll a state transition at a call site.** A hover, press, focus, or
  selection treatment that is not in the interaction layer is a second answer to a
  question the layer already answers; add it to the layer instead.
- **Interruptible, always.** Reversing mid-flight resumes from the painted value; a
  transition that restarts from `from` is a snap with extra steps.
- **Motion never gates an action.** The new state is applied even if the frame never
  paints.
- **Do not animate large areas when a small state transition is sufficient.** A hover
  fades a wash; it does not move the row.
- **Nothing animates on first paint of existing content.** No stagger-in, no list
  entrance, no scroll-triggered reveal.
- **The transcript is exempt from every entry animation.** Its motion is the stream;
  a word-level fade on arriving text is part of the stream, not an entrance.

## Agent Interaction States

Agent activity is a core product state and must be visually legible without relying on color.

### State Model

`SessionStatus` (`app/session_status.rs`) is the shipped model. Every state resolves
into one **status lane** — a 16px slot at the leading edge of the row's state line —
and each lane carries a glyph, and where it matters a word, so colour is never the
only signal:

| `SessionStatus` | Lane | Glyph | Words |
| -- | -- | -- | -- |
| `Idle` | Idle | — | — |
| `Running` | Progress | turning spinner | — |
| `Retrying` | Progress | turning spinner | — |
| `Compacting` | Progress | turning spinner | — |
| `WaitingApproval` | Blocked(Approval) | lock | "Needs approval" |
| `WaitingInput` | Blocked(Input) | chat | "Needs input" |
| `Completed` (read) | Done | check | — |
| `Completed` (unread) | UnreadDone | eye | "Needs review" |
| `Failed` | Failed | cross | "Failed" |
| `Failed` (unread) | UnreadFailed | cross + amber corner | "Failed" |
| `Stopped` | Stopped | stop | — |
| `Disconnected` | Failed | cross | "Failed" |

The lane keeps its width whether or not it paints, so every row's text starts at the
same x and a state change never reflows the list. "Read" and "unread" are separate
from execution on purpose: reading a result does not undo its completion.

Streaming is not a state in this table — it is what `Running` looks like while
content arrives, and it is bounded by the rules below.

### Streaming

Streaming should feel continuous without causing the interface to jump.

- Coalesce frequent updates.
- Preserve scroll position unless the user is already following the bottom.
- Never steal scroll position from a user who has manually scrolled upward.
- Keep the composer stable while output streams.
- Tool activity should update in place rather than repeatedly creating new cards.

### Tool Activity

Tool calls are operational information, not decorative chat bubbles.

- Group related activity when appropriate.
- Show tool name, meaningful status, and relevant output.
- Collapse verbose output by default.
- Make failure states immediately discoverable.
- Allow expansion without leaving the current session.
- Preserve a compact representation in long transcripts.
- An expanded command's input and output share **one recessed code plane**
  (`code_bg`, the same treatment a markdown code block takes), divided by a
  hairline. Sections read top-to-bottom — command, then output — the way a
  shell transcript does; they are not stacked cards, and each carries its
  label and copy control on one shared axis.

### Approval

Approval requests are high-priority contextual states.

- Clearly state what requires approval.
- Provide the available actions explicitly.
- Do not hide the approval action inside a generic menu.
- Keep the underlying session context visible.
- Restore focus to the session after the decision.

## Transcript and Conversation UX

The transcript is one workspace surface, not the definition of the application.

### Message Hierarchy

Separate:

- User intent.
- Agent response.
- Tool activity.
- System/session state.
- Approval/input requests.

Avoid making every item look like a chat card.

### Long Sessions

Long transcripts must remain scanable.

- Virtualize long lists.
- Preserve stable message anchors.
- Use compact metadata.
- Collapse verbose tool output.
- Provide in-transcript find.
- Do not repeatedly repaint unaffected content.
- Keep timestamps and secondary metadata visually subordinate.

### Composer

The composer is a primary work surface.

- Use `input::composer_radius` (fixed 12px) and the composer surface; frost it only
  with enabled, supported wallpaper glass, falling back to opaque otherwise.
- Keep the input visually distinct from the transcript without making it look like a
  floating SaaS widget.
- **The new-task page is the same composer in the middle of the page.** There is no second
  composer and no second page template: the chat column with an empty transcript centers the
  column it already owns, at `new_task::COMPOSER_MAX_W` (800px, still narrowing with the
  window) instead of the session's 960. It is centered by *weight*, not measurement — the
  transcript's slot above takes 44 % of the free space and a spacer below takes the rest —
  because the composer's height moves with its content and the UI scale. Nothing is drawn
  above it: no headline, mark, or card. The field and its context explain the task.
- **The context header shares the composer surface.** A hairline separates it from the input.
  The folder and branch controls sit together on the left; the working-directory control sits
  at the right when a repository is available. The folder chip shows the folder name, project
  mark or folder glyph, and caret; its tooltip and picker expose the full path. It remains a
  keyboard button: Tab reaches it, Enter/Space opens the picker, and Escape returns focus. The
  branch and working-directory controls reuse the session's existing pickers. The status row
  waits for the dock to finish before taking the controls back, so they are not duplicated
  mid-transition. The composer border remains a quiet hairline while its input is focused;
  the caret indicates text focus. The empty footer leaves out the
  context meter until real usage exists rather than decorating a task with a zero-valued ring.
- **The first send docks it.** One `Motion::Travel` (260ms) carries the composer from the page's
  centered column into the session's: the gap below it eases to nothing, the column widens from
  800 to 960, the folder row collapses, and the transcript's ink fades in. All four read the same
  role and easing, from `new_task::settle`, so the page cannot arrive in pieces. The slide uses a
  gentle cubic ease-out and starts on the first visible frame, rather than expiring before paint.
  Reduce Motion paints the session frame on the frame of the send. A session resumed from the sidebar
  never animates: it was never on this page. See `the_send_transition_is_continuous_at_both_ends`
  and `reduce_motion_docks_the_composer_on_the_frame_it_sends`.
- Commands and file mentions remain content tokens.
- Access and workflow stay visible at every composer width. When space is tight,
  the mode controls wrap above the model, thinking and send controls rather than disappearing.
- Primary submit/stop controls remain discoverable.
- Model, thinking effort, access mode, and workflow mode should be available without
  turning the composer into a control dashboard. Four chips is the ceiling at rest, and
  each one carries a rule for when it is not on the row at all:
  - **Access mode** shows its name only when it is *not* `AccessMode::default()` (Full
    access). At the default the chip keeps the glyph alone and names the mode on hover:
    the label is the warning, and the default is not a warning. The rule reads the mode
    and never the open picker, so opening a picker cannot resize the row.
  - **Thinking effort** is absent unless the active model reports a ladder with a choice
    in it. `can_choose_thinking` (`available_thinking_levels.len() > 1`) is the single
    predicate behind the chip, the ⌘T shortcut, and the palette entry, so the three can
    never disagree about whether the picker has options.
  - **Workflow mode** and **model** stay on the row in every state; both are the live
    state of the run.
  - **Steer** appears only while a run is in flight with something to send.
- **The model picker is one searchable list, not a dashboard.** Search, then a quiet
  horizontally scrolling scope rail (All, Recents/Favorites, then provider chips); the
  selected provider scrolls to the rail's centre, and the rail's left/right edge shadows
  grow over the first 28px of travel instead of switching on. Every chip carries its mark —
  the provider's brand icon, or the shared grid/clock/star glyph for All/Recents/Favorites.
  Provider-grouped one-line
  rows carry the model name and its context window, with the provider mark and name stated
  once in the group header. The current model keeps the row fill and an accent check;
  favorites keep their star, which only appears on hover for the rest. The footer is a
  single quiet count, not a repeated key legend. Arrow keys, Enter, and Escape still work
  without the legend. Long names truncate; the full provider/id is the row's tooltip.
- Advanced controls belong behind contextual disclosure when they are not needed. A
  control that disappears keeps a usable keyboard path; a palette route alone
  does not establish that the complete workflow is accessible.
- Attachments sit in one tray above the input, one chip per item (image, file, or
  collapsed paste) with its own always-visible remove button — a chip large enough to
  read a file name is large enough to delete by accident. The tray names its count and
  offers one **Clear** action only once it holds more than one chip; a single chip is its
  own explanation. Every chip leads with the same 20px slot — the image thumbnail, or the
  file type's devicons mark — so the three kinds line up and a file never reads as a bare or
  undersized mark. The mark is drawn bare: no tinted square is painted behind it. The chip
  is already the highlighted surface, and a second rounded square behind a glyph whose art
  is itself a box reads as an avatar monogram rather than a file icon. Glyph artwork also
  sits off-centre inside its font cell, so the mark is nudged by its *ink* (the text
  system's typographic bounds) rather than centred by its line box. When no Nerd Font or no
  glyph is available the slot carries the type's short label (`MD`, `PDF`, `ZIP`, …) on a
  tinted square instead, stepped down for longer labels. Chip text sits in one line box at
  `line_height(1.)`, the same box every button uses, and the chip's insets are symmetric, so
  the mark and the path share one centreline with balanced edges. Chips are buttons by role (button radius, hairline, elevated surface),
  never pills, and a chip's hover describes the item while its remove button names the
  removal.

## Explorer and File UX

### Explorer

The file tree is a navigation instrument.

- Prefer indentation and whitespace over nested cards.
- A file open in the viewer uses `surface_selected`; during inline rename the
  row uses `surface_active` for editing. An unselected keyboard-cursor row uses
  `overlay`, and an unselected, non-cursor row gets hover treatment. When the tree
  has focus and its cursor is on the open file, a focus ring overlays the selected
  fill without changing row layout; it is omitted during inline rename.
- Git status uses semantic indicators plus text/icon where necessary.
- Hidden files remain controlled by an explicit setting.
- Directory expansion should be keyboard accessible.
- Quick-open should complement, not replace, the tree.

### File Editor

The editor is a primary workspace.

- Code uses the mono token family or user-selected code font.
- Tabs remain compact.
- Dirty state is visible without overpowering the filename.
- Save/conflict states are explicit.
- Binary, oversized, and non-UTF-8 states must be honest and actionable.
- Editor chrome should remain quieter than the code itself.

### Diff

Diffs prioritize comprehension.

- Added/removed content is differentiated by semantic treatment and non-color cues.
- File-level metadata remains compact.
- Actions are contextual.
- Line numbers and code remain the strongest visual anchors.
- Do not wrap every hunk in a card.

## Git and Review UX

Git surfaces should feel like workbench tools rather than dashboards.

- Branch, status, changed-file count, and review state use compact metadata.
- Changed files form a navigable list.

## Data Visualization

Charts are information surfaces, not decoration.

- Use one accent ramp for a single series.
- Multiple series use neutral differentiation or semantic colors only when the data
  requires categorical distinction.
- Never introduce arbitrary colors merely to make a chart visually richer.
- Grid lines remain subordinate.
- Tooltips provide exact values.
- Axes and units must be explicit.
- Missing data is represented as missing, never zero-filled without a documented reason.
- Hovering a point may reveal detail but must not permanently alter layout.

## Empty, Loading, and Error States

### Empty

An empty state explains:

1. What is empty.
2. Why it may be empty.
3. What the user can do next.

Avoid decorative illustrations in core workbench surfaces.

### Loading

Loading states represent actual asynchronous work.

- Prefer skeletons only where the final structure is known.
- Prefer progress/status text for agent and process work.
- Do not show indefinite spinners for operations whose state can be described.

### Error

Errors must be actionable. Use `helpers::error_state` when no resolving control
is adjacent; its card answers three questions in order:

| Slot | Carries |
| -- | -- |
| `title` | What failed, in the user's terms ("Could not update providers") |
| `detail` | The underlying message, wrapped — never truncated, never a raw code |
| `hint` or `action` | What to do next: a button when there is something to press, otherwise one sentence that names the step |

- State what failed.
- Preserve relevant context.
- Explain the next available action.
- Avoid generic "Something went wrong" as the only message.
- Never fabricate recovery state.
- Distinguish connection/process errors from user-action errors.
- An error a user cannot read is not an error message: `runtime_error` wraps
  rather than truncating, and no error surface carries a bare string.
- An **inline** error is legitimate where the control that resolves it is on the
  same row or in the same footer — a validation line under a field, a provider
  row's error beside its Remove button, a dialog's error above its actions.
  Anywhere else it is the card, because a string with no adjacent action is a
  dead end.

## Accessibility

Accessibility is part of the component contract.

- Every interactive element must have an accessible name, role and relevant state.
  GPUI 0.2.2 does not provide a ready-made native accessibility tree API; native
  screen-reader semantics remain an implementation gap until a platform bridge is
  implemented and verified. The shared switch also lacks keyboard activation and
  native naming today. These limitations are not exemptions from the requirement.
- Focus must remain visible.
- Do not encode meaning with color alone.
- Maintain sufficient contrast for text and controls.
- Hit targets must remain usable at compact density.
- Keyboard navigation must reach all primary functionality.
- Tooltips cannot be the only way to access essential information.
- Respect reduced motion.
- Dynamic agent state changes should be announced appropriately without flooding the
  accessibility tree.

## Component Contracts

Every reusable GPUI component must define its applicable contract; the presence
of a contract does not mean the current implementation has already satisfied it:

```text
Component
├── anatomy
├── variants
├── states
├── keyboard behavior
├── pointer behavior
├── focus behavior
├── accessibility semantics
├── motion and reduced motion
├── loading, error and empty states where applicable
├── bounds, clipping and long-content behavior
└── data contract
```

A component is not considered complete when it only has a visual default.

### State Matrix

Every interactive component resolves its states through
`interaction::InteractionState` (see §Interaction System). At minimum a component
accounts for:

- default (`Rest`)
- hover (`Hover`)
- focus (`Focused` — the 1px `focus_ring` border)
- active/pressed (`Pressed`)
- selected (`Selected`)
- disabled (`Disabled` — non-interactive, not faded)
- loading (the spinner/shimmer roles; must not shift layout when it resolves)
- error (semantic role **and** a word or icon)
- destructive (separated from the primary action; never the default focus)
- keyboard navigation (the same states, reached without a pointer)

A component that defines its own colours or radii for these states is incomplete,
even if it looks right: it has forked the contract.

## Card Budget

Cards are reserved for conceptually singular or elevated objects.

Use cards for:

- composer
- metric board
- important grouped information
- floating/contextual surfaces

Prefer hairlines, whitespace, indentation, and hover states for:

- navigation
- transcripts
- tool activity
- file trees
- dense tables
- lists
- settings rows

Never build:

```text
card → card → row → card → button
```

If hierarchy can be communicated through spacing and typography, do not add another container.

## Design Tokens and Theme Architecture

All visual values must flow through the theme system.

- Never hardcode colors in components.
- Never hardcode semantic state colors where a theme role exists.
- Add a semantic role when a new visual meaning is required.
- Keep palette-specific values inside `Palette` / `Theme`.
- Components consume semantic roles rather than palette implementation details.
- User-selected fonts remain part of the supported theme/configuration system.
- New components must work across dark and light palettes before being considered complete.

### Token Naming

Prefer semantic names:

```text
background
surface
surface_secondary
surface_elevated
surface_hover
surface_active
border
border_subtle
text_primary
text_secondary
text_muted
text_disabled
accent
accent_hover
accent_pressed
accent_fg
accent_text
selection
selection_fg
focus_ring
success
warning
error
error_hover
info
```

Specialized roles (`code_bg`, `syn_*`, `overlay`, `border_strong`, `toggle_knob`)
exist alongside the vocabulary; they are read through the same `Theme`.

Avoid component-specific color names such as:

```text
blue_button
dark_card
orange_row
```

Semantic tokens allow the same component grammar to survive palette changes.

### Anatomy

Four tiers, one direction. A surface reads tier three or four, never tier one.

```text
Palette      per theme id, raw hex             theme.rs
   ↓
Theme        the 40 semantic roles            theme.rs
   ↓
tokens       geometry · state · motion        theme/tokens.rs
   ↓
frames       shared component shells          app/helpers.rs
```

- **`Palette`** holds raw values only and is never read by a surface.
- **`Theme`** is the only thing a surface paints from: `theme.surface_hover`,
  `theme.text_secondary`, `theme.focus_ring`.
- **`tokens`** maps those roles onto geometry, state and motion:
  `DynamicSpacing::Base08.px(&theme)`, `panel::RADIUS.px(&theme)`,
  `InteractionState::Hover.fill(&theme)`, `Motion::Interaction.duration(&theme)`.
- **`frames`** are the shared shells: `button_frame`, `icon_button_frame`,
  `input_field_frame`, `context_menu_surface`, `segmented_segment`, `empty_state`.
  A new surface composes these; it does not re-derive their geometry.

### Palettes and design systems

Twelve palettes, two modes, two palette families. Geometry is shared across all of
them; `themes_for_system(mode, system)` is the single ordering both the dropdown and the
click resolve through. The "design system" control selects which family you are
choosing from — the shapes do not change with it.

| System | Dark | Light |
| -- | -- | -- |
| Classic | Orbit, Orbit Darker, Orbit Cobalt, Orbit Azure, Flexoki | Orbit Light, Orbit Azure Light, Orbit Paper, Orbit Contrast, Flexoki Light |
| Operational | Operational | Operational Light |

`AppearanceMode::System` follows the OS appearance live; an explicit Light or Dark
choice ignores it. The choice and the remembered palette per system both persist.

These families share the non-color component grammar in the Orbit design skill.
Components must resolve semantic roles from the active `Theme`, not reuse the Azure
reference colors in this document. Appearance also provides a Default accent plus
Blue, Coral, Orange, Lime, Shu, Kaki, Sakura, Matcha, Asagi, Ruri, Fuji, Kogane,
Ume and Gin. Accent overrides change interaction roles independently of the selected
palette; the named custom accents also derive selection and inline-code treatments.
Wallpaper and its optional extracted accent are separate appearance choices. Validate
actual text, state and focus contrast against the resolved theme, including overrides;
selection of a family does not guarantee every possible combination is verified.

### Geometry reference

Spacing scales with UI font size and the Spacing Density setting; other values
follow their own tokens. Radii are fixed pixel values plus the corner-rounding
preference, except `None` and `Full`, which do not change. The radius values
below are before that preference.

| Role | Value | Token |
| -- | -- | -- |
| Control radius | 5px compact / 6px standard | `button::radius(size)` |
| Input radius | 6px | `input::RADIUS` |
| Chat composer radius | fixed 12px | `input::composer_radius` |
| Panel radius | 8px | `panel::RADIUS` |
| Row / menu-entry radius | 5px | `list_item::RADIUS` |
| Badge radius | 4px | `Radius::Small` |
| Control heights | 16 / 18 / 22 / 28 / 32px | `ButtonSize` |
| Icons | 10 / 12 / 14 / 16px | `IconSize`, or `ButtonSize::icon_size` inside a button |
| Type | 11 / 12 / 14 / 16px | `TextSize` |
| Spacing | 0, 1, 2, 3, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48px | `DynamicSpacing` |
| Separators | 1px | `BORDER_WIDTH` |
| Focus ring | 1px border | `FOCUS_RING_WIDTH` |
| Motion | 50 / 150 / 200 / 260ms | `Motion` |

### Verification

The token layer is asserted, not assumed. A palette or role that breaks a rule fails
these tests, which run over every shipped palette:

- Colour — `every_theme_has_readable_foregrounds`,
  `interactive_and_muted_foregrounds_clear_aa`,
  `selection_rows_keep_their_metadata_readable`,
  `every_theme_holds_its_surface_grammar`.
- Interaction states — `interaction_states_never_collide` (hover, pressed and selected are
  three different colours in all thirteen palettes).
- Motion — `motion_roles_fill_the_spec_bands`,
  `motion_collapses_to_nothing_under_reduce_motion`,
  `the_motion_preference_resolves_in_one_place`.
- Micro-interactions — the layer's adoption gate, landing with it:
  `every_control_state_resolves_to_a_role`, `a_reversal_resumes_from_the_painted_value`,
  `a_spring_settles_without_overshoot_at_any_refresh_rate`,
  `a_disclosure_measures_at_the_current_width`, `travel_moves_a_card_never_a_row`,
  `popovers_enter_through_the_chokepoint`.
- Geometry — `default_settings_resolve_to_zeds_default_pixels`,
  `elevation_shadows_match_zed`.

`scripts/check_motion.py` enforces the two negative rules mechanically: no animation
types its own duration, and no file outside `theme/tokens.rs` calls `.nominal()`.
`scripts/check_states.py` enforces that a call site does not name a state colour, which
is the same contract one layer down.

A change to this file that is not also a change to those tests is a wish, not a rule.
The micro-interaction set is the one deliberate exception: it is the gate the
layer must pass before it is called shipped, not a description of today.

## Quality Bar

A new Orbit surface is ready when:

- Its hierarchy is understandable within a few seconds.
- The primary action is obvious without being visually loud.
- Hover, focus, selection, disabled, loading, and error states are defined.
- Keyboard interaction is supported.
- The surface works in both dark and light themes.
- No unnecessary cards, borders, gradients, or shadows were introduced.
- Content remains readable at Orbit's compact density.
- Long-running or streaming states do not cause layout instability.
- Color is not the sole carrier of meaning.
- All colors and dimensions come from the design system.
- The surface feels native to the existing workbench rather than like a separate mini-product.

## Design Non-Goals

Orbit should not become:

- a web dashboard translated into GPUI;
- a ChatGPT clone;
- a glassmorphism interface;
- a neon AI interface;
- a card-heavy SaaS admin panel;
- a permanently animated interface;
- an IDE clone with unnecessary chrome;
- a collection of disconnected page-specific design systems.

The goal is not maximum visual novelty.

The goal is a coherent native workbench whose interaction quality, information density,
performance, and visual restraint make long agent sessions feel natural.

## Do's and Don'ts

### Do:
- **Do** read every color, size, and radius from `theme::get(cx)`; add a role to
  `Palette` when a new semantic need appears so all thirteen palettes stay legible.
- **Do** use hairlines and whitespace to separate sections; reserve rounded bordered
  surfaces for objects that are conceptually singular (composer, metric board, popover).
- **Do** keep control heights on the `ButtonSize` scale (22 / 28 / 32px) so a
  900px-tall window shows 15+ table rows.
- **Do** mark unavailable data in words ("Cost unavailable"): zero and unknown are
  different facts.
- **Do** pair color with a non-color signal (↑/↓, icon, label) for any state.

### Don't:
- **Don't** introduce a second accent hue; categorical series use one accent ramp
  (100% → 62% → 40% → 22% wash) plus neutral inks.
- **Don't** wrap every row in a card, nest a card inside a card, or use a card where a
  hairline-separated section reads better.
- **Don't** use display fonts, decorative gradients, glass, glow, or emoji in the
  chrome.
- **Don't** animate decoration except for the documented new-task backdrop loop.
  One-shot UI transitions use a `Motion` role and confirm state changes; chart
  reveals and re-scales use the separate chart cadence.
- **Don't** write a transition inline at a call site, and don't leave an interactive
  state with no transition: an instant swap next to a faded one is two answers to the
  same question.
- **Don't** show a metric the underlying data cannot support — no invented latency,
  cost, or retry counts.
- **Don't** ship a chip that appears in every state "just in case". Give it a rule for
  when it is absent, and make that rule read the state the chip describes rather than a
  neighbouring control's open state.
- **Don't** give a control a permanent row merely to announce its own default. The access
  chip drops its label at Full access, and the MCP chip is absent while every server is
  connected. The "Work in" chip is an exception: at the main checkout it provides access
  to other working locations.
