#!/usr/bin/env node
/**
 * Tests for the child event log.
 *
 * Run: node --test contrib/orbit-subagents-extension/log.test.mjs
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import { logDir, openChildLog, pruneLogs } from "./log.js";

function tempHome() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "orbit-log-test-"));
}

test("a child's lines are appended verbatim, one per line", () => {
  const home = tempHome();
  const log = openChildLog({ home, run: "r1", child: "c1" });
  assert.ok(log);
  assert.equal(log.path, path.join(logDir(home), "r1-c1.jsonl"));
  log.append(['{"type":"agent_start"}', '{"type":"turn_start"}']);
  log.append([]);
  log.append(['{"type":"agent_end"}']);
  log.close();
  assert.equal(
    fs.readFileSync(log.path, "utf8"),
    '{"type":"agent_start"}\n{"type":"turn_start"}\n{"type":"agent_end"}\n',
  );
  if (process.platform !== "win32") {
    assert.equal(fs.statSync(log.path).mode & 0o777, 0o600);
  }
});

test("names cannot leave the log directory, and a taken name is refused", () => {
  const home = tempHome();
  const log = openChildLog({ home, run: "../../etc", child: "c/1" });
  assert.equal(path.dirname(log.path), logDir(home));
  assert.equal(path.basename(log.path), "etc-c1.jsonl");
  log.close();
  assert.equal(openChildLog({ home, run: "../../etc", child: "c/1" }), null);
});

test("no home or an unwritable directory yields no log, not an error", () => {
  assert.equal(openChildLog({ home: "", run: "r", child: "c" }), null);
  const failing = { mkdirSync() { throw new Error("EACCES"); } };
  assert.equal(openChildLog({ home: "/x", run: "r", child: "c", io: failing }), null);
});

test("a write failure closes the log quietly", () => {
  let closed = 0;
  const io = {
    mkdirSync() {},
    openSync() { return 7; },
    writeSync() { throw new Error("ENOSPC"); },
    closeSync() { closed += 1; },
  };
  const log = openChildLog({ home: "/h", run: "r", child: "c", io });
  log.append(["a"]);
  log.append(["b"]);
  log.close();
  assert.equal(closed, 1);
});

test("prune removes only week-old logs", () => {
  const home = tempHome();
  const dir = logDir(home);
  fs.mkdirSync(dir, { recursive: true });
  const old = path.join(dir, "old-c1.jsonl");
  const fresh = path.join(dir, "new-c1.jsonl");
  const other = path.join(dir, "notes.txt");
  for (const file of [old, fresh, other]) fs.writeFileSync(file, "x");
  const now = Date.now();
  const eightDays = (now - 8 * 24 * 60 * 60 * 1000) / 1000;
  fs.utimesSync(old, eightDays, eightDays);
  fs.utimesSync(other, eightDays, eightDays);
  assert.equal(pruneLogs({ home, now }), 1);
  assert.equal(fs.existsSync(old), false);
  assert.equal(fs.existsSync(fresh), true);
  assert.equal(fs.existsSync(other), true);
  assert.equal(pruneLogs({ home: tempHome(), now }), 0, "a missing directory is fine");
});
