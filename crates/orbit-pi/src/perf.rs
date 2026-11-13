//! Frame-time sampling for the render baseline — **debug builds only**.
//!
//! This is not a benchmark harness. gpui 0.2.2 has no `#[gpui::bench]`, and the
//! headless test window's `draw()` is a no-op, so the only honest place to
//! measure frame cost is the real app on the real GPU. This module records the
//! interval between frames the app actually produced while the user drives the
//! interaction (a scroll, a panel drag, a palette typing burst), which is the
//! number `docs/performance-baseline.md` is missing.
//!
//! **It samples, it does not force.** No `request_animation_frame` is issued, so
//! a still window produces no frames and therefore no samples: the percentiles
//! describe work, not waiting. Gaps longer than [`IDLE_GAP`] are dropped as idle
//! time rather than counted as a slow frame.
//!
//! Output is one JSON object per capture, appended to `~/.orbit-pi/perf.jsonl`.
//! `scripts/read-perf.py` prints it as a table.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A gap longer than this is the app waiting, not the app working. Dropped.
///
/// 100 ms is six 60 Hz frames: a real frame that misses a vsync is ~33 ms, and
/// two missed is ~50 ms. Anything slower than that is the app asleep between
/// repaints — most often the pause between the last idle frame and the first
/// frame after a keystroke, which would otherwise be recorded as a 200 ms stall.
const IDLE_GAP: Duration = Duration::from_millis(100);

/// A capture stops by itself here, so a forgotten recording cannot run forever.
const MAX_CAPTURE: Duration = Duration::from_secs(120);
const MAX_SAMPLES: usize = 20_000;

/// How long a launch-started capture ignores its own frames. From launch the
/// first second is window setup and first paint; counting it would report
/// startup as interaction cost.
const WARMUP: Duration = Duration::from_secs(1);

/// An interval longer than this is a frame the app missed.
///
/// Not the 16.7 ms budget itself: frames paced by vsync land with jitter, so a
/// smooth 60 Hz stream alternates ~15.7 and ~17.6 ms intervals and half of it
/// would read as slow. One missed vsync is ~33 ms; 25 ms sits halfway, so only
/// a frame that actually skipped a refresh is counted.
const MISSED_FRAME_MS: f32 = 25.0;

/// Whether one frame interval is a missed frame rather than vsync jitter.
fn is_missed_frame(ms: f32) -> bool {
    ms > MISSED_FRAME_MS
}

/// One running capture.
struct Capture {
    label: String,
    started: Instant,
    /// The previous frame's timestamp. `None` until the first frame lands, so
    /// the wait between pressing the chord and the first repaint is not a
    /// sample.
    previous: Option<Instant>,
    /// When to stop by itself. Set for a capture started from the environment,
    /// so a launch-and-interact run needs no second keypress.
    deadline: Option<Instant>,
    /// Samples before this instant are dropped: a capture that starts at launch
    /// would otherwise report the window's first paint as an interaction cost.
    warmup_until: Instant,
    /// Frame intervals in milliseconds.
    samples: Vec<f32>,
    /// Indices of frames over the 60 Hz budget, so the reader can show whether
    /// slow frames are periodic (a heartbeat) or scattered (allocation).
    slow: Vec<usize>,
}

/// The finished numbers for one capture.
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub label: String,
    pub frames: usize,
    pub seconds: f64,
    pub mean_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
    pub max_ms: f64,
    /// Frames that missed a vsync (see [`MISSED_FRAME_MS`]).
    pub slow_frames: usize,
    /// Median distance, in frames, between two slow frames. Periodic slow
    /// frames (a heartbeat landing on a frame) show a small, stable number;
    /// scattered ones (allocation, GC) do not.
    pub slow_spacing: Option<f64>,
}

impl Summary {
    /// One line of the report, for a toast or a log.
    pub fn line(&self) -> String {
        format!(
            "{}: {} frames over {:.1}s · p50 {:.1}ms · p95 {:.1}ms · max {:.1}ms",
            self.label, self.frames, self.seconds, self.p50_ms, self.p95_ms, self.max_ms
        )
    }
}

static CAPTURE: Mutex<Option<Capture>> = Mutex::new(None);

/// Whether a capture is running. The view paints a recording chip from this,
/// so the user can see the sampler is armed — a tool that only reports after
/// the fact is hard to trust while wiring it up.
pub fn is_active() -> bool {
    CAPTURE.lock().map(|slot| slot.is_some()).unwrap_or(false)
}

/// How many frames the running capture has sampled. Zero when idle.
pub fn sample_count() -> usize {
    CAPTURE
        .lock()
        .map(|slot| {
            slot.as_ref()
                .map(|capture| capture.samples.len())
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

/// Begin a capture. False when one is already running — a second press stops it
/// instead of starting a nested one.
pub fn start(label: &str) -> bool {
    let mut slot = CAPTURE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_some() {
        return false;
    }
    let started = Instant::now();
    *slot = Some(Capture {
        label: label.to_owned(),
        started,
        previous: None,
        deadline: None,
        warmup_until: started,
        samples: Vec::new(),
        slow: Vec::new(),
    });
    true
}

/// Start a capture that stops by itself after `seconds`, and arm the frame hook.
///
/// This is the path with no keypress in it: `ORBIT_PERF=8 cargo run -p orbit-pi`
/// records for eight seconds from launch. It exists because a chord that never
/// fires is indistinguishable from a sampler that never ran.
pub fn start_timed(label: &str, seconds: f32, window: &mut gpui::Window) -> bool {
    if !start(label) {
        return false;
    }
    {
        let mut slot = CAPTURE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(capture) = slot.as_mut() {
            let deadline = Instant::now() + Duration::from_secs_f32(seconds.max(1.));
            capture.deadline = Some(deadline);
            // Skip the window's own first paint: from launch, the first second
            // is startup, not the interaction the user wants measured.
            capture.warmup_until = capture.started + WARMUP;
        }
    }
    arm(window);
    true
}

/// Record that a debug build with the sampler started, so the reader can say
/// "the app ran" or "it never ran" without anyone having to watch a terminal.
pub fn note_launch() {
    let path = crate::platform::home_dir()
        .join(".orbit-pi")
        .join("perf-ready.json");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let marker = serde_json::json!({
        "pid": std::process::id(),
        "started_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "version": env!("CARGO_PKG_VERSION"),
    });
    let _ = std::fs::write(path, marker.to_string());
}

/// Honour `ORBIT_PERF` at launch. No-op without it, and debug builds only — the
/// whole module is behind `cfg(debug_assertions)`.
pub fn start_from_env(window: &mut gpui::Window) {
    let Some(raw) = std::env::var("ORBIT_PERF").ok() else {
        return;
    };
    let seconds: f32 = raw.trim().parse().unwrap_or(10.);
    if start_timed("launch", seconds, window) {
        eprintln!("perf: recording {seconds:.0}s from launch (ORBIT_PERF)");
    }
}

/// Whether the frame hook is currently armed. The hook is one-shot, so it has to
/// be re-armed each frame; this keeps `ensure_armed` from stacking callbacks.
static ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Arm the frame hook if a capture is live and nothing is armed yet.
///
/// Called from the render path, which is the only place with a `Window`. It is a
/// single atomic load when idle, and it is what lets a capture start from `tick`
/// (which has no window) without a keypress.
pub fn ensure_armed(window: &mut gpui::Window) {
    if !is_active() {
        return;
    }
    if ARMED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    window.on_next_frame(frame);
}

/// Start a capture when a run begins streaming and file it when the run settles.
///
/// This is the experiment that needs no keypress: the transcript cost of arrival
/// is measured by using the app, not by driving a stopwatch. Called from `tick`,
/// which runs on every heartbeat while a session is active.
pub fn auto_capture(streaming: bool) {
    let running = CAPTURE
        .lock()
        .map(|slot| {
            slot.as_ref()
                .map(|capture| capture.label == STREAM_LABEL)
                .unwrap_or(false)
        })
        .unwrap_or(false);
    match (streaming, running) {
        (true, false) => {
            if start(STREAM_LABEL) {
                eprintln!("perf: streaming capture started");
            }
        }
        (false, true) => {
            if let Some(summary) = finish() {
                eprintln!("perf: {}", summary.line());
            }
        }
        _ => {}
    }
}

/// The label an auto-captured streaming run carries.
const STREAM_LABEL: &str = "stream";

/// Arm the frame hook. Called when a capture starts, because the hook is not
/// kept running: a permanently armed callback would repaint an idle app at the
/// display's refresh rate, which is a different app than the one being measured.
pub fn arm(window: &mut gpui::Window) {
    ensure_armed(window);
}

fn frame(window: &mut gpui::Window, _cx: &mut gpui::App) {
    if tick(Instant::now()) {
        window.on_next_frame(frame);
    } else {
        ARMED.store(false, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Record one frame. Called from the frame hook while a capture is running.
///
/// Returns true while the capture should keep its frame loop armed.
pub fn tick(now: Instant) -> bool {
    let over = {
        let mut slot = CAPTURE
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(capture) = slot.as_mut() else {
            return false;
        };
        let expired = capture
            .deadline
            .map(|deadline| now >= deadline)
            .unwrap_or(false);
        if expired
            || now.duration_since(capture.started) >= MAX_CAPTURE
            || capture.samples.len() >= MAX_SAMPLES
        {
            true
        } else {
            if let Some(previous) = capture.previous {
                let gap = now.duration_since(previous);
                if gap <= IDLE_GAP && previous >= capture.warmup_until {
                    let ms = gap.as_secs_f32() * 1000.;
                    if is_missed_frame(ms) {
                        capture.slow.push(capture.samples.len());
                    }
                    capture.samples.push(ms);
                }
            }
            capture.previous = Some(now);
            return true;
        }
    };
    if over {
        // A forgotten capture stops itself and still files its report, so the
        // numbers are not lost to a missing keypress.
        if let Some(summary) = finish() {
            eprintln!("perf: {}", summary.line());
        }
    }
    false
}

/// Finish the capture and append its summary to the report. `None` when nothing
/// was running, or when no frames arrived (a capture with no samples would be a
/// fabricated number).
pub fn finish() -> Option<Summary> {
    let capture = CAPTURE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()?;
    let summary = summarize(
        &capture.label,
        &capture.samples,
        &capture.slow,
        capture.started.elapsed().as_secs_f64(),
    )?;
    append(&summary);
    Some(summary)
}

/// Percentiles over the sampled intervals. `None` when there is nothing to
/// report: an empty capture is not a zero.
pub(crate) fn summarize(
    label: &str,
    samples: &[f32],
    slow: &[usize],
    seconds: f64,
) -> Option<Summary> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let sum: f32 = sorted.iter().sum();
    Some(Summary {
        label: label.to_owned(),
        frames: sorted.len(),
        seconds,
        mean_ms: (sum / sorted.len() as f32) as f64,
        p50_ms: percentile(&sorted, 0.50) as f64,
        p95_ms: percentile(&sorted, 0.95) as f64,
        p99_ms: percentile(&sorted, 0.99) as f64,
        max_ms: sorted[sorted.len() - 1] as f64,
        slow_frames: slow.len(),
        slow_spacing: slow_spacing(slow),
    })
}

/// Median distance between consecutive slow-frame indices. `None` with fewer
/// than two of them — one slow frame has no spacing.
pub(crate) fn slow_spacing(slow: &[usize]) -> Option<f64> {
    if slow.len() < 2 {
        return None;
    }
    let mut gaps: Vec<f64> = slow
        .windows(2)
        .map(|pair| (pair[1] - pair[0]) as f64)
        .collect();
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(gaps[gaps.len() / 2])
}

/// Nearest-rank percentile on a sorted slice.
pub(crate) fn percentile(sorted: &[f32], fraction: f64) -> f32 {
    if sorted.is_empty() {
        return 0.;
    }
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

#[cfg(not(test))]
fn report_path() -> std::path::PathBuf {
    crate::platform::home_dir()
        .join(".orbit-pi")
        .join("perf.jsonl")
}

#[cfg(not(test))]
fn append(summary: &Summary) {
    let path = report_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let line = serde_json::json!({
        "label": summary.label,
        "frames": summary.frames,
        "seconds": summary.seconds,
        "mean_ms": summary.mean_ms,
        "p50_ms": summary.p50_ms,
        "p95_ms": summary.p95_ms,
        "p99_ms": summary.p99_ms,
        "max_ms": summary.max_ms,
        "slow_frames": summary.slow_frames,
        "slow_spacing": summary.slow_spacing,
        "recorded_at": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
    });
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        use std::io::Write;
        let _ = writeln!(file, "{line}");
    }
}

/// Tests exercise `finish`, but they must never append to the report a real
/// capture writes to the user's home directory.
#[cfg(test)]
fn append(_summary: &Summary) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The capture lives in a process-wide static, so the tests that touch it
    /// take this lock: `cargo test` runs them on parallel threads otherwise.
    static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

    fn take_capture() -> std::sync::MutexGuard<'static, ()> {
        CAPTURE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let sorted: Vec<f32> = (1..=100).map(|v| v as f32).collect();
        assert_eq!(percentile(&sorted, 0.50), 50.);
        assert_eq!(percentile(&sorted, 0.95), 95.);
        assert_eq!(percentile(&sorted, 0.99), 99.);
        assert_eq!(percentile(&sorted, 1.0), 100.);
        assert_eq!(percentile(&[], 0.5), 0.);
    }

    #[test]
    fn a_summary_reports_the_sample_it_was_given() {
        let summary = summarize("scroll", &[16.0, 8.0, 24.0, 12.0], &[2], 1.5).expect("samples");
        assert_eq!(summary.frames, 4);
        assert_eq!(summary.mean_ms, 15.0);
        assert_eq!(summary.max_ms, 24.0);
        assert_eq!(summary.p50_ms, 12.0);
        assert!(summary.line().starts_with("scroll: 4 frames over 1.5s"));
    }

    /// A timed capture stops at its deadline even though the global cap is two
    /// minutes away, and files what it has.
    #[test]
    fn a_deadline_stops_a_capture() {
        let _guard = take_capture();
        let _ = finish();
        assert!(start("timed"));
        {
            let mut slot = CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let capture = slot.as_mut().expect("running");
            capture.deadline = Some(Instant::now() + Duration::from_millis(50));
        }
        let now = Instant::now();
        // Two active frames inside the window.
        assert!(tick(now));
        assert!(tick(now + Duration::from_millis(16)));
        // The next frame is past the deadline: the loop stops and the capture
        // files itself.
        assert!(!tick(now + Duration::from_millis(200)));
    }

    /// Frames inside the warm-up window are the window's first paint, not the
    /// interaction; a launch-started capture must not report them.
    /// The pause between an idle app and the first frame after a keystroke is not
    /// a frame: a 200 ms "sample" is six missed vsyncs of nothing happening, and
    /// recording it inflates the max and the p99.
    #[test]
    fn an_idle_pause_is_not_a_slow_frame() {
        let _guard = take_capture();
        let _ = finish();
        assert!(start("pause"));
        let base = Instant::now();
        assert!(tick(base));
        assert!(tick(base + Duration::from_millis(16)));
        // 200 ms of the app doing nothing, then it wakes for a keystroke.
        assert!(tick(base + Duration::from_millis(216)));
        assert!(tick(base + Duration::from_millis(232)));
        let samples = {
            let slot = CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            slot.as_ref().expect("running").samples.clone()
        };
        assert_eq!(samples, vec![16.0, 16.0], "the 200 ms pause is not a frame");
        let _ = finish();
    }

    #[test]
    fn warmup_frames_are_not_samples() {
        let _guard = take_capture();
        let _ = finish();
        assert!(start("warmup"));
        {
            let mut slot = CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let capture = slot.as_mut().expect("running");
            capture.warmup_until = capture.started + Duration::from_millis(100);
        }
        let base = Instant::now();
        assert!(tick(base));
        // Inside the warm-up: the gap is measured from a warm-up frame, so it is
        // not a sample.
        assert!(tick(base + Duration::from_millis(16)));
        // Still inside.
        assert!(tick(base + Duration::from_millis(32)));
        let samples = {
            let slot = CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            slot.as_ref().expect("running").samples.clone()
        };
        assert!(samples.is_empty(), "warm-up frames are startup, not cost");
        let _ = finish();
    }

    #[test]
    fn an_empty_capture_is_not_a_zero() {
        assert!(summarize("scroll", &[], &[], 2.0).is_none());
    }

    /// The auto-capture is a state machine over the run's lifecycle: start once
    /// when streaming begins, file once when it settles, and never nest.
    #[test]
    fn auto_capture_tracks_the_run() {
        let _guard = take_capture();
        let _ = finish();

        // A run begins: one capture, labelled for what it measured.
        auto_capture(true);
        assert!(is_active());
        assert_eq!(
            CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_ref()
                .map(|capture| capture.label.as_str()),
            Some(STREAM_LABEL)
        );
        // A second tick while streaming must not restart it.
        auto_capture(true);
        assert!(is_active());

        // The run settles: it files itself and stops.
        auto_capture(false);
        assert!(!is_active());
        // Idle again: nothing to do.
        auto_capture(false);
        assert!(!is_active());
    }

    /// Vsync jitter around the budget is a smooth stream, not a slow one; a
    /// skipped refresh is.
    #[test]
    fn only_a_skipped_refresh_is_a_slow_frame() {
        assert!(!is_missed_frame(15.7));
        assert!(!is_missed_frame(17.6));
        assert!(!is_missed_frame(22.7));
        assert!(is_missed_frame(33.3));
        assert!(is_missed_frame(50.0));
    }

    /// A periodic tail and a scattered tail look the same in the percentiles and
    /// completely different here: the median gap between slow frames is what
    /// tells a heartbeat apart from allocation noise.
    #[test]
    fn slow_frame_spacing_separates_periodic_from_scattered() {
        assert_eq!(slow_spacing(&[]), None);
        assert_eq!(slow_spacing(&[7]), None);
        // Every fifth frame: a ~90ms heartbeat landing on a 60Hz stream.
        assert_eq!(slow_spacing(&[5, 10, 15, 20, 25]), Some(5.0));
        // Scattered: gaps of 1, 26, 1, 59 frames — no rhythm to read.
        assert_eq!(slow_spacing(&[3, 4, 30, 31, 90]), Some(26.0));
    }

    /// The capture lifecycle and the sampling rules share one global, so they
    /// are exercised in one test rather than racing each other on parallel
    /// threads.
    #[test]
    fn capture_lifecycle_and_gap_filtering() {
        let _guard = take_capture();
        let _ = finish();
        // Exclusive: a running capture is not restarted.
        assert!(start("first"));
        assert!(!start("second"), "a running capture is not restarted");

        let start_at = Instant::now();
        // The first frame only primes the clock — no interval yet.
        assert!(tick(start_at));
        // An active frame.
        assert!(tick(start_at + Duration::from_millis(16)));
        // The app goes quiet for a second: waiting, not a slow frame.
        assert!(tick(start_at + Duration::from_millis(1016)));
        assert!(tick(start_at + Duration::from_millis(1032)));
        assert!(tick(start_at + Duration::from_millis(1048)));

        let samples = {
            let slot = CAPTURE
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            slot.as_ref().expect("running").samples.clone()
        };
        assert_eq!(
            samples,
            vec![16.0, 16.0, 16.0],
            "the idle second is not a frame"
        );

        // Finishing files the summary and clears the slot.
        assert!(finish().is_some());

        // A capture with no frames is not a zero, and reports nothing.
        assert!(start("empty"));
        assert!(finish().is_none());
    }
}
