//! The travel-time models: p-median and p-center.

use super::{better, first_best, map_of, Key, LocationModel, LocationSolution, Problem};
use crate::RouteError;

/// No open facility reaches the demand point.
const NONE: u32 = u32::MAX;

/// The open facilities and, per demand point, the two nearest.
struct State<'a> {
    p: &'a Problem<'a>,
    /// The demand points that count: with weight, and reached by some site.
    active: Vec<usize>,
    /// Minutes charged to a point no open facility reaches.
    unreachable: f64,
    /// Existing facilities first, then the chosen sites.
    open: Vec<usize>,
    /// Minutes to the nearest open facility and to the second nearest
    /// (`unreachable` without one), and the position in `open` of the
    /// nearest.
    first: Vec<f64>,
    second: Vec<f64>,
    nearest: Vec<u32>,
}

impl State<'_> {
    fn refresh(&mut self) {
        self.first.iter_mut().for_each(|x| *x = self.unreachable);
        self.second.iter_mut().for_each(|x| *x = self.unreachable);
        self.nearest.iter_mut().for_each(|x| *x = NONE);
        for (q, &j) in self.open.iter().enumerate() {
            let (rows, times) = self.p.costs.site(j);
            for (&i, &t) in rows.iter().zip(times) {
                let i = i as usize;
                if t < self.first[i] {
                    self.second[i] = self.first[i];
                    self.first[i] = t;
                    self.nearest[i] = q as u32;
                } else if t < self.second[i] {
                    self.second[i] = t;
                }
            }
        }
    }

    /// The weighted sum and the largest of the times to the nearest open
    /// facility.
    fn sum_and_max(&self) -> (f64, f64) {
        let (mut sum, mut max) = (0f64, 0f64);
        for &i in &self.active {
            sum += self.p.demand[i] * self.first[i];
            max = max.max(self.first[i]);
        }
        (sum, max)
    }

    fn key_of(&self, sum: f64, max: f64) -> Key {
        match self.p.params.model {
            LocationModel::PCenter => [max, sum],
            _ => [sum, 0.0],
        }
    }

    fn key(&self) -> Key {
        let (sum, max) = self.sum_and_max();
        self.key_of(sum, max)
    }

    /// The model's objective in a key.
    fn objective(&self, key: &Key) -> f64 {
        key[0]
    }

    /// The times of site `j` spread over all the demand points (infinite
    /// where it does not reach), `f` on them, and the scratch column put
    /// back as it was.
    fn with_column<T>(&self, j: usize, column: &mut [f64], f: impl FnOnce(&[f64]) -> T) -> T {
        let (rows, times) = self.p.costs.site(j);
        for (&i, &t) in rows.iter().zip(times) {
            column[i as usize] = t;
        }
        let out = f(column);
        for &i in rows {
            column[i as usize] = f64::INFINITY;
        }
        out
    }

    /// The key with site `j` open as well.
    fn key_with(&self, j: usize, column: &mut [f64]) -> Key {
        self.with_column(j, column, |c| {
            let (mut sum, mut max) = (0f64, 0f64);
            for &i in &self.active {
                let t = self.first[i].min(c[i]);
                sum += self.p.demand[i] * t;
                max = max.max(t);
            }
            self.key_of(sum, max)
        })
    }

    /// The best key reached by opening `j` and closing one of the chosen
    /// sites, and the position in `open` of the site to close (the first
    /// of them on a tie).
    fn key_swapping(&self, j: usize, s: &mut Scratch) -> (Key, usize) {
        let slots = self.open.len() + 1; // the last: points nobody reaches
        s.shift[..slots].iter_mut().for_each(|x| *x = 0.0);
        s.kept[..slots]
            .iter_mut()
            .for_each(|x| *x = f64::NEG_INFINITY);
        s.moved[..slots]
            .iter_mut()
            .for_each(|x| *x = f64::NEG_INFINITY);
        let Scratch {
            column,
            shift,
            kept,
            moved,
        } = s;
        let base = self.with_column(j, column, |c| {
            let mut base = 0f64;
            for &i in &self.active {
                let q = match self.nearest[i] {
                    NONE => slots - 1,
                    q => q as usize,
                };
                // with its nearest facility open, and with it closed
                let stay = self.first[i].min(c[i]);
                let go = self.second[i].min(c[i]);
                base += self.p.demand[i] * stay;
                shift[q] += self.p.demand[i] * (go - stay);
                kept[q] = kept[q].max(stay);
                moved[q] = moved[q].max(go);
            }
            base
        });
        // the largest of `kept` and the largest elsewhere
        let (mut top, mut top_at, mut next) = (f64::NEG_INFINITY, usize::MAX, f64::NEG_INFINITY);
        for (q, &v) in kept[..slots].iter().enumerate() {
            if v > top {
                (next, top, top_at) = (top, v, q);
            } else if v > next {
                next = v;
            }
        }
        let mut best: Option<(Key, usize)> = None;
        for r in self.p.existing.len()..self.open.len() {
            let others = if top_at == r { next } else { top };
            let key = self.key_of(base + shift[r], moved[r].max(others).max(0.0));
            if best.as_ref().map_or(true, |b| better(&key, &b.0)) {
                best = Some((key, r));
            }
        }
        best.expect("a site to close")
    }
}

/// A worker's buffers for [`State::key_swapping`].
struct Scratch {
    column: Vec<f64>,
    shift: Vec<f64>,
    kept: Vec<f64>,
    moved: Vec<f64>,
}

pub(super) fn solve(p: &Problem) -> Result<LocationSolution, RouteError> {
    let n = p.costs.n_demand();
    let params = p.params;
    let mut reached = vec![false; n];
    for j in 0..p.costs.n_sites() {
        for &i in p.costs.site(j).0 {
            reached[i as usize] = true;
        }
    }
    let unservable: Vec<usize> = (0..n)
        .filter(|&i| p.demand[i] > 0.0 && !reached[i])
        .collect();
    let unreachable = params.unreachable_time.unwrap_or_else(|| {
        let max = p.costs.max_minutes();
        if max > 0.0 {
            2.0 * max
        } else {
            1.0
        }
    });
    let mut s = State {
        p,
        active: (0..n)
            .filter(|&i| p.demand[i] > 0.0 && reached[i])
            .collect(),
        unreachable,
        open: p.existing.to_vec(),
        first: vec![0.0; n],
        second: vec![0.0; n],
        nearest: vec![NONE; n],
    };
    s.refresh();
    let before = s.key();
    let column = || vec![f64::INFINITY; n];

    // greedy: the candidate that improves the objective most
    let mut potential = vec![0f64; p.costs.n_sites()];
    let mut current = before;
    let mut left: Vec<usize> = p.candidates.to_vec();
    while s.open.len() - p.existing.len() < params.n_facilities {
        let keys: Vec<Option<(Key, ())>> = map_of(
            &left,
            params.n_threads,
            &params.cancel,
            column,
            |j, c: &mut Vec<f64>| Some((s.key_with(j, c), ())),
        )?;
        if s.open.len() == p.existing.len() {
            for (&j, key) in left.iter().zip(&keys) {
                potential[j] = key
                    .as_ref()
                    .map_or(0.0, |k| s.objective(&before) - s.objective(&k.0));
            }
        }
        let Some(k) = first_best(&keys) else { break };
        let key = keys[k].as_ref().map_or(current, |v| v.0);
        if !better(&key, &current) {
            break;
        }
        s.open.push(left.remove(k));
        s.refresh();
        current = s.key();
    }
    let greedy = current;

    // vertex substitution: the best swap of a chosen site for a closed one
    let mut n_swaps = 0;
    if params.swap && s.open.len() > p.existing.len() {
        loop {
            let slots = s.open.len() + 1;
            let swaps: Vec<Option<(Key, usize)>> = map_of(
                &left,
                params.n_threads,
                &params.cancel,
                || Scratch {
                    column: column(),
                    shift: vec![0.0; slots],
                    kept: vec![0.0; slots],
                    moved: vec![0.0; slots],
                },
                |j, scratch: &mut Scratch| Some(s.key_swapping(j, scratch)),
            )?;
            let Some(k) = first_best(&swaps) else { break };
            let (key, r) = swaps[k].as_ref().map_or((current, 0), |v| (v.0, v.1));
            if !better(&key, &current) {
                break;
            }
            let (j, out) = (left[k], s.open[r]);
            s.open[r] = j;
            s.refresh();
            let now = s.key();
            if !better(&now, &current) {
                // a gain that was only rounding
                s.open[r] = out;
                s.refresh();
                break;
            }
            left[k] = out;
            left.sort_unstable();
            current = now;
            n_swaps += 1;
        }
    }

    // the chosen sites by what each adds
    let mut rest: Vec<usize> = s.open.split_off(p.existing.len());
    rest.sort_unstable();
    s.refresh();
    let (mut sites, mut marginal, mut cumulative) = (Vec::new(), Vec::new(), Vec::new());
    let mut so_far = before;
    let mut scratch = column();
    while !rest.is_empty() {
        params.cancel.check()?;
        let keys: Vec<Option<(Key, ())>> = rest
            .iter()
            .map(|&j| Some((s.key_with(j, &mut scratch), ())))
            .collect();
        let k = first_best(&keys).unwrap_or(0);
        let j = rest.remove(k);
        s.open.push(j);
        s.refresh();
        let now = s.key();
        sites.push(j);
        marginal.push(s.objective(&so_far) - s.objective(&now));
        cumulative.push(s.objective(&now));
        so_far = now;
    }

    Ok(LocationSolution {
        model: params.model,
        sites,
        marginal,
        cumulative,
        potential,
        assigned_site: Vec::new(),
        assigned_minutes: Vec::new(),
        objective_before: s.objective(&before),
        objective_greedy: s.objective(&greedy),
        objective: s.objective(&current),
        covered_before: f64::NAN,
        covered: f64::NAN,
        total_demand: p.demand.iter().sum(),
        unservable_demand: unservable.iter().map(|&i| p.demand[i]).sum(),
        unservable,
        n_swaps,
    })
}
