#!/usr/bin/env node
/**
 * The seam the unit tests cannot reach: a real `pi` loading this extension
 * next to another extension that registers `subagent`.
 *
 * Run: node --test contrib/orbit-subagents-extension/seam.test.mjs
 *
 * Issue #122 was exactly this shape and no fake could have caught it: pi 1.5
 * treats a tool name registered by two extensions as a fatal load error, and
 * `pi.getAllTools()` throws during load, so the old guard read "no tools yet"
 * and registered second. pi then exited with
 * `Failed to load extension … Tool "subagent" conflicts with …` before it
 * served the session, which took the whole app down, not just the tool.
 *
 * Skipped when `pi` is not on PATH, so this stays a local gate and never blocks
 * a machine without pi installed.
 */

import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const SELF_INDEX = fileURLToPath(new URL("./index.js", import.meta.url));
const PI = findPi();

function findPi() {
  for (const dir of (process.env.PATH ?? "").split(path.delimiter)) {
    if (dir === "") continue;
    const candidate = path.join(dir, process.platform === "win32" ? "pi.cmd" : "pi");
    try {
      fs.accessSync(candidate, fs.constants.X_OK);
      return candidate;
    } catch {
      // next
    }
  }
  return null;
}

const skip = PI === null && "pi is not on PATH";

/** A temp dir pi can call home, so the real `~/.pi` and `~/.orbit-pi` are untouched. */
function sandbox(files = {}) {
  const home = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-subagents-seam-"));
  fs.mkdirSync(path.join(home, ".pi", "agent", "agents"), { recursive: true });
  fs.mkdirSync(path.join(home, ".orbit-pi"), { recursive: true });
  for (const [name, contents] of Object.entries(files)) {
    fs.writeFileSync(path.join(home, name), contents);
  }
  return home;
}

const FOREIGN = `
export default function activate(pi) {
  pi.registerTool({
    name: "subagent",
    label: "Foreign",
    description: "what pi-subagents registers",
    parameters: { type: "object", properties: {} },
    async execute() {
      return { content: [{ type: "text", text: "foreign" }] };
    },
  });
}
`;

/** Reports who owns `subagent` once the session is bound — the fact under test. */
const PROBE = `
export default function activate(pi) {
  pi.on("session_start", () => {
    const owners = pi.getAllTools()
      .filter((tool) => tool.name === "subagent")
      .map((tool) => tool.sourceInfo?.path ?? null);
    console.error("PROBE:" + JSON.stringify(owners));
  });
}
`;

/**
 * Load the given extension files in a real pi. RPC mode binds the session (so
 * `session_start` runs) and exits on stdin EOF, so this needs no model, no
 * network, and no prompt.
 */
function runPi(home) {
  const extensions = [
    path.join(home, "foreign.js"),
    SELF_INDEX,
    path.join(home, "probe.js"),
  ].filter((file) => fs.existsSync(file));
  // Each file needs its own `-e`; a spread after one flag would leave the rest
  // as positional arguments (in RPC mode, a prompt) and silently load nothing.
  const argv = ["--mode", "rpc", ...extensions.flatMap((file) => ["-e", file])];
  const result = spawnSync(PI, argv, {
    cwd: home,
    env: { ...process.env, HOME: home, PI_SKIP_VERSION_CHECK: "1", CI: "1", NO_COLOR: "1" },
    stdio: ["ignore", "pipe", "pipe"],
    encoding: "utf8",
    timeout: 60_000,
  });
  return {
    code: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

function probeOwners(stderr) {
  const line = stderr.split("\n").find((entry) => entry.startsWith("PROBE:"));
  assert.ok(line, `the probe reported nothing. stderr:\n${stderr}`);
  return JSON.parse(line.slice("PROBE:".length));
}

test("a foreign `subagent` no longer kills pi, and Orbit stands down", { skip }, () => {
  const home = sandbox({ "foreign.js": FOREIGN, "probe.js": PROBE });
  const { stdout, stderr } = runPi(home);

  // The bug: this line, followed by exit 1 and no session at all.
  assert.doesNotMatch(stderr, /Failed to load extension/);
  assert.doesNotMatch(stderr, /conflicts with/);
  // Standing down is visible rather than silent (D-C).
  assert.match(stdout, /another extension .* provides a `subagent` tool/);
  // And the name really belongs to the other extension: first registration wins.
  assert.deepEqual(probeOwners(stderr), [path.join(home, "foreign.js")]);
});

test("with the name free, Orbit's tool is the one in the registry", { skip }, () => {
  const home = sandbox({ "probe.js": PROBE });
  const { stdout, stderr } = runPi(home);

  assert.doesNotMatch(stderr, /Failed to load extension/);
  assert.doesNotMatch(stdout, /inactive/);
  assert.deepEqual(probeOwners(stderr), [SELF_INDEX]);
});
