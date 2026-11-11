//! Accessibility to opportunities, as r5r's `accessibility()`: for each
//! origin, the sum over destinations of `opportunities * weight(t)`, where
//! `t` is a travel-time percentile in seconds and `weight` a decay function
//! of `t` and a cutoff (R5 `TravelTimeReducer.recordTravelTimePercentilesForTarget`,
//! `AccessibilityResult`, r5r `AccessibilityEstimator`).
//!
//! Travel times come from the same computation as the travel-time
//! matrices: [`super::matrix`] for walk + transit and
//! [`crate::street::router`] for street-only modes. Unlike the matrices,
//! nothing is rounded to minutes: the decay is applied to the percentile in
//! whole seconds (`TravelTimeReducer.java:217-226`), and the sum is r5r's
//! raw double (no rounding on the R side either).

use super::decay::{Decay, DecayFunction};
use super::matrix::{
    check_percentiles, par_origins, percentile_index, Query, TransitData, TransitParams,
};
use super::raptor::UNREACHED;
use super::TransitLayer;
use crate::street::router::{travel_time_seconds, Mode, RouteParams};
use crate::street::StreetNetwork;
use crate::RouteError;

/// `AnalysisWorkerTask.MAX_REGIONAL_CUTOFFS`.
const MAX_CUTOFFS: usize = 12;

/// How to weight the opportunities, with r5r's defaults (`cutoffs = 30`,
/// `decay_function = "step"`). Fill it in by changing the fields of
/// [`AccessibilityParams::default()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AccessibilityParams {
    /// Cutoffs in whole minutes: 1 to 12 values in 0..=120, in any order
    /// (they are sorted ascending, as r5r does). r5r passes the single
    /// placeholder `0` with [`DecayFunction::FixedExponential`], which
    /// ignores it.
    pub cutoffs: Vec<i32>,
    /// The decay function and its parameter.
    pub decay: DecayFunction,
}

impl Default for AccessibilityParams {
    fn default() -> Self {
        AccessibilityParams {
            cutoffs: vec![30],
            decay: DecayFunction::Step,
        }
    }
}

/// Accessibility per origin, opportunity column, percentile and cutoff:
/// r5r's raw double, unrounded.
///
/// The values are held in one buffer, origin by origin, then opportunity
/// column, percentile and cutoff (the cutoff varies fastest), which
/// [`into_values`](Accessibility::into_values) hands over without copying.
#[derive(Debug, Clone, PartialEq)]
pub struct Accessibility {
    /// Percentiles in output order (ascending).
    percentiles: Vec<i32>,
    /// Cutoffs in minutes, in output order (ascending).
    cutoffs: Vec<i32>,
    n_opportunities: usize,
    /// `[origin][opportunity][percentile][cutoff]`, flattened.
    values: Vec<f64>,
}

impl Accessibility {
    /// Number of origins.
    pub fn n_origins(&self) -> usize {
        match self.per_origin() {
            0 => 0,
            per => self.values.len() / per,
        }
    }

    /// Number of opportunity columns, in the order of the destinations'
    /// columns (see [`Destinations::new`]).
    pub fn n_opportunities(&self) -> usize {
        self.n_opportunities
    }

    /// The percentiles, in output order (ascending).
    pub fn percentiles(&self) -> &[i32] {
        &self.percentiles
    }

    /// The cutoffs in minutes, in output order (ascending). With
    /// [`DecayFunction::FixedExponential`], which ignores cutoffs, whatever
    /// [`AccessibilityParams::cutoffs`] held (r5r passes the placeholder
    /// `0`).
    pub fn cutoffs(&self) -> &[i32] {
        &self.cutoffs
    }

    /// Values per origin: opportunity columns x percentiles x cutoffs.
    pub fn per_origin(&self) -> usize {
        self.n_opportunities * self.percentiles.len() * self.cutoffs.len()
    }

    /// Accessibility of `origin` to opportunity column `opportunity` at the
    /// `percentile`-th percentile and `cutoff`-th cutoff (indices into
    /// [`percentiles`](Accessibility::percentiles) and
    /// [`cutoffs`](Accessibility::cutoffs)). Panics when an index is out of
    /// range.
    pub fn get(&self, origin: usize, opportunity: usize, percentile: usize, cutoff: usize) -> f64 {
        let (np, nc) = (self.percentiles.len(), self.cutoffs.len());
        assert!(
            origin < self.n_origins()
                && opportunity < self.n_opportunities
                && percentile < np
                && cutoff < nc,
            "({origin}, {opportunity}, {percentile}, {cutoff}) is out of range"
        );
        self.values[origin * self.per_origin() + (opportunity * np + percentile) * nc + cutoff]
    }

    /// Every value, origin by origin, then opportunity column, percentile
    /// and cutoff (the cutoff varies fastest).
    pub fn values(&self) -> &[f64] {
        &self.values
    }

    /// Every value, in the order of [`values`](Accessibility::values),
    /// without copying.
    pub fn into_values(self) -> Vec<f64> {
        self.values
    }
}

/// Destination points with their opportunity counts: `opportunities[k][j]`
/// is the count of opportunity kind `k` at `points[j]` (one column per
/// r5r `opportunities_colnames` entry). Made with
/// [`Destinations::new`].
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Destinations<'a> {
    /// `(lat, lon)` degrees.
    pub points: &'a [(f64, f64)],
    /// One column per kind of opportunity, one count per point.
    pub opportunities: &'a [Vec<f64>],
}

impl<'a> Destinations<'a> {
    /// Destination `points` (`(lat, lon)` degrees) and their opportunity
    /// counts, one column per kind of opportunity with one count per
    /// point. The accessibility functions check that every column has one
    /// count per point.
    pub fn new(points: &'a [(f64, f64)], opportunities: &'a [Vec<f64>]) -> Self {
        Destinations {
            points,
            opportunities,
        }
    }
}

/// The validated request: decay, cutoffs in seconds with their zero points,
/// percentiles and opportunity counts (R5 `TravelTimeReducer` constructor).
struct Reducer {
    decay: Decay,
    percentiles: Vec<i32>,
    cutoffs_minutes: Vec<i32>,
    cutoffs_seconds: Vec<i32>,
    zero_points: Vec<i32>,
    /// `[opportunity][destination]`, NaN replaced by 0.
    opportunities: Vec<Vec<f64>>,
    /// Destinations where some opportunity count is positive; the others
    /// cannot change any sum.
    useful: Vec<bool>,
}

impl Reducer {
    fn new(
        percentiles: &[i32],
        a: &AccessibilityParams,
        opportunities: &[Vec<f64>],
        n_destinations: usize,
    ) -> Result<Self, String> {
        // AnalysisWorkerTask.validatePercentiles (r5r sorts them first)
        check_percentiles(percentiles)?;
        let mut percentiles = percentiles.to_vec();
        percentiles.sort_unstable();
        // AnalysisWorkerTask.validateCutoffsMinutes (r5r sorts them first)
        let mut cutoffs = a.cutoffs.clone();
        cutoffs.sort_unstable();
        if cutoffs.is_empty() || cutoffs.len() > MAX_CUTOFFS {
            return Err(format!(
                "between 1 and {MAX_CUTOFFS} cutoffs are allowed, got {}",
                cutoffs.len()
            ));
        }
        if let Some(c) = cutoffs.iter().find(|c| !(0..=120).contains(*c)) {
            return Err(format!("cutoffs must be 0 to 120 minutes, got {c}"));
        }
        if opportunities.is_empty() {
            return Err("at least one opportunity column is required".into());
        }
        let mut cols = Vec::with_capacity(opportunities.len());
        for (k, col) in opportunities.iter().enumerate() {
            if col.len() != n_destinations {
                return Err(format!(
                    "opportunity column {k} has {} values for {n_destinations} destinations",
                    col.len()
                ));
            }
            if let Some(v) = col.iter().find(|v| v.is_infinite()) {
                return Err(format!(
                    "opportunity column {k} has a non-finite value ({v})"
                ));
            }
            // r5r turns NA (and NaN, which R counts as NA) into 0
            // (R/assign.R `assign_opportunities`); R5 would skip a NaN
            // anyway (`!(count > 0)`).
            cols.push(
                col.iter()
                    .map(|&v| if v.is_nan() { 0.0 } else { v })
                    .collect::<Vec<f64>>(),
            );
        }
        let decay = Decay::new(a.decay.clone())?;
        let cutoffs_seconds: Vec<i32> = cutoffs.iter().map(|c| c * 60).collect();
        let zero_points = cutoffs_seconds
            .iter()
            .map(|&c| decay.reaches_zero_at(c))
            .collect();
        let useful = (0..n_destinations)
            .map(|j| cols.iter().any(|c| c[j] > 0.0))
            .collect();
        Ok(Reducer {
            decay,
            percentiles,
            cutoffs_minutes: cutoffs,
            cutoffs_seconds,
            zero_points,
            opportunities: cols,
            useful,
        })
    }

    fn per_origin(&self) -> usize {
        self.opportunities.len() * self.percentiles.len() * self.cutoffs_seconds.len()
    }

    /// `TravelTimeReducer.extractTravelTimePercentilesAndRecord` for one
    /// target: sort the per-iteration seconds, take the percentiles, add the
    /// weighted opportunities into `acc` (`[opportunity][percentile][cutoff]`).
    fn record(&self, target: usize, times: &mut [i32], pct: &mut Vec<i32>, acc: &mut [f64]) {
        if !self.useful[target] {
            return;
        }
        times.sort_unstable();
        let n = times.len();
        pct.clear();
        pct.extend(
            self.percentiles
                .iter()
                .map(|&p| times[percentile_index(p, n).min(n - 1)]),
        );
        let (np, nc) = (self.percentiles.len(), self.cutoffs_seconds.len());
        // TravelTimeReducer.java:250-279
        for k in 0..self.opportunities.len() {
            let count = self.opportunities[k][target];
            // `if (!(opportunityCountAtTarget > 0)) continue;`
            if count.is_nan() || count <= 0.0 {
                continue;
            }
            for (p, &t) in pct.iter().enumerate() {
                if t == UNREACHED {
                    // percentiles are ascending: the rest are UNREACHED too
                    break;
                }
                // backward through the cutoffs, stopping at the first whose
                // zero point the time reaches
                for c in (0..nc).rev() {
                    if t >= self.zero_points[c] {
                        break;
                    }
                    let w = self.decay.weight(self.cutoffs_seconds[c], t);
                    if w > 0.0 {
                        acc[(k * np + p) * nc + c] += count * w;
                    }
                }
            }
        }
    }

    fn finish(self, n_origins: usize, values: Vec<f64>) -> Accessibility {
        debug_assert_eq!(values.len(), n_origins * self.per_origin());
        Accessibility {
            n_opportunities: self.opportunities.len(),
            percentiles: self.percentiles,
            cutoffs: self.cutoffs_minutes,
            values,
        }
    }
}

/// Accessibility by public transport (r5r `accessibility()` with public
/// transport in `mode`; the modes, access and egress as in
/// [`transit_travel_time_matrix`](crate::transit_travel_time_matrix)). Opportunity counts that are zero,
/// negative or NaN contribute nothing (`TravelTimeReducer.java:251-253`).
/// `p.percentiles` are the travel-time percentiles; `p.max_trip_duration`
/// and `p.max_walk_time` must already be what r5r hands to R5 (for the
/// step function r5r sets `max_trip_duration` to the largest cutoff before
/// deriving the walk limit; see [`r5r_trip_limits`]).
///
/// Every origin gets a value for every (opportunity, percentile, cutoff),
/// 0 when nothing is reachable or the origin cannot be linked. Points are
/// `(lat, lon)` degrees. [`RouteError::Invalid`] when the parameters,
/// cutoffs or opportunity columns are invalid, [`RouteError::Cancelled`]
/// when `p.cancel` is cancelled before every origin is done.
pub fn transit_accessibility(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &Destinations,
    p: &TransitParams,
    a: &AccessibilityParams,
) -> Result<Accessibility, RouteError> {
    let r = Reducer::new(
        &p.percentiles,
        a,
        destinations.opportunities,
        destinations.points.len(),
    )?;
    let q = Query::new(net, layer, data, destinations.points, p)?;
    let per = r.per_origin();
    let rows = par_origins(p.n_threads, origins.len(), &p.cancel, |i, scratch| {
        let mut acc = vec![0.0; per];
        let mut pct = Vec::with_capacity(r.percentiles.len());
        // An unlinked origin records nothing: all zeros
        // (TravelTimeComputer.java:313-317).
        q.route_origin(origins[i], scratch, |j, times| {
            r.record(j, times, &mut pct, &mut acc)
        });
        acc
    })?;
    Ok(r.finish(origins.len(), rows.concat()))
}

/// Accessibility by a street mode alone (r5r `accessibility()` with `mode`
/// one of `WALK`, `BICYCLE`, `CAR`): one unvarying travel time per
/// destination, so every percentile gets the same value
/// (`TravelTimeReducer.recordUnvaryingTravelTimeAtTarget`).
/// `params.max_trip_duration_min` is the search limit r5r passes to R5,
/// and `percentiles` are checked as
/// [`validate_percentiles`](crate::validate_percentiles) does.
/// Errors as [`transit_accessibility`].
pub fn street_accessibility(
    net: &StreetNetwork,
    origins: &[(f64, f64)],
    destinations: &Destinations,
    mode: Mode,
    params: &RouteParams,
    percentiles: &[i32],
    a: &AccessibilityParams,
) -> Result<Accessibility, RouteError> {
    let r = Reducer::new(
        percentiles,
        a,
        destinations.opportunities,
        destinations.points.len(),
    )?;
    let per = r.per_origin();
    let rows = travel_time_seconds(
        net,
        origins,
        destinations.points,
        mode,
        params,
        |_, times| {
            let mut acc = vec![0.0; per];
            if let Some(times) = times {
                let mut pct = Vec::with_capacity(r.percentiles.len());
                let mut one = [0i32];
                for (j, &t) in times.iter().enumerate() {
                    one[0] = t;
                    r.record(j, &mut one, &mut pct, &mut acc);
                }
            }
            acc
        },
    )?;
    Ok(r.finish(origins.len(), rows.concat()))
}

/// r5r's trip limits for `accessibility()` (`R/accessibility.R:201-231`,
/// `R/assign.R` `assign_max_street_time` / `assign_max_trip_duration`).
/// Given the user's `max_trip_duration` and `max_walk_time` (`None` = `Inf`)
/// in minutes, returns `(max_trip_duration, max_walk_time)` as R5 receives
/// them:
/// - `max_trip_duration` may not be below the largest cutoff (an error);
/// - with the `step` function it becomes the largest cutoff;
/// - `max_walk_time = Inf` then becomes that `max_trip_duration`;
/// - for walking alone (`walk_only`, no transit) `max_trip_duration` is
///   reduced to `max_walk_time`. (Bicycle and car alone have their own
///   limits, which r5r also defaults to `max_trip_duration`.)
pub fn r5r_trip_limits(
    decay: &DecayFunction,
    cutoffs: Option<&[i32]>,
    max_trip_duration: i32,
    max_walk_time: Option<i32>,
    walk_only: bool,
) -> Result<(i32, i32), RouteError> {
    if max_trip_duration < 1 {
        return Err(RouteError::Invalid(format!(
            "max_trip_duration must be at least 1, got {max_trip_duration}"
        )));
    }
    let mut m = max_trip_duration;
    if let Some(cutoffs) = cutoffs {
        let top = cutoffs.iter().copied().max().unwrap_or(0);
        if m < top {
            return Err("'max_trip_duration' cannot be shorter than 'max(cutoffs)'".into());
        }
        if *decay == DecayFunction::Step {
            m = top;
        }
    }
    let w = match max_walk_time {
        Some(w) if w >= 1 => w,
        Some(w) => {
            return Err(RouteError::Invalid(format!(
                "max_walk_time must be at least 1, got {w}"
            )))
        }
        None => m,
    };
    if walk_only && w < m {
        m = w;
    }
    Ok((m, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(decay: DecayFunction, cutoffs: &[i32]) -> AccessibilityParams {
        AccessibilityParams {
            cutoffs: cutoffs.to_vec(),
            decay,
        }
    }

    #[test]
    fn step_reduction_counts_strictly_below_each_cutoff() {
        let opp = vec![vec![1.0, 2.0, 4.0, 8.0, 0.0, -5.0]];
        let a = params(DecayFunction::Step, &[30, 10]);
        let r = Reducer::new(&[50], &a, &opp, 6).unwrap();
        assert_eq!(r.cutoffs_minutes, vec![10, 30]);
        let mut acc = vec![0.0; r.per_origin()];
        let mut pct = Vec::new();
        for (j, t) in [0, 599, 600, 1799, 100, 100].into_iter().enumerate() {
            r.record(j, &mut [t], &mut pct, &mut acc);
        }
        // cutoff 10 min: t < 600 -> 1 + 2; 30 min: + 4 + 8; zero and
        // negative counts never count
        assert_eq!(acc, vec![3.0, 15.0]);
    }

    #[test]
    fn percentiles_use_r5_indices_and_stop_at_unreached() {
        let opp = vec![vec![1.0]];
        let a = params(DecayFunction::Step, &[60]);
        let r = Reducer::new(&[95, 5, 50], &a, &opp, 1).unwrap();
        assert_eq!(r.percentiles, vec![5, 50, 95]);
        let mut acc = vec![0.0; r.per_origin()];
        let mut pct = Vec::new();
        // 10 iterations: p5 -> index 0, p50 -> 4, p95 -> 9 (UNREACHED)
        let mut t = [100, 200, 300, 400, 3599, 3600, 3700, 3800, 3900, UNREACHED];
        r.record(0, &mut t, &mut pct, &mut acc);
        assert_eq!(pct, vec![100, 3599, UNREACHED]);
        assert_eq!(acc, vec![1.0, 1.0, 0.0]);
    }

    #[test]
    fn accumulation_is_in_destination_order() {
        // 0.1 + 0.2 + 0.3 in this order, as Java's += would
        let opp = vec![vec![0.1, 0.2, 0.3]];
        let a = params(DecayFunction::Step, &[1]);
        let r = Reducer::new(&[50], &a, &opp, 3).unwrap();
        let mut acc = vec![0.0; 1];
        let mut pct = Vec::new();
        for j in 0..3 {
            r.record(j, &mut [0], &mut pct, &mut acc);
        }
        assert_eq!(acc[0], 0.1 + 0.2 + 0.3);
    }

    #[test]
    fn invalid_requests_are_rejected() {
        let opp = vec![vec![1.0; 3]];
        let step = |c: &[i32]| params(DecayFunction::Step, c);
        assert!(Reducer::new(&[50], &step(&[]), &opp, 3).is_err());
        assert!(Reducer::new(&[50], &step(&[121]), &opp, 3).is_err());
        assert!(Reducer::new(&[50], &step(&[-1]), &opp, 3).is_err());
        assert!(Reducer::new(&[50], &step(&[1; 13]), &opp, 3).is_err());
        assert!(Reducer::new(&[], &step(&[30]), &opp, 3).is_err());
        assert!(Reducer::new(&[0], &step(&[30]), &opp, 3).is_err());
        assert!(Reducer::new(&[100], &step(&[30]), &opp, 3).is_err());
        assert!(Reducer::new(&[1, 2, 3, 4, 5, 6], &step(&[30]), &opp, 3).is_err());
        assert!(Reducer::new(&[50], &step(&[30]), &opp, 4).is_err());
        assert!(Reducer::new(&[50], &step(&[30]), &[], 3).is_err());
        assert!(Reducer::new(&[50], &step(&[30]), &[vec![f64::INFINITY; 3]], 3).is_err());
        let bad = params(DecayFunction::Linear { width_minutes: 60 }, &[30]);
        assert!(Reducer::new(&[50], &bad, &opp, 3).is_err());
        // NaN counts as 0
        let r = Reducer::new(&[50], &step(&[30]), &[vec![f64::NAN, 1.0, 0.0]], 3).unwrap();
        assert_eq!(r.useful, vec![false, true, false]);
        assert_eq!(r.opportunities[0][0], 0.0);
    }

    #[test]
    fn trip_limits_follow_r5r() {
        let step = DecayFunction::Step;
        let exp = DecayFunction::Exponential;
        assert_eq!(
            r5r_trip_limits(&step, Some(&[30, 60]), 120, None, false),
            Ok((60, 60))
        );
        assert_eq!(
            r5r_trip_limits(&exp, Some(&[30, 60]), 120, None, false),
            Ok((120, 120))
        );
        assert_eq!(
            r5r_trip_limits(&exp, Some(&[30, 60]), 120, Some(15), false),
            Ok((120, 15))
        );
        assert_eq!(
            r5r_trip_limits(&exp, Some(&[30, 60]), 120, Some(15), true),
            Ok((15, 15))
        );
        assert!(r5r_trip_limits(&exp, Some(&[30, 60]), 45, None, false).is_err());
        let fixed = DecayFunction::FixedExponential {
            decay_constant: 0.01,
        };
        assert_eq!(r5r_trip_limits(&fixed, None, 90, None, false), Ok((90, 90)));
    }
}
