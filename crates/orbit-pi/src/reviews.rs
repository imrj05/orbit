//! The durable record of an AI review run: what was reviewed, with which
//! model, how it ended, and what it found.
//!
//! Orbit persists finished runs to `~/.orbit-pi/reviews.json` so findings
//! survive restarts:
//!
//! ```jsonc
//! { "runs": [ { "id": 3, "workspace": "/src/orbit", "kind": { "kind": "uncommitted" },
//!               "model": "claude-opus-4-5", "thinking": "high", "status": "completed",
//!               "started_at": 1760000000, "finished_at": 1760000041,
//!               "report": { "verdict": "needs_attention", "findings": [...],
//!                           "summary": "…" } } ] }
//! ```
//!
//! This module is pure: it owns the run shape, its persistence format, and the
//! retention rules. The process lifecycle and the store that owns live runs
//! live in [`crate::app::reviews`]; the prompts and parsing live in
//! [`crate::ai_review`].

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::ai_review::{Finding, Report, ReviewKind, Severity, Verdict};

/// How many runs are kept per workspace when the store is written.
pub const MAX_RUNS_PER_WORKSPACE: usize = 50;

/// Overall cap, across all workspaces, so a user with many projects cannot
/// grow the file without bound.
pub const MAX_RUNS_TOTAL: usize = 200;

/// The model and thinking level one review run uses. Run-scoped on purpose:
/// starting a review must never change the chat session's model.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReviewRunConfig {
    /// pi provider id (`anthropic`), when one was chosen.
    pub provider: Option<String>,
    /// pi model id (`claude-opus-4-5`), when one was chosen.
    pub model: Option<String>,
    /// Thinking level (`off` … `max`), when one was chosen.
    pub thinking: Option<String>,
}

impl ReviewRunConfig {
    /// The short label the page and the run list show (`Opus 4.5 · high`).
    pub fn label(&self) -> String {
        let model = self.model.as_deref().unwrap_or("");
        let model = if model.is_empty() {
            tr!("ai_review.model_default")
        } else {
            model.to_string()
        };
        match self.thinking.as_deref().filter(|level| !level.is_empty()) {
            Some(level) => format!("{model} · {level}"),
            None => model,
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "provider": self.provider,
            "model": self.model,
            "thinking": self.thinking,
        })
    }

    fn from_json(value: &Value) -> Self {
        let string = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        Self {
            provider: string("provider"),
            model: string("model"),
            thinking: string("thinking"),
        }
    }
}

/// A run's lifecycle. `Queued` exists so a run started while the concurrency
/// cap is full still appears in the list immediately.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed(String),
    Cancelled,
}

impl RunStatus {
    /// The status row label shown in the run list.
    pub fn label(&self) -> String {
        match self {
            Self::Queued => tr!("ai_review.status_queued"),
            Self::Running => tr!("ai_review.status_running"),
            Self::Completed => tr!("ai_review.status_completed"),
            Self::Failed(_) => tr!("ai_review.status_failed"),
            Self::Cancelled => tr!("ai_review.status_cancelled"),
        }
    }

    /// Whether the run has stopped (successfully or not).
    pub fn is_finished(&self) -> bool {
        !matches!(self, Self::Queued | Self::Running)
    }

    fn wire(&self) -> Value {
        match self {
            Self::Queued => json!("queued"),
            Self::Running => json!("running"),
            Self::Completed => json!("completed"),
            Self::Failed(_) => json!("failed"),
            Self::Cancelled => json!("cancelled"),
        }
    }

    /// The failure text, when this status carries one.
    pub fn failure(&self) -> Option<&str> {
        match self {
            Self::Failed(error) => Some(error.as_str()),
            _ => None,
        }
    }
}

/// Coarse progress for a running review: what the reviewer has touched so far.
/// Derived from its streamed tool events — never a fabricated percentage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunProgress {
    /// Distinct files the reviewer has read.
    pub files_read: usize,
    /// The last path it touched, for the status line.
    pub last_path: Option<String>,
}

impl RunProgress {
    /// The status line under a running run (`Reading 14 files…`).
    pub fn label(&self) -> String {
        if self.files_read == 0 {
            return tr!("ai_review.working");
        }
        if self.files_read == 1 {
            return tr!("ai_review.files_read", count = 1);
        }
        tr!("ai_review.files_read_plural", count = self.files_read)
    }

    /// Record a touched path; returns whether anything changed.
    pub fn note_path(&mut self, path: &str) -> bool {
        let path = path.trim();
        if path.is_empty() {
            return false;
        }
        let changed = self.last_path.as_deref() != Some(path);
        self.last_path = Some(path.to_string());
        changed
    }

    /// Record a file read. Paths are counted once (the reviewer re-reads).
    pub fn note_file(&mut self, path: &str) {
        let path = path.trim();
        if path.is_empty() {
            return;
        }
        self.files_read += 1;
        self.last_path = Some(path.to_string());
    }
}

/// One review run — the unit the store owns and `reviews.json` persists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewRun {
    /// Monotonic id, unique for the life of the process.
    pub id: u64,
    /// The workspace this run belongs to, pinned at start. A run started in
    /// Project A keeps reviewing A after the user switches to B.
    pub workspace: PathBuf,
    /// The workspace's `HEAD` when the run started, for staleness checks.
    pub head: Option<String>,
    /// What was reviewed.
    pub kind: ReviewKind,
    /// The model and thinking level the run used.
    pub config: ReviewRunConfig,
    pub status: RunStatus,
    /// Unix seconds the run was created.
    pub started_at: u64,
    /// Unix seconds the run reached a finished status.
    pub finished_at: Option<u64>,
    /// Coarse progress, meaningful while running.
    pub progress: RunProgress,
    /// The parsed findings, once the run has settled.
    pub report: Option<Report>,
}

impl ReviewRun {
    /// Whether this run is still queued or running.
    pub fn is_active(&self) -> bool {
        !self.status.is_finished()
    }

    /// The run's severity counts, `(errors, warnings, infos)`.
    pub fn counts(&self) -> (usize, usize, usize) {
        let report = self.report.as_ref();
        (
            report.map(|report| report.count(Severity::Error)).unwrap_or(0),
            report
                .map(|report| report.count(Severity::Warning))
                .unwrap_or(0),
            report.map(|report| report.count(Severity::Info)).unwrap_or(0),
        )
    }

    /// How long the run took, in whole seconds, once it finished.
    pub fn elapsed_secs(&self) -> Option<u64> {
        self.finished_at.map(|end| end.saturating_sub(self.started_at))
    }

    /// A one-line status summary for the run list (`3 findings · 41s`).
    pub fn summary_line(&self) -> String {
        let elapsed = self
            .elapsed_secs()
            .map(|secs| tr!("ai_review.elapsed", secs = secs))
            .unwrap_or_default();
        match self.status {
            RunStatus::Completed => {
                let found = self
                    .report
                    .as_ref()
                    .map(|report| report.findings.len())
                    .unwrap_or(0);
                let findings = if found == 0 {
                    tr!("ai_review.no_issues")
                } else if found == 1 {
                    tr!("ai_review.finding_one", count = found)
                } else {
                    tr!("ai_review.finding_many", count = found)
                };
                if elapsed.is_empty() {
                    findings
                } else {
                    format!("{findings} · {elapsed}")
                }
            }
            _ => elapsed,
        }
    }

    fn to_json(&self) -> Value {
        let report = self.report.as_ref().map(report_to_json);
        json!({
            "id": self.id,
            "workspace": self.workspace.to_string_lossy(),
            "head": self.head,
            "kind": self.kind.to_json(),
            "config": self.config.to_json(),
            "status": self.status.wire(),
            "error": self.status.failure(),
            "started_at": self.started_at,
            "finished_at": self.finished_at,
            "report": report,
        })
    }

    /// Restore a persisted run. Runs that were mid-flight when the app quit
    /// come back as `Failed("Interrupted by app restart")` so an abandoned
    /// process can never read as a live one.
    fn from_json(value: &Value) -> Option<Self> {
        let workspace = value.get("workspace")?.as_str()?;
        let kind = ReviewKind::from_json(value.get("kind")?)?;
        let status = match value.get("status")?.as_str()? {
            "queued" | "running" => RunStatus::Failed(tr!("ai_review.interrupted")),
            "completed" => RunStatus::Completed,
            "cancelled" => RunStatus::Cancelled,
            "failed" => RunStatus::Failed(
                value
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            ),
            _ => return None,
        };
        Some(Self {
            id: value.get("id")?.as_u64()?,
            workspace: PathBuf::from(workspace),
            head: value
                .get("head")
                .and_then(Value::as_str)
                .map(str::to_string),
            kind,
            config: value
                .get("config")
                .map(ReviewRunConfig::from_json)
                .unwrap_or_default(),
            status,
            started_at: value.get("started_at").and_then(Value::as_u64).unwrap_or(0),
            finished_at: value.get("finished_at").and_then(Value::as_u64),
            progress: RunProgress::default(),
            report: value.get("report").and_then(report_from_json),
        })
    }
}

fn report_to_json(report: &Report) -> Value {
    let findings: Vec<Value> = report
        .findings
        .iter()
        .map(|finding| {
            json!({
                "severity": match finding.severity {
                    Severity::Error => "error",
                    Severity::Warning => "warning",
                    Severity::Info => "info",
                },
                "file": finding.file,
                "line": finding.line,
                "title": finding.title,
                "detail": finding.detail,
            })
        })
        .collect();
    json!({
        "verdict": match report.verdict {
            Verdict::Correct => "correct",
            Verdict::NeedsAttention => "needs_attention",
        },
        "summary": report.summary,
        "findings": findings,
    })
}

fn report_from_json(value: &Value) -> Option<Report> {
    let findings: Vec<Finding> = value
        .get("findings")?
        .as_array()?
        .iter()
        .filter_map(|finding| {
            let title = finding.get("title")?.as_str()?.trim().to_string();
            if title.is_empty() {
                return None;
            }
            Some(Finding {
                severity: finding
                    .get("severity")
                    .and_then(Value::as_str)
                    .map(Severity::from_wire)
                    .unwrap_or(Severity::Info),
                file: finding
                    .get("file")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                line: finding
                    .get("line")
                    .and_then(Value::as_u64)
                    .map(|line| line as u32),
                title,
                detail: finding
                    .get("detail")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect();
    Some(Report {
        verdict: value
            .get("verdict")
            .and_then(Value::as_str)
            .map(Verdict::from_wire)
            .unwrap_or_default(),
        findings,
        summary: value
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    })
}

/// Apply the retention caps: per workspace first, then overall (newest kept).
/// `runs` is expected newest-first.
pub fn prune(runs: &mut Vec<ReviewRun>) {
    use std::collections::HashMap;

    let mut kept_per_workspace: HashMap<PathBuf, usize> = HashMap::new();
    runs.retain(|run| {
        let count = kept_per_workspace.entry(run.workspace.clone()).or_default();
        *count += 1;
        *count <= MAX_RUNS_PER_WORKSPACE
    });
    runs.truncate(MAX_RUNS_TOTAL);
}

/// Read `~/.orbit-pi/reviews.json`. Missing or unreadable files yield an empty
/// history; individual malformed rows are skipped, never fatal.
pub fn load() -> Vec<ReviewRun> {
    let Ok(text) = std::fs::read_to_string(store_path()) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    let Some(runs) = value.get("runs").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut runs: Vec<ReviewRun> = runs.iter().filter_map(ReviewRun::from_json).collect();
    // Newest first: the store writes them in that order, but a hand-edited
    // file may not be sorted.
    runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    prune(&mut runs);
    runs
}

/// Write the run history, pruned and newest-first. Failures are ignored: a
/// history that cannot be saved must never fail a review.
pub fn save(runs: &[ReviewRun]) {
    let mut runs = runs.to_vec();
    prune(&mut runs);
    let value = json!({ "runs": runs.iter().map(ReviewRun::to_json).collect::<Vec<_>>() });
    let Ok(text) = serde_json::to_string_pretty(&value) else {
        return;
    };
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, text);
}

/// `~/.orbit-pi/reviews.json` — Orbit-owned, like the other stores.
pub fn store_path() -> PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("reviews.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: u64, workspace: &str, status: RunStatus) -> ReviewRun {
        ReviewRun {
            id,
            workspace: PathBuf::from(workspace),
            head: Some("abc".into()),
            kind: ReviewKind::Uncommitted,
            config: ReviewRunConfig {
                provider: Some("anthropic".into()),
                model: Some("opus-4-5".into()),
                thinking: Some("high".into()),
            },
            status,
            started_at: 1_760_000_000,
            finished_at: None,
            progress: RunProgress::default(),
            report: None,
        }
    }

    #[test]
    fn runs_round_trip_through_json() {
        let mut original = run(7, "/src/orbit", RunStatus::Completed);
        original.finished_at = Some(1_760_000_041);
        original.report = Some(Report {
            verdict: Verdict::NeedsAttention,
            findings: vec![Finding {
                severity: Severity::Error,
                file: Some("src/a.rs".into()),
                line: Some(12),
                title: "Unwrap on None".into(),
                detail: "This panics.".into(),
            }],
            summary: "One issue.".into(),
        });

        let restored = ReviewRun::from_json(&original.to_json()).expect("round trips");
        assert_eq!(restored.id, 7);
        assert_eq!(restored.workspace, PathBuf::from("/src/orbit"));
        assert_eq!(restored.kind, ReviewKind::Uncommitted);
        assert_eq!(restored.config, original.config);
        assert_eq!(restored.status, RunStatus::Completed);
        assert_eq!(restored.finished_at, Some(1_760_000_041));
        let report = restored.report.expect("report survives");
        assert_eq!(report.verdict, Verdict::NeedsAttention);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].location().as_deref(), Some("src/a.rs:12"));
    }

    #[test]
    fn in_flight_runs_come_back_as_interrupted() {
        for status in [RunStatus::Queued, RunStatus::Running] {
            let restored = ReviewRun::from_json(&run(1, "/w", status).to_json()).expect("round trips");
            assert!(matches!(restored.status, RunStatus::Failed(_)));
            assert!(restored.status.is_finished());
        }
    }

    #[test]
    fn failed_runs_keep_their_reason() {
        let restored = ReviewRun::from_json(&run(1, "/w", RunStatus::Failed("boom".into())).to_json())
            .expect("round trips");
        assert_eq!(restored.status, RunStatus::Failed("boom".into()));
    }

    #[test]
    fn malformed_rows_are_skipped() {
        assert!(ReviewRun::from_json(&json!({})).is_none());
        assert!(ReviewRun::from_json(&json!({ "workspace": "/w" })).is_none());
        assert!(ReviewRun::from_json(&json!({
            "workspace": "/w",
            "kind": { "kind": "nope" },
            "status": "completed"
        }))
        .is_none());
    }

    #[test]
    fn pruning_keeps_the_newest_runs_per_workspace_and_overall() {
        let mut runs: Vec<ReviewRun> = (0..MAX_RUNS_PER_WORKSPACE + 10)
            .map(|id| run(id as u64, "/a", RunStatus::Completed))
            .collect();
        prune(&mut runs);
        assert_eq!(runs.len(), MAX_RUNS_PER_WORKSPACE);

        let mut many: Vec<ReviewRun> = (0..MAX_RUNS_TOTAL + 40)
            .map(|id| run(id as u64, &format!("/w{}", id % 7), RunStatus::Completed))
            .collect();
        prune(&mut many);
        assert!(many.len() <= MAX_RUNS_TOTAL);
    }

    #[test]
    fn counts_and_summary_reflect_the_report() {
        let mut run = run(1, "/w", RunStatus::Completed);
        run.finished_at = Some(1_760_000_041);
        run.report = Some(Report {
            verdict: Verdict::NeedsAttention,
            findings: vec![
                Finding {
                    severity: Severity::Error,
                    file: None,
                    line: None,
                    title: "a".into(),
                    detail: String::new(),
                },
                Finding {
                    severity: Severity::Info,
                    file: None,
                    line: None,
                    title: "b".into(),
                    detail: String::new(),
                },
            ],
            summary: String::new(),
        });
        assert_eq!(run.counts(), (1, 0, 1));
        assert_eq!(run.summary_line(), "2 findings · 41s");
    }

    #[test]
    fn no_findings_reads_as_no_issues() {
        let mut run = run(1, "/w", RunStatus::Completed);
        run.report = Some(Report::default());
        assert_eq!(run.summary_line(), "No issues found.");
    }

    #[test]
    fn progress_tracks_paths() {
        let mut progress = RunProgress::default();
        assert_eq!(progress.label(), tr!("ai_review.working"));
        progress.note_file("src/a.rs");
        assert_eq!(progress.files_read, 1);
        assert_eq!(progress.last_path.as_deref(), Some("src/a.rs"));
        assert_eq!(progress.label(), tr!("ai_review.files_read", count = 1));
        progress.note_file("src/b.rs");
        assert_eq!(progress.label(), tr!("ai_review.files_read_plural", count = 2));
        assert!(progress.note_path("src/c.rs"));
        assert!(!progress.note_path("src/c.rs"));
    }

    #[test]
    fn config_label_falls_back_to_the_session_default() {
        assert_eq!(ReviewRunConfig::default().label(), tr!("ai_review.model_default"));
        let config = ReviewRunConfig {
            provider: None,
            model: Some("opus-4-5".into()),
            thinking: Some("high".into()),
        };
        assert_eq!(config.label(), "opus-4-5 · high");
    }

    #[test]
    fn store_path_is_the_file_the_app_owns() {
        assert!(store_path().ends_with(".orbit-pi/reviews.json"));
    }
}
