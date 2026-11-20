# Provider accounts

Orbit can keep more than one account for the same provider — two ChatGPT
subscriptions, a work and a personal Anthropic login, separate API keys — and
**all of them are usable at the same time, in every session**. There is no
per-session switch: pick any account's model in the model picker.

## How it works

Every extra account is its own pi provider. The bundled extension
`contrib/orbit-accounts-extension/` reads `~/.orbit-pi/accounts.json` when pi
starts and, for each account, registers a copy of the built-in provider
(`builtinProviders()` from pi-ai) under an alias id and name:

| Account | pi provider id | Shown as |
|---|---|---|
| Default | `openai-codex` | ChatGPT (Codex) |
| Work | `openai-codex~work` | ChatGPT (Codex) · Work |

The copy keeps the provider's models, OAuth login and refresh, and request
handling. pi keys credentials by provider id, so each account signs in,
refreshes and is stored separately — all in pi's own `~/.pi/agent/auth.json`.
Models are reported under the alias (so pi uses that account's credential) and
handed back to the built-in provider when a request streams.

## What is stored where

| Thing | Path | Notes |
|---|---|---|
| Account names, provider, alias | `~/.orbit-pi/accounts.json` | Metadata only, `0600`. Never a token or key. |
| Every credential, Default and accounts | `~/.pi/agent/auth.json` | pi's own store, keyed by provider id / alias. |

The alias slug comes from the name the account was created with and never
changes; renaming changes only the display name, so the sign-in stays.

## Using it

- **Add** — Settings → Providers & models → open a provider → **Accounts** →
  **Add account**. pi restarts (at once when idle, otherwise via the "Restart
  pi" banner) so it registers the new provider, and the account's own row opens.
- **Sign in** — on the account's own row, with any method the provider offers
  (browser, device code, pasted code, API key). It goes through pi's ordinary
  `auth.*` flow under the alias id.
- **Use** — the account's models appear in the model picker as
  `<Provider> · <name>`, next to the Default account's. Sessions, parked
  sessions and subagents can use any of them at once (the extension is
  forwarded to subagent children).
- **Rename / Remove** — from the Accounts panel. Removing signs the account out:
  its entry is deleted from pi's `auth.json`.

## Migration from the per-session design

The first version kept one account per provider per session, routed through a
pi patch (`contrib/pi-auth-profiles/`) with a credential file per profile. On
load, Orbit migrates that store (`provider-accounts.json`, version 1):

1. each profile gets an alias;
2. its credential is **moved** into pi's `auth.json` under the alias — never
   overwriting an entry already there, and never copied, so an OAuth refresh
   token stays single;
3. the per-profile folders and per-session bindings are removed;
4. `provider-accounts.json` is rewritten as an empty version-1 store, so an
   older Orbit build still starts (on Default accounts only).

## Limits

- Built-in providers only. A `models.json` custom provider can be duplicated by
  hand under another id.
- Local sessions only; a remote host uses its own pi providers.
- Adding, renaming or removing an account restarts pi, because providers are
  registered at startup.
- The retired per-session routing still exists in the spawn path but always
  carries an empty selection, so the routing patch is never applied.
