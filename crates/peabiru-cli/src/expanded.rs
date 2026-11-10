//! `peabiru expanded`: r5r's `expanded_travel_time_matrix()` and, with
//! `--arrival`, `arrival_travel_time_matrix()`, written as r5r's CSV.

use crate::cli::{self, runtime_error, usage_error};
use peabiru_core::street::BuildOptions;
use peabiru_core::transit::expanded::{
    arrival_departure, arrival_travel_time_matrix, expanded_travel_time_matrix, ExpandedRow,
};
use peabiru_core::transit::matrix::{TransitData, TransitParams};
use std::io::Write;
use std::time::Instant;

/// Origins routed per batch, to bound the rows held in memory.
const ORIGIN_BATCH: usize = 64;

pub const USAGE: &str = "peabiru expanded <dir> <points.csv> <out.csv> --date YYYY-MM-DD \
     --time HH:MM:SS [--arrival] [--breakdown] [--origins N] [--tw 10] [--max-walk 120] \
     [--max-trip 120] [--max-rides 3] [--draws 5] [--seed N] [--threads 0]";

/// `Utils.getTimeFromSeconds`: `HH:MM:SS`, hours not wrapped at 24.
fn hhmmss(s: i32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

pub fn run(args: &[String]) {
    let cmd = "expanded";
    let (dir, points, out) = crate::positional3(cmd, args);
    let mut p = TransitParams::default();
    let (mut arrival, mut breakdown, mut n_origins) = (false, false, usize::MAX);
    let mut have_date = false;
    let mut have_time = false;
    let mut i = 3;
    while i < args.len() {
        let name = args[i].as_str();
        match name {
            "--arrival" => {
                arrival = true;
                i += 1;
                continue;
            }
            "--breakdown" => {
                breakdown = true;
                i += 1;
                continue;
            }
            _ => {}
        }
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
            "--origins" => n_origins = cli::size(cmd, name, v),
            "--tw" => p.time_window = cli::count(cmd, name, v),
            "--max-walk" => p.max_walk_time = cli::count(cmd, name, v),
            "--max-trip" => p.max_trip_duration = cli::count(cmd, name, v),
            "--max-rides" => p.max_rides = cli::count(cmd, name, v) as usize,
            "--draws" => p.draws_per_minute = cli::count(cmd, name, v),
            "--seed" => p.seed = Some(cli::int(cmd, name, v, i32::MIN, i32::MAX)),
            "--threads" => p.n_threads = cli::size(cmd, name, v),
            _ => usage_error(cmd, format!("unknown option '{name}'")),
        }
        i += 2;
    }
    if !(have_date && have_time) {
        usage_error(cmd, "--date and --time are required");
    }
    if arrival {
        // `arrival_travel_time_matrix.R`: departure = arrival - M, window M.
        let (date, t) = arrival_departure(p.date, p.departure_time, p.max_trip_duration)
            .unwrap_or_else(|e| usage_error(cmd, e));
        p.date = date;
        p.departure_time = t;
        p.time_window = p.max_trip_duration;
    }
    let (ids, pts) = crate::read_points(cmd, points);
    let t0 = Instant::now();
    let (net, layer) = peabiru_core::transit::build_from_dir(
        dir,
        &BuildOptions {
            n_threads: p.n_threads,
        },
    )
    .unwrap_or_else(|e| runtime_error(cmd, e));
    let data = TransitData::new(&net, &layer);
    let build_s = t0.elapsed().as_secs_f64();
    let origins = &pts[..n_origins.min(pts.len())];

    let file = std::fs::File::create(out)
        .unwrap_or_else(|e| runtime_error(cmd, format!("cannot create {out}: {e}")));
    let write_failed =
        |e: std::io::Error| -> ! { runtime_error(cmd, format!("cannot write {out}: {e}")) };
    let mut w = std::io::BufWriter::new(file);
    let header = if breakdown {
        "from_id,to_id,departure_time,draw_number,access_time,wait_time,ride_time,\
         transfer_time,egress_time,routes,n_rides,total_time"
    } else {
        "from_id,to_id,departure_time,draw_number,routes,total_time"
    };
    writeln!(w, "{header}").unwrap_or_else(|e| write_failed(e));
    let t1 = Instant::now();
    let mut n_rows = 0usize;
    for (b, batch) in origins.chunks(ORIGIN_BATCH).enumerate() {
        let res = if arrival {
            arrival_travel_time_matrix(&net, &layer, &data, batch, &pts, &p)
        } else {
            expanded_travel_time_matrix(&net, &layer, &data, batch, &pts, &p)
        };
        let rows = res.unwrap_or_else(|e| usage_error(cmd, e));
        for (k, origin_rows) in rows.iter().enumerate() {
            let from = &ids[b * ORIGIN_BATCH + k];
            for r in origin_rows {
                write_row(&mut w, from, &ids, r, breakdown).unwrap_or_else(|e| write_failed(e));
            }
            n_rows += origin_rows.len();
        }
    }
    w.flush().unwrap_or_else(|e| write_failed(e));
    println!(
        "stops {}  patterns {}  build {:.3}s  route {:.3}s  origins {}  rows {}",
        layer.stop_ids.len(),
        layer.patterns.len(),
        build_s,
        t1.elapsed().as_secs_f64(),
        origins.len(),
        n_rows
    );
}

/// A CSV field, quoted when needed (fwrite's `quote = "auto"`).
fn field(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\"")).into()
    } else {
        s.into()
    }
}

/// One CSV row as `data.table::fwrite` writes r5r's result: NA as empty,
/// numbers in their shortest form.
fn write_row(
    w: &mut impl Write,
    from: &str,
    ids: &[String],
    r: &ExpandedRow,
    breakdown: bool,
) -> std::io::Result<()> {
    let (from, to) = (field(from), field(&ids[r.to as usize]));
    let dep = hhmmss(r.departure);
    match (&r.trip, breakdown) {
        (Some(t), true) => writeln!(
            w,
            "{from},{to},{dep},{},{},{},{},{},{},{},{},{}",
            r.draw,
            t.access_time,
            t.wait_time,
            t.ride_time,
            t.transfer_time,
            t.egress_time,
            field(&t.routes),
            t.n_rides,
            t.total_time
        ),
        (Some(t), false) => writeln!(
            w,
            "{from},{to},{dep},{},{},{}",
            r.draw,
            field(&t.routes),
            t.total_time
        ),
        (None, true) => writeln!(w, "{from},{to},{dep},{},,,,,,,,", r.draw),
        (None, false) => writeln!(w, "{from},{to},{dep},{},,", r.draw),
    }
}
