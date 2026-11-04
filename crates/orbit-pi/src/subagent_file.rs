//! The agent file format: a lenient reader for `~/.pi/agent/agents/*.md` and a
//! **surgical** writer that changes only the five keys Orbit owns.
//!
//! The file is shared with `pi-subagents`, so anything this module does not
//! understand — another extension's `memory:`, `skills:`, comments, key order,
//! the body's exact bytes — must survive a save untouched. That is why the
//! writer replaces individual frontmatter lines instead of re-serializing:
//! a YAML round-trip would drop the comment above `name:` and reorder the rest.
//!
//! Pure and offline: discovery and I/O take paths, so every rule here is
//! testable without a home directory.

use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The builtins the extension ships, embedded from the same file it reads, so
/// the page can show and reset the shipped text (D-X).
pub(crate) const BUILTINS_JSON: &str =
    include_str!("../../../contrib/orbit-subagents-extension/builtins.json");

/// Where an agent came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Builtin,
    User,
}

/// The five keys Orbit owns plus the prompt body.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fields {
    pub name: String,
    pub description: String,
    pub model: Option<String>,
    pub thinking: Option<String>,
    /// `None` — no `tools:` key, so pi's defaults apply (they include `bash`).
    /// `Some(vec![])` — the key is present and empty: no tools at all.
    pub tools: Option<Vec<String>>,
    pub body: String,
}

/// A parsed agent file: the fields, and the raw pieces the writer needs.
#[derive(Clone, Debug)]
pub(crate) struct FileDoc {
    pub fields: Fields,
    /// The frontmatter lines between the delimiters, verbatim.
    preamble: Vec<String>,
    /// Everything after the closing delimiter's line terminator, verbatim.
    body_raw: String,
    newline: String,
    original: Vec<u8>,
}

impl FileDoc {
    /// Parse a file. Structural failures are errors the page shows instead of
    /// rewriting: a file with no frontmatter is reported, never "fixed".
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, String> {
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| "the file is not valid UTF-8".to_string())?;
        let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let lines: Vec<&str> = text.split(newline).collect();
        if lines.first().map(|line| line.trim_end()) != Some("---") {
            return Err("no frontmatter".to_string());
        }
        let end = lines
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, line)| line.trim_end() == "---")
            .map(|(index, _)| index)
            .ok_or_else(|| "unterminated frontmatter".to_string())?;

        let preamble: Vec<String> = lines[1..end].iter().map(|line| line.to_string()).collect();
        // `body_raw` starts right after the closing delimiter's terminator, so
        // reassembly is exact even when the body begins with a blank line.
        let head = lines[..=end].join(newline);
        let body_raw = text
            .strip_prefix(&head)
            .map(|rest| rest.strip_prefix(newline).unwrap_or(rest))
            .unwrap_or("")
            .to_string();

        let fields = Fields {
            name: scalar(&preamble, "name").unwrap_or_default(),
            description: scalar(&preamble, "description").unwrap_or_default(),
            model: scalar(&preamble, "model").filter(|value| !value.is_empty()),
            thinking: scalar(&preamble, "thinking").filter(|value| !value.is_empty()),
            tools: list_value(&preamble, "tools"),
            body: body_raw.trim_matches(['\r', '\n']).to_string(),
        };
        Ok(Self {
            fields,
            preamble,
            body_raw,
            newline: newline.to_string(),
            original: bytes.to_vec(),
        })
    }

    /// Write `next` over this document. Unchanged values keep their exact
    /// original lines (quotes, comments, and block-list shape included); only
    /// the intended line changes.
    pub(crate) fn rewrite(&self, next: &Fields) -> Vec<u8> {
        if next == &self.fields {
            return self.original.clone();
        }
        let mut lines = self.preamble.clone();

        set_scalar(&mut lines, "name", Some(&next.name));
        set_scalar(&mut lines, "description", Some(&next.description));
        set_scalar(&mut lines, "model", next.model.as_deref());
        set_scalar(&mut lines, "thinking", next.thinking.as_deref());
        set_list(&mut lines, "tools", next.tools.as_deref());

        let header = lines.join(&self.newline);
        let text = if next.body == self.fields.body {
            // The untouched body keeps its own leading blank line exactly.
            format!(
                "---{nl}{header}{nl}---{nl}{body}",
                nl = self.newline,
                body = self.body_raw
            )
        } else {
            format!(
                "---{nl}{header}{nl}---{nl}{nl}{body}{nl}",
                nl = self.newline,
                body = next.body.trim_end()
            )
        };
        text.into_bytes()
    }
}

/// The value of a scalar key in the frontmatter, unquoted.
fn scalar(lines: &[String], key: &str) -> Option<String> {
    for line in lines {
        let Some((found, value)) = split_key(line) else {
            continue;
        };
        if found == key {
            return Some(unquote(value.trim()));
        }
    }
    None
}

/// A `tools:` value: inline (`[a, b]` or `a, b`) or a following block list.
fn list_value(lines: &[String], key: &str) -> Option<Vec<String>> {
    for (index, line) in lines.iter().enumerate() {
        let Some((found, value)) = split_key(line) else {
            continue;
        };
        if found != key {
            continue;
        }
        let value = value.trim();
        if value.is_empty() {
            let mut items = Vec::new();
            for follow in &lines[index + 1..] {
                let trimmed = follow.trim_start();
                if let Some(item) = trimmed.strip_prefix("- ") {
                    items.push(unquote(item.trim()));
                } else if follow.trim().is_empty() {
                    continue;
                } else {
                    break;
                }
            }
            return Some(items);
        }
        let inner = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(value);
        return Some(
            inner
                .split(',')
                .map(|item| unquote(item.trim()))
                .filter(|item| !item.is_empty())
                .collect(),
        );
    }
    None
}

/// `key: value` with no indentation. Returns `(key, rest)`.
fn split_key(line: &str) -> Option<(&str, &str)> {
    if line.starts_with([' ', '\t', '#']) {
        return None;
    }
    let (key, value) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some((key, value))
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return trimmed[1..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

/// Replace, insert, or remove one scalar key, leaving every other line intact.
fn set_scalar(lines: &mut Vec<String>, key: &str, value: Option<&str>) {
    let existing = lines
        .iter()
        .position(|line| split_key(line).is_some_and(|(found, _)| found == key));
    match (existing, value) {
        (Some(index), Some(value)) => {
            // Keep the original line when the value already matches, so a
            // quoted or commented line round-trips byte-for-byte.
            let current = split_key(&lines[index]).map(|(_, value)| unquote(value.trim()));
            if current.as_deref() != Some(value) {
                lines[index] = format!("{key}: {value}");
            }
        }
        (Some(index), None) => {
            lines.remove(index);
        }
        (None, Some(value)) => {
            let at = insert_position(lines, key);
            lines.insert(at, format!("{key}: {value}"));
        }
        (None, None) => {}
    }
}

/// Replace a list key. A changed list becomes one comma-separated line; a
/// removed key takes its block-list items with it.
fn set_list(lines: &mut Vec<String>, key: &str, value: Option<&[String]>) {
    let index = lines
        .iter()
        .position(|line| split_key(line).is_some_and(|(found, _)| found == key));
    let Some(index) = index else {
        if let Some(value) = value {
            let at = insert_position(lines, key);
            lines.insert(at, format!("{key}: {}", value.join(", ")));
        }
        return;
    };
    // The items that belong to this key: following `- item` lines.
    let mut last = index;
    while last + 1 < lines.len() && lines[last + 1].trim_start().starts_with("- ") {
        last += 1;
    }
    match value {
        Some(value) => {
            let rendered = format!("{key}: {}", value.join(", "));
            lines[index] = rendered;
            if last > index {
                lines.drain(index + 1..=last);
            }
        }
        None => {
            lines.drain(index..=last);
        }
    }
}

/// The canonical order of the keys Orbit owns, used to insert a missing key in
/// place rather than at the end.
fn owned_rank(key: &str) -> Option<u8> {
    match key {
        "name" => Some(0),
        "description" => Some(1),
        "model" => Some(2),
        "thinking" => Some(3),
        "tools" => Some(4),
        _ => None,
    }
}

/// Where a missing `key` belongs: after the last owned key that precedes it in
/// the canonical order, or at the top when none does.
fn insert_position(lines: &[String], key: &str) -> usize {
    let target = owned_rank(key).unwrap_or(4);
    let mut at = None;
    for (index, line) in lines.iter().enumerate() {
        if let Some((found, _)) = split_key(line) {
            if owned_rank(found).is_some_and(|rank| rank < target) {
                at = Some(index);
            }
        }
    }
    at.map(|index| index + 1).unwrap_or(0)
}

/// One agent, discovered or shipped.
#[derive(Clone, Debug)]
pub(crate) struct Agent {
    pub fields: Fields,
    pub source: Source,
    pub path: Option<PathBuf>,
    pub modified: Option<SystemTime>,
    /// Why this file could not be read as an agent; the row shows it instead of
    /// pretending the file is one.
    pub parse_error: Option<String>,
}

impl Agent {
    pub(crate) fn name(&self) -> &str {
        &self.fields.name
    }

    /// The maximum risk the agent's declared tools imply (D-Q).
    pub(crate) fn ceiling(&self) -> &'static str {
        ceiling_for_tools(self.fields.tools.as_deref())
    }

    /// The capability tier as the page's radio group understands it.
    pub(crate) fn tier(&self) -> Option<Tier> {
        match self.fields.tools.as_deref() {
            Some(tools) => Tier::from_tools(tools),
            None => None,
        }
    }

    /// The content hash the page captured when it loaded this file, used to
    /// refuse a save that would clobber an external edit.
    pub(crate) fn hash(&self) -> Option<u64> {
        let path = self.path.as_ref()?;
        let bytes = fs::read(path).ok()?;
        Some(hash_bytes(&bytes))
    }
}

/// The page's three capability tiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    Read,
    Edit,
    Exec,
}

impl Tier {
    pub(crate) const ALL: [Tier; 3] = [Tier::Read, Tier::Edit, Tier::Exec];

    pub(crate) fn tools(self) -> Vec<String> {
        let read = ["read", "grep", "find", "ls"];
        let mut tools: Vec<String> = read.iter().map(|tool| tool.to_string()).collect();
        if matches!(self, Self::Edit | Self::Exec) {
            tools.extend(["edit".to_string(), "write".to_string()]);
        }
        if matches!(self, Self::Exec) {
            tools.push("bash".to_string());
            // Windows' command tool is `powershell`; naming it on Unix would be
            // an unknown tool that does nothing.
            if cfg!(windows) {
                tools.push("powershell".to_string());
            }
        }
        tools
    }

    /// The tier a declared list matches exactly, if any.
    pub(crate) fn from_tools(tools: &[String]) -> Option<Self> {
        Self::ALL.into_iter().find(|tier| tier.tools() == tools)
    }
}

/// The ceiling rule, mirroring `agents.js` exactly: absent means pi's default
/// set (which includes `bash`), empty means no tools, anything unrecognised
/// means `all`.
pub(crate) fn ceiling_for_tools(tools: Option<&[String]>) -> &'static str {
    let Some(tools) = tools else {
        return "exec";
    };
    if tools.is_empty() {
        return "read";
    }
    let mut rank = 1;
    for tool in tools {
        let kind = classify_tool(tool);
        let value = match kind {
            "read" => 1,
            "edit" => 2,
            "exec" => 3,
            _ => 4,
        };
        rank = rank.max(value);
    }
    match rank {
        1 => "read",
        2 => "edit",
        3 => "exec",
        _ => "all",
    }
}

/// Tool risk classes, the same vocabulary the guard uses.
pub(crate) fn classify_tool(name: &str) -> &'static str {
    let tool = name.trim().to_ascii_lowercase();
    match tool.as_str() {
        "read" | "grep" | "find" | "ls" | "glob" | "list" | "webfetch" => "read",
        "edit" | "write" | "multiedit" | "apply_patch" | "str_replace_editor" => "edit",
        "bash" | "powershell" | "shell" | "exec" | "terminal" => "exec",
        _ => "all",
    }
}

/// `~/.pi/agent/agents` — the directory `pi-subagents` also reads (D-D).
pub(crate) fn agents_dir() -> PathBuf {
    crate::platform::home_dir_opt()
        .unwrap_or_default()
        .join(".pi")
        .join("agent")
        .join("agents")
}

/// The shipped agents, lowest priority.
pub(crate) fn builtin_agents() -> Vec<Agent> {
    let parsed: serde_json::Value = match serde_json::from_str(BUILTINS_JSON) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let mut agents = Vec::new();
    for value in parsed
        .get("agents")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let text = |key: &str| value.get(key).and_then(serde_json::Value::as_str);
        agents.push(Agent {
            fields: Fields {
                name: text("name").unwrap_or_default().to_string(),
                description: text("description").unwrap_or_default().to_string(),
                model: None,
                thinking: None,
                tools: text("tools").map(|tools| {
                    tools
                        .split(',')
                        .map(|tool| tool.trim().to_string())
                        .filter(|tool| !tool.is_empty())
                        .collect()
                }),
                body: text("prompt").unwrap_or_default().to_string(),
            },
            source: Source::Builtin,
            path: None,
            modified: None,
            parse_error: None,
        });
    }
    agents
}

/// Every agent the tool can pick: builtins, then user files (which override a
/// builtin by name), sorted by name. `skipped` carries the files that could not
/// be read as an agent, so the page can surface them instead of hiding them.
pub(crate) fn discover() -> (Vec<Agent>, Vec<(PathBuf, String)>) {
    let mut by_name: std::collections::BTreeMap<String, Agent> = std::collections::BTreeMap::new();
    for agent in builtin_agents() {
        by_name.insert(agent.fields.name.clone(), agent);
    }
    let mut skipped = Vec::new();
    for path in markdown_files(&agents_dir()) {
        match read(&path) {
            Ok(doc) => {
                if doc.fields.name.trim().is_empty() || doc.fields.description.trim().is_empty() {
                    skipped.push((path, "name and description are required".to_string()));
                    continue;
                }
                if !is_kebab_name(&doc.fields.name) {
                    skipped.push((path, "name is not kebab-case".to_string()));
                    continue;
                }
                let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
                by_name.insert(
                    doc.fields.name.clone(),
                    Agent {
                        modified,
                        fields: doc.fields,
                        source: Source::User,
                        path: Some(path),
                        parse_error: None,
                    },
                );
            }
            Err(error) => skipped.push((path, error)),
        }
    }
    (by_name.into_values().collect(), skipped)
}

/// `*.md` files under `dir`, recursively, sorted for determinism.
fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, 0, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, out);
        } else if path.extension().is_some_and(|extension| extension == "md") {
            out.push(path);
        }
    }
}

pub(crate) fn is_kebab_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// A stable content hash for change detection.
pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SaveError {
    /// The file changed since the page loaded it; the caller asks before
    /// overwriting (D-U).
    Changed,
    Io(String),
    Parse(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Changed => formatter.write_str("the file changed since it was loaded"),
            Self::Io(error) => formatter.write_str(error),
            Self::Parse(error) => formatter.write_str(error),
        }
    }
}

/// Read a user agent file.
pub(crate) fn read(path: &Path) -> Result<FileDoc, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    FileDoc::parse(&bytes)
}

/// Save `fields` into `path`, refusing to clobber a file whose bytes no longer
/// match `expected_hash`. Creates the directory for a new agent.
pub(crate) fn save(
    path: &Path,
    fields: &Fields,
    expected_hash: Option<u64>,
) -> Result<u64, SaveError> {
    let current = fs::read(path).ok();
    if let Some(expected) = expected_hash {
        let matches = current
            .as_deref()
            .map(hash_bytes)
            .is_some_and(|hash| hash == expected);
        if !matches {
            return Err(SaveError::Changed);
        }
    }
    let doc = match current.as_deref() {
        Some(bytes) => FileDoc::parse(bytes).map_err(SaveError::Parse)?,
        None => FileDoc::parse(format!("---{}---{}", "\n", "\n").as_bytes())
            .map_err(SaveError::Parse)?,
    };
    let bytes = doc.rewrite(fields);
    write_atomic(path, &bytes).map_err(SaveError::Io)?;
    Ok(hash_bytes(&bytes))
}

/// Write beside the target and rename into place, so a crash cannot leave half
/// a file for the other reader (the `mcp/config.rs` rule).
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "no parent directory".to_string())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("agent"),
        std::process::id()
    ));
    fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "---\n# keep this comment\nname: scout\nmemory: project\nskills: [pi-web-access]\ndescription: Fast recon\ntools: read, grep, find, ls\nmodel: anthropic/claude-haiku-4-5\n---\n\nYou are a scout.\n";

    fn parse(text: &str) -> FileDoc {
        FileDoc::parse(text.as_bytes()).expect("parses")
    }

    fn fields(doc: &FileDoc) -> Fields {
        doc.fields.clone()
    }

    #[test]
    fn parses_the_five_keys_and_preserves_the_body() {
        let doc = parse(SAMPLE);
        assert_eq!(doc.fields.name, "scout");
        assert_eq!(doc.fields.description, "Fast recon");
        assert_eq!(
            doc.fields.model.as_deref(),
            Some("anthropic/claude-haiku-4-5")
        );
        assert_eq!(doc.fields.thinking, None);
        assert_eq!(
            doc.fields.tools.as_deref(),
            Some(["read", "grep", "find", "ls"].map(String::from).as_slice())
        );
        assert_eq!(doc.fields.body, "You are a scout.");
    }

    #[test]
    fn a_round_trip_with_no_changes_is_byte_identical() {
        let doc = parse(SAMPLE);
        assert_eq!(doc.rewrite(&fields(&doc)), SAMPLE.as_bytes());
        // Inline comments, quotes and CRLF survive too.
        let quoted = "---\r\n# note\r\nname: 'worker'\r\ndescription: \"Implements\"\r\ntools:\r\n  - read\r\n  - bash\r\n---\r\n\r\nBody.\r\n";
        let doc = parse(quoted);
        assert_eq!(doc.rewrite(&fields(&doc)), quoted.as_bytes());
    }

    #[test]
    fn changing_one_key_touches_one_line() {
        let doc = parse(SAMPLE);
        let mut next = fields(&doc);
        next.tools = Some(vec!["read".into(), "grep".into()]);
        let written = String::from_utf8(doc.rewrite(&next)).unwrap();
        assert!(written.contains("tools: read, grep\n"));
        // The comment, the foreign `memory:` key, and the body are untouched.
        assert!(written.contains("# keep this comment"));
        assert!(written.contains("memory: project"));
        assert!(written.contains("skills: [pi-web-access]"));
        assert!(written.contains("You are a scout."));
        // Exactly one line differs from the original.
        let before: Vec<&str> = SAMPLE.lines().collect();
        let after: Vec<&str> = written.lines().collect();
        assert_eq!(before.len(), after.len());
        let diffs: Vec<usize> = before
            .iter()
            .zip(after.iter())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(diffs.len(), 1);
    }

    #[test]
    fn a_missing_key_is_inserted_after_description_without_reordering() {
        let doc = parse("---\nname: scout\ndescription: Recon\n---\nBody.\n");
        let mut next = fields(&doc);
        next.model = Some("provider/model".into());
        next.thinking = Some("high".into());
        let written = String::from_utf8(doc.rewrite(&next)).unwrap();
        assert_eq!(
            written,
            "---\nname: scout\ndescription: Recon\nmodel: provider/model\nthinking: high\n---\nBody.\n"
        );
    }

    #[test]
    fn clearing_an_optional_key_removes_its_line() {
        let doc = parse(SAMPLE);
        let mut next = fields(&doc);
        next.model = None;
        let written = String::from_utf8(doc.rewrite(&next)).unwrap();
        assert!(!written.contains("model:"));
        assert!(written.contains("description: Fast recon"));
    }

    #[test]
    fn a_block_list_is_replaced_by_one_inline_line() {
        let text =
            "---\nname: worker\ndescription: Implements\ntools:\n  - read\n  - bash\n---\nBody.\n";
        let doc = parse(text);
        assert_eq!(
            doc.fields.tools.as_deref(),
            Some(["read", "bash"].map(String::from).as_slice())
        );
        let mut next = fields(&doc);
        next.tools = Some(vec!["read".into()]);
        let written = String::from_utf8(doc.rewrite(&next)).unwrap();
        assert_eq!(
            written,
            "---\nname: worker\ndescription: Implements\ntools: read\n---\nBody.\n"
        );
    }

    #[test]
    fn changing_the_body_replaces_only_the_body() {
        let doc = parse(SAMPLE);
        let mut next = fields(&doc);
        next.body = "You are a reviewer.".into();
        let written = String::from_utf8(doc.rewrite(&next)).unwrap();
        assert!(written.starts_with("---\n# keep this comment\n"));
        assert!(written.ends_with("---\n\nYou are a reviewer.\n"));
    }

    #[test]
    fn a_file_with_no_frontmatter_or_no_end_is_reported() {
        assert_eq!(
            FileDoc::parse(b"just a prompt").err().as_deref(),
            Some("no frontmatter")
        );
        assert_eq!(
            FileDoc::parse(b"---\nname: x\n").err().as_deref(),
            Some("unterminated frontmatter")
        );
    }

    #[test]
    fn the_ceiling_matches_the_extension_rule() {
        assert_eq!(ceiling_for_tools(None), "exec");
        assert_eq!(ceiling_for_tools(Some(&[])), "read");
        assert_eq!(
            ceiling_for_tools(Some(&["read".into(), "grep".into()])),
            "read"
        );
        assert_eq!(
            ceiling_for_tools(Some(&["read".into(), "edit".into()])),
            "edit"
        );
        assert_eq!(
            ceiling_for_tools(Some(&["read".into(), "bash".into()])),
            "exec"
        );
        assert_eq!(ceiling_for_tools(Some(&["mcp__x__y".into()])), "all");
        assert_eq!(ceiling_for_tools(Some(&["my_tool".into()])), "all");
    }

    #[test]
    fn tiers_round_trip_through_their_tool_lists() {
        for tier in Tier::ALL {
            assert_eq!(Tier::from_tools(&tier.tools()), Some(tier));
        }
        // A hand-written list above a tier is not a tier; the page shows it as
        // Advanced rather than silently narrowing it.
        assert_eq!(Tier::from_tools(&["read".into()]), None);
    }

    #[test]
    fn kebab_names_only() {
        assert!(is_kebab_name("scout"));
        assert!(is_kebab_name("code-reviewer"));
        assert!(!is_kebab_name("Scout"));
        assert!(!is_kebab_name("code_reviewer"));
        assert!(!is_kebab_name("-scout"));
        assert!(!is_kebab_name("a--b"));
    }

    #[test]
    fn the_shipped_builtins_parse() {
        let agents = builtin_agents();
        assert_eq!(
            agents.iter().map(Agent::name).collect::<Vec<_>>(),
            ["scout", "worker", "reviewer"]
        );
        assert_eq!(agents[0].ceiling(), "read");
        assert_eq!(agents[1].ceiling(), "exec");
        assert_eq!(agents[2].ceiling(), "read");
        for agent in &agents {
            assert!(!agent.fields.body.is_empty());
            assert!(agent
                .fields
                .tools
                .as_ref()
                .is_some_and(|tools| !tools.is_empty()));
        }
    }

    #[test]
    fn save_refuses_to_clobber_an_externally_changed_file() {
        let dir = std::env::temp_dir().join(format!(
            "orbit-subagent-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("scout.md");
        fs::write(&path, SAMPLE).unwrap();
        let doc = read(&path).unwrap();
        let loaded = hash_bytes(SAMPLE.as_bytes());

        // Someone else edits the file after the page loaded it.
        fs::write(&path, SAMPLE.replace("Fast recon", "Fast recon, edited")).unwrap();
        assert_eq!(
            save(&path, &doc.fields, Some(loaded)),
            Err(SaveError::Changed)
        );
        // The explicit overwrite (expected hash `None`) is allowed.
        save(&path, &doc.fields, None).unwrap();
        // And a successful save returns the new hash for the page to keep.
        let hash = save(
            &path,
            &doc.fields,
            Some(hash_bytes(&fs::read(&path).unwrap())),
        )
        .unwrap();
        assert_eq!(hash, hash_bytes(&fs::read(&path).unwrap()));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_creates_a_new_agent_with_the_expected_shape() {
        let dir = std::env::temp_dir().join(format!(
            "orbit-subagent-new-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("checker.md");
        let fields = Fields {
            name: "checker".into(),
            description: "Checks things".into(),
            model: None,
            thinking: None,
            tools: Some(Tier::Read.tools()),
            body: "You check things.".into(),
        };
        save(&path, &fields, None).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert_eq!(
            text,
            "---\nname: checker\ndescription: Checks things\ntools: read, grep, find, ls\n---\n\nYou check things.\n"
        );
        fs::remove_dir_all(&dir).ok();
    }
}
