//! `trip_options` on the São Paulo sample of the repository (frequency-based
//! routes): the expected-wait rule checked by hand, and properties that must
//! hold whatever the network (no option of a minute dominates another; a
//! criterion added keeps the old frontier's vectors; the limits only
//! filter; the legs add up to the values; threads change nothing). With
//! `PEABIRU_OPTIONS_POA=<dir with the Porto Alegre .pbf and .zip files>` and
//! `PEABIRU_OPTIONS_POA_FARES=<fares_poa.zip>` the properties are checked on
//! Porto Alegre with a fare structure too.

use super::search::frequency_board;
use super::*;
use crate::transit::layer::FrequencyEntry;
use crate::transit::matrix::TransitData;
use crate::{BuildOptions, TransitLayer, TransitMode};

#[test]
fn frequency_boarding_rules() {
    let e = |start, end, headway, exact| FrequencyEntry {
        start,
        end,
        headway,
        exact,
    };
    // half a headway after the traveller is ready, within the period
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, false), 0, 36_500),
        Some(36_800)
    );
    // before the period: half a headway after its start (plus the stop's offset)
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, false), 120, 30_000),
        Some(36_420)
    );
    // the expected departure past the period: no vehicle
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, false), 0, 39_400),
        None
    );
    // exact times: the first scheduled departure at or after
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, true), 0, 36_500),
        Some(36_600)
    );
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, true), 0, 36_600),
        Some(36_600)
    );
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, true), 60, 39_661),
        None
    );
    assert_eq!(
        frequency_board(&e(36_000, 39_600, 600, true), 60, 39_660),
        Some(39_660)
    );
}

/// A route every 10 minutes from 10:00 to 11:00, 5 minutes from A to B;
/// 2 minutes' walk to A, 1 minute from B. Leaving at 10:10 the traveller is
/// at A at 10:12, can board from 10:13, and waits half a headway on
/// average: on board at 10:18, at B at 10:23, at the destination at
/// 10:24. 14 minutes, and the same at every departure minute.
#[test]
fn hand_checked_expected_wait() {
    use super::search::{Config, Network, Search, StopLeg};
    use crate::transit::layer::{Pattern, TripSchedule};
    let layer = TransitLayer {
        stop_ids: vec!["f:A".into(), "f:B".into()],
        stop_lat: vec![0.0; 2],
        stop_lon: vec![0.0; 2],
        stop_vertex: vec![-1; 2],
        patterns_for_stop: vec![vec![0], vec![0]],
        services: vec![Default::default()],
        patterns: vec![Pattern {
            route_id: "R".into(),
            route_type: 3,
            stops: vec![0, 1],
            pickups: vec![true; 2],
            dropoffs: vec![true; 2],
            trips: vec![TripSchedule {
                trip_id: "t".into(),
                service: 0,
                arrivals: vec![0, 300],
                departures: vec![0, 300],
                frequencies: vec![FrequencyEntry {
                    start: 36_000,
                    end: 39_600,
                    headway: 600,
                    exact: false,
                }],
            }],
            services: vec![0],
            has_frequencies: true,
            has_schedules: false,
        }],
        has_frequencies: true,
        ..TransitLayer::default()
    };
    let transfers = vec![Vec::new(); 2];
    let net = Network::new(
        &layer,
        &transfers,
        &[true],
        &[true],
        None,
        Config {
            max_rides: 3,
            max_t: 7200,
            walk_mms: 1000,
            max_transfer_mm: i32::MAX,
            criteria: Criteria::of(&[Criterion::Time]),
            max_fare: i32::MAX,
        },
    )
    .unwrap();
    let access = [StopLeg {
        stop: 0,
        secs: 120,
        walk: 120,
    }];
    let egress = [StopLeg {
        stop: 1,
        secs: 60,
        walk: 60,
    }];
    let bounds = super::bounds::Bounds::new(&net, &egress);
    let mut s = Search::default();
    for dep in [36_600, 36_660, 37_200] {
        let a = s.run(
            &net,
            dep,
            &access,
            &egress,
            None,
            &bounds,
            &CancelToken::default(),
        );
        assert_eq!(a.len(), 1);
        assert_eq!(
            (a[0].total, a[0].rides, a[0].walk),
            (840, 1, 180),
            "at {dep}"
        );
    }
    // leaving at 10:51:40: ready at 10:54:40, expected on board at 10:59:40,
    // still within the period; leaving at 10:53:20 the expected departure
    // (11:01:20) is past its end: no trip
    let a = s.run(
        &net,
        39_100,
        &access,
        &egress,
        None,
        &bounds,
        &CancelToken::default(),
    );
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].total, 840);
    let a = s.run(
        &net,
        39_200,
        &access,
        &egress,
        None,
        &bounds,
        &CancelToken::default(),
    );
    assert!(a.is_empty());
}

fn read_points(path: &str) -> Vec<(f64, f64)> {
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

/// The values of an option on criteria `c` as `trip_options` compares them
/// (total time to a tenth of a minute, walking to the minute, rounded half
/// up), with fare -1 when absent.
fn on(c: &[Criterion], t: &TripOption, total: i32) -> Vec<i64> {
    let round = |v: i32, r: i32| i64::from((v + r / 2).div_euclid(r));
    let mut v = Vec::new();
    for k in c {
        v.push(match k {
            Criterion::Time => round(total, TIME_RESOLUTION_S),
            Criterion::Fare => t.fare.map_or(-1, |f| (f * 100.0).round() as i64),
            Criterion::Transfers => i64::from(t.transfers),
            Criterion::Walk => round(t.walk_time_s, 60),
        });
    }
    v
}

/// Per departure minute, the options on its frontier with their values.
fn by_minute(opts: &[TripOption], c: &[Criterion]) -> Vec<(i32, Vec<i64>)> {
    let mut v = Vec::new();
    for t in opts {
        for (&d, &tt) in t.departures.iter().zip(&t.totals_s) {
            v.push((d, on(c, t, tt)));
        }
    }
    v.sort();
    v
}

fn dominates(a: &[i64], b: &[i64]) -> bool {
    a.iter().zip(b).all(|(x, y)| x <= y) && a != b
}

/// The properties on one network, for the pairs `o[i]`, `d[i]`.
fn properties(
    net: &crate::StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    o: &[(f64, f64)],
    d: &[(f64, f64)],
    fares: Option<FareStructure>,
) -> usize {
    let base = TripOptionsParams {
        transit_modes: TransitMode::ALL.to_vec(),
        date: 20190513,
        departure_time: 14 * 3600,
        time_window: 5,
        fare_structure: fares.map(Arc::new),
        n_threads: 1,
        ..Default::default()
    };
    let transit = Some((layer, data));
    let run = |p: &TripOptionsParams| {
        crate::itinerary::trip_options_impl(net, transit, o, d, p).expect("options")
    };
    let all = base.resolved_criteria().unwrap();
    let full = run(&base);
    let mut n = 0;
    // threads change nothing
    let mut p4 = base.clone();
    p4.n_threads = 4;
    assert_eq!(run(&p4), full, "1 and 4 threads");
    for (i, opts) in full.iter().enumerate() {
        // no option of a minute dominates another of the same minute
        let m = by_minute(opts, &all);
        for a in &m {
            for b in &m {
                assert!(
                    a.0 != b.0 || !dominates(&a.1, &b.1),
                    "pair {i}: {a:?} dominates {b:?}"
                );
            }
        }
        // and no two with the same values as compared
        for w in m.windows(2) {
            assert!(w[0] != w[1], "pair {i}: {:?} twice", w[0]);
        }
        n += opts.len();
        // the legs add up to the values
        for t in opts {
            let it = t.itinerary.as_ref().expect("legs");
            let total: i32 = it.legs.iter().map(|l| l.duration_s + l.wait_s).sum();
            assert_eq!(total, t.total_time_s, "pair {i} option {}", t.option);
            let walk: i32 = it
                .legs
                .iter()
                .filter(|l| l.mode == "WALK")
                .map(|l| l.duration_s)
                .sum();
            assert_eq!(walk, t.walk_time_s, "pair {i} option {}", t.option);
            let rides = it.legs.iter().filter(|l| !l.route.is_empty()).count() as u32;
            assert_eq!(rides.saturating_sub(1), t.transfers, "pair {i}");
            if let (Some(f), Some(last)) = (t.fare, it.legs.last()) {
                assert_eq!(last.cumulative_fare, Some(f), "pair {i}");
            }
        }
    }
    // a criterion added keeps every vector of the smaller frontier, on the
    // smaller criteria, at every minute
    let subsets: Vec<Vec<Criterion>> = vec![
        vec![Criterion::Time],
        vec![Criterion::Time, Criterion::Transfers],
        vec![Criterion::Time, Criterion::Walk],
        vec![Criterion::Transfers, Criterion::Walk],
    ];
    for small in subsets {
        let mut p = base.clone();
        p.criteria = small.clone();
        let fewer = run(&p);
        for (i, (a, b)) in fewer.iter().zip(&full).enumerate() {
            let want = by_minute(a, &small);
            let have = by_minute(b, &small);
            for v in &want {
                assert!(
                    have.contains(v),
                    "pair {i} {small:?}: {v:?} not in {have:?}"
                );
            }
        }
    }
    // the trip limit only filters (with the time criterion)
    let mut p = base.clone();
    p.max_trip_duration = 45;
    let short = run(&p);
    // The limit applies to exact times, and what is returned is compared in
    // tenths of a minute: a trip of 45 min and 1 or 2 s counts as 45.0 but is
    // over the limit. So the trips of the full frontier within the limit are
    // all kept, and the only others the shorter limit may let through are
    // trips that such a trip (over the limit, within the rounding) beat.
    for (i, (a, b)) in short.iter().zip(&full).enumerate() {
        let have = by_minute(a, &all);
        let mut want = Vec::new();
        for t in b {
            for (&d, &tt) in t.departures.iter().zip(&t.totals_s) {
                if tt <= 45 * 60 {
                    want.push((d, on(&all, t, tt)));
                }
            }
        }
        want.sort();
        for w in &want {
            assert!(have.contains(w), "pair {i}: max_trip_duration drops {w:?}");
        }
        for h in &have {
            assert!(
                want.contains(h) || h.1[0] == 450,
                "pair {i}: max_trip_duration adds {h:?}"
            );
        }
    }
    // the fare limit only filters (with the fare criterion)
    if base.fare_structure.is_some() {
        let mut p = base.clone();
        p.max_fare = 5.0;
        let cheap = run(&p);
        let k = all.iter().position(|&c| c == Criterion::Fare).unwrap();
        for (i, (a, b)) in cheap.iter().zip(&full).enumerate() {
            let have = by_minute(a, &all);
            let want: Vec<_> = by_minute(b, &all)
                .into_iter()
                .filter(|(_, v)| v[k] <= 500)
                .collect();
            assert_eq!(have, want, "pair {i}: max_fare");
        }
    }
    n
}

#[test]
fn properties_on_sao_paulo() {
    let Some(spo) = crate::sample_data::spo() else {
        return;
    };
    let (net, layer) = crate::transit::build_from_dir(spo, &BuildOptions::default()).unwrap();
    let data = TransitData::new(&net, &layer);
    let pts = read_points(&format!("{spo}/spo_points.csv"));
    let mut rng = super::testing::Rng::new(5);
    let (mut o, mut d) = (Vec::new(), Vec::new());
    for _ in 0..12 {
        o.push(pts[rng.below(pts.len())]);
        d.push(pts[rng.below(pts.len())]);
    }
    // a flat fare by mode, with a 60-minute integration
    let mut fs = crate::fares::setup_fare_structure(&layer, 4.4, "MODE").unwrap();
    fs.transfer_time_allowance = 60.0;
    for t in &mut fs.fares_per_transfer {
        t.fare = 6.0;
    }
    let n = properties(&net, &layer, &data, &o, &d, Some(fs));
    let n0 = properties(&net, &layer, &data, &o, &d, None);
    assert!(n > 12 && n0 > 12, "{n} {n0}");
}

#[test]
fn cancelled_calls_stop() {
    let Some(spo) = crate::sample_data::spo() else {
        return;
    };
    let (net, layer) = crate::transit::build_from_dir(spo, &BuildOptions::default()).unwrap();
    let data = TransitData::new(&net, &layer);
    let pts = read_points(&format!("{spo}/spo_points.csv"));
    let p = TripOptionsParams {
        transit_modes: TransitMode::ALL.to_vec(),
        date: 20190513,
        departure_time: 14 * 3600,
        ..Default::default()
    };
    p.cancel.cancel();
    let r = crate::itinerary::trip_options_impl(&net, Some((&layer, &data)), &pts, &pts, &p);
    assert!(matches!(r, Err(RouteError::Cancelled)), "{r:?}");
    // and the fare criterion without a fare structure is an error
    let mut q = p.clone();
    q.cancel = CancelToken::default();
    q.criteria = vec![Criterion::Fare];
    let r =
        crate::itinerary::trip_options_impl(&net, Some((&layer, &data)), &pts[..1], &pts[..1], &q);
    assert!(matches!(r, Err(RouteError::Invalid(m)) if m.contains("FARE")));
}

#[test]
#[ignore = "needs PEABIRU_OPTIONS_POA and PEABIRU_OPTIONS_POA_FARES"]
fn properties_on_porto_alegre() {
    let dir = std::env::var("PEABIRU_OPTIONS_POA").expect("dir");
    let fares = std::env::var("PEABIRU_OPTIONS_POA_FARES").expect("fares");
    let (net, layer) = crate::transit::build_from_dir(&dir, &BuildOptions::default()).unwrap();
    let data = TransitData::new(&net, &layer);
    let fs = crate::read_fare_structure(&std::fs::read(fares).unwrap(), "UTF-8").unwrap();
    let pts = read_points(&std::env::var("PEABIRU_OPTIONS_POA_POINTS").expect("points"));
    let n_pairs: usize = std::env::var("PEABIRU_OPTIONS_POA_PAIRS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let mut rng = super::testing::Rng::new(9);
    let (mut o, mut d) = (Vec::new(), Vec::new());
    for _ in 0..n_pairs {
        o.push(pts[rng.below(pts.len())]);
        d.push(pts[rng.below(pts.len())]);
    }
    let n = properties(&net, &layer, &data, &o, &d, Some(fs));
    eprintln!("{n_pairs} pairs, {n} options");
}
