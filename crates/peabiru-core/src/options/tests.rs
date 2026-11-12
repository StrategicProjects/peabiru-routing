//! The search against an exhaustive enumeration of the same trips, on
//! random small networks: per departure minute the router must return
//! exactly the Pareto set of the enumeration (as vectors of values), and
//! every trip it returns must have those values when its legs are added up
//! again. `PEABIRU_OPTIONS_SOAK=<n>` runs `n` instances instead of the
//! committed few hundred.

use super::search::{frequency_board, Arrival, Config, Network, Search, Step, StopLeg, NONE};
use super::testing::Rng;
use super::Criteria;
use crate::fares::incremental::{FareModel, FareState};
use crate::fares::{FareCalculator, FarePerRoute, FarePerTransfer, FarePerType, FareStructure};
use crate::transit::layer::{FrequencyEntry, Pattern, RouteInfo, TripSchedule};
use crate::{CancelToken, TransitLayer};

/// A random instance: a layer, transfers, access, egress, a direct trip, a
/// fare structure and the settings.
struct Instance {
    layer: TransitLayer,
    transfers: Vec<Vec<(u32, i32)>>,
    access: Vec<StopLeg>,
    egress: Vec<StopLeg>,
    direct: Option<(i32, i32)>,
    fares: Option<FareStructure>,
    criteria: Criteria,
    max_rides: usize,
    max_t: i32,
    max_transfer_mm: i32,
    max_fare: i32,
    departure: i32,
}

const WALK_MMS: i32 = 1000;

fn random_trip(rng: &mut Rng, n: usize, start: i32) -> (Vec<i32>, Vec<i32>) {
    let mut arr = Vec::with_capacity(n);
    let mut dep = Vec::with_capacity(n);
    let mut t = start;
    for i in 0..n {
        if i > 0 {
            t += 60 + rng.below(600) as i32;
        }
        arr.push(t);
        t += rng.below(3) as i32 * 30;
        dep.push(t);
    }
    (arr, dep)
}

fn instance(seed: u64) -> Instance {
    let mut rng = Rng::new(seed);
    let departure = 36_000;
    let n_stops = 4 + rng.below(6);
    let n_patterns = 2 + rng.below(6);
    let n_routes = 1 + rng.below(n_patterns);
    let mut layer = TransitLayer {
        stop_ids: (0..n_stops).map(|s| format!("f:{s}")).collect(),
        stop_lat: vec![0.0; n_stops],
        stop_lon: vec![0.0; n_stops],
        stop_vertex: vec![-1; n_stops],
        patterns_for_stop: vec![Vec::new(); n_stops],
        services: vec![Default::default()],
        ..TransitLayer::default()
    };
    for r in 0..n_routes {
        layer.routes.push(RouteInfo {
            agency_id: Some("ag".into()),
            route_id: format!("R{r}"),
            short_name: Some(format!("{r}")),
            route_type: [3, 2][r % 2],
            ..RouteInfo::default()
        });
    }
    for pi in 0..n_patterns {
        let len = 2 + rng.below(4.min(n_stops - 1));
        let mut stops: Vec<u32> = Vec::new();
        while stops.len() < len {
            let s = rng.below(n_stops) as u32;
            // a stop may come back in a loop, but not twice in a row
            if stops.last() != Some(&s) && (rng.chance(1, 8) || !stops.contains(&s)) {
                stops.push(s);
            }
        }
        let route = rng.below(n_routes);
        let kind = rng.below(10); // 0-5 scheduled, 6-8 frequency, 9 both
        let mut trips = Vec::new();
        if kind <= 5 || kind == 9 {
            for _ in 0..1 + rng.below(6) {
                let start = departure - 300 + rng.below(3300) as i32;
                let (arrivals, departures) = random_trip(&mut rng, len, start);
                trips.push(TripSchedule {
                    trip_id: format!("p{pi}t{}", trips.len()),
                    service: 0,
                    arrivals,
                    departures,
                    frequencies: Vec::new(),
                });
            }
        }
        if kind >= 6 {
            for _ in 0..1 + rng.below(2) {
                let (arrivals, departures) = random_trip(&mut rng, len, 0);
                let mut frequencies = Vec::new();
                let mut s = departure - 1800 + rng.below(2400) as i32;
                for _ in 0..1 + rng.below(3) {
                    let end = s + 300 + rng.below(3600) as i32;
                    frequencies.push(FrequencyEntry {
                        start: s,
                        end,
                        headway: 120 + rng.below(1080) as i32,
                        exact: rng.chance(1, 3),
                    });
                    s = end - rng.below(600) as i32;
                }
                trips.push(TripSchedule {
                    trip_id: format!("p{pi}f{}", trips.len()),
                    service: 0,
                    arrivals,
                    departures,
                    frequencies,
                });
            }
        }
        // R5's order: a stable sort by first departure
        trips.sort_by_key(|t| t.departures[0]);
        let has_f = trips.iter().any(|t| t.is_frequency());
        let has_s = trips.iter().any(|t| !t.is_frequency());
        for &s in &stops {
            if !layer.patterns_for_stop[s as usize].contains(&(pi as u32)) {
                layer.patterns_for_stop[s as usize].push(pi as u32);
            }
        }
        layer.patterns.push(Pattern {
            route_id: format!("R{route}"),
            route_type: layer.routes[route].route_type,
            pickups: (0..len).map(|_| !rng.chance(1, 8)).collect(),
            dropoffs: (0..len).map(|_| !rng.chance(1, 8)).collect(),
            stops,
            trips,
            services: vec![0],
            has_frequencies: has_f,
            has_schedules: has_s,
        });
        layer.pattern_route.push(route as u32);
        layer.has_frequencies |= has_f;
        layer.has_schedules |= has_s;
    }
    let mut transfers = vec![Vec::new(); n_stops];
    for _ in 0..rng.below(3 * n_stops) {
        let (a, b) = (rng.below(n_stops), rng.below(n_stops));
        if a != b && !transfers[a].iter().any(|&(t, _)| t as usize == b) {
            transfers[a].push((b as u32, rng.below(900_000) as i32));
        }
    }
    let legs = |rng: &mut Rng| {
        let mut v: Vec<StopLeg> = Vec::new();
        for _ in 0..2 + rng.below(4) {
            let stop = rng.below(n_stops) as u32;
            if v.iter().any(|l| l.stop == stop) {
                continue;
            }
            let secs = rng.below(1200) as i32;
            let walk = if rng.chance(1, 4) { 0 } else { secs };
            v.push(StopLeg { stop, secs, walk });
        }
        v.sort_by_key(|l| l.stop);
        v
    };
    let access = legs(&mut rng);
    let egress = legs(&mut rng);
    let direct = rng.chance(1, 2).then(|| {
        let secs = 600 + rng.below(6000) as i32;
        (secs, if rng.chance(1, 2) { secs } else { 0 })
    });
    let fares = rng.chance(2, 3).then(|| random_fares(&mut rng, &layer));
    let mut criteria = Criteria::default();
    while criteria == Criteria::default() {
        criteria = Criteria {
            time: rng.chance(4, 5),
            fare: fares.is_some() && rng.chance(4, 5),
            transfers: rng.chance(3, 4),
            walk: rng.chance(3, 4),
        };
    }
    let max_fare = if fares.is_some() && rng.chance(1, 4) {
        rng.below(1500) as i32
    } else {
        i32::MAX
    };
    Instance {
        layer,
        transfers,
        access,
        egress,
        direct,
        fares,
        criteria,
        max_rides: [1, 2, 2, 3, 3, 4][rng.below(6)],
        max_t: 2400 + rng.below(6000) as i32,
        max_transfer_mm: [i32::MAX, 300_000, 600_000][rng.below(3)],
        max_fare,
        departure,
    }
}

/// A fare structure over the routes of `layer`: three types, random
/// unlimited/same-route/route-fare flags, random transfer fares (some above
/// two full fares), random caps and allowances.
fn random_fares(rng: &mut Rng, layer: &TransitLayer) -> FareStructure {
    let types = ["A", "B", "C"];
    let mut s = FareStructure {
        max_discounted_transfers: [0.0, 1.0, 1.0, 2.0, 3.0][rng.below(5)],
        transfer_time_allowance: [0.0, 10.0, 30.0, 60.0, 120.0][rng.below(5)],
        fare_cap: if rng.chance(1, 3) {
            (100 + rng.below(1400)) as f64 / 100.0
        } else {
            f64::INFINITY
        },
        ..FareStructure::default()
    };
    for t in types {
        s.fares_per_type.push(FarePerType {
            fare_type: Some(t.into()),
            unlimited_transfers: rng.chance(1, 4),
            allow_same_route_transfer: rng.chance(1, 2),
            use_route_fare: rng.chance(1, 3),
            fare: rng.below(700) as f64 / 100.0,
        });
    }
    for a in types {
        for b in types {
            if rng.chance(2, 3) {
                s.fares_per_transfer.push(FarePerTransfer {
                    first_leg: Some(a.into()),
                    second_leg: Some(b.into()),
                    fare: rng.below(1400) as f64 / 100.0,
                });
            }
        }
    }
    for r in &layer.routes {
        s.fares_per_route.push(FarePerRoute {
            agency_id: r.agency_id.clone(),
            agency_name: Some("ag".into()),
            route_id: Some(r.route_id.clone()),
            route_short_name: r.short_name.clone(),
            route_long_name: Some(String::new()),
            mode: Some("BUS".into()),
            route_fare: rng.below(800) as f64 / 100.0,
            fare_type: Some(types[rng.below(3)].into()),
        });
    }
    s
}

/// The values a trip is compared on (chosen criteria) and ranked by (the
/// tracked ones), as the search defines them.
fn values(c: &Criteria, fare_tracked: bool, a: &Arrival) -> ([i64; 4], [i64; 4]) {
    let full = [
        i64::from(a.total),
        i64::from(a.transfers()),
        if c.walk { i64::from(a.walk) } else { 0 },
        if fare_tracked { i64::from(a.fare) } else { 0 },
    ];
    let chosen = [
        if c.time { full[0] } else { 0 },
        if c.transfers { full[1] } else { 0 },
        full[2],
        if c.fare { full[3] } else { 0 },
    ];
    (chosen, full)
}

/// The Pareto set of `all`: per vector of chosen values the least ranking
/// vector, for the vectors no other dominates.
fn pareto(all: &[([i64; 4], [i64; 4])]) -> Vec<([i64; 4], [i64; 4])> {
    let mut best: std::collections::BTreeMap<[i64; 4], [i64; 4]> = Default::default();
    for &(c, f) in all {
        best.entry(c)
            .and_modify(|g| {
                if f < *g {
                    *g = f
                }
            })
            .or_insert(f);
    }
    best.iter()
        .filter(|(c, _)| !best.keys().any(|o| o != *c && (0..4).all(|k| o[k] <= c[k])))
        .map(|(c, f)| (*c, *f))
        .collect()
}

/// A partial trip of the enumeration.
#[derive(Clone)]
struct Partial {
    stop: u32,
    time: i32,
    rides: u32,
    walk: i32,
    fare: FareState,
    rides_list: Vec<(i32, i32)>,
    /// 0 access, 1 off a vehicle, 2 after a walk between stops.
    kind: u8,
}

/// Every trip of the instance (the router's trip definition, enumerated
/// without any pruning), as `Arrival`s (labels unset).
fn enumerate(inst: &Instance, calc: Option<&FareCalculator>) -> Vec<Arrival> {
    let model = calc.map(FareModel::new);
    let horizon = inst.departure + inst.max_t;
    let mut out = Vec::new();
    if let Some((secs, walk)) = inst.direct {
        out.push(Arrival {
            total: secs,
            fare: 0,
            rides: 0,
            walk,
            label: NONE,
            egress: NONE,
        });
    }
    let mut stack: Vec<Partial> = inst
        .access
        .iter()
        .map(|a| Partial {
            stop: a.stop,
            time: inst.departure + a.secs,
            rides: 0,
            walk: a.walk,
            fare: FareState::default(),
            rides_list: Vec::new(),
            kind: 0,
        })
        .filter(|p| p.time < horizon)
        .collect();
    while let Some(cur) = stack.pop() {
        if cur.kind == 1 {
            if let Some(e) = inst.egress.iter().find(|e| e.stop == cur.stop) {
                out.push(Arrival {
                    total: cur.time + e.secs - inst.departure,
                    fare: model.as_ref().map_or(0, |m| m.total(&cur.fare)),
                    rides: cur.rides,
                    walk: cur.walk + e.walk,
                    label: 0,
                    egress: 0,
                });
            }
            if (cur.rides as usize) < inst.max_rides {
                for &(to, mm) in &inst.transfers[cur.stop as usize] {
                    if mm >= inst.max_transfer_mm {
                        continue;
                    }
                    let secs = mm / WALK_MMS;
                    let t = cur.time + secs;
                    if t < horizon {
                        let mut n = cur.clone();
                        n.stop = to;
                        n.time = t;
                        n.walk += secs;
                        n.kind = 2;
                        stack.push(n);
                    }
                }
            }
        }
        if cur.rides as usize >= inst.max_rides {
            continue;
        }
        for (pi, pat) in inst.layer.patterns.iter().enumerate() {
            for pos in 0..pat.stops.len() - 1 {
                if pat.stops[pos] != cur.stop || !pat.pickups[pos] {
                    continue;
                }
                let earliest = cur.time + 60;
                // (board time, arrival per stop) of every vehicle
                let mut cands: Vec<(i32, Vec<i32>)> = Vec::new();
                for t in &pat.trips {
                    if t.is_frequency() {
                        for f in &t.frequencies {
                            if let Some(b) = frequency_board(f, t.departures[pos], earliest) {
                                let vs = b - t.departures[pos];
                                cands.push((b, t.arrivals.iter().map(|a| vs + a).collect()));
                            }
                        }
                    } else if t.departures[pos] > earliest {
                        cands.push((t.departures[pos], t.arrivals.clone()));
                    }
                }
                for j in pos + 1..pat.stops.len() {
                    if !pat.dropoffs[j] {
                        continue;
                    }
                    let Some((b, arr)) = cands
                        .iter()
                        .map(|(b, a)| (*b, a[j]))
                        .min_by_key(|&(b, a)| (a, b))
                    else {
                        continue;
                    };
                    if arr >= horizon {
                        continue;
                    }
                    let mut n = cur.clone();
                    n.stop = pat.stops[j];
                    n.time = arr;
                    n.rides += 1;
                    n.kind = 1;
                    n.rides_list.push((pi as i32, b));
                    if let Some(m) = &model {
                        n.fare = m.step(&cur.fare, pi as i32, b);
                    }
                    stack.push(n);
                }
            }
        }
    }
    out
}

/// One instance: the router at the departure minute against the
/// enumeration, and each trip's values recomputed from its legs.
fn check(seed: u64) -> (usize, usize) {
    let inst = instance(seed);
    let calc = inst
        .fares
        .as_ref()
        .map(|s| FareCalculator::new(s, &inst.layer).expect("valid structure"));
    let fare_tracked = calc.is_some() && (inst.criteria.fare || inst.max_fare < i32::MAX);
    let model = if fare_tracked {
        calc.as_ref().map(FareModel::new)
    } else {
        None
    };
    let cfg = Config {
        max_rides: inst.max_rides,
        max_t: inst.max_t,
        walk_mms: WALK_MMS,
        max_transfer_mm: inst.max_transfer_mm,
        criteria: inst.criteria,
        max_fare: inst.max_fare,
    };
    let allowed = vec![true; inst.layer.patterns.len()];
    let net =
        Network::new(&inst.layer, &inst.transfers, &allowed, &[true], model, cfg).expect("network");
    let mut search = Search::default();
    let bounds = super::bounds::Bounds::new(&net, &inst.egress);
    let got = search.run(
        &net,
        inst.departure,
        &inst.access,
        &inst.egress,
        inst.direct,
        &bounds,
        &CancelToken::default(),
    );
    // the enumeration, filtered by the limits, its fares the calculator's
    let mut all: Vec<Arrival> = enumerate(&inst, calc.as_ref());
    all.retain(|a| a.total <= inst.max_t && (!fare_tracked || a.fare <= inst.max_fare));
    if !fare_tracked {
        all.iter_mut().for_each(|a| a.fare = 0);
    }
    let want = pareto(
        &all.iter()
            .map(|a| values(&inst.criteria, fare_tracked, a))
            .collect::<Vec<_>>(),
    );
    let mut have: Vec<_> = got
        .iter()
        .map(|a| values(&inst.criteria, fare_tracked, a))
        .collect();
    have.sort();
    assert_eq!(
        have, want,
        "seed {seed}: router {:?}\nenumeration {:?}",
        got, want
    );
    for a in &got {
        recheck(&inst, &search, a, calc.as_ref(), fare_tracked, seed);
    }
    // the exact frontier filtered with walking and total time rounded keeps
    // exactly the coarse Pareto set of every trip, one trip per vector
    let mut rng = Rng::new(seed ^ 0xc0a5);
    let walk_res = [1, 7, 30, 60, 300][rng.below(5)];
    let time_res = [1, 6, 60][rng.below(3)];
    let kept = net.coarse_frontier(&got, walk_res, time_res);
    let coarse = |a: &Arrival| -> [i64; 4] {
        let (c, _) = values(&inst.criteria, fare_tracked, a);
        let round = |v: i64, r: i64| (v + r / 2).div_euclid(r);
        [
            if inst.criteria.time {
                round(c[0], i64::from(time_res))
            } else {
                0
            },
            c[1],
            if inst.criteria.walk {
                round(c[2], i64::from(walk_res))
            } else {
                0
            },
            c[3],
        ]
    };
    let mut have: Vec<[i64; 4]> = kept.iter().map(coarse).collect();
    have.sort_unstable();
    let n = have.len();
    have.dedup();
    assert_eq!(
        n,
        have.len(),
        "seed {seed}: two trips with the same coarse values"
    );
    let all_coarse: Vec<[i64; 4]> = all.iter().map(coarse).collect();
    let mut want: Vec<[i64; 4]> = all_coarse
        .iter()
        .filter(|v| {
            !all_coarse
                .iter()
                .any(|o| o != *v && (0..4).all(|k| o[k] <= v[k]))
        })
        .copied()
        .collect();
    want.sort_unstable();
    want.dedup();
    assert_eq!(
        have, want,
        "seed {seed}: coarse frontier ({walk_res} s, {time_res} s)"
    );
    // of trips with the same coarse values, the first in exact terms
    for k in &kept {
        let ck = coarse(k);
        let fk = values(&inst.criteria, fare_tracked, k).1;
        for o in &got {
            if coarse(o) == ck {
                assert!(
                    fk <= values(&inst.criteria, fare_tracked, o).1,
                    "seed {seed}"
                );
            }
        }
    }
    (got.len(), all.len())
}

/// Add up the legs of trip `a` again: times, waits, rides, fare and time on
/// foot must be its values, and each ride must be the vehicle the trip
/// definition takes.
fn recheck(
    inst: &Instance,
    s: &Search,
    a: &Arrival,
    calc: Option<&FareCalculator>,
    fare_tracked: bool,
    seed: u64,
) {
    if a.label == NONE {
        let (secs, walk) = inst.direct.expect("direct");
        assert_eq!((a.total, a.walk, a.rides), (secs, walk, 0), "seed {seed}");
        return;
    }
    let mut chain = Vec::new();
    let mut k = a.label;
    while k != NONE {
        chain.push(s.arena[k as usize]);
        k = s.arena[k as usize].back;
    }
    chain.reverse();
    let first = chain[0];
    assert_eq!(first.step, Step::Access, "seed {seed}");
    let acc = inst
        .access
        .iter()
        .find(|l| l.stop == first.stop)
        .expect("access stop");
    let mut clock = inst.departure + acc.secs;
    let mut walk = acc.walk;
    let mut rides = Vec::new();
    for l in &chain[1..] {
        match l.step {
            Step::Ride {
                pattern,
                board_pos,
                alight_pos,
                board_time,
            } => {
                let pat = &inst.layer.patterns[pattern as usize];
                assert_eq!(
                    pat.stops[board_pos as usize],
                    stop_of(&chain, l),
                    "seed {seed}"
                );
                assert!(board_time >= clock + 60, "seed {seed}: boards too early");
                // the vehicle that reaches the stop first
                let mut best = None;
                for t in &pat.trips {
                    let bp = board_pos as usize;
                    let ap = alight_pos as usize;
                    let opts: Vec<(i32, i32)> = if t.is_frequency() {
                        t.frequencies
                            .iter()
                            .filter_map(|f| frequency_board(f, t.departures[bp], clock + 60))
                            .map(|b| (b - t.departures[bp] + t.arrivals[ap], b))
                            .collect()
                    } else if t.departures[bp] > clock + 60 {
                        vec![(t.arrivals[ap], t.departures[bp])]
                    } else {
                        Vec::new()
                    };
                    for o in opts {
                        if best.map_or(true, |b| o < b) {
                            best = Some(o);
                        }
                    }
                }
                assert_eq!(
                    best,
                    Some((l.time, board_time)),
                    "seed {seed}: not the first vehicle"
                );
                rides.push((pattern as i32, board_time));
                clock = l.time;
            }
            Step::Transfer => {
                let from = stop_of(&chain, l);
                let mm = inst.transfers[from as usize]
                    .iter()
                    .find(|&&(t, _)| t == l.stop)
                    .expect("a transfer")
                    .1;
                assert!(mm < inst.max_transfer_mm, "seed {seed}");
                clock += mm / WALK_MMS;
                walk += mm / WALK_MMS;
                assert_eq!(clock, l.time, "seed {seed}");
            }
            Step::Access => panic!("seed {seed}: access inside a trip"),
        }
    }
    let last = chain[chain.len() - 1];
    let e = inst
        .egress
        .iter()
        .find(|l| l.stop == last.stop)
        .expect("egress stop");
    assert_eq!(
        clock + e.secs - inst.departure,
        a.total,
        "seed {seed}: total"
    );
    assert_eq!(walk + e.walk, a.walk, "seed {seed}: walk");
    assert_eq!(rides.len() as u32, a.rides, "seed {seed}: rides");
    if fare_tracked {
        let f = calc.expect("calc").fare(&rides).paid;
        assert_eq!(f, a.fare, "seed {seed}: fare");
    }
}

/// The stop of the label behind `l` in `chain`.
fn stop_of(chain: &[super::search::Label], l: &super::search::Label) -> u32 {
    let k = chain
        .iter()
        .position(|c| std::ptr::eq(c, l))
        .expect("in chain");
    chain[k - 1].stop
}

#[test]
fn router_matches_the_enumeration() {
    let n: u64 = std::env::var("PEABIRU_OPTIONS_SOAK")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500);
    let mut options = 0;
    let mut trips = 0;
    let (mut multi, mut big) = (0, 0);
    for seed in 0..n {
        let (o, t) = check(seed);
        multi += usize::from(o >= 2);
        big += usize::from(o >= 5);
        options += o;
        trips += t;
    }
    eprintln!(
        "{n} instances: {options} options of {trips} trips enumerated; {multi} instances with \
         two or more options, {big} with five or more"
    );
    assert!(multi > n as usize / 3);
}
