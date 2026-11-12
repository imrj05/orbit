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
  effectiveCeiling,
  loadConfig,
  modeLabel,
  readAccessMode,
  readRules,
  readWorkflowStore,
  resolveWorkflowMode,
  rulesAllowFamily,
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

// ── permission rules (Settings → Permissions) ───────────────────────────

test("only an unconditional allow rule raises a family for an agent", () => {
  const rules = [
    { scope: "agent:worker", target: "exec", pattern: "", action: "allow" },
    { scope: "agent:scout", target: "exec", pattern: "git *", action: "allow" },
  ];
  assert.ok(rulesAllowFamily(rules, "worker", "exec"));
  // A pattern authorizes the commands it matches, not the tier.
  assert.equal(rulesAllowFamily(rules, "scout", "exec"), false);
  // Scoped rules never leak to another agent or to a main session.
  assert.equal(rulesAllowFamily(rules, "reviewer", "exec"), false);
  assert.equal(rulesAllowFamily(rules, "", "exec"), false);
  assert.equal(rulesAllowFamily(rules, "worker", "edit"), false);
});

test("a deny or ask rule never widens, and beats a broader allow", () => {
  assert.equal(
    rulesAllowFamily([{ scope: "any", target: "any", pattern: "", action: "deny" }], "worker", "exec"),
    false,
  );
  assert.equal(
    rulesAllowFamily([{ scope: "any", target: "exec", pattern: "", action: "ask" }], "worker", "exec"),
    false,
  );
  // The most specific scope with a matching rule decides, guard order.
  const rules = [
    { scope: "any", target: "exec", pattern: "", action: "allow" },
    { scope: "agent:worker", target: "exec", pattern: "", action: "deny" },
  ];
  assert.equal(rulesAllowFamily(rules, "worker", "exec"), false, "the worker's own deny wins");
  assert.equal(rulesAllowFamily(rules, "scout", "exec"), true, "other agents keep the allow");
});

test("effectiveCeiling raises only what a rule pre-authorizes, and never in Plan or Ask", () => {
  const rules = [{ scope: "agent:worker", target: "exec", pattern: "", action: "allow" }];
  assert.equal(
    effectiveCeiling({ agentName: "worker", allowed: "read", workflowMode: "build", rules }),
    "exec",
  );
  assert.equal(
    effectiveCeiling({ agentName: "scout", allowed: "read", workflowMode: "build", rules }),
    "read",
    "another agent is unchanged",
  );
  // Plan and Ask drop the write tools outright: a rule must not promise more.
  for (const workflowMode of ["plan", "ask"]) {
    assert.equal(
      effectiveCeiling({ agentName: "worker", allowed: "read", workflowMode, rules }),
      "read",
    );
  }
  // No rules at all is exactly the old behaviour.
  for (const allowed of ["read", "edit", "exec", "all"]) {
    assert.equal(
      effectiveCeiling({ agentName: "worker", allowed, workflowMode: "build", rules: [] }),
      allowed,
    );
  }
  // An allow for MCP/other needs the `all` ceiling.
  const mcp = [{ scope: "any", target: "mcp", pattern: "", action: "allow" }];
  assert.equal(
    effectiveCeiling({ agentName: "worker", allowed: "read", workflowMode: "build", rules: mcp }),
    "all",
  );
});

test("readRules loads what the Permissions page wrote and ignores the rest", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-rules-"));
  const file = path.join(dir, "access-rules.json");
  try {
    assert.deepEqual(readRules(file), [], "missing file");
    fs.writeFileSync(
      file,
      JSON.stringify({
        rules: [
          { scope: "agent:worker", target: "exec", pattern: "", action: "allow" },
          { scope: "any", target: "exec", pattern: "rm *", action: "deny" },
          { scope: "everyone", target: "exec", pattern: "", action: "allow" },
          "not a rule",
        ],
      }),
    );
    assert.deepEqual(readRules(file), [
      { scope: "agent:worker", target: "exec", pattern: "", action: "allow" },
      { scope: "any", target: "exec", pattern: "rm *", action: "deny" },
    ]);
    fs.writeFileSync(file, "{not json");
    assert.deepEqual(readRules(file), [], "a broken file is no rules, so nothing widens");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
