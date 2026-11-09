//! Range RAPTOR over a departure window, a literal port of R5 7.6
//! `FastRaptorWorker` + `RaptorState` + `FrequencyRandomOffsets` (the path
//! r5r takes without fares): scheduled search carried from minute to minute,
//! frequency upper bounds, and Monte Carlo copies with random offsets drawn
//! in R5's order so results are reproducible with r5r's `seed`.

use super::layer::{Pattern, TransitLayer};
use crate::jcompat::{objects_hash_int_double_double, MersenneTwister, TroveIntKeys};

pub const UNREACHED: i32 = i32::MAX;
const DEPARTURE_STEP_SEC: i32 = 60;
const MINIMUM_BOARD_WAIT_SEC: i32 = 60;

/// Transit modes by GTFS route type (R5 `TransitLayer.getTransitModes`);
/// `None` for types R5 rejects. AIR (1100-1199) is `Some(false)`: known
/// but excluded by r5r's `TRANSIT`.
pub fn route_type_allowed(route_type: i32) -> Option<bool> {
    match route_type {
        0..=7 | 11 | 12 => Some(true),
        100..=199
        | 200..=299
        | 300..=499
        | 500..=699
        | 700..=899
        | 900..=999
        | 1000..=1099
        | 1200..=1299
        | 1300..=1399
        | 1400..=1499 => Some(true),
        1100..=1199 => Some(false),
        _ => None,
    }
}

/// A small fixed-size bit set.
#[derive(Clone)]
struct BitSet(Vec<u64>);

impl BitSet {
    fn new(n: usize) -> Self {
        BitSet(vec![0; n.div_ceil(64)])
    }
    #[inline]
    fn set(&mut self, i: usize) {
        self.0[i / 64] |= 1 << (i % 64);
    }
    #[inline]
    fn get(&self, i: usize) -> bool {
        self.0[i / 64] >> (i % 64) & 1 == 1
    }
    fn clear(&mut self) {
        self.0.iter_mut().for_each(|w| *w = 0);
    }
    fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(wi, &w)| {
            let mut w = w;
            std::iter::from_fn(move || {
                if w == 0 {
                    return None;
                }
                let b = w.trailing_zeros() as usize;
                w &= w - 1;
                Some(wi * 64 + b)
            })
        })
    }
}

/// One round's state (R5 `RaptorState`, without path bookkeeping).
#[derive(Clone)]
struct State {
    best: Vec<i32>,
    best_nt: Vec<i32>,
    updated: BitSet,
    nt_updated: BitSet,
    departure: i32,
    max_duration: i32,
}

impl State {
    fn new(n: usize, max_duration: i32, departure: i32) -> Self {
        State {
            best: vec![UNREACHED; n],
            best_nt: vec![UNREACHED; n],
            updated: BitSet::new(n),
            nt_updated: BitSet::new(n),
            departure,
            max_duration,
        }
    }

    /// `copy()`: values kept, update flags empty.
    fn copy(&self) -> Self {
        let n = self.best.len();
        State {
            best: self.best.clone(),
            best_nt: self.best_nt.clone(),
            updated: BitSet::new(n),
            nt_updated: BitSet::new(n),
            departure: self.departure,
            max_duration: self.max_duration,
        }
    }

    #[inline]
    fn set_time_at_stop(&mut self, stop: usize, time: i32, transfer: bool) -> bool {
        if time >= self.departure + self.max_duration {
            return false;
        }
        let mut optimal = false;
        if !transfer && time < self.best_nt[stop] {
            self.best_nt[stop] = time;
            self.nt_updated.set(stop);
            optimal = true;
        }
        if time < self.best[stop] {
            self.best[stop] = time;
            self.updated.set(stop);
            optimal = true;
        }
        optimal
    }

    fn set_departure_time(&mut self, departure: i32) {
        self.departure = departure;
        self.updated.clear();
        self.nt_updated.clear();
        let max_clock = departure + self.max_duration;
        for t in self.best.iter_mut().chain(self.best_nt.iter_mut()) {
            if *t >= max_clock {
                *t = UNREACHED;
            }
        }
    }
}

/// `minMergePrevious` of round `r` from round `r - 1`.
fn min_merge_previous(states: &mut [State], r: usize) {
    let (prev, cur) = states.split_at_mut(r);
    let (p, c) = (&prev[r - 1], &mut cur[0]);
    for s in 0..c.best.len() {
        if p.best[s] <= c.best[s] {
            c.best[s] = p.best[s];
        }
        if p.best_nt[s] <= c.best_nt[s] {
            c.best_nt[s] = p.best_nt[s];
        }
    }
}

/// `stopWasUpdated` of round `r` (previous round `r - 1`, none for round 0).
#[inline]
fn stop_was_updated(states: &[State], r: usize, stop: usize, within_minute: bool) -> bool {
    if within_minute {
        states[r].updated.get(stop)
    } else {
        let prev = if r == 0 {
            UNREACHED
        } else {
            states[r - 1].best[stop]
        };
        states[r].best[stop] < prev
    }
}

/// Running scheduled and frequency trip indices of a pattern, and whether
/// the scheduled ones never overtake each other.
type RunningTrips = (Vec<u32>, Vec<u32>, bool);

/// Patterns running on the request date (R5 `FilteredPatterns`).
pub struct Filtered {
    /// Per pattern, indices into `pattern.trips` of running scheduled and
    /// frequency trips, and whether scheduled trips never overtake.
    patterns: Vec<Option<RunningTrips>>,
    running_scheduled: Vec<bool>,
    running_frequency: Vec<bool>,
}

impl Filtered {
    /// Patterns with an active service whose mode is allowed. Errors on a
    /// route type R5 does not know (R5 fails the whole request then).
    pub fn new(layer: &TransitLayer, active: &[bool]) -> Result<Filtered, String> {
        let mut f = Filtered {
            patterns: Vec::with_capacity(layer.patterns.len()),
            running_scheduled: vec![false; layer.patterns.len()],
            running_frequency: vec![false; layer.patterns.len()],
        };
        for (pi, p) in layer.patterns.iter().enumerate() {
            let allowed = route_type_allowed(p.route_type)
                .ok_or_else(|| format!("unsupported GTFS route_type {}", p.route_type))?;
            if !(allowed && p.services.iter().any(|&s| active[s as usize])) {
                f.patterns.push(None);
                continue;
            }
            let mut sched = Vec::new();
            let mut freq = Vec::new();
            for (ti, t) in p.trips.iter().enumerate() {
                if active[t.service as usize] {
                    if t.is_frequency() {
                        freq.push(ti as u32);
                    } else {
                        sched.push(ti as u32);
                    }
                }
            }
            let overtakes = sched.windows(2).any(|w| {
                let (a, b) = (&p.trips[w[0] as usize], &p.trips[w[1] as usize]);
                a.departures.iter().zip(&b.departures).any(|(x, y)| x > y)
            });
            f.patterns.push(Some((sched, freq, !overtakes)));
            f.running_frequency[pi] = p.has_frequencies;
            f.running_scheduled[pi] = p.has_schedules;
        }
        Ok(f)
    }
}

/// Random offsets of every frequency entry (R5 `FrequencyRandomOffsets`).
struct Offsets {
    /// Frequency patterns in Trove `TIntObjectHashMap` iteration order.
    order: Vec<usize>,
    /// `offsets[pattern][trip]` per frequency entry (empty for scheduled trips).
    offsets: Vec<Vec<Vec<i32>>>,
    mt: MersenneTwister,
}

impl Offsets {
    fn new(layer: &TransitLayer, mt: MersenneTwister) -> Self {
        let mut keys = TroveIntKeys::new();
        let mut offsets = vec![Vec::new(); layer.patterns.len()];
        for (pi, p) in layer.patterns.iter().enumerate() {
            if !p.has_frequencies {
                continue;
            }
            keys.insert(pi as i32);
            offsets[pi] = p
                .trips
                .iter()
                .map(|t| vec![-1; t.frequencies.len()])
                .collect();
        }
        Offsets {
            order: keys.iteration_order().map(|k| k as usize).collect(),
            offsets,
            mt,
        }
    }

    fn randomize(&mut self, layer: &TransitLayer) {
        for &pi in &self.order {
            for (ti, t) in layer.patterns[pi].trips.iter().enumerate() {
                for (e, f) in t.frequencies.iter().enumerate() {
                    self.offsets[pi][ti][e] = self.mt.next_int(f.headway);
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FreqMode {
    MonteCarlo,
    UpperBound,
}

/// What a RAPTOR search needs besides the network.
pub struct RaptorRequest {
    /// First departure, seconds after midnight of the service day.
    pub from_time: i32,
    /// Window length in minutes (one departure per minute).
    pub time_window: i32,
    pub max_rides: usize,
    /// `max_trip_duration` in seconds.
    pub max_duration: i32,
    /// `(int)(walkSpeed * 1000)`.
    pub walk_mms: i32,
    /// `max_walk_time` in minutes.
    pub max_walk_min: i32,
    /// `time_window * draws_per_minute`.
    pub monte_carlo_draws: i32,
    /// r5r `seed`; `None` draws from the clock as R5 does.
    pub seed: Option<i32>,
    /// Origin coordinates (they seed the per-origin generator).
    pub origin: (f64, f64),
}

impl RaptorRequest {
    /// Iterations per departure minute (R5 `getMonteCarloDrawsPerMinute`).
    pub fn iterations_per_minute(&self, has_frequencies: bool) -> usize {
        if !has_frequencies || self.monte_carlo_draws == 0 {
            1
        } else {
            (f64::from(self.monte_carlo_draws) / f64::from(self.time_window)).ceil() as usize
        }
    }
}

struct Worker<'a> {
    layer: &'a TransitLayer,
    transfers: &'a [Vec<(u32, i32)>],
    filtered: &'a Filtered,
    req: &'a RaptorRequest,
    n_stops: usize,
}

impl Worker<'_> {
    fn patterns_to_explore(
        &self,
        input: &[State],
        r: usize,
        running: &[bool],
        within_minute: bool,
    ) -> Vec<usize> {
        let mut set = BitSet::new(self.layer.patterns.len());
        for stop in 0..self.n_stops {
            if !stop_was_updated(input, r, stop, within_minute) {
                continue;
            }
            for &p in &self.layer.patterns_for_stop[stop] {
                if running[p as usize] {
                    set.set(p as usize);
                }
            }
        }
        set.ones().collect()
    }

    fn scheduled_round(&self, states: &mut [State], r: usize) {
        let patterns =
            self.patterns_to_explore(states, r - 1, &self.filtered.running_scheduled, true);
        for pi in patterns {
            let Some((sched, _, no_overtaking)) = &self.filtered.patterns[pi] else {
                continue;
            };
            let pattern: &Pattern = &self.layer.patterns[pi];
            let mut on_trip: Option<usize> = None; // index into `sched`
            for (i, &stop) in pattern.stops.iter().enumerate() {
                let stop = stop as usize;
                if let Some(t) = on_trip {
                    if pattern.dropoffs[i] {
                        let trip = &pattern.trips[sched[t] as usize];
                        states[r].set_time_at_stop(stop, trip.arrivals[i], false);
                    }
                }
                if stop_was_updated(states, r - 1, stop, true) && pattern.pickups[i] {
                    let earliest = states[r - 1].best[stop] + MINIMUM_BOARD_WAIT_SEC;
                    if let Some(t) = on_trip {
                        if earliest >= pattern.trips[sched[t] as usize].departures[i] {
                            continue;
                        }
                    }
                    let dep = |t: usize| pattern.trips[sched[t] as usize].departures[i];
                    let new_trip = match on_trip {
                        Some(cur) if *no_overtaking => {
                            let mut best = cur;
                            let mut cand = cur;
                            while cand > 0 {
                                cand -= 1;
                                if dep(cand) > earliest {
                                    best = cand;
                                } else {
                                    break;
                                }
                            }
                            Some(best)
                        }
                        _ => {
                            let mut best = None;
                            let mut best_dep = i32::MAX;
                            for t in 0..sched.len() {
                                let d = dep(t);
                                if d > earliest && d < best_dep {
                                    best = Some(t);
                                    best_dep = d;
                                    if *no_overtaking {
                                        break;
                                    }
                                }
                            }
                            best
                        }
                    };
                    if new_trip != on_trip && new_trip.is_some() {
                        on_trip = new_trip;
                    }
                }
            }
        }
    }

    fn frequency_round(&self, states: &mut [State], r: usize, mode: FreqMode, offsets: &Offsets) {
        let within_minute = mode == FreqMode::UpperBound;
        let patterns = self.patterns_to_explore(
            states,
            r - 1,
            &self.filtered.running_frequency,
            within_minute,
        );
        for pi in patterns {
            let Some((_, freq, _)) = &self.filtered.patterns[pi] else {
                continue;
            };
            let pattern = &self.layer.patterns[pi];
            for &ti in freq {
                let trip = &pattern.trips[ti as usize];
                for (e, entry) in trip.frequencies.iter().enumerate() {
                    let mut board_time = -1;
                    let mut board_pos = 0usize;
                    for (i, &stop) in pattern.stops.iter().enumerate() {
                        let stop = stop as usize;
                        if board_time > -1 && pattern.dropoffs[i] {
                            let alight =
                                board_time + (trip.arrivals[i] - trip.departures[board_pos]);
                            states[r].set_time_at_stop(stop, alight, false);
                        }
                        if stop_was_updated(states, r - 1, stop, within_minute)
                            && pattern.pickups[i]
                        {
                            let earliest = states[r - 1].best[stop] + MINIMUM_BOARD_WAIT_SEC;
                            let h = entry.headway;
                            let tt = trip.departures[i];
                            let cand = match mode {
                                FreqMode::MonteCarlo => {
                                    let offset = offsets.offsets[pi][ti as usize][e];
                                    let first = entry.start + tt + offset;
                                    let n_trips = (entry.end - (entry.start + offset)) / h + 1;
                                    let after = earliest - 1;
                                    let k = if after <= first {
                                        0
                                    } else {
                                        (after - first) / h + 1
                                    };
                                    if k < n_trips {
                                        first + k * h
                                    } else {
                                        -1
                                    }
                                }
                                FreqMode::UpperBound => {
                                    if entry.end - h + tt < earliest {
                                        -1
                                    } else {
                                        (earliest + h).max(entry.start + h + tt)
                                    }
                                }
                            };
                            let stay = if board_time > -1 {
                                board_time + trip.departures[i] - trip.departures[board_pos]
                            } else {
                                i32::MAX
                            };
                            if cand > -1 && cand < stay {
                                board_time = cand;
                                board_pos = i;
                            }
                        }
                    }
                }
            }
        }
    }

    fn transfers_round(&self, state: &mut State) {
        let max_mm = self.req.walk_mms.saturating_mul(self.req.max_walk_min * 60);
        let from: Vec<usize> = state.nt_updated.ones().collect();
        for stop in from {
            for &(target, mm) in &self.transfers[stop] {
                if mm < max_mm {
                    let t = state.best_nt[stop] + mm / self.req.walk_mms;
                    state.set_time_at_stop(target as usize, t, true);
                }
            }
        }
    }
}

/// Run the search for one origin. `access` holds `(stop, seconds)`.
/// Returns, per iteration, the vehicle-arrival duration at every stop
/// (`UNREACHED` where none), in R5 iteration order (latest minute first).
pub fn route(
    layer: &TransitLayer,
    transfers: &[Vec<(u32, i32)>],
    filtered: &Filtered,
    access: &[(u32, i32)],
    req: &RaptorRequest,
) -> Vec<Vec<i32>> {
    let n_stops = layer.stop_ids.len();
    let w = Worker {
        layer,
        transfers,
        filtered,
        req,
        n_stops,
    };
    let per_minute = req.iterations_per_minute(layer.has_frequencies);
    let mt = match req.seed {
        Some(seed) => {
            let h = objects_hash_int_double_double(seed, req.origin.0, req.origin.1);
            MersenneTwister::from_long(i64::from(h))
        }
        None => {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos() as i64);
            MersenneTwister::from_long(nanos)
        }
    };
    let mut offsets = Offsets::new(layer, mt);
    let mut out = Vec::with_capacity(req.time_window as usize * per_minute);
    let mut sched: Vec<State> = Vec::new();
    let to_time = req.from_time + req.time_window * DEPARTURE_STEP_SEC;
    let mut dep = to_time - DEPARTURE_STEP_SEC;
    while dep >= req.from_time {
        if sched.is_empty() {
            sched = (0..=req.max_rides)
                .map(|_| State::new(n_stops, req.max_duration, dep))
                .collect();
        } else {
            for s in &mut sched {
                s.set_departure_time(dep);
            }
        }
        for &(stop, t) in access {
            sched[0].set_time_at_stop(stop as usize, t + dep, true);
        }
        if layer.has_schedules {
            for r in 1..=req.max_rides {
                min_merge_previous(&mut sched, r);
                w.scheduled_round(&mut sched, r);
                if layer.has_frequencies {
                    w.frequency_round(&mut sched, r, FreqMode::UpperBound, &offsets);
                }
                if r < req.max_rides {
                    w.transfers_round(&mut sched[r]);
                }
            }
        }
        let results: Vec<Vec<i32>> = if layer.has_frequencies {
            (0..per_minute)
                .map(|_| {
                    let mut f: Vec<State> = sched.iter().map(State::copy).collect();
                    offsets.randomize(layer);
                    for r in 1..=req.max_rides {
                        min_merge_previous(&mut f, r);
                        w.scheduled_round(&mut f, r);
                        w.frequency_round(&mut f, r, FreqMode::MonteCarlo, &offsets);
                        if r < req.max_rides {
                            w.transfers_round(&mut f[r]);
                        }
                    }
                    f[req.max_rides].best_nt.clone()
                })
                .collect()
        } else {
            vec![sched[req.max_rides].best_nt.clone()]
        };
        for arr in results {
            out.push(
                arr.into_iter()
                    .map(|t| if t == UNREACHED { UNREACHED } else { t - dep })
                    .collect(),
            );
        }
        dep -= DEPARTURE_STEP_SEC;
    }
    out
}
