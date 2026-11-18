#!/usr/bin/env python3
"""Fail when a UI element gets a second owner.

`docs/plans/ui-component-layer.md` gives every element family exactly one
defining file under `crates/orbit-pi/src/ui/`. This guard keeps it that way
with two rules:

1. **One definition.** Each named component (`struct Button`, `struct TreeRow`
   …) and each owning function (`fn search_field`, `fn virtual_list` …) is
   defined in its owner file only. A surface that defines a type or a
   top-level function with the same name has started a second owner — or is
   shadowing the shared one, which reads the same at a call site. Methods
   (indented `fn`) do not count: `.icon()` on a builder is not an `icon()`.
2. **Primitives stay in ui/.** The raw constructs the owners are built from
   (`anchored_popover(`, `.on_mouse_down_out(`, `segmented_segment(`) are not
   called outside `ui/`. A raw `icon_button_frame(` outside `ui/` must carry a
   `// Bespoke:` comment in the four lines above it, saying why `IconButton`
   does not fit.

Run from the repo root:

    python3 scripts/check_ui_owners.py             # check the crate
    python3 scripts/check_ui_owners.py --self-test # prove both rules can fail

Exit status 1 on a violation, so CI can gate on it.
"""
import os
import re
import sys
import tempfile

ROOT = "crates/orbit-pi/src"

# Named components: type name → the one file that defines it.
TYPES = {
    "Button": "ui/button.rs",
    "IconButton": "ui/button.rs",
    "ButtonGroup": "ui/button.rs",
    "LinkButton": "ui/button.rs",
    "ChipButton": "ui/chip.rs",
    "Badge": "ui/chip.rs",
    "Pill": "ui/chip.rs",
    "Banner": "ui/banner.rs",
    "ToastCard": "ui/toast.rs",
    "Tab": "ui/tab.rs",
    "TreeRow": "ui/tree.rs",
    "DiffRow": "ui/diff.rs",
    "StatusBar": "ui/status_bar.rs",
    "TextField": "ui/field.rs",
    "FormRow": "ui/field.rs",
    "ModalHeader": "ui/dialog.rs",
    "ModalFocus": "ui/dialog.rs",
    "Avatar": "ui/avatar.rs",
    "MenuState": "ui/menu.rs",
    "MenuCursor": "ui/menu.rs",
    "PickerRow": "ui/row.rs",
    "Column": "ui/table.rs",
}

# Owning functions: name → the one file that defines it (top level only).
FUNCTIONS = {
    "button_frame": "ui/button.rs",
    "icon_button_frame": "ui/button.rs",
    "segmented_segment": "ui/button.rs",
    "icon": "ui/icon.rs",
    "spinner": "ui/feedback.rs",
    "empty_state": "ui/feedback.rs",
    "error_state": "ui/feedback.rs",
    "progress_bar": "ui/feedback.rs",
    "skeleton_bar": "ui/feedback.rs",
    "search_field": "ui/field.rs",
    "input_field_frame": "ui/field.rs",
    "form_label": "ui/field.rs",
    "virtual_list": "ui/list.rs",
    "lightbox": "ui/lightbox.rs",
    "tab_strip": "ui/tab.rs",
    "list_row": "ui/row.rs",
    "picker_entry": "ui/row.rs",
    "table_row": "ui/table.rs",
    "data_table": "ui/table.rs",
    "dropdown_below": "ui/popover.rs",
    "dropdown_above": "ui/popover.rs",
    "column_menu": "ui/popover.rs",
    "anchored_popover": "ui/popover.rs",
    "modal_card": "ui/dialog.rs",
    "modal_scrim": "ui/dialog.rs",
    "modal_header": "ui/dialog.rs",
    "modal_footer": "ui/dialog.rs",
    "modal_button": "ui/dialog.rs",
    "menu_header": "ui/menu.rs",
    "context_menu_surface": "ui/menu.rs",
    "kbd_key": "ui/kbd.rs",
    "resize_handle": "ui/resize.rs",
    "switch": "ui/switch.rs",
}

# Named exceptions: a same-named item that is data, not an element. Each says
# why, so a new one has to argue its case here.
EXCEPTIONS = {
    # The changed-files tree's *model* (directory / file entries); its rows
    # are painted by `ui::tree::TreeRow`.
    ("review.rs", "TreeRow"),
}

# Primitives no surface calls directly.
PRIMITIVES = [
    (re.compile(r"\banchored_popover\("), "anchor popovers with ui::popover (dropdown_below/above, column_menu)"),
    (re.compile(r"\.on_mouse_down_out\("), "dismiss with ui::popover::PopoverExt::dismiss_on_outside"),
    (re.compile(r"\bsegmented_segment\("), "build segmented controls with ui::button::ButtonGroup"),
]
ICON_FRAME = re.compile(r"\bicon_button_frame\(")
BESPOKE = re.compile(r"//\s*Bespoke:")

TYPE_DEF = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?(?:struct|enum)\s+(\w+)\b")
FN_DEF = re.compile(r"^(?:pub(?:\([^)]*\))?\s+)?fn\s+(\w+)\s*[<(]")


def rust_files(root):
    for base, _dirs, files in os.walk(root):
        for name in sorted(files):
            if name.endswith(".rs"):
                yield os.path.join(base, name)


def read(path):
    with open(path, encoding="utf-8", errors="ignore") as handle:
        return handle.readlines()


def check(root=ROOT):
    violations = []
    for path in rust_files(root):
        rel = os.path.relpath(path, root).replace(os.sep, "/")
        in_ui = rel.startswith("ui/")
        lines = read(path)
        in_tests = False
        for number, line in enumerate(lines, start=1):
            stripped = line.strip()
            if stripped.startswith("#[cfg(test)]"):
                in_tests = True
            if stripped.startswith("//"):
                continue
            match = TYPE_DEF.match(line)
            if match:
                name = match.group(1)
                owner = TYPES.get(name)
                if owner and rel != owner and (rel, name) not in EXCEPTIONS:
                    violations.append((rel, number, f"`{name}` is owned by {owner}", stripped))
            match = FN_DEF.match(line)
            if match:
                name = match.group(1)
                owner = FUNCTIONS.get(name)
                if owner and rel != owner and (rel, name) not in EXCEPTIONS:
                    violations.append((rel, number, f"`{name}()` is owned by {owner}", stripped))
            if in_ui or in_tests:
                continue
            for pattern, advice in PRIMITIVES:
                if pattern.search(line):
                    violations.append((rel, number, advice, stripped))
            if ICON_FRAME.search(line):
                context = "".join(lines[max(0, number - 5) : number])
                if not BESPOKE.search(context):
                    violations.append(
                        (rel, number, "use ui::button::IconButton, or say why with `// Bespoke:`", stripped)
                    )
    return violations


def self_test():
    with tempfile.TemporaryDirectory() as tmp:
        os.makedirs(os.path.join(tmp, "ui"))
        with open(os.path.join(tmp, "ui", "button.rs"), "w", encoding="utf-8") as handle:
            handle.write("pub(crate) struct Button {}\npub(crate) fn icon_button_frame() {}\n")
        with open(os.path.join(tmp, "surface.rs"), "w", encoding="utf-8") as handle:
            handle.write("struct Button {}\n")
            handle.write("fn search_field() {}\n")
            handle.write("fn f() { anchored_popover(a); x.on_mouse_down_out(h); }\n")
            handle.write("fn g() { icon_button_frame(div(), t, s); }\n")
            handle.write("// Bespoke: a floating control\n")
            handle.write("fn h() { icon_button_frame(div(), t, s); }\n")
            handle.write("impl X { fn icon(self) {} }\n")
        found = check(tmp)
    rules = [rule for _p, _n, rule, _t in found]
    expected = [
        "`Button` is owned by ui/button.rs",
        "`search_field()` is owned by ui/field.rs",
        "anchor popovers with ui::popover (dropdown_below/above, column_menu)",
        "dismiss with ui::popover::PopoverExt::dismiss_on_outside",
        "use ui::button::IconButton, or say why with `// Bespoke:`",
    ]
    missing = [rule for rule in expected if rule not in rules]
    if missing:
        print(f"self-test FAILED: rules never fired: {missing}")
        return 1
    if rules.count("use ui::button::IconButton, or say why with `// Bespoke:`") != 1:
        print("self-test FAILED: a commented bespoke frame was reported")
        return 1
    if any(path.startswith("ui/") for path, _n, _r, _t in found):
        print("self-test FAILED: an owner file was reported")
        return 1
    print("self-test: every rule fires; owners, methods and commented bespoke frames pass")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    violations = check()
    if violations:
        print("A UI element has a second owner (docs/plans/ui-component-layer.md §9).")
        print("Use the ui/ component, or extend it; a one-off says why with `// Bespoke:`.\n")
        for path, number, rule, text in violations:
            print(f"  {path}:{number}: {rule}\n      {text}")
        return 1
    print(f"ui-owner check: {len(TYPES)} components and {len(FUNCTIONS)} owning functions have one home")
    return 0


if __name__ == "__main__":
    sys.exit(main())
