//! Reproductions from the pre-release review of the query-time API: public
//! functions that panic on inputs they do not validate. Each test asserts
//! the behaviour a library should have (no panic) and is `#[ignore]`d until
//! the fix lands: run with `cargo test -p peabiru-core --test
//! review_query_panics -- --ignored`.

use peabiru_core::street::link::find_split;
use peabiru_core::street::router::street_vertex_times;
use peabiru_core::street::{BuildOptions, Mode};
use peabiru_core::transit::expanded::arrival_departure;
use peabiru_core::transit::matrix::TransitData;
use peabiru_core::transit::raptor::{self, Filtered, RaptorRequest};
use std::panic::catch_unwind;

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

/// `street_vertex_times` is public and takes an unvalidated walking speed;
/// 0 km/h divides by zero (`router.rs` `distance_to_edge_mm / walk_mms`),
/// in release builds too.
#[test]
#[ignore = "review finding: panics"]
fn street_vertex_times_rejects_zero_walk_speed_without_panicking() {
    let (net, _) = peabiru_core::transit::build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let mut sc = Vec::new();
    let o = find_split(&net, -23.5563, -46.6290, Mode::Walk, &mut sc).unwrap();
    let r = catch_unwind(|| street_vertex_times(&net, &o, &[Some(o)], Mode::Walk, 0.0, 600));
    assert!(r.is_ok(), "street_vertex_times panicked on walk_speed_kmh = 0");
}

/// `raptor::route` is public; `max_duration = i32::MAX` overflows
/// `departure + max_duration` (debug panic, silent wrap in release).
#[test]
#[ignore = "review finding: panics in debug builds"]
fn raptor_route_does_not_overflow_on_a_huge_max_duration() {
    let (net, layer) = peabiru_core::transit::build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let data = TransitData::new(&net, &layer);
    let active = layer.active_services(20190513, 0);
    let f = Filtered::new(&layer, &active).unwrap();
    let req = RaptorRequest {
        from_time: 14 * 3600,
        time_window: 10,
        max_rides: 3,
        max_duration: i32::MAX,
        walk_mms: 1000,
        max_walk_min: 120,
        monte_carlo_draws: 50,
        seed: Some(1),
        origin: (0.0, 0.0),
    };
    let access: Vec<(u32, i32)> = (0..20).map(|s| (s, 60)).collect();
    let r = catch_unwind(|| raptor::route(&layer, &data.transfers, &f, &access, &req));
    assert!(r.is_ok(), "raptor::route overflowed");
}

/// `arrival_departure` is public and does not validate its date or limit:
/// month 0 underflows in `previous_day`, a huge `max_trip_duration`
/// overflows the multiplication (debug panics, garbage dates in release).
#[test]
#[ignore = "review finding: panics in debug builds"]
fn arrival_departure_does_not_overflow() {
    assert!(catch_unwind(|| arrival_departure(20190001, 0, 120)).is_ok());
    assert!(catch_unwind(|| arrival_departure(0, 0, 120)).is_ok());
    assert!(catch_unwind(|| arrival_departure(20190513, 0, i32::MAX)).is_ok());
}
