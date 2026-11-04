#!/usr/bin/env node
/**
 * Tests for agent discovery and the capability ceiling.
 *
 * Run: node --test contrib/orbit-subagents-extension/agents.test.mjs
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  buildDescription,
  buildPromptGuidelines,
  ceilingForTools,
  classifyTool,
  discoverAgents,
  isKebabName,
  loadBuiltins,
  parseAgentFile,
  parseTools,
} from "./agents.js";

const SCOUT = `---
name: scout
description: Fast recon
model: anthropic/claude-haiku-4-5
tools: read, grep, find, ls
---

You are a scout.
`;

test("parses the five keys and the body", () => {
  const parsed = parseAgentFile(SCOUT);
  assert.equal(parsed.error, undefined);
  assert.equal(parsed.fields.name, "scout");
  assert.equal(parsed.fields.model, "anthropic/claude-haiku-4-5");
  assert.equal(parsed.fields.tools, "read, grep, find, ls");
  assert.equal(parsed.body, "You are a scout.");
});

test("reads bracketed and block lists", () => {
  assert.deepEqual(parseTools("[read, edit]"), { tools: ["read", "edit"], present: true });
  assert.deepEqual(parseTools("read,bash"), { tools: ["read", "bash"], present: true });
  assert.deepEqual(parseTools(undefined), { tools: null, present: false });
  assert.deepEqual(parseTools(""), { tools: [], present: true });
  assert.deepEqual(parseTools(["read", "write"]), { tools: ["read", "write"], present: true });
});

test("a block list survives the frontmatter reader", () => {
  const parsed = parseAgentFile(`---
name: worker
description: Implements
tools:
  - read
  - edit
  - bash
thinking: high
---

Body.
`);
  assert.deepEqual(parseTools(parsed.fields.tools), {
    tools: ["read", "edit", "bash"],
    present: true,
  });
  assert.equal(parsed.fields.thinking, "high");
});

test("a file with no frontmatter or no closing delimiter is reported, not thrown", () => {
  assert.equal(parseAgentFile("just a prompt").error, "no frontmatter");
  assert.equal(parseAgentFile("---\nname: x\n").error, "unterminated frontmatter");
});

test("comments and unknown keys are ignored", () => {
  const parsed = parseAgentFile(`---
# keep this comment
name: scout
memory: project
description: Recon
---
Body.
`);
  assert.equal(parsed.fields.name, "scout");
  assert.equal(parsed.fields.memory, "project");
  assert.equal(parsed.body, "Body.");
});

test("kebab-case names only", () => {
  assert.ok(isKebabName("scout"));
  assert.ok(isKebabName("code-reviewer"));
  assert.ok(!isKebabName("Scout"));
  assert.ok(!isKebabName("code_reviewer"));
  assert.ok(!isKebabName(""));
});

test("a tool's risk is classified with the guard's vocabulary", () => {
  assert.equal(classifyTool("read"), "read");
  assert.equal(classifyTool("Grep"), "read");
  assert.equal(classifyTool("write"), "edit");
  assert.equal(classifyTool("powershell"), "exec");
  assert.equal(classifyTool("mcp__github__search"), "all");
  assert.equal(classifyTool("my_extension_tool"), "all");
});

test("the ceiling is derived from the tools line", () => {
  assert.equal(ceilingForTools("read, grep, find, ls"), "read");
  assert.equal(ceilingForTools("read, edit, write"), "edit");
  assert.equal(ceilingForTools("read, bash"), "exec");
  assert.equal(ceilingForTools("read, mcp__x__y"), "all");
  // Absent means pi's own defaults, which include bash.
  assert.equal(ceilingForTools(undefined), "exec");
  // Explicitly empty means the child has no tools at all.
  assert.equal(ceilingForTools(""), "read");
});

test("user files override builtins and malformed files are skipped", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-subagents-agents-"));
  const userDir = path.join(home, ".pi", "agent", "agents");
  fs.mkdirSync(userDir, { recursive: true });
  fs.writeFileSync(path.join(userDir, "scout.md"), SCOUT);
  fs.writeFileSync(path.join(userDir, "broken.md"), "no frontmatter here");
  fs.writeFileSync(path.join(userDir, "unnamed.md"), "---\ndescription: x\n---\nBody.\n");
  fs.writeFileSync(
    path.join(userDir, "extra.md"),
    "---\nname: extra\ndescription: Extra agent\ntools: read\n---\nBody.\n",
  );

  const builtins = [
    { name: "scout", description: "shipped", ceiling: "read", source: "builtin", tools: ["read"] },
    { name: "worker", description: "shipped", ceiling: "exec", source: "builtin", tools: ["bash"] },
  ];
  const { agents, skipped } = discoverAgents({ builtins, userDir });
  assert.deepEqual(agents.map((agent) => agent.name), ["extra", "scout", "worker"]);
  assert.equal(agents.find((agent) => agent.name === "scout").source, "user");
  assert.equal(skipped.length, 2);
});

test("nested agent directories are discovered", () => {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-subagents-nested-"));
  const userDir = path.join(home, ".pi", "agent", "agents");
  fs.mkdirSync(path.join(userDir, "team"), { recursive: true });
  fs.writeFileSync(
    path.join(userDir, "team", "reviewer.md"),
    "---\nname: reviewer\ndescription: Review\ntools: read\n---\nBody.\n",
  );
  const { agents } = discoverAgents({ builtins: [], userDir });
  assert.deepEqual(agents.map((agent) => agent.name), ["reviewer"]);
});

test("the shipped builtins parse and carry real pi tool names", () => {
  const builtins = loadBuiltins();
  assert.deepEqual(builtins.map((agent) => agent.name), ["scout", "worker", "reviewer"]);
  const known = new Set(["read", "grep", "find", "ls", "edit", "write", "bash", "powershell"]);
  for (const agent of builtins) {
    assert.ok(agent.description.length > 0, `${agent.name} has a description`);
    for (const tool of agent.tools ?? []) {
      assert.ok(known.has(tool), `${agent.name} names a pi tool: ${tool}`);
    }
  }
  assert.equal(builtins.find((agent) => agent.name === "scout").ceiling, "read");
  assert.equal(builtins.find((agent) => agent.name === "worker").ceiling, "exec");
});

test("the description is built from the enabled agents", () => {
  const text = buildDescription([
    { name: "scout", description: "Recon", ceiling: "read" },
    { name: "worker", description: "Implementation", ceiling: "exec" },
  ]);
  // The trigger comes first: the decision to delegate is made before reading.
  assert.match(text, /^Call this first for broad work:/);
  assert.match(text, /Agents \(built in, or files in ~\/\.pi\/agent\/agents\):/);
  assert.match(text, /scout\s+read only · Recon/);
  assert.match(text, /worker\s+runs commands · Implementation/);
  assert.match(text, /a short report, not a transcript/);
  // The cost caveat closes the block, after the trigger has landed.
  assert.match(text, /Skip it for one file you already know/);
  assert.ok(text.indexOf("Call this first") < text.indexOf("Agents"));
  assert.ok(text.indexOf("Agents") < text.indexOf("Skip it"));
});

test("prompt guidelines name the tool and only the enabled agents", () => {
  const lines = buildPromptGuidelines([{ name: "scout" }, { name: "reviewer" }]);
  assert.ok(lines.every((line) => line.includes("subagent")));
  assert.ok(lines.some((line) => line.includes('"reviewer"')));
  assert.ok(lines.some((line) => line.includes('"scout"')));

  const withoutScout = buildPromptGuidelines([{ name: "worker" }]);
  assert.ok(withoutScout.every((line) => !line.includes('"scout"')));
  assert.ok(withoutScout.every((line) => !line.includes('"reviewer"')));
});
