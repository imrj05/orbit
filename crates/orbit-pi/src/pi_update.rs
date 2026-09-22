//! Background pi self-update: the launch check against pi's own release
//! endpoint and the delegated `pi update self`.
//!
//! Orbit never patches pi's files itself. The check reads the same endpoint
//! pi's startup notice reads (`pi.dev/api/latest-version`), and the install
//! is pi's own updater — so install-method detection (npm / pnpm / bun,
//! managed builds), registry configuration, and write locations all stay
//! pi's truth. Everything here blocks; the app calls it from the background
//! executor.

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

use crate::onboarding;

/// pi's release endpoint — the one pi's own version check reads.
const LATEST_VERSION_URL: &str = "https://pi.dev/api/latest-version";
/// pi waits up to ten seconds for its check; match it.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

/// The latest published pi version, or `None` when the endpoint is
/// unreachable or answers something unexpected. Never an error: no network
/// just means no update.
pub fn latest_version() -> Option<String> {
    let response = reqwest::blocking::Client::builder()
        .timeout(CHECK_TIMEOUT)
        .build()
        .ok()?
        .get(LATEST_VERSION_URL)
        .header("accept", "application/json")
        .header("User-Agent", concat!("Orbit/", env!("CARGO_PKG_VERSION")))
        .send()
        .ok()?;
    let bytes = response.error_for_status().ok()?.bytes().ok()?;
    let body: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let version = body.get("version")?.as_str()?.trim();
    (!version.is_empty()).then(|| version.to_string())
}

/// The installed pi's version, straight from `pi --version`.
pub fn installed_version() -> Option<String> {
    onboarding::version_of(Path::new(&orbit_rpc::pi_binary()))
}

/// Run pi's own updater. `--no-approve` keeps the run from loading
/// project-local files; the update itself (self target, install method,
/// registry) is pi's. Blocks until the updater exits.
pub fn run_self_update() -> std::io::Result<Output> {
    let bin = orbit_rpc::pi_binary();
    let mut command = Command::new(&bin);
    // pi's updater shells out to the package manager that owns it; a bundled
    // `.app` launches with a minimal PATH that usually lacks `node`, so use
    // the same augmented PATH as RPC session spawns.
    command.env("PATH", orbit_rpc::augmented_path(Path::new(&bin).parent()));
    command
        // The update run needs no startup notice of its own.
        .env("PI_SKIP_VERSION_CHECK", "1")
        .args(["update", "self", "--no-approve"]);
    orbit_rpc::hide_console(&mut command);
    command.output()
}

/// Semver-ish ordering for dotted release numbers: `0.10.0` is newer than
/// `0.9.9`, a missing field counts as zero (`1.2` == `1.2.0`), and anything
/// after `-`/`+` is build metadata and is not ordered.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare(candidate, current) == std::cmp::Ordering::Greater
}

fn compare(left: &str, right: &str) -> std::cmp::Ordering {
    fn fields(version: &str) -> impl Iterator<Item = u64> + '_ {
        version
            .split(['-', '+'])
            .next()
            .unwrap_or(version)
            .split('.')
            .map(|field| field.trim().parse::<u64>().unwrap_or(0))
    }

    let mut left = fields(left);
    let mut right = fields(right);
    loop {
        match (left.next(), right.next()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (left, right) => {
                let ordering = left.unwrap_or(0).cmp(&right.unwrap_or(0));
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
}

/// The line worth showing when an update fails: pi prints `Error: …` and npm
/// failures trail after it, so prefer that line and fall back to the last
/// non-empty one.
pub fn failure_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr)
    );
    let text = strip_ansi(&text);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    lines
        .iter()
        .find(|line| line.starts_with("Error:"))
        .or_else(|| lines.last())
        .map(|line| line.trim_start_matches("Error:").trim().to_string())
        .filter(|line| !line.is_empty())
        .unwrap_or_else(|| tr!("pi_update.failed_short"))
}

/// Drop ANSI SGR sequences, so a colored child's output still matches.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch != '\u{1b}' {
            out.push(ch);
            continue;
        }
        // Consume through the sequence's final byte (`m` for SGR).
        for next in chars.by_ref() {
            if next.is_ascii_alphabetic() {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_field_by_field_not_lexically() {
        assert!(is_newer("0.10.0", "0.9.0"));
        assert!(is_newer("0.85.2", "0.85.1"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.85.1", "0.85.1"));
        assert!(!is_newer("0.85.0", "0.85.1"));
        assert!(is_newer("1.2.1", "1.2"));
        assert!(!is_newer("1.2", "1.2.0"));
        assert!(!is_newer("1.2.0+build.7", "1.2.0"));
    }

    #[test]
    fn failure_detail_prefers_pi_error_and_strips_color() {
        let out = b"";
        let err = b"\x1b[31mError: npm failed with ERESOLVE\x1b[39m\nnpm details follow\n";
        assert_eq!(failure_detail(out, err), "npm failed with ERESOLVE");

        // Without an `Error:` line, the tail is the complaint.
        let out = b"fetching...\ninstall refused\n";
        assert_eq!(failure_detail(out, b""), "install refused");

        assert_eq!(failure_detail(b"", b""), "pi update failed");
    }

    #[test]
    fn strip_ansi_keeps_plain_text() {
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\x1b[32mgreen\x1b[39m"), "green");
    }

    #[test]
    fn installed_version_reads_this_machines_pi_when_present() {
        // Skips cleanly where pi isn't installed.
        let Some(version) = installed_version() else {
            return;
        };
        assert!(
            version
                .split('.')
                .next()
                .is_some_and(|major| major.parse::<u64>().is_ok()),
            "unexpected pi version {version:?}"
        );
    }

    /// Manual: `cargo test -p orbit-pi live_latest_version -- --ignored`.
    #[ignore = "hits pi.dev"]
    #[test]
    fn live_latest_version_answers_while_online() {
        let latest = latest_version().expect("pi.dev did not answer");
        assert_eq!(latest.split('.').count(), 3, "{latest}");
    }
}
