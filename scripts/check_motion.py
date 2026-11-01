#!/usr/bin/env python3
"""Fail if an animation types its own duration.

Phase 3 of the UI/UX Modernization epic ends with one rule: a surface picks a
*motion primitive*, never a number. There are three:

* `Motion` (`theme/tokens.rs`) — one-shot transitions: Instant / Interaction /
  Layout, each with its easing and its Reduce Motion guard built in.
* `looping` (`theme/tokens.rs`) — the cadences for loops that report live state
  (spinner, shimmer, pulse, indeterminate progress). These are guarded by Reduce
  Motion at the call site, where the glyph stays and the motion drops.
* `chart` (`theme/tokens.rs`) — the data-visualisation cadence, deliberately
  longer than any transition.

A duration derived from *data* is the one exception: the toast's life bar runs
the toast's own TTL, so it is a countdown rather than a motion choice. Those are
written as `Duration::from_secs_f32(...)` and are the only raw durations allowed.

Run from the repo root:

    python3 scripts/check_motion.py

Exit status 1 on a raw millisecond duration in an animation, so CI can gate on it.
"""
import os
import re
import sys

ROOT = "crates/orbit-pi/src"
# Where the primitives are defined.
ALLOWED_FILES = {"theme/tokens.rs"}

ANIMATION = re.compile(r"Animation::new\(\s*([^)]*)\)")
RAW_MILLIS = re.compile(r"Duration::from_millis")


def main():
    violations = []
    for base, _, files in os.walk(ROOT):
        for name in files:
            if not name.endswith(".rs"):
                continue
            path = os.path.join(base, name)
            rel = os.path.relpath(path, ROOT)
            if rel in ALLOWED_FILES:
                continue
            with open(path, encoding="utf-8") as handle:
                for ix, line in enumerate(handle, start=1):
                    if "Animation::new(" not in line:
                        continue
                    # The argument may continue on the next line.
                    if RAW_MILLIS.search(line):
                        violations.append((rel, ix, line.strip()))
    if violations:
        print("Animations must use a motion primitive, not a typed duration.")
        print("Use Motion::<role>.duration(&theme) for a transition,")
        print("looping::<cadence> for a state loop, chart::<cadence> for a chart.\n")
        for rel, line, source in violations:
            print(f"  {rel}:{line}")
            print(f"      {source}")
        return 1

    print("motion check: 0 animations type their own duration")
    return 0


if __name__ == "__main__":
    sys.exit(main())
