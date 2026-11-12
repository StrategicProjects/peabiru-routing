//! r5r's fare rules ([`FareCalculator::fare`]) one ride at a time, with
//! what a multi-criteria search needs to compare partial trips soundly.
//!
//! [`FareCalculator::fare`] walks the rides of a trip in order, keeping the
//! fare paid so far (before the cap), the previous ride's pattern, the time
//! it was boarded and the number of discounted transfers used. That state
//! is all the rest of the trip depends on: [`FareState`] holds it and
//! [`FareModel::step`] adds one ride exactly as the loop of `fare` does, so
//! that the fare of a trip built ride by ride is the calculator's
//! (`fare(rides).paid`, see the tests).
//!
//! Two partial trips can then be compared on fare without knowing how they
//! continue ([`FareModel::dominates`]): one is at least as cheap as the
//! other for *every* continuation when
//!
//! - their states are equivalent (same type of the last ride and, while a
//!   discount can still be granted, the same discounts used, the same full
//!   fare and route of the last ride, the same time and the same deadline for
//!   a discount), and it has paid no more; or
//! - it has paid less by at least the most the other could still save, plus
//!   the most it could still lose, on discounted transfers
//!   ([`FareModel::slack`]).
//!
//! R5's `FareDominatingList` compares the fare paid and the transfer
//! allowance (its value, number and expiration). With r5r's calculator that
//! is not always sound: a discounted transfer can cost more than two full
//! fares (nothing in the structure forbids it), the discount is taken at the
//! first transfer that qualifies (so holding a later deadline can lose a
//! bigger discount further on), the same-route rule and the full fare of
//! the previous ride change what a transfer costs, and the cap is applied to
//! the total. The rules here make no assumption about the structure.

use super::FareCalculator;

/// The fare state of a partial trip: [`FareCalculator::fare`]'s loop
/// variables after its rides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FareState {
    /// Paid so far, cents, before the cap.
    pub(crate) paid: i32,
    /// Pattern of the previous ride (`prev`), -1 before the first ride.
    pub(crate) last: i32,
    /// When `prev` was boarded (not updated by a free ride of an
    /// unlimited type, as in the calculator).
    pub(crate) prev_board: i32,
    /// Discounted transfers used.
    pub(crate) discounts: i32,
}

impl Default for FareState {
    fn default() -> Self {
        FareState {
            paid: 0,
            last: -1,
            prev_board: 0,
            discounts: 0,
        }
    }
}

/// A calculator with the bounds the comparisons need.
pub(crate) struct FareModel<'a> {
    pub(crate) calc: &'a FareCalculator,
    /// No transfer is ever discounted (no rule, `max_discounted_transfers`
    /// not positive, or no allowance).
    never: bool,
    /// Per fare type: whether any discounted transfer starts from it, and
    /// whether the transfer to the same type is subject to the same-route
    /// rule.
    rule_from: Vec<bool>,
    same_route_matters: Vec<bool>,
    /// Per pattern: an index of its `route_id` (the same-route rule compares
    /// these strings).
    route_class: Vec<u32>,
    /// Largest saving and largest loss of one discounted transfer against
    /// paying the second ride's full fare, cents, both >= 0.
    gain: i64,
    loss: i64,
    /// Smallest change of the paid fare one ride can make (<= 0).
    min_step: i64,
    /// Per fare type `T` and number of rides `r` left after the next ride
    /// that is not free (0, 1, 2 or more): how much more a trip whose
    /// discount deadline is later can end up paying than one with an
    /// earlier deadline, the two being otherwise equivalent (see
    /// [`FareModel::later_deadline_extra`]).
    later_extra: Vec<[i64; 3]>,
}

impl<'a> FareModel<'a> {
    pub(crate) fn new(calc: &'a FareCalculator) -> Self {
        let n_types = calc.types.len();
        let mut lo = vec![i64::MAX; n_types];
        let mut hi = vec![i64::MIN; n_types];
        let mut classes: std::collections::BTreeMap<Option<&str>, u32> = Default::default();
        let mut route_class = Vec::with_capacity(calc.patterns.len());
        for &(t, cents, k) in &calc.patterns {
            let t = t as usize;
            lo[t] = lo[t].min(i64::from(cents));
            hi[t] = hi[t].max(i64::from(cents));
            let id = calc.route_ids[k as usize].as_deref();
            let next = classes.len() as u32;
            route_class.push(*classes.entry(id).or_insert(next));
        }
        let mut gain = 0i64;
        let mut loss = 0i64;
        let mut min_step = 0i64;
        let mut rule_from = vec![false; n_types];
        let mut same_route_matters = vec![false; n_types];
        let mut any_rule = false;
        for a in 0..n_types {
            for b in 0..n_types {
                let Some(r) = calc.transfers[a][b] else {
                    continue;
                };
                rule_from[a] = true;
                any_rule = true;
                if a == b && !calc.types[a].same_route {
                    same_route_matters[a] = true;
                }
                if lo[a] == i64::MAX || lo[b] == i64::MAX {
                    continue; // no pattern of one of the types
                }
                // the second ride costs fare - full(prev) instead of full(cur)
                let f = i64::from(r.fare);
                gain = gain.max(hi[b] + hi[a] - f);
                loss = loss.max(f - lo[a] - lo[b]);
                min_step = min_step.min(f - hi[a]);
            }
        }
        let never = !any_rule || calc.max_discounted <= 0 || calc.allowance_s < 0;
        // the largest saving of a discounted transfer from each type
        let saving = |_a: usize, b: usize, f: i32, hi_a: i64| hi[b] + hi_a - i64::from(f);
        let mut max_from = vec![0i64; n_types];
        for a in 0..n_types {
            for b in 0..n_types {
                if let (Some(r), true) =
                    (calc.transfers[a][b], lo[a] != i64::MAX && lo[b] != i64::MAX)
                {
                    max_from[a] = max_from[a].max(saving(a, b, r.fare, hi[a]));
                }
            }
        }
        let mut later_extra = vec![[0i64; 3]; n_types];
        for t in 0..n_types {
            for x in 0..n_types {
                let Some(r) = calc.transfers[t][x] else {
                    continue;
                };
                if lo[t] == i64::MAX || lo[x] == i64::MAX {
                    continue;
                }
                // the least saving of a discount from t to x
                let s1 = lo[x] + lo[t] - i64::from(r.fare);
                for (k, later) in [0, max_from[x], gain].into_iter().enumerate() {
                    later_extra[t][k] = later_extra[t][k].max((later - s1).max(0));
                }
            }
        }
        FareModel {
            calc,
            never,
            rule_from,
            same_route_matters,
            route_class,
            gain,
            loss,
            min_step,
            later_extra,
        }
    }

    /// How much more a trip in a state with a later discount deadline can
    /// end up paying than one in an otherwise equivalent state (same time,
    /// type, discounts used, full fare and route of the last ride) with an
    /// earlier deadline, over any continuation with at most `rides_left`
    /// rides.
    ///
    /// The two continuations board at the same times, so they differ only
    /// at the first ride that is not free: the trip with the later deadline
    /// may get a discount there that the other does not. From then on their
    /// states are the same but for one discount more used by the first, so
    /// the other gets exactly one discount more at most, later. The first
    /// is worse by at most that later saving less the first saving.
    fn later_deadline_extra(&self, t_type: u32, rides_left: i64) -> i64 {
        let k = (rides_left - 1).clamp(0, 2) as usize;
        self.later_extra[t_type as usize][k]
    }

    /// The state after riding `pattern`, boarded at `board`: one turn of
    /// [`FareCalculator::fare`]'s loop (its first ride when `s` has none).
    pub(crate) fn step(&self, s: &FareState, pattern: i32, board: i32) -> FareState {
        let c = self.calc;
        if s.last < 0 {
            return FareState {
                paid: c.full(pattern),
                last: pattern,
                prev_board: board,
                discounts: 0,
            };
        }
        let mut n = *s;
        let (ta, tb) = (c.type_of(s.last), c.type_of(pattern));
        if ta == tb && c.types[ta as usize].unlimited {
            n.last = pattern;
            return n;
        }
        if s.discounts >= c.max_discounted {
            n.paid = n.paid.wrapping_add(c.full(pattern));
        } else {
            let gap = board.wrapping_sub(s.prev_board);
            match c.transfers[ta as usize][tb as usize] {
                Some(t) if c.transfer_allowed(s.last, pattern) && gap <= c.allowance_s => {
                    n.paid = n.paid.wrapping_add(t.fare.wrapping_sub(c.full(s.last)));
                    n.discounts += 1;
                }
                _ => n.paid = n.paid.wrapping_add(c.full(pattern)),
            }
        }
        n.last = pattern;
        n.prev_board = board;
        n
    }

    /// The fare of a whole trip in state `s`: the cap applied.
    pub(crate) fn total(&self, s: &FareState) -> i32 {
        if self.calc.cap_positive {
            s.paid.min(self.calc.cap)
        } else {
            s.paid
        }
    }

    /// Fare type of the last ride, or `u32::MAX` before the first.
    fn last_type(&self, s: &FareState) -> u32 {
        if s.last < 0 {
            u32::MAX
        } else {
            self.calc.type_of(s.last)
        }
    }

    fn exhausted(&self, s: &FareState) -> bool {
        self.never || s.discounts >= self.calc.max_discounted
    }

    /// How many more discounted transfers a trip in state `s` could get,
    /// with at most `rides_left` more rides.
    fn discounts_left(&self, s: &FareState, rides_left: i64) -> i64 {
        if self.exhausted(s) {
            0
        } else {
            (i64::from(self.calc.max_discounted) - i64::from(s.discounts))
                .max(0)
                .min(rides_left.max(0))
        }
    }

    /// The most the paid fares of two trips in states `a` and `b` (with the
    /// same type of last ride) can move apart in `a`'s disfavour over any
    /// common continuation: what `b` could still save plus what `a` could
    /// still lose on discounted transfers.
    pub(crate) fn slack(&self, a: &FareState, b: &FareState, a_left: i64, b_left: i64) -> i64 {
        self.discounts_left(b, b_left) * self.gain + self.discounts_left(a, a_left) * self.loss
    }

    /// A deadline as seen by a trip at the clock time `t` of a search
    /// whose rides all board before `horizon`: past (no later ride can use
    /// it), beyond the horizon (every later ride can), or the time itself.
    fn canonical_deadline(&self, s: &FareState, t: i32, horizon: i32) -> i64 {
        let d = i64::from(s.prev_board) + i64::from(self.calc.allowance_s);
        // the next ride boards at t + 60 s at the earliest
        if d < i64::from(t) + 60 {
            i64::MIN
        } else if d >= i64::from(horizon) {
            i64::MAX
        } else {
            d
        }
    }

    /// Whether a trip in state `a` pays no more than one in state `b` for
    /// every way both can continue, where `a` is at its stop no later than
    /// `b` (`same_time`: at the same moment `t`), can make `a_left` more
    /// rides and `b` `b_left`, and every ride boards before `horizon`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn dominates(
        &self,
        a: &FareState,
        b: &FareState,
        same_time: bool,
        t: i32,
        horizon: i32,
        a_left: i64,
        b_left: i64,
    ) -> bool {
        let ta = self.last_type(a);
        if ta != self.last_type(b) {
            return false;
        }
        if a.paid <= b.paid && self.equivalent(a, b, ta, same_time, t, horizon) {
            return true;
        }
        if ta != u32::MAX
            && same_time
            && self.same_but_deadline(a, b, ta)
            && self.canonical_deadline(a, t, horizon) > self.canonical_deadline(b, t, horizon)
            && i64::from(a.paid) + self.later_deadline_extra(ta, b_left) <= i64::from(b.paid)
        {
            return true;
        }
        if ta == u32::MAX {
            return false; // no ride yet: equivalent above
        }
        i64::from(a.paid) + self.slack(a, b, a_left, b_left) <= i64::from(b.paid)
    }

    /// Neither exhausted, the same discounts used, full fare and route of
    /// the last ride (of type `t_type`), and a rule from that type: the
    /// states differ at most by their deadline.
    fn same_but_deadline(&self, a: &FareState, b: &FareState, t_type: u32) -> bool {
        let tt = t_type as usize;
        let c = self.calc;
        !self.exhausted(a)
            && !self.exhausted(b)
            && a.discounts == b.discounts
            && self.rule_from[tt]
            && c.full(a.last) == c.full(b.last)
            && (!self.same_route_matters[tt]
                || self.route_class[a.last as usize] == self.route_class[b.last as usize])
    }

    /// The two states give the same fare increments on every continuation.
    fn equivalent(
        &self,
        a: &FareState,
        b: &FareState,
        t_type: u32,
        same_time: bool,
        t: i32,
        horizon: i32,
    ) -> bool {
        if t_type == u32::MAX {
            return true; // the first ride resets everything
        }
        let (ea, eb) = (self.exhausted(a), self.exhausted(b));
        if ea && eb {
            // every later ride pays its full fare (or nothing, by the
            // unlimited rule, which depends on the type alone)
            return true;
        }
        if ea != eb || a.discounts != b.discounts {
            return false;
        }
        // a discount is still possible: the times of the later rides
        // matter, and the two trips board them at the same times only when
        // they are at the stop at the same time
        if !same_time {
            return false;
        }
        let tt = t_type as usize;
        if !self.rule_from[tt] {
            // the next ride that is not free pays its full fare and resets
            // the previous ride; until then nothing else is read
            return true;
        }
        let c = self.calc;
        if c.full(a.last) != c.full(b.last) {
            return false;
        }
        if self.same_route_matters[tt]
            && self.route_class[a.last as usize] != self.route_class[b.last as usize]
        {
            return false;
        }
        self.canonical_deadline(a, t, horizon) == self.canonical_deadline(b, t, horizon)
    }

    /// A lower bound of the total fare of any trip continuing from `s`
    /// with at most `rides_left` more rides.
    pub(crate) fn lower_bound(&self, s: &FareState, rides_left: i64) -> i64 {
        let lb = i64::from(s.paid) + self.min_step * rides_left.max(0);
        if self.calc.cap_positive {
            lb.min(i64::from(self.calc.cap))
        } else {
            lb
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fares::{FarePerRoute, FarePerTransfer, FarePerType, FareStructure};
    use crate::transit::layer::{Pattern, RouteInfo};
    use crate::TransitLayer;

    /// A layer of `n` one-stop patterns, each on its own route, of types
    /// cycling through `types`, with a fare structure of random rules.
    pub(crate) fn random_structure(seed: u64, n_patterns: usize) -> (TransitLayer, FareStructure) {
        let mut rng = crate::options::testing::Rng::new(seed);
        let types = ["A", "B", "C"];
        let mut layer = TransitLayer::default();
        let mut s = FareStructure {
            max_discounted_transfers: rng.below(4) as f64,
            transfer_time_allowance: [0.0, 5.0, 30.0, 120.0, f64::INFINITY][rng.below(5)],
            fare_cap: if rng.below(3) == 0 {
                (rng.below(1500) as f64) / 100.0
            } else {
                f64::INFINITY
            },
            ..FareStructure::default()
        };
        for t in types {
            s.fares_per_type.push(FarePerType {
                fare_type: Some(t.into()),
                unlimited_transfers: rng.below(3) == 0,
                allow_same_route_transfer: rng.below(2) == 0,
                use_route_fare: rng.below(2) == 0,
                fare: (rng.below(600) as f64) / 100.0,
            });
        }
        for a in types {
            for b in types {
                if rng.below(3) > 0 {
                    s.fares_per_transfer.push(FarePerTransfer {
                        first_leg: Some(a.into()),
                        second_leg: Some(b.into()),
                        fare: (rng.below(1300) as f64) / 100.0,
                    });
                }
            }
        }
        for p in 0..n_patterns {
            // two patterns per route, so the same-route rule applies
            let route = format!("R{}", p / 2);
            let t = types[(p / 2) % types.len()];
            if p % 2 == 0 {
                layer.routes.push(RouteInfo {
                    agency_id: Some("ag".into()),
                    route_id: route.clone(),
                    route_type: 3,
                    ..RouteInfo::default()
                });
                s.fares_per_route.push(FarePerRoute {
                    agency_id: Some("ag".into()),
                    agency_name: Some("ag".into()),
                    route_id: Some(route.clone()),
                    route_short_name: Some(route.clone()),
                    route_long_name: Some(route.clone()),
                    mode: Some("BUS".into()),
                    route_fare: (rng.below(700) as f64) / 100.0,
                    fare_type: Some(t.into()),
                });
            }
            layer.patterns.push(Pattern {
                route_id: route,
                route_type: 3,
                ..Pattern::default()
            });
            layer.pattern_route.push((p / 2) as u32);
        }
        (layer, s)
    }

    /// Whenever a state is said to dominate another, it pays no more on
    /// any continuation: the same patterns boarded at the same times (when
    /// both are at the stop at the same time) or at any times (otherwise,
    /// which the rules for that case must survive).
    #[test]
    fn dominance_holds_for_every_continuation() {
        let n_cases: u64 = std::env::var("PEABIRU_FARE_SOAK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(400);
        let mut checked = 0u64;
        for seed in 0..n_cases {
            let (layer, s) = random_structure(seed, 6);
            let calc = FareCalculator::new(&s, &layer).expect("structure");
            let m = FareModel::new(&calc);
            let mut rng = crate::options::testing::Rng::new(seed ^ 0xabcd);
            let allowance = calc.allowance_s.max(60);
            for _ in 0..300 {
                // two prefixes ending at the same clock time t
                let max_rides = 1 + rng.below(4) as i64;
                let t = 20_000 + rng.below(4000) as i32;
                let prefix = |rng: &mut crate::options::testing::Rng, same_last: Option<i32>| {
                    let n = 1 + rng.below(max_rides as usize);
                    let mut st = FareState::default();
                    let mut b = t - rng.below(3 * allowance as usize) as i32 - 1;
                    let mut boards = Vec::new();
                    for _ in 0..n {
                        boards.push(b);
                        b += rng.below(allowance as usize) as i32;
                    }
                    boards.sort_unstable();
                    for (k, &bt) in boards.iter().enumerate() {
                        let p = match (k + 1 == n, same_last) {
                            (true, Some(p)) => p,
                            _ => rng.below(6) as i32,
                        };
                        st = m.step(&st, p, bt.min(t - 1));
                    }
                    (st, n as i64)
                };
                let (a, ra) = prefix(&mut rng, None);
                let (b, rb) = prefix(&mut rng, Some(a.last));
                let same_time = rng.chance(2, 3);
                let horizon = t + 7200;
                let (al, bl) = (max_rides - ra, max_rides - rb);
                if al < bl {
                    continue; // a must be able to ride as much more as b
                }
                if !m.dominates(&a, &b, same_time, t, horizon, al, bl) {
                    continue;
                }
                checked += 1;
                for _ in 0..40 {
                    let len = rng.below(bl.max(0) as usize + 1);
                    let mut fa = a;
                    let mut fb = b;
                    let mut ta = t + 60;
                    let mut tb = t + 60;
                    for _ in 0..len {
                        let p = rng.below(6) as i32;
                        let gap = rng.below(2 * allowance as usize) as i32;
                        tb += gap;
                        ta = if same_time {
                            tb
                        } else {
                            ta + rng.below(2 * allowance as usize) as i32
                        };
                        if tb >= horizon || ta >= horizon {
                            break;
                        }
                        fa = m.step(&fa, p, ta);
                        fb = m.step(&fb, p, tb);
                    }
                    assert!(
                        m.total(&fa) <= m.total(&fb),
                        "seed {seed}: {a:?} dominates {b:?} (same time {same_time}) but pays \
                         {} > {}",
                        m.total(&fa),
                        m.total(&fb)
                    );
                }
            }
        }
        assert!(checked > n_cases * 20, "{checked}");
    }

    /// Building a trip ride by ride gives the calculator's fare.
    #[test]
    fn steps_compose_to_the_calculators_fare() {
        for seed in 0..300u64 {
            let (layer, s) = random_structure(seed, 8);
            let calc = FareCalculator::new(&s, &layer).expect("structure");
            let m = FareModel::new(&calc);
            let mut rng = crate::options::testing::Rng::new(seed ^ 0xfeed);
            for _ in 0..50 {
                let n = 1 + rng.below(5);
                let mut rides = Vec::new();
                let mut t = rng.below(1000) as i32;
                let mut st = FareState::default();
                for _ in 0..n {
                    let p = rng.below(8) as i32;
                    t += rng.below(4000) as i32;
                    rides.push((p, t));
                    st = m.step(&st, p, t);
                }
                let want = calc.fare(&rides).paid;
                assert_eq!(m.total(&st), want, "seed {seed} rides {rides:?}");
            }
        }
    }
}
