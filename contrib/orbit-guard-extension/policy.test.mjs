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
  classify,
  confirmOptions,
  decide,
  formatTitle,
  grantable,
  modeFilePath,
  normalizeMode,
  OPTION_ALWAYS_ALLOW,
  OPTION_ALLOW_ONCE,
  OPTION_DENY,
  readAllowlist,
  readMode,
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
