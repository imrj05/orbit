//! GitHub issue / pull-request draft generation.
//!
//! Like [`crate::commit_message`], the primary path is a **one-shot, tool-free
//! pi call** (`pi -p --no-tools --no-session …`). It processes the prompt and
//! exits, so the working tree never touches the user's active session and no
//! tool can run in the workspace. Extensions stay enabled so a user's custom
//! providers resolve. When pi is missing, has no credentials, or times out,
//! [`heuristic`] derives an editable draft from the branch and its commits.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::git;

const MAX_CONTEXT_BYTES: usize = 24 * 1024;
const MAX_COMMITS: usize = 30;
const TIMEOUT: Duration = Duration::from_secs(180);

/// Which draft the caller is asking for. Drives the prompt's framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DraftKind {
    Issue,
    PullRequest,
}

impl DraftKind {
    fn noun(self) -> &'static str {
        match self {
            Self::Issue => "GitHub issue",
            Self::PullRequest => "GitHub pull request",
        }
    }

    fn heading(self) -> &'static str {
        match self {
            Self::Issue => "issue",
            Self::PullRequest => "pull request",
        }
    }
}

/// A generated draft: one title line plus a Markdown body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub body: String,
}

impl Draft {
    fn is_empty(&self) -> bool {
        self.title.trim().is_empty() && self.body.trim().is_empty()
    }
}

/// Everything the prompt knows about the repository, sampled once so the
/// blocking git reads happen together.
#[derive(Default)]
struct RepoContext {
    repo: String,
    branch: Option<String>,
    base: Option<String>,
    commits: Vec<String>,
    changed: Vec<String>,
}

/// Generate a draft for `kind`. Blocking; call on the background executor.
/// `hint` is the user's free-form notes (may be empty); `base` is the PR's
/// target branch when known.
pub fn generate(
    cwd: &Path,
    provider: Option<&str>,
    model: Option<&str>,
    kind: DraftKind,
    hint: &str,
    base: Option<&str>,
) -> Result<Draft, String> {
    let context = collect(cwd, base);
    let system = system_prompt(kind);
    let prompt = user_prompt(kind, hint, &context);
    let raw = run_pi(cwd, provider, model, &system, &prompt)?;
    process_response(&raw).ok_or_else(|| tr!("errors.no_draft"))
}

/// A no-model fallback: a title from the hint or branch, and a Markdown body
/// built from the branch's commits and changed files. Always editable, never
/// empty when there is anything to say.
pub fn heuristic(cwd: &Path, kind: DraftKind, hint: &str, base: Option<&str>) -> Draft {
    let context = collect(cwd, base);
    let hint = hint.trim();
    let title = heuristic_title(kind, hint, &context);
    let body = heuristic_body(kind, hint, &context);
    Draft { title, body }
}

/// Sample repository identity, commits, and changed files in one pass.
fn collect(cwd: &Path, base: Option<&str>) -> RepoContext {
    let repo = cwd
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let branch = git::current_branch(cwd);
    let range = base
        .filter(|base| !base.is_empty())
        .map(|base| format!("{base}..HEAD"));
    let commits = match &range {
        Some(range) => git::run_git(
            cwd,
            &["log", "--no-decorate", "--format=%s", "-n", "30", range],
        ),
        None => git::run_git(
            cwd,
            &["log", "--no-decorate", "--format=%s", "-n", "20"],
        ),
    }
    .map(|out| {
        out.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(MAX_COMMITS)
            .map(str::to_string)
            .collect()
    })
    .unwrap_or_default();

    let mut changed = Vec::new();
    let stat_args: Vec<Vec<&str>> = match &range {
        Some(range) => vec![vec!["diff", "--stat", range]],
        None => vec![
            vec!["diff", "--stat", "HEAD"],
            vec!["diff", "--stat"],
            vec!["diff", "--stat", "--cached"],
        ],
    };
    for args in stat_args {
        if let Ok(out) = git::run_git(cwd, &args) {
            for line in out.lines().map(str::trim).filter(|line| !line.is_empty()) {
                if !changed.iter().any(|seen| seen == line) {
                    changed.push(line.to_string());
                }
            }
        }
    }
    changed.truncate(60);

    RepoContext {
        repo,
        branch,
        base: base.map(str::to_string),
        commits,
        changed,
    }
}

/// The system rules. Output shape mirrors [`crate::commit_message`]: a single
/// fenced block so the parser is identical in spirit.
fn system_prompt(kind: DraftKind) -> String {
    format!(
        "You are an AI assistant helping a software developer write a {noun}.\n\
         You excel at turning sparse notes and repository context into concise, concrete prose that a maintainer can act on.\n\n\
         # Rules:\n\
         1. The TITLE is a single line: specific, no trailing period, no `Issue:`/`PR:`/`#` prefix, under ~72 characters.\n\
         2. The BODY is GitHub-flavored Markdown. Lead with the *why*, then the *what*.\n\
         3. For a pull request, include a `## Summary` section and a `## Testing` section when the context reveals how it was verified; otherwise omit Testing.\n\
         4. For an issue, include `## What happened` and `## Expected` sections when they fit the notes.\n\
         5. Use bullets and short headings. Do not pad. Do not invent facts that the provided context does not support.\n\
         6. Never mention that the text was generated, and never add meta-commentary.\n\
         7. Output EXACTLY one fenced ```text block: the title on the first line, a blank line, then the Markdown body. No other prose.\n",
        noun = kind.noun()
    )
}

/// The user message: repository context, the user's notes, and a closing
/// reminder. `hint` is optional and only shapes the draft when present.
fn user_prompt(kind: DraftKind, hint: &str, context: &RepoContext) -> String {
    let mut prompt = String::new();
    prompt.push_str("<repository-context>\n");
    prompt.push_str(&format!("Repository name: {}\n", context.repo));
    prompt.push_str(&format!(
        "Branch name: {}\n",
        context.branch.as_deref().unwrap_or("(detached HEAD)")
    ));
    if let Some(base) = &context.base {
        prompt.push_str(&format!("Target base branch: {base}\n"));
    }
    prompt.push_str("</repository-context>\n\n");

    let hint = hint.trim();
    if !hint.is_empty() {
        prompt.push_str("<user-notes>\n");
        prompt.push_str(&truncate(hint, MAX_CONTEXT_BYTES));
        prompt.push_str("\n</user-notes>\n\n");
    } else {
        prompt.push_str("<user-notes>\n(none — infer the subject from the repository context)\n</user-notes>\n\n");
    }

    if !context.commits.is_empty() {
        prompt.push_str("<commits>\n# COMMITS ON THIS BRANCH\n");
        for subject in &context.commits {
            prompt.push_str(&format!("- {subject}\n"));
        }
        prompt.push_str("</commits>\n\n");
    }

    if !context.changed.is_empty() {
        prompt.push_str("<changed-files>\n# CHANGED FILES\n");
        for line in &context.changed {
            prompt.push_str(line);
            prompt.push('\n');
        }
        prompt.push_str("</changed-files>\n\n");
    }

    prompt.push_str(&format!(
        "<reminder>\n\
         Write a {heading} draft from the context above.\n\
         Title first, then a blank line, then the Markdown body.\n\
         ONLY return a single markdown code block, NO OTHER PROSE!\n\
         ```text\n\
         title goes here\n\n\
         body goes here\n\
         ```\n\
         </reminder>",
        heading = kind.heading()
    ));
    prompt
}

/// The fallback title: the user's first note line when present, else the
/// branch name read as words, else the latest commit subject.
fn heuristic_title(kind: DraftKind, hint: &str, context: &RepoContext) -> String {
    if let Some(line) = hint.lines().map(str::trim).find(|line| !line.is_empty()) {
        return clean_title(line);
    }
    if let Some(subject) = context.commits.first() {
        return clean_title(subject);
    }
    match &context.branch {
        Some(branch) => clean_title(&humanize(branch)),
        None => match kind {
            DraftKind::Issue => tr!("git_panel.draft_issue_fallback_title"),
            DraftKind::PullRequest => tr!("git_panel.draft_pull_fallback_title"),
        },
    }
}

/// The fallback body: a short summary, then the commit subjects and the
/// changed files, so the field is never blank.
fn heuristic_body(kind: DraftKind, hint: &str, context: &RepoContext) -> String {
    let mut body = String::new();
    let hint_rest = hint
        .lines()
        .skip(1)
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if !hint_rest.is_empty() {
        body.push_str(&hint_rest);
        body.push_str("\n\n");
    }
    let summary_heading = match kind {
        DraftKind::Issue => tr!("git_panel.draft_what_happened"),
        DraftKind::PullRequest => tr!("git_panel.draft_summary"),
    };
    body.push_str(&format!("## {summary_heading}\n\n"));
    if !context.commits.is_empty() {
        for subject in context.commits.iter().take(10) {
            body.push_str(&format!("- {subject}\n"));
        }
    } else if !hint.trim().is_empty() {
        body.push_str(&format!("- {}\n", hint.trim()));
    } else {
        body.push_str(&format!("- {}\n", tr!("git_panel.draft_placeholder")));
    }
    if !context.changed.is_empty() {
        let files_heading = tr!("git_panel.draft_changed_files");
        body.push_str(&format!("\n## {files_heading}\n\n"));
        for line in context.changed.iter().take(20) {
            body.push_str(&format!("- `{line}`\n"));
        }
    }
    body.trim_end().to_string()
}

/// A branch name read as words (`feat/git-issues-pr-ui` → `git issues pr ui`).
fn humanize(branch: &str) -> String {
    branch
        .rsplit('/')
        .next()
        .unwrap_or(branch)
        .replace(['-', '_'], " ")
        .trim()
        .to_string()
}

/// Strip a list marker, a leading type prefix, and trailing punctuation so a
/// title reads as a title regardless of how it arrived.
fn clean_title(raw: &str) -> String {
    let line = raw
        .trim()
        .trim_start_matches(['-', '*', '•', '#'])
        .trim()
        .trim_matches(['"', '`', '\'']);
    let line = line
        .strip_prefix("Title:")
        .or_else(|| line.strip_prefix("title:"))
        .map(str::trim)
        .unwrap_or(line);
    let mut title = line.trim_end_matches(['.', ':', ';']).trim().to_string();
    if title.len() > 120 {
        let mut end = 120;
        while end > 0 && !title.is_char_boundary(end) {
            end -= 1;
        }
        title.truncate(end);
    }
    title
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n… [truncated]", &text[..end])
}

fn run_pi(
    cwd: &Path,
    provider: Option<&str>,
    model: Option<&str>,
    system: &str,
    prompt: &str,
) -> Result<String, String> {
    let bin = orbit_rpc::pi_binary();
    let mut command = Command::new(&bin);
    command
        .arg("-p")
        // Extensions stay enabled so custom providers resolve (see
        // `commit_message::run_pi`); every capability that could run code is
        // still disabled.
        .args([
            "--no-tools",
            "--no-session",
            "--no-skills",
            "--no-context-files",
            "--no-approve",
        ])
        .arg("--system-prompt")
        .arg(system)
        .env("PI_SKIP_VERSION_CHECK", "1")
        // `pi` is `#!/usr/bin/env node`; a bundled `.app` PATH lacks `node`.
        .env("PATH", orbit_rpc::augmented_path(Path::new(&bin).parent()))
        .env("NO_COLOR", "1")
        .env("CI", "1")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(provider) = provider.filter(|value| !value.is_empty()) {
        command.args(["--provider", provider]);
    }
    if let Some(model) = model.filter(|value| !value.is_empty()) {
        command.args(["--model", model]);
    }
    command.arg("--").arg(prompt);
    orbit_rpc::hide_console(&mut command);

    let mut child = command
        .spawn()
        .map_err(|err| tr!("errors.failed_to_run", bin = bin, error = err))?;
    let stdout = child.stdout.take().ok_or("pi stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("pi stderr unavailable")?;
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
                    Err(tr!(
                        "errors.pi_exited_with",
                        status = status.to_string(),
                        stderr = first_line(&err)
                    ))
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(tr!("errors.draft_generation_timed_out"));
                }
                std::thread::sleep(Duration::from_millis(40));
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
        .find(|line| !line.is_empty())
        .unwrap_or(&tr!("commit_message.no_error_output"))
        .to_string()
}

/// Extract the title and body from the model output. Tolerates a preamble or
/// trailing prose around the fence, and a missing fence (title-first text).
fn process_response(raw: &str) -> Option<Draft> {
    let trimmed = raw.trim();
    let fenced = extract_fence(trimmed, "```text")
        .or_else(|| extract_fence(trimmed, "```markdown"))
        .or_else(|| extract_fence(trimmed, "```md"))
        .or_else(|| extract_fence(trimmed, "```"));
    let text = match fenced {
        Some(body) => body,
        None => {
            let raw = trimmed.trim_matches('"').trim();
            // A lone, empty fence is not a draft; reject it rather than
            // reading the marker itself as a title.
            if raw.starts_with("```") {
                return None;
            }
            raw.to_string()
        }
    };
    let mut lines = text.lines();
    let title = lines
        .by_ref()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(clean_title)
        .unwrap_or_default();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    let draft = Draft { title, body };
    (!draft.is_empty()).then_some(draft)
}

/// The body of the first fence opened with `marker` at the start of a line.
fn extract_fence(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)?;
    if start != 0 && !text[..start].ends_with('\n') {
        return None;
    }
    let rest = &text[start + marker.len()..];
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    let end = rest.find("\n```")?;
    let body = rest[..end].trim();
    (!body.is_empty()).then(|| body.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_title_and_body_from_a_text_fence() {
        let draft = process_response("```text\nfeat: add issue generator\n\n## Summary\n\n- drafts a title\n```")
            .unwrap();
        assert_eq!(draft.title, "feat: add issue generator");
        assert!(draft.body.contains("## Summary"));
        assert!(draft.body.contains("- drafts a title"));
    }

    #[test]
    fn tolerates_preamble_and_trailing_prose() {
        let raw = "Here you go:\n```text\nFix the flaky login test\n\n## What happened\n\nIt fails 1 in 10 runs.\n```\nLet me know!";
        let draft = process_response(raw).unwrap();
        assert_eq!(draft.title, "Fix the flaky login test");
        assert!(draft.body.contains("## What happened"));
    }

    #[test]
    fn accepts_markdown_fence_and_title_first_prose() {
        let fenced = process_response("```markdown\nAdd retry to the uploader\n\n## Summary\n\nRetries twice.\n```")
            .unwrap();
        assert_eq!(fenced.title, "Add retry to the uploader");

        let prose = process_response("Add retry to the uploader\n\n## Summary\n\nRetries twice.").unwrap();
        assert_eq!(prose.title, "Add retry to the uploader");
        assert!(prose.body.contains("## Summary"));
    }

    #[test]
    fn clean_title_strips_prefixes_and_punctuation() {
        assert_eq!(clean_title("- Title: fix the thing."), "fix the thing");
        assert_eq!(clean_title("`Add the widget`"), "Add the widget");
        assert_eq!(clean_title("## feat: do stuff:"), "feat: do stuff");
    }

    #[test]
    fn process_response_rejects_blank_output() {
        assert!(process_response("   \n  ").is_none());
        assert!(process_response("```text\n```").is_none());
    }

    #[test]
    fn heuristic_uses_the_hint_first_line_as_the_title() {
        let context = RepoContext {
            repo: "orbit".into(),
            branch: Some("feat/git-issues-pr-ui".into()),
            base: Some("main".into()),
            commits: vec!["feat(git): tidy the issue list".into()],
            changed: vec!["git_panel/mod.rs | 20 +++++".into()],
        };
        assert_eq!(
            heuristic_title(DraftKind::PullRequest, "Improve the pages\n\nMore.", &context),
            "Improve the pages"
        );
        let body = heuristic_body(DraftKind::PullRequest, "Improve the pages\n\nMore.", &context);
        assert!(body.contains("More."));
        assert!(body.contains("## Summary"));
        assert!(body.contains("- feat(git): tidy the issue list"));
    }

    #[test]
    fn heuristic_falls_back_to_the_branch_or_latest_commit() {
        let context = RepoContext {
            repo: "orbit".into(),
            branch: Some("feat/git-issues-pr-ui".into()),
            base: None,
            commits: vec!["feat(git): tidy the issue list".into()],
            changed: Vec::new(),
        };
        assert_eq!(
            heuristic_title(DraftKind::Issue, "", &context),
            "feat(git): tidy the issue list"
        );
        let no_commits = RepoContext {
            branch: Some("fix/flaky-login".into()),
            ..RepoContext::default()
        };
        assert_eq!(
            heuristic_title(DraftKind::Issue, "", &no_commits),
            "flaky login"
        );
    }

    #[test]
    fn prompt_carries_context_and_reminder() {
        let context = RepoContext {
            repo: "orbit".into(),
            branch: Some("feat/x".into()),
            base: Some("main".into()),
            commits: vec!["feat: a".into()],
            changed: vec!["src/lib.rs | 2 +-".into()],
        };
        let prompt = user_prompt(DraftKind::PullRequest, "notes here", &context);
        assert!(prompt.contains("Repository name: orbit"));
        assert!(prompt.contains("Target base branch: main"));
        assert!(prompt.contains("<user-notes>"));
        assert!(prompt.contains("notes here"));
        assert!(prompt.contains("feat: a"));
        assert!(prompt.contains("src/lib.rs | 2 +-"));
        assert!(prompt.contains("ONLY return a single markdown code block"));

        let system = system_prompt(DraftKind::Issue);
        assert!(system.contains("GitHub issue"));
        assert!(system.contains("EXACTLY one fenced"));
    }
}
