//! The time and fare frontier of `trip_options()`'s search against the one
//! R5's McRAPTOR port (`FareDominatingList` with r5r's calculator) leaves at
//! the destination, departure minute by departure minute, on a real
//! network: `PEABIRU_R5CMP_DIR=<network dir> PEABIRU_R5CMP_FARES=<fares
//! zip> PEABIRU_R5CMP_PAIRS=<origins.csv>,<destinations.csv>`.

use super::*;
use crate::transit::mcraptor::{Dominance, DrawOut, McRaptor, McRequest};
use crate::{BuildOptions, TransitMode};

fn points(path: &str) -> Vec<(f64, f64)> {
    let text = std::fs::read_to_string(path).expect("points");
    let mut lines = text.lines();
    let head: Vec<&str> = lines.next().expect("header").split(',').collect();
    let col = |n: &str| head.iter().position(|h| *h == n).expect("column");
    let (la, lo) = (col("lat"), col("lon"));
    lines
        .map(|l| {
            let v: Vec<&str> = l.split(',').collect();
            (v[la].parse().unwrap(), v[lo].parse().unwrap())
        })
        .collect()
}

/// The (time, fare) Pareto set of `v`.
fn pareto(mut v: Vec<(i32, i32)>) -> Vec<(i32, i32)> {
    v.sort_unstable();
    v.dedup();
    let mut out: Vec<(i32, i32)> = Vec::new();
    for (t, f) in v {
        if out.iter().all(|&(_, g)| g > f) {
            out.push((t, f));
        }
    }
    out
}

#[test]
#[ignore = "needs PEABIRU_R5CMP_DIR, PEABIRU_R5CMP_FARES, PEABIRU_R5CMP_PAIRS"]
fn time_and_fare_frontier_against_r5s_list() {
    let dir = std::env::var("PEABIRU_R5CMP_DIR").expect("dir");
    let fares = std::env::var("PEABIRU_R5CMP_FARES").expect("fares");
    let pairs = std::env::var("PEABIRU_R5CMP_PAIRS").expect("pairs");
    let (of, df) = pairs.split_once(',').expect("two files");
    let (o, d) = (points(of), points(df));
    let (net, layer) =
        crate::transit::build_from_dir(&dir, &BuildOptions::default()).expect("network");
    let data = TransitData::new(&net, &layer);
    let read =
        crate::read_fare_structure(&std::fs::read(&fares).expect("zip"), "UTF-8").expect("fares");
    for variant in [
        "as read",
        "penalty",
        "two discounts, cap",
        "short allowance",
    ] {
        let mut fs = read.clone();
        match variant {
            // a transfer dearer than two full fares
            "penalty" => {
                for t in &mut fs.fares_per_transfer {
                    if t.first_leg.as_deref() == Some("BUS")
                        && t.second_leg.as_deref() == Some("BUS")
                    {
                        t.fare = 11.0;
                    }
                }
            }
            "two discounts, cap" => {
                fs.max_discounted_transfers = 2.0;
                fs.fare_cap = 9.0;
            }
            "short allowance" => fs.transfer_time_allowance = 20.0,
            _ => {}
        }
        compare(variant, &fs, &net, &layer, &data, &o, &d);
    }
}

fn compare(
    variant: &str,
    fs: &crate::FareStructure,
    net: &crate::StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    o: &[(f64, f64)],
    d: &[(f64, f64)],
) {
    let (net, layer, data) = (net, layer, data);
    let p = TripOptionsParams {
        transit_modes: TransitMode::ALL.to_vec(),
        date: 20190513,
        departure_time: 14 * 3600,
        time_window: 1,
        fare_structure: Some(std::sync::Arc::new(fs.clone())),
        ..Default::default()
    };
    let ip = itinerary_params(&p);
    let calc = FareCalculator::new(fs, layer).expect("calc");
    let planner =
        Planner::with_frequencies(net, layer, data, &ip, Some(&calc), true).expect("planner");
    let walk_mms = (ip.req().walk * 1000.0f32) as i32;
    let cfg = search::Config {
        max_rides: p.max_rides,
        max_t: p.max_trip_duration * 60,
        walk_mms,
        max_transfer_mm: walk_mms.saturating_mul(p.max_walk_time * 60),
        criteria: Criteria {
            time: true,
            fare: true,
            transfers: false,
            walk: false,
        },
        max_fare: i32::MAX,
    };
    let sn = search::Network::new(
        layer,
        &data.transfers,
        &planner.mc_allowed,
        &planner.mc_active,
        Some(FareModel::new(&calc)),
        cfg,
    )
    .expect("search network");
    let mut search = Search::default();
    let mut scratch = Vec::new();
    let (mut same, mut r5_short, mut ours_short, mut n) = (0, 0, 0, 0);
    let mut shown = 0;
    for minute in [14 * 3600, 14 * 3600 + 17 * 60, 15 * 3600 + 3 * 60] {
        for i in 0..o.len() {
            let mut via = ViaOrders::as_stated();
            let ss = stop_searches(&planner, o[i], d[i], &mut scratch, &mut via).expect("ss");
            if ss.access_legs.is_empty() || ss.egress_legs.is_empty() {
                continue;
            }
            let bounds = crate::options::bounds::Bounds::new(&sn, &ss.egress_legs);
            let ours: Vec<(i32, i32)> = search
                .run(
                    &sn,
                    minute,
                    &ss.access_legs,
                    &ss.egress_legs,
                    None,
                    &bounds,
                    &p.cancel,
                )
                .iter()
                .map(|a| (a.total, a.fare))
                .collect();
            let req = McRequest {
                from_time: minute,
                to_time: minute + 60,
                max_trip_s: p.max_trip_duration * 60,
                max_rides: p.max_rides,
                suboptimal_minutes: 0,
                max_fare: crate::transit::matrix::max_fare_cents(f64::INFINITY),
                monte_carlo_draws: 1,
                walk_mms: f64::from(ip.req().walk * 1000.0f32),
                seed: Some(1),
                origin: o[i],
            };
            let access: Vec<(u32, i32)> = ss.access_legs.iter().map(|l| (l.stop, l.secs)).collect();
            let egress: Vec<(u32, i32)> = ss.egress_legs.iter().map(|l| (l.stop, l.secs)).collect();
            let mut r5 = McRaptor::new(
                layer,
                &data.transfers,
                &planner.mc_allowed,
                &planner.mc_active,
                planner.mc_schedules.get_or_init(|| {
                    crate::transit::mcraptor::Schedules::new(
                        layer,
                        &planner.mc_allowed,
                        &planner.mc_active,
                    )
                }),
                &req,
                Dominance::Fare(&calc),
                Some(&egress),
                1,
            );
            let mut theirs = Vec::new();
            r5.route_departures(&[minute], &access, &p.cancel, |dep, out| {
                if let DrawOut::Final(arena, states) = out {
                    for &s in states {
                        let st = &arena[s as usize];
                        let total = st.time - dep;
                        if total <= p.max_trip_duration * 60 {
                            theirs.push((total, st.fare.map_or(0, |f| f.paid)));
                        }
                    }
                }
            })
            .expect("r5");
            let (a, b) = (pareto(ours), pareto(theirs));
            n += 1;
            if a == b {
                same += 1;
                continue;
            }
            // a vector of one set that no vector of the other reaches
            let beats = |x: &[(i32, i32)], y: &[(i32, i32)]| {
                x.iter()
                    .any(|&(t, f)| !y.iter().any(|&(u, g)| u <= t && g <= f))
            };
            if beats(&a, &b) {
                r5_short += 1;
                if r5_short <= 3 {
                    eprintln!("{variant}: pair {i} at {minute}: R5 misses; ours {a:?} r5 {b:?}");
                }
            }
            if beats(&b, &a) {
                ours_short += 1;
                if shown < 10 {
                    shown += 1;
                    eprintln!("pair {i} at {minute}: ours {a:?} r5 {b:?}");
                }
            }
        }
    }
    eprintln!(
        "{variant}: {n} pair-minutes: {same} identical frontiers; R5's list misses a vector of ours in \
         {r5_short}; ours misses one of R5's in {ours_short}"
    );
}
