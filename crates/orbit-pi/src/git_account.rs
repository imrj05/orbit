//! Commit identity and GitHub CLI account management for Settings → Git.
//!
//! The commit identity is read and written through `git config` — `--global`
//! for the default, `--local` for a per-repository override. GitHub sign-in
//! stays with the `gh` CLI: Orbit pipes a pasted token to
//! `gh auth login --with-token` and forgets it, and lists / switches / removes
//! accounts through `gh auth` — never storing a token itself (see `gh.rs` and
//! INTENT.md D7).
//!
//! Everything here is blocking process I/O and runs on the background
//! executor; the Settings page paints cached state.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

use crate::gh;

/// The oldest `gh` that has `gh auth switch`.
pub const MIN_SWITCH_VERSION: (u32, u32, u32) = (2, 40, 0);

/// The global commit identity (`git config --global user.name` / `user.email`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

/// One account `gh auth status` reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhAccount {
    pub login: String,
    /// The GitHub host the account belongs to (usually `github.com`).
    pub host: String,
    /// `gh`'s active account for its host (`- Active account: true`).
    pub active: bool,
}

/// Read the global commit identity. A missing key reads as an empty string and
/// is never an error — a fresh machine has no identity yet.
pub fn read_identity() -> GitIdentity {
    GitIdentity {
        name: config_get(Scope::Global, None, "user.name"),
        email: config_get(Scope::Global, None, "user.email"),
    }
}

/// Write the global commit identity. An empty field unsets the key.
pub fn write_identity(identity: &GitIdentity) -> Result<(), String> {
    write_identity_in(Scope::Global, None, identity)
}

/// Read the repository-local identity override (`git config --local`). Empty
/// fields mean the repository inherits the global identity.
pub fn read_local_identity(repo: &Path) -> GitIdentity {
    GitIdentity {
        name: config_get(Scope::Local, Some(repo), "user.name"),
        email: config_get(Scope::Local, Some(repo), "user.email"),
    }
}

/// Write the repository-local identity override. An empty field unsets the
/// local key so the global value applies again.
pub fn write_local_identity(repo: &Path, identity: &GitIdentity) -> Result<(), String> {
    write_identity_in(Scope::Local, Some(repo), identity)
}

/// Remove both repository-local overrides.
pub fn clear_local_identity(repo: &Path) -> Result<(), String> {
    set_config(Scope::Local, Some(repo), "user.name", "")?;
    set_config(Scope::Local, Some(repo), "user.email", "")?;
    Ok(())
}

// ── Saved Git identities ────────────────────────────────────────────────

/// How a saved identity authenticates pushes and pulls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AuthMethod {
    /// Whatever Git on this machine already uses (default).
    #[default]
    Machine,
    /// A signed-in `gh` account, applied through `credential.username`.
    Account,
    /// A specific key from `~/.ssh`, applied through `core.sshCommand`.
    Ssh,
    /// No credentials; only public remotes work.
    Anonymous,
}

/// The identity color swatches offered in the editor: name and RGB hex.
pub const ACCOUNT_COLORS: &[(&str, u32)] = &[
    ("green", 0x2ea043),
    ("red", 0xf85149),
    ("peach", 0xffa198),
    ("lime", 0x7ee787),
    ("blue", 0x58a6ff),
    ("purple", 0xbc8cff),
];

/// The identity icon choices offered in the editor: id and bundled asset.
pub const ACCOUNT_ICONS: &[(&str, &str)] = &[
    ("git", "icons/git-merge.svg"),
    ("folder", "icons/folder.svg"),
    ("terminal", "icons/terminal.svg"),
    ("monitor", "icons/monitor.svg"),
    ("cloud", "icons/cloud.svg"),
    ("rocket", "icons/rocket-01.svg"),
    ("star", "icons/star.svg"),
];

/// The RGB value for a stored color name (falls back to the first swatch).
pub fn color_value(name: &str) -> u32 {
    ACCOUNT_COLORS
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .or_else(|| ACCOUNT_COLORS.first())
        .map(|(_, value)| *value)
        .unwrap_or(0x2ea043)
}

/// The asset path for a stored icon id (falls back to the first icon).
pub fn icon_path(id: &str) -> &'static str {
    ACCOUNT_ICONS
        .iter()
        .find(|(candidate, _)| *candidate == id)
        .or_else(|| ACCOUNT_ICONS.first())
        .map(|(_, path)| *path)
        .unwrap_or("icons/git-merge.svg")
}

/// A saved Git identity profile: the commit identity plus how it authenticates.
/// Applying one writes only the current repository's `.git/config`; nothing
/// global changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GitAccount {
    pub label: String,
    /// A color name from [`ACCOUNT_COLORS`].
    pub color: String,
    /// An icon id from [`ACCOUNT_ICONS`].
    pub icon: String,
    pub name: String,
    pub email: String,
    /// A signed-in `gh` login, used when `auth_method` is `Account`.
    pub source_account: String,
    pub auth_method: AuthMethod,
    /// Path to a key from `~/.ssh`, used when `auth_method` is `Ssh`.
    pub ssh_key: String,
    pub sign_commits: bool,
    /// A GPG/SSH signing key (id or path) when `sign_commits` is set.
    pub signing_key: String,
}

impl Default for GitAccount {
    fn default() -> Self {
        Self {
            label: String::new(),
            color: ACCOUNT_COLORS[0].0.to_string(),
            icon: ACCOUNT_ICONS[0].0.to_string(),
            name: String::new(),
            email: String::new(),
            source_account: String::new(),
            auth_method: AuthMethod::Machine,
            ssh_key: String::new(),
            sign_commits: false,
            signing_key: String::new(),
        }
    }
}

/// The saved Git identities, persisted to `~/.orbit-pi/git-accounts.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GitAccountsConfig {
    pub accounts: Vec<GitAccount>,
}

impl GitAccountsConfig {
    /// `~/.orbit-pi/git-accounts.json`.
    pub fn store_path() -> PathBuf {
        crate::platform::home_dir()
            .join(".orbit-pi")
            .join("git-accounts.json")
    }

    /// Load the saved identities, falling back to empty on any error.
    pub fn load() -> Self {
        let Ok(raw) = std::fs::read_to_string(Self::store_path()) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    /// Persist the identities, creating `~/.orbit-pi/` when needed.
    pub fn persist(&self) -> std::io::Result<()> {
        let path = Self::store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_string_pretty(self)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        std::fs::write(path, payload)
    }
}

/// Apply an identity to `repo` through `git config --local`.
///
/// The commit identity is always written. `auth_method` then decides the
/// transport: `Machine` clears the overrides so the machine's own setup wins,
/// `Ssh` pins `core.sshCommand` to the chosen key (`IdentitiesOnly=yes`),
/// `Account` sets `credential.username`, and `Anonymous` clears the
/// credentials. Signing writes `commit.gpgsign` and `user.signingkey`.
///
/// Local scope keeps the choice to this repository; the global identity and the
/// user's `~/.ssh/config` are left untouched.
pub fn apply_to_repo(repo: &Path, account: &GitAccount) -> Result<(), String> {
    use AuthMethod::*;

    set_config(Scope::Local, Some(repo), "user.name", &account.name)?;
    set_config(Scope::Local, Some(repo), "user.email", &account.email)?;

    match account.auth_method {
        Ssh if !account.ssh_key.trim().is_empty() => {
            let key = expand_key(&account.ssh_key);
            let command = format!("ssh -i {} -o IdentitiesOnly=yes", shell_quote(&key));
            set_config(Scope::Local, Some(repo), "core.sshCommand", &command)?;
            set_config(Scope::Local, Some(repo), "credential.username", "")?;
        }
        Account => {
            set_config(Scope::Local, Some(repo), "core.sshCommand", "")?;
            set_config(
                Scope::Local,
                Some(repo),
                "credential.username",
                &account.source_account,
            )?;
        }
        // Machine, Anonymous, and Ssh-without-a-key all defer the transport to
        // whatever the machine already has (or nothing, for public remotes).
        _ => {
            set_config(Scope::Local, Some(repo), "core.sshCommand", "")?;
            set_config(Scope::Local, Some(repo), "credential.username", "")?;
        }
    }

    if account.sign_commits && !account.signing_key.trim().is_empty() {
        set_config(Scope::Local, Some(repo), "commit.gpgsign", "true")?;
        set_config(
            Scope::Local,
            Some(repo),
            "user.signingkey",
            &account.signing_key,
        )?;
    } else {
        set_config(Scope::Local, Some(repo), "commit.gpgsign", "")?;
        set_config(Scope::Local, Some(repo), "user.signingkey", "")?;
    }
    Ok(())
}

/// Expand a leading `~` in a stored key path.
fn expand_key(raw: &str) -> String {
    let raw = raw.trim();
    if raw == "~" {
        return crate::platform::home_dir().to_string_lossy().into_owned();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return crate::platform::home_dir()
            .join(rest)
            .to_string_lossy()
            .into_owned();
    }
    raw.to_string()
}

/// Quote a path for the shell `git` uses to run `core.sshCommand`. A path with
/// only safe characters is left bare; anything else is single-quoted.
pub fn shell_quote(value: &str) -> String {
    let safe = !value.is_empty()
        && value.chars().all(|ch| {
            ch.is_ascii_alphanumeric()
                || matches!(ch, '/' | '.' | '_' | '-' | ':' | '@' | '+' | '=')
        });
    if safe {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn write_identity_in(
    scope: Scope,
    cwd: Option<&Path>,
    identity: &GitIdentity,
) -> Result<(), String> {
    set_config(scope, cwd, "user.name", &identity.name)?;
    set_config(scope, cwd, "user.email", &identity.email)?;
    Ok(())
}

/// Which git config file a read/write targets.
#[derive(Clone, Copy)]
enum Scope {
    Global,
    Local,
}

impl Scope {
    fn flag(self) -> &'static str {
        match self {
            Self::Global => "--global",
            Self::Local => "--local",
        }
    }
}

fn config_get(scope: Scope, cwd: Option<&Path>, key: &str) -> String {
    git_command(cwd)
        .args(["config", scope.flag(), "--get", key])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn set_config(scope: Scope, cwd: Option<&Path>, key: &str, value: &str) -> Result<(), String> {
    let value = value.trim();
    let mut command = git_command(cwd);
    if value.is_empty() {
        // `--unset-all` exits non-zero when the key is absent; an empty value
        // means "leave it unset", not an error.
        command.args(["config", scope.flag(), "--unset-all", key]);
    } else {
        command.args(["config", scope.flag(), key, value]);
    }
    let output = command
        .output()
        .map_err(|err| tr!("git_settings.git_not_available", error = err))?;
    if output.status.success() || value.is_empty() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        tr!("git_settings.git_command_failed")
    } else {
        stderr
    })
}

/// Every account `gh auth status` reports, active flag included. Returns an
/// empty list when signed out, on older `gh`, or when `gh` is missing.
pub fn list_accounts() -> Vec<GhAccount> {
    let output = gh_command().args(["auth", "status"]).output();
    let Ok(output) = output else {
        return Vec::new();
    };
    // `gh` has written this to stderr on some versions; combine both streams.
    let mut raw = String::from_utf8_lossy(&output.stdout).into_owned();
    raw.push_str(&String::from_utf8_lossy(&output.stderr));
    parse_accounts(&raw)
}

/// Whether `gh auth switch` exists (gh >= 2.40), so the page can hide the
/// action rather than fail on an older CLI.
pub fn switch_supported() -> bool {
    let output = gh_command().arg("--version").output();
    let Ok(output) = output else {
        return false;
    };
    let line = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    gh::parse_version(&line).is_some_and(|version| version >= MIN_SWITCH_VERSION)
}

/// Sign in with a personal access token by piping it to `gh auth login
/// --with-token`. Orbit never persists the token; `gh` owns the credential.
pub fn login_with_token(token: &str, hostname: &str) -> Result<String, String> {
    let token = token.trim();
    if token.is_empty() {
        return Err(tr!("git_settings.token_required"));
    }
    let mut command = gh_command();
    command.args(["auth", "login", "--with-token"]);
    let hostname = hostname.trim();
    if !hostname.is_empty() {
        command.args(["--hostname", hostname]);
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|err| tr!("git_panel.gh_not_available", error = err))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(token.as_bytes())
            .map_err(|err| err.to_string())?;
        // stdin drops here, closing the pipe so `gh` proceeds.
    }
    let output = child.wait_with_output().map_err(|err| err.to_string())?;
    if output.status.success() {
        Ok(tr!("git_settings.signed_in"))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            tr!(
                "git_panel.gh_exited_with",
                status = output.status.to_string()
            )
        } else {
            stderr
        })
    }
}

/// Make `login` the active `gh` account for `host`.
pub fn switch_account(login: &str, host: &str) -> Result<String, String> {
    let mut args = vec!["auth", "switch", "--user", login];
    let host = host.trim();
    if !host.is_empty() {
        args.push("--hostname");
        args.push(host);
    }
    run_gh(&args)?;
    Ok(tr!(
        "git_settings.account_switched",
        name = login.to_string()
    ))
}

/// Remove `login` from `gh`'s credential store.
pub fn sign_out(login: &str, host: &str) -> Result<String, String> {
    let mut args = vec!["auth", "logout", "--user", login];
    let host = host.trim();
    if !host.is_empty() {
        args.push("--hostname");
        args.push(host);
    }
    run_gh(&args)?;
    Ok(tr!("git_settings.signed_out", name = login.to_string()))
}

fn run_gh(args: &[&str]) -> Result<String, String> {
    let output = gh_command()
        .args(args)
        .output()
        .map_err(|err| tr!("git_panel.gh_not_available", error = err))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(if stderr.is_empty() {
        tr!(
            "git_panel.gh_exited_with",
            status = output.status.to_string()
        )
    } else {
        stderr
    })
}

fn git_command(cwd: Option<&Path>) -> Command {
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.env("GIT_OPTIONAL_LOCKS", "0");
    orbit_rpc::hide_console(&mut command);
    command
}

fn gh_command() -> Command {
    let mut command = Command::new(gh::binary());
    command.env("GH_PROMPT_DISABLED", "1");
    command.env("GH_NO_UPDATE_NOTIFIER", "1");
    command.env("GH_PAGER", "cat");
    command.env("NO_COLOR", "1");
    command.env("CLICOLOR", "0");
    orbit_rpc::hide_console(&mut command);
    command
}

/// Parse `gh auth status` into accounts, tolerant of both the modern
/// `account <login>` form and the older `as <login>` form. The `Active
/// account:` line that follows a login marks it; when only one account is
/// listed and no flag was seen, it is the active one.
pub fn parse_accounts(raw: &str) -> Vec<GhAccount> {
    let mut accounts: Vec<GhAccount> = Vec::new();
    let mut last: Option<usize> = None;
    let mut saw_active_flag = false;
    for line in raw.lines() {
        let line = line.trim();
        if let Some((host, login)) = login_from_line(line) {
            accounts.push(GhAccount {
                login,
                host,
                active: false,
            });
            last = Some(accounts.len() - 1);
            continue;
        }
        if let Some(active) = active_from_line(line) {
            saw_active_flag = true;
            if let Some(ix) = last {
                accounts[ix].active = active;
            }
        }
    }
    if !saw_active_flag && accounts.len() == 1 {
        accounts[0].active = true;
    }
    accounts
}

fn login_from_line(line: &str) -> Option<(String, String)> {
    let rest = line.split_once("Logged in to")?.1;
    let mut parts = rest.split_whitespace();
    let host = parts.next()?.to_string();
    match parts.next()? {
        "account" | "as" => Some((host, parts.next()?.to_string())),
        _ => None,
    }
}

fn active_from_line(line: &str) -> Option<bool> {
    let rest = line.split_once("Active account:")?.1.trim();
    Some(rest.eq_ignore_ascii_case("true"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modern_multi_account_status() {
        let raw = "\
github.com
  ✓ Logged in to github.com account monalisa (keyring)
  - Active account: true
  - Git operations protocol: https
  - Token: gho_****

  ✓ Logged in to github.com account hubot (keyring)
  - Active account: false
  - Git operations protocol: https
";
        let accounts = parse_accounts(raw);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].login, "monalisa");
        assert_eq!(accounts[0].host, "github.com");
        assert!(accounts[0].active);
        assert_eq!(accounts[1].login, "hubot");
        assert!(!accounts[1].active);
    }

    #[test]
    fn parses_legacy_single_account_status() {
        let raw = "\
github.com
  ✓ Logged in to github.com as octocat (/home/u/.config/gh/hosts.yml)
  ✓ Git operations for github.com configured to use https protocol.
";
        let accounts = parse_accounts(raw);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].login, "octocat");
        assert!(accounts[0].active, "the lone account is active");
    }

    #[test]
    fn signed_out_status_has_no_accounts() {
        assert!(parse_accounts("You are not logged into any GitHub hosts.").is_empty());
        assert!(parse_accounts("").is_empty());
    }

    #[test]
    fn ignores_other_logged_in_lines() {
        // A repository or host mention must not be read as an account.
        assert_eq!(
            login_from_line("✓ Logged in to github.com account monalisa"),
            Some(("github.com".to_string(), "monalisa".to_string()))
        );
        assert_eq!(
            login_from_line("✓ Logged in to github.com as octocat (x)"),
            Some(("github.com".to_string(), "octocat".to_string()))
        );
        assert_eq!(
            login_from_line("✓ Logged in to example.com account octocat"),
            Some(("example.com".to_string(), "octocat".to_string()))
        );
        assert_eq!(
            login_from_line("You are not logged into any GitHub hosts."),
            None
        );
        assert_eq!(login_from_line("github.com"), None);
    }

    #[test]
    fn parses_active_flag() {
        assert_eq!(active_from_line("- Active account: true"), Some(true));
        assert_eq!(active_from_line("- Active account: false"), Some(false));
        assert_eq!(active_from_line("Token: gho_x"), None);
    }

    #[test]
    fn local_identity_round_trips_and_clears() {
        let root = std::env::temp_dir().join(format!(
            "orbit-git-account-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let init = Command::new("git")
            .current_dir(&root)
            .args(["init", "--quiet", "--initial-branch=main"])
            .output()
            .unwrap();
        assert!(init.status.success(), "git init failed");

        // No override yet.
        assert_eq!(read_local_identity(&root), GitIdentity::default());

        let identity = GitIdentity {
            name: "Repo Author".into(),
            email: "repo@example.com".into(),
        };
        write_local_identity(&root, &identity).unwrap();
        assert_eq!(read_local_identity(&root), identity);

        clear_local_identity(&root).unwrap();
        assert_eq!(read_local_identity(&root), GitIdentity::default());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn shell_quote_only_quotes_when_needed() {
        assert_eq!(
            shell_quote("/Users/ada/.ssh/id_ed25519"),
            "/Users/ada/.ssh/id_ed25519"
        );
        assert_eq!(
            shell_quote("/Users/ada my/.ssh/key"),
            "'/Users/ada my/.ssh/key'"
        );
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn applies_account_to_the_repository_only() {
        let root = std::env::temp_dir().join(format!(
            "orbit-git-apply-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let init = Command::new("git")
            .current_dir(&root)
            .args(["init", "--quiet", "--initial-branch=main"])
            .output()
            .unwrap();
        assert!(init.status.success(), "git init failed");

        let account = GitAccount {
            label: "Work".into(),
            auth_method: AuthMethod::Ssh,
            ssh_key: "/Users/ada/.ssh/id_work".into(),
            name: "Ada Work".into(),
            email: "ada@work.example.com".into(),
            sign_commits: true,
            signing_key: "ABCD1234".into(),
            ..GitAccount::default()
        };
        apply_to_repo(&root, &account).unwrap();

        assert_eq!(
            read_local_identity(&root),
            GitIdentity {
                name: account.name.clone(),
                email: account.email.clone(),
            }
        );
        let ssh = config_get(Scope::Local, Some(&root), "core.sshCommand");
        assert_eq!(ssh, "ssh -i /Users/ada/.ssh/id_work -o IdentitiesOnly=yes");
        assert_eq!(
            config_get(Scope::Local, Some(&root), "commit.gpgsign"),
            "true"
        );
        assert_eq!(
            config_get(Scope::Local, Some(&root), "user.signingkey"),
            "ABCD1234"
        );

        // A machine-auth identity clears the pinned SSH command and signing.
        let identity_only = GitAccount {
            label: "Personal".into(),
            name: "Ada".into(),
            email: "ada@example.com".into(),
            ..GitAccount::default()
        };
        apply_to_repo(&root, &identity_only).unwrap();
        assert!(config_get(Scope::Local, Some(&root), "core.sshCommand").is_empty());
        assert!(config_get(Scope::Local, Some(&root), "commit.gpgsign").is_empty());
        assert!(config_get(Scope::Local, Some(&root), "user.signingkey").is_empty());

        // An account identity records the gh login instead of a key.
        let account_auth = GitAccount {
            label: "GH".into(),
            auth_method: AuthMethod::Account,
            source_account: "octocat".into(),
            name: "Ada".into(),
            email: "ada@example.com".into(),
            ..GitAccount::default()
        };
        apply_to_repo(&root, &account_auth).unwrap();
        assert_eq!(
            config_get(Scope::Local, Some(&root), "credential.username"),
            "octocat"
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}
