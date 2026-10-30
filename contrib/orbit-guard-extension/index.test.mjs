#!/usr/bin/env node
/**
 * Behaviour tests for the Orbit guard extension entry.
 *
 * Run: node --test contrib/orbit-guard-extension/index.test.mjs
 *
 * Drives the extension the way pi does — `activate(pi)`, then `tool_call`
 * events — with a fake `pi` and a fake `ctx.ui`, asserting exactly when a
 * tool call is prompted, allowed, or blocked.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { beforeEach } from "node:test";

import activate from "./index.js";
import { OPTION_ALLOW_ONCE, OPTION_ALWAYS_ALLOW, OPTION_DENY } from "./policy.js";

let home;
let selectCalls;
let selectResult;
let lastOptions;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-guard-entry-"));
  fs.mkdirSync(path.join(home, ".orbit-pi"), { recursive: true });
  selectCalls = 0;
  selectResult = OPTION_ALLOW_ONCE;
  lastOptions = [];
  process.env.HOME = home;
});

function setMode(mode) {
  fs.writeFileSync(path.join(home, ".orbit-pi", "access.json"), JSON.stringify({ mode }));
}

function allowFile() {
  return path.join(home, ".orbit-pi", "access-allow.json");
}

function fakePi() {
  const handlers = new Map();
  return {
    on(event, handler) {
      handlers.set(event, handler);
    },
    fire(event, payload, ctx) {
      const handler = handlers.get(event);
      assert.ok(handler, `no handler for ${event}`);
      return handler(payload, ctx);
    },
  };
}

function fakeCtx({ hasUI = true } = {}) {
  return {
    hasUI,
    ui: {
      select: async (title, options) => {
        selectCalls += 1;
        lastOptions = options;
        assert.ok(options.includes(OPTION_ALLOW_ONCE), "offers allow-once");
        assert.ok(options.includes(OPTION_DENY), "offers deny");
        assert.ok(title.startsWith("[orbit-guard] "), "title carries the marker");
        return selectResult;
      },
    },
  };
}

test("full-access never prompts", async () => {
  setMode("full-access");
  const pi = fakePi();
  activate(pi);
  const result = await pi.fire(
    "tool_call",
    { toolName: "bash", input: { command: "rm -rf build" } },
    fakeCtx(),
  );
  assert.equal(result, undefined);
  assert.equal(selectCalls, 0);
});

test("supervised allows reads and prompts for edits", async () => {
  setMode("supervised");
  const pi = fakePi();
  activate(pi);

  const read = await pi.fire("tool_call", { toolName: "read", input: { path: "a" } }, fakeCtx());
  assert.equal(read, undefined, "reads pass through");
  assert.equal(selectCalls, 0);

  const edit = await pi.fire("tool_call", { toolName: "edit", input: { path: "a" } }, fakeCtx());
  assert.equal(edit, undefined, "allowed once runs");
  assert.equal(selectCalls, 1);
});

test("a denied call blocks with a reason", async () => {
  setMode("supervised");
  selectResult = OPTION_DENY;
  const pi = fakePi();
  activate(pi);
  const result = await pi.fire(
    "tool_call",
    { toolName: "bash", input: { command: "sudo rm -rf /" } },
    fakeCtx(),
  );
  assert.equal(result.block, true);
  assert.match(result.reason, /Denied/);
});

test("always allow records a whole-file tool and stops prompting for it", async () => {
  setMode("supervised");
  selectResult = OPTION_ALWAYS_ALLOW;
  const pi = fakePi();
  activate(pi);

  await pi.fire("tool_call", { toolName: "edit", input: { path: "a" } }, fakeCtx());
  assert.equal(selectCalls, 1);
  assert.ok(lastOptions.includes(OPTION_ALWAYS_ALLOW), "edit may be granted");
  const written = JSON.parse(fs.readFileSync(allowFile(), "utf8"));
  assert.deepEqual(written.supervised, ["edit"]);

  // A second edit in the same mode is allowed without a prompt.
  const second = await pi.fire("tool_call", { toolName: "edit", input: { path: "b" } }, fakeCtx());
  assert.equal(second, undefined);
  assert.equal(selectCalls, 1, "no second prompt");
});

test("a command can never be granted for the whole session", async () => {
  // Approving `ls` once must not approve every later command: the button is not
  // offered for exec tools, and a legacy `["bash"]` grant in the file is ignored.
  setMode("supervised");
  selectResult = OPTION_ALWAYS_ALLOW;
  const pi = fakePi();
  activate(pi);

  const result = await pi.fire(
    "tool_call",
    { toolName: "bash", input: { command: "ls" } },
    fakeCtx(),
  );
  assert.ok(!lastOptions.includes(OPTION_ALWAYS_ALLOW), "no grant button for bash");
  assert.equal(result.block, true, "an unsupported choice is a denial, not an allow");
  assert.equal(fs.existsSync(allowFile()), false, "nothing recorded");

  // A grant written by an older Orbit does not survive either.
  fs.writeFileSync(allowFile(), JSON.stringify({ supervised: ["bash"] }));
  selectResult = OPTION_DENY;
  await pi.fire("tool_call", { toolName: "bash", input: { command: "ls" } }, fakeCtx());
  assert.equal(selectCalls, 2, "still asks after a stale grant");
});

test("auto-accept-edits skips the prompt for edits but asks for exec", async () => {
  setMode("auto-accept-edits");
  const pi = fakePi();
  activate(pi);

  const edit = await pi.fire("tool_call", { toolName: "write", input: { path: "a" } }, fakeCtx());
  assert.equal(edit, undefined);
  assert.equal(selectCalls, 0, "edits are not confirmed");

  await pi.fire("tool_call", { toolName: "bash", input: { command: "ls" } }, fakeCtx());
  assert.equal(selectCalls, 1, "exec is confirmed");
});

test("without a UI a confirmation-required call is blocked, not allowed", async () => {
  setMode("supervised");
  const pi = fakePi();
  activate(pi);
  const result = await pi.fire(
    "tool_call",
    { toolName: "edit", input: { path: "a" } },
    fakeCtx({ hasUI: false }),
  );
  assert.equal(result.block, true);
  assert.equal(selectCalls, 0);
});

test("a dismissed or throwing prompt fails closed", async () => {
  setMode("supervised");
  const pi = fakePi();
  activate(pi);

  selectResult = undefined; // Esc / cancel
  const dismissed = await pi.fire("tool_call", { toolName: "bash", input: { command: "ls" } }, fakeCtx());
  assert.equal(dismissed.block, true);

  const throwing = {
    hasUI: true,
    ui: {
      select: async () => {
        throw new Error("dialog cancelled");
      },
    },
  };
  const threw = await pi.fire("tool_call", { toolName: "bash", input: { command: "ls" } }, throwing);
  assert.equal(threw.block, true);
});

test("a missing mode file behaves as supervised, not full-access", async () => {
  // The file is absent, so the guard has no mode to enforce: the safe reading is
  // "ask", never "allow everything".
  const pi = fakePi();
  activate(pi);
  const result = await pi.fire("tool_call", { toolName: "bash", input: { command: "ls" } }, fakeCtx());
  assert.equal(selectCalls, 1, "it asked instead of allowing");
  assert.equal(result, undefined, "the answer was allow-once");
});

test("a corrupt mode file behaves as supervised, not full-access", async () => {
  fs.writeFileSync(path.join(home, ".orbit-pi", "access.json"), "{not json");
  const pi = fakePi();
  activate(pi);
  await pi.fire("tool_call", { toolName: "edit", input: { path: "a" } }, fakeCtx());
  assert.equal(selectCalls, 1, "a half-written file must not widen access");
});

test("parallel prompts are serialized, never stacked", async () => {
  setMode("supervised");
  let active = 0;
  let maxActive = 0;
  const ctx = {
    hasUI: true,
    ui: {
      select: async () => {
        active += 1;
        maxActive = Math.max(maxActive, active);
        await new Promise((resolve) => setTimeout(resolve, 5));
        active -= 1;
        return OPTION_ALLOW_ONCE;
      },
    },
  };
  const pi = fakePi();
  activate(pi);
  await Promise.all([
    pi.fire("tool_call", { toolName: "bash", input: { command: "a" } }, ctx),
    pi.fire("tool_call", { toolName: "bash", input: { command: "b" } }, ctx),
  ]);
  assert.equal(maxActive, 1, "only one prompt in flight at a time");
});
