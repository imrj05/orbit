//! A critically damped move: quick to respond, quiet to settle, no overshoot.
//!
//! `DESIGN.md` §Motion System forbids bounce and overshoot. A critically damped
//! spring reaches its target without passing it, so it is the one spring the
//! layer allows — for a panel's width, where a fixed duration reads as a lurch
//! when the distance changes.
//!
//! The closed form is the standard one for a critically damped system:
//!
//! ```text
//! x(t) = target + (displacement + c·t)·e^(−ω·t),  c = velocity + ω·displacement
//! ```
//!
//! sampled at paint. There is no timer, so the motion is identical at 60, 90,
//! 120 and 144 Hz — the tests assert exactly that.
//!
//! `pixels_per_unit` is what makes the settle thresholds honest for a normalised
//! value (a 0..1 ratio) and a pixel value alike: 0.2 px of travel is the same
//! distance whichever unit the caller animates.

use std::time::Instant;

/// The distance a spring's pace is tuned for: the sidebar's width.
const BASE_DISTANCE: f32 = 360.0;
/// Settled once the value is within this many pixels of its target…
const SETTLE_PX: f32 = 0.2;
/// …and moving slower than this many pixels a second.
const SETTLE_VELOCITY_PX: f32 = 5.0;

/// One sample of a spring: where it is, how fast, and whether it has arrived.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpringSample {
    pub(crate) value: f32,
    pub(crate) velocity: f32,
    pub(crate) done: bool,
}

/// A move between two values, sampled at paint time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spring {
    from: f32,
    to: f32,
    velocity: f32,
    frequency: f32,
    pixels_per_unit: f32,
    limits: (f32, f32),
    started: Instant,
}

impl Spring {
    /// Start moving from `from` to `to` at `started`.
    ///
    /// `pixels_per_unit` is how many pixels one unit of `from`/`to` represents
    /// (1.0 for a value already in pixels, the panel's width for a 0..1 ratio).
    /// It scales the settle thresholds and nothing else.
    pub(crate) fn new(from: f32, to: f32, pixels_per_unit: f32, started: Instant) -> Self {
        let pixels_per_unit = if pixels_per_unit.is_finite() && pixels_per_unit > 0.0 {
            pixels_per_unit
        } else {
            1.0
        };
        let distance = (to - from).abs() * pixels_per_unit;
        // Longer moves take proportionally longer, but never so long that a
        // full-window panel feels sluggish.
        let pace = (distance / BASE_DISTANCE).sqrt().clamp(1.0, 1.35);
        let opening = to > from;
        Self {
            from,
            to,
            velocity: (to - from) * if opening { 12.0 } else { 13.5 } / pace,
            frequency: if opening { 34.0 } else { 40.0 } / pace,
            pixels_per_unit,
            limits: (from.min(to), from.max(to)),
            started,
        }
    }

    /// Where it began — the value an animator calls `from`.
    pub(crate) fn from(&self) -> f32 {
        self.from
    }

    /// Where it is heading.
    pub(crate) fn to(&self) -> f32 {
        self.to
    }

    /// Whether the move is an opening (value rising) rather than a closing.
    pub(crate) fn opening(&self) -> bool {
        self.to >= self.from
    }

    /// The closed-form sample at `now`.
    pub(crate) fn sample(&self, now: Instant) -> SpringSample {
        let t = now.saturating_duration_since(self.started).as_secs_f32();
        let displacement = self.from - self.to;
        let c = self.velocity + self.frequency * displacement;
        let decay = (-self.frequency * t).exp();
        let value = self.to + (displacement + c * t) * decay;
        let velocity = (self.velocity - self.frequency * c * t) * decay;

        if (value - self.to).abs() * self.pixels_per_unit < SETTLE_PX
            && velocity.abs() * self.pixels_per_unit < SETTLE_VELOCITY_PX
        {
            return SpringSample {
                value: self.to,
                velocity: 0.0,
                done: true,
            };
        }
        // A critically damped system does not pass its target; clamping guards
        // the analytic form against a caller that retargets mid-flight.
        SpringSample {
            value: value.clamp(self.limits.0, self.limits.1),
            velocity,
            done: false,
        }
    }

    /// The value and whether the move has settled.
    pub(crate) fn at(&self, now: Instant) -> (f32, bool) {
        let sample = self.sample(now);
        (sample.value, sample.done)
    }

    /// Point the spring at a new target without losing where it is or how fast
    /// it is going — the reversal a panel needs when it is reopened mid-close.
    pub(crate) fn retarget(&self, to: f32, now: Instant) -> Self {
        let sample = self.sample(now);
        let pixels_per_unit = self.pixels_per_unit;
        let remaining = (to - sample.value).abs() * pixels_per_unit;
        let mut next = Self::new(sample.value, to, pixels_per_unit, now);
        next.velocity = if remaining < SETTLE_PX {
            0.0
        } else {
            sample.velocity
        };
        next.limits = (self.limits.0.min(to), self.limits.1.max(to));
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_spring_settles_without_overshoot_at_any_refresh_rate() {
        for hz in [60, 90, 120, 144] {
            for distance in [220.0, 360.0, 640.0] {
                for opening in [true, false] {
                    let (from, to) = if opening {
                        (0.0, distance)
                    } else {
                        (distance, 0.0)
                    };
                    let start = Instant::now();
                    let spring = Spring::new(from, to, 1.0, start);
                    assert_eq!(
                        spring.at(start),
                        (from, false),
                        "the first frame is the start value"
                    );

                    let mut previous = from;
                    let mut finished = false;
                    for frame in 1..hz {
                        let (value, done) =
                            spring.at(start + Duration::from_secs_f32(frame as f32 / hz as f32));
                        assert!(
                            (0.0..=distance).contains(&value),
                            "{hz} Hz: {value} left {from}..{to}"
                        );
                        assert!(
                            if opening {
                                value >= previous
                            } else {
                                value <= previous
                            },
                            "{hz} Hz: a critically damped spring never travels back"
                        );
                        previous = value;
                        if done {
                            assert_eq!(value, to, "a settled spring is exactly its target");
                            finished = true;
                            break;
                        }
                    }
                    assert!(finished, "{hz} Hz: did not settle within a second");
                }
            }
        }
    }

    #[test]
    fn a_spring_reversal_keeps_position_and_velocity() {
        let start = Instant::now();
        let spring = Spring::new(0.0, 360.0, 1.0, start);
        spring.at(start);

        let now = start + Duration::from_millis(70);
        let before = spring.sample(now);
        let reversed = spring.retarget(0.0, now);
        let after = reversed.sample(now);

        assert!(
            (after.value - before.value).abs() < 1e-3,
            "the reversal jumps from {} to {}",
            before.value,
            after.value
        );
        assert!(
            (after.velocity - before.velocity).abs() < 1e-3,
            "the reversal drops the velocity it had"
        );
        assert_eq!(reversed.at(now + Duration::from_secs(1)), (0.0, true));
    }

    #[test]
    fn a_normalised_and_a_pixel_spring_move_the_same_distance() {
        let start = Instant::now();
        let pixels = Spring::new(0.0, 480.0, 1.0, start);
        let ratio = Spring::new(0.0, 0.5, 960.0, start);
        for millis in [0_u64, 16, 50, 100, 180, 300, 450] {
            let now = start + Duration::from_millis(millis);
            let (pixel_value, pixel_done) = pixels.at(now);
            let (ratio_value, ratio_done) = ratio.at(now);
            assert!(
                (pixel_value - ratio_value * 960.0).abs() < 0.001,
                "at {millis}ms: {pixel_value} vs {}",
                ratio_value * 960.0
            );
            assert_eq!(pixel_done, ratio_done);
        }
    }

    #[test]
    fn a_spring_with_no_distance_is_already_settled() {
        let now = Instant::now();
        let spring = Spring::new(0.5, 0.5, 1.0, now);
        assert_eq!(spring.at(now), (0.5, true));
    }
}
