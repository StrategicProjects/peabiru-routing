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

use peabiru_core::street::{self, BuildOptions, Mode, RouteParams, StreetNetwork};
use peabiru_core::transit::accessibility::{
    street_accessibility, transit_accessibility, AccessibilityParams, Destinations,
};
use peabiru_core::transit::decay::DecayFunction;
use peabiru_core::transit::expanded::{
    arrival_travel_time_matrix, expanded_travel_time_matrix, ExpandedRow,
};
use peabiru_core::transit::matrix::{self, TransitData, TransitParams};
use peabiru_core::transit::{self as tr, TransitLayer};
use peabiru_core::{CancelToken, RouteError};
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
                d.set_item("feeds", t.layer.feed_ids.clone())?;
                d.set_item("stops", t.layer.stop_ids.len())?;
                d.set_item("patterns", t.layer.patterns.len())?;
                d.set_item("services", t.layer.services.len())?;
                d.set_item("frequencies", t.layer.has_frequencies)?;
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
        matrix::validate_date(date).map_err(value_err)?;
        let Some(t) = &self.transit else {
            return Ok((0, 0));
        };
        let wd = matrix::weekday(date);
        let active = t
            .layer
            .services
            .iter()
            .filter(|s| s.active_on(date, wd))
            .count();
        Ok((t.layer.services.len(), active))
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
        let opts = BuildOptions { n_threads };
        let (net, layer) = tr::build_from_dir(&data_path, &opts).map_err(runtime_err)?;
        let transit = if layer.feed_ids.is_empty() {
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
/// `(from, to, minutes)` int32 buffers, origin-major.
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
) -> PyResult<(Col<'py>, Col<'py>, Col<'py>)> {
    let n = network.get();
    let (o, d) = (points(olat, olon)?, points(dlat, dlon)?);
    let mode = street_mode(mode)?;
    let params = RouteParams {
        walk_speed_kmh: walk_speed,
        bike_speed_kmh: bike_speed,
        max_lts,
        max_trip_duration_min: max_trip_duration,
        n_threads,
        cancel: CancelToken::new(),
    };
    let (from, to, tt) = interruptible(py, &params.cancel, || {
        let m = street::travel_time_matrix(&n.net, &o, &d, mode, &params)?;
        let nd = d.len();
        let (mut from, mut to, mut tt) = (Vec::new(), Vec::new(), Vec::new());
        for (k, v) in m.iter().enumerate() {
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
    TransitParams {
        date,
        departure_time,
        time_window,
        percentiles,
        max_walk_time,
        max_trip_duration,
        walk_speed_kmh: walk_speed,
        max_rides,
        draws_per_minute,
        seed,
        n_threads,
        cancel: CancelToken::new(),
    }
}

/// Walk + transit travel-time matrix: the reported pairs as `from`, `to`
/// and one int32 buffer per percentile (ascending), with `i32::MIN` where
/// r5r gives NA; origin-major.
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
    let (from, to, cols) = interruptible(py, &p.cancel, || {
        let m = matrix::travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)?;
        let (mut from, mut to) = (Vec::new(), Vec::new());
        let mut cols: Vec<Vec<i32>> = vec![Vec::new(); np];
        for (i, row) in m.iter().enumerate() {
            for (j, cell) in row.iter().enumerate() {
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
/// opportunity column.
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
    let a = AccessibilityParams {
        cutoffs,
        decay: decay(decay_function, decay_value)?,
    };
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
    let values = interruptible(py, &cancel, || {
        let dest = Destinations {
            points: &d,
            opportunities: &opp,
        };
        match (street, t) {
            (None, Some(t)) => {
                let p = TransitParams {
                    cancel: cancel.clone(),
                    ..transit_params(
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
                    )
                };
                transit_accessibility(&n.net, &t.layer, &t.data, &o, &dest, &p, &a)
            }
            (Some(m), _) => {
                let params = RouteParams {
                    walk_speed_kmh: walk_speed,
                    bike_speed_kmh: bike_speed,
                    max_lts,
                    max_trip_duration_min: max_trip_duration,
                    n_threads,
                    cancel: cancel.clone(),
                };
                street_accessibility(&n.net, &o, &dest, m, &params, &percentiles, &a)
            }
            (None, None) => unreachable!("transit checked above"),
        }
    })?;
    f64_bytes(py, &values.values)
}

/// Expanded (or, with `arrival`, arrival) travel-time matrix as a dict of
/// columns: `from`, `to` (0-based int32), `departure` (int32 seconds after
/// midnight), `draw` (int32), the breakdown times (f64, NaN where r5r
/// gives NA), `n_rides` (int32, `i32::MIN` for NA), `total_time` (f64),
/// and `routes` dictionary-encoded as `route_codes` (int32, -1 for NA) into
/// the list of strings `route_levels`. For an arrival search the
/// parameters must already be the departure side (date and time of
/// `arrival - max_trip_duration`, `time_window = max_trip_duration`), as
/// r5r's R wrapper sets them.
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
    let cols = interruptible(py, &p.cancel, || {
        let rows: Vec<Vec<ExpandedRow>> = if arrival {
            arrival_travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)
        } else {
            expanded_travel_time_matrix(&n.net, &t.layer, &t.data, &o, &d, &p)
        }?;
        Ok(ExpandedColumns::new(rows))
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
    Ok(())
}
