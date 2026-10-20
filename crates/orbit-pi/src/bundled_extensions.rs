//! Orbit's bundled pi extensions.
//!
//! Orbit ships five small pi extensions under `contrib/` and loads them with
//! `pi --extension <path>` on every session process it spawns:
//!
//! - **quota bridge** (`contrib/orbit-quota-extension/`) — fetches each
//!   configured provider's account quota through pi's own credential
//!   resolution and appends normalized snapshots as custom session entries.
//! - **guard** (`contrib/orbit-guard-extension/`) — intercepts `tool_call` and
//!   confirms the mutating calls the active access mode does not
//!   auto-approve (see [`crate::access`]).
//! - **auto-title** (`contrib/orbit-title-extension/`) — after the first turn
//!   settles, asks a model for a short session title and sets it through
//!   `pi.setSessionName` (see [`crate::auto_title`]).
//! - **workflow** (`contrib/orbit-workflow-extension/`) — scopes the session
//!   to Plan/Build/Ask: disables write tools in read-only modes, gates bash to
//!   a read-only allowlist, and injects mode guidance (see [`crate::workflow`]).
//! - **subagent** (`contrib/orbit-subagent-extension/`) — registers a
//!   `subagent` tool that runs child pi processes with isolated context:
//!   single, parallel, and chained delegation over built-in scout / planner /
//!   reviewer / worker roles plus user and project agent files.
//!
//! Nothing is installed and no settings file is touched: each extension is
//! materialized under `~/.orbit-pi/` and rewritten only when its contents
//! change, so app updates can never leave a stale registration behind. The
//! quota payload contract is `crates/orbit-rpc/docs/quota-rpc.md`.

use std::fs;
use std::path::{Path, PathBuf};

use orbit_rpc::PiClient;

use crate::session_defaults::SessionDefault;

const QUOTA_INDEX_JS: &str = include_str!("../../../contrib/orbit-quota-extension/index.js");
const QUOTA_ADAPTERS_JS: &str = include_str!("../../../contrib/orbit-quota-extension/adapters.js");
const GUARD_INDEX_JS: &str = include_str!("../../../contrib/orbit-guard-extension/index.js");
const GUARD_POLICY_JS: &str = include_str!("../../../contrib/orbit-guard-extension/policy.js");
const TITLE_INDEX_JS: &str = include_str!("../../../contrib/orbit-title-extension/index.js");
const TITLE_HELPERS_JS: &str = include_str!("../../../contrib/orbit-title-extension/title.js");
const WORKFLOW_INDEX_JS: &str = include_str!("../../../contrib/orbit-workflow-extension/index.js");
const WORKFLOW_POLICY_JS: &str =
    include_str!("../../../contrib/orbit-workflow-extension/policy.js");
const SUBAGENT_INDEX_JS: &str = include_str!("../../../contrib/orbit-subagent-extension/index.js");
const SUBAGENT_AGENTS_JS: &str =
    include_str!("../../../contrib/orbit-subagent-extension/agents.js");
const SUBAGENT_LIB_JS: &str = include_str!("../../../contrib/orbit-subagent-extension/lib.js");
const SUBAGENT_SCOUT_MD: &str =
    include_str!("../../../contrib/orbit-subagent-extension/agents/scout.md");
const SUBAGENT_PLANNER_MD: &str =
    include_str!("../../../contrib/orbit-subagent-extension/agents/planner.md");
const SUBAGENT_REVIEWER_MD: &str =
    include_str!("../../../contrib/orbit-subagent-extension/agents/reviewer.md");
const SUBAGENT_WORKER_MD: &str =
    include_str!("../../../contrib/orbit-subagent-extension/agents/worker.md");

/// The bundled extensions materialized on disk, kept for the process lifetime.
#[derive(Default)]
pub(crate) struct BundledExtensions {
    /// The quota bridge's `index.js`; `None` when it could not be written
    /// (quota then depends on the `quota.*` RPC, as before).
    quota: Option<PathBuf>,
    /// The guard's `index.js`; `None` when it could not be written (the app
    /// then runs unguarded and the access-mode chip is honest about it).
    guard: Option<PathBuf>,
    /// The auto-title extension's `index.js`; `None` when it could not be
    /// written (sessions then keep their first-message title).
    title: Option<PathBuf>,
    /// The workflow extension's `index.js`; `None` when it could not be
    /// written (sessions then run unscoped and the mode chip is honest).
    workflow: Option<PathBuf>,
    /// The subagent extension's `index.js`; `None` when it could not be
    /// written (the parent then has no `subagent` tool).
    subagent: Option<PathBuf>,
}

impl BundledExtensions {
    /// Materialize the bundled extensions under `~/.orbit-pi/`. Best-effort:
    /// a filesystem failure degrades to no extension, never a blocked launch.
    pub(crate) fn install() -> Self {
        Self {
            quota: install_quota_bridge(),
            guard: install_guard(),
            title: install_title_extension(),
            workflow: install_workflow_extension(),
            subagent: install_subagent_extension(),
        }
    }

    /// The quota bridge's `index.js`; `None` when it could not be written.
    pub(crate) fn quota(&self) -> Option<&Path> {
        self.quota.as_deref()
    }

    /// The guard's `index.js`; `None` when it could not be written.
    pub(crate) fn guard(&self) -> Option<&Path> {
        self.guard.as_deref()
    }

    /// The workflow extension's `index.js`; `None` when it could not be written.
    pub(crate) fn workflow(&self) -> Option<&Path> {
        self.workflow.as_deref()
    }

    /// Spawn a pi session process with every available bundled extension
    /// loaded. Every session spawn in the app goes through here.
    ///
    /// `apply_default` seeds the child's startup model from the user's
    /// default model (`~/.orbit-pi/session-defaults.json`). Pass `false` for
    /// a process that is about to *resume* a session — a resumed session
    /// keeps the model recorded in its file, never Orbit's default.
    ///
    /// `env` carries Orbit's MCP secret values, so Pi can expand the
    /// `${NAME}` references in `mcp.json` without the config ever holding a
    /// raw credential. It applies to this child only.
    pub(crate) fn spawn(
        &self,
        workspace: &Path,
        apply_default: bool,
        env: &[(String, String)],
    ) -> anyhow::Result<PiClient> {
        // A `pi update` can restore the pristine RPC bundle while Orbit is
        // running; re-check before every spawn so a session switch or Runtime
        // restart still gets custom UI, quota, and auth. The unchanged fast
        // path is a stat, so this costs nothing in the common case.
        crate::rpc_patches::apply_on_launch();
        let args = if apply_default {
            SessionDefault::load().cli_args()
        } else {
            Vec::new()
        };
        let env_refs: Vec<(&str, &str)> = env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let extensions = self.extension_paths();
        PiClient::spawn_with_extensions_and_env_and_args(
            workspace,
            None,
            &extensions,
            &env_refs,
            &args,
        )
    }

    /// The installed bundled extension entry points, in load order.
    fn extension_paths(&self) -> Vec<PathBuf> {
        [
            &self.quota,
            &self.guard,
            &self.title,
            &self.workflow,
            &self.subagent,
        ]
        .into_iter()
        .flatten()
        .cloned()
        .collect()
    }
}

/// Write a multi-file extension into its own `<home>/.orbit-pi/<slug>/`
/// directory and return the entry file. `None` on any filesystem failure.
fn install_extension(home: &Path, slug: &str, files: &[(&str, &str)]) -> Option<PathBuf> {
    let dir = home.join(".orbit-pi").join(slug);
    fs::create_dir_all(&dir).ok()?;
    let mut entry = None;
    for (name, contents) in files {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).ok()?;
        }
        write_if_changed(&path, contents).ok()?;
        if *name == "index.js" || *name == "index.ts" {
            entry = Some(path);
        }
    }
    entry
}

fn install_quota_bridge() -> Option<PathBuf> {
    install_extension(
        &home_dir()?,
        "quota-extension",
        &[
            ("index.js", QUOTA_INDEX_JS),
            ("adapters.js", QUOTA_ADAPTERS_JS),
        ],
    )
}

fn install_guard() -> Option<PathBuf> {
    install_extension(
        &home_dir()?,
        "guard-extension",
        &[("index.js", GUARD_INDEX_JS), ("policy.js", GUARD_POLICY_JS)],
    )
}

fn install_title_extension() -> Option<PathBuf> {
    install_extension(
        &home_dir()?,
        "title-extension",
        &[("index.js", TITLE_INDEX_JS), ("title.js", TITLE_HELPERS_JS)],
    )
}

fn install_workflow_extension() -> Option<PathBuf> {
    install_extension(
        &home_dir()?,
        "workflow-extension",
        &[
            ("index.js", WORKFLOW_INDEX_JS),
            ("policy.js", WORKFLOW_POLICY_JS),
        ],
    )
}

fn install_subagent_extension() -> Option<PathBuf> {
    install_extension(
        &home_dir()?,
        "subagent-extension",
        &[
            ("index.js", SUBAGENT_INDEX_JS),
            ("agents.js", SUBAGENT_AGENTS_JS),
            ("lib.js", SUBAGENT_LIB_JS),
            ("agents/scout.md", SUBAGENT_SCOUT_MD),
            ("agents/planner.md", SUBAGENT_PLANNER_MD),
            ("agents/reviewer.md", SUBAGENT_REVIEWER_MD),
            ("agents/worker.md", SUBAGENT_WORKER_MD),
        ],
    )
}

fn home_dir() -> Option<PathBuf> {
    crate::platform::home_dir_opt()
}

/// Write only when the bytes differ, so relaunches don't churn mtimes (and a
/// running pi process is never surprised mid-session).
fn write_if_changed(path: &Path, contents: &str) -> std::io::Result<()> {
    if fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return Ok(());
    }
    fs::write(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        let unique = format!(
            "orbit-extensions-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(unique);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_only_when_contents_change() {
        let dir = scratch_dir();
        let path = dir.join("index.js");
        write_if_changed(&path, "one").unwrap();
        let first = fs::metadata(&path).unwrap().modified().unwrap();
        write_if_changed(&path, "one").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), first);
        write_if_changed(&path, "two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn installs_both_files_and_returns_the_entry() {
        let dir = scratch_dir();
        let entry = install_extension(
            &dir,
            "demo-extension",
            &[("index.js", "// entry"), ("helper.js", "// helper")],
        )
        .expect("install writes files");
        assert!(entry.ends_with("demo-extension/index.js"));
        assert!(entry.exists());
        assert!(entry.with_file_name("helper.js").exists());
        // Nested files create their parent directories.
        let nested = install_extension(
            &dir,
            "nested-extension",
            &[("index.js", "// entry"), ("agents/scout.md", "agent")],
        )
        .expect("nested install writes files");
        assert!(nested.ends_with("nested-extension/index.js"));
        assert!(nested.with_file_name("agents/scout.md").exists());
        // A directory named without an `index.js` yields no entry point.
        assert!(install_extension(&dir, "no-entry", &[("other.js", "x")]).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn title_extension_ships_its_entry_and_helpers() {
        // The embedded sources are real JS entry points, and the entry's
        // sibling `title.js` import resolves next to it.
        assert!(TITLE_INDEX_JS.contains("agent_settled"));
        assert!(TITLE_INDEX_JS.contains("./title.js"));
        assert!(TITLE_HELPERS_JS.contains("buildTitlePrompt"));
        let dir = scratch_dir();
        let entry = install_extension(
            &dir,
            "title-extension",
            &[("index.js", TITLE_INDEX_JS), ("title.js", TITLE_HELPERS_JS)],
        )
        .expect("title extension installs");
        assert!(entry.ends_with("title-extension/index.js"));
        assert!(entry.with_file_name("title.js").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn subagent_extension_ships_its_entry_helpers_and_agents() {
        // The entry imports its siblings and registers the tool under the name
        // the transcript already paints with a Task icon.
        assert!(SUBAGENT_INDEX_JS.contains("registerTool"));
        assert!(SUBAGENT_INDEX_JS.contains("./agents.js"));
        assert!(SUBAGENT_INDEX_JS.contains("./lib.js"));
        assert!(SUBAGENT_INDEX_JS.contains("name: \"subagent\""));
        assert!(SUBAGENT_AGENTS_JS.contains("discoverAgents"));
        assert!(SUBAGENT_LIB_JS.contains("mapWithConcurrencyLimit"));
        for (slug, source) in [
            ("scout", SUBAGENT_SCOUT_MD),
            ("planner", SUBAGENT_PLANNER_MD),
            ("reviewer", SUBAGENT_REVIEWER_MD),
            ("worker", SUBAGENT_WORKER_MD),
        ] {
            assert!(source.contains(&format!("name: {slug}")));
            assert!(source.contains("---"));
        }

        let dir = scratch_dir();
        let entry = install_subagent_extension_at(&dir).expect("subagent extension installs");
        assert!(entry.ends_with("subagent-extension/index.js"));
        assert!(entry.with_file_name("agents.js").exists());
        assert!(entry.with_file_name("lib.js").exists());
        assert!(entry.with_file_name("agents/scout.md").exists());
        assert!(entry.with_file_name("agents/worker.md").exists());
        fs::remove_dir_all(&dir).ok();
    }

    /// Install the embedded subagent extension under a scratch home so the
    /// test never touches the real `~/.orbit-pi`.
    fn install_subagent_extension_at(dir: &Path) -> Option<PathBuf> {
        install_extension(
            dir,
            "subagent-extension",
            &[
                ("index.js", SUBAGENT_INDEX_JS),
                ("agents.js", SUBAGENT_AGENTS_JS),
                ("lib.js", SUBAGENT_LIB_JS),
                ("agents/scout.md", SUBAGENT_SCOUT_MD),
                ("agents/planner.md", SUBAGENT_PLANNER_MD),
                ("agents/reviewer.md", SUBAGENT_REVIEWER_MD),
                ("agents/worker.md", SUBAGENT_WORKER_MD),
            ],
        )
    }
}
