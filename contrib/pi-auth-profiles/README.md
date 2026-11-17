# Local pi auth-profile routing

Reproducible, version/anchor-guarded patch for **pi 1.5.2**. It patches both
`dist/core/auth-storage.js` and the credential-storage chunk under
`dist/bundle/chunks/`; no pi extension can replace this process-wide storage
boundary. Nothing changes when `ORBIT_AUTH_PROFILE_ROUTES` is absent.

```sh
PI_PACKAGE_ROOT=/absolute/pi/package node contrib/pi-auth-profiles/apply.mjs
PI_PACKAGE_ROOT=/absolute/pi/package node contrib/pi-auth-profiles/apply.mjs --check
# --probe aliases --check; --revert removes only our marker block.
```

`PI_BIN`/PATH and the managed installer can also locate the package. Apply is
idempotent, refreshes old marker-delimited payloads, and keeps a dedicated
`.orbit-auth-profiles-orig` backup. Revert preserves unrelated patches. Unknown
versions/layouts and incomplete markers fail, rather than guessing anchors.
`--check` never writes the installation: it verifies exact patch bytes and
executes synthetic credential operations in temporary files against **both**
modules. When routes are supplied it also validates their paths by importing
the modules, without reading their credential contents.

## Process contract

Set this environment variable **only on the selected local child process**:

```json
{"anthropic":"/canonical/profile-directory/auth.json"}
```

- Provider IDs remain real pi IDs. Unselected providers keep normal behavior.
- Paths must be absolute, normalized, canonical (no symlink aliases), writable,
  and named `auth.json`. The parent directory must already exist; pi can create
  the file with its normal 0600 policy. Invalid routes abort module loading;
  diagnostics do not include the supplied JSON or file contents.
- `AuthStorage` and `ReadOnlyAuthStorage` read/modify/delete/list use a separate
  **writable pi AuthStorage** per selected file. OAuth refresh remains pi's own
  locked read/check/refresh/write operation. Two sessions selecting the same
  canonical file share pi's cross-process lock; credentials are never copied.
- An absent selected provider throws `auth_required` on read, even if the
  default store, environment or models config could supply another account.
  Metadata listing omits an empty selected provider. Storage failures cannot
  return pi's last valid snapshot. Only selected provider metadata is exposed
  from a profile containing other entries.
- This is credential-store routing, not a sandbox. Explicit runtime/request
  API-key overrides bypass pi's store and must not be supplied for a selected
  provider. The synchronous SDK helper `readStoredCredential` is not used by
  ModelRuntime and is not routed.
- RPC, `pi -p` one-shots and delegated child pi processes use the same patched
  storage when they inherit this environment. Never change
  `PI_CODING_AGENT_DIR`, copy OAuth data, or invent provider aliases.

## Required spawn gate

Before each launch with routes, call
`rpc_patches::ensure_auth_profile_routing(routes, remote)` off the UI thread.
It materializes these sources, applies and behaviorally probes the live
installation without relying on the optional RPC patch cache. Disabled patches,
missing Node, drift, a failed probe or a remote target are **errors that must
reject spawning**, not reasons to launch with a default account. Forward the
same routes to the actual child and its descendants. A pi update requires this
gate again. The gate is provided here; caller wiring lives outside this patch.
Remote/SSH profile selection is intentionally unavailable.

## Verification

```sh
node --test contrib/pi-auth-profiles/*.test.mjs
```

Fixtures cover routing, defaults, empty/corrupt profiles, metadata isolation,
path rejection and patch lifecycle. If pi 1.5.2 is installed at its managed
location (or `PI_AUTH_PROFILE_TEST_ROOT` points to it), tests copy its bundle
and source into a temporary sandbox and exercise real storage and OAuth
refresh locking across independent processes. They never modify installed pi,
read real auth.json, or call a provider/network endpoint.
