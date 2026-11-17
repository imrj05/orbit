#!/usr/bin/env python3
"""Fail if the UI layer reaches back into a surface.

`docs/plans/ui-component-layer.md` §3.1 sets one dependency direction:

    theme  ->  ui/style  ->  ui  ->  app/* and every other surface

`ui/*` is the shared component layer, so it must not know about the app. If a
file there imports `crate::app`, names `OrbitApp`, or pulls in a surface module,
the arrow has silently reversed — and the next surface will copy whatever was
imported instead of asking for a component.

It also fails when a file under `ui/` calls `gpui::backdrop_blur_marker` outside
`ui/style/surface.rs`: material is resolved once, in the surface seam
(§3.4 rule 2), never re-derived per element.

Run from the repo root:

    python3 scripts/check_ui_layers.py           # check the crate
    python3 scripts/check_ui_layers.py --self-test  # prove the rules can fail

Exit status 1 on a violation, so CI can gate on it.
"""
import os
import re
import sys
import tempfile

ROOT = "crates/orbit-pi/src"
UI = "ui"

# Surfaces: modules a `ui/` file may not reach into. `app` covers `crate::app`,
# `super::` from inside ui resolves to `ui`, so only explicit paths matter.
SURFACE_MODULES = [
    "app",
    "command_palette",
    "composer",
    "dialog",
    "diff_view",
    "explorer",
    "git_panel",
    "model_selector",
    "sidepane",
    "terminal",
    "transcript",
    "transcript_view",
    "usage",
    "wallpaper",
    "widgets",
]

APP_PATH = re.compile(
    r"crate::(" + "|".join(SURFACE_MODULES) + r")(::|\b)|OrbitApp\b"
)

BLUR_MARKER = re.compile(r"\bbackdrop_blur_marker\b")
# The one file allowed to put a blur request in a border channel.
BLUR_ALLOWED = {"ui/style/surface.rs"}


def rust_files(root):
    for base, _dirs, files in os.walk(os.path.join(ROOT, UI)):
        for name in sorted(files):
            if name.endswith(".rs"):
                yield os.path.join(base, name)


def check(roots=None):
    """Return a list of (path, line number, rule, text) violations."""
    violations = []
    found_ui = False
    for path in rust_files(ROOT):
        found_ui = True
        rel = os.path.relpath(path, ROOT).replace(os.sep, "/")
        for number, line in enumerate(read(path), start=1):
            stripped = line.strip()
            if stripped.startswith("//"):
                continue
            if APP_PATH.search(line):
                violations.append((rel, number, "ui/ must not import a surface", stripped))
            if BLUR_MARKER.search(line) and rel not in BLUR_ALLOWED:
                violations.append(
                    (rel, number, "material is resolved in ui/style/surface.rs", stripped)
                )
    if not found_ui:
        violations.append((f"{ROOT}/{UI}", 0, "ui/ is missing", "nothing to check"))
    return violations


def read(path):
    with open(path, encoding="utf-8", errors="ignore") as handle:
        return handle.readlines()


def self_test():
    """Write one violating file and one clean file, and check both verdicts."""
    here = os.path.dirname(os.path.abspath(__file__))
    original = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp:
        os.chdir(tmp)
        os.makedirs(os.path.join(ROOT, UI, "style"), exist_ok=True)
        with open(os.path.join(ROOT, UI, "bad.rs"), "w", encoding="utf-8") as handle:
            handle.write("use crate::app::OrbitApp;\n")
            handle.write("fn f() { div().border_color(gpui::backdrop_blur_marker(1.0)) }\n")
        with open(os.path.join(ROOT, UI, "style", "surface.rs"), "w", encoding="utf-8") as handle:
            handle.write("fn f() { div().border_color(gpui::backdrop_blur_marker(1.0)) }\n")
        bad = check()
        rules = sorted({rule for _p, _n, rule, _t in bad})
        os.chdir(original)
    expected = {
        "ui/ must not import a surface",
        "material is resolved in ui/style/surface.rs",
    }
    missing = expected - set(rules)
    if missing:
        print(f"self-test FAILED: rules never fired: {sorted(missing)}")
        return 1
    # The allowed file must not be reported.
    if any(path.endswith("style/surface.rs") for path, _n, _r, _t in bad):
        print("self-test FAILED: ui/style/surface.rs was reported")
        return 1
    print(f"self-test: both rules fire, the allowed file is skipped ({os.path.basename(here)})")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    violations = check()
    if violations:
        print("The ui/ layer must not reach into a surface (docs/plans/ui-component-layer.md §3.1).")
        print("A shared component knows the theme and gpui — not the app.\n")
        for path, number, rule, text in violations:
            print(f"  {path}:{number}: {rule}\n      {text}")
        return 1
    print("ui-layer check: ui/ imports no surface and resolves material in one place")
    return 0


if __name__ == "__main__":
    sys.exit(main())
