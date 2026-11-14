# Plan — Working folder ("Work in")

**Status:** P0–P2 implemented. P3 (clone from a URL) is implemented, including Windows job-object
cancellation; the Windows runtime case now has a CI leg (`repository_clone::`, `working_folder::`,
and `worktree::` tests on `windows-latest`), and manual GUI verification remains. See §13 for the
phase gates, §17 for what shipped versus what the plan assumed, and §12 for the tests that exist.
**Scope:** one new pure module (`crates/orbit-pi/src/working_folder.rs`), a rewrite of the status
bar's folder/location controls (`app/worktrees.rs`, `app/view.rs`), the new-task page's folder
field (`workspace_picker.rs`, `app/pickers.rs`), three new `WorktreeManager` methods
(`worktree.rs`), and `en.yml` keys. **No `crates/orbit-rpc` change.**
**Related:** INTENT D14 (worktrees are workspaces), D13 (command registry), DESIGN.md *Status Row*,
`app/pickers.rs` (`start_task_in_folder`, `toggle_workspace_picker`), `app/session.rs`
(`browse_for_folder`, resume + `WorktreeDialog::Unavailable`), `sessions.rs`
(`canonical_workspace_path`), `git_ops::list_refs` / `git::RefKind::Remote`.

Sections 2–11 and 13–16 preserve the original pre-implementation snapshot and proposal, not
current behavior. §12 records the tests that shipped (the proposal's names mostly did not survive)
and §17 records what was built, where it deviates from the proposal, and what still needs
verification. The user-facing reference is
[`crates/orbit-pi/docs/worktrees.md`](../../crates/orbit-pi/docs/worktrees.md).

---

## 1. Goal

One control answers one question — **"where does this task run?"** — and answers it for every
kind of place Orbit supports:

| Place | Meaning |
|---|---|
| **Local** | the repository's main checkout (the default) |
| **Worktree** | a linked worktree of this repository |
| **Existing worktree** | a worktree directory that exists on disk but is *not* in `git worktree list` |
| **New worktree** | create one now, from a new branch or an existing one |
| **Remote** | work from a remote-tracking ref, or clone a repository from a URL |
| **Folder** | any directory, repo or not (today's recents + Browse) |

Today that question is answered by three unrelated controls that each know a different subset,
and the smallest of them is hidden exactly when it would be most useful.

## 2. What exists (verify before changing)

| Surface | Code | Behavior today |
|---|---|---|
| Status bar folder chip | `view.rs:3413` (`status-workspace`) → `pickers.rs` `on_pick_folder_click` | opens the native folder dialog, then **tears the session down / parks a run** (`start_task_in_folder`). Presented as a readout; behaves as "new task here". |
| Status bar "Work in" chip | `worktrees.rs:2682` `work_in_chip`, `2690` hides at Local | Local / linked worktrees / New worktree. **Invisible at the main checkout**, so the composer can never *enter* a worktree. |
| New-task page field | `view.rs:2856` → `WorkspacePicker` (`workspace_picker.rs`) | recent folders + Browse only. No worktree, no "create one", no remote. |
| Worktrees page | `app/worktrees.rs` | full lifecycle: create/rename/move/lock/remove/prune/repair + setup script. The only place a worktree can be created. |
| Worktree list | `refresh_worktrees` | fetched on workspace move and while the page is open — **not** when the picker opens. |
| Create dialog "Existing" branch | `worktrees.rs:197` appends `git_ops::list_refs` remote refs into one flat list | choosing `origin/feat` runs `git worktree add <path> -- origin/feat` (`worktree.rs:829`) |

## 3. The gaps

**G1 — you cannot start a task in a worktree from the new-task page.** The page that exists to
choose a working folder offers folders only; worktrees live behind a second, unrelated page.

**G2 — the "Work in" chip is invisible at the main checkout** (`worktrees.rs:2690`, defended in
DESIGN.md *Status Row*). So "switch this session's folder to a worktree" needs: open Worktrees →
click a row → come back. The one control that names the working folder is absent in the state where
every session starts.

**G3 — the status bar has two folder controls with different semantics.** `status-workspace`
restarts the session in a new folder; `status-work-in` switches workspace in place. Same concept,
two behaviors, adjacent pixels.

**G4 — "existing worktree" is not a concept.** `work_in_rows` (`worktrees.rs:2432`) lists only
what `git worktree list` reports for the active repo. Three real on-disk states are unreachable:

- a worktree **moved** on disk — Git reports the old path as `prunable`
  (`git worktree list` → `… prunable`); `git worktree repair <newpath>` re-registers it at the new
  path (verified);
- a worktree of **another repository** — opening it is fine, but it must not be merged into this
  repo's location list;
- a worktree whose admin entry is **gone** (`<main>/.git/worktrees/<id>` deleted) — the directory's
  `.git` file dangles, `git worktree repair` fails (`unable to locate repository`), and
  `git worktree add --force` refuses (`already exists`). This state is **not recoverable by
  porcelain** and must be reported honestly, never opened as if it were a healthy worktree.

**G5 — remote refs silently produce detached worktrees.** Verified against git:

| Command | Result |
|---|---|
| `git worktree add <path> origin/feat` | **detached HEAD** — no branch, nothing to push |
| `git worktree add <path> feat` (unambiguous remote) | local branch `feat` tracking `origin/feat` |
| `git worktree add -b <local> <path> origin/feat` | local branch tracking `origin/feat`, explicit |

The create dialog routes every "Existing" branch through `WorktreeManager::create`, so a user who
picks a remote branch gets a detached worktree with no signal that the commit is unsaved-able. The
flat list also doesn't mark which rows are remote.

**G6 — staleness.** The picker renders `self.worktrees`, last refreshed on a workspace move or
while the Worktrees page is open. A worktree created in another terminal is missing until a
workspace switch.

## 4. Vocabulary

- **Working folder** — the absolute directory a session runs in. Equals `current_workspace`, equals
  the pi child's cwd, equals the session's persisted `cwd`. One identity (D14).
- **Repository root** — the main checkout (`git worktree list` first row), cached as
  `worktree_repo_root`.
- **Location** — how a folder relates to Git: `Local`, `Worktree`, `Remote`-tracking,
  `PlainFolder`, `BrokenWorktree`.
- **Source** — how a *new* folder is obtained: `Local`, `ExistingWorktree`, `NewWorktree`,
  `RemoteRef`, `Clone`, `Browse`.

## 5. Data model (`working_folder.rs`, pure, no gpui)

```rust
/// Where the next task runs. Built by the picker, resolved off the UI thread.
pub enum FolderSource {
    Local,
    ExistingWorktree(PathBuf),
    NewWorktree(NewWorktreeSpec),
    RemoteRef(RemoteRefSpec),
    Clone(CloneSpec),      // P3
    Folder(PathBuf),
}

pub struct NewWorktreeSpec {
    pub name: String,
    /// Branch to create.
    pub branch: String,
    /// `None` = HEAD.
    pub start_point: Option<String>,
    /// `true` when `branch` already exists and must be checked out as-is.
    pub existing_branch: bool,
    pub run_setup: bool,
}

pub struct RemoteRefSpec {
    /// `origin/feat`
    pub remote_ref: String,
    /// Local branch to create; defaults to the short name.
    pub local_branch: String,
    pub worktree_name: String,
    pub run_setup: bool,
}

pub enum Location {
    Local,
    Worktree { name: String, branch: Option<String>, detached: bool },
    Tracking { branch: String, upstream: String },
    PlainFolder { is_repo: bool },
    BrokenWorktree { reason: BrokenReason },
}

pub struct WorkingFolder {
    pub path: PathBuf,
    pub location: Location,
    /// What the chip shows: `Local`, the worktree name, or the folder name.
    pub label: String,
}
```

`working_folder.rs` also owns the two things that must be unit-testable without a window:

- `classify(path) -> Location` — reads `<path>/.git` (dir vs `gitdir:` file), compares against the
  cached repo root, and returns `BrokenWorktree` (with the reason) instead of guessing;
- `local_branch_for(remote_ref, existing_branches) -> String` — `origin/feat` → `feat`, then
  `feat-2`, `feat-3` … until free (same loop shape as `suggest_worktree_name`).

## 6. Resolution — the table the implementation must honor

`OrbitApp::resolve_folder(source, cx)` runs the Git work in one `background_executor` task and
returns `Result<WorkingFolder, FolderError>`.

| Source | Precondition | Git | On success | On failure |
|---|---|---|---|---|
| `Local` | cached root known | — | open root | notice (`worktree.error.not_a_repository`) |
| `ExistingWorktree(p)` | listed, `p.is_dir()` | — | open `p` | `BrokenWorktree` notice → offer *Repair* (`worktree repair`) / *Remove* |
| `ExistingWorktree(p)`, prunable | `git worktree list` says prunable, `p` is a dir | `worktree repair p` | re-list, open | notice with Git's stderr |
| `NewWorktree` | name valid, target free, branch valid | `worktree add -b <branch> <path> [<start>]` | setup script → open | `BranchInUse` dialog (exists) / error |
| `RemoteRef` | `refs/remotes/<r>` exists | `worktree add --track -b <local> <path> <r>` | setup → open as `Tracking` | local branch taken → name field prefilled `feat-2` |
| `RemoteRef`, detached requested | — | `worktree add --detach <path> <r>` | open, label `(detached)` | — |
| `Folder(p)` | `p.is_dir()` | none (classification only) | open; add to sidebar | notice |
| `Clone(url)` (P3) | url parses | `git clone <url> <dir>` | then `Local` | progress + cancel |

Rules that are not negotiable:

1. **No detached HEAD by accident.** `create` gains a guard: a ref under `refs/remotes/` (or a name
   that only resolves remotely) routes to `create_tracking_branch`, never to `worktree add -- ref`.
   Detached is an explicit, labeled choice.
2. **Setup script runs before open**, as today (`begin_worktree_setup`, `ORBIT_ROOT_PATH` /
   `ORBIT_WORKTREE_PATH`); a failed setup never auto-opens.
3. **Nothing opens a folder Git says is broken.** `BrokenWorktree` renders a notice with the two
   honest actions and never sets `current_workspace`.
4. **A missing folder on resume** keeps the existing behavior (`app/session.rs:1209` →
   `WorktreeDialog::Unavailable`) and gains a *Work in* footer so the user can pick Local there.

## 7. Surface changes

### 7.1 Status bar — one chip, always visible

`status-workspace` and `status-work-in` merge into **`status-work-in`**, whose label is
`WorkingFolder::label`: `Local`, a worktree name, or a folder name. The native folder dialog moves
out of the chip (it becomes a row inside the popover), so a click never destroys a session.

Popover (width 320, sections with quiet headers, keyboard per the existing `WorkInMenu` context):

```
WORK IN
  ▣  Local                    main · ↓2
  ⑂  feat-login               origin/feat-login
  ⑂  fix-typo                 2h ago
  ─────────────────────────────────────
  +  New worktree…            from a branch
  ⑂  Existing worktree…       a folder Git already tracks
  ⇅  Remote branch…           origin/…
  ─────────────────────────────────────
  ⊞  Recent folders           (existing recents)
  …  Choose folder…
```

- The **Remote branch…** row opens a second-level list of `refs/remotes/*` minus `*/HEAD`, sorted
  by committer date, filtered by the same type-to-filter input. Selecting one creates a worktree on
  a tracking branch (G5) and opens it.
- **Clone repository…** (P3) is a row in the same group, not a section of its own.
- The **Existing worktree…** row opens the folder dialog and then the resolution table above.

### 7.2 DESIGN.md amendment (required, quoted)

Current *Status Row* bullet:

> **The "Work in" chip appears only in a linked worktree.** At the repository's main checkout the
> location is "Local" — the default — so the chip stays away. …

Replacement:

> **The "Work in" chip is always present in a repository.** It is not a readout of the default: it
> is the control that decides where the next task runs, and its rows include the places — a linked
> worktree, a new one, a remote branch — that have no other affordance in the composer. Outside a
> repository the chip is absent. The chip never restarts the session; switching location is a
> workspace move, and creating a folder is a dialog.

Also update the *Status Row* line "It carries live state only" to carve out this one control, and
delete the reference to `the_work_in_chip_is_absent_at_the_main_checkout`.

### 7.3 New-task page — same picker, same rows

`render_empty_state`'s "Workspace" field keeps its layout; its popover is the same component as
§7.1, with the recents group last. `WorkspacePicker` is extended rather than replaced: it gains
`entries: Vec<FolderRow>` (kind, label, detail, path/source) and an `on_run(FolderSource)` callback;
`on_browse` stays as the "Choose folder…" row. A row that creates something (new worktree, remote,
clone) routes to the existing create dialog instead of spawning immediately — a task never starts
in a folder that does not exist yet.

### 7.4 Worktrees page

Unchanged as the management surface (rename/move/lock/remove/prune/repair/setup). Adds one header
action, **Adopt folder…**, running the `ExistingWorktree` repair path from the table in §6, and one
row state for `prunable` entries (*Repair*), which is already modeled in `Worktree` but not offered.

## 8. "Remote", precisely

"Remote" has three possible readings. They cost very different amounts, and only the first is a true
gap in the worktree model:

| Reading | What it is | Cost | Verdict |
|---|---|---|---|
| **Remote-tracking ref** | work on `origin/feat` via a local tracking branch | small: one `WorktreeManager` method + picker rows + the G5 guard | **in scope, P0–P1** |
| **Clone from a URL** | a new working folder that doesn't exist locally yet | medium: off-thread `git clone`, progress, directory choice, first-branch selection, sidebar registration | **in scope, P3** |
| **Remote machine / SSH / devcontainer** | run the agent on another host | large: transport, path mapping, file watch, terminal, Git page — a second filesystem abstraction (contradicts D14) | **out of scope**; needs its own plan |

## 9. State, refresh, staleness

- Extend the existing fetch, don't add a second one: `refresh_worktrees` already returns
  `(root, list)`; it also returns `remotes: Vec<RefEntry>` (reusing `git_ops::list_refs`, remote rows
  only, `*/HEAD` dropped) into a new `worktree_remotes` field.
- Opening the picker sets `worktree_stale = true` and triggers one fetch; rows render from the
  cached catalog and refresh in place when it lands. A workspace switch already bumps
  `worktree_fetch`, which invalidates a late answer — keep that guard for the remotes list too.
- `worktrees_busy` stays the single in-flight flag; no new throttle.

## 10. Session binding and honesty

A session's folder is its persisted `cwd` — already true; nothing new is stored. The rules that
must be visible in the UI:

- Picking a location never spawns a task by itself on the status bar (it is a workspace move,
  D14). On the new-task page it is the task's folder.
- While a run is in flight, the status-bar picker shows one quiet line at the bottom —
  *"Switching folder parks the running session."* — because `start_task_in_folder` parks it
  (`pickers.rs:126`). One sentence, only while `is_running()`.
- A folder that Git refuses to open is reported with Git's own stderr, never a generic failure.

## 11. i18n

New keys (append after the existing `worktree.work_in.*` block in `en.yml`, then
`python3 scripts/gen_locales.py` + `check_i18n.py` + `cargo test -p orbit-pi --bin orbit-pi i18n`):

| Key | English |
|---|---|
| `working_folder.title` | `Work in` |
| `working_folder.local_hint` | `This repository's main checkout.` |
| `working_folder.new` | `New worktree…` |
| `working_folder.new_hint` | `Create an isolated worktree and switch to it.` |
| `working_folder.existing` | `Existing worktree…` |
| `working_folder.existing_hint` | `Open a worktree folder that already exists on disk.` |
| `working_folder.remote` | `Remote branch…` |
| `working_folder.remote_hint` | `Start from a branch that only exists on the remote.` |
| `working_folder.remote_local_name` | `Local branch` |
| `working_folder.clone` | `Clone repository…` |
| `working_folder.choose` | `Choose folder…` |
| `working_folder.recent` | `Recent folders` |
| `working_folder.switch_parks` | `Switching folder parks the running session.` |
| `working_folder.detached_note` | `No branch — commits here are not on a branch.` |
| `working_folder.broken` | `This folder's worktree metadata is gone.` |
| `working_folder.repair` | `Repair` |
| `working_folder.prunable` | `Git lists this worktree as prunable.` |

Delete: `worktree.work_in.*` (five keys) once no call site remains.

## 12. Tests (as shipped)

There is no `app/working_folder_tests.rs`: the gpui tests live beside the picker they drive, in
`app/worktree_chip_tests.rs`, with the layout regressions in `app/workspace_picker_layout_tests.rs`.
Run them with `cargo test -p orbit-pi --bin orbit-pi <filter>`; `repository_clone::`,
`working_folder::`, and `worktree::` are the three non-gpui modules that also run on the Windows CI
leg (see §13, P3), because the job-object cancellation path has no local Windows coverage.

**`crates/orbit-pi/src/worktree.rs`** (real temp repos, the in-file `TempRepo` helper):
- `a_remote_ref_creates_a_tracking_branch_not_a_detached_head` — the P0 regression: `branch ==
  Some("feat")`, `detached == false`.
- `create_routes_a_remote_only_ref_to_a_tracking_branch` — the `create` guard (rule 1 in §6).
- `a_tracking_branch_whose_local_name_is_taken_is_named_not_suffixed` — asserts both callers:
  `create_tracking_branch` refuses with `BranchExists`, and `create` picks `feat-2` for a caller that
  has no name yet (see §17).
- `a_tracking_branch_rejects_an_unknown_remote_ref`.
- `a_moved_worktree_is_adopted_at_its_new_path`, `adopting_a_plain_folder_reports_not_a_worktree`,
  `repair_succeeds_on_a_healthy_repository`.
- No `--detach` test, because no UI can request a detached worktree (see §17).

**`crates/orbit-pi/src/working_folder.rs`** (filesystem only, no Git child):
`the_main_checkout_classifies_as_local`, `a_plain_folder_is_not_a_worktree`,
`a_linked_worktree_reports_its_owner`, `a_worktree_whose_admin_entry_is_gone_is_broken_not_usable`
(the §3 G4 unrecoverable state), `a_missing_directory_is_broken`,
`an_unreadable_gitfile_is_broken`, `location_rows_put_local_first_then_worktrees`,
`location_rows_are_empty_without_a_main_worktree`, `a_plain_folder_row_is_labelled_by_its_directory_name`,
`a_local_folder_is_labelled_local`, `remote_rows_skip_head_and_build_a_local_branch`,
`remote_rows_suffix_a_local_branch_that_is_taken`, `two_remotes_with_the_same_branch_stay_reachable`,
`a_remote_with_a_slash_is_split_at_the_longest_name`, `a_deleted_remotes_ref_still_produces_a_row`,
`local_branch_for_keeps_the_name_when_free`.

**`crates/orbit-pi/src/app/worktree_chip_tests.rs`** (gpui, no network):
- Chip: `the_chip_is_present_at_the_main_checkout_and_reads_local` (replaces the deleted
  `the_work_in_chip_is_absent_at_the_main_checkout`), `the_work_in_chip_is_absent_outside_a_repository`,
  `the_chip_names_the_active_linked_worktree`.
- Rows: `the_picker_lists_locations_then_the_ways_to_a_new_one`,
  `choosing_the_active_folder_is_not_a_change`, `a_prunable_row_goes_through_the_repair_path`,
  `a_remote_row_opens_the_dialog_on_a_tracking_branch`.
- Honesty (§10): `a_missing_recent_folder_does_not_change_the_workspace`,
  `a_broken_recent_worktree_does_not_change_the_workspace`.
- Recents: `recent_folders_are_deduplicated_from_worktrees_and_limited`,
  `keyboard_navigation_reveals_the_last_recent_folder`.
- Keyboard and layout: `working_folder_hints_match_the_registered_keys`,
  `work_in_keyboard_hints_fit_at_maximum_ui_scale`, `escape_leaves_the_remote_level_before_closing_the_picker`,
  `remote_rows_fit_without_overlap_and_keyboard_reveals_the_last`,
  `long_remote_details_are_keyboard_accessible_and_bounded`,
  `the_status_bar_picker_rows_tile_at_one_two_line_height`.
- Clone (P3): `clone_action_opens_a_dialog_without_starting_a_task`,
  `clone_dialog_rejects_missing_inputs_without_starting`,
  `clone_dialog_keyboard_walks_controls_and_submits_validation`,
  `clone_dialog_keyboard_clones_a_local_repository`, `clone_dialog_cancellation_signals_the_operation`.
- Missing-worktree recovery: `missing_worktree_local_is_found_only_while_git_lists_it`,
  `resuming_a_missing_cold_session_does_not_redirect_it`,
  `missing_worktree_only_offers_local_when_an_owner_is_known`,
  `ambiguous_missing_worktree_owner_has_no_local_fallback`.

**`crates/orbit-pi/src/app/workspace_picker_layout_tests.rs`** — the two-line row box:
`picker_rows_tile_at_one_two_line_height`, `a_filter_that_matches_nothing_keeps_the_action_rows`.

**`crates/orbit-pi/src/app/new_task_panels_tests.rs`** —
`clone_is_available_on_the_new_task_page_outside_a_repository`.

**`crates/orbit-pi/src/repository_clone.rs`** — `clones_a_local_bare_repository_and_reports_progress`,
`clones_a_local_bare_repository_from_a_file_url` (Unix), `clones_into_an_existing_empty_directory`,
`missing_repository_reports_git_failure`, `failure_does_not_overwrite_existing_files`,
`cancellation_after_progress_does_not_report_success`, `cancellation_before_start_does_not_create_destination`,
`long_unterminated_stderr_is_chunked_and_keeps_a_bounded_tail` (Unix),
plus the process-tree cases: `cancellation_stops_transport_children` and
`cancellation_stops_transport_after_parent_exits` (Unix) and the job-object equivalent,
`cancellation_stops_windows_transport_after_parent_exits` (Windows only — see §13).

**Not written, and why:** `a_remote_ref_whose_short_name_is_taken_is_suffixed` became
`a_tracking_branch_whose_local_name_is_taken_is_named_not_suffixed`; `a_detached_request_is_explicit_and_labeled`
has no subject because the UI cannot request a detach; `filter_matches_branch_and_path` has no
subject because the status-bar popover has no type-to-filter (see §17).

## 13. Phases

| Phase | Content | Gate |
|---|---|---|
| **P0** | `working_folder.rs` (classification, specs, `local_branch_for`); `WorktreeManager::create_tracking_branch` + the `create` remote guard; `repair`-on-open path | `cargo test -p orbit-pi worktree::` green, incl. the detached-HEAD regression |
| **P1** | status bar: merged chip, always visible in a repo; popover sections; remotes in the fetch; DESIGN.md amendment | `cargo test -p orbit-pi working_folder` + manual `bacon run`: main checkout → New worktree → task starts there; remote row → tracking branch, `↑0 ↓0` visible |
| **P2** | new-task page uses the same picker; `FolderRow`/`on_run` in `WorkspacePicker`; "parks the running session" line; Adopt folder… on the Worktrees page | new-task tests + manual: pick an existing worktree from the empty state |
| **P3** | `Clone(CloneSpec)`: directory choice, off-thread clone with progress, cancel, then `Local`; sidebar registration | clone tests against a local bare repo fixture, including the Windows job-object case, which now runs on the `windows-latest` CI leg; manual clone of a public repo |
| **P4** | polish: recents merge, keycaps, CHANGELOG + README/INTENT *Implementation status* | docs consistency pass |

**Verification per phase** (AGENTS.md): `cargo check -p orbit-pi` → filtered `cargo test -p orbit-pi
--bin orbit-pi <filter>` → `bacon run` for the flow → `python3 scripts/check_i18n.py` after string
edits → `python3 scripts/check_states.py` / `check_motion.py` / `check_keyboard.py` because new
interactive elements and one focus context are added.

## 14. Risks

- **Chip always visible = new chrome.** Mitigated by: it replaces an existing chip (net zero
  pixels on the status bar), and it is absent outside a repository.
- **Two-level popover** ("Remote branch…") is new interaction inside a picker. Keep it a real
  second level with a back row, not a hover submenu — hover submenus already bit this app
  (`menu_dismissed_at` click-through guard).
- **Repair writes Git metadata.** `git worktree repair` is porcelain and safe, but the action must
  be explicit (a row action), never automatic on open.
- **Clone (P3) is the only networked step**; it must be cancellable and must never register a
  half-cloned directory in the sidebar.
- **D14 tension:** every row ends in `current_workspace` or a plain folder. Nothing here adds a
  second filesystem abstraction, and the plan explicitly excludes SSH for that reason.

## 15. Out of scope

Remote-host execution (SSH/devcontainer), multi-root workspaces, per-session folder overrides that
diverge from pi's `cwd`, worktree templates, and automatic branch-name generation from the first
prompt (a title-driven name would start the task before the workspace exists).

## 16. Open questions (answered)

1. **Which "remote"?** Remote-tracking refs and clone-from-URL; both exclude remote machines.
   Answer: implement the refs (P0–P1), keep clone (P3). A remote host needs its own plan.
2. **Chip label.** `Local` / `feat-login` / `my-folder`, no repository prefix — the sidebar's
   active project already answers "which repository".
3. **Existing worktree…** opens the native picker at the last-used folder (the platform default),
   then reconnects through `OrbitApp::adopt_worktree`. Defaulting to the worktree root would hide
   the folders Orbit does not know about, which is the case this row exists for.

## 17. Implementation notes (shipped work and remaining verification)

- **`working_folder.rs`** — `classify` (filesystem only), `Location` / `BrokenReason`, `FolderRow`,
  `location_rows`, `remote_ref_rows`, `local_branch_for`, `short_branch_name`. Unit tests cover the
  live-worktree, plain-folder, missing-directory, unreadable-gitfile, and lost-admin-entry cases,
  plus remote-name splitting and collision suffixing.
- **`worktree.rs`** — `create_tracking_branch` (`worktree add --track -b`), the `create` guard that
  routes a remote-only ref through it, `local_branches`, `adopt` (`worktree repair`, with a
  `BrokenWorktree` error for the unrecoverable state), and `WorktreeError::{BranchExists,
  BrokenWorktree}`. Six new tests, including the detached-HEAD regression.
- **Status bar** — one chip (`status-work-in`), always present in a repository, `Local` at the main
  checkout. Two levels: locations + verbs, then remote refs. `Dismisses` the picker from every
  other popover through `dismiss_work_in_menu`. DESIGN.md's *Status Row* bullet is amended.
- **New-task page** — the same vocabulary through `WorkspacePicker` (entry marks, action rows,
  divider). "Remote branch…" opens the create dialog on its existing-branch list rather than a third
  picker screen; the dialog now states which local branch a remote ref creates.
- **Semantics correction found while wiring it** — `open_worktree` used to move `current_workspace`
  without moving the agent, leaving the live pi process rooted in the previous folder. Choosing a
  folder now starts the task there (`start_task_in_folder_quiet`), which parks a running session and
  keeps an idle one's transcript on disk. The popover says so while there is a conversation to
  leave.
- **Row system (UI pass after review)** — both lists now draw rows from one builder,
  `helpers::picker_row_text`, inside `picker::two_line_entry_height`: a title over a secondary line,
  each on a fixed `BufferLineHeight::Comfortable` box. The first cut pinned two-line rows to the
  *one-line* picker height, so every hint painted over the next row's title and the keyboard reveal
  scrolled to a wrong offset; the field label and the picker header also disagreed
  ("Workspace" vs "Work in"). Three regressions hold it now:
  `picker_rows_tile_at_one_two_line_height`, `a_filter_that_matches_nothing_keeps_the_action_rows`,
  and `the_status_bar_picker_rows_tile_at_one_two_line_height`. Filtering no longer replaces the
  whole list — a search that finds no folder keeps "New worktree…", since that is the way out.
- **Second UI pass (dropdown alignment)** — the row body is now `PickerRow`, which *is* the
  flexible child (`flex_1` + `min_w_0`). It used to sit inside a wrapper div added only to set the
  title weight; the wrapper sized to its content, so the selection check landed right after the
  label and long text overflowed the popover and was clipped without an ellipsis. Weight is now a
  builder parameter, and the status-bar popover is a compact menu: two-line location rows (name,
  branch) and **one-line verbs** whose sentence-length hints moved to tooltips, because a 300px
  menu row cannot hold "Open a worktree folder that already exists on disk." The new-task page's
  picker keeps the hint lines — it has the width for them. The check's position is pinned by the
  same test (`work-in-check` must sit right of the row's centre).
- **Missing-worktree recovery**: the unavailable notice offers to start a new task in Local only
  when Git lists the missing path in exactly one known repository. Orbit rechecks ownership before
  opening and never moves the missing session; unknown or ambiguous owners get no Local shortcut.
  A warm process stays parked if its folder is missing.
- **P4 keycaps**: both working-folder pickers show the existing Up/Down, Enter, and Escape
  bindings in a compact, localized legend. Escape says **Back** on the remote-branch level and
  **Close** elsewhere; the legend wraps when space is tight.
- **P4 recents**: the status-bar picker lists up to eight recent session folders after the actions,
  excluding the active folder and listed worktrees. Missing or broken folders are reported instead
  of opened. Keyboard navigation scrolls the highlighted row into view.
- **P3 (clone) — code and non-GUI tests shipped; manual GUI verification pending**: the
  status-bar and new-task pickers offer cloning via a URL, folder name,
  and destination picker. Git runs in the background with progress and process-tree cancellation
  (a process group on Unix, a job object on Windows); a successful clone opens as a working folder
  and is added to the sidebar. Local bare-repository tests and a public GitHub backend clone pass.
  Failed or cancelled clones are not opened, but Git may leave a partial directory; interactive
  terminal credential prompts are disabled. The Windows job-object case has a test
  (`cancellation_stops_windows_transport_after_parent_exits`) but nothing ran it: CI built on
  `windows-latest` and ran no tests. `.github/workflows/ci.yml` now runs the three platform-sensitive
  modules (`repository_clone::`, `working_folder::`, `worktree::`) on every leg, with the general
  no-tests-in-CI rule unchanged elsewhere. Manual GUI verification of the clone flow still remains.

### Deviations from the proposal above

Recorded so the document stops promising things the tree does not do:

- **Explicit detach is not exposed.** §6 lists a `--detach` row and §11 a `detached_note` string;
  neither exists. Every path to a worktree goes through a branch, so the *no detached HEAD by
  accident* rule holds by having no detach at all. `Location::Worktree.detached` is still parsed and
  labelled, because a hand-made detached worktree can appear in `git worktree list`.
- **A taken local branch name is split by caller, not by the manager.** `create_tracking_branch`
  refuses an existing name with `WorktreeError::BranchExists` (rendered by `worktree.error.branch_exists`),
  and a caller that has not chosen a name yet — the create dialog's existing-branch list — goes through
  `create`, which picks the free name itself via `local_branch_for` (`origin/feat` → `feat-2`). One
  function never guesses and the other never asks; both tests assert the pair.
- **Adopt lives in the Worktrees page's Advanced menu** (`wa-adopt`), not as a page-header action
  (§7.4). One menu entry, same `adopt_worktree` path.
- **The status-bar popover has no type-to-filter.** §7.1 proposed reusing the picker's filter on
  both levels. The popover is a compact menu with keyboard navigation instead; only the new-task
  page's `WorkspacePicker` keeps a filter field, and the remote level is reached with a *Branch
  details* row rather than a search.
- **No `worktree_stale` flag.** §9's "opening the picker marks the catalog stale" became
  `worktree_remotes_busy` plus the existing `worktree_fetch` guard; the remotes list is loaded on
  demand (`load_worktree_remotes`) rather than folded into `refresh_worktrees`.
- **i18n keys renamed while writing the UI.** `working_folder.local_hint` became `local` (a chip
  label, not a hint line), `remote_local_name` became `local_detail_label`, and `clone` became
  `clone_title` plus the `clone_*` family (`clone_url`, `clone_name`, `clone_parent`,
  `clone_fields_required`, `clone_action`, `clone_running`). `switch_parks`, `repair`, and
  `detached_note` never landed: the parks sentence is `working_folder.new_task_note`, shown only
  when a conversation would be left; repair reuses `worktree.advanced.repair`. Keys the plan did not
  anticipate: `hint_navigate`, `hint_select`, `tracking_hint`, `connecting`, `repaired`,
  `remote_details`, `remote_empty`, `remote_creates`, `prunable`.
- **No `app/working_folder_tests.rs`.** The gpui coverage sits in `app/worktree_chip_tests.rs` and
  `app/workspace_picker_layout_tests.rs`; §12 lists the shipped names.
