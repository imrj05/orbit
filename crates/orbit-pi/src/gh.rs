//! GitHub integration through the official `gh` CLI.
//!
//! Orbit never stores a GitHub token and never calls `api.github.com` directly.
//! It shells out to the GitHub CLI, which already owns the user's
//! authentication and the API contract, and parses its `--json` output into
//! typed values. The binary is resolved like `pi` is (a `GH_BIN` override, then
//! `PATH`), and every invocation runs with a quiet, non-interactive
//! environment so a missing credential fails fast instead of opening a prompt.
//!
//! This module currently probes availability and sign-in state. Issue and PR
//! listing and mutations land in later phases; keeping the runner and parsers
//! here means they can be unit-tested without a network.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Deserializer};

/// Environment override for the `gh` binary, mirroring `PI_BIN`.
pub const GH_BIN_ENV: &str = "GH_BIN";

/// The oldest `gh` whose `--json` surface this client relies on.
pub const MIN_GH_VERSION: (u32, u32, u32) = (2, 20, 0);

#[cfg(windows)]
const GH_BIN_NAMES: &[&str] = &["gh.exe", "gh.cmd", "gh.bat", "gh.ps1"];
#[cfg(not(windows))]
const GH_BIN_NAMES: &[&str] = &["gh"];

/// What probing the GitHub CLI found. Drives the setup empty state on the
/// Issues and Pull requests tabs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhStatus {
    /// The CLI is installed and runnable.
    pub installed: bool,
    /// `gh auth status` reported a signed-in host.
    pub authenticated: bool,
    /// Human-readable context: the auth error, or the resolved version line.
    pub detail: String,
}

impl GhStatus {
    /// The not-installed state.
    pub fn missing() -> Self {
        Self {
            installed: false,
            authenticated: false,
            detail: String::new(),
        }
    }
}

/// Probe `gh` for the current workspace. Never fails: a missing or broken
/// binary is reported through [`GhStatus`], not an error, so the page can show
/// an honest setup state instead of a red banner.
pub fn status(cwd: &Path) -> GhStatus {
    let Ok(version_out) = run(cwd, &["--version"]) else {
        return GhStatus::missing();
    };
    let version = version_out.lines().next().unwrap_or("").trim().to_string();
    let supported = parse_version(&version).is_none_or(|v| v >= MIN_GH_VERSION);
    match run(cwd, &["auth", "status"]) {
        Ok(_) if supported => GhStatus {
            installed: true,
            authenticated: true,
            detail: version,
        },
        // An old CLI still reports auth, but the `--json` fields this client
        // asks for may not exist yet, so surface the mismatch.
        Ok(_) => GhStatus {
            installed: true,
            authenticated: true,
            detail: format!("{version} — {}", tr!("git_panel.gh_version_old")),
        },
        Err(err) => GhStatus {
            installed: true,
            authenticated: false,
            detail: err,
        },
    }
}

/// The signed-in `gh` user (login, name, avatar) for the Git branch row's
/// account chip. Only called once [`status`] has reported authentication.
pub fn current_user(cwd: &Path) -> Result<GhUser, String> {
    let raw = run(cwd, &["api", "user"])?;
    serde_json::from_str(&raw).map_err(|err| err.to_string())
}

/// The resolved `gh` executable path. Falls back to the same Homebrew,
/// system, and shim dirs as `pi`/`node`, since a bundled `.app` launches
/// with a minimal PATH that omits a user's install.
pub fn binary() -> String {
    if let Ok(bin) = std::env::var(GH_BIN_ENV) {
        if !bin.is_empty() {
            return bin;
        }
    }
    for name in GH_BIN_NAMES {
        if let Some(found) = crate::onboarding::locate(name) {
            return found.to_string_lossy().into_owned();
        }
    }
    GH_BIN_NAMES[0].to_string()
}

/// A `gh` invocation rooted at `cwd`, with prompts disabled so it fails rather
/// than blocking on a TTY that does not exist.
pub(crate) fn command(cwd: &Path) -> Command {
    let mut command = Command::new(binary());
    command.current_dir(cwd);
    command.env("GH_PROMPT_DISABLED", "1");
    command.env("GH_NO_UPDATE_NOTIFIER", "1");
    command.env("GH_PAGER", "cat");
    command.env("NO_COLOR", "1");
    command.env("CLICOLOR", "0");
    orbit_rpc::hide_console(&mut command);
    command
}

/// Run `gh` with `args`, returning trimmed stdout or the trimmed stderr.
fn run(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = command(cwd)
        .args(args)
        .output()
        .map_err(|err| tr!("git_panel.gh_not_available", error = err))?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(tr!(
            "git_panel.gh_exited_with",
            status = output.status.to_string()
        ))
    } else {
        Err(stderr)
    }
}

/// Parse a semantic version out of `gh --version` output, e.g.
/// `gh version 2.40.1 (2024-01-01)` → `(2, 40, 1)`.
pub fn parse_version(raw: &str) -> Option<(u32, u32, u32)> {
    for token in raw.split_whitespace() {
        let token = token.trim_start_matches('v');
        let mut parts = token.split('.');
        let (Some(a), Some(b), Some(c)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let b = b.trim_end_matches(|ch: char| !ch.is_ascii_digit());
        let c = c.trim_end_matches(|ch: char| !ch.is_ascii_digit());
        if let (Ok(a), Ok(b), Ok(c)) = (a.parse::<u32>(), b.parse::<u32>(), c.parse::<u32>()) {
            return Some((a, b, c));
        }
    }
    None
}

/// Latest stable GitHub CLI release. This anonymous request is made only on
/// explicit user action, never during Git Accounts' local status refresh.
pub(crate) fn latest_cli_version() -> Option<(u32, u32, u32)> {
    let response = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?
        .head("https://github.com/cli/cli/releases/latest")
        .header("User-Agent", concat!("Orbit/", env!("CARGO_PKG_VERSION")))
        .send()
        .ok()?;
    if !response.status().is_redirection() {
        return None;
    }
    parse_latest_release_location(
        response
            .headers()
            .get(reqwest::header::LOCATION)?
            .to_str()
            .ok()?,
    )
}

fn parse_latest_release_location(location: &str) -> Option<(u32, u32, u32)> {
    let url = url::Url::parse(location).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let version = url.path().strip_prefix("/cli/cli/releases/tag/v")?;
    let mut parts = version.split('.');
    let parse = |s: &str| {
        (!s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    };
    let result = (
        parse(parts.next()?)?,
        parse(parts.next()?)?,
        parse(parts.next()?)?,
    );
    parts.next().is_none().then_some(result)
}

// ── Issues ─────────────────────────────────────────────────────────────

/// A GitHub user as `gh` serializes it.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhUser {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    /// The user's avatar URL, for the Git branch row's account chip.
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// A repository label.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhLabel {
    pub name: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// One issue comment.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhComment {
    #[serde(default)]
    pub author: Option<GhUser>,
    #[serde(default)]
    pub body: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(default)]
    pub url: String,
}

/// An issue as returned by `gh issue list` / `gh issue view`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhIssue {
    pub number: u64,
    pub title: String,
    /// `OPEN` or `CLOSED`.
    pub state: String,
    #[serde(default)]
    pub body: String,
    pub author: GhUser,
    #[serde(default)]
    pub labels: Vec<GhLabel>,
    #[serde(default)]
    pub assignees: Vec<GhUser>,
    #[serde(default)]
    pub comments: Vec<GhComment>,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
    #[serde(default)]
    pub url: String,
}

impl GhIssue {
    pub fn is_open(&self) -> bool {
        self.state.eq_ignore_ascii_case("open")
    }
}

/// The issue list's state filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueState {
    #[default]
    Open,
    Closed,
    All,
}

impl IssueState {
    fn flag(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }
}

/// Filters for the issue list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueFilter {
    pub state: IssueState,
    pub label: Option<String>,
    pub assignee: Option<String>,
    pub search: Option<String>,
}

impl IssueFilter {
    pub fn is_active(&self) -> bool {
        self.state != IssueState::Open
            || self.label.as_deref().is_some_and(|v| !v.is_empty())
            || self.assignee.as_deref().is_some_and(|v| !v.is_empty())
            || self.search.as_deref().is_some_and(|v| !v.is_empty())
    }
}

const ISSUE_LIST_FIELDS: &str =
    "number,title,state,author,labels,assignees,comments,createdAt,updatedAt,url";
const ISSUE_VIEW_FIELDS: &str =
    "number,title,state,body,author,labels,assignees,comments,createdAt,updatedAt,url";

/// The repository's issues, newest first, under `filter`.
pub fn list_issues(cwd: &Path, filter: &IssueFilter) -> Result<Vec<GhIssue>, String> {
    let mut args = vec![
        "issue",
        "list",
        "--json",
        ISSUE_LIST_FIELDS,
        "--limit",
        "50",
        "--state",
        filter.state.flag(),
    ];
    for (flag, value) in [
        ("--label", filter.label.as_deref()),
        ("--assignee", filter.assignee.as_deref()),
        ("--search", filter.search.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            args.push(flag);
            args.push(value);
        }
    }
    parse_json(&run(cwd, &args)?)
}

/// One issue with its comments.
pub fn view_issue(cwd: &Path, number: u64) -> Result<GhIssue, String> {
    let number = number.to_string();
    let mut issue: GhIssue = parse_json(&run(
        cwd,
        &["issue", "view", &number, "--json", ISSUE_VIEW_FIELDS],
    )?)?;
    normalize_issue(&mut issue);
    Ok(issue)
}

/// Create an issue, returning `gh`'s printed URL. `repo` targets an explicit
/// `owner/name`; `None` uses the repository of `cwd` (the workspace's own
/// issue tracker). The in-app bug report passes the project's repository so a
/// report is filed there no matter which workspace is open. `attachments` are
/// local image/video paths uploaded with `--attach` and appended to the body.
pub fn create_issue(
    cwd: &Path,
    repo: Option<&str>,
    title: &str,
    body: &str,
    labels: &[String],
    assignees: &[String],
    attachments: &[PathBuf],
) -> Result<String, String> {
    let mut args: Vec<String> = vec!["issue".into(), "create".into()];
    if let Some(repo) = repo.map(str::trim).filter(|repo| !repo.is_empty()) {
        args.push("--repo".into());
        args.push(repo.to_string());
    }
    args.push("--title".into());
    args.push(title.into());
    args.push("--body".into());
    args.push(body.into());
    for attachment in attachments {
        args.push("--attach".into());
        args.push(attachment.to_string_lossy().into_owned());
    }
    for label in labels {
        args.push("--label".into());
        args.push(label.clone());
    }
    for assignee in assignees {
        args.push("--assignee".into());
        args.push(assignee.clone());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(cwd, &refs)
}

/// Add a comment to an issue.
pub fn comment_issue(cwd: &Path, number: u64, body: &str) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["issue", "comment", &number, "--body", body])?;
    Ok(tr!("git_panel.gh_comment_added"))
}

/// Add and remove labels on an issue.
pub fn edit_issue_labels(
    cwd: &Path,
    number: u64,
    add: &[String],
    remove: &[String],
) -> Result<String, String> {
    let number = number.to_string();
    let mut args: Vec<String> = vec!["issue".into(), "edit".into(), number];
    for label in add {
        args.push("--add-label".into());
        args.push(label.clone());
    }
    for label in remove {
        args.push("--remove-label".into());
        args.push(label.clone());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(cwd, &refs)?;
    Ok(tr!("git_panel.gh_labels_updated"))
}

/// Close an issue, optionally as `completed` or `not planned`.
pub fn close_issue(cwd: &Path, number: u64, reason: Option<&str>) -> Result<String, String> {
    let number = number.to_string();
    let mut args = vec!["issue", "close", &number];
    if let Some(reason) = reason {
        args.push("--reason");
        args.push(reason);
    }
    run(cwd, &args)?;
    Ok(tr!("git_panel.gh_issue_closed"))
}

/// Reopen a closed issue.
pub fn reopen_issue(cwd: &Path, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["issue", "reopen", &number])?;
    Ok(tr!("git_panel.gh_issue_reopened"))
}

/// Every label in the repository, for the label picker.
pub fn list_labels(cwd: &Path) -> Result<Vec<GhLabel>, String> {
    parse_json(&run(
        cwd,
        &[
            "label",
            "list",
            "--json",
            "name,color,description",
            "--limit",
            "100",
        ],
    )?)
}

fn parse_json<T: serde::de::DeserializeOwned>(out: &str) -> Result<T, String> {
    serde_json::from_str(out).map_err(|err| tr!("git_panel.gh_json_error", error = err))
}

// ── GitHub HTML → Markdown ───────────────────────────────────────────

/// GitHub stores Dependabot/Renovate bodies (and some comments and reviews)
/// with raw HTML: `<details>`, `<summary>`, `<ul>`, `<li>`, `<a href>`,
/// `<h2>`, `<code>`, `<em>`, `<blockquote>`, `<br>`. Orbit's Markdown
/// renderer prints unknown tags literally, so convert the common GitHub HTML
/// to Markdown before it reaches the transcript renderer. Raw `<img>` tags
/// become Markdown images, which the renderer already understands.
pub fn markdown_body(raw: &str) -> String {
    if !looks_like_html(raw) {
        return raw.to_string();
    }
    let (protected, code, marker) = protect_markdown_code(raw);
    let mut result = decode_entities(&collapse_blank_lines(&normalize_html(&protected)));
    for (index, original) in code.iter().enumerate() {
        result = result.replace(&format!("{marker}{index}END"), original);
    }
    result
}

/// Hide Markdown code from the HTML converter, including entity decoding.
fn protect_markdown_code(raw: &str) -> (String, Vec<&str>, String) {
    let mut marker = "ORBITCODETOKEN".to_string();
    while raw.contains(&marker) {
        marker.push('X');
    }
    let mut output = String::with_capacity(raw.len());
    let mut code = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut indented_code = false;
    for line in raw.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = content.trim_start_matches(' ');
        let indent = content.len() - trimmed.len();
        let first = trimmed.as_bytes().first().copied();
        let run = first.map_or(0, |ch| trimmed.bytes().take_while(|&b| b == ch).count());
        let fence_line = indent <= 3 && matches!(first, Some(b'`' | b'~')) && run >= 3;
        let in_fence = fence.is_some();
        if let Some((ch, length)) = fence {
            if fence_line && first == Some(ch) && run >= length && trimmed[run..].trim().is_empty()
            {
                fence = None;
            }
        } else if fence_line {
            fence = Some((first.unwrap_or_default(), run));
        }
        let indented = content.starts_with("    ") || content.starts_with('\t');
        indented_code = indented || (indented_code && content.trim().is_empty());
        if in_fence || fence.is_some() || fence_line || indented_code {
            output.push_str(&format!("{marker}{}END", code.len()));
            code.push(content);
        } else {
            let mut rest = content;
            while let Some(start) = rest.find('`') {
                output.push_str(&rest[..start]);
                let ticks = rest[start..].bytes().take_while(|&b| b == b'`').count();
                let delimiter = "`".repeat(ticks);
                let after = start + ticks;
                if let Some(end) = rest[after..].find(&delimiter) {
                    let end = after + end + ticks;
                    output.push_str(&format!("{marker}{}END", code.len()));
                    code.push(&rest[start..end]);
                    rest = &rest[end..];
                } else {
                    output.push_str(&rest[start..]);
                    rest = "";
                    break;
                }
            }
            output.push_str(rest);
        }
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    (output, code, marker)
}

/// A body needs normalization only when it actually carries an HTML tag, not
/// when prose merely mentions `<something>` — a tag is `<` + a letter or `/`.
fn looks_like_tag_at(text: &str, at: usize) -> bool {
    text[at + 1..]
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!')
}

fn looks_like_html(text: &str) -> bool {
    let bytes = text.as_bytes();
    (0..bytes.len().saturating_sub(1)).any(|i| bytes[i] == b'<' && looks_like_tag_at(text, i))
}

/// One parsed tag: its lowercase name, whether it closes, and its raw body
/// (`href="…" src="…" …`), with `rest` pointing past the `>`.
fn take_tag(mut rest: &str) -> Option<(String, bool, &str, &str)> {
    rest = rest.strip_prefix('<')?;
    let end = rest.find('>')?;
    let inner = &rest[..end];
    let after = &rest[end + 1..];
    let (closing, inner) = match inner.strip_prefix('/') {
        Some(inner) => (true, inner.trim_start()),
        None => (false, inner.trim()),
    };
    let name_len = inner
        .find(|c: char| c.is_ascii_whitespace() || c == '/')
        .unwrap_or(inner.len());
    Some((
        inner[..name_len].to_ascii_lowercase(),
        closing,
        inner[name_len..].trim(),
        after,
    ))
}

/// The value of one attribute in a tag body (`href="…"` or `href='…'`),
/// matched case-insensitively. Values are extracted from the original body by
/// byte offset whenever lowercasing preserves length (true for ASCII HTML);
/// otherwise the exact-case scan covers real GitHub markup.
fn tag_attr(body: &str, name: &str) -> Option<String> {
    // `to_ascii_lowercase` is byte-length preserving, so offsets map one to
    // one onto the original body.
    let lower = body.to_ascii_lowercase();
    tag_attr_exact(&lower, name).map(|value| {
        let start = value.as_ptr() as usize - lower.as_ptr() as usize;
        body[start..start + value.len()].to_string()
    })
}

fn tag_attr_exact<'a>(body: &'a str, name: &str) -> Option<&'a str> {
    let mut search_from = 0;
    while let Some(at) = body[search_from..].find(name) {
        let at = search_from + at;
        let after_name = at + name.len();
        let rest = body[after_name..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            search_from = after_name;
            continue;
        };
        let rest = rest.trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            search_from = after_name;
            continue;
        }
        let content = &rest[quote.len_utf8()..];
        let end = content.find(quote)?;
        return Some(&content[..end]);
    }
    None
}

/// Find the matching `</name>` for a tag opened at `rest`, honoring nesting of
/// the same tag. Returns (inner, after-close) or the whole rest when unclosed.
/// The scan steps byte by byte but only ever *slices* at ASCII `<`, which is
/// always a char boundary even in bodies with multi-byte characters.
fn take_until_close<'a>(rest: &'a str, name: &str) -> (&'a str, &'a str) {
    let open = format!("<{name}");
    let close = format!("</{name}");
    let lower = rest.to_ascii_lowercase();
    let mut depth = 1usize;
    let mut at = 0;
    while at < lower.len() {
        if lower.as_bytes()[at] != b'<' {
            at += 1;
            continue;
        }
        let tail = &lower[at..];
        if tail.starts_with(&close) {
            depth -= 1;
            if depth == 0 {
                let close_end = tail.find('>').map(|e| at + e + 1).unwrap_or(lower.len());
                return (&rest[..at], &rest[close_end..]);
            }
            at += close.len();
        } else if tail.starts_with(&open) {
            depth += 1;
            at += open.len();
        } else {
            at += 1;
        }
    }
    (rest, "")
}

/// The recursive normalizer. Paired inline containers (a/code/em/strong and
/// h1–h4/summary) wrap their inner, already-normalized content; block tags
/// contribute newlines; unknown tags are dropped and their text kept.
fn normalize_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while !rest.is_empty() {
        let Some(open) = rest
            .match_indices('<')
            .map(|(index, _)| index)
            .find(|&index| looks_like_tag_at(rest, index))
        else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..open]);
        rest = &rest[open..];
        let tag_start = rest;
        let Some((name, closing, body, after)) = take_tag(rest) else {
            // A lone '<' with no '>' is prose, not markup.
            out.push('<');
            rest = &rest[1..];
            continue;
        };
        rest = after;
        if closing {
            if matches!(
                name.as_str(),
                "p" | "ul" | "ol" | "blockquote" | "div" | "section"
            ) {
                out.push('\n');
            } else if !matches!(name.as_str(), "li" | "details") {
                out.push_str(&tag_start[..tag_start.len() - after.len()]);
            }
            continue;
        }
        match name.as_str() {
            "a" => {
                let href = tag_attr(body, "href").unwrap_or_default();
                let (inner, after) = take_until_close(rest, "a");
                let inner = normalize_html(inner);
                if href.is_empty() {
                    out.push_str(&inner);
                } else {
                    out.push_str(&format!("[{}]({})", inner.trim(), href));
                }
                rest = after;
            }
            "code" | "em" | "i" | "strong" | "b" => {
                let (inner, after) = take_until_close(rest, &name);
                let inner = normalize_html(inner);
                let marks = if name == "code" {
                    "`"
                } else if matches!(name.as_str(), "em" | "i") {
                    "*"
                } else {
                    "**"
                };
                out.push_str(marks);
                out.push_str(inner.trim());
                out.push_str(marks);
                rest = after;
            }
            "h1" | "h2" | "h3" | "h4" | "summary" => {
                let (inner, after) = take_until_close(rest, &name);
                let inner = normalize_html(inner);
                let hashes = "#".repeat(name.trim_start_matches('h').parse().unwrap_or(2).max(2));
                if name == "summary" {
                    out.push_str(&format!("\n\n**{}**\n\n", inner.trim()));
                } else {
                    out.push_str(&format!("\n\n{hashes} {}\n\n", inner.trim()));
                }
                rest = after;
            }
            "img" => {
                let src = tag_attr(body, "src").unwrap_or_default();
                let alt = tag_attr(body, "alt").unwrap_or_default();
                if !src.is_empty() {
                    out.push_str(&format!("\n\n![{}]({})\n\n", alt, src));
                }
            }
            "br" => out.push('\n'),
            "hr" => out.push_str("\n\n---\n\n"),
            "li" => out.push_str("\n- "),
            "p" | "ul" | "ol" | "blockquote" | "details" | "div" | "section" => {
                out.push('\n');
            }
            _ => out.push_str(&tag_start[..tag_start.len() - after.len()]),
        }
    }
    out
}

/// Decode the handful of entities GitHub actually emits in bodies.
fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
}

/// Normalization can introduce blank runs and dangling list markers (a `<li>`
/// whose text sits inside a nested `<p>`). Collapse 3+ newlines to 2 and
/// reattach a lone `-` line to the content that follows it.
fn collapse_blank_lines(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = false;
    let mut dangling_item = false;
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            blank = !out.is_empty();
            continue;
        }
        if trimmed == "-" {
            dangling_item = true;
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if blank {
                out.push('\n');
            }
        }
        if dangling_item {
            out.push_str("- ");
            dangling_item = false;
        }
        blank = false;
        out.push_str(line);
    }
    out
}

/// Normalize every Markdown-carrying field of a fetched issue.
fn normalize_issue(issue: &mut GhIssue) {
    issue.body = markdown_body(&issue.body);
    for comment in &mut issue.comments {
        comment.body = markdown_body(&comment.body);
    }
}

/// Normalize every Markdown-carrying field of a fetched pull request.
fn normalize_pull(pull: &mut GhPull) {
    pull.body = markdown_body(&pull.body);
    for comment in &mut pull.comments {
        comment.body = markdown_body(&comment.body);
    }
    for review in &mut pull.reviews {
        review.body = markdown_body(&review.body);
    }
}

/// A compact "3d ago"-style label for an RFC3339 timestamp. Unknown formats
/// fall back to the raw date so nothing is hidden.
pub fn relative_time(iso: &str) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let seconds = (chrono::Utc::now() - then.with_timezone(&chrono::Utc)).num_seconds();
    if seconds < 60 {
        return tr!("git_panel.time_now");
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return tr!("git_panel.time_minutes", count = minutes);
    }
    let hours = minutes / 60;
    if hours < 24 {
        return tr!("git_panel.time_hours", count = hours);
    }
    let days = hours / 24;
    if days < 30 {
        return tr!("git_panel.time_days", count = days);
    }
    let months = days / 30;
    if months < 12 {
        return tr!("git_panel.time_months", count = months);
    }
    tr!("git_panel.time_years", count = months / 12)
}

// ── Pull requests ───────────────────────────────────────────────────────

/// The normalized bucket for one CI check. `gh pr checks` provides it
/// directly; the `statusCheckRollup` on a PR has to be inferred from the
/// CheckRun conclusion or the StatusContext state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckBucket {
    Passed,
    Failed,
    Pending,
    Skipped,
    Unknown,
}

/// One CI check on a pull request. The rollup mixes CheckRuns (`name`,
/// `conclusion`, `detailsUrl`) and StatusContexts (`context`, `state`,
/// `targetUrl`), so both shapes are accepted.
#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct GhCheck {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub context: String,
    /// StatusContext result (`SUCCESS`, `PENDING`, …).
    #[serde(default)]
    pub state: String,
    /// CheckRun execution state (`QUEUED`, `IN_PROGRESS`, `COMPLETED`, …).
    /// This is distinct from StatusContext's `state` in `gh --json` output.
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub conclusion: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "detailsUrl", default)]
    pub details_url: String,
    #[serde(rename = "targetUrl", default)]
    pub target_url: String,
    #[serde(default)]
    pub link: String,
}

impl GhCheck {
    pub fn bucket_kind(&self) -> CheckBucket {
        if !self.bucket.is_empty() {
            return match self.bucket.as_str() {
                "pass" => CheckBucket::Passed,
                "fail" => CheckBucket::Failed,
                "pending" => CheckBucket::Pending,
                "skipping" | "cancel" => CheckBucket::Skipped,
                _ => CheckBucket::Unknown,
            };
        }
        let raw = if !self.conclusion.is_empty() {
            self.conclusion.as_str()
        } else if !self.state.is_empty() {
            self.state.as_str()
        } else {
            self.status.as_str()
        };
        match raw.to_ascii_uppercase().as_str() {
            "SUCCESS" => CheckBucket::Passed,
            "FAILURE" | "ERROR" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED" => {
                CheckBucket::Failed
            }
            "PENDING" | "QUEUED" | "IN_PROGRESS" | "WAITING" | "REQUESTED" => CheckBucket::Pending,
            "SKIPPED" | "NEUTRAL" | "STALE" => CheckBucket::Skipped,
            _ => CheckBucket::Unknown,
        }
    }

    pub fn label(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else {
            &self.context
        }
    }

    pub fn link(&self) -> &str {
        if !self.details_url.is_empty() {
            &self.details_url
        } else if !self.target_url.is_empty() {
            &self.target_url
        } else {
            &self.link
        }
    }
}

/// One review on a pull request.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhReview {
    #[serde(default)]
    pub author: Option<GhUser>,
    #[serde(default)]
    pub body: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`, or `DISMISSED`.
    #[serde(default)]
    pub state: String,
    #[serde(rename = "submittedAt", default)]
    pub submitted_at: String,
    #[serde(default)]
    pub url: String,
}

/// One commit on a pull request.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhPullCommit {
    #[serde(rename = "messageHeadline", default)]
    pub headline: String,
    #[serde(default)]
    pub oid: String,
    #[serde(default)]
    pub authors: Vec<GhUser>,
}

/// One changed file on a pull request.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhPullFile {
    pub path: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(rename = "changeType", default)]
    pub change_type: String,
}

/// A pull request as returned by `gh pr list` / `gh pr view`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct GhPull {
    pub number: u64,
    pub title: String,
    /// `OPEN`, `CLOSED`, or `MERGED`.
    pub state: String,
    #[serde(rename = "isDraft", default)]
    pub is_draft: bool,
    #[serde(default)]
    pub body: String,
    pub author: GhUser,
    #[serde(default)]
    pub labels: Vec<GhLabel>,
    #[serde(rename = "reviewDecision", default)]
    pub review_decision: Option<String>,
    #[serde(default)]
    pub mergeable: Option<String>,
    #[serde(rename = "mergeStateStatus", default)]
    pub merge_state_status: Option<String>,
    #[serde(rename = "headRefName", default)]
    pub head_ref: String,
    #[serde(rename = "baseRefName", default)]
    pub base_ref: String,
    #[serde(
        rename = "statusCheckRollup",
        default,
        deserialize_with = "deserialize_null_default"
    )]
    pub checks: Vec<GhCheck>,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(rename = "changedFiles", default)]
    pub changed_files: u64,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(rename = "updatedAt", default)]
    pub updated_at: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub comments: Vec<GhComment>,
    #[serde(default)]
    pub reviews: Vec<GhReview>,
    #[serde(default)]
    pub commits: Vec<GhPullCommit>,
    #[serde(default)]
    pub files: Vec<GhPullFile>,
}

impl GhPull {
    pub fn is_open(&self) -> bool {
        self.state.eq_ignore_ascii_case("open")
    }

    pub fn is_merged(&self) -> bool {
        self.state.eq_ignore_ascii_case("merged")
    }

    /// The overall CI state, or `None` when the PR has no checks.
    pub fn checks_summary(&self) -> Option<CheckBucket> {
        if self.checks.is_empty() {
            return None;
        }
        let mut pending = false;
        let mut passed = false;
        for check in &self.checks {
            match check.bucket_kind() {
                CheckBucket::Failed => return Some(CheckBucket::Failed),
                CheckBucket::Pending => pending = true,
                CheckBucket::Passed => passed = true,
                _ => {}
            }
        }
        if pending {
            Some(CheckBucket::Pending)
        } else if passed {
            Some(CheckBucket::Passed)
        } else {
            Some(CheckBucket::Skipped)
        }
    }
}

/// The pull request list's state filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrState {
    #[default]
    Open,
    Closed,
    Merged,
    All,
}

impl PrState {
    fn flag(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
            Self::All => "all",
        }
    }
}

/// Filters for the pull request list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrFilter {
    pub state: PrState,
    pub base: Option<String>,
    pub author: Option<String>,
    pub search: Option<String>,
}

impl PrFilter {
    pub fn is_active(&self) -> bool {
        self.state != PrState::Open
            || self.base.as_deref().is_some_and(|v| !v.is_empty())
            || self.author.as_deref().is_some_and(|v| !v.is_empty())
            || self.search.as_deref().is_some_and(|v| !v.is_empty())
    }
}

/// How a PR merge combines the branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}

impl MergeMethod {
    fn flag(self) -> &'static str {
        match self {
            Self::Merge => "--merge",
            Self::Squash => "--squash",
            Self::Rebase => "--rebase",
        }
    }
}

/// The review action `gh pr review` should submit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewAction {
    Approve,
    RequestChanges,
}

impl ReviewAction {
    fn flag(self) -> &'static str {
        match self {
            Self::Approve => "--approve",
            Self::RequestChanges => "--request-changes",
        }
    }
}

const PR_LIST_FIELDS: &str = "number,title,state,isDraft,author,labels,reviewDecision,mergeable,mergeStateStatus,headRefName,baseRefName,statusCheckRollup,additions,deletions,changedFiles,createdAt,updatedAt,url";
const PR_VIEW_FIELDS: &str = "number,title,state,isDraft,body,author,labels,reviewDecision,mergeable,mergeStateStatus,headRefName,baseRefName,statusCheckRollup,additions,deletions,changedFiles,createdAt,updatedAt,url,comments,reviews,commits,files";

/// The repository's pull requests, newest first, under `filter`.
pub fn list_pulls(cwd: &Path, filter: &PrFilter) -> Result<Vec<GhPull>, String> {
    let mut args = vec![
        "pr",
        "list",
        "--json",
        PR_LIST_FIELDS,
        "--limit",
        "50",
        "--state",
        filter.state.flag(),
    ];
    for (flag, value) in [
        ("--base", filter.base.as_deref()),
        ("--author", filter.author.as_deref()),
        ("--search", filter.search.as_deref()),
    ] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            args.push(flag);
            args.push(value);
        }
    }
    parse_json(&run(cwd, &args)?)
}

/// GitHub serializes an absent rollup as either an empty list or `null`.
/// Neither should invalidate the otherwise complete pull-request summary.
fn deserialize_null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}

/// One pull request with its description, comments, reviews, commits, and files.
pub fn view_pull(cwd: &Path, number: u64) -> Result<GhPull, String> {
    let number = number.to_string();
    let mut pull: GhPull = parse_json(&run(
        cwd,
        &["pr", "view", &number, "--json", PR_VIEW_FIELDS],
    )?)?;
    normalize_pull(&mut pull);
    Ok(pull)
}

/// Create a pull request from the current branch, returning `gh`'s printed URL.
pub fn create_pull(
    cwd: &Path,
    title: &str,
    body: &str,
    base: &str,
    draft: bool,
    labels: &[String],
) -> Result<String, String> {
    let mut args: Vec<String> = vec![
        "pr".into(),
        "create".into(),
        "--title".into(),
        title.into(),
        "--body".into(),
        body.into(),
    ];
    if !base.trim().is_empty() {
        args.push("--base".into());
        args.push(base.trim().to_string());
    }
    if draft {
        args.push("--draft".into());
    }
    for label in labels {
        args.push("--label".into());
        args.push(label.clone());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(cwd, &refs)
}

/// Add a comment to a pull request.
pub fn comment_pull(cwd: &Path, number: u64, body: &str) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["pr", "comment", &number, "--body", body])?;
    Ok(tr!("git_panel.gh_comment_added"))
}

/// Submit a review on a pull request.
pub fn review_pull(
    cwd: &Path,
    number: u64,
    action: ReviewAction,
    body: Option<&str>,
) -> Result<String, String> {
    let number = number.to_string();
    let mut args = vec!["pr", "review", &number, action.flag()];
    if let Some(body) = body.filter(|body| !body.trim().is_empty()) {
        args.push("--body");
        args.push(body);
    }
    run(cwd, &args)?;
    Ok(tr!("git_panel.gh_review_submitted"))
}

/// Merge a pull request. `delete_branch` removes the head branch afterwards.
pub fn merge_pull(
    cwd: &Path,
    number: u64,
    method: MergeMethod,
    delete_branch: bool,
) -> Result<String, String> {
    let number = number.to_string();
    let mut args = vec!["pr", "merge", &number, method.flag()];
    if delete_branch {
        args.push("--delete-branch");
    }
    run(cwd, &args)?;
    Ok(tr!("git_panel.gh_pr_merged"))
}

/// Close a pull request without merging.
pub fn close_pull(cwd: &Path, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["pr", "close", &number])?;
    Ok(tr!("git_panel.gh_pr_closed"))
}

/// Reopen a closed pull request.
pub fn reopen_pull(cwd: &Path, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["pr", "reopen", &number])?;
    Ok(tr!("git_panel.gh_pr_reopened"))
}

/// Check the PR's head branch out locally. The caller must refuse this when
/// the worktree is dirty or mid-operation.
pub fn checkout_pull(cwd: &Path, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["pr", "checkout", &number])?;
    Ok(tr!("git_panel.gh_pr_checked_out"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "contacts GitHub"]
    fn live_latest_cli_release_can_be_checked_anonymously() {
        assert!(latest_cli_version().is_some());
    }

    #[test]
    fn latest_release_location_is_strictly_validated() {
        assert_eq!(
            parse_latest_release_location("https://github.com/cli/cli/releases/tag/v2.103.3"),
            Some((2, 103, 3))
        );
        for location in [
            "https://example.com/cli/cli/releases/tag/v2.103.3",
            "https://github.com/other/cli/releases/tag/v2.103.3",
            "https://github.com/cli/cli/releases/tag/v2.103.3-rc1",
            "https://github.com/cli/cli/releases/tag/v2.103.3?token=secret",
            "https://github.com/cli/cli/releases/tag/v2.bad.3",
        ] {
            assert_eq!(parse_latest_release_location(location), None);
        }
    }

    #[test]
    fn parses_versions_from_gh_output() {
        assert_eq!(
            parse_version("gh version 2.40.1 (2024-01-01)"),
            Some((2, 40, 1))
        );
        assert_eq!(parse_version("gh version 2.40.1"), Some((2, 40, 1)));
        assert_eq!(parse_version("v2.1.0"), Some((2, 1, 0)));
        assert_eq!(parse_version("no version here"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn version_floor_matches_the_json_surface_we_need() {
        assert_eq!(MIN_GH_VERSION, (2, 20, 0));
        assert!(parse_version("gh version 2.40.1").unwrap() >= MIN_GH_VERSION);
        assert!(parse_version("gh version 2.10.0").unwrap() < MIN_GH_VERSION);
    }

    #[test]
    fn parses_issue_json() {
        let raw = r#"[
          {"assignees":[],"author":{"id":"1","is_bot":false,"login":"octocat","name":"The Octocat"},
           "comments":[{"author":{"id":"1","is_bot":false,"login":"octocat","name":null},"body":"hi","createdAt":"2026-01-01T00:00:00Z","url":"u"}],
           "createdAt":"2026-01-01T00:00:00Z","labels":[{"id":"L","name":"bug","description":"d","color":"d73a4a"}],
           "number":7,"state":"OPEN","title":"Bug","body":"body","updatedAt":"2026-01-02T00:00:00Z","url":"https://example/7"}
        ]"#;
        let issues: Vec<GhIssue> = serde_json::from_str(raw).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].number, 7);
        assert!(issues[0].is_open());
        assert_eq!(issues[0].labels[0].name, "bug");
        assert_eq!(issues[0].comments.len(), 1);
        assert_eq!(issues[0].author.login, "octocat");
    }

    #[test]
    fn parses_labels_json() {
        let raw = r#"[{"color":"f143ab","name":"accessibility"},{"color":"d73a4a","name":"bug"}]"#;
        let labels: Vec<GhLabel> = serde_json::from_str(raw).unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].name, "accessibility");
    }

    #[test]
    fn relative_time_handles_unknown_formats() {
        assert_eq!(relative_time("not a date"), "not a date");
        let now = chrono::Utc::now().to_rfc3339();
        assert!(!relative_time(&now).is_empty());
    }

    #[test]
    fn parses_pull_json_and_check_rollup() {
        let raw = r#"[{
          "number":17,"title":"Feature","state":"OPEN","isDraft":true,
          "author":{"id":"1","login":"dimixar","name":"D"},
          "labels":[],"reviewDecision":"REVIEW_REQUIRED","mergeable":"MERGEABLE","mergeStateStatus":"BLOCKED",
          "headRefName":"feat/x","baseRefName":"main",
          "statusCheckRollup":[{"__typename":"CheckRun","name":"build","conclusion":"SUCCESS"},{"__typename":"StatusContext","context":"lint","state":"FAILURE"}],
          "additions":10,"deletions":2,"changedFiles":3,"updatedAt":"2026-01-01T00:00:00Z","url":"u"
        }]"#;
        let pulls: Vec<GhPull> = serde_json::from_str(raw).unwrap();
        assert_eq!(pulls.len(), 1);
        assert!(pulls[0].is_open());
        assert!(!pulls[0].is_merged());
        assert!(pulls[0].is_draft);
        assert_eq!(pulls[0].head_ref, "feat/x");
        assert_eq!(pulls[0].review_decision.as_deref(), Some("REVIEW_REQUIRED"));
        assert_eq!(pulls[0].checks_summary(), Some(CheckBucket::Failed));
    }

    #[test]
    fn check_buckets_prefer_the_normalized_field() {
        let check: GhCheck =
            serde_json::from_str(r#"{"name":"build","bucket":"pass","state":"SUCCESS"}"#).unwrap();
        assert_eq!(check.bucket_kind(), CheckBucket::Passed);

        let status: GhCheck =
            serde_json::from_str(r#"{"context":"lint","state":"PENDING"}"#).unwrap();
        assert_eq!(status.label(), "lint");
        assert_eq!(status.bucket_kind(), CheckBucket::Pending);

        let failure: GhCheck =
            serde_json::from_str(r#"{"name":"test","conclusion":"TIMED_OUT"}"#).unwrap();
        assert_eq!(failure.bucket_kind(), CheckBucket::Failed);
    }

    #[test]
    fn running_check_run_status_makes_the_rollup_pending() {
        let raw = r#"[{
          "number":17,"title":"Feature","state":"OPEN","author":{"login":"octocat"},
          "statusCheckRollup":[
            {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"SUCCESS"},
            {"__typename":"CheckRun","name":"build","status":"IN_PROGRESS","conclusion":""}
          ]
        }]"#;
        let pulls: Vec<GhPull> = serde_json::from_str(raw).unwrap();
        assert_eq!(pulls[0].checks[1].bucket_kind(), CheckBucket::Pending);
        assert_eq!(pulls[0].checks_summary(), Some(CheckBucket::Pending));
    }

    #[test]
    fn a_null_check_rollup_does_not_invalidate_the_pr_summary() {
        let raw = r#"{
          "number":17,"title":"Feature","state":"OPEN","author":{"login":"octocat"},
          "additions":10,"deletions":2,"changedFiles":3,"statusCheckRollup":null
        }"#;
        let pull: GhPull = serde_json::from_str(raw).unwrap();
        assert!(pull.checks.is_empty());
        assert_eq!(pull.additions, 10);
        assert_eq!(pull.deletions, 2);
        assert_eq!(pull.changed_files, 3);
    }

    #[test]
    fn reviews_and_files_parse() {
        let review: GhReview = serde_json::from_str(
            r#"{"author":{"login":"octocat"},"body":"lgtm","state":"APPROVED","submittedAt":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(review.state, "APPROVED");
        let file: GhPullFile = serde_json::from_str(
            r#"{"path":"src/lib.rs","additions":3,"deletions":1,"changeType":"MODIFIED"}"#,
        )
        .unwrap();
        assert_eq!(file.path, "src/lib.rs");
        assert_eq!(file.change_type, "MODIFIED");
    }

    #[test]
    fn dependabot_html_becomes_markdown() {
        let raw = "Updates `cn` from 0.3.3 to 0.5.2\n\
<details>\n<summary>Release notes</summary>\n\
<p><em>Sourced from <a href=\"https://github.com/shadcn-ui/cn/releases\">cn's releases</a>.</em></p>\n\
<blockquote>\n<h2>cn@0.5.2</h2>\n<h3>Patch Changes</h3>\n<ul>\n<li>\n\
<p><a href=\"https://redirect.github.com/shadcn-ui/cn/pull/162\">#162</a> \
<a href=\"https://github.com/shadcn-ui/cn/commit/b8c5a1b\"><code>b8c5a1b</code></a> Thanks!</p>\n\
</li>\n</ul>\n</blockquote>\n</details>\n<br />\n\
<img alt=\"Image\" src=\"https://example.com/shot.png\" />";
        let md = markdown_body(raw);
        assert!(!md.contains('<'), "no raw HTML remains:\n{md}");
        assert!(
            md.contains("**Release notes**"),
            "summary becomes bold: {md}"
        );
        assert!(md.contains("## cn@0.5.2"), "h2 becomes a heading: {md}");
        assert!(md.contains("### Patch Changes"), "h3: {md}");
        assert!(
            md.contains("[cn's releases](https://github.com/shadcn-ui/cn/releases)"),
            "link: {md}"
        );
        assert!(
            md.contains("[#162](https://redirect.github.com/shadcn-ui/cn/pull/162)"),
            "pr link: {md}"
        );
        assert!(
            md.contains("[`b8c5a1b`](https://github.com/shadcn-ui/cn/commit/b8c5a1b)"),
            "commit link keeps inline code: {md}"
        );
        assert!(md.contains("- "), "list item: {md}");
        assert!(
            md.contains("![Image](https://example.com/shot.png)"),
            "img: {md}"
        );
        assert!(!md.contains("\n\n\n"), "blank runs collapse: {md}");
    }

    #[test]
    fn html_normalization_preserves_markdown_code_and_indentation() {
        let raw = "<details><summary>Example</summary></details>\n\n    indented code\n\n```rust\nfn f<T>() {\n    println!(\"&lt;T&gt;\");\n}\n```\n\nInline `Vec<T>` and Vec<T>.";
        let result = markdown_body(raw);
        assert!(result.contains("    indented code"), "{result}");
        assert!(
            result.contains("```rust\nfn f<T>() {\n    println!(\"&lt;T&gt;\");\n}\n```"),
            "{result}"
        );
        assert!(result.contains("Inline `Vec<T>` and Vec<T>."), "{result}");
        assert!(result.contains("**Example**"), "{result}");
    }

    #[test]
    fn plain_markdown_and_prose_angles_pass_through() {
        let plain = "Fix the bug.\n\n- step one\n- step two";
        assert_eq!(markdown_body(plain), plain);
        // A `<` that does not open a tag is prose, not markup.
        let prose = "assert!(a < b && c > d)";
        assert_eq!(markdown_body(prose), prose);
    }
}
