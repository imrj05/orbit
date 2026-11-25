# INTENT.md — Decisions, rationale, and plan

This file captures **why** Orbit was rebuilt as a native app and the decisions that shape it.
It is the source of truth for architecture choices; when AGENTS.md or README.md conflict with
a decision recorded here, this file wins. Update it when a decision changes (and say why).

**Maintenance contract:** every shipped feature updates the *Implementation status* below — and
adds or revises a `D#` decision when the architecture changed. A feature is not done until this
file describes it. `CHANGELOG.md` records what shipped; this file records why, and the
architecture that follows from it.

## Vision

Orbit is a **100% Rust, GPU-rendered desktop workbench for the pi coding agent** —
the same architecture as Waku, scoped to pi only — with **every current feature preserved**:

- no browser, no webview, no Node.js daemon anywhere in the app
- UI rendered by GPUI (the framework Zed is built on) on Metal
- the pi CLI (`pi --mode rpc`) as the only agent runtime, spoken to over stdio JSONL
- same sessions as the terminal (`~/.pi/agent/sessions/`), pi's own catalog, thinking levels, and persistence

## Decisions

### D1 — Transport: pi CLI RPC over stdio (replaces the Node daemon + SSE)
**Choice:** spawn `pi --mode rpc` as a child process per open session; JSONL commands on stdin,
JSONL events on stdout. Retire `agent/` (pi-coding-agent SDK daemon) and the SSE surface.

**Rationale (verified against the official RPC spec, `packages/coding-agent/docs/rpc.md`):**
- streaming text/thinking, tool calls, model catalog (`get_available_models`), thinking levels
  (`get_available_thinking_levels`), prompt-with-images, steer/abort/clear_queue, session ops
  (switch/new/fork/clone/rename/get_tree/get_entries), slash commands (`get_commands`)
- user interaction via the extension UI sub-protocol (`extension_ui_request`/`extension_ui_response`:
  `select`/`confirm`/`input`/`editor` dialogs) — covers question prompts and approval UI
- providers CRUD and scoped models are file-based (`~/.pi/…` config), readable/writable from Rust
- removes Node as a runtime, the bundled `pi-sse.mjs` sidecar, and localhost port management

**Residual risk:** per-tool *permission* requests in RPC mode are unverified (pi's permission
system is TUI/launch-flag territory; Waku sidesteps with `--approve`). **Resolved:** pi core has no
per-tool permission surface in RPC mode and no sandbox, but an extension's `tool_call` hook can
block/confirm and `ctx.ui.select` *does* surface over RPC as `extension_ui_request`. Orbit ships
`contrib/orbit-guard-extension/` for its Supervised / Auto-accept edits / Full access modes rather
than faking a pi-native mode. `--approve` is project trust (a separate concern). The Node daemon is
removed and is not a fallback.

### D2 — Mermaid diagrams
**Choice (revised):** a **native GPUI renderer for a Mermaid subset** — option (c) below — shipped
for `flowchart`/`graph` (TD/TB, BT, LR, RL; common node shapes; chained and labelled edges;
`subgraph` groups drawn as titled frames).
Everything outside the subset keeps the code-block fallback. Fences are still parsed as ordinary
code blocks, so the transcript's block model, copy path, and fold state are unchanged; the
renderer upgrades a fence to a diagram at paint time and offers the source behind a disclosure.
The original "code-block fallback only" choice and the two alternatives it weighed:
(a) graphviz DOT for flowchart/sequence and (b) a hidden native WKWebView used solely as a
**rasterizer** are both **rejected**: (b) reopens the no-webview rule for one feature, and (a)
trades a large dependency plus a translation step for diagrams whose source the agent already
wrote in Mermaid.
**Reason:** no good native Rust Mermaid exists, so the renderer is ours; a full custom renderer is
2–3 weeks and still won't cover gantt/pie — which is why the subset is explicit and the fallback is
the default for anything outside it. **Never present a diagram that isn't rendered:** a fence the
parser does not fully understand (another diagram type, an unparsable statement) falls back to the
copyable source rather than drawing a partial graph. Layout is
cached by source + interface font and needs no JS, no webview, and no network.
**Cost of the subset:** sequence, class, state, gantt, pie, ER, and journey are not drawn yet; each
is its own parser + layout in `src/mermaid/`.

### D3 — Rollout: full commit to GPUI (supersedes the original staged plan)
**Choice (revised):** the web stack (React/Vite/Tauri/Node daemon) was **removed from the repo** and the GPUI app is the only app. The original plan kept both apps alive until chat parity; that was superseded when the web stack was deleted at the user's direction.
**Consequence:** with no fallback UI, the GPUI app is the product, and the parity contract below was the build-forward backlog rather than a switch-over checklist. P3/P4 have since landed (see Implementation status); the deleted web app remains recoverable from git history if a reference implementation is ever needed.

### D4 — GPUI pinned to crates.io, not tracking main
**Choice:** `gpui = { version = "0.2.2", features = ["runtime_shaders"] }` — upgrades are
deliberate and scheduled, never `git main`.
**Reason:** pre-1.0 API with frequent breaking changes; reproducibility for users and CI.
**Note:** `runtime_shaders` compiles Metal shaders at app start via the GPU driver, because this
machine lacks the Xcode MetalToolchain (`metal` CLI). For release builds, consider installing
the Metal toolchain and precompiling shaders (drop the feature) to shave startup time.

### D5 — Platform: macOS first-class, Windows/Linux shipped best-effort
**Choice (revised):** macOS is the first-class target — signed + notarized `.app`/DMG, native
window chrome and menu bar, desktop notifications. Windows (`scripts/bundle-windows.ps1` → Inno
Setup installer / bare `.exe`) and Linux (`scripts/bundle-linux.sh` → `.deb` / `.tar.gz`) build
from the same codebase and ship via `.github/workflows/release.yml`; they are best-effort
(`continue-on-error`) until those platforms are validated.
**Reason:** the app was written cross-platform from the start (`secondary` modifiers, its own
window chrome on Windows, no macOS-only assumptions in shared code), so once the macOS app
shipped the earlier "out of scope" framing no longer held. The dev machine is macOS, which is
why its backend (Metal + font-kit) got first attention.
**Open platform gaps:** notifications and the alert sound are macOS-only stubs; Windows/Linux
native polish and signing remain.

### D6 — Reference, don't vendor
Waku's gpui is a fork (`waku-webview` branch) — **their code is pattern reference, not
copy-paste** for our pinned 0.2.2. Reuse the *designs*: stream veil, one-StyledText-per-block
markdown, highlight-as-paint, coalesced ≤8.3 Hz commits, per-session process management.

### D7 — Provider OAuth is an RPC capability, not client logic
**Choice:** provider authentication is a first-class `auth.*` RPC namespace. Orbit defines the
wire contract (`crates/orbit-rpc/docs/auth-rpc.md`), reduces the events into non-sensitive UI
state (`crates/orbit-pi/src/auth.rs`), and opens authorization URLs with the OS. The **pi**
process implements the server side by reusing its existing provider auth/OAuth implementations
and its `~/.pi/agent/auth.json` credential store. Orbit never implements a provider OAuth flow,
never reads token material, and never persists tokens.

**Rationale:** credential storage and refresh already live in pi; duplicating them in Rust
would fork the source of truth and risk token leakage into GPUI state. Login session ids,
structured error codes, cancellation, timeout, and restart recovery make the async flow
deterministic on the client.

**Compatibility:** a pi build without `auth.*` replies "Unknown command", which Orbit treats as
*unsupported* and falls back to the existing file/Terminal login path. Existing provider
functionality is preserved; nothing is faked.

**Interim server side:** stock pi has no `auth.*` handlers, so `contrib/pi-auth-rpc/apply.mjs`
injects them into an installed pi by wrapping `ModelRuntime.login`/`logout`/`listCredentials`
(reusing pi's OAuth and `auth.json`). It writes a `.orbit-orig` backup and reverts with
`--revert`; re-run it after `pi update`. Upstreaming the handlers is the durable fix.

### D8 — Workflow modes: per-session scope via a `tool_call`/`before_agent_start` extension
**Choice:** a session is scoped **Plan**, **Build**, or **Ask**. Orbit persists the
mode per pi session id in `~/.orbit-pi/workflow.json` (a map), and the bundled
`contrib/orbit-workflow-extension/` reads it fresh on every hook. Plan and Ask drop
the write tools with `pi.setActiveTools`, default-deny MCP, custom, and orchestration
tools, gate `bash` to conservative per-utility read-only policies on `tool_call`,
and inject hidden guidance on `before_agent_start`; Build is the neutral default. The mode is also written into the session as an `orbit:workflow` entry so
it survives resume and branch.

**Rationale:** pi exposes no RPC command for active tools or the system prompt, so
extension hooks are the only honest mechanism — the same call D1 made for access
modes. Per-session (not global like `access.json`) because scope is a property of
the task, and up to six parked sessions run at once. Reading the store fresh on every
hook re-arms a live session with no restart.

**Residual:** this is a guard, not a sandbox (pi ships none).

### D9 — Localization: `en.yml` owns the keys, each locale file owns its translations
**Choice:** all user-facing copy goes through the crate-root `tr!` / `tr_cow!` macros
(`crates/orbit-pi/src/i18n.rs`). `locales/en.yml` is the source of truth for keys, their order and
placeholders; each of the other nine locale files is hand-edited and holds that language's
translations. `scripts/check_i18n.py` fails on a locale that is missing a key, keeps a stale one,
drifts from `en.yml`'s order or drops a placeholder, and `--sync` brings every locale in line
(new keys arrive in English, to be translated); `cargo test -p orbit-pi i18n` guards completeness.
*Revised:* translations used to live in sixty `scripts/i18n_glossary*.py` tables keyed by the
English text and were generated into the locale files. That stored every translation twice, let a
later table silently override an earlier one (91 conflicts), and tied one translation to every key
sharing an English string; the locale files are now the only copy. Ships English, 简体中文, 日本語, 한국어,
Español, Français, Deutsch, Português do Brasil, Русский, Italiano — plus `System`, which walks
the OS preferred-language list and picks the first shipped language.
**Rationale:** ten hand-maintained locale files drift unless something checks them; a guard that
compares each file to `en.yml` (keys, order, placeholders) plus an untranslated-string baseline
keeps them honest without blocking releases, and with one copy of each translation. Long-lived fields resolve
placeholders from keys at paint time, so a language change is live (including the native menu
bar) without rebuilding them. **Consequence:** new copy adds an `en.yml` key, a `--sync`, and a
translation in each locale file — hard-coded English in a render path is a bug.

### D10 — Provider quota: a bundled extension bridge over pi's own credentials
**Choice:** account usage (quota / balance / spend) is read by the bundled
`contrib/orbit-quota-extension/`, loaded with `pi --extension` like the access/workflow guards.
It fetches through pi's own authenticated `ctx.modelRegistry`, appends normalized `orbit:quota`
custom session entries **only when the snapshot changes** (never in LLM context), and the app
polls `get_entries` with a per-session `since` cursor; `crates/orbit-pi/src/quota.rs` reduces
them to non-secret UI state, and `crates/orbit-rpc/docs/quota-rpc.md` is the optional
`quota.list` server contract for a patched pi. Only normalized percentages / counts / resets /
amounts cross the boundary — never a token, key, or account id.
**Rationale:** the same reasoning as D7 — credential storage and refresh already live in pi, so
a Rust re-implementation would fork the source of truth and risk leaking secrets into GPUI
state. The extension path needs no pi patch and degrades to `unsupported` per provider (Gemini,
for example, has no account-quota API). **Residual:** coverage is per-provider and rate-limited;
a report is cached 60 s, and a provider without a verified adapter reports `unsupported` rather
than an invented number.

### D11 — Design tokens: Zed's sizing vocabulary, resolved through Orbit's theme
**Choice:** `crates/orbit-pi/src/theme/tokens.rs` uses a Zed-derived vocabulary adapted
to Orbit: `DynamicSpacing` (Base00–Base48 × three densities), `TextSize` / `HeadlineSize` /
`BufferLineHeight`, `IconSize`, `ButtonSize`, `ListItemSpacing`, `Radius`, `ElevationIndex`,
`Motion`, and per-component metrics. Orbit's radius ramp and restrained elevation differ
from Zed's original values. Dynamic spacing resolves the Compact / Default / Comfortable
anchors at 80 / 100 / 120% density, interpolates between them, then follows UI font scaling.
Other tokens have their own resolvers: radii use fixed pixel steps plus the user's corner
rounding, while motion durations do not scale with font size or density. UI transitions use
`Motion` roles; charts and looping affordances have separate cadences. Colors remain Orbit's
semantic palette. The Orbit design skill specifies the shared non-color grammar.
**Rationale:** the UI audit found no shared sizing layer: ~1,200 per-call-site px literals, 17
distinct text sizes, 12 radii, and chrome heights that did not follow the UI font size. Zed is
a useful GPUI precedent, and adapting its token vocabulary gives every surface a shared
scale while allowing Orbit's design to evolve. **Consequence:** new chrome sizes through
`tokens` instead of literals; surfaces migrate one at a time. Context menus (sidebar session /
workspace, Explorer, Git sync / branch / file), the tooltip, and the extension dialog are on
the tokens, as are the floating modals (provider usage / API-key / provider editor, the update
dialog, and the custom-UI card) via `elevation_3` and the `modal` metrics; the legacy heavier
`popover_shadow` is gone. Sidebar session / workspace rows use Zed's list-item radius and the
`TextSize` / `DynamicSpacing` scales, as do the transcript's message rows, user bubbles, tool /
question card shells, error strips, the usage footer, and the settings section / group / row
chrome and keycap chips. The sidebar's Search / Usage controls share one
`button_frame(ButtonSize::Large)`. The ad-hoc `Theme::space` (density-only) helper is gone: every spacing
site now resolves through `DynamicSpacing`, so chrome tracks the UI font size as well as
density. UI type is on the `TextSize` scale across every surface (471 of 517 `.text_size` sites);
what remains is sub-10px badge glyphs, 17px+ display headings, and editor / code text. Corner
radii are on the `Radius` scale: gpui's fixed `rounded_sm/md/lg/xl` helpers and every on-scale
`rounded(px(N))` are gone (only 23 off-scale bespoke radii remained at that sweep). One-shot
UI transitions use `Motion` roles, layered on `AnimationDuration`; charts, looping affordances
(spinner, shimmer, streaming pulses) and feedback timers keep their own cadences. The settings page is
on the token scales throughout — its side nav, headers, sections, rows, toolbars, selects, cards,
and modals read `DynamicSpacing` / `TextSize` / `Radius` / `ButtonSize`, and it has no `ui_px` or
gpui spacing utility left; only bespoke geometry remains (avatar / dot / toggle sizes, fixed
panel and modal widths). The Usage page and its `chart` / `heatmap` / `table` / `filters` /
`tooltip` / `sparkline` modules are likewise on the token scales for spacing, type, and radius;
only their chart / table geometry is bespoke. The page itself is one continuous read — Daily
activity (the trailing-year calendar), Summary, Usage over time, Breakdown, Records — with each
section's selector inside its card rather than a page-level Simple | Details switch. The
Summary board is a row of KPI cards with a 40px line each (the bklit KPI-sparkline recipe):
the four figures read the range at rest, and pointing at any line points the whole row at that
bucket — figure, date, change, and secondary line — with the line drawn in on first paint,
morphed when the range changes, and highlighted (no hover animation) where the pointer reads. The Breakdown section's four tabs share one legend row (the bklit
legend with progress bars): a brand tile from the picker's own marks, the name and scope, the
figure and share, and a share track; the table under it leads each name with the same mark. Every single-line input box in the app routes through
`input_field_frame` (or the `input::*` metrics directly, in the extension dialog) — the main
composer, the file editor, and the inline tree rename are the deliberate exceptions, and each is
its own component rather than an `InputField`. Every search box in the app shares one look — the
picker search row (`picker_search_frame`): the command palette, all pickers, the Explorer /
Sidepane filters, the settings / Git / Usage searches, and the transcript find bar. The Git
panel's tab strip and branch row are on the tokens too (one shared `tab_bar` for Changes /
History / Graph / Issues / Pulls). The 2026-10 spacing sweep finished the holdout chrome: the
top-level view (sidebar / onboarding / popovers), the transcript's row chrome, the session
strip, the Sidepane review rows, diff view, the context meter, the Explorer panel and viewer,
the update dialog, the Skills page, the custom-UI surfaces, the Git panel widgets, and the
dialogs — every on-scale `p/px/py/pl/pr/pt/pb/m*/gap(px(N))` site in the app now resolves through
`DynamicSpacing`. What stays px on purpose: 1–2px optical nudges (dot / badge insets), the
legacy 6–10px rhythm values with no token equivalent (10/14/18/22 — folded into the scale or
retired by design, pending), and bespoke geometry (widths, heights, icon and dot sizes).
The palette set is thirteen, in one design system (D15): seven dark and six light. The blue
design holds **Orbit Azure** / **Orbit Azure Light** and is the appearance default, while
**Orbit** / **Orbit Light** carry the original ember identity, and **Operational** /
**Operational Light** sit in the same Dark and Light galleries as every other palette. Primary
buttons fill from a role separate from the identity accent — `action` / `action_hover` /
`action_pressed` with `action_fg` ink — because the ember palettes key their buttons to a light
neutral with dark ink while Azure keys them to its accent; `accent` / `accent_hover` /
`accent_pressed` stay the identity hue for selection washes, focus, tints, and accent text
(see DESIGN.md, *The Primary-Action Role*).

### D12 — Default session model: Orbit-owned config, applied at spawn and session birth
**Choice:** one optional default model (`provider` + `modelId`) and thinking level for new
sessions, persisted to Orbit's own `~/.orbit-pi/session-defaults.json`. An empty slot means
pi's own startup default is left alone. The choice is applied through both paths a session
can be born on: `--provider` / `--model` / `--thinking` flags at process spawn
(`BundledExtensions::spawn`), which cover the first session, and `set_model` /
`set_thinking_level` RPC for every later `new_session` inside a live process.
`switch_session` and `clone` deliberately do not override a resumed session. Settings →
Agent exposes the model plus a thinking-level select whose options are derived from the
catalog entry exactly like pi's `getSupportedThinkingLevels` (`reasoning` +
`thinkingLevelMap`); a model or level pi's catalog does not support is skipped, and a
malformed file loads as no default. Per-mode defaults were deliberately dropped: mode is a
per-session scope (D8), not a reason for a different startup model, and the composer chip
already changes model/thinking per session.
**Rationale:** one pi process hosts many sessions and a `new_session` re-applies the
spawn-time CLI flags to each one, but those flags are frozen at spawn: a default
changed while the process is live would only reach a new session through the RPC
path, and RPC alone would leave the first session on pi's global default. Orbit owns
the file for the same reason themes do (D9):
`~/.pi/agent/settings.json` is global to the machine, is rewritten by pi itself, and
writing it would silently change terminal `pi` sessions. Deriving the thinking ladder from
the catalog keeps the settings control honest for the selected model instead of showing
the live session's levels; fail-soft keeps a stale id or level from poisoning a session.
**Consequence:** new sessions only — a resumed session keeps the model and thinking level
recorded in its file, and a manual composer choice wins until the next new session.

### D13 — Command registry is the source of truth for keys, palette, and help
**Choice:** app commands live in one static table (`crates/orbit-pi/src/commands.rs`): id,
category, title/detail i18n keys, icon, palette/help flags, and the advertised chord.
`BINDINGS` rows name the chord, key context, and command, and `key_bindings()` turns them
into the typed gpui bindings that `bind_keys` installs first. The command palette builds
its rows from the registry; Settings → Shortcuts renders the same table; both label
chords through `platform::shortcuts::label`, which formats canonical chords
(`secondary-n`) per platform. Text-entry conventions and modal keymaps stay literal in
`bind_keys` — they are conventions, not commands. Tests reject duplicate
(chord, context) pairs, missing specs, missing keymap rows, and chords that do not parse.
**Rationale:** the previous layout had four sources for one shortcut (the keymap literal,
hand-typed glyph constants, palette items, and the settings reference) which had already
drifted: `cmd-` bindings never fired on Windows/Linux, and `secondary-period` advertised
a chord gpui never matches (the key is `.`). One table makes drift impossible at
test time and is the only way “discoverable keyboard-first” stays true as commands are
added.
**Consequence:** a new app command is added once — spec, `BINDINGS` row, and a `CommandId`
run arm — and appears in the palette, the shortcut reference, and the keymap together.
Context actions (the review tree, Git tabs) are registry commands too, but never palette
rows.

### D14 — Git worktrees are workspaces, not a parallel filesystem
**Choice:** a Git worktree is modeled by a dedicated `WorktreeManager`
(`crates/orbit-pi/src/worktree.rs`, with the optional setup script in
`worktree_setup.rs`) and driven only through the `git worktree` porcelain; the UI
(`app/worktrees.rs`) consumes that abstraction. Opening a worktree sets
`current_workspace` and adds it to the project list like any other folder — the
Explorer, terminal, Git page, Review, MCP scope, file watcher, and agent read that one
path, so no second filesystem abstraction exists. Worktree **name**, **branch**, and
**path** stay separate identities; renames move only the directory.
**Rationale:** sessions already persist an absolute `cwd`, and every consumer already
reads the active workspace path. A separate worktree-specific layer would duplicate
that plumbing and create a second source of truth for “where the agent runs”. The
porcelain requirement (never editing `.git/worktrees` or `.git/config` by hand) keeps
Orbit compatible with Git's own metadata format across versions.
**Consequence:** new workspace-scoped features get worktree support for free by honoring
`current_workspace`; worktree lifecycle/UI changes stay in `worktree.rs` / `app/worktrees.rs`.
A missing worktree surfaces a notice rather than silently re-pointing a session at the
main repository.

### D15 — One design system; palettes are colorways, not forks

The shared visual language uses softly rounded controls and cards, with frosted
floating surfaces where supported and soft shadows for separation. Palette roles,
contrast floors and opaque accessibility fallbacks remain common to every colorway.
**Choice:** there is one design system. Classic (the Orbit family, Flexoki, Onyx) and
Operational used to be separate `DesignSystem` values behind their own Settings switch; once
both shared the same radius, typography and elevation grammar the split only hid palettes,
so `DesignSystem` was removed. `AppearancePrefs` keeps one remembered palette per
appearance — any palette of that appearance, Operational included — and
`AppearancePrefs::resolve` honors Light/Dark/System. Operational still supplies its own
palette and `surface_selected` row wash. Preferences written with
`"design_system": "operational"` migrate to the Operational pair, so what was on screen
stays on screen.
**Rationale:** a second widget tree or parallel CSS-like layer would duplicate every surface
and drift; a palette-family switch on top of a shared grammar only made two of thirteen
palettes harder to find.
**Consequence:** new visual rules use semantic `Theme` roles and shared `theme/tokens.rs`
resolvers; never branch geometry on a palette. New palettes join `ThemeId::ALL` and the
palette-invariant tests, and appear in Settings → Appearance's Dark or Light gallery through
`themes_for(mode)`, the one ordering the tiles and their keyboard handler resolve through.

### D16 — Git account diagnostics are local and read-only

Settings → Git Accounts inspects effective Git author settings, normalized generic and host-scoped helper configuration, SSH agent public fingerprints, conventional public-key file presence, sanitized remote hosts, GitHub CLI version and the presence (not values) of selected environment variables. Its separate, explicitly requested release check makes an anonymous request to GitHub for the latest stable `gh` version and shows manual package-manager commands; ordinary status refresh stays local and Orbit never runs the update. It does not manage credentials, execute helpers, inspect private keys or test remote access. Commit identity is not authentication, and helper or agent presence does not establish valid credentials or remote permissions. `gh auth status` may access the network and refresh credentials, so this local-only inspector does not run it and reports GitHub authentication as unknown. Desktop process environment can differ from the interactive shell; users resolve authentication manually in their own tools. No additional service or secret store is involved.

### D17 — The check reads the feed; a download is a user's decision

**Choice:** `Updater::start_check` fetches only the per-architecture appcast
(`updater.rs::fetch_feed`) and publishes `Available` with a `ReleaseOffer` — the
release's version, notes, artifact URL, length, and signature. The artifact is
downloaded, length-checked, signature-verified, and staged only when
`Updater::download_available_update` is called from the modal's **Download**
button, which reports throttled `Progress` events and can be cancelled. A
verified payload publishes `Ready`; the install handoff is a separate press
(**Restart & Update**). The modal shows the target version beside the running
build and links the release page whose body is the notes it renders inline.
**Rationale:** the old flow downloaded during a silent launch check, so the
user's first sight of a release came after the bytes had moved, and the pill
had to mean "downloaded" while wearing a download arrow — the wrong affordance
for a restart. Splitting read from fetch makes the changelog and version
genuinely precede the download, gives the download a real progress bar, and
makes the silent launch check cost one small request.
**Consequence:** `UpdateStatus` distinguishes offer (`Available`) from payload
(`Ready`); release details stay on the worker and only the version, notes, and
progress cross into a frame. Anything that gates on "staged" must gate on
`Ready`, and the modal's primary action changes meaning with that status.

### D18 — Subagents: a delegation tool over child `pi` processes

**Choice:** Orbit ships one bundled extension (`contrib/orbit-subagents-extension/`) that registers a
`subagent` tool. A call spawns a child `pi --mode json -p --no-session` process per task and returns
the child's final text plus its summed `usage`; the child's transcript, tool arguments and file
contents never enter the parent's context. An agent is a markdown file in
`~/.pi/agent/agents/*.md` (the path `pi-subagents` reads) with `name`, `description`, `model`,
`thinking` and `tools`. The child's **ceiling** is derived from that `tools:` line, passed as
`ORBIT_SUBAGENT_CEILING`, and the guard clamps every child tool call against it even under Full
access. Before spawning, the extension intersects the ceiling with the session's access and workflow
modes: a run that cannot finish is refused with the remedy instead of spending tokens and dying on
its first denied call.
**Rationale:** the parent's context and the user's money are the scarce resources. Delegation pays
only when the child's reading stays in the child, so the report is capped at 4 KiB, the cost rides
the tool result (pi folds it into session totals), and the child is a normal session in the same
workspace with the same guardrails — there is no second permission system to drift. The tool stands
down when another extension already owns the name (`pi-subagents`), because two implementations
racing for one name is worse than one honest absence.
**Consequence:** `subagent/1` is the `details` contract the transcript card parses (absent or foreign
details degrade to the generic card). The guard classifies `subagent` as `delegate` and allows it in
every mode: the child cannot exceed the mode and the extension refuses impossible runs, so a prompt
would approve nothing. Depth is 1 and applies to Orbit's own tool; a third-party delegation extension
loaded in the child is not constrained by it. D23 scopes approval and exact MCP
secret inheritance to the same canonical workspace; cross-workspace children
use `--no-approve` and drop those secrets.

### D19 — Permissions: rules over the guard, per command and per agent

**Choice:** the access mode stays the global baseline, and a second layer —
*granular rules* — lives in `~/.orbit-pi/access-rules.json`, written by
Settings → Permissions and read fresh by the guard extension on every tool call.
A rule names a scope (`any` / `session` / `agent:<name>`), a tool family (the
guard's own `classify()` kinds plus `any`), an optional glob on the call's
salient argument, and an action (`allow` / `ask` / `deny`). Precedence: **Full
access is answered before the table** — it means "nothing asks, nothing is
refused", so no rule can turn it back into a prompt; in a confined mode the
scopes resolve most-specific first, then deny > ask > allow inside the winning
scope, then the mode's default. The subagent ceiling is clamped before any
rule. `deny` blocks
without a prompt, `ask` with no UI to ask through is refused, and an `allow`
never covers a compound shell command.

**Rationale:** the mode could only answer "ask for everything" or "ask for
nothing", and `grantable()` already refused tool-wide grants for `bash` and MCP
for exactly that reason — one approval would have covered every future command.
Rules close that gap without a second decision engine: `policy.js` keeps the
one decision table, and `permissions.rs` only describes it (the defaults table is
mirrored for the page's baseline, with a test on each side). Per-agent scoping is
free because a child already carries `ORBIT_SUBAGENT_AGENT`, and it is where the
feature pays most: a child has no approval window, so an `allow` rule is what
lets a worker run `git status` at all.

**Consequence:** `net` (`webfetch`, `websearch`) became its own family so a fetch
can be gated, while its mode default stays `allow` — no existing session changed
behaviour. The file has one writer each: the app owns `access-rules.json`, the
extension owns `access-allow.json`, so there is no read-modify-write race. A
rule is a guard, not a sandbox: command matching is a glob, not a shell parser,
which is why an `allow` is refused for chained/redirected/substituted commands
and why `deny` re-checks every segment.

### D20 — Remote sessions: pi over ssh stdio, agent only

**Choice:** a remote session is the same `PiClient` driving
`ssh -T -o BatchMode=yes … HOST "sh -lc '<script>'"`, where the script installs
the access guard under `~/.orbit-pi/remote-guard/`, pins the host's
`~/.orbit-pi/access.json` to `supervised`, refuses to continue without the
guard (exit 78) or without `pi` on the login PATH (exit 127), and `exec`s
`pi --mode rpc (--approve only when D23 trusts the remote workspace, otherwise
--no-approve) --extension <guard>`. Hosts live in
`~/.orbit-pi/remote-hosts.json`, managed from Settings → Remote hosts (add/edit
form with `~/.ssh/config` alias suggestions and a read-only Test connection
probe) and written surgically — only the keys Orbit owns change. Only the agent is remote: every surface that
reads the local disk (Files, Git, Review, Worktrees, Explorer, `@` mentions) is
shown as unavailable while `OrbitApp::remote` is set.

**Rationale:** pi's RPC is already JSONL over stdio, and ssh carries stdio
unchanged, so nothing new crosses the wire and `orbit-rpc` keeps its
serde/anyhow-only dependency set. The alternative t3code takes — run a full
server on the host, forward a port, pair a bearer session — exists because its
remote side is a web server; Orbit's is a CLI. Rendering the local disk beside a
remote agent would describe the wrong machine, which D1's "trust pi's truth"
forbids, so those surfaces say so instead.

**Consequence:** one spawn seam (`OrbitApp::spawn_pi`) carries every restart
(Start, reconnect, MCP apply, forced-stop respawn) so a remote session never
comes back local; ⌘N, a folder pick and a listed session are local by
construction. A remote session is not parked: its file lives on the host, so a
parked process could never be resumed, and it is stopped with a toast instead.
The workflow extension ships with the guard (`RemoteLaunch.extensions`): the
mode starts as `ORBIT_WORKFLOW_MODE` and later changes are written into the
host's own store over ssh, because the extension reads that file, not this
Mac's. Projects reach a host by `git clone` there, never by uploading the
working tree (t3code's model): only pushed history travels, the host needs its
own git access, and the ignored files a clone cannot bring are copied one by
one on stdin into the clone. Not yet: quota, auto-title and subagents on the host (they read state
Orbit keeps locally), MCP secrets
forwarding, password logins on Windows, and trusting a new host key in-app.
Passwords follow t3code: keys first in `BatchMode`; on `Permission denied`
with a password method listed, a masked dialog asks, and the probe reruns with
`SSH_ASKPASS` pointed at a secret-free helper that reads the password from one
ssh process's environment. Memory only, keyed by ssh account, dropped when
refused.

### D21 — Multiple accounts per provider: every account is its own provider

**Choice (revised):** an extra account is an *alias provider*. Its metadata
(name, provider, alias) lives in `~/.orbit-pi/accounts.json`; the bundled
`contrib/orbit-accounts-extension/` registers, at pi startup, a copy of the
built-in provider (`builtinProviders()` from pi-ai) under the alias id
`<provider>~<slug>` and the name `<Provider> · <name>`. pi keys credentials by
provider id, so each account signs in (browser, device code, pasted code or API
key, through the ordinary `auth.*` flow), refreshes and is stored separately in
pi's own `~/.pi/agent/auth.json`. The Default account is the built-in provider
itself. Every account is live in every session at once: their models sit side by
side in the model picker, and nothing is selected per session.

**Rationale:** the first version routed one account per provider per session
through a pi patch (`contrib/pi-auth-profiles/`), so two accounts of one provider
could never be used in the same session and switching restarted pi. A complete
`Provider` registered by an extension carries the base provider's OAuth login,
refresh and streaming unchanged (verified against pi 1.5.2: `auth.list` lists the
alias with `browser` and `device_code`, `--list-models` lists its models, and a
request dispatches with the alias's own credential). No patch, no copied token,
no restart on use.

**Consequence:** adding, renaming or removing an account restarts pi (it
registers providers at startup) — at once when idle, otherwise through the
"Restart pi" banner. Removing an account signs it out (its `auth.json` entry is
deleted). The alias slug is fixed at creation; a rename changes only the name.
Built-in providers only: a `models.json` custom provider can be duplicated by
hand. Remote sessions use the host's own providers. Version-1 stores
(`provider-accounts.json` + per-profile `auth.json`) migrate on load: each
credential is *moved* under its alias (never overwriting a live entry), the old
folders are deleted, the per-session bindings are dropped, and the old file is
left as an empty version-1 store so an older Orbit build still starts. The
per-session routing plumbing remains in the spawn path but always carries an
empty selection; removing it is a follow-up.

### D22 — UI components: one owner per element, in `ui/`

**Choice:** every UI element family has exactly one defining file under
`crates/orbit-pi/src/ui/` — `Button`, `IconButton`, `ButtonGroup`,
`LinkButton`, `ChipButton`, `Badge`, `Pill`, `Banner`, `TextField`, `FormRow`,
`TreeRow`, `DiffRow`, `StatusBar`, `ModalHeader`, the table, tabs, menus,
popovers and the rest (DESIGN.md §Components lists them). Surfaces import
`crate::ui::prelude::*` and the named components; they own content, state and
actions, never an element's look. Two style tiers stay: zero-cost traits in
`ui/style/` (states, material, text) and components that own composition or
behaviour. Entities (`ComposerInput`, `FileViewer`, terminals) are unchanged.

**Rationale:** the design-system audit finished the vocabulary (roles,
tokens, states, motion) but each surface still re-assembled the same shells —
44 button functions, 24 popup renderers, 95 row renderers — so "primary" or
"selected" drifted between screens. One owner makes a fix land everywhere at
once and lets a guard prove it.

**Consequence:** `app/helpers.rs` is deleted (formatting lives in
`app/format.rs`). `scripts/check_ui_layers.py` keeps `ui/` from importing a
surface; `scripts/check_ui_owners.py` fails a second definition of a
component or owning function and any raw popover / dismissal / segment
primitive outside `ui/`. A one-off control that cannot use its component (the
composer's morphing send button, the updater pill) carries a `// Bespoke:`
comment saying why. The migration's declared visual changes are recorded in
`docs/plans/ui-component-layer.md` §6.2–§6.13.

### D23 — Project trust is explicit and scoped to the canonical workspace

**Choice:** Orbit no longer treats opening a folder as consent to execute its code. Every
pi process starts with `--no-approve` unless `~/.orbit-pi/project-trust.json` contains an
explicit trusted decision for that canonical local workspace or remote account+directory.
Unknown and denied workspaces keep Orbit's explicitly loaded extensions but do not load
project settings, extensions, skills, or MCP servers, and receive no MCP secret environment.
A native dialog persists Trust / Continue untrusted; the palette can revisit a denial. Trust
changes restart only after the active run settles. Local MCP commands always use the local
decision, never the remote session's.

**Consequence:** delegated children inherit approval only when their canonical cwd is the
same trusted workspace. Cross-workspace children use `--no-approve`, drop the exact exported
secret-name set, and re-apply Orbit-owned guardrail variables after secret removal. Plan/Ask
use conservative per-utility shell policies and default-deny MCP, custom, and orchestration
tools; invalid workflow evidence falls through to the session branch instead of becoming
Build. Remote Markdown images load inline through the same bounded client avatars use
(HTTPS only, timeouts, bounded redirects, an 8 MiB body cap); this revises the earlier
passive-link rule at the user's request (2026-11-23), and `PRIVACY.md` says what an image
host can see.
Updater verification consumes the same open file it verified, scratch directories are
owner-only, feed/error writes are bounded and non-following, clone URLs lose userinfo and
query credentials, and quota adapters require exact official HTTPS origins.

**Rationale:** project trust, tool approval, and workflow scope are separate controls. A
repository author, model response, remote document, or another local OS user must not cross
one of those boundaries without an explicit user decision or verified byte stream.

### D24 — A delegated child's live transcript is its own event stream

**Choice:** the subagents extension tees every JSONL line a child `pi --mode json` prints,
verbatim, into `~/.orbit-pi/subagent-logs/<run>-<child>.jsonl` (0600, `log.js`) and names the
file in the child's `subagent/1` details (`log`). A child's side-pane tab tails that file off the
UI thread on the heartbeat (`subagent_log.rs`: byte offset + partial line, ≤ 4 MiB per read) and
feeds each line through `Event::parse_line` into a second `Transcript`, painted by
`Transcript::render`. Children still run `--no-session`.

**Consequence:** a child streams in the pane exactly as the main session streams in the chat —
thoughts, tool calls with their output, the answer — with the same rows (activity groups, the
file-read listing, Markdown), and a finished child replays from its log. The `details` summary
stays bounded and is still what the card and the parent model see. The path is honoured only
when it is a plain `.jsonl` directly inside Orbit's own log directory, because a session file
can carry `details` from anywhere. Without a readable local log (an older extension, a remote
host, an unwritable home) the tab shows the summary panel as before. Logs older than a week are
removed when a run starts.

**Rationale:** pi's own event stream is the truth of what a child did (hard rule 5); widening the
`details` payload toward a transcript would resend the whole history on every 400 ms update,
while the event log only grows at its end and needs no new parser — the app already turns those
exact events into a transcript.

## The feature parity contract

Everything below must behave identically in the GPUI app (against the pi CLI) as it does in the
legacy app today:

1. Chat sessions — prompt composer, model selection, thinking-effort, steer/cancel/abort, streaming
2. Sessions grouped by project, persistent, reopenable, cross-workspace
3. Model catalog from pi's own list; thinking levels per model
4. Tool activity (bash, edit, todo, plan, search, mcp, thinking, question) + approval dialogs
5. Git diff panel + file-change blocks
6. Workbench pages — usage (activity calendar + KPI board + trend chart + breakdowns +
   record tables), skills, plugins, models, providers
   (CRUD + OAuth/quota), settings; Explorer + Files editor, integrated terminal, Git/GitHub,
   and the in-app signed updater (offer → download with progress → restart) with
   Version History and a changelog link
7. Markdown — GFM, syntax-highlighted code, copy-able code blocks, image lightbox
8. Theming — dark/light palettes, accent, font, density, reduce-motion. Persisted to **Orbit's own store** (`~/.orbit-pi/{theme,ui.json,fonts.json}`), not pi's settings: theme/UI state is Orbit's, and writing it into pi's config would fork pi's own source of truth. (Revised from "persisted to pi's settings".)
9. Attachments — images for prompts, compressed at ingest (1400 px longest edge, 512 KiB base64, JPEG quality 75; alpha images become lossless WebP) so a large screenshot never rides the session history at full size; non-image files ride as `@path` mentions the agent opens with its tools, held as chips in one tray beside the images and pastes
10. macOS native expectations — traffic lights, a native menu bar (Orbit/File/Edit/View, rebuilt
    on language change), native dialogs, keyboard operability. Orbit's actions are also exposed
    as keyboard/mouse surfaces (⌘P palette, shortcuts), not only through menus.

### Implementation status (living)

The prompt composer marks misspelled words with the wavy underline a native text
view draws, once typing pauses. Orbit paints its own text, so AppKit never
spell-checked it: `spellcheck.rs` splits a draft into prose runs (fenced and
inline code, paths, identifiers, commands, `@`-mentions and URLs break a run and
are never checked) and asks `NSSpellChecker` for the misspelled words in them,
which `composer.rs` paints as a per-run wavy underline — a paint channel only,
so the caret, selection and wrap points do not move. The check is debounced
350 ms from `render` (every edit path reaches it through the composer's
`revision`) and gated on `spellcheck::start()`, so a unit test never calls
main-thread AppKit; stale marks are clamped to character boundaries between an
edit and the next check. No auto-correction and no grammar checking on purpose.
A right-click on a marked word opens a context menu at the pointer with up to
five of `NSSpellChecker`'s suggestions (best first) and **Learn spelling**, which
writes the word to the user's own macOS dictionary and re-checks so the mark it
came from goes away. Only a *marked* word opens it — the squiggle is the promise
that the word was checked — and the caret does not move under an open menu. The
rows act on release rather than `on_click`: they are deferred children of the
composer, where the press→release pairing does not survive the frame the press
itself triggers (measured in the test below). Settings → Agent → Check spelling
(default on) is the switch, stored in `ui.json`; the row appears only on macOS
because the other platforms have no checker in this stack. Unit tests cover the
prose/exclusion rules, the UTF-16 to byte mapping, the clamp and the
"only a marked word" rule; composer tests prove the mark pass preserves the
covered length, that a suggestion replaces exactly the marked word, and — with a
real layout, a right-click and a release on a row — that the menu paints at the
pointer and applies the choice. GPUI tests cover the switch's platform presence
and the gate; the AppKit round trip in the running app is not yet verified by
hand.

The sidebar's Git button carries a dot when there is something to look at, and
its tooltip names what: changed files in the working tree, and issues or pull
requests opened on GitHub since the user last viewed that tab. The Git page's
Changes tab shows its changed-file count, and Issues / Pull requests show their
new counts until opened (the shared `Tab` gained a quiet count badge, capped at
99+). Local changes are counted on the branch fetch the workspace watcher
already runs, so they cost no network. The GitHub half (`git_activity.rs`,
`app/git_activity_ui.rs`) asks `gh` for two fields of the open lists every five
minutes and on window activation (no more than once a minute), skips the
user's own items, baselines a repository the first time it is seen so the
backlog never reads as new, and keys what was seen by `owner/name` in
`~/.orbit-pi/git-activity.json`. GitHub has no unread state, so "new" is
"opened after the last view"; Settings → Privacy turns the check off. Covered
by unit tests for the counting, baseline and store, and app tests for the dot,
its tooltip and marking a tab seen; the live `gh` round trip and the dot in the
running app are not yet verified by hand.

Markdown images render inline (`transcript_view::render_markdown_image`): an HTTPS
image loads through GPUI's image cache and the bounded `http.rs` client, records its
decoded size, and lays out at its own width narrowed to the column and to a 30 rem
height, keeping its aspect ratio; a click opens it in the browser. While it loads a
skeleton bar holds its place; a failed load (a private attachment, a dead link, a
plain `http://` URL) falls back to the alt text and an **Open** action. `file://` and
`data:` images never load. This revises the earlier passive-image rule; PDF export
still omits remote images. Covered by layout tests for inline, wide, quoted and
failing images; not yet verified by hand in the running app.

The shared tooltip fades in on the interaction motion role; persistent update
notice actions and their close control use the retargetable hover treatment.
Reduce Motion presents their final appearance immediately.

Tabs (terminal, right dock, Git page) fade their resting hover through
`motion::fade` from the one `ui/tab.rs` owner. A sidebar project header's `…`
and `+` actions fade in with the header's own wash through `motion::follow`,
which reads the parent's hover transition instead of keeping a second clock;
under Reduce Motion they fall back to `group_hover` and appear on one frame.
Covered by motion and tab unit tests; the fade in the running app is not yet
verified by hand.

Project trust (D23) now gates every local, remote, MCP, plugin, and delegated pi
spawn. Unknown workspaces run restricted with no project resources or MCP secrets;
the native trust dialog persists an explicit choice and the palette can reopen it.
Plan/Ask close direct shell, MCP, custom-tool, and resumed-delegation bypasses.
Remote Markdown images no longer fetch automatically, avatar responses are bounded,
quota origins are exact, clone credentials are stripped before remote use, and the
signed updater verifies and extracts one immutable open artifact from a private
scratch root. Regression coverage includes cross-workspace trust, invalid workflow
state, mutating shell forms, hostile scratch roots, pathname replacement after
verification, credential-bearing URLs, oversized bodies, and lookalike quota hosts.

The idle composer send control transitions between its disabled square and its
labelled action as a draft becomes sendable or is cleared; its transition can
reverse and Reduce Motion presents the final shape immediately.

Queue, run-status, approval and inline-question cards use the same floating
glass material as the composer, retaining their distinct edges and interactions.

Shared Settings and Usage boards use the page-scoped floating material: dark
boards can frost the workbench without wallpaper, while full-window wallpaper
and accessibility preferences continue to control the fallback.

The shared workbench geometry uses 6px compact controls and rows, 9px standard
controls, 12px cards and 16px dialogs (fixed; the Corner rounding setting was removed in #166). Floating surfaces use
soft shadows alongside their existing contrast-controlled frost and opaque
fallbacks; the semantic palette roles remain unchanged.

Settings → Appearance → Background offers **Desktop** (`WallpaperMode::Desktop`):
the window's background appearance becomes `Blurred`, so the OS compositor
frosts the desktop behind it (macOS `NSVisualEffectView`, Windows Acrylic), the
root paints no canvas, and panes take the full-window wallpaper's translucent
fills. With no picture to sample, `wallpaper::desktop_surfaces` floors every
text-bearing alpha at `Theme::floating_glass_alpha` (contrast held over both a
black and a white backdrop); Card opacity can only raise it. The window follows
the theme from `rebuild_theme_for` (deferred, so it never re-enters a window's
own update) and opens see-through when Desktop is on. Light palettes, Reduce
Transparency and Linux keep `SurfaceTreatment::OPAQUE` and an opaque window.
Modelled on monocode's window glass, but on Orbit's contrast floors instead of
fixed alphas. Unit tests cover the mode, the floors and the opaque fallbacks; the
see-through window itself has not been looked at in the running app.

The right pane is tabbed: **Review** first, then one tab per delegated child
the reader opens from its card ("Open in side pane"). The tabs (`right_dock.rs`)
ride on the session's `Transcript`, so parking a session parks them; whether
the pane is *shown* stays the app-wide `SidePane` flag, and only that click sets
it. Returning to a session therefore restores its tabs without popping a hidden
pane open — the rule upstream needed `aafd644f` to patch in. Review paints its
own strip through a `DockHeader` callback it asks on every paint; a subagent
tab paints in the same `dock_shell` frame (width, resize handle, card) with
`transcript_view::render_subagent_panel`: a header (state mark, name, status
badge, elapsed · tools · tokens · cost, a live "now" line), error and blocked
banners, then the full task, setup and tool badges, steps and report in inset
wells filled with `page_fill`, so they stay on the pane's glass. The steps and
report builders are shared with the card's inline log. The panel also streams
the child's reasoning (`thinkingText`, a tail ≤ 4,000 chars) and each step's
result (`output`, a head ≤ 1,200 chars, live from `tool_execution_update`):
optional `subagent/1` fields the bundled extension adds (additive per D-L),
display only, never sent to the parent model. This is
Orbit's own `subagent/1` delegation data, not upstream's dock, which reads the
third-party `pi-subagents` async protocol. Covered by model tests, controller
tests (open, hide-then-return, reload, close-last) and GPUI tests for the strip,
the subagent view and the card's button; not yet tried by hand in the app.

Transcript text selection counts its own clicks (Pi TUI's `getClickCount`:
same block, visual line and word within 500 ms, cycling 1 → 2 → 3 → 1)
instead of trusting GPUI's `click_count`. Two selects the word — UAX #29
segments merged across Pi's joiners `/ - + \ ~ _ .` — and three the wrapped
row from the text layout; drags after either snap to words or rows, and those
presses never resolve a link on release. Unit tests cover word segmentation,
the click cycle, drag snapping and past-the-end hits, and GPUI tests drive real
double- and triple-clicks; not yet tried by hand in the running app.

Settings → About stars Orbit's repository through the user's signed-in `gh`
(`gh api /user/starred/OWNER/REPO`; GET to read, PUT/DELETE on click). The
state is read once per launch when About is first shown; a missing or
signed-out `gh`, or a token without the scope, turns the same Star button into
a link that opens the repository in the browser. No token is collected by Orbit.
About itself is an identity header (mark, name, description, one
"Version · GPUI" line) over one board — Updates, the pi agent, and Source with
Star beside GitHub. Covered by unit tests for the slug, the 404 classification
and the state machine, plus a GPUI layout test for the header and the
Star/GitHub row; the live `gh` round trip and the page in the running app are
not yet manually verified.

The main window restores its frame and display from `~/.orbit-pi/window.json`,
with windowed, maximized, and native fullscreen as distinct states. Fullscreen
keeps the ordinary restore frame, so leaving it recovers the previous size;
older state files without a fullscreen flag remain compatible (#131).

Settings' Back control and section list are keyboard stops. The list's focus
ring moves separately from the open section: Up/Down and Home/End move the
cursor, and Enter or Space opens the focused section. Back or Escape returns
focus to the opener when it is still available.

Settings → Appearance → Background offers a **Dithered Hero** preset for a
chosen image: Hero only, Dither at 1× and 35% intensity, 40% image opacity, no
image blur, and a bottom fade. The separate **Bottom fade** preference works in
both Hero and Full window placements and blends into the live theme canvas at
paint time, without modifying the source image or changing wallpaper defaults.

A wallpaper with any artwork effect on (Dither, Halftone, ASCII or Scanlines,
at any strength, Hero or Full window) carries the new-task page's living
field: `backdrop::overlay` paints the same drifting tone, screened monogram
and sweeping light on a clear plane over the treated picture, at the picture's
own image opacity, printed through the effect's own `backdrop::Screen` —
ordered dither, halftone dots whose area is the tone, the ASCII ramp shared
with `wallpaper_fx`, or fixed scanlines whose thickness follows the tone
(they once rolled downward, which turned over a third of the plane per frame
and strobed; `no_screen_flickers_between_frames` now holds every screen under
3 % per frame) — at the effect's pixel size. It shares the
backdrop's clock and raster-ahead cache, and moves only on the new-task page
in a frontmost window; behind a chat, Settings or a feature page it holds the
frame it was showing, and Reduce Motion freezes it. The Settings preview shows
it at rest.

The field is deliberately light. It runs at 10 Hz, not 30: the field drifts so
slowly that at 30 Hz consecutive frames differed in 0.01–0.03 % of their pixels,
while each frame paid for a full raster, a texture upload and a whole-window
repaint with every frosted pane's backdrop blur. Both the new-task backdrop and
the wallpaper field also stop whenever the window is in the background
(`OrbitApp::tick_backdrop` reads `window_active`), dropping their clock and
asking for no repaint, and resume from the frame they held. Covered by unit
tests for the clear raster, every screen, which settings animate, the 10 Hz
beat and the background pause (each screen rasters within the cost of the
original dither); a manual pass in the
running app, including frame cost behind frosted panes, is still outstanding.

The component layer (D22) is complete: every element family has one owner in
`crates/orbit-pi/src/ui/`, surfaces import `crate::ui::prelude`, `app/helpers.rs`
is gone, and `scripts/check_ui_owners.py` guards ownership. The Explorer,
transcript and sidebar context menus work from the keyboard (Shift-F10 / the
Menu key opens a sidebar row's menu) and form-modal footer buttons are Tab
stops. Covered by focused unit and GPUI layout tests; a manual pass across
palettes in the running app is still outstanding.

Remote sessions (D20) ship agent-only SSH sessions: Settings → Remote hosts
(host list with Test / Edit / Remove, an add/edit form with `~/.ssh/config` alias
suggestions, an optional user and port, and a read-only **Test connection**),
per-host cards with an on/off switch and a ⋯ menu, a Machine selector on Providers & models (a host's providers read from its own `pi --list-models`; sign-in with pi's `/login` on the host), **Install pi** over ssh (pi's
installer, plus checksum-verified Node.js 22 when missing; no sudo), explained
failures (cloud root guard with **Use opc**, signals, pi missing), lookup through
the user's own login shell and pi's install folders,
host rows in the "Work in" picker (the new-task folder picker lists folders
only; the host config lives in Settings → Remote hosts), the host named on the folder chip and the
status bar, the unavailable state on Files / Worktrees, a read-only Git page that
shows the host's own repository facts for the session's folder (root, branch,
upstream and ahead/behind, changed files, last commit, `origin`; read over ssh
with `git_status_script`, nothing written on the host) and the same read's
branch in the same header/status-bar branch selector as local sessions
(refreshed at start and after each turn; the picker lists the host's branches
and switches or creates them there over ssh), Explorer and Review
closed with a toast, `@` mentions off, file attachments refused with the reason,
**Add a project to a server** (clone on the host with its own git, then the
ticked ignored files over ssh stdin; a host entry saved for the clone and
linked to the local project, so its sessions list inside that project's sidebar
group — any entry can be linked from the form's optional **Local project
folder**), the
guard and the workflow extension installed and the mode pinned to
Supervised on every launch (the access chip shows that and stays locked), the
Plan / Build / Ask mode synced to the host's store, and the default model
applied over RPC when the host's catalog has it, and each host's sessions listed in the sidebar (read from the host's own store over ssh, metadata only) and resumed there with `switch_session`. Covered by `orbit-rpc`'s fake-ssh transport
test, `remote_hosts`' store tests (surgical writes, unique names, alias
discovery) and the app's `remote_ui_tests`; not yet exercised against a real VPS
or in the GUI by hand.

Settings → Git Accounts provides the local-only, read-only Git diagnostics described in D16; it does not check remote access or manage credentials.

The third round of #131 refines the workbench around projects and models. A
session's **Delete session…** row is always listed — disabled with the reason
while pi's process still owns the file, instead of vanishing — and the macOS-only
**Reveal in Finder** row is gone. A project's ⋯ menu now separates **Delete
project** (drop the sidebar row; pi's sessions untouched) from **Delete project &
files** (a confirmation that names the folder, then `platform::trash_path`
off-thread, with the row removed only on success); `project_delete_refusal`
refuses a filesystem root, the home folder, a folder that contains another
project, and a folder a live task still works in, and its cases are covered in
`sidebar_remove_tests`. **New project…** (command palette) adds a folder to the sidebar with no pi session
(`OrbitApp::new_project` → `add_workspace`); since #166 it is also a
labelled button beside New session on the sidebar's own creation row under
search (`sidebar_create_actions`: a square icon-only New project leading, a labelled New session filling the rest, both Secondary); the search row
keeps search and the view menu. `Button` labels now truncate with an ellipsis
when their row squeezes them. #166 also adds a project display name (`WorkspaceMark::name`,
stored as `name` in `workspaces.json` beside icon and tint, edited from the
header menu's **Rename…** in `app/project_rename_ui.rs`): the group's identity
stays the folder label, so a rename moves no collapsed state, order or pin.
**Delete session…** is enabled for any session without a run in flight
(`session_delete_blocked`); confirming on the open session first hands the
window to a new task (which parks it), then drops the parked process before
removing the file. The session menu's **Branch from a message…** opens the
rewind picker, switching to the session first and waiting for its `get_state`
like Rename. A launch with no remembered folder uses `first_launch_workspace`:
the process cwd unless it is a filesystem root or inside the executable's
directory, otherwise the home folder. The **Work in** picker groups its rows
under two section labels painted inside the first row of each section
(`work_in_section_start`, so the row count `scroll_to_item` uses is
unchanged): **This device** and **Remote**, the latter carrying the single
Experimental tag. Remote always ends with `WorkInRow::AddRemoteHost`, which
opens Settings → Remote hosts with the add-host form (the form is painted by
Settings). Settings → Providers & models renders
connected and unconnected providers as rows on two grouped boards (the shared
`settings_group` surface, not a card grid). A row's head is one line — brand mark, name,
status, one truncated facts line, and on the right only the model count, the first
sign-in action while the provider is not connected, and a disclosure chevron. Clicking the
head opens the row's panel: every other action on a full-width wrapping bar (provider
buttons stop propagation so they never toggle the row), the sign-in progress card, and
the provider's models as a two-column, height-bounded list of click-to-select chips. The
panel opens by default for the session model's provider, whenever the search or Favorites
filter narrows the list, while a sign-in runs and while a removal is confirmed; a hand
toggle is kept per provider in `provider_models_open`. A selected chip means
the composer's picker offers that model, a click writes the hide-list
(`hidden_models`), the context window shows where pi reports one, and Hide all / Show all
sits beside the count. Each expanded list has a **Search models…** field matching model
ids and display names, case-insensitively. Search drafts are independent per provider/account
and survive folding the row; an empty result keeps the field visible. Local search only
filters chips: visibility counts and Show all / Hide all keep their section-wide scope.
The row's **Models**/**Configure** dialog edits the ids, endpoint
and key (the text field is the bulk-paste path; **Fetch models** adds the endpoint's ids to
it) and does not repeat the list.

Provider **accounts** (D21) ship as an **Accounts** panel in each provider's row
(add, rename, remove, and each account's sign-in state) and as one provider row per
account right after its provider, where that account signs in and manages its
models. All accounts are live in every session at once and appear in the model
picker as `<Provider> · <name>`. Covered by the account-store tests (aliases,
migration, sign-out on remove), the rendered Providers-page test, the bundled
extension tests and `contrib/orbit-accounts-extension` `node --test`. Not yet:
a manual GUI pass on the panel, accounts of `models.json` custom providers, and
removing the retired per-session routing plumbing.

Subagents (D18) ship the delegation tool, agent discovery, the three run modes, the caps, the
mode/ceiling pre-flight, child cost reporting and the delegation card (per-child rows with the
current step, the run's usage and mode, a blocked/refused banner, and — expanded — the child's real
steps and its report). The parent's working line names the child while it runs, and the turn header
counts delegations instead of "Ran 1 tool". Settings → Subagents manages the whole
library in-app: the list groups builtins and user files, and the detail column edits the capability
tier (which writes the `tools:` line and derives the child's ceiling), the workflow mode, the model,
the thinking level, the description and the prompt. Saves are surgical — only the five keys Orbit
owns change, and a file edited elsewhere is never clobbered without an explicit overwrite; a user
agent can be created, duplicated, renamed, disabled, deleted (Undo restores the exact bytes) and
reset to builtin. A builtin has no file of its own, so saving one writes the user override that
takes precedence; the form's primary button says Create for a draft and Save for anything that
already has a file, stays disabled until the form differs from what was loaded, and carries the
override path in its tooltip. Leaving the page or switching settings sections with unsaved edits
holds the navigation and names the destination it was going to before the edits are dropped. A child's side-pane tab is its live transcript (D24): the child's own pi
events, read from the extension's event log and painted with the chat's rows, under a one-line
status (state mark, status, elapsed); each tab wears the child's state mark, and two tabs for the
same agent are numbered. Without a readable local log the tab keeps the summary panel. Covered by
`node --test` (log module, the tee end to end), `subagent_log` unit tests and a GPUI test for the
transcript, the fallback and the numbered tabs; not yet tried by hand with a real delegation.

Permissions (D19) ship Settings → Permissions as one flat list, not an editor:
a scope row (every session / main sessions only / one subagent), then one row per
tool family carrying `Inherit · Allow · Ask · Deny`, the value that resolves to
(`→ Ask`), and its pattern rules indented beneath it (`rm *` → Deny) with an
inline form to add one. `Inherit` stores nothing, so the file only holds
decisions a person made. Under Full access the board is replaced by a statement
of what that mode does, because the mode short-circuits the table. The guard
enforces the same file on every tool call, live sessions included; a `deny`
blocks with a reason the model can act on, and an `ask` in a child (which has no
approval window) is refused.
`contrib/orbit-guard-extension/*.test.mjs` covers the engine and the entry point,
`permissions.rs` covers the store, and `app/permissions_ui_tests.rs` covers the
page. `docs/permissions.md` is the reference.

The Report a bug draft generator has a searchable model selector. It defaults to the active session model and saves an independent override in `~/.orbit-pi/bug-report-model.json`, leaving both the session and commit-message model unchanged.

Review and the Git page re-read a diff when a change or manual refresh arrives during an in-flight load; failures are surfaced rather than silently leaving an old snapshot. Bug reports with screenshots fall back to the browser and open the saved screenshot folder when `gh` rejects an upload before issue creation, including its repository-write-access preflight.

Where space permits, the Review toolbar shows the changed-file total beside the ±stats and previous / next file arrows with the reader's place among them (`12/187`). On narrower panes the total is shown in the navigator instead, and the navigator also folds away when space is tight. The arrows and the `n` / `p` shortcuts use the same step, measured from the file at the top of the diff rather than a tree selection scrolled away from, and both go inert at the first and last file.

Floating glass without a wallpaper: in dark palettes on macOS, every floating
layer built on `elevation_2` / `elevation_3` (menus, pickers, popovers,
tooltips, toasts, modals, the jump-to-latest button) frosts the workbench
behind it through the vendored backdrop pass at `FLOATING_GLASS_BLUR` (24 pt)
under a tint of `surface_elevated` at `Theme::floating_glass_alpha`: the lowest alpha
from `FLOATING_GLASS_MIN_ALPHA` (0.55) up at which the palette's primary and
secondary text keep their WCAG targets over both a black and a white backdrop
(0.62 for Onyx, up to 0.82 for Flexoki), computed from the live colours. `Theme::floating_glass_sigma` is the one rule: over a wallpaper it
defers to `glass_sigma` (so Glass panes still turns it off); light palettes,
renderers without the pass, and macOS Reduce Transparency / Increase Contrast
(read with Reduce Motion at launch and on window activation) keep the opaque
card. Structural panes frost only over a wallpaper; the composer also uses the
contrast-controlled floating material on the bare dark workbench.

Inspector glass: the transcript's cards (tool calls, thoughts, delegations,
questions, summaries) and the right-dock cards (session details, Review,
Explorer) frost under the same floating rule — so in dark palettes they are glass
even with no wallpaper. `SurfaceExt::inspector_card` frames a transcript card
(`Theme::inspector_fill` tint and frost — no top-edge highlight and no outline: the 1px border stays transparent, carrying only the
blur request); `SurfaceExt::dock_card` frames a dock, whose root paints
`Theme::dock_canvas` (clear while `Theme::dock_glass`); a tool call's
command/output plane is `Theme::inspector_well`, held at the code background's own
contrast floor. An expanded tool card takes `ElevationIndex::ElevatedSurface`'s
contact shadow while frosted. Over a wallpaper with Glass panes off, and on a hero
wallpaper for the docks, everything stays opaque. The renderer cost is one
backdrop pass per visible card; on-device frame-time measurement is outstanding.
Every builder of such a layer scopes its theme with `Theme::for_floating()`, so
the rows and controls inside take the ink washes (`hover_wash`, `pressed_wash`,
`selected_wash`) on the frost instead of the palette's opaque state steps; over a
wallpaper the same washes already apply everywhere. Faceless controls in `ui/`
(rows, tree rows, quiet chips, icon buttons, tabs) and the surfaces' own
highlighted / selected rows all read those washes; buttons and chips with an
opaque face keep the palette steps on their face. Info / success / warning
banners and the Usage skeleton use `page_fill`, so they are translucent on a glass
page.

Settings → Appearance → Theme is one compact row per choice: Light / Dark / System
as a segmented control, and Dark theme, Light theme and Accent color as the
standard Settings dropdown (search, keyboard, virtualized list). Palette options
lead with a small swatch (canvas, sidebar, accent, with the current accent
applied); accent options with a dot of the accent they would give the palette on
screen. The former picture cards and thumbnail galleries are gone.

Built-in backgrounds (`wallpaper_builtin.rs`): Settings → Appearance →
Background offers four rows of one-click backgrounds: Gradients (seven
generated gradients, rendered with GPUI's own `linear_gradient` math so the
thumbnail is the image), Nature, Anime and Japanese art (eight bundled 2560 px
JPEGs under `assets/wallpapers/`, ~5.5 MB, with 240 px thumbnails; Unsplash
License images and public-domain Hokusai / Hiroshige prints, credited in
`assets/wallpapers/CREDITS.md` and named in each tooltip). Picking one writes the
file into the managed wallpaper folder off the UI thread and imports it through
the ordinary wallpaper path, so placement, blur, effects, glass and
accent-from-image apply unchanged; it replaces the current image as a picked file
does. No network is involved.

Your backgrounds (`wallpaper_library.rs`): below the built-in rows, Settings →
Appearance → Background lists the pictures in `~/.orbit-pi/wallpapers/library/`
as the same tiles, wrapping across the row, with **Open folder** (creating the
folder first). Orbit only reads that folder: images there are never bundled,
uploaded or changed, which is what lets a picture licensed for use on the user's
own device (a Freepik download, say) be a background without Orbit distributing
it. Listing runs off the UI thread in two passes: the folder with any cached
thumbnails first, then the missing 240×150 thumbnails decoded into
`~/.orbit-pi/wallpapers/library-thumbs/` (named by a hash of name, size and
modification time; orphans pruned). It starts the first time Appearance is
shown, and a debounced watch on the folder, polled on the heartbeat, lists it
again when pictures come or go. Picking one imports it through the ordinary
wallpaper path. Because `include_dir!` embeds all of `assets/`, a test
(`the_bundled_folder_holds_only_catalogued_images`) fails if an image in
`assets/wallpapers/` is not a catalogued, credited built-in.

Glass pages over a full-window wallpaper: the feature pages (Git, Usage, Files,
Worktrees, the expanded terminal) and the dock cards (Review, Explorer, session
details) frame themselves with `SurfaceExt::page_card` — the panel radius, the
contrast-floored `card_fill(surface_secondary)` tint, a frost at `GLASS_BLUR`
and the hairline ring painted after the page. Each page root paints
`Theme::page_canvas` (transparent over glass; previously an opaque `background`
that hid the glass entirely), raised surfaces inside it — diff file headers, the
commit box, history detail cards, usage boards, table headers — use
`Theme::page_fill`, and the Review diff's sticky section / file headers frost
the rows under them (`page_sticky_frost`, falling back to the page's composited
appearance when Glass panes is off). `Theme::for_page` scopes all of it to
full-window mode: hero wallpaper is never behind a page, so those pages, and
every page without a wallpaper, resolve to their opaque palette fills exactly
as before. The terminal grid keeps its own opaque background (ANSI owns its
colours). Settings' nav column frosts like the session sidebar, and so do the
Skills and Subagents list columns (their page roots stay clear like every other
Settings section); wells nested inside Settings cards and the About banner take
`card_fill` tints rather than opaque slabs. The sidebar's sticky project header
frosts with the rail through `SurfaceExt::pane_sticky_frost` and keeps the opaque
surface when Glass is off, so rows never print through it. The reader's bubble
is `SurfaceExt::edgeless_card` (the panel corner, `Theme::glass_fill`, frost only
under glass, no hairline); `glass_fill` is also the composer's and the tool
card's code plane's fill. Every Settings modal dims with `scrim_modal`, the Git
page's in-page confirmations with `scrim`, the sidebar drag preview is an
`elevation_2` floating layer, and the composer's second send disc rests on
`overlay` like the controls around it. Inline errors in dialogs and forms are
`Banner::note` (the tone's own card tint, a Small glyph, error ink for an error and
secondary ink otherwise), as is the Git branch menu's dirty-worktree warning; the transcript's failed-turn
card and the Git page's operation-in-progress card are ordinary `Banner`s. The
MCP server switch and Settings' sidebar toggle are the shared `ui::switch`, MCP
tags are `Badge`s on a card-tinted face, the extension dialog's input is a
`TextField`, the open Settings section reads `selected_wash`, and the top-bar
chips' gradient stops are `Theme::header_chip_fill` / `header_chip_lift`.

Appearance's interface-font picker includes bundled Onest in Regular, Medium,
SemiBold, and Bold weights; it works offline without an OS font installation.
The default interface face and saved font preferences remain unchanged.

Done: streaming transcript + virtualization; markdown + highlighting; composer with steering, follow-ups, cancel, autocomplete, attachments; extension dialogs; diff/Review + Git page (outside a repository it says so and offers **Initialize Git repository**, a plain `git init` that keeps the user's `init.defaultBranch`) + GitHub issues/PRs (where `gh` is available); sessions (list/switch/new/delete/clone/cross-workspace, plus **rewind** — pi's `fork` at an earlier user message, which branches the conversation into a new session file, keeps the old one in the sidebar, and hands the forked-from prompt back to the composer) over an Orbit-owned project list (only folders the user added; removing one never touches pi) whose groups sort by activity, added date, name, or session count — or, from the view menu, one list under local-calendar day sections (**By date**: Pinned, Today, Yesterday, This week, This month, Older; `build_date_sidebar_rows`), with a **warm process pool** so re-opening a recent session is a resume, not a Node spawn; Git worktrees as first-class workspaces (`worktree.rs` + the Worktrees page: discovery, create from a new/existing branch, open/rename/move/lock/remove/prune/repair, configurable `.wt` root, optional async setup script with `ORBIT_ROOT_PATH`/`ORBIT_WORKTREE_PATH`, sessions rooted in and restored to the worktree — D14) plus **Work in** on the status bar in a repository and on the new-task page — in a session it lists only where the task runs (the current checkout, or its worktree); on the new-task page it lists the current checkout (and the active linked worktree, checked), **New worktree** (the Worktrees page's quick-create dialog), **Existing worktrees ›** — a flyout beside the picker with a search field over every linked worktree (branch over path; ↑/↓/Enter, Escape back; → opens it) — and every enabled remote host (`remote_hosts.rs`; hosts are read once when the picker opens, and picking one runs the same guarded launch as the new-task picker) — while the new-task page's folder selector answers only *which folder* (recent folders — the current one, then session folders newest-first, then sidebar projects, existing folders only, linked worktrees left to Work in — plus **Choose folder…**), so the two pickers never offer the same row; Work in also carries **Add this project to a server…** under the hosts, and host rows carry the **Experimental** tag; a worktree is marked with `icons/worktree.svg` (folder + branch), never the branch glyph — while the remaining worktree verbs (rename, move, lock, remove) and worktree creation (from a new or existing branch, including a remote ref checked out as a local tracking branch via `create_tracking_branch`, because `git worktree add <path> origin/feat` leaves a detached HEAD), opening a worktree folder with `git worktree repair`, and repository cloning all live on the Worktrees page (the Windows job-object clone-cancellation path is covered on the Windows CI leg; manual GUI verification remains); Explorer project panel (its own top-bar button; a two-line workspace header with New File / New Folder and a "…" menu for Refresh, Collapse All Folders and hidden files; an inset filter field; indent guides) + editable Files surface; a **session-details panel** in the right dock (the top bar's `ⓘ` chip toggles it: an inspector of Session / Environment / Model / Context / Activity / Usage / Changes / Details sections, every value read from live state — the branch watcher, `get_session_stats` context usage and session token/cost totals, the transcript's own prompt / reply / tool-call counts (`Transcript::activity_counts`), the working-tree counts — and none shown before it is known (no cache rows when nothing was cached, no Cost row without a cost table); rows take a minimum height that grows with the UI font, and the session id and file wrap in full in the code face instead of truncating; it takes the dock one at a time with Review and the Explorer, and no outside click dismisses it); integrated terminal (⌘J, with a full-width expand that opens it as a page with its own top-bar Back and title); usage, skills, plugins, models, providers, settings pages; transcript find; image lightbox; theming (dark/light/system with picture cards; thirteen palettes in one design system, chosen from Dark and Light thumbnail galleries; accent swatches; a Background board with an empty state, segmented placement and a separate Image adjustments board) + reduce-motion; localization (ten locales + System, D9); in-app signed updater + Version History (D17); notifications; open-in-editor; signed/notarizable macOS packaging + best-effort Windows/Linux bundles (D5); CI; access modes (a guard, not a sandbox), workflow modes (Plan/Build/Ask per D8), and the auto-title / quota extension bridges (D10); Zed design tokens (D11) with context menus, the tooltip, the extension dialog, the floating modal cards (provider usage / API-key / editor, update dialog, custom UI), the sidebar session / workspace rows, the transcript's message / card chrome, the settings section / group / row chrome, UI type on `TextSize` across every surface, corner radii on `Radius`, and one-shot UI motion on `Motion` roles migrated onto them.

Finder file URLs and Windows CF_HDROP lists take precedence over clipboard preview images: pasted image files enter the image queue, and other files enter the existing file-reference queue. On Linux, file URI lists exposed as clipboard text use the same queue; GPUI cannot currently read file-only clipboard offers. Plain path text stays text.

Attached images are compressed when they are queued (`attachments.rs`): the
longest edge is capped at 1400 px (never upscaled), opaque images are re-encoded
as JPEG with quality stepping down to keep the base64 payload within 512 KiB, and
images with transparency become lossless WebP. Images already under both caps pass
through untouched, and animated GIFs pass through so their frames survive. The
composer chip and bug-report screenshots both read the compressed bytes, and the
status line reports the before/after size when an image shrank.

The composer holds every queued item in one attachment tray (`attachments_row` in
`app/composer_ops.rs`): images, non-image files, and collapsed long pastes are the
same chip — a leading 20px slot (image thumbnail, or the file type's devicons mark drawn
bare, with no tinted square behind it, and centred by the glyph's ink because icon artwork
is not centred in its font cell; the type's short label on a tinted square fills the slot
when no Nerd Font is available), the name, one secondary fact (size or line count), and an
always-visible remove button — and a tray of two or more grows a
count header with a **Clear** action. Files do not ride the wire as bytes, because
pi's `prompt` payload carries text and images only; they fold into the prompt text on
send as `@path` mentions (`fold_attached_files` in `app/session.rs`), workspace-relative
when possible and backticked when the path contains whitespace, so the agent opens
them with its own tools. The sent user message reads that block back out of its own
text (`split_attached_files`) and renders it as the same chips under the bubble, which
is why a reloaded session shows the same thing as the live send — the presentation is
derived from the message, not from local-only metadata. All three kinds share the
eight-item `MAX_ATTACHMENTS` budget, which the composer's send button, the drain paths,
and the pickers all read through one predicate.

Session metadata is cached per file (`sessions.rs`) keyed on the file's
`(mtime, length)`, so a turn settle or watcher reload re-reads only the session
files that actually grew instead of rescanning the whole store; a reload that
changed nothing is skipped. The transcript's tok/s readout is labeled `avg`
because it averages output tokens over the whole request (prefill plus decode).
The navigation rail's user-turn index is likewise cached in `Transcript` and
rebuilt only when the message list changes, and debug builds log one local
`[orbit:perf]` line per settle (window, cache, prompt, output, duration, rate).

Workspace groups can be reordered (issue #143). Dragging a project header
carries a chip with the project's name, paints an insertion line on the header a
release would use (targets come from the list's own measured row bounds, so a
scrolled or virtualized list resolves correctly), and writes the new order on
the drop; the header menu's **Move up** / **Move down** rows (listed disabled at
an edge, with the reason) and ⌥↑/⌥↓ while the sidebar has keyboard focus run the
same `move_project_group`. A move is always applied to the order the sidebar is
showing — that displayed order is pinned into `workspaces.json` first, so a drag
can never contradict what the user saw — and the first move pins
`workspace_sort` to Manual (with a toast) rather than being silently re-sorted
away. `sidebar_sort_tests` covers the reorder algebra, the displayed-order pin
and the edge no-op; the pointer drag itself has not yet been checked in the GUI
by hand.

Sessions inside a project group can be rearranged the same way, as long as the
sidebar is showing the grouped list. A session row carries its own drag (a chip
with the session title, an insertion line on the landing row) and only accepts a
drop from its own project — a session never leaves its folder. `⌥↑`/`⌥↓` and the
row menu's **Move up** / **Move down** run the same `move_session`, which pins
that group's current display order into `session_order` (one ordered path list
per project label, `~/.orbit-pi/session-order.json`) and persists it. Ordering is
then: pinned sessions, then sessions nobody has arranged (recency — a task
created after the group was arranged still appears at the top), then the arranged
block. Flat, Recent and search lists keep pure recency and show no session move
affordance. `session_order`'s store tests, `sidebar_sort_tests` (the rank sort,
the reorder algebra, a persisted move, a refused cross-project move) and
`sidebar_drag_tests` (the insert line paints only on the landing row) cover it;
the drag gesture is unverified in the GUI.

The sidebar keeps the title, prompt preview, status/age, branch/divergence,
provider/model marks, and pin/archive indicators. Flat mode and search name each
row's workspace; grouped mode names it once in the project header, and a row there
shows the location line only when its folder is not the header's — a branch belongs
to a folder, so repeating it down one group told the reader nothing. Tinted project
marks and count badges establish hierarchy, semantic status washes distinguish
outcomes, and primary-ink titles lead secondary previews and muted metadata.
The selected row is marked by `surface_selected` alone — a neutral step of the
palette's own ramp under every accent — with no outline, no "current" badge and no
heavier title; the keyboard cursor adds a full focus outline even when that row
is selected.
Preview and branch tooltips expose truncated text without widening the
sidebar, while the session title stays a single visible line (its row tooltip
names the keyboard shortcut instead). Spacing and mark geometry follow the
existing theme tokens, and the list remains virtualized with the same session
data and action handlers.
The sidebar view menu's **Show first message** toggle hides or restores the preview
in both grouped and flat lists. It defaults to hidden — a grouped row is one 28px
line until the preview or a differing location grows it — and persists as
`show_first_message` in `workspaces.json`; missing or invalid values keep previews
hidden. Toggling remeasures rows while retaining the logical scroll anchor, and
never edits session content or hides other metadata. When enabled, the message line
shows the whole prompt — pi titles a session from that message, so the two
can start alike, but the title is capped at 80 characters while a row shows
about thirty, so trimming the message to a "remainder" hid the end of the ask
instead of de-duplicating it. Branch metadata is a
per-project cache (`OrbitApp::sidebar_branches`), filled off-thread by
`git branch --show-current` + `rev-list --left-right` at launch and whenever the
project list or the current checkout changes, so rendering a row never spawns
git; a project that is not a repository, or is on a detached HEAD, has no branch
item rather than an empty slot, and `↑n`/`↓n` appear only for non-zero counts
(`divergence_labels`). Row actions are the right-click context menu's job — the
session row carries no hover-revealed `…`, which is why the title owns the full
line width and why the keyboard/palette delete path anchors its confirmation to
the row itself.

The session hover/keyboard preview card groups title and optional prompt summary,
then folder/workspace and its full monospace path, with branch and divergence on
a separate row with the same icon/text columns (no nested branch indent).
All provider/model labels wrap in full; the card uses a definite viewport-derived
width and maximum height with vertical scrolling instead of hiding metadata.
The optional Open shortcut is a quiet divided footer. Compact grouping and explicit
1.3 line height retain readability. Shared floating elevation, the 500 ms opening
delay, cached data and keyboard preview path remain; a 200 ms leave grace permits
pointer handoff into the card for scrolling, including a keyboard-opened preview.
No I/O is added to rendering. Native screen-reader semantics, keyboard-only scrolling
of preview contents and on-device visual verification remain unverified.

Session rows show execution status independently of read/unread state. The same
`SessionExecution` reducer consumes the foreground and parked sessions' real pi
events: Running, Waiting for approval/input, Retrying, Compacting, Completed,
Failed, Stopped, Disconnected, or Idle when no outcome is known. Only
`agent_settled` completes a run; `agent_end` may still lead to retries or queued
work. **A stop the user asked for is the one local exception**: `abort` is only a
request, and a pi whose event loop is stuck never reads it, so an unanswered stop
is given a grace period and then escalated in Orbit — the turn is closed and the
process replaced, instead of Stop being a dead button. The request stays a
request (pi's own settle still wins the race); it just cannot hang forever.
The RPC parser preserves the settle's `aborted` bit, including stops before
an assistant message exists. Outcomes survive reading, switching, and warm-process
reaping for this app launch (`session_outcomes`); historical and externally written
sessions carry no status label until observed, rather than guessing from old files. Tool errors alone do not fail the session.

Native approval/dialog/questionnaire surfaces park with their session, including
partially entered answers, and background requests wait instead of being denied.
Opening the session restores its surface; waiting sessions remain busy and visible
under collapsed project groups. Custom terminal surfaces still cancel on departure.

Session rows spend exactly one glyph on execution state, in a fixed 16px **status
lane** at the leading edge of the row's state line (`status_lane`, `status_lane_icon`,
`status_lane_ink`, `status_lane_words`): accent spinner while running, amber lock for
an approval gate or chat glyph for a question, green check for a read result, green
eye for an unread one, red cross for a failure or a dead process, mute stop for an
aborted run, and no glyph at all for Idle / Unknown (the lane still reserves its
width). Only the spinner animates. Blocked, unread, and failed states add words in
place of the age, so colour is never the only signal; the tooltip names the rest.

A turn that settles while the user is not reading it leaves a separate unread mark on
its row (`OrbitApp::awaiting_review`) — a green eye with "Needs review", or a red cross
with a small corner dot when the turn failed. The execution glyph underneath
distinguishes Completed from Failed and remains after the mark is spent, so reading a
result costs the mark, not the outcome. Only the mark clears when the session is
genuinely in front of the user — `transcript_in_front` (window frontmost, and no
full-page surface covering the chat area: Files, Git, Usage, Worktrees, or a
full-width Review pane, the same rule the renderer uses to hide the chat column),
checked once on the heartbeat rather than from a hook on each path that can bring a
session into view. That single rule is what keeps the distinction honest: selecting a
row in a background window does not spend the mark, and a settle behind a full-page
surface or another app still leaves one. Marks are memory-only — a cold start does
not guess what was read elsewhere — and a marked row joins the open, running, and
pinned rows in staying visible under a collapsed project group, so the result cannot
scroll out of reach before it is seen.

Workspace-list and last-workspace persistence use isolated, self-cleaning
per-thread directories under tests. Simulated session switches must never add
fixture folders to the user's real `~/.orbit-pi/workspaces.json` or replace the
last workspace.

During a run the composer footer shows Queue and Steer shortcuts in the same
platform-aware keycaps as the sidebar. Their order and chords follow the chosen
Enter behavior; the hints stay hidden while idle.

Settings uses the same UI type and button scale as the rest of the workbench:
readable page and section headings, regular-sized row descriptions, 32px selects,
and neutral selected navigation. Navigation rows keep their height and scroll
instead of compressing in short windows.

Holding ⌘ (Ctrl on Windows/Linux) reveals numbered keycaps beside the first
nine visible sidebar sessions. The hints follow the same filtered, sorted row
order as the session-switching shortcuts and disappear on release or blur.

The empty sidebar session search shows **⌘P / Ctrl+P** as compact grouped
keycaps for the separate command palette, with a tooltip naming that action.
**⌘⇧F / Ctrl+Shift+F** focuses the session search itself; **⌘P / ⌘K** open
the command palette. The footer no longer duplicates a Search icon.

Every icon-only button carries the app's native tooltip — a localized label, usually
with its shortcut appended (the send / stop button set the register) — across the top
bar, sidebar, transcript copy / diff controls, composer add button, window caption
buttons, Explorer, Review pane, Git, Usage, terminal, settings, and the picker / dialog
controls, so no glyph is a guess. A button that already shows its label does not repeat
it on hover: New Task, the Git tabs, and a widened access chip keep their visible text
as the label, while truncated text and icon-only controls keep a tooltip.

The top bar's dirty-tree `+N −N` chip opens the Git page on **Changes**, so the
counts reveal the files they name instead of opening Review; the plain
side-panel glyph still toggles the Review dock.

Opening a commit on **History** or **Graph** shows one quiet card: the message, a
column of labeled facts (Author and, when different, Committer, each with a relative
time and the exact local time on hover; the short hash with copy / open-on-GitHub;
parents), then a **GitHub** section and the changed files with their totals. The
GitHub section is one `gh api graphql` round trip per opened commit (`gh::commit_github`),
asked only for a GitHub `origin` with a signed-in `gh` and re-asked on every open so
checks stay current (the previous answer keeps painting meanwhile). It lists each pull
request GitHub associates with the commit — state, title (opens it on the Pull requests
tab), who opened it and who merged it — and the CI checks as a tally with only failing
and running checks listed by name. Git records no "pushed by"; the merger on GitHub is
the closest true fact and is labeled as such. A commit GitHub has not seen shows "Not in
a pull request" / "No checks"; a failed request says so with the error on hover.

The Git page's **Issues** and **Pull requests** tabs gained a **Generate** action: the
**New issue** and **New pull request** forms take optional free-form notes, then draft a
title and Markdown body with the same one-shot, tool-free `pi -p` call the commit-message
button uses (`issue_message.rs`), falling back to a local heuristic built from the branch's
commits and changed files when pi is unavailable. The forms also read the repository's own
templates (`gh_templates.rs`): a lone issue/PR template applies automatically, several are
chosen from a **Template** picker, the body is prefilled with the template's skeleton (YAML
issue forms are flattened to Markdown), the template's title prefix and labels are applied,
and the generator is told to fill the template faithfully. Those tabs were also tightened
for scanning — labels collapse to `+N`, every row ends in a fixed relative-time column, and
the PR list drops its duplicated state pill — and the PR detail now shares the issue
detail's reading-column + metadata-rail layout (review, checks, changes, branches,
timeline).

The commit bar's **Generate** button has its own model picker (`commit_model.rs` +
the chip beside Generate). Commit messages are a short, cheap task, so they do not
have to run on the session model; the chip opens a searchable catalog with each
provider's mark, and its first row always restores **Active session model**. The pick
persists to `~/.orbit-pi/commit-model.json` (Orbit-owned, like the auto-title slot) and
the commit prompt is then sized from the picked model's own context window, so a small
local model gets a smaller diff budget instead of an overflowing prompt. The provider
and model id are passed straight to the one-shot `pi -p` call, so a custom provider
that pi resolves through an extension still works even when its catalog entry has not
arrived; issue/PR drafts keep using the session model.

The in-app bug report (Settings → Report a bug) picks an issue type (bug /
feature / other, which sets the GitHub label and the generated section
headings), takes free-form context at the top, and drafts the issue title and
description from it with the selected model (the active session model by default) — the same one-shot, tool-free
`pi -p` call the Git page's generator uses (`issue_message.rs`), with a
kind-aware prompt that returns a heading-free description so the form supplies
the section headings; when pi is unavailable the draft falls back to the notes
themselves. Screenshots are supported too: they upload with
`gh issue create --attach` when `gh` supports the upload and the account has permission (Orbit writes them to a temp
directory first). Otherwise, when the CLI reports a pre-creation limitation or is signed out, GitHub's prefilled web form and the saved image folder open for the user to drag the images in.
Filing goes to `imrj05/orbit` with the machine's build details attached — Orbit
version, app ID (`dev.orbit.pi`), OS and architecture, install kind, pi CLI
version, and the signed-in `gh` account — through `gh issue create --repo` when
the CLI is installed and authenticated, falling back to GitHub's new-issue page
with the title, body, and label prefilled. `gh` keeps ownership of the token:
Orbit still never stores one (`bug_report.rs`).

Settings → Privacy pairs session retention with the guard's remembered tool
approvals.

Composer sending is configurable in Settings → Agent → Behavior: Enter queues a follow-up
by default or steers the running task, with Alt/Option+Enter selecting the opposite mode.
The explicit ⌘⇧Enter / Ctrl+Shift+Enter steering shortcut stays fixed. The choice is a
live, global Orbit UI preference (`composer_send_mode` in `~/.orbit-pi/ui.json`), not a
pi setting; it affects future sends only, never an existing queue. Idle sends remain
normal prompts. Compact Send/Steer/Queue hints sit on the right side of the composer
footer, immediately left of the context-window indicator, without a newline hint.
These persistent hints, button tooltips, and the Shortcuts reference track the
preference, with dedicated chat-only shortcut context to protect other fields.
Non-blocking optional model questions remain outside this change.

A message already queued can be changed before pi delivers it (issue #79). The queue
bar gives each pending steer or follow-up its own **Edit** (an inline field on the row,
⏎ saves, Esc cancels) and **Send now**. pi's protocol has no per-message queue command —
`queue_update` reports bare strings and `clear_queue` drops everything — so either action
rebuilds the queue: `clear_queue`, then re-queue every survivor in its original lane and
order. "Send now" re-queues that message as the *first* steering message, which is pi's
earliest delivery point short of aborting the run; the leading steer therefore offers no
Send now, since it is already next. The mirror is updated optimistically and the
`clear_queue` echo is skipped for that one rebuild, so the bar never blinks through the
empty state; `queue_update` stays authoritative and re-anchors or closes an open field
(`app/session.rs`, `app/view.rs`).

Compaction and branch summaries are visible in the transcript: pi's
`compactionSummary` / `branchSummary` context messages now render as boundary
cards ("Compaction — Compacted from N tokens", click to expand the summary
markdown) instead of being filtered out, and a successful `compaction_end`
re-reads `get_messages` so the live chat adopts pi's compacted projection
(summary + kept entries) instead of holding the pre-compaction history until
the session is reopened.

Agent call details — an activity group or a tool card the reader has not opened
themselves — default to following the run: open while the turn streams, closed
once it settles. Settings → Agent → Agent call details can pin them always
expanded or always collapsed instead. The mode is a global Orbit UI preference
(`~/.orbit-pi/agent-calls.json`, `agent_call_display.rs`), not a pi setting; an
explicit per-group or per-call toggle always wins, so the setting only moves the
default and never fights the reader.

The sessions sidebar reads as three levels, not one: the "Projects" section
label in muted 12px, a project group header in the accent's hue at body-ink
brightness — or in the project's own mark tint, when the user picked one — and
session titles a step below at secondary. Before this a header and the rows
under it shared the same ink at the same size, so a long list of projects and
sessions was a wall of identically colored lines. Project names keep the body
ink's lightness and vary only in hue and saturation (`workspace_mark::label_ink`,
`name_ink`): that is what keeps a tinted name above the AA floor in every
palette, since several curated tints are fill/glyph colors too dark to read as
text. A tint the palette collapsed onto body ink falls back to the default hue.

The keyboard layer is command-registry driven (`commands.rs`, D13): every app
command declares its chord once, and the command palette, Settings →
Shortcuts, and tooltips all render from that table. The palette gained session
rename/pin/clone/delete, model and thinking pickers (⌘⇧M / ⌘⇧T), Review
(⌘⇧R), and the shortcut reference (⌘/). Session management is number- and
cycle-addressable: ⌘1…⌘9 open the Nth session in the sidebar's visible order
and Ctrl+Tab / Ctrl+Shift+Tab cycle sessions (the Git page tabs moved to
⌘⌥1…⌘⌥6 to free the numbers). Hover hints come from the same registry
(`commands::tooltip`), so the model/thinking chips, New Task, Search/Usage
rows, top-bar toggles, and Git tabs name their chord on hover — and a session
row shows its ⌘1…⌘9 slot. The Review pane is fully keyboard navigable —
Tab / Shift-Tab traverse the workbench's focus stops (composer, sidebar,
review tree, terminal, Explorer), the tree takes arrows and Enter, and
`n`/`p` jump changed files, `[`/`]` hunks, `e`/`c` expand or collapse every
file, Escape returns to the chat; the composer's autocomplete and the terminal
keep Tab for themselves. `secondary` chords replaced the macOS-only `cmd-`
bindings (Editor clipboard/undo/save, Files close, Git tabs), so Windows and
Linux get the same shortcuts, and the dead ⌘. abort chord was fixed (gpui's key
name is `.`, not `period`).

The Explorer builds its tree off the UI thread and in linear time. The walk was
already on the background executor, but `TreeIndex::build` — one insert per entry,
then a sort per directory — ran inside the `update` that published the result,
so it ran on the main thread; and each insert scanned the siblings already there,
which made a folder holding many files quadratic. 20 000 files in one directory
cost ~1.5 s of the main thread and froze the window. The build now happens in the
same background task as the walk, with the UI thread only storing the finished
index. The insert is linear because the snapshot is sorted by *path component*
before it is walked, so a sibling that already exists is always the last child:
the key is the path with `/` replaced by NUL, which cannot occur in a path, so
ordinary byte order becomes component order (`/` otherwise sorts after `-` and
`.`, putting `a/b-c` before `a/b/d`). A workspace switch or the hidden-files
toggle now bumps a walk generation, and a superseded walk hands the `loading`
slot to the request it superseded instead of dropping it — the panel could
previously sit on a spinner until another event arrived. `render` no longer
re-flattens the whole tree every frame; the filter observer and the row-changing
events already do that. Measured in one process, best of seven: 1.568 s → 27.5 ms
on a single 20 000-file directory (57×) and 161 ms → 42 ms on a realistic 22 000-
entry tree (3.9×). Covered by `tree.rs` tests for the insert order and the
directory/file tie, and by `panel.rs` tests for the asynchronous publish and the
superseded walk.

Open: migrating the remaining surfaces (the settings page's toolbars / cards / controls, transcript inner content, the modal bodies' inner text, the remaining sub-10px / 17px+ type, and off-scale radii) onto the D11 tokens; drawn scrollbars (gpui 0.2.2 draws none); on-device scroll-perf measurement; stream veil + an explicit ≤8.3 Hz streaming commit pipeline; screen-reader labeling (gpui 0.2.2 exposes no accessibility tree); richer per-tool renderers (bash/thinking are dedicated, the rest generic).

## Non-goals (explicitly out of scope)

- Any web technology in the UI — no webview surfaces as UI, no React, no HTML/CSS
- Multi-provider support à la Waku (pi only; Oh My Pi *may* come later as a second RPC flavor)
- Always-latest GPUI (the app is pinned, D4)
- Fabricating permission modes, diagrams, or perf claims the protocol/renderer can't deliver

## Phases — outcome (the original plan, kept for the record)

The staged plan below is history: P0–P4 shipped and P5/P6 are partial. The GPUI app is the only
app (D3), so this is a status board rather than a forward schedule. `cargo run -p orbit-pi` is
the product; the per-item detail lives in AGENTS.md.

| Phase | Deliverable | Status |
|---|---|---|
| P0 | Transport + workspace + process spawner + RPC client + first live round-trip | ✅ Shipped |
| P1 | Shell, theme, fonts, sidebar sessions | ✅ Shipped |
| P2 | Data layer: serde models, sessions/catalog/workbench clients | ✅ Shipped |
| P3 | Core chat: transcript, streaming, markdown, composer, tools, diff | ✅ Shipped (stream veil + explicit ≤8.3 Hz pipeline open) |
| P4 | Workbench pages + settings persistence | ✅ Shipped (plus Explorer/Files, terminal, Git/GitHub, localization, updater) |
| P5 | Mermaid + a11y | ◐ Native flowchart rendering + code-block fallback for the rest + reduce-motion shipped; other diagram types, screen-reader labeling open; focus rings are deliberately not painted (keyboard focus still moves) |
| P6 | Tests, perf harness, packaging/notarization, CI | ◐ Unit/live tests, 10k perf test, signed `.app`/DMG, CI shipped; on-device scroll benchmark + streaming edge coverage open |

Transcript Markdown export is available from each settled response's footer and,
for the visible projection, from the command palette; it excludes thinking, tool
output and image data and refuses to overwrite existing files. Manual GUI
verification remains open (`docs/plans/transcript-actions.md`).

Open backlog (detail in AGENTS.md): on-device scroll-perf measurement,
stream veil, explicit streaming commit pipeline, screen-reader labeling, richer
per-tool renderers, Windows/Linux native polish.

## Top risks (with mitigations)

1. **P3 scope** — *resolved:* was the dominant estimate (25–40 days); it shipped by keeping the
   Waku pattern map (AGENTS.md), one-StyledText-per-block markdown from day one, and tool
   renderers by spec.
2. **Tool approval protocol gap (D1 residual)** — resolved: pi exposes no native per-tool permission in RPC mode, so Orbit enforces access modes through a bundled `tool_call` extension that prompts via `ctx.ui.select` (Allow once / Always allow this tool / Deny), rendered natively as an inline bar. Settings → Permissions adds per-command and per-agent rules over the same guard (D19), so "`bash` may run but `rm` asks" no longer needs a mode change.
3. **Composer editor** — *resolved:* the multi-line `ComposerInput` (`EntityInputHandler`) has
   shipped, wrapping, auto-growing, and scrolling. Reuse it for new text controls rather than
   introducing another editor abstraction.
4. **Mermaid coverage** — a native renderer covers flowcharts only; every other diagram type
   keeps the honest code-block fallback (D2), so no fence ever draws a partial graph.
5. **GPUI churn** — pinned 0.2.2 (D4); upgrade = its own task with the registry source as docs.
6. **Velocity** — Rust UI iteration is 3–5× slower than React hot-reload; the estimates assume
   this, not hope.
