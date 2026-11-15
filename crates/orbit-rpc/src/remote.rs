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

/// Build the POSIX script that installs the guard, pins the access mode and
/// execs `pi --mode rpc`. The guard files are written with quoted heredocs, so
/// their contents are never interpreted by the shell.
///
/// Fails if a file's contents would end the heredoc early.
pub fn launch_script(target: &RemoteTarget, guard: &[RemoteFile<'_>]) -> Result<String> {
    target.validate()?;
    let delimiter = "ORBIT_GUARD_EOF";
    let mut script = String::from("set -eu\n");
    script.push_str(&format!("G=\"$HOME/{REMOTE_GUARD_DIR}\"\n"));
    script.push_str("mkdir -p \"$G\" \"$HOME/.orbit-pi\"\n");
    for file in guard {
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
            "cat > \"$G/{name}\" <<'{delimiter}'\n{contents}\n{delimiter}\n",
            name = file.name,
            contents = file.contents.trim_end_matches('\n'),
        ));
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
    script.push_str(
        "exec env PI_SKIP_VERSION_CHECK=1 \"$PI\" --mode rpc --approve --extension \"$G/index.js\"\n",
    );
    Ok(script)
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
            &[RemoteFile {
                name: "index.js",
                contents: "export default 1;",
            }],
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
        assert!(launch_script(
            &target("/a"),
            &[RemoteFile {
                name: "../x.js",
                contents: ""
            }]
        )
        .is_err());
        assert!(launch_script(
            &target("/a"),
            &[RemoteFile {
                name: "x.js",
                contents: "a\nORBIT_GUARD_EOF\nb"
            }]
        )
        .is_err());
    }
}
