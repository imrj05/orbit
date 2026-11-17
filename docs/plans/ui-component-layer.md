# Plan — UI component layer

**Status:** **Phase 1a landed; Phases 1b and 2 partially implemented, verification pending.** `ui/` exists and the interaction
seam moved into it (`ui/style/control.rs`, re-exported so no call site changed),
the shared tooltip moved out of the `usage/` page, and
`scripts/check_ui_layers.py` enforces the dependency direction. Phase 2's first
navigation controls and the switch move are implemented but unverified; later work remains proposed.
The document is the inventory and the plan for giving
every UI element in `crates/orbit-pi` exactly one named owner. It builds on
`docs/design-system-audit.md` (which finished the *vocabulary*: colour roles,
geometry tokens, interaction states, motion roles); it does not re-open any of
that.

**What this is not.** It is not a redesign. `DESIGN.md` is the visual spec and
stays the spec; `orbit-design` is the normative component contract. This plan is
about **ownership and composition** — the layer that is still missing between
`theme::tokens` and the 24 popup renderers.

---

## 1. What already ships — do not rebuild it

Verified against the code, not the docs:

| Layer | Where | Proof |
|---|---|---|
| Semantic colour roles + 13 palettes, contrast-tested | `theme.rs` | `cargo test -p orbit-pi --bin orbit-pi theme` |
| Geometry tokens: `Radius`, `DynamicSpacing`, `TextSize`, `IconSize`, `ButtonSize`, `ElevationIndex`, `Motion` | `theme/tokens.rs` | 16 token tests |
| Component metric modules: `button`, `list_item`, `panel`, `list`, `picker`, `popover`, `context_menu`, `toast`, `modal`, `tooltip`, `input`, `scrollbar` | `theme/tokens.rs` | resolved through `Theme`, never px literals |
| Interaction-state vocabulary (`InteractionState` + `ControlExt` 24 treatments + `StatefulControlExt` press/disabled) | `theme/tokens.rs`, `app/helpers.rs` | **245 call sites**; raw `.hover(\|` outside `app/helpers.rs` = **0** |
| State guard | `scripts/check_states.py` | fails when a call site names a state colour |
| Motion roles + `looping` + `chart` cadences, reduce-motion inside `Motion::duration` | `theme/tokens.rs`, `motion/` | `scripts/check_motion.py` |
| Focus inherited by buttons and picker rows | `app/helpers.rs` | `focus_state` in `button_frame`, `icon_button_frame`, `picker_entry` |
| Keyboard guards | `scripts/check_keyboard.py`, `commands.rs` tests | 59 commands, 26 bindings, 28 contexts |
| Virtualization contracts (transcript, sidebar, trees, tables) | `message_scroller.rs`, `explorer/*`, `usage/table.rs` | `list()` + `h_full` + `min_h_0` |
| i18n call-site guard | `scripts/check_i18n.py` | 10 locales |

**Consequence:** a "button component" that only restyles buttons would duplicate
work already done. The gap is narrower and more specific than "no components".

---

## 2. What is actually missing

The missing thing is **a named owner for composition and behaviour**. Today the
element families are spread across surface files, and each surface re-derives the
same shell:

| Element family | Function definitions (non-test) | Files | Today's owner |
|---|---:|---:|---|
| Row | 95 | 34 | each surface |
| Menu / menu behaviour | 81 | 15 | each menu |
| Button | 44 | 18 | each surface (geometry from `button_frame` only) |
| Section / board | 43 | 15 | each surface |
| Card | 31 | 15 | each surface |
| Chip | 27 | 12 | each surface |
| Popup | 24 | 8 | each surface |
| Tab | 22 | 5 | terminal, settings, usage |
| Table | 21 | 4 | usage, settings, git |
| Layer (modal/popover overlay) | 12 | 10 | each surface |
| Badge | 11 | 8 | each surface |
| Toast | 8 | 3 | `app/toast_ui.rs` + two callers |

Four concrete failure modes, each with evidence:

**2.1 `button_frame` is geometry only.** Its own doc says *"Callers keep the
colors, border, hover, press, and click handling."* Result: 44 `*_button`
functions (`primary_button`, `ghost_button`, `action_button`, `text_button`,
`provider_button`, `subagent_primary_button`, `subagent_secondary_button`,
`subagent_danger_button`, `dialog_button`, `send_button`, `stop_button`, …) each
re-assemble a variant from `RaisedExt` + `ControlExt` + `theme.action`. The
*states* are consistent — the states guard proves that — but the *variant* is
not: "primary" means three different fill choices across surfaces.

**2.2 24 popup renderers + 12 layer renderers, 43 `on_mouse_down_out` sites in
21 files.** `sidebar_show_popup`,
`workspace_menu_popup`, `session_menu_popup`, `add_menu_popup`, `chip_popup`,
`access_popup`, `workflow_popup`, `settings_select_popup`, `render_quota_popup`,
`open_in_menu_popup`, `template_picker_popup`, `pr_base_popup`, `label_popup`,
`ref_picker_popup`, `branch_prompt_popup`, `stage_prompt_popup`,
`confirm_popup`, … and `*_layer` for modals. Each re-derives: surface
(`picker_surface` vs `context_menu_surface` vs `elevation_2`), dismissal
(`on_mouse_down_out` with its own click-through guard), width, scroll behaviour,
and focus restore. `picker_surface` and `context_menu_surface` are two names for
nearly the same thing, which is how the drift started.

**2.3 Menu highlight is reimplemented per menu.** `add_menu_highlight`,
`access_menu_highlight`, `menu_highlight`, `menu_highlighted`,
`set_menu_highlight`, `highlight_menu` — plus `explorer/panel.rs::PanelMenu`,
`transcript_view.rs::TextMenu`, `app.rs::SessionMenu`/`WorkspaceMenu`/
`WorktreeMenu`, `usage/page.rs::MenuKind`, each with its own
next/prev/wrap/enter/escape logic. There is no shared `MenuState`.

**2.4 95 row renderers.** Sidebar rows, settings rows, permission rows, model
rows, provider rows, worktree rows, MCP rows, git rows, usage rows, transcript
rows: the same anatomy (start slot → content column → metadata → trailing
actions) rebuilt 95 times, with density and trailing-action reveal re-decided
each time.

---

## 3. Target architecture

### 3.1 A new `crates/orbit-pi/src/ui/` module

```
theme.rs                  colour roles + palettes (unchanged)
theme/tokens.rs           geometry + component metrics + InteractionState (unchanged)
theme/appearance.rs       mode + palette resolution (unchanged)
        │
        ▼
ui/style/                 extension traits — zero-cost, return Self
  control.rs              re-export of ControlExt / StatefulControlExt
  surface.rs              SurfaceExt: panel, card, well, floating
  text.rs                 TextRole: title / body / meta / numeric / mono
        │
        ▼
ui/                       named elements — IntoElement, own composition
  button.rs               Button, IconButton, ButtonGroup, LinkButton
  chip.rs                 Chip, ChipButton (trigger), Badge, Pill
  menu.rs                 Menu, MenuState, MenuItem, MenuSeparator, MenuHeader
  dropdown.rs             Dropdown (trigger + Menu + keyboard + focus restore)
  popover.rs              Popover host, PopoverAnchor
  dialog.rs               Modal (scrim + card + header/body/footer + focus trap)
  field.rs                TextField, SearchField, FieldLabel, FormRow
  switch.rs               Switch (moved from app/helpers.rs)
  tab.rs                  TabStrip, Tab
  card.rs                 Card, Section, MetricBoard, MetricCell
  row.rs                  ListRow, NavRow, TableRow
  table.rs                Table + header/columns/sort
  tree.rs                 TreeRow + disclosure
  list.rs                 VirtualList (pins the h_full + min_h_0 contract)
  toast.rs                ToastCard
  tooltip.rs              Tooltip host
  banner.rs               Banner (info/notice/error)
  feedback.rs             EmptyState, ErrorState, Skeleton, Spinner, Progress
  kbd.rs                  Kbd, Shortcut
  icon.rs                 Icon, FileGlyph, ProviderMark, Avatar
        │
        ▼
app/*, transcript_view.rs, git_panel/*, usage/*, explorer/*, sidepane.rs,
terminal.rs, composer.rs, command_palette.rs, dialog.rs   ← surfaces
```

**Import rule (enforced):** `ui/*` may import `theme`, `motion`, `i18n` and
`gpui` — and **must not import `app::*`, `OrbitApp`, or any surface module**.
A surface may import `ui`. Today the direction is inverted in places: shared
tables like `app/helpers.rs::session_display_title` and
`queue_row_slot` are UI-adjacent domain helpers living in the UI grab bag.

### 3.2 Two tiers, and when to use each

This distinction is the whole reason the layer stays cheap:

| Tier | Shape | Use when | Cost |
|---|---|---|---|
| **Style trait** (`ui/style`) | `fn hover_state(self, theme) -> Self`, generic over `E: Styled + InteractiveElement` | The element is a plain tree and every caller must still chain `.id()`, `.on_click()`, `.child()` | zero — no allocation, no wrapper |
| **Component** (`ui/*`) | `#[derive(IntoElement)] struct Button { … }` with `fn render` | The element owns anatomy or behaviour: a variant, a highlight index, dismissal, focus restore | one struct per render, no heap |
| **Entity** | `Entity<ComposerInput>`, `Entity<FileViewer>`, `Entity<Terminal>` | Long-lived state, IME, PTY, async I/O | unchanged — **do not convert these** |

A component never replaces a style trait for no reason. `button_frame` +
`ControlExt` stay as the geometry/state seam; `Button` is the variant above them.

### 3.3 Component rules

1. **One element, one name.** `picker_surface` and `context_menu_surface` become
   one `Popover`; `Button` is the only thing that decides what "primary" means.
2. **`&Theme` at paint.** No component caches a colour at construction — the
   Instant-Switch Rule (`app/theme_switch_tests.rs` guards it).
3. **Text is localized by the caller** (`tr!` at the call site), except shared
   states that own their copy (`EmptyState`, `ErrorState`) — matching the
   existing `empty_state` / `error_state` signature.
4. **No literal colours, no px literals** where a token exists — `check_states.py`
   and `theme/tokens.rs` already enforce the first.
5. **`VirtualList` keeps the load-bearing contract:** `.h_full()` inside a
   `flex_1 min_h_0` parent, never wrapped in `overflow_y_scroll()`. The component
   exists partly to stop that being re-derived.
6. **No new dependency.** Everything here is `gpui` + what is already in
   `Cargo.toml`.
7. **Migrate without restyling.** A phase that moves call sites must not change
   pixels; a phase that changes pixels says so and gets a visual check.

### 3.4 Material contract — glass is not a component variant

The wallpaper/glass system already ships and is **theme-resolved**, so a
component must never decide material per surface. It asks the theme:

| Need | Ask | Today's leak |
|---|---|---|
| Card / board / menu fill | `theme.card_fill(base)` | call sites passing `.bg(theme.surface_elevated)` directly |
| Panel fill (sidebar, main column) | `theme.panel_fill(base)` | `app/view.rs` inline |
| Hover / pressed wash | `theme.hover_wash()` / `pressed_wash()` | already used by `ControlExt` ✅ |
| The frost itself | `theme.glass_sigma()` → `frosted_card(...)` | 10 `frosted_card` sites, plus 2 special shapes below |
| Composer | `theme.for_composer(empty)` → `composer_fill()` | already centralized ✅ |
| Page behind transcript | `theme.page_appearance()` | ✅ |

**Rules for every `ui/` component:**

1. A component never names `card_alpha`, `panel_alpha`, or an `.opacity()` on a
   container. It calls `card_fill` / `panel_fill`. Faking glass with container
   opacity is the legacy overlay path and is not allowed.
2. Anything card- or pane-shaped paints through **`frosted_card`** — the one
   wrapper that puts the blur request in the border channel and draws the
   hairline outside the blurred quad. No component may reach for
   `gpui::backdrop_blur_marker` itself.
   Two shapes are legitimate exceptions and must be *named*, not re-derived:
   - the **sidebar rail** (`app/view.rs:705`) frosts edge-less, because it is a
     full-height structural pane with no card edge and no hairline to draw;
   - the **transcript edge fade** (`message_scroller.rs:471`) uses
     `backdrop_blur_fade_marker`, which is a different primitive entirely.
   `SurfaceExt` therefore offers `frosted(base, radius)` **and**
   `frosted_pane(base)`; a third spelling at a new call site is the signal
     that the API is missing a shape — add it in `ui/style/surface.rs`.
3. A control sitting **on** glass takes `hover_wash` / `pressed_wash` (ink at
   alpha), never `surface_hover` (an opaque slab cut into the glass).
   `ControlExt` already does this correctly; the rule is what keeps a new
   component from regressing it.
4. **Degrade, don't differ.** `glass_sigma() == None` (Glass off, or a renderer
   without a backdrop pass) means opaque fill and *identical geometry*. A
   component may not change size, spacing, border or radius because glass is off.
5. **Do not claim blur where it does not happen.** In this GPUI only macOS
   provides `gpui::backdrop_blur_supported()`; elsewhere "glass" is a
   translucent fill. The component paints the same thing; only the material
   differs.
6. Contrast belongs to the theme: `wallpaper::surfaces` raises the requested
   alpha to the floor that keeps ink readable (cards clamped 0.35–1.0, page
   ≥0.65). A component may not override that to get "more glass".
7. Two documented exceptions stay: the chat composer's contact shadow and the
   transcript edge fade (`message_scroller.rs`). Neither is a card treatment.

This makes the material layer a **`ui/style` concern**: `SurfaceExt` gains
`card_surface`, `panel_surface`, `well_surface`, `frosted(base, radius)` so the
10 + 2 existing sites collapse into one path and Phase 2–4 components inherit
glass without knowing it exists.

### 3.5 Decided — wallpaper stays static

**Decision (A): static wallpaper, no animation, no mod surface.** The prepared
texture is the whole contract; the only motion remains the one `Motion::Layout`
fade-in on install, which Reduce Motion already removes. Options B (frame
sequence) and C (procedural animation) are rejected, not deferred: neither has a
user need that justifies the memory, battery or GPU cost, and a photographic
animation would put a texture swap on every frame of a workbench that is graded
on scroll latency.

**Shipped today: static.** `wallpaper::prepare` decodes the image **once**, off
the UI thread, applies the artwork effect, downscales (main 2560px, effect
1800px, blur 1280px), blurs and samples brightness, and the result is cached as
a GPUI texture. `wallpaper::layer` then paints that texture. The wallpaper is a
*preference record* (`WallpaperSettings`), not a live system.

This is a documented constraint, not an omission: the library table in
`AGENTS.md` says decode once into the cache and **never re-decode per frame**.

Consequence for this plan: the material contract in §3.4 is the *only* wallpaper
work that lands. Nothing in Phase 1–7 may add a per-frame wallpaper pass, a frame
cache, or a shader path for the wallpaper.

**"Mod functionality" — does not exist, and is not planned under decision A.**

- The 13 palettes are compiled in (`ThemeId`) — there is no user palette file.
- The 5 artwork effects are compiled in (`wallpaper_fx::Effect`) — not pluggable.
- `plugins.rs` is the **pi plugin** surface (agent-side packages), unrelated to
  the UI layer.
- The bundled pi extensions load JS into the user's pi process; they cannot
  paint Orbit.

If a data-only wallpaper/palette folder is ever wanted, it is a feature with a
manifest, a validator and the same `MAX_EDGE`/`BLUR_MAX_EDGE`/`EFFECT_MAX_EDGE`
caps — and it must not be described as a mod system before it exists.

---

## 4. Element inventory — every element in the app

Grouped by family. "Sites" = function definitions in non-test code (measured
snapshot, reproducible — see Appendix) or call sites (measured with
`grep -c`). "Target" is the `ui/` element that should own it.

### 4.1 Actions

| Element | Where it appears | Today | Target |
|---|---|---|---|
| Button (primary / secondary / ghost / destructive / disabled / loading) | every surface; 229 `button_frame` sites, 44 `*_button` defs | `button_frame` = geometry only; variant re-decided per surface | `Button` |
| Icon button | headers, rows, transcript footers; 91 `icon_button_frame` sites | `icon_button_frame` + `.tip()` | `IconButton` |
| Segmented control | composer delivery mode, reduce motion, permissions scope | `segmented_segment` + `SegmentPosition` (11 sites) | `ButtonGroup` |
| Link / text button | settings, MCP, onboarding, git panel (`text_button`, `mcp_text_button`, `row_button`, `name_action_button`) | one-off `div` per surface | `LinkButton` |
| Menu trigger | top bar chips, git toolbar, usage filters (`chip_with_menu`, `workspace_menu_button`, `add_menu_button`) | `chip` + hand-rolled `open_menu`/`toggle_menu` | `Dropdown` |
| Toggle switch | settings, permissions, MCP | `ui/switch.rs::switch()` + `Switch` `RenderOnce`, re-exported by `app/helpers.rs` | `ui/switch.rs` (moved, unchanged) |
| Window controls | title bar (minimize / maximize / close) | `window_controls`, `platform.rs` | `ui/chrome.rs` |
| Disclosure | explorer tree, fold rows (13 sites) | `disclosure_chevron`, `dropdown_chevron` | `TreeRow` / `ui/icon.rs` |
| *(absent)* Checkbox / radio | — | selection is done with rows and segments | do not add without a real need |

### 4.2 Containers

| Element | Where it appears | Today | Target |
|---|---|---|---|
| Card | transcript (`render_activity_card`, `render_ask_card`, `render_subagent_card`, `render_summary_card`), settings (`plugin_card`, `provider_error_card`, `remote_host_card`), usage (`usage_breakdown_card`, `summary_card`), MCP (4 cards), worktrees (`worktree_howto_card`), session (`quota_provider_card`) — 31 defs | each re-derives `panel::RADIUS` + `elevation_1` + padding | `Card` |
| Section / board | settings pages, usage page, permissions — 43 defs | `section` header + body per page | `Section` |
| Pane / panel | sidebar, side pane, explorer, git, terminal (`panel::RADIUS` 111 sites) | `elevation_1` inline | `Surface` (style) + `Pane` |
| Metric cell / stat tile | usage (`stat_tile`), status bar, session details | two variants (`stat_tile`, `metric cells` in DESIGN.md) | `MetricBoard` |
| Modal card | 8 modals | `modal_card` (12) + `modal_scrim` (8) ✅ | `Modal` — add header/body/footer + trap |
| Popover surface | 24 popups | `picker_surface` vs `context_menu_surface` vs `elevation_2` | `Popover` |
| Toast card | notifications, retention | `toast_card` + 8 fns | `ToastCard` |
| Banner | run failure (`error_banner`), explorer notices, process exit, star project, subagent inactive — 5 defs | 5 different shapes | `Banner` |
| Empty state | 18 sites | `empty_state` ✅ | `EmptyState` (rename, add action slot) |
| Error state | 3 call sites + hand-rolled cards | `error_state` ✅ | `ErrorState` |
| Lightbox | image attachments | `lightbox_layer` | `ui/lightbox.rs` |
| Backdrop (dither field) | new-task page | `backdrop.rs` | unchanged — it is a surface, not a component |

### 4.3 Overlays

| Element | Where it appears | Today | Target |
|---|---|---|---|
| Context menu | sidebar (session / workspace / row), explorer, transcript selection, sidepane | `context_menu_surface` (2 sites) + 81 menu fns | `Menu` + `MenuState` |
| Dropdown menu | top bar, git toolbar, usage filters, composer add/access/workflow | 24 popup renderers | `Dropdown` |
| Dialog / modal | `app/dialogs.rs` (extension UI), worktrees (5), updater, MCP editor, provider editor/key/usage, remote host editor, remote key/password, server clone | `modal_card` + 12 `*_layer` renderers | `Modal` |
| Popover (anchored) | model selector, branch picker, workspace picker, quota, context meter | `anchored_popover` (17 sites) ✅ | `Popover` host |
| Tooltip | `TipExt::tip`, usage tooltip | 3 fns, `context_tooltip` | `Tooltip` (one host, not per-element) |
| Toast | notifications, retention, pi update | `app/toast_ui.rs` | `ToastCard` + existing model |
| Command palette | ⌘P | `command_palette.rs` (own scrim, own sections) | `Palette` built on `Modal` + `ListRow` |

### 4.4 Inputs

| Element | Where it appears | Today | Target |
|---|---|---|---|
| Text field | settings (providers, MCP, remote hosts, subagents, permissions), git panel, worktrees — 26 `input_field_frame` sites | `input_field_frame` + `input_label` | `TextField` |
| Search field | sidebar session search, settings filters, side pane, git panel, usage — `search_field_composing` | `ComposerInput` reused ad hoc | `SearchField` |
| Composer | chat, new-task | `composer.rs` entity (3,759 lines) | keep as entity; chrome → `ComposerFrame` |
| Dropdown / select | `settings_select_popup`, `chip_popup`, provider pickers | hand-rolled popup + trigger | `Dropdown` (single-select) |
| Field label + form row | subagents detail form, bug report, remote hosts | `subagent_field`, `bug_field` | `FieldLabel`, `FormRow` |
| Attachment chip / file chip | composer tray, subagent tools, transcript | `attachment_chip`, `user_file_chip`, `subagent_tool_chip` | `Chip` variant |
| Editor | Files surface | `explorer/viewer.rs` entity | keep as entity |
| *(absent)* Stepper / slider / checkbox | — | — | not needed yet |

### 4.5 Navigation

| Element | Where it appears | Today | Target |
|---|---|---|---|
| Sidebar row | sessions, workspaces, groups | `session_card` + `session_card_summary` | `NavRow` |
| Navigation rail | transcript user-turn rail | `render_navigation_rail`, `render_rail_hint` (7 fns) | `ui/rail.rs` |
| Tab / tab strip | terminal tabs, settings tabs, usage breakdown tabs — 22 defs | `tab_chip`, `tab_strip` per surface | `TabStrip` |
| Page header | settings pages, worktrees, usage, git | `header` (72 defs, mostly table headers) | `PageHeader` |
| Section header | settings, palette, menus | `menu_header` (14 sites), `ListSubHeader` tokens | `SectionHeader` |
| Status lane / glyph | transcript status row | `app/session_status.rs` | keep — it is a state machine, not an element |

### 4.6 Lists, tables, data

| Element | Where it appears | Today | Target |
|---|---|---|---|
| List row | 95 defs across 34 files | re-decided per surface | `ListRow` |
| Virtualized list | transcript, sidebar, explorer, usage tables | `list()` + `ListState` per surface | `VirtualList` |
| Table | usage (filters/breakdown/failure), settings, git history | 21 defs in `usage/` + `settings.rs` | `Table` |
| Tree row | explorer project panel | `explorer/tree.rs` + `panel.rs` | `TreeRow` |
| Diff row / hunk header | Review, transcript diffs | `diff_view.rs`, `sidepane.rs` | `DiffRow` (paint-only, keep streaming contract) |
| Chart / sparkline / meter | usage charts, context meter | `usage/chart.rs`, `context_meter.rs` | unchanged — painted, not componentised |
| Progress: spinner, refresh, bar, skeleton | everywhere | `spinner`, `running_spinner`, `refresh_glyph`, `shimmer` | `Spinner`, `Progress`, `Skeleton` |

### 4.7 Feedback, identity, chrome

| Element | Where it appears | Today | Target |
|---|---|---|---|
| Badge / pill / status dot | git status, provider, queue, activity, staged, ref | 11 badge + 6 pill defs | `Badge`, `Pill` |
| Kbd / shortcut hint | menus, palette, shortcut reference | `kbd_key`, `kbd_shortcut` | `Kbd` |
| Icon | everywhere | `icon`, `icon_dyn` ✅ | `Icon` |
| File glyph / icon tile | explorer, tabs, transcript | `file_glyph`, `file_icon_tile`, `file_type_tile`, `file_badge` | `FileGlyph` |
| Provider mark | settings, model selector, chips | `provider_mark_color` + assets | `ProviderMark` |
| Avatar | transcript message disc | `avatar_client` (1) | `Avatar` |
| Title bar / top bar | shell | `header_lift`, `header_fill`, `window_drag_region` | `ui/chrome.rs` |
| Resize handle / splitter | 4 panels | `resize` (6) + `handle` (30) | `ResizeHandle` |
| Scrollbar | all scrollers | `scrollbar` token module + gpui | unchanged |
| Status bar chips | bottom bar | `status_chip`, `queue_badge` | `Chip` + `StatusBar` |

### 4.8 Materials and wallpaper

These are **surfaces, not components** — but every component above is drawn on
top of them, which is why they have a contract (§3.4).

| Element | Where it appears | Today | Owner |
|---|---|---|---|
| Wallpaper image layer | behind the window / behind the main column | `wallpaper::layer` (static, one `Motion::Layout` fade-in) | `wallpaper.rs` — unchanged |
| Wallpaper artwork effects | Settings → Appearance | `wallpaper_fx::{Dither, Ascii, Halftone, Scanlines}`, baked once at `prepare` | `wallpaper_fx.rs` — unchanged |
| Surface treatment (alpha, page, glass flag) | every panel | `wallpaper::surfaces` + contrast pass | `wallpaper.rs` — unchanged |
| Frost (backdrop blur) | cards, panes, menus, dialogs, composer | `theme.glass_sigma()` + `StyledExt::frosted_card` | `ui/style/surface.rs` (Phase 1) |
| New-task backdrop (dither field + monogram + light) | new-task page | `backdrop.rs` (painted, own loop) | unchanged |
| Palette / accent derivation | Settings → Appearance | `theme.rs` 13 palettes + `appearance::with_accent` | unchanged |

---

## 5. The four the request named — contracts

`DESIGN.md` §Components is the visual spec; this is the ownership contract.

### 5.1 `Button`

```
Button::new("save", tr!("common.save"))
    .variant(ButtonVariant::Primary)     // Primary | Secondary | Ghost | Danger | Link
    .size(ButtonSize::Default)           // token decides height, padding, label, icon
    .icon("icons/check.svg")
    .shortcut("⌘S")
    .loading(bool)
    .disabled(bool)
    .on_click(handler)
```

- **Anatomy:** `button_frame` (unchanged geometry) → optional icon at
  `size.icon_size()` → label at `button::label_size()` → optional shortcut →
  optional spinner replacing the label while `loading`.
- **States:** inherited from `ControlExt`/`StatefulControlExt` — the component
  never names a hover/press colour. `Primary` resolves its fill once, here, so
  "primary" stops having three meanings.
- **Keyboard:** Enter/Space via the caller's `on_click` key binding; `focus_state`
  is inherited from the frame. No new focus mechanism.
- **Guards:** `check_states.py` continues to hold the colour half.

### 5.2 `Card`

```
Card::new("activity")
    .variant(CardVariant::Outlined)   // Outlined | Raised | Flat
    .padding(CardPadding::Comfortable)
```

- **Anatomy:** `panel::RADIUS`, one hairline (`border` / `border_strong` for
  floating), one fill level, optional header row separated by a hairline.
- **Rule it enforces:** `DESIGN.md` forbids nested cards; a `Card` inside a
  `Card` becomes a `Section`. The composer's contact shadow stays an explicit
  exception and is *not* a card variant.
- **Does not own:** content. `Card` never renders text, only frames it.

### 5.3 `Dropdown`

```
Dropdown::new("workspace-menu", chip_element)
    .menu(my_menu_state)          // shared MenuState
    .placement(Corner::BottomLeft)
    .on_dismiss(handler)
```

- **Owns:** trigger anchoring (`anchored_popover`), open/close, click-through
  suppression (the existing 200 ms `menu_dismissed_at` guard becomes internal),
  viewport snapping, dismissal on outside click/Escape, and **focus restore to
  the trigger**.
- **Does not own:** the menu's contents — those are `Menu`/`MenuItem`.
- **Replaces:** 24 popup renderers and their per-surface dismissal logic.

### 5.4 `Menu` + `MenuState`

- **`MenuState`** (the missing shared type): `items`, `highlighted`, plus
  `next`, `prev`, `home`, `end`, `selected()` — one implementation of wrap-around
  navigation, replacing `add_menu_highlight` / `access_menu_highlight` /
  `menu_highlight` / `set_menu_highlight` / `PanelMenu` / `TextMenu` /
  `SessionMenu` / `WorkspaceMenu` / `WorktreeMenu`.
- **`Menu`**: surface (`context_menu_surface` metrics), `MIN_WIDTH`,
  `MAX_HEIGHT_FRACTION`, scroll, keyboard context declaration.
- **`MenuItem`**: icon slot, label, secondary line, shortcut, danger variant,
  checked state, disabled-with-reason, submenu caret.
- **`MenuHeader` / `MenuSeparator`**: already exist as helpers; become components.
- **Contract:** pointer-invoked menus may anchor to the pointer and **must** have
  a keyboard route (`DESIGN.md` §Context Menus, `check_keyboard.py`).

---

## 6. Phases

Each phase is a coherent landing. Order is dependency-driven, not priority:
style → controls → overlays → containers → states → surfaces → guards.

### Phase 1 — `ui/style` + the module boundary

Split into two landings so the trait move stays a pure move:

**1a — done.** `TipExt`, `ControlExt`, `StatefulControlExt`, `PressedRamp` and
`AccentWash` moved from `app/helpers.rs` to `ui/style/control.rs`;
`app/helpers.rs` re-exports them, so `crate::app::ControlExt` and every existing
path keep resolving and no call site changed. `ui/mod.rs` + `ui/style/mod.rs`
added, `mod ui;` registered. The guard found one real violation on its first
run: the shared tooltip view lived at `usage/tooltip.rs` — a shared element under
a page — so it moved to `ui/tooltip.rs` and its seven importers followed.
`scripts/check_states.py`'s allowed-file path moved to `ui/style/control.rs` (the
contract's new home). `scripts/check_ui_layers.py` was added, with `--self-test`
proving both rules can fail and that the allowed file is skipped.

Verified: `cargo check -p orbit-pi` (no new warnings),
`cargo test -p orbit-pi --bin orbit-pi theme` (84 passed), `helpers` (16),
`switch` (16), `check_states.py`, `check_motion.py`, `check_keyboard.py`,
`check_ui_layers.py` + `--self-test`.

**1b (partial, intended no pixel change).** `ui/style/surface.rs` owns the
existing `frosted_card` treatment and the sidebar's edge-less `frosted_pane`;
the existing card sites import the moved trait without changing their fill or
geometry. Elevation's separate frost path remains in `theme/tokens.rs`, and
fill helpers stay on `Theme`: additional surface APIs are deferred until real
call sites justify them. `TextRole` is deferred to Phase 4. Cargo checks and
manual glass-on/off comparison are still pending; do not treat acceptance as met.

**Acceptance:** byte-identical rendering; `cargo check -p orbit-pi` clean and
warning-free; the new script passes and fails on a seeded violation (tested in
the script's own `--self-test`); glass on/off produces identical geometry on a
spot-checked settings card, a menu and the composer.

### Phase 2 — Core controls
**Partial:** `ui/button.rs` owns the existing labeled-button and icon-button
frames. `app/helpers.rs` re-exports both, so the existing labeled-button sites
now share the moved frame without changing their call sites, styling or behavior.
It also owns a small navigation icon control recipe, used by the sidebar Git,
Usage and Settings buttons. The callers retain localized tooltips, icons and mouse handlers; the
open, hover, press and focus treatments are unchanged. A separate quiet icon
action recipe now also owns the existing frame, pointer, hover and wash-press
treatment for Review's tree, refresh and close buttons; their tooltips and
handlers remain at the call sites. `ui/button.rs` also owns `SegmentPosition`
and `segmented_segment`, re-exported through `app/helpers.rs` for existing
callers. A shared medium settings-segment recipe preserves the selected and
resting treatment for delivery mode, design system, appearance mode and
subagent mode; their labels, keyboard and pointer handlers remain local. Other
segmented controls retain their distinct treatments. `ui/switch.rs` now owns the existing switch
and its knob-travel test; `app/helpers.rs` re-exports `switch` unchanged, and
the existing keyboard test remains at the compatibility path. `ui/icon.rs` now
owns the shared tinted SVG icon primitives and hover group, re-exported by
`app/helpers.rs`; domain icons stay where they are. The existing layout
regression test for SVG flex shrinking remains in `app/backdrop_layout_tests.rs`
(it tests the GPUI behavior directly). This is not a general
`IconButton` API or completion of the phase. `cargo check -p orbit-pi`
passes after the switch move; the segmented-control migration has not had
Cargo checks or tests run; the icon move likewise has not had Cargo checks or
tests run. Switch tests and manual visual/glass verification remain pending.

`Button`, `IconButton`, `ButtonGroup`, `LinkButton`, `Chip`, `ChipButton`,
`Badge`, `Pill`, `Kbd`, `Switch` (moved), `Icon`.
Migrate the 44 `*_button` defs and the 27 chip / 11 badge / 6 pill defs.
**Acceptance:** zero `*_button` definitions outside `ui/`; `primary` resolves in
one place; every migrated surface renders identically (spot check: settings,
composer, sidebar, git panel, transcript footer).

### Phase 3 — `Menu`, `Dropdown`, `Popover`, `Modal`
Build `MenuState` first (pure, unit-tested like `command_palette.rs`'s scoring),
then `Menu`/`MenuItem`, then `Dropdown`, then `Modal` (header/body/footer + focus
trap) and `Popover` (merging `picker_surface` + `context_menu_surface`).
Migrate the 24 popups and 12 layers.
**Acceptance:** no `on_mouse_down_out` outside `ui/popover.rs`/`ui/dropdown.rs`;
no menu next/prev logic outside `ui/menu.rs`; focus restore tested per migrated
dialog; `app/popup_layout_tests.rs` geometry assertions still pass.

### Phase 4 — Containers and rows
`Card`, `Section`, `MetricBoard`, `ListRow`, `NavRow`, `TableRow`, `Table`,
`TabStrip`, `TreeRow`, `VirtualList`, `PageHeader`, `SectionHeader`.
Migrate the 31 card / 43 section / 95 row defs, starting with the surfaces that
repeat most: settings, usage, git panel, sidebar, transcript.
**Acceptance:** row density and trailing-action reveal come from one place;
`VirtualList` is used by every virtualized surface and no surface wraps it in
`overflow_y_scroll`; scroll behaviour unchanged in a manual pass.

### Phase 5 — Fields, feedback, banner
`TextField`, `SearchField`, `FieldLabel`, `FormRow`; `EmptyState` (add the
resolving-action slot the audit asked for), `ErrorState`, `Banner`, `Skeleton`,
`Progress`, `Spinner`, `ToastCard`.
**Acceptance:** one error shape (what happened → detail → next step → actions);
every empty state can offer an action; the 5 banner shapes become 1 variant set.

### Phase 6 — Surface sweeps
Mechanical adoption in `app/settings.rs`, `app/view.rs`, `app/sidebar.rs`,
`transcript_view.rs`, `git_panel/`, `usage/`, `explorer/`, `sidepane.rs`,
`terminal.rs`, `command_palette.rs`. Then delete `app/helpers.rs` entirely:
UI → `ui/`, formatting/domain helpers → `app/format.rs`.
**Acceptance:** no element family has two owners; `rg "app::helpers"` is empty.

### Phase 7 — Guards, docs, verification
Extend `check_states.py` to also reject a raw `.bg(theme.surface…)` in a
`ui/`-owned element; add a "one owner" guard that names each element family's
defining file. Update `DESIGN.md` §Components with the component names, fold the
result into `INTENT.md` (D# decision: components live in `ui/`, surfaces may not
re-implement an element), and update `README.md`/`CHANGELOG.md` per the repo's
shipping rule.

---

## 7. Verification per phase

| Phase | Normal checks |
|---|---|
| 1 | `cargo check -p orbit-pi`; `python3 scripts/check_ui_layers.py`; `cargo test -p orbit-pi --bin orbit-pi theme` |
| 2 | `cargo check -p orbit-pi`; `cargo test -p orbit-pi --bin orbit-pi helpers`; `scripts/check_states.py`; `bacon run` on settings + composer + sidebar |
| 3 | `cargo test -p orbit-pi --bin orbit-pi popup_layout`; `scripts/check_keyboard.py`; manual: open/close every migrated menu with pointer and keyboard, Escape returns focus |
| 4 | `cargo test -p orbit-pi --bin orbit-pi sidebar` / `usage` / `git`; manual scroll pass at 960×640 and one large UI size |
| 5 | `cargo test -p orbit-pi --bin orbit-pi i18n`; `scripts/check_i18n.py`; manual empty/error states |
| 6 | `cargo check -p orbit-pi --all-targets`; `cargo clippy -p orbit-pi --all-targets -- -D warnings` |
| 7 | `cargo test -p orbit-pi --bin orbit-pi theme`; all four `check_*.py`; `git diff --check` |

A phase that changes pixels additionally needs a `bacon run` pass in one light
palette, one dark palette with a non-blue accent, and Operational.

---

## 8. Risks and non-goals

**Risks**

| Risk | Mitigation |
|---|---|
| Visual drift during migration | Phases 1–4 must not restyle; a pixel change is a separate, declared commit |
| Material drift (glass decided per surface) | §3.4 rules 1–3 plus the `backdrop_blur_marker` clause in `check_ui_layers.py`; the sidebar rail's edge-less frost is the case that proved one shape is not enough |
| Perf regression in the transcript | Components on the streaming path stay allocation-free structs; no per-frame theme re-resolution; measure with ⌘⌥P (`scripts/read-perf.py`) before/after Phase 4 |
| A component that is too generic | Reject a component until 3 real call sites need it; `DESIGN.md` forbids abstractions without repetition |
| `list()` height regressions | `VirtualList` owns the contract and has a test asserting non-zero measured height |
| Focus restoration regressions | Per-dialog test, modelled on `app/focus_return_tests.rs` |
| Migration stalls halfway (two owners) | `check_ui_layers.py` + the one-owner guard in Phase 7; `app/helpers.rs` is deleted, not left as a shim |

**Non-goals**

- Not a React-style builder DSL: GPUI's fluent tree is the API, components
  wrap it only where they own something.
- Not converting entities (`ComposerInput`, `FileViewer`, `Terminal`,
  `MessageScrollerState`) — their state and perf contracts are already right.
- Not componentising content-rendered surfaces: terminal ANSI, syntax
  highlighting, diff washes, file-type marks, provider brand colours, Mermaid.
- Not adding accessibility claims: GPUI 0.2.2 has no native a11y tree; the
  component layer gives every interactive element an `ElementId`, a focus
  treatment and a shortcut route, and records the semantic gap rather than
  claiming it is solved.
- Not adding dependencies, a second icon pack, or a design-language change.

---

## 9. Definition of done

- Every element family in §4 has exactly one defining file under
  `crates/orbit-pi/src/ui/`.
- `app/helpers.rs` is deleted; nothing imports it.
- `ui/` imports no surface module (guard), and no surface names an interaction
  colour (existing guard).
- Every popup/menu/dialog has: keyboard navigation, Escape, outside-click
  dismissal, focus restore, viewport clamping.
- `VirtualList`, `Table`, `TreeRow` and the transcript still satisfy their
  virtualization contracts.
- `DESIGN.md`, `INTENT.md` (a `D#` decision), `README.md` and `CHANGELOG.md`
  describe the layer.
- The checks in §7 pass, with the manual passes recorded as manual.

---

## Appendix — reproduction

```sh
cd crates/orbit-pi/src

# element-family function definitions, excluding test code
python3 - <<'PY'
import re, pathlib, collections
pats=["card","popup","menu","dialog","chip","badge","button","field","tab","table","section","banner","toast","panel","tile","row","list","layer","switch","pill"]
hits=collections.defaultdict(list)
for p in pathlib.Path('.').rglob('*.rs'):
    if p.name.endswith('tests.rs') or p.name.startswith('test_'): continue
    src=p.read_text(errors='ignore'); i=src.find('#[cfg(test)]')
    if i!=-1: src=src[:i]
    for m in re.finditer(r'\bfn ([a-z_0-9]+)\(',src):
        for pat in pats:
            if re.search(rf'(^|_){pat}(_|$)',m.group(1)): hits[pat].append(p); break
for pat in pats: print(pat, len(hits[pat]), len(set(hits[pat])))
PY

grep -rn '\.hover(|' --include=*.rs . | grep -v helpers.rs | wc -l   # 0
grep -rn 'surface_hover' --include=*.rs . | wc -l                    # 55 (motion + rest fills)
grep -rn 'button_frame(' --include=*.rs . | grep -v 'fn ' | wc -l    # 229
grep -rn 'on_mouse_down_out' --include=*.rs . | wc -l                # 43 in 21 files
#   — the dismissal logic each popup shell re-derives
```
