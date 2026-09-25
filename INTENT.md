# INTENT.md — Decisions, rationale, and plan

This file captures **why** Orbit was rebuilt as a native app and the decisions that shape it.
It is the source of truth for architecture choices; when AGENT.md or README.md conflict with
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
**Choice:** code-block fallback first; no diagram rendering in the P0–P3 core. Later, evaluate
(a) graphviz DOT for flowchart/sequence, or (b) a hidden native WKWebView used solely as a
**rasterizer** (render → snapshot image → GPU image; the Waku `browser.rs` pattern), or
(c) a custom GPUI renderer (only if we own enough of the diagram types).
**Reason:** no good native Rust Mermaid exists; a full custom renderer is 2–3 weeks and still
won't cover gantt/pie. This is the one accepted feature regression in the migration. Never
present a diagram that isn't rendered — render a copyable code block instead.

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
the write tools with `pi.setActiveTools`, gate `bash` to a read-only allowlist on
`tool_call`, and inject hidden guidance on `before_agent_start`; Build is the neutral
default. The mode is also written into the session as an `orbit:workflow` entry so
it survives resume and branch.

**Rationale:** pi exposes no RPC command for active tools or the system prompt, so
extension hooks are the only honest mechanism — the same call D1 made for access
modes. Per-session (not global like `access.json`) because scope is a property of
the task, and up to six parked sessions run at once. Reading the store fresh on every
hook re-arms a live session with no restart.

**Residual:** this is a guard, not a sandbox (pi ships none).

### D9 — Localization: `en.yml` is the only hand-edited locale
**Choice:** all user-facing copy goes through the crate-root `tr!` / `tr_cow!` macros
(`crates/orbit-pi/src/i18n.rs`). `locales/en.yml` is the source of truth; the other nine locale
files are generated from the `scripts/i18n_glossary*.py` tables by `scripts/gen_locales.py`, and
`cargo test -p orbit-pi i18n` guards completeness. Ships English, 简体中文, 日本語, 한국어,
Español, Français, Deutsch, Português do Brasil, Русский, Italiano — plus `System`, which walks
the OS preferred-language list and picks the first shipped language.
**Rationale:** ten hand-maintained locale files drift; generated files with an English fallback
and a completeness test keep them honest without blocking releases. Long-lived fields resolve
placeholders from keys at paint time, so a language change is live (including the native menu
bar) without rebuilding them. **Consequence:** new copy adds an `en.yml` key and a glossary
entry — hard-coded English in a render path is a bug.

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
**Choice:** `crates/orbit-pi/src/theme/tokens.rs` is a one-to-one port of Zed's `ui` crate sizing
tokens: `DynamicSpacing` (Base00–Base48 × three densities), `TextSize` / `HeadlineSize` /
`BufferLineHeight`, `IconSize`, `ButtonSize`, `ListItemSpacing`, the rounded scale, `ElevationIndex`
(Zed's exact shadow stacks) with the `elevation_1/2/3` `StyledExt` helpers, `AnimationDuration`,
and the per-component metrics (list item, list, popover, context menu, modal, tooltip, input
field, scrollbar). Names and numbers follow Zed's source so any value is checkable against it.
Zed's rem (its UI font size, 16px by default) maps onto Orbit's 14px-authored UI font like
`Theme::ui_px`, so every token equals Zed's default pixel value at Orbit's defaults and scales
with the UI font size setting. Zed's Compact / Default / Comfortable densities are anchored at
Spacing Density 80 / 100 / 120 % (exact at the anchors, interpolated between). Colors stay
Orbit's semantic palette — Zed's token set is sizing, elevation, and motion only.
**Rationale:** the UI audit found no shared sizing layer: ~1,200 per-call-site px literals, 17
distinct text sizes, 12 radii, and chrome heights that did not follow the UI font size. Zed is
the canonical GPUI design system, and porting its tokens (rather than inventing a scale) gives
every surface one vocabulary with a proven precedent. **Consequence:** new chrome sizes through
`tokens` instead of literals; surfaces migrate one at a time. Context menus (sidebar session /
workspace, Explorer, Git sync / branch / file), the tooltip, and the extension dialog are on
the tokens, as are the floating modals (provider usage / API-key / provider editor, the update
dialog, and the custom-UI card) via `elevation_3` and the `modal` metrics; the legacy heavier
`popover_shadow` is gone. Sidebar session / workspace rows use Zed's list-item radius and the
`TextSize` / `DynamicSpacing` scales, as do the transcript's message rows, user bubbles, tool /
question card shells, error strips, the usage footer, and the settings section / group / row
chrome and keycap chips. The ad-hoc `Theme::space` (density-only) helper is gone: every spacing
site now resolves through `DynamicSpacing`, so chrome tracks the UI font size as well as
density. UI type is on the `TextSize` scale across every surface (471 of 517 `.text_size` sites);
what remains is sub-10px badge glyphs, 17px+ display headings, and editor / code text. Corner
radii are on the `Radius` scale: gpui's fixed `rounded_sm/md/lg/xl` helpers and every on-scale
`rounded(px(N))` are gone (only 23 off-scale bespoke radii remain). One-shot transitions use
`AnimationDuration`; the looping affordances (spinner, shimmer, streaming pulses) and feedback
timers keep their own cadences, which the three-value scale does not cover. The settings page's
toolbars, cards, and controls, and the transcript's inner content (activity bodies, thinking,
edit diffs, detail sections) still carry bespoke layout px.

## The feature parity contract

Everything below must behave identically in the GPUI app (against the pi CLI) as it does in the
legacy app today:

1. Chat sessions — prompt composer, model selection, thinking-effort, steer/cancel/abort, streaming
2. Sessions grouped by project, persistent, reopenable, cross-workspace
3. Model catalog from pi's own list; thinking levels per model
4. Tool activity (bash, edit, todo, plan, search, mcp, thinking, question) + approval dialogs
5. Git diff panel + file-change blocks
6. Workbench pages — usage (charts + activity heatmap), skills, plugins, models, providers
   (CRUD + OAuth/quota), settings; Explorer + Files editor, integrated terminal, Git/GitHub,
   and the in-app updater with Version History
7. Markdown — GFM, syntax-highlighted code, copy-able code blocks, image lightbox
8. Theming — dark/light palettes, accent, font, density, reduce-motion. Persisted to **Orbit's own store** (`~/.orbit-pi/{theme,ui.json,fonts.json}`), not pi's settings: theme/UI state is Orbit's, and writing it into pi's config would fork pi's own source of truth. (Revised from "persisted to pi's settings".)
9. Attachments — files read to base64 images for prompts
10. macOS native expectations — traffic lights, a native menu bar (Orbit/File/Edit/View, rebuilt
    on language change), native dialogs, keyboard operability. Orbit's actions are also exposed
    as keyboard/mouse surfaces (⌘P palette, shortcuts), not only through menus.

### Implementation status (living)

Done: streaming transcript + virtualization; markdown + highlighting; composer with steering, follow-ups, cancel, autocomplete, attachments; extension dialogs; diff/Review + Git page + GitHub issues/PRs (where `gh` is available); sessions (list/switch/new/delete/clone/cross-workspace) over an Orbit-owned project list (only folders the user added; removing one never touches pi) with a **warm process pool** so re-opening a recent session is a resume, not a Node spawn; Explorer project panel + editable Files surface; integrated terminal (⌘J); usage, skills, plugins, models, providers, settings pages; transcript find; image lightbox; theming (dark/light/system, 42 palettes) + reduce-motion; localization (ten locales + System, D9); in-app signed updater + Version History; notifications; open-in-editor; signed/notarizable macOS packaging + best-effort Windows/Linux bundles (D5); CI; AI review agent (a read-only reviewer over the selected change set or the whole project that renders findings in the Review pane, on its own Ask-mode process); access modes (a guard, not a sandbox), workflow modes (Plan/Build/Ask per D8), and the auto-title / quota extension bridges (D10); Zed design tokens (D11) with context menus, the tooltip, the extension dialog, the floating modal cards (provider usage / API-key / editor, update dialog, custom UI), the sidebar session / workspace rows, the transcript's message / card chrome, the settings section / group / row chrome, UI type on `TextSize` across every surface, corner radii on `Radius`, and one-shot motion on `AnimationDuration` migrated onto them.

Open: migrating the remaining surfaces (the settings page's toolbars / cards / controls, transcript inner content, the modal bodies' inner text, the remaining sub-10px / 17px+ type, and off-scale radii) onto the D11 tokens; drawn scrollbars (gpui 0.2.2 draws none); conversation **fork/rewind** (clone exists; rewind needs entry ids); on-device scroll-perf measurement; stream veil + an explicit ≤8.3 Hz streaming commit pipeline; focus rings / screen-reader labeling; richer per-tool renderers (bash/thinking are dedicated, the rest generic). An "Auto" AI reviewer awaits a pi reviewer API.

## Non-goals (explicitly out of scope)

- Any web technology in the UI — no webview surfaces as UI, no React, no HTML/CSS
- Multi-provider support à la Waku (pi only; Oh My Pi *may* come later as a second RPC flavor)
- Always-latest GPUI (the app is pinned, D4)
- Fabricating permission modes, diagrams, or perf claims the protocol/renderer can't deliver

## Phases — outcome (the original plan, kept for the record)

The staged plan below is history: P0–P4 shipped and P5/P6 are partial. The GPUI app is the only
app (D3), so this is a status board rather than a forward schedule. `cargo run -p orbit-pi` is
the product; the per-item detail lives in AGENT.md.

| Phase | Deliverable | Status |
|---|---|---|
| P0 | Transport + workspace + process spawner + RPC client + first live round-trip | ✅ Shipped |
| P1 | Shell, theme, fonts, sidebar sessions | ✅ Shipped |
| P2 | Data layer: serde models, sessions/catalog/workbench clients | ✅ Shipped |
| P3 | Core chat: transcript, streaming, markdown, composer, tools, diff | ✅ Shipped (stream veil + explicit ≤8.3 Hz pipeline open) |
| P4 | Workbench pages + settings persistence | ✅ Shipped (plus Explorer/Files, terminal, Git/GitHub, localization, updater) |
| P5 | Mermaid fallback + a11y | ◐ Mermaid code-block fallback + reduce-motion shipped; focus rings / screen-reader labeling open |
| P6 | Tests, perf harness, packaging/notarization, CI | ◐ Unit/live tests, 10k perf test, signed `.app`/DMG, CI shipped; on-device scroll benchmark + streaming edge coverage open |

Open backlog (detail in AGENT.md): conversation fork/rewind, on-device scroll-perf measurement,
stream veil, explicit streaming commit pipeline, focus rings / screen-reader labeling, richer
per-tool renderers, Windows/Linux native polish.

## Top risks (with mitigations)

1. **P3 scope** — *resolved:* was the dominant estimate (25–40 days); it shipped by keeping the
   Waku pattern map (AGENT.md), one-StyledText-per-block markdown from day one, and tool
   renderers by spec.
2. **Tool approval protocol gap (D1 residual)** — resolved: pi exposes no native per-tool permission in RPC mode, so Orbit enforces access modes through a bundled `tool_call` extension that prompts via `ctx.ui.select` (Allow once / Always allow this tool / Deny), rendered natively as an inline bar. An "Auto" AI reviewer stays open until pi exposes a reviewer API to extensions.
3. **Composer editor** — *resolved:* the multi-line `ComposerInput` (`EntityInputHandler`) has
   shipped, wrapping, auto-growing, and scrolling. Reuse it for new text controls rather than
   introducing another editor abstraction.
4. **Mermaid regression** — accepted; D2 path chosen, fallback never fakes.
5. **GPUI churn** — pinned 0.2.2 (D4); upgrade = its own task with the registry source as docs.
6. **Velocity** — Rust UI iteration is 3–5× slower than React hot-reload; the estimates assume
   this, not hope.
