import assert from "node:assert/strict";
import { test } from "node:test";

import { aliasProvider, parseAccounts } from "./aliases.js";

test("parseAccounts keeps well-formed aliases only", () => {
  const text = JSON.stringify({
    version: 2,
    profiles: {
      "account-a": { provider: "openai-codex", label: " Work ", alias: "openai-codex~work" },
      "account-b": { provider: "openai-codex", label: "Dup", alias: "openai-codex~work" },
      "account-c": { provider: "anthropic", label: "Wrong", alias: "openai-codex~x" },
      "account-d": { provider: "anthropic", label: "Bad", alias: "anthropic~../x" },
      "account-e": { provider: "anthropic", label: "   ", alias: "anthropic~blank" },
      "account-f": { provider: "anthropic", label: "Old" },
    },
  });
  assert.deepEqual(parseAccounts(text), [
    { provider: "openai-codex", label: "Work", alias: "openai-codex~work" },
  ]);
  assert.deepEqual(parseAccounts("not json"), []);
  assert.deepEqual(parseAccounts("{}"), []);
});

test("aliasProvider renames models and streams them as the base", () => {
  const seen = [];
  const base = {
    id: "p",
    name: "Provider",
    auth: { oauth: {} },
    getModels: () => [{ id: "m", provider: "p" }],
    stream: (model) => seen.push(["stream", model.provider]),
    streamSimple: (model) => seen.push(["simple", model.provider]),
    filterModels: (models) => models.filter((m) => m.provider === "p"),
  };
  const alias = aliasProvider(base, { provider: "p", label: "Work", alias: "p~work" });
  assert.equal(alias.id, "p~work");
  assert.equal(alias.name, "Provider · Work");
  assert.equal(alias.auth, base.auth);
  assert.deepEqual(alias.getModels(), [{ id: "m", provider: "p~work" }]);
  assert.deepEqual(alias.filterModels(alias.getModels()), [{ id: "m", provider: "p~work" }]);
  alias.stream({ id: "m", provider: "p~work" });
  alias.streamSimple({ id: "m", provider: "p~work" });
  assert.deepEqual(seen, [["stream", "p"], ["simple", "p"]]);
  assert.equal(alias.refreshModels, undefined);
  assert.equal(alias.getAllModels, undefined);
});
