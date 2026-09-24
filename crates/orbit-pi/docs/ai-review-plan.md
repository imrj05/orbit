# AI Review Agent — read-only review over changes or the whole project

Status: **Implemented**
Owner: TBD
Issue: [#6](https://github.com/imrj05/orbit/issues/6)

The README roadmap calls for an **AI review agent**: "Run a read-only reviewer
over current changes or the whole project; findings render in the Review pane."
This document records the decisions locked before implementation.

## Decisions locked

- **Dedicated read-only session.** The reviewer runs on its **own** pi process,
  scoped to **Ask mode**, so the user's chat session, transcript, and access-mode
  approval bar are never touched. It is not a second turn in the active session.
- **Findings render in the Review pane.** A collapsible *AI findings* section
  sits between the Review toolbar and the diff, with severity chips and
  click-to-file rows. The raw answer's prose is shown when no structured
  findings parse.
- **Scope is user-selectable.** Two actions: **Review changes** (the selected
  Review source — Last Turn / Uncommitted / Unstaged / Staged / Committed /
  Branch) and **Review project** (the whole workspace).
- **Read-only is enforced by the workflow extension, not a sandbox.** The
  reviewer process is spawned with `ORBIT_WORKFLOW_MODE=ask`, so the workflow
  extension drops `edit`/`write` and gates bash to its read-only allowlist
  (`git status|log|diff|show|blame|…`) from the very first hook. The session id
  is also persisted to `~/.orbit-pi/workflow.json` when it becomes known.

## The guard conflict (and its fix)

Orbit's access guard (`contrib/orbit-guard-extension/`) classifies `bash` as
`exec`, so under the default **Supervised** access mode it would raise an
approval dialog for the reviewer's read-only `git diff`. That dialog lands on a
process nobody routes, so the run would hang.

Fix: the reviewer process is launched with `ORBIT_REVIEW=1`. The guard treats
that marker as pre-approved (returns immediately), because the workflow
extension — loaded in the same process — is the stricter read-only gate and
still blocks writes and unsafe bash. The marker is **process-scoped**: it never
touches `~/.orbit-pi/access.json` and cannot widen the active session.

## Architecture

| Piece | File | Responsibility |
|---|---|---|
| Pure model | `src/ai_review.rs` | `ReviewKind`, `Severity`, `Finding`, `Report`, prompt builders, `parse_report` |
| Lifecycle | `src/app/ai_review.rs` | spawn / prompt / drain / settle / cancel; owns the reviewer `PiClient` + `Transcript` |
| Pane state | `src/sidepane.rs` | `ai_findings`, `ai_review_status`, `ai_review_kind`, `ai_findings_open` |
| Spawn | `crates/orbit-rpc/src/client.rs` + `src/bundled_extensions.rs` | env-aware spawn; `spawn_reviewer` sets `ORBIT_WORKFLOW_MODE=ask` + `ORBIT_REVIEW=1` |
| Guard | `contrib/orbit-guard-extension/index.js` | honour `ORBIT_REVIEW=1` |
| Entry points | `src/command_palette.rs` + `src/app/pickers.rs` + `src/sidepane.rs` | palette commands and the pane's sparkles menu |

The reviewer reuses the parked-session event pattern (`ParkedSession`): its own
`Transcript` accumulates events, and on `agent_settled` the final answer is read
with `Transcript::last_response_text()`.

## Findings contract

The prompt asks the model to end with one fenced `json` block:

```json
{"findings": [
  {"severity": "error|warning|info", "file": "relative/path", "line": 42,
   "title": "Short summary", "detail": "Why it matters / how to fix"}
]}
```

An empty `{"findings": []}` is a valid "no issues" answer. `parse_report`
prefers the last parseable block, strips it from the prose, and never errors:
a malformed or missing block yields zero findings and keeps the full markdown.

## Phases

- **Phase 0 — pure model.** `src/ai_review.rs` + 9 unit tests. Done.
- **Phase 1 — process.** env-aware `PiClient` spawn, `BundledExtensions::spawn_reviewer`
  (`ORBIT_WORKFLOW_MODE=ask` + `ORBIT_REVIEW=1`), guard bypass + node test. Done.
- **Phase 2 — lifecycle.** `src/app/ai_review.rs`, heartbeat drain, settle parse,
  cancel, session-switch pruning. Done.
- **Phase 3 — UI.** pane state + findings panel + sparkles menu; palette
  commands. Done.
- **Phase 4 — polish.** i18n (en + regenerated locales), docs (README /
  CHANGELOG / PRODUCT / INTENT / AGENT). Done.
