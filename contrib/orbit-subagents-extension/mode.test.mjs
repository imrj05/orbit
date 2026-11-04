#!/usr/bin/env node
/**
 * Tests for the Orbit-owned subagent settings and the mode intersection.
 *
 * Run: node --test contrib/orbit-subagents-extension/mode.test.mjs
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  DEFAULT_CONFIG,
  agentEntry,
  allowedCeiling,
  ceilingAllowed,
  loadConfig,
  modeLabel,
  readAccessMode,
  readWorkflowStore,
  resolveWorkflowMode,
} from "./mode.js";

function tempDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "orbit-subagents-mode-"));
}

test("missing config falls back to the documented defaults", () => {
  const config = loadConfig(path.join(tempDir(), "subagents.json"));
  assert.deepEqual(config, DEFAULT_CONFIG);
});

test("partial config keeps defaults and clamps the caps", () => {
  const dir = tempDir();
  const file = path.join(dir, "subagents.json");
  fs.writeFileSync(
    file,
    JSON.stringify({
      enabled: false,
      maxConcurrent: 99,
      maxChildrenPerSession: 0,
      maxTasksPerRun: 99,
      timeoutMinutes: 99,
      agents: { worker: { mode: "build" } },
    }),
  );
  const config = loadConfig(file);
  assert.equal(config.enabled, false);
  assert.equal(config.maxConcurrent, 4);
  assert.equal(config.maxChildrenPerSession, DEFAULT_CONFIG.maxChildrenPerSession);
  assert.equal(config.maxTasksPerRun, 8);
  assert.equal(config.timeoutMinutes, 99);
  assert.deepEqual(config.agents.worker, { enabled: true, mode: "build" });
});

test("unknown agent modes fall back to inherit", () => {
  const dir = tempDir();
  const file = path.join(dir, "subagents.json");
  fs.writeFileSync(file, JSON.stringify({ agents: { worker: { mode: "sideways" } } }));
  assert.equal(loadConfig(file).agents.worker.mode, "inherit");
});

test("a disabled list wins over an agent entry", () => {
  const config = { ...DEFAULT_CONFIG, disabled: ["scout"], agents: { scout: { enabled: true } } };
  assert.equal(agentEntry(config, "scout").enabled, false);
  assert.equal(agentEntry(config, "worker").enabled, true);
});

test("the access mode falls back to supervised", () => {
  const dir = tempDir();
  assert.equal(readAccessMode(path.join(dir, "access.json")), "supervised");
  fs.writeFileSync(path.join(dir, "access.json"), JSON.stringify({ mode: "full-access" }));
  assert.equal(readAccessMode(path.join(dir, "access.json")), "full-access");
  fs.writeFileSync(path.join(dir, "access.json"), "{not json");
  assert.equal(readAccessMode(path.join(dir, "access.json")), "supervised");
});

test("the workflow mode resolves store → env → build", () => {
  const store = { "session-a": "plan" };
  assert.equal(resolveWorkflowMode({ sessionId: "session-a", store, env: {} }), "plan");
  assert.equal(resolveWorkflowMode({ sessionId: "session-b", store, env: { ORBIT_WORKFLOW_MODE: "ask" } }), "ask");
  assert.equal(resolveWorkflowMode({ sessionId: "session-b", store, env: {} }), "build");
  assert.equal(
    resolveWorkflowMode({ sessionId: "session-b", store: { "session-b": "bogus" }, env: {} }),
    "build",
  );
});

test("the workflow store is read leniently", () => {
  const dir = tempDir();
  assert.deepEqual(readWorkflowStore(path.join(dir, "missing.json")), {});
  const file = path.join(dir, "workflow.json");
  fs.writeFileSync(file, JSON.stringify({ a: "plan", b: "nonsense", c: 3 }));
  assert.deepEqual(readWorkflowStore(file), { a: "plan" });
});

test("Plan and Ask cap every child at read, whatever the access mode", () => {
  assert.equal(allowedCeiling("full-access", "plan"), "read");
  assert.equal(allowedCeiling("full-access", "ask"), "read");
  assert.equal(allowedCeiling("auto-accept-edits", "ask"), "read");
});

test("Build intersects the access mode", () => {
  assert.equal(allowedCeiling("supervised", "build"), "read");
  assert.equal(allowedCeiling("auto-accept-edits", "build"), "edit");
  assert.equal(allowedCeiling("full-access", "build"), "all");
});

test("ceiling comparisons rank all above exec", () => {
  assert.ok(ceilingAllowed("read", "read"));
  assert.ok(!ceilingAllowed("edit", "read"));
  assert.ok(ceilingAllowed("edit", "edit"));
  assert.ok(!ceilingAllowed("exec", "edit"));
  assert.ok(ceilingAllowed("all", "all"));
  assert.ok(!ceilingAllowed("all", "edit"));
});

test("the mode label names both halves", () => {
  assert.equal(modeLabel("supervised", "plan"), "Plan · Supervised");
  assert.equal(modeLabel("full-access", "build"), "Build · Full access");
});
