# pi `auth.*` RPC patch

Orbit talks to `pi --mode rpc` for **all** provider authentication. pi's RPC
server does not ship the `auth.*` commands yet, so without this patch Orbit
falls back to the legacy path: it launches `pi /login <provider>` in
Terminal.app instead of opening the browser.

This directory contains a small, reproducible patch that adds six
`auth.*` commands to the **installed** pi build by reusing pi's own
`ModelRuntime` and `~/.pi/agent/auth.json`. It is a stop-gap until pi ships
the handlers upstream. The client-side contract is
[`../../crates/orbit-rpc/docs/auth-rpc.md`](../../crates/orbit-rpc/docs/auth-rpc.md).

## Apply

```sh
node contrib/pi-auth-rpc/apply.mjs
```

Then restart the agent: **Settings → Runtime → Restart**, or reopen Orbit.

The script resolves `pi` on `PATH` (override with `PI_BIN`), finds the bundled
RPC mode (bundle and source module), and injects the handlers. It is idempotent,
updates an older marker-delimited auth patch in place, writes a
`<file>.orbit-orig` backup next to the patched file, and prints what it did.

## Revert

```sh
node contrib/pi-auth-rpc/apply.mjs --revert
```

## What it adds

| Command | Behavior |
|---|---|
| `auth.list` | Advertises every provider from `ModelRuntime.getProviders()` with `credential`, `authenticated`, and login `methods` (`browser`, `device_code`, `api_key`). The UI renders entirely from this. |
| `auth.status` | Credential kind + configured state for one provider. |
| `auth.login` | Runs `ModelRuntime.login(provider, type, interaction)` in the background, mapping pi's `AuthInteraction` callbacks onto `auth_login_started` / `auth_login_url` / `auth_device_code` / `auth_login_waiting` / `auth_login_succeeded` / `auth_login_failed`. |
| `auth.logout` | `ModelRuntime.logout(provider)`. |
| `auth.cancel` | Aborts the session's `AbortController`, which unwinds the OAuth wait. |
| `auth.respond` | Answers a pending text/code prompt using `sessionId`, `requestId`, and string `value` (maximum 16384 characters). Unknown, stale, cross-flow and duplicate replies fail without echoing the value. |

OAuth method `select` prompts are answered from the requested method
(`browser`/`device_code`). `text` and `manual_code` prompts emit:

```json
{"type":"auth_login_prompt","sessionId":"flow-id","provider":"anthropic","requestId":"prompt-id","kind":"manual_code","message":"Paste code","placeholder":"code#state"}
```

The client answers explicitly (an empty string can accept a text default):

```json
{"id":"reply-id","type":"auth.respond","sessionId":"flow-id","requestId":"prompt-id","value":"user-supplied-code"}
```

`auth_login_prompt_closed` repeats the flow/provider/request IDs with
`reason: "answered" | "cancelled"`; it carries no answer. A prompt-specific
abort (browser callback wins) closes only that prompt, not the login flow.
Cancel/logout and the five-minute flow deadline clean up pending prompts;
only one login per provider may run at a time. `secret` (API-key) prompts
remain rejected — Orbit edits keys through its own provider editor. Failure
messages are deliberately generic so provider errors cannot reflect a pasted
code. Client event handling/UI must be wired separately before this is usable.

With the [auth-profile patch](../pi-auth-profiles/README.md), login/logout use
the selected provider's real writable credential store automatically.

## Tests

```sh
node --test contrib/pi-auth-rpc/*.test.mjs
```

Network-free tests cover prompt correlation, cancellation, callback races,
timeouts, reply validation, redaction, patch upgrades and both module layouts.

## Notes

- This edits a **global npm install**. Re-run `apply.mjs` after `pi update`;
  it detects the new bundle and patches it again.
- Tokens never cross the patch boundary: only the credential *kind*, device
  codes, and verification URLs are emitted. The full credential is written by
  pi's own `ModelRuntime` into `auth.json` (0600).
