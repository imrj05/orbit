#!/usr/bin/env python3
"""Export Orbit's theme as Design System files for the claude.ai design systems.

Orbit's look lives in Rust: the palettes in `crates/orbit-pi/src/theme.rs`, the
scales in `crates/orbit-pi/src/theme/tokens.rs`. Two Design System artifacts on
claude.ai mirror them so people and Claude can design against Orbit:

  orbit     Orbit Azure, Orbit Azure Light, Orbit, Orbit Light, Operational,
            Operational Light (the six main palettes)
  palettes  Orbit Darker, Orbit Cobalt, Orbit Onyx, Flexoki, Orbit Paper,
            Orbit Contrast, Flexoki Light (a design system holds at most 8 themes)

This script writes `project/tokens.json` and copies the bundled IBM Plex Sans and
Lilex faces into `project/fonts/` under an output folder. Palettes, spacing,
radii and elevation shadows are parsed from the Rust source, so a palette edit
flows through without retyping hex; the type scale, control and icon sizes are
mirrored below and cite where they come from.

It does not write the README, the component guides or the index
(`design-system.json`): those hold prose and uploaded asset ids, and are edited
on the artifact. Publish the output with Claude Code ("publish these tokens to
my Orbit design system") so the page's own files and asset records are kept.

Run from the repo root:

    python3 scripts/export_design_system.py orbit    out/design-system/orbit
    python3 scripts/export_design_system.py palettes out/design-system/palettes

Pass `--check <tokens.json>` to compare against an exported copy instead of
writing: exit status 1 when the Rust theme has drifted from it.
"""
import argparse
import colorsys
import json
import os
import re
import shutil
import subprocess
import sys

THEME_RS = "crates/orbit-pi/src/theme.rs"
TOKENS_RS = "crates/orbit-pi/src/theme/tokens.rs"
FONT_DIR = "crates/orbit-pi/assets/fonts/zed"

# (theme id, Palette const in theme.rs, label, appearance) in display order.
# The first theme is the one a token falls back to, so each set leads with its
# default dark palette.
SYSTEMS = {
    "orbit": ("Orbit", "Orbit", [
        ("azure", "ORBIT_AZURE", "Orbit Azure", "dark"),
        ("azure-light", "ORBIT_AZURE_LIGHT", "Orbit Azure Light", "light"),
        ("orbit", "ORBIT", "Orbit", "dark"),
        ("orbit-light", "ORBIT_LIGHT", "Orbit Light", "light"),
        ("operational", "OPERATIONAL", "Operational", "dark"),
        ("operational-light", "OPERATIONAL_LIGHT", "Operational Light", "light"),
    ]),
    "palettes": ("Orbit Palettes", "OrbitPalettes", [
        ("orbit-darker", "ORBIT_DARKER", "Orbit Darker", "dark"),
        ("orbit-cobalt", "ORBIT_COBALT", "Orbit Cobalt", "dark"),
        ("orbit-onyx", "ORBIT_ONYX", "Orbit Onyx", "dark"),
        ("flexoki", "FLEXOKI", "Flexoki", "dark"),
        ("orbit-paper", "ORBIT_PAPER", "Orbit Paper", "light"),
        ("orbit-contrast", "ORBIT_CONTRAST", "Orbit Contrast", "light"),
        ("flexoki-light", "FLEXOKI_LIGHT", "Flexoki Light", "light"),
    ]),
}

# The `Palette` struct's fields, in declaration order.
PALETTE_FIELDS = [
    "background", "surface", "surface_secondary", "elevated", "hover", "active",
    "selection", "selected_surface", "selection_fg", "border", "border_subtle",
    "text", "text_secondary", "text_muted", "text_disabled", "success", "error",
    "error_hover", "error_fg", "accent", "accent_hover", "accent_pressed",
    "accent_fg", "accent_text", "focus_ring", "action", "action_hover",
    "action_pressed", "action_fg", "warning", "info", "assistant_text", "code_bg",
    "code_text", "syn_string", "syn_number", "syn_function", "syn_type",
    "syn_comment", "syn_literal", "syn_meta", "syn_operator",
]


def fail(msg):
    sys.exit(f"export_design_system: {msg}")


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def parse_palette(src, const):
    m = re.search(r"const %s: Palette = Palette \{(.*?)\n\};" % const, src, re.S)
    if not m:
        fail(f"no `const {const}: Palette` in {THEME_RS}")
    fields = dict(
        (k, int(v, 16))
        for k, v in re.findall(r"^\s*(\w+): 0x([0-9A-Fa-f]{6}),", m.group(1), re.M)
    )
    if set(fields) != set(PALETTE_FIELDS):
        fail(f"{const} fields differ from the exporter's: {sorted(set(fields) ^ set(PALETTE_FIELDS))}")
    return fields


def check_struct_fields(src):
    m = re.search(r"struct Palette \{(.*?)\n\}", src, re.S)
    found = re.findall(r"^\s*(\w+): u32,", m.group(1), re.M) if m else []
    if found != PALETTE_FIELDS:
        fail("the Palette struct changed; update PALETTE_FIELDS and the colour roles below")


def h(v):
    return "#%06x" % v


def rgba(v, a):
    alpha = ("%.2f" % a).rstrip("0").rstrip(".")
    return "rgba(%d, %d, %d, %s)" % ((v >> 16) & 255, (v >> 8) & 255, v & 255, alpha)


def mention_file(v):
    """`Theme::mention_file`: the accent text's hue turned half way round."""
    r, g, b = ((v >> 16) & 255) / 255, ((v >> 8) & 255) / 255, (v & 255) / 255
    hue, light, sat = colorsys.rgb_to_hls(r, g, b)
    if sat < 0.15:
        return rgba(v, 0.7)
    r, g, b = colorsys.hls_to_rgb((hue + 0.5) % 1.0, light, max(sat, 0.5))
    return "#%02x%02x%02x" % (round(r * 255), round(g * 255), round(b * 255))


# Derived values mirror `Theme::build`: ink washes and the strong border are the
# primary ink at a mode-dependent alpha; scrims are black.
def wash(dark, light):
    return lambda p, m: rgba(p["text"], dark if m == "dark" else light)


def black(dark, light):
    return lambda p, m: rgba(0, dark if m == "dark" else light)


# (token, palette field or derivation, usage note)
COLOR_ROLES = [
    ("background", "background", "Window canvas, the plane the work sits on (`theme.background`). Panels stand on `surface`, never on this."),
    ("surface", "surface", "Panels and chrome: sidebar, panes, title bar, composer plane (`theme.surface`)."),
    ("surface-secondary", "surface_secondary", "Nested well inside a panel; never a substitute for a panel."),
    ("surface-elevated", "elevated", "The one raised plane: chips, menus, popovers, dialogs, metric boards. Reach it through ElevationIndex so the hairline and shadow come with it."),
    ("surface-hover", "hover", "Pointer hover fill for rows and controls. At least 1.045:1 over `surface-elevated`."),
    ("surface-active", "active", "Pressed or open control, rename row. One visible step above `surface-hover`."),
    ("surface-selected", "selected_surface", "Selected navigation/list row (the open session, the open file). Neutral step between hover and active; never equals `surface-hover`."),
    ("selection", "selection", "Text-selection and find-match wash in transcript, editor and terminal. A text wash, not a surface."),
    ("selection-fg", "selection_fg", "Ink on `selection` and on the selected navigation row."),
    ("border", "border", "1px region seam: between panes, around cards, table rules, input resting edge."),
    ("border-subtle", "border_subtle", "Quiet internal rule inside one region."),
    ("border-strong", wash(0.14, 0.16), "Floating-surface edge (modal card) and hover emphasis on a bordered control. Ink at 14% (dark) / 16% (light)."),
    ("text-primary", "text", "Body ink. At least 7:1 on `background`; also used on every surface and row wash."),
    ("text-secondary", "text_secondary", "Secondary ink: sidebar labels, row metadata, sorted column header. At least 4:1 on `background`."),
    ("text-muted", "text_muted", "Tertiary ink: section labels, placeholders, sort chevrons. At least 4.5:1 on `background` and on `surface-hover`, `surface-active` and `surface-selected`."),
    ("text-disabled", "text_disabled", "Ink of disabled controls, always over the `overlay` fill. Exempt from contrast floors; never used for readable content."),
    ("accent", "accent", "The palette's identity hue, rationed to at most 10% of a screen: caret, switch track (on), active data series, running-session arc. A fill, not an automatic button fill."),
    ("accent-hover", "accent_hover", "Hover step of an accent fill."),
    ("accent-pressed", "accent_pressed", "Pressed step of an accent fill."),
    ("accent-fg", "accent_fg", "Ink on an `accent` fill."),
    ("accent-text", "accent_text", "Readable accent for links, accent icons, `/command` tokens and syntax keywords. At least 4.5:1 on `background`."),
    ("action", "action", "Primary-button fill. Azure and Operational key it to the accent; Orbit keys it to a neutral step (light buttons, dark ink)."),
    ("action-hover", "action_hover", "Primary-button hover fill."),
    ("action-pressed", "action_pressed", "Primary-button pressed fill."),
    ("action-fg", "action_fg", "Label and icon on `action`, `action-hover`, `action-pressed`; at least 4.5:1 on each."),
    ("focus-ring", "focus_ring", "1px keyboard-focus ring; at least 3:1 on canvas and raised surfaces. Distinct from selection and the resting hairline."),
    ("success", "success", "Success status, reinforcing a word or icon. At least 3:1 as reinforcement."),
    ("warning", "warning", "Warning status, reinforcing a word or icon."),
    ("error", "error", "Danger and error status; destructive-button fill."),
    ("error-hover", "error_hover", "Destructive-button hover fill."),
    ("error-fg", "error_fg", "Ink on an `error` fill; at least 4.5:1."),
    ("info", "info", "Informational status, reinforcing a word or icon."),
    ("overlay", wash(0.05, 0.06), "Ink at 5% (dark) / 6% (light): disabled-control fill, keyboard-cursor row in Explorer, controls over wallpaper. Never a slab over a panel."),
    ("overlay-strong", wash(0.09, 0.10), "Ink at 9% (dark) / 10% (light): stronger translucent control over wallpaper."),
    ("toggle-knob", lambda p, m: h(0xFFFFFF) if m == "light" else h(p["text"]), "Switch knob: white in light palettes, primary ink in dark."),
    ("assistant-text", "assistant_text", "Assistant message ink in the transcript."),
    ("code-bg", "code_bg", "Code-block and editor background."),
    ("code-text", "code_text", "Code-block ink on `code-bg`."),
    ("inline-code-bg", lambda p, m: rgba(p["accent"], 0.10), "Inline-code wash: the accent at 10%."),
    ("mention-command", lambda p, m: "{accent-text}", "Composer `/command` token (Theme::mention_command): the readable accent."),
    ("mention-file", lambda p, m: mention_file(p["accent_text"]), "Composer `@file` mention (Theme::mention_file): the accent text's hue rotated 180°, saturation at least 50%, so a reference never reads as a command."),
    ("syn-string", "syn_string", "Syntax: strings."),
    ("syn-number", "syn_number", "Syntax: numbers."),
    ("syn-function", "syn_function", "Syntax: function names."),
    ("syn-type", "syn_type", "Syntax: types."),
    ("syn-comment", "syn_comment", "Syntax: comments."),
    ("syn-literal", "syn_literal", "Syntax: literals and constants."),
    ("syn-meta", "syn_meta", "Syntax: meta, attributes, preprocessor."),
    ("syn-operator", "syn_operator", "Syntax: operators and punctuation."),
    ("scrim", black(0.26, 0.14), "Backdrop under the command palette: black at 26% (dark) / 14% (light)."),
    ("scrim-modal", black(0.32, 0.18), "Backdrop under every blocking modal: black at 32% (dark) / 18% (light). One value for all modals."),
    ("scrim-media", black(0.72, 0.6), "Backdrop under the image lightbox: black at 72% (dark) / 60% (light)."),
]

SPACING_USAGE = {
    "base-02": "Icon padding inside its square hit target; switch track padding.",
    "base-04": "Button x-padding at Default/Compact; list top/bottom padding; gap between dense items.",
    "base-06": "Margin above and below a list separator.",
    "base-08": "Button x-padding at Medium/Large; inner padding of full-bleed list rows.",
    "base-12": "Section header-to-content gap; card inner padding (12-14px).",
    "base-16": "Generous inset inside dialogs and boards.",
    "base-20": "Gap between sections; content-plane gutters; switch track height.",
    "base-24": "Modal scrim padding around the centred card.",
}

RADIUS_USAGE = {
    "None": ("radius-none", "Radius::None. Square edges: flush panes and splits."),
    "XSmall": ("radius-xs", "Radius::XSmall. Tiny inline marks."),
    "Small": ("radius-sm", "Radius::Small. Badges and checkboxes."),
    "Medium": ("radius-md", "Radius::Medium (list_item::RADIUS). Sidebar and list rows, Compact/None button frames."),
    "Large": ("radius-lg", "Radius::Large (button::RADIUS, input::RADIUS). Default/Medium/Large buttons, chips, segmented controls, inline inputs."),
    "XLarge": ("radius-xl", "Radius::XLarge (panel::RADIUS). Cards, panels, popovers, menus, dialogs, metric boards."),
    "XXLarge": ("radius-xxl", "Radius::XXLarge. Rarely used intermediate step."),
    "XXXLarge": ("radius-xxxl", "Radius::XXXLarge. Also the chat composer's fixed corner (input::composer_radius), which ignores the rounding preference."),
    "Full": ("radius-full", "Radius::Full. Only genuinely round things: send/stop button, switch track and knob, status dots, avatars, meters."),
}

# Mirrored from tokens.rs: ButtonSize::height, IconSize::px, BORDER_WIDTH, and
# DESIGN.md's 44px header and 26px table row.
CONTROL_SIZES = [
    ("button-none", "16px", "ButtonSize::None height; icon at icon-indicator (10px); 1px x-padding."),
    ("button-compact", "18px", "ButtonSize::Compact height; icon at icon-xsmall (12px); base-04 x-padding; 5px corner."),
    ("button-default", "22px", "ButtonSize::Default height, also a dense list row; icon at icon-small (14px); base-04 x-padding."),
    ("button-medium", "28px", "ButtonSize::Medium height: sidebar rows, chips, header ghost controls; icon at icon-small; base-08 x-padding."),
    ("button-large", "32px", "ButtonSize::Large height: primary buttons, inputs (min height); icon at icon-medium (16px); base-08 x-padding."),
    ("header-height", "44px", "Page header row with a bottom hairline."),
    ("table-row", "26px", "Data-table row height."),
    ("border-width", "1px", "BORDER_WIDTH and FOCUS_RING_WIDTH: every border and the focus ring."),
]
ICON_SIZES = [
    ("icon-indicator", "10px", "IconSize::Indicator: dense tree rows, ButtonSize::None."),
    ("icon-xsmall", "12px", "IconSize::XSmall: chevrons and disclosure glyphs, ButtonSize::Compact."),
    ("icon-small", "14px", "IconSize::Small: sidebar rows (in a 20px slot), menu entries, chips, Default/Medium buttons."),
    ("icon-medium", "16px", "IconSize::Medium (default): Large buttons."),
    ("icon-xlarge", "48px", "IconSize::XLarge: empty-state illustrations."),
]

FONTS = [
    ("IBMPlexSans-Regular.ttf", "IBM Plex Sans", "400", "normal"),
    ("IBMPlexSans-Italic.ttf", "IBM Plex Sans", "400", "italic"),
    ("IBMPlexSans-SemiBold.ttf", "IBM Plex Sans", "600", "normal"),
    ("IBMPlexSans-SemiBoldItalic.ttf", "IBM Plex Sans", "600", "italic"),
    ("Lilex-Regular.ttf", "Lilex", "400", "normal"),
    ("Lilex-Italic.ttf", "Lilex", "400", "italic"),
    ("Lilex-Bold.ttf", "Lilex", "700", "normal"),
    ("Lilex-BoldItalic.ttf", "Lilex", "700", "italic"),
]

# HeadlineSize rems × 16px, TextSize steps, and DESIGN.md §Hierarchy.
TYPE_GROUPS = [
    {"name": "Interface", "family": "sans", "styles": [
        {"name": "headline-xlarge", "fontSize": "22.88px", "lineHeight": 1.3, "fontWeight": 600, "sample": "Workspace settings", "usage": "HeadlineSize::XLarge (1.43rem)."},
        {"name": "headline-large", "fontSize": "20.32px", "lineHeight": 1.3, "fontWeight": 600, "sample": "Usage this month", "usage": "HeadlineSize::Large (1.27rem)."},
        {"name": "headline-medium", "fontSize": "18px", "lineHeight": 1.3, "fontWeight": 600, "sample": "Connect a provider", "usage": "HeadlineSize::Medium (1.125rem)."},
        {"name": "headline-small", "fontSize": "16px", "lineHeight": 1.3, "fontWeight": 600, "sample": "Delete worktree?", "usage": "HeadlineSize::Small: every modal headline (modal::HEADLINE)."},
        {"name": "number", "fontSize": "20px", "lineHeight": "24px", "fontWeight": 500, "sample": "18.4M", "usage": "One per metric cell, in `text-primary`, never accent. Tabular figures where supported."},
        {"name": "title", "fontSize": "16px", "lineHeight": 1.3, "fontWeight": 500, "sample": "Sessions", "usage": "TextSize::Large: page titles."},
        {"name": "page-header", "fontSize": "15px", "lineHeight": "20px", "fontWeight": 500, "sample": "Appearance", "usage": "Title inside a 44px page header row."},
        {"name": "body", "fontSize": "14px", "lineHeight": "20px", "fontWeight": 400, "sample": "Refactor the session cursor to step with j and k", "usage": "TextSize::Default (the UI font size setting): UI text, list rows, table cells. Set to 13 for the denser rhythm."},
        {"name": "body-small", "fontSize": "12px", "lineHeight": "16px", "fontWeight": 400, "sample": "main · 3 files changed", "usage": "TextSize::Small: secondary row text, chips, buttons, list sub-headers."},
        {"name": "label", "fontSize": "11px", "lineHeight": "16px", "fontWeight": 500, "letterSpacing": "0.02em", "sample": "RECENT SESSIONS", "usage": "TextSize::XSmall, uppercase, in `text-muted`: section labels and table headers, paired with a count on the same row."},
    ]},
    {"name": "Code", "family": "mono", "styles": [
        {"name": "mono", "fontSize": "13px", "lineHeight": "20px", "fontWeight": 400, "sample": "git commit -m \"feat(focus)\"", "usage": "Editor font size (13px default): code, diffs, paths, hashes, durations, exact figures."},
        {"name": "mono-standard", "fontSize": "13px", "lineHeight": 1.3, "fontWeight": 400, "sample": "fn switch_travel(theme: &Theme) -> Pixels", "usage": "BufferLineHeight::Standard (1.3) for dense buffers."},
        {"name": "mono-comfortable", "fontSize": "13px", "lineHeight": 1.618, "fontWeight": 400, "sample": "let travel = px(36.) - padding * 2.0;", "usage": "BufferLineHeight::Comfortable (1.618): context menus and reading-heavy buffers."},
    ]},
]


def parse_spacing(src):
    rows = re.findall(r"Self::Base(\d\d) => \(([\d.]+), ([\d.]+), ([\d.]+)\),", src)
    if len(rows) != 14:
        fail(f"expected 14 DynamicSpacing rows in {TOKENS_RS}, found {len(rows)}")
    out = []
    for step, compact, default, comfy in rows:
        name = f"base-{step}"
        note = SPACING_USAGE.get(name, f"DynamicSpacing::Base{step}.")
        out.append({"name": name, "value": "%gpx" % float(default),
                    "usage": f"{note} Compact {float(compact):g}px / comfortable {float(comfy):g}px."})
    return out


def parse_radius(src):
    body = re.search(r"impl Radius \{(.*?)\n\}", src, re.S)
    found = dict(re.findall(r"Self::(\w+) => px\(([\d.]+)\)", body.group(1) if body else ""))
    found.setdefault("None", "0")   # Radius::None and Full are not px() arms.
    found.setdefault("Full", "9999")
    missing = set(RADIUS_USAGE) - set(found)
    if missing:
        fail(f"Radius steps missing from {TOKENS_RS}: {sorted(missing)}")
    return [{"name": RADIUS_USAGE[k][0], "value": "%gpx" % float(found[k]), "usage": RADIUS_USAGE[k][1]}
            for k in RADIUS_USAGE]


def parse_elevation(src):
    found = dict(re.findall(r"Self::(ElevatedSurface|ModalSurface) => vec!\[drop_shadow\(([^)]*)\)\]", src))
    if set(found) != {"ElevatedSurface", "ModalSurface"}:
        fail(f"could not read ElevationIndex::shadow in {TOKENS_RS}")

    def css(args):
        y, blur, alpha = (float(a.strip().rstrip(".")) for a in args.split(","))
        return "0 %gpx %gpx %s" % (y, blur, rgba(0, alpha))

    return css(found["ElevatedSurface"]), css(found["ModalSurface"])


def git_ref():
    try:
        branch = subprocess.check_output(["git", "rev-parse", "--abbrev-ref", "HEAD"], text=True).strip()
        sha = subprocess.check_output(["git", "rev-parse", "--short", "HEAD"], text=True).strip()
        return f"{branch}@{sha}"
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def build(system):
    title, _, spec = SYSTEMS[system]
    theme_src, tokens_src = read(THEME_RS), read(TOKENS_RS)
    check_struct_fields(theme_src)
    pal = {tid: parse_palette(theme_src, const) for tid, const, _, _ in spec}
    mode = {tid: m for tid, _, _, m in spec}
    themes = [tid for tid, _, _, _ in spec]

    colors = []
    for name, src, usage in COLOR_ROLES:
        value = {t: (src(pal[t], mode[t]) if callable(src) else h(pal[t][src])) for t in themes}
        colors.append({"name": name, "value": value, "usage": usage})

    elevated, modal = parse_elevation(tokens_src)
    shadows = [
        {"name": "elevation-2", "value": elevated, "usage": "ElevationIndex::ElevatedSurface: menus, dropdowns, tooltips, popovers, a notice floating over a pane. Always with a 1px hairline."},
        {"name": "elevation-3", "value": modal, "usage": "ElevationIndex::ModalSurface: modals, dialogs, the command palette. With `border-strong`."},
        # Theme::composer_shadow: offset 0 2, blur 10, spread -3, shadow_contact.
        {"name": "composer-shadow", "value": {t: "0 2px 10px -3px " + rgba(0, 0.32 if mode[t] == "dark" else 0.10) for t in themes},
         "usage": "Theme::composer_shadow: tight contact shadow for the chat composer and the Git commit box only. Not a card treatment."},
    ]

    tokens = {
        "name": title,
        "version": 1,
        "meta": {
            "source": "github", "repo": "local checkout (orbit)", "ref": git_ref(), "package": "crates/orbit-pi",
            "paths": {"tokens": [THEME_RS, TOKENS_RS], "fonts": [FONT_DIR],
                      "assets": ["crates/orbit-pi/assets/icons", "crates/orbit-pi/assets/app-icon.png",
                                 "crates/orbit-pi/assets/logo.png", "crates/orbit-pi/assets/logo-dark.png"],
                      "docs": ["DESIGN.md", ".agents/skills/orbit-design/SKILL.md"]},
            "exporter": "scripts/export_design_system.py",
        },
        "color": {"themes": [{"id": tid, "name": label} for tid, _, label, _ in spec], "tokens": colors},
        "type": {
            "fonts": [{"family": fam, "file": f"fonts/{f}", "weight": w, "style": st} for f, fam, w, st in FONTS],
            "families": {"sans": "\"IBM Plex Sans\", system-ui, sans-serif", "mono": "Lilex, ui-monospace, monospace"},
            "groups": TYPE_GROUPS,
        },
        "spacing": {"note": "DynamicSpacing at default density; compact is 80%, comfortable 120%, before UI font scaling.",
                    "tokens": parse_spacing(tokens_src)},
        "radius": {"note": "Before the corner-rounding preference, which adds up to 4px to intermediate steps (not none or full).",
                   "tokens": parse_radius(tokens_src)},
        "shadow": {"note": "Structural surfaces cast nothing. Only floating layers and the composer have a shadow.", "tokens": shadows},
        "controlSize": {"note": "Reference heights before UI scaling.",
                        "tokens": [{"name": n, "value": v, "usage": u} for n, v, u in CONTROL_SIZES]},
        "iconSize": {"tokens": [{"name": n, "value": v, "usage": u} for n, v, u in ICON_SIZES]},
    }
    return tokens


def comparable(tokens):
    """Everything but provenance, which changes on every run."""
    return {k: v for k, v in tokens.items() if k != "meta"}


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("system", choices=sorted(SYSTEMS))
    ap.add_argument("out", nargs="?", help="output folder; files land in <out>/project/")
    ap.add_argument("--check", metavar="TOKENS_JSON", help="compare with an exported tokens.json instead of writing")
    args = ap.parse_args()
    if not os.path.isfile(THEME_RS):
        fail("run from the repo root")

    tokens = build(args.system)

    if args.check:
        with open(args.check, encoding="utf-8") as f:
            current = json.load(f)
        if comparable(current) == comparable(tokens):
            print(f"{args.system}: {args.check} matches the Rust theme")
            return
        names = lambda t: {x["name"]: x["value"] for fam in ("color", "spacing", "radius", "shadow") for x in t.get(fam, {}).get("tokens", [])}
        a, b = names(current), names(tokens)
        changed = sorted(k for k in set(a) | set(b) if a.get(k) != b.get(k))
        print(f"{args.system}: {args.check} has drifted from the Rust theme", file=sys.stderr)
        for k in changed:
            print(f"  {k}: {a.get(k)} -> {b.get(k)}", file=sys.stderr)
        if not changed:
            print("  (type, sizes or usage notes differ)", file=sys.stderr)
        sys.exit(1)

    if not args.out:
        ap.error("an output folder is required unless --check is given")
    project = os.path.join(args.out, "project")
    os.makedirs(os.path.join(project, "fonts"), exist_ok=True)
    with open(os.path.join(project, "tokens.json"), "w", encoding="utf-8") as f:
        json.dump(tokens, f, indent=1, ensure_ascii=False)
        f.write("\n")
    for name, *_ in FONTS:
        shutil.copyfile(os.path.join(FONT_DIR, name), os.path.join(project, "fonts", name))
    print(f"{args.system}: wrote {project}/tokens.json ({len(tokens['color']['tokens'])} colours × "
          f"{len(tokens['color']['themes'])} themes) and {len(FONTS)} fonts")


if __name__ == "__main__":
    main()
