//! `tripOptions()`: the core's options as columns, one element per option,
//! per option and departure minute, and per leg.

use super::{
    err, f64_array, i32_array, parse_transit_modes, points, route_err, street_mode, strings,
    Network,
};
use js_sys::Array;
use peabiru_core::{trip_options, Criterion, TripOptionsParams};
use wasm_bindgen::prelude::*;

/// `HH:MM:SS`, hours past 24 as they come.
fn clock(s: i32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Options between `o[i]` and `d[i]` as an array of three arrays of
/// columns. Options: `pair` (0-based), `option`, `transfers`,
/// `n_departures` (Int32Array), `total_time`, `fare` (NaN without a fare
/// structure), `walk_time` (Float64Array), `departure_time`,
/// `arrival_time`, `first_departure`, `last_departure`, `description`
/// (strings). Departures: `pair`, `option` (Int32Array), `total_time`
/// (Float64Array), `departure_time`, `arrival_time` (strings). Legs (empty
/// without `itineraries`): `pair`, `option`, `segment`, `distance`,
/// `n_points` (Int32Array), `cumulative_fare`, `segment_duration`, `wait`,
/// `x`, `y` (Float64Array), `mode`, `route` and, with `osm_link_ids`,
/// `osm_id_list`, `edge_id_list`, `board_stop_id`, `alight_stop_id`
/// (strings).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn trip_options_js(
    network: &Network,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    transit_modes: &str,
    egress_mode: &str,
    date: u32,
    departure_time: i32,
    time_window: i32,
    criteria: Vec<String>,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_rides: usize,
    max_lts: i32,
    scenario: JsValue,
    fare: JsValue,
    max_fare: f64,
    max_options: usize,
    itineraries: bool,
    osm_link_ids: bool,
) -> Result<Array, JsError> {
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    if o.len() != d.len() {
        return Err(err("origins and destinations must pair up"));
    }
    let mut p = TripOptionsParams::default();
    p.scenario = crate::scenario::scenario_of(&scenario)?;
    p.mode = street_mode(mode)?;
    p.transit_modes = parse_transit_modes(transit_modes)?;
    if !p.transit_modes.is_empty() {
        p.egress_mode = street_mode(egress_mode)?;
    }
    p.date = date;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.criteria = criteria
        .iter()
        .map(|c| Criterion::from_name(c).ok_or_else(|| err(format!("unknown criterion '{c}'"))))
        .collect::<Result<_, _>>()?;
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_rides = max_rides;
    p.max_lts = max_lts;
    p.fare_structure = super::fares::fare_arg(&fare)?;
    p.max_fare = max_fare;
    p.max_options = max_options;
    p.itineraries = itineraries;
    p.osm_link_ids = osm_link_ids;
    p.n_threads = 1;
    let res = trip_options(&network.net, &o, &d, &p).map_err(route_err)?;
    let mut oi: [Vec<i32>; 4] = Default::default();
    let mut of: [Vec<f64>; 3] = Default::default();
    let mut os: [Vec<String>; 5] = Default::default();
    let mut di: [Vec<i32>; 2] = Default::default();
    let mut df: Vec<f64> = Vec::new();
    let mut ds: [Vec<String>; 2] = Default::default();
    let mut li: [Vec<i32>; 5] = Default::default();
    let mut lf: [Vec<f64>; 5] = Default::default();
    let mut ls: [Vec<String>; 6] = Default::default();
    for (k, options) in res.iter().enumerate() {
        let pk = k as i32;
        for t in options {
            for (c, v) in oi.iter_mut().zip([
                pk,
                t.option as i32,
                t.transfers as i32,
                t.n_departures() as i32,
            ]) {
                c.push(v);
            }
            of[0].push(t.total_time);
            of[1].push(t.fare.unwrap_or(f64::NAN));
            of[2].push(t.walk_time);
            os[0].push(clock(t.departure_s));
            os[1].push(clock(t.arrival_s));
            os[2].push(clock(t.departures[0]));
            os[3].push(clock(*t.departures.last().expect("a departure")));
            os[4].push(t.description.clone());
            for ((&dp, &tt), &tm) in t.departures.iter().zip(&t.totals_s).zip(&t.totals) {
                di[0].push(pk);
                di[1].push(t.option as i32);
                df.push(tm);
                ds[0].push(clock(dp));
                ds[1].push(clock(dp + tt));
            }
            if let Some(it) = &t.itinerary {
                for (j, l) in it.legs.iter().enumerate() {
                    for (c, v) in li.iter_mut().zip([
                        pk,
                        t.option as i32,
                        j as i32 + 1,
                        l.distance,
                        l.geometry.len() as i32,
                    ]) {
                        c.push(v);
                    }
                    lf[0].push(l.cumulative_fare.unwrap_or(f64::NAN));
                    lf[1].push(l.segment_duration);
                    lf[2].push(l.wait);
                    for c in &l.geometry {
                        lf[3].push(c[0]);
                        lf[4].push(c[1]);
                    }
                    ls[0].push(l.mode.clone());
                    ls[1].push(l.route.clone());
                    if osm_link_ids {
                        ls[2].push(l.osm_id_list());
                        ls[3].push(l.edge_id_list());
                        ls[4].push(l.board_stop_id.clone());
                        ls[5].push(l.alight_stop_id.clone());
                    }
                }
            }
        }
    }
    let options = Array::new();
    oi.iter().for_each(|c| {
        options.push(&i32_array(c));
    });
    of.iter().for_each(|c| {
        options.push(&f64_array(c));
    });
    os.iter().for_each(|c| {
        options.push(&strings(c));
    });
    let departures = Array::new();
    di.iter().for_each(|c| {
        departures.push(&i32_array(c));
    });
    departures.push(&f64_array(&df));
    ds.iter().for_each(|c| {
        departures.push(&strings(c));
    });
    let legs = Array::new();
    li.iter().for_each(|c| {
        legs.push(&i32_array(c));
    });
    lf.iter().for_each(|c| {
        legs.push(&f64_array(c));
    });
    ls.iter().for_each(|c| {
        legs.push(&strings(c));
    });
    let out = Array::new();
    out.push(&options);
    out.push(&departures);
    out.push(&legs);
    Ok(out)
}
