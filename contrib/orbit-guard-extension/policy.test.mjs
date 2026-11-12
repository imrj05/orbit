#!/usr/bin/env node
/**
 * Offline tests for the Orbit guard policy.
 *
 * Run: node --test contrib/orbit-guard-extension/
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  DEFAULT_MODE,
  MODES,
  addToAllowlist,
  allowlistPath,
  ceilingAllows,
  ceilingBlockReason,
  classify,
  confirmOptions,
  commandSegments,
  decide,
  defaultAction,
  formatTitle,
  grantable,
  modeFilePath,
  normalizeMode,
  OPTION_ALWAYS_ALLOW,
  OPTION_ALLOW_ONCE,
  OPTION_DENY,
  isCompoundCommand,
  matchText,
  patternMatches,
  readAllowlist,
  readMode,
  readRules,
  ruleDecision,
  rulesFilePath,
  scopesFor,
  summarize,
} from "./policy.js";

test("classify buckets built-in tools by effect", () => {
  assert.equal(classify("read"), "read");
  assert.equal(classify("grep"), "read");
  assert.equal(classify("ls"), "read");
  assert.equal(classify("edit"), "edit");
  assert.equal(classify("write"), "edit");
  assert.equal(classify("bash"), "exec");
  assert.equal(classify("powershell"), "exec");
  assert.equal(classify("some_extension_tool"), "other");
  assert.equal(classify(undefined), "other");
  // MCP tools keep their own kind so they can never be mistaken for a
  // read-only built-in.
  assert.equal(classify("mcp__github__search_repositories"), "mcp");
  assert.equal(classify("MCP__GITHUB__GET_ISSUE"), "mcp");
});

test("normalizeMode falls back for unknown or missing values", () => {
  for (const mode of MODES) assert.equal(normalizeMode(mode), mode);
  assert.equal(normalizeMode("bypass"), DEFAULT_MODE);
  assert.equal(normalizeMode(undefined), DEFAULT_MODE);
  assert.equal(normalizeMode(42), DEFAULT_MODE);
});

test("supervised allows reads and asks for mutations", () => {
  assert.equal(decide("supervised", "read"), "allow");
  assert.equal(decide("supervised", "edit"), "ask");
  assert.equal(decide("supervised", "write"), "ask");
  assert.equal(decide("supervised", "bash"), "ask");
  assert.equal(decide("supervised", "mcp__x"), "ask");
  assert.equal(decide("supervised", "mcp__github__create_issue"), "ask");
});

test("auto-accept-edits allows edits and asks for exec/other", () => {
  assert.equal(decide("auto-accept-edits", "read"), "allow");
  assert.equal(decide("auto-accept-edits", "edit"), "allow");
  assert.equal(decide("auto-accept-edits", "write"), "allow");
  assert.equal(decide("auto-accept-edits", "bash"), "ask");
  assert.equal(decide("auto-accept-edits", "mcp__x"), "ask");
});

test("full-access allows everything", () => {
  for (const tool of ["read", "edit", "bash", "mcp__x"]) {
    assert.equal(decide("full-access", tool), "allow");
  }
});

test("the default mode asks before mutating, so every read failure is fail-safe", () => {
  assert.equal(DEFAULT_MODE, "supervised");
  assert.equal(decide(DEFAULT_MODE, "bash"), "ask");
  assert.equal(decide(DEFAULT_MODE, "edit"), "ask");
});

test("a grant is never honoured for a tool whose meaning changes per call", () => {
  // The finding this closes: approving one `ls` used to allow every future
  // command, because the grant was keyed by the tool name alone.
  assert.equal(decide("supervised", "bash", ["bash"]), "ask");
  assert.equal(decide("supervised", "mcp__x", ["mcp__x"]), "ask");
  assert.equal(decide("supervised", "some_extension_tool", ["some_extension_tool"]), "ask");
  // Whole-file edits are the same risk whatever the path, so a grant stands.
  assert.equal(decide("supervised", "edit", ["edit"]), "allow");
  assert.equal(decide("supervised", "bash", ["edit"]), "ask");
});

test("grantable and confirmOptions agree with decide", () => {
  for (const [tool, allowed] of [
    ["edit", true],
    ["bash", false],
    ["mcp__x", false],
    ["some_extension_tool", false],
  ]) {
    assert.equal(grantable(tool), allowed, tool);
    assert.equal(
      confirmOptions(tool).includes(OPTION_ALWAYS_ALLOW),
      allowed,
      `${tool} options`,
    );
    assert.ok(confirmOptions(tool).includes(OPTION_ALLOW_ONCE), tool);
    assert.ok(confirmOptions(tool).includes(OPTION_DENY), tool);
  }
});

test("an unknown mode is treated as the default, never as allow-all of a stricter mode", () => {
  assert.equal(decide("nonsense", "bash"), decide(DEFAULT_MODE, "bash"));
});

test("formatTitle encodes the tool and detail Orbit parses", () => {
  const title = formatTitle("bash", "rm -rf build");
  assert.ok(title.startsWith("[orbit-guard] "));
  const [tool, detail] = title.slice("[orbit-guard] ".length).split("\t");
  assert.equal(tool, "bash");
  assert.equal(detail, "rm -rf build");
});

test("summarize prefers the salient argument and stays single-line", () => {
  assert.equal(summarize("bash", { command: "rm -rf build" }), "rm -rf build");
  assert.equal(summarize("edit", { path: "src/a.rs" }), "src/a.rs");
  const long = summarize("bash", { command: "x".repeat(500) });
  assert.ok(long.length <= 202, "truncated");
  assert.ok(!long.includes("\n"));
});

test("readMode reads the mode file and degrades on any failure", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-guard-"));
  const file = path.join(dir, "access.json");
  try {
    assert.equal(readMode(file), DEFAULT_MODE, "missing file → default");
    fs.writeFileSync(file, JSON.stringify({ mode: "supervised" }));
    assert.equal(readMode(file), "supervised");
    fs.writeFileSync(file, JSON.stringify({ mode: "bogus" }));
    assert.equal(readMode(file), DEFAULT_MODE, "unknown value → default");
    fs.writeFileSync(file, "{not json");
    assert.equal(readMode(file), DEFAULT_MODE, "broken json → default");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("allowlist round-trips per mode and degrades on failure", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-guard-"));
  const file = path.join(dir, "access-allow.json");
  try {
    assert.deepEqual(readAllowlist("supervised", file), [], "missing → empty");
    assert.equal(addToAllowlist("supervised", "bash", file), false, "exec is not grantable");
    assert.equal(addToAllowlist("supervised", "edit", file), true);
    addToAllowlist("supervised", "edit", file); // idempotent
    addToAllowlist("supervised", "write", file);
    assert.deepEqual(readAllowlist("supervised", file).sort(), ["edit", "write"]);
    assert.deepEqual(readAllowlist("auto-accept-edits", file), [], "mode-scoped");
    if (process.platform !== "win32") {
      assert.equal(fs.statSync(file).mode & 0o777, 0o600, "grants are owner-only");
    }
    fs.writeFileSync(file, "{not json");
    assert.deepEqual(readAllowlist("supervised", file), [], "broken json → empty");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("path helpers point under ~/.orbit-pi", () => {
  const prev = process.env.HOME;
  process.env.HOME = "/tmp/orbit-home-test";
  try {
    assert.equal(modeFilePath(), "/tmp/orbit-home-test/.orbit-pi/access.json");
    assert.equal(allowlistPath(), "/tmp/orbit-home-test/.orbit-pi/access-allow.json");
  } finally {
    process.env.HOME = prev;
  }
});

test("the delegation tool is its own kind and is always allowed", () => {
  assert.equal(classify("subagent"), "delegate");
  assert.equal(classify("SUBAGENT"), "delegate");
  for (const mode of MODES) {
    assert.equal(decide(mode, "subagent"), "allow", mode);
  }
  // It is never grantable: there is no prompt to offer it from.
  assert.equal(grantable("subagent"), false);
});

test("a child's ceiling clamps what it may run", () => {
  // read blocks edit, exec and anything unrecognised
  assert.ok(ceilingAllows("read", "read"));
  assert.ok(ceilingAllows("read", "grep"));
  assert.ok(!ceilingAllows("read", "edit"));
  assert.ok(!ceilingAllows("read", "bash"));
  assert.ok(!ceilingAllows("read", "mcp__github__create_issue"));
  assert.ok(!ceilingAllows("read", "some_extension_tool"));
  assert.ok(!ceilingAllows("read", "subagent"));

  // edit blocks exec and above
  assert.ok(ceilingAllows("edit", "read"));
  assert.ok(ceilingAllows("edit", "write"));
  assert.ok(!ceilingAllows("edit", "bash"));
  assert.ok(!ceilingAllows("edit", "mcp__x"));

  // exec blocks MCP / custom tools, which need `all`
  assert.ok(ceilingAllows("exec", "bash"));
  assert.ok(ceilingAllows("exec", "powershell"));
  assert.ok(!ceilingAllows("exec", "mcp__x"));
  assert.ok(!ceilingAllows("exec", "some_extension_tool"));

  // all is unrestricted
  assert.ok(ceilingAllows("all", "mcp__x"));
  assert.ok(ceilingAllows("all", "some_extension_tool"));
  assert.ok(ceilingAllows("all", "subagent"));
});

test("no ceiling behaves exactly as before", () => {
  assert.ok(ceilingAllows(undefined, "bash"));
  assert.ok(ceilingAllows(null, "mcp__x"));
  assert.ok(ceilingAllows("bogus", "bash"));
});

test("a ceiling block names the agent and the tool", () => {
  assert.match(ceilingBlockReason("read", "bash", "scout"), /Subagent scout is read-only and cannot use bash/);
  assert.match(ceilingBlockReason("edit", "bash", "worker"), /may edit files but cannot use bash/);
  assert.match(ceilingBlockReason("exec", "mcp__x", "worker"), /may run commands but may not use mcp__x/);
});

// ── permission rules ────────────────────────────────────────────────────

test("network tools are their own kind but still allowed by the mode defaults", () => {
  assert.equal(classify("webfetch"), "net");
  assert.equal(classify("websearch"), "net");
  assert.equal(classify("WebFetch"), "net");
  for (const mode of MODES) {
    assert.equal(decide(mode, "webfetch"), "allow", mode);
  }
});

test("defaultAction is the table the mode defaults implement", () => {
  // One representative tool per kind; the Rust table the Settings page renders
  // (`permissions::Target::default_action`) mirrors this and has its own test.
  const sample = {
    read: "read",
    net: "webfetch",
    edit: "edit",
    exec: "bash",
    mcp: "mcp__github__get_issue",
    other: "some_extension_tool",
    delegate: "subagent",
  };
  for (const mode of MODES) {
    for (const [kind, tool] of Object.entries(sample)) {
      assert.equal(classify(tool), kind, tool);
      assert.equal(decide(mode, tool), defaultAction(mode, kind), `${mode}/${kind}`);
    }
  }
});

test("patternMatches is an anchored glob with one metacharacter", () => {
  assert.ok(patternMatches("", "anything"), "empty pattern matches every call");
  assert.ok(patternMatches("ls", "ls"));
  assert.ok(!patternMatches("ls", "ls -la"), "no implicit prefix match");
  assert.ok(patternMatches("ls *", "ls -la"));
  assert.ok(patternMatches("git status*", "git status --short"));
  assert.ok(patternMatches("*.log", "build/output.log"), "* spans directories");
  assert.ok(patternMatches("*rm -rf*", "cd x && rm -rf y"), "found anywhere");
  assert.ok(!patternMatches("rm *", "/bin/rm -rf x"), "a different binary is a different text");
  assert.ok(patternMatches("a.b(c)", "a.b(c)"), "regex metacharacters are literal");
  assert.ok(!patternMatches("a.b", "axb"), "dots do not act as wildcards");
  assert.ok(patternMatches("*", "anything at all"));
});

test("matchText names the text the approval bar shows", () => {
  assert.equal(matchText("bash", { command: "git status" }), "git status");
  assert.equal(matchText("edit", { path: "src/a.rs" }), "src/a.rs");
  assert.equal(matchText("grep", { pattern: "TODO" }), "TODO");
  assert.equal(matchText("webfetch", { url: "https://example.com" }), "https://example.com");
  assert.equal(matchText("subagent", { task: "review the diff" }), "review the diff");
  assert.ok(matchText("mcp__github__get_issue", { number: 12 }).startsWith("mcp__github__get_issue "));
  assert.ok(matchText("some_extension_tool", {}).startsWith("some_extension_tool "));
});

test("a compound command is split so a deny rule still sees every segment", () => {
  assert.deepEqual(commandSegments("cd x && rm -rf y"), ["cd x", "rm -rf y"]);
  assert.deepEqual(commandSegments("a; b | c || d"), ["a", "b", "c", "d"]);
  assert.ok(isCompoundCommand("ls && rm -rf x"));
  assert.ok(isCompoundCommand("ls > out.txt"));
  assert.ok(isCompoundCommand("echo $(rm -rf x)"));
  assert.ok(isCompoundCommand("rm -rf x &"));
  assert.ok(!isCompoundCommand("git status --short"));
});

test("a rule decides the call, and the most specific scope wins", () => {
  const rules = [
    { scope: "any", target: "exec", pattern: "git *", action: "allow" },
    { scope: "session", target: "exec", pattern: "git push*", action: "ask" },
    { scope: "agent:worker", target: "exec", pattern: "git push*", action: "deny" },
  ];
  // Main session: the `session` rule narrows the `any` allow.
  assert.equal(
    decide("supervised", "bash", [], { rules, input: { command: "git push origin main" } }),
    "ask",
  );
  // The same call in the worker: its own scope overrides both.
  assert.equal(
    decide("supervised", "bash", [], {
      rules,
      agent: "worker",
      input: { command: "git push origin main" },
    }),
    "deny",
  );
  // Unmatched calls fall through to the `any` allow, then the mode default.
  assert.equal(
    decide("supervised", "bash", [], { rules, input: { command: "git status" } }),
    "allow",
  );
  assert.equal(decide("supervised", "bash", [], { rules, input: { command: "rm -rf x" } }), "ask");
});

test("rules apply in the confined modes, including delegation", () => {
  const deny = [{ scope: "any", target: "exec", pattern: "rm *", action: "deny" }];
  assert.equal(
    decide("supervised", "bash", [], { rules: deny, input: { command: "rm -rf build" } }),
    "deny",
  );
  assert.equal(
    decide("auto-accept-edits", "bash", [], { rules: deny, input: { command: "rm -rf build" } }),
    "deny",
  );
  const noDelegate = [{ scope: "any", target: "delegate", pattern: "", action: "deny" }];
  assert.equal(decide("supervised", "subagent", [], { rules: noDelegate }), "deny");
});

test("full access runs before the rule table: nothing asks, nothing is refused", () => {
  // Full access is a promise, not a baseline: an Ask rule must not turn it back
  // into a prompt, and a Deny rule must not block a mode chosen for trust.
  const rules = [
    { scope: "any", target: "exec", pattern: "rm *", action: "deny" },
    { scope: "any", target: "any", pattern: "*", action: "ask" },
    { scope: "agent:worker", target: "mcp", pattern: "", action: "deny" },
  ];
  for (const [tool, input] of [
    ["bash", { command: "rm -rf build" }],
    ["edit", { path: "a" }],
    ["mcp__github__create_issue", { title: "x" }],
    ["webfetch", { url: "https://example.com" }],
  ]) {
    assert.equal(decide("full-access", tool, [], { rules, agent: "worker", input }), "allow", tool);
  }
});

test("deny beats allow inside one scope", () => {
  const rules = [
    { scope: "any", target: "exec", pattern: "*", action: "allow" },
    { scope: "any", target: "any", pattern: "rm *", action: "deny" },
  ];
  assert.equal(decide("supervised", "bash", [], { rules, input: { command: "rm -rf x" } }), "deny");
  assert.equal(decide("supervised", "bash", [], { rules, input: { command: "ls" } }), "allow");
});

test("an allow rule never covers a chained command", () => {
  const rules = [{ scope: "any", target: "exec", pattern: "ls*", action: "allow" }];
  assert.equal(decide("supervised", "bash", [], { rules, input: { command: "ls -la" } }), "allow");
  assert.equal(
    decide("supervised", "bash", [], { rules, input: { command: "ls; rm -rf build" } }),
    "ask",
  );
});

test("a rule can gate a fetch, which no mode gates by default", () => {
  const rules = [{ scope: "any", target: "net", pattern: "*", action: "ask" }];
  assert.equal(decide("supervised", "webfetch", [], { rules }), "ask");
  assert.equal(decide("supervised", "read", [], { rules }), "allow", "only the net family is gated");
});

test("a malformed rule is dropped, never widened", () => {
  assert.equal(ruleDecision([{ scope: "any", target: "nope", pattern: "", action: "deny" }], null, "bash", "exec", { command: "rm x" }), null);
  assert.equal(ruleDecision([{ scope: "any", target: "exec", pattern: "", action: "maybe" }], null, "bash", "exec", { command: "rm x" }), null);
  assert.equal(ruleDecision([{ scope: "bogus", target: "exec", pattern: "", action: "deny" }], null, "bash", "exec", { command: "rm x" }), null);
  assert.equal(ruleDecision(null, null, "bash", "exec", {}), null);
});

test("readRules loads valid rules and fails closed on malformed stores", () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-rules-"));
  const file = path.join(dir, "access-rules.json");
  try {
    assert.deepEqual(readRules(file), [], "missing file");
    fs.writeFileSync(
      file,
      JSON.stringify({
        version: 1,
        rules: [
          { scope: "any", target: "exec", pattern: "rm *", action: "deny" },
          { scope: "agent:worker", target: "any", pattern: "", action: "ask" },
        ],
      }),
    );
    assert.deepEqual(readRules(file), [
      { scope: "any", target: "exec", pattern: "rm *", action: "deny" },
      { scope: "agent:worker", target: "any", pattern: "", action: "ask" },
    ]);
    for (const pattern of [null, 42, undefined]) {
      fs.writeFileSync(file, JSON.stringify({ rules: [{ scope: "any", target: "exec", pattern, action: "allow" }] }));
      assert.equal(readRules(file), null, "invalid pattern cannot become an unconditional grant");
    }
    for (const raw of ["{not json", "{}", JSON.stringify({ rules: [{ scope: "any", target: "exec", pattern: "", action: "unknown" }] })]) {
      fs.writeFileSync(file, raw);
      assert.equal(readRules(file), null, "invalid store fails closed");
    }
    assert.equal(readRules(dir), null, "unreadable file fails closed");
  } finally {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

test("scopesFor is most-specific-first and never applies an agent scope to a main session", () => {
  assert.deepEqual(scopesFor(null), ["session", "any"]);
  assert.deepEqual(scopesFor(""), ["session", "any"]);
  assert.deepEqual(scopesFor("worker"), ["agent:worker", "any"]);
});

test("rulesFilePath sits beside the mode file", () => {
  const prev = process.env.HOME;
  process.env.HOME = "/tmp/orbit-home-test";
  try {
    assert.equal(rulesFilePath(), "/tmp/orbit-home-test/.orbit-pi/access-rules.json");
  } finally {
    process.env.HOME = prev;
  }
});
