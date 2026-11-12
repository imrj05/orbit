import test from "node:test";
import assert from "node:assert/strict";
import activate, { contextText, parseSuggestions } from "./index.js";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

test("context contains recent messages and tool arguments, bounded in size", () => {
  const entries = [
    { type: "message", message: { role: "user", content: "Fix the tests" } },
    { type: "message", message: { role: "assistant", content: [{ type: "toolCall", name: "bash", arguments: { command: "cargo test" } }] } },
    { type: "message", message: { role: "toolResult", content: [{ type: "text", text: "test failed" }] } },
  ];
  const result = contextText(entries);
  assert.match(result, /cargo test/);
  assert.match(result, /test failed/);
  assert.ok(contextText(Array(30).fill({ type: "message", message: { role: "user", content: "x".repeat(2000) } })).length <= 8000);
});

test("accepts at most three unique short suggestions", () => {
  const response = { content: [{ type: "text", text: JSON.stringify(["Run tests", "Run tests", "Fix lint", "Continue", "Fourth", "x".repeat(181)]) }] };
  assert.deepEqual(parseSuggestions(response), ["Run tests", "Fix lint", "Continue"]);
});

test("ignores malformed and non-array model output", () => {
  assert.deepEqual(parseSuggestions({ content: [{ type: "text", text: "not json" }] }), []);
  assert.deepEqual(parseSuggestions({ content: [{ type: "text", text: "{}" }] }), []);
});

test("a new turn prevents a delayed result from appearing", async () => {
  const oldHome = process.env.HOME;
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-suggestions-"));
  process.env.HOME = home;
  fs.mkdirSync(path.join(home, ".orbit-pi"));
  fs.writeFileSync(path.join(home, ".orbit-pi", "suggestions.json"), '{"enabled":true}');
  try {
    const handlers = {};
    activate({ on: (name, fn) => { handlers[name] = fn; } });
    const widgets = [];
    let finish;
    const ctx = {
      ui: { setWidget: (...args) => widgets.push(args) },
      model: { provider: "test", id: "test" },
      modelRegistry: { complete: () => new Promise(resolve => { finish = resolve; }) },
      sessionManager: { getSessionId: () => "s", getLeafId: () => "leaf", getBranch: () => [{ type: "message", message: { role: "assistant", content: "Test failed" } }] },
    };
    handlers.agent_settled({}, ctx);
    await new Promise(resolve => setTimeout(resolve, 400));
    assert.equal(typeof finish, "function");
    handlers.before_agent_start({}, ctx);
    finish({ content: [{ type: "text", text: '["Run tests"]' }] });
    await new Promise(resolve => setTimeout(resolve, 0));
    assert.deepEqual(widgets, [["orbit-follow-up-suggestions", undefined]]);

    handlers.agent_settled({}, ctx);
    await new Promise(resolve => setTimeout(resolve, 400));
    finish({ content: [{ type: "text", text: '["Fix the failing test"]' }] });
    await new Promise(resolve => setTimeout(resolve, 0));
    assert.deepEqual(JSON.parse(widgets.at(-1)[1][0]), {
      mode: "full-access", workflow: "build", suggestions: ["Fix the failing test"],
    });
  } finally {
    process.env.HOME = oldHome;
    fs.rmSync(home, { recursive: true, force: true });
  }
});
