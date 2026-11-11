//! `peabiru itineraries`: r5r's `detailed_itineraries()` (no fares),
//! origin `i` to destination `i`, written as CSV with the geometry as WKT
//! (coordinates in shortest round-trip form).

use crate::cli::{self, runtime_error, usage_error};
use peabiru_core::{
    detailed_itineraries, BuildOptions, ItineraryParams, Mode, TransitData, TransitMode,
};
use std::fmt::Write as _;
use std::time::Instant;

pub const USAGE: &str = "peabiru itineraries <dir> <origins.csv> <destinations.csv> <out.csv> \
     [--mode WALK|BICYCLE|CAR] [--transit MODES] [--egress WALK] [--date YYYY-MM-DD] \
     [--time HH:MM:SS] [--tw 10] [--max-walk 120] [--max-bike 120] [--max-car 120] \
     [--max-trip 120] [--walk-speed 3.6] [--bike-speed 12] [--max-rides 3] [--max-lts 2] \
     [--threads 0]";

fn mode(cmd: &str, v: &str) -> Mode {
    match v {
        "WALK" => Mode::Walk,
        "BICYCLE" => Mode::Bicycle,
        "CAR" => Mode::Car,
        _ => usage_error(cmd, format!("unknown street mode '{v}'")),
    }
}

pub fn run(args: &[String]) {
    let cmd = "itineraries";
    if args.len() < 4 {
        usage_error(cmd, USAGE);
    }
    let (dir, origins, dests, out) = (&args[0], &args[1], &args[2], &args[3]);
    let mut p = ItineraryParams::default();
    let mut i = 4;
    while i < args.len() {
        let name = args[i].as_str();
        let v = cli::value(cmd, args, i);
        match name {
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
            "--walk-speed" => p.walk_speed_kmh = cli::float(cmd, name, v),
            "--bike-speed" => p.bike_speed_kmh = cli::float(cmd, name, v),
            "--max-rides" => p.max_rides = cli::size(cmd, name, v),
            "--max-lts" => p.max_lts = cli::count(cmd, name, v),
            "--threads" => p.n_threads = cli::size(cmd, name, v),
            _ => usage_error(cmd, format!("unknown option '{name}'")),
        }
        i += 2;
    }
    let (oid, opts) = crate::read_points(cmd, origins);
    let (did, dpts) = crate::read_points(cmd, dests);
    if opts.len() != dpts.len() {
        runtime_error(cmd, "origins and destinations must have as many rows");
    }
    let t0 = Instant::now();
    let (net, layer) = cli::build_from_dir(cmd, dir, &BuildOptions::default())
        .unwrap_or_else(|e| runtime_error(cmd, e));
    let data = (!p.transit_modes.is_empty()).then(|| TransitData::new(&net, &layer));
    let transit = data.as_ref().map(|d| (&layer, d));
    let t1 = Instant::now();
    let res = detailed_itineraries(&net, transit, &opts, &dpts, &p)
        .unwrap_or_else(|e| runtime_error(cmd, e));
    let t2 = Instant::now();
    let mut s = String::from(
        "from_id,to_id,option,departure_time,total_duration,total_distance,segment,mode,\
         segment_duration,wait,distance,route,geometry\n",
    );
    for (k, its) in res.iter().enumerate() {
        for it in its {
            for (j, l) in it.legs.iter().enumerate() {
                let _ = write!(
                    s,
                    "{},{},{},{},{},{},{},{},{},{},{},{},\"LINESTRING (",
                    oid[k],
                    did[k],
                    it.option,
                    it.departure_time,
                    it.total_duration,
                    it.total_distance,
                    j + 1,
                    l.mode,
                    l.segment_duration,
                    l.wait,
                    l.distance,
                    l.route
                );
                for (n, c) in l.geometry.iter().enumerate() {
                    if n > 0 {
                        s.push_str(", ");
                    }
                    let _ = write!(s, "{:?} {:?}", c[0], c[1]);
                }
                s.push_str(")\"\n");
            }
        }
    }
    cli::write_file(cmd, out, s);
    eprintln!(
        "build {:.2}s itineraries {:.2}s",
        (t1 - t0).as_secs_f64(),
        (t2 - t1).as_secs_f64()
    );
}
