# Subagents in Orbit

A **subagent** is a child `pi` process that does one scoped job and returns only
a short report. The parent session keeps its own context; the child burns its
own. Delegation pays when the work is broad — "find where auth is wired",
"review this diff", "a second opinion" — and does not pay for one file you
already know.

The tool is one bundled pi extension
([`contrib/orbit-subagents-extension/`](../../../contrib/orbit-subagents-extension/)),
loaded with `--extension` on every session Orbit spawns. Nothing about it is in
the RPC protocol: the call arrives as an ordinary tool call, and its `details`
carry the run state.

## Agents

An agent is a markdown file in `~/.pi/agent/agents/` — the same path the
`pi-subagents` package reads, so an agent written for one works in the other:

```markdown
---
name: scout
description: Fast local codebase recon — relevant files, entry points, data flow, risks.
model: anthropic/claude-haiku-4-5   # optional; absent inherits the session model
thinking: medium                    # optional; absent inherits the session level
tools: read, grep, find, ls
---

You are a scout. Find, do not change. Return a compressed map: …
```

Only `name` and `description` are required. `tools` absent means pi's own
defaults (which include `bash`); `tools:` with nothing after it means the child
gets no tools at all. A malformed file is skipped with a diagnostic and never
takes down discovery of the others.

Three ship built in: **scout** (read only — recon), **worker** (runs commands —
implementation) and **reviewer** (read only — review). A user file with the same
name overrides the builtin. Because the names are shared with `pi-subagents`,
writing `scout.md` also overrides its `scout`.

## What a child may do

The `tools:` line decides a **ceiling**:

| Declared tools | Ceiling | The child may |
|---|---|---|
| `read`, `grep`, `find`, `ls`, `webfetch`, … | `read` | observe only |
| + `edit`, `write` | `edit` | change files |
| + `bash`, `powershell` | `exec` | run commands |
| an MCP or extension tool | `all` | anything the tool can |

The ceiling is enforced twice: the child's `--tools` list narrows what it is
offered, and the access guard blocks anything above the ceiling even under Full
access. `--tools` alone is a declaration, not a boundary — pi keeps MCP tools
reachable through `codemode`, which is why the guard clamp exists.

The session's own modes intersect it. Plan and Ask cap every child at `read`,
whatever the access mode says:

| Access mode | Build | Plan / Ask |
|---|---|---|
| Supervised | read | read |
| Auto-accept edits | edit | read |
| Full access | exec / all | read |

A delegation the session mode cannot finish is **refused before it spawns** —
`worker is a command-running agent; this session is Build · Supervised. Switch
the session to Build + Full access, or delegate to a read-only agent.` No
process, no tokens, no half-done work.

A child is a normal `pi` in the same workspace and trust context, with the same
project instructions and skills. It also loads the extensions pi discovers for
that user and project. This is a **guard, not a sandbox**: a child in Full
access can do what the parent could do.

## The three shapes

```
subagent({ agent: "scout", task: "find where auth is wired" })

subagent({ chain: [
  { agent: "scout",  task: "find where auth is wired" },
  { agent: "worker", task: "add the validation: {previous}" },
] })

subagent({ tasks: [
  { agent: "reviewer", task: "review src/auth.rs" },
  { agent: "scout",    task: "find the callers of that file" },
] })
```

Limits: **8 tasks per run, 4 concurrent, 24 children per session, depth 1**. A
child cannot start another subagent. Every cap is reported in the error text
rather than silently dropped.

The child sees only its `task` — not the conversation. Put everything it needs
in the task: paths, what was decided, and what to return.

## What comes back

One line per child (agent, status, usage) plus each report, capped at 4 KiB per
child so a delegation cannot replace one context problem with another. The full
report (up to 64 KiB) is in `details`. The child's transcript, tool arguments and
file contents are never returned.

The child's token and cost usage rides the tool result, so it lands in the
session's usage meter. A delegation that fails or is aborted still reports what
it spent.

## Reading the card

A delegation is one card in the transcript, and it answers the questions you
would otherwise have to open a log for.

**Collapsed**, one line per child: the agent, the capability it ran under, what
it is doing *right now* (the tool call, not the task), how many tools it has
called, how long it has run, and the model it actually used. A running child's
glyph is accent-colored; a finished one is ink.

```
⠿ scout     read only   grep /auth/ in src/   12 tools   0:34   haiku-4-5
⠿ reviewer  read only   thinking                8 tools   0:21   sonnet-4-5
✓ worker    runs commands   edit src/auth.rs   24 tools   1:41   ↑8.2k ↓1.4k <$0.03
```

Under the rows, the run footer says the state and the cost — `2 of 3 running`,
`3 done · 44 tools`, `↑12.1K ↓2.6K <$0.05` — and a chain adds `step 2 of 3`.

**A reason line** appears when a run was refused before it started (`worker is a
command-running agent; this session is Build · Supervised…`), when the mode
blocked a call (`blocked by Plan · edit src/auth.rs`), or when a child failed.
That line is the whole "why did it stop?" answer; nothing is inferred.

**Expanded** (click a child, or ⌘O), that child's real steps — `grep`, `read`,
`edit`, `bash`, with the argument preview — then the report it returned,
rendered as Markdown. One child at a time: the row you clicked keeps the
selected fill, and clicking it again closes the card. A one-child delegation
needs no choice, so opening the card is opening its log. A child that settled
with nothing to say reads `no closing message for the parent`; it is not shown
as a failure.

While a child runs, the parent's own working line names it — `scout ·
grep /auth/ in src/`, or `2 agents working` — and the turn header reads
`Delegated to 2 agents` rather than counting the child's calls as your work.

## Managing agents

**Settings → Subagents** is the whole library in one page. The left column
groups **Built in** and **Your agents**; the right column edits the selected
one:

- **What it can do** — read only, edits files, or runs commands. This is the
  single input: it writes the file's `tools:` line and it is what the child's
  ceiling is derived from, so the page cannot claim a capability the child does
  not have. A hand-written tool list that matches no tier is shown as a custom
  list rather than silently narrowed.
- **Mode** — inherit the session, or pin Plan / Build / Ask. The more
  restrictive of this and the session's mode wins.
- **Model and thinking** — leave empty to inherit the session's; otherwise
  `provider/model` and a level from `off` to `max`.
- **Description** — paid for on every turn of every session, so the page counts
  the characters and warns past 200.
- **Prompt** — appended to the child's system prompt.
- **What the model sees** — the exact agent block the tool description will
  carry, with disabled agents omitted. The page states that it takes effect on
  the next delegation, because the tool description is part of the request.

**Save** changes only the five keys Orbit owns (`name`, `description`, `model`,
`thinking`, `tools`) and the prompt. Comments, key order, another extension's
`memory:` or `skills:` keys, and the rest of the file survive byte-for-byte. If
the file changed on disk since it was loaded, the save is refused until you
reload — an external edit is never clobbered silently.

| Row | Action |
|---|---|
| builtin | **Disable** (the model stops seeing it; the shipped file is untouched) |
| your file | **Delete**, with **Undo** restoring the exact bytes |
| your file that shadows a builtin | **Reset to builtin** |
| any | **Duplicate** (starts a `<name>-copy`), **Reveal file**, **Open in editor** |

Writing a builtin's page creates `~/.pi/agent/agents/<name>.md`, which overrides
the shipped definition for Orbit **and** for `pi-subagents`; the page labels the
row *modified* so the divergence is visible.

## Running the checks

```sh
node --test contrib/orbit-subagents-extension/*.test.mjs
node --test contrib/orbit-guard-extension/*.test.mjs
cargo test -p orbit-pi --bin orbit-pi bundled_extensions
```

## Current state

The tool, agent discovery, the three run shapes, the caps, the mode/ceiling
pre-flight, cost reporting and the delegation card are shipped. The
**Settings → Subagents** management page and inspectable child transcripts are
not built yet: agents are edited as files, and a child's transcript is not kept
after it exits. The design and the remaining phases are in
[`docs/plans/subagents.md`](../../../docs/plans/subagents.md).
