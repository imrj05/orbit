#!/usr/bin/env node
/**
 * Tests for the inherited guardrail paths.
 *
 * Run: node --test contrib/orbit-subagents-extension/resources.test.mjs
 */

import assert from "node:assert/strict";
import test from "node:test";

import { extensionArgs, parseBundledExtensions } from "./resources.js";

test("an absent or malformed value yields no extensions", () => {
  assert.deepEqual(parseBundledExtensions(undefined), []);
  assert.deepEqual(parseBundledExtensions(""), []);
  assert.deepEqual(parseBundledExtensions("not json"), []);
  assert.deepEqual(parseBundledExtensions('{"a":1}'), []);
});

test("a string array is deduped and keeps order", () => {
  assert.deepEqual(parseBundledExtensions('["/a.js","/b.js","/a.js"]'), ["/a.js", "/b.js"]);
  assert.deepEqual(parseBundledExtensions('["/a.js", 3, null]'), ["/a.js"]);
});

test("the args are --extension pairs", () => {
  assert.deepEqual(extensionArgs('["/a.js","/b.js"]'), [
    "--extension",
    "/a.js",
    "--extension",
    "/b.js",
  ]);
});
