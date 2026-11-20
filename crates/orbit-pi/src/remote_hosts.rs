//! Remote hosts for agent-only sessions: the config file, its surgical writer,
//! `~/.ssh/config` alias discovery, and the read-only connection probe.
//!
//! Hosts live in `~/.orbit-pi/remote-hosts.json`. Settings → Remote hosts
//! writes it; hand edits keep working:
//!
//! ```jsonc
//! { "hosts": [ { "name": "prod-vps", "ssh": "prod-vps", "user": "deploy", "port": 22, "dir": "/srv/app", "local_dir": "~/code/app" } ] }
//! ```
//!
//! `ssh` is an `~/.ssh/config` alias or a hostname (`user@host` still works);
//! `user` and `port` are optional and override the alias. Only the agent runs
//! on the host; files, Git and the explorer stay local (see
//! `crates/orbit-pi/docs/remote-sessions.md`). Parsing is lenient: a bad entry
//! is skipped with a reason, never a blocked launch. Writing is surgical: only
//! the keys Orbit owns change, so other keys on an entry and other
//! top-level keys survive a save.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use orbit_rpc::remote::RemoteTarget;
use serde::Deserialize;
use serde_json::{Map, Value};

/// One configured host, as written in the file.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteHost {
    pub name: String,
    pub ssh: String,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub port: Option<u16>,
    pub dir: String,
    /// Off hides the host from the New Task folder picker without forgetting
    /// it (Settings → Remote hosts toggle). Absent in the file means on.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// The folder on this machine the entry is the same project as (`~/` is
    /// the home folder). Set, the entry's sessions list inside that project's
    /// sidebar group instead of a group of their own. "Add to a server"
    /// fills it with the project it cloned; Settings → Remote hosts edits it.
    #[serde(default)]
    pub local_dir: Option<String>,
}

fn default_enabled() -> bool {
    true
}

impl RemoteHost {
    /// What ssh connects to: `user@ssh` when a user is set, else `ssh` as
    /// written (which may itself carry `user@`).
    pub fn destination(&self) -> String {
        let host = self.ssh.trim();
        match self
            .user
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
        {
            Some(user) => format!("{user}@{host}"),
            None => host.to_string(),
        }
    }

    /// The ssh/rpc target for this host, validated.
    pub fn target(&self) -> Result<RemoteTarget> {
        if self.name.trim().is_empty() {
            bail!("remote host has no name");
        }
        if let Some(user) = &self.user {
            if user.trim().starts_with('-') || user.contains(['@', ' ', '\t']) {
                bail!("username `{user}` is not a plain user name");
            }
        }
        let target = RemoteTarget {
            host: self.destination(),
            port: self.port,
            dir: self.dir.clone(),
        };
        target
            .validate()
            .with_context(|| format!("remote host `{}`", self.name))?;
        Ok(target)
    }

    /// `ssh:dir`, the row detail that says exactly where the agent runs.
    pub fn location(&self) -> String {
        format!("{}:{}", self.destination(), self.dir)
    }

    /// Which ssh account a password belongs to: the destination and port.
    /// Two hosts on the same account (different directories) share one
    /// password; renaming a host keeps it.
    pub fn account_key(&self) -> String {
        format!("{}:{}", self.destination(), self.port.unwrap_or(22))
    }

    /// The command that installs pi on this host with pi's own installer, in
    /// a terminal (`-t`: the installer asks questions).
    pub fn install_command(&self) -> String {
        let port = self
            .port
            .map(|port| format!("-p {port} "))
            .unwrap_or_default();
        format!(
            "ssh -t {port}{} 'curl -fsSL https://pi.dev/install.sh | sh'",
            self.destination()
        )
    }

    /// `user@host:port`, the one-line connection summary a row shows.
    pub fn connection_label(&self) -> String {
        match self.port {
            Some(port) => format!("{}:{port}", self.destination()),
            None => self.destination(),
        }
    }

    /// The JSON keys Orbit owns, written over an entry without touching the
    /// rest of it.
    fn write_into(&self, entry: &mut Map<String, Value>) {
        entry.insert("name".into(), Value::String(self.name.trim().to_string()));
        entry.insert("ssh".into(), Value::String(self.ssh.trim().to_string()));
        match self
            .user
            .as_deref()
            .map(str::trim)
            .filter(|u| !u.is_empty())
        {
            Some(user) => {
                entry.insert("user".into(), Value::String(user.to_string()));
            }
            None => {
                entry.remove("user");
            }
        }
        match self.port {
            Some(port) => {
                entry.insert("port".into(), Value::from(port));
            }
            None => {
                entry.remove("port");
            }
        }
        entry.insert("dir".into(), Value::String(self.dir.trim().to_string()));
        // Written only when off, so a file that never used the toggle stays as
        // it was.
        if self.enabled {
            entry.remove("enabled");
        } else {
            entry.insert("enabled".into(), Value::Bool(false));
        }
        match self
            .local_dir
            .as_deref()
            .map(str::trim)
            .filter(|dir| !dir.is_empty())
        {
            Some(dir) => {
                entry.insert("local_dir".into(), Value::String(dir.to_string()));
            }
            None => {
                entry.remove("local_dir");
            }
        }
    }

    /// The linked local folder with `~/` expanded, when one is set.
    pub fn local_path(&self) -> Option<PathBuf> {
        let dir = self.local_dir.as_deref()?.trim();
        if dir.is_empty() {
            return None;
        }
        Some(match dir.strip_prefix("~/") {
            Some(rest) => crate::platform::home_dir().join(rest),
            None if dir == "~" => crate::platform::home_dir(),
            None => PathBuf::from(dir),
        })
    }
}

#[derive(Deserialize)]
struct HostsFile {
    #[serde(default)]
    hosts: Vec<Value>,
}

/// Parse the file's contents. Entries that do not decode or validate are
/// returned as skipped reasons, so one typo never hides the other hosts.
pub fn parse(contents: &str) -> (Vec<RemoteHost>, Vec<String>) {
    let Ok(file) = serde_json::from_str::<HostsFile>(contents) else {
        return (
            Vec::new(),
            vec!["remote-hosts.json is not valid JSON".into()],
        );
    };
    let mut hosts = Vec::new();
    let mut skipped = Vec::new();
    for (index, raw) in file.hosts.into_iter().enumerate() {
        match serde_json::from_value::<RemoteHost>(raw) {
            Ok(host) => match host.target() {
                Ok(_) => hosts.push(host),
                Err(err) => skipped.push(format!("entry {}: {err:#}", index + 1)),
            },
            Err(err) => skipped.push(format!("entry {}: {err}", index + 1)),
        }
    }
    (hosts, skipped)
}

/// `~/.orbit-pi/remote-hosts.json`.
pub fn config_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("remote-hosts.json")
}

/// Load the configured hosts. A missing file means no hosts.
pub fn load() -> (Vec<RemoteHost>, Vec<String>) {
    load_from(&config_path())
}

fn load_from(path: &Path) -> (Vec<RemoteHost>, Vec<String>) {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse(&contents),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => (Vec::new(), Vec::new()),
        Err(err) => (
            Vec::new(),
            vec![format!("cannot read remote-hosts.json: {err}")],
        ),
    }
}

/// Add `host`, or replace the entry named `original` (an edit, possibly a
/// rename). Names are the identity, so a second host with the same name is
/// refused rather than shadowing the first.
pub fn upsert(original: Option<&str>, host: &RemoteHost) -> Result<()> {
    upsert_at(&config_path(), original, host)
}

/// Remove the host named `name`. Removing a host that is not there is a no-op.
pub fn remove(name: &str) -> Result<()> {
    remove_at(&config_path(), name)
}

fn read_document(path: &Path) -> Result<Map<String, Value>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(err) => return Err(err).context("cannot read remote-hosts.json"),
    };
    match serde_json::from_str::<Value>(&contents) {
        Ok(Value::Object(map)) => Ok(map),
        // Overwriting a file Orbit cannot read would destroy what is in it.
        _ => bail!("remote-hosts.json is not a JSON object; fix it by hand before saving"),
    }
}

fn entry_name(entry: &Value) -> Option<&str> {
    entry.get("name").and_then(Value::as_str).map(str::trim)
}

fn write_document(path: &Path, document: &Map<String, Value>) -> Result<()> {
    let mut text = serde_json::to_string_pretty(&Value::Object(document.clone()))
        .context("cannot encode remote-hosts.json")?;
    text.push('\n');
    crate::subagent_file::write_atomic(path, text.as_bytes())
        .map_err(|err| anyhow::anyhow!("cannot write remote-hosts.json: {err}"))
}

fn upsert_at(path: &Path, original: Option<&str>, host: &RemoteHost) -> Result<()> {
    host.target()?;
    let name = host.name.trim();
    let mut document = read_document(path)?;
    let hosts = document
        .entry("hosts")
        .or_insert_with(|| Value::Array(Vec::new()));
    let Value::Array(entries) = hosts else {
        bail!("remote-hosts.json `hosts` is not a list; fix it by hand before saving");
    };
    let original = original.map(str::trim);
    let taken = entries
        .iter()
        .any(|entry| entry_name(entry) == Some(name) && Some(name) != original);
    if taken {
        bail!("a remote host named `{name}` already exists");
    }
    let existing = original.and_then(|original| {
        entries
            .iter()
            .position(|entry| entry_name(entry) == Some(original))
    });
    match existing {
        Some(ix) => {
            if !entries[ix].is_object() {
                entries[ix] = Value::Object(Map::new());
            }
            if let Value::Object(entry) = &mut entries[ix] {
                host.write_into(entry);
            }
        }
        None => {
            let mut entry = Map::new();
            host.write_into(&mut entry);
            entries.push(Value::Object(entry));
        }
    }
    write_document(path, &document)
}

fn remove_at(path: &Path, name: &str) -> Result<()> {
    let mut document = read_document(path)?;
    let Some(Value::Array(entries)) = document.get_mut("hosts") else {
        return Ok(());
    };
    let before = entries.len();
    entries.retain(|entry| entry_name(entry) != Some(name.trim()));
    if entries.len() == before {
        return Ok(());
    }
    write_document(path, &document)
}

/// Concrete `Host` aliases from `~/.ssh/config` (and the files it
/// `Include`s), in file order, for the Add host form's suggestions. Patterns
/// (`*`, `?`, `!negated`) are not hosts anyone can connect to by name, so they
/// are left out. A missing or unreadable file is no suggestions.
pub fn discover_ssh_aliases() -> Vec<String> {
    let ssh_dir = crate::platform::home_dir().join(".ssh");
    let mut aliases = Vec::new();
    let mut seen_files = Vec::new();
    collect_aliases(
        &ssh_dir.join("config"),
        &ssh_dir,
        &mut aliases,
        &mut seen_files,
    );
    aliases
}

fn collect_aliases(
    file: &Path,
    ssh_dir: &Path,
    aliases: &mut Vec<String>,
    seen_files: &mut Vec<PathBuf>,
) {
    // `Include` cycles are possible in hand-written configs; read each file once.
    if seen_files.iter().any(|seen| seen == file) || seen_files.len() > 32 {
        return;
    }
    seen_files.push(file.to_path_buf());
    let Ok(contents) = std::fs::read_to_string(file) else {
        return;
    };
    for (keyword, values) in contents.lines().filter_map(config_line) {
        match keyword.as_str() {
            "host" => {
                for alias in values {
                    let pattern = alias.contains(['*', '?']) || alias.starts_with('!');
                    if !pattern && !aliases.contains(&alias) {
                        aliases.push(alias);
                    }
                }
            }
            "include" => {
                for pattern in values {
                    for include in expand_include(&pattern, ssh_dir) {
                        collect_aliases(&include, ssh_dir, aliases, seen_files);
                    }
                }
            }
            _ => {}
        }
    }
}

/// `Keyword value value…` (or `Keyword=value`), lowercased keyword. Comments
/// and blank lines are `None`.
fn config_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (keyword, rest) = match line.find(|c: char| c.is_whitespace() || c == '=') {
        Some(ix) => (&line[..ix], &line[ix + 1..]),
        None => (line, ""),
    };
    let values = rest
        .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
        .split_whitespace()
        .map(|value| value.trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
        .collect();
    Some((keyword.to_ascii_lowercase(), values))
}

/// An `Include` argument: `~/` is the home directory, a relative path is
/// under `~/.ssh`, and a trailing `*` in the file name matches by prefix (the
/// common `config.d/*` shape). Other globs are not expanded.
fn expand_include(pattern: &str, ssh_dir: &Path) -> Vec<PathBuf> {
    let path = if let Some(rest) = pattern.strip_prefix("~/") {
        crate::platform::home_dir().join(rest)
    } else if Path::new(pattern).is_absolute() {
        PathBuf::from(pattern)
    } else {
        ssh_dir.join(pattern)
    };
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    if !name.contains('*') {
        return vec![path];
    }
    let (Some(dir), Some(prefix)) = (path.parent(), name.split('*').next()) else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|file| {
            file.is_file()
                && file
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(prefix))
        })
        .collect();
    files.sort();
    files
}

/// Why a connection check did not reach `pi`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeFailure {
    /// The server refused every key. `offered` says whether it accepts a
    /// password at all, read from the methods ssh listed.
    NeedsPassword { offered: bool },
    /// A password was tried and refused.
    WrongPassword,
    /// ssh would not trust the host key; the user verifies it in a terminal.
    HostKey(orbit_rpc::remote::HostKeyProblem),
    /// The key was accepted, but the account is locked to a forced command
    /// that only says which user to log in as — the cloud-image root guard
    /// (Oracle Cloud: `opc`, AWS: `ec2-user`, Ubuntu: `ubuntu`), which ends
    /// the connection with status 142 after `sleep 10`.
    LoginRefused {
        /// The account the server asks for, when its message names one.
        suggested: Option<String>,
        /// The server's own line.
        message: String,
    },
    /// Connected, but no `pi` on the login PATH or in the usual install
    /// places: it is not installed for this account.
    PiMissing,
    /// Anything else: the last line ssh or the host wrote.
    Other(String),
}

/// Where the askpass helper is written: `~/.orbit-pi/ssh-askpass/askpass.sh`,
/// in a directory only the user can enter.
fn askpass_file() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("ssh-askpass")
        .join("askpass.sh")
}

/// Write the askpass helper (only when its contents changed) and return its
/// path. The helper holds no secret: it prints the one ssh passes it.
#[cfg(unix)]
pub fn askpass_path() -> Result<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let path = askpass_file();
    let dir = path.parent().context("askpass helper has no directory")?;
    std::fs::create_dir_all(dir).context("cannot create the askpass directory")?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .context("cannot restrict the askpass directory")?;
    let current = std::fs::read_to_string(&path).ok();
    if current.as_deref() != Some(orbit_rpc::remote::ASKPASS_SCRIPT) {
        crate::subagent_file::write_atomic(&path, orbit_rpc::remote::ASKPASS_SCRIPT.as_bytes())
            .map_err(|err| anyhow::anyhow!("cannot write the askpass helper: {err}"))?;
    }
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))
        .context("cannot make the askpass helper executable")?;
    Ok(path)
}

/// Password logins need a POSIX askpass helper; on Windows only keys work.
#[cfg(not(unix))]
pub fn askpass_path() -> Result<PathBuf> {
    bail!("SSH password logins are not supported on Windows yet; use an SSH key")
}

/// Whether this build can ask for an SSH password at all.
pub fn passwords_supported() -> bool {
    cfg!(unix)
}

/// Run the read-only connection check for `host` (see
/// [`orbit_rpc::remote::probe_script`]). Blocking: call it off the UI thread.
/// With `password`, ssh may answer one password prompt through the askpass
/// helper; without one it never asks. `Ok` carries what `pi --version`
/// printed.
pub fn probe(
    host: &RemoteHost,
    password: Option<&str>,
) -> std::result::Result<String, ProbeFailure> {
    let target = host
        .target()
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let script = orbit_rpc::remote::probe_script(&target)
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    run_script(&target, &script, password, orbit_rpc::remote::PROBE_STARTED)
}

/// Install pi on `host` for its login account (see
/// [`orbit_rpc::remote::install_script`]): Node.js 22 from nodejs.org when the
/// host has none, checksum-verified, then pi's official installer, with no
/// terminal and no sudo. Blocking and slow (minutes): call it off the UI
/// thread. `Ok` carries the installed `pi --version`.
pub fn install(
    host: &RemoteHost,
    password: Option<&str>,
) -> std::result::Result<String, ProbeFailure> {
    let target = host
        .target()
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let script = orbit_rpc::remote::install_script();
    let started = format!("{}started", orbit_rpc::remote::INSTALL_STAGE_PREFIX);
    run_script(&target, &script, password, &started)
}

/// One row of `pi --list-models`: a model the machine can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedModel {
    pub provider: String,
    pub id: String,
    /// As pi prints them: `1M`, `128K`, `65.5K`.
    pub context: String,
    pub max_output: String,
    pub thinking: bool,
    pub images: bool,
}

/// Read `pi --list-models`' table: the header row, then one whitespace-
/// separated row per model. Anything before the header (Orbit's start marker,
/// stray output) and any row that does not have six columns is skipped, so a
/// changed or decorated table degrades to fewer rows, never a wrong one.
pub fn parse_model_list(stdout: &str) -> Vec<ListedModel> {
    let mut models = Vec::new();
    let mut in_table = false;
    for line in stdout.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if !in_table {
            in_table = words.first() == Some(&"provider") && words.get(1) == Some(&"model");
            continue;
        }
        let [provider, id, context, max_output, thinking, images] = words[..] else {
            continue;
        };
        models.push(ListedModel {
            provider: provider.to_string(),
            id: id.to_string(),
            context: context.to_string(),
            max_output: max_output.to_string(),
            thinking: thinking == "yes",
            images: images == "yes",
        });
    }
    models
}

/// Save `key` as `provider`'s API key in the host's `~/.pi/agent/auth.json`
/// (see [`orbit_rpc::remote::set_api_key_script`]). The key travels on ssh's
/// stdin only. Blocking: call it off the UI thread.
pub fn set_api_key(
    host: &RemoteHost,
    password: Option<&str>,
    provider: &str,
    key: &str,
) -> std::result::Result<(), ProbeFailure> {
    let target = host
        .target()
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let script = orbit_rpc::remote::set_api_key_script(provider)
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::KEY_STARTED,
        Some(&format!("{}\n", key.trim())),
    )
    .map(|_| ())
}

/// Record `mode` for the pi session `session_id` in `host`'s workflow store
/// (see [`orbit_rpc::remote::set_workflow_mode_script`]). Blocking: call it
/// off the UI thread.
pub fn set_workflow_mode(
    host: &RemoteHost,
    password: Option<&str>,
    session_id: &str,
    mode: &str,
) -> std::result::Result<(), ProbeFailure> {
    let other = |err: anyhow::Error| ProbeFailure::Other(format!("{err:#}"));
    let target = host.target().map_err(other)?;
    let script = orbit_rpc::remote::set_workflow_mode_script(session_id, mode).map_err(other)?;
    run_script(
        &target,
        &script,
        password,
        orbit_rpc::remote::WORKFLOW_STARTED,
    )
    .map(|_| ())
}

/// Clone `url` into `dest` (`~/…` or absolute) on `host` with the host's own
/// git (see [`orbit_rpc::remote::clone_script`]). Returns the script's stdout
/// for [`crate::server_clone::parse_clone`]; git's own failures are in it, so
/// an `Err` is always about reaching the host. Blocking.
pub fn clone_project(
    host: &RemoteHost,
    password: Option<&str>,
    url: &str,
    dest: &str,
) -> std::result::Result<String, ProbeFailure> {
    let other = |err: anyhow::Error| ProbeFailure::Other(format!("{err:#}"));
    let target = host.target().map_err(other)?;
    let script = orbit_rpc::remote::clone_script(url, dest).map_err(other)?;
    run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::CLONE_STARTED,
        None,
    )
}

/// Write the files in `payload` (see [`crate::server_clone::copy_payload`])
/// into `dest` on `host`; the contents travel on ssh's stdin only. Returns the
/// script's stdout for [`crate::server_clone::parse_copy`]. Blocking.
pub fn copy_files(
    host: &RemoteHost,
    password: Option<&str>,
    dest: &str,
    payload: &str,
) -> std::result::Result<String, ProbeFailure> {
    let other = |err: anyhow::Error| ProbeFailure::Other(format!("{err:#}"));
    let target = host.target().map_err(other)?;
    let script = orbit_rpc::remote::copy_files_script(dest).map_err(other)?;
    run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::COPY_STARTED,
        Some(payload),
    )
}

/// The newest sessions in `host`'s pi store (see
/// [`orbit_rpc::remote::list_sessions_script`]): one ssh run, read-only, only
/// row metadata. Blocking: call it off the UI thread.
pub fn list_sessions(
    host: &RemoteHost,
    password: Option<&str>,
) -> std::result::Result<Vec<crate::remote_sessions::RemoteSession>, ProbeFailure> {
    let target = host
        .target()
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let script = orbit_rpc::remote::list_sessions_script(crate::remote_sessions::LIST_LIMIT);
    let stdout = run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::LIST_SESSIONS_STARTED,
        None,
    )?;
    Ok(crate::remote_sessions::parse(&stdout))
}

/// Git facts about a host entry's folder, read over ssh by
/// [`git_status`]. Nothing here is fabricated: a field the host did not
/// answer is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteGit {
    /// `git` is not on the host's login PATH.
    Missing,
    /// The folder is not inside a Git work tree.
    NotARepository,
    Repository(RemoteGitRepo),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteGitRepo {
    /// The work tree's top level on the host.
    pub root: Option<String>,
    /// `None` on a detached HEAD.
    pub branch: Option<String>,
    /// The last commit: short hash and subject.
    pub head: Option<String>,
    pub upstream: Option<String>,
    /// Commits ahead of and behind the upstream, when there is one.
    pub ahead_behind: Option<(u32, u32)>,
    /// Changed paths in `git status --porcelain`.
    pub changed: Option<u32>,
    /// `origin`, with any password or token removed.
    pub origin: Option<String>,
}

/// Read [`orbit_rpc::remote::git_status_script`]'s output.
pub fn parse_git_status(stdout: &str) -> RemoteGit {
    let mut repo = RemoteGitRepo::default();
    let mut is_repo = false;
    for line in stdout.lines() {
        let Some((key, value)) = line.split_once(": ") else {
            continue;
        };
        let value = value.trim();
        let text = || Some(value.to_string()).filter(|v| !v.is_empty());
        match key.trim() {
            "git" if value == "missing" => return RemoteGit::Missing,
            "repo" if value == "no" => return RemoteGit::NotARepository,
            "repo" if value == "yes" => is_repo = true,
            "root" => repo.root = text(),
            "branch" => repo.branch = text(),
            "head" => repo.head = text(),
            "upstream" => repo.upstream = text(),
            "ahead_behind" => {
                let mut counts = value.split_whitespace().map(str::parse::<u32>);
                repo.ahead_behind = match (counts.next(), counts.next()) {
                    (Some(Ok(ahead)), Some(Ok(behind))) => Some((ahead, behind)),
                    _ => None,
                };
            }
            "changed" => repo.changed = value.parse().ok(),
            "origin" => {
                repo.origin = text().map(|url| crate::server_clone::strip_credentials(&url))
            }
            _ => {}
        }
    }
    if is_repo {
        RemoteGit::Repository(repo)
    } else {
        RemoteGit::NotARepository
    }
}

/// The local branches of `host`'s folder, newest commit first, as this
/// Mac's branch picker lists its own (`orbit_rpc::remote::list_branches_script`).
/// Empty when git is missing or the folder is not a repository. Read-only;
/// blocking: call it off the UI thread.
pub fn list_branches(
    host: &RemoteHost,
    password: Option<&str>,
) -> std::result::Result<Vec<crate::git::BranchEntry>, ProbeFailure> {
    let other = |err: anyhow::Error| ProbeFailure::Other(format!("{err:#}"));
    let target = host.target().map_err(other)?;
    let script = orbit_rpc::remote::list_branches_script(&target).map_err(other)?;
    let stdout = run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::BRANCHES_STARTED,
        None,
    )?;
    Ok(parse_branch_lines(&stdout))
}

/// The `name<TAB>date` lines after the script's start marker.
pub fn parse_branch_lines(stdout: &str) -> Vec<crate::git::BranchEntry> {
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty() && *line != orbit_rpc::remote::BRANCHES_STARTED)
        .map(crate::git::parse_branch_entry)
        .collect()
}

/// Switch `host`'s folder to branch `name`, or with `create` make it and
/// switch (`orbit_rpc::remote::switch_branch_script`). The outer `Err` is
/// about reaching the host; the inner one is git's own last line.
pub fn switch_branch(
    host: &RemoteHost,
    password: Option<&str>,
    name: &str,
    create: bool,
) -> std::result::Result<std::result::Result<(), String>, ProbeFailure> {
    let other = |err: anyhow::Error| ProbeFailure::Other(format!("{err:#}"));
    let target = host.target().map_err(other)?;
    let script = orbit_rpc::remote::switch_branch_script(&target, name, create).map_err(other)?;
    let stdout = run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::SWITCH_BRANCH_STARTED,
        None,
    )?;
    Ok(parse_switch_result(&stdout))
}

/// `Ok` when the script ended with `orbit-branch: ok`; otherwise git's last
/// line (the most specific) as the reason.
pub fn parse_switch_result(stdout: &str) -> std::result::Result<(), String> {
    if stdout.lines().any(|line| line == "orbit-branch: ok") {
        return Ok(());
    }
    let detail = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("orbit-branch: detail "))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    Err(detail
        .iter()
        .find(|line| line.starts_with("error:") || line.starts_with("fatal:"))
        .or(detail.last())
        .map(|line| line.to_string())
        .unwrap_or_else(|| tr!("remote_git.switch_failed_unknown")))
}

/// The Git state of `host`'s folder. Read-only; blocking: call it off the
/// UI thread.
pub fn git_status(
    host: &RemoteHost,
    password: Option<&str>,
) -> std::result::Result<RemoteGit, ProbeFailure> {
    let target = host
        .target()
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let script = orbit_rpc::remote::git_status_script(&target)
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let stdout = run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::GIT_STATUS_STARTED,
        None,
    )?;
    Ok(parse_git_status(&stdout))
}

/// The models `host`'s pi can use (see
/// [`orbit_rpc::remote::list_models_script`]), which is also the list of
/// providers signed in there. Blocking: call it off the UI thread.
pub fn list_models(
    host: &RemoteHost,
    password: Option<&str>,
) -> std::result::Result<Vec<ListedModel>, ProbeFailure> {
    let target = host
        .target()
        .map_err(|err| ProbeFailure::Other(format!("{err:#}")))?;
    let script = orbit_rpc::remote::list_models_script();
    let stdout = run_script_output(
        &target,
        &script,
        password,
        orbit_rpc::remote::LIST_MODELS_STARTED,
        None,
    )?;
    Ok(parse_model_list(&stdout))
}

/// Run `script` on the host with the session's ssh options and read the
/// result. `started` is what the script prints first, which tells a login
/// profile that ended the connection apart from a failure in the script.
fn run_script(
    target: &orbit_rpc::remote::RemoteTarget,
    script: &str,
    password: Option<&str>,
    started: &str,
) -> std::result::Result<String, ProbeFailure> {
    let stdout = run_script_output(target, script, password, started, None)?;
    Ok(stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string())
}

/// [`run_script`], keeping the whole of stdout.
fn run_script_output(
    target: &orbit_rpc::remote::RemoteTarget,
    script: &str,
    password: Option<&str>,
    started: &str,
    input: Option<&str>,
) -> std::result::Result<String, ProbeFailure> {
    let other = |err: anyhow::Error| ProbeFailure::Other(format!("{err:#}"));
    let askpass = match password {
        Some(_) => Some(askpass_path().map_err(other)?),
        None => None,
    };
    let args = orbit_rpc::remote::ssh_args(target, script, password.is_some());
    let mut command = std::process::Command::new("ssh");
    command
        .args(&args)
        .stdin(if input.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let (Some(askpass), Some(secret)) = (&askpass, password) {
        orbit_rpc::remote::apply_password(
            &mut command,
            orbit_rpc::remote::SshPassword { askpass, secret },
        );
    }
    orbit_rpc::hide_console(&mut command);
    let mut child = command
        .spawn()
        .map_err(|err| ProbeFailure::Other(format!("cannot run ssh: {err}")))?;
    // The input (an API key) goes through ssh's stdin and is then closed, so
    // the remote side reads it to the end. A write failure surfaces as the
    // remote command's own error.
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        use std::io::Write;
        let _ = stdin.write_all(input.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|err| ProbeFailure::Other(format!("cannot run ssh: {err}")))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Err(classify_failure(
        &stderr,
        &stdout,
        started,
        password.is_some(),
        output.status.code(),
    ))
}

/// The cloud-image root guard: a forced command in `authorized_keys` that
/// prints `Please login as the user "opc" rather than the user "root".` and
/// exits. Returns the account it names (when it is a plain user name) and the
/// line itself.
pub fn login_refusal(stdout: &str) -> Option<(Option<String>, String)> {
    let line = stdout.lines().map(str::trim).find(|line| {
        let lower = line.to_lowercase();
        lower.contains("login as the user") || lower.contains("rather than the user")
    })?;
    let suggested = line
        .to_lowercase()
        .find("as the user")
        .map(|ix| &line[ix + "as the user".len()..])
        .and_then(|rest| {
            let rest = rest.trim_start();
            let quote = rest
                .chars()
                .next()
                .filter(|c| matches!(c, '"' | '\'' | '`'))?;
            let name = rest[quote.len_utf8()..].split(quote).next()?;
            let plain = !name.is_empty()
                && !name.starts_with('-')
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
            plain.then(|| name.to_string())
        });
    Some((suggested, line.to_string()))
}

/// ssh's own failures (it could not connect or authenticate) exit 255; any
/// other status is the remote command's.
const SSH_ERROR_EXIT: i32 = 255;

/// Read a failed run's output into a [`ProbeFailure`].
fn classify_failure(
    stderr: &str,
    stdout: &str,
    started: &str,
    with_password: bool,
    code: Option<i32>,
) -> ProbeFailure {
    if let Some(problem) = orbit_rpc::remote::host_key_problem(stderr) {
        return ProbeFailure::HostKey(problem);
    }
    if orbit_rpc::remote::is_auth_failure(stderr) {
        return if with_password {
            ProbeFailure::WrongPassword
        } else {
            ProbeFailure::NeedsPassword {
                offered: orbit_rpc::remote::offers_password(stderr),
            }
        };
    }
    if let Some((suggested, message)) = login_refusal(stdout) {
        return ProbeFailure::LoginRefused { suggested, message };
    }
    if code == Some(orbit_rpc::remote::PI_MISSING_EXIT)
        && stderr.contains(orbit_rpc::remote::PI_MISSING_MESSAGE)
    {
        return ProbeFailure::PiMissing;
    }
    let last = stderr
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or_default()
        .to_string();
    // The connection worked but Orbit's script never started: the host's
    // login profile (`sh -l`) ended it. A bare status used to be all the user
    // saw (142 is a shell or a program stopped by SIGALRM).
    if let Some(status) = code.filter(|code| *code != SSH_ERROR_EXIT) {
        if last.is_empty() && !stdout.contains(started) {
            return ProbeFailure::Other(match orbit_rpc::remote::signal_name(status) {
                Some(signal) => tr!(
                    "remote_hosts.profile_exited_signal",
                    status = status,
                    signal = signal
                ),
                None => tr!("remote_hosts.profile_exited", status = status),
            });
        }
    }
    if !last.is_empty() {
        // The script's own "pi --version (…) failed with status N" names a
        // status; say which signal it was when it is one.
        let signal = last
            .rsplit_once("failed with status ")
            .and_then(|(_, rest)| rest.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|digits| digits.parse::<i32>().ok())
            .and_then(orbit_rpc::remote::signal_name);
        return ProbeFailure::Other(match signal {
            Some(signal) => tr!("remote_hosts.with_signal", message = last, signal = signal),
            None => last,
        });
    }
    ProbeFailure::Other(match code {
        Some(status) => match orbit_rpc::remote::signal_name(status) {
            Some(signal) => tr!(
                "remote_hosts.ssh_exit_signal",
                status = status,
                signal = signal
            ),
            None => tr!("remote_hosts.ssh_exit", status = status),
        },
        None => tr!("remote_hosts.ssh_signal"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str) -> RemoteHost {
        RemoteHost {
            name: name.into(),
            ssh: format!("{name}.example"),
            user: None,
            port: None,
            dir: "/srv/app".into(),
            enabled: true,
            local_dir: None,
        }
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "orbit-remote-hosts-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn parses_valid_hosts_and_skips_bad_ones_with_reasons() {
        let (hosts, skipped) = parse(
            r#"{"hosts":[
                {"name":"vps","ssh":"vps","dir":"/srv/app"},
                {"name":"bad-dir","ssh":"vps","dir":"relative/path"},
                {"name":"bad-host","ssh":"-oProxyCommand=x","dir":"/srv"},
                {"name":"port","ssh":"h","user":"deploy","port":2222,"dir":"/x"}
            ]}"#,
        );
        assert_eq!(hosts.len(), 2);
        assert_eq!(hosts[0].name, "vps");
        assert_eq!(hosts[1].port, Some(2222));
        assert_eq!(hosts[1].destination(), "deploy@h");
        assert_eq!(hosts[1].connection_label(), "deploy@h:2222");
        assert_eq!(skipped.len(), 2);
        assert!(skipped[0].contains("entry 2"));
        assert!(skipped[1].contains("entry 3"));
    }

    #[test]
    fn missing_hosts_key_is_an_empty_list_and_bad_json_is_reported() {
        assert_eq!(parse("{}"), (Vec::new(), Vec::new()));
        let (hosts, skipped) = parse("{not json");
        assert!(hosts.is_empty());
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn a_user_must_be_a_plain_name() {
        let mut bad = host("a");
        bad.user = Some("-oProxyCommand=x".into());
        assert!(bad.target().is_err());
        bad.user = Some("a@b".into());
        assert!(bad.target().is_err());
        bad.user = Some("deploy".into());
        assert!(bad.target().is_ok());
    }

    #[test]
    fn upsert_adds_edits_and_keeps_keys_orbit_does_not_own() {
        let dir = TempDir::new("upsert");
        let path = dir.0.join("remote-hosts.json");
        std::fs::write(
            &path,
            r#"{"example":{"keep":true},"hosts":[{"name":"a","ssh":"a","dir":"/x","note":"mine"}]}"#,
        )
        .unwrap();

        let mut edited = host("a2");
        edited.user = Some("deploy".into());
        edited.port = Some(2200);
        edited.local_dir = Some("~/code/app".into());
        upsert_at(&path, Some("a"), &edited).expect("rename");
        upsert_at(&path, None, &host("b")).expect("add");

        let document: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(document["example"]["keep"], Value::Bool(true));
        let entries = document["hosts"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0]["name"], "a2");
        assert_eq!(
            entries[0]["note"], "mine",
            "a foreign key survives the edit"
        );
        assert_eq!(entries[0]["user"], "deploy");
        assert_eq!(entries[0]["port"], 2200);
        assert_eq!(entries[0]["local_dir"], "~/code/app");
        assert_eq!(entries[1]["name"], "b");
        assert!(
            entries[1].get("local_dir").is_none(),
            "unlinked writes no key"
        );

        // Clearing the optional fields removes them instead of writing null.
        upsert_at(&path, Some("a2"), &host("a2")).unwrap();
        let document: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(document["hosts"][0].get("user").is_none());
        assert!(document["hosts"][0].get("port").is_none());
        assert!(document["hosts"][0].get("local_dir").is_none());

        let (hosts, skipped) = load_from(&path);
        assert!(skipped.is_empty());
        assert_eq!(hosts.len(), 2);
        let linked = RemoteHost {
            local_dir: Some("~/code/app".into()),
            ..host("x")
        };
        assert_eq!(
            linked.local_path(),
            Some(crate::platform::home_dir().join("code/app")),
            "~/ is the home folder"
        );
    }

    #[test]
    fn git_status_lines_become_facts() {
        let stdout = concat!(
            "orbit-git: started\n",
            "repo: yes\n",
            "root: /srv/app\n",
            "branch: main\n",
            "head: abc1234 fix: things\n",
            "upstream: origin/main\n",
            "ahead_behind: 2\t1\n",
            "changed: 3\n",
            "origin: https://user:secret@github.com/me/app.git\n",
        );
        let RemoteGit::Repository(repo) = parse_git_status(stdout) else {
            panic!("a repository");
        };
        assert_eq!(repo.root.as_deref(), Some("/srv/app"));
        assert_eq!(repo.branch.as_deref(), Some("main"));
        assert_eq!(repo.head.as_deref(), Some("abc1234 fix: things"));
        assert_eq!(repo.ahead_behind, Some((2, 1)));
        assert_eq!(repo.changed, Some(3));
        assert!(
            !repo
                .origin
                .as_deref()
                .unwrap_or_default()
                .contains("secret"),
            "credentials never reach the UI"
        );

        let detached = parse_git_status("repo: yes\nbranch: \nupstream: \nahead_behind: \n");
        assert_eq!(
            detached,
            RemoteGit::Repository(RemoteGitRepo::default()),
            "an unanswered field stays unknown"
        );
        assert_eq!(parse_git_status("repo: no\n"), RemoteGit::NotARepository);
        assert_eq!(parse_git_status("git: missing\n"), RemoteGit::Missing);
    }

    #[test]
    fn enabled_defaults_on_and_is_written_only_when_off() {
        let (hosts, _) = parse(r#"{"hosts":[{"name":"a","ssh":"a","dir":"/x"}]}"#);
        assert!(hosts[0].enabled);
        let dir = TempDir::new("enabled");
        let path = dir.0.join("remote-hosts.json");
        let mut off = host("a");
        off.enabled = false;
        upsert_at(&path, None, &off).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"enabled\": false"), "{text}");
        upsert_at(&path, Some("a"), &host("a")).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("enabled"));
    }

    #[test]
    fn upsert_refuses_a_duplicate_name_and_an_unreadable_file() {
        let dir = TempDir::new("dup");
        let path = dir.0.join("remote-hosts.json");
        upsert_at(&path, None, &host("a")).expect("first write creates the file");
        assert!(
            upsert_at(&path, None, &host("a")).is_err(),
            "names are unique"
        );
        upsert_at(&path, None, &host("b")).unwrap();
        assert!(
            upsert_at(&path, Some("b"), &host("a")).is_err(),
            "rename onto a taken name"
        );

        std::fs::write(&path, "{not json").unwrap();
        assert!(upsert_at(&path, None, &host("c")).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{not json",
            "never overwritten"
        );
    }

    #[test]
    fn remove_drops_only_the_named_host() {
        let dir = TempDir::new("remove");
        let path = dir.0.join("remote-hosts.json");
        upsert_at(&path, None, &host("a")).unwrap();
        upsert_at(&path, None, &host("b")).unwrap();
        remove_at(&path, "a").unwrap();
        remove_at(&path, "missing").unwrap();
        let (hosts, _) = load_from(&path);
        assert_eq!(
            hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["b"]
        );
    }

    #[test]
    fn failures_are_classified_from_stderr() {
        use orbit_rpc::remote::HostKeyProblem;
        let denied = "deploy@vps: Permission denied (publickey,password).";
        assert_eq!(
            classify_failure(
                denied,
                "",
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(255)
            ),
            ProbeFailure::NeedsPassword { offered: true }
        );
        assert_eq!(
            classify_failure(
                "deploy@vps: Permission denied (publickey).",
                "",
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(255)
            ),
            ProbeFailure::NeedsPassword { offered: false }
        );
        assert_eq!(
            classify_failure(
                denied,
                "",
                orbit_rpc::remote::PROBE_STARTED,
                true,
                Some(255)
            ),
            ProbeFailure::WrongPassword
        );
        assert_eq!(
            classify_failure(
                "Host key verification failed.",
                "",
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(255)
            ),
            ProbeFailure::HostKey(HostKeyProblem::Unknown)
        );
        let started = format!("{}\n", orbit_rpc::remote::PROBE_STARTED);
        assert_eq!(
            classify_failure(
                "orbit: pi is not on the PATH\n",
                &started,
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(127)
            ),
            ProbeFailure::Other("orbit: pi is not on the PATH".into())
        );
        assert_eq!(
            classify_failure(
                "",
                &started,
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(66)
            ),
            ProbeFailure::Other(tr!("remote_hosts.ssh_exit", status = 66))
        );
    }

    #[test]
    fn the_cloud_root_guard_is_recognised_with_the_account_it_asks_for() {
        // Oracle Cloud's /root/.ssh/authorized_keys forced command.
        let oracle = "Please login as the user \"opc\" rather than the user \"root\".\n\n";
        assert_eq!(
            classify_failure(
                "",
                oracle,
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(142)
            ),
            ProbeFailure::LoginRefused {
                suggested: Some("opc".into()),
                message: "Please login as the user \"opc\" rather than the user \"root\".".into(),
            }
        );
        // AWS and Ubuntu cloud images word it the same way.
        assert_eq!(
            login_refusal("Please login as the user \"ec2-user\" rather than the user \"root\".")
                .and_then(|(user, _)| user),
            Some("ec2-user".into())
        );
        // A name that is not a plain user name is not offered.
        assert_eq!(
            login_refusal("Please login as the user \"-oProxy=x\" rather than the user \"root\".")
                .map(|(user, _)| user),
            Some(None)
        );
        assert!(login_refusal("orbit-probe: started\npi 1.5.2").is_none());
    }

    #[test]
    fn status_142_is_explained_by_where_it_happened() {
        let started = format!("{}\n", orbit_rpc::remote::PROBE_STARTED);
        // The login profile ended the connection: Orbit's script never started.
        assert_eq!(
            classify_failure("", "", orbit_rpc::remote::PROBE_STARTED, false, Some(142)),
            ProbeFailure::Other(tr!(
                "remote_hosts.profile_exited_signal",
                status = 142,
                signal = "SIGALRM"
            ))
        );
        // pi itself was stopped: the script's line, with the signal named.
        let line = "orbit: pi --version (/usr/bin/pi) failed with status 142: ";
        let ProbeFailure::Other(message) = classify_failure(
            line,
            &started,
            orbit_rpc::remote::PROBE_STARTED,
            false,
            Some(70),
        ) else {
            panic!("a message");
        };
        assert!(
            message.contains("/usr/bin/pi") && message.contains("SIGALRM"),
            "{message}"
        );
        // After Orbit's script started, a bare 142 still names its signal.
        assert_eq!(
            classify_failure(
                "",
                &started,
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(142)
            ),
            ProbeFailure::Other(tr!(
                "remote_hosts.ssh_exit_signal",
                status = 142,
                signal = "SIGALRM"
            ))
        );
        // ssh's own failures (255) are never blamed on the profile.
        assert_eq!(
            classify_failure("", "", orbit_rpc::remote::PROBE_STARTED, false, Some(255)),
            ProbeFailure::Other(tr!("remote_hosts.ssh_exit", status = 255))
        );
    }

    #[test]
    fn a_missing_pi_is_its_own_failure_with_an_install_command() {
        let stderr = format!("{}\n", orbit_rpc::remote::PI_MISSING_MESSAGE);
        let started = format!("{}\n", orbit_rpc::remote::PROBE_STARTED);
        assert_eq!(
            classify_failure(
                &stderr,
                &started,
                orbit_rpc::remote::PROBE_STARTED,
                false,
                Some(127)
            ),
            ProbeFailure::PiMissing
        );
        let mut h = host("vps");
        h.user = Some("ubuntu".into());
        assert_eq!(
            h.install_command(),
            "ssh -t ubuntu@vps.example 'curl -fsSL https://pi.dev/install.sh | sh'"
        );
        h.port = Some(2222);
        assert!(h
            .install_command()
            .starts_with("ssh -t -p 2222 ubuntu@vps.example "));
    }

    #[test]
    fn the_model_table_is_read_and_anything_else_skipped() {
        let stdout = "orbit-models: started\n\
provider        model                                             context  max-out  thinking  images\n\
amazon-bedrock  amazon.nova-2-lite-v1:0                           1M       65.5K    yes       yes   \n\
anthropic       claude-fable-5                                    1M       128K     yes       no    \n\
a stray line that is not a row\n";
        let models = parse_model_list(stdout);
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].provider, "amazon-bedrock");
        assert_eq!(models[0].id, "amazon.nova-2-lite-v1:0");
        assert_eq!(models[0].context, "1M");
        assert_eq!(models[0].max_output, "65.5K");
        assert!(models[0].thinking && models[0].images);
        assert!(models[1].thinking && !models[1].images);
        assert!(parse_model_list("no table here").is_empty());
    }

    #[test]
    fn the_account_key_ignores_the_name_and_directory() {
        let mut a = host("a");
        let mut b = host("b");
        b.ssh = a.ssh.clone();
        b.dir = "/other".into();
        assert_eq!(a.account_key(), b.account_key());
        a.port = Some(2222);
        assert_ne!(a.account_key(), b.account_key());
    }

    #[test]
    fn ssh_config_aliases_skip_patterns_and_follow_includes() {
        let dir = TempDir::new("ssh");
        let ssh = dir.0.clone();
        std::fs::create_dir_all(ssh.join("config.d")).unwrap();
        std::fs::write(
            ssh.join("config"),
            "# comment\nHost prod staging\n  HostName 10.0.0.1\nHost *.internal !bastion\nHost=devbox\nInclude config.d/*\nInclude config\n",
        )
        .unwrap();
        std::fs::write(ssh.join("config.d/work"), "Host work-vm\nHost prod\n").unwrap();
        let mut aliases = Vec::new();
        let mut seen = Vec::new();
        collect_aliases(&ssh.join("config"), &ssh, &mut aliases, &mut seen);
        assert_eq!(aliases, ["prod", "staging", "devbox", "work-vm"]);
    }
}
