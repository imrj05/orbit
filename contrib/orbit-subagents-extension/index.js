/**
 * Orbit's subagent tool (pi extension).
 *
 * Orbit loads this file with `--extension` on every session it spawns. It
 * registers one model-callable tool, `subagent`, which delegates a scoped task
 * to a child `pi --mode json -p --no-session` process and returns only the
 * child's report plus its cost. The child burns its own context; the parent's
 * context grows by the answer, not by the reading.
 *
 * Deliberate properties, each enforced here rather than in the UI:
 *   - the child inherits the parent's guardrails (access mode, workflow mode)
 *     and its declared tool list is clamped by a derived ceiling;
 *   - a delegation that cannot succeed is refused before it spends anything;
 *   - abort and timeout kill the whole process group, never leaving a child;
 *   - nothing is reported that an event did not say.
 *
 * See `docs/plans/subagents.md` for the decisions (D-A … D-Z).
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawn as nodeSpawn } from "node:child_process";

import { buildDescription, buildPromptGuidelines, discoverAgents, loadBuiltins, userAgentDir } from "./agents.js";
import {
  HARD_CAPS,
  agentEntry,
  allowedCeiling,
  ceilingAllowed,
  loadConfig,
  modeLabel,
  readAccessMode,
  readWorkflowStore,
  resolveWorkflowMode,
} from "./mode.js";
import { extensionArgs } from "./resources.js";
import {
  BOUNDS,
  LIMITS,
  LineDecoder,
  applyEvent,
  assemblePrompt,
  buildArgv,
  buildEnv,
  emptyUsage,
  finalizeChild,
  newChildState,
  parseRecord,
  runContent,
  runDetails,
  totalUsage,
  truncate,
} from "./run.js";

/** The tool's name. Prompts and muscle memory already use it (D-C). */
export const TOOL_NAME = "subagent";

/**
 * The tool's one-line entry in the `<tools>` section of the system prompt. pi
 * shows it only while the tool is active, and without it the model is told
 * only "Additional tools may be available" — the tool was never considered
 * early enough to win against doing the work inline.
 */
export const PROMPT_SNIPPET =
  "Delegate a scoped task to a child agent with its own context window.";

/**
 * One JSON Schema for all three modes (D-I). Plain JSON Schema, not TypeBox:
 * pi validates any schema object, and `contrib/` ships no dependencies.
 */
export const PARAMETERS = {
  type: "object",
  additionalProperties: false,
  properties: {
    agent: { type: "string", description: "Agent name for a single delegation." },
    task: { type: "string", description: "The task for the child. It sees nothing else." },
    chain: {
      type: "array",
      description: "Sequential steps; later tasks may use {previous}.",
      items: {
        type: "object",
        additionalProperties: false,
        properties: { agent: { type: "string" }, task: { type: "string" } },
        required: ["agent", "task"],
      },
    },
    tasks: {
      type: "array",
      description: "Independent tasks to run at once.",
      items: {
        type: "object",
        additionalProperties: false,
        properties: { agent: { type: "string" }, task: { type: "string" } },
        required: ["agent", "task"],
      },
    },
    cwd: { type: "string", description: "Working directory; defaults to the session's." },
  },
};

/** Children spawned per parent session in this process. */
const sessionCounts = new Map();

/**
 * Activate the extension. `overrides` exists for tests: `spawn`, `home`,
 * `env`, `now`, `tempDir`, `piBinary`.
 */
export default function activate(pi, overrides = {}) {
  const deps = {
    spawn: overrides.spawn ?? nodeSpawn,
    now: overrides.now ?? (() => Date.now()),
    home: overrides.home ?? process.env.HOME ?? process.env.USERPROFILE,
    env: overrides.env ?? process.env,
    tempDir: overrides.tempDir ?? os.tmpdir(),
    piBinary: overrides.piBinary ?? "pi",
  };

  // D-C: stand down if anything else already owns the name.
  const existing = safeGetAllTools(pi);
  if (existing.some((tool) => tool?.name === TOOL_NAME)) {
    pi?.on?.("session_start", (_event, ctx) => {
      ctx?.ui?.notify?.(
        "Orbit subagents is inactive: another extension (for example pi-subagents) already provides a `subagent` tool.",
        "warning",
      );
    });
    return { active: false, reason: "tool name already registered" };
  }

  if (typeof pi?.registerTool !== "function") {
    return { active: false, reason: "no tool registration API" };
  }

  let registered = null;

  /** Build the prompt surface from the enabled agents and register on change. */
  function refreshDescription() {
    const builtins = safeBuiltins();
    const { agents } = discoverAgents({ builtins, userDir: userAgentDir(deps.home) });
    const config = loadConfig(configPath(deps));
    const enabled = agents.filter((agent) => agentEntry(config, agent.name).enabled);
    const prompt = {
      description: buildDescription(enabled),
      promptGuidelines: buildPromptGuidelines(enabled),
    };
    const signature = JSON.stringify(prompt);
    if (signature === registered) return enabled;
    registered = signature;
    pi.registerTool({
      name: TOOL_NAME,
      label: "Subagent",
      ...prompt,
      promptSnippet: PROMPT_SNIPPET,
      parameters: PARAMETERS,
      async execute(toolCallId, params, signal, onUpdate, ctx) {
        return executeTool({ params, signal, onUpdate, ctx, pi, deps });
      },
    });
    return enabled;
  }

  refreshDescription();

  // A new session may have different agent files; re-register if the catalog
  // changed. This is the only place the request prefix changes (D-S).
  pi.on?.("session_start", () => {
    refreshDescription();
  });

  return { active: true };
}

function configPath(deps) {
  return path.join(deps.home ?? os.homedir(), ".orbit-pi", "subagents.json");
}

function safeGetAllTools(pi) {
  try {
    const all = pi?.getAllTools?.();
    return Array.isArray(all) ? all : [];
  } catch {
    return [];
  }
}

function safeBuiltins() {
  try {
    return loadBuiltins();
  } catch {
    return [];
  }
}

/** Resolve the requested shape into a list of tasks and a mode. */
export function parseRequest(params) {
  const value = params ?? {};
  const single = typeof value.agent === "string" && value.agent !== "";
  const chain = Array.isArray(value.chain) && value.chain.length > 0;
  const parallel = Array.isArray(value.tasks) && value.tasks.length > 0;
  const count = [single, chain, parallel].filter(Boolean).length;
  if (count !== 1) {
    return { error: "Give exactly one of: `agent` + `task`, `chain`, or `tasks`." };
  }
  if (single) {
    if (typeof value.task !== "string" || value.task.trim() === "") {
      return { error: "`task` is required with `agent`." };
    }
    return { mode: "single", tasks: [{ agent: value.agent, task: value.task }], cwd: value.cwd };
  }
  const list = chain ? value.chain : value.tasks;
  for (const [index, step] of list.entries()) {
    if (typeof step?.agent !== "string" || step.agent === "") {
      return { error: `Step ${index + 1} is missing an agent name.` };
    }
    if (typeof step?.task !== "string" || step.task.trim() === "") {
      return { error: `Step ${index + 1} is missing a task.` };
    }
  }
  return { mode: chain ? "chain" : "parallel", tasks: list, cwd: value.cwd };
}

/**
 * Execute one `subagent` call. Returns the tool result pi hands to the parent:
 * `{ content, details, usage }`.
 */
export async function executeTool({ params, signal, onUpdate, ctx, pi, deps, counts = sessionCounts }) {
  const now = deps.now;
  const sessionId = safeSessionId(ctx);
  const cwd = typeof params?.cwd === "string" && params.cwd !== "" ? params.cwd : ctx?.cwd ?? process.cwd();

  const builtins = safeBuiltins();
  const { agents } = discoverAgents({ builtins, userDir: userAgentDir(deps.home) });
  const config = loadConfig(configPath(deps));

  if (!config.enabled) {
    return refusal("subagent", "", "Subagents are disabled in Settings → Subagents.", {
      mode: "single",
      cwd,
      now: now(),
    });
  }

  const request = parseRequest(params);
  if (signal?.aborted) {
    return refusal("subagent", "", "The turn was aborted before the delegation started.", {
      mode: request.mode ?? "single",
      cwd,
      now: now(),
    });
  }
  if (request.error) {
    const available = agents.map((agent) => agent.name).join(", ");
    return refusal("subagent", "", `${request.error} Available agents: ${available}.`, {
      mode: "single",
      cwd,
      now: now(),
    });
  }

  if (deps.env?.ORBIT_SUBAGENT_DEPTH === String(LIMITS.depth)) {
    return refusal("subagent", "", "A subagent cannot start another subagent (depth limit 1).", {
      mode: request.mode,
      cwd,
      now: now(),
    });
  }

  const accessMode = readAccessMode(path.join(deps.home ?? os.homedir(), ".orbit-pi", "access.json"));
  const workflowMode = resolveWorkflowMode({
    sessionId,
    store: readWorkflowStore(path.join(deps.home ?? os.homedir(), ".orbit-pi", "workflow.json")),
    env: deps.env,
  });
  const allowed = allowedCeiling(accessMode, workflowMode);

  const byName = new Map(agents.map((agent) => [agent.name, agent]));
  const resolved = [];
  for (const task of request.tasks) {
    const agent = byName.get(task.agent);
    if (!agent) {
      const available = agents.map((candidate) => candidate.name).join(", ");
      return refusal(task.agent, task.task, `Agent "${task.agent}" not found; available: ${available}.`, {
        mode: request.mode,
        cwd,
        now: now(),
        workflowMode,
      });
    }
    if (!agentEntry(config, agent.name).enabled) {
      return refusal(
        agent.name,
        task.task,
        `Agent "${agent.name}" is disabled in Settings → Subagents.`,
        { mode: request.mode, cwd, now: now(), workflowMode },
      );
    }
    if (!ceilingAllowed(agent.ceiling, allowed)) {
      return refusal(
        agent.name,
        task.task,
        `${agent.name} is ${capabilityPhrase(agent.ceiling)}; this session is ${modeLabel(accessMode, workflowMode)}. ` +
          `${remedy(agent.ceiling)}`,
        { mode: request.mode, cwd, now: now(), workflowMode },
      );
    }
    resolved.push({ agent, task: task.task });
  }

  if (resolved.length > Math.min(config.maxTasksPerRun, HARD_CAPS.maxTasksPerRun)) {
    return refusal(
      resolved[0].agent.name,
      "",
      `That is ${resolved.length} tasks; the limit is ${config.maxTasksPerRun} per run.`,
      { mode: request.mode, cwd, now: now(), workflowMode },
    );
  }

  const already = counts.get(sessionId) ?? 0;
  if (already + resolved.length > config.maxChildrenPerSession) {
    return refusal(
      resolved[0].agent.name,
      "",
      `This session has already started ${already} children; the limit is ${config.maxChildrenPerSession}.`,
      { mode: request.mode, cwd, now: now(), workflowMode },
    );
  }
  counts.set(sessionId, already + resolved.length);

  const extensions = extensionArgs(deps.env?.ORBIT_BUNDLED_EXTENSIONS);
  const agentMode = (agent) => {
    const pinned = agentEntry(config, agent.name).mode;
    if (pinned === "inherit") return workflowMode;
    if (pinned === "plan" || pinned === "ask") return pinned;
    return "build";
  };

  const children = [];
  const runOne = async (agent, task) => {
    const state = newChildState({
      id: `c${children.length + 1}`,
      agent,
      task,
      cwd,
      workflowMode: agentMode(agent),
      now: now(),
    });
    children.push(state);
    return runChild(state, {
      agent,
      deps,
      extensions,
      signal,
      timeoutMinutes: config.timeoutMinutes,
      sessionId,
      parentModel: modelId(ctx?.model),
      parentThinking: typeof ctx?.thinkingLevel === "string" ? ctx.thinkingLevel : null,
      onProgress: (force) => emitProgress(onUpdate, request.mode, children, force),
    });
  };

  if (request.mode === "chain") {
    let previous = "";
    for (const { agent, task } of resolved) {
      const expanded = task.replaceAll("{previous}", previous);
      const state = await runOne(agent, expanded);
      if (state.status !== "done") break;
      previous = state.report ?? "";
    }
  } else if (request.mode === "parallel") {
    await runPool(resolved, Math.min(config.maxConcurrent, LIMITS.concurrent), ({ agent, task }) =>
      runOne(agent, task),
    );
  } else {
    await runOne(resolved[0].agent, resolved[0].task);
  }

  const details = runDetails(request.mode, children);
  return {
    content: [{ type: "text", text: runContent(children) }],
    details,
    usage: totalUsage(children),
  };
}

/** A refusal: a text result the parent can act on, and a card that says why. */
function refusal(agentName, task, message, { mode, cwd, now, workflowMode = "build" }) {
  const child = {
    id: "c1",
    agent: agentName,
    source: "builtin",
    capability: "read",
    task: truncate(task, BOUNDS.task),
    status: "failed",
    phase: "refused",
    current: null,
    tools: 0,
    toolCounts: {},
    steps: [],
    stepsTotal: 0,
    report: null,
    error: message,
    stderrTail: "",
    blocked: [],
    model: null,
    thinking: null,
    workflowMode,
    cwd,
    usage: emptyUsage(),
    stopReason: null,
    queuedAt: now,
    lastEventAt: now,
    startedAt: null,
    endedAt: now,
    aborted: false,
    timedOut: false,
    sawAssistant: false,
  };
  return {
    content: [{ type: "text", text: message }],
    details: runDetails(mode, [child]),
    usage: emptyUsage(),
  };
}

function capabilityPhrase(ceiling) {
  if (ceiling === "read") return "read-only";
  if (ceiling === "edit") return "an editing agent";
  if (ceiling === "all") return "a tool-using agent";
  return "a command-running agent";
}

function remedy(ceiling) {
  if (ceiling === "read") return "Its tool list is below this mode's limit, so this should not happen.";
  if (ceiling === "edit") return "Switch the session to Build + Auto-accept edits or Full access, or delegate to a read-only agent.";
  if (ceiling === "all") return "Switch the session to Build + Full access, or delegate to an agent whose tools are file tools only.";
  return "Switch the session to Build + Full access, or delegate to a read-only agent.";
}

function safeSessionId(ctx) {
  try {
    return ctx?.sessionManager?.getSessionId?.() ?? "unknown";
  } catch {
    return "unknown";
  }
}

function modelId(model) {
  if (typeof model === "string" && model !== "") return model;
  if (model && typeof model.provider === "string" && typeof model.id === "string") {
    return `${model.provider}/${model.id}`;
  }
  return null;
}

/** Run `count` workers over `items`, never more than `limit` at once. */
export async function runPool(items, limit, run) {
  const results = new Array(items.length);
  let next = 0;
  const workers = Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (true) {
      const index = next;
      next += 1;
      if (index >= items.length) return;
      results[index] = await run(items[index], index);
    }
  });
  await Promise.all(workers);
  return results;
}

/** Send a coalesced progress update; a transition always sends. */
export function emitProgress(onUpdate, mode, children, force = false, now = Date.now()) {
  if (typeof onUpdate !== "function") return now;
  const last = emitProgress.last ?? {};
  const key = mode;
  if (!force && last[key] && now - last[key] < 400) return last[key];
  last[key] = now;
  emitProgress.last = last;
  const details = runDetails(mode, children);
  const running = children.filter((child) => child.status === "running").length;
  const text = running > 0 ? `${running} of ${children.length} running` : runContent(children);
  try {
    onUpdate({ content: [{ type: "text", text }], details });
  } catch {
    // A failing progress sink must never fail the delegation.
  }
  return now;
}

/**
 * Spawn and observe one child. Resolves with the settled state; never rejects
 * for a child-side failure (only an internal fault throws).
 */
export async function runChild(state, {
  agent,
  deps,
  extensions,
  signal,
  timeoutMinutes,
  sessionId,
  parentModel,
  parentThinking,
  onProgress,
}) {
  const now = deps.now;
  const promptPath = path.join(deps.tempDir, `orbit-subagent-${process.pid}-${state.id}.md`);
  const systemPrompt = assemblePrompt({ body: agent?.body ?? "" });
  const argv = buildArgv({
    promptPath,
    prompt: `Task: ${state.task}`,
    extensions,
    model: state.model ?? parentModel,
    thinking: state.thinking ?? parentThinking,
    tools: agent?.tools ?? null,
    toolsPresent: agent?.toolsPresent === true,
    ceiling: state.capability,
  });
  const env = { ...deps.env, ...buildEnv({
    parentSessionId: sessionId,
    agentName: state.agent,
    ceiling: state.capability,
    workflowMode: state.workflowMode,
  }) };

  try {
    fs.mkdirSync(path.dirname(promptPath), { recursive: true });
    fs.writeFileSync(promptPath, systemPrompt, { mode: 0o600 });
  } catch (error) {
    state.status = "failed";
    state.phase = "spawning";
    state.error = truncate(`could not write the child prompt: ${error?.message ?? error}`, 500);
    state.endedAt = now();
    return state;
  }

  let child;
  try {
    child = deps.spawn(deps.piBinary, argv, {
      cwd: state.cwd,
      env,
      stdio: ["ignore", "pipe", "pipe"],
      detached: process.platform !== "win32",
      windowsHide: true,
    });
  } catch (error) {
    state.status = "failed";
    state.phase = "spawning";
    state.error = truncate(`could not start pi: ${error?.message ?? error}`, 500);
    state.endedAt = now();
    try {
      fs.rmSync(promptPath, { force: true });
    } catch {
      // Best effort.
    }
    return state;
  }

  state.phase = "spawning";
  state.startedAt = now();
  onProgress?.(true);

  let stderr = "";
  const decoder = new LineDecoder();
  const consume = (line) => {
    const event = parseRecord(line);
    if (event) applyEvent(state, event, now());
  };

  child.stdout?.on?.("data", (chunk) => {
    for (const line of decoder.push(chunk)) consume(line);
    onProgress?.(false);
  });
  child.stderr?.on?.("data", (chunk) => {
    stderr = truncate(stderr + chunk.toString(), BOUNDS.stderr + 1);
    state.stderrTail = stderr;
  });

  let timedOut = false;
  const timer = timeoutMinutes > 0
    ? setTimeout(() => {
        timedOut = true;
        killTree(child, "SIGTERM", deps);
        setTimeout(() => killTree(child, "SIGKILL", deps), 3000).unref?.();
      }, timeoutMinutes * 60_000)
    : null;
  timer?.unref?.();

  let onAbort;
  const abortPromise = new Promise((resolve) => {
    if (!signal) return;
    onAbort = () => {
      state.aborted = true;
      killTree(child, "SIGTERM", deps);
      setTimeout(() => killTree(child, "SIGKILL", deps), 3000).unref?.();
      resolve();
    };
    if (signal.aborted) onAbort();
    else signal.addEventListener?.("abort", onAbort, { once: true });
  });

  const exitCode = await new Promise((resolve) => {
    let settled = false;
    const finish = (code) => {
      if (settled) return;
      settled = true;
      resolve(code);
    };
    child.once?.("error", () => finish(1));
    child.once?.("close", (code) => finish(typeof code === "number" ? code : 1));
    // An abort must not wait for a process that ignores SIGTERM.
    abortPromise.then(() => setTimeout(() => finish(1), 3500).unref?.());
  });

  for (const line of decoder.end()) consume(line);
  if (timer) clearTimeout(timer);
  if (onAbort) signal?.removeEventListener?.("abort", onAbort);
  try {
    fs.rmSync(promptPath, { force: true });
  } catch {
    // Best effort; the temp dir is cleaned by the OS.
  }

  finalizeChild(state, { exitCode, timedOut, timeoutMinutes, now: now() });
  onProgress?.(true);
  return state;
}

/** Kill the child's whole process group; Windows needs `taskkill /T`. */
export function killTree(child, signal, deps = {}) {
  if (!child?.pid) return;
  const spawn = deps.spawn ?? nodeSpawn;
  if (process.platform === "win32") {
    try {
      spawn("taskkill", ["/pid", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
    } catch {
      // Nothing else to try.
    }
    return;
  }
  try {
    process.kill(-child.pid, signal);
  } catch {
    try {
      child.kill?.(signal);
    } catch {
      // Already gone.
    }
  }
}
