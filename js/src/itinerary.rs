//! `detailedItineraries()`: the core's itineraries as columns, one element
//! per leg.

use super::{
    err, f64_array, i32_array, parse_transit_modes, points, route_err, street_mode, strings,
    Network,
};
use js_sys::Array;
use peabiru_core::{detailed_itineraries_noted, ItineraryNotes, ItineraryParams};
use wasm_bindgen::prelude::*;

/// Itineraries between `o[i]` and `d[i]` as an array of columns: `pair`
/// (0-based), `option`, `total_distance`, `segment`, `distance`,
/// `n_points` (Int32Array), `total_duration`, `segment_duration`, `wait`,
/// `x` (longitude), `y` (latitude) (Float64Array), `departure_time`,
/// `mode`, `route` (arrays of strings). The lines' points are `x`, `y`,
/// leg after leg, `n_points` per leg. With `osm_link_ids`, four more
/// arrays of strings: `osm_id_list`, `edge_id_list`, `board_stop_id`,
/// `alight_stop_id` (empty without), then `total_fare` and
/// `cumulative_fare` (Float64Array), and last the pairs (0-based,
/// Int32Array) whose itineraries in r5r depend on what ran before (see
/// [`itinerary_warnings`]).
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn itineraries(
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
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_rides: usize,
    max_lts: i32,
    shortest_path: bool,
    scenario: JsValue,
    osm_link_ids: bool,
    suboptimal_minutes: i32,
    fare: JsValue,
    max_fare: f64,
) -> Result<Array, JsError> {
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    if o.len() != d.len() {
        return Err(err("origins and destinations must pair up"));
    }
    let mut p = ItineraryParams::default();
    p.scenario = crate::scenario::scenario_of(&scenario)?;
    p.mode = street_mode(mode)?;
    p.transit_modes = parse_transit_modes(transit_modes)?;
    if !p.transit_modes.is_empty() {
        p.egress_mode = street_mode(egress_mode)?;
    }
    p.date = date;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed = walk_speed;
    p.bike_speed = bike_speed;
    p.max_rides = max_rides;
    p.max_lts = max_lts;
    p.shortest_path = shortest_path;
    p.suboptimal_minutes = suboptimal_minutes;
    p.fare_structure = super::fares::fare_arg(&fare)?;
    p.max_fare = max_fare;
    p.n_threads = 1;
    p.osm_link_ids = osm_link_ids;
    let (res, notes) = detailed_itineraries_noted(&network.net, &o, &d, &p).map_err(route_err)?;
    let mut ints: [Vec<i32>; 6] = Default::default();
    let mut nums: [Vec<f64>; 5] = Default::default();
    let mut fare_cols: [Vec<f64>; 2] = Default::default();
    let mut texts: [Vec<String>; 7] = Default::default();
    for (k, its) in res.iter().enumerate() {
        for it in its {
            for (j, l) in it.legs.iter().enumerate() {
                for (c, v) in ints.iter_mut().zip([
                    k as i32,
                    it.option as i32,
                    it.total_distance,
                    j as i32 + 1,
                    l.distance,
                    l.geometry.len() as i32,
                ]) {
                    c.push(v);
                }
                nums[0].push(it.total_duration);
                nums[1].push(l.segment_duration);
                nums[2].push(l.wait);
                for c in &l.geometry {
                    nums[3].push(c[0]);
                    nums[4].push(c[1]);
                }
                fare_cols[0].push(it.total_fare.unwrap_or(f64::NAN));
                fare_cols[1].push(l.cumulative_fare.unwrap_or(f64::NAN));
                texts[0].push(it.departure_time.clone());
                texts[1].push(l.mode.clone());
                texts[2].push(l.route.clone());
                if osm_link_ids {
                    texts[3].push(l.osm_id_list());
                    texts[4].push(l.edge_id_list());
                    texts[5].push(l.board_stop_id.clone());
                    texts[6].push(l.alight_stop_id.clone());
                }
            }
        }
    }
    let out = Array::new();
    for c in &ints {
        out.push(&i32_array(c));
    }
    for c in &nums {
        out.push(&f64_array(c));
    }
    // the four id columns are empty without osm_link_ids
    for c in &texts {
        out.push(&strings(c));
    }
    // with a fare structure: total_fare, cumulative_fare
    for c in &fare_cols {
        out.push(&f64_array(c));
    }
    let dependent: Vec<i32> = notes.history_dependent.iter().map(|&i| i as i32).collect();
    out.push(&i32_array(&dependent));
    Ok(out)
}

/// The warnings of a call of `n_pairs` pairs, given the pairs (0-based,
/// ascending, over the whole call) whose itineraries in r5r depend on what
/// ran before (`ItineraryNotes::warnings` of the core).
#[wasm_bindgen]
pub fn itinerary_warnings(history_dependent: &[i32], n_pairs: usize) -> JsValue {
    let mut notes = ItineraryNotes::default();
    notes.history_dependent = history_dependent.iter().map(|&i| i as usize).collect();
    strings(&notes.warnings(n_pairs))
}
