//! Range RAPTOR over a departure window, a literal port of R5 7.6
//! `FastRaptorWorker` + `RaptorState` + `FrequencyRandomOffsets` (the path
//! r5r takes without fares): scheduled search carried from minute to minute,
//! frequency upper bounds, and Monte Carlo copies with random offsets drawn
//! in R5's order so results are reproducible with r5r's `seed`.

use super::layer::{Pattern, TransitLayer};
use crate::jcompat::{objects_hash_int_double_double, MersenneTwister, TroveIntKeys};
use crate::CancelToken;

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

/// One round's state (R5 `RaptorState`). The path bookkeeping arrays
/// (`previousPatterns`, `previousStop`, `previousBoardTime`,
/// `transferStop`) are only kept when `P` (paths requested): otherwise they
/// stay empty and every access to them is compiled out, so the plain matrix
/// runs the same code as before paths existed.
#[derive(Clone)]
struct State<const P: bool> {
    best: Vec<i32>,
    best_nt: Vec<i32>,
    updated: BitSet,
    nt_updated: BitSet,
    departure: i32,
    max_duration: i32,
    prev_pattern: Vec<i32>,
    prev_stop: Vec<i32>,
    prev_board: Vec<i32>,
    transfer_stop: Vec<i32>,
}

impl<const P: bool> State<P> {
    fn new(n: usize, max_duration: i32, departure: i32) -> Self {
        let m = if P { n } else { 0 };
        State {
            best: vec![UNREACHED; n],
            best_nt: vec![UNREACHED; n],
            updated: BitSet::new(n),
            nt_updated: BitSet::new(n),
            departure,
            max_duration,
            prev_pattern: vec![-1; m],
            prev_stop: vec![-1; m],
            prev_board: vec![-1; m],
            transfer_stop: vec![-1; m],
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
            prev_pattern: self.prev_pattern.clone(),
            prev_stop: self.prev_stop.clone(),
            prev_board: self.prev_board.clone(),
            transfer_stop: self.transfer_stop.clone(),
        }
    }

    /// `copy()` into an existing state of the same size.
    fn copy_from(&mut self, o: &Self) {
        self.best.copy_from_slice(&o.best);
        self.best_nt.copy_from_slice(&o.best_nt);
        self.updated.clear();
        self.nt_updated.clear();
        self.departure = o.departure;
        self.max_duration = o.max_duration;
        if P {
            self.prev_pattern.copy_from_slice(&o.prev_pattern);
            self.prev_stop.copy_from_slice(&o.prev_stop);
            self.prev_board.copy_from_slice(&o.prev_board);
            self.transfer_stop.copy_from_slice(&o.transfer_stop);
        }
    }

    /// `setTimeAtStop(stop, time, fromPattern, fromStop, boardTime, transfer)`
    /// (`RaptorState.java:210-252`).
    #[inline]
    fn set_time_at_stop(
        &mut self,
        stop: usize,
        time: i32,
        pattern: i32,
        from_stop: i32,
        board_time: i32,
        transfer: bool,
    ) -> bool {
        if time >= self.departure + self.max_duration {
            return false;
        }
        let mut optimal = false;
        if !transfer && time < self.best_nt[stop] {
            self.best_nt[stop] = time;
            if P {
                self.prev_pattern[stop] = pattern;
                self.prev_stop[stop] = from_stop;
                self.prev_board[stop] = board_time;
            }
            self.nt_updated.set(stop);
            optimal = true;
        }
        if time < self.best[stop] {
            self.best[stop] = time;
            if P {
                self.transfer_stop[stop] = if transfer { from_stop } else { -1 };
            }
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
        if !P {
            // the same, written so that it vectorizes
            for t in self.best.iter_mut().chain(self.best_nt.iter_mut()) {
                *t = if *t >= max_clock { UNREACHED } else { *t };
            }
            return;
        }
        for i in 0..self.best.len() {
            if self.best[i] >= max_clock {
                self.best[i] = UNREACHED;
                if P {
                    self.transfer_stop[i] = -1;
                }
            }
            if self.best_nt[i] >= max_clock {
                self.best_nt[i] = UNREACHED;
                if P {
                    self.prev_pattern[i] = -1;
                    self.prev_stop[i] = -1;
                    self.prev_board[i] = -1;
                }
            }
        }
    }

    /// Walking time within this round to the optimal arrival at `stop`
    /// (`RaptorState.transferTime`, `:337-340`).
    fn transfer_time(&self, stop: usize) -> i32 {
        match self.transfer_stop[stop] {
            -1 => 0,
            from => self.best[stop] - self.best_nt[from as usize],
        }
    }
}

/// `shorterAccessOrTransferLeg(here, there)` of round `r`
/// (`RaptorState.java:320-334`): round 0 compares access arrivals, later
/// rounds the transfer walks.
fn shorter_access_or_transfer_leg<const P: bool>(
    states: &[State<P>],
    r: usize,
    here: usize,
    there: usize,
) -> bool {
    if here == there {
        return false;
    }
    let s = &states[r];
    if r == 0 {
        s.best[here] < s.best[there]
    } else {
        s.transfer_time(here) < s.transfer_time(there)
    }
}

/// `minMergePrevious` of round `r` from round `r - 1`; ties prefer the
/// previous round (`RaptorState.java:180-198`).
fn min_merge_previous<const P: bool>(states: &mut [State<P>], r: usize) {
    let (prev, cur) = states.split_at_mut(r);
    let (p, c) = (&prev[r - 1], &mut cur[0]);
    if !P {
        // ties take the previous value, which is the same value: a min
        for (c, &p) in c.best.iter_mut().zip(&p.best) {
            *c = (*c).min(p);
        }
        for (c, &p) in c.best_nt.iter_mut().zip(&p.best_nt) {
            *c = (*c).min(p);
        }
        return;
    }
    for s in 0..c.best.len() {
        if p.best[s] <= c.best[s] {
            c.best[s] = p.best[s];
            if P {
                c.transfer_stop[s] = p.transfer_stop[s];
            }
        }
        if p.best_nt[s] <= c.best_nt[s] {
            c.best_nt[s] = p.best_nt[s];
            if P {
                c.prev_pattern[s] = p.prev_pattern[s];
                c.prev_stop[s] = p.prev_stop[s];
                c.prev_board[s] = p.prev_board[s];
            }
        }
    }
}

/// `stopWasUpdated` of round `r` (previous round `r - 1`, none for round 0).
#[inline]
fn stop_was_updated<const P: bool>(
    states: &[State<P>],
    r: usize,
    stop: usize,
    within_minute: bool,
) -> bool {
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

/// First index in `0..n` where `pred` is false, for a `pred` that is true
/// on a prefix (`slice::partition_point` over indices).
#[inline]
fn partition_point_by(n: usize, pred: impl Fn(usize) -> bool) -> usize {
    let (mut lo, mut hi) = (0, n);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if pred(mid) {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// Running scheduled and frequency trip indices of a pattern, and whether
/// the scheduled ones never overtake each other.
type RunningTrips = (Vec<u32>, Vec<u32>, bool);

/// The running scheduled trips of a pattern copied into flat trip-major
/// tables, `arrivals[t * n_stops + i]` for the `t`-th running trip at the
/// pattern's `i`-th stop, so the scheduled round reads one array instead of
/// following a pointer per trip.
#[derive(Default)]
struct ScheduleTable {
    n_stops: usize,
    arrivals: Vec<i32>,
    departures: Vec<i32>,
}

/// Patterns running on the request date (R5 `FilteredPatterns`).
pub struct Filtered {
    /// Per pattern, indices into `pattern.trips` of running scheduled and
    /// frequency trips, and whether scheduled trips never overtake.
    patterns: Vec<Option<RunningTrips>>,
    /// Per pattern, the running scheduled trips' times (empty if none).
    tables: Vec<ScheduleTable>,
    /// Per pattern, parallel to its running frequency trips: the least and
    /// greatest departure offset over the stops, and whether times never
    /// decrease along the trip.
    freq_bounds: Vec<Vec<(i32, i32, bool)>>,
    running_scheduled: Vec<bool>,
    running_frequency: Vec<bool>,
}

impl Filtered {
    /// Patterns with an active service whose mode is allowed. Errors on a
    /// route type R5 does not know (R5 fails the whole request then).
    pub fn new(layer: &TransitLayer, active: &[bool]) -> Result<Filtered, String> {
        let mut f = Filtered {
            patterns: Vec::with_capacity(layer.patterns.len()),
            tables: Vec::with_capacity(layer.patterns.len()),
            freq_bounds: Vec::with_capacity(layer.patterns.len()),
            running_scheduled: vec![false; layer.patterns.len()],
            running_frequency: vec![false; layer.patterns.len()],
        };
        for (pi, p) in layer.patterns.iter().enumerate() {
            let allowed = route_type_allowed(p.route_type)
                .ok_or_else(|| format!("unsupported GTFS route_type {}", p.route_type))?;
            if !(allowed && p.services.iter().any(|&s| active[s as usize])) {
                f.patterns.push(None);
                f.tables.push(ScheduleTable::default());
                f.freq_bounds.push(Vec::new());
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
            let mut table = ScheduleTable {
                n_stops: p.stops.len(),
                arrivals: Vec::with_capacity(sched.len() * p.stops.len()),
                departures: Vec::with_capacity(sched.len() * p.stops.len()),
            };
            for &t in &sched {
                let trip = &p.trips[t as usize];
                // GTFS loading gives every trip one time per pattern stop
                if trip.arrivals.len() != p.stops.len() || trip.departures.len() != p.stops.len() {
                    return Err(format!(
                        "trip {} has {} times for {} stops",
                        trip.trip_id,
                        trip.arrivals.len(),
                        p.stops.len()
                    ));
                }
                table.arrivals.extend_from_slice(&trip.arrivals);
                table.departures.extend_from_slice(&trip.departures);
            }
            f.tables.push(table);
            f.freq_bounds.push(
                freq.iter()
                    .map(|&t| {
                        let trip = &p.trips[t as usize];
                        let d = &trip.departures;
                        let a = &trip.arrivals;
                        let monotone = a.len() == d.len()
                            && (0..d.len()).all(|i| d[i] >= a[i])
                            && (1..d.len()).all(|i| a[i] >= d[i - 1]);
                        let lo = d.iter().copied().min().unwrap_or(0);
                        let hi = d.iter().copied().max().unwrap_or(0);
                        (lo, hi, monotone)
                    })
                    .collect(),
            );
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
    #[cfg(test)]
    /// Check that the search can run: a window of at least one minute,
    /// positive durations and speeds, and clock times (`from_time` plus the
    /// window plus `max_duration`) that fit the `i32` seconds every state
    /// holds. [`route`] and [`route_with_paths`] run this first.
    pub fn validate(&self) -> Result<(), String> {
        if self.time_window < 1 {
            return Err(format!(
                "time_window must be at least 1 minute, got {}",
                self.time_window
            ));
        }
        if self.from_time < 0 {
            return Err(format!(
                "from_time must not be negative, got {}",
                self.from_time
            ));
        }
        if self.max_duration < 1 {
            return Err(format!(
                "max_duration must be at least 1 second, got {}",
                self.max_duration
            ));
        }
        if self.walk_mms < 1 {
            return Err(format!(
                "walk_mms must be at least 1, got {}",
                self.walk_mms
            ));
        }
        if self.max_walk_min < 0 || self.max_walk_min.checked_mul(60).is_none() {
            return Err(format!(
                "max_walk_min must be from 0 to {} minutes, got {}",
                i32::MAX / 60,
                self.max_walk_min
            ));
        }
        if self.monte_carlo_draws < 0 {
            return Err(format!(
                "monte_carlo_draws must not be negative, got {}",
                self.monte_carlo_draws
            ));
        }
        if self.max_rides > i32::MAX as usize {
            return Err(format!(
                "max_rides must be at most {}, got {}",
                i32::MAX,
                self.max_rides
            ));
        }
        let last_clock = i64::from(self.from_time)
            + i64::from(self.time_window) * i64::from(DEPARTURE_STEP_SEC)
            + i64::from(self.max_duration);
        if last_clock > i64::from(i32::MAX) {
            return Err(format!(
                "from_time ({} s) + time_window ({} min) + max_duration ({} s) must stay \
                 within {} seconds",
                self.from_time,
                self.time_window,
                self.max_duration,
                i32::MAX
            ));
        }
        Ok(())
    }

    /// Iterations per departure minute (R5 `getMonteCarloDrawsPerMinute`);
    /// an error for a window under one minute.
    pub fn iterations_per_minute(&self, has_frequencies: bool) -> Result<usize, String> {
        if self.time_window < 1 {
            return Err(format!(
                "time_window must be at least 1 minute, got {}",
                self.time_window
            ));
        }
        Ok(self.per_minute(has_frequencies))
    }

    /// [`Self::iterations_per_minute`] of a validated request.
    fn per_minute(&self, has_frequencies: bool) -> usize {
        if !has_frequencies || self.monte_carlo_draws <= 0 {
            1
        } else {
            (f64::from(self.monte_carlo_draws) / f64::from(self.time_window)).ceil() as usize
        }
    }

    /// Iterations of a validated request: minutes x iterations per minute.
    fn n_iterations(&self, has_frequencies: bool) -> usize {
        self.time_window.max(0) as usize * self.per_minute(has_frequencies)
    }
}

#[cfg(test)]
/// The checks of the public entry points: the request, and access stops
/// that exist with non-negative times.
fn check_input(
    layer: &TransitLayer,
    access: &[(u32, i32)],
    req: &RaptorRequest,
) -> Result<(), String> {
    req.validate()?;
    let n = layer.stop_ids.len();
    if let Some(&(s, t)) = access.iter().find(|&&(s, t)| s as usize >= n || t < 0) {
        return Err(format!(
            "access entry (stop {s}, {t} s) is not a stop of the layer ({n} stops) with a \
             non-negative time"
        ));
    }
    Ok(())
}

struct Worker<'a> {
    layer: &'a TransitLayer,
    transfers: &'a [Vec<(u32, i32)>],
    filtered: &'a Filtered,
    req: &'a RaptorRequest,
    n_stops: usize,
}

impl Worker<'_> {
    fn patterns_to_explore<const P: bool>(
        &self,
        input: &[State<P>],
        r: usize,
        running: &[bool],
        within_minute: bool,
    ) -> Vec<usize> {
        let mut set = BitSet::new(self.layer.patterns.len());
        let mut add = |stop: usize| {
            for &p in &self.layer.patterns_for_stop[stop] {
                if running[p as usize] {
                    set.set(p as usize);
                }
            }
        };
        if within_minute {
            // `stop_was_updated` is the update flag: visit just those stops
            input[r].updated.ones().for_each(&mut add);
        } else {
            for stop in 0..self.n_stops {
                if stop_was_updated(input, r, stop, within_minute) {
                    add(stop);
                }
            }
        }
        set.ones().collect()
    }

    fn scheduled_round<const P: bool>(&self, states: &mut [State<P>], r: usize) {
        let patterns =
            self.patterns_to_explore(states, r - 1, &self.filtered.running_scheduled, true);
        for pi in patterns {
            let Some((sched, _, no_overtaking)) = &self.filtered.patterns[pi] else {
                continue;
            };
            let table = &self.filtered.tables[pi];
            let ns = table.n_stops;
            let pattern: &Pattern = &self.layer.patterns[pi];
            let mut on_trip: Option<usize> = None; // index into `sched`
            let mut board_time = -1;
            let mut board_stop = -1;
            for (i, &stop) in pattern.stops.iter().enumerate() {
                let stop = stop as usize;
                if let Some(t) = on_trip {
                    if pattern.dropoffs[i] {
                        states[r].set_time_at_stop(
                            stop,
                            table.arrivals[t * ns + i],
                            pi as i32,
                            board_stop,
                            board_time,
                            false,
                        );
                    }
                }
                if stop_was_updated(states, r - 1, stop, true) && pattern.pickups[i] {
                    let earliest = states[r - 1].best[stop] + MINIMUM_BOARD_WAIT_SEC;
                    if let Some(t) = on_trip {
                        if earliest >= table.departures[t * ns + i] {
                            continue;
                        }
                    }
                    let dep = |t: usize| table.departures[t * ns + i];
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
                        _ if *no_overtaking => {
                            // Departures never decrease from one trip to the
                            // next, so R5's scan for the first trip leaving
                            // after `earliest` stops where this search lands.
                            let t = partition_point_by(sched.len(), |t| dep(t) <= earliest);
                            // (the scan also skips a departure of
                            // i32::MAX, and so every later one)
                            (t < sched.len() && dep(t) < i32::MAX).then_some(t)
                        }
                        _ => {
                            let mut best = None;
                            let mut best_dep = i32::MAX;
                            for t in 0..sched.len() {
                                let d = dep(t);
                                if d > earliest && d < best_dep {
                                    best = Some(t);
                                    best_dep = d;
                                }
                            }
                            best
                        }
                    };
                    // With paths, re-board the same trip here when that
                    // shortens the access/transfer walk: times are unchanged,
                    // only the walk/wait breakdown moves
                    // (`FastRaptorWorker.java:602-617`).
                    let reboard_for_paths = P
                        && on_trip.is_some()
                        && shorter_access_or_transfer_leg(states, r - 1, stop, board_stop as usize);
                    if (new_trip != on_trip || reboard_for_paths) && new_trip.is_some() {
                        on_trip = new_trip;
                        board_time = dep(new_trip.unwrap_or_default());
                        board_stop = stop as i32;
                    }
                }
            }
        }
    }

    fn frequency_round<const P: bool>(
        &self,
        states: &mut [State<P>],
        r: usize,
        mode: FreqMode,
        offsets: &Offsets,
    ) {
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
            // Every stop's earliest boarding is at least a minute after the
            // departure (states hold clock times from the departure on),
            // and alighting at or after departure + max duration is
            // discarded. An entry whose vehicles all leave before the first
            // or, on a trip whose times never decrease, after the second
            // cannot change a state: skip it.
            let (dep, dur) = (
                i64::from(states[r].departure),
                i64::from(states[r].max_duration),
            );
            let earliest_any = dep + i64::from(MINIMUM_BOARD_WAIT_SEC);
            for (&ti, &(min_tt, max_tt, monotone)) in
                freq.iter().zip(&self.filtered.freq_bounds[pi])
            {
                let trip = &pattern.trips[ti as usize];
                for (e, entry) in trip.frequencies.iter().enumerate() {
                    let (start, end, h) = (
                        i64::from(entry.start),
                        i64::from(entry.end),
                        i64::from(entry.headway),
                    );
                    let (last_board, first_board) = match mode {
                        FreqMode::MonteCarlo => {
                            let offset = i64::from(offsets.offsets[pi][ti as usize][e]);
                            let n_trips = (end - (start + offset)) / h + 1;
                            if n_trips <= 0 {
                                continue; // no vehicle of this entry boards
                            }
                            // (+1: the rule below still boards a vehicle
                            // that leaves one second before the earliest
                            // boarding time)
                            (
                                start + offset + i64::from(max_tt) + (n_trips - 1) * h + 1,
                                start + offset + i64::from(min_tt),
                            )
                        }
                        FreqMode::UpperBound => {
                            (end - h + i64::from(max_tt), start + h + i64::from(min_tt))
                        }
                    };
                    if last_board < earliest_any || (monotone && first_board >= dep + dur) {
                        continue;
                    }
                    let mut board_time = -1;
                    let mut board_pos = 0usize;
                    for (i, &stop) in pattern.stops.iter().enumerate() {
                        let stop = stop as usize;
                        if board_time > -1 && pattern.dropoffs[i] {
                            let alight =
                                board_time + (trip.arrivals[i] - trip.departures[board_pos]);
                            states[r].set_time_at_stop(
                                stop,
                                alight,
                                pi as i32,
                                pattern.stops[board_pos] as i32,
                                board_time,
                                false,
                            );
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

    fn transfers_round<const P: bool>(&self, state: &mut State<P>) {
        let max_mm = self.req.walk_mms.saturating_mul(self.req.max_walk_min * 60);
        let from: Vec<usize> = state.nt_updated.ones().collect();
        for stop in from {
            for &(target, mm) in &self.transfers[stop] {
                if mm < max_mm {
                    // saturating: past i32::MAX is past any max_duration
                    let t = state.best_nt[stop].saturating_add(mm / self.req.walk_mms);
                    state.set_time_at_stop(target as usize, t, -1, stop as i32, -1, true);
                }
            }
        }
    }
}

#[cfg(test)]
/// A transit itinerary to a stop, as R5's `Path` (`transit/path/Path.java`):
/// the vehicles ridden at clock times, read back from the round states.
/// One `TransitPath` can serve several departure minutes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitPath {
    /// Pattern index of each leg.
    pub patterns: Vec<u32>,
    pub board_stops: Vec<u32>,
    pub alight_stops: Vec<u32>,
    /// Clock time at which each leg's vehicle leaves its boarding stop.
    pub board_times: Vec<i32>,
    /// In-vehicle seconds of each leg.
    pub ride_times: Vec<i32>,
    /// Walking seconds of the transfer before each leg (0 for the first).
    pub transfer_times: Vec<i32>,
}

#[cfg(test)]
impl TransitPath {
    /// `computeWaitTimes(departureTime)` (`Path.java:103-117`) given the
    /// access walk to the first boarding stop. R5 asserts the waits are
    /// non-negative; `None` here if one is not.
    pub fn wait_times(&self, departure: i32, access: i32) -> Option<Vec<i32>> {
        let mut ready = departure + access;
        let mut waits = Vec::with_capacity(self.board_times.len());
        for leg in 0..self.board_times.len() {
            ready += self.transfer_times[leg];
            let w = self.board_times[leg] - ready;
            if w < 0 {
                return None;
            }
            waits.push(w);
            ready = self.board_times[leg] + self.ride_times[leg];
        }
        Some(waits)
    }
}

/// No path to a stop in an iteration.
pub const NO_PATH: u32 = u32::MAX;

#[cfg(test)]
/// Paths kept by a search with [`route_with_paths`].
pub struct PathResults {
    /// Distinct path objects; several iterations may share one.
    pub paths: Vec<TransitPath>,
    /// Per iteration, per retained stop, an index into `paths` or `NO_PATH`.
    pub path_of: Vec<Vec<u32>>,
    /// Column of each stop in the `path_of` rows, `u32::MAX` if not retained.
    pub slot: Vec<u32>,
}

#[cfg(test)]
impl PathResults {
    /// Path to `stop` in `iteration`, if any.
    pub fn get(&self, iteration: usize, stop: usize) -> Option<&TransitPath> {
        let k = *self.slot.get(stop)?;
        if k == u32::MAX {
            return None;
        }
        match self.path_of.get(iteration)?[k as usize] {
            NO_PATH => None,
            i => Some(&self.paths[i as usize]),
        }
    }
}

/// One ride of a path and the transfer walk before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Leg {
    pub(crate) pattern: u32,
    pub(crate) board_stop: u32,
    pub(crate) alight_stop: u32,
    /// Clock time at which the vehicle leaves the boarding stop.
    pub(crate) board_time: i32,
    pub(crate) ride_time: i32,
    /// Walking seconds of the transfer before this leg (0 for the first).
    pub(crate) transfer_time: i32,
}

/// The paths of one search, stored flat: what [`PathResults`] holds, with
/// every path's legs in one array and the per-iteration rows in another,
/// reused from origin to origin instead of a handful of small allocations
/// per path.
#[derive(Default)]
pub(crate) struct PathArena {
    legs: Vec<Leg>,
    /// Per path, its range in `legs`.
    paths: Vec<(u32, u32)>,
    /// Retained stops, increasing.
    retained: Vec<u32>,
    /// Column of each stop in the rows, `u32::MAX` if not retained.
    slot: Vec<u32>,
    /// Iteration-major rows of path indices (`NO_PATH` where none).
    path_of: Vec<u32>,
    n_rows: usize,
    /// The scheduled search's retained paths (R5 `retainedPaths`): per
    /// retained stop, the path built when its arrival last changed.
    kept: Vec<u32>,
    kept_arrival: Vec<i32>,
}

impl PathArena {
    fn reset(&mut self, n_stops: usize, retain: &[bool]) {
        self.legs.clear();
        self.paths.clear();
        self.retained.clear();
        self.retained.extend(
            (0..n_stops)
                .filter(|&s| retain.get(s).copied().unwrap_or(false))
                .map(|s| s as u32),
        );
        self.slot.clear();
        self.slot.resize(n_stops, u32::MAX);
        for (k, &s) in self.retained.iter().enumerate() {
            self.slot[s as usize] = k as u32;
        }
        self.path_of.clear();
        self.n_rows = 0;
        self.kept.clear();
        self.kept.resize(self.retained.len(), NO_PATH);
        self.kept_arrival.clear();
        self.kept_arrival.resize(self.retained.len(), -1);
    }

    /// Number of distinct paths.
    pub(crate) fn len(&self) -> usize {
        self.paths.len()
    }

    /// The legs of path `k`.
    pub(crate) fn legs(&self, k: u32) -> &[Leg] {
        let (a, b) = self.paths[k as usize];
        &self.legs[a as usize..b as usize]
    }

    /// Index of the path to `stop` in `iteration`, if any.
    pub(crate) fn index(&self, iteration: usize, stop: usize) -> Option<u32> {
        let k = *self.slot.get(stop)?;
        if k == u32::MAX || iteration >= self.n_rows {
            return None;
        }
        match self.path_of[iteration * self.retained.len() + k as usize] {
            NO_PATH => None,
            i => Some(i),
        }
    }

    /// `new Path(state, stop)` (`Path.java:30-95`): step back through the
    /// rounds, skipping rounds that did not change the stop's vehicle
    /// arrival, recording each ride and the transfer that led to it.
    /// Returns the new path's index.
    fn reconstruct<const P: bool>(&mut self, states: &[State<P>], stop: usize) -> u32 {
        let start = self.legs.len();
        let mut r = states.len() - 1;
        let mut stop = stop;
        while r > 0 {
            let (prev, cur) = (&states[r - 1], &states[r]);
            if prev.best_nt[stop] == cur.best_nt[stop] {
                r -= 1;
                continue;
            }
            let alight = cur.best_nt[stop];
            let board = cur.prev_board[stop];
            let pattern = cur.prev_pattern[stop] as u32;
            let alight_stop = stop as u32;
            stop = cur.prev_stop[stop] as usize;
            let board_stop = stop as u32;
            r -= 1;
            let prev = &states[r];
            let transfer_time = match prev.transfer_stop[stop] {
                -1 => 0,
                from => {
                    let t = prev.best[stop] - prev.best_nt[from as usize];
                    stop = from as usize;
                    t
                }
            };
            self.legs.push(Leg {
                pattern,
                board_stop,
                alight_stop,
                board_time: board,
                ride_time: alight - board,
                transfer_time,
            });
        }
        self.legs[start..].reverse();
        self.paths.push((start as u32, self.legs.len() as u32));
        (self.paths.len() - 1) as u32
    }

    /// `pathToEachStop` (`FastRaptorWorker.java:472-494`): one row of path
    /// indices. With `reuse` (the scheduled search only) an unchanged
    /// arrival at a stop keeps the path built at a later minute, as R5's
    /// `retainedPaths`.
    fn add_row<const P: bool>(&mut self, states: &[State<P>], reuse: bool) {
        let last = &states[states.len() - 1];
        let n = self.retained.len();
        self.path_of.resize(self.path_of.len() + n, NO_PATH);
        let row0 = self.n_rows * n;
        self.n_rows += 1;
        for k in 0..n {
            let s = self.retained[k] as usize;
            let arrival = last.best_nt[s];
            if arrival == UNREACHED {
                continue;
            }
            if reuse && self.kept_arrival[k] == arrival {
                self.path_of[row0 + k] = self.kept[k];
                continue;
            }
            let i = self.reconstruct(states, s);
            self.path_of[row0 + k] = i;
            if reuse {
                self.kept[k] = i;
                self.kept_arrival[k] = arrival;
            }
        }
    }

    #[cfg(test)]
    /// The same paths as public [`PathResults`].
    fn to_results(&self) -> PathResults {
        let n = self.retained.len();
        PathResults {
            paths: (0..self.paths.len() as u32)
                .map(|k| {
                    let legs = self.legs(k);
                    TransitPath {
                        patterns: legs.iter().map(|l| l.pattern).collect(),
                        board_stops: legs.iter().map(|l| l.board_stop).collect(),
                        alight_stops: legs.iter().map(|l| l.alight_stop).collect(),
                        board_times: legs.iter().map(|l| l.board_time).collect(),
                        ride_times: legs.iter().map(|l| l.ride_time).collect(),
                        transfer_times: legs.iter().map(|l| l.transfer_time).collect(),
                    }
                })
                .collect(),
            path_of: (0..self.n_rows)
                .map(|r| self.path_of[r * n..(r + 1) * n].to_vec())
                .collect(),
            slot: self.slot.clone(),
        }
    }
}

#[cfg(test)]
/// Run the search for one origin. `access` holds `(stop, seconds)`.
/// Returns, per iteration, the vehicle-arrival duration at every stop
/// (`UNREACHED` where none), in R5 iteration order (latest minute first);
/// an error when the request or the access entries are invalid (see
/// [`RaptorRequest::validate`]).
pub fn route(
    layer: &TransitLayer,
    transfers: &[Vec<(u32, i32)>],
    filtered: &Filtered,
    access: &[(u32, i32)],
    req: &RaptorRequest,
) -> Result<Vec<Vec<i32>>, String> {
    check_input(layer, access, req)?;
    let mut out = Vec::new();
    let mut arena = PathArena::default();
    route_impl::<false>(
        layer,
        transfers,
        filtered,
        access,
        req,
        &CancelToken::default(),
        &mut arena,
        |_, arr| out.push(arr.to_vec()),
    );
    Ok(out)
}

/// Vehicle-arrival durations of every iteration at every stop, stop-major:
/// `get(stop)[k]` is iteration `k`'s value (`UNREACHED` where none), so
/// propagation to a destination reads each stop's iterations contiguously.
#[derive(Default)]
pub(crate) struct StopTimes {
    n_iter: usize,
    data: Vec<i32>,
    /// Per stop, the least value over the iterations.
    min: Vec<i32>,
}

impl StopTimes {
    pub(crate) fn n_iterations(&self) -> usize {
        self.n_iter
    }

    #[inline]
    pub(crate) fn get(&self, stop: usize) -> &[i32] {
        &self.data[stop * self.n_iter..(stop + 1) * self.n_iter]
    }

    /// The least value of `stop` over all iterations.
    #[inline]
    pub(crate) fn min(&self, stop: usize) -> i32 {
        self.min[stop]
    }
}

/// [`route`] writing into a reused stop-major buffer; incomplete when
/// `cancel` is cancelled meanwhile.
pub(crate) fn route_stop_major(
    layer: &TransitLayer,
    transfers: &[Vec<(u32, i32)>],
    filtered: &Filtered,
    access: &[(u32, i32)],
    req: &RaptorRequest,
    cancel: &CancelToken,
    out: &mut StopTimes,
) {
    let n_stops = layer.stop_ids.len();
    let n_iter = req.n_iterations(layer.has_frequencies);
    out.n_iter = n_iter;
    out.data.clear();
    out.data.resize(n_iter * n_stops, UNREACHED);
    out.min.clear();
    out.min.resize(n_stops, UNREACHED);
    let mut k = 0;
    let mut arena = PathArena::default();
    route_impl::<false>(
        layer,
        transfers,
        filtered,
        access,
        req,
        cancel,
        &mut arena,
        |_, arr| {
            for (s, &t) in arr.iter().enumerate() {
                out.data[s * n_iter + k] = t;
                if t < out.min[s] {
                    out.min[s] = t;
                }
            }
            k += 1;
        },
    );
    debug_assert!(k == n_iter || cancel.is_cancelled());
}

#[cfg(test)]
/// [`route`] that also keeps, at every iteration, the path to each stop
/// flagged in `retain` (R5 with `retainPaths`). r5r reconstructs paths to
/// all stops; a path depends only on its own stop, so restricting the set
/// to the stops that matter changes nothing. The times are the same as
/// [`route`]'s, and so are the errors.
pub fn route_with_paths(
    layer: &TransitLayer,
    transfers: &[Vec<(u32, i32)>],
    filtered: &Filtered,
    access: &[(u32, i32)],
    req: &RaptorRequest,
    retain: &[bool],
) -> Result<(Vec<Vec<i32>>, PathResults), String> {
    check_input(layer, access, req)?;
    let mut times = Vec::new();
    let mut arena = PathArena::default();
    arena.reset(layer.stop_ids.len(), retain);
    route_impl::<true>(
        layer,
        transfers,
        filtered,
        access,
        req,
        &CancelToken::default(),
        &mut arena,
        |_, arr| times.push(arr.to_vec()),
    );
    Ok((times, arena.to_results()))
}

/// [`route_with_paths`] into reused buffers: `times` gets the durations
/// iteration-major (`times[i * n_stops + stop]`), `arena` the paths;
/// incomplete when `cancel` is cancelled meanwhile.
#[allow(clippy::too_many_arguments)]
pub(crate) fn route_paths_into(
    layer: &TransitLayer,
    transfers: &[Vec<(u32, i32)>],
    filtered: &Filtered,
    access: &[(u32, i32)],
    req: &RaptorRequest,
    retain: &[bool],
    cancel: &CancelToken,
    times: &mut Vec<i32>,
    arena: &mut PathArena,
) {
    times.clear();
    arena.reset(layer.stop_ids.len(), retain);
    route_impl::<true>(
        layer,
        transfers,
        filtered,
        access,
        req,
        cancel,
        arena,
        |_, arr| times.extend_from_slice(arr),
    );
}

/// The search, handing each iteration's durations at every stop to
/// `sink(departure, durations)` in R5 iteration order. It stops early,
/// leaving the iterations after it out, when `cancel` is cancelled: checked
/// at every departure minute and every Monte Carlo draw.
#[allow(clippy::too_many_arguments)]
fn route_impl<const P: bool>(
    layer: &TransitLayer,
    transfers: &[Vec<(u32, i32)>],
    filtered: &Filtered,
    access: &[(u32, i32)],
    req: &RaptorRequest,
    cancel: &CancelToken,
    arena: &mut PathArena,
    mut sink: impl FnMut(i32, &[i32]),
) {
    let n_stops = layer.stop_ids.len();
    let w = Worker {
        layer,
        transfers,
        filtered,
        req,
        n_stops,
    };
    let per_minute = req.per_minute(layer.has_frequencies);
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
    let mut sched: Vec<State<P>> = Vec::new();
    // Monte Carlo copies of `sched`, reused from draw to draw.
    let mut draw: Vec<State<P>> = Vec::new();
    let mut durations = vec![UNREACHED; n_stops];
    let mut emit = |arr: &[i32], dep: i32, sink: &mut dyn FnMut(i32, &[i32])| {
        for (d, &t) in durations.iter_mut().zip(arr) {
            *d = if t == UNREACHED { UNREACHED } else { t - dep };
        }
        sink(dep, &durations);
    };
    let to_time = req.from_time + req.time_window * DEPARTURE_STEP_SEC;
    let mut dep = to_time - DEPARTURE_STEP_SEC;
    while dep >= req.from_time {
        if cancel.is_cancelled() {
            return;
        }
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
            sched[0].set_time_at_stop(stop as usize, t.saturating_add(dep), -1, -1, -1, true);
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
        if layer.has_frequencies {
            for _ in 0..per_minute {
                if cancel.is_cancelled() {
                    return;
                }
                if draw.is_empty() {
                    draw = sched.iter().map(State::copy).collect();
                } else {
                    for (f, s) in draw.iter_mut().zip(&sched) {
                        f.copy_from(s);
                    }
                }
                let f = &mut draw;
                offsets.randomize(layer);
                for r in 1..=req.max_rides {
                    min_merge_previous(f, r);
                    w.scheduled_round(f, r);
                    w.frequency_round(f, r, FreqMode::MonteCarlo, &offsets);
                    if r < req.max_rides {
                        w.transfers_round(&mut f[r]);
                    }
                }
                if P {
                    // No reuse across Monte Carlo draws (`:423-425`).
                    arena.add_row(f, false);
                }
                emit(&f[req.max_rides].best_nt, dep, &mut sink);
            }
        } else {
            if P {
                arena.add_row(&sched, true);
            }
            emit(&sched[req.max_rides].best_nt, dep, &mut sink);
        }
        dep -= DEPARTURE_STEP_SEC;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transit::layer::TripSchedule;

    /// Stops 0..4. Pattern 0 runs 0 -> 1, pattern 1 runs 2 -> 3, stop 1
    /// transfers to stop 2 (50 m), and pattern 2 runs 1 -> 3 later.
    fn layer() -> TransitLayer {
        let trip = |t: [i32; 2]| TripSchedule {
            trip_id: String::new(),
            service: 0,
            arrivals: t.to_vec(),
            departures: t.to_vec(),
            frequencies: Vec::new(),
        };
        let pattern = |route: &str, stops: [u32; 2], t: [i32; 2]| Pattern {
            route_id: route.into(),
            route_type: 3,
            stops: stops.to_vec(),
            pickups: vec![true; 2],
            dropoffs: vec![true; 2],
            trips: vec![trip(t)],
            services: vec![0],
            has_frequencies: false,
            has_schedules: true,
        };
        TransitLayer {
            stop_ids: (0..4).map(|s| format!("f:{s}")).collect(),
            stop_lat: vec![0.0; 4],
            stop_lon: vec![0.0; 4],
            stop_vertex: vec![0; 4],
            patterns: vec![
                pattern("A", [0, 1], [200, 300]),
                pattern("B", [2, 3], [450, 550]),
                pattern("C", [1, 3], [900, 950]),
            ],
            patterns_for_stop: vec![vec![0], vec![0, 2], vec![1], vec![1, 2]],
            has_schedules: true,
            ..TransitLayer::default()
        }
    }

    /// A frequency vehicle that leaves a stop one second before the earliest
    /// boarding time there is still boarded (`after <= first` in the Monte
    /// Carlo rule), also when it is the only vehicle of its entry.
    #[test]
    fn boards_the_only_vehicle_one_second_early() {
        let layer = TransitLayer {
            stop_ids: (0..2).map(|s| format!("f:{s}")).collect(),
            stop_lat: vec![0.0; 2],
            stop_lon: vec![0.0; 2],
            stop_vertex: vec![0; 2],
            patterns: vec![Pattern {
                route_id: "F".into(),
                route_type: 3,
                stops: vec![0, 1],
                pickups: vec![true; 2],
                dropoffs: vec![true; 2],
                trips: vec![TripSchedule {
                    trip_id: String::new(),
                    service: 0,
                    arrivals: vec![0, 0],
                    departures: vec![0, 0],
                    // headway 1: the random offset is always 0
                    frequencies: vec![crate::transit::layer::FrequencyEntry {
                        start: 59,
                        end: 59,
                        headway: 1,
                    }],
                }],
                services: vec![0],
                has_frequencies: true,
                has_schedules: false,
            }],
            patterns_for_stop: vec![vec![0], vec![0]],
            has_frequencies: true,
            ..TransitLayer::default()
        };
        let filtered = Filtered::new(&layer, &[true]).unwrap();
        let req = RaptorRequest {
            from_time: 0,
            time_window: 1,
            max_rides: 1,
            max_duration: 7200,
            walk_mms: 1000,
            max_walk_min: 120,
            monte_carlo_draws: 1,
            seed: Some(1),
            origin: (0.0, 0.0),
        };
        // At stop 0 at 0 s: earliest boarding 60 s, the vehicle leaves at 59.
        let times = route(&layer, &[vec![], vec![]], &filtered, &[(0, 0)], &req).unwrap();
        assert_eq!(times[0][1], 59);
    }

    #[test]
    fn paths_follow_rides_and_transfers() {
        let layer = layer();
        let transfers = vec![vec![], vec![(2, 50_000)], vec![], vec![]];
        let filtered = Filtered::new(&layer, &[true]).unwrap();
        let req = RaptorRequest {
            from_time: 0,
            time_window: 2,
            max_rides: 3,
            max_duration: 7200,
            walk_mms: 1000,
            max_walk_min: 120,
            monte_carlo_draws: 0,
            seed: Some(1),
            origin: (0.0, 0.0),
        };
        let access = [(0, 10)];
        let plain = route(&layer, &transfers, &filtered, &access, &req).unwrap();
        let (times, paths) =
            route_with_paths(&layer, &transfers, &filtered, &access, &req, &[true; 4]).unwrap();
        assert_eq!(times, plain);
        // Iteration 0 departs at 60 s, iteration 1 at 0 s.
        assert_eq!(times[0][3], 550 - 60);
        assert_eq!(times[1][3], 550);
        let p = paths.get(1, 3).unwrap();
        assert_eq!(p.patterns, [0, 1]);
        assert_eq!(p.board_stops, [0, 2]);
        assert_eq!(p.alight_stops, [1, 3]);
        assert_eq!(p.board_times, [200, 450]);
        assert_eq!(p.ride_times, [100, 100]);
        assert_eq!(p.transfer_times, [0, 50]);
        // Ready at 10 s, board at 200; alight at 300, walk 50, board at 450.
        assert_eq!(p.wait_times(0, 10), Some(vec![190, 100]));
        assert_eq!(p.wait_times(195, 10), None);
        // An unchanged arrival reuses the path of the later minute.
        assert!(std::ptr::eq(p, paths.get(0, 3).unwrap()));
        // Stop 0 is reached on foot only, stop 2 by a transfer only.
        assert!(paths.get(0, 0).is_none());
        assert!(paths.get(0, 2).is_none());
        assert_eq!(paths.get(0, 1).unwrap().patterns, [0]);
    }

    #[test]
    fn retained_stops_only() {
        let layer = layer();
        let transfers = vec![vec![], vec![(2, 50_000)], vec![], vec![]];
        let filtered = Filtered::new(&layer, &[true]).unwrap();
        let req = RaptorRequest {
            from_time: 0,
            time_window: 1,
            max_rides: 1,
            max_duration: 7200,
            walk_mms: 1000,
            max_walk_min: 120,
            monte_carlo_draws: 0,
            seed: Some(1),
            origin: (0.0, 0.0),
        };
        let retain = [false, false, false, true];
        let (_, paths) =
            route_with_paths(&layer, &transfers, &filtered, &[(0, 10)], &req, &retain).unwrap();
        assert!(paths.get(0, 1).is_none()); // not retained
        assert!(paths.get(0, 3).is_none()); // needs two rides
    }

    #[test]
    fn reboards_where_the_access_walk_is_shorter() {
        // One trip 0 -> 1 -> 2; stop 1 is closer to the origin than stop 0.
        let layer = TransitLayer {
            stop_ids: (0..3).map(|s| format!("f:{s}")).collect(),
            stop_lat: vec![0.0; 3],
            stop_lon: vec![0.0; 3],
            stop_vertex: vec![0; 3],
            patterns: vec![Pattern {
                route_id: "R".into(),
                route_type: 3,
                stops: vec![0, 1, 2],
                pickups: vec![true; 3],
                dropoffs: vec![true; 3],
                trips: vec![TripSchedule {
                    trip_id: String::new(),
                    service: 0,
                    arrivals: vec![200, 300, 400],
                    departures: vec![200, 300, 400],
                    frequencies: Vec::new(),
                }],
                services: vec![0],
                has_frequencies: false,
                has_schedules: true,
            }],
            patterns_for_stop: vec![vec![0], vec![0], vec![0]],
            has_schedules: true,
            ..TransitLayer::default()
        };
        let filtered = Filtered::new(&layer, &[true]).unwrap();
        let req = RaptorRequest {
            from_time: 0,
            time_window: 1,
            max_rides: 1,
            max_duration: 7200,
            walk_mms: 1000,
            max_walk_min: 120,
            monte_carlo_draws: 0,
            seed: Some(1),
            origin: (0.0, 0.0),
        };
        let transfers = vec![vec![]; 3];
        let access = [(0, 100), (1, 10)];
        let (times, paths) =
            route_with_paths(&layer, &transfers, &filtered, &access, &req, &[true; 3]).unwrap();
        assert_eq!(times[0][2], 400);
        let p = paths.get(0, 2).unwrap();
        assert_eq!(
            (p.board_stops.as_slice(), p.board_times.as_slice()),
            (&[1][..], &[300][..])
        );
    }
}
