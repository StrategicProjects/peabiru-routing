//! Public-transport travel-time matrices, as r5r's `travel_time_matrix()`
//! with public transport in `mode`: access by the street mode (carried on
//! on foot after a bicycle or car), RAPTOR over the departure window on the
//! patterns of the chosen transit modes, egress by `mode_egress`,
//! percentiles over the iterations (R5 `TravelTimeComputer`,
//! `PerTargetPropagater`, `TravelTimeReducer`).

use super::egress::{mode_egress_tables, Targets};
use super::mcraptor::{self, Dominance, DrawOut, McRaptor, McRequest};
use super::raptor::{self, Filtered, RaptorRequest, StopTimes, TransitMode, UNREACHED};
use super::walk;
use super::TransitLayer;
use crate::fares::{FareCalculator, FareStructure};
use crate::isochrone::linkage::{grid_links, Links};
use crate::street::link::{find_split, link_points};
use crate::street::router::{
    access_search_from, check_speed, worker_threads, Mode, ModeGraph, Search,
};
use crate::street::StreetNetwork;
use crate::{CancelToken, RouteError};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Public transport routing parameters (modes, access, egress and transfers),
/// with r5r's defaults but for the date and departure time, which have
/// none. Fill it in by changing the fields of [`TransitParams::default()`]:
///
/// ```
/// let mut p = peabiru_core::TransitParams::default();
/// p.date = 20190513; // a Monday
/// p.departure_time = 14 * 3600; // 14:00
/// p.percentiles = vec![25, 50, 75];
/// assert!(p.validate().is_ok());
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TransitParams {
    /// Service date `yyyymmdd`; it must be a real calendar date. The
    /// default, 0, is a placeholder that [`TransitParams::validate`]
    /// rejects: there is no default date, as r5r has no default departure.
    pub date: u32,
    /// Departure, seconds after midnight of `date` (up to 7 days; the
    /// default 0 is midnight).
    pub departure_time: i32,
    /// Departure window in minutes: a search departs at every minute of
    /// `[departure_time, departure_time + time_window)` (default 10).
    pub time_window: i32,
    /// Travel-time percentiles: 1 to 5 values in 1..=99 (see
    /// [`validate_percentiles`]). Any order and repeats are accepted; the
    /// results list them in ascending order, a repeated value once per
    /// occurrence. r5r sorts them and rejects repeats before calling R5,
    /// which wants them ascending. The expanded and arrival matrices do not
    /// use them but check them all the same, so keep the default there.
    pub percentiles: Vec<i32>,
    /// Longest walk in minutes at each end of a trip and in each transfer
    /// (default 120); r5r's `max_walk_time`, where its `Inf` is
    /// `max_trip_duration`.
    pub max_walk_time: i32,
    /// Trip limit in whole minutes (default 120).
    pub max_trip_duration: i32,
    /// Walking speed, km/h (default 3.6).
    pub walk_speed_kmh: f64,
    /// The street mode that reaches public transport, which is also the
    /// mode of the trip without it: r5r's street mode next to the transit
    /// modes in `mode` (default [`Mode::Walk`]). By bicycle or car the
    /// search goes on on foot from wherever the vehicle got, within the
    /// same time limit, as R5 does (`keepRoutingOnFoot`), so that stops in
    /// pedestrian-only areas are reached too; the trip without public
    /// transport keeps those walking ends as well.
    pub access_mode: Mode,
    /// The street mode from public transport to the destinations, r5r's
    /// `mode_egress` (default [`Mode::Walk`]). By bicycle or car R5 links
    /// every stop again by that mode and searches up to 5 km by bicycle,
    /// or below 30 minutes by car, with its default request (any level of
    /// traffic stress); the egress then must stay below `max_bike_time` or
    /// `max_car_time`, at `bike_speed_kmh` by bicycle.
    pub egress_mode: Mode,
    /// Cycling speed, km/h (default 12), for bicycle access and egress.
    pub bike_speed_kmh: f64,
    /// Highest level of traffic stress a cyclist rides on, 1 to 4 (default
    /// 2), for bicycle access (R5's bicycle egress ignores it, see
    /// `egress_mode`).
    pub max_lts: i32,
    /// Longest bicycle access in minutes, the walk after it included, and
    /// longest bicycle egress (default 120); r5r's `max_bike_time`, where
    /// its `Inf` is `max_trip_duration`.
    pub max_bike_time: i32,
    /// Longest car access in minutes, the walk after it included, and
    /// longest car egress (default 120); r5r's `max_car_time`, where its
    /// `Inf` is `max_trip_duration`.
    pub max_car_time: i32,
    /// Most vehicles boarded in one trip (default 3).
    pub max_rides: usize,
    /// The public transport modes to ride, r5r's transit modes in `mode`
    /// (default: all of them, r5r's `"TRANSIT"`). Routes of other modes
    /// are left out of the search; it must not be empty.
    pub transit_modes: Vec<TransitMode>,
    /// Monte Carlo draws per departure minute for frequency-based routes
    /// (default 5).
    pub draws_per_minute: i32,
    /// Seed of the Monte Carlo draws, r5r's `seed`: with the same seed the
    /// results are those of r5r with that seed. `None` (the default) seeds
    /// from the clock, as R5 does, so frequency-based results vary from
    /// call to call (on WebAssembly, which has no clock, from a counter:
    /// they vary from call to call but repeat from one run of the program
    /// to the next).
    pub seed: Option<i32>,
    /// Worker threads; 0 means all cores, and more than the cores
    /// available run as many as there are cores.
    pub n_threads: usize,
    /// Stops the computation when cancelled, from any thread, through a
    /// clone of it (see [`CancelToken`]); the default is a fresh token,
    /// which nothing cancels unless a clone of it is kept.
    pub cancel: CancelToken,
    /// r5r's `fare_structure` (default `None`): with one, public transport
    /// is routed as r5r routes it then, with R5's multi-criteria router
    /// (McRAPTOR) over `time_window x draws_per_minute` sampled departure
    /// times instead of RAPTOR at every minute, keeping per stop the
    /// fastest trip within `max_fare`. The structure must cover every
    /// route of the network ([`FareStructure::validate_for`]).
    pub fare_structure: Option<Arc<FareStructure>>,
    /// r5r's `max_fare` (default infinity: no limit), in the currency of
    /// the fare structure, read as a Java `float`; a finite value needs a
    /// `fare_structure`.
    pub max_fare: f64,
}

impl TransitParams {
    /// Check the parameters before routing. The bounds are those of R5 and
    /// r5r 2.5.0 (percentiles: `AnalysisWorkerTask.validatePercentiles`;
    /// counts: r5r's `checkmate::assert_count(positive = TRUE)` and R5's
    /// `int` fields; walk speed: at least 1 mm/s, as
    /// `TravelTimeComputer` requires), plus what keeps the arithmetic in
    /// R5's `int` seconds: minutes times 60 must fit, and so must the
    /// departure plus the window plus `max_trip_duration`, the latest clock
    /// time a search can hold. A departure is at most 7 days after
    /// midnight of the service day. Every public transport function calls
    /// it first; [`RouteError::Invalid`] says what is wrong.
    pub fn validate(&self) -> Result<(), RouteError> {
        self.check().map_err(RouteError::Invalid)
    }

    /// [`TransitParams::validate`] with the message alone.
    pub(crate) fn check(&self) -> Result<(), String> {
        check_date(self.date)?;
        check_percentiles(&self.percentiles)?;
        let check_in = |name: &str, v: i64, lo: i64, hi: i64, unit: &str| {
            if (lo..=hi).contains(&v) {
                Ok(())
            } else {
                Err(format!("{name} must be from {lo} to {hi}{unit}, got {v}"))
            }
        };
        let check = |name: &str, v: i64, lo: i64, hi: i64| check_in(name, v, lo, hi, "");
        let int_max = i64::from(i32::MAX);
        let max_minutes = int_max / 60;
        check_in(
            "departure_time",
            self.departure_time.into(),
            0,
            7 * 86_400,
            " seconds",
        )?;
        check_in(
            "time_window",
            self.time_window.into(),
            1,
            int_max,
            " minutes",
        )?;
        check("draws_per_minute", self.draws_per_minute.into(), 1, int_max)?;
        // r5r hands R5 `as.integer(time_window * draws_per_minute)`
        let draws = i64::from(self.time_window) * i64::from(self.draws_per_minute);
        if draws > int_max {
            return Err(format!(
                "time_window x draws_per_minute ({} x {} = {draws} Monte Carlo draws) must be \
                 at most {int_max}",
                self.time_window, self.draws_per_minute
            ));
        }
        // max_trip_duration first: r5r passes max_walk_time = Inf as
        // max_trip_duration, so a value too large is reported under the
        // name the user set
        let mins = " minutes";
        check_in(
            "max_trip_duration",
            self.max_trip_duration.into(),
            1,
            max_minutes,
            mins,
        )?;
        check_in(
            "max_walk_time",
            self.max_walk_time.into(),
            1,
            max_minutes,
            mins,
        )?;
        check_in(
            "max_bike_time",
            self.max_bike_time.into(),
            1,
            max_minutes,
            mins,
        )?;
        check_in(
            "max_car_time",
            self.max_car_time.into(),
            1,
            max_minutes,
            mins,
        )?;
        let last_clock = i64::from(self.departure_time)
            + 60 * (i64::from(self.time_window) + i64::from(self.max_trip_duration));
        if last_clock > int_max {
            return Err(format!(
                "time_window + max_trip_duration ({} + {} min) after a departure at {} s must \
                 end within {int_max} s (about 68 years); reduce time_window or \
                 max_trip_duration",
                self.time_window, self.max_trip_duration, self.departure_time
            ));
        }
        check(
            "max_rides",
            i64::try_from(self.max_rides).unwrap_or(i64::MAX),
            1,
            int_max,
        )?;
        check_speed("walk_speed_kmh", self.walk_speed_kmh)?;
        check_speed("bike_speed_kmh", self.bike_speed_kmh)?;
        // r5r `set_max_lts`: 1 to 4 (R5 treats anything else as 4)
        if !(1..=4).contains(&self.max_lts) {
            return Err(format!(
                "max_lts must be from 1 to 4 (the highest level of traffic stress), got {}",
                self.max_lts
            ));
        }
        if self.transit_modes.is_empty() {
            return Err("transit_modes must hold at least one public transport mode".into());
        }
        check_max_fare(self.max_fare, self.fare_structure.is_some())
    }
}

/// r5r's `set_max_fare()`: a number >= 0, finite only with a fare
/// structure.
pub(crate) fn check_max_fare(max_fare: f64, fares: bool) -> Result<(), String> {
    if max_fare.is_nan() || max_fare < 0.0 {
        return Err(format!("max_fare must be a number >= 0, got {max_fare}"));
    }
    if max_fare.is_finite() && !fares {
        return Err(format!(
            "max_fare requires a fare_structure: max_fare is {max_fare} but there is no \
             fare_structure, so no fares are calculated and the limit would be ignored"
        ));
    }
    Ok(())
}

/// r5r's `request.maxFare`: `Math.round(maxFare * 100.0f)` of the float
/// r5r sets, `Integer.MAX_VALUE` for an infinite `max_fare`.
pub(crate) fn max_fare_cents(max_fare: f64) -> i32 {
    let f = if max_fare.is_finite() {
        max_fare as f32
    } else {
        i32::MAX as f32
    };
    (f64::from(f * 100.0f32) + 0.5).floor() as i32
}

/// What the fare path of a query needs: the calculator, the patterns
/// McRAPTOR may ride and the services running on the date.
pub(crate) struct FareSetup {
    pub(crate) calc: FareCalculator,
    pub(crate) allowed: Vec<bool>,
    pub(crate) active: Vec<bool>,
}

impl FareSetup {
    pub(crate) fn new(
        fs: &FareStructure,
        layer: &TransitLayer,
        date: u32,
        modes: &[TransitMode],
    ) -> Result<Self, RouteError> {
        let calc = FareCalculator::new(fs, layer)?;
        let active = layer.active_services(date, weekday(date));
        let allowed =
            mcraptor::allowed_patterns(layer, &active, modes).map_err(RouteError::Invalid)?;
        Ok(FareSetup {
            calc,
            allowed,
            active,
        })
    }
}

impl Default for TransitParams {
    fn default() -> Self {
        TransitParams {
            date: 0,
            departure_time: 0,
            time_window: 10,
            percentiles: vec![50],
            max_walk_time: 120,
            max_trip_duration: 120,
            walk_speed_kmh: 3.6,
            access_mode: Mode::Walk,
            egress_mode: Mode::Walk,
            bike_speed_kmh: 12.0,
            max_lts: 2,
            max_bike_time: 120,
            max_car_time: 120,
            max_rides: 3,
            transit_modes: TransitMode::ALL.to_vec(),
            draws_per_minute: 5,
            seed: None,
            n_threads: 0,
            cancel: CancelToken::default(),
            fare_structure: None,
            max_fare: f64::INFINITY,
        }
    }
}

/// `AnalysisWorkerTask.MAX_PERCENTILES`.
pub const MAX_PERCENTILES: usize = 5;

/// Check travel-time percentiles as R5 does
/// (`AnalysisWorkerTask.validatePercentiles`): 1 to 5 of them, each in
/// 1..=99. R5 wants them ascending and accepts equal neighbours, so in any
/// order and with repeats they pass here (the results sort them). Street
/// modes have one travel time, reported for every percentile; they take
/// the same check, so that an empty list is an error there too.
pub fn validate_percentiles(percentiles: &[i32]) -> Result<(), RouteError> {
    check_percentiles(percentiles).map_err(RouteError::Invalid)
}

/// [`validate_percentiles`] with the message alone.
pub(crate) fn check_percentiles(percentiles: &[i32]) -> Result<(), String> {
    if percentiles.is_empty() || percentiles.len() > MAX_PERCENTILES {
        return Err(format!(
            "between 1 and {MAX_PERCENTILES} percentiles are allowed, got {}",
            percentiles.len()
        ));
    }
    if let Some(p) = percentiles.iter().find(|p| !(1..=99).contains(*p)) {
        return Err(format!("percentiles must be in 1..=99, got {p}"));
    }
    Ok(())
}

/// Days in month `m` (1-12) of year `y`, proleptic Gregorian calendar.
fn days_in_month(y: i64, m: i64) -> i64 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Check that `yyyymmdd` is a real calendar date (Gregorian, years 1 to
/// 9999): `20190513` passes, `20190230`, `20191301` and `0` do not. The
/// service calendar is looked up by this date, so a date that does not
/// exist would silently match no service.
pub fn validate_date(date: u32) -> Result<(), RouteError> {
    check_date(date).map_err(RouteError::Invalid)
}

/// [`validate_date`] with the message alone.
pub(crate) fn check_date(date: u32) -> Result<(), String> {
    let (y, m, d) = (
        i64::from(date / 10000),
        i64::from((date / 100) % 100),
        i64::from(date % 100),
    );
    if (1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m) {
        Ok(())
    } else {
        Err(format!(
            "date must be a calendar date written yyyymmdd, got {date}"
        ))
    }
}

/// Days since 1970-01-01 of a valid `yyyymmdd` (Howard Hinnant's
/// `days_from_civil`).
pub(crate) fn days_from_civil(date: u32) -> i64 {
    let (y, m, d) = (
        i64::from(date / 10000),
        i64::from((date / 100) % 100),
        i64::from(date % 100),
    );
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The `yyyymmdd` of a day count since 1970-01-01 (`civil_from_days`), or
/// `None` outside years 1 to 9999.
pub(crate) fn civil_from_days(days: i64) -> Option<u32> {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    if !(1..=9999).contains(&y) {
        return None;
    }
    u32::try_from(y * 10000 + m * 100 + d).ok()
}

/// Day of week of `yyyymmdd`, 0 = Monday.
pub fn weekday(date: u32) -> usize {
    // 1970-01-01 is a Thursday
    ((days_from_civil(date) + 3).rem_euclid(7)) as usize
}

/// Walking tables of a network with public transport, computed once by
/// [`TransitData::new`] and shared by every public transport query on it:
/// the transfers between stops and the streets reached on foot from each
/// stop (for the walk to the destinations).
#[derive(Debug, Clone)]
pub struct TransitData {
    /// Per stop: `(target stop, millimetres)` of its transfers (see
    /// `walk::find_transfers`).
    pub(crate) transfers: Vec<Vec<(u32, i32)>>,
    /// Per stop: `(vertex, millimetres)` reached on foot within 2 km, or
    /// `None` for an unlinked stop.
    pub(crate) trees: Vec<Option<Vec<(u32, i32)>>>,
}

impl TransitData {
    /// Compute the tables of `layer` on `net`, the network and layer
    /// [`build_from_dir`](crate::build_from_dir) returned together. This is
    /// most of the work of building a network with public transport (it
    /// walks from every stop), so do it once per network.
    pub fn new(net: &StreetNetwork, layer: &TransitLayer) -> Self {
        let trees = walk::stop_trees(net, layer);
        let transfers = walk::find_transfers(layer, &trees);
        TransitData { transfers, trees }
    }
}

/// The egress time in seconds of an egress cost, or `None` at or beyond
/// the egress mode's limit (`PerTargetPropagater.propagateTransit`): the
/// cost is millimetres walked or cycled, at `(int)(speed * 1000)` mm/s, or
/// seconds driven; the limit is `max_walk_time`, `max_bike_time` or
/// `max_car_time`.
pub(crate) fn egress_seconds(p: &TransitParams) -> impl Fn(i32) -> Option<i32> + Sync {
    let mms = |kmh: f64| (((kmh * 5.0 / 18.0) as f32) * 1000.0f32) as i32;
    let (speed, limit) = match p.egress_mode {
        Mode::Walk => (mms(p.walk_speed_kmh), p.max_walk_time),
        Mode::Bicycle => (mms(p.bike_speed_kmh), p.max_bike_time),
        Mode::Car => (1, p.max_car_time),
    };
    let limit = limit * 60;
    move |cost: i32| Some(cost / speed).filter(|&e| e < limit)
}

/// Percentile index into `n` sorted values (R5: `ceil(p / 100 * n) - 1`).
pub(crate) fn percentile_index(p: i32, n: usize) -> usize {
    ((f64::from(p) / 100.0 * n as f64).ceil() as i64 - 1).max(0) as usize
}

/// Everything a transit query needs that does not depend on the origin:
/// the request in R5's units, the filtered patterns, the destination
/// linkage and egress tables. Shared by the travel-time matrix and
/// accessibility (R5 `TravelTimeComputer` up to the reducer).
pub(crate) struct Query<'a> {
    net: &'a StreetNetwork,
    layer: &'a TransitLayer,
    data: &'a TransitData,
    p: &'a TransitParams,
    filtered: Filtered,
    /// The access and direct street search.
    access: Access,
    /// Links of the destinations by the access mode, for the direct times
    /// (kept with the network for a grid's cells).
    dest_splits: Links,
    /// Per destination, `(stop, egress walk seconds)` for the stops within
    /// the egress walk limit, in increasing stop order.
    egress: Vec<Vec<(u32, i32)>>,
    /// `max_trip_duration` in seconds.
    max_t: i32,
    /// `(int)(walkSpeed * 1000)`, mm/s.
    walk_mms: i32,
    /// With a fare structure, McRAPTOR's setup.
    fares: Option<FareSetup>,
    /// `request.maxFare`, cents.
    max_fare: i32,
    /// `(double) (walkSpeed * 1000)`, McRAPTOR's walking speed.
    walk_mms_f: f64,
}

/// The street search from an origin of a public transport query (R5
/// `TravelTimeComputer`, part I): by the access mode within its time
/// limit, carried on on foot after a bicycle or car (`keepRoutingOnFoot`).
/// It gives the access times to the stops and the direct times to the
/// destinations.
pub(crate) struct Access {
    /// The graph of the access mode.
    graph: ModeGraph,
    /// The walking graph that carries a bicycle or car search on.
    foot: Option<ModeGraph>,
    /// `min(max_trip_duration, the access mode's own limit)` in seconds
    /// (`ProfileRequest.getMaxTimeSeconds`).
    pub(crate) limit: i32,
}

impl Access {
    pub(crate) fn new(net: &StreetNetwork, p: &TransitParams) -> Self {
        let graph =
            |mode| ModeGraph::with_speeds(net, mode, p.walk_speed_kmh, p.bike_speed_kmh, p.max_lts);
        let mode_limit = match p.access_mode {
            Mode::Walk => p.max_walk_time,
            Mode::Bicycle => p.max_bike_time,
            Mode::Car => p.max_car_time,
        };
        Access {
            graph: graph(p.access_mode),
            foot: (p.access_mode != Mode::Walk).then(|| graph(Mode::Walk)),
            limit: p.max_trip_duration.min(mode_limit) * 60,
        }
    }

    /// The street mode of the access and of the direct trip.
    pub(crate) fn mode(&self) -> Mode {
        self.graph.mode()
    }

    /// Search from `(lat, lon)`: afterwards `access` holds `(stop, seconds)`
    /// of the stops reached, in stop order, and `direct` the direct time to
    /// each destination of `dest_splits` (`UNREACHED` where unreachable).
    /// `false`, with neither filled in, when the origin cannot be linked to
    /// the streets by the access mode.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn search(
        &self,
        net: &StreetNetwork,
        layer: &TransitLayer,
        (lat, lon): (f64, f64),
        dest_splits: &[Option<crate::street::link::Split>],
        split: &mut Vec<u32>,
        search: &mut Option<Search>,
        direct: &mut Vec<i32>,
        access: &mut Vec<(u32, i32)>,
    ) -> bool {
        let Some(origin) = find_split(net, lat, lon, self.mode(), split) else {
            return false;
        };
        let search = search.get_or_insert_with(|| Search::new(net));
        access_search_from(
            net,
            &self.graph,
            self.foot.as_ref(),
            &origin,
            dest_splits,
            self.limit,
            search,
            direct,
        );
        let vertex_time = search.vertex_time();
        access.clear();
        access.extend(
            layer
                .stop_vertex
                .iter()
                .enumerate()
                .filter(|(_, &v)| v >= 0 && vertex_time[v as usize] < self.limit)
                .map(|(s, &v)| (s as u32, vertex_time[v as usize])),
        );
        true
    }
}

/// The destinations of a [`Query`].
#[derive(Clone, Copy)]
enum Dests<'p> {
    Points(&'p [(f64, f64)]),
    /// The cells of a grid, whose egress tables R5 builds from the cells
    /// each stop's envelope overlaps.
    Grid(&'p crate::WebMercatorGrid),
}

/// Per-thread buffers reused across origins.
#[derive(Default)]
pub(crate) struct Scratch {
    split: Vec<u32>,
    times: Vec<i32>,
    search: Option<Search>,
    direct: Vec<i32>,
    access: Vec<(u32, i32)>,
    stop_times: StopTimes,
}

impl<'a> Query<'a> {
    pub(crate) fn new(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        destinations: &[(f64, f64)],
        p: &'a TransitParams,
    ) -> Result<Self, RouteError> {
        Self::build(net, layer, data, Dests::Points(destinations), p)
    }

    /// [`Query::new`] to the cells of a grid, whose egress tables R5 builds
    /// from the cells each stop's envelope overlaps.
    pub(crate) fn new_grid(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        grid: &crate::WebMercatorGrid,
        p: &'a TransitParams,
    ) -> Result<Self, RouteError> {
        Self::build(net, layer, data, Dests::Grid(grid), p)
    }

    fn build(
        net: &'a StreetNetwork,
        layer: &'a TransitLayer,
        data: &'a TransitData,
        destinations: Dests<'_>,
        p: &'a TransitParams,
    ) -> Result<Self, RouteError> {
        p.check().map_err(RouteError::Invalid)?;
        // RAPTOR keeps iterations x stops durations per origin; their byte
        // count must be a valid allocation size
        let per_minute = if layer.has_frequencies {
            p.draws_per_minute as u64
        } else {
            1
        };
        let cells = p.time_window as u64 * per_minute * layer.stop_ids.len() as u64;
        if cells > isize::MAX as u64 / 4 {
            return Err(RouteError::Invalid(format!(
                "time_window x draws_per_minute x stops ({cells}) is too large to allocate"
            )));
        }
        let active = layer.active_services(p.date, weekday(p.date));
        let filtered = Filtered::new(layer, &active, &p.transit_modes)?;
        let walk_speed = (p.walk_speed_kmh * 5.0 / 18.0) as f32;
        let link = |mode| -> Result<Links, RouteError> {
            Ok(match destinations {
                Dests::Points(points) => Arc::new(
                    link_points(
                        net,
                        points.len(),
                        |j| points[j],
                        mode,
                        p.n_threads,
                        &p.cancel,
                    )
                    .ok_or(RouteError::Cancelled)?,
                ),
                Dests::Grid(g) => grid_links(net, g, mode, p.n_threads, &p.cancel)?,
            })
        };
        // the destinations by the egress mode (walking) and, for the
        // direct times, by the access mode
        let egress_splits = link(p.egress_mode)?;
        let dest_splits = if p.access_mode == p.egress_mode {
            egress_splits.clone()
        } else {
            link(p.access_mode)?
        };
        let walk_mms = (walk_speed * 1000.0f32) as i32;
        // PerTargetPropagater: egress seconds from the cost (mm at the
        // mode's speed, or seconds by car), stops at or beyond the mode's
        // egress limit skipped.
        let seconds = egress_seconds(p);
        let egress = match (p.egress_mode, destinations) {
            (Mode::Walk, Dests::Points(points)) => walk::egress_tables(
                layer,
                &data.trees,
                net,
                points,
                &egress_splits,
                p.n_threads,
                &p.cancel,
            )
            .ok_or(RouteError::Cancelled)?
            .into_iter()
            .map(|table| {
                table
                    .into_iter()
                    .filter_map(|(stop, mm)| Some((stop, seconds(mm)?)))
                    .collect()
            })
            .collect(),
            // converted as they are found
            (Mode::Walk, Dests::Grid(g)) => walk::egress_tables_grid(
                layer,
                &data.trees,
                net,
                g,
                &egress_splits,
                seconds,
                p.n_threads,
                &p.cancel,
            )
            .ok_or(RouteError::Cancelled)?,
            (mode, dests) => {
                let targets = match dests {
                    Dests::Points(points) => Targets::Points(points),
                    Dests::Grid(g) => Targets::Grid(g),
                };
                mode_egress_tables(
                    net,
                    layer,
                    mode,
                    targets,
                    &egress_splits,
                    p.n_threads,
                    &p.cancel,
                )
                .ok_or(RouteError::Cancelled)?
                .into_iter()
                .map(|table| {
                    table
                        .into_iter()
                        .filter_map(|(stop, c)| Some((stop, seconds(c)?)))
                        .collect()
                })
                .collect()
            }
        };
        let fares = match &p.fare_structure {
            Some(fs) => Some(FareSetup::new(fs, layer, p.date, &p.transit_modes)?),
            None => None,
        };
        Ok(Query {
            net,
            layer,
            data,
            p,
            filtered,
            access: Access::new(net, p),
            dest_splits,
            egress,
            max_t: p.max_trip_duration * 60,
            walk_mms,
            fares,
            max_fare: max_fare_cents(p.max_fare),
            walk_mms_f: f64::from(walk_speed * 1000.0f32),
        })
    }

    /// Route from one origin and hand each destination's travel times in
    /// seconds to `per_destination(j, times)`, destinations in order: one
    /// value per RAPTOR iteration, or a single (unvarying) value when no
    /// stop was reached on foot, `UNREACHED` where unreachable. The slice
    /// may be reordered by the callee. Returns `false`, without calling it,
    /// when the origin cannot be linked to the street network (R5 then
    /// records nothing for the origin).
    pub(crate) fn route_origin(
        &self,
        origin: (f64, f64),
        scratch: &mut Scratch,
        per_destination: impl FnMut(usize, &mut [i32]),
    ) -> Result<bool, RouteError> {
        self.route_origin_fare(origin, scratch, self.max_fare, per_destination)
    }

    /// [`Query::route_origin`] with `request.maxFare` set to `max_fare`
    /// cents (r5r's `pareto_frontier()` routes once per fare cutoff).
    /// Errors are McRAPTOR's exceptions.
    pub(crate) fn route_origin_fare(
        &self,
        (lat, lon): (f64, f64),
        scratch: &mut Scratch,
        max_fare: i32,
        mut per_destination: impl FnMut(usize, &mut [i32]),
    ) -> Result<bool, RouteError> {
        let (net, layer, p) = (self.net, self.layer, self.p);
        let Scratch {
            split,
            times: tt,
            search,
            direct,
            access,
            stop_times,
        } = scratch;
        // Street search: direct times and access to stops.
        if !self.access.search(
            net,
            layer,
            (lat, lon),
            &self.dest_splits,
            split,
            search,
            direct,
            access,
        ) {
            return Ok(false);
        }
        let transit = !access.is_empty();
        if let (true, Some(f)) = (transit, &self.fares) {
            // R5TravelTimeComputer with a fare calculator: McRAPTOR and
            // FareDominatingList, times collated per draw
            let req = McRequest {
                from_time: p.departure_time,
                to_time: p.departure_time + p.time_window * 60,
                max_trip_s: self.max_t,
                max_rides: p.max_rides,
                suboptimal_minutes: 0,
                max_fare,
                monte_carlo_draws: p.time_window * p.draws_per_minute,
                walk_mms: self.walk_mms_f,
                seed: p.seed,
                origin: (lat, lon),
            };
            let mut router = McRaptor::new(
                layer,
                &self.data.transfers,
                &f.allowed,
                &f.active,
                &req,
                Dominance::Fare(&f.calc),
                None,
                McRaptor::departure_seed(p.seed, (lat, lon)),
            );
            stop_times.reset(layer.stop_ids.len(), req.monte_carlo_draws.max(0) as usize);
            let mut k = 0;
            router
                .route(access, &p.cancel, |_, out| {
                    if let DrawOut::Times(t) = out {
                        stop_times.set_iteration(k, t);
                        k += 1;
                    }
                })
                .map_err(RouteError::Invalid)?;
        } else if transit {
            let req = RaptorRequest {
                from_time: p.departure_time,
                time_window: p.time_window,
                max_rides: p.max_rides,
                max_duration: self.max_t,
                walk_mms: self.walk_mms,
                max_walk_min: p.max_walk_time,
                monte_carlo_draws: p.time_window * p.draws_per_minute,
                seed: p.seed,
                origin: (lat, lon),
            };
            raptor::route_stop_major(
                layer,
                &self.data.transfers,
                &self.filtered,
                access,
                &req,
                &p.cancel,
                stop_times,
            );
        }
        let max_t = self.max_t;
        for (j, &d) in direct.iter().enumerate() {
            if p.cancel.is_cancelled() {
                // the caller discards the origin
                return Ok(true);
            }
            // PerTargetPropagater.propagate: start from the non-transit time,
            // improve it by vehicle arrival + egress walk. Only the least
            // candidate per iteration matters, so the order of the stops
            // does not.
            tt.clear();
            if !transit {
                tt.push(d);
            } else {
                tt.resize(stop_times.n_iterations(), d);
                // No candidate at or above `bound` can improve any
                // iteration: the smaller of max_trip_duration and the worst
                // time so far.
                let mut bound = max_t.min(d);
                for &(stop, e) in &self.egress[j] {
                    let stop = stop as usize;
                    // every iteration's candidate is at least the stop's
                    // least duration plus the egress walk
                    if stop_times.min(stop).saturating_add(e) >= bound {
                        continue;
                    }
                    let mut worst = 0;
                    for (t, &ts) in tt.iter_mut().zip(stop_times.get(stop)) {
                        // ts + e < max_t also means ts < max_t
                        let c = ts.saturating_add(e);
                        let c = if c < max_t { c } else { UNREACHED };
                        *t = (*t).min(c);
                        worst = worst.max(*t);
                    }
                    bound = max_t.min(worst);
                }
            }
            per_destination(j, tt);
        }
        Ok(true)
    }
}

/// Run `f(origin_index, scratch)` for every origin on `n_threads` threads
/// (0 = all cores; capped at the cores and the origins), results in origin
/// order; `Cancelled` when `cancel` is, and then the workers stop before
/// their next origin.
pub(crate) fn par_origins<T: Send>(
    n_threads: usize,
    n_origins: usize,
    cancel: &CancelToken,
    f: impl Fn(usize, &mut Scratch) -> T + Sync,
) -> Result<Vec<T>, RouteError> {
    let n_threads = worker_threads(n_threads, n_origins);
    let next = AtomicUsize::new(0);
    let chunks: Vec<Vec<(usize, T)>> = crate::sys::scoped(0..n_threads, |_| {
        let mut done = Vec::new();
        let mut scratch = Scratch::default();
        while !cancel.is_cancelled() {
            let i = next.fetch_add(1, Ordering::Relaxed);
            if i >= n_origins {
                break;
            }
            done.push((i, f(i, &mut scratch)));
        }
        done
    });
    cancel.check()?;
    let mut slots: Vec<Option<T>> = (0..n_origins).map(|_| None).collect();
    for (i, r) in chunks.into_iter().flatten() {
        slots[i] = Some(r);
    }
    Ok(slots
        .into_iter()
        .map(|r| r.expect("every origin is routed"))
        .collect())
}

/// A public transport travel-time matrix: for each origin and destination,
/// whole minutes at each requested percentile, `None` where r5r reports NA.
/// A pair is reported (its lowest percentile is `Some`) when that
/// percentile is within `max_trip_duration`.
///
/// The percentiles are in ascending order, whatever the order of
/// [`TransitParams::percentiles`]; a repeated percentile, which R5
/// accepts, gets one value per occurrence (see [`validate_percentiles`]).
/// The values are held in one buffer, origin by origin, destination by
/// destination, percentile by percentile, which
/// [`into_vec`](TransitMatrix::into_vec) hands over without copying.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitMatrix {
    n_origins: usize,
    n_destinations: usize,
    percentiles: Vec<i32>,
    /// `minutes[(origin * n_destinations + destination) * n_percentiles + k]`.
    minutes: Vec<Option<u32>>,
}

impl TransitMatrix {
    /// Number of origins.
    pub fn n_origins(&self) -> usize {
        self.n_origins
    }

    /// Number of destinations.
    pub fn n_destinations(&self) -> usize {
        self.n_destinations
    }

    /// The percentiles, ascending: the order of the values of each pair.
    pub fn percentiles(&self) -> &[i32] {
        &self.percentiles
    }

    /// Minutes from origin `origin` to destination `destination` at each
    /// percentile, in the order of [`percentiles`](TransitMatrix::percentiles).
    /// Panics when either index is out of range.
    pub fn get(&self, origin: usize, destination: usize) -> &[Option<u32>] {
        assert!(
            origin < self.n_origins && destination < self.n_destinations,
            "({origin}, {destination}) is outside a {} x {} matrix",
            self.n_origins,
            self.n_destinations
        );
        let np = self.percentiles.len();
        let k = (origin * self.n_destinations + destination) * np;
        &self.minutes[k..k + np]
    }

    /// The values from origin `origin`: destination by destination, one
    /// value per percentile each (`n_destinations * percentiles().len()`
    /// values). Panics when `origin` is out of range.
    pub fn row(&self, origin: usize) -> &[Option<u32>] {
        assert!(origin < self.n_origins, "origin {origin} out of range");
        let per = self.n_destinations * self.percentiles.len();
        &self.minutes[origin * per..(origin + 1) * per]
    }

    /// The rows in origin order, as [`row`](TransitMatrix::row) gives them.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[Option<u32>]> + '_ {
        (0..self.n_origins).map(move |i| self.row(i))
    }

    /// The whole matrix as one slice, in the order of
    /// [`into_vec`](TransitMatrix::into_vec).
    pub fn as_slice(&self) -> &[Option<u32>] {
        &self.minutes
    }

    /// The whole matrix as one vector, without copying: the value for
    /// `(origin, destination)` at the `k`-th percentile is at
    /// `(origin * n_destinations + destination) * percentiles().len() + k`.
    pub fn into_vec(self) -> Vec<Option<u32>> {
        self.minutes
    }
}

/// Public transport travel-time matrix, as r5r's `travel_time_matrix()` with
/// public transport in `mode` (by default `c("WALK", "TRANSIT")`; see
/// [`TransitParams::transit_modes`], [`TransitParams::access_mode`] and
/// [`TransitParams::egress_mode`]). Points are `(lat, lon)` degrees.
/// [`RouteError::Invalid`] when the parameters are invalid (see
/// [`TransitParams::validate`]), [`RouteError::Cancelled`] when `p.cancel`
/// is cancelled before every origin is done.
pub fn transit_travel_time_matrix(
    net: &StreetNetwork,
    layer: &TransitLayer,
    data: &TransitData,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TransitParams,
) -> Result<TransitMatrix, RouteError> {
    let q = Query::new(net, layer, data, destinations, p)?;
    let m = p.max_trip_duration;
    let mut percentiles = p.percentiles.clone();
    percentiles.sort_unstable();
    let nd = destinations.len();
    let np = percentiles.len();
    let rows = par_origins(p.n_threads, origins.len(), &p.cancel, |i, scratch| {
        let mut row = vec![None; nd * np];
        q.route_origin(origins[i], scratch, |j, tt| {
            // TravelTimeReducer.extractTravelTimePercentilesAndRecord
            tt.sort_unstable();
            let n = tt.len();
            let vals: Vec<i32> = percentiles
                .iter()
                .map(|&pc| {
                    let v = tt[percentile_index(pc, n).min(n - 1)];
                    if v == UNREACHED {
                        UNREACHED
                    } else {
                        v / 60
                    }
                })
                .collect();
            if vals[0] <= m {
                for (c, &v) in row[j * np..(j + 1) * np].iter_mut().zip(&vals) {
                    if v <= m {
                        *c = Some(v as u32);
                    }
                }
            }
        })?;
        Ok::<_, RouteError>(row)
    })?;
    let mut minutes = Vec::with_capacity(origins.len() * nd * np);
    for row in rows {
        let row: Vec<Option<u32>> = row?;
        minutes.extend_from_slice(&row);
    }
    Ok(TransitMatrix {
        n_origins: origins.len(),
        n_destinations: nd,
        percentiles,
        minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekdays() {
        assert_eq!(weekday(20190513), 0); // Monday
        assert_eq!(weekday(19700101), 3); // Thursday
        assert_eq!(weekday(20240229), 3); // Thursday
        assert_eq!(weekday(20261109), 0); // Monday
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        let ok = TransitParams {
            date: 20190513,
            ..TransitParams::default()
        };
        assert!(ok.check().is_ok());
        for bad in [
            TransitParams {
                percentiles: vec![],
                ..ok.clone()
            },
            TransitParams {
                percentiles: vec![100],
                ..ok.clone()
            },
            TransitParams {
                time_window: 0,
                ..ok.clone()
            },
            TransitParams {
                draws_per_minute: 0,
                ..ok.clone()
            },
            TransitParams {
                time_window: 1 << 16,
                draws_per_minute: 1 << 16,
                ..ok.clone()
            },
            TransitParams {
                max_trip_duration: i32::MAX / 60 + 1,
                max_walk_time: i32::MAX / 60 + 1,
                ..ok.clone()
            },
            TransitParams {
                time_window: i32::MAX / 100,
                max_trip_duration: i32::MAX / 100,
                ..ok.clone()
            },
            TransitParams {
                max_rides: i32::MAX as usize + 1,
                ..ok.clone()
            },
            TransitParams {
                walk_speed_kmh: 0.0,
                ..ok.clone()
            },
            TransitParams {
                max_trip_duration: 0,
                ..ok.clone()
            },
            TransitParams {
                max_rides: 0,
                ..ok.clone()
            },
            TransitParams {
                walk_speed_kmh: f64::NAN,
                ..ok.clone()
            },
            TransitParams {
                date: 0,
                ..ok.clone()
            },
            TransitParams {
                date: 20190230,
                ..ok.clone()
            },
            TransitParams {
                transit_modes: vec![],
                ..ok.clone()
            },
            TransitParams {
                max_lts: 0,
                ..ok.clone()
            },
            TransitParams {
                max_lts: 5,
                ..ok.clone()
            },
            TransitParams {
                bike_speed_kmh: 0.0,
                ..ok.clone()
            },
            TransitParams {
                max_bike_time: 0,
                ..ok.clone()
            },
            TransitParams {
                max_car_time: i32::MAX / 60 + 1,
                ..ok.clone()
            },
        ] {
            assert!(bad.check().is_err(), "{bad:?}");
        }
    }

    /// Values r5r 2.5.0 accepts that earlier caps rejected, and the
    /// messages of what remains bounded.
    #[test]
    fn bounds_are_r5s_and_the_int_clock() {
        let ok = TransitParams {
            date: 20190513,
            departure_time: 14 * 3600,
            ..TransitParams::default()
        };
        for fine in [
            TransitParams {
                time_window: 2000,
                ..ok.clone()
            },
            TransitParams {
                time_window: 1000,
                draws_per_minute: 1000,
                ..ok.clone()
            },
            TransitParams {
                max_rides: 1000,
                ..ok.clone()
            },
            TransitParams {
                max_trip_duration: 5_000_000,
                max_walk_time: 5_000_000,
                ..ok.clone()
            },
            TransitParams {
                walk_speed_kmh: 5000.0,
                ..ok.clone()
            },
        ] {
            assert_eq!(fine.check(), Ok(()), "{fine:?}");
        }
        let err = |p: TransitParams| p.check().unwrap_err();
        // max_walk_time = Inf in r5r is max_trip_duration: the message
        // names max_trip_duration
        let huge = err(TransitParams {
            max_trip_duration: i32::MAX,
            max_walk_time: i32::MAX,
            ..ok.clone()
        });
        assert!(
            huge.starts_with("max_trip_duration must be from 1 to 35791394"),
            "{huge}"
        );
        let clock = err(TransitParams {
            time_window: 30_000_000,
            max_trip_duration: 10_000_000,
            ..ok.clone()
        });
        assert!(
            clock.starts_with("time_window + max_trip_duration"),
            "{clock}"
        );
        let speed = err(TransitParams {
            walk_speed_kmh: 1e-300,
            ..ok.clone()
        });
        assert_eq!(
            speed,
            "walk_speed_kmh must be at least 0.0036 km/h (1 mm/s), got 1e-300"
        );
    }

    #[test]
    fn percentiles_follow_r5() {
        assert!(validate_percentiles(&[50]).is_ok());
        assert!(validate_percentiles(&[75, 25, 50]).is_ok());
        // R5 checks `percentiles[p] >= percentiles[p - 1]`: repeats pass
        assert!(validate_percentiles(&[50, 50]).is_ok());
        assert_eq!(
            check_percentiles(&[]).unwrap_err(),
            "between 1 and 5 percentiles are allowed, got 0"
        );
        assert!(validate_percentiles(&[1, 2, 3, 4, 5, 6]).is_err());
        assert!(validate_percentiles(&[0]).is_err());
        assert!(validate_percentiles(&[100]).is_err());
    }

    #[test]
    fn dates_are_calendar_dates() {
        for ok in [20190513, 20200229, 20000229, 19700101, 10101, 99991231] {
            assert!(validate_date(ok).is_ok(), "{ok}");
        }
        for bad in [
            0, 20190230, 20190229, 19000229, 20191301, 20190500, 20190001, 101,
        ] {
            assert!(validate_date(bad).is_err(), "{bad}");
        }
        assert_eq!(
            check_date(20190230).unwrap_err(),
            "date must be a calendar date written yyyymmdd, got 20190230"
        );
    }

    #[test]
    fn day_counts_round_trip() {
        assert_eq!(days_from_civil(19700101), 0);
        assert_eq!(civil_from_days(-1), Some(19691231));
        for date in [10101, 20000229, 20190513, 20191231, 99991231] {
            assert_eq!(civil_from_days(days_from_civil(date)), Some(date));
        }
        assert_eq!(civil_from_days(days_from_civil(10101) - 1), None);
        assert_eq!(civil_from_days(days_from_civil(99991231) + 1), None);
    }

    #[test]
    fn percentile_indices_match_r5() {
        assert_eq!(percentile_index(50, 10), 4);
        assert_eq!(percentile_index(50, 50), 24);
        assert_eq!(percentile_index(5, 10), 0);
        assert_eq!(percentile_index(95, 10), 9);
    }
}
