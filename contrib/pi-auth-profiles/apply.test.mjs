import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync, spawn } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { apply, plan } from "./apply.mjs";

const fixture = `import fs from "node:fs";
import {join} from "node:path";
const getAgentDir=()=>{throw new Error("default auth file must not be opened in tests")};
export class AuthStorage {
  constructor(file,data={}) {this.file=file;this.data=data;}
  static create(authPath = join(getAgentDir(), "auth.json")) {return new AuthStorage(authPath);}
  static inMemory(data={}) {return new AuthStorage(undefined,data);}
  async reloadFromStorageAsync(options) {options?.signal?.throwIfAborted();if(this.file){if(!fs.existsSync(this.file))fs.writeFileSync(this.file,"{}");this.data=JSON.parse(fs.readFileSync(this.file,"utf8"));}return this.data;}
  async readLatestData(options) {return this.reloadFromStorageAsync(options);}
  async read(provider, options) {return (await this.readLatestData(options))[provider];}
  async modify(provider, fn, options) {const data=await this.readLatestData(options);const next=await fn(data[provider]);if(next!==undefined){data[provider]=next;if(this.file)fs.writeFileSync(this.file,JSON.stringify(data));}return next??data[provider];}
  async delete(provider, options) {const data=await this.readLatestData(options);delete data[provider];if(this.file)fs.writeFileSync(this.file,JSON.stringify(data));}
  async list(options) {return Object.entries(await this.readLatestData(options)).map(([providerId,c])=>({providerId,type:c.type}));}
}
export class ReadOnlyAuthStorage {
  constructor(file){this.file=file;}
  async read(providerId,options){return (JSON.parse(fs.readFileSync(this.file,"utf8")))[providerId];}
  async list(){return Object.entries(JSON.parse(fs.readFileSync(this.file,"utf8"))).map(([providerId,c])=>({providerId,type:c.type}));}
  async modify(){throw new Error("read only");}
  async delete(){throw new Error("read only");}
}
export function readStoredCredential(providerId, authPath = join(getAgentDir(), "auth.json")) {return undefined;}
`;
const bundledFixture = fixture
  .replace("export class AuthStorage {", "var AuthStorage=class _AuthStorage{")
  .replace("export class ReadOnlyAuthStorage {", "var ReadOnlyAuthStorage=class{")
  .replace('static create(authPath = join(getAgentDir(), "auth.json")) {', 'static create(authPath=join(getAgentDir(),"auth.json")){')
  .replace("async read(provider, options) {", "async read(provider,options){")
  .replace("async modify(provider, fn, options) {", "async modify(provider,fn,options){")
  .replace("async delete(provider, options) {", "async delete(provider,options){")
  .replace("async reloadFromStorageAsync(options) {", "async reloadFromStorageAsync(options){")
  .replace('export function readStoredCredential(providerId, authPath = join(getAgentDir(), "auth.json")) {', 'function readStoredCredential(providerId,authPath=join(getAgentDir(),"auth.json")){') + '\nexport {AuthStorage,ReadOnlyAuthStorage};\n';

function sandbox(t, installed) {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "orbit-auth-profile-test-")));
  t.after(() => fs.rmSync(root, { recursive: true, force: true }));
  fs.mkdirSync(path.join(root, "dist", "core"), { recursive: true });
  fs.mkdirSync(path.join(root, "dist", "bundle"), { recursive: true });
  fs.writeFileSync(path.join(root, "package.json"), JSON.stringify({ name: "@earendil-works/pi-coding-agent", version: "1.5.2", type: "module" }));
  if (installed) {
    fs.cpSync(path.join(installed, "dist", "bundle"), path.join(root, "dist", "bundle"), { recursive: true });
    fs.symlinkSync(path.dirname(path.dirname(installed)), path.join(root, "node_modules"), "dir");
    const source = path.join(installed, "dist", "core", "auth-storage.js");
    const text = fs.readFileSync(source, "utf8").replace(/from "(\.[^"]+)"/g, (_all, ref) => `from ${JSON.stringify(pathToFileURL(path.resolve(path.dirname(source), ref)).href)}`);
    fs.writeFileSync(path.join(root, "dist", "core", "auth-storage.js"), text);
  } else {
    fs.writeFileSync(path.join(root, "dist", "core", "auth-storage.js"), fixture);
    fs.writeFileSync(path.join(root, "dist", "bundle", "fixture.js"), bundledFixture);
  }
  return root;
}
function profile(root, name, data = {}) {
  const dir = path.join(root, name);
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "auth.json");
  fs.writeFileSync(file, JSON.stringify(data), { mode: 0o600 });
  return file;
}
function run(target, script, routes, extra = {}) {
  const env = { ...process.env, PI_OFFLINE: "1", ...extra };
  delete env.ORBIT_AUTH_PROFILE_ROUTES;
  if (routes !== undefined) env.ORBIT_AUTH_PROFILE_ROUTES = typeof routes === "string" ? routes : JSON.stringify(routes);
  return execFileSync(process.execPath, ["--input-type=module", "-e", `import assert from "node:assert/strict"; import fs from "node:fs"; const {AuthStorage,ReadOnlyAuthStorage}=await import(${JSON.stringify(pathToFileURL(target.file).href)}); ${script}`], { env, encoding: "utf8", stdio: "pipe", timeout: 20000 });
}

test("routing isolates read/write/delete/list, excludes foreign profile entries and preserves defaults", (t) => {
  const root = sandbox(t);
  apply(root);
  const first = profile(root, "one", { example: { type: "api_key", key: "one" }, foreign: { type: "api_key", key: "hidden" } });
  const second = profile(root, "two", { example: { type: "api_key", key: "two" } });
  for (const target of plan(root)) {
    for (const [file, expected] of [[first, "one"], [second, "two"]]) {
      run(target, `
        const store=AuthStorage.inMemory({example:{type:"api_key",key:"default"},other:{type:"api_key",key:"other"}});
        assert.equal((await store.read("example")).key,${JSON.stringify(expected)});
        assert.equal((await store.read("other")).key,"other");
        assert.deepEqual((await store.list()).map(x=>x.providerId).sort(),["example","other"]);
        await store.modify("example",c=>({...c,key:"changed"}));
        assert.equal((await store.read("example")).key,"changed");
        await store.delete("example");
        await assert.rejects(store.read("example"),/auth_required/);
        assert.ok(!(await store.list()).some(x=>x.providerId==="example"));
        await store.modify("example",()=>({type:"api_key",key:${JSON.stringify(expected)}}));
      `, { example: file });
    }
    run(target, `const store=AuthStorage.inMemory({example:{type:"api_key",key:"default"}});assert.equal((await store.read("example")).key,"default");await store.delete("example");assert.equal(await store.read("example"),undefined);`);
  }
});

test("empty/deleted/corrupt selected profile never falls back, read-only stores route too", (t) => {
  const root = sandbox(t);
  apply(root);
  const file = profile(root, "selected");
  const defaultFile = profile(root, "default", { example: { type: "api_key", key: "must-not-use" } });
  for (const target of plan(root)) {
    run(target, `
      const store=AuthStorage.inMemory({example:{type:"api_key",key:"must-not-use"}});
      const readonly=new ReadOnlyAuthStorage(${JSON.stringify(defaultFile)});
      await assert.rejects(store.read("example"),/auth_required/);
      await assert.rejects(readonly.read("example"),/auth_required/);
      await readonly.modify("example",()=>({type:"api_key",key:"selected"}));
      assert.equal((await readonly.read("example")).key,"selected");
      fs.unlinkSync(${JSON.stringify(file)});
      await assert.rejects(store.read("example"),/auth_required/);
      fs.writeFileSync(${JSON.stringify(file)},"not-json-private-marker");
      await assert.rejects(store.read("example"),e=>/auth_profile_storage_error/.test(e.message)&&!e.message.includes("private-marker"));
      fs.writeFileSync(${JSON.stringify(file)},"{}");
    `, { example: file }, { EXAMPLE_API_KEY: "ambient-must-not-use" });
  }
});

test("invalid JSON, relative/noncanonical paths, symlinks and malformed route IDs fail closed without reflecting input", (t) => {
  const root = sandbox(t);
  apply(root);
  const file = profile(root, "valid");
  const alias = path.join(root, "alias");
  fs.symlinkSync(path.dirname(file), alias, "dir");
  const linkDir = path.join(root, "link");
  fs.mkdirSync(linkDir);
  fs.symlinkSync(file, path.join(linkDir, "auth.json"));
  const bad = ["private-invalid-json", "[]", "null", '{"__proto__":"private"}', { example: "relative/auth.json" }, { example: path.dirname(file) }, { example: `${path.dirname(file)}/../valid/auth.json` }, { example: `${alias}/auth.json` }, { example: `${linkDir}/auth.json` }, { example: 42 }, { "bad\nprovider": file }];
  for (const target of plan(root)) for (const routes of bad) {
    assert.throws(() => run(target, "throw new Error('must not reach script')", routes), (error) => {
      const stderr = error.stderr.toString();
      return stderr.includes("Invalid ORBIT_AUTH_PROFILE_ROUTES") && !stderr.includes("private-invalid-json");
    });
  }
});

test("apply/check/revert are idempotent, refresh stale payloads, reject drift before writes", (t) => {
  const root = sandbox(t);
  const pristine = plan(root).map(x=>x.text);
  assert.throws(() => apply(root, "check"));
  apply(root);
  const patched = plan(root).map(x=>x.text);
  apply(root);
  assert.deepEqual(plan(root).map(x=>x.text), patched);
  apply(root, "check");
  const cli = fileURLToPath(new URL("./apply.mjs", import.meta.url));
  execFileSync(process.execPath, [cli, "--check"], { env: { ...process.env, PI_PACKAGE_ROOT: root, ORBIT_AUTH_PROFILE_ROUTES: "{}" }, stdio: "pipe" });
  assert.deepEqual(plan(root).map(x=>x.text), patched, "CLI check does not write installation files");
  assert.throws(() => execFileSync(process.execPath, [cli, "--check"], { env: { ...process.env, PI_PACKAGE_ROOT: root, ORBIT_AUTH_PROFILE_ROUTES: "private-invalid-json" }, stdio: "pipe" }), (error) => !error.stderr.toString().includes("private-invalid-json"));
  const first = plan(root)[0];
  for (const malformed of [patched[0].replace("/*orbit-auth-profiles:end*/", ""), patched[0] + "/*orbit-auth-profiles:begin*/", patched[0].replace("/*orbit-auth-profiles:begin*/", "")]) {
    fs.writeFileSync(first.file, malformed);
    assert.throws(() => apply(root));
    fs.writeFileSync(first.file, patched[0]);
  }
  for (const target of plan(root)) fs.writeFileSync(target.file, target.text.replace("const raw =", "/* old */const raw ="));
  assert.throws(() => apply(root, "check"));
  apply(root);
  assert.deepEqual(plan(root).map(x=>x.text), patched);
  apply(root, "revert");
  assert.deepEqual(plan(root).map(x=>x.text), pristine);
  const target = plan(root)[0];
  fs.writeFileSync(target.file,target.text.replace("async modify(provider,fn,options){", "async changed(provider,fn,options){"));
  assert.throws(() => apply(root));
  assert.equal(fs.readFileSync(path.join(root,"dist/core/auth-storage.js"),"utf8"),pristine[1]);
  fs.writeFileSync(path.join(root,"package.json"),JSON.stringify({name:"@earendil-works/pi-coding-agent",version:"1.5.3"}));
  assert.throws(() => apply(root));
});

const installed = process.env.PI_AUTH_PROFILE_TEST_ROOT || path.join(os.homedir(), ".pi/agent/install/releases/1.5.2/node_modules/@earendil-works/pi-coding-agent");
test("real installed 1.5.2 source and minified bundle execute in a temporary sandbox; two processes refresh once under pi's lock", { skip: !fs.existsSync(installed) }, async (t) => {
  const root = sandbox(t, installed);
  apply(root);
  apply(root, "check");
  const resolver = pathToFileURL(path.join(path.dirname(installed), "pi-ai/dist/auth/resolve.js")).href;
  for (const [index,target] of plan(root).entries()) {
    const file = profile(root, `real-${index}`, { example: { type:"oauth",access:"synthetic-old",refresh:"synthetic-refresh",expires:0 } });
    const log = path.join(root,`refresh-${index}.log`);
    const script = `
      import fs from "node:fs"; import assert from "node:assert/strict";
      const {AuthStorage}=await import(${JSON.stringify(pathToFileURL(target.file).href)});
      const store=AuthStorage.inMemory({example:{type:"api_key",key:"wrong-default"}});
      assert.equal((await store.read("example")).type,"oauth");
      const {refreshStoredOAuthCredential}=await import(${JSON.stringify(resolver)});
      await refreshStoredOAuthCredential(store,"example",{refresh:async current=>{
        fs.appendFileSync(${JSON.stringify(log)},"refresh\\n");
        await new Promise(r=>setTimeout(r,150));
        return {...current,access:"synthetic-new",expires:9999999999999};
      }},current=>current.expires===0,new AbortController().signal);
      assert.equal((await store.read("example")).access,"synthetic-new");
    `;
    const child = () => new Promise((resolve,reject) => {
      const proc = spawn(process.execPath,["--input-type=module","-e",script],{env:{...process.env,PI_OFFLINE:"1",ORBIT_AUTH_PROFILE_ROUTES:JSON.stringify({example:file})},stdio:["ignore","pipe","pipe"]});
      let stderr="";proc.stderr.on("data",x=>stderr+=x);proc.on("error",reject);proc.on("exit",code=>code===0?resolve():reject(new Error(stderr)));
    });
    await Promise.all([child(),child()]);
    assert.equal(fs.readFileSync(log,"utf8"),"refresh\n");
    run(target,`
      const store=AuthStorage.inMemory({example:{type:"api_key",key:"default"}});
      assert.equal((await store.read("example")).type,"oauth");
      await store.delete("example");await assert.rejects(store.read("example"),/auth_required/);
      const {resolveProviderAuth}=await import(${JSON.stringify(resolver)});
      let fallbackCalls=0;
      const provider={id:"example",auth:{apiKey:{resolve:async()=>{fallbackCalls++;return {auth:{apiKey:"ambient-or-models-fallback"}};}}}};
      await assert.rejects(resolveProviderAuth(provider,store,{}),/Credential store read failed/);
      assert.equal(fallbackCalls,0);
    `,{example:file});
    run(target,`const store=AuthStorage.inMemory({example:{type:"api_key",key:"default"}});assert.equal((await store.read("example")).key,"default");`);
  }
});
