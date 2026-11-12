//! The pairs of [`trip_options`](crate::trip_options): the street searches
//! of r5r's trip planner (direct trip, access and egress stop searches,
//! reused from `transit` and `mc`), peabiru's multi-criteria search
//! (`crate::options::search`) at every departure minute, the merge of the
//! minutes into options, and their legs built as `detailed_itineraries()`
//! builds them (`Planner::augment`).

use super::geom::minutes_1dp;
use super::mc::{RawLeg, RawTrip};
use super::transit::{egress_line, Planner};
use super::via::ViaOrders;
use super::{direct_trip, ItineraryParams, Trip};
use crate::fares::incremental::FareModel;
use crate::fares::FareCalculator;
use crate::options::search::{self, Arrival, Search, Step, StopLeg, NONE};
use crate::options::{Criteria, TripOption, TripOptionsParams};
use crate::street::link::{find_split, Split};
use crate::street::p2p::Router;
use crate::street::{Mode, StreetNetwork};
use crate::transit::raptor::route_type_mode;
use crate::{RouteError, TransitData, TransitLayer};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// One leg between stops of a trip found by the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathLeg {
    Ride {
        pattern: u32,
        board_pos: u32,
        alight_pos: u32,
        board_time: i32,
        alight_time: i32,
        /// Seconds between reaching the stop and boarding.
        wait: i32,
    },
    Walk {
        from: u32,
        to: u32,
        secs: i32,
    },
}

/// A trip found by the search, read off its labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Path {
    pub(crate) access_stop: u32,
    pub(crate) access_secs: i32,
    pub(crate) legs: Vec<PathLeg>,
    pub(crate) egress_stop: u32,
    pub(crate) egress_secs: i32,
}

/// Read the trip behind `a` from the search's labels.
pub(crate) fn path_of(s: &Search, a: &Arrival, egress: &[StopLeg], departure: i32) -> Path {
    let mut legs = Vec::new();
    let mut k = a.label;
    let mut first = a.label;
    while k != NONE {
        let l = s.arena[k as usize];
        match l.step {
            Step::Access => first = k,
            Step::Ride {
                pattern,
                board_pos,
                alight_pos,
                board_time,
            } => {
                let before = s.arena[l.back as usize].time;
                legs.push(PathLeg::Ride {
                    pattern,
                    board_pos,
                    alight_pos,
                    board_time,
                    alight_time: l.time,
                    wait: board_time - before,
                });
            }
            Step::Transfer => {
                let b = s.arena[l.back as usize];
                legs.push(PathLeg::Walk {
                    from: b.stop,
                    to: l.stop,
                    secs: l.time - b.time,
                });
            }
        }
        k = l.back;
    }
    legs.reverse();
    let f = s.arena[first as usize];
    let e = egress[a.egress as usize];
    Path {
        access_stop: f.stop,
        access_secs: f.time - departure,
        legs,
        egress_stop: e.stop,
        egress_secs: e.secs,
    }
}

impl Path {
    /// `(pattern, board time)` of the rides.
    pub(crate) fn rides(&self) -> Vec<(i32, i32)> {
        self.legs
            .iter()
            .filter_map(|l| match l {
                PathLeg::Ride {
                    pattern,
                    board_time,
                    ..
                } => Some((*pattern as i32, *board_time)),
                PathLeg::Walk { .. } => None,
            })
            .collect()
    }
}

/// What makes two trips of different minutes the same option, besides
/// their values: the street modes, and the route and stops of each ride.
fn signature(layer: Option<&TransitLayer>, p: &TripOptionsParams, path: Option<&Path>) -> Vec<u32> {
    let code = |m: Mode| m as u32;
    match path {
        None => vec![0, code(p.mode)],
        Some(path) => {
            let layer = layer.expect("transit");
            let mut v = vec![1, code(p.mode), code(p.egress_mode)];
            for l in &path.legs {
                if let PathLeg::Ride {
                    pattern,
                    board_pos,
                    alight_pos,
                    ..
                } = *l
                {
                    let pat = &layer.patterns[pattern as usize];
                    v.extend([
                        layer.pattern_route[pattern as usize],
                        pat.stops[board_pos as usize],
                        pat.stops[alight_pos as usize],
                    ]);
                }
            }
            v
        }
    }
}

/// `WALK > BUS 343 > WALK`.
fn description(layer: Option<&TransitLayer>, p: &TripOptionsParams, path: Option<&Path>) -> String {
    let Some(path) = path else {
        return p.mode.r5_name().to_string();
    };
    let layer = layer.expect("transit");
    let mut parts = vec![p.mode.r5_name().to_string()];
    for l in &path.legs {
        if let PathLeg::Ride { pattern, .. } = *l {
            let pat = &layer.patterns[pattern as usize];
            let mode = match route_type_mode(pat.route_type) {
                Some(Some(m)) => m.name(),
                _ => "AIR",
            };
            let r = &layer.routes[layer.pattern_route[pattern as usize] as usize];
            let name = r
                .short_name
                .as_deref()
                .filter(|s| !s.is_empty())
                .unwrap_or(&r.route_id);
            parts.push(format!("{mode} {name}"));
        }
    }
    parts.push(p.egress_mode.r5_name().to_string());
    parts.join(" > ")
}

/// The values of an option: total seconds, fare cents (-1 without a fare
/// structure), transfers, seconds on foot.
type Values = (i32, i32, u32, i32);

/// An option being merged over the minutes.
struct Merged {
    /// At the representative minute.
    values: Values,
    /// The representative minute: the least total time, the earliest of
    /// equal ones.
    departure: i32,
    departures: Vec<i32>,
    totals: Vec<i32>,
    /// The trip at the representative minute (`None`: the direct trip).
    path: Option<Path>,
    description: String,
}

/// What every pair of a call shares.
struct Shared<'a> {
    net: &'a StreetNetwork,
    layer: Option<&'a TransitLayer>,
    planner: Option<Planner<'a>>,
    search_net: Option<search::Network<'a>>,
    calc: Option<&'a FareCalculator>,
    ip: &'a ItineraryParams,
    p: &'a TripOptionsParams,
}

pub(crate) fn trip_options_impl(
    net: &StreetNetwork,
    transit: Option<(&TransitLayer, &TransitData)>,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TripOptionsParams,
) -> Result<Vec<Vec<TripOption>>, RouteError> {
    let ip = itinerary_params(p);
    ip.check().map_err(RouteError::Invalid)?;
    let criteria = Criteria::of(&p.resolved_criteria()?);
    if origins.len() != destinations.len() {
        return Err(RouteError::Invalid(format!(
            "origins and destinations must pair up: {} origins, {} destinations",
            origins.len(),
            destinations.len()
        )));
    }
    let calc = match (&p.fare_structure, transit) {
        (Some(fs), Some((layer, _))) => Some(FareCalculator::new(fs, layer)?),
        (Some(fs), None) => {
            fs.validate()?;
            None
        }
        _ => None,
    };
    let by_car = p.mode == Mode::Car || (!p.transit_modes.is_empty() && p.egress_mode == Mode::Car);
    let scen = crate::street::scenario::scenario_network(net, &p.scenario, by_car)?;
    let net = scen.as_ref().unwrap_or(net);
    let mut shared = Shared {
        net,
        layer: None,
        planner: None,
        search_net: None,
        calc: calc.as_ref(),
        ip: &ip,
        p,
    };
    if !p.transit_modes.is_empty() {
        let Some((layer, data)) = transit else {
            return Err(RouteError::Invalid(
                "public transport options need the network's transit layer".into(),
            ));
        };
        let planner = Planner::with_frequencies(net, layer, data, &ip, calc.as_ref(), true)?;
        let track_fare = criteria.fare || p.max_fare.is_finite();
        let fare = match (&calc, track_fare) {
            (Some(c), true) => Some(FareModel::new(c)),
            _ => None,
        };
        let walk_mms = (ip.req().walk * 1000.0f32) as i32;
        let cfg = search::Config {
            max_rides: p.max_rides,
            max_t: p.max_trip_duration.saturating_mul(60),
            walk_mms: walk_mms.max(1),
            max_transfer_mm: walk_mms.saturating_mul(p.max_walk_time.saturating_mul(60)),
            criteria,
            max_fare: if p.max_fare.is_finite() {
                crate::transit::matrix::max_fare_cents(p.max_fare)
            } else {
                i32::MAX
            },
        };
        let sn = search::Network::new(
            layer,
            &data.transfers,
            &planner.mc_allowed,
            &planner.mc_active,
            fare,
            cfg,
        )
        .map_err(RouteError::Invalid)?;
        shared.search_net = Some(sn);
        shared.layer = Some(layer);
        shared.planner = Some(planner);
    }
    let n = origins.len();
    let next = AtomicUsize::new(0);
    let threads = crate::street::router::worker_threads(p.n_threads, n);
    let cancel = &p.cancel;
    let shared = &shared;
    type Chunk = Result<Vec<(usize, Vec<TripOption>)>, RouteError>;
    let chunks: Vec<Chunk> = crate::sys::scoped(0..threads, |_| {
        let mut out = Vec::new();
        let mut scratch = Vec::new();
        let mut search = Search::default();
        while !cancel.is_cancelled() {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= n {
                break;
            }
            let v = plan_pair(
                shared,
                i,
                origins[i],
                destinations[i],
                &mut scratch,
                &mut search,
            )?;
            out.push((i, v));
        }
        Ok(out)
    });
    cancel.check()?;
    let mut slots: Vec<Option<Vec<TripOption>>> = (0..n).map(|_| None).collect();
    for chunk in chunks {
        for (i, v) in chunk? {
            slots[i] = Some(v);
        }
    }
    Ok(slots
        .into_iter()
        .map(|s| s.expect("every pair is planned"))
        .collect())
}

/// The trip planner's parameters for the same settings.
fn itinerary_params(p: &TripOptionsParams) -> ItineraryParams {
    ItineraryParams {
        mode: p.mode,
        transit_modes: p.transit_modes.clone(),
        egress_mode: p.egress_mode,
        date: p.date,
        departure_time: p.departure_time,
        time_window: p.time_window,
        suboptimal_minutes: 0,
        max_walk_time: p.max_walk_time,
        max_bike_time: p.max_bike_time,
        max_car_time: p.max_car_time,
        max_trip_duration: p.max_trip_duration,
        walk_speed_kmh: p.walk_speed_kmh,
        bike_speed_kmh: p.bike_speed_kmh,
        max_rides: p.max_rides,
        max_lts: p.max_lts,
        shortest_path: false,
        fare_structure: p.fare_structure.clone(),
        max_fare: p.max_fare,
        n_threads: p.n_threads,
        cancel: p.cancel.clone(),
        osm_link_ids: p.osm_link_ids,
        scenario: p.scenario.clone(),
        fail_fast_raptor: false,
    }
}

/// The access and egress stop searches of a pair (`findAccessPaths`,
/// `findEgressPaths`), with the legs the search starts and ends with. An
/// egress stop whose leg r5r could not draw (`EgressPath`'s
/// `InvalidOriginException`) is left out.
struct StopSearches<'a> {
    access: Router<'a>,
    egress: Router<'a>,
    egress_split: Option<Split>,
    access_legs: Vec<StopLeg>,
    egress_legs: Vec<StopLeg>,
}

fn stop_searches<'a>(
    planner: &'a Planner<'a>,
    o: (f64, f64),
    d: (f64, f64),
    scratch: &mut Vec<u32>,
    via: &mut ViaOrders,
) -> Result<StopSearches<'a>, RouteError> {
    let (net, p, layer) = (planner.net, planner.p, planner.layer);
    let mut access = Router::new(net, p.mode, p.req(), false);
    access.stop_search(&planner.vertex_stop);
    access.time_limit = p.max_time_s(p.mode);
    via.prepare(&mut access);
    let access_split = find_split(net, o.0, o.1, p.mode, scratch);
    if let Some(os) = access_split {
        access.set_origin_split(os);
        access.route();
    }
    via.collect(&mut access);
    let mut egress = Router::new(net, p.egress_mode, p.req(), true);
    egress.stop_search(&planner.vertex_stop);
    egress.time_limit = p.max_time_s(p.egress_mode);
    via.prepare(&mut egress);
    let egress_split = find_split(net, d.0, d.1, p.egress_mode, scratch);
    let mut egress_ok = false;
    if let Some(ds) = egress_split {
        if egress.set_egress_origin(ds) {
            egress.route();
            via.collect(&mut egress);
            egress_ok = true;
            if let Some(why) = egress.unsupported {
                return Err(RouteError::Invalid(format!("not supported: {why}")));
            }
        }
    }
    let on_foot = |m: Mode, secs: i32| if m == Mode::Walk { secs } else { 0 };
    let mut access_legs: Vec<StopLeg> = Vec::new();
    if access_split.is_some() {
        for &s in &access.stops_order {
            let secs = access.reached_stops()[&s];
            access_legs.push(StopLeg {
                stop: s,
                secs,
                walk: on_foot(p.mode, secs),
            });
        }
    }
    access_legs.sort_unstable_by_key(|l| l.stop);
    let mut egress_legs: Vec<StopLeg> = Vec::new();
    if let (true, Some(ds)) = (egress_ok, egress_split) {
        let mut stops: Vec<u32> = egress.stops_order.clone();
        stops.sort_unstable();
        for s in stops {
            let v = layer.stop_vertex[s as usize];
            let drawable = v >= 0
                && egress
                    .state_at_vertex(v as u32)
                    .is_some_and(|st| egress_line(&egress, st, &ds, d).is_some());
            if !drawable {
                continue;
            }
            let secs = egress.reached_stops()[&s];
            egress_legs.push(StopLeg {
                stop: s,
                secs,
                walk: on_foot(p.egress_mode, secs),
            });
        }
    }
    Ok(StopSearches {
        access,
        egress,
        egress_split,
        access_legs,
        egress_legs,
    })
}

/// The options of one pair.
fn plan_pair(
    sh: &Shared,
    pair: usize,
    o: (f64, f64),
    d: (f64, f64),
    scratch: &mut Vec<u32>,
    search: &mut Search,
) -> Result<Vec<TripOption>, RouteError> {
    let p = sh.p;
    if ![o.0, o.1, d.0, d.1].iter().all(|v| v.is_finite()) {
        return Ok(Vec::new());
    }
    let mut via = ViaOrders::as_stated();
    let direct = direct_trip(sh.net, sh.ip, o, d, scratch, &mut via);
    let max_t = p.max_trip_duration.saturating_mul(60);
    let direct_values = direct
        .as_ref()
        .map(|t| {
            (
                t.sort_duration,
                if p.mode == Mode::Walk {
                    t.sort_duration
                } else {
                    0
                },
            )
        })
        .filter(|&(secs, _)| secs <= max_t);
    let fare_none = if p.fare_structure.is_some() { 0 } else { -1 };
    // (signature, fare, transfers, time on foot) -> merged option: the
    // same trip but for the vehicles and the wait, whose total time changes
    // with the departure minute
    type Key = (Vec<u32>, i32, u32, i32);
    let mut merged: BTreeMap<Key, Merged> = BTreeMap::new();
    let mut add = |values: Values, departure: i32, path: Option<Path>, sig: Vec<u32>| {
        let key = (sig, values.1, values.2, values.3);
        match merged.get_mut(&key) {
            Some(m) => {
                m.departures.push(departure);
                m.totals.push(values.0);
                if values.0 < m.values.0 {
                    m.values = values;
                    m.departure = departure;
                    m.path = path;
                }
            }
            None => {
                merged.insert(
                    key,
                    Merged {
                        values,
                        departure,
                        departures: vec![departure],
                        totals: vec![values.0],
                        description: description(sh.layer, p, path.as_ref()),
                        path,
                    },
                );
            }
        }
    };
    let mut searches = None;
    match (&sh.planner, &sh.search_net) {
        (Some(planner), Some(sn)) => {
            let ss = stop_searches(planner, o, d, scratch, &mut via)?;
            let bounds = crate::options::bounds::Bounds::new(sn, &ss.egress_legs);
            for k in 0..p.time_window {
                p.cancel.check()?;
                let dep = p.departure_time + k * 60;
                let arrivals = search.run(
                    sn,
                    dep,
                    &ss.access_legs,
                    &ss.egress_legs,
                    direct_values,
                    &bounds,
                    &p.cancel,
                );
                for a in arrivals {
                    if a.label == NONE {
                        let (secs, walk) = direct_values.expect("direct");
                        add(
                            (secs, fare_none, 0, walk),
                            dep,
                            None,
                            signature(sh.layer, p, None),
                        );
                        continue;
                    }
                    let path = path_of(search, &a, &ss.egress_legs, dep);
                    let fare = match sh.calc {
                        Some(c) => c.fare(&path.rides()).paid,
                        None => -1,
                    };
                    let sig = signature(sh.layer, p, Some(&path));
                    add((a.total, fare, a.transfers(), a.walk), dep, Some(path), sig);
                }
            }
            p.cancel.check()?;
            searches = Some(ss);
        }
        _ => {
            if let Some((secs, walk)) = direct_values {
                for k in 0..p.time_window {
                    let dep = p.departure_time + k * 60;
                    add(
                        (secs, fare_none, 0, walk),
                        dep,
                        None,
                        signature(sh.layer, p, None),
                    );
                }
            }
        }
    }
    let mut list: Vec<Merged> = merged.into_values().collect();
    list.sort_by(|a, b| {
        let (ta, fa, xa, wa) = a.values;
        let (tb, fb, xb, wb) = b.values;
        (ta, fa, xa, wa, a.departure, &a.description).cmp(&(
            tb,
            fb,
            xb,
            wb,
            b.departure,
            &b.description,
        ))
    });
    if p.max_options > 0 {
        list.truncate(p.max_options);
    }
    let mut out = Vec::with_capacity(list.len());
    for (k, m) in list.into_iter().enumerate() {
        let (total, fare, transfers, walk) = m.values;
        let departure = m.departure;
        let itinerary = if p.itineraries {
            let trip = match &m.path {
                None => {
                    let mut t = direct.clone().expect("direct trip");
                    t.departure = departure;
                    if p.fare_structure.is_some() {
                        t.fares = Some((0, vec![0; t.legs.len()]));
                    }
                    Some(t)
                }
                Some(path) => {
                    let ss = searches.as_ref().expect("stop searches");
                    build_trip(sh, path, departure, total, ss, d, scratch)?
                }
            };
            trip.map(|t| {
                let mut it = super::finish(pair, vec![t]).remove(0);
                it.option = k as u32 + 1;
                it
            })
        } else {
            None
        };
        out.push(TripOption {
            pair,
            option: k as u32 + 1,
            total_time_s: total,
            total_time: minutes_1dp(total),
            fare: (fare >= 0).then(|| f64::from(fare) / 100.0),
            transfers,
            walk_time_s: walk,
            walk_time: minutes_1dp(walk),
            departure_s: departure,
            arrival_s: departure + total,
            departures: m.departures,
            totals: m.totals.iter().map(|&t| minutes_1dp(t)).collect(),
            totals_s: m.totals,
            description: m.description,
            itinerary,
        });
    }
    Ok(out)
}

/// The legs of a trip found by the search, as `Trip.augment` builds those
/// of R5's McRAPTOR trips.
fn build_trip(
    sh: &Shared,
    path: &Path,
    departure: i32,
    total: i32,
    ss: &StopSearches,
    d: (f64, f64),
    scratch: &mut Vec<u32>,
) -> Result<Option<Trip>, RouteError> {
    let planner = sh.planner.as_ref().expect("planner");
    let rides = path.rides();
    let mut n_rides = 0;
    let mut fare_now = 0;
    let mut legs = Vec::with_capacity(path.legs.len());
    for l in &path.legs {
        match *l {
            PathLeg::Ride {
                pattern,
                board_pos,
                alight_pos,
                board_time,
                alight_time,
                wait,
            } => {
                n_rides += 1;
                fare_now = sh.calc.map_or(0, |c| c.fare(&rides[..n_rides]).paid);
                legs.push(RawLeg::Ride {
                    pattern: pattern as usize,
                    board_pos: board_pos as usize,
                    alight_pos: alight_pos as usize,
                    duration: alight_time - board_time,
                    wait,
                    fare: fare_now,
                });
            }
            PathLeg::Walk { from, to, secs } => legs.push(RawLeg::Walk {
                from,
                to,
                duration: secs,
                fare: fare_now,
            }),
        }
    }
    let raw = RawTrip {
        direct: None,
        departure,
        duration: total,
        fare: fare_now,
        legs,
    };
    planner.augment(
        raw,
        &ss.access,
        &ss.egress,
        ss.egress_split.as_ref(),
        d,
        sh.calc.is_some(),
        scratch,
    )
}

#[cfg(test)]
#[path = "options_r5cmp.rs"]
mod r5_compare;
