#!/usr/bin/env node
/** Reproducible pi 1.5.2 credential routing patch. --check is read-only. */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";

const BEGIN = "/*orbit-auth-profiles:begin*/";
const END = "/*orbit-auth-profiles:end*/";
const payload = fs.readFileSync(new URL("./routing.js", import.meta.url), "utf8");
const block = `${BEGIN}\n${payload}\n${END}\n`;
const PACKAGE = "@earendil-works/pi-coding-agent";
const fail = () => { throw new Error("Auth-profile routing unavailable: unsupported pi version/layout or incomplete patch (requires pi 1.5.2)"); };

export function packageRoot() {
  if (process.env.PI_PACKAGE_ROOT) return fs.realpathSync(process.env.PI_PACKAGE_ROOT);
  let bin = process.env.PI_BIN;
  if (!bin) {
    for (const dir of (process.env.PATH || "").split(path.delimiter)) {
      const candidate = path.join(dir, process.platform === "win32" ? "pi.cmd" : "pi");
      if (fs.existsSync(candidate)) { bin = candidate; break; }
    }
  }
  if (!bin) fail();
  let dir = path.dirname(fs.realpathSync(bin));
  for (let i = 0; i < 12; i++) {
    try {
      if (JSON.parse(fs.readFileSync(path.join(dir, "package.json"), "utf8")).name === PACKAGE) return dir;
    } catch { /* walk launcher ancestors */ }
    const install = path.join(dir, "install");
    try {
      const version = fs.readFileSync(path.join(install, "current-version"), "utf8").trim();
      if (!/^[\w.-]+$/.test(version)) fail();
      const root = path.join(install, "releases", version, "node_modules", PACKAGE);
      if (fs.existsSync(root)) return root;
    } catch { /* not a managed launcher */ }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  fail();
}

function strip(text) {
  const start = text.indexOf(BEGIN);
  if (start < 0) { if (text.includes(END)) fail(); return text; }
  const end = text.indexOf(END, start);
  if (end < start || text.indexOf(BEGIN, start + BEGIN.length) >= 0 || text.indexOf(END, end + END.length) >= 0) fail();
  return text.slice(0, start) + text.slice(end + END.length).replace(/^\n/, "");
}
function once(text, anchor) {
  if (text.split(anchor).length !== 2) fail();
}
export function plan(root) {
  const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
  if (pkg.name !== PACKAGE || pkg.version !== "1.5.2") fail();
  const files = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(file);
      else if (entry.isFile() && entry.name.endsWith(".js")) {
        const text = fs.readFileSync(file, "utf8");
        if (text.includes("AuthStorage=class _AuthStorage")) files.push({ file, text, bundled: true });
      }
    }
  };
  walk(path.join(root, "dist", "bundle"));
  if (files.length !== 1) fail();
  const file = path.join(root, "dist", "core", "auth-storage.js");
  files.push({ file, text: fs.readFileSync(file, "utf8"), bundled: false });
  return files.map((target) => {
    const text = strip(target.text);
    const anchor = target.bundled ? 'function readStoredCredential(providerId,authPath=join(getAgentDir(),"auth.json")){' : 'export function readStoredCredential(providerId, authPath = join(getAgentDir(), "auth.json")) {';
    for (const item of target.bundled ? [
      'AuthStorage=class _AuthStorage{', 'ReadOnlyAuthStorage=class{',
      'async read(provider,options){', 'async modify(provider,fn,options){',
      'async delete(provider,options){', 'static create(authPath=join(getAgentDir(),"auth.json")){',
      'async reloadFromStorageAsync(options){',
    ] : [
      'export class AuthStorage {', 'export class ReadOnlyAuthStorage {',
      'async read(provider, options) {', 'async modify(provider, fn, options) {',
      'async delete(provider, options) {', 'static create(authPath = join(getAgentDir(), "auth.json")) {',
      'async reloadFromStorageAsync(options) {',
    ]) once(text, item);
    once(text, anchor);
    return { ...target, clean: text, patched: text.replace(anchor, block + anchor) };
  });
}

/** Execute only synthetic credential operations, never the user's auth.json. */
export function probe(targets) {
  const dir = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "orbit-auth-profile-probe-")));
  try {
    const file = path.join(dir, "auth.json");
    fs.writeFileSync(file, JSON.stringify({ orbit_probe: { type: "api_key", key: "synthetic-profile" } }), { mode: 0o600 });
    for (const target of targets) {
      // Importing the module validates route syntax/paths without opening any
      // auth.json. This also checks the caller's real route configuration.
      if (process.env.ORBIT_AUTH_PROFILE_ROUTES !== undefined) {
        try {
          execFileSync(process.execPath, ["--input-type=module", "-e", `await import(${JSON.stringify(pathToFileURL(target.file).href)})`], { env: process.env, stdio: "pipe", timeout: 15000 });
        } catch { throw new Error("Invalid auth-profile routes or unavailable capability"); }
      }
      const script = `
        import assert from "node:assert/strict";
        const {AuthStorage} = await import(${JSON.stringify(pathToFileURL(target.file).href)});
        const store = AuthStorage.inMemory({orbit_probe:{type:"api_key",key:"synthetic-default"}});
        assert.equal((await store.read("orbit_probe")).key,"synthetic-profile");
        await store.delete("orbit_probe");
        await assert.rejects(store.read("orbit_probe"), /auth_required/);
        await store.modify("orbit_probe",()=>({type:"api_key",key:"synthetic-profile"}));
        assert.equal((await store.list()).find(x=>x.providerId==="orbit_probe").type,"api_key");
      `;
      try {
        execFileSync(process.execPath, ["--input-type=module", "-e", script], {
          env: { ...process.env, ORBIT_AUTH_PROFILE_ROUTES: JSON.stringify({ orbit_probe: file }), PI_OFFLINE: "1" },
          stdio: "pipe", timeout: 15000,
        });
      } catch { throw new Error("Auth-profile routing capability probe failed"); }
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}

export function apply(root, mode = "apply") {
  const targets = plan(root); // Validate all anchors before changing any file.
  if (mode === "check") {
    if (targets.some((target) => target.text !== target.patched)) fail();
    probe(targets);
    return;
  }
  for (const target of targets) {
    const next = mode === "revert" ? target.clean : target.patched;
    if (next === target.text) continue;
    const backup = `${target.file}.orbit-auth-profiles-orig`;
    if (!fs.existsSync(backup)) fs.copyFileSync(target.file, backup, fs.constants.COPYFILE_EXCL);
    // Marker-only replacement preserves other independently applied RPC patches.
    const temporary = `${target.file}.orbit-auth-profiles-${process.pid}.tmp`;
    try {
      fs.writeFileSync(temporary, next, { mode: fs.statSync(target.file).mode });
      fs.renameSync(temporary, target.file);
    } finally { fs.rmSync(temporary, { force: true }); }
  }
  if (mode !== "revert") probe(plan(root));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length > 1 || args.some((arg) => !["--check", "--probe", "--revert"].includes(arg))) fail();
    apply(packageRoot(), args.includes("--revert") ? "revert" : args.length ? "check" : "apply");
    console.log("Auth-profile routing: " + (args.includes("--revert") ? "removed" : "verified"));
  } catch {
    console.error("Auth-profile routing unavailable: invalid package, unsupported layout, permission failure, or failed capability probe. No account fallback is safe.");
    process.exitCode = 1;
  }
}
