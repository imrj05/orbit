/**
 * Orbit guard policy — pure, offline-testable access decisions.
 *
 * Orbit runs with one of three access modes. For every tool call pi emits on
 * its `tool_call` hook, this module answers whether the call is allowed
 * outright, must be confirmed by the user, or is denied:
 *
 *   supervised        read/net → allow · edit/exec/other → ask
 *   auto-accept-edits read/net/edit → allow · exec/other → ask
 *   full-access       allow everything (no prompts)
 *
 * Two things narrow those defaults, and both are read fresh on every call so a
 * change in Orbit re-arms live sessions without a restart:
 *
 * - `~/.orbit-pi/access-allow.json` — the user's "Always allow this tool"
 *   grants, per mode (written by this extension's prompt).
 * - `~/.orbit-pi/access-rules.json` — Orbit-authored rules: scope (every
 *   session / this session / one subagent), tool family, argument pattern and
 *   action (allow / ask / deny). That is the per-command and per-agent
 *   granularity a mode cannot express: "`bash` may run, but `rm` asks".
 *
 * Precedence: **Full access short-circuits every rule** — it is Orbit's "this
 * session may do anything" switch, so nothing asks and nothing is refused
 * there. In the confined modes the scopes are resolved most specific first (a
 * subagent's own rules, then main-session rules, then `any`); the first scope
 * with a matching rule decides (within it deny > ask > allow), and with no
 * matching rule the mode default applies. A rule can never exceed a child's
 * ceiling, which is clamped before any of this runs.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

/** Fallback when the file is missing, unreadable, or names an unknown mode.
 *
 * The strictest mode, deliberately. The guard is a confirmation boundary, so
 * every failure to establish a mode has to fall back to *asking*, never to
 * allowing. A first launch before Orbit has written the file, a half-written
 * file after a crash, a read error — each one must degrade to prompts, not to
 * silent tool execution. */
export const DEFAULT_MODE = "supervised";

/** Every mode Orbit can write. Kept in sync with `crate::access::AccessMode`. */
export const MODES = ["supervised", "auto-accept-edits", "full-access"];

/**
 * Marker Orbit's dialog router looks for: a `select` whose title carries it is
 * rendered as the inline access-guard bar instead of the blocking modal. The
 * title is `PREFIX + tool + "\t" + detail`. Kept in sync with
 * `crate::dialog::GUARD_TITLE_PREFIX`.
 */
export const GUARD_TITLE_PREFIX = "[orbit-guard] ";

/** Options offered for a confirmation-required call. */
export const OPTION_ALLOW_ONCE = "Allow once";
export const OPTION_ALWAYS_ALLOW = "Always allow this tool";
export const OPTION_DENY = "Deny";

/** The option list, in display order. */
export const CONFIRM_OPTIONS = [OPTION_ALLOW_ONCE, OPTION_ALWAYS_ALLOW, OPTION_DENY];

/**
 * The tool families a rule may name. They are exactly [`classify`]'s kinds
 * plus `"any"`, so a rule can never target something the decision table does
 * not understand. Kept in sync with `crate::permissions::Target`.
 */
export const RULE_TARGETS = [
  "read",
  "edit",
  "exec",
  "net",
  "mcp",
  "delegate",
  "other",
  "any",
];

/** What a rule does to a matching call. */
export const RULE_ACTIONS = ["allow", "ask", "deny"];

/** A pattern is the user's glob; it never needs to be longer than this. */
export const MAX_PATTERN = 200;

/** Hard cap on stored rules, so the file and the per-call scan stay bounded. */
export const MAX_RULES = 200;

/** Tools that only observe: they never mutate the workspace. */
const READ_TOOLS = new Set(["read", "grep", "find", "ls", "glob", "list"]);

/**
 * Tools that reach the network. They are their own kind for one reason: a
 * fetch sends workspace text somewhere else, so it may deserve a prompt even
 * though it mutates nothing locally. The mode defaults allow them exactly as
 * before — a rule is the only way to gate one — so classifying them apart
 * changes no existing behavior.
 */
const NET_TOOLS = new Set(["webfetch", "websearch"]);

/** Tools that write files. */
const EDIT_TOOLS = new Set(["edit", "write", "multiedit", "apply_patch", "str_replace_editor"]);

/** Tools that run a process / shell command. */
const EXEC_TOOLS = new Set(["bash", "powershell", "shell", "exec", "terminal"]);

/**
 * Pi names every MCP tool `mcp__<server>__<tool>`. They are treated as their
 * own kind so the confirmation can name the server, and they are always asked
 * about in the confined modes: an MCP call can run a local process or change
 * an external service, so nothing about it is auto-approved except through
 * the user's explicit "Always allow this tool" entry.
 */
const MCP_PREFIX = "mcp__";

/**
 * The delegation tool. It is its own kind because it is the one tool whose
 * *effect* lives in another session: the child inherits this session's access
 * and workflow modes, so a delegation can never exceed what this session may
 * already do. A confirmation would approve nothing (D-Z), so it is allowed;
 * the subagent extension refuses runs the mode could not finish, before it
 * spawns anything.
 */
const DELEGATE_TOOL = "subagent";

/** A child's maximum risk, as the subagent extension derives it. */
const CEILING_RANK = { read: 1, edit: 2, exec: 3, all: 4 };

/** What a tool needs from the ceiling. `mcp`/`other` need `all`. */
const TOOL_RANK = { read: 1, net: 1, edit: 2, exec: 3, mcp: 4, other: 4, delegate: 4 };

/**
 * Coerce an arbitrary value to a known mode, falling back to
 * [`DEFAULT_MODE`]. Unknown values never allow more than intended.
 */
export function normalizeMode(raw) {
  return typeof raw === "string" && MODES.includes(raw) ? raw : DEFAULT_MODE;
}

/**
 * Classify a pi tool name into a risk kind: `"read"`, `"net"`, `"edit"`,
 * `"exec"`, `"mcp"`, `"delegate"`, or `"other"` (extension / custom tools,
 * which are treated conservatively).
 */
export function classify(toolName) {
  const name = String(toolName ?? "").toLowerCase();
  if (name === DELEGATE_TOOL) return "delegate";
  if (READ_TOOLS.has(name)) return "read";
  if (NET_TOOLS.has(name)) return "net";
  if (EDIT_TOOLS.has(name)) return "edit";
  if (EXEC_TOOLS.has(name)) return "exec";
  if (name.startsWith(MCP_PREFIX)) return "mcp";
  return "other";
}

/**
 * The mode's default decision for a tool kind. Settings → Permissions renders
 * the same table from `crate::permissions::Target::default_action`; the two
 * mirror each other, and both have tests asserting their own copy.
 */
export function defaultAction(mode, kind) {
  // A delegation is allowed in every mode: its child is clamped by that same
  // mode (a child has no UI, so anything that would prompt is denied), and the
  // subagent extension refuses a run that cannot finish before it spends
  // tokens. Only a deny rule can stop one.
  if (kind === "delegate") return "allow";
  const resolved = normalizeMode(mode);
  if (resolved === "full-access") return "allow";
  // Reads and fetches are non-mutating, so every confined mode allows them.
  if (kind === "read" || kind === "net") return "allow";
  if (resolved === "auto-accept-edits" && kind === "edit") return "allow";
  return "ask";
}

/**
 * The decision for one tool call: `"allow"`, `"ask"`, or `"deny"`. There is
 * intentionally no silent failure mode — the caller treats a failed prompt as a
 * denial (fail-safe).
 *
 * `"Always allow this tool"` is a *tool-wide* grant, so it is only honoured for
 * tools whose risk does not depend on the arguments: a `read`, an `edit`. It is
 * never honoured for `exec` or `mcp`, where the same tool name covers everything
 * from `ls` to `rm -rf ~` — approving one command must not approve the next.
 * [`grantable`] is the one place that rule lives; the UI asks it before it
 * offers the button. Narrow, per-command grants are what `rules` are for.
 */
export function decide(mode, toolName, allowlist = [], options = {}) {
  const name = String(toolName ?? "");
  const kind = classify(name);
  // Full access is answered before any rule. Its whole meaning is "allow
  // everything, ask nothing": an Ask rule must not turn it back into a prompt,
  // and a Deny rule must not block a mode the user chose to trust. Rules are
  // consulted only where the mode leaves room for them — see `decide`'s doc.
  if (normalizeMode(mode) === "full-access") return "allow";
  // Otherwise a rule decides before anything else, including the delegation
  // allowance: a user who denies `subagent` for one agent means it.
  const ruled = ruleDecision(
    options.rules ?? [],
    options.agent ?? null,
    name,
    kind,
    options.input ?? null,
  );
  if (ruled !== null) return ruled;
  if (kind === "delegate") return "allow";
  if (grantable(name) && allowlist.includes(name)) return "allow";
  return defaultAction(mode, kind);
}

/**
 * Whether "Always allow this tool" may be offered for `toolName`, and whether a
 * stored grant for it counts. False for the kinds whose meaning changes with
 * every call: one `bash` approval would otherwise cover any future command,
 * which is exactly the boundary the guard exists to hold.
 */
export function grantable(toolName) {
  const kind = classify(toolName);
  return kind !== "exec" && kind !== "mcp" && kind !== "other" && kind !== "delegate";
}

// ── permission rules ────────────────────────────────────────────────────
//
// A rule is `{ scope, target, pattern, action }`:
//
//   scope    "any" (every session) · "session" (main sessions only) ·
//            "agent:<name>" (one subagent)
//   target   a tool family from `RULE_TARGETS`, or "any"
//   pattern  a glob (`*` = any run of characters) matched against the call's
//            salient text — see [`matchText`]. Empty matches every call.
//   action   "allow" | "ask" | "deny"

/**
 * The glob a rule's pattern uses, matched against the whole text. `*` is the
 * only metacharacter, so a user cannot write a regex that behaves differently
 * from what the Settings page shows. An empty pattern matches everything.
 */
export function patternMatches(pattern, text) {
  const glob = String(pattern ?? "").trim();
  if (glob === "") return true;
  const source = String(text ?? "");
  const parts = glob.split("*").map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
  try {
    return new RegExp(`^${parts.join(".*")}$`, "s").test(source);
  } catch {
    // A pattern the engine cannot compile never matches, which falls through
    // to the mode default (asking) rather than silently allowing.
    return false;
  }
}

/**
 * The text a rule's pattern is tested against. It is exactly what the approval
 * bar shows, so "what I read is what the rule matches" holds:
 *
 *   exec      the command
 *   file      the path (falling back to the search pattern)
 *   net       the URL or query
 *   mcp/other the tool name and its arguments
 *   delegate  the task text
 */
export function matchText(toolName, input) {
  const kind = classify(toolName);
  const value = input ?? {};
  const first = (...keys) => {
    for (const key of keys) {
      if (typeof value[key] === "string" && value[key].trim() !== "") return value[key];
    }
    return null;
  };
  const fallback = () => summarize(toolName, input);
  if (kind === "exec") return first("command", "cmd", "script") ?? fallback();
  if (kind === "edit" || kind === "read") {
    return first("path", "file_path", "filename", "pattern", "query") ?? fallback();
  }
  if (kind === "net") return first("url", "query") ?? fallback();
  if (kind === "delegate") return first("task", "description", "prompt") ?? fallback();
  return `${toolName} ${fallback()}`;
}

/**
 * The segments of a shell command, split on the operators that chain one
 * command into another. A `deny`/`ask` rule is tested against each segment as
 * well as the whole line, so `cd x && rm -rf y` cannot slip past `rm *`.
 */
export function commandSegments(command) {
  return String(command ?? "")
    .split(/&&|\|\||;|\||\n|\r/)
    .map((segment) => segment.trim())
    .filter(Boolean);
}

/**
 * Whether a command does more than run one program: chaining, piping,
 * redirecting, backgrounding or substituting. An `allow` rule never applies to
 * one of these, because its pattern described the whole line and only the
 * first command is checked by eye. `deny`/`ask` rules still apply.
 */
export function isCompoundCommand(command) {
  return /[&|;<>`\n\r]|\$\(/.test(String(command ?? ""));
}

/** Coerce a stored scope, or `null` when it is not one Orbit writes. */
export function normalizeScope(raw) {
  if (raw === "any" || raw === "session") return raw;
  if (typeof raw === "string" && raw.startsWith("agent:")) {
    const name = raw.slice("agent:".length).trim();
    return name === "" ? null : `agent:${name}`;
  }
  return null;
}

/**
 * The scopes that apply to a call, most specific first. A child session knows
 * its agent from `ORBIT_SUBAGENT_AGENT`; a main session has none, so
 * `"session"` is its own scope and `"agent:*"` rules never apply to it.
 */
export function scopesFor(agent) {
  const name = typeof agent === "string" ? agent.trim() : "";
  return name === "" ? ["session", "any"] : [`agent:${name}`, "any"];
}

/**
 * The action a rule set decides for one call, or `null` when no rule matches.
 * The most specific scope with a matching rule wins; inside it, deny beats ask
 * beats allow, so a broad `allow` can never override a narrower `deny` written
 * in the same scope.
 */
export function ruleDecision(rules, agent, toolName, kind, input) {
  if (!Array.isArray(rules) || rules.length === 0) return null;
  const text = matchText(toolName, input);
  const command = kind === "exec" ? text : null;
  const segments = command === null ? [] : commandSegments(command);
  for (const scope of scopesFor(agent)) {
    const matched = [];
    for (const rule of rules) {
      const target = rule?.target;
      // A rule whose shape this module does not understand is inert: acting on
      // half of it would turn a typo into a permission grant.
      if (!RULE_TARGETS.includes(target) || !RULE_ACTIONS.includes(rule?.action)) continue;
      if (target !== "any" && target !== kind) continue;
      if (normalizeScope(rule?.scope) !== scope) continue;
      const pattern = rule?.pattern ?? "";
      const hit = patternMatches(pattern, text) || segments.some((segment) => patternMatches(pattern, segment));
      if (!hit) continue;
      // An `allow` rule is a statement about one command line; never let it
      // cover a chain whose later commands the pattern did not describe.
      if (rule.action === "allow" && kind === "exec" && isCompoundCommand(command)) continue;
      matched.push(rule.action);
    }
    if (matched.length === 0) continue;
    if (matched.includes("deny")) return "deny";
    if (matched.includes("ask")) return "ask";
    return "allow";
  }
  return null;
}

/** One stored rule, or `null` when it is not a shape Orbit writes. */
function normalizeRule(raw) {
  if (!raw || typeof raw !== "object") return null;
  const scope = normalizeScope(raw.scope);
  const target = RULE_TARGETS.includes(raw.target) ? raw.target : null;
  const action = RULE_ACTIONS.includes(raw.action) ? raw.action : null;
  if (scope === null || target === null || action === null ||
      typeof raw.pattern !== "string" || [...raw.pattern].length > MAX_PATTERN) return null;
  const pattern = raw.pattern;
  return { scope, target, pattern, action };
}

/** Return rules, or `null` when an existing store cannot be trusted. */
export function readRules(filePath = rulesFilePath()) {
  let raw;
  try {
    raw = fs.readFileSync(filePath, "utf8");
  } catch (error) {
    return error?.code === "ENOENT" ? [] : null;
  }
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed?.rules) || parsed.rules.length > MAX_RULES) return null;
    const rules = parsed.rules.map(normalizeRule);
    return rules.some((rule) => rule === null) ? null : rules;
  } catch {
    return null;
  }
}

/** Coerce a ceiling value; `null` means "no ceiling" (the parent session). */
export function normalizeCeiling(raw) {
  return typeof raw === "string" && Object.hasOwn(CEILING_RANK, raw) ? raw : null;
}

/**
 * Whether a child with `ceiling` may call `toolName` at all.
 *
 * This is the second enforcement of the agent's declared tool list: `--tools`
 * decides what the child is *offered*, and this decides what it may *run*, so
 * an MCP tool reached through `codemode`, or a tool another extension loaded
 * into the child, still cannot exceed the agent's tier. A missing ceiling
 * behaves exactly as before ceilings existed.
 */
export function ceilingAllows(ceiling, toolName) {
  const resolved = normalizeCeiling(ceiling);
  if (resolved === null) return true;
  const needed = TOOL_RANK[classify(toolName)] ?? TOOL_RANK.other;
  return needed <= CEILING_RANK[resolved];
}

/** A block reason naming the agent, its tier and the refused tool. */
export function ceilingBlockReason(ceiling, toolName, agent = "") {
  const resolved = normalizeCeiling(ceiling) ?? "all";
  const who = agent === "" ? "This subagent" : `Subagent ${agent}`;
  if (resolved === "read") return `Blocked: ${who} is read-only and cannot use ${toolName}`;
  if (resolved === "edit") return `Blocked: ${who} may edit files but cannot use ${toolName}`;
  return `Blocked: ${who} may run commands but may not use ${toolName}`;
}

/** The path Orbit writes the active mode to. */
export function modeFilePath() {
  const home = process.env.HOME || process.env.USERPROFILE || os.homedir() || ".";
  return path.join(home, ".orbit-pi", "access.json");
}

/** The path the "always allow" decisions live in. */
export function allowlistPath() {
  const home = process.env.HOME || process.env.USERPROFILE || os.homedir() || ".";
  return path.join(home, ".orbit-pi", "access-allow.json");
}

/** The path Settings → Permissions writes the rules to. */
export function rulesFilePath() {
  const home = process.env.HOME || process.env.USERPROFILE || os.homedir() || ".";
  return path.join(home, ".orbit-pi", "access-rules.json");
}

/**
 * Read the active mode from disk. Read fresh on every call so a change in the
 * app applies to a running session; any failure degrades to
 * [`DEFAULT_MODE`].
 */
export function readMode(filePath = modeFilePath()) {
  try {
    const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
    return normalizeMode(parsed?.mode);
  } catch {
    return DEFAULT_MODE;
  }
}

/** Tool names the user chose "Always allow" for under `mode`. */
export function readAllowlist(mode, filePath = allowlistPath()) {
  try {
    const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
    const list = parsed?.[normalizeMode(mode)];
    return Array.isArray(list) ? list.filter((name) => typeof name === "string") : [];
  } catch {
    return [];
  }
}

/** Record `toolName` as always-allowed under `mode`. Best-effort and atomic:
 * the file is written beside the target and renamed into place, so a crash
 * cannot leave half a JSON document for the guard to trip over. */
export function addToAllowlist(mode, toolName, filePath = allowlistPath()) {
  // Refuse to record a grant the decision table would never honour (an `exec`
  // tool, or an extension's custom tool) — writing it would only be misleading.
  if (!grantable(toolName)) return false;
  const resolved = normalizeMode(mode);
  let all = {};
  try {
    all = JSON.parse(fs.readFileSync(filePath, "utf8")) ?? {};
  } catch {
    all = {};
  }
  const list = new Set(Array.isArray(all[resolved]) ? all[resolved] : []);
  list.add(String(toolName));
  all[resolved] = [...list];
  const temporary = `${filePath}.${process.pid}.tmp`;
  try {
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(temporary, JSON.stringify(all), { mode: 0o600 });
    fs.renameSync(temporary, filePath);
    return true;
  } catch {
    // Best-effort: a failed write just means the prompt appears again.
    try {
      fs.unlinkSync(temporary);
    } catch {
      // Nothing to clean up.
    }
    return false;
  }
}

/** The confirmation options to offer for `toolName`, in display order. */
export function confirmOptions(toolName) {
  return grantable(toolName)
    ? CONFIRM_OPTIONS
    : CONFIRM_OPTIONS.filter((option) => option !== OPTION_ALWAYS_ALLOW);
}

/** Build the guard title Orbit recognizes: `PREFIX + tool + "\t" + detail`. */
export function formatTitle(toolName, detail) {
  return `${GUARD_TITLE_PREFIX}${toolName}\t${detail}`;
}

/** A short, single-line summary of a tool call's salient argument. */
export function summarize(toolName, input) {
  const value = input ?? {};
  const first = (...keys) => {
    for (const key of keys) {
      if (typeof value[key] === "string" && value[key].trim() !== "") return value[key];
    }
    return null;
  };
  const detail =
    first("command", "path", "file_path", "filename", "pattern", "url", "query") ??
    JSON.stringify(value);
  const text = String(detail ?? "").replace(/\s+/g, " ").trim();
  return text.length > 200 ? `${text.slice(0, 200)}…` : text;
}
