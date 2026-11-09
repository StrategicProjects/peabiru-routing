use peabiru_core::osm::pbf::{read_pbf_file, tag, ReadOptions};
use std::time::Instant;

fn usage() -> ! {
    eprintln!("usage: peabiru --version\n       peabiru osm-stats <file.osm.pbf> [n_threads]");
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
        _ => usage(),
    }
}

fn osm_stats(path: &str, n_threads: usize) {
    let highways = |t: &[(&str, &str)]| tag(t, "highway").is_some();
    let restrictions = |t: &[(&str, &str)]| tag(t, "type") == Some("restriction");
    let opts = ReadOptions {
        way_filter: &highways,
        relation_filter: Some(&restrictions),
        node_tag_filter: None,
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
