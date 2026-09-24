# INTENT.md — Decisions, rationale, and plan

This file captures **why** Orbit is being rebuilt and the decisions that shape the plan.
It is the source of truth for architecture choices; when AGENT.md or README.md conflict with
a decision recorded here, this file wins. Update it when a decision changes (and say why).

## Vision

Orbit becomes a **100% Rust, GPU-rendered desktop workbench for the pi coding agent** —
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
**Consequence:** until P3/P4 land there is no fallback UI — the GPUI app is the product, and the parity contract below becomes the build-forward backlog rather than a switch-over checklist. The deleted web app remains recoverable from git history if a reference implementation is ever needed.

### D4 — GPUI pinned to crates.io, not tracking main
**Choice:** `gpui = { version = "0.2.2", features = ["runtime_shaders"] }` — upgrades are
deliberate and scheduled, never `git main`.
**Reason:** pre-1.0 API with frequent breaking changes; reproducibility for users and CI.
**Note:** `runtime_shaders` compiles Metal shaders at app start via the GPU driver, because this
machine lacks the Xcode MetalToolchain (`metal` CLI). For release builds, consider installing
the Metal toolchain and precompiling shaders (drop the feature) to shave startup time.

### D5 — Platform: macOS first
**Choice:** macOS is the shipping target for P0–P6; Windows/Linux GPUI support exists but is
out of scope until the macOS app ships.
**Reason:** the dev machine is macOS; GPUI's macOS backend (Metal + font-kit) is its most mature.

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
default. Plan steps are parsed from the assistant's `Plan:` section and `[DONE:n]`
markers and appended as `orbit:workflow-todos` custom session entries; Orbit reuses
the quota bridge's `get_entries` poll to reduce them into a slim progress strip above the composer.

**Rationale:** pi exposes no RPC command for active tools, system prompt, or plan
state, and no native todo tool, so extension hooks are the only honest mechanism —
the same call D1 made for access modes. Per-session (not global like `access.json`)
because scope is a property of the task, and up to six parked sessions run at once.
Reading the store fresh on every hook re-arms a live session with no restart. The
session entry keeps the mode and plan authoritative across resume and branch.

**Residual:** this is a guard, not a sandbox (pi ships none). Progress depends on the
model tagging `[DONE:n]`, which the injected guidance states plainly; an
extension-registered `todo` tool is the fallback if compliance proves unreliable.

## The feature parity contract

Everything below must behave identically in the GPUI app (against the pi CLI) as it does in the
legacy app today:

1. Chat sessions — prompt composer, model selection, thinking-effort, steer/cancel/abort, streaming
2. Sessions grouped by project, persistent, reopenable, cross-workspace
3. Model catalog from pi's own list; thinking levels per model
4. Tool activity (bash, edit, todo, plan, search, mcp, thinking, question) + approval dialogs
5. Git diff panel + file-change blocks
6. Workbench pages — usage (charts), skills, plugins, models, providers (CRUD), settings
7. Markdown — GFM, syntax-highlighted code, copy-able code blocks, image lightbox
8. Theming — dark/light palettes, accent, font, density, reduce-motion. Persisted to **Orbit's own store** (`~/.orbit-pi/{theme,ui.json,fonts.json}`), not pi's settings: theme/UI state is Orbit's, and writing it into pi's config would fork pi's own source of truth. (Revised from "persisted to pi's settings".)
9. Attachments — files read to base64 images for prompts
10. macOS native expectations — traffic lights, native dialogs, keyboard operability. Menu-bar menus are **not** implemented; Orbit's actions are keyboard/mouse surfaces (⌘P palette, shortcuts), not an `NSMenu`.

### Implementation status (living)

Done: streaming transcript + virtualization; markdown + highlighting; composer with steering, follow-ups, cancel, autocomplete, attachments; extension dialogs; diff/Review + Git page; sessions (list/switch/new/delete/clone/cross-workspace) over an Orbit-owned project list (only folders the user added; removing one never touches pi) with a **warm process pool** so re-opening a recent session is a resume, not a Node spawn; usage, skills, plugins, models, providers, settings pages; transcript find; image lightbox; theming + reduce-motion; signed/notarizable packaging; CI; AI review agent (a read-only reviewer over the selected change set or the whole project that renders findings in the Review pane, on its own Ask-mode process).

Open: conversation **fork/rewind** (clone exists; rewind needs entry ids); on-device scroll-perf measurement. Access modes ship as a real `tool_call` confirmation guard (not a sandbox); an "Auto" AI reviewer awaits a pi reviewer API. Workflow modes (Plan / Build / Ask) ship per D8, including the plan-progress strip above the composer.

## Non-goals (explicitly out of scope)

- Any web technology in the UI — no webview surfaces as UI, no React, no HTML/CSS
- Multi-provider support à la Waku (pi only; Oh My Pi *may* come later as a second RPC flavor)
- Always-latest GPUI; auto-updater; Windows/Linux in the first release
- Fabricating permission modes, diagrams, or perf claims the protocol/renderer can't deliver

## Phases, estimates, and gates

Solo experienced dev, full-time. Ranges are honest uncertainty; P3 (core chat) dominates.

| Phase | Deliverable | Est. | Exit gate |
|---|---|---|---|
| P0 | Transport probe + workspace + process spawner + RPC client skeleton + first live round-trip | 3–5 d | D1 certified or `--approve` fallback decided; round-trip renders in window |
| P1 | Shell, theme, fonts, sidebar sessions | 5–8 d | App opens, themed, sessions listed per project |
| P2 | Data layer: serde models, sessions/catalog/workbench clients | 8–12 d | All 34 legacy endpoints have a Rust equivalent behavior |
| P3 | Core chat: transcript, streaming, markdown, composer, tools, diff | 25–40 d | Chat parity vs legacy app (D3 gate) |
| P4 | Workbench pages + settings persistence | 12–20 d | All workbench pages natively functional |
| P5 | Mermaid fallback + a11y | 5–18 d | Keyboard-operable; reduce-motion honored; diagrams never faked |
| P6 | Tests, perf harness, packaging/notarization, CI | 8–13 d | Ship; legacy tracks deleted |
| **Total** | | **~66–116 d (14–22 wk solo; 9–14 wk with two devs)** | |

Parallel with a second Rust dev (workbench + chat split): P3 and P4 run concurrently, cutting
wall-clock roughly in half.

## Top risks (with mitigations)

1. **P3 scope** — 25–40 days. Mitigate: Waku pattern map (AGENT.md), one-StyledText-per-block
   markdown from day one, tool renderers by spec from `agent-elements/`.
2. **Tool approval protocol gap (D1 residual)** — resolved: pi exposes no native per-tool permission in RPC mode, so Orbit enforces access modes through a bundled `tool_call` extension that prompts via `ctx.ui.select` (Allow once / Always allow this tool / Deny), rendered natively as an inline bar. An "Auto" AI reviewer stays open until pi exposes a reviewer API to extensions.
3. **Composer editor** — a good multi-line markdown-aware input is the hardest single control in
   GPUI 0.2.2 (could blow P3's composer sub-item to 10–12 d). Mitigate: single-line field first,
   multi-line as a deliberate follow-up; never ship a broken input.
4. **Mermaid regression** — accepted; D2 path chosen, fallback never fakes.
5. **GPUI churn** — pinned 0.2.2 (D4); upgrade = its own task with the registry source as docs.
6. **Velocity** — Rust UI iteration is 3–5× slower than React hot-reload; the estimates assume
   this, not hope.
