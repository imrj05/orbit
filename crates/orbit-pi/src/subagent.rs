//! The subagent delegation card's data: a lenient parser for the `details`
//! payload the `subagent` tool attaches, plus the phrases the card and the
//! parent's working line read.
//!
//! Pure and I/O-free, like [`crate::transcript`]: the payload is the contract
//! (see `docs/plans/subagents.md` §6.2), and anything this module does not
//! recognise must degrade — a foreign or newer payload returns `None` so the
//! call renders as the generic activity card, never as a broken row.
//!
//! Nothing here is inferred from the child's output text. The status, phase,
//! counts and cost are exactly what the extension stamped or pi reported; when
//! a fact is absent it stays absent.

use serde_json::Value;

use orbit_rpc::MessageUsage;

use crate::transcript::ToolCall;

/// The `details` version this parser understands. An unknown version is not
/// guessed at: the card falls back.
pub(crate) const DETAILS_VERSION: &str = "subagent/1";

/// Whether `name` is the delegation tool.
pub(crate) fn is_subagent_tool(name: &str) -> bool {
    name.eq_ignore_ascii_case("subagent")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Single,
    Parallel,
    Chain,
}

impl Mode {
    fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("parallel") => Self::Parallel,
            Some("chain") => Self::Chain,
            _ => Self::Single,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Status {
    #[default]
    Running,
    Done,
    Failed,
    Aborted,
}

impl Status {
    fn parse(raw: Option<&str>) -> Option<Self> {
        match raw {
            Some("running") => Some(Self::Running),
            Some("done") => Some(Self::Done),
            Some("failed") => Some(Self::Failed),
            Some("aborted") => Some(Self::Aborted),
            _ => None,
        }
    }

    fn is_live(self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Phase {
    Queued,
    Spawning,
    #[default]
    Thinking,
    Tool,
    Blocked,
    Settled,
    Killed,
    Refused,
}

impl Phase {
    fn parse(raw: Option<&str>, ended: bool) -> Option<Self> {
        match raw {
            Some("queued") => Some(Self::Queued),
            Some("spawning") => Some(Self::Spawning),
            Some("thinking") => Some(Self::Thinking),
            Some("tool") => Some(Self::Tool),
            Some("blocked") => Some(Self::Blocked),
            Some("settled") => Some(Self::Settled),
            Some("killed") => Some(Self::Killed),
            Some("refused") => Some(Self::Refused),
            // A future version's phase is not guessed at; a finished child is
            // settled, an unfinished one is somewhere in the run.
            _ if ended => Some(Self::Settled),
            _ => Some(Self::Thinking),
        }
    }
}

/// How one of a child's tool calls ended, as the extension recorded it from
/// pi's `tool_execution_end`. `Unknown` is an older payload that never said.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum StepStatus {
    #[default]
    Unknown,
    Running,
    Ok,
    /// The tool ran and reported an error (a red test, a grep with no match).
    Error,
    /// A guard refused the call before it ran.
    Blocked,
}

impl StepStatus {
    fn parse(raw: Option<&str>) -> Self {
        match raw {
            Some("running") => Self::Running,
            Some("ok") => Self::Ok,
            Some("error") => Self::Error,
            Some("blocked") => Self::Blocked,
            _ => Self::Unknown,
        }
    }
}

/// One tool call a child made, as much as the card shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubagentStep {
    pub kind: String,
    pub name: String,
    pub preview: String,
    pub status: StepStatus,
}

/// How long a live child may go without a single event before the card says
/// so instead of repeating its last phase (§9.4, D-V).
pub(crate) const SILENCE_AFTER_MS: i64 = 15_000;

/// The reason older extensions stamped on *every* failed tool call. Those
/// entries were the work's own failures, not a guard's refusal, so they are
/// not blocked calls and the card does not report them as one.
const LEGACY_FAILED_REASON: &str = "Tool failed";

/// One denied call, with the reason the guard gave.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlockedCall {
    pub tool: String,
    pub target: Option<String>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SubagentChild {
    pub agent: String,
    pub status: Status,
    pub phase: Phase,
    /// `grep /auth/ in src/` — what the child is doing *now*.
    pub current: Option<String>,
    pub tools: u64,
    /// ≤12 entries, most used first.
    pub tool_counts: Vec<(String, u64)>,
    /// ≤5, most recent last.
    pub blocked: Vec<BlockedCall>,
    /// `read` | `edit` | `exec` | `all`.
    pub capability: Option<String>,
    pub workflow_mode: Option<String>,
    pub model: Option<String>,
    /// The thinking level the child ran at, as pi reported it.
    pub thinking: Option<String>,
    pub task: String,
    pub steps: Vec<SubagentStep>,
    pub steps_total: u64,
    pub report: Option<String>,
    pub error: Option<String>,
    /// The child's own stderr (the extension caps it at 2 KiB). Machine output:
    /// shown as-is beneath a failure's sentence, never reinterpreted.
    pub stderr_tail: Option<String>,
    pub usage: MessageUsage,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    /// When the extension last saw any event from the child.
    pub last_event_at: Option<i64>,
    /// The child was stopped by its time budget, not by the user.
    pub timed_out: bool,
}

impl SubagentChild {
    fn from_value(value: &Value) -> Self {
        let ended_at = value.get("endedAt").and_then(Value::as_i64);
        let status = Status::parse(value.get("status").and_then(Value::as_str)).unwrap_or(
            if ended_at.is_some() {
                Status::Done
            } else {
                Status::Running
            },
        );
        let phase = Phase::parse(
            value.get("phase").and_then(Value::as_str),
            ended_at.is_some(),
        )
        .unwrap_or_default();
        let current = value
            .get("current")
            .and_then(Value::as_str)
            .filter(|current| !current.trim().is_empty())
            .map(str::to_string);
        let tool_counts = value
            .get("toolCounts")
            .and_then(Value::as_object)
            .map(|counts| {
                let mut entries: Vec<(String, u64)> = counts
                    .iter()
                    .filter_map(|(name, count)| Some((name.clone(), count.as_u64()?)))
                    .collect();
                entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                entries.truncate(12);
                entries
            })
            .unwrap_or_default();
        let blocked = value
            .get("blocked")
            .and_then(Value::as_array)
            .map(|calls| {
                calls
                    .iter()
                    .filter_map(|call| {
                        let tool = call.get("tool").and_then(Value::as_str)?;
                        let reason = call.get("reason").and_then(Value::as_str);
                        if reason == Some(LEGACY_FAILED_REASON) {
                            return None;
                        }
                        Some(BlockedCall {
                            tool: tool.to_string(),
                            target: call
                                .get("target")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            reason: call
                                .get("reason")
                                .and_then(Value::as_str)
                                .unwrap_or("Blocked")
                                .to_string(),
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let steps = value
            .get("steps")
            .and_then(Value::as_array)
            .map(|steps| {
                steps
                    .iter()
                    .map(|step| SubagentStep {
                        kind: step
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("tool")
                            .to_string(),
                        name: step
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        preview: step
                            .get("preview")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                        status: StepStatus::parse(step.get("status").and_then(Value::as_str)),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            agent: value
                .get("agent")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            status,
            phase,
            current,
            tools: value.get("tools").and_then(Value::as_u64).unwrap_or(0),
            tool_counts,
            blocked,
            capability: value
                .get("capability")
                .and_then(Value::as_str)
                .map(str::to_string),
            workflow_mode: value
                .get("workflowMode")
                .and_then(Value::as_str)
                .map(str::to_string),
            model: value
                .get("model")
                .and_then(Value::as_str)
                .filter(|model| !model.trim().is_empty())
                .map(str::to_string),
            thinking: value
                .get("thinking")
                .and_then(Value::as_str)
                .filter(|level| !level.trim().is_empty())
                .map(str::to_string),
            task: value
                .get("task")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            steps,
            steps_total: value.get("stepsTotal").and_then(Value::as_u64).unwrap_or(0),
            report: value
                .get("report")
                .and_then(Value::as_str)
                .filter(|report| !report.trim().is_empty())
                .map(str::to_string),
            error: value
                .get("error")
                .and_then(Value::as_str)
                .filter(|error| !error.trim().is_empty())
                .map(str::to_string),
            stderr_tail: value
                .get("stderrTail")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|tail| !tail.is_empty())
                .map(str::to_string),
            usage: MessageUsage::from_value(value.get("usage")).unwrap_or_else(zero_usage),
            started_at: value.get("startedAt").and_then(Value::as_i64),
            ended_at,
            last_event_at: value.get("lastEventAt").and_then(Value::as_i64),
            timed_out: value
                .get("timedOut")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    /// The agent's name, or a neutral label when the payload omitted it.
    pub(crate) fn label(&self) -> &str {
        if self.agent.is_empty() {
            "subagent"
        } else {
            &self.agent
        }
    }

    /// Whether this child is still working.
    pub(crate) fn is_live(&self) -> bool {
        self.status.is_live()
    }

    /// The model's display name: the provider namespace dropped, so a row
    /// narrow enough to truncate keeps the part that tells models apart
    /// (`anthropic/claude-haiku-4-5` → `claude-haiku-4-5`).
    pub(crate) fn model_label(&self) -> Option<String> {
        self.model.as_deref().map(crate::usage::model::model_label)
    }

    /// How long a live child has produced no event, once that passes
    /// [`SILENCE_AFTER_MS`]. Only while it is starting or thinking: a running
    /// tool is quiet by nature (a long `cargo build`), and the row already
    /// names that tool. `None` when the payload carried no timestamp.
    pub(crate) fn silent_for_ms(&self, now_ms: i64) -> Option<i64> {
        if !self.is_live() || !matches!(self.phase, Phase::Thinking | Phase::Spawning) {
            return None;
        }
        let quiet = now_ms - self.last_event_at?;
        (quiet >= SILENCE_AFTER_MS).then_some(quiet)
    }
}

fn zero_usage() -> MessageUsage {
    MessageUsage {
        input: 0,
        output: 0,
        cache_read: 0,
        cache_write: 0,
        total: 0,
        cost: None,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SubagentRun {
    pub mode: Mode,
    pub children: Vec<SubagentChild>,
}

impl SubagentRun {
    /// Reads `tool.details`; `None` for a foreign tool, an unknown version, a
    /// non-object payload, or a payload with no children array.
    pub(crate) fn from_tool(tool: &ToolCall) -> Option<Self> {
        if !is_subagent_tool(&tool.name) {
            return None;
        }
        let details = tool.details.as_ref()?;
        if details.get("orbit").and_then(Value::as_str) != Some(DETAILS_VERSION) {
            return None;
        }
        let children = details
            .get("children")
            .and_then(Value::as_array)
            .map(|children| children.iter().map(SubagentChild::from_value).collect())
            .unwrap_or_default();
        Some(Self {
            mode: Mode::parse(details.get("mode").and_then(Value::as_str)),
            children,
        })
    }

    pub(crate) fn is_live(&self) -> bool {
        self.children.iter().any(SubagentChild::is_live)
    }

    pub(crate) fn total_tools(&self) -> u64 {
        self.children.iter().map(|child| child.tools).sum()
    }

    /// The run's wall clock: from its first child's start to its last child's
    /// end, or to `now` while any child runs. `None` when no child was ever
    /// stamped, so the footer never invents a clock.
    pub(crate) fn elapsed(&self, now_ms: i64) -> Option<String> {
        let started = self
            .children
            .iter()
            .filter_map(|child| child.started_at)
            .min()?;
        let ended = if self.is_live() {
            None
        } else {
            self.children
                .iter()
                .filter_map(|child| child.ended_at)
                .max()
        };
        format_elapsed(Some(started), ended, now_ms)
    }

    pub(crate) fn running(&self) -> usize {
        self.children.iter().filter(|child| child.is_live()).count()
    }

    /// The run's summed usage. `None` when no child reported any, so the card
    /// never shows `↑0 ↓0`.
    pub(crate) fn usage(&self) -> Option<MessageUsage> {
        let mut total: Option<MessageUsage> = None;
        for usage in self.children.iter().map(|child| &child.usage) {
            if usage.total == 0 && usage.cost.unwrap_or(0.0) <= 0.0 {
                continue;
            }
            match &mut total {
                Some(total) => total.add(usage),
                None => total = Some(usage.clone()),
            }
        }
        total
    }

    /// What the parent's working line should say (§9.5); `None` when nothing
    /// is running. Facts, not a sentence — the view owns the words, so they
    /// stay translatable.
    pub(crate) fn live_activity(&self) -> Option<Activity<'_>> {
        let running: Vec<&SubagentChild> = self
            .children
            .iter()
            .filter(|child| child.is_live())
            .collect();
        if running.is_empty() {
            return None;
        }
        if self.children.len() > 1 {
            return Some(Activity::Agents(running.len()));
        }
        let child = running[0];
        Some(Activity::Agent {
            agent: child.label(),
            action: child.current.as_deref(),
        })
    }
}

/// The live activity a running delegation reports, in pieces the view can
/// phrase in the reader's language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Activity<'a> {
    /// One agent, and the tool call it is on (`None` while it thinks).
    Agent {
        agent: &'a str,
        action: Option<&'a str>,
    },
    /// Several children working at once.
    Agents(usize),
}

/// `0:34` / `1:02:03`, from the timestamps the extension stamped. `None` when
/// no start was recorded, so the card never invents a clock.
pub(crate) fn format_elapsed(
    started_at: Option<i64>,
    ended_at: Option<i64>,
    now: i64,
) -> Option<String> {
    let started = started_at?;
    let end = ended_at.unwrap_or(now);
    let seconds = (end - started).max(0) / 1000;
    Some(format_duration(seconds as u64))
}

fn format_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            (seconds % 3600) / 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::transcript::ToolFacts;

    fn plain_tool(name: &str) -> ToolCall {
        ToolCall {
            name: name.to_string(),
            summary: String::new(),
            path: None,
            added: 0,
            removed: 0,
            id: None,
            args: None,
            output: None,
            failed: false,
            facts: ToolFacts::default(),
            details: None,
        }
    }

    fn tool(details: Value) -> ToolCall {
        ToolCall {
            details: Some(details),
            ..plain_tool("subagent")
        }
    }

    fn child(value: Value) -> SubagentChild {
        SubagentChild::from_value(&value)
    }

    #[test]
    fn parses_a_good_payload() {
        let run = SubagentRun::from_tool(&tool(json!({
            "orbit": "subagent/1",
            "mode": "parallel",
            "children": [{
                "agent": "scout",
                "status": "running",
                "phase": "tool",
                "current": "grep /auth/ in src/",
                "capability": "read",
                "workflowMode": "build",
                "model": "anthropic/claude-haiku-4-5",
                "task": "find auth",
                "tools": 12,
                "toolCounts": {"read": 4, "grep": 7, "bash": 1},
                "steps": [{"kind": "tool", "name": "grep", "preview": "auth in src/"}],
                "stepsTotal": 12,
                "report": "auth is in src/auth.rs",
                "usage": {"input": 3100, "output": 812, "cost": 0.0123}
            }]
        })))
        .expect("payload parses");
        assert_eq!(run.mode, Mode::Parallel);
        let child = &run.children[0];
        assert_eq!(child.agent, "scout");
        assert_eq!(child.status, Status::Running);
        assert_eq!(child.phase, Phase::Tool);
        assert_eq!(child.current.as_deref(), Some("grep /auth/ in src/"));
        assert_eq!(child.capability.as_deref(), Some("read"));
        assert_eq!(child.model_label().as_deref(), Some("claude-haiku-4-5"));
        assert_eq!(child.tool_counts[0], ("grep".to_string(), 7));
        assert_eq!(child.usage.input, 3100);
        assert_eq!(child.usage.cost, Some(0.0123));
    }

    #[test]
    fn a_foreign_payload_or_unknown_version_is_none() {
        assert!(SubagentRun::from_tool(&tool(json!({"mode": "single", "children": []}))).is_none());
        assert!(
            SubagentRun::from_tool(&tool(json!({"orbit": "subagent/2", "children": []}))).is_none()
        );
        assert!(SubagentRun::from_tool(&tool(json!("nope"))).is_none());
        let plain = plain_tool("subagent");
        assert!(SubagentRun::from_tool(&plain).is_none());
        let other = ToolCall {
            details: Some(json!({"orbit": "subagent/1"})),
            ..plain_tool("read")
        };
        assert!(SubagentRun::from_tool(&other).is_none());
    }

    #[test]
    fn missing_and_null_fields_never_panic_and_stay_absent() {
        let child = child(json!({
            "agent": null,
            "status": null,
            "phase": null,
            "current": null,
            "toolCounts": null,
            "blocked": null,
            "steps": null,
            "report": null,
            "error": null,
            "usage": null,
            "endedAt": null
        }));
        assert_eq!(child.label(), "subagent");
        assert!(child.current.is_none());
        assert!(child.report.is_none());
        assert!(child.error.is_none());
        assert!(child.tool_counts.is_empty());
        assert!(child.usage.total == 0);
        assert!(child.usage.cost.is_none());
    }

    #[test]
    fn an_absent_status_falls_back_to_the_clock_and_unknown_phases_degrade() {
        let finished = child(json!({"agent": "scout", "endedAt": 5}));
        assert_eq!(finished.status, Status::Done);
        assert_eq!(finished.phase, Phase::Settled);
        let live = child(json!({"agent": "scout"}));
        assert_eq!(live.status, Status::Running);
        assert_eq!(live.phase, Phase::Thinking);
        let future = child(json!({"agent": "scout", "phase": "teleporting", "endedAt": 5}));
        assert_eq!(future.phase, Phase::Settled);
    }

    #[test]
    fn live_activity_only_exists_while_a_child_runs() {
        let run = SubagentRun {
            mode: Mode::Single,
            children: vec![child(json!({
                "agent": "scout",
                "status": "running",
                "phase": "tool",
                "current": "grep /auth/ in src/"
            }))],
        };
        assert_eq!(
            run.live_activity(),
            Some(Activity::Agent {
                agent: "scout",
                action: Some("grep /auth/ in src/")
            })
        );

        let thinking = SubagentRun {
            mode: Mode::Single,
            children: vec![child(json!({"agent": "scout", "status": "running"}))],
        };
        assert_eq!(
            thinking.live_activity(),
            Some(Activity::Agent {
                agent: "scout",
                action: None
            })
        );

        let settled = SubagentRun {
            mode: Mode::Single,
            children: vec![child(
                json!({"agent": "scout", "status": "done", "endedAt": 1}),
            )],
        };
        assert_eq!(settled.live_activity(), None);

        let refused = SubagentRun {
            mode: Mode::Single,
            children: vec![child(json!({
                "agent": "worker",
                "status": "failed",
                "phase": "refused"
            }))],
        };
        assert_eq!(refused.live_activity(), None);
    }

    #[test]
    fn parallel_live_activity_counts_agents() {
        let run = SubagentRun {
            mode: Mode::Parallel,
            children: vec![
                child(json!({"agent": "a", "status": "running", "current": "read a"})),
                child(json!({"agent": "b", "status": "running", "current": "grep b"})),
                child(json!({"agent": "c", "status": "done", "endedAt": 1})),
            ],
        };
        assert_eq!(run.live_activity(), Some(Activity::Agents(2)));
        assert_eq!(run.running(), 2);
    }

    #[test]
    fn elapsed_is_read_from_the_stamped_timestamps() {
        assert_eq!(
            format_elapsed(Some(1_000), Some(35_000), 99_000).as_deref(),
            Some("0:34")
        );
        assert_eq!(
            format_elapsed(Some(0), Some(3_723_000), 0).as_deref(),
            Some("1:02:03")
        );
        // A live child measures to now; a missing start invents nothing.
        assert_eq!(
            format_elapsed(Some(1_000), None, 5_000).as_deref(),
            Some("0:04")
        );
        assert_eq!(format_elapsed(None, None, 5_000), None);
        // A clock that went backwards reads as zero rather than underflowing.
        assert_eq!(
            format_elapsed(Some(5_000), Some(1_000), 0).as_deref(),
            Some("0:00")
        );
    }

    #[test]
    fn blocked_calls_and_errors_are_exposed_as_facts() {
        let blocked = child(json!({
            "agent": "worker",
            "status": "running",
            "workflowMode": "plan",
            "blocked": [{"tool": "edit", "target": "src/auth.rs", "reason": "Blocked: Plan mode"}]
        }));
        assert_eq!(blocked.blocked.len(), 1);
        assert_eq!(blocked.blocked[0].tool, "edit");
        assert_eq!(blocked.blocked[0].target.as_deref(), Some("src/auth.rs"));
        assert_eq!(blocked.workflow_mode.as_deref(), Some("plan"));

        let refused = child(json!({
            "agent": "worker",
            "status": "failed",
            "phase": "refused",
            "error": "worker runs commands; this session is supervised"
        }));
        assert_eq!(refused.phase, Phase::Refused);
        assert_eq!(
            refused.error.as_deref(),
            Some("worker runs commands; this session is supervised")
        );
    }

    #[test]
    fn a_failed_child_carries_its_stderr_but_an_empty_tail_is_absent() {
        let failed = child(json!({
            "agent": "scout",
            "status": "failed",
            "endedAt": 1,
            "error": "child exited 1",
            "stderrTail": "\nError: Failed to load extension \"/x\"\n"
        }));
        assert_eq!(
            failed.stderr_tail.as_deref(),
            Some("Error: Failed to load extension \"/x\"")
        );

        assert_eq!(child(json!({"agent": "scout"})).stderr_tail, None);
        assert_eq!(
            child(json!({"agent": "scout", "stderrTail": "   \n"})).stderr_tail,
            None
        );
    }

    #[test]
    fn a_run_counts_running_children_and_tools() {
        let run = SubagentRun {
            mode: Mode::Parallel,
            children: vec![
                child(json!({"agent": "a", "status": "running", "tools": 20})),
                child(json!({"agent": "b", "status": "running", "tools": 24})),
                child(json!({"agent": "c", "status": "done", "endedAt": 1, "tools": 0})),
            ],
        };
        assert!(run.is_live());
        assert_eq!(run.running(), 2);
        assert_eq!(run.total_tools(), 44);
        assert_eq!(run.mode, Mode::Parallel);
    }

    #[test]
    fn usage_sums_and_hides_all_zero() {
        let run = SubagentRun {
            mode: Mode::Parallel,
            children: vec![
                child(json!({"agent": "a", "usage": {"input": 10, "output": 2, "cost": 0.01}})),
                child(json!({"agent": "b", "usage": {"input": 5, "output": 1}})),
            ],
        };
        let usage = run.usage().expect("some usage");
        assert_eq!(usage.input, 15);
        assert_eq!(usage.output, 3);

        let empty = SubagentRun {
            mode: Mode::Single,
            children: vec![child(json!({"agent": "a"}))],
        };
        assert!(empty.usage().is_none());
    }

    #[test]
    fn step_outcomes_parse_and_an_older_step_is_unknown() {
        let child = child(json!({
            "agent": "worker",
            "steps": [
                {"name": "read", "preview": "a", "status": "ok"},
                {"name": "bash", "preview": "cargo test", "status": "error"},
                {"name": "edit", "preview": "b", "status": "blocked"},
                {"name": "grep", "preview": "c", "status": "running"},
                {"name": "ls", "preview": "d"}
            ]
        }));
        let statuses: Vec<StepStatus> = child.steps.iter().map(|step| step.status).collect();
        assert_eq!(
            statuses,
            vec![
                StepStatus::Ok,
                StepStatus::Error,
                StepStatus::Blocked,
                StepStatus::Running,
                StepStatus::Unknown
            ]
        );
    }

    #[test]
    fn an_older_payloads_failed_tool_is_not_a_blocked_call() {
        let child = child(json!({
            "agent": "worker",
            "blocked": [
                {"tool": "bash", "target": "cargo test", "reason": "Tool failed"},
                {"tool": "edit", "target": "src/a.rs", "reason": "Plan mode: edit is disabled."}
            ]
        }));
        assert_eq!(child.blocked.len(), 1);
        assert_eq!(child.blocked[0].tool, "edit");
    }

    #[test]
    fn silence_is_reported_only_for_a_quiet_thinking_child() {
        let thinking = child(json!({
            "agent": "scout", "status": "running", "phase": "thinking", "lastEventAt": 1_000
        }));
        assert_eq!(thinking.silent_for_ms(1_000 + SILENCE_AFTER_MS - 1), None);
        assert_eq!(
            thinking.silent_for_ms(1_000 + SILENCE_AFTER_MS),
            Some(SILENCE_AFTER_MS)
        );

        // A running tool is quiet by nature; the row names the tool instead.
        let tool = child(json!({
            "agent": "scout", "status": "running", "phase": "tool", "lastEventAt": 1_000
        }));
        assert_eq!(tool.silent_for_ms(60_000), None);

        // A settled child is not silent, and no timestamp invents nothing.
        let done = child(json!({
            "agent": "scout", "status": "done", "endedAt": 2, "lastEventAt": 1
        }));
        assert_eq!(done.silent_for_ms(60_000), None);
        let unstamped = child(json!({"agent": "scout", "status": "running"}));
        assert_eq!(unstamped.silent_for_ms(60_000), None);
    }

    #[test]
    fn timeout_thinking_and_model_are_read_as_given() {
        let child = child(json!({
            "agent": "scout",
            "status": "failed",
            "phase": "killed",
            "timedOut": true,
            "thinking": "medium",
            "model": "claude-haiku-4-5"
        }));
        assert!(child.timed_out);
        assert_eq!(child.thinking.as_deref(), Some("medium"));
        // A bare id has no namespace to drop.
        assert_eq!(child.model_label().as_deref(), Some("claude-haiku-4-5"));
        assert!(!super::SubagentChild::from_value(&json!({})).timed_out);
    }

    #[test]
    fn the_run_clock_spans_its_children_and_runs_while_one_does() {
        let settled = SubagentRun {
            mode: Mode::Parallel,
            children: vec![
                child(
                    json!({"agent": "a", "status": "done", "startedAt": 1_000, "endedAt": 20_000}),
                ),
                child(
                    json!({"agent": "b", "status": "done", "startedAt": 2_000, "endedAt": 61_000}),
                ),
            ],
        };
        assert_eq!(settled.elapsed(999_000).as_deref(), Some("1:00"));

        let live = SubagentRun {
            mode: Mode::Parallel,
            children: vec![
                child(
                    json!({"agent": "a", "status": "done", "startedAt": 1_000, "endedAt": 20_000}),
                ),
                child(json!({"agent": "b", "status": "running", "startedAt": 2_000})),
            ],
        };
        assert_eq!(live.elapsed(31_000).as_deref(), Some("0:30"));

        let unstamped = SubagentRun {
            mode: Mode::Single,
            children: vec![child(json!({"agent": "a", "status": "running"}))],
        };
        assert_eq!(unstamped.elapsed(5_000), None);
    }
}
