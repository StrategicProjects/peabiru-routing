//! `detailed_itineraries()`: the core's itineraries as flat columns, one
//! element per leg; the lines as `x`, `y` and the points of each leg.

use super::{
    f64_bytes, i32_bytes, interruptible, parse_transit_modes, points, street_mode,
    transit_street_mode, value_err, Network,
};
use peabiru_core::{detailed_itineraries, write_itineraries_csv, ItineraryParams};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Itineraries between `origins[i]` and `destinations[i]` as a dict of
/// columns (`pair`, `option`, `total_distance`, `segment`, `distance`,
/// `n_points` as int32 bytes, `total_duration`, `segment_duration`,
/// `wait`, `x`, `y` as float64 bytes, `departure_time`, `mode`, `route` as
/// lists; `x`, `y` and `n_points` empty with `drop_geometry`; with
/// `osm_link_ids` also `osm_id_list`, `edge_id_list`, `board_stop_id`,
/// `alight_stop_id` as lists). With
/// `output` (`(dir, from ids, to ids)`) the files are written instead (the
/// geometry as JTS's WKT unless `drop_geometry`) and `None` returned.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (network, olat, olon, dlat, dlon, mode, transit_modes, egress_mode, date,
    departure_time, time_window, max_walk_time, max_bike_time, max_car_time,
    max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, shortest_path,
    n_threads, drop_geometry, osm_link_ids, output))]
pub(crate) fn itineraries<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    dlat: &[u8],
    dlon: &[u8],
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
    n_threads: usize,
    drop_geometry: bool,
    osm_link_ids: bool,
    output: Option<(String, Vec<String>, Vec<String>)>,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let n = network.get();
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let mut p = ItineraryParams::default();
    p.mode = street_mode(mode)?;
    p.transit_modes = parse_transit_modes(transit_modes)?;
    if !p.transit_modes.is_empty() {
        p.egress_mode = transit_street_mode(egress_mode)?;
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
    p.n_threads = n_threads;
    p.osm_link_ids = osm_link_ids;
    let transit = n.transit.as_ref().map(|t| (&t.layer, &t.data));
    let res = interruptible(py, &p.cancel, || {
        detailed_itineraries(&n.net, transit, &o, &d, &p)
    })?;
    if let Some((dir, from, to)) = output {
        let ids: Vec<(String, String)> = from.into_iter().zip(to).collect();
        let pts: Vec<_> = o.iter().copied().zip(d.iter().copied()).collect();
        write_itineraries_csv(&dir, &ids, &pts, &res, drop_geometry, osm_link_ids)
            .map_err(|e| value_err(format!("cannot write {dir}: {e}")))?;
        return Ok(None);
    }
    let (mut pair, mut option, mut total_distance, mut segment, mut distance, mut n_points) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let (mut total_duration, mut segment_duration, mut wait, mut x, mut y) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut departure, mut modes, mut route) = (Vec::new(), Vec::new(), Vec::new());
    let mut links: [Vec<String>; 4] = Default::default();
    for (k, its) in res.iter().enumerate() {
        for it in its {
            for (j, l) in it.legs.iter().enumerate() {
                pair.push(k as i32);
                option.push(it.option as i32);
                total_distance.push(it.total_distance);
                segment.push(j as i32 + 1);
                distance.push(l.distance);
                n_points.push(l.geometry.len() as i32);
                total_duration.push(it.total_duration);
                segment_duration.push(l.segment_duration);
                wait.push(l.wait);
                for c in &l.geometry {
                    x.push(c[0]);
                    y.push(c[1]);
                }
                departure.push(it.departure_time.clone());
                modes.push(l.mode.clone());
                route.push(l.route.clone());
                if osm_link_ids {
                    links[0].push(l.osm_id_list());
                    links[1].push(l.edge_id_list());
                    links[2].push(l.board_stop_id.clone());
                    links[3].push(l.alight_stop_id.clone());
                }
            }
        }
    }
    let out = PyDict::new(py);
    out.set_item("pair", i32_bytes(py, &pair)?)?;
    out.set_item("option", i32_bytes(py, &option)?)?;
    out.set_item("total_distance", i32_bytes(py, &total_distance)?)?;
    out.set_item("segment", i32_bytes(py, &segment)?)?;
    out.set_item("distance", i32_bytes(py, &distance)?)?;
    out.set_item("n_points", i32_bytes(py, &n_points)?)?;
    out.set_item("total_duration", f64_bytes(py, &total_duration)?)?;
    out.set_item("segment_duration", f64_bytes(py, &segment_duration)?)?;
    out.set_item("wait", f64_bytes(py, &wait)?)?;
    out.set_item("x", f64_bytes(py, &x)?)?;
    out.set_item("y", f64_bytes(py, &y)?)?;
    out.set_item("departure_time", departure)?;
    out.set_item("mode", modes)?;
    out.set_item("route", route)?;
    if osm_link_ids {
        let [a, b, c, e] = links;
        out.set_item("osm_id_list", a)?;
        out.set_item("edge_id_list", b)?;
        out.set_item("board_stop_id", c)?;
        out.set_item("alight_stop_id", e)?;
    }
    Ok(Some(out))
}
