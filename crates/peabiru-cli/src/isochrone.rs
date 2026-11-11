//! `peabiru isochrone`: polygon or line isochrones, as r5r's `isochrone()`,
//! for testing the core against r5r.

use crate::cli::{self, runtime_error, usage_error};
use peabiru_core::{
    build_network, isochrone_lines, isochrone_polygons, isochrone_trip_limit, street_edges,
    street_travel_time_matrix, street_travel_time_surface, transit_travel_time_matrix,
    transit_travel_time_surface, BuildOptions, Mode, RouteParams, TransitData, TransitParams,
};
use std::fmt::Write as _;

const CMD: &str = "isochrone";

pub const USAGE: &str = "peabiru isochrone <dir|file.osm.pbf> <points.csv> <out> \
     [--mode WALK|BICYCLE|CAR|TRANSIT] [--cutoffs 0,15,30] [--zoom 10] [--lines] \
     [--percentiles 50] [--surface <surface.csv>] [--edges <edges.csv>] [--date YYYY-MM-DD --time HH:MM:SS] [--tw 10] \
     [--max-walk N|inf] [--max-rides 3] [--draws 5] [--seed N] [--walk-speed 3.6] \
     [--bike-speed 12] [--max-lts 2] [--threads 0]";

/// A ring as WKT coordinates, every number in Rust's shortest round-trip form.
fn ring_wkt(out: &mut String, ring: &[[f64; 2]]) {
    out.push('(');
    for (i, [x, y]) in ring.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{x:?} {y:?}");
    }
    out.push(')');
}

pub fn run(args: &[String]) {
    let (net_path, points_path, out) = crate::positional3(CMD, args);
    let mut mode = "TRANSIT".to_string();
    let mut cutoffs: Vec<f64> = vec![0.0, 15.0, 30.0];
    let mut zoom = 10;
    let mut lines = false;
    let mut percentiles: Option<Vec<i32>> = None;
    let mut surface_path: Option<String> = None;
    let mut edges_path: Option<String> = None;
    let mut tp = TransitParams::default();
    let mut rp = RouteParams::default();
    let mut max_walk: Option<i32> = None;
    let (mut have_date, mut have_time) = (false, false);
    let mut i = 3;
    while i < args.len() {
        let name = args[i].as_str();
        if name == "--lines" {
            lines = true;
            i += 1;
            continue;
        }
        let v = cli::value(CMD, args, i);
        match name {
            "--mode" => mode = v.to_uppercase(),
            "--cutoffs" => cutoffs = cli::list(CMD, name, v),
            "--zoom" => zoom = cli::int(CMD, name, v, 9, 12),
            "--percentiles" => percentiles = Some(cli::list(CMD, name, v)),
            "--surface" => surface_path = Some(v.to_string()),
            "--edges" => edges_path = Some(v.to_string()),
            "--date" => {
                tp.date = cli::date(CMD, name, v);
                have_date = true;
            }
            "--time" => {
                tp.departure_time = cli::time_of_day(CMD, name, v);
                have_time = true;
            }
            "--tw" => tp.time_window = cli::count(CMD, name, v),
            "--max-walk" => {
                max_walk = if v.eq_ignore_ascii_case("inf") {
                    None
                } else {
                    Some(cli::count(CMD, name, v))
                }
            }
            "--max-rides" => tp.max_rides = cli::count(CMD, name, v) as usize,
            "--draws" => tp.draws_per_minute = cli::count(CMD, name, v),
            "--seed" => tp.seed = Some(cli::int(CMD, name, v, i32::MIN, i32::MAX)),
            "--walk-speed" => {
                tp.walk_speed_kmh = cli::float(CMD, name, v);
                rp.walk_speed_kmh = tp.walk_speed_kmh;
            }
            "--bike-speed" => rp.bike_speed_kmh = cli::float(CMD, name, v),
            "--max-lts" => rp.max_lts = cli::int(CMD, name, v, i32::MIN, i32::MAX),
            "--threads" => {
                tp.n_threads = cli::size(CMD, name, v);
                rp.n_threads = tp.n_threads;
            }
            other => usage_error(CMD, format!("unknown option '{other}'")),
        }
        i += 2;
    }
    let street_mode = match mode.as_str() {
        "WALK" => Some(Mode::Walk),
        "BICYCLE" => Some(Mode::Bicycle),
        "CAR" => Some(Mode::Car),
        "TRANSIT" | "WALK,TRANSIT" | "TRANSIT,WALK" => None,
        _ => usage_error(CMD, format!("unsupported mode '{mode}'")),
    };
    if street_mode.is_none() && !(have_date && have_time) {
        usage_error(CMD, "--date and --time are required with TRANSIT");
    }
    if lines && percentiles.is_some() {
        usage_error(CMD, "Percentiles not supported for line-based isochrones");
    }
    let percentiles = percentiles.unwrap_or_else(|| vec![50]);
    // r5r: max_trip_duration = as.integer(max(cutoffs)), plus 10 for polygons
    let mut limit = isochrone_trip_limit(&cutoffs).unwrap_or_else(|e| usage_error(CMD, e));
    if !lines {
        limit += 10;
    }
    // r5r assign_max_street_time / assign_max_trip_duration
    let walk_limit = max_walk.unwrap_or(limit);
    if street_mode == Some(Mode::Walk) && walk_limit < limit {
        limit = walk_limit;
    }
    let (ids, pts) = crate::read_points(CMD, points_path);
    let is_dir = std::path::Path::new(net_path).is_dir();
    let mut build = BuildOptions::default();
    build.n_threads = tp.n_threads;
    let fail_io = |e: peabiru_core::NetworkError| -> ! { runtime_error(CMD, e) };
    let (net, layer) = if is_dir {
        let (n, l) = cli::build_from_dir(CMD, net_path, &build).unwrap_or_else(|e| fail_io(e));
        (n, Some(l))
    } else {
        if street_mode.is_none() {
            usage_error(CMD, "TRANSIT needs a directory with GTFS feeds");
        }
        (
            build_network(net_path, &build).unwrap_or_else(|e| fail_io(e)),
            None,
        )
    };
    let data = layer
        .as_ref()
        .filter(|_| street_mode.is_none())
        .map(|l| TransitData::new(&net, l));
    rp.max_trip_duration_min = limit;
    tp.max_trip_duration = limit;
    tp.max_walk_time = walk_limit;
    tp.percentiles = percentiles.clone();
    let mut text = String::new();
    if lines {
        let edges = street_edges(&net);
        let dest = edges.centroid_points();
        if let Some(path) = edges_path {
            let mut s = String::from("edge_index,lon,lat\n");
            for (e, &(lat, lon)) in dest.iter().enumerate() {
                let _ = writeln!(s, "{},{lon:?},{lat:?}", edges.edge_index(e));
            }
            cli::write_file(CMD, &path, s);
        }
        let minutes: Vec<Option<u32>> = match street_mode {
            Some(m) => street_travel_time_matrix(&net, &pts, &dest, m, &rp)
                .unwrap_or_else(|e| usage_error(CMD, e))
                .into_vec(),
            None => {
                let (layer, data) = (layer.as_ref().unwrap(), data.as_ref().unwrap());
                transit_travel_time_matrix(&net, layer, data, &pts, &dest, &tp)
                    .unwrap_or_else(|e| usage_error(CMD, e))
                    .into_vec()
            }
        };
        let res = isochrone_lines(pts.len(), edges.len(), &minutes, &cutoffs)
            .unwrap_or_else(|e| usage_error(CMD, e));
        text.push_str("id,edge_index,osm_id,isochrone,travel_time_p50\n");
        for l in &res {
            let _ = writeln!(
                text,
                "{},{},{},{},{}",
                ids[l.origin],
                edges.edge_index(l.edge),
                edges.osm_id(l.edge),
                l.isochrone,
                l.travel_time
            );
        }
        cli::write_file(CMD, out, text);
        println!(
            "edges {}  rows {}  max_trip {limit}",
            edges.len(),
            res.len()
        );
        return;
    }
    let surface = match street_mode {
        Some(m) => street_travel_time_surface(&net, &pts, m, &rp, &percentiles, zoom),
        None => transit_travel_time_surface(
            &net,
            layer.as_ref().unwrap(),
            data.as_ref().unwrap(),
            &pts,
            &tp,
            zoom,
        ),
    }
    .unwrap_or_else(|e| usage_error(CMD, e));
    if let Some(path) = surface_path {
        let g = surface.grid();
        let mut s = format!(
            "zoom {} west {} north {} width {} height {}\n",
            g.zoom(),
            g.west(),
            g.north(),
            g.width(),
            g.height()
        );
        for (o, id) in ids.iter().enumerate() {
            for (k, p) in surface.percentiles().iter().enumerate() {
                let _ = write!(s, "{id},{p}");
                for m in surface.minutes(o, k) {
                    let _ = write!(s, ",{m}");
                }
                s.push('\n');
            }
        }
        cli::write_file(CMD, &path, s);
    }
    let polys = isochrone_polygons(&surface, &cutoffs).unwrap_or_else(|e| usage_error(CMD, e));
    text.push_str("id\tisochrone\tpercentile\tgeometry\n");
    for p in &polys {
        let _ = write!(
            text,
            "{}\t{}\tp{}\t",
            ids[p.origin], p.isochrone, p.percentile
        );
        if p.polygons.is_empty() {
            text.push_str("MULTIPOLYGON EMPTY\n");
            continue;
        }
        text.push_str("MULTIPOLYGON (");
        for (k, poly) in p.polygons.iter().enumerate() {
            if k > 0 {
                text.push_str(", ");
            }
            text.push('(');
            for (r, ring) in poly.iter().enumerate() {
                if r > 0 {
                    text.push_str(", ");
                }
                ring_wkt(&mut text, ring);
            }
            text.push(')');
        }
        text.push_str(")\n");
    }
    cli::write_file(CMD, out, text);
    let g = surface.grid();
    println!(
        "grid {}x{} at zoom {}  rows {}  max_trip {limit}",
        g.width(),
        g.height(),
        g.zoom(),
        polys.len()
    );
}
