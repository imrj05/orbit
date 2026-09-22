# Git page

The Git page is a full main-area surface (`src/git_panel/`) that combines local
Git work with the GitHub CLI. It opens from the session-details **Commit or
push** row or the top-bar Git affordance; `escape` / **Back** returns.

## Tabs

| Tab | Source | What it does |
|---|---|---|
| **Changes** | `git status --porcelain` | Staged/unstaged lists with per-file stage/unstage and confirmed discard; Include-unstaged toggle; commit bar (branch menu, conventional-commit input, **Generate**); a collapsible **Stashes** section (stash all, pop, apply, drop). |
| **History** | `git log` | Commit rows with ref badges, author, relative time, paged. Clicking a row expands it in place: full body, author/committer facts, **Open on GitHub**, copy hash, and the changed files with a per-file actions menu. |
| **Graph** | `git log --branches HEAD` (plus `--remotes --tags` with **All refs**) | Lane graph laid out by `git::layout_graph`, with the same expandable commit detail as History. |
| **Issues** | `gh issue …` | Open/Closed/All, search, label filter/picker, **New issue**, list, and a detail view with the rendered Markdown body, comments, a comment composer, **Close/Reopen**, **Edit labels**, and **Open on GitHub**. |
| **Pull requests** | `gh pr …` | Open/Closed/Merged/All, search, **New pull request** (title, body, base picker, draft), list with CI rollup and review-decision chips, and a detail view with description, checks, commits, changed files, comments, reviews, **Approve** / **Request changes**, **Merge** (method + delete-branch confirm), **Close/Reopen**, and **Checkout branch**. |

The Issues and Pull requests tabs only appear when `gh` is installed. A missing
`gh` or a signed-out CLI renders an honest setup state instead of an empty tab.

## Commit bar

The action cluster follows Git state: uncommitted changes → **Commit** /
**Commit and push**; clean with unpushed commits → **Push** (**Publish branch**
when there is no upstream); clean and behind → **Pull**; diverged → **Merge**;
otherwise a quiet "Up to date". Push/pull never touch the message. A blank
Commit message auto-generates through `commit_message.rs`
(`pi -p --no-tools --no-session --no-extensions --no-skills --no-context-files`,
falling back to a local `type(scope): …` heuristic).

The branch chip opens a menu with **Merge branch…**, **Rebase onto…**, every
local branch (checkout; delete uses safe `-d` first, then a confirmed `-D`),
**New branch…**, and **Rename current branch…**. Merge can fast-forward, create
a merge commit, or squash; rebase autostashes a dirty worktree.

## Failure classification and recovery

All Git failures run through `git_panel/failure.rs`, which classifies raw stderr
into an `ActionError { kind, title, detail }`. The banner keeps the full output
(copyable) and offers the recovery actions that are valid for that kind:

- push rejected → **Pull**, **Merge**, **Force push (with lease)**
- diverged → **Merge**, **Rebase**
- merge/rebase conflicts → **Resolve conflicts**, **Abort**, **Continue**,
  **Skip** (rebase/cherry-pick only)
- auth → **Re-authenticate** (copies `gh auth login`), **Retry**
- no upstream → **Publish branch**; network → **Retry**

Force push and abort are confirmed. Force push always uses
`--force-with-lease`, never bare `--force`.

## Operation in progress

`git_ops::operation_state` detects a half-finished merge, rebase, cherry-pick,
revert, or bisect from the marker files Git writes under `.git`. When one is
active a single bar sits above every tab with **Continue**, **Skip** (where
valid), **Abort**, the conflict count, and clickable conflicted-file chips that
open the file in the Files editor. Continue is disabled until the conflicts are
resolved and staged.

## History detail and file actions

Selecting a commit (History or Graph) loads `git::commit_detail` — full message
body, author/committer/dates, parents, refs, and the changed files with
`--numstat` counts. Each file's `⋯` menu offers:

- **Open file** (Files editor, working tree)
- **Open diff** (Review pane, working tree)
- **Reveal in File Manager**
- **Copy path**
- **File history** (filters History to `git log --follow -- <path>`)

The History filter bar has an **All branches** toggle, removable author and path
chips (click an author name to filter by them), and **Clear**.

## Keyboard

- `⌘1`…`⌘5` switch tabs while the page is open (a no-op elsewhere)
- `escape` dismisses a modal, then the branch/ref/file popovers, then leaves the
  page via the Back affordance

## GitHub CLI dependency

All host features shell out to `gh` (`src/gh.rs`) and parse its `--json` output
into typed models. Orbit never stores a GitHub token and never calls
`api.github.com` directly. `GH_BIN` overrides the binary; the minimum supported
`gh` is 2.20.0. Commands run with `GH_PROMPT_DISABLED=1`, `GH_PAGER=cat`,
`NO_COLOR=1`, and no update notifier, off the UI thread.

`gh pr checkout` rewrites the working tree, so it is refused while the worktree
is dirty or a merge/rebase is in progress.

## Limits and deferred work

- Commit/PR changed-file lists render at most `MAX_FILE_ROWS` (300) rows and
  summarize the rest.
- Issue and PR lists are bounded at `gh`'s 50-result page and are not yet
  virtualized.
- Not yet wired: `git blame`, opening a file/diff at a specific revision (the
  Review pane and Files viewer are working-tree only), per-hunk staging,
  assignee editing on issues, label/assignee editing on PRs, a neutral review
  comment, and an inline PR diff.
