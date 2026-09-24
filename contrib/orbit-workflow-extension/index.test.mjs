#!/usr/bin/env node
/**
 * Behaviour tests for the Orbit workflow extension entry.
 *
 * Run: node --test contrib/orbit-workflow-extension/index.test.mjs
 *
 * Drives the extension the way pi does — `activate(pi)`, then lifecycle hooks
 * — with a fake `pi` and a fake `ctx`, asserting tool scoping, guidance
 * injection, plan tracking, and the session entries Orbit reads.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { beforeEach } from "node:test";

import activate, { lastCustom, reconstructTodos, resolveMode } from "./index.js";

let home;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-workflow-entry-"));
  fs.mkdirSync(path.join(home, ".orbit-pi"), { recursive: true });
  process.env.HOME = home;
  delete process.env.ORBIT_WORKFLOW_MODE;
});

function writeStore(map) {
  fs.writeFileSync(path.join(home, ".orbit-pi", "workflow.json"), JSON.stringify(map));
}

function fakePi() {
  const handlers = new Map();
  const state = { tools: ["read", "bash", "edit", "write", "grep"], entries: [] };
  return {
    state,
    on(event, handler) {
      handlers.set(event, handler);
    },
    fire(event, payload, ctx) {
      const handler = handlers.get(event);
      assert.ok(handler, `no handler for ${event}`);
      return handler(payload, ctx);
    },
    getActiveTools() {
      return [...state.tools];
    },
    setActiveTools(names) {
      state.tools = [...names];
    },
    appendEntry(customType, data) {
      state.entries.push({ type: "custom", customType, data });
    },
  };
}

function fakeCtx(sessionId = "s1", branch = [], entries = branch) {
  return {
    hasUI: true,
    sessionManager: {
      getSessionId: () => sessionId,
      getBranch: () => branch,
      getEntries: () => entries,
    },
  };
}

function assistant(text) {
  return { role: "assistant", content: [{ type: "text", text }] };
}

test("plan mode disables write tools on session start", async () => {
  writeStore({ s1: "plan" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx());
  assert.deepEqual(pi.state.tools, ["read", "bash", "grep"]);
  const modeEntry = pi.state.entries.find((e) => e.customType === "orbit:workflow");
  assert.equal(modeEntry.data.mode, "plan");
});

test("build keeps every tool", async () => {
  writeStore({ s1: "build" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx());
  assert.deepEqual(pi.state.tools, ["read", "bash", "edit", "write", "grep"]);
});

test("before_agent_start injects guidance and re-arms the mode live", async () => {
  writeStore({ s1: "plan" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx());
  const result = await pi.fire("before_agent_start", {}, fakeCtx());
  assert.equal(result.message.customType, "orbit-workflow-context");
  assert.match(result.message.content, /PLAN MODE/);

  // The store changed underneath: the next hook re-reads it (live re-arm).
  writeStore({ s1: "build" });
  const next = await pi.fire("before_agent_start", {}, fakeCtx());
  assert.equal(next, undefined); // build with no plan adds nothing
  assert.deepEqual(pi.state.tools, ["read", "bash", "edit", "write", "grep"]);
});

test("tool_call blocks writes in read-only modes and passes in build", async () => {
  writeStore({ s1: "ask" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx());
  const blocked = await pi.fire("tool_call", { toolName: "edit", input: {} }, fakeCtx());
  assert.equal(blocked.block, true);
  const unsafe = await pi.fire(
    "tool_call",
    { toolName: "bash", input: { command: "rm -rf build" } },
    fakeCtx(),
  );
  assert.equal(unsafe.block, true);
  const safe = await pi.fire(
    "tool_call",
    { toolName: "bash", input: { command: "git status" } },
    fakeCtx(),
  );
  assert.equal(safe, undefined);

  writeStore({ s1: "build" });
  assert.equal(await pi.fire("tool_call", { toolName: "edit", input: {} }, fakeCtx()), undefined);
});

test("a Plan section becomes todos and [DONE:n] marks progress", async () => {
  writeStore({ s1: "build" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx());

  await pi.fire("turn_end", { message: assistant("Plan:\n1. Read the code\n2. Add tests") }, fakeCtx());
  let todoEntry = lastCustom(pi.state.entries, "orbit:workflow-todos");
  assert.deepEqual(
    todoEntry.todos.map((t) => t.done),
    [false, false],
  );

  await pi.fire("turn_end", { message: assistant("Done. [DONE:1]") }, fakeCtx());
  todoEntry = lastCustom(pi.state.entries, "orbit:workflow-todos");
  assert.deepEqual(
    todoEntry.todos.map((t) => t.done),
    [true, false],
  );

  // An unchanged message appends nothing further.
  const count = pi.state.entries.length;
  await pi.fire("turn_end", { message: assistant("Still here") }, fakeCtx());
  assert.equal(pi.state.entries.length, count);
});

test("[DONE:n] advances the plan even in Plan mode", async () => {
  writeStore({ s1: "plan" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx());
  await pi.fire("turn_end", { message: assistant("Plan:\n1. One\n2. Two") }, fakeCtx());
  await pi.fire("turn_end", { message: assistant("Marking [DONE:1] [DONE:2]") }, fakeCtx());
  const todoEntry = lastCustom(pi.state.entries, "orbit:workflow-todos");
  assert.deepEqual(
    todoEntry.todos.map((t) => t.done),
    [true, true],
  );
});

test("a resumed process rebuilds todos before marking", async () => {
  // session_start sees no branch (fresh process), but the session file still
  // holds the plan entry — the DONE message must rebuild it first.
  const planEntry = {
    type: "custom",
    customType: "orbit:workflow-todos",
    data: {
      todos: [
        { step: 1, text: "One", done: false },
        { step: 2, text: "Two", done: false },
      ],
    },
  };
  writeStore({ s1: "build" });
  const pi = fakePi();
  activate(pi);
  const ctx = fakeCtx("s1", [], [planEntry]);
  await pi.fire("session_start", {}, ctx);
  await pi.fire("turn_end", { message: assistant("[DONE:1]") }, ctx);
  const todoEntry = lastCustom(pi.state.entries, "orbit:workflow-todos");
  assert.deepEqual(
    todoEntry.todos.map((t) => t.done),
    [true, false],
  );
});

test("session_start replays past [DONE:n] and persists the correction", async () => {
  const planEntry = {
    type: "custom",
    customType: "orbit:workflow-todos",
    data: {
      todos: [
        { step: 1, text: "One", done: false },
        { step: 2, text: "Two", done: false },
      ],
    },
  };
  const branch = [
    planEntry,
    { type: "message", message: assistant("did [DONE:1] [DONE:2]") },
  ];
  writeStore({ s1: "build" });
  const pi = fakePi();
  activate(pi);
  await pi.fire("session_start", {}, fakeCtx("s1", branch));
  const todoEntry = lastCustom(pi.state.entries, "orbit:workflow-todos");
  assert.deepEqual(
    todoEntry.todos.map((t) => t.done),
    [true, true],
  );
});

test("the store for the session id wins over the env fallback", () => {
  writeStore({ s1: "ask" });
  process.env.ORBIT_WORKFLOW_MODE = "plan";
  assert.equal(resolveMode(fakeCtx("s1"), []), "ask");
  delete process.env.ORBIT_WORKFLOW_MODE;
  // No store entry: env applies.
  process.env.ORBIT_WORKFLOW_MODE = "plan";
  assert.equal(resolveMode(fakeCtx("missing"), []), "plan");
  delete process.env.ORBIT_WORKFLOW_MODE;
  // No store, no env: the session entry, then Build.
  const branch = [{ type: "custom", customType: "orbit:workflow", data: { mode: "ask" } }];
  assert.equal(resolveMode(fakeCtx("missing"), branch), "ask");
  assert.equal(resolveMode(fakeCtx("missing"), []), "build");
});

test("session reconstruction restores the newest todo snapshot", () => {
  const branch = [
    { type: "custom", customType: "orbit:workflow-todos", data: { todos: [{ step: 1, text: "old", done: false }] } },
    { type: "message" },
    { type: "custom", customType: "orbit:workflow-todos", data: { todos: [{ step: 1, text: "new", done: true }] } },
  ];
  assert.deepEqual(reconstructTodos(branch), [{ step: 1, text: "new", done: true }]);
  assert.deepEqual(reconstructTodos([]), []);
});

test("reconstruction replays [DONE:n] tagged after the plan entry", () => {
  const entries = [
    {
      type: "custom",
      customType: "orbit:workflow-todos",
      data: {
        todos: [
          { step: 1, text: "One", done: false },
          { step: 2, text: "Two", done: false },
        ],
      },
    },
    {
      type: "message",
      message: { role: "assistant", content: [{ type: "text", text: "did [DONE:1]" }] },
    },
  ];
  assert.deepEqual(
    reconstructTodos(entries).map((t) => t.done),
    [true, false],
  );
});

test("context drops stale guidance only when the mode adds none", async () => {
  writeStore({ s1: "build" });
  const pi = fakePi();
  activate(pi);
  const result = await pi.fire(
    "context",
    { messages: [{ customType: "orbit-workflow-context" }, { role: "user" }] },
    fakeCtx(),
  );
  assert.equal(result.messages.length, 1);
  // Nothing to filter → no transform.
  assert.equal(await pi.fire("context", { messages: [{ role: "user" }] }, fakeCtx()), undefined);
});

test("context keeps active guidance (Plan/Ask, or Build with steps)", async () => {
  writeStore({ s1: "plan" });
  const pi = fakePi();
  activate(pi);
  const kept = await pi.fire(
    "context",
    { messages: [{ customType: "orbit-workflow-context" }, { role: "user" }] },
    fakeCtx(),
  );
  assert.equal(kept, undefined);
});
