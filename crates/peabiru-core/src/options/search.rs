//! The multi-criteria round-based search of [`trip_options`](crate::trip_options):
//! McRAPTOR (Delling, Pajor and Werneck, "Round-Based Public Transit
//! Routing", 2012) from one departure minute, with Pareto bags of labels
//! (arrival time, rides, seconds on foot, fare state) at every stop.
//!
//! # The trips searched
//!
//! A trip is an access leg to a stop, rides and walks between stops (at most
//! one walk between two rides, from the transfer tables), and an egress leg
//! from the stop of the last ride, as in R5's routers. A ride boards at
//! least 60 s after reaching its stop (R5's minimum boarding wait: a
//! scheduled vehicle leaving strictly after, a frequency-based one at or
//! after) and is taken on the vehicle that reaches the alighting stop
//! first, the earlier departure on a tie (the "first vehicle" of RAPTOR
//! where vehicles do not overtake each other; where they do, the one that
//! gets there first). Nobody waits on purpose for a later vehicle.
//!
//! A frequency-based vehicle (GTFS `frequencies.txt`) is boarded at the
//! expected time: half a headway after reaching the stop plus the wait (or
//! after the start of the period, if later), when that is within the
//! period; with `exact_times = 1` at the first scheduled departure. There
//! are no random draws: a trip's times are its expected times, so a trip
//! is still a path and the search stays exact on those values.
//!
//! # Dominance
//!
//! Label `a` dominates label `b` at a stop when every way `b` can continue
//! can be followed from `a` at least as well on every criterion: `a` is
//! there no later (later rides are then boarded no later and reach every
//! stop no later), has ridden no more often (so it may ride as much more),
//! has walked no more (with the walking criterion), and its fare state is
//! at least as good for every continuation (`FareModel::dominates`); and
//! `b` is not a state off a vehicle when `a` is not (only those may walk to
//! another stop or leave for the destination). Dominated labels are dropped
//! (the weak form: of two equal labels the first stays), and a label is not
//! kept when a trip already found to the destination dominates any trip it
//! could become (target pruning, with lower bounds of each criterion).
//!
//! At the destination the trips are compared on the chosen criteria only;
//! of trips equal on those the one with the least (total time, transfers,
//! seconds on foot, fare) in that order is kept, counting only the values
//! the search tracks (walking with the walking criterion, fare with a fare
//! structure and the fare criterion or a fare limit). The result per
//! departure minute is the exact Pareto set of the trips above, one trip per
//! vector of values.

use super::bounds::{Bounds, INF};
use super::Criteria;
use crate::fares::incremental::{FareModel, FareState};
use crate::transit::layer::{FrequencyEntry, TransitLayer};
use crate::CancelToken;

/// No label.
pub(crate) const NONE: u32 = u32::MAX;

/// R5's minimum wait before boarding.
const BOARD_SLACK: i32 = 60;

/// What a search needs besides the access, egress and departure.
pub(crate) struct Config {
    pub(crate) max_rides: usize,
    /// Trip limit, seconds: no stop is reached at or after `departure +
    /// max_t`, and no trip arrives after it (as R5's RAPTOR and r5r's trip
    /// planner do).
    pub(crate) max_t: i32,
    /// `(int)(walkSpeed * 1000)`, mm/s, for the walks between stops.
    pub(crate) walk_mms: i32,
    /// Walks between stops shorter than this, millimetres.
    pub(crate) max_transfer_mm: i32,
    pub(crate) criteria: Criteria,
    /// Highest total fare, cents (`i32::MAX`: none).
    pub(crate) max_fare: i32,
}

/// How a pattern's running trips are boarded.
#[derive(Debug, Clone)]
pub(crate) struct PatternRun {
    /// Running scheduled trips (indices into `pattern.trips`), in order.
    pub(crate) sched: Vec<u32>,
    /// Running frequency-based trips.
    pub(crate) freq: Vec<u32>,
    /// The first vehicle that can be boarded reaches every later stop
    /// first: scheduled trips that never overtake each other (departures
    /// and arrivals in order at every stop), or a single frequency-based
    /// trip.
    fifo: bool,
}

/// The network side of a search, built once per call.
pub(crate) struct Network<'a> {
    pub(crate) layer: &'a TransitLayer,
    pub(crate) transfers: &'a [Vec<(u32, i32)>],
    pub(crate) fare: Option<FareModel<'a>>,
    pub(crate) runs: Vec<Option<PatternRun>>,
    pub(crate) cfg: Config,
    /// The graph of the lower bounds (`None`: no bounds).
    pub(crate) graph: Option<super::bounds::Graph>,
}

impl<'a> Network<'a> {
    /// `allowed[p]`: pattern `p` is of a mode asked for; `active[s]`:
    /// service `s` runs on the date. Errors on a frequency entry with a
    /// headway that is not positive (R5 fails such a network too).
    pub(crate) fn new(
        layer: &'a TransitLayer,
        transfers: &'a [Vec<(u32, i32)>],
        allowed: &[bool],
        active: &[bool],
        fare: Option<FareModel<'a>>,
        cfg: Config,
    ) -> Result<Self, String> {
        if let Some((trip, h)) = layer.non_positive_headway() {
            return Err(format!(
                "trip '{trip}' runs by frequency with a headway_secs of {h}, which cannot be \
                 boarded"
            ));
        }
        let runs = layer
            .patterns
            .iter()
            .enumerate()
            .map(|(pi, p)| {
                if !allowed[pi] {
                    return None;
                }
                let mut sched = Vec::new();
                let mut freq = Vec::new();
                for (ti, t) in p.trips.iter().enumerate() {
                    if !active[t.service as usize] || t.departures.len() != p.stops.len() {
                        continue;
                    }
                    if t.is_frequency() {
                        freq.push(ti as u32);
                    } else {
                        sched.push(ti as u32);
                    }
                }
                if sched.is_empty() && freq.is_empty() {
                    return None;
                }
                let fifo = if freq.is_empty() {
                    sched.windows(2).all(|w| {
                        let (a, b) = (&p.trips[w[0] as usize], &p.trips[w[1] as usize]);
                        a.departures.iter().zip(&b.departures).all(|(x, y)| x <= y)
                            && a.arrivals.iter().zip(&b.arrivals).all(|(x, y)| x <= y)
                    })
                } else {
                    sched.is_empty() && freq.len() == 1
                };
                Some(PatternRun { sched, freq, fifo })
            })
            .collect();
        let mut net = Network {
            layer,
            transfers,
            fare,
            runs,
            cfg,
            graph: None,
        };
        net.graph = super::bounds::Graph::new(&net);
        Ok(net)
    }

    fn track_fare(&self) -> bool {
        self.fare.is_some()
    }
}

/// When a vehicle of frequency entry `f` can be boarded at a stop whose
/// template departure is `tt`, by a traveller ready at `earliest`: the
/// expected departure (half a headway after `earliest` or the start of the
/// period), or with `exact_times` the first departure at or after
/// `earliest`; `None` past the period or for unreadable times.
pub(crate) fn frequency_board(f: &FrequencyEntry, tt: i32, earliest: i32) -> Option<i32> {
    let missing = crate::gtfs::jparse::INT_MISSING;
    if f.start == missing || f.end == missing || f.headway <= 0 {
        return None;
    }
    let (start, end, h, e) = (
        i64::from(f.start) + i64::from(tt),
        i64::from(f.end) + i64::from(tt),
        i64::from(f.headway),
        i64::from(earliest),
    );
    let b = if f.exact {
        let k = if e <= start {
            0
        } else {
            (e - start + h - 1) / h
        };
        start + k * h
    } else {
        e.max(start) + h / 2
    };
    (b <= end).then_some(b).and_then(|b| i32::try_from(b).ok())
}

/// How a label was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    /// The access leg, `secs` long.
    Access,
    /// A ride of `pattern` boarded at position `board_pos` at `board_time`
    /// and left at `alight_pos` (the label's stop and time).
    Ride {
        pattern: u32,
        board_pos: u32,
        alight_pos: u32,
        board_time: i32,
    },
    /// A walk from the stop of the label behind.
    Transfer,
}

/// A label: a partial trip at a stop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Label {
    pub(crate) time: i32,
    pub(crate) walk: i32,
    pub(crate) fare: FareState,
    pub(crate) rides: u32,
    pub(crate) stop: u32,
    pub(crate) back: u32,
    pub(crate) step: Step,
    alive: bool,
}

impl Label {
    fn is_ride(&self) -> bool {
        matches!(self.step, Step::Ride { .. })
    }
}

/// A trip to the destination found by a search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Arrival {
    /// Door to door, seconds from the departure minute.
    pub(crate) total: i32,
    /// Total fare, cents (0 without fares).
    pub(crate) fare: i32,
    pub(crate) rides: u32,
    /// Seconds on foot.
    pub(crate) walk: i32,
    /// The label at the last ride's stop, or `NONE` for the direct trip.
    pub(crate) label: u32,
    /// The egress stop's entry in the egress list.
    pub(crate) egress: u32,
}

impl Arrival {
    pub(crate) fn transfers(&self) -> u32 {
        self.rides.saturating_sub(1)
    }
}

/// A leg of an access or egress list: a stop, the seconds to or from it,
/// and the seconds of those on foot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StopLeg {
    pub(crate) stop: u32,
    pub(crate) secs: i32,
    pub(crate) walk: i32,
}

/// A riding traveller in a pattern scan.
#[derive(Debug, Clone, Copy)]
struct Riding {
    src: u32,
    /// Index into `pattern.trips`.
    trip: u32,
    /// For a frequency-based trip, its expected start: arrivals are `vs +`
    /// the template times. 0 for a scheduled trip.
    vs: i32,
    board_pos: u32,
    board_time: i32,
    fare: FareState,
    walk: i32,
    rides: u32,
}

/// Counters of one search, for measuring.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Stats {
    pub(crate) labels: u64,
    pub(crate) max_bag: u64,
    pub(crate) rounds: u64,
}

/// Reused buffers of one thread's searches.
#[derive(Default)]
pub(crate) struct Search {
    pub(crate) arena: Vec<Label>,
    bags: Vec<Vec<u32>>,
    touched: Vec<u32>,
    /// Per stop, the egress entry (index into the egress list), or `NONE`.
    egress_at: Vec<u32>,
    dest: Dest,
    /// Marked labels of the round, grouped by stop.
    marked_at: Vec<(u32, u32)>,
    marked_sorted: Vec<u32>,
    riding: Vec<Riding>,
    pub(crate) stats: Stats,
}

/// The trips to the destination kept so far, with their values.
#[derive(Default)]
struct Dest {
    trips: Vec<Arrival>,
    values: Vec<([i64; 4], [i64; 4])>,
}

/// What a label could at best become at the destination.
struct Bound {
    total: i64,
    transfers: i64,
    walk: i64,
    fare: i64,
}

impl Search {
    /// The Pareto set of trips leaving at `departure` (clock seconds):
    /// `access` and `egress` legs, and the direct trip `(seconds, seconds
    /// on foot)` if any. Arrivals in the order kept; their labels stay in
    /// [`Search::arena`] until the next call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run(
        &mut self,
        net: &Network,
        departure: i32,
        access: &[StopLeg],
        egress: &[StopLeg],
        direct: Option<(i32, i32)>,
        bounds: &Bounds,
        cancel: &CancelToken,
    ) -> Vec<Arrival> {
        let n_stops = net.layer.stop_ids.len();
        if self.bags.len() != n_stops {
            self.bags = vec![Vec::new(); n_stops];
            self.egress_at = vec![NONE; n_stops];
            self.marked_at = vec![(0, 0); n_stops];
            self.touched.clear();
        }
        for &s in &self.touched {
            self.bags[s as usize].clear();
        }
        self.touched.clear();
        self.arena.clear();
        self.dest.trips.clear();
        self.dest.values.clear();
        for e in self.egress_at.iter_mut() {
            *e = NONE;
        }
        for m in self.marked_at.iter_mut() {
            *m = (0, 0);
        }
        self.marked_sorted.clear();
        for (k, e) in egress.iter().enumerate() {
            let slot = &mut self.egress_at[e.stop as usize];
            // the faster leg of a stop listed twice
            if *slot == NONE || egress[*slot as usize].secs > e.secs {
                *slot = k as u32;
            }
        }
        let ctx = Ctx {
            net,
            departure,
            horizon: departure.saturating_add(net.cfg.max_t),
            bounds,
        };
        if let Some((secs, walk)) = direct {
            ctx.add_arrival(
                &mut self.dest,
                Arrival {
                    total: secs,
                    fare: 0,
                    rides: 0,
                    walk,
                    label: NONE,
                    egress: NONE,
                },
            );
        }
        if egress.is_empty() {
            return self.dest.trips.clone();
        }
        let mut marked: Vec<u32> = Vec::new();
        for a in access {
            let l = Label {
                time: departure.saturating_add(a.secs),
                walk: a.walk,
                fare: FareState::default(),
                rides: 0,
                stop: a.stop,
                back: NONE,
                step: Step::Access,
                alive: true,
            };
            if let Some(id) = self.insert(&ctx, l, egress) {
                marked.push(id);
            }
        }
        let max_rides = net.cfg.max_rides;
        for round in 1..=max_rides {
            if cancel.is_cancelled() || marked.is_empty() {
                break;
            }
            self.stats.rounds += 1;
            marked.retain(|&id| self.arena[id as usize].alive);
            self.mark(&marked);
            let mut patterns: Vec<u32> = Vec::new();
            for &id in &self.marked_sorted {
                let s = self.arena[id as usize].stop as usize;
                patterns.extend_from_slice(&net.layer.patterns_for_stop[s]);
            }
            patterns.sort_unstable();
            patterns.dedup();
            let first_new = self.arena.len();
            for p in patterns {
                if net.runs[p as usize].is_none() {
                    continue;
                }
                self.scan(&ctx, p, round as u32, egress);
            }
            let rides_new: Vec<u32> = (first_new..self.arena.len())
                .map(|i| i as u32)
                .filter(|&i| {
                    let l = &self.arena[i as usize];
                    l.alive && l.is_ride()
                })
                .collect();
            marked = rides_new.clone();
            if round < max_rides {
                for id in rides_new {
                    if !self.arena[id as usize].alive {
                        continue;
                    }
                    let from = self.arena[id as usize];
                    for &(to, mm) in &net.transfers[from.stop as usize] {
                        if mm >= net.cfg.max_transfer_mm {
                            continue;
                        }
                        let secs = mm / net.cfg.walk_mms;
                        let l = Label {
                            time: from.time.saturating_add(secs),
                            walk: from.walk.saturating_add(secs),
                            fare: from.fare,
                            rides: from.rides,
                            stop: to,
                            back: id,
                            step: Step::Transfer,
                            alive: true,
                        };
                        if let Some(t) = self.insert(&ctx, l, egress) {
                            marked.push(t);
                        }
                    }
                }
            }
        }
        self.stats.labels += self.arena.len() as u64;
        let mut out = self.dest.trips.clone();
        // the order of insertion is the order kept; sort for the caller
        out.sort_by_key(|a| (a.total, a.transfers(), a.walk, a.fare, a.label));
        out
    }

    /// Group the marked labels by stop (the previous round's index
    /// cleared).
    fn mark(&mut self, marked: &[u32]) {
        for &id in &self.marked_sorted {
            if let Some(l) = self.arena.get(id as usize) {
                self.marked_at[l.stop as usize] = (0, 0);
            }
        }
        let mut v: Vec<(u32, u32)> = marked
            .iter()
            .map(|&id| (self.arena[id as usize].stop, id))
            .collect();
        v.sort_unstable();
        self.marked_sorted.clear();
        let mut i = 0;
        while i < v.len() {
            let s = v[i].0;
            let start = self.marked_sorted.len() as u32;
            while i < v.len() && v[i].0 == s {
                self.marked_sorted.push(v[i].1);
                i += 1;
            }
            self.marked_at[s as usize] = (start, self.marked_sorted.len() as u32 - start);
        }
    }

    /// The range of `marked_sorted` holding the marked labels at `stop`
    /// (some may have been dropped since: check `alive`).
    fn marked_range(&self, stop: u32) -> std::ops::Range<usize> {
        let (a, n) = self.marked_at[stop as usize];
        a as usize..(a + n) as usize
    }

    /// Ride pattern `p` in round `round` from the marked labels at its stops.
    fn scan(&mut self, ctx: &Ctx, p: u32, round: u32, egress: &[StopLeg]) {
        let net = ctx.net;
        let pattern = &net.layer.patterns[p as usize];
        let run = net.runs[p as usize].as_ref().expect("running pattern");
        if !run.fifo {
            return self.scan_general(ctx, p, round, egress);
        }
        let mut riding = std::mem::take(&mut self.riding);
        riding.clear();
        for (pos, &stop) in pattern.stops.iter().enumerate() {
            if !riding.is_empty() && pattern.dropoffs[pos] {
                for &r in &riding {
                    let trip = &pattern.trips[r.trip as usize];
                    let time = if run.freq.is_empty() {
                        trip.arrivals[pos]
                    } else {
                        r.vs.saturating_add(trip.arrivals[pos])
                    };
                    let l = Label {
                        time,
                        walk: r.walk,
                        fare: r.fare,
                        rides: r.rides,
                        stop,
                        back: r.src,
                        step: Step::Ride {
                            pattern: p,
                            board_pos: r.board_pos,
                            alight_pos: pos as u32,
                            board_time: r.board_time,
                        },
                        alive: true,
                    };
                    self.insert(ctx, l, egress);
                }
            }
            if !pattern.pickups[pos] || pos + 1 == pattern.stops.len() {
                continue;
            }
            for m in self.marked_range(stop) {
                let id = self.marked_sorted[m];
                let from = self.arena[id as usize];
                if !from.alive || from.rides + 1 != round {
                    continue;
                }
                let earliest = from.time.saturating_add(BOARD_SLACK);
                let (trip, board_time, vs) = if run.freq.is_empty() {
                    // departures never decrease from trip to trip here
                    let k = run.sched.partition_point(|&t| {
                        pattern.trips[t as usize].departures[pos] <= earliest
                    });
                    let Some(&t) = run.sched.get(k) else { continue };
                    (t, pattern.trips[t as usize].departures[pos], 0)
                } else {
                    let t = run.freq[0];
                    let trip = &pattern.trips[t as usize];
                    let tt = trip.departures[pos];
                    let Some(b) = trip
                        .frequencies
                        .iter()
                        .filter_map(|f| frequency_board(f, tt, earliest))
                        .min()
                    else {
                        continue;
                    };
                    (t, b, b.saturating_sub(tt))
                };
                let fare = match &net.fare {
                    Some(m) => m.step(&from.fare, p as i32, board_time),
                    None => from.fare,
                };
                let r = Riding {
                    src: id,
                    trip,
                    vs,
                    board_pos: pos as u32,
                    board_time,
                    fare,
                    walk: from.walk,
                    rides: round,
                };
                // the label it alights as cannot be within the limits
                if ctx.out_of_bounds_fare(&fare, round) {
                    continue;
                }
                let now = if run.freq.is_empty() {
                    pattern.trips[trip as usize].arrivals[pos]
                } else {
                    vs.saturating_add(pattern.trips[trip as usize].arrivals[pos])
                };
                if riding.iter().any(|o| ctx.riding_dominates(o, &r, now)) {
                    continue;
                }
                riding.retain(|o| !ctx.riding_dominates(&r, o, now));
                riding.push(r);
            }
        }
        self.riding = riding;
    }

    /// Ride a pattern whose vehicles may overtake each other: each label
    /// boarding takes, for each later stop, the vehicle that gets there
    /// first.
    fn scan_general(&mut self, ctx: &Ctx, p: u32, round: u32, egress: &[StopLeg]) {
        let net = ctx.net;
        let pattern = &net.layer.patterns[p as usize];
        let run = net.runs[p as usize].as_ref().expect("running pattern");
        let n = pattern.stops.len();
        // (board time, trip, vs) of each vehicle that can be boarded
        let mut cands: Vec<(i32, u32, i32, bool)> = Vec::new();
        for pos in 0..n.saturating_sub(1) {
            if !pattern.pickups[pos] {
                continue;
            }
            let stop = pattern.stops[pos];
            for m in self.marked_range(stop) {
                let id = self.marked_sorted[m];
                let from = self.arena[id as usize];
                if !from.alive || from.rides + 1 != round {
                    continue;
                }
                let earliest = from.time.saturating_add(BOARD_SLACK);
                cands.clear();
                for &t in &run.sched {
                    let d = pattern.trips[t as usize].departures[pos];
                    if d > earliest {
                        cands.push((d, t, 0, false));
                    }
                }
                for &t in &run.freq {
                    let trip = &pattern.trips[t as usize];
                    let tt = trip.departures[pos];
                    for f in &trip.frequencies {
                        if let Some(b) = frequency_board(f, tt, earliest) {
                            cands.push((b, t, b.saturating_sub(tt), true));
                        }
                    }
                }
                if cands.is_empty() {
                    continue;
                }
                // ties on arrival go to the earlier departure, then to the
                // trip listed first
                cands.sort_unstable();
                for (j, &alight) in pattern.stops.iter().enumerate().skip(pos + 1) {
                    if !pattern.dropoffs[j] {
                        continue;
                    }
                    let mut best: Option<(i32, i32)> = None; // (arrival, board time)
                    for &(b, t, vs, freq) in &cands {
                        let trip = &pattern.trips[t as usize];
                        let a = if freq {
                            vs.saturating_add(trip.arrivals[j])
                        } else {
                            trip.arrivals[j]
                        };
                        if best.map_or(true, |(ba, _)| a < ba) {
                            best = Some((a, b));
                        }
                    }
                    let Some((arrival, board_time)) = best else {
                        continue;
                    };
                    let fare = match &net.fare {
                        Some(m) => m.step(&from.fare, p as i32, board_time),
                        None => from.fare,
                    };
                    let l = Label {
                        time: arrival,
                        walk: from.walk,
                        fare,
                        rides: round,
                        stop: alight,
                        back: id,
                        step: Step::Ride {
                            pattern: p,
                            board_pos: pos as u32,
                            alight_pos: j as u32,
                            board_time,
                        },
                        alive: true,
                    };
                    self.insert(ctx, l, egress);
                }
            }
        }
    }

    /// Put `l` in its stop's bag unless dominated or out of bounds; the
    /// labels it dominates there are dropped. A label off a vehicle at an
    /// egress stop also gives a trip to the destination.
    fn insert(&mut self, ctx: &Ctx, l: Label, egress: &[StopLeg]) -> Option<u32> {
        if l.time >= ctx.horizon || ctx.out_of_bounds_fare(&l.fare, l.rides) {
            return None;
        }
        if ctx.pruned(&self.dest, &l) {
            return None;
        }
        let s = l.stop as usize;
        let bag = &self.bags[s];
        for &id in bag {
            if ctx.dominates(&self.arena[id as usize], &l) {
                return None;
            }
        }
        let id = self.arena.len() as u32;
        if bag.is_empty() {
            self.touched.push(l.stop);
        }
        let arena = &mut self.arena;
        self.bags[s].retain(|&o| {
            if ctx.dominates(&l, &arena[o as usize]) {
                arena[o as usize].alive = false;
                false
            } else {
                true
            }
        });
        self.bags[s].push(id);
        self.stats.max_bag = self.stats.max_bag.max(self.bags[s].len() as u64);
        self.arena.push(l);
        if l.is_ride() {
            let e = self.egress_at[s];
            if e != NONE {
                let leg = egress[e as usize];
                let fare = ctx.net.fare.as_ref().map_or(0, |m| m.total(&l.fare));
                ctx.add_arrival(
                    &mut self.dest,
                    Arrival {
                        total: l
                            .time
                            .saturating_add(leg.secs)
                            .saturating_sub(ctx.departure),
                        fare,
                        rides: l.rides,
                        walk: l.walk.saturating_add(leg.walk),
                        label: id,
                        egress: e,
                    },
                );
            }
        }
        Some(id)
    }
}

/// One search's constants.
struct Ctx<'n, 'a> {
    net: &'n Network<'a>,
    departure: i32,
    /// `departure + max_t`.
    horizon: i32,
    bounds: &'n Bounds,
}

impl Ctx<'_, '_> {
    fn rides_left(&self, rides: u32) -> i64 {
        self.net.cfg.max_rides as i64 - i64::from(rides)
    }

    /// No trip continuing from fare state `f` after `rides` rides can be
    /// within `max_fare`.
    fn out_of_bounds_fare(&self, f: &FareState, rides: u32) -> bool {
        match &self.net.fare {
            Some(m) if self.net.cfg.max_fare < i32::MAX => {
                m.lower_bound(f, self.rides_left(rides)) > i64::from(self.net.cfg.max_fare)
            }
            _ => false,
        }
    }

    /// Label dominance (see the module documentation).
    fn dominates(&self, a: &Label, b: &Label) -> bool {
        if b.is_ride() && !a.is_ride() {
            return false;
        }
        if a.time > b.time || a.rides > b.rides {
            return false;
        }
        let c = &self.net.cfg.criteria;
        if c.walk && a.walk > b.walk {
            return false;
        }
        match &self.net.fare {
            None => true,
            Some(m) => m.dominates(
                &a.fare,
                &b.fare,
                a.time == b.time,
                b.time,
                self.horizon,
                self.rides_left(a.rides),
                self.rides_left(b.rides),
            ),
        }
    }

    /// Dominance between two travellers on the same pattern, at the stop
    /// they are reached at `now`: on the same vehicle (a frequency-based one
    /// started no later), as labels there.
    fn riding_dominates(&self, a: &Riding, b: &Riding, now: i32) -> bool {
        if a.trip != b.trip || a.vs > b.vs || a.rides > b.rides {
            return false;
        }
        let c = &self.net.cfg.criteria;
        if c.walk && a.walk > b.walk {
            return false;
        }
        match &self.net.fare {
            None => true,
            Some(m) => m.dominates(
                &a.fare,
                &b.fare,
                a.vs == b.vs,
                now,
                self.horizon,
                self.rides_left(a.rides),
                self.rides_left(b.rides),
            ),
        }
    }

    /// The values a trip is compared on at the destination (the chosen
    /// criteria; 0 for the others) and the values it is ranked by among
    /// trips equal on those (every value the search tracks).
    fn values(&self, a: &Arrival) -> ([i64; 4], [i64; 4]) {
        let c = &self.net.cfg.criteria;
        let fare_tracked = self.net.track_fare();
        let full = [
            i64::from(a.total),
            i64::from(a.transfers()),
            if c.walk { i64::from(a.walk) } else { 0 },
            if fare_tracked { i64::from(a.fare) } else { 0 },
        ];
        let chosen = [
            if c.time { full[0] } else { 0 },
            if c.transfers { full[1] } else { 0 },
            full[2],
            if c.fare { full[3] } else { 0 },
        ];
        (chosen, full)
    }

    /// `a` is kept over `b` at the destination.
    fn arrival_dominates(a: ([i64; 4], [i64; 4]), b: ([i64; 4], [i64; 4])) -> bool {
        let le = (0..4).all(|k| a.0[k] <= b.0[k]);
        le && (a.0 != b.0 || a.1 <= b.1)
    }

    fn add_arrival(&self, dest: &mut Dest, a: Arrival) {
        if a.total > self.net.cfg.max_t {
            return;
        }
        if self.net.track_fare() && a.fare > self.net.cfg.max_fare {
            return;
        }
        let va = self.values(&a);
        if dest.values.iter().any(|&o| Self::arrival_dominates(o, va)) {
            return;
        }
        let mut k = 0;
        while k < dest.trips.len() {
            if Self::arrival_dominates(va, dest.values[k]) {
                dest.trips.remove(k);
                dest.values.remove(k);
            } else {
                k += 1;
            }
        }
        dest.trips.push(a);
        dest.values.push(va);
    }

    /// Lower bounds of what label `l` can become at the destination.
    /// `None` when `l` cannot reach the destination within the limits.
    fn bound(&self, l: &Label) -> Option<Bound> {
        let b = if l.is_ride() {
            self.bounds.off[l.stop as usize]
        } else {
            self.bounds.on[l.stop as usize]
        };
        if b.contains(&INF) {
            return None;
        }
        let total = i64::from(l.time) + i64::from(b[0]) - i64::from(self.departure);
        let rides = i64::from(l.rides) + i64::from(b[1]);
        if total > i64::from(self.net.cfg.max_t) || rides > self.net.cfg.max_rides as i64 {
            return None;
        }
        let fare = match &self.net.fare {
            Some(m) => m.lower_bound(&l.fare, self.rides_left(l.rides)),
            None => 0,
        };
        Some(Bound {
            total,
            transfers: (rides - 1).max(0),
            walk: i64::from(l.walk) + i64::from(b[2]),
            fare,
        })
    }

    /// A trip found to the destination is better than anything `l` can
    /// become.
    fn pruned(&self, dest: &Dest, l: &Label) -> bool {
        let Some(b) = self.bound(l) else {
            return true;
        };
        if dest.trips.is_empty() {
            return false;
        }
        let c = &self.net.cfg.criteria;
        let fare_tracked = self.net.track_fare();
        let full = [
            b.total,
            b.transfers,
            if c.walk { b.walk } else { 0 },
            if fare_tracked { b.fare } else { 0 },
        ];
        let chosen = [
            if c.time { full[0] } else { 0 },
            if c.transfers { full[1] } else { 0 },
            full[2],
            if c.fare { full[3] } else { 0 },
        ];
        dest.values.iter().any(|&(dc, df)| {
            (0..4).all(|k| dc[k] <= chosen[k]) && (dc != chosen || (0..4).all(|k| df[k] <= full[k]))
        })
    }
}
