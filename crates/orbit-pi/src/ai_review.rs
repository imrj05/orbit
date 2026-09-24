//! AI review agent — a read-only reviewer over the workspace changes or the
//! whole project, whose findings render in the Review pane.
//!
//! Orbit runs the reviewer on its **own** pi process scoped to **Ask mode**
//! (read-only via the workflow extension), so the user's chat session and
//! transcript are never touched. The process is launched with `ORBIT_REVIEW=1`,
//! which the bundled guard extension treats as pre-approved — the workflow
//! extension remains the stricter read-only gate. The process lifecycle lives
//! in [`crate::app::ai_review`].
//!
//! This module is pure: it builds the reviewer prompt and parses the structured
//! findings block out of the final answer. No I/O, no GPUI — unit-tested.

use serde_json::Value;

/// Cap on the diff text embedded in the changes prompt. A pathological patch
/// must not turn one request into an unbounded prompt; the reviewer can still
/// read the full files itself.
pub const MAX_REVIEW_PATCH_BYTES: usize = 200_000;

/// What the reviewer is asked to inspect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewKind {
    /// The working-tree / turn changes for the selected Review source.
    Changes,
    /// The whole project tree.
    Project,
}

impl ReviewKind {
    /// Label for the pane header and palette rows.
    pub fn label(self) -> String {
        match self {
            Self::Changes => tr!("ai_review.review_changes"),
            Self::Project => tr!("ai_review.review_project"),
        }
    }

    /// One-line description shown in the pane while running.
    pub fn description(self) -> String {
        match self {
            Self::Changes => tr!("ai_review.changes_hint"),
            Self::Project => tr!("ai_review.project_hint"),
        }
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

/// A parsed reviewer answer: the structured findings plus the answer's prose
/// with the findings block removed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// The answer's remaining markdown (usually a short summary). Empty when
    /// the model returned only the findings block.
    pub summary: String,
}

impl Report {
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty() && self.summary.trim().is_empty()
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

/// The findings-contract shared by both prompts. Kept in one place so the
/// parser and the instructions can never drift.
const FINDINGS_CONTRACT: &str = "\
Then answer in exactly two parts:
1. A short prose summary (a few bullet points at most).
2. One fenced ```json block holding the findings, in exactly this shape:
```json
{\"findings\":[{\"severity\":\"error|warning|info\",\"file\":\"relative/path\",\"line\":42,\"title\":\"Short summary\",\"detail\":\"Why it matters and how to fix it\"}]}
```
Use \"error\" for bugs, crashes, security holes, or data loss; \"warning\" for
likely problems; \"info\" for suggestions. \"file\" and \"line\" are optional.
If there are no issues, return {\"findings\":[]}.";

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
- Focus on correctness, security, error handling, data loss, concurrency, and \
regressions. Skip style nits unless they hide a bug.
- Cite the file and line where the problem lives.
- Be concise. Only report issues you are confident about; do not invent problems.
{note}
Diff:
```diff
{patch}
```

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
- Focus on correctness, security, error handling, data loss, concurrency, and \
regressions. Skip style nits unless they hide a bug.
- Cite the file and line where the problem lives.
- Prefer a handful of real issues over a long list of speculation.

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
        if let Some(findings) = findings_from_json(&block.body) {
            let mut summary = markdown[..block.start].trim_end().to_string();
            let tail = markdown[block.end..].trim();
            if !tail.is_empty() {
                if !summary.is_empty() {
                    summary.push_str("\n\n");
                }
                summary.push_str(tail);
            }
            return Report {
                findings,
                summary,
            };
        }
    }
    Report {
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

/// Extract findings from a JSON body, or `None` when it is not a findings
/// payload (so a stray example block is skipped rather than treated as empty).
fn findings_from_json(body: &str) -> Option<Vec<Finding>> {
    let value: Value = serde_json::from_str(body.trim()).ok()?;
    let array = value.get("findings")?.as_array()?;
    Some(array.iter().filter_map(finding_from_value).collect())
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
        assert!(prompt.contains("{\"findings\":"));
        assert!(!prompt.contains("truncated"));
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
        assert!(prompt.contains("{\"findings\":"));
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
        let answer = "Two issues found.\n\n```json\n{\"findings\":[\
            {\"severity\":\"error\",\"file\":\"src/a.rs\",\"line\":12,\
             \"title\":\"Unwrap on None\",\"detail\":\"This panics.\"},\
            {\"severity\":\"info\",\"title\":\"Nit\"}]}\n```\n";
        let report = parse_report(answer);
        assert_eq!(report.findings.len(), 2);
        assert_eq!(report.findings[0].severity, Severity::Error);
        assert_eq!(report.findings[0].location().as_deref(), Some("src/a.rs:12"));
        assert_eq!(report.findings[1].severity, Severity::Info);
        assert_eq!(report.findings[1].location(), None);
        assert_eq!(report.summary, "Two issues found.");
    }

    #[test]
    fn empty_findings_is_a_valid_answer() {
        let report = parse_report("No issues.\n```json\n{\"findings\":[]}\n```");
        assert!(report.findings.is_empty());
        assert_eq!(report.summary, "No issues.");
    }

    #[test]
    fn malformed_or_missing_block_keeps_the_prose() {
        let report = parse_report("Just prose, no block.");
        assert!(report.findings.is_empty());
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
    fn severity_maps_aliases_and_falls_back_to_info() {
        assert_eq!(Severity::from_wire("ERROR"), Severity::Error);
        assert_eq!(Severity::from_wire("critical"), Severity::Error);
        assert_eq!(Severity::from_wire("warn"), Severity::Warning);
        assert_eq!(Severity::from_wire("medium"), Severity::Warning);
        assert_eq!(Severity::from_wire("nonsense"), Severity::Info);
    }
}
