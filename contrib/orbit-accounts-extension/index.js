// Orbit's provider-accounts extension, loaded with `pi --extension`.
//
// Registers every extra account in ~/.orbit-pi/accounts.json as its
// own provider (`<provider>~<slug>`, named "<Provider> · <label>"). pi keys
// credentials by provider id, so each account signs in and refreshes on its
// own, and all of them are usable in the same session at once. The Default
// account is the built-in provider itself and is left untouched.
//
// Read once at startup: Orbit restarts pi after an account is added, renamed
// or removed, the same way it loads a changed credential.
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { builtinProviders } from "@earendil-works/pi-ai/providers/all";

import { aliasProvider, parseAccounts } from "./aliases.js";

export default function orbitAccounts(pi) {
  let text;
  try {
    text = readFileSync(join(homedir(), ".orbit-pi", "accounts.json"), "utf8");
  } catch {
    return;
  }
  const accounts = parseAccounts(text);
  if (accounts.length === 0) return;
  const bases = new Map(builtinProviders().map((provider) => [provider.id, provider]));
  for (const account of accounts) {
    const base = bases.get(account.provider);
    if (!base) continue;
    try {
      pi.registerProvider(aliasProvider(base, account));
    } catch {
      // One unusable account must not take the others (or the session) down.
    }
  }
}
