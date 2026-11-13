# Plan — Subagents (delegated child sessions)

**Status:** Phases 0–4 implemented with automated tests (the extension, agent discovery, all three
run shapes, the caps, the mode/ceiling pre-flight, child cost, the guard rule, the delegation card,
and the Settings → Subagents management page). Phase 5 (the mode matrix end to end) and Phase 6
(inspectable child transcripts) are not started. Manual verification is partial: real-pi
delegations have been exercised end to end, and the card and page have render coverage but have not
been inspected in the running app.
**Owner:** TBD
**Scope:** one bundled pi extension (`contrib/orbit-subagents-extension/`), a Settings → Subagents
master-detail **management page** (`subagent_file.rs`, `subagents.rs`, `app/subagents_ui.rs`), a
retained `ToolCall.details` field, and one guard-policy rule. **No `crates/orbit-rpc` wire change.**
**Related:** `bundled_extensions.rs` (materialization + spawn env), `transcript.rs` (`ToolCall`,
`tool_execution_update`), `transcript_view.rs` (`render_activity_card` dispatch), `ask.rs` +
`render_ask_card` (the precedent for a bespoke tool card), `app/skills_ui.rs` + `skills.rs` (the
master-detail page precedent this one copies), `mcp/config.rs` (atomic surgical file writes),
`context_meter.rs` (usage formatting), `access.rs` + `workflow.rs` + `contrib/orbit-{guard,workflow}-extension/`
(the two guardrails a child must inherit), `plugins.rs` (pi package discovery / `pi-subagents`
co-existence), `commands.rs` (D13), INTENT D1 (RPC over stdio), D8 (workflow modes), D9 (i18n),
D10 (extension bridge), D12 (Orbit-owned config), and **proposes INTENT D18** — D17 is taken by the
release check, and `docs/plans/review-guide.md` no longer exists.

**How to use this document.** §1–§10 are the design. §12 is the work plan: phases, ordered tasks,
and a gate per phase. §14 names the commands each phase must run. Work one phase at a time, in
order; each phase is releasable on its own, and stopping after any gate leaves the app coherent.

## Status at a glance

| Phase | Implementation | Automated tests | Manual gate |
|---|---|---|---|
| 0 — spike | Done (the seam was proven with a real `pi` rather than committed fixtures) | — | One real child run captured and inspected |
| 1 — one child | Done, plus chain/parallel from Phase 3 | `node --test contrib/orbit-subagents-extension/*.test.mjs` (85, including the real-`pi` seam), guard (31), `bundled_extensions` (6) | One real delegation returned its report and cost; abort and refusal paths covered by tests |
| 2 — the card | Done: `ToolCall.details`, `subagent.rs`, the three zoom levels | `subagent::` (13), transcript details (3), card render | Not inspected in the running app |
| 3 — fan-out | Done (folded into Phase 1) | Chain `{previous}`, 5-task pool ≤ 4 concurrent, depth/cap refusals | Real 5-child pool not exercised |
| 4 — the page | Done: `subagent_file.rs`, `subagents.rs`, Settings → Subagents | `subagent_file` (14), `subagents` (6), page render (2) | Not inspected in the running app |
| 5 — safety pass | **Not started** | Mode matrix unit-tested in `mode.test.mjs` | Full Access × Plan × tier matrix not exercised in the GUI |
| 6 — inspectable runs | **Not started** | — | — |

What is shipped is described for users in
[`crates/orbit-pi/docs/subagents.md`](../../crates/orbit-pi/docs/subagents.md).

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

It is not an orchestrator app, not a background job runner, and not a second copy of
`pi-subagents`.

### 1.1 What it saves, concretely

The scarce resource is the parent's context and the user's money. A delegation is worth it when the
child's reading stays in the child. The plan therefore optimizes for three numbers:

| Number | Target | Why |
|---|---|---|
| Tokens the parent receives per child | **≤ ~1k** (4 KiB report cap, D-G) | The report is paid for on every subsequent turn of the session |
| Tokens the parent would have spent reading | **≥ 10×** the report for the delegation to be worth it | The tool description says so; the card shows the real cost |
| Prompts the user must answer per delegation | **0** | The child is clamped by the session's own mode (D-E, D-Z); an extra approval would approve nothing |

The honest acceptance case is the one in the Phase 1 gate: on the Orbit repo, "use scout to find
where extensions are loaded" must come back under ~1k tokens and under a cent, where doing it
inline costs tens of thousands of tokens. If that case does not reproduce, the feature is not
working, whatever the card looks like.

### 1.2 What "done" looks like for a user

1. The model delegates without being told to, because the tool description says when it pays.
2. The user sees *which agent is working, on what, for how long and at what cost* without opening a
   window or reading a log.
3. A delegation that cannot succeed is refused **before it spends anything**, with the reason and
   the remedy.
4. An agent can be created, edited, renamed, disabled, deleted and reset in Settings, and the next
   delegation uses it with no restart.

---

## 2. Why this fits Orbit

Almost all the machinery exists. The table is the feasibility argument, with the symbol that
already does the work.

| Need | Already in Orbit | Gap |
|---|---|---|
| Register a model-callable tool | the bundled-extension pattern: `install_extension` (multi-file), five `contrib/` extensions loaded with `--extension` | new extension |
| Spawn a `pi` child with a resolved binary and a working `PATH` | `orbit_rpc::pi_binary()`, `augmented_path()` (`client.rs`); `commit_message.rs::run_pi` is the headless-spawn precedent | children spawn from inside pi, not from Rust |
| Live tool progress in the transcript | `Event::ToolExecutionUpdate` → `transcript.rs::on_tool_execution_update` (accumulated `partialResult`) | the extension must call `onUpdate` |
| A bespoke tool card | `render_activity_card` early-returns to `render_ask_card` for `ask_user_question` (`transcript_view.rs:3535`) | new card + dispatch |
| A master-detail manager page inside Settings | `SettingsSection::Skills` (`app/skills_ui.rs` + `skills.rs`): searchable list, detail column, actions enum, discovery/I/O split | new page, same shape |
| Surgical edits to a user-owned markdown file | `mcp/config.rs` (atomic, preserves what Orbit does not own) | agent-file writer (§6.5) |
| Structured tool data on the card | `ToolFacts::from_result` reads `details` | `details` is **dropped** by `normalize_tool_result` — needs a retained field (§6.4) |
| Clamping child tool calls | `contrib/orbit-guard-extension/` already **denies** when `ctx.hasUI` is false (`index.js:62`) | pass the ceiling (D-Q) |
| Refusing a run the session mode forbids | `access.rs` + `workflow.rs` are Orbit-owned stores with pure readers | compute the allowed ceiling (D-Z) |
| Keeping the child non-mutating in Plan/Ask | the workflow extension falls back to `ORBIT_WORKFLOW_MODE` (`index.js:73`) | pass it (D-E) |
| Cost/usage display | `orbit_rpc::MessageUsage`, `context_meter::{format_tokens, format_cost}`, the session meter (`SessionUsage::from_stats`) | tool result carries child `usage` (D-H); `format_cost` goes `pub(crate)` |
| Orbit-owned config next to the app | `access.rs`, `workflow.rs`, `session_defaults.rs` → `~/.orbit-pi/*.json` | new file |
| Localization | `locales/en.yml` + `tr!` (D9) | new keys |
| A palette row and a key | `commands.rs` `COMMANDS` / `BINDINGS` (D13) | one command |

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
- **Ceiling** — the maximum risk a child may reach, derived from its `tools:` line (D-Q).
- **Allowed ceiling** — the maximum risk the session's access and workflow modes permit (D-Z).

What pi gives us: `--mode json -p` streams one JSON object per line (`session`, `message_end`,
`tool_execution_*`, `agent_settled`, …) and exits after one prompt (`pi docs/json.md`). That is the
entire integration surface, and it is documented, not internal.

---

## 4. Decisions

### D-A — The feature is an extension plus a card; the RPC wire does not change

`crates/orbit-rpc` stays a transport (`AGENT.md`: "transport, not a place for UI, HTTP, or runtime
helpers"). The tool call arrives as an ordinary `tool_execution_*` event, its `details` carry the
run state, and Orbit renders it. Nothing in the protocol has to learn what a subagent is.

**Consequence:** no fake RPC capability, no version negotiation, and the feature works against any
pi that supports `--extension`, `--mode json`, and `tool_execution_update`.

### D-B — Each delegation is a real `pi` process, one per child, `--no-session`

Mirrors pi's own example extension (`--mode json -p --no-session`) and `commit_message.rs`.

**Consequence:** children leave nothing behind to clean up, cannot be reopened, and cannot appear
in the sidebar. Phase 6 optionally trades this for inspectable runs (§12.7).

**Consequence:** a cold child costs Node startup (a few hundred ms) per delegation. Accept it; the
warm pool in `sessions` exists for user-visible sessions, not for delegated work.

### D-C — The tool is named `subagent`, and Orbit stands down if something else provides it

The name is what prompts and muscle memory already use. The extension asks `pi.getAllTools()` at
`session_start` — **not** while it loads — and if a tool named `subagent` is already registered by
some other file (for example `pi-subagents` is installed), Orbit's extension **does not register**
and says so, naming the owner. Two implementations racing for one name is worse than one honest
message.

**Why `session_start`, in one paragraph (issue #122).** pi 1.5 treats two extensions registering one
tool name as a fatal load error: it records `Tool "subagent" conflicts with …`, then exits before
serving the session — so every Orbit session died, not just the tool. Orbit loads its extensions with
`--extension`, which pi appends *after* discovered extensions and packages, so Orbit was always the
second registration. The check that should have prevented that ran at load, where `getAllTools()` is
an unbound stub that throws; the guard swallowed the throw, read it as an empty list, and registered
anyway. At `session_start` the list is bound and every entry carries `sourceInfo.path`, so a foreign
owner is distinguishable from our own earlier registration in the same process. The rule that came
out of this: **an unreadable tool list is not an empty one, and nothing registers on a guess.**

**The scan no longer protects us.** Once nobody registers at load, pi's conflict check has nothing to
compare, so a missed ownership check cannot crash the app — it does something quieter. pi keeps the
*first* registration for a name, and discovered extensions sort before `--extension` ones, so a
shadowed Orbit registration is inert while the app believes the feature is on. That is why the check
is repeated at every `session_start` and followed by a read-back: if `subagent` in the registry is
not ours, Orbit records that it stood down instead of reporting a catalog with no effect.

**Where the page learns it.** The extension writes `~/.orbit-pi/subagents-status.json` at every
`session_start` (`{ active, tool, reason, owner, updatedAt }`), and Settings → Subagents reads it
when the section opens. `subagents::load_status` returns `None` for a missing or unparsable file —
absence is not a verdict, so the page claims nothing until a session has actually decided. A banner
above the list names the owner and links to Settings → Plugins; the agent list stays editable,
because those files are what the owning extension reads too. The extension also raises a toast with
the same sentence, for the user who is not in Settings when it happens.

**Decided here rather than left open:** standing down, not namespacing. A `orbit_subagent` name
would be predictable for us and unknown to every prompt the user already has. The cost of standing
down is that the feature is absent, which the page states; the cost of namespacing is that the
feature is present and never called.

### D-D — Agents are markdown in pi's own directories; v1 loads user scope only

`~/.pi/agent/agents/*.md`, frontmatter `name` / `description` / `model` / `thinking` / `tools` —
the path and the keys `pi-subagents` reads, so an agent written for one works in the other. The pi
CLI itself has no agents directory and no `--agent` flag: this is one package's convention, adopted
because it is the one users already have files in. Builtins are embedded in the extension (as
`builtins.json`, D-X), user files override them by name — also `pi-subagents`' resolution.

`.pi/agents/*.md` (project scope) is **not** loaded in v1: those are repo-controlled prompts that
can instruct a model to run anything. pi's own example defaults to user scope for the same reason.
Phase 5 may add project scope behind Orbit's project trust, off by default.

Frontmatter is read by a ~20-line reader for exactly these five keys, not by a YAML parser
(`AGENT.md`: `contrib/` is zero-dependency). A malformed file is skipped with a diagnostic; it
never takes down discovery of the others.

### D-E — Children inherit the parent's guardrails; they never prompt

A child is a *new* session, so it is not automatically covered by anything Orbit passes to the
parent. Three things are therefore made explicit:

1. **Extensions** — Orbit sets `ORBIT_BUNDLED_EXTENSIONS` (a JSON array of absolute paths) on every
   session spawn in `BundledExtensions::spawn`. The subagent extension passes each path on to the
   child as `--extension`. The list is the **two guardrails** (guard + workflow), not every bundled
   bridge: a child needs the parent's permissions and its Plan/Build/Ask scope, and forwarding the
   quota, title or suggestions bridges would make every delegation fetch provider quota or spend a
   model call on a session title nobody sees.
2. **Workflow mode** — read fresh from `~/.orbit-pi/workflow.json` for the parent's session id at
   call time (the workflow extension's own store, so there is one source of truth), then handed to
   the child as `ORBIT_WORKFLOW_MODE=<mode>`.
3. **Access mode** — nothing to pass. The guard reads `~/.orbit-pi/access.json` fresh on every tool
   call, so a child sees the same mode the parent does.

Because json mode has no UI, the guard's existing fail-closed path takes over: a call the mode
would *ask* about is **denied** in the child
(`if (!ctx?.hasUI) return { block: true, reason: "Blocked: Orbit has no UI to confirm this action" }`).
And the workflow extension's `tool_call` hook blocks write tools in Plan/Ask regardless.

**Consequence, stated in the UI:** children can only do what the current access mode auto-approves.
Supervised → read-only children. Auto-accept edits → children may edit, cannot run commands.
Full access → children are unrestricted, exactly like the parent. This is a guard, not a sandbox
(`INTENT.md` on access modes), and the card says which mode the child ran under.

**Consequence — the child's tool universe is wider than the tier.** A child is a normal `pi` in the
same trust context, so it also loads every extension pi auto-discovers from
`~/.pi/agent/extensions/` and, under `--approve`, the project's `.pi/extensions/` — Orbit's bundled
extensions are passed explicitly with `--extension`; everything else the user installed is present
in the child too. So the ceiling (D-Q) is the only thing that bounds what a child *can do* —
`--tools` narrows what is *declared*. And depth 1 (D-I) is enforced by this extension reading
`ORBIT_SUBAGENT_DEPTH`; it does not stop a third-party delegation extension loaded in the child, so
Full access plus `pi-subagents` installed means a grandchild is possible. The docs say this instead
of implying that depth is absolute.

### D-F — Model and thinking inherit; an agent may pin a model

Default: the parent's current model and thinking level (`ctx.model`, `ctx.thinkingLevel`), passed
explicitly so a child never silently falls back to a provider default. An agent's `model:` wins,
and its `thinking:` sets the level. The card shows the model the child **actually used** (read from
its `message_end` events), never the requested one — pi clamps a level the model does not support.

### D-G — The parent sees reports only, capped

`content` returned to the parent model is: one line per child (`agent`, status, usage) plus each
report, capped at **4 KiB per child** (~1k tokens) with an explicit `… truncated (N KiB) — full
report in the card` marker. 16 KiB was the first instinct and it defeats the purpose: at roughly
four characters per token it is ~4k tokens per child, so three children would put ~12k tokens into
the parent's context — most of what delegating was meant to save. The full report (≤ 64 KiB) lives
in `details` for the card, where the user reads it on demand.

Never returned to the parent: the child's transcript, tool arguments, or file contents. That is the
whole point of delegating.

### D-H — Cost is real, so it is reported

Children spend the user's credential. The tool result carries `usage` — the summed child usage —
including failed, aborted and timed-out runs (their tokens were spent too). pi adds tool-result
usage to session totals, so Orbit's existing meter (`get_session_stats` → `SessionUsage::from_stats`)
and the per-session cost stay honest with no new plumbing. Per-child usage appears on the card.

**Known gap:** the meter reads pi's `get_session_stats`, but a turn's footer usage
(`Turn::usage`, `transcript.rs:270`) sums *assistant step* usage and never sees a tool result's
`usage`. So a delegation's cost lands in the session meter and not in that turn's line. Either sum
tool-result usage into `Turn::usage` in §6.4's change, or accept one under-reported turn per
delegation — but do not let the plan imply both are covered.

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
    "worker": { "enabled": true, "mode": "inherit" },
    "reviewer": { "enabled": true, "mode": "plan" }
  }
}
```

Orbit-only facts (`enabled`, `mode`) live here because they do not exist in a portable agent file
(D-P). Everything else about an agent — its name, description, model, thinking, tools and prompt —
lives in the markdown file, which is also what `pi-subagents` reads.

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

An agent is a file in `~/.pi/agent/agents/`. The page reads and writes **that file**, so
`pi-subagents` and Orbit load the same definition and nothing drifts between "what Orbit thinks an
agent is" and "what the child runs".

Only two facts cannot live in a portable file — whether the agent is enabled, and which workflow
mode it is pinned to — so those live in `~/.orbit-pi/subagents.json` (D-M). Thinking level *is*
portable (`thinking: high`, `pi-subagents`' own key), so it goes in the file with the model. The
page presents one form; §6.5 says which store each field lands in.

### D-Q — "What this subagent can do" is a capability tier, written once and derived everywhere

Three page-facing tiers, each a fixed tool set:

| Tier | `tools:` written | Guard ceiling |
|---|---|---|
| **Read only** | `read, grep, find, ls` | `read` |
| **Edits files** | + `edit, write` | `edit` |
| **Runs commands** | + `bash` (`+ powershell` on Windows) | `exec` |

The **ceiling is derived from the `tools:` line**, not stored a second time: `ceilingForTools()`
maps the declared names to `read` / `edit` / `exec` / `all` (any name it does not recognise, MCP or
extension tool, makes it `all`; an **absent** line is `exec`, because pi's default set includes
`bash`). That is what makes a hand-written file and a page-authored file behave identically, and it
is why phases 1–3 work before the page exists.

The tier itself is only the page's input to that line. It writes `tools:`, and the derived ceiling
rides to the child as `ORBIT_SUBAGENT_CEILING`, which the guard clamps `classify(tool)` against. A
read-only agent therefore cannot edit **even under Full access**, and a tool the agent never
declared is denied as well as unavailable.

Two corrections the pi CLI forces:

- **`--tools` narrows what is *declared*, not what is *reachable*.** pi keeps MCP tools callable
  through `codemode` exposure whatever the list says. So the argv also carries
  `--exclude-tools 'mcp__*'` for the read and edit tiers, and the ceiling is what actually holds. A
  read-only tier that relies on `--tools` alone is a declaration, not a boundary.
- **The tier names must be pi's real tool names.** There is no `glob` tool — `find` takes the glob
  patterns — and Windows uses `powershell`. The guard's vocabulary already knows `glob`, `list`,
  `webfetch` and `powershell` because other agent CLIs use them, but only pi's names belong in
  `tools:`; an unknown name is a shipped default that does nothing.

An advanced field accepts a custom tool list (MCP servers, extension tools). Any name outside the
table makes the derived ceiling `all`, and the page says so inline — a custom tool cannot be
silently narrower than it is. A hand-written file may also use `pi-subagents` forms this reader does
not understand (`mcp:server/tool`, `tools: inherit`, `excludeTools:`); those derive `all`, which is
the honest outcome rather than a silent narrowing.

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

The registration also carries pi's `promptSnippet` and `promptGuidelines`: the snippet puts the
tool's name and purpose in the `<tools>` section, and the bullets route review and recon requests
to `reviewer` and `scout` in `<rules>`. Both are fixed text except for those agent names, which
follow the enabled set for the same reason the description does — a disabled agent must not be
advertised. The page preview still renders the description only; the snippet and bullets are not
shown there.

**Consequence — the catalog is part of the request prefix.** A tool description is fixed at
registration, so a new or edited agent cannot reach the model by being rediscovered at call time.
The extension re-registers when the enabled set or a description actually changes, which costs one
prompt-cache miss for the rest of that session — acceptable because edits are rare, and the reason
the page says *used from the next delegation* rather than *next turn*. The preview block is labelled
as what the model sees **after** the next change, not what the current session already carries.
`pi-subagents` takes the other road — an agent catalog in its own prompt *section*, appended as a
small system message when it changes, so the cache survives. Orbit's simpler description-in-tool is
the price of not owning the system prompt; say it once rather than pretending the catalog is free.

### D-T — The page is Settings → Subagents, master-detail, with three ways in

Same shell as Settings → Skills (searchable list, detail column, one action enum, discovery and
file I/O in a non-UI module): `SettingsSection::Subagents`, a ⌘P row "Manage subagents" (D13
command registry), and an "Edit agent" action on the transcript card. Full-bleed inside Settings,
not a top-level surface — a top-level Subagents surface is what phase 6's run inspection would
become, if it ever ships.

### D-U — Writing an agent file is surgical and atomic

The writer replaces only the five keys Orbit owns (`name`, `description`, `model`, `thinking`,
`tools`), keeps every other frontmatter key intact (another extension's `extensions:`, `skills:`,
`memory:`), keeps comments, key order, and the body byte-for-byte, and lands with temp-file + rename
(the `mcp/config.rs` rule).

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
| **Edit** — user agent | Rewrites the five keys + body in that file (D-U) |
| **Edit** — builtin | Writes `~/.pi/agent/agents/<name>.md`, a user file that **overrides** the builtin (pi's own resolution). The shipped text stays available, so *Reset to builtin* always works |
| **Rename** | Writes the new file, removes the old one, after a confirm that says the agent's identity changes |
| **Disable** — builtin | Records it in `subagents.json → disabled`; the model no longer sees it. There is no file to delete, so the button does not claim to delete one |
| **Delete** — user agent | Removes the file (inline confirm), keeping the bytes for the toast's **Undo** |
| **Reset to builtin** | Removes the override; the shipped definition returns |

**Consequence — three labels, three real outcomes.** A builtin row offers *Disable*, an override row
offers *Reset to builtin*, a user row offers *Delete*. "Delete" never means two different things,
and every one of them is reversible in one click.

**Consequence — the names are shared with `pi-subagents`.** Its builtins are `scout`, `worker`,
`reviewer`, `researcher`, `oracle`, `delegate`, and it resolves a user file over its builtin exactly
as this page does. Writing `~/.pi/agent/agents/scout.md` — the *Edit builtin* and *Reset to builtin*
paths — therefore also replaces `pi-subagents`' `scout` for anyone running both. Orbit's builtins
keep their three names because they are the ones people already type, but the confirm copy for
those two actions names the other reader: "overrides the shipped `scout` in Orbit and in
`pi-subagents`."

**Consequence — one source for the shipped text.** The builtins live in
`contrib/orbit-subagents-extension/builtins.json`; the extension reads it next to itself and Orbit
`include_str!`s the same file, so the page can show the shipped definition, mark a row *modified*, and
render the override against it. Two copies of a prompt that disagree would be worse than no reset.

**Consequence — changes land on the next delegation.** The extension rediscovers agents per call, so
no restart is needed, and a running child keeps the prompt it started with (§10.6).

### D-Y — The child is told what it is and what comes back

A child is a fresh session with no memory of this conversation and no idea of its own limits, so two
fixed blocks ride in the appended system prompt (both borrowed from `dsh-subagent`'s design, both
cheap and prefix-stable for the child's lifetime):

**Scope** — `Your permission scope was fixed when you started and cannot be widened from inside this
session; operations that need approval are rejected automatically. When the job needs access beyond
that scope, do not retry the denied operation — state the limitation in your reply so the delegating
agent can handle it.`

**Return guidance** — `Your final answer is delivered to your parent. Include the result and the
evidence in that answer: the parent does not receive your transcript, tool output, or reasoning.`

Without the first, a ceiling denial (D-Q) becomes a retry loop and a confusing report; without the
second, a child either buries the answer in prose the parent never sees or burns tokens restating
tool calls it thinks someone is reading. Both are one fixed block per child, so the cost is known
and constant — and they are written into the temp prompt file, never into the tool arguments.

### D-Z — A delegation that cannot succeed is refused before it spends anything

The child is clamped by the session's own modes (D-E), so a `worker` (exec tools) started in
Supervised or in Plan would spend tokens and then be denied on its first command. That is the worst
of both worlds: money spent, nothing done, and a blocked banner the user has to interpret.

The extension therefore computes the **allowed ceiling** before spawning:

| Access mode | Workflow mode | Allowed ceiling |
|---|---|---|
| supervised | build | `read` |
| auto-accept-edits | build | `edit` |
| full-access | build | `exec` (and `all`) |
| any | plan / ask | `read` |

A run whose derived ceiling exceeds the allowed ceiling is **refused with a text result** naming the
agent, its ceiling, the session's mode and the remedy — `worker runs commands; this session is
supervised. Switch to Full access, or delegate to a read-only agent.` No process is spawned, no
tokens are spent, and the card shows the refusal as a failed run with that sentence.

**Consequence — the guard never prompts for `subagent`.** `classify("subagent")` becomes its own
`delegate` kind, and `decide` returns `allow` in every mode. Justification: the child cannot exceed
the session's mode (D-E), and the extension refuses runs it could not finish (above). A prompt
would be asking the user to approve something that is already fully constrained, which is friction,
not safety. The card names the mode the child ran under, so the fact is still visible.

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
tools: read, grep, find, ls
---

You are a scout. Find, do not change. Return a compressed map: …
```

- `name` (required, kebab-case, unique) — how the model names it.
- `description` (required) — appears in the tool description, so it decides whether the model picks
  this agent.
- `model` (optional) — `provider/model`; absent → inherit (D-F).
- `thinking` (optional) — pi's level set (`off`/`minimal`/`low`/`medium`/`high`/`xhigh`/`max`);
  absent → inherit (D-F). This *is* a portable key: `pi-subagents` reads `thinking:` in the same
  frontmatter, so the file carries it and Orbit passes the same value to the child as `--thinking`.
  The page writes `thinking:` rather than a `:high` suffix on `model:`, so the two stay independently
  editable; pi clamps a level the chosen model does not support, and the card reports what the child
  actually ran with.
- `tools` (optional) — comma list or bracketed list; absent → pi's defaults. **The management page
  writes this line from the capability tier (D-Q)**, and the child's ceiling is derived from it, so
  a page-authored file and a hand-authored one behave the same; a hand-written list above the tier
  is honored and the page shows it as Advanced.
- Body — appended system prompt (`--append-system-prompt`).

Builtins shipped in the extension, defined in `builtins.json` beside it (read by the extension, and
`include_str!`d by Orbit so the page can show and reset the shipped text — D-X): **scout** (read
only — recon), **worker** (runs commands — implementation), **reviewer** (read only — review).
Three, not seven; each earns its place or it is not added. Scout deliberately cannot edit, so a
delegation for "find and fix" belongs to `worker`.

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
      "stopReason": "endTurn",
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
      "report": null,
      "error": null,
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
settled, killed, refused}` (§9.4); `stopReason` is pi's own value copied verbatim — never inferred from the
report text, and `"endTurn"` with an empty `report` is an honest *done*, not a failure (D-Y, §8).
New optional fields are additive inside `subagent/1` — older or narrower payloads still render,
with the card showing only what it was told (D-L).

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
    pub phase: Phase,                  // Queued | Spawning | Thinking | Tool | Blocked | Settled | Killed | Refused
    pub current: Option<String>,       // "grep /auth/ in src/" — the row's middle slot (§9.1)
    pub tools: u64,                    // total tool calls
    pub tool_counts: Vec<(String, u64)>, // ≤12, hover tally
    pub blocked: Vec<BlockedCall>,     // ≤5, drives the card banner (§9.2)
    pub capability: Option<String>,    // read | edit | exec | all — the tier the child ran under
    pub workflow_mode: Option<String>, // the effective mode (D-R)
    pub model: Option<String>,
    pub stop_reason: Option<String>,   // pi's own value, verbatim (§8) — never inferred
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
    /// What the parent's working line should say (§9.5), `None` when nothing runs.
    /// Facts, not a sentence: the view owns the words so they stay translatable.
    pub(crate) fn live_activity(&self) -> Option<Activity<'_>>;
    pub(crate) fn is_live(&self) -> bool;
    pub(crate) fn running(&self) -> usize;
    pub(crate) fn total_tools(&self) -> u64;
    pub(crate) fn usage(&self) -> Option<MessageUsage>;
}

pub(crate) enum Activity<'a> {
    Agent { agent: &'a str, action: Option<&'a str> },
    Agents(usize),
}

/// `0:34` / `1:02:03` from the stamped timestamps; `None` without a start.
pub(crate) fn format_elapsed(started: Option<i64>, ended: Option<i64>, now: i64) -> Option<String>;
```

Parsing rules: unknown keys ignored, absent optional fields stay absent (`None`) — the UI never
invents a count or a status (same rule as `ToolFacts`).

### 6.4 `transcript.rs` change

- `ToolCall.details: Option<Value>` — new field, set from the envelope in `on_tool_execution_end`
  and `on_tool_execution_update`, `None` when absent, `null`, or > 128 KiB.
- `ToolFacts::from_result` keeps its current job (header facts); it does not read subagent data.
- Optionally (D-H): add a tool result's `usage` to `Turn::usage` so the turn footer stops
  under-reporting the delegation.
- Regression tests: details retained from a result and from a partial; oversize details dropped;
  `normalize_tool_result` unchanged.

### 6.5 The agent file, and how the page writes it

`~/.pi/agent/agents/scout.md` — the page owns **five keys and the body**, nothing else:

```markdown
---
# reviewed by hand, keep this comment
name: scout                                   <- Orbit
memory: project                               <- another extension's key, preserved
skills: [pi-web-access]                       <- another extension's key, preserved
description: Fast local codebase recon.       <- Orbit
tools: read, grep, find, ls                   <- Orbit (derived from the capability tier)
model: anthropic/claude-haiku-4-5             <- Orbit (optional; absent = inherit)
thinking: medium                              <- Orbit (optional; absent = inherit)
---

You are a scout. Find, do not change. …      <- Orbit (the prompt body)
```

Save = re-read the file, replace those five lines in place (adding a missing key after
`description`, never reordering the rest), keep everything else byte-for-byte, write to a temp file
in the same directory, `rename` over the original. No YAML library, no full re-serialization — a
re-serialization would drop comments and reorder keys the file's other reader may care about.

Rust side (`subagent_file.rs`, pure): `AgentFile::parse(bytes)` → `AgentFile { name, description,
model, thinking, tools, body, raw: RawFrontmatter }` and `write_with(&self, fields) -> Vec<u8>`;
tests assert a round-trip with no changes produces identical bytes, and that a change touches only
the intended line.

---

## 7. The tool

Registration happens at activation. The description is built from discovery, so the model is not
guessing agent names:

```
Call this first for broad work: reviewing a diff, finding where something is wired, or a
second opinion. The child has its own context window; the parent pays only for the report.

Agents (built in, or files in ~/.pi/agent/agents):
  reviewer   read only · Read-only review of a change: correctness, edge cases, and what the tests miss.
  scout      read only · Fast local codebase recon — relevant files, entry points, data flow, risks.
  worker     runs commands · Implementation work in this workspace: edit files, run the checks, report what changed.

The capability shown is what the agent may do; do not ask an agent to exceed it.
Modes: single {agent, task}; chain {chain:[…]} with {previous}; parallel {tasks:[…]}.
The child sees only `task`, so put paths, decisions and the wanted result in it; it returns
a short report, not a transcript.

Skip it for one file you already know: a child starts cold and costs a process.
```

The same registration carries pi's prompt surface, so the tool is visible before the model starts
reading rather than only in its schema: a `<tools>` line
("Delegate a scoped task to a child agent with its own context window.") and `<rules>` bullets that
route work to an agent, naming `reviewer` and `scout` only while they are enabled:

```
- Use subagent when a job needs more reading than the answer needs writing; the child sees only `task`, so put paths and decisions in it.
- Use subagent with agent: "reviewer" when asked to review a diff or a change, instead of reading the whole change yourself.
- Use subagent with agent: "scout" to locate code across the repository before reading files one by one.
```

Execution outline:

1. Validate exactly one mode; unknown agent → error listing available agents (not a crash).
2. Read `~/.orbit-pi/subagents.json`; if `enabled` is false, return a plain refusal.
3. Resolve the effective mode (D-R) and the allowed ceiling (D-Z); refuse above depth 1, above the
   session cap, or when the agent's ceiling exceeds what the mode permits — **before** spawning.
4. Run children (1, a sequential loop with `{previous}`, or a pool of `maxConcurrent`).
5. Report per D-G, usage per D-H, `details` per D-K.

Every failure returns a *text* result the parent can act on (`agent "xyz" not found; available: …`,
`child exited 1: <stderr tail>`, `timed out after 30 min`, `worker runs commands; this session is
supervised`). Only an internal extension fault throws.

---

## 8. Spawning a child

Argv (built in one pure function, unit-tested):

```
pi --mode json -p --no-session
   [--model <provider/model>] [--thinking <level>]
   [--tools <a,b,c>] [--exclude-tools 'mcp__*']
   --extension <guard> --extension <workflow>
   --approve
   --append-system-prompt <tmpdir>/orbit-subagent-<id>.md
   -- "Task: <task>"
```

`--exclude-tools 'mcp__*'` rides along for the read and edit ceilings (D-Q): `--tools` does not
remove MCP tools reachable through `codemode` exposure, and a ceiling is only as good as what the
child's registry actually holds. There is deliberately no `--no-extensions` — the child needs the
provider extension for a pinned `--model`, so it also keeps every extension pi auto-discovers (D-E).

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
`finally`. On abort the temp dir is removed too. The file contains the agent body, the scope block
and the return-guidance block; the task itself rides argv as `Task: <task>`, so the child's system
prompt never restates the task and the file stays constant for the agent (D-Y).

Output parsing, against pi's `docs/json.md`:

- **Framing.** Strict JSONL split on LF over a UTF-8 decoder. Not `readline`: it also breaks on
  U+2028/U+2029, which are legal inside JSON strings. The first record is a `session` header;
  ignore it.
- **Facts.** `message_end` with `role: "assistant"` → text + `usage` + `stopReason`;
  `tool_execution_start` / `tool_execution_end` (fields `result`, `isError`, `durationMs`) → a step;
  `tool_execution_update` (`partialResult`) → nothing, that is the parent's channel; and
  `agent_settled { aborted }` → the authoritative idle signal. `agent_end` is not one: a retry,
  overflow recovery or compaction retry can follow it.
- **Tolerance.** A line that does not parse is ignored (never fatal — pi may add events). A stream
  that ends with no assistant `message_end` is `failed` with `child produced no assistant message`
  and the stderr tail.
- **Empty is not failed.** Exit code 0 with a clean `stopReason` and no text is `done` with
  `report: null`, and the card reads `no closing message for the parent`. A child that had nothing to
  add did its job; calling that `failed` would be a status the events do not support (D-V).
  Exit code ≠ 0, `stopReason: "error"`, or `agent_settled { aborted: true }` → `failed` / `aborted`,
  with the stderr tail attached.

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
- A refused run (D-Z) renders as a settled row with the refusal sentence, not as a spinner and not
  as an empty card.

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
- **Refusal line** when the run never started (D-Z): the sentence from the tool result, with the
  remedy, and no usage line.
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
| `settled` | `message_end` with a stop reason | `done` / `no closing message` / `failed: <error>` / `aborted` |
| `killed` | timeout or abort | `stopped after <elapsed>` |
| `refused` | never spawned (D-Z) | the refusal sentence |

A child that has produced no event for 15 s says `no activity for 15s` instead of pretending to work —
the same honesty rule that puts `waiting for the first step` where a skeleton would be.

### 9.5 The parent's live line

While a child runs the parent's turn is blocked, so its own working line is the user's main signal.
`working_activity_label` (`transcript_view.rs:5469`) today returns `thinking…` or nothing; extend it:
when the live step's last tool is a `subagent` call with a running child, return
`scout is grepping src/` (`2 agents working` for parallel). The line already carries elapsed time and
rate, so the user reads:

```
scout is grepping src/ · working for 34s
```

No new element, no second indicator — one honest sentence about the child.

### 9.6 The activity-group header

`activity_title` (`transcript_view.rs:2673`) counts the turn's steps by kind
(`Ran 7 commands · 4 thoughts`). A `subagent` call falls into `other` and reads `Ran 1 tool`, which
understates a three-agent run. Add a delegation count —
`Delegated to 3 agents · Ran 2 commands · 1 thought` — with new locale keys
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

### 9.10 The approval bar — there is none

`classify("subagent")` is its own `delegate` kind and is always allowed (D-Z): the child cannot
exceed the session's mode, and a run that could not finish is refused before it starts. So there is
no per-delegation prompt, no "Always allow this tool" entry, and no guard `summarize()` case to
write. The approval the user actually gets is the card: capability, mode and cost, per run.

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
   (`read grep find ls`), an **Advanced** disclosure for a custom tool list, and the ceiling
   sentence: "Runs read tools only. Orbit denies anything else in this child, even in Full access."
2. **Mode** — `Inherit the session` / `Plan` / `Build` / `Ask`, each with one line of what it does,
   plus the resolved rule (D-R) computed live against the current session's mode.
3. **Model** — `Session: <model>` (writes no `model:` line) or one of this machine's models from a
   select (writes `provider/model`); a pin the catalog cannot resolve keeps its own row instead of
   being displayed as inherited. **Thinking level** beside it: `Inherit` or an explicit level, written as
   the file's `thinking:` line (`pi-subagents`' own key, D-P) and passed to the child as `--thinking`.
   One field, one store — the page never holds a second copy in `subagents.json`.
4. **Description** — single line, live character count, cap warning at 200 (D-S), helper text
   "the model reads this to pick an agent".
5. **Prompt** — multi-line editor (the `composer` input handler, monospace off, UI font on), the
   field that becomes `--append-system-prompt`.
6. **What the model will see** — the generated tool-description block rendered verbatim, with its
   character count, labelled *after the next change* rather than as what the live session already
   carries (D-S). This is the page's honesty check: no claim about an agent that the model is not
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
| `contrib/orbit-subagents-extension/index.js` | **new** — activation, stand-down check (D-C), tool registration + re-registration (D-S), run orchestration, mode pre-flight (D-Z), abort/timeout |
| `contrib/orbit-subagents-extension/agents.js` | **new** — frontmatter subset reader, builtin + user discovery, override rules, `ceilingForTools()` |
| `contrib/orbit-subagents-extension/builtins.json` | **new** — shipped scout / worker / reviewer definitions (name, description, model, tools, prompt); the extension reads it beside itself, Orbit `include_str!`s the same file (D-X) |
| `contrib/orbit-subagents-extension/run.js` | **new** — argv + env build, UTF-8/LF JSONL decoders, prompt file assembly (D-Y), event fold, phase/current/tool tallies, details shaping, 400 ms coalescing (§9.7), usage sum, report caps |
| `contrib/orbit-subagents-extension/mode.js` | **new** — read `~/.orbit-pi/workflow.json` for a session id (twin of the workflow extension's reader) + `subagents.json` config with defaults, `allowedCeiling()` (D-Z) |
| `contrib/orbit-subagents-extension/resources.js` | **new** — the child's guardrail paths from `ORBIT_BUNDLED_EXTENSIONS` |
| `contrib/orbit-subagents-extension/*.test.mjs`, `package.json` | **new** — `node --test`, zero deps |
| `contrib/orbit-guard-extension/policy.js` | `classify("subagent")` → `delegate`; `decide` allows it (D-Z); the `ORBIT_SUBAGENT_CEILING` clamp in the decision path + tests |
| `crates/orbit-pi/src/bundled_extensions.rs` | install the extension (multi-file), add the `subagents` field, `extension_paths()`, set `ORBIT_BUNDLED_EXTENSIONS` on every spawn |
| `crates/orbit-pi/src/subagent.rs` | **new** — §6.3 parse/summary + tests |
| `crates/orbit-pi/src/subagent_file.rs` | **new** — §6.5 frontmatter parse + surgical write, round-trip tests |
| `crates/orbit-pi/src/transcript.rs` | `ToolCall.details` (§6.4) + tests; optional tool-result usage in `Turn::usage` |
| `crates/orbit-pi/src/transcript_view.rs` | `render_subagent_card` (§9.1–9.4), dispatch beside the ask card, `working_activity_label` forwarding (§9.5), `activity_title` delegation count (§9.6), `activity_action_label` verb |
| `crates/orbit-pi/src/context_meter.rs` | `format_cost` → `pub(crate)` for the card header |
| `crates/orbit-pi/src/subagents.rs` | **new** — config store `~/.orbit-pi/subagents.json` (lenient, atomic), twin of `workflow.rs` |
| `crates/orbit-pi/src/app/subagents_ui.rs` | **new** — the master-detail page of §10: list, detail form, validation, actions enum, controllers |
| `crates/orbit-pi/src/app.rs` | `SettingsSection::Subagents`, page fields (selection, filter, draft, dirty flag), nav wiring |
| `crates/orbit-pi/src/commands.rs` | "Manage subagents" `CommandId` + binding + palette row (D13) |
| `crates/orbit-pi/src/app/settings.rs` | section entry + page dispatch for the new page |
| `crates/orbit-pi/src/lib.rs` | module declarations, re-exports |
| `crates/orbit-pi/locales/en.yml` | `subagent.*`, `settings.subagents.*` keys (D9: only `en.yml` is hand-edited) |
| `crates/orbit-pi/docs/subagents.md` | **new** — user one-pager: agents, modes, what children may do |
| `AGENT.md` | file map rows for the new modules + `contrib/` dir |
| `INTENT.md` | new **D18**, *Implementation status* |
| `README.md`, `CHANGELOG.md`, `PRODUCT.md` | feature bullet, `[Unreleased]` entry, capability list |

---

## 12. Work plan

Each phase is one coherent change that ends at a green gate. Do not start a phase before the
previous gate passes; do not carry a phase's UI work into another phase's tests.

### 12.0 Ground rules for every phase

- **One source of truth per fact.** The ceiling derives from `tools:` (D-Q); the mode comes from the
  existing stores (D-M); the builtins come from `builtins.json` (D-X).
- **No invented state.** If an event was not seen, the card says nothing about it (D-V).
- **Fail with a sentence, not a stack.** Every refusal is a text result the parent model can act on.
- **Zero-dependency `contrib/`.** No npm install, no bundler; `node --test` only.
- **i18n is part of the change**, not a follow-up: `en.yml`, the glossary, `gen_locales.py`,
  `check_i18n.py`, and the `i18n` test run once at the end of the phase that adds strings.
- **Keep the feedback loop short** (`AGENT.md`): `node --test` for the extension, `cargo check -p
  orbit-pi` plus the named filter for Rust, `bacon run` only when the behavior is visual.

### 12.1 Phase 0 — spike: prove the seam (½ day)

No production code. The point is to replace three assumptions with fixtures.

- **T0.1** Write a two-line prompt file, then run the bundled `pi` by hand in a scratch repo:
  `pi --mode json -p --no-session --approve --append-system-prompt /tmp/p.md -- "Task: list src/"`
  (with and without `--extension <guard>`). Save the raw streams under
  `contrib/orbit-subagents-extension/fixtures/`.
- **T0.2** Confirm from the streams: `session` header first; `tool_execution_end.result`;
  `agent_settled { aborted }`; `message_end.usage`; at least one `tool_execution_update` from a
  long-running tool.
- **T0.3** Confirm the guard's existing fail-closed path in a child: a write tool with no UI is
  blocked with the `Blocked: Orbit has no UI…` reason. (The `ORBIT_SUBAGENT_CEILING` clamp does not
  exist yet; T1.8 adds it and §14.2 verifies it.)
- **T0.4** Confirm the `pi-subagents` premise by reading its `docs/agents.md` at the installed
  version (it reads `~/.pi/agent/agents/**/*.md`, user scope, user-over-builtin), and record the
  version checked. If the path has changed, D-D changes with it.
- **T0.5** Decide the stand-down probe: with `pi-subagents` installed, `pi.getAllTools()` lists a
  `subagent` tool. Record the exact predicate the extension will use.

**Gate:** fixtures exist and are committed; §15's phase-0 questions are answered in writing. No app
code changed. If T0.2 or T0.3 fails, stop and revise §8 before writing the extension.

### 12.2 Phase 1 — one child, no card (~2 days) — implemented

The feature becomes usable: a real report, real cost, real refusal, no UI work.

- **T1.1** `contrib/orbit-subagents-extension/package.json` + `agents.js` — frontmatter subset
  (plain, bracketed, comma string, missing, unknown), user discovery, builtin override, kebab-case
  validation, `ceilingForTools()`. Tests: `agents.test.mjs`.
- **T1.2** `builtins.json` — scout / worker / reviewer, with the corrected read tier
  (`read, grep, find, ls`).
- **T1.3** `run.js` — argv builder (inherit vs pinned model, tools list, `--exclude-tools` for
  read/edit ceilings, always `--mode json -p --no-session`), env table (§8), prompt-file assembly
  (body + scope + return guidance + task, D-Y), LF splitter, event fold, report cap, usage sum.
  Tests: `run.test.mjs` against the Phase 0 fixtures plus a scripted 300-event stream.
- **T1.4** `mode.js` — workflow store read for the parent session id with the `ORBIT_WORKFLOW_MODE`
  fallback, `subagents.json` with defaults and clamped caps, `allowedCeiling(access, workflow)`
  (D-Z). Tests: `mode.test.mjs`.
- **T1.5** `resources.js` — `ORBIT_BUNDLED_EXTENSIONS` → `--extension` args, deduped, existing paths
  only. Tests.
- **T1.6** `index.js` — activation, `getAllTools()` stand-down (D-C), `registerTool` (single mode
  only), re-registration only when the enabled set or a description changed (D-S), mode pre-flight
  refusal (D-Z), spawn, `SIGTERM`→`SIGKILL`, `signal` abort, depth and session caps, result
  `{ content, details, usage }`. Tests: `index.test.mjs` with a fake spawn that records
  argv/env/signals.
- **T1.7** `bundled_extensions.rs` — `subagents` field, install, `extension_paths()`, and
  `ORBIT_BUNDLED_EXTENSIONS` in `spawn`'s env (JSON array of the installed entry points). Test: the
  env carries the array, every entry exists, `extension_paths()` includes the new one.
- **T1.8** `contrib/orbit-guard-extension/policy.js` — `delegate` kind for `subagent`; `decide`
  allows it; ceiling clamp in `index.js` before the prompt path. Tests in `policy.test.mjs` and
  `index.test.mjs`.

**Gate:**

```
node --test contrib/orbit-subagents-extension/*.test.mjs
node --test contrib/orbit-guard-extension/*.test.mjs
cargo check -p orbit-pi
cargo test -p orbit-pi --bin orbit-pi bundled_extensions
```

Manual, on a real repo with the app running:

1. "Use scout to find where extensions are loaded" returns a report; the session's cost meter grows
   by the child's cost; the process list shows the child during the run and none after.
2. **Value check (§1.1):** the report is under ~1k tokens and the delegation costs under a cent,
   where the same question inline visibly spends tens of thousands of tokens. Record both numbers.
3. The stop button mid-run leaves no `pi` process behind.
4. Plan mode + `worker` → refused before spawn with the remedy sentence; no process, no usage.
5. Plan mode + `scout` → runs. A hand-written agent whose `tools:` line includes `edit` → refused
   before spawn, because its derived ceiling exceeds what Plan allows.
6. No approval bar appears for the delegation.

### 12.3 Phase 2 — the card (~3 days) — implemented

- **T2.1** `transcript.rs` — `ToolCall.details` on end and on update, 128 KiB bound, tests.
- **T2.2** `subagent.rs` — `SubagentRun::from_tool`, `headline`, `live_activity`, `status_phrase`,
  `is_live`, all phase/status strings, no panics on `null`.
- **T2.3** `context_meter.rs` — `format_cost` visibility, reuse the token formatter.
- **T2.4** `transcript_view.rs` — `render_subagent_card` with the three zoom levels, the phase table
  of §9.4, the blocked banner, the refusal line, the timeout line; dispatch beside `ask_user_question`.
- **T2.5** `working_activity_label` forwarding, `activity_title` delegation count,
  `activity_action_label` verb. Tests for each.
- **T2.6** Extension — `onUpdate` coalescing (400 ms per child + one per transition) and the
  `details` shaping to §6.2; test asserts a bounded update count on a 300-event stream.
- **T2.7** i18n — `subagent.*` and `transcript_view.delegated_to_agents_*`; regenerate locales.

**Gate:**

```
cargo test -p orbit-pi --bin orbit-pi subagent
cargo test -p orbit-pi --bin orbit-pi transcript
node --test contrib/orbit-subagents-extension/*.test.mjs
python3 scripts/check_i18n.py && cargo test -p orbit-pi --bin orbit-pi i18n
```

Manual: the row names the child's current tool while it runs and its counts when it settles; the
parent's working line reads `scout is grepping src/ · working for 34s`; the group header reads
`Delegated to 1 agent`; expanding shows real steps and a blocked call is labelled with the mode; a
hand-written foreign `details` payload falls back to the generic card; a live run does not churn list
layout and does not steal focus; a refused run reads as a sentence, not an empty card.

### 12.4 Phase 3 — chain, parallel, caps (~2–3 days) — implemented with Phase 1

- **T3.1** Chain mode with `{previous}`, the sequential loop, and the `step 2 of 3` footer.
- **T3.2** Parallel mode with the `maxConcurrent` pool and the `queued` phase.
- **T3.3** All caps (8/run, 4 concurrent, 24/session, depth 1) enforced with readable refusals.
- **T3.4** Per-child usage summing and the run footer; parallel rows each name their own tool.

**Gate:** 3 parallel children never exceed 4 concurrent and each row names its own current tool; the
9th task is refused with a readable message; a chain passes the first report into `{previous}` and
shows `step 2 of 3`; a child failing mid-chain stops the chain and names the step; the 25th child in
one session is refused.

### 12.5 Phase 4 — the management page (~3 days) — implemented

- **T4.1** `subagent_file.rs` — parse / surgical write / round-trip, plus the `builtins.json`
  consistency test (names, tiers, tools are real pi tools).
- **T4.2** `subagents.rs` — config store, lenient parse, clamped caps, atomic write.
- **T4.3** `app/subagents_ui.rs` — list column, detail form, validation, one action enum, CRUD
  flows with Undo and reload/overwrite on a changed file.
- **T4.4** `app.rs` + `app/settings.rs` + `commands.rs` — section, nav, palette row, dispatch.
- **T4.5** i18n for the page; regenerate + check.

**Gate:**

```
cargo test -p orbit-pi --bin orbit-pi subagent_file
cargo test -p orbit-pi --bin orbit-pi subagents_ui
node --test contrib/orbit-subagents-extension/*.test.mjs
python3 scripts/check_i18n.py && cargo test -p orbit-pi --bin orbit-pi i18n
```

Manual: an agent created on the page is offered in the tool description on the next delegation with
no restart; it can be edited, renamed, disabled, deleted (Undo restores the exact bytes) and reset to
builtin, each leaving the expected file and row; dropping an agent from *Edits* to *Read only*
removes `edit` from the next child's `--tools` **and** an `edit` attempt is denied with a ceiling
reason even in Full access; a hand-added `memory:` key and a comment survive a save byte-for-byte; a
file changed elsewhere is never overwritten without the explicit choice.

### 12.6 Phase 5 — safety and honesty pass (~2 days)

End-to-end mode matrix (Supervised / Auto-accept / Full × Plan / Build / Ask), project-agent toggle
behind trust, `pi-subagents` stand-down message, the notification (§9.9, off by default), docs,
CHANGELOG, INTENT D18.

**Gate:** a child in every access mode does exactly what the mode auto-approves and nothing else, and
every impossible combination is refused before spawn; installing `pi-subagents` and restarting shows
one tool, owned by it, and Settings says so.

### 12.7 Phase 6 — inspectable runs (optional, ~2 days)

Children spawn with `--session-dir ~/.orbit-pi/subagents/<parentSessionId>/` instead of
`--no-session`, and the card's report block grows an "open transcript" action that reuses
`sessions.rs` to read that file into a read-only viewer. Nothing appears in the sidebar.

**Gate:** opening a finished child shows its real transcript; deleting the parent session's subagent
dir is not required for a clean uninstall (documented).

### 12.8 Dependency order

```
Phase 0 ─► Phase 1 ─► Phase 2 ─► Phase 3
                │
                └────► Phase 4 (needs 1's agents.js + builtins.json; can run in parallel with 2)
                             │
Phase 2 + 4 ─────────────────┴─► Phase 5 ─► Phase 6 (optional)
```

Phase 4 can start once Phase 1's `agents.js`/`builtins.json` are stable; it does not depend on the
card. Phase 2 does not depend on the page. If only one more phase can be funded, ship Phase 2 — the
card is what makes the feature trustworthy.

---

## 13. Risks

| # | Risk | Mitigation |
|---|---|---|
| 1 | **Runaway fan-out** (a prompt that spawns forever) | depth 1, 8/run, 4 concurrent, 24/session, all enforced in the extension and reported in text; no config path to "unlimited". Depth applies to Orbit's own tool only (D-E) |
| 2 | **A child doing things the parent's mode would have asked about** | D-E: guard + workflow extensions are passed, mode is inherited, non-UI prompts are denied. D-Z refuses impossible runs up front. It is a guard, not a sandbox — say so in the docs |
| 3 | **Prompt injection via project agents** (`.pi/agents`) | not loaded in v1 (D-D); phase 5 behind trust, off by default |
| 4 | **Cost surprise** — delegated work is real spend | usage on every result (D-H), per-child usage on the card, one Settings line that says children bill the same credential |
| 5 | **Orphan child processes** after a crash/abort | `SIGTERM`→`SIGKILL` on abort, hard timeout, kill on extension deactivation, and a node test that asserts the kill path |
| 6 | **Two `subagent` tools** if `pi-subagents` is installed | D-C stand-down + one honest Settings line; read `pi.getAllTools()` at activation |
| 7 | **Child cannot start in a bundled `.app`** (no `node` on `PATH`) | reuse the parent's already-augmented `PATH`; surface the stderr tail in the card, never a silent empty report |
| 8 | **`--mode json` event shape drifts in a future pi** | parse defensively (unknown events ignored, no throw), keep Phase 0 fixtures in `node --test`, and fail with a readable "child produced no assistant message" rather than a blank report |
| 9 | **`details` growth in memory** (many live children) | bounded at the source (8 steps, 160-char previews) and again in Rust (128 KiB, steps capped) |
| 10 | **Two children editing one file** (D-N) | documented; parallel mode is described as "independent tasks", and the tool description says so |
| 11 | **Layout churn while streaming** | D-K: streaming content lives only in the expanded card; the collapsed row is a constant-height line |
| 12 | **The page and the file drift** (hand edits, another tool writes it) | hash + mtime captured on load; a save refuses to clobber a changed file without an explicit Reload/Overwrite (D-U) |
| 13 | **Clobbering another extension's frontmatter** (`extensions:`, `skills:`, `memory:`) | surgical writer (D-U) + round-trip tests asserting byte identity when nothing changed |
| 14 | **The tier and the real tool list disagree** | the ceiling is *derived* from `tools:` (D-Q), so they cannot disagree; a test asserts the argv, the env and the file line agree for every tier |
| 15 | **A pinned agent mode weakens Plan** | D-R intersection; D-Z caps the allowed ceiling in Plan/Ask; the card names the effective mode |
| 16 | **Context cost creep** — agent descriptions ride in the tool description on every turn | 200-char cap with a live count, enabled-only inclusion, and the preview block showing the real cost (D-S) |
| 17 | **Disabling an agent here does not disable it for `pi-subagents`** | the page says so when that package owns the tool (D-C); Orbit's config governs Orbit's tool only |
| 18 | **A user override quietly diverges from the shipped builtin** | *modified* badge on the row, the shipped text viewable, and *Reset to builtin* one click away (D-X); updates replace `builtins.json` only, never a user file |
| 19 | **A status that reads as more than it knows** (a spinner implying progress) | D-V: phases, counts and elapsed only, silence named as silence; no % / ETA anywhere |
| 20 | **Update flood** — a child with 200 tool calls pushes 200 `onUpdate` frames | 400 ms coalescing per child plus one per transition; a node test asserts the count against a 300-event fixture |
| 21 | **A long report stalls a frame** | display-capped Markdown, one open child log at a time, report rendering only after settle |
| 22 | **Focus or layout churn while a child streams** | fixed-height row (D-K), no focus movement on update (§9.8) |
| 23 | **Localization drift** | new keys land in `en.yml` only (D9); `scripts/check_i18n.py` stays green |
| 24 | **A child inherits the user's ambient pi extensions** (`~/.pi/agent/extensions/`, project `.pi/extensions/`), so its tools are wider than the tier suggests | the ceiling (D-Q) is the enforcement, `--tools` is only declaration; the card and the docs say *guard, not sandbox* (D-E); depth 1 is documented as Orbit's own tool only |
| 25 | **Agent names collide with `pi-subagents` builtins** — writing `scout.md` shadows its `scout` too | confirm copy on *Edit builtin* / *Reset to builtin* names both readers (D-X); no silent cross-tool change |
| 26 | **The agent catalog is part of the request prefix** — re-registering mid-session costs a prompt-cache miss | re-register only when the enabled set or a description actually changes (D-S); the page says *used from the next delegation* and the preview is labelled accordingly |
| 27 | **The mode pre-flight drifts from the guard's real rule** | both read the same mode stores and the same `ceilingForTools()`; a matrix test asserts the refusal set and the guard's allow set are complementary |

---

## 14. Verification

### 14.1 Commands per change (from `AGENT.md` §Verification)

| Change | Command |
|---|---|
| Extension, tests only | `node --test contrib/orbit-subagents-extension/*.test.mjs` |
| Guard policy | `node --test contrib/orbit-guard-extension/*.test.mjs` |
| `orbit-pi` Rust | `cargo check -p orbit-pi` + `cargo test -p orbit-pi --bin orbit-pi <filter>` |
| Strings | `python3 scripts/gen_locales.py`, `python3 scripts/check_i18n.py`, `cargo test -p orbit-pi --bin orbit-pi i18n` |
| Interaction/motion/keyboard | `python3 scripts/check_states.py`, `check_motion.py`, `check_keyboard.py` |
| Docs only | `git diff --check` |

Widen (`cargo clippy -p orbit-pi --all-targets`, `--workspace`) only when a phase gate says so or a
change touches shared state. A successful run with zero selected tests is not coverage.

### 14.2 Node tests, no network

- `agents.js` — frontmatter subset (plain, bracketed list, comma string, missing keys, unknown keys),
  a malformed file skipped without losing the others, builtin→user override, kebab-case check,
  `ceilingForTools()` for every tier and for an unknown/MCP name.
- `run.js` — argv for inherit vs pinned model, tools list present/absent, `--exclude-tools 'mcp__*'`
  for read/edit ceilings, `--no-session` and `--mode json -p` always present, temp prompt path,
  env table of §8, JSONL parse of the Phase 0 fixtures (report, usage sum, `stopReason: "error"`,
  unparsable line ignored, missing `message_end`), details bounding (steps, report, stderr, blocked,
  toolCounts), phase transitions, coalescing (300 events in one window → bounded updates + one per
  transition), prompt assembly order (body, scope, return guidance) with the task in argv and never
  in the file,
  U+2028 inside a string staying one line, `agent_settled { aborted: true }` → `aborted`,
  `agent_end` + retry not treated as settle, clean `stopReason` + empty report → `done`.
- `mode.js` — workflow store read for a session id, `ORBIT_WORKFLOW_MODE` fallback, defaults for a
  missing/malformed `subagents.json`, caps never above the hard maximum, `allowedCeiling()` for the
  full access × workflow matrix, and the refusal set matching D-Z's table.
- `index.test.mjs` — abort kills (fake spawn records signals), depth-1 refusal, cap refusals,
  pre-flight refusal without spawning, stand-down when `getAllTools()` already lists `subagent`,
  re-registration only when the agent set changes, `usage` on the result.
- `contrib/orbit-guard-extension/policy.test.mjs` — `delegate` kind always allowed, and the ceiling
  clamp: `read` blocks `edit`/`bash`/`mcp__*`, `edit` blocks `bash`, `exec` blocks `mcp__*`, no
  ceiling behaves exactly as today.

### 14.3 Rust tests, no network

- `subagent.rs` — parse a good payload, a foreign payload, an unknown version, missing fields,
  oversize steps, `headline()` for every phase, `live_activity()` non-`None` only while live,
  `status_phrase()` for running/blocked/failed/settled and for `done` with no report; `stop_reason`
  carried verbatim; no panics on `null` or absent optional fields.
- `transcript_view` — the card renders from a `ToolCall` with subagent details, from one without
  (generic fallback), and with a failed and a refused child; `working_activity_label` returns the
  child's activity for a live `subagent` step and `None` for a settled one; `activity_title` counts
  delegations instead of `Ran 1 tool`; only one child log is open at a time.
- `transcript.rs` — details retained on end and on partial update, oversize dropped, absent stays
  `None`.
- `subagent_file.rs` — round-trip is byte-identical when nothing changed; changing only `tools`
  touches one line and leaves a comment and a foreign key intact; a missing key is inserted in place
  without reordering; CRLF and odd spacing survive; a file with no frontmatter is reported rather
  than rewritten; an invalid name is rejected; the temp file never outlives a failed write; every
  tool name in `builtins.json` is a tool pi actually registers.
- `subagents.rs` (config) — defaults, partial JSON, unknown agent entries preserved, caps clamped to
  the hard maximum, atomic write.
- `app/subagents_ui` — the page renders a user agent and a builtin (actions differ), a malformed file
  shows the read-only fallback, save is disabled while clean, delete asks exactly once, and the CRUD
  matrix of D-X (new edits one file; edit rewrites five keys; `thinking:` round-trips into
  `--thinking`; rename writes new and removes old; disable records `subagents.json`; delete + Undo
  restores the exact bytes; reset returns the row to *builtin*; a changed file blocks the save).
- `bundled_extensions.rs` — the extension installs its files and returns the entry; every file the
  entry imports exists next to it; the spawn env carries `ORBIT_BUNDLED_EXTENSIONS`.
- `crates/orbit-rpc` fake-pi test (the `quota_fake_pi.rs` pattern): a scripted `pi` records its
  argv/env — asserts the extension arg list and the new env var are forwarded.

### 14.4 Manual matrix (one pass, phase 5)

Access (Supervised / Auto-accept / Full) × workflow (Plan / Build / Ask) × agent tier (read / edit /
exec): every cell either runs and stays inside its ceiling, or is refused before spawn with the
remedy. Plus: abort mid-run, timeout, an agent file edited while a child runs, a malformed agent file,
`pi-subagents` installed, and a window that loses focus during a long run.

### 14.5 Optional CI

A `ci.yml` job running `node --test` in each `contrib/*-extension` (they exist and are currently run
locally only).

---

## 15. Open decisions, with deadlines

1. **Per-run budget (tokens/cost) that fails a child.** Deferred to v2; usage is already summed, so
   it is additive. Revisit only if cost surprises appear — the Phase 1 value check is the evidence.
2. **Do chain intermediates reach the parent?** v1: no — only the final child's report, because the
   intermediates are the reason to chain (context savings). The card shows all. Revisit with real
   use.
3. **Per-child stop while a run continues.** Not in v1 (abort = whole run); it needs per-child RPC
   the protocol does not have. Revisit if parallel runs prove common.
4. **Should `scout` get `bash`?** It ships read-only (grep/find/ls cover most recon). If real use
   shows it needs `git log`, raise the agent's tier explicitly or add a `runner` agent — do not
   quietly widen the tier of the recon agent. Decided by the Phase 1 gate's evidence.
5. **Project agents (`.pi/agents`).** Phase 5 at the earliest, behind project trust, off by default.
6. **Catalog location: tool description vs prompt section.** D-S decides the tool description and
   accepts one cache miss per change. If live agent editing turns out to be common, the prompt
   section (pi-subagents' approach) is the proven escape hatch. Decide before phase 4, not after.
7. **Phase 6 pruning.** If child session dirs are created, decide whether the parent session's delete
   path removes them; otherwise document the leftover directory.
8. **Is three tiers enough?** A `researcher` needs `webfetch` (a read tool by the guard's
   classification) — fine — but "reads plus one command" has no tier. Add a fourth only when a real
   agent needs it; Advanced covers it meanwhile.
9. **Pinnable card / completion notification.** §9.8 covers at-a-glance, §9.9 is one optional signal.
   Default off; revisit if long runs become normal.

---

## 16. Docs to update when this ships

Per `AGENT.md` §7, a feature is not done until the docs describe it.

- `INTENT.md` — new **D18 — Subagents: a bundled delegation tool over child `pi` processes**, plus
  *Implementation status*.
- `crates/orbit-pi/docs/subagents.md` — **new**: what an agent is, the three modes, the caps, what
  children may do under each access mode (and that a child also loads the extensions pi discovers —
  a guard, not a sandbox), the capability tiers and how the ceiling is derived and enforced, where
  each setting lands (agent file vs `~/.orbit-pi/subagents.json`), how to add/edit/rename/disable/
  delete/reset an agent in Settings, that writing a builtin override also overrides `pi-subagents`'
  same-named builtin, that an impossible run is refused before it spends, and how to read the
  delegation card (phases, blocked banner, refusal line, where a child's real steps live).
- `AGENT.md` — file map rows (`subagent.rs`, `subagent_file.rs`, `subagents.rs`,
  `app/subagents_ui.rs`, `contrib/orbit-subagents-extension/`).
- `README.md` — a Features bullet: delegate to a child agent, and manage agents (capabilities, mode,
  model, prompt) from Settings.
- `CHANGELOG.md` — `[Unreleased] → Added`.
- `PRODUCT.md` — the capability list gains "delegate scoped work to a child agent".
- This file — set **Status** to the phases actually shipped (§12's gates are the status).

---

## 17. Definition of done

- Every phase gate in §12 has run, with the command output recorded in the change that landed.
- The Phase 1 value check reproduces: a delegation returns ≤ ~1k tokens and costs under a cent for
  work that costs an order of magnitude more inline.
- No impossible delegation spawns a process; every refusal is a sentence with a remedy.
- A child never exceeds the session's access and workflow modes; the card states the mode and the
  ceiling it ran under.
- Killing the run, the turn, the session or the app leaves no `pi` child behind.
- An agent can be authored, edited, renamed, disabled, deleted and reset inside Settings, and the
  next delegation uses it with no restart and no second source of truth.
- `en.yml` is the only hand-edited locale and `check_i18n.py` is green.
- INTENT D18, `AGENT.md`'s file map, `README`, `CHANGELOG`, `PRODUCT.md` and the user-facing
  `docs/subagents.md` describe the shipped behavior.
