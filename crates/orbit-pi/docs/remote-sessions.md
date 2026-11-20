# Remote sessions over SSH (agent only)

Status: **shipped, agent only** (D20 in `INTENT.md`). Covered by the fake-ssh
transport test and `app/remote_ui_tests.rs`; not yet exercised against a real
VPS or in the GUI by hand.

## What it is

A session whose `pi --mode rpc` process runs on another machine (a VPS) while
Orbit stays local. pi's RPC is JSONL over stdio and ssh forwards stdio
unchanged, so the existing `PiClient` transport drives it with no remote
server, port forward or pairing step.

Only the agent runs remotely. The explorer, Git, Review and the terminal read
the local disk and are not remote-aware.

## Pieces

| Piece | Where |
|---|---|
| Target validation, ssh argv, launch script | `crates/orbit-rpc/src/remote.rs` |
| Spawn over ssh | `PiClient::spawn_remote_with_bin` (`crates/orbit-rpc/src/client.rs`) |
| Host config (`~/.orbit-pi/remote-hosts.json`) | `crates/orbit-pi/src/remote_hosts.rs` |
| Guarded spawn | `BundledExtensions::spawn_remote` (`crates/orbit-pi/src/bundled_extensions.rs`) |
| App: picker rows, spawn seam, unavailable state | `crates/orbit-pi/src/app/remote_ui.rs` |
| App: the host's Git page (read-only) | `crates/orbit-pi/src/app/remote_git_ui.rs` |
| Settings → Remote hosts: list, form, test, remove | `crates/orbit-pi/src/app/remote_hosts_ui.rs` |
| App tests | `crates/orbit-pi/src/app/remote_ui_tests.rs` |
| Transport test with a fake ssh | `crates/orbit-rpc/tests/remote_fake_ssh.rs` |

## Host config

Settings → Remote hosts writes it; hand edits keep working.

```jsonc
// ~/.orbit-pi/remote-hosts.json
{ "hosts": [ { "name": "prod-vps", "ssh": "prod-vps", "user": "deploy", "port": 22, "dir": "/srv/app" } ] }
```

`ssh` is an `~/.ssh/config` alias or a hostname (`user@host` still works);
`user` and `port` are optional and override the alias; `dir` must be absolute.
Names are unique. Bad entries are skipped with a reason; the rest still load.
Saves change only `name`, `ssh`, `user`, `port` and `dir` on an entry, so other
keys (on an entry or at the top level) survive, and a file Orbit cannot parse
is never overwritten.

## Settings → Remote hosts

Modelled on t3code's environment panel, built from the MCP page's anatomy
(`app/remote_hosts_ui.rs`):

- **Toolbar:** **Add host**.
- **Cards** (one per host, after t3code's environment cards): name with a
  green **In use** for the active remote session, `SSH user@host:port ·
  status` (Not checked / Connecting… / Connected / Needs attention / pi not
  installed / Installing pi… / Off), an on/off switch and a ⋯ menu (Test
  connection, Edit, Remove with an inline confirm). Under a hairline: the
  directory, the pi version once connected, and any problem with its fix
  (Install pi, Use opc, …). Off keeps the host saved (`"enabled": false`,
  written only when off) and hides it from the New Task folder picker.
- **Form:** SSH host or alias with `~/.ssh/config` suggestions (concrete
  `Host` aliases; patterns skipped; `Include` followed), Username and Port side
  by side, Remote directory, Display name (defaults to the host), an inline
  error, and **Test connection** / Cancel / Save. Enter saves, Escape cancels.
- **Test connection** runs `orbit_rpc::remote::probe_script` over the same ssh
  options as a launch: `cd` into the directory, check `pi` is on the login
  PATH, print `pi --version`. It writes nothing on the host. It runs on the
  background executor; a result for older form values is dropped.
- Editing the host the active remote session uses updates it for the next
  restart; the running process keeps its connection.

## Launch

```
ssh -T -o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=15 \
    -o ServerAliveCountMax=3 [-p PORT] -- HOST "sh -lc '<script>'"
```

The script, in order:

1. writes the access guard (`index.js`, `policy.js`, `package.json`) to
   `~/.orbit-pi/remote-guard/` and the workflow extension (same three files)
   to `~/.orbit-pi/remote-workflow/`, with quoted heredocs;
2. writes `{"mode":"supervised"}` to `~/.orbit-pi/access.json` on the host;
3. `cd`s into `dir` (exit 66 if missing);
4. refuses to continue without the guard (exit 78);
5. checks `pi` is on the login shell's PATH (exit 127);
6. `exec`s `env ORBIT_WORKFLOW_MODE=<mode> pi --mode rpc --approve --extension
   ~/.orbit-pi/remote-guard/index.js --extension ~/.orbit-pi/remote-workflow/index.js`.

The whole script is one argument of the remote shell's `-c`, which Linux caps
at 128 KiB; a test keeps the shipped files under 96 KiB.

**Workflow mode on the host.** `ORBIT_WORKFLOW_MODE` covers the session until
the host's store has its id: the New Task choice, the live mode on a respawn,
otherwise Build. Changing the chip mid-session writes the session's entry into
the host's `~/.orbit-pi/workflow.json` over a short ssh of its own
(`orbit_rpc::remote::set_workflow_mode_script`, run by the host's Node, merged
through a temp file and a rename); the extension reads it on its next hook. A
failed write raises a toast naming the host, so the chip never claims a mode
the host did not get.

## Safety

- **Fails closed.** No remote session starts without the guard: Orbit
  refuses if the guard could not be materialized locally, and the script
  refuses if it is missing on the host.
- **Always supervised.** The host's access mode is rewritten to `supervised`
  on every launch, whatever the local access chip says.
- **Keys first, password only when asked for.** Every connection is first
  tried with `BatchMode=yes` (keys and the agent, no prompt). See *Passwords*
  below for what happens when the server refuses every key.
- Host and directory are validated (no leading `-`, no whitespace or control
  characters, absolute path) and shell-quoted.

## Passwords

After t3code's flow (`app/ssh_auth_ui.rs`):

1. A connection (Test connection, a row's Test, or starting a task from the
   folder picker) runs the read-only probe with `BatchMode=yes`.
2. If ssh reports `Permission denied (…)` and lists `password` or
   `keyboard-interactive`, Orbit opens the **SSH password** dialog (a masked
   field that refuses copy). A server that lists only `publickey` gets an
   explanation instead of a prompt.
3. The probe runs again with `BatchMode=no`, `NumberOfPasswordPrompts=1` and
   `SSH_ASKPASS` pointed at `~/.orbit-pi/ssh-askpass/askpass.sh` (0700, holds
   no secret), with the password in `ORBIT_SSH_AUTH_SECRET` on that one ssh
   process only (`SSH_ASKPASS_REQUIRE=force`).
4. The helper prints the password for password and verification-code prompts,
   answers `no` to "trust this host key? (yes/no)", and declines key
   passphrases, so the account password is never offered as a passphrase and
   a host key is never accepted silently.
5. A refused password is forgotten at once and asked again, up to three tries.
   An accepted one is kept in memory for that ssh account (`user@host:port`)
   until Orbit quits, and reused by the session spawn and its restarts. It is
   never written to disk or logged.
6. An unknown or changed host key is reported with the `ssh …` command to run
   in a terminal to check the fingerprint.

## What a failed check says

The probe prints `orbit-probe: started` before anything else, and runs
`pi --version` with no stdin and no update check, so a failure names its step:

- **Cloud root guard.** Oracle Cloud, AWS and Ubuntu cloud images put a forced
  command in `/root/.ssh/authorized_keys` that prints `Please login as the user
  "opc" rather than the user "root".`, sleeps 10 seconds and exits **142**. No
  profile or command of Orbit's runs. Orbit reads the account out of that line
  and the row (and the form) offers **Use opc**, which saves that username and
  tests again. t3code has no special case for this; it fails generically.
- **Login profile** — no start marker on stdout and nothing on stderr: the
  host's `/etc/profile` or `~/.profile` ended the shell; the status's signal
  is named (142 = SIGALRM).
- **pi** — `pi --version (<path>) failed with status N: <pi's last line>`, so a
  different program named `pi` on the login PATH is visible.
- **pi missing** — the scripts look for `pi` on the login `PATH` and then in
  pi's usual install places (`~/.pi/agent/bin`, `~/.local/bin`,
  `~/.npm-global/bin`, `~/.bun/bin`, pnpm, nvm, `/usr/local/bin`,
  `/opt/homebrew/bin`), putting the found directory on `PATH` so an npm or nvm
  install finds its `node`. Ubuntu's `~/.bashrc` stops early outside a
  terminal, so a `PATH` line there never reaches a login shell; this is the
  usual reason a terminal finds `pi` and Orbit did not. When none exists, the
  row says pi is not installed and offers **Install pi** and **Copy command**.
  **Install pi** (`orbit_rpc::remote::install_script`, started only by that
  click) runs over the same ssh connection with no terminal and no sudo:
  Node.js 22 from `nodejs.org/dist/latest-v22.x` into
  `~/.local/share/pi-node` when the host has no Node 22.19+, checked against
  the release's `SHASUMS256.txt` (refused when no `sha256sum`/`shasum` exists,
  where pi's installer would skip the check); then `https://pi.dev/install.sh`
  saved to a temp file and run with stdin closed, which takes the installer's
  unattended route (no `/dev/tty`: install with defaults, no profile edits);
  then the session lookup confirms it and the row is tested again. pi's
  launcher adds `~/.local/share/pi-node/current/bin` to `PATH` itself, so no
  profile change is needed. **Copy command** keeps the manual route:
  `ssh -t user@host 'curl -fsSL https://pi.dev/install.sh | sh'`. t3code does
  the same for its own server (it downloads a verified runtime into
  `~/.t3/runtime`); for agent CLIs it only documents the login PATH.
- Any status above 128 is shown with its signal name.

The scripts run under the user's own login shell, as t3code's launch does:
`sh -c '<dispatch>' orbit '<script>'`, where the dispatcher runs bash, zsh or
ksh as a login shell (their own profile files set `PATH`) and has it `exec sh`
on the script; any other shell gets `sh -l`.

## Providers on a host

- **API keys, entirely in Orbit.** The host view's **Add API key** lists the
  built-in providers that accept a key (signed-in ones marked). The key is
  typed into a masked field, or **Use this Mac's key** reads this Mac's own
  `api_key` entry for that provider. It is written by
  `set_api_key_script`: the key arrives on ssh's **stdin** (never a command
  line, never a file on this Mac), and the host's Node merges
  `{"type": "api_key", "key": …}` into `~/.pi/agent/auth.json` through a
  0600 temp file and a rename, refusing a file it cannot parse.
- **Not copied:** OAuth credentials (providers rotate refresh tokens, so two
  machines sharing one sign each other out) and `!command` keys (a secret
  manager on this Mac, not the host).
- **Subscriptions** (Claude Pro/Max, ChatGPT/Codex, Copilot) need the
  provider's browser approval once per machine: **Sign in** runs pi's
  `/login` on the host over `ssh -t` in a tab of Orbit's terminal panel
  (Settings steps aside so it is in view; Windows uses the system terminal).
- The This Mac view always acts on this Mac.

## Add a project to a server

After t3code's "Add to another machine". Nothing of the working tree is
uploaded: the server clones the project with **its own git**, and Orbit copies
only the ignored files the user ticks.

- **Where:** the new-task folder picker's **Add this project to a server…**
  (when a host exists and a project is open) and a host card's ⋯ →
  **Add a project from this Mac…** (that host picked).
- **The dialog** (`app/server_clone_ui.rs`) reads the project off the UI
  thread (`server_clone::inspect`): the repository root, `origin` with any
  `user:token@` stripped (it would otherwise land on the server's command line
  and `.git/config`), and `git ls-files --others --ignored --exclude-standard
  --directory` — folders git ignores whole (`node_modules/`) are one skipped
  entry. It shows the clone URL with an SSH / HTTPS switch, the server, the
  folder (`~/<repo>` by default; `~/…` or absolute), and the ignored files with
  `.env`, `.env.*`, `.envrc` and `.dev.vars` ticked. Files over 1 MB are listed
  but cannot be ticked; at most 200 are listed.
- **Connect:** the Remote hosts check (`AuthThen::AddToServer`) against `/` —
  the password prompt, host-key, wrong-user and pi-missing explanations are
  that flow's own. pi must be on the server: its Node writes the files.
- **Clone** (`orbit_rpc::remote::clone_script`): `GIT_TERMINAL_PROMPT=0 git
  clone -- <url> <dest>` with stdin closed, so nothing waits on a prompt. A
  checkout of the same URL already there is reused; a non-empty folder is
  refused. Git's errors stay on **stdout** as `detail` lines with exit 0,
  because the ssh runner reads stderr for the *server's* login problems and a
  clone's "Permission denied (publickey)" is about the git host. They are
  explained as: git missing, host key of the git host not yet trusted, key
  refused, HTTPS login needed, repository not found.
- **Copy** (`copy_files_script`): one ssh run, the files as base64 JSON on
  **stdin**; the host's Node writes each 0600 through a temp file and a
  rename. A path that is absolute, has `..`, or reaches outside the clone
  through a symlinked folder is refused; a symlink at the file's own name is
  replaced, never written through. Files that did not copy are listed in a
  toast.
- **Then:** a host entry for the new folder is saved (`<project> on <host>`,
  reused when one already points there) and a session starts in it — in the
  workspace's own subfolder when the project was opened below its root.

Only pushed commits reach the server; uncommitted work stays on this Mac.

## Remote sessions in the sidebar

pi writes a remote session's file on the host, so this Mac's
`~/.pi/agent/sessions` never has it. The sidebar lists them from the host:

- **Listing** (`orbit_rpc::remote::list_sessions_script`, read-only): the
  host's Node walks `${PI_CODING_AGENT_DIR:-~/.pi/agent}/sessions`, newest
  files first (at most 300), and prints one JSON line per session with only
  what a row shows — id, cwd, pi's latest `session_info` name, the first user
  message's text and the newest user message's time. No transcript and no
  base64 image crosses; drafts (no user message) are skipped like locally;
  files over 16 MB are read head and tail only. `remote_sessions::parse` turns
  the lines into rows.
- **Grouping** (`remote_sessions::group`): one group per enabled host entry,
  after this Mac's projects. A server is listed once and its sessions shared
  by its entries; each session sits under the entry with the longest folder
  containing its cwd, so `demo on vps` (`~/demo`) takes its sessions from a
  `vps` entry rooted at `~`. A group shows its newest eight; a search hides
  the groups (it filters local sessions only).
- **Linked to a local project** (`RemoteHost::local_dir`, the `local_dir`
  key; `~/` is the home folder): grouped by project, the entry's header and
  sessions list inside that project's group, after its local sessions
  (`place_remote_session_rows`), and a collapsed project hides them like its
  own rows (the open remote session stays). "Add to a server" links the entry
  to the project it cloned (an existing unlinked entry for the same folder
  gains the link); Settings → Remote hosts edits it as **Local project
  folder**. Unlinked entries, entries linked to a project that is not listed,
  and the One list / Recent views keep the groups at the end.
- **When:** at launch and on ⌘R without asking for anything (a server that
  wants a password says so on its group), after a remote turn settles (the
  new session becomes listable), and from a group's refresh button — the only
  one that may open the password prompt (`AuthThen::SessionList`).
- **Resume:** the connection check (`AuthThen::ResumeSession`), then the
  remote start in the session's own cwd, then `switch_session` with the host
  path — the reattach a remote restart uses. The launch carries no
  `ORBIT_WORKFLOW_MODE` (the session's own store entry or recorded mode
  decides) and no default model (a resumed session keeps its own).

## In the app

- **Start:** the new-task folder picker lists one row per host, then **Add a
  remote host…** (opens Settings → Remote hosts with the form) or **Edit remote
  hosts…** (opens the page).
- **Shown as remote:** the folder chip and the status bar name the host
  (tooltip `ssh:dir`). The branch chip shows the **host's** branch and
  ahead/behind (`remote_git_ui::remote_branch_chip`), from the same read-only
  `git_status_script` the remote Git page uses, read when the session starts,
  after each settled turn and when the Git page opens. It is read-only — a
  checkout would change the server's files — and a press opens the remote Git
  page; a detached HEAD or a folder that is not a repository shows no chip.
  Since then the chip is the same branch **selector** as this Mac's
  (`branch_chip_face` + `BranchPicker`): it opens on the host's branches,
  newest first (`list_branches_script`, read when the chip is pressed), and a
  pick or **Create and checkout new branch…** runs on the host
  (`switch_branch_script`: `git switch`, falling back to `git checkout NAME --`
  so the name is never read as a file to restore; names checked against git's
  ref rules first). Git's errors come back as the toast; the folder is read
  again after every switch.
  The Work-in control is left out.
- **Off while remote:** Files, Git and Worktrees render "Files live on
  <host>"; the Explorer and Review toggles explain with a toast and stay closed;
  `@` mentions return nothing. The local terminal still works: it is a real
  local shell.
- **Restarts stay remote:** every respawn goes through `OrbitApp::spawn_pi`.
  ⌘N, a folder pick and opening a listed session are local and clear the
  remote state.
- **Leaving:** a remote session is stopped, not parked, with a toast.

## What works in a remote session (audit)

Where each surface gets its data decides whether it is right for a remote
session. RPC = asked of the remote pi (correct); local = this Mac's disk or a
local process (describes the wrong machine); missing = the bundled extension
behind it is not installed on the host (only the access guard is).

| Surface | Source | Remote today | Fix |
|---|---|---|---|
| Chat, streaming, tools, approvals | RPC | Works | — |
| Steer, queue, abort, compaction, retry | RPC | Works | — |
| Rewind, fork, clone | RPC | Works | — |
| `/` commands and skills in the composer | RPC `get_commands` | Works (the host's) | — |
| Image attachments | RPC base64 | Works | — |
| Model and thinking pickers | RPC `get_available_models` | Works: lists the providers signed in **on the host** | — |
| Settings → Providers & models, per machine | a **Machine** selector (This Mac / each enabled host); a host's view is read from its own `pi --list-models` over ssh (`orbit_rpc::remote::list_models_script`) — connected providers, model counts and each model's context / thinking / images; credentials never leave the host | **Done** | — |
| Settings → Providers sign-in | `pi /login` over `ssh -t` on the host during a remote session (`orbit_rpc::remote::login_script` + `interactive_ssh_command`); **Restart pi** then reattaches to the host's session file | **Fixed** | — |
| Settings → Providers API keys, custom providers | This Mac view: local `auth.json`, `models.json`. Host view: **Add API key** (pick a key-based provider, paste the key or **Use this Mac's key**; written into the host's `auth.json` by its own Node, the key on ssh stdin only — `orbit_rpc::remote::set_api_key_script`), and **Sign in** (pi's `/login` over `ssh -t` in a tab of Orbit's own terminal panel, for subscriptions) | **Done**; custom `models.json` providers on a host are edited on the host | Edit a host's `models.json` over ssh |
| Default session model | local `session-defaults.json`; a remote start arms the RPC push (`apply_default_model`), checked against the **host's** catalog | **Fixed**: applied when the host has that model, skipped silently when it does not | — |
| File attachments (non-image) | folded into `@<local path>` | **Refused** with a toast (`push_file`, and again at send for a tray filled earlier); images still work | Upload over ssh |
| `@` file mentions | local index | Off (correct) | Remote index over ssh |
| Access mode chip | local `access.json` | **Fixed**: shows Supervised with a tooltip naming the host; the menu stays shut and says why | — |
| Settings → Permissions rules | local `access-rules.json` | Not applied (guard reads the host's file) | Sync the rules file to the host at launch |
| Plan / Build / Ask chip | workflow extension, installed on the host with the guard | **Fixed**: starts in the chosen mode; changes are written to the host's `workflow.json` | — |
| Auto-title | title extension | **Missing**: remote sessions stay untitled | Install with the guard |
| Subagents (tool, Settings page) | subagents extension, local agent files | **Missing** | Install extension; agent files are the host's |
| Quota / usage meters | quota extension + `get_entries` | **Missing**: no quota for remote sessions | Install the quota extension |
| Usage page | local session files | Local history only | Read the host's sessions over ssh |
| Git page | the host's own `git`, read over ssh (`orbit_rpc::remote::git_status_script`, read-only, `GIT_OPTIONAL_LOCKS=0`) | **Done (read-only)**: the session folder's repository root, branch (or detached HEAD), upstream with ahead/behind, changed-file count, last commit and `origin` (credentials stripped), or "not a repository" / "git is not installed"; refreshed when the page opens and from its Refresh button | Staging, commit and push on the host |
| Sidebar sessions, resume | the host's own store, listed over ssh (`list_sessions_script`) | **Done**: a group per host entry; a click resumes on the host | — |
| Settings → Skills, Plugins, MCP | local files, local `pi mcp list` | Describe this Mac | Per-host versions over ssh |
| Files, Git, Review, Worktrees, Explorer | local disk | Off with an explanation (correct) | Remote file/Git over ssh |
| Terminal (⌘J) | local PTY | A local shell (honest) | Optional "ssh into host" tab |
| Commit message generation | local `pi -p` | Local (and off with Git) | — |
| Notifications, toasts, export | local UI | Works | — |

## Not done

- Remote file, Git and Review views (the panels are off, not remote-aware).
- Remote sessions in the sidebar have no pin, rename, archive, delete or
  "show more"; a group shows its newest eight, and the list is in memory only
  (it is fetched again at launch, so an offline server shows no sessions).
- The quota, title and subagent extensions are not installed on the host, so
  remote sessions have no usage meters, no auto-title and no `subagent` tool.
- Non-image file attachments are refused rather than uploaded.
- Add to a server has no file search (t3code's dialog has one) and does not
  forward this Mac's git credentials: the server needs its own access.
- MCP secrets are not forwarded.
- Password logins on Windows (the askpass helper is POSIX sh); keys work there.
- Trusting a new host key from the app: it is verified in a terminal.
