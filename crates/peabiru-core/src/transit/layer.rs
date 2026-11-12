//! The transit layer: stops, trip patterns and schedules loaded from GTFS
//! the way R5 7.6 (`TransitLayer.loadFromGtfs` over conveyal gtfs-lib) does,
//! and the linking of stops to the street network
//! (`StreetLayer.associateStops`).

use crate::gtfs::feed::{Feed, StopTime};
use crate::gtfs::zip::{Limits, ZipError};
use crate::gtfs::GtfsError;
use crate::street::geom::{haversine_m, java_int, FIXED_FACTOR};
use crate::street::{parkride, Mode, StreetNetwork};
use crate::transit::matrix::weekday;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

/// The path and the bytes of a GTFS zip file on disk, for
/// [`TransitLayer::from_feeds`]; an error naming the file when it cannot
/// be read.
pub(crate) fn read_feed_file(
    path: &std::path::Path,
) -> Result<(std::path::PathBuf, Cow<'static, [u8]>), GtfsError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok((path.to_path_buf(), Cow::Owned(bytes))),
        Err(e) => Err(GtfsError::in_feed(path, GtfsError::Zip(ZipError::Io(e)))),
    }
}

/// One frequency entry of a trip: departures from the first stop every
/// `headway` seconds between `start` and `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrequencyEntry {
    pub start: i32,
    pub end: i32,
    pub headway: i32,
    /// GTFS `exact_times = 1`: the vehicles leave at `start`, `start +
    /// headway`, ... exactly. R5 ignores it, and so do all the functions
    /// but `trip_options()`, which reads it.
    pub exact: bool,
}

/// A trip's times at every stop of its pattern. Frequency trips hold
/// template times relative to their first arrival.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TripSchedule {
    pub trip_id: String,
    pub service: u32,
    pub arrivals: Vec<i32>,
    pub departures: Vec<i32>,
    /// Empty for a scheduled trip.
    pub frequencies: Vec<FrequencyEntry>,
}

impl TripSchedule {
    pub fn is_frequency(&self) -> bool {
        !self.frequencies.is_empty()
    }
}

/// Trips sharing route, stop sequence and pickup/drop-off types.
#[derive(Debug, Clone, Default)]
pub struct Pattern {
    /// GTFS `route_id`, unscoped (R5 `RouteInfo.route_id`, what the results
    /// report in `routes`).
    pub route_id: String,
    pub route_type: i32,
    pub stops: Vec<u32>,
    /// Boarding / alighting allowed at each stop (only GTFS type 1 forbids).
    pub pickups: Vec<bool>,
    pub dropoffs: Vec<bool>,
    /// In R5 order: a stable sort by first departure.
    pub trips: Vec<TripSchedule>,
    /// Service codes of the trips, sorted.
    pub services: Vec<u32>,
    pub has_frequencies: bool,
    pub has_schedules: bool,
}

/// When a service runs (R5 / gtfs-lib `Service.activeOn`).
#[derive(Debug, Clone, Default)]
pub struct Service {
    /// Monday-first weekdays, start and end `yyyymmdd` (as numbers, not
    /// checked to be dates).
    pub calendar: Option<([bool; 7], i32, i32)>,
    /// `yyyymmdd` -> whether the service runs (exception type 1); any
    /// other type removes the day.
    pub exceptions: HashMap<u32, bool>,
}

impl Service {
    /// `date` is `yyyymmdd`, `weekday` 0 = Monday.
    pub fn active_on(&self, date: u32, weekday: usize) -> bool {
        if let Some(&runs) = self.exceptions.get(&date) {
            return runs;
        }
        match self.calendar {
            Some((days, start, end)) => {
                let d = i64::from(date);
                i64::from(start) <= d && d <= i64::from(end) && days[weekday]
            }
            None => false,
        }
    }
}

/// Whether [`TransitLayer::from_feeds`] checks the feeds for the errors
/// of priority `HIGH`, for which the build is refused, and how.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FeedCheck {
    /// No check: load what can be read.
    None,
    /// Check each feed, then load it.
    Sequential,
    /// Check and load each feed at the same time, on two threads.
    Parallel,
}

/// The public transport of a network: the stops, trip patterns and
/// timetables of its GTFS feeds, loaded as R5 7.6 loads them, with each stop
/// linked to the street network it was built with. Made by
/// [`build_from_dir`](crate::build_from_dir), together with that street
/// network; read-only once built.
#[derive(Debug, Clone, Default)]
pub struct TransitLayer {
    pub(crate) feed_ids: Vec<String>,
    /// `feed_id:stop_id`, in stop index order.
    pub(crate) stop_ids: Vec<String>,
    pub(crate) stop_lat: Vec<f64>,
    pub(crate) stop_lon: Vec<f64>,
    /// Street vertex created for each stop, or -1 when unlinked.
    pub(crate) stop_vertex: Vec<i32>,
    pub(crate) services: Vec<Service>,
    pub(crate) patterns: Vec<Pattern>,
    pub(crate) patterns_for_stop: Vec<Vec<u32>>,
    pub(crate) has_frequencies: bool,
    pub(crate) has_schedules: bool,
    /// Trips of all feeds left out of the timetable, by reason.
    pub(crate) dropped_trips: DroppedTrips,
    /// What was found in each feed, in loading order (see
    /// [`TransitLayer::feed_warnings`]).
    pub(crate) feed_reports: Vec<FeedReport>,
    /// Drawn, not routed on (`transit_network_to_sf()`; see
    /// `crate::inspect`): the `stop_name` of each stop (`None` when blank),
    /// R5's `RouteInfo` list, and per pattern its index in it and the
    /// shape of the trip that made the pattern (into `shapes`).
    pub(crate) stop_names: Vec<Option<String>>,
    /// `service_id` of each service.
    pub(crate) service_ids: Vec<String>,
    pub(crate) routes: Vec<RouteInfo>,
    pub(crate) pattern_route: Vec<u32>,
    pub(crate) pattern_shape: Vec<Option<u32>>,
    /// Per pattern with a shape whose points and whose first trip's stop
    /// times all have `shape_dist_traveled`: R5's location of each stop
    /// along the shape found from those distances (`(segment, fraction as
    /// float)`, `TransitLayer.loadFromGtfs`). `None` when R5 projects the
    /// stops on the shape instead.
    pub(crate) pattern_shape_cuts: Vec<Option<ShapeCuts>>,
    /// Shapes of `shapes.txt` as `[lon, lat]` points, in sequence order.
    pub(crate) shapes: Vec<Vec<[f64; 2]>>,
}

/// R5's `RouteInfo`: a GTFS route as it is reported, `None` for a blank
/// value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RouteInfo {
    pub agency_id: Option<String>,
    pub agency_name: Option<String>,
    pub route_id: String,
    pub short_name: Option<String>,
    pub long_name: Option<String>,
    pub route_type: i32,
}

/// Trips left out of the timetable while loading, by reason. R5 leaves out
/// the same trips (most of them without a word).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DroppedTrips {
    /// No row in `stop_times.txt`.
    pub no_stop_times: usize,
    /// A `stop_times.txt` row names a stop missing from `stops.txt`.
    pub unknown_stop: usize,
    /// The `route_id` is missing from `routes.txt`.
    pub unknown_route: usize,
    /// The `service_id` is in neither `calendar.txt` nor `calendar_dates.txt`.
    pub unknown_service: usize,
    /// No time at the first or the last stop.
    pub missing_end_time: usize,
    /// A time earlier than the one before it (a negative hop or dwell).
    pub negative_time: usize,
    /// Frequency-based, with no valid `frequencies.txt` entry.
    pub invalid_frequencies: usize,
}

impl DroppedTrips {
    /// All trips left out.
    pub fn total(&self) -> usize {
        self.no_stop_times
            + self.unknown_stop
            + self.unknown_route
            + self.unknown_service
            + self.missing_end_time
            + self.negative_time
            + self.invalid_frequencies
    }

    fn add(&mut self, o: &DroppedTrips) {
        self.no_stop_times += o.no_stop_times;
        self.unknown_stop += o.unknown_stop;
        self.unknown_route += o.unknown_route;
        self.unknown_service += o.unknown_service;
        self.missing_end_time += o.missing_end_time;
        self.negative_time += o.negative_time;
        self.invalid_frequencies += o.invalid_frequencies;
    }
}

/// What loading one GTFS feed found. Some of it is a GTFS error of
/// priority `HIGH` (as gtfs-lib ranks it), for which the build is refused
/// unless such errors are ignored: see [`FeedReport::high_priority_cases`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FeedReport {
    /// The feed's `.zip` file.
    pub path: std::path::PathBuf,
    /// Entries of the tables read from a subfolder of the `.zip` rather than
    /// its root (gtfs-lib's `TableInSubdirectoryError`).
    pub tables_in_subfolder: Vec<String>,
    /// The feed has no `agency.txt` (gtfs-lib's `NoAgencyInFeedError`).
    pub no_agency: bool,
    /// Trips of this feed left out of the timetable, by reason. Only
    /// `unknown_stop` and `unknown_route` are errors of priority `HIGH`
    /// (`ReferentialIntegrityError`).
    pub dropped_trips: DroppedTrips,
}

impl FeedReport {
    /// The file name of the feed, as the warnings name it.
    pub fn file_name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// The errors of priority `HIGH` found in this feed, one phrase per
    /// case (empty when there are none): tables in a subfolder, no `agency.txt`, trips that refer
    /// to a stop or route that does not exist.
    pub fn high_priority_cases(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.tables_in_subfolder.is_empty() {
            let mut folders: Vec<&str> = Vec::new();
            for t in &self.tables_in_subfolder {
                let f = t.rsplit_once('/').map_or("", |(f, _)| f);
                if !folders.contains(&f) {
                    folders.push(f);
                }
            }
            let folders: Vec<String> = folders.iter().map(|f| format!("'{f}/'")).collect();
            out.push(format!(
                "its tables are in a subfolder of the .zip ({})",
                folders.join(", ")
            ));
        }
        if self.no_agency {
            out.push("it has no agency.txt".to_string());
        }
        let (stop, route) = (
            self.dropped_trips.unknown_stop,
            self.dropped_trips.unknown_route,
        );
        let trips = |n: usize| format!("{n} trip{} left out", if n == 1 { "" } else { "s" });
        let refer = |n: usize| if n == 1 { "it refers" } else { "they refer" };
        match (stop, route) {
            (0, 0) => {}
            (n, 0) => out.push(format!(
                "{} because {} to a stop that does not exist",
                trips(n),
                refer(n)
            )),
            (0, n) => out.push(format!(
                "{} because {} to a route that does not exist",
                trips(n),
                refer(n)
            )),
            (s, r) => out.push(format!(
                "{} ({s} refer{} to a stop that does not exist, {r} to a route that does not exist)",
                trips(s + r),
                if s == 1 { "s" } else { "" }
            )),
        }
        out
    }

    /// One warning for this feed naming every case of
    /// [`FeedReport::high_priority_cases`], or `None` when there are none.
    pub fn warning(&self) -> Option<String> {
        let found = self.high_priority_cases();
        (!found.is_empty()).then(|| feed_warning(&self.file_name(), &found.join("; ")))
    }

    /// One warning per case of [`FeedReport::high_priority_cases`].
    pub fn warnings_per_case(&self) -> Vec<String> {
        let file = self.file_name();
        self.high_priority_cases()
            .iter()
            .map(|c| feed_warning(&file, c))
            .collect()
    }
}

fn feed_warning(file: &str, found: &str) -> String {
    format!("GTFS feed '{file}': {found}. A network is built from this feed only because GTFS errors are ignored.")
}

/// WGS84 ellipsoidal distance in metres (Vincenty's inverse formula; R5 uses
/// GeoTools' `GeodeticCalculator`, which agrees far below a millimetre).
pub fn ellipsoidal_distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let a = 6_378_137.0f64;
    let f = 1.0 / 298.257_223_563;
    let b = a * (1.0 - f);
    let l = (lon2 - lon1).to_radians();
    let u1 = ((1.0 - f) * lat1.to_radians().tan()).atan();
    let u2 = ((1.0 - f) * lat2.to_radians().tan()).atan();
    let (sin_u1, cos_u1) = u1.sin_cos();
    let (sin_u2, cos_u2) = u2.sin_cos();
    let mut lambda = l;
    for _ in 0..200 {
        let (sin_l, cos_l) = lambda.sin_cos();
        let sin_sigma =
            ((cos_u2 * sin_l).powi(2) + (cos_u1 * sin_u2 - sin_u1 * cos_u2 * cos_l).powi(2)).sqrt();
        if sin_sigma == 0.0 {
            return 0.0;
        }
        let cos_sigma = sin_u1 * sin_u2 + cos_u1 * cos_u2 * cos_l;
        let sigma = sin_sigma.atan2(cos_sigma);
        let sin_alpha = cos_u1 * cos_u2 * sin_l / sin_sigma;
        let cos2_alpha = 1.0 - sin_alpha * sin_alpha;
        let cos_2sm = if cos2_alpha != 0.0 {
            cos_sigma - 2.0 * sin_u1 * sin_u2 / cos2_alpha
        } else {
            0.0
        };
        let c = f / 16.0 * cos2_alpha * (4.0 + f * (4.0 - 3.0 * cos2_alpha));
        let prev = lambda;
        lambda = l
            + (1.0 - c)
                * f
                * sin_alpha
                * (sigma
                    + c * sin_sigma * (cos_2sm + c * cos_sigma * (-1.0 + 2.0 * cos_2sm * cos_2sm)));
        if (lambda - prev).abs() < 1e-13 {
            let u_sq = cos2_alpha * (a * a - b * b) / (b * b);
            let big_a =
                1.0 + u_sq / 16384.0 * (4096.0 + u_sq * (-768.0 + u_sq * (320.0 - 175.0 * u_sq)));
            let big_b = u_sq / 1024.0 * (256.0 + u_sq * (-128.0 + u_sq * (74.0 - 47.0 * u_sq)));
            let delta_sigma = big_b
                * sin_sigma
                * (cos_2sm
                    + big_b / 4.0
                        * (cos_sigma * (-1.0 + 2.0 * cos_2sm * cos_2sm)
                            - big_b / 6.0
                                * cos_2sm
                                * (-3.0 + 4.0 * sin_sigma * sin_sigma)
                                * (-3.0 + 4.0 * cos_2sm * cos_2sm)));
            return b * big_a * (sigma - delta_sigma);
        }
    }
    // no convergence (nearly antipodal points): fall back to a sphere
    haversine_m(lat1, lon1, lat2, lon2)
}

/// The times of a trip's stops, or `None`.
type Interpolated = Option<(Vec<i32>, Vec<i32>)>;

/// Fill blank times as gtfs-lib `getInterpolatedStopTimesForTrip` does;
/// `None` when the first or last stop has no time. GeoTools'
/// `GeodeticCalculator` measures the hops around blank times and throws on
/// a latitude outside ±90° and on a longitude that is not finite (NaN
/// included; it measures any finite longitude): `Err` with the position of
/// such a stop.
fn interpolate(rows: &[&StopTime], coords: &[(f64, f64)]) -> Result<Interpolated, usize> {
    let n = rows.len();
    let mut arr: Vec<Option<i32>> = Vec::with_capacity(n);
    let mut dep: Vec<Option<i32>> = Vec::with_capacity(n);
    for r in rows {
        let (a, d) = match (r.arrival, r.departure) {
            (Some(a), None) => (Some(a), Some(a)),
            (None, Some(d)) => (Some(d), Some(d)),
            x => x,
        };
        arr.push(a);
        dep.push(d);
    }
    if arr[0].is_none() || arr[n - 1].is_none() {
        return Ok(None);
    }
    let valid = |(lat, lon): (f64, f64)| (-90.0..=90.0).contains(&lat) && lon.is_finite();
    let mut i = 1;
    while i < n {
        if arr[i].is_some() {
            i += 1;
            continue;
        }
        let a = i; // first untimed row
        let mut b = i;
        while arr[b].is_none() {
            b += 1;
        }
        // cumulative lengths from row a-1
        let mut cum = vec![0.0f64; b - a + 1];
        let mut total = 0.0;
        for k in a..=b {
            for j in [k - 1, k] {
                if !valid(coords[j]) {
                    return Err(j);
                }
            }
            let (lat0, lon0) = coords[k - 1];
            let (lat1, lon1) = coords[k];
            total += ellipsoidal_distance_m(lat0, lon0, lat1, lon1);
            cum[k - a] = total;
        }
        let start = dep[a - 1].expect("timed");
        // Java int arithmetic
        let span = f64::from(arr[b].expect("timed").wrapping_sub(start));
        for k in a..b {
            let t = java_int(f64::from(start) + span * (cum[k - a] / total));
            arr[k] = Some(t);
            dep[k] = Some(t);
        }
        i = b;
    }
    Ok(Some((
        arr.into_iter().map(|x| x.expect("filled")).collect(),
        dep.into_iter().map(|x| x.expect("filled")).collect(),
    )))
}

/// gtfs-lib's pattern key: route, stops, pickup types, drop-off types.
type PatternKey = (String, Vec<u32>, Vec<i32>, Vec<i32>);

/// The frequency entries of one trip in the order of gtfs-lib's set of
/// `(trip_id, Frequency)` tuples, which compares entries by
/// `start_time - o.start_time` in Java `int` arithmetic and keeps the first
/// of entries that compare equal. That comparison is a consistent order
/// when the start times differ by less than 2^31, and also with
/// `INT_MISSING` (an unreadable time) among start times that are all
/// positive (it sorts last) or all negative (first); otherwise which order
/// the set ends up in depends on its B-tree and `None` is returned.
fn java_frequency_order(entries: &[FrequencyEntry]) -> Option<Vec<FrequencyEntry>> {
    let missing = crate::gtfs::jparse::INT_MISSING;
    let read: Vec<i64> = entries
        .iter()
        .filter(|e| e.start != missing)
        .map(|e| i64::from(e.start))
        .collect();
    let (lo, hi) = (read.iter().min().copied(), read.iter().max().copied());
    let spread_ok = match (lo, hi) {
        (Some(lo), Some(hi)) => hi - lo < 1 << 31,
        _ => true,
    };
    let has_missing = read.len() < entries.len();
    // INT_MISSING compares above every positive start and below every
    // negative one; against 0 both ways
    let all_negative = hi.is_some_and(|hi| hi < 0);
    if !spread_ok || (has_missing && !all_negative && lo.is_some_and(|lo| lo <= 0)) {
        return None;
    }
    let key = |e: &FrequencyEntry| match (e.start == missing, all_negative) {
        (true, true) => i64::MIN,
        (true, false) => i64::MAX,
        _ => i64::from(e.start),
    };
    let mut out: Vec<FrequencyEntry> = Vec::with_capacity(entries.len());
    for e in entries {
        match out.binary_search_by_key(&key(e), key) {
            Ok(_) => {} // equal: the set keeps the first
            Err(i) => out.insert(i, *e),
        }
    }
    Some(out)
}

/// An error for a feed that R5 fails building from: `why`, and the
/// exception R5 throws.
fn r5_fails(why: String, exception: &str) -> GtfsError {
    GtfsError::R5Fails(format!(
        "{why}, and R5 fails building its transit layer from the feed ({exception})"
    ))
}

/// Feed id: `feed_info.feed_id`, else the file name without `.zip`.
fn feed_id(feed: &Feed, path: &std::path::Path) -> String {
    feed.feed_id.clone().unwrap_or_else(|| {
        let name = path
            .file_name()
            .map_or(String::new(), |n| n.to_string_lossy().into_owned());
        name.strip_suffix(".zip")
            .map_or(name.clone(), str::to_string)
    })
}

/// R5's `new RouteInfo(route, gtfs.agency.get(route.agency_id))`, with the
/// route's agency as gtfs-lib's `Route.Loader` sets it: the `agency_id`
/// given, when `agency.txt` has it, else the only agency of the feed.
fn route_info(route: &crate::gtfs::feed::Route, agencies: &BTreeMap<&str, &str>) -> RouteInfo {
    let given =
        Some(route.agency_id.as_str()).filter(|id| !id.is_empty() && agencies.contains_key(id));
    let agency_id = given
        .or_else(|| (agencies.len() == 1).then(|| *agencies.keys().next().expect("one agency")));
    let some = |s: &str| (!s.is_empty()).then(|| s.to_string());
    RouteInfo {
        agency_id: agency_id.map(str::to_string),
        agency_name: agency_id.and_then(|id| some(agencies[id])),
        route_id: route.route_id.clone(),
        short_name: some(&route.short_name),
        long_name: some(&route.long_name),
        route_type: route.route_type,
    }
}

impl TransitLayer {
    /// The id of each feed, in loading order: the `feed_id` of its
    /// `feed_info.txt`, or else the file name without `.zip` (as R5 names
    /// them). Empty when the network has no public transport.
    pub fn feed_ids(&self) -> &[String] {
        &self.feed_ids
    }

    /// Number of stops, all feeds together.
    pub fn n_stops(&self) -> usize {
        self.stop_ids.len()
    }

    /// Number of stops linked to the street network (a stop too far from
    /// any walkable street is not).
    pub fn n_linked_stops(&self) -> usize {
        self.stop_vertex.iter().filter(|&&v| v >= 0).count()
    }

    /// Number of trip patterns (trips of a route with the same stops), all
    /// feeds together.
    pub fn n_patterns(&self) -> usize {
        self.patterns.len()
    }

    /// Number of services (calendars), all feeds together.
    pub fn n_services(&self) -> usize {
        self.services.len()
    }

    /// Number of services running on `date` (`yyyymmdd`);
    /// [`RouteError::Invalid`](crate::RouteError::Invalid) when `date` is
    /// not a calendar date (see [`validate_date`](crate::validate_date)).
    pub fn n_services_active_on(&self, date: u32) -> Result<usize, crate::RouteError> {
        crate::validate_date(date)?;
        let wd = weekday(date);
        Ok(self
            .services
            .iter()
            .filter(|s| s.active_on(date, wd))
            .count())
    }

    /// Whether any trip runs by frequency (`frequencies.txt`) rather than
    /// by timetable; their departures are drawn at random (see
    /// [`TransitParams::seed`](crate::TransitParams::seed)).
    pub fn has_frequencies(&self) -> bool {
        self.has_frequencies
    }

    /// The first frequency trip (and its headway) with an entry whose
    /// headway is not positive, which R5 keeps and fails routing on.
    pub(crate) fn non_positive_headway(&self) -> Option<(&str, i32)> {
        self.patterns.iter().flat_map(|p| &p.trips).find_map(|t| {
            t.frequencies
                .iter()
                .find(|e| e.headway <= 0)
                .map(|e| (t.trip_id.as_str(), e.headway))
        })
    }

    /// Trips of all feeds left out of the timetable, by reason.
    pub fn dropped_trips(&self) -> DroppedTrips {
        self.dropped_trips
    }

    /// What was found in each feed, in loading order.
    pub fn feed_reports(&self) -> &[FeedReport] {
        &self.feed_reports
    }

    /// Load feeds in the given order (R5 uses the data directory's listing
    /// order; see [`gtfs_files_in_dir`]).
    #[cfg(test)]
    pub(crate) fn load(paths: &[std::path::PathBuf]) -> Result<TransitLayer, GtfsError> {
        Self::load_with_limits(paths, Limits::default())
    }

    /// [`TransitLayer::load`] with other bounds on how far each zip file
    /// may expand.
    #[cfg(test)]
    pub(crate) fn load_with_limits(
        paths: &[std::path::PathBuf],
        limits: Limits,
    ) -> Result<TransitLayer, GtfsError> {
        Self::from_feeds(
            paths.iter().map(|p| read_feed_file(p)),
            limits,
            FeedCheck::None,
        )
    }

    /// Load feeds given as the path (or file name) and the bytes of each
    /// `.zip`, in the order given; reading stops at the first error. The
    /// path names the feed in errors and reports and, without a
    /// `feed_info.feed_id`, gives the feed id.
    ///
    /// With `check`, each feed is first validated as gtfs-lib loads it
    /// (`crate::gtfs::validate`): a feed gtfs-lib fails on is an error at
    /// once; feeds with errors of priority `HIGH` are not loaded, the
    /// following feeds are still validated, and the error names them all
    /// ([`GtfsError::HighPriority`]): no network is built when any feed has
    /// one.
    pub(crate) fn from_feeds<'a>(
        feeds: impl IntoIterator<Item = Result<(std::path::PathBuf, Cow<'a, [u8]>), GtfsError>>,
        limits: Limits,
        check: FeedCheck,
    ) -> Result<TransitLayer, GtfsError> {
        use crate::gtfs::validate::{validate, GtfsErrorPriority, Outcome};
        let high = |path: &std::path::Path, o: Outcome| match o {
            Outcome::Errors(e) => Ok(e
                .iter()
                .filter(|e| e.priority() == GtfsErrorPriority::High)
                .count()),
            Outcome::Fails(why) => Err(GtfsError::in_feed(path, GtfsError::GtfsLibFails(why))),
        };
        let mut layer = TransitLayer::default();
        let mut refused: Vec<(std::path::PathBuf, usize)> = Vec::new();
        for f in feeds {
            let (path, bytes) = f?;
            let loaded = match check {
                FeedCheck::None => Feed::read_zip_checked(&path, bytes, limits),
                _ if !refused.is_empty() => {
                    // already refused: only the errors of the rest count
                    let n = high(&path, validate(&bytes))?;
                    if n > 0 {
                        refused.push((path, n));
                    }
                    continue;
                }
                FeedCheck::Sequential => {
                    let n = high(&path, validate(&bytes))?;
                    if n > 0 {
                        refused.push((path, n));
                        continue;
                    }
                    Feed::read_zip_checked(&path, bytes, limits)
                }
                FeedCheck::Parallel => {
                    // the check and the load read the same bytes on two
                    // threads; the check decides, the load is kept only if
                    // the feed passes
                    enum Job<L> {
                        Check(Outcome),
                        Load(Box<L>),
                    }
                    let (bytes, path_ref) = (&*bytes, path.as_path());
                    let mut both = crate::sys::scoped([false, true], |load| {
                        if load {
                            Job::Load(Box::new(Feed::read_zip_checked(
                                path_ref,
                                Cow::Borrowed(bytes),
                                limits,
                            )))
                        } else {
                            Job::Check(validate(bytes))
                        }
                    });
                    let (Some(Job::Load(loaded)), Some(Job::Check(outcome))) =
                        (both.pop(), both.pop())
                    else {
                        unreachable!("one load and one check")
                    };
                    let loaded = *loaded;
                    let n = high(&path, outcome)?;
                    if n > 0 {
                        refused.push((path, n));
                        continue;
                    }
                    loaded
                }
            };
            let (feed, archive) = loaded.map_err(|e| {
                if check == FeedCheck::None {
                    e
                } else {
                    // gtfs-lib loads it with errors of lower priority only
                    GtfsError::NotRead(Box::new(e))
                }
            })?;
            let id = feed_id(&feed, &path);
            let dropped_trips = layer
                .add_feed(&feed, &id)
                .map_err(|e| GtfsError::in_feed(&path, e))?;
            layer.dropped_trips.add(&dropped_trips);
            layer.feed_ids.push(id);
            layer.feed_reports.push(FeedReport {
                path,
                tables_in_subfolder: archive.tables_in_subfolder,
                no_agency: archive.no_agency,
                dropped_trips,
            });
        }
        if !refused.is_empty() {
            return Err(GtfsError::HighPriority(refused));
        }
        layer.patterns_for_stop = vec![Vec::new(); layer.stop_ids.len()];
        for (pi, p) in layer.patterns.iter().enumerate() {
            for &s in &p.stops {
                let list = &mut layer.patterns_for_stop[s as usize];
                if !list.contains(&(pi as u32)) {
                    list.push(pi as u32);
                }
            }
        }
        Ok(layer)
    }

    /// Warnings for the feeds built in spite of GTFS errors of priority
    /// `HIGH`, one per feed, in loading order (empty when there are
    /// none). Such feeds are built only with
    /// [`BuildOptions::ignore_gtfs_errors`](crate::BuildOptions::ignore_gtfs_errors);
    /// otherwise the build fails. The core never prints them; the CLI does.
    pub fn feed_warnings(&self) -> Vec<String> {
        self.feed_reports
            .iter()
            .filter_map(FeedReport::warning)
            .collect()
    }

    /// Add one feed, as R5's `TransitLayer.loadFromGtfs` does; returns the
    /// trips it left out. Fails where R5 throws while doing so.
    fn add_feed(&mut self, feed: &Feed, feed_id: &str) -> Result<DroppedTrips, GtfsError> {
        // Stops: every row, in stop_id order (gtfs-lib keeps tables in maps).
        let stops: BTreeMap<&str, &crate::gtfs::feed::Stop> =
            feed.stops.iter().map(|s| (s.stop_id.as_str(), s)).collect();
        let stop_offset = self.stop_ids.len();
        let mut stop_index: HashMap<&str, u32> = HashMap::new();
        for (i, (id, s)) in stops.iter().enumerate() {
            stop_index.insert(id, (stop_offset + i) as u32);
            self.stop_ids.push(format!("{feed_id}:{id}"));
            self.stop_lat.push(s.lat);
            self.stop_lon.push(s.lon);
            self.stop_vertex.push(-1);
            self.stop_names
                .push((!s.stop_name.is_empty()).then(|| s.stop_name.clone()));
        }

        // Services: calendar and calendar_dates ids, in service_id order.
        let mut services: BTreeMap<&str, Service> = BTreeMap::new();
        for c in &feed.calendars {
            let s = services.entry(c.service_id.as_str()).or_default();
            if s.calendar.is_none() {
                s.calendar = Some((c.days, c.start_date, c.end_date));
            }
        }
        for d in &feed.calendar_dates {
            let s = services.entry(d.service_id.as_str()).or_default();
            // gtfs-lib keeps a blank date under a null key, which no day
            // matches
            if let Some(date) = d.date {
                s.exceptions.entry(date).or_insert(d.exception_type == 1);
            }
        }
        let service_offset = self.services.len();
        let service_code: HashMap<&str, u32> = services
            .keys()
            .enumerate()
            .map(|(i, k)| (*k, (service_offset + i) as u32))
            .collect();
        self.service_ids
            .extend(services.keys().map(|k| k.to_string()));
        self.services.extend(services.into_values());

        let route_type: HashMap<&str, i32> = feed
            .routes
            .iter()
            .map(|r| (r.route_id.as_str(), r.route_type))
            .collect();
        // For RouteInfo: routes and agencies by id, a repeated id keeping
        // the last row (gtfs-lib's maps); an agency without an id is "NONE".
        let routes_by_id: HashMap<&str, &crate::gtfs::feed::Route> = feed
            .routes
            .iter()
            .map(|r| (r.route_id.as_str(), r))
            .collect();
        let agencies: BTreeMap<&str, &str> = feed
            .agencies
            .iter()
            .map(|(id, name)| {
                (
                    if id.is_empty() { "NONE" } else { id.as_str() },
                    name.as_str(),
                )
            })
            .collect();
        let mut route_index: HashMap<&str, u32> = HashMap::new();
        let mut shape_index: HashMap<&str, Option<u32>> = HashMap::new();

        // stop_times per trip, by stop_sequence; a repeated sequence keeps the last row.
        let mut times: HashMap<&str, BTreeMap<i32, &StopTime>> = HashMap::new();
        for st in &feed.stop_times {
            times
                .entry(st.trip_id.as_str())
                .or_default()
                .insert(st.stop_sequence, st);
        }
        // frequency entries per trip, in file order
        let mut freqs: HashMap<&str, Vec<FrequencyEntry>> = HashMap::new();
        for f in &feed.frequencies {
            freqs
                .entry(f.trip_id.as_str())
                .or_default()
                .push(FrequencyEntry {
                    start: f.start_time,
                    end: f.end_time,
                    headway: f.headway_secs,
                    exact: f.exact_times,
                });
        }

        let trips: BTreeMap<&str, &crate::gtfs::feed::Trip> =
            feed.trips.iter().map(|t| (t.trip_id.as_str(), t)).collect();
        // gtfs-lib pattern key -> R5 pattern index
        let mut pattern_of: HashMap<PatternKey, usize> = HashMap::new();
        let first_pattern = self.patterns.len();
        let mut dropped = DroppedTrips::default();
        for (trip_id, trip) in trips {
            // on-demand (GTFS-Flex) trips are not routed (R5 is never asked
            // for on-demand services)
            if feed.flex_trips.contains(trip_id) {
                continue;
            }
            // gtfs.routes.get(trip.route_id): MapDB refuses a null key
            let Some(route_id) = trip.route_id.as_deref() else {
                return Err(r5_fails(
                    format!("trip '{trip_id}' has no route_id"),
                    "NullPointerException",
                ));
            };
            let rows: Vec<&StopTime> = times
                .get(trip_id)
                .map_or_else(Vec::new, |r| r.values().copied().collect());
            if rows.is_empty() {
                dropped.no_stop_times += 1;
                continue;
            }
            let Some(stop_seq) = rows
                .iter()
                .map(|r| stop_index.get(r.stop_id.as_str()).copied())
                .collect::<Option<Vec<u32>>>()
            else {
                dropped.unknown_stop += 1; // referential integrity error
                continue;
            };
            let Some(&rtype) = route_type.get(route_id) else {
                dropped.unknown_route += 1;
                continue;
            };
            let key = (
                route_id.to_string(),
                stop_seq.clone(),
                rows.iter().map(|r| r.pickup_type).collect::<Vec<_>>(),
                rows.iter().map(|r| r.drop_off_type).collect::<Vec<_>>(),
            );
            let coords: Vec<(f64, f64)> = stop_seq
                .iter()
                .map(|&s| (self.stop_lat[s as usize], self.stop_lon[s as usize]))
                .collect();
            let interpolated = interpolate(&rows, &coords).map_err(|k| {
                r5_fails(
                    format!(
                        "stop '{}' of trip '{trip_id}', next to a stop time without a time, \
                         has no valid coordinates",
                        rows[k].stop_id
                    ),
                    "IllegalArgumentException",
                )
            })?;
            let Some((mut arrivals, mut departures)) = interpolated else {
                dropped.missing_end_time += 1;
                continue;
            };
            // reject the whole trip on any negative hop or dwell
            let mut prev_dep = i32::MIN;
            if (0..arrivals.len()).any(|i| {
                let bad = prev_dep > arrivals[i] || arrivals[i] > departures[i];
                prev_dep = departures[i];
                bad
            }) {
                dropped.negative_time += 1;
                continue;
            }
            // R5 makes the pattern only now, for a trip with valid times,
            // whatever happens to the trip next.
            let pi = match pattern_of.get(&key) {
                Some(&pi) => pi,
                None => {
                    // PickDropType.forGtfsCode: codes above 3 read as 0, a
                    // negative one is outside its array
                    if let Some(c) = key.2.iter().chain(&key.3).find(|&&c| c < 0) {
                        return Err(r5_fails(
                            format!(
                                "trip '{trip_id}' has the pickup or drop-off type {c}, which is \
                                 not a GTFS code"
                            ),
                            "ArrayIndexOutOfBoundsException",
                        ));
                    }
                    let ri = match route_index.get(route_id) {
                        Some(&ri) => ri,
                        None => {
                            let info = route_info(routes_by_id[route_id], &agencies);
                            // gtfs.agency.get(route.agency_id), the agency
                            // being null when the route names none and the
                            // feed has several
                            if info.agency_id.is_none() && agencies.len() > 1 {
                                return Err(r5_fails(
                                    format!(
                                        "route '{route_id}' names none of the {} agencies of \
                                         the feed",
                                        agencies.len()
                                    ),
                                    "NullPointerException",
                                ));
                            }
                            self.routes.push(info);
                            let ri = (self.routes.len() - 1) as u32;
                            route_index.insert(route_id, ri);
                            ri
                        }
                    };
                    let shape = match trip.shape_id.as_str() {
                        "" => None,
                        id => match shape_index.get(id) {
                            Some(&s) => s,
                            None => {
                                // gtfs-lib's getShape: no shape without
                                // points; JTS refuses a line of one point
                                let points = feed.shapes.get(id).filter(|p| !p.is_empty());
                                if points.is_some_and(|p| p.len() == 1) {
                                    return Err(r5_fails(
                                        format!(
                                            "shape '{id}' of trip '{trip_id}' has a single point"
                                        ),
                                        "IllegalArgumentException",
                                    ));
                                }
                                let s = points.map(|points| {
                                    self.shapes.push(
                                        points.values().map(|&(lon, lat)| [lon, lat]).collect(),
                                    );
                                    (self.shapes.len() - 1) as u32
                                });
                                shape_index.insert(id, s);
                                s
                            }
                        },
                    };
                    self.patterns.push(Pattern {
                        route_id: route_id.to_string(),
                        route_type: rtype,
                        stops: stop_seq.clone(),
                        pickups: rows.iter().map(|r| r.pickup_type != 1).collect(),
                        dropoffs: rows.iter().map(|r| r.drop_off_type != 1).collect(),
                        ..Pattern::default()
                    });
                    self.pattern_route.push(ri);
                    let shape_dists = feed
                        .shape_dists
                        .get(trip.shape_id.as_str())
                        .filter(|m| m.values().all(|d| !d.is_nan()));
                    let cuts = match (shape, shape_dists) {
                        (Some(_), Some(m)) if rows.iter().all(|r| !r.shape_dist.is_nan()) => {
                            let dists: Vec<f64> = m.values().copied().collect();
                            Some(cuts_by_distance(&dists, rows.iter().map(|r| r.shape_dist)))
                        }
                        _ => None,
                    };
                    let by_dist = cuts.is_some();
                    // Without distances R5 projects each stop on the shape
                    // (JTS LocationIndexedLine, x scaled by the cosine of
                    // the first point's latitude); a stop that is no
                    // finite distance from any segment, as a stop without
                    // coordinates, leaves JTS without a location (NPE)
                    if let (Some(s), false) = (shape, by_dist) {
                        let points = &self.shapes[s as usize];
                        let nan = |p: &[f64; 2]| p[0].is_nan() || p[1].is_nan();
                        let no_segment = points[0][1].is_nan()
                            || points.windows(2).all(|w| nan(&w[0]) || nan(&w[1]));
                        if no_segment || coords.iter().any(|c| c.0.is_nan() || c.1.is_nan()) {
                            return Err(r5_fails(
                                format!(
                                    "trip '{trip_id}' has a stop or a shape without coordinates"
                                ),
                                "NullPointerException",
                            ));
                        }
                    }
                    self.pattern_shape.push(shape);
                    self.pattern_shape_cuts.push(cuts);
                    pattern_of.insert(key, self.patterns.len() - 1);
                    self.patterns.len() - 1
                }
            };
            // serviceCodeNumber.get(trip.service_id) is -1 for a blank
            // service_id, which fails when the trip is added below
            let service = match trip.service_id.as_deref() {
                None => None,
                Some(id) => match service_code.get(id) {
                    Some(&s) => Some(s),
                    None => {
                        dropped.unknown_service += 1;
                        continue;
                    }
                },
            };
            let mut frequencies = Vec::new();
            if let Some(f) = freqs.get(trip_id) {
                let Some(ordered) = java_frequency_order(f) else {
                    return Err(GtfsError::R5Fails(format!(
                        "the frequency entries of trip '{trip_id}' have start times so far apart \
                         that gtfs-lib's comparison of them overflows, and the order R5 keeps \
                         them in cannot be reproduced"
                    )));
                };
                // TripSchedule.create: the trip is left out when every entry
                // ends before it starts; such entries are left out of the
                // others. A headway that is not positive is kept (it fails
                // routing, as in R5).
                frequencies = ordered.into_iter().filter(|e| e.start <= e.end).collect();
                if frequencies.is_empty() {
                    dropped.invalid_frequencies += 1;
                    continue;
                }
                let base = arrivals[0];
                arrivals.iter_mut().for_each(|t| *t = t.wrapping_sub(base));
                departures
                    .iter_mut()
                    .for_each(|t| *t = t.wrapping_sub(base));
            }
            let Some(service) = service else {
                return Err(r5_fails(
                    format!("trip '{trip_id}' has no service_id"),
                    "IndexOutOfBoundsException",
                ));
            };
            let p = &mut self.patterns[pi];
            if frequencies.is_empty() {
                p.has_schedules = true;
                self.has_schedules = true;
            } else {
                p.has_frequencies = true;
                self.has_frequencies = true;
            }
            if let Err(i) = p.services.binary_search(&service) {
                p.services.insert(i, service);
            }
            p.trips.push(TripSchedule {
                trip_id: trip_id.to_string(),
                service,
                arrivals,
                departures,
                frequencies,
            });
        }
        for p in &mut self.patterns[first_pattern..] {
            // TripSchedule.compareTo is departures[0] - other.departures[0]
            // in int arithmetic: an order only while they differ by less
            // than 2^31
            let first = p.trips.iter().map(|t| i64::from(t.departures[0]));
            if let (Some(lo), Some(hi)) = (first.clone().min(), first.max()) {
                if hi - lo >= 1 << 31 {
                    return Err(GtfsError::R5Fails(format!(
                        "the trips of route '{}' leave at times so far apart that R5's \
                         comparison of them overflows, and the order it sorts them in cannot \
                         be reproduced",
                        p.route_id
                    )));
                }
            }
            p.trips.sort_by_key(|t| t.departures[0]); // stable
        }
        Ok(dropped)
    }

    /// Link every stop to the street network (R5 `associateStops`): a stop
    /// vertex, a split of the nearest walkable street, and a LINK pair whose
    /// length is the haversine distance from the stop to the street vertex.
    ///
    /// It changes `net`, so it drops any contraction hierarchy and grid
    /// linkage kept with it (see the invariant on `StreetNetwork`'s fields); the only caller,
    /// `build_from_dir`, links before the network is handed out.
    pub(crate) fn link_stops(&mut self, net: &mut StreetNetwork) {
        net.forget_kept();
        for s in 0..self.stop_ids.len() {
            let (lat, lon) = (self.stop_lat[s], self.stop_lon[s]);
            let stop_vertex = parkride::add_vertex_fixed(
                net,
                java_int(lat * FIXED_FACTOR),
                java_int(lon * FIXED_FACTOR),
            );
            self.stop_vertex[s] = stop_vertex as i32;
            let Some(street) = parkride::get_or_create_vertex_near(net, lat, lon, Mode::Walk)
            else {
                self.stop_vertex[s] = -1;
                continue;
            };
            let v = street as usize;
            let length_m = haversine_m(
                lat,
                lon,
                f64::from(net.v_lat[v]) / FIXED_FACTOR,
                f64::from(net.v_lon[v]) / FIXED_FACTOR,
            );
            parkride::add_link_pair(net, stop_vertex, street, (length_m * 1000.0) as i32, -1);
        }
        crate::street::build::build_adjacency(net);
    }

    /// Active service codes for a date (`yyyymmdd`, weekday 0 = Monday).
    pub(crate) fn active_services(&self, date: u32, weekday: usize) -> Vec<bool> {
        self.services
            .iter()
            .map(|s| s.active_on(date, weekday))
            .collect()
    }
}

/// Whether a file name ends with `suffix`, case-sensitively (Java's
/// `String.endsWith`).
pub(crate) fn has_suffix(name: &std::ffi::OsStr, suffix: &str) -> bool {
    name.as_encoded_bytes().ends_with(suffix.as_bytes())
}

/// The `.zip` files of a directory (lower-case extension only)
/// in the order the operating system lists them, which is the order R5
/// (Java `File.listFiles`) loads feeds in.
pub fn gtfs_files_in_dir(
    dir: impl AsRef<std::path::Path>,
) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        if has_suffix(&e.file_name(), ".zip") {
            out.push(e.path());
        }
    }
    Ok(out)
}

/// Stop locations along a shape: `(segment, fraction as float)` per stop.
pub(crate) type ShapeCuts = Box<[(u32, f32)]>;

/// R5's stop locations along a shape from `shape_dist_traveled`
/// (`TransitLayer.loadFromGtfs`): for each stop's distance, the first
/// segment whose end is not before it (the last segment for a distance past
/// the end), the fraction `(dist - begin) / (end - begin)` taken as JTS's
/// `LinearLocation(segment, fraction)` normalizes it (clamped to [0, 1], a
/// fraction of 1 the next vertex, NaN kept), then stored as a `float`. The
/// distances are used as given: no unit, no check that they increase.
/// `shape` has at least two points.
pub(crate) fn cuts_by_distance(shape: &[f64], stops: impl Iterator<Item = f64>) -> ShapeCuts {
    stops
        .map(|dist| {
            let mut segment = 0usize;
            while segment + 2 < shape.len() && dist > shape[segment + 1] {
                segment += 1;
            }
            let (begin, end) = (shape[segment], shape[segment + 1]);
            // LinearLocation.normalize() (a NaN stays NaN)
            let mut frac = ((dist - begin) / (end - begin)).clamp(0.0, 1.0);
            if frac == 1.0 {
                frac = 0.0;
                segment += 1;
            }
            (segment as u32, frac as f32)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frequency_entries_as_r5() {
        use crate::gtfs::zip::tests::make_zip;
        let z = make_zip(
            &[
                ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,-30.0,-51.2\nB,-30.01,-51.21\n"),
                ("routes.txt", b"route_id,route_type\nR,3\n"),
                ("trips.txt", b"route_id,service_id,trip_id\nR,S,T1\nR,S,T2\n"),
                (
                    "stop_times.txt",
                    b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,B,2\nT2,08:00:00,08:00:00,A,1\nT2,08:10:00,08:10:00,B,2\n",
                ),
                ("calendar.txt", b"service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n"),
                ("frequencies.txt", b"trip_id,start_time,end_time,headway_secs\nT1,09:00:00,06:00:00,600\nT2,06:00:00,09:00:00,-60\nT2,10:00:00,11:00:00,600\nT2,10:00:00,10:30:00,60\nT2,14:00,15:00:00,300\n"),
            ],
            true,
        );
        let dir = std::env::temp_dir().join(format!("peabiru-headway-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("feed.zip");
        std::fs::write(&path, z).unwrap();
        let layer = TransitLayer::load(&[path]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        // T1 only ends before it starts: dropped. T2 keeps its entries in
        // gtfs-lib's order (by start, the first of equal starts, the
        // unreadable start INT_MISSING last), the negative headway too
        let trips: Vec<&TripSchedule> = layer.patterns.iter().flat_map(|p| &p.trips).collect();
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].trip_id, "T2");
        let e = |start, end, headway| FrequencyEntry {
            start,
            end,
            headway,
            exact: false,
        };
        assert_eq!(
            trips[0].frequencies,
            [
                e(21_600, 32_400, -60),
                e(36_000, 39_600, 600),
                e(crate::gtfs::jparse::INT_MISSING, 54_000, 300)
            ]
        );
        assert_eq!(layer.non_positive_headway(), Some(("T2", -60)));
        assert_eq!(layer.dropped_trips().invalid_frequencies, 1);
    }

    #[test]
    fn frequency_order_needs_a_consistent_comparison() {
        let e = |start| FrequencyEntry {
            start,
            end: 0,
            headway: 1,
            exact: false,
        };
        let m = crate::gtfs::jparse::INT_MISSING;
        let order = |v: &[i32]| {
            java_frequency_order(&v.iter().map(|&s| e(s)).collect::<Vec<_>>())
                .map(|o| o.iter().map(|f| f.start).collect::<Vec<_>>())
        };
        assert_eq!(order(&[5, m, 3, 5]), Some(vec![3, 5, m]));
        assert_eq!(order(&[m, m]), Some(vec![m]));
        // MIN - 0 and 0 - MIN are both negative in int arithmetic
        assert_eq!(order(&[0, m]), None);
        assert_eq!(order(&[-5, m, -9]), Some(vec![m, -9, -5]));
        assert_eq!(order(&[-5, m, 9]), None);
        assert_eq!(order(&[-5, i32::MAX - 1]), None);
    }

    /// Trips left out of the timetable are counted by reason, over all feeds.
    #[test]
    fn dropped_trips_are_counted() {
        use crate::gtfs::zip::tests::make_zip;
        let feed = make_zip(
            &[
                ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,-30.0,-51.2\nB,-30.01,-51.21\n"),
                ("routes.txt", b"route_id,route_type\nR,3\n"),
                (
                    "trips.txt",
                    b"route_id,service_id,trip_id\nR,S,OK\nR,S,NOTIMES\nR,S,BADSTOP\nX,S,BADROUTE\nR,X,BADSERVICE\nR,S,NOEND\nR,S,BACKWARDS\nR,S,BADFREQ\n",
                ),
                (
                    "stop_times.txt",
                    b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
OK,08:00:00,08:00:00,A,1\nOK,08:10:00,08:10:00,B,2\n\
BADSTOP,08:00:00,08:00:00,A,1\nBADSTOP,08:10:00,08:10:00,Z,2\n\
BADROUTE,08:00:00,08:00:00,A,1\nBADROUTE,08:10:00,08:10:00,B,2\n\
BADSERVICE,08:00:00,08:00:00,A,1\nBADSERVICE,08:10:00,08:10:00,B,2\n\
NOEND,08:00:00,08:00:00,A,1\nNOEND,,,B,2\n\
BACKWARDS,08:10:00,08:10:00,A,1\nBACKWARDS,08:00:00,08:00:00,B,2\n\
BADFREQ,08:00:00,08:00:00,A,1\nBADFREQ,08:10:00,08:10:00,B,2\n",
                ),
                ("calendar.txt", b"service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n"),
                ("frequencies.txt", b"trip_id,start_time,end_time,headway_secs\nBADFREQ,09:00:00,08:00:00,600\n"),
            ],
            true,
        );
        let dir = std::env::temp_dir().join(format!("peabiru-dropped-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.zip"), dir.join("b.zip"));
        std::fs::write(&a, &feed).unwrap();
        std::fs::write(&b, &feed).unwrap();
        let one = TransitLayer::load(std::slice::from_ref(&a)).unwrap();
        let two = TransitLayer::load(&[a, b]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let kept: Vec<&str> = one
            .patterns
            .iter()
            .flat_map(|p| &p.trips)
            .map(|t| t.trip_id.as_str())
            .collect();
        assert_eq!(kept, ["OK"]);
        let each = DroppedTrips {
            no_stop_times: 1,
            unknown_stop: 1,
            unknown_route: 1,
            unknown_service: 1,
            missing_end_time: 1,
            negative_time: 1,
            invalid_frequencies: 1,
        };
        assert_eq!(one.dropped_trips, each);
        assert_eq!(one.dropped_trips.total(), 7);
        assert_eq!(two.dropped_trips.total(), 14);
        assert_eq!(two.dropped_trips.unknown_stop, 2);
        // and per feed
        assert_eq!(one.feed_reports.len(), 1);
        assert_eq!(one.feed_reports[0].dropped_trips, each);
        assert_eq!(two.feed_reports[1].dropped_trips, each);
        assert_eq!(two.feed_reports[1].file_name(), "b.zip");
    }

    /// The files of a feed: name and contents.
    type Files = Vec<(String, Vec<u8>)>;

    /// A clean feed for the warning tests: agency.txt, two stops, one trip.
    fn clean_feed() -> Files {
        [
            ("agency.txt", &b"agency_id,agency_name,agency_url,agency_timezone\nA,A,http://a,UTC\n"[..]),
            ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,-30.0,-51.2\nB,-30.01,-51.21\n"),
            ("routes.txt", b"route_id,route_type\nR,3\n"),
            ("trips.txt", b"route_id,service_id,trip_id\nR,S,T1\n"),
            (
                "stop_times.txt",
                b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,B,2\n",
            ),
            ("calendar.txt", b"service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,1,1,20190101,20191231\n"),
        ]
        .iter()
        .map(|(n, b)| (n.to_string(), b.to_vec()))
        .collect()
    }

    fn set(files: &mut Files, name: &str, body: &[u8]) {
        files.retain(|f| f.0 != name);
        files.push((name.to_string(), body.to_vec()));
    }

    /// Load each feed alone (written to a temporary file of that name).
    fn reports(feeds: &[(&str, Files)]) -> Vec<(TransitLayer, FeedReport)> {
        use crate::gtfs::zip::tests::make_zip;
        // tests run in parallel: one directory per call, named after its first feed
        let dir = std::env::temp_dir().join(format!(
            "peabiru-feed-warn-{}-{}",
            std::process::id(),
            feeds[0].0
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let out = feeds
            .iter()
            .map(|(name, files)| {
                let files: Vec<(&str, &[u8])> = files
                    .iter()
                    .map(|(n, b)| (n.as_str(), b.as_slice()))
                    .collect();
                let path = dir.join(name);
                std::fs::write(&path, make_zip(&files, true)).unwrap();
                let layer = TransitLayer::load(&[path]).unwrap();
                let report = layer.feed_reports[0].clone();
                (layer, report)
            })
            .collect();
        std::fs::remove_dir_all(&dir).unwrap();
        out
    }

    /// Feeds built in spite of errors of priority HIGH are reported, with a
    /// warning naming the file; a clean feed gives none.
    #[test]
    fn feeds_with_ignored_errors_are_reported() {
        let clean = clean_feed();
        let nested: Files = clean
            .iter()
            .map(|(n, b)| (format!("gtfs/{n}"), b.clone()))
            .collect();
        let mut no_agency = clean.clone();
        no_agency.retain(|f| f.0 != "agency.txt");
        let mut bad_refs = clean.clone();
        set(
            &mut bad_refs,
            "trips.txt",
            b"route_id,service_id,trip_id\nR,S,T1\nR,S,T2\nR,S,T3\nX,S,T4\n",
        );
        set(
            &mut bad_refs,
            "stop_times.txt",
            b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
T1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,B,2\n\
T2,08:00:00,08:00:00,A,1\nT2,08:10:00,08:10:00,Z,2\n\
T3,08:00:00,08:00:00,Y,1\nT3,08:10:00,08:10:00,B,2\n\
T4,08:00:00,08:00:00,A,1\nT4,08:10:00,08:10:00,B,2\n",
        );
        let mut one_route = clean.clone();
        set(
            &mut one_route,
            "trips.txt",
            b"route_id,service_id,trip_id\nR,S,T1\nX,S,T2\n",
        );
        set(
            &mut one_route,
            "stop_times.txt",
            b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
T1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,B,2\n\
T2,08:00:00,08:00:00,A,1\nT2,08:10:00,08:10:00,B,2\n",
        );
        // Trips left out for other reasons are not errors of priority HIGH.
        let mut other_drops = clean.clone();
        set(
            &mut other_drops,
            "trips.txt",
            b"route_id,service_id,trip_id\nR,S,T1\nR,NOSERVICE,T2\nR,S,NOTIMES\n",
        );
        set(
            &mut other_drops,
            "stop_times.txt",
            b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
T1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,B,2\n\
T2,08:00:00,08:00:00,A,1\nT2,08:10:00,08:10:00,B,2\n",
        );
        let all = reports(&[
            ("clean.zip", clean.clone()),
            ("nested.zip", nested),
            ("no_agency.zip", no_agency),
            ("bad_refs.zip", bad_refs),
            ("one_route.zip", one_route),
            ("other_drops.zip", other_drops),
        ]);
        let warn = |i: usize| all[i].1.warning();

        assert_eq!(all[0].1.high_priority_cases(), Vec::<String>::new());
        assert_eq!(warn(0), None);
        assert!(all[0].0.feed_warnings().is_empty());

        assert_eq!(all[1].1.tables_in_subfolder.len(), 6);
        assert!(!all[1].1.no_agency);
        assert_eq!(
            warn(1).unwrap(),
            "GTFS feed 'nested.zip': its tables are in a subfolder of the .zip ('gtfs/'). \
A network is built from this feed only because GTFS errors are ignored."
        );
        // the nested feed is built all the same, as the clean one
        assert_eq!(all[1].0.patterns.len(), 1);

        assert!(all[2].1.no_agency);
        assert_eq!(
            warn(2).unwrap(),
            "GTFS feed 'no_agency.zip': it has no agency.txt. \
A network is built from this feed only because GTFS errors are ignored."
        );

        assert_eq!(all[3].1.dropped_trips.unknown_stop, 2);
        assert_eq!(all[3].1.dropped_trips.unknown_route, 1);
        assert_eq!(
            warn(3).unwrap(),
            "GTFS feed 'bad_refs.zip': 3 trips left out (2 refer to a stop that does not exist, \
1 to a route that does not exist). A network is built from this feed only because GTFS errors are ignored."
        );
        assert_eq!(all[3].0.feed_warnings(), [warn(3).unwrap()]);

        assert_eq!(
            warn(4).unwrap(),
            "GTFS feed 'one_route.zip': 1 trip left out because it refers to a route that \
does not exist. A network is built from this feed only because GTFS errors are ignored."
        );

        assert_eq!(all[5].1.dropped_trips.total(), 2);
        assert_eq!(warn(5), None);
    }

    /// Every case found in one feed goes into its one warning, and into
    /// one line each for the CLI.
    #[test]
    fn several_cases_in_one_feed() {
        let mut files: Files = clean_feed()
            .into_iter()
            .filter(|f| f.0 != "agency.txt")
            .map(|(n, b)| (format!("feed/{n}"), b))
            .collect();
        set(
            &mut files,
            "feed/stop_times.txt",
            b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,Q,2\n",
        );
        let all = reports(&[("x.zip", files)]);
        let r = &all[0].1;
        assert_eq!(
            r.warning().unwrap(),
            "GTFS feed 'x.zip': its tables are in a subfolder of the .zip ('feed/'); \
it has no agency.txt; 1 trip left out because it refers to a stop that does not exist. \
A network is built from this feed only because GTFS errors are ignored."
        );
        assert_eq!(
            r.warnings_per_case(),
            [
                "GTFS feed 'x.zip': its tables are in a subfolder of the .zip ('feed/'). \
A network is built from this feed only because GTFS errors are ignored.",
                "GTFS feed 'x.zip': it has no agency.txt. \
A network is built from this feed only because GTFS errors are ignored.",
                "GTFS feed 'x.zip': 1 trip left out because it refers to a stop that does not exist. \
A network is built from this feed only because GTFS errors are ignored."
            ]
        );
    }

    /// Extensions are matched case-sensitively.
    #[test]
    fn data_files_match_lower_case_extensions() {
        use std::ffi::OsStr;
        assert!(has_suffix(OsStr::new("feed.zip"), ".zip"));
        assert!(has_suffix(OsStr::new("sp.osm.pbf"), ".pbf"));
        assert!(!has_suffix(OsStr::new("FEED.ZIP"), ".zip"));
        assert!(!has_suffix(OsStr::new("sp.osm.PBF"), ".pbf"));
        assert!(!has_suffix(OsStr::new("feed.zip.bak"), ".zip"));
        let dir = std::env::temp_dir().join(format!("peabiru-feed-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["a.zip", "B.ZIP", "c.Zip", "d.txt"] {
            std::fs::write(dir.join(n), b"").unwrap();
        }
        let found = gtfs_files_in_dir(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found, [dir.join("a.zip")]);
    }

    /// Linking many stops splits many edges; the edge index keeps split
    /// edges by cell, so linking stays near linear and later point lookups
    /// do not slow down. Timing test: run in release with `--ignored`.
    #[test]
    #[ignore]
    fn linking_many_stops_stays_fast() {
        use crate::street::build::network_from_extract;
        use crate::street::link::find_split;
        use crate::street::link::tests::{jittered_grid, Rng};
        use std::time::{Duration, Instant};
        let lookups = |net: &StreetNetwork| {
            let mut scratch = Vec::new();
            let t = Instant::now();
            for i in 0..1000 {
                let (lat, lon) = (-23.5 + 0.0001 * i as f64, -46.6 + 0.0001 * i as f64);
                let _ = find_split(net, lat, lon, Mode::Walk, &mut scratch);
            }
            t.elapsed()
        };
        let mut net = network_from_extract(jittered_grid(&mut Rng(11), 120, -23.5, -46.6, 0.001));
        let before = lookups(&net);
        let n_stops = 16_000;
        let side = (n_stops as f64).sqrt().ceil() as usize;
        let mut layer = TransitLayer::default();
        for i in 0..n_stops {
            let (r, c) = (i / side, i % side);
            layer.stop_ids.push(format!("f:{i}"));
            layer
                .stop_lat
                .push(-23.5 + (r as f64 + 0.37) * 0.119 / side as f64);
            layer
                .stop_lon
                .push(-46.6 + (c as f64 + 0.41) * 0.119 / side as f64);
            layer.stop_vertex.push(-1);
        }
        let t = Instant::now();
        layer.link_stops(&mut net);
        let link = t.elapsed();
        let after = lookups(&net);
        eprintln!(
            "{n_stops} stops linked in {link:?}; 1000 lookups {before:?} before, {after:?} after"
        );
        // A quadratic linking takes seconds here, and lookups that scan every
        // split edge are tens of times slower after linking.
        assert!(link < Duration::from_secs(1), "{link:?}");
        assert!(
            after < before * 5 + Duration::from_millis(20),
            "{after:?} vs {before:?}"
        );
    }

    /// With several feeds in a directory, errors say which one failed.
    #[test]
    fn feed_errors_name_the_file() {
        use crate::gtfs::zip::tests::make_zip;
        let dir = std::env::temp_dir().join(format!("peabiru-feed-error-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let good = dir.join("good.zip");
        let bad = dir.join("broken_feed.zip");
        std::fs::write(
            &good,
            make_zip(
                &[
                    ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,1,1\nC,1.001,1\n"),
                    ("routes.txt", b"route_id,route_type\nR,3\n"),
                    ("trips.txt", b"route_id,service_id,trip_id\nR,S,T1\n"),
                    (
                        "stop_times.txt",
                        b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\nT1,08:10:00,08:10:00,C,2\n",
                    ),
                ],
                false,
            ),
        )
        .unwrap();
        // a missing table has no records, as for gtfs-lib
        std::fs::write(
            &bad,
            make_zip(&[("routes.txt", b"route_id,route_type\nR,3\n")], false),
        )
        .unwrap();
        assert!(TransitLayer::load(&[good.clone(), bad.clone()]).is_ok());
        std::fs::write(&bad, b"not a zip").unwrap();
        let garbage = TransitLayer::load(&[good.clone(), bad.clone()]).unwrap_err();
        let absent = TransitLayer::load(&[good, dir.join("absent.zip")]).unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
        let bad = bad.display().to_string();
        assert_eq!(
            garbage.to_string(),
            format!("{bad}: malformed zip file: no end of central directory record")
        );
        assert!(absent
            .to_string()
            .contains("absent.zip: cannot read the zip file"));
    }

    /// Callers can set the bounds on how far a zip file may expand.
    #[test]
    fn zip_limits_can_be_set() {
        use crate::gtfs::zip::tests::make_zip;
        let dir = std::env::temp_dir().join(format!("peabiru-zip-limits-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("feed.zip");
        std::fs::write(
            &path,
            make_zip(
                &[
                    ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,1,1\n"),
                    ("routes.txt", b"route_id,route_type\nR,3\n"),
                    ("trips.txt", b"route_id,service_id,trip_id\n"),
                    ("stop_times.txt", b"trip_id,stop_id,stop_sequence\n"),
                ],
                true,
            ),
        )
        .unwrap();
        let paths = [path.clone()];
        let tight = Limits {
            max_entry: 10,
            ..Limits::default()
        };
        let err = TransitLayer::load_with_limits(&paths, tight).unwrap_err();
        let ok = TransitLayer::load(&paths).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            err.to_string(),
            format!(
                "{}: malformed zip file: stops.txt: entry of 32 bytes exceeds the limit of 10 bytes per entry",
                path.display()
            )
        );
        assert_eq!(ok.stop_ids, ["feed:A"]);
    }

    #[test]
    fn vincenty_matches_known_distance() {
        // Flinders Peak - Buninyong (Vincenty's paper): 54 972.271 m
        let d = ellipsoidal_distance_m(
            -37.951_033_416_7,
            144.424_867_888_9,
            -37.652_821_138_9,
            143.926_495_527_8,
        );
        assert!((d - 54_972.271).abs() < 0.001, "{d}");
    }

    #[test]
    fn stops_cut_the_shape_by_distance_as_r5_does() {
        let cuts = |shape: &[f64], stops: &[f64]| -> Vec<(u32, f32)> {
            cuts_by_distance(shape, stops.iter().copied()).into_vec()
        };
        let shape = [0.0, 100.0, 300.0, 600.0];
        // inside segments, on vertices (a fraction of 1 is the next vertex),
        // before the start and past the end (clamped)
        assert_eq!(
            cuts(&shape, &[50.0, 100.0, 200.0, 600.0, -10.0, 900.0]),
            [(0, 0.5), (1, 0.0), (1, 0.5), (3, 0.0), (0, 0.0), (3, 0.0)]
        );
        // distances are not checked: a decrease is passed over by the scan
        assert_eq!(
            cuts(&[0.0, 200.0, 100.0, 300.0], &[150.0, 250.0]),
            [(0, 0.75), (2, 0.75)]
        );
        // a repeated distance: 0/0 is NaN, kept (JTS's normalize leaves it)
        let c = cuts(&[0.0, 0.0, 10.0], &[0.0, 5.0]);
        assert!(c[0].0 == 0 && c[0].1.is_nan());
        assert_eq!(c[1], (1, 0.5));
        // below a repeated distance: -x/0 is -inf, clamped to 0
        assert_eq!(cuts(&[5.0, 5.0, 10.0], &[1.0]), [(0, 0.0)]);
        // a fraction that rounds to 1 as a float stays below the vertex here
        // (hops.rs normalizes it again when it reads it back)
        assert_eq!(cuts(&[0.0, 1.0, 2.0], &[1.0 - 1e-12]), [(0, 1.0)]);
    }

    #[test]
    fn interpolation_truncates_by_distance() {
        let st = |a: Option<i32>| StopTime {
            trip_id: "t".into(),
            arrival: a,
            departure: a,
            stop_id: "s".into(),
            stop_sequence: 0,
            pickup_type: 0,
            drop_off_type: 0,
            shape_dist: f64::NAN,
        };
        let rows = [st(Some(100)), st(None), st(Some(200))];
        let refs: Vec<&StopTime> = rows.iter().collect();
        // three stops on a meridian, 1/3 and 2/3 of the way
        let coords = [(0.0, 0.0), (0.001, 0.0), (0.003, 0.0)];
        let (arr, _) = interpolate(&refs, &coords).unwrap().unwrap();
        assert_eq!(arr, [100, 133, 200]); // 100 + 100/3 truncated
        let rows = [st(None), st(Some(1)), st(Some(2))];
        let refs: Vec<&StopTime> = rows.iter().collect();
        assert!(interpolate(&refs, &coords).unwrap().is_none());
        // GeoTools refuses coordinates out of range around a blank time
        let rows = [st(Some(100)), st(None), st(Some(200))];
        let refs: Vec<&StopTime> = rows.iter().collect();
        let nan = [(0.0, 0.0), (f64::NAN, 0.0), (0.003, 0.0)];
        assert_eq!(interpolate(&refs, &nan), Err(1));
        let far = [(0.0, 0.0), (0.001, 0.0), (90.001, 0.0)];
        assert_eq!(interpolate(&refs, &far), Err(2));
        // a longitude out of range is measured (as R5 does)
        let east = [(0.0, 0.0), (0.001, 0.0), (0.003, 181.0)];
        assert!(interpolate(&refs, &east).is_ok());
    }
}
