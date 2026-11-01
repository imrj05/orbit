#!/usr/bin/env python3
"""Fail if an interaction state names a colour at the call site.

Phase 2 of the UI/UX Modernization epic ends with one rule: a component does not
decide what hover, press, focus or disabled look like — it inherits the state
from `app/helpers.rs` (`ControlExt` / `StatefulControlExt`), which resolves every
colour from `InteractionState` in `theme/tokens.rs`.

This check enforces that rule mechanically, so a component added later inherits
correct states by default instead of quietly re-deciding them.

A `.hover(…)` or `.active(…)` closure anywhere in the crate must not name one of
the interaction tokens. The contract itself (`app/helpers.rs`) is the one file
allowed to, because that is where the tokens are resolved.

Run from the repo root:

    python3 scripts/check_states.py

Exit status 1 when a call site names a state colour, so CI can gate on it.
"""
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
# The contract resolves the tokens; every other file must consume treatments.
ALLOWED = {"app/helpers.rs"}

STATE_TOKENS = [
    "surface_hover",
    "surface_active",
    "surface_selected",
    "overlay_strong",
    "overlay",
    "action_hover",
    "action_pressed",
    "accent_hover",
    "accent_pressed",
    "accent_pressed",
    "error_hover",
    "focus_ring",
    "border_strong",
    "text_primary",
    "text_secondary",
    "text_muted",
    "text_disabled",
    "selection_fg",
]

STATE_KEYS = re.compile(r"\.(hover|active)\(\s*\|")

TOKEN = re.compile(r"theme\.(" + "|".join(STATE_TOKENS) + r")\b")


def body_from(lines, start):
    """Yield the closure body starting at `start`, until its braces balance."""
    depth = 0
    seen_brace = False
    out = []
    for line in lines[start:]:
        out.append(line)
        depth += line.count("{") - line.count("}")
        if "{" in line:
            seen_brace = True
        if seen_brace and depth <= 0:
            break
        if not seen_brace and line.rstrip().endswith(")"):
            break
        if len(out) > 12:
            break
    return out


def main():
    violations = []
    for base, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(base, name)
            rel = os.path.relpath(path, ROOT)
            if rel in ALLOWED:
                continue
            with open(path, encoding="utf-8") as handle:
                lines = handle.readlines()
            for ix, line in enumerate(lines):
                if not STATE_KEYS.search(line):
                    continue
                for body_line in body_from(lines, ix):
                    match = TOKEN.search(body_line)
                    if match:
                        violations.append((rel, ix + 1, match.group(0), body_line.strip()))
                        break

    if violations:
        print("Interaction states must come from the contract, not from a call site.")
        print("Use hover_state / hover_quiet / hover_bordered / hover_action /")
        print("hover_danger / hover_overlay / hover_ink / hover_on_glass / hover_border")
        print("and pressed_state(ramp) / disabled_state.\n")
        for rel, line, token, source in violations:
            print(f"  {rel}:{line}: {token}")
            print(f"      {source}")
        return 1

    print("interaction-state check: 0 call sites name a state colour")
    return 0


if __name__ == "__main__":
    sys.exit(main())
