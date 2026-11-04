/**
 * Agent discovery and capability derivation for the subagent tool.
 *
 * An agent is a markdown file with YAML-ish frontmatter. This module reads
 * exactly the five keys the tool needs (`name`, `description`, `model`,
 * `thinking`, `tools`) with a ~40-line reader rather than a YAML parser:
 * `contrib/` ships no dependencies, and a malformed file must degrade to one
 * skipped agent, never to a broken extension.
 *
 * The **ceiling** is derived from the declared `tools:` line. Deriving it —
 * rather than storing it a second time — is what makes a hand-written agent
 * file and a page-authored one behave identically, and it is what the guard
 * clamps every child tool call against.
 */

import fs from "node:fs";
import path from "node:path";

/** Tools that only observe. Mirrors the guard's read vocabulary. */
const READ_TOOLS = new Set(["read", "grep", "find", "ls", "glob", "list", "webfetch"]);

/** Tools that write files. */
const EDIT_TOOLS = new Set(["edit", "write", "multiedit", "apply_patch", "str_replace_editor"]);

/** Tools that run a process. */
const EXEC_TOOLS = new Set(["bash", "powershell", "shell", "exec", "terminal"]);

/** Risk ranks. `all` outranks `exec`: an unrecognised tool can do anything. */
export const CEILING_RANK = { read: 1, edit: 2, exec: 3, all: 4 };

/** The user-scope agent directory `pi-subagents` also reads. */
export function userAgentDir(home = defaultHome()) {
  return path.join(home, ".pi", "agent", "agents");
}

function defaultHome() {
  return process.env.HOME || process.env.USERPROFILE || ".";
}

/** Whether `name` is a lowercase kebab-case identifier. */
export function isKebabName(name) {
  return typeof name === "string" && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(name);
}

/**
 * Split a markdown agent file into frontmatter fields and body.
 *
 * Returns `{ fields, body, error }`. `error` is set when the file has no
 * frontmatter or no closing delimiter; callers skip such a file with a
 * diagnostic instead of throwing.
 */
export function parseAgentFile(text) {
  const source = String(text ?? "");
  const lines = source.split(/\r?\n/);
  if (lines[0]?.trim() !== "---") {
    return { fields: {}, body: source.trim(), error: "no frontmatter" };
  }
  let end = -1;
  for (let i = 1; i < lines.length; i += 1) {
    if (lines[i].trim() === "---") {
      end = i;
      break;
    }
  }
  if (end === -1) {
    return { fields: {}, body: source.trim(), error: "unterminated frontmatter" };
  }
  const header = lines.slice(1, end);
  const body = lines.slice(end + 1).join("\n").trim();
  return { fields: readFields(header), body, error: undefined };
}

/**
 * Read scalar and list fields from the frontmatter lines.
 *
 * Supports `key: value`, `key: [a, b]`, `key: a, b`, a quoted value, and a
 * block list (`key:` followed by `- item` lines). Comments and blank lines are
 * ignored; unknown keys are not returned.
 */
function readFields(lines) {
  const fields = {};
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const match = /^([A-Za-z][A-Za-z0-9_-]*)\s*:\s*(.*)$/.exec(line);
    i += 1;
    if (!match) continue;
    const [, key, rawValue] = match;
    const value = unquote(rawValue.trim());
    if (value === "" && i < lines.length && /^\s*-\s+/.test(lines[i])) {
      const items = [];
      while (i < lines.length && /^\s*-\s+/.test(lines[i])) {
        items.push(unquote(lines[i].replace(/^\s*-\s+/, "").trim()));
        i += 1;
      }
      fields[key] = items;
    } else {
      fields[key] = value;
    }
  }
  return fields;
}

function unquote(value) {
  if (value.length >= 2 && value.startsWith('"') && value.endsWith('"')) return value.slice(1, -1);
  if (value.length >= 2 && value.startsWith("'") && value.endsWith("'")) return value.slice(1, -1);
  return value;
}

/**
 * Normalise a `tools:` value into `{ tools, present }`.
 *
 * `present` distinguishes an absent key (pi's own defaults apply) from an
 * explicitly empty list (the child gets no tools). That difference decides the
 * ceiling: absent means pi's default set, which includes `bash`.
 */
export function parseTools(raw) {
  if (raw === undefined || raw === null) return { tools: null, present: false };
  if (Array.isArray(raw)) return { tools: raw.map(String).filter(Boolean), present: true };
  const text = String(raw).trim();
  if (text === "") return { tools: [], present: true };
  const inner =
    text.startsWith("[") && text.endsWith("]") ? text.slice(1, -1) : text;
  const tools = inner
    .split(",")
    .map((item) => unquote(item.trim()))
    .filter(Boolean);
  return { tools, present: true };
}

/** A single tool's risk. Anything unrecognised (MCP, extension tools) is `all`. */
export function classifyTool(name) {
  const tool = String(name ?? "").toLowerCase();
  if (READ_TOOLS.has(tool)) return "read";
  if (EDIT_TOOLS.has(tool)) return "edit";
  if (EXEC_TOOLS.has(tool)) return "exec";
  return "all";
}

/**
 * The maximum risk an agent may reach, derived from its `tools:` line.
 *
 * - absent  → `exec`, because pi's default set includes `bash`.
 * - empty   → `read`, since the child has no tools at all.
 * - unknown → `all`, because an MCP or extension tool can do anything.
 */
export function ceilingForTools(raw) {
  const { tools, present } = parseTools(raw);
  if (!present) return "exec";
  if (tools.length === 0) return "read";
  let ceiling = "read";
  for (const tool of tools) {
    const kind = classifyTool(tool);
    if (CEILING_RANK[kind] > CEILING_RANK[ceiling]) ceiling = kind;
  }
  return ceiling;
}

/** Build one agent record from a parsed file. `source` is `builtin` or `user`. */
function toAgent({ fields, body, error, source, filePath }) {
  const tools = parseTools(fields.tools);
  return {
    name: typeof fields.name === "string" ? fields.name : "",
    description: typeof fields.description === "string" ? fields.description : "",
    model: typeof fields.model === "string" && fields.model.trim() !== "" ? fields.model.trim() : null,
    thinking:
      typeof fields.thinking === "string" && fields.thinking.trim() !== ""
        ? fields.thinking.trim()
        : null,
    tools: tools.tools,
    toolsPresent: tools.present,
    ceiling: ceilingForTools(fields.tools),
    body,
    source,
    filePath: filePath ?? null,
    error,
  };
}

/** Read the builtins shipped beside this file. */
export function loadBuiltins(builtinsPath = new URL("./builtins.json", import.meta.url)) {
  const raw = JSON.parse(fs.readFileSync(builtinsPath, "utf8"));
  const list = Array.isArray(raw?.agents) ? raw.agents : [];
  return list
    .map((agent) =>
      toAgent({
        fields: {
          name: agent.name,
          description: agent.description,
          model: agent.model,
          thinking: agent.thinking,
          tools: agent.tools,
        },
        body: agent.prompt ?? "",
        source: "builtin",
      }),
    )
    .filter((agent) => agent.name !== "" && agent.description !== "");
}

/** Recursively list `*.md` files under `dir`, sorted for determinism. */
function listMarkdown(dir) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
  const found = [];
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) found.push(...listMarkdown(full));
    else if (entry.isFile() && entry.name.endsWith(".md")) found.push(full);
  }
  return found;
}

/**
 * Discover every agent, lowest priority first: builtins, then user files
 * (which override a builtin of the same name, the way `pi-subagents` does).
 *
 * A malformed or unnamed file is skipped and reported in `skipped`; it never
 * removes the other agents.
 */
export function discoverAgents({ builtins = [], userDir = userAgentDir() } = {}) {
  const byName = new Map();
  for (const agent of builtins) byName.set(agent.name, agent);

  const skipped = [];
  for (const filePath of listMarkdown(userDir)) {
    let text;
    try {
      text = fs.readFileSync(filePath, "utf8");
    } catch (error) {
      skipped.push({ filePath, error: String(error?.message ?? error) });
      continue;
    }
    const parsed = parseAgentFile(text);
    const agent = toAgent({ ...parsed, source: "user", filePath });
    if (parsed.error) {
      skipped.push({ filePath, error: parsed.error });
      continue;
    }
    if (!isKebabName(agent.name)) {
      skipped.push({ filePath, error: "name is not kebab-case" });
      continue;
    }
    if (agent.description === "") {
      skipped.push({ filePath, error: "description is required" });
      continue;
    }
    byName.set(agent.name, agent);
  }

  return {
    agents: [...byName.values()].sort((a, b) => a.name.localeCompare(b.name)),
    skipped,
  };
}

/** The tool description, built from the agents the model can actually pick. */
export function buildDescription(agents) {
  const lines = [
    "Delegate a scoped task to a subagent with its own context window.",
    "",
    "Agents (user scope, ~/.pi/agent/agents):",
  ];
  for (const agent of agents) {
    const capability = agent.ceiling === "read" ? "read only" : agent.ceiling === "edit" ? "edits files" : "runs commands";
    lines.push(`  ${agent.name.padEnd(10)} ${capability} · ${agent.description}`);
  }
  lines.push(
    "",
    "The capability shown is what the agent may do; do not ask an agent to exceed it.",
    "",
    "Modes: single {agent, task}; chain {chain:[…]} with {previous}; parallel {tasks:[…]}.",
    "The child sees only the task; it does not see this conversation. Put everything it needs",
    "in `task` (paths, what was decided, what to return). Returns a short report, not a transcript.",
    "",
    "Delegate when the work is broad (finding where something is wired, reviewing a diff,",
    "a second opinion) and skip it for one file you already know: a child starts cold and",
    "costs a process, so delegation pays when it saves more reading than it costs.",
  );
  return lines.join("\n");
}
