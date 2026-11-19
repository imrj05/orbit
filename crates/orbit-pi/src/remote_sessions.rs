//! A remote host's pi sessions, for the sidebar. The host lists its own
//! store (`orbit_rpc::remote::list_sessions_script`, read-only, over ssh) and
//! sends back one line per session with only what a row shows; this module
//! turns those lines into rows and decides which saved host entry each
//! session belongs under. No transcript crosses: opening one resumes it on the
//! host with `switch_session`, and pi reads the file there.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::remote_hosts::RemoteHost;
use crate::sessions::{cap_chars, readable_prompt, timestamp_millis};

/// How many of a host's newest sessions are listed.
pub const LIST_LIMIT: usize = 300;
/// How many sessions a host group shows in the sidebar.
pub const GROUP_VISIBLE: usize = 8;

/// One session in a host's store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteSession {
    /// The `.jsonl` path on the host (what `switch_session` takes there).
    pub path: String,
    pub id: String,
    /// The folder the session ran in, on the host.
    pub cwd: String,
    /// pi's name for it, or the first message.
    pub title: String,
    pub first_message: String,
    /// The newest user message's time (the file's mtime when unstamped).
    pub modified: SystemTime,
}

/// Read the script's output, newest first. Lines that do not parse are
/// skipped.
pub fn parse(stdout: &str) -> Vec<RemoteSession> {
    let mut out: Vec<RemoteSession> = stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|row| {
            let text = |key: &str| row.get(key).and_then(Value::as_str).map(str::to_string);
            let first = readable_prompt(&text("first")?).replace('\n', " ");
            let first_message = cap_chars(&first, 110);
            if first_message.is_empty() {
                return None;
            }
            let title = text("name")
                .filter(|name| !name.trim().is_empty())
                .map(|name| cap_chars(&name, 80))
                .unwrap_or_else(|| cap_chars(&first, 80));
            let modified = timestamp_millis(row.get("time"))
                .and_then(|millis| u64::try_from(millis).ok())
                .and_then(|millis| UNIX_EPOCH.checked_add(Duration::from_millis(millis)))
                .unwrap_or(UNIX_EPOCH);
            Some(RemoteSession {
                path: text("path")?,
                id: text("id")?,
                cwd: text("cwd")?,
                title,
                first_message,
                modified,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        b.modified
            .cmp(&a.modified)
            .then_with(|| b.path.cmp(&a.path))
    });
    out
}

/// Whether a session that ran in `cwd` belongs under a host entry rooted at
/// `dir`: the same folder or one inside it.
pub fn belongs_to(cwd: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    let cwd = cwd.trim_end_matches('/');
    dir.is_empty() || cwd == dir || cwd.starts_with(&format!("{dir}/"))
}

/// The host entries a session list is shown under: the enabled ones, each
/// with the sessions that ran in its folder, newest first. A session sits
/// under the entry with the longest matching folder, so `demo on vps`
/// (`/home/u/demo`) takes its own sessions from a `vps` entry rooted at
/// `/home/u`.
pub fn group<'a>(
    hosts: &'a [RemoteHost],
    lists: &'a std::collections::HashMap<String, Vec<RemoteSession>>,
) -> Vec<(&'a RemoteHost, Vec<&'a RemoteSession>)> {
    let enabled: Vec<&RemoteHost> = hosts.iter().filter(|host| host.enabled).collect();
    enabled
        .iter()
        .map(|host| {
            let sessions = lists
                .get(&host.account_key())
                .map(|list| {
                    list.iter()
                        .filter(|session| {
                            let best = enabled
                                .iter()
                                .filter(|other| {
                                    other.account_key() == host.account_key()
                                        && belongs_to(&session.cwd, &other.dir)
                                })
                                .max_by_key(|other| other.dir.trim_end_matches('/').len());
                            best.is_some_and(|best| best.name == host.name)
                        })
                        .collect()
                })
                .unwrap_or_default();
            (*host, sessions)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str, dir: &str) -> RemoteHost {
        RemoteHost {
            name: name.into(),
            ssh: "vps.example".into(),
            user: Some("u".into()),
            port: None,
            dir: dir.into(),
            enabled: true,
            local_dir: None,
        }
    }

    #[test]
    fn rows_come_from_the_host_lines() {
        let stdout = concat!(
            "orbit-sessions: started\n",
            r#"{"path":"/home/u/.pi/agent/sessions/x/a.jsonl","id":"a","cwd":"/home/u/demo","name":"Build fix","first":"fix the\nbuild","time":"2026-01-02T10:00:00Z"}"#,
            "\n",
            r#"{"path":"/p/b.jsonl","id":"b","cwd":"/home/u/demo","name":null,"first":"hello","time":1767434400000}"#,
            "\n",
            r#"{"path":"/p/c.jsonl","id":"c","cwd":"/x","name":null,"first":"   ","time":1}"#,
            "\nnot json\n",
        );
        let rows = parse(stdout);
        assert_eq!(rows.len(), 2, "a blank first message is a draft");
        assert_eq!(rows[0].id, "b", "newest first");
        assert_eq!(rows[0].title, "hello");
        assert_eq!(rows[1].title, "Build fix");
        assert_eq!(rows[1].first_message, "fix the build");
        assert!(rows[0].modified > rows[1].modified);
    }

    #[test]
    fn sessions_land_under_the_closest_host_folder() {
        assert!(belongs_to("/home/u/demo/apps/web", "/home/u/demo/"));
        assert!(!belongs_to("/home/u/demo2", "/home/u/demo"));
        let hosts = vec![
            host("vps", "/home/u"),
            host("demo on vps", "/home/u/demo"),
            RemoteHost {
                enabled: false,
                ..host("off", "/home/u/demo/apps")
            },
        ];
        let session = |id: &str, cwd: &str| RemoteSession {
            path: format!("/p/{id}.jsonl"),
            id: id.into(),
            cwd: cwd.into(),
            title: id.into(),
            first_message: id.into(),
            modified: UNIX_EPOCH,
        };
        let mut lists = std::collections::HashMap::new();
        lists.insert(
            hosts[0].account_key(),
            vec![
                session("a", "/home/u/demo/apps"),
                session("b", "/home/u/other"),
                session("c", "/srv/elsewhere"),
            ],
        );
        let groups = group(&hosts, &lists);
        let ids = |ix: usize| -> Vec<&str> { groups[ix].1.iter().map(|s| s.id.as_str()).collect() };
        assert_eq!(groups.len(), 2, "a disabled entry has no group");
        assert_eq!(ids(0), ["b"], "vps keeps what no closer entry claims");
        assert_eq!(
            ids(1),
            ["a"],
            "the closest folder wins, a disabled one does not count"
        );
    }
}
