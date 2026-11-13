#!/usr/bin/env python3
"""Print the frame-time captures Orbit recorded.

The app writes one JSON object per capture to `~/.orbit-pi/perf.jsonl` when a
debug build is running and the user pressed ⌘⌥P (`src/perf.rs`, debug builds
only). This reads that file back as a table, newest last.

A capture measures the interval between frames the app actually produced while
the user drove an interaction — a scroll, a panel drag, a palette typing burst.
Idle gaps are dropped, so the percentiles describe work rather than waiting.

Usage:

    python3 scripts/read-perf.py            # the default report
    python3 scripts/read-perf.py PATH       # another file

The capture cannot be produced by a test: gpui 0.2.2 has no `#[gpui::bench]`, and
the headless test window's `draw()` is a no-op, so frame cost only exists in the
real app.
"""
import json
import os
import sys
from pathlib import Path


def default_path() -> Path:
    return Path.home() / ".orbit-pi" / "perf.jsonl"


def read(path: Path) -> list[dict]:
    if not path.exists():
        return []
    rows = []
    with path.open(encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError:
                # A truncated line (the app was killed mid-write) is skipped
                # rather than failing the whole report.
                continue
    return rows


def readiness() -> dict | None:
    marker = Path.home() / ".orbit-pi" / "perf-ready.json"
    if not marker.exists():
        return None
    try:
        return json.loads(marker.read_text(encoding="utf-8"))
    except (json.JSONDecodeError, OSError):
        return None


def describe_readiness() -> str:
    marker = readiness()
    if marker is None:
        return "sampler: NEVER SEEN — no debug build with the sampler has launched on this machine."
    stamp = __import__("datetime").datetime.fromtimestamp(marker.get("started_at", 0))
    return (
        f"sampler: last seen {stamp:%Y-%m-%d %H:%M:%S} "
        f"(pid {marker.get('pid', '?')}, v{marker.get('version', '?')})"
    )


def main(argv: list[str]) -> int:
    path = Path(argv[1]) if len(argv) > 1 else default_path()
    rows = read(path)
    print(describe_readiness())
    print()
    if not rows:
        print(f"No captures in {path}.")
        print("Run a debug build, press ⌘⌥P, drive the interaction, press it again.")
        return 1

    header = (
        f"{'when':<20}{'label':<14}{'frames':>7}{'secs':>7}{'mean':>8}{'p50':>8}"
        f"{'p95':>8}{'p99':>8}{'max':>8}{'slow':>7}{'gap':>7}"
    )
    print(header)
    print("-" * len(header))
    for row in rows:
        when = row.get("recorded_at", 0)
        stamp = (
            __import__("datetime").datetime.fromtimestamp(when).strftime("%Y-%m-%d %H:%M:%S")
            if when
            else "-"
        )
        print(
            f"{stamp:<20}{str(row.get('label', '-'))[:13]:<14}"
            f"{row.get('frames', 0):>7}{row.get('seconds', 0.0):>7.1f}"
            f"{row.get('mean_ms', 0.0):>8.1f}{row.get('p50_ms', 0.0):>8.1f}"
            f"{row.get('p95_ms', 0.0):>8.1f}{row.get('p99_ms', 0.0):>8.1f}"
            f"{row.get('max_ms', 0.0):>8.1f}"
            f"{row.get('slow_frames', 0):>7}"
            f"{(row.get('slow_spacing') or 0):>7.0f}"
        )
    print()
    print("Frame budget: 8.33ms at 120Hz, 16.67ms at 60Hz.")
    print(
        "slow = frames over 25ms (a missed vsync) · gap = median frames between two slow frames.\n"
        "A small, stable gap means the tail is periodic (a heartbeat landing on a\n"
        "frame); a large or noisy one means it is scattered (allocation, stalls)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
