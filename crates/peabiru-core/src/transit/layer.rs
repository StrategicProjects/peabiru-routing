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
    /// GTFS `route_id`, unscoped (R5 `RouteInfo.route_id`, what r5r reports
    /// in `routes`).
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
    /// Monday-first weekdays, start and end `yyyymmdd`.
    pub calendar: Option<([bool; 7], u32, u32)>,
    /// `yyyymmdd` -> exception type (1 added, 2 removed).
    pub exceptions: HashMap<u32, u8>,
}

impl Service {
    /// `date` is `yyyymmdd`, `weekday` 0 = Monday.
    pub fn active_on(&self, date: u32, weekday: usize) -> bool {
        if let Some(&e) = self.exceptions.get(&date) {
            return e == 1;
        }
        match self.calendar {
            Some((days, start, end)) => start <= date && date <= end && days[weekday],
            None => false,
        }
    }
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
    /// [`TransitLayer::r5r_warnings`]).
    pub(crate) feed_reports: Vec<FeedReport>,
    /// Drawn, not routed on (r5r's `transit_network_to_sf()`; see
    /// `crate::inspect`): the `stop_name` of each stop (`None` when blank),
    /// R5's `RouteInfo` list, and per pattern its index in it and the
    /// shape of the trip that made the pattern (into `shapes`).
    pub(crate) stop_names: Vec<Option<String>>,
    /// `service_id` of each service.
    pub(crate) service_ids: Vec<String>,
    pub(crate) routes: Vec<RouteInfo>,
    pub(crate) pattern_route: Vec<u32>,
    pub(crate) pattern_shape: Vec<Option<u32>>,
    /// Per pattern with a shape: whether its first trip's stop times and
    /// the shape's points all have `shape_dist_traveled` (R5 then cuts the
    /// shape by those distances, which detailed itineraries do not follow).
    pub(crate) pattern_shape_dist: Vec<bool>,
    /// Shapes of `shapes.txt` as `[lon, lat]` points, in sequence order.
    pub(crate) shapes: Vec<Vec<[f64; 2]>>,
}

/// R5's `RouteInfo`: a GTFS route as r5r reports it, `None` for a blank
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

/// What loading one GTFS feed found. Some of it makes r5r 2.5.0 refuse to
/// build a network from the feed (gtfs-lib reports a high-priority error),
/// while peabiru builds it: see [`FeedReport::r5r_refusals`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct FeedReport {
    /// The feed's `.zip` file.
    pub path: std::path::PathBuf,
    /// Entries of the tables read from a subfolder of the `.zip` rather than
    /// its root (r5r: `TableInSubdirectoryError`).
    pub tables_in_subfolder: Vec<String>,
    /// The feed has no `agency.txt` (r5r: `NoAgencyInFeedError`).
    pub no_agency: bool,
    /// Trips of this feed left out of the timetable, by reason. Only
    /// `unknown_stop` and `unknown_route` make r5r refuse the feed
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

    /// Why r5r would refuse this feed, one phrase per case (empty when it
    /// would not): tables in a subfolder, no `agency.txt`, trips that refer
    /// to a stop or route that does not exist.
    pub fn r5r_refusals(&self) -> Vec<String> {
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

    /// One warning for this feed naming everything r5r would refuse it
    /// for, or `None` when r5r would build it.
    pub fn r5r_warning(&self) -> Option<String> {
        let found = self.r5r_refusals();
        (!found.is_empty()).then(|| warning(&self.file_name(), &found.join("; ")))
    }

    /// One warning per case r5r would refuse this feed for.
    pub fn r5r_warnings_per_case(&self) -> Vec<String> {
        let file = self.file_name();
        self.r5r_refusals()
            .iter()
            .map(|c| warning(&file, c))
            .collect()
    }
}

fn warning(file: &str, found: &str) -> String {
    format!("GTFS feed '{file}': {found}. r5r refuses to build a network from this feed.")
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

/// Fill blank times as gtfs-lib `getInterpolatedStopTimesForTrip` does;
/// `None` when the first or last stop has no time.
fn interpolate(rows: &[&StopTime], coords: &[(f64, f64)]) -> Option<(Vec<i32>, Vec<i32>)> {
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
        return None;
    }
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
            let (lat0, lon0) = coords[k - 1];
            let (lat1, lon1) = coords[k];
            total += ellipsoidal_distance_m(lat0, lon0, lat1, lon1);
            cum[k - a] = total;
        }
        let start = dep[a - 1].expect("timed");
        let span = f64::from(arr[b].expect("timed") - start);
        for k in a..b {
            let t = java_int(f64::from(start) + span * (cum[k - a] / total));
            arr[k] = Some(t);
            dep[k] = Some(t);
        }
        i = b;
    }
    Some((
        arr.into_iter().map(|x| x.expect("filled")).collect(),
        dep.into_iter().map(|x| x.expect("filled")).collect(),
    ))
}

/// gtfs-lib's pattern key: route, stops, pickup types, drop-off types.
type PatternKey = (String, Vec<u32>, Vec<u8>, Vec<u8>);

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

    /// Number of services running on `date` (`yyyymmdd`), as r5r's
    /// `getTransitServiceAvailability` counts them;
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
        Self::from_feeds(paths.iter().map(|p| read_feed_file(p)), limits)
    }

    /// Load feeds given as the path (or file name) and the bytes of each
    /// `.zip`, in the order given; reading stops at the first error. The
    /// path names the feed in errors and reports and, without a
    /// `feed_info.feed_id`, gives the feed id.
    pub(crate) fn from_feeds<'a>(
        feeds: impl IntoIterator<Item = Result<(std::path::PathBuf, Cow<'a, [u8]>), GtfsError>>,
        limits: Limits,
    ) -> Result<TransitLayer, GtfsError> {
        let mut layer = TransitLayer::default();
        for f in feeds {
            let (path, bytes) = f?;
            let (feed, archive) = Feed::read_zip_checked(&path, bytes, limits)?;
            let id = feed_id(&feed, &path);
            let dropped_trips = layer.add_feed(&feed, &id);
            layer.dropped_trips.add(&dropped_trips);
            layer.feed_ids.push(id);
            layer.feed_reports.push(FeedReport {
                path,
                tables_in_subfolder: archive.tables_in_subfolder,
                no_agency: archive.no_agency,
                dropped_trips,
            });
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

    /// Warnings for the feeds that r5r refuses to build a network from and
    /// peabiru builds, one per feed, in loading order (empty when there are
    /// none). The core never prints them; the bindings and the CLI do.
    pub fn r5r_warnings(&self) -> Vec<String> {
        self.feed_reports
            .iter()
            .filter_map(FeedReport::r5r_warning)
            .collect()
    }

    /// Add one feed; returns the trips it left out.
    fn add_feed(&mut self, feed: &Feed, feed_id: &str) -> DroppedTrips {
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
            services
                .entry(d.service_id.as_str())
                .or_default()
                .exceptions
                .entry(d.date)
                .or_insert(d.exception_type);
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
        // frequencies per trip, by start time; an equal start keeps the first.
        let mut freqs: HashMap<&str, BTreeMap<i32, FrequencyEntry>> = HashMap::new();
        for f in &feed.frequencies {
            freqs
                .entry(f.trip_id.as_str())
                .or_default()
                .entry(f.start_time)
                .or_insert(FrequencyEntry {
                    start: f.start_time,
                    end: f.end_time,
                    headway: f.headway_secs,
                });
        }

        let trips: BTreeMap<&str, &crate::gtfs::feed::Trip> =
            feed.trips.iter().map(|t| (t.trip_id.as_str(), t)).collect();
        // gtfs-lib pattern key -> R5 pattern index
        let mut pattern_of: HashMap<PatternKey, usize> = HashMap::new();
        let first_pattern = self.patterns.len();
        let mut dropped = DroppedTrips::default();
        for (trip_id, trip) in trips {
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
            let Some(&rtype) = route_type.get(trip.route_id.as_str()) else {
                dropped.unknown_route += 1;
                continue;
            };
            let key = (
                trip.route_id.clone(),
                stop_seq.clone(),
                rows.iter().map(|r| r.pickup_type).collect::<Vec<_>>(),
                rows.iter().map(|r| r.drop_off_type).collect::<Vec<_>>(),
            );
            let coords: Vec<(f64, f64)> = stop_seq
                .iter()
                .map(|&s| (self.stop_lat[s as usize], self.stop_lon[s as usize]))
                .collect();
            let Some((mut arrivals, mut departures)) = interpolate(&rows, &coords) else {
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
                    self.patterns.push(Pattern {
                        route_id: trip.route_id.clone(),
                        route_type: rtype,
                        stops: stop_seq.clone(),
                        pickups: rows.iter().map(|r| r.pickup_type != 1).collect(),
                        dropoffs: rows.iter().map(|r| r.drop_off_type != 1).collect(),
                        ..Pattern::default()
                    });
                    let ri = *route_index
                        .entry(trip.route_id.as_str())
                        .or_insert_with(|| {
                            self.routes
                                .push(route_info(routes_by_id[trip.route_id.as_str()], &agencies));
                            (self.routes.len() - 1) as u32
                        });
                    self.pattern_route.push(ri);
                    let shape = match trip.shape_id.as_str() {
                        "" => None,
                        id => *shape_index.entry(id).or_insert_with(|| {
                            // gtfs-lib's getShape: no shape without points
                            let points = feed.shapes.get(id).filter(|p| !p.is_empty())?;
                            self.shapes
                                .push(points.values().map(|&(lon, lat)| [lon, lat]).collect());
                            Some((self.shapes.len() - 1) as u32)
                        }),
                    };
                    let by_dist = shape.is_some()
                        && rows.iter().all(|r| r.shape_dist)
                        && feed
                            .shape_dists
                            .get(trip.shape_id.as_str())
                            .is_some_and(|m| m.values().all(|&d| d));
                    self.pattern_shape.push(shape);
                    self.pattern_shape_dist.push(by_dist);
                    pattern_of.insert(key, self.patterns.len() - 1);
                    self.patterns.len() - 1
                }
            };
            let Some(&service) = service_code.get(trip.service_id.as_str()) else {
                dropped.unknown_service += 1;
                continue;
            };
            let mut frequencies = Vec::new();
            if let Some(f) = freqs.get(trip_id) {
                // start > end is dropped as R5 does; a non-positive headway would
                // make R5 fail the whole query, so it is dropped as invalid too.
                frequencies = f
                    .values()
                    .copied()
                    .filter(|e| e.start <= e.end && e.headway > 0)
                    .collect();
                if frequencies.is_empty() {
                    dropped.invalid_frequencies += 1; // only invalid entries
                    continue;
                }
                let base = arrivals[0];
                arrivals.iter_mut().for_each(|t| *t -= base);
                departures.iter_mut().for_each(|t| *t -= base);
            }
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
            p.trips.sort_by_key(|t| t.departures[0]); // stable
        }
        dropped
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

/// Whether a file name ends with `suffix`, case-sensitively, as r5r's
/// `NetworkBuilder.loadDirectory` tests names (`String.endsWith`).
pub(crate) fn has_suffix(name: &std::ffi::OsStr, suffix: &str) -> bool {
    name.as_encoded_bytes().ends_with(suffix.as_bytes())
}

/// The `.zip` files of a directory (lower-case extension, as r5r matches
/// it) in the order the operating system lists them, which is the order R5
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_headways_are_dropped() {
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
                ("frequencies.txt", b"trip_id,start_time,end_time,headway_secs\nT1,06:00:00,09:00:00,0\nT2,06:00:00,09:00:00,-60\nT2,10:00:00,11:00:00,600\n"),
            ],
            true,
        );
        let dir = std::env::temp_dir().join(format!("peabiru-headway-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("feed.zip");
        std::fs::write(&path, z).unwrap();
        let layer = TransitLayer::load(&[path]).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        // T1 has only a zero headway: dropped; T2 keeps its valid entry
        let trips: Vec<&TripSchedule> = layer.patterns.iter().flat_map(|p| &p.trips).collect();
        assert_eq!(trips.len(), 1);
        assert_eq!(trips[0].trip_id, "T2");
        assert_eq!(
            trips[0].frequencies,
            [FrequencyEntry {
                start: 36_000,
                end: 39_600,
                headway: 600
            }]
        );
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
            "peabiru-r5r-warn-{}-{}",
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

    /// Feeds that r5r refuses and peabiru builds are reported, with a
    /// warning naming the file; a clean feed gives none.
    #[test]
    fn feeds_r5r_refuses_are_reported() {
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
        // Trips left out for other reasons are not what r5r refuses a feed for.
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
        let warn = |i: usize| all[i].1.r5r_warning();

        assert_eq!(all[0].1.r5r_refusals(), Vec::<String>::new());
        assert_eq!(warn(0), None);
        assert!(all[0].0.r5r_warnings().is_empty());

        assert_eq!(all[1].1.tables_in_subfolder.len(), 6);
        assert!(!all[1].1.no_agency);
        assert_eq!(
            warn(1).unwrap(),
            "GTFS feed 'nested.zip': its tables are in a subfolder of the .zip ('gtfs/'). \
r5r refuses to build a network from this feed."
        );
        // the nested feed is built all the same, as the clean one
        assert_eq!(all[1].0.patterns.len(), 1);

        assert!(all[2].1.no_agency);
        assert_eq!(
            warn(2).unwrap(),
            "GTFS feed 'no_agency.zip': it has no agency.txt. \
r5r refuses to build a network from this feed."
        );

        assert_eq!(all[3].1.dropped_trips.unknown_stop, 2);
        assert_eq!(all[3].1.dropped_trips.unknown_route, 1);
        assert_eq!(
            warn(3).unwrap(),
            "GTFS feed 'bad_refs.zip': 3 trips left out (2 refer to a stop that does not exist, \
1 to a route that does not exist). r5r refuses to build a network from this feed."
        );
        assert_eq!(all[3].0.r5r_warnings(), [warn(3).unwrap()]);

        assert_eq!(
            warn(4).unwrap(),
            "GTFS feed 'one_route.zip': 1 trip left out because it refers to a route that \
does not exist. r5r refuses to build a network from this feed."
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
            r.r5r_warning().unwrap(),
            "GTFS feed 'x.zip': its tables are in a subfolder of the .zip ('feed/'); \
it has no agency.txt; 1 trip left out because it refers to a stop that does not exist. \
r5r refuses to build a network from this feed."
        );
        assert_eq!(
            r.r5r_warnings_per_case(),
            [
                "GTFS feed 'x.zip': its tables are in a subfolder of the .zip ('feed/'). \
r5r refuses to build a network from this feed.",
                "GTFS feed 'x.zip': it has no agency.txt. r5r refuses to build a network from this feed.",
                "GTFS feed 'x.zip': 1 trip left out because it refers to a stop that does not exist. \
r5r refuses to build a network from this feed."
            ]
        );
    }

    /// Extensions are matched case-sensitively, as r5r does.
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
        std::fs::write(
            &bad,
            make_zip(&[("routes.txt", b"route_id,route_type\nR,3\n")], false),
        )
        .unwrap();
        let missing = TransitLayer::load(&[good.clone(), bad.clone()]).unwrap_err();
        std::fs::write(&bad, b"not a zip").unwrap();
        let garbage = TransitLayer::load(&[good.clone(), bad.clone()]).unwrap_err();
        let absent = TransitLayer::load(&[good, dir.join("absent.zip")]).unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
        let bad = bad.display().to_string();
        assert_eq!(
            missing.to_string(),
            format!("{bad}: GTFS feed has no stops.txt")
        );
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
    fn interpolation_truncates_by_distance() {
        let st = |a: Option<i32>| StopTime {
            trip_id: "t".into(),
            arrival: a,
            departure: a,
            stop_id: "s".into(),
            stop_sequence: 0,
            pickup_type: 0,
            drop_off_type: 0,
            shape_dist: false,
        };
        let rows = [st(Some(100)), st(None), st(Some(200))];
        let refs: Vec<&StopTime> = rows.iter().collect();
        // three stops on a meridian, 1/3 and 2/3 of the way
        let coords = [(0.0, 0.0), (0.001, 0.0), (0.003, 0.0)];
        let (arr, _) = interpolate(&refs, &coords).unwrap();
        assert_eq!(arr, [100, 133, 200]); // 100 + 100/3 truncated
        let rows = [st(None), st(Some(1)), st(Some(2))];
        let refs: Vec<&StopTime> = rows.iter().collect();
        assert!(interpolate(&refs, &coords).is_none());
    }
}
