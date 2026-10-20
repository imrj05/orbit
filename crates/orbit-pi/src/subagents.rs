//! Subagent role discovery and file management for Settings → Subagents.
//!
//! The runtime extension reads these same files:
//!
//! - built-in: `~/.orbit-pi/subagent-extension/agents/`
//! - global:   `~/.pi/agent/agents/`
//! - project:  the nearest `.pi/agents/` above the active workspace
//!
//! This module parses the small frontmatter subset the extension supports,
//! marks shadowed roles (project > global > built-in), and writes/deletes
//! user and project files. Built-in files are read-only in the panel; the UI
//! duplicates them into an editable scope.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

/// Which root an agent came from. Lower priority is shadowed by higher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SubagentScope {
    Builtin,
    Global,
    Project,
}

impl SubagentScope {
    pub(crate) fn priority(self) -> u8 {
        match self {
            Self::Builtin => 0,
            Self::Global => 1,
            Self::Project => 2,
        }
    }
}

/// One agent Markdown file, parsed for display and editing.
#[derive(Clone, Debug)]
pub(crate) struct Subagent {
    /// Frontmatter `name`; falls back to the file stem for an invalid file.
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub scope: SubagentScope,
    pub file: PathBuf,
    /// The complete file contents, kept for the raw editor.
    pub raw: String,
    pub valid: bool,
    /// A higher-priority role with the same runtime name exists.
    pub shadowed: bool,
    /// Why the file is invalid; `None` when it loads.
    pub error: Option<String>,
}

impl Subagent {
    pub fn display_path(&self) -> String {
        crate::skills::display_path(&self.file)
    }
}

/// The built-in agents directory the runtime extension materializes.
pub(crate) fn builtin_dir() -> Option<PathBuf> {
    crate::platform::home_dir_opt().map(|home| {
        home.join(".orbit-pi")
            .join("subagent-extension")
            .join("agents")
    })
}

/// The user-level agent directory.
pub(crate) fn global_dir() -> Option<PathBuf> {
    crate::platform::home_dir_opt().map(|home| home.join(".pi").join("agent").join("agents"))
}

/// Where a new project agent is written: the nearest existing `.pi/agents`
/// above the workspace (the same directory the runtime extension reads), or
/// the workspace itself when no project config exists yet.
pub(crate) fn project_dir(workspace: &Path) -> PathBuf {
    find_project_agents_dir(workspace).unwrap_or_else(|| workspace.join(".pi").join("agents"))
}

/// The write target for a scope. Built-ins are never written.
pub(crate) fn target_dir(workspace: &Path, scope: SubagentScope) -> Option<PathBuf> {
    match scope {
        SubagentScope::Builtin => None,
        SubagentScope::Global => global_dir(),
        SubagentScope::Project => Some(project_dir(workspace)),
    }
}

/// The nearest ancestor (or the workspace itself) with a `.pi/agents` dir.
pub(crate) fn find_project_agents_dir(workspace: &Path) -> Option<PathBuf> {
    let mut current = workspace.to_path_buf();
    loop {
        let candidate = current.join(".pi").join("agents");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Discover every agent role, highest-priority scope first.
pub(crate) fn discover(workspace: &Path) -> Vec<Subagent> {
    discover_in(
        builtin_dir().as_deref(),
        global_dir().as_deref(),
        find_project_agents_dir(workspace).as_deref(),
    )
}

/// Injectable discovery for tests.
pub(crate) fn discover_in(
    builtin: Option<&Path>,
    global: Option<&Path>,
    project: Option<&Path>,
) -> Vec<Subagent> {
    let mut agents = Vec::new();
    for (dir, scope) in [
        (builtin, SubagentScope::Builtin),
        (global, SubagentScope::Global),
        (project, SubagentScope::Project),
    ] {
        if let Some(dir) = dir {
            load_dir(dir, scope, &mut agents);
        }
    }

    let mut effective: HashMap<String, SubagentScope> = HashMap::new();
    for agent in &agents {
        effective
            .entry(agent.name.clone())
            .and_modify(|scope| {
                if agent.scope.priority() > scope.priority() {
                    *scope = agent.scope;
                }
            })
            .or_insert(agent.scope);
    }
    for agent in &mut agents {
        agent.shadowed = effective
            .get(&agent.name)
            .is_some_and(|scope| *scope != agent.scope);
    }

    // Project first, then global, then built-in; names alphabetically inside a
    // group. This mirrors the list's grouping.
    agents.sort_by(|a, b| {
        b.scope
            .priority()
            .cmp(&a.scope.priority())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.file.cmp(&b.file))
    });
    agents
}

fn load_dir(dir: &Path, scope: SubagentScope, out: &mut Vec<Subagent>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            load_dir(&path, scope, out);
            continue;
        }
        if path.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        out.push(parse_agent(&raw, &path, scope));
    }
}

/// Parse one agent file. A malformed file is still returned (with `valid:
/// false` and an `error`) so the panel can show it and let the user fix it.
pub(crate) fn parse_agent(raw: &str, file: &Path, scope: SubagentScope) -> Subagent {
    let fallback_name = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "agent".to_string());

    let Some((frontmatter, _body)) = split_frontmatter(raw) else {
        return Subagent {
            name: fallback_name,
            description: String::new(),
            tools: Vec::new(),
            model: None,
            thinking: None,
            scope,
            file: file.to_path_buf(),
            raw: raw.to_string(),
            valid: false,
            shadowed: false,
            error: Some("missing frontmatter".to_string()),
        };
    };

    let meta = parse_frontmatter(frontmatter);
    let name = meta
        .get("name")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback_name.clone());
    let description = meta
        .get("description")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let tools = meta
        .get("tools")
        .map(|value| parse_tool_list(value))
        .unwrap_or_default();
    let model = meta
        .get("model")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let thinking = meta
        .get("thinking")
        .map(|value| value.trim().to_lowercase())
        .filter(|value| is_thinking_level(value));

    let mut error = None;
    if !meta.contains_key("name") || meta.get("name").is_some_and(|v| v.trim().is_empty()) {
        error = Some("missing `name`".to_string());
    } else if description.is_empty() {
        error = Some("missing `description`".to_string());
    }

    Subagent {
        name,
        description,
        tools,
        model,
        thinking,
        scope,
        file: file.to_path_buf(),
        raw: raw.to_string(),
        valid: error.is_none(),
        shadowed: false,
        error,
    }
}

fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    let text = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    if !text.starts_with("---") {
        return None;
    }
    let rest = &text[3..];
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let body = rest[end + 4..].trim_start_matches(['\r', '\n']).trim();
    Some((frontmatter, body))
}

fn parse_frontmatter(raw: &str) -> BTreeMap<String, String> {
    let mut meta = BTreeMap::new();
    let lines: Vec<&str> = raw.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let Some((key, value)) = lines[i].split_once(':') else {
            i += 1;
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() {
            i += 1;
            continue;
        }
        if key == "tools" && value.is_empty() {
            let mut tools = Vec::new();
            while i + 1 < lines.len() {
                let next = lines[i + 1].trim_start();
                let Some(item) = next.strip_prefix('-') else {
                    break;
                };
                let item = unquote(item.trim());
                if !item.is_empty() {
                    tools.push(item);
                }
                i += 1;
            }
            meta.insert("tools".to_string(), tools.join(", "));
        } else {
            meta.insert(key.to_string(), unquote(value));
        }
        i += 1;
    }
    meta
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

fn parse_tool_list(value: &str) -> Vec<String> {
    let mut raw = value.trim();
    if raw.starts_with('[') && raw.ends_with(']') {
        raw = &raw[1..raw.len() - 1];
    }
    raw.split(',')
        .map(unquote)
        .map(|tool| tool.trim().to_string())
        .filter(|tool| !tool.is_empty())
        .collect()
}

pub(crate) fn is_thinking_level(value: &str) -> bool {
    matches!(
        value,
        "off" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
    )
}

/// `~`-shortened path for the detail facts.
pub(crate) fn display_path(path: &Path) -> String {
    crate::skills::display_path(path)
}

/// A safe file stem for a runtime name.
pub(crate) fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in name.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash && !out.is_empty() {
            out.push('-');
            last_dash = true;
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "agent".to_string()
    } else {
        out
    }
}

/// A fresh `<dir>/<base>.md`, `<dir>/<base>-2.md`, … path.
pub(crate) fn unique_new_path(dir: &Path, base: &str) -> PathBuf {
    let base = slug(base);
    let mut candidate = dir.join(format!("{base}.md"));
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{base}-{n}.md"));
        n += 1;
    }
    candidate
}

/// The path a duplicate would occupy, or an error when it already exists.
pub(crate) fn duplicate_path(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let path = dir.join(format!("{}.md", slug(name)));
    if path.exists() {
        Err(format!(
            "An agent named `{}` already exists at {}.",
            name,
            display_path(&path)
        ))
    } else {
        Ok(path)
    }
}

/// A new-agent template with a unique runtime name.
pub(crate) fn template(name: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: Describe what this agent does\ntools: read, grep, find, ls\n---\n\nYou are {name}. Work only on the task you are given, and return a concise,\nstructured result the parent agent can use without re-reading your work.\n"
    )
}

pub(crate) fn write_agent(path: &Path, raw: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, raw)
}

pub(crate) fn delete_agent(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir() -> PathBuf {
        let unique = format!(
            "orbit-subagents-test-{}-{}",
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

    fn write_agent_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{name}.md"));
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn parses_frontmatter_block_tools_and_thinking() {
        let raw = "---\nname: repo-reviewer\ndescription: Reviews Rust changes\ntools:\n  - read\n  - grep\nmodel: ollama-cloud/glm-5.3\nthinking: HIGH\n---\n\nReview the change.\n";
        let agent = parse_agent(
            raw,
            Path::new("/tmp/repo-reviewer.md"),
            SubagentScope::Project,
        );
        assert!(agent.valid);
        assert_eq!(agent.name, "repo-reviewer");
        assert_eq!(agent.description, "Reviews Rust changes");
        assert_eq!(agent.tools, vec!["read", "grep"]);
        assert_eq!(agent.model.as_deref(), Some("ollama-cloud/glm-5.3"));
        assert_eq!(agent.thinking.as_deref(), Some("high"));
        assert!(!agent.shadowed);
        assert!(agent.error.is_none());
    }

    #[test]
    fn malformed_files_are_kept_for_editing() {
        let agent = parse_agent(
            "no frontmatter",
            Path::new("/tmp/broken-agent.md"),
            SubagentScope::Global,
        );
        assert!(!agent.valid);
        assert_eq!(agent.name, "broken-agent");
        assert_eq!(agent.error.as_deref(), Some("missing frontmatter"));
    }

    #[test]
    fn discovery_marks_shadowed_roles_and_orders_scopes() {
        let root = scratch_dir();
        let builtin = root.join("builtin");
        let global = root.join("global");
        let project = root.join("project");
        write_agent_file(
            &builtin,
            "scout",
            "---\nname: scout\ndescription: builtin scout\ntools: read\n---\n\nPrompt.\n",
        );
        write_agent_file(
            &global,
            "scout",
            "---\nname: scout\ndescription: global scout\ntools: read, grep\n---\n\nPrompt.\n",
        );
        write_agent_file(
            &project,
            "scout",
            "---\nname: scout\ndescription: project scout\ntools: read, grep, find\n---\n\nPrompt.\n",
        );
        write_agent_file(
            &global,
            "worker",
            "---\nname: worker\ndescription: worker\n---\n\nPrompt.\n",
        );

        let agents = discover_in(Some(&builtin), Some(&global), Some(&project));
        assert_eq!(agents.len(), 4);
        assert_eq!(agents[0].scope, SubagentScope::Project);
        let project_scout = &agents[0];
        assert!(!project_scout.shadowed);
        let global_scout = agents
            .iter()
            .find(|agent| agent.scope == SubagentScope::Global && agent.name == "scout")
            .unwrap();
        assert!(global_scout.shadowed);
        let builtin_scout = agents
            .iter()
            .find(|agent| agent.scope == SubagentScope::Builtin && agent.name == "scout")
            .unwrap();
        assert!(builtin_scout.shadowed);

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn unique_paths_avoid_collisions_and_templates_parse() {
        let root = scratch_dir();
        let first = unique_new_path(&root, "new-agent");
        assert!(first.ends_with("new-agent.md"));
        fs::write(&first, template("new-agent")).unwrap();
        let second = unique_new_path(&root, "new-agent");
        assert!(second.ends_with("new-agent-2.md"));

        let parsed = parse_agent(&template("new-agent-2"), &second, SubagentScope::Global);
        assert!(parsed.valid);
        assert_eq!(parsed.name, "new-agent-2");
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn duplicate_path_rejects_an_existing_role() {
        let root = scratch_dir();
        let path = root.join("scout.md");
        fs::write(&path, "x").unwrap();
        assert!(duplicate_path(&root, "scout").is_err());
        assert!(duplicate_path(&root, "other").is_ok());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn find_project_agents_dir_walks_up() {
        let root = scratch_dir();
        let nested = root.join("packages").join("app").join("src");
        fs::create_dir_all(&nested).unwrap();
        fs::create_dir_all(root.join(".pi").join("agents")).unwrap();
        assert_eq!(
            find_project_agents_dir(&nested).as_deref(),
            Some(root.join(".pi").join("agents").as_path())
        );
        fs::remove_dir_all(&root).ok();
    }
}
