//! `peabiru options`: peabiru's `trip_options()`, origin `i` to destination
//! `i`, written as three CSV files: `<prefix>_options.csv` (one row per
//! option), `<prefix>_departures.csv` (one row per option and departure
//! minute) and `<prefix>_legs.csv` (the legs, geometry as WKT).

use super::cli::{self, runtime_error, usage_error};
use super::expanded::field;
use peabiru_core::{
    read_fare_structure, trip_options, BuildOptions, Criterion, Mode, TransitMode,
    TripOptionsParams,
};
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::Instant;

pub const USAGE: &str = "peabiru options <dir> <origins.csv> <destinations.csv> <out prefix> \
     [--criteria TIME,FARE,TRANSFERS,WALK] [--fares fares.zip] [--max-fare X] \
     [--max-options 0] [--no-itineraries 1] [--mode WALK|BICYCLE|CAR] [--transit MODES] \
     [--egress WALK] [--date YYYY-MM-DD] [--time HH:MM:SS] [--tw 10] [--max-walk 120] \
     [--max-bike 120] [--max-car 120] [--max-trip 120] [--walk-speed 3.6] [--bike-speed 12] \
     [--max-rides 3] [--max-lts 2] [--threads 0] [--repeats 1]";

fn mode(cmd: &str, v: &str) -> Mode {
    match v {
        "WALK" => Mode::Walk,
        "BICYCLE" => Mode::Bicycle,
        "CAR" => Mode::Car,
        _ => usage_error(cmd, format!("unknown street mode '{v}'")),
    }
}

fn clock(s: i32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

pub fn run(args: &[String]) {
    let cmd = "options";
    if args.len() < 4 {
        usage_error(cmd, USAGE);
    }
    let (dir, origins, dests, out) = (&args[0], &args[1], &args[2], &args[3]);
    let mut p = TripOptionsParams::default();
    p.scenario = super::cli::scenario(cmd);
    let mut repeats = 1;
    let mut i = 4;
    while i < args.len() {
        let name = args[i].as_str();
        let v = cli::value(cmd, args, i);
        match name {
            "--criteria" => {
                p.criteria = v
                    .split(',')
                    .map(|c| {
                        Criterion::from_name(c)
                            .unwrap_or_else(|| usage_error(cmd, format!("unknown criterion '{c}'")))
                    })
                    .collect()
            }
            "--fares" => {
                let bytes = std::fs::read(v)
                    .unwrap_or_else(|e| runtime_error(cmd, format!("cannot read {v}: {e}")));
                let fs =
                    read_fare_structure(&bytes, "UTF-8").unwrap_or_else(|e| runtime_error(cmd, e));
                p.fare_structure = Some(Arc::new(fs));
            }
            "--max-fare" => p.max_fare = cli::float(cmd, name, v),
            "--max-options" => p.max_options = cli::size(cmd, name, v),
            "--no-itineraries" => p.itineraries = v == "0",
            "--mode" => p.mode = mode(cmd, v),
            "--egress" => p.egress_mode = mode(cmd, v),
            "--transit" => {
                p.transit_modes = if v == "TRANSIT" {
                    TransitMode::ALL.to_vec()
                } else {
                    v.split(',')
                        .map(|m| {
                            TransitMode::from_name(m)
                                .unwrap_or_else(|| usage_error(cmd, format!("unknown mode '{m}'")))
                        })
                        .collect()
                }
            }
            "--date" => p.date = cli::date(cmd, name, v),
            "--time" => p.departure_time = cli::time_of_day(cmd, name, v),
            "--tw" => p.time_window = cli::count(cmd, name, v),
            "--max-walk" => p.max_walk_time = cli::count(cmd, name, v),
            "--max-bike" => p.max_bike_time = cli::count(cmd, name, v),
            "--max-car" => p.max_car_time = cli::count(cmd, name, v),
            "--max-trip" => p.max_trip_duration = cli::count(cmd, name, v),
            "--walk-speed" => p.walk_speed = cli::float(cmd, name, v),
            "--bike-speed" => p.bike_speed = cli::float(cmd, name, v),
            "--max-rides" => p.max_rides = cli::size(cmd, name, v),
            "--max-lts" => p.max_lts = cli::count(cmd, name, v),
            "--threads" => p.n_threads = cli::size(cmd, name, v),
            "--repeats" => repeats = cli::count(cmd, name, v).max(1),
            _ => usage_error(cmd, format!("unknown option '{name}'")),
        }
        i += 2;
    }
    let (oid, opts) = super::read_points(cmd, origins);
    let (did, dpts) = super::read_points(cmd, dests);
    if opts.len() != dpts.len() {
        runtime_error(cmd, "origins and destinations must have as many rows");
    }
    let t0 = Instant::now();
    let net = cli::build_from_dir(cmd, dir, &BuildOptions::default())
        .unwrap_or_else(|e| runtime_error(cmd, e));
    let t1 = Instant::now();
    let mut res = Vec::new();
    for _ in 0..repeats {
        res = trip_options(&net, &opts, &dpts, &p).unwrap_or_else(|e| runtime_error(cmd, e));
    }
    let t2 = Instant::now();
    let mut o = String::from(
        "from_id,to_id,option,total_time,fare,transfers,walk_time,departure_time,arrival_time,\
         n_departures,first_departure,last_departure,description\n",
    );
    let mut dep = String::from("from_id,to_id,option,departure_time,arrival_time\n");
    let mut legs = String::from(
        "from_id,to_id,option,segment,mode,segment_duration,wait,distance,route,geometry\n",
    );
    for (k, options) in res.iter().enumerate() {
        let (f, t) = (field(&oid[k]), field(&did[k]));
        for x in options {
            let _ = writeln!(
                o,
                "{f},{t},{},{},{},{},{},{},{},{},{},{},{}",
                x.option,
                x.total_time,
                x.fare.map_or("NA".to_string(), |v| v.to_string()),
                x.transfers,
                x.walk_time,
                clock(x.departure_s),
                clock(x.arrival_s),
                x.n_departures(),
                clock(x.departures[0]),
                clock(*x.departures.last().expect("a departure")),
                field(&x.description)
            );
            for (&d, &tt) in x.departures.iter().zip(&x.totals_s) {
                let _ = writeln!(dep, "{f},{t},{},{},{}", x.option, clock(d), clock(d + tt));
            }
            if let Some(it) = &x.itinerary {
                for (j, l) in it.legs.iter().enumerate() {
                    let _ = write!(
                        legs,
                        "{f},{t},{},{},{},{},{},{},{},\"",
                        x.option,
                        j + 1,
                        field(&l.mode),
                        l.segment_duration,
                        l.wait,
                        l.distance,
                        field(&l.route)
                    );
                    legs.push_str(&l.wkt());
                    legs.push_str("\"\n");
                }
            }
        }
    }
    cli::write_file(cmd, &format!("{out}_options.csv"), o);
    cli::write_file(cmd, &format!("{out}_departures.csv"), dep);
    cli::write_file(cmd, &format!("{out}_legs.csv"), legs);
    eprintln!(
        "build {:.2}s options {:.2}s",
        (t1 - t0).as_secs_f64(),
        (t2 - t1).as_secs_f64() / f64::from(repeats)
    );
}
