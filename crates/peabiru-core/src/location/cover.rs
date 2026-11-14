//! The covering models: maximal covering and location set covering.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::{first_best, map_of, Key, LocationModel, LocationSolution, Problem};
use crate::RouteError;

/// A candidate in the greedy queue, with the score it had after `round`
/// sites were open. Scores only fall as sites open, so a stale score is an
/// upper bound.
struct Entry {
    score: f64,
    site: usize,
    round: usize,
}

impl PartialEq for Entry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    /// The largest score first, then the lowest site.
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then(other.site.cmp(&self.site))
    }
}

/// The demand points of a problem and who covers them.
struct Cover<'a> {
    p: &'a Problem<'a>,
    cutoff: f64,
    /// Covered by an existing facility.
    fixed: Vec<bool>,
    /// What a demand point is worth to the choice of sites: its weight
    /// (maximal covering), or 1 for any point with weight (set covering).
    worth: Vec<f64>,
    /// How many of the chosen sites cover each demand point.
    count: Vec<u32>,
    /// The exclusive or of `position + 1` of the chosen sites covering each
    /// demand point: with a count of 1, the one that does.
    who: Vec<u32>,
}

impl Cover<'_> {
    /// The demand points site `j` covers.
    fn covered_by(&self, j: usize) -> impl Iterator<Item = usize> + '_ {
        let (rows, times) = self.p.costs.site(j);
        rows.iter()
            .zip(times)
            .filter(|(_, &t)| t <= self.cutoff)
            .map(|(&i, _)| i as usize)
    }

    /// What site `j` would add to the chosen sites, in `values`.
    fn gain(&self, j: usize, values: &[f64]) -> f64 {
        self.covered_by(j)
            .filter(|&i| !self.fixed[i] && self.count[i] == 0)
            .map(|i| values[i])
            .sum()
    }

    fn open(&mut self, j: usize, position: usize) {
        let (rows, times) = self.p.costs.site(j);
        for (&i, &t) in rows.iter().zip(times) {
            if t <= self.cutoff {
                self.count[i as usize] += 1;
                self.who[i as usize] ^= position as u32 + 1;
            }
        }
    }

    fn close(&mut self, j: usize, position: usize) {
        let (rows, times) = self.p.costs.site(j);
        for (&i, &t) in rows.iter().zip(times) {
            if t <= self.cutoff {
                self.count[i as usize] -= 1;
                self.who[i as usize] ^= position as u32 + 1;
            }
        }
    }

    /// The demand covered, by the existing facilities and the chosen sites.
    fn covered(&self) -> f64 {
        (0..self.fixed.len())
            .filter(|&i| self.fixed[i] || self.count[i] > 0)
            .map(|i| self.p.demand[i])
            .sum()
    }
}

pub(super) fn solve(p: &Problem) -> Result<LocationSolution, RouteError> {
    let n = p.costs.n_demand();
    let model = p.params.model;
    let cancel = &p.params.cancel;
    let mut c = Cover {
        p,
        cutoff: p.params.cutoff,
        fixed: vec![false; n],
        worth: match model {
            LocationModel::SetCover => p
                .demand
                .iter()
                .map(|&w| f64::from(u8::from(w > 0.0)))
                .collect(),
            _ => p.demand.to_vec(),
        },
        count: vec![0; n],
        who: vec![0; n],
    };
    for &e in p.existing {
        let rows: Vec<usize> = c.covered_by(e).collect();
        for i in rows {
            c.fixed[i] = true;
        }
    }
    let covered_before = c.covered();

    // what each candidate would cover alone, and who nobody covers
    let mut potential = vec![0f64; p.costs.n_sites()];
    let mut servable = c.fixed.clone();
    let mut queue = BinaryHeap::with_capacity(p.candidates.len());
    for &j in p.candidates {
        cancel.check()?;
        potential[j] = c.gain(j, p.demand);
        for i in c.covered_by(j) {
            servable[i] = true;
        }
        queue.push(Entry {
            score: c.gain(j, &c.worth),
            site: j,
            round: 0,
        });
    }
    let unservable: Vec<usize> = (0..n)
        .filter(|&i| p.demand[i] > 0.0 && !servable[i])
        .collect();

    // greedy: the candidate worth most to the points still uncovered
    let limit = match model {
        LocationModel::SetCover => usize::MAX,
        _ => p.params.n_facilities,
    };
    let floor = match model {
        LocationModel::SetCover => 0.0,
        _ => p.params.min_gain,
    };
    let mut chosen: Vec<usize> = Vec::new();
    while chosen.len() < limit {
        cancel.check()?;
        let Some(top) = queue.pop() else { break };
        if top.round < chosen.len() {
            queue.push(Entry {
                score: c.gain(top.site, &c.worth),
                site: top.site,
                round: chosen.len(),
            });
            continue;
        }
        if top.score <= 0.0 || top.score < floor {
            break;
        }
        c.open(top.site, chosen.len());
        chosen.push(top.site);
    }
    let objective_of = |c: &Cover, chosen: &[usize]| match model {
        LocationModel::SetCover => chosen.len() as f64,
        _ => c.covered(),
    };
    let objective_greedy = objective_of(&c, &chosen);

    let mut n_swaps = 0;
    if p.params.swap && model == LocationModel::SetCover {
        // drop the sites the others make redundant, the last chosen first
        let mut k = chosen.len();
        while k > 0 {
            k -= 1;
            cancel.check()?;
            let j = chosen[k];
            let needed = c
                .covered_by(j)
                .any(|i| !c.fixed[i] && c.count[i] == 1 && c.worth[i] > 0.0);
            if !needed {
                // positions matter to `who` only while swapping: rebuild
                chosen.remove(k);
                c.count.iter_mut().for_each(|x| *x = 0);
                c.who.iter_mut().for_each(|x| *x = 0);
                for (position, &site) in chosen.iter().enumerate() {
                    c.open(site, position);
                }
                n_swaps += 1;
            }
        }
    } else if p.params.swap && !chosen.is_empty() {
        n_swaps = substitute(&mut c, &mut chosen)?;
    }
    let objective = objective_of(&c, &chosen);
    let covered = c.covered();

    // the chosen sites by what each adds
    let mut left = chosen;
    let (mut sites, mut marginal, mut cumulative) = (Vec::new(), Vec::new(), Vec::new());
    c.count.iter_mut().for_each(|x| *x = 0);
    let mut so_far = covered_before;
    while !left.is_empty() {
        let mut best = 0;
        let mut best_score = f64::NEG_INFINITY;
        for (k, &j) in left.iter().enumerate() {
            let score = c.gain(j, &c.worth);
            if score > best_score || (score == best_score && j < left[best]) {
                (best, best_score) = (k, score);
            }
        }
        let j = left.swap_remove(best);
        let gain = c.gain(j, p.demand);
        c.open(j, sites.len());
        so_far += gain;
        sites.push(j);
        marginal.push(gain);
        cumulative.push(so_far);
    }

    Ok(LocationSolution {
        model,
        sites,
        marginal,
        cumulative,
        potential,
        assigned_site: Vec::new(),
        assigned_minutes: Vec::new(),
        objective_before: match model {
            LocationModel::SetCover => 0.0,
            _ => covered_before,
        },
        objective_greedy,
        objective,
        covered_before,
        covered,
        total_demand: p.demand.iter().sum(),
        unservable_demand: unservable.iter().map(|&i| p.demand[i]).sum(),
        unservable,
        n_swaps,
    })
}

/// Vertex substitution for maximal covering: while some swap of a chosen
/// site for a closed candidate covers more demand, make the best of them.
fn substitute(c: &mut Cover, chosen: &mut [usize]) -> Result<usize, RouteError> {
    let p = c.p;
    let mut n_swaps = 0;
    let mut current = c.covered();
    loop {
        // what closing each chosen site would uncover
        let lost: Vec<f64> = chosen
            .iter()
            .enumerate()
            .map(|(r, &j)| {
                c.covered_by(j)
                    .filter(|&i| !c.fixed[i] && c.count[i] == 1 && c.who[i] == r as u32 + 1)
                    .map(|i| p.demand[i])
                    .sum()
            })
            .collect();
        let closed: Vec<usize> = p
            .candidates
            .iter()
            .copied()
            .filter(|j| !chosen.contains(j))
            .collect();
        let view = &*c;
        let swaps: Vec<Option<(Key, usize)>> = map_of(
            &closed,
            p.params.n_threads,
            &p.params.cancel,
            || vec![0f64; chosen.len()],
            |j, shared: &mut Vec<f64>| {
                // demand `j` covers that nobody covers, and that only the
                // chosen site `r` covers (kept if `r` is the one to close)
                let mut alone = 0f64;
                for i in view.covered_by(j) {
                    if view.fixed[i] {
                        continue;
                    }
                    match view.count[i] {
                        0 => alone += p.demand[i],
                        1 => shared[view.who[i] as usize - 1] += p.demand[i],
                        _ => {}
                    }
                }
                let mut best: Option<(f64, usize)> = None;
                for (r, kept) in shared.iter_mut().enumerate() {
                    let delta = alone + *kept - lost[r];
                    *kept = 0.0;
                    if delta > 0.0 && best.map_or(true, |b| delta > b.0) {
                        best = Some((delta, r));
                    }
                }
                best.map(|(delta, r)| ([-delta, 0.0], r))
            },
        )?;
        let Some(k) = first_best(&swaps) else { break };
        let (j, r) = (closed[k], swaps[k].as_ref().map_or(0, |s| s.1));
        let out = chosen[r];
        c.close(out, r);
        c.open(j, r);
        let now = c.covered();
        if now > current {
            chosen[r] = j;
            current = now;
            n_swaps += 1;
        } else {
            // a gain that was only rounding
            c.close(j, r);
            c.open(out, r);
            break;
        }
    }
    Ok(n_swaps)
}
