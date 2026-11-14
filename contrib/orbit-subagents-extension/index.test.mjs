#!/usr/bin/env node
/**
 * Behaviour tests for the subagent tool entry.
 *
 * Run: node --test contrib/orbit-subagents-extension/index.test.mjs
 *
 * Drives the extension the way pi does — `activate(pi)`, then the registered
 * tool's `execute` — with a fake `pi`, a fake `spawn`, and a temporary home so
 * the real `~/.orbit-pi` files are never touched.
 */

import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test, { beforeEach } from "node:test";

import activate, { PARAMETERS, PROMPT_SNIPPET, executeTool, parseRequest, runPool } from "./index.js";

let home;

beforeEach(() => {
  home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-subagents-entry-"));
  fs.mkdirSync(path.join(home, ".orbit-pi"), { recursive: true });
  fs.mkdirSync(path.join(home, ".pi", "agent", "agents"), { recursive: true });
});

function writeOrbitFile(name, value) {
  fs.writeFileSync(path.join(home, ".orbit-pi", name), JSON.stringify(value));
}

/**
 * A fake pi session: records registrations and the handlers it is given, so a
 * test can drive `session_start` the way pi does.
 */
function fakePi(overrides = {}) {
  const tools = [];
  const handlers = new Map();
  const pi = {
    tools,
    handlers,
    getAllTools: overrides.getAllTools ?? (() => []),
    registerTool(definition) {
      tools.push(definition);
    },
    on(event, handler) {
      handlers.set(event, [...(handlers.get(event) ?? []), handler]);
    },
    ...overrides,
  };
  /** Fire an event the way pi's runner does: every handler, in order. */
  pi.fire = (event, ctx) => {
    for (const handler of handlers.get(event) ?? []) handler({ type: event }, ctx);
  };
  return pi;
}

/** Activate and run the first session, which is where registration happens. */
function startSession(pi, spawn = fakeSpawn(), eventCtx = { ui: { notify() {} } }) {
  const result = activate(pi, deps(spawn));
  pi.fire?.("session_start", eventCtx);
  return result;
}

/**
 * A fake `pi` process. `script(child, argv, options)` runs on the next tick and
 * is responsible for writing stdout and closing.
 */
function fakeSpawn(script = () => {}) {
  const calls = [];
  const spawn = (bin, argv, options) => {
    const child = new EventEmitter();
    child.pid = 4242 + calls.length;
    child.stdout = new EventEmitter();
    child.stderr = new EventEmitter();
    child.killed = [];
    child.kill = (signal) => {
      child.killed.push(signal);
      child.emit("close", null);
    };
    calls.push({ bin, argv, options, child });
    setTimeout(() => script(child, argv, options), 0);
    return child;
  };
  spawn.calls = calls;
  return spawn;
}

function deps(spawn, extra = {}) {
  return {
    spawn,
    home,
    tempDir: home,
    env: { PATH: process.env.PATH ?? "" },
    now: () => 1000,
    piBinary: "pi",
    ...extra,
  };
}

function ctx(sessionId = "session-a", cwd = "/repo") {
  return { cwd, sessionManager: { getSessionId: () => sessionId } };
}

function stream(child, events) {
  child.stdout.emit("data", Buffer.from(events.map((event) => `${JSON.stringify(event)}\n`).join(""), "utf8"));
}

function successfulChild(child) {
  stream(child, [
    { type: "session", cwd: "/repo" },
    { type: "message_start", message: { role: "assistant", model: "sonnet" } },
    { type: "tool_execution_start", toolName: "grep", args: { pattern: "auth" } },
    { type: "tool_execution_end", toolName: "grep", args: { pattern: "auth" } },
    {
      type: "message_end",
      message: {
        role: "assistant",
        content: [{ type: "text", text: "auth is in src/auth.rs" }],
        usage: { input: 1000, output: 200, cost: { total: 0.01 } },
        stopReason: "stop",
      },
    },
    { type: "agent_settled", aborted: false },
  ]);
  child.emit("close", 0);
}

async function callTool(pi, spawn, params, { signal, onUpdate, sessionId = "session-a", cwd = "/repo", counts = new Map() } = {}) {
  return executeTool({
    params,
    signal,
    onUpdate,
    ctx: ctx(sessionId, cwd),
    pi,
    deps: deps(spawn),
    counts,
  });
}

test("activation registers nothing until the session starts", () => {
  const pi = fakePi();
  const result = activate(pi, deps(fakeSpawn()));
  assert.equal(result.active, true);
  assert.equal(result.deferred, true);
  // Registering at load is what made pi exit on a tool conflict (issue #122).
  assert.equal(pi.tools.length, 0);
  pi.fire("session_start", { ui: { notify() {} } });
  assert.equal(pi.tools.length, 1);
});

test("activation stands down when another subagent tool exists", () => {
  const notices = [];
  const pi = fakePi({
    getAllTools: () => [
      { name: "subagent", sourceInfo: { path: "/home/me/.pi/agent/extensions/pi-subagents/index.js" } },
    ],
  });
  const result = activate(pi, deps(fakeSpawn()));
  pi.fire("session_start", { ui: { notify: (message) => notices.push(message) } });
  assert.equal(result.active, true);
  assert.equal(pi.tools.length, 0);
  assert.match(notices.join("\n"), /another extension .* provides a `subagent` tool/);
  // The toast names the file, so the user can find the extension to switch off.
  assert.match(notices.join("\n"), /pi-subagents\/index\.js/);
});

test("a throwing tool list at load no longer disables the guard", () => {
  // The regression behind issue #122: during load `getAllTools()` throws
  // ("Extension runtime not initialized"). Reading that as an empty list
  // registered us second, and pi exited on the conflict. The same call answers
  // once the session is bound, so the decision must wait for it.
  let bound = false;
  const pi = fakePi({
    getAllTools: () => {
      if (!bound) throw new Error("Extension runtime not initialized. Action methods cannot be called during extension loading.");
      return [];
    },
  });
  activate(pi, deps(fakeSpawn()));
  assert.equal(pi.tools.length, 0, "an unreadable list is not an empty one");
  bound = true;
  pi.fire("session_start", { ui: { notify() {} } });
  assert.equal(pi.tools.length, 1);
});

test("a later session decides again, so one unreadable list is not permanent", () => {
  // The owner can appear or disappear between sessions, and a decision cached
  // for the life of the process would leave the tool off long after the cause
  // was gone — while the page still showed a stand-down banner.
  const notices = [];
  let readable = false;
  const pi = fakePi({
    getAllTools: () => {
      if (!readable) throw new Error("Extension runtime not initialized.");
      return [];
    },
  });
  activate(pi, deps(fakeSpawn()));
  const ctx = { ui: { notify: (message) => notices.push(message) } };
  pi.fire("session_start", ctx);
  assert.equal(pi.tools.length, 0);
  assert.match(notices[0], /tool list could not be read/);
  assert.doesNotMatch(notices[0], /another extension/, "an unreadable list is not a foreign owner");

  readable = true;
  pi.fire("session_start", ctx);
  assert.equal(pi.tools.length, 1, "the next session registers");
  const status = JSON.parse(
    fs.readFileSync(path.join(home, ".orbit-pi", "subagents-status.json"), "utf8"),
  );
  assert.equal(status.active, true, "and the stand-down banner is cleared");
});

test("an owner that reports no file is named in words, not invented", () => {
  // `sourceInfo.path` is how we tell our own registration from a foreign one;
  // when it is absent the tool is still not ours, but there is no file to show.
  const pi = fakePi({ getAllTools: () => [{ name: "subagent" }] });
  activate(pi, deps(fakeSpawn()));
  pi.fire("session_start", { ui: { notify() {} } });
  assert.equal(pi.tools.length, 0);
  const status = JSON.parse(
    fs.readFileSync(path.join(home, ".orbit-pi", "subagents-status.json"), "utf8"),
  );
  assert.equal(status.active, false);
  assert.equal(status.owner, null, "no path was reported, so none is claimed");
  assert.match(status.reason, /did not report its file/);
});

test("a registration that another extension shadows is reported, not pretended", () => {
  // First-wins: pi keeps whichever extension registered the name first, so a
  // tool of ours with the same name can be inert even with no error at all.
  let registered = false;
  const notices = [];
  const pi = fakePi({
    getAllTools: () =>
      registered ? [{ name: "subagent", sourceInfo: { path: "/somewhere/else/index.js" } }] : [],
  });
  pi.registerTool = (definition) => {
    registered = true;
    pi.tools.push(definition);
  };
  activate(pi, deps(fakeSpawn()));
  pi.fire("session_start", { ui: { notify: (message) => notices.push(message) } });
  assert.equal(pi.tools.length, 1);
  assert.match(notices.join("\n"), /inactive/);
});

test("the decision reaches the file Settings → Subagents reads", () => {
  const statusFile = path.join(home, ".orbit-pi", "subagents-status.json");

  // Free name: reported as active, so an old stand-down cannot outlive the
  // cause of it.
  const free = fakePi();
  startSession(free);
  let status = JSON.parse(fs.readFileSync(statusFile, "utf8"));
  assert.equal(status.active, true);
  assert.equal(status.tool, "subagent");

  // A foreign owner: reported inactive, naming the file that owns the name.
  const owner = "/home/me/.pi/agent/extensions/pi-subagents/index.js";
  const taken = fakePi({ getAllTools: () => [{ name: "subagent", sourceInfo: { path: owner } }] });
  startSession(taken);
  status = JSON.parse(fs.readFileSync(statusFile, "utf8"));
  assert.equal(status.active, false);
  assert.equal(status.owner, owner);
  assert.match(status.reason, /another extension/);
});

test("a status file that cannot be written never fails the session", () => {
  const readOnly = path.join(home, "read-only");
  fs.mkdirSync(readOnly, { recursive: true });
  fs.chmodSync(readOnly, 0o500);
  const pi = fakePi();
  const result = activate(pi, deps(fakeSpawn(), { home: readOnly }));
  pi.fire("session_start", { ui: { notify() {} } });
  assert.equal(result.active, true);
  assert.equal(pi.tools.length, 1, "registration does not depend on the write");
  fs.chmodSync(readOnly, 0o700);
});

test("activation registers one tool with the fixed schema", () => {
  const pi = fakePi();
  startSession(pi);
  assert.equal(pi.tools.length, 1);
  const [definition] = pi.tools;
  assert.equal(definition.name, "subagent");
  assert.match(definition.description, /scout\s+read only/);
  assert.match(definition.description, /worker\s+runs commands/);
  assert.ok(PARAMETERS.properties.agent && PARAMETERS.properties.chain && PARAMETERS.properties.tasks);
  assert.equal(typeof definition.execute, "function");
  // The prompt surface: a `<tools>` line and `<rules>` bullets, so the tool is
  // visible before the model starts reading rather than only in its schema.
  assert.equal(definition.promptSnippet, PROMPT_SNIPPET);
  assert.ok(Array.isArray(definition.promptGuidelines));
  assert.ok(definition.promptGuidelines.length >= 1);
  for (const guideline of definition.promptGuidelines) {
    assert.match(guideline, /subagent/, "pi appends these flat, so each names the tool");
  }
  assert.ok(definition.promptGuidelines.some((line) => line.includes('"reviewer"')));
  assert.ok(definition.promptGuidelines.some((line) => line.includes('"scout"')));
});

test("a disabled agent drops its prompt guideline", () => {
  writeOrbitFile("subagents.json", { disabled: ["reviewer"] });
  const pi = fakePi();
  startSession(pi);
  const [definition] = pi.tools;
  assert.ok(definition.promptGuidelines.every((line) => !line.includes('"reviewer"')));
  assert.ok(definition.promptGuidelines.some((line) => line.includes('"scout"')));
});

test("activation re-registers only when the catalog changes", () => {
  const pi = fakePi();
  startSession(pi);
  assert.equal(pi.tools.length, 1);
  // A second session with the same catalog is a no-op: the prompt surface is
  // identical, so a single registration is the expected cost (D-S).
  pi.fire("session_start", { ui: { notify() {} } });
  assert.equal(pi.tools.length, 1);
});

test("one request shape exactly", () => {
  assert.equal(parseRequest({ agent: "scout", task: "x" }).mode, "single");
  assert.equal(parseRequest({ chain: [{ agent: "scout", task: "x" }] }).mode, "chain");
  assert.equal(parseRequest({ tasks: [{ agent: "scout", task: "x" }] }).mode, "parallel");
  assert.match(parseRequest({}).error, /exactly one/);
  assert.match(
    parseRequest({ agent: "scout", task: "x", tasks: [{ agent: "scout", task: "y" }] }).error,
    /exactly one/,
  );
  assert.match(parseRequest({ agent: "scout" }).error, /task` is required/);
  assert.match(parseRequest({ chain: [{ agent: "scout" }] }).error, /Step 1 is missing a task/);
});

test("a single delegation returns the report, details and usage", async () => {
  const spawn = fakeSpawn(successfulChild);
  const pi = fakePi();
  const result = await callTool(pi, spawn, { agent: "scout", task: "find auth" });

  assert.equal(spawn.calls.length, 1);
  const [call] = spawn.calls;
  assert.equal(call.bin, "pi");
  assert.deepEqual(call.argv.slice(0, 4), ["--mode", "json", "-p", "--no-session"]);
  assert.ok(call.argv.includes("--tools"));
  assert.deepEqual(call.argv.slice(-1), ["Task: find auth"]);
  assert.equal(call.options.cwd, "/repo");
  assert.equal(call.options.env.ORBIT_SUBAGENT_CEILING, "read");
  assert.equal(call.options.env.ORBIT_SUBAGENT_DEPTH, "1");
  assert.equal(call.options.env.ORBIT_WORKFLOW_MODE, "build");

  assert.match(result.content[0].text, /scout — done/);
  assert.match(result.content[0].text, /auth is in src\/auth\.rs/);
  assert.equal(result.details.orbit, "subagent/1");
  assert.equal(result.details.mode, "single");
  assert.equal(result.details.children[0].phase, "settled");
  assert.equal(result.details.children[0].tools, 1);
  assert.deepEqual(result.details.children[0].toolCounts, { grep: 1 });
  assert.equal(result.details.children[0].usage.input, 1000);
  assert.equal(result.usage.input, 1000);
  assert.equal(result.usage.turns, 1);
});

test("inherited guardrails become one --extension per path, never doubled", async () => {
  const spawn = fakeSpawn(successfulChild);
  const guard = "/home/.orbit-pi/guard-extension/index.js";
  const workflow = "/home/.orbit-pi/workflow-extension/index.js";
  await executeTool({
    params: { agent: "scout", task: "find auth" },
    ctx: ctx(),
    pi: fakePi(),
    deps: deps(spawn, {
      env: {
        PATH: process.env.PATH ?? "",
        ORBIT_BUNDLED_EXTENSIONS: JSON.stringify([guard, workflow]),
      },
    }),
    counts: new Map(),
  });

  const [call] = spawn.calls;
  const first = call.argv.indexOf("--extension");
  assert.deepEqual(call.argv.slice(first, first + 4), [
    "--extension",
    guard,
    "--extension",
    workflow,
  ]);
  assert.equal(call.argv.filter((value) => value === "--extension").length, 2);
});

test("the prompt file holds the body and both fixed blocks, then is removed", async () => {
  let promptPath = null;
  let promptText = null;
  const spawn = fakeSpawn((child, argv) => {
    const index = argv.indexOf("--append-system-prompt");
    promptPath = argv[index + 1];
    promptText = fs.readFileSync(promptPath, "utf8");
    successfulChild(child);
  });
  const pi = fakePi();
  await callTool(pi, spawn, { agent: "scout", task: "find auth" });

  assert.match(promptText, /You are a scout\./);
  assert.match(promptText, /permission scope was fixed/);
  assert.match(promptText, /does not receive your transcript/);
  assert.ok(!promptText.includes("Task: find auth"));
  // The prompt lives in a directory of its own, so a predictable file name in
  // the shared temp dir cannot be pre-created or shared with another child.
  assert.match(path.basename(path.dirname(promptPath)), /^orbit-subagent-/);
  assert.equal(fs.existsSync(promptPath), false);
  assert.equal(fs.existsSync(path.dirname(promptPath)), false);
});

test("an unknown agent is refused with the available names", async () => {
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "nosuch", task: "x" });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /Agent "nosuch" not found; available: .*scout/);
  assert.equal(result.details.children[0].phase, "refused");
});

test("a disabled agent is refused before spawning", async () => {
  writeOrbitFile("subagents.json", { disabled: ["scout"] });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "scout", task: "x" });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /disabled in Settings/);
});

test("subagents can be switched off entirely", async () => {
  writeOrbitFile("subagents.json", { enabled: false });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "scout", task: "x" });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /disabled in Settings → Subagents/);
});

test("an impossible delegation is refused before it spends anything (D-Z)", async () => {
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "worker", task: "fix it" });

  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /worker is a command-running agent; this session is Build · Supervised/);
  assert.match(result.content[0].text, /Full access/);
  assert.equal(result.usage.input, 0);
});

test("Plan caps an editing agent even under Full access", async () => {
  writeOrbitFile("access.json", { mode: "full-access" });
  writeOrbitFile("workflow.json", { "session-a": "plan" });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "worker", task: "fix it" });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /Plan · Full access/);
});

test("a read-only agent runs in Plan and is told the mode", async () => {
  writeOrbitFile("workflow.json", { "session-a": "plan" });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "scout", task: "map it" });
  assert.equal(spawn.calls.length, 1);
  assert.equal(result.details.children[0].workflowMode, "plan");
  assert.equal(spawn.calls[0].options.env.ORBIT_WORKFLOW_MODE, "plan");
});

test("an agent's pinned mode is used for the child", async () => {
  writeOrbitFile("subagents.json", { agents: { reviewer: { mode: "plan" } } });
  const spawn = fakeSpawn(successfulChild);
  await callTool(fakePi(), spawn, { agent: "reviewer", task: "review" });
  assert.equal(spawn.calls[0].options.env.ORBIT_WORKFLOW_MODE, "plan");
});

test("a depth-1 child cannot delegate again", async () => {
  const spawn = fakeSpawn(successfulChild);
  const pi = fakePi();
  const result = await executeTool({
    params: { agent: "scout", task: "x" },
    ctx: ctx(),
    pi,
    deps: deps(spawn, { env: { ORBIT_SUBAGENT_DEPTH: "1" } }),
  });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /depth limit 1/);
});

test("the run and session caps are enforced with a readable message", async () => {
  writeOrbitFile("subagents.json", { maxChildrenPerSession: 1 });
  const spawn = fakeSpawn(successfulChild);
  const pi = fakePi();
  const counts = new Map();
  await callTool(pi, spawn, { agent: "scout", task: "one" }, { counts });
  const second = await callTool(pi, spawn, { agent: "scout", task: "two" }, { counts });
  assert.equal(spawn.calls.length, 1);
  assert.match(second.content[0].text, /limit is 1/);
});

test("a chain substitutes {previous} and stops when a step fails", async () => {
  let index = 0;
  const spawn = fakeSpawn((child) => {
    index += 1;
    if (index === 1) {
      stream(child, [
        {
          type: "message_end",
          message: { role: "assistant", content: [{ type: "text", text: "first report" }], stopReason: "stop" },
        },
        { type: "agent_settled", aborted: false },
      ]);
      child.emit("close", 0);
      return;
    }
    stream(child, [
      { type: "message_end", message: { role: "assistant", content: [], stopReason: "error", errorMessage: "boom" } },
      { type: "agent_settled", aborted: false },
    ]);
    child.emit("close", 0);
  });

  const result = await callTool(fakePi(), spawn, {
    chain: [
      { agent: "scout", task: "find auth" },
      { agent: "scout", task: "now expand: {previous}" },
      { agent: "scout", task: "third" },
    ],
  });

  assert.equal(spawn.calls.length, 2);
  assert.deepEqual(spawn.calls[1].argv.slice(-1), ["Task: now expand: first report"]);
  assert.equal(result.details.mode, "chain");
  assert.equal(result.details.children.length, 2);
  assert.equal(result.details.children[0].status, "done");
  assert.equal(result.details.children[1].status, "failed");
});

test("parallel tasks never exceed the concurrency limit", async () => {
  let active = 0;
  let peak = 0;
  const spawn = fakeSpawn((child) => {
    active += 1;
    peak = Math.max(peak, active);
    setTimeout(() => {
      active -= 1;
      successfulChild(child);
    }, 5);
  });

  const tasks = ["a", "b", "c", "d", "e"].map((task) => ({ agent: "scout", task }));
  const result = await callTool(fakePi(), spawn, { tasks });
  assert.equal(spawn.calls.length, 5);
  assert.ok(peak <= 4, `peak concurrency ${peak}`);
  assert.equal(result.details.mode, "parallel");
  assert.equal(result.details.children.length, 5);
});

test("runPool keeps the output order", async () => {
  const results = await runPool([1, 2, 3], 2, async (value) => {
    await new Promise((resolve) => setTimeout(resolve, value === 1 ? 5 : 0));
    return value * 2;
  });
  assert.deepEqual(results, [2, 4, 6]);
});

test("abort kills the child's process group and reports aborted", async () => {
  const controller = new AbortController();
  const spawn = fakeSpawn(() => {
    // Never produces an assistant message: only the abort ends this run.
  });
  const pi = fakePi();
  const running = executeTool({
    params: { agent: "scout", task: "long" },
    signal: controller.signal,
    ctx: ctx(),
    pi,
    deps: deps(spawn),
  });
  await new Promise((resolve) => setTimeout(resolve, 5));
  controller.abort();
  const result = await running;

  assert.equal(spawn.calls.length, 1);
  assert.ok(spawn.calls[0].child.killed.includes("SIGTERM"));
  assert.equal(result.details.children[0].status, "aborted");
  assert.equal(result.details.children[0].phase, "killed");
});

test("progress updates are coalesced", async () => {
  const updates = [];
  const spawn = fakeSpawn((child) => {
    const events = [{ type: "message_start", message: { role: "assistant" } }];
    for (let i = 0; i < 50; i += 1) {
      events.push({ type: "tool_execution_start", toolName: "grep", args: { pattern: `p${i}` } });
    }
    events.push({
      type: "message_end",
      message: { role: "assistant", content: [{ type: "text", text: "done" }], stopReason: "stop" },
    });
    events.push({ type: "agent_settled", aborted: false });
    // One chunk: every event arrives inside one coalescing window.
    stream(child, events);
    child.emit("close", 0);
  });

  await callTool(fakePi(), spawn, { agent: "scout", task: "grep a lot" }, {
    onUpdate: (update) => updates.push(update),
  });

  assert.ok(updates.length >= 1);
  assert.ok(updates.length <= 4, `expected few coalesced updates, got ${updates.length}`);
  assert.equal(updates.at(-1).details.children[0].status, "done");
});

test("an already-aborted turn never spawns a child", async () => {
  const controller = new AbortController();
  controller.abort();
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "scout", task: "x" }, {
    signal: controller.signal,
  });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /aborted before the delegation started/);
});

test("a failing progress sink never fails the delegation", async () => {
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "scout", task: "x" }, {
    onUpdate: () => {
      throw new Error("sink exploded");
    },
  });
  assert.match(result.content[0].text, /auth is in src\/auth\.rs/);
});

test("a spawn failure is a readable failed run, not a throw", async () => {
  const spawn = () => {
    throw new Error("spawn ENOENT");
  };
  const result = await callTool(fakePi(), spawn, { agent: "scout", task: "x" });
  assert.equal(result.details.children[0].status, "failed");
  assert.match(result.content[0].text, /could not start pi: spawn ENOENT/);
});

test("a Permissions allow rule lets a worker run under Supervised", async () => {
  // The rule is what the Settings → Permissions page writes: unconditional, so
  // the pre-flight can promise the run rather than refusing it up front.
  writeOrbitFile("access-rules.json", {
    version: 1,
    rules: [{ scope: "agent:worker", target: "exec", pattern: "", action: "allow" }],
  });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "worker", task: "fix it" });
  assert.equal(spawn.calls.length, 1, "the child started");
  assert.doesNotMatch(result.content[0].text, /command-running agent/);
});

test("a pattern rule does not promise a tier the run cannot finish", async () => {
  writeOrbitFile("access-rules.json", {
    version: 1,
    rules: [{ scope: "agent:worker", target: "exec", pattern: "git status*", action: "allow" }],
  });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "worker", task: "fix it" });
  assert.equal(spawn.calls.length, 0, "still refused up front");
  assert.match(result.content[0].text, /Settings → Permissions/);
});

test("a rule never widens Plan mode", async () => {
  writeOrbitFile("workflow.json", { "session-a": "plan" });
  writeOrbitFile("access-rules.json", {
    version: 1,
    rules: [{ scope: "any", target: "any", pattern: "", action: "allow" }],
  });
  const spawn = fakeSpawn(successfulChild);
  const result = await callTool(fakePi(), spawn, { agent: "worker", task: "fix it" });
  assert.equal(spawn.calls.length, 0);
  assert.match(result.content[0].text, /Plan/);
});
