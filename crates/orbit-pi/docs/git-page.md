# Git page

The Git page is a full main-area surface (`src/git_panel/`) that combines local
Git work with the GitHub CLI. It opens from the session-details **Commit or
push** row or the top-bar Git affordance; `escape` / **Back** returns.

## Worktrees

The page always operates on the **active workspace**, which may be a linked Git
worktree (`repo/.wt/113`) rather than the main repository. Every Git command
(`status`, `diff`, `branch`, `log`, `commit`, `push`, `pull`, staging, graph,
stash) runs with that path as its cwd, so the Changes/History/Graph tabs show
that worktree's branch and working tree. Worktree lifecycle — creation,
removal, moving, locking, pruning — lives on the Worktrees page
(`app/worktrees.rs`) and is not duplicated here; see
[worktrees.md](./worktrees.md).

## Tabs

| Tab | Source | What it does |
|---|---|---|
| **Changes** | `git status --porcelain` | Staged/unstaged lists with per-file stage/unstage and confirmed discard; Include-unstaged toggle; commit bar (branch menu, conventional-commit input, **Generate**). |
| **History** | `git log` | Commit rows with ref badges, author, relative time, paged. Clicking a row expands it in place: full body, author/committer facts, **Open on GitHub**, copy hash, and the changed files with a per-file actions menu. |
| **Graph** | `git log --branches HEAD` (plus `--remotes --tags` with **All refs**) | Lane graph laid out by `git::layout_graph`, with the same expandable commit detail as History. |
| **Stashes** | `git stash list` | The stash stack newest first: **Stash changes**, then pop / apply / drop per entry. |
| **Issues** | `gh issue …` | Open/Closed/All state tabs, search, label filter/picker, **New issue** (with **Generate**), a compact list, and a detail view with the rendered Markdown body, comments, a comment composer, **Close/Reopen**, **Edit labels**, and **Open on GitHub**. |
| **Pull requests** | `gh pr …` | Open/Closed/Merged/All state tabs, search, **New pull request** (title, body, base picker, draft, **Generate**), a compact list, and a detail view with description, checks, commits, changed files, comments, reviews, **Approve** / **Request changes**, **Merge** (method + delete-branch confirm), **Close/Reopen**, and **Checkout branch**. |

The Issues and Pull requests tabs only appear when `gh` is installed. A missing
`gh` or a signed-out CLI renders an honest setup state instead of an empty tab.

## GitHub at a glance

With `gh` signed in and a remote set, two quiet chips carry what GitHub knows:

- **Repository summary** (right end of the tab bar): `owner/name`, with a lock
  when private, opens the repository; the star count beside it stars or unstars it
  with the user's own `gh` account. Visibility, language, forks, default branch and
  the description are in the tooltip. Read with `gh repo view --json`.
- **Branch pull request** (branch row, after the branch chip): `#123` tinted by its
  state, with the checks' status at the trailing edge and the title, checks and
  review decision in the tooltip. Clicking it opens the PR in the Pull requests tab.
  A pushed branch that is not the base and has no PR shows **Create PR** instead.
  Read with `gh pr view` (no number: the current branch).

Both are read when the page opens, on the refresh button, and when the branch
changes; tab switches and Git operations re-read them at most once a minute
(`GITHUB_META_TTL`). Without `gh`, signed out, or with no remote, neither shows.

## Pull request quick actions

The merge card follows the PR's state: a draft offers only **Ready for review**
(`gh pr ready`); an open PR offers **Update branch** while GitHub says it is behind
its base (`gh pr update-branch`), **Auto-merge** / **Cancel auto-merge** with the
chosen merge method (`gh pr merge --auto` / `--disable-auto`, with a note while it
is on), and **Merge**. **Back to draft** (`gh pr ready --undo`) sits with Checkout
and Close in the quiet utility row.

When a check failed, the checks card adds **Re-run failed** (`gh run rerun
--failed` for each failed GitHub Actions run) and **Fix with agent**. The latter
reads the end of each failed job's log (`gh run view --job … --log-failed`, the
last 12 KiB of at most three jobs) and starts an agent task with the PR, its branch
and those logs; checks that are not Actions jobs are named without a log. It is
enabled only while the PR's branch is checked out, and says so otherwise, so the
agent never edits the wrong branch.

## Agent menu

Issues and pull requests carry one **Agent ▾** dropdown instead of a row of
agent buttons. Every entry starts an agent task through the same path the
transcript uses (`start_github_task`); prompts are English, since they go to the
model.

- **Issue → Work on issue (new branch):** `gh issue develop <n> --checkout`
  makes a branch linked to the issue on GitHub and checks it out (refused with
  uncommitted work, like checkout), records it as `branch.<name>.orbitIssue` in
  the repository's own config, and starts the task with the issue's title and
  body (first 8 KiB). That branch's **New pull request** body then starts with
  `Closes #<n>`.
- **Issue → Fix on this branch:** the same prompt, on the branch checked out now.
- **Pull request → Review with agent / Summarize for review:** the agent reads
  the PR through `gh pr view` / `gh pr diff`; the summary changes no files.
- **Pull request → Address review comments:** reads the unresolved review
  threads (`gh api graphql`, first 100 threads, 20 comments each) and hands the
  agent each one's file, line and comments. Like **Fix with agent**, it needs the
  PR's branch checked out.

## People and "For me" filters

Issues and pull requests offer **Assign me** / **Unassign me** (`gh … edit
--add-assignee @me`) once `gh` has named the signed-in user. Pull requests add
**Request review…**, which takes GitHub logins (`@a, b c`) and runs `gh pr edit
--add-reviewer`; their requested reviewers and assignees are listed under the
summary. Both lists carry a **For me** dropdown beside the state tabs:
*Everyone*, *Review requested* (pull requests), *Assigned to me*, *Created by me*,
or *Mentioning me* (issues), mapped onto `gh`'s `@me` flags and search qualifiers.

## Pull request diff and line comments

Clicking a file in a pull request's **Files changed** expands its diff inline.
The whole diff is read once with `gh pr diff` and split per file
(`gh::parse_pr_diff`); each file shows at most 600 lines and says how many more
are on GitHub. Hovering a line reveals a comment button: the comment posts to
that line through `gh api …/pulls/<n>/comments`, anchored to the PR's head
commit — the new side for added and unchanged lines, the old side for removed
ones. The comment button is pointer-only for now (a keyboard path is not built).

## Actions tab

With `gh` installed, an **Actions** tab lists the 30 most recent workflow runs
for the current branch, or every branch (`gh run list`). A run reads like a
check (status glyph and words) with its workflow, branch, trigger and age; a
failed run offers **Re-run failed** (`gh run rerun --failed`), a running one
**Cancel run** (`gh run cancel`), and every run opens on GitHub. The list reloads
on the tab, the refresh button and after an action; it does not poll.

## Releases

The branch row's **…** menu offers **Create release…** while `gh` is signed in
and a remote exists: a tag (checked against Git's ref rules) and a **Save as a
draft** option, then `gh release create <tag> --generate-notes --target
<branch>`. The repository summary's tooltip names the latest release.

## Links and notifications

- **Copy GitHub link** is in a History commit's file menu (the file at that
  commit) and in the Files viewer's toolbar (the file at the checked-out commit,
  at the caret's line). The viewer says when the commit is not pushed yet, since
  the link only opens once it is.
- The sidebar Git button's background check also counts open pull requests
  waiting on the user's review in the current repository (`review-requested:@me`);
  that count stays until they are reviewed, unlike the "new since you looked"
  issue and pull request counts.

## Fork and push

A push GitHub refuses for lack of access (`Permission to … denied`, HTTP 403) is
classified `NoPushAccess` and offers **Fork and push** while `gh` is signed in:
`gh repo fork --remote` makes `origin` the user's fork and keeps the original as
`upstream`, `gh repo set-default` keeps pull requests aimed at the original, and
the branch is pushed to the fork.

## Cloning your repositories

The clone dialog (Worktrees page) lists up to five of the signed-in user's
repositories under the URL field (`gh repo list`, read each time the dialog opens),
filtered by what the field holds; picking one fills the URL and folder name. The
list steps aside once the field holds a URL, and without `gh` the dialog is the
plain URL form.

## Commit bar

The action cluster follows Git state: uncommitted changes → **Commit** /
**Commit and push**; clean with unpushed commits → **Push** (**Publish branch**
when there is no upstream); clean and behind → **Pull**; diverged → **Merge**;
otherwise a quiet "Up to date". A repository with commits but **no remote at
all** shows **Publish to GitHub** instead of a push with nowhere to go.

### Publish to GitHub

**Publish to GitHub** opens the one-field name prompt (the branch prompt's): the
repository name, prefilled from the folder name with characters GitHub rejects
turned into `-`, and a **Private repository** toggle that is on by default. It
runs `gh repo create <name> --source . --remote origin --private|--public`, plus
`--push` when HEAD has a commit, through the user's signed-in `gh` under their own
account. A failed push whose error is "remote not found" offers the same button in
the failure banner, but only while no remote is configured: with a remote set, the
error means a wrong URL or missing access, which a new repository would not fix.
`gh` errors (name taken, signed out, missing scope) show in the banner as `gh`
wrote them.

**New pull request** pushes the branch first, publishing it when it has no
upstream and pushing when it is ahead, because `gh pr create` runs without
prompts and cannot offer to push an unpublished branch itself. Push/pull never touch the message. A blank
Commit message auto-generates through `commit_message.rs`
(`pi -p --no-tools --no-session --no-skills --no-context-files --no-approve`,
falling back to a local `type(scope): …` heuristic that the failure banner
reports).

### Commit message context

The generator describes *what the commit records*: with **Include unstaged
changes** on it diffs the whole working tree against HEAD (and synthesizes a
`/dev/null` diff for untracked files, which `git diff` never shows); otherwise
it reads the index only. Each changed file is sent with an ORIGINAL CODE
excerpt — line-numbered, windowed 24 lines around every hunk — plus its diff,
and each regenerated attempt is told about the message you rejected so it does
not repeat it.

The prompt is sized from the selected model's context window (half the window,
at ~3 bytes per token; 384 KiB when the provider reports none), split between
diffs and source excerpts. Every changed file keeps at least 1 KiB of its diff,
so a commit of a hundred-plus files still describes each one; a diff longer than
its share keeps its first and last hunk. The model's fenced reply is used
verbatim: no bullet or period normalization.

`ORBIT_COMMIT_PROMPT_DUMP=/tmp/prompt.txt` writes the exact system and user
messages the model sees, for comparing against another client's prompt.

## Issue and pull-request draft generation

The **New issue** and **New pull request** forms have a **Generate** button beside their
heading. An optional **Notes** field steers the draft; leave it blank to infer the subject
from the repository context. Generation reuses the commit-message pattern: a one-shot,
tool-free `pi -p` call (`issue_message.rs`) that reads the current branch, the branch's
commits (against the chosen base for a PR), and its changed-file stat — never writing to
the working tree or the active session. The model is asked for a single fenced block with
the title on the first line and a Markdown body below it.

### Repository templates

Both forms discover the repository's own templates (`gh_templates.rs`, mirroring GitHub's
lookup): the `.github/ISSUE_TEMPLATE/` directory (Markdown templates with YAML front
matter, and the newer YAML issue forms, `config.yml` skipped) plus the legacy
single-file locations, and `.github/PULL_REQUEST_TEMPLATE.md` plus its variants. A form
with exactly one template applies it automatically; with several, a **Template** chip
opens a picker (including a **Blank** choice). Choosing a template prefills the title
prefix (`bug: `) and the body skeleton, and **Create** passes the template's `labels:` to
`gh`. An issue form is flattened into Markdown — each field's label becomes a heading,
its description a guidance comment, dropdown/checkbox options rendered as comments and
checkboxes — since Orbit edits plain Markdown rather than GitHub's dynamic form.

The **Generate** prompt hands the model the selected template and asks it to fill every
heading faithfully, preserving the template's checkboxes and sections. Without pi, the
heuristic drops the notes (or the branch's commit subjects) into the template's first
section.

When pi is missing, unauthenticated, or times out, the button falls back to a local
heuristic: the title comes from the first note line, else the latest commit subject, else
the branch name read as words; the body lists the branch's commits and changed files under
`## Summary` / `## What happened`. Either way the result lands in the editable title and
body fields, and **Create** is still a separate, deliberate press.

Issues and pull requests share a quiet three-line row: type, number, state glyph, relative
time and external link; a single-line title; then repository and up to two label badges
(with a `+N` overflow badge). Open items use primary title ink and closed items are
de-emphasized.

Every line of that row is a truncating run inside a flex row, with `flex_1 min_w_0` on the
text itself. A percentage-width block as a direct child of the row's column is measured
against an indefinite width: its run truncates at zero and paints nothing but the ellipsis.
The row's column therefore stretches its children rather than aligning them to the start,
and the row clips any run longer than its content box, so an over-long title stops inside the
row's padding instead of under the pane divider.

State tabs sit above one separate tool row — the search field, the repository-label picker
and **New issue** / **New pull request**. Search submits from the glyph inside its own frame,
so the field keeps the row's spare width instead of sharing it with a duplicate button; at
narrow list widths the label picker drops to its glyph, and the create action to a plus with
a tooltip, so nothing wraps away from reach.

At 800px and wider a resizable list sits beside the detail; below that the detail replaces
the list and its back arrow restores the list. The detail header keeps the item type,
number, state, full wrapping title, author/timeline and actions above a separately scrolling
reading column. Descriptions and discussions use full Markdown with hairline separation
rather than nested cards. The PR header also carries branch flow, review/check state and
diff totals; its reading column retains checks, commits, changed files, reviews, merge and
comment/review controls. Empty descriptions and unselected detail panes have explicit
states rather than blank surfaces.

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

- `⌘1`…`⌘6` switch tabs while the page is open (a no-op elsewhere)
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
- Commit-message generation sizes its prompt from the model's context window:
  half the window at ~3 bytes per token (384 KiB when the provider reports
  none), split between diffs and source excerpts, capped at 2 MiB. Every file
  keeps at least 1 KiB of diff; a set too large for even that still lists every
  changed file and says how many have no excerpt.
- Issue and PR lists are bounded at `gh`'s 50-result page and are not yet
  virtualized.
- Not yet wired: `git blame`, opening a file/diff at a specific revision (the
  Review pane and Files viewer are working-tree only), per-hunk staging,
  assignee editing on issues, label/assignee editing on PRs, a neutral review
  comment, and an inline PR diff.
