/**
 * Agent discovery for Orbit's subagent tool.
 *
 * Adapted from pi's `examples/extensions/subagent/agents.ts` (MIT, see NOTICE).
 * Unlike the upstream example this has no pi imports: it parses the small
 * frontmatter subset agent files need, so it stays testable under plain Node
 * and independent of pi's internal exports.
 *
 * Priority (lowest to highest): built-in (next to this file), user
 * (`~/.pi/agent/agents`), project (nearest `.pi/agents`). A later source
 * overrides an earlier one with the same runtime name.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** The built-in agents that ship beside this extension. */
export const BUILTIN_AGENTS_DIR = path.join(
  path.dirname(fileURLToPath(import.meta.url)),
  "agents",
);

/** The project config directory pi uses. */
const CONFIG_DIR_NAME = ".pi";

/** Strip matching single or double quotes from a scalar. */
function unquote(value) {
  const text = String(value ?? "").trim();
  if (text.length >= 2) {
    const first = text[0];
    const last = text[text.length - 1];
    if ((first === '"' && last === '"') || (first === "'" && last === "'")) {
      return text.slice(1, -1);
    }
  }
  return text;
}

/**
 * Normalize a frontmatter `tools` value.
 *
 * Both spellings are accepted:
 *   tools: read, grep, find
 *   tools: [read, grep, find]
 */
export function parseToolList(value) {
  if (Array.isArray(value)) {
    const tools = value
      .filter((tool) => typeof tool === "string")
      .map((tool) => tool.trim())
      .filter(Boolean);
    return tools.length > 0 ? tools : undefined;
  }
  if (typeof value !== "string") return undefined;
  let raw = value.trim();
  if (raw.startsWith("[") && raw.endsWith("]")) raw = raw.slice(1, -1);
  const tools = raw
    .split(",")
    .map((tool) => unquote(tool))
    .filter(Boolean);
  return tools.length > 0 ? tools : undefined;
}

/**
 * Parse one agent Markdown file.
 *
 * The supported frontmatter is deliberately small: scalar `name`,
 * `description`, `model`, and a scalar/list `tools`. A file missing `name` or
 * `description` is ignored rather than taking down every other agent.
 */
export function parseAgentFile(content, source, filePath) {
  if (typeof content !== "string") return null;
  const text = content.replace(/^\uFEFF/, "");
  if (!text.startsWith("---")) return null;
  const closing = text.indexOf("\n---", 3);
  if (closing === -1) return null;

  const raw = text.slice(3, closing);
  const body = text.slice(closing + 4).trim();
  const meta = {};
  const lines = raw.split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    const match = /^([A-Za-z0-9_-]+)\s*:\s*(.*)$/.exec(lines[i]);
    if (!match) continue;
    const [, key, value] = match;
    // YAML block list:
    //   tools:
    //     - read
    //     - grep
    if (key === "tools" && value.trim() === "") {
      const tools = [];
      while (i + 1 < lines.length) {
        const item = /^\s*-\s*(.*)$/.exec(lines[i + 1]);
        if (!item) break;
        tools.push(unquote(item[1]));
        i++;
      }
      meta.tools = tools;
      continue;
    }
    meta[key] = unquote(value);
  }

  const name = typeof meta.name === "string" ? meta.name.trim() : "";
  const description =
    typeof meta.description === "string" ? meta.description.trim() : "";
  if (!name || !description) return null;

  return {
    name,
    description,
    tools: parseToolList(meta.tools),
    model: typeof meta.model === "string" && meta.model ? meta.model : undefined,
    thinking: parseThinking(meta.thinking),
    systemPrompt: body,
    source,
    filePath,
  };
}

/** Valid pi thinking levels; an unknown value is ignored, never forwarded. */
const THINKING_LEVELS = new Set([
  "off",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
]);

export function parseThinking(value) {
  if (typeof value !== "string") return undefined;
  const level = unquote(value).toLowerCase();
  return THINKING_LEVELS.has(level) ? level : undefined;
}

function loadAgentsFromDir(dir, source) {
  const agents = [];
  if (!dir || !fs.existsSync(dir)) return agents;

  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch {
    return agents;
  }

  for (const entry of entries) {
    const filePath = path.join(dir, entry.name);
    // Nested directories are supported; symlinked files are read, symlinked
    // directories are not (never walk outside the agent tree).
    if (entry.isDirectory()) {
      agents.push(...loadAgentsFromDir(filePath, source));
      continue;
    }
    if (!entry.name.endsWith(".md")) continue;
    if (!entry.isFile() && !entry.isSymbolicLink()) continue;

    let content;
    try {
      content = fs.readFileSync(filePath, "utf-8");
    } catch {
      continue;
    }
    const agent = parseAgentFile(content, source, filePath);
    if (agent) agents.push(agent);
  }

  return agents;
}

function isDirectory(candidate) {
  try {
    return fs.statSync(candidate).isDirectory();
  } catch {
    return false;
  }
}

/** The nearest ancestor (or cwd) containing `.pi/agents`, if any. */
export function findNearestProjectAgentsDir(cwd) {
  let current;
  try {
    current = path.resolve(cwd || process.cwd());
  } catch {
    return null;
  }
  while (true) {
    const candidate = path.join(current, CONFIG_DIR_NAME, "agents");
    if (isDirectory(candidate)) return candidate;
    const parent = path.dirname(current);
    if (parent === current) return null;
    current = parent;
  }
}

/**
 * Discover agents for one invocation.
 *
 * `options` exists for tests: pass explicit directories instead of touching
 * the machine's real home/workspace. When `projectDir` is present but null,
 * project discovery is skipped.
 */
export function discoverAgents(cwd, scope = "both", options = {}) {
  const resolvedScope =
    scope === "user" || scope === "project" ? scope : "both";
  const builtinDir = options.builtinDir ?? BUILTIN_AGENTS_DIR;
  const userDir =
    options.userDir ?? path.join(os.homedir(), CONFIG_DIR_NAME, "agent", "agents");
  const projectDir = Object.prototype.hasOwnProperty.call(options, "projectDir")
    ? options.projectDir
    : findNearestProjectAgentsDir(cwd);

  const builtin = loadAgentsFromDir(builtinDir, "builtin");
  const user =
    resolvedScope === "project" ? [] : loadAgentsFromDir(userDir, "user");
  const project =
    resolvedScope === "user" || !projectDir
      ? []
      : loadAgentsFromDir(projectDir, "project");

  const byName = new Map();
  for (const agent of builtin) byName.set(agent.name, agent);
  for (const agent of user) byName.set(agent.name, agent);
  for (const agent of project) byName.set(agent.name, agent);

  const agents = [...byName.values()].sort((a, b) => a.name.localeCompare(b.name));
  return { agents, projectAgentsDir: projectDir };
}

/** A short, deterministic listing for tool descriptions and errors. */
export function formatAgentList(agents, maxItems = 12) {
  const listed = agents.slice(0, maxItems);
  const remaining = agents.length - listed.length;
  return {
    text:
      listed
        .map((agent) => `${agent.name} (${agent.source}): ${agent.description}`)
        .join("; ") || "none",
    remaining,
  };
}
