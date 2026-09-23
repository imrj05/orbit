# AGENT.md — Working on Orbit

Project guidance for agents and humans. Read this before touching the repo.

When this file conflicts with `INTENT.md` on architecture choices, **`INTENT.md` wins**.
Update `INTENT.md` if a recorded decision (D1–D6) changes.

## What this project is

**Orbit** is a native desktop workbench for the [pi coding agent](https://github.com/earendil-works/pi) —
a chat-style GUI rendered **entirely in Rust** on **GPUI** (Zed's GPU-accelerated UI framework),
speaking the **pi CLI's RPC protocol** directly over stdio.

The reference architecture is [Waku](https://github.com/egoist/waku): native Rust UI on the GPU,
pi as a child process, no web, no webview, no Node daemon.

**Product docs:** `PRODUCT.md` (positioning, capabilities, constraints) · `README.md` (status, setup) ·
`INTENT.md` (decisions and rationale).

## Hard rules

1. **No web anywhere in the UI.** Do not reintroduce React, Vite, Tauri, webviews, DOM, Tailwind, or Node tooling. New UI is GPUI only. The legacy `src-tauri/` tree has been deleted; never rebuild it.
2. **The pi CLI is the only agent runtime.** Spawn `pi --mode rpc` as a child process per open session and speak newline-delimited JSON over stdio (`crates/orbit-rpc`). There is no Node daemon.
3. **GPUI is pre-1.0 and pinned.** `crates/orbit-pi` uses `gpui = { version = "0.2.2", features = ["runtime_shaders"] }`. Upgrade deliberately on a schedule, never track `main`.
4. **Performance is a product requirement.** The transcript and sidebar are virtualized (`list()`). The UI drains RPC events on a ~90 ms heartbeat — never block a frame with I/O. Syntax highlighting, when it lands, must be paint-only so streaming never reflows.
5. **Trust pi's truth.** Render real RPC events and on-disk state; nothing decorative pretending to be functional. Access modes the protocol can't deliver are shown as unavailable (or as a static fact), not faked. The sidebar **Search** row opens the command palette (sessions/commands/settings) — it is not a transcript-content search; do not wire a fake one.

## Repo layout

```
Cargo.toml              workspace: orbit-pi, orbit-rpc
crates/orbit-pi/        GPUI app — window, shell, chat, settings
  src/main.rs           bootstrap, assets, keybindings, heartbeat, native menu (localized)
  src/i18n.rs           AppLanguage + locale detection; the `tr!`/`tr_cow!` macros
  locales/              translations: en.yml is the source of truth, one file per locale
                        (generated from en.yml + scripts/i18n_glossary*.py)
  src/app.rs            OrbitApp state model + shared types + controller wiring (module map in the file header)
  src/app/runtime.rs    pi process lifecycle, status/error, provider auth
  src/app/events.rs     heartbeat event drain, RPC response routing, session/workspace watchers
  src/app/session.rs    prompt/queue/turn lifecycle, abort, session switching + navigation
  src/app/pickers.rs    model, command-palette, branch, and new-task workspace pickers
  src/app/composer_ops.rs autocomplete, attachments, "+" add-menu, model/thinking chips
  src/app/sidebar.rs    sidebar rows + session/workspace row menus; project-list prefs
  src/app/settings.rs   Settings surface: General/Runtime/Agent/Skills/Plugins/Models/Appearance/Providers/About (+ provider CRUD, plugin install, model list)
  src/app/skills_ui.rs  Settings → Skills master-detail page and its controllers
  src/app/updater_ui.rs signed-updater modal (search/Version History/Update now), rows, pill, event drain
  src/app/search.rs     in-transcript find (⌘F): matches, counts, prev/next, row wash
  src/app/toast_ui.rs   in-app toast stack: push/dismiss helpers + the bottom-right layer
  src/app/view.rs       top-level chrome: sidebar, transcript, composer, status bar, onboarding, image lightbox
  src/app/open_in.rs    installed-editor/terminal detection and the "open in" menu
  src/app/pi_update_ui.rs launch-time pi self-update: background check, install, toasts
  src/app/helpers.rs    icon/file-glyph UI kit + small formatting helpers
  src/auth.rs           non-sensitive provider-auth state machine for the auth.* RPC namespace (login/cancel/timeout/restart recovery)
  src/transcript.rs     virtualized messages (snapshot + stream)
  src/transcript_view.rs Waku-style transcript paint (rail, activity cards, copy); plain-GPUI message rows (no component library)
  src/message_scroller.rs tail-following list + jump-to-latest (original, plain gpui ListState)
  src/composer.rs       multi-line EntityInputHandler (wraps, auto-grows, scrolls)
  src/model_selector.rs model / thinking picker popover
  src/notifications.rs  background notifications + channel prefs in ~/.orbit-pi/notifications.json: desktop banners (UNUserNotificationCenter; its delegate routes a clicked banner back to the session; osascript fallback when unbundled), system alert sound, and in-app toasts (the frontmost-window stand-in for the banner)
  src/toast.rs          the toast stack model: kinds, TTLs, fade, dedupe, cap (painted by app/toast_ui.rs)
  src/pi_update.rs      pi self-update check + delegated install: latest-version fetch (pi.dev), version ordering, `pi update self`, failure extraction
  src/command_palette.rs  ⌘P command palette (sections, fuzzy match, modal scrim layer)
  src/workspace_picker.rs new-task folder selector (recent folders + native browse)
  src/sessions.rs       reads ~/.pi/agent/sessions (+ debounced session-store watcher)
  src/watch.rs          shared debounced fs watching (workspace tree for Review/Git)
  src/explorer/         workspace file tree + file editor
    walk.rs             gitignore-aware background snapshot (`ignore` crate)
    tree.rs             pure tree: expand/collapse, filter, git badges
    panel.rs            ProjectPanel entity: right dock, virtualized tree
    viewer.rs           FileViewer entity: editable code/text, markdown/image views, autosave
  docs/explorer.md      Explorer design (project panel + Files surface)
  src/assets.rs         include_dir AssetSource (SVGs + app icon)
  src/app_icon.rs       dock icon (macOS; Alpha in debug builds) + Settings → About mark
  src/review.rs         git diff model: sources, parsing, context gaps, changed-files tree
  src/checkpoint.rs     per-turn git snapshot refs (refs/orbit/…) backing Review's Last Turn
  src/highlight.rs      paint-only syntax lexer for diff lines (Waku port, 16 languages)
  src/sidepane.rs       Review pane: virtualized diff, sticky file headers, tree, source menu
  src/git.rs            git plumbing: branch discovery, review diffs, status/staging/history/graph
  src/git_panel.rs      full-page Git surface: Changes / History / Graph + commit bar
  src/terminal.rs       bottom terminal panel (⌘J): alacritty_terminal PTY + VT grid, GPUI canvas render, keys/selection/paste
  src/providers.rs      provider catalog + models.json / auth.json read/write for Settings → Providers (provider metadata introspected from pi-ai; curated table is the fallback)
  src/quota.rs          account quota/balance/spend reducer over the `quota.*` RPC and the bridge's session entries (no secrets, no I/O)
  src/access.rs         access mode (Supervised / Auto-accept edits / Full access): persistence to `~/.orbit-pi/access.json`, which the guard extension reads
  src/bundled_extensions.rs  materializes the bundled pi extensions (quota bridge + access guard) under `~/.orbit-pi/` and spawns every session with `--extension`
  src/commit_message.rs one-shot, tool-free `pi -p` conventional-commit generation
  assets/icons/         HugeIcons SVGs (MIT) + provider brand marks
  assets/fonts/         SymbolsNerdFont-Regular.ttf
  assets/app-icon.png   512px app mark (from assets/icons/logo-icon.png)
  dev-assets/alpha-app-icon.png  512px Alpha mark; macOS debug-only asset overlay (scripts/make-alpha-icon.sh)
crates/orbit-rpc/       pi CLI process + JSONL protocol
  src/client.rs         spawn, writer/reader/stderr threads, kill-on-drop
  src/types.rs          CommandBody / Event (permissive serde) + auth.* / quota.* wire types
  docs/auth-rpc.md      provider-auth RPC server contract (secrets never cross it)
  docs/quota-rpc.md     provider-quota RPC server contract (normalized, non-secret)
  tests/live_pi.rs      live get_state / prompt / process-alive (skip if no pi)
  tests/live_catalog.rs live get_available_models + thinking levels
  tests/auth_fake_pi.rs scripted-server coverage of the auth.* round trip
  tests/quota_fake_pi.rs scripted-server coverage of the quota.list round trip + `--extension` forwarding
contrib/pi-auth-rpc/    apply.mjs: inject auth.* into an installed pi build
                        (browser OAuth until pi ships the server upstream)
contrib/pi-quota-rpc/    apply.mjs + quota-handler.js: inject quota.list into an
                        installed pi build (per-provider usage adapters)
                        quota-handler.test.mjs: node --test coverage of the
                        network-free adapters (unsupported + dispatch)
contrib/orbit-quota-extension/  Orbit's bundled pi extension (loaded with
                        `pi --extension`): fetches quota via pi's own auth and
                        appends normalized `orbit:quota` session entries.
                        adapters.js is the extension twin of quota-handler.js;
                        index.test.mjs / adapters.test.mjs are `node --test`
contrib/orbit-guard-extension/  Orbit's bundled pi extension (loaded with
                        `pi --extension`): hooks `tool_call`, asks the user
                        (via `ctx.ui.select` → `extension_ui_request`) before
                        mutating calls the active access mode does not
                        auto-approve, offering Allow once / Always allow this
                        tool / Deny. policy.js holds the decision table and
                        the `~/.orbit-pi/access-allow.json` allowlist;
                        policy.test.mjs / index.test.mjs are `node --test`
contrib/orbit-title-extension/  Orbit's bundled pi extension (loaded with
                        `pi --extension`): after a session's first turn
                        settles, asks a model for a short title from the
                        first exchange and sets it with `pi.setSessionName`.
                        Reads `~/.orbit-pi/auto-title.json` (the model is the
                        active session model unless Settings → Agent picks
                        one); title.js is the pure helper set and both
                        files are `node --test` covered
assets/icons/           logo-icon.png app-icon source + icon.icns / icon.ico / icon.png
                        + alpha-logo.png / alpha-logo.icns (macOS debug builds)
PRODUCT.md  INTENT.md  README.md  AGENT.md
```

Workspace edition is **2021**. Prefer **Rust 1.94+**. Root `Cargo.lock` is the lockfile; ignore a nested `crates/orbit-pi/Cargo.lock` if present.

Override the pi binary with `PI_BIN` (default: `pi` on `PATH`).

## Localization (i18n)

UI copy is translated through [rust-i18n](https://github.com/longbridgeapp/rust-i18n).
`locales/en.yml` is the **source of truth** — every key and its English text.
The other nine locale files are generated, never hand-edited:

```
python3 scripts/gen_locales.py        # regenerate locales/*.yml + report gaps
cargo test -p orbit-pi i18n           # completeness guard
```

- **Ships:** English, 简体中文 (`zh-CN`), 日本語 (`ja`), 한국어 (`ko`),
  Español (`es`), Français (`fr`), Deutsch (`de`), Português do Brasil
  (`pt-BR`), Русский (`ru`), Italiano (`it`) — plus `System`, which resolves
  through the OS preferred language (`AppLanguage::from_locale_id`).
- **Call sites:** wrap literals with the crate-root macros — `tr!("key")` for
  plain text, `tr!("key", count = n)` for `%{count}` interpolation, and
  `tr_cow!("key")` only on hot render paths that borrow. Never hard-code
  user-facing English in a render path; add a key instead.
- **Translations** live in `scripts/i18n_glossary*.py`, keyed by the exact
  English string from `en.yml`. Split by surface (core, settings, palette,
  transcript). A string with no entry falls back to English.
- **Keys** are `surface.slug` (`settings.general`, `transcript.thinking`) so a
  translator sees context; identical English can appear under several keys.
- **Native menu:** `set_app_menus` rebuilds the macOS menu bar when the
  language changes (`theme::set_ui_prefs` calls it).
- **Placeholders:** a `ComposerInput` placeholder must be set with
  `.with_placeholder_key("surface.slug")` (plus `.with_placeholder_var` for
  `%{…}` values), not `.with_placeholder(tr!("…"))`. The key resolves at paint
  time, so long-lived search fields (settings/provider/model filters, the
  side pane, git panel, usage page) follow a language change without being
  rebuilt. `.with_placeholder` is only for literal, untranslated text such as
  `sk-…` or a URL example.

### Adding a string

1. Wrap the literal with `tr!("surface.slug")` at the call site.
2. Add `surface.slug: "English"` to `locales/en.yml` (append; keep key order).
3. Add `"English": "translation"` under each locale in the matching
   `scripts/i18n_glossary_*.py`.
4. Run `python3 scripts/gen_locales.py` and `cargo test -p orbit-pi i18n`.

The codemods in `scripts/` (`localize_calls.py`, `localize_settings.py`,
`localize_call_args.py`) wrap common GPUI builder patterns in bulk; run one,
merge its key TSV into `en.yml`, then translate. They skip test code and
`*_tests.rs` on purpose.

## Current state

The product is the GPUI app (`cargo run -p orbit-pi`). The v0.1 React/Vite/Tauri stack is gone from git history as of the native rewrite; do not resurrect it.

| Area | Location | What is real today |
|---|---|---|
| Shell | `app.rs` | Platform-conditional window chrome. macOS keeps the transparent titlebar with AppKit's traffic lights inside the sidebar's 44px row (`platform::titlebar_options`); Windows is transparent too, because the app draws its own minimize / maximize / close buttons at the right of the header row (Waku-style, `helpers::window_controls`, 46px each, red close hover) on every surface — settings, git, usage, onboarding and the session view — with `platform::WINDOW_CONTROLS_W` reserved by the top bar, the Review pane's header, and the Git header. Nothing goes through GPUI's `WindowControlArea` on Windows, and that is deliberate: the app's root tracks focus, so GPUI's focus-transfer listener calls `Window::prevent_default` on every press, and a prevented (or stopped) press is treated as handled — which makes GPUI drop its window-control path entirely. So `platform::start_window_drag` starts the OS move loop itself (`ReleaseCapture` + `WM_NCLBUTTONDOWN`/`HTCAPTION`) and `platform::window_command` posts `WM_SYSCOMMAND` (`SC_MINIMIZE`/`SC_MAXIMIZE`/`SC_RESTORE`/`SC_CLOSE`) from the user32 FFI in `platform.rs`; the trade is that Win11's snap-layouts flyout needs a real system caption button and is therefore absent. `platform::WINDOW_CONTROLS_CLEARANCE` is the matching leading inset for the app's own left controls (80px past the lights on macOS, 12px without them). Sessions sidebar grouped by workspace (collapsible; the open, expanded workspace's header pins to the top while its own sessions scroll — the next group's header pushes it away, mirroring the Review pane's sticky file header), top-bar back/forward + title + edit +/− counts, floating composer, status bar (workspace / git branch from `.git/HEAD` with ↑ahead ↓behind versus its upstream, plus a fork chip counting the repo's other local branches; both open the branch picker). The top-bar right cluster also carries a **provider-quota pill** (`session.rs`: `render_quota_pill`/`render_quota_popup`): the single most-constraining window across every connected account as a small meter + percent (or a quiet **Usage** label for balance/credits-only providers), opening a provider-independent popover of every normalized `QuotaReport`. Hidden entirely when pi lacks `quota.*`. No provider-specific UI — it reads `QuotaManager::peak_window`/`reports` only |
| Menu bar | `main.rs` | Native macOS menu bar (`app_menus`, installed with `cx.set_menus`): **Orbit** (About, Check for Updates, Settings, Services, Quit), **File** (New Task, Refresh Sessions), **Edit** (Cut/Copy/Paste/Select All, live only while a composer-style field has focus), **View** (Command Palette, Find in Transcript, Usage). AppKit labels the application menu from the process/bundle name and ignores menu-item titles, so `platform::set_process_name("Orbit")` renames the process before the menu is built (a bare `cargo run` would otherwise read `orbit-pi`). Every item is a real GPUI action dispatched through the same handlers as its keybinding, so it is enabled exactly where the command works. GPUI exposes no selector for the standard Hide/Hide Others/Show All or Window items, so those are intentionally absent. |
| Sessions | `sessions.rs` | Reads `~/.pi/agent/sessions/<slug>/*.jsonl`; title = pi's persisted auto-title (`session_info.name`, read from a bounded tail window) with the first user message as fallback and preview; header-only files (fresh `new_session`, nothing sent yet) are drafts and **not listed** until the first user message lands (Waku drafts parity); `cmd-n` new, click to `switch_session` + `get_messages`. **Clone:** the sidebar row menu duplicates a session *on disk* (`clone_session_file` — fresh UUIDv7 id, pi's `<timestamp>_<id>.jsonl` naming, verbatim entries, `parentSession` provenance), so it works on any row without switching the live process. **Warm pool:** switching away parks the pi process (running *or* idle) in `lives`, so re-opening is a resume with no Node spawn; the cap is `MAX_LIVE_SESSIONS` (least-recently-parked idle evicted first) and idle entries reap after `PARKED_IDLE_TTL`. Capability/auth/quota probes are queued **after** the session command, never in front of the load. |
| Transcript | `transcript.rs` + `transcript_view.rs` + `message_scroller.rs` | Virtualized `list()` (content column max 960px) with MessageScroller tail-follow: append/remeasure without blank rows, **Jump to latest** when you scroll up (a **New activity** capsule when content landed while away). Rows use Message Start/End slots (avatar + content + Copy footer). Waku turn order: **Worked for** fold → answer → files-changed card → **Copy** (+ completion time when pi provides one); live turns keep a **Working** activity cluster and close with an activity-aware indicator (**Running cargo test · 12s**, **Reading src/auth.rs**, **Thinking…**). Left navigation ticks jump to user turns and show a hover preview card (turn number, prompt + response snippet); a one-time dismissable hint (`~/.orbit-pi/hints.json`) teaches the rail on first appearance; `cmd-shift-c` copies the latest response and `cmd-up`/`cmd-down` mirror the rail's jumps. Message footers (copy + timestamp) are persistent, not hover-gated. Markdown blocks parse once per content change (thread-local memo), so streaming never re-parses settled rows. Body prose uses a 14/26 line measure so wrapped lines and inline-code washes do not collide. Code blocks carry a language chip from the fence info string; settled blocks past 28 lines fold to a 24-line preview (**Show remaining N lines**), and copy always yields the full code. Tool rows lead with a category-tinted HugeIcons glyph (`assets/icons/tools/`: edit/write, read, search, find, list, bash, task, web, fetch, skill, ask, plan, code, mcp, plus a wrench fallback) and a short verb label, then expand into detail cards with Arguments/Output sections (Output streams live from accumulated `tool_execution_update` partials and is normalized out of pi's `{content:[…]}` envelope at `tool_execution_end`) and per-section copy buttons; sections past 16 lines collapse to a 12-line preview, expanded paint caps at 400 lines (copy stays complete); failed tools show a drawn stop glyph and their first error line inline; a turn the provider/agent rejected renders pi's `errorMessage` as an inline **Agent error** card; a cancelled turn keeps its partial answer under a quiet **Stopped** marker. Wide tables scroll horizontally instead of crushing columns. A status strip above the composer carries **Retrying — attempt N of M** (with Cancel → `abort_retry`) and **Preparing conversation context…** while compacting; the error banner has Copy and, once the process exits, Reconnect. Counts from edit/write args, not git; no fork |
| Composer | `composer.rs` + `mentions.rs` | **Multi-line** input (wraps, grows to 8 rows, then scrolls). Enter submits — while the agent is running as a **follow-up** (`follow_up`), queued and delivered once the current task finishes; as a `prompt` otherwise; `shift-enter` newline, `cmd-enter` submits. A sticky queue bar above the composer mirrors pi's `queue_update` (each pending message labelled, with a **Clear** action) until it is delivered; `escape` sends `clear_queue` (restoring the dropped text to the composer) before `abort`. `/`-command + `@`-file autocomplete, image attachments (paste / drop / "+" menu → Attach image) that ride `prompt`, `follow_up`, and `steer` alike, non-image files referenced by path at the caret, "+" add menu (keyboard-driven, `AddMenu` context). **Steer** (`cmd-shift-enter` / `alt-enter`, or the round control beside Stop while running): injects the composer text into the live turn via `steer`. Failed sends keep the prompt and attachments |
| Side pane | `sidepane.rs` + `review.rs` + `checkpoint.rs` + `git.rs` + `highlight.rs` | Right panel (top-bar `panel-right` toggle, the top-bar `+N -M` diff-stat chip opens it on **Uncommitted**, drag-resizable left edge) showing **Review**: a source dropdown grouped like Waku — `Last Turn` (per-turn git snapshot refs under `refs/orbit/…`, captured at prompt start and `agent_settled` by `checkpoint.rs`; branch-switch-safe via a merge-tree diff base), then `Uncommitted` (`git diff <head>` + untracked via an isolated worktree commit), `Unstaged` (`<index tree>` → worktree), `Staged` (`<head>` → index tree), `Committed`/`Branch` (merge-base against `main`/`master`). `git.rs` captures `--numstat` + a full-context patch (falls back to `-U3` past the 32 MB cap) off-thread; `review.rs` parses it into a virtualized row list (file headers, hunk separators, tinted ±rows with a single new/old line-number gutter, deleted/binary handling, expandable context gaps) with a sticky file header and devicon marks; `highlight.rs` colors code-line tokens. A filterable changed-files tree (Waku indentation `7+depth*14` dirs / `23+depth*14` files, `A/M/D/B` badges, collapsible dirs, click-to-jump, keyboard cursor) sits alongside and auto-hides on narrow panes (tree ≥440 px, stats ≥380 px). Reloads on open, workspace/session change, source change, and turn settle. The transcript's changed-files cards' **Review** buttons open the pane on the latest **Last Turn** checkpoint diff via a `ReviewOpener` callback threaded from the app |
| Explorer | `explorer/` + `watch.rs` + `git.rs` | Left project-panel dock (`cmd-shift-e`, top-bar folder toggle, ⌘P row) showing the active workspace as a virtualized tree. `walk.rs` snapshots the tree on the background executor through ripgrep's `ignore` walker — nested `.gitignore`/`.ignore`, global and repo excludes, symlinks never followed, an entry cap surfaced as "truncated", and a hidden-files toggle. `tree.rs` is pure: dirs-first ordering, expand/collapse set, case-insensitive filter (ancestors and matching subtrees revealed), and `M/A/D/R/U` git badges from `git::status_rows` (changed ancestors get a dot). `panel.rs` renders fixed-height rows with devicon glyphs, keyboard nav (↑↓←→, home/end, `cmd-←`/`cmd-→` collapse/expand all, enter/space open), a filter field, and a right-click menu (Open, Open in Default App, Reveal in File Manager, Copy Path / Relative Path). Clicking a file opens the **Files** surface in the main area — a full-page editor (`viewer.rs`): a tab strip (unsaved dot per tab), a toolbar with path/size/line count and an autosave status chip (or a Read-only mark for Markdown/images/binary), syntax-highlighted code in a gutted `ComposerInput` (no wrap, line numbers, undo/redo), editable Markdown with an Edit/Preview toggle (`render_markdown_document`), images, and honest Binary / TooLarge / error states (2 MiB and 200k-line caps; directory and non-UTF-8 handled). Code/text edits autosave after a 500 ms debounce (`cmd-s` flushes now; closing a tab or the surface flushes too) through a temp-file+rename write; truncated and non-UTF-8 text stays read-only so a save cannot corrupt it. File reads, writes, and the workspace snapshot run off-thread; `watch::WorkspaceWatcher` re-reads the active file and the completion tells our own save apart from an external change (flagged as "Changed on disk"). `cmd-w` closes the active tab (or the X / `cmd-shift-w` the whole surface). **File operations** (`ops.rs`, pure + unit-tested): New File / New Folder from the header or a directory's menu, Rename edited in place, and Delete behind a confirmation; the panel sends a workspace-relative `FileOpRequest` to the app, which runs create/rename (`create_new`/`create_dir`/`rename`, never clobbering) or `platform::trash_path` (`NSFileManager` Trash on macOS, permanent elsewhere, worded from `TRASH_IS_RECOVERABLE`) on the background executor, then reconciles the tree and open Files tabs (`reconcile_rename`/`close_under`). Rename/delete are refused while a tab under the target is dirty. The inline prompt rides a `Composer ExplorerEntry` field with `ExplorerEntryConfirm`/`ExplorerEntryCancel` bound after the composer keys. |
| Git page | `git_panel/` + `git_ops.rs` + `gh.rs` + `git.rs` + `commit_message.rs` | Full main-area surface (session-details **Commit or push**, top-bar chip; `escape`/Back returns) with five tabs and ⌘1–⌘5 shortcuts. **Changes**: staged/unstaged lists with per-file stage/unstage and confirmed discard, Include-unstaged, a commit bar (branch menu, message input, **Generate**), and a collapsible **Stashes** section (stash all/pop/apply/drop). **History**: `git log` rows with ref badges; clicking expands `commit_detail` (full body, author/committer, changed files) and a per-file actions menu (Open file, Open diff, Reveal in File Manager, Copy path, File history `--follow`); filter bar with All branches, author/path chips, Clear. **Graph**: lane graph over `--branches HEAD` (plus `--remotes --tags` under **All refs**) with the same expandable detail. **Issues**/`gh issue`: Open/Closed/All + search + label picker, New issue, detail with rendered Markdown body/comments, comment composer, Close/Reopen, Edit labels. **Pull requests**/`gh pr`: Open/Closed/Merged/All + search, New PR (title/body/base/draft), CI rollup and review-decision chips, detail with checks/commits/files/comments/reviews, Approve/Request changes, Merge (method + delete-branch confirm), Close/Reopen, Checkout branch (refused on a dirty worktree). Branch menu: Merge branch…/Rebase onto… (ff/merge-commit/squash), New/Rename, safe-then-forced delete. `operation_state` paints one in-progress bar (merge/rebase/cherry-pick/revert/bisect) with Continue/Skip/Abort and conflicted-file chips. Failures classify into `ActionError` kinds with valid `RecoveryAction` buttons (pull/merge/rebase/force-with-lease/publish/re-auth/retry); force push is always `--force-with-lease` and confirmed. `gh` is required for host tabs: install/sign-in setup states otherwise; `GH_BIN` overrides the binary; min `gh` 2.20.0. Design: `crates/orbit-pi/docs/git-page.md`, plan: `docs/git-page-plan.md` |
| Catalog | `model_selector.rs` | Searchable popovers for `get_available_models` / `get_available_thinking_levels`; `set_model` / `set_thinking_level`. Two chips plus an interactive **access-mode** chip (`access.rs` + `bundled_extensions.rs` guard) that selects Supervised / Auto-accept edits / Full access |
| pi self-update | `pi_update.rs` + `app/pi_update_ui.rs` | A few seconds after launch, off-thread, Orbit asks pi's own release endpoint (the one pi's startup notice reads) for the latest version. A newer release posts an info toast and installs through pi's own updater (`pi update self --no-approve`) — install-method detection and registry choice stay pi's truth — then a success toast names the new version. Live sessions keep the old pi; sessions started afterwards pick it up. Offline/endpoint failure is silent, `PI_BIN` custom builds are never touched, and a failed install posts pi's `Error:` line. |
| Access guard | `access.rs` + `bundled_extensions.rs` + `dialog.rs` + `contrib/orbit-guard-extension/` | The composer's access chip picks a mode, persisted to `~/.orbit-pi/access.json`. The bundled guard extension hooks pi's `tool_call`, classifies the tool (read / edit / exec / other), and for the mutating calls the mode does not auto-approve calls `ctx.ui.select` with the options **Allow once / Always allow this tool / Deny**. The guard marks the `select` title (`dialog::GUARD_TITLE_PREFIX`), so Orbit routes it to the **inline approval bar** above the composer (not the scrim modal) — transcript stays visible, answer sits by the composer; ↑/↓/⏎/esc drive it. "Always allow" is recorded per mode in `~/.orbit-pi/access-allow.json`. Supervised asks before edits/commands; Auto-accept edits allows edits and asks before commands; Full access allows all. An unknown/missing mode degrades to Full access. The extension reads mode + allowlist fresh per tool call, so a change re-arms live sessions with no restart |
| Settings | `app.rs` + `providers.rs` + `auth.rs` + `quota.rs` + `skills.rs` + `plugins.rs` | In-app surface (`cmd-,`): General / Runtime / Agent / Skills / Plugins / Models / Appearance / Providers / About. Read-only facts from the live process + a real sidebar toggle; About shows the app icon, a GitHub link, and the update check — a signed release turns the check into an icon-only download button and the sidebar pill into a 20px download glyph that expands to **Update** on hover; both open the **update modal**: the search progress bar, the release's changelog (the appcast's `<description>`, written from `CHANGELOG.md` by `scripts/appcast.py`), **Version History** (every feed release newest first, the running build marked), and **Update now** / **Later**; Update now stages and relaunches Orbit after it quits. **General** also owns the three notification channels — a real desktop banner, the system alert sound, and in-app toasts (the frontmost-window stand-in for the banner; `src/toast.rs` model + `app/toast_ui.rs` layer). Banner and sound are held while the window is frontmost; a clicked banner reopens (and un-minimizes) its session. `notifications.rs` posts through `UNUserNotificationCenter` when bundled and falls back to `osascript` (Script Editor attribution, stated in the UI) for `cargo run`; `scripts/run-bundled.sh` runs the debug build from a real `.app` so banners carry Orbit's icon, permission, and click routing. **Skills** (`skills.rs` + `app/skills_ui.rs`) is a full-bleed master-detail surface: a searchable, scope-grouped list column over a detail column with the skill's facts (Invoke `/skill:<name>`, Location, Contents, Updated), a real enable/disable toggle, and the rendered `SKILL.md` (the transcript's block renderer). It scans the four roots pi loads (`<workspace>/.pi/skills`, ancestor `.agents/skills`, `~/.pi/agent/skills`, `~/.agents/skills`), parses each `SKILL.md` frontmatter (name/description/`disable-model-invocation`), and dedupes by name with project precedence. The toggle writes/removes pi's own `-<path>` override in the scope's `settings.json` `skills` array (exact paths; glob `!` excludes are left to pi); **Open SKILL.md** / **Show in File Manager** / **Copy Path** act on the file, and **Delete** (confirmed) removes the skill directory and its override. **Plugins** reads the `packages` arrays from `~/.pi/agent/settings.json` + `<workspace>/.pi/settings.json`, resolves each npm/git/local source to its managed install dir (version from `package.json`), and installs / removes / updates through `pi install|remove|update` (project scope passes `-l -a`) on the background executor. **Agent** controls pi's live behavior over RPC: follow-up delivery mode (`set_follow_up_mode`), auto-compaction and auto-retry toggles, and an **Abort retry** action while `auto_retry_*` is in flight (`abort_retry`). Manual compaction (**Compact now**, `compact`) lives in the context-usage popover, and the session rename (`set_session_name`) in the session-details popover — each next to the state it acts on; Enter in the rename field commits it. **Session titles** toggles the bundled auto-title extension and picks the model it asks (first option: the active session model); both are written to `~/.orbit-pi/auto-title.json`, which the extension reads when a session's first turn settles. **Providers** lists every built-in pi provider individually (metadata introspected from pi-ai, curated table as fallback) plus models.json endpoints, with real status, API-key entry (`auth.json`, 0600), models.json overrides for custom providers, Sign out, Remove, search, and Refresh. Provider OAuth is a **first-class RPC capability**: `auth.list` discovers providers/methods (no hardcoded ids), `auth.login`/`auth.logout`/`auth.cancel` drive a login session id, and asynchronous `auth.*` events render Connect / Connecting / Device code / Success / Error / Disconnect / Cancel. Browser URLs open via the OS; tokens never cross RPC or touch GPUI state. A pi without `auth.*` falls back to the legacy `pi /login` Terminal path and the credentials-changed restart banner; run `node contrib/pi-auth-rpc/apply.mjs` once to add the server side to an installed pi and get browser OAuth. Connected cards also render **quota meters** (subscription windows, balances, trailing spend) from the `quota.*` RPC; add the server side with `node contrib/pi-quota-rpc/apply.mjs`. Quota is normalized and non-secret: pi queries the provider, Orbit only sees percentages/counts/resets/amounts |
| RPC | `orbit-rpc` | Spawn (`--mode rpc --approve`, `PI_SKIP_VERSION_CHECK=1` — Waku parity), JSONL I/O, response correlation, typed-enough events incl. `steer`/`follow_up` + the queue modes (`set_steering_mode`/`set_follow_up_mode`), `compact`/`set_auto_compaction`/`set_auto_retry`/`abort_retry`, `set_session_name`, fork family (`get_fork_messages`/`fork`/`clone`), `session_info_changed` (live auto-title), `queue_update` (`PendingQueue`), `auto_retry_*`, and the typed `auth.*` namespace (`Event::Auth` + `AuthProvider`/`AuthErrorCode`; contract in `docs/auth-rpc.md`), plus the `quota.list` command and `QuotaReport`/`QuotaWindow`/`QuotaBalance` types (`docs/quota-rpc.md`). `SessionState` parses the `get_state` agent-control fields. UI answers `extension_ui_request` dialogs (`select`/`confirm`/`input`/`editor`) natively via `dialog.rs`; `clone` is wired from ⌘P ("Clone Session") and the sidebar row menu, rewind (`fork`) still has no UI |
| Find & media | `app/search.rs` + `transcript_view.rs` | ⌘F in-transcript find: floating bar over the transcript, message-level matching (case-insensitive), a live "n of m" count, prev/next jump (Enter/Shift-Enter), matched rows washed and the selected hit stronger; closes on Escape. Clicking an attachment image opens a full-window lightbox (`app.rs` `lightbox` + `view.rs` `lightbox_layer`). |
| Workbench | `app/settings.rs` + `usage/` + `skills_ui.rs` | Settings sections General / Runtime / Agent / Skills / Plugins / **Models** / Appearance / Providers / About. **Models** shows the `get_available_models` catalog as a provider-grouped three-column card grid (name, id, context window, active mark) with a search field, a favorites-only filter, and a star shared with the composer picker (`favorites.rs`). Usage / Skills / Plugins / Providers are full pages (see the Settings row). Clone session is offered from ⌘P and each session row's `…` menu. |
| Stubs | — | No rewind-to-turn UI (clone exists; `fork` needs entry ids). The access guard confirms *tool calls*; it is not a sandbox (pi has none) and does not yet cover project trust or an "Auto" reviewer mode |

**Accessibility:** reduce-motion is a real Appearance setting (`theme.ui.reduce_motion`) that renders the running-session shimmer, refresh spinners, and drop-overlay fade statically; it is persisted to `~/.orbit-pi/ui.json`. Full keyboard/screen-reader work (focus rings, labeling) is still backlog.

**Shipped infra:** GitHub Actions CI (`.github/workflows/ci.yml`, macOS build + clippy); `make-dmg.sh` signs, optionally notarizes and staples each `.app`/DMG when `NOTARY_PROFILE` is set; `transcript.rs` carries a 10k-message model-layer perf test.

## How the app runs

1. `OrbitApp::new` spawns `PiClient::spawn(cwd, None)` so sessions land in pi's default store (`~/.pi/agent/sessions/`), shared with the CLI. Tests pass `Some("/tmp/…")` via `--session-dir`.
2. On connect it sends `get_state`, `get_available_models`, `get_available_thinking_levels`.
3. `main.rs` starts a window `spawn` loop: every **90 ms** call `OrbitApp::tick` → `PiClient::drain_events` (`try_recv`, never blocking).
4. Transcript data has two inlets into the same model: `get_messages` rebuilds; `message_start` / `message_update` / `message_end` mutate live.
5. One `PiClient` = one child. Drop kills the process. `ProcessExited` is surfaced in the status line.
6. A few seconds after launch the pi self-update check runs off-thread (`pi_update.rs`): a newer pi installs through `pi update self` and reports via toasts; up-to-date or offline stays quiet.

## GPUI 0.2.2 API notes (learned on this repo — save re-deriving them)

> **Reference: Zed.** Use Zed source code as a reference when a task concerns GPUI implementation — layout and styling idioms, focus and key dispatch, virtualized lists, menus and popovers, window and platform behavior — or when an in-house `src/ui` primitive needs a proven native precedent. Zed is the canonical GPUI codebase; read its crates, and read the gpui revision pinned in `Cargo.toml` so the APIs match what Waku builds against.

These compiled and ran against the pinned version. When in doubt, check
`~/.cargo/registry/src/.../gpui-0.2.2/` (source is the docs) and `examples/` inside it.

- **Bootstrap** (`main.rs`):
  ```rust
  Application::new().with_assets(assets::Assets).run(|cx: &mut App| {
      cx.text_system().add_fonts(vec![/* Cow<[u8]> font bytes */]).unwrap();
      let bounds = Bounds::centered(None, size(px(w), px(h)), cx);
      cx.open_window(WindowOptions {
          window_bounds: Some(WindowBounds::Windowed(bounds)),
          titlebar: Some(TitlebarOptions {
              title: Some(SharedString::from("Orbit Pi")),
              appears_transparent: true,
              traffic_light_position: Some(point(px(12.), px(13.))),
              ..Default::default()
          }),
          focus: true, ..Default::default()
      }, |window, cx| { /* Entity<OrbitApp> */ }).unwrap();
      cx.activate(true);
  });
  ```
  Size the window from `cx.primary_display()` (a hardcoded 1240×840 gets clamped top-left on small/scaled screens). `open_window`'s callback is `FnOnce(&mut Window, &mut App) -> Entity<V>`; create entities with `cx.new(|cx| …)`.
- **App icon:** source is `assets/icons/logo-icon.png`. `icon.icns` is for bundled `.app`s (`package.metadata.bundle`); `scripts/make-dmg.sh` ships it under `dev.orbit.pi`. `cargo run` has no bundle, so `app_icon::set_dock_icon()` calls AppKit `setApplicationIconImage` with the embedded 512 PNG, and Settings → About paints the same PNG via `img(app_icon::ASSET)`. **macOS debug builds swap in the Alpha mark** (`assets/icons/alpha-logo.png` → derived `alpha-logo.icns` + `crates/orbit-pi/dev-assets/alpha-app-icon.png`, regenerated by `scripts/make-alpha-icon.sh`; a debug-only asset overlay, so release binaries do not embed it): `debug_assertions` selects `alpha-app-icon.png` for the Dock and About, and `scripts/run-bundled.sh` builds the separate `Orbit Pi Alpha` / `dev.orbit.pi.alpha` bundle from the Alpha icns. Release builds, DMG, and CI still take the production branches (Windows/Linux debug builds keep the production mark).
- **macOS chrome:** `appears_transparent` + `traffic_light_position` — no objc2. Drag regions: `.window_control_area(WindowControlArea::Drag)` on the sidebar strip and the top-bar spacer. When the sidebar is hidden, pad the top bar so controls clear the traffic lights (`pl` ≈ 76 px).
- **Virtualized lists** (transcript **and** sidebar):
  - Transcript uses `message_scroller.rs` (`ListAlignment::Bottom`, 400px overdraw) — 0.2.2 has no `FollowMode::Tail`, so a past-the-end `scroll_to` is the equivalent. Append while following sticks to the live edge; `remeasure_items` grows the streaming row; scroll away shows **Jump to latest**.
  - Rows are plain GPUI flex trees in `transcript_view.rs` (the `Message` component port was removed — no component library anywhere): Start/End alignment, avatar disc, content column, footer. The footer sits outside the avatar row so the 32px disc stays flush with content; the footer is indented `avatar + row gap` to line up with the content column.
  - Sidebar: `ListState::new(count, ListAlignment::Top, overdraw_px)`.
  - `list(state.clone(), move |ix, window, cx| … .into_any_element())` — the closure is `'static`, so capture `Rc<RefCell<_>>` / `Rc<Vec<_>>`, never `&self`.
  - Stick-to-latest: `scroll_to(ListOffset { item_ix: len, offset_in_item: px(0.) })` (past-the-end).
  - Pin tracking: `set_scroll_handler` — for `Bottom` alignment, `is_scrolled` is "not at bottom", and it only fires for real user scrolls, not programmatic `scroll_to`.
- **`list()` needs a definite height:** default `ListSizingBehavior` is **`Auto`** (no intrinsic height). As a plain block child it lays out at height 0 → `prepaint_items` clears its item list → **nothing paints** (while request_layout's overdraw probe still renders ~3 items — the telltale symptom). Fix: `.h_full()` on the list inside a `flex_1` + `min_h_0` parent. Never wrap a `list()` in an outer `overflow_y_scroll()` div — the list scrolls itself.
- **Heartbeat / async:** `window.spawn(cx, async move |cx: &mut AsyncWindowContext| { loop { Timer::after(dur).await; view.update(cx, |this, cx| this.tick(cx)).ok(); } }).detach()`. `AsyncWindowContext` derefs to `AsyncApp` and implements `AppContext`. `pub use smol::Timer` is at the gpui crate root.
- **Rendering:** `impl Render { fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement }`.
- **Events:** handlers are `cx.listener(Self::method)`; the listener bound is `Fn(&Event, &mut Window, &mut App)` (not `Context<T>`). `.hover(|s| …)` for hover styles.
- **Text wrapping:** `.whitespace_normal()` exists in 0.2.2 and is what the assistant transcript uses. There is still **no `WhiteSpace::PreWrap`**. For preformatted blocks (thinking, code), render **one `div()` per line** with a fixed height (e.g. `div().h(px(17.)).child(line)`).
- **Any-elements:** `.into_any()` is on `Element` (concrete types); on an opaque `impl IntoElement` return use `.into_any_element()`.
- **Custom text input:** there is no multi-line editor widget we use. `ComposerInput` is an `EntityInputHandler` adapted from gpui's `input` example (grapheme-aware caret, clipboard, IME stubbed). Keep it single-line until a deliberate composer upgrade (INTENT.md risk #3).
- **Key-binding precedence:** bindings are ordered by dispatch-tree depth (deepest/focused node first); ties at the same depth break by **registration order (later wins)**. Bindings with no context count as the deepest depth. The picker filter uses `.with_key_context("Composer Picker")` so backspace still edits, and picker `enter`/`escape`/`arrows` are registered **after** composer bindings in `main.rs` so they win over `Submit` / `AbortRun`.
- **Popovers:** `anchored().position_mode(AnchoredPositionMode::Local).anchor(Corner::BottomLeft).offset(…).snap_to_window()` + `deferred(entity)` paints a floating popup. `on_mouse_down_out` dismisses. **Click-through:** the same click's mouse-up would re-open the chip — `OrbitApp` swallows toggles for ~200 ms after an outside mouse-down dismiss (`menu_dismissed_at`). Layered `BoxShadow`es (`vec![contact, ambient]`) read as a real modal surface.
- **Picker lists:** **do not use `uniform_list` inside a `deferred` + `anchored` popover** — measured layout can collapse to height 0. Use `max_h` + `overflow_y_scroll` + `ScrollHandle` (Zed `ContextMenu` shape). Keyboard nav must `set_offset` to keep the highlighted row in view. `.on_hover` / `.on_click` live on *stateful* elements (`.id(...)` required).
- **Assets:** implement `gpui::AssetSource` over `include_dir!` so packaged builds don't depend on cwd. SVG icons: `img("icons/….svg")` via helpers in `app.rs`. Provider marks live under `assets/icons/providers/`.
- **Fonts:** `cx.text_system().add_fonts` for embedded `Symbols Nerd Font` (devicons) and the curated catalog under `assets/fonts/bundled/`. `assets::register_zed_fonts` loads IBM Plex Sans / Lilex; `assets::register_bundled_fonts` recursively loads the bundled faces so `theme::UI_FONTS` / `CODE_FONTS` resolve without an OS dependency. Each family is a statically instanced, subset 400/500/600/700 TTF with a normalized name table (one typographic family, unique PostScript names) so font-kit weight selection works.

## Keybindings (current)

| Keys | Action | Context |
|---|---|---|
| `cmd-q` | Quit | global |
| `cmd-n` | New session | global |
| `cmd-r` | Reload session list from disk | global |
| `cmd-,` | Settings | global |
| `cmd-p` | Command palette (sessions / commands / settings) | global |
| `cmd-shift-e` | Toggle project panel (Explorer) | global |
| `cmd-w` | Close the active file tab (last tab closes the Files surface) | `Files` |
| `cmd-shift-w` | Close the Files surface | `Files` |
| `cmd-s` | Save the active file now (autosave is on anyway) | `Editor` |
| `cmd-z` / `cmd-shift-z` | Undo / redo | `Composer` / `Editor` |
| `cmd-shift-c` | Copy the newest assistant response (footer copy mirror) | global |
| `cmd-up` / `cmd-down` | Jump to previous / next user turn (rail mirror) | global |
| `escape` / `cmd-.` | Close settings → close picker → `abort` | global / Composer |
| `enter` / `cmd-enter` | Submit prompt | `Composer` |
| `shift-enter` | Newline | `Composer` |
| `enter` / `escape` / `up` / `down` | Confirm / cancel / move | `Picker` (registered after Composer) |
| `enter` / `escape` / `up` / `down` | Run / close / move | `AddMenu` (composer "+" menu; registered after Picker) |
| `enter` / `escape` / `up` / `down` | Select / close / move | `AccessMenu` (composer access-mode picker) |
| `enter` / `escape` / `up` / `down` | Confirm / deny / move | `Approval` (inline access-guard bar) |

## Pi CLI RPC protocol (official spec)

- Start: `pi --mode rpc [--provider …] [--model …] [--name …]` — see `packages/coding-agent/docs/rpc.md` in the pi repo.
- **Framing is strict JSONL, LF only.** Split on `\n`; strip a trailing `\r`; never use a reader that splits on `U+2028/U+2029`. `PiClient` uses `read_until(b'\n')`.
- **Typed in `CommandBody` today:** `prompt` (with `images` + `streamingBehavior`), `abort`, `clear_queue`, `steer`/`follow_up` (with `images`), `set_steering_mode`/`set_follow_up_mode`, `compact`, `set_auto_compaction`/`set_auto_retry`/`abort_retry`, `set_session_name`, `new_session`, `switch_session`, `get_state`, `get_messages`, `get_available_models`, `set_model`, `cycle_model`, `get_available_thinking_levels`, `set_thinking_level`, `cycle_thinking_level`, `get_commands`, `get_session_stats`, `get_fork_messages`, `fork`, `clone`, `auth.list`, `auth.status`, `auth.login`, `auth.logout`, `auth.cancel`, plus `Raw(Value)` for anything else.
- **Used by the UI today:** `prompt`/`follow_up` (text + images), `abort`, `clear_queue`, `set_follow_up_mode`, `compact`, `set_auto_compaction`/`set_auto_retry`/`abort_retry`, `set_session_name`, `new_session`, `switch_session`, `get_state`, `get_messages`, `get_entries` (bridge quota snapshots, incremental `since` cursor), `get_available_models`, `set_model`, `get_available_thinking_levels`, `set_thinking_level`, `get_session_stats`, and the `auth.*` family when the running pi advertises it (falls back to file/Terminal login otherwise). `SessionState` reads `steeringMode`/`followUpMode`/`autoCompactionEnabled`/`sessionName`/`isCompacting`/`pendingMessageCount` from `get_state`; `PendingQueue` mirrors `queue_update`. `steer` is sent by the composer's steer controls; `clone` by ⌘P. `set_steering_mode`, `cycle_model`, `cycle_thinking_level`, `get_fork_messages`, and `fork` remain typed but unwired.
- **Not typed / not wired yet:** `get_tree`, `bash`/`abort_bash`, `export_html`, `get_last_assistant_text`, and `new_session.parentSession`.
- **Error handling:** a `response` with `success: false` carries an `error` string. The app never swallows it: `on_command_failure` surfaces it in a dismissible error banner (chat and settings) and re-reads `get_state` to reconcile optimistic UI after `set_model`/thinking/rename failures. A `parse` command, `extension_error` events, failed compaction (`compaction_end.errorMessage`), exhausted `auto_retry_end`, and `send` write failures all route to the same banner. A successful retry of the same command clears its banner (`clear_error_for`). **Provider/LLM failures are not responses at all**: pi sets `stopReason: "error"` + `errorMessage` on the final assistant message and emits it via `message_start`/`message_end` (see pi-agent-core `agent-loop.js`). `transcript::message_error` extracts it; the app raises `Agent error: …` and `transcript_view` renders the same text as a red card inline, so a failed turn is never an empty row.
- Events → stdout JSON lines: `agent_start/end/settled`, `turn_start/end`, `message_start/update/end` (`assistantMessageEvent`: `text_delta`, `thinking_delta`, `toolcall_start/delta/end`), `tool_execution_*`, `bash_execution_update`, `queue_update`, `compaction_*`, `auto_retry_*`, `extension_ui_request`, `extension_error`, typed `auth.*` (`Event::Auth`), plus `Unknown` for forward-compat.
- **Provider authentication:** `auth.*` is a first-class RPC namespace. The client reducer is `orbit-pi/src/auth.rs`; the wire types are in `orbit-rpc/src/types.rs`; the server contract (which pi must implement by reusing its provider OAuth + `auth.json`) is `crates/orbit-rpc/docs/auth-rpc.md`. Tokens never cross RPC or GPUI state. **Stock pi 0.85.1 has no server side** — without it Orbit falls back to `pi /login` in Terminal. To get browser OAuth on an installed pi now, run `node contrib/pi-auth-rpc/apply.mjs` (idempotent; `--revert` restores the backup) and restart the agent. `auth_fake_pi.rs` covers the round trip against a scripted server.
- **Provider quota:** `quota.list` is the account-usage sibling of `auth.*`. The client reducer is `orbit-pi/src/quota.rs`; the wire types (`QuotaReport`/`QuotaWindow`/`QuotaBalance`/`QuotaKind`) are in `orbit-rpc/src/types.rs`; the server contract is `crates/orbit-rpc/docs/quota-rpc.md`. pi resolves the credential, verifies the provider origin, rejects redirects, and returns only normalized percentages/counts/resets/amounts — no token, key, or account id. Coverage follows `@narumitw/pi-usage`'s verified reference: Anthropic, ChatGPT Codex, GitHub Copilot, Kimi, MiniMax, Z.AI, OpenCode Go, OpenRouter, Vercel AI Gateway, DeepSeek, Moonshot, xAI, Fireworks, Baseten. Google/Gemini is registered explicitly as `unsupported` (no account-quota API — AI Studio-only). **Ollama Cloud** has a real adapter for both billing generations: the current monthly-credit model via `GET https://ollama.com/api/usage` with a user-supplied cloud API key, and the legacy 5-hour/weekly model via the authenticated `ollama.com/settings` page parsed behind an isolated `OllamaCloudParser` with a user-supplied session cookie. Credentials are explicit (`auth.json`, 0600; type `api_key` or `ollama_cloud_session`), entered in Settings → Providers → Ollama; the local `models.json` placeholder is never sent to ollama.com and browser cookies are never auto-extracted. Every other provider reports `unsupported`. Reports are per-provider cached 60 s (these endpoints rate-limit). **The default path needs no patch:** `bundled_extensions.rs` materializes the bundled `contrib/orbit-quota-extension/` under `~/.orbit-pi/quota-extension/` and every session process is spawned with `pi --extension <index.js>`; the extension fetches through pi's `ctx.modelRegistry`, appends one `orbit:quota` custom entry **only when the snapshot changes** (never in LLM context), and the app polls `get_entries` with a per-session `since` cursor (60 s, plus `agent_settled`, throttled by `QUOTA_ENTRY_POLL_INTERVAL`). Entry ids are per-session: the cursor resets on a `sessionId` change, a dead cursor is dropped and re-read, and bridge data never flips `QuotaSupport` (so a patched pi is still probed). `node contrib/pi-quota-rpc/apply.mjs` remains available for the `quota.list` server side; `quota_fake_pi.rs` covers the round trip and `--extension` forwarding, and `contrib/orbit-quota-extension/*.test.mjs` (run `node --test`) covers the bridge end to end offline, and `contrib/pi-quota-rpc/quota-handler.test.mjs` covers the patch adapters.
- **User interaction:** `extension_ui_request` (`select`/`confirm`/`input`/`editor`; also fire-and-forget `notify`/`setStatus`/`setWidget`/`setTitle`) on stdout; answer with `extension_ui_response` on stdin. `dialog.rs` + `app/dialogs.rs` render the dialog methods as a blocking modal card and reply (`cancelled` on Escape/scrim). `notify` renders as an in-app toast, `setStatus` surfaces on the status bar, and `set_editor_text` seeds the composer; `setWidget`/`setTitle` are terminal chrome and ignored. `ask_user_question`'s multi-select arrives as an `input` of comma-separated option numbers (pi's RPC fallback), so it is selectable too.
- Envelopes parse **loosely**; unknown shapes flow through as raw JSON so protocol additions don't crash the client.
- **Access guard (resolved without the protocol):** pi has no built-in per-tool permission surface in RPC mode and no sandbox (`pi docs/security.md`); its only tool gate is an extension's `tool_call` hook. Orbit ships one (`contrib/orbit-guard-extension/`) that prompts for mutating calls through `ctx.ui.select` — Allow once / Always allow this tool / Deny — which *does* surface over RPC as an `extension_ui_request` and renders as Orbit's inline approval bar. `--approve`/`--no-approve` remain **project trust** (load project-local settings/extensions/skills), a separate concern not yet wired to the access chip. There is no "Auto"/AI-reviewer mode: the installed pi exposes no `modelRegistry.streamSimple` for extensions, so shipping one would be fake.

## Plan (P0–P6) — see `INTENT.md` for decisions, rationale, estimates, and phase gates

### P0 — Transport probe + foundation (mostly done)
1. ✅ RPC framing + `get_state` against real pi; live prompt streaming (`agent_start → text → agent_settled`).
2. ✅ Root `Cargo.toml` workspace (`orbit-pi`, `orbit-rpc`).
3. ✅ Process spawner: `crates/orbit-rpc/src/client.rs` (per-session process, kill-on-drop, stderr ring).
4. ✅ Rust RPC client: permissive serde types + JSONL reader/writer threads.
5. ✅ First live round-trip in the real window (no more `live.rs` spike / mock tab).
6. ✅ Extension dialogs (`extension_ui_request`: `select`/`confirm`/`input`/`editor`) render natively and block the run until answered. ✅ Access guard: the bundled extension confirms tool calls per the active mode (no protocol permission surface needed).

### P1 — Shell & theme (mostly done; theme persistence still open)
- ✅ Window, top bar, macOS traffic lights (`TitlebarOptions.traffic_light_position`).
- ✅ Dark zinc palette (hardcoded in `app.rs`, Waku-like). ⬜ `Theme` entity, light mode, accent, density, persist to pi settings.
- ✅ Sidebar + sessions grouped by workspace; ⬜ Search.
- ✅ Keybindings / focus for composer + picker; ✅ native macOS menu bar (App/File/Edit/View); ⬜ broader native dialogs.
- ✅ Curated embedded font catalog (Inter, Fixel Text, Geist, Atkinson, Source Sans 3, Roboto, Noto Sans, DM Sans, Manrope, JetBrains Mono, Fira Code, Geist Mono, Commit Mono, Source Code Pro, Cascadia Code, Roboto Mono, Iosevka, DM Mono, IBM Plex Mono, Inconsolata, Noto Sans Mono, Space Mono, Anonymous Pro, Martian Mono).

### P2 — Data layer (substantially done)
- ✅ Permissive command/event models.
- ✅ Sessions: list, open, switch, new, **clone** (⌘P), delete, cross-workspace, over an Orbit-owned project list (remove a workspace from the sidebar without deleting pi's sessions). ⬜ rewind (`fork`) needs entry-id plumbing; rename lives in the session-details popover and stays active-session-only.
- ✅ Catalog: `get_available_models`, thinking levels, plus a **Models** settings page.
- ✅ Workbench readers: skills, plugins, usage history, on-disk model/provider CRUD.
- ✅ Settings persistence to `~/.orbit-pi/` (`theme`, `ui.json`, `fonts.json`, `favorites.json`, `notifications.json`). ⬜ scoped-models file I/O.

### P3 — Core chat (bulk done)
- ✅ Virtualized transcript, stick-to-latest. ⬜ stream veil (Waku `md/veil.rs`).
- ✅ Event drain on the ~90 ms heartbeat coalesces commits. ⬜ explicit ≤8.3 Hz streaming pipeline.
- ✅ Markdown: GFM tables/lists/alerts, one `StyledText` per block, highlight-as-paint, copyable code blocks.
- ✅ Multi-line composer, file attach → base64 images, `/`-command + `@`-file autocomplete, **steer**.
- ✅ Extension dialogs (`select`/`confirm`/`input`/`editor`) as a native modal. ⬜ richer per-tool renderers (bash/thinking dedicated; edit/todo/plan/search/mcp are generic). ✅ tool approval via the access guard's confirm dialog.
- ✅ Diff viewer + per-turn `Last Turn` checkpoints; ✅ image lightbox. Changed files render in the tail summary card **by design** (not per message). ✅ ⌘F transcript find.

### P4 — Workbench (substantially done)
- ✅ Git page (Changes / History / Graph + commit bar, commit-message generation). ✅ Providers: built-in catalog, API-key + OAuth, models.json CRUD, refresh. ✅ Skills, ✅ Plugins (+ search), ✅ **Models** page, ✅ Usage charts + filters + export. ✅ Theme/UI persistence (Orbit store). ✅ Explorer: left project panel (gitignore-aware tree, git badges, filter, hidden toggle) + full-page Files editor (editable, syntax-highlighted code/text with debounced autosave; markdown/image; binary/size guards) + file operations (New File/Folder, Rename, Delete-to-Trash).

### P5 — Mermaid + accessibility (partial)
- ✅ Mermaid fences stay copyable code blocks with an honest label (never faked). ✅ Reduce-motion setting honored by shimmer/spinners/drop fade. ✅ tool approval (`tool_call` guard + access modes; not a sandbox). ⬜ focus rings / screen-reader labeling.

### P6 — Test & ship (partial)
- ✅ Unit + live tests (skip cleanly without `pi`), ✅ 10k-message model-layer perf test, ✅ signed `.app`/DMG + optional notarization/stapling, ✅ GitHub Actions CI. ⬜ on-device scroll-perf benchmark, streaming state-machine edge coverage.

## Waku reference file map (pattern reuse, not copy — their gpui is a fork)

| Need | Waku file(s) |
|---|---|
| Transcript + streaming + veil | `src/app/transcript_view.rs`, `src/app/streaming.rs`, `src/md/veil.rs` |
| Markdown renderer / highlight | `src/md/render.rs`, `src/md/highlight.rs`, `src/md/parser.rs` |
| pi RPC transport (adapt to raw protocol) | `crates/waku-core/src/driver/pi.rs` |
| Review diff sources, parsing, sticky headers, changed-files tree | `src/review_diff.rs`, `src/app/right_panel.rs`, `crates/waku-core/src/workspace.rs` (`resolve_diff_range`) |
| Per-turn checkpoints (Last Turn) | `crates/waku-core/src/checkpoint.rs` |
| Diff syntax highlighting | `src/md/highlight.rs` |
| Integrated terminal (PTY + emulator) | `src/terminal.rs`, `crates/waku-core/src/terminal.rs` — both build on `alacritty_terminal` |
| Theme/chrome/sidebar/sessions | `src/theme.rs`, `src/app/window_chrome.rs`, `src/app/sidebar.rs`, `src/app/sessions.rs` |
| Usage/charts | `src/app/usage_page.rs`, `src/app/usage_meter.rs` |

## Definition of done (whole migration)

Every feature in `PRODUCT.md` → `Capabilities` runs natively in the GPUI app against the pi CLI with the same behavior as the legacy UI; legacy `src-tauri` deleted; `cargo run -p orbit-pi` is the only way to launch.

## Verification

- `cargo build --workspace` must stay clean (zero warnings) after every change.
- `cargo test --workspace` — unit tests + live pi integration tests. `live_pi.rs`, `live_catalog.rs`, and the other live tests skip when `pi` is missing; `orbit-pi` session-store tests skip when `~/.pi/agent/sessions` is empty. CI runs build + clippy on macOS (`.github/workflows/ci.yml`); tests run locally.
- Run the app: `cargo run -p orbit-pi`. Check: sessions list from disk, new session (`cmd-n`), prompt streams into the transcript, model/thinking pickers, abort (`escape`), settings (`cmd-,`), sidebar toggle. Notifications need an app bundle — `scripts/run-bundled.sh` builds the debug binary into the ad-hoc-signed `Orbit Pi Alpha` (bundle id `dev.orbit.pi.alpha`, Alpha icon) and opens it.
- Windows: the same `cargo run -p orbit-pi` builds and runs (GPUI renders through DXGI here). `gpui`'s default `windows-manifest` feature is off in `crates/orbit-pi/Cargo.toml` — it links a second `RT_MANIFEST` resource beside the one `build.rs` embeds, and CVTRES fails the link with `CVT1100: duplicate resource`. `scripts/bundle-windows.ps1` builds the Inno Setup installer (`scripts/installer/orbit-pi.iss`); it needs Inno Setup 6 (`ISCC.exe`) and, per-user under `%LocalAppData%\Programs`, no elevation. Desktop notifications and the alert sound are still macOS-only stubs. Window chrome is the app's own: the header row carries minimize / maximize / close, drawing a restore glyph when `window.is_maximized()`, and the header strips drag the window through `platform::start_window_drag`. Check them by clicking — `IsIconic`/`IsZoomed` flip, closing exits the process — and check dragging by pressing a header strip and moving the pointer; both were broken while the presses were routed through GPUI's `WindowControlArea` path, which this app's focusable root makes unusable (see the Shell row).
- No `unsafe` without a comment; no new dependencies without a stated reason.
