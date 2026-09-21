# Changelog

All notable changes to Orbit Pi are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.11] - 2026-09-22

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

[Unreleased]: https://github.com/imrj05/orbit/compare/v0.0.11...HEAD
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
