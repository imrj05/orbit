#!/usr/bin/env python3
"""Fail when a key binding is scoped to a key context no element declares.

A binding with `context: Some("Sidebar")` only fires while the focused element
sits under a `key_context("Sidebar")`. If that context string is renamed or
typo'd on either side, the shortcut silently stops working — the app still builds,
the registry still advertises the chord, and nothing points at the cause.

This is the keyboard sweep's mechanical half. The other half is in
`commands::tests`: every chord must parse, and no two bindings may share a chord
in one context (which shadows one of them).

Run from the repo root:

    python3 scripts/check_keyboard.py

Exit status 1 on an orphan context, so CI can gate on it.
"""
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
COMMANDS = os.path.join(ROOT, "commands.rs")

DECLARED = re.compile(r'key_context\(\s*"([^"]+)"\s*\)')
USED = re.compile(r'context:\s*Some\(\s*"([^"]+)"\s*\)')
# Bindings whose context is built from a constant rather than a literal.
IGNORED_CONTEXTS = {"Composer"}


def declared_contexts() -> set[str]:
    found: set[str] = set()
    for base, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs"):
                continue
            with open(os.path.join(base, name), encoding="utf-8") as handle:
                found.update(DECLARED.findall(handle.read()))
    return found


def binding_contexts() -> list[tuple[str, str]]:
    with open(COMMANDS, encoding="utf-8") as handle:
        source = handle.read()
    # Keep the file order so the report names a line the reader can find.
    out = []
    for match in USED.finditer(source):
        line = source.count("\n", 0, match.start()) + 1
        out.append((match.group(1), f"commands.rs:{line}"))
    return out


def main() -> int:
    declared = declared_contexts()
    orphans = [
        (context, where)
        for context, where in binding_contexts()
        if context not in declared and context not in IGNORED_CONTEXTS
    ]
    if orphans:
        print("A key binding is scoped to a context no element declares, so it")
        print("can never fire. Declare the context or fix the spelling.\n")
        for context, where in orphans:
            print(f"  {where}: {context}")
        print(f"\nDeclared contexts: {', '.join(sorted(declared))}")
        return 1

    print(f"keyboard check: {len(declared)} contexts declared, every binding lands in one")
    return 0


if __name__ == "__main__":
    sys.exit(main())
