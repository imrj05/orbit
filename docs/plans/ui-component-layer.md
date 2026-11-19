# Plan — UI component layer

**Status (2026-11-19):** Phases 1–7 landed and committed (§6.14 in `6fe96e59`).
Every element family has one owner under `ui/`. The §6.15 sweep (buttons,
toggle/select chips, popover anchors, disclosures, Glass, `FileChip`) is done
and committed on `feat/sidebar-toolbar` (`refactor(ui): move the last
hand-built controls onto ui/ owners`).
The last 20 raw button/popover constructs are also dealt with (§6.15 last
cohort below): every site now runs an owner or carries a `// Bespoke:` reason,
so the raw-construct ratchet sits at **0**. Left: the manual pass, which has
never been done. Ratchets: raw constructs 0 (`scripts/ui_owner_budget.json`),
one-frame hovers 104 (`scripts/hover_budget.json`). Start at **Resume here**
below; §6.2–§6.15 is the per-pass log (what moved, the checks that ran and
every declared pixel change).

## Resume here

### Where it stands

| Family | Owner | State |
|---|---|---|
| Interaction states, press, tooltip | `ui/style/control.rs` | ✅ done |
| Material (frost) | `ui/style/surface.rs` | ✅ done |
| Button (8 variants, loading, disabled, faded) | `ui/button.rs` `Button` (`new` for string ids, `keyed` for tuple ids), `ButtonVariant` Primary / Secondary / Outline / Tinted / Danger / Ghost / Destructive / Glass | ✅ owner; the §6.15 sweep moved the hand-built buttons. `loading` keeps a caller's leading glyph; busy buttons no longer stay clickable |
| Icon button (ghost/glass, open, faded, glyph) | `ui/button.rs` `IconButton` | ✅ 15 `icon_button_frame` sites left, each a `// Bespoke:` control (send/stop, jump-to-latest, updater pill, open-in split, action-filled, copy-acknowledge, pager, account mark) |
| Disclosure toggle | `ui/button.rs` `disclosure_toggle` (style seam, returns the frame) | ✅ show/hide-more and expand toggles |
| Segmented control | `ui/button.rs` `ButtonGroup`, `SegmentFill`, `segmented_segment` | ✅ all 11 segmented controls; `settings_segment` / `active_segment` deleted |
| Link | `ui/button.rs` `LinkButton` | ✅ updater changelog link, transcript tool URL, Subagents' *Undo* |
| Trigger / choice / toggle chip | `ui/chip.rs` `ChipButton` (`open` / `selected` / `checked` / `applied`, caret control, + size, leading / trailing, mono, `label_tip`, `label_max_w`, disabled, shrink (truncating), `render`) | ✅ Usage filters + toggles, composer chips, Settings select, Machine, Review source, MCP/provider kinds, permissions scope, subagent tool/thinking, model scope tabs, Git triggers (branch, commit-model, label filter), merge-mode choices, the transcript activity fold, ssh alias suggestions. **Decided: neutral selected** — `surface_selected` fill, a check glyph on toggles, never an accent fill. Usage's narrowed-filter token stays `// Bespoke:` (a static chip with an inline clear) |
| File chip | `ui/chip.rs` `FileChip` (visual, name, meta, `on_open`, `on_remove`) | ✅ composer attachments + screenshots; only the nested ✕ removes |
| Badge | `ui/chip.rs` `Badge` (+ `Framed`, leading glyph) | ✅ owner; Git refs, provider badges, Explorer save state |
| Pill | `ui/chip.rs` `Pill` (bare / faced) | ✅ MCP status, composer queue count (the save-state chip proved to be a `Badge`) |
| Banner / error card | `ui/banner.rs` `Banner` (Info/Success/Warning/Error, glyph, `strip`), `error_state` | ✅ run error (via `error_state`), Git failure, Explorer notice strip, worktree setup |
| Empty / error / spinner / refresh | `ui/feedback.rs` | ✅ owner (+ action slot) |
| Progress bar | `ui/feedback.rs` `progress_bar` | ✅ 3 meters |
| Skeleton | `ui/feedback.rs` `skeleton_bar` | ✅ Usage (the only loading shape) |
| Toast card | `ui/toast.rs` `ToastCard`, `kind_glyph` | ✅ ordinary toasts + the plugin-update prompt (`on_close`: persistent, only × closes; `count`, `content` from `update_row` / `separator`, `actions`) |
| Menu cursor | `ui/menu.rs` `MenuState`, `MenuCursor` (hidden until a key, ignored by `==`), `MenuKey` | ✅ 9 menus: add, access, workflow, Work in, extension select, Explorer row menu, transcript text menu, sidebar session + workspace menus (Shift-F10 / Menu key open the row menus) |
| Menu frame + hint | `ui/menu.rs` `context_menu_*`, `menu_hint` | ✅ |
| Popover shell + dismissal | `ui/popover.rs` `popover_list`, `popover_picker`, `dismiss_on_outside`, `DismissGuard` | ✅ `on_mouse_down_out` only in `ui/` |
| Dropdown (trigger ↔ menu) | `ui/popover.rs` `dropdown_below/above`, `column_menu`, `row_menu`, `anchored_menu` | ✅ 11 dropdowns + 6 column menus; row and pointer menus on `row_menu` / `anchored_menu`; 2 raw `anchored()` left, both `// Bespoke:` |
| Modal | `ui/dialog.rs` `modal_card/scrim/footer`, `ModalHeader` (+ `modal_header`), `modal_button`, `ModalFocus`, `focus_ring` | ✅ every modal header; footer buttons are Tab stops (Enter/Space press them) in all six ring-managed form modals |
| Rows | `ui/row.rs` `PickerRow`, `picker_entry`, `option_row`, `nav_row`, `setting_row`, `RowState`, `list_row` | ✅ sidebar + pickers; Usage on `table_row`; trees on `TreeRow` (fills from `RowState`); Git's History/Graph/GitHub rows on `list_row` |
| Resize handle | `ui/resize.rs` | ✅ all 5 |
| Window chrome | `ui/chrome.rs` | ✅ moved (no API change) |
| Glyphs (file, provider, images) | `ui/glyph.rs`, `ui/icon.rs` (+ chevrons) | ✅ moved |
| Card / section | `ui/card.rs`, `SurfaceExt::card_surface` | ✅ titled board (Usage) + the card treatment at 13 sites (transcript activity/ask/subagent cards, composer strips, queue bar, Settings board) |
| Field frame / label / search / form | `ui/field.rs` `input_field_frame`, `input_label`, `picker_search_frame`, `search_field`, `TextField` (+ `multiline`), `FormRow`, `form_label` | ✅ `search_field` at 20 sites; `TextField` at 21; `FormRow` for the worktree, subagent and bug-report forms. 8 raw frames left, each with a real difference (icon inside, margins + strong border, top-aligned multi-line, fixed-height panel field) |
| Switch, keycap, tooltip view | `ui/switch.rs`, `ui/kbd.rs`, `ui/tooltip.rs` | ✅ moved |
| Tab / tab strip | `ui/tab.rs` `Tab`, `tab_strip` | ✅ terminal + file viewer. Usage's "tabs" are a segmented control (now on `segmented_segment`); Settings uses nav rows |
| VirtualList | `ui/list.rs` `virtual_list` | ✅ all 7 `list()` sites (transcript, sidebar, Review diff + tree, Explorer, viewer, scroller test) |
| Table / TableRow | `ui/table.rs` `data_table`, `Column`, `SortState`, `TableHandlers`, `table_row`, cells | ✅ the grid moved out of `usage/table.rs`; all five Usage tables and their rows (only Usage has a data table) |
| TreeRow | `ui/tree.rs` `TreeRow`, `TreeNode`, `TreeRowState`, `indent` | ✅ Explorer project tree + Review changed-files tree |
| DiffRow | `ui/diff.rs` `DiffRow`, `DiffTone`, `DiffGutter` | ✅ Review unified + split rows, transcript edit diffs; one wash set |
| Lightbox | `ui/lightbox.rs` `lightbox` | ✅ |
| Avatar | `ui/avatar.rs` `Avatar`, `initials` | ✅ Git (photo + monogram); GitHub login lookup stays in `git_panel` |
| StatusBar | `ui/status_bar.rs` `StatusBar`, `status_item`, `status_button`, `status_message` | ✅ the composer status row |
| Navigation rail | `ui/rail.rs` `tick_width`, `tick_ink`, `tick_bar`, `edge_fade`, `side_card` | ✅ transcript turn rail (state, scroll and hint lifetime stay in the transcript) |
| `app/helpers.rs` deletion (Phase 6) | `ui/prelude.rs`, `app/format.rs` | ✅ deleted; nothing imports it; `app.rs` re-exports no UI |
| One-owner guard, docs (Phase 7) | `scripts/check_ui_owners.py`; `DESIGN.md` §Components, `INTENT.md` D22, `README.md`, `CHANGELOG.md`, `AGENTS.md` | ✅ |
| Raw-control ratchet | `scripts/check_ui_owners.py` (`button_frame(` / `anchored()` per file vs `scripts/ui_owner_budget.json`; `--update` lowers it) | ✅ runs in CI's design-guards job with `check_ui_layers.py` |

### Next, in order

1. ~~**Keyboard routes for pointer menus**~~ — done (§6.11).
2. ~~**Modals**~~ — done (§6.13).
3. ~~Popovers, `DiffRow`, `StatusBar`, `Rail`, `LinkButton`, `ButtonGroup`,
   modal headers and Tab stops~~ — done (§6.13). Every element family in §4
   now has an owner under `ui/`.
4. ~~**Phase 6**~~ — done (§6.14). Was: classify what is left in `app/helpers.rs` (UI → `ui/`,
   formatting → `app/format.rs`), switch imports from `app::helpers` /
   `crate::app::{…}` re-exports to `crate::ui::…`, delete the file.
5. ~~**Phase 7**~~ — done (§6.14). Was: a one-owner guard (each family's defining file), `DESIGN.md`
   §Components, an `INTENT.md` `D#` decision, `README.md`.
6. ~~**§6.15 sweep**~~ — buttons, toggle/select chips, anchors, disclosures,
   Glass, `FileChip`, triggers, small controls: done.
7. ~~**The last 20 raw constructs**~~ — done (§6.15 last cohort): every site
   moved onto an owner or carries a `// Bespoke:` reason; ratchet at 0.
8. ~~**Commit**~~ — done, as one commit (most files were touched by several
   cohorts, so a per-cohort split would not build slice by slice).
9. **Manual pass** (never done yet): `bacon run` in one light, one dark
    non-blue-accent and the Operational palette; check sidebar rows/drag,
    composer chips (neutral selected + check glyph), file chips (only ✕
    removes), menus (pointer + keyboard), modals (Tab/Escape/focus
    return), resize handles, Usage filters and meters, and every §6.15
    declared change.

### How to work on it

- **Small cohorts, no restyle.** A migration must not move pixels; if it does,
  list it as a declared change in that pass's §6.x entry and in `CHANGELOG.md`
  when a user would notice.
- **Codemods convert only exact matches.** The ones used here (chain parser:
  match the frame's exact call list, else skip) are in the session scratchpad
  and are not kept; re-create from the pattern if useful. Report the skip
  count — it is the honest size of what is left.
- **`ui/*` must not import a surface** (`scripts/check_ui_layers.py`).
  `crate::platform` and `crate::mentions` are allowed.
- **Raw-control ratchet**: `scripts/check_ui_owners.py` fails when a file's
  raw `button_frame(` / `anchored()` count rises above
  `scripts/ui_owner_budget.json`, and when it falls (run `--update` to lock the
  gain in). A genuine one-off gets `// Bespoke: <why>` within four lines above.
- **Hover ratchet**: `scripts/check_hovers.py` fails when a file's one-frame
  hover count changes; after moving hovers *into* a `ui/` owner (total down),
  run `python3 scripts/check_hovers.py --update`.
- **rustfmt** only files whose `HEAD` version was already formatted (the
  repo is not fully formatted; do not reformat other people's code).
- **Parallel work**: another session was editing `app/remote_sessions_ui.rs`,
  `remote_sessions.rs`, `events.rs`, `toast_ui.rs`. Leave those files alone
  unless they are committed; `remote_sessions_ui.rs` has two copies of the
  `nav_row` frame/fill to adopt later.

### Verify a cohort

```sh
cargo check -p orbit-pi --all-targets
cargo test -p orbit-pi --bin orbit-pi -- ui:: popup_layout focus_return sidebar composer   # + the touched surfaces
python3 scripts/check_ui_layers.py && python3 scripts/check_states.py && \
python3 scripts/check_motion.py && python3 scripts/check_keyboard.py && \
python3 scripts/check_i18n.py && python3 scripts/check_hovers.py
cargo clippy -p orbit-pi --all-targets -- -D warnings
git diff --check
```

Last full run (2026-11-18): 921 focused tests, six guards and clippy clean.

---

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
subagent mode; their labels, keyboard and pointer handlers remain local. A
separate size-parameterized active/rest recipe preserves the bug-report type and
server-clone protocol segments, including their existing pointer handlers. It
keeps the active fill and default weight distinct from settings segments; other
segmented controls retain their distinct treatments. The new recipe's state and
geometry tests have not been run. `ui/switch.rs` now owns the existing switch
and its knob-travel test; `app/helpers.rs` re-exports `switch` unchanged, and
the existing keyboard test remains at the compatibility path. `ui/icon.rs` now
owns the shared tinted SVG icon primitives and hover group, re-exported by
`app/helpers.rs`; domain icons stay where they are. The existing layout
regression test for SVG flex shrinking remains in `app/backdrop_layout_tests.rs`
(it tests the GPUI behavior directly). This is not a general
`IconButton` API or completion of the phase. `cargo check -p orbit-pi`
passes after the switch move; the segmented-control migration has not had
Cargo checks or tests run; the icon move likewise has not had Cargo checks or
tests run. `ui/kbd.rs` now owns the existing keycap renderer; shortcut parsing
and the working-folder legend remain in `app/helpers.rs`. This move preserves
the existing rendering; `cargo check -p orbit-pi` and the working-folder hint
test pass. Manual visual verification remains pending. Switch tests and manual
visual/glass verification remain pending.

`Button`, `IconButton`, `ButtonGroup`, `LinkButton`, `Chip`, `ChipButton`,
`Badge`, `Pill`, `Kbd` (keycap moved), `Switch` (moved), `Icon`.
Migrate the 44 `*_button` defs and the 27 chip / 11 badge / 6 pill defs.
**Acceptance:** zero `*_button` definitions outside `ui/`; `primary` resolves in
one place; every migrated surface renders identically (spot check: settings,
composer, sidebar, git panel, transcript footer).

### Phase 3 — `Menu`, `Dropdown`, `Popover`, `Modal`
**Partial:** `ui/menu.rs` provides pure highlight navigation, used by the
composer's add and access menus. It also owns the existing context-menu surface,
entry and separator helpers, re-exported through `app/helpers.rs`; positioning,
row content, focus and dismissal remain local. `ui/popover.rs` owns the existing
anchoring and entrance helpers; `ui/dialog.rs` owns the existing modal card and
scrim. These are frame moves, not a dropdown, focus trap or dismissal host.
Unit tests cover highlight
navigation for empty, wrap, single-item and changing list lengths. `cargo check
-p orbit-pi`, the menu-state tests, and the popup-layout tests pass. Manual
interaction and visual verification remain pending. Other menus and the planned
components remain unmigrated.

Build `MenuState` first (pure, unit-tested like `command_palette.rs`'s scoring),
then `Menu`/`MenuItem`, then `Dropdown`, then `Modal` (header/body/footer + focus
trap) and `Popover` (merging `picker_surface` + `context_menu_surface`).
Migrate the 24 popups and 12 layers.
**Acceptance:** no `on_mouse_down_out` outside `ui/popover.rs`/`ui/dropdown.rs`;
no menu next/prev logic outside `ui/menu.rs`; focus restore tested per migrated
dialog; `app/popup_layout_tests.rs` geometry assertions still pass.

### Phase 4 — Containers and rows
**Partial:** `ui/card.rs` owns the existing Usage card and section recipes, and
`ui/row.rs` owns the existing settings-row composition. Other surfaces and the
broader container/list APIs remain unmigrated; visual verification is pending.

`Card`, `Section`, `MetricBoard`, `ListRow`, `NavRow`, `TableRow`, `Table`,
`TabStrip`, `TreeRow`, `VirtualList`, `PageHeader`, `SectionHeader`.
Migrate the 31 card / 43 section / 95 row defs, starting with the surfaces that
repeat most: settings, usage, git panel, sidebar, transcript.
**Acceptance:** row density and trailing-action reveal come from one place;
`VirtualList` is used by every virtualized surface and no surface wraps it in
`overflow_y_scroll`; scroll behaviour unchanged in a manual pass.

### Phase 5 — Fields, feedback, banner
**Partial:** `ui/field.rs` owns the existing input frame and label, and
`ui/feedback.rs` owns the existing empty/error states and spinner recipes.
Compatibility exports preserve existing callers. The planned action slot and
remaining feedback components are not implemented; visual verification is pending.

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

## 6.2 Landed in the component pass (2026-11-17)

Verified: `cargo check -p orbit-pi --all-targets` warning-free; focused
`cargo test` runs over `ui::`, settings, MCP, worktrees, subagents, Git, Usage,
permissions, providers, dialogs, sidepane, explorer, i18n and theme;
`check_ui_layers.py`, `check_states.py`, `check_motion.py`,
`check_keyboard.py`, `check_i18n.py`. **Not verified:** a manual `bacon run`
pass of the migrated surfaces across palettes.

- **`Button` (Phase 2).** `ButtonVariant::{Primary, Secondary, Outline, Tinted,
  Danger, Ghost}` — the variant → fill/hairline/ink/hover/press map lives only in
  `ui/button.rs`. `Button` owns size, label, leading icon or element, tooltip,
  `loading` (spinner, keeps width, cannot act) and `disabled` (shared disabled
  state; a ghost stays flat). Handlers go through `on_click` / `on_mouse_up` /
  `on_mouse_down` / `faded(…)` and are attached only while the button can act;
  `faded` cross-fades the hover on `Motion::Interaction`. `press` moved to
  `ui/style/control.rs`. Migrated: permissions, subagents (enum removed), MCP,
  updater dialog, Git action bar, provider cards (`ProviderButtonStyle`
  removed), worktree + clone dialogs, plugins, skills, runtime, retention,
  provider model fetch (now a real `.loading`), Usage text buttons.
  "Primary" now means the action fill everywhere; the worktree/clone accent
  wash is the separately named `Tinted`.
- **Declared pixel changes** (rule 7): primary buttons that had no edge gain a
  transparent 1px border (2px wider, aligned with their secondary siblings);
  primary press is the colour ramp only (no extra dim); Git's disabled action
  uses `disabled_state` (`border_subtle` edge); Usage's disabled text button
  inks `text_disabled`; retention/plugin/runtime/permissions-primary labels are
  medium weight; clone-dialog secondary gains the press dim; provider badges
  are 18px (Compact) instead of 20px; Worktrees' error banner uses the shared
  12px inset, `error @ 0.07` wash and Base04 line gap; `error_state`'s glyph is
  `circle-x`.
- **`Badge` (Phase 2).** One geometry (Compact height, `Radius::Small`, XSmall
  medium), `BadgeFill::{Solid, Outline}`, caller-owned tone. Migrated: Git ref
  badges, provider badges.
- **`MenuState` (Phase 3).** Adds `highlighted`, `first`/`last`,
  `next_enabled`/`prev_enabled` (plain `next`/`prev` delegate) and `reconcile`.
  The composer workflow menu, worktree "Work in" menu and extension select
  dialog dropped their hand-rolled wrap arithmetic; the latter two gained
  Home/End, and "Work in" Enter reconciles against rows that changed while open.
- **`Banner` (Phase 5).** `BannerTone::{Warning, Error}` with title, detail,
  hint, custom lines and an action; `error_state` renders through it. Migrated:
  Worktrees error, inactive-subagents notice.
- **`EmptyState` action slot (Phase 5).** `empty_state_with_action`; Review's
  "Changes unavailable" offers Retry.

Not yet owned (still per surface): `IconButton` beyond the frame recipes,
`Chip`/`ChipButton`/`Pill`, `Dropdown`/`Popover` host/`Modal` focus trap,
remaining banners (`error_banner`, Git `failure_banner`, Explorer notice strip,
worktree setup), `ListRow`/`NavRow`/`Table`/`TabStrip`/`VirtualList`, fields,
toasts, chrome. Wrappers like `provider_button` that bind a surface action and
only pick a variant are intended to stay (§6.1 cross-cutting gates).

## 6.3 Overlay pass (2026-11-18)

Verified: clippy `-D warnings` on all targets; focused `cargo test` over
`ui::`, popup layout, focus return, session details, rewind, Explorer, Git,
Usage, sidebar, worktrees, pickers, MCP, remote, provider accounts, server
clone and dialogs; all six repo guards. A new focus-return test was checked
to fail with the restore removed. **Not verified:** a manual pointer and
keyboard pass of the migrated menus and modals.

- **`DismissGuard`** replaces 9 hand-copied timestamp fields
  (`menu_dismissed_at` on the app, side pane, Explorer and Git panel, plus
  Git's sync/commit-model/branch-more and Explorer's header and Usage's chip
  guards) and ~30 copies of the 200 ms check. `swallows()` consumes the arm
  (a trigger's click), `is_recent()` does not (a surface that must not repaint
  under the dismissing click). Declared timing change: Explorer's context-menu
  window goes from 150 ms and Usage's chip window from 250 ms to the shared
  200 ms (`CLICK_THROUGH`).
- **`PopoverExt::dismiss_on_outside`** — the plan's Phase 3 acceptance:
  `on_mouse_down_out` now appears only in `ui/popover.rs`. All 41 sites already
  paired it with `.occlude()`; the pair became one call, so hit testing is
  unchanged. `picker_surface` moved here from `app/helpers.rs` (re-exported).
- **`ModalFocus`** gives six modals that left focus wherever it fell the
  capture-on-open / restore-the-frame-after-close contract: MCP editor, remote
  host editor, provider account editor, remote key, add-to-a-server, SSH
  password. `ReturnFocus` moved from `app.rs` (re-exported). The clone dialog's
  Tab cycle uses the shared `focus_ring`; from outside the ring, Tab now enters
  at the first stop rather than the second.

- **Tab ring (2026-11-18).** Those six modals now trap Tab / Shift-Tab in their
  painted fields (`OrbitApp::modal_fields` + `ui::dialog::focus_ring`). Before
  this, Tab inside a modal field resolved to the composer's
  `AutocompleteAccept` and did nothing. The four Settings-painted modals own
  the keyboard (and restore it) only while Settings is open. Tests drive real
  Tab/Shift-Tab keystrokes and were checked to fail with the ring removed.
- **`modal_header` / `modal_footer`** (`ui/dialog.rs`, `ModalSubtitle::{Text,
  Id}`): 7 headers and 7 footers (remote host, MCP, SSH password, account,
  provider usage, provider key, provider editor) were byte-identical copies and
  now call these. Declared change: the account editor's footer gap is the
  shared 4px (`modal::footer_gap`) instead of 8px. Not migrated because they
  differ visibly: the updater (no hairlines), the worktree dialog (`Large`
  title), the extension dialog (eyebrow line).

- **`popover_list` / `popover_picker`** (`ui/popover.rs`): the two popover
  shells. A list is the surface + `picker::list_padding_y` + a `Base01` row gap
  in a column; a picker is the surface in a clipped column. 25 popovers now
  start from one of them (9 lists, 16 pickers) with identical styles, so no
  pixel change; widths stay with the caller. Two outliers keep
  `picker_surface` (the sidebar options popover, which does not clip, and the
  worktree Advanced popover, which has no column).
- The extension dialog's eyebrow is localized (`dialog.question`).

Still per surface: the `Dropdown` trigger/anchor pairing (each trigger still
owns its open flag and anchor), and focusable modal *buttons* (only the clone
dialog's buttons are tab stops; the rings above cover fields).

## 6.4 Rows pass (2026-11-18)

Verified: clippy `-D warnings`; focused tests over `ui::`, popup layout,
sidebar, Explorer, Review, composer, model/branch/workspace pickers and
worktrees; the six guards. **Not verified:** a manual look at the moved rows.

- **`picker_entry` + `PickerRow`** (with `EntryMarkLike`) moved from
  `app/helpers.rs` to `ui/row.rs` unchanged and re-exported, so the 61 picker
  rows and 8 `PickerRow` bodies now have their owner in the UI layer.
- **`option_row`** (`ui/row.rs`): the access-mode and workflow-mode pickers
  carried byte-identical 59-line copies of one row (glyph tile, name over a
  wrapping description, check on the choice in force); both now call it.
- **`menu_hint`** (`ui/menu.rs`): the trailing shortcut / disabled-reason hint
  copied in the composer add menu, Review's source menu and Explorer's context
  menu. Declared change: the add menu's hint gains `flex_none`, so under a
  narrow window the label truncates before the shortcut does.

- **`NavRow`** (`ui/row.rs`, 2026-11-18): `nav_row(el, theme, NavRowHeight,
  cursor)` is the navigation-row frame — full width, `Header` (Default) or
  `Item` (Medium) minimum height, list-item corners, a centered row, and a 1px
  hairline reserved in every state that turns `focus_ring` on the keyboard
  cursor. `nav_row_fill(theme, selected, cursor)` is the one fill rule (the open
  row holds `surface_selected` at rest and under the pointer; the cursor rests
  on the wash; other rows fade it in) for `motion::hover`. It is a style seam,
  not a wrapper struct, because these rows chain drag, drop, scroll-wheel and
  right-click handlers. The sidebar's project header, session row, "Show more"
  and "Show less" now use it; "Show more/less" move from a fixed to a minimum
  height (same pixels for their one-line content). Padding and gap stay with
  each row on purpose. Tests pin geometry-independence of the cursor in every
  palette and the fill rule. **Follow-up:** `app/remote_sessions_ui.rs` (new,
  from parallel work) carries two more copies of this frame and fill and should
  adopt `nav_row` / `nav_row_fill`.

What remains in Phase 4 is mostly not copy-paste: the sidebar's 26 row
functions, Git's 23 and the Usage tables each carry surface-specific state,
drag/reorder and virtualization. They need `ListRow`/`NavRow`/`TableRow`
designed against those behaviours, not extracted mechanically.

## 6.5 Controls pass (2026-11-18)

Verified: clippy `-D warnings`; 848 focused tests (composer layout, Usage,
subagents, terminal, context meter, Explorer, session, transcript, MCP, `ui::`);
the six guards. **Not verified:** a manual look at the migrated controls.

- **`IconButton`** (`ui/button.rs`): `IconButton::new(id, icon)` or
  `IconButton::glyph(id, element)` with size, `IconButtonVariant::{Ghost,
  Glass}`, ink, tooltip and `disabled`, attaching its handler only while it can
  act. 13 sites use it (a strict codemod converted the 12 whose chains were
  exactly the shared treatment; the Subagents limit stepper is the first
  disabled user). Declared change: converted buttons that had no press
  feedback gain the shared wash press, and the stepper's hover is the shared
  wash instead of the overlay. The remaining ~74 `icon_button_frame` sites
  fade their hover, wrap in `press()`, carry an open state, or are bespoke
  chrome (open-in split button, updater pill); they need `IconButton` to grow a
  faded mode and an `open` state before they move.
- **`ChipButton`** (`ui/chip.rs`): trigger chips in two named styles —
  `Quiet` (composer: no frame, faded open fill, turning caret) and
  `Outlined { inset }` (Usage filters: hairline, raised face, static caret) —
  with icon, label (+ truncation width), trailing glyph, open, caret and tooltip.
  Usage's `filters::chip` (10 chips) and the composer's access and workflow
  chips use it; test selectors are kept (`<selector>-label`).
- **`disclosure_chevron` / `dropdown_chevron`** moved from `app/helpers.rs` to
  `ui/icon.rs` with their tests.
- Hover ratchet rebaselined: 240 → 229 one-frame hovers, the increase confined
  to the new owners that absorbed the call-site hovers.

Deferred on purpose: `ToastCard` (`app/toast_ui.rs` has uncommitted work from
a parallel session), the model/thinking chips (they carry provider marks and a
compact mode), and the families listed in §6.1.

## 6.6 Dropdown and faded icon buttons (2026-11-18)

Verified: clippy `-D warnings`; 547 focused tests including popup layout,
pickers, Git, Explorer, sidebar and composer; the six guards. **Not verified:**
a manual pass of the moved menus and buttons.

- **`IconButton`** gains `open` (active fill, primary ink) and
  `faded(theme, attach)`: the hover fades on `Motion::Interaction`, an open
  button holds `surface_active` under the pointer, and `attach` adds the
  caller's handler and extras (reveal-on-row-hover opacity, an anchored popover
  child). The sidebar's sort/options button, the project header's
  reveal-on-hover "new session" button and the session-details close button use
  it. Declared: the close button presses with the shared wash instead of the
  opacity dim.
- **`dropdown_below` / `dropdown_above`** (`ui/popover.rs`, `DropdownAlign::{Start,
  End}`): the trigger-to-menu pairing — a zero-size anchor at the trigger's
  bottom (or top) corner and the anchored, snapped, entering menu `gap` away.
  Nine menus that spelled this out now call it (open-in, quota, workspace /
  branch / remote pickers, Explorer header, three Git menus). Same corner,
  offset and snap, so no pixel change. The composer's mode menus keep their
  column layout (their menu is a sibling above the chip, not an anchored child).
  The trigger keeps its open flag and `DismissGuard`: a `Dropdown` that also
  owns open state would need the state in the UI layer, which GPUI 0.2.2 makes
  an entity per dropdown — not worth it while each owner already has the flag.

## 6.7 Handles, progress, chrome and glyphs (2026-11-18)

Verified: clippy `-D warnings`; 921 focused tests (`ui::`, helpers, devicons,
sidebar, Review, Explorer, Git, terminal, Usage, Settings, session, context
meter, popup and backdrop layout, composer); the six guards. **Not verified:**
a manual drag of each handle and a look at the moved chrome.

- **`ResizeHandle`** (`ui/resize.rs`): `resize_handle(id, theme, ResizeAxis,
  ResizeLook::{Wash, Seam, Border, Divider})` owns the axis cursor and how a
  grab strip answers the pointer; position, width and the drag payload stay
  with the surface. All five handles use it: the Review and Explorer dock
  edges (`Wash`, previously byte-identical copies), the sidebar edge (`Seam`),
  Git's list/detail divider (`Border`) and the terminal splitter (`Divider`).
  Hover groups are now named after the handle's id.
- **`progress_bar`** (`ui/feedback.rs`, `ProgressSize::{Small, Medium,
  Large}` = 4/5/6px, `ProgressTrack::{Solid, Glass}`, optional `Motion::Layout`
  reveal): the quota popover meter, Settings' provider quota meter and Usage's
  legend bar. Non-finite fractions draw an empty bar. The context meter's
  stacked multi-slice bar is a different control and stays.
- **`ui/chrome.rs`**: the top bar's glass chip, lift, header icon/ghost buttons,
  the app-drawn caption buttons and the drag strip, moved from `app/helpers.rs`
  unchanged (re-exported).
- **`ui/glyph.rs`**: embedded images, devicon file glyphs, tiles and badges,
  and provider tints, moved from `app/helpers.rs` with their tests.
- `app/helpers.rs` is 462 lines (from 1,391 at the start of this work); what
  remains is mostly domain formatting (`session_display_title`, runtime
  text), the star banner, the picker search frame, `menu_header` and the
  working-folder legend — Phase 6 material.

## 6.8 Pill, skeleton, toast, tabs, avatar, lightbox, search, list (2026-11-18)

Verified: `cargo check -p orbit-pi --all-targets` and `cargo clippy -p orbit-pi
--all-targets -- -D warnings` clean; 810 focused tests (`ui::`, popup layout,
focus return, sidebar, composer, toast, terminal, Explorer, Usage, Git, MCP,
Review, scroller, pickers, palette, rewind, Settings, skills, subagents,
open-in); the six guards (hover budget rebaselined 229 → 227, the hovers moved
into `ui/tab.rs` and `ui/toast.rs`); `git diff --check`. rustfmt ran only on
files whose `HEAD` was formatted. **Not verified:** a manual `bacon run` pass.
No declared pixel changes — every move keeps the call site's metrics.

- **`Pill`** (`ui/chip.rs`): a status read-out, glyph + label in one tone.
  Bare (Small, `Base04` gap) for the MCP connection state; faced
  (`.face(fill, ButtonSize)`: capsule, XSmall, `Base02` gap) for the
  composer's queued-messages count. Reports, never acts.
- **`skeleton_bar`** (`ui/feedback.rs`): the static trough bar Usage's loading
  page is built from. No shimmer, by design.
- **`ToastCard`** (`ui/toast.rs`): anatomy and the kind → glyph/tint map moved
  out of `app/toast_ui.rs`; TTL, deck, hold and the enter/exit animation stay
  in the layer, wrapped around the returned card. Tested: only warning/error
  take a status hue, every kind has its own glyph.
- **`Tab` / `tab_strip`** (`ui/tab.rs`): the terminal's and file viewer's
  byte-identical tab chips. Density follows `ButtonSize` (Default: `Base06`
  inset / `Base04` gap; Medium: `Base08` / `Base06`); the close button stops
  propagation itself, so a stale-index select can no longer be reintroduced at
  a call site. Optional unsaved dot.
- **`Avatar`** (`ui/avatar.rs`): photo with monogram on load and failure; the
  monogram's tint and initials moved from `git_panel` with tests. The GitHub
  login resolution stays in `git_panel` (domain).
- **`lightbox`** (`ui/lightbox.rs`): the image lightbox frame; the caller owns
  the open image and the close handler.
- **`search_field`** (`ui/field.rs`, with `picker_search_frame` moved from
  `app/helpers.rs`): frame + search glass + `flex_1` input. A strict codemod
  converted 20 of 21 sites; the ⌘F bar (`app/search.rs`) is a floating keyed
  element and keeps the raw frame. Callers import it from `crate::ui::field`
  directly (no `app::helpers` re-export).
- **`virtual_list`** (`ui/list.rs`): `list()` with `h_full` pinned, plus a
  GPUI test that the list fills a `flex_1 min_h_0` parent and paints only its
  visible rows. Every `list()` site uses it.

Still open after this pass: `TreeRow`, `Table`/`TableRow`, `DiffRow`,
`StatusBar`, the transcript `Rail`, `TextField`/`FormRow`, `LinkButton` /
`ButtonGroup`, and the migrations in **Next, in order** (icon buttons,
model/thinking chips, banners, menus, Phases 6–7).

## 6.9 Table and TreeRow (2026-11-18)

Verified: `cargo check` / `cargo clippy -p orbit-pi --all-targets -- -D
warnings` clean; 656 focused tests (`ui::`, Usage, Explorer, Review, transcript,
popup layout, focus return, sidebar) plus the 84 tree-specific ones; the six
guards (hover budget 227 → 221: the Usage row hovers moved into
`table_row`); `git diff --check`. **Not verified:** a manual `bacon run` pass.

- **`TreeRow`** (`ui/tree.rs`): indent (`indent(depth, directory)`: 7px base,
  14px step, files +16px so names align with sibling folders — tested), the
  turning disclosure chevron and folder mark, the name (Small; Medium weight
  for folders), an inline editor slot (rename), trailing marks, optional
  indent guides and focus ring, and one fill per `TreeRowState::{Editing,
  Selected, Cursor, Rest}` (tested per palette). Handlers stay with the
  surface. Explorer and Review's changed-files tree use it.
  **Declared pixel changes (Review tree only):** the keyboard-cursor wash is
  `overlay` (was `overlay_strong` on folders, `surface_hover` on files — now
  Explorer's); folders turn one chevron instead of swapping two glyphs; the
  right inset is `Base08` (was 7px). Explorer is unchanged apart from test
  hooks: the caret's animation key is `<row id>-caret`, and the focus ring's
  selector is `tree-row-focus-ring`.
- **`Table`** (`ui/table.rs`): the generic grid moved out of `usage/table.rs`
  — `data_table` (frame, sticky header band, body scroll rules, empty slot),
  `Column`, `SortState` (three-state cycle, tested), `TableHandlers` (sort,
  resize start/move/end), the header cell's sort caret and drag divider,
  `cell_shell`, `text_cell`, `two_line_cell`, `status_badge`, `empty_cell` and
  `clip_to` (tested). New: **`table_row`** — the body-row frame (48px, column
  min width, hairline except the last, hover), which the five Usage row loops
  had each spelled out; callers keep ids, selection fill and handlers.
  `usage/table.rs` keeps only `TableKind` and the failures table's keys and
  cell text. Column-divider ids are now `<table id>-col-handle-<column>`
  (were `usage-col-handle-<column>`).
- **`num_font`** moved to `ui/style/text.rs` (tabular, lining figures);
  Review and the transcript import it from there, Usage re-exports it.

## 6.10 Adoption pass: built components onto their remaining sites (2026-11-18)

Verified: `cargo clippy -p orbit-pi --all-targets -- -D warnings` clean; the
**full** `cargo test -p orbit-pi --bin orbit-pi` (2150 passed, 9 ignored); the
six guards (hover budget 221 → 185); `git diff --check`. rustfmt ran only on
files formatted at `HEAD`. **Not verified:** a manual `bacon run` pass.

- **`IconButton`** gains `on_mouse_down`. 55 call sites moved (Git panel,
  transcript footers and code copy, composer attach/queue/error actions,
  Review toggles and file steps, session details, MCP, Usage, sidebar,
  remote hosts/sessions, Explorer, search, terminal, worktrees, provider
  accounts, custom UI, toast prompt). Reveal-on-row-hover, extra borders and
  focus stay as chains on the returned element. The 19 left are named
  bespoke controls, each with a `// Bespoke:` comment.
- **`ChipButton`** gains `size`, `leading` (a caller-built mark), `disabled`
  (shared disabled treatment, never open, no hover or handler) and `shrink`;
  the composer's model and thinking chips use it.
- **`Badge`** gains `BadgeFill::Framed` and a leading glyph; Explorer's
  save-state chip is a badge.
- **`Banner`** gains `Info` / `Success` tones, a caller glyph (a spinner) and
  `Banner::strip`; Git's failure banner, Explorer's notice strip and the
  worktree setup banner use it. Tested: tone colours, strip ink.
- **Explorer row menu on `MenuState`**: one `MenuItem` list feeds pointer and
  keyboard (`run_menu_item`); arrows/Home/End walk it, Enter/Space run the
  highlighted item, Shift-F10 or the Menu key opens it on the cursor row.
  Tested: layout invariants and a keyboard open → wrap → Enter round trip.
- **`column_menu`** (`ui/popover.rs`): the placement six composer-column
  menus spelled out.
- **`SurfaceExt::card_surface`**: the frosted-card chain (corner + frost +
  fill), adopted at 13 sites including `ui/card.rs` itself.
- **`modal_header`** at three more dialogs (remote key, server clone,
  worktree).
- Usage's segmented control uses `segmented_segment` (same radius).

**Declared pixel changes:** converted icon buttons that dimmed by opacity or
had no press now take the shared wash press; the sidebar/worktree "more"
buttons and the idle generate-title button gain the ghost hover; disabled
icon buttons show the not-allowed cursor; the queue-edit save glyph disables
to `text_disabled` (was muted at 50%); the composer "+" and Git sync caret
glyphs take primary ink while open. Git's failure banner is the shared
`Banner` (Default-size title, 12px padding, `circle-x`, `error` at 7%/35%,
copy/dismiss in the action slot); the Explorer strip pads `Base08`/`Base04`
(was 10/5px); the worktree setup banner puts its glyph beside the column
(lines indent under the title) with 12px padding (was 16). Explorer's
save-state chip is 18px (was 20) with XSmall medium text. The worktree
dialog's header gains the hairline and the modal headline size.

Left in this bucket: keyboard routes for the transcript text menu and the
sidebar session/workspace menus; modal buttons as tab stops; the updater,
extension and custom-UI headers; Git's row functions (`ListRow`); the Git
label popover and sidebar layout menu placements.

## 6.11 Keyboard routes for the pointer menus (2026-11-18)

Verified: clippy `-D warnings` clean; the full `cargo test -p orbit-pi --bin
orbit-pi` (2156 passed, 9 ignored); the six guards; `git diff --check`.
**Not verified:** a manual pass with the keyboard in the running app.

- **`MenuCursor` / `MenuKey`** (`ui/menu.rs`): a pointer-openable menu's
  cursor — hidden until the first key, which shows it on the first entry
  that can act; disabled entries are skipped; `MenuCursor::keyed` for a menu
  opened from the keyboard. Its `PartialEq` always matches, so menus that
  compare themselves to decide a toggle-close still do. Tested.
- **`Hover::held`** (`motion/interactive.rs`): holds a row's hover look at
  rest, so the keyboard cursor reads exactly as the pointer's hover.
- **Transcript text menu**: takes focus with its own handle when it opens
  (the transcript has no keyboard owner) and returns it to the opener on
  every close. Arrows / Home / End / Enter / Space / Escape drive it; chords
  such as ⌘C pass through. Copy Selection is skipped while nothing is
  selected. Its actions already have keys (⌘C, ⇧⌘C), so no open key was
  added. Tested: skip, wrap, Enter copies, focus returns.
- **Sidebar session and workspace menus**: `session_menu_items` /
  `workspace_menu_items` list each entry with whether it can act;
  `run_row_menu_item` calls the same handlers the rows' clicks call. While a
  menu is open the sidebar's own up/down/home/end/enter actions drive it.
  New **`SidebarOpenRowMenu`** (Shift-F10, the Menu key; `Sidebar` context)
  opens the cursor row's menu under the row with its first enabled entry
  lit. The delete confirmation and the icon picker keep their own controls:
  Enter never confirms a delete, Escape cancels it. Tested: open, End/Home,
  arrows drive the menu not the rows, Enter runs Copy Path, Enter on the
  confirmation does nothing.

## 6.12 Fields, forms and list rows (2026-11-18)

Verified: clippy `-D warnings` clean; the full `cargo test -p orbit-pi --bin
orbit-pi` (2158 passed, 9 ignored); the six guards; `git diff --check`.
**Not verified:** a manual look at the forms and Git lists.

- **`TextField`** (`ui/field.rs`): the field frame at full width on a
  caller-chosen fill with the input filling the row; `.multiline(min_h)`
  stretches the input column to the box (the shim Git and the bug report each
  carried as `items_stretch` / `stretch` is gone). 21 sites.
- **`FormRow`** / **`form_label`**: label (Small, medium, secondary) with an
  optional action at its right and hint below, the control, an optional help
  line. `worktree_field` is now a thin wrapper (its `worktree_field_label`
  is gone); the subagent and bug-report fields use it. Tested.
- **`RowState`** / **`list_row`** (`ui/row.rs`): the selected / cursor / rest
  fill and the faded hover wash every list row shares; Git's History, Graph
  and GitHub rows use it (`row_rest` is gone) and `TreeRow` takes its fills
  from it. The row frame stays the caller's — the GitHub cards and the
  commit rows have different geometry on purpose. Tested.
- **Decided, not built:** `LinkButton` (only two inline text links exist —
  the updater's changelog link and a transcript tool URL; every other
  "open on GitHub" is already a `Button`), and `ButtonGroup` (above).

**Declared pixel changes:** bug-report field labels take secondary ink
(were primary), like every other form label; single-line fields now always
wrap the input in a `flex_1 min_w_0` slot (no visible change expected).

## 6.13 The last components (2026-11-18)

Verified: clippy `-D warnings` clean; the full `cargo test -p orbit-pi --bin
orbit-pi` (2164 passed, 9 ignored); the six guards (hover budget 185 → 177);
`git diff --check`. **Not verified:** a manual pass in the running app.

- **`LinkButton`**: inline accent-ink link, optional external arrow,
  underline and grow/truncate, ink fading toward the accent. Updater
  changelog link, transcript tool URL.
- **`ButtonGroup`** (+ `SegmentFill::{Selected, Active}`): the whole
  segmented control — geometry, rest face (or inset), chosen fill, medium
  labels, disabled options, the hover and selection cross-fading together,
  and the click. All 11 segmented controls (Settings ×6, subagent mode,
  quota tabs, permissions, bug-report type, clone protocol, Usage).
  `settings_segment` and `active_segment` are deleted. Tested.
- **Popovers**: the Git label menu and the sidebar options menu are
  `dropdown_below`; nothing outside `ui/` calls `anchored_popover` now.
- **`StatusBar`** (`ui/status_bar.rs`): the row, the leading/trailing split,
  the wallpaper-protecting fill, and the item shapes. Tested.
- **Navigation rail** (`ui/rail.rs`): tick fan-out, ink, bar, edge fades and
  the side-card shell; tested. The transcript keeps the turn data, hover,
  scrolling and the hint's lifetime.
- **`DiffRow`** (`ui/diff.rs`): tone → edge / gutter / body washes and the
  row anatomy, with a number gutter (Review) or a marker gutter
  (transcript). Paint-only: the caller supplies the text runs and font.
  Tested.
- **Modals**: `ModalHeader` (eyebrow, subtitle, description, leading mark,
  trailing control, optional hairline) now builds every header, including
  the updater, extension and custom-UI ones. `Button::focus` +
  `modal_button` make footer buttons Tab stops; the six ring-managed form
  modals share `modal_cancel_focus` / `modal_confirm_focus` and their ring
  runs fields → Cancel → Save (tested).

**Declared pixel changes:** segmented controls all take medium labels and
the cross-faded hover/selection (permissions segments gain the raised rest
face; the quota tabs lose their 2px gaps and abut; no segment dims on
press). The transcript tool URL fades its ink on hover. The Git label menu
opens 5px under its trigger (was 2px). Transcript edit-diff washes use
Review's strengths (body 20%/12%, gutter 15%/9%; were 18/12 and 24/15), and
Review's unified code body pads 12px on the right (was 10). The extension
dialog header uses the modal header gap and drops the description's extra
bottom margin; the custom-UI header uses the modal headline, padding-based
height (was a fixed 64px) and the standard hairline (was `border_subtle`).
The MCP, remote-host and account editors' Cancel/Save are `Button`s
(Secondary/Primary), and footer buttons show the focus ring when tabbed to.

## 6.14 Phase 6 and Phase 7 (2026-11-18)

- **Phase 6:** `app/helpers.rs` is deleted. Its UI pieces moved to their
  owners (`kbd_shortcut` and the working-folder legend → `ui/kbd.rs`,
  `menu_header` → `ui/menu.rs`, `picker_row_height` → `ui/row.rs`,
  `text_width` → `ui/style/text.rs`), the star banner to `app/sidebar.rs` and
  the Runtime value cells to `app/settings.rs`; formatting and small domain
  helpers to `app/format.rs`. Its re-exports and `app.rs`'s became
  `ui/prelude.rs`, and every `crate::app::{…UI…}` import now names
  `crate::ui::…`. Four surface items that shadowed component names were
  renamed (`FileTab`, `EditDiffLine`, `filter_field`, `picker_empty_state`).
- **Phase 7:** `scripts/check_ui_owners.py` (+ `--self-test`) fails a second
  definition of any of 22 components / 33 owning functions and any raw
  `anchored_popover` / `on_mouse_down_out` / `segmented_segment` outside
  `ui/`, and requires a `// Bespoke:` note on a raw `icon_button_frame`.
  `review::TreeRow` (the diff tree's data model) is its one named exception.
  DESIGN.md §Components gained the owner table, INTENT.md D22 records the
  decision, and README, CHANGELOG and AGENTS describe the layer.

## 6.15 Audit after Phase 7, and the button sweep (2026-11-18)

**Now (2026-11-19, after the cohorts below):**

| Raw construct | Sites | State |
|---|---:|---|
| `button_frame(` + `anchored()` outside `ui/` | **0** (was 98 once the guard counted past the first `cfg(test)`; the last 20 went in the final cohort below) | ratcheted by `scripts/ui_owner_budget.json` |
| GPUI `anchored()` | 2 | both `// Bespoke:` — the sidebar *Show* submenu (inside an already-deferred popup) and the worktree toolbar menu (window-space, unsnapped so it cannot slide over its button) |
| `icon_button_frame` | 15 | all `// Bespoke:` |
| `picker_surface` / `context_menu_surface` | 11 | not yet ratcheted; `popover_list` / `popover_picker` where the shape matches |
| hand focus-ring border | 7 | unchanged; `RowState` / `TextField` |
| plugin-update prompt card | 0 | done (Tail cohort) |
| one-frame hovers | 104 | `scripts/hover_budget.json` |

CI now runs `check_ui_layers.py` and `check_ui_owners.py` (+ `--self-test`).

**At the start of the sweep (2026-11-18).** The guards passed, but they did
not watch every raw construct. An audit of what surfaces still built by hand
outside `ui/`:

| Raw construct | Sites | What it should be |
|---|---:|---|
| `button_frame(` | **67** (was 85) | `Button` / `IconButton`, or `// Bespoke:` |
| GPUI `anchored()` | 13 | `anchored_popover_at` / `dropdown_*` (pointer menus mostly) |
| `picker_surface` / `context_menu_surface` | 10 | `popover_list` / `popover_picker` where the shape matches |
| hand focus-ring border | 7 | `RowState` / `TextField` (`remote_sessions_ui.rs` ×2, `mcp_ui.rs` ×2, `view.rs` ×2, `transcript_view.rs` ×1) |
| plugin-update prompt card | 1 | `ToastCard` (`app/toast_ui.rs`) |

Also, then: CI (`.github/workflows/ci.yml`) did not run `check_ui_owners.py` or
`check_ui_layers.py` (fixed in the second cohort).

**Button sweep, first cohort.** `Button::keyed(id: impl Into<ElementId>, …)`
added beside `Button::new` (string ids), for tuple-keyed rows. 18 hand-built
buttons now use `Button`: 7 Secondary (response retry, Usage retry / clear,
three Settings modal Close/Cancel, subagent and updater actions), 6 Primary
(MCP ×2, quota "Show MCP servers", remote-host Add, provider Add (faded),
provider editor Save), 2 Ghost (attachments Clear, Usage), plus the
`press(…)`-wrapped ones unwrapped. Declared changes: Primary gains the shared
1px transparent edge (2px wider); remote-host Add gains the action hover it
lacked; Secondary gains the press dim, and the Settings modal buttons the
secondary ink + medium weight every other modal's Cancel has; Ghost gains the
wash press. Verified: 987 focused tests, the seven guards, clippy `-D
warnings`; hover ratchet 177 → 161. **Not verified:** a manual look.

**What the remaining button sites are** (classified, not yet moved):
- **On/off toggle chips** — Usage `toggle_button`, Subagents tool + thinking
  chips, MCP exposure chips, permissions scope, a Settings select. They do
  *not* agree with each other (Usage: `surface_active` on the canvas with a
  faded glass wash; Subagents: `surface_selected`, a checkbox, muted ink, hover
  only while off), so one owner means a design decision first — likely a
  `ChipButton` toggle mode — not a mechanical move.
- **Faded near-variants** — wrapped in `motion::hover` instead of the swap;
  `Button::faded` covers them once each is checked.
- **Icon-only glass buttons without an id** — the composer attachment remove
  buttons (`composer_ops.rs`, ×6): `IconButton` with `Glass`, ids from their
  debug selectors.
- **Real one-offs** — send/stop, the toast action buttons, the updater pill
  states, the open-state source trigger: keep and add `// Bespoke:`.

**Second cohort and the ratchet (same day).** 9 more sites: the attachment
remove and sidebar search Clear are `IconButton` (Glass / faded; their frames
are now square, ~2px narrower); two Settings toolbar buttons are faded
Secondary; a new **`ButtonVariant::Destructive`** (filled `error`, `error_fg`
ink, `hover_danger`) owns the confirm of an irreversible step — the sidebar's
two delete confirmations and Explorer's prompt, whose Cancels became
Secondary / Outline (declared: they gain the hairline, and the project-delete
Cancel gains the hover it lacked). Verified: 432 focused tests, the seven
guards, clippy `-D warnings`; hover ratchet 161 → 158.

`check_ui_owners.py` now **ratchets** raw `button_frame(` and GPUI
`anchored()` outside `ui/`: each file may keep at most its count in
`scripts/ui_owner_budget.json`, a `// Bespoke:` comment within four lines
above exempts a site, and the budget only goes down (`--update` after a
migration). Baseline: **98 raw constructs in 27 files** — higher than the 67
above, because the old classifier and the guard both stopped reading a file at
its first `#[cfg(test)]`, even when that guarded one test fn mid-file (fixed:
only a test *module* starts the test section; the self-test covers it). CI now
runs `check_ui_layers.py` and `check_ui_owners.py` (+ `--self-test`).

**Toggle chips and select triggers (decided: neutral selected).** They were
three behaviours drawn five ways. `ChipButton` now has three states, each on
the role `DESIGN.md` assigns it:

| State | Fill | Edge | Ink | Glyph | Hover |
|---|---|---|---|---|---|
| `open(true)` — a trigger whose menu shows | `surface_active` | `border_strong` | primary | caret | held |
| `selected(true)` — one choice of several | `surface_selected` | `border_strong` | primary | — | held |
| `checked(true)` — an independent on/off | `surface_selected` | `border_strong` | primary | leading check | held |
| rest | `surface_elevated` (canvas when `inset`) | `border` | secondary | — | shared wash |

`selected` / `checked` drop the caret (a choice opens nothing); the check keeps
on/off from resting on colour alone. Also new: `mono()` (code-face label),
`render()` (no handler — placeholders), `on_click`, and `selector` takes an
owned string. Migrated (11): Usage `toggle_button`, Subagent tool chips
(checked, mono) and thinking chips, MCP transport kind and provider API kind
(**declared: accent tint → neutral selected**), permissions scope, model-picker
scope tabs (leading mark + trailing count), the Settings select trigger
(declared: open fill `overlay` → `surface_active`), the provider Machine
trigger (menu in a `relative` wrapper), Review's source trigger (declared:
gains the open fill), and the two "no models" placeholders (now the shared
disabled state). Other declared changes: tool chips' resting ink muted →
secondary and checkbox → check; scope tabs take the chip radius and label size;
Usage toggles swap their hover instead of fading. Tests pin each state's fill
and edge in every palette, and that choices/toggles carry no caret. Verified:
526 focused tests, the seven guards, clippy `-D warnings`. Ratchets: raw
constructs 98 → 86, one-frame hovers 158 → 150. **Not verified:** a manual
look.

Deliberately not moved: the composer **branch chip** (live pending state,
per-label tooltip, ahead/behind counts — needs `ChipButton` to grow a
`pending` state first), Usage's **filter token** (`toggle_chip`: a static
"narrowed" chip with an inline clear — its own small family), and Settings'
**model checklist rows** (full-width checkbox rows, a row family, not chips).

**Popover anchors and the next cohort (same day).** Two new owners in
`ui/popover.rs`: **`row_menu(at, corner, below, popup)`** — a row's action menu
at the pointer after a right-click, or hung from the row's corner on the
keyboard route (the sidebar session and workspace menus and the worktree row
menu each spelled this out) — and **`anchored_menu(corner, offset, popup)`**,
the local, snapped, deferred panel with no entrance (the sidebar hover card,
the remote-host and machine menus, Usage's row menu and filter panel). Same
corner, offset and snap: no pixel change. Two anchors are marked
`// Bespoke:` with the reason: the Worktrees *Advanced* menu (window-space and
unsnapped on purpose) and the sidebar's *Show* submenu (nested inside the
already-deferred options popup). Buttons: worktrees Clone/New, a Git conflict
action and a Git list action are Secondary; the Git and Worktrees back buttons
are Ghost (declared: wash press instead of the dim); the empty sidebar's
*New session* is a faded Primary and the empty search *Clear* a faded
Secondary (declared: it gains the raised face). The test-section fix also went
into the migration classifier, which surfaced buttons it had been skipping.
Verified: 528 focused tests, the seven guards, clippy `-D warnings`.
Ratchets: raw constructs 86 → **65**, one-frame hovers 150 → 144. `sidebar.rs`
is at 0. **Not verified:** a manual look.

**Git panel and view.rs (same day).** Git's `github_state_tab` is a Quiet
`ChipButton` choice, `filter_toggle_chip` a checked toggle (declared: accent
fill → neutral selected + check, per the decision above) and `dropdown_chip` a
trigger. That retired the last accent-filled control, so `hover_accent` and
`PressedRamp::Accent` were deleted. The onboarding *Refresh* and *Migrate pi*
buttons are Ghost `Button`s with `loading` — which also **fixes a real bug**:
while busy they dimmed but kept their click handler, so they could be pressed
again mid-operation. `Button::loading` now keeps a caller-built `leading`
element (Refresh's turning glyph) instead of swapping in the generic spinner.
Marked `// Bespoke:` with verified reasons: Git's delete-branch checkbox (an
explicit off state for a consequential option), the composer send-hint row
(inert, borrows the frame only to align), and the composer branch chip (live
pending state + ahead/behind counts). Verified: 479 focused tests, the seven
guards, clippy `-D warnings`. Ratchets: raw constructs 65 → **57**, one-frame
hovers 144 → 138. **Not verified:** a manual look.

**Tail cohort (same day).** Plugin-update toast *Skip* (Ghost) and *Update
now* (Primary); terminal *Restart* (Primary — its icon was primary ink on the
action fill, now `action_fg`); *Compact now* (Secondary + `loading`); GitHub
*Check for updates* (Secondary + `loading`, replacing a 60% dim); *Clear
remembered approvals* (Secondary, `disabled` when empty); the worktree how-to
dismiss (`IconButton` Glass); and the updater's *Downloading…* / *Updating…*,
which were inert look-alikes of the same button and are now its `loading`
state (declared: they gain the spinner). Marked `// Bespoke:`: the Usage
pager's current page (an inert marker among page buttons). Verified: 406
focused tests, the seven guards, clippy. Ratchets: raw constructs 57 → **47**,
one-frame hovers 138 → 131.

**Disclosures and verified one-offs (same day).** New **`disclosure_toggle(id,
theme, size)`** in `ui/button.rs`: the text disclosure frame (medium muted
label, ink lifting under the pointer; the caller places the chevron, label and
note). The transcript's tool-section fold, edit-diff fold and turn fold use it
(declared: the turn fold's ink now lifts to primary like the others, and
`hover_ink_quiet` — its last user — was deleted). Marked `// Bespoke:` with
checked reasons: Usage's calendar day cells (the grid's own selected /
in-range / today fills), the labelled jump-to-latest (the same floating,
frosted control as its icon twin), and Git's Sync (the left half of a split
button). Kept pending as a real family rather than one-offs: the composer
attachment chip and the bug-report screenshot chips are one **file chip**
(body opens a preview, a nested button removes) — a `FileChip` owner, two
sites. Verified: 512 focused tests, the seven guards, clippy. Ratchets: raw
constructs 47 → **41**, one-frame hovers 131 → 129.

**Glass controls and FileChip (same day).** New **`ButtonVariant::Glass`**
(the `overlay` ink wash, no hairline, `hover_overlay_strong`) for a control on
glass or a translucent panel: the ask panel's *Back*, the MCP popup's secondary
action and the error banner's *Reconnect*. The ask panel's *Next / Submit* is
`Tinted` (declared: it gains Tinted's accent hairline). Both ask buttons now
use `disabled` instead of a 45% dim, so they stop accepting clicks once the
questionnaire is submitted. New **`FileChip`** (`ui/chip.rs`, registered in
`check_ui_owners.py`): thumbnail or glyph, truncated name with its tooltip,
optional meta, a nested remove button and an optional preview on the body.
The composer attachment chip and the bug-report screenshot chips use it.
**Declared behaviour change:** a screenshot chip used to remove itself on a
click anywhere; now only its ✕ removes — the attachment chip's existing rule
that a chip big enough to read is big enough to hit by accident. Verified: 558
focused tests, the seven guards (+ self-test), clippy. Ratchets: raw constructs
41 → **35**, one-frame hovers 129 → 124.

**Triggers onto ChipButton (2026-11-19).** No new state was needed: Git's
branch control, worktree *Advanced* and the status-bar *Work in* are `open`
triggers; the worktree mode chips (declared: accent → neutral, per the
decision) and the plugin scope chips are `selected` choices; the models
*Favorites* filter is a `checked` toggle. New: `ChipButton::label_tip` — the
full value on a truncated label, separate from the chip's tooltip — so *Work
in* keeps both its "working folder" tip and the path tip the design rules ask
for. Declared: Advanced's label takes the chip label size (was Small), and its
open fill moves from `surface_hover` to the shared `surface_active`.
Verified: 398 focused tests, the seven guards, clippy. Ratchets: raw constructs
35 → **29**, one-frame hovers 124 → 119.

**Small-control sweep (2026-11-19).** Nine more: the worktree text and icon
buttons (faded Secondary; the icon one keeps its padded frame), branch-picker
*Create* (Primary, now keyed), Git *Load more* (Secondary + `loading`; it
gains the raised face), the Explorer markdown *Edit / Preview* toggle
(Secondary), Usage's menu-footer actions (Ghost; ink muted → secondary), the
session-details name actions (Primary / Outline, faded), the remote-host
`~/.ssh/config` alias suggestions (`ChipButton` mono), and Git-accounts
*Refresh* (Secondary + `loading`, keeping its turning glyph). Verified: 608
focused tests, the seven guards, clippy. Ratchets: raw constructs 29 → **20**,
one-frame hovers 119 → 114.

**The last twenty (2026-11-19).** The remaining 20 raw constructs, each onto
an owner or a `// Bespoke:` reason; after this the raw-construct ratchet sits
at **0**:

*Moved onto owners (14).*
- `Button` — Settings show/hide-all-models (Ghost); the session-details
  *Review* / *Git* actions (`Button::keyed` Outline); the Usage pager's page
  buttons (`keyed` Ghost `faded`, its numeral font chained); the composer
  retry-bar *Cancel* (Ghost Compact `faded`); the terminal page's *Back*
  (Ghost `faded`); the MCP status-bar chip (Ghost, leading its muted icon and
  state dot — declared: its quiet hover becomes the shared wash); Git's
  *Stash changes* (Secondary, declared: `hover_bordered_ink` → the variant
  hover, and its last user was deleted); Git's *Generate* (Outline +
  `loading`, declared: the in-flight chip's `overlay` fill and muted text
  become the shared loading treatment and the spinner takes the variant ink).
- `ChipButton` — Git's commit-model trigger (Outlined, `open(true)`, leading
  provider mark; declared: the open fill moves `overlay` → `surface_active`,
  as the Settings select trigger did); the GitHub label-filter (Outlined +
  `applied(active)` beside `open` — the state the applied decision added — and
  its compact mode is `caret(false)`, collapsing to the tag mark the tooltip
  names; declared: the label ink lifts to primary while a filter is in force
  and the hover is held when open); the merge-mode choices in the ref picker
  (Quiet `selected`; declared: selected ink `selection_fg` → primary, per the
  neutral-selected decision); the transcript's activity-fold chip (Outlined
  `open`, glyph cluster as the leading mark, `shrink`; the owner grew
  truncate-when-`shrink` so the title gives way inside the column — declared:
  the open state takes the shared open fill, the chevron is the chip's static
  one, and the open label reads primary).
- `Tab` — the Git tab strip, whose fills, inks, press and density were already
  this owner's (declared: the label takes `TextSize::Small` and loses its
  hand weight swap, which the owner owns).
- `LinkButton` — Subagents' *Undo* (declared: its wash hover becomes the
  link's accent deepening).

*`// Bespoke:` (6), with verified reasons.*
- Settings' provider model rows (a full-width checkbox row, a row family, not
  a chip) and the model-selector *Manage models…* (carries its chord after the
  label in muted ink — the label slot is a single run).
- `view.rs`'s new-task folder trigger (one of the composer header's three
  peers, needing `track_focus` and a key context and action the chip owner
  does not expose) and the approval tiles (the deny option wants a danger
  hover on the glass face and the keyboard cursor swaps the hairline per
  render).
- Usage's filter token (its own small family: a static "narrowed" chip with an
  inline clear) and the failures drill-down link (a bare danger link; Danger
  would add the chip chrome).

Verified: `cargo check` clean, the seven guards, hover ratchet 114 → 104.
**Not verified:** a manual look.

## 6.1 Remaining owner work — actionable backlog

> The checkboxes below are the original backlog and acceptance criteria. Current
> state per family is the **Resume here** table at the top; several items are
> partly done and stay unticked until their acceptance criteria are met.

This is a **proposed backlog**, not a list of shipped components. The current
`ui/` files include moved helpers and recipes, not necessarily the full owners
named below. Existing surface functionality must keep working throughout each
migration; moving a frame alone does not satisfy an ownership task. Confirm the
current call sites before each landing, since this inventory is a snapshot.
Each task needs relevant automated tests, the applicable §7 checks and a recorded
manual pass; do not remove compatibility exports until their callers migrate.

### Controls and identity (Phase 2)

- [ ] Complete `Button`, `IconButton`, `ButtonGroup` and `LinkButton` over the
  existing frames, navigation/quiet icon and segment recipes. Centralize variants,
  sizing, loading and disabled behavior, while leaving surface actions and localized
  labels with callers. **Accept:** primary has one treatment; disabled/loading
  cannot invoke an action; pointer and existing Enter/Space routes, focus indication,
  selected segments, tooltips and shortcuts survive migration. Test state and
  activation behavior; spot-check settings, composer, sidebar, git and transcript.
- [ ] Add `Chip`, `ChipButton`, `Badge` and `Pill` ownership for the existing
  attachment, status and trigger shapes. **Accept:** distinguish static status
  from actionable triggers; preserve removal, disabled/selected states, accessible
  keyboard routes, labels and tooltip behavior; migrate representative callers and
  test their actions rather than treating decorative badges as buttons.
- [ ] Complete `Kbd`/`Shortcut` around the moved keycap renderer and shared
  shortcut formatting, without changing the working-folder legend. Complete
  `Icon`, `FileGlyph`, `ProviderMark` and `Avatar` around the existing SVG helpers;
  surface/domain data and brand choices remain inputs. **Accept:** key sequences,
  fallbacks, tint, sizing and SVG flex behavior are unchanged; test formatting and
  missing-glyph fallback and visually check transcript, menus and file rows.
- [ ] Keep the moved `Switch` owner; migrate compatibility imports and verify its
  existing toggle, disabled, focus and keyboard behavior with switch tests and a
  manual settings pass. It is not evidence that the other controls are complete.

### Overlay hosts and navigation (Phase 3)

- [ ] Extend the partial `MenuState` beyond index next/prev: home/end, selection,
  disabled-item skipping and reconciliation when items change. Build `Menu`,
  `MenuItem`, `MenuHeader` and `MenuSeparator` over the moved recipes. **Accept:**
  empty/all-disabled menus are safe; pointer and keyboard select the same enabled
  action; arrows wrap, Home/End and Enter work, Escape dismisses, highlighted rows
  scroll into view, and submenu/checked/danger/disabled-reason content survives.
  Unit-test each transition and changing lists; manually exercise pointer and
  keyboard context menus including a keyboard route to pointer-anchored menus.
- [ ] Build `Dropdown` and a `Popover` host over the moved anchoring/entrance
  helpers, retaining trigger ownership and menu contents as separate inputs.
  **Accept:** viewport snapping, outside-click and Escape dismissal, click-through
  suppression and trigger focus restoration work for pointer and keyboard; opening
  a second popup and dismissing a stale one cannot steal focus. Test dismissal,
  restore and geometry; manually check edge-of-window placement and scrolling.
  Keep popover lists bounded and scrollable; do not use `uniform_list` in a
  deferred anchored popover or nest a virtual list in an outer scroller.
- [ ] Build `Modal` on the moved scrim/card with header/body/footer, focus trap,
  initial focus, Escape/outside dismissal where the dialog permits it, and focus
  restoration. Preserve destructive confirmations and extension responses,
  including cancel semantics. **Accept:** Tab/Shift-Tab stay within the open
  modal, closing returns focus to the invoking control, and a rejected or cancelled
  operation never looks successful. Test each migrated dialog's focus return and
  cancellation; exercise dialog and nested overlay flows manually.
- [ ] Compose the existing command palette as `Palette` from modal and row
  owners without replacing its search/state model. **Accept:** existing shortcuts,
  ranking, sections, keyboard selection, dismissal and focus return are unchanged;
  test search/selection and manually open it from both keyboard and sidebar.
- [ ] Consolidate `Tooltip` hosting around the existing `ui/tooltip.rs` and
  `TipExt`, preserving delayed display, positioning, dismissal and localized text.
  **Accept:** one host contract serves usage and ordinary controls without
  stealing focus or intercepting clicks; manually check pointer/focus and window
  edges. Do not claim a focus-triggered tooltip unless implemented and tested.

### Containers, navigation and data (Phase 4)

- [ ] Complete `Card`, `Section`, `Pane`, `MetricBoard`/`MetricCell`,
  `PageHeader` and `SectionHeader` beyond the Usage-only recipes. **Accept:**
  callers supply content, headers and metrics; shared geometry and material are
  resolved once, nested cards are avoided, and settings/usage/git retain their
  visual hierarchy. Compare glass on/off and light/dark without changing geometry.
- [ ] Complete `ListRow`, `NavRow` and `TableRow` beyond `setting_row`; add
  `TabStrip`/`Tab`, `TreeRow`, and the transcript navigation `Rail`. **Accept:**
  density, slots, selection, disclosure and trailing-action reveal have shared
  ownership while surface state/actions stay local; pointer and existing keyboard
  routes, focus, tab selection and tree expand/collapse remain intact. Test
  navigation and disclosure; manually check sidebar, explorer, terminal tabs and
  transcript rail with long labels and at small window sizes.
- [ ] Implement `VirtualList` and `Table` (header/columns/sort) around existing
  state and data sources, not a replacement for them. **Accept:** definite list
  height (`h_full` within `flex_1 min_h_0`), no outer scroll wrapper, virtualized
  rows, unchanged sort/selection and keyboard behavior, and nonzero measured
  height. Preserve transcript bottom alignment, tail-follow, remeasurement and
  jump-to-latest, and sidebar/tree scroll positions. Test geometry and sort, then
  manually scroll large sidebar, tree, usage table and streaming transcript; record
  before/after frame evidence using `scripts/read-perf.py` for the streaming path.
- [ ] Give `DiffRow`/hunk headers a shared paint-only owner without moving diff
  parsing or syntax highlighting into `ui/`. **Accept:** unchanged hunk navigation,
  copy/selection and streaming layout; no highlighting-driven reflow. Test the
  relevant diff model/render behavior and manually inspect Review and transcript.

### Fields, feedback and remaining chrome (Phases 4–5)

- [ ] Complete `TextField`, `SearchField`, `FieldLabel` and `FormRow` over the
  moved frame/label; add `ComposerFrame` without replacing `ComposerInput` or
  `FileViewer` entities. **Accept:** input, selection, IME behavior as currently
  supported, submit/escape and focus routes remain intact; translated placeholders
  resolve at paint time and validation/help text stays associated with its field.
  Test field actions and language switching; manually check forms and composer.
- [ ] Complete `EmptyState`/`ErrorState` from the moved helpers with optional
  resolving actions; create `Banner`, `Skeleton`, `Progress`, `Spinner` and
  `ToastCard` owners around existing feedback recipes and the existing toast
  model. **Accept:** errors retain what happened, detail and actionable next step;
  empty actions actually invoke their callbacks; toast TTL/dedupe/dismissal and
  progress values remain truthful; reduce motion is honored. Test actions,
  toast lifecycle and motion-off behavior; manually inspect empty, error, loading
  and notification states, including keyboard dismissal where supported.
- [ ] Add `Lightbox`, `ResizeHandle`, `StatusBar` and window/title-bar chrome
  owners for the existing surface behavior, not new visual designs. **Accept:**
  lightbox close/Escape and focus return, splitter pointer resizing and persisted
  sizes, status actions, platform window controls and title-bar drag/double-click
  retain their existing behavior. Test state/persistence and focus where feasible;
  manually exercise the platform-specific flows and record untested platforms.

### Cross-cutting completion gates

- [ ] For each owner, resolve theme/material at paint through §3.4, preserve
  glass-off geometry and platform blur limitations, and use existing interaction,
  motion and localization contracts. Never import a surface into `ui/`; keep
  wallpaper, charts, terminal rendering and long-lived entities in their existing
  owners. Extract only where real reuse justifies it; document a named exception
  rather than forcing a unique surface into a generic wrapper.
- [ ] Finish migrations in small cohorts: test behavior and run the appropriate
  guards, manually compare affected surfaces, then remove superseded duplicate
  ownership and compatibility exports. Phase 6 must classify domain helpers
  before removing `app/helpers.rs`; Phase 7 adds guards and shipping docs only
  after implementation and verification. Scope the zero-duplicate checks to
  reusable owners, not unrelated surface-specific actions or state machines.

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
