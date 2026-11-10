# Permissions — per-command and per-agent rules

Orbit's access mode (`~/.orbit-pi/access.json`) is one global dial: it decides
whether *a whole class* of tool call asks first. That is not enough to express
"`bash` may run, but `rm` must ask", and it cannot say anything per subagent.

Settings → **Permissions** writes the missing layer to
`~/.orbit-pi/access-rules.json`. The bundled guard extension
(`contrib/orbit-guard-extension/`) reads that file **fresh on every tool call**,
so a saved rule applies to running sessions with no restart.

## The page

One row per kind of tool call, four states each:

```
Applies to   [Every session] [Main sessions only] [scout] [worker]

Commands   bash, powershell, shell, exec, terminal      → Ask
  [Inherit] [Allow] [Ask] [Deny]                    + Add pattern
  "git status*"                                          → Allow
    [Allow] [Ask] [Deny]                                   Remove
  "rm *"                                                 → Deny
    [Allow] [Ask] [Deny]                                   Remove
```

- **`Inherit`** stores nothing: the row resolves to the access mode's default,
  which the `→ …` label names. Choosing `Allow`/`Ask`/`Deny` stores an explicit
  rule, and choosing `Inherit` again deletes it, so the file only ever holds
  decisions a person made.
- **The `→ …` label** is what the guard will actually do: the most specific scope
  with a matching rule (page scope, then `any`), then the mode's default.
- **A pattern** narrows one kind to the calls it matches — commands for
  `Commands`, paths for file tools, URLs for fetches, the tool name for MCP. It
  carries its own three-state control.
- **The scope chips** switch the whole page between every session, main sessions
  only, and one subagent. There is no second editor and no separate per-agent
  page.
- **Under Full access** the board is replaced by one statement: that mode runs
  every tool call without a prompt, so there is nothing to configure. Stored
  rules are kept and reappear when the mode is confined again.

This page is a *guard*, not a sandbox — the same disclaimer as the access modes.
It changes when Orbit asks; it cannot stop a process that has already been
allowed to run from doing something else.

## A rule

```json
{ "scope": "any", "target": "exec", "pattern": "rm *", "action": "deny" }
```

A rule is written by the page, but the file is plain JSON:

| Field | Values |
| --- | --- |
| `scope` | `any` (every session) · `session` (main sessions only) · `agent:<name>` (one subagent) |
| `target` | `read` · `edit` · `exec` · `net` · `mcp` · `delegate` · `other` · `any` (`any` is for hand-written rules; the page lists the families) |
| `pattern` | A glob with one metacharacter: `*` matches any run of characters. Empty matches every call of the family. |
| `action` | `allow` (run it) · `ask` (confirm it) · `deny` (refuse it) |

The families are the guard's own classification, so a rule can never name a
category the decision table does not understand. `net` (`webfetch`,
`websearch`) is its own family precisely so a fetch can be gated — the mode
defaults still allow it, unchanged.

### What the pattern is matched against

The text the approval bar would show, so "what I read is what the rule matches":

- `exec` — the command;
- `read`, `edit` — the path (falling back to a search pattern);
- `net` — the URL or query;
- `mcp`, `other` — the tool name and its arguments;
- `delegate` — the task text.

Three consequences worth knowing:

1. **Anchored.** `git status` does not match `git status --short`; write
   `git status*`. `*` spans `/`, so `*.log` works for a path.
2. **Literal otherwise.** `.`, `(`, `[` and friends are literal characters, not
   regex syntax.
3. **Different binary, different text.** `/bin/rm -rf x` is not `rm -rf x`.

## Precedence

0. **Full access answers first.** The mode runs before the whole rule table:
   nothing asks and nothing is refused, whatever is stored. Rules exist for the
   confined modes, and the page says so instead of showing rows that cannot take
   effect. Switch to Supervised or Auto-accept edits to use them.
1. The subagent **ceiling** — derived from the agent's `tools:` line — is
   enforced first. No rule can widen it.
2. Then scopes, **most specific first**: `agent:<name>` (only in that child),
   then `session` (main sessions only), then `any`. The first scope with a
   matching rule decides.
3. Inside that scope: `deny` beats `ask` beats `allow`.
4. No matching rule ⇒ the access mode's default for that family.

A rule therefore both narrows and widens: a subagent-scoped `allow` can loosen
a broader `ask`, and an `agent:worker` `deny` cannot be overridden by anything
broader — inside a confined mode. Under Full access no rule is consulted at all.

## Two safety behaviours

- **An `allow` rule never covers a compound command.** `ls` may be allowed;
  `ls; rm -rf build` is not, because the pattern described the whole line and
  only the first command is checked by eye (`[&|;<>`\n]` or `$(` anywhere in
  the command is compound). `ask` and `deny` rules *do* apply per segment, so
  `cd x && rm -rf y` still hits `rm *`.
- **A child has no approval window.** Children run `pi --mode json -p`, so a
  call an `ask` rule covers is refused there, not prompted. The guard reports it
  to the model like any other block. That is why the Subagents section of the
  page exists: allow what an agent needs to finish, deny what it must not do.

## Storage and failure behaviour

`~/.orbit-pi/access-rules.json` is written only by the app
(`crate::permissions::Rules::save`: temp file + rename, owner-only `0600`) and
read only by the extension. The extension's own "Always allow this tool" grants
stay in `access-allow.json`, written only by the extension, so there is never a
two-writer race on one file.

The file is read leniently in the safe direction:

- a missing, unreadable or malformed file ⇒ **no rules**, i.e. the mode default;
- a rule with an unknown `scope`, `target` or `action` is **dropped**, never
  half-applied — a dropped `deny` falls back to asking, never to allowing;
- at most 200 rules are stored, and a pattern is truncated at 200 characters.

## Where the code lives

| Path | Role |
| --- | --- |
| `crates/orbit-pi/src/permissions.rs` | The rules model, the file, the mode defaults table, validation |
| `crates/orbit-pi/src/app/permissions_ui.rs` | Settings → Permissions: baseline select, scope chips, one row per family, pattern rules + inline form |
| `contrib/orbit-guard-extension/policy.js` | The enforcing copy: `classify`, `defaultAction`, `ruleDecision`, `decide` |
| `contrib/orbit-guard-extension/index.js` | Reads the rules and the agent name, blocks on `deny`, prompts on `ask` |

`permissions.rs::Target::default_action` and `policy.js::defaultAction` are the
same table written twice — the app cannot import the extension. Both have tests
asserting their own copy; change one, change both.
