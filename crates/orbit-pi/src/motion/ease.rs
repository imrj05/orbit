//! The layer's easing curves: the one place a transition's shape is defined.
//!
//! `DESIGN.md` §Motion System puts easing inside the role. `Motion::easing`
//! resolves `Interaction` / `Layout` / `Travel` through [`ease_out_quint`] and
//! `Instant` through [`linear`]; `theme::tokens::chart::ease` resolves through
//! [`bezier`]. Keeping both here means a chart curve and a panel curve are built
//! by the same code even though they are different curves.

/// A linear ramp. The `Instant` role's easing: feedback is not travel.
pub(crate) fn linear(delta: f32) -> f32 {
    delta.clamp(0.0, 1.0)
}

/// Zed's ease-out quint, the deceleration every moving role shares. Delegates
/// to gpui's own function so the layer and the dependency cannot drift.
pub(crate) fn ease_out_quint(delta: f32) -> f32 {
    gpui::ease_out_quint()(delta.clamp(0.0, 1.0))
}

/// A cubic Bézier easing curve, parameterised by its two control points.
///
/// `x(s)` is monotone for the curves this crate uses, so bisection finds the
/// parameter that lands on a given `x` and then reads `y` there. The result is
/// clamped to 0..=1: gpui asserts the animated delta stays in range, and an
/// overshooting curve would trip that assertion instead of looking wrong.
pub(crate) fn bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 {
    move |t: f32| {
        let x = t.clamp(0.0, 1.0);
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..18 {
            let mid = (low + high) / 2.0;
            if curve(mid, x1, x2) < x {
                low = mid;
            } else {
                high = mid;
            }
        }
        curve((low + high) / 2.0, y1, y2).clamp(0.0, 1.0)
    }
}

fn curve(t: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - t;
    3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layer_easings_are_the_ones_the_roles_name() {
        // `Instant` is linear and feedback-shaped, not travel-shaped.
        assert_eq!(linear(0.0), 0.0);
        assert_eq!(linear(0.5), 0.5);
        assert_eq!(linear(1.0), 1.0);
        // `Interaction` / `Layout` / `Travel` decelerate, and match gpui's own
        // function exactly so `Motion::easing` and the layer agree.
        for step in 0..=20 {
            let delta = step as f32 / 20.0;
            assert_eq!(
                ease_out_quint(delta),
                gpui::ease_out_quint()(delta),
                "ease_out_quint drifted from gpui at {delta}"
            );
        }
        assert!(ease_out_quint(0.25) > 0.25, "decelerating, not linear");
    }

    #[test]
    fn every_curve_stays_inside_the_unit_interval() {
        let curves: [&dyn Fn(f32) -> f32; 3] = [&linear, &ease_out_quint, &|t| {
            bezier(0.85, 0.0, 0.15, 1.0)(t)
        }];
        for curve in curves {
            for step in -10..=110 {
                let value = curve(step as f32 / 100.0);
                assert!(
                    (0.0..=1.0).contains(&value),
                    "a curve left the unit interval at {step}%: {value}"
                );
            }
        }
    }

    #[test]
    fn the_bezier_starts_and_ends_on_its_endpoints() {
        let ease = bezier(0.85, 0.0, 0.15, 1.0);
        assert!(ease(0.0).abs() < 1e-6);
        assert!((ease(1.0) - 1.0).abs() < 1e-6);
        // Out-of-range input is clamped to the endpoints rather than extrapolated.
        assert_eq!(ease(-1.0), 0.0);
        assert_eq!(ease(2.0), 1.0);
    }

    #[test]
    fn the_bezier_never_travels_backwards() {
        let ease = bezier(0.85, 0.0, 0.15, 1.0);
        let mut previous = 0.0;
        for step in 0..=100 {
            let value = ease(step as f32 / 100.0);
            assert!(
                value >= previous - 1e-6,
                "the curve reversed at {step}%: {previous} -> {value}"
            );
            previous = value;
        }
    }
}
