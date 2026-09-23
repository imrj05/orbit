---
name: Orbit Pi
description: Native GPUI workbench for the pi coding agent — quiet chrome, dense data, GPU-drawn.
colors:
  canvas: "#1A1A1A"
  sidebar: "#181818"
  raised: "#232323"
  hover: "#262626"
  active: "#2A2A2A"
  ink: "#E2E2E2"
  ink-secondary: "#A3A3A3"
  ink-tertiary: "#7D7D7D"
  hairline: "#2A2A2A"
  ember: "#E2795B"
  success: "#62C987"
  danger: "#E2726A"
  caution: "#E0B36A"
  canvas-light: "#F6F5F6"
  raised-light: "#ECECEC"
  ink-light: "#242424"
  ink-secondary-light: "#666666"
  hairline-light: "#E2E2E2"
  ember-light: "#B55035"
typography:
  title:
    fontFamily: ".ZedSans, IBM Plex Sans, system-ui, sans-serif"
    fontSize: "15px"
    fontWeight: 500
    lineHeight: "20px"
  body:
    fontFamily: ".ZedSans, IBM Plex Sans, system-ui, sans-serif"
    fontSize: "13px"
    lineHeight: "18px"
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
  sm: "6px"
  md: "8px"
  lg: "12px"
  xl: "16px"
  full: "9999px"
spacing:
  hair: "2px"
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "20px"
  xxl: "28px"
components:
  nav-row:
    backgroundColor: "{colors.canvas}"
    textColor: "{colors.ink-secondary}"
    rounded: "{rounded.sm}"
    height: "28px"
    padding: "0 10px"
  nav-row-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.ink}"
  primary-button:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    height: "34px"
    padding: "0 10px"
  chip:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.md}"
    height: "28px"
    padding: "0 8px"
  metric-board:
    backgroundColor: "{colors.raised}"
    textColor: "{colors.ink}"
    rounded: "{rounded.lg}"
    padding: "0"
  composer:
    backgroundColor: "{colors.sidebar}"
    textColor: "{colors.ink}"
    rounded: "{rounded.xl}"
    padding: "8px 12px"
---

# Design System: Orbit Pi

## Overview

**Creative North Star: "The Instrument Panel"**

Orbit is a workbench, not a website. Every surface is drawn by the GPU through GPUI,
so the chrome stays flat, honest, and quiet while the data — transcripts, diffs,
graphs, usage — carries the density. There is one accent (ember orange) and it is
rationed: selection, focus, the caret, the active series. Everything else is ink on
canvas at three weights of emphasis.

The system is monochrome-by-default with semantic color reserved for state, so a
screenshot in any of the forty-two palettes still reads as Orbit. Depth is delivered by
hairlines and tonal steps rather than shadows; shadows exist only for surfaces that
genuinely float above the page (composer, popovers, modals).

**Key Characteristics:**
- One accent, rationed: ember marks the active thing and nothing else.
- Hairlines over boxes: `border_1` at `theme.border` separates; cards are the exception.
- Three ink weights: `text` (primary), `text_2` (secondary), `text_3` (tertiary/labels).
- Density with air: 28–34px controls, 6–10px padding, 44px page headers. The
  Appearance panel exposes this as UI, Terminal/Editor font sizes (px), and
  Spacing Density (a global multiplier through `theme.space`).
- Forty-two palettes, one grammar: every color is read from `theme::get(cx)`, never hardcoded.

## Colors

A near-black canvas with a single ember accent; light mode is warm off-white, not white.

### Primary
- **Ember** (#E2795B dark / #B55035 light): selection, focus rings, links, the active
  data series, the caret, inline-code wash. Never decoration, never a large fill.
  The light ember is held dark enough to clear 4.5:1 on the light canvas, since links
  and inline code paint it as text.

### Neutral
- **Canvas** (#1A1A1A / #F6F5F6): the main content plane and every page background.
- **Sidebar** (#181818 / #F3F3F3): the chrome column; also the composer surface.
- **Raised** (#232323 / #ECECEC): chips, primary buttons, hover cards, grouped rows.
- **Hover** (#262626 / #E8E8E8): pointer feedback only.
- **Active** (#2A2A2A / #E2E2E2): the selected row; pairs with `active_fg`.
- **Hairline** (#2A2A2A / #E2E2E2): 1px separators, table rules, section boundaries.
- **Ink** (#E2E2E2 / #242424), **Ink Secondary** (#A3A3A3 / #666666),
  **Ink Tertiary** (#7D7D7D / #858585): the three text weights.

### Named Rules
**The One Accent Rule.** Ember appears on ≤10% of any screen. If two things are ember,
one of them is wrong.

**The Ink Weight Rule.** Emphasis comes from ink weight and size, never from hue.
A number that must shout gets `text` + a larger size, not a color.

**The Surface Ramp Rule.** On dark palettes `raised` clears both the canvas and the
chrome plane, and each state steps up from there — `canvas ≤ raised`, `sidebar ≤
raised`, `raised ≤ hover ≤ active` — so a card, a hovered row and a selected row never
invert into holes. Light palettes mirror the ramp downward. `menu_bg` sits at `raised`;
a popover floats, it never recesses. Every palette is asserted against this
monotonicity, not just Orbit.

**The Accent-Is-Not-Status Rule.** The accent may never equal `ok_green`, `add_green`,
`stop_red`, or `crit`. If the theme's signature hue is also its success or error color,
the accent moves to the palette's second hue; a green selection over a green checkmark
reads as one state, not two.

### Composer tokens
The composer paints two inline token roles. They are read as content — siblings of the
`syn_*` editor colors — not as chrome accents, so they sit outside the accent economy:
- **`/command`** takes the accent (`mention_command`): commands are actions, the same role
  terminal command words and syntax keywords already carry.
- **`@file`** takes the accent's complement (`mention_file`), so a reference never reads as
  a command. Chroma-less palettes (Ashwood, Mono) stay monochrome and split the two by
  ink instead of hue.

Both are derived in `Theme`, so all forty-two palettes stay legible without per-palette tuning.

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
Manrope, and JetBrains Mono, Fira Code, Geist Mono, Commit Mono, Source Code
Pro, Cascadia Code, Roboto Mono, Iosevka, DM Mono, IBM Plex Mono, Inconsolata,
Noto Sans Mono, Space Mono, Anonymous Pro, Martian Mono. Every family ships as
statically instanced, subset TTFs at the 400–700 weights it offers, under
`assets/fonts/bundled/` (glyph set
is Latin + punctuation + box-drawing + powerline), so a picked face never
depends on the OS. The bundled IBM Plex Sans and Lilex faces stay in the list
under their own names (their gpui aliases are `.ZedSans` / `.ZedMono`).

### Hierarchy
- **Title** (500, 15px): page titles in a 44px header row.
- **Body** (400, 13px): default UI text, list rows, table cells.
- **Body small** (400, 12–12.5px): secondary row text, chips, buttons.
- **Label** (500, 11px, +0.02em, uppercase): section labels in `text_3`, always paired
  with a count or meta on the right of the same row.
- **Number** (500, 20px): one per metric cell, no accent color.
- **Mono** (400, 12–13px): tokens, hashes, durations, exact figures.

### Named Rules
**The Size-Not-Weight Rule.** Sizes step 11 → 12 → 13 → 15 → 20; weights use only 400,
500, 600. No 700+, no letter-spacing tricks, no tracking below -0.01em.

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

Flat by default. Depth comes from tonal steps (canvas → raised) and 1px hairlines. Only
three surfaces float, and each has one authored shadow from `Theme`:

### Shadow Vocabulary
- **`composer_shadow`** (1 tight contact layer): the composer box and the usage metric
  board — objects that sit on the page rather than in it.
- **`popover_shadow`** (contact + wide ambient): anchored menus, dropdowns, tooltips.
- **`card_shadow`** (compact contact + ambient): hover cards over dense lists.

### Named Rules
**The Border-Or-Shadow Rule.** A surface declares one: either a 1px `border_strong`
hairline or a shadow. Never both, or it reads as a ghost card.

## Shapes

Corners are soft but restrained: 6px for navigation and session rows and
compact controls (≤24px icon buttons, toggles), 8px for chips, action buttons,
inline inputs, and data-list rows, 12px for grouped surfaces and boards, 16px for
the composer, `full` only for status dots, avatar monograms, and the send
button. Nothing is a pill except those circles.

Borders are 1px at `theme.border`; `border_strong` (ink at ~14% alpha) is reserved for
focus and hover emphasis. Charts and graphs draw with 1.5px strokes; grid lines are 1px
at low-opacity hairline. No drop-shadowed text, no gradients except the one backdrop
fade on the new-task page.

## Components

### Buttons
- **Shape:** 8px radius, 28px tall (chips) or 34px (the sidebar's primary action).
- **Primary:** `bg_raised` + 1px `border`, hover steps to `bg_hover` with a
  `border_strong` edge. One per screen region.
- **Ghost:** no fill until hover (`bg_hover`); used for every icon action in a header.
- **Disabled:** `overlay` fill with `text_3` content; never opacity-faded color.

### Chips
- **Style:** 28px tall, 8px radius, `bg_raised` + hairline, 12px ink, optional leading
  12–13px icon and trailing chevron.
- **State:** open/selected = `active` fill + `active_fg` text; hover = `bg_hover`.

### Metric cells
- **Style:** a single bordered board (`bg_raised` + hairline + 12px radius) whose cells
  are divided by 1px hairlines — never a grid of separate floating cards.
- **Content order:** 11px uppercase label → 20px number → 11.5px secondary line
  (comparison or average). Missing data reads "Unavailable", never a fabricated `0`.

### Cards / Containers
- **Corner Style:** 12px.
- **Background:** `bg_composer`/`bg_raised`; nested cards are forbidden.
- **Border:** 1px `border`.
- **Internal Padding:** 12–14px; lists use full-bleed rows with 8px inner padding and a
  hover fill instead.

### Inputs / Fields
- **Style:** composer-style box — `bg_composer`, 1px hairline, 16px radius on the main
  composer, 8px on inline fields.
- **Focus:** the border steps to `border_strong`; the accent is not used for focus fills.
- **Placeholder:** `text_3` — always a real prompt ("Search sessions…"), never a label.
- **Tokens:** a leading `/command` and any `@file` mention paint inline in their token
  colors while typing; the caret, selection, and wrapping are unaffected (paint only).
- **Caret & selection:** a 2px accent caret that holds solid while the field is being
  edited and blinks at the platform cadence (~1s) once idle. The selection wash is the
  accent at 25%; double-click selects the character run, triple-click the line, and
  shift-click extends — matching native text fields.

### Navigation
- Sidebar rows: 28px tall, 6px radius, 13px icon in a fixed 20px slot, label in
  `text_2`/`text_3`; hover fills `bg_hover`; the active destination is marked by
  `active` fill with `active_fg`, never by accent color alone.
- A running session row leads with an 11px spinner in the accent and its title carries a
  shimmer — an ember highlight band swept left-to-right across the ink on a 2s loop. This is
  the only per-row motion in the sidebar; a settled row is still.
- Page headers: 44px, back affordance first, title in 15px/500, actions right-aligned
  as 28px ghost controls.

### Tables
- Header row: 11px uppercase `text_3` labels, hairline underneath, sticky.
- Rows: 26px, hairline separators, hover fills `bg_hover`, selected fills `active`,
  numeric columns right-aligned in tabular figures, first column left-aligned.
- Clipped cell text always ends in `…`: the character budget is computed from the
  column's width rather than left to the renderer, so no cell is ever cut mid-glyph.
- Row-level actions are revealed on row hover, never as a permanent icon rail.
- Sort indicators are small chevrons in `text_3`; the sorted column's label steps to
  `text_2`.
- **Data tables and plots are drawn with GPUI's own elements.** The usage tables
  are built from `div` in `usage::table` and the timeline from `canvas` in
  `usage::chart`, each reading colors only from `theme::get(cx)` — the same
  surface as the rest of the page. There is no component layer and no second
  palette to synchronise.

**The Bridge Rule.** A table or chart reads every color from `theme::get(cx)`. If
it renders in a color this file does not list, the element is wrong — fix the
element, not the palette.

## Do's and Don'ts

### Do:
- **Do** read every color, size, and radius from `theme::get(cx)`; add a role to
  `Palette` when a new semantic need appears so all forty-two palettes stay legible.
- **Do** use hairlines and whitespace to separate sections; reserve rounded bordered
  surfaces for objects that are conceptually singular (composer, metric board, popover).
- **Do** keep controls at 28px (compact) / 34px (primary) so a 900px-tall window shows
  15+ table rows.
- **Do** mark unavailable data in words ("Cost unavailable"): zero and unknown are
  different facts.
- **Do** pair color with a non-color signal (↑/↓, icon, label) for any state.

### Don't:
- **Don't** introduce a second accent hue; categorical series use one accent ramp
  (100% → 62% → 40% → 22% wash) plus neutral inks.
- **Don't** wrap every row in a card, nest a card inside a card, or use a card where a
  hairline-separated section reads better.
- **Don't** use display fonts, gradients, glass, glow, or emoji in the chrome.
- **Don't** animate decoration; motion is 120–200ms and only confirms a state change
  (hover, expand, selection, chart re-scale).
- **Don't** show a metric the underlying data cannot support — no invented latency,
  cost, or retry counts.
