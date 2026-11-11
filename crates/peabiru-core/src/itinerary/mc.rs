//! The public transport trips of r5r's `TripPlanner` with McRAPTOR: with
//! `shortest_path = FALSE` (the alternatives, with `suboptimal_minutes`),
//! with a fare structure, and when FastRaptor's path cannot be read back
//! (r5r then routes the pair again with McRAPTOR). The search is R5's
//! `McRaptorSuboptimalPathProfileRouter` from the stops of the access
//! search to those of the egress search; every state at the destination of
//! every draw becomes a trip (`Trip(state, departureTime, ...)`), one per
//! sequence of routes (`Trip.getKey`), the fastest of each.

use super::geom::line_length_m;
use super::hops::ride_line;
use super::transit::{egress_line, Planner};
use super::{street_path_line, Leg, Trip};
use crate::fares::FareCalculator;
use crate::jcompat::{string_hash, JavaHashMap, TroveIntKeys};
use crate::street::link::{find_split, Split};
use crate::street::p2p::Router;
use crate::transit::mcraptor::{Dominance, DrawOut, McRaptor, McRequest, McState, NONE};
use crate::transit::raptor::route_type_mode;
use crate::RouteError;

/// One leg of a trip as `Trip.loadTransitLegs` reads it from the states.
#[derive(Debug, Clone)]
enum RawLeg {
    /// A walk between two stops (`newStopTransferLeg`).
    Walk {
        from: u32,
        to: u32,
        duration: i32,
        fare: i32,
    },
    /// A ride (`newTransitLeg` with `setPatternData`).
    Ride {
        pattern: usize,
        board_pos: usize,
        alight_pos: usize,
        duration: i32,
        wait: i32,
        fare: i32,
    },
}

/// A trip before `augment`.
#[derive(Debug, Clone)]
pub(super) struct RawTrip {
    pub(super) direct: Option<Trip>,
    departure: i32,
    /// `state.time - departureTime`.
    pub(super) duration: i32,
    pub(super) fare: i32,
    legs: Vec<RawLeg>,
}

impl RawTrip {
    pub(super) fn n_legs(&self) -> usize {
        match &self.direct {
            Some(t) => t.sort_legs,
            None => self.legs.len(),
        }
    }
}

/// The fare setup of a McRAPTOR itinerary search.
pub(crate) struct McFares<'a> {
    pub(crate) calc: &'a FareCalculator,
    /// `request.maxFare`, cents.
    pub(crate) max_fare: i32,
}

/// Read the legs of the chain ending at `s` (`Trip.loadTransitLegs`).
fn raw_trip(
    arena: &[McState],
    s: u32,
    departure: i32,
    layer: &crate::TransitLayer,
) -> Result<(RawTrip, String), String> {
    let st = &arena[s as usize];
    let fare = st.fare.map_or(0, |f| f.paid);
    let mut legs = Vec::new();
    let mut routes = Vec::new();
    let mut k = s;
    while k != NONE {
        let st = &arena[k as usize];
        if st.stop != -1 && (st.pattern != -1 || st.back != NONE) {
            let fare = st.fare.map_or(0, |f| f.paid);
            let back = &arena[st.back as usize];
            if st.pattern == -1 {
                legs.push(RawLeg::Walk {
                    from: back.stop as u32,
                    to: st.stop as u32,
                    duration: st.time - back.time,
                    fare,
                });
            } else {
                let pattern = st.pattern as usize;
                routes.push(layer.patterns[pattern].route_id.clone());
                legs.push(RawLeg::Ride {
                    pattern,
                    board_pos: st.board_pos as usize,
                    alight_pos: st.alight_pos as usize,
                    duration: st.time - st.board_time,
                    wait: st.board_time - back.time,
                    fare,
                });
            }
        }
        k = st.back;
    }
    legs.reverse();
    routes.reverse();
    let key = routes
        .into_iter()
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>()
        .join("|");
    Ok((
        RawTrip {
            direct: None,
            departure,
            duration: st.time - departure,
            fare,
            legs,
        },
        key,
    ))
}

impl Planner<'_> {
    /// `TripPlanner.plan()` with McRAPTOR for one pair: the direct trip (if
    /// any) and the transit trips, filtered, sorted and (with
    /// `shortest_path`) reduced to the fastest, then augmented.
    pub(super) fn mc_plan(
        &self,
        o: (f64, f64),
        d: (f64, f64),
        direct: Option<Trip>,
        fares: Option<&McFares>,
        scratch: &mut Vec<u32>,
    ) -> Result<Vec<Trip>, RouteError> {
        let (net, p, layer) = (self.net, self.p, self.layer);
        let max_t = p.max_trip_duration * 60;
        // trips: Map<String, Trip>, the direct trip under its mode's name
        let mut keys: JavaHashMap<String> = JavaHashMap::new();
        let mut values: Vec<RawTrip> = Vec::new();
        let put =
            |key: String, t: RawTrip, keys: &mut JavaHashMap<String>, values: &mut Vec<RawTrip>| {
                let h = string_hash(Some(&key));
                let i = keys.insert(key, h).map_err(RouteError::Invalid)?;
                if i == values.len() {
                    values.push(t);
                } else if values[i].duration > t.duration {
                    values[i] = t;
                }
                Ok::<_, RouteError>(())
            };
        if let Some(t) = &direct {
            let key = p.mode.r5_name().to_string();
            let h = string_hash(Some(&key));
            keys.insert(key, h).map_err(RouteError::Invalid)?;
            values.push(RawTrip {
                direct: Some(t.clone()),
                departure: t.departure,
                duration: t.sort_duration,
                fare: 0,
                legs: Vec::new(),
            });
        }
        // access and egress stop searches (findAccessPaths, findEgressPaths)
        let mut access = Router::new(net, p.mode, p.req(), false);
        access.stop_search(&self.vertex_stop);
        access.time_limit = p.max_time_s(p.mode);
        let access_split = find_split(net, o.0, o.1, p.mode, scratch);
        if let Some(os) = access_split {
            access.set_origin_split(os);
            access.route();
        }
        let mut egress = Router::new(net, p.egress_mode, p.req(), true);
        egress.stop_search(&self.vertex_stop);
        egress.time_limit = p.max_time_s(p.egress_mode);
        let egress_split: Option<Split> = find_split(net, d.0, d.1, p.egress_mode, scratch);
        let mut egress_ok = false;
        if let Some(ds) = egress_split {
            if egress.set_egress_origin(ds) {
                egress.route();
                egress_ok = true;
                if let Some(why) = egress.unsupported {
                    return Err(RouteError::Invalid(format!("not supported: {why}")));
                }
            }
        }
        let access_times: Vec<(u32, i32)> = if access_split.is_some() {
            access
                .stops_order
                .iter()
                .map(|s| (*s, access.reached_stops()[s]))
                .collect()
        } else {
            Vec::new()
        };
        // the egress stops in the iteration order of R5's TIntIntHashMap
        let egress_times: Vec<(u32, i32)> = if egress_ok {
            let mut t = TroveIntKeys::new();
            for &s in &egress.stops_order {
                t.insert(s as i32);
            }
            t.iteration_order()
                .map(|s| (s as u32, egress.reached_stops()[&(s as u32)]))
                .collect()
        } else {
            Vec::new()
        };
        let max_fare = fares.map_or(-1, |f| f.max_fare);
        let req = McRequest {
            from_time: p.departure_time,
            to_time: p.departure_time + p.time_window * 60,
            max_trip_s: max_t,
            max_rides: p.max_rides,
            suboptimal_minutes: p.suboptimal_minutes,
            max_fare,
            monte_carlo_draws: p.time_window,
            walk_mms: f64::from(p.req().walk * 1000.0f32),
            seed: None,
            origin: o,
        };
        let dom = match fares {
            Some(f) => Dominance::Fare(f.calc),
            None => {
                let mut max_duration = max_t;
                if p.shortest_path {
                    if let Some(t) = &direct {
                        max_duration = max_duration.min(t.sort_duration);
                    }
                }
                Dominance::Suboptimal {
                    max_duration,
                    shortest: p.shortest_path,
                }
            }
        };
        let allowed = &self.mc_allowed;
        let active = &self.mc_active;
        let mut router = McRaptor::new(
            layer,
            &self.data.transfers,
            allowed,
            active,
            &req,
            dom,
            Some(&egress_times),
            McRaptor::departure_seed(None, o),
        );
        // finalStatesByDepartureTime: per draw, its trips and keys
        let mut by_departure: Vec<(i32, Vec<(RawTrip, String)>)> = Vec::new();
        let mut failure = None;
        router
            .route(&access_times, &p.cancel, |dep, out| {
                if let DrawOut::Final(arena, states) = out {
                    let mut v = Vec::with_capacity(states.len());
                    for &s in states {
                        match raw_trip(arena, s, dep, layer) {
                            Ok(t) => v.push(t),
                            Err(e) => failure = Some(e),
                        }
                    }
                    by_departure.push((dep, v));
                }
            })
            .map_err(RouteError::Invalid)?;
        p.cancel.check()?;
        if let Some(e) = failure {
            return Err(RouteError::Invalid(e));
        }
        let mut order = TroveIntKeys::new();
        for (dep, _) in &by_departure {
            order.insert(*dep);
        }
        let mut by_dep: std::collections::HashMap<i32, Vec<(RawTrip, String)>> =
            by_departure.into_iter().collect();
        for dep in order.iteration_order().collect::<Vec<_>>() {
            for (t, key) in by_dep.remove(&dep).unwrap_or_default() {
                put(key, t, &mut keys, &mut values)?;
            }
        }
        // values in the map's order, filtered, sorted (stable)
        let mut list: Vec<RawTrip> = keys
            .order()
            .map(|i| values[i].clone())
            .filter(|t| t.duration <= max_t && t.fare <= fares.map_or(i32::MAX, |f| f.max_fare))
            .collect();
        list.sort_by_key(|t| (i32::from(t.direct.is_none()), t.n_legs(), t.duration));
        if p.shortest_path {
            if let Some(best) = list.iter().map(|t| t.duration).min() {
                let k = list.iter().position(|t| t.duration == best).expect("min");
                list = vec![list.swap_remove(k)];
            }
        }
        let mut out = Vec::with_capacity(list.len());
        for t in list {
            if let Some(mut d) = t.direct {
                if fares.is_some() {
                    d.fares = Some((0, vec![0; d.legs.len()]));
                }
                out.push(d);
                continue;
            }
            if let Some(t) = self.augment(
                t,
                &access,
                &egress,
                egress_split.as_ref(),
                d,
                fares.is_some(),
                scratch,
            )? {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// `Trip.augment` of a McRAPTOR trip: the rides drawn, the walks between
    /// stops routed, the access and egress legs added. `None` when r5r skips
    /// the trip (`EgressPath.InvalidOriginException`).
    #[allow(clippy::too_many_arguments)]
    fn augment(
        &self,
        t: RawTrip,
        access: &Router,
        egress: &Router,
        egress_split: Option<&Split>,
        d: (f64, f64),
        with_fares: bool,
        scratch: &mut Vec<u32>,
    ) -> Result<Option<Trip>, RouteError> {
        let (net, layer, p) = (self.net, self.layer, self.p);
        let n_legs = t.legs.len();
        let mut out: Vec<Leg> = Vec::with_capacity(n_legs + 2);
        let mut fares: Vec<i32> = Vec::with_capacity(n_legs + 2);
        let mut first_board = None;
        let mut last_alight = None;
        for l in &t.legs {
            match *l {
                RawLeg::Walk {
                    from,
                    to,
                    duration,
                    fare,
                } => {
                    let mut leg = self.stop_transfer_leg(from, to, duration);
                    self.route_transfer(&mut leg, from, to, scratch);
                    out.push(leg);
                    fares.push(fare);
                    if first_board.is_none() {
                        first_board = Some(from);
                    }
                    last_alight = Some(to);
                }
                RawLeg::Ride {
                    pattern,
                    board_pos,
                    alight_pos,
                    duration,
                    wait,
                    fare,
                } => {
                    let pat = &layer.patterns[pattern];
                    let mode = match route_type_mode(pat.route_type) {
                        Some(Some(m)) => m.name().to_string(),
                        Some(None) => "AIR".to_string(),
                        None => {
                            return Err(RouteError::Invalid(format!(
                                "unsupported GTFS route_type {}",
                                pat.route_type
                            )))
                        }
                    };
                    let line = ride_line(net, layer, pattern, board_pos, alight_pos)
                        .map_err(|why| RouteError::Invalid(format!("not supported: {why}")))?;
                    let distance = line_length_m(&line);
                    out.push(Leg {
                        mode,
                        duration,
                        wait,
                        route: pat.route_id.clone(),
                        line,
                        distance,
                    });
                    fares.push(fare);
                    if first_board.is_none() {
                        first_board = Some(pat.stops[board_pos]);
                    }
                    last_alight = Some(pat.stops[alight_pos]);
                }
            }
        }
        let last_fare = fares.last().copied().unwrap_or(0);
        if let Some(first) = first_board {
            let v = layer.stop_vertex[first as usize];
            if let Some(st) = (v >= 0).then(|| access.state_at_vertex(v as u32)).flatten() {
                let line = street_path_line(access, st);
                let distance = line_length_m(&line);
                out.insert(
                    0,
                    Leg {
                        mode: p.mode.r5_name().to_string(),
                        duration: access.states[st as usize].dur,
                        wait: 0,
                        route: String::new(),
                        line,
                        distance,
                    },
                );
                fares.insert(0, 0);
            }
        }
        if let (Some(last), Some(ds)) = (last_alight, egress_split) {
            let v = layer.stop_vertex[last as usize];
            if let Some(st) = (v >= 0).then(|| egress.state_at_vertex(v as u32)).flatten() {
                match egress_line(egress, st, ds, d) {
                    Some(line) => {
                        let distance = line_length_m(&line);
                        out.push(Leg {
                            mode: p.egress_mode.r5_name().to_string(),
                            duration: egress.states[st as usize].dur,
                            wait: 0,
                            route: String::new(),
                            line,
                            distance,
                        });
                        fares.push(last_fare);
                    }
                    None => return Ok(None),
                }
            }
        }
        let duration = out.iter().map(|l| l.duration + l.wait).sum();
        Ok(Some(Trip {
            direct: false,
            departure: t.departure,
            sort_duration: t.duration,
            sort_legs: n_legs,
            duration,
            legs: out,
            fares: with_fares.then_some((t.fare, fares)),
        }))
    }
}
