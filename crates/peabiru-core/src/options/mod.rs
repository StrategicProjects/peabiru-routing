//! peabiru's own multi-criteria trip planner, [`trip_options`]: for each
//! origin-destination pair, every trip worth considering on the chosen
//! criteria (total time, fare, transfers, time on foot) over a departure
//! window. Unlike the rest of the crate it reproduces nothing of r5r: it
//! builds on R5's network semantics (street searches, stop linking,
//! transfers, boarding rules) so that its times agree with the other
//! functions, and adds its own search ([`search`]) and fare comparisons
//! (`fares::incremental`).

pub(crate) mod bounds;
#[cfg(test)]
mod real_tests;
pub(crate) mod search;
#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests;

use crate::street::Mode;
use crate::transit::raptor::TransitMode;
use crate::{CancelToken, FareStructure, Itinerary, RouteError};
use std::sync::Arc;

/// Seconds to which [`trip_options`] compares total times when it decides
/// which trips of a minute's exact frontier to return: a tenth of a minute,
/// the resolution the times are reported in, so that no option of a minute
/// looks dominated by another in the reported values. (Compared to the
/// second, two trips whose reported total times are equal and whose other
/// values make one look better would both be returned; on 200 pairs of
/// Porto Alegre with all four criteria that was 2% of the minutes.)
pub(crate) const TIME_RESOLUTION_S: i32 = 6;

/// A criterion [`trip_options`] compares trips on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Criterion {
    /// Door-to-door time from the departure minute, waits included.
    Time,
    /// The fare of the trip by the fare structure (needs one).
    Fare,
    /// Transfers: rides minus one (0 for a trip without public transport).
    Transfers,
    /// Time on foot: the walking legs (access and egress when they are on
    /// foot, and the walks between stops).
    Walk,
}

impl Criterion {
    /// Every criterion, in this order.
    pub const ALL: [Criterion; 4] = [
        Criterion::Time,
        Criterion::Fare,
        Criterion::Transfers,
        Criterion::Walk,
    ];

    /// `"TIME"`, `"FARE"`, `"TRANSFERS"` or `"WALK"`.
    pub fn name(self) -> &'static str {
        match self {
            Criterion::Time => "TIME",
            Criterion::Fare => "FARE",
            Criterion::Transfers => "TRANSFERS",
            Criterion::Walk => "WALK",
        }
    }

    /// The criterion [`Criterion::name`] gives `name` (upper case), or
    /// `None`.
    pub fn from_name(name: &str) -> Option<Criterion> {
        Criterion::ALL.into_iter().find(|c| c.name() == name)
    }
}

/// The chosen criteria as flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Criteria {
    pub(crate) time: bool,
    pub(crate) fare: bool,
    pub(crate) transfers: bool,
    pub(crate) walk: bool,
}

impl Criteria {
    pub(crate) fn of(list: &[Criterion]) -> Self {
        Criteria {
            time: list.contains(&Criterion::Time),
            fare: list.contains(&Criterion::Fare),
            transfers: list.contains(&Criterion::Transfers),
            walk: list.contains(&Criterion::Walk),
        }
    }
}

/// Parameters of [`trip_options`]. The street and public transport
/// settings mean what they mean in [`ItineraryParams`](crate::ItineraryParams),
/// with the same defaults. Fill it in by changing the fields of
/// [`TripOptionsParams::default()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TripOptionsParams {
    /// The street mode of the trip without public transport and of the
    /// access to it (default [`Mode::Walk`]).
    pub mode: Mode,
    /// The public transport modes; empty (the default) for street trips
    /// only (then the only option is the direct trip).
    pub transit_modes: Vec<TransitMode>,
    /// The street mode from public transport to the destination (default
    /// [`Mode::Walk`]).
    pub egress_mode: Mode,
    /// Service date `yyyymmdd` (needed with public transport).
    pub date: u32,
    /// First departure, seconds after midnight.
    pub departure_time: i32,
    /// Departure window in minutes (default 10): the trips are searched
    /// from each minute `departure_time`, `departure_time + 60`, ...
    pub time_window: i32,
    /// The criteria, in any order; empty (the default) for all four when
    /// there is a fare structure and all but [`Criterion::Fare`] otherwise.
    /// `Fare` needs a fare structure.
    pub criteria: Vec<Criterion>,
    /// Longest walk, bicycle and car leg in minutes (default 120 each).
    pub max_walk_time: i32,
    #[allow(missing_docs)]
    pub max_bike_time: i32,
    #[allow(missing_docs)]
    pub max_car_time: i32,
    /// Trip limit in whole minutes (default 120): trips longer than this
    /// are left out.
    pub max_trip_duration: i32,
    /// Walking speed, km/h (default 3.6).
    pub walk_speed_kmh: f64,
    /// Cycling speed, km/h (default 12).
    pub bike_speed_kmh: f64,
    /// Most vehicles boarded (default 3).
    pub max_rides: usize,
    /// Highest level of traffic stress a cyclist rides on, 1 to 4
    /// (default 2).
    pub max_lts: i32,
    /// r5r's fare structure (default `None`): needed for the fare
    /// criterion, and gives every option its fare.
    pub fare_structure: Option<Arc<FareStructure>>,
    /// Highest fare (default infinity), read as a Java `float` as the
    /// other functions do; a finite value needs a `fare_structure`. The
    /// options are the trips not dominated among those within the limit.
    pub max_fare: f64,
    /// Most options per pair (default 0: no limit); the first ones in the
    /// order of [`TripOption::option`] are kept.
    pub max_options: usize,
    /// Seconds to which time on foot is compared (default 60: whole
    /// minutes, rounded half up; 1: exact). The search is exact; of the
    /// trips of each minute's exact frontier, those that another trip beats
    /// once walking is rounded to this step (and total time to a tenth of a
    /// minute, the resolution it is reported in) are left out, so that two
    /// trips a few seconds apart are not both returned. Of trips equal in
    /// those terms the one with the least exact total time, then transfers,
    /// walking and fare is kept. The values returned are exact.
    pub walk_resolution_s: i32,
    /// Build each option's legs ([`TripOption::itinerary`], default true).
    pub itineraries: bool,
    /// Fill in the street edges, OpenStreetMap ids and stops of the legs,
    /// as [`ItineraryParams::osm_link_ids`](crate::ItineraryParams::osm_link_ids)
    /// (default false).
    pub osm_link_ids: bool,
    /// Car speeds and levels of traffic stress changed for this call, as in
    /// [`ItineraryParams::scenario`](crate::ItineraryParams::scenario).
    pub scenario: crate::Scenario,
    /// Worker threads; 0 means all cores.
    pub n_threads: usize,
    /// Stops the computation when cancelled.
    pub cancel: CancelToken,
}

impl Default for TripOptionsParams {
    fn default() -> Self {
        TripOptionsParams {
            mode: Mode::Walk,
            transit_modes: Vec::new(),
            egress_mode: Mode::Walk,
            date: 0,
            departure_time: 0,
            time_window: 10,
            criteria: Vec::new(),
            max_walk_time: 120,
            max_bike_time: 120,
            max_car_time: 120,
            max_trip_duration: 120,
            walk_speed_kmh: 3.6,
            bike_speed_kmh: 12.0,
            max_rides: 3,
            max_lts: 2,
            fare_structure: None,
            max_fare: f64::INFINITY,
            max_options: 0,
            walk_resolution_s: 60,
            itineraries: true,
            osm_link_ids: false,
            scenario: crate::Scenario::default(),
            n_threads: 0,
            cancel: CancelToken::default(),
        }
    }
}

impl TripOptionsParams {
    /// The criteria a call uses: [`TripOptionsParams::criteria`], or its
    /// default; an error for none valid (`Fare` without a fare structure).
    pub fn resolved_criteria(&self) -> Result<Vec<Criterion>, RouteError> {
        let mut c = if self.criteria.is_empty() {
            Criterion::ALL
                .into_iter()
                .filter(|&c| c != Criterion::Fare || self.fare_structure.is_some())
                .collect()
        } else {
            self.criteria.clone()
        };
        c.sort_unstable();
        c.dedup();
        if c.contains(&Criterion::Fare) && self.fare_structure.is_none() {
            return Err(RouteError::Invalid(
                "the FARE criterion needs a fare_structure; create one with \
                 setup_fare_structure() or read_fare_structure()"
                    .into(),
            ));
        }
        Ok(c)
    }
}

/// One option of a pair: a way of making the trip that is not dominated
/// on the chosen criteria at one or more departure minutes of the window.
///
/// Two trips found at the same or different minutes are the same option
/// when they take the same street modes, ride the same routes between the
/// same stops in the same order (whichever vehicles of those routes), and
/// have the same fare, transfers and time on foot. Their total time,
/// which counts the wait from the departure minute, changes from minute to
/// minute; the option reports the trip of its *representative* minute, the
/// one with the least total time (the earliest of equal ones).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TripOption {
    /// The pair (index into the origins and destinations given).
    pub pair: usize,
    /// Index within the pair, from 1, in the order of total time, then
    /// fare, transfers, time on foot, representative minute and
    /// description.
    pub option: u32,
    /// Door to door from the representative minute, seconds, and minutes
    /// with one decimal place.
    pub total_time_s: i32,
    #[allow(missing_docs)]
    pub total_time: f64,
    /// The fare, in the currency of the fare structure (`None` without
    /// one).
    pub fare: Option<f64>,
    /// Rides minus one; 0 without public transport.
    pub transfers: u32,
    /// Seconds on foot, and minutes with one decimal place.
    pub walk_time_s: i32,
    #[allow(missing_docs)]
    pub walk_time: f64,
    /// The representative departure minute (seconds after midnight), and
    /// the arrival from it.
    pub departure_s: i32,
    #[allow(missing_docs)]
    pub arrival_s: i32,
    /// Every departure minute at which the option is on the frontier,
    /// ascending (seconds after midnight).
    pub departures: Vec<i32>,
    /// The total time at each of [`TripOption::departures`], seconds, and
    /// minutes with one decimal place.
    pub totals_s: Vec<i32>,
    #[allow(missing_docs)]
    pub totals: Vec<f64>,
    /// The street modes and the rides in order, as `WALK > BUS 343 > RAIL
    /// LINHA1 > WALK` (route short names, else their ids).
    pub description: String,
    /// The legs of the trip of the representative minute, in the format
    /// of [`detailed_itineraries`](crate::detailed_itineraries) (with
    /// [`TripOptionsParams::itineraries`]).
    pub itinerary: Option<Itinerary>,
}

impl TripOption {
    /// How many departure minutes the option is on the frontier at.
    pub fn n_departures(&self) -> usize {
        self.departures.len()
    }
}

/// peabiru's multi-criteria trip planner: for each origin-destination pair
/// (`origins[i]` to `destinations[i]`, `(lat, lon)` degrees), the trips not
/// dominated on the chosen criteria at each departure minute of the
/// window, merged over the window into options.
///
/// At each minute the result is the exact Pareto set of the trips with
/// R5's network semantics (the direct trip by the street mode, and public
/// transport trips from the stops of the access search to those of the
/// egress search, boarding at least 60 s after reaching a stop), one trip
/// per vector of values: a trip is dominated when another is no worse on
/// every chosen criterion and better on one; of trips with the same values
/// on the chosen criteria, the one with the least total time, then
/// transfers, time on foot and fare is kept. Of that exact set, the trips
/// are then compared with total time to a tenth of a minute and walking to
/// [`TripOptionsParams::walk_resolution_s`] (a minute by default), and those
/// beaten in those terms are left out; the values returned are exact. There are no random draws:
/// frequency-based trips are boarded at their expected times (half a
/// headway of waiting; the scheduled departures with `exact_times = 1`),
/// so the result does not change from call to call and differs on purpose
/// from r5r's Monte Carlo draws. See the guide of the packages for the
/// definitions and how this differs from `pareto_frontier()` and
/// `detailed_itineraries()`.
///
/// Errors: invalid parameters, the fare criterion without a fare
/// structure, cancellation, and a car search that R5 does not route
/// reproducibly (see [`detailed_itineraries`](crate::detailed_itineraries)).
pub fn trip_options(
    network: &crate::Network,
    origins: &[(f64, f64)],
    destinations: &[(f64, f64)],
    p: &TripOptionsParams,
) -> Result<Vec<Vec<TripOption>>, RouteError> {
    let (net, transit) = (network.street(), network.transit_pair());
    crate::itinerary::trip_options_impl(net, transit, origins, destinations, p)
}

/// Write [`trip_options`]' result to `dir`, one file per pair with options,
/// `<dir>/from_<origin id>_to_<destination id>.csv`: one row per leg of each
/// option (with [`TripOptionsParams::itineraries`]) or per option, with the
/// pair's ids and points, the option's columns (`option`, `total_time`,
/// `fare` with a fare structure, `transfers`, `walk_time`,
/// `departure_time`, `arrival_time`, `n_departures`, `first_departure`,
/// `last_departure`, `description`) and the leg's (`segment`, `mode`,
/// `cumulative_fare` with a fare structure, `segment_duration`, `wait`,
/// `distance`, `route`, the four id columns with `osm_link_ids`, and the
/// `geometry` as WKT unless `drop_geometry`). Numbers are written as Java
/// prints a `double`, as the other files of the package are. `ids` and
/// `points` are those of the pairs given to [`trip_options`].
#[allow(clippy::type_complexity)]
pub fn write_trip_options_csv(
    dir: &str,
    ids: &[(String, String)],
    points: &[((f64, f64), (f64, f64))],
    options: &[Vec<TripOption>],
    drop_geometry: bool,
    osm_link_ids: bool,
) -> std::io::Result<()> {
    use crate::output::{csv_field, java_double, LINE_END};
    let clock = |s: i32| format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60);
    for (k, opts) in options.iter().enumerate() {
        let Some(first) = opts.first() else {
            continue;
        };
        let (from, to) = (&ids[k].0, &ids[k].1);
        let ((olat, olon), (dlat, dlon)) = points[k];
        let fares = first.fare.is_some();
        let legs = first.itinerary.is_some();
        let mut s = String::from("from_id,from_lat,from_lon,to_id,to_lat,to_lon,option,total_time");
        if fares {
            s.push_str(",fare");
        }
        s.push_str(
            ",transfers,walk_time,departure_time,arrival_time,n_departures,first_departure,\
             last_departure,description",
        );
        if legs {
            s.push_str(",segment,mode");
            if fares {
                s.push_str(",cumulative_fare");
            }
            s.push_str(",segment_duration,wait,distance,route");
            if osm_link_ids {
                s.push_str(",osm_id_list,edge_id_list,board_stop_id,alight_stop_id");
            }
            if !drop_geometry {
                s.push_str(",geometry");
            }
        }
        s.push_str(LINE_END);
        for x in opts {
            let mut head = String::new();
            csv_field(from, &mut head);
            for v in [olat, olon] {
                head.push(',');
                java_double(v, &mut head);
            }
            head.push(',');
            csv_field(to, &mut head);
            for v in [dlat, dlon] {
                head.push(',');
                java_double(v, &mut head);
            }
            head.push_str(&format!(",{},", x.option));
            java_double(x.total_time, &mut head);
            if let Some(f) = x.fare {
                head.push(',');
                java_double(f, &mut head);
            }
            head.push_str(&format!(",{},", x.transfers));
            java_double(x.walk_time, &mut head);
            head.push_str(&format!(
                ",{},{},{},{},{},",
                clock(x.departure_s),
                clock(x.arrival_s),
                x.n_departures(),
                clock(x.departures[0]),
                clock(*x.departures.last().expect("a departure"))
            ));
            csv_field(&x.description, &mut head);
            match &x.itinerary {
                Some(it) if legs => {
                    for (j, l) in it.legs.iter().enumerate() {
                        s.push_str(&head);
                        s.push_str(&format!(",{},", j + 1));
                        csv_field(&l.mode, &mut s);
                        if let Some(f) = l.cumulative_fare {
                            s.push(',');
                            java_double(f, &mut s);
                        }
                        for v in [l.segment_duration, l.wait] {
                            s.push(',');
                            java_double(v, &mut s);
                        }
                        s.push_str(&format!(",{},", l.distance));
                        csv_field(&l.route, &mut s);
                        if osm_link_ids {
                            for v in [
                                l.osm_id_list(),
                                l.edge_id_list(),
                                l.board_stop_id.clone(),
                                l.alight_stop_id.clone(),
                            ] {
                                s.push(',');
                                csv_field(&v, &mut s);
                            }
                        }
                        if !drop_geometry {
                            s.push(',');
                            csv_field(&l.wkt(), &mut s);
                        }
                        s.push_str(LINE_END);
                    }
                }
                _ => {
                    s.push_str(&head);
                    s.push_str(LINE_END);
                }
            }
        }
        std::fs::write(format!("{dir}/from_{from}_to_{to}.csv"), s)?;
    }
    Ok(())
}
