// Injected into pi's auth-storage module (source and CLI bundle).
import * as orbitProfileFs from "node:fs";
import * as orbitProfilePath from "node:path";
(function orbitInstallAuthProfiles() {
  const raw = process.env.ORBIT_AUTH_PROFILE_ROUTES;
  if (raw === undefined) return;
  const invalid = () => new Error("Invalid ORBIT_AUTH_PROFILE_ROUTES: expected provider IDs mapped to canonical writable absolute auth.json paths");
  let routes;
  try {
    if (raw.length > 65536) throw invalid();
    const parsed = JSON.parse(raw);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed) || Object.keys(parsed).length > 128) throw invalid();
    routes = new Map(Object.entries(parsed));
    for (const [provider, file] of routes) {
      if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,127}$/.test(provider) || ["constructor", "prototype", "__proto__"].includes(provider)) throw invalid();
      if (typeof file !== "string" || file.includes("\0") || !orbitProfilePath.isAbsolute(file) || orbitProfilePath.resolve(file) !== file || orbitProfilePath.basename(file) !== "auth.json") throw invalid();
      const parent = orbitProfilePath.dirname(file);
      if (orbitProfileFs.realpathSync(parent) !== parent) throw invalid();
      orbitProfileFs.accessSync(parent, orbitProfileFs.constants.W_OK);
      if (orbitProfileFs.existsSync(file)) {
        if (orbitProfileFs.realpathSync(file) !== file || !orbitProfileFs.lstatSync(file).isFile()) throw invalid();
        orbitProfileFs.accessSync(file, orbitProfileFs.constants.R_OK | orbitProfileFs.constants.W_OK);
      } else if (orbitProfileFs.lstatSync(file, { throwIfNoEntry: false })) throw invalid(); // dangling symlink
    }
  } catch { throw invalid(); } // Never include the supplied JSON or filesystem errors.

  const originals = {};
  for (const name of ["read", "modify", "delete", "list"]) originals[name] = AuthStorage.prototype[name];
  const stores = new Map();
  const internal = new WeakSet();
  const required = () => new Error("auth_required: selected auth profile has no credential; sign in to that profile");
  const storageError = () => new Error("auth_profile_storage_error: selected auth profile could not be accessed");
  const storeFor = (provider) => {
    const file = routes.get(provider);
    if (!stores.has(file)) {
      // Construct pi's real writable backend, not a snapshot/copy of credentials.
      const store = AuthStorage.create(file);
      internal.add(store);
      stores.set(file, store);
    }
    return stores.get(file);
  };
  const guarded = async (fn, options) => {
    try { return await fn(); }
    catch { options?.signal?.throwIfAborted(); throw storageError(); }
  };
  const latest = (store, options) => guarded(async () => {
    // pi normally preserves an old snapshot on read failure. A selected profile
    // must not silently reuse that snapshot after deletion/corruption/lock failure.
    const data = await store.reloadFromStorageAsync(options);
    if (!data || typeof data !== "object" || Array.isArray(data)) throw storageError();
    return data;
  }, options);
  const routedRead = async (provider, options) => {
    const store = storeFor(provider);
    const data = await latest(store, options);
    if (!Object.hasOwn(data, provider)) throw required();
    const credential = data[provider];
    if (!credential || !["oauth", "api_key"].includes(credential.type)) throw required();
    const result = await guarded(() => originals.read.call(store, provider, options), options);
    // The file can change between the locked reload and pi's own cached read.
    if (!result) throw required();
    return result;
  };
  for (const Storage of [AuthStorage, ReadOnlyAuthStorage]) {
    for (const name of ["read", "modify", "delete"]) {
      const original = Storage.prototype[name];
      Storage.prototype[name] = async function(provider, ...args) {
        if (internal.has(this) || !routes.has(provider)) return original.call(this, provider, ...args);
        if (name === "read") return routedRead(provider, args[0]);
        const options = name === "modify" ? args[1] : args[0];
        return guarded(() => originals[name].call(storeFor(provider), provider, ...args), options);
      };
    }
    const originalList = Storage.prototype.list;
    Storage.prototype.list = async function(options) {
      if (internal.has(this) || routes.size === 0) return originalList.call(this, options);
      const entries = (await originalList.call(this, options)).filter((entry) => !routes.has(entry.providerId));
      for (const [provider] of routes) {
        const data = await latest(storeFor(provider), options);
        const credential = Object.hasOwn(data, provider) ? data[provider] : undefined;
        if (credential && ["oauth", "api_key"].includes(credential.type)) entries.push({ providerId: provider, type: credential.type });
      }
      return entries;
    };
  }
})();
