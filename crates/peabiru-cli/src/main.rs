mod accessibility;
mod cli;
mod expanded;

use cli::{runtime_error, usage_error};
use peabiru_core::osm::pbf::{read_pbf_file, tag, ReadOptions};
use std::time::Instant;

fn usage_text() -> String {
    format!(
        "usage: peabiru --version\n       peabiru osm-stats <file.osm.pbf> [n_threads]\n       \
         peabiru ttm <file.osm.pbf> <points.csv> <WALK|BICYCLE|CAR> <out.csv> [n_threads] [repeats]\n       \
         peabiru transit <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS [--tw 10] [--percentiles 50] [--max-walk 120] [--max-trip 120] [--max-rides 3] [--draws 5] [--seed N] [--threads 0] [--repeats 1]\n       \
         peabiru accessibility <dir|file.osm.pbf> <points.csv> <out.csv> --opportunities a,b [options]\n       \
         peabiru dump-edges <file.osm.pbf>   (CSV like r5r::street_network_to_sf edges)\n       \
         {}",
        expanded::USAGE
    )
}

/// Usage on stderr, exit status 2.
fn usage() -> ! {
    eprintln!("{}", usage_text());
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if cli::wants_help(&args) || args.first().map(String::as_str) == Some("help") {
        println!("{}", usage_text());
        return;
    }
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => println!("peabiru {}", peabiru_core::VERSION),
        Some("osm-stats") => {
            let cmd = "osm-stats";
            let path = args
                .get(1)
                .unwrap_or_else(|| usage_error(cmd, "expected <file.osm.pbf> [n_threads]"));
            if let Some(extra) = args.get(3) {
                usage_error(cmd, format!("unexpected argument '{extra}'"));
            }
            let n_threads = args.get(2).map_or(0, |s| cli::size(cmd, "n_threads", s));
            osm_stats(path, n_threads);
        }
        Some("ttm") => {
            let cmd = "ttm";
            if args.len() < 5 {
                usage_error(
                    cmd,
                    "expected <network> <points.csv> <WALK|BICYCLE|CAR> <out.csv> [n_threads] [repeats]",
                );
            }
            if let Some(extra) = args.get(7) {
                usage_error(cmd, format!("unexpected argument '{extra}'"));
            }
            let n_threads = args.get(5).map_or(0, |s| cli::size(cmd, "n_threads", s));
            let repeats = args
                .get(6)
                .map_or(1, |s| cli::count(cmd, "repeats", s) as usize);
            ttm(&args[1], &args[2], &args[3], &args[4], n_threads, repeats);
        }
        Some("transit") => transit(&args[1..]),
        Some("accessibility") => accessibility::run(&args[1..]),
        Some("expanded") => expanded::run(&args[1..]),
        Some("dump-edges") => {
            let path = args
                .get(1)
                .unwrap_or_else(|| usage_error("dump-edges", "expected <file.osm.pbf>"));
            dump_edges(path)
        }
        Some(other) => usage_error(other, "unknown command (peabiru --help lists them)"),
        None => usage(),
    }
}

/// The three positional arguments `<network> <points.csv> <out.csv>` of the
/// routing commands.
pub(crate) fn positional3<'a>(cmd: &str, args: &'a [String]) -> (&'a str, &'a str, &'a str) {
    if args.len() < 3 || args[..3].iter().any(|a| a.starts_with("--")) {
        usage_error(cmd, "expected <network> <points.csv> <out.csv> [options]");
    }
    (&args[0], &args[1], &args[2])
}

/// `peabiru transit <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS
///  [--tw 10] [--percentiles 50] [--max-walk 120] [--max-trip 120] [--max-rides 3]
///  [--draws 5] [--seed N] [--threads 0] [--repeats 1]`
fn transit(args: &[String]) {
    use peabiru_core::street::BuildOptions;
    use peabiru_core::transit::matrix::{travel_time_matrix, TransitData, TransitParams};
    let cmd = "transit";
    let (dir, points, out) = positional3(cmd, args);
    let mut p = TransitParams::default();
    let mut repeats = 1usize;
    let (mut have_date, mut have_time) = (false, false);
    let mut i = 3;
    while i < args.len() {
        let name = args[i].as_str();
        let v = cli::value(cmd, args, i);
        match name {
            "--date" => {
                p.date = cli::date(cmd, name, v);
                have_date = true;
            }
            "--time" => {
                p.departure_time = cli::time_of_day(cmd, name, v);
                have_time = true;
            }
            "--tw" => p.time_window = cli::count(cmd, name, v),
            "--percentiles" => p.percentiles = cli::list(cmd, name, v),
            "--max-walk" => p.max_walk_time = cli::count(cmd, name, v),
            "--max-trip" => p.max_trip_duration = cli::count(cmd, name, v),
            "--max-rides" => p.max_rides = cli::count(cmd, name, v) as usize,
            "--draws" => p.draws_per_minute = cli::count(cmd, name, v),
            "--seed" => p.seed = Some(cli::int(cmd, name, v, i32::MIN, i32::MAX)),
            "--threads" => p.n_threads = cli::size(cmd, name, v),
            "--repeats" => repeats = cli::count(cmd, name, v) as usize,
            _ => usage_error(cmd, format!("unknown option '{name}'")),
        }
        i += 2;
    }
    if !(have_date && have_time) {
        usage_error(cmd, "--date and --time are required");
    }
    let (ids, pts) = read_points(cmd, points);
    let t0 = Instant::now();
    let (net, layer) = cli::build_from_dir(
        cmd,
        dir,
        &BuildOptions {
            n_threads: p.n_threads,
        },
    )
    .unwrap_or_else(|e| runtime_error(cmd, e));
    let data = TransitData::new(&net, &layer);
    let build_s = t0.elapsed().as_secs_f64();
    let mut times = Vec::new();
    let mut m = Vec::new();
    for _ in 0..repeats {
        let t = Instant::now();
        m = travel_time_matrix(&net, &layer, &data, &pts, &pts, &p)
            .unwrap_or_else(|e| usage_error(cmd, e));
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let mut pcs = p.percentiles.clone();
    pcs.sort_unstable();
    let mut csv = String::from("from_id,to_id");
    for pc in &pcs {
        csv.push_str(&format!(",travel_time_p{pc:02}"));
    }
    csv.push('\n');
    let mut rows = 0usize;
    for (i, row) in m.iter().enumerate() {
        for (j, cell) in row.iter().enumerate() {
            if cell[0].is_none() {
                continue;
            }
            rows += 1;
            csv.push_str(&format!("{},{}", ids[i], ids[j]));
            for v in cell {
                match v {
                    Some(v) => csv.push_str(&format!(",{v}")),
                    None => csv.push(','),
                }
            }
            csv.push('\n');
        }
    }
    cli::write_file(cmd, out, csv);
    println!(
        "stops {}  patterns {}  build {:.3}s  route {:.3}s (median of {})  rows {}",
        layer.stop_ids.len(),
        layer.patterns.len(),
        build_s,
        times[times.len() / 2],
        times.len(),
        rows
    );
}

/// Every directed edge with the attributes r5r's `street_network_to_sf()`
/// reports, for comparing networks edge by edge (bench/compare/edges.py).
fn dump_edges(pbf: &str) {
    use peabiru_core::street::flags::*;
    use peabiru_core::street::labels::{cms_to_ms, lts_of};
    use peabiru_core::street::{build_network, BuildOptions};
    use std::io::Write;
    let cmd = "dump-edges";
    let net =
        build_network(pbf, &BuildOptions::default()).unwrap_or_else(|e| runtime_error(cmd, e));
    let out = std::io::stdout();
    let mut w = std::io::BufWriter::new(out.lock());
    let fail = |e: std::io::Error| -> ! { runtime_error(cmd, format!("cannot write output: {e}")) };
    writeln!(
        w,
        "edge_index,osm_id,length,walk,car,car_speed,bicycle,bicycle_lts,lon0,lat0,lon1,lat1"
    )
    .unwrap_or_else(|e| fail(e));
    for e in 0..net.n_edges() {
        let f = net.edge_flags[e];
        let (v0, v1) = (net.edge_from(e), net.edge_to(e));
        writeln!(
            w,
            "{},{},{:.3},{},{},{},{},{},{:.7},{:.7},{:.7},{:.7}",
            e,
            net.pair_way[e / 2],
            f64::from(net.pair_length_mm[e / 2]) / 1000.0,
            f & ALLOWS_PEDESTRIAN != 0,
            f & ALLOWS_CAR != 0,
            f64::from(cms_to_ms(net.edge_speed_cms[e])) * 3.6,
            f & ALLOWS_BIKE != 0,
            lts_of(f),
            f64::from(net.v_lon[v0]) / 1e7,
            f64::from(net.v_lat[v0]) / 1e7,
            f64::from(net.v_lon[v1]) / 1e7,
            f64::from(net.v_lat[v1]) / 1e7
        )
        .unwrap_or_else(|e| fail(e));
    }
    w.flush().unwrap_or_else(|e| fail(e));
}

/// Read `id,lon,lat,...` points (header required; columns found by name).
/// A file that cannot be read or parsed ends the process with status 1 and
/// a message naming the file and line.
pub(crate) fn read_points(cmd: &str, path: &str) -> (Vec<String>, Vec<(f64, f64)>) {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| runtime_error(cmd, format!("cannot read {path}: {e}")));
    let mut lines = text.lines().enumerate();
    let header: Vec<&str> = lines.next().map_or("", |(_, l)| l).split(',').collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h.trim_matches('"') == name)
            .unwrap_or_else(|| runtime_error(cmd, format!("{path}:1: no '{name}' column")))
    };
    let (ci, clon, clat) = (col("id"), col("lon"), col("lat"));
    let need = ci.max(clon).max(clat) + 1;
    let mut ids = Vec::new();
    let mut pts = Vec::new();
    for (n, line) in lines.filter(|(_, l)| !l.is_empty()) {
        let line_no = n + 1;
        let f: Vec<&str> = line.split(',').collect();
        if f.len() < need {
            runtime_error(
                cmd,
                format!(
                    "{path}:{line_no}: expected at least {need} fields, got {}",
                    f.len()
                ),
            );
        }
        ids.push(f[ci].trim_matches('"').to_string());
        let num = |name: &str, s: &str| {
            s.trim_matches('"').parse::<f64>().unwrap_or_else(|_| {
                runtime_error(
                    cmd,
                    format!("{path}:{line_no}: {name} '{s}' is not a number"),
                )
            })
        };
        pts.push((num("lat", f[clat]), num("lon", f[clon])));
    }
    (ids, pts)
}

fn ttm(pbf: &str, points: &str, mode: &str, out: &str, n_threads: usize, repeats: usize) {
    let cmd = "ttm";
    // A directory is a network folder like r5r's: one .pbf plus GTFS zips.
    let is_dir = std::path::Path::new(pbf).is_dir();
    use peabiru_core::street::{
        build_network, travel_time_matrix, BuildOptions, Mode, RouteParams,
    };
    let mode = match mode {
        "WALK" => Mode::Walk,
        "BICYCLE" => Mode::Bicycle,
        "CAR" => Mode::Car,
        _ => usage_error(
            cmd,
            format!("mode must be WALK, BICYCLE or CAR, got '{mode}'"),
        ),
    };
    let (ids, pts) = read_points(cmd, points);
    let t0 = Instant::now();
    let net = if is_dir {
        let (net, layer) = cli::build_from_dir(cmd, pbf, &BuildOptions { n_threads })
            .unwrap_or_else(|e| runtime_error(cmd, e));
        let linked = layer.stop_vertex.iter().filter(|&&v| v >= 0).count();
        eprintln!(
            "feeds {:?}  stops {} (linked {})  patterns {}  frequencies {}",
            layer.feed_ids,
            layer.stop_ids.len(),
            linked,
            layer.patterns.len(),
            layer.has_frequencies
        );
        net
    } else {
        build_network(pbf, &BuildOptions { n_threads }).unwrap_or_else(|e| runtime_error(cmd, e))
    };
    let build_s = t0.elapsed().as_secs_f64();
    let params = RouteParams {
        n_threads,
        ..RouteParams::default()
    };
    let mut times = Vec::new();
    let mut m = Vec::new();
    for _ in 0..repeats {
        let t = Instant::now();
        m = travel_time_matrix(&net, &pts, &pts, mode, &params)
            .unwrap_or_else(|e| usage_error(cmd, e));
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let route_s = times[times.len() / 2];
    let n = pts.len();
    let mut csv = String::from("from_id,to_id,travel_time_p50\n");
    let mut reached = 0usize;
    for i in 0..n {
        for j in 0..n {
            if let Some(t) = m[i * n + j] {
                reached += 1;
                csv.push_str(&format!("{},{},{}\n", ids[i], ids[j], t));
            }
        }
    }
    cli::write_file(cmd, out, csv);
    println!(
        "vertices {}  edges {}  restrictions {}  build {:.3}s  route {:.3}s (median of {})  pairs {}  reached {}",
        net.n_vertices(),
        net.n_edges(),
        net.restrictions.len(),
        build_s,
        route_s,
        times.len(),
        n * n,
        reached
    );
}

fn osm_stats(path: &str, n_threads: usize) {
    let highways = |t: &[(&str, &str)]| tag(t, "highway").is_some();
    let restrictions = |t: &[(&str, &str)]| tag(t, "type") == Some("restriction");
    let opts = ReadOptions {
        way_filter: &highways,
        relation_filter: Some(&restrictions),
        node_tag_filter: None,
        ref_count_filter: None,
        poi_filter: None,
        n_threads,
    };
    let t0 = Instant::now();
    let ex = read_pbf_file(path, &opts).unwrap_or_else(|e| runtime_error("osm-stats", e));
    let secs = t0.elapsed().as_secs_f64();
    let missing = ex.lat.iter().filter(|&&v| v == i32::MIN).count();
    let lat_sum: i64 = ex
        .lat
        .iter()
        .filter(|&&v| v != i32::MIN)
        .map(|&v| i64::from(v))
        .sum();
    let lon_sum: i64 = ex
        .lon
        .iter()
        .filter(|&&v| v != i32::MIN)
        .map(|&v| i64::from(v))
        .sum();
    let refs: usize = ex.ways.iter().map(|w| w.nodes.len()).sum();
    println!(
        "highway ways {}  node refs {}  unique nodes {}  missing coords {}  restrictions {}  lat_sum {}  lon_sum {}  read {:.3}s",
        ex.ways.len(),
        refs,
        ex.node_ids.len(),
        missing,
        ex.relations.len(),
        lat_sum,
        lon_sum,
        secs
    );
}
