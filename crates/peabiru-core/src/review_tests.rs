//! Query-time functions of the crate on inputs they cannot compute: each
//! returns an error instead of panicking or overflowing. (Moved here from
//! `tests/review_query_panics.rs` when these functions stopped being
//! public; the test of the public `arrival_departure` stayed there.)

use crate::street::link::find_split;
use crate::street::router::street_vertex_times;
use crate::street::{BuildOptions, Mode};
use crate::transit::matrix::TransitData;
use crate::transit::raptor::{self, Filtered, RaptorRequest};
use std::panic::catch_unwind;

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

/// `street_vertex_times` takes a walking speed; 0 km/h used to divide by
/// zero (`distance_to_edge_mm / walk_mms`), in release builds too.
#[test]
fn street_vertex_times_rejects_zero_walk_speed_without_panicking() {
    let (net, _) = crate::build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let mut sc = Vec::new();
    let o = find_split(&net, -23.5563, -46.6290, Mode::Walk, &mut sc).unwrap();
    let r = catch_unwind(|| street_vertex_times(&net, &o, &[Some(o)], Mode::Walk, 0.0, 600));
    let r = r.expect("street_vertex_times panicked on walk_speed_kmh = 0");
    assert!(r.unwrap_err().to_string().contains("walk_speed_kmh"));
    let (times, direct) = street_vertex_times(&net, &o, &[Some(o)], Mode::Walk, 3.6, 600).unwrap();
    assert_eq!(times.len(), net.n_vertices());
    assert_eq!(direct.len(), 1);
}

fn request(max_duration: i32, time_window: i32) -> RaptorRequest {
    RaptorRequest {
        from_time: 14 * 3600,
        time_window,
        max_rides: 3,
        max_duration,
        walk_mms: 1000,
        max_walk_min: 120,
        monte_carlo_draws: 50,
        seed: Some(1),
        origin: (0.0, 0.0),
    }
}

/// `raptor::route` with `max_duration = i32::MAX` used to overflow
/// `departure + max_duration`; a window of 0 minutes made
/// `iterations_per_minute` return usize::MAX.
#[test]
fn raptor_route_does_not_overflow_on_a_huge_max_duration() {
    let (net, layer) = crate::build_from_dir(SPO, &BuildOptions::default()).unwrap();
    let data = TransitData::new(&net, &layer);
    let active = layer.active_services(20190513, 0);
    let f = Filtered::new(&layer, &active, &crate::TransitMode::ALL).unwrap();
    let access: Vec<(u32, i32)> = (0..20).map(|s| (s, 60)).collect();
    let route = |req: &RaptorRequest, access: &[(u32, i32)]| {
        catch_unwind(|| raptor::route(&layer, &data.transfers, &f, access, req))
            .expect("raptor::route panicked")
    };
    let err = route(&request(i32::MAX, 10), &access).unwrap_err();
    assert!(err.contains("max_duration"), "{err}");
    assert!(route(&request(7200, 0), &access).is_err());
    assert!(request(7200, 0).iterations_per_minute(true).is_err());
    assert_eq!(request(7200, 10).iterations_per_minute(true), Ok(5));
    // access to a stop the layer does not have, or at a negative time
    assert!(route(&request(7200, 10), &[(u32::MAX, 60)]).is_err());
    assert!(route(&request(7200, 10), &[(0, -1)]).is_err());
    // the largest max_duration that fits still runs
    let fits = i32::MAX - 14 * 3600 - 10 * 60;
    let times = route(&request(fits, 10), &access).unwrap();
    assert_eq!(times.len(), 10 * 5);
    let (paths_times, _) = raptor::route_with_paths(
        &layer,
        &data.transfers,
        &f,
        &access,
        &request(7200, 10),
        &[],
    )
    .unwrap();
    assert_eq!(paths_times, route(&request(7200, 10), &access).unwrap());
}
