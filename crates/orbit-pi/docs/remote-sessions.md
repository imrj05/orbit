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
   `~/.orbit-pi/remote-guard/` with quoted heredocs;
2. writes `{"mode":"supervised"}` to `~/.orbit-pi/access.json` on the host;
3. `cd`s into `dir` (exit 66 if missing);
4. refuses to continue without the guard (exit 78);
5. checks `pi` is on the login shell's PATH (exit 127);
6. `exec`s `pi --mode rpc --approve --extension ~/.orbit-pi/remote-guard/index.js`.

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

## In the app

- **Start:** the new-task folder picker lists one row per host, then **Add a
  remote host…** (opens Settings → Remote hosts with the form) or **Edit remote
  hosts…** (opens the page).
- **Shown as remote:** the folder chip and the status bar name the host
  (tooltip `ssh:dir`); the branch and Work-in controls are left out.
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
| Default session model | local `session-defaults.json`, passed as spawn args | Not applied to remote spawns | Pass `--provider/--model` when the host has that provider |
| File attachments (non-image) | folded into `@<local path>` | **Broken**: path does not exist on the host | Refuse with a reason, or upload over ssh |
| `@` file mentions | local index | Off (correct) | Remote index over ssh |
| Access mode chip | local `access.json` | Host pinned to Supervised; chip shows the local mode | Show "Supervised (remote)" and lock it, or write the host's file |
| Settings → Permissions rules | local `access-rules.json` | Not applied (guard reads the host's file) | Sync the rules file to the host at launch |
| Plan / Build / Ask chip | workflow extension | **Missing** on the host: chip has no effect | Install the workflow extension with the guard |
| Auto-title | title extension | **Missing**: remote sessions stay untitled | Install with the guard |
| Subagents (tool, Settings page) | subagents extension, local agent files | **Missing** | Install extension; agent files are the host's |
| Quota / usage meters | quota extension + `get_entries` | **Missing**: no quota for remote sessions | Install the quota extension |
| Usage page | local session files | Local history only | Read the host's sessions over ssh |
| Sidebar sessions, resume | local `~/.pi/agent/sessions` | Remote sessions not listed | List the host's sessions over ssh |
| Settings → Skills, Plugins, MCP | local files, local `pi mcp list` | Describe this Mac | Per-host versions over ssh |
| Files, Git, Review, Worktrees, Explorer | local disk | Off with an explanation (correct) | Remote file/Git over ssh |
| Terminal (⌘J) | local PTY | A local shell (honest) | Optional "ssh into host" tab |
| Commit message generation | local `pi -p` | Local (and off with Git) | — |
| Notifications, toasts, export | local UI | Works | — |

## Not done

- Remote file, Git and Review views (the panels are off, not remote-aware).
- Sidebar listing and resume: pi writes session files on the host, so remote
  sessions are not in the local `~/.pi/agent/sessions` list.
- The workflow, quota, title and subagent extensions are not installed on the
  host, so Plan/Build/Ask does not apply to remote sessions yet.
- MCP secrets are not forwarded.
- Password logins on Windows (the askpass helper is POSIX sh); keys work there.
- Trusting a new host key from the app: it is verified in a terminal.
