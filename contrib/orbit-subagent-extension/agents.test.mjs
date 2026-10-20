import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  discoverAgents,
  findNearestProjectAgentsDir,
  formatAgentList,
  parseAgentFile,
  parseThinking,
  parseToolList,
} from "./agents.js";

function scratchDir() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "orbit-subagent-agents-"));
}

function writeAgent(dir, filename, { name, description, tools, model, body }) {
  fs.mkdirSync(dir, { recursive: true });
  const frontmatter = [
    "---",
    `name: ${name}`,
    `description: ${description}`,
    ...(tools ? [`tools: ${tools}`] : []),
    ...(model ? [`model: ${model}`] : []),
    "---",
    "",
    body ?? "Do the thing.",
    "",
  ].join("\n");
  fs.writeFileSync(path.join(dir, filename), frontmatter);
}

test("parseToolList accepts comma and bracket spellings", () => {
  assert.deepEqual(parseToolList("read, grep, find"), ["read", "grep", "find"]);
  assert.deepEqual(parseToolList("[read, grep]"), ["read", "grep"]);
  assert.deepEqual(parseToolList("read"), ["read"]);
  assert.equal(parseToolList(""), undefined);
  assert.equal(parseToolList(42), undefined);
});

test("parseAgentFile keeps the supported frontmatter and body", () => {
  const agent = parseAgentFile(
    [
      "---",
      "name: repo-reviewer",
      "description: Reviews Rust changes",
      "tools: read, grep",
      "model: ollama-cloud/glm-5.3",
      "---",
      "",
      "You are a strict reviewer.",
    ].join("\n"),
    "project",
    "/tmp/repo-reviewer.md",
  );
  assert.equal(agent.name, "repo-reviewer");
  assert.equal(agent.description, "Reviews Rust changes");
  assert.deepEqual(agent.tools, ["read", "grep"]);
  assert.equal(agent.model, "ollama-cloud/glm-5.3");
  assert.equal(agent.systemPrompt, "You are a strict reviewer.");
  assert.equal(agent.source, "project");
});

test("parseAgentFile accepts a YAML block tool list and a thinking level", () => {
  const agent = parseAgentFile(
    [
      "---",
      "name: reviewer",
      "description: Reviews changes",
      "tools:",
      "  - read",
      "  - grep",
      "thinking: high",
      "---",
      "",
      "Review it.",
    ].join("\n"),
    "project",
    "/tmp/reviewer.md",
  );
  assert.deepEqual(agent.tools, ["read", "grep"]);
  assert.equal(agent.thinking, "high");
  assert.equal(parseThinking("bogus"), undefined);
  assert.equal(parseThinking("HIGH"), "high");
});

test("parseAgentFile ignores a file without a name or description", () => {
  assert.equal(parseAgentFile("---\nname: x\n---\nbody", "user", "/tmp/x.md"), null);
  assert.equal(parseAgentFile("no frontmatter", "user", "/tmp/x.md"), null);
});

test("discovery prioritizes project over user over built-in", () => {
  const root = scratchDir();
  const builtinDir = path.join(root, "builtin");
  const userDir = path.join(root, "user");
  const projectDir = path.join(root, "project");

  writeAgent(builtinDir, "scout.md", {
    name: "scout",
    description: "builtin scout",
    tools: "read",
  });
  writeAgent(builtinDir, "worker.md", {
    name: "worker",
    description: "builtin worker",
  });
  writeAgent(userDir, "scout.md", {
    name: "scout",
    description: "user scout",
    tools: "read, grep",
  });
  writeAgent(userDir, "custom.md", {
    name: "custom",
    description: "user custom",
  });
  writeAgent(projectDir, "scout.md", {
    name: "scout",
    description: "project scout",
    tools: "read, grep, find",
  });

  const { agents } = discoverAgents(root, "both", {
    builtinDir,
    userDir,
    projectDir,
  });
  const byName = new Map(agents.map((agent) => [agent.name, agent]));
  assert.equal(byName.get("scout").description, "project scout");
  assert.deepEqual(byName.get("scout").tools, ["read", "grep", "find"]);
  assert.equal(byName.get("worker").source, "builtin");
  assert.equal(byName.get("custom").source, "user");
  assert.deepEqual(
    agents.map((agent) => agent.name),
    ["custom", "scout", "worker"],
  );

  fs.rmSync(root, { recursive: true, force: true });
});

test("scope user skips project agents; scope project skips user agents", () => {
  const root = scratchDir();
  const builtinDir = path.join(root, "builtin");
  const userDir = path.join(root, "user");
  const projectDir = path.join(root, "project");
  writeAgent(builtinDir, "scout.md", { name: "scout", description: "b" });
  writeAgent(userDir, "user.md", { name: "user", description: "u" });
  writeAgent(projectDir, "project.md", { name: "project", description: "p" });

  const userOnly = discoverAgents(root, "user", {
    builtinDir,
    userDir,
    projectDir,
  }).agents.map((agent) => agent.name);
  const projectOnly = discoverAgents(root, "project", {
    builtinDir,
    userDir,
    projectDir,
  }).agents.map((agent) => agent.name);

  assert.deepEqual(userOnly.sort(), ["scout", "user"]);
  assert.deepEqual(projectOnly.sort(), ["project", "scout"]);

  fs.rmSync(root, { recursive: true, force: true });
});

test("nested agent directories are discovered", () => {
  const root = scratchDir();
  const builtinDir = path.join(root, "builtin");
  writeAgent(path.join(builtinDir, "nested"), "deep.md", {
    name: "deep",
    description: "nested agent",
  });
  const { agents } = discoverAgents(root, "user", {
    builtinDir,
    userDir: path.join(root, "missing-user"),
    projectDir: null,
  });
  assert.equal(agents.length, 1);
  assert.equal(agents[0].name, "deep");
  fs.rmSync(root, { recursive: true, force: true });
});

test("findNearestProjectAgentsDir walks up to the nearest .pi/agents", () => {
  const root = scratchDir();
  const nested = path.join(root, "packages", "app", "src");
  fs.mkdirSync(path.join(root, ".pi", "agents"), { recursive: true });
  fs.mkdirSync(nested, { recursive: true });
  assert.equal(
    findNearestProjectAgentsDir(nested),
    path.join(root, ".pi", "agents"),
  );
  fs.rmSync(root, { recursive: true, force: true });
});

test("formatAgentList truncates and reports the remainder", () => {
  const agents = [
    { name: "a", source: "builtin", description: "A" },
    { name: "b", source: "user", description: "B" },
    { name: "c", source: "project", description: "C" },
  ];
  const listed = formatAgentList(agents, 2);
  assert.equal(listed.text, "a (builtin): A; b (user): B");
  assert.equal(listed.remaining, 1);
});
