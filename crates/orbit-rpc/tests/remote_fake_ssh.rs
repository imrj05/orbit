//! End-to-end transport test for remote (SSH) pi sessions.
//!
//! A fake `ssh` executable receives the real argv Orbit builds, runs the
//! remote script in a fake home directory with a fake `pi` on its PATH, and
//! the real [`PiClient`] talks RPC through it. This proves the launch script
//! installs the guard, pins the access mode, and that stdio carries RPC both
//! ways. Unix-only because the fakes are shell scripts.

#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use orbit_rpc::remote::{
    RemoteExtension, RemoteFile, RemoteLaunch, RemoteTarget, SshPassword, ASKPASS_SCRIPT,
};
use orbit_rpc::{CommandBody, Event, PiClient};

/// Runs the remote script the way sshd would hand it to the login shell, but
/// without the login profile (which can reset PATH on some hosts).
const FAKE_SSH: &str = r#"#!/bin/sh
# The last argument is `sh -c '<dispatch>' orbit '<script>'`. Split it back and run
# the script with a non-login sh, after pointing HOME and PATH at the fakes.
last=""
batch=""
for arg; do
  last="$arg"
  case "$arg" in BatchMode=*) batch="$arg" ;; esac
done
printf '%s\n' "$batch" > "__HOME__/ssh-batch"
# Ask the askpass helper the three kinds of question ssh asks, the way ssh
# would, and record each answer (one line per question, empty when declined).
if [ -n "${SSH_ASKPASS:-}" ]; then
  {
    "$SSH_ASKPASS" "deploy@vps's password: " || printf '\n'
    "$SSH_ASKPASS" "Are you sure you want to continue connecting (yes/no/[fingerprint])? " || printf '\n'
    "$SSH_ASKPASS" "Enter passphrase for key '/home/u/.ssh/id_ed25519': " || printf '<declined>\n'
  } > "__HOME__/askpass-answers"
fi
eval "set -- $last"
export HOME="__HOME__"
export PATH="__BIN__:$PATH"
# `sh -c '<dispatch>' orbit '<script>'`: run the script with a plain sh. The
# dispatcher itself is covered in `remote.rs`; a login shell here would read
# this machine's profiles.
exec sh -c "$5"
"#;

const FAKE_PI: &str = r#"#!/bin/sh
# Records its argv, then answers RPC commands with the command id echoed back.
printf '%s\n' "$*" > "__HOME__/pi-argv"
printf '%s\n' "${ORBIT_WORKFLOW_MODE:-<unset>}" > "__HOME__/pi-workflow-env"
emit() { printf '%s\n' "$1" | sed "s/^{/{\"id\":\"$id\",/"; }
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"type":"get_state"'*)
      emit '{"type":"response","command":"get_state","success":true,"data":{"sessionId":"remote-1"}}'
      ;;
  esac
done
"#;

fn make_exec(path: &Path, body: &str) {
    fs::write(path, body).expect("write script");
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("chmod");
}

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "orbit-remote-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("home")).unwrap();
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("project")).unwrap();
        let home = root.join("home").display().to_string();
        let bin = root.join("bin").display().to_string();
        make_exec(&root.join("bin/pi"), &FAKE_PI.replace("__HOME__", &home));
        make_exec(
            &root.join("ssh"),
            &FAKE_SSH.replace("__HOME__", &home).replace("__BIN__", &bin),
        );
        Self { root }
    }

    fn ssh(&self) -> PathBuf {
        self.root.join("ssh")
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn remote_session_installs_guard_pins_mode_and_speaks_rpc() {
    let sandbox = Sandbox::new("ok");
    let target = RemoteTarget {
        host: "vps".into(),
        port: Some(2222),
        dir: sandbox.root.join("project").display().to_string(),
    };
    let guard = [RemoteFile {
        name: "index.js",
        contents: "export default function activate() {}",
    }];
    let workflow = [RemoteFile {
        name: "index.js",
        contents: "export default function workflow() {}",
    }];
    let client = PiClient::spawn_remote_with_bin(
        sandbox.ssh().to_str().expect("utf-8"),
        &target,
        &RemoteLaunch {
            guard: &guard,
            extensions: &[RemoteExtension {
                dir: "remote-workflow",
                files: &workflow,
            }],
            env: &[("ORBIT_WORKFLOW_MODE", "plan")],
        },
        None,
    )
    .expect("spawn remote");

    let event = client
        .send_blocking(CommandBody::GetState, Duration::from_secs(10))
        .expect("get_state answered over ssh stdio");
    match event {
        Event::Response { success, .. } => assert!(success),
        other => panic!("unexpected event: {other:?}"),
    }

    let installed = fs::read_to_string(sandbox.home().join(".orbit-pi/remote-guard/index.js"))
        .expect("guard installed on host");
    assert_eq!(
        installed.trim_end(),
        "export default function activate() {}"
    );
    let mode = fs::read_to_string(sandbox.home().join(".orbit-pi/access.json")).expect("mode");
    assert!(mode.contains("\"supervised\""), "mode pinned: {mode}");
    let argv = fs::read_to_string(sandbox.home().join("pi-argv")).expect("pi argv");
    assert!(
        argv.contains("--mode rpc --approve --extension"),
        "argv: {argv}"
    );
    assert!(argv.contains("remote-guard/index.js"), "argv: {argv}");
    assert!(
        argv.ends_with("remote-workflow/index.js\n"),
        "the extra extension loads after the guard: {argv}"
    );
    let workflow = fs::read_to_string(sandbox.home().join(".orbit-pi/remote-workflow/index.js"))
        .expect("workflow extension installed on host");
    assert_eq!(workflow.trim_end(), "export default function workflow() {}");
    let env = fs::read_to_string(sandbox.home().join("pi-workflow-env")).expect("env seen");
    assert_eq!(env.trim(), "plan");
}

#[test]
fn missing_pi_on_host_never_answers_rpc() {
    let sandbox = Sandbox::new("nopi");
    fs::remove_file(sandbox.root.join("bin/pi")).unwrap();
    let target = RemoteTarget {
        host: "vps".into(),
        port: None,
        dir: sandbox.root.join("project").display().to_string(),
    };
    let client = PiClient::spawn_remote_with_bin(
        sandbox.ssh().to_str().expect("utf-8"),
        &target,
        &RemoteLaunch {
            guard: &[RemoteFile {
                name: "index.js",
                contents: "x",
            }],
            ..RemoteLaunch::default()
        },
        None,
    )
    .expect("spawn ssh");
    // The process exits on its own; no RPC answer ever comes back.
    let result = client.send_blocking(CommandBody::GetState, Duration::from_secs(5));
    assert!(result.is_err(), "no response expected without pi");
}

/// A password run: ssh is allowed to ask (BatchMode=no) and the real askpass
/// helper answers the password prompt with the secret, refuses an unknown
/// host key, and declines a key passphrase.
#[test]
fn a_password_reaches_ssh_only_through_the_askpass_helper() {
    let sandbox = Sandbox::new("password");
    let askpass = sandbox.root.join("askpass.sh");
    make_exec(&askpass, ASKPASS_SCRIPT);
    let target = RemoteTarget {
        host: "deploy@vps".into(),
        port: None,
        dir: sandbox.root.join("project").display().to_string(),
    };
    let client = PiClient::spawn_remote_with_bin(
        sandbox.ssh().to_str().expect("utf-8"),
        &target,
        &RemoteLaunch {
            guard: &[RemoteFile {
                name: "index.js",
                contents: "export default function activate() {}",
            }],
            ..RemoteLaunch::default()
        },
        Some(SshPassword {
            askpass: &askpass,
            secret: "s3cret 'with quotes'",
        }),
    )
    .expect("spawn remote");
    client
        .send_blocking(CommandBody::GetState, Duration::from_secs(10))
        .expect("the session still speaks RPC");

    let batch = fs::read_to_string(sandbox.home().join("ssh-batch")).expect("argv seen");
    assert_eq!(batch.trim(), "BatchMode=no");
    let answers = fs::read_to_string(sandbox.home().join("askpass-answers")).expect("answers");
    let answers: Vec<&str> = answers.lines().collect();
    assert_eq!(
        answers,
        ["s3cret 'with quotes'", "no", "<declined>"],
        "password answered, host key refused, passphrase declined"
    );
}

/// Without a password ssh never asks.
#[test]
fn no_password_means_batch_mode() {
    let sandbox = Sandbox::new("batch");
    let target = RemoteTarget {
        host: "vps".into(),
        port: None,
        dir: sandbox.root.join("project").display().to_string(),
    };
    let client = PiClient::spawn_remote_with_bin(
        sandbox.ssh().to_str().expect("utf-8"),
        &target,
        &RemoteLaunch {
            guard: &[RemoteFile {
                name: "index.js",
                contents: "x",
            }],
            ..RemoteLaunch::default()
        },
        None,
    )
    .expect("spawn remote");
    let _ = client.send_blocking(CommandBody::GetState, Duration::from_secs(10));
    let batch = fs::read_to_string(sandbox.home().join("ssh-batch")).expect("argv seen");
    assert_eq!(batch.trim(), "BatchMode=yes");
    assert!(!sandbox.home().join("askpass-answers").exists());
}
