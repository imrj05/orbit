# Plan — Subagents (delegated child sessions)

**Status:** Proposed. Nothing implemented.
**Owner:** TBD
**Scope:** one bundled pi extension (`contrib/orbit-subagents-extension/`), a Settings → Subagents
master-detail **management page** (`subagent_file.rs`, `subagents.rs`, `app/subagents_ui.rs`), a
retained `ToolCall.details` field, and one guard-policy line. **No `crates/orbit-rpc` wire change.**
**Related:** `bundled_extensions.rs` (materialization + spawn env), `transcript.rs` (`ToolCall`,
`tool_execution_update`), `transcript_view.rs` (`render_activity_card` dispatch), `ask.rs` +
`render_ask_card` (the precedent for a bespoke tool card), `app/skills_ui.rs` + `skills.rs` (the
master-detail page precedent this one copies), `mcp/config.rs` (atomic surgical file writes),
`context_meter.rs` (usage formatting),
`workflow.rs` / `access.rs` + `contrib/orbit-{guard,workflow}-extension/` (the two guardrails a
child must inherit), `plugins.rs` (pi package discovery / `pi-subagents` co-existence),
INTENT D1 (RPC over stdio), D8 (workflow modes), D10 (extension bridge), D12 (Orbit-owned config).
Proposes **INTENT D17** — note `docs/plans/review-guide.md` also proposes D17; whichever ships
first keeps it, the other renumbers.

---

## 1. Goal

Let the model delegate a scoped job — "find where auth is wired", "review this diff", "add the
validation" — to a **fresh child `pi` process** with its own context window, and get back a short
report instead of a 40k-token exploration.

Today Orbit has exactly one conversation per session. Everything the agent reads to answer one
question stays in that context forever. Subagents are the standard fix: the child burns its own
context, the parent receives only the answer.

The feature is four things:

1. a `subagent` **tool** the model can call (single, chain, parallel),
2. a **child process** per delegation, with the same guardrails the parent session has,
3. a **card** in the transcript that shows the work, its current step and its cost honestly,
4. a **management page** where an agent's capability, workflow mode, model and prompt are set.

It is not a orchestrator app, not a background job runner, and not a second copy of
`pi-subagents`.

---

## 2. Why this fits Orbit

Almost all the machinery exists. The table is the feasibility argument.

| Need | Already in Orbit | Gap |
|---|---|---|
| Register a model-callable tool | the bundled-extension pattern (`bundled_extensions.rs` + five `contrib/` extensions loaded with `--extension`) | new extension |
| Spawn a `pi` child with a resolved binary and a working `PATH` | `orbit_rpc::pi_binary()`, `augmented_path()` (`client.rs`, issue #81) — `commit_message.rs::run_pi` is the existing headless-spawn precedent | children spawn from inside pi, not from Rust |
| Live tool progress in the transcript | `Event::ToolExecutionUpdate` → `transcript.rs::on_tool_execution_update` (accumulated `partialResult`) | extension must call `onUpdate` |
| A bespoke tool card | `render_activity_card` early-returns to `render_ask_card` for `ask_user_question` | new card + dispatch |
| A master-detail manager page inside Settings | `Settings → Skills` (`app/skills_ui.rs` + `skills.rs`): searchable list column, detail column, actions enum, discovery/I/O split | new page, same shape |
| Surgical edits to a user-owned markdown file | `mcp/config.rs` (atomic, preserves what Orbit does not own) | agent-file writer (§6.5) |
| Structured tool data on the card | `ToolFacts::from_result` reads `details` | `details` is **dropped** by `normalize_tool_result` — needs a retained field (§6.4) |
| Guarding child tool calls | `contrib/orbit-guard-extension/` already **denies** when `ctx.hasUI` is false | pass the extension to children (§D-E) |
| Keeping the child read-only in Plan/Ask | workflow extension already falls back to `ORBIT_WORKFLOW_MODE` (`index.js:76`) | Orbit never sets it; the child must (§D-E) |
| Cost/usage display | `orbit_rpc::MessageUsage`, `context_meter::{format_tokens, format_cost}`, the session usage meter | tool result must carry child `usage` (§D-H); `format_cost` goes `pub(crate)` |
| Orbit-owned config next to the app | `access.rs`, `workflow.rs`, `session_defaults.rs` → `~/.orbit-pi/*.json` | new file |
| Localization | `locales/en.yml` + `tr!` | new keys |

The one thing Orbit cannot borrow is pi's TUI subagent UI. That is fine: Orbit draws its own card,
and never depends on `pi-subagents`.

---

## 3. Vocabulary

- **Parent** — the session the user is talking to.
- **Child** — one `pi --mode json -p --no-session` process spawned by the tool.
- **Agent** — a named definition (markdown + frontmatter) that gives a child its prompt, model and
  tool list. `scout`, `worker`, `reviewer` ship built in.
- **Report** — the child's final assistant text; the only thing the parent model sees.
- **Run** — one `subagent` tool call: one child (`single`), N children at once (`parallel`), or N
  children in sequence (`chain`).

What pi gives us: `--mode json -p` streams one JSON object per line (`message_end`,
`tool_execution_*`, `message_update`, …) and exits after one prompt ([pi `docs/json.md`]). That is
the entire integration surface, and it is documented, not internal.

---

## 4. Decisions

### D-A — The feature is an extension plus a card; the RPC wire does not change

`crates/orbit-rpc` stays a transport (`AGENT.md`: "transport, not a place for UI, HTTP, or runtime
helpers"). The tool call arrives as an ordinary `tool_execution_*` event, its `details` carry the
run state, and Orbit renders it. Nothing in the protocol has to learn what a subagent is.

**Consequence:** no fake RPC capability, no version negotiation, and the feature works against any
pi that supports `--extension` and `--mode json`.

### D-B — Each delegation is a real `pi` process, one per child, `--no-session`

Mirrors pi's own example extension (`--mode json -p --no-session`) and `commit_message.rs`.

**Consequence:** children leave nothing behind to clean up, cannot be reopened, and cannot appear
in the sidebar. Phase 6 optionally trades this for inspectable runs (§12).

**Consequence:** a cold child costs Node startup (a few hundred ms) per delegation. Accept it; the
warm pool in `sessions` exists for user-visible sessions, not for delegated work.

### D-C — The tool is named `subagent`, and Orbit stands down if something else provides it

The name is what prompts and muscle memory already use. At activation the extension asks
`pi.getAllTools()`; if a `subagent` tool is already registered (for example `pi-subagents` is
installed), Orbit's extension **does not register** and Settings → Subagents says so in plain
words. Two implementations racing for one name is worse than one honest message.

### D-D — Agents are markdown in pi's own directories; v1 loads user scope only

`~/.pi/agent/agents/*.md`, frontmatter `name` / `description` / `model` / `tools` — the same files
the pi CLI and `pi-subagents` read, so an agent written for one works in the other. Builtins are
embedded in the extension (as `builtins.json`, D-X), user files override them by name.

`.pi/agents/*.md` (project scope) is **not** loaded in v1: those are repo-controlled prompts that
can instruct a model to run anything. pi's own example defaults to user scope for the same reason.
Phase 5 may add project scope behind Orbit's project trust, off by default.

Frontmatter is read by a ~20-line reader for exactly these four keys, not by a YAML parser
(`AGENT.md`: `contrib/` is zero-dependency). A malformed file is skipped with a diagnostic; it
never takes down discovery of the others.

### D-E — Children inherit the parent's guardrails; they never prompt

A child is a *new* session, so it is not automatically covered by anything Orbit passes to the
parent. Three things are therefore made explicit:

1. **Extensions** — Orbit sets `ORBIT_BUNDLED_EXTENSIONS` (a JSON array of absolute paths) on every
   session spawn in `BundledExtensions::spawn`. The subagent extension passes each path on to the
   child as `--extension`.
2. **Workflow mode** — read fresh from `~/.orbit-pi/workflow.json` for the parent's session id at
   call time (the workflow extension's own store, so there is one source of truth), then handed to
   the child as `ORBIT_WORKFLOW_MODE=<mode>`.
3. **Access mode** — nothing to pass. The guard reads `~/.orbit-pi/access.json` fresh on every tool
   call, so a child sees the same mode the parent does.

Because `JsonMode` has no UI, the guard's existing fail-closed path takes over: a call the mode
would *ask* about is **denied** in the child
(`if (!ctx?.hasUI) return { block: true, reason: "Blocked: Orbit has no UI to confirm this action" }`).
And the workflow extension's `tool_call` hook blocks write tools in Plan/Ask regardless.

**Consequence, stated in the UI:** children can only do what the current access mode auto-approves.
Supervised → read-only children. Auto-accept edits → children may edit, cannot run commands.
Full access → children are unrestricted, exactly like the parent. This is a guard, not a sandbox
(`INTENT.md` on access modes), and the card says which mode the child ran under.

### D-F — Model and thinking inherit; an agent may pin a model

Default: the parent's current model and thinking level (`ctx.model`, `ctx.thinkingLevel`), passed
explicitly so a child never silently falls back to a provider default. An agent's `model:` wins.
The card shows the model the child **actually used** (read from its `message_end` events), never
the requested one.

### D-G — The parent sees reports only, capped

`content` returned to the parent model is: one line per child (`agent`, status, usage) plus each
report, capped at **16 KiB per child** with an explicit `… truncated (N KiB) — full report in the
card` marker. The full report (≤ 64 KiB) lives in `details` for the card.

Never returned to the parent: the child's transcript, tool arguments, or file contents. That is the
whole point of delegating.

### D-H — Cost is real, so it is reported

Children spend the user's credential. The tool result carries `usage` — the summed child usage —
including failed, aborted and timed-out runs (their tokens were spent too). pi adds tool-result
usage to session totals, so Orbit's existing meter and the per-session cost stay honest with no new
plumbing. Per-child usage appears on the card.

### D-I — One schema, three modes, bounded fan-out

```
subagent({ agent, task })                      // single
subagent({ chain: [{ agent, task }, …] })      // sequential, {previous} placeholder
subagent({ tasks: [{ agent, task }, …] })      // parallel
```

Caps, enforced in the extension and reported in the error text (never silently dropped):
**8 tasks per run, 4 concurrent, 24 children per parent session, depth 1.** The schema is fixed in
the first release so the model-facing description does not churn.

### D-J — Abort is immediate and total

`execute(…, signal)` is honoured: on abort, each child's process group gets `SIGTERM`, then
`SIGKILL` after 3 s. A 30-minute hard timeout per child bounds a hung agent. The parent's stop
button aborts the turn, which aborts the tool, which kills every child — no orphan processes.

**Consequence:** an aborted run reports `aborted`, not `failed`. Orbit's UI already tells those
apart.

### D-K — Live progress rides `tool_execution_update`; the collapsed row keeps a fixed height

The extension calls `onUpdate` with `{ content, details }` as events arrive; pi emits
`tool_execution_update` with the accumulated `partialResult`; `transcript.rs` already replaces the
call's output in place and **only remeasures when the detail card is open**.

**Consequence:** everything that streams — the step log, the per-child rows — lives inside the
expanded card. The collapsed header is one fixed-height line (status · agent · model · usage), so a
streaming run never churns list layout.

### D-L — `ToolCall` keeps `details`; the card is parsed leniently

`normalize_tool_result` currently collapses `{content:[…], details:{…}}` to its text, so today the
only thing Orbit keeps from `details` is `ToolFacts`. Subagent state cannot survive that, so
`ToolCall` gains `details: Option<Value>`, filled from the result envelope on
`tool_execution_end`/`_update` and **bounded at ingest** (drop above 128 KiB rather than hold it).

`subagent.rs` parses `details` when it carries `"orbit": "subagent/1"`. Unknown version, missing
fields, or a foreign shape returns `None` and the call renders as the generic activity card — a
future extension version degrades, it does not break the transcript.

### D-M — `~/.orbit-pi/subagents.json` is the Orbit-owned config, read fresh

`~/.orbit-pi/subagents.json`, read by the extension on every call (the `workflow.rs` / `access.rs`
pattern), so a change in the app re-arms live sessions with no restart:

```json
{
  "enabled": true,
  "maxConcurrent": 4,
  "maxChildrenPerSession": 24,
  "maxTasksPerRun": 8,
  "timeoutMinutes": 30,
  "projectAgents": false,
  "agents": {
    "scout": { "enabled": true, "mode": "inherit" },
    "worker": { "enabled": true, "mode": "inherit", "thinking": "high" },
    "reviewer": { "enabled": true, "mode": "plan" }
  }
}
```

Orbit-only facts (`enabled`, `mode`, `thinking`) live here because they do not exist in a portable
agent file (D-P). Everything else about an agent — its name, description, model, tools and prompt —
lives in the markdown file, which is also what the pi CLI and `pi-subagents` read.

Absent or malformed → every value falls back to the defaults above (never to "unlimited").

### D-N — A child is just a pi session in the workspace

Same cwd, same project trust (`--approve`, because the parent has it), same `AGENTS.md`/skills.
No worktree, no copy, no isolated filesystem. Two children editing the same file can conflict, and
the UI does not pretend otherwise.

### D-O — Delegation is model-initiated, and nothing runs automatically

No auto-review on settle, no background reviewer, no injected "always delegate" instruction. The
tool description explains when delegation pays off (broad search, independent review, a second
opinion) and when it does not (one file you already know). A suggestion in `before_agent_start` was
considered and rejected: Orbit already makes one unsolicited model call per session (auto-title),
and that is the budget.

### D-P — The management page edits the agent files, not a shadow copy

An agent is a file in `~/.pi/agent/agents/`. The page reads and writes **that file**, so the pi CLI,
`pi-subagents`, and Orbit all load the same definition and nothing drifts between "what Orbit thinks
an agent is" and "what pi runs".

Only three facts cannot live in a portable file — whether the agent is enabled, which workflow mode
it is pinned to, and its thinking level — so those live in `~/.orbit-pi/subagents.json` (D-M). The
page presents one form; §6.5 says which store each field lands in.

### D-Q — "What this subagent can do" is a capability tier, and it is enforced twice

Three page-facing tiers, each a fixed tool set:

| Tier | `tools:` written | Guard ceiling |
|---|---|---|
| **Read only** | `read, grep, find, ls, glob` | `read` |
| **Edits files** | + `edit, write` | `edit` |
| **Runs commands** | + `bash` | `exec` |

The tier is the single input. It writes the file's `tools:` line (so the child's `--tools` cannot
even offer the others) and it is passed to the child as `ORBIT_SUBAGENT_CEILING`, which the guard
clamps `classify(tool)` against. A read-only agent therefore cannot edit **even under Full access**,
and a tool the agent never declared is denied as well as unavailable.

An advanced field accepts a custom tool list (MCP servers, extension tools). Any name outside the
table raises the ceiling to `all`, and the page says so inline — a custom tool cannot be silently
narrower than it is.

### D-R — Per-agent workflow mode, intersected with the session's

`inherit` (default) | `plan` | `build` | `ask`, stored in `subagents.json`, handed to the child as
`ORBIT_WORKFLOW_MODE`. The effective mode is the **more restrictive** of the agent's and the
parent session's: Plan on the session means no child writes, whichever mode an agent pins.

The page states the resolved rule in one line ("pinned to Build · writes blocked while the session
is in Plan"), and the card shows the mode each child actually ran under.

### D-S — The page shows exactly what the model sees

The tool description is generated from **enabled** agents, and the page renders that exact block
with its character count ("adds 412 characters to every request"). Descriptions are capped at 200
characters with an inline warning, because they are paid for on every turn of every session.

### D-T — The page is Settings → Subagents, master-detail, with three ways in

Same shell as Settings → Skills (searchable list, detail column, one action enum, discovery and
file I/O in a non-UI module): `SettingsSection::Subagents`, a ⌘P row "Manage subagents" (D13
command registry), and an "Edit agent" action on the transcript card. Full-bleed inside Settings,
not a top-level surface — a top-level Subagents surface is what phase 6's run inspection would
become, if it ever ships.

### D-U — Writing an agent file is surgical and atomic

The writer replaces only the four keys Orbit owns (`name`, `description`, `model`, `tools`), keeps
every other frontmatter key intact (another extension's `extensions:`, `skills:`, `memory:`), keeps
comments, key order, and the body byte-for-byte, and lands with temp-file + rename (the
`mcp/config.rs` rule).

A file changed elsewhere is detected before a save (mtime + content hash captured on load). A dirty
form never overwrites a changed file: the page asks, and offers Reload or Overwrite.

### D-V — Every status is derived from a real event; nothing is invented

The card's phase, counts, elapsed and cost come from the child's own JSONL events and from
timestamps the extension stamps. No percentage, no ETA, no "probably doing X". Silence is reported as
silence (`no activity for 15s`), not as a spinner pretending progress.

**Consequence:** the card looks boring while a child is idle mid-thought, and that is correct — it is
the truth, and it is what makes the live activity trustworthy when it does appear.

### D-W — One card, three zoom levels; the collapsed row always names the current action

The row is the parent transcript's claim about the child, and a claim is only useful if it says what
the child is doing **now** rather than what it was asked to do. So: the current action in the row,
per-child status/counts/mode/model in the card, real steps one level deeper.

**Consequence:** the task text does not repeat in the row (it is in the card header and in the tool
arguments), which is what keeps the row to one fixed line (D-K).

### D-X — The agent library is the user's: add, edit, rename, delete, restore — all in-app

Every agent is a file, and the page manages it end to end. No config editing, no restart, no
protected rows. Builtins are shipped *defaults*, not untouchable ones:

| Action on | What happens |
|---|---|
| **Add** | Writes `~/.pi/agent/agents/<name>.md` (read-only tier, `inherit` mode, prompt skeleton), selects it |
| **Edit** — user agent | Rewrites the four keys + body in that file (D-U) |
| **Edit** — builtin | Writes `~/.pi/agent/agents/<name>.md`, a user file that **overrides** the builtin (pi's own resolution). The shipped text stays available, so *Reset to builtin* always works |
| **Rename** | Writes the new file, removes the old one, after a confirm that says the agent's identity changes |
| **Disable** — builtin | Records it in `subagents.json → disabled`; the model no longer sees it. There is no file to delete, so the button does not claim to delete one |
| **Delete** — user agent | Removes the file (inline confirm), keeping the bytes for the toast's **Undo** |
| **Reset to builtin** | Removes the override; the shipped definition returns |

**Consequence — three labels, three real outcomes.** A builtin row offers *Disable*, an override row
offers *Reset to builtin*, a user row offers *Delete*. "Delete" never means two different things,
and every one of them is reversible in one click.

**Consequence — one source for the shipped text.** The builtins live in
`contrib/orbit-subagents-extension/builtins.json`; the extension reads it next to itself and Orbit
`include_str!`s the same file, so the page can show the shipped definition, mark a row *modified*, and
render the override against it. Two copies of a prompt that disagree would be worse than no reset.

**Consequence — changes land on the next delegation.** The extension rediscovers agents per call, so
no restart is needed, and a running child keeps the prompt it started with (§10.6).

---

## 5. Non-goals

- **Background children** that keep working after the turn returns, and a way to check on them.
- **Steering/intercom** — talking to a live child mid-run.
- A **fleet pane**, agent browser page, or per-child transcript viewer (phase 6 revisits the last).
- **Missions, schedules, councils, debates**, watchdog reviewers, worktree isolation.
- Installing, bundling, or depending on `pi-subagents`. It is a source of ideas and a
  compatibility constraint (D-C), nothing more.
- Model tiering heuristics ("scout on a cheap model"). One optional per-agent override, nothing
  clever.

---

## 6. Data model

### 6.1 An agent file

```markdown
---
name: scout
description: Fast local codebase recon — relevant files, entry points, data flow, risks.
model: anthropic/claude-haiku-4-5
tools: read, grep, find, ls, glob
---

You are a scout. Find, do not change. Return a compressed map: …
```

- `name` (required, kebab-case, unique) — how the model names it.
- `description` (required) — appears in the tool description, so it decides whether the model picks
  this agent.
- `model` (optional) — `provider/model`; absent → inherit (D-F).
- `tools` (optional) — comma list or bracketed list; absent → pi's defaults. **The management page
  writes this line from the capability tier (D-Q)**, so a page-authored file and a hand-authored one
  are the same shape; a hand-written list above the tier is honored and the page shows it as
  Advanced.
- Body — appended system prompt (`--append-system-prompt`).

No thinking key: the level is per-agent Orbit config passed as `--thinking` (§6.2), because the file
has no portable key for it.

Builtins shipped in the extension, defined in `builtins.json` beside it (read by the extension, and
`include_str!`d by Orbit so the page can show and reset the shipped text — D-X): **scout** (read
only — recon), **worker** (runs commands — implementation), **reviewer** (read only — review). Three, not seven; each earns its place
or it is not added. Scout deliberately cannot edit, so a delegation for "find and fix" belongs to
`worker`.

### 6.2 The details contract (`details["orbit"] == "subagent/1"`)

```json
{
  "orbit": "subagent/1",
  "mode": "single",
  "children": [
    {
      "id": "c1",
      "agent": "scout",
      "source": "builtin",
      "capability": "read",
      "task": "find where auth is wired",
      "status": "running",
      "model": "anthropic/claude-haiku-4-5",
      "thinking": "medium",
      "workflowMode": "build",
      "phase": "tool",
      "current": "grep /auth/ in src/",
      "tools": 12,
      "toolCounts": { "read": 4, "grep": 7, "bash": 1 },
      "cwd": "/Users/me/project",
      "steps": [{ "kind": "tool", "name": "grep", "preview": "auth in src/" }],
      "stepsTotal": 12,
      "report": "",
      "error": "",
      "stderrTail": "",
      "blocked": [{ "tool": "edit", "target": "src/legacy.rs", "reason": "Blocked: Plan mode" }],
      "queuedAt": 1767225601200,
      "lastEventAt": 1767225634000,
      "usage": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "cost": 0, "turns": 0, "contextTokens": 0 },
      "startedAt": 1767225600000,
      "endedAt": null
    }
  ]
}
```

Bounds, applied by the extension before anything crosses the pipe: `steps` last **8** (each
`preview` ≤ 160 chars, `stepsTotal` keeps the honest count), `report` ≤ 64 KiB, `stderrTail` ≤ 2 KiB,
`task` ≤ 2 KiB, `blocked` last **5**, `toolCounts` ≤ 12 entries.
`status ∈ {running, done, failed, aborted}`; `phase ∈ {queued, spawning, thinking, tool, blocked,
settled, killed}` (§9.4). New optional fields are additive inside `subagent/1` — older or narrower
payloads still render, with the card showing only what it was told (D-L).

### 6.3 Rust shapes (`crates/orbit-pi/src/subagent.rs`, pure, no I/O)

```rust
pub(crate) const DETAILS_VERSION: &str = "subagent/1";
pub(crate) fn is_subagent_tool(name: &str) -> bool;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SubagentRun {
    pub mode: Mode,                    // Single | Parallel | Chain
    pub children: Vec<SubagentChild>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SubagentChild {
    pub agent: String,
    pub status: Status,                // Running | Done | Failed | Aborted
    pub phase: Phase,                  // Queued | Spawning | Thinking | Tool | Blocked | Settled | Killed
    pub current: Option<String>,       // "grep /auth/ in src/" — the row's middle slot (§9.1)
    pub tools: u64,                    // total tool calls
    pub tool_counts: Vec<(String, u64)>, // ≤12, hover tally
    pub blocked: Vec<BlockedCall>,     // ≤5, drives the card banner (§9.2)
    pub capability: Option<String>,    // read | edit | exec | all — the tier the child ran under
    pub workflow_mode: Option<String>, // the effective mode (D-R)
    pub model: Option<String>,
    pub task: String,
    pub steps: Vec<SubagentStep>,      // bounded again on ingest
    pub steps_total: u64,
    pub report: Option<String>,
    pub error: Option<String>,
    pub usage: orbit_rpc::MessageUsage, // parsed with MessageUsage::from_value
}

impl SubagentRun {
    /// Reads `tool.details`; `None` for a foreign/unknown shape (D-L).
    pub(crate) fn from_tool(tool: &ToolCall) -> Option<Self>;
    /// The collapsed row's text (§9.1): "scout · grep /auth/ in src/ · 12 tools · 0:34".
    pub(crate) fn headline(&self) -> String;
    /// What the parent's working line should say (§9.5), `None` when nothing runs.
    pub(crate) fn live_activity(&self) -> Option<String>;
    /// "2 of 3 running", "blocked by Plan mode", "3 done · 44 tools".
    pub(crate) fn status_phrase(&self) -> String;
    pub(crate) fn is_live(&self) -> bool;
}
```

Parsing rules: unknown keys ignored, absent optional fields stay absent (`None`) — the UI never
invents a count or a status (same rule as `ToolFacts`).

### 6.4 `transcript.rs` change

- `ToolCall.details: Option<Value>` — new field, set from the envelope in `on_tool_execution_end`
  and `on_tool_execution_update`, `None` when absent, `null`, or > 128 KiB.
- `ToolFacts::from_result` keeps its current job (header facts); it does not read subagent data.
- Regression tests: details retained from a result and from a partial; oversize details dropped;
  `normalize_tool_result` unchanged.

### 6.5 The agent file, and how the page writes it

`~/.pi/agent/agents/scout.md` — the page owns **four keys and the body**, nothing else:

```markdown
---
# reviewed by hand, keep this comment
name: scout                                   <- Orbit
memory: project                               <- another extension's key, preserved
skills: [pi-web-access]                       <- another extension's key, preserved
description: Fast local codebase recon.       <- Orbit
tools: read, grep, find, ls, glob             <- Orbit (derived from the capability tier)
model: anthropic/claude-haiku-4-5             <- Orbit (optional; absent = inherit)
---

You are a scout. Find, do not change. …      <- Orbit (the prompt body)
```

Save = re-read the file, replace those four lines in place (adding a missing key after
`description`, never reordering the rest), keep everything else byte-for-byte, write to a temp file
in the same directory, `rename` over the original. No YAML library, no full re-serialization — a
re-serialization would drop comments and reorder keys the file's other reader may care about.

Rust side (`subagent_file.rs`, pure): `AgentFile::parse(bytes)` → `AgentFile { name, description,
model, tools, body, raw: RawFrontmatter }` and `write_with(&self, fields) -> Vec<u8>`; tests assert
a round-trip with no changes produces identical bytes, and that a change touches only the intended
line.

---

## 7. The tool

Registration happens at activation. The description is built from discovery, so the model is not
guessing agent names:

```
Delegate a scoped task to a subagent with its own context window.

Agents (user scope, ~/.pi/agent/agents):
  scout     read only · Fast local codebase recon — files, entry points, flow, risks.
  worker    runs commands · Implementation work in this workspace.
  reviewer  read only · Read-only review of a change.

The capability shown is what the agent may do; do not ask an agent to exceed it.

Modes: single {agent, task}; chain {chain:[…]} with {previous}; parallel {tasks:[…]}.
The child sees only the task; it does not see this conversation. Put everything it needs
in `task` (paths, what was decided, what to return). Returns a short report, not a transcript.
```

Execution outline:

1. Validate exactly one mode; unknown agent → error listing available agents (not a crash).
2. Read `~/.orbit-pi/subagents.json`; if `enabled` is false, return a plain refusal.
3. Resolve mode + access mode + agent list; refuse above depth 1 and above the session cap.
4. Run children (1, a sequential loop with `{previous}`, or a pool of `maxConcurrent`).
5. Report per D-G, usage per D-H, `details` per D-K.

Every failure returns a *text* result the parent can act on (`agent "xyz" not found; available: …`,
`child exited 1: <stderr tail>`, `timed out after 30 min`, `blocked by Plan mode: bash`). Only an
internal extension fault throws.

---

## 8. Spawning a child

Argv (built in one pure function, unit-tested):

```
pi --mode json -p --no-session
   [--model <provider/model>] [--thinking <level>]
   [--tools <a,b,c>]
   --extension <guard> --extension <workflow>
   --approve
   --append-system-prompt <tmpdir>/orbit-subagent-<id>.md
   -- "Task: <task>"
```

Env (inherited env, plus):

| Var | Value | Why |
|---|---|---|
| `PI_SKIP_VERSION_CHECK` | `1` | no network in a spawned child |
| `CI` | `1` | deterministic, non-interactive |
| `NO_COLOR` | `1` | json mode, but be explicit |
| `ORBIT_WORKFLOW_MODE` | the effective mode (D-R) | workflow extension resolves it before any session id exists |
| `ORBIT_SUBAGENT_CEILING` | `read` / `edit` / `exec` / `all` (D-Q) | the guard clamps every child tool call against it |
| `ORBIT_SUBAGENT_AGENT` | the agent's name | names the child in block reasons and logs |
| `ORBIT_SUBAGENT_DEPTH` | `1` | recursion guard |
| `ORBIT_SUBAGENT_PARENT` | parent session id | diagnosis/logs |

`cwd`: the run's `cwd` param, else the parent's `ctx.cwd`. The workspace, never a temp dir.

The system prompt goes in a temp file rather than argv (length, and `ps` exposure), removed in a
`finally`. On abort the temp dir is removed too.

Output parsing: line-buffered JSONL; `message_end` with `role: "assistant"` → text + usage +
`stopReason`/`errorMessage`; `tool_execution_start/end` → a step for the card; everything else
ignored. A line that does not parse is ignored (never fatal — pi may add events). Exit code ≠ 0,
`stopReason: "error"`, or an empty report with no error → `failed`, with the stderr tail attached.

---

## 9. UI — the delegation card

The parent session is where the user is. A delegation must therefore answer, at a glance and without
opening anything: **which agents are working, on what, how far along, and at what cost**. The card is
the only place a child's work appears — a child's tool calls are never parent tool calls, never a
parent activity row, and never mixed into the parent's own steps.

Three zoom levels, one component (`render_subagent_card`):

| Level | Shown by default | Answers |
|---|---|---|
| Row (collapsed) | in the activity stream | who is working, on what, how long |
| Card (expanded) | click / ⌘O | every child's status, counts, live activity, report |
| Child log | click a child row | that child's real step list |

### 9.1 The collapsed row

Fixed height, so a streaming run never churns list layout (D-K). Four shapes:

```
running, single     ⣿ Subagent · scout · grep /auth/ in src/      12 tools · 0:34
running, parallel   ⣿ Subagents · 2 of 3 running                  31 tools · 1:12
settled, ok         ⣿ Subagent · scout · 12 tools                  ↑3.1k ↓812 <$0.01   ✓
settled, blocked    ⣿ Subagent · worker · 24 tools                 blocked by Plan mode  ⚠
```

- The middle slot is the **current activity**, not a restatement of the task: for a running child it
  is the tool it is executing right now (`grep /auth/ in src/`), truncated to the row.
- The right side is the running total: tool calls, elapsed (`format_working_elapsed`), then usage and
  cost once settled.
- Icon: `activity_icon` already maps `subagent`/`agent`/`task` to `task.svg`.
- Tone: accent while live, `theme.error` on failure, warning for blocked, `theme.text_secondary`
  when settled.
- The row is a button — click / ⏎ expands — and carries its state through the existing
  `ElementId::NamedInteger` card-id scheme, so pointer and keyboard land on one target.
- **One card per `subagent` call.** Two delegations in one turn are two cards, never merged: the
  reader is tracking which agent did what, and a merged card would blur it.

### 9.2 The card

One header line per child, always in the same order:

```
⣿ scout     read only   grep /auth/ in src/        12 tools   0:34   haiku-4-5
⣿ reviewer  read only   waiting for the model       8 tools   0:21   sonnet-4-5
✓ worker    commands    edited src/auth.rs          24 tools   1:41   ↑8.2k ↓1.4k <$0.03
```

Per child: status glyph · agent · capability chip · **current activity** · tool count (with a
per-tool tally on hover: `read ×4 · grep ×7 · bash ×1`) · elapsed · mode chip when not inherited ·
model · usage. The running child's glyph pulses with the composer's existing wave.

Under the rows:

- **Run footer** — `3 agents · 44 tools · 1:52 · ↑12.1k ↓2.6k <$0.05`; in chain mode also `step 2 of 3`.
- **Blocked banner** when a child had a call denied, naming the mode:
  `Plan mode blocked 2 calls in worker · edit src/auth.rs`. This is the single most useful line for
  "why did my subagent stop?" — the guard's `block` reason is already in the child's stream, so this
  is display, not inference.
- **Timeout line** once a child passes 80% of its budget: `times out in 6m`.

### 9.3 A child's log (third level)

Clicking a child row reveals that child's steps in the same visual language as the parent's own
activity rows — glyph, verb, argument preview — so a child is read the way the parent is read:

```
grep   /auth/ in src/          12 matches
read   src/auth.rs             214 lines
edit   src/auth.rs             +12 −3
bash   cargo test -p auth      0:41  ✓
⚠      edit src/legacy.rs      Blocked: Plan mode
```

Bounded: last 8 steps live, a `+37 earlier steps` line, previews ≤160 chars, and only one child's log
open at a time (predictable row heights).

The final **report** renders below the log as Markdown, display-capped with the existing
`DETAIL_TEXT_CAP`, a copy action, and a `truncated N KiB` strip when the parent received a capped copy.

### 9.4 States, and what each one says

Nothing on this card is guessed: every phase maps to a real event from the child's JSONL stream, or
to the absence of one, and elapsed time comes from timestamps the extension stamps (D-V).

| Phase | When | Row reads |
|---|---|---|
| `queued` | parallel/chain slot not started | `waiting for a slot` |
| `spawning` | process started, no event yet | `starting agent` |
| `thinking` | between `message_start` and the first tool | `thinking` |
| `tool` | `tool_execution_start` without its end | `<tool> <argument preview>` |
| `blocked` | the child reported a denied call | `<tool> denied by <mode>` |
| `settled` | `message_end` with a stop reason | `done` / `failed: <error>` / `aborted` |
| `killed` | timeout or abort | `stopped after <elapsed>` |

A child that has produced no event for 15 s says `no activity for 15s` instead of pretending to work —
the same honesty rule that puts `waiting for the first step` where a skeleton would be.

### 9.5 The parent's live line

While a child runs the parent's turn is blocked, so its own working line is the user's main signal.
`working_activity_label` (`transcript_view.rs`) today returns `thinking…` or nothing; extend it: when
the live step's last tool is a `subagent` call with a running child, return `scout is grepping src/`
(`2 agents working` for parallel). The line already carries elapsed time and rate, so the user reads:

```
scout is grepping src/ · working for 34s
```

No new element, no second indicator — one honest sentence about the child.

### 9.6 The activity-group header

`activity_title` counts the turn's steps by kind (`Ran 7 commands · 4 thoughts`). A `subagent` call
falls into `other` and reads `Ran 1 tool`, which understates a three-agent run. Add a delegation
count — `Delegated to 3 agents · Ran 2 commands · 1 thought` — with new locale keys
(`transcript_view.delegated_to_agents_one/_other`). The children's own tool calls stay out of this
total: they are not the parent's work, and counting them would inflate the turn.

### 9.7 Cadence, motion, and what never happens

- **Coalescing** — the extension emits `onUpdate` at most every 400 ms per child, plus one on every
  state transition (start, blocked, settle). A child that greps 200 files produces ~50 updates, not
  200; the RPC channel and the layout both stay calm.
- **Motion** — a running child pulses; nothing else animates. Reduce-motion keeps the accent tone and
  drops the pulse.
- **No progress bars, no ETAs, no percentages.** We cannot know how many steps remain, so we show
  elapsed, counts, and the current action (D-V).
- **No live token stream of the report.** The report lands when the child settles; until then the log
  shows the work. Streaming prose into a card the user reads afterwards would be noise.

### 9.8 Keyboard and focus

`⏎` on the focused row expands, `Esc` collapses, `⌘O` toggles the detail of the row under the cursor
(the existing tool-detail binding), and a `jump to the running delegation` palette action scrolls to
the card and focuses it. Focus never moves on its own when a child updates — a streaming card that
steals focus mid-typing is worse than no card.

### 9.9 When the user is not looking

A delegation can outlive attention. One optional signal (phase 5): when a run longer than a minute
settles while the window is unfocused, push the existing notification channel
(`notifications.rs`, respecting its prefs): `scout finished · 3.1k tokens`. One per run, never per
child, and none while the transcript is on screen.

### 9.10 The approval bar

The guard classifies `subagent` as unknown ("other"), so in Supervised and Auto-accept it already
asks before the run. `summarize("subagent", input)` in `contrib/orbit-guard-extension/policy.js`
should render `scout · find where auth is wired` (or `3 agents · scout, reviewer`) instead of raw
JSON. Pure function, one test. The bar also states what the children will run under —
`2 agents · read only · Plan mode` — so the approval is informed (D-Q, D-R).

### 9.11 The manager

Agent management is Settings → Subagents — §10.

---

## 10. The management page

Where the user answers two questions per agent: **what may it do**, and **how does it work**. The
shape is `Settings → Skills` (list column, detail column, actions through one enum), because that
page already solved discovery, enable/disable, inline delete confirmation, and "open in editor".

### 10.1 Ways in

- Settings → **Subagents**, in the nav between Skills and Plugins.
- ⌘P → "Manage subagents" (D13: `commands.rs` gets the action, key, and palette row).
- The transcript card's "Edit agent" action, opening the page with that agent selected.
- Empty state when no user agent exists: the three builtins, and "New agent".

### 10.2 List column

Searchable (`picker_search_frame`, the Skills pattern), grouped **Built in** / **Your agents** (and
**Project** once phase 5 enables that scope). One row per agent:

```
scout                    read only      inherit   haiku-4-5    ●
worker                   runs commands  inherit   inherit      ●
reviewer                 read only      plan      sonnet-4-5   ○ disabled
```

Name, capability chip, mode chip, model, enabled dot, and a warning glyph when the file is
malformed (with the parse error on hover). "New agent" sits at the column's foot.

### 10.3 Detail column

Header: name, source badge (*builtin* / *modified* / *yours*), enabled toggle, and the actions of
D-X — **Duplicate**, **Reveal file**, **Copy path**, and the one label that fits this row:
**Disable** (builtin), **Reset to builtin** (override), or **Delete** (your file). The *modified* badge
appears exactly when a user file shadows a builtin, so an override is never invisible.

Then, in this order:

1. **What it can do** — the three tiers as a radio group with the derived tool chips underneath
   (`read grep find ls glob`), an **Advanced** disclosure for a custom tool list, and the ceiling
   sentence: "Runs read tools only. Orbit denies anything else in this child, even in Full access."
2. **Mode** — `Inherit the session` / `Plan` / `Build` / `Ask`, each with one line of what it does,
   plus the resolved rule (D-R) computed live against the current session's mode.
3. **Model** — `Inherit the session model` (writes no `model:` line) or the existing model picker
   (writes `provider/model`). **Thinking level** beside it: `Inherit` or an explicit level, stored in
   `subagents.json` rather than the file, because the file has no portable key for it and Orbit passes
   `--thinking` itself.
4. **Description** — single line, live character count, cap warning at 200 (D-S), helper text
   "the model reads this to pick an agent".
5. **Prompt** — multi-line editor (the `composer` input handler, monospace off, UI font on), the
   field that becomes `--append-system-prompt`.
6. **What the model will see** — the generated tool-description block rendered verbatim, with its
   character count. This is the page's honesty check: no claim about an agent that the model is not
   actually told.
7. **File** — absolute path, dirty state, **Reload**, **Open in editor**, last-modified time.

Focus: the search field and the prompt editor are real inputs, so Tab order runs list → header →
tiers → mode → model → description → prompt → file, and the page never steals focus from the prompt
while a delegation streams.

### 10.4 Add, edit, rename, delete

One `New agent` button at the foot of the list; the rest live in the selected row's header.

**New agent** asks two things — a kebab-case name and one sentence — then opens a draft with the
read-only tier, `inherit` mode and a three-line prompt skeleton. Nothing is written until **Save**,
and nothing else is guessed.

**Save** is the only write path: it validates (§10.5), writes surgically (D-U), refreshes the row, and
toasts `scout saved · used from the next delegation`. A rename goes through the same path with the
confirm of D-X.

**Disable / Delete / Reset to builtin** are one button whose label and copy follow the row's source
(D-X), each with an inline confirm that names the outcome in one sentence:

| Row | Button | Confirm says |
|---|---|---|
| builtin | **Disable** | "scout stays installed but the model will not see it." |
| user file | **Delete** | "Deletes ~/.pi/agent/agents/scout.md." + an **Undo** toast for 10 s |
| user file that shadows a builtin | **Reset to builtin** | "Removes your scout.md; the shipped scout returns." |

**Empty state** (no user agents): the three builtins, one line naming the path — "agents live in
~/.pi/agent/agents" — and **New agent** / **Reveal folder**.

**Duplicate** stays on every row (writes `<name>-copy.md`, selects it): the fastest way to start from
a working definition without editing a shipped one.

### 10.5 Validation

Blocking: name required, kebab-case, unique across scopes; description required; prompt non-empty;
no tool name outside the known set **unless** Advanced is open (an MCP server may not be connected
at edit time).

Non-blocking (warnings): a description over 200 chars, an unknown tool name, a model the provider
list does not currently know, and an agent whose file cannot be parsed (the row shows the error; the
detail column offers the raw text read-only so the user can repair it elsewhere).

### 10.6 Edits apply to the next delegation

A running child keeps the prompt and tools it started with. The page says so once, under the Prompt
field: "changes apply to new delegations". Nothing about a live run is rewritten underneath it.

### 10.7 Limits panel

Below the list (a collapsible section, not a second page): max concurrent, max tasks per run, max
children per parent session, timeout, project-agents toggle (phase 5, off), and — when
`pi-subagents` owns `subagent` (D-C) — one line naming it, with the page otherwise read-only.

---

## 11. File map

| Path | Change |
|---|---|
| `contrib/orbit-subagents-extension/index.js` | **new** — activation, stand-down check (D-C), tool registration, run orchestration, abort/timeout |
| `contrib/orbit-subagents-extension/agents.js` | **new** — frontmatter subset reader, builtin + user discovery, override rules |
| `contrib/orbit-subagents-extension/builtins.json` | **new** — shipped scout / worker / reviewer definitions (name, description, model, tools, prompt); the extension reads it beside itself, Orbit `include_str!`s the same file (D-X) |
| `contrib/orbit-subagents-extension/run.js` | **new** — argv + env build, JSONL parse, phase/current/tool tallies, details shaping, 400 ms update coalescing (§9.7), usage sum, report caps |
| `contrib/orbit-subagents-extension/mode.js` | **new** — read `~/.orbit-pi/workflow.json` for a session id (twin of the workflow extension's reader) + `subagents.json` config with defaults |
| `contrib/orbit-subagents-extension/resources.js` | **new** — the child's guardrail paths from `ORBIT_BUNDLED_EXTENSIONS` |
| `contrib/orbit-subagents-extension/*.test.mjs`, `package.json` | **new** — `node --test`, zero deps |
| `contrib/orbit-guard-extension/policy.js` | `summarize()` case for `subagent` (§9.3) and the `ORBIT_SUBAGENT_CEILING` clamp in `decide()` (§D-Q) + tests |
| `crates/orbit-pi/src/bundled_extensions.rs` | install the extension (multi-file), add it to `extension_paths()`, set `ORBIT_BUNDLED_EXTENSIONS` on every spawn |
| `crates/orbit-pi/src/subagent.rs` | **new** — §6.3 parse/summary + tests |
| `crates/orbit-pi/src/subagent_file.rs` | **new** — §6.5 frontmatter parse + surgical write, round-trip tests |
| `crates/orbit-pi/src/transcript.rs` | `ToolCall.details` (§6.4) + tests |
| `crates/orbit-pi/src/transcript_view.rs` | `render_subagent_card` (§9.1–9.4), dispatch beside the ask card, `working_activity_label` forwarding (§9.5), `activity_title` delegation count (§9.6), `activity_action_label` verb |
| `crates/orbit-pi/src/context_meter.rs` | `format_cost` → `pub(crate)` for the card header |
| `crates/orbit-pi/src/subagents.rs` | **new** — config store `~/.orbit-pi/subagents.json` (lenient, atomic), twin of `workflow.rs` |
| `crates/orbit-pi/src/app/subagents_ui.rs` | **new** — the master-detail page of §10: list, detail form, validation, actions enum, controllers |
| `crates/orbit-pi/src/app.rs` | `SettingsSection::Subagents`, page fields (selection, filter, draft, dirty flag), nav wiring |
| `crates/orbit-pi/src/commands.rs` | "Manage subagents" action + key + palette row (D13) |
| `crates/orbit-pi/src/app/settings.rs` | section entry + page dispatch for the new page |
| `crates/orbit-pi/src/lib.rs` | module declarations, re-exports |
| `crates/orbit-pi/locales/en.yml` | `subagent.*`, `settings.subagents.*` keys (D9: only `en.yml` is hand-edited) |
| `crates/orbit-pi/docs/subagents.md` | **new** — user one-pager: agents, modes, what children may do |
| `AGENT.md` | file map rows for the new modules + `contrib/` dir |
| `INTENT.md` | new **D17**, *Implementation status* |
| `README.md`, `CHANGELOG.md` | feature bullet, `[Unreleased]` entry |

---

## 12. Phased delivery

Each phase ships on its own.

### Phase 1 — one child, no card (~2 days)

`agents.js`, `builtins.json`, `run.js`, `mode.js`, `resources.js`, `index.js` with **single mode
only**; `ORBIT_BUNDLED_EXTENSIONS` in `BundledExtensions::spawn`; usage on the result. The tool call
renders as the existing generic activity card — no Rust work.

**Gate:** on a real repo, "use scout to find where auth is wired" returns a report, the session's
cost meter grows by the child's cost, `ps` shows the child during the run and none after, and an
abort (stop button) leaves no `pi` process behind. In Plan mode, a `worker` child's `edit` call is
denied with a blocked-tool reason rather than hanging.

### Phase 2 — the card (~3 days)

`ToolCall.details`, `subagent.rs` (including `headline` / `live_activity` / `status_phrase`),
`render_subagent_card` with all three zoom levels, the phase table of §9.4, the blocked banner and
log, `working_activity_label` forwarding, `activity_title` delegation count, i18n keys, settings read
for `enabled`.

**Gate:** the row names the child's current tool while it runs and its counts when it settles; the
parent's working line reads `scout is grepping src/ · working for 34s`; the group header reads
`Delegated to 1 agent`; expanding shows real steps and a blocked call is labelled with the mode; a
hand-written foreign `details` payload falls back to the generic card; a live run does not churn list
layout and does not steal focus.

### Phase 3 — chain, parallel, caps (~2–3 days)

Sequence with `{previous}`, the concurrency pool, all caps, guard `summarize`.

**Gate:** 3 parallel children never exceed 4 concurrent and each row names its own current tool; the
9th task is refused with a readable message; a chain passes the first report into `{previous}` and
shows `step 2 of 3`; a child failing mid-chain stops the chain and names the step; the 25th child in
one session is refused.

### Phase 4 — the management page (~3 days)

`subagent_file.rs`, `subagents.rs`, `app/subagents_ui.rs`, `SettingsSection::Subagents`, the ⌘P row,
the card's "Edit agent" action; **full CRUD (D-X)** — add, edit, rename, disable, delete with Undo,
reset to builtin; capability tier → `tools:` **and** `ORBIT_SUBAGENT_CEILING` with the guard clamp;
per-agent mode; model; description count; prompt editor; the model-facing preview; the limits panel.

**Gate:** an agent created on the page is offered in the tool description on the next delegation
with no restart; it can be edited, renamed, disabled, deleted (Undo restores the exact bytes), and
reset to builtin, each leaving the expected file and the expected row; dropping an agent from *Edits*
to *Read only* removes `edit` from the next child's `--tools` **and** an `edit` attempt in that child
is denied with a ceiling reason even in Full access; a hand-added `memory:` key and a comment survive
a save byte-for-byte; a file changed elsewhere is never overwritten without the explicit choice.

### Phase 5 — safety and honesty pass (~2 days)

End-to-end mode matrix (Supervised / Auto-accept / Full × Plan / Build / Ask), project-agent toggle
behind trust, `pi-subagents` stand-down message, docs, CHANGELOG, INTENT D17.

**Gate:** a child in every access mode does exactly what the mode auto-approves and nothing else;
installing `pi-subagents` and restarting shows one tool, owned by it, and Settings says so.

### Phase 6 — inspectable runs (optional, ~2 days)

Children spawn with `--session-dir ~/.orbit-pi/subagents/<parentSessionId>/` instead of
`--no-session`, and the card's report block grows an "open transcript" action that reuses
`sessions.rs` to read that file into a read-only viewer. Nothing appears in the sidebar.

**Gate:** opening a finished child shows its real transcript; deleting the parent session's
subagent dir is not required for a clean uninstall (documented).

---

## 13. Risks

| # | Risk | Mitigation |
|---|---|---|
| 1 | **Runaway fan-out** (a prompt that spawns forever) | depth 1, 8/run, 4 concurrent, 24/session, all enforced in the extension and reported in text; no config path to "unlimited" |
| 2 | **A child doing things the parent's mode would have asked about** | D-E: guard + workflow extensions are passed, mode is inherited, non-UI prompts are denied. The card names the mode. It is a guard, not a sandbox — say so in the docs |
| 3 | **Prompt injection via project agents** (`.pi/agents`) | not loaded in v1 (D-D); phase 5 behind trust, off by default |
| 4 | **Cost surprise** — delegated work is real spend | usage on every result (D-H), per-child usage on the card, one Settings line that says children bill the same credential |
| 5 | **Orphan child processes** after a crash/abort | `SIGTERM`→`SIGKILL` on abort, hard timeout, kill on extension deactivation, and a node test that asserts the kill path |
| 6 | **Two `subagent` tools** if `pi-subagents` is installed | D-C stand-down + one honest Settings line; read `pi.getAllTools()` at activation |
| 7 | **Child cannot start in a bundled `.app`** (no `node` on `PATH`) | reuse the parent's already-augmented `PATH`; surface the stderr tail in the card, never a silent empty report |
| 8 | **`--mode json` event shape drifts in a future pi** | parse defensively (unknown events ignored, no throw), keep fixtures of real pi output in `node --test`, and fail with a readable "child produced no assistant message" rather than a blank report |
| 9 | **`details` growth in memory** (many live children) | bounded at the source (8 steps, 160-char previews) and again in Rust (128 KiB, steps capped) |
| 10 | **Two children editing one file** (D-N) | documented; parallel mode is described as "independent tasks", and the tool description says so |
| 11 | **Layout churn while streaming** | D-K: streaming content lives only in the expanded card; the collapsed row is a constant-height line |
| 12 | **The page and the file drift** (hand edits, another tool writes it) | hash + mtime captured on load; a save refuses to clobber a changed file without an explicit Reload/Overwrite (D-U) |
| 13 | **Clobbering another extension's frontmatter** (`extensions:`, `skills:`, `memory:`) | surgical writer (D-U) + round-trip tests asserting byte identity when nothing changed |
| 14 | **The capability tier and the real tool list disagree** | one input, two uses (D-Q): the tier writes `tools:` and is passed as the child's ceiling; a test asserts the argv and the env agree for every tier |
| 15 | **A pinned agent mode weakens Plan** | D-R intersection; the card names the effective mode |
| 16 | **Context cost creep** — agent descriptions ride in the tool description on every turn | 200-char cap with a live count, enabled-only inclusion, and the preview block showing the real cost (D-S) |
| 17 | **Disabling an agent here does not disable it for `pi-subagents`** | the page says so when that package owns the tool (D-C); Orbit's config governs Orbit's tool only |
| 18 | **A user override quietly diverges from the shipped builtin** | *modified* badge on the row, the shipped text viewable, and *Reset to builtin* one click away (D-X); updates replace `builtins.json` only, never a user file |
| 19 | **A status that reads as more than it knows** (a spinner implying progress) | D-V: phases, counts and elapsed only, silence named as silence; no % / ETA anywhere |
| 20 | **Update flood** — a child with 200 tool calls pushes 200 `onUpdate` frames | 400 ms coalescing per child plus one per transition; a node test asserts the count against a 300-event fixture |
| 21 | **A long report stalls a frame** | display-capped Markdown, one open child log at a time, report rendering only after settle |
| 22 | **Focus or layout churn while a child streams** | fixed-height row (D-K), no focus movement on update (§9.8) |
| 23 | **Localization drift** | new keys land in `en.yml` only (D9); `scripts/check_i18n.py` stays green |

---

## 14. Verification

**Node, no network** (`cd contrib/orbit-subagents-extension && node --test`):

- `agents.js` — frontmatter subset (plain, bracketed list, comma string, missing keys, unknown keys),
  a malformed file skipped without losing the others, builtin→user override, kebab-case name check.
- `run.js` — argv for inherit vs pinned model, tools list present/absent, `--no-session` and
  `--mode json -p` always present, temp prompt file path, env table of §8, JSONL parse of a canned
  child stream (report, usage sum, `stopReason: "error"`, unparsable line ignored, missing
  `message_end`), details bounding (steps, report, stderr, blocked, toolCounts), phase transitions
  for a scripted stream, and **coalescing**: 300 events inside one 400 ms window yield a bounded
  number of updates plus exactly one per state transition.
- `mode.js` — workflow store read for a session id, `ORBIT_WORKFLOW_MODE` fallback, defaults for a
  missing/malformed `subagents.json`, caps never above the hard maximum.
- `index.test.mjs` — abort kills (fake spawn records signals), depth-1 refusal, cap refusals,
  stand-down when `getAllTools()` already lists `subagent`.
- `contrib/orbit-guard-extension/policy.test.mjs` — new `summarize("subagent", …)` case, and the
  ceiling clamp: `read` blocks `edit`/`bash`/`mcp__*`, `edit` blocks `bash`, `exec` blocks `mcp__*`,
  no ceiling behaves exactly as today.

**Rust, no network:**

- `subagent.rs` — parse a good payload, a foreign payload, an unknown version, missing fields,
  oversize steps, `headline()` for every phase, `live_activity()` non-`None` only while live,
  `status_phrase()` for running/blocked/failed/settled; no panics on `null` or absent optional fields.
- `transcript_view` — the card renders from a `ToolCall` with subagent details, from one without
  (generic fallback), and with a failed child (error strip present); `working_activity_label` returns
  the child's activity for a live `subagent` step and `None` for a settled one; `activity_title`
  counts delegations instead of `Ran 1 tool`; only one child log is open at a time — the existing
  `TestAppContext` view-test pattern.
- `transcript.rs` — details retained on end and on partial update, oversize dropped, absent stays
  `None`.
- `subagent_file.rs` — parse/save round-trip is byte-identical when nothing changed; changing only
  `tools` touches one line and leaves a comment and a foreign key intact; a missing key is inserted
  in place without reordering; CRLF and odd spacing survive; a file with no frontmatter is reported
  rather than rewritten; an invalid name is rejected; the temp file never outlives a failed write.
- `subagents.rs` (config) — defaults, partial JSON, unknown agent entries preserved, caps clamped to
  the hard maximum, atomic write.
- `app/subagents_ui` — the page renders a user agent and a builtin (actions differ), a malformed
  file shows the read-only fallback, save is disabled while clean, and delete asks exactly once.
- `app/subagents_ui` (CRUD, D-X) — new agent writes one file and selects it; edit rewrites only the
  four keys; rename writes the new file and removes the old; disable records `subagents.json`; delete
  removes the file and Undo restores the exact bytes; reset removes the override and the row returns
  to *builtin*; a save is refused while the file on disk differs from the loaded hash.
- `subagent_file.rs` reads the same `contrib/orbit-subagents-extension/builtins.json` the extension
  reads — one test parses it on the Rust side and asserts the three builtins, their tiers and their
  tools, so a shipped default cannot drift between the page and the tool.
- `bundled_extensions.rs` — the extension installs its files and returns the entry; every file the
  entry imports exists next to it; the spawn env carries `ORBIT_BUNDLED_EXTENSIONS`.
- `crates/orbit-rpc` fake-pi test (the `quota_fake_pi.rs` pattern): a scripted `pi` records its
  argv/env — asserts the extension arg list and the new env var are forwarded.

**Manual, per phase gate** (§12). Plus the standing checks: `cargo test --workspace`,
`cargo clippy --workspace --all-targets`, `cargo fmt --check`, `python3 scripts/check_i18n.py`.

Optional, and worth it: a `ci.yml` job that runs `node --test` in each `contrib/*-extension`
(they already exist and are currently only run locally).

---

## 15. Open questions

1. **Name collision policy.** Stand down (D-C) or namespace ours as `orbit_subagent`? Standing down
   is friendlier; namespacing is more predictable. Decide before phase 1 ships.
2. **When a chain runs, does the parent see intermediate reports?** v1: no — only the final child's
   report, because the intermediates are the reason to chain (context savings). The card shows all.
3. **Should the stop button be able to kill one child while the run continues?** Not in v1 (abort =
   whole run). Parallel runs make it tempting; it needs per-child RPC the protocol does not have.
4. **Is `scout` too weak without `bash`?** It ships *Read only* in v1 (grep/find/ls cover most recon).
   If real use shows it needs `git log`, either raise it to *Runs commands* or add a separate
   `runner` agent — do not quietly widen the tier of the agent people delegate reconnaissance to.
5. **Do we want a per-run "budget" (tokens/cost) that fails the child?** Cheap to add later
   (usage is already summed); only worth it if cost surprises appear in practice.
6. **Should phase 6's child sessions be pruned with the parent session?** If yes, `sessions.rs`'s
   delete path grows a rule; if no, document the leftover directory.
7. **Is three tiers enough?** A `researcher` needs `webfetch` (a read tool by the guard's
   classification) — fine — but an agent limited to *reads plus one command* has no tier. Add a
   fourth only if a real agent needs it; the Advanced field covers it meanwhile.
8. **Should the card be pinnable** — a compact "2 agents running" strip above the composer while a
   long delegation runs, so the user can keep typing elsewhere and still see it? Section 9.5 covers the
   at-a-glance case for now; a pinned strip is the status-bar chip that would replace it.
9. **Does the collapsed row want live output tokens?** It shows input/output only when settled.
   Live token counts would move every update — the kind of churn §9.7 avoids. Leave it out.
10. **Is a completion notification (§9.9) worth it at all?** It is the only signal that reaches a
    user who walked away, but Orbit already has auto-title and delegation noise to be careful about.
    Default off unless a real long-run case appears.
11. **Should the page be able to try an agent** — a one-off delegation from the detail column, so a
    prompt or tier can be tested while editing? It spends money and needs somewhere to show the run;
    phase 6 territory.
12. **Does an app update ever touch a user's override?** No — updates replace the bundled
    `builtins.json` only, and the *modified* badge plus *Reset to builtin* keep the divergence visible.
    Worth stating in the docs when this ships.

---

## 16. Docs to update when this ships

Per `AGENT.md` §7, a feature is not done until the docs describe it.

- `INTENT.md` — new **D17 — Subagents: a bundled delegation tool over child `pi` processes** (or
  the next free number if `review-guide.md` lands first), plus *Implementation status*.
- `crates/orbit-pi/docs/subagents.md` — **new**: what an agent is, the three modes, the caps, what
  children may do under each access mode, the capability tiers and how they are enforced twice, where
  each setting lands (agent file vs `~/.orbit-pi/subagents.json`), how to add/edit/rename/disable/
  delete/reset an agent in Settings, and how to read the delegation card (phases, blocked banner, where
  a child's real steps live).
- `AGENT.md` — file map rows (`subagent.rs`, `subagent_file.rs`, `subagents.rs`,
  `app/subagents_ui.rs`, `contrib/orbit-subagents-extension/`).
- `README.md` — a Features bullet: delegate to a child agent, and manage agents (capabilities, mode,
  model, prompt) from Settings.
- `CHANGELOG.md` — `[Unreleased] → Added`.
- `PRODUCT.md` — only if delegation changes the stated capability list (it should: "delegate scoped
  work to a child agent").
