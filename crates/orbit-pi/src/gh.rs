//! GitHub integration through the official `gh` CLI.
//!
//! Orbit never stores a GitHub token and never calls `api.github.com` directly.
//! It shells out to the GitHub CLI, which already owns the user's
//! authentication and the API contract, and parses its `--json` output into
//! typed values. The binary is resolved like `pi` is (a `GH_BIN` override, then
//! `PATH`), and every invocation runs with a quiet, non-interactive
//! environment so a missing credential fails fast instead of opening a prompt.
//!
//! This module currently probes availability and sign-in state. Issue and PR
//! listing and mutations land in later phases; keeping the runner and parsers
//! here means they can be unit-tested without a network.

use std::path::Path;
use std::process::Command;

/// Environment override for the `gh` binary, mirroring `PI_BIN`.
pub const GH_BIN_ENV: &str = "GH_BIN";

/// The oldest `gh` whose `--json` surface this client relies on.
pub const MIN_GH_VERSION: (u32, u32, u32) = (2, 20, 0);

#[cfg(windows)]
const GH_BIN_NAMES: &[&str] = &["gh.exe", "gh.cmd", "gh.bat", "gh.ps1"];
#[cfg(not(windows))]
const GH_BIN_NAMES: &[&str] = &["gh"];

/// What probing the GitHub CLI found. Drives the setup empty state on the
/// Issues and Pull requests tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhStatus {
    /// The CLI is installed and runnable.
    pub installed: bool,
    /// `gh auth status` reported a signed-in host.
    pub authenticated: bool,
    /// Human-readable context: the auth error, or the resolved version line.
    pub detail: String,
}

impl GhStatus {
    /// The not-installed state.
    pub fn missing() -> Self {
        Self {
            installed: false,
            authenticated: false,
            detail: String::new(),
        }
    }
}

/// Probe `gh` for the current workspace. Never fails: a missing or broken
/// binary is reported through [`GhStatus`], not an error, so the page can show
/// an honest setup state instead of a red banner.
pub fn status(cwd: &Path) -> GhStatus {
    let Ok(version_out) = run(cwd, &["--version"]) else {
        return GhStatus::missing();
    };
    let version = version_out.lines().next().unwrap_or("").trim().to_string();
    let supported = parse_version(&version).is_none_or(|v| v >= MIN_GH_VERSION);
    match run(cwd, &["auth", "status"]) {
        Ok(_) if supported => GhStatus {
            installed: true,
            authenticated: true,
            detail: version,
        },
        // An old CLI still reports auth, but the `--json` fields this client
        // asks for may not exist yet, so surface the mismatch.
        Ok(_) => GhStatus {
            installed: true,
            authenticated: true,
            detail: format!("{version} — {}", tr!("git_panel.gh_version_old")),
        },
        Err(err) => GhStatus {
            installed: true,
            authenticated: false,
            detail: err,
        },
    }
}

/// The resolved `gh` executable path.
pub fn binary() -> String {
    if let Ok(bin) = std::env::var(GH_BIN_ENV) {
        if !bin.is_empty() {
            return bin;
        }
    }
    for name in GH_BIN_NAMES {
        if let Some(found) = find_on_path(name) {
            return found;
        }
    }
    GH_BIN_NAMES[0].to_string()
}

/// A `gh` invocation rooted at `cwd`, with prompts disabled so it fails rather
/// than blocking on a TTY that does not exist.
fn command(cwd: &Path) -> Command {
    let mut command = Command::new(binary());
    command.current_dir(cwd);
    command.env("GH_PROMPT_DISABLED", "1");
    command.env("GH_NO_UPDATE_NOTIFIER", "1");
    command.env("GH_PAGER", "cat");
    command.env("NO_COLOR", "1");
    command.env("CLICOLOR", "0");
    orbit_rpc::hide_console(&mut command);
    command
}

/// Run `gh` with `args`, returning trimmed stdout or the trimmed stderr.
fn run(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = command(cwd)
        .args(args)
        .output()
        .map_err(|err| tr!("git_panel.gh_not_available", error = err))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(tr!(
            "git_panel.gh_exited_with",
            status = output.status.to_string()
        ))
    } else {
        Err(stderr)
    }
}

/// Walk `PATH` and return the first file named `name` found on it.
fn find_on_path(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Parse a semantic version out of `gh --version` output, e.g.
/// `gh version 2.40.1 (2024-01-01)` → `(2, 40, 1)`.
pub fn parse_version(raw: &str) -> Option<(u32, u32, u32)> {
    for token in raw.split_whitespace() {
        let token = token.trim_start_matches('v');
        let mut parts = token.split('.');
        let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let b = b.trim_end_matches(|ch: char| !ch.is_ascii_digit());
        let c = c.trim_end_matches(|ch: char| !ch.is_ascii_digit());
        if let (Ok(a), Ok(b), Ok(c)) = (a.parse::<u32>(), b.parse::<u32>(), c.parse::<u32>()) {
            return Some((a, b, c));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_versions_from_gh_output() {
        assert_eq!(
            parse_version("gh version 2.40.1 (2024-01-01)"),
            Some((2, 40, 1))
        );
        assert_eq!(parse_version("gh version 2.40.1"), Some((2, 40, 1)));
        assert_eq!(parse_version("v2.1.0"), Some((2, 1, 0)));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn version_floor_matches_the_json_surface_we_need() {
        assert_eq!(MIN_GH_VERSION, (2, 20, 0));
        assert!(parse_version("gh version 2.40.1").unwrap() >= MIN_GH_VERSION);
        assert!(parse_version("gh version 2.10.0").unwrap() < MIN_GH_VERSION);
    }
}
