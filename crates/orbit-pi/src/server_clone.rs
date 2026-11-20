//! "Add to a server": put a local project on a remote host the way t3code's
//! "Add to another machine" does — `git clone` on the host from the project's
//! `origin`, then copy the files git does not track (`.env` and the like),
//! which a clone cannot bring.
//!
//! This module is the local half and holds no UI: reading the project with
//! `git` (blocking — call it off the UI thread), the clone URL in the other
//! protocol, the copy payload, and parsing what the host's scripts print
//! (`orbit_rpc::remote::{clone_script, copy_files_script}`). The dialog and
//! the flow live in `app/server_clone_ui.rs`.

use std::path::{Path, PathBuf};

use base64::Engine as _;

/// Files larger than this are listed but cannot be picked (t3code's cap).
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// At most this many ignored files are listed.
pub const MAX_LISTED_FILES: usize = 200;

/// How the host reaches the git server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneProtocol {
    Ssh,
    Https,
}

/// One file git ignores in the project, relative to the repository root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IgnoredFile {
    pub path: String,
    pub size: u64,
    pub selected: bool,
}

impl IgnoredFile {
    pub fn too_large(&self) -> bool {
        self.size > MAX_FILE_BYTES
    }
}

/// What the dialog needs to know about the local project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSource {
    /// The repository's top level on this Mac.
    pub root: PathBuf,
    /// The workspace below the root (`apps/web`), or "" at the root. The
    /// session on the host starts in the same place.
    pub subdir: String,
    /// The repository name from the URL (`orbit`), for the folder and host.
    pub name: String,
    /// `origin`'s URL with any password or token removed.
    pub origin: String,
    pub files: Vec<IgnoredFile>,
    /// More ignored files exist than [`MAX_LISTED_FILES`].
    pub truncated: bool,
}

impl ProjectSource {
    /// The clone URL in `protocol`, when the origin can be written that way.
    pub fn url(&self, protocol: CloneProtocol) -> String {
        clone_url_for(&self.origin, protocol).unwrap_or_else(|| self.origin.clone())
    }

    /// Whether the origin can be switched between SSH and HTTPS.
    pub fn switchable(&self) -> bool {
        clone_url_for(&self.origin, CloneProtocol::Ssh).is_some()
            && clone_url_for(&self.origin, CloneProtocol::Https).is_some()
    }
}

/// Read the project at `workspace`: its repository root, `origin`, and the
/// files git ignores (folders git ignores whole, like `node_modules/`, are
/// left out). Blocking.
pub fn inspect(workspace: &Path) -> Result<ProjectSource, String> {
    let git = |args: &[&str]| crate::git::run_git(workspace, args);
    let root = PathBuf::from(
        git(&["rev-parse", "--show-toplevel"]).map_err(|_| tr!("server_clone.not_a_repo"))?,
    );
    let origin = git(&["remote", "get-url", "origin"])
        .ok()
        .filter(|url| !url.is_empty())
        .ok_or_else(|| tr!("server_clone.no_origin"))?;
    let origin = strip_credentials(&origin);
    let subdir = workspace
        .canonicalize()
        .ok()
        .zip(root.canonicalize().ok())
        .and_then(|(here, root)| {
            here.strip_prefix(&root)
                .ok()
                .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        })
        .unwrap_or_default();
    let listed = crate::git::run_git_ok(
        &root,
        [
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    )
    .map_err(|err| format!("{err:#}"))?;
    let (files, truncated) = ignored_files(&root, &listed);
    let name = repo_name(&origin)
        .or_else(|| {
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "project".into());
    Ok(ProjectSource {
        root,
        subdir,
        name,
        origin,
        files,
        truncated,
    })
}

/// Turn `git ls-files -z` output into files with sizes, the env files
/// picked. Directory entries (a trailing `/`) are dropped.
fn ignored_files(root: &Path, listed: &str) -> (Vec<IgnoredFile>, bool) {
    let mut files: Vec<IgnoredFile> = listed
        .split('\0')
        .filter(|path| !path.is_empty() && !path.ends_with('/'))
        .filter_map(|path| {
            let meta = std::fs::metadata(root.join(path)).ok()?;
            meta.is_file().then(|| IgnoredFile {
                path: path.to_string(),
                size: meta.len(),
                selected: is_default_copied(path) && meta.len() <= MAX_FILE_BYTES,
            })
        })
        .collect();
    // The ones people almost always want first, then by path.
    files.sort_by(|a, b| {
        is_default_copied(&b.path)
            .cmp(&is_default_copied(&a.path))
            .then_with(|| a.path.cmp(&b.path))
    });
    let truncated = files.len() > MAX_LISTED_FILES;
    files.truncate(MAX_LISTED_FILES);
    (files, truncated)
}

/// `.env`, `.env.*`, `.envrc` and `.dev.vars`, in any folder: the ignored
/// files a new machine almost always needs.
pub fn is_default_copied(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == ".env" || name.starts_with(".env.") || name == ".envrc" || name == ".dev.vars"
}

/// Drop the `user:password@` part of an http(s) URL: a token in `origin`
/// would otherwise land on the host's command line and in its `.git/config`.
pub fn strip_credentials(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    if !matches!(scheme, "http" | "https") {
        return url.to_string();
    }
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if path.is_empty() && !rest.contains('/') {
        format!("{scheme}://{host}")
    } else {
        format!("{scheme}://{host}/{path}")
    }
}

/// The protocol `url` is written in, if it is SSH or HTTPS.
pub fn protocol_of(url: &str) -> Option<CloneProtocol> {
    if url.starts_with("https://") || url.starts_with("http://") {
        Some(CloneProtocol::Https)
    } else if url.starts_with("ssh://") || scp_parts(url).is_some() {
        Some(CloneProtocol::Ssh)
    } else {
        None
    }
}

/// `git@host:owner/repo.git` → (`git`, `host`, `owner/repo.git`).
fn scp_parts(url: &str) -> Option<(&str, &str, &str)> {
    if url.contains("://") {
        return None;
    }
    let (user_host, path) = url.split_once(':')?;
    let (user, host) = user_host.split_once('@')?;
    (!user.is_empty() && !host.is_empty() && !path.is_empty() && !path.starts_with('/'))
        .then_some((user, host, path))
}

/// `(host, path)` of an SSH or HTTPS URL, the path without a leading `/`.
fn host_and_path(url: &str) -> Option<(String, String)> {
    if let Some((_, host, path)) = scp_parts(url) {
        return Some((host.to_string(), path.to_string()));
    }
    let (scheme, rest) = url.split_once("://")?;
    if !matches!(scheme, "ssh" | "http" | "https") {
        return None;
    }
    let (authority, path) = rest.split_once('/')?;
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    // An SSH port is not the HTTPS port: drop it either way.
    let host = host.split(':').next().unwrap_or(host);
    (!host.is_empty() && !path.is_empty()).then(|| (host.to_string(), path.to_string()))
}

/// `url` written in `protocol` — `git@host:path` for SSH and
/// `https://host/path` for HTTPS — or `None` when it is neither form (a local
/// path, `git://`).
pub fn clone_url_for(url: &str, protocol: CloneProtocol) -> Option<String> {
    let current = protocol_of(url)?;
    if current == protocol {
        return Some(url.to_string());
    }
    let (host, path) = host_and_path(url)?;
    Some(match protocol {
        CloneProtocol::Ssh => format!("git@{host}:{path}"),
        CloneProtocol::Https => format!("https://{host}/{path}"),
    })
}

/// The git host a URL points at (`github.com`), for messages.
pub fn git_host(url: &str) -> Option<String> {
    host_and_path(url).map(|(host, _)| host)
}

/// The repository's name: the last path segment without `.git`.
pub fn repo_name(url: &str) -> Option<String> {
    let path = host_and_path(url)
        .map(|(_, path)| path)
        .unwrap_or_else(|| url.to_string());
    let last = path.trim_end_matches('/').rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    let plain = !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.starts_with('.');
    plain.then(|| name.to_string())
}

/// What the clone on the host reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloneResult {
    /// Cloned into this absolute path.
    Cloned(String),
    /// A checkout of the same URL was already there.
    Reused(String),
    /// It did not clone: the script's kind and git's last lines.
    Failed { kind: String, detail: Vec<String> },
}

/// Read [`orbit_rpc::remote::clone_script`]'s stdout.
pub fn parse_clone(stdout: &str) -> CloneResult {
    use orbit_rpc::remote::CLONE_PREFIX;
    let mut detail = Vec::new();
    let mut failed = None;
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix(CLONE_PREFIX) else {
            continue;
        };
        if let Some(path) = rest.strip_prefix("done ") {
            return CloneResult::Cloned(path.to_string());
        }
        if let Some(path) = rest.strip_prefix("existing ") {
            return CloneResult::Reused(path.to_string());
        }
        if let Some(kind) = rest.strip_prefix("failed ") {
            failed = Some(kind.to_string());
        } else if let Some(line) = rest.strip_prefix("detail ") {
            detail.push(line.to_string());
        }
    }
    CloneResult::Failed {
        kind: failed.unwrap_or_else(|| "other".into()),
        detail,
    }
}

/// One sentence for a failed clone, naming what to do about it.
pub fn clone_failure_text(kind: &str, detail: &[String], url: &str, dest: &str) -> String {
    let host = git_host(url).unwrap_or_else(|| url.to_string());
    let last = detail.last().cloned().unwrap_or_default();
    match kind {
        "git-missing" => tr!("server_clone.git_missing"),
        "not-empty" => tr!("server_clone.not_empty", folder = dest.to_string()),
        "parent" => tr!("server_clone.parent_failed", folder = dest.to_string()),
        "hostkey" => tr!("server_clone.git_host_key", host = host),
        "publickey" => tr!("server_clone.git_publickey", host = host),
        "login" => tr!("server_clone.git_login", host = host),
        "not-found" => tr!("server_clone.git_not_found", host = host),
        _ if !last.is_empty() => last,
        _ => tr!("server_clone.clone_failed"),
    }
}

/// The JSON list the copy script reads on stdin, from the picked files;
/// files that cannot be read are returned as failures instead.
pub fn copy_payload(root: &Path, files: &[String]) -> (String, Vec<(String, String)>) {
    let mut items = Vec::new();
    let mut failures = Vec::new();
    for path in files {
        match std::fs::read(root.join(path)) {
            Ok(bytes) if bytes.len() as u64 > MAX_FILE_BYTES => {
                failures.push((path.clone(), tr!("server_clone.too_large")));
            }
            Ok(bytes) => items.push(serde_json::json!({
                "path": path,
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
            })),
            Err(err) => failures.push((path.clone(), err.to_string())),
        }
    }
    (serde_json::Value::Array(items).to_string(), failures)
}

/// The files [`orbit_rpc::remote::copy_files_script`] could not write, with
/// its reason for each. A file the output never mentions failed too.
pub fn parse_copy(stdout: &str, sent: &[String]) -> Vec<(String, String)> {
    let mut wrote = std::collections::HashSet::new();
    let mut failed = Vec::new();
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("orbit-copy: wrote ") {
            wrote.insert(path.to_string());
        } else if let Some(rest) = line.strip_prefix("orbit-copy: failed ") {
            let (path, reason) = rest.split_once('\t').unwrap_or((rest, ""));
            failed.push((path.to_string(), reason.to_string()));
        }
    }
    for path in sent {
        if !wrote.contains(path) && !failed.iter().any(|(failed, _)| failed == path) {
            failed.push((path.clone(), tr!("server_clone.not_written")));
        }
    }
    failed
}

/// The folder on the host the session starts in: the clone plus the
/// workspace's place inside the repository.
pub fn session_dir(clone: &str, subdir: &str) -> String {
    let subdir = subdir.trim_matches('/');
    if subdir.is_empty() {
        clone.to_string()
    } else {
        format!("{}/{subdir}", clone.trim_end_matches('/'))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_switch_between_ssh_and_https() {
        let ssh = "git@github.com:owner/repo.git";
        let https = "https://github.com/owner/repo.git";
        assert_eq!(protocol_of(ssh), Some(CloneProtocol::Ssh));
        assert_eq!(protocol_of(https), Some(CloneProtocol::Https));
        assert_eq!(
            clone_url_for(ssh, CloneProtocol::Https).as_deref(),
            Some(https)
        );
        assert_eq!(
            clone_url_for(https, CloneProtocol::Ssh).as_deref(),
            Some(ssh)
        );
        assert_eq!(
            clone_url_for(
                "ssh://git@gitlab.example:2222/team/app.git",
                CloneProtocol::Https
            )
            .as_deref(),
            Some("https://gitlab.example/team/app.git"),
            "an SSH port is not carried to HTTPS"
        );
        assert_eq!(clone_url_for("/srv/git/app", CloneProtocol::Ssh), None);
        assert_eq!(clone_url_for("git://host/app", CloneProtocol::Https), None);
    }

    #[test]
    fn credentials_never_leave_this_mac() {
        assert_eq!(
            strip_credentials("https://x-access-token:ghp_secret@github.com/o/r.git"),
            "https://github.com/o/r.git"
        );
        assert_eq!(
            strip_credentials("https://github.com/o/r.git"),
            "https://github.com/o/r.git"
        );
        assert_eq!(
            strip_credentials("git@github.com:o/r.git"),
            "git@github.com:o/r.git"
        );
    }

    #[test]
    fn names_hosts_and_default_files() {
        assert_eq!(
            repo_name("git@github.com:o/orbit.git").as_deref(),
            Some("orbit")
        );
        assert_eq!(repo_name("https://h/o/app/").as_deref(), Some("app"));
        assert_eq!(repo_name("https://h/o/..git"), None);
        assert_eq!(
            git_host("git@github.com:o/r").as_deref(),
            Some("github.com")
        );
        for yes in [".env", "apps/web/.env.local", ".envrc", "w/.dev.vars"] {
            assert!(is_default_copied(yes), "{yes}");
        }
        for no in [".envoy", "env", "dist/app.js", ".env/"] {
            assert!(!is_default_copied(no), "{no}");
        }
        assert_eq!(session_dir("/home/u/app", ""), "/home/u/app");
        assert_eq!(
            session_dir("/home/u/app/", "/apps/web/"),
            "/home/u/app/apps/web"
        );
    }

    #[test]
    fn reads_what_the_host_printed() {
        assert_eq!(
            parse_clone("orbit-clone: started\norbit-clone: done /home/u/app\n"),
            CloneResult::Cloned("/home/u/app".into())
        );
        assert_eq!(
            parse_clone("orbit-clone: started\norbit-clone: existing /home/u/app\n"),
            CloneResult::Reused("/home/u/app".into())
        );
        assert_eq!(
            parse_clone(
                "orbit-clone: started\norbit-clone: detail fatal: x\norbit-clone: failed publickey\n"
            ),
            CloneResult::Failed {
                kind: "publickey".into(),
                detail: vec!["fatal: x".into()]
            }
        );
        let failed = parse_copy(
            "orbit-copy: started\norbit-copy: wrote .env\norbit-copy: failed ../x\tnot inside\n",
            &[".env".into(), "../x".into(), "lost".into()],
        );
        assert_eq!(
            failed,
            vec![
                ("../x".to_string(), "not inside".to_string()),
                ("lost".to_string(), tr!("server_clone.not_written")),
            ]
        );
    }

    /// A real repository: the env files are listed and picked, a wholly
    /// ignored folder is one skipped entry, a large file is listed but not
    /// picked, and the payload carries base64.
    #[test]
    fn inspects_a_real_repository() {
        let root = std::env::temp_dir().join(format!(
            "orbit-server-clone-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("apps/web")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        let git = |args: &[&str]| crate::git::run_git(&root, args).expect("git");
        git(&["init", "-q"]);
        git(&[
            "remote",
            "add",
            "origin",
            "https://user:tok@github.com/o/demo.git",
        ]);
        std::fs::write(
            root.join(".gitignore"),
            ".env\n.env.*\nnode_modules/\nbig.bin\n",
        )
        .unwrap();
        std::fs::write(root.join(".env"), "A=1\n").unwrap();
        std::fs::write(root.join("apps/web/.env.local"), "B=2\n").unwrap();
        std::fs::write(root.join("node_modules/x/index.js"), "").unwrap();
        std::fs::write(root.join("big.bin"), vec![0u8; MAX_FILE_BYTES as usize + 1]).unwrap();

        let source = inspect(&root.join("apps/web")).expect("inspect");
        assert_eq!(source.origin, "https://github.com/o/demo.git");
        assert_eq!(source.name, "demo");
        assert_eq!(source.subdir, "apps/web");
        assert!(source.switchable());
        assert_eq!(source.url(CloneProtocol::Ssh), "git@github.com:o/demo.git");
        let paths: Vec<_> = source.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, [".env", "apps/web/.env.local", "big.bin"]);
        assert!(source.files[0].selected && source.files[1].selected);
        assert!(source.files[2].too_large() && !source.files[2].selected);

        let (payload, failures) = copy_payload(&source.root, &[".env".into(), "big.bin".into()]);
        assert_eq!(failures.len(), 1, "too large");
        let items: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(items[0]["path"], ".env");
        assert_eq!(items[0]["data"], "QT0xCg==");
        let _ = std::fs::remove_dir_all(&root);
    }
}
