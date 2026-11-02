# Performance baseline — UI/UX Modernization

**Issue:** OPI-39 (Phase 9 — Performance and accessibility validation)
**Recorded:** 2026-11-01 · `feat/ui-redesign` + uncommitted phase 1–3 work
**Machine:** Apple Silicon macOS, `dev` profile (`opt-level = 1`)

The epic asks for measurements of transcript scroll, panel resize and palette
filtering, with before/after numbers. Only the pure paths can be measured
headlessly in this repository today; the rendering paths need a GPU harness that
does not exist yet. This document records what is measured, how to reproduce it,
and what remains unmeasured — rather than inventing numbers.

## How to reproduce

```sh
cargo test -p orbit-pi --bin orbit-pi baseline -- --ignored --nocapture
```

The test (`theme::tokens::tests::baseline_palette_and_layout_costs`) times only
pure computation, so it is stable enough to record but too fast to gate CI on.

## Measured

| Path | Time | What it covers |
| -- | --: | -- |
| Palette resolve | **171 ns** per palette | `Theme::for_id(id)` for all 12 shipped palettes |
| Layout restore | **638 ns** per launch | JSON parse + bounds validation of the four persisted sizes |
| Registry scan | **0.3 µs** per launch | Iterating `COMMANDS` |

Read together: the work a launch pays before the first frame — resolving the
theme and restoring the workspace — is **under one microsecond**. Neither is a
launch-time risk, which is why the epic's "restoring a layout must be fast
enough to be invisible" requirement is satisfied by construction rather than by
optimisation.

## Rendering: the in-app sampler

Because gpui 0.2.2 has no `#[gpui::bench]` and the headless test window's
`draw()` is a no-op, frame cost can only be measured in the real app. A **debug
build** carries a sampler (`crates/orbit-pi/src/perf.rs`):

1. Run `cargo run -p orbit-pi` (debug builds only — the module is behind
   `#[cfg(debug_assertions)]`, and a release binary carries neither the hook nor
   the chord). On launch the terminal should say
   `perf: frame sampler ready — ⌘⌥P starts a capture`. **If that line is missing,
   the binary you are running does not contain the sampler** — it is a release or
   a stale build, and no keypress will help.

   **No-keypress path:** `ORBIT_PERF=10 cargo run -p orbit-pi` records for ten
   seconds from launch and files the report by itself. Use it when the chord is
   the suspect rather than the sampler.
2. Press **⌘⌥P** to start. A toast confirms; the chord is deliberately outside the
   command registry, so it never appears in the palette or the Shortcuts page.
3. Drive the interaction you want measured — scroll a long transcript, drag a
   panel edge, type into the palette.
4. Press **⌘⌥P** again. The summary is written to `~/.orbit-pi/perf.jsonl` and
   shown in a toast. A capture nobody stops files itself after two minutes.

```sh
python3 scripts/read-perf.py
```

The reader starts with a **readiness line**, because "no captures" has two very
different causes and they look identical otherwise:

| Reader says | Meaning |
| -- | -- |
| `sampler: NEVER SEEN` | no debug build carrying the sampler has launched on this machine — you are running a release, a bundled app, or a stale binary |
| `sampler: last seen …` but no rows | the app ran with the sampler; the chord or the sampling is the problem |
| rows present | it worked |

**What it measures.** The interval between frames the app actually produced, in
milliseconds: mean, p50, p95, p99, max. **It does not force frames** — no
`request_animation_frame` is issued, so a still window produces no samples. Gaps
longer than 250 ms are dropped as idle time, so the percentiles describe work
rather than waiting. A capture with no frames reports nothing rather than a
fabricated zero.

**What it means.** Compare against the frame budget: 8.33 ms at 120 Hz, 16.67 ms
at 60 Hz. A p95 above the budget is visible jank. Because the numbers come from
one machine and one session, treat them as a baseline to compare against later
runs on the same machine — not as an absolute score.

**What it still cannot answer.** Per-phase render cost for the *past* phases: the
pre-epic baseline was never recorded. To answer "did this epic make scrolling
slower", build `v0.2.15` (`4036aceb`) and the branch and run the same capture on
both.

## First capture — 2026-11-01

The first real frame-time capture, taken with the in-app sampler on this machine.

**Machine:** Apple Silicon macOS, built-in display at **60.0 Hz**, 1470×956 logical
(`CGDisplayModeGetRefreshRate`). Frame budget therefore **16.67 ms**.
**Command:** `ORBIT_PERF=10 cargo run -p orbit-pi` (dev build), scrolled during the
ten seconds.

| frames | secs | mean | p50 | p95 | p99 | max |
| --: | --: | --: | --: | --: | --: | --: |
| 577 | 10.0 | **17.3** | **16.7** | **21.0** | **30.2** | **226.5** |

**Reading it:**

* **p50 = 16.7 ms is exactly one 60 Hz frame.** The app is pacing at vsync, not
  struggling to keep up — 577 frames in 10 s is 57.7 fps.
* **Mean 17.3 ms is just over the 16.67 ms budget**, and **p95 = 21.0 ms** means
  about 5 % of frames (≈29 in this window) take 26 % longer than a frame. That is
  the visible-hitch band, not a catastrophic one.
* **Max 226.5 ms is a stall of ~13 frames.** In a capture that started at launch
  this is almost certainly the first paint — which is why the sampler now ignores
  the first second of a launch-started capture (see below).

**Caveat, stated because it matters:** this run includes startup. It is a *first
baseline*, not a scroll-only number.

## Second capture — the scroll baseline (2026-11-01 22:03)

Started and stopped with the chord (so the chord path works), 30.9 seconds of
continuous activity, scrolling the transcript. Same machine, same 60 Hz display.

| capture | frames | secs | mean | p50 | p95 | p99 | max |
| -- | --: | --: | --: | --: | --: | --: | --: |
| `launch` (21:05, includes startup) | 577 | 10.0 | 17.3 | 16.7 | 21.0 | 30.2 | 226.5 |
| `capture` (22:03, scroll) | **1805** | **30.9** | **17.0** | **16.7** | **20.7** | **22.0** | **56.7** |

**The two runs agree on the middle and differ at the tail.** p50 is 16.7 ms in
both — the app paces at vsync. The 226 ms stall was startup: with startup excluded
(warm-up on the launch run, chord start on this one) the max falls to 56.7 ms, and
p99 drops from 30.2 to 22.0.

**What is left is a structural tail, not noise.** p95 is 20.7 ms here and 21.0 ms
in the launch capture — two runs with different durations, different content and
different start conditions landing within 0.3 ms of each other. About 5 % of
frames take 24 % longer than a 60 Hz frame, consistently. That is the number to
attack, and the one future changes should move.

`mean 17.0 ms × 1805 frames = 30.6 s`, close to the 30.9 s capture window — the
app was repainting continuously, which is what a scroll looks like.

**This is the baseline.** Any later capture on this machine should be compared
against these four numbers: **p50 16.7 · p95 20.7 · p99 22.0 · max 56.7**.

### Streaming measured with the instrument fixed (23:27)

Two auto-captures, started and stopped by the run itself — no chord, no scrolling,
1 s warm-up, idle threshold 100 ms.

| capture | frames | secs | mean | p50 | p95 | p99 | max | slow | gap |
| -- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| settled scroll 22:21 | 1034 | 17.3 | 16.7 | 16.7 | 17.6 | 17.7 | 42.0 | 117 | 5 |
| **stream A** | 1417 | 23.7 | **16.67** | 16.67 | **16.71** | 17.45 | **17.61** | 101 | 9 |
| **stream B** | 542 | 9.0 | 16.67 | 16.67 | **17.19** | 17.62 | 20.57 | 114 | 3 |
| mixed 120 s 23:26 | 7099 | 120.0 | 16.9 | 16.7 | 22.3 | 32.7 | 50.3 | 1301 | 3 |

**Streaming on its own is free.** p95 is **16.71 ms** in the long capture and
17.19 ms in the short one — at or *below* the settled-scroll baseline of 17.6,
and the worst frame in the 23.7 s run is 17.61 ms, i.e. under two thirds of a
missed vsync. Against the criteria fixed before the run (`gap` near the
heartbeat's cadence, p95 within ~2 ms of 17.6), **Phase 7's smoothness gate
passes for streaming**.

The 120 s capture is the outlier and it is **not attributable**: it ran to the
two-minute cap, so it mixes idle time, streaming and whatever the user did while
it ran. p95 22.3 / p99 32.7 says something in those two minutes was expensive;
without knowing whether the transcript was scrolled during it, that number cannot
be assigned to streaming or to scrolling.

### First streaming reading (23:16, chord-started)

A 52 s capture taken while a response streamed. Chord-started, so no warm-up and
no `stream` label — the auto-capture was not in that build.

| capture | frames | secs | mean | p50 | p95 | p99 | max | slow | gap |
| -- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| settled scroll 22:21 | 1034 | 17.3 | 16.7 | 16.7 | **17.6** | 17.7 | 42.0 | 117 | 5 |
| **streaming 23:16** | 3097 | 52.0 | 16.8 | 16.7 | **19.3** | 26.2 | **202.1** | 662 | 3 |

Streaming costs something: p95 is **1.7 ms** worse, the share of frames over
budget roughly **doubles** (11 % → 21 %), and the slow frames arrive every third
frame instead of every fifth.

**But the max is not trustworthy, and the instrument was at fault.** A capture
started by a keystroke has no warm-up, and `IDLE_GAP` was 250 ms — so the pause
between the app's last idle frame and the first frame after the chord was
recorded as a **202 ms frame**. It is the app asleep, not the app stalling: six
missed vsyncs of nothing happening. `IDLE_GAP` is now **100 ms** (six 60 Hz
frames; a real miss is ~33 ms), with a test asserting a 200 ms pause is not a
sample.

So: the distribution reading (p95 19.3, slow share 21 %) is real; the tail
reading (p99 26.2, max 202) is contaminated and must be re-measured. The verdict
stays open, and it is not leaning towards "free".

## The streaming experiment

Phase 7's gate is that a long streaming session stays smooth. That needs a
capture of frames produced *while tokens arrive*, and it should need no
stopwatch: a debug build now starts a capture labelled **`stream`** when the
active session begins streaming and files it when the run settles.

```
cargo run -p orbit-pi          # debug build
# ask for a long answer; use the app normally
python3 scripts/read-perf.py
```

The capture appears as a `stream` row with no keypress. The hook is armed from
the render path (`perf::ensure_armed`), because `tick` — which knows a run is
streaming — has no window; it is a single atomic load when no capture runs, so an
idle app still paints nothing.

**How to read it, decided before the run:**

| Result | Verdict |
| -- | -- |
| `gap` ≈ 5, p95 within ~2 ms of the settled baseline (17.6) | streaming repaint is free — Phase 7's smoothness gate passes |
| p95 ≥ 20 with a **noisy** gap | streaming repaint is the cost; that is a Phase 7 code target (coalescing, row rebuild) |
| p95 high but `gap` ≈ 5 | the same heartbeat cost as the settled run — nothing about streaming |

One capture is one sample: tool-call-heavy runs and pure-prose runs arrive at
different rates, so an ambiguous result wants a second capture before it is
believed.

## Attributing the tail

The p95 is stable across runs, which says the tail has a *cause* rather than being
noise. Two causes are worth separating, and the report now separates them:

```sh
python3 scripts/read-perf.py
```

```
slow = frames over 16.7ms · gap = median frames between two slow frames
```

| `gap` | Reading |
| --: | -- |
| small and stable (≈5) | periodic — a 90 ms heartbeat landing on a 60 Hz stream, or another fixed-cadence task |
| large or noisy | scattered — allocation, a one-off stall, a relayout |

The heartbeat in `main_window::spawn_heartbeat` idles at **90 ms**, which at ~58 fps
is one tick every ~5 frames — the first hypothesis to test against the recorded
gap.

Two safe reductions went in while looking:

* `ChatMessage::has_text()` answers "does this message have any text" without
  joining the whole turn. The render path asks that question per visible row per
  frame, and used to answer it by allocating the answer.
* The hover card's scan for the first non-empty answer no longer joins every
  message it walks past.

Neither is expected to move p95 on its own — a per-frame cost lifts the median, and
the median is already exactly one frame. They remove work from the frame that is
about to be slow, which is where it costs most.

## Third capture — the tail is periodic (22:21)

| capture | frames | secs | mean | p50 | p95 | p99 | max | slow | gap |
| -- | --: | --: | --: | --: | --: | --: | --: | --: | --: |
| `launch` 21:05 | 577 | 10.0 | 17.3 | 16.7 | 21.0 | 30.2 | 226.5 | — | — |
| `capture` 22:03 | 1805 | 30.9 | 17.0 | 16.7 | 20.7 | 22.0 | 56.7 | — | — |
| `capture` 22:21 | **1034** | **17.3** | **16.68** | **16.67** | **17.59** | **17.67** | **42.0** | **117** | **5** |

**The gap answers the question.** Every fifth frame is the slow one, exactly.
At 60 Hz that is a wake every ~83 ms, which is `spawn_heartbeat`'s **90 ms** idle
cadence — one heartbeat landing on every fifth frame. The tail is not allocation
churn and not scroll relayout; it is the app's own timer.

**The magnitude is small, and that matters more than the pattern.** The slow
frames run at 17.6 ms — **0.9 ms** over the 16.67 ms budget — and the mean is
16.68 ms, which is 59.9 fps. No frame is dropped: a missed 60 Hz deadline would
show as ~33 ms, and only one frame in the capture (42 ms) is anything like that.
So the heartbeat costs under a millisecond and the loop recovers it.

**The tighter tail is not yet attributable.** p95 fell from 20.7 to 17.6 ms
between the 22:03 and 22:21 captures, but the content and duration differed, so
the two render-path reductions (`has_text`, the hover-card scan) cannot take
credit for it on this evidence. What the run does establish is the *cadence*.

**Verdict:** this is below the level where it is worth changing a timer's
schedule. Gating the heartbeat to "only when something is pending" would trade a
real behaviour change for less than a millisecond. The one thing left to
understand is the *streaming* case, where the 22:03 capture's 20.7 ms tail came
from — that is a repaint-under-load question, not a timer one.

## Sampling rules that came out of the first run

The first capture exposed two flaws in the instrument, both fixed:

* **A launch-started capture counted the window's own first paint.** Frames inside
  the warm-up window (1 s) are now dropped, so `ORBIT_PERF` measures the
  interaction rather than the launch.
* **Nothing proved the sampler was in the binary.** A debug build now prints
  `perf: frame sampler ready — ⌘⌥P starts a capture` at launch, and every
  start/stop reports to stderr as well as a toast.

To record a scroll-only number, use the chord after the app has settled:

```bash
cargo run -p orbit-pi
# let it settle, then: ⌘⌥P → scroll a long transcript → ⌘⌥P
python3 scripts/read-perf.py
```

While a capture runs, the top bar carries a **`REC · 128`** chip counting the
frames sampled. It exists because a capture that samples nothing writes no
report — the difference between "the chord did not fire", "no frames arrived" and
"it worked" has to be visible *while* it is happening, not discovered afterwards.

## Unmeasured, and why

| Path | Why it is not measured here | What would measure it |
| -- | -- | -- |
| Transcript scroll (long sessions) | Recorded by the in-app sampler, but no capture has been taken yet | ⌘⌥P while scrolling (`src/perf.rs`) |
| Panel resize | Same | ⌘⌥P while dragging a panel edge |
| Palette filtering per keystroke | Same — the sampler measures painted frames, which is the number that matters | ⌘⌥P while typing in the palette |
| Streaming frame budget | Needs a live pi RPC session plus a capture | ⌘⌥P during a streaming turn |

The sampler makes these *measurable*; they are not yet *measured*, because that
needs one run on the machine that will be the baseline.

None of these has a recorded baseline, so **no phase in this epic can claim a
performance non-regression yet**. That is the honest state: correctness and
consistency work landed; the numbers did not.

## Accessibility checks that *are* enforced

These run in CI, over every shipped palette:

| Check | Test |
| -- | -- |
| Foregrounds clear 4.5:1 on their surfaces | `every_theme_has_readable_foregrounds` |
| Muted/secondary ink clears AA | `interactive_and_muted_foregrounds_clear_aa` |
| Selected-row metadata stays readable | `selection_rows_keep_their_metadata_readable` |
| Focus ring clears 3:1 on canvas and elevated | `every_theme_has_readable_foregrounds` |
| Surface ramp monotonic per palette | `every_theme_holds_its_surface_grammar` |
| Hover/pressed/selected never collide | `interaction_states_never_collide` |
| Motion roles sit inside the spec bands and collapse under Reduce Motion | `motion_roles_fill_the_spec_bands`, `motion_collapses_to_nothing_under_reduce_motion` |

## Known gaps and follow-ups

1. **No rendering baseline.** Build a `gpui::bench` target for transcript
   scroll, panel resize and palette filtering before any future performance
   claim. This is the single blocking item for OPI-39's done-gate.
2. ~~OS Reduce Motion is not read.~~ **Closed.** GPUI has no accessibility API, so
   the read goes through the platform layer: `platform::os_reduce_motion()` calls
   `NSWorkspace.accessibilityDisplayShouldReduceMotion`, cached in `theme` and
   refreshed at launch and on window activation. Surfaces read
   `theme.reduce_motion()`, which ORs the two sources. Linux and Windows have no
   equivalent read and fall back to the in-app toggle.
3. **Keyboard sweep not done.** The state contract gives every control a focus
   ring, but no automated pass walks the 12 surfaces and asserts reachability.
4. ~~Contrast is asserted for pairs, not for every rendered combination.~~
   **Closed.** `metadata_stays_readable_on_every_row_state` now checks muted ink
   on hover, active and selected rows, assistant ink on the canvas, and code ink
   on the code plane — every palette, both modes. It found a systemic failure:
   muted ink sat below 4.5:1 on row washes in 20 of 36 combinations (worst:
   Flexoki on active at 3.30, Orbit Azure on selected at 3.65). Eleven palettes
   had their `text_muted` raised by the smallest step that clears AA on their
   worst wash; the values are recorded in `DESIGN.md`.
