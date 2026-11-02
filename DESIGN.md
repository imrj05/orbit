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
rounded:
  xs: "2px"
  sm: "4px"
  md: "6px"
  lg: "8px"
  xl: "12px"
  xxl: "16px"
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
    rounded: "{rounded.xxl}"
    padding: "8px 12px"
---

# Design System: Orbit Pi

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
screenshot in any of the twelve palettes still reads as Orbit. Depth is delivered by
hairlines and tonal steps rather than shadows; shadows exist only for surfaces that
genuinely float above the page (composer, popovers, modals).

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
| Shadows belong to floating overlays only | `elevation_2` / `elevation_3` (`ElevationIndex`) for menus, popovers, modals | Shipped |
| Controls are 6px, panels 8px | `Radius::Large` 6px, `panel::RADIUS` 8px, for every palette | Shipped |
| Density over air: 28px rows, 13–14px body | `ButtonSize::Medium` 28px, `list_item` rows, `TextSize::Default` | Shipped |
| Accent is identity, never a button fill | `accent` marks selection, focus, tints; primary fills read `action` | Shipped |
| One type family, 400–600, no tracking | `TextSize` 10–16, weights 400/500/600 | Shipped |
| Motion is fast and decelerating | `Motion::Instant` / `Interaction` / `Layout` | Tokens shipped; adoption is phase 3 |
| Every interactive surface answers the same states | `interaction::InteractionState` | Vocabulary shipped; adoption is phase 2 |
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

Both are derived in `Theme`, so all twelve palettes stay legible without per-palette tuning.

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

**The Palette Catalog.** `ThemeId::ALL` ships twelve palettes: Orbit's own family plus
Flexoki plus the two Operational palettes. The Classic system keeps five dark (Orbit,
Orbit Darker, Orbit Cobalt, Orbit Azure, Flexoki) and five light (Orbit Light, Orbit
Azure Light, Orbit Paper, Orbit Contrast, Flexoki Light) palettes. The blue design is
**Orbit Azure** (dark) and **Orbit Azure Light**; the original ember palette is simply
**Orbit** / **Orbit Light**. Settings → Appearance → **Theme** is a single control, not
one row per appearance mode: it shows and edits the palette currently in effect, listing
only the palettes of that mode and design system. `themes_for_system(mode, system)` is
the single ordering the dropdown paints and a click resolves, so a row index always maps
to the palette it painted. Retired editor themes still load from a saved preference:
dark keys fall back to Orbit, light keys to Orbit Light.

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
| `surface_hover` | `#1B1D20` | `#EEEEEA` |
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
`Radius::Small` for badges, chips, and checkboxes. A large multi-line composer is still
a field: it takes `input::composer_radius`, which is the field's own corner. Fully round
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
**The Size-Not-Weight Rule.** Sizes step 10/11 → 12 → 14 → 16, with 15 for page titles and
20 for metric numbers; weights use only 400, 500, 600. No 700+, no letter-spacing tricks,
no tracking below -0.01em.

**The Tabular Figure Rule.** Any column of numbers uses fixed-width rendering and is
right-aligned; compact forms in the main UI (`18.4M`), exact forms in tooltips and
detail tables (`18,421,842`).

## Layout

A three-part shell: a fixed sidebar column (200–320px, default 248px), the content
plane, and an optional right pane. The content plane is full-bleed with a centered
inner column (960px for reading surfaces, 1180px for data surfaces) and 20px gutters.

Vertical rhythm: 44px header rows with a bottom hairline, 20px between sections, 12px
inside a section's header-to-content gap, and 6–8px between dense rows. Long lists are
virtualized (`uniform_list`) and scroll inside the plane — the page itself never scrolls
as a whole when a table is present.

Responsive behavior is structural, never fluid type: grids drop columns (4 → 2), paired
columns stack into one, and the sidebar hides before the content squashes.

## Elevation & Depth

Flat by default. Depth comes from tonal steps (canvas → raised) and 1px hairlines.
Shadows belong to surfaces that float above the page, and each reads one authored
layer stack from `ElevationIndex`:

### Elevation Vocabulary
- **`ElevationIndex::ElevatedSurface`** (`elevation_2`, one tight layer): anchored
  menus, dropdowns, tooltips, popovers.
- **`ElevationIndex::ModalSurface`** (`elevation_3`, one restrained layer): modals,
  dialogs, the command palette.
- **`ElevationIndex::Background` / `Surface` / `EditorSurface`** cast nothing. A panel
  is a tonal step and a hairline, not a shadow.
- **`Theme::composer_shadow`** (one tight contact layer): the composer box and the usage
  metric board — objects that sit on the page rather than in it. This is the single
  non-floating shadow.

### Named Rules
**The Border-Or-Shadow Rule.** A surface declares one: either a 1px `border`
hairline or an elevation shadow. Never both, or it reads as a ghost card.
(`border_strong` is a focus/hover/selection edge, not a resting card edge.)

**The Rectangle-Clip Rule.** gpui 0.2.2 clips `overflow_hidden` to a rectangle,
not the border radius (`Style::overflow_mask` builds a `ContentMask` from
`bounds` alone), so a child background that reaches a rounded container's edge
paints square corners over it. Such a child must carry the matching radius
itself — `rounded_t` on the first, `rounded_b` on the last — or the container
must be a popover whose content is `deferred` (deferred draws bypass ancestor
masks). Applies to the transcript cards, the code-block title strip, and
list-board rows (`settings_group`, the MCP server rows).

## Shapes

Corners are soft but restrained: 6px for navigation and session rows and
compact controls (≤24px icon buttons), 8px for chips, action buttons,
inline inputs, and data-list rows, 12px for grouped surfaces and boards, 16px for
the composer, `full` only for status dots, avatar monograms, the send button,
and the toggle-switch track. Nothing else is a pill.

Borders are 1px at `theme.border`; `border_strong` (ink at ~14% alpha) is reserved for
focus and hover emphasis. Charts and graphs draw with 1.5px strokes; grid lines are 1px
at low-opacity hairline. No drop-shadowed text, no gradients except the raised-button
fill (see Buttons).

### New-task backdrop

The new-task page is the one surface with a background treatment, and it has two forms
under one grammar — ordered dither, the same screen the wallpaper's `Dither` effect uses:

- **No image configured:** a painted halftone field (`backdrop`) — an 8×8 Bayer screen of
  four dot sizes over a soft density field, densest in a band behind the composer and
  calmer — never empty — toward the window's edges through a floored elliptical
  vignette, so the screen runs edge to edge and its drift is visible to the frame. It is
  drawn in
  `text_muted` at 55% (dark) / 62% (light), so it is paper texture, never a second content
  layer, and it works in all twelve palettes without a per-palette asset.
- **The monogram is screened into that field.** Where the app mark sits — between the
  empty-state column and the composer — the same screen takes over its shape: a coverage
  mask sampled from the shipped `app-icon.png` (never redrawn by hand, so the backdrop
  cannot drift from the icon), glyphs holding a quiet density at rest while a soft band
  of light crosses them every twenty seconds, filling each letter with the largest dot in
  turn. The mark paints one step quieter than the grain around it (`MARK_INK`), and a
  layer too small to hold it drops it rather than stretching a wordmark into mush.
- **A wallpaper configured:** the processed image (`wallpaper`), with an optional artwork
  effect (`None`, `Dither`, `ASCII`, `Halftone`, or `Scanlines`), an effect intensity, an
  effect pixel size, and a theme-coloured dim overlay. Hero mode paints it only in the main
  column; full-window mode paints it behind the sidebar too, with translucent surfaces above.

Both are the page's floor, not an illustration: nothing on the page depends on them, and
the chat page has neither.

## Components

### Buttons
- **Shape:** `button::radius(size)` — 6px on compact frames, 8px on the standard scale;
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
- **Style:** 28px tall, 8px radius, `surface_elevated` + hairline, 12px ink, optional leading
  Small (14px) icon and XSmall (12px) trailing chevron — `ButtonSize::icon_size`
  for the leading glyph, the chip-caret size for the chevron.
- **State:** open/selected = `surface_active` fill + `text_primary`; hover = `surface_hover`.

### Metric cells
- **Style:** a single bordered board (`surface_elevated` + hairline + 12px radius) whose cells
  are divided by 1px hairlines — never a grid of separate floating cards.
- **Content order:** 11px uppercase label → 20px number → 11.5px secondary line
  (comparison or average). Missing data reads "Unavailable", never a fabricated `0`.

### Cards / Containers
- **Corner Style:** 12px.
- **Background:** `surface`/`surface_elevated`; nested cards are forbidden.
- **Border:** 1px `border`.
- **Internal Padding:** 12–14px; lists use full-bleed rows with 8px inner padding and a
  hover fill instead.

### Inputs / Fields
- **Style:** composer-style box — `surface`, 1px hairline, 16px radius on the main
  composer (`input::composer_radius`), 8px on inline fields.
- **Focus:** the border steps to `focus_ring` (1px); a fill is never the focus signal.
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
  click on the scrim dismisses; a click on the card never reaches it. There is one
  scrim value for every modal — a dialog that wants a darker backdrop changes
  `scrim_modal`, not itself.
- **Card:** `elevation_3` (elevated surface, `border_strong`, one restrained shadow),
  the panel corner, a clipped column, and the UI font. Width and height bounds belong
  to the caller; everything else belongs to the shell.
- **Anatomy:** header (`modal::header_padding_*`, Small headline) → body → footer, with
  hairlines between. The footer holds the actions; a destructive action is separated
  from the primary one.
- **Keyboard:** Escape cancels, the primary action takes Return, and focus is trapped
  inside the card until it closes, then returned to the control that opened it.

A dialog that paints its own surface, corner, or backdrop is wrong even if it looks
plausible — that is how the worktree dialogs drifted to `surface_secondary` while every
other modal floated on `surface_elevated`.

### Navigation
- Sidebar rows: 28px tall, 6px radius, 14px icon (Small) in a fixed 20px slot, label in
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
  numeric columns right-aligned in tabular figures, first column left-aligned.
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
  twelve.
- A focused selected row adds the ring without changing its fill.

### The State Contract

Every interactive surface answers the same six states, and the answers come from
`theme::tokens::interaction::InteractionState` — not from the call site:

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
- A chordless command is registered with `command(…)`, which puts it in the palette and
  in no keymap. That is the intended shape for a chip's keyboard path, not a gap.
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
- **The "Work in" chip is always present in a repository.** It is not a readout of the default:
  it is the control that decides where the next task runs, and its rows include the places — a
  linked worktree, a new one, an existing folder, a remote branch — that have no other affordance
  in the composer. At the main checkout it reads `Local`; inside a worktree it names that
  worktree; on a broken worktree it says so instead of naming a branch. Outside a repository the
  chip is absent, and the plain folder chip takes its place — there is no location to choose
  between there. The chip never restarts the session: switching a location is a workspace move,
  and creating one is a dialog. See `the_work_in_chip_is_absent_outside_a_repository` and
  `the_chip_is_present_at_the_main_checkout_and_reads_local`.
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

Every control on the composer row is a `div` with a mouse handler, not a tab stop, so its
only routes are its chord and the command palette. A chip whose command has neither is a
setting a keyboard-only user cannot change at all — `composer_controls_are_reachable_without_a_pointer`
enforces this for the agent controls (model, thinking, workflow mode, access mode).
When a chord is not available, a palette-only command is the whole fix: `command()`
registers no keymap row, so nothing has to be invented to give a chip a keyboard path.

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

### Roles

A surface picks a **role**, never a number. The three roles are the only durations a
one-shot transition may use, and they live in `theme::tokens::Motion`:

| Role | Duration | Band | Used for |
| -- | --: | -- | -- |
| `Instant` | 50ms | 0–80ms | Focus, simple visual feedback |
| `Interaction` | 150ms | 120–160ms | Hover, selection, menu appearance |
| `Layout` | 200ms | 160–220ms | Panel open/close, disclosure, contextual transitions |

`Instant` and `Interaction` are Zed's own values (`AnimationDuration::Instant` /
`Fast`), so a surface ported from Zed keeps its cadence. `Layout` is Orbit's: Zed's
`Slow` is 300ms, which reads as hesitation on a workbench panel.

- **Easing is part of the role.** `Instant` is linear (it is feedback, not travel);
  `Interaction` and `Layout` decelerate (ease-out quint). Nothing bounces, nothing
  overshoots.
- **Streaming is not a role.** Content updates are driven by incoming state and must
  not introduce layout animation; nothing animates text position.

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

There are two sources, and either one suppresses motion:

- **Orbit's own toggle** — Settings → Accessibility → Reduce Motion.
- **The operating system's preference** — `NSWorkspace.accessibilityDisplayShouldReduceMotion`
  on macOS, read at launch and re-read whenever a window becomes active. A user who
  turns on Reduce Motion system-wide does not also have to find Orbit's switch.

Surfaces read `theme.reduce_motion()`, never `ui.reduce_motion`, so the system
preference cannot be half-honoured. Linux and Windows expose no equivalent read;
there the in-app toggle is the only input.

### Loops

A loop is sanctioned only when it reports live state. Everything else is decoration.

- **State loops (allowed):** the running-session shimmer (a session is running), the
  spinner and refresh glyph (work is in flight), the streaming pulse (output is
  arriving). Each freezes or stops when the state it reports ends, and each honours
  Reduce Motion.
- **The one non-state loop:** the new-task backdrop's dither field, whose screen drifts
  so dots cross their Bayer thresholds a few at a time (~2% of the cells a second), and
  whose light crosses the monogram on a twenty-second pass. It is the page's texture
  rather than an event, it is drawn behind everything, it never moves a glyph or a box,
  and Reduce Motion freezes it on the frame it was showing.
- No other decorative looping animation. No parallax. No spring-heavy motion.
- Loading animation must communicate actual work, not elapsed time.

### Motion Rules

- Do not animate large areas when a small state transition is sufficient.
- A transition is interruptible by the user's next action.
- Motion never gates an action: the new state is applied even if the frame never paints.
- The transcript is exempt from every entry animation. Its motion is the stream.

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

- Use the existing 16px radius and composer surface.
- Keep the input visually distinct from the transcript without making it look like a
  floating SaaS widget.
- Commands and file mentions remain content tokens.
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
- Advanced controls belong behind contextual disclosure when they are not needed. A
  control that disappears keeps a keyboard or palette path — add that path before hiding
  the control, not after.
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
- Selected files use `active`.
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

Errors must be actionable, and every one of them renders through
`helpers::error_state` — one card that answers three questions in order:

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

- Every interactive element must have an accessible name.
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

Every reusable GPUI component should define:

```text
Component
├── anatomy
├── variants
├── states
├── keyboard behavior
├── pointer behavior
├── focus behavior
├── accessibility semantics
├── motion
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

### Geometry reference

Every value below scales with the UI font size and the Spacing Density setting unless it
is pinned in px (hairlines, the 200px menu minimum width, scrollbar metrics).

| Role | Value | Token |
| -- | -- | -- |
| Control radius | 6px compact / 8px standard | `button::radius(size)` |
| Input radius | 6px; composer shares it | `input::RADIUS`, `input::composer_radius` |
| Panel radius | 8px | `panel::RADIUS` |
| Row / menu-entry radius | 6px | `list_item::RADIUS` |
| Badge radius | 4px | `Radius::Small` |
| Control heights | 16 / 18 / 22 / 28 / 32px | `ButtonSize` |
| Icons | 10 / 12 / 14 / 16px | `IconSize`, or `ButtonSize::icon_size` inside a button |
| Type | 10–11 / 12 / 14 / 16px | `TextSize` |
| Spacing | 0, 1, 2, 3, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48px | `DynamicSpacing` |
| Separators | 1px | `BORDER_WIDTH` |
| Focus ring | 1px border | `FOCUS_RING_WIDTH` |
| Motion | 50 / 150 / 200ms | `Motion` |

### Verification

The token layer is asserted, not assumed. A palette or role that breaks a rule fails
these tests, which run over every shipped palette:

- Colour — `every_theme_has_readable_foregrounds`,
  `interactive_and_muted_foregrounds_clear_aa`,
  `selection_rows_keep_their_metadata_readable`,
  `every_theme_holds_its_surface_grammar`.
- Interaction states — `interaction_states_never_collide` (hover, pressed and selected are
  three different colours in all twelve palettes).
- Motion — `motion_roles_fill_the_spec_bands`,
  `motion_collapses_to_nothing_under_reduce_motion`.
- Geometry — `default_settings_resolve_to_zeds_default_pixels`,
  `elevation_shadows_match_zed`.

A change to this file that is not also a change to those tests is a wish, not a rule.

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
  `Palette` when a new semantic need appears so all twelve palettes stay legible.
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
- **Don't** animate decoration; a one-shot transition uses a `Motion` role
  (`Instant` / `Interaction` / `Layout`) and only confirms a state change (hover,
  expand, selection, chart re-scale).
- **Don't** show a metric the underlying data cannot support — no invented latency,
  cost, or retry counts.
- **Don't** ship a chip that appears in every state "just in case". Give it a rule for
  when it is absent, and make that rule read the state the chip describes rather than a
  neighbouring control's open state.
- **Don't** give a control a permanent row for its own default. The access chip drops its
  label at Full access, the "Work in" chip is absent at the main checkout, and the MCP
  chip is absent while every server is connected — the default needs no announcement, and
  the row it frees is paid back on every frame.
