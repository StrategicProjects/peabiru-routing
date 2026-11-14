//! Tests of the location models: instances solved by hand, and random
//! small instances against an exhaustive search.

#![allow(clippy::field_reassign_with_default)]

use super::*;
use crate::DecayFunction;

/// splitmix64: the tests' own random numbers.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// A whole number in `0..n`.
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random instance: times (`None` = unreachable), weights, existing
/// facilities.
struct Instance {
    n: usize,
    m: usize,
    times: Vec<Option<f64>>,
    demand: Vec<f64>,
    existing: Vec<usize>,
}

impl Instance {
    fn random(rng: &mut Rng, n: usize, m: usize, n_existing: usize) -> Instance {
        let times = (0..n * m)
            .map(|_| (rng.below(4) != 0).then(|| 1.0 + rng.below(30) as f64))
            .collect();
        let demand = (0..n).map(|_| rng.below(10) as f64).collect();
        let mut existing = Vec::new();
        while existing.len() < n_existing {
            let e = rng.below(m as u64) as usize;
            if !existing.contains(&e) {
                existing.push(e);
            }
        }
        Instance {
            n,
            m,
            times,
            demand,
            existing,
        }
    }

    fn table(&self) -> CostTable {
        let dense: Vec<f64> = self.times.iter().map(|t| t.unwrap_or(f64::NAN)).collect();
        CostTable::from_dense(self.n, self.m, &dense).unwrap()
    }

    fn t(&self, i: usize, j: usize) -> Option<f64> {
        self.times[i * self.m + j]
    }

    fn candidates(&self) -> Vec<usize> {
        (0..self.m).filter(|j| !self.existing.contains(j)).collect()
    }

    fn open(&self, new: &[usize]) -> Vec<usize> {
        self.existing.iter().chain(new).copied().collect()
    }

    /// Demand within `cutoff` of a facility of `new` or an existing one.
    fn covered(&self, new: &[usize], cutoff: f64) -> f64 {
        let open = self.open(new);
        (0..self.n)
            .filter(|&i| {
                open.iter()
                    .any(|&j| self.t(i, j).is_some_and(|t| t <= cutoff))
            })
            .map(|i| self.demand[i])
            .sum()
    }

    /// Whether every demand point with weight that some site covers is
    /// covered.
    fn covers_all(&self, new: &[usize], cutoff: f64) -> bool {
        let open = self.open(new);
        let within = |i: usize, sites: &[usize]| {
            sites
                .iter()
                .any(|&j| self.t(i, j).is_some_and(|t| t <= cutoff))
        };
        let all: Vec<usize> = (0..self.m).collect();
        (0..self.n).all(|i| self.demand[i] == 0.0 || !within(i, &all) || within(i, &open))
    }

    /// The weighted sum and the largest time to the nearest facility,
    /// over the points with weight that some site reaches.
    fn sum_and_max(&self, new: &[usize], unreachable: f64) -> (f64, f64) {
        let open = self.open(new);
        let (mut sum, mut max) = (0f64, 0f64);
        for i in 0..self.n {
            if self.demand[i] == 0.0 || (0..self.m).all(|j| self.t(i, j).is_none()) {
                continue;
            }
            let t = open
                .iter()
                .filter_map(|&j| self.t(i, j))
                .fold(unreachable, f64::min);
            sum += self.demand[i] * t;
            max = max.max(t);
        }
        (sum, max)
    }

    fn unreachable(&self) -> f64 {
        2.0 * self.times.iter().flatten().fold(0f64, |a, &b| a.max(b))
    }
}

/// Every subset of `k` of `items`.
fn subsets(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    if k == 0 {
        return vec![Vec::new()];
    }
    let mut out = Vec::new();
    for (at, &first) in items.iter().enumerate() {
        for mut rest in subsets(&items[at + 1..], k - 1) {
            rest.insert(0, first);
            out.push(rest);
        }
    }
    out
}

fn params(model: LocationModel, p: usize, cutoff: f64, existing: &[usize]) -> LocationParams {
    let mut q = LocationParams::default();
    q.model = model;
    q.n_facilities = p;
    q.cutoff = cutoff;
    q.existing = existing.to_vec();
    q.n_threads = 1;
    q
}

#[test]
fn a_table_holds_the_reachable_pairs() {
    let t = CostTable::from_pairs(3, 2, &[2, 0, 1], &[1, 1, 0], &[4.0, 2.5, f64::NAN]).unwrap();
    assert_eq!((t.n_demand(), t.n_sites(), t.n_pairs()), (3, 2, 2));
    assert_eq!(t.get(0, 1), Some(2.5));
    assert_eq!(t.get(2, 1), Some(4.0));
    assert_eq!(t.get(1, 0), None);
    assert_eq!(t.max_minutes(), 4.0);
    assert_eq!(t.site(1), (&[0u32, 2][..], &[2.5, 4.0][..]));
    let d = CostTable::from_dense(
        3,
        2,
        &[f64::NAN, 2.5, f64::INFINITY, f64::NAN, f64::NAN, 4.0],
    )
    .unwrap();
    assert_eq!(d, t);

    let m = CostTable::from_minutes(3, 2, &[None, Some(2), None, None, None, Some(4)]).unwrap();
    assert_eq!(
        (m.get(0, 1), m.get(2, 1), m.n_pairs()),
        (Some(2.0), Some(4.0), 2)
    );
    assert!(CostTable::from_minutes(3, 2, &[None; 5]).is_err());

    let bad = |r: Result<CostTable, RouteError>, what: &str| match r {
        Err(RouteError::Invalid(m)) => assert!(m.contains(what), "{m}"),
        other => panic!("{other:?}"),
    };
    bad(
        CostTable::from_pairs(1, 1, &[0], &[0, 0], &[1.0]),
        "as many",
    );
    bad(
        CostTable::from_pairs(1, 1, &[1], &[0], &[1.0]),
        "demand point 1",
    );
    bad(CostTable::from_pairs(1, 1, &[0], &[3], &[1.0]), "site 3");
    bad(CostTable::from_pairs(1, 1, &[0], &[0], &[-1.0]), "negative");
    bad(
        CostTable::from_pairs(1, 1, &[0, 0], &[0, 0], &[1.0, 2.0]),
        "more than once",
    );
    bad(CostTable::from_dense(2, 2, &[1.0; 3]), "needs 4 times");
}

/// Six demand points of weight 1; site 0 covers four of them, sites 1 and
/// 2 three each and all six together.
fn greedy_trap() -> CostTable {
    let mut dense = vec![f64::NAN; 6 * 3];
    for i in [0, 1, 2, 3] {
        dense[i * 3] = 5.0;
    }
    for i in [0, 1, 4] {
        dense[i * 3 + 1] = 5.0;
    }
    for i in [2, 3, 5] {
        dense[i * 3 + 2] = 5.0;
    }
    CostTable::from_dense(6, 3, &dense).unwrap()
}

#[test]
fn max_cover_by_hand() {
    let table = greedy_trap();
    let demand = [1.0; 6];
    let mut q = params(LocationModel::MaxCover, 2, 10.0, &[]);
    q.swap = false;
    let greedy = locate_facilities(&table, &demand, &q).unwrap();
    assert_eq!(greedy.potential, [4.0, 3.0, 3.0]);
    assert_eq!(greedy.sites, [0, 1]);
    assert_eq!(greedy.marginal, [4.0, 1.0]);
    assert_eq!(greedy.cumulative, [4.0, 5.0]);
    assert_eq!((greedy.objective, greedy.n_swaps), (5.0, 0));
    assert_eq!(greedy.total_demand, 6.0);

    q.swap = true;
    let swapped = locate_facilities(&table, &demand, &q).unwrap();
    assert_eq!(swapped.sites, [1, 2]);
    assert_eq!(swapped.marginal, [3.0, 3.0]);
    assert_eq!(swapped.cumulative, [3.0, 6.0]);
    assert_eq!(
        (
            swapped.objective_before,
            swapped.objective_greedy,
            swapped.objective
        ),
        (0.0, 5.0, 6.0)
    );
    assert_eq!(swapped.n_swaps, 1);
    assert_eq!(
        swapped.assigned_site,
        [Some(1), Some(1), Some(2), Some(2), Some(1), Some(2)]
    );
    assert!(swapped.unservable.is_empty());

    // the cutoff is inclusive, and a tighter one covers nothing
    q.cutoff = 5.0;
    assert_eq!(
        locate_facilities(&table, &demand, &q).unwrap().objective,
        6.0
    );
    q.cutoff = 4.9;
    let none = locate_facilities(&table, &demand, &q).unwrap();
    assert!(none.sites.is_empty());
    assert_eq!(none.unservable, [0, 1, 2, 3, 4, 5]);
    assert_eq!(none.unservable_demand, 6.0);
    // zero, not the negative zero of an empty sum
    assert!(none.objective.is_sign_positive() && none.covered_before.is_sign_positive());

    // with a floor on the gain the second greedy site is not worth it
    q.cutoff = 10.0;
    q.swap = false;
    q.min_gain = 2.0;
    assert_eq!(locate_facilities(&table, &demand, &q).unwrap().sites, [0]);
}

#[test]
fn existing_facilities_count_first() {
    let table = greedy_trap();
    let demand = [1.0, 1.0, 1.0, 1.0, 7.0, 2.0];
    // site 0 is open: only points 4 and 5 are left to cover
    let q = params(LocationModel::MaxCover, 1, 10.0, &[0]);
    let s = locate_facilities(&table, &demand, &q).unwrap();
    assert_eq!(s.potential, [0.0, 7.0, 2.0]);
    assert_eq!(s.sites, [1]);
    assert_eq!((s.covered_before, s.covered), (4.0, 11.0));
    assert_eq!(s.cumulative, [11.0]);
    assert_eq!(s.assigned_site[0], Some(0));

    let q = params(LocationModel::SetCover, 0, 10.0, &[0]);
    let s = locate_facilities(&table, &demand, &q).unwrap();
    assert_eq!(s.sites, [1, 2]);
    assert_eq!(s.objective, 2.0);
    assert_eq!(s.marginal, [7.0, 2.0]);
}

#[test]
fn set_cover_drops_redundant_sites() {
    // greedy takes site 0 (4 points), then 1 and 2 for the last two,
    // which together make site 0 redundant
    let table = greedy_trap();
    let demand = [1.0; 6];
    let mut q = params(LocationModel::SetCover, 0, 10.0, &[]);
    q.swap = false;
    let greedy = locate_facilities(&table, &demand, &q).unwrap();
    assert_eq!(greedy.sites, [0, 1, 2]);
    assert_eq!(greedy.objective, 3.0);
    q.swap = true;
    let s = locate_facilities(&table, &demand, &q).unwrap();
    assert_eq!(s.sites, [1, 2]);
    assert_eq!(
        (s.objective_greedy, s.objective, s.n_swaps, s.covered),
        (3.0, 2.0, 1, 6.0)
    );
}

#[test]
fn median_and_center_by_hand() {
    // demand points on a line at 0, 1, 2, 10, 11 (weights 1, 1, 1, 1, 5),
    // candidate sites at each of them, travel time = distance
    let at = [0.0f64, 1.0, 2.0, 10.0, 11.0];
    let demand = [1.0, 1.0, 1.0, 1.0, 5.0];
    let mut dense = Vec::new();
    for a in at {
        for b in at {
            dense.push((a - b).abs());
        }
    }
    let table = CostTable::from_dense(5, 5, &dense).unwrap();

    // 1-median: the weighted median is the point at 11
    let s = locate_facilities(
        &table,
        &demand,
        &params(LocationModel::PMedian, 1, 0.0, &[]),
    )
    .unwrap();
    assert_eq!(s.sites, [4]);
    assert_eq!(s.objective, 11.0 + 10.0 + 9.0 + 1.0);
    // nothing open before: every point is charged twice the largest time
    assert_eq!(s.objective_before, 9.0 * 22.0);
    // 2-median: 1 and 11
    let s = locate_facilities(
        &table,
        &demand,
        &params(LocationModel::PMedian, 2, 0.0, &[]),
    )
    .unwrap();
    assert_eq!(s.sites, [4, 1]);
    assert_eq!(s.objective, 3.0);
    assert_eq!(s.cumulative, [31.0, 3.0]);
    assert_eq!(s.marginal, [198.0 - 31.0, 28.0]);
    assert_eq!(
        s.assigned_site,
        [Some(1), Some(1), Some(1), Some(4), Some(4)]
    );
    assert_eq!(s.assigned_minutes, [1.0, 0.0, 1.0, 1.0, 0.0]);

    // 1-center: the point at 2 (largest time 9), whatever the weights
    let s = locate_facilities(
        &table,
        &demand,
        &params(LocationModel::PCenter, 1, 0.0, &[]),
    )
    .unwrap();
    assert_eq!((s.sites.clone(), s.objective), (vec![2], 9.0));
    // 2-center: 1 with 10 or 11 (largest time 1); greedy from 2 alone
    // reaches it only by substitution
    let s = locate_facilities(
        &table,
        &demand,
        &params(LocationModel::PCenter, 2, 0.0, &[]),
    )
    .unwrap();
    assert_eq!(s.objective, 1.0);
    assert!(s.objective_greedy > 1.0);
    assert!(s.n_swaps >= 1);
    let mut sites = s.sites.clone();
    sites.sort_unstable();
    assert_eq!(sites, [1, 4]);

    // with the point at 0 open, one more site: at 11 for the median
    let s = locate_facilities(
        &table,
        &demand,
        &params(LocationModel::PMedian, 1, 0.0, &[0]),
    )
    .unwrap();
    assert_eq!(s.sites, [4]);
    assert_eq!(s.objective_before, 1.0 + 2.0 + 10.0 + 55.0);
    assert_eq!(s.objective, 1.0 + 2.0 + 1.0);
    assert_eq!(s.potential[0], 0.0);
}

#[test]
fn unservable_demand_is_reported_and_left_out() {
    // point 2 reaches no site; point 3 has no weight
    let dense = [1.0, 4.0, 3.0, 1.0, f64::NAN, f64::NAN, 9.0, 9.0];
    let table = CostTable::from_dense(4, 2, &dense).unwrap();
    let demand = [2.0, 1.0, 5.0, 0.0];
    for model in [LocationModel::PMedian, LocationModel::PCenter] {
        let s = locate_facilities(&table, &demand, &params(model, 1, 0.0, &[])).unwrap();
        assert_eq!(s.unservable, [2]);
        assert_eq!(s.unservable_demand, 5.0);
        assert_eq!(s.assigned_site[2], None);
        assert!(s.assigned_minutes[2].is_nan());
        match model {
            LocationModel::PMedian => assert_eq!((s.sites.clone(), s.objective), (vec![0], 5.0)),
            _ => assert_eq!((s.sites.clone(), s.objective), (vec![0], 3.0)),
        }
    }
    let s = locate_facilities(
        &table,
        &demand,
        &params(LocationModel::SetCover, 0, 3.0, &[]),
    )
    .unwrap();
    assert_eq!((s.sites.clone(), s.unservable.clone()), (vec![0], vec![2]));
}

#[test]
fn random_instances_against_exhaustive_search() {
    let mut rng = Rng(2026);
    let bound = 1.0 - (-1f64).exp();
    // per model: instances where greedy and the improved solution are
    // optimal, and the worst ratio to the optimum
    let mut tally = [[0usize; 2]; 3];
    let mut worst_cover = 1f64;
    let rounds = 300;
    for round in 0..rounds {
        let n = 4 + rng.below(11) as usize;
        let m = 3 + rng.below(10) as usize; // at most 12 sites
        let n_existing = (rng.below(3) as usize).min(m - 3);
        let inst = Instance::random(&mut rng, n, m, n_existing);
        let table = inst.table();
        let cands = inst.candidates();
        let p = 1 + rng.below(3.min(cands.len() as u64)) as usize;
        let cutoff = 3.0 + rng.below(25) as f64;
        let what = format!("round {round}: n {n}, m {m}, p {p}, cutoff {cutoff}");

        // maximal covering
        let optimum = subsets(&cands, p)
            .iter()
            .map(|s| inst.covered(s, cutoff))
            .fold(0f64, f64::max);
        let before = inst.covered(&[], cutoff);
        let mut q = params(LocationModel::MaxCover, p, cutoff, &inst.existing);
        q.swap = false;
        let greedy = locate_facilities(&table, &inst.demand, &q).unwrap();
        q.swap = true;
        let s = locate_facilities(&table, &inst.demand, &q).unwrap();
        assert_eq!(
            greedy.objective,
            inst.covered(&greedy.sites, cutoff),
            "{what}"
        );
        assert_eq!(s.objective, inst.covered(&s.sites, cutoff), "{what}");
        assert_eq!(s.objective_before, before, "{what}");
        assert_eq!(s.objective_greedy, greedy.objective, "{what}");
        assert!(s.objective >= greedy.objective, "{what}");
        assert!(s.objective <= optimum, "{what}");
        assert!(
            greedy.objective - before >= bound * (optimum - before) - 1e-9,
            "{what}: greedy {} of {optimum} from {before}",
            greedy.objective
        );
        if p == 1 {
            assert_eq!(s.objective, optimum, "{what}");
        }
        tally[0][0] += usize::from(greedy.objective == optimum);
        tally[0][1] += usize::from(s.objective == optimum);
        if optimum > before {
            worst_cover = worst_cover.min((greedy.objective - before) / (optimum - before));
        }
        assert_eq!(s.cumulative.last().copied().unwrap_or(before), s.objective);
        assert!(s.sites.iter().all(|j| cands.contains(j)), "{what}");
        for &j in &cands {
            assert_eq!(
                s.potential[j],
                inst.covered(&[j], cutoff) - before,
                "{what}"
            );
        }

        // set covering
        let q = params(LocationModel::SetCover, 0, cutoff, &inst.existing);
        let s = locate_facilities(&table, &inst.demand, &q).unwrap();
        assert!(inst.covers_all(&s.sites, cutoff), "{what}");
        let fewest = (0..=cands.len())
            .find(|&k| {
                subsets(&cands, k)
                    .iter()
                    .any(|c| inst.covers_all(c, cutoff))
            })
            .unwrap();
        assert!(s.sites.len() >= fewest, "{what}");
        assert!(s.objective <= s.objective_greedy, "{what}");
        assert_eq!(s.objective, s.sites.len() as f64, "{what}");
        if fewest <= 1 {
            assert_eq!(s.sites.len(), fewest, "{what}");
        }
        let unservable: Vec<usize> = (0..n)
            .filter(|&i| {
                inst.demand[i] > 0.0 && (0..m).all(|j| !inst.t(i, j).is_some_and(|t| t <= cutoff))
            })
            .collect();
        assert_eq!(s.unservable, unservable, "{what}");

        // p-median and p-center
        let u = inst.unreachable();
        for model in [LocationModel::PMedian, LocationModel::PCenter] {
            let of = |sites: &[usize]| {
                let (sum, max) = inst.sum_and_max(sites, u);
                if model == LocationModel::PMedian {
                    sum
                } else {
                    max
                }
            };
            let optimum = (0..=p)
                .flat_map(|k| subsets(&cands, k))
                .map(|c| of(&c))
                .fold(f64::INFINITY, f64::min);
            let mut q = params(model, p, 0.0, &inst.existing);
            q.swap = false;
            let greedy = locate_facilities(&table, &inst.demand, &q).unwrap();
            q.swap = true;
            let s = locate_facilities(&table, &inst.demand, &q).unwrap();
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * (1.0 + b.abs());
            assert!(close(greedy.objective, of(&greedy.sites)), "{what}");
            assert!(close(s.objective, of(&s.sites)), "{what} {model:?}");
            assert!(close(s.objective_before, of(&[])), "{what}");
            assert_eq!(s.objective_greedy, greedy.objective, "{what}");
            assert!(s.objective <= greedy.objective, "{what}");
            assert!(s.objective >= optimum - 1e-9, "{what}");
            if p == 1 {
                assert!(close(s.objective, optimum), "{what} {model:?}");
            }
            let k = if model == LocationModel::PMedian {
                1
            } else {
                2
            };
            tally[k][0] += usize::from(close(greedy.objective, optimum));
            tally[k][1] += usize::from(close(s.objective, optimum));
            assert_eq!(
                s.cumulative.last().copied().unwrap_or(s.objective_before),
                s.objective,
                "{what}"
            );
            // each point is assigned to its nearest open facility
            let open = inst.open(&s.sites);
            for i in 0..n {
                let nearest = open
                    .iter()
                    .filter_map(|&j| inst.t(i, j))
                    .fold(f64::INFINITY, f64::min);
                match s.assigned_site[i] {
                    Some(j) => {
                        assert_eq!(inst.t(i, j), Some(nearest), "{what}");
                        assert_eq!(s.assigned_minutes[i], nearest, "{what}");
                    }
                    None => assert!(nearest.is_infinite(), "{what}"),
                }
            }
        }
    }
    println!(
        "{rounds} instances, optimal greedy / improved: max_cover {:?}, p_median {:?}, p_center {:?}; worst greedy share of the best new coverage {worst_cover:.4}",
        tally[0], tally[1], tally[2]
    );
    // the improvement finds the optimum more often than greedy alone
    for t in tally {
        assert!(t[1] >= t[0] && t[1] * 10 >= rounds * 9, "{tally:?}");
    }
}

#[test]
fn the_result_does_not_depend_on_the_threads() {
    let mut rng = Rng(7);
    let inst = Instance::random(&mut rng, 400, 220, 3);
    let table = inst.table();
    for model in [
        LocationModel::MaxCover,
        LocationModel::SetCover,
        LocationModel::PMedian,
        LocationModel::PCenter,
    ] {
        let mut q = params(model, 6, 4.0, &inst.existing);
        let one = locate_facilities(&table, &inst.demand, &q).unwrap();
        assert!(!one.sites.is_empty(), "{model:?}");
        for threads in [2, 3, 4, 0] {
            q.n_threads = threads;
            let many = locate_facilities(&table, &inst.demand, &q).unwrap();
            // NaN fields (covered, for the travel-time models) compare
            // unequal: compare the text
            assert_eq!(
                format!("{one:?}"),
                format!("{many:?}"),
                "{model:?} {threads}"
            );
        }
    }
}

#[test]
fn ties_go_to_the_lowest_site() {
    // four identical sites
    let table = CostTable::from_dense(2, 4, &[3.0; 8]).unwrap();
    for model in [
        LocationModel::MaxCover,
        LocationModel::SetCover,
        LocationModel::PMedian,
        LocationModel::PCenter,
    ] {
        let s = locate_facilities(&table, &[1.0, 1.0], &params(model, 2, 5.0, &[])).unwrap();
        assert_eq!(s.sites, [0], "{model:?}");
        let s = locate_facilities(&table, &[1.0, 1.0], &params(model, 2, 5.0, &[0])).unwrap();
        assert!(s.sites.is_empty(), "{model:?}");
    }
}

#[test]
fn a_cancelled_call_returns_cancelled() {
    let mut rng = Rng(1);
    let inst = Instance::random(&mut rng, 50, 80, 0);
    let table = inst.table();
    for model in [
        LocationModel::MaxCover,
        LocationModel::SetCover,
        LocationModel::PMedian,
        LocationModel::PCenter,
    ] {
        let q = params(model, 2, 10.0, &[]);
        q.cancel.cancel();
        assert_eq!(
            locate_facilities(&table, &inst.demand, &q),
            Err(RouteError::Cancelled)
        );
    }
    let mut f = FloatingCatchmentParams::default();
    f.cutoff = 10.0;
    f.cancel.cancel();
    assert_eq!(
        floating_catchment(&table, &inst.demand, &[1.0; 80], &f),
        Err(RouteError::Cancelled)
    );
}

#[test]
fn bad_arguments_are_refused() {
    let table = greedy_trap();
    let demand = [1.0; 6];
    let refused =
        |q: &LocationParams, demand: &[f64], what: &str| match locate_facilities(&table, demand, q)
        {
            Err(RouteError::Invalid(m)) => assert!(m.contains(what), "{m}"),
            other => panic!("{what}: {other:?}"),
        };
    let ok = params(LocationModel::MaxCover, 1, 10.0, &[]);
    refused(&ok, &[1.0; 5], "5 weights");
    refused(&ok, &[1.0, 1.0, -1.0, 1.0, 1.0, 1.0], "not negative");
    refused(&ok, &[1.0, 1.0, f64::NAN, 1.0, 1.0, 1.0], "finite");
    refused(
        &params(LocationModel::MaxCover, 1, 10.0, &[3]),
        &demand,
        "not a site",
    );
    refused(
        &params(LocationModel::MaxCover, 1, 10.0, &[1, 1]),
        &demand,
        "more than once",
    );
    refused(
        &params(LocationModel::MaxCover, 1, 10.0, &[0, 1, 2]),
        &demand,
        "no candidate",
    );
    refused(
        &params(LocationModel::PMedian, 0, 10.0, &[]),
        &demand,
        "at least 1",
    );
    refused(
        &params(LocationModel::PCenter, 3, 10.0, &[0]),
        &demand,
        "only 2 candidate",
    );
    refused(
        &params(LocationModel::MaxCover, 1, f64::NAN, &[]),
        &demand,
        "needs a cutoff",
    );
    refused(
        &params(LocationModel::SetCover, 1, -1.0, &[]),
        &demand,
        "needs a cutoff",
    );
    let mut q = ok.clone();
    q.min_gain = -1.0;
    refused(&q, &demand, "min_gain");
    let mut q = params(LocationModel::PMedian, 1, 0.0, &[]);
    q.unreachable_time = Some(0.0);
    refused(&q, &demand, "unreachable_time");
    // set covering takes any n_facilities
    assert!(locate_facilities(
        &table,
        &demand,
        &params(LocationModel::SetCover, 99, 10.0, &[])
    )
    .is_ok());
    for model in [
        LocationModel::MaxCover,
        LocationModel::SetCover,
        LocationModel::PMedian,
        LocationModel::PCenter,
    ] {
        assert_eq!(LocationModel::from_name(model.name()), Some(model));
    }
    assert_eq!(LocationModel::from_name("median"), None);
}

#[test]
fn floating_catchment_by_hand() {
    // two demand points (100 and 200 residents), two facilities (10 and
    // 30 places); minutes: d0-f0 5, d0-f1 20, d1-f0 12, d1-f1 8
    let table = CostTable::from_dense(2, 2, &[5.0, 20.0, 12.0, 8.0]).unwrap();
    let (demand, supply) = ([100.0, 200.0], [10.0, 30.0]);
    let mut q = FloatingCatchmentParams::default();
    q.cutoff = 15.0;
    let f = floating_catchment(&table, &demand, &supply, &q).unwrap();
    // f0 serves both points, f1 only the second
    assert_eq!(f.catchment_demand, [300.0, 200.0]);
    assert_eq!(f.ratio, [10.0 / 300.0, 30.0 / 200.0]);
    assert_eq!(f.index, [10.0 / 300.0, 10.0 / 300.0 + 30.0 / 200.0]);
    assert_eq!(f.unreached_supply, 0.0);
    let shared: f64 = demand.iter().zip(&f.index).map(|(p, a)| p * a).sum();
    assert!((shared - 40.0).abs() < 1e-12);

    // the cutoff is inclusive; a facility nobody reaches shares nothing
    q.cutoff = 8.0;
    let f = floating_catchment(&table, &demand, &supply, &q).unwrap();
    assert_eq!(f.catchment_demand, [100.0, 200.0]);
    assert_eq!(f.index, [0.1, 0.15]);
    q.cutoff = 7.0;
    let f = floating_catchment(&table, &demand, &supply, &q).unwrap();
    assert_eq!(f.ratio, [0.1, 0.0]);
    assert_eq!(f.index, [0.1, 0.0]);
    assert_eq!(f.unreached_supply, 30.0);

    // enhanced: weights 2^(-t / 15)
    q.cutoff = 15.0;
    q.decay = DecayFunction::Exponential;
    let f = floating_catchment(&table, &demand, &supply, &q).unwrap();
    let w = |t: f64| 0.5f64.powf(t / 15.0);
    let d0 = 100.0 * w(5.0) + 200.0 * w(12.0);
    let d1 = 100.0 * w(20.0) + 200.0 * w(8.0);
    let expected = [
        10.0 / d0 * w(5.0) + 30.0 / d1 * w(20.0),
        10.0 / d0 * w(12.0) + 30.0 / d1 * w(8.0),
    ];
    // several parallel columns are read by the same index
    #[allow(clippy::needless_range_loop)]
    for k in 0..2 {
        assert!((f.index[k] - expected[k]).abs() < 1e-12, "{:?}", f.index);
    }
    assert!((f.catchment_demand[0] - d0).abs() < 1e-9);

    let refused =
        |q: &FloatingCatchmentParams, d: &[f64], s: &[f64], what: &str| match floating_catchment(
            &table, d, s, q,
        ) {
            Err(RouteError::Invalid(m)) => assert!(m.contains(what), "{m}"),
            other => panic!("{what}: {other:?}"),
        };
    refused(&q, &[1.0], &supply, "1 weights");
    refused(&q, &demand, &[1.0], "1 capacities");
    refused(&q, &[1.0, -2.0], &supply, "not negative");
    refused(&q, &demand, &[f64::INFINITY, 1.0], "supply must be finite");
    q.cutoff = f64::NAN;
    refused(&q, &demand, &supply, "needs a cutoff");
    q.decay = DecayFunction::FixedExponential {
        decay_constant: 2.0,
    };
    refused(&q, &demand, &supply, "between 0 and 1");
    // the fixed exponential needs no cutoff
    q.decay = DecayFunction::FixedExponential {
        decay_constant: 0.001,
    };
    assert!(floating_catchment(&table, &demand, &supply, &q).is_ok());
}

#[test]
fn floating_catchment_shares_out_the_supply_reached() {
    let mut rng = Rng(99);
    for decay in [
        DecayFunction::Step,
        DecayFunction::Exponential,
        DecayFunction::Linear { width_minutes: 10 },
        DecayFunction::Logistic {
            standard_deviation_minutes: 5.0,
        },
        DecayFunction::FixedExponential {
            decay_constant: 0.002,
        },
    ] {
        for _ in 0..20 {
            let inst = Instance::random(&mut rng, 30, 12, 0);
            let supply: Vec<f64> = (0..12).map(|_| rng.below(50) as f64).collect();
            let mut q = FloatingCatchmentParams::default();
            q.cutoff = 12.0;
            q.decay = decay.clone();
            let f = floating_catchment(&inst.table(), &inst.demand, &supply, &q).unwrap();
            let shared: f64 = inst.demand.iter().zip(&f.index).map(|(p, a)| p * a).sum();
            let total: f64 = supply.iter().sum();
            assert!(
                (shared - (total - f.unreached_supply)).abs() < 1e-9 * (1.0 + total),
                "{decay:?}: {shared} of {total} less {}",
                f.unreached_supply
            );
        }
    }
}
