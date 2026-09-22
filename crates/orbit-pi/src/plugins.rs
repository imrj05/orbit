//! Plugin (pi package) discovery and management.
//!
//! pi's "packages" are its extensions — npm packages, git repos, or local
//! paths recorded in the `packages` array of `~/.pi/agent/settings.json`
//! (user scope) or `<workspace>/.pi/settings.json` (project scope). Each
//! source resolves to a managed install directory:
//!
//! - npm  → `<base>/npm/node_modules/<name>`
//! - git  → `<base>/git/<host>/<path>`
//! - local → the path itself, relative to `<base>`
//!
//! where `<base>` is `~/.pi/agent` for user scope and `<workspace>/.pi` for
//! project scope. Orbit reads those settings files and the install dirs, and
//! installs / removes / updates through the `pi` CLI itself — pi owns the
//! network fetch, lockfile, and settings write, so nothing is reimplemented.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

/// How long an install / update may run before it is killed.
const TIMEOUT: Duration = Duration::from_secs(240);

/// Where a package is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageScope {
    /// `~/.pi/agent/settings.json`.
    User,
    /// `<workspace>/.pi/settings.json`.
    Project,
}

impl PackageScope {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PackageScope::User => "Global",
            PackageScope::Project => "Project",
        }
    }
}

/// The source family, shown as a badge on each card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageKind {
    Npm,
    Git,
    Local,
}

impl PackageKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            PackageKind::Npm => "npm",
            PackageKind::Git => "git",
            PackageKind::Local => "local",
        }
    }
}

/// One configured package, flattened to what the settings page renders.
#[derive(Debug, Clone)]
pub(crate) struct PluginPackage {
    /// The settings string, e.g. `npm:@ollama/pi-web-search`.
    pub source: String,
    /// Short display name (package or repo name).
    pub name: String,
    pub scope: PackageScope,
    pub kind: PackageKind,
    /// The directory pi installs this source into.
    pub install_path: PathBuf,
    /// The install directory exists on disk.
    pub installed: bool,
    /// `version` from the installed package's `package.json`, when readable.
    pub version: Option<String>,
}

/// Read every configured package for `workspace`. The optional error string
/// is a malformed settings file; the returned list still holds whatever the
/// other file contributed, so one bad file never hides the rest.
pub(crate) fn discover(workspace: &Path) -> (Vec<PluginPackage>, Option<String>) {
    let mut packages = Vec::new();
    let mut error = None;
    let mut seen: Vec<String> = Vec::new();

    // Project scope first — it wins a duplicate source over user scope.
    for (scope, path) in [
        (PackageScope::Project, workspace.join(".pi/settings.json")),
        (PackageScope::User, agent_dir().join("settings.json")),
    ] {
        match read_packages(&path, scope, workspace) {
            Ok(list) => {
                for package in list {
                    if seen.contains(&package.source) {
                        continue;
                    }
                    seen.push(package.source.clone());
                    packages.push(package);
                }
            }
            Err(err) => {
                error.get_or_insert(err);
            }
        }
    }

    packages.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.scope.label().cmp(b.scope.label()))
    });
    (packages, error)
}

/// Install `source` via `pi install`, optionally into project scope.
pub(crate) fn install(source: &str, project: bool, workspace: &Path) -> Result<String, String> {
    let mut args = vec!["install".to_string(), source.to_string()];
    if project {
        // Trust the project-local settings for this command only; the app
        // already launches pi with `--approve`.
        args.push("-l".to_string());
        args.push("-a".to_string());
    }
    run_pi(&args, workspace)
}

/// Remove `source` from the scope it was installed in.
pub(crate) fn remove(source: &str, project: bool, workspace: &Path) -> Result<String, String> {
    let mut args = vec!["remove".to_string(), source.to_string()];
    if project {
        args.push("-l".to_string());
        args.push("-a".to_string());
    }
    run_pi(&args, workspace)
}

/// Update a single installed `source`.
pub(crate) fn update(source: &str, workspace: &Path) -> Result<String, String> {
    let args = vec!["update".to_string(), source.to_string()];
    run_pi(&args, workspace)
}

fn read_packages(
    path: &Path,
    scope: PackageScope,
    workspace: &Path,
) -> Result<Vec<PluginPackage>, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(tr!(
                "errors.could_not_read",
                path = path.display().to_string(),
                error = err
            ))
        }
    };
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: Value = serde_json::from_str(&raw).map_err(|err| {
        tr!(
            "errors.not_valid_json",
            path = path.display().to_string(),
            error = err
        )
    })?;
    Ok(package_sources(&root)
        .into_iter()
        .map(|source| resolve(&source, scope, workspace))
        .collect())
}

/// Pull the package source strings out of a settings document. Entries are
/// strings (`"npm:foo"`) or objects with a `source` field.
fn package_sources(root: &Value) -> Vec<String> {
    let Some(list) = root.get("packages").and_then(Value::as_array) else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| match entry {
            Value::String(source) if !source.trim().is_empty() => Some(source.clone()),
            Value::Object(map) => map
                .get("source")
                .and_then(Value::as_str)
                .filter(|source| !source.trim().is_empty())
                .map(str::to_string),
            _ => None,
        })
        .collect()
}

fn resolve(source: &str, scope: PackageScope, workspace: &Path) -> PluginPackage {
    let parsed = parse_source(source);
    let install_path = parsed.install_path(scope, workspace);
    let installed = install_path.exists();
    let version = if installed {
        read_version(&install_path)
    } else {
        None
    };
    PluginPackage {
        source: source.to_string(),
        name: parsed.name().to_string(),
        scope,
        kind: parsed.kind(),
        install_path,
        installed,
        version,
    }
}

/// A parsed package source, before it is resolved against a scope.
#[derive(Debug, PartialEq, Eq)]
enum Source {
    Npm {
        name: String,
    },
    Git {
        host: String,
        path: String,
        name: String,
    },
    Local {
        path: PathBuf,
    },
}

impl Source {
    fn name(&self) -> &str {
        match self {
            Source::Npm { name } => name,
            Source::Git { name, .. } => name,
            Source::Local { path } => path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("package"),
        }
    }

    fn kind(&self) -> PackageKind {
        match self {
            Source::Npm { .. } => PackageKind::Npm,
            Source::Git { .. } => PackageKind::Git,
            Source::Local { .. } => PackageKind::Local,
        }
    }

    fn install_path(&self, scope: PackageScope, workspace: &Path) -> PathBuf {
        let base = match scope {
            PackageScope::User => agent_dir(),
            PackageScope::Project => workspace.join(".pi"),
        };
        match self {
            Source::Npm { name } => base.join("npm").join("node_modules").join(name),
            Source::Git { host, path, .. } => base.join("git").join(host).join(path),
            Source::Local { path } => {
                if path.is_absolute() {
                    path.clone()
                } else {
                    base.join(path)
                }
            }
        }
    }
}

fn parse_source(raw: &str) -> Source {
    let source = raw.trim();
    if let Some(spec) = source.strip_prefix("npm:") {
        return Source::Npm {
            name: npm_name(spec),
        };
    }
    if let Some(rest) = source.strip_prefix("git:") {
        return parse_git(rest);
    }
    if let Some(rest) = source.strip_prefix("github:") {
        return parse_git(&format!("github.com/{rest}"));
    }
    for scheme in ["https://", "http://", "ssh://", "git://"] {
        if let Some(rest) = source.strip_prefix(scheme) {
            return parse_git(rest);
        }
    }
    let path = source.strip_prefix("./").unwrap_or(source);
    Source::Local {
        path: PathBuf::from(path),
    }
}

/// Parse a git source that has already had its scheme / `git:` prefix
/// removed — `github.com/owner/repo`, `git@github.com:owner/repo`, etc.
fn parse_git(rest: &str) -> Source {
    let rest = rest.split('#').next().unwrap_or(rest);
    let rest = rest.trim().trim_start_matches("git@");
    let (host, path) = if let Some((host, path)) = rest.split_once(':') {
        (host.to_string(), path.trim_start_matches('/').to_string())
    } else if let Some((host, path)) = rest.split_once('/') {
        (host.to_string(), path.to_string())
    } else {
        (String::new(), rest.to_string())
    };
    let path = path.trim().trim_end_matches(".git").to_string();
    let name = path.rsplit('/').next().unwrap_or(&path).to_string();
    Source::Git { host, path, name }
}

/// `@scope/name@version` / `name@version` → the bare package name.
fn npm_name(spec: &str) -> String {
    let spec = spec.trim();
    if spec.starts_with('@') {
        if let Some(slash) = spec.find('/') {
            if let Some(at) = spec[slash..].find('@') {
                return spec[..slash + at].to_string();
            }
        }
        return spec.to_string();
    }
    spec.split_once('@')
        .map(|(name, _)| name)
        .unwrap_or(spec)
        .to_string()
}

fn read_version(install_path: &Path) -> Option<String> {
    let raw = fs::read_to_string(install_path.join("package.json")).ok()?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    value
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn agent_dir() -> PathBuf {
    crate::platform::home_dir().join(".pi").join("agent")
}

/// Run `pi <args>` from `workspace`, returning stdout on success. Mirrors the
/// commit-message runner: blocking, so call it on the background executor.
fn run_pi(args: &[String], workspace: &Path) -> Result<String, String> {
    let bin = orbit_rpc::pi_binary();
    let mut command = Command::new(&bin);
    command
        .args(args)
        .env("PI_SKIP_VERSION_CHECK", "1")
        // `pi` is `#!/usr/bin/env node`; a bundled `.app` PATH lacks `node`.
        .env("PATH", orbit_rpc::augmented_path(Path::new(&bin).parent()))
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    orbit_rpc::hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|err| tr!("errors.failed_to_run", bin = bin, error = err))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| tr!("plugins.stdout_unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| tr!("plugins.stderr_unavailable"))?;
    let out_handle = std::thread::spawn(move || read_to_end(stdout));
    let err_handle = std::thread::spawn(move || read_to_end(stderr));

    let deadline = Instant::now() + TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = out_handle.join().unwrap_or_default();
                let err = err_handle.join().unwrap_or_default();
                return if status.success() {
                    Ok(out)
                } else {
                    Err(first_line(&err))
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(tr!("errors.pi_timed_out", args = args.join(" ")));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => return Err(tr!("errors.pi_process_error", error = err)),
        }
    }
}

fn read_to_end(mut reader: impl Read) -> String {
    let mut buffer = Vec::new();
    let _ = reader.read_to_end(&mut buffer);
    String::from_utf8_lossy(&buffer).into_owned()
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_npm_sources() {
        assert_eq!(
            parse_source("npm:@ollama/pi-web-search"),
            Source::Npm {
                name: "@ollama/pi-web-search".into()
            }
        );
        assert_eq!(
            parse_source("npm:pi-tracker@1.2.3"),
            Source::Npm {
                name: "pi-tracker".into()
            }
        );
        assert_eq!(
            parse_source("npm:@scope/pkg@2.0.0"),
            Source::Npm {
                name: "@scope/pkg".into()
            }
        );
    }

    #[test]
    fn parses_git_sources() {
        assert_eq!(
            parse_source("git:github.com/earendil-works/pi-review"),
            Source::Git {
                host: "github.com".into(),
                path: "earendil-works/pi-review".into(),
                name: "pi-review".into(),
            }
        );
        assert_eq!(
            parse_source("https://github.com/user/repo.git"),
            Source::Git {
                host: "github.com".into(),
                path: "user/repo".into(),
                name: "repo".into(),
            }
        );
        assert_eq!(
            parse_source("git:git@github.com:user/repo"),
            Source::Git {
                host: "github.com".into(),
                path: "user/repo".into(),
                name: "repo".into(),
            }
        );
    }

    #[test]
    fn parses_local_sources() {
        assert_eq!(
            parse_source("./plugins/demo"),
            Source::Local {
                path: PathBuf::from("plugins/demo")
            }
        );
    }

    #[test]
    fn resolves_install_paths() {
        let workspace = Path::new("/tmp/work");
        let npm = parse_source("npm:@scope/pkg");
        assert_eq!(
            npm.install_path(PackageScope::Project, workspace),
            PathBuf::from("/tmp/work/.pi/npm/node_modules/@scope/pkg")
        );
        let git = parse_source("git:github.com/o/r");
        assert_eq!(
            git.install_path(PackageScope::User, workspace),
            agent_dir().join("git/github.com/o/r")
        );
    }

    #[test]
    fn reads_string_and_object_packages() {
        let raw = r#"{"packages":["npm:a",{"source":"git:github.com/x/y"},42,"  "]}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            package_sources(&value),
            vec!["npm:a".to_string(), "git:github.com/x/y".to_string()]
        );
    }
}
