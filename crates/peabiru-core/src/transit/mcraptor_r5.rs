//! The first port of R5 7.6's `McRaptorSuboptimalPathProfileRouter` in this
//! crate, statement by statement as R5 has it (every state made and kept,
//! the fare of a state from all its rides, each list a list of states, a
//! copy of every bag per round): the one whose results were validated before
//! [`super::mcraptor`] was made faster. Kept, for the tests only, as what
//! the faster router must reproduce: `mcraptor`'s tests run both on random
//! networks and compare everything a search hands back.

#![allow(dead_code)]

use super::layer::TransitLayer;
use super::mcraptor::{Dominance, McRequest};
use super::raptor::{Offsets, UNREACHED};
use crate::fares::FareBounds;
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
    pub(crate) round: i32,
    pub(crate) stop: i32,
    pub(crate) board_pos: i32,
    pub(crate) alight_pos: i32,
    pub(crate) fare: Option<FareBounds>,
}

/// A state bag (`McRaptorStateBag`): the best states, and those whose last
/// step was not a walk between stops.
#[derive(Default, Clone)]
struct Bag {
    best: Vec<u32>,
    nt: Vec<u32>,
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
    bags: Vec<Option<Bag>>,
    with_bag: Vec<u32>,
    touched_stops: Bits,
    touched_patterns: Bits,
    round: i32,
    departure: i32,
    best_at_target: i32,
    /// `Suboptimal`'s limit, shared by every list.
    max_duration: i32,
    /// Per pattern scratch: the latest (trip, board position, board time)
    /// put for a state, by arena index.
    slot: Vec<(i32, i32, i32)>,
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
        McRaptor {
            layer,
            transfers,
            allowed,
            active,
            req,
            dom,
            egress,
            egress_time,
            near_destination,
            offsets: Offsets::new(layer, offsets_mt),
            mt: MersenneTwister::from_long(departure_seed),
            arena: Vec::new(),
            bags: vec![None; n_stops],
            with_bag: Vec::new(),
            touched_stops: Bits::new(n_stops),
            touched_patterns: Bits::new(layer.patterns.len()),
            round: 0,
            departure: 0,
            best_at_target: i32::MAX,
            max_duration,
            slot: Vec::new(),
        }
    }

    /// the seed of the departure-time sample
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
                self.bags[s as usize] = None;
            }
            self.with_bag.clear();
            self.arena.clear();
            self.touched_patterns.clear();
            self.touched_stops.clear();
            // TripPlanner.AccessTimes (the trip planner ported with the router): target pruning per draw
            self.best_at_target = i32::MAX;
            self.round = 0;
            for &(stop, t) in access {
                if self.add_state(stop, -1, -1, dep.wrapping_add(t), -1, -1, -1, NONE)? {
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

    /// The `(pattern, board time)` of the rides behind a state, in order.
    fn rides(&self, mut s: u32, out: &mut Vec<(i32, i32)>) {
        out.clear();
        while s != NONE {
            let st = &self.arena[s as usize];
            if st.pattern > -1 {
                out.push((st.pattern, st.board_time));
            }
            s = st.back;
        }
        out.reverse();
    }

    /// `FareDominatingList.betterOrEqual`.
    fn fare_better_or_equal(a: &McState, b: &McState) -> bool {
        let (fa, fb) = (a.fare.expect("fare"), b.fare.expect("fare"));
        let consumed = fb.paid.wrapping_sub(fb.value);
        if a.time <= b.time {
            if fa.paid <= consumed {
                return true;
            }
            if fa.paid <= fb.paid
                && fa.value >= fb.value
                && fa.expiration >= fb.expiration
                && fa.number >= fb.number
            {
                return true;
            }
        }
        false
    }

    /// `SuboptimalDominatingList.dominates(a, b)`: one access mode.
    fn sub_dominates(&self, a: &McState, b: &McState) -> bool {
        if a.round < b.round && a.time <= b.time {
            return true;
        }
        let threshold = self.req.suboptimal_minutes.wrapping_mul(60);
        a.time.wrapping_add(threshold) < b.time
    }

    /// `DominatingList.add` of state `s` to `list`.
    fn list_add(&mut self, list: &mut Vec<u32>, s: u32, rides: &mut Vec<(i32, i32)>) -> bool {
        match self.dom {
            Dominance::Fare(calc) => {
                let max_clock = self.departure.wrapping_add(self.req.max_trip_s);
                if self.arena[s as usize].time > max_clock {
                    return false;
                }
                if self.arena[s as usize].fare.is_none() {
                    self.rides(s, rides);
                    self.arena[s as usize].fare = Some(calc.fare(rides));
                }
                let f = self.arena[s as usize].fare.expect("fare");
                if f.paid.wrapping_sub(f.value) > self.req.max_fare {
                    return false;
                }
                let new = self.arena[s as usize];
                let mut i = 0;
                while i < list.len() {
                    let old = &self.arena[list[i] as usize];
                    if Self::fare_better_or_equal(old, &new) {
                        return false;
                    }
                    if Self::fare_better_or_equal(&new, old) {
                        list.remove(i);
                    } else {
                        i += 1;
                    }
                }
                list.push(s);
                true
            }
            Dominance::Suboptimal { shortest, .. } => {
                let new = self.arena[s as usize];
                if new.time.wrapping_sub(self.departure) > self.max_duration {
                    return false;
                }
                let mut i = 0;
                while i < list.len() {
                    let old = self.arena[list[i] as usize];
                    if self.sub_dominates(&old, &new) {
                        return false;
                    }
                    if self.sub_dominates(&new, &old) {
                        list.remove(i);
                    } else {
                        i += 1;
                    }
                }
                list.push(s);
                if shortest && new.stop == -1 {
                    self.max_duration =
                        self.max_duration.min(new.time.wrapping_sub(self.departure));
                }
                true
            }
        }
    }

    /// `addState`.
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
        if back != NONE && self.arena[back as usize].time > time {
            return Err("Attempt to decrement time in state! (R5 McRAPTOR)".into());
        }
        let id = self.arena.len() as u32;
        self.arena.push(McState {
            back,
            time,
            board_time,
            pattern,
            trip,
            round: self.round,
            stop: stop as i32,
            board_pos,
            alight_pos,
            fare: None,
        });
        let si = stop as usize;
        if self.bags[si].is_none() {
            self.bags[si] = Some(Bag::default());
            self.with_bag.push(stop);
        }
        let mut rides = Vec::new();
        let mut best = std::mem::take(&mut self.bags[si].as_mut().expect("bag").best);
        let mut optimal = self.list_add(&mut best, id, &mut rides);
        self.bags[si].as_mut().expect("bag").best = best;
        if pattern != -1 {
            let mut nt = std::mem::take(&mut self.bags[si].as_mut().expect("bag").nt);
            if self.list_add(&mut nt, id, &mut rides) {
                optimal = true;
            }
            self.bags[si].as_mut().expect("bag").nt = nt;
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
        // bestStatesBeforeRound: a copy of every bag's best states
        let mut before: Vec<Option<Vec<u32>>> = vec![None; self.bags.len()];
        for &s in &self.with_bag {
            before[s as usize] = Some(self.bags[s as usize].as_ref().expect("bag").best.clone());
        }
        // on the last round of a point-to-point search, only the patterns
        // near the destination
        if self.round == self.req.max_rides as i32 && self.egress.is_some() {
            self.touched_patterns.and(&self.near_destination);
        }
        let layer = self.layer;
        let mut states: Vec<u32> = Vec::new();
        for pat in self.touched_patterns.ones() {
            if !self.allowed[pat] {
                continue;
            }
            let pattern = &layer.patterns[pat];
            states.clear();
            for (pos, &stop) in pattern.stops.iter().enumerate() {
                let reached_before = before[stop as usize].is_some();
                // get off the bus
                for &s in &states {
                    let (trip, board_pos, freq_time) = self.slot[s as usize];
                    let sched = &pattern.trips[trip as usize];
                    let (arrival, board_time) = if sched.is_frequency() {
                        let travel =
                            sched.arrivals[pos].wrapping_sub(sched.departures[board_pos as usize]);
                        (freq_time.wrapping_add(travel), freq_time)
                    } else {
                        (sched.arrivals[pos], sched.departures[board_pos as usize])
                    };
                    if self.add_state(
                        stop, board_pos, pos as i32, arrival, board_time, pat as i32, trip, s,
                    )? {
                        self.touched_stops.set(stop as usize);
                    }
                }
                // get on the bus
                if !reached_before {
                    continue;
                }
                let candidates = before[stop as usize].as_ref().expect("copied");
                for &s in candidates {
                    let st = self.arena[s as usize];
                    if st.round != self.round - 1 {
                        continue;
                    }
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
                            if t.departures[pos] > st.time.wrapping_add(BOARD_SLACK) {
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
                            let earliest = st.time.wrapping_add(BOARD_SLACK);
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
        self.do_transfers()?;
        self.mark_patterns();
        self.round += 1;
        Ok(!self.touched_patterns.is_empty())
    }

    /// The pattern scratch maps (`tripIndicesInPattern`,
    /// `boardStopPositionInPattern`, `boardTimeForFrequencyTrips`): a later
    /// put replaces an earlier one.
    fn put(&mut self, s: u32, v: (i32, i32, i32)) {
        let i = s as usize;
        if self.slot.len() <= i {
            self.slot.resize(self.arena.len().max(i + 1), (0, 0, 0));
        }
        self.slot[i] = v;
    }

    /// `doTransfers`.
    fn do_transfers(&mut self) -> Result<(), String> {
        let mut by_transfer = Bits::new(self.layer.stop_ids.len());
        for stop in self.touched_stops.ones() {
            let nt = self.bags[stop].as_ref().expect("bag").nt.clone();
            for s in nt {
                let time = self.arena[s as usize].time;
                for &(to, mm) in &self.transfers[stop] {
                    let walk = (f64::from(mm) / self.req.walk_mms) as i32;
                    if self.add_state(to, -1, -1, time.wrapping_add(walk), -1, -1, -1, s)? {
                        by_transfer.set(to as usize);
                    }
                }
            }
        }
        self.touched_stops.or(&by_transfer);
        Ok(())
    }

    /// `collateTravelTimes` with R5's collater
    /// (`InRoutingFareCalculator.getCollator`): per stop, the earliest
    /// state off a vehicle whose fare is within `max_fare`.
    fn collate(&self, times: &mut [i32]) {
        times.iter_mut().for_each(|t| *t = UNREACHED);
        for &stop in &self.with_bag {
            let bag = self.bags[stop as usize].as_ref().expect("bag");
            let mut best: Option<&McState> = None;
            for &s in &bag.nt {
                let st = &self.arena[s as usize];
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
        let mut dest: Vec<u32> = Vec::new();
        let mut rides = Vec::new();
        for &(stop, e) in egress {
            let Some(bag) = &self.bags[stop as usize] else {
                continue;
            };
            let nt = bag.nt.clone();
            for s in nt {
                let id = self.arena.len() as u32;
                let time = self.arena[s as usize].time.wrapping_add(e);
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
                    fare: None,
                });
                // McRaptorStateBag.add of a walk (pattern -1): best only
                self.list_add(&mut dest, id, &mut rides);
            }
        }
        out.clear();
        out.extend_from_slice(&dest);
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
