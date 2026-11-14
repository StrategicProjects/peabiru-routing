//! Facility location on a table of travel times: where to open facilities
//! ([`locate_facilities`]) and how supply compares with demand around each
//! place ([`floating_catchment`]).
//!
//! Everything here works on a [`CostTable`] (demand point x site ->
//! minutes, a missing pair being unreachable) and never looks at how the
//! table was computed: by a street or a public transport matrix of this
//! crate, or by anything else.
//!
//! The four siting models are solved by heuristics, with no integer
//! programming solver:
//!
//! - maximal covering (Church and ReVelle, 1974): greedy, each site the
//!   one that covers the most demand still uncovered, which reaches at
//!   least `1 - 1/e` of the best coverage possible (Nemhauser, Wolsey and
//!   Fisher, 1978), then vertex substitution;
//! - location set covering (Toregas et al., 1971): greedy by the number
//!   of demand points newly covered, then the sites the others make
//!   redundant are dropped;
//! - p-median (Hakimi, 1964): greedy, then the vertex substitution of
//!   Teitz and Bart (1968);
//! - p-center (Hakimi, 1964): greedy on the largest travel time (ties by
//!   the weighted sum), then vertex substitution on the same pair.
//!
//! Ties are always broken by the lowest site index, the candidates are
//! evaluated in parallel only where each one's value is computed on its
//! own, and every sum is taken in a fixed order: the result does not
//! depend on the number of threads.

mod catchment;
mod cover;
mod median;
mod table;

#[cfg(test)]
mod tests;

pub use catchment::{floating_catchment, FloatingCatchment, FloatingCatchmentParams};
pub use table::CostTable;

use crate::{sys, CancelToken, RouteError};

/// The siting model of [`locate_facilities`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LocationModel {
    /// Maximal covering: with [`LocationParams::n_facilities`] new
    /// facilities, cover the most demand within
    /// [`LocationParams::cutoff`] minutes.
    MaxCover,
    /// Location set covering: the fewest new facilities that leave every
    /// demand point some site can cover within
    /// [`LocationParams::cutoff`] minutes.
    SetCover,
    /// p-median: [`LocationParams::n_facilities`] new facilities with the
    /// smallest demand-weighted sum of travel times to the nearest
    /// facility.
    PMedian,
    /// p-center: [`LocationParams::n_facilities`] new facilities with the
    /// smallest travel time of the demand point farthest from its nearest
    /// facility.
    PCenter,
}

impl LocationModel {
    /// The model's name in the packages: `max_cover`, `set_cover`,
    /// `p_median` or `p_center`.
    pub fn name(self) -> &'static str {
        match self {
            LocationModel::MaxCover => "max_cover",
            LocationModel::SetCover => "set_cover",
            LocationModel::PMedian => "p_median",
            LocationModel::PCenter => "p_center",
        }
    }

    /// The model of that name (`None` for any other text).
    pub fn from_name(name: &str) -> Option<LocationModel> {
        Some(match name {
            "max_cover" => LocationModel::MaxCover,
            "set_cover" => LocationModel::SetCover,
            "p_median" => LocationModel::PMedian,
            "p_center" => LocationModel::PCenter,
            _ => return None,
        })
    }

    fn is_cover(self) -> bool {
        matches!(self, LocationModel::MaxCover | LocationModel::SetCover)
    }
}

/// What [`locate_facilities`] solves. Fill it in by changing the fields of
/// [`LocationParams::default()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct LocationParams {
    /// The model (default: maximal covering).
    pub model: LocationModel,
    /// How many new facilities to open (default 1). Fewer are returned
    /// when no further site improves the objective. Ignored by
    /// [`LocationModel::SetCover`], which opens as many as it takes.
    pub n_facilities: usize,
    /// The covering models' service standard in minutes: a demand point is
    /// covered by a site at most this far (`time <= cutoff`). No default:
    /// it must be set for [`LocationModel::MaxCover`] and
    /// [`LocationModel::SetCover`]; the other models ignore it.
    pub cutoff: f64,
    /// Sites of the table (column indices) that are already open. They
    /// count first: only the demand they leave uncovered (covering
    /// models), or the travel time they leave (p-median, p-center), drives
    /// the choice of the new sites. They are never closed or moved. Every
    /// other site of the table is a candidate.
    pub existing: Vec<usize>,
    /// Improve the greedy solution by vertex substitution (default
    /// `true`): swap an open new site for a closed candidate while that
    /// improves the objective, taking at each step the best swap of all.
    /// For [`LocationModel::SetCover`] the improvement is to drop the
    /// sites the others make redundant. The result is never worse than
    /// the greedy one ([`LocationSolution::objective_greedy`]).
    pub swap: bool,
    /// [`LocationModel::MaxCover`] stops opening sites when the best one
    /// left would cover less than this much uncovered demand (default 0:
    /// it stops only when a site would cover nothing).
    pub min_gain: f64,
    /// For [`LocationModel::PMedian`] and [`LocationModel::PCenter`], the
    /// minutes charged to a demand point that reaches no open facility
    /// (while some site of the table could serve it). `None` (the
    /// default): twice the largest time of the table, so that serving a
    /// point always beats shortening the trip of another.
    pub unreachable_time: Option<f64>,
    /// Worker threads (0 = all cores). The result does not depend on it.
    pub n_threads: usize,
    /// Cancels the computation: checked between candidates.
    pub cancel: CancelToken,
}

impl Default for LocationParams {
    fn default() -> Self {
        LocationParams {
            model: LocationModel::MaxCover,
            n_facilities: 1,
            cutoff: f64::NAN,
            existing: Vec::new(),
            swap: true,
            min_gain: 0.0,
            unreachable_time: None,
            n_threads: 0,
            cancel: CancelToken::default(),
        }
    }
}

/// The sites [`locate_facilities`] chose and what they achieve.
///
/// The *objective* is, by model: the demand covered (maximal covering),
/// the number of new sites (set covering), the demand-weighted sum of
/// minutes to the nearest facility (p-median) and the largest such time
/// (p-center). Only the first is to be maximised.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LocationSolution {
    /// The model solved.
    pub model: LocationModel,
    /// The new sites (column indices of the table), ordered by what each
    /// adds: the first is the site of the solution that does best alone
    /// (after the existing facilities), the second the one that adds most
    /// to it, and so on. Without [`LocationParams::swap`] this is the
    /// order in which the greedy heuristic chose them.
    pub sites: Vec<usize>,
    /// What each site of `sites` adds to the ones before it: demand newly
    /// covered (covering models), or by how much the objective falls
    /// (p-median, p-center).
    pub marginal: Vec<f64>,
    /// The state after each site of `sites`: demand covered so far,
    /// including what the existing facilities cover (covering models), or
    /// the objective so far (p-median, p-center).
    pub cumulative: Vec<f64>,
    /// Per site of the table, what it would add if it were the only new
    /// facility, in the unit of `marginal`: for the covering models, the
    /// demand not covered by the existing facilities that it would cover
    /// alone. 0 for the existing facilities.
    pub potential: Vec<f64>,
    /// Per demand point, the nearest open facility, existing or new
    /// (`None` when it reaches none); ties go to the lowest site index.
    pub assigned_site: Vec<Option<usize>>,
    /// Per demand point, the minutes to `assigned_site` (NaN when `None`).
    pub assigned_minutes: Vec<f64>,
    /// The objective with the existing facilities only. For set covering,
    /// 0.
    pub objective_before: f64,
    /// The objective of the greedy solution, before the improvement.
    pub objective_greedy: f64,
    /// The objective of the solution.
    pub objective: f64,
    /// The demand covered by the existing facilities (covering models; NaN
    /// for the others).
    pub covered_before: f64,
    /// The demand covered by the solution (covering models; NaN for the
    /// others).
    pub covered: f64,
    /// The sum of the demand weights.
    pub total_demand: f64,
    /// The demand points with weight that no site of the table, existing
    /// or candidate, can serve: within the cutoff for the covering models,
    /// at all for p-median and p-center. They are left out of the
    /// objectives.
    pub unservable: Vec<usize>,
    /// The sum of the weights of `unservable`.
    pub unservable_demand: f64,
    /// How many improving swaps were made (set covering: redundant sites
    /// dropped).
    pub n_swaps: usize,
}

/// Choose where to open facilities.
///
/// `costs` gives the minutes from each demand point to each site;
/// `demand` the weight of each demand point (residents, pupils...), which
/// must be finite and not negative. The sites in
/// [`LocationParams::existing`] are already open; all the others are
/// candidates. See [`LocationModel`] for the four models,
/// [`LocationParams`] for what can be set and [`LocationSolution`] for
/// what comes back.
///
/// The models are solved by greedy construction followed by vertex
/// substitution: the solution is good and reproducible, not proven
/// optimal. For maximal covering the greedy coverage of the demand the
/// existing facilities leave uncovered is at least `1 - 1/e` (63%) of the
/// best possible. In set covering and p-center the points with weight 0
/// are not demand; the others count the same whatever their weight.
///
/// Errors: [`RouteError::Invalid`] for weights that do not match the
/// table, a negative or non-finite weight, an index in `existing` outside
/// the table or repeated, no candidate, `n_facilities` of 0 or more than
/// the candidates, or a covering model without a cutoff;
/// [`RouteError::Cancelled`].
pub fn locate_facilities(
    costs: &CostTable,
    demand: &[f64],
    params: &LocationParams,
) -> Result<LocationSolution, RouteError> {
    let invalid = |m: String| Err(RouteError::Invalid(m));
    if demand.len() != costs.n_demand() {
        return invalid(format!(
            "demand has {} weights, but the table has {} demand points",
            demand.len(),
            costs.n_demand()
        ));
    }
    if let Some(i) = demand.iter().position(|w| !(w.is_finite() && *w >= 0.0)) {
        return invalid(format!(
            "demand weights must be finite and not negative, got {} for demand point {i}",
            demand[i]
        ));
    }
    let mut is_existing = vec![false; costs.n_sites()];
    for &e in &params.existing {
        if e >= costs.n_sites() {
            return invalid(format!(
                "existing facility {e} is not a site of the table, which has {}",
                costs.n_sites()
            ));
        }
        if std::mem::replace(&mut is_existing[e], true) {
            return invalid(format!("existing facility {e} is listed more than once"));
        }
    }
    let candidates: Vec<usize> = (0..costs.n_sites()).filter(|&j| !is_existing[j]).collect();
    if candidates.is_empty() {
        return invalid(
            "there is no candidate site: every site of the table is an existing facility".into(),
        );
    }
    if params.model != LocationModel::SetCover {
        if params.n_facilities == 0 {
            return invalid("n_facilities must be at least 1".into());
        }
        if params.n_facilities > candidates.len() {
            return invalid(format!(
                "n_facilities is {}, but there are only {} candidate sites",
                params.n_facilities,
                candidates.len()
            ));
        }
    }
    if params.model.is_cover() {
        if !(params.cutoff.is_finite() && params.cutoff >= 0.0) {
            return invalid(format!(
                "the {} model needs a cutoff in minutes, finite and not negative, got {}",
                params.model.name(),
                params.cutoff
            ));
        }
        if !(params.min_gain.is_finite() && params.min_gain >= 0.0) {
            return invalid(format!(
                "min_gain must be finite and not negative, got {}",
                params.min_gain
            ));
        }
    } else if let Some(u) = params.unreachable_time {
        if !(u.is_finite() && u > 0.0) {
            return invalid(format!(
                "unreachable_time must be finite and positive, got {u}"
            ));
        }
    }
    params.cancel.check()?;
    let problem = Problem {
        costs,
        demand,
        existing: &params.existing,
        candidates: &candidates,
        params,
    };
    let mut solution = if params.model.is_cover() {
        cover::solve(&problem)?
    } else {
        median::solve(&problem)?
    };
    assign(&problem, &mut solution);
    Ok(solution)
}

/// A validated call of [`locate_facilities`].
struct Problem<'a> {
    costs: &'a CostTable,
    demand: &'a [f64],
    existing: &'a [usize],
    /// The sites that are not existing facilities, ascending.
    candidates: &'a [usize],
    params: &'a LocationParams,
}

/// The nearest open facility of each demand point.
fn assign(p: &Problem, s: &mut LocationSolution) {
    let n = p.costs.n_demand();
    let mut site: Vec<Option<usize>> = vec![None; n];
    let mut minutes = vec![f64::NAN; n];
    let mut open: Vec<usize> = p.existing.iter().chain(&s.sites).copied().collect();
    open.sort_unstable();
    for &j in &open {
        let (rows, times) = p.costs.site(j);
        for (&i, &t) in rows.iter().zip(times) {
            let i = i as usize;
            // ascending site index: a strictly shorter time only
            if site[i].is_none() || t < minutes[i] {
                site[i] = Some(j);
                minutes[i] = t;
            }
        }
    }
    s.assigned_site = site;
    s.assigned_minutes = minutes;
}

/// A value to minimise, compared field by field.
type Key = [f64; 2];

fn better(a: &Key, b: &Key) -> bool {
    a[0] < b[0] || (a[0] == b[0] && a[1] < b[1])
}

/// `f(item, scratch)` for every item, in the order of `items`. The items
/// are shared out in contiguous blocks among the workers, each with a
/// scratch value of its own, and every value is computed on its own: the
/// answer is the same for any number of workers.
fn map_of<S: Send, T: Send>(
    items: &[usize],
    n_threads: usize,
    cancel: &CancelToken,
    scratch: impl Fn() -> S + Sync,
    f: impl Fn(usize, &mut S) -> T + Sync,
) -> Result<Vec<T>, RouteError> {
    let workers = if items.len() < 64 {
        1
    } else {
        let cores = sys::cores();
        let asked = if n_threads == 0 {
            cores
        } else {
            n_threads.min(cores)
        };
        asked.clamp(1, items.len())
    };
    let block = items.len().div_ceil(workers).max(1);
    let (scratch, f) = (&scratch, &f);
    let run = move |part: &[usize]| -> Result<Vec<T>, RouteError> {
        let mut s = scratch();
        let mut out = Vec::with_capacity(part.len());
        for (n, &j) in part.iter().enumerate() {
            if n % 64 == 0 {
                cancel.check()?;
            }
            out.push(f(j, &mut s));
        }
        Ok(out)
    };
    if workers == 1 {
        return run(items);
    }
    let mut all = Vec::with_capacity(items.len());
    for part in sys::scoped(items.chunks(block), run) {
        all.extend(part?);
    }
    Ok(all)
}

/// The position of the smallest key, the first of them on a tie (`None`
/// entries are out of the running).
fn first_best<P>(values: &[Option<(Key, P)>]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (k, v) in values.iter().enumerate() {
        if let Some((key, _)) = v {
            let wins = match best.and_then(|b| values[b].as_ref()) {
                Some((b, _)) => better(key, b),
                None => true,
            };
            if wins {
                best = Some(k);
            }
        }
    }
    best
}
