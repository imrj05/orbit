# Plan — Git page refresh: clean, modern, complete

**Status:** Phase 1 **implemented** in this change (structure + visual unification +
Graph keyboard path + i18n fix; verification below). Phases 2–3 are **proposed**,
not shipped — do not document them as features until they exist.

This document is the UI analysis of the full-page Git surface and the change
program that follows from it. It does not re-open the design system: `DESIGN.md`
is the visual spec, `orbit-design` is the component contract, and
`docs/design-system-audit.md` owns the vocabulary. The bar here is Orbit's own
rule — *instrument panel, not website*: quiet chrome, dense but legible rows,
hairlines over cards, one honest state per control.

---

## 1. UI analysis — what ships today

Verified against the code (`crates/orbit-pi/src/git_panel/`), not memory.
The page is `tab_bar` → `branch_bar` → `failure_banner` → `operation_bar` →
tab body → 6 overlay popups (`mod.rs:8182`). Six tabs, not three: the module
header was stale (fixed in Phase 1). Issues and Pulls appear only when `gh` is
installed (`mod.rs:4003`).

### Tab bar (`mod.rs:4003`)

- 40px strip, hairline bottom, selected = `surface_active` fill + medium weight,
  unselected = muted ink + `hover_quiet`. Icon + localized label per tab,
  ⌘1–⌘6 wired through `commands.rs`.
- **Verdict: already clean.** No change beyond keeping it that way — no pills,
  no gradients, no per-tab colour.

### Branch bar (`mod.rs:4090`)

- Branch controls left, transient status center-right, GitHub account chip,
  overflow menu, and a split Sync button (full sync left, step menu right) with
  a real spinner while pending.
- Gaps: the transient `status` message was fed an **untranslated literal** in
  one path (`"No workspace is available for this task."`) — fixed in Phase 1.
  Ahead/behind counts exist in state but are carried by the sync menu only;
  surfacing them at rest is Phase 3.

### Changes tab (`mod.rs:4678`)

- The tab body is the embedded Review diff (`sidepane.rs`) + the commit bar.
  Staged/Changes sections carry their own action strips (Stage all / Discard
  all / Unstage all) through `install_review_section_actions`.
- The section actions are routed by comparing the **label string** the panel
  itself fed to the section parser (`mod.rs:1030–1070`). It is not an i18n bug
  — both sides are the same `tr!` value — but string identity is a fragile
  contract between `sidepane` and the panel; a typed key is Phase 2.
- Commit box mirrors the chat composer (documented exception, `composer_shadow`),
  grows to six rows, Enter = newline, ⌘Enter = commit. This is the right anchor
  of the page and stays.

### History tab (`mod.rs:5104`)

- Filter chips (all-branches, author, path/file-history — the plumbing for
  file-scoped history already exists), keyboard list (`GitList` context:
  ↑/↓/j/k/Home/End/Enter), expanded commit detail with changed files, paging.
- Rows stacked five affordances: ref badges + subject (was **12px** — primary
  content at metadata size), avatar + author + relative time below, short hash,
  always-visible forge button, expand caret.

### Graph tab (`mod.rs:5488`)

- Same commit data with lane drawing (`layout_graph`), an all-refs toggle, and
  an `origin/*`-capable lane gutter — but three real defects:
  1. **Lanes are dashed between rows.** The gutter is a fixed 38px canvas
     centered in a row that carries 6px vertical padding plus a 1px separator
     (`commit_separator`, `mod.rs:10604`) → every lane line breaks by ~13px at
     each row boundary. Graphs read as dashed, not drawn.
  2. **No keyboard path.** `list_len`/`list_selected_index`/`open_list_cursor`
     return 0/None for `GitTab::Graph` (`mod.rs:2503–2577`), so ↑/↓/Enter do
     nothing there while they work in History.
  3. **Duplicated, drifting rows.** `graph_row` (`mod.rs:9909`) and
     `commit_row` (`mod.rs:10131`) are ~90% identical copies with small drifts
     (padding, hover fill, forge-button reveal, cursor wash). Graph rows never
     showed the keyboard-cursor wash (`row_rest`) because they predate it.

### Stashes / Issues / Pulls (`mod.rs:5034`, `5730`, `6447`)

- Stashes: list with restore/pop/drop actions.
- Issues and Pulls: list/detail split with a persisted divider, filters, comment
  drafts, new-issue form with templates, merge-method choice, avatars, forge
  links; honest `gh`-missing and sign-in states. Feature set is respectable —
  Phase 3 work here is *surfacing*, not building (see §3).

### Cross-cutting code health

- `git_panel/mod.rs` is 11,974 lines with a ~150-field struct and a 7,700-line
  `impl`; `history_tab` and `graph_tab` repeat the same error/empty/loading,
  expand-detail, separator and load-more scaffolding.
- All git I/O is off-thread (`spawn_data`), state is painted from cache, and
  failures route through `failure.rs` with recovery actions — this part is
  sound and untouched.

## 2. Design rules for this page (the anti-slop guardrails)

1. **Two ink levels per row, never badge soup.** Line 1 = primary (14px,
   `TextSize::Default`): refs + subject. Line 2 = metadata (12px, muted/
   secondary): avatar, author, time. Hash right-aligned in the code family.
   Everything else lives in the expanded detail.
2. **One visual grammar for commit rows.** History and Graph share one row
   builder; their only real difference is the leading element (marker disc vs
   lane gutter). Drift between them is what reads as "two tools in one page".
3. **Continuous lanes in Graph.** The gutter spans the full row height and
   Graph rows carry no separator hairlines; History keeps its hairlines (they
   inset past the marker and do not cut anything).
4. **Tokens only.** `DynamicSpacing`, `TextSize`, `Radius`, `button::RADIUS` /
   `input::RADIUS` / `panel::RADIUS`, semantic `theme.*` roles. No literal
   colours, no gradients, no decorative cards, no card nesting.
5. **States from the shared vocabulary.** `motion::hover` + `hover_wash`,
   `row_rest` for selection vs keyboard cursor, spinner only while pending,
   disabled with a reason.
6. **Keyboard is a first-class path, not a tooltip.** Any list that is clickable
   is in the `GitList` context; icon-only buttons keep their tooltip *and* get
   a declared key path where one exists.
7. **Localized or nothing.** Every visible string goes through `tr!`; the
   status line was the one exception and is fixed.

## 3. Change program

### Phase 1 — shipped in this change

| # | Change | Where |
|---|---|---|
| 1 | **Unified commit row**: one `commit_row_core` builds refs+subject, author meta, hash, forge link and caret; `commit_row` (History) and `graph_row` (Graph) are thin wrappers supplying their leading element. Kills ~150 duplicated lines and the drift (padding, hover, forge reveal). | `mod.rs` |
| 2 | **Subject line to 14px** (`TextSize::Default`); metadata stays 12px. Primary content no longer renders at metadata size. | `mod.rs` |
| 3 | **Continuous Graph lanes**: the lane gutter's fixed height now *defines* the Graph row's height (rows carry no vertical padding or separator in Graph), so lanes meet cleanly row-to-row instead of breaking ~13px at every boundary. History keeps its inset hairlines. | `mod.rs` |
| 4 | **Graph keyboard path**: Graph joins `list_len` / `list_selected_index` / `open_list_cursor` / `list_focus_handle` and wraps its list in `keyboard_list`, so ↑/↓/j/k/Enter expand commits exactly as in History; cursor wash now draws on Graph rows. | `mod.rs` |
| 5 | **i18n fix**: the untranslated status literal becomes `git_panel.status_no_workspace` (en.yml + glossary + regenerated locales). | `mod.rs`, `locales/*` |
| 6 | **Module header** updated to describe all six tabs. | `mod.rs` |

### Phase 2 — code health (proposed)

- Split `git_panel/mod.rs` into per-tab modules (`changes/history/graph/
  stashes/github.rs`) with shared row/list chrome in `widgets.rs`, mirroring the
  existing `git_panel/{mod,failure,widgets}.rs` split; move tests to
  `git_panel/*_tests.rs`.
- Collapse `history_tab`/`graph_tab` scaffolding (error/empty/loading, expand
  detail, load-more) into one list helper — after Phase 1 the remaining
  duplication is exactly this chrome.
- Replace the label-string section-action contract with a typed key passed
  through `set_section_actions`.
- Shared `list_filter_bar` helper for the History/Graph filter chrome.

### Phase 3 — feature completion (proposed, needs its own design pass each)

- **PR status where the work happens**: current branch's PR + check state in
  the branch bar (red/green check → Pulls detail), "Create pull request" from
  the branch menu (`gh pr create`), `gh pr checkout` from Pulls rows.
- **Line-level staging** in the Changes diff (`git apply --cached` on a selected
  range, off-thread, honest failure recovery).
- **Commit operations** from History rows: revert, cherry-pick, branch-here,
  copy full hash — behind proportional confirms.
- **Graph node interactions**: click-to-highlight a lane's branch, right-click
  node menu (checkout / branch-here / copy SHA), lane legend.
- **Section-action reason tooltips** on the commit bar's disabled states.

Each Phase 3 item updates this document's status and the usual
`INTENT.md` / `README.md` / `CHANGELOG.md` trio when it ships — not before.

## 4. Verification (Phase 1)

- `cargo check -p orbit-pi` — type-check.
- `cargo test -p orbit-pi --bin orbit-pi git_panel` — the panel's own tests
  (keyboard list, rows, tabs).
- `python3 scripts/gen_locales.py` then `python3 scripts/check_i18n.py` and
  `cargo test -p orbit-pi --bin orbit-pi i18n` — the new key.
- `python3 scripts/check_states.py`, `check_motion.py`, `check_keyboard.py` —
  state/motion/keyboard contracts on touched call sites.
- Manual (`bacon run`): Graph lanes continuous across rows; ↑/↓/Enter on
  Graph; History row hierarchy at 14/12; hover/selected/cursor states in one
  light and one dark palette.
