import test from "node:test";
import assert from "node:assert/strict";
import * as crypto from "node:crypto";
import { HELPERS, CASES, patchText } from "./apply.mjs";

const tick = () => new Promise((resolve) => setImmediate(resolve));
function harness(login) {
  const events = [];
  const timers = new Set();
  const provider = { id: "example", auth: { oauth: {} } };
  const runtime = {
    getProvider: (id) => id === provider.id ? provider : undefined,
    login: async (_provider, _type, interaction) => login(interaction),
    logout: async () => {},
    listCredentials: async () => [],
    hasConfiguredAuth: () => true,
    getProviders: () => [provider],
  };
  const handle = new Function("session", "output", "success", "error", "crypto3", "setTimeout", "clearTimeout", `${HELPERS};return orbitAuthHandle;`)(
    { modelRuntime: runtime }, (event) => events.push(event),
    (id, command, data) => ({ id, command, success: true, data }),
    (id, command, error) => ({ id, command, success: false, error }), crypto,
    (callback) => { timers.add(callback); return callback; }, (callback) => timers.delete(callback),
  );
  return { events, timers, runtime, send: (type, args = {}) => handle({ type, ...args }) };
}

test("text and authorization-code prompts are correlated, single-use, and never echoed", async () => {
  const answers = [];
  const h = harness(async (ui) => {
    answers.push(await ui.prompt({ type: "text", message: "Enterprise host", placeholder: "github.com" }));
    answers.push(await ui.prompt({ type: "manual_code", message: "Paste code" }));
  });
  assert.equal((await h.send("auth.login", { provider: "example", sessionId: "flow" })).success, true);
  let prompt = h.events.find((event) => event.type === "auth_login_prompt");
  assert.equal(prompt.kind, "text");
  for (const args of [{ sessionId: "other", requestId: prompt.requestId, value: "x" }, { sessionId: "flow", requestId: "stale", value: "x" }, { sessionId: "flow", requestId: prompt.requestId, value: 1 }]) {
    assert.equal((await h.send("auth.respond", args)).success, false);
  }
  assert.equal((await h.send("auth.respond", { sessionId: "flow", requestId: prompt.requestId, value: "" })).success, true);
  assert.equal((await h.send("auth.respond", { sessionId: "flow", requestId: prompt.requestId, value: "duplicate" })).success, false);
  await tick();
  prompt = h.events.filter((event) => event.type === "auth_login_prompt").at(-1);
  assert.equal(prompt.kind, "manual_code");
  const response = await h.send("auth.respond", { sessionId: "flow", requestId: prompt.requestId, value: "synthetic-code-do-not-echo" });
  await tick();
  assert.deepEqual(answers, ["", "synthetic-code-do-not-echo"]);
  assert.ok(h.events.some((event) => event.type === "auth_login_succeeded"));
  assert.ok(!JSON.stringify([response, h.events]).includes("synthetic-code-do-not-echo"));
  assert.equal(h.timers.size, 0);
});

test("cancel, logout and timeout unwind pending prompts and reject stale answers", async () => {
  for (const action of ["auth.cancel", "auth.logout", "timeout"]) {
    const h = harness((ui) => ui.prompt({ type: "manual_code", message: "Paste code" }));
    await h.send("auth.login", { provider: "example", sessionId: "flow" });
    const prompt = h.events.find((event) => event.type === "auth_login_prompt");
    if (action === "timeout") [...h.timers][0]();
    else await h.send(action, { sessionId: "flow", provider: "example" });
    await tick();
    assert.equal(h.timers.size, 0);
    assert.ok(h.events.some((event) => event.type === "auth_login_prompt_closed"));
    assert.equal((await h.send("auth.respond", { sessionId: "flow", requestId: prompt.requestId, value: "late" })).success, false);
    assert.ok(h.events.some((event) => action === "timeout" ? event.type === "auth_login_failed" && event.code === "timeout" : event.type === "auth_login_cancelled"));
  }
});

test("a browser callback can cancel only its manual prompt and still finish login", async () => {
  const h = harness(async (ui) => {
    const controller = new AbortController();
    const pending = ui.prompt({ type: "manual_code", message: "Paste code", signal: controller.signal });
    controller.abort();
    await assert.rejects(pending, /cancelled/);
  });
  await h.send("auth.login", { provider: "example", sessionId: "flow" });
  await tick();
  assert.ok(h.events.some((event) => event.type === "auth_login_succeeded"));
  assert.equal(h.events.filter((event) => event.type === "auth_login_prompt_closed").length, 1);
});

test("concurrent provider login is rejected; provider errors never reflect submitted values", async () => {
  const h = harness(async (ui) => { const value = await ui.prompt({ type: "text", message: "Host" }); throw new Error(value); });
  await h.send("auth.login", { provider: "example", sessionId: "flow" });
  assert.equal((await h.send("auth.login", { provider: "example", sessionId: "other" })).success, false);
  const prompt = h.events.find((event) => event.type === "auth_login_prompt");
  assert.equal((await h.send("auth.respond", { sessionId: "flow", requestId: prompt.requestId, value: "x".repeat(16385) })).success, false);
  await h.send("auth.respond", { sessionId: "flow", requestId: prompt.requestId, value: "private-synthetic-value" });
  await tick();
  assert.ok(!JSON.stringify(h.events).includes("private-synthetic-value"));
});

test("select remains automatic and secret prompts fail closed", async () => {
  const h = harness(async (ui) => {
    assert.equal(await ui.prompt({ type: "select", options: [{ id: "browser" }, { id: "device_code" }] }), "device_code");
    await ui.prompt({ type: "secret", message: "API key" });
  });
  await h.send("auth.login", { provider: "example", method: "device_code" });
  await tick();
  assert.ok(h.events.some((event) => event.type === "auth_login_failed" && event.code === "unsupported_method"));
  assert.ok(!h.events.some((event) => event.type === "auth_login_prompt"));
});

test("selected empty profile status ignores ambient/stale state; committed logout sync errors are reconciled", async (t) => {
  const previous = process.env.ORBIT_AUTH_PROFILE_ROUTES;
  process.env.ORBIT_AUTH_PROFILE_ROUTES = '{"example":"/synthetic/auth.json"}';
  t.after(() => {
    if (previous === undefined) delete process.env.ORBIT_AUTH_PROFILE_ROUTES;
    else process.env.ORBIT_AUTH_PROFILE_ROUTES = previous;
  });
  const h = harness(async () => {});
  const status = await h.send("auth.status", { provider: "example" });
  assert.equal(status.data.authenticated, false);
  assert.equal(status.data.credential, "none");
  h.runtime.logout = async () => { throw Object.assign(new Error("committed, sync failed"), { providerId: "example", operation: "logout" }); };
  assert.equal((await h.send("auth.logout", { provider: "example" })).success, true);
  h.runtime.listCredentials = async () => [{ providerId: "example", type: "oauth" }];
  await assert.rejects(h.send("auth.logout", { provider: "example" }), /committed/);
});

test("API key writes use the locked runtime credential path and never echo keys or failures", async () => {
  const h = harness(async () => {});
  const calls = [];
  h.runtime.credentials = { modify: async (id, fn) => { calls.push(["modify", id]); const value = await fn(); assert.deepEqual(value, { type: "api_key", key: "synthetic-private-key" }); } };
  h.runtime.enqueueCredentialOperation = async (id, signal, fn) => { calls.push(["queue", id]); return fn(); };
  h.runtime.synchronizeCredentialState = async (id) => { calls.push(["sync", id]); };
  for (const key of [undefined, "", " ", 1, "x".repeat(16385)]) {
    assert.equal((await h.send("auth.set_api_key", { provider: "example", key })).success, false);
  }
  const saved = await h.send("auth.set_api_key", { provider: "example", key: " synthetic-private-key " });
  assert.equal(saved.success, true);
  assert.deepEqual(calls, [["queue", "example"], ["modify", "example"], ["sync", "example"]]);
  assert.ok(!JSON.stringify([saved, h.events]).includes("synthetic-private-key"));
  h.runtime.credentials.modify = async () => { throw new Error("synthetic-private-key"); };
  const failed = await h.send("auth.set_api_key", { provider: "example", key: "synthetic-private-key" });
  assert.equal(failed.success, false);
  assert.ok(!JSON.stringify(failed).includes("synthetic-private-key"));
  delete h.runtime.credentials;
  assert.equal((await h.send("auth.set_api_key", { provider: "example", key: "synthetic-private-key" })).success, false);
});

test("a following prompt waits for an earlier API-key write and live synchronization", async () => {
  const calls = [];
  let release;
  const waiting = new Promise(resolve => { release = resolve; });
  const runtime = {
    getProvider: () => ({ id: "example" }),
    enqueueCredentialOperation: async (_id, _signal, fn) => fn(),
    credentials: { modify: async (_id, fn) => { calls.push("write"); await waiting; await fn(); } },
    synchronizeCredentialState: async () => { calls.push("sync"); },
  };
  const source = minFixture.replace('default:{', 'case"prompt":return success(id,"prompt");default:{');
  const handle = new Function("session", "output", "success", "error", "crypto3", `${patchText(source)};return handleCommand;`)(
    { modelRuntime: runtime }, () => {},
    (_id, command) => { calls.push(command); return { success: true }; },
    () => ({ success: false }), crypto,
  );
  const save = handle({ type: "auth.set_api_key", provider: "example", key: "synthetic" });
  const prompt = handle({ type: "prompt" });
  await tick();
  assert.deepEqual(calls, ["write"]);
  release();
  await Promise.all([save, prompt]);
  assert.deepEqual(calls, ["write", "sync", "auth.set_api_key", "prompt"]);
});

const minFixture = 'let handleCommand=async command=>{let id=command.id;switch(command.type){default:{let unknownCommand=command;return error(id,unknownCommand.type,`Unknown command: ${unknownCommand.type}`)}}};';
test("patch updates old blocks, is byte-idempotent and rejects drift/incomplete markers", () => {
  const patched = patchText(minFixture);
  assert.equal(patchText(patched), patched);
  const legacy = patched.replace(HELPERS, '/*orbit-auth-rpc:begin*/obsolete/*orbit-auth-rpc:end*/\n').replace(CASES, CASES.replace('case"auth.respond":', "").replace('case"auth.set_api_key":', ""));
  assert.equal(patchText(legacy), patched);
  for (const bad of [minFixture.replace("unknownCommand", "changed"), patched.replace("/*orbit-auth-rpc:end*/", ""), patched + HELPERS, minFixture + minFixture]) assert.throws(() => patchText(bad));
});

test("unbundled source has guarded anchors and uses its crypto import", () => {
  const source = 'const handleCommand = async (command) => { switch(command.type){default: {\n                const unknownCommand = command;\n                return error(id, unknownCommand.type, `Unknown command: ${unknownCommand.type}`);\n            }}};';
  const patched = patchText(source, true);
  assert.ok(patched.includes("crypto.randomUUID()"));
  assert.ok(!patched.includes("crypto3."));
  assert.equal(patchText(patched, true), patched);
});
