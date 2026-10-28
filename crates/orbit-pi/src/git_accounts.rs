//! Read-only, local Git configuration inspection. No credential protocol,
//! authentication command, private key, or remote connection is used here.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub enum State<T> {
    Detected(T),
    NotConfigured,
    Unavailable,
    Unknown,
    Failed,
}

#[derive(Clone, Debug)]
pub struct ConfigValue {
    pub value: String,
    pub scope: String,
}
#[derive(Clone, Debug)]
pub struct Helper {
    pub kind: &'static str,
    pub scope: String,
    /// Sanitized host for URL-specific helper settings, never the URL itself.
    pub host: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Remote {
    pub name: String,
    pub host: String,
    pub transport: &'static str,
}
#[derive(Clone, Debug)]
pub struct Key {
    pub algorithm: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub name: State<ConfigValue>,
    pub email: State<ConfigValue>,
    pub helpers: State<Vec<Helper>>,
    pub remotes: State<Vec<Remote>>,
    pub ssh: State<Vec<Key>>,
    pub public_keys: State<usize>,
    pub gh: State<String>,
    pub gh_token_present: bool,
    pub github_token_present: bool,
}

// Read at most this much per invocation; discard stderr rather than retaining
// configuration, URLs, credentials, or arbitrary subprocess diagnostics.
const MAX_OUTPUT: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug)]
enum RunError {
    Missing,
    Failed,
    Timeout,
    Oversized,
}
struct Output {
    code: Option<i32>,
    bytes: Vec<u8>,
}

fn run(mut command: Command, args: &[&str]) -> Result<Output, RunError> {
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    orbit_rpc::hide_console(&mut command);
    let mut child = command.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RunError::Missing
        } else {
            RunError::Failed
        }
    })?;
    let stdout = child.stdout.take().ok_or(RunError::Failed)?;
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take((MAX_OUTPUT + 1) as u64)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = tx.send(result);
    });
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            result => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if result.is_err() {
                    RunError::Failed
                } else {
                    RunError::Timeout
                });
            }
        }
    };
    let bytes = rx
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|_| RunError::Timeout)?
        .map_err(|_| RunError::Failed)?;
    if bytes.len() > MAX_OUTPUT {
        return Err(RunError::Oversized);
    }
    Ok(Output {
        code: status.code(),
        bytes,
    })
}

fn error<T>(err: RunError) -> State<T> {
    match err {
        RunError::Missing => State::Unavailable,
        RunError::Timeout | RunError::Oversized => State::Unknown,
        RunError::Failed => State::Failed,
    }
}

fn git_command(workspace: Option<&Path>) -> Command {
    let executable = crate::onboarding::locate("git").unwrap_or_else(|| PathBuf::from("git"));
    let mut command = Command::new(executable);
    if let Some(path) = workspace {
        command.current_dir(path);
    }
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat");
    command
}

fn config(workspace: Option<&Path>, key: &str, all: bool) -> Result<Output, RunError> {
    let mut args = vec!["config", "--null", "--show-scope"];
    if workspace.is_none() {
        args.push("--global");
    }
    args.push(if all { "--get-all" } else { "--get" });
    args.push(key);
    run(git_command(workspace), &args)
}

fn scoped(bytes: &[u8]) -> Option<Vec<(String, String)>> {
    if !bytes.ends_with(&[0]) {
        return None;
    }
    let raw = std::str::from_utf8(bytes).ok()?;
    let mut parts = raw.split_terminator('\0');
    let mut values = Vec::new();
    while let Some(scope) = parts.next() {
        let value = parts.next()?;
        if !matches!(
            scope,
            "system" | "global" | "local" | "worktree" | "command"
        ) || value.chars().any(char::is_control)
        {
            return None;
        }
        values.push((scope.to_owned(), value.to_owned()));
    }
    Some(values)
}

fn identity(workspace: Option<&Path>, key: &str) -> State<ConfigValue> {
    match config(workspace, key, false) {
        Err(e) => error(e),
        Ok(out) if out.code == Some(1) => State::NotConfigured,
        Ok(out) if out.code == Some(0) => {
            match scoped(&out.bytes).and_then(|v| v.into_iter().last()) {
                Some((scope, value)) if !value.trim().is_empty() => {
                    State::Detected(ConfigValue { scope, value })
                }
                _ => State::Unknown,
            }
        }
        Ok(_) => State::Failed,
    }
}

fn helper_kind(value: &str) -> &'static str {
    match value.split_whitespace().next().unwrap_or("") {
        "osxkeychain" => "osxkeychain",
        "manager" | "manager-core" => "manager",
        "store" => "store",
        "cache" => "cache",
        "libsecret" => "libsecret",
        _ => "custom",
    }
}
fn helpers(workspace: Option<&Path>) -> State<Vec<Helper>> {
    let mut args = vec!["config", "--null", "--show-scope"];
    if workspace.is_none() {
        args.push("--global");
    }
    args.extend(["--get-regexp", "^credential\\.(.*\\.)?helper$"]);
    match run(git_command(workspace), &args) {
        Err(e) => error(e),
        Ok(out) if out.code == Some(1) => State::NotConfigured,
        Ok(out) if out.code == Some(0) => parse_helpers(&out.bytes),
        Ok(_) => State::Failed,
    }
}

fn parse_helpers(bytes: &[u8]) -> State<Vec<Helper>> {
    // `--get-regexp` returns scope NUL key NEWLINE value NUL. URL
    // subsections and helper values can contain secrets; neither is retained.
    if !bytes.ends_with(&[0]) {
        return State::Unknown;
    }
    let Ok(raw) = std::str::from_utf8(bytes) else {
        return State::Unknown;
    };
    let mut parts = raw.split_terminator('\0');
    let mut helpers: Vec<(String, Helper)> = Vec::new();
    while let Some(scope) = parts.next() {
        if !matches!(
            scope,
            "system" | "global" | "local" | "worktree" | "command"
        ) {
            return State::Unknown;
        }
        let Some((key, value)) = parts.next().and_then(|entry| entry.split_once('\n')) else {
            return State::Unknown;
        };
        if value.chars().any(char::is_control) {
            return State::Unknown;
        }
        let host = if key == "credential.helper" {
            None
        } else {
            let Some(url) = key
                .strip_prefix("credential.")
                .and_then(|key| key.strip_suffix(".helper"))
            else {
                return State::Unknown;
            };
            let Some(host) = url::Url::parse(url)
                .ok()
                .and_then(|url| url.host_str().and_then(safe_name))
            else {
                return State::Unknown;
            };
            Some(host)
        };
        if value.is_empty() {
            helpers.retain(|(previous, _)| previous != key);
        } else {
            helpers.push((
                key.to_owned(),
                Helper {
                    kind: helper_kind(value),
                    scope: scope.to_owned(),
                    host,
                },
            ));
        }
    }
    if helpers.is_empty() {
        State::NotConfigured
    } else {
        State::Detected(helpers.into_iter().map(|(_, helper)| helper).collect())
    }
}

fn safe_name(s: &str) -> Option<String> {
    if !s.is_empty()
        && s.len() <= 253
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        Some(s.to_owned())
    } else {
        None
    }
}
fn remote(name: &str, value: &str) -> Option<Remote> {
    let name = safe_name(name)?;
    let (host, transport) = if value.contains("://") {
        let url = url::Url::parse(value).ok()?;
        let transport = match url.scheme() {
            "https" => "HTTPS",
            "http" => "HTTP",
            "ssh" => "SSH",
            "git" => "Git",
            _ => return None,
        };
        (url.host_str()?.to_owned(), transport)
    } else {
        let (host, _) = value.split_once(':')?;
        let host = host.rsplit('@').next()?;
        (host.to_owned(), "SSH")
    };
    Some(Remote {
        name,
        host: safe_name(&host)?,
        transport,
    })
}
fn remotes(workspace: Option<&Path>) -> State<Vec<Remote>> {
    let Some(workspace) = workspace else {
        return State::Unavailable;
    };
    match run(
        git_command(Some(workspace)),
        &["rev-parse", "--is-inside-work-tree"],
    ) {
        Ok(out) if out.code == Some(0) && out.bytes == b"true\n" => {}
        Ok(out) if out.code == Some(128) => return State::NotConfigured,
        Err(e) => return error(e),
        _ => return State::Unknown,
    }
    match run(
        git_command(Some(workspace)),
        &["config", "--null", "--get-regexp", "^remote\\..*\\.url$"],
    ) {
        Err(e) => error(e),
        Ok(out) if out.code == Some(1) => State::NotConfigured,
        Ok(out) if out.code == Some(0) => {
            let Ok(raw) = std::str::from_utf8(&out.bytes) else {
                return State::Unknown;
            };
            let mut result = Vec::new();
            for entry in raw.split_terminator('\0') {
                let Some((key, value)) = entry.split_once('\n') else {
                    return State::Unknown;
                };
                let Some(name) = key
                    .strip_prefix("remote.")
                    .and_then(|s| s.strip_suffix(".url"))
                else {
                    return State::Unknown;
                };
                if let Some(remote) = remote(name, value) {
                    result.push(remote);
                } else {
                    return State::Unknown;
                }
            }
            if result.is_empty() {
                State::NotConfigured
            } else {
                State::Detected(result)
            }
        }
        Ok(_) => State::Failed,
    }
}
/// Check only conventional public-key filenames, without opening file contents.
/// Opening the directory without following symlinks and using no-follow stat
/// prevents a renamed private key or symlink from exposing key material.
#[cfg(unix)]
fn public_keys(home: &Path) -> State<usize> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;

    let dir = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY | libc::O_CLOEXEC)
        .open(home.join(".ssh"))
    {
        Ok(dir) => dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return State::NotConfigured,
        Err(_) => return State::Unknown,
    };
    let mut count = 0;
    for name in [
        b"id_rsa.pub\0".as_slice(),
        b"id_ed25519.pub\0",
        b"id_ecdsa.pub\0",
        b"id_dsa.pub\0",
        b"id_ecdsa_sk.pub\0",
        b"id_ed25519_sk.pub\0",
    ] {
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: `dir` is a live directory fd, names are NUL-terminated
        // static byte strings, and stat is initialized on successful return.
        let rc = unsafe {
            libc::fstatat(
                dir.as_raw_fd(),
                name.as_ptr().cast(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc == 0 {
            // SAFETY: fstatat returned success and initialized the structure.
            let stat = unsafe { stat.assume_init() };
            if stat.st_mode & libc::S_IFMT == libc::S_IFREG {
                count += 1;
            }
        } else if std::io::Error::last_os_error().kind() != std::io::ErrorKind::NotFound {
            return State::Unknown;
        }
    }
    if count == 0 {
        State::NotConfigured
    } else {
        State::Detected(count)
    }
}

#[cfg(not(unix))]
fn public_keys(_home: &Path) -> State<usize> {
    State::Unavailable
}

fn ssh() -> State<Vec<Key>> {
    let binary = crate::onboarding::locate("ssh-add").unwrap_or_else(|| PathBuf::from("ssh-add"));
    match run(Command::new(binary), &["-l"]) {
        Err(e) => error(e),
        Ok(out) if out.code == Some(1) => State::NotConfigured,
        Ok(out) if out.code == Some(2) => State::Unavailable,
        Ok(out) if out.code == Some(0) => parse_ssh(&out.bytes),
        Ok(_) => State::Failed,
    }
}

fn parse_ssh(bytes: &[u8]) -> State<Vec<Key>> {
    let Ok(raw) = std::str::from_utf8(bytes) else {
        return State::Unknown;
    };
    let mut keys = Vec::new();
    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let Some(bits) = parts.next() else {
            return State::Unknown;
        };
        let Some(fp) = parts.next() else {
            return State::Unknown;
        };
        let Some(algo) = line.split_whitespace().last() else {
            return State::Unknown;
        };
        if bits.parse::<u32>().is_err()
            || !fp.starts_with("SHA256:")
            || !fp
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '+' | '/' | '='))
        {
            return State::Unknown;
        }
        let algorithm = algo.trim_matches(['(', ')']);
        if !matches!(
            algorithm,
            "RSA" | "ED25519" | "ECDSA" | "DSA" | "ED25519-SK" | "ECDSA-SK"
        ) {
            return State::Unknown;
        }
        keys.push(Key {
            algorithm: algorithm.to_owned(),
            fingerprint: fp.to_owned(),
        });
    }
    if keys.is_empty() {
        State::Unknown
    } else {
        State::Detected(keys)
    }
}
fn gh() -> State<String> {
    // `gh auth status` validates credentials over the network and may refresh
    // them. Only the existing gh binary resolver and version parser are safe.
    gh_version(crate::gh::command(Path::new(".")))
}

fn gh_version(command: Command) -> State<String> {
    match run(command, &["--version"]) {
        Err(e) => error(e),
        Ok(out) if out.code == Some(0) => match std::str::from_utf8(&out.bytes)
            .ok()
            .and_then(crate::gh::parse_version)
        {
            Some((a, b, c)) => State::Detected(format!("{a}.{b}.{c}")),
            None => State::Unknown,
        },
        Ok(_) => State::Failed,
    }
}
pub fn discover(workspace: Option<PathBuf>) -> Snapshot {
    let path = workspace.as_deref();
    Snapshot {
        name: identity(path, "user.name"),
        email: identity(path, "user.email"),
        helpers: helpers(path),
        remotes: remotes(path),
        ssh: ssh(),
        public_keys: public_keys(&crate::platform::home_dir()),
        gh: gh(),
        gh_token_present: std::env::var_os("GH_TOKEN").is_some(),
        github_token_present: std::env::var_os("GITHUB_TOKEN").is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_scopes_and_malformed_output() {
        assert_eq!(
            scoped(b"global\0A\0local\0B\0").unwrap()[1],
            ("local".into(), "B".into())
        );
        assert!(scoped(b"global\0missing").is_none());
        assert!(scoped(b"other\0value\0").is_none());
    }
    #[test]
    fn ssh_metadata_discards_comments_and_rejects_malformed_lines() {
        let parsed = parse_ssh(b"256 SHA256:abc123 private-comment (ED25519)\n");
        match parsed {
            State::Detected(keys) => {
                assert_eq!(keys[0].algorithm, "ED25519");
                assert!(!format!("{keys:?}").contains("private-comment"));
            }
            other => panic!("unexpected ssh status: {other:?}"),
        }
        assert!(matches!(parse_ssh(b"invalid listing\n"), State::Unknown));
    }
    #[cfg(unix)]
    #[test]
    fn gh_discovery_only_invokes_version() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!(
            "orbit-fake-gh-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let binary = dir.join("gh");
        let log = dir.join("args");
        std::fs::write(&binary, "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$ORBIT_GH_TEST_LOG\"\nprintf 'gh version 2.40.1\\n'\n").unwrap();
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut command = Command::new(binary);
        command.env("ORBIT_GH_TEST_LOG", &log);
        assert!(matches!(gh_version(command), State::Detected(_)));
        assert_eq!(std::fs::read_to_string(log).unwrap(), "--version\n");
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn timed_out_process_does_not_look_signed_out() {
        assert!(matches!(
            run(Command::new("sleep"), &["4"]),
            Err(RunError::Timeout)
        ));
    }
    #[test]
    fn missing_process_is_not_reported_as_unauthenticated() {
        assert!(matches!(
            run(Command::new("orbit-command-that-does-not-exist"), &[]),
            Err(RunError::Missing)
        ));
    }
    #[cfg(unix)]
    #[test]
    fn command_output_is_bounded() {
        assert!(matches!(
            run(Command::new("yes"), &[]),
            Err(RunError::Oversized)
        ));
    }
    #[test]
    fn scoped_helpers_are_sanitized_and_resets_are_applied() {
        let bytes = b"global\0credential.helper\nstore\0local\0credential.helper\n\0local\0credential.https://user:password@example.com/path.helper\n!echo secret\0";
        match parse_helpers(bytes) {
            State::Detected(helpers) => {
                assert_eq!(helpers.len(), 1);
                assert_eq!(helpers[0].host.as_deref(), Some("example.com"));
                assert_eq!(helpers[0].kind, "custom");
                let shown = format!("{helpers:?}");
                assert!(!shown.contains("password"));
                assert!(!shown.contains("secret"));
                assert!(!shown.contains("path"));
            }
            other => panic!("unexpected helper status: {other:?}"),
        }
        assert!(matches!(
            parse_helpers(b"local\0credential.helper"),
            State::Unknown
        ));
    }
    #[cfg(unix)]
    #[test]
    fn public_key_detection_does_not_follow_symlinks_or_read_files() {
        use std::os::unix::fs::symlink;
        let home = std::env::temp_dir().join(format!(
            "orbit-public-keys-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dir = home.join(".ssh");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("id_rsa"), b"PRIVATE KEY MATERIAL").unwrap();
        symlink("id_rsa", dir.join("id_rsa.pub")).unwrap();
        assert!(matches!(public_keys(&home), State::NotConfigured));
        std::fs::write(dir.join("id_ed25519.pub"), b"public").unwrap();
        assert!(matches!(public_keys(&home), State::Detected(1)));
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn helpers_never_expose_commands() {
        assert_eq!(helper_kind("osxkeychain"), "osxkeychain");
        assert_eq!(helper_kind("store --file=/tmp/token"), "store");
        assert_eq!(helper_kind("cache --timeout=3600"), "cache");
        assert_eq!(helper_kind("!echo secret"), "custom");
    }
    #[test]
    fn workspace_local_identity_and_remotes_are_read_without_remote_access() {
        let path = std::env::temp_dir().join(format!(
            "orbit-git-accounts-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .current_dir(&path)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        git(&["config", "--local", "user.name", "Local Author"]);
        git(&["config", "--local", "user.email", "author@example.test"]);
        let marker = path.join("helper-was-executed");
        git(&[
            "config",
            "--local",
            "credential.helper",
            &format!("!touch {}", marker.display()),
        ]);
        assert!(matches!(helpers(Some(&path)), State::Detected(_)));
        assert!(
            !marker.exists(),
            "inspection must not execute credential helpers"
        );
        git(&[
            "config",
            "--local",
            "remote.origin.url",
            "https://user:secret@example.test/owner/repo?token=secret",
        ]);
        match identity(Some(&path), "user.name") {
            State::Detected(value) => {
                assert_eq!(value.value, "Local Author");
                assert_eq!(value.scope, "local");
            }
            other => panic!("unexpected identity: {other:?}"),
        }
        match remotes(Some(&path)) {
            State::Detected(remote) => {
                assert_eq!(remote[0].host, "example.test");
                assert!(!format!("{remote:?}").contains("secret"));
            }
            other => panic!("unexpected remotes: {other:?}"),
        }
        std::fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn remote_reduces_urls_to_host_and_transport() {
        let r = remote("origin", "https://user:secret@example.com/a?token=secret").unwrap();
        assert_eq!(r.host, "example.com");
        assert_eq!(r.transport, "HTTPS");
        assert!(!format!("{r:?}").contains("secret"));
        assert_eq!(
            remote("origin", "git@github.com:owner/repo").unwrap().host,
            "github.com"
        );
        assert!(remote("origin", "file:///tmp/key").is_none());
    }
}
