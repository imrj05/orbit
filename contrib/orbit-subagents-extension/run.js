/**
 * The child process contract: argv, environment, prompt, and the fold from
 * pi's JSONL stream into the card's state. Pure functions only — spawning and
 * timers live in `index.js`, so all of this is unit-testable offline.
 *
 * The stream rules come from pi's `docs/json.md`: strict JSONL split on LF
 * (never `readline`, which also splits on U+2028/U+2029 that are legal inside
 * JSON strings), `message_end` is authoritative, and `agent_settled` is the
 * only real idle signal.
 */

import { StringDecoder } from "node:string_decoder";

/** The `details` version Orbit renders. */
export const DETAILS_VERSION = "subagent/1";

/** Fixed prompt blocks (D-Y). Cheap, constant, and prefix-stable. */
export const SCOPE_BLOCK =
  "Your permission scope was fixed when you started and cannot be widened from inside this " +
  "session; operations that need approval are rejected automatically. When the job needs access " +
  "beyond that scope, do not retry the denied operation — state the limitation in your reply so " +
  "the delegating agent can handle it.";

export const GUIDANCE_BLOCK =
  "Your final answer is delivered to your parent. Include the result and the evidence in that " +
  "answer: the parent does not receive your transcript, tool output, or reasoning.";

/** Bounds, applied before anything crosses the pipe (§6.2). */
export const BOUNDS = {
  steps: 8,
  preview: 160,
  report: 64 * 1024,
  contentReport: 4 * 1024,
  stderr: 2 * 1024,
  task: 2 * 1024,
  blocked: 5,
  toolCounts: 12,
};

/** Caps the extension enforces regardless of configuration (D-I). */
export const LIMITS = { tasksPerRun: 8, concurrent: 4, childrenPerSession: 24, depth: 1 };

/** The smallest prompt a child can be given while still following D-Y. */
export function assemblePrompt({ body, scope = SCOPE_BLOCK, guidance = GUIDANCE_BLOCK } = {}) {
  return [String(body ?? "").trim(), scope, guidance].filter((part) => part !== "").join("\n\n");
}

/**
 * Build the child's argv. One pure function so the tier, the ceiling and the
 * model can be asserted together in tests.
 */
export function buildArgv({
  promptPath,
  prompt,
  extensions = [],
  model = null,
  thinking = null,
  tools = null,
  toolsPresent = false,
  ceiling = "exec",
}) {
  const argv = ["--mode", "json", "-p", "--no-session"];
  if (model) argv.push("--model", model);
  if (thinking) argv.push("--thinking", thinking);
  if (toolsPresent) {
    if (tools.length === 0) argv.push("--no-tools");
    else argv.push("--tools", tools.join(","));
  }
  // `--tools` narrows what is declared, not what MCP can reach through
  // codemode; the ceiling is the real clamp, and this closes the declaration.
  if (ceiling === "read" || ceiling === "edit") argv.push("--exclude-tools", "mcp__*");
  for (const extension of extensions) argv.push("--extension", extension);
  argv.push("--approve");
  if (promptPath) argv.push("--append-system-prompt", promptPath);
  argv.push("--", prompt);
  return argv;
}

/** The extra environment a child needs to inherit the parent's guardrails. */
export function buildEnv({
  parentSessionId = "",
  agentName = "",
  ceiling = "exec",
  workflowMode = "build",
} = {}) {
  return {
    PI_SKIP_VERSION_CHECK: "1",
    CI: "1",
    NO_COLOR: "1",
    ORBIT_WORKFLOW_MODE: workflowMode,
    ORBIT_SUBAGENT_CEILING: ceiling,
    ORBIT_SUBAGENT_AGENT: agentName,
    ORBIT_SUBAGENT_DEPTH: String(LIMITS.depth),
    ORBIT_SUBAGENT_PARENT: parentSessionId,
  };
}

/**
 * Decode a byte stream into JSONL records: split on LF over a UTF-8 decoder,
 * strip one trailing CR, and never treat U+2028/U+2029 as a boundary.
 */
export class LineDecoder {
  constructor() {
    this.decoder = new StringDecoder("utf8");
    this.buffer = "";
  }

  /** Feed one chunk; returns every complete record it completed. */
  push(chunk) {
    this.buffer += typeof chunk === "string" ? chunk : this.decoder.write(chunk);
    const lines = [];
    let index = this.buffer.indexOf("\n");
    while (index !== -1) {
      let line = this.buffer.slice(0, index);
      this.buffer = this.buffer.slice(index + 1);
      if (line.endsWith("\r")) line = line.slice(0, -1);
      lines.push(line);
      index = this.buffer.indexOf("\n");
    }
    return lines;
  }

  /** Flush whatever is left (a stream that ended without a final LF). */
  end() {
    const rest = this.buffer + this.decoder.end();
    this.buffer = "";
    return rest === "" ? [] : [rest.endsWith("\r") ? rest.slice(0, -1) : rest];
  }
}

/** Parse one record; `null` for a blank or unparsable line. */
export function parseRecord(line) {
  const text = String(line ?? "").trim();
  if (text === "") return null;
  try {
    const value = JSON.parse(text);
    return value && typeof value === "object" ? value : null;
  } catch {
    return null;
  }
}

/** A single-line argument preview for the card, capped at BOUNDS.preview. */
export function previewArgs(args) {
  const value = args ?? {};
  const keys = ["command", "path", "file_path", "filename", "pattern", "url", "query"];
  for (const key of keys) {
    if (typeof value[key] === "string" && value[key].trim() !== "") {
      return truncate(value[key].replace(/\s+/g, " ").trim(), BOUNDS.preview);
    }
  }
  try {
    return truncate(JSON.stringify(value).replace(/\s+/g, " "), BOUNDS.preview);
  } catch {
    return "";
  }
}

/** `grep /auth/ in src/` — the row's middle slot. */
export function describeTool(name, args) {
  const preview = previewArgs(args);
  return preview === "" ? String(name) : `${name} ${preview}`;
}

/** Truncate to `max` characters with an ellipsis. */
export function truncate(text, max) {
  const value = String(text ?? "");
  return value.length <= max ? value : `${value.slice(0, max - 1)}…`;
}

/** Truncate to `max` bytes (UTF-8), with an explicit marker. */
export function capBytes(text, max) {
  const value = String(text ?? "");
  const size = Buffer.byteLength(value, "utf8");
  if (size <= max) return { text: value, truncated: 0 };
  // Binary search the largest character prefix that fits `max` bytes.
  let low = 0;
  let high = value.length;
  while (low < high) {
    const middle = Math.ceil((low + high) / 2);
    if (Buffer.byteLength(value.slice(0, middle), "utf8") <= max) low = middle;
    else high = middle - 1;
  }
  const cut = value.slice(0, low);
  return { text: cut, truncated: size - Buffer.byteLength(cut, "utf8") };
}

export function emptyUsage() {
  return { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, cost: 0, turns: 0, contextTokens: 0 };
}

/** Fold one message's pi `usage` into the child total. */
export function addUsage(total, usage) {
  if (!usage || typeof usage !== "object") return total;
  const input = Number(usage.input ?? 0) || 0;
  const output = Number(usage.output ?? 0) || 0;
  const cacheRead = Number(usage.cacheRead ?? 0) || 0;
  const cacheWrite = Number(usage.cacheWrite ?? 0) || 0;
  const cost =
    typeof usage.cost === "number"
      ? usage.cost
      : typeof usage.cost?.total === "number"
        ? usage.cost.total
        : 0;
  const contextTokens = Number(usage.totalTokens ?? input + output + cacheRead + cacheWrite) || 0;
  return {
    input: total.input + input,
    output: total.output + output,
    cacheRead: total.cacheRead + cacheRead,
    cacheWrite: total.cacheWrite + cacheWrite,
    cost: total.cost + cost,
    turns: total.turns + 1,
    contextTokens,
  };
}

/** A new child's state. `phase` starts at `queued` so a pool can show it. */
export function newChildState({ id, agent, task, cwd, workflowMode, now = Date.now() }) {
  return {
    id,
    agent: agent.name,
    source: agent.source,
    capability: agent.ceiling,
    task: truncate(task, BOUNDS.task),
    status: "running",
    phase: "queued",
    current: null,
    tools: 0,
    toolCounts: {},
    steps: [],
    stepsTotal: 0,
    report: null,
    error: null,
    stderrTail: "",
    blocked: [],
    model: agent.model,
    thinking: agent.thinking,
    workflowMode,
    cwd,
    usage: emptyUsage(),
    stopReason: null,
    queuedAt: now,
    lastEventAt: now,
    startedAt: null,
    endedAt: null,
    aborted: false,
    timedOut: false,
    sawAssistant: false,
  };
}

/** Text of an assistant message's text blocks. */
function assistantText(message) {
  const content = message?.content;
  if (!Array.isArray(content)) return "";
  return content
    .filter((block) => block?.type === "text" && typeof block.text === "string")
    .map((block) => block.text)
    .join("\n\n")
    .trim();
}

/**
 * Fold one stream record into the child's state. Unknown events are ignored —
 * pi may add events, and a newer pi must degrade, not crash.
 */
export function applyEvent(state, event, now = Date.now()) {
  const type = event?.type;
  if (typeof type !== "string") return state;
  state.lastEventAt = now;

  if (type === "message_start") {
    const message = event.message;
    if (message?.role === "assistant") {
      state.phase = "thinking";
      state.current = null;
      if (typeof message.model === "string") state.model = message.model;
      if (typeof message.thinkingLevel === "string") state.thinking = message.thinkingLevel;
    }
    return state;
  }

  if (type === "message_end") {
    const message = event.message;
    if (message?.role !== "assistant") return state;
    state.sawAssistant = true;
    state.usage = addUsage(state.usage, message.usage);
    if (typeof message.stopReason === "string") state.stopReason = message.stopReason;
    const text = assistantText(message);
    if (text !== "") state.report = text;
    if (message.stopReason === "error") {
      state.error = truncate(message.errorMessage ?? "child reported an error", 500);
    }
    state.phase = "settled";
    return state;
  }

  if (type === "tool_execution_start") {
    const name = String(event.toolName ?? "tool");
    state.tools += 1;
    state.stepsTotal += 1;
    state.toolCounts[name] = (state.toolCounts[name] ?? 0) + 1;
    state.phase = "tool";
    state.current = describeTool(name, event.args);
    state.steps.push({ kind: "tool", name, preview: previewArgs(event.args) });
    if (state.steps.length > BOUNDS.steps) state.steps = state.steps.slice(-BOUNDS.steps);
    return state;
  }

  if (type === "tool_execution_end") {
    state.phase = "thinking";
    state.current = null;
    if (event.isError === true) {
      const name = String(event.toolName ?? "tool");
      state.blocked.push({
        tool: name,
        target: previewArgs(event.args) || null,
        reason: "Tool failed",
      });
      if (state.blocked.length > BOUNDS.blocked) state.blocked = state.blocked.slice(-BOUNDS.blocked);
    }
    return state;
  }

  if (type === "agent_settled") {
    if (event.aborted === true) {
      state.aborted = true;
      state.phase = "killed";
    }
    return state;
  }

  return state;
}

/** Settle a child from its own evidence (§8), never from an assumption. */
export function finalizeChild(state, { exitCode = 0, timedOut = false, timeoutMinutes = 30, now = Date.now() } = {}) {
  state.endedAt = now;
  if (state.startedAt === null) state.startedAt = state.queuedAt;
  state.timedOut = timedOut;
  if (state.aborted) {
    state.status = "aborted";
    state.phase = "killed";
    return state;
  }
  if (timedOut) {
    state.status = "failed";
    state.phase = "killed";
    state.error = `timed out after ${timeoutMinutes} min`;
    return state;
  }
  if (state.stopReason === "error" || exitCode !== 0 || !state.sawAssistant) {
    state.status = "failed";
    if (!state.error) {
      state.error =
        exitCode !== 0
          ? `child exited ${exitCode}`
          : state.sawAssistant
            ? "child reported an error"
            : "child produced no assistant message";
    }
    state.phase = "settled";
    return state;
  }
  state.status = "done";
  state.phase = "settled";
  return state;
}

/** The bounded `details` payload for one child (§6.2). */
export function childDetails(state) {
  const report = state.report === null || state.report === undefined
    ? null
    : capBytes(state.report, BOUNDS.report).text;
  const toolCounts = Object.entries(state.toolCounts)
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, BOUNDS.toolCounts);
  return {
    id: state.id,
    agent: state.agent,
    source: state.source,
    capability: state.capability,
    task: truncate(state.task, BOUNDS.task),
    status: state.status,
    stopReason: state.stopReason,
    model: state.model,
    thinking: state.thinking,
    workflowMode: state.workflowMode,
    phase: state.phase,
    current: state.current,
    tools: state.tools,
    toolCounts: Object.fromEntries(toolCounts),
    cwd: state.cwd,
    steps: state.steps.map((step) => ({ ...step, preview: truncate(step.preview, BOUNDS.preview) })),
    stepsTotal: state.stepsTotal,
    report,
    error: state.error,
    stderrTail: state.stderrTail === "" ? "" : capBytes(state.stderrTail, BOUNDS.stderr).text,
    blocked: state.blocked.slice(-BOUNDS.blocked),
    queuedAt: state.queuedAt,
    lastEventAt: state.lastEventAt,
    usage: state.usage,
    startedAt: state.startedAt,
    endedAt: state.endedAt,
  };
}

/** The whole run's `details` envelope. */
export function runDetails(mode, children) {
  return { orbit: DETAILS_VERSION, mode, children: children.map(childDetails) };
}

/** `↑3.1k ↓812 · $0.01` — only what the child actually reported. */
export function formatUsage(usage) {
  const parts = [];
  if (usage.input > 0 || usage.output > 0) parts.push(`↑${shortTokens(usage.input)} ↓${shortTokens(usage.output)}`);
  if (usage.cost > 0) parts.push(usage.cost < 0.01 ? "<$0.01" : `$${usage.cost.toFixed(2)}`);
  return parts.join(" · ");
}

function shortTokens(value) {
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`;
  return String(value);
}

/**
 * The reason a failed child contributes to the parent's copy: the extension's
 * sentence plus the child's own stderr, so the parent can act on what pi
 * actually said rather than only on an exit code (§8). The card keeps the two
 * apart — `error` is the claim, `stderrTail` is the evidence beside it.
 */
function failureReason(child) {
  if (!child.error) return null;
  const tail = String(child.stderrTail ?? "").trim();
  return tail === "" ? child.error : `${child.error}: ${tail}`;
}

/**
 * What the parent model receives (D-G): one line per child plus a capped
 * report. The full report stays in `details`.
 */
export function runContent(children) {
  const blocks = [];
  for (const child of children) {
    // A run that never started is one sentence: the reason, and nothing else.
    if (child.phase === "refused") {
      blocks.push(`${child.agent || "subagent"} — refused: ${child.error ?? "not started"}`);
      continue;
    }
    const usage = formatUsage(child.usage ?? emptyUsage());
    const reason = child.status === "failed" ? failureReason(child) : null;
    const status = reason === null ? child.status : `failed: ${reason}`;
    const header = `${child.agent} — ${status}${usage ? ` · ${usage}` : ""}`;
    const report = child.report == null || child.report.trim() === ""
      ? "no closing message for the parent"
      : capBytes(child.report, BOUNDS.contentReport).text;
    const capped = child.report == null ? 0 : capBytes(child.report, BOUNDS.contentReport).truncated;
    const marker = capped > 0 ? `\n\n… truncated (${Math.ceil(capped / 1024)} KiB) — full report in the card` : "";
    blocks.push(`${header}\n${report}${marker}`);
  }
  return blocks.join("\n\n");
}

/** Sum every child's usage, spent or failed alike (D-H). */
export function totalUsage(children) {
  return children.reduce((total, child) => {
    const usage = child.usage ?? emptyUsage();
    return {
      input: total.input + usage.input,
      output: total.output + usage.output,
      cacheRead: total.cacheRead + usage.cacheRead,
      cacheWrite: total.cacheWrite + usage.cacheWrite,
      cost: total.cost + usage.cost,
      turns: total.turns + usage.turns,
      contextTokens: Math.max(total.contextTokens, usage.contextTokens ?? 0),
    };
  }, emptyUsage());
}
