//! SSH key discovery for Settings → Git.
//!
//! Settings lists the machine's SSH identities next to its `gh` accounts so
//! the page answers "what can authenticate my Git?" in one place. Discovery is
//! read-only: it reads `~/.ssh/*.pub`, asks `ssh-keygen` for the fingerprint,
//! asks `ssh-add` which keys the agent is holding, and reads `~/.ssh/config`
//! for the hosts each key is bound to. Nothing here writes a key or touches a
//! passphrase.
//!
//! `ssh-keygen` / `ssh-add` are optional: when they are missing the key still
//! lists, minus its fingerprint and agent status.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::onboarding;
use crate::worktree;

/// One SSH keypair found under `~/.ssh`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKey {
    /// The key's file stem, e.g. `id_ed25519`.
    pub name: String,
    pub private_path: PathBuf,
    pub public_path: PathBuf,
    /// The key algorithm, e.g. `ssh-ed25519`.
    pub key_type: String,
    /// The public key's comment, usually an email or `user@host`.
    pub comment: String,
    /// `SHA256:…`, when `ssh-keygen` is available.
    pub fingerprint: Option<String>,
    /// Whether `ssh-add -l` reports the key as loaded in the agent.
    pub loaded: bool,
    /// `Host` aliases from `~/.ssh/config` whose `IdentityFile` is this key.
    pub hosts: Vec<String>,
}

/// `~/.ssh`, when it exists.
pub fn ssh_dir() -> Option<PathBuf> {
    let dir = crate::platform::home_dir().join(".ssh");
    dir.is_dir().then_some(dir)
}

/// Every public key under `~/.ssh`, sorted by name. Keys without a `.pub`
/// sibling are skipped: a `.pub` is what a key comment and fingerprint are
/// read from without ever touching (or prompting for) the private half.
pub fn list_keys() -> Vec<SshKey> {
    let Some(dir) = ssh_dir() else {
        return Vec::new();
    };
    let Ok(read) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();

    let loaded = agent_fingerprints();
    let host_map = std::fs::read_to_string(dir.join("config"))
        .map(|raw| config_hosts(&raw, &dir))
        .unwrap_or_default();

    let mut keys = Vec::new();
    for name in &names {
        let Some(stem) = name.strip_suffix(".pub") else {
            continue;
        };
        let public_path = dir.join(name);
        let Ok(contents) = std::fs::read_to_string(&public_path) else {
            continue;
        };
        let Some((key_type, comment)) = parse_public_key(&contents) else {
            continue;
        };
        let fingerprint = fingerprint_of(&public_path);
        let private_path = dir.join(stem);
        let hosts = host_map.get(&private_path).cloned().unwrap_or_default();
        keys.push(SshKey {
            name: stem.to_string(),
            private_path,
            public_path,
            key_type,
            comment,
            loaded: fingerprint
                .as_deref()
                .is_some_and(|fp| loaded.iter().any(|loaded| loaded == fp)),
            fingerprint,
            hosts,
        });
    }
    keys
}

/// Parse an OpenSSH public-key line into `(type, comment)`. The base64 blob is
/// ignored. Returns `None` for a file that is not a key (a stray `.pub`).
pub fn parse_public_key(contents: &str) -> Option<(String, String)> {
    let line = contents.lines().find(|line| !line.trim().is_empty())?;
    let mut parts = line.split_whitespace();
    let key_type = parts.next()?.to_string();
    if !looks_like_key_type(&key_type) {
        return None;
    }
    let _blob = parts.next()?;
    let comment = parts.collect::<Vec<_>>().join(" ");
    Some((key_type, comment))
}

fn looks_like_key_type(key_type: &str) -> bool {
    key_type.starts_with("ssh-")
        || key_type.starts_with("ecdsa-")
        || key_type.starts_with("sk-")
        || key_type.starts_with("rsa-")
        || key_type == "ssh-rsa"
}

/// The `SHA256:…` fingerprint `ssh-keygen` prints, when it is installed.
fn fingerprint_of(path: &Path) -> Option<String> {
    let output = keygen_command()
        .args(["-l", "-f"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .find(|token| token.starts_with("SHA256:"))
        .map(str::to_string)
}

/// The fingerprints `ssh-add -l` reports. Empty when no agent is running.
fn agent_fingerprints() -> Vec<String> {
    let Ok(output) = Command::new("ssh-add").arg("-l").output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter(|token| token.starts_with("SHA256:"))
        .map(str::to_string)
        .collect()
}

/// Map each `IdentityFile` in an `~/.ssh/config` to the `Host` aliases it is
/// listed under. A `Host *` block is skipped so unscoped defaults do not label
/// every key. `base` resolves relative and `~` paths (the SSH directory).
pub fn config_hosts(raw: &str, base: &Path) -> HashMap<PathBuf, Vec<String>> {
    let mut map: HashMap<PathBuf, Vec<String>> = HashMap::new();
    let mut current: Vec<String> = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some(keyword) if keyword.eq_ignore_ascii_case("Host") => {
                current = parts
                    .filter(|host| *host != "*")
                    .map(str::to_string)
                    .collect();
            }
            Some(keyword) if keyword.eq_ignore_ascii_case("IdentityFile") => {
                if let Some(value) = parts.next() {
                    if current.is_empty() {
                        continue;
                    }
                    let path = worktree::expand_path(base, value);
                    let hosts = map.entry(path).or_default();
                    for host in &current {
                        if !hosts.contains(host) {
                            hosts.push(host.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    map
}

fn keygen_command() -> Command {
    let mut command = match onboarding::locate("ssh-keygen") {
        Some(path) => Command::new(path),
        None => Command::new("ssh-keygen"),
    };
    orbit_rpc::hide_console(&mut command);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_public_keys_and_rejects_other_files() {
        assert_eq!(
            parse_public_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI ada@example.com\n"),
            Some(("ssh-ed25519".to_string(), "ada@example.com".to_string()))
        );
        assert_eq!(
            parse_public_key("ssh-rsa AAAAB3NzaC1yc2E user@host"),
            Some(("ssh-rsa".to_string(), "user@host".to_string()))
        );
        // A comment with spaces stays one string.
        assert_eq!(
            parse_public_key("ecdsa-sha2-nistp256 AAAA work laptop key"),
            Some((
                "ecdsa-sha2-nistp256".to_string(),
                "work laptop key".to_string()
            ))
        );
        // Not a key at all.
        assert_eq!(parse_public_key("github.com ssh-ed25519 AAAA"), None);
        assert_eq!(parse_public_key(""), None);
    }

    #[test]
    fn maps_identity_files_to_hosts() {
        let base = Path::new("/home/u/.ssh");
        let raw = "\
Host github.com
  HostName github.com
  User git
  IdentityFile ~/.ssh/id_ed25519
  IdentityFile ~/.ssh/id_work

Host work-github
  HostName github.com
  IdentityFile ~/.ssh/id_work

Host *
  IdentityFile ~/.ssh/id_global
";
        let map = config_hosts(raw, base);
        let home = crate::platform::home_dir();
        let ed = home.join(".ssh").join("id_ed25519");
        let work = home.join(".ssh").join("id_work");
        assert_eq!(map.get(&ed), Some(&vec!["github.com".to_string()]));
        assert_eq!(
            map.get(&work),
            Some(&vec!["github.com".to_string(), "work-github".to_string()])
        );
        // `Host *` is skipped.
        assert!(!map.contains_key(&home.join(".ssh").join("id_global")));
    }
}
