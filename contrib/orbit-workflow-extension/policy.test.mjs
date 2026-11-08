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
  applyToolScoping,
  blockReason,
  guidance,
  isSafeCommand,
  normalizeMode,
  policyTools,
  wasScopedReadOnly,
  WORKFLOW_ENTRY_TYPE,
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

test("applyToolScoping keeps tools activated after session start", () => {
  // Session start: no codemode yet.
  let state = applyToolScoping("build", ["read", "edit", "write", "bash"], []);
  assert.deepEqual(state.tools, ["read", "edit", "write", "bash"]);
  // Pi's MCP extension activates `codemode` once a codemode server connects.
  const active = [...state.tools, "codemode"];
  // A mode change must not drop a tool this extension does not own.
  state = applyToolScoping("plan", active, state.removed);
  assert.deepEqual(state.tools, ["read", "bash", "codemode"]);
  // Switching back restores the writes and still keeps codemode.
  state = applyToolScoping("build", state.tools, state.removed);
  assert.deepEqual(state.tools, ["read", "bash", "codemode", "edit", "write"]);
  // Nothing is left to restore once the mode allows everything.
  assert.deepEqual(state.removed, []);
});

test("applyToolScoping restores only what it removed", () => {
  const state = applyToolScoping("plan", ["read", "edit"], []);
  assert.deepEqual(state.tools, ["read"]);
  assert.deepEqual(state.removed.sort(), ["edit"]);
  // `grep` was never active (another extension removed it); this extension
  // must not resurrect it when the mode changes.
  const next = applyToolScoping("build", ["read", "codemode"], state.removed);
  assert.deepEqual(next.tools, ["read", "codemode", "edit"]);
  assert.deepEqual(next.removed, []);
});

test("a scoped-out tool set survives a resume, so Build restores the policy's own tools", () => {
  // The resumed process starts with the restricted set pi left behind: no
  // memory of the removal, and nothing to restore from (issue #109).
  const resumed = applyToolScoping("build", ["read", "bash"], [], ["edit", "write"]);
  assert.deepEqual(resumed.tools, ["read", "bash", "edit", "write"]);

  // An already-active name is not duplicated, and a read-only mode still
  // keeps the policy's tools off whatever it is offered.
  assert.deepEqual(
    applyToolScoping("build", ["read", "edit"], [], ["edit", "write"]).tools,
    ["read", "edit", "write"],
  );
  assert.deepEqual(
    applyToolScoping("plan", ["read", "bash"], [], ["edit", "write"]).tools,
    ["read", "bash"],
  );
});

test("only a session this extension scoped read-only may be restored", () => {
  const entry = (mode, customType = WORKFLOW_ENTRY_TYPE) => ({
    type: "custom",
    customType,
    data: { mode },
  });
  assert.equal(wasScopedReadOnly([entry("plan")]), true);
  assert.equal(wasScopedReadOnly([entry("build"), entry("ask")]), true);
  assert.equal(wasScopedReadOnly([entry("build")]), false);
  assert.equal(wasScopedReadOnly([entry("plan", "someone-else")]), false);
  assert.equal(wasScopedReadOnly([]), false);
  assert.equal(wasScopedReadOnly(undefined), false);
  assert.deepEqual(policyTools(), ["edit", "write"]);
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

test("guidance differs per mode", () => {
  assert.match(guidance("plan"), /PLAN MODE/);
  assert.match(guidance("ask"), /ASK MODE/);
  assert.equal(guidance("build"), "");
});
