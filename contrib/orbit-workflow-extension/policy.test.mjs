#!/usr/bin/env node
/**
 * Tests for the pure workflow policy.
 *
 * Run: node --test contrib/orbit-workflow-extension/policy.test.mjs
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_MODE,
  allowedTools,
  blockReason,
  cleanStepText,
  extractPlanSteps,
  guidance,
  isSafeCommand,
  markCompletedSteps,
  normalizeMode,
} from "./policy.js";

test("normalizeMode falls back to build", () => {
  assert.equal(normalizeMode("plan"), "plan");
  assert.equal(normalizeMode("ask"), "ask");
  assert.equal(normalizeMode("yolo"), DEFAULT_MODE);
  assert.equal(normalizeMode(undefined), DEFAULT_MODE);
});

test("allowedTools subtracts the disabled write tools", () => {
  const base = ["read", "bash", "edit", "write", "grep"];
  assert.deepEqual(allowedTools("plan", base), ["read", "bash", "grep"]);
  assert.deepEqual(allowedTools("ask", base), ["read", "bash", "grep"]);
  assert.deepEqual(allowedTools("build", base), base);
  // Unknown names are ignored, duplicates collapse.
  assert.deepEqual(allowedTools("plan", ["read", "read", "nope", "edit"]), ["read", "nope"]);
});

test("build never blocks", () => {
  assert.equal(blockReason("build", "edit", {}), undefined);
  assert.equal(blockReason("build", "bash", { command: "rm -rf /" }), undefined);
});

test("plan and ask block the write tools", () => {
  assert.match(blockReason("plan", "edit", {}), /edit is disabled/);
  assert.match(blockReason("ask", "write", {}), /write is disabled/);
});

test("plan and ask gate bash by the allowlist", () => {
  assert.equal(blockReason("plan", "bash", { command: "git log --oneline" }), undefined);
  assert.equal(blockReason("ask", "bash", { command: "rg TODO src" }), undefined);
  // The AI reviewer's own commands: a read-only diff runs, a commit is blocked.
  assert.equal(blockReason("ask", "bash", { command: "git diff HEAD" }), undefined);
  assert.match(blockReason("plan", "bash", { command: "rm -rf build" }), /blocked/);
  assert.match(blockReason("ask", "bash", { command: "git commit -m x" }), /blocked/);
  // Compound/redirect/substitution are rejected outright.
  for (const command of ["cat a; rm b", "cat a && rm b", "cat a | sh", "echo x > f", "echo $(rm x)", "echo `rm x`", "ls\nrm x"]) {
    assert.equal(isSafeCommand(command), false, `${command} must be rejected`);
  }
  assert.equal(isSafeCommand(""), false);
});

test("non-bash tools are not gated", () => {
  assert.equal(blockReason("plan", "grep", { pattern: "x" }), undefined);
  assert.equal(blockReason("plan", "read", { path: "x" }), undefined);
});

test("cleanStepText strips emphasis, verbs, and trailing punctuation", () => {
  assert.equal(cleanStepText("**Add** the `reducer`."), "reducer");
  assert.equal(cleanStepText("Run the tests"), "tests");
  assert.equal(cleanStepText("  Implement   the widget  "), "widget");
});

test("extractPlanSteps reads numbered steps under a Plan header", () => {
  const text = `Here is my plan.\n\nPlan:\n1. Read the code\n2. **Add** the reducer\n3. Run tests\n`;
  const steps = extractPlanSteps(text);
  assert.deepEqual(steps, [
    { step: 1, text: "code", done: false },
    { step: 2, text: "reducer", done: false },
    { step: 3, text: "tests", done: false },
  ]);
  // No header → no plan.
  assert.deepEqual(extractPlanSteps("1. one\n2. two"), []);
  // Bold header works.
  assert.equal(extractPlanSteps("**Plan:**\n1. x").length, 1);
});

test("markCompletedSteps applies [DONE:n] and reports changes", () => {
  const todos = [
    { step: 1, text: "one", done: false },
    { step: 2, text: "two", done: false },
  ];
  assert.equal(markCompletedSteps("did it [DONE:1]", todos), 1);
  assert.equal(todos[0].done, true);
  assert.equal(todos[1].done, false);
  // Idempotent.
  assert.equal(markCompletedSteps("[DONE:1]", todos), 0);
  assert.equal(markCompletedSteps("[done:2]", todos), 1);
  assert.equal(todos[1].done, true);
});

test("guidance differs per mode and lists remaining build steps", () => {
  assert.match(guidance("plan"), /PLAN MODE/);
  assert.match(guidance("ask"), /ASK MODE/);
  assert.equal(guidance("build", []), "");
  const build = guidance("build", [
    { step: 1, text: "one", done: true },
    { step: 2, text: "two", done: false },
  ]);
  assert.match(build, /EXECUTING PLAN/);
  assert.match(build, /2\. two/);
  assert.doesNotMatch(build, /1\. one/);
});
