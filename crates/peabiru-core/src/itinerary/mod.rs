//! Detailed itineraries, as r5r 2.5.0's `detailed_itineraries()`.
//!
//! r5r routes each origin-destination pair on its own with its
//! `TripPlanner` (`FastDetailedItineraryPlanner`): a direct trip by the
//! street mode (R5's `StreetRouter` toward the destination, A* and target
//! pruning included), and, with public transport, the fastest trip found by
//! R5's `FastRaptorWorker` over the departure window from the stops an
//! access search reaches to the stops an egress search (r5r's
//! `R5REgressRouter`) reaches, rebuilt leg by leg: the access and egress
//! street paths, the walks between stops routed again on the streets, and
//! the rides drawn along the GTFS shapes. With `shortest_path = FALSE` or
//! a fare structure r5r routes public transport with R5's McRAPTOR instead
//! (see `mc`), and so does it for a pair whose FastRaptor path cannot be
//! read back.

pub(crate) mod geom;
mod hops;
mod links;
mod mc;
mod options;
mod transit;
mod via;
mod wkt;

pub(crate) use options::trip_options_impl;

use crate::street::link::find_split;
use crate::street::p2p::{Req, Router};
use crate::street::{Mode, StreetNetwork};
use crate::transit::matrix::TransitData;
use crate::transit::raptor::TransitMode;
use crate::{CancelToken, FareStructure, RouteError, TransitLayer};
use geom::{clock, edge_line, line_length_m, minutes_1dp, without_repeats, Line};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Parameters of [`detailed_itineraries`](crate::detailed_itineraries), with r5r's defaults but for the
/// date and departure time. Fill it in by changing the fields of
/// [`ItineraryParams::default()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ItineraryParams {
    /// The street mode of the trip without public transport and of the
    /// access to it: r5r's street mode in `mode` (default [`Mode::Walk`]).
    pub mode: Mode,
    /// The public transport modes, r5r's transit modes in `mode`; empty
    /// (the default) for a street-only itinerary.
    pub transit_modes: Vec<TransitMode>,
    /// The street mode from public transport to the destination, r5r's
    /// `mode_egress` (default [`Mode::Walk`]).
    pub egress_mode: Mode,
    /// Service date `yyyymmdd` (needed with public transport).
    pub date: u32,
    /// Departure, seconds after midnight.
    pub departure_time: i32,
    /// Departure window in minutes (default 10).
    pub time_window: i32,
    /// r5r's `suboptimal_minutes` (default 0): with `shortest_path =
    /// false`, how many minutes slower than the best an alternative may be
    /// and still be kept; it must be 0 with `shortest_path` or a
    /// `fare_structure`.
    pub suboptimal_minutes: i32,
    /// Longest walk, bicycle and car leg in minutes (default 120 each):
    /// r5r's `max_walk_time` and friends, where `Inf` is
    /// `max_trip_duration`.
    pub max_walk_time: i32,
    #[allow(missing_docs)]
    pub max_bike_time: i32,
    #[allow(missing_docs)]
    pub max_car_time: i32,
    /// Trip limit in whole minutes (default 120). Without public transport
    /// r5r lowers it to the street mode's own limit.
    pub max_trip_duration: i32,
    /// Walking speed, km/h (default 3.6).
    pub walk_speed_kmh: f64,
    /// Cycling speed, km/h (default 12).
    pub bike_speed_kmh: f64,
    /// Most vehicles boarded (default 3).
    pub max_rides: usize,
    /// Highest level of traffic stress a cyclist rides on, 1 to 4
    /// (default 2).
    pub max_lts: i32,
    /// r5r's `shortest_path` (default true): the fastest itinerary only;
    /// `false` gives the alternatives McRAPTOR finds (one per sequence of
    /// routes).
    pub shortest_path: bool,
    /// r5r's `fare_structure` (default `None`): with public transport the
    /// trips are routed with McRAPTOR and fares (R5's
    /// `FareDominatingList`), and the itineraries get their fares.
    pub fare_structure: Option<Arc<FareStructure>>,
    /// r5r's `max_fare` (default infinity), read as a Java `float`; a
    /// finite value needs a `fare_structure`.
    pub max_fare: f64,
    /// Worker threads; 0 means all cores.
    pub n_threads: usize,
    /// Stops the computation when cancelled.
    pub cancel: CancelToken,
    /// r5r's `osm_link_ids` (default false): fill in each leg's
    /// [`osm_ids`](ItineraryLeg::osm_ids), [`edge_ids`](ItineraryLeg::edge_ids)
    /// and stop ids.
    pub osm_link_ids: bool,
    /// Car speeds and levels of traffic stress changed for this call
    /// (r5r's `new_carspeeds`, `carspeed_scale`, `new_lts`); the default
    /// changes nothing. r5r's trip planner routes every street leg (direct
    /// trip, access, egress, walks between stops) on the scenario's
    /// network, and so does this.
    pub scenario: crate::Scenario,
    /// For tests: fail every FastRaptor trip, as r5r does with the Java
    /// system property `r5r.failFastRaptor`, so that each pair with public
    /// transport that reaches stops is planned again with McRAPTOR.
    #[doc(hidden)]
    pub fail_fast_raptor: bool,
}

impl Default for ItineraryParams {
    fn default() -> Self {
        ItineraryParams {
            mode: Mode::Walk,
            transit_modes: Vec::new(),
            egress_mode: Mode::Walk,
            date: 0,
            departure_time: 0,
            time_window: 10,
            suboptimal_minutes: 0,
            max_walk_time: 120,
            max_bike_time: 120,
            max_car_time: 120,
            max_trip_duration: 120,
            walk_speed_kmh: 3.6,
            bike_speed_kmh: 12.0,
            max_rides: 3,
            max_lts: 2,
            shortest_path: true,
            fare_structure: None,
            max_fare: f64::INFINITY,
            n_threads: 0,
            cancel: CancelToken::default(),
            osm_link_ids: false,
            scenario: crate::Scenario::default(),
            fail_fast_raptor: false,
        }
    }
}

impl ItineraryParams {
    fn check(&self) -> Result<(), String> {
        let t = crate::TransitParams {
            date: if self.transit_modes.is_empty() {
                20190513
            } else {
                self.date
            },
            departure_time: self.departure_time,
            time_window: self.time_window,
            max_walk_time: self.max_walk_time,
            max_bike_time: self.max_bike_time,
            max_car_time: self.max_car_time,
            max_trip_duration: self.max_trip_duration,
            walk_speed_kmh: self.walk_speed_kmh,
            bike_speed_kmh: self.bike_speed_kmh,
            max_rides: self.max_rides,
            max_lts: self.max_lts,
            draws_per_minute: 1,
            transit_modes: if self.transit_modes.is_empty() {
                TransitMode::ALL.to_vec()
            } else {
                self.transit_modes.clone()
            },
            ..Default::default()
        };
        t.check()?;
        self.scenario.check()?;
        if self.suboptimal_minutes < 0 {
            return Err(format!(
                "suboptimal_minutes must be 0 or more, got {}",
                self.suboptimal_minutes
            ));
        }
        if self.fare_structure.is_some() && self.suboptimal_minutes > 0 {
            return Err(
                "suboptimal_minutes must be 0 when calculating fares with detailed_itineraries()"
                    .into(),
            );
        }
        if self.shortest_path && self.suboptimal_minutes > 0 {
            return Err("suboptimal_minutes must be 0 when shortest_path is true".into());
        }
        crate::transit::matrix::check_max_fare(self.max_fare, self.fare_structure.is_some())
    }

    fn req(&self) -> Req {
        Req::new(self.walk_speed_kmh, self.bike_speed_kmh, self.max_lts)
    }

    /// `getMaxTimeSeconds(mode)`.
    fn max_time_s(&self, mode: Mode) -> i32 {
        60 * match mode {
            Mode::Walk => self.max_walk_time,
            Mode::Bicycle => self.max_bike_time,
            Mode::Car => self.max_car_time,
        }
    }

    /// `max_trip_duration` as r5r hands it to R5 (`assign_max_trip_duration`):
    /// lowered to the street mode's limit when there is no public transport.
    fn trip_limit_min(&self) -> i32 {
        if self.transit_modes.is_empty() {
            self.max_trip_duration.min(self.max_time_s(self.mode) / 60)
        } else {
            self.max_trip_duration
        }
    }
}

/// One leg (r5r's `segment`) of an [`Itinerary`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ItineraryLeg {
    /// `WALK`, `BICYCLE`, `CAR`, or the public transport mode of a ride
    /// (`BUS`, `RAIL`, `SUBWAY`, `TRAM`, ...), as r5r writes it.
    pub mode: String,
    /// Seconds on the leg, and r5r's `segment_duration` (minutes, one
    /// decimal place).
    pub duration_s: i32,
    #[allow(missing_docs)]
    pub segment_duration: f64,
    /// Seconds waiting before a ride, and r5r's `wait` (minutes, one
    /// decimal place); 0 for street legs.
    pub wait_s: i32,
    #[allow(missing_docs)]
    pub wait: f64,
    /// Metres, along [`geometry`](Self::geometry) (r5r's `distance`).
    pub distance: i32,
    /// GTFS `route_id` of a ride, empty for a street leg.
    pub route: String,
    /// With a fare structure, r5r's `cumulative_fare`: the fare paid up to
    /// the end of the leg (0 before the first ride); `None` without one.
    pub cumulative_fare: Option<f64>,
    /// The leg's line as `(lon, lat)` degrees, as r5r's `geometry`
    /// column holds it (the values read back from the WKT text JTS writes,
    /// see [`wkt`](Self::wkt)).
    pub geometry: Vec<[f64; 2]>,
    /// The line before JTS's formatting.
    raw: Vec<[f64; 2]>,
    /// With [`ItineraryParams::osm_link_ids`]: the OpenStreetMap way ids
    /// and the street edge indices of a street leg, in order and once
    /// each, as r5r lists them (`osm_id_list`, `edge_id_list`; see
    /// [`osm_id_list`](Self::osm_id_list)); empty for a ride and otherwise.
    pub osm_ids: Vec<i64>,
    #[allow(missing_docs)]
    pub edge_ids: Vec<u32>,
    /// With [`ItineraryParams::osm_link_ids`]: the stops a ride is boarded
    /// and left at, `feed_id:stop_id` (r5r's `board_stop_id`,
    /// `alight_stop_id`); empty for street legs and otherwise.
    pub board_stop_id: String,
    #[allow(missing_docs)]
    pub alight_stop_id: String,
}

impl ItineraryLeg {
    /// The leg's line as the WKT text r5r writes for it (JTS 1.18.1's
    /// `LineString.toString()` on JDK 21): `LINESTRING (x y, ...)` with at
    /// most 16 fraction digits per ordinate, or `LINESTRING EMPTY`.
    pub fn wkt(&self) -> String {
        let mut s = String::new();
        wkt::line_wkt(&self.raw, &mut s);
        s
    }

    /// r5r's `osm_id_list`: [`osm_ids`](Self::osm_ids) as Java prints a
    /// set, `[id1, id2, ...]`, or `[]`.
    pub fn osm_id_list(&self) -> String {
        links::list_text(&self.osm_ids)
    }

    /// r5r's `edge_id_list`, as [`osm_id_list`](Self::osm_id_list).
    pub fn edge_id_list(&self) -> String {
        links::list_text(&self.edge_ids)
    }
}

/// One itinerary between an origin-destination pair: r5r's rows of one
/// `option`, one per leg.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Itinerary {
    /// The pair (index into the origins and destinations given).
    pub pair: usize,
    /// r5r's `option`, from 1.
    pub option: u32,
    /// Departure, seconds after midnight, and as r5r writes it (`HH:MM:SS`).
    pub departure_s: i32,
    #[allow(missing_docs)]
    pub departure_time: String,
    /// Door-to-door seconds, and r5r's `total_duration` (minutes, one
    /// decimal place).
    pub total_duration_s: i32,
    #[allow(missing_docs)]
    pub total_duration: f64,
    /// Metres, the sum of the legs' distances (r5r's `total_distance`).
    pub total_distance: i32,
    /// With a fare structure, r5r's `total_fare`; `None` without one.
    pub total_fare: Option<f64>,
    /// The legs in order (r5r's `segment` 1, 2, ...).
    pub legs: Vec<ItineraryLeg>,
}

/// A leg before r5r turns it into output columns.
#[derive(Debug, Clone)]
struct Leg {
    mode: String,
    duration: i32,
    wait: i32,
    route: String,
    /// The line as built, repeated points included (lengths are measured
    /// on it).
    line: Line,
    /// Metres, as the leg reports it.
    distance: i32,
    /// `osm_link_ids`.
    links: links::Links,
}

impl Leg {
    fn finish(self, fare: Option<i32>) -> ItineraryLeg {
        let raw = without_repeats(&self.line);
        let geometry = raw
            .iter()
            .map(|&[x, y]| [wkt::ordinate_value(x), wkt::ordinate_value(y)])
            .collect();
        ItineraryLeg {
            mode: self.mode,
            duration_s: self.duration,
            segment_duration: minutes_1dp(self.duration),
            wait_s: self.wait,
            wait: minutes_1dp(self.wait),
            distance: self.distance,
            route: self.route,
            cumulative_fare: fare.map(|f| f64::from(f) / 100.0),
            geometry,
            raw,
            osm_ids: self.links.osm,
            edge_ids: self.links.edges,
            board_stop_id: self.links.board_stop,
            alight_stop_id: self.links.alight_stop,
        }
    }
}

/// A trip of r5r's `TripPlanner` (`Trip`).
#[derive(Debug, Clone)]
struct Trip {
    direct: bool,
    departure: i32,
    /// Seconds and legs as `TripPlanner.plan()` filters and sorts by, before
    /// `Trip.augment` adds the access and egress legs and sums the legs.
    sort_duration: i32,
    sort_legs: usize,
    /// Seconds after `augment` (r5r's `total_duration`).
    duration: i32,
    legs: Vec<Leg>,
    /// With a fare structure: the total fare and each leg's cumulative
    /// fare, cents.
    fares: Option<(i32, Vec<i32>)>,
}

/// The line of a street path: the full geometry of every edge it
/// traversed, the first edge's start included (`StreetSegment`), and the
/// path's seconds and millimetres.
fn street_path_line(router: &Router, last: u32) -> Line {
    let mut line: Line = Vec::new();
    for id in router.path(last) {
        let e = router.states[id as usize].back_edge;
        if e < 0 {
            continue;
        }
        let g = edge_line(router.net, e as usize);
        if line.is_empty() {
            line.extend_from_slice(&g);
        } else {
            line.extend_from_slice(&g[1..]);
        }
    }
    line
}

/// `TripPlanner.findDirectPaths` for one mode: the direct trip, or `None`
/// when a point cannot be linked or the destination is not reached.
fn direct_trip(
    net: &StreetNetwork,
    p: &ItineraryParams,
    o: (f64, f64),
    d: (f64, f64),
    scratch: &mut Vec<u32>,
    via: &mut via::ViaOrders,
) -> Option<Trip> {
    let mode = p.mode;
    let mut limit = p.trip_limit_min() * 60;
    if !p.transit_modes.is_empty() {
        limit = limit.min(p.max_time_s(mode));
    }
    let mut router = Router::new(net, mode, p.req(), false);
    router.time_limit = limit;
    via.prepare(&mut router);
    let os = find_split(net, o.0, o.1, mode, scratch)?;
    router.set_origin_split(os);
    let ds = find_split(net, d.0, d.1, mode, scratch)?;
    router.set_destination(ds);
    router.route();
    let last = router.final_state(&ds);
    via.collect(&mut router);
    let last = last?;
    let line = street_path_line(&router, last);
    let duration = router.states[last as usize].dur;
    let distance = line_length_m(&line);
    let links = if p.osm_link_ids {
        links::compacted(net, &links::path_edges(&router, last))
    } else {
        links::Links::default()
    };
    Some(Trip {
        direct: true,
        departure: p.departure_time,
        sort_duration: duration,
        sort_legs: 1,
        duration,
        legs: vec![Leg {
            mode: mode.r5_name().to_string(),
            duration,
            wait: 0,
            route: String::new(),
            line,
            distance,
            links,
        }],
        fares: None,
    })
}

/// What the packages warn about after [`detailed_itineraries_noted`](crate::detailed_itineraries_noted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ItineraryNotes {
    /// The pairs (indices, ascending) whose itineraries in r5r depend on the
    /// searches that ran before them in the session: with car egress, R5
    /// reverses the via edges of turn restrictions with two or more via
    /// ways in place on every check of a backward car search. For every
    /// pair peabiru returns the itineraries r5r gives on a freshly built
    /// network for that pair alone with one thread.
    pub history_dependent: Vec<usize>,
    /// The pairs (indices, ascending) whose fastest public transport trip
    /// FastRaptor found but whose path r5r cannot read back
    /// (`Trip(Path, ...)` fails), so that r5r plans them again with
    /// McRAPTOR (`FastDetailedItineraryPlanner`), and so does peabiru. Not
    /// a warning: r5r only logs it.
    pub mcraptor_fallbacks: Vec<usize>,
}

impl ItineraryNotes {
    /// The warnings for a call of `n_pairs` pairs (at most one).
    pub fn warnings(&self, n_pairs: usize) -> Vec<String> {
        if self.history_dependent.is_empty() {
            return Vec::new();
        }
        vec![history_warning(&self.history_dependent, n_pairs)]
    }
}

/// The work of [`crate::detailed_itineraries_noted`].
pub(crate) fn detailed_itineraries_noted(
    net: &StreetNetwork,
    transit: Option<(&TransitLayer, &TransitData)>,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &ItineraryParams,
) -> Result<(Vec<Vec<Itinerary>>, ItineraryNotes), RouteError> {
    p.check().map_err(RouteError::Invalid)?;
    if origins.len() != destinations.len() {
        return Err(RouteError::Invalid(format!(
            "origins and destinations must pair up: {} origins, {} destinations",
            origins.len(),
            destinations.len()
        )));
    }
    // r5r reads the fare structure in as soon as it is given (for the
    // network's routes, whatever the modes)
    let calc = match (&p.fare_structure, transit) {
        (Some(fs), Some((layer, _))) => Some(crate::fares::FareCalculator::new(fs, layer)?),
        (Some(fs), None) => {
            fs.validate()?;
            None
        }
        _ => None,
    };
    // the scenario's network for every street leg (r5r's TripPlanner routes
    // on getTransportNetwork(), the scenario copy)
    let by_car = p.mode == Mode::Car || (!p.transit_modes.is_empty() && p.egress_mode == Mode::Car);
    let scen = crate::street::scenario::scenario_network(net, &p.scenario, by_car)?;
    let net = scen.as_ref().unwrap_or(net);
    let planner = if p.transit_modes.is_empty() {
        None
    } else {
        let Some((layer, data)) = transit else {
            return Err(RouteError::Invalid(
                "public transport itineraries need the network's transit layer".into(),
            ));
        };
        Some(transit::Planner::new(net, layer, data, p, calc.as_ref())?)
    };
    let n = origins.len();
    let next = AtomicUsize::new(0);
    let threads = crate::street::router::worker_threads(p.n_threads, n);
    let cancel = &p.cancel;
    // a car egress search runs backwards, which in R5 reverses the via
    // edges of turn restrictions in place (see `via`)
    let free = !p.transit_modes.is_empty() && p.egress_mode == Mode::Car;
    type Chunk = Result<Vec<(usize, (Vec<Itinerary>, bool), bool)>, RouteError>;
    let chunks: Vec<Chunk> = crate::sys::scoped(0..threads, |_| {
        let mut out = Vec::new();
        let mut scratch = Vec::new();
        while !cancel.is_cancelled() {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= n {
                break;
            }
            let (its, depends) = via::settle(free, |via| {
                let trips = plan(
                    net,
                    planner.as_ref(),
                    p,
                    origins[i],
                    destinations[i],
                    &mut scratch,
                    via,
                )?;
                Ok((finish(i, trips), via.mcraptor_fallback))
            })?;
            out.push((i, its, depends));
        }
        Ok(out)
    });
    cancel.check()?;
    let mut slots: Vec<Option<Vec<Itinerary>>> = (0..n).map(|_| None).collect();
    let mut dependent = Vec::new();
    let mut fallbacks = Vec::new();
    for chunk in chunks {
        for (i, (v, fell_back), depends) in chunk? {
            slots[i] = Some(v);
            if depends {
                dependent.push(i);
            }
            if fell_back {
                fallbacks.push(i);
            }
        }
    }
    dependent.sort_unstable();
    fallbacks.sort_unstable();
    let its = slots
        .into_iter()
        .map(|s| s.expect("every pair is planned"))
        .collect();
    let notes = ItineraryNotes {
        history_dependent: dependent,
        mcraptor_fallbacks: fallbacks,
    };
    Ok((its, notes))
}

/// The warning for the pairs (0-based, sorted) whose itineraries in r5r
/// depend on what ran before them.
fn history_warning(pairs: &[usize], n: usize) -> String {
    const SHOWN: usize = 10;
    let mut list: Vec<String> = pairs
        .iter()
        .take(SHOWN)
        .map(|i| (i + 1).to_string())
        .collect();
    if pairs.len() > SHOWN {
        list.push("...".into());
    }
    format!(
        "{} of {} pairs ({} {}) have car searches that cross a turn restriction with two or \
         more via ways, whose via edges R5 reverses in place on every check of a backward car \
         search: r5r's itineraries for them may depend on the searches that ran before in the \
         session (other pairs, earlier calls, threads). peabiru returns those r5r gives on a \
         freshly built network for the pair alone with one thread.",
        pairs.len(),
        n,
        if pairs.len() == 1 { "pair" } else { "pairs" },
        list.join(", ")
    )
}

/// `TripPlanner.plan()` for one pair.
fn plan(
    net: &StreetNetwork,
    planner: Option<&transit::Planner>,
    p: &ItineraryParams,
    o: (f64, f64),
    d: (f64, f64),
    scratch: &mut Vec<u32>,
    via: &mut via::ViaOrders,
) -> Result<Vec<Trip>, RouteError> {
    // r5r hands R5 NaN for a missing coordinate: no street is found
    if ![o.0, o.1, d.0, d.1].iter().all(|v| v.is_finite()) {
        return Ok(Vec::new());
    }
    let mut trips = Vec::new();
    let direct = direct_trip(net, p, o, d, scratch, via);
    if let Some(planner) = planner {
        if !p.shortest_path || planner.fares.is_some() {
            return planner.mc_plan(o, d, direct, planner.fares.as_ref(), scratch, via);
        }
        match planner.fastest_trip(o, d, scratch, via) {
            Ok(t) => {
                trips.extend(direct);
                trips.extend(t);
            }
            // r5r plans the pair again with McRAPTOR, from the direct trip
            // on (which the egress search may have changed, see `via`)
            Err(e) if transit::is_fallback(&e) => {
                via.mcraptor_fallback = true;
                let direct = direct_trip(net, p, o, d, scratch, via);
                return planner.mc_plan(o, d, direct, None, scratch, via);
            }
            Err(e) => return Err(e),
        }
    } else {
        trips.extend(direct);
    }
    let limit = p.trip_limit_min() * 60;
    trips.retain(|t| t.sort_duration <= limit);
    // sorted direct first, then by legs and duration (stable); with
    // shortest_path only the first of the fastest is kept
    trips.sort_by_key(|t| (i32::from(!t.direct), t.sort_legs, t.sort_duration));
    if p.shortest_path {
        if let Some(best) = trips.iter().map(|t| t.sort_duration).min() {
            let k = trips
                .iter()
                .position(|t| t.sort_duration == best)
                .expect("min");
            let t = trips.swap_remove(k);
            trips = vec![t];
        }
    }
    // with a fare structure r5r reports the fare columns, 0 for a trip
    // without public transport
    if p.fare_structure.is_some() {
        for t in &mut trips {
            t.fares.get_or_insert_with(|| (0, vec![0; t.legs.len()]));
        }
    }
    Ok(trips)
}

/// The output of a pair's trips.
fn finish(pair: usize, trips: Vec<Trip>) -> Vec<Itinerary> {
    trips
        .into_iter()
        .enumerate()
        .map(|(k, t)| {
            let total_distance = t.legs.iter().map(|l| l.distance).sum();
            let (total_fare, leg_fares) = match t.fares {
                Some((total, legs)) => (Some(f64::from(total) / 100.0), Some(legs)),
                None => (None, None),
            };
            Itinerary {
                pair,
                option: k as u32 + 1,
                departure_s: t.departure,
                departure_time: clock(t.departure),
                total_duration_s: t.duration,
                total_duration: minutes_1dp(t.duration),
                total_distance,
                total_fare,
                legs: t
                    .legs
                    .into_iter()
                    .enumerate()
                    .map(|(k, l)| l.finish(leg_fares.as_ref().map(|f| f[k])))
                    .collect(),
            }
        })
        .collect()
}

/// Write itineraries as r5r's `detailed_itineraries()` does with
/// `output_dir`: one file per pair with itineraries,
/// `<dir>/from_<origin id>_to_<destination id>.csv`, with r5r's columns
/// and number formats (`RDataFrame.saveToCsv`): with `osm_link_ids` (the
/// itineraries routed with it) `osm_id_list`, `edge_id_list`,
/// `board_stop_id` and `alight_stop_id` after `route`, and the `geometry`
/// column (the WKT text of [`ItineraryLeg::wkt`]) last unless
/// `drop_geometry`. `ids`
/// and `points` are those of the pairs given to [`detailed_itineraries`](crate::detailed_itineraries)
/// (`(from_id, to_id)` and `((lat, lon), (lat, lon))`).
#[allow(clippy::type_complexity)]
pub fn write_itineraries_csv(
    dir: &str,
    ids: &[(String, String)],
    points: &[((f64, f64), (f64, f64))],
    itineraries: &[Vec<Itinerary>],
    drop_geometry: bool,
    osm_link_ids: bool,
) -> std::io::Result<()> {
    use crate::output::{csv_field, java_double, LINE_END};
    for (k, its) in itineraries.iter().enumerate() {
        if its.is_empty() {
            continue;
        }
        let (from, to) = (&ids[k].0, &ids[k].1);
        let ((olat, olon), (dlat, dlon)) = points[k];
        let fares = its[0].total_fare.is_some();
        let mut s = String::from(if fares {
            "from_id,from_lat,from_lon,to_id,to_lat,to_lon,option,departure_time,\
             total_duration,total_distance,total_fare,segment,mode,cumulative_fare,\
             segment_duration,wait,distance,route"
        } else {
            "from_id,from_lat,from_lon,to_id,to_lat,to_lon,option,departure_time,\
             total_duration,total_distance,segment,mode,segment_duration,wait,distance,route"
        });
        if osm_link_ids {
            s.push_str(",osm_id_list,edge_id_list,board_stop_id,alight_stop_id");
        }
        if !drop_geometry {
            s.push_str(",geometry");
        }
        s.push_str(LINE_END);
        for it in its {
            for (j, l) in it.legs.iter().enumerate() {
                csv_field(from, &mut s);
                for v in [olat, olon] {
                    s.push(',');
                    java_double(v, &mut s);
                }
                s.push(',');
                csv_field(to, &mut s);
                for v in [dlat, dlon] {
                    s.push(',');
                    java_double(v, &mut s);
                }
                s.push_str(&format!(",{},", it.option));
                csv_field(&it.departure_time, &mut s);
                s.push(',');
                java_double(it.total_duration, &mut s);
                s.push_str(&format!(",{},", it.total_distance));
                if let Some(f) = it.total_fare {
                    java_double(f, &mut s);
                    s.push(',');
                }
                s.push_str(&format!("{},", j + 1));
                csv_field(&l.mode, &mut s);
                if let Some(f) = l.cumulative_fare {
                    s.push(',');
                    java_double(f, &mut s);
                }
                for v in [l.segment_duration, l.wait] {
                    s.push(',');
                    java_double(v, &mut s);
                }
                s.push_str(&format!(",{},", l.distance));
                csv_field(&l.route, &mut s);
                if osm_link_ids {
                    for v in [
                        l.osm_id_list(),
                        l.edge_id_list(),
                        l.board_stop_id.clone(),
                        l.alight_stop_id.clone(),
                    ] {
                        s.push(',');
                        csv_field(&v, &mut s);
                    }
                }
                if !drop_geometry {
                    s.push(',');
                    csv_field(&l.wkt(), &mut s);
                }
                s.push_str(LINE_END);
            }
        }
        std::fs::write(format!("{dir}/from_{from}_to_{to}.csv"), s)?;
    }
    Ok(())
}
