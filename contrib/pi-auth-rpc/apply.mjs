#!/usr/bin/env node
/**
 * Apply the `auth.*` RPC capability to an installed pi build.
 *
 * pi's RPC server has no provider-auth commands, so Orbit would otherwise fall
 * back to launching `pi /login` in a terminal. This script injects five command
 * handlers (`auth.list`, `auth.status`, `auth.login`, `auth.logout`,
 * `auth.cancel`) into pi's bundled RPC mode. The handlers reuse pi's own
 * `session.modelRuntime` (ModelRuntime + auth.json) — Orbit never sees a token.
 *
 * The matching client contract is crates/orbit-rpc/docs/auth-rpc.md.
 *
 * Usage:
 *   node contrib/pi-auth-rpc/apply.mjs            # patch the resolved pi
 *   node contrib/pi-auth-rpc/apply.mjs --revert   # restore the backup
 *   PI_BIN=/path/to/pi node contrib/pi-auth-rpc/apply.mjs
 *
 * A `.orbit-orig` backup is written next to the patched file. The patch is
 * idempotent and is safe to re-run after a pi update (it will not apply twice
 * to the same file).
 *
 * This modifies a global npm install. It is intentionally a local,
 * reproducible stop-gap until pi ships auth.* upstream.
 */

import { execSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const MARKER_BEGIN = "/*orbit-auth-rpc:begin*/";
const MARKER_END = "/*orbit-auth-rpc:end*/";

const ANCHOR_HELPERS = "let handleCommand=async command=>{";
const ANCHOR_CASES =
  "default:{let unknownCommand=command;return error(id,unknownCommand.type," +
  "`Unknown command: ${unknownCommand.type}`)}";

/**
 * Injected immediately before pi's command switch. It only depends on names
 * already in `runRpcMode`'s scope: `session`, `output`, `success`, `error`,
 * and `crypto3`.
 */
export const HELPERS = `${MARKER_BEGIN}
const orbitAuthSessions=new Map();
let orbitAuthKeyWrites=Promise.resolve();
const orbitAuthPrompt=(entry,sessionId,prompt)=>new Promise((resolve,reject)=>{const requestId=crypto3.randomUUID();const signal=prompt.signal?AbortSignal.any([entry.controller.signal,prompt.signal]):entry.controller.signal;const close=(reason,value)=>{if(!entry.prompts.has(requestId))return;entry.prompts.delete(requestId);signal.removeEventListener("abort",onAbort);output({type:"auth_login_prompt_closed",sessionId,provider:entry.providerId,requestId,reason});if(reason==="answered")resolve(value);else reject(new Error("Login cancelled"))};const onAbort=()=>close("cancelled");entry.prompts.set(requestId,{answer:value=>close("answered",value),cancel:()=>close("cancelled")});if(signal.aborted){onAbort();return}signal.addEventListener("abort",onAbort,{once:true});output({type:"auth_login_prompt",sessionId,provider:entry.providerId,requestId,kind:prompt.type,message:prompt.message,placeholder:prompt.placeholder})});
const orbitAuthClassify=(err)=>{const message=err instanceof Error?err.message:String(err);const lower=message.toLowerCase();if(lower.includes("cancel"))return "cancelled";if(lower.includes("timed out")||lower.includes("timeout"))return "timeout";if(lower.includes("unknown provider"))return "invalid_provider";if(lower.includes("unsupported")||lower.includes("not available")||lower.includes("only available"))return "unsupported_method";if(lower.includes("auth.json")||lower.includes("persist")||lower.includes("storage")||lower.includes("locked"))return "storage_error";if(lower.includes("fetch")||lower.includes("network")||lower.includes("http request failed")||lower.includes("econn")||lower.includes("enotfound"))return "network_error";return "login_failed"};
const orbitAuthProfileSelected=(providerId)=>{try{return Object.hasOwn(JSON.parse(process.env.ORBIT_AUTH_PROFILE_ROUTES||"{}"),providerId)}catch{return false}};
const orbitAuthAuthenticated=(providerId,kinds)=>orbitAuthProfileSelected(providerId)?kinds.has(providerId):session.modelRuntime.hasConfiguredAuth(providerId);
const orbitAuthCredentialKinds=async()=>{const list=await session.modelRuntime.listCredentials();return new Map(list.map((info)=>[info.providerId,info.type]))};
const orbitAuthProviderPayload=async(provider,kinds)=>{const id=provider.id;const oauth=provider.auth&&provider.auth.oauth;const apiKey=provider.auth&&provider.auth.apiKey;const methods=[];if(oauth){const label=oauth.loginLabel||oauth.name||"Sign in";methods.push({id:"browser",label});methods.push({id:"device_code",label:"Use a device code"})}if(apiKey&&apiKey.login){methods.push({id:"api_key",label:apiKey.name||"API key"})}return{id,name:provider.name||id,credential:kinds.get(id)||"none",authenticated:orbitAuthAuthenticated(id,kinds),methods}};
const orbitAuthHandle=async(command)=>{const id=command.id;
if(command.type==="auth.list"){const kinds=await orbitAuthCredentialKinds();const providers=[];for(const provider of session.modelRuntime.getProviders()){providers.push(await orbitAuthProviderPayload(provider,kinds))}return success(id,"auth.list",{providers})}
if(command.type==="auth.status"){const providerId=command.provider;const provider=session.modelRuntime.getProvider(providerId);if(!provider)return error(id,"auth.status","invalid_provider: unknown provider");const kinds=await orbitAuthCredentialKinds();return success(id,"auth.status",{id:providerId,authenticated:orbitAuthAuthenticated(providerId,kinds),credential:kinds.get(providerId)||"none"})}
if(command.type==="auth.login"){const providerId=command.provider;const method=command.method||"oauth";const authType=method==="api_key"?"api_key":"oauth";const sessionId=command.sessionId||crypto3.randomUUID();const provider=session.modelRuntime.getProvider(providerId);if(!provider)return error(id,"auth.login","invalid_provider: unknown provider");if(authType==="oauth"&&!(provider.auth&&provider.auth.oauth))return error(id,"auth.login","unsupported_method: provider does not support OAuth");if(authType==="api_key"&&!(provider.auth&&provider.auth.apiKey&&provider.auth.apiKey.login))return error(id,"auth.login","unsupported_method: provider has no interactive API-key login");if(typeof sessionId!=="string"||!sessionId||sessionId.length>128)return error(id,"auth.login","invalid_session");if(orbitAuthSessions.has(sessionId)||[...orbitAuthSessions.values()].some(entry=>entry.providerId===providerId))return error(id,"auth.login","already_in_progress");const controller=new AbortController();const entry={controller,providerId,prompts:new Map(),timedOut:false};orbitAuthSessions.set(sessionId,entry);const timer=setTimeout(()=>{entry.timedOut=true;controller.abort()},5*60*1000);output({type:"auth_login_started",sessionId,provider:providerId,method,expiresAt:Date.now()+5*60*1000});const interaction={signal:controller.signal,notify(event){if(event.type==="auth_url"){output({type:"auth_login_url",sessionId,provider:providerId,url:event.url})}else if(event.type==="device_code"){const expiresAt=typeof event.expiresInSeconds==="number"?Date.now()+event.expiresInSeconds*1000:undefined;output({type:"auth_device_code",sessionId,provider:providerId,userCode:event.userCode,verificationUri:event.verificationUri,expiresAt})}else if(event.type==="progress"||event.type==="info"){output({type:"auth_login_waiting",sessionId,provider:providerId,message:event.message})}},prompt(prompt){if(prompt.type==="select"){const options=prompt.options||[];const preferred=method==="device_code"?"device_code":"browser";const chosen=options.find((option)=>option.id===preferred)||options[0];if(!chosen)return Promise.reject(new Error("Login cancelled: no options available"));return Promise.resolve(chosen.id)}if(prompt.type==="text"||prompt.type==="manual_code"){return orbitAuthPrompt(entry,sessionId,prompt)}return Promise.reject(new Error("unsupported_method: this login prompt is not supported"))}};void (async()=>{try{await session.modelRuntime.login(providerId,authType,interaction);output({type:"auth_login_succeeded",sessionId,provider:providerId,method,credential:authType});output({type:"auth_credentials_changed",providers:[providerId]})}catch(loginError){if(entry.timedOut){output({type:"auth_login_failed",sessionId,provider:providerId,code:"timeout",message:"Login timed out"})}else if(controller.signal.aborted){output({type:"auth_login_cancelled",sessionId,provider:providerId})}else{output({type:"auth_login_failed",sessionId,provider:providerId,code:orbitAuthClassify(loginError),message:"Provider sign-in failed"})}}finally{clearTimeout(timer);for(const pending of entry.prompts.values())pending.cancel();if(orbitAuthSessions.get(sessionId)===entry)orbitAuthSessions.delete(sessionId)}})();return success(id,"auth.login",{sessionId,status:"pending"})}
if(command.type==="auth.set_api_key"){const providerId=command.provider;const runtime=session.modelRuntime;if(!runtime.getProvider(providerId))return error(id,"auth.set_api_key","invalid_provider: unknown provider");if(typeof command.key!=="string"||!command.key.trim()||command.key.length>16384)return error(id,"auth.set_api_key","invalid_value: expected a nonempty key up to 16384 characters");if([...orbitAuthSessions.values()].some(entry=>entry.providerId===providerId))return error(id,"auth.set_api_key","already_in_progress");if(typeof runtime.enqueueCredentialOperation!=="function"||typeof runtime.credentials?.modify!=="function"||typeof runtime.synchronizeCredentialState!=="function")return error(id,"auth.set_api_key","unsupported: credential writer unavailable");const key=command.key.trim();const operation=orbitAuthKeyWrites.then(async()=>{try{const signal=new AbortController().signal;await runtime.enqueueCredentialOperation(providerId,signal,async()=>{await runtime.credentials.modify(providerId,async()=>({type:"api_key",key}),{signal});await runtime.synchronizeCredentialState(providerId,"set_api_key",undefined,signal)});output({type:"auth_credentials_changed",providers:[providerId]});return success(id,"auth.set_api_key",{provider:providerId})}catch{return error(id,"auth.set_api_key","storage_error: API key could not be saved")}});orbitAuthKeyWrites=operation.then(()=>undefined,()=>undefined);return await operation}
if(command.type==="auth.respond"){ const entry=orbitAuthSessions.get(command.sessionId);const pending=entry&&entry.prompts.get(command.requestId);if(!pending)return error(id,"auth.respond","invalid_request: no pending login prompt");if(typeof command.value!=="string"||command.value.length>16384)return error(id,"auth.respond","invalid_value: expected text up to 16384 characters");pending.answer(command.value);return success(id,"auth.respond")}
if(command.type==="auth.cancel"){const entry=orbitAuthSessions.get(command.sessionId);if(entry)entry.controller.abort();return success(id,"auth.cancel")}
if(command.type==="auth.logout"){const providerId=command.provider;const provider=session.modelRuntime.getProvider(providerId);if(!provider)return error(id,"auth.logout","invalid_provider: unknown provider");for(const[sid,entry]of orbitAuthSessions){if(entry.providerId===providerId){entry.controller.abort();orbitAuthSessions.delete(sid)}}try{await session.modelRuntime.logout(providerId)}catch(logoutError){if(!orbitAuthProfileSelected(providerId)||logoutError?.providerId!==providerId||logoutError?.operation!=="logout"||(await orbitAuthCredentialKinds()).has(providerId))throw logoutError}output({type:"auth_credentials_changed",providers:[providerId]});return success(id,"auth.logout",{provider:providerId,status:"signed_out"})}
return undefined};
${MARKER_END}
`;

export const CASES =
  'case"auth.list":case"auth.status":case"auth.login":case"auth.logout":' +
  'case"auth.cancel":case"auth.respond":case"auth.set_api_key":return await orbitAuthHandle(command);';

const PREVIOUS_CASES = CASES.replace('case"auth.set_api_key":', "");
const LEGACY_CASES = PREVIOUS_CASES.replace('case"auth.respond":', "");
// RPC input lines run concurrently in pi 1.5.2. Later prompts must wait for
// previously received key writes, including their live catalog synchronization.
const KEY_GATE = '/*orbit-auth-key-gate*/if(command.type!=="auth.set_api_key")await orbitAuthKeyWrites;';
const SOURCE_HELPERS = "const handleCommand = async (command) => {";
const SOURCE_CASES = 'default: {\n                const unknownCommand = command;\n                return error(id, unknownCommand.type, `Unknown command: ${unknownCommand.type}`);\n            }';

export function patchText(original, source = false) {
  let text = original;
  if (text.includes(KEY_GATE)) {
    if (text.split(KEY_GATE).length !== 2) throw new Error("Invalid auth key gate");
    text = text.replace(KEY_GATE, "");
  }
  const begin = text.indexOf(MARKER_BEGIN);
  if (begin >= 0) {
    const end = text.indexOf(MARKER_END, begin);
    if (end < 0 || text.indexOf(MARKER_BEGIN, begin + MARKER_BEGIN.length) >= 0 || text.indexOf(MARKER_END, end + MARKER_END.length) >= 0) throw new Error("Invalid auth patch markers");
    text = text.slice(0, begin) + text.slice(end + MARKER_END.length).replace(/^\n/, "");
    const cases = [CASES, PREVIOUS_CASES, LEGACY_CASES].find((cases) => text.includes(cases));
    if (!cases) throw new Error("Invalid auth patch cases");
    if (text.split(cases).length !== 2) throw new Error("Invalid auth patch cases");
    text = text.replace(cases, "");
  } else if (text.includes(MARKER_END) || text.includes('case"auth.list":') || text.includes('case "auth.list":') || text.includes("const orbitAuthSessions=")) {
    throw new Error("Incomplete auth patch");
  }
  const helpers = source ? SOURCE_HELPERS : ANCHOR_HELPERS;
  const cases = source ? SOURCE_CASES : ANCHOR_CASES;
  if (text.split(helpers).length !== 2 || text.split(cases).length !== 2) throw new Error("Unsupported RPC anchors");
  return text.replace(helpers, `${source ? HELPERS.replaceAll("crypto3.", "crypto.") : HELPERS}${helpers}${KEY_GATE}`).replace(cases, `${CASES}${cases}`);
}

function resolvePiBin() {
  if (process.env.PI_BIN) return process.env.PI_BIN;
  try {
    return execSync("command -v pi", { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}

const PI_PACKAGE = "@earendil-works/pi-coding-agent";

function packageRoot(bin) {
  // Orbit resolves the package (npm/pnpm global or pi's managed installer) and
  // passes it in, so the script works on layouts its own walk cannot reach.
  if (process.env.PI_PACKAGE_ROOT) return process.env.PI_PACKAGE_ROOT;
  const real = fs.realpathSync(bin);
  let dir = path.dirname(real);
  for (let i = 0; i < 12; i += 1) {
    const pkg = path.join(dir, "package.json");
    if (fs.existsSync(pkg)) {
      try {
        if (JSON.parse(fs.readFileSync(pkg, "utf8")).name === PI_PACKAGE) {
          return dir;
        }
      } catch {
        /* keep walking */
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  // pi's managed installer: <agent>/bin/pi -> <agent>/install/releases/<v>/…
  const install = path.join(path.dirname(path.dirname(real)), "install");
  try {
    const version = fs
      .readFileSync(path.join(install, "current-version"), "utf8")
      .trim();
    const root = path.join(install, "releases", version, "node_modules", PI_PACKAGE);
    if (fs.existsSync(root)) return root;
  } catch {
    /* not a managed install */
  }
  throw new Error(`Could not locate the pi package from ${bin}`);
}

function findTargets(root) {
  const bundleDir = path.join(root, "dist", "bundle");
  const targets = [];
  const walk = (dir) => {
    if (!fs.existsSync(dir)) return;
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (entry.name.endsWith(".js")) {
        const text = fs.readFileSync(full, "utf8");
        if (text.includes(ANCHOR_HELPERS) && text.includes(ANCHOR_CASES)) {
          targets.push({ path: full, text });
        }
      }
    }
  };
  walk(bundleDir);
  const source = path.join(root, "dist", "modes", "rpc", "rpc-mode.js");
  if (fs.existsSync(source)) targets.push({ path: source, text: fs.readFileSync(source, "utf8"), source: true });
  return targets;
}

function revert(target) {
  const backup = `${target.path}.orbit-orig`;
  if (!fs.existsSync(backup)) {
    console.log(`No backup for ${target.path}; nothing to revert.`);
    return;
  }
  fs.copyFileSync(backup, target.path);
  console.log(`Reverted ${target.path} from backup.`);
}

function main() {
  const bin = resolvePiBin();
  if (!bin) {
    console.error("Could not find `pi`. Set PI_BIN or put pi on PATH.");
    process.exitCode = 1;
    return;
  }
  const root = packageRoot(bin);
  const targets = findTargets(root);
  if (targets.length === 0) {
    console.error(
      `No RPC-mode bundle found under ${root}. The pi layout may have changed.`,
    );
    process.exitCode = 1;
    return;
  }

  const reverting = process.argv.includes("--revert");
  for (const target of targets) {
    if (reverting) {
      revert(target);
      continue;
    }
    let patched;
    try { patched = patchText(target.text, target.source); }
    catch {
      console.error("Unsupported RPC layout; not patching.");
      process.exitCode = 1;
      continue;
    }
    if (target.text === patched) {
      console.log(`Already patched: ${target.path}`);
      continue;
    }
    const backup = `${target.path}.orbit-orig`;
    if (!fs.existsSync(backup)) {
      fs.copyFileSync(target.path, backup);
    }
    fs.writeFileSync(target.path, patched);
    console.log(`Patched ${target.path}`);
  }
  if (!reverting) {
    console.log("");
    console.log("Restart pi (Settings → Runtime → Restart, or reopen Orbit) to load the patch.");
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
