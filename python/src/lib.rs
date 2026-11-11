//! Python bindings of the peabiru routing core.
//!
//! Everything that mirrors r5r's R wrapper (argument checks, defaults,
//! datetime handling, trip limits, column assembly) lives in the Python
//! code of the package (`peabiru/_routing.py`); these functions only take
//! already validated values, run the core with the GIL released and hand
//! back plain columns.
//!
//! Columns cross the boundary as `bytes` holding native-endian `f64`/`i32`
//! arrays, which the Python side views with `numpy.frombuffer`: one buffer
//! per column instead of one Python object per cell (a matrix can have
//! millions of rows), without a compile-time dependency on numpy. Row
//! indices are 0-based.
//!
//! A routing call runs the core on a thread of its own while the calling
//! thread, without the GIL, wakes up every [`POLL`] to run Python's signal
//! handlers: Ctrl-C cancels the computation, and once its workers have
//! stopped the call raises `KeyboardInterrupt`.

use peabiru_core::{
    arrival_travel_time_matrix, build_from_dir, expanded_travel_time_matrix,
    isochrone_lines as isochrone_lines_core, isochrone_polygons as isochrone_polygons_core,
    street_accessibility, street_edges as street_edges_core, street_travel_time_matrix,
    street_travel_time_surface, transit_accessibility, transit_travel_time_matrix,
    transit_travel_time_surface, validate_date, AccessibilityParams, BuildOptions, CancelToken,
    CsvOutput, DecayFunction, Destinations, ExpandedRow, Mode, RouteError, RouteParams,
    StreetNetwork, TransitData, TransitLayer, TransitParams,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Missing value in the `i32` columns handed to Python.
const NA_I32: i32 = i32::MIN;

/// A column handed to Python: native-endian values in a `bytes` object.
type Col<'py> = Bound<'py, PyBytes>;

/// A network built once and shared by every routing call.
#[pyclass(frozen, module = "peabiru._peabiru", name = "Network")]
struct Network {
    net: StreetNetwork,
    /// Present when the data directory had at least one GTFS feed.
    transit: Option<Transit>,
}

struct Transit {
    layer: TransitLayer,
    data: TransitData,
}

fn runtime_err(msg: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(msg.to_string())
}

fn value_err(msg: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(msg.to_string())
}

/// How often a routing call checks for signals while the core runs.
const POLL: Duration = Duration::from_millis(50);

/// Tells the calling thread that the work has ended, also when it ended in
/// a panic: dropped by the worker, it sets the flag and wakes the caller.
/// The caller waits on the flag and not on the thread having exited, which
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

/// Run `work` on a thread of its own and wait for it with the GIL
/// released, running Python's signal handlers every [`POLL`]. When they
/// raise (`KeyboardInterrupt` on Ctrl-C), `cancel` is cancelled, the
/// workers are waited for, and that exception is raised. `work` must hand
/// `cancel` to the core.
fn interruptible<T: Send>(
    py: Python<'_>,
    cancel: &CancelToken,
    work: impl FnOnce() -> Result<T, RouteError> + Send,
) -> PyResult<T> {
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
        let mut raised = None;
        while !done.load(Ordering::Acquire) {
            py.detach(|| std::thread::park_timeout(POLL));
            if raised.is_none() && !done.load(Ordering::Acquire) {
                if let Err(e) = py.check_signals() {
                    cancel.cancel();
                    raised = Some(e);
                }
            }
        }
        let r = worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
        match (raised, r) {
            (Some(e), _) => Err(e),
            (None, Ok(v)) => Ok(v),
            (None, Err(RouteError::Invalid(msg))) => Err(value_err(msg)),
            (None, Err(e)) => Err(runtime_err(e)),
        }
    })
}

/// Where and for which ids to write CSV files instead of returning the
/// result (r5r's `output_dir`): the `output` argument of a routing
/// function, `None` or `(dir, origin ids, destination ids)`.
type Output = Option<(String, Vec<String>, Vec<String>)>;

/// The writer of an [`Output`].
fn csv_writer(out: &(String, Vec<String>, Vec<String>), n_threads: usize) -> CsvOutput<'_> {
    CsvOutput::new(&out.0, &out.1, &out.2, n_threads)
}

/// The outcome of writing the files: a file that could not be written is an
/// `OSError` (of the subclass its error kind maps to).
fn written(r: std::io::Result<()>) -> PyResult<()> {
    r.map_err(PyErr::from)
}

fn transit(n: &Network) -> PyResult<&Transit> {
    n.transit
        .as_ref()
        .ok_or_else(|| value_err("the network has no public transport (no GTFS feed)"))
}

/// Decode a buffer of native-endian `f64`.
fn f64s(b: &[u8]) -> PyResult<Vec<f64>> {
    if b.len() % 8 != 0 {
        return Err(value_err("a coordinate buffer must hold whole f64 values"));
    }
    Ok(b.chunks_exact(8)
        .map(|c| f64::from_ne_bytes(c.try_into().expect("8 bytes")))
        .collect())
}

/// Points as `(lat, lon)` from two `f64` buffers.
fn points(lat: &[u8], lon: &[u8]) -> PyResult<Vec<(f64, f64)>> {
    let (lat, lon) = (f64s(lat)?, f64s(lon)?);
    if lat.len() != lon.len() {
        return Err(value_err("lat and lon must have the same length"));
    }
    Ok(lat.into_iter().zip(lon).collect())
}

fn i32_bytes<'py>(py: Python<'py>, v: &[i32]) -> PyResult<Bound<'py, PyBytes>> {
    PyBytes::new_with(py, v.len() * 4, |buf| {
        for (c, x) in buf.chunks_exact_mut(4).zip(v) {
            c.copy_from_slice(&x.to_ne_bytes());
        }
        Ok(())
    })
}

fn f64_bytes<'py>(py: Python<'py>, v: &[f64]) -> PyResult<Bound<'py, PyBytes>> {
    PyBytes::new_with(py, v.len() * 8, |buf| {
        for (c, x) in buf.chunks_exact_mut(8).zip(v) {
            c.copy_from_slice(&x.to_ne_bytes());
        }
        Ok(())
    })
}

#[pymethods]
impl Network {
    /// Sizes of the network: vertices, edges, feeds, stops, patterns,
    /// services and whether any trip is frequency-based.
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        d.set_item("vertices", self.net.n_vertices())?;
        d.set_item("edges", self.net.n_edges())?;
        match &self.transit {
            Some(t) => {
                d.set_item("feeds", t.layer.feed_ids().to_vec())?;
                d.set_item("stops", t.layer.n_stops())?;
                d.set_item("patterns", t.layer.n_patterns())?;
                d.set_item("services", t.layer.n_services())?;
                d.set_item("frequencies", t.layer.has_frequencies())?;
            }
            None => {
                d.set_item("feeds", Vec::<String>::new())?;
                d.set_item("stops", 0)?;
                d.set_item("patterns", 0)?;
                d.set_item("services", 0)?;
                d.set_item("frequencies", false)?;
            }
        }
        Ok(d)
    }

    /// Warnings for the GTFS feeds that r5r refuses to build a network
    /// from, one per feed.
    fn r5r_warnings(&self) -> Vec<String> {
        self.transit
            .as_ref()
            .map_or_else(Vec::new, |t| t.layer.r5r_warnings())
    }

    /// Number of services in the network and of those active on `date`
    /// (`yyyymmdd`), as r5r's `getTransitServiceAvailability`.
    fn transit_availability(&self, date: u32) -> PyResult<(usize, usize)> {
        validate_date(date).map_err(value_err)?;
        let Some(t) = &self.transit else {
            return Ok((0, 0));
        };
        let active = t.layer.n_services_active_on(date).map_err(value_err)?;
        Ok((t.layer.n_services(), active))
    }
}

/// Version of the Rust core this extension was built against.
#[pyfunction]
fn core_version() -> &'static str {
    peabiru_core::VERSION
}

/// Build a network from a directory with one `.pbf` and any GTFS zips.
#[pyfunction]
fn build_network(py: Python<'_>, data_path: String, n_threads: usize) -> PyResult<Network> {
    py.detach(move || {
        let mut opts = BuildOptions::default();
        opts.n_threads = n_threads;
        let (net, layer) = build_from_dir(&data_path, &opts).map_err(runtime_err)?;
        let transit = if layer.feed_ids().is_empty() {
            None
        } else {
            let data = TransitData::new(&net, &layer);
            Some(Transit { layer, data })
        };
        Ok(Network { net, transit })
    })
}

fn street_mode(mode: &str) -> PyResult<Mode> {
    match mode {
        "WALK" => Ok(Mode::Walk),
        "BICYCLE" => Ok(Mode::Bicycle),
        "CAR" => Ok(Mode::Car),
        _ => Err(value_err(format!("unsupported street mode '{mode}'"))),
    }
}

/// Street-only travel-time matrix: the reported pairs as
/// `(from, to, minutes)` int32 buffers, origin-major. With `output` (see
/// [`Output`]) the files are written instead, with one `travel_time_pXX`
/// column per value of `percentiles`, and the buffers are empty.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn street_ttm<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    dlat: &[u8],
    dlon: &[u8],
    mode: &str,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_trip_duration: i32,
    n_threads: usize,
    percentiles: Vec<i32>,
    output: Output,
) -> PyResult<(Col<'py>, Col<'py>, Col<'py>)> {
    let n = network.get();
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let mode = street_mode(mode)?;
    let params = route_params(
        walk_speed,
        bike_speed,
        max_lts,
        max_trip_duration,
        n_threads,
    );
    if let Some(out) = &output {
        let w = csv_writer(out, n_threads);
        written(interruptible(py, &params.cancel, || {
            for r in w.origin_chunks(d.len()) {
                let m = street_travel_time_matrix(&n.net, &o[r.clone()], &d, mode, &params)?;
                if let Err(e) = w.write_street_matrix(r.start, &m, &percentiles) {
                    return Ok(Err(e));
                }
            }
            Ok(Ok(()))
        })?)?;
        let empty = i32_bytes(py, &[])?;
        return Ok((empty.clone(), empty.clone(), empty));
    }
    let (from, to, tt) = interruptible(py, &params.cancel, || {
        let m = street_travel_time_matrix(&n.net, &o, &d, mode, &params)?;
        let nd = d.len();
        let (mut from, mut to, mut tt) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in m.as_slice().iter().enumerate() {
            if let Some(v) = v {
                from.push((k / nd) as i32);
                to.push((k % nd) as i32);
                tt.push(*v as i32);
            }
        }
        Ok((from, to, tt))
    })?;
    Ok((
        i32_bytes(py, &from)?,
        i32_bytes(py, &to)?,
        i32_bytes(py, &tt)?,
    ))
}

#[allow(clippy::too_many_arguments)]
fn transit_params(
    date: u32,
    departure_time: i32,
    time_window: i32,
    percentiles: Vec<i32>,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: usize,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: usize,
) -> TransitParams {
    let mut p = TransitParams::default();
    p.date = date;
    p.departure_time = departure_time;
    p.time_window = time_window;
    p.percentiles = percentiles;
    p.max_walk_time = max_walk_time;
    p.max_trip_duration = max_trip_duration;
    p.walk_speed_kmh = walk_speed;
    p.max_rides = max_rides;
    p.draws_per_minute = draws_per_minute;
    p.seed = seed;
    p.n_threads = n_threads;
    p.cancel = CancelToken::new();
    p
}

/// Street routing parameters with a fresh cancellation token.
fn route_params(
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_trip_duration: i32,
    n_threads: usize,
) -> RouteParams {
    let mut p = RouteParams::default();
    p.walk_speed_kmh = walk_speed;
    p.bike_speed_kmh = bike_speed;
    p.max_lts = max_lts;
    p.max_trip_duration_min = max_trip_duration;
    p.n_threads = n_threads;
    p.cancel = CancelToken::new();
    p
}

/// Walk + transit travel-time matrix: the reported pairs as `from`, `to`
/// and one int32 buffer per percentile (ascending), with `i32::MIN` where
/// r5r gives NA; origin-major. With `output` (see [`Output`]) the files
/// are written instead and the buffers are empty.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn transit_ttm<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    dlat: &[u8],
    dlon: &[u8],
    date: u32,
    departure_time: i32,
    time_window: i32,
    percentiles: Vec<i32>,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: usize,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: usize,
    output: Output,
) -> PyResult<(Col<'py>, Col<'py>, Vec<Col<'py>>)> {
    let n = network.get();
    let t = transit(n)?;
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let np = percentiles.len();
    let p = transit_params(
        date,
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed,
        n_threads,
    );
    if let Some(out) = &output {
        let w = csv_writer(out, n_threads);
        written(interruptible(py, &p.cancel, || {
            for r in w.origin_chunks(d.len() * np) {
                let m =
                    transit_travel_time_matrix(&n.net, &t.layer, &t.data, &o[r.clone()], &d, &p)?;
                if let Err(e) = w.write_transit_matrix(r.start, &m) {
                    return Ok(Err(e));
                }
            }
            Ok(Ok(()))
        })?)?;
        let empty = i32_bytes(py, &[])?;
        return Ok((empty.clone(), empty, Vec::new()));
    }
    let (from, to, cols) = interruptible(py, &p.cancel, || {
        let m = transit_travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)?;
        let (mut from, mut to) = (Vec::new(), Vec::new());
        let mut cols: Vec<Vec<i32>> = vec![Vec::new(); np];
        for (i, row) in m.rows().enumerate() {
            for (j, cell) in row.chunks_exact(np).enumerate() {
                if cell[0].is_none() {
                    continue;
                }
                from.push(i as i32);
                to.push(j as i32);
                for (c, v) in cols.iter_mut().zip(cell) {
                    c.push(v.map_or(NA_I32, |v| v as i32));
                }
            }
        }
        Ok((from, to, cols))
    })?;
    let cols = cols
        .iter()
        .map(|c| i32_bytes(py, c))
        .collect::<PyResult<Vec<_>>>()?;
    Ok((i32_bytes(py, &from)?, i32_bytes(py, &to)?, cols))
}

fn decay(name: &str, value: f64) -> PyResult<DecayFunction> {
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
        _ => return Err(value_err(format!("unknown decay function '{name}'"))),
    })
}

/// Accessibility values `[origin][opportunity][percentile][cutoff]`,
/// flattened into one f64 buffer. `mode` is `TRANSIT` (walk access and
/// egress) or a street mode; `opportunities` holds one f64 buffer per
/// opportunity column. With `output` (see [`Output`]; the third element
/// holds the opportunity column names instead of destination ids) the files
/// are written instead and the buffer is empty.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn accessibility<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    dlat: &[u8],
    dlon: &[u8],
    opportunities: Vec<Vec<u8>>,
    mode: &str,
    decay_function: &str,
    decay_value: f64,
    cutoffs: Vec<i32>,
    percentiles: Vec<i32>,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: usize,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: usize,
    output: Output,
) -> PyResult<Bound<'py, PyBytes>> {
    let n = network.get();
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let opp: Vec<Vec<f64>> = opportunities
        .iter()
        .map(|b| f64s(b))
        .collect::<PyResult<_>>()?;
    if opp.iter().any(|c| c.len() != d.len()) {
        return Err(value_err(
            "every opportunity column must have one value per destination",
        ));
    }
    let per_origin = opp.len() * percentiles.len() * cutoffs.len();
    let mut a = AccessibilityParams::default();
    a.cutoffs = cutoffs;
    a.decay = decay(decay_function, decay_value)?;
    let street = if mode == "TRANSIT" {
        None
    } else {
        Some(street_mode(mode)?)
    };
    let t = if street.is_none() {
        Some(transit(n)?)
    } else {
        None
    };
    let cancel = CancelToken::new();
    let w = output
        .as_ref()
        .map(|(dir, ids, _)| CsvOutput::new(dir, ids, &[], n_threads));
    let names: &[String] = output.as_ref().map_or(&[], |out| &out.2);
    // every origin's values are few: the origins are routed in slices only
    // to bound them
    let chunks: Vec<std::ops::Range<usize>> = match &w {
        Some(w) => w.origin_chunks(per_origin).collect(),
        None => std::iter::once(0..o.len()).collect(),
    };
    let mut p = transit_params(
        date,
        departure_time,
        time_window,
        percentiles.clone(),
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed,
        n_threads,
    );
    p.cancel = cancel.clone();
    let mut params = route_params(
        walk_speed,
        bike_speed,
        max_lts,
        max_trip_duration,
        n_threads,
    );
    params.cancel = cancel.clone();
    let values = interruptible(py, &cancel, || {
        let dest = Destinations::new(&d, &opp);
        let mut all = None;
        for r in &chunks {
            let o = &o[r.clone()];
            let acc = match (street, t) {
                (None, Some(t)) => {
                    transit_accessibility(&n.net, &t.layer, &t.data, o, &dest, &p, &a)?
                }
                (Some(m), _) => {
                    street_accessibility(&n.net, o, &dest, m, &params, &percentiles, &a)?
                }
                (None, None) => unreachable!("transit checked above"),
            };
            match &w {
                Some(w) => {
                    if let Err(e) = w.write_accessibility(r.start, &acc, names) {
                        return Ok(Err(e));
                    }
                }
                None => all = Some(acc),
            }
        }
        Ok(Ok(all))
    })?;
    match values {
        Ok(Some(acc)) => f64_bytes(py, acc.values()),
        Ok(None) => f64_bytes(py, &[]),
        Err(e) => Err(PyErr::from(e)),
    }
}

/// Expanded (or, with `arrival`, arrival) travel-time matrix as a dict of
/// columns: `from`, `to` (0-based int32), `departure` (int32 seconds after
/// midnight), `draw` (int32), the breakdown times (f64, NaN where r5r
/// gives NA), `n_rides` (int32, `i32::MIN` for NA), `total_time` (f64),
/// and `routes` dictionary-encoded as `route_codes` (int32, -1 for NA) into
/// the list of strings `route_levels`. For an arrival search the
/// parameters must already be the departure side (date and time of
/// `arrival - max_trip_duration`, `time_window = max_trip_duration`), as
/// r5r's R wrapper sets them. With `output` (see [`Output`]) the files are
/// written instead (with the breakdown columns when `breakdown`) and the
/// dict is empty.
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn expanded<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    dlat: &[u8],
    dlon: &[u8],
    arrival: bool,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    max_rides: usize,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: usize,
    breakdown: bool,
    output: Output,
) -> PyResult<Bound<'py, PyDict>> {
    let n = network.get();
    let t = transit(n)?;
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let p = transit_params(
        date,
        departure_time,
        time_window,
        vec![50],
        max_walk_time,
        max_trip_duration,
        walk_speed,
        max_rides,
        draws_per_minute,
        seed,
        n_threads,
    );
    let route = |o: &[(f64, f64)]| {
        if arrival {
            arrival_travel_time_matrix(&n.net, &t.layer, &t.data, o, &d, &p)
        } else {
            expanded_travel_time_matrix(&n.net, &t.layer, &t.data, o, &d, &p)
        }
    };
    if let Some(out) = &output {
        let w = csv_writer(out, n_threads);
        // at most one row per destination, departure minute and draw
        let draws = if t.layer.has_frequencies() {
            draws_per_minute.max(1) as usize
        } else {
            1
        };
        let per_origin = d.len() * time_window.max(1) as usize * draws;
        written(interruptible(py, &p.cancel, || {
            for r in w.origin_chunks(per_origin) {
                let m = route(&o[r.clone()])?;
                if let Err(e) = w.write_expanded(r.start, &m, breakdown) {
                    return Ok(Err(e));
                }
            }
            Ok(Ok(()))
        })?)?;
        return Ok(PyDict::new(py));
    }
    let cols = interruptible(py, &p.cancel, || {
        Ok(ExpandedColumns::new(route(&o)?.into_vec()))
    })?;
    let out = PyDict::new(py);
    out.set_item("from", i32_bytes(py, &cols.from)?)?;
    out.set_item("to", i32_bytes(py, &cols.to)?)?;
    out.set_item("departure", i32_bytes(py, &cols.departure)?)?;
    out.set_item("draw", i32_bytes(py, &cols.draw)?)?;
    let names = [
        "access_time",
        "wait_time",
        "ride_time",
        "transfer_time",
        "egress_time",
        "total_time",
    ];
    for (name, col) in names.iter().zip(&cols.times) {
        out.set_item(*name, f64_bytes(py, col)?)?;
    }
    out.set_item("n_rides", i32_bytes(py, &cols.n_rides)?)?;
    out.set_item("route_codes", i32_bytes(py, &cols.route_codes)?)?;
    out.set_item("route_levels", cols.route_levels)?;
    Ok(out)
}

/// The rows of an expanded matrix as columns.
struct ExpandedColumns {
    from: Vec<i32>,
    to: Vec<i32>,
    departure: Vec<i32>,
    draw: Vec<i32>,
    times: [Vec<f64>; 6],
    n_rides: Vec<i32>,
    route_codes: Vec<i32>,
    route_levels: Vec<String>,
}

impl ExpandedColumns {
    fn new(rows: Vec<Vec<ExpandedRow>>) -> Self {
        let len: usize = rows.iter().map(Vec::len).sum();
        let mut c = ExpandedColumns {
            from: Vec::with_capacity(len),
            to: Vec::with_capacity(len),
            departure: Vec::with_capacity(len),
            draw: Vec::with_capacity(len),
            times: Default::default(),
            n_rides: Vec::with_capacity(len),
            route_codes: Vec::with_capacity(len),
            route_levels: Vec::new(),
        };
        let mut levels: HashMap<String, i32> = HashMap::new();
        for (i, origin_rows) in rows.into_iter().enumerate() {
            for r in origin_rows {
                c.from.push(i as i32);
                c.to.push(r.to as i32);
                c.departure.push(r.departure);
                c.draw.push(r.draw as i32);
                let vals = r.trip.as_ref().map(|b| {
                    [
                        b.access_time,
                        b.wait_time,
                        b.ride_time,
                        b.transfer_time,
                        b.egress_time,
                        b.total_time,
                    ]
                });
                for (k, col) in c.times.iter_mut().enumerate() {
                    col.push(vals.map_or(f64::NAN, |v| v[k]));
                }
                match r.trip {
                    Some(b) => {
                        c.n_rides.push(b.n_rides as i32);
                        let next = c.route_levels.len() as i32;
                        let code = *levels.entry(b.routes).or_insert_with_key(|k| {
                            c.route_levels.push(k.clone());
                            next
                        });
                        c.route_codes.push(code);
                    }
                    None => {
                        c.n_rides.push(NA_I32);
                        c.route_codes.push(-1);
                    }
                }
            }
        }
        c
    }
}

/// Polygon isochrones as a dict of buffers the Python side assembles into
/// geometries: per row (origin, percentile, cutoff; r5r's order) `origin`
/// (0-based int32), `isochrone` (f64), `percentile` and `n_polygons`
/// (int32); per polygon `n_rings`; per ring `n_points` (int32); per point
/// `x` (longitude) and `y` (latitude), f64. `mode` is `TRANSIT` (walk
/// access and egress) or a street mode; `max_trip_duration` is the search
/// limit (r5r: the largest cutoff plus 10).
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn isochrone_polygons<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
    olat: &[u8],
    olon: &[u8],
    mode: &str,
    cutoffs: Vec<f64>,
    zoom: i32,
    percentiles: Vec<i32>,
    date: u32,
    departure_time: i32,
    time_window: i32,
    max_walk_time: i32,
    max_trip_duration: i32,
    walk_speed: f64,
    bike_speed: f64,
    max_lts: i32,
    max_rides: usize,
    draws_per_minute: i32,
    seed: Option<i32>,
    n_threads: usize,
) -> PyResult<Bound<'py, PyDict>> {
    let n = network.get();
    let o = points(olat, olon)?;
    let street = if mode == "TRANSIT" {
        None
    } else {
        Some(street_mode(mode)?)
    };
    let t = if street.is_none() {
        Some(transit(n)?)
    } else {
        None
    };
    let cancel = CancelToken::new();
    let rows = interruptible(py, &cancel, || {
        let surface = match (street, t) {
            (None, Some(t)) => {
                let mut p = transit_params(
                    date,
                    departure_time,
                    time_window,
                    percentiles,
                    max_walk_time,
                    max_trip_duration,
                    walk_speed,
                    max_rides,
                    draws_per_minute,
                    seed,
                    n_threads,
                );
                p.cancel = cancel.clone();
                transit_travel_time_surface(&n.net, &t.layer, &t.data, &o, &p, zoom)?
            }
            (Some(m), _) => {
                let mut params = route_params(
                    walk_speed,
                    bike_speed,
                    max_lts,
                    max_trip_duration,
                    n_threads,
                );
                params.cancel = cancel.clone();
                street_travel_time_surface(&n.net, &o, m, &params, &percentiles, zoom)?
            }
            (None, None) => unreachable!("transit checked above"),
        };
        isochrone_polygons_core(&surface, &cutoffs)
    })?;
    let (mut origin, mut iso, mut pct, mut n_polygons) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut n_rings, mut n_points, mut x, mut y) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for r in &rows {
        origin.push(r.origin as i32);
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
    let out = PyDict::new(py);
    out.set_item("origin", i32_bytes(py, &origin)?)?;
    out.set_item("isochrone", f64_bytes(py, &iso)?)?;
    out.set_item("percentile", i32_bytes(py, &pct)?)?;
    out.set_item("n_polygons", i32_bytes(py, &n_polygons)?)?;
    out.set_item("n_rings", i32_bytes(py, &n_rings)?)?;
    out.set_item("n_points", i32_bytes(py, &n_points)?)?;
    out.set_item("x", f64_bytes(py, &x)?)?;
    out.set_item("y", f64_bytes(py, &y)?)?;
    Ok(out)
}

/// The street edges of r5r's `street_network_to_sf()$edges` as a dict of
/// buffers: `edge_index`, `from_vertex`, `to_vertex`, `bicycle_lts`
/// (int32), `osm_id` (f64, as r5r reports it), `length`, `car_speed` (f64),
/// `walk`, `car`, `bicycle` (int32 0/1), `street_class` (a list of
/// strings), the centroid r5r routes to as `lat`, `lon` (f64, NaN without
/// one), and the line of each edge as `n_points` (int32) and flat `x`, `y`
/// (f64).
#[pyfunction]
fn street_edges<'py>(
    py: Python<'py>,
    network: &Bound<'py, Network>,
) -> PyResult<Bound<'py, PyDict>> {
    let n = network.get();
    let e = py.detach(|| street_edges_core(&n.net));
    let rows = 0..e.len();
    let ints = |f: &dyn Fn(usize) -> i32| -> Vec<i32> { rows.clone().map(f).collect() };
    let floats = |f: &dyn Fn(usize) -> f64| -> Vec<f64> { rows.clone().map(f).collect() };
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
    let out = PyDict::new(py);
    out.set_item(
        "edge_index",
        i32_bytes(py, &ints(&|i| e.edge_index(i) as i32))?,
    )?;
    out.set_item("osm_id", f64_bytes(py, &floats(&|i| e.osm_id(i) as f64))?)?;
    out.set_item(
        "from_vertex",
        i32_bytes(py, &ints(&|i| e.from_vertex(i) as i32))?,
    )?;
    out.set_item(
        "to_vertex",
        i32_bytes(py, &ints(&|i| e.to_vertex(i) as i32))?,
    )?;
    let classes: Vec<&str> = rows.clone().map(|i| e.street_class(i)).collect();
    out.set_item("street_class", classes)?;
    out.set_item("length", f64_bytes(py, &floats(&|i| e.length_m(i)))?)?;
    out.set_item("walk", i32_bytes(py, &ints(&|i| i32::from(e.walk(i))))?)?;
    out.set_item("car", i32_bytes(py, &ints(&|i| i32::from(e.car(i))))?)?;
    out.set_item(
        "car_speed",
        f64_bytes(py, &floats(&|i| e.car_speed_kph(i)))?,
    )?;
    out.set_item(
        "bicycle",
        i32_bytes(py, &ints(&|i| i32::from(e.bicycle(i))))?,
    )?;
    out.set_item("bicycle_lts", i32_bytes(py, &ints(&|i| e.bicycle_lts(i)))?)?;
    let lat: Vec<f64> = cen.iter().map(|c| c.0).collect();
    let lon: Vec<f64> = cen.iter().map(|c| c.1).collect();
    out.set_item("lat", f64_bytes(py, &lat)?)?;
    out.set_item("lon", f64_bytes(py, &lon)?)?;
    out.set_item("n_points", i32_bytes(py, &n_points)?)?;
    out.set_item("x", f64_bytes(py, &x)?)?;
    out.set_item("y", f64_bytes(py, &y)?)?;
    Ok(out)
}

/// Line isochrones from a matrix of median minutes (int32, `i32::MIN`
/// where not reported), `n_origins` rows of `n_edges` values,
/// origin-major: the rows in r5r's order as `(origin, edge, travel_time)`
/// int32 buffers (0-based) and `isochrone` f64.
#[pyfunction]
fn isochrone_lines<'py>(
    py: Python<'py>,
    n_origins: usize,
    n_edges: usize,
    minutes: &[u8],
    cutoffs: Vec<f64>,
) -> PyResult<(Col<'py>, Col<'py>, Col<'py>, Col<'py>)> {
    if minutes.len() % 4 != 0 {
        return Err(value_err("the minutes buffer must hold whole int32 values"));
    }
    let m: Vec<Option<u32>> = minutes
        .chunks_exact(4)
        .map(|c| u32::try_from(i32::from_ne_bytes(c.try_into().expect("4 bytes"))).ok())
        .collect();
    let lines = isochrone_lines_core(n_origins, n_edges, &m, &cutoffs).map_err(value_err)?;
    let origin: Vec<i32> = lines.iter().map(|l| l.origin as i32).collect();
    let edge: Vec<i32> = lines.iter().map(|l| l.edge as i32).collect();
    let tt: Vec<i32> = lines.iter().map(|l| l.travel_time as i32).collect();
    let iso: Vec<f64> = lines.iter().map(|l| l.isochrone).collect();
    Ok((
        i32_bytes(py, &origin)?,
        i32_bytes(py, &edge)?,
        i32_bytes(py, &tt)?,
        f64_bytes(py, &iso)?,
    ))
}

#[pymodule]
fn _peabiru(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("NA_I32", NA_I32)?;
    m.add_class::<Network>()?;
    m.add_function(wrap_pyfunction!(core_version, m)?)?;
    m.add_function(wrap_pyfunction!(build_network, m)?)?;
    m.add_function(wrap_pyfunction!(street_ttm, m)?)?;
    m.add_function(wrap_pyfunction!(transit_ttm, m)?)?;
    m.add_function(wrap_pyfunction!(accessibility, m)?)?;
    m.add_function(wrap_pyfunction!(expanded, m)?)?;
    m.add_function(wrap_pyfunction!(isochrone_polygons, m)?)?;
    m.add_function(wrap_pyfunction!(street_edges, m)?)?;
    m.add_function(wrap_pyfunction!(isochrone_lines, m)?)?;
    Ok(())
}
