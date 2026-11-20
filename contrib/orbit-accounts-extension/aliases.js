// Pure helpers for Orbit's provider-accounts extension. No I/O here: index.js
// reads the account store and hands its text in, so this file is testable
// offline with `node --test`.
//
// An account is an *alias provider*: a copy of a built-in provider under its
// own id (`<provider>~<slug>`). pi keys credentials by provider id, so each
// alias signs in, refreshes and stores its credential separately in pi's own
// auth.json, while every account's models are live in the same process.

const ALIAS = /^[a-z0-9][a-z0-9._-]*~[a-z0-9][a-z0-9-]{0,47}$/;

/// The accounts in `~/.orbit-pi/accounts.json`, as
/// `{ provider, label, alias }`. Anything malformed is skipped, never guessed.
export function parseAccounts(text) {
  let data;
  try {
    data = JSON.parse(text);
  } catch {
    return [];
  }
  const profiles = data && typeof data === "object" ? data.profiles : null;
  if (!profiles || typeof profiles !== "object") return [];
  const out = [];
  const seen = new Set();
  for (const profile of Object.values(profiles)) {
    if (!profile || typeof profile !== "object") continue;
    const { provider, label, alias } = profile;
    if (typeof provider !== "string" || typeof label !== "string") continue;
    if (typeof alias !== "string" || !ALIAS.test(alias)) continue;
    if (!alias.startsWith(`${provider}~`) || seen.has(alias)) continue;
    if (!label.trim()) continue;
    seen.add(alias);
    out.push({ provider, label: label.trim(), alias });
  }
  return out;
}

/// A provider that behaves exactly like `base` under the account's id and
/// name. Models are reported under the alias (so pi resolves the alias's
/// credential) and handed back to `base` with its own id when it streams.
export function aliasProvider(base, account) {
  const id = account.alias;
  const toAlias = (model) => (model ? { ...model, provider: id } : model);
  const toBase = (model) => (model ? { ...model, provider: base.id } : model);
  const call = (name) =>
    typeof base[name] === "function" ? base[name].bind(base) : undefined;

  const provider = {
    id,
    name: `${base.name} · ${account.label}`,
    baseUrl: base.baseUrl,
    headers: base.headers,
    auth: base.auth,
    refreshModelsAuth: base.refreshModelsAuth,
    getModels: () => (base.getModels() ?? []).map(toAlias),
    stream: (model, context, options) => base.stream(toBase(model), context, options),
    streamSimple: (model, context, options) =>
      base.streamSimple(toBase(model), context, options),
  };
  if (call("getAllModels")) {
    provider.getAllModels = () => (base.getAllModels() ?? []).map(toAlias);
  }
  if (call("refreshModels")) provider.refreshModels = call("refreshModels");
  if (call("filterModels")) {
    provider.filterModels = (models, credential) =>
      base.filterModels(models.map(toBase), credential).map(toAlias);
  }
  if (call("filterAllModels")) {
    provider.filterAllModels = (models, credential) =>
      base.filterAllModels(models.map(toBase), credential).map(toAlias);
  }
  if (call("fetchDeferred")) {
    provider.fetchDeferred = (model, handle, options) =>
      base.fetchDeferred(toBase(model), handle, options);
  }
  if (call("cancelDeferred")) {
    provider.cancelDeferred = (model, handle, options) =>
      base.cancelDeferred(toBase(model), handle, options);
  }
  if (call("generateImages")) {
    provider.generateImages = (model, context, options) =>
      base.generateImages(toBase(model), context, options);
  }
  if (call("classify")) {
    provider.classify = (model, context, options) =>
      base.classify(toBase(model), context, options);
  }
  return provider;
}
