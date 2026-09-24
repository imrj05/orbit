//! Workflow mode — what a session is *for*, and its plan progress.
//!
//! Orbit sessions are scoped as **Plan** (explore and produce a plan, change
//! nothing), **Build** (normal agent work — the default), or **Ask** (read-only
//! question answering). This is orthogonal to [`crate::access`]: access mode
//! answers "may this mutating call run?", workflow mode answers "what is this
//! session for?".
//!
//! Workflow state is **per session**, keyed by pi's session id in
//! `~/.orbit-pi/workflow.json`:
//!
//! ```json
//! { "0192…-uuid": "plan", "0193…-uuid": "build" }
//! ```
//!
//! The bundled workflow extension (`contrib/orbit-workflow-extension/`) reads
//! that file fresh on every agent hook, so changing the mode re-arms a live
//! session with no restart — the same contract the access guard relies on.
//!
//! Plan progress rides a second channel, mirroring the quota bridge: the
//! extension parses the plan and `[DONE:n]` markers, appends an
//! `orbit:workflow-todos` custom session entry whenever the snapshot changes,
//! and [`WorkflowTodos`] reduces those entries into UI state. Orbit parses no
//! plan text itself — the extension owns that truth.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// Custom session-entry type the workflow extension appends. The payload is
/// `{"todos": [{"step": u32, "text": str, "done": bool}, …]}`.
pub const TODO_ENTRY_TYPE: &str = "orbit:workflow-todos";

/// The workflow a session is scoped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkflowMode {
    /// Explore and produce a plan; mutating tools are disabled.
    Plan,
    /// Normal agent work. The default, so existing sessions are unchanged.
    #[default]
    Build,
    /// Answer questions from the codebase; mutating tools are disabled.
    Ask,
}

impl WorkflowMode {
    /// Every mode, in picker order.
    pub const ALL: [WorkflowMode; 3] = [Self::Plan, Self::Build, Self::Ask];

    /// The wire/persistence id. Kept in sync with `MODES` in the extension's
    /// `policy.js`.
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Build => "build",
            Self::Ask => "ask",
        }
    }

    /// Parse a stored id, falling back to [`WorkflowMode::default`] (Build) on
    /// anything unknown — a hand-edited file never widens what a session may do.
    pub fn from_wire(value: &str) -> Self {
        match value {
            "plan" => Self::Plan,
            "build" => Self::Build,
            "ask" => Self::Ask,
            _ => Self::default(),
        }
    }

    /// Short label for the composer chip and picker rows.
    pub fn label(self) -> String {
        match self {
            Self::Plan => tr!("workflow.plan"),
            Self::Build => tr!("workflow.build"),
            Self::Ask => tr!("workflow.ask"),
        }
    }

    /// One-line explanation shown under the label in the picker.
    pub fn description(self) -> String {
        match self {
            Self::Plan => tr!("workflow.plan_hint"),
            Self::Build => tr!("workflow.build_hint"),
            Self::Ask => tr!("workflow.ask_hint"),
        }
    }

    /// The bundled icon path for the chip and picker row.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Plan => "icons/tools/todo.svg",
            Self::Build => "icons/rocket-01.svg",
            Self::Ask => "icons/tools/ask.svg",
        }
    }

    /// Whether the mode keeps the agent read-only (Plan and Ask). The
    /// extension disables `edit`/`write` and gates bash for these.
    pub fn is_read_only(self) -> bool {
        matches!(self, Self::Plan | Self::Ask)
    }

    /// Whether the bottom todo bar applies to this mode. Ask answers questions
    /// and has no plan to track.
    pub fn tracks_todos(self) -> bool {
        matches!(self, Self::Plan | Self::Build)
    }
}

// ── per-session store ───────────────────────────────────────────────────────

/// `~/.orbit-pi/workflow.json` — session id → wire id.
fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("workflow.json")
}

fn load_map_at(path: &Path) -> BTreeMap<String, String> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    serde_json::from_str::<BTreeMap<String, String>>(&raw).unwrap_or_default()
}

fn write_map_at(path: &Path, map: &BTreeMap<String, String>) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(body) = serde_json::to_string(map) else {
        return;
    };
    // Temp-file + rename: the extension reads this file fresh while a pi
    // process is running, so a reader must never observe a partial write.
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

/// Load the mode recorded for a session, falling back to Build.
pub fn load_for(session_id: &str) -> WorkflowMode {
    load_map_at(&store_path())
        .get(session_id)
        .map(|wire| WorkflowMode::from_wire(wire))
        .unwrap_or_default()
}

/// Record the mode for a session. The workflow extension reads the file fresh
/// on every hook, so a write re-arms a live session.
pub fn persist_for(session_id: &str, mode: WorkflowMode) {
    let path = store_path();
    let mut map = load_map_at(&path);
    if map.get(session_id).map(String::as_str) == Some(mode.as_wire()) {
        return;
    }
    map.insert(session_id.to_string(), mode.as_wire().to_string());
    write_map_at(&path, &map);
}

/// Drop entries whose session no longer exists, so the store cannot grow
/// without bound as sessions are deleted.
pub fn prune(known_session_ids: &[String]) {
    let path = store_path();
    let mut map = load_map_at(&path);
    let before = map.len();
    map.retain(|id, _| known_session_ids.iter().any(|known| known == id));
    if map.len() != before {
        write_map_at(&path, &map);
    }
}

// ── plan progress ───────────────────────────────────────────────────────────

/// One step of a session's plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowTodo {
    /// The step number the model tags with `[DONE:n]` (1-based).
    pub step: u32,
    /// The step text, already cleaned by the extension.
    pub text: String,
    /// Whether the extension has seen a completing marker for this step.
    pub done: bool,
}

/// Reducer over the extension's `orbit:workflow-todos` session entries.
///
/// Like [`crate::quota::QuotaManager`], this performs no I/O: the GPUI layer
/// polls `get_entries` and hands the slice here. The newest matching entry
/// wins, because the extension appends one entry per changed snapshot.
#[derive(Default)]
pub struct WorkflowTodos {
    todos: Vec<WorkflowTodo>,
    session: Option<String>,
}

impl WorkflowTodos {
    /// Point the reducer at a session, clearing state when it actually
    /// changes so one session's plan never paints on another.
    pub fn reset_for(&mut self, session: Option<&str>) {
        if self.session.as_deref() != session {
            self.session = session.map(str::to_string);
            self.todos.clear();
        }
    }

    /// Merge the newest bridge snapshot from a `get_entries` response. Returns
    /// `true` when the list changed and the caller should repaint.
    pub fn on_entries(&mut self, entries: &[Value]) -> bool {
        let snapshot = entries.iter().rev().find(|entry| {
            entry.get("type").and_then(Value::as_str) == Some("custom")
                && entry.get("customType").and_then(Value::as_str) == Some(TODO_ENTRY_TYPE)
        });
        let Some(data) = snapshot.and_then(|entry| entry.get("data")) else {
            return false;
        };
        let todos = parse_todos(data);
        if todos == self.todos {
            return false;
        }
        self.todos = todos;
        true
    }

    pub fn todos(&self) -> &[WorkflowTodo] {
        &self.todos
    }

    pub fn is_empty(&self) -> bool {
        self.todos.is_empty()
    }

    /// `(done, total)` — the numbers behind the progress bar.
    pub fn progress(&self) -> (usize, usize) {
        let done = self.todos.iter().filter(|todo| todo.done).count();
        (done, self.todos.len())
    }

    /// The first not-done step — what the collapsed bar names as "Next".
    pub fn current(&self) -> Option<&WorkflowTodo> {
        self.todos.iter().find(|todo| !todo.done)
    }

    /// Whether every step is complete (and there is at least one).
    pub fn all_done(&self) -> bool {
        !self.todos.is_empty() && self.todos.iter().all(|todo| todo.done)
    }
}

fn parse_todos(data: &Value) -> Vec<WorkflowTodo> {
    data.get("todos")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let text = item.get("text")?.as_str()?.to_string();
                    let step = item.get("step").and_then(Value::as_u64).unwrap_or(0) as u32;
                    let done = item.get("done").and_then(Value::as_bool).unwrap_or(false);
                    Some(WorkflowTodo { step, text, done })
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_path(tag: &str) -> PathBuf {
        let unique = format!(
            "orbit-workflow-test-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        std::env::temp_dir().join(unique).join("workflow.json")
    }

    #[test]
    fn wire_round_trips() {
        for mode in WorkflowMode::ALL {
            assert_eq!(WorkflowMode::from_wire(mode.as_wire()), mode);
        }
    }

    #[test]
    fn unknown_wire_falls_back_to_build() {
        assert_eq!(WorkflowMode::from_wire("yolo"), WorkflowMode::Build);
        assert_eq!(WorkflowMode::from_wire(""), WorkflowMode::Build);
        assert_eq!(WorkflowMode::default(), WorkflowMode::Build);
    }

    #[test]
    fn every_mode_has_distinct_copy() {
        let labels: std::collections::HashSet<_> =
            WorkflowMode::ALL.iter().map(|m| m.label()).collect();
        assert_eq!(labels.len(), WorkflowMode::ALL.len());
        for mode in WorkflowMode::ALL {
            assert!(!mode.description().is_empty());
            assert!(mode.icon().starts_with("icons/"));
        }
    }

    #[test]
    fn read_only_and_todo_flags() {
        assert!(WorkflowMode::Plan.is_read_only());
        assert!(WorkflowMode::Ask.is_read_only());
        assert!(!WorkflowMode::Build.is_read_only());
        assert!(WorkflowMode::Plan.tracks_todos());
        assert!(WorkflowMode::Build.tracks_todos());
        assert!(!WorkflowMode::Ask.tracks_todos());
    }

    #[test]
    fn store_round_trips_per_session() {
        let path = temp_path("store");
        let mut map = load_map_at(&path);
        map.insert("s1".into(), "plan".into());
        map.insert("s2".into(), "build".into());
        write_map_at(&path, &map);

        let read = load_map_at(&path);
        assert_eq!(read.get("s1").map(String::as_str), Some("plan"));
        assert_eq!(read.get("s2").map(String::as_str), Some("build"));
        assert_eq!(WorkflowMode::from_wire(read.get("s1").unwrap()), WorkflowMode::Plan);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn parse_todos_reads_the_snapshot() {
        let data = json!({
            "todos": [
                {"step": 1, "text": "First", "done": true},
                {"step": 2, "text": "Second", "done": false},
            ]
        });
        let todos = parse_todos(&data);
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0], WorkflowTodo { step: 1, text: "First".into(), done: true });
        // A malformed item is skipped, not fatal.
        assert!(parse_todos(&json!({"todos": [{"text": "no step"}]})).len() == 1);
        assert!(parse_todos(&json!({"todos": "nope"})).is_empty());
    }

    fn entry(todos: Value) -> Value {
        json!({"type": "custom", "customType": TODO_ENTRY_TYPE, "data": {"todos": todos}})
    }

    #[test]
    fn on_entries_takes_the_newest_and_reports_change() {
        let mut reducer = WorkflowTodos::default();
        assert!(reducer.on_entries(&[entry(json!([
            {"step": 1, "text": "One", "done": false}
        ]))]));
        assert_eq!(reducer.progress(), (0, 1));
        assert_eq!(reducer.current().map(|t| t.text.as_str()), Some("One"));

        // The newest matching entry wins.
        assert!(reducer.on_entries(&[
            entry(json!([{"step": 1, "text": "One", "done": false}])),
            entry(json!([
                {"step": 1, "text": "One", "done": true},
                {"step": 2, "text": "Two", "done": false}
            ])),
        ]));
        assert_eq!(reducer.progress(), (1, 2));
        assert!(!reducer.all_done());

        // An identical snapshot is not a change.
        assert!(!reducer.on_entries(&[entry(json!([
            {"step": 1, "text": "One", "done": true},
            {"step": 2, "text": "Two", "done": false}
        ]))]));

        // No matching entry leaves the state alone.
        assert!(!reducer.on_entries(&[json!({"type": "message"})]));
    }

    #[test]
    fn reset_for_clears_on_session_change() {
        let mut reducer = WorkflowTodos::default();
        reducer.reset_for(Some("s1"));
        reducer.on_entries(&[entry(json!([{"step": 1, "text": "One", "done": false}]))]);
        assert!(!reducer.is_empty());
        // Same session keeps it; a new session clears it.
        reducer.reset_for(Some("s1"));
        assert!(!reducer.is_empty());
        reducer.reset_for(Some("s2"));
        assert!(reducer.is_empty());
    }

    #[test]
    fn all_done_requires_a_nonempty_list() {
        let mut reducer = WorkflowTodos::default();
        assert!(!reducer.all_done());
        reducer.on_entries(&[entry(json!([{"step": 1, "text": "One", "done": true}]))]);
        assert!(reducer.all_done());
    }
}
