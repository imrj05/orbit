/**
 * Orbit-owned settings for the subagent tool, read fresh on every call.
 *
 * Three files, all Orbit-written and all read leniently (a missing, partial or
 * corrupt file falls back to the strict default, never to "unlimited"):
 *
 *   ~/.orbit-pi/subagents.json  the limits and the per-agent Orbit-only facts
 *   ~/.orbit-pi/workflow.json   pi session id → plan | build | ask (D8)
 *   ~/.orbit-pi/access.json     the active access mode
 *
 * The last two are the same files the workflow extension and the guard read,
 * so there is exactly one source of truth for each.
 */

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { CEILING_RANK } from "./agents.js";

/** Hard maxima. Configuration may lower these, never raise them. */
export const HARD_CAPS = {
  maxTasksPerRun: 8,
  maxConcurrent: 4,
  maxChildrenPerSession: 24,
  timeoutMinutes: 120,
};

/** Defaults, matching the plan's D-M. */
export const DEFAULT_CONFIG = {
  enabled: true,
  maxConcurrent: 4,
  maxChildrenPerSession: 24,
  maxTasksPerRun: 8,
  timeoutMinutes: 30,
  projectAgents: false,
  agents: {},
  disabled: [],
};

export const DEFAULT_WORKFLOW_MODE = "build";
export const DEFAULT_ACCESS_MODE = "supervised";

function home() {
  return process.env.HOME || process.env.USERPROFILE || os.homedir() || ".";
}

/** `~/.orbit-pi` — the directory Orbit owns. */
export function orbitDir(dir = path.join(home(), ".orbit-pi")) {
  return dir;
}

export function configPath(dir = orbitDir()) {
  return path.join(dir, "subagents.json");
}

export function workflowPath(dir = orbitDir()) {
  return path.join(dir, "workflow.json");
}

export function accessPath(dir = orbitDir()) {
  return path.join(dir, "access.json");
}

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch {
    return null;
  }
}

function clampInt(value, fallback, max) {
  const number = typeof value === "number" && Number.isFinite(value) ? Math.floor(value) : fallback;
  if (number < 1) return fallback;
  return Math.min(number, max);
}

/**
 * Read `subagents.json`, merged over the defaults and clamped to the hard
 * maxima. Unknown agent entries are preserved so a newer page cannot lose
 * settings to an older extension.
 */
export function loadConfig(filePath = configPath()) {
  const raw = readJson(filePath);
  const source = raw && typeof raw === "object" ? raw : {};
  const agents = {};
  if (source.agents && typeof source.agents === "object" && !Array.isArray(source.agents)) {
    for (const [name, entry] of Object.entries(source.agents)) {
      agents[name] = {
        enabled: entry?.enabled !== false,
        mode: ["inherit", "plan", "build", "ask"].includes(entry?.mode) ? entry.mode : "inherit",
      };
    }
  }
  const disabled = Array.isArray(source.disabled)
    ? source.disabled.filter((name) => typeof name === "string")
    : [];
  return {
    enabled: source.enabled !== false,
    maxConcurrent: clampInt(source.maxConcurrent, DEFAULT_CONFIG.maxConcurrent, HARD_CAPS.maxConcurrent),
    maxChildrenPerSession: clampInt(
      source.maxChildrenPerSession,
      DEFAULT_CONFIG.maxChildrenPerSession,
      HARD_CAPS.maxChildrenPerSession,
    ),
    maxTasksPerRun: clampInt(source.maxTasksPerRun, DEFAULT_CONFIG.maxTasksPerRun, HARD_CAPS.maxTasksPerRun),
    timeoutMinutes: clampInt(source.timeoutMinutes, DEFAULT_CONFIG.timeoutMinutes, HARD_CAPS.timeoutMinutes),
    projectAgents: source.projectAgents === true,
    agents,
    disabled,
  };
}

/** The per-agent Orbit-only facts, with the builtin defaults. */
export function agentEntry(config, name) {
  if (config.disabled.includes(name)) return { enabled: false, mode: "inherit" };
  return config.agents[name] ?? { enabled: true, mode: "inherit" };
}

/** The access mode Orbit's guard would use, read from its own store. */
export function readAccessMode(filePath = accessPath()) {
  const raw = readJson(filePath);
  const mode = raw?.mode;
  return ["supervised", "auto-accept-edits", "full-access"].includes(mode) ? mode : DEFAULT_ACCESS_MODE;
}

/**
 * The session's workflow mode: the app's store for this session id first (the
 * workflow extension's own resolution order), then the environment value Orbit
 * hands a spawned session, then Build.
 */
export function resolveWorkflowMode({ sessionId, store = null, env = process.env } = {}) {
  const stored = typeof sessionId === "string" && store && typeof store === "object" ? store[sessionId] : undefined;
  if (["plan", "build", "ask"].includes(stored)) return stored;
  const fromEnv = env?.ORBIT_WORKFLOW_MODE;
  if (["plan", "build", "ask"].includes(fromEnv)) return fromEnv;
  return DEFAULT_WORKFLOW_MODE;
}

/** Read the workflow store; `{}` when absent or malformed. */
export function readWorkflowStore(filePath = workflowPath()) {
  const raw = readJson(filePath);
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
  const out = {};
  for (const [sessionId, mode] of Object.entries(raw)) {
    if (["plan", "build", "ask"].includes(mode)) out[sessionId] = mode;
  }
  return out;
}

/**
 * The maximum ceiling the session permits (D-Z).
 *
 * Access and workflow intersect: Plan/Ask make every child read-only whatever
 * the access mode says. `all` is only reachable under Full access in Build,
 * because an unrecognised tool can do anything.
 */
export function allowedCeiling(accessMode, workflowMode) {
  const workflow = ["plan", "ask", "build"].includes(workflowMode) ? workflowMode : DEFAULT_WORKFLOW_MODE;
  if (workflow !== "build") return "read";
  if (accessMode === "full-access") return "all";
  if (accessMode === "auto-accept-edits") return "edit";
  return "read";
}

/** Whether `ceiling` is permitted by `allowed`. */
export function ceilingAllowed(ceiling, allowed) {
  return (CEILING_RANK[ceiling] ?? CEILING_RANK.all) <= (CEILING_RANK[allowed] ?? CEILING_RANK.read);
}

/** A human sentence naming the mode, for refusal and card copy. */
export function modeLabel(accessMode, workflowMode) {
  const access = {
    supervised: "Supervised",
    "auto-accept-edits": "Auto-accept edits",
    "full-access": "Full access",
  }[accessMode] ?? "Supervised";
  const workflow = { plan: "Plan", build: "Build", ask: "Ask" }[workflowMode] ?? "Build";
  return `${workflow} · ${access}`;
}
