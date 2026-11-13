//! The command-line tool as it was before 0.1.0: positional arguments and
//! behaviour switched by environment variables. The repository's
//! benchmark and comparison scripts (and its reference outputs) run these
//! spellings, so they stay, undocumented: `peabiru ttm|transit|osm-stats|
//! dump-edges|dump-elevation|sample-raster|crs-transform ...` and the old
//! positional forms of `accessibility`, `expanded`, `isochrone`,
//! `itineraries` and `options`, also as `peabiru dev <command> ...`.

mod accessibility;
mod cli;
mod expanded;
mod isochrone;
mod itineraries;
mod options;

use cli::{runtime_error, usage_error};
use peabiru_core::unstable::{read_pbf_file, tag, ReadOptions};
use std::time::Instant;

/// The commands only the old tool has.
const OLD_ONLY: [&str; 7] = [
    "ttm",
    "transit",
    "osm-stats",
    "dump-edges",
    "dump-elevation",
    "sample-raster",
    "crs-transform",
];

/// Whether `args` (without the program name) is an old spelling: a command
/// only the old tool has, `dev <command>`, or one of the routing commands
/// in its old positional form (three or four file arguments before any
/// option; the new forms take one).
pub fn is_legacy(args: &[String]) -> bool {
    let Some(cmd) = args.first().map(String::as_str) else {
        return false;
    };
    if cmd == "dev" || OLD_ONLY.contains(&cmd) {
        return true;
    }
    let positional = |n: usize| args.len() > n && args[1..=n].iter().all(|a| !a.starts_with('-'));
    match cmd {
        "accessibility" | "expanded" | "isochrone" => positional(3),
        "itineraries" | "options" => positional(4),
        _ => false,
    }
}

fn usage_text() -> String {
    format!(
        "usage: peabiru --version\n       peabiru osm-stats <file.osm.pbf> [n_threads]\n       \
         peabiru ttm <file.osm.pbf> <points.csv> <WALK|BICYCLE|CAR> <out.csv> [n_threads] [repeats]\n       \
         peabiru transit <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS [--tw 10] [--percentiles 50] [--max-walk 120] [--max-trip 120] [--max-rides 3] [--draws 5] [--seed N] [--threads 0] [--repeats 1]\n       \
         peabiru accessibility <dir|file.osm.pbf> <points.csv> <out.csv> --opportunities a,b [options]\n       \
         peabiru dump-edges <file.osm.pbf | dir>   (CSV of the directed street edges;\n                                             a directory links its GTFS stops)\n       \
         peabiru dump-elevation <dir> <out prefix>   (elevation samples and slope factors)\n       \
         peabiru sample-raster <file.tif> <points> <out>   (the elevation read at points)\n       \
         peabiru crs-transform <epsg code> <points> <out>   (the transform of a raster CRS)\n       \
         (PEABIRU_ELEVATION=TOBLER|MINETTI|NONE: elevation of the networks built from a\n        directory; without it .tif rasters are ignored)\n       \
         {}\n       {}\n       {}\n       {}",
        expanded::USAGE,
        isochrone::USAGE,
        itineraries::USAGE,
        options::USAGE
    )
}

/// Usage on stderr, exit status 2.
fn usage() -> ! {
    eprintln!("{}", usage_text());
    std::process::exit(2);
}

/// Run an old spelling (see [`is_legacy`]); `args` without the program
/// name, `dev` included or not.
pub fn run(args: &[String]) {
    let args: Vec<String> = match args.first().map(String::as_str) {
        Some("dev") => args[1..].to_vec(),
        _ => args.to_vec(),
    };
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
        Some("isochrone") => isochrone::run(&args[1..]),
        Some("itineraries") => itineraries::run(&args[1..]),
        Some("options") => options::run(&args[1..]),
        Some("dump-elevation") => {
            let cmd = "dump-elevation";
            if args.len() != 3 {
                usage_error(cmd, "expected <dir> <out prefix>");
            }
            dump_elevation(&args[1], &args[2]);
        }
        Some("sample-raster") => {
            let cmd = "sample-raster";
            if args.len() != 4 {
                usage_error(cmd, "expected <file.tif> <points> <out>");
            }
            sample_raster(&args[1], &args[2], &args[3]);
        }
        Some("crs-transform") => {
            let cmd = "crs-transform";
            if args.len() != 4 {
                usage_error(cmd, "expected <epsg code> <points> <out>");
            }
            crs_transform(&args[1], &args[2], &args[3]);
        }
        Some("dump-edges") => {
            let path = args
                .get(1)
                .unwrap_or_else(|| usage_error("dump-edges", "expected <file.osm.pbf or dir>"));
            dump_edges(path)
        }
        Some(other) => usage_error(other, "unknown command (peabiru --help lists them)"),
        None => usage(),
    }
}

/// The three positional arguments `<network> <points.csv> <out.csv>` of the
/// routing commands.
pub(super) fn positional3<'a>(cmd: &str, args: &'a [String]) -> (&'a str, &'a str, &'a str) {
    if args.len() < 3 || args[..3].iter().any(|a| a.starts_with("--")) {
        usage_error(cmd, "expected <network> <points.csv> <out.csv> [options]");
    }
    (&args[0], &args[1], &args[2])
}

/// `peabiru transit <dir> <points.csv> <out.csv> --date YYYY-MM-DD --time HH:MM:SS
///  [--tw 10] [--percentiles 50] [--max-walk 120] [--max-trip 120] [--max-rides 3]
///  [--draws 5] [--seed N] [--threads 0] [--repeats 1]`
fn transit(args: &[String]) {
    use peabiru_core::{transit_travel_time_matrix, BuildOptions, TransitParams};
    let cmd = "transit";
    let (dir, points, out) = positional3(cmd, args);
    let mut p = TransitParams::default();
    p.scenario = cli::scenario(cmd);
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
    let mut opts = BuildOptions::default();
    opts.n_threads = p.n_threads;
    let net = cli::build_from_dir(cmd, dir, &opts).unwrap_or_else(|e| runtime_error(cmd, e));
    let empty = peabiru_core::TransitLayer::default();
    let layer = net.transit().unwrap_or(&empty);
    let build_s = t0.elapsed().as_secs_f64();
    let mut times = Vec::new();
    let mut m = None;
    for _ in 0..repeats {
        let t = Instant::now();
        m = Some(
            transit_travel_time_matrix(&net, &pts, &pts, &p)
                .unwrap_or_else(|e| usage_error(cmd, e)),
        );
        times.push(t.elapsed().as_secs_f64());
    }
    let m = m.expect("at least one run");
    times.sort_by(|a, b| a.total_cmp(b));
    let pcs = m.percentiles();
    let mut csv = String::from("from_id,to_id");
    for pc in pcs {
        csv.push_str(&format!(",travel_time_p{pc:02}"));
    }
    csv.push('\n');
    let mut rows = 0usize;
    for (i, row) in m.rows().enumerate() {
        for (j, cell) in row.chunks_exact(pcs.len()).enumerate() {
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
        layer.n_stops(),
        layer.n_patterns(),
        build_s,
        times[times.len() / 2],
        times.len(),
        rows
    );
}

/// The elevations sampled on the network of a data directory (decimetres,
/// per vertex and per edge pair) and the slope factor of each directed
/// edge (float bits), in the layout of R5's `ElevationCostField`, for
/// comparing with R5 field by field. The model is `PEABIRU_ELEVATION`
/// (default `TOBLER`).
fn dump_elevation(dir: &str, out: &str) {
    use peabiru_core::unstable::{edge_slope, elevation_samples};
    use peabiru_core::BuildOptions;
    use std::io::Write;
    let cmd = "dump-elevation";
    let mut opts = BuildOptions::default();
    opts.elevation = cli::elevation(cmd).unwrap_or(peabiru_core::Elevation::Tobler);
    let network = cli::build_from_dir(cmd, dir, &opts).unwrap_or_else(|e| runtime_error(cmd, e));
    let net = network.street();
    let tif = std::fs::read_dir(dir)
        .unwrap_or_else(|e| runtime_error(cmd, e))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "tif"))
        .unwrap_or_else(|| runtime_error(cmd, format!("{dir}: no .tif raster")));
    let bytes = std::fs::read(&tif).unwrap_or_else(|e| runtime_error(cmd, e));
    let (vertices, profiles) =
        elevation_samples(net, &bytes).unwrap_or_else(|e| runtime_error(cmd, e));
    let file = |suffix: &str| {
        let p = format!("{out}_{suffix}.csv");
        std::io::BufWriter::new(
            std::fs::File::create(&p)
                .unwrap_or_else(|e| runtime_error(cmd, format!("cannot write {p}: {e}"))),
        )
    };
    let fail = |e: std::io::Error| -> ! { runtime_error(cmd, format!("cannot write output: {e}")) };
    let mut w = file("vertices");
    writeln!(w, "vertex,dm").unwrap_or_else(|e| fail(e));
    for (v, d) in vertices.iter().enumerate() {
        writeln!(w, "{v},{d}").unwrap_or_else(|e| fail(e));
    }
    w.flush().unwrap_or_else(|e| fail(e));
    let mut w = file("profiles");
    writeln!(w, "pair,dm").unwrap_or_else(|e| fail(e));
    for (k, p) in profiles.iter().enumerate() {
        let s: Vec<String> = p.iter().map(i16::to_string).collect();
        writeln!(w, "{k},{}", s.join(" ")).unwrap_or_else(|e| fail(e));
    }
    w.flush().unwrap_or_else(|e| fail(e));
    let mut w = file("factors");
    writeln!(w, "edge,bits").unwrap_or_else(|e| fail(e));
    for e in 0..net.n_edges() {
        let k = edge_slope(net, e).unwrap_or(0.0);
        writeln!(w, "{e},{:x}", k.to_bits()).unwrap_or_else(|e| fail(e));
    }
    w.flush().unwrap_or_else(|e| fail(e));
    eprintln!(
        "{} vertices, {} edge pairs, {} edges, elevation {}",
        net.n_vertices(),
        profiles.len(),
        net.n_edges(),
        net.elevation().name()
    );
}

/// R5's `readElevation` at points given as lines `lon,lat` of the hex bits
/// of two doubles; one line out per point, the hex bits of the value or
/// `ERR` (a debugging aid for comparing with R5 point by point).
fn sample_raster(tif: &str, points: &str, out: &str) {
    let cmd = "sample-raster";
    let bytes = std::fs::read(tif).unwrap_or_else(|e| runtime_error(cmd, format!("{tif}: {e}")));
    let text = std::fs::read_to_string(points)
        .unwrap_or_else(|e| runtime_error(cmd, format!("{points}: {e}")));
    let bits = |s: &str| {
        u64::from_str_radix(s.trim(), 16)
            .map(f64::from_bits)
            .unwrap_or_else(|_| runtime_error(cmd, format!("{points}: bad value '{s}'")))
    };
    let pts: Vec<(f64, f64)> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (a, b) = l
                .split_once(',')
                .unwrap_or_else(|| runtime_error(cmd, "bad line"));
            (bits(a), bits(b))
        })
        .collect();
    let values = peabiru_core::unstable::raster_samples(&bytes, &pts)
        .unwrap_or_else(|e| runtime_error(cmd, format!("{tif}: {e}")));
    let mut csv = String::new();
    for v in values {
        match v {
            Ok((x, false)) => csv.push_str(&format!("{:x}\n", x.to_bits())),
            // within nanometres of a pixel boundary: R5's value depends on the machine
            Ok((x, true)) => csv.push_str(&format!("{:x} AMBIGUOUS\n", x.to_bits())),
            Err(_) => csv.push_str("ERR\n"),
        }
    }
    cli::write_file(cmd, out, csv);
}

/// The transform R5 (GeoTools) applies to WGS 84 points for a raster in
/// EPSG `code`, at points given as lines `lon,lat` of the hex bits of two
/// doubles; one line out per point, `x y` as hex bits (for comparing with
/// GeoTools, bench/compare/crs/).
fn crs_transform(code: &str, points: &str, out: &str) {
    let cmd = "crs-transform";
    let code: u32 = code
        .parse()
        .unwrap_or_else(|_| usage_error(cmd, format!("bad EPSG code '{code}'")));
    let text = std::fs::read_to_string(points)
        .unwrap_or_else(|e| runtime_error(cmd, format!("{points}: {e}")));
    let bits = |s: &str| {
        u64::from_str_radix(s.trim(), 16)
            .map(f64::from_bits)
            .unwrap_or_else(|_| runtime_error(cmd, format!("{points}: bad value '{s}'")))
    };
    let pts: Vec<(f64, f64)> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let mut f = l.split(',');
            let a = f.next().unwrap_or_else(|| runtime_error(cmd, "bad line"));
            let b = f.next().unwrap_or_else(|| runtime_error(cmd, "bad line"));
            (bits(a), bits(b))
        })
        .collect();
    let xy = peabiru_core::unstable::crs_transform(code, &pts)
        .unwrap_or_else(|| runtime_error(cmd, format!("EPSG:{code} is not supported")));
    let mut csv = String::with_capacity(xy.len() * 34);
    for (x, y) in xy {
        csv.push_str(&format!("{:x} {:x}\n", x.to_bits(), y.to_bits()));
    }
    cli::write_file(cmd, out, csv);
}

/// Every directed edge with the attributes `street_network_to_sf()`
/// reports, for comparing networks edge by edge (bench/compare/edges.py).
fn dump_edges(pbf: &str) {
    use peabiru_core::unstable::flags::*;
    use peabiru_core::unstable::{cms_to_ms, edge, lts_of};
    use peabiru_core::{build_network, BuildOptions};
    use std::io::Write;
    let cmd = "dump-edges";
    // a directory: the network with its GTFS stops linked
    let net = if std::path::Path::new(pbf).is_dir() {
        cli::build_from_dir(cmd, pbf, &BuildOptions::default())
            .unwrap_or_else(|e| runtime_error(cmd, e))
    } else {
        build_network(pbf, &BuildOptions::default()).unwrap_or_else(|e| runtime_error(cmd, e))
    };
    let net = net.street();
    let out = std::io::stdout();
    let mut w = std::io::BufWriter::new(out.lock());
    let fail = |e: std::io::Error| -> ! { runtime_error(cmd, format!("cannot write output: {e}")) };
    writeln!(
        w,
        "edge_index,osm_id,length,walk,car,car_speed,bicycle,bicycle_lts,lon0,lat0,lon1,lat1{}",
        if net.elevation() == peabiru_core::Elevation::None {
            ""
        } else {
            ",slope"
        }
    )
    .unwrap_or_else(|e| fail(e));
    for e in 0..net.n_edges() {
        let d = edge(net, e);
        let f = d.flags;
        write!(
            w,
            "{},{},{:.3},{},{},{},{},{},{:.7},{:.7},{:.7},{:.7}",
            e,
            d.way,
            f64::from(d.length_mm) / 1000.0,
            f & ALLOWS_PEDESTRIAN != 0,
            f & ALLOWS_CAR != 0,
            f64::from(cms_to_ms(d.speed_cms)) * 3.6,
            f & ALLOWS_BIKE != 0,
            lts_of(f),
            f64::from(d.from.1) / 1e7,
            f64::from(d.from.0) / 1e7,
            f64::from(d.to.1) / 1e7,
            f64::from(d.to.0) / 1e7
        )
        .unwrap_or_else(|e| fail(e));
        if let Some(k) = peabiru_core::unstable::edge_slope(net, e) {
            // the factor's bits, as R5's ElevationCostField.edgeFactors
            write!(w, ",{:x}", k.to_bits()).unwrap_or_else(|e| fail(e));
        }
        writeln!(w).unwrap_or_else(|e| fail(e));
    }
    w.flush().unwrap_or_else(|e| fail(e));
}

/// Read `id,lon,lat,...` points (header required; columns found by name).
/// A file that cannot be read or parsed ends the process with status 1 and
/// a message naming the file and line.
pub(super) fn read_points(cmd: &str, path: &str) -> (Vec<String>, Vec<(f64, f64)>) {
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
    // A directory is a network folder: one .pbf plus GTFS zips.
    let is_dir = std::path::Path::new(pbf).is_dir();
    use peabiru_core::{
        build_network, street_travel_time_matrix, BuildOptions, Mode, RouteParams, StreetEngine,
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
    let mut opts = BuildOptions::default();
    opts.n_threads = n_threads;
    let net = if is_dir {
        let net = cli::build_from_dir(cmd, pbf, &opts).unwrap_or_else(|e| runtime_error(cmd, e));
        let empty = peabiru_core::TransitLayer::default();
        let layer = net.transit().unwrap_or(&empty);
        eprintln!(
            "feeds {:?}  stops {} (linked {})  patterns {}  frequencies {}",
            layer.feed_ids(),
            layer.n_stops(),
            layer.n_linked_stops(),
            layer.n_patterns(),
            layer.has_frequencies()
        );
        net
    } else {
        build_network(pbf, &opts).unwrap_or_else(|e| runtime_error(cmd, e))
    };
    let build_s = t0.elapsed().as_secs_f64();
    // For benchmarks: force one way of computing the matrix (the result is
    // the same either way).
    let engine = match std::env::var("PEABIRU_STREET_MATRIX").as_deref() {
        Ok("dijkstra") => StreetEngine::Search,
        Ok("ch") => StreetEngine::Hierarchy,
        Ok("auto") | Err(_) => StreetEngine::Auto,
        Ok(other) => usage_error(
            cmd,
            format!("PEABIRU_STREET_MATRIX must be dijkstra, ch or auto, got '{other}'"),
        ),
    };
    let mut params = RouteParams::default();
    params.n_threads = n_threads;
    params.engine = engine;
    params.scenario = cli::scenario(cmd);
    let mut times = Vec::new();
    let mut m = None;
    for _ in 0..repeats {
        let t = Instant::now();
        m = Some(
            street_travel_time_matrix(&net, &pts, &pts, mode, &params)
                .unwrap_or_else(|e| usage_error(cmd, e)),
        );
        times.push(t.elapsed().as_secs_f64());
    }
    let m = m.expect("at least one run");
    // The first call may build a contraction hierarchy the later ones reuse.
    let first_s = times[0];
    let mut later = times[1..].to_vec();
    later.sort_by(|a, b| a.total_cmp(b));
    times.sort_by(|a, b| a.total_cmp(b));
    let route_s = times[times.len() / 2];
    let n = pts.len();
    let mut csv = String::from("from_id,to_id,travel_time_p50\n");
    let mut reached = 0usize;
    for i in 0..n {
        for j in 0..n {
            if let Some(t) = m.get(i, j) {
                reached += 1;
                csv.push_str(&format!("{},{},{}\n", ids[i], ids[j], t));
            }
        }
    }
    cli::write_file(cmd, out, csv);
    println!(
        "vertices {}  edges {}  restrictions {}  build {:.3}s  route {:.3}s (median of {})  pairs {}  reached {}",
        net.street().n_vertices(),
        net.street().n_edges(),
        net.street().n_restrictions(),
        build_s,
        route_s,
        times.len(),
        n * n,
        reached
    );
    if !later.is_empty() {
        println!(
            "first call {:.3}s  later calls {:.3}s (median of {})",
            first_s,
            later[later.len() / 2],
            later.len()
        );
    }
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
