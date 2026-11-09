mod expanded;

use peabiru_core::osm::pbf::{read_pbf_file, tag, ReadOptions};
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage: peabiru --version\n       peabiru osm-stats <file.osm.pbf> [n_threads]\n       \
         peabiru ttm <file.osm.pbf> <points.csv> <WALK|BICYCLE|CAR> <out.csv> [n_threads] [repeats]\n       \
         peabiru dump-edges <file.osm.pbf>   (CSV like r5r::street_network_to_sf edges)\n       \
         {}",
        expanded::USAGE
    );
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => println!("peabiru {}", peabiru_core::VERSION),
        Some("osm-stats") => {
            let path = args.get(1).unwrap_or_else(|| usage());
            let n_threads = args
                .get(2)
                .map_or(0, |s| s.parse().unwrap_or_else(|_| usage()));
            osm_stats(path, n_threads);
        }
        Some("ttm") => {
            if args.len() < 5 {
                usage();
            }
            let n_threads = args
                .get(5)
                .map_or(0, |s| s.parse().unwrap_or_else(|_| usage()));
            let repeats = args
                .get(6)
                .map_or(1, |s| s.parse().unwrap_or_else(|_| usage()));
            ttm(&args[1], &args[2], &args[3], &args[4], n_threads, repeats);
        }
        Some("transit") => transit(&args[1..]),
        Some("expanded") => expanded::run(&args[1..]),
        Some("dump-edges") => dump_edges(args.get(1).unwrap_or_else(|| usage())),
        _ => usage(),
    }
}

/// `peabiru transit <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS
///  [--tw 10] [--percentiles 50] [--max-walk 120] [--max-trip 120] [--max-rides 3]
///  [--draws 5] [--seed N] [--threads 0] [--repeats 1]`
fn transit(args: &[String]) {
    use peabiru_core::street::BuildOptions;
    use peabiru_core::transit::matrix::{travel_time_matrix, TransitData, TransitParams};
    if args.len() < 3 {
        usage();
    }
    let (dir, points, out) = (&args[0], &args[1], &args[2]);
    let mut p = TransitParams::default();
    let mut repeats = 1usize;
    let mut i = 3;
    let num =
        |v: Option<&String>| -> i64 { v.and_then(|s| s.parse().ok()).unwrap_or_else(|| usage()) };
    while i < args.len() {
        let v = args.get(i + 1);
        match args[i].as_str() {
            "--date" => {
                let d = v.unwrap_or_else(|| usage()).replace('-', "");
                p.date = d.parse().unwrap_or_else(|_| usage());
            }
            "--time" => {
                let t: Vec<i32> = v
                    .unwrap_or_else(|| usage())
                    .split(':')
                    .map(|x| x.parse().unwrap_or_else(|_| usage()))
                    .collect();
                p.departure_time = t[0] * 3600 + t[1] * 60 + t.get(2).copied().unwrap_or(0);
            }
            "--tw" => p.time_window = num(v) as i32,
            "--percentiles" => {
                p.percentiles = v
                    .unwrap_or_else(|| usage())
                    .split(',')
                    .map(|x| x.parse().unwrap_or_else(|_| usage()))
                    .collect()
            }
            "--max-walk" => p.max_walk_time = num(v) as i32,
            "--max-trip" => p.max_trip_duration = num(v) as i32,
            "--max-rides" => p.max_rides = num(v) as usize,
            "--draws" => p.draws_per_minute = num(v) as i32,
            "--seed" => p.seed = Some(num(v) as i32),
            "--threads" => p.n_threads = num(v) as usize,
            "--repeats" => repeats = num(v) as usize,
            _ => usage(),
        }
        i += 2;
    }
    let t0 = Instant::now();
    let (net, layer) = peabiru_core::transit::build_from_dir(
        dir,
        &BuildOptions {
            n_threads: p.n_threads,
        },
    )
    .unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let data = TransitData::new(&net, &layer);
    let build_s = t0.elapsed().as_secs_f64();
    let (ids, pts) = read_points(points);
    let mut times = Vec::new();
    let mut m = Vec::new();
    for _ in 0..repeats.max(1) {
        let t = Instant::now();
        m = travel_time_matrix(&net, &layer, &data, &pts, &pts, &p).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        });
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
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
    std::fs::write(out, csv).expect("write output");
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
    let net = build_network(pbf, &BuildOptions::default()).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
    let out = std::io::stdout();
    let mut w = std::io::BufWriter::new(out.lock());
    writeln!(
        w,
        "edge_index,osm_id,length,walk,car,car_speed,bicycle,bicycle_lts,lon0,lat0,lon1,lat1"
    )
    .unwrap();
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
        .unwrap();
    }
}

/// Read `id,lon,lat,...` points (header required; columns found by name).
fn read_points(path: &str) -> (Vec<String>, Vec<(f64, f64)>) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(1)
    });
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split(',').collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| h.trim_matches('"') == name)
            .unwrap_or_else(|| {
                eprintln!("{path}: no '{name}' column");
                std::process::exit(1)
            })
    };
    let (ci, clon, clat) = (col("id"), col("lon"), col("lat"));
    let mut ids = Vec::new();
    let mut pts = Vec::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let f: Vec<&str> = line.split(',').collect();
        ids.push(f[ci].trim_matches('"').to_string());
        let num = |s: &str| {
            s.trim_matches('"')
                .parse::<f64>()
                .expect("numeric coordinate")
        };
        pts.push((num(f[clat]), num(f[clon])));
    }
    (ids, pts)
}

fn ttm(pbf: &str, points: &str, mode: &str, out: &str, n_threads: usize, repeats: usize) {
    // A directory is a network folder like r5r's: one .pbf plus GTFS zips.
    let is_dir = std::path::Path::new(pbf).is_dir();
    use peabiru_core::street::{
        build_network, travel_time_matrix, BuildOptions, Mode, RouteParams,
    };
    let mode = match mode {
        "WALK" => Mode::Walk,
        "BICYCLE" => Mode::Bicycle,
        "CAR" => Mode::Car,
        _ => usage(),
    };
    let t0 = Instant::now();
    let net = if is_dir {
        let (net, layer) = peabiru_core::transit::build_from_dir(pbf, &BuildOptions { n_threads })
            .unwrap_or_else(|e| {
                eprintln!("{e}");
                std::process::exit(1)
            });
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
        build_network(pbf, &BuildOptions { n_threads }).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1)
        })
    };
    let build_s = t0.elapsed().as_secs_f64();
    let (ids, pts) = read_points(points);
    let params = RouteParams {
        n_threads,
        ..RouteParams::default()
    };
    let mut times = Vec::new();
    let mut m = Vec::new();
    for _ in 0..repeats.max(1) {
        let t = Instant::now();
        m = travel_time_matrix(&net, &pts, &pts, mode, &params).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(2)
        });
        times.push(t.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
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
    std::fs::write(out, csv).expect("write output");
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
    let ex = read_pbf_file(path, &opts).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
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
