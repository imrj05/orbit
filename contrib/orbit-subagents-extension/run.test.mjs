#!/usr/bin/env node
/**
 * Tests for arg/env/prompt construction and the JSONL fold.
 *
 * Run: node --test contrib/orbit-subagents-extension/run.test.mjs
 */

import assert from "node:assert/strict";
import test from "node:test";

import {
  BOUNDS,
  GUIDANCE_BLOCK,
  LineDecoder,
  SCOPE_BLOCK,
  addUsage,
  applyEvent,
  assemblePrompt,
  buildArgv,
  buildEnv,
  capBytes,
  childDetails,
  emptyUsage,
  finalizeChild,
  formatUsage,
  newChildState,
  parseRecord,
  runContent,
  runDetails,
  totalUsage,
} from "./run.js";

const agent = {
  name: "scout",
  source: "builtin",
  ceiling: "read",
  tools: ["read", "grep", "find", "ls"],
  toolsPresent: true,
  body: "You are a scout.",
  model: null,
  thinking: null,
};

function child(overrides = {}) {
  return {
    ...newChildState({
      id: "c1",
      agent,
      task: "find auth",
      cwd: "/repo",
      workflowMode: "build",
      now: 1000,
    }),
    ...overrides,
  };
}

test("the prompt is the agent body plus the two fixed blocks", () => {
  const prompt = assemblePrompt({ body: "You are a scout." });
  assert.ok(prompt.startsWith("You are a scout."));
  assert.ok(prompt.includes(SCOPE_BLOCK));
  assert.ok(prompt.includes(GUIDANCE_BLOCK));
  assert.ok(prompt.indexOf(SCOPE_BLOCK) < prompt.indexOf(GUIDANCE_BLOCK));
});

test("argv always pins json mode, no session, approval and a task", () => {
  const argv = buildArgv({
    promptPath: "/tmp/p.md",
    prompt: "Task: find auth",
    extensions: ["/a/guard.js", "/a/workflow.js"],
    model: "anthropic/claude-haiku-4-5",
    thinking: "high",
    tools: ["read", "grep"],
    toolsPresent: true,
    ceiling: "read",
  });
  assert.deepEqual(argv.slice(0, 4), ["--mode", "json", "-p", "--no-session"]);
  assert.ok(argv.includes("--model"));
  assert.ok(argv.includes("--thinking"));
  assert.deepEqual(argv.slice(argv.indexOf("--tools"), argv.indexOf("--tools") + 2), [
    "--tools",
    "read,grep",
  ]);
  assert.deepEqual(argv.slice(argv.indexOf("--exclude-tools"), argv.indexOf("--exclude-tools") + 2), [
    "--exclude-tools",
    "mcp__*",
  ]);
  assert.equal(argv.filter((value) => value === "--extension").length, 2);
  assert.ok(argv.includes("--approve"));
  assert.deepEqual(argv.slice(-4), ["--append-system-prompt", "/tmp/p.md", "--", "Task: find auth"]);
});

test("an exec ceiling keeps MCP tools declared and omits exclude-tools", () => {
  const argv = buildArgv({ promptPath: "/tmp/p.md", prompt: "Task: x", ceiling: "exec" });
  assert.ok(!argv.includes("--exclude-tools"));
});

test("an absent tools line passes no tool flag; an empty one disables tools", () => {
  const absent = buildArgv({ promptPath: "/tmp/p.md", prompt: "Task: x", tools: null, toolsPresent: false });
  assert.ok(!absent.includes("--tools"));
  const empty = buildArgv({ promptPath: "/tmp/p.md", prompt: "Task: x", tools: [], toolsPresent: true });
  assert.ok(empty.includes("--no-tools"));
});

test("the child env carries every guardrail", () => {
  const env = buildEnv({
    parentSessionId: "s1",
    agentName: "worker",
    ceiling: "exec",
    workflowMode: "plan",
  });
  assert.equal(env.ORBIT_WORKFLOW_MODE, "plan");
  assert.equal(env.ORBIT_SUBAGENT_CEILING, "exec");
  assert.equal(env.ORBIT_SUBAGENT_AGENT, "worker");
  assert.equal(env.ORBIT_SUBAGENT_DEPTH, "1");
  assert.equal(env.ORBIT_SUBAGENT_PARENT, "s1");
  assert.equal(env.PI_SKIP_VERSION_CHECK, "1");
});

test("the decoder splits on LF, not on Unicode separators", () => {
  const decoder = new LineDecoder();
  const text = '{"type":"message_end","message":{"content":"a\u2028b"}}\n{"type":"agent_settled"}\n';
  const lines = decoder.push(Buffer.from(text, "utf8"));
  assert.equal(lines.length, 2);
  assert.ok(lines[0].includes("\u2028"));
  assert.deepEqual(parseRecord(lines[1]), { type: "agent_settled" });
});

test("the decoder handles chunk boundaries and CRLF", () => {
  const decoder = new LineDecoder();
  assert.deepEqual(decoder.push(Buffer.from('{"a":1}\r', "utf8")), []);
  assert.deepEqual(decoder.push(Buffer.from("\n{\"b\":2}\n", "utf8")), ['{"a":1}', '{"b":2}']);
  assert.deepEqual(decoder.end(), []);
});

test("an unparsable record is ignored", () => {
  assert.equal(parseRecord("not json"), null);
  assert.equal(parseRecord(""), null);
  assert.equal(parseRecord("42"), null);
});

test("events fold into phase, current action, counts and usage", () => {
  const state = child();
  applyEvent(state, { type: "message_start", message: { role: "assistant", model: "x/y" } }, 1100);
  assert.equal(state.phase, "thinking");
  assert.equal(state.model, "x/y");

  applyEvent(
    state,
    { type: "tool_execution_start", toolName: "grep", args: { pattern: "auth in src/" } },
    1200,
  );
  assert.equal(state.phase, "tool");
  assert.equal(state.tools, 1);
  assert.equal(state.current, "grep auth in src/");
  assert.deepEqual(state.steps.at(-1), { kind: "tool", name: "grep", preview: "auth in src/" });

  applyEvent(state, { type: "tool_execution_end", toolName: "grep" }, 1300);
  assert.equal(state.phase, "thinking");
  assert.equal(state.current, null);

  applyEvent(
    state,
    {
      type: "message_end",
      message: {
        role: "assistant",
        content: [{ type: "text", text: "  auth is in src/auth.rs  " }],
        usage: { input: 1000, output: 200, cacheRead: 50, cacheWrite: 10, cost: { total: 0.012 } },
        stopReason: "stop",
      },
    },
    1400,
  );
  assert.equal(state.report, "auth is in src/auth.rs");
  assert.equal(state.stopReason, "stop");
  assert.equal(state.sawAssistant, true);
  assert.deepEqual(state.usage, {
    input: 1000,
    output: 200,
    cacheRead: 50,
    cacheWrite: 10,
    cost: 0.012,
    turns: 1,
    contextTokens: 1260,
  });
});

test("usage accumulates across turns and nested cost shapes", () => {
  const total = addUsage(addUsage(emptyUsage(), { input: 10, output: 5, cost: 0.5 }), {
    input: 1,
    output: 2,
    cost: { total: 0.25 },
  });
  assert.equal(total.input, 11);
  assert.equal(total.output, 7);
  assert.equal(total.cost, 0.75);
  assert.equal(total.turns, 2);
});

test("a clean stop with no text is done, not failed", () => {
  const state = child({ sawAssistant: true, stopReason: "stop" });
  finalizeChild(state, { exitCode: 0, now: 2000 });
  assert.equal(state.status, "done");
  assert.equal(state.report, null);
  const content = runContent([state]);
  assert.match(content, /scout — done/);
  assert.match(content, /no closing message for the parent/);
});

test("abort and timeout are labelled from the evidence", () => {
  const aborted = child({ aborted: true });
  finalizeChild(aborted, { exitCode: 1, now: 2000 });
  assert.equal(aborted.status, "aborted");
  assert.equal(aborted.phase, "killed");

  const timedOut = child();
  finalizeChild(timedOut, { exitCode: 0, timedOut: true, timeoutMinutes: 30, now: 2000 });
  assert.equal(timedOut.status, "failed");
  assert.equal(timedOut.error, "timed out after 30 min");
});

test("a child with no assistant message fails with a readable reason", () => {
  const state = child();
  finalizeChild(state, { exitCode: 0, now: 2000 });
  assert.equal(state.status, "failed");
  assert.equal(state.error, "child produced no assistant message");

  const exited = child({ sawAssistant: true, stopReason: "stop" });
  finalizeChild(exited, { exitCode: 2, now: 2000 });
  assert.equal(exited.error, "child exited 2");
});

test("an error stop reason carries its message", () => {
  const state = child();
  applyEvent(
    state,
    { type: "message_end", message: { role: "assistant", content: [], stopReason: "error", errorMessage: "rate limited" } },
    1500,
  );
  finalizeChild(state, { exitCode: 0, now: 2000 });
  assert.equal(state.status, "failed");
  assert.equal(state.error, "rate limited");
});

test("agent_settled aborted is the only abort signal", () => {
  const state = child();
  applyEvent(state, { type: "agent_end", willRetry: true }, 1100);
  assert.equal(state.aborted, false);
  applyEvent(state, { type: "agent_settled", aborted: true }, 1200);
  assert.equal(state.aborted, true);
});

test("details are bounded", () => {
  const state = child();
  for (let i = 0; i < 12; i += 1) {
    applyEvent(state, { type: "tool_execution_start", toolName: `t${i}`, args: { path: "x".repeat(400) } }, 1000 + i);
  }
  state.report = "r".repeat(BOUNDS.report + 100);
  state.stderrTail = "e".repeat(BOUNDS.stderr + 100);
  state.task = "t".repeat(BOUNDS.task + 100);

  const details = childDetails(state);
  assert.equal(details.steps.length, BOUNDS.steps);
  assert.equal(details.stepsTotal, 12);
  assert.ok(details.steps.every((step) => step.preview.length <= BOUNDS.preview));
  assert.ok(details.report.length <= BOUNDS.report);
  assert.ok(details.report.length > BOUNDS.report * 0.99);
  assert.ok(Buffer.byteLength(details.stderrTail) <= BOUNDS.stderr);
  assert.ok(details.task.length <= BOUNDS.task);
  assert.ok(Object.keys(details.toolCounts).length <= BOUNDS.toolCounts);
});

test("the parent's copy truncates the report with an explicit marker", () => {
  const state = child({ report: "x".repeat(BOUNDS.contentReport + 4096) });
  finalizeChild(state, { exitCode: 0, now: 2000 });
  const content = runContent([state]);
  assert.match(content, /truncated \(\d+ KiB\) — full report in the card/);
  assert.ok(content.length < state.report.length);
});

test("the run envelope names its version and mode", () => {
  const details = runDetails("parallel", [child()]);
  assert.equal(details.orbit, "subagent/1");
  assert.equal(details.mode, "parallel");
  assert.equal(details.children.length, 1);
  assert.equal(details.children[0].agent, "scout");
});

test("usage is formatted the way the card reads it", () => {
  assert.equal(formatUsage({ ...emptyUsage(), input: 3100, output: 812, cost: 0.0123 }), "↑3.1k ↓812 · $0.01");
  assert.equal(formatUsage({ ...emptyUsage(), input: 900, output: 12, cost: 0.004 }), "↑900 ↓12 · <$0.01");
  assert.equal(formatUsage(emptyUsage()), "");
});

test("the run's usage sums every child, spent or failed", () => {
  const first = child({ usage: { ...emptyUsage(), input: 10, output: 2, cost: 0.1 } });
  const second = child({ usage: { ...emptyUsage(), input: 5, output: 1, cost: 0.05 } });
  const total = totalUsage([first, second]);
  assert.equal(total.input, 15);
  assert.equal(total.output, 3);
  assert.equal(total.cost, 0.15000000000000002);
});

test("capBytes reports what it dropped", () => {
  const { text, truncated } = capBytes("hello world", 5);
  assert.ok(Buffer.byteLength(text) <= 5);
  assert.ok(truncated > 0);
});
