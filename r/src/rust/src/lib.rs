//! R bindings of the peabiru routing core.
//!
//! Everything that mirrors r5r's R wrapper (argument checks, defaults,
//! datetime handling, trip limits, column assembly) lives in the R code of
//! the package; these functions only take already validated values, run the
//! core and hand back plain vectors. Points are passed as separate latitude
//! and longitude vectors; row indices returned to R are 1-based.
//!
//! A routing call runs the core on a thread of its own while the R thread
//! checks every [`POLL`] for a user interrupt (see [`interrupt`]). On an
//! interrupt the computation is cancelled and, once its workers have
//! stopped, the function returns a `peabiru_stopped` object holding the
//! condition, which the R code raises with `raise_if_stopped()`: no R
//! longjmp ever crosses the Rust frames.

use extendr_api::prelude::*;
use extendr_api::{Error, Result};
use peabiru_core::{
    arrival_travel_time_matrix, build_from_dir, expanded_travel_time_matrix, isochrone_lines,
    isochrone_polygons, street_accessibility, street_edges, street_travel_time_matrix,
    street_travel_time_surface, transit_accessibility, transit_travel_time_matrix,
    transit_travel_time_surface, validate_date, AccessibilityParams, BuildOptions, CancelToken,
    CsvOutput, DecayFunction, Destinations, Mode, RouteError, RouteParams, StreetNetwork,
    TransitData, TransitLayer, TransitMode, TransitParams,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

mod fares;
mod inspect;
mod itinerary;
mod options;
mod scenario;

/// A network built once and shared by every routing call.
struct Network {
    net: StreetNetwork,
    /// Present when the data directory had at least one GTFS feed.
    transit: Option<Transit>,
}

struct Transit {
    layer: TransitLayer,
    data: TransitData,
}

fn err(msg: impl std::fmt::Display) -> Error {
    Error::Other(msg.to_string())
}

/// The error of a failed build; for feeds with errors of priority `HIGH`,
/// r5r's message, pointing to `lister` to see them.
fn build_error(e: &peabiru_core::NetworkError, lister: &str) -> String {
    match e.high_priority_gtfs_errors() {
        None => e.to_string(),
        Some(feeds) => {
            let list: Vec<String> = feeds
                .iter()
                .map(|(p, n)| {
                    let name = p.file_name().map_or_else(
                        || p.display().to_string(),
                        |f| f.to_string_lossy().into_owned(),
                    );
                    format!("{name}: {n} error{}", if *n == 1 { "" } else { "s" })
                })
                .collect();
            format!(
                "High priority GTFS errors found; network build failed ({}). Use {lister} to see them.",
                list.join(", ")
            )
        }
    }
}

/// Checking for an R interrupt without letting R's longjmp cross Rust
/// frames.
///
/// `R_CheckUserInterrupt` runs inside `R_tryCatch`, which hands the
/// `interrupt` condition (Ctrl-C, RStudio's Stop button), or an error raised
/// by the same check (a limit set by `setTimeLimit()`), to a handler that
/// returns it; the whole runs under `R_ToplevelExec`, so a jump that
/// escapes the `tryCatch` (an interrupt while R sets it up) still ends
/// there. The functions R calls back are plain C-ABI frames that own
/// nothing.
mod interrupt {
    use extendr_api::prelude::*;
    use extendr_api::SEXP;
    use std::os::raw::{c_int, c_void};
    use std::ptr;

    /// A `SEXP` as the declarations below take it (`SEXPREC` is opaque).
    type Sexp = *mut c_void;

    extern "C" {
        fn R_CheckUserInterrupt();
        fn R_tryCatch(
            body: unsafe extern "C" fn(*mut c_void) -> Sexp,
            bdata: *mut c_void,
            conds: Sexp,
            handler: unsafe extern "C" fn(Sexp, *mut c_void) -> Sexp,
            hdata: *mut c_void,
            finally: unsafe extern "C" fn(*mut c_void),
            fdata: *mut c_void,
        ) -> Sexp;
        fn R_ToplevelExec(fun: unsafe extern "C" fn(*mut c_void), data: *mut c_void) -> c_int;
        fn R_PreserveObject(x: Sexp);
        fn R_ReleaseObject(x: Sexp);
    }

    /// What [`check`] reads and writes.
    struct Slots {
        /// `NULL`, returned by the body when nothing was raised.
        nil: Sexp,
        /// `c("interrupt", "error")`.
        conds: Sexp,
        /// The condition caught, preserved; null if none.
        caught: Sexp,
    }

    unsafe extern "C" fn body(nil: *mut c_void) -> Sexp {
        R_CheckUserInterrupt();
        nil
    }

    unsafe extern "C" fn handler(cond: Sexp, _: *mut c_void) -> Sexp {
        cond
    }

    unsafe extern "C" fn finally(_: *mut c_void) {}

    unsafe extern "C" fn check(data: *mut c_void) {
        let slots = &mut *(data as *mut Slots);
        let r = R_tryCatch(
            body,
            slots.nil,
            slots.conds,
            handler,
            ptr::null_mut(),
            finally,
            ptr::null_mut(),
        );
        if r != slots.nil {
            R_PreserveObject(r);
            slots.caught = r;
        }
    }

    /// The condition of a pending interrupt (or of an error raised while
    /// checking for one); `NULL` for an interrupt whose condition was lost;
    /// `None` when there is nothing pending. Call on the R thread only.
    pub fn pending() -> Option<Robj> {
        let nil = Robj::from(());
        let conds = Robj::from(vec!["interrupt", "error"]);
        let mut slots = Slots {
            nil: unsafe { nil.get() } as Sexp,
            conds: unsafe { conds.get() } as Sexp,
            caught: ptr::null_mut(),
        };
        let completed = single_threaded(|| unsafe {
            R_ToplevelExec(check, &mut slots as *mut Slots as *mut c_void)
        });
        if !slots.caught.is_null() {
            let cond = unsafe { Robj::from_sexp(slots.caught as SEXP) };
            unsafe { R_ReleaseObject(slots.caught) };
            Some(cond)
        } else if completed == 0 {
            Some(nil)
        } else {
            None
        }
    }
}

/// How often a routing call checks for an R interrupt while the core runs.
const POLL: Duration = Duration::from_millis(50);

/// The end of a routing call: its result, or the condition that stopped it.
enum Outcome<T> {
    Done(T),
    Stopped(Robj),
}

/// Tells the R thread that the work has ended, also when it ended in a
/// panic: dropped by the worker, it sets the flag and wakes the caller. The
/// caller waits on the flag and not on the thread having exited, which
/// happens a moment after the wake-up and would cost it a whole [`POLL`].
struct Wake<'a> {
    done: &'a AtomicBool,
    caller: std::thread::Thread,
}

impl Drop for Wake<'_> {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        self.caller.unpark();
    }
}

/// Run `work` on a thread of its own and wait for it, checking for an R
/// interrupt every [`POLL`]. On one, `cancel` is cancelled and the workers
/// are waited for. `work` must hand `cancel` to the core.
fn interruptible<T: Send>(
    cancel: &CancelToken,
    work: impl FnOnce() -> std::result::Result<T, RouteError> + Send,
) -> Result<Outcome<T>> {
    let done = AtomicBool::new(false);
    let wake = Wake {
        done: &done,
        caller: std::thread::current(),
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(move || {
            let _wake = wake;
            work()
        });
        let mut stopped = None;
        while !done.load(Ordering::Acquire) {
            std::thread::park_timeout(POLL);
            if stopped.is_none() && !done.load(Ordering::Acquire) {
                if let Some(cond) = interrupt::pending() {
                    cancel.cancel();
                    stopped = Some(cond);
                }
            }
        }
        let r = worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        match (stopped, r) {
            (Some(cond), _) => Ok(Outcome::Stopped(cond)),
            (None, Ok(v)) => Ok(Outcome::Done(v)),
            (None, Err(e)) => Err(err(e)),
        }
    })
}

/// What a routing function returns to R when an interrupt stopped it:
/// `raise_if_stopped()` raises `condition` in R.
fn stopped(condition: Robj) -> Result<Robj> {
    let mut out: Robj = list!(condition = condition).into();
    out.set_class(&["peabiru_stopped"])?;
    Ok(out)
}

/// Unwrap an [`Outcome`] inside a function returning `Result<Robj>`.
macro_rules! done {
    ($outcome:expr) => {
        match $outcome {
            Outcome::Done(v) => v,
            Outcome::Stopped(cond) => return stopped(cond),
        }
    };
}

fn network(ptr: &Robj) -> Result<&Network> {
    let p: &ExternalPtr<Network> = ptr
        .try_into()
        .map_err(|_| err("not a peabiru network (was it saved and reloaded? build it again)"))?;
    p.try_addr()
        .map_err(|_| err("this network is no longer valid; build it again"))
}

fn transit(n: &Network) -> Result<&Transit> {
    n.transit
        .as_ref()
        .ok_or_else(|| err("the network has no public transport (no GTFS feed)"))
}

fn threads(n: i32) -> usize {
    n.max(0) as usize
}

fn points(lat: &[f64], lon: &[f64]) -> Vec<(f64, f64)> {
    lat.iter().zip(lon).map(|(&a, &b)| (a, b)).collect()
}

/// Where and for which ids to write CSV files instead of returning the
/// result (r5r's `output_dir`), from the `output` argument of a routing
/// function: `NULL`, or a list with `dir`, `from_ids`, `to_ids` and the
/// extra element the function needs (`percentiles`, `breakdown` or
/// `opportunities`).
struct Output {
    dir: String,
    from_ids: Vec<String>,
    to_ids: Vec<String>,
    spec: Robj,
}

impl Output {
    fn of(x: &Robj) -> Result<Option<Output>> {
        if x.is_null() {
            return Ok(None);
        }
        if !x.is_list() {
            return Err(err("output must be NULL or a list"));
        }
        let spec = x.clone();
        let strings = |name: &str| -> Result<Vec<String>> {
            let v: Strings = spec
                .dollar(name)
                .map_err(err)?
                .try_into()
                .map_err(|_| err(format!("output${name} must be a character vector")))?;
            Ok(v.iter().map(|s| <str>::to_owned(s)).collect())
        };
        Ok(Some(Output {
            dir: spec
                .dollar("dir")
                .map_err(err)?
                .as_str()
                .ok_or_else(|| err("output$dir must be a string"))?
                .to_owned(),
            from_ids: strings("from_ids")?,
            to_ids: strings("to_ids")?,
            spec,
        }))
    }

    fn writer(&self, n_threads: i32) -> CsvOutput<'_> {
        CsvOutput::new(&self.dir, &self.from_ids, &self.to_ids, threads(n_threads))
    }

    fn get(&self, name: &str) -> Result<Robj> {
        self.spec.dollar(name).map_err(err)
    }
}

/// A file that could not be written, as a routing error.
fn io_err(e: std::io::Error) -> RouteError {
    RouteError::from(e.to_string())
}

/// Version of the Rust core this package was built against.
/// @noRd
#[extendr]
fn rs_core_version() -> &'static str {
    peabiru_core::VERSION
}

/// Build a network from a directory with one `.pbf`, any GTFS zips and an
/// optional `.tif` elevation raster (used unless `elevation` is `"NONE"`).
/// @noRd
#[extendr]
fn rs_build_network(data_path: &str, n_threads: i32, elevation: &str) -> Result<Robj> {
    let mut opts = BuildOptions::default();
    opts.n_threads = threads(n_threads);
    opts.elevation = peabiru_core::Elevation::from_r5r_name(elevation)
        .ok_or_else(|| err(format!("unknown elevation model {elevation:?}")))?;
    let lister = format!("get_gtfs_errors({data_path:?})");
    let (net, layer) =
        build_from_dir(data_path, &opts).map_err(|e| err(build_error(&e, &lister)))?;
    let transit = if layer.feed_ids().is_empty() {
        None
    } else {
        let data = TransitData::new(&net, &layer);
        Some(Transit { layer, data })
    };
    let ptr = ExternalPtr::new(Network { net, transit });
    Ok(ptr.into())
}

/// Whether `ptr` points to a live network.
/// @noRd
#[extendr]
fn rs_network_valid(ptr: Robj) -> bool {
    network(&ptr).is_ok()
}

/// Warnings about the network build: one per GTFS feed that r5r refuses
/// to build a network from, then those about the elevation raster.
/// @noRd
#[extendr]
fn rs_network_r5r_warnings(ptr: Robj) -> Result<Vec<String>> {
    let n = network(&ptr)?;
    let mut w = n
        .transit
        .as_ref()
        .map_or_else(Vec::new, |t| t.layer.r5r_warnings());
    w.extend(n.net.elevation_warnings().iter().cloned());
    Ok(w)
}

/// Sizes of the network, for `print()`.
/// @noRd
#[extendr]
fn rs_network_info(ptr: Robj) -> Result<Robj> {
    let n = network(&ptr)?;
    let (feeds, stops, patterns, services, freq) = match &n.transit {
        Some(t) => (
            t.layer.feed_ids().to_vec(),
            t.layer.n_stops(),
            t.layer.n_patterns(),
            t.layer.n_services(),
            t.layer.has_frequencies(),
        ),
        None => (Vec::new(), 0, 0, 0, false),
    };
    Ok(list!(
        vertices = n.net.n_vertices() as f64,
        edges = n.net.n_edges() as f64,
        feeds = feeds,
        stops = stops as f64,
        patterns = patterns as f64,
        services = services as f64,
        frequencies = freq,
        elevation = n.net.elevation().r5r_name()
    )
    .into())
}

/// Number of services in the network and of those active on `date`
/// (`yyyymmdd`), as r5r's `getTransitServiceAvailability`.
/// @noRd
#[extendr]
fn rs_transit_availability(ptr: Robj, date: i32) -> Result<Vec<f64>> {
    let n = network(&ptr)?;
    let date = u32::try_from(date).unwrap_or(0);
    validate_date(date).map_err(err)?;
    let Some(t) = &n.transit else {
        return Ok(vec![0.0, 0.0]);
    };
    let active = t.layer.n_services_active_on(date).map_err(err)?;
    Ok(vec![t.layer.n_services() as f64, active as f64])
}

fn street_mode(mode: &str) -> Result<Mode> {
    match mode {
        "WALK" => Ok(Mode::Walk),
        "BICYCLE" => Ok(Mode::Bicycle),
        "CAR" => Ok(Mode::Car),
        _ => Err(err(format!("unsupported street mode '{mode}'"))),
    }
}

/// The transit modes of r5r's `mode_list$transit_mode` (`"BUS;RAIL"`, or
/// every mode and `TRANSIT` for `"TRANSIT"`); `TRANSIT` itself selects no
/// route.
fn transit_modes(modes: &str) -> Result<Vec<TransitMode>> {
    modes
        .split(';')
        .filter(|m| !m.is_empty() && *m != "TRANSIT")
        .map(|m| {
            TransitMode::from_name(m).ok_or_else(|| err(format!("unknown transit mode '{m}'")))
        })
        .collect()
}

/// Street-only travel-time matrix: the reported pairs as
/// `list(from, to, minutes)`, origin-major. With `output` (see [`Output`],
/// with `percentiles`), the files are written instead and `NULL` returned.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_street_ttm(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    mode: &str,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_trip_duration: i32,
    n_threads: i32,
    scenario: Robj,
    output: Robj,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let mut params = route_params(
        walk_speed,
        bike_speed,
        max_lts,
        max_trip_duration,
        n_threads,
    );
    params.scenario = scenario::scenario_of(&scenario)?;
    let mode = street_mode(mode)?;
    if let Some(out) = Output::of(&output)? {
        let percentiles: Vec<i32> = out
            .get("percentiles")?
            .as_integer_vector()
            .ok_or_else(|| err("output$percentiles must be an integer vector"))?;
        let w = out.writer(n_threads);
        done!(interruptible(&params.cancel, || {
            for r in w.origin_chunks(d.len()) {
                let m = street_travel_time_matrix(&n.net, &o[r.clone()], &d, mode, &params)?;
                w.write_street_matrix(r.start, &m, &percentiles)
                    .map_err(io_err)?;
            }
            Ok(())
        })?);
        return Ok(().into());
    }
    let m = done!(interruptible(&params.cancel, || {
        street_travel_time_matrix(&n.net, &o, &d, mode, &params)
    })?);
    let nd = d.len();
    let (mut from, mut to, mut tt) = (Vec::new(), Vec::new(), Vec::new());
    for (k, v) in m.as_slice().iter().enumerate() {
        if let Some(v) = v {
            from.push((k / nd) as i32 + 1);
            to.push((k % nd) as i32 + 1);
            tt.push(*v as i32);
        }
    }
    Ok(list!(from = from, to = to, time = tt).into())
}

#[allow(clippy::too_many_arguments)]
fn transit_params(
    modes: &str,
    access_mode: &str,
    egress_mode: &str,
    date: i32,
    departure_time: i32,
    time_window: i32,
    percentiles: &[i32],
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: i32,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: i32,
) -> Result<TransitParams> {
    let mut p = TransitParams::default();
    p.transit_modes = transit_modes(modes)?;
    p.access_mode = street_mode(access_mode)?;
    p.egress_mode = street_mode(egress_mode)?;
    p.date = date as u32;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.percentiles = percentiles.to_vec();
    p.max_walk_time = max_walk_time;
    p.max_bike_time = max_bike_time;
    p.max_car_time = max_car_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_lts = max_lts;
    p.max_rides = max_rides.max(0) as usize;
    p.draws_per_minute = draws_per_minute;
    p.seed = seed;
    p.n_threads = threads(n_threads);
    p.cancel = CancelToken::new();
    Ok(p)
}

/// Street routing parameters with a fresh cancellation token.
fn route_params(
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_trip_duration: i32,
    n_threads: i32,
) -> RouteParams {
    let mut p = RouteParams::default();
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_lts = max_lts;
    p.max_trip_duration_min = max_trip_duration;
    p.n_threads = threads(n_threads);
    p.cancel = CancelToken::new();
    p
}

fn seed_of(has_seed: bool, seed: i32) -> Option<i32> {
    has_seed.then_some(seed)
}

/// Public transport travel-time matrix: the reported pairs as
/// `list(from, to, p1, ..., pk)` (percentiles ascending, NA where r5r
/// gives NA), origin-major. With `output` (see [`Output`]), the files are
/// written instead and `NULL` returned.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_transit_ttm(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    transit_modes: &str,
    access_mode: &str,
    egress_mode: &str,
    bike_speed: f64,
    max_lts: i32,
    max_bike_time: i32,
    max_car_time: i32,
    date: i32,
    departure_time: i32,
    time_window: i32,
    percentiles: &[i32],
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
    scenario: Robj,
    output: Robj,
    fare_structure: Robj,
    max_fare: f64,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let t = transit(n)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let mut p = transit_params(
        transit_modes,
        access_mode,
        egress_mode,
        date,
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_bike_time,
        max_car_time,
        max_trip_duration,
        walk_speed,
        bike_speed,
        max_lts,
        max_rides,
        draws_per_minute,
        seed_of(has_seed, seed),
        n_threads,
    )?;
    p.fare_structure = fares::fare_args(&fare_structure)?;
    p.max_fare = max_fare;
    p.scenario = scenario::scenario_of(&scenario)?;
    if let Some(out) = Output::of(&output)? {
        let w = out.writer(n_threads);
        done!(interruptible(&p.cancel, || {
            for r in w.origin_chunks(d.len() * percentiles.len()) {
                let m =
                    transit_travel_time_matrix(&n.net, &t.layer, &t.data, &o[r.clone()], &d, &p)?;
                w.write_transit_matrix(r.start, &m).map_err(io_err)?;
            }
            Ok(())
        })?);
        return Ok(().into());
    }
    let m = done!(interruptible(&p.cancel, || {
        transit_travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)
    })?);
    let np = percentiles.len();
    let (mut from, mut to) = (Vec::new(), Vec::new());
    let mut cols: Vec<Vec<Option<i32>>> = vec![Vec::new(); np];
    for (i, row) in m.rows().enumerate() {
        for (j, cell) in row.chunks_exact(np).enumerate() {
            if cell[0].is_none() {
                continue;
            }
            from.push(i as i32 + 1);
            to.push(j as i32 + 1);
            for (c, v) in cols.iter_mut().zip(cell) {
                c.push(v.map(|v| v as i32));
            }
        }
    }
    let mut out: Vec<Robj> = vec![from.into(), to.into()];
    out.extend(cols.into_iter().map(Robj::from));
    Ok(List::from_values(out).into())
}

fn decay(name: &str, value: f64) -> Result<DecayFunction> {
    Ok(match name {
        "step" => DecayFunction::Step,
        "exponential" => DecayFunction::Exponential,
        "fixed_exponential" => DecayFunction::FixedExponential {
            decay_constant: value,
        },
        "linear" => DecayFunction::Linear {
            width_minutes: value as i32,
        },
        "logistic" => DecayFunction::Logistic {
            standard_deviation_minutes: value,
        },
        _ => return Err(err(format!("unknown decay function '{name}'"))),
    })
}

/// Accessibility values `[origin][opportunity][percentile][cutoff]`,
/// flattened. `mode` is the street mode (of the access to public
/// transport when `transit_modes`, r5r's `transit_mode`, is not empty);
/// `opportunities` is a list of double vectors, one per column. With
/// `output` (see [`Output`], with the `opportunities` names), the files are
/// written instead and `NULL` returned.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_accessibility(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    opportunities: List,
    mode: &str,
    transit_modes: &str,
    egress_mode: &str,
    decay_function: &str,
    decay_value: f64,
    cutoffs: &[i32],
    percentiles: &[i32],
    date: i32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
    scenario: Robj,
    output: Robj,
    fare_structure: Robj,
    max_fare: f64,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let out = Output::of(&output)?;
    let names: Vec<String> = match &out {
        Some(out) => out
            .get("opportunities")?
            .as_str_vector()
            .ok_or_else(|| err("output$opportunities must be a character vector"))?
            .into_iter()
            .map(str::to_owned)
            .collect(),
        None => Vec::new(),
    };
    let w = out.as_ref().map(|out| out.writer(n_threads));
    // every origin's values are few: route the origins in slices only to
    // bound them
    let per_origin = opportunities.len() * percentiles.len() * cutoffs.len();
    let chunks: Vec<std::ops::Range<usize>> = match &w {
        Some(w) => w.origin_chunks(per_origin).collect(),
        None => std::iter::once(0..o.len()).collect(),
    };
    let opp: Vec<Vec<f64>> = opportunities
        .values()
        .map(|v| {
            v.as_real_slice()
                .map(<[f64]>::to_vec)
                .ok_or_else(|| err("opportunities must be double vectors"))
        })
        .collect::<Result<_>>()?;
    let dest = Destinations::new(&d, &opp);
    let mut a = AccessibilityParams::default();
    a.cutoffs = cutoffs.to_vec();
    a.decay = decay(decay_function, decay_value)?;
    let acc = if !transit_modes.is_empty() {
        let t = transit(n)?;
        let mut p = transit_params(
            transit_modes,
            mode,
            egress_mode,
            date,
            departure_time,
            time_window,
            percentiles,
            max_walk_time,
            max_bike_time,
            max_car_time,
            max_trip_duration,
            walk_speed,
            bike_speed,
            max_lts,
            max_rides,
            draws_per_minute,
            seed_of(has_seed, seed),
            n_threads,
        )?;
        p.fare_structure = fares::fare_args(&fare_structure)?;
        p.max_fare = max_fare;
        p.scenario = scenario::scenario_of(&scenario)?;
        done!(interruptible(&p.cancel, || {
            let mut all = None;
            for r in &chunks {
                let acc =
                    transit_accessibility(&n.net, &t.layer, &t.data, &o[r.clone()], &dest, &p, &a)?;
                match &w {
                    Some(w) => w
                        .write_accessibility(r.start, &acc, &names)
                        .map_err(io_err)?,
                    None => all = Some(acc),
                }
            }
            Ok(all)
        })?)
    } else {
        let mut params = route_params(
            walk_speed,
            bike_speed,
            max_lts,
            max_trip_duration,
            n_threads,
        );
        params.scenario = scenario::scenario_of(&scenario)?;
        let mode = street_mode(mode)?;
        done!(interruptible(&params.cancel, || {
            let mut all = None;
            for r in &chunks {
                let acc = street_accessibility(
                    &n.net,
                    &o[r.clone()],
                    &dest,
                    mode,
                    &params,
                    percentiles,
                    &a,
                )?;
                match &w {
                    Some(w) => w
                        .write_accessibility(r.start, &acc, &names)
                        .map_err(io_err)?,
                    None => all = Some(acc),
                }
            }
            Ok(all)
        })?)
    };
    Ok(match acc {
        Some(acc) => acc.into_values().into(),
        None => ().into(),
    })
}

/// Expanded (or, with `arrival`, arrival) travel-time matrix as columns:
/// `from`, `to` (1-based), `departure` (seconds after midnight), `draw`,
/// the breakdown times (NA where r5r gives NA), `routes`, `n_rides` and
/// `total_time`. For an arrival search the parameters must already be the
/// departure side (date and time of `arrival - max_trip_duration`,
/// `time_window = max_trip_duration`), as r5r's R wrapper sets them. With
/// `output` (see [`Output`], with `breakdown`), the files are written
/// instead and `NULL` returned.
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_expanded(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    dlat: &[f64],
    dlon: &[f64],
    transit_modes: &str,
    access_mode: &str,
    egress_mode: &str,
    bike_speed: f64,
    max_lts: i32,
    max_bike_time: i32,
    max_car_time: i32,
    arrival: bool,
    date: i32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
    scenario: Robj,
    output: Robj,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let t = transit(n)?;
    let (o, d) = (points(olat, olon), points(dlat, dlon));
    let mut p = transit_params(
        transit_modes,
        access_mode,
        egress_mode,
        date,
        departure_time,
        time_window,
        &[50],
        max_walk_time,
        max_bike_time,
        max_car_time,
        max_trip_duration,
        walk_speed,
        bike_speed,
        max_lts,
        max_rides,
        draws_per_minute,
        seed_of(has_seed, seed),
        n_threads,
    )?;
    p.scenario = scenario::scenario_of(&scenario)?;
    let route = |o: &[(f64, f64)]| {
        if arrival {
            arrival_travel_time_matrix(&n.net, &t.layer, &t.data, o, &d, &p)
        } else {
            expanded_travel_time_matrix(&n.net, &t.layer, &t.data, o, &d, &p)
        }
    };
    if let Some(out) = Output::of(&output)? {
        let breakdown = out
            .get("breakdown")?
            .as_bool()
            .ok_or_else(|| err("output$breakdown must be TRUE or FALSE"))?;
        let w = out.writer(n_threads);
        // at most one row per destination, departure minute and draw
        let draws = if t.layer.has_frequencies() {
            draws_per_minute.max(1) as usize
        } else {
            1
        };
        let per_origin = d.len() * time_window.max(1) as usize * draws;
        done!(interruptible(&p.cancel, || {
            for r in w.origin_chunks(per_origin) {
                let m = route(&o[r.clone()])?;
                w.write_expanded(r.start, &m, breakdown).map_err(io_err)?;
            }
            Ok(())
        })?);
        return Ok(().into());
    }
    let rows = done!(interruptible(&p.cancel, || route(&o))?);
    let len = rows.n_rows();
    let mut from = Vec::with_capacity(len);
    let mut to = Vec::with_capacity(len);
    let mut dep = Vec::with_capacity(len);
    let mut draw = Vec::with_capacity(len);
    let mut times: [Vec<Option<f64>>; 6] = Default::default();
    let mut routes: Vec<Option<String>> = Vec::with_capacity(len);
    let mut n_rides: Vec<Option<i32>> = Vec::with_capacity(len);
    for (i, origin_rows) in rows.into_vec().into_iter().enumerate() {
        for r in origin_rows {
            from.push(i as i32 + 1);
            to.push(r.to as i32 + 1);
            dep.push(r.departure);
            draw.push(r.draw as i32);
            let b = r.trip.as_ref();
            let vals = b.map(|b| {
                [
                    b.access_time,
                    b.wait_time,
                    b.ride_time,
                    b.transfer_time,
                    b.egress_time,
                    b.total_time,
                ]
            });
            for (k, col) in times.iter_mut().enumerate() {
                col.push(vals.map(|v| v[k]));
            }
            n_rides.push(b.map(|b| b.n_rides as i32));
            routes.push(r.trip.map(|b| b.routes));
        }
    }
    let [access, wait, ride, transfer, egress, total] = times;
    Ok(list!(
        from = from,
        to = to,
        departure = dep,
        draw = draw,
        access_time = access,
        wait_time = wait,
        ride_time = ride,
        transfer_time = transfer,
        egress_time = egress,
        routes = routes,
        n_rides = n_rides,
        total_time = total
    )
    .into())
}

/// Polygon isochrones, as flat columns the R code assembles into `sf`:
/// per row (origin, percentile, cutoff; r5r's order) `origin` (1-based),
/// `isochrone`, `percentile` and `n_polygons`; per polygon `n_rings`; per
/// ring `n_points`; per point `x` (longitude) and `y` (latitude). `mode` is
/// the street mode (of the access to public transport when
/// `transit_modes`, r5r's `transit_mode`, is not empty); `max_trip_duration`
/// is the search limit (r5r: the largest cutoff plus 10).
/// @noRd
#[extendr]
#[allow(clippy::too_many_arguments)]
fn rs_isochrone_polygons(
    ptr: Robj,
    olat: &[f64],
    olon: &[f64],
    mode: &str,
    transit_modes: &str,
    egress_mode: &str,
    cutoffs: &[f64],
    zoom: i32,
    percentiles: &[i32],
    date: i32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_bike_time: i32,
    max_car_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: i32,
    draws_per_minute: i32,
    has_seed: bool,
    seed: i32,
    n_threads: i32,
) -> Result<Robj> {
    let n = network(&ptr)?;
    let o = points(olat, olon);
    let rows = if !transit_modes.is_empty() {
        let t = transit(n)?;
        let p = transit_params(
            transit_modes,
            mode,
            egress_mode,
            date,
            departure_time,
            time_window,
            percentiles,
            max_walk_time,
            max_bike_time,
            max_car_time,
            max_trip_duration,
            walk_speed,
            bike_speed,
            max_lts,
            max_rides,
            draws_per_minute,
            seed_of(has_seed, seed),
            n_threads,
        )?;
        done!(interruptible(&p.cancel, || {
            let s = transit_travel_time_surface(&n.net, &t.layer, &t.data, &o, &p, zoom)?;
            isochrone_polygons(&s, cutoffs)
        })?)
    } else {
        let params = route_params(
            walk_speed,
            bike_speed,
            max_lts,
            max_trip_duration,
            n_threads,
        );
        let mode = street_mode(mode)?;
        done!(interruptible(&params.cancel, || {
            let s = street_travel_time_surface(&n.net, &o, mode, &params, percentiles, zoom)?;
            isochrone_polygons(&s, cutoffs)
        })?)
    };
    let (mut origin, mut iso, mut pct, mut n_polygons) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut n_rings, mut n_points, mut x, mut y) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in &rows {
        origin.push(r.origin as i32 + 1);
        iso.push(r.isochrone);
        pct.push(r.percentile);
        n_polygons.push(r.polygons.len() as i32);
        for poly in &r.polygons {
            n_rings.push(poly.len() as i32);
            for ring in poly {
                n_points.push(ring.len() as i32);
                for &[px, py] in ring {
                    x.push(px);
                    y.push(py);
                }
            }
        }
    }
    Ok(list!(
        origin = origin,
        isochrone = iso,
        percentile = pct,
        n_polygons = n_polygons,
        n_rings = n_rings,
        n_points = n_points,
        x = x,
        y = y
    )
    .into())
}

/// The street edges of r5r's `street_network_to_sf()$edges`, as columns,
/// with the centroid r5r routes to (`lat`, `lon`; NaN without one) and the
/// line of each edge as `n_points` and flat `x`, `y`.
/// @noRd
#[extendr]
fn rs_street_edges(ptr: Robj) -> Result<Robj> {
    Ok(edges_list(&network(&ptr)?.net))
}

/// The street edges of the network a scenario makes (as `rs_street_edges`),
/// for comparing it with r5r's; `NULL` when the scenario changes nothing.
/// @noRd
#[extendr]
fn rs_scenario_street_edges(ptr: Robj, scenario: Robj) -> Result<Robj> {
    let n = network(&ptr)?;
    let s = scenario::scenario_of(&scenario)?;
    let net = peabiru_core::unstable::scenario_network(&n.net, &s).map_err(err)?;
    Ok(net.map_or_else(|| ().into(), |net| edges_list(&net)))
}

/// The columns of r5r's `street_network_to_sf()` edges.
fn edges_list(net: &StreetNetwork) -> Robj {
    let e = street_edges(net);
    let rows = 0..e.len();
    let cen = e.centroid_points();
    let (mut n_points, mut x, mut y) = (Vec::with_capacity(e.len()), Vec::new(), Vec::new());
    for i in rows.clone() {
        let g = e.geometry(i);
        n_points.push(g.len() as i32);
        for &[px, py] in g {
            x.push(px);
            y.push(py);
        }
    }
    list!(
        edge_index = rows
            .clone()
            .map(|i| e.edge_index(i) as i32)
            .collect::<Vec<_>>(),
        osm_id = rows.clone().map(|i| e.osm_id(i) as f64).collect::<Vec<_>>(),
        from_vertex = rows
            .clone()
            .map(|i| e.from_vertex(i) as i32)
            .collect::<Vec<_>>(),
        to_vertex = rows
            .clone()
            .map(|i| e.to_vertex(i) as i32)
            .collect::<Vec<_>>(),
        street_class = rows.clone().map(|i| e.street_class(i)).collect::<Vec<_>>(),
        length = rows.clone().map(|i| e.length_m(i)).collect::<Vec<_>>(),
        walk = rows.clone().map(|i| e.walk(i)).collect::<Vec<_>>(),
        car = rows.clone().map(|i| e.car(i)).collect::<Vec<_>>(),
        car_speed = rows.clone().map(|i| e.car_speed_kph(i)).collect::<Vec<_>>(),
        bicycle = rows.clone().map(|i| e.bicycle(i)).collect::<Vec<_>>(),
        bicycle_lts = rows.clone().map(|i| e.bicycle_lts(i)).collect::<Vec<_>>(),
        lat = cen.iter().map(|c| c.0).collect::<Vec<_>>(),
        lon = cen.iter().map(|c| c.1).collect::<Vec<_>>(),
        n_points = n_points,
        x = x,
        y = y
    )
    .into()
}

/// Line isochrones from an integer matrix of median minutes (`NA` where not
/// reported), `n_origins` rows of `n_edges` values, origin-major: the rows
/// in r5r's order as `origin`, `edge` (1-based), `travel_time` and
/// `isochrone`.
/// @noRd
#[extendr]
fn rs_isochrone_lines(
    n_origins: i32,
    n_edges: i32,
    minutes: &[i32],
    cutoffs: &[f64],
) -> Result<Robj> {
    // NA_integer_ is i32::MIN
    let m: Vec<Option<u32>> = minutes.iter().map(|&v| u32::try_from(v).ok()).collect();
    let lines = isochrone_lines(
        n_origins.max(0) as usize,
        n_edges.max(0) as usize,
        &m,
        cutoffs,
    )
    .map_err(err)?;
    Ok(list!(
        origin = lines
            .iter()
            .map(|l| l.origin as i32 + 1)
            .collect::<Vec<_>>(),
        edge = lines.iter().map(|l| l.edge as i32 + 1).collect::<Vec<_>>(),
        travel_time = lines
            .iter()
            .map(|l| l.travel_time as i32)
            .collect::<Vec<_>>(),
        isochrone = lines.iter().map(|l| l.isochrone).collect::<Vec<_>>()
    )
    .into())
}

// Macro to generate exports.
// This ensures exported functions are registered with R.
// See corresponding C code in `entrypoint.c`.
extendr_module! {
    mod peabiru;
    fn rs_core_version;
    fn rs_build_network;
    fn rs_network_valid;
    fn rs_network_r5r_warnings;
    fn rs_network_info;
    fn rs_transit_availability;
    fn rs_street_ttm;
    fn rs_transit_ttm;
    fn rs_accessibility;
    fn rs_expanded;
    fn rs_isochrone_polygons;
    fn rs_street_edges;
    fn rs_scenario_street_edges;
    fn rs_isochrone_lines;
    use inspect;
    use fares;
    use itinerary;
    use options;
}
