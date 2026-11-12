//! Scenarios, as r5r's `new_carspeeds`, `carspeed_scale` and `new_lts`:
//! the car speeds and the levels of traffic stress of the streets changed
//! for one call, without rebuilding the network.
//!
//! ```sh
//! cargo run --release -p peabiru-core --example scenarios [network_dir]
//! ```

mod common;

use peabiru_core::{
    build_from_dir, street_edges, street_travel_time_matrix, BuildOptions, CarSpeeds, LtsChange,
    Mode, Network, RouteParams, Scenario, SpeedPolygon,
};

/// Sum of the travel times from the first point to every point, minutes.
fn total(
    net: &Network,
    pts: &[(f64, f64)],
    mode: Mode,
    scenario: Scenario,
) -> Result<u32, peabiru_core::RouteError> {
    let mut p = RouteParams::default();
    p.n_threads = 2;
    p.scenario = scenario;
    let m = street_travel_time_matrix(net, &pts[..1], pts, mode, &p)?;
    Ok(m.row(0).iter().flatten().sum())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = common::data_dir();
    let pts = common::read_points(&dir, &[]).coords;
    let mut opts = BuildOptions::default();
    opts.n_threads = 2;
    let net = build_from_dir(&dir, &opts)?;
    let edges = street_edges(&net);

    println!(
        "car, as built:          {:>4} min",
        total(&net, &pts, Mode::Car, Scenario::default())?
    );

    // carspeed_scale: every car speed at 70%.
    let mut s = Scenario::default();
    s.car_speed_scale = 0.7;
    println!(
        "car, all speeds x 0.7:  {:>4} min",
        total(&net, &pts, Mode::Car, s)?
    );

    // new_carspeeds by OSM way: the primary streets at 20 km/h.
    let mut primary: Vec<i64> = (0..edges.len())
        .filter(|&i| edges.street_class(i) == "PRIMARY")
        .map(|i| edges.osm_id(i))
        .collect();
    primary.sort_unstable();
    primary.dedup();
    let mut s = Scenario::default();
    s.car_speeds = CarSpeeds::KmhByWay(primary.iter().map(|&w| (w, 20.0)).collect());
    println!(
        "car, primary 20 km/h:   {:>4} min ({} ways)",
        total(&net, &pts, Mode::Car, s)?,
        primary.len()
    );

    // new_carspeeds by polygon: half speed in a box around the first point
    // (a polygon is a shell then its holes, [lon, lat], closed rings).
    let (lat, lon) = pts[0];
    let (dx, dy) = (0.005, 0.005);
    let ring = vec![
        [lon - dx, lat - dy],
        [lon + dx, lat - dy],
        [lon + dx, lat + dy],
        [lon - dx, lat + dy],
        [lon - dx, lat - dy],
    ];
    let mut s = Scenario::default();
    s.car_speeds = CarSpeeds::ByPolygon(vec![SpeedPolygon::new(vec![vec![ring]], 0.5, 1.0)]);
    println!(
        "car, x 0.5 near origin: {:>4} min",
        total(&net, &pts, Mode::Car, s)?
    );

    // new_lts by OSM way: every street of stress 3 or 4 calmed to 2, so a
    // cyclist with max_lts = 2 (the default) rides on it instead of walking
    // the bike.
    let mut stressful: Vec<i64> = (0..edges.len())
        .filter(|&i| edges.bicycle_lts(i) > 2)
        .map(|i| edges.osm_id(i))
        .collect();
    stressful.sort_unstable();
    stressful.dedup();
    println!(
        "bicycle, as built:      {:>4} min",
        total(&net, &pts, Mode::Bicycle, Scenario::default())?
    );
    let mut s = Scenario::default();
    s.lts = LtsChange::ByWay(stressful.iter().map(|&w| (w, 2)).collect());
    println!(
        "bicycle, LTS 3-4 to 2:  {:>4} min ({} ways)",
        total(&net, &pts, Mode::Bicycle, s)?,
        stressful.len()
    );
    Ok(())
}
