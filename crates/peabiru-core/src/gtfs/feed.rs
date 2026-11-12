//! GTFS tables as read from a feed, with string ids and raw values. How they
//! become a routable timetable (calendars for a date, missing times,
//! patterns) is decided by the transit layer, following R5.

use super::csv::{for_each_row, CsvError, Header, ReadError, Row};
use super::jparse::{parse_double, parse_int, INT_MISSING};
use super::zip::{Archive, Limits, ZipError};
use std::collections::{BTreeMap, HashMap};
use std::fmt;

#[derive(Debug)]
pub enum GtfsError {
    Zip(ZipError),
    Csv {
        file: &'static str,
        error: CsvError,
    },
    /// Why gtfs-lib fails loading the feed (see `validate`).
    GtfsLibFails(String),
    /// Why R5 fails building its transit layer from a feed that gtfs-lib
    /// loads.
    R5Fails(String),
    /// Feeds with errors of priority `HIGH`, and how many each has.
    HighPriority(Vec<(std::path::PathBuf, usize)>),
    /// A feed peabiru cannot read that gtfs-lib loads with errors of lower
    /// priority only, so that R5 builds a network from it.
    NotRead(Box<GtfsError>),
    /// An error in the feed read from `path`.
    InFeed {
        path: std::path::PathBuf,
        error: Box<GtfsError>,
    },
}

impl fmt::Display for GtfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GtfsError::Zip(e) => write!(f, "{e}"),
            GtfsError::Csv { file, error } => write!(f, "{file} {error}"),
            GtfsError::GtfsLibFails(why) | GtfsError::R5Fails(why) => f.write_str(why),
            GtfsError::NotRead(e) => write!(
                f,
                "{e} (the GTFS errors of this feed are not of priority HIGH, but peabiru cannot \
                 read it)"
            ),
            GtfsError::HighPriority(feeds) => {
                let list: Vec<String> = feeds
                    .iter()
                    .map(|(p, n)| {
                        format!(
                            "{} ({n} error{})",
                            p.display(),
                            if *n == 1 { "" } else { "s" }
                        )
                    })
                    .collect();
                write!(
                    f,
                    "high priority GTFS errors found in {}; network build failed",
                    list.join(", ")
                )
            }
            GtfsError::InFeed { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for GtfsError {}

impl GtfsError {
    /// `error`, in the feed read from `path`.
    pub(crate) fn in_feed(path: &std::path::Path, error: GtfsError) -> Self {
        GtfsError::InFeed {
            path: path.to_path_buf(),
            error: Box::new(error),
        }
    }
}

impl From<ZipError> for GtfsError {
    fn from(e: ZipError) -> Self {
        GtfsError::Zip(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stop {
    pub stop_id: String,
    /// Empty when blank.
    pub stop_name: String,
    /// NaN when blank or not a number (gtfs-lib's `getDoubleField`).
    pub lat: f64,
    pub lon: f64,
    /// 0 stop/platform, 1 station, 2 entrance, 3 generic node, 4 boarding
    /// area; 0 when blank, [`INT_MISSING`] when not a number.
    pub location_type: i32,
    pub parent_station: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub route_id: String,
    /// 0 when blank, [`INT_MISSING`] when not a number.
    pub route_type: i32,
    /// `agency_id`, `route_short_name` and `route_long_name`, empty when
    /// blank.
    pub agency_id: String,
    pub short_name: String,
    pub long_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trip {
    pub trip_id: String,
    /// `None` when blank.
    pub route_id: Option<String>,
    pub service_id: Option<String>,
    /// Empty when blank.
    pub shape_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StopTime {
    pub trip_id: String,
    /// Seconds after midnight of the service day; `None` when blank or not
    /// a time gtfs-lib reads (its `INT_MISSING`).
    pub arrival: Option<i32>,
    pub departure: Option<i32>,
    pub stop_id: String,
    /// 0 when blank, [`INT_MISSING`] when not a number.
    pub stop_sequence: i32,
    /// 0 regular, 1 none, 2 phone, 3 coordinate with driver; 0 when blank,
    /// other numbers as given.
    pub pickup_type: i32,
    pub drop_off_type: i32,
    /// `shape_dist_traveled`, NaN when blank or missing (gtfs-lib's
    /// `getDoubleField`; a value out of range is kept, with a LOW error).
    /// R5 cuts a shape by these distances when they and the shape's are
    /// all numbers.
    pub shape_dist: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Calendar {
    pub service_id: String,
    /// Monday first: whether the day's value is the number 1, which is
    /// what R5 tests (`calendar.monday == 1`).
    pub days: [bool; 7],
    /// `yyyymmdd` as numbers, not checked to be dates: 0 when blank.
    pub start_date: i32,
    pub end_date: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDate {
    pub service_id: String,
    /// `yyyymmdd`; `None` when blank (gtfs-lib keeps such a row under a
    /// null date, which no day matches).
    pub date: Option<u32>,
    /// 1 added, 2 removed; 0 when blank. Only 1 makes the service run.
    pub exception_type: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frequency {
    pub trip_id: String,
    /// Seconds; [`INT_MISSING`] when blank or not a time gtfs-lib reads,
    /// as R5 then keeps them.
    pub start_time: i32,
    pub end_time: i32,
    /// 0 when blank.
    pub headway_secs: i32,
    /// `exact_times` is 1 (R5 ignores it; only peabiru's
    /// `trip_options()` reads it).
    pub exact_times: bool,
}

/// The tables of one feed.
#[derive(Debug, Clone, Default)]
pub struct Feed {
    /// `feed_info.feed_id` of the first record, when it has one.
    pub feed_id: Option<String>,
    pub stops: Vec<Stop>,
    pub routes: Vec<Route>,
    pub trips: Vec<Trip>,
    pub stop_times: Vec<StopTime>,
    pub calendars: Vec<Calendar>,
    pub calendar_dates: Vec<CalendarDate>,
    pub frequencies: Vec<Frequency>,
    /// `agency.txt` rows as `(agency_id, agency_name)`, both empty when
    /// blank.
    pub agencies: Vec<(String, String)>,
    /// `shapes.txt`: the points of each shape as `(lon, lat)`, by
    /// `shape_pt_sequence` (a repeated sequence keeps the last row, as in
    /// gtfs-lib's map).
    pub shapes: HashMap<String, BTreeMap<i32, (f64, f64)>>,
    /// Per shape and sequence, the point's `shape_dist_traveled` (NaN when
    /// blank or missing), kept like the points.
    pub shape_dists: HashMap<String, BTreeMap<i32, f64>>,
    /// Trips with a GTFS-Flex stop time (gtfs-lib's `flexTripIds`): R5
    /// leaves them out of the timetable.
    pub flex_trips: std::collections::HashSet<String>,
}

/// gtfs-lib's `getTimeField` on a value that is not blank: `H:M:S` split
/// as Java's `String.split(":")` splits (trailing empty parts dropped), each
/// part read by `Integer.parseInt`, and `h * 3600 + m * 60 + s` in Java's
/// wrapping `int` arithmetic; `None` where gtfs-lib records a
/// `TimeParseError` and keeps `INT_MISSING`. Out-of-range parts (`25:61:00`)
/// are kept, as gtfs-lib keeps them (with a `RangeError`).
pub(crate) fn gtfs_time(s: &str) -> Option<i32> {
    let mut parts: Vec<&str> = s.split(':').collect();
    while parts.last() == Some(&"") {
        parts.pop();
    }
    let [h, m, sec] = parts[..] else {
        return None;
    };
    let (h, m, sec) = (parse_int(h)?, parse_int(m)?, parse_int(sec)?);
    let v = h
        .wrapping_mul(3600)
        .wrapping_add(m.wrapping_mul(60))
        .wrapping_add(sec);
    // a value that wraps to INT_MISSING is read by R5 as missing
    (v != INT_MISSING).then_some(v)
}

/// The value getters of gtfs-lib's `Entity.Loader` for one record: what a
/// blank value, a value that does not parse and a missing column (read as
/// blank by javacsv) become. The errors gtfs-lib records are listed by
/// `validate`; here only the values matter.
struct Fields<'a, 'r> {
    h: &'a Header,
    r: &'a Row<'r>,
}

impl<'a> Fields<'a, '_> {
    fn str(&self, col: &str) -> &'a str {
        self.r.get(self.h.col(col))
    }

    fn opt(&self, col: &str) -> Option<&'a str> {
        Some(self.str(col)).filter(|s| !s.is_empty())
    }

    fn string(&self, col: &str) -> String {
        self.str(col).to_string()
    }

    /// `getIntField(col, _, _, _, default)`.
    fn int(&self, col: &str, default: i32) -> i32 {
        self.opt(col)
            .map_or(default, |s| parse_int(s).unwrap_or(INT_MISSING))
    }

    /// `getDoubleField`.
    fn double(&self, col: &str) -> f64 {
        self.opt(col).and_then(parse_double).unwrap_or(f64::NAN)
    }

    /// `getTimeField`, `None` for `INT_MISSING`.
    fn time(&self, col: &str) -> Option<i32> {
        self.opt(col).and_then(gtfs_time)
    }
}

impl Feed {
    /// Read a feed from the bytes of a GTFS zip file, with bounds on how
    /// far the archive may expand, also saying how the archive is laid out
    /// (for the warnings about feeds with ignored errors). Errors name the
    /// file, `path`.
    pub(crate) fn read_zip_checked(
        path: &std::path::Path,
        bytes: std::borrow::Cow<'_, [u8]>,
        limits: Limits,
    ) -> Result<(Feed, ArchiveLayout), GtfsError> {
        Archive::with_limits(bytes, limits)
            .map_err(GtfsError::from)
            .and_then(|a| Ok((Self::from_archive(&a)?, ArchiveLayout::of(&a))))
            .map_err(|error| GtfsError::in_feed(path, error))
    }

    /// The tables of a feed, every value read as gtfs-lib's loaders read it
    /// (see [`Fields`]): a blank or unreadable value takes gtfs-lib's
    /// default rather than failing, so that every feed gtfs-lib loads is
    /// read. Records gtfs-lib does not keep (no key) are left out. Fails
    /// only on a missing required table or a file javacsv refuses.
    pub fn from_archive(a: &Archive) -> Result<Feed, GtfsError> {
        let mut feed = Feed::default();

        table(a, "agency.txt", |f| {
            feed.agencies
                .push((f.string("agency_id"), f.string("agency_name")));
        })?;

        let mut first_info = true;
        table(a, "feed_info.txt", |f| {
            // FeedInfo.Loader: the first record sets the feed id
            if std::mem::take(&mut first_info) {
                feed.feed_id = f.opt("feed_id").map(str::to_string);
            }
        })?;

        table(a, "stops.txt", |f| {
            if f.opt("stop_id").is_none() {
                return; // MissingKeyError: not kept
            }
            feed.stops.push(Stop {
                stop_id: f.string("stop_id"),
                stop_name: f.string("stop_name"),
                lat: f.double("stop_lat"),
                lon: f.double("stop_lon"),
                location_type: f.int("location_type", 0),
                parent_station: f.string("parent_station"),
            });
        })?;

        table(a, "routes.txt", |f| {
            if f.opt("route_id").is_none() {
                return;
            }
            feed.routes.push(Route {
                route_id: f.string("route_id"),
                route_type: f.int("route_type", 0),
                agency_id: f.string("agency_id"),
                short_name: f.string("route_short_name"),
                long_name: f.string("route_long_name"),
            });
        })?;

        table(a, "trips.txt", |f| {
            if f.opt("trip_id").is_none() {
                return;
            }
            feed.trips.push(Trip {
                trip_id: f.string("trip_id"),
                route_id: f.opt("route_id").map(str::to_string),
                service_id: f.opt("service_id").map(str::to_string),
                shape_id: f.string("shape_id"),
            });
        })?;

        // StopTime.Loader.detectRowIsFlex: a table with a GTFS-Flex column,
        // a row with a value in one
        let mut table_has_flex = None;
        table(a, "stop_times.txt", |f| {
            if f.opt("trip_id").is_none() {
                return; // belongs to no trip
            }
            let has_flex = *table_has_flex.get_or_insert_with(|| {
                super::validate::FLEX_FIELDS
                    .iter()
                    .any(|c| f.h.col(c).is_some())
            });
            if has_flex
                && super::validate::FLEX_FIELDS
                    .iter()
                    .any(|c| f.opt(c).is_some())
            {
                feed.flex_trips.insert(f.string("trip_id"));
            }
            feed.stop_times.push(StopTime {
                trip_id: f.string("trip_id"),
                arrival: f.time("arrival_time"),
                departure: f.time("departure_time"),
                stop_id: f.string("stop_id"),
                stop_sequence: f.int("stop_sequence", 0),
                pickup_type: f.int("pickup_type", 0),
                drop_off_type: f.int("drop_off_type", 0),
                shape_dist: f.double("shape_dist_traveled"),
            });
        })?;

        const DAYS: [&str; 7] = [
            "monday",
            "tuesday",
            "wednesday",
            "thursday",
            "friday",
            "saturday",
            "sunday",
        ];
        table(a, "calendar.txt", |f| {
            let mut days = [false; 7];
            for (d, name) in days.iter_mut().zip(DAYS) {
                *d = f.int(name, 0) == 1;
            }
            feed.calendars.push(Calendar {
                service_id: f.string("service_id"),
                days,
                start_date: f.int("start_date", 0),
                end_date: f.int("end_date", 0),
            });
        })?;

        table(a, "calendar_dates.txt", |f| {
            feed.calendar_dates.push(CalendarDate {
                service_id: f.string("service_id"),
                date: f.opt("date").and_then(super::validate::basic_iso_date),
                exception_type: f.int("exception_type", 0),
            });
        })?;

        table(a, "frequencies.txt", |f| {
            feed.frequencies.push(Frequency {
                trip_id: f.string("trip_id"),
                start_time: f.time("start_time").unwrap_or(INT_MISSING),
                end_time: f.time("end_time").unwrap_or(INT_MISSING),
                headway_secs: f.int("headway_secs", 0),
                exact_times: f.int("exact_times", 0) == 1,
            });
        })?;

        // Shapes are only drawn (transit_network_to_sf()), never
        // routed on.
        table(a, "shapes.txt", |f| {
            let Some(id) = f.opt("shape_id") else {
                return;
            };
            let seq = f.int("shape_pt_sequence", 0);
            let point = (f.double("shape_pt_lon"), f.double("shape_pt_lat"));
            let dist = f.double("shape_dist_traveled");
            feed.shape_dists
                .entry(id.to_string())
                .or_default()
                .insert(seq, dist);
            match feed.shapes.get_mut(id) {
                Some(s) => {
                    s.insert(seq, point);
                }
                None => {
                    feed.shapes
                        .insert(id.to_string(), BTreeMap::from([(seq, point)]));
                }
            }
        })?;
        Ok(feed)
    }
}

/// The tables looked up in a feed: `agency.txt` and those read into a
/// [`Feed`].
const TABLES: [&str; 10] = [
    "agency.txt",
    "feed_info.txt",
    "stops.txt",
    "routes.txt",
    "trips.txt",
    "stop_times.txt",
    "calendar.txt",
    "calendar_dates.txt",
    "frequencies.txt",
    "shapes.txt",
];

/// How a feed's archive is laid out, in the two ways that make gtfs-lib
/// report an error of priority `HIGH` in a feed that peabiru can read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ArchiveLayout {
    /// Entries of the tables found in a subfolder rather than at the root
    /// (gtfs-lib's `TableInSubdirectoryError`), in [`TABLES`] order.
    pub tables_in_subfolder: Vec<String>,
    /// No `agency.txt` anywhere (gtfs-lib's `NoAgencyInFeedError`).
    pub no_agency: bool,
}

impl ArchiveLayout {
    fn of(a: &Archive) -> Self {
        ArchiveLayout {
            tables_in_subfolder: TABLES
                .iter()
                .filter_map(|&t| a.find(t).filter(|e| e.name != t))
                .map(|e| e.name.clone())
                .collect(),
            no_agency: a.find("agency.txt").is_none(),
        }
    }
}

/// Stream the records of one file of the archive into `f`. A missing table
/// has no records, as for gtfs-lib (which records a `MissingTableError` of
/// priority `MEDIUM` for a required one and loads the feed).
fn table(a: &Archive, name: &'static str, mut f: impl FnMut(&Fields)) -> Result<(), GtfsError> {
    let Some(e) = a.find(name) else {
        return Ok(());
    };
    match for_each_row::<()>(&a.read(e)?, |h, _, r| {
        f(&Fields { h, r });
        Ok(())
    }) {
        Ok(_) => Ok(()),
        Err(ReadError::Csv(error)) => Err(GtfsError::Csv { file: name, error }),
        Err(ReadError::Callback(())) => unreachable!("the callback does not fail"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtfs::zip::tests::make_zip;

    #[test]
    fn times_as_gtfs_lib() {
        assert_eq!(gtfs_time("08:05:09"), Some(8 * 3600 + 5 * 60 + 9));
        assert_eq!(gtfs_time("25:30:00"), Some(25 * 3600 + 1800));
        // Integer.parseInt takes signs; parts out of range are kept
        assert_eq!(gtfs_time("+8:00:00"), Some(8 * 3600));
        assert_eq!(gtfs_time("-1:00:00"), Some(-3600));
        assert_eq!(gtfs_time("8:61:00"), Some(8 * 3600 + 61 * 60));
        // String.split drops trailing empty parts
        assert_eq!(gtfs_time("8:00:00:"), Some(8 * 3600));
        assert_eq!(gtfs_time("8:00:00::"), Some(8 * 3600));
        for bad in [
            "8:00",
            "x:00:00",
            " 8:00:00",
            "8::00:00",
            ":8:00:00",
            "8:00:00:1",
            ":::",
        ] {
            assert_eq!(gtfs_time(bad), None, "{bad}");
        }
        // Java int arithmetic wraps
        assert_eq!(
            gtfs_time("596524:00:00"),
            Some(596_524i32.wrapping_mul(3600))
        );
    }

    #[test]
    fn reads_a_small_feed() {
        let z = make_zip(
            &[
                ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,-30.0,-51.2\nB,-30.01,-51.21\n"),
                ("routes.txt", b"route_id,route_type\nR,3\n"),
                ("trips.txt", b"route_id,service_id,trip_id\nR,S,T1\n"),
                (
                    "stop_times.txt",
                    b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\nT1,\"\",\"\",B,2\n",
                ),
                ("calendar.txt", b"service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nS,1,1,1,1,1,0,0,20190415,20190715\n"),
                ("frequencies.txt", b"trip_id,start_time,end_time,headway_secs\nT1,06:00:00,09:00:00,600\n"),
            ],
            true,
        );
        let f = Feed::from_archive(&Archive::from_bytes(z).unwrap()).unwrap();
        assert_eq!(f.stops.len(), 2);
        assert_eq!(f.stop_times[1].arrival, None);
        assert_eq!(
            f.calendars[0].days,
            [true, true, true, true, true, false, false]
        );
        assert_eq!(f.frequencies[0].headway_secs, 600);
    }

    fn small_feed(stops: &[u8]) -> Feed {
        let z = make_zip(
            &[
                ("stops.txt", stops),
                ("routes.txt", b"route_id,route_type\nR,3\n"),
                ("trips.txt", b"route_id,service_id,trip_id\nR,S,T1\n"),
                (
                    "stop_times.txt",
                    b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\n",
                ),
            ],
            false,
        );
        Feed::from_archive(&Archive::from_bytes(z).unwrap()).unwrap()
    }

    /// A quote inside an unquoted field is a literal character (as in
    /// javacsv), not the start of a quoted field running to the next quote.
    #[test]
    fn stray_quote_in_a_name_keeps_the_following_records() {
        let f = small_feed(
            b"stop_id,stop_name,stop_lat,stop_lon\nA,Rua 7\" Norte,-23.55,-46.63\nB,Praca,-23.56,-46.64\nC,Se,-23.57,-46.65\n",
        );
        let ids: Vec<&str> = f.stops.iter().map(|s| s.stop_id.as_str()).collect();
        assert_eq!(ids, ["A", "B", "C"]);
        assert_eq!(f.stops[0].lat, -23.55);
    }

    /// CR-only line endings end records, as in javacsv.
    #[test]
    fn cr_only_line_endings() {
        let f = small_feed(b"stop_id,stop_lat,stop_lon\rA,-23.55,-46.63\rC,-23.57,-46.65\r");
        assert_eq!(f.stops.len(), 2);
        assert_eq!(f.stops[1].lon, -46.65);
    }

    fn feed_with(tables: &[(&str, &[u8])]) -> Result<Feed, GtfsError> {
        let mut files: Vec<(&str, &[u8])> = vec![
            ("stops.txt", b"stop_id,stop_lat,stop_lon\nA,1,1\n"),
            ("routes.txt", b"route_id,route_type\nR,3\n"),
            ("trips.txt", b"route_id,service_id,trip_id\nR,S,T1\n"),
            (
                "stop_times.txt",
                b"trip_id,arrival_time,departure_time,stop_id,stop_sequence\nT1,08:00:00,08:00:00,A,1\n",
            ),
        ];
        for t in tables {
            files.retain(|f| f.0 != t.0);
            files.push(*t);
        }
        Feed::from_archive(&Archive::from_bytes(make_zip(&files, false)).unwrap())
    }

    /// Values gtfs-lib cannot read take its defaults instead of failing:
    /// a blank `route_type` or `stop_sequence` is 0, a blank calendar date
    /// 0, a time it cannot parse missing; records without a key are not
    /// kept.
    #[test]
    fn unreadable_values_take_gtfs_lib_defaults() {
        let f = feed_with(&[
            ("routes.txt", b"route_id\nR\n\n"),
            (
                "stop_times.txt",
                b"trip_id,arrival_time,departure_time,stop_id\nT1,14:00,x,A\n,08:00:00,08:00:00,A\n",
            ),
            ("calendar.txt", b"service_id,monday,tuesday,start_date,end_date\nS,01,+1,,x\n"),
            ("calendar_dates.txt", b"service_id,date,exception_type\nS,,\nS,20190513Z,+1\n"),
            ("frequencies.txt", b"trip_id,start_time,end_time\nT1,6:00,07:00:00\n"),
            ("stops.txt", b"stop_id,stop_lat,stop_lon,location_type\nA,x,1,\n,1,1,0\n"),
        ])
        .unwrap();
        assert_eq!(f.routes[0].route_type, 0);
        assert_eq!(f.stop_times.len(), 1);
        let st = &f.stop_times[0];
        assert_eq!(
            (st.arrival, st.departure, st.stop_sequence),
            (None, None, 0)
        );
        let c = &f.calendars[0];
        assert_eq!(&c.days[..3], &[true, true, false]);
        assert_eq!((c.start_date, c.end_date), (0, INT_MISSING));
        assert_eq!(f.calendar_dates[0].date, None);
        assert_eq!(f.calendar_dates[0].exception_type, 0);
        assert_eq!(
            (f.calendar_dates[1].date, f.calendar_dates[1].exception_type),
            (Some(20190513), 1)
        );
        let q = &f.frequencies[0];
        assert_eq!(
            (q.start_time, q.end_time, q.headway_secs),
            (INT_MISSING, 7 * 3600, 0)
        );
        assert_eq!(f.stops.len(), 1);
        assert!(f.stops[0].lat.is_nan());
        assert_eq!(f.stops[0].location_type, 0);
    }

    #[test]
    fn the_first_feed_info_record_gives_the_feed_id() {
        let f = feed_with(&[(
            "feed_info.txt",
            b"feed_publisher_name,feed_id\nP,\nQ,second\n",
        )])
        .unwrap();
        assert_eq!(f.feed_id, None);
    }

    /// gtfs-lib reads a missing column as empty values (javacsv returns ""
    /// for an unknown header) and R5 keeps the stops with NaN coordinates,
    /// which then link to no street; the same happens here.
    #[test]
    fn stops_without_coordinate_columns_have_nan_coordinates() {
        let f = feed_with(&[(
            "stops.txt",
            b"stop_id,stop_latitude,stop_lon\nA,-23.55,-46.63\nC,-23.57,-46.65\n",
        )])
        .unwrap();
        assert_eq!(f.stops.len(), 2);
        assert!(f.stops.iter().all(|s| s.lat.is_nan() && !s.lon.is_nan()));
    }
}
