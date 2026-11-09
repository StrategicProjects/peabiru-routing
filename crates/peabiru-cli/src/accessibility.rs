//! `peabiru accessibility`: r5r's `accessibility()` from the command line.
//!
//! ```text
//! peabiru accessibility <network dir | file.osm.pbf> <points.csv> <out.csv>
//!   --opportunities jobs,schools  [--mode WALK,TRANSIT | WALK | BICYCLE | CAR]
//!   [--decay step|exponential|fixed_exponential|linear|logistic] [--decay-value V]
//!   [--cutoffs 30,60] [--percentiles 50] [--date YYYY-MM-DD] [--time HH:MM:SS]
//!   [--tw 10] [--max-walk N|Inf] [--max-trip 120] [--max-rides 3] [--draws 5]
//!   [--seed N] [--walk-speed 3.6] [--bike-speed 12] [--max-lts 2]
//!   [--destinations other.csv] [--threads 0] [--repeats 1]
//! ```
//!
//! Points are CSV with `id`, `lon`, `lat` and the opportunity columns; they
//! are the origins and, unless `--destinations` is given, the destinations.
//! Arguments are checked as r5r checks them, and the trip limits derived as
//! r5r derives them (`step` caps `max_trip_duration` at the largest cutoff).
//! The output has r5r's columns `id,opportunity,percentile,cutoff,accessibility`
//! (no `cutoff` with `fixed_exponential`), one row per origin, opportunity,
//! percentile and cutoff in that order, with every double written in full.

use peabiru_core::street::{build_network, BuildOptions, Mode, RouteParams};
use peabiru_core::transit::accessibility::{
    r5r_trip_limits, street_accessibility, transit_accessibility, Accessibility,
    AccessibilityParams, Destinations,
};
use peabiru_core::transit::decay::DecayFunction;
use peabiru_core::transit::matrix::{TransitData, TransitParams};
use std::time::Instant;

fn fail(msg: impl std::fmt::Display) -> ! {
    eprintln!("accessibility: {msg}");
    std::process::exit(2)
}

/// Points and the named numeric columns of a CSV (header required).
struct Points {
    ids: Vec<String>,
    coords: Vec<(f64, f64)>,
    columns: Vec<Vec<f64>>,
}

fn read_points(path: &str, columns: &[String]) -> Points {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| fail(format!("{path}: {e}")));
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .unwrap_or("")
        .split(',')
        .map(|h| h.trim().trim_matches('"'))
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| fail(format!("{path}: no '{name}' column")))
    };
    let (ci, clon, clat) = (col("id"), col("lon"), col("lat"));
    let cols: Vec<usize> = columns.iter().map(|c| col(c)).collect();
    let mut p = Points {
        ids: Vec::new(),
        coords: Vec::new(),
        columns: vec![Vec::new(); cols.len()],
    };
    for (n, line) in lines.filter(|l| !l.trim().is_empty()).enumerate() {
        let f: Vec<&str> = line
            .split(',')
            .map(|x| x.trim().trim_matches('"'))
            .collect();
        let get = |i: usize| {
            f.get(i)
                .copied()
                .unwrap_or_else(|| fail(format!("{path}: line {} is short", n + 2)))
        };
        // NA / empty -> NaN; the core treats a NaN count as 0, like r5r
        let num = |i: usize| match get(i) {
            "" | "NA" => f64::NAN,
            s => s
                .parse::<f64>()
                .unwrap_or_else(|_| fail(format!("{path}: line {}: bad number '{s}'", n + 2))),
        };
        p.ids.push(get(ci).to_string());
        p.coords.push((num(clat), num(clon)));
        for (k, &c) in cols.iter().enumerate() {
            p.columns[k].push(num(c));
        }
    }
    p
}

fn list<T: std::str::FromStr>(s: &str, what: &str) -> Vec<T> {
    s.split([',', ';'])
        .map(|x| {
            x.trim()
                .parse()
                .unwrap_or_else(|_| fail(format!("bad {what} '{x}'")))
        })
        .collect()
}

/// r5r `assign_decay_function` and `set_cutoffs` checks.
fn decay_function(name: &str, value: Option<f64>, cutoffs: &Option<Vec<i32>>) -> DecayFunction {
    if let Some(v) = value {
        if !v.is_finite() {
            fail("decay_value must be finite");
        }
    }
    let need = |v: Option<f64>| {
        v.unwrap_or_else(|| fail(format!("decay_value must not be NULL with {name}")))
    };
    let f = match name {
        "step" | "exponential" => {
            if value.is_some() {
                fail(format!("decay_value must be NULL with {name}"));
            }
            if name == "step" {
                DecayFunction::Step
            } else {
                DecayFunction::Exponential
            }
        }
        "fixed_exponential" => {
            let v = need(value);
            if v <= 0.0 || v >= 1.0 {
                fail("decay_value must be between 0 and 1 (exclusive) with fixed_exponential");
            }
            DecayFunction::FixedExponential { decay_constant: v }
        }
        "linear" => {
            let v = need(value);
            if v.fract() != 0.0 || !(1.0..=59.0).contains(&v) {
                fail("decay_value must be a whole number from 1 to 59 with linear");
            }
            DecayFunction::Linear {
                width_minutes: v as i32,
            }
        }
        "logistic" => {
            let v = need(value);
            if v < 1.0 {
                fail("decay_value must be at least 1 with logistic");
            }
            DecayFunction::Logistic {
                standard_deviation_minutes: v,
            }
        }
        _ => fail(format!("unknown decay function '{name}'")),
    };
    match (name, cutoffs) {
        ("fixed_exponential", Some(_)) => fail("cutoffs must be NULL with fixed_exponential"),
        ("fixed_exponential", None) => {}
        (_, None) => fail(format!("cutoffs must not be NULL with {name}")),
        (_, Some(c)) => {
            let mut s = c.clone();
            s.sort_unstable();
            s.dedup();
            if c.is_empty() || c.len() > 12 || s.len() != c.len() {
                fail("cutoffs must be 1 to 12 distinct values");
            }
            if c.iter().any(|v| !(1..=120).contains(v)) {
                fail("cutoffs must be whole minutes from 1 to 120");
            }
        }
    }
    f
}

pub fn run(args: &[String]) {
    if args.len() < 3 {
        super::usage();
    }
    let (net_path, points_path, out) = (&args[0], &args[1], &args[2]);
    let mut mode = "WALK".to_string();
    let mut opportunities: Vec<String> = Vec::new();
    let mut decay = "step".to_string();
    let mut decay_value: Option<f64> = None;
    let mut cutoffs: Option<Vec<i32>> = None;
    let mut percentiles = vec![50];
    let mut tp = TransitParams::default();
    let mut max_walk: Option<i32> = None;
    let mut max_trip = 120;
    let mut rp = RouteParams::default();
    let mut dest_path: Option<String> = None;
    let mut repeats = 1usize;
    let mut i = 3;
    while i < args.len() {
        let v = args
            .get(i + 1)
            .unwrap_or_else(|| fail(format!("{} needs a value", args[i])));
        let num = |what: &str| -> f64 {
            v.parse()
                .unwrap_or_else(|_| fail(format!("bad {what} '{v}'")))
        };
        let int = |what: &str| -> i32 {
            v.parse()
                .unwrap_or_else(|_| fail(format!("bad {what} '{v}'")))
        };
        match args[i].as_str() {
            "--mode" => mode = v.to_uppercase(),
            "--opportunities" => opportunities = list(v, "opportunity column"),
            "--decay" => decay = v.to_lowercase(),
            "--decay-value" => decay_value = Some(num("decay value")),
            "--cutoffs" => cutoffs = Some(list(v, "cutoff")),
            "--percentiles" => percentiles = list(v, "percentile"),
            "--date" => {
                tp.date = v
                    .replace('-', "")
                    .parse()
                    .unwrap_or_else(|_| fail(format!("bad date '{v}'")))
            }
            "--time" => {
                let t: Vec<i32> = list(&v.replace(':', ","), "time");
                if t.len() < 2 || t.len() > 3 {
                    fail(format!("bad time '{v}'"));
                }
                tp.departure_time = t[0] * 3600 + t[1] * 60 + t.get(2).copied().unwrap_or(0);
            }
            "--tw" => tp.time_window = int("time window"),
            "--max-walk" => {
                max_walk = if v.eq_ignore_ascii_case("inf") {
                    None
                } else {
                    Some(int("max walk time"))
                }
            }
            "--max-trip" => max_trip = int("max trip duration"),
            "--max-rides" => tp.max_rides = int("max rides").max(0) as usize,
            "--draws" => tp.draws_per_minute = int("draws per minute"),
            "--seed" => tp.seed = Some(int("seed")),
            "--walk-speed" => {
                tp.walk_speed_kmh = num("walk speed");
                rp.walk_speed_kmh = tp.walk_speed_kmh;
            }
            "--bike-speed" => rp.bike_speed_kmh = num("bike speed"),
            "--max-lts" => rp.max_lts = int("max lts"),
            "--destinations" => dest_path = Some(v.clone()),
            "--threads" => {
                tp.n_threads = int("threads").max(0) as usize;
                rp.n_threads = tp.n_threads;
            }
            "--repeats" => repeats = int("repeats").max(1) as usize,
            other => fail(format!("unknown option '{other}'")),
        }
        i += 2;
    }
    if opportunities.is_empty() {
        fail("--opportunities is required");
    }
    // r5r set_percentiles: 1..=99, at most 5, distinct
    let mut sorted = percentiles.clone();
    sorted.sort_unstable();
    sorted.dedup();
    if percentiles.is_empty() || percentiles.len() > 5 || sorted.len() != percentiles.len() {
        fail("percentiles must be 1 to 5 distinct values");
    }
    let modes: Vec<&str> = mode.split([',', ';']).map(str::trim).collect();
    let transit = modes.contains(&"TRANSIT");
    let street_mode = match (transit, modes.as_slice()) {
        (true, _) if modes.iter().all(|m| ["WALK", "TRANSIT"].contains(m)) => None,
        (false, ["WALK"]) => Some(Mode::Walk),
        (false, ["BICYCLE"]) => Some(Mode::Bicycle),
        (false, ["CAR"]) => Some(Mode::Car),
        _ => fail(format!(
            "unsupported mode '{mode}' (WALK,TRANSIT | WALK | BICYCLE | CAR)"
        )),
    };
    let f = decay_function(&decay, decay_value, &cutoffs);
    let (m, w) = r5r_trip_limits(
        &f,
        cutoffs.as_deref(),
        max_trip,
        max_walk,
        street_mode == Some(Mode::Walk),
    )
    .unwrap_or_else(|e| fail(e));
    let fixed = matches!(f, DecayFunction::FixedExponential { .. });
    let a = AccessibilityParams {
        // r5r passes the placeholder 0 when cutoffs is NULL
        cutoffs: cutoffs.unwrap_or_else(|| vec![0]),
        decay: f,
    };

    let origins = read_points(points_path, &[]);
    let dest = read_points(dest_path.as_deref().unwrap_or(points_path), &opportunities);
    let destinations = Destinations {
        points: &dest.coords,
        opportunities: &dest.columns,
    };

    let t0 = Instant::now();
    let is_dir = std::path::Path::new(net_path).is_dir();
    let build = BuildOptions {
        n_threads: tp.n_threads,
    };
    let mut times = Vec::new();
    let mut result: Option<Accessibility> = None;
    let build_s;
    if let Some(mode) = street_mode {
        let net = if is_dir {
            peabiru_core::transit::build_from_dir(net_path, &build)
                .unwrap_or_else(|e| fail(e))
                .0
        } else {
            build_network(net_path, &build).unwrap_or_else(|e| fail(e))
        };
        build_s = t0.elapsed().as_secs_f64();
        rp.max_trip_duration_min = m;
        for _ in 0..repeats {
            let t = Instant::now();
            result = Some(
                street_accessibility(
                    &net,
                    &origins.coords,
                    &destinations,
                    mode,
                    &rp,
                    &percentiles,
                    &a,
                )
                .unwrap_or_else(|e| fail(e)),
            );
            times.push(t.elapsed().as_secs_f64());
        }
    } else {
        if tp.date == 0 {
            fail("--date and --time are required with TRANSIT");
        }
        let (net, layer) =
            peabiru_core::transit::build_from_dir(net_path, &build).unwrap_or_else(|e| fail(e));
        let data = TransitData::new(&net, &layer);
        build_s = t0.elapsed().as_secs_f64();
        tp.percentiles = percentiles.clone();
        tp.max_trip_duration = m;
        tp.max_walk_time = w;
        for _ in 0..repeats {
            let t = Instant::now();
            result = Some(
                transit_accessibility(&net, &layer, &data, &origins.coords, &destinations, &tp, &a)
                    .unwrap_or_else(|e| fail(e)),
            );
            times.push(t.elapsed().as_secs_f64());
        }
    }
    let acc = result.expect("at least one run");
    times.sort_by(|a, b| a.total_cmp(b));

    let mut csv = String::from(if fixed {
        "id,opportunity,percentile,accessibility\n"
    } else {
        "id,opportunity,percentile,cutoff,accessibility\n"
    });
    for (o, id) in origins.ids.iter().enumerate() {
        for (k, name) in opportunities.iter().enumerate() {
            for (p, pc) in acc.percentiles.iter().enumerate() {
                for (c, cut) in acc.cutoffs.iter().enumerate() {
                    // `{}` prints the shortest string that parses back to
                    // the same double
                    let v = acc.get(o, k, p, c);
                    if fixed {
                        csv.push_str(&format!("{id},{name},{pc},{v}\n"));
                    } else {
                        csv.push_str(&format!("{id},{name},{pc},{cut},{v}\n"));
                    }
                }
            }
        }
    }
    std::fs::write(out, csv).unwrap_or_else(|e| fail(format!("{out}: {e}")));
    println!(
        "build {:.3}s  route {:.3}s (median of {})  rows {}  max_trip {}  max_walk {}",
        build_s,
        times[times.len() / 2],
        times.len(),
        acc.values.len(),
        m,
        w
    );
}
