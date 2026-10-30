# pi `quota.*` RPC patch

Orbit shows account-level provider quota, balance, and spend on the Providers
page. pi's RPC server has no such command — it only exposes session-local
`get_session_stats` — so this patch adds one command, `quota.list`, to the
installed pi build.

The handler resolves each connected provider's credential through pi's own
`session.modelRuntime` (Orbit never receives a token), queries that provider's
usage endpoint, and returns a normalized, non-secret report. The client
contract is
[`../../crates/orbit-rpc/docs/quota-rpc.md`](../../crates/orbit-rpc/docs/quota-rpc.md).

## Apply

```sh
node contrib/pi-quota-rpc/apply.mjs
```

Then restart the agent: **Settings → Runtime → Restart**, or reopen Orbit.

The script resolves `pi` on `PATH` (override with `PI_BIN`), finds the bundled
RPC mode, and injects the handler from `quota-handler.js`. It is idempotent,
shares the `<file>.orbit-orig` backup with the auth patch, and prints what it
did.

## Revert

```sh
node contrib/pi-quota-rpc/apply.mjs --revert
```

Revert restores the original pre-Orbit bundle, which also removes the
`auth.*` patch; re-run `contrib/pi-auth-rpc/apply.mjs` afterwards if you still
want in-app sign-in.

## Provider coverage

Adapters are modeled on [`@narumitw/pi-usage`](https://www.npmjs.com/package/@narumitw/pi-usage)'s
verified provider reference. Providers without a usage surface report
`unsupported`; nothing is fabricated.

| Provider id | Kind | Source |
|---|---|---|
| `anthropic` (OAuth) | subscription | `/api/oauth/usage` |
| `openai-codex` | subscription | `/backend-api/wham/usage` |
| `github-copilot` | subscription | `/copilot_internal/user` |
| `kimi-coding` | subscription | `/coding/v1/usages` |
| `minimax`, `minimax-cn` | subscription / balance | `/v1/token_plan/remains`, `/account/query_balance` |
| `zai`, `zai-coding-cn` | subscription | `/api/monitor/usage/quota/limit` |
| `opencode`, `opencode-go` | subscription | `/zen[/go]/v1/usage` |
| `openrouter` | credits | `/api/v1/key` |
| `vercel-ai-gateway` | credits | `/v1/credits` |
| `deepseek` | balance | `/user/balance` |
| `moonshotai`, `moonshotai-cn` | balance | `/v1/users/me/balance` |
| `xai` (OAuth) | credits | consumer billing proxy |
| `fireworks` | spend | `/v1/accounts/{id}/billing/summary` |
| `baseten` | spend | `/v1/billing/usage_summary` |
| `google`, `google-vertex` | unsupported | no account quota API (AI Studio only) |
| `ollama` | subscription | `GET https://ollama.com/api/balance` (session/weekly meters + reset times, purchased balance) + `GET https://ollama.com/api/usage` (legacy `limits` windows, request counts), or the authenticated settings page (legacy reset times) |

`google`/`google-vertex` are registered explicitly so a connected account gets a
provider-specific explanation rather than the generic fallback. Gemini rate
limits are per Google Cloud project and visible only in AI Studio, so there is
no account-quota endpoint to call.

### Ollama Cloud

Ollama Cloud accounts are on one of two billing generations, and the adapter
handles both:

- **Current (live, Oct 2026)** — `GET https://ollama.com/api/balance` with a
  real cloud API key carries the meters: `included.session` and
  `included.weekly` are `{remaining_percent, resets_at}` buckets, and
  `purchased.balance_usd` is the remaining purchased credit. The percentage is
  *remaining*, so the adapter reports `100 - remaining_percent` as used.
  `GET https://ollama.com/api/usage` now returns request-count history
  (`range`, `totals`, daily `buckets`) with no `limits` object, so it is read
  for the legacy shape only.
- **Legacy (usage credits)** — `GET https://ollama.com/api/usage` returns a
  0..1 `limits.monthly.usage` fraction (or `limits.session`/`limits.weekly`).
  The monthly reset is on the plan's subscription anniversary, which the
  payload does not expose, so that one countdown is omitted rather than
  guessed.
- **Legacy (GPU time)** — 5-hour session + weekly windows, rendered server-side
  on `https://ollama.com/settings`. These have no API, so the adapter can fetch
  that page with a session cookie and parse it via `OllamaCloudParser`. Orbit
  no longer collects that cookie in the app: the entry is only read when it is
  already in `auth.json`.

Both API payloads are merged by window id, and the `/api/balance` copy wins
because it carries the reset time. The balance request is additive — if it
fails the meters from `/api/usage` are returned unchanged and no error is
attached. A `note` is attached only when something is actually missing (the
monthly anniversary reset, a window without a countdown, or no data at all).

Credentials are **explicit and user-supplied**, stored in `auth.json` (0600).
A session entry lives under its own key so it never shadows the `ollama`
provider credential:

```json
"ollama":               {"type":"api_key","key":"<real ollama.com key>"}             // current model
"ollama-cloud-session": {"type":"ollama_cloud_session","session":"__Secure-session=…"} // legacy page (read-only; hand-written)
```

The adapter never reads browser cookies, never sends the local `models.json`
placeholder (`api_key: "ollama"` pointing at `127.0.0.1`) to ollama.com, and
never logs a session. The API key is set in **Settings → Providers → Ollama →
`Add API key`**; there is no session-cookie input in the app.

A real key under either id meters the same account: each adapter prefers its own
entry and falls back to the sibling's (`ollama` ↔ `ollama-cloud`), and the
bridge keeps the sibling in the snapshot when only one has a key. Reports are
keyed by provider id, so the usage popover will list both ids with identical
meters — one report per id, not one per account.

## Notes

- Handler adapters have network-free unit tests:
  `node --test contrib/pi-quota-rpc/quota-handler.test.mjs`.
- This edits a **global npm install**. Re-run `apply.mjs` after `pi update`.
- Requests use a 12 s timeout, reject redirects, and cache per provider for
  60 s (these endpoints rate-limit aggressively).
- Every request goes through one helper, `orbitQuotaRequest`, which carries a
  `// lgtm[js/file-access-to-http]` suppression. CodeQL flags "file data in an
  outbound request" for any code that sends a stored credential, and its rule
  offers no sanitizer for that deliberate pattern; the suppression is the
  documented way to accept CWE-200 after review. Keep new requests on that one
  helper, and keep them carrying nothing but a provider's own credential to
  that provider's own origin.
- Errors are sanitized: credential-shaped substrings are redacted before they
  can reach Orbit.
