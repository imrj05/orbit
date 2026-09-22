# Changelog

All notable changes to Orbit Pi are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/imrj05/orbit/compare/v0.0.5...HEAD
[0.0.1]: https://github.com/imrj05/orbit/releases/tag/v0.0.1
[0.0.2]: https://github.com/imrj05/orbit/releases/tag/v0.0.2
[0.0.3]: https://github.com/imrj05/orbit/releases/tag/v0.0.3
[0.0.4]: https://github.com/imrj05/orbit/releases/tag/v0.0.4
[0.0.5]: https://github.com/imrj05/orbit/releases/tag/v0.0.5
