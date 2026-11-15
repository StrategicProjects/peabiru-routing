//! Capacitated maximal covering (Current and Storbeck, 1988): a facility
//! serves at most its capacity, a site may open more than once, and a
//! demand point may be served in part.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use super::{LocationSolution, Problem};
use crate::RouteError;

/// A site with a value: the largest value first, then the lowest site.
struct Entry {
    value: f64,
    site: usize,
    /// Greedy queue: how many facilities were open when `value` was
    /// computed. Bound: how much demand in reach the site has left to
    /// offer after this unit.
    extra: f64,
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
    fn cmp(&self, other: &Self) -> Ordering {
        self.value
            .total_cmp(&other.value)
            .then(other.site.cmp(&self.site))
    }
}

/// The demand points within the cutoff of site `j`, the nearest first
/// (ties by the lowest index).
fn in_reach(p: &Problem, j: usize) -> Vec<(f64, usize)> {
    let (rows, times) = p.costs.site(j);
    let mut near: Vec<(f64, usize)> = rows
        .iter()
        .zip(times)
        .filter(|(_, &t)| t <= p.params.cutoff)
        .map(|(&i, &t)| (t, i as usize))
        .collect();
    near.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    near
}

/// The demand still unserved within the cutoff of site `j`.
fn reach(p: &Problem, j: usize, left: &[f64]) -> f64 {
    let (rows, times) = p.costs.site(j);
    rows.iter()
        .zip(times)
        .filter(|(_, &t)| t <= p.params.cutoff)
        .map(|(&i, _)| left[i as usize])
        .fold(0.0, |a, b| a + b)
}

pub(super) fn solve(p: &Problem, capacity: &[f64]) -> Result<LocationSolution, RouteError> {
    let n = p.costs.n_demand();
    let cancel = &p.params.cancel;
    let total_demand = p.demand.iter().fold(0.0, |a, b| a + b);
    // the demand of each point not served yet
    let mut left = p.demand.to_vec();

    // the existing facilities serve first: every pair of facility and
    // demand point within the cutoff, the shortest trips first
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    let mut servable = vec![false; n];
    for &e in p.existing {
        cancel.check()?;
        for (t, i) in in_reach(p, e) {
            servable[i] = true;
            pairs.push((t, e, i));
        }
    }
    pairs.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut room = capacity.to_vec();
    let mut covered_before = 0.0;
    for &(_, e, i) in &pairs {
        let take = room[e].min(left[i]);
        if take > 0.0 {
            room[e] -= take;
            left[i] -= take;
            covered_before += take;
        }
    }
    drop(pairs);

    // what one facility at each candidate would serve alone
    let mut potential = vec![0f64; p.costs.n_sites()];
    let mut reach_at_start = vec![0f64; p.costs.n_sites()];
    let mut queue = BinaryHeap::with_capacity(p.candidates.len());
    for &j in p.candidates {
        cancel.check()?;
        let (rows, times) = p.costs.site(j);
        for (&i, &t) in rows.iter().zip(times) {
            if t <= p.params.cutoff {
                servable[i as usize] = true;
            }
        }
        reach_at_start[j] = reach(p, j, &left);
        potential[j] = capacity[j].min(reach_at_start[j]);
        queue.push(Entry {
            value: potential[j],
            site: j,
            extra: 0.0,
        });
    }
    let unservable: Vec<usize> = (0..n)
        .filter(|&i| p.demand[i] > 0.0 && !servable[i])
        .collect();

    // no choice of `n_facilities` facilities serves more than the largest
    // of the units the candidates can fill, nor than the demand in reach
    let upper_bound = {
        let mut units: BinaryHeap<Entry> = p
            .candidates
            .iter()
            .map(|&j| Entry {
                value: potential[j],
                site: j,
                extra: reach_at_start[j] - potential[j],
            })
            .collect();
        let mut sum = 0.0;
        for _ in 0..p.params.n_facilities {
            let Some(top) = units.pop() else { break };
            if top.value <= 0.0 {
                break;
            }
            sum += top.value;
            let next = capacity[top.site].min(top.extra);
            units.push(Entry {
                value: next,
                site: top.site,
                extra: top.extra - next,
            });
        }
        let in_reach_of_a_candidate = {
            let mut seen = vec![false; n];
            for &j in p.candidates {
                let (rows, times) = p.costs.site(j);
                for (&i, &t) in rows.iter().zip(times) {
                    if t <= p.params.cutoff {
                        seen[i as usize] = true;
                    }
                }
            }
            (0..n)
                .filter(|&i| seen[i])
                .map(|i| left[i])
                .fold(0.0, |a, b| a + b)
        };
        covered_before + sum.min(in_reach_of_a_candidate)
    };

    // greedy: the facility that would serve the most demand, taken from
    // the nearest points; a site stays in the queue while it has demand
    // in reach, and its value only falls as facilities open
    let (mut sites, mut marginal, mut cumulative) = (Vec::new(), Vec::new(), Vec::new());
    let mut so_far = covered_before;
    while sites.len() < p.params.n_facilities {
        cancel.check()?;
        let Some(top) = queue.pop() else { break };
        if top.extra < sites.len() as f64 {
            queue.push(Entry {
                value: capacity[top.site].min(reach(p, top.site, &left)),
                site: top.site,
                extra: sites.len() as f64,
            });
            continue;
        }
        if top.value <= 0.0 || top.value < p.params.min_gain {
            break;
        }
        let mut room = capacity[top.site];
        let mut gain = 0.0;
        for (_, i) in in_reach(p, top.site) {
            let take = room.min(left[i]);
            if take > 0.0 {
                room -= take;
                left[i] -= take;
                gain += take;
            }
            if room <= 0.0 {
                break;
            }
        }
        so_far += gain;
        sites.push(top.site);
        marginal.push(gain);
        cumulative.push(so_far);
        // stale by construction: recomputed if it comes to the top again
        queue.push(Entry {
            value: top.value,
            site: top.site,
            extra: sites.len() as f64 - 1.0,
        });
    }

    let served: Vec<f64> = p.demand.iter().zip(&left).map(|(w, l)| w - l).collect();
    Ok(LocationSolution {
        model: p.params.model,
        sites,
        marginal,
        cumulative,
        potential,
        assigned_site: Vec::new(),
        assigned_minutes: Vec::new(),
        objective_before: covered_before,
        objective_greedy: so_far,
        objective: so_far,
        covered_before,
        covered: so_far,
        upper_bound: upper_bound.max(so_far).min(total_demand),
        served,
        total_demand,
        unservable_demand: unservable.iter().fold(0.0, |a, &i| a + p.demand[i]),
        unservable,
        n_swaps: 0,
    })
}
