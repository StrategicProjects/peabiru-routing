//! `trip_options()`: the core's options as plain vectors, one element per
//! option, per option and departure minute, and per leg (the geometry as
//! WKT text for `sf::st_as_sfc()`).

use super::{err, interruptible, network, points, street_mode, transit_modes, Outcome};
use extendr_api::prelude::*;
use extendr_api::Result;
use peabiru_core::{
    trip_options, write_trip_options_csv, Criterion, RouteError, TripOptionsParams,
};

/// `HH:MM:SS`, hours past 24 as they come.
fn clock(s: i32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// Options between `origins[i]` and `destinations[i]` as
/// `list(options = list(pair, option, total_time, fare, transfers,
/// walk_time, departure_time, arrival_time, n_departures, first_departure,
/// last_departure, description), departures = list(pair, option,
/// departure_time, arrival_time, total_time), legs = list(pair, option,
/// segment, mode, cumulative_fare, segment_duration, wait, distance, route,
/// osm_id_list, edge_id_list, board_stop_id, alight_stop_id, geometry))`
/// (`pair` 1-based; `fare` and `cumulative_fare` `NULL` without a fare
/// structure, the id columns `NULL` without `osm_link_ids`, `geometry`
/// `NULL` with `drop_geometry`, `legs` `NULL` without `itineraries`).
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_trip_options(
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
    criteria: Vec<String>,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_rides: i32,
    max_lts: i32,
    n_threads: i32,
    scenario: Robj,
    fare_structure: Robj,
    max_fare: f64,
    max_options: i32,
    itineraries: bool,
    drop_geometry: bool,
    osm_link_ids: bool,
    output: Robj,
    walk_resolution: i32,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let mut p = TripOptionsParams::default();
    p.scenario = crate::scenario::scenario_of(&scenario)?;
    p.mode = street_mode(mode)?;
    p.transit_modes = transit_modes(transit)?;
    if !p.transit_modes.is_empty() {
        p.egress_mode = street_mode(egress_mode)?;
    }
    p.date = date.max(0) as u32;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.criteria = criteria
        .iter()
        .map(|c| Criterion::from_name(c).ok_or_else(|| err(format!("unknown criterion '{c}'"))))
        .collect::<Result<_>>()?;
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_rides = max_rides.max(0) as usize;
    p.max_lts = max_lts;
    p.fare_structure = super::fares::fare_args(&fare_structure)?;
    p.max_fare = max_fare;
    p.max_options = max_options.max(0) as usize;
    p.walk_resolution_s = walk_resolution;
    p.itineraries = itineraries;
    p.osm_link_ids = osm_link_ids;
    p.n_threads = n_threads.max(0) as usize;
    let res = match interruptible(&p.cancel, || trip_options(&n.net, &o, &d, &p))? {
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
        write_trip_options_csv(&dir, &ids, &pts, &res, drop_geometry, osm_link_ids)
            .map_err(|e| err(RouteError::Invalid(format!("cannot write {dir}: {e}"))))?;
        return Ok(().into());
    }
    let with_fares = p.fare_structure.is_some();
    let nullable = |on: bool, v: Robj| -> Robj {
        if on {
            v
        } else {
            ().into()
        }
    };
    // options
    let (mut pair, mut option, mut total, mut fare, mut transfers, mut walk) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    let (mut dep, mut arr, mut n_dep, mut first, mut last, mut desc) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    // departures
    let (mut dpair, mut doption, mut ddep, mut darr, mut dtotal) =
        (vec![], vec![], vec![], vec![], vec![]);
    // legs
    let (mut lpair, mut loption, mut segment, mut lmode, mut cum) =
        (vec![], vec![], vec![], vec![], vec![]);
    let (mut seg_dur, mut wait, mut dist, mut route, mut geometry) =
        (vec![], vec![], vec![], vec![], vec![]);
    let mut links: [Vec<String>; 4] = Default::default();
    for (k, options) in res.iter().enumerate() {
        let pk = k as i32 + 1;
        for x in options {
            pair.push(pk);
            option.push(x.option as i32);
            total.push(x.total_time);
            fare.push(x.fare.unwrap_or(0.0));
            transfers.push(x.transfers as i32);
            walk.push(x.walk_time);
            dep.push(clock(x.departure_s));
            arr.push(clock(x.arrival_s));
            n_dep.push(x.n_departures() as i32);
            first.push(clock(x.departures[0]));
            last.push(clock(*x.departures.last().expect("a departure")));
            desc.push(x.description.clone());
            for ((&dp, &tt), &tm) in x.departures.iter().zip(&x.totals_s).zip(&x.totals) {
                dpair.push(pk);
                doption.push(x.option as i32);
                ddep.push(clock(dp));
                darr.push(clock(dp + tt));
                dtotal.push(tm);
            }
            if let Some(it) = &x.itinerary {
                for (j, l) in it.legs.iter().enumerate() {
                    lpair.push(pk);
                    loption.push(x.option as i32);
                    segment.push(j as i32 + 1);
                    lmode.push(l.mode.clone());
                    cum.push(l.cumulative_fare.unwrap_or(0.0));
                    seg_dur.push(l.segment_duration);
                    wait.push(l.wait);
                    dist.push(l.distance);
                    route.push(l.route.clone());
                    if osm_link_ids {
                        links[0].push(l.osm_id_list());
                        links[1].push(l.edge_id_list());
                        links[2].push(l.board_stop_id.clone());
                        links[3].push(l.alight_stop_id.clone());
                    }
                    if !drop_geometry {
                        geometry.push(l.wkt());
                    }
                }
            }
        }
    }
    let options = list!(
        pair = pair,
        option = option,
        total_time = total,
        fare = nullable(with_fares, fare.into()),
        transfers = transfers,
        walk_time = walk,
        departure_time = dep,
        arrival_time = arr,
        n_departures = n_dep,
        first_departure = first,
        last_departure = last,
        description = desc
    );
    let departures = list!(
        pair = dpair,
        option = doption,
        departure_time = ddep,
        arrival_time = darr,
        total_time = dtotal
    );
    let [osm, edges, board, alight] = links.map(|v| nullable(osm_link_ids, v.into()));
    let legs: Robj = if itineraries {
        list!(
            pair = lpair,
            option = loption,
            segment = segment,
            mode = lmode,
            cumulative_fare = nullable(with_fares, cum.into()),
            segment_duration = seg_dur,
            wait = wait,
            distance = dist,
            route = route,
            osm_id_list = osm,
            edge_id_list = edges,
            board_stop_id = board,
            alight_stop_id = alight,
            geometry = nullable(!drop_geometry, geometry.into())
        )
        .into()
    } else {
        ().into()
    };
    Ok(list!(options = options, departures = departures, legs = legs).into())
}

extendr_module! {
    mod options;
    fn rs_trip_options;
}
