//! `trip_options()`: the core's options as flat columns, one element per
//! option, per option and departure minute, and per leg (the lines as `x`,
//! `y` and the points of each leg).

use super::{
    f64_bytes, i32_bytes, interruptible, parse_transit_modes, points, street_mode,
    transit_street_mode, value_err, Network,
};
use peabiru_core::{trip_options, write_trip_options_csv, Criterion, TripOptionsParams};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// `HH:MM:SS`, hours past 24 as they come.
fn clock(s: i32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Options between `origins[i]` and `destinations[i]` as a dict of three
/// dicts of columns: `options` (`pair`, `option`, `transfers`,
/// `n_departures` as int32 bytes; `total_time`, `fare`, `walk_time` as
/// float64 bytes; `departure_time`, `arrival_time`, `first_departure`,
/// `last_departure`, `description` as lists), `departures` (`pair`,
/// `option` int32; `total_time` float64; `departure_time`, `arrival_time`
/// lists) and, with `itineraries`, `legs` (`pair`, `option`, `segment`,
/// `distance`, `n_points` int32; `cumulative_fare`, `segment_duration`,
/// `wait`, `x`, `y` float64; `mode`, `route` lists, and with
/// `osm_link_ids` the four id lists). With `output` (`(dir, from ids, to
/// ids)`) the files are written instead and `None` returned.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
#[pyo3(signature = (network, olat, olon, dlat, dlon, mode, transit_modes, egress_mode, date,
    departure_time, time_window, criteria, max_walk_time, max_bike_time, max_car_time,
    max_trip_duration, walk_speed, bike_speed, max_rides, max_lts, n_threads, scenario, fare,
    max_fare, max_options, itineraries, drop_geometry, osm_link_ids, output, walk_resolution=60))]
pub(crate) fn trip_options_py<'py>(
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
    criteria: Vec<String>,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_rides: usize,
    max_lts: i32,
    n_threads: usize,
    scenario: Option<Bound<'py, PyDict>>,
    fare: Option<Bound<'py, PyDict>>,
    max_fare: f64,
    max_options: usize,
    itineraries: bool,
    drop_geometry: bool,
    osm_link_ids: bool,
    output: Option<(String, Vec<String>, Vec<String>)>,
    walk_resolution: i32,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let n = network.get();
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let mut p = TripOptionsParams::default();
    p.scenario = crate::scenario::scenario_of(scenario.as_ref())?;
    p.mode = street_mode(mode)?;
    p.transit_modes = parse_transit_modes(transit_modes)?;
    if !p.transit_modes.is_empty() {
        p.egress_mode = transit_street_mode(egress_mode)?;
    }
    p.date = date;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.criteria = criteria
        .iter()
        .map(|c| {
            Criterion::from_name(c).ok_or_else(|| value_err(format!("unknown criterion '{c}'")))
        })
        .collect::<PyResult<_>>()?;
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_rides = max_rides;
    p.max_lts = max_lts;
    p.fare_structure = super::fares::fare_arg(fare.as_ref())?;
    p.max_fare = max_fare;
    p.max_options = max_options;
    p.walk_resolution_s = walk_resolution;
    p.itineraries = itineraries;
    p.osm_link_ids = osm_link_ids;
    p.n_threads = n_threads;
    let transit = n.transit.as_ref().map(|t| (&t.layer, &t.data));
    let res = interruptible(py, &p.cancel, || trip_options(&n.net, transit, &o, &d, &p))?;
    if let Some((dir, from, to)) = output {
        let ids: Vec<(String, String)> = from.into_iter().zip(to).collect();
        let pts: Vec<_> = o.iter().copied().zip(d.iter().copied()).collect();
        write_trip_options_csv(&dir, &ids, &pts, &res, drop_geometry, osm_link_ids)
            .map_err(|e| value_err(format!("cannot write {dir}: {e}")))?;
        return Ok(None);
    }
    let (mut pair, mut option, mut transfers, mut n_dep) = (vec![], vec![], vec![], vec![]);
    let (mut total, mut fares, mut walk) = (vec![], vec![], vec![]);
    let (mut dep, mut arr, mut first, mut last, mut desc) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut dpair, mut doption, mut dtotal, mut ddep, mut darr) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut lpair, mut loption, mut segment, mut distance, mut n_points) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut cum, mut seg_dur, mut wait, mut x, mut y) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut lmode, mut route) = (vec![], vec![]);
    let mut links: [Vec<String>; 4] = Default::default();
    for (k, options) in res.iter().enumerate() {
        let pk = k as i32;
        for t in options {
            pair.push(pk);
            option.push(t.option as i32);
            transfers.push(t.transfers as i32);
            n_dep.push(t.n_departures() as i32);
            total.push(t.total_time);
            fares.push(t.fare.unwrap_or(0.0));
            walk.push(t.walk_time);
            dep.push(clock(t.departure_s));
            arr.push(clock(t.arrival_s));
            first.push(clock(t.departures[0]));
            last.push(clock(*t.departures.last().expect("a departure")));
            desc.push(t.description.clone());
            for ((&dp, &tt), &tm) in t.departures.iter().zip(&t.totals_s).zip(&t.totals) {
                dpair.push(pk);
                doption.push(t.option as i32);
                dtotal.push(tm);
                ddep.push(clock(dp));
                darr.push(clock(dp + tt));
            }
            if let Some(it) = &t.itinerary {
                for (j, l) in it.legs.iter().enumerate() {
                    lpair.push(pk);
                    loption.push(t.option as i32);
                    segment.push(j as i32 + 1);
                    distance.push(l.distance);
                    cum.push(l.cumulative_fare.unwrap_or(0.0));
                    seg_dur.push(l.segment_duration);
                    wait.push(l.wait);
                    lmode.push(l.mode.clone());
                    route.push(l.route.clone());
                    if !drop_geometry {
                        n_points.push(l.geometry.len() as i32);
                        for c in &l.geometry {
                            x.push(c[0]);
                            y.push(c[1]);
                        }
                    }
                    if osm_link_ids {
                        links[0].push(l.osm_id_list());
                        links[1].push(l.edge_id_list());
                        links[2].push(l.board_stop_id.clone());
                        links[3].push(l.alight_stop_id.clone());
                    }
                }
            }
        }
    }
    let opts = PyDict::new(py);
    opts.set_item("pair", i32_bytes(py, &pair)?)?;
    opts.set_item("option", i32_bytes(py, &option)?)?;
    opts.set_item("total_time", f64_bytes(py, &total)?)?;
    opts.set_item("fare", f64_bytes(py, &fares)?)?;
    opts.set_item("transfers", i32_bytes(py, &transfers)?)?;
    opts.set_item("walk_time", f64_bytes(py, &walk)?)?;
    opts.set_item("departure_time", dep)?;
    opts.set_item("arrival_time", arr)?;
    opts.set_item("n_departures", i32_bytes(py, &n_dep)?)?;
    opts.set_item("first_departure", first)?;
    opts.set_item("last_departure", last)?;
    opts.set_item("description", desc)?;
    let deps = PyDict::new(py);
    deps.set_item("pair", i32_bytes(py, &dpair)?)?;
    deps.set_item("option", i32_bytes(py, &doption)?)?;
    deps.set_item("departure_time", ddep)?;
    deps.set_item("arrival_time", darr)?;
    deps.set_item("total_time", f64_bytes(py, &dtotal)?)?;
    let out = PyDict::new(py);
    out.set_item("options", opts)?;
    out.set_item("departures", deps)?;
    if itineraries {
        let legs = PyDict::new(py);
        legs.set_item("pair", i32_bytes(py, &lpair)?)?;
        legs.set_item("option", i32_bytes(py, &loption)?)?;
        legs.set_item("segment", i32_bytes(py, &segment)?)?;
        legs.set_item("mode", lmode)?;
        legs.set_item("cumulative_fare", f64_bytes(py, &cum)?)?;
        legs.set_item("segment_duration", f64_bytes(py, &seg_dur)?)?;
        legs.set_item("wait", f64_bytes(py, &wait)?)?;
        legs.set_item("distance", i32_bytes(py, &distance)?)?;
        legs.set_item("route", route)?;
        legs.set_item("n_points", i32_bytes(py, &n_points)?)?;
        legs.set_item("x", f64_bytes(py, &x)?)?;
        legs.set_item("y", f64_bytes(py, &y)?)?;
        if osm_link_ids {
            let [a, b, c, e] = links;
            legs.set_item("osm_id_list", a)?;
            legs.set_item("edge_id_list", b)?;
            legs.set_item("board_stop_id", c)?;
            legs.set_item("alight_stop_id", e)?;
        }
        out.set_item("legs", legs)?;
    }
    Ok(Some(out))
}
