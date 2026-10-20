import assert from "node:assert/strict";
import test from "node:test";

import {
  countToolCalls,
  formatTokens,
  formatUsage,
  getFinalOutput,
  getResultOutput,
  isFailedResult,
  mapWithConcurrencyLimit,
  selectChildTools,
  summarizeResult,
  truncateOutput,
} from "./lib.js";

function assistant(text, extra = {}) {
  return {
    role: "assistant",
    content: [{ type: "text", text }],
    usage: {
      input: 100,
      output: 20,
      cacheRead: 0,
      cacheWrite: 0,
      cost: { total: 0.0012 },
      totalTokens: 120,
    },
    stopReason: "end",
    ...extra,
  };
}

function result(overrides = {}) {
  return {
    agent: "scout",
    agentSource: "builtin",
    task: "find the thing",
    exitCode: 0,
    messages: [assistant("the answer")],
    stderr: "",
    usage: {
      input: 100,
      output: 20,
      cacheRead: 0,
      cacheWrite: 0,
      cost: 0.0012,
      contextTokens: 120,
      turns: 1,
    },
    ...overrides,
  };
}

test("formatTokens keeps small counts and compacts larger ones", () => {
  assert.equal(formatTokens(999), "999");
  assert.equal(formatTokens(1200), "1.2k");
  assert.equal(formatTokens(12_000), "12k");
  assert.equal(formatTokens(2_400_000), "2.4M");
});

test("formatUsage joins the facts it has", () => {
  assert.equal(
    formatUsage({
      turns: 2,
      input: 1200,
      output: 300,
      cacheRead: 0,
      cacheWrite: 0,
      cost: 0.0012,
    }, "ollama-cloud/glm-5.3"),
    "2 turns · ↑1.2k · ↓300 · $0.0012 · ollama-cloud/glm-5.3",
  );
  assert.equal(formatUsage(null), "");
});

test("getFinalOutput returns the newest assistant text", () => {
  assert.equal(getFinalOutput([assistant("first"), assistant("last")]), "last");
  assert.equal(getFinalOutput([assistant("   ")]), "");
  assert.equal(getFinalOutput([]), "");
});

test("string message content is accepted", () => {
  assert.equal(
    getFinalOutput([{ role: "assistant", content: "plain string" }]),
    "plain string",
  );
});

test("countToolCalls counts toolCall blocks", () => {
  const messages = [
    {
      role: "assistant",
      content: [
        { type: "toolCall", name: "read" },
        { type: "toolCall", name: "grep" },
        { type: "text", text: "done" },
      ],
    },
    { role: "assistant", content: [{ type: "tool_call", name: "bash" }] },
  ];
  assert.equal(countToolCalls(messages), 3);
});

test("failure detection covers exit code, provider error, and abort", () => {
  assert.equal(isFailedResult(result()), false);
  assert.equal(isFailedResult(result({ exitCode: 1 })), true);
  assert.equal(isFailedResult(result({ stopReason: "error" })), true);
  assert.equal(isFailedResult(result({ stopReason: "aborted" })), true);
});

test("getResultOutput prefers the error detail for a failed child", () => {
  assert.equal(getResultOutput(result()), "the answer");
  assert.equal(
    getResultOutput(result({ exitCode: 1, errorMessage: "provider failed" })),
    "provider failed",
  );
  assert.equal(
    getResultOutput(result({ exitCode: 1, stderr: "spawn failed" })),
    "spawn failed",
  );
  assert.equal(
    getResultOutput(result({ exitCode: 1, messages: [] })),
    "(no output)",
  );
});

test("truncateOutput respects the byte cap without splitting UTF-8", () => {
  assert.equal(truncateOutput("short", 10), "short");
  const output = "é".repeat(20);
  const truncated = truncateOutput(output, 21);
  assert.ok(Buffer.byteLength(truncated, "utf8") <= 21 + 200);
  assert.ok(truncated.startsWith("é"));
  assert.match(truncated, /Output truncated/);
});

test("summarizeResult drops the child transcript and reports status", () => {
  const summary = summarizeResult(result());
  assert.equal(summary.agent, "scout");
  assert.equal(summary.output, "the answer");
  assert.equal(summary.status, "completed");
  assert.equal(summary.messages, undefined);
  assert.equal(summarizeResult(result({ exitCode: -1 })).status, "running");
  assert.equal(summarizeResult(result({ exitCode: 1 })).status, "failed");
});

test("selectChildTools keeps Build intact and narrows Plan/Ask", () => {
  assert.deepEqual(selectChildTools(["read", "bash", "edit"], "build"), [
    "read",
    "bash",
    "edit",
  ]);
  assert.equal(selectChildTools(undefined, "build"), undefined);
  assert.deepEqual(selectChildTools(undefined, "plan"), [
    "read",
    "grep",
    "find",
    "ls",
  ]);
  assert.deepEqual(selectChildTools(["read", "bash", "edit"], "ask"), ["read"]);
  assert.deepEqual(selectChildTools(["bash", "edit"], "plan"), []);
});

test("mapWithConcurrencyLimit preserves order and bounds concurrency", async () => {
  let active = 0;
  let peak = 0;
  const items = [0, 1, 2, 3, 4, 5];
  const results = await mapWithConcurrencyLimit(items, 2, async (item) => {
    active += 1;
    peak = Math.max(peak, active);
    await new Promise((resolve) => setTimeout(resolve, 5));
    active -= 1;
    return item * 2;
  });
  assert.deepEqual(results, [0, 2, 4, 6, 8, 10]);
  assert.equal(peak, 2);
});
