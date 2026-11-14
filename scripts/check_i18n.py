#!/usr/bin/env python3
"""Fail if a `tr!(...)` in Orbit is misused.

Three checks:

1. Every `tr!("key")` used in the source has an entry in `locales/en.yml`.
   rust-i18n would otherwise render the raw key.
2. Every `tr!("key", name = ...)` binds exactly the placeholders the key
   declares (`%{name}`). An unbound placeholder stays literal in the output,
   e.g. `Show all %{count} lines` when the call binds `total` for a `%{count}`
   key (`total` vs `count`), or `Branch create failed: %{err}` when it binds
   `error` for `%{err}`.
3. No locale gains an untranslated string (#131). A value that still reads
   exactly like its English is "untranslated" unless it is listed in
   `scripts/i18n_untranslated_baseline.json` — the existing debt, burned down
   per release instead of blocking PRs. A new feature ships its translations
   (a glossary entry, then `gen_locales.py`) or adds its keys to the baseline
   on purpose. Translating a baselined string leaves a stale entry; `--update`
   rewrites the baseline to what is untranslated now.

Run from the repo root:

    python3 scripts/check_i18n.py            # check (CI)
    python3 scripts/check_i18n.py --update   # rewrite the untranslated baseline

Exit status is 1 when a key is missing, a placeholder does not match, or a
locale has an untranslated string outside the baseline, so CI can gate on it.
"""
import io
import json
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
EN = "crates/orbit-pi/locales/en.yml"
LOCALES = "crates/orbit-pi/locales"
BASELINE = "scripts/i18n_untranslated_baseline.json"
# A string counts as prose (and so as translatable) when, placeholders aside,
# it holds a run of at least two letters. Symbols, numbers and bare
# placeholders read the same in every language.
PROSE = re.compile(r"[A-Za-z]{2,}")
# `tr!` but not `include_str!`, `write_str!`, etc.: the leading character must
# not be an identifier byte.
KEY = re.compile(r'(?<![A-Za-z0-9_])tr!\(\s*"((?:[^"\\]|\\.)*)"')
# `tr!` / `tr_cow!` with a literal key, matched against the whole file so
# multi-line calls are seen.
CALL = re.compile(r'(?<![A-Za-z0-9_])tr(?:_cow)?!\(\s*"((?:[^"\\]|\\.)*)"')
PLACEHOLDER = re.compile(r"%\{(\w+)\}")
# A binding is `name =` not followed by `>` (so `=>` match arms don't count).
BINDING = re.compile(r"([a-z_][a-z0-9_]*)\s*=(?!>)")


def declared_placeholders():
    """key -> the set of `%{name}` placeholders its en.yml value declares."""
    result = {}
    for line in io.open(EN, encoding="utf-8"):
        match = re.match(r'^([\w.]+):\s*"(.*)"\s*$', line.rstrip("\n"))
        if match:
            result[match.group(1)] = set(PLACEHOLDER.findall(match.group(2)))
    return result


def bound_names(src, call_start):
    """The `name =` bindings of one call, at the call's own paren depth.

    Nested calls (a `tr!` inside another `tr!`'s argument) are skipped, so
    only the bindings that belong to this key are returned.
    """
    start = src.index("(", call_start)
    depth = 0
    end = start
    while end < len(src):
        char = src[end]
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
            if depth == 0:
                break
        end += 1
    body = src[start + 1 : end]
    names = set()
    depth = 0
    i = 0
    while i < len(body):
        char = body[i]
        if char in "([{":
            depth += 1
        elif char in ")]}":
            depth -= 1
        elif depth == 0:
            match = BINDING.match(body, i)
            if match:
                names.add(match.group(1))
                i = match.end()
                continue
        i += 1
    return names


def parse_values(path):
    """key -> raw value for the flat `key: "value"` lines a locale file holds."""
    values = {}
    for line in io.open(path, encoding="utf-8"):
        match = re.match(r'^([\w.\-]+):\s*"(.*)"\s*$', line.rstrip("\n"))
        if match:
            values[match.group(1)] = match.group(2)
    return values


def untranslated():
    """locale -> sorted keys whose value is still the English prose."""
    english = parse_values(EN)
    report = {}
    for name in sorted(os.listdir(LOCALES)):
        if not name.endswith(".yml") or name == "en.yml":
            continue
        values = parse_values(os.path.join(LOCALES, name))
        report[name[: -len(".yml")]] = sorted(
            key
            for key, value in values.items()
            if english.get(key) == value
            and PROSE.search(PLACEHOLDER.sub("", value))
        )
    return report


def check_untranslated(update):
    """The untranslated pass; returns the number of new untranslated keys."""
    report = untranslated()
    if update:
        with io.open(BASELINE, "w", encoding="utf-8") as handle:
            json.dump(report, handle, indent=1, sort_keys=True)
            handle.write("\n")
        total = sum(len(keys) for keys in report.values())
        print(f"untranslated baseline rewritten: {total} key(s) across {len(report)} locale(s)")
        return 0
    try:
        baseline = json.load(io.open(BASELINE, encoding="utf-8"))
    except (OSError, ValueError):
        baseline = {}
    new = 0
    stale = 0
    for locale, keys in report.items():
        known = set(baseline.get(locale, []))
        fresh = [key for key in keys if key not in known]
        stale += len(known - set(keys))
        new += len(fresh)
        for key in fresh:
            print(f"UNTRANSLATED {locale}: {key}  (add a glossary entry and run gen_locales.py)")
    summary = ", ".join(f"{locale} {len(keys)}" for locale, keys in report.items())
    print(f"untranslated: {summary}")
    if stale:
        print(
            f"note: {stale} baselined key(s) are translated now — "
            "run `python3 scripts/check_i18n.py --update` to shrink the baseline"
        )
    return new


def main():
    update = "--update" in sys.argv[1:]
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

    # Placeholder pass: a call's bindings must match its key's placeholders.
    declared = declared_placeholders()
    mismatches = []
    for dirpath, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(dirpath, name)
            src = io.open(path, encoding="utf-8").read()
            for match in CALL.finditer(src):
                key = match.group(1)
                wants = declared.get(key)
                if not wants:
                    continue
                got = bound_names(src, match.start())
                if got != wants:
                    line_no = src.count("\n", 0, match.start()) + 1
                    mismatches.append((path, line_no, key, got, wants))
    for path, line_no, key, got, wants in mismatches:
        print(
            f"PLACEHOLDER MISMATCH {path}:{line_no}  {key}: "
            f"binds {sorted(got) or 'nothing'}, key declares {sorted(wants)}"
        )

    fresh_untranslated = check_untranslated(update)

    print(
        f"{len(used)} keys used, {len(defined)} defined, {len(missing)} missing, "
        f"{len(mismatches)} placeholder mismatch(es), "
        f"{fresh_untranslated} new untranslated string(s)"
    )
    return 1 if missing or mismatches or fresh_untranslated else 0


if __name__ == "__main__":
    sys.exit(main())
