# Plan — Review page: a first-class, background AI review workspace

**Status:** Proposed — design locked for discussion; no implementation started.
**Owner:** TBD
**Scope:** Orbit desktop app (`crates/orbit-pi`). Touches `ai_review.rs`, `app/ai_review.rs`,
`git.rs`, a new `review_page/` module, `app.rs` routing, `sidepane.rs`, i18n.
**Related:** `docs/ai-review-plan.md` (shipped v0.0.17–0.0.18), `crates/orbit-pi/docs/git-page-plan.md`
(page pattern), `usage/page.rs` (feature-page pattern), `review.rs` (`Source` model),
`model_selector.rs` (model + thinking picker), `workflow.rs` (Ask-mode read-only scope)
**Reference (inspiration only):** `earendil-works/pi-review` — rubric ideas, not code.

---

## 1. Goal

Turn AI review from a section in the Review side pane into a **dedicated page** that is:

1. **Self-contained** — pick target → pick model/thinking → start → watch → read findings,
   all on one page.
2. **Background-first** — reviews run as app-owned tasks, independent of the active
   session, workspace, or page. Start one for Project A, switch to Project B, start
   another, come back later.
3. **History-preserving** — every run is kept and can be revisited, re-run, or exported
   to markdown.
4. **Actionable** — findings are structured rows with location, severity, and detail,
   not chat paragraphs.

The old in-pane section (`sidepane.rs` AI findings) is **superseded by this page**.
The Review pane itself (diff viewer with sources) stays; it remains the place you go to
*read a diff*, while the Review page is where you go to *get a review*.

---

## 2. What exists today (starting point)

- `crates/orbit-pi/src/ai_review.rs` — pure model: `ReviewKind` (Changes | Project),
  `Severity`, `Finding`, `Report`, prompt builders, `parse_report` (fenced JSON block).
  9 unit tests.
- `crates/orbit-pi/src/app/ai_review.rs` — `start_ai_review` / `tick_ai_review` /
  `cancel_ai_review` / `discard_ai_review`. One reviewer process at a time, tied to the
  active workspace. Read-only via `ORBIT_WORKFLOW_MODE=ask` + `ORBIT_REVIEW=1`.
- `sidepane.rs` — findings section with severity chips, summary prose, click-to-file.
- `git.rs` — `collect_review_diff(workspace, Source, session)`, `history()`,
  `commit_detail()`, `status_rows()`. `Source` = LastTurn | Uncommitted | Unstaged |
  Staged | Committed | Branch (branch = merge-base vs main/master).
- `review.rs` — `Snapshot`/`File` render model for the diff pane.
- Page pattern: `UsagePage` (`usage/page.rs` + `usage/view.rs`) is the template — an
  `Entity` owned by `OrbitApp`, rendered in place of the chat body via a `*_open` flag,
  with `top_bar_leading()` contributing its title into the shared top bar.

**What changes conceptually:** review state moves from "the app's current review" to
"a store of review runs," and execution moves from "the active session/worktree" to
"a task pinned to the workspace it was started from."

---

## 3. Information architecture

**Design rule: tabs, one shared scope.** The page has three working tabs — **New review**,
**Running**, **Changes** — plus **History**. All of them read/write the same scope: the
target picked on *New review* decides what *Changes* shows; the files checked in
*Changes* feed the **Files…** target back on *New review*. No tab is a separate page.

```
┌────────────────────────────────────────────────────────────┐
│ Review                                         History     │
├────────────────────────────────────────────────────────────┤
│  New review   │   Running ①   │   Changes ⑭               │
├────────────────────────────────────────────────────────────┤

  NEW REVIEW (default)                RUNNING                   CHANGES
┌──────────────────────────────┐ ┌──────────────────────┐ ┌────────────────────────┐
│ 14 files changed · +120 −43  │ │ ▸ Uncommitted · orbit│ │ Uncommitted · 14 files │
│                See changes → │ │   62% · 14 files 12s │ │ ────────────────────── │
│ (•) Uncommitted     14 files │ │            [Stop]    │ │ ☐ M src/config.rs  +42 │
│ ( ) Branch vs main  22 files │ │ ✓ Branch vs main ·    │ │ ☑ M auth/token.rs  +31 │
│ ( ) A commit          ▾      │ │   pi-sdk · no issues │ │ ☐ A refresh.rs    +88  │
│     9f0e1d2 Refactor… 12 f   │ └──────────────────────┘ │ ☐ D legacy.rs     −23  │
│ ( ) Files…           none    │ ┌──────────────────────┐ │ ────────────────────── │
│ ( ) Whole project  1,204 f   │ │ ● Needs attention    │ │ @@ -118,6 +118,7 @@   │
│ ─────────────────────────────│ │   3 findings · 41s   │ │   let token = …       │
│ Opus 4.5 · high ▾ [ Start ]  │ │  [error] Unwrap  …   │ │ - refresh(…).ok();    │
└──────────────────────────────┘ │  [warn]  Blocking …  │ │ + refresh(…)?.context │
                                 └──────────────────────┘ │ 2 of 5 hunks · open →  │
                                                          │ [Review selected]      │
                                                          └────────────────────────┘
```

**Page chrome.** Follows `UsagePage`: entity owned by the app, shown in place of the
chat body when `review_page_open` is true. `top_bar_leading()` paints "Review" plus the
History control. Entry points: sidebar row, command palette ("Open Review"), and a link
from the Review pane's toolbar (replacing the sparkles menu).

**Tab bar.** A flat tab strip under the page title; the active tab is marked by an
accent underline (the app's existing tab language). Badges: **Running** shows the active
run count, **Changes** shows the current target's file count, both hidden at zero.
The strip is the only persistent chrome — content below it follows the minimal,
single-column rule (~660px max, one primary action per tab).

**New review (default tab).** Target card: changes summary line (`See changes →` jumps
to the Changes tab), five target rows, inline commit list, then the model chip and the
page's only accent button, **Start review**. This is the whole idle page.

**Running (activity home).** Live runs first: target + workspace, progress bar, files
read / elapsed, **Stop**. When a run finishes it **stays in place** — a green
**Review completed** status replaces the progress bar ("no issues" when empty) — and its
findings render below with inline expansion. Nothing auto-archives, nothing moves.
The **History** control opens the archive of older runs (all workspaces); picking one
swaps this tab's content to that run with a "Back to current" affordance.

**Changes (preview + picker).** The file-changes surface for the current target: file
list with per-file counts and a checkbox per row, selected file's inline diff,
**Review selected** action. `See changes →` on the New review tab just switches tabs.
See §4a.

---

## 4. Target picker

One card under the header holds the changes summary, the five target rows, and the
Start row. Five targets (radio semantics; exactly one selected):

| Target | Meaning | Count shown | How computed |
|---|---|---|---|
| **Uncommitted** | HEAD → worktree (staged + unstaged + untracked) | `N files +A −D` | `git status` numstat (existing `status_rows`) |
| **Current branch** | merge-base(HEAD, base) → worktree; base defaults to `main`/`master`, selectable | `N files` | `git merge-base` + `collect_review_diff(Source::Branch)` |
| **A commit** | one commit, `parent → commit` | `N files` | inline expandable commit list (below) |
| **Files…** | snapshot review of chosen paths (not a diff) | `N files` | multi-select from the Changes tab |
| **Whole project** | repo-wide pass | `N files` (tracked count) | `git ls-files` count only, no diff collected |

Notes:
- **Uncommitted vs current branch** are distinct on purpose: "what have I not saved yet"
  vs "what does my branch add." Both were explicitly requested.
- **Files…** supports several paths. It maps to the reference's folder-snapshot mode but
  is scoped from the app's Changes tab (§4a) — that is the picker.
- The old side-pane `ReviewKind::Changes` (diff of one `Source`) becomes
  **Uncommitted** here; `Source` selection stays a Review *pane* concern. The page's
  "Current branch" replaces the pane's hardcoded `Source::Branch` for the review flow.
- Counts are collected off-thread and cached; each target row shows a skeleton count
  while loading, then the real numbers. If git fails, the row shows `—` and Start is
  disabled with a reason.
- **Changes summary (card's first line).** `14 files changed · +120 −43` with a
  **See changes →** link that jumps to the Changes tab. One line, always present,
  never a separate panel.

**Commit list (inline, no separate card).** Selecting **A commit** expands a short list
*inside* the target card, indented under its row (a nested accordion — not a new
section):
- Source: `git::history(cwd, 20, 0)` (exists) plus **new** `commit_stats(cwd)` returning
  per-commit `(additions, deletions, file_count)` in one `git log --numstat` pass (no N+1
  `commit_detail` calls).
- Row: `9f0e1d2  Refactor auth token refresh   12 files +540 −388`. The selected row also
  shows the first ~5 file names; the rest of the list stays one line tall.
- Selecting a commit switches the review target to it. **Start review** is then one more
  click (no hidden auto-start — a misclick must not spend a review).
- Empty repo / no commits: the row is disabled with "No commits yet" and the other four
  targets remain available.

---

## 4a. File-changes preview (Changes tab)

Directly requested: the user must be able to *see where the changes are* before (and
while) spending a review on them. It now lives in the **Changes tab** — a permanent view
of the current target's files, not a temporary drawer. It also **is** the file picker for
the **Files…** target.

**Layout (top → bottom):**

1. **Scope line** — `Uncommitted changes · 14 files · +120 −43`, plus a file filter once
   the list passes ~20 files.
2. **File list.** One row per changed file: `☐ M src/auth/token.rs +31 −12`. Checkbox,
   status letter (git's `M`/`A`/`D`/`R`), monospace path, tabular counts. Virtualized
   (the pane's `ListState` pattern), directory-grouped, biggest churn first. Checking
   files sets the **Files…** target; the target row on *New review* and the tab badge
   update live.
3. **Selected-file diff.** Selecting a row renders its unified diff below:
   syntax-highlighted via `highlight`, with the pane's context collapsing and
   expand-in-place controls. Opens on the first 2 hunks (or ~300 lines), whichever is
   smaller; footer `2 of 5 hunks · open full diff in the Review pane →`.
4. **Action row** — `2 files selected` · **Review selected** (sets Files… + switches to
   *New review* so **Start review** is still the explicit commit step).

**Interactions:**
- Clicking a **finding** on the Running tab → switches to *Changes* with that file
  selected, scrolled to the finding's line. The loop for "where is this problem?"
- Per-file **Review this file** row action → runs a snapshot review of just that file.
- Per-file context menu: copy path, open in editor, show in file tree — matching the Git
  panel's existing file menu.
- **Whole project** target shows a quiet "No diff for this target" state (the file list
  is not a diff); the tab badge hides.

**Data & performance:**
- The tab consumes the *same* diff collection the reviewer would send, so selecting a
  target collects once off-thread and caches it (`ReviewDiff` + parsed `Snapshot`).
- Per-file preview is capped (~8k rendered lines) so one generated file cannot bloat the
  page; the full diff stays reachable through the Review pane.
- Collection for the preview is UI data only — `cap_patch` applies when the reviewer
  starts, independently.

**Implementation impact:**
- `git.rs` (phase 1) grows a collect path for the preview targets
  (`collect_review_diff` already returns `ReviewDiff { patch, files }` — the preview also
  needs the parsed `Snapshot`, or a thin parse on the page side).
- `sidepane.rs` diff-list rendering is extracted to a shared widget
  (`widgets/diff_view.rs`, phase 3.5) so pane and tab cannot drift. The pane keeps its
  own state wiring; only the row painting + expansion UI move.

---

## 5. Run configuration

One quiet row at the bottom of the target card — no separate card:

- **Model chip** — `Opus 4.5 · high ▾`, a text-weight chip (muted until hover), opening a
  popover with the existing `ModelSelector` sections: model first, then thinking levels
  (`thinking_icon` / `thinking_display` are already `pub(crate)`). The page keeps its own
  `ReviewRunConfig { model, provider, thinking }`, defaulted from the session default
  (`session_defaults.rs`), and reads `available_models` / `available_thinking_levels`
  from the app. Writes are run-scoped.
- **Start review** — the page's only accent button, right-aligned. Disabled while the
  target's count is loading or the target has no changes.
- **Stop** — lives in the running card (§6), not here.
- **Re-run** — on a finished run, replaces Start on that run's view.

One deliberate simplification: **the reviewer model is run-scoped, never session-scoped.**
Starting a review must not change the chat session's model.

---

## 6. Progress and findings (Running tab)

**While running** (live rows at the top of the Running tab):
- Progress bar + live status line ("Reading 14 files…", "Reviewing src/…") derived from
  the reviewer's streamed tool events (the transcript drain already sees them;
  `app/ai_review.rs` can surface a coarse `ReviewProgress` — counts and last path).
- Target + workspace, elapsed time, **Stop**.
- On completion the row stays exactly where it is: the progress bar is replaced by a
  green **Review completed** status (or the failure state), never a jump to another view.
- The tab badge (`Running ①`) and the sidebar row show activity so a background run is
  discoverable from anywhere.
- Multiple concurrent runs (up to the cap) are separate rows.

**When done** (same tab, same position; the row stays above its findings):
- **Verdict line**: `needs attention` / `correct` + run metadata (target, model,
  thinking, duration, files inspected), plus **Copy as markdown** and **Re-run**.
- **Findings list**: severity chip, title, `file:line`. Clicking a row expands it inline
  (detail + **Open file** / **Copy**) — no panels, no navigation. The expanded row also
  jumps to the Changes tab at that line.
- **Large result sets**: past ~15 findings, `info`-severity rows collapse behind a
  "show N more" line; errors and warnings always stay visible.
- **Prose summary** (when the model returns one) renders as a muted paragraph under the
  verdict, above the findings.
- **Empty result** (`{"findings":[]}`) renders as a positive state: green check,
  "No issues found," with the model/target metadata intact.
- **Failed run**: error card with the failure reason, **Retry** and **Copy error**.

**Optional (phase 6)**: "Fix with agent" — sends a follow-up prompt to the *active*
session referencing the findings (never mutates the reviewer). Not in scope for the
first cut.

---

## 7. Background execution model

This is the architectural core of the request and the biggest change.

### 7.1 Review runs are app-owned tasks

```rust
// app/reviews/mod.rs (new module)
pub struct ReviewRun {
    pub id: ReviewRunId,             // monotonic u64
    pub workspace: PathBuf,          // pinned at start
    pub session: Option<String>,     // for LastTurn/commit context, pinned at start
    pub kind: ReviewKind,            // Uncommitted | Branch{base} | Commit{sha}
                                     // | Files{paths} | Project
    pub config: ReviewRunConfig,     // model, provider, thinking
    pub status: ReviewStatus,        // Queued | Running | Done | Failed | Cancelled
    pub started_at: SystemTime,
    pub finished_at: Option<SystemTime>,
    pub progress: ReviewProgress,    // coarse, from streamed events
    pub report: Option<Report>,      // findings + verdict + summary
}

pub struct ReviewStore {
    runs: Vec<ReviewRun>,            // newest first, capped (e.g. 50 per workspace)
    active: HashMap<ReviewRunId, ReviewTask>, // process + transcript
    max_concurrent: usize,           // default 2
}
```

- **Per-run everything.** The current `OrbitApp` fields (`ai_review`,
  `ai_review_generation`, `ai_review_kind`, `ai_review_status`, `ai_report`) are replaced
  by `reviews: ReviewStore`. `spawn_reviewer(&workspace)` is called with the *run's*
  workspace, so a run started in Project A keeps reviewing Project A after the user
  switches to Project B.
- **Event loop.** `tick_ai_review` becomes `tick_reviews(cx)`: drains every active task's
  events, updates that run's `progress`/`report`, and notifies. Runs are independent; a
  slow one never blocks another.
- **Cancellation is per-run** (`Cancel` on a run row / Stop button), not global.
- **Generation counters** remain per-run to discard stale async work.
- **Session safety.** Runs never touch the user's session — unchanged from today
  (`ORBIT_WORKFLOW_MODE=ask`, own process, `ORBIT_REVIEW=1`).
- **Concurrency cap.** `max_concurrent` (default 2) with a FIFO queue so a user can start
  several runs without spawning an unbounded number of pi processes. Expose in Settings →
  Agent later; fixed default now.
- **Completion notifications.** When a run finishes, post a toast ("Review finished —
  2 errors, 1 warning" → opens the page focused on that run). Reuse `toast.rs`.
  Suppress if the user is already on the Review page viewing that run.

### 7.2 Persistence

Persist finished runs to `~/.orbit-pi/reviews.json` (same pattern as
`session-defaults.json` / workspace persistence): id, workspace, kind, config, status,
timestamps, report, verdict. Cap per workspace (50) and total (200). Active runs are not
persisted; on launch, runs that were `Running` become `Failed("Interrupted by app
restart")`. Findings survive restarts — the value is in reading them later.

### 7.3 Staleness

A persisted run records the workspace's `HEAD` at start. If `HEAD` differs when viewed
(or the workspace is gone), the run shows a muted **"Based on an older revision"** chip
and a prominent **Re-run** action. Never silently re-review; never silently hide the
staleness.

### 7.4 Git collection

- `git::collect_review_diff` already takes `(workspace, Source, session)` and runs
  off-thread. Add:
  - `git::commit_stats(cwd, limit) -> Vec<CommitStats>` — one pass, `--numstat`, for the
    inline commit list.
  - `git::target_counts(cwd, kind) -> TargetCounts` — the per-target badges. One
    off-thread call per workspace page load; refresh on git-state change (reuse the
    pane's staleness signal) and after each run.
  - `ReviewKind` grows `Branch { base }`, `Commit { sha }`, `Files { paths }`; the prompt
    builders in `ai_review.rs` grow matching cases (`FILES_SNAPSHOT` prompt for Files;
    commit/branch prompts reuse the existing diff embedding + `cap_patch`).
- All git work stays off the UI thread (existing `background_executor` pattern).

---

## 8. Relationship to existing surfaces

| Surface | Change |
|---|---|
| Review pane (diff) | **Keep.** Sources, diff rendering, expansions — untouched. Its toolbar's sparkles menu and AI findings section are replaced by a single "Open in Review page →" chip. |
| `sidepane.rs` AI fields (`ai_review`, `ai_findings_open`, `ai_review_action`, `render_ai_review`, `render_finding`) | **Remove** once the page ships (phase 6). Diff pane state stays. |
| `app/ai_review.rs` | **Replaced** by `app/reviews/` (store + task + git collection). Pure `ai_review.rs` model survives and is extended. |
| Command palette | Add "Open Review page"; remove "Review changes/project" rows that targeted the pane. |
| Sidebar | Add a "Review" feature row (same pattern as Git/Usage) with a running-count badge. |
| i18n | New `review_page.*` keys; remove the `ai_review.*` keys that die with the pane section. en-first, then regenerate the other locales per the existing workflow. |

---

## 9. UX references (adapted, not copied)

- **GitHub Files-changed** — per-file counts beside the diff; the Changes tab and the
  findings-by-file grouping.
- **Linear inbox / triage** — severity-tinted rows, one item selected; the History
  popover's run rows and the inline finding expansion.
- **Sentry issue list** — severity chips + verdict banner above a grouped findings list.
- **Zed's project search / git panel** — inline status + count in a toolbar; the
  target-count chips and the commit list's density.
- **Orbit's own Usage page** — the page shell: `top_bar_leading`, section cards, table
  density, tooltips (`Tooltip`), skeleton/loading treatment. All new UI uses the token
  scales (`DynamicSpacing`, `TextSize`, `Radius`, `elevation_*`) per DESIGN.md — no
  ad-hoc pixels, no new colors.

Concretely for the page's look: warm canvas, hairline separators between the three
sections, raised cards only for Run config and each run's verdict; severity tint comes
from existing `theme.del_red` / `theme.warn` / `theme.text_3`, exactly as the pane's
chips do today.

---

## 10. Phases

Each phase is independently shippable and verifiable.

**Phase 0 — model extensions** (`ai_review.rs`, pure)
`ReviewKind::{Uncommitted, Branch{base}, Commit{sha}, Files{paths}, Project}`; prompts
for each; `Report.verdict`; callout support deferred. Unit tests for every prompt and
parse case. Verify: `cargo test -p orbit-pi ai_review`.

**Phase 1 — git collection** (`git.rs`)
`commit_stats`, `target_counts`, `collect_review_diff` support for Commit/Files kinds and
for the preview's parsed snapshot. Unit tests against fixture repos (pattern exists:
`git.rs` tests build temp repos).
Verify: new tests + `cargo test -p orbit-pi git`.

**Phase 2 — background store** (`app/reviews/`)
`ReviewStore`, `ReviewTask`, `tick_reviews`, per-run cancel, concurrency cap,
persistence to `reviews.json`, completion toast. `OrbitApp` fields replaced. The old
pane still renders from the store's *latest run for the current workspace* so nothing
regresses mid-refactor. Verify: unit tests on store (queue, cancel, cap, persistence) +
manual: start a run, switch workspace, switch session, confirm it still completes.

**Phase 3 — page shell + tabs + targets** (new `review_page/`)
`ReviewPage` entity, `review_page_open` routing in `app/view.rs`, sidebar row, palette
entry, top-bar leading, the tab bar, the target card with counts, and the inline commit
list. The Running and Changes tabs can be stubs at this point (empty states only).
Verify: manual walkthrough; `cargo test` for any page-level helpers.

**Phase 3.5 — Changes tab** (shared diff widget + `review_page/`)
Extract the diff-list rendering from `sidepane.rs` into a reusable
`widgets/diff_view.rs` (both the pane and the tab mount it); build the tab: file list +
selected-file diff + open-full-diff handoff, plus the per-file checkboxes that set the
Files… target. Verify: the Review pane still renders identically (existing pane
tests/snapshots), the tab matches the pane's diff for the same source, and a 10k-line
diff scrolls smoothly.

**Phase 4 — run flow + Running tab**
Model/thinking picker wired to `ReviewRunConfig` (a popover on the page's chip),
Start/Stop, the Running tab's progress rows, results on completion, and the tab badges.
Verify: run all five targets; confirm the chat session's model is untouched; confirm
background completion while on another workspace; confirm the badge and the finished
card stay in sync.

**Phase 5 — findings polish**
Verdict line, inline finding expansion, open-file → Changes handoff, copy actions, re-run,
empty/failed states, staleness chip, markdown export. Verify: review a real diff with
planted issues; check expansion behavior and markdown output.

**Phase 6 — removal + docs**
Delete the pane's AI section and dead i18n keys; update README roadmap row,
`CHANGELOG.md`, `PRODUCT.md`, `INTENT.md`, `AGENT.md`; mark
`crates/orbit-pi/docs/ai-review-plan.md` superseded and add this plan's shipped status.
Verify: full `cargo test`, `cargo clippy`, app smoke test.

---

## 11. Open decisions (need a yes/no before implementation)

1. **History placement** — top-bar popover (proposed) vs. a fourth tab. Proposal favors
   the popover: three tabs is the limit before the page starts feeling like a dashboard;
   history is browsed occasionally. The store makes a History tab trivial later.
2. **Concurrency cap default** — 2 (proposed) vs. unlimited. Each run is a full pi
   process; unlimited is a footgun.
3. **Persist findings across restarts** — proposed yes (capped). Costs a small store
   file; gains "what did the reviewer say last week."
4. **"Fix with agent"** — proposed phase-later. It couples the review page to the active
   session; worth doing deliberately.
5. **Old pane section removal timing** — remove in the same release (proposed) vs. keep
   one release as a fallback. Proposal favors removal: two review UIs is the main risk
   of this feature.
6. **Commit list file names** — show first ~5 on the selected row (proposed) vs. none in
   the list. Proposal favors the row preview; full names are in the Changes tab.
7. **Files… picker** — checkboxes in the Changes tab (proposed) vs. a dedicated path
   editor. Proposal favors the tab: the files are already on screen.
8. **Finished runs stay on Running** — **decided: yes, in place.** A completed run keeps
   its row (status: *Review completed*) and its findings below it, exactly where it ran.
   Nothing moves and nothing auto-archives; History is for older runs. Bounding is by
   retention (§7.2), not by hiding the run the user is looking at.

---

## 12. Risks

- **Two review surfaces drifting** — mitigated by removing the pane section in the same
  release (decision 5).
- **Process spam** — mitigated by the concurrency cap and per-workspace run cleanup.
- **Long prompts for big diffs** — `cap_patch` already bounds; the Files/Project targets
  must bound their prompts too (read-from-disk prompts, not embedded text).
- **Store growth** — capped per workspace and total; prune on load.
- **Stale findings** — explicit staleness chip + re-run; never silent.
- **Preview vs. reviewer drift** — the preview shows the exact diff collection the
  reviewer receives; both come from the same `ReviewDiff` call for a target, so they
  cannot diverge.
- **Shared diff widget regressions** — extracting row painting out of `sidepane.rs` is
  the one place this plan refactors working code; it ships as an isolated phase with the
  pane's existing tests as the regression check (decision in §10, phase 3.5).
- **Minimalism hiding the feature** — the tabs carry the feature (Changes is explicit
  in the tab bar, Running is a badge), and none of the three is load-bearing for the
  primary path (pick target → Start). Guard against over-hiding: the changes summary
  line is always visible on New review, the Running badge shows activity, and a finished
  run always surfaces its findings in place.
