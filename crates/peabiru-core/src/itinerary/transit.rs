//! The public transport trip of the ported `TripPlanner` with `fastRaptor`
//! (`shortest_path = TRUE`, no fares): an access stop search, an egress
//! stop search backwards from the destination,
//! `FastRaptorWorker` over the departure window keeping the paths to the
//! egress stops (`searchFastRaptorOrigin`, `addFastRaptorTrip`), the trip
//! read from the best path (`Trip(Path, ...)`), and its legs rebuilt
//! (`Trip.augment`).

use super::geom::{edge_line, line_length_m, Line};
use super::hops::ride_line;
use super::links::{self, Links};
use super::mc::McFares;
use super::{street_path_line, via::ViaOrders, ItineraryParams, Leg, Trip};
use crate::street::link::{find_split, Split};
use crate::street::p2p::{Router, NONE};
use crate::street::{Mode, StreetNetwork};
use crate::transit::matrix::weekday;
use crate::transit::matrix::TransitData;
use crate::transit::raptor::{
    self, route_type_mode, Filtered, PathArena, RaptorRequest, UNREACHED,
};
use crate::{RouteError, TransitLayer};

/// What every pair of a call shares.
pub(crate) struct Planner<'a> {
    pub(super) net: &'a StreetNetwork,
    pub(super) layer: &'a TransitLayer,
    pub(super) data: &'a TransitData,
    pub(super) p: &'a ItineraryParams,
    filtered: Filtered,
    /// The stop at each street vertex, or -1.
    pub(super) vertex_stop: Vec<i32>,
    /// McRAPTOR's patterns (running on the date, of the modes asked for)
    /// and services running on the date.
    pub(super) mc_allowed: Vec<bool>,
    pub(super) mc_active: Vec<bool>,
    /// McRAPTOR's timetables of those patterns, made by the first search
    /// that needs them.
    pub(super) mc_schedules: std::sync::OnceLock<crate::transit::mcraptor::Schedules>,
    /// With a fare structure, the calculator and `request.maxFare`.
    pub(super) fares: Option<McFares<'a>>,
}

/// R5's `TransitLayer.TRANSFER_DISTANCE_LIMIT_METERS`.
const TRANSFER_DISTANCE_LIMIT_M: i32 = 1000;

impl<'a> Planner<'a> {
    pub(crate) fn new(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        p: &'a ItineraryParams,
        calc: Option<&'a crate::fares::FareCalculator>,
    ) -> Result<Self, RouteError> {
        Self::with_frequencies(net, layer, data, p, calc, false)
    }

    /// [`Planner::new`], or with `frequencies` a planner that accepts a
    /// network with frequency-based trips (for `trip_options()`, whose
    /// router handles them; the FastRaptor and McRAPTOR paths of the
    /// planner are not used then).
    pub(crate) fn with_frequencies(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        p: &'a ItineraryParams,
        calc: Option<&'a crate::fares::FareCalculator>,
        frequencies: bool,
    ) -> Result<Self, RouteError> {
        if layer.has_frequencies && !frequencies {
            // detailed_itineraries() refuses them before calling R5
            return Err(RouteError::Invalid(
                "None of the GTFS feeds used to create the transit network can contain a \
                 'frequencies' table. Try using gtfstools::frequencies_to_stop_times() to \
                 create a suitable feed."
                    .into(),
            ));
        }
        let active = layer.active_services(p.date, weekday(p.date));
        let filtered =
            Filtered::new(layer, &active, &p.transit_modes).map_err(RouteError::Invalid)?;
        let mut vertex_stop = vec![-1i32; net.n_vertices()];
        for (s, &v) in layer.stop_vertex.iter().enumerate() {
            if v >= 0 {
                vertex_stop[v as usize] = s as i32;
            }
        }
        let mc_allowed =
            crate::transit::mcraptor::allowed_patterns(layer, &active, &p.transit_modes)
                .map_err(RouteError::Invalid)?;
        let fares = calc.map(|calc| McFares {
            calc,
            max_fare: crate::transit::matrix::max_fare_cents(p.max_fare),
        });
        Ok(Planner {
            net,
            layer,
            data,
            p,
            filtered,
            vertex_stop,
            mc_allowed,
            mc_active: active,
            mc_schedules: std::sync::OnceLock::new(),
            fares,
        })
    }

    /// The fastest public transport trip from `o` to `d`, already
    /// augmented, or `None`.
    pub(crate) fn fastest_trip(
        &self,
        o: (f64, f64),
        d: (f64, f64),
        scratch: &mut Vec<u32>,
        via: &mut ViaOrders,
    ) -> Result<Option<Trip>, RouteError> {
        let (net, p, layer) = (self.net, self.p, self.layer);
        let max_t = p.max_trip_duration * 60;
        // egress: the stops reached backwards from the destination
        let mut egress = Router::new(net, p.egress_mode, p.req(), true);
        egress.stop_search(&self.vertex_stop);
        egress.time_limit = p.max_time_s(p.egress_mode);
        via.prepare(&mut egress);
        let Some(ds) = find_split(net, d.0, d.1, p.egress_mode, scratch) else {
            return Ok(None);
        };
        if !egress.set_egress_origin(ds) {
            return Ok(None);
        }
        egress.route();
        via.collect(&mut egress);
        if let Some(why) = egress.unsupported {
            return Err(RouteError::Invalid(format!("not supported: {why}")));
        }
        let mut egress_times: Vec<(u32, i32)> = egress
            .reached_stops()
            .iter()
            .filter(|(_, &t)| t < max_t)
            .map(|(&s, &t)| (s, t))
            .collect();
        egress_times.sort_unstable();
        if egress_times.is_empty() {
            return Ok(None);
        }
        // access
        let mut access = Router::new(net, p.mode, p.req(), false);
        access.stop_search(&self.vertex_stop);
        access.time_limit = p.max_time_s(p.mode);
        via.prepare(&mut access);
        let Some(os) = find_split(net, o.0, o.1, p.mode, scratch) else {
            return Ok(None);
        };
        access.set_origin_split(os);
        access.route();
        via.collect(&mut access);
        let mut access_times: Vec<(u32, i32)> = access
            .reached_stops()
            .iter()
            .filter(|(_, &t)| t < max_t)
            .map(|(&s, &t)| (s, t))
            .collect();
        access_times.sort_unstable();
        if access_times.is_empty() {
            return Ok(None);
        }
        // the test switch fails in addFastRaptorTrip, once an origin
        // search exists
        if p.fail_fast_raptor {
            return Err(fallback("fail_fast_raptor is set"));
        }
        // FastRaptorWorker over the window, paths kept to the egress stops
        let n_stops = layer.stop_ids.len();
        let mut retain = vec![false; n_stops];
        for &(s, _) in &egress_times {
            retain[s as usize] = true;
        }
        let req = RaptorRequest {
            from_time: p.departure_time,
            time_window: p.time_window,
            max_rides: p.max_rides,
            max_duration: max_t,
            walk_mms: (p.req().walk * 1000.0f32) as i32,
            max_walk_min: p.max_walk_time,
            monte_carlo_draws: p.time_window,
            seed: None,
            origin: o,
        };
        let mut times = Vec::new();
        let mut arena = PathArena::default();
        raptor::route_paths_into(
            layer,
            &self.data.transfers,
            &self.filtered,
            &access_times,
            &req,
            &retain,
            &p.cancel,
            &mut times,
            &mut arena,
        );
        p.cancel.check()?;
        let n_iter = times.len() / n_stops.max(1);
        // best departure per stop: least travel time, ties to the earliest
        let mut tt = vec![UNREACHED; n_stops];
        let mut dep = vec![0i32; n_stops];
        let mut path = vec![NONE; n_stops];
        for i in 0..n_iter {
            let departure = p.departure_time + (p.time_window - 1 - i as i32) * 60;
            for &(s, _) in &egress_times {
                let s = s as usize;
                let time = times[i * n_stops + s];
                let Some(k) = arena.index(i, s) else { continue };
                if time == UNREACHED {
                    continue;
                }
                if time < tt[s] || (time == tt[s] && departure < dep[s]) {
                    tt[s] = time;
                    dep[s] = departure;
                    path[s] = k;
                }
            }
        }
        let mut best: Option<(usize, i32)> = None;
        for &(s, e) in &egress_times {
            let s = s as usize;
            if path[s] == NONE {
                continue;
            }
            let total = tt[s] + e;
            let better = match best {
                None => true,
                Some((b, bt)) => total < bt || (total == bt && dep[s] < dep[b]),
            };
            if better {
                best = Some((s, total));
            }
        }
        let Some((stop, total)) = best else {
            return Ok(None);
        };
        let egress_s = egress_times
            .iter()
            .find(|&&(s, _)| s as usize == stop)
            .expect("an egress stop")
            .1;
        let access_of = |s: u32| {
            access_times
                .binary_search_by_key(&s, |&(k, _)| k)
                .ok()
                .map(|i| access_times[i].1)
        };
        // Trip(Path, departure, accessTimes, egressSeconds, expectedTotal)
        let legs = arena.legs(path[stop]);
        let departure = dep[stop];
        let Some(first_access) = legs.first().and_then(|l| access_of(l.board_stop)) else {
            return Err(fallback("its path does not start at an access stop"));
        };
        let mut ready = departure + first_access;
        let mut out: Vec<Leg> = Vec::new();
        let mut rides: Vec<(usize, u32, u32)> = Vec::new(); // (index in out, from stop, to stop) for walks
        for (k, l) in legs.iter().enumerate() {
            if k > 0 {
                let prev_alight = legs[k - 1].alight_stop;
                if prev_alight != l.board_stop {
                    rides.push((out.len(), prev_alight, l.board_stop));
                    out.push(self.stop_transfer_leg(prev_alight, l.board_stop, l.transfer_time));
                    ready += l.transfer_time;
                }
            }
            let wait = l.board_time - ready;
            if wait < 0 {
                return Err(fallback("its path boards before reaching the stop"));
            }
            let pattern = l.pattern as usize;
            let (from_pos, to_pos) = stop_positions(
                layer,
                pattern,
                l.board_stop,
                l.alight_stop,
                l.board_time,
                l.ride_time,
            )
            .ok_or_else(|| fallback("its ride stops are not in its pattern"))?;
            let pat = &layer.patterns[pattern];
            let mode = match route_type_mode(pat.route_type) {
                Some(Some(m)) => m.name().to_string(),
                _ => return Err(fallback("a route type without a transit mode")),
            };
            let line = ride_line(net, layer, pattern, from_pos, to_pos)
                .map_err(|why| RouteError::Invalid(format!("not supported: {why}")))?;
            let distance = line_length_m(&line);
            let mut links = Links::default();
            if p.osm_link_ids {
                links.board_stop = layer.stop_ids[l.board_stop as usize].clone();
                links.alight_stop = layer.stop_ids[l.alight_stop as usize].clone();
            }
            out.push(Leg {
                mode,
                duration: l.ride_time,
                wait,
                route: pat.route_id.clone(),
                line,
                distance,
                links,
            });
            ready = l.board_time + l.ride_time;
        }
        if ready - departure + egress_s != total {
            return Err(fallback("its path duration does not match its travel time"));
        }
        let n_legs = out.len();
        // Trip.augment: access and egress legs, walks between stops on the
        // streets
        for (i, from, to) in rides {
            self.route_transfer(&mut out[i], from, to, scratch);
        }
        let first_board = legs[0].board_stop;
        let v = layer.stop_vertex[first_board as usize];
        if let Some(st) = (v >= 0).then(|| access.state_at_vertex(v as u32)).flatten() {
            let line = street_path_line(&access, st);
            let distance = line_length_m(&line);
            let links = if p.osm_link_ids {
                links::compacted(net, &links::path_edges(&access, st))
            } else {
                Links::default()
            };
            out.insert(
                0,
                Leg {
                    mode: p.mode.r5_name().to_string(),
                    duration: access.states[st as usize].dur,
                    wait: 0,
                    route: String::new(),
                    line,
                    distance,
                    links,
                },
            );
        }
        let last_alight = legs[legs.len() - 1].alight_stop;
        let v = layer.stop_vertex[last_alight as usize];
        if let Some(st) = (v >= 0).then(|| egress.state_at_vertex(v as u32)).flatten() {
            match egress_line(&egress, st, &ds, d) {
                Some((line, edges)) => {
                    let distance = line_length_m(&line);
                    let links = if p.osm_link_ids {
                        links::lists(net, edges)
                    } else {
                        Links::default()
                    };
                    out.push(Leg {
                        mode: p.egress_mode.r5_name().to_string(),
                        duration: egress.states[st as usize].dur,
                        wait: 0,
                        route: String::new(),
                        line,
                        distance,
                        links,
                    });
                }
                // EgressPath.InvalidOriginException: the trip is skipped
                None => return Ok(None),
            }
        }
        let duration = out.iter().map(|l| l.duration + l.wait).sum();
        Ok(Some(Trip {
            direct: false,
            departure,
            sort_duration: total,
            sort_legs: n_legs,
            duration,
            legs: out,
            fares: None,
        }))
    }

    /// `Trip.newStopTransferLeg`: a walk between two stops as a straight
    /// line between their vertices (empty when either is unlinked or they
    /// are the same stop), `(int)` its planar length as its distance.
    pub(super) fn stop_transfer_leg(&self, from: u32, to: u32, duration: i32) -> Leg {
        let (net, layer) = (self.net, self.layer);
        let (a, b) = (
            layer.stop_vertex[from as usize],
            layer.stop_vertex[to as usize],
        );
        let line: Line = if from != to && a >= 0 && b >= 0 {
            let pt = |v: i32| {
                let v = v as usize;
                [f64::from(net.v_lon[v]) / 1e7, f64::from(net.v_lat[v]) / 1e7]
            };
            vec![pt(a), pt(b)]
        } else {
            Vec::new()
        };
        let planar = match line.as_slice() {
            [p, q] => ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt(),
            _ => 0.0,
        };
        Leg {
            mode: Mode::Walk.r5_name().to_string(),
            duration,
            wait: 0,
            route: String::new(),
            line,
            distance: planar as i32,
            links: Links::default(),
        }
    }

    /// `TripLeg.augmentTransitLeg` for a walk between stops: routed again
    /// on foot, at most 1 km, toward the second stop's coordinates; the
    /// path replaces the line and gives the distance, the duration stays
    /// RAPTOR's.
    pub(super) fn route_transfer(&self, leg: &mut Leg, from: u32, to: u32, scratch: &mut Vec<u32>) {
        let (net, layer, p) = (self.net, self.layer, self.p);
        let (a, b) = (
            layer.stop_vertex[from as usize],
            layer.stop_vertex[to as usize],
        );
        if a < 0 || b < 0 {
            return;
        }
        let mut r = Router::new(net, Mode::Walk, p.req(), false);
        r.distance_limit_m = TRANSFER_DISTANCE_LIMIT_M;
        r.set_origin_vertex(a as u32);
        let (lat, lon) = (
            f64::from(net.v_lat[b as usize]) / 1e7,
            f64::from(net.v_lon[b as usize]) / 1e7,
        );
        let dest = find_split(net, lat, lon, Mode::Walk, scratch);
        if let Some(ds) = dest {
            r.set_destination(ds);
        }
        r.route();
        let Some(ds) = dest else { return };
        let Some(last) = r.final_state(&ds) else {
            return;
        };
        leg.line = street_path_line(&r, last);
        leg.distance = line_length_m(&leg.line);
        if p.osm_link_ids {
            leg.links = links::compacted(net, &links::path_edges(&r, last));
        }
    }
}

/// The error for a pair routed with McRAPTOR after FastRaptor fails.
pub(super) fn fallback(why: &str) -> RouteError {
    RouteError::Invalid(format!("{FALLBACK}({why})"))
}

const FALLBACK: &str =
    "not supported: this trip is routed with the multi-criteria router because the fastest-trip \
     router's path fails ";

/// Whether `e` is [`fallback`]'s.
pub(super) fn is_fallback(e: &RouteError) -> bool {
    matches!(e, RouteError::Invalid(m) if m.starts_with(FALLBACK))
}

/// `Trip.stopPositions`: the positions in the pattern of a ride's board
/// and alight stops whose schedule matches the ride, else the shortest
/// span.
fn stop_positions(
    layer: &TransitLayer,
    pattern: usize,
    board: u32,
    alight: u32,
    board_time: i32,
    ride_time: i32,
) -> Option<(usize, usize)> {
    let pat = &layer.patterns[pattern];
    let mut best: Option<(usize, usize)> = None;
    for pp in 0..pat.stops.len() {
        if pat.stops[pp] != board {
            continue;
        }
        for q in pp + 1..pat.stops.len() {
            if pat.stops[q] != alight {
                continue;
            }
            for trip in &pat.trips {
                if trip.departures[pp] == board_time && trip.arrivals[q] == board_time + ride_time {
                    return Some((pp, q));
                }
            }
            if best.map_or(true, |(a, b)| q - pp < b - a) {
                best = Some((pp, q));
            }
        }
    }
    best
}

/// `EgressPath.build`: the edges from the stop's state back to the
/// seed at the destination's split, the part of the split edge up to the
/// split point, and the destination itself; consecutive equal points
/// once. With the line, the edges of its `streetEdges` (the partial edge
/// when it has a length). `None` when the path does not end at a seed
/// (R5's `InvalidOriginException`).
pub(super) fn egress_line(
    r: &Router,
    st: u32,
    split: &Split,
    d: (f64, f64),
) -> Option<(Line, Vec<u32>)> {
    let mut line: Line = Vec::new();
    let mut edges = Vec::new();
    let append = |line: &mut Line, pts: &[[f64; 2]]| {
        for &p in pts {
            if line.last() != Some(&p) {
                line.push(p);
            }
        }
    };
    let mut root = st;
    while r.states[root as usize].back != NONE {
        let e = r.states[root as usize].back_edge as usize;
        append(&mut line, &edge_line(r.net, e));
        edges.push(e as u32);
        root = r.states[root as usize].back;
    }
    let rs = &r.states[root as usize];
    let from0 = rs.back_edge == split.edge as i32 && rs.vertex == split.vertex0;
    let from1 = rs.back_edge == split.edge as i32 + 1 && rs.vertex == split.vertex1;
    if !from0 && !from1 {
        return None;
    }
    let full = edge_line(r.net, split.edge as usize);
    let seg = split.seg as usize;
    let mut partial: Line = if from0 {
        full[..=seg].to_vec()
    } else {
        full[seg + 1..].iter().rev().copied().collect()
    };
    partial.push([
        f64::from(split.fixed_lon) / 1e7,
        f64::from(split.fixed_lat) / 1e7,
    ]);
    if partial.len() == 1 {
        partial.push(partial[0]);
    }
    append(&mut line, &partial);
    let partial_mm = if from0 {
        split.distance0_mm
    } else {
        split.distance1_mm
    };
    if partial_mm > 0 {
        edges.push(rs.back_edge as u32);
    }
    append(&mut line, &[[d.1, d.0]]);
    if line.len() == 1 {
        line.push(line[0]);
    }
    Some((line, edges))
}
