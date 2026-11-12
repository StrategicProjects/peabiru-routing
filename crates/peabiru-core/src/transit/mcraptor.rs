//! R5 7.6's multi-criteria router, `McRaptorSuboptimalPathProfileRouter`,
//! ported literally (tie-breaking and iteration orders included), with the
//! dominating lists r5r uses with it: R5's `FareDominatingList` (time and
//! fare, with r5r's fare calculator) and r5r's `TripPlanner` wrapper of
//! `SuboptimalDominatingList` (time, with `suboptimal_minutes`).
//!
//! It is not range RAPTOR: it samples `monte_carlo_draws` departure times
//! in the window by a constrained random walk (R5's
//! `generateDepartureTimesToSample`, with r5r's seeding of the generator),
//! and runs a full search from each. Every state is kept as an object with
//! a back pointer, as R5 does; the states of a draw live in an arena that
//! the next draw reuses.
//!
//! What differs from R5 is only how the same decisions are computed (each
//! difference is argued where it is made):
//!
//! - the dominating lists hold, beside each state, the values the
//!   comparisons read ([`Entry`]), and a removal compacts the list in place;
//! - a state's fare comes from the fare variables of the state behind it
//!   ([`FareCalculator::ride`]) instead of from all its rides, and is
//!   worked out once per boarding rather than once per stop alighted at;
//! - a state that no list accepts is not kept (R5 leaves it to the garbage
//!   collector);
//! - `bestStatesBeforeRound` keeps only the states a round can board with.

use super::layer::TransitLayer;
use super::raptor::{route_type_mode, Offsets, TransitMode, UNREACHED};
use crate::fares::{FareBounds, FareCalculator, FareProgress};
use crate::jcompat::{double_hash, objects_hash, MersenneTwister};
use crate::CancelToken;

/// No state.
pub(crate) const NONE: u32 = u32::MAX;

/// `McRaptorSuboptimalPathProfileRouter.BOARD_SLACK`.
const BOARD_SLACK: i32 = 60;

/// R5's `McRaptorState`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct McState {
    pub(crate) back: u32,
    pub(crate) time: i32,
    pub(crate) board_time: i32,
    pub(crate) pattern: i32,
    #[allow(dead_code)]
    pub(crate) trip: i32,
    #[allow(dead_code)]
    pub(crate) round: i32,
    pub(crate) stop: i32,
    pub(crate) board_pos: i32,
    pub(crate) alight_pos: i32,
    pub(crate) fare: Option<FareBounds>,
}

/// Which dominating list the state bags use.
pub(crate) enum Dominance<'a> {
    /// R5's `FareDominatingList` with r5r's calculator.
    Fare(&'a FareCalculator),
    /// r5r `TripPlanner`'s `SuboptimalDominatingList` wrapper: states more
    /// than `max_duration` after their draw's departure are dropped and,
    /// with `shortest`, the limit shrinks to the best arrival at the
    /// destination found so far (shared by every list, across draws).
    Suboptimal { max_duration: i32, shortest: bool },
}

/// What a search needs besides the network (R5's `ProfileRequest`).
pub(crate) struct McRequest {
    pub(crate) from_time: i32,
    pub(crate) to_time: i32,
    /// `maxTripDurationMinutes * 60`.
    pub(crate) max_trip_s: i32,
    pub(crate) max_rides: usize,
    pub(crate) suboptimal_minutes: i32,
    /// `request.maxFare`, cents; below 0 turns on R5's target pruning.
    pub(crate) max_fare: i32,
    pub(crate) monte_carlo_draws: i32,
    /// `(double) (request.walkSpeed * 1000)`, mm/s.
    pub(crate) walk_mms: f64,
    /// r5r's `seed` for the frequency offsets (`None`: from the clock).
    pub(crate) seed: Option<i32>,
    /// `seed` used for the departure-time sample too (r5r's
    /// `seedOffsets(router, seed, ...)`): the matrices pass r5r's seed,
    /// `TripPlanner` passes `null` for both.
    pub(crate) origin: (f64, f64),
}

/// A state bag (`McRaptorStateBag`): the best states, and those whose last
/// step was not a walk between stops.
#[derive(Default, Clone)]
struct Bag {
    best: Vec<Entry>,
    nt: Vec<Entry>,
}

/// A state in a dominating list, with what the list's comparisons read of
/// it. The values are copies of the state's own (`time`, `round`) and of
/// its fare (`paid`, `value`, `expiration`, `number`; `consumed` is
/// `paid - value`), none of which changes once the state is made; with the
/// suboptimal list the fare values are 0 and not read.
#[derive(Debug, Clone, Copy)]
struct Entry {
    id: u32,
    time: i32,
    round: i32,
    paid: i32,
    consumed: i32,
    value: i32,
    expiration: i32,
    number: i32,
}

/// The fare of the states that alight from one boarding, once worked out:
/// what the calculator returns and its variables.
type Memo = Option<(FareBounds, FareProgress)>;

/// The pattern scratch of a state on board (`tripIndicesInPattern`,
/// `boardStopPositionInPattern`, `boardTimeForFrequencyTrips`), and the
/// fare of the states that alight from that boarding.
#[derive(Debug, Clone, Copy)]
struct Slot {
    trip: i32,
    board_pos: i32,
    freq_time: i32,
    /// `None` after every `put`.
    memo: Memo,
}

/// A state on board along a pattern with a [`Timetable`]: what the scratch
/// maps hold for it, beside it.
#[derive(Debug, Clone, Copy)]
struct Rider {
    /// The state, and its time.
    s: u32,
    s_time: i32,
    /// Its trip among the timetable's running trips, where it boarded and
    /// when that trip leaves there.
    k: u32,
    board_pos: u32,
    board_time: i32,
    /// It boarded at the pattern's last visit to its stop.
    at_last_visit: bool,
    /// A state that boarded the same trip before it, at the last visit to
    /// its own stop, offers states no worse than this one's at every stop
    /// (see [`McRaptor::ride_timetable`]).
    shadowed: bool,
    memo: Memo,
}

/// Where the fare of a new state comes from (see [`McRaptor::fare_entry`]).
enum FareFrom<'m> {
    /// No state behind it: no rides.
    Access,
    /// Not off a vehicle: the rides, hence the fare, of the state behind
    /// it (given here).
    Walk(FareBounds),
    /// Off a vehicle: the rides of the state behind it and one more; the
    /// memo of the boarding.
    Ride(&'m mut Memo),
}

impl Entry {
    /// The fare of the entry's state (with the fare list).
    fn fare(&self) -> FareBounds {
        FareBounds {
            paid: self.paid,
            value: self.value,
            number: self.number,
            expiration: self.expiration,
        }
    }
}

/// `DominatingList.add` of R5's two lists, which differ in `dominates`
/// alone: `new` is refused, and the list left as it is at that point, at
/// the first state of the list that dominates it; the states before that
/// one which `new` dominates are removed; `new` goes to the end when no
/// state refuses it.
///
/// R5 (and the first version of this port) removes each state at once,
/// shifting the rest of the list down, then goes on from the same index.
/// Here the states that stay are moved down as the scan goes (`w` is where
/// the next one goes, `r` the next to examine), and a refusal moves the
/// states not yet examined down as well. Both examine every state once, in
/// the list's order, with the same two tests on the same values, and leave
/// the survivors in their order followed by the unexamined ones: the same
/// list, and the same answer.
#[inline(always)]
fn list_add(list: &mut Vec<Entry>, new: Entry, dominates: impl Fn(&Entry, &Entry) -> bool) -> bool {
    let n = list.len();
    let mut w = 0;
    for r in 0..n {
        let old = list[r];
        if dominates(&old, &new) {
            if w != r {
                list.copy_within(r..n, w);
                list.truncate(n - (r - w));
            }
            return false;
        }
        if !dominates(&new, &old) {
            if w != r {
                list[w] = old;
            }
            w += 1;
        }
    }
    list.truncate(w);
    list.push(new);
    true
}

/// `add` of r5r `TripPlanner`'s wrapper of `SuboptimalDominatingList`: a
/// state more than `max_duration` after the draw's `departure` is refused;
/// one that the list accepts shrinks `max_duration` to its own duration
/// when `shrink` (r5r: the shortest path is asked for and the state is at
/// the destination).
fn suboptimal_add(
    list: &mut Vec<Entry>,
    new: Entry,
    suboptimal_minutes: i32,
    departure: i32,
    max_duration: &mut i32,
    shrink: bool,
) -> bool {
    if new.time.wrapping_sub(departure) > *max_duration {
        return false;
    }
    // SuboptimalDominatingList.dominates(a, b): one access mode
    let threshold = suboptimal_minutes.wrapping_mul(60);
    let dominates = |a: &Entry, b: &Entry| {
        (a.round < b.round && a.time <= b.time) || a.time.wrapping_add(threshold) < b.time
    };
    if !list_add(list, new, dominates) {
        return false;
    }
    if shrink {
        *max_duration = (*max_duration).min(new.time.wrapping_sub(departure));
    }
    true
}

/// [`fare_better_or_equal`] of two states of the same time, from their
/// fares.
#[inline(always)]
fn fare_no_worse(a: &FareBounds, b: &FareBounds) -> bool {
    a.paid <= b.paid.wrapping_sub(b.value)
        || (a.paid <= b.paid
            && a.value >= b.value
            && a.expiration >= b.expiration
            && a.number >= b.number)
}

/// `FareDominatingList.betterOrEqual(a, b)`.
#[inline(always)]
fn fare_better_or_equal(a: &Entry, b: &Entry) -> bool {
    a.time <= b.time
        && (a.paid <= b.consumed
            || (a.paid <= b.paid
                && a.value >= b.value
                && a.expiration >= b.expiration
                && a.number >= b.number))
}

/// A bit set over stops or patterns.
#[derive(Clone)]
struct Bits(Vec<u64>);

impl Bits {
    fn new(n: usize) -> Self {
        Bits(vec![0; n.div_ceil(64)])
    }
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    fn clear(&mut self) {
        self.0.iter_mut().for_each(|w| *w = 0);
    }
    fn is_empty(&self) -> bool {
        self.0.iter().all(|&w| w == 0)
    }
    fn and(&mut self, o: &Bits) {
        for (a, b) in self.0.iter_mut().zip(&o.0) {
            *a &= b;
        }
    }
    fn or(&mut self, o: &Bits) {
        for (a, b) in self.0.iter_mut().zip(&o.0) {
            *a |= b;
        }
    }
    fn ones(&self) -> Vec<usize> {
        let mut v = Vec::new();
        for (wi, &w) in self.0.iter().enumerate() {
            let mut w = w;
            while w != 0 {
                v.push(wi * 64 + w.trailing_zeros() as usize);
                w &= w - 1;
            }
        }
        v
    }
}

/// The router. Build it once per origin (it holds the per-draw state).
pub(crate) struct McRaptor<'a> {
    layer: &'a TransitLayer,
    transfers: &'a [Vec<(u32, i32)>],
    /// Per pattern: running on the date and of an allowed mode.
    allowed: &'a [bool],
    /// Per service: running on the date.
    active: &'a [bool],
    schedules: &'a Schedules,
    req: &'a McRequest,
    dom: Dominance<'a>,
    /// Egress `(stop, seconds)` in R5's iteration order, for a
    /// point-to-point search; with `egress_time` per stop.
    egress: Option<&'a [(u32, i32)]>,
    egress_time: Vec<i32>,
    /// `patternsNearDestination`: the patterns of the egress stops.
    near_destination: Bits,
    offsets: Offsets,
    mt: MersenneTwister,
    // per draw
    arena: Vec<McState>,
    /// Parallel to `arena`: with the fare list, the fare variables after
    /// each state's rides.
    progress: Vec<FareProgress>,
    /// A bag per stop; `has_bag` tells the stops whose bag the draw has
    /// made (`with_bag`, in the order they were made). The lists of the
    /// other stops are empty: a draw empties those it made instead of
    /// freeing them, so the next draw does not allocate them again.
    bags: Vec<Bag>,
    has_bag: Vec<bool>,
    with_bag: Vec<u32>,
    /// `bestStatesBeforeRound`, of every stop one after the other: the
    /// `(state, time)` of the best states of the round before the current
    /// one, in list order; `before_range` has each stop's part (empty for
    /// a stop without a bag at the start of the round).
    before: Vec<(u32, i32)>,
    before_range: Vec<(u32, u32)>,
    /// `do_transfers`' stops reached by a transfer.
    by_transfer: Bits,
    /// The states on board along a pattern.
    on_board: Vec<u32>,
    /// The same along a pattern with a timetable.
    riders: Vec<Rider>,
    /// With the fare list, [`FareCalculator::exact_within`] the rides of
    /// a search: `ride_timetable` may then leave shadowed states behind.
    exact_fares: bool,
    /// Per running trip of the pattern being ridden: the fares offered by
    /// the states on board that can shadow others (none of them no worse
    /// than another); `shadow_trips` lists the trips with any.
    shadow: Vec<Vec<FareBounds>>,
    shadow_trips: Vec<u32>,
    touched_stops: Bits,
    touched_patterns: Bits,
    round: i32,
    departure: i32,
    best_at_target: i32,
    /// `Suboptimal`'s limit, shared by every list.
    max_duration: i32,
    /// Per pattern scratch: the latest (trip, board position, board time)
    /// put for a state, by arena index.
    slot: Vec<Slot>,
}

/// The patterns a search may ride: running on a service active on the
/// date (`pattern.servicesActive.intersects(servicesActive)`) and of one of
/// `modes`. Errors on a route type R5 does not know.
pub(crate) fn allowed_patterns(
    layer: &TransitLayer,
    active: &[bool],
    modes: &[TransitMode],
) -> Result<Vec<bool>, String> {
    layer
        .patterns
        .iter()
        .map(|p| {
            let m = route_type_mode(p.route_type)
                .ok_or_else(|| format!("unsupported GTFS route_type {}", p.route_type))?;
            Ok(m.is_some_and(|m| modes.contains(&m))
                && p.services.iter().any(|&s| active[s as usize]))
        })
        .collect()
}

/// The times of a pattern's trips, laid out for the search.
struct Timetable {
    /// The trips running on the date, as indices into `pattern.trips`, in
    /// its order.
    trips: Vec<u32>,
    /// `departures[pos * trips.len() + k]`: when the `k`-th running trip
    /// leaves the pattern's stop at `pos`; `arrivals` likewise.
    departures: Vec<i32>,
    arrivals: Vec<i32>,
    /// Per running trip: the first position from which its arrivals never
    /// decrease to the end of the pattern (0 for a trip whose times are in
    /// order).
    settled: Vec<u32>,
    /// Per position: the pattern does not come back to this stop later.
    last_visit: Vec<bool>,
    /// Per position: the pattern has been at this stop before.
    revisit: Vec<bool>,
}

/// The timetables of the patterns a search may ride that run by schedule
/// only: the same numbers as `pattern.trips[..].departures` and
/// `.arrivals`, copied stop by stop so that the trips at one stop are side
/// by side (R5, and the search without a timetable, follow a pointer per
/// trip). Made once per request and shared by its searches.
pub(crate) struct Schedules(Vec<Option<Timetable>>);

impl Schedules {
    /// `allowed` and `active` as [`McRaptor::new`] takes them.
    pub(crate) fn new(layer: &TransitLayer, allowed: &[bool], active: &[bool]) -> Self {
        Schedules(
            layer
                .patterns
                .iter()
                .enumerate()
                .map(|(pi, p)| {
                    let n = p.stops.len();
                    // only where the search reads a time of every trip at
                    // every stop: a pattern it may ride, by schedule alone
                    // (so that no trip is a frequency trip), each trip with
                    // a time per stop
                    if !allowed[pi]
                        || !p.has_schedules
                        || p.has_frequencies
                        || p.trips.iter().any(|t| {
                            t.is_frequency() || t.departures.len() != n || t.arrivals.len() != n
                        })
                    {
                        return None;
                    }
                    let trips: Vec<u32> = (0..p.trips.len() as u32)
                        .filter(|&t| active[p.trips[t as usize].service as usize])
                        .collect();
                    let mut departures = Vec::with_capacity(n * trips.len());
                    let mut arrivals = Vec::with_capacity(n * trips.len());
                    for pos in 0..n {
                        for &t in &trips {
                            departures.push(p.trips[t as usize].departures[pos]);
                            arrivals.push(p.trips[t as usize].arrivals[pos]);
                        }
                    }
                    let settled = trips
                        .iter()
                        .map(|&t| {
                            let a = &p.trips[t as usize].arrivals;
                            let mut from = n.saturating_sub(1);
                            while from > 0 && a[from - 1] <= a[from] {
                                from -= 1;
                            }
                            from as u32
                        })
                        .collect();
                    let last_visit = (0..n)
                        .map(|pos| !p.stops[pos + 1..].contains(&p.stops[pos]))
                        .collect();
                    let revisit = (0..n)
                        .map(|pos| p.stops[..pos].contains(&p.stops[pos]))
                        .collect();
                    Some(Timetable {
                        trips,
                        departures,
                        arrivals,
                        settled,
                        last_visit,
                        revisit,
                    })
                })
                .collect(),
        )
    }
}

/// What a draw leaves for the caller.
pub(crate) enum DrawOut<'s> {
    /// Collated travel time to every stop (R5's `collateTravelTimes`):
    /// seconds from the departure, `UNREACHED` where none.
    Times(&'s [i32]),
    /// The states at the destination (`doPropagationToDestination`), as
    /// chains read from the arena.
    Final(&'s [McState], &'s [u32]),
}

impl<'a> McRaptor<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        layer: &'a TransitLayer,
        transfers: &'a [Vec<(u32, i32)>],
        allowed: &'a [bool],
        active: &'a [bool],
        schedules: &'a Schedules,
        req: &'a McRequest,
        dom: Dominance<'a>,
        egress: Option<&'a [(u32, i32)]>,
        departure_seed: i64,
    ) -> Self {
        let n_stops = layer.stop_ids.len();
        let offsets_mt = match req.seed {
            Some(seed) => MersenneTwister::from_long(i64::from(objects_hash(&[
                seed,
                double_hash(req.origin.0),
                double_hash(req.origin.1),
            ]))),
            None => MersenneTwister::from_long(crate::sys::clock_seed()),
        };
        let mut egress_time = Vec::new();
        let mut near_destination = Bits::new(layer.patterns.len());
        if let Some(e) = egress {
            egress_time = vec![-1; n_stops];
            for &(s, t) in e {
                egress_time[s as usize] = t;
                for &p in &layer.patterns_for_stop[s as usize] {
                    near_destination.set(p as usize);
                }
            }
        }
        let max_duration = match dom {
            Dominance::Suboptimal { max_duration, .. } => max_duration,
            Dominance::Fare(_) => i32::MAX,
        };
        let exact_fares = match dom {
            Dominance::Fare(calc) => calc.exact_within(req.max_rides),
            Dominance::Suboptimal { .. } => false,
        };
        McRaptor {
            layer,
            transfers,
            allowed,
            active,
            schedules,
            req,
            dom,
            egress,
            egress_time,
            near_destination,
            offsets: Offsets::new(layer, offsets_mt),
            mt: MersenneTwister::from_long(departure_seed),
            arena: Vec::new(),
            progress: Vec::new(),
            bags: vec![Bag::default(); n_stops],
            has_bag: vec![false; n_stops],
            with_bag: Vec::new(),
            before: Vec::new(),
            before_range: vec![(0, 0); n_stops],
            by_transfer: Bits::new(n_stops),
            on_board: Vec::new(),
            riders: Vec::new(),
            exact_fares,
            shadow: Vec::new(),
            shadow_trips: Vec::new(),
            touched_stops: Bits::new(n_stops),
            touched_patterns: Bits::new(layer.patterns.len()),
            round: 0,
            departure: 0,
            best_at_target: i32::MAX,
            max_duration,
            slot: Vec::new(),
        }
    }

    /// r5r's seed of the departure-time sample
    /// (`R5TravelTimeComputer.seedOffsets`): `Objects.hash(fromLat,
    /// fromLon)` without a seed, `Objects.hash(seed, fromLat, fromLon, 1)`
    /// with one.
    pub(crate) fn departure_seed(seed: Option<i32>, (lat, lon): (f64, f64)) -> i64 {
        let h = match seed {
            None => objects_hash(&[double_hash(lat), double_hash(lon)]),
            Some(s) => objects_hash(&[s, double_hash(lat), double_hash(lon), 1]),
        };
        i64::from(h)
    }

    /// `generateDepartureTimesToSample`, repeated until it gives exactly
    /// `monte_carlo_draws` times, as R5 does (an error rather than R5's
    /// endless loop when that cannot happen).
    fn departure_times(&mut self) -> Result<Vec<i32>, String> {
        let r = self.req;
        if r.monte_carlo_draws == 200 {
            return Err(
                "Monte Carlo draws set to UI default, this is probably not what you want \
                        (R5's McRAPTOR refuses exactly 200 draws)"
                    .into(),
            );
        }
        if r.monte_carlo_draws <= 0 {
            return Err("McRAPTOR needs at least one Monte Carlo draw".into());
        }
        let mean = (r.to_time - r.from_time) / r.monte_carlo_draws;
        if mean < 2 {
            return Err(format!(
                "McRAPTOR cannot sample {} departure times in a window of {} s (R5 would \
                 fail or loop forever)",
                r.monte_carlo_draws,
                r.to_time - r.from_time
            ));
        }
        let half = mean / 2;
        for _ in 0..1_000_000 {
            let mut v = Vec::new();
            let mut t = r.from_time + self.mt.next_int(mean);
            while t < r.to_time {
                v.push(t);
                t += self.mt.next_int(mean) + half;
            }
            if v.len() == r.monte_carlo_draws as usize {
                return Ok(v);
            }
        }
        Err(format!(
            "R5's McRAPTOR cannot sample exactly {} departure times in this window: it would \
             retry forever",
            r.monte_carlo_draws
        ))
    }

    /// `route()`: every draw in turn, handing `sink(departure, out)` the
    /// draw's result. Errors are R5's exceptions.
    pub(crate) fn route(
        &mut self,
        access: &[(u32, i32)],
        cancel: &CancelToken,
        sink: impl FnMut(i32, DrawOut<'_>),
    ) -> Result<(), String> {
        let deps = self.departure_times()?;
        self.route_departures(&deps, access, cancel, sink)
    }

    /// [`McRaptor::route`]'s draws from the departure times `deps` (for
    /// comparing with it at chosen times).
    pub(crate) fn route_departures(
        &mut self,
        deps: &[i32],
        access: &[(u32, i32)],
        cancel: &CancelToken,
        mut sink: impl FnMut(i32, DrawOut<'_>),
    ) -> Result<(), String> {
        let n_stops = self.layer.stop_ids.len();
        let mut times = vec![UNREACHED; n_stops];
        let mut final_states = Vec::new();
        for &dep in deps {
            if cancel.is_cancelled() {
                return Ok(());
            }
            self.departure = dep;
            self.offsets.randomize(self.layer);
            for &s in &self.with_bag {
                let bag = &mut self.bags[s as usize];
                bag.best.clear();
                bag.nt.clear();
                self.has_bag[s as usize] = false;
                self.before_range[s as usize] = (0, 0);
            }
            self.with_bag.clear();
            self.arena.clear();
            self.progress.clear();
            self.touched_patterns.clear();
            self.touched_stops.clear();
            // r5r's TripPlanner.AccessTimes: target pruning per draw
            self.best_at_target = i32::MAX;
            self.round = 0;
            for &(stop, t) in access {
                let from = FareFrom::Access;
                if self.add_state(stop, -1, -1, dep.wrapping_add(t), -1, -1, -1, NONE, 0, from)? {
                    self.touched_stops.set(stop as usize);
                }
            }
            self.mark_patterns();
            self.round += 1;
            while self.do_one_round()? && self.round < self.req.max_rides as i32 + 1 {}
            if self.egress.is_some() {
                self.propagate_to_destination(&mut final_states);
                sink(dep, DrawOut::Final(&self.arena, &final_states));
            } else {
                self.collate(&mut times);
                sink(dep, DrawOut::Times(&times));
            }
        }
        Ok(())
    }

    fn mark_patterns(&mut self) {
        self.touched_patterns.clear();
        for stop in self.touched_stops.ones() {
            for &p in &self.layer.patterns_for_stop[stop] {
                self.touched_patterns.set(p as usize);
            }
        }
        self.touched_stops.clear();
    }

    /// The entry of a new state for the fare list, with its fare, or `None`
    /// when `FareDominatingList.add` refuses it before looking at the list:
    /// too late for the draw, or over `max_fare`. R5 makes these two tests
    /// in each list the state is offered to (`best`, then `nonTransfer`);
    /// they read only the state and the request, so both lists answer alike
    /// and neither changes.
    ///
    /// The fare is `calculateFare` of the state's rides. R5 hands the
    /// calculator the whole chain of the state; the fare is a function of
    /// the rides alone (their patterns and board times, in order), so:
    ///
    /// - a state with no state behind it has no rides: `fare(&[])`;
    /// - a state that is not off a vehicle has the rides of the state
    ///   behind it, hence its fare (which the caller reads from that
    ///   state's list entry, a copy of it);
    /// - a state off a vehicle has the rides of the state behind it and one
    ///   more, `(pattern, board_time)`: [`FareCalculator::ride`] from the
    ///   variables kept for that state, and [`FareCalculator::bounds`].
    ///   The states alighting along a pattern from the same boarding have
    ///   the same state behind, pattern and board time, hence the same
    ///   fare: it is worked out for the first that needs it and kept in
    ///   the boarding's memo (emptied whenever the boarding changes).
    ///
    /// Every state behind another was accepted by a list, so its variables
    /// are in `progress` (`add_state` keeps only accepted states, each
    /// with its variables).
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn fare_entry(
        &self,
        calc: &FareCalculator,
        id: u32,
        time: i32,
        round: i32,
        pattern: i32,
        board_time: i32,
        back: u32,
        from: &mut FareFrom<'_>,
    ) -> Option<(Entry, FareBounds)> {
        let max_clock = self.departure.wrapping_add(self.req.max_trip_s);
        if time > max_clock {
            return None;
        }
        let f = match from {
            FareFrom::Access => calc.bounds(&FareProgress::default()),
            FareFrom::Walk(f) => *f,
            FareFrom::Ride(memo) => match memo {
                Some((f, _)) => *f,
                None => {
                    let p = calc.ride(&self.progress[back as usize], pattern, board_time);
                    let f = calc.bounds(&p);
                    **memo = Some((f, p));
                    f
                }
            },
        };
        let consumed = f.paid.wrapping_sub(f.value);
        if consumed > self.req.max_fare {
            return None;
        }
        let e = Entry {
            id,
            time,
            round,
            paid: f.paid,
            consumed,
            value: f.value,
            expiration: f.expiration,
            number: f.number,
        };
        Some((e, f))
    }

    /// The fare variables of a state whose entry [`McRaptor::fare_entry`]
    /// made from `from`.
    fn progress_from(&self, back: u32, from: &FareFrom<'_>) -> FareProgress {
        match from {
            FareFrom::Access => FareProgress::default(),
            FareFrom::Walk(_) => self.progress[back as usize],
            FareFrom::Ride(memo) => memo.expect("fare_entry").1,
        }
    }

    /// `addState`. `back_time` is the time of the state `back` (not read
    /// without one); `from` gives the state's fare with the fare list.
    ///
    /// R5 makes the state, then offers it to the lists. Here the lists are
    /// offered its entry first and the state is kept (at the index the
    /// entry already names, the next of the arena) only when one accepts
    /// it: a refused state is in no list, nothing points back to it and no
    /// result reads it.
    #[allow(clippy::too_many_arguments)]
    fn add_state(
        &mut self,
        stop: u32,
        board_pos: i32,
        alight_pos: i32,
        time: i32,
        board_time: i32,
        pattern: i32,
        trip: i32,
        back: u32,
        back_time: i32,
        mut from: FareFrom<'_>,
    ) -> Result<bool, String> {
        let r = self.req;
        if time > r.to_time.wrapping_add(r.max_trip_s) {
            return Ok(false);
        }
        if r.max_fare < 0
            && time.wrapping_sub(r.suboptimal_minutes.wrapping_mul(60)) > self.best_at_target
        {
            return Ok(false);
        }
        if back != NONE && back_time > time {
            return Err("Attempt to decrement time in state! (R5 McRAPTOR)".into());
        }
        let id = self.arena.len() as u32;
        let si = stop as usize;
        if !self.has_bag[si] {
            self.has_bag[si] = true;
            self.with_bag.push(stop);
        }
        let round = self.round;
        let mut fare = None;
        let mut progress = FareProgress::default();
        let optimal = match self.dom {
            Dominance::Fare(calc) => {
                match self.fare_entry(calc, id, time, round, pattern, board_time, back, &mut from) {
                    None => false,
                    Some((e, f)) => {
                        let bag = &mut self.bags[si];
                        let mut optimal = list_add(&mut bag.best, e, fare_better_or_equal);
                        if pattern != -1 && list_add(&mut bag.nt, e, fare_better_or_equal) {
                            optimal = true;
                        }
                        if optimal {
                            fare = Some(f);
                            progress = self.progress_from(back, &from);
                        }
                        optimal
                    }
                }
            }
            Dominance::Suboptimal { .. } => {
                let e = Entry {
                    id,
                    time,
                    round,
                    paid: 0,
                    consumed: 0,
                    value: 0,
                    expiration: 0,
                    number: 0,
                };
                // never at the destination (which has no stop): the limit
                // does not shrink here
                let (sub, dep) = (r.suboptimal_minutes, self.departure);
                let (bag, limit) = (&mut self.bags[si], &mut self.max_duration);
                let mut optimal = suboptimal_add(&mut bag.best, e, sub, dep, limit, false);
                if pattern != -1 && suboptimal_add(&mut bag.nt, e, sub, dep, limit, false) {
                    optimal = true;
                }
                optimal
            }
        };
        if optimal {
            self.arena.push(McState {
                back,
                time,
                board_time,
                pattern,
                trip,
                round,
                stop: stop as i32,
                board_pos,
                alight_pos,
                fare,
            });
            self.progress.push(progress);
        }
        if self.egress.is_some() && optimal && pattern != -1 {
            let e = self.egress_time[si];
            if e != -1 {
                let at = time.wrapping_add(e);
                if at < self.best_at_target {
                    self.best_at_target = at;
                }
            }
        }
        Ok(optimal)
    }

    /// `doOneRound`: `true` when a pattern is to be explored next round.
    fn do_one_round(&mut self) -> Result<bool, String> {
        // bestStatesBeforeRound: a copy of every bag's best states. R5
        // copies each list whole and, when boarding, skips the states
        // whose round is not the one before this (before anything else is
        // done with them); the copy here leaves those out, and takes the
        // time, the one other value boarding reads of a state. A stop
        // whose bag is made during the round has no copy in R5 and an
        // empty one here (`before_range` is emptied for every stop at the
        // start of the draw and written only for the stops with a bag):
        // nothing boards there either way.
        let mut before = std::mem::take(&mut self.before);
        before.clear();
        let previous = self.round - 1;
        for &s in &self.with_bag {
            let start = before.len() as u32;
            for e in &self.bags[s as usize].best {
                if e.round == previous {
                    before.push((e.id, e.time));
                }
            }
            self.before_range[s as usize] = (start, before.len() as u32);
        }
        let mut states = std::mem::take(&mut self.on_board);
        let done = self.ride_patterns(&before, &mut states);
        self.before = before;
        self.on_board = states;
        done?;
        self.do_transfers()?;
        self.mark_patterns();
        self.round += 1;
        Ok(!self.touched_patterns.is_empty())
    }

    /// The patterns of `doOneRound`.
    fn ride_patterns(
        &mut self,
        before: &[(u32, i32)],
        states: &mut Vec<u32>,
    ) -> Result<(), String> {
        // on the last round of a point-to-point search, only the patterns
        // near the destination
        if self.round == self.req.max_rides as i32 && self.egress.is_some() {
            self.touched_patterns.and(&self.near_destination);
        }
        let layer = self.layer;
        for pat in self.touched_patterns.ones() {
            if !self.allowed[pat] {
                continue;
            }
            let pattern = &layer.patterns[pat];
            states.clear();
            if let Some(table) = &self.schedules.0[pat] {
                let mut riders = std::mem::take(&mut self.riders);
                let done = self.ride_timetable(pat, table, before, &mut riders);
                self.riders = riders;
                done?;
                continue;
            }
            for (pos, &stop) in pattern.stops.iter().enumerate() {
                // get off the bus
                for &s in states.iter() {
                    let Slot {
                        trip,
                        board_pos,
                        freq_time,
                        ..
                    } = self.slot[s as usize];
                    let sched = &pattern.trips[trip as usize];
                    let (arrival, board_time) = if sched.is_frequency() {
                        let travel =
                            sched.arrivals[pos].wrapping_sub(sched.departures[board_pos as usize]);
                        (freq_time.wrapping_add(travel), freq_time)
                    } else {
                        (sched.arrivals[pos], sched.departures[board_pos as usize])
                    };
                    // the memo of this boarding: `put` emptied it, and the
                    // pattern and the board time are the same until the
                    // next `put` for the state (the board time comes from
                    // the slot)
                    let mut memo = self.slot[s as usize].memo;
                    let back_time = self.arena[s as usize].time;
                    let from = FareFrom::Ride(&mut memo);
                    let added = self.add_state(
                        stop, board_pos, pos as i32, arrival, board_time, pat as i32, trip, s,
                        back_time, from,
                    );
                    self.slot[s as usize].memo = memo;
                    if added? {
                        self.touched_stops.set(stop as usize);
                    }
                }
                // get on the bus
                let (from, to) = self.before_range[stop as usize];
                for &(s, s_time) in &before[from as usize..to as usize] {
                    if pattern.has_frequencies && pattern.has_schedules {
                        return Err("McRAPTOR router does not support frequencies and \
                                    schedules in the same trip pattern!"
                            .into());
                    }
                    if pattern.has_schedules {
                        for (ti, t) in pattern.trips.iter().enumerate() {
                            if !self.active[t.service as usize] {
                                continue;
                            }
                            if t.departures[pos] > s_time.wrapping_add(BOARD_SLACK) {
                                states.push(s);
                                self.put(s, (ti as i32, pos as i32, 0));
                                break;
                            }
                        }
                    } else if pattern.has_frequencies {
                        for (ti, t) in pattern.trips.iter().enumerate() {
                            if !self.active[t.service as usize] {
                                continue;
                            }
                            let earliest = s_time.wrapping_add(BOARD_SLACK);
                            for (e, f) in t.frequencies.iter().enumerate() {
                                let mut departure = f
                                    .start
                                    .wrapping_add(self.offsets.offsets[pat][ti][e])
                                    .wrapping_add(t.departures[pos]);
                                let latest = f.end.wrapping_add(t.departures[pos]);
                                if earliest > latest {
                                    continue;
                                }
                                if f.headway <= 0 {
                                    return Err(format!(
                                        "frequency of trip {} has headway {}",
                                        t.trip_id, f.headway
                                    ));
                                }
                                while departure < earliest {
                                    departure = departure.wrapping_add(f.headway);
                                }
                                if departure > latest {
                                    continue;
                                }
                                states.push(s);
                                self.put(s, (ti as i32, pos as i32, departure));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// One pattern of [`McRaptor::ride_patterns`] from its timetable: what
    /// the loop there does for a pattern that runs by schedule only (the
    /// frequency branches and the mixed-pattern error cannot be reached
    /// for it), reading the same times from the timetable.
    ///
    /// - Getting off: every trip is scheduled, so the state arrives at
    ///   `trip.arrivals[pos]` having boarded at
    ///   `trip.departures[board_pos]`; these are `arrivals[pos * n + k]`
    ///   and `departures[board_pos * n + k]` for the `k`-th running trip.
    /// - Getting on: R5 takes the first trip of the pattern, in its order,
    ///   that runs on the date and leaves the stop after the state's time
    ///   plus the slack. The timetable has the running trips in that
    ///   order: the first `k` with `departures[pos * n + k]` after it.
    ///
    /// R5 keeps the trip and the board position of the states on board in
    /// maps keyed by the state, where a later boarding of the same state
    /// replaces an earlier one, for every copy of the state on board. A
    /// state boards only at its own stop (the candidates at a position are
    /// the best states of that position's stop, each once), so that
    /// happens only where the pattern comes back to a stop. Here each
    /// state on board carries its own trip and board position
    /// ([`Rider`]), and a boarding at such a position rewrites those of
    /// the state's earlier copies, with their fare memo emptied.
    ///
    /// A state on board is left behind once it can only be refused for
    /// the rest of the pattern. R5 keeps it on board to the last stop and
    /// offers a new state at every stop. The state on board `s` is dropped
    /// at `pos` when all of these hold:
    ///
    /// 1. it boarded at the pattern's last visit to its stop, so the state
    ///    does not board again further on and its trip `k` (and the fare
    ///    of what it offers, its memo) is the same at every later
    ///    position. (A state that boarded at an earlier visit of a stop
    ///    the pattern comes back to stays on board whatever its time: a
    ///    second boarding there would change its trip.)
    /// 2. `pos >= settled[k]`: the arrivals of trip `k` do not decrease
    ///    from `pos` to the end of the pattern, so the state offered at
    ///    `pos` and at every later position `q` has a time
    ///    `arrivals[q * n + k] >= arrivals[pos * n + k]`.
    /// 3. one of:
    ///    - (a) its arrival at `pos` is later than `limit` (the end of the
    ///      window plus the longest trip, the same for the whole search):
    ///      `add_state` refuses such a state in its first statement;
    ///    - (b) with the fare list, its arrival at `pos` is not before the
    ///      time of `s` and either is later than the draw's `max_clock`
    ///      or the fare of what it offers, already worked out, is over
    ///      `max_fare`. `add_state` then, for the state offered at `pos`
    ///      or later: may refuse it in its first two tests (too late, or
    ///      target pruning); does not fail the test on going back in time;
    ///      makes the stop's bag if it has none; and gets no entry from
    ///      [`McRaptor::fare_entry`], which refuses on these same two
    ///      grounds (the time, or the same fare against the same limit)
    ///      before any list is looked at;
    ///    - (c) with the fare list and exact fares, its arrival at `pos` is
    ///      not before the time of `s` and it is shadowed: a state `a`
    ///      boarded the same trip `k` before it (it comes earlier in the
    ///      order of the states on board), at the last visit to its own
    ///      stop, and the fare `a` offers is no worse than the fare this
    ///      one offers ([`fare_no_worse`]). At `pos` and at every later
    ///      position both offer a state of the same time (the trip's
    ///      arrival), `a` first. Call the two states offered at a stop
    ///      `A` and `B`: `betterOrEqual(A, B)`, at every such stop. Then
    ///      offering `B` changes nothing:
    ///
    ///      - `betterOrEqual` is transitive when fares are exact (what is
    ///        paid less the value of the allowance is not a wrapped
    ///        number, [`FareCalculator::exact_within`]) since the value of
    ///        an allowance is never negative: with `X >= A` and `A >= B`,
    ///        by the first rule both times `X.paid <= A.paid - A.value <=
    ///        A.paid <= B.paid - B.value`; by the first then the second,
    ///        `X.paid <= A.paid - A.value <= B.paid - B.value` (`A` pays no
    ///        more and holds no less); by the second then the first,
    ///        `X.paid <= A.paid <= B.paid - B.value`; by the second both
    ///        times, each of its comparisons is transitive. The same shows
    ///        `A.paid - A.value <= B.paid - B.value`.
    ///      - No state of a list is better than or equal to another of the
    ///        same list: a state enters only after a scan of the whole
    ///        list that removed those it is better than or equal to and
    ///        found none better than or equal to it.
    ///      - If `A` was offered to a list, the list has from then on a
    ///        state `X >= A`: `A` itself or the state that refused it; and
    ///        `X` leaves a list only in the scan of a state `C >= X`,
    ///        which then enters (`C >= A`) or is refused by a state
    ///        `F >= C >= A` that stays.
    ///      - So when `B` is offered to that list, with `X >= A >= B`: its
    ///        scan stops at `X` at the latest, refused; and it removes
    ///        nothing on the way, for a state `O` with `B >= O` would have
    ///        `X >= O`, two states of one list.
    ///      - `A` and `B`, both off a vehicle, are offered to the same two
    ///        lists of the stop. When `A` is not offered to them, `B` is
    ///        not either or is refused before them: `add_state`'s first
    ///        two tests read the time alone (and `best_at_target`, which
    ///        only decreases); the test on going back in time passes for
    ///        `B` (its arrival is not before the time of its state from
    ///        `pos` on); and `fare_entry` refuses `B` when it refuses `A`,
    ///        on the time or because `A.paid - A.value`, hence
    ///        `B.paid - B.value`, is over `max_fare`.
    ///      - When `a` has itself been left behind, by (a) or (b) its
    ///        offers would be refused on grounds that refuse `B` too, as
    ///        just shown, and by (c) it is shadowed by a state whose fare
    ///        is no worse than its own, hence than this one's.
    ///
    /// In every case the state offered is refused and no list changes. The
    /// one trace (b) and (c) can skip is a bag made for a stop that has
    /// none, and
    /// an empty bag is the same as none for all that reads the bags:
    /// `do_one_round`'s copy (no state to board with), `do_transfers` (only
    /// stops where a state was accepted), `collate` (no time for a stop
    /// without non-transfer states) and `propagate_to_destination` (no
    /// state to walk from). The states that stay keep their order, so the
    /// others are offered in the order R5 offers them.
    fn ride_timetable(
        &mut self,
        pat: usize,
        table: &Timetable,
        before: &[(u32, i32)],
        states: &mut Vec<Rider>,
    ) -> Result<(), String> {
        let layer = self.layer;
        let n = table.trips.len();
        // `add_state`'s first test
        let limit = self.req.to_time.wrapping_add(self.req.max_trip_s);
        // `fare_entry`'s two tests
        let fare_list = matches!(self.dom, Dominance::Fare(_));
        let max_clock = self.departure.wrapping_add(self.req.max_trip_s);
        let max_fare = self.req.max_fare;
        let over =
            |memo: &Memo| matches!(memo, Some((f, _)) if f.paid.wrapping_sub(f.value) > max_fare);
        // (c): the fares that can shadow, per trip
        let shadowing = match self.dom {
            Dominance::Fare(calc) if self.exact_fares => Some(calc),
            _ => None,
        };
        let mut shadow = std::mem::take(&mut self.shadow);
        let mut shadow_trips = std::mem::take(&mut self.shadow_trips);
        for k in shadow_trips.drain(..) {
            shadow[k as usize].clear();
        }
        if shadow.len() < n {
            shadow.resize(n, Vec::new());
        }
        states.clear();
        for (pos, &stop) in layer.patterns[pat].stops.iter().enumerate() {
            let at = pos * n;
            // get off the bus
            let mut kept = 0;
            for r in 0..states.len() {
                let mut rider = states[r];
                let k = rider.k as usize;
                let arrival = table.arrivals[at + k];
                if rider.at_last_visit
                    && pos as u32 >= table.settled[k]
                    && (arrival > limit
                        || (fare_list
                            && arrival >= rider.s_time
                            && (arrival > max_clock || over(&rider.memo) || rider.shadowed)))
                {
                    continue;
                }
                let added = self.add_state(
                    stop,
                    rider.board_pos as i32,
                    pos as i32,
                    arrival,
                    rider.board_time,
                    pat as i32,
                    table.trips[k] as i32,
                    rider.s,
                    rider.s_time,
                    FareFrom::Ride(&mut rider.memo),
                );
                states[kept] = rider;
                kept += 1;
                if added? {
                    self.touched_stops.set(stop as usize);
                }
            }
            states.truncate(kept);
            // get on the bus
            let (from, to) = self.before_range[stop as usize];
            for &(s, s_time) in &before[from as usize..to as usize] {
                let earliest = s_time.wrapping_add(BOARD_SLACK);
                if let Some(k) = table.departures[at..at + n]
                    .iter()
                    .position(|&d| d > earliest)
                {
                    // `trip.departures[board_pos]` of the generic loop
                    let board_time = table.departures[at + k];
                    if table.revisit[pos] {
                        for earlier in states.iter_mut().filter(|r| r.s == s) {
                            earlier.k = k as u32;
                            earlier.board_pos = pos as u32;
                            earlier.board_time = board_time;
                            earlier.memo = None;
                        }
                    }
                    let at_last_visit = table.last_visit[pos];
                    let mut memo = None;
                    let mut shadowed = false;
                    if let Some(calc) = shadowing {
                        // the fare `fare_entry` would work out at the
                        // first stop that needs it
                        let p = calc.ride(&self.progress[s as usize], pat as i32, board_time);
                        let f = calc.bounds(&p);
                        memo = Some((f, p));
                        if at_last_visit {
                            let seen = &mut shadow[k];
                            if seen.iter().any(|a| fare_no_worse(a, &f)) {
                                shadowed = true;
                            } else {
                                // this one shadows whatever those it is
                                // no worse than would (transitivity)
                                if seen.is_empty() {
                                    shadow_trips.push(k as u32);
                                }
                                seen.retain(|o| !fare_no_worse(&f, o));
                                seen.push(f);
                            }
                        }
                    }
                    states.push(Rider {
                        s,
                        s_time,
                        k: k as u32,
                        board_pos: pos as u32,
                        board_time,
                        at_last_visit,
                        shadowed,
                        memo,
                    });
                }
            }
        }
        self.shadow = shadow;
        self.shadow_trips = shadow_trips;
        Ok(())
    }

    /// The pattern scratch maps (`tripIndicesInPattern`,
    /// `boardStopPositionInPattern`, `boardTimeForFrequencyTrips`): a later
    /// put replaces an earlier one (and forgets the fare kept for it).
    fn put(&mut self, s: u32, (trip, board_pos, freq_time): (i32, i32, i32)) {
        let i = s as usize;
        let slot = Slot {
            trip,
            board_pos,
            freq_time,
            memo: None,
        };
        if self.slot.len() <= i {
            let empty = Slot {
                trip: 0,
                board_pos: 0,
                freq_time: 0,
                memo: None,
            };
            self.slot.resize(self.arena.len().max(i + 1), empty);
        }
        self.slot[i] = slot;
    }

    /// `doTransfers`.
    fn do_transfers(&mut self) -> Result<(), String> {
        let mut by_transfer = std::mem::replace(&mut self.by_transfer, Bits(Vec::new()));
        by_transfer.clear();
        let mut done = Ok(());
        'stops: for stop in self.touched_stops.ones() {
            // R5 walks from a copy of the stop's non-transfer states. A
            // state that walks is added with pattern -1, which goes to the
            // `best` list of its stop only: no non-transfer list changes
            // during the loop, and this one can be lent instead of copied.
            let nt = std::mem::take(&mut self.bags[stop].nt);
            for e in &nt {
                for &(to, mm) in &self.transfers[stop] {
                    let walk = (f64::from(mm) / self.req.walk_mms) as i32;
                    let time = e.time.wrapping_add(walk);
                    let from = FareFrom::Walk(e.fare());
                    match self.add_state(to, -1, -1, time, -1, -1, -1, e.id, e.time, from) {
                        Ok(true) => by_transfer.set(to as usize),
                        Ok(false) => {}
                        Err(err) => {
                            done = Err(err);
                            self.bags[stop].nt = nt;
                            break 'stops;
                        }
                    }
                }
            }
            self.bags[stop].nt = nt;
        }
        self.touched_stops.or(&by_transfer);
        self.by_transfer = by_transfer;
        done
    }

    /// `collateTravelTimes` with R5's collater
    /// (`InRoutingFareCalculator.getCollator`): per stop, the earliest
    /// state off a vehicle whose fare is within `max_fare`.
    fn collate(&self, times: &mut [i32]) {
        times.iter_mut().for_each(|t| *t = UNREACHED);
        for &stop in &self.with_bag {
            let bag = &self.bags[stop as usize];
            let mut best: Option<&McState> = None;
            for e in &bag.nt {
                let st = &self.arena[e.id as usize];
                if let Some(f) = st.fare {
                    if f.paid > self.req.max_fare {
                        continue;
                    }
                }
                if best.is_none_or_later(st.time) {
                    best = Some(st);
                }
            }
            if let Some(b) = best {
                times[stop as usize] = b.time.wrapping_sub(self.departure);
            }
        }
    }

    /// `doPropagationToDestination`: a state at the destination for every
    /// state off a vehicle at an egress stop, in a fresh bag.
    fn propagate_to_destination(&mut self, out: &mut Vec<u32>) {
        let egress = self.egress.expect("egress");
        let mut dest: Vec<Entry> = Vec::new();
        for &(stop, e) in egress {
            if !self.has_bag[stop as usize] {
                continue;
            }
            // lent, not copied: only `dest` changes in the loop
            let nt = std::mem::take(&mut self.bags[stop as usize].nt);
            for en in &nt {
                let s = en.id;
                let id = self.arena.len() as u32;
                let time = en.time.wrapping_add(e);
                // McRaptorStateBag.add of a walk (pattern -1): best only.
                // As in `add_state`, the state is kept when accepted.
                let mut fare = None;
                let mut progress = FareProgress::default();
                let accepted = match self.dom {
                    Dominance::Fare(calc) => {
                        let mut from = FareFrom::Walk(en.fare());
                        match self.fare_entry(calc, id, time, 0, -1, 0, s, &mut from) {
                            None => false,
                            Some((entry, f)) => {
                                fare = Some(f);
                                progress = self.progress[s as usize];
                                list_add(&mut dest, entry, fare_better_or_equal)
                            }
                        }
                    }
                    Dominance::Suboptimal { shortest, .. } => {
                        let entry = Entry {
                            id,
                            time,
                            round: 0,
                            paid: 0,
                            consumed: 0,
                            value: 0,
                            expiration: 0,
                            number: 0,
                        };
                        let (sub, dep) = (self.req.suboptimal_minutes, self.departure);
                        suboptimal_add(&mut dest, entry, sub, dep, &mut self.max_duration, shortest)
                    }
                };
                if accepted {
                    self.arena.push(McState {
                        back: s,
                        time,
                        board_time: 0,
                        pattern: -1,
                        trip: -1,
                        round: 0,
                        stop: -1,
                        board_pos: 0,
                        alight_pos: 0,
                        fare,
                    });
                    self.progress.push(progress);
                }
            }
            self.bags[stop as usize].nt = nt;
        }
        out.clear();
        out.extend(dest.iter().map(|e| e.id));
    }
}

trait LaterThan {
    fn is_none_or_later(&self, t: i32) -> bool;
}

impl LaterThan for Option<&McState> {
    /// `best == null || state.time < best.time`.
    fn is_none_or_later(&self, t: i32) -> bool {
        self.map_or(true, |b| t < b.time)
    }
}

#[cfg(test)]
impl Schedules {
    /// No timetable: every pattern goes through the loop of
    /// [`McRaptor::ride_patterns`].
    fn none(layer: &TransitLayer) -> Self {
        Schedules(layer.patterns.iter().map(|_| None).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::super::layer::{FrequencyEntry, TripSchedule};
    use super::super::mcraptor_r5 as r5;
    use super::*;
    use crate::fares::incremental::tests::random_structure;
    use crate::options::testing::Rng;

    /// R5's `DominatingList.add`, as the first port had it.
    fn r5_list_add(list: &mut Vec<Entry>, new: Entry) -> bool {
        let mut i = 0;
        while i < list.len() {
            if fare_better_or_equal(&list[i], &new) {
                return false;
            }
            if fare_better_or_equal(&new, &list[i]) {
                list.remove(i);
            } else {
                i += 1;
            }
        }
        list.push(new);
        true
    }

    /// [`list_add`] leaves the list R5's loop leaves, refusals after
    /// removals included, on lists of any content.
    #[test]
    fn list_add_is_r5s_loop() {
        let mut rng = Rng::new(11);
        let entry = |rng: &mut Rng, id: u32| {
            let paid = rng.below(4) as i32 * 100;
            let value = rng.below(3) as i32 * 100;
            Entry {
                id,
                time: rng.below(5) as i32,
                round: 0,
                paid,
                consumed: paid - value,
                value,
                expiration: rng.below(3) as i32,
                number: rng.below(3) as i32,
            }
        };
        let (mut refused_after_removal, mut accepted, mut removed) = (0, 0, 0);
        for _ in 0..200_000 {
            let n = rng.below(9);
            let mut a: Vec<Entry> = (0..n).map(|i| entry(&mut rng, i as u32)).collect();
            let mut b = a.clone();
            let new = entry(&mut rng, 99);
            let (ra, rb) = (
                list_add(&mut a, new, fare_better_or_equal),
                r5_list_add(&mut b, new),
            );
            assert_eq!(ra, rb);
            let ids = |l: &[Entry]| l.iter().map(|e| e.id).collect::<Vec<_>>();
            assert_eq!(ids(&a), ids(&b));
            refused_after_removal += usize::from(!ra && a.len() < n);
            accepted += usize::from(ra);
            removed += usize::from(ra && a.len() <= n);
        }
        assert!(
            refused_after_removal > 500 && accepted > 10_000 && removed > 5_000,
            "{refused_after_removal} {accepted} {removed}"
        );
    }

    /// A random network of a few stops: patterns by schedule (trips in any
    /// order, on a running or an idle service, some with times that go
    /// back), by frequency, and now and then both; patterns that come back
    /// to a stop; transfers.
    struct Net {
        layer: TransitLayer,
        fares: crate::FareStructure,
        transfers: Vec<Vec<(u32, i32)>>,
        allowed: Vec<bool>,
        active: Vec<bool>,
    }

    fn network(seed: u64) -> Net {
        let mut rng = Rng::new(seed ^ 0x5eed);
        let n_patterns = 4 + rng.below(5);
        let (mut layer, fares) = random_structure(seed, n_patterns);
        let n_stops = 5 + rng.below(8);
        layer.stop_ids = (0..n_stops).map(|s| s.to_string()).collect();
        layer.patterns_for_stop = vec![Vec::new(); n_stops];
        for (pi, p) in layer.patterns.iter_mut().enumerate() {
            let len = 2 + rng.below(6);
            let mut stops: Vec<u32> = Vec::new();
            let distinct = rng.chance(1, 2);
            while stops.len() < len.min(n_stops) {
                let s = rng.below(n_stops) as u32;
                if !distinct || !stops.contains(&s) {
                    stops.push(s);
                }
            }
            let kind = rng.below(25);
            for t in 0..1 + rng.below(5) {
                let frequency = match kind {
                    0..=17 => false,
                    18..=23 => true,
                    _ => t % 2 == 0,
                };
                let mut clock = if frequency {
                    0
                } else {
                    900 + rng.below(3000) as i32
                };
                let (mut arrivals, mut departures) = (Vec::new(), Vec::new());
                for _ in 0..stops.len() {
                    arrivals.push(clock);
                    clock += rng.below(40) as i32;
                    departures.push(clock);
                    clock += rng.below(500) as i32;
                    if !frequency && rng.chance(1, 12) {
                        clock -= rng.below(400) as i32;
                    }
                }
                let frequencies = if frequency {
                    (0..1 + rng.below(2))
                        .map(|_| {
                            let start = 900 + rng.below(2500) as i32;
                            FrequencyEntry {
                                start,
                                end: start + rng.below(3000) as i32,
                                headway: 60 + rng.below(900) as i32,
                                exact: false,
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                p.has_frequencies |= frequency;
                p.has_schedules |= !frequency;
                p.trips.push(TripSchedule {
                    trip_id: format!("{pi}-{t}"),
                    service: rng.below(3).min(1) as u32,
                    arrivals,
                    departures,
                    frequencies,
                });
            }
            for &s in &stops {
                if !layer.patterns_for_stop[s as usize].contains(&(pi as u32)) {
                    layer.patterns_for_stop[s as usize].push(pi as u32);
                }
            }
            p.stops = stops;
        }
        let transfers = (0..n_stops)
            .map(|_| {
                (0..rng.below(3))
                    .map(|_| (rng.below(n_stops) as u32, rng.below(400_000) as i32))
                    .collect()
            })
            .collect();
        let allowed = (0..n_patterns).map(|_| !rng.chance(1, 10)).collect();
        Net {
            layer,
            fares,
            transfers,
            allowed,
            // service 0 runs, service 1 does not
            active: vec![true, false],
        }
    }

    /// What a search hands back: per draw its stop times, or the chain of
    /// every state at the destination (each state's every field).
    type Chain = Vec<(i32, i32, i32, i32, i32, i32, i32, i32, Option<FareBounds>)>;
    #[derive(Debug, PartialEq, Default)]
    struct Found {
        times: Vec<(i32, Vec<i32>)>,
        chains: Vec<(i32, Vec<Chain>)>,
        error: Option<String>,
    }

    macro_rules! chain {
        ($arena:expr, $s:expr, $none:expr) => {{
            let mut chain: Chain = Vec::new();
            let mut k = $s;
            while k != $none {
                let st = &$arena[k as usize];
                chain.push((
                    st.time,
                    st.board_time,
                    st.pattern,
                    st.trip,
                    st.round,
                    st.stop,
                    st.board_pos,
                    st.alight_pos,
                    st.fare,
                ));
                k = st.back;
            }
            chain
        }};
    }

    /// The faster router, with and without timetables, hands back what the
    /// first port hands back: times per stop, the states at the
    /// destination with all that is behind them, and errors; with both
    /// dominating lists, with and without a fare limit and target pruning.
    #[test]
    fn same_as_the_first_port() {
        let n_cases: u64 = std::env::var("PEABIRU_MCRAPTOR_SOAK")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8000);
        let cancel = CancelToken::default();
        let (mut with_states, mut with_times, mut errors, mut searches) = (0u64, 0u64, 0u64, 0u64);
        // two more networks, which caught wrong versions of the rules on the
        // patterns that come back to a stop
        for seed in (0..n_cases).chain([5060, 16860]) {
            let net = network(seed);
            let Ok(calc) = FareCalculator::new(&net.fares, &net.layer) else {
                continue;
            };
            let mut rng = Rng::new(seed ^ 0xfa4e);
            let n_stops = net.layer.stop_ids.len();
            let stops = |rng: &mut Rng| -> Vec<(u32, i32)> {
                (0..1 + rng.below(3))
                    .map(|_| (rng.below(n_stops) as u32, rng.below(600) as i32))
                    .collect()
            };
            let timetables = Schedules::new(&net.layer, &net.allowed, &net.active);
            let no_timetables = Schedules::none(&net.layer);
            for _ in 0..4 {
                let from_time = 900 + rng.below(1500) as i32;
                let fare_list = rng.chance(2, 3);
                let req = McRequest {
                    from_time,
                    to_time: from_time + 60 * (1 + rng.below(10) as i32),
                    max_trip_s: 300 + rng.below(4000) as i32,
                    max_rides: 1 + rng.below(4),
                    suboptimal_minutes: [0, 0, 5][rng.below(3)],
                    max_fare: match (fare_list, rng.below(4)) {
                        (true, 0) | (false, 1..) => -1,
                        (_, 2) => rng.below(1500) as i32,
                        _ => i32::MAX,
                    },
                    monte_carlo_draws: 1,
                    walk_mms: 1000.0 + rng.below(500) as f64,
                    seed: Some(seed as i32),
                    origin: (-30.0, -51.0),
                };
                let dom = || {
                    if fare_list {
                        Dominance::Fare(&calc)
                    } else {
                        Dominance::Suboptimal {
                            max_duration: 300 + rng_free(seed) % 4000,
                            shortest: seed % 2 == 0,
                        }
                    }
                };
                let access = stops(&mut rng);
                let egress = rng.chance(1, 2).then(|| stops(&mut rng));
                let mut deps: Vec<i32> = (0..1 + rng.below(3))
                    .map(|_| from_time + rng.below((req.to_time - from_time) as usize) as i32)
                    .collect();
                deps.sort_unstable();
                let mut want = Found::default();
                {
                    let mut router = r5::McRaptor::new(
                        &net.layer,
                        &net.transfers,
                        &net.allowed,
                        &net.active,
                        &req,
                        dom(),
                        egress.as_deref(),
                        7,
                    );
                    let (times, chains) = (&mut want.times, &mut want.chains);
                    want.error = router
                        .route_departures(&deps, &access, &cancel, |dep, out| match out {
                            r5::DrawOut::Times(t) => times.push((dep, t.to_vec())),
                            r5::DrawOut::Final(arena, states) => chains.push((
                                dep,
                                states.iter().map(|&s| chain!(arena, s, r5::NONE)).collect(),
                            )),
                        })
                        .err();
                }
                for schedules in [&timetables, &no_timetables] {
                    let mut got = Found::default();
                    let mut router = McRaptor::new(
                        &net.layer,
                        &net.transfers,
                        &net.allowed,
                        &net.active,
                        schedules,
                        &req,
                        dom(),
                        egress.as_deref(),
                        7,
                    );
                    let (times, chains) = (&mut got.times, &mut got.chains);
                    got.error = router
                        .route_departures(&deps, &access, &cancel, |dep, out| match out {
                            DrawOut::Times(t) => times.push((dep, t.to_vec())),
                            DrawOut::Final(arena, states) => chains.push((
                                dep,
                                states.iter().map(|&s| chain!(arena, s, NONE)).collect(),
                            )),
                        })
                        .err();
                    if want.error.is_some() {
                        // the draws before the failing one, and the failure
                        assert_eq!(got.error, want.error, "seed {seed}");
                    }
                    assert_eq!(got, want, "seed {seed}");
                }
                searches += 1;
                errors += u64::from(want.error.is_some());
                with_states += want
                    .chains
                    .iter()
                    .map(|(_, c)| c.iter().filter(|c| c.len() > 3).count() as u64)
                    .sum::<u64>();
                with_times += want
                    .times
                    .iter()
                    .map(|(_, t)| t.iter().filter(|&&t| t != UNREACHED).count() as u64)
                    .sum::<u64>();
            }
        }
        // the searches find trips (with a transfer or more behind them),
        // and some fail as R5 fails
        if n_cases >= 8000 {
            assert!(
                searches > 25_000 && with_states > 1_500 && with_times > 30_000 && errors > 150,
                "{searches} {with_states} {with_times} {errors}"
            );
            assert!(errors < searches / 2, "{errors} of {searches}");
        }
    }

    /// A number from the seed alone (the same for the three routers of a
    /// case).
    fn rng_free(seed: u64) -> i32 {
        (Rng::new(seed ^ 0xd0).below(1 << 20)) as i32
    }
}
