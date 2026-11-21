/**
 * Orbit workflow policy — pure, offline-testable decisions.
 *
 * A session runs in one of three workflow modes. This module answers, for a
 * given mode, which tools stay active, whether a tool call must be blocked,
 * and what prompt guidance to inject.
 *
 *   plan   read-only exploration; produce a numbered plan under a `Plan:` header
 *   build  normal agent work (default)
 *   ask    read-only question answering
 *
 * Plan and Ask disable the write tools, default-deny untrusted tools, and gate
 * bash to a read-only allowlist. This is a guard, not a sandbox — pi ships none,
 * and Orbit does not add one.
 *
 * The mode itself lives per session in `~/.orbit-pi/workflow.json` (a map of
 * pi session id → wire id) and is read fresh on every hook, so a change in the
 * app re-arms a live session without a restart.
 */

/** Fallback when nothing is stored, readable, or known. */
export const DEFAULT_MODE = "build";

/** Every mode the app can write. Kept in sync with `crate::workflow`. */
export const MODES = ["plan", "build", "ask"];

/** Custom session-entry type recording the mode (survives resume). */
export const WORKFLOW_ENTRY_TYPE = "orbit:workflow";

/** Custom message type carrying per-mode prompt guidance. */
export const CONTEXT_TYPE = "orbit-workflow-context";

/** Tools forced off per mode. Unknown names are ignored by pi. */
export const DISABLED_TOOLS = {
  plan: ["edit", "write"],
  ask: ["edit", "write"],
  build: [],
};

/** Pi's built-in tools that only observe the workspace. */
const READ_ONLY_TOOLS = new Set(["read", "grep", "find", "ls"]);

/** Tools that execute a process, where bash is gated by the allowlist. */
const EXEC_TOOLS = new Set(["bash", "powershell", "shell", "exec", "terminal"]);

/** Delegation is safe only because the child inherits this workflow policy. */
const DELEGATE_TOOLS = new Set(["subagent"]);

/** Whether a tool may stay active in Plan/Ask. Everything else is untrusted. */
function availableInReadOnlyMode(name) {
  return READ_ONLY_TOOLS.has(name) || EXEC_TOOLS.has(name) || DELEGATE_TOOLS.has(name);
}

/** Coerce an arbitrary value to a known mode, falling back to the default. */
export function normalizeMode(raw) {
  return typeof raw === "string" && MODES.includes(raw) ? raw : DEFAULT_MODE;
}

/** A readable label for a block reason: `plan` → `Plan`. */
function label(mode) {
  return normalizeMode(mode).replace(/^./, (c) => c.toUpperCase());
}

/**
 * The tools a mode leaves active. Build preserves pi's set; Plan/Ask subtract
 * the write tools and default-deny every tool outside the known read-only set.
 */
export function allowedTools(mode, baseTools = []) {
  const resolved = normalizeMode(mode);
  const disabled = new Set(DISABLED_TOOLS[resolved] ?? []);
  const seen = new Set();
  const out = [];
  for (const name of baseTools) {
    if (typeof name !== "string" || seen.has(name)) continue;
    const normalized = name.toLowerCase();
    if (disabled.has(normalized)) continue;
    // MCP and extension annotations cannot prove that a call is read-only.
    // Default-deny everything except known observers, gated shells, and the
    // subagent whose child is clamped by this same workflow mode.
    if (resolved !== "build" && !availableInReadOnlyMode(normalized)) continue;
    seen.add(name);
    out.push(name);
  }
  return out;
}

/**
 * The next active set for `mode`, derived read-modify-write from the tools
 * active *now* plus the names this extension previously removed.
 *
 * A session-start snapshot is not a safe base: other extensions activate
 * tools after `session_start` — the MCP extension turns on `codemode` (or
 * `tool_search`) when a server with that exposure connects — and a cached
 * snapshot would silently drop them on the next mode change.
 *
 * `restorable` names are this policy's own disabled tools that a *resumed*
 * session may re-activate. A restricted set outlives the process that applied
 * it: pi starts the next process with the tools the last one left active, so
 * `removedByUs` (per process) is empty while `edit`/`write` are simply gone.
 * Only names the caller has established are ours may ride here; the default
 * empty list keeps this function strictly "restore what I removed".
 *
 * Returns the new tool list and the updated set of names this extension has
 * removed, so switching to a mode that allows them restores exactly what it
 * took away (and never re-adds a tool another extension removed).
 */
export function applyToolScoping(mode, current = [], removedByUs = [], restorable = []) {
  const disabled = new Set(DISABLED_TOOLS[normalizeMode(mode)] ?? []);
  const removed = new Set(removedByUs);
  const restore = [];
  for (const name of removed) {
    if (disabled.has(name)) continue;
    restore.push(name);
    removed.delete(name);
  }
  for (const name of restorable) {
    if (disabled.has(name) || current.includes(name) || restore.includes(name)) continue;
    restore.push(name);
  }
  const base = [...new Set([...current, ...restore])];
  const tools = allowedTools(mode, base);
  for (const name of base) {
    if (!tools.includes(name)) removed.add(name);
  }
  return { tools, removed: [...removed] };
}

/**
 * Whether this session was scoped read-only by this extension at least once,
 * read from the `orbit:workflow` entries it appends as it applies a mode.
 *
 * That entry is the record of a restriction that may have outlived its
 * process: a resumed session can start with the write tools already gone, and
 * then there is nothing for a per-process memory to restore from. Once the
 * session says this extension took them away, Build may put the policy's own
 * disabled tools back without guessing at pi's defaults.
 */
export function wasScopedReadOnly(entries = []) {
  return (entries ?? []).some((entry) => {
    if (entry?.type !== "custom" || entry.customType !== WORKFLOW_ENTRY_TYPE) return false;
    const disabled = DISABLED_TOOLS[normalizeMode(entry.data?.mode)] ?? [];
    return disabled.length > 0;
  });
}

/** Every tool name this policy owns: what plan and ask force off. */
export function policyTools() {
  return [...new Set([...DISABLED_TOOLS.plan, ...DISABLED_TOOLS.ask])];
}

// Destructive shell operations. A conservative backstop to the allowlist: if
// any matches, the command is blocked even when it also looks safe.
const DESTRUCTIVE_PATTERNS = [
  /\brm\b/i,
  /\brmdir\b/i,
  /\bmv\b/i,
  /\bcp\b/i,
  /\bmkdir\b/i,
  /\btouch\b/i,
  /\bchmod\b/i,
  /\bchown\b/i,
  /\bln\b/i,
  /\btee\b/i,
  /\btruncate\b/i,
  /\bdd\b/i,
  /\bnpm\s+(install|uninstall|update|ci|link|publish)/i,
  /\byarn\s+(add|remove|install|publish)/i,
  /\bpnpm\s+(add|remove|install|publish)/i,
  /\bpip\s+(install|uninstall)/i,
  /\bbrew\s+(install|uninstall|upgrade)/i,
  /\bgit\s+(add|commit|push|pull|merge|rebase|reset|checkout|stash|cherry-pick|revert|tag|init|clone)/i,
  /\bgit\s+branch\s+-[dD]/i,
  /\bsudo\b/i,
  /\bsu\b/i,
  /\bkill\b/i,
  /\bpkill\b/i,
  /\bkillall\b/i,
  /\breboot\b/i,
  /\bshutdown\b/i,
  /\b(vim?|nano|emacs|code|subl)\b/i,
];

// Read-only commands allowed in plan/ask mode. Anchored at the start.
const SAFE_PATTERNS = [
  /^\s*cat\b/, /^\s*head\b/, /^\s*tail\b/,
  /^\s*grep\b/, /^\s*rg\b/, /^\s*find\b/, /^\s*fd\b/, /^\s*ls\b/, /^\s*pwd\b/,
  /^\s*tree\b/, /^\s*wc\b/, /^\s*stat\b/, /^\s*du\b/, /^\s*df\b/,
  /^\s*sort\b/, /^\s*uniq\b/, /^\s*diff\b/, /^\s*which\b/, /^\s*whereis\b/,
  /^\s*uname\b/, /^\s*whoami\b/, /^\s*id\b/, /^\s*date\b/, /^\s*uptime\b/,
  /^\s*ps\b/, /^\s*env\b/, /^\s*printenv\b/, /^\s*eza\b/, /^\s*jq\b/,
  /^\s*git\s+(status|log|diff|show|blame|rev-parse|ls-files|ls-tree|remote|config\s+--get)/i,
  /^\s*git\s+branch(?!\s+-[dD])/i,
  /^\s*npm\s+(list|ls|view|info|search|outdated|audit)/i,
  /^\s*yarn\s+(list|info|why|audit)/i,
  /^\s*node\s+--version/i,
  /^\s*python\d?\s+--version/i,
];

/** Split one simple shell command into argv. Quoting is honored; bad input fails closed. */
function shellWords(command) {
  const words = [];
  let word = "";
  let quote = null;
  let escaped = false;
  let started = false;
  for (const char of command) {
    if (escaped) {
      word += char;
      escaped = false;
      started = true;
      continue;
    }
    if (char === "\\" && quote !== "'") {
      escaped = true;
      started = true;
      continue;
    }
    if (quote !== null) {
      if (char === quote) quote = null;
      else word += char;
      started = true;
      continue;
    }
    if (char === "'" || char === '"') {
      quote = char;
      started = true;
      continue;
    }
    if (/\s/.test(char)) {
      if (started) words.push(word);
      word = "";
      started = false;
      continue;
    }
    word += char;
    started = true;
  }
  if (quote !== null || escaped) return null;
  if (started) words.push(word);
  return words;
}

/** `find` actions that delete, write a report, or execute another program. */
const FIND_MUTATING_ACTIONS = new Set([
  "-delete",
  "-exec",
  "-execdir",
  "-ok",
  "-okdir",
  "-fls",
  "-fprint",
  "-fprint0",
  "-fprintf",
]);

function readOnlyFind(args) {
  return !args.some((arg) => FIND_MUTATING_ACTIONS.has(arg));
}

/** ripgrep's preprocessor is an arbitrary command, even when its name is benign. */
function readOnlyRg(args) {
  for (const arg of args) {
    if (arg === "--") return true;
    if (arg === "--pre" || arg.startsWith("--pre=")) return false;
    if (arg === "--hostname-bin" || arg.startsWith("--hostname-bin=")) return false;
  }
  return true;
}

/** fd's immediate and batch exec forms, including bundled short flags. */
function readOnlyFd(args) {
  for (const arg of args) {
    if (arg === "--") return true;
    if (/^--exec(?:-batch)?(?:=|$)/.test(arg)) return false;
    if (/^-[^-]+/.test(arg) && /[xX]/.test(arg.slice(1))) return false;
  }
  return true;
}

function envAssignment(arg) {
  return /^[^=]+=/.test(arg);
}

/** env may print the environment, but may never reach a utility operand. */
function readOnlyEnv(args) {
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--") return args.slice(index + 1).every(envAssignment);
    if (arg.startsWith("--")) {
      if (arg === "--ignore-environment" || arg === "--null") continue;
      if (arg === "--unset") {
        index += 1;
        continue;
      }
      if (arg.startsWith("--unset=")) continue;
      // --split-string/--chdir/--argv0 and unknown future execution options
      // fail closed rather than being mistaken for printing options.
      return false;
    }
    if (arg.startsWith("-") && arg !== "-") {
      if (/^-[i0]+$/.test(arg)) continue;
      if (arg === "-u") {
        index += 1;
        continue;
      }
      if (/^-u.+/.test(arg)) continue;
      return false;
    }
    if (!envAssignment(arg)) return false;
  }
  return true;
}

/** Whether a GNU long option (including an unambiguous abbreviation) appears. */
function hasLongOption(args, longName) {
  for (const arg of args) {
    if (arg === "--") return false;
    if (!arg.startsWith("--")) continue;
    const option = arg.slice(2).split("=", 1)[0];
    if (option !== "" && longName.startsWith(option)) return true;
  }
  return false;
}

/** Whether an output option appears before `--` ends option parsing. */
function hasOutputOption(args, longName, shortName) {
  for (const arg of args) {
    if (arg === "--") return false;
    if (arg.startsWith("--")) {
      const option = arg.slice(2).split("=", 1)[0];
      // GNU long options may be abbreviated when unambiguous (`--out`).
      if (option !== "" && longName.startsWith(option)) return true;
    }
    // Utilities such as sort accept short-option bundles (`-ro output`).
    if (shortName && /^-[^-]+/.test(arg) && arg.slice(1).includes(shortName)) return true;
  }
  return false;
}

const GIT_BRANCH_MUTATING_OPTIONS = new Set([
  "--delete",
  "--move",
  "--copy",
  "--force",
  "--create-reflog",
  "--set-upstream-to",
  "--unset-upstream",
  "--edit-description",
  "--track",
  "--no-track",
  "--recurse-submodules",
]);

/** Options that make the remaining positional arguments filters, not a new branch name. */
const GIT_BRANCH_LIST_OPTIONS = new Set([
  "--list",
  "--show-current",
  "--contains",
  "--no-contains",
  "--merged",
  "--no-merged",
  "--points-at",
  "--sort",
  "--format",
]);

const GIT_BRANCH_READ_OPTIONS = new Set([
  ...GIT_BRANCH_LIST_OPTIONS,
  "--all",
  "--remotes",
  "--verbose",
  "--ignore-case",
  "--omit-empty",
  "--column",
  "--no-column",
  "--color",
  "--no-color",
  "--abbrev",
  "--no-abbrev",
]);

function readOnlyGitBranch(args) {
  let listing = false;
  let positional = false;
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--") {
      positional ||= index + 1 < args.length;
      break;
    }
    if (arg.startsWith("--")) {
      const option = arg.split("=", 1)[0];
      if (GIT_BRANCH_MUTATING_OPTIONS.has(option)) return false;
      if (!GIT_BRANCH_READ_OPTIONS.has(option)) return false;
      listing ||= GIT_BRANCH_LIST_OPTIONS.has(option);
      continue;
    }
    if (arg.startsWith("-") && arg !== "-") {
      // The read-only short flags are -a, -r and -v (including -vv). Every
      // creation/deletion/upstream flag and every unknown flag fails closed.
      if (!/^-[arv]+$/.test(arg)) return false;
      continue;
    }
    positional = true;
  }
  return listing || !positional;
}

function readOnlyGitRemote(args) {
  if (args.length === 0) return true;
  if (args.every((arg) => arg === "-v" || arg === "--verbose")) return true;
  // These subcommands inspect configuration or the remote without changing it.
  if (args[0] === "show") {
    return !args.slice(1).some((arg) => arg === "-p" || arg === "--prune");
  }
  if (args[0] === "get-url") return true;
  // Prune is allowed only in its explicitly non-mutating form.
  if (args[0] === "prune") {
    return args.slice(1).some((arg) => arg === "-n" || arg === "--dry-run");
  }
  // add/remove/rename/set-*/update and unknown future subcommands fail closed.
  return false;
}

function readOnlyGit(args) {
  const [rawSubcommand, ...rest] = args;
  const subcommand = rawSubcommand?.toLowerCase();
  if (subcommand === "branch") return readOnlyGitBranch(rest);
  if (subcommand === "remote") return readOnlyGitRemote(rest);
  // Diff output files and external diff programs are not read-only.
  return !rest.some(
    (arg) =>
      arg === "--output" ||
      arg.startsWith("--output=") ||
      arg === "--ext-diff" ||
      arg === "--textconv",
  );
}

function readOnlyNpm(args) {
  if (args[0]?.toLowerCase() !== "audit") return true;
  return !args.slice(1).some((arg) => {
    const option = arg.toLowerCase();
    return option === "fix" || option === "--fix" || option.startsWith("--fix=");
  });
}

/**
 * Whether a bash command is safe under plan/ask mode.
 *
 * Rejects compound commands (`;`, `&&`, `||`, `|`), redirects (`>`, `<`),
 * command substitution (`$(`, backticks), subshells, and newlines outright —
 * a false *allow* is worse than a false block. The command must then match the
 * read-only allowlist, no destructive pattern, and the parsed per-utility
 * policy. The last step catches write/execute options and subcommands that a
 * broad command-name allowlist cannot (`rg --pre`, `fd -x`, `env sh`,
 * `find -exec`, `sort -o`, `npm audit fix`).
 */
export function isSafeCommand(command) {
  const cmd = String(command ?? "");
  if (cmd.trim() === "") return false;
  if (/[;&|`<>\n]|\$\(|[()]/.test(cmd)) return false;
  // ANSI-C and locale-aware quotes can hide option bytes from the parser.
  if (cmd.includes("$'") || cmd.includes('$"')) return false;
  if (DESTRUCTIVE_PATTERNS.some((re) => re.test(cmd))) return false;
  if (!SAFE_PATTERNS.some((re) => re.test(cmd))) return false;
  const words = shellWords(cmd);
  if (!words || words.length === 0) return false;
  const [rawUtility, ...args] = words;
  const utility = rawUtility.toLowerCase();
  if (utility === "find") return readOnlyFind(args);
  if (utility === "rg") return readOnlyRg(args);
  if (utility === "fd") return readOnlyFd(args);
  if (utility === "env") return readOnlyEnv(args);
  if (utility === "sort") {
    return !hasOutputOption(args, "output", "o") && !hasLongOption(args, "compress-program");
  }
  if (utility === "tree") return !hasOutputOption(args, "output", "o");
  if (utility === "git") return readOnlyGit(args);
  if (utility === "npm") return readOnlyNpm(args);
  return true;
}

/**
 * Why a tool call is blocked under `mode`, or `undefined` when it may run.
 * `build` never blocks; plan/ask block write tools, unsafe bash, and every
 * MCP/custom/orchestration tool except the workflow-clamped subagent.
 */
export function blockReason(mode, toolName, input) {
  const resolved = normalizeMode(mode);
  if (resolved === "build") return undefined;

  const name = String(toolName ?? "").toLowerCase();
  if ((DISABLED_TOOLS[resolved] ?? []).includes(name)) {
    return `${label(resolved)} mode: ${name} is disabled. Switch to Build to make changes.`;
  }
  if (EXEC_TOOLS.has(name)) {
    const command = input?.command ?? input?.cmd ?? "";
    if (!isSafeCommand(command)) {
      return `${label(resolved)} mode: command blocked (not on the read-only allowlist). Switch to Build to run it.`;
    }
    return undefined;
  }
  if (availableInReadOnlyMode(name)) return undefined;
  return `${label(resolved)} mode: ${name || "tool"} is blocked. Only read-only built-ins, safe shell commands, and guarded subagents are available; switch to Build for MCP, custom, or other orchestration tools.`;
}

/**
 * The system-prompt addendum for a mode, or `""` when there is nothing to add
 * (Build keeps pi's own prompt untouched).
 */
export function guidance(mode) {
  const resolved = normalizeMode(mode);
  if (resolved === "plan") {
    return [
      "[WORKFLOW: PLAN MODE]",
      "You are in plan mode — read-only exploration for safe code analysis.",
      "Do not make changes: write tools are disabled and shell commands are limited to a read-only allowlist.",
      "Investigate, then present a numbered plan under a `Plan:` header, one line per step, e.g.",
      "Plan:",
      "1. First step",
      "2. Second step",
    ].join("\n");
  }
  if (resolved === "ask") {
    return [
      "[WORKFLOW: ASK MODE]",
      "Answer the user's question from the codebase. Do not modify anything — write tools are disabled and shell commands are limited to a read-only allowlist.",
      "Do not produce an implementation plan unless the user asks for one.",
    ].join("\n");
  }
  return "";
}
