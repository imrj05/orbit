//! Orbit's bundled pi extensions.
//!
//! Orbit ships six small pi extensions under `contrib/` and loads them with
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
//! - **subagents** (`contrib/orbit-subagents-extension/`) — registers the
//!   `subagent` tool: delegates a scoped task to a child `pi` process and
//!   returns only its report and cost. Every session spawn also exports the
//!   installed entry points as `ORBIT_BUNDLED_EXTENSIONS`, which the subagent
//!   tool forwards with `--extension` so a child inherits the guardrails.
//!
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
const SUGGESTIONS_INDEX_JS: &str =
    include_str!("../../../contrib/orbit-suggestions-extension/index.js");
const TITLE_INDEX_JS: &str = include_str!("../../../contrib/orbit-title-extension/index.js");
const TITLE_HELPERS_JS: &str = include_str!("../../../contrib/orbit-title-extension/title.js");
const WORKFLOW_INDEX_JS: &str = include_str!("../../../contrib/orbit-workflow-extension/index.js");
const WORKFLOW_POLICY_JS: &str =
    include_str!("../../../contrib/orbit-workflow-extension/policy.js");
const SUBAGENTS_INDEX_JS: &str =
    include_str!("../../../contrib/orbit-subagents-extension/index.js");
const SUBAGENTS_AGENTS_JS: &str =
    include_str!("../../../contrib/orbit-subagents-extension/agents.js");
const SUBAGENTS_RUN_JS: &str = include_str!("../../../contrib/orbit-subagents-extension/run.js");
const SUBAGENTS_MODE_JS: &str = include_str!("../../../contrib/orbit-subagents-extension/mode.js");
const SUBAGENTS_RESOURCES_JS: &str =
    include_str!("../../../contrib/orbit-subagents-extension/resources.js");
const SUBAGENTS_BUILTINS_JSON: &str =
    include_str!("../../../contrib/orbit-subagents-extension/builtins.json");

/// The subagent extension's files. `builtins.json` ships beside the entry
/// because the extension reads it with `import.meta.url`, and Orbit embeds the
/// same file so Settings can show and reset the shipped definitions (D-X).
const SUBAGENTS_FILES: &[(&str, &str)] = &[
    ("index.js", SUBAGENTS_INDEX_JS),
    ("agents.js", SUBAGENTS_AGENTS_JS),
    ("run.js", SUBAGENTS_RUN_JS),
    ("mode.js", SUBAGENTS_MODE_JS),
    ("resources.js", SUBAGENTS_RESOURCES_JS),
    ("builtins.json", SUBAGENTS_BUILTINS_JSON),
];

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
    suggestions: Option<PathBuf>,
    /// The workflow extension's `index.js`; `None` when it could not be
    /// written (sessions then run unscoped and the mode chip is honest).
    workflow: Option<PathBuf>,
    /// The subagent extension's `index.js`; `None` when it could not be
    /// written (the `subagent` tool is then absent, not broken).
    subagents: Option<PathBuf>,
}

impl BundledExtensions {
    /// Materialize the bundled extensions under `~/.orbit-pi/`. Best-effort:
    /// a filesystem failure degrades to no extension, never a blocked launch.
    pub(crate) fn install() -> Self {
        Self {
            quota: install_quota_bridge(),
            guard: install_guard(),
            title: install_title_extension(),
            suggestions: install_suggestions_extension(),
            workflow: install_workflow_extension(),
            subagents: install_subagents_extension(),
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

    /// The two guardrail entry points as a JSON array for the spawn
    /// environment. Handed to each session as `ORBIT_BUNDLED_EXTENSIONS` so the
    /// subagent tool can forward them to a child (D-E).
    ///
    /// Only the guard and the workflow extension: a child needs the parent's
    /// permissions and its Plan/Build/Ask scope, and nothing else. Forwarding
    /// the quota, title and suggestions bridges would make every delegation
    /// fetch provider quota or spend a model call on a session title the user
    /// never sees.
    pub(crate) fn guardrails_json(&self) -> Option<String> {
        let guardrails: Vec<PathBuf> = [&self.guard, &self.workflow]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        bundled_extensions_env(&guardrails)
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
        let extensions = self.extension_paths();
        let bundled = self.guardrails_json();
        let mut env_refs: Vec<(&str, &str)> = env
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        // A child session does not inherit extensions implicitly: the subagent
        // tool reads this list and passes each path on as `--extension`.
        if let Some(bundled) = bundled.as_deref() {
            env_refs.push(("ORBIT_BUNDLED_EXTENSIONS", bundled));
        }
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
            &self.suggestions,
            &self.workflow,
            &self.subagents,
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
        write_if_changed(&dir.join(name), contents).ok()?;
        if *name == "index.js" {
            entry = Some(dir.join(name));
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

fn install_suggestions_extension() -> Option<PathBuf> {
    install_extension(
        &home_dir()?,
        "suggestions-extension",
        &[("index.js", SUGGESTIONS_INDEX_JS)],
    )
}

fn install_subagents_extension() -> Option<PathBuf> {
    install_extension(&home_dir()?, "subagents-extension", SUBAGENTS_FILES)
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

fn home_dir() -> Option<PathBuf> {
    crate::platform::home_dir_opt()
}

/// The installed entry points as a JSON array for `ORBIT_BUNDLED_EXTENSIONS`.
/// `None` when nothing installed — the variable is then simply absent, which
/// the subagent tool treats the same as an empty list.
fn bundled_extensions_env(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let strings: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    serde_json::to_string(&strings).ok()
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
        // A directory named without an `index.js` yields no entry point.
        assert!(install_extension(&dir, "no-entry", &[("other.js", "x")]).is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn subagents_extension_ships_every_file_it_imports() {
        // The entry imports five siblings and reads `builtins.json` beside
        // itself; every one of them must be in the embedded set, or a child
        // would fail to load the guardrail extension at all.
        for name in [
            "agents.js",
            "run.js",
            "mode.js",
            "resources.js",
            "builtins.json",
        ] {
            assert!(
                SUBAGENTS_FILES.iter().any(|(file, _)| *file == name),
                "{name} is installed next to index.js"
            );
        }
        assert!(SUBAGENTS_INDEX_JS.contains("./agents.js"));
        assert!(SUBAGENTS_INDEX_JS.contains("./run.js"));
        assert!(SUBAGENTS_INDEX_JS.contains("./mode.js"));
        assert!(SUBAGENTS_INDEX_JS.contains("./resources.js"));
        assert!(SUBAGENTS_AGENTS_JS.contains("./builtins.json"));

        // The tool name and the details version are the contract the card
        // parses; renaming either is a protocol change.
        assert!(SUBAGENTS_INDEX_JS.contains("\"subagent\""));
        assert!(SUBAGENTS_RUN_JS.contains("subagent/1"));
        // The child is a json-mode process with no session to clean up.
        assert!(SUBAGENTS_RUN_JS.contains("\"--mode\", \"json\""));
        assert!(SUBAGENTS_RUN_JS.contains("--no-session"));
        assert!(SUBAGENTS_RUN_JS.contains("--append-system-prompt"));
    }

    #[test]
    fn installs_the_subagents_extension_and_returns_its_entry() {
        let dir = scratch_dir();
        let entry = install_extension(&dir, "subagents-extension", SUBAGENTS_FILES)
            .expect("install writes files");
        assert!(entry.ends_with("subagents-extension/index.js"));
        assert!(entry.exists());
        for name in [
            "agents.js",
            "run.js",
            "mode.js",
            "resources.js",
            "builtins.json",
        ] {
            assert!(entry.with_file_name(name).exists(), "{name} is installed");
        }
        // The shipped builtins must parse as JSON: the page embeds this file.
        let builtins: serde_json::Value =
            serde_json::from_str(SUBAGENTS_BUILTINS_JSON).expect("builtins.json is valid JSON");
        let agents = builtins["agents"].as_array().expect("agents array");
        assert_eq!(agents.len(), 3);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_spawn_env_lists_only_the_guardrail_extensions() {
        let paths = vec![
            PathBuf::from("/home/u/.orbit-pi/guard-extension/index.js"),
            PathBuf::from("/home/u/.orbit-pi/workflow-extension/index.js"),
        ];
        let json = bundled_extensions_env(&paths).expect("non-empty list");
        let parsed: Vec<String> = serde_json::from_str(&json).expect("json array");
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].ends_with("guard-extension/index.js"));
        assert!(parsed[1].ends_with("workflow-extension/index.js"));
        assert!(bundled_extensions_env(&[]).is_none());
    }

    #[test]
    fn title_extension_ships_its_entry_and_helpers() {
        // The embedded sources are real JS entry points, and the entry's
        // sibling `title.js` import resolves next to it.
        assert!(TITLE_INDEX_JS.contains("agent_settled"));
        assert!(TITLE_INDEX_JS.contains("./title.js"));
        assert!(TITLE_HELPERS_JS.contains("buildTitlePrompt"));
        // The automatic pass decides from the session's *name*, not from how
        // many prompts it holds: a resumed untitled session, or one whose first
        // run took a steering message, still gets a title (see CHANGELOG).
        assert!(!TITLE_INDEX_JS.contains("userCount !== 1"));
        assert!(TITLE_INDEX_JS.contains("MAX_ASK_ATTEMPTS"));
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
}
