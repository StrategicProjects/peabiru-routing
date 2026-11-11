//! `detailed_itineraries()`: the core's itineraries as plain vectors, one
//! element per leg, with the geometry as WKT text for `sf::st_as_sfc()`.

use super::{err, interruptible, network, points, street_mode, transit_modes, Outcome};
use extendr_api::prelude::*;
use extendr_api::Result;
use peabiru_core::{detailed_itineraries, write_itineraries_csv, ItineraryParams, RouteError};
use std::fmt::Write as _;

/// One leg's line as `LINESTRING (x y, ...)` with every coordinate in its
/// shortest round-trip decimal form (what sf reads back is the same
/// double).
fn wkt(line: &[[f64; 2]]) -> String {
    if line.is_empty() {
        return "LINESTRING EMPTY".into();
    }
    let mut s = String::from("LINESTRING (");
    for (i, p) in line.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        let _ = write!(s, "{} {}", p[0], p[1]);
    }
    s.push(')');
    s
}

/// Itineraries between `origins[i]` and `destinations[i]`, as
/// `list(pair, option, departure_time, total_duration, total_distance,
/// segment, mode, segment_duration, wait, distance, route, geometry)`
/// (`pair` 1-based; `geometry` `NULL` with `drop_geometry`). With `output`
/// (`list(dir, from_ids, to_ids)`) the files are written instead and
/// `NULL` returned.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_detailed_itineraries(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    transit: &str,
    egress_mode: &str,
    date: i32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_rides: i32,
    max_lts: i32,
    shortest_path: bool,
    n_threads: i32,
    drop_geometry: bool,
    scenario: Robj,
    output: Robj,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let mut p = ItineraryParams::default();
    p.scenario = crate::scenario::scenario_of(&scenario)?;
    p.mode = street_mode(mode)?;
    p.transit_modes = transit_modes(transit)?;
    if !p.transit_modes.is_empty() {
        p.egress_mode = street_mode(egress_mode)?;
    }
    p.date = date.max(0) as u32;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_rides = max_rides.max(0) as usize;
    p.max_lts = max_lts;
    p.shortest_path = shortest_path;
    p.n_threads = n_threads.max(0) as usize;
    let transit = n.transit.as_ref().map(|t| (&t.layer, &t.data));
    let res = match interruptible(&p.cancel, || {
        detailed_itineraries(&n.net, transit, &o, &d, &p)
    })? {
        Outcome::Done(v) => v,
        Outcome::Stopped(cond) => return super::stopped(cond),
    };
    if !output.is_null() {
        let get = |name: &str| -> Result<Vec<String>> {
            output
                .dollar(name)
                .ok()
                .and_then(|x| x.as_string_vector())
                .ok_or_else(|| err(format!("output${name} must be a character vector")))
        };
        let dir = get("dir")?.pop().unwrap_or_default();
        let ids: Vec<(String, String)> = get("from_ids")?.into_iter().zip(get("to_ids")?).collect();
        let pts: Vec<_> = o.iter().copied().zip(d.iter().copied()).collect();
        write_itineraries_csv(&dir, &ids, &pts, &res)
            .map_err(|e| err(RouteError::Invalid(format!("cannot write {dir}: {e}"))))?;
        return Ok(().into());
    }
    let mut pair = Vec::new();
    let mut option = Vec::new();
    let mut departure = Vec::new();
    let mut total_duration = Vec::new();
    let mut total_distance = Vec::new();
    let mut segment = Vec::new();
    let mut mode = Vec::new();
    let mut segment_duration = Vec::new();
    let mut wait = Vec::new();
    let mut distance = Vec::new();
    let mut route = Vec::new();
    let mut geometry = Vec::new();
    for (k, its) in res.iter().enumerate() {
        for it in its {
            for (j, l) in it.legs.iter().enumerate() {
                pair.push(k as i32 + 1);
                option.push(it.option as i32);
                departure.push(it.departure_time.clone());
                total_duration.push(it.total_duration);
                total_distance.push(it.total_distance);
                segment.push(j as i32 + 1);
                mode.push(l.mode.clone());
                segment_duration.push(l.segment_duration);
                wait.push(l.wait);
                distance.push(l.distance);
                route.push(l.route.clone());
                if !drop_geometry {
                    geometry.push(wkt(&l.geometry));
                }
            }
        }
    }
    let geometry: Robj = if drop_geometry {
        ().into()
    } else {
        geometry.into()
    };
    Ok(list!(
        pair = pair,
        option = option,
        departure_time = departure,
        total_duration = total_duration,
        total_distance = total_distance,
        segment = segment,
        mode = mode,
        segment_duration = segment_duration,
        wait = wait,
        distance = distance,
        route = route,
        geometry = geometry
    )
    .into())
}

extendr_module! {
    mod itinerary;
    fn rs_detailed_itineraries;
}
