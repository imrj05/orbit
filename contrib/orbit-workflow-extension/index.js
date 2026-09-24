/**
 * Orbit workflow mode (pi extension).
 *
 * Orbit loads this file with `pi --extension <path>` on every session it
 * spawns. It scopes the session to Plan, Build, or Ask:
 *
 *   - `pi.setActiveTools()` drops the write tools in Plan/Ask.
 *   - a `tool_call` hook blocks write tools and non-allowlisted bash as a
 *     fail-safe (a tool pi preflighted before the tool set changed).
 *   - `before_agent_start` injects hidden guidance (produce a plan; execute
 *     remaining steps and tag `[DONE:n]`; or answer only).
 *   - plan steps are tracked from the assistant's `Plan:` section and
 *     `[DONE:n]` markers, and appended as `orbit:workflow-todos` custom
 *     session entries. Orbit polls `get_entries` and paints the progress bar.
 *
 * The mode lives per session in `~/.orbit-pi/workflow.json` (a map of pi
 * session id → wire id) and is read **fresh on every hook**, so changing it in
 * Orbit re-arms a live session with no restart. An `ORBIT_WORKFLOW_MODE` env
 * var covers the moment before the store has the id. The mode is also written
 * into the session as an `orbit:workflow` entry so it survives resume even if
 * the store is lost; the pure pieces live in `policy.js`.
 *
 * Fail-safe: any resolve/read failure degrades to Build, never to a silently
 * weaker read-only state.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import {
  CONTEXT_TYPE,
  DEFAULT_MODE,
  TODO_ENTRY_TYPE,
  WORKFLOW_ENTRY_TYPE,
  allowedTools,
  blockReason,
  extractPlanSteps,
  guidance,
  markCompletedSteps,
  normalizeMode,
} from "./policy.js";

/** `~/.orbit-pi/workflow.json`, resolved per call so a test `HOME` is honored. */
function storePath() {
  return path.join(os.homedir(), ".orbit-pi", "workflow.json");
}

/** The mode stored for `sessionId`, or `undefined` when absent/unreadable. */
export function readStoredMode(sessionId, filePath = storePath()) {
  if (!sessionId) return undefined;
  try {
    const map = JSON.parse(fs.readFileSync(filePath, "utf8"));
    const wire = map?.[sessionId];
    return typeof wire === "string" ? normalizeMode(wire) : undefined;
  } catch {
    return undefined;
  }
}

function branchEntries(ctx) {
  try {
    return ctx?.sessionManager?.getBranch?.() ?? [];
  } catch {
    return [];
  }
}

/** Every session entry (includes non-context custom entries). */
function sessionEntries(ctx) {
  try {
    return ctx?.sessionManager?.getEntries?.() ?? [];
  } catch {
    return [];
  }
}

/** The newest custom entry of `customType` in `entries`, or `undefined`. */
export function lastCustom(entries, customType) {
  const index = lastCustomIndex(entries, customType);
  return index >= 0 ? entries[index].data : undefined;
}

/** Index of the newest custom entry of `customType`, or `-1`. */
function lastCustomIndex(entries, customType) {
  for (let i = entries.length - 1; i >= 0; i -= 1) {
    const entry = entries[i];
    if (entry?.type === "custom" && entry.customType === customType) return i;
  }
  return -1;
}

/** The newest todo entry's normalized list, plus that entry's index. */
function todosFromEntry(entries) {
  const index = lastCustomIndex(entries, TODO_ENTRY_TYPE);
  if (index < 0) return { todos: [], index: -1 };
  const data = entries[index].data;
  if (!Array.isArray(data?.todos)) return { todos: [], index };
  return {
    todos: data.todos
      .filter((item) => typeof item?.text === "string")
      .map((item) => ({
        step: typeof item.step === "number" ? item.step : 0,
        text: item.text,
        done: item.done === true,
      })),
    index,
  };
}

/** Apply `[DONE:n]` tags from assistant messages after `index`, in place. */
function replayDones(todos, entries, index) {
  for (let i = index + 1; i < entries.length; i += 1) {
    const entry = entries[i];
    if (entry?.type !== "message") continue;
    markCompletedSteps(assistantText(entry.message), todos);
  }
  return todos;
}

/**
 * Rebuild the todo list from the newest todo entry, then replay `[DONE:n]`
 * tags from assistant messages that came after it. Replaying recovers a
 * process that resumed mid-execution (or a session whose dones were tagged
 * before this extension honored them), so the bar does not silently sit at
 * 0/N.
 */
export function reconstructTodos(entries) {
  const { todos, index } = todosFromEntry(entries);
  return index < 0 ? todos : replayDones(todos, entries, index);
}

/** Resolve the mode: store → env → session entry → Build. */
export function resolveMode(ctx, entries) {
  const sessionId = ctx?.sessionManager?.getSessionId?.();
  const stored = readStoredMode(sessionId);
  if (stored) return stored;
  if (typeof process.env.ORBIT_WORKFLOW_MODE === "string") {
    return normalizeMode(process.env.ORBIT_WORKFLOW_MODE);
  }
  const entry = lastCustom(entries ?? [], WORKFLOW_ENTRY_TYPE);
  if (typeof entry?.mode === "string") return normalizeMode(entry.mode);
  return DEFAULT_MODE;
}

/** The assistant text of a message, or `""` for any other role. */
export function assistantText(message) {
  if (!message || message.role !== "assistant") return "";
  const content = message.content;
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content
    .filter((block) => block?.type === "text" && typeof block.text === "string")
    .map((block) => block.text)
    .join("\n");
}

export default function activate(pi) {
  let baseTools = null;
  let appliedMode = null;
  let todos = [];
  let todosSnapshot = "";

  /** Apply a mode: keep the full tool set as the base, subtract per mode. */
  function applyMode(mode) {
    if (baseTools === null) {
      try {
        baseTools = pi.getActiveTools();
      } catch {
        baseTools = [];
      }
    }
    try {
      pi.setActiveTools(allowedTools(mode, baseTools));
    } catch {
      // A tool-set change is best-effort; the tool_call hook still blocks.
    }
    if (mode !== appliedMode) {
      appliedMode = mode;
      try {
        pi.appendEntry(WORKFLOW_ENTRY_TYPE, { mode });
      } catch {
        // Persistence is best-effort; the store remains authoritative.
      }
    }
  }

  /** Append the todo snapshot only when it changed, so the entry log stays small. */
  function persistTodos() {
    const snapshot = JSON.stringify(todos);
    if (snapshot === todosSnapshot) return;
    todosSnapshot = snapshot;
    try {
      pi.appendEntry(TODO_ENTRY_TYPE, { todos });
    } catch {
      // Best-effort.
    }
  }

  /** Update todos from one assistant message. */
  function updateTodos(text, mode, ctx) {
    if (!text) return;
    // A process that resumed mid-task (or never saw the plan itself) rebuilds
    // from the newest session entry before applying this message. The entry
    // already exists, so the snapshot baseline is set without re-appending.
    if (todos.length === 0) {
      const entries = sessionEntries(ctx);
      const { todos: baseline, index } = todosFromEntry(entries);
      if (baseline.length > 0) {
        const baselineJson = JSON.stringify(baseline);
        todos = replayDones(baseline, entries, index);
        todosSnapshot = baselineJson;
      }
    }
    if (mode !== "ask") {
      const extracted = extractPlanSteps(text);
      if (extracted.length > 0) {
        // A fresh plan replaces the list, keeping completion for matching steps.
        const doneByText = new Map(todos.map((todo) => [todo.text, todo.done]));
        todos = extracted.map((todo) => ({ ...todo, done: doneByText.get(todo.text) ?? false }));
      }
    }
    // `[DONE:n]` is honored in every mode: the model tagging a step is the
    // signal, and a plan executed from Plan mode must still advance.
    if (todos.length > 0) {
      markCompletedSteps(text, todos);
    }
    persistTodos();
  }

  pi.on("session_start", (_event, ctx) => {
    const entries = branchEntries(ctx);
    // Baseline against the raw entry (before replay), so a replay that
    // recovers dones appends the corrected snapshot for Orbit; an unchanged
    // resume appends nothing.
    const { todos: baseline, index } = todosFromEntry(entries);
    const baselineJson = JSON.stringify(baseline);
    todos = replayDones(baseline, entries, index);
    todosSnapshot = baselineJson;
    persistTodos();
    applyMode(resolveMode(ctx, entries));
  });

  pi.on("before_agent_start", (_event, ctx) => {
    const mode = resolveMode(ctx, branchEntries(ctx));
    applyMode(mode);
    const content = guidance(mode, todos);
    if (!content) return undefined;
    return { message: { customType: CONTEXT_TYPE, content, display: false } };
  });

  // Fail-safe: block write tools and unsafe bash in read-only modes even if
  // the tool set still lists them.
  pi.on("tool_call", (event, ctx) => {
    const mode = resolveMode(ctx, branchEntries(ctx));
    const reason = blockReason(mode, event?.toolName, event?.input);
    if (reason) return { block: true, reason };
    return undefined;
  });

  pi.on("turn_end", (event, ctx) => {
    const mode = resolveMode(ctx, branchEntries(ctx));
    updateTodos(assistantText(event?.message), mode, ctx);
  });

  pi.on("agent_end", (event, ctx) => {
    const mode = resolveMode(ctx, branchEntries(ctx));
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    updateTodos(messages.map(assistantText).join("\n"), mode, ctx);
  });

  // Drop stale injected guidance once the mode no longer produces any — a
  // finished plan in Build leaves nothing to say, so old instructions must
  // not keep steering the run. While guidance is active, leave it alone:
  // `context` can run after `before_agent_start` injected this turn's copy.
  pi.on("context", (event, ctx) => {
    const mode = resolveMode(ctx, branchEntries(ctx));
    if (guidance(mode, todos) !== "") return undefined;
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    const filtered = messages.filter((message) => message?.customType !== CONTEXT_TYPE);
    if (filtered.length !== messages.length) return { messages: filtered };
    return undefined;
  });
}
