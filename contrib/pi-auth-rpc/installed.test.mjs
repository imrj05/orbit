import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { HELPERS, patchText } from "./apply.mjs";
import { pathToFileURL } from "node:url";

const root = process.env.PI_AUTH_PROFILE_TEST_ROOT || path.join(os.homedir(), ".pi/agent/install/releases/1.5.2/node_modules/@earendil-works/pi-coding-agent");

test("API-key setter persists through real pi 1.5.2 ModelRuntime and refreshes live auth without returning credentials", { skip: !fs.existsSync(root) }, (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-auth-key-test-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const auth = path.join(dir, "auth.json");
  fs.writeFileSync(auth, JSON.stringify({ other: { type: "api_key", key: "synthetic-other" } }), { mode: 0o600 });
  const script = `
    import assert from "node:assert/strict";
    import fs from "node:fs";
    import * as crypto from "node:crypto";
    import { ModelRuntime } from ${JSON.stringify(pathToFileURL(path.join(root, "dist/core/model-runtime.js")).href)};
    globalThis.fetch = async () => { throw new Error("No network permitted in credential tests"); };
    const runtime = await ModelRuntime.create({ authPath: ${JSON.stringify(auth)}, modelsPath: null, refreshOnCreate: false });
    const events = [];
    const handle = new Function("session", "output", "success", "error", "crypto3", ${JSON.stringify(HELPERS + ";return orbitAuthHandle;")})(
      { modelRuntime: runtime }, event => events.push(event),
      (id, command, data) => ({ success: true, command, data }),
      (id, command, error) => ({ success: false, command, error }), crypto);
    const response = await handle({ type: "auth.set_api_key", provider: "anthropic", key: "synthetic-private-key" });
    assert.equal(response.success, true, JSON.stringify(response));
    const stored = JSON.parse(fs.readFileSync(${JSON.stringify(auth)}, "utf8"));
    assert.deepEqual(stored.anthropic, { type: "api_key", key: "synthetic-private-key" });
    assert.equal(stored.other.key, "synthetic-other");
    assert.equal((await runtime.getAuth("anthropic")).auth.apiKey, "synthetic-private-key");
    assert.ok(!JSON.stringify([response, events]).includes("synthetic-private-key"));
  `;
  execFileSync(process.execPath, ["--input-type=module", "-e", script], {
    env: { PATH: process.env.PATH, HOME: dir, USERPROFILE: dir, PI_OFFLINE: "1", ORBIT_AUTH_PROFILE_ROUTES: "{}" }, stdio: "pipe", timeout: 15000,
  });
});
test("installed pi 1.5.2 RPC source and bundled anchors upgrade and parse without modifying the installation", { skip: !fs.existsSync(root) }, (t) => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "orbit-auth-rpc-test-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const files = [{file:path.join(root,"dist/modes/rpc/rpc-mode.js"),source:true}];
  for (const name of fs.readdirSync(path.join(root,"dist/bundle/chunks"))) {
    if (!name.endsWith(".js")) continue;
    const file = path.join(root,"dist/bundle/chunks",name);
    if (fs.readFileSync(file,"utf8").includes("let handleCommand=async command=>{")) files.push({file,source:false});
  }
  assert.equal(files.length,2);
  for (const [index,target] of files.entries()) {
    const original=fs.readFileSync(target.file,"utf8");
    const patched=patchText(original,target.source);
    assert.equal(patchText(patched,target.source),patched);
    const temporary=path.join(dir,`${index}.mjs`);
    fs.writeFileSync(temporary,patched);
    execFileSync(process.execPath,["--check",temporary],{stdio:"pipe"});
  }
});
