use peabiru_core::osm::pbf::{read_pbf_file, tag, ReadOptions};
use std::time::Instant;

fn usage() -> ! {
    eprintln!(
        "usage: peabiru --version\n       peabiru osm-stats <file.osm.pbf> [n_threads]\n       \
         peabiru ttm <file.osm.pbf> <points.csv> <WALK|BICYCLE|CAR> <out.csv> [n_threads] [repeats]"
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
        _ => usage(),
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
    let net = build_network(pbf, &BuildOptions { n_threads }).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1)
    });
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
        m = travel_time_matrix(&net, &pts, &pts, mode, &params);
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
        "vertices {}  edges {}  build {:.3}s  route {:.3}s (median of {})  pairs {}  reached {}",
        net.n_vertices(),
        net.n_edges(),
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
