//! What the commands share: errors and exit codes, building the network,
//! reading points, checking arguments as the packages check them (r5r's
//! `assign_*` and `set_*` functions, as the Python package ports them),
//! scenarios and fare structures, and writing CSV and GeoJSON.

use crate::app::{
    DrawArgs, ElevationArg, FareArgs, ModeArgs, NetworkArgs, ScenarioArgs, StreetArgs, TransitArgs,
};
use peabiru_core::{
    build_from_bytes, build_from_bytes_with_elevation, build_from_dir, build_network, BuildOptions,
    CancelToken, Elevation, FareStructure, Mode, Network, NetworkError, RouteError, RouteParams,
    Scenario, TransitMode, TransitParams,
};
use std::fmt::Display;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

/// A failure: the message and the exit status (1 a file or network, 2 an
/// argument, 130 an interruption).
#[derive(Debug)]
pub struct Fail {
    pub code: i32,
    pub msg: String,
}

pub type Res<T> = Result<T, Fail>;

/// A bad argument (exit status 2).
pub fn usage(msg: impl Display) -> Fail {
    Fail {
        code: 2,
        msg: msg.to_string(),
    }
}

/// A file or network that cannot be read or written (exit status 1).
pub fn runtime(msg: impl Display) -> Fail {
    Fail {
        code: 1,
        msg: msg.to_string(),
    }
}

impl From<RouteError> for Fail {
    fn from(e: RouteError) -> Self {
        match e {
            RouteError::Invalid(m) => usage(m),
            RouteError::Cancelled => Fail {
                code: 130,
                msg: "interrupted".into(),
            },
            other => runtime(other),
        }
    }
}

/// The token Ctrl-C cancels (see `main`).
pub fn cancel_token() -> &'static CancelToken {
    static TOKEN: OnceLock<CancelToken> = OnceLock::new();
    TOKEN.get_or_init(CancelToken::new)
}

/// Messages of a command on standard error.
pub struct Ctx {
    pub cmd: &'static str,
    pub quiet: bool,
    start: Instant,
}

impl Ctx {
    pub fn new(cmd: &'static str, quiet: bool) -> Self {
        Ctx {
            cmd,
            quiet,
            start: Instant::now(),
        }
    }

    /// A progress line, unless `--quiet`.
    pub fn progress(&self, msg: impl Display) {
        if !self.quiet {
            eprintln!(
                "peabiru {}: {msg} ({:.2} s)",
                self.cmd,
                self.start.elapsed().as_secs_f64()
            );
        }
    }

    pub fn warn(&self, msg: impl Display) {
        eprintln!("peabiru {}: warning: {msg}", self.cmd);
    }
}

// ---------------------------------------------------------------------------
// Numbers

/// A number as the packages take them (`Inf` and `-Inf` included).
pub fn number(name: &str, v: &str) -> Res<f64> {
    let t = v.trim();
    match t.to_ascii_lowercase().as_str() {
        "inf" | "+inf" | "infinity" => return Ok(f64::INFINITY),
        "-inf" | "-infinity" => return Ok(f64::NEG_INFINITY),
        _ => {}
    }
    t.parse::<f64>()
        .ok()
        .filter(|x| !x.is_nan())
        .ok_or_else(|| usage(format!("--{name} must be a number, got '{v}'")))
}

/// A whole number from `lo` (0 or 1) to the integer range of R.
pub fn count(name: &str, v: &str, lo: i64) -> Res<i32> {
    let x = number(name, v)?;
    if !x.is_finite() || x.fract() != 0.0 || x < lo as f64 || x > f64::from(i32::MAX) {
        return Err(usage(format!(
            "--{name} must be a whole number from {lo} to {}, got '{v}'",
            i32::MAX
        )));
    }
    Ok(x as i32)
}

/// A whole number of the integer range (the seed).
pub fn int(name: &str, v: &str) -> Res<i32> {
    let x = number(name, v)?;
    if !x.is_finite() || x.fract() != 0.0 || x.abs() > f64::from(i32::MAX) {
        return Err(usage(format!("--{name} must be a whole number, got '{v}'")));
    }
    Ok(x as i32)
}

/// A count or `Inf` (`None`).
pub fn count_or_inf(name: &str, v: &str) -> Res<Option<i32>> {
    let x = number(name, v)?;
    if x == f64::INFINITY {
        return Ok(None);
    }
    if x < 1.0 {
        return Err(usage(format!("--{name} must be at least 1, got '{v}'")));
    }
    count(name, v, 1).map(Some)
}

/// r5r's `set_speed()`: a positive finite number.
pub fn speed(name: &str, v: &str) -> Res<f64> {
    let x = number(name, v)?;
    if !x.is_finite() || x <= 0.0 {
        return Err(usage(format!(
            "--{name} must be a finite number greater than 0, got '{v}'"
        )));
    }
    Ok(x)
}

/// r5r's `set_max_lts()`: 1 to 4.
pub fn max_lts(v: &str) -> Res<i32> {
    let x = count("max-lts", v, 1)?;
    if x > 4 {
        return Err(usage(format!(
            "{x} is not a valid value for the maximum Level of Transit Stress (LTS); \
             --max-lts must be from 1 to 4"
        )));
    }
    Ok(x)
}

/// r5r's `set_percentiles()`: 1 to 5 distinct values from 1 to 99,
/// ascending.
pub fn percentiles(values: &[String]) -> Res<Vec<i32>> {
    let mut p = Vec::with_capacity(values.len());
    for v in values {
        let x = count("percentiles", v, 1)?;
        if x > 99 {
            return Err(usage(format!(
                "--percentiles must be from 1 to 99, got '{v}'"
            )));
        }
        p.push(x);
    }
    let n = p.len();
    p.sort_unstable();
    p.dedup();
    if n == 0 || n > 5 || p.len() != n {
        return Err(usage("--percentiles must be 1 to 5 distinct values"));
    }
    Ok(p)
}

/// The number of threads of `--threads` (0: every core).
pub fn threads(a: &NetworkArgs) -> usize {
    a.threads
}

// ---------------------------------------------------------------------------
// Modes and departures

/// r5r's transit modes, as its `assign_mode()` lists them.
const TRANSIT_NAMES: [&str; 9] = [
    "TRANSIT",
    "TRAM",
    "SUBWAY",
    "RAIL",
    "BUS",
    "FERRY",
    "CABLE_CAR",
    "GONDOLA",
    "FUNICULAR",
];
const DIRECT_NAMES: [&str; 5] = ["WALK", "BICYCLE", "CAR", "BICYCLE_RENT", "CAR_PARK"];

/// The modes of a call, as r5r's `assign_mode()` sorts them out.
#[derive(Clone, Debug)]
pub struct Modes {
    /// The street mode alone, or of the access to public transport.
    pub street: Mode,
    /// The street mode was given (or is the default walk to transit).
    pub transit: Vec<TransitMode>,
    pub egress: Mode,
}

impl Modes {
    pub fn has_transit(&self) -> bool {
        !self.transit.is_empty()
    }
}

pub fn street_mode(name: &str, v: &str) -> Res<Mode> {
    match v.trim().to_ascii_uppercase().as_str() {
        "WALK" => Ok(Mode::Walk),
        "BICYCLE" => Ok(Mode::Bicycle),
        "CAR" => Ok(Mode::Car),
        _ => Err(usage(format!(
            "--{name} must be WALK, BICYCLE or CAR, got '{v}'"
        ))),
    }
}

/// r5r's `assign_mode()`.
pub fn modes(mode: &[String], egress: &str) -> Res<Modes> {
    let mut seen: Vec<String> = Vec::new();
    for m in mode {
        let m = m.trim().to_ascii_uppercase();
        if m.is_empty() {
            continue;
        }
        if !TRANSIT_NAMES.contains(&m.as_str()) && !DIRECT_NAMES.contains(&m.as_str()) {
            return Err(usage(format!(
                "--mode: unknown mode '{m}' (WALK, BICYCLE, CAR, TRANSIT, TRAM, SUBWAY, RAIL, \
                 BUS, FERRY, CABLE_CAR, GONDOLA, FUNICULAR)"
            )));
        }
        if !seen.contains(&m) {
            seen.push(m);
        }
    }
    if seen.is_empty() {
        return Err(usage("--mode must name at least one mode"));
    }
    let egress = street_mode("mode-egress", egress)?;
    let direct: Vec<&String> = seen
        .iter()
        .filter(|m| DIRECT_NAMES.contains(&m.as_str()))
        .collect();
    let transit: Vec<&String> = seen
        .iter()
        .filter(|m| TRANSIT_NAMES.contains(&m.as_str()))
        .collect();
    if direct.len() > 1 {
        return Err(usage(format!(
            "--mode: use only one of {} when routing",
            direct
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if direct
        .iter()
        .any(|m| *m == "CAR_PARK" || *m == "BICYCLE_RENT")
    {
        return Err(usage(
            "CAR_PARK and BICYCLE_RENT are not supported (nor are they by r5r)",
        ));
    }
    let street = match direct.first() {
        Some(m) => street_mode("mode", m)?,
        None => Mode::Walk,
    };
    let transit = if transit.is_empty() {
        Vec::new()
    } else if transit.iter().any(|m| *m == "TRANSIT") {
        TransitMode::ALL.to_vec()
    } else {
        transit
            .iter()
            .map(|m| TransitMode::from_name(m).expect("a transit mode"))
            .collect()
    };
    Ok(Modes {
        street,
        transit,
        egress,
    })
}

/// A departure: `yyyymmdd` and seconds after midnight.
#[derive(Clone, Copy, Debug)]
pub struct When {
    pub date: u32,
    pub time: i32,
}

/// `YYYY-MM-DDTHH:MM[:SS]` (or with a space), as the network's local time.
pub fn datetime(name: &str, v: &str) -> Res<When> {
    let bad = || {
        usage(format!(
            "--{name} must be a date and time YYYY-MM-DDTHH:MM[:SS], got '{v}'"
        ))
    };
    let t = v.trim();
    let (d, c) = t.split_once(['T', ' ']).ok_or_else(bad)?;
    let date = date(name, d).map_err(|_| bad())?;
    let parts: Vec<&str> = c.split(':').collect();
    if !(2..=3).contains(&parts.len()) {
        return Err(bad());
    }
    let mut n = [0i32; 3];
    for (k, p) in parts.iter().enumerate() {
        if p.len() != 2 || !p.bytes().all(|b| b.is_ascii_digit()) {
            return Err(bad());
        }
        n[k] = p.parse().map_err(|_| bad())?;
    }
    if n[0] > 23 || n[1] > 59 || n[2] > 59 {
        return Err(bad());
    }
    Ok(When {
        date,
        time: n[0] * 3600 + n[1] * 60 + n[2],
    })
}

/// `YYYY-MM-DD` as `yyyymmdd`, a real calendar date.
pub fn date(name: &str, v: &str) -> Res<u32> {
    let bad = || usage(format!("--{name} must be a date YYYY-MM-DD, got '{v}'"));
    let f: Vec<&str> = v.trim().split('-').collect();
    let ok = f.len() == 3
        && f[0].len() == 4
        && f[1].len() == 2
        && f[2].len() == 2
        && f.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()));
    if !ok {
        return Err(bad());
    }
    let d: u32 = format!("{}{}{}", f[0], f[1], f[2])
        .parse()
        .map_err(|_| bad())?;
    peabiru_core::validate_date(d).map_err(|_| bad())?;
    Ok(d)
}

/// r5r's `assign_departure()`: required with public transport; without it
/// the date only has to be a date (street routing does not read it).
pub fn departure(v: Option<&str>, m: &Modes) -> Res<When> {
    match v {
        Some(v) => datetime("departure", v),
        None if m.has_transit() => Err(usage(
            "--departure is required with public transport: results depend on the departure \
             time",
        )),
        // as r5r, the current time (here in UTC: only itineraries show it)
        None => {
            let s = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let (days, secs) = (s.div_euclid(86400), s.rem_euclid(86400));
            let z = days + 719468;
            let era = z.div_euclid(146097);
            let doe = z - era * 146097;
            let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
            let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
            let mp = (5 * doy + 2) / 153;
            let d = doy - (153 * mp + 2) / 5 + 1;
            let m = if mp < 10 { mp + 3 } else { mp - 9 };
            let y = yoe + era * 400 + i64::from(m <= 2);
            Ok(When {
                date: (y * 10000 + m * 100 + d) as u32,
                time: secs as i32,
            })
        }
    }
}

/// r5r's `check_transit_availability_on_date()`.
pub fn check_service(ctx: &Ctx, net: &Network, date: u32) -> Res<()> {
    let (total, active) = match net.transit() {
        Some(t) => (t.n_services(), t.n_services_active_on(date)?),
        None => (0, 0),
    };
    let pct = if total == 0 {
        0.0
    } else {
        active as f64 / total as f64
    };
    let shown = format!("{}-{:02}-{:02}", date / 10000, date / 100 % 100, date % 100);
    if pct == 0.0 {
        return Err(usage(format!(
            "there are no transit services on the departure date {shown}: check that the \
             network has GTFS feeds and that the date falls within their calendars \
             (`peabiru info --service-dates` lists the dates)"
        )));
    }
    if pct < 0.2 {
        ctx.warn("less than 20% of the transit services in the GTFS run on the departure date");
    }
    Ok(())
}

/// The street limits of a call (r5r's `assign_max_street_time()` and
/// `assign_max_trip_duration()`), in minutes.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub walk: i32,
    pub bike: i32,
    pub car: i32,
    pub trip: i32,
}

/// The street options checked: speeds, `max_lts` and the limits.
#[derive(Clone, Copy, Debug)]
pub struct Street {
    pub walk_speed: f64,
    pub bike_speed: f64,
    pub max_lts: i32,
    pub lim: Limits,
}

/// r5r's street limits with `max_trip_duration` (already resolved, e.g.
/// capped by the cutoffs of accessibility) as `mtd`.
pub fn street(a: &StreetArgs, m: &Modes, mtd: i32) -> Res<Street> {
    let walk_speed = speed("walk-speed", &a.walk_speed)?;
    let bike_speed = speed("bike-speed", &a.bike_speed)?;
    let max_lts = max_lts(&a.max_lts)?;
    let t = |name: &str, v: &str| -> Res<i32> { Ok(count_or_inf(name, v)?.unwrap_or(mtd)) };
    let (walk, bike, car) = (
        t("max-walk-time", &a.max_walk_time)?,
        t("max-bike-time", &a.max_bike_time)?,
        t("max-car-time", &a.max_car_time)?,
    );
    let mut trip = mtd;
    if !m.has_transit() {
        let own = match m.street {
            Mode::Walk => walk,
            Mode::Bicycle => bike,
            _ => car,
        };
        trip = trip.min(own);
    }
    Ok(Street {
        walk_speed,
        bike_speed,
        max_lts,
        lim: Limits {
            walk,
            bike,
            car,
            trip,
        },
    })
}

/// `--max-trip-duration`.
pub fn max_trip_duration(a: &StreetArgs) -> Res<i32> {
    count("max-trip-duration", &a.max_trip_duration, 1)
}

/// r5r's `set_monte_carlo_draws()`; with McRAPTOR (fares, alternatives)
/// the draws R5 cannot sample are refused, as r5r refuses them.
pub fn check_draws(dpm: i32, tw: i32, mcraptor: bool) -> Res<()> {
    if !mcraptor {
        return Ok(());
    }
    let draws = i64::from(tw) * i64::from(dpm);
    if draws == 200 {
        return Err(usage(format!(
            "a --time-window of {tw} minutes with {dpm} draws per minute gives exactly 200 \
             Monte Carlo draws, which R5's McRaptor algorithm does not accept; use a slightly \
             different --time-window (see https://github.com/ipea/r5r/issues/602)"
        )));
    }
    let m = f64::from(60 / dpm);
    let mu = (m - 1.0) / 2.0 + (60 / dpm / 2) as f64;
    let span = 60.0 * f64::from(tw);
    let mut p = 0.0;
    if m >= 2.0 {
        let sd = (span * (m * m - 1.0) / 12.0 / mu.powi(3)).sqrt();
        let z = ((span - (m - 1.0) / 2.0) / mu + 1.0 - draws as f64) / sd;
        p = (-z * z / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt() / sd;
    }
    if p < 5e-5 {
        return Err(usage(format!(
            "--draws-per-minute {dpm} with a --time-window of {tw} minutes is not supported \
             with a fare structure: R5's McRaptor algorithm cannot sample this many departure \
             times; use fewer draws or a shorter window"
        )));
    }
    Ok(())
}

/// The public transport parameters of a call.
pub struct TransitCall {
    pub when: When,
    pub time_window: i32,
    pub max_rides: usize,
    pub draws: i32,
    pub seed: Option<i32>,
}

pub fn transit_call(t: &TransitArgs, d: Option<&DrawArgs>, when: When) -> Res<TransitCall> {
    let time_window = count("time-window", &t.time_window, 1)?;
    let max_rides = count("max-rides", &t.max_rides, 1)? as usize;
    let (draws, seed) = match d {
        Some(d) => (
            count("draws-per-minute", &d.draws_per_minute, 1)?,
            d.seed.as_deref().map(|s| int("seed", s)).transpose()?,
        ),
        None => (5, None),
    };
    Ok(TransitCall {
        when,
        time_window,
        max_rides,
        draws,
        seed,
    })
}

/// Street routing parameters.
pub fn route_params(s: &Street, trip: i32, threads: usize, scenario: &Scenario) -> RouteParams {
    let mut p = RouteParams::default();
    p.walk_speed_kmh = s.walk_speed;
    p.bike_speed_kmh = s.bike_speed;
    p.max_lts = s.max_lts;
    p.max_trip_duration_min = trip;
    p.n_threads = threads;
    p.scenario = scenario.clone();
    p.cancel = cancel_token().clone();
    p
}

/// Public transport parameters.
pub fn transit_params(
    m: &Modes,
    s: &Street,
    c: &TransitCall,
    pcts: Vec<i32>,
    threads: usize,
    scenario: &Scenario,
) -> TransitParams {
    let mut p = TransitParams::default();
    p.transit_modes = m.transit.clone();
    p.access_mode = m.street;
    p.egress_mode = m.egress;
    p.date = c.when.date;
    p.departure_time = c.when.time;
    p.time_window = c.time_window;
    p.percentiles = pcts;
    p.max_walk_time = s.lim.walk;
    p.max_bike_time = s.lim.bike;
    p.max_car_time = s.lim.car;
    p.max_trip_duration = s.lim.trip;
    p.walk_speed_kmh = s.walk_speed;
    p.bike_speed_kmh = s.bike_speed;
    p.max_lts = s.max_lts;
    p.max_rides = c.max_rides;
    p.draws_per_minute = c.draws;
    p.seed = c.seed;
    p.n_threads = threads;
    p.scenario = scenario.clone();
    p.cancel = cancel_token().clone();
    p
}

// ---------------------------------------------------------------------------
// Networks

/// A built network and what the commands need to know about how.
pub struct Built {
    pub net: Network,
    /// Whether r5r would see an elevation raster (then it does not swap
    /// origins and destinations of walking matrices).
    pub has_tif: bool,
}

/// The files of a directory as r5r's `list.files()` gives them (no hidden
/// files), sorted.
fn dir_files(dir: &Path) -> Res<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| runtime(format!("{}: {e}", dir.display())))?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort();
    Ok(names)
}

/// The elevation model of `--elevation`: with no raster r5r uses none.
fn elevation(a: &NetworkArgs, has_tif: bool) -> Elevation {
    if !has_tif {
        return Elevation::None;
    }
    match a.elevation {
        Some(ElevationArg::None) => Elevation::None,
        Some(ElevationArg::Minetti) => Elevation::Minetti,
        Some(ElevationArg::Tobler) | None => Elevation::Tobler,
    }
}

/// The error of a failed build; for feeds with errors of priority `HIGH`,
/// which feeds, and how to see the errors.
fn build_error(e: &NetworkError, a: &NetworkArgs) -> Fail {
    match e.high_priority_gtfs_errors() {
        None => runtime(e),
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
            runtime(format!(
                "high priority GTFS errors found; network build failed ({}). See them with \
                 `peabiru info {} --gtfs-errors`, or build anyway with --ignore-gtfs-errors",
                list.join(", "),
                a.network.display()
            ))
        }
    }
}

/// The paths of the GTFS feeds of `--gtfs` or of a directory.
pub fn feed_paths(a: &NetworkArgs) -> Res<Vec<PathBuf>> {
    if a.network.is_dir() {
        peabiru_core::gtfs_files_in_dir(&a.network)
            .map_err(|e| runtime(format!("{}: {e}", a.network.display())))
    } else {
        Ok(a.gtfs.clone())
    }
}

/// Build the network of `a`, with its warnings on standard error.
pub fn build(ctx: &Ctx, a: &NetworkArgs) -> Res<Built> {
    let read = |p: &Path| {
        std::fs::read(p).map_err(|e| runtime(format!("cannot read {}: {e}", p.display())))
    };
    let mut opts = BuildOptions::default();
    opts.n_threads = threads(a);
    opts.ignore_gtfs_errors = a.ignore_gtfs_errors;
    let (net, has_tif) = if a.network.is_dir() {
        if !a.gtfs.is_empty() || a.tif.is_some() {
            return Err(usage(
                "--gtfs and --tif go with a .pbf NETWORK; a directory's feeds and raster are \
                 its own files",
            ));
        }
        let files = dir_files(&a.network)?;
        let pbf: Vec<&String> = files.iter().filter(|f| f.ends_with(".pbf")).collect();
        if pbf.is_empty() {
            return Err(runtime(format!(
                "an OpenStreetMap .pbf file is required in {} to build a network",
                a.network.display()
            )));
        }
        if pbf.len() > 1 {
            return Err(runtime(format!(
                "{} has more than one .pbf file ({}); keep only one",
                a.network.display(),
                pbf.iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        // r5r looks for ".tif" as a regular expression: any character
        // before "tif" anywhere in the name
        let has_tif = files
            .iter()
            .any(|f| f.get(1..).is_some_and(|s| s.contains("tif")));
        opts.elevation = elevation(a, has_tif);
        let net = build_from_dir(&a.network, &opts).map_err(|e| build_error(&e, a))?;
        (net, has_tif)
    } else if !a.network.exists() {
        return Err(runtime(format!(
            "{}: no such file or directory",
            a.network.display()
        )));
    } else {
        let has_tif = a.tif.is_some();
        opts.elevation = elevation(a, has_tif);
        if a.gtfs.is_empty() && a.tif.is_none() {
            (
                build_network(&a.network, &opts).map_err(|e| build_error(&e, a))?,
                false,
            )
        } else {
            let pbf = read(&a.network)?;
            let mut feeds = Vec::with_capacity(a.gtfs.len());
            for f in &a.gtfs {
                let name = f.file_name().map_or_else(
                    || f.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                feeds.push((name, read(f)?));
            }
            let named: Vec<(&str, &[u8])> = feeds
                .iter()
                .map(|(n, b)| (n.as_str(), b.as_slice()))
                .collect();
            let net = match &a.tif {
                Some(t) => build_from_bytes_with_elevation(&pbf, &named, &read(t)?, &opts),
                None => build_from_bytes(&pbf, &named, &opts),
            }
            .map_err(|e| {
                let mut f = build_error(&e, a);
                f.msg = format!("{}: {}", a.network.display(), f.msg);
                f
            })?;
            (net, has_tif)
        }
    };
    for w in net.build_warnings() {
        ctx.warn(w);
    }
    let streets = net.street();
    match net.transit() {
        Some(t) => ctx.progress(format!(
            "network built: {} street vertices, {} stops and {} trip patterns from {} feed{}",
            streets.n_vertices(),
            t.n_stops(),
            t.n_patterns(),
            t.feed_ids().len(),
            if t.feed_ids().len() == 1 { "" } else { "s" }
        )),
        None => ctx.progress(format!(
            "network built: {} street vertices, no public transport",
            streets.n_vertices()
        )),
    }
    Ok(Built { net, has_tif })
}

// ---------------------------------------------------------------------------
// CSV input

/// A CSV file: its header and rows (RFC 4180: quoted fields, doubled
/// quotes, line breaks inside quotes; CRLF or LF; a byte order mark).
pub struct Csv {
    pub path: PathBuf,
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Csv {
    pub fn read(path: &Path) -> Res<Csv> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| runtime(format!("cannot read {}: {e}", path.display())))?;
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        let mut records = parse_csv(text)
            .map_err(|line| {
                runtime(format!(
                    "{}:{line}: unterminated quoted field",
                    path.display()
                ))
            })?
            .into_iter();
        let header = records
            .next()
            .ok_or_else(|| runtime(format!("{}: the file is empty", path.display())))?;
        let header: Vec<String> = header.into_iter().map(|h| h.trim().to_string()).collect();
        let rows = records
            .filter(|r| !(r.len() == 1 && r[0].trim().is_empty()))
            .collect();
        Ok(Csv {
            path: path.to_path_buf(),
            header,
            rows,
        })
    }

    /// The index of column `name`.
    pub fn col(&self, name: &str) -> Res<usize> {
        self.header
            .iter()
            .position(|h| h == name)
            .ok_or_else(|| runtime(format!("{}: no '{name}' column", self.path.display())))
    }

    /// The field of row `r` in column `c` ("" when the row is short).
    pub fn get(&self, r: usize, c: usize) -> &str {
        self.rows[r].get(c).map_or("", String::as_str)
    }

    /// A number in row `r`, column `c`: `None` for an empty field or `NA`.
    pub fn num(&self, r: usize, c: usize) -> Res<Option<f64>> {
        let s = self.get(r, c).trim();
        if s.is_empty() || s == "NA" {
            return Ok(None);
        }
        match s.to_ascii_lowercase().as_str() {
            "inf" => return Ok(Some(f64::INFINITY)),
            "-inf" => return Ok(Some(f64::NEG_INFINITY)),
            _ => {}
        }
        s.parse::<f64>().map(Some).map_err(|_| {
            runtime(format!(
                "{}:{}: {} '{s}' is not a number",
                self.path.display(),
                r + 2,
                self.header[c]
            ))
        })
    }
}

/// The records of a CSV text; `Err(line)` for a quote never closed.
fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, usize> {
    let mut out = Vec::new();
    let mut rec = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut line = 1;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                c => {
                    if c == '\n' {
                        line += 1;
                    }
                    field.push(c);
                }
            }
            continue;
        }
        match c {
            '"' if field.trim().is_empty() => {
                field.clear();
                quoted = true;
                started = true;
            }
            ',' => {
                rec.push(std::mem::take(&mut field));
                started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                line += 1;
                rec.push(std::mem::take(&mut field));
                out.push(std::mem::take(&mut rec));
                started = false;
            }
            c => {
                field.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err(line);
    }
    if started || !field.is_empty() {
        rec.push(field);
        out.push(rec);
    }
    Ok(out)
}

/// Points of a CSV file with `id`, `lon` and `lat` (and the numeric
/// columns asked for).
#[derive(Clone)]
pub struct Points {
    pub ids: Vec<String>,
    /// `(lat, lon)`, NaN where missing.
    pub coords: Vec<(f64, f64)>,
    pub columns: Vec<Vec<f64>>,
}

impl Points {
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// The points at `idx`.
    pub fn pick(&self, idx: &[usize]) -> Points {
        Points {
            ids: idx.iter().map(|&i| self.ids[i].clone()).collect(),
            coords: idx.iter().map(|&i| self.coords[i]).collect(),
            columns: self
                .columns
                .iter()
                .map(|c| idx.iter().map(|&i| c[i]).collect())
                .collect(),
        }
    }
}

/// r5r's `assign_points_input()` on a CSV file, and
/// `assign_opportunities()` for `columns`: points with a missing
/// coordinate are not routed (a warning), duplicated ids are an error
/// when `unique`, missing opportunity counts are 0 (a warning).
pub fn read_points(
    ctx: &Ctx,
    path: &Path,
    what: &str,
    unique: bool,
    columns: &[String],
) -> Res<Points> {
    let csv = Csv::read(path)?;
    let (ci, clon, clat) = (csv.col("id")?, csv.col("lon")?, csv.col("lat")?);
    let cols: Vec<usize> = columns.iter().map(|c| csv.col(c)).collect::<Res<_>>()?;
    let mut p = Points {
        ids: Vec::with_capacity(csv.rows.len()),
        coords: Vec::with_capacity(csv.rows.len()),
        columns: vec![Vec::with_capacity(csv.rows.len()); cols.len()],
    };
    let mut missing = Vec::new();
    let mut na_counts = vec![0usize; cols.len()];
    for r in 0..csv.rows.len() {
        let id = csv.get(r, ci).to_string();
        let lon = csv.num(r, clon)?.unwrap_or(f64::NAN);
        let lat = csv.num(r, clat)?.unwrap_or(f64::NAN);
        if lon.is_nan() || lat.is_nan() {
            missing.push(format!("\"{id}\""));
        }
        p.ids.push(id);
        p.coords.push((lat, lon));
        for (k, &c) in cols.iter().enumerate() {
            let v = csv.num(r, c)?;
            match v {
                Some(x) if x.is_infinite() => {
                    return Err(runtime(format!(
                        "{}:{}: {} must be finite",
                        path.display(),
                        r + 2,
                        columns[k]
                    )))
                }
                Some(x) => p.columns[k].push(x),
                None => {
                    na_counts[k] += 1;
                    p.columns[k].push(0.0);
                }
            }
        }
    }
    if !missing.is_empty() {
        ctx.warn(format!(
            "{} point(s) of the {what} have missing lon/lat coordinates and will not be \
             routed: {}",
            missing.len(),
            missing.join(", ")
        ));
    }
    for (k, n) in na_counts.iter().enumerate() {
        if *n > 0 {
            ctx.warn(format!(
                "'{}' has {n} missing value(s), treated as 0",
                columns[k]
            ));
        }
    }
    if unique {
        let mut seen = std::collections::HashSet::new();
        let mut dups = Vec::new();
        for id in &p.ids {
            if !seen.insert(id.as_str()) && !dups.contains(id) {
                dups.push(id.clone());
            }
        }
        if !dups.is_empty() {
            return Err(runtime(format!(
                "{}: the {what} must not have duplicated ids ({})",
                path.display(),
                dups.iter()
                    .map(|d| format!("\"{d}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    Ok(p)
}

/// The origins and destinations of a command (`--destinations` defaults
/// to the origins).
pub fn read_od(
    ctx: &Ctx,
    o: &Path,
    d: Option<&Path>,
    unique: bool,
    columns: &[String],
) -> Res<(Points, Points)> {
    let origins = read_points(ctx, o, "origins", unique, &[])?;
    let dest = read_points(ctx, d.unwrap_or(o), "destinations", unique, columns)?;
    Ok((origins, dest))
}

// ---------------------------------------------------------------------------
// Fares and scenarios

/// A fare structure file, checked against the network.
pub fn fare_structure(path: &Path, net: Option<&Network>) -> Res<FareStructure> {
    let bytes =
        std::fs::read(path).map_err(|e| runtime(format!("cannot read {}: {e}", path.display())))?;
    let fs = peabiru_core::read_fare_structure(&bytes, "UTF-8")
        .map_err(|e| runtime(format!("{}: {e}", path.display())))?;
    fs.validate()?;
    if let Some(n) = net {
        fs.validate_for(n)?;
    }
    Ok(fs)
}

/// `--fare-structure` and `--max-fare` (r5r's `fare_arg` checks).
pub fn fares(a: &FareArgs, net: &Network) -> Res<(Option<Arc<FareStructure>>, f64)> {
    let fs = a
        .fare_structure
        .as_deref()
        .map(|p| fare_structure(p, Some(net)))
        .transpose()?;
    let max_fare = number("max-fare", &a.max_fare)?;
    if max_fare < 0.0 {
        return Err(usage(format!(
            "--max-fare must be >= 0, got '{}'",
            a.max_fare
        )));
    }
    if max_fare.is_finite() && fs.is_none() {
        return Err(usage(
            "--max-fare needs a --fare-structure: without one no fares are calculated and the \
             limit would be ignored",
        ));
    }
    Ok((fs.map(Arc::new), max_fare))
}

/// A GeoJSON file of features.
fn geojson_features(path: &Path) -> Res<Vec<serde_json::Value>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| runtime(format!("cannot read {}: {e}", path.display())))?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| runtime(format!("{}: not GeoJSON: {e}", path.display())))?;
    match v.get("features").and_then(|f| f.as_array()) {
        Some(f) => Ok(f.clone()),
        None => Err(runtime(format!(
            "{}: a GeoJSON FeatureCollection is expected",
            path.display()
        ))),
    }
}

fn is_geojson(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("geojson") || e.eq_ignore_ascii_case("json"))
}

/// The coordinate pairs of a GeoJSON position array.
fn positions(path: &Path, v: &serde_json::Value) -> Res<Vec<[f64; 2]>> {
    let bad = || runtime(format!("{}: bad coordinates in a geometry", path.display()));
    v.as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|p| {
            let a = p.as_array().ok_or_else(bad)?;
            match (
                a.first().and_then(|x| x.as_f64()),
                a.get(1).and_then(|x| x.as_f64()),
            ) {
                (Some(x), Some(y)) => Ok([x, y]),
                _ => Err(bad()),
            }
        })
        .collect()
}

fn property<'a>(path: &Path, f: &'a serde_json::Value, name: &str) -> Res<&'a serde_json::Value> {
    f.get("properties")
        .and_then(|p| p.get(name))
        .ok_or_else(|| {
            runtime(format!(
                "{}: a feature has no '{name}' property",
                path.display()
            ))
        })
}

fn prop_f64(path: &Path, f: &serde_json::Value, name: &str) -> Res<f64> {
    property(path, f, name)?
        .as_f64()
        .ok_or_else(|| runtime(format!("{}: '{name}' must be a number", path.display())))
}

/// The scenario of `--new-carspeeds`, `--carspeed-scale` and `--new-lts`
/// (warning, as r5r does, that a scenario drops the slope costs).
pub fn scenario(ctx: &Ctx, a: &ScenarioArgs, net: &Network) -> Res<Scenario> {
    use peabiru_core::unstable::{
        gdal_geojson_coordinate, java_parse_float, java_parse_int, java_parse_long,
    };
    use peabiru_core::{CarSpeeds, LtsChange, LtsLine, SpeedPolygon};
    let mut s = Scenario::default();
    let scale = number("carspeed-scale", &a.carspeed_scale)?;
    if !scale.is_finite() || scale < 0.0 {
        return Err(usage(format!(
            "--carspeed-scale must be a finite number >= 0, got '{}'",
            a.carspeed_scale
        )));
    }
    s.car_speed_scale = scale as f32;
    let changed = a.new_carspeeds.is_some() || scale != 1.0 || a.new_lts.is_some();
    if let Some(p) = &a.new_carspeeds {
        if is_geojson(p) {
            let mut polys = Vec::new();
            for f in geojson_features(p)? {
                let g = f.get("geometry").ok_or_else(|| {
                    runtime(format!("{}: a feature has no geometry", p.display()))
                })?;
                let kind = g.get("type").and_then(|t| t.as_str()).unwrap_or("");
                let coords = g.get("coordinates").cloned().unwrap_or_default();
                let one = |v: &serde_json::Value| -> Res<peabiru_core::Polygon> {
                    v.as_array()
                        .ok_or_else(|| runtime(format!("{}: bad polygon", p.display())))?
                        .iter()
                        .map(|r| {
                            Ok(positions(p, r)?
                                .into_iter()
                                .map(|[x, y]| {
                                    [gdal_geojson_coordinate(x), gdal_geojson_coordinate(y)]
                                })
                                .collect())
                        })
                        .collect()
                };
                let parts = match kind {
                    "Polygon" => vec![one(&coords)?],
                    "MultiPolygon" => coords
                        .as_array()
                        .ok_or_else(|| runtime(format!("{}: bad multipolygon", p.display())))?
                        .iter()
                        .map(one)
                        .collect::<Res<_>>()?,
                    other => {
                        return Err(runtime(format!(
                        "{}: --new-carspeeds polygons must be Polygon or MultiPolygon, got {other}",
                        p.display()
                    )))
                    }
                };
                property(p, &f, "poly_id")?;
                let sc = prop_f64(p, &f, "scale")?;
                let pr = prop_f64(p, &f, "priority")?;
                polys.push(SpeedPolygon::new(parts, sc, pr));
            }
            s.car_speeds = CarSpeeds::ByPolygon(polys);
        } else {
            let csv = Csv::read(p)?;
            let (ci, cv, ct) = (
                csv.col("osm_id")?,
                csv.col("max_speed")?,
                csv.col("speed_type")?,
            );
            let mut types: Vec<String> = Vec::new();
            let mut pairs = Vec::new();
            for r in 0..csv.rows.len() {
                let t = csv.get(r, ct).trim().to_string();
                if !types.contains(&t) {
                    types.push(t);
                }
                let id = csv.get(r, ci).trim();
                let v = csv.get(r, cv).trim();
                let id = java_parse_long(id).ok_or_else(|| {
                    runtime(format!(
                        "{}:{}: osm_id '{id}' is not an integer",
                        p.display(),
                        r + 2
                    ))
                })?;
                let v = java_parse_float(v).ok_or_else(|| {
                    runtime(format!(
                        "{}:{}: max_speed '{v}' is not a number",
                        p.display(),
                        r + 2
                    ))
                })?;
                pairs.push((id, v));
            }
            let types: Vec<&str> = types.iter().map(String::as_str).collect();
            s.car_speeds = match types.as_slice() {
                ["km/h"] => CarSpeeds::KmhByWay(pairs),
                ["scale"] => CarSpeeds::ScaleByWay(pairs),
                _ => {
                    return Err(runtime(format!(
                        "{}: speed_type must be the same in every row, km/h or scale",
                        p.display()
                    )))
                }
            };
        }
    }
    if let Some(p) = &a.new_lts {
        if is_geojson(p) {
            let mut lines = Vec::new();
            for f in geojson_features(p)? {
                let g = f.get("geometry").ok_or_else(|| {
                    runtime(format!("{}: a feature has no geometry", p.display()))
                })?;
                if g.get("type").and_then(|t| t.as_str()) != Some("LineString") {
                    return Err(runtime(format!(
                        "{}: --new-lts lines must be LineString",
                        p.display()
                    )));
                }
                let pts = positions(p, g.get("coordinates").unwrap_or(&serde_json::Value::Null))?;
                property(p, &f, "line_id")?;
                property(p, &f, "priority")?;
                let lts = prop_f64(p, &f, "lts")?;
                if lts.fract() != 0.0 || !(1.0..=4.0).contains(&lts) {
                    return Err(runtime(format!("{}: lts must be 1 to 4", p.display())));
                }
                lines.push(LtsLine::new(pts, lts as i32));
            }
            s.lts = LtsChange::ByLine(lines);
        } else {
            let csv = Csv::read(p)?;
            let (ci, cl) = (csv.col("osm_id")?, csv.col("lts")?);
            let mut pairs = Vec::new();
            for r in 0..csv.rows.len() {
                let (id, l) = (csv.get(r, ci).trim(), csv.get(r, cl).trim());
                let id = java_parse_long(id).ok_or_else(|| {
                    runtime(format!(
                        "{}:{}: osm_id '{id}' is not an integer",
                        p.display(),
                        r + 2
                    ))
                })?;
                let l = java_parse_int(l).ok_or_else(|| {
                    runtime(format!(
                        "{}:{}: lts '{l}' is not an integer",
                        p.display(),
                        r + 2
                    ))
                })?;
                pairs.push((id, l));
            }
            s.lts = LtsChange::ByWay(pairs);
        }
    }
    if changed && net.street().elevation() != Elevation::None {
        ctx.warn(
            "a scenario was used for this calculation: walking and cycling are routed without \
             the slope costs of elevation (as in r5r, issue #555)",
        );
    }
    Ok(s)
}

// ---------------------------------------------------------------------------
// Output

/// Where a result goes: a file or standard output.
pub struct Out {
    w: Box<dyn Write>,
    name: String,
}

impl Out {
    pub fn open(path: Option<&Path>) -> Res<Out> {
        Ok(match path {
            Some(p) if p.as_os_str() != "-" => Out {
                w: Box::new(std::io::BufWriter::new(std::fs::File::create(p).map_err(
                    |e| runtime(format!("cannot write {}: {e}", p.display())),
                )?)),
                name: p.display().to_string(),
            },
            _ => Out {
                w: Box::new(std::io::BufWriter::new(std::io::stdout())),
                name: "standard output".into(),
            },
        })
    }

    fn fail(&self, e: std::io::Error) -> Fail {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            // the reader went away (`| head`): stop quietly
            std::process::exit(0);
        }
        runtime(format!("cannot write {}: {e}", self.name))
    }

    pub fn write_str(&mut self, s: &str) -> Res<()> {
        let r = self.w.write_all(s.as_bytes());
        r.map_err(|e| self.fail(e))
    }

    /// A CSV record (RFC 4180 quoting).
    pub fn row<S: AsRef<str>>(&mut self, fields: &[S]) -> Res<()> {
        let mut line = String::new();
        for (k, f) in fields.iter().enumerate() {
            if k > 0 {
                line.push(',');
            }
            line.push_str(&quote(f.as_ref()));
        }
        line.push('\n');
        self.write_str(&line)
    }

    pub fn finish(mut self) -> Res<()> {
        let r = self.w.flush();
        r.map_err(|e| self.fail(e))
    }
}

/// A CSV field, quoted when it holds a comma, a quote or a line break.
pub fn quote(s: &str) -> std::borrow::Cow<'_, str> {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\"")).into()
    } else {
        s.into()
    }
}

/// A number for CSV and GeoJSON: the shortest text that reads back to the
/// same double; empty for a missing value (NaN).
pub fn num(x: f64) -> String {
    if x.is_nan() {
        String::new()
    } else if x.is_infinite() {
        if x > 0.0 {
            "Inf".into()
        } else {
            "-Inf".into()
        }
    } else {
        format!("{x}")
    }
}

/// An optional value for CSV: empty when missing.
pub fn opt<T: Display>(x: Option<T>) -> String {
    x.map_or_else(String::new, |v| v.to_string())
}

/// `HH:MM:SS`, hours past 24 as they come.
pub fn clock(s: i32) -> String {
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// A property value of a GeoJSON feature.
pub enum J<'a> {
    S(&'a str),
    N(f64),
    I(i64),
    B(bool),
    Null,
}

impl J<'_> {
    fn json(&self) -> String {
        match self {
            J::S(s) => serde_json::to_string(s).expect("a string"),
            J::N(x) if x.is_finite() => format!("{x}"),
            J::N(_) | J::Null => "null".into(),
            J::I(i) => i.to_string(),
            J::B(b) => b.to_string(),
        }
    }
}

/// A string property, `null` when missing.
pub fn js(s: Option<&str>) -> J<'_> {
    s.map_or(J::Null, J::S)
}

/// A geometry of a GeoJSON feature, coordinates `[lon, lat]`.
pub enum Geom<'a> {
    Point([f64; 2]),
    Line(&'a [[f64; 2]]),
    MultiPolygon(&'a [peabiru_core::Polygon]),
    Polygon(Vec<Vec<[f64; 2]>>),
    None,
}

fn coords(out: &mut String, pts: &[[f64; 2]]) {
    out.push('[');
    for (k, p) in pts.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        out.push_str(&format!("[{},{}]", p[0], p[1]));
    }
    out.push(']');
}

fn rings(out: &mut String, poly: &[Vec<[f64; 2]>]) {
    out.push('[');
    for (k, r) in poly.iter().enumerate() {
        if k > 0 {
            out.push(',');
        }
        coords(out, r);
    }
    out.push(']');
}

impl Geom<'_> {
    fn json(&self) -> String {
        let mut s = String::new();
        match self {
            Geom::Point(p) => s.push_str(&format!(
                "{{\"type\":\"Point\",\"coordinates\":[{},{}]}}",
                p[0], p[1]
            )),
            Geom::Line(pts) => {
                s.push_str("{\"type\":\"LineString\",\"coordinates\":");
                coords(&mut s, pts);
                s.push('}');
            }
            Geom::MultiPolygon(polys) => {
                s.push_str("{\"type\":\"MultiPolygon\",\"coordinates\":[");
                for (k, p) in polys.iter().enumerate() {
                    if k > 0 {
                        s.push(',');
                    }
                    rings(&mut s, p);
                }
                s.push_str("]}");
            }
            Geom::Polygon(p) => {
                s.push_str("{\"type\":\"Polygon\",\"coordinates\":");
                rings(&mut s, p);
                s.push('}');
            }
            Geom::None => s.push_str("null"),
        }
        s
    }

    /// The geometry as WKT, for CSV.
    pub fn wkt(&self) -> String {
        let pts = |v: &[[f64; 2]]| {
            v.iter()
                .map(|p| format!("{} {}", p[0], p[1]))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let poly = |p: &[Vec<[f64; 2]>]| {
            p.iter()
                .map(|r| format!("({})", pts(r)))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self {
            Geom::Point(p) => format!("POINT ({} {})", p[0], p[1]),
            Geom::Line([]) => "LINESTRING EMPTY".into(),
            Geom::Line(v) => format!("LINESTRING ({})", pts(v)),
            Geom::MultiPolygon([]) => "MULTIPOLYGON EMPTY".into(),
            Geom::MultiPolygon(v) => format!(
                "MULTIPOLYGON ({})",
                v.iter()
                    .map(|p| format!("({})", poly(p)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Geom::Polygon(p) => format!("POLYGON ({})", poly(p)),
            Geom::None => String::new(),
        }
    }
}

/// A GeoJSON FeatureCollection written feature by feature.
pub struct GeoJson {
    out: Out,
    n: usize,
}

impl GeoJson {
    pub fn start(mut out: Out) -> Res<GeoJson> {
        out.write_str("{\"type\":\"FeatureCollection\",\"features\":[\n")?;
        Ok(GeoJson { out, n: 0 })
    }

    pub fn feature(&mut self, props: &[(&str, J<'_>)], geom: Geom<'_>) -> Res<()> {
        let mut s = String::new();
        if self.n > 0 {
            s.push_str(",\n");
        }
        s.push_str("{\"type\":\"Feature\",\"properties\":{");
        for (k, (name, v)) in props.iter().enumerate() {
            if k > 0 {
                s.push(',');
            }
            s.push_str(&serde_json::to_string(name).expect("a name"));
            s.push(':');
            s.push_str(&v.json());
        }
        s.push_str("},\"geometry\":");
        s.push_str(&geom.json());
        s.push('}');
        self.n += 1;
        self.out.write_str(&s)
    }

    pub fn finish(mut self) -> Res<()> {
        self.out.write_str("\n]}\n")?;
        self.out.finish()
    }
}

/// `--output-dir`: an existing directory.
pub fn output_dir(p: &Path) -> Res<String> {
    if !p.is_dir() {
        return Err(usage(format!(
            "--output-dir: directory '{}' does not exist",
            p.display()
        )));
    }
    Ok(p.to_string_lossy().into_owned())
}

/// The core's check of the ids file names are built from (with
/// `--output-dir`), before anything is routed.
pub fn check_file_ids(ids: &[&String]) -> Res<()> {
    Ok(peabiru_core::unstable::check_output_ids(
        ids.iter().map(|s| s.as_str()),
    )?)
}

/// `--mode`, or the command's default.
pub fn mode_list<'a>(a: &'a ModeArgs, default: &'a [String]) -> &'a [String] {
    a.mode.as_deref().unwrap_or(default)
}
