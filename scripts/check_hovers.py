#!/usr/bin/env python3
"""Fail if the count of one-frame hover treatments changes without its budget.

The design system's motion contract says hover, press and selection fade on
`Motion::Interaction` (`motion::hover`). The `ControlExt` hover treatments in
`app/helpers.rs` (`hover_state`, `hover_quiet`, `hover_bordered`, …) swap their
fill on one frame instead. Many call sites still use them; converting each
takes care, because a faded hover needs a stable unique name and the exact
fill pair it replaces. So this is a ratchet rather than a ban:

* `scripts/hover_budget.json` records, per file, how many one-frame
  treatments it may use.
* A file **over** its budget added a snapping hover: route it through
  `motion::hover` instead.
* A file **under** its budget converted one: lower the budget so the freed
  slot cannot be quietly spent again. `--update` rewrites the file.

The treatment list is read from `app/helpers.rs` itself (every
`fn hover_*(self, …)`), so a treatment added there is counted from then on.
Test files are not counted, and neither is `app/helpers.rs`, where the
treatments are defined.

Run from the repo root:

    python3 scripts/check_hovers.py            # check (CI)
    python3 scripts/check_hovers.py --update   # rewrite the budget

Exit status 1 when any file's count differs from its budget.
"""
import json
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
HELPERS = "app/helpers.rs"
BUDGET = "scripts/hover_budget.json"

TREATMENT_DEF = re.compile(r"\bfn\s+(hover_[a-z_]+)\s*\(\s*(?:mut\s+)?self\b")


def treatments():
    """The one-frame hover treatments `app/helpers.rs` defines."""
    with open(os.path.join(ROOT, HELPERS), encoding="utf-8") as f:
        names = set(TREATMENT_DEF.findall(f.read()))
    if not names:
        sys.exit(f"hover check: no `fn hover_*(self, …)` found in {HELPERS}")
    return names


TEST_MOD = re.compile(r"^[ \t]*#\[cfg\(test\)\]\s*\n[ \t]*mod\s+\w+\s*\{", re.M)


def strip_test_modules(text):
    """Drop each inline `#[cfg(test)] mod … { … }` body, brace-matched, so code
    after a test module is still counted."""
    while True:
        m = TEST_MOD.search(text)
        if not m:
            return text
        depth, i = 1, m.end()
        while i < len(text) and depth:
            depth += {"{": 1, "}": -1}.get(text[i], 0)
            i += 1
        text = text[: m.start()] + text[i:]


def counts(names):
    """Per-file call counts of the treatments, outside tests and helpers."""
    call = re.compile(r"\.(" + "|".join(sorted(names)) + r")\s*\(")
    found = {}
    for base, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs") or name.endswith("_tests.rs"):
                continue
            path = os.path.join(base, name)
            rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
            if rel == HELPERS:
                continue
            with open(path, encoding="utf-8") as f:
                text = f.read()
            text = strip_test_modules(text)
            n = sum(
                len(call.findall(line))
                for line in text.splitlines()
                if not line.lstrip().startswith("//")
            )
            if n:
                found[rel] = n
    return dict(sorted(found.items()))


def main():
    current = counts(treatments())
    if "--update" in sys.argv[1:]:
        with open(BUDGET, "w", encoding="utf-8") as f:
            json.dump(current, f, indent=2, sort_keys=True)
            f.write("\n")
        print(f"hover check: budget rewritten, {sum(current.values())} one-frame hovers")
        return 0

    try:
        with open(BUDGET, encoding="utf-8") as f:
            budget = json.load(f)
    except FileNotFoundError:
        sys.exit(f"hover check: {BUDGET} is missing; run with --update")

    over, under = [], []
    for path in sorted(set(current) | set(budget)):
        have, allowed = current.get(path, 0), budget.get(path, 0)
        if have > allowed:
            over.append((path, have, allowed))
        elif have < allowed:
            under.append((path, have, allowed))

    for path, have, allowed in over:
        print(
            f"{ROOT}/{path}: {have} one-frame hovers, budget {allowed} — "
            "fade the new one with `motion::hover` instead of a `ControlExt` "
            "hover treatment"
        )
    for path, have, allowed in under:
        print(
            f"{ROOT}/{path}: {have} one-frame hovers, budget {allowed} — "
            "converted one; lower the budget with "
            "`python3 scripts/check_hovers.py --update`"
        )
    total = sum(current.values())
    if over or under:
        print(f"hover check: {total} one-frame hovers; budget out of date")
        return 1
    print(f"hover check: {total} one-frame hovers, every file within its budget")
    return 0


if __name__ == "__main__":
    sys.exit(main())
