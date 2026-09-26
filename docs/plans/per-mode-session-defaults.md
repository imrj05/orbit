# Plan — Per-mode session defaults (model + thinking level)

**Status:** Proposed (not implemented)
**Owner:** TBD
**Scope:** Orbit desktop app (`crates/orbit-pi`, `crates/orbit-rpc`)
**Related:** `workflow.rs` (Plan/Build/Ask), `auto_title.rs` (store pattern),
`app/settings.rs` (Settings → Agent), `bundled_extensions.rs` (spawn chokepoint)

---

## 1. Goal

Let a user pick, for each **workflow mode** (Plan / Build / Ask):

- the **default model** (`provider` + `modelId`), and
- the **default thinking level** (`off` … `max`),

in the Settings surface. When Orbit starts a session in that mode — and when a
session's mode changes — the session is put on that model and thinking level.

Today Orbit spawns pi with only `--mode rpc --approve` and lets pi choose the
startup model from `~/.pi/agent/settings.json`. There is no per-mode control.

---

## 2. Decisions

- **D-A — Per-mode, not single default.** Three independent slots
  (`plan`, `build`, `ask`). An empty slot means "inherit pi's own default" —
  do *not* force a model.
- **D-B — Orbit owns the config.** Persist to
  `~/.orbit-pi/session-defaults.json`, matching the existing
  `auto-title.json` / `workflow.json` pattern. Do **not** write pi's
  `~/.pi/agent/settings.json` — that file is global to the machine, is
  rewritten by pi itself, and would silently affect terminal `pi` users.
- **D-C — Apply at every session birth, not only at process spawn.** One pi
  process can host many sessions (`new_session`, `switch_session`, `clone`,
  workspace switches), and a `new_session` inside a live process does not
  re-read CLI flags. So defaults are applied both:
  1. as CLI args when a process is spawned (covers the first session), and
  2. via `set_model` / `set_thinking_level` RPC whenever a session is created
     or its mode is set/changed (covers every later session).
- **D-D — New sessions only.** A *resumed* session keeps the model recorded
  in its session file; we do not override history on open. A manual "Use mode
  default" action can re-apply on demand.
- **D-E — Fail soft.** Unknown/missing model or thinking level is skipped with
  no error; the session simply keeps whatever pi chose. A malformed config
  file loads as "all slots empty".

---

## 3. Non-goals

- Not a per-session override UI (already exists in the composer chip).
- Not syncing to pi's `settings.json` or to `defaultModel` /
  `defaultThinkingLevel` (that is the rejected "Option A").
- Not model selection for the AI reviewer or commit-message helper — those
  stay scoped/read-only as they are now.
- Not changing `enabledModels` / cycling behaviour.

---

## 4. Data model

New module: `crates/orbit-pi/src/session_defaults.rs` (mirror
`auto_title.rs`).

```jsonc
// ~/.orbit-pi/session-defaults.json
{
  "plan":  { "provider": "anthropic", "model_id": "claude-sonnet-4-5", "thinking": "high" },
  "build": { "provider": null,        "model_id": null,               "thinking": null },
  "ask":   { "provider": "openai",    "model_id": "gpt-5.6",          "thinking": "low" }
}
```

Rust shape (lenient parse; unknown keys ignored; a slot is "unset" when
`model_id` is `null` or absent):

```rust
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct ModeDefault {
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub thinking: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct SessionDefaults {
    pub plan: ModeDefault,
    pub build: ModeDefault,
    pub ask: ModeDefault,
}

impl SessionDefaults {
    pub fn load() -> Self;                       // read ~/.orbit-pi/session-defaults.json
    pub fn persist(&self);                       // temp-file + rename (see workflow.rs)
    pub fn for_mode(&self, mode: WorkflowMode) -> &ModeDefault;
    pub fn set(&mut self, mode: WorkflowMode, slot: ModeDefault);
}
```

Notes:

- Reuse the atomic temp-file + rename write from `workflow.rs::write_map_at`
  so a running extension/reader never sees a partial file. (This config is
  read by the app only, but the convention is cheap and consistent.)
- `thinking` is a plain `String` here; validate against the model's reported
  levels at apply time (`available_thinking_levels`) rather than at parse time,
  so a stale value can never brick the session.
- Add `store_path()` unit tests next to `auto_title.rs`'s.

---

## 5. Settings UI (Settings → Agent)

Reuse the existing dropdown machinery. Reference implementation:
`title_model_select()` (model) and `SettingsSelect::TitleModel` in
`app/settings.rs`.

### 5.1 New `SettingsSelect` variants

```rust
// app.rs, enum SettingsSelect
PlanModel, PlanThinking,
BuildModel, BuildThinking,
AskModel, AskThinking,
```

### 5.2 Rows

In the Settings → Agent section (near the behavior / auto-title group), add a
`settings_section(theme, &tr!("settings.session_defaults"), rows)` with three
sub-groups — Plan, Build, Ask — each with two rows:

| Row | Control | Options source | Selected default |
|---|---|---|---|
| `settings.plan_model` | `select_control` | `self.available_models` | slot `model_id` |
| `settings.plan_thinking` | `select_control` | `self.available_thinking_levels` | slot `thinking` |
| … Build … | | | |
| … Ask … | | | |

- Model option label: `"{provider_display_name} · {model.name}"`, exactly as
  `title_model_select()` renders it.
- Model `selected` index: `position()` over `available_models` matching
  `provider` + `id`; `0` = a leading "pi default" / "Unset" entry.
- Thinking control only enabled when a model is chosen; options come from
  `self.available_thinking_levels` (already fetched via
  `get_available_thinking_levels` in `refresh_catalogs`).
- Reuse `select_control(id, kind, label, options, selected, theme, this, cx)`
  and `settings_select_popup`. Extend the `on_settings_select` match arms
  (`settings.rs` ~line 5841) to write the slot and `persist()`.

### 5.3 i18n

Add keys under `settings.` in the translation catalog(s) used by `tr!`
(see `AGENT.md › Localization` / `i18n.rs`), for all supported locales:

```
settings.session_defaults
settings.session_defaults_hint
settings.plan_model / settings.plan_thinking
settings.build_model / settings.build_thinking
settings.ask_model  / settings.ask_thinking
settings.use_mode_default
```

Also add a short hint line stating: "Applied to new sessions in this mode.
Existing sessions keep their model."

---

## 6. Spawn wiring

### 6.1 `PiClient` accepts extra CLI args

`crates/orbit-rpc/src/client.rs` → `spawn_inner()` currently builds
`["--mode", "rpc", "--approve"]`. Add one seam:

```rust
fn spawn_inner(
    bin: &str,
    workspace_dir: &Path,
    session_dir: Option<&Path>,
    extensions: &[PathBuf],
    env: &[(&str, &str)],
    extra_args: &[String],   // NEW
) -> Result<Self>
```

- Append `extra_args` after the base args and before `current_dir`.
- Thread through the public constructors (`spawn`, `spawn_with_extensions`,
  `spawn_with_extensions_and_env`, `spawn_with_bin*`). To avoid churning every
  caller, add **new** constructors that take args and keep the existing ones
  delegating with `&[]`:
  - `spawn_with_extensions_and_args(workspace, session_dir, extensions, args)`
  - `spawn_with_extensions_and_env_and_args(...)`
- Existing tests keep calling the old signatures → zero behaviour change.

### 6.2 `BundledExtensions::spawn` takes a mode

```rust
// bundled_extensions.rs
pub(crate) fn spawn(&self, workspace: &Path, mode: WorkflowMode) -> Result<PiClient>
```

Compute args from the mode default:

```rust
let def = SessionDefaults::load().for_mode(mode);
let args = def.cli_args(); // [] when unset
```

`ModeDefault::cli_args()`:

- model set → `["--provider", p, "--model", format!("{p}/{id}")]`
  (exact `provider/id`; if `provider` is null, emit just
  `["--model", id]` and let pi fuzzy-match)
- thinking set → append `["--thinking", level]`
- unset → `[]`

Prefer `--model provider/id` + `--thinking` over the `:thinking` suffix so the
two can be set independently and stay readable. pi clamps the level to the
model's capabilities. (See `pi --help` / `docs/cli.md`; `--thinking`
overrides a `--model` suffix.)

Call sites to update:

- `app.rs` initial spawn (~line 876): pass the mode of the workspace's
  pending/stored session. For a brand-new app launch with no session yet, use
  the pending New-Task mode if present, else Build.
- `BundledExtensions::spawn_reviewer` — **unchanged**, stays `ask` and
  read-only; do **not** apply user defaults there (the reviewer is Orbit's own
  helper).

### 6.3 Apply on every session birth / mode change (RPC)

Because `new_session` inside a live process ignores CLI flags, add an apply
step that sends RPC commands. Wire it where the app already learns the new
session and its mode:

1. **`reset_quota_entries()`** (`app/runtime.rs`) already resolves
   `self.workflow_mode` from `workflow_pending` / `workflow::load_for()`. After
   it sets the mode, call `self.apply_mode_defaults(cx)`.
2. **`new_session` / `switch_session` / `clone` response handlers**
   (`app/events.rs` ~728–755): they already send `get_state` +
   `refresh_catalogs`; after `get_state` returns, apply defaults **only when
   the response reports the same session/mode we expect** (avoid racing a
   resumed session — see D-D; for `switch_session`/`clone` skip by default).
3. **Mode switch** (wherever `workflow_pending` / `persist_for` is called when
   the user changes mode in the composer): after `persist_for`, call
   `apply_mode_defaults`.

```rust
impl OrbitApp {
    /// Push the current mode's default model + thinking level to the live
    /// session. No-op for any field that is unset, unknown, or already active.
    fn apply_mode_defaults(&mut self, cx: &mut Context<Self>) {
        let def = SessionDefaults::load().for_mode(self.workflow_mode);
        // model
        if let Some(id) = &def.model_id {
            if let Some(provider) = &def.provider {
                let already = self.active_model_matches(provider, id);
                if !already && self.model_is_available(provider, id) {
                    self.set_model(id.clone(), provider.clone(), cx); // composer_ops.rs
                }
            }
        }
        // thinking
        if let Some(level) = &def.thinking {
            if self.available_thinking_levels.iter().any(|l| l == level)
                && self.active_thinking_level.as_deref() != Some(level)
            {
                self.set_thinking_level(level.clone(), cx);
            }
        }
    }
}
```

- `set_model` / `set_thinking_level` already exist in `app/composer_ops.rs`
  and send `CommandBody::SetModel` / `SetThinkingLevel`.
- Guard against sending a model the catalog doesn't list
  (`self.available_models`) — a removed/renamed model must not poison the
  session. Log/toast nothing by default; optionally an inline note on the
  Settings row when the saved model is missing from the catalog.
- The `set_model` path may itself trigger a `get_state` / catalog refresh;
  make sure `apply_mode_defaults` is idempotent and does not loop. Track the
  last-applied `(provider, id, level, session_id)` to suppress repeats.

---

## 7. Semantics / precedence

| Situation | Result |
|---|---|
| Slot unset (`model_id: null`) | pi's own startup default is left untouched |
| Slot set, model in catalog | applied on session birth and mode switch |
| Slot set, model missing from catalog | skipped; session keeps pi's default |
| Slot set, thinking unsupported by model | level filtered against `available_thinking_levels`, skipped if absent |
| Resumed/opened historical session | not overridden (D-D) |
| Composer chip changed by user mid-session | wins until next mode switch / new session |
| AI reviewer process | never uses these defaults |

---

## 8. Edge cases to test

- `new_session` after a `switch_session` in the same process gets the new
  mode's default (proves the RPC path, not just CLI args).
- Switching Plan → Build applies Build's model without restarting the process.
- Empty `session-defaults.json`, `{}`, malformed JSON, and unknown mode keys
  all load as "all unset" and never error.
- Model id present but not in `available_models` → no `set_model` sent.
- Thinking level valid in config but not in `available_thinking_levels` → no
  `set_thinking_level` sent.
- Concurrent sessions in different modes (background parked session on
  Plan, active on Build) do not cross-contaminate.
- `spawn_inner` with empty `extra_args` produces byte-identical argv to today
  (transport tests).

---

## 9. Files touched

| File | Change |
|---|---|
| `crates/orbit-pi/src/session_defaults.rs` | **new** — store, lenient parse, `for_mode`, `cli_args`, tests |
| `crates/orbit-pi/src/lib.rs` (or `main.rs` mod tree) | register `mod session_defaults;` |
| `crates/orbit-pi/src/app.rs` | `SettingsSelect::{Plan,Build,Ask}{Model,Thinking}` variants; load `SessionDefaults` into app state; `apply_mode_defaults`; `active_model_matches` / `model_is_available` helpers |
| `crates/orbit-pi/src/app/settings.rs` | three model + three thinking `select_control` rows; `on_settings_select` arms; optional "Use mode default" action |
| `crates/orbit-pi/src/app/runtime.rs` | call `apply_mode_defaults` after mode resolution in `reset_quota_entries` |
| `crates/orbit-pi/src/app/events.rs` | apply defaults on session-birth responses where appropriate |
| `crates/orbit-pi/src/bundled_extensions.rs` | `spawn(workspace, mode)` + build `cli_args`; leave `spawn_reviewer` alone |
| `crates/orbit-rpc/src/client.rs` | `extra_args` seam + `*_and_args` constructors; existing signatures delegate |
| `crates/orbit-pi/src/i18n/*` (all locales) | new `settings.*` keys |
| `INTENT.md` | record a `D#` decision for Orbit-owned per-mode defaults |
| `CHANGELOG.md` | entry once shipped |

---

## 10. Implementation order (suggested commits)

1. `session_defaults.rs` + unit tests (store + `cli_args`).
2. `PiClient` `extra_args` seam + transport test for byte-identical argv.
3. `BundledExtensions::spawn(workspace, mode)` and spawn call sites.
4. `apply_mode_defaults` + RPC wiring on mode/session change.
5. Settings UI rows + `SettingsSelect` arms + i18n.
6. INTENT.md / CHANGELOG.md; manual verification pass.

---

## 11. Verification

- `cargo test -p orbit-rpc -p orbit-pi`
- `cargo clippy --all-targets -- -D warnings` (repo convention — check
  `bacon.toml` / CI)
- Manual: set Plan = model A / `high`, Build = model B / `low`; start a Plan
  session, confirm `get_state` reports A + high; switch to Build without
  restarting pi, confirm B + low; start a second session in Ask, confirm it
  inherits the global default (empty slot).
- Confirm terminal `pi` and `~/.pi/agent/settings.json` are untouched.

---

## 12. Open questions

- Should an empty **Ask** slot inherit the **Build** slot rather than pi's
  default? (Plan/Build/Ask precedence chain vs. flat.)
- Do we want a visible inline badge when a saved model is missing from the
  catalog, or stay silent?
- Should "Use mode default" be a composer action, a Settings action, or both?
- Per-workspace overrides (`.pi`-scoped) — out of scope now, note as future.
