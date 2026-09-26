# Changelog

All notable changes to Orbit Pi are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Zed design tokens** — `theme/tokens.rs` ports Zed's sizing system: dynamic
  spacing with Compact / Default / Comfortable density, text / headline / icon /
  button sizes, list, popover, context-menu, modal, tooltip, input and scrollbar
  metrics, the corner-radius scale, elevation shadows with `elevation_1/2/3`, and
  motion durations. Tokens equal Zed's default pixels at Orbit's defaults and
  follow the UI font size and Spacing Density settings.

### Changed

- Context menus (sidebar session / workspace, Explorer, Git sync / branch / file)
  now follow Zed's context-menu metrics: 200px minimum width, 23px entries,
  14px text and icons, Base04 / Base06 insets, `ListSeparator` spacing, an 8px
  radius with Zed's lighter popover shadow, and an 8px window-edge margin.
  Their sizes now scale with the UI font size.
- Tooltips use Zed's tooltip metrics: offset off the cursor, 14px text, and
  wrapping at 288px instead of overflowing on one line.
- Extension dialogs use Zed's modal layout: `ModalHeader` / section / footer
  insets, a Small headline, a 32px input field, 32px confirm buttons, and the
  four-layer modal shadow.
- The remaining floating modals — provider usage / API key / provider editor
  (Settings), the update dialog, and the custom-UI card — use Zed's
  `ModalSurface` (`elevation_3`): the four-layer shadow, an 8px radius, and
  `ModalHeader` / section / `ModalFooter` insets. The legacy heavier
  `popover_shadow` helper is gone.
- Sidebar session and workspace rows use Zed's list-item tokens: a 4px
  (`rounded_sm`) hover/selection radius, `TextSize` for title / label / metadata
  text, and `DynamicSpacing` for their padding and gaps.
- Transcript chrome follows the token scales: message-row padding, user-bubble
  radius / padding / text, tool and question card shells and headers, tool error
  strips, and the usage footer metric and breakdown card now read `TextSize`,
  `DynamicSpacing`, `Radius`, and `BufferLineHeight` instead of ad-hoc px.
- Settings chrome follows the token scales: the page header, section labels,
  grouped boards, and `setting_row` layout use `TextSize` / `DynamicSpacing` /
  `Radius`, the row separators use the 1px hairline token, and keycap chips size
  to `ButtonSize::Default` height. Setting cards and toolbars migrate next.
- Spacing is unified on `DynamicSpacing`. The ad-hoc `Theme::space` helper — which
  scaled with density but ignored the UI font size — is removed, and its ~185
  call sites (mostly the Git panel, Usage page, and Settings) now resolve through
  the token scale, so they follow the UI font size too.
- UI type is unified on `TextSize`. ~370 `.text_size(theme.ui_px(…))` sites snap
  onto the 10 / 12 / 14 / 16 scale (`XSmall` / `Small` / `Default` / `Large`); at
  most a 1px shift each. Sub-10px badge glyphs and 17px+ display headings, which
  the UI scale doesn't cover, keep their sizing.
- Corner radii are unified on `Radius`. gpui's fixed `rounded_sm/md/lg/xl` helpers
  and every on-scale `rounded(px(N))` (`2/4/6/8/12`) now resolve through the theme,
  so card, chip, and input corners also follow the UI font size.
- One-shot transitions adopt `AnimationDuration` (`Fast`, 150ms). Looping affordances
  (spinner, shimmer, streaming) and feedback timers keep their own cadences, which
  the three-value token scale doesn't cover.
- The settings page is fully on the token scales. Its side nav, headers, sections,
  rows, toolbars, select popups, cards, and modals now read `DynamicSpacing` /
  `TextSize` / `Radius` / `ButtonSize`; the last gpui spacing utilities and raw px
  literals are gone, leaving only bespoke geometry (avatar / dot / toggle sizes,
  fixed panel and modal widths).
- The Usage page (and its chart / heatmap / table / filters / tooltip modules) is on
  the same scales: all spacing, type, and on-scale radii resolve through the tokens;
  only chart and table geometry keeps its own px.
- Every input box follows the `InputField` tokens. The branch picker's create-branch
  field (previously frameless) now uses `input_field_frame`, and the Ask panel's
  custom-answer field uses `DynamicSpacing` offsets; all other single-line inputs
  already routed through `input_field_frame` / `picker_search_frame`. The main
  composer, file editor, and inline tree rename stay bespoke components.
- Every search box is the picker search row (`picker_search_frame`), matching the
  command palette: the settings / Git / Usage searches and the skills filter were
  boxed `InputField`s and now share the palette's flat 36px row, so all searches
  read identically. The Git panel's issue / pull-request search grows to fill its
  filter bar and shrinks when tight (min 160px) instead of a fixed 200px, so the
  Pulls tab no longer overflows with its extra state chip.
- The session-details title (rename) input follows the `InputField` tokens: its
  block uses `DynamicSpacing` / `input::gap` / `input_label` / `input_field_frame`,
  and the Generate-title and Update buttons size to `ButtonSize::Large` (32px) so
  the row lines up with the 32px field.
- Picker ↑/↓ now scroll the focused row into view. The command palette,
  workspace picker, and branch picker set a **positive** `ScrollHandle` offset,
  but gpui stores it **negative** once scrolled down (the model selector already
  negated it) — so the list never actually scrolled and the highlight walked
  off-screen, looking like ↑/↓ did nothing. They negate it now, and the palette
  also scrolls against its actual (window-capped) list height instead of the
  uncapped `list_max_h`. Two command-palette tests cover the ↑/↓ move and its
  scroll-into-view; the scroll test fails on the old sign.
- The model picker had the same ↑/↓-doesn't-stick symptom for four reasons,
  all now fixed. (1) Its open-time "pin the highlight to the active model"
  re-ran on **every** render for the first 400 ms (the hover-suppression
  window), so a ↓ pressed right after opening was silently snapped back; the
  pin now runs only on a genuine open / catalog change. (2) Its deferred scroll
  re-targeted the active model for up to 250 ms after open; it now follows the
  current highlight. (3) pi re-reports the whole catalog on every `get_state`,
  and `set_catalog` re-armed the pin and re-scrolled to the active model each
  time — so any periodic sync undid an ↑/↓ while the popup was open. An
  unchanged `set_catalog` is now a no-op. (4) The pin stayed armed until the
  popup's *first* render, so a ↑/↓ arriving before that frame (or right after a
  scope change / catalog refresh re-armed it) was undone by it; a deliberate
  ↑/↓ now cancels the pin. Covered end to end as well: a test opens the real
  `OrbitApp` with the real `bind_keys`, asserts the popup's filter actually
  holds focus, and steps the highlight through several presses.
- Picker rows no longer move the keyboard highlight on hover: the command
  palette, workspace picker, and extension-dialog option rows used `on_hover` to
  follow the pointer, and `on_hover` re-fires whenever hit-testing changes — so
  scrolling a row under a stationary pointer hijacked the highlight. They use
  `on_mouse_move` now, so only real pointer movement moves it.
- Single-line inputs no longer wrap. Every `ComposerInput` capped at
  `with_max_lines(1)` — the session-details name, the Git branch field, the
  provider / model / plugin / skill filters, the usage searches, the extension
  dialog, and the find bar — now also sets `with_wrap(false)`, so a long value is
  clipped at the edge instead of wrapping into a second row and painting the
  editor's vertical scrollbar. Non-wrapping text also scrolls horizontally to
  follow the caret, so typing past the field's width still shows what you type
  (clipped to the text area, so the code editor's gutter stays put).
- The Git panel's tab strip is on the §5 button language: each tab is a
  `button_frame(ButtonSize::Medium)` — the same 28px height, `Base08` padding,
  `Base04` gap, `button::RADIUS` (4px), and Default label as the branch chip and
  the panel's filter chips — so the tabs and the chips beside them read as one
  system. The tab strip and branch row are `Base40` tall, and the whole panel's
  spacing resolves through `DynamicSpacing` (one shared `tab_bar` for Changes /
  History / Graph / Issues / Pulls).
- The sidebar's New Task, Search, and Usage controls share one
  `button_frame(ButtonSize::Large)`: the hand-rolled 28px Search / Usage rows
  now match the New Task button's height, `Base08` padding, `button::RADIUS`
  (4px), and Default label, while keeping their ghost treatment (no fill or
  border, hover only) — one evenly sized stack.
- The model selector's option rows keep the picker's two-line token height. A
  capped list is a flex column, so without `flex_none` every row shrinks toward
  the `picker_entry` minimum (31px) instead of `picker::two_line_entry_height`
  (50px), leaving the 28px leading provider / thinking chip with almost no
  vertical padding. The no-match empty row, its provider-header gap, and the
  row's hover highlight are on the same tokens / `on_mouse_move` as the other
  pickers.

### Fixed

- The model and thinking chip pickers keep keyboard focus when opened from
  their composer chip. The composer box's own mouse-up handler (which focuses
  the input) is an ancestor of the chip and ran after it in gpui's bubble
  order, so ↑/↓/Enter/Escape went to the composer instead of the popup. The
  composer box now leaves focus alone while any of its popovers is open, and a
  click inside the model popup no longer bubbles out to it.
- The top-bar provider-quota popover no longer clips its last provider card
  when several accounts are connected: the card list scrolls inside the
  popover's height cap instead of stretching past it. The list now carries its
  own max height rather than living in a `flex_1` body — inside the deferred,
  anchored popover the available height is zero, where a flexible child
  collapses and the overflow is clipped.

## [0.0.17] - 2026-09-25

### Added

- **Workflow modes** — plan, build, or answer instead of one undifferentiated
  chat. A session is scoped **Plan**, **Build**, or **Ask** from the New Task
  page or the composer chip, per session. Plan and Ask are read-only: the
  bundled `orbit-workflow-extension` disables the write tools, gates `bash` to
  a read-only allowlist, and injects mode guidance, re-arming live sessions
  from `~/.orbit-pi/workflow.json` with no restart.
- **AI review agent** — a read-only reviewer over the current change set or the
  whole project, from the Review pane's sparkles menu or the command palette.
  It runs on its own pi process scoped to Ask mode (so the chat session and
  transcript are untouched), launched with `ORBIT_REVIEW=1` so the access guard
  never blocks its read-only `git diff`. Findings parse from a fenced JSON
  block into severity-chipped rows that scroll the diff to the offending file;
  the answer's prose is kept when no findings parse.

## [0.0.16] - 2026-09-24

### Added

- Add structured data, harden headers, and extend cache windows
- Rebuild landing around story sections and refresh OG

### Changed

- Feat/UI refinement (#23)

## [0.0.15] - 2026-09-23

### Added

- Per-tool glyph badges in the transcript, tinted by work kind, with a
  chip-style folded activity group
- Surface pi's capped-result facts on tool cards as a `truncated` chip
- Success check on the session-details Update button after a rename commits

### Changed

- Refactor the transcript UI: readable tool labels, a real shared spinner,
  shared button hover/press feedback, and redesigned git issue/PR details

## [0.0.14] - 2026-09-23

### Changed

- Auto-apply pi RPC patches and harden custom UI surfaces

## [0.0.13] - 2026-09-22

### Changed

- Feat/GitHub page features (#21)
- Feat/main updates (#20)
- Feat/GitHub page features (#19)
- Remember Open in preferences per workspace and add Rider support (#17)
- Update GitHub Sponsors username in FUNDING.yml (#18)
- Update GitHub Sponsors username in FUNDING.yml (#16)

## [0.0.12] - 2026-09-22

### Added

- Add ten new light palettes and toggle_knob role

### Fixed

- Keep the update modal's footer inside the card

## [0.0.11] - 2026-09-22

### Contributors

### Added

- **Ten more palettes** — the appearance catalog grows from 32 to 42. Two new
  Orbit-family light palettes join the set: **Orbit Paper**, a cooler neutral
  paper canvas that is the light counterpart to Orbit's warm off-white (same
  ember accent), and **Orbit Contrast**, a near-white, high-contrast palette
  with deeper ink and a stronger hairline for maximum legibility. The curated
  light counterparts of the editor themes also land — **Ayu Light**,
  **Catppuccin Latte**, **Flexoki Light**, **Gruvbox Light**, **Nord Light**,
  **One Light**, **Solarized Light**, and **Tokyonight Light** — so light is a
  family in the Appearance dropdown rather than a single option. The picker
  partitions the catalog by mode, so every choice it offers matches the active
  mode.

- **`toggle_knob` palette role** — a mode-aware token for the knob of a toggle
  switch, kept light in both modes so it reads on the accent (on) and the
  raised track (off). The knob previously used `text`, which painted a black
  dot on the light track in light mode.

### Changed

- The saved theme keys `one-light` and `flexoki-light` now resolve to the real
  **One Light** and **Flexoki Light** palettes instead of being folded into
  Orbit Light; the generic `light` alias still maps to Orbit Light.

### Fixed

- Clicking a radio row no longer leaves a stray accent focus ring behind.
  gpui 0.2 has no `:focus-visible`, so the automatic focus transfer on
  mouse-down is suppressed; the ring appears only for keyboard focus, while
  the row's own click still selects on mouse-up.

- **Dumitru Moloșnic** ([#11](https://github.com/imrj05/orbit/pull/11)) — light,
  dark, and system appearance modes; transcript table sizing, streaming
  scroll-position, and multiline command-preview fixes.

## [0.0.10] - 2026-09-21

### Added

- **Explorer** — a Zed-style file-and-folder explorer for the active workspace.
  A left **project panel** (`cmd-shift-e`, top-bar folder toggle, or the ⌘P
  row) renders the workspace as a virtualized tree: expand/collapse,
  case-insensitive filter, a hidden-files toggle, devicon glyphs, `M/A/D/R/U`
  git-status badges, and keyboard navigation (arrows, home/end,
  `cmd-←`/`cmd-→`, enter/space). The walk is gitignore-aware (nested
  `.gitignore`/`.ignore`, global and repo excludes; symlinks never followed)
  and runs off-thread with a bounded entry cap surfaced honestly as
  "truncated". Right-clicking a row offers Open, Open in Default App, Reveal
  in File Manager, and Copy Path / Relative Path. Clicking a file opens a
  full-page **Files** editor: a tab strip over syntax-highlighted
  code with line numbers, editable Markdown (with an Edit/Preview toggle), or
  an image, with a path/size/line-count toolbar and a save-status chip. Code,
  text, and Markdown files are editable and autosave to disk (debounced;
  `cmd-s` saves now); unsaved tabs carry a dot, and external changes are
  flagged rather than silently overwriting edits. Binary files, oversized
  files (2 MiB), truncated files, non-UTF-8 files, and directories get an
  honest notice instead of a lossy dump, and never enter the editor. The
  workspace watcher refreshes the tree and the open file on change. `cmd-w`
  closes the active tab (the last tab, or `cmd-shift-w`, closes the surface),
  and the toolbar X returns to the chat.

- **Editing** — the Explorer's editor (and the chat composer) supports
  **undo/redo** (`cmd-z` / `cmd-shift-z`), where a run of typing within 400 ms
  coalesces into a single undo step and a caret move starts a new one. History
  is bounded by both step count and total bytes so a whole-file buffer cannot
  balloon memory.

- **Explorer file operations** — create, rename, and delete files and folders
  from the project panel. **New File** and **New Folder** sit in the panel
  header and in a directory's right-click menu; **Rename** edits a row's name
  in place; **Delete** asks first and moves the item to the OS Trash on macOS
  (permanent delete elsewhere, with the confirmation worded to match). Names
  are validated before anything touches disk, a create never clobbers an
  existing file, and a rename refuses an occupied name. An operation is refused
  while an open Files tab under the target has unsaved edits, so it can never
  race an autosave or silently discard a buffer; a renamed file's open tab
  follows the new path, and a deleted one closes.

- **More code fonts** — the Code font picker now offers seven more bundled
  faces alongside the existing set: **DM Mono**, **IBM Plex Mono**,
  **Inconsolata**, **Noto Sans Mono**, **Space Mono**, **Anonymous Pro**, and
  **Martian Mono**. Like the rest of the catalog they ship as subset static
  TTFs, so a picked face resolves without an OS dependency.

### Fixed

- Explorer rows now fill the panel width, matching the Review pane's file tree.
  The selection/hover highlight no longer collapses to a narrow pill around the
  devicon, and git badges align to the right. This also fixes the inline rename
  and new-file prompts, which rendered as an empty bubble: the prompt input is
  a `ComposerInput`, which has no intrinsic width, so on a content-sized row
  its `flex_1` collapsed to zero. A regression test renders the real panel and
  asserts the prompt input keeps a usable width.

- The Explorer's row context menu opens at the pointer, flipping above/left
  when it would overflow the window. It was pinned to the panel's top-left, so
  a row's menu always appeared at the top of the tree regardless of where the
  row was right-clicked.

- Closing a file tab keeps keyboard focus inside the Files surface: it moves to
  the newly active editor, or to the surface itself for preview/read-only tabs,
  instead of being dropped with the removed editor. Previously focus was lost
  after the first close (when the next tab did not paint an editor), so `cmd-w`
  stopped working. Closing a tab to the left of the active one also selected the
  wrong neighbour.

- Search and filter placeholders (provider, model, settings, plugins, skills,
  side pane, git panel, usage, in-transcript find, branch/workspace pickers)
  now follow a language change immediately instead of keeping the language
  they were created in. Placeholders are resolved from translation keys at
  paint time.
- With no saved language preference, `System` now walks the OS's ordered
  preferred-language list and picks the first language Orbit ships (falling
  back through unshipped choices such as Hindi) instead of defaulting to
  English. The macOS bundle also declares its supported localizations.
- The setup page's dependency descriptions follow a language change without
  re-probing the host.

## [0.0.9] - 2026-09-21

### Added

- **Version History** in the update modal, opened from the update and
  up-to-date dialogs: every release the feed carries, newest first, each with
  its changelog and the running build marked **Current**. Appcasts now
  accumulate across releases (the newest 20 signed items), so the list grows
  with each release.

### Changed

- The update modal leads with the app icon, checks against an indeterminate
  progress bar, and names the current version when up to date.
- **Check for Updates** always opens the modal. A build that can't check
  (debug, or a binary outside a managed install) says so there instead of
  posting a toast; with `ORBIT_FORCE_UPDATER=1` and the signing key compiled
  in, such a build runs a real check-only flow without **Update now**.
- The update flow now runs through a single modal. A staged release and
  **Check for Updates…** both open it: the check shows a search progress bar,
  then the release's changelog with **Cancel** / **Update now**, while an
  already-staged release offers **Later** / **Update now**. The Settings
  update control is an icon-only download button, and the sidebar shows a
  20px download glyph that expands to **Update** on hover (the reference
  app's pattern); the changelog rides in each appcast item's `<description>`
  (written from `CHANGELOG.md` at release time).

## [0.0.8] - 2026-09-20

## [0.0.7] - 2026-09-20

### Added

- Interface localization via `rust-i18n`, shipped with ten locales — English,
  Simplified Chinese, Japanese, Korean, Spanish, French, German, Brazilian
  Portuguese, Russian, and Italian — plus a `System` option that follows the
  OS preferred language. Settings → Appearance → Language switches it live
  (including the native menu bar); `locales/en.yml` is the source of truth and
  `scripts/gen_locales.py` regenerates the other files from the glossary.

## [0.0.6] - 2026-09-19

### Added

- Clone a session straight from the sidebar — each session row's `…` menu
  gained **Clone session**, which duplicates that session on disk (fresh id,
  pi's `<timestamp>_<id>.jsonl` naming, entries copied verbatim) and drops the
  copy into the list to branch off. Unlike Delete, it only reads the source, so
  it works on any row — including one with a live pi process — without
  switching away from what you're doing. The ⌘P "Clone Session" command still
  clones the open session through pi.
- Usage page — a full-width **Daily activity** calendar heatmap: one cell per
  day across a trailing 12 months, shaded by the active metric (tokens,
  requests, cost, …), with month/weekday axes, a less→more legend, a hover
  readout, and click-to-scope to a single day (click again to restore the
  range). It is independent of the date range — a contribution graph needs a
  year to read — but the active workspace / model / provider / errors filters
  apply to every cell.

### Fixed

- App and composer shortcuts now bind GPUI's cross-platform `secondary`
  modifier (Cmd on macOS, Ctrl on Windows/Linux). They previously used
  `cmd-*`, which GPUI maps to the Windows/Super key on Windows, so
  copy/paste/cut/select-all and every other primary shortcut did nothing
  there. Shortcut hint chips now render the platform-correct label too.

## [0.0.5] - 2026-09-17

### Added

- Windows releases attach the bare `orbit-pi.exe` alongside the `.zip`, so the
  executable is a direct single-file download (`scripts/bundle-windows.ps1`).
- Linux releases attach a native `.deb` alongside the `.tar.gz`, installable
  with `apt install ./orbit-pi_*.deb` (`scripts/bundle-linux.sh`).

## [0.0.4] - 2026-09-17

## [0.0.3] - 2026-09-16

### Added

- Integrated terminal — a real login shell in a resizable bottom panel (⌘J or
  the top-bar toggle), independent of the right side pane. Built on
  `alacritty_terminal` for PTY and VT/ANSI emulation, rendered natively by
  GPUI: scrollback, click-drag selection with ⌘C/⌘V, bracketed paste, a
  blinking cursor, and a per-theme ANSI palette. The shell follows the active
  workspace and restarts when it changes.

## [0.0.2] - 2026-09-16

### Added

- Native macOS menu bar (Orbit / File / Edit / View) wired to the app's
  existing actions; ⌘N and the About surface now have a real home.

## [0.0.1] - 2026-09-14

### Added

- Chat-style `pi` sessions — streaming transcript, model selection, thinking
  effort, follow-up queueing, mid-run steering, and cancel.
- Sessions grouped by project, reopenable and clonable, with an Orbit-owned
  project list that leaves `pi`'s own session store untouched.
- Access guard — Supervised / Auto-accept edits / Full access, enforced by a
  bundled `tool_call` extension with an inline Allow once / Always allow this
  tool / Deny bar.
- Review pane with a live `git diff HEAD`, and a Git page for Changes /
  History / Graph with staging and commit.
- Workbench pages — usage, skills, plugins, models, providers, and appearance,
  read from `pi`'s on-disk data.
- In-transcript find (⌘F) and a full-window image lightbox.
- Native macOS app bundle, Developer-ID signed and notarizable.

[Unreleased]: https://github.com/imrj05/orbit/compare/v0.0.17...HEAD
[0.0.1]: https://github.com/imrj05/orbit/releases/tag/v0.0.1
[0.0.2]: https://github.com/imrj05/orbit/releases/tag/v0.0.2
[0.0.3]: https://github.com/imrj05/orbit/releases/tag/v0.0.3
[0.0.4]: https://github.com/imrj05/orbit/releases/tag/v0.0.4
[0.0.5]: https://github.com/imrj05/orbit/releases/tag/v0.0.5
[0.0.6]: https://github.com/imrj05/orbit/releases/tag/v0.0.6
[0.0.7]: https://github.com/imrj05/orbit/releases/tag/v0.0.7
[0.0.8]: https://github.com/imrj05/orbit/releases/tag/v0.0.8
[0.0.9]: https://github.com/imrj05/orbit/releases/tag/v0.0.9
[0.0.10]: https://github.com/imrj05/orbit/releases/tag/v0.0.10
[0.0.11]: https://github.com/imrj05/orbit/releases/tag/v0.0.11
[0.0.12]: https://github.com/imrj05/orbit/releases/tag/v0.0.12
[0.0.13]: https://github.com/imrj05/orbit/releases/tag/v0.0.13
[0.0.14]: https://github.com/imrj05/orbit/releases/tag/v0.0.14
[0.0.15]: https://github.com/imrj05/orbit/releases/tag/v0.0.15
[0.0.16]: https://github.com/imrj05/orbit/releases/tag/v0.0.16
[0.0.17]: https://github.com/imrj05/orbit/releases/tag/v0.0.17
