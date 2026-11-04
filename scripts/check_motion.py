#!/usr/bin/env python3
"""Fail if an animation picks its own duration, or bypasses the motion roles.

`DESIGN.md` §Motion System ends with one rule: a surface picks a *motion
primitive*, never a number. There are three:

* `Motion` (`theme/tokens.rs`) — one-shot transitions: Instant / Interaction /
  Layout / Travel, each with its easing and its Reduce Motion guard built in.
* `looping` (`theme/tokens.rs`) — the cadences for loops that report live state
  (spinner, shimmer, pulse, indeterminate progress). These are guarded by Reduce
  Motion at the call site, where the glyph stays and the motion drops.
* `chart` (`theme/tokens.rs`) — the data-visualisation cadence, deliberately
  longer than any transition.

Four checks, each a real defect rather than a style preference:

1. `Animation::new(Duration::from_millis(…))` — a typed millisecond duration.
2. `Animation::new(some_binding)` — a duration laundered through a variable, so
   nobody can see which role it is.
3. `Animation::new(Duration::from_secs_f32(…))` — allowed only in
   `app/toast_ui.rs`, where the toast's life bar runs the toast's own TTL. That is
   a countdown over data, not a motion choice, and it is the sole exception.
4. `.nominal()` outside `theme/tokens.rs` — the role's duration *before* the
   Reduce Motion guard. A surface that calls it animates for a user who asked for
   reduced motion.

Run from the repo root:

    python3 scripts/check_motion.py

Exit status 1 on a violation, so CI can gate on it.
"""
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
# Where the roles and their `nominal()` durations are defined.
ROLE_DEFINITIONS = {"theme/tokens.rs"}
# Countdowns over data, by verdict: the toast's life bar runs its own TTL.
DATA_DURATION_FILES = {"app/toast_ui.rs"}

ANIMATION_CALL = re.compile(r"Animation::new\(\s*(.*)")
# A bare binding as the whole argument, wherever it sits in the line:
# `Animation::new(copy_duration).with_easing(…)`. Paths (`looping::SPIN`) and
# method calls (`Motion::Layout.duration(&theme)`) contain punctuation, so a
# lone identifier is the only thing this matches.
BARE_BINDING = re.compile(r"Animation::new\(\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)")
# The same argument written across lines, where the line-based match cannot see
# the closing paren.
BARE_ARGUMENT = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\s*[),\]]*\s*$")
RAW_MILLIS = re.compile(r"Duration::from_millis")
SECONDS = re.compile(r"Duration::from_secs")
NOMINAL = re.compile(r"\.nominal\(\)")


def argument(lines, index):
    """The first argument of the call that starts on `lines[index]`.

    The argument may continue on the next non-empty line, which is how the
    multi-line role calls are written.
    """
    rest = ANIMATION_CALL.search(lines[index]).group(1).strip()
    if rest and rest not in {")", "( "}:
        return rest
    for line in lines[index + 1 : index + 4]:
        stripped = line.strip()
        if stripped and stripped != ")":
            return stripped
    return ""


def main():
    violations = []
    for base, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(base, name)
            rel = os.path.relpath(path, ROOT)
            with open(path, encoding="utf-8") as handle:
                lines = handle.readlines()

            for ix, line in enumerate(lines):
                if "Animation::new(" in line:
                    arg = argument(lines, ix)
                    binding = BARE_BINDING.search(line)
                    if binding:
                        violations.append(
                            (
                                rel,
                                ix + 1,
                                f"a duration hidden in `{binding.group(1)}`",
                                line.strip(),
                            )
                        )
                    elif RAW_MILLIS.search(arg):
                        violations.append(
                            (rel, ix + 1, "a typed millisecond duration", arg)
                        )
                    elif SECONDS.search(arg):
                        if rel not in DATA_DURATION_FILES:
                            violations.append(
                                (rel, ix + 1, "a hand-timed duration", arg)
                            )
                    else:
                        hidden = BARE_ARGUMENT.match(arg)
                        if hidden:
                            violations.append(
                                (
                                    rel,
                                    ix + 1,
                                    f"a duration hidden in `{hidden.group(1)}`",
                                    arg,
                                )
                            )

                if rel not in ROLE_DEFINITIONS and NOMINAL.search(line):
                    violations.append(
                        (rel, ix + 1, "`.nominal()` skips the Reduce Motion guard", line.strip())
                    )

    if violations:
        print("Animations must use a motion primitive, never a number.")
        print("Use Motion::<role>.duration(&theme) for a transition,")
        print("looping::<cadence> for a state loop, chart::<cadence> for a chart.\n")
        for rel, line, problem, source in violations:
            print(f"  {rel}:{line}")
            print(f"      {problem}")
            print(f"      {source}")
        return 1

    print("motion check: 0 animations type their own duration")
    print("motion check: 0 call sites resolve a role's `nominal()` duration")
    return 0


if __name__ == "__main__":
    sys.exit(main())
