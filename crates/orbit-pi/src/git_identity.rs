//! A repository's own commit identity: `user.name` / `user.email` written to
//! that repository's `.git/config` (`git config --local`), overriding the
//! global identity for commits made there — and only there.
//!
//! Settings → Git Accounts edits it for the current project. The global
//! (`~/.gitconfig`) identity is never written. Linked worktrees share the
//! repository's config, so an override applies to every worktree of it.
//! Every call shells out to `git` and blocks, so callers run it off the UI
//! thread.
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const NAME_KEY: &str = "user.name";
const EMAIL_KEY: &str = "user.email";
/// `git config --unset` exits 5 when the key was not set: nothing to remove.
const UNSET_MISSING: i32 = 5;
/// Bound on a surfaced git failure, so an error row stays one readable line.
const MAX_ERROR_CHARS: usize = 240;

/// The identity set in one repository's own config; `None` fields fall back
/// to the global (or system) identity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
}

impl LocalIdentity {
    /// The repository overrides at least one of the two values.
    pub fn is_set(&self) -> bool {
        self.name.is_some() || self.email.is_some()
    }
}

/// Why a typed identity is refused before git is asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    EmptyName,
    EmptyEmail,
    /// Line breaks, control characters or `<` `>` — git strips or rejects them.
    BadName,
    /// Not one `local@domain` token.
    BadEmail,
}

fn git() -> Command {
    let executable = crate::onboarding::locate("git").unwrap_or_else(|| PathBuf::from("git"));
    let mut command = Command::new(executable);
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .stdin(Stdio::null());
    orbit_rpc::hide_console(&mut command);
    command
}

fn run(workspace: &Path, args: &[&str]) -> std::io::Result<Output> {
    git().current_dir(workspace).args(args).output()
}

/// git's own complaint, first line only and bounded.
fn failure(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr.lines().map(str::trim).find(|l| !l.is_empty());
    match line {
        Some(line) => line.chars().take(MAX_ERROR_CHARS).collect(),
        None => format!("git exited with {}", output.status),
    }
}

fn local_value(workspace: &Path, key: &str) -> Option<String> {
    let output = run(workspace, &["config", "--local", "--get", key]).ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!value.is_empty()).then_some(value)
}

/// The repository-local identity, or `None` when `workspace` is not inside a
/// Git repository (or git is missing) — there is nothing to override then.
pub fn read(workspace: &Path) -> Option<LocalIdentity> {
    let inside = run(workspace, &["rev-parse", "--git-dir"]).ok()?;
    if !inside.status.success() {
        return None;
    }
    Some(LocalIdentity {
        name: local_value(workspace, NAME_KEY),
        email: local_value(workspace, EMAIL_KEY),
    })
}

/// Trim and check a typed name and email; both are required, an identity is
/// a pair.
pub fn validate(name: &str, email: &str) -> Result<(String, String), Invalid> {
    let name = name.trim();
    let email = email.trim();
    if name.is_empty() {
        return Err(Invalid::EmptyName);
    }
    if email.is_empty() {
        return Err(Invalid::EmptyEmail);
    }
    let unsafe_char = |c: char| c.is_control() || c == '<' || c == '>';
    if name.chars().any(unsafe_char) {
        return Err(Invalid::BadName);
    }
    let well_formed = match email.split_once('@') {
        Some((local, domain)) => {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        }
        None => false,
    };
    if !well_formed || email.chars().any(|c| unsafe_char(c) || c.is_whitespace()) {
        return Err(Invalid::BadEmail);
    }
    Ok((name.to_string(), email.to_string()))
}

/// Write `name` / `email` (already validated) to the repository's config.
pub fn save(workspace: &Path, name: &str, email: &str) -> Result<(), String> {
    for (key, value) in [(NAME_KEY, name), (EMAIL_KEY, email)] {
        let output =
            run(workspace, &["config", "--local", key, value]).map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(failure(&output));
        }
    }
    Ok(())
}

/// Remove the repository's override so the global identity applies again.
pub fn clear(workspace: &Path) -> Result<(), String> {
    for key in [NAME_KEY, EMAIL_KEY] {
        let output = run(workspace, &["config", "--local", "--unset-all", key])
            .map_err(|e| e.to_string())?;
        if !output.status.success() && output.status.code() != Some(UNSET_MISSING) {
            return Err(failure(&output));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "orbit-git-identity-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn validate_trims_and_requires_both_values() {
        assert_eq!(
            validate("  Ada Lovelace ", " ada@example.test "),
            Ok(("Ada Lovelace".into(), "ada@example.test".into()))
        );
        assert_eq!(validate("  ", "a@b"), Err(Invalid::EmptyName));
        assert_eq!(validate("Ada", ""), Err(Invalid::EmptyEmail));
    }

    #[test]
    fn validate_refuses_what_git_would_strip_or_mangle() {
        assert_eq!(validate("Ada\nEvil", "a@b"), Err(Invalid::BadName));
        assert_eq!(validate("Ada <x>", "a@b"), Err(Invalid::BadName));
        assert_eq!(validate("Ada", "not-an-email"), Err(Invalid::BadEmail));
        assert_eq!(validate("Ada", "a b@c"), Err(Invalid::BadEmail));
        assert_eq!(validate("Ada", "@c"), Err(Invalid::BadEmail));
        assert_eq!(validate("Ada", "a@"), Err(Invalid::BadEmail));
        assert_eq!(validate("Ada", "a@b@c"), Err(Invalid::BadEmail));
        assert_eq!(validate("Ada", "<a@b>"), Err(Invalid::BadEmail));
    }

    #[test]
    fn outside_a_repository_there_is_nothing_to_override() {
        let dir = temp_dir("plain");
        // A temp dir may itself sit under a repository on some machines;
        // only assert when git agrees it is outside one.
        let outside = !run(&dir, &["rev-parse", "--git-dir"])
            .map(|o| o.status.success())
            .unwrap_or(true);
        if outside {
            assert_eq!(read(&dir), None);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_then_clear_round_trips_through_the_local_config_only() {
        let dir = temp_dir("repo");
        assert!(run(&dir, &["init", "-q"]).unwrap().status.success());
        assert_eq!(read(&dir), Some(LocalIdentity::default()));
        assert!(!read(&dir).unwrap().is_set());

        save(&dir, "Work Me", "me@work.test").unwrap();
        let identity = read(&dir).unwrap();
        assert_eq!(identity.name.as_deref(), Some("Work Me"));
        assert_eq!(identity.email.as_deref(), Some("me@work.test"));
        let config = std::fs::read_to_string(dir.join(".git/config")).unwrap();
        assert!(config.contains("me@work.test"), "written to .git/config");

        // Overwriting replaces, never appends a second value.
        save(&dir, "Other Me", "me@other.test").unwrap();
        assert_eq!(read(&dir).unwrap().email.as_deref(), Some("me@other.test"));

        clear(&dir).unwrap();
        assert!(!read(&dir).unwrap().is_set());
        // Clearing an already-clear repository is not an error.
        clear(&dir).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
