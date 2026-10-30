//! The Summary board's sparklines.
//!
//! One 40px line per headline metric, mapped from the bklit **KPI sparklines**
//! recipe (bklit.com/recipes#kpi-sparklines): hovering any line points every
//! card in the row at that bucket, so all four numbers, dates and changes read
//! the same day. Leaving snaps back to the range.
//!
//! The recipe's motion, kept to the data and nothing else: the line draws itself
//! in over 900ms on first paint (cubic-bezier .85, 0, .15, 1), and a range
//! change morphs the old shape into the new one over 500ms, resampling when the
//! bucket count changed. Pointing at a line does not animate: the guide, the
//! marker and the highlighted segment land on the pointed bucket immediately,
//! so reading the row never waits on a transition. Reduced motion paints the
//! final frame.
//!
//! Geometry is the recipe's too: a point scale with points on the plot's edges,
//! a min..max domain (a small move stays visible), the area filled to the
//! clamped zero, a 1.5px monotone stroke, a 3px marker ringed in the card
//! colour, and a 2/2 dashed guide. The palette is Orbit's: the metric the trend
//! below is plotting keeps the one accent, the rest are quiet ink.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    canvas, div, fill, linear_color_stop, linear_gradient, point, prelude::*, px, size, Animation,
    AnimationExt, AnyElement, App, Bounds, ContentMask, ElementId, Hsla, PathBuilder, Pixels,
    Window,
};

use super::aggregate::{ChartMetric, SeriesPoint, TimeSeries};
use super::chart::curve_controls;
use crate::theme::Theme;

/// Rendered height of one sparkline (the recipe's `height={40}`).
pub(super) const HEIGHT: f32 = 40.;
/// The recipe's 4px plot padding, widened to 5 so the marker's 4.5px ring is
/// never clipped by the canvas edge.
const PAD: f32 = 5.;
const STROKE: f32 = 1.5;
const DOT_R: f32 = 3.;
const RING: f32 = 1.5;

/// The recipe's cadences: 900ms draw-in and 500ms data transition. Hover has no
/// transition.
const DRAW: Duration = Duration::from_millis(900);
const MORPH: Duration = Duration::from_millis(500);

/// The recipe's `DEFAULT_ANIMATION_EASING`: `cubic-bezier(0.85, 0, 0.15, 1)`.
fn recipe_ease() -> impl Fn(f32) -> f32 {
    cubic_bezier(0.85, 0.0, 0.15, 1.0)
}

/// A cubic Bézier easing curve, parameterised by its two control points.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32) -> impl Fn(f32) -> f32 {
    move |t: f32| {
        let x = t.clamp(0.0, 1.0);
        if x <= 0.0 {
            return 0.0;
        }
        if x >= 1.0 {
            return 1.0;
        }
        // x(s) is monotone for these curves, so bisection finds the curve
        // parameter that lands on this x; then read y there.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        for _ in 0..18 {
            let mid = (low + high) / 2.0;
            if bezier(mid, x1, x2) < x {
                low = mid;
            } else {
                high = mid;
            }
        }
        bezier((low + high) / 2.0, y1, y2).clamp(0.0, 1.0)
    }
}

fn bezier(t: f32, p1: f32, p2: f32) -> f32 {
    let mt = 1.0 - t;
    3.0 * mt * mt * t * p1 + 3.0 * mt * t * t * p2 + t * t * t
}

/// One card of the Summary board. Fixed order: the board builds its cells and
/// the page caches the previous shapes in the same order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum KpiMetric {
    Requests,
    Tokens,
    CacheRate,
    Latency,
}

impl KpiMetric {
    pub(super) const ALL: [Self; 4] =
        [Self::Requests, Self::Tokens, Self::CacheRate, Self::Latency];

    /// Position in [`Self::ALL`], for indexing a cached shape list.
    pub(super) fn index(self) -> usize {
        match self {
            Self::Requests => 0,
            Self::Tokens => 1,
            Self::CacheRate => 2,
            Self::Latency => 3,
        }
    }

    /// Stable element-id root; one card per metric, so it is unique.
    pub(super) fn id(self) -> &'static str {
        match self {
            Self::Requests => "usage-kpi-requests",
            Self::Tokens => "usage-kpi-tokens",
            Self::CacheRate => "usage-kpi-cache",
            Self::Latency => "usage-kpi-latency",
        }
    }

    /// The bucket's measure, or `None` where the bucket has nothing to measure
    /// for this card — a gap in the line, never a fabricated zero (§6).
    pub(super) fn value(self, point: &SeriesPoint) -> Option<f64> {
        match self {
            Self::Requests => Some(point.totals.requests as f64),
            Self::Tokens => Some(point.totals.tokens.total as f64),
            Self::CacheRate => point.totals.cache_hit_rate(),
            Self::Latency => point.totals.avg_duration_ms(),
        }
    }

    /// The chart metric that carries this card's period-over-period change.
    pub(super) fn chart_metric(self) -> ChartMetric {
        match self {
            Self::Requests => ChartMetric::Requests,
            Self::Tokens => ChartMetric::Tokens,
            Self::CacheRate => ChartMetric::Cache,
            Self::Latency => ChartMetric::Latency,
        }
    }
}

/// Every card's line over the range's buckets, in [`KpiMetric::ALL`] order.
/// Gaps are `NaN`.
pub(super) fn kpi_values(series: &TimeSeries) -> Vec<Vec<f64>> {
    KpiMetric::ALL
        .into_iter()
        .map(|metric| {
            series
                .points
                .iter()
                .map(|point| metric.value(point).unwrap_or(f64::NAN))
                .collect()
        })
        .collect()
}

/// Whether two cached shape lists are the same data. `NaN` is a gap and must
/// compare equal to `NaN`, or every refresh would replay the morph.
pub(super) fn same_values(a: &[Vec<f64>], b: &[Vec<f64>]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(left, right)| {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| (left.is_nan() && right.is_nan()) || left == right)
        })
}

/// Change from the previous point, in percent; `None` for the first point or a
/// bucket with no baseline. The recipe's `percentChange`.
pub(super) fn point_delta(values: &[f64], index: usize) -> Option<f64> {
    let current = *values.get(index)?;
    let previous = *values.get(index.checked_sub(1)?)?;
    (current.is_finite() && previous.is_finite() && previous.abs() > f64::EPSILON)
        .then(|| (current - previous) / previous * 100.0)
}

/// The previous shape's value at the new shape's index `i`. A range change can
/// change the bucket count, and the recipe resamples the old series to the new
/// point count rather than snapping.
fn sample(previous: &[f64], i: usize, count: usize) -> Option<f64> {
    if previous.is_empty() || i >= count {
        return None;
    }
    if previous.len() == count {
        return previous[i].is_finite().then(|| previous[i]);
    }
    let position = if count > 1 {
        i as f64 / (count - 1) as f64 * (previous.len() - 1) as f64
    } else {
        0.0
    };
    let lower = position.floor() as usize;
    let upper = (lower + 1).min(previous.len() - 1);
    let (start, end) = (previous[lower], previous[upper]);
    if !start.is_finite() || !end.is_finite() {
        return None;
    }
    Some(start + (end - start) * (position - lower as f64))
}

/// The plot's vertical span: the data's own min..max, widened when flat, so a
/// small move is visible (the recipe's `includeZero: false` domain).
fn domain(values: &[f64]) -> (f64, f64) {
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for value in values.iter().copied().filter(|value| value.is_finite()) {
        low = low.min(value);
        high = high.max(value);
    }
    if !low.is_finite() || !high.is_finite() {
        return (0.0, 1.0);
    }
    if (high - low).abs() < f64::EPSILON {
        return (low - 1.0, high + 1.0);
    }
    (low, high)
}

fn normalize(value: f64, low: f64, high: f64) -> f32 {
    if !value.is_finite() || high <= low {
        return 0.5;
    }
    ((value - low) / (high - low)).clamp(0.0, 1.0) as f32
}

/// Contiguous measured runs; the line breaks at a gap instead of bridging it.
fn finite_runs(values: &[f64]) -> Vec<Range<usize>> {
    let mut runs = Vec::new();
    let mut start = 0;
    while start < values.len() {
        if !values[start].is_finite() {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < values.len() && values[end].is_finite() {
            end += 1;
        }
        runs.push(start..end);
        start = end;
    }
    runs
}

fn last_measured(values: &[f64]) -> Option<usize> {
    (0..values.len()).rev().find(|ix| values[*ix].is_finite())
}

/// Everything one sparkline needs to paint.
pub(super) struct Spark<'a> {
    pub id: &'static str,
    pub values: &'a [f64],
    /// The previous shape, for the 500ms data transition.
    pub previous: Option<&'a [f64]>,
    /// The bucket under the pointer, shared by the whole board.
    pub hover: Option<usize>,
    /// Restarts the data transition when the range's buckets change.
    pub morph_serial: u64,
    pub color: Hsla,
    pub reduced_motion: bool,
    pub theme: Theme,
}

/// The page's pointer callback: the bucket under the pointer, or nothing.
pub(super) type HoverFn = Rc<dyn Fn(Option<usize>, &mut Window, &mut App)>;

pub(super) fn sparkline(spec: Spark<'_>, on_hover: HoverFn) -> AnyElement {
    let Spark {
        id,
        values: source,
        previous: source_previous,
        hover,
        morph_serial,
        color,
        reduced_motion,
        theme,
    } = spec;

    let values = source.to_vec();
    let previous = source_previous.map(|values| values.to_vec());
    let count = values.len();
    let hover = hover.filter(|ix| *ix < count);
    // Reading the board is immediate: the line dims and the read segment
    // highlights the moment the pointed bucket changes.
    let dim = if hover.is_some() { 0.5 } else { 1.0 };

    let reveal = Rc::new(Cell::new(if reduced_motion { 1.0_f32 } else { 0.0 }));
    let morph = Rc::new(Cell::new(1.0_f32));

    let paint_reveal = reveal.clone();
    let paint_morph = morph.clone();
    let paint = move |bounds: Bounds<Pixels>, _: (), window: &mut Window, _: &mut App| {
        let width = f32::from(bounds.size.width);
        let height = f32::from(bounds.size.height);
        if width <= 0.0 || height <= 0.0 || values.is_empty() {
            return;
        }

        let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
        let (low, high) = domain(&values);
        let inner_h = (height - 2.0 * PAD).max(0.0);
        let y_at =
            |value: f64, low: f64, high: f64| PAD + inner_h * (1.0 - normalize(value, low, high));
        // The area closes on the clamped zero, which with a min..max domain is
        // the bottom of the plot for the all-positive series the page plots.
        let baseline = y_at(0.0_f64.clamp(low, high), low, high);
        let (previous_low, previous_high) = match previous.as_deref() {
            Some(previous) => domain(previous),
            None => (0.0, 1.0),
        };
        let blend = paint_morph.get().clamp(0.0, 1.0);
        let x_at = |ix: usize| {
            let inner_w = (width - 2.0 * PAD).max(0.0);
            if values.len() > 1 {
                PAD + inner_w * ix as f32 / (values.len() - 1) as f32
            } else {
                PAD + inner_w / 2.0
            }
        };
        // One y per bucket, blending the old pixel position into the new one.
        let y_at_point = |ix: usize| -> f32 {
            let Some(current) = values.get(ix).copied().filter(|value| value.is_finite()) else {
                return f32::NAN;
            };
            let target = y_at(current, low, high);
            let Some(previous) = previous.as_deref() else {
                return target;
            };
            if blend >= 1.0 {
                return target;
            }
            match sample(previous, ix, values.len()) {
                Some(earlier) if earlier.is_finite() => {
                    let earlier = y_at(earlier, previous_low, previous_high);
                    earlier + (target - earlier) * blend
                }
                _ => target,
            }
        };

        let runs = finite_runs(&values);
        let build_area = || {
            let mut area = PathBuilder::fill();
            let mut drawn = false;
            for run in &runs {
                if run.end - run.start < 2 {
                    continue;
                }
                area.move_to(at(x_at(run.start), y_at_point(run.start)));
                for ix in run.start + 1..run.end {
                    let (first, second) =
                        curve_controls(x_at(ix - 1), y_at_point(ix - 1), x_at(ix), y_at_point(ix));
                    area.cubic_bezier_to(
                        at(x_at(ix), y_at_point(ix)),
                        at(first.0, first.1),
                        at(second.0, second.1),
                    );
                }
                area.line_to(at(x_at(run.end - 1), baseline));
                area.line_to(at(x_at(run.start), baseline));
                area.close();
                drawn = true;
            }
            drawn.then(|| area.build().ok()).flatten()
        };
        let build_line = || {
            let mut line = PathBuilder::stroke(px(STROKE));
            let mut drawn = false;
            for run in &runs {
                line.move_to(at(x_at(run.start), y_at_point(run.start)));
                for ix in run.start + 1..run.end {
                    let (first, second) =
                        curve_controls(x_at(ix - 1), y_at_point(ix - 1), x_at(ix), y_at_point(ix));
                    line.cubic_bezier_to(
                        at(x_at(ix), y_at_point(ix)),
                        at(first.0, first.1),
                        at(second.0, second.1),
                    );
                }
                drawn = true;
            }
            drawn.then(|| line.build().ok()).flatten()
        };

        // The marker sits on the pointed bucket; there is nothing to travel
        // between, so reading a day never waits on a transition.
        let marker_x = hover.map(x_at).unwrap_or(0.0);
        let marker_y = hover.map(y_at_point).unwrap_or(f32::NAN);

        let revealed = width * paint_reveal.get().clamp(0.0, 1.0);
        let mask = ContentMask {
            bounds: Bounds::new(bounds.origin, size(px(revealed), px(height))),
        };
        window.with_content_mask(Some(mask), |window| {
            if let Some(path) = build_area() {
                window.paint_path(
                    path,
                    linear_gradient(
                        180.0,
                        linear_color_stop(color.opacity(0.22 * dim), 0.0),
                        linear_color_stop(color.opacity(0.0), 1.0),
                    ),
                );
            }
            if let Some(path) = build_line() {
                window.paint_path(path, color.opacity(dim));
            }
            // The read segment: the same line at full ink, clipped to the
            // window around the pointed bucket.
            if let Some(ix) = hover {
                let left_ix = ix.saturating_sub(1);
                let right_ix = (ix + 1).min(values.len() - 1);
                let left = x_at(left_ix);
                let span = (x_at(right_ix) - left).max(1.0);
                let segment = ContentMask {
                    bounds: Bounds::new(at(left, 0.0), size(px(span), px(height))),
                };
                window.with_content_mask(Some(segment), |window| {
                    if let Some(path) = build_line() {
                        window.paint_path(path, color);
                    }
                });
            }
            // The latest point is marked while nothing is hovered.
            if hover.is_none() {
                if let Some(ix) = last_measured(&values) {
                    paint_marker(
                        window,
                        at(x_at(ix), y_at_point(ix)),
                        color,
                        theme.surface_elevated,
                    );
                }
            }
        });

        // A card with no measured point has nothing to point at.
        if hover.is_some() && !runs.is_empty() {
            paint_crosshair(window, &at, marker_x, height, theme);
            if marker_y.is_finite() {
                paint_marker(
                    window,
                    at(marker_x, marker_y),
                    color,
                    theme.surface_elevated,
                );
            }
        }
    };

    let painted = canvas(|_, _, _| (), paint).absolute().inset_0();
    let painted: AnyElement = if reduced_motion {
        painted.into_any_element()
    } else {
        painted
            .with_animation(
                ElementId::Name(format!("{id}-draw").into()),
                Animation::new(DRAW).with_easing(recipe_ease()),
                move |canvas, progress| {
                    reveal.set(progress);
                    canvas
                },
            )
            .with_animation(
                ElementId::Name(format!("{id}-morph-{morph_serial}").into()),
                Animation::new(MORPH).with_easing(recipe_ease()),
                move |canvas, progress| {
                    morph.set(progress);
                    canvas
                },
            )
            .into_any_element()
    };

    // One invisible cell per bucket, so hit-testing is the layout's job (the
    // page's timeline uses the same trick) and the readout lands on a real
    // measurement, never an interpolated pixel.
    let clear = on_hover.clone();
    let cells: Vec<AnyElement> = (0..count)
        .map(|ix| {
            let set = on_hover.clone();
            div()
                .id(ElementId::NamedInteger(
                    format!("{id}-cell").into(),
                    ix as u64,
                ))
                .flex_1()
                .h_full()
                .on_hover(move |entered, window, cx| {
                    if *entered {
                        set(Some(ix), window, cx);
                    }
                })
                .into_any_element()
        })
        .collect();

    div()
        .id(ElementId::Name(format!("{id}-hover").into()))
        .relative()
        .w_full()
        .h(px(HEIGHT))
        .on_hover(move |entered, window, cx| {
            if !*entered {
                clear(None, window, cx);
            }
        })
        .child(painted)
        .child(div().absolute().inset_0().flex().children(cells))
        .into_any_element()
}

/// A filled circle ringed in the card's own colour, so it reads as a point on
/// the line rather than a blob over it.
fn paint_marker(window: &mut Window, center: gpui::Point<Pixels>, color: Hsla, ring: Hsla) {
    let outer = px(DOT_R + RING);
    window.paint_quad(
        fill(
            Bounds::new(
                point(center.x - outer, center.y - outer),
                size(outer * 2.0, outer * 2.0),
            ),
            ring,
        )
        .corner_radii(outer),
    );
    let inner = px(DOT_R);
    window.paint_quad(
        fill(
            Bounds::new(
                point(center.x - inner, center.y - inner),
                size(inner * 2.0, inner * 2.0),
            ),
            color,
        )
        .corner_radii(inner),
    );
}

/// The recipe's dashed guide: 2 on, 2 off, from half a padding in at each end.
fn paint_crosshair(
    window: &mut Window,
    at: &impl Fn(f32, f32) -> gpui::Point<Pixels>,
    x: f32,
    height: f32,
    theme: Theme,
) {
    let color = theme.text_muted.opacity(0.8);
    let top = PAD / 2.0;
    let bottom = height - PAD / 2.0;
    let mut y = top;
    while y < bottom {
        let dash = 2.0_f32.min(bottom - y);
        window.paint_quad(fill(Bounds::new(at(x, y), size(px(1.0), px(dash))), color));
        y += 4.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_easing_starts_and_ends_exactly() {
        let ease = recipe_ease();
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        // Strongly eased at both ends: slow out, fast middle, soft landing.
        assert!(ease(0.25) < 0.25);
        assert!(ease(0.75) > 0.75);
    }

    #[test]
    fn morph_resamples_a_changed_bucket_count() {
        // Seven daily points collapsing into three days: the old curve keeps
        // its shape at the first and last point instead of snapping.
        let old = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(sample(&old, 0, 3), Some(0.0));
        assert_eq!(sample(&old, 2, 3), Some(6.0));
        assert_eq!(sample(&old, 1, 3), Some(3.0));
        assert_eq!(sample(&old, 3, 3), None);
        // A gap in the old shape stays a gap.
        let gapped = [1.0, f64::NAN, 3.0];
        assert_eq!(sample(&gapped, 1, 3), None);
    }

    #[test]
    fn point_change_reads_the_previous_bucket() {
        assert_eq!(point_delta(&[100.0, 150.0], 1), Some(50.0));
        assert_eq!(point_delta(&[100.0, 50.0], 1), Some(-50.0));
        // No baseline, no percentage — never a made-up infinity.
        assert_eq!(point_delta(&[0.0, 50.0], 1), None);
        assert_eq!(point_delta(&[10.0], 0), None);
        assert_eq!(point_delta(&[10.0, f64::NAN], 1), None);
    }

    #[test]
    fn a_flat_series_still_has_a_domain() {
        let (low, high) = domain(&[4.0, 4.0, 4.0]);
        assert!(low < 4.0 && high > 4.0);
        assert_eq!(domain(&[f64::NAN]), (0.0, 1.0));
        assert_eq!(domain(&[2.0, 8.0]), (2.0, 8.0));
    }

    #[test]
    fn gaps_break_the_line_into_runs() {
        let runs = finite_runs(&[1.0, 2.0, f64::NAN, 4.0, 5.0, 6.0]);
        assert_eq!(runs, vec![0..2, 3..6]);
        assert!(finite_runs(&[f64::NAN, f64::NAN]).is_empty());
    }

    #[test]
    fn cached_shapes_compare_gaps_as_equal() {
        let a = vec![vec![1.0, f64::NAN]];
        let b = vec![vec![1.0, f64::NAN]];
        assert!(same_values(&a, &b));
        assert!(!same_values(&a, &[vec![1.0, 2.0]]));
        assert!(!same_values(&a, &[vec![1.0]]));
    }
}
