# Git worktrees in Orbit

Orbit treats a Git worktree as a **first-class workspace**. A worktree is a
second working directory backed by the same repository, so you can run several
independent pi sessions on different branches without stashing, switching, or
cloning. Opening a worktree in Orbit is exactly like opening any other folder:
the Explorer, Files editor, terminal, Git page, Review pane, file watcher, MCP
scope, and agent all follow the active workspace path — there is no special
filesystem mode for worktrees.

The lifecycle itself lives in [`crate::worktree`](../src/worktree.rs)
(`WorktreeManager`), the optional setup script in
[`crate::worktree_setup`](../src/worktree_setup.rs), and the UI in
[`app/worktrees.rs`](../src/app/worktrees.rs). Orbit never writes
`.git/worktrees/*` or `.git/config` by hand; every operation goes through the
`git worktree` porcelain.

## What a worktree is

```text
my-project/                  # the main worktree (a normal repository)
├── .git/
├── .wt/
│   ├── 113/                 # linked worktree → feature/issue-113
│   ├── 127/                 # linked worktree → fix/login-timeout
│   └── 141/                 # linked worktree → refactor/auth
├── src/
├── package.json
└── ...
```

Each directory is a full checkout of the repository at a different branch.
They share one object database and one set of refs; Git keeps per-worktree
HEAD/index metadata under the common `.git/worktrees/<name>` directory. A
linked worktree contains a `.git` **file** (not a directory) pointing back at
the common Git directory, which is why Orbit resolves Git paths with
`git rev-parse --absolute-git-dir` / `--git-common-dir` instead of assuming
`worktree/.git` is a directory.

## Creating worktrees

Open **Worktrees** from:

- the workspace menu in the sidebar,
- worktree settings, or
- the command palette (`Open Worktrees`).

The page lists every worktree Git reports for the active repository — including
ones created outside Orbit — and offers `+ Worktree`. The create dialog asks
for:

- **Name** — a folder identifier such as `113`. See *Naming* below.
- **Branch** — either a **new branch** (optionally starting from a chosen
  commit/branch; empty means the repository's current `HEAD`) or an
  **existing branch** (local or remote-tracking). A remote-tracking ref creates
  a local branch that tracks it, using a numeric suffix if its short name is taken.
- **Location** — derived as `<worktree root>/<name>`; the dialog previews the
  resolved path.
- **Setup** — whether to run the repository's setup script after creation
  (when one exists and automatic setup is enabled).

Under the hood:

```bash
git worktree add .wt/113 feature/issue-113        # existing branch
git worktree add -b feature/issue-113 .wt/113 main  # new branch from main
```

A branch can only be checked out in one worktree at a time. If the chosen
branch already has one, Orbit shows an actionable dialog naming the path and
offering to open that worktree instead of failing with raw Git output.

After creation, **Open new worktree after creation** (Settings → Worktrees,
on by default) switches the active workspace to the new directory. Starting a
new task there runs pi with that path as its cwd.

### From the status bar ("Work in")

The status bar shows where the next task will run. In a Git repository, the
**Work in** chip reads `Local` at the main working directory or the linked
worktree's name. Outside a repository, a folder chip takes its place. Opening
**Work in** lists:

- **Local** — the repository's main working directory.
- Every linked worktree (name over its branch).
- **New worktree** — the quick create flow.
- **Existing worktree** — choose a worktree folder to reconnect and open.
- **Remote branch** — create a worktree on a local tracking branch.
- **Clone repository** — clone a repository into a new folder.
- **Choose folder** — browse to any directory.
- **Recent folders** — reopen up to eight recent session folders, excluding the
  active folder and listed worktrees.

The new-task page's **Work in** picker offers the same choices. Its **Remote
branch** action opens the create dialog's existing-branch list. Outside a
repository, the new-task picker still offers **Clone repository** and folder
selection, but not repository-specific worktree actions.

To clone, enter a repository URL and folder name and choose a parent directory.
Git runs in the background with progress and cancellation. A successful clone
opens as a working folder and appears in the sidebar; a failed or cancelled
clone is not opened, although Git may leave a partial directory. Interactive
terminal credential prompts are disabled. The Windows job-object cancellation
path runs on the Windows CI leg; manual GUI verification remains.

Choosing a different location starts a task there and switches the active
workspace, so Explorer, terminal, Git page, Review, and the agent all follow.
A running session is parked; an idle session's transcript remains on disk.
**New worktree** opens the create dialog prefilled with a suggested,
**fully editable name** — derived from the current branch, with a numeric
suffix when taken — and a new branch derived from that name. Editing the name
updates the branch until you edit the branch yourself, so a custom name costs
one field and never silently overwrites a deliberate branch.

## Naming and branches are separate

Three identities never collapse into one:

| Identity | Example | Rules |
| --- | --- | --- |
| Name | `113` | One path component, no `/` or `\`, not `.`/`..` |
| Branch | `feature/issue-113` | Any legal Git branch name; slashes are fine |
| Path | `~/Projects/my-project/.wt/113` | Created at `<worktree root>/<name>`; may be moved later |

Renaming a worktree renames **only its directory** (`git worktree move`); the
Git branch keeps its name. The create dialog suggests a branch from the name,
but both are editable. Moving uses the same Git command with a destination path.

`git worktree list --porcelain -z` is the discovery source, so worktrees
created outside Orbit appear automatically — no re-import step.

## The worktree root

The default root is `.wt` inside the repository:

```text
repo/.wt/113
```

Settings → Worktrees → **Default location** changes it. Values are resolved
relative to the repository root, or may be absolute; `~` expands to the home
directory:

```text
.wt
worktrees
../project-worktrees
~/Developer/orbit-worktrees
```

Orbit creates the directory on demand — you never have to `mkdir .wt` first.
The resolved path is validated to stay inside the configured root, and names
are rejected if they could escape it.

A repository can override the global defaults with a local
`.orbit/worktree.json`:

```json
{
  "directory": ".wt",
  "setup_script": ".orbit/worktree-setup.sh"
}
```

Global settings live in `~/.orbit-pi/worktrees.json`. Repository-local values
win for `directory` and `setup_script`; the booleans stay global.

## Setup scripts

Repositories often need a few steps before a fresh worktree is usable —
linking a shared `.env`, sharing `node_modules`, warming a build cache. Orbit
does **not** hard-code any of those. Instead it can run a repository-provided
script after `git worktree add`:

```text
git worktree add
        ↓
worktree created
        ↓
setup script (optional)
        ↓
worktree ready
        ↓
open workspace / session
```

The default path is `.orbit/worktree-setup.sh`, configurable globally and per
repository. The script receives:

```text
ORBIT_ROOT_PATH      # main repository root, e.g. /Users/you/project
ORBIT_WORKTREE_PATH  # the new worktree, e.g. /Users/you/project/.wt/113
```

and runs with the worktree as its working directory. Example:

```bash
#!/bin/sh
ln -sfn "$ORBIT_ROOT_PATH/.env" "$ORBIT_WORKTREE_PATH/.env"
ln -sfn "$ORBIT_ROOT_PATH/node_modules" "$ORBIT_WORKTREE_PATH/node_modules"
```

Setup scripts execute arbitrary commands, so they are user-controlled:

- **Run setup script automatically** (Settings → Worktrees, on by default)
  controls whether a script runs without being asked each time.
- The first run in any repository is an explicit **Allow & Create**
  confirmation; Orbit remembers the repository in
  `~/.orbit-pi/worktrees.json` (`allowed_setup_repos`).
- The create dialog's per-worktree **Run setup script** checkbox can opt out
  for one worktree.

Execution happens on the background executor — `npm install`, `pnpm install`,
`cargo build`, or `bundle install` never block the UI. Output (stdout, stderr,
exit code) is captured and shown behind **Show Output**; on failure the
worktree is **kept**, with **Retry Setup** and **Open Worktree** offered.
A failed setup is never a reason to delete a successfully created worktree.

On Windows the script is invoked through `cmd /C`; POSIX scripts that rely on
`/bin/sh` will not run there unless a shell provides one.

## Removing worktrees

**Remove** runs `git worktree remove <path>`. Orbit checks the worktree first:

- A clean worktree asks once, with the name, branch, and path.
- A worktree with uncommitted changes says so explicitly and requires
  **Remove Anyway**, which is the only path that passes `--force`.
- A locked worktree refuses removal until it is unlocked.
- The main worktree never exposes Remove.

Removing a worktree deletes its directory and its Git administrative entry;
the branch itself is never deleted.

## Locking

`git worktree lock` marks a worktree as intentionally parked (for example, on
an external drive). Orbit shows a lock glyph in the list and disables removal
until `Unlock`. Locking is oracle for `git worktree prune`, which will not
touch a locked worktree.

## Pruning and repair

The Worktrees page's **Advanced** menu exposes two maintenance commands that
are intentionally not prominent:

- **Prune stale worktrees** — `git worktree prune`, dropping administrative
  entries whose directories are gone.
- **Repair worktrees** — `git worktree repair`, rebuilding administrative
  links after a manual move.
- **Existing worktree…** — choose an existing worktree directory and reconnect it
  with `git worktree repair` before opening it.

These operations re-list the repository afterwards. If the worktree's Git
administrative metadata is gone, Orbit reports the problem instead of opening
it as a healthy worktree.

## Worktrees and sessions

A worktree is the source of truth for a session's working directory. When a
session runs in `.wt/113`:

- pi is spawned with `.wt/113` as its cwd,
- the Explorer, Files, terminal, Git page, Review, and MCP project scope all
  read `.wt/113`,
- the session file records `.wt/113` as its `cwd`, so restarting Orbit restores
  the session to the worktree, never to the main repository.

If a session's worktree (or folder) no longer exists, Orbit shows a
**Worktree unavailable** notice with the recorded path and **Open Worktrees**.
When Git identifies the missing worktree in exactly one known repository, the
notice also offers **Start new task in Local**. This starts a separate task in
that repository's main checkout; it does not move or resume the missing session.
If ownership cannot be established, Orbit does not guess a Local folder.
It never silently re-points the session at another workspace, and it does not
spawn the agent in a fallback directory.

The workspace file watcher follows the active worktree. Because a linked
worktree's Git metadata lives outside its own tree, Orbit also watches the
per-worktree Git directory (`.git/worktrees/<name>` — HEAD, index, operation
markers) and the common Git directory's refs, so commits, checkouts, and
external `git worktree` changes refresh the UI without polling.

## External worktrees

Worktrees created outside Orbit are discovered on the next refresh:

```bash
git worktree add .wt/200 feature/issue-200
```

The page re-lists when it opens, after every mutation, and (throttled) when the
workspace watcher reports a change. Nothing in Orbit's state needs to be
recreated.

## What Orbit does not do

- No multi-window "open in new window": Orbit is a single-window workbench.
- No branch deletion as part of worktree removal; delete branches deliberately
  from the Git page.
- No automatic relocation of a session when its worktree is deleted; the
  unavailable notice is the explicit step.
