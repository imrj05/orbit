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

test("allowedTools keeps read-only built-ins and default-denies custom tools", () => {
  const base = ["read", "bash", "edit", "write", "grep", "find", "ls"];
  assert.deepEqual(allowedTools("plan", base), ["read", "bash", "grep", "find", "ls"]);
  assert.deepEqual(allowedTools("ask", base), ["read", "bash", "grep", "find", "ls"]);
  assert.deepEqual(allowedTools("build", base), base);
  // Duplicates collapse. Unknown, MCP, and tool-broker tools are not trusted
  // to be read-only; the guarded subagent remains available.
  assert.deepEqual(
    allowedTools("plan", [
      "read",
      "read",
      "custom_tool",
      "mcp__filesystem__read_file",
      "codemode",
      "tool_search",
      "subagent",
      "edit",
    ]),
    ["read", "subagent"],
  );
});

test("applyToolScoping keeps tools activated after session start", () => {
  // Session start: no codemode yet.
  let state = applyToolScoping("build", ["read", "edit", "write", "bash"], []);
  assert.deepEqual(state.tools, ["read", "edit", "write", "bash"]);
  // Pi's MCP extension activates `codemode` once a codemode server connects.
  const active = [...state.tools, "codemode"];
  // Plan disables the broker rather than trusting the tools it can reach.
  state = applyToolScoping("plan", active, state.removed);
  assert.deepEqual(state.tools, ["read", "bash"]);
  // Switching back restores exactly what this policy removed, including codemode.
  state = applyToolScoping("build", state.tools, state.removed);
  assert.deepEqual(state.tools, ["read", "bash", "edit", "write", "codemode"]);
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

test("Plan and Ask reject mutating utility options and subcommands", () => {
  const mutations = [
    "find /tmp/target -delete",
    "find . -exec custom-mutation {} +",
    "find . -execdir custom-mutation {} +",
    "find . -fprint /tmp/files",
    "sed -n '1w /tmp/x' Cargo.toml",
    "sed -n '1W /tmp/x' Cargo.toml",
    "sed -n 'e custom-mutation' Cargo.toml",
    "sed -n 's/foo/bar/e' Cargo.toml",
    "sed -n 's/foo/bar/w /tmp/x' Cargo.toml",
    "sed -n -e '1w /tmp/x' Cargo.toml",
    "sed -n -f script.sed Cargo.toml",
    "sed -n -i.bak 's/foo/bar/' Cargo.toml",
    "rg --pre 'sh mutate.sh' p .",
    "rg --pre=custom-mutation p .",
    "rg --hostname-bin=custom-mutation p .",
    "bat --paging=always --pager=custom-mutation Cargo.toml",
    "file -C -m /tmp/magic",
    "git ls-remote --upload-pack=custom-mutation /tmp/repo",
    "git diff --ext-diff HEAD",
    "sort $'-\\x6f' /tmp/out /etc/passwd",
    "sort $\"-o\" /tmp/out /etc/passwd",
    "fd -x touch x",
    "fd -x custom-mutation",
    "fd --exec custom-mutation",
    "fd --exec-batch custom-mutation",
    "fd -X custom-mutation",
    "fd -Hix custom-mutation",
    "env sh -c 'echo x'",
    "env custom-mutation",
    "env FOO=bar custom-mutation",
    "env --split-string=custom-mutation",
    "env -S custom-mutation",
    "sort --compress-program=custom-mutation input",
    "sort --compress-program custom-mutation input",
    "sort --compress-p=custom-mutation input",
    "sort -o output input",
    "sort --output=output input",
    "sort --out=output input",
    "sort -ro output input",
    "tree -o output.txt",
    "git branch topic",
    "git branch -m old new",
    "git branch --set-upstream-to=origin/main topic",
    "git branch --delete topic",
    "git remote add origin https://example.test/repo.git",
    "git remote set-url origin https://example.test/repo.git",
    "git remote remove origin",
    "git remote show --prune origin",
    "git remote show -p origin",
    "git remote rename old new",
    "git remote prune origin",
    "npm audit fix",
    "npm audit fix --force",
    "npm audit --fix",
    "NPM AUDIT FIX",
    "GIT branch topic",
    "git diff --output=changes.patch",
  ];
  for (const command of mutations) {
    assert.equal(isSafeCommand(command), false, `${command} must be rejected`);
  }
});

test("the parsed utility policy preserves established read-only forms", () => {
  const reads = [
    "find . -type f -print",
    "find . -name '*delete*' -print",
    "rg -n TODO src",
    "rg --hidden --glob '*.rs' TODO src",
    "rg -- --pre",
    "fd . src",
    "fd --type f --hidden . src",
    "fd -- --exec",
    "env",
    "env -0",
    "env -i PATH=/usr/bin",
    "env --ignore-environment FOO=bar",
    "env -u FOO",
    "env -- FOO=bar",
    "sort -u input",
    "sort --reverse input",
    "sort -- -o",
    "sort -- --compress-program=literal",
    "git branch",
    "git branch -a -vv",
    "git branch --list topic",
    "git branch --show-current",
    "git branch --contains HEAD",
    "git branch --format=%H --list",
    "git remote",
    "git remote -v",
    "git remote show origin",
    "git remote get-url --all origin",
    "git remote prune --dry-run origin",
    "npm audit --json",
    "npm audit signatures",
    "git diff HEAD",
  ];
  for (const command of reads) {
    assert.equal(isSafeCommand(command), true, `${command} must remain allowed`);
  }
});

test("read-only built-ins and guarded delegation pass; custom and broker tools fail closed", () => {
  for (const mode of ["plan", "ask"]) {
    assert.equal(blockReason(mode, "grep", { pattern: "x" }), undefined);
    assert.equal(blockReason(mode, "read", { path: "x" }), undefined);
    assert.equal(blockReason(mode, "find", { path: "." }), undefined);
    assert.equal(blockReason(mode, "ls", { path: "." }), undefined);
    assert.equal(blockReason(mode, "subagent", { agent: "scout", task: "map" }), undefined);
    for (const tool of [
      "codemode",
      "tool_search",
      "mcp__filesystem__write_file",
      "mcp__filesystem__read_file",
      "custom_tool",
    ]) {
      assert.match(blockReason(mode, tool, {}), /blocked/, `${mode} must block ${tool}`);
    }
  }
  // Build still leaves the access guard in charge of these tools.
  assert.equal(blockReason("build", "mcp__filesystem__write_file", {}), undefined);
  assert.equal(blockReason("build", "codemode", {}), undefined);
});

test("guidance differs per mode", () => {
  assert.match(guidance("plan"), /PLAN MODE/);
  assert.match(guidance("ask"), /ASK MODE/);
  assert.equal(guidance("build"), "");
});
