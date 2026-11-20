//! Remote pi sessions over SSH.
//!
//! `ssh <host> sh -c '<login dispatch>' orbit '<launch script>'` runs `pi --mode rpc` on another
//! machine. The RPC protocol is already JSONL over stdio, and ssh forwards
//! stdio unchanged, so the same [`PiClient`](crate::PiClient) transport drives
//! the remote agent. Only the agent runs remotely: files, Git and the explorer
//! stay on the local machine (see `crates/orbit-pi/docs/remote-sessions.md`).
//!
//! Safety: the launch script installs Orbit's access guard on the host and
//! refuses to start pi unless the guard file is in place, so a remote session
//! never runs unguarded. The host's access mode is pinned to `supervised` on
//! every launch.

use anyhow::{bail, Result};

/// Where the guard lives on the remote host, relative to `$HOME`.
pub const REMOTE_GUARD_DIR: &str = ".orbit-pi/remote-guard";

/// Exit status when the guard is missing (`EX_CONFIG`). Reported by the launch
/// script so the app can explain it instead of showing a bare exit.
pub const GUARD_MISSING_EXIT: i32 = 78;

/// The variable the askpass helper reads the password from. It is set on the
/// one ssh process that needs it and never written anywhere.
pub const ASKPASS_SECRET_ENV: &str = "ORBIT_SSH_AUTH_SECRET";

/// The `SSH_ASKPASS` helper (POSIX sh). ssh runs it for every question it
/// would otherwise ask on a terminal, with the question as `$1`:
///
/// - "(yes/no…)" — trusting an unknown host key. Answered `no`: a host key is
///   verified by the user in a terminal, never accepted silently.
/// - "passphrase" — unlocking a local key. Declined, so the account password
///   is never offered as a key passphrase; ssh moves on to the next method.
/// - anything else — a password or keyboard-interactive code: the secret.
pub const ASKPASS_SCRIPT: &str = r#"#!/bin/sh
# Orbit's ssh-askpass helper. Orbit asks for the password in its own dialog and
# passes it to this one ssh process in ORBIT_SSH_AUTH_SECRET; nothing is stored.
case "$1" in
  *yes/no*) printf 'no
'; exit 0 ;;
  *assphrase*) exit 1 ;;
esac
if [ "${ORBIT_SSH_AUTH_SECRET+x}" = "x" ]; then
  printf '%s
' "$ORBIT_SSH_AUTH_SECRET"
  exit 0
fi
printf 'orbit: ssh-askpass called without a password
' >&2
exit 1
"#;

/// A password for one ssh run: the helper script on disk and the secret.
#[derive(Clone, Copy)]
pub struct SshPassword<'a> {
    pub askpass: &'a std::path::Path,
    pub secret: &'a str,
}

/// Point `command` at the askpass helper with `password`. `SSH_ASKPASS_REQUIRE
/// =force` makes ssh use the helper with no terminal; a `DISPLAY` is supplied
/// when unset because older ssh builds only consult askpass when one exists.
pub fn apply_password(command: &mut std::process::Command, password: SshPassword<'_>) {
    command
        .env("SSH_ASKPASS", password.askpass)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(ASKPASS_SECRET_ENV, password.secret);
    if std::env::var_os("DISPLAY").is_none() {
        command.env("DISPLAY", "orbit");
    }
}

/// Whether ssh's stderr says the server refused every credential offered —
/// the case a password can fix.
pub fn is_auth_failure(stderr: &str) -> bool {
    let text = stderr.to_lowercase();
    let denied = text.match_indices("permission denied (").any(|(ix, _)| {
        let methods = &text[ix + "permission denied (".len()..];
        [
            "publickey",
            "password",
            "keyboard-interactive",
            "hostbased",
            "gssapi-with-mic",
        ]
        .iter()
        .any(|method| methods.starts_with(method))
    });
    denied
        || text.contains("authentication failed")
        || text.contains("too many authentication failures")
}

/// Whether the server allows password (or keyboard-interactive) logins at
/// all, read from the methods ssh lists after `Permission denied`.
pub fn offers_password(stderr: &str) -> bool {
    let text = stderr.to_lowercase();
    text.contains("password") || text.contains("keyboard-interactive")
}

/// Why ssh refused the host key, when it did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyProblem {
    /// The host is not in `known_hosts` yet.
    Unknown,
    /// The host's key differs from the one trusted before.
    Changed,
}

/// Read a host-key refusal out of ssh's stderr.
pub fn host_key_problem(stderr: &str) -> Option<HostKeyProblem> {
    if stderr.contains("REMOTE HOST IDENTIFICATION HAS CHANGED") {
        Some(HostKeyProblem::Changed)
    } else if stderr.contains("Host key verification failed") {
        Some(HostKeyProblem::Unknown)
    } else {
        None
    }
}

/// A remote working directory reached over SSH.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTarget {
    /// An `~/.ssh/config` alias or `user@host`. Passed to ssh after `--`.
    pub host: String,
    pub port: Option<u16>,
    /// Absolute path on the remote host, where pi is rooted.
    pub dir: String,
}

/// One file the launch script writes into [`REMOTE_GUARD_DIR`].
#[derive(Debug, Clone, Copy)]
pub struct RemoteFile<'a> {
    pub name: &'a str,
    pub contents: &'a str,
}

/// An extension the launch script installs beside the guard, in
/// `~/.orbit-pi/<dir>` on the host, and loads with `--extension`. Unlike the
/// guard it is not a safety boundary: it adds behavior (the workflow scope),
/// so a session never refuses to start over one.
#[derive(Debug, Clone, Copy)]
pub struct RemoteExtension<'a> {
    /// A plain directory name under `~/.orbit-pi` (`remote-workflow`).
    pub dir: &'a str,
    /// Its files; one must be `index.js`, the entry point.
    pub files: &'a [RemoteFile<'a>],
}

/// Everything a remote session launch carries besides the target.
#[derive(Debug, Clone, Copy, Default)]
pub struct RemoteLaunch<'a> {
    /// The access guard, written into [`REMOTE_GUARD_DIR`]. Required: the
    /// script refuses to start pi without it.
    pub guard: &'a [RemoteFile<'a>],
    /// More extensions, each in its own directory.
    pub extensions: &'a [RemoteExtension<'a>],
    /// Environment for pi (`ORBIT_WORKFLOW_MODE`). Names must be plain
    /// `[A-Z_][A-Z0-9_]*`; values are shell-quoted. Never a secret: the
    /// script travels on the ssh command line.
    pub env: &'a [(&'a str, &'a str)],
}

impl RemoteTarget {
    /// Reject anything that could change the meaning of the ssh command line
    /// or the remote script. Values are also shell-quoted; this is the second
    /// line of defense, and it turns bad config into a clear error.
    pub fn validate(&self) -> Result<()> {
        let host = self.host.trim();
        if host.is_empty() {
            bail!("remote host is empty");
        }
        if host.starts_with('-') {
            bail!("remote host `{host}` must not start with `-`");
        }
        if host.chars().any(|c| c.is_whitespace() || c.is_control()) {
            bail!("remote host `{host}` contains whitespace or control characters");
        }
        if self.port == Some(0) {
            bail!("remote port must be between 1 and 65535");
        }
        if !self.dir.starts_with('/') {
            bail!("remote directory `{}` must be an absolute path", self.dir);
        }
        if self
            .dir
            .chars()
            .any(|c| c == '\n' || c == '\r' || c == '\0')
        {
            bail!("remote directory contains a line break or NUL");
        }
        Ok(())
    }
}

/// Quote one word for POSIX `sh`: wrap in single quotes, escaping embedded
/// single quotes. Always safe for any input.
pub fn shell_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "'\\''"))
}

/// Runs a script in the user's own login environment, the way t3code
/// launches: bash, zsh and ksh read their own login files (where installers put
/// `PATH`), then hand the script to a plain `sh`, so the script never depends
/// on the login shell's dialect. Any other shell gets `sh -l`. `$1` is the
/// script.
pub const LOGIN_DISPATCH: &str = r#"case "${SHELL##*/}" in bash|zsh|ksh) exec "$SHELL" -l -c 'exec sh -c "$1"' orbit "$1" ;; *) exec sh -l -c "$1" ;; esac"#;

/// Exit status when no `pi` is found, on PATH or in the usual install places.
pub const PI_MISSING_EXIT: i32 = 127;

/// What the scripts print (on stderr) when no `pi` is found.
pub const PI_MISSING_MESSAGE: &str = "orbit: pi is not installed on this host";

/// Find `pi` and leave its path in `$PI`. Login `PATH` first; then the places
/// pi's installers and npm put it, which a non-interactive login shell often
/// misses (Ubuntu's `~/.bashrc` stops early outside a terminal, so a `PATH`
/// line added there never applies). A pi found outside `PATH` brings its own
/// directory onto `PATH`, so the `node` beside an npm or nvm install resolves.
fn locate_pi() -> String {
    format!(
        r#"PI=$(command -v pi 2>/dev/null || true)
if [ -z "$PI" ]; then
  for c in "$HOME/.pi/agent/bin/pi" "$HOME/.local/bin/pi" "$HOME/.npm-global/bin/pi" "$HOME/.bun/bin/pi" "$HOME/.local/share/pnpm/pi" /usr/local/bin/pi /opt/homebrew/bin/pi "$HOME"/.nvm/versions/node/*/bin/pi; do
    if [ -x "$c" ]; then PI=$c; break; fi
  done
fi
[ -n "$PI" ] || {{ echo '{PI_MISSING_MESSAGE}' >&2; exit {PI_MISSING_EXIT}; }}
PATH="${{PI%/*}}:$PATH"; export PATH
"#
    )
}

/// The argv passed to `ssh` (everything after the binary name).
///
/// `-T` and `BatchMode=yes` make ssh non-interactive: a host that needs a
/// password fails fast with its own message instead of hanging the app. The
/// keepalive lets a dead link surface as an exited process.
pub fn ssh_args(target: &RemoteTarget, remote_script: &str, password: bool) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-o",
        // Without a password, never ask: keys and the agent only. With one,
        // ssh may ask once, through the askpass helper (see `apply_password`).
        if password {
            "BatchMode=no"
        } else {
            "BatchMode=yes"
        },
        "-o",
        "ConnectTimeout=15",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();
    if password {
        // One try: the helper answers every prompt with the same secret, so a
        // wrong password would otherwise be sent three times.
        args.push("-o".into());
        args.push("NumberOfPasswordPrompts=1".into());
    }
    if let Some(port) = target.port {
        args.push("-p".into());
        args.push(port.to_string());
    }
    args.push("--".into());
    args.push(target.host.trim().to_string());
    // ssh joins its remote arguments with spaces and the remote login shell
    // parses the result, so the dispatcher and the script each travel as one
    // quoted word (`orbit` is the dispatcher's `$0`, the script its `$1`).
    args.push(remote_command(remote_script));
    args
}

/// The one remote word ssh hands the host's login shell: the login dispatcher
/// with `script` as its `$1`.
fn remote_command(script: &str) -> String {
    format!(
        "sh -c {} orbit {}",
        shell_quote(LOGIN_DISPATCH),
        shell_quote(script)
    )
}

/// `pi /login <provider>` on the host, for a terminal the user is watching:
/// pi is found the way sessions find it, then replaces the shell so the login
/// flow owns the terminal. `provider` must be a plain id (it reaches a shell).
pub fn login_script(provider: Option<&str>) -> Result<String> {
    let arg = match provider {
        // No provider: pi's own `/login` picker, which offers every OAuth and
        // API-key provider.
        None => String::new(),
        Some(id) => {
            let plain = !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
            if !plain {
                bail!("provider id `{id}` is not a plain id");
            }
            format!(" {id}")
        }
    };
    Ok(format!(
        "set -eu\n{}exec \"$PI\" /login{arg}\n",
        locate_pi()
    ))
}

/// What [`list_models_script`] prints before anything else.
pub const LIST_MODELS_STARTED: &str = "orbit-models: started";

/// List the models the host's pi can use, the providers signed in there
/// included: `pi --list-models` reads the host's own credentials and prints
/// one row per usable model (provider, id, context, max output, thinking,
/// images) and never a credential. Read-only.
pub fn list_models_script() -> String {
    format!(
        "set -eu\nprintf '%s\\n' '{LIST_MODELS_STARTED}'\n{}PI_SKIP_VERSION_CHECK=1 \"$PI\" --list-models </dev/null\n",
        locate_pi()
    )
}

/// A whole command line for a local shell that runs `script` on the host in
/// an interactive terminal: `ssh -t [-p PORT] -- HOST '<remote command>'`,
/// every word quoted. `-t` gives the remote side a terminal, so pi's login
/// prompts, a password and a new host key are all asked in that window by
/// ssh and pi themselves.
pub fn interactive_ssh_command(target: &RemoteTarget, script: &str) -> Result<String> {
    target.validate()?;
    let mut words = vec!["ssh".to_string(), "-t".to_string()];
    if let Some(port) = target.port {
        words.push("-p".into());
        words.push(port.to_string());
    }
    words.push("--".into());
    words.push(target.host.trim().to_string());
    words.push(remote_command(script));
    Ok(words
        .iter()
        .map(|word| shell_quote(word))
        .collect::<Vec<_>>()
        .join(" "))
}

/// Build the POSIX script that installs the guard (and any other extensions),
/// pins the access mode and execs `pi --mode rpc`. Files are written with
/// quoted heredocs, so their contents are never interpreted by the shell.
///
/// Fails if a file's contents would end the heredoc early, or a name, a
/// directory or an environment variable is not plain.
pub fn launch_script(target: &RemoteTarget, launch: &RemoteLaunch<'_>) -> Result<String> {
    target.validate()?;
    let mut script = String::from("set -eu\n");
    script.push_str(&format!("G=\"$HOME/{REMOTE_GUARD_DIR}\"\n"));
    script.push_str("mkdir -p \"$G\" \"$HOME/.orbit-pi\"\n");
    write_files(&mut script, "$G", launch.guard)?;
    let mut extension_args = String::new();
    for extension in launch.extensions {
        if !is_plain_file_name(extension.dir) || extension.dir == "remote-guard" {
            bail!(
                "extension directory `{}` is not a plain name",
                extension.dir
            );
        }
        if !extension.files.iter().any(|file| file.name == "index.js") {
            bail!("extension `{}` has no index.js", extension.dir);
        }
        let dir = format!("$HOME/.orbit-pi/{}", extension.dir);
        script.push_str(&format!("mkdir -p \"{dir}\"\n"));
        write_files(&mut script, &dir, extension.files)?;
        extension_args.push_str(&format!(" --extension \"{dir}/index.js\""));
    }
    let mut env = String::new();
    for (name, value) in launch.env {
        if !is_env_name(name) {
            bail!("environment variable `{name}` is not a plain name");
        }
        env.push_str(&format!(" {name}={}", shell_quote(value)));
    }
    // Orbit owns `~/.orbit-pi` on the host. The guard reads this file fresh on
    // every tool call; pinning it keeps a remote session supervised no matter
    // what the host's file said before.
    script
        .push_str("printf '%s\\n' '{\"mode\":\"supervised\"}' > \"$HOME/.orbit-pi/access.json\"\n");
    script.push_str(&format!(
        "cd -- {dir} || exit 66\n",
        dir = shell_quote(&target.dir)
    ));
    script.push_str(&format!(
        "[ -f \"$G/index.js\" ] || {{ echo 'orbit: access guard missing on host; refusing to start pi' >&2; exit {GUARD_MISSING_EXIT}; }}\n"
    ));
    script.push_str(&locate_pi());
    script.push_str(&format!(
        "exec env PI_SKIP_VERSION_CHECK=1{env} \"$PI\" --mode rpc --approve --extension \"$G/index.js\"{extension_args}\n",
    ));
    Ok(script)
}

/// Append one quoted heredoc per file, writing it into the shell directory
/// expression `dir` (already quoted where it expands).
fn write_files(script: &mut String, dir: &str, files: &[RemoteFile<'_>]) -> Result<()> {
    let delimiter = "ORBIT_GUARD_EOF";
    for file in files {
        if !is_plain_file_name(file.name) {
            bail!("guard file name `{}` is not a plain name", file.name);
        }
        if file.contents.lines().any(|line| line == delimiter) {
            bail!(
                "guard file `{}` collides with the heredoc delimiter",
                file.name
            );
        }
        script.push_str(&format!(
            "cat > \"{dir}/{name}\" <<'{delimiter}'\n{contents}\n{delimiter}\n",
            name = file.name,
            contents = file.contents.trim_end_matches('\n'),
        ));
    }
    Ok(())
}

fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_uppercase() || c == '_')
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// A read-only check that a host can run a remote session: ssh connects, the
/// directory exists, and `pi` is on the login PATH. It writes nothing on the
/// host (the guard and mode file are installed only by [`launch_script`]) and
/// prints `pi --version` on success, so "Test connection" can show what
/// answered. The exit codes match the launch script's.
pub fn probe_script(target: &RemoteTarget) -> Result<String> {
    target.validate()?;
    let dir = shell_quote(&target.dir);
    let mut script = String::from("set -eu\n");
    // The first thing Orbit's own script does. Its absence on stdout means the
    // host's login profile (`sh -l` reads /etc/profile and ~/.profile) ended
    // the connection before the check ran.
    script.push_str(&format!("printf '%s\\n' '{PROBE_STARTED}'\n"));
    script.push_str(&format!(
        "cd -- {dir} 2>/dev/null || {{ echo 'orbit: directory not found on host' >&2; exit 66; }}\n"
    ));
    script.push_str(&locate_pi());
    // Started the way a session starts it: no stdin, no update check. A
    // failure names its status and pi's own last line instead of ending the
    // connection with a bare code.
    script.push_str(
        "pibin=$PI; rc=0; out=$(PI_SKIP_VERSION_CHECK=1 \"$PI\" --version </dev/null 2>&1) || rc=$?\n",
    );
    script.push_str(&format!(
        "if [ \"$rc\" -ne 0 ]; then printf 'orbit: pi --version (%s) failed with status %s: %s\\n' \"$pibin\" \"$rc\" \"$(printf '%s\\n' \"$out\" | tail -n 1)\" >&2; exit {PI_FAILED_EXIT}; fi\n"
    ));
    script.push_str("printf '%s\\n' \"$out\" | tail -n 1\n");
    Ok(script)
}

/// What [`probe_script`] prints before anything else.
pub const PROBE_STARTED: &str = "orbit-probe: started";

/// [`probe_script`]'s exit status when `pi --version` fails (`EX_SOFTWARE`).
pub const PI_FAILED_EXIT: i32 = 70;

/// The signal behind a shell exit status above 128 (a process killed by
/// signal N exits 128 + N), named the way `kill -l` names it.
pub fn signal_name(status: i32) -> Option<&'static str> {
    let name = match status.checked_sub(128)? {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        4 => "SIGILL",
        6 => "SIGABRT",
        8 => "SIGFPE",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        24 => "SIGXCPU",
        25 => "SIGXFSZ",
        _ => return None,
    };
    Some(name)
}

/// What [`install_script`] prints, on stdout, at each stage. The app shows
/// the latest one as progress.
pub const INSTALL_STAGE_PREFIX: &str = "orbit-install: ";

/// Install pi for the login account, without a terminal and without sudo.
///
/// pi's installer (`https://pi.dev/install.sh`) runs unattended when there is
/// no `/dev/tty`: it installs with its defaults and leaves shell profiles
/// alone. It only stops when Node.js 22.19+ and npm are missing, because it
/// offers to install Node only after asking. So this script does that step
/// itself, the way the installer would: the latest Node 22 from nodejs.org,
/// checked against the release's SHA-256 list, into `~/.local/share/pi-node`
/// (the folder pi's launcher puts on `PATH` for itself). Unlike the
/// installer, a missing checksum tool refuses the download instead of
/// skipping the check. Then it runs pi's installer with stdin closed and
/// confirms the result with the same lookup sessions use.
pub fn install_script() -> String {
    format!(
        r#"set -eu
say() {{ printf '{INSTALL_STAGE_PREFIX}%s\n' "$1"; }}
fail() {{ printf 'orbit: %s\n' "$1" >&2; exit 1; }}
say started
case "$(uname -s)" in
  Linux) plat=linux ;;
  Darwin) plat=darwin ;;
  *) fail "pi can be installed on Linux and macOS only" ;;
esac
command -v curl >/dev/null 2>&1 || fail "curl is required to install pi; install curl on the server first"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
node_home="${{XDG_DATA_HOME:-$HOME/.local/share}}/pi-node"
PATH="$node_home/current/bin:$PATH"; export PATH
node_ok() {{
  command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 &&
    node -e 'const [a,b]=process.versions.node.split(".").map(Number);process.exit(a>22||(a===22&&b>=19)?0:1)' >/dev/null 2>&1
}}
if ! node_ok; then
  say "downloading Node.js 22"
  case "$(uname -m)" in
    x86_64|amd64) arch=x64 ;;
    arm64|aarch64) arch=arm64 ;;
    *) fail "no Node.js build for CPU $(uname -m)" ;;
  esac
  command -v xz >/dev/null 2>&1 || fail "xz is required to unpack Node.js; install xz-utils on the server first"
  base=https://nodejs.org/dist/latest-v22.x
  curl -fsSL "$base/SHASUMS256.txt" -o "$tmp/SHASUMS256.txt" || fail "could not download the Node.js checksum list"
  name=$(grep -o "node-v[0-9.]*-$plat-$arch\.tar\.xz" "$tmp/SHASUMS256.txt" | head -n 1)
  [ -n "$name" ] || fail "no Node.js 22 build for $plat-$arch"
  curl -fsSL "$base/$name" -o "$tmp/$name" || fail "could not download $name"
  expected=$(grep " $name\$" "$tmp/SHASUMS256.txt" | awk '{{print $1}}')
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/$name" | awk '{{print $1}}')
  elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$tmp/$name" | awk '{{print $1}}')
  else
    fail "sha256sum or shasum is required to verify Node.js"
  fi
  [ -n "$expected" ] && [ "$actual" = "$expected" ] || fail "the Node.js download did not match its published checksum"
  say "unpacking Node.js"
  mkdir -p "$node_home"
  tar -xJf "$tmp/$name" -C "$node_home"
  ln -sfn "$node_home/${{name%.tar.xz}}" "$node_home/current"
  node_ok || fail "Node.js was unpacked but does not run on this server"
fi
say "installing pi"
curl -fsSL https://pi.dev/install.sh -o "$tmp/pi-install.sh" || fail "could not download pi's installer"
sh "$tmp/pi-install.sh" </dev/null >"$tmp/install.log" 2>&1 || {{
  tail -n 5 "$tmp/install.log" >&2
  fail "pi's installer failed"
}}
{locate}
say "installed"
PI_SKIP_VERSION_CHECK=1 "$PI" --version </dev/null 2>&1 | tail -n 1
"#,
        locate = locate_pi()
    )
}

/// The JavaScript [`set_api_key_script`] runs with the host's Node: read the
/// key from stdin, merge `{type: "api_key", key}` into `auth.json` under the
/// provider id, write a 0600 temp file and rename it into place. A file it
/// cannot parse is left alone (the user's other sign-ins live there).
/// No single quotes: it travels inside a single-quoted shell word.
const SET_API_KEY_JS: &str = r#"const fs = require("fs");
const [file, id] = process.argv.slice(1);
let key = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => { key += chunk; });
process.stdin.on("end", () => {
  key = key.trim();
  if (!key) { console.error("orbit: no API key was sent"); process.exit(65); }
  let root = {};
  if (fs.existsSync(file)) {
    const raw = fs.readFileSync(file, "utf8");
    if (raw.trim()) {
      try { root = JSON.parse(raw); } catch (error) {
        console.error("orbit: auth.json on the server is not valid JSON; fix it by hand first");
        process.exit(65);
      }
      if (root === null || typeof root !== "object" || Array.isArray(root)) {
        console.error("orbit: auth.json on the server is not a JSON object; fix it by hand first");
        process.exit(65);
      }
    }
  }
  root[id] = { type: "api_key", key };
  const temp = file + ".orbit-" + process.pid;
  fs.writeFileSync(temp, JSON.stringify(root, null, 2) + "\n", { mode: 0o600 });
  fs.renameSync(temp, file);
  fs.chmodSync(file, 0o600);
  console.log("orbit: saved");
});
"#;

/// What [`set_api_key_script`] prints before anything else.
pub const KEY_STARTED: &str = "orbit-key: started";

/// Save an API key for `provider` in the host's `~/.pi/agent/auth.json`, the
/// file pi's `/login` writes. The key is read from **stdin**: it never
/// appears in a command line (visible to other users in `ps`) or a file on
/// either machine besides `auth.json` itself, which stays 0600. Runs with the
/// host's Node — pi's own (`~/.local/share/pi-node`) or the system's.
pub fn set_api_key_script(provider: &str) -> Result<String> {
    let plain = !provider.is_empty()
        && provider
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if !plain {
        bail!("provider id `{provider}` is not a plain id");
    }
    Ok(format!(
        r#"set -eu
printf '%s\n' '{KEY_STARTED}'
umask 077
node_bin="${{XDG_DATA_HOME:-$HOME/.local/share}}/pi-node/current/bin"
if [ -x "$node_bin/node" ]; then PATH="$node_bin:$PATH"; export PATH; fi
command -v node >/dev/null 2>&1 || {{ echo 'orbit: Node.js is not available on this server; install pi first' >&2; exit 69; }}
agent="${{PI_CODING_AGENT_DIR:-$HOME/.pi/agent}}"
mkdir -p "$agent"
exec node -e {js} "$agent/auth.json" {provider}
"#,
        js = shell_quote(SET_API_KEY_JS),
        provider = shell_quote(provider)
    ))
}

/// The JavaScript [`set_workflow_mode_script`] runs with the host's Node:
/// merge `{<session id>: <mode>}` into `~/.orbit-pi/workflow.json`, the map
/// the workflow extension reads on every hook, through a temp file and a
/// rename. A file it cannot parse is replaced: Orbit owns it, and every
/// session's mode is also recorded in its own session file.
/// No single quotes: it travels inside a single-quoted shell word.
const SET_WORKFLOW_JS: &str = r#"const fs = require("fs");
const [file, id, mode] = process.argv.slice(1);
let root = {};
try {
  const parsed = JSON.parse(fs.readFileSync(file, "utf8"));
  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) root = parsed;
} catch (error) {}
root[id] = mode;
const temp = file + ".orbit-" + process.pid;
fs.writeFileSync(temp, JSON.stringify(root) + "\n");
fs.renameSync(temp, file);
console.log("orbit: saved");
"#;

/// What [`set_workflow_mode_script`] prints before anything else.
pub const WORKFLOW_STARTED: &str = "orbit-workflow: started";

/// Record `mode` (a workflow wire id: `plan`, `build`, `ask`) for the pi
/// session `session_id` in the host's `~/.orbit-pi/workflow.json`, so the
/// workflow extension a remote session loads applies it from its next hook.
/// Runs with the Node that runs the host's pi.
pub fn set_workflow_mode_script(session_id: &str, mode: &str) -> Result<String> {
    let plain = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if !plain(session_id) {
        bail!("session id `{session_id}` is not a plain id");
    }
    if !plain(mode) {
        bail!("workflow mode `{mode}` is not a plain id");
    }
    Ok(format!(
        r#"set -eu
printf '%s\n' '{WORKFLOW_STARTED}'
{node}mkdir -p "$HOME/.orbit-pi"
exec node -e {js} "$HOME/.orbit-pi/workflow.json" {id} {mode}
"#,
        node = node_on_path(),
        js = shell_quote(SET_WORKFLOW_JS),
        id = shell_quote(session_id),
        mode = shell_quote(mode),
    ))
}

/// Put the Node that runs the host's pi on `PATH`: pi's own directory (an
/// npm or nvm install keeps `node` beside it), then the Node.js Orbit's
/// installer unpacks. Exits 69 when there is none.
fn node_on_path() -> String {
    format!(
        r#"{locate}node_bin="${{XDG_DATA_HOME:-$HOME/.local/share}}/pi-node/current/bin"
if [ -x "$node_bin/node" ]; then PATH="$node_bin:$PATH"; export PATH; fi
command -v node >/dev/null 2>&1 || {{ echo 'orbit: Node.js is not available on this server' >&2; exit 69; }}
"#,
        locate = locate_pi()
    )
}

/// What [`clone_script`] prints first; every later line starts with
/// [`CLONE_PREFIX`].
pub const CLONE_STARTED: &str = "orbit-clone: started";
/// The prefix of every [`clone_script`] result line: `done <path>`,
/// `existing <path>`, `failed <kind>` (`git-missing`, `not-empty`, `parent`,
/// `hostkey`, `publickey`, `login`, `not-found`, `other`) and `detail <line>`.
pub const CLONE_PREFIX: &str = "orbit-clone: ";

/// Whether `dest` names a folder the clone may create: `~/…` or absolute,
/// one line, and no `..` segment.
pub fn is_clone_destination(dest: &str) -> bool {
    let rest = match dest.strip_prefix("~/") {
        Some(rest) => rest,
        None if dest.starts_with('/') => dest,
        None => return false,
    };
    !rest.trim_matches('/').is_empty()
        && !dest.chars().any(|c| c == '\n' || c == '\r' || c == '\0')
        && !dest.split('/').any(|segment| segment == "..")
}

/// Clone `url` into `dest` on the host with the host's own `git` and its own
/// credentials. Git's errors stay on **stdout** as `detail` lines and the
/// script exits 0 for them: the ssh runner reads stderr for the *server's*
/// login problems, and a clone's "Permission denied (publickey)" is about the
/// git host, not this one. A checkout of the same `url` already at `dest` is
/// reused, so a retry after a failed file copy does not clone twice.
pub fn clone_script(url: &str, dest: &str) -> Result<String> {
    let url = url.trim();
    if url.is_empty()
        || url.starts_with('-')
        || url.len() > 2048
        || url.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!("`{url}` is not a repository URL");
    }
    if !is_clone_destination(dest) {
        bail!("`{dest}` is not a folder under ~ or an absolute path");
    }
    let dest = match dest.strip_prefix("~/") {
        Some(rest) => format!("\"$HOME\"/{}", shell_quote(rest.trim_end_matches('/'))),
        None => shell_quote(dest.trim_end_matches('/')),
    };
    Ok(format!(
        r#"set -u
printf '%s\n' '{CLONE_STARTED}'
url={url}
dest={dest}
say() {{ printf '{CLONE_PREFIX}%s\n' "$1"; }}
fail() {{ say "failed $1"; exit 0; }}
command -v git >/dev/null 2>&1 || fail git-missing
if [ -d "$dest/.git" ] && [ "$(git -C "$dest" remote get-url origin 2>/dev/null)" = "$url" ]; then
  cd "$dest" && say "existing $(pwd -P)"
  exit 0
fi
if [ -e "$dest" ] && [ -n "$(ls -A "$dest" 2>/dev/null)" ]; then fail not-empty; fi
mkdir -p "$(dirname "$dest")" 2>/dev/null || fail parent
if err=$(GIT_TERMINAL_PROMPT=0 git clone --quiet -- "$url" "$dest" 2>&1 </dev/null); then
  cd "$dest" && say "done $(pwd -P)"
  exit 0
fi
case "$err" in
  *"Host key verification failed"*) kind=hostkey ;;
  *"Permission denied (publickey"*) kind=publickey ;;
  *"could not read Username"*|*"could not read Password"*|*"terminal prompts disabled"*|*"Authentication failed"*) kind=login ;;
  *"not found"*|*"does not exist"*) kind=not-found ;;
  *) kind=other ;;
esac
printf '%s\n' "$err" | tail -n 3 | while IFS= read -r line; do say "detail $line"; done
fail "$kind"
"#,
        url = shell_quote(url),
    ))
}

/// The JavaScript [`copy_files_script`] runs with the host's Node: read a
/// JSON list of `{path, data}` (base64) from stdin and write each file under
/// the project, 0600, through a temp file and a rename. A path that is
/// absolute, has a `..` segment, or reaches outside the project through a
/// symlinked folder is refused; the rename replaces a symlink at the file's
/// own name instead of writing through it.
/// No single quotes: it travels inside a single-quoted shell word.
const COPY_FILES_JS: &str = r#"const fs = require("fs");
const path = require("path");
const root = fs.realpathSync(process.argv[1]);
const inside = (dir) => dir === root || dir.startsWith(root + path.sep);
let input = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => { input += chunk; });
process.stdin.on("end", () => {
  let files;
  try { files = JSON.parse(input); } catch (error) {
    console.log("orbit-copy: failed \tthe file list did not arrive intact");
    return;
  }
  for (const file of files) {
    const rel = String((file && file.path) || "");
    try {
      if (!rel || path.isAbsolute(rel) || rel.includes("\0") || rel.split(/[\\/]/).includes("..")) {
        throw new Error("not a path inside the project");
      }
      const dir = path.dirname(path.join(root, rel));
      let existing = dir;
      while (!fs.existsSync(existing)) existing = path.dirname(existing);
      if (!inside(fs.realpathSync(existing))) throw new Error("its folder leads outside the project");
      fs.mkdirSync(dir, { recursive: true });
      const parent = fs.realpathSync(dir);
      if (!inside(parent)) throw new Error("its folder leads outside the project");
      const name = path.basename(rel);
      const temp = path.join(parent, ".orbit-copy-" + process.pid + "-" + name);
      fs.writeFileSync(temp, Buffer.from(String(file.data || ""), "base64"), { mode: 0o600 });
      fs.renameSync(temp, path.join(parent, name));
      console.log("orbit-copy: wrote " + rel);
    } catch (error) {
      console.log("orbit-copy: failed " + rel + "\t" + ((error && error.message) || String(error)));
    }
  }
});
"#;

/// What [`copy_files_script`] prints first.
pub const COPY_STARTED: &str = "orbit-copy: started";

/// Write files sent on **stdin** (see [`COPY_FILES_JS`]) into the project at
/// `dest`, an absolute path on the host (what [`clone_script`] reported).
/// The contents never appear on a command line. Prints `orbit-copy: wrote
/// <path>` or `orbit-copy: failed <path>\t<reason>` per file.
pub fn copy_files_script(dest: &str) -> Result<String> {
    if !dest.starts_with('/') || dest.chars().any(|c| c == '\n' || c == '\r' || c == '\0') {
        bail!("`{dest}` is not an absolute path");
    }
    Ok(format!(
        "set -eu\nprintf '%s\\n' '{COPY_STARTED}'\n{node}exec node -e {js} {dest}\n",
        node = node_on_path(),
        js = shell_quote(COPY_FILES_JS),
        dest = shell_quote(dest),
    ))
}

/// The JavaScript [`list_sessions_script`] runs with the host's Node: walk
/// pi's session store (newest files first, at most `limit`), and print one
/// JSON line per session with only what a sidebar row shows — the header's
/// id and cwd, pi's latest `session_info` name, the first user message's
/// text, and the newest user message's timestamp (the file's mtime when
/// none is stamped). Drafts with no user message are skipped, like the local
/// list does. Huge files are read head and tail only.
/// No single quotes: it travels inside a single-quoted shell word.
const LIST_SESSIONS_JS: &str = r#"const fs = require("fs");
const path = require("path");
const root = process.argv[1];
const limit = Number(process.argv[2]) || 300;
const HUGE = 16 * 1024 * 1024;
const files = [];
let groups = [];
try { groups = fs.readdirSync(root, { withFileTypes: true }); } catch (error) { groups = []; }
for (const group of groups) {
  if (!group.isDirectory()) continue;
  const dir = path.join(root, group.name);
  let names = [];
  try { names = fs.readdirSync(dir); } catch (error) { names = []; }
  for (const name of names) {
    if (!name.endsWith(".jsonl")) continue;
    const file = path.join(dir, name);
    try {
      const stat = fs.statSync(file);
      if (stat.isFile()) files.push({ file, mtime: stat.mtimeMs, size: stat.size });
    } catch (error) {}
  }
}
files.sort((a, b) => b.mtime - a.mtime);
const read = (entry) => {
  if (entry.size <= HUGE) return fs.readFileSync(entry.file, "utf8");
  const fd = fs.openSync(entry.file, "r");
  try {
    const head = Buffer.alloc(2 * 1024 * 1024);
    const tail = Buffer.alloc(4 * 1024 * 1024);
    fs.readSync(fd, head, 0, head.length, 0);
    fs.readSync(fd, tail, 0, tail.length, entry.size - tail.length);
    return head.toString("utf8") + "\n" + tail.toString("utf8");
  } finally { fs.closeSync(fd); }
};
const text = (message) => {
  const content = message.content;
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    const block = content.find((b) => b && b.type === "text" && typeof b.text === "string");
    return block ? block.text : "";
  }
  return "";
};
for (const entry of files.slice(0, limit)) {
  try {
    const lines = read(entry).split("\n");
    const header = JSON.parse(lines[0]);
    if (!header || header.type !== "session" || typeof header.id !== "string" || typeof header.cwd !== "string") continue;
    let first = null;
    let name = null;
    let time = null;
    for (let i = 1; i < lines.length; i += 1) {
      const line = lines[i];
      const info = line.includes("session_info");
      if (!info && !(line.includes("message") && line.includes("user"))) continue;
      let value;
      try { value = JSON.parse(line); } catch (error) { continue; }
      if (value.type === "session_info") {
        name = typeof value.name === "string" && value.name.trim() ? value.name.trim() : null;
      } else if (value.type === "message" && value.message && value.message.role === "user") {
        if (first === null) first = text(value.message);
        const stamp = value.timestamp !== undefined ? value.timestamp : value.message.timestamp;
        if (stamp !== undefined) time = stamp;
      }
    }
    if (!first || !first.trim()) continue;
    console.log(JSON.stringify({
      path: entry.file, id: header.id, cwd: header.cwd, name,
      first: first.slice(0, 16000), time: time === null ? Math.round(entry.mtime) : time,
    }));
  } catch (error) {}
}
"#;

/// What [`list_sessions_script`] prints first; one JSON object per line
/// follows (see [`LIST_SESSIONS_JS`]).
pub const LIST_SESSIONS_STARTED: &str = "orbit-sessions: started";

/// List the newest `limit` sessions in the host's pi store
/// (`$PI_CODING_AGENT_DIR/sessions`, default `~/.pi/agent/sessions`).
/// Read-only. Runs with the Node that runs the host's pi.
pub fn list_sessions_script(limit: usize) -> String {
    format!(
        "set -eu\nprintf '%s\\n' '{LIST_SESSIONS_STARTED}'\n{node}exec node -e {js} \"${{PI_CODING_AGENT_DIR:-$HOME/.pi/agent}}/sessions\" {limit}\n",
        node = node_on_path(),
        js = shell_quote(LIST_SESSIONS_JS),
        limit = limit.max(1),
    )
}

/// What [`git_status_script`] prints first; `key: value` lines follow.
pub const GIT_STATUS_STARTED: &str = "orbit-git: started";

/// Read-only Git facts about the host folder `target.dir`: whether `git` is
/// installed, whether the folder is in a work tree, and if so its top level,
/// branch, upstream, ahead/behind counts, number of changed paths, last
/// commit and `origin` URL. One `key: value` line each (see the parser in
/// `orbit-pi`'s `remote_hosts`). Writes nothing: every command is a read, and
/// `GIT_OPTIONAL_LOCKS=0` keeps `git status` from refreshing the index.
pub fn git_status_script(target: &RemoteTarget) -> Result<String> {
    target.validate()?;
    let dir = shell_quote(&target.dir);
    let mut script = String::from("set -u\n");
    script.push_str(&format!("printf '%s\\n' '{GIT_STATUS_STARTED}'\n"));
    script.push_str(&format!(
        "cd -- {dir} 2>/dev/null || {{ echo 'orbit: directory not found on host' >&2; exit 66; }}\n"
    ));
    script.push_str("export GIT_OPTIONAL_LOCKS=0\n");
    script.push_str("command -v git >/dev/null 2>&1 || { echo 'git: missing'; exit 0; }\n");
    script.push_str(
        "[ \"$(git rev-parse --is-inside-work-tree 2>/dev/null)\" = true ] || { echo 'repo: no'; exit 0; }\n",
    );
    script.push_str("echo 'repo: yes'\n");
    for (key, command) in [
        ("root", "git rev-parse --show-toplevel"),
        ("branch", "git branch --show-current"),
        ("head", "git log -1 --format='%h %s'"),
        (
            "upstream",
            "git rev-parse --abbrev-ref --symbolic-full-name '@{u}'",
        ),
        (
            "ahead_behind",
            "git rev-list --left-right --count 'HEAD...@{u}'",
        ),
        ("changed", "git status --porcelain | wc -l"),
        ("origin", "git remote get-url origin"),
    ] {
        script.push_str(&format!(
            "printf '%s: %s\\n' {key} \"$({command} 2>/dev/null | head -n 1)\"\n"
        ));
    }
    Ok(script)
}

/// What [`list_branches_script`] prints first.
pub const BRANCHES_STARTED: &str = "orbit-branches: started";

/// The host folder's local branches, newest commit first: one
/// `name<TAB>committer-date` line each, the same `for-each-ref` this Mac's
/// branch picker reads. Prints nothing more when git is missing or the folder
/// is not a repository. Read-only.
pub fn list_branches_script(target: &RemoteTarget) -> Result<String> {
    target.validate()?;
    let dir = shell_quote(&target.dir);
    Ok(format!(
        r#"set -u
printf '%s\n' '{BRANCHES_STARTED}'
cd -- {dir} 2>/dev/null || {{ echo 'orbit: directory not found on host' >&2; exit 66; }}
export GIT_OPTIONAL_LOCKS=0
command -v git >/dev/null 2>&1 || exit 0
git for-each-ref --sort=-committerdate refs/heads/ --format='%(refname:short)%09%(committerdate:iso-strict)' 2>/dev/null
exit 0
"#
    ))
}

/// What [`switch_branch_script`] prints first; it ends with
/// `orbit-branch: ok` or `orbit-branch: failed`, after git's last lines as
/// `orbit-branch: detail …`.
pub const SWITCH_BRANCH_STARTED: &str = "orbit-branch: started";

/// Whether `name` is a branch name the scripts may pass to git: git's own
/// ref rules (no whitespace or control characters, none of `~^:?*[\`, no
/// `..`, `@{`, `//`, leading `-` or `/`, trailing `/`, `.` or `.lock`, no
/// component starting with `.`). The host's git checks again.
pub fn is_branch_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "@"
        && !name.starts_with(['-', '/'])
        && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock")
        && !name.contains("..")
        && !name.contains("@{")
        && !name.contains("//")
        && !name.split('/').any(|part| part.starts_with('.'))
        && !name.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
}

/// Switch the host folder to branch `name`, or with `create` make it from
/// HEAD and switch to it (uncommitted changes come along, as `git switch`
/// carries them). `git switch` first; the fallback is `git checkout NAME --`,
/// whose `--` makes git read `NAME` as a branch only — never as a file to
/// restore over the user's changes. Git's errors stay on stdout and the
/// script exits 0 for them, like [`clone_script`], so the ssh runner never
/// reads them as the server's own login problems.
pub fn switch_branch_script(target: &RemoteTarget, name: &str, create: bool) -> Result<String> {
    target.validate()?;
    if !is_branch_name(name) {
        bail!("`{name}` is not a valid branch name");
    }
    let dir = shell_quote(&target.dir);
    let name = shell_quote(name);
    let (switch, checkout) = if create {
        (
            format!("git switch -c {name}"),
            format!("git checkout -b {name}"),
        )
    } else {
        (
            format!("git switch {name}"),
            format!("git checkout {name} --"),
        )
    };
    Ok(format!(
        r#"set -u
printf '%s\n' '{SWITCH_BRANCH_STARTED}'
cd -- {dir} 2>/dev/null || {{ echo 'orbit: directory not found on host' >&2; exit 66; }}
export GIT_TERMINAL_PROMPT=0
command -v git >/dev/null 2>&1 || {{ echo 'orbit-branch: detail git is not installed on this server'; echo 'orbit-branch: failed'; exit 0; }}
if out=$({switch} 2>&1 </dev/null); then echo 'orbit-branch: ok'; exit 0; fi
if out2=$({checkout} 2>&1 </dev/null); then echo 'orbit-branch: ok'; exit 0; fi
printf '%s\n%s\n' "$out" "$out2" | grep -v '^$' | tail -n 3 | while IFS= read -r line; do printf 'orbit-branch: detail %s\n' "$line"; done
echo 'orbit-branch: failed'
"#
    ))
}

fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
        && !name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(dir: &str) -> RemoteTarget {
        RemoteTarget {
            host: "vps".into(),
            port: None,
            dir: dir.into(),
        }
    }

    #[test]
    fn git_status_script_reads_only_in_the_host_folder() {
        let script = git_status_script(&target("/srv/it's")).expect("script");
        assert!(script.contains(GIT_STATUS_STARTED));
        assert!(script.contains("cd -- '/srv/it'\\''s'"), "{script}");
        assert!(script.contains("GIT_OPTIONAL_LOCKS=0"));
        for key in [
            "root",
            "branch",
            "head",
            "upstream",
            "ahead_behind",
            "changed",
            "origin",
        ] {
            assert!(script.contains(&format!("'%s: %s\\n' {key} ")), "{key}");
        }
        for write in [
            "git init",
            "git add",
            "git commit",
            "git fetch",
            "git checkout",
        ] {
            assert!(!script.contains(write), "read-only: {write}");
        }
        assert!(git_status_script(&target("relative")).is_err());
    }

    #[test]
    fn quotes_embedded_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(shell_quote("plain"), "'plain'");
        assert_eq!(shell_quote("$(rm -rf x)"), "'$(rm -rf x)'");
    }

    #[test]
    fn validate_rejects_option_like_and_relative_targets() {
        assert!(target("/srv/app").validate().is_ok());
        assert!(RemoteTarget {
            host: "-oProxyCommand=x".into(),
            ..target("/a")
        }
        .validate()
        .is_err());
        assert!(RemoteTarget {
            host: "a b".into(),
            ..target("/a")
        }
        .validate()
        .is_err());
        assert!(RemoteTarget {
            host: "".into(),
            ..target("/a")
        }
        .validate()
        .is_err());
        assert!(target("srv/app").validate().is_err());
        assert!(target("/srv/a\nb").validate().is_err());
        assert!(RemoteTarget {
            port: Some(0),
            ..target("/a")
        }
        .validate()
        .is_err());
    }

    #[test]
    fn ssh_args_end_options_before_host_and_quote_script() {
        let args = ssh_args(
            &RemoteTarget {
                port: Some(2222),
                ..target("/srv")
            },
            "echo 'hi'",
            false,
        );
        let dash = args.iter().position(|a| a == "--").expect("-- present");
        assert_eq!(args[dash + 1], "vps");
        assert_eq!(
            args[dash + 2],
            format!(
                "sh -c {} orbit 'echo '\\''hi'\\'''",
                shell_quote(LOGIN_DISPATCH)
            )
        );
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(!args
            .iter()
            .any(|a| a.starts_with("NumberOfPasswordPrompts")));
        assert_eq!(args[0], "-T");

        let with_password = ssh_args(&target("/srv"), "true", true);
        assert!(with_password.contains(&"BatchMode=no".to_string()));
        assert!(with_password.contains(&"NumberOfPasswordPrompts=1".to_string()));
    }

    #[test]
    fn stderr_is_read_for_auth_and_host_key_refusals() {
        let denied = "user@vps: Permission denied (publickey,password).";
        assert!(is_auth_failure(denied));
        assert!(offers_password(denied));
        let keys_only = "user@vps: Permission denied (publickey).";
        assert!(is_auth_failure(keys_only));
        assert!(!offers_password(keys_only));
        assert!(!is_auth_failure(
            "ssh: connect to host vps port 22: Connection refused"
        ));
        assert!(is_auth_failure(
            "Received disconnect: Too many authentication failures"
        ));

        assert_eq!(
            host_key_problem("Host key verification failed."),
            Some(HostKeyProblem::Unknown)
        );
        assert_eq!(
            host_key_problem(
                "@@@ WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED! @@@\nHost key verification failed."
            ),
            Some(HostKeyProblem::Changed)
        );
        assert_eq!(host_key_problem("Connection timed out"), None);
    }

    #[test]
    fn launch_script_refuses_to_start_without_guard_and_pins_mode() {
        let script = launch_script(
            &target("/srv/app"),
            &RemoteLaunch {
                guard: &[RemoteFile {
                    name: "index.js",
                    contents: "export default 1;",
                }],
                ..RemoteLaunch::default()
            },
        )
        .expect("script");
        let guard_check = script.find("access guard missing").expect("guard check");
        let exec = script.find("exec env").expect("exec");
        assert!(guard_check < exec, "guard check must precede exec");
        assert!(script.contains("'{\"mode\":\"supervised\"}' > \"$HOME/.orbit-pi/access.json\""));
        assert!(script.contains("cd -- '/srv/app' || exit 66"));
        assert!(script.contains("<<'ORBIT_GUARD_EOF'\nexport default 1;\nORBIT_GUARD_EOF"));
    }

    #[test]
    fn probe_script_is_read_only_and_reports_pi() {
        let script = probe_script(&target("/srv/it's")).expect("script");
        assert!(script.contains("cd -- '/srv/it'\\''s'"));
        assert!(script.contains("command -v pi"));
        let started = script.find(PROBE_STARTED).expect("start marker");
        assert!(
            started < script.find("cd --").expect("cd"),
            "the marker comes first"
        );
        assert!(script.contains("PI_SKIP_VERSION_CHECK=1 \"$PI\" --version </dev/null"));
        assert!(
            script.contains("$HOME/.pi/agent/bin/pi"),
            "looks where the installer puts pi"
        );
        assert!(script.contains(&format!("exit {PI_FAILED_EXIT}")));
        for write in ["mkdir", "cat >", "> \"$HOME", "exec "] {
            assert!(!script.contains(write), "probe must not write: {write}");
        }
        assert!(probe_script(&target("relative")).is_err());
    }

    #[test]
    fn signal_exits_are_named() {
        assert_eq!(signal_name(142), Some("SIGALRM"));
        assert_eq!(signal_name(143), Some("SIGTERM"));
        assert_eq!(signal_name(137), Some("SIGKILL"));
        assert_eq!(signal_name(127), None);
        assert_eq!(signal_name(255), None);
        assert_eq!(signal_name(0), None);
    }

    /// Run the probe script with a fake `pi` and check each outcome.
    #[cfg(unix)]
    #[test]
    fn the_probe_script_reports_each_step() {
        let root = std::env::temp_dir().join(format!("orbit-probe-{}", std::process::id()));
        let bin = root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let run = |pi_body: Option<&str>| {
            let pi = bin.join("pi");
            let _ = std::fs::remove_file(&pi);
            if let Some(body) = pi_body {
                std::fs::write(&pi, format!("#!/bin/sh\n{body}\n")).unwrap();
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&pi, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            let script = probe_script(&target(root.to_str().unwrap())).unwrap();
            std::process::Command::new("sh")
                .arg("-c")
                .arg(&script)
                .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                // The fallback locations are under $HOME: keep them in the
                // sandbox, away from this machine's own pi.
                .env("HOME", &root)
                .output()
                .unwrap()
        };

        let ok = run(Some("echo 'pi 1.5.2'"));
        assert!(ok.status.success());
        let stdout = String::from_utf8_lossy(&ok.stdout);
        assert!(stdout.starts_with(PROBE_STARTED));
        assert_eq!(stdout.lines().last(), Some("pi 1.5.2"));

        // A pi killed by SIGALRM: reported with its status, not a bare exit.
        let alarm = run(Some("echo 'about to stop' >&2; kill -ALRM $$"));
        assert_eq!(alarm.status.code(), Some(PI_FAILED_EXIT));
        let stderr = String::from_utf8_lossy(&alarm.stderr);
        assert!(
            stderr.contains("failed with status 142")
                && stderr.contains(&format!("({}/pi)", bin.display())),
            "{stderr}"
        );

        // The fallbacks include system-wide places a sandboxed $HOME cannot
        // hide; only assert "missing" on a machine without a pi there.
        let system_pi = ["/usr/local/bin/pi", "/opt/homebrew/bin/pi"]
            .iter()
            .any(|path| std::path::Path::new(path).exists());
        if !system_pi {
            let missing = run(None);
            assert_eq!(missing.status.code(), Some(PI_MISSING_EXIT));
            assert!(String::from_utf8_lossy(&missing.stderr).contains(PI_MISSING_MESSAGE));
        }

        // Installed by pi's installer but not on the login PATH (Ubuntu's
        // ~/.bashrc stops early outside a terminal): still found.
        let managed = root.join(".pi/agent/bin");
        std::fs::create_dir_all(&managed).unwrap();
        std::fs::write(managed.join("pi"), "#!/bin/sh\necho 'pi 1.5.2'\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(managed.join("pi"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let found = run(None);
        assert!(
            found.status.success(),
            "{}",
            String::from_utf8_lossy(&found.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&found.stdout).lines().last(),
            Some("pi 1.5.2")
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The dispatcher hands the script to `sh` under the user's own login
    /// shell, and falls back to `sh -l` for any other shell.
    #[cfg(unix)]
    #[test]
    fn the_login_dispatch_runs_the_script_under_each_shell() {
        let home = std::env::temp_dir().join(format!("orbit-dispatch-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        for shell in ["/bin/sh", "/bin/bash", "/bin/zsh", "/usr/bin/fish"] {
            if !std::path::Path::new(shell).exists() {
                continue;
            }
            let output = std::process::Command::new("sh")
                .arg("-c")
                .arg(LOGIN_DISPATCH)
                .arg("orbit")
                .arg("printf 'ran:%s\\n' \"$0\"")
                .env("SHELL", shell)
                .env("HOME", &home)
                .output()
                .unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(
                output.status.success() && stdout.lines().any(|l| l.starts_with("ran:")),
                "{shell}: {stdout} {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn the_install_script_verifies_node_and_never_asks() {
        let script = install_script();
        // pi's official installer, from a file, with no stdin: its unattended route.
        assert!(script.contains("https://pi.dev/install.sh"));
        assert!(script.contains("sh \"$tmp/pi-install.sh\" </dev/null"));
        // Node is checked against the published list, and refused unverified.
        assert!(script.contains("SHASUMS256.txt"));
        assert!(script.contains("did not match its published checksum"));
        assert!(script.contains("sha256sum or shasum is required"));
        // The folder pi's launcher puts on PATH for itself.
        assert!(script.contains("pi-node"));
        for forbidden in ["sudo", "/dev/tty", ">> \"$HOME/.bashrc", ".profile"] {
            assert!(
                !script.contains(forbidden),
                "install must not use {forbidden}"
            );
        }
        assert!(
            script.contains("$HOME/.pi/agent/bin/pi"),
            "confirms with the session lookup"
        );
    }

    /// The generated script is valid POSIX sh.
    #[cfg(unix)]
    #[test]
    fn the_install_script_parses() {
        let output = std::process::Command::new("sh")
            .arg("-n")
            .arg("-c")
            .arg(install_script())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn the_login_runs_on_the_host_in_a_terminal() {
        let script = login_script(Some("anthropic")).expect("plain id");
        assert!(
            script.contains("$HOME/.pi/agent/bin/pi"),
            "finds pi like a session"
        );
        assert!(script.trim_end().ends_with("exec \"$PI\" /login anthropic"));
        assert!(login_script(Some("x; rm -rf ~")).is_err());
        assert!(login_script(Some("")).is_err());
        assert!(login_script(None)
            .unwrap()
            .trim_end()
            .ends_with("exec \"$PI\" /login"));
        let listing = list_models_script();
        assert!(listing.contains("--list-models </dev/null"));
        assert!(listing.find(LIST_MODELS_STARTED) < listing.find("command -v pi"));

        let command = interactive_ssh_command(
            &RemoteTarget {
                host: "ubuntu@vps".into(),
                port: Some(2222),
                dir: "/srv".into(),
            },
            &script,
        )
        .expect("command");
        assert!(
            command.starts_with("'ssh' '-t' '-p' '2222' '--' 'ubuntu@vps' "),
            "{command}"
        );
        assert!(
            !command.contains("BatchMode"),
            "a person is at the terminal"
        );
    }

    /// The local shell's view of the command parses back to the remote word
    /// ssh sends, and that word runs the login script under the dispatcher.
    #[cfg(unix)]
    #[test]
    fn the_terminal_command_round_trips_through_two_shells() {
        let script = "printf 'login-ran:%s\\n' \"$0\"";
        let command = interactive_ssh_command(
            &RemoteTarget {
                host: "vps".into(),
                port: None,
                dir: "/srv".into(),
            },
            script,
        )
        .unwrap();
        // Replace `ssh` with a stand-in that runs the remote word locally, the
        // way sshd hands it to the login shell.
        let local = command.replacen("'ssh' '-t' '--' 'vps' ", "sh -c ", 1);
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(&local)
            .env("SHELL", "/bin/sh")
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("login-ran:"),
            "{stdout} {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn the_key_script_takes_the_key_on_stdin_only() {
        let script = set_api_key_script("openrouter").expect("plain id");
        assert!(script.contains("exec node -e "));
        assert!(script.contains("umask 077"));
        assert!(
            !SET_API_KEY_JS.contains('\''),
            "the JS travels in single quotes"
        );
        assert!(set_api_key_script("x; rm -rf ~").is_err());
        assert!(set_api_key_script("").is_err());
    }

    /// Run the key script against a sandboxed home with this machine's Node:
    /// it merges, keeps other sign-ins, writes 0600, and refuses a file it
    /// cannot parse.
    #[cfg(unix)]
    #[test]
    fn the_key_script_merges_into_auth_json() {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no Node on this machine: covered by the shape test above
        }
        let home = std::env::temp_dir().join(format!("orbit-key-{}", std::process::id()));
        let agent = home.join(".pi/agent");
        std::fs::create_dir_all(&agent).unwrap();
        let auth = agent.join("auth.json");
        std::fs::write(&auth, r#"{"anthropic":{"type":"oauth","refresh":"r"}}"#).unwrap();
        let run = |key: &str| {
            let mut child = std::process::Command::new("sh")
                .arg("-c")
                .arg(set_api_key_script("openrouter").unwrap())
                .env("HOME", &home)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(key.as_bytes())
                .unwrap();
            child.wait_with_output().unwrap()
        };
        let saved = run("sk-or-test\n");
        assert!(
            saved.status.success(),
            "{}",
            String::from_utf8_lossy(&saved.stderr)
        );
        let text = std::fs::read_to_string(&auth).unwrap();
        let root: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(root["openrouter"]["key"], "sk-or-test");
        assert_eq!(root["openrouter"]["type"], "api_key");
        assert_eq!(root["anthropic"]["type"], "oauth", "other sign-ins survive");
        let mode = std::fs::metadata(&auth).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);

        std::fs::write(&auth, "{not json").unwrap();
        let refused = run("sk-or-test\n");
        assert!(!refused.status.success());
        assert_eq!(
            std::fs::read_to_string(&auth).unwrap(),
            "{not json",
            "never overwritten"
        );
        assert!(!run("\n").status.success(), "an empty key is refused");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn launch_script_rejects_unsafe_guard_files() {
        let guard = |file: RemoteFile<'static>| {
            launch_script(
                &target("/a"),
                &RemoteLaunch {
                    guard: &[file],
                    ..RemoteLaunch::default()
                },
            )
        };
        assert!(guard(RemoteFile {
            name: "../x.js",
            contents: ""
        })
        .is_err());
        assert!(guard(RemoteFile {
            name: "x.js",
            contents: "a\nORBIT_GUARD_EOF\nb"
        })
        .is_err());
    }

    const ENTRY: [RemoteFile<'static>; 1] = [RemoteFile {
        name: "index.js",
        contents: "export default 1;",
    }];

    #[test]
    fn launch_script_installs_and_loads_extra_extensions_with_env() {
        let script = launch_script(
            &target("/srv/app"),
            &RemoteLaunch {
                guard: &ENTRY,
                extensions: &[RemoteExtension {
                    dir: "remote-workflow",
                    files: &ENTRY,
                }],
                env: &[("ORBIT_WORKFLOW_MODE", "plan's")],
            },
        )
        .expect("script");
        assert!(script.contains("mkdir -p \"$HOME/.orbit-pi/remote-workflow\""));
        assert!(script
            .contains("cat > \"$HOME/.orbit-pi/remote-workflow/index.js\" <<'ORBIT_GUARD_EOF'"));
        let exec = script.lines().last().expect("exec line");
        assert!(exec.starts_with(
            "exec env PI_SKIP_VERSION_CHECK=1 ORBIT_WORKFLOW_MODE='plan'\\''s' \"$PI\""
        ));
        assert!(exec.ends_with(
            "--extension \"$G/index.js\" --extension \"$HOME/.orbit-pi/remote-workflow/index.js\""
        ));
    }

    #[test]
    fn launch_script_rejects_unsafe_extensions_and_env() {
        let launch = |extensions: &[RemoteExtension<'_>], env: &[(&str, &str)]| {
            launch_script(
                &target("/a"),
                &RemoteLaunch {
                    guard: &ENTRY,
                    extensions,
                    env,
                },
            )
        };
        let ext = |dir| RemoteExtension { dir, files: &ENTRY };
        assert!(launch(&[ext("../x")], &[]).is_err());
        assert!(
            launch(&[ext("remote-guard")], &[]).is_err(),
            "never over the guard"
        );
        assert!(
            launch(
                &[RemoteExtension {
                    dir: "remote-x",
                    files: &[RemoteFile {
                        name: "other.js",
                        contents: ""
                    }]
                }],
                &[]
            )
            .is_err(),
            "an entry point is required"
        );
        assert!(launch(&[], &[("PATH=x; rm", "v")]).is_err());
        assert!(launch(&[], &[("lower", "v")]).is_err());
        assert!(launch(&[ext("remote-x")], &[("ORBIT_X", "$(rm -rf ~)")]).is_ok());
    }

    #[test]
    fn the_workflow_script_takes_plain_ids_only() {
        let script = set_workflow_mode_script("0198-abc_d.e", "plan").expect("plain");
        assert!(script.contains("'0198-abc_d.e' 'plan'"));
        assert!(script.contains("exec node -e "));
        assert!(
            !SET_WORKFLOW_JS.contains('\''),
            "the JS travels in single quotes"
        );
        assert!(set_workflow_mode_script("a b", "plan").is_err());
        assert!(set_workflow_mode_script("id", "plan; rm").is_err());
        assert!(set_workflow_mode_script("", "plan").is_err());
    }

    /// Run the workflow script against a sandboxed home with this machine's
    /// Node and a fake `pi`: it merges into the map and replaces a broken one.
    #[cfg(unix)]
    #[test]
    fn the_workflow_script_merges_into_workflow_json() {
        if std::process::Command::new("node")
            .arg("--version")
            .output()
            .is_err()
        {
            return; // no Node on this machine: covered by the shape test above
        }
        let home = std::env::temp_dir().join(format!("orbit-workflow-{}", std::process::id()));
        let bin = home.join(".pi/agent/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("pi"), "#!/bin/sh\n").unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(bin.join("pi"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let store = home.join(".orbit-pi/workflow.json");
        let run = |id: &str, mode: &str| {
            // Keep this machine's node reachable, but not a system pi.
            let node = std::process::Command::new("sh")
                .args(["-c", "command -v node"])
                .output()
                .unwrap();
            let node_dir = std::path::Path::new(String::from_utf8_lossy(&node.stdout).trim())
                .parent()
                .unwrap()
                .to_path_buf();
            std::process::Command::new("sh")
                .arg("-c")
                .arg(set_workflow_mode_script(id, mode).unwrap())
                .env("HOME", &home)
                .env("PATH", format!("{}:/usr/bin:/bin", node_dir.display()))
                .output()
                .unwrap()
        };
        let read = || -> serde_json::Value {
            serde_json::from_str(&std::fs::read_to_string(&store).unwrap()).unwrap()
        };
        let first = run("s1", "plan");
        assert!(
            first.status.success(),
            "{}",
            String::from_utf8_lossy(&first.stderr)
        );
        assert!(String::from_utf8_lossy(&first.stdout).starts_with(WORKFLOW_STARTED));
        assert!(run("s2", "ask").status.success());
        assert_eq!(read(), serde_json::json!({"s1": "plan", "s2": "ask"}));
        std::fs::write(&store, "not json").unwrap();
        assert!(run("s3", "build").status.success());
        assert_eq!(read(), serde_json::json!({"s3": "build"}));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A sandbox home with a stub `pi` where the installer puts it, and a
    /// `PATH` that reaches this machine's `node` and `git` but not a system
    /// `pi`. `None` when this machine has no Node.
    #[cfg(unix)]
    fn script_sandbox(tag: &str) -> Option<(std::path::PathBuf, String)> {
        use std::os::unix::fs::PermissionsExt;
        let node = std::process::Command::new("sh")
            .args(["-c", "command -v node"])
            .output()
            .ok()?;
        let node = String::from_utf8_lossy(&node.stdout).trim().to_string();
        if node.is_empty() {
            return None;
        }
        let home = std::env::temp_dir().join(format!(
            "orbit-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bin = home.join(".pi/agent/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("pi"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(bin.join("pi"), std::fs::Permissions::from_mode(0o755)).unwrap();
        let node_dir = std::path::Path::new(&node)
            .parent()
            .unwrap()
            .display()
            .to_string();
        Some((home, format!("{node_dir}:/usr/bin:/bin")))
    }

    #[cfg(unix)]
    fn run_in(
        home: &std::path::Path,
        path: &str,
        script: &str,
        input: Option<&str>,
    ) -> std::process::Output {
        use std::io::Write;
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .env("HOME", home)
            .env("PATH", path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        if let Some(input) = input {
            stdin.write_all(input.as_bytes()).unwrap();
        }
        drop(stdin);
        child.wait_with_output().unwrap()
    }

    #[test]
    fn clone_destinations_are_home_relative_or_absolute() {
        assert!(is_clone_destination("~/code/app"));
        assert!(is_clone_destination("/srv/app"));
        assert!(!is_clone_destination("~/"));
        assert!(!is_clone_destination("/"));
        assert!(!is_clone_destination("code/app"));
        assert!(!is_clone_destination("~/a/../../etc"));
        assert!(!is_clone_destination("~/a\nb"));
        assert!(clone_script("--upload-pack=x", "~/a").is_err());
        assert!(clone_script("https://h/r.git x", "~/a").is_err());
        assert!(clone_script("https://h/r.git", "a").is_err());
        let script = clone_script("git@github.com:o/r.git", "~/code/it's").unwrap();
        assert!(
            script.contains("dest=\"$HOME\"/'code/it'\\''s'"),
            "{script}"
        );
        assert!(script.contains("git clone --quiet -- \"$url\" \"$dest\""));
        assert!(
            !COPY_FILES_JS.contains('\''),
            "the JS travels in single quotes"
        );
        assert!(copy_files_script("relative").is_err());
    }

    /// Clone a real local repository into a sandbox home: done, then reused,
    /// then refused over a different folder's contents; a bad URL reports
    /// git's own words as `detail` lines and still exits 0.
    #[cfg(unix)]
    #[test]
    fn the_clone_script_clones_reuses_and_explains() {
        let Some((home, path)) = script_sandbox("clone") else {
            return;
        };
        let git = |args: &[&str], dir: &std::path::Path| {
            let ok = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("HOME", &home)
                .output()
                .map(|out| out.status.success())
                .unwrap_or(false);
            assert!(ok, "git {args:?}");
        };
        let origin = home.join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        git(&["init", "-q"], &origin);
        std::fs::write(origin.join("README"), "hi\n").unwrap();
        git(&["add", "README"], &origin);
        git(
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-qm",
                "init",
            ],
            &origin,
        );
        let url = origin.display().to_string();

        let lines = |out: &std::process::Output| String::from_utf8_lossy(&out.stdout).into_owned();
        let first = run_in(
            &home,
            &path,
            &clone_script(&url, "~/code/app").unwrap(),
            None,
        );
        assert!(first.status.success());
        let real = std::fs::canonicalize(home.join("code/app")).unwrap();
        assert!(
            lines(&first).contains(&format!("{CLONE_PREFIX}done {}", real.display())),
            "{}{}",
            lines(&first),
            String::from_utf8_lossy(&first.stderr)
        );
        assert!(home.join("code/app/README").exists());

        let again = run_in(
            &home,
            &path,
            &clone_script(&url, "~/code/app").unwrap(),
            None,
        );
        assert!(lines(&again).contains(&format!("{CLONE_PREFIX}existing {}", real.display())));

        std::fs::create_dir_all(home.join("busy")).unwrap();
        std::fs::write(home.join("busy/file"), "x").unwrap();
        let busy = run_in(&home, &path, &clone_script(&url, "~/busy").unwrap(), None);
        assert!(lines(&busy).contains(&format!("{CLONE_PREFIX}failed not-empty")));

        let missing = home.join("no-such-repo").display().to_string();
        let bad = run_in(
            &home,
            &path,
            &clone_script(&missing, "~/other").unwrap(),
            None,
        );
        assert!(
            bad.status.success(),
            "git's failure is reported, not exited with"
        );
        assert!(lines(&bad).contains(&format!("{CLONE_PREFIX}failed ")));
        assert!(lines(&bad).contains(&format!("{CLONE_PREFIX}detail ")));
        assert!(
            bad.stderr.is_empty(),
            "nothing for the ssh runner to misread"
        );
        assert!(
            !home.join("other").exists(),
            "a failed clone leaves no folder"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    /// Copy files into a project with this machine's Node: nested folders are
    /// created, files are 0600, and paths that escape — `..`, absolute, or a
    /// folder symlinked outside — are refused one by one.
    #[cfg(unix)]
    #[test]
    fn the_copy_script_writes_inside_the_project_only() {
        use std::os::unix::fs::PermissionsExt;
        let Some((home, path)) = script_sandbox("copy") else {
            return;
        };
        let project = home.join("project");
        let outside = home.join("outside");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, project.join("linked")).unwrap();
        std::os::unix::fs::symlink(outside.join("target"), project.join(".envrc")).unwrap();
        let b64 = |text: &str| {
            use std::fmt::Write;
            // Tiny base64 encoder: the crate has no base64 dependency.
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let bytes = text.as_bytes();
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let n = (chunk[0] as u32) << 16
                    | (*chunk.get(1).unwrap_or(&0) as u32) << 8
                    | *chunk.get(2).unwrap_or(&0) as u32;
                for i in 0..4 {
                    if i <= chunk.len() {
                        let _ = out.write_char(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                    } else {
                        out.push('=');
                    }
                }
            }
            out
        };
        let input = serde_json::json!([
            {"path": ".env", "data": b64("KEY=1\n")},
            {"path": "apps/web/.env.local", "data": b64("WEB=2\n")},
            {"path": ".envrc", "data": b64("use nix\n")},
            {"path": "../escape", "data": b64("x")},
            {"path": "/etc/passwd", "data": b64("x")},
            {"path": "linked/.env", "data": b64("x")},
        ])
        .to_string();
        let real = std::fs::canonicalize(&project).unwrap();
        let out = run_in(
            &home,
            &path,
            &copy_files_script(&real.display().to_string()).unwrap(),
            Some(&input),
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(stdout.starts_with(COPY_STARTED));
        for wrote in [".env", "apps/web/.env.local", ".envrc"] {
            assert!(
                stdout.contains(&format!("orbit-copy: wrote {wrote}\n")),
                "{stdout}"
            );
        }
        for refused in ["../escape", "/etc/passwd", "linked/.env"] {
            assert!(
                stdout.contains(&format!("orbit-copy: failed {refused}\t")),
                "{stdout}"
            );
        }
        assert_eq!(
            std::fs::read_to_string(project.join(".env")).unwrap(),
            "KEY=1\n"
        );
        assert_eq!(
            std::fs::read_to_string(project.join("apps/web/.env.local")).unwrap(),
            "WEB=2\n"
        );
        let mode = std::fs::metadata(project.join(".env"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(
            !std::fs::symlink_metadata(project.join(".envrc"))
                .unwrap()
                .file_type()
                .is_symlink(),
            "a symlink at the file's name is replaced, not written through"
        );
        assert!(!outside.join("target").exists());
        assert!(!outside.join(".env").exists());
        assert!(!home.join("escape").exists());
        let _ = std::fs::remove_dir_all(&home);
    }

    /// List a sandboxed session store with this machine's Node: named and
    /// unnamed sessions come back newest-first with the newest user message's
    /// time, a draft and a non-session file are skipped, and `limit` holds.
    #[cfg(unix)]
    #[test]
    fn the_session_list_reads_only_what_a_row_shows() {
        let Some((home, path)) = script_sandbox("sessions") else {
            return;
        };
        assert!(
            !LIST_SESSIONS_JS.contains('\''),
            "the JS travels in single quotes"
        );
        let store = home.join(".pi/agent/sessions/--srv-app--");
        std::fs::create_dir_all(&store).unwrap();
        let write = |name: &str, lines: &[serde_json::Value]| {
            let text: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
            std::fs::write(store.join(name), text.join("\n") + "\n").unwrap();
            // Space the mtimes so the newest-first order is deterministic.
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        let header = |id: &str| serde_json::json!({"type": "session", "id": id, "cwd": "/srv/app"});
        let user = |text: &str, at: &str| {
            serde_json::json!({
                "type": "message", "timestamp": at,
                "message": {"role": "user", "content": [{"type": "text", "text": text}]}
            })
        };
        write(
            "a.jsonl",
            &[
                header("a"),
                user("fix the build", "2026-01-01T10:00:00Z"),
                serde_json::json!({"type": "session_info", "name": "Build fix"}),
                user("and the tests", "2026-01-02T10:00:00Z"),
                serde_json::json!({"type": "message", "message": {"role": "assistant", "content": "ok"}}),
            ],
        );
        write("draft.jsonl", &[header("draft")]);
        write("notes.txt", &[header("nope")]);
        write(
            "b.jsonl",
            &[header("b"), user("hello there", "2026-01-03T10:00:00Z")],
        );

        let out = run_in(&home, &path, &list_sessions_script(300), None);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(
            out.status.success(),
            "{stdout}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut lines = stdout.lines();
        assert_eq!(lines.next(), Some(LIST_SESSIONS_STARTED));
        let rows: Vec<serde_json::Value> =
            lines.map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(rows.len(), 2, "{stdout}");
        assert_eq!(rows[0]["id"], "b", "newest file first");
        assert_eq!(rows[0]["name"], serde_json::Value::Null);
        assert_eq!(rows[1]["id"], "a");
        assert_eq!(rows[1]["name"], "Build fix");
        assert_eq!(rows[1]["first"], "fix the build");
        assert_eq!(
            rows[1]["time"], "2026-01-02T10:00:00Z",
            "the newest user message"
        );
        assert_eq!(rows[1]["cwd"], "/srv/app");
        assert!(rows[1]["path"]
            .as_str()
            .unwrap()
            .ends_with("/--srv-app--/a.jsonl"));

        let one = run_in(&home, &path, &list_sessions_script(1), None);
        assert_eq!(
            String::from_utf8_lossy(&one.stdout).lines().count(),
            2,
            "limit holds"
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn branch_names_follow_gits_rules() {
        for ok in ["main", "feat/remote-ssh", "fix_1.2", "release/v2"] {
            assert!(is_branch_name(ok), "{ok}");
        }
        for bad in [
            "", "-f", "/a", "a/", "a.", "a.lock", "a..b", "a@{1}", "a//b", "a/.b", "a b", "a~1",
            "a^", "a:b", "a?", "a*", "a[", "a\\b", "@",
        ] {
            assert!(!is_branch_name(bad), "{bad:?}");
        }
        assert!(switch_branch_script(&target("/a"), "x; rm -rf ~", false).is_err());
        let script = switch_branch_script(&target("/srv/app"), "feat/x", false).unwrap();
        assert!(script.contains("git switch 'feat/x'"));
        assert!(
            script.contains("git checkout 'feat/x' --"),
            "the fallback never reads the name as a file"
        );
        let create = switch_branch_script(&target("/srv/app"), "feat/x", true).unwrap();
        assert!(create.contains("git switch -c 'feat/x'"));
        assert!(create.contains("git checkout -b 'feat/x'"));
    }

    /// Against a real repository: list newest first, switch, create, and a
    /// failed switch reported on stdout with exit 0 while the file of the same
    /// name keeps its edits.
    #[cfg(unix)]
    #[test]
    fn the_branch_scripts_list_switch_and_create() {
        let Some((home, path)) = script_sandbox("branches") else {
            return;
        };
        let repo = home.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("HOME", &home)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("notes"), "v1\n").unwrap();
        git(&["add", "notes"]);
        git(&[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-qm",
            "one",
        ]);
        git(&["branch", "older"]);
        let target = RemoteTarget {
            host: "vps".into(),
            port: None,
            dir: repo.display().to_string(),
        };
        let stdout = |script: String| {
            let out = run_in(&home, &path, &script, None);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).into_owned()
        };

        let listed = stdout(list_branches_script(&target).unwrap());
        let names: Vec<&str> = listed
            .lines()
            .skip(1)
            .map(|line| line.split('\t').next().unwrap())
            .collect();
        assert_eq!(listed.lines().next(), Some(BRANCHES_STARTED));
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"main") && names.contains(&"older"));
        assert!(listed.lines().nth(1).unwrap().contains('\t'), "dated");

        assert!(
            stdout(switch_branch_script(&target, "older", false).unwrap())
                .ends_with("orbit-branch: ok\n")
        );
        let created = stdout(switch_branch_script(&target, "feat/new", true).unwrap());
        assert!(created.ends_with("orbit-branch: ok\n"), "{created}");
        let head = std::fs::read_to_string(repo.join(".git/HEAD")).unwrap();
        assert!(head.contains("refs/heads/feat/new"));

        // A file named like a missing branch keeps its edits.
        std::fs::write(repo.join("notes"), "edited\n").unwrap();
        let failed = stdout(switch_branch_script(&target, "notes", false).unwrap());
        assert!(failed.ends_with("orbit-branch: failed\n"), "{failed}");
        assert!(failed.contains("orbit-branch: detail "));
        assert_eq!(
            std::fs::read_to_string(repo.join("notes")).unwrap(),
            "edited\n"
        );
        let _ = std::fs::remove_dir_all(&home);
    }
}
