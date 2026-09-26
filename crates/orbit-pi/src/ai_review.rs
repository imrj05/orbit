//! AI review agent — a read-only reviewer over the workspace changes or the
//! whole project, whose findings render in the Review pane.
//!
//! Orbit runs the reviewer on its **own** pi process scoped to **Ask mode**
//! (read-only via the workflow extension), so the user's chat session and
//! transcript are never touched. The process is launched with `ORBIT_REVIEW=1`,
//! which the bundled guard extension treats as pre-approved — the workflow
//! extension remains the stricter read-only gate. The process lifecycle lives
//! in [`crate::app::ai_review`] and the durable run store in
//! [`crate::reviews`].
//!
//! This module is pure: it builds the reviewer prompts and parses the
//! structured findings block out of the final answer. No I/O, no GPUI —
//! unit-tested.

use serde_json::Value;

/// Cap on the diff text embedded in the changes prompt. A pathological patch
/// must not turn one request into an unbounded prompt; the reviewer can still
/// read the full files itself.
pub const MAX_REVIEW_PATCH_BYTES: usize = 200_000;

/// What the reviewer is asked to inspect.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReviewKind {
    /// The working-tree / turn changes for the selected Review source.
    Changes,
    /// The whole project tree.
    Project,
    /// HEAD → worktree: staged, unstaged, and untracked files together.
    Uncommitted,
    /// merge-base(HEAD, base) → worktree, as a branch/PR-style review.
    Branch { base: String },
    /// One commit, `parent → commit`.
    Commit { sha: String, title: String },
    /// A snapshot of chosen workspace-relative paths (not a diff).
    Files { paths: Vec<String> },
}

impl ReviewKind {
    /// The kind's `changes`-family target: the kinds whose review prompt is
    /// anchored on a diff and therefore collect it before the reviewer starts.
    pub fn is_diff(&self) -> bool {
        !matches!(self, Self::Project | Self::Files { .. })
    }

    /// Label for the page, pane header, and history rows.
    pub fn label(&self) -> String {
        match self {
            Self::Changes => tr!("ai_review.review_changes"),
            Self::Project => tr!("ai_review.review_project"),
            Self::Uncommitted => tr!("ai_review.kind_uncommitted"),
            Self::Branch { base } if base.is_empty() => tr!("ai_review.kind_branch"),
            Self::Branch { base } => tr!("ai_review.kind_branch_base", base = base.clone()),
            Self::Commit { sha, .. } => {
                let short: String = sha.chars().take(7).collect();
                tr!("ai_review.kind_commit", sha = short)
            }
            Self::Files { paths } => tr!("ai_review.kind_files", count = paths.len()),
        }
    }

    /// One-line description shown in the pane/page while running.
    pub fn description(&self) -> String {
        match self {
            Self::Changes => tr!("ai_review.changes_hint"),
            Self::Project => tr!("ai_review.project_hint"),
            Self::Uncommitted => tr!("ai_review.hint_uncommitted"),
            Self::Branch { .. } => tr!("ai_review.hint_branch"),
            Self::Commit { .. } => tr!("ai_review.hint_commit"),
            Self::Files { .. } => tr!("ai_review.hint_files"),
        }
    }

    /// The wire/storage shape (`uncommitted` | `branch` | `commit` | `files`
    /// | `changes` | `project`), for `reviews.json`.
    pub fn wire(&self) -> String {
        match self {
            Self::Changes => "changes".into(),
            Self::Project => "project".into(),
            Self::Uncommitted => "uncommitted".into(),
            Self::Branch { .. } => "branch".into(),
            Self::Commit { .. } => "commit".into(),
            Self::Files { .. } => "files".into(),
        }
    }

    /// Persist this kind, leniently (`None` for an unknown wire id).
    pub fn to_json(&self) -> Value {
        let mut value = serde_json::json!({ "kind": self.wire() });
        match self {
            Self::Branch { base } => {
                value["base"] = Value::String(base.clone());
            }
            Self::Commit { sha, title } => {
                value["sha"] = Value::String(sha.clone());
                value["title"] = Value::String(title.clone());
            }
            Self::Files { paths } => {
                value["paths"] = Value::Array(paths.iter().cloned().map(Value::String).collect());
            }
            _ => {}
        }
        value
    }

    /// Restore a kind from its persisted shape; `None` when the wire id is
    /// unknown or a required field is missing (a corrupt row is skipped, never
    /// fatal).
    pub fn from_json(value: &Value) -> Option<Self> {
        let kind = value.get("kind")?.as_str()?;
        Some(match kind {
            "changes" => Self::Changes,
            "project" => Self::Project,
            "uncommitted" => Self::Uncommitted,
            "branch" => Self::Branch {
                base: value.get("base")?.as_str()?.to_string(),
            },
            "commit" => Self::Commit {
                sha: value.get("sha")?.as_str()?.to_string(),
                title: value
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            "files" => Self::Files {
                paths: value
                    .get("paths")?
                    .as_array()?
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            },
            _ => return None,
        })
    }
}

/// Severity of one finding, in display order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    /// Parse a model-supplied severity; unknown values degrade to `Info`.
    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" | "critical" | "high" | "bug" => Self::Error,
            "warning" | "warn" | "medium" => Self::Warning,
            _ => Self::Info,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Error => tr!("ai_review.severity_error"),
            Self::Warning => tr!("ai_review.severity_warning"),
            Self::Info => tr!("ai_review.severity_info"),
        }
    }
}

/// One review finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    /// Workspace-relative path, when the model named one.
    pub file: Option<String>,
    /// 1-based line in `file`, when the model named one.
    pub line: Option<u32>,
    pub title: String,
    pub detail: String,
}

impl Finding {
    /// `path:line` (or just `path`), for the pane's location row.
    pub fn location(&self) -> Option<String> {
        match (&self.file, self.line) {
            (Some(file), Some(line)) => Some(format!("{file}:{line}")),
            (Some(file), None) => Some(file.clone()),
            (None, Some(line)) => Some(format!(":{line}")),
            (None, None) => None,
        }
    }
}

/// The reviewer run's lifecycle, for the pane's empty / spinner / result states.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum ReviewStatus {
    #[default]
    Idle,
    Running,
    Done,
    Failed(String),
}

/// The reviewer's verdict for the change as a whole.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Verdict {
    /// Blocking issues were found.
    #[default]
    NeedsAttention,
    /// No qualifying issues were found.
    Correct,
}

impl Verdict {
    /// Parse a model-supplied verdict; unknown values degrade to
    /// `NeedsAttention` (the conservative reading of an unclear answer).
    pub fn from_wire(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "correct" | "ok" | "pass" | "clean" => Self::Correct,
            _ => Self::NeedsAttention,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::NeedsAttention => tr!("ai_review.verdict_needs_attention"),
            Self::Correct => tr!("ai_review.verdict_correct"),
        }
    }
}

/// A parsed reviewer answer: the structured findings plus the answer's prose
/// with the findings block removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// The model's overall verdict; defaults to `NeedsAttention` so an answer
    /// that omits it never reads as a clean bill of health.
    pub verdict: Verdict,
    pub findings: Vec<Finding>,
    /// The answer's remaining markdown (usually a short summary). Empty when
    /// the model returned only the findings block.
    pub summary: String,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty() && self.summary.trim().is_empty()
    }

    /// How many findings carry `severity`, for the pane/header counts.
    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }

    /// Errors first, then warnings, then info — the order both the pane and the
    /// Review page render findings in.
    pub fn sorted_findings(&self) -> Vec<&Finding> {
        let rank = |severity: Severity| match severity {
            Severity::Error => 0,
            Severity::Warning => 1,
            Severity::Info => 2,
        };
        let mut findings: Vec<&Finding> = self.findings.iter().collect();
        findings.sort_by_key(|finding| rank(finding.severity));
        findings
    }
}

/// Cap a patch to [`MAX_REVIEW_PATCH_BYTES`], returning whether it was cut.
pub fn cap_patch(patch: &str) -> (String, bool) {
    if patch.len() <= MAX_REVIEW_PATCH_BYTES {
        return (patch.to_string(), false);
    }
    let mut end = MAX_REVIEW_PATCH_BYTES;
    while end > 0 && !patch.is_char_boundary(end) {
        end -= 1;
    }
    (patch[..end].to_string(), true)
}

/// The findings-contract shared by every prompt. Kept in one place so the
/// parser and the instructions can never drift.
const FINDINGS_CONTRACT: &str = "\
Then answer in exactly two parts:
1. A short prose summary (a few bullet points at most).
2. One fenced ```json block holding the verdict and findings, in exactly this shape:
```json
{\"verdict\":\"correct|needs attention\",\"findings\":[{\"severity\":\"error|warning|info\",\"file\":\"relative/path\",\"line\":42,\"title\":\"Short summary\",\"detail\":\"Why it matters and how to fix it\"}]}
```
Use \"correct\" only when no qualifying issue was found; otherwise \"needs
attention\". Use \"error\" for bugs, crashes, security holes, or data loss;
\"warning\" for likely problems; \"info\" for suggestions. \"file\" and
\"line\" are optional. If there are no issues, return
{\"verdict\":\"correct\",\"findings\":[]}.";

/// The review rubric shared by every prompt: what to flag, how to judge, and
/// how to phrase findings. Every rule here is checked against the diff or the
/// files under review — the reviewer never invents context.
const REVIEW_RUBRIC: &str = "\
Review like a senior engineer on this codebase, not a linter.

What to flag:
- Issues the change introduces — not pre-existing problems, and not issues the
  author clearly intended.
- Concrete, discrete problems: correctness, crashes, security, data loss,
  concurrency, error handling, performance, and regressions in callers.
- Silent failure: swallowed errors, fallbacks that mask a programming error,
  ignored results (`let _ =`, `.ok()`, a match arm that drops the error).
- Code that violates the project's own conventions or duplicates an existing
  helper. Name the existing implementation when you flag duplication.

What not to flag:
- Style preferences, naming, formatting, or trivia unless they hide a bug.
- Speculation about code you did not read or an impact you cannot point to.
- Hypothetical abstractions without a current need, or \"best practice\" advice
  that does not change what the code does.
- Demands for rigor the surrounding code does not itself follow.

How to write each finding:
- State the problem, why it matters, and the scenario where it bites — briefly,
  one paragraph at most. Keep code snippets short.
- Cite the file and line where the problem lives; reference lines that are part
  of the change under review.
- Don't exaggerate severity. A style-level suggestion is \"info\".
- Only report issues you are confident about; never invent a problem to fill a
  quota. A short list of real issues beats a long list of maybes.

If project-specific review guidelines are provided below, they override this
rubric.";

/// Build the prompt for reviewing a change set.
///
/// `source_label` names the Review source (Last Turn / Uncommitted / …) so the
/// reviewer knows what the diff represents. `truncated` appends a note when
/// [`cap_patch`] cut the diff, so the reviewer reads the rest from disk.
pub fn build_changes_prompt(source_label: &str, patch: &str, truncated: bool) -> String {
    let note = if truncated {
        "\n(The diff was truncated to fit this prompt — read the affected files for the rest.)\n"
    } else {
        ""
    };
    format!(
        "You are a meticulous senior code reviewer. Review the change described \
by the diff below (`git diff` for the source \"{source_label}\").

Rules:
- Read-only: never modify files or run mutating commands.
- Use read/search/find and read-only shell commands to inspect surrounding \
code, callers, and tests before judging.
{note}
{REVIEW_RUBRIC}

Diff:
```diff
{patch}
```

{FINDINGS_CONTRACT}"
    )
}

/// Build the prompt for reviewing one branch/commit comparison. The diff is
/// embedded exactly like [`build_changes_prompt`], so the two share the note,
/// rubric, and findings contract.
pub fn build_revision_prompt(context: &str, patch: &str, truncated: bool) -> String {
    let note = if truncated {
        "\n(The diff was truncated to fit this prompt — read the affected files for the rest.)\n"
    } else {
        ""
    };
    format!(
        "You are a meticulous senior code reviewer. Review the change below \
({context}).

Rules:
- Read-only: never modify files or run mutating commands.
- Use read/search/find and read-only shell commands to inspect surrounding \
code, callers, and tests before judging.
{note}
{REVIEW_RUBRIC}

Diff:
```diff
{patch}
```

{FINDINGS_CONTRACT}"
    )
}

/// Build the prompt for a snapshot review of chosen workspace paths.
pub fn build_files_prompt(paths: &[String]) -> String {
    let list = paths
        .iter()
        .map(|path| format!("- {path}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "You are a meticulous senior code reviewer. Review the code in the \
following paths of this workspace:

{list}

This is a snapshot review, not a diff: read the files under those paths \
directly and judge the code as it stands.

Rules:
- Read-only: never modify files or run mutating commands.
- Read the files first; do not assume their contents.
{REVIEW_RUBRIC}

{FINDINGS_CONTRACT}"
    )
}

/// Build the prompt for reviewing the whole project.
pub fn build_project_prompt() -> String {
    format!(
        "You are a meticulous senior code reviewer. Review this repository as a \
whole and report the highest-signal problems you can substantiate.

Rules:
- Read-only: never modify files or run mutating commands.
- Start from the project layout, then read the files that matter. Use \
read/search/find and read-only shell commands.
- Prefer a handful of real issues over a long list of speculation.
{REVIEW_RUBRIC}

{FINDINGS_CONTRACT}"
    )
}

/// Parse the reviewer's final markdown answer into a [`Report`].
///
/// Prefers the last fenced `json` block that carries a `findings` array (an
/// empty array is a valid "no issues" answer). The block is stripped from the
/// prose. A missing or malformed block yields zero findings and keeps the whole
/// answer as `summary` — parsing never fails the run.
pub fn parse_report(markdown: &str) -> Report {
    for block in fenced_blocks(markdown).into_iter().rev() {
        if !block.lang.is_empty() && !block.lang.eq_ignore_ascii_case("json") {
            continue;
        }
        if let Some(report) = report_from_json(&block.body) {
            let mut summary = markdown[..block.start].trim_end().to_string();
            let tail = markdown[block.end..].trim();
            if !tail.is_empty() {
                if !summary.is_empty() {
                    summary.push_str("\n\n");
                }
                summary.push_str(tail);
            }
            return Report { summary, ..report };
        }
    }
    Report {
        verdict: Verdict::default(),
        findings: Vec::new(),
        summary: markdown.trim().to_string(),
    }
}

/// A fenced code block's language, body, and byte range in the source.
struct Block {
    lang: String,
    body: String,
    start: usize,
    end: usize,
}

fn fenced_blocks(markdown: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    // (fence start byte, language, body start byte)
    let mut open: Option<(usize, String, usize)> = None;
    let mut pos = 0;
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if let Some(rest) = trimmed.trim_start().strip_prefix("```") {
            if open.is_none() {
                open = Some((pos, rest.trim().to_string(), pos + line.len()));
            } else {
                let (start, lang, body_start) = open.take().expect("guarded by open.is_none()");
                blocks.push(Block {
                    lang,
                    body: markdown[body_start..pos].to_string(),
                    start,
                    end: pos + line.len(),
                });
            }
        }
        pos += line.len();
    }
    blocks
}

/// Extract a report from a JSON body, or `None` when it is not a findings
/// payload (so a stray example block is skipped rather than treated as empty).
/// An omitted `verdict` defaults from the findings: any error/warning means
/// `NeedsAttention`, otherwise `Correct` — an old-format answer still reads
/// sensibly.
fn report_from_json(body: &str) -> Option<Report> {
    let value: Value = serde_json::from_str(body.trim()).ok()?;
    let array = value.get("findings")?.as_array()?;
    let findings: Vec<Finding> = array.iter().filter_map(finding_from_value).collect();
    let verdict = value
        .get("verdict")
        .and_then(Value::as_str)
        .map(Verdict::from_wire)
        .unwrap_or_else(|| {
            if findings.iter().any(|finding| finding.severity != Severity::Info) {
                Verdict::NeedsAttention
            } else {
                Verdict::Correct
            }
        });
    Some(Report {
        verdict,
        findings,
        summary: String::new(),
    })
}

fn finding_from_value(value: &Value) -> Option<Finding> {
    let title = value
        .get("title")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)?
        .trim()
        .to_string();
    if title.is_empty() {
        return None;
    }
    let severity = value
        .get("severity")
        .and_then(Value::as_str)
        .map(Severity::from_wire)
        .unwrap_or(Severity::Info);
    let file = value
        .get("file")
        .or_else(|| value.get("path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|file| !file.is_empty())
        .map(str::to_string);
    let line = value.get("line").and_then(Value::as_u64).map(|n| n as u32);
    let detail = value
        .get("detail")
        .or_else(|| value.get("description"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    Some(Finding {
        severity,
        file,
        line,
        title,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_prompt_carries_the_diff_and_contract() {
        let prompt = build_changes_prompt("Uncommitted", "-old\n+new\n", false);
        assert!(prompt.contains("Uncommitted"));
        assert!(prompt.contains("-old\n+new"));
        assert!(prompt.contains("{\"verdict\":"));
        assert!(prompt.contains("What to flag"));
        assert!(!prompt.contains("truncated"));
    }

    #[test]
    fn revision_prompt_names_the_comparison() {
        let prompt = build_revision_prompt("commit abc123 Fix parser", "+x", false);
        assert!(prompt.contains("commit abc123 Fix parser"));
        assert!(prompt.contains("+x"));
        assert!(prompt.contains("What to flag"));
    }

    #[test]
    fn files_prompt_lists_the_paths() {
        let prompt = build_files_prompt(&["src/".into(), "docs/a.md".into()]);
        assert!(prompt.contains("- src/"));
        assert!(prompt.contains("- docs/a.md"));
        assert!(prompt.contains("snapshot review"));
    }

    #[test]
    fn truncated_prompt_says_so() {
        let prompt = build_changes_prompt("Staged", "diff", true);
        assert!(prompt.contains("truncated"));
    }

    #[test]
    fn project_prompt_asks_for_the_whole_repo() {
        let prompt = build_project_prompt();
        assert!(prompt.contains("repository as a"));
        assert!(prompt.contains("{\"verdict\":"));
    }

    #[test]
    fn cap_patch_cuts_on_a_char_boundary() {
        let long = "é".repeat(MAX_REVIEW_PATCH_BYTES);
        let (capped, truncated) = cap_patch(&long);
        assert!(truncated);
        assert!(capped.len() <= MAX_REVIEW_PATCH_BYTES);
        assert!(capped.is_char_boundary(capped.len()));
        let (short, truncated) = cap_patch("small");
        assert!(!truncated);
        assert_eq!(short, "small");
    }

    #[test]
    fn parses_a_findings_block_and_strips_it() {
        let answer = "Two issues found.\n\n```json\n{\"verdict\":\"needs attention\",\"findings\":[\
            {\"severity\":\"error\",\"file\":\"src/a.rs\",\"line\":12,\
             \"title\":\"Unwrap on None\",\"detail\":\"This panics.\"},\
            {\"severity\":\"info\",\"title\":\"Nit\"}]}\n```\n";
        let report = parse_report(answer);
        assert_eq!(report.verdict, Verdict::NeedsAttention);
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].severity, Severity::Error);
        assert_eq!(
            report.findings[0].location().as_deref(),
            Some("src/a.rs:12")
        );
        assert_eq!(report.findings[1].severity, Severity::Info);
        assert_eq!(report.findings[1].location(), None);
        assert_eq!(report.summary, "Two issues found.");
        assert_eq!(report.count(Severity::Error), 1);
        assert_eq!(report.count(Severity::Warning), 0);
    }

    #[test]
    fn old_format_block_defaults_the_verdict_from_severity() {
        let mixed = parse_report(
            "```json\n{\"findings\":[{\"severity\":\"warning\",\"title\":\"W\"}]}\n```",
        );
        assert_eq!(mixed.verdict, Verdict::NeedsAttention);

        let clean = parse_report(
            "```json\n{\"findings\":[{\"severity\":\"info\",\"title\":\"N\"}]}\n```",
        );
        assert_eq!(clean.verdict, Verdict::Correct);
    }

    #[test]
    fn explicit_verdict_wins() {
        let report = parse_report(
            "```json\n{\"verdict\":\"correct\",\"findings\":[{\"severity\":\"error\",\"title\":\"E\"}]}\n```",
        );
        assert_eq!(report.verdict, Verdict::Correct);
    }

    #[test]
    fn empty_findings_is_a_valid_answer() {
        let report = parse_report(
            "No issues.\n```json\n{\"verdict\":\"correct\",\"findings\":[]}\n```",
        );
        assert!(report.findings.is_empty());
        assert_eq!(report.verdict, Verdict::Correct);
        assert_eq!(report.summary, "No issues.");
    }

    #[test]
    fn malformed_or_missing_block_keeps_the_prose() {
        let report = parse_report("Just prose, no block.");
        assert!(report.findings.is_empty());
        // No block means no verdict claim: the default is the cautious one.
        assert_eq!(report.verdict, Verdict::NeedsAttention);
        assert_eq!(report.summary, "Just prose, no block.");

        let broken = parse_report("prose\n```json\n{not json}\n```");
        assert!(broken.findings.is_empty());
        assert!(broken.summary.contains("prose"));
    }

    #[test]
    fn ignores_a_non_findings_json_block() {
        let answer = "Example:\n```json\n{\"foo\":1}\n```\n```json\n{\"findings\":[]}\n```";
        let report = parse_report(answer);
        assert!(report.findings.is_empty());
        // The first, non-findings block stays in the prose.
        assert!(report.summary.contains("Example:"));
        assert!(report.summary.contains("{\"foo\":1}"));
    }

    #[test]
    fn sorted_findings_puts_errors_first() {
        let report = parse_report(
            "```json\n{\"findings\":[\
             {\"severity\":\"info\",\"title\":\"a\"},\
             {\"severity\":\"error\",\"title\":\"b\"},\
             {\"severity\":\"warning\",\"title\":\"c\"}]}\n```",
        );
        let order: Vec<&str> = report
            .sorted_findings()
            .iter()
            .map(|finding| finding.title.as_str())
            .collect();
        assert_eq!(order, ["b", "c", "a"]);
    }

    #[test]
    fn severity_maps_aliases_and_falls_back_to_info() {
        assert_eq!(Severity::from_wire("ERROR"), Severity::Error);
        assert_eq!(Severity::from_wire("critical"), Severity::Error);
        assert_eq!(Severity::from_wire("warn"), Severity::Warning);
        assert_eq!(Severity::from_wire("medium"), Severity::Warning);
        assert_eq!(Severity::from_wire("nonsense"), Severity::Info);
    }

    #[test]
    fn verdict_maps_aliases_and_defaults_to_needs_attention() {
        assert_eq!(Verdict::from_wire("CORRECT"), Verdict::Correct);
        assert_eq!(Verdict::from_wire("pass"), Verdict::Correct);
        assert_eq!(Verdict::from_wire("needs attention"), Verdict::NeedsAttention);
        assert_eq!(Verdict::from_wire("nonsense"), Verdict::NeedsAttention);
        assert_eq!(Verdict::default(), Verdict::NeedsAttention);
    }

    #[test]
    fn kinds_round_trip_through_their_wire_shape() {
        let kinds = [
            ReviewKind::Changes,
            ReviewKind::Project,
            ReviewKind::Uncommitted,
            ReviewKind::Branch {
                base: "main".into(),
            },
            ReviewKind::Commit {
                sha: "abc1234".into(),
                title: "Fix parser".into(),
            },
            ReviewKind::Files {
                paths: vec!["src/".into(), "docs/".into()],
            },
        ];
        for kind in kinds {
            assert_eq!(ReviewKind::from_json(&kind.to_json()).as_ref(), Some(&kind));
        }
        assert!(ReviewKind::from_json(&serde_json::json!({"kind": "nope"})).is_none());
        assert!(ReviewKind::from_json(&serde_json::json!({"kind": "branch"})).is_none());
    }

    #[test]
    fn only_diff_kinds_are_marked_as_diff() {
        assert!(ReviewKind::Uncommitted.is_diff());
        assert!(ReviewKind::Branch {
            base: "main".into()
        }
        .is_diff());
        assert!(!ReviewKind::Project.is_diff());
        assert!(!ReviewKind::Files { paths: vec![] }.is_diff());
    }
}
