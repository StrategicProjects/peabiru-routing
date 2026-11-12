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

use crate::cli::{self, runtime_error, usage_error};
use peabiru_core::{
    build_network, r5r_trip_limits, street_accessibility, transit_accessibility, Accessibility,
    AccessibilityParams, BuildOptions, DecayFunction, Destinations, Mode, RouteParams,
    TransitParams,
};
use std::time::Instant;

const CMD: &str = "accessibility";

/// A bad argument: exit status 2.
fn fail(msg: impl std::fmt::Display) -> ! {
    usage_error(CMD, msg)
}

/// A file or network that cannot be read: exit status 1.
fn fail_io(msg: impl std::fmt::Display) -> ! {
    runtime_error(CMD, msg)
}

/// Points and the named numeric columns of a CSV (header required).
struct Points {
    ids: Vec<String>,
    coords: Vec<(f64, f64)>,
    columns: Vec<Vec<f64>>,
}

fn read_points(path: &str, columns: &[String]) -> Points {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| fail_io(format!("cannot read {path}: {e}")));
    let mut lines = text.lines().enumerate();
    let header: Vec<&str> = lines
        .next()
        .map_or("", |(_, l)| l)
        .split(',')
        .map(|h| h.trim().trim_matches('"'))
        .collect();
    let col = |name: &str| {
        header
            .iter()
            .position(|h| *h == name)
            .unwrap_or_else(|| fail_io(format!("{path}:1: no '{name}' column")))
    };
    let (ci, clon, clat) = (col("id"), col("lon"), col("lat"));
    let cols: Vec<usize> = columns.iter().map(|c| col(c)).collect();
    let mut p = Points {
        ids: Vec::new(),
        coords: Vec::new(),
        columns: vec![Vec::new(); cols.len()],
    };
    for (n, line) in lines.filter(|(_, l)| !l.trim().is_empty()) {
        let line_no = n + 1;
        let f: Vec<&str> = line
            .split(',')
            .map(|x| x.trim().trim_matches('"'))
            .collect();
        let get = |i: usize| {
            f.get(i)
                .copied()
                .unwrap_or_else(|| fail_io(format!("{path}:{line_no}: line is short")))
        };
        // NA / empty -> NaN; the core treats a NaN count as 0, like r5r
        let num = |i: usize| match get(i) {
            "" | "NA" => f64::NAN,
            s => s
                .parse::<f64>()
                .unwrap_or_else(|_| fail_io(format!("{path}:{line_no}: '{s}' is not a number"))),
        };
        p.ids.push(get(ci).to_string());
        p.coords.push((num(clat), num(clon)));
        for (k, &c) in cols.iter().enumerate() {
            p.columns[k].push(num(c));
        }
    }
    p
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
    let (net_path, points_path, out) = crate::positional3(CMD, args);
    let mut mode = "WALK".to_string();
    let mut opportunities: Vec<String> = Vec::new();
    let mut decay = "step".to_string();
    let mut decay_value: Option<f64> = None;
    let mut cutoffs: Option<Vec<i32>> = None;
    let mut percentiles = vec![50];
    let mut tp = TransitParams::default();
    tp.scenario = cli::scenario(CMD);
    let mut max_walk: Option<i32> = None;
    let mut max_trip = 120;
    let mut rp = RouteParams::default();
    rp.scenario = tp.scenario.clone();
    let mut dest_path: Option<String> = None;
    let mut repeats = 1usize;
    let (mut have_date, mut have_time) = (false, false);
    let mut i = 3;
    while i < args.len() {
        let name = args[i].as_str();
        let v = cli::value(CMD, args, i);
        // decay_value may be given as Inf; decay_function rejects it with
        // r5r's message
        let num = || -> f64 {
            v.trim()
                .parse()
                .unwrap_or_else(|_| fail(format!("{name} must be a number, got '{v}'")))
        };
        let any_int = || cli::int(CMD, name, v, i32::MIN, i32::MAX);
        match name {
            "--mode" => mode = v.to_uppercase(),
            "--opportunities" => opportunities = cli::list(CMD, name, v),
            "--decay" => decay = v.to_lowercase(),
            "--decay-value" => decay_value = Some(num()),
            "--cutoffs" => cutoffs = Some(cli::list(CMD, name, v)),
            "--percentiles" => percentiles = cli::list(CMD, name, v),
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
            "--max-trip" => max_trip = cli::count(CMD, name, v),
            "--max-rides" => tp.max_rides = cli::count(CMD, name, v) as usize,
            "--draws" => tp.draws_per_minute = cli::count(CMD, name, v),
            "--seed" => tp.seed = Some(any_int()),
            "--walk-speed" => {
                tp.walk_speed_kmh = cli::float(CMD, name, v);
                rp.walk_speed_kmh = tp.walk_speed_kmh;
            }
            "--bike-speed" => rp.bike_speed_kmh = cli::float(CMD, name, v),
            "--max-lts" => rp.max_lts = any_int(),
            "--destinations" => dest_path = Some(v.to_string()),
            "--threads" => {
                tp.n_threads = cli::size(CMD, name, v);
                rp.n_threads = tp.n_threads;
            }
            "--repeats" => repeats = cli::count(CMD, name, v) as usize,
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
    if street_mode.is_none() && !(have_date && have_time) {
        fail("--date and --time are required with TRANSIT");
    }
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
    let mut a = AccessibilityParams::default();
    // r5r passes the placeholder 0 when cutoffs is NULL
    a.cutoffs = cutoffs.unwrap_or_else(|| vec![0]);
    a.decay = f;

    let origins = read_points(points_path, &[]);
    let dest = read_points(dest_path.as_deref().unwrap_or(points_path), &opportunities);
    let destinations = Destinations::new(&dest.coords, &dest.columns);

    let t0 = Instant::now();
    let is_dir = std::path::Path::new(net_path).is_dir();
    let mut build = BuildOptions::default();
    build.n_threads = tp.n_threads;
    let mut times = Vec::new();
    let mut result: Option<Accessibility> = None;
    let build_s;
    if let Some(mode) = street_mode {
        let net = if is_dir {
            cli::build_from_dir(CMD, net_path, &build).unwrap_or_else(|e| fail_io(e))
        } else {
            build_network(net_path, &build).unwrap_or_else(|e| fail_io(e))
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
        let net = cli::build_from_dir(CMD, net_path, &build).unwrap_or_else(|e| fail_io(e));
        build_s = t0.elapsed().as_secs_f64();
        tp.percentiles = percentiles.clone();
        tp.max_trip_duration = m;
        tp.max_walk_time = w;
        for _ in 0..repeats {
            let t = Instant::now();
            result = Some(
                transit_accessibility(&net, &origins.coords, &destinations, &tp, &a)
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
            for (p, pc) in acc.percentiles().iter().enumerate() {
                for (c, cut) in acc.cutoffs().iter().enumerate() {
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
    cli::write_file(CMD, out, csv);
    println!(
        "build {:.3}s  route {:.3}s (median of {})  rows {}  max_trip {}  max_walk {}",
        build_s,
        times[times.len() / 2],
        times.len(),
        acc.values().len(),
        m,
        w
    );
}
