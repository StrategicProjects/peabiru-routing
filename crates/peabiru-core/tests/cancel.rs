//! Cancelling the long computations through their parameters' token.

use peabiru_core::street::{self, BuildOptions, Mode, RouteParams, StreetNetwork};
use peabiru_core::transit::accessibility::{
    street_accessibility, transit_accessibility, AccessibilityParams, Destinations,
};
use peabiru_core::transit::decay::DecayFunction;
use peabiru_core::transit::expanded::{arrival_travel_time_matrix, expanded_travel_time_matrix};
use peabiru_core::transit::matrix::{transit_travel_time_matrix, TransitData, TransitParams};
use peabiru_core::transit::TransitLayer;
use peabiru_core::{CancelToken, RouteError};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

const SPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../r/inst/extdata/spo");

struct Sample {
    net: StreetNetwork,
    layer: TransitLayer,
    data: TransitData,
    points: Vec<(f64, f64)>,
    jobs: Vec<Vec<f64>>,
}

fn sample() -> &'static Sample {
    static SAMPLE: OnceLock<Sample> = OnceLock::new();
    SAMPLE.get_or_init(|| {
        let (net, layer) =
            peabiru_core::transit::build_from_dir(SPO, &BuildOptions::default()).unwrap();
        let data = TransitData::new(&net, &layer);
        let text = std::fs::read_to_string(format!("{SPO}/spo_points.csv")).unwrap();
        let (mut points, mut jobs) = (Vec::new(), Vec::new());
        for line in text.lines().skip(1) {
            let f: Vec<&str> = line.split(',').collect();
            points.push((f[2].parse().unwrap(), f[1].parse().unwrap()));
            jobs.push(f[5].parse().unwrap());
        }
        Sample {
            net,
            layer,
            data,
            points,
            jobs: vec![jobs],
        }
    })
}

fn transit_params(time_window: i32, draws_per_minute: i32, cancel: &CancelToken) -> TransitParams {
    TransitParams {
        date: 20190513,
        departure_time: 0,
        time_window,
        max_trip_duration: time_window,
        draws_per_minute,
        seed: Some(1),
        n_threads: 1,
        cancel: cancel.clone(),
        ..TransitParams::default()
    }
}

/// A computation run with a token, its result dropped.
type Call = Box<dyn Fn(&CancelToken) -> Result<(), RouteError>>;

/// Every long computation, with `cancel` in its parameters, mapped to
/// `Ok(())` or its error. `heavy` makes each take seconds.
fn calls(heavy: bool) -> Vec<(&'static str, Call)> {
    let s = sample();
    let (tw, dpm) = if heavy { (1440, 20) } else { (10, 1) };
    let reps = if heavy { 5000 } else { 1 };
    let many: Vec<(f64, f64)> = s.points.repeat(reps);
    let mut a = AccessibilityParams::default();
    a.cutoffs = vec![30];
    a.decay = DecayFunction::Step;
    let route = |cancel: &CancelToken| RouteParams {
        n_threads: 1,
        cancel: cancel.clone(),
        ..RouteParams::default()
    };
    let many2 = many.clone();
    let a2 = a.clone();
    vec![
        (
            "street matrix",
            Box::new(move |c: &CancelToken| {
                street::street_travel_time_matrix(&s.net, &many, &s.points, Mode::Car, &route(c))
                    .map(drop)
            }),
        ),
        (
            "street accessibility",
            Box::new(move |c: &CancelToken| {
                let dest = Destinations::new(&s.points, &s.jobs);
                street_accessibility(&s.net, &many2, &dest, Mode::Car, &route(c), &[50], &a)
                    .map(drop)
            }),
        ),
        (
            "transit matrix",
            Box::new(move |c: &CancelToken| {
                let p = transit_params(tw, dpm, c);
                transit_travel_time_matrix(&s.net, &s.layer, &s.data, &s.points, &s.points, &p)
                    .map(drop)
            }),
        ),
        (
            "transit accessibility",
            Box::new(move |c: &CancelToken| {
                let dest = Destinations::new(&s.points, &s.jobs);
                let p = transit_params(tw, dpm, c);
                transit_accessibility(&s.net, &s.layer, &s.data, &s.points, &dest, &p, &a2)
                    .map(drop)
            }),
        ),
        (
            "expanded matrix",
            Box::new(move |c: &CancelToken| {
                let p = transit_params(tw, dpm, c);
                expanded_travel_time_matrix(&s.net, &s.layer, &s.data, &s.points, &s.points, &p)
                    .map(drop)
            }),
        ),
        (
            "arrival matrix",
            Box::new(move |c: &CancelToken| {
                let p = transit_params(tw, dpm, c);
                arrival_travel_time_matrix(&s.net, &s.layer, &s.data, &s.points, &s.points, &p)
                    .map(drop)
            }),
        ),
    ]
}

/// Run `f` with a token cancelled from another thread after `after`;
/// returns its result and how long it took past the cancellation.
fn cancel_after(
    after: Duration,
    f: impl FnOnce(&CancelToken) -> Result<(), RouteError>,
) -> (Result<(), RouteError>, Duration) {
    let cancel = CancelToken::new();
    let remote = cancel.clone();
    let timer = std::thread::spawn(move || {
        std::thread::sleep(after);
        remote.cancel();
        Instant::now()
    });
    let r = f(&cancel);
    let end = Instant::now();
    let fired = timer.join().unwrap();
    (r, end.saturating_duration_since(fired))
}

#[test]
fn a_cancelled_token_stops_every_computation() {
    for (name, f) in calls(false) {
        assert_eq!(f(&CancelToken::new()), Ok(()), "{name}");
        let cancelled = CancelToken::new();
        cancelled.cancel();
        assert_eq!(f(&cancelled), Err(RouteError::Cancelled), "{name}");
    }
}

#[test]
fn a_running_computation_stops_soon_after_the_cancel() {
    for (name, f) in calls(true) {
        let (r, late) = cancel_after(Duration::from_millis(200), f);
        assert_eq!(r, Err(RouteError::Cancelled), "{name}");
        assert!(late < Duration::from_millis(500), "{name}: {late:?}");
    }
}

/// One origin whose search alone takes seconds: the search itself checks.
#[test]
fn a_single_slow_origin_stops_soon_after_the_cancel() {
    let s = sample();
    let one = &s.points[..1];
    let (r, late) = cancel_after(Duration::from_millis(200), |c| {
        let p = transit_params(1440, 100, c);
        transit_travel_time_matrix(&s.net, &s.layer, &s.data, one, &s.points, &p).map(drop)
    });
    assert_eq!(r, Err(RouteError::Cancelled));
    assert!(late < Duration::from_millis(500), "{late:?}");
    let (r, late) = cancel_after(Duration::from_millis(200), |c| {
        let p = transit_params(1440, 20, c);
        expanded_travel_time_matrix(&s.net, &s.layer, &s.data, one, &s.points, &p).map(drop)
    });
    assert_eq!(r, Err(RouteError::Cancelled));
    assert!(late < Duration::from_millis(500), "{late:?}");
}

#[test]
fn invalid_parameters_are_not_a_cancellation() {
    let s = sample();
    let p = TransitParams {
        date: 20190230,
        ..TransitParams::default()
    };
    let e = transit_travel_time_matrix(&s.net, &s.layer, &s.data, &s.points, &s.points, &p)
        .unwrap_err();
    assert!(
        matches!(e, RouteError::Invalid(ref m) if m.contains("date")),
        "{e}"
    );
}
