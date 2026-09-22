#!/usr/bin/env python3
"""Fail if a `tr!("key")` used in Orbit has no entry in `locales/en.yml`.

Catches the class of bug where a literal is wrapped but its key is never
registered — rust-i18n then renders the raw key (or the English fallback
without a translation). Run from the repo root:

    python3 scripts/check_i18n.py

Exit status is 1 when keys are missing, so CI can gate on it.
"""
import io
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
EN = "crates/orbit-pi/locales/en.yml"
# `tr!` but not `include_str!`, `write_str!`, etc.: the leading character must
# not be an identifier byte.
KEY = re.compile(r'(?<![A-Za-z0-9_])tr!\(\s*"((?:[^"\\]|\\.)*)"')


def main():
    used = {}
    for dirpath, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            for line_no, line in enumerate(io.open(path, encoding="utf-8"), 1):
                for match in KEY.finditer(line):
                    used.setdefault(match.group(1), []).append(f"{path}:{line_no}")

    defined = set()
    for line in io.open(EN, encoding="utf-8"):
        line = line.rstrip("\n")
        if line and not line.startswith("_version"):
            defined.add(line.partition(": ")[0])

    missing = sorted(key for key in used if key not in defined)
    unused = sorted(key for key in defined if key not in used)
    for key in missing:
        print(f"MISSING en.yml key: {key}  (used at {used[key][0]})")
    if unused:
        print(f"note: {len(unused)} en.yml key(s) not referenced in source (ok if built dynamically)")

    print(f"{len(used)} keys used, {len(defined)} defined, {len(missing)} missing")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
