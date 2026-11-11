//! `detailedItineraries()`: the core's itineraries as columns, one element
//! per leg.

use super::{
    err, f64_array, i32_array, parse_transit_modes, points, route_err, street_mode, strings,
    Network,
};
use js_sys::Array;
use peabiru_core::{detailed_itineraries, ItineraryParams};
use wasm_bindgen::prelude::*;

/// Itineraries between `o[i]` and `d[i]` as an array of columns: `pair`
/// (0-based), `option`, `total_distance`, `segment`, `distance`,
/// `n_points` (Int32Array), `total_duration`, `segment_duration`, `wait`,
/// `x` (longitude), `y` (latitude) (Float64Array), `departure_time`,
/// `mode`, `route` (arrays of strings). The lines' points are `x`, `y`,
/// leg after leg, `n_points` per leg.
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
) -> Result<Array, JsError> {
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    if o.len() != d.len() {
        return Err(err("origins and destinations must pair up"));
    }
    let mut p = ItineraryParams::default();
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
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_rides = max_rides;
    p.max_lts = max_lts;
    p.shortest_path = shortest_path;
    p.n_threads = 1;
    let transit = network.transit.as_ref().map(|t| (&t.layer, &t.data));
    let res = detailed_itineraries(&network.net, transit, &o, &d, &p).map_err(route_err)?;
    let mut ints: [Vec<i32>; 6] = Default::default();
    let mut nums: [Vec<f64>; 5] = Default::default();
    let mut texts: [Vec<String>; 3] = Default::default();
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
                texts[0].push(it.departure_time.clone());
                texts[1].push(l.mode.clone());
                texts[2].push(l.route.clone());
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
    for c in &texts {
        out.push(&strings(c));
    }
    Ok(out)
}
