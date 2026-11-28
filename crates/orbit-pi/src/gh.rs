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
/// Whose issues or pull requests a list shows: everyone's, or the ones that
/// need the signed-in user (`@me` in `gh`'s terms).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mine {
    #[default]
    Everyone,
    /// Pull requests waiting on the user's review.
    ReviewRequested,
    Assigned,
    Created,
    /// Issues that mention the user.
    Mentioned,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IssueFilter {
    pub state: IssueState,
    pub label: Option<String>,
    pub assignee: Option<String>,
    pub search: Option<String>,
    pub mine: Mine,
}

impl IssueFilter {
    pub fn is_active(&self) -> bool {
        self.mine != Mine::Everyone
            || self.state != IssueState::Open
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
    let args = issue_list_args(filter);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    parse_json(&run(cwd, &refs)?)
}

/// `gh issue list` argv for `filter`. "For me" choices replace the matching
/// free-form filter (assigned to me wins over a typed assignee).
pub fn issue_list_args(filter: &IssueFilter) -> Vec<String> {
    let mut args: Vec<String> = [
        "issue",
        "list",
        "--json",
        ISSUE_LIST_FIELDS,
        "--limit",
        "50",
        "--state",
        filter.state.flag(),
    ]
    .map(String::from)
    .to_vec();
    let assignee = match filter.mine {
        Mine::Assigned => Some("@me"),
        _ => filter.assignee.as_deref(),
    };
    let mine_flag = match filter.mine {
        Mine::Created => Some(("--author", "@me")),
        Mine::Mentioned => Some(("--mention", "@me")),
        _ => None,
    };
    for (flag, value) in [
        ("--label", filter.label.as_deref()),
        ("--assignee", assignee),
        ("--search", filter.search.as_deref()),
    ]
    .into_iter()
    .chain(mine_flag.map(|(flag, value)| (flag, Some(value))))
    {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            args.push(flag.into());
            args.push(value.into());
        }
    }
    args
}

/// The repository's `owner/name`, as `gh` resolves it for `cwd`. The Git
/// button keys what the user has seen by it, so every clone and worktree of
/// one repository shares that state.
pub fn repo_name(cwd: &Path) -> Result<String, String> {
    let name = run(
        cwd,
        &["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"],
    )?;
    let name = name.trim();
    if name.is_empty() {
        Err(tr!("git_activity.no_repository"))
    } else {
        Ok(name.to_owned())
    }
}

/// When each open issue (or pull request) was opened, and by whom — the
/// newest 50, which is all the Git button's "new" count reads. Asks for two
/// fields only, so the background check stays one small request per list.
pub fn open_stamps(
    cwd: &Path,
    kind: crate::git_activity::Kind,
) -> Result<Vec<crate::git_activity::Stamp>, String> {
    #[derive(Deserialize)]
    struct Row {
        #[serde(rename = "createdAt", default)]
        created_at: String,
        #[serde(default)]
        author: Option<GhUser>,
    }
    let noun = match kind {
        crate::git_activity::Kind::Issues => "issue",
        crate::git_activity::Kind::Pulls => "pr",
    };
    let rows: Vec<Row> = parse_json(&run(
        cwd,
        &[
            noun, "list", "--state", "open", "--limit", "50", "--json", "createdAt,author",
        ],
    )?)?;
    Ok(rows
        .into_iter()
        .map(|row| crate::git_activity::Stamp {
            created_at: row.created_at,
            author: row.author.map(|author| author.login),
        })
        .collect())
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
    /// Present while auto-merge is enabled; only the detail view asks for it.
    #[serde(rename = "autoMergeRequest", default)]
    pub auto_merge: Option<GhAutoMerge>,
    /// The head commit: line comments are anchored to it.
    #[serde(rename = "headRefOid", default)]
    pub head_oid: String,
    #[serde(default, deserialize_with = "deserialize_null_default")]
    pub assignees: Vec<GhUser>,
    #[serde(
        rename = "reviewRequests",
        default,
        deserialize_with = "deserialize_null_default"
    )]
    pub review_requests: Vec<GhReviewRequest>,
}

/// A requested reviewer: a user (`login`) or a team (`name` / `slug`).
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct GhReviewRequest {
    #[serde(default)]
    pub login: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
}

impl GhReviewRequest {
    pub fn label(&self) -> String {
        self.login
            .clone()
            .or_else(|| self.slug.clone())
            .or_else(|| self.name.clone())
            .unwrap_or_default()
    }
}

/// An enabled auto-merge request on a pull request.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct GhAutoMerge {
    /// `MERGE`, `SQUASH` or `REBASE`.
    #[serde(rename = "mergeMethod", default)]
    pub merge_method: String,
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
        checks_rollup(&self.checks)
    }
}

/// The overall CI state of a set of checks: any failure wins, then anything
/// still running, then a pass; `None` when there are no checks at all.
pub fn checks_rollup(checks: &[GhCheck]) -> Option<CheckBucket> {
    if checks.is_empty() {
        return None;
    }
    let mut pending = false;
    let mut passed = false;
    for check in checks {
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
    pub mine: Mine,
}

impl PrFilter {
    pub fn is_active(&self) -> bool {
        self.mine != Mine::Everyone
            || self.state != PrState::Open
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
    pub(crate) fn flag(self) -> &'static str {
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
const PR_VIEW_FIELDS: &str = "headRefOid,assignees,reviewRequests,autoMergeRequest,number,title,state,isDraft,body,author,labels,reviewDecision,mergeable,mergeStateStatus,headRefName,baseRefName,statusCheckRollup,additions,deletions,changedFiles,createdAt,updatedAt,url,comments,reviews,commits,files";

/// The repository's pull requests, newest first, under `filter`.
pub fn list_pulls(cwd: &Path, filter: &PrFilter) -> Result<Vec<GhPull>, String> {
    let args = pull_list_args(filter);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    parse_json(&run(cwd, &refs)?)
}

/// `gh pr list` argv for `filter`. "Review requested" and "mentions" are
/// search qualifiers, joined with any typed search.
pub fn pull_list_args(filter: &PrFilter) -> Vec<String> {
    let mut args: Vec<String> = [
        "pr",
        "list",
        "--json",
        PR_LIST_FIELDS,
        "--limit",
        "50",
        "--state",
        filter.state.flag(),
    ]
    .map(String::from)
    .to_vec();
    let author = match filter.mine {
        Mine::Created => Some("@me".to_string()),
        _ => filter.author.clone(),
    };
    let assignee = (filter.mine == Mine::Assigned).then(|| "@me".to_string());
    let qualifier = match filter.mine {
        Mine::ReviewRequested => Some("review-requested:@me"),
        Mine::Mentioned => Some("mentions:@me"),
        _ => None,
    };
    let search = match (qualifier, filter.search.as_deref().map(str::trim)) {
        (Some(q), Some(text)) if !text.is_empty() => Some(format!("{q} {text}")),
        (Some(q), _) => Some(q.to_string()),
        (None, text) => text.map(str::to_string),
    };
    for (flag, value) in [
        ("--base", filter.base.clone()),
        ("--author", author),
        ("--assignee", assignee),
        ("--search", search),
    ] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            args.push(flag.into());
            args.push(value);
        }
    }
    args
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

// ── Publish a local repository ──────────────────────────────────────────

/// Whether `name` is a repository name GitHub accepts: 1–100 of
/// `[A-Za-z0-9._-]`, and not `.` or `..`.
pub fn valid_repo_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// A repository name suggested from a folder name: every character GitHub
/// rejects becomes `-` (the same thing github.com's own form does).
pub fn suggested_repo_name(folder: &str) -> String {
    let name: String = folder
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let name = name.trim_matches('-').to_string();
    if valid_repo_name(&name) {
        name
    } else {
        String::new()
    }
}

/// `gh repo create` argv for publishing the repository in the working
/// directory as `origin`. `push` sends the current branch and sets its
/// upstream; it must be off while HEAD is unborn (`gh` refuses to push then).
pub fn repo_create_args(name: &str, private: bool, push: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "repo".into(),
        "create".into(),
        name.into(),
        "--source".into(),
        ".".into(),
        "--remote".into(),
        "origin".into(),
        if private { "--private" } else { "--public" }.into(),
    ];
    if push {
        args.push("--push".into());
    }
    args
}

/// Create a GitHub repository under the signed-in account from the local
/// repository at `cwd`, add it as `origin`, and push the current branch when
/// there is a commit to push. Returns `gh`'s output (the new repository URL).
pub fn create_repo(cwd: &Path, name: &str, private: bool, push: bool) -> Result<String, String> {
    let args = repo_create_args(name, private, push);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(cwd, &refs)
}

// ── Repository summary ──────────────────────────────────────────────────

/// A `{ "name": … }` object, as `gh` reports a language or a branch ref.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GhNamed {
    #[serde(default)]
    pub name: String,
}

/// The facts the Git page shows about the repository behind `origin`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GhRepo {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "stargazerCount", default)]
    pub stars: u64,
    #[serde(rename = "forkCount", default)]
    pub forks: u64,
    /// `PUBLIC`, `PRIVATE` or `INTERNAL`.
    #[serde(default)]
    pub visibility: String,
    #[serde(rename = "primaryLanguage", default)]
    pub language: Option<GhNamed>,
    #[serde(rename = "defaultBranchRef", default)]
    pub default_branch: Option<GhNamed>,
    #[serde(rename = "viewerHasStarred", default)]
    pub starred: bool,
    #[serde(rename = "latestRelease", default)]
    pub latest_release: Option<GhRelease>,
}

/// A release, as much of it as the summary names.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GhRelease {
    #[serde(rename = "tagName", default)]
    pub tag: String,
}

impl GhRepo {
    pub fn is_private(&self) -> bool {
        !self.visibility.eq_ignore_ascii_case("public")
    }

    /// `(owner, name)` from `owner/name`.
    pub fn slug(&self) -> Option<(String, String)> {
        let (owner, name) = self.name_with_owner.split_once('/')?;
        Some((owner.to_string(), name.to_string()))
    }
}

const REPO_VIEW_FIELDS: &str = "latestRelease,nameWithOwner,description,url,stargazerCount,forkCount,visibility,primaryLanguage,defaultBranchRef,viewerHasStarred";

/// The repository the working directory's remote points at.
pub fn repo_info(cwd: &Path) -> Result<GhRepo, String> {
    parse_json(&run(cwd, &["repo", "view", "--json", REPO_VIEW_FIELDS])?)
}

/// A star count the way GitHub abbreviates it: `999`, `1.2k`, `12k`, `1.5m`.
pub fn compact_count(count: u64) -> String {
    // Truncate, never round up: 1,999 stars is "1.9k", not "2k".
    let scaled = |count: u64, unit: u64, suffix: &str| {
        let tenths = count / (unit / 10);
        if tenths < 100 && !tenths.is_multiple_of(10) {
            format!("{}.{}{suffix}", tenths / 10, tenths % 10)
        } else {
            format!("{}{suffix}", count / unit)
        }
    };
    match count {
        0..=999 => count.to_string(),
        1_000..=999_999 => scaled(count, 1_000, "k"),
        _ => scaled(count, 1_000_000, "m"),
    }
}

// ── The signed-in user's repositories (clone picker) ────────────────────

/// One of the user's repositories, for the clone dialog's list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GhRepoEntry {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
    #[serde(default)]
    pub url: String,
    #[serde(rename = "isPrivate", default)]
    pub private: bool,
    #[serde(default)]
    pub description: Option<String>,
}

impl GhRepoEntry {
    /// The repository name without its owner: the clone's folder name.
    pub fn name(&self) -> &str {
        self.name_with_owner
            .rsplit_once('/')
            .map_or(self.name_with_owner.as_str(), |(_, name)| name)
    }
}

/// The signed-in user's own repositories, most recently pushed first.
pub fn list_my_repos(cwd: &Path) -> Result<Vec<GhRepoEntry>, String> {
    parse_json(&run(
        cwd,
        &[
            "repo",
            "list",
            "--limit",
            "100",
            "--json",
            "nameWithOwner,url,isPrivate,description",
        ],
    )?)
}

/// Repositories whose `owner/name` contains `query` (case-insensitive), at
/// most `limit` of them. A query that is already a URL or an scp-style
/// address matches nothing: the user is pasting, not searching.
pub fn filter_repos<'a>(
    repos: &'a [GhRepoEntry],
    query: &str,
    limit: usize,
) -> Vec<&'a GhRepoEntry> {
    let query = query.trim().to_lowercase();
    if query.contains("://") || query.contains('@') {
        return Vec::new();
    }
    repos
        .iter()
        .filter(|repo| query.is_empty() || repo.name_with_owner.to_lowercase().contains(&query))
        .take(limit)
        .collect()
}

// ── The current branch's pull request ───────────────────────────────────

/// The pull request for the checked-out branch, or `None` when the branch
/// has none. Any other `gh` failure is an error.
pub fn pull_for_branch(cwd: &Path) -> Result<Option<GhPull>, String> {
    match run(cwd, &["pr", "view", "--json", PR_LIST_FIELDS]) {
        Ok(out) => {
            let mut pull: GhPull = parse_json(&out)?;
            normalize_pull(&mut pull);
            Ok(Some(pull))
        }
        Err(err) if is_no_pull(&err) => Ok(None),
        Err(err) => Err(err),
    }
}

fn is_no_pull(stderr: &str) -> bool {
    stderr.to_ascii_lowercase().contains("no pull requests found")
}

// ── Pull request quick actions ──────────────────────────────────────────

/// Mark a draft ready for review (`ready`), or turn it back into a draft.
pub fn set_pull_ready(cwd: &Path, number: u64, ready: bool) -> Result<String, String> {
    let number = number.to_string();
    let mut args = vec!["pr", "ready", number.as_str()];
    if !ready {
        args.push("--undo");
    }
    run(cwd, &args)?;
    Ok(if ready {
        tr!("git_panel.pr_marked_ready")
    } else {
        tr!("git_panel.pr_marked_draft")
    })
}

/// Turn on auto-merge with `method` (GitHub merges once requirements pass),
/// or turn it off with `None`.
pub fn set_auto_merge(
    cwd: &Path,
    number: u64,
    method: Option<MergeMethod>,
) -> Result<String, String> {
    let number = number.to_string();
    match method {
        Some(method) => {
            run(cwd, &["pr", "merge", &number, "--auto", method.flag()])?;
            Ok(tr!("git_panel.pr_auto_merge_on"))
        }
        None => {
            run(cwd, &["pr", "merge", &number, "--disable-auto"])?;
            Ok(tr!("git_panel.pr_auto_merge_off"))
        }
    }
}

/// Bring the base branch's new commits into the pull request's branch.
pub fn update_pull_branch(cwd: &Path, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["pr", "update-branch", &number])?;
    Ok(tr!("git_panel.pr_branch_updated"))
}

// ── GitHub Actions: re-run and failed logs ──────────────────────────────

/// `(run id, job id)` from a GitHub Actions check link
/// (`…/actions/runs/<run>/job/<job>`). `None` for checks that are not
/// Actions runs (an external status, for instance).
pub fn actions_ids(url: &str) -> Option<(u64, Option<u64>)> {
    let digits = |text: &str| -> Option<u64> {
        let end = text
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(text.len());
        text[..end].parse().ok()
    };
    let (_, rest) = url.split_once("/actions/runs/")?;
    let run = digits(rest)?;
    let job = rest
        .split_once("/job/")
        .and_then(|(_, job)| digits(job));
    Some((run, job))
}

/// Re-run only the failed jobs of a workflow run.
pub fn rerun_failed(cwd: &Path, run_id: u64) -> Result<(), String> {
    let run_id = run_id.to_string();
    run(cwd, &["run", "rerun", &run_id, "--failed"]).map(|_| ())
}

/// The log of a run's (or one job's) failed steps, cut to its last
/// `max_bytes`: the end of a CI log is where the failure is.
pub fn failed_log(
    cwd: &Path,
    run_id: u64,
    job_id: Option<u64>,
    max_bytes: usize,
) -> Result<String, String> {
    let run_id = run_id.to_string();
    let job_id = job_id.map(|job| job.to_string());
    let mut args = vec!["run", "view", run_id.as_str()];
    if let Some(job) = job_id.as_deref() {
        args.push("--job");
        args.push(job);
    }
    args.push("--log-failed");
    Ok(log_tail(&run(cwd, &args)?, max_bytes))
}

/// The last `max_bytes` of `log`, cut at a line start so no line is split,
/// and on a UTF-8 boundary.
pub fn log_tail(log: &str, max_bytes: usize) -> String {
    if log.len() <= max_bytes {
        return log.to_string();
    }
    let mut start = log.len() - max_bytes;
    while !log.is_char_boundary(start) {
        start += 1;
    }
    let tail = &log[start..];
    match tail.find('\n') {
        Some(newline) if newline + 1 < tail.len() => tail[newline + 1..].to_string(),
        _ => tail.to_string(),
    }
}

// ── Issues and pull requests: people ────────────────────────────────────

/// Create a branch linked to issue `number` on GitHub and check it out.
/// Returns the branch now checked out.
pub fn develop_issue(cwd: &Path, number: u64) -> Result<String, String> {
    let number = number.to_string();
    run(cwd, &["issue", "develop", &number, "--checkout"])?;
    crate::git::current_branch(cwd).ok_or_else(|| tr!("git.no_branch_to_push"))
}

/// Assign the signed-in user to (or remove them from) an issue or a pull
/// request. `kind` is `issue` or `pr`.
pub fn assign_me(cwd: &Path, kind: &str, number: u64, assign: bool) -> Result<String, String> {
    let number = number.to_string();
    let flag = if assign {
        "--add-assignee"
    } else {
        "--remove-assignee"
    };
    run(cwd, &[kind, "edit", &number, flag, "@me"])?;
    Ok(if assign {
        tr!("git_panel.assigned_to_you")
    } else {
        tr!("git_panel.unassigned_you")
    })
}

/// GitHub logins from a typed list: `@alice, bob  carol` → `alice,bob,carol`.
pub fn parse_logins(text: &str) -> Vec<String> {
    text.split(|c: char| c == ',' || c.is_whitespace())
        .map(|login| login.trim().trim_start_matches('@'))
        .filter(|login| !login.is_empty())
        .map(str::to_string)
        .collect()
}

/// Ask `logins` to review a pull request.
pub fn request_reviewers(cwd: &Path, number: u64, logins: &[String]) -> Result<String, String> {
    let number = number.to_string();
    let logins = logins.join(",");
    run(cwd, &["pr", "edit", &number, "--add-reviewer", &logins])?;
    Ok(tr!("git_panel.reviewers_requested"))
}

/// How many open pull requests in this repository wait on the user's review.
pub fn review_requested_count(cwd: &Path) -> Result<usize, String> {
    let out = run(
        cwd,
        &[
            "pr",
            "list",
            "--state",
            "open",
            "--search",
            "review-requested:@me",
            "--limit",
            "100",
            "--json",
            "number",
        ],
    )?;
    let items: Vec<serde_json::Value> = parse_json(&out)?;
    Ok(items.len())
}

// ── Pull request diff and line comments ─────────────────────────────────

/// What a line of a pull request's diff is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrLineKind {
    Hunk,
    Context,
    Added,
    Removed,
}

/// One line of a file's diff, with the line numbers a comment anchors to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDiffLine {
    pub kind: PrLineKind,
    pub old_line: Option<u64>,
    pub new_line: Option<u64>,
    pub text: String,
}

/// One file of a pull request's diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PrDiffFile {
    pub path: String,
    pub lines: Vec<PrDiffLine>,
    /// Lines left out past [`MAX_PR_DIFF_LINES`].
    pub hidden: usize,
}

/// The most lines one file's inline diff shows; the rest is on GitHub.
pub const MAX_PR_DIFF_LINES: usize = 600;

/// The unified diff of a pull request.
pub fn pull_diff(cwd: &Path, number: u64) -> Result<Vec<PrDiffFile>, String> {
    let number = number.to_string();
    Ok(parse_pr_diff(&run(cwd, &["pr", "diff", &number, "--color", "never"])?))
}

/// Split a unified diff into files and number its lines. Binary files and
/// renames without content changes come back with no lines.
pub fn parse_pr_diff(raw: &str) -> Vec<PrDiffFile> {
    let mut files: Vec<PrDiffFile> = Vec::new();
    let (mut old, mut new) = (0u64, 0u64);
    let mut in_hunk = false;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            // `a/<path> b/<path>`: the new path follows the last " b/". Git
            // quotes a side whose path has non-ASCII or special characters —
            // `"b/\346\227\245.rs"` — and that side is decoded.
            let path = rest
                .rsplit_once(" \"b/")
                .and_then(|(_, tail)| unquote_git_path(&format!("\"b/{tail}")))
                .or_else(|| rest.rsplit_once(" b/").map(|(_, path)| format!("b/{path}")))
                .and_then(|path| path.strip_prefix("b/").map(str::to_string))
                .unwrap_or_else(|| rest.to_string());
            files.push(PrDiffFile {
                path,
                ..Default::default()
            });
            in_hunk = false;
            continue;
        }
        let Some(file) = files.last_mut() else { continue };
        if let Some(side) = line.strip_prefix("+++ ") {
            let side = if side.starts_with('"') {
                unquote_git_path(side)
            } else {
                Some(side.to_string())
            };
            // `/dev/null` (a deleted file) keeps the header's path.
            if let Some(path) = side.as_deref().and_then(|side| side.strip_prefix("b/")) {
                file.path = path.to_string();
            }
            continue;
        }
        if line.starts_with("@@") {
            if let Some((o, n)) = crate::review::parse_hunk_starts(line) {
                old = o as u64;
                new = n as u64;
                in_hunk = true;
            }
            push_pr_line(file, PrLineKind::Hunk, None, None, line);
            continue;
        }
        if !in_hunk {
            continue;
        }
        if let Some(text) = line.strip_prefix('+') {
            push_pr_line(file, PrLineKind::Added, None, Some(new), text);
            new += 1;
        } else if let Some(text) = line.strip_prefix('-') {
            push_pr_line(file, PrLineKind::Removed, Some(old), None, text);
            old += 1;
        } else if let Some(text) = line.strip_prefix(' ') {
            push_pr_line(file, PrLineKind::Context, Some(old), Some(new), text);
            old += 1;
            new += 1;
        } else if line.is_empty() {
            push_pr_line(file, PrLineKind::Context, Some(old), Some(new), "");
            old += 1;
            new += 1;
        }
        // `\ No newline at end of file` and other markers are skipped.
    }
    files
}

/// Decode a path Git printed in C-style quotes (`core.quotePath`): the
/// surrounding quotes, `\\`, `\"`, the usual letter escapes, and `\NNN` octal
/// bytes, which together spell the path's UTF-8. `None` when `quoted` is not
/// a complete quoted string.
pub fn unquote_git_path(quoted: &str) -> Option<String> {
    let inner = quoted.strip_prefix('"')?.strip_suffix('"')?;
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.bytes().peekable();
    while let Some(byte) = chars.next() {
        if byte != b'\\' {
            bytes.push(byte);
            continue;
        }
        let escaped = chars.next()?;
        let decoded = match escaped {
            b'a' => 0x07,
            b'b' => 0x08,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 0x0b,
            b'f' => 0x0c,
            b'r' => b'\r',
            b'0'..=b'7' => {
                let mut value = u32::from(escaped - b'0');
                for _ in 0..2 {
                    match chars.peek() {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(digit - b'0');
                            chars.next();
                        }
                        _ => break,
                    }
                }
                u8::try_from(value).ok()?
            }
            other => other,
        };
        bytes.push(decoded);
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn push_pr_line(
    file: &mut PrDiffFile,
    kind: PrLineKind,
    old_line: Option<u64>,
    new_line: Option<u64>,
    text: &str,
) {
    if file.lines.len() >= MAX_PR_DIFF_LINES {
        file.hidden += 1;
        return;
    }
    file.lines.push(PrDiffLine {
        kind,
        old_line,
        new_line,
        text: text.to_string(),
    });
}

/// Where a line comment goes: the new side for added and unchanged lines,
/// the old side for removed ones. `None` for a hunk header.
pub fn comment_anchor(line: &PrDiffLine) -> Option<(u64, &'static str)> {
    match line.kind {
        PrLineKind::Hunk => None,
        PrLineKind::Removed => line.old_line.map(|n| (n, "LEFT")),
        PrLineKind::Added | PrLineKind::Context => line.new_line.map(|n| (n, "RIGHT")),
    }
}

/// Post a review comment on one line of a pull request.
#[allow(clippy::too_many_arguments)]
pub fn add_line_comment(
    owner: &str,
    name: &str,
    number: u64,
    commit: &str,
    path: &str,
    line: u64,
    side: &str,
    body: &str,
) -> Result<String, String> {
    let endpoint = format!("repos/{owner}/{name}/pulls/{number}/comments");
    let fields = [
        format!("body={body}"),
        format!("commit_id={commit}"),
        format!("path={path}"),
        format!("side={side}"),
    ];
    let line = format!("line={line}");
    let mut args = vec!["api", "--method", "POST", endpoint.as_str()];
    for field in &fields {
        args.push("-f");
        args.push(field);
    }
    args.push("-F");
    args.push(&line);
    run(&std::env::temp_dir(), &args)?;
    Ok(tr!("git_panel.line_comment_added"))
}

/// One unresolved review conversation on a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewThread {
    pub path: String,
    pub line: Option<u64>,
    /// `(author, body)` in order.
    pub comments: Vec<(String, String)>,
}

const REVIEW_THREADS_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!,$after:String){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100,after:$after){pageInfo{hasNextPage endCursor} nodes{isResolved path line originalLine comments(first:20){nodes{author{login} body}}}}}}}";

/// How many pages of 100 threads are read before giving up: a guard against
/// a cursor that never ends, far past any real pull request.
const MAX_REVIEW_THREAD_PAGES: usize = 50;

/// The pull request's unresolved review threads, from every page. GitHub
/// hands threads out 100 at a time and resolved ones are filtered here, so
/// stopping at the first page would silently drop unresolved threads past it.
pub fn review_threads(owner: &str, name: &str, number: u64) -> Result<Vec<ReviewThread>, String> {
    let query = format!("query={REVIEW_THREADS_QUERY}");
    let owner = format!("owner={owner}");
    let name = format!("name={name}");
    let number = format!("number={number}");
    let mut threads = Vec::new();
    let mut after: Option<String> = None;
    for _ in 0..MAX_REVIEW_THREAD_PAGES {
        let after_field = after.as_ref().map(|cursor| format!("after={cursor}"));
        let mut args = vec![
            "api", "graphql", "-f", &query, "-f", &owner, "-f", &name, "-F", &number,
        ];
        if let Some(field) = after_field.as_deref() {
            args.push("-f");
            args.push(field);
        }
        let raw = run(&std::env::temp_dir(), &args)?;
        let (page, next) = parse_review_thread_page(&raw)?;
        threads.extend(page);
        match next {
            Some(cursor) => after = Some(cursor),
            None => return Ok(threads),
        }
    }
    Err(tr!("git_panel.review_threads_truncated"))
}

/// One page of `reviewThreads`, read leniently: its unresolved threads, and
/// the cursor of the next page when GitHub says there is one.
pub fn parse_review_thread_page(raw: &str) -> Result<(Vec<ReviewThread>, Option<String>), String> {
    let value: serde_json::Value = parse_json(raw)?;
    let threads = value.pointer("/data/repository/pullRequest/reviewThreads");
    let next = threads
        .filter(|threads| threads.pointer("/pageInfo/hasNextPage").and_then(|v| v.as_bool()) == Some(true))
        .and_then(|threads| threads.pointer("/pageInfo/endCursor"))
        .and_then(|cursor| cursor.as_str())
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_string);
    let nodes = value
        .pointer("/data/repository/pullRequest/reviewThreads/nodes")
        .and_then(|nodes| nodes.as_array())
        .cloned()
        .unwrap_or_default();
    let threads: Vec<ReviewThread> = nodes
        .iter()
        .filter(|node| !node["isResolved"].as_bool().unwrap_or(false))
        .map(|node| ReviewThread {
            path: node["path"].as_str().unwrap_or_default().to_string(),
            line: node["line"].as_u64().or_else(|| node["originalLine"].as_u64()),
            comments: node
                .pointer("/comments/nodes")
                .and_then(|c| c.as_array())
                .map(|comments| {
                    comments
                        .iter()
                        .map(|c| {
                            (
                                c.pointer("/author/login")
                                    .and_then(|l| l.as_str())
                                    .unwrap_or("ghost")
                                    .to_string(),
                                c["body"].as_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    Ok((threads, next))
}

// ── GitHub Actions runs ─────────────────────────────────────────────────

/// One workflow run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GhRun {
    #[serde(rename = "databaseId", default)]
    pub id: u64,
    #[serde(rename = "displayTitle", default)]
    pub title: String,
    #[serde(rename = "workflowName", default)]
    pub workflow: String,
    /// `queued`, `in_progress`, `completed`, …
    #[serde(default)]
    pub status: String,
    /// `success`, `failure`, `cancelled`, … once completed.
    #[serde(default)]
    pub conclusion: String,
    #[serde(rename = "headBranch", default)]
    pub branch: String,
    #[serde(default)]
    pub event: String,
    #[serde(rename = "createdAt", default)]
    pub created_at: String,
    #[serde(default)]
    pub url: String,
}

impl GhRun {
    pub fn is_running(&self) -> bool {
        !self.status.eq_ignore_ascii_case("completed")
    }

    /// The run's state on the checks scale, so it reads like a check.
    pub fn bucket(&self) -> CheckBucket {
        if self.is_running() {
            return CheckBucket::Pending;
        }
        match self.conclusion.to_ascii_lowercase().as_str() {
            "success" => CheckBucket::Passed,
            "failure" | "timed_out" | "startup_failure" => CheckBucket::Failed,
            "cancelled" | "skipped" | "neutral" => CheckBucket::Skipped,
            _ => CheckBucket::Unknown,
        }
    }
}

/// Recent workflow runs, newest first: the branch's, or every branch's.
pub fn list_runs(cwd: &Path, branch: Option<&str>) -> Result<Vec<GhRun>, String> {
    let mut args = vec![
        "run",
        "list",
        "--limit",
        "30",
        "--json",
        "databaseId,displayTitle,workflowName,status,conclusion,headBranch,event,createdAt,url",
    ];
    if let Some(branch) = branch {
        args.push("--branch");
        args.push(branch);
    }
    parse_json(&run(cwd, &args)?)
}

/// Cancel a queued or running workflow run.
pub fn cancel_run(cwd: &Path, run_id: u64) -> Result<String, String> {
    let run_id = run_id.to_string();
    run(cwd, &["run", "cancel", &run_id])?;
    Ok(tr!("git_panel.run_cancelled"))
}

// ── Releases ────────────────────────────────────────────────────────────

/// Whether `tag` can name a release tag: non-empty, no spaces, and none of
/// the characters Git refuses in a ref.
pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && !tag.starts_with('-')
        && !tag.contains("..")
        && !tag.ends_with('.')
        && !tag.ends_with('/')
        && !tag
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || "~^:?*[\\".contains(c))
}

/// Create a release tagged `tag` at the commit HEAD points at, with notes
/// GitHub writes from the merged pull requests. Returns the release URL.
///
/// The target is the pushed commit's SHA, never the branch name: GitHub
/// would resolve a name against its own copy of the branch, so unpushed local
/// commits would be left out and an unpushed branch would fail. A commit no
/// remote branch contains yet is refused with "push first".
pub fn create_release(cwd: &Path, tag: &str, draft: bool) -> Result<String, String> {
    let target = crate::git::pushed_head(cwd)?;
    let target = target.as_str();
    let mut args = vec![
        "release",
        "create",
        tag,
        "--generate-notes",
        "--target",
        target,
    ];
    if draft {
        args.push("--draft");
    }
    run(cwd, &args)
}

// ── Fork and push ───────────────────────────────────────────────────────

/// Fork the repository to the user's account and push the branch there:
/// `origin` becomes the fork, the original stays as `upstream` and becomes
/// `gh`'s default, so a pull request still targets it.
pub fn fork_and_push(cwd: &Path) -> Result<String, String> {
    let upstream = repo_name(cwd)?;
    run(cwd, &["repo", "fork", "--remote"])?;
    run(cwd, &["repo", "set-default", &upstream])?;
    crate::git::push(cwd)?;
    Ok(tr!("git_panel.forked_and_pushed"))
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


// ── Commit → pull requests and checks ─────────────────────────────────────

/// A pull request a commit belongs to, as GitHub associates them (the PR that
/// introduced it, or the one that merged it).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitPull {
    pub number: u64,
    pub title: String,
    /// `OPEN`, `CLOSED` or `MERGED`.
    pub state: String,
    pub is_draft: bool,
    pub url: String,
    pub author: Option<String>,
    /// Who pressed merge on GitHub. Git itself never records who pushed a
    /// commit; this is the closest fact GitHub keeps, so it is labeled as such.
    pub merged_by: Option<String>,
    pub head_ref: String,
    pub base_ref: String,
}

impl CommitPull {
    pub fn is_open(&self) -> bool {
        self.state.eq_ignore_ascii_case("open")
    }

    pub fn is_merged(&self) -> bool {
        self.state.eq_ignore_ascii_case("merged")
    }
}

/// What GitHub knows about one commit: its pull requests and its CI checks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitGithub {
    pub pulls: Vec<CommitPull>,
    pub checks: Vec<GhCheck>,
}

const COMMIT_GITHUB_QUERY: &str = "query($owner:String!,$name:String!,$oid:GitObjectID!){repository(owner:$owner,name:$name){object(oid:$oid){... on Commit{associatedPullRequests(first:5){nodes{number title state isDraft url headRefName baseRefName author{login} mergedBy{login}}} statusCheckRollup{contexts(first:100){nodes{__typename ... on CheckRun{name status conclusion detailsUrl} ... on StatusContext{context state targetUrl description}}}}}}}}";

/// The pull requests and checks for `oid` in `owner/name`, in one GraphQL
/// round trip through `gh api`. A commit GitHub has never seen (local only,
/// not pushed) comes back empty, not as an error.
pub fn commit_github(cwd: &Path, owner: &str, name: &str, oid: &str) -> Result<CommitGithub, String> {
    let query = format!("query={COMMIT_GITHUB_QUERY}");
    let owner = format!("owner={owner}");
    let name = format!("name={name}");
    let oid = format!("oid={oid}");
    let raw = run(
        cwd,
        &["api", "graphql", "-f", &query, "-f", &owner, "-f", &name, "-f", &oid],
    )?;
    parse_commit_github(&raw)
}

/// Parse [`commit_github`]'s response. Every field GitHub may send as `null`
/// (an in-progress run's conclusion, a deleted author) is optional here.
pub fn parse_commit_github(raw: &str) -> Result<CommitGithub, String> {
    #[derive(Deserialize, Default)]
    struct Login {
        login: Option<String>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawPull {
        number: u64,
        title: Option<String>,
        state: Option<String>,
        is_draft: Option<bool>,
        url: Option<String>,
        head_ref_name: Option<String>,
        base_ref_name: Option<String>,
        author: Option<Login>,
        merged_by: Option<Login>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct RawCheck {
        name: Option<String>,
        status: Option<String>,
        conclusion: Option<String>,
        details_url: Option<String>,
        context: Option<String>,
        state: Option<String>,
        target_url: Option<String>,
        description: Option<String>,
    }
    #[derive(Deserialize)]
    struct Nodes<T> {
        #[serde(default = "Vec::new")]
        nodes: Vec<Option<T>>,
    }
    #[derive(Deserialize)]
    struct Rollup {
        contexts: Option<Nodes<RawCheck>>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Commit {
        associated_pull_requests: Option<Nodes<RawPull>>,
        status_check_rollup: Option<Rollup>,
    }
    #[derive(Deserialize)]
    struct Repo {
        object: Option<Commit>,
    }
    #[derive(Deserialize)]
    struct Data {
        repository: Option<Repo>,
    }
    #[derive(Deserialize)]
    struct Envelope {
        data: Option<Data>,
    }

    let envelope: Envelope = parse_json(raw)?;
    let Some(commit) = envelope
        .data
        .and_then(|data| data.repository)
        .and_then(|repo| repo.object)
    else {
        return Ok(CommitGithub::default());
    };
    let login = |who: Option<Login>| who.and_then(|who| who.login).filter(|l| !l.is_empty());
    let pulls = commit
        .associated_pull_requests
        .map(|nodes| nodes.nodes)
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(|pull| CommitPull {
            number: pull.number,
            title: pull.title.unwrap_or_default(),
            state: pull.state.unwrap_or_default(),
            is_draft: pull.is_draft.unwrap_or(false),
            url: pull.url.unwrap_or_default(),
            author: login(pull.author),
            merged_by: login(pull.merged_by),
            head_ref: pull.head_ref_name.unwrap_or_default(),
            base_ref: pull.base_ref_name.unwrap_or_default(),
        })
        .collect();
    let checks = commit
        .status_check_rollup
        .and_then(|rollup| rollup.contexts)
        .map(|nodes| nodes.nodes)
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(|check| GhCheck {
            name: check.name.unwrap_or_default(),
            context: check.context.unwrap_or_default(),
            state: check.state.unwrap_or_default(),
            status: check.status.unwrap_or_default(),
            conclusion: check.conclusion.unwrap_or_default(),
            description: check.description.unwrap_or_default(),
            details_url: check.details_url.unwrap_or_default(),
            target_url: check.target_url.unwrap_or_default(),
            ..GhCheck::default()
        })
        .collect();
    Ok(CommitGithub { pulls, checks })
}

/// `owner/name` of a GitHub repository URL (`https://github.com/o/r`, with
/// or without `.git` or a trailing slash). `None` for any other host or shape,
/// so a fork that points its manifest elsewhere never stars the wrong repo.
pub fn github_repo_slug(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if parsed.host_str()? != "github.com" {
        return None;
    }
    let mut parts = parsed.path_segments()?.filter(|part| !part.is_empty());
    let owner = parts.next()?;
    let name = parts.next()?.trim_end_matches(".git");
    if parts.next().is_some() || owner.is_empty() || name.is_empty() {
        return None;
    }
    Some((owner.to_string(), name.to_string()))
}

/// `(owner, name)` from any github.com URL inside a repository
/// (`…/owner/name/pull/7`, `…/owner/name/issues/3`).
pub fn url_repo_slug(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if parsed.host_str()? != "github.com" {
        return None;
    }
    let mut parts = parsed.path_segments()?.filter(|part| !part.is_empty());
    let owner = parts.next()?.to_string();
    let name = parts.next()?.trim_end_matches(".git").to_string();
    Some((owner, name))
}

/// `gh api` answers a missing resource on stderr with `(HTTP 404)`. For
/// `/user/starred/…` that is GitHub's "not starred", not a failure.
fn is_not_found(stderr: &str) -> bool {
    stderr.contains("HTTP 404")
}

/// Classify the starred probe's outcome: 204 (success) is starred, 404 is not;
/// anything else — gh missing, signed out, a token without the scope — is an
/// error the caller reads as "cannot star from here".
fn starred_from(result: Result<String, String>) -> Result<bool, String> {
    match result {
        Ok(_) => Ok(true),
        Err(error) if is_not_found(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Whether the account `gh` is signed in to has starred `owner/name`.
/// Blocking: run it off the UI thread.
pub fn repo_starred(owner: &str, name: &str) -> Result<bool, String> {
    let path = format!("/user/starred/{owner}/{name}");
    starred_from(run(&std::env::temp_dir(), &["api", path.as_str()]))
}

/// Star (`true`) or unstar `owner/name` as the signed-in `gh` account. Both
/// are idempotent on GitHub's side. Blocking: run it off the UI thread.
pub fn set_repo_starred(owner: &str, name: &str, starred: bool) -> Result<(), String> {
    let path = format!("/user/starred/{owner}/{name}");
    let method = if starred { "PUT" } else { "DELETE" };
    run(
        &std::env::temp_dir(),
        &["api", "--method", method, path.as_str()],
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    #[test]
    fn for_me_filters_become_gh_flags_and_search_qualifiers() {
        let pulls = |mine, search: Option<&str>| {
            super::pull_list_args(&super::PrFilter {
                mine,
                search: search.map(str::to_string),
                ..Default::default()
            })
            .join(" ")
        };
        assert!(pulls(super::Mine::ReviewRequested, None).ends_with("--search review-requested:@me"));
        assert!(pulls(super::Mine::ReviewRequested, Some("fix"))
            .ends_with("--search review-requested:@me fix"));
        assert!(pulls(super::Mine::Assigned, None).contains("--assignee @me"));
        assert!(pulls(super::Mine::Created, None).contains("--author @me"));
        assert!(!pulls(super::Mine::Everyone, None).contains("@me"));

        let issues = |mine| {
            super::issue_list_args(&super::IssueFilter {
                mine,
                assignee: Some("someone".into()),
                ..Default::default()
            })
            .join(" ")
        };
        assert!(issues(super::Mine::Assigned).contains("--assignee @me"));
        assert!(!issues(super::Mine::Assigned).contains("someone"));
        assert!(issues(super::Mine::Mentioned).contains("--mention @me"));
        assert!(issues(super::Mine::Created).contains("--author @me"));
        assert!(issues(super::Mine::Everyone).contains("--assignee someone"));
    }

    #[test]
    fn logins_parse_from_commas_spaces_and_at_signs() {
        assert_eq!(super::parse_logins("@alice, bob  carol,"), ["alice", "bob", "carol"]);
        assert!(super::parse_logins(" , ").is_empty());
    }

    /// Git quotes a path with non-ASCII or special characters in the diff
    /// headers; the file row must still match `日本語.rs`, not the escapes.
    #[test]
    fn pr_diff_decodes_git_quoted_paths() {
        let raw = "diff --git \"a/\\346\\227\\245\\346\\234\\254\\350\\252\\236.rs\" \"b/\\346\\227\\245\\346\\234\\254\\350\\252\\236.rs\"\nindex 1..2 100644\n--- \"a/\\346\\227\\245\\346\\234\\254\\350\\252\\236.rs\"\n+++ \"b/\\346\\227\\245\\346\\234\\254\\350\\252\\236.rs\"\n@@ -1 +1 @@\n-a\n+b\ndiff --git \"a/tab\\there.txt\" \"b/tab\\there.txt\"\ndeleted file mode 100644\n--- \"a/tab\\there.txt\"\n+++ /dev/null\n@@ -1 +0,0 @@\n-x\n";
        let files = super::parse_pr_diff(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "日本語.rs");
        assert_eq!(files[0].lines.len(), 3, "the hunk and both lines");
        assert_eq!(files[1].path, "tab\there.txt", "a deleted file keeps its decoded name");
    }

    #[test]
    fn git_quoted_paths_decode_escapes_and_octal_bytes() {
        assert_eq!(super::unquote_git_path(r#""a\"b\\c""#).as_deref(), Some("a\"b\\c"));
        assert_eq!(super::unquote_git_path(r#""\303\251t\303\251""#).as_deref(), Some("été"));
        assert_eq!(super::unquote_git_path("plain"), None, "not quoted");
        assert_eq!(super::unquote_git_path(r#""dangling\""#), None, "unterminated");
    }

    /// Review threads come 100 a page; the cursor is only followed while
    /// GitHub says another page exists, and resolved threads are dropped per
    /// page so later pages' unresolved threads are never lost.
    #[test]
    fn review_thread_pages_carry_their_cursor() {
        let page = |more: bool| {
            format!(
                r#"{{"data":{{"repository":{{"pullRequest":{{"reviewThreads":{{
                    "pageInfo":{{"hasNextPage":{more},"endCursor":"Y3Vy"}},
                    "nodes":[
                        {{"isResolved":true,"path":"a.rs","line":1,"comments":{{"nodes":[]}}}},
                        {{"isResolved":false,"path":"b.rs","line":2,"comments":{{"nodes":[]}}}}
                    ]}}}}}}}}}}"#
            )
        };
        let (threads, next) = super::parse_review_thread_page(&page(true)).unwrap();
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].path, "b.rs");
        assert_eq!(next.as_deref(), Some("Y3Vy"));
        let (_, last) = super::parse_review_thread_page(&page(false)).unwrap();
        assert_eq!(last, None, "the last page ends the walk");
    }

    #[test]
    fn pr_diff_splits_files_and_numbers_both_sides() {
        let raw = "diff --git a/src/a.rs b/src/a.rs\nindex 1..2 100644\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -1,3 +1,3 @@ fn a\n keep\n-old\n+new\n keep2\ndiff --git a/img.png b/img.png\nBinary files differ\n";
        let files = super::parse_pr_diff(raw);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        let kinds: Vec<_> = files[0].lines.iter().map(|l| l.kind).collect();
        use super::PrLineKind::*;
        assert_eq!(kinds, [Hunk, Context, Removed, Added, Context]);
        let removed = &files[0].lines[2];
        assert_eq!((removed.old_line, removed.new_line), (Some(2), None));
        let added = &files[0].lines[3];
        assert_eq!((added.old_line, added.new_line), (None, Some(2)));
        assert_eq!(files[0].lines[4].new_line, Some(3));
        assert_eq!(super::comment_anchor(removed), Some((2, "LEFT")));
        assert_eq!(super::comment_anchor(added), Some((2, "RIGHT")));
        assert_eq!(super::comment_anchor(&files[0].lines[0]), None);
        assert!(files[1].lines.is_empty(), "a binary file has no text lines");
    }

    #[test]
    fn pr_diff_caps_long_files() {
        let mut raw = String::from("diff --git a/x b/x\n+++ b/x\n@@ -0,0 +1,700 @@\n");
        for n in 0..700 {
            raw.push_str(&format!("+line {n}\n"));
        }
        let files = super::parse_pr_diff(&raw);
        assert_eq!(files[0].lines.len(), super::MAX_PR_DIFF_LINES);
        assert_eq!(files[0].hidden, 701 - super::MAX_PR_DIFF_LINES);
    }

    #[test]
    fn review_threads_keep_only_unresolved_ones() {
        let raw = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[
            {"isResolved":true,"path":"a.rs","line":1,"comments":{"nodes":[]}},
            {"isResolved":false,"path":"b.rs","line":null,"originalLine":9,
             "comments":{"nodes":[{"author":{"login":"rev"},"body":"rename this"},{"author":null,"body":"ok"}]}}
        ]}}}}}"#;
        let threads = super::parse_review_thread_page(raw).unwrap().0;
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].path, "b.rs");
        assert_eq!(threads[0].line, Some(9));
        assert_eq!(threads[0].comments[0], ("rev".into(), "rename this".into()));
        assert_eq!(threads[0].comments[1].0, "ghost");
        assert!(super::parse_review_thread_page("{}").unwrap().0.is_empty());
    }

    #[test]
    fn release_tags_follow_gits_ref_rules() {
        for good in ["v1.2.0", "release/2026-11", "1.0"] {
            assert!(super::valid_tag(good), "{good}");
        }
        for bad in ["", "v 1", "-v1", "a..b", "v1.", "x~1", "a:b", "tag/", "a\\b"] {
            assert!(!super::valid_tag(bad), "{bad}");
        }
    }

    #[test]
    fn runs_read_like_checks() {
        let run = |status: &str, conclusion: &str| super::GhRun {
            status: status.into(),
            conclusion: conclusion.into(),
            ..Default::default()
        };
        assert_eq!(run("in_progress", "").bucket(), super::CheckBucket::Pending);
        assert!(run("queued", "").is_running());
        assert_eq!(run("completed", "success").bucket(), super::CheckBucket::Passed);
        assert_eq!(run("completed", "failure").bucket(), super::CheckBucket::Failed);
        assert_eq!(run("completed", "cancelled").bucket(), super::CheckBucket::Skipped);
    }

    #[test]
    fn repo_slug_reads_from_any_url_inside_a_repository() {
        assert_eq!(
            super::url_repo_slug("https://github.com/o/r/pull/7"),
            Some(("o".into(), "r".into()))
        );
        assert_eq!(super::url_repo_slug("https://gitlab.com/o/r"), None);
    }

    #[test]
    fn star_counts_abbreviate_like_github_without_rounding_up() {
        assert_eq!(super::compact_count(0), "0");
        assert_eq!(super::compact_count(999), "999");
        assert_eq!(super::compact_count(1_000), "1k");
        assert_eq!(super::compact_count(1_050), "1k");
        assert_eq!(super::compact_count(1_250), "1.2k");
        assert_eq!(super::compact_count(1_999), "1.9k");
        assert_eq!(super::compact_count(12_500), "12k");
        assert_eq!(super::compact_count(999_999), "999k");
        assert_eq!(super::compact_count(1_500_000), "1.5m");
    }

    #[test]
    fn repo_view_json_parses_with_nulls() {
        let raw = r#"{"nameWithOwner":"imrj05/orbit","description":null,"url":"https://github.com/imrj05/orbit",
            "stargazerCount":42,"forkCount":3,"visibility":"PUBLIC","primaryLanguage":{"name":"Rust"},
            "defaultBranchRef":{"name":"main"},"viewerHasStarred":true}"#;
        let repo: super::GhRepo = serde_json::from_str(raw).unwrap();
        assert_eq!(repo.stars, 42);
        assert!(!repo.is_private());
        assert!(repo.starred);
        assert_eq!(repo.description, None);
        assert_eq!(repo.slug(), Some(("imrj05".into(), "orbit".into())));
        let private: super::GhRepo = serde_json::from_str(
            r#"{"nameWithOwner":"a/b","visibility":"PRIVATE","primaryLanguage":null,"defaultBranchRef":null}"#,
        )
        .unwrap();
        assert!(private.is_private());
        assert_eq!(private.language, None);
    }

    #[test]
    fn repo_filter_matches_owner_and_name_and_steps_aside_for_urls() {
        let repo = |name: &str| super::GhRepoEntry {
            name_with_owner: name.into(),
            ..Default::default()
        };
        let repos = vec![repo("me/orbit"), repo("me/notes"), repo("org/Orbit-site")];
        let names = |query: &str, limit: usize| -> Vec<String> {
            super::filter_repos(&repos, query, limit)
                .into_iter()
                .map(|r| r.name_with_owner.clone())
                .collect()
        };
        assert_eq!(names("", 2), ["me/orbit", "me/notes"]);
        assert_eq!(names("ORBIT", 5), ["me/orbit", "org/Orbit-site"]);
        assert_eq!(names("org/", 5), ["org/Orbit-site"]);
        assert!(names("https://github.com/me/orbit", 5).is_empty());
        assert!(names("git@github.com:me/orbit.git", 5).is_empty());
        assert_eq!(repos[2].name(), "Orbit-site");
    }

    #[test]
    fn actions_ids_read_run_and_job_from_check_links() {
        assert_eq!(
            super::actions_ids("https://github.com/o/r/actions/runs/123/job/456"),
            Some((123, Some(456)))
        );
        assert_eq!(
            super::actions_ids("https://github.com/o/r/actions/runs/123?check_suite_focus=true"),
            Some((123, None))
        );
        assert_eq!(super::actions_ids("https://ci.example.com/build/9"), None);
        assert_eq!(super::actions_ids(""), None);
    }

    #[test]
    fn log_tail_keeps_whole_lines_from_the_end() {
        assert_eq!(super::log_tail("short", 100), "short");
        let log = "first line\nsecond line\nthird line\n";
        assert_eq!(super::log_tail(log, 15), "third line\n");
        // Never splits a multi-byte character.
        let tail = super::log_tail("ééééé\nend", 6);
        assert!(tail.ends_with("end"), "{tail}");
    }

    #[test]
    fn a_branch_without_a_pull_request_is_not_an_error() {
        assert!(super::is_no_pull(
            "no pull requests found for branch \"feature\""
        ));
        assert!(!super::is_no_pull("HTTP 401: Bad credentials"));
    }

    #[test]
    fn repo_names_follow_githubs_rules() {
        assert!(super::valid_repo_name("orbit"));
        assert!(super::valid_repo_name("my_app-2.0"));
        assert!(!super::valid_repo_name(""));
        assert!(!super::valid_repo_name("."));
        assert!(!super::valid_repo_name(".."));
        assert!(!super::valid_repo_name("has space"));
        assert!(!super::valid_repo_name("owner/name"));
        assert!(!super::valid_repo_name(&"a".repeat(101)));
        assert_eq!(super::suggested_repo_name("My Project"), "My-Project");
        assert_eq!(super::suggested_repo_name("  app  "), "app");
        assert_eq!(super::suggested_repo_name("日本"), "");
    }

    #[test]
    fn repo_create_args_publish_the_working_directory_as_origin() {
        assert_eq!(
            super::repo_create_args("app", true, true),
            [
                "repo", "create", "app", "--source", ".", "--remote", "origin", "--private",
                "--push"
            ]
        );
        assert_eq!(
            super::repo_create_args("app", false, false),
            ["repo", "create", "app", "--source", ".", "--remote", "origin", "--public"]
        );
    }

    #[test]
    fn github_repo_slug_reads_owner_and_name() {
        let slug = |url: &str| super::github_repo_slug(url);
        let pair = |o: &str, n: &str| Some((o.to_string(), n.to_string()));
        assert_eq!(slug("https://github.com/imrj05/orbit"), pair("imrj05", "orbit"));
        assert_eq!(slug("https://github.com/imrj05/orbit/"), pair("imrj05", "orbit"));
        assert_eq!(slug("https://github.com/imrj05/orbit.git"), pair("imrj05", "orbit"));
        assert_eq!(slug("https://gitlab.com/imrj05/orbit"), None);
        assert_eq!(slug("https://github.com/imrj05"), None);
        assert_eq!(slug("https://github.com/imrj05/orbit/issues"), None);
        assert_eq!(slug("not a url"), None);
    }

    #[test]
    fn starred_probe_reads_404_as_not_starred() {
        assert_eq!(super::starred_from(Ok(String::new())), Ok(true));
        assert_eq!(
            super::starred_from(Err("gh: Not Found (HTTP 404)".into())),
            Ok(false)
        );
        // Signed out or a missing scope is not "not starred": the card must
        // fall back to the browser instead of offering a button that fails.
        assert!(super::starred_from(Err(
            "To get started with GitHub CLI, please run:  gh auth login".into()
        ))
        .is_err());
        assert!(super::starred_from(Err("gh: Resource not accessible (HTTP 403)".into())).is_err());
    }

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

    #[test]
    fn commit_github_reads_pulls_checks_and_nulls() {
        let raw = r#"{"data":{"repository":{"object":{
            "associatedPullRequests":{"nodes":[{"number":42,"title":"Add graph","state":"MERGED","isDraft":false,
              "url":"https://github.com/o/r/pull/42","headRefName":"feat/graph","baseRefName":"main",
              "author":{"login":"ada"},"mergedBy":{"login":"lin"}},
              {"number":7,"title":null,"state":"OPEN","isDraft":true,"url":null,"headRefName":null,
               "baseRefName":null,"author":null,"mergedBy":null}]},
            "statusCheckRollup":{"contexts":{"nodes":[
              {"__typename":"CheckRun","name":"build","status":"IN_PROGRESS","conclusion":null,"detailsUrl":null},
              {"__typename":"StatusContext","context":"ci/lint","state":"SUCCESS","targetUrl":"https://ci","description":null}]}}
        }}}}"#;
        let info = parse_commit_github(raw).unwrap();
        assert_eq!(info.pulls.len(), 2);
        assert_eq!(info.pulls[0].number, 42);
        assert!(info.pulls[0].is_merged());
        assert_eq!(info.pulls[0].merged_by.as_deref(), Some("lin"));
        assert_eq!(info.pulls[1].author, None);
        assert!(info.pulls[1].is_draft);
        assert_eq!(info.checks[0].bucket_kind(), CheckBucket::Pending);
        assert_eq!(info.checks[1].bucket_kind(), CheckBucket::Passed);
        assert_eq!(checks_rollup(&info.checks), Some(CheckBucket::Pending));
    }

    #[test]
    fn commit_github_unknown_commit_is_empty_not_an_error() {
        let raw = r#"{"data":{"repository":{"object":null}}}"#;
        assert_eq!(parse_commit_github(raw).unwrap(), CommitGithub::default());
        let raw = r#"{"data":{"repository":{"object":{"associatedPullRequests":{"nodes":[]},"statusCheckRollup":null}}}}"#;
        let info = parse_commit_github(raw).unwrap();
        assert!(info.pulls.is_empty() && info.checks.is_empty());
        assert_eq!(checks_rollup(&info.checks), None);
    }
}
